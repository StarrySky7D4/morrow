use crate::{SealedExecutable, above_stdio, invalid, kernel_error};
use std::{
    ffi::{CString, OsStr},
    fs::{File, OpenOptions},
    io::{self, Read},
    os::{
        fd::{AsFd, AsRawFd, FromRawFd, OwnedFd, RawFd},
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

/// Private coordination present only in unit-test builds. Ordinary launches
/// never hold exec or retain output writers, and have no pending callback.
#[cfg(test)]
struct ExecStatusTestHandshake<'a> {
    child_hold: (RawFd, RawFd),
    on_pending: &'a mut dyn FnMut(),
    retained_outputs: &'a mut Vec<OwnedFd>,
}

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
        Self::spawn_inner(
            executable,
            cwd,
            args,
            None,
            None,
            None,
            #[cfg(test)]
            None,
        )
    }

    pub(crate) fn spawn_with_inherited(
        executable: SealedExecutable,
        cwd: &Path,
        args: &[&OsStr],
        inherited: crate::controller_transport::InheritedLaunch,
    ) -> Result<Self, SpawnFailure> {
        Self::spawn_inner(
            executable,
            cwd,
            args,
            Some(inherited),
            None,
            None,
            #[cfg(test)]
            None,
        )
    }

    /// Synthetic controller-fenced launch. All blocking artifact preparation is
    /// completed before the final original-object/lease check adjacent to clone.
    /// An unobserved controller exit can still race clone; any created child is
    /// retained and requires exact reclamation, never classified NotCreated.
    pub fn spawn_for_controller_fixture(
        executable: SealedExecutable,
        cwd: &Path,
        args: &[&OsStr],
        watch: &mut crate::ControllerWatch,
    ) -> Result<Self, SpawnFailure> {
        Self::spawn_inner(
            executable,
            cwd,
            args,
            None,
            Some(watch),
            None,
            #[cfg(test)]
            None,
        )
    }

    /// Prepare every digest/argv/cwd operation before activating the live lease.
    /// The returned object retains the same sealed executable and owned cwd.
    pub fn prepare_controller_fixture(
        executable: SealedExecutable,
        cwd: &Path,
        args: &[&OsStr],
    ) -> io::Result<PreparedControllerFixtureChild> {
        let prepared = prepare(&executable, cwd, args)?;
        Ok(PreparedControllerFixtureChild {
            executable,
            prepared,
        })
    }

    fn spawn_inner(
        executable: SealedExecutable,
        cwd: &Path,
        args: &[&OsStr],
        inherited: Option<crate::controller_transport::InheritedLaunch>,
        mut controller_guard: Option<&mut crate::ControllerWatch>,
        already_prepared: Option<Prepared>,
        #[cfg(test)] mut test_handshake: Option<ExecStatusTestHandshake<'_>>,
    ) -> Result<Self, SpawnFailure> {
        let prepared = match already_prepared {
            Some(prepared) => {
                executable
                    .verify_object()
                    .map_err(SpawnFailure::NotCreated)?;
                let metadata = prepared.cwd.metadata().map_err(SpawnFailure::NotCreated)?;
                if prepared.parent != unsafe { libc::getpid() }
                    || !metadata.is_dir()
                    || metadata.uid() != unsafe { libc::geteuid() }
                    || metadata.mode() & 0o7777 != 0o700
                {
                    return Err(SpawnFailure::NotCreated(invalid(
                        "prepared launch process/cwd changed",
                    )));
                }
                prepared
            }
            None => prepare(&executable, cwd, args).map_err(SpawnFailure::NotCreated)?,
        };
        // Stage executable and exec-status away from fixed bootstrap slots3..6.
        // Every optional role source is already staged>=16 by typed construction.
        let staged_executable = if inherited.is_some() {
            Some(
                crate::controller_transport::duplicate_high(executable.as_fd().as_raw_fd())
                    .map_err(SpawnFailure::NotCreated)?,
            )
        } else {
            None
        };
        let stdout = pipe().map_err(SpawnFailure::NotCreated)?;
        let stderr = pipe().map_err(SpawnFailure::NotCreated)?;
        let input = pipe().map_err(SpawnFailure::NotCreated)?;
        let status = pipe().map_err(SpawnFailure::NotCreated)?;
        let staged_status = if inherited.is_some() {
            Some(
                crate::controller_transport::duplicate_high(status.1.as_raw_fd())
                    .map_err(SpawnFailure::NotCreated)?,
            )
        } else {
            None
        };
        let executable_fd = staged_executable
            .as_ref()
            .map_or(executable.as_fd().as_raw_fd(), AsRawFd::as_raw_fd);
        let status_fd = staged_status
            .as_ref()
            .map_or(status.1.as_raw_fd(), AsRawFd::as_raw_fd);
        #[cfg(test)]
        let test_child_hold = if let Some(test) = test_handshake.as_mut() {
            // Actual duplicate pipe writers let the test observe each real EOF
            // separately after reaping. Preparation failure creates no child.
            for fd in [stdout.1.as_raw_fd(), stderr.1.as_raw_fd()] {
                test.retained_outputs.push(
                    crate::controller_transport::duplicate_high(fd)
                        .map_err(SpawnFailure::NotCreated)?,
                );
            }
            Some(test.child_hold)
        } else {
            None
        };
        if let Some(watch) = controller_guard.as_mut() {
            if !watch.poll() || !watch.admission_open() {
                return Err(SpawnFailure::NotCreated(invalid(
                    "original controller admission closed before clone",
                )));
            }
        }
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
                    executable_fd,
                    input.0.as_raw_fd(),
                    stdout.1.as_raw_fd(),
                    stderr.1.as_raw_fd(),
                    status_fd,
                    inherited.as_ref(),
                    #[cfg(test)]
                    test_child_hold,
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
        drop(staged_status);
        drop(staged_executable);
        drop(inherited);
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
            read_exec_status(
                status.0.as_raw_fd(),
                SPAWN_TIMEOUT,
                controller_guard.as_deref_mut(),
                #[cfg(test)]
                test_handshake.as_mut(),
            )
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

    // Exact clone object/readers exported only to the private, typed guardian
    // fixture registration path. This does not construct an owner from a PID/fd.
    pub(crate) fn guardian_fixture_handles(&self) -> io::Result<(OwnedFd, OwnedFd, OwnedFd)> {
        if self.exit.is_some() {
            return Err(invalid("reaped fixture child cannot be registered"));
        }
        let duplicate = crate::controller_transport::duplicate_high;
        Ok((
            duplicate(self.pidfd.as_raw_fd())?,
            duplicate(self.stdout.as_raw_fd())?,
            duplicate(self.stderr.as_raw_fd())?,
        ))
    }

    pub fn original_controller(&self) -> io::Result<crate::OriginalController> {
        if self.exit.is_some() {
            return Err(invalid("reaped child cannot create controller watch"));
        }
        crate::OriginalController::from_owned_child(self.pidfd.as_fd(), self.pid)
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

/// Typed prelaunch object, not a generic verified flag or pathname cache. The
/// immutable main ELF is held from full digest admission to the actual clone.
pub struct PreparedControllerFixtureChild {
    executable: SealedExecutable,
    prepared: Prepared,
}
impl PreparedControllerFixtureChild {
    pub fn spawn(self, watch: &mut crate::ControllerWatch) -> Result<OwnedProcess, SpawnFailure> {
        OwnedProcess::spawn_inner(
            self.executable,
            Path::new("/"),
            &[],
            None,
            Some(watch),
            Some(self.prepared),
            #[cfg(test)]
            None,
        )
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
    inherited: Option<&crate::controller_transport::InheritedLaunch>,
    #[cfg(test)] test_child_hold: Option<(RawFd, RawFd)>,
) -> ! {
    unsafe {
        // Only the private typed, registered guardian-supervisor fixture can
        // select its special lifetime. Ordinary C/D and all existing launches
        // retain SIGKILL; there is no public boolean or raw-PID lifetime factory.
        let parent_death = inherited.map_or(libc::SIGKILL, |launch| launch.parent_death_signal());
        if libc::prctl(libc::PR_SET_PDEATHSIG, parent_death) < 0
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
        if let Some(inherited) = inherited {
            for (target, source) in &inherited.descriptors {
                if libc::dup2(source.as_raw_fd(), *target) < 0
                    || libc::fcntl(*target, libc::F_SETFD, 0) < 0
                {
                    child_error(status);
                }
            }
        }
        #[cfg(test)]
        if let Some((ready, release)) = test_child_hold {
            // Only bounded libc operations are permitted in this copied
            // post-clone address space; no Rust allocation, lock or Drop.
            hold_before_exec_for_test(ready, release);
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
fn read_exec_status(
    fd: RawFd,
    timeout: Duration,
    mut watch: Option<&mut crate::ControllerWatch>,
    #[cfg(test)] mut test_handshake: Option<&mut ExecStatusTestHandshake<'_>>,
) -> io::Result<()> {
    let until = Instant::now() + timeout;
    loop {
        if let Some(watch) = watch.as_mut() {
            if !watch.poll() || !watch.admission_open() {
                return Err(invalid(
                    "controller lost during exec handshake; real child retained",
                ));
            }
        }
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
        #[cfg(test)]
        if error.kind() == io::ErrorKind::WouldBlock {
            if let Some(test) = test_handshake.take() {
                // The unchanged read has really observed an open, pending
                // exec-status pipe before test coordination may cause loss.
                (test.on_pending)();
            }
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

/// The unit-test child announces its real PID, then holds every inherited
/// descriptor (including exec-status) until released or a bounded timeout.
#[cfg(test)]
unsafe fn hold_before_exec_for_test(ready: RawFd, release: RawFd) {
    unsafe {
        let pid = libc::getpid();
        if libc::write(ready, (&pid as *const libc::pid_t).cast(), 4) != 4 {
            libc::_exit(126);
        }
        let mut input = libc::pollfd {
            fd: release,
            events: libc::POLLIN,
            revents: 0,
        };
        // No sleep-based ordering and no unbounded blocking after clone.
        if libc::poll(&mut input, 1, 5000) != 1 {
            libc::_exit(124);
        }
        let mut byte = 0u8;
        if libc::read(release, (&mut byte as *mut u8).cast(), 1) != 1 || byte != 1 {
            libc::_exit(125);
        }
    }
}

#[cfg(test)]
mod exec_status_tests {
    use super::*;
    use crate::{ControllerLoss, ControllerWatch, OriginalController, digest};
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> io::Result<Self> {
            let root = std::env::temp_dir().join(format!(
                "morrow-exec-status-unit-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root)?;
            let directory = Self(root);
            fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o700))?;
            Ok(directory)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn readable(fd: RawFd) -> io::Result<()> {
        let mut item = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // One kernel-bounded readiness operation. An interruption is a retained
        // test failure rather than a fresh deadline or a guessed observation.
        let result = unsafe { libc::poll(&mut item, 1, 2000) };
        if result != 1 || item.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
            return Err(invalid("test object readiness not observed"));
        }
        Ok(())
    }

    fn ready_pid(fd: RawFd) -> io::Result<u32> {
        readable(fd)?;
        let mut pid = 0i32;
        let count = unsafe { libc::read(fd, (&mut pid as *mut i32).cast(), 4) };
        if count != 4 || pid <= 0 {
            return Err(invalid("test child readiness identity missing"));
        }
        Ok(pid as u32)
    }

    /// Real CLONE_PIDFD controller with an owned release pipe and exact reaper.
    /// Only its initial watch lease is synthetic; no process/handle is mocked.
    struct TestController {
        pid: u32,
        pidfd: OwnedFd,
        _release: OwnedFd,
        reaped: bool,
    }
    impl TestController {
        fn start() -> io::Result<Self> {
            if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
                return Err(invalid("unsupported test raw-clone architecture"));
            }
            let ready = pipe()?;
            let release = pipe()?;
            let mut pidfd = -1i32;
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
                return Err(kernel_error("test controller clone(CLONE_PIDFD)"));
            }
            if pid == 0 {
                // No Rust unwinding or destructor may run after raw clone.
                unsafe {
                    hold_before_exec_for_test(ready.1.as_raw_fd(), release.0.as_raw_fd());
                    libc::_exit(0);
                }
            }
            let controller = Self {
                pid: pid as u32,
                pidfd: unsafe { OwnedFd::from_raw_fd(pidfd) },
                _release: release.1,
                reaped: false,
            };
            drop(ready.1);
            drop(release.0);
            if ready_pid(ready.0.as_raw_fd())? != controller.pid {
                return Err(invalid("test controller clone/readiness mismatch"));
            }
            Ok(controller)
        }

        fn original(&self) -> io::Result<OriginalController> {
            OriginalController::from_owned_child(self.pidfd.as_fd(), self.pid)
        }

        fn stop_and_reap(&mut self) -> io::Result<()> {
            if self.reaped {
                return Ok(());
            }
            let sent = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    self.pidfd.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0u32,
                )
            };
            if sent < 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
                return Err(kernel_error("test controller pidfd signal"));
            }
            readable(self.pidfd.as_raw_fd())?;
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PIDFD,
                    self.pidfd.as_raw_fd() as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG,
                )
            };
            if result != 0 || unsafe { info.si_pid() } != self.pid as i32 {
                return Err(invalid("test controller exact reap missing"));
            }
            self.reaped = true;
            if info.si_code != libc::CLD_KILLED || unsafe { info.si_status() } != libc::SIGKILL {
                return Err(invalid("test controller exited before coordinated loss"));
            }
            Ok(())
        }
    }
    impl Drop for TestController {
        fn drop(&mut self) {
            let _ = self.stop_and_reap();
        }
    }

    #[test]
    fn original_controller_loss_during_pending_exec_retains_real_child_until_both_eofs() {
        let directory = TestDirectory::new().unwrap();
        let artifact = directory.0.join("never-executed-test-elf");
        let bytes = fs::read(std::env::current_exe().unwrap()).unwrap();
        fs::write(&artifact, &bytes).unwrap();
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        let executable = SealedExecutable::read(&artifact, digest(&bytes)).unwrap();
        let mut controller = TestController::start().unwrap();
        let original_pid = controller.pid;
        let mut watch =
            ControllerWatch::synthetic_active_for_exec_status_test(controller.original().unwrap())
                .unwrap();
        assert!(watch.poll() && watch.admission_open());
        let ready = pipe().unwrap();
        let release = pipe().unwrap();
        let mut observed_child = None;
        let mut retained_outputs = Vec::new();
        let mut on_pending = || {
            // This runs only after the production reader saw WouldBlock.
            observed_child = Some(ready_pid(ready.0.as_raw_fd()).unwrap());
            controller.stop_and_reap().unwrap();
        };
        let failure = OwnedProcess::spawn_inner(
            executable,
            &directory.0,
            &[],
            None,
            Some(&mut watch),
            None,
            Some(ExecStatusTestHandshake {
                child_hold: (ready.1.as_raw_fd(), release.0.as_raw_fd()),
                on_pending: &mut on_pending,
                retained_outputs: &mut retained_outputs,
            }),
        )
        .err()
        .expect("coordinated original loss must refuse the held exec handshake");
        let SpawnFailure::Created { error, mut owner } = failure else {
            panic!("actual cloned child's owner was lost: {failure}");
        };
        assert!(
            error
                .to_string()
                .contains("controller lost during exec handshake")
        );
        assert_eq!(observed_child, Some(owner.pid()));
        assert_ne!(owner.pid(), original_pid);
        assert!(controller.reaped);
        let first_loss = watch.loss().unwrap();
        assert_eq!(first_loss.reason, ControllerLoss::OriginalProcessExited);
        assert!(!watch.poll() && !watch.admission_open());
        watch.revoke(ControllerLoss::TransportEof);
        assert_eq!(watch.loss(), Some(first_loss));
        assert!(owner.exit_observation().is_none());
        assert!(!owner.cleanup_complete());
        assert_eq!(retained_outputs.len(), 2);

        readable(owner.pidfd.as_raw_fd()).unwrap();
        assert!(!owner.observe().unwrap());
        assert_eq!(
            owner.exit_observation(),
            Some(ExitObservation::Signaled(libc::SIGKILL))
        );
        assert!(!owner.output().stdout_eof && !owner.output().stderr_eof);
        drop(retained_outputs.remove(0));
        assert!(!owner.observe().unwrap());
        assert!(owner.output().stdout_eof && !owner.output().stderr_eof);
        drop(retained_outputs.remove(0));
        assert!(owner.observe().unwrap());
        assert!(owner.output().stdout_eof && owner.output().stderr_eof);
        assert!(owner.cleanup_complete());
        assert_eq!(watch.loss(), Some(first_loss));
        eprintln!(
            "exec-status actual-controller={original_pid} ready-child={} retained-created=true exact-pidfd-reaped=true stdout-eof=true stderr-eof=true synthetic-initial-lease=true product-owner=false tree-empty=unproved",
            owner.pid()
        );
    }
}
