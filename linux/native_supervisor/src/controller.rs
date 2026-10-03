//! Original-controller object and sticky liveness lease for synthetic native
//! prerequisites. No product owner, durable release or arbitrary PID factory.
use crate::{invalid, kernel_error};
use std::{
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
    time::{Duration, Instant},
};

/// A typed duplicate of the exact CLONE_PIDFD object retained by OwnedProcess.
/// Numeric PID is a bound diagnostic, never a reconstructed authority source.
pub struct OriginalController {
    fd: OwnedFd,
    pid: u32,
}
impl OriginalController {
    pub(crate) fn from_owned_child(fd: BorrowedFd<'_>, pid: u32) -> io::Result<Self> {
        let copied = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 16) };
        if copied < 0 {
            return Err(kernel_error("duplicate original controller pidfd"));
        }
        let result = Self {
            fd: unsafe { OwnedFd::from_raw_fd(copied) },
            pid,
        };
        result.validate()?;
        Ok(result)
    }
    /// Only the fixture bootstrap consumes this descriptor. Caller must own it
    /// exclusively and have received it from a typed owned-child or self handoff.
    pub(crate) unsafe fn from_inherited(fd: OwnedFd, pid: u32) -> io::Result<Self> {
        let result = Self { fd, pid };
        result.validate()?;
        Ok(result)
    }
    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
    pub fn diagnostic_pid(&self) -> u32 {
        self.pid
    }
    fn validate(&self) -> io::Result<()> {
        if self.pid == 0 {
            return Err(invalid("missing original controller identity"));
        }
        let flags = unsafe { libc::fcntl(self.fd.as_raw_fd(), libc::F_GETFD) };
        if flags < 0 || flags & libc::FD_CLOEXEC == 0 {
            return Err(invalid("original controller descriptor policy"));
        }
        // Signal zero does not deliver a signal. It validates that the inherited
        // kernel object is a pidfd, not a regular file/pipe. The trusted typed
        // handoff, not the diagnostic PID, supplies its original provenance.
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.fd.as_raw_fd(),
                0i32,
                std::ptr::null::<libc::siginfo_t>(),
                0u32,
            )
        };
        if result < 0 {
            return Err(kernel_error("validate original controller pidfd"));
        }
        if self.poll_exited()? {
            return Err(invalid("original controller already exited"));
        }
        Ok(())
    }
    /// Watches the original process/thread group. This never reaps a controller
    /// or treats observation failure as trusted resource-release proof.
    pub fn poll_exited(&self) -> io::Result<bool> {
        let mut poll = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        for _ in 0..4 {
            let result = unsafe { libc::poll(&mut poll, 1, 0) };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if poll.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
                return Err(invalid("original controller observation failed"));
            }
            return Ok(poll.revents & (libc::POLLIN | libc::POLLHUP) != 0);
        }
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "bounded controller poll interrupted",
        ))
    }
}

/// A self-only reference captured by the currently executing controller.
/// No caller PID or descriptor is accepted. The kernel object is retained,
/// while the captured process context rejects reuse after fork before launch.
/// This experimental fixture reference grants neither reaping nor owner rights.
pub struct CurrentController {
    original: OriginalController,
    captured_in: libc::pid_t,
}
impl CurrentController {
    pub fn capture() -> io::Result<Self> {
        Self::capture_with(|| {
            // The only production capture source is this executing process.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, libc::getpid(), 0u32) };
            if fd < 0 {
                return Err(kernel_error("capture current controller pidfd"));
            }
            Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
        })
    }
    fn capture_with(open_self: impl FnOnce() -> io::Result<OwnedFd>) -> io::Result<Self> {
        let captured_in = unsafe { libc::getpid() };
        let original = OriginalController {
            fd: open_self()?,
            pid: captured_in as u32,
        };
        original.validate()?;
        Ok(Self {
            original,
            captured_in,
        })
    }
    pub fn diagnostic_pid(&self) -> u32 {
        self.original.diagnostic_pid()
    }
    pub(crate) fn into_original(self) -> io::Result<OriginalController> {
        if self.captured_in != unsafe { libc::getpid() } {
            return Err(invalid("current controller capture process changed"));
        }
        self.original.validate()?;
        Ok(self.original)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerLoss {
    OriginalProcessExited,
    WatchError,
    HeartbeatExpired,
    TransportEof,
    Protocol,
    Backpressure,
    ClockRegression,
    BootstrapExpired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LossObservation {
    pub reason: ControllerLoss,
    pub at: Duration,
}

/// Pure monotonic liveness state. This has no reset, owner-generation creation,
/// resource release, business-deadline extension or persistence operation.
pub(crate) struct LeaseState {
    active: bool,
    timeout: Duration,
    last_pulse: Duration,
    last_now: Duration,
    heartbeat_sequence: u64,
    loss: Option<LossObservation>,
}
impl LeaseState {
    pub(crate) fn new(timeout: Duration) -> io::Result<Self> {
        if !(100..=5000).contains(&timeout.as_millis()) {
            return Err(invalid("controller lease timeout outside 100..5000ms"));
        }
        Ok(Self {
            active: true,
            timeout,
            last_pulse: Duration::ZERO,
            last_now: Duration::ZERO,
            heartbeat_sequence: 0,
            loss: None,
        })
    }
    fn check(&mut self, now: Duration) {
        if self.loss.is_some() {
            return;
        }
        if now < self.last_now {
            self.revoke(ControllerLoss::ClockRegression, self.last_now);
            return;
        }
        self.last_now = now;
        if !self.active {
            if now >= Duration::from_secs(5) {
                self.revoke(ControllerLoss::BootstrapExpired, now);
            }
        } else if now.saturating_sub(self.last_pulse) >= self.timeout {
            self.revoke(ControllerLoss::HeartbeatExpired, now);
        }
    }
    pub(crate) fn heartbeat(&mut self, sequence: u64, now: Duration) -> io::Result<()> {
        self.check(now); // An expired heartbeat cannot first renew itself.
        if self.loss.is_some() {
            return Err(invalid("controller lease already lost"));
        }
        if sequence <= self.heartbeat_sequence {
            self.revoke(ControllerLoss::Protocol, now);
            return Err(invalid("controller heartbeat sequence replay"));
        }
        self.heartbeat_sequence = sequence;
        self.active = true;
        self.last_pulse = now;
        Ok(())
    }
    pub(crate) fn revoke(&mut self, reason: ControllerLoss, now: Duration) {
        self.loss.get_or_insert(LossObservation {
            reason,
            at: now.max(self.last_now),
        });
    }
}

/// One original-controller watch. Poll before admitting work; heartbeat inputs
/// must come from the bound authenticated transport, not a guest/public frame.
/// Every operation is bounded/nonblocking; the driver must keep polling while
/// output is blocked. A loss closes admission but proves no resource cleanup.
pub struct ControllerWatch {
    original: OriginalController,
    state: LeaseState,
    session: [u8; 32],
    created: Instant,
}
impl ControllerWatch {
    /// Synthetic initial admission for the exec-status unit regression only.
    /// This is not an authenticated heartbeat or a product bootstrap.
    #[cfg(test)]
    pub(crate) fn synthetic_active_for_exec_status_test(
        original: OriginalController,
    ) -> io::Result<Self> {
        let mut watch = Self::new(original, Duration::from_secs(5), [0x71; 32])?;
        watch.state.heartbeat(1, Duration::ZERO)?;
        Ok(watch)
    }

    pub(crate) fn new(
        original: OriginalController,
        timeout: Duration,
        session: [u8; 32],
    ) -> io::Result<Self> {
        let mut state = LeaseState::new(timeout)?;
        state.active = false;
        Ok(Self {
            original,
            state,
            session,
            created: Instant::now(),
        })
    }
    pub fn diagnostic_pid(&self) -> u32 {
        self.original.diagnostic_pid()
    }
    pub fn poll(&mut self) -> bool {
        let now = self.created.elapsed();
        if self.state.loss.is_none() {
            match self.original.poll_exited() {
                Ok(false) => (),
                Ok(true) => self
                    .state
                    .revoke(ControllerLoss::OriginalProcessExited, now),
                Err(_) => self.state.revoke(ControllerLoss::WatchError, now),
            }
        }
        self.state.check(now);
        self.state.loss.is_none()
    }
    pub fn heartbeat(
        &mut self,
        frame: &crate::controller_transport::AuthenticatedFrame,
    ) -> io::Result<()> {
        if !self.poll() {
            return Err(invalid("controller watch already lost"));
        }
        if frame.session() != &self.session
            || frame.kind() != crate::controller_transport::FrameKind::Heartbeat
        {
            self.revoke(ControllerLoss::Protocol);
            return Err(invalid("heartbeat lacks original authenticated session"));
        }
        self.state
            .heartbeat(frame.sequence(), self.created.elapsed())
    }
    pub fn revoke(&mut self, reason: ControllerLoss) {
        self.state.revoke(reason, self.created.elapsed());
    }
    pub fn admission_open(&self) -> bool {
        self.state.active && self.state.loss.is_none()
    }
    pub fn gate_closed(&self) -> bool {
        self.state.loss.is_some()
    }
    pub fn loss(&self) -> Option<LossObservation> {
        self.state.loss
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_capture_retains_self_kernel_object_and_cloexec() {
        let captured = CurrentController::capture().unwrap();
        assert_eq!(captured.diagnostic_pid(), std::process::id());
        assert!(!captured.original.poll_exited().unwrap());
        let flags = unsafe { libc::fcntl(captured.original.fd.as_raw_fd(), libc::F_GETFD) };
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
        assert!(captured.into_original().is_ok());
    }
    #[test]
    fn current_capture_failure_has_no_numeric_or_descriptor_fallback() {
        // Inject only the capture syscall result; no pidfd was created here.
        let result =
            CurrentController::capture_with(|| Err(io::Error::from_raw_os_error(libc::EMFILE)));
        assert_eq!(result.err().unwrap().raw_os_error(), Some(libc::EMFILE));
    }
    #[test]
    fn timeout_boundary_and_late_heartbeat_are_sticky() {
        let mut lease = LeaseState::new(Duration::from_millis(100)).unwrap();
        lease.heartbeat(1, Duration::from_millis(99)).unwrap();
        assert!(lease.heartbeat(2, Duration::from_millis(199)).is_err());
        assert_eq!(lease.loss.unwrap().reason, ControllerLoss::HeartbeatExpired);
        assert!(lease.heartbeat(3, Duration::from_millis(200)).is_err());
    }
    #[test]
    fn first_loss_wins_and_sequence_cannot_replay() {
        let mut lease = LeaseState::new(Duration::from_secs(1)).unwrap();
        lease.heartbeat(2, Duration::from_millis(1)).unwrap();
        assert!(lease.heartbeat(2, Duration::from_millis(2)).is_err());
        let original = lease.loss;
        lease.revoke(ControllerLoss::TransportEof, Duration::from_millis(3));
        assert_eq!(lease.loss, original);
    }
    #[test]
    fn bootstrap_deadline_is_separate_and_first_pulse_starts_live_lease() {
        let mut lease = LeaseState::new(Duration::from_millis(200)).unwrap();
        lease.active = false;
        lease.check(Duration::from_secs(1));
        assert!(lease.loss.is_none());
        lease.heartbeat(1, Duration::from_secs(1)).unwrap();
        lease.check(Duration::from_millis(1200));
        assert_eq!(lease.loss.unwrap().reason, ControllerLoss::HeartbeatExpired);
        let mut lease = LeaseState::new(Duration::from_millis(200)).unwrap();
        lease.active = false;
        lease.check(Duration::from_secs(5));
        assert_eq!(lease.loss.unwrap().reason, ControllerLoss::BootstrapExpired);
    }
    #[test]
    fn backwards_clock_and_invalid_limits_reject() {
        assert!(LeaseState::new(Duration::from_millis(99)).is_err());
        let mut lease = LeaseState::new(Duration::from_secs(1)).unwrap();
        lease.check(Duration::from_millis(10));
        lease.check(Duration::from_millis(9));
        assert_eq!(lease.loss.unwrap().reason, ControllerLoss::ClockRegression);
    }
}
