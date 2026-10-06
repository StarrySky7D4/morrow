//! Linux fixed-artifact adapter. This verifies the loaded ELF image and anchored
//! cwd, clears inherited environment, delivers bounded stdin and enforces a
//! runtime/output budget. It does not claim OS sandbox qualification.
use morrow_agent_session_exec_v1_r2::{
    Error, ExecutionFacts, Intent, MAX_OUTPUT_BYTES, Result, hash,
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            fs::OpenOptionsExt,
            process::{CommandExt, ExitStatusExt},
        },
    },
    process::{Child, Command, Stdio},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};

const MAX_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;
pub struct LinuxFixedExecutor {
    program: String,
    cwd_path: String,
    domain: String,
    image: File,
    cwd: File,
    artifact: [u8; 32],
    output_limit: usize,
}
pub struct ExecutionCapture {
    pub facts: ExecutionFacts,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}
impl LinuxFixedExecutor {
    /// Host review registers the exact artifact and already opened working directory.
    pub fn register(
        program: &str,
        cwd: &str,
        domain: &str,
        expected: [u8; 32],
        output_limit: usize,
    ) -> Result<Self> {
        if output_limit == 0 || output_limit > MAX_OUTPUT_BYTES as usize || expected == [0; 32] {
            return Err(Error::Limit);
        }
        let source = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(program)
            .map_err(|_| Error::Denied)?;
        let metadata = source.metadata().map_err(|_| Error::Storage)?;
        if !metadata.is_file() || metadata.len() > MAX_ARTIFACT_BYTES as u64 {
            return Err(Error::Denied);
        }
        let mut bytes = Vec::new();
        source
            .take(MAX_ARTIFACT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Storage)?;
        if bytes.len() > MAX_ARTIFACT_BYTES
            || bytes.get(..4) != Some(b"\x7fELF")
            || hash(&bytes) != expected
        {
            return Err(Error::Denied);
        }
        let cwd_file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(cwd)
            .map_err(|_| Error::Denied)?;
        // SAFETY: constant NUL-terminated name; returned fd is checked and uniquely owned.
        let fd = unsafe {
            libc::memfd_create(
                c"morrow-fixed-elf".as_ptr(),
                libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
            )
        };
        if fd < 0 {
            return Err(Error::Storage);
        }
        // SAFETY: fd is a fresh valid descriptor transferred exactly once to File.
        let mut image = unsafe { File::from_raw_fd(fd) };
        image.write_all(&bytes).map_err(|_| Error::Storage)?;
        // SAFETY: fcntl operates on the live owned descriptor, with constant seal flags.
        if unsafe {
            libc::fcntl(
                fd,
                libc::F_ADD_SEALS,
                libc::F_SEAL_WRITE | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_SEAL,
            )
        } < 0
        {
            return Err(Error::Storage);
        }
        Ok(Self {
            program: program.into(),
            cwd_path: cwd.into(),
            domain: domain.into(),
            image,
            cwd: cwd_file,
            artifact: expected,
            output_limit,
        })
    }
    pub fn execute(&self, intent: &Intent) -> Result<ExecutionCapture> {
        intent.validate()?;
        if intent.program != self.program
            || intent.cwd != self.cwd_path
            || intent.execution_domain != self.domain
            || intent.artifact_sha256 != self.artifact
        {
            return Err(Error::Denied);
        }
        let cleanup = cleanup_manager()?;
        let permit = cleanup.reserve()?;
        let start = Instant::now();
        let cwd_fd = self.cwd.as_raw_fd();
        let mut command = Command::new(format!("/proc/self/fd/{}", self.image.as_raw_fd()));
        command
            .args(&intent.argv)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for env in &intent.env {
            command.env(&env.name, &env.value);
        }
        // SAFETY: the post-fork closure only uses async-signal-safe fchdir and
        // setpgid on an owned descriptor; it does not allocate or lock.
        unsafe {
            command.pre_exec(move || {
                if libc::fchdir(cwd_fd) != 0 || libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().map_err(|_| Error::Storage)?;
        let mut guard = ProcessGuard {
            child: Some(child),
            permit: Some(permit),
            reaped: false,
        };
        let mut stdin = Some(guard.child_mut().stdin.take().ok_or(Error::Storage)?);
        let mut stdout = guard.child_mut().stdout.take().ok_or(Error::Storage)?;
        let mut stderr = guard.child_mut().stderr.take().ok_or(Error::Storage)?;
        nonblocking(stdin.as_ref().ok_or(Error::Storage)?.as_raw_fd())?;
        nonblocking(stdout.as_raw_fd())?;
        nonblocking(stderr.as_raw_fd())?;
        let mut input_offset = 0;
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut out_closed = false;
        let mut err_closed = false;
        let deadline = Duration::from_millis(intent.max_runtime_ms);
        let mut timed_out = false;
        let status = loop {
            // Apply the elapsed budget before accepting a late exit/EOF. Spawn
            // itself is synchronous, but a late return cannot hide its cost.
            let elapsed = start.elapsed();
            if elapsed >= deadline + Duration::from_millis(500) {
                return Err(Error::CommitUnknown);
            }
            if elapsed >= deadline && !timed_out {
                guard.kill_group();
                timed_out = true;
            }
            if let Some(writer) = stdin.as_mut() {
                match writer.write(&intent.input[input_offset..]) {
                    Ok(n) => input_offset += n,
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                        stdin = None;
                    }
                    Err(_) => return Err(Error::Storage),
                }
                // Drop the sole pipe handle after delivering all fixed input.
                if input_offset == intent.input.len() {
                    stdin = None;
                }
            }
            read_bounded(&mut stdout, &mut out, &mut out_closed, self.output_limit)?;
            read_bounded(&mut stderr, &mut err, &mut err_closed, self.output_limit)?;
            // Keep the leader unreaped while any output handle remains open.
            // Its PID therefore cannot be reused before group cleanup. Once
            // both EOFs arrive, reaping ends the loop before any later signal.
            if out_closed
                && err_closed
                && let Some(status) = guard.child_mut().try_wait().map_err(|_| Error::Storage)?
            {
                break status;
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        guard.reaped = true;
        if input_offset != intent.input.len() {
            return Err(Error::CommitUnknown);
        }
        let exit = status
            .code()
            .or_else(|| status.signal().map(|signal| -signal));
        let facts = ExecutionFacts {
            exit_code: exit,
            output_closed: out_closed && err_closed,
            stdout_sha256: hash(&out),
            stderr_sha256: hash(&err),
            stdout_bytes: out.len() as u64,
            stderr_bytes: err.len() as u64,
        };
        facts.validate()?;
        Ok(ExecutionCapture {
            facts,
            stdout: out,
            stderr: err,
            timed_out,
        })
    }
    pub fn execute_facts(&self, intent: &Intent) -> Result<ExecutionFacts> {
        self.execute(intent).map(|capture| capture.facts)
    }
}
fn nonblocking(fd: i32) -> Result<()> {
    // SAFETY: descriptor belongs to the current live child pipe.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(Error::Storage);
    }
    Ok(())
}
fn read_bounded(
    reader: &mut impl Read,
    bytes: &mut Vec<u8>,
    closed: &mut bool,
    limit: usize,
) -> Result<()> {
    if *closed {
        return Ok(());
    }
    let mut buffer = [0u8; 8192];
    // Bound each scheduling turn even if a child continuously writes.
    for _ in 0..8 {
        let room = limit.saturating_sub(bytes.len());
        let size = buffer.len().min(room.saturating_add(1));
        match reader.read(&mut buffer[..size]) {
            Ok(0) => {
                *closed = true;
                return Ok(());
            }
            Ok(n) => {
                if n > room {
                    return Err(Error::Limit);
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(Error::Storage),
        }
    }
    Ok(())
}
struct ProcessGuard {
    child: Option<Child>,
    permit: Option<ProcessPermit>,
    reaped: bool,
}
impl ProcessGuard {
    fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("owned child")
    }
    fn kill_group(&mut self) {
        // SAFETY: the child was placed into its own group before exec; positive
        // checked child id is negated only for that group, never the host group.
        if let Ok(pid) = i32::try_from(self.child_mut().id()) {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
        let _ = self.child_mut().kill();
    }
}
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        if !self.reaped {
            self.kill_group();
            if let (Some(child), Some(permit)) = (self.child.take(), self.permit.take()) {
                // Cleanup never waits under SDK authority or callback locks.
                // The process-wide eight-slot bound includes unreaped jobs.
                let job = (child, permit);
                match cleanup_manager() {
                    Ok(manager) => match manager.sender.try_send(job) {
                        Ok(()) => {}
                        Err(
                            mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job),
                        ) => {
                            // A failed worker must not silently return its slot.
                            std::mem::forget(job);
                        }
                    },
                    Err(_) => std::mem::forget(job),
                }
            }
        }
    }
}
struct ProcessPermit(Arc<AtomicUsize>);
impl Drop for ProcessPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
struct CleanupManager {
    sender: SyncSender<(Child, ProcessPermit)>,
    slots: Arc<AtomicUsize>,
}
impl CleanupManager {
    fn reserve(&self) -> Result<ProcessPermit> {
        self.slots
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < 8).then_some(n + 1)
            })
            .map_err(|_| Error::Limit)?;
        Ok(ProcessPermit(self.slots.clone()))
    }
}
fn cleanup_manager() -> Result<&'static CleanupManager> {
    static CLEANUP: OnceLock<std::result::Result<CleanupManager, ()>> = OnceLock::new();
    CLEANUP
        .get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel::<(Child, ProcessPermit)>(8);
            std::thread::Builder::new()
                .name("morrow-fixed-reaper".into())
                .spawn(move || {
                    let mut jobs: Vec<(Child, ProcessPermit)> = Vec::new();
                    loop {
                        if let Ok(job) = receiver.recv_timeout(Duration::from_millis(20)) {
                            jobs.push(job);
                        }
                        let mut index = 0;
                        while index < jobs.len() {
                            match jobs[index].0.try_wait() {
                                Ok(None) => index += 1,
                                Ok(Some(_)) => {
                                    jobs.swap_remove(index);
                                }
                                // An unconfirmed reap never returns an execution slot.
                                // Keep retrying within the same process-wide bound.
                                Err(_) => index += 1,
                            }
                        }
                    }
                })
                .map_err(|_| ())?;
            Ok(CleanupManager {
                sender,
                slots: Arc::new(AtomicUsize::new(0)),
            })
        })
        .as_ref()
        .map_err(|_| Error::Storage)
}
