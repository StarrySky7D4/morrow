//! Owned setup capture only. A cap boundary is NEVER physical EOF.
//! A single prelaunch admission retains all original child/pipe/reader users on
//! error, deadline or unwind. No recovery, replay, kill, PID reopen or cleanup.
use crate::setup_launch::SpawnedOwnedSetupChild;
use crate::setup_provisioning::{PinnedOwnedSetupDirectories, OwnedAppliedPolicyProof, OwnedSetupValidation, OwnedWindowsSandboxSetup};
use std::io::{self, Read};
use std::os::windows::io::AsRawHandle;
use std::process::{Child, ChildStdout, ChildStderr, ExitStatus};
use std::sync::{Arc, Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static RETAINED: OnceLock<State> = OnceLock::new();
pub(super) struct Admission { _private: () }
impl Admission {
    pub(super) fn reserve() -> io::Result<Self> {
        ACTIVE.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock,
                "owned setup original capture active or retained Unknown; no replay"))?;
        Ok(Self { _private: () })
    }
}
impl Drop for Admission {
    fn drop(&mut self) { ACTIVE.store(false, Ordering::SeqCst); }
}
// Only this read loop can produce the physical-EOF success value.
#[derive(Debug)]
struct EofBytes(Vec<u8>);
fn read_to_real_eof<R: Read>(input: &mut R, cap: usize) -> io::Result<EofBytes> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        // Reject exact-cap too: another blocking read would be needed to prove
        // EOF, so a valid prefix at this artificial boundary is not accepted.
        let remaining = cap.saturating_sub(bytes.len());
        if remaining == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData,
                "owned setup capture reached byte cap before real EOF"));
        }
        let count = chunk.len().min(remaining);
        let n = match input.read(&mut chunk[..count]) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if n == 0 { return Ok(EofBytes(bytes)); }
        if n > count { return Err(io::Error::new(io::ErrorKind::InvalidData, "reader exceeded requested bound")); }
        bytes.extend_from_slice(&chunk[..n]);
    }
}
type Worker = JoinHandle<io::Result<EofBytes>>;
struct State {
    // Keep the permit LAST for normal release only. OnceLock retains it forever
    // on Unknown; no new execution slot or later invocation can replace it.
    child: Child,
    child_pid: u32,
    stdout: Arc<Mutex<ChildStdout>>,
    stderr: Arc<Mutex<ChildStderr>>,
    stdout_worker: Option<Worker>,
    stderr_worker: Option<Worker>,
    stdout_result: Option<io::Result<EofBytes>>,
    stderr_result: Option<io::Result<EofBytes>>,
    status: Option<ExitStatus>,
    validation: OwnedSetupValidation,
    validation_started: bool,
    validation_complete: bool,
    original_deadline: Option<Instant>,
    first_error: Option<io::Error>,
    complete: bool,
    started: bool,
    _admission: Admission,
}
pub(super) struct Capture { state: Option<State> }
fn copy_error(e: &io::Error) -> io::Error {
    match e.raw_os_error() {
        Some(code) => io::Error::from_raw_os_error(code),
        None => io::Error::new(e.kind(), e.to_string()),
    }
}
fn start_reader<R: Read + Send + 'static>(
    name: &'static str, stream: Arc<Mutex<R>>, cap: usize,
) -> io::Result<Worker> {
    std::thread::Builder::new().name(name.into()).spawn(move || {
        // Private pipe mutex only, never Core/owner/registry state. The parent
        // retains an Arc even if spawn fails or the actual reader panics. It
        // never locks this stream while observing/waiting/joining the worker.
        let mut original = stream.lock().map_err(|_| io::Error::other("owned setup pipe poisoned"))?;
        read_to_real_eof(&mut *original, cap)
    })
}
fn collect_worker(worker: &mut Option<Worker>, result: &mut Option<io::Result<EofBytes>>) -> io::Result<()> {
    let Some(handle) = worker.as_ref() else { return Ok(()) };
    // is_finished alone does not prove native/TLS termination. Observe the
    // SAME original thread handle without waiting past the caller's budget.
    match unsafe { WaitForSingleObject(handle.as_raw_handle() as HANDLE, 0) } {
        WAIT_OBJECT_0 => {
            let handle = worker.take().expect("same original signaled reader");
            *result = Some(handle.join().unwrap_or_else(|_| Err(io::Error::other("owned setup reader panicked"))));
            Ok(())
        }
        WAIT_TIMEOUT => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}
// Borrowed transaction driver: the callback never receives ownership of T.
// Its caller must keep T in the Capture through every check including postcheck.
fn run_retained_validation<T>(
    original: &mut T, started: &mut bool, first_error: &mut Option<io::Error>,
    deadline: Instant, verify: impl FnOnce(&mut T) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    if let Some(error) = first_error.as_ref() { return Err(copy_error(error).into()); }
    if *started {
        let error = io::Error::other("owned setup validation already attempted; no replay");
        *first_error = Some(copy_error(&error));
        return Err(error.into());
    }
    *started = true;
    struct UnwindLatch<'a> { error: &'a mut Option<io::Error>, armed: bool }
    impl Drop for UnwindLatch<'_> {
        fn drop(&mut self) {
            if self.armed && self.error.is_none() {
                *self.error = Some(io::Error::other("owned setup validation unwound; original owners retained/noReplay"));
            }
        }
    }
    let mut latch = UnwindLatch { error: first_error, armed: true };
    let outcome: anyhow::Result<()> = (|| {
        crate::setup_provisioning::owned_setup_remaining_lock_ms(deadline, Instant::now())?;
        verify(original)?;
        crate::setup_provisioning::owned_setup_remaining_lock_ms(deadline, Instant::now())?;
        Ok(())
    })();
    if let Err(error) = &outcome {
        *latch.error = Some(match error.downcast_ref::<io::Error>() {
            Some(original) => copy_error(original),
            None => io::Error::other(format!("{error:#}")),
        });
    }
    latch.armed = false;
    outcome
}

impl Capture {
    pub(super) fn new(child: SpawnedOwnedSetupChild, pins: PinnedOwnedSetupDirectories, policy: OwnedAppliedPolicyProof, admission: Admission) -> Self {
        Self { state: Some(State { child: child.child, child_pid: child.child_pid,
            stdout: Arc::new(Mutex::new(child.stdout)), stderr: Arc::new(Mutex::new(child.stderr)),
            stdout_worker: None, stderr_worker: None, stdout_result: None, stderr_result: None,
            status: None, validation: OwnedSetupValidation::new(pins, policy), validation_started: false, validation_complete: false, original_deadline: None, first_error: None, complete: false, started: false, _admission: admission }) }
    }
    pub(super) fn observe(&mut self, deadline: Instant) -> io::Result<()> {
        let state = self.state.as_mut().expect("original capture retained");
        if let Some(error) = &state.first_error { return Err(copy_error(error)); }
        if state.started {
            state.first_error = Some(io::Error::other("owned setup capture already observed; no replay"));
            return Err(copy_error(state.first_error.as_ref().unwrap()));
        }
        state.started = true;
        state.original_deadline = Some(deadline);
        let outcome = (|| {
            if Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "owned setup capture budget expired before readers")); }
            // Both original pipe Arcs already belong to State before a spawner
            // can fail, and the first original reader is anchored before the second.
            state.stdout_worker = Some(start_reader("owned setup stdout", Arc::clone(&state.stdout), 512 * 1024)?);
            state.stderr_worker = Some(start_reader("owned setup stderr", Arc::clone(&state.stderr), 256 * 1024)?);
            loop {
                if Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "owned setup capture budget expired")); }
                collect_worker(&mut state.stdout_worker, &mut state.stdout_result)?;
                if let Some(Err(error)) = &state.stdout_result { return Err(copy_error(error)); }
                collect_worker(&mut state.stderr_worker, &mut state.stderr_result)?;
                if let Some(Err(error)) = &state.stderr_result { return Err(copy_error(error)); }
                if state.status.is_none() { state.status = state.child.try_wait()?; }
                if state.status.is_some() && state.stdout_result.is_some() && state.stderr_result.is_some() {
                    if Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "owned setup completion arrived after budget")); }
                    state.complete = true;
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        })();
        if let Err(error) = outcome {
            let returned = copy_error(&error);
            state.first_error = Some(error); // Original first error retained, no clearing/retry.
            return Err(returned);
        }
        Ok(())
    }
    pub(super) fn child_pid(&self) -> u32 { self.state.as_ref().unwrap().child_pid }
    pub(super) fn status(&self) -> ExitStatus { self.state.as_ref().unwrap().status.expect("original exit was observed") }
    pub(super) fn stdout_bytes(&self) -> &[u8] {
        let state = self.state.as_ref().unwrap();
        assert!(state.complete);
        &state.stdout_result.as_ref().unwrap().as_ref().expect("real stdout EOF").0
    }
    pub(super) fn validate(
        &mut self, deadline: Instant,
        verify: impl FnOnce(&mut OwnedSetupValidation) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let state = self.state.as_mut().unwrap();
        assert!(state.complete, "original native exit and IO must precede validation");
        assert_eq!(state.original_deadline, Some(deadline), "validation cannot extend the original capture deadline");
        run_retained_validation(&mut state.validation, &mut state.validation_started,
            &mut state.first_error, deadline, verify)?;
        assert!(state.validation.ready(), "all partial original proofs must remain owned");
        state.validation_complete = true;
        Ok(())
    }
    pub(super) fn commit(mut self) -> OwnedWindowsSandboxSetup {
        let state = self.state.as_ref().unwrap();
        // Assertions occur BEFORE disarming so any unwind retains the owner.
        assert!(state.complete && state.validation_complete && state.first_error.is_none());
        assert!(state.validation.ready());
        let state = self.state.take().unwrap();
        // No further fallible check/callback/native operation after this move.
        // The same original handles are transferred, never duplicated/reopened.
        state.validation.commit()
    }

}
impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(mut state) = self.state.take() {
            if state.first_error.is_none() {
                state.first_error = Some(io::Error::other("owned setup capture/receipt abandoned; retained Unknown/noReplay"));
            }
            // Admission is private/non-Clone and remains held by this State.
            // Before parking there is exactly one active capture; after parking
            // the retained Admission prevents every later reserve. Thus the
            // initializer runs exactly once for the permitted original attempt.
            // No overflow list, forget, recovery or replay path is introduced.
            RETAINED.get_or_init(|| state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn synthetic_reader_success_requires_real_zero_read_below_cap() {
        let mut input = Cursor::new(b"receipt".to_vec());
        assert_eq!(read_to_real_eof(&mut input, 8).unwrap().0, b"receipt");
        assert!(read_to_real_eof(&mut Cursor::new(Vec::<u8>::new()), 8).unwrap().0.is_empty());
    }
    #[test]
    fn synthetic_exact_cap_and_overflow_are_both_rejected_without_prefix_receipt() {
        for bytes in [br#"{"ok":1}"#.to_vec(), br#"{"ok":1}trailing-data"#.to_vec()] {
            let mut input = Cursor::new(bytes);
            assert_eq!(read_to_real_eof(&mut input, 8).unwrap_err().kind(), io::ErrorKind::InvalidData);
            assert_eq!(input.position(), 8); // bounded; no extra read/pretended EOF
        }
    }
    #[test]
    fn synthetic_non_eof_boundary_never_calls_an_extra_blocking_read() {
        struct Boundary { left: usize }
        impl Read for Boundary {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                assert!(self.left != 0, "must not block beyond capture cap");
                let n = self.left.min(buf.len());buf[..n].fill(b'x');self.left -= n;Ok(n)
            }
        }
        assert_eq!(read_to_real_eof(&mut Boundary { left: 4 }, 4).unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
    #[test]
    fn synthetic_reader_io_failure_preserves_error_instead_of_eof() {
        struct Fail;
        impl Read for Fail { fn read(&mut self, _: &mut [u8]) -> io::Result<usize> { Err(io::Error::from_raw_os_error(123)) } }
        assert_eq!(read_to_real_eof(&mut Fail, 8).unwrap_err().raw_os_error(), Some(123));
    }
    #[test]
    fn synthetic_interrupted_read_is_not_zero_eof() {
        struct Interrupted { once: bool }
        impl Read for Interrupted {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if !self.once { self.once = true; return Err(io::Error::from(io::ErrorKind::Interrupted)); }
                Cursor::new(b"abc").read(buf)
            }
        }
        // This stream never returns0: even after Interrupted, cap rejects it.
        assert_eq!(read_to_real_eof(&mut Interrupted { once: false }, 8).unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
    #[test]
    fn original_pipe_resource_remains_owned_after_reader_error_or_panic() {
        use std::sync::atomic::AtomicUsize;
        struct Original { drops: Arc<AtomicUsize>, panics: bool }
        impl Read for Original {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                if self.panics { panic!("pure original reader panic"); }
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
        }
        impl Drop for Original { fn drop(&mut self) { self.drops.fetch_add(1, Ordering::SeqCst); } }
        for panics in [false, true] {
            let drops = Arc::new(AtomicUsize::new(0));
            let original = Arc::new(Mutex::new(Original { drops: Arc::clone(&drops), panics }));
            let worker = start_reader("synthetic original reader", Arc::clone(&original), 8).unwrap();
            let joined = worker.join(); // the same actual test thread, no fake EOF
            if panics { assert!(joined.is_err());assert!(original.is_poisoned()); }
            else { assert_eq!(joined.unwrap().unwrap_err().kind(), io::ErrorKind::BrokenPipe); }
            assert_eq!(drops.load(Ordering::SeqCst), 0, "error must not drop original pipe owner");
            drop(original);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }

    // These call the SAME production borrowed transaction driver, using real
    // original file owners and separately acquired partial resources, not copied
    // handles or a mirror of its decision logic. They do not provision accounts.
    struct ValidationOriginals {
        original: std::fs::File,
        partial: Option<std::fs::File>,
    }
    impl ValidationOriginals {
        fn new() -> Self { Self { original: tempfile::tempfile().unwrap(), partial: None } }
        fn assert_live(&self, original: std::os::windows::io::RawHandle,
            partial: Option<std::os::windows::io::RawHandle>) {
            assert_eq!(self.original.as_raw_handle(), original);
            assert!(self.original.metadata().is_ok(), "same original file remains owned");
            if let Some(expected) = partial {
                let actual = self.partial.as_ref().expect("partial owner retained");
                assert_eq!(actual.as_raw_handle(), expected);
                assert!(actual.metadata().is_ok(), "same partial original remains owned");
            }
        }
    }
    #[test]
    fn validation_error_at_each_stage_retains_original_and_partial_owners_and_first_error() {
        for stage in ["receipt", "installation", "settings", "continuity", "principal",
            "hidden", "firewall", "WFP", "ACL", "postcheck"] {
            let mut originals = ValidationOriginals::new();
            let original = originals.original.as_raw_handle();
            let mut partial = None;
            let mut started = false;let mut error = None;
            let returned = run_retained_validation(&mut originals, &mut started, &mut error,
                Instant::now() + Duration::from_secs(2), |owned| {
                    owned.partial = Some(tempfile::tempfile()?);
                    partial = Some(owned.partial.as_ref().unwrap().as_raw_handle());
                    anyhow::bail!("injected {stage} validation error")
                }).unwrap_err();
            assert_eq!(returned.to_string(), format!("injected {stage} validation error"));
            originals.assert_live(original, partial);assert!(started);
            let first = error.as_ref().unwrap().to_string();
            assert_eq!(first, returned.to_string());
            let mut replayed = false;
            assert!(run_retained_validation(&mut originals, &mut started, &mut error,
                Instant::now()+Duration::from_secs(2), |_| { replayed = true;Ok(()) }).is_err());
            assert!(!replayed);assert_eq!(error.as_ref().unwrap().to_string(), first);
            originals.assert_live(original, partial);
        }
    }
    #[test]
    fn validation_expired_budget_and_lock_timeout_keep_originals_and_timedout_first_error() {
        for expired in [true, false] {
            let mut originals = ValidationOriginals::new();
            originals.partial = Some(tempfile::tempfile().unwrap());
            let original = originals.original.as_raw_handle();
            let partial = Some(originals.partial.as_ref().unwrap().as_raw_handle());
            let mut started = false;let mut error = None;let mut entered = false;
            let deadline = if expired { Instant::now()-Duration::from_secs(1) }
                else { Instant::now()+Duration::from_secs(2) };
            let result = run_retained_validation(&mut originals, &mut started, &mut error, deadline,
                |_| { entered = true;Err(io::Error::new(io::ErrorKind::TimedOut, "injected native mutex wait timeout").into()) });
            assert_eq!(result.unwrap_err().downcast_ref::<io::Error>().unwrap().kind(), io::ErrorKind::TimedOut);
            assert_eq!(entered, !expired);
            assert_eq!(error.as_ref().unwrap().kind(), io::ErrorKind::TimedOut);
            originals.assert_live(original, partial);
        }
    }
    #[test]
    fn validation_unwind_retains_same_partial_originals_and_latches_no_replay() {
        let mut originals = ValidationOriginals::new();
        let original = originals.original.as_raw_handle();
        let mut partial = None;let mut started = false;let mut error = None;
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = run_retained_validation(&mut originals, &mut started, &mut error,
                Instant::now()+Duration::from_secs(2), |owned| {
                    owned.partial = Some(tempfile::tempfile().unwrap());
                    partial = Some(owned.partial.as_ref().unwrap().as_raw_handle());
                    panic!("injected validation unwind after partial construction");
                });
        }));
        assert!(unwind.is_err());assert!(started);
        assert!(error.as_ref().unwrap().to_string().contains("unwound"));
        originals.assert_live(original, partial);
        let mut replayed = false;
        assert!(run_retained_validation(&mut originals, &mut started, &mut error,
            Instant::now()+Duration::from_secs(2), |_| { replayed = true;Ok(()) }).is_err());
        assert!(!replayed);originals.assert_live(original, partial);
    }
    #[test]
    fn validation_preserves_existing_first_error_and_success_does_not_transfer_owners() {
        let mut originals = ValidationOriginals::new();
        let original = originals.original.as_raw_handle();let mut started = false;
        let mut error = Some(io::Error::from_raw_os_error(32));let mut entered = false;
        let returned = run_retained_validation(&mut originals, &mut started, &mut error,
            Instant::now()+Duration::from_secs(2), |_| { entered=true;Ok(()) }).unwrap_err();
        assert_eq!(returned.downcast_ref::<io::Error>().unwrap().raw_os_error(), Some(32));
        assert!(!entered);assert!(!started);originals.assert_live(original, None);
        // Separate fresh transaction: callback success only validates a borrowed
        // owner. It cannot commit, release or fabricate a native setup proof.
        let mut fresh_started = false;let mut fresh_error = None;
        run_retained_validation(&mut originals, &mut fresh_started, &mut fresh_error,
            Instant::now()+Duration::from_secs(2), |owned| { owned.partial=Some(tempfile::tempfile()?);Ok(()) }).unwrap();
        assert!(fresh_error.is_none());assert!(fresh_started);
        originals.assert_live(original, Some(originals.partial.as_ref().unwrap().as_raw_handle()));
    }

}
