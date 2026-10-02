use crate::{SealedExecutable, above_stdio, invalid, kernel_error};
use std::{
    ffi::{CString, OsStr},
    fs::{File, OpenOptions},
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::Path,
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 64 * 1024;
const OUTPUT_PER_POLL: usize = 32 * 1024;
const SPAWN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitObservation {
    Exited(i32),
    Signaled(i32),
}

#[derive(Debug, Default)]
pub struct OutputObservation {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_eof: bool,
    pub stderr_eof: bool,
    pub stdout_discarded: u64,
    pub stderr_discarded: u64,
}

/// If process creation succeeded, errors carry the real owner for reclamation.
/// Callers must keep it until `cleanup_complete()`; an error is never a claim
/// that no child ran. The fallback Drop is best effort and is not release proof.
pub enum SpawnFailure {
    NotCreated(io::Error),
    Created {
        error: io::Error,
        owner: Box<OwnedProcess>,
    },
}
impl std::fmt::Debug for SpawnFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotCreated(error) => f.debug_tuple("NotCreated").field(error).finish(),
            Self::Created { error, owner } => f
                .debug_struct("Created")
                .field("error", error)
                .field("pid", &owner.pid)
                .finish(),
        }
    }
}
impl std::fmt::Display for SpawnFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotCreated(e) => write!(f, "not created: {e}"),
            Self::Created { error, owner } => write!(
                f,
                "created child {} requires reclamation: {error}",
                owner.pid
            ),
        }
    }
}
impl std::error::Error for SpawnFailure {}

/// A child created atomically with its pidfd. This never opens a pidfd from an
/// arbitrary caller-supplied numeric PID and never kills by PID/process-group.
/// Other code must not reap this child or change SIGCHLD while it is owned.
/// The independent supervisor must retain this object across pending cleanup.
pub struct OwnedProcess {
    pid: u32,
    pidfd: OwnedFd,
    executable: SealedExecutable,
    stdout: File,
    stderr: File,
    output: OutputObservation,
    exit: Option<ExitObservation>,
}

impl OwnedProcess {
    /// Execute the sealed main ELF with an empty environment and closed stdin,
    /// using an already opened private directory as cwd. This is a platform
    /// prerequisite, not authorization to launch a product/plugin guest.
    /// Kernel denial is returned without an alternative launch path.
    pub fn spawn(
        executable: SealedExecutable,
        cwd: &Path,
        args: &[&OsStr],
    ) -> Result<Self, SpawnFailure> {
        let prepared = prepare(&executable, cwd, args).map_err(SpawnFailure::NotCreated)?;
        let stdout = pipe().map_err(SpawnFailure::NotCreated)?;
        let stderr = pipe().map_err(SpawnFailure::NotCreated)?;
        let input = pipe().map_err(SpawnFailure::NotCreated)?;
        let status = pipe().map_err(SpawnFailure::NotCreated)?;
        let mut pidfd = -1i32;
        // Raw clone with CLONE_PIDFD and no VM/thread/stack-sharing flags has
        // fork semantics. On x86_64/aarch64 parent_tid receives the owned pidfd.
        // All later optional arguments are zero in both architecture ABIs.
        // SAFETY: pidfd points to writable parent storage, no custom stack or
        // shared VM is requested; the child branch calls only prepared C/syscall
        // operations and ends in execveat or _exit, never Rust unwinding/Drop.
        let pid = unsafe {
            libc::syscall(
                libc::SYS_clone,
                (libc::CLONE_PIDFD | libc::SIGCHLD) as libc::c_ulong,
                std::ptr::null_mut::<libc::c_void>(),
                &mut pidfd as *mut i32,
                0usize,
                0usize,
            )
        };
        if pid < 0 {
            return Err(SpawnFailure::NotCreated(kernel_error("clone(CLONE_PIDFD)")));
        }
        if pid == 0 {
            // SAFETY: prepared owns argv and cwd, and pipes stay alive in this
            // copied address space. The helper cannot return or unwind.
            unsafe {
                child_exec(
                    &prepared,
                    executable.as_fd().as_raw_fd(),
                    input.0.as_raw_fd(),
                    stdout.1.as_raw_fd(),
                    stderr.1.as_raw_fd(),
                    status.1.as_raw_fd(),
                )
            }
        }
        // A successful CLONE_PIDFD always returns a fresh descriptor in pidfd.
        // SAFETY: descriptor belongs to this parent and has not been wrapped.
        let pidfd = unsafe { OwnedFd::from_raw_fd(pidfd) };
        drop(stdout.1);
        drop(stderr.1);
        drop(input);
        drop(status.1);
        let mut owner = Self {
            pid: pid as u32,
            pidfd,
            executable,
            stdout: File::from(stdout.0),
            stderr: File::from(stderr.0),
            output: OutputObservation::default(),
            exit: None,
        };
        let initialized = (|| {
            require_cloexec(owner.pidfd.as_raw_fd())?;
            nonblocking(owner.stdout.as_raw_fd())?;
            nonblocking(owner.stderr.as_raw_fd())?;
            nonblocking(status.0.as_raw_fd())?;
            read_exec_status(status.0.as_raw_fd(), SPAWN_TIMEOUT)
        })();
        if let Err(error) = initialized {
            // Keep a real owner even when exec failed or timed out. The caller
            // can observe actual pidfd reaping + pipe EOF instead of assuming
            // failure means the process did not exist or has already stopped.
            let _ = owner.terminate();
            return Err(SpawnFailure::Created {
                error,
                owner: Box::new(owner),
            });
        }
        Ok(owner)
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn artifact_digest(&self) -> [u8; 32] {
        self.executable.artifact_digest()
    }
    pub fn exit_observation(&self) -> Option<ExitObservation> {
        self.exit
    }
    pub fn output(&self) -> &OutputObservation {
        &self.output
    }
    pub fn cleanup_complete(&self) -> bool {
        self.exit.is_some() && self.output.stdout_eof && self.output.stderr_eof
    }

    /// Signal the held process object. No numeric-PID retry or PID disappearance
    /// is used as proof. ESRCH only means no signal was delivered; waitid still
    /// has to positively reap the exact child before cleanup can complete.
    pub fn terminate(&mut self) -> io::Result<()> {
        if self.exit.is_some() {
            return Ok(());
        }
        // SAFETY: pidfd stays owned, siginfo is null and flags are zero.
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.pidfd.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                0u32,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
        }
        Ok(())
    }

    /// One bounded nonblocking observation. Child reaping is performed through
    /// P_PIDFD, with the returned identity checked against clone's actual child.
    /// An external reaper/ECHILD, invalid descriptor or pipe error is not EOF.
    pub fn observe(&mut self) -> io::Result<bool> {
        drain(
            &mut self.stdout,
            &mut self.output.stdout,
            &mut self.output.stdout_eof,
            &mut self.output.stdout_discarded,
        )?;
        drain(
            &mut self.stderr,
            &mut self.output.stderr,
            &mut self.output.stderr_eof,
            &mut self.output.stderr_discarded,
        )?;
        if self.exit.is_none() {
            // SAFETY: siginfo is initialized writable storage, pidfd is held,
            // and WNOHANG ensures this observation does not block.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PIDFD,
                    self.pidfd.as_raw_fd() as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG,
                )
            };
            if result < 0 {
                return Err(kernel_error("waitid(P_PIDFD)"));
            }
            // SAFETY: waitid populates the SIGCHLD union described by si_code.
            let pid = unsafe { info.si_pid() };
            if pid != 0 {
                if pid != self.pid as i32 {
                    return Err(invalid("pidfd reap returned another child identity"));
                }
                let status = unsafe { info.si_status() };
                self.exit = Some(match info.si_code {
                    libc::CLD_EXITED => ExitObservation::Exited(status),
                    libc::CLD_KILLED | libc::CLD_DUMPED => ExitObservation::Signaled(status),
                    _ => return Err(invalid("pidfd reap returned an unexpected event")),
                });
            }
        }
        Ok(self.cleanup_complete())
    }

    /// false means cleanup is still pending and this owner must remain held.
    /// A direct child exit is insufficient if a descendant retained either pipe.
    pub fn wait_bounded(&mut self, timeout: Duration) -> io::Result<bool> {
        let until = Instant::now()
            .checked_add(timeout)
            .ok_or(invalid("wait duration overflow"))?;
        loop {
            if self.observe()? {
                return Ok(true);
            }
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(false);
            }
            thread::sleep(remaining.min(Duration::from_millis(2)));
        }
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if !self.cleanup_complete() {
            let _ = self.terminate();
            let _ = self.wait_bounded(Duration::from_millis(100));
        }
        // Closing a pidfd is not kill-on-close and never proves tree cleanup.
        // A stalled kernel task/external retained pipe may outlive this backstop;
        // product ownership must not depend on Drop as a release observation.
    }
}

struct Prepared {
    cwd: File,
    _strings: Vec<CString>,
    argv: Vec<*const libc::c_char>,
    parent: libc::pid_t,
}
fn prepare(executable: &SealedExecutable, cwd: &Path, args: &[&OsStr]) -> io::Result<Prepared> {
    executable.verify()?;
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
        return Err(invalid("unsupported raw-clone architecture"));
    }
    if !cwd.is_absolute() || args.len() > 8 || args.iter().any(|arg| arg.as_bytes().len() > 256) {
        return Err(invalid("invalid private cwd/argument limits"));
    }
    // This foundation is intended for a dedicated supervisor. Refuse known
    // auto-reap/handler policies; do not reset process-wide SIGCHLD settings.
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    if unsafe { libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut action) } < 0 {
        return Err(kernel_error("inspect SIGCHLD"));
    }
    if action.sa_sigaction != libc::SIG_DFL || action.sa_flags & libc::SA_NOCLDWAIT != 0 {
        return Err(invalid(
            "requires default SIGCHLD and exclusive child reaping",
        ));
    }
    let cwd = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(cwd)?;
    let cwd = above_stdio(cwd)?;
    let metadata = cwd.metadata()?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o7777 != 0o700
    {
        return Err(invalid("requires a held same-UID private 0700 cwd"));
    }
    let mut strings = vec![CString::new("morrow-linux-guest").unwrap()];
    for arg in args {
        strings.push(CString::new(arg.as_bytes()).map_err(|_| invalid("argument NUL"))?);
    }
    let mut argv: Vec<_> = strings.iter().map(|s| s.as_ptr()).collect();
    argv.push(std::ptr::null());
    Ok(Prepared {
        cwd,
        _strings: strings,
        argv,
        parent: unsafe { libc::getpid() },
    })
}

/// All Rust allocation, format, filesystem-path and lock operations are done
/// before clone. This post-fork path calls only C/kernel functions with stable
/// prepared storage and never returns through Rust destructors.
unsafe fn child_exec(
    prepared: &Prepared,
    executable: RawFd,
    stdin: RawFd,
    stdout: RawFd,
    stderr: RawFd,
    status: RawFd,
) -> ! {
    unsafe {
        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) < 0
            || libc::getppid() != prepared.parent
            || libc::fchdir(prepared.cwd.as_raw_fd()) < 0
        {
            child_error(status);
        }
        // All acquired descriptors are >=3. stdio gets exactly these three
        // anonymous pipes, and all other inherited fds are CLOEXEC atomically.
        if libc::dup2(stdin, 0) < 0 || libc::dup2(stdout, 1) < 0 || libc::dup2(stderr, 2) < 0 {
            child_error(status);
        }
        if libc::syscall(
            libc::SYS_close_range,
            3u32,
            u32::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        ) < 0
        {
            child_error(status);
        }
        let environment: [*const libc::c_char; 1] = [std::ptr::null()];
        libc::syscall(
            libc::SYS_execveat,
            executable,
            c"".as_ptr(),
            prepared.argv.as_ptr(),
            environment.as_ptr(),
            libc::AT_EMPTY_PATH,
        );
        child_error(status);
    }
}
unsafe fn child_error(status: RawFd) -> ! {
    unsafe {
        let error = *libc::__errno_location();
        // A fixed four-byte write is below PIPE_BUF and cannot allocate. The
        // parent interprets any nonempty status payload as failed execution.
        let _ = libc::write(
            status,
            (&error as *const i32).cast(),
            std::mem::size_of::<i32>(),
        );
        libc::_exit(127)
    }
}

fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    // SAFETY: fds is writable two-element storage; pipe2 creates fresh fds.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
        return Err(kernel_error("pipe2"));
    }
    let (mut read, mut write) =
        unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    // Keep private launch descriptors away from the explicit stdio slots even
    // when a caller's standard descriptor was already closed.
    for fd in [&mut read, &mut write] {
        if fd.as_raw_fd() < 3 {
            let copied = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
            if copied < 0 {
                return Err(kernel_error("duplicate pipe away from stdio"));
            }
            *fd = unsafe { OwnedFd::from_raw_fd(copied) };
        }
    }
    Ok((read, write))
}
fn require_cloexec(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(kernel_error("F_GETFD pidfd"));
    }
    if flags & libc::FD_CLOEXEC == 0 {
        return Err(invalid("pidfd is inheritable"));
    }
    Ok(())
}
fn nonblocking(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(kernel_error("nonblocking observation pipe"));
    }
    Ok(())
}
fn read_exec_status(fd: RawFd, timeout: Duration) -> io::Result<()> {
    let until = Instant::now() + timeout;
    loop {
        let mut error = 0i32;
        let count = unsafe {
            libc::read(
                fd,
                (&mut error as *mut i32).cast(),
                std::mem::size_of::<i32>(),
            )
        };
        if count == 0 {
            return Ok(());
        }
        if count > 0 {
            if count != std::mem::size_of::<i32>() as isize {
                return Err(invalid("partial exec failure observation"));
            }
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "child exec preparation/execveat: {}",
                    io::Error::from_raw_os_error(error)
                ),
            ));
        }
        let error = io::Error::last_os_error();
        if !matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        ) {
            return Err(error);
        }
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        thread::sleep(remaining.min(Duration::from_millis(2)));
    }
}
fn drain(
    file: &mut File,
    captured: &mut Vec<u8>,
    eof: &mut bool,
    discarded: &mut u64,
) -> io::Result<()> {
    if *eof {
        return Ok(());
    }
    let mut buffer = [0u8; 4096];
    let mut total = 0;
    let mut attempts = 0;
    while total < OUTPUT_PER_POLL && attempts < 128 {
        attempts += 1;
        match file.read(&mut buffer) {
            Ok(0) => {
                *eof = true;
                break;
            }
            Ok(count) => {
                total += count;
                let retained = count.min(OUTPUT_LIMIT.saturating_sub(captured.len()));
                captured.extend_from_slice(&buffer[..retained]);
                *discarded = discarded.saturating_add((count - retained) as u64);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
