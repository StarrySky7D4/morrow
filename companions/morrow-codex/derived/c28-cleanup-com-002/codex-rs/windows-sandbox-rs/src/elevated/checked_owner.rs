//! Process-lifetime bounded startup and runtime ownership for checked elevated runners.
//!
//! This module retains the original runner process `OwnedHandle`, validated creation
//! identity, thread disposition, connect helper attempt records, and runtime I/O worker
//! handles across startup errors, unwinds, dropped transports, cancelled `spawn_blocking`
//! awaits, and runtime execution until an explicit bounded finalization/reap succeeds.

use super::owned_connect_worker::{
    self, AttemptId, CONNECT_TIMEOUT, CancelObservation, JoinedErrorMeta, Observation,
    UnknownReason, finite_wait_ms,
};
use crate::MatchedRunnerArtifact;
use crate::ipc_framed::{ControlHello, RunnerControlCapabilities};
use std::fmt;
use std::fs::File;
use std::future::Future;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, OwnedHandle};
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context as TaskContext, Poll, Wake, Waker};
use std::thread::JoinHandle as StdJoinHandle;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle as TokioJoinHandle;
use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_INVALID_HANDLE, FILETIME,
    GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessId, GetProcessTimes, WaitForSingleObject,
};

/// Conservative local retention limit matching the outer 16-slot native execution budget.
/// This crate does not claim unified external quota or release eligibility without an
/// outer process-map/keeper adapter.
pub(crate) const LOCAL_CHECKED_OWNER_CAPACITY: usize = 16;
pub(crate) const RELEASE_ELIGIBLE: bool = false;
pub(crate) const RUNNER_STARTUP_DEADLINE_WINDOW: Duration = Duration::from_secs(15);

/// Declaration seam for binding local checked runner retention to the outer Morrow native
/// execution authority (`MAX_NATIVE_EXECUTIONS = 16`, `NativeExecutionRegistry`, and
/// `ExecutionSlot` in `companions/morrow-codex/session-exec-windows-v1/src/registry.rs`).
///
/// This trait is only declared in `windows-sandbox-rs` and is not yet wired to the outer
/// `session-exec-windows-v1` registry/provider.
#[allow(dead_code)]
pub(crate) trait ExternalNativeQuotaAdapter: Send + Sync {
    fn try_acquire_slot(&self, startup_id: StartupId) -> io::Result<()>;
    fn release_slot_after_physical_close(&self, startup_id: StartupId);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct StartupId(pub(crate) u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OriginalProcessIdentity {
    pub(crate) pid: u32,
    pub(crate) creation_filetime: u64,
}

impl OriginalProcessIdentity {
    /// Read only this retained original handle. A signaled wait precedes the
    /// DWORD query, so an actual exit259 is never confused with STILL_ACTIVE.
    pub(crate) fn observe_startup_process(&self, process: &OwnedHandle, startup_id: StartupId)
        -> crate::WindowsStartRunnerObservation
    {
        crate::start_diagnostic::context_query::sample_runner(
            process.as_raw_handle() as HANDLE,startup_id.0,self.pid,self.creation_filetime)
    }

    pub(crate) fn from_live_process(
        process: &OwnedHandle,
        expected_pid: u32,
    ) -> io::Result<Self> {
        if expected_pid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected runner pid cannot be zero",
            ));
        }
        let raw = process.as_raw_handle() as HANDLE;
        if raw == 0 || raw == INVALID_HANDLE_VALUE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "original runner process handle is invalid",
            ));
        }
        let actual_pid = unsafe { GetProcessId(raw) };
        if actual_pid == 0 || actual_pid != expected_pid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "original runner handle pid {actual_pid} did not match expected pid {expected_pid}"
                ),
            ));
        }
        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut kernel = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut user = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let ok = unsafe {
            GetProcessTimes(raw, &mut creation, &mut exit, &mut kernel, &mut user)
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let creation_filetime =
            ((creation.dwHighDateTime as u64) << 32) | (creation.dwLowDateTime as u64);
        if creation_filetime == 0 {
            return Err(io::Error::other(
                "original runner process reported zero creation time",
            ));
        }
        Ok(Self {
            pid: actual_pid,
            creation_filetime,
        })
    }

    pub(crate) fn verify_same_process(&self, process: &OwnedHandle) -> io::Result<()> {
        let observed = Self::from_live_process(process, self.pid)?;
        if observed.creation_filetime != self.creation_filetime {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "original runner creation time 0x{:016x} did not match expected 0x{:016x} for pid {}",
                    observed.creation_filetime, self.creation_filetime, self.pid
                ),
            ));
        }
        Ok(())
    }
}

/// Explicit uncertain close ownership/debt record for an original primary thread handle
/// whose `CloseHandle` call failed after `IntoRawHandle::into_raw_handle` consumed the
/// `OwnedHandle`.
///
/// Because Windows handle values can become invalid or be reused after an uncertain
/// `CloseHandle` failure, this raw handle value is never reconstructed into an
/// `OwnedHandle`, dropped, or retried with `CloseHandle`. While this debt is present,
/// `CheckedRunnerOwner` must never report physical reap/normal close or release its
/// bounded `CheckedOwnerRegistry` slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OriginalThreadCloseDebt {
    pub(crate) raw_handle: usize,
    pub(crate) thread_id: u32,
    pub(crate) os_error: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OriginalThreadDisposition {
    ClosedOnCapture {
        thread_id: u32,
    },
    CloseFailedUnknown {
        raw_handle: usize,
        thread_id: u32,
        os_error: u32,
    },
    ScopedSynthetic,
}

impl OriginalThreadDisposition {
    pub(crate) fn close_original_thread(thread_handle: OwnedHandle, thread_id: u32) -> Self {
        Self::close_original_thread_with(thread_handle, thread_id, |raw| {
            let ok = unsafe { CloseHandle(raw) };
            if ok != 0 {
                Ok(())
            } else {
                Err(unsafe { GetLastError() })
            }
        })
    }

    pub(crate) fn close_original_thread_with<F>(
        thread_handle: OwnedHandle,
        thread_id: u32,
        close_fn: F,
    ) -> Self
    where
        F: FnOnce(HANDLE) -> Result<(), u32>,
    {
        let raw = thread_handle.into_raw_handle() as HANDLE;
        if raw == 0 || raw == INVALID_HANDLE_VALUE {
            return Self::CloseFailedUnknown {
                raw_handle: raw as usize,
                thread_id,
                os_error: ERROR_INVALID_HANDLE,
            };
        }
        match close_fn(raw) {
            Ok(()) => Self::ClosedOnCapture { thread_id },
            Err(os_error) => Self::CloseFailedUnknown {
                raw_handle: raw as usize,
                thread_id,
                os_error,
            },
        }
    }

    pub(crate) fn is_clean_close(&self) -> bool {
        matches!(self, Self::ClosedOnCapture { .. } | Self::ScopedSynthetic)
    }

    pub(crate) fn close_debt(&self) -> Option<OriginalThreadCloseDebt> {
        match *self {
            Self::CloseFailedUnknown {
                raw_handle,
                thread_id,
                os_error,
            } => Some(OriginalThreadCloseDebt {
                raw_handle,
                thread_id,
                os_error,
            }),
            Self::ClosedOnCapture { .. } | Self::ScopedSynthetic => None,
        }
    }
}

/// Marker attached to `anyhow::Error` when a checked startup fails after process launch
/// or enters an `Unknown` / `noReplay` state. Preserves the underlying error in the
/// `anyhow` chain while vetoing credential refresh and relaunch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckedStartupNonRefreshable {
    pub(crate) startup_id: StartupId,
    pub(crate) message: String,
}

impl fmt::Display for CheckedStartupNonRefreshable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

pub(crate) fn mark_error_non_refreshable(
    err: anyhow::Error,
    startup_id: StartupId,
) -> anyhow::Error {
    if err.downcast_ref::<CheckedStartupNonRefreshable>().is_some() {
        return err;
    }
    let message = err.to_string();
    err.context(CheckedStartupNonRefreshable {
        startup_id,
        message,
    })
}

pub(crate) fn is_error_marked_non_refreshable(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CheckedStartupNonRefreshable>().is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HelperAttemptRecord {
    pub(crate) attempt_id: AttemptId,
    pub(crate) pipe_label: &'static str,
    pub(crate) acknowledged: bool,
    pub(crate) within_deadline: bool,
    pub(crate) had_unknown: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StickyUnknownReason {
    CallerAwaitCancelled,
    IdentityValidationFailed(String),
    ThreadCloseFailed {
        raw_handle: usize,
        thread_id: u32,
        os_error: u32,
    },
    ConnectRetainedUnknown {
        attempt_id: AttemptId,
        reason: UnknownReason,
        cancel: CancelObservation,
    },
    LateConnectSuccess {
        attempt_id: AttemptId,
    },
    ConnectFailed {
        attempt_id: AttemptId,
        message: String,
    },
    HandshakeFailed(String),
    StartupUnwound,
    TransportDroppedUnadopted,
    WrongLegacyConversion,
    CheckedIoDroppedUnadopted,
    RuntimeLifecycleFailure,
    ReaderIncompleteOrFailed,
    WriterFailed,
    InputTaskFailedOrAborted,
    FinalizationDeadlineExpired,
    ProcessWaitExpiredOrFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerLifecycleStage {
    ReservedBeforeLaunch,
    ProcessLaunchedInHandshake,
    HandshakeSucceededInTransport,
    CheckedIoExtracted,
    ActorAdopted,
    FailedOrAbandoned,
    FinalizedClean,
    PhysicallyReapedStickyUnknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReaderWorkerOutcome {
    pub(crate) protocol_exit: Option<i32>,
    pub(crate) physical_eof: bool,
    pub(crate) clean: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WriterWorkerOutcome {
    pub(crate) clean: bool,
    pub(crate) termination_sent: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InputWorkerOutcome {
    pub(crate) clean: bool,
}

pub(crate) enum StdWorkerSlot<T> {
    Pending(StdJoinHandle<T>),
    Joined {
        outcome: Result<T, String>,
        within_deadline: bool,
    },
}

impl<T> StdWorkerSlot<T> {
    fn observe_and_join(
        &mut self,
        deadline: Instant,
        label: &'static str,
    ) -> io::Result<(Result<T, String>, bool)>
    where
        T: Clone,
    {
        let call_in_time = Instant::now() < deadline;
        if let Self::Joined {
            ref outcome,
            ref mut within_deadline,
        } = *self
        {
            if !call_in_time {
                *within_deadline = false;
            }
            return Ok((outcome.clone(), *within_deadline));
        }
        let wait = match *self {
            Self::Pending(ref handle) => {
                let wait_ms =
                    finite_wait_ms(deadline.saturating_duration_since(Instant::now()));
                unsafe { WaitForSingleObject(handle.as_raw_handle() as HANDLE, wait_ms) }
            }
            Self::Joined { .. } => WAIT_OBJECT_0,
        };
        if wait != WAIT_OBJECT_0 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                format!("{label} native thread has not signaled exit"),
            ));
        }
        let signaled_in_time = call_in_time && Instant::now() < deadline;
        // Only join after bounded native thread signal observation (WAIT_OBJECT_0).
        let prev = std::mem::replace(
            self,
            Self::Joined {
                outcome: Err(format!("{label} join in progress")),
                within_deadline: false,
            },
        );
        let Self::Pending(handle) = prev else {
            return Err(io::Error::other("worker slot state corrupted"));
        };
        let joined = handle
            .join()
            .map_err(|_| format!("{label} panicked during execution"));
        let within_deadline = signaled_in_time && Instant::now() < deadline;
        *self = Self::Joined {
            outcome: joined.clone(),
            within_deadline,
        };
        Ok((joined, within_deadline))
    }
}

struct SignalWaker {
    notified: Mutex<bool>,
    cv: Condvar,
}

impl Wake for SignalWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if let Ok(mut guard) = self.notified.lock() {
            *guard = true;
            self.cv.notify_all();
        }
    }
}

pub(crate) enum AsyncInputSlot {
    Pending(TokioJoinHandle<InputWorkerOutcome>),
    Joined {
        outcome: Result<InputWorkerOutcome, String>,
        within_deadline: bool,
        error_kind: Option<HostAsyncInputJoinErrorKind>,
    },
}

impl AsyncInputSlot {
    fn poll_once(&mut self) -> Option<&Result<InputWorkerOutcome, String>> {
        if let Self::Joined { ref outcome, .. } = *self {
            return Some(outcome);
        }
        let signal = Arc::new(SignalWaker {
            notified: Mutex::new(false),
            cv: Condvar::new(),
        });
        let waker = Waker::from(signal);
        let mut cx = TaskContext::from_waker(&waker);
        let polled = match *self {
            Self::Pending(ref mut handle) => Pin::new(handle).poll(&mut cx),
            Self::Joined { .. } => return None,
        };
        match polled {
            Poll::Pending => None,
            Poll::Ready(res) => {
                let (mapped, error_kind) = match res {
                    Ok(outcome) => (Ok(outcome), None),
                    Err(err) if err.is_cancelled() => (
                        Err("checked async input task was cancelled".to_owned()),
                        Some(HostAsyncInputJoinErrorKind::Cancelled),
                    ),
                    Err(_) => (
                        Err("checked async input task panicked".to_owned()),
                        Some(HostAsyncInputJoinErrorKind::Panicked),
                    ),
                };
                *self = Self::Joined {
                    outcome: mapped,
                    within_deadline: true,
                    error_kind,
                };
                let Self::Joined { ref outcome, .. } = *self else {
                    return None;
                };
                Some(outcome)
            }
        }
    }

    fn observe_and_join_blocking(
        &mut self,
        deadline: Instant,
    ) -> io::Result<(Result<InputWorkerOutcome, String>, bool)> {
        let call_in_time = Instant::now() < deadline;
        if let Self::Joined {
            ref outcome,
            ref mut within_deadline,
            ..
        } = *self
        {
            if !call_in_time {
                *within_deadline = false;
            }
            return Ok((outcome.clone(), *within_deadline));
        }
        let signal = Arc::new(SignalWaker {
            notified: Mutex::new(false),
            cv: Condvar::new(),
        });
        let waker = Waker::from(Arc::clone(&signal));
        loop {
            let mut cx = TaskContext::from_waker(&waker);
            let polled = match *self {
                Self::Pending(ref mut handle) => Pin::new(handle).poll(&mut cx),
                Self::Joined {
                    ref outcome,
                    within_deadline,
                    ..
                } => return Ok((outcome.clone(), within_deadline && call_in_time)),
            };
            if let Poll::Ready(res) = polled {
                let (mapped, error_kind) = match res {
                    Ok(outcome) => (Ok(outcome), None),
                    Err(err) if err.is_cancelled() => (
                        Err("checked async input task was cancelled".to_owned()),
                        Some(HostAsyncInputJoinErrorKind::Cancelled),
                    ),
                    Err(_) => (
                        Err("checked async input task panicked".to_owned()),
                        Some(HostAsyncInputJoinErrorKind::Panicked),
                    ),
                };
                let within_deadline = call_in_time && Instant::now() < deadline;
                *self = Self::Joined {
                    outcome: mapped.clone(),
                    within_deadline,
                    error_kind,
                };
                return Ok((mapped, within_deadline));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "checked async input task has not completed",
                ));
            }
            let Ok(guard) = signal.notified.lock() else {
                return Err(io::Error::other("input waker lock poisoned"));
            };
            if *guard {
                drop(guard);
                if let Ok(mut reset) = signal.notified.lock() {
                    *reset = false;
                }
                continue;
            }
            let wait_slice = remaining.min(Duration::from_millis(10));
            let Ok((mut guard, _)) = signal.cv.wait_timeout(guard, wait_slice) else {
                return Err(io::Error::other("input waker wait poisoned"));
            };
            *guard = false;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckedFinalizationOutcome {
    HostNativeClosedOuterScopePending {
        startup_id: StartupId,
        protocol_exit_code: i32,
        native_exit_code: u32,
        release_eligible: bool,
    },
    CleanRelease {
        startup_id: StartupId,
        protocol_exit_code: i32,
        native_exit_code: u32,
        release_eligible: bool,
    },
    PhysicallyReapedWithStickyUnknown {
        startup_id: StartupId,
        native_exit_code: Option<u32>,
        reason: StickyUnknownReason,
        no_replay_sealed: bool,
        release_eligible: bool,
    },
}

fn duplicate_thread_handle_anchor<T>(join_handle: &StdJoinHandle<T>) -> Option<Arc<OwnedHandle>> {
    let raw = join_handle.as_raw_handle() as HANDLE;
    if raw == 0 || raw == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut dup: HANDLE = 0;
    let current_process = unsafe { GetCurrentProcess() };
    let ok = unsafe {
        DuplicateHandle(
            current_process,
            raw,
            current_process,
            &mut dup,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if ok == 0 || dup == 0 || dup == INVALID_HANDLE_VALUE {
        None
    } else {
        Some(Arc::new(unsafe { OwnedHandle::from_raw_handle(dup as _) }))
    }
}

pub(crate) enum WorkerSlotState<S> {
    NotInstalled,
    ReservedForInstall,
    Installed(S),
    LoanedToFinalizer,
}

impl<S> WorkerSlotState<S> {
    fn is_present(&self) -> bool {
        !matches!(self, Self::NotInstalled)
    }

    fn is_installed(&self) -> bool {
        matches!(self, Self::Installed(_))
    }

    fn loan(&mut self) -> Option<S> {
        match std::mem::replace(self, Self::LoanedToFinalizer) {
            Self::Installed(slot) => Some(slot),
            other => {
                *self = other;
                None
            }
        }
    }

    fn restore(&mut self, slot: S) {
        *self = Self::Installed(slot);
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[path = "checked_owner_host_facts.rs"]
pub(crate) mod checked_owner_host_facts;
pub(crate) use checked_owner_host_facts::*;

#[cfg_attr(not(test), allow(dead_code))]
#[path = "checked_owner_observation.rs"]
mod owner_observation;
use owner_observation::ObservedOwnerState;

struct OwnerState {
    stage: OwnerLifecycleStage,
    artifact: Option<Arc<MatchedRunnerArtifact>>,
    diagnostic: Option<crate::WindowsStartDiagnostic>,
    process: Option<Arc<OwnedHandle>>,
    identity: Option<OriginalProcessIdentity>,
    identity_failure: Option<HostIdentityFailureRecord>,
    runner_wait_and_exit_fact: HostOriginalRunnerProcessWaitAndExitFact,
    runner_process_handle_released_by_drop: bool,
    thread_disposition: Option<OriginalThreadDisposition>,
    thread_close_debt: Option<OriginalThreadCloseDebt>,
    startup_pipe_write: Option<File>,
    startup_pipe_read: Option<File>,
    reader_pipe_retained: bool,
    reader_pipe_released_by_drop: bool,
    writer_pipe_retained: bool,
    writer_pipe_released_by_drop: bool,
    hello: Option<ControlHello>,
    capabilities: RunnerControlCapabilities,
    negotiated_handshake: Option<codex_utils_pty::RunnerHandshakeHistory>,
    helper_attempts: [Option<HelperAttemptRecord>; 2],
    no_replay_seal: bool,
    sticky_unknown: Option<StickyUnknownReason>,
    first_error_fact: Option<HostFirstStickyFailureFact>,
    owner_mutex_poison_observed: bool,
    controls_correlation_poison_observed: bool,
    late_physical_reap_after_sticky_unknown: bool,
    actor_adopted: bool,
    partial_actor_startup_aborted: bool,
    protocol_exit_code: Option<i32>,
    reader_physical_eof: bool,
    reader_pipe_error_fact: Option<crate::elevated::native_close_protocol::PipeEofFact>,
    reader_eof_ordering: HostReaderEofOrderingFact,
    controls_and_writer_facts: HostPendingControlsAndWriterFacts,
    native_process_exit_code: Option<u32>,
    v8_closed: [bool;7],
    native_desktop_closed: bool,
    native_desktop_close_debt: Option<i32>,
    native_desktop_debt_target:Option<super::native_close_protocol::CloseDebtTarget>,
    v8_close_debts: [Option<(usize,u32)>;7],
    v8_first_close_debt: Option<(usize,u32)>,
    reader_thread_anchor: Option<Arc<OwnedHandle>>,
    reader_thread_anchor_released_by_drop: bool,
    writer_thread_anchor: Option<Arc<OwnedHandle>>,
    writer_thread_anchor_released_by_drop: bool,
    reader_worker: WorkerSlotState<StdWorkerSlot<ReaderWorkerOutcome>>,
    writer_worker: WorkerSlotState<StdWorkerSlot<WriterWorkerOutcome>>,
    input_worker: WorkerSlotState<AsyncInputSlot>,
    reader_join_timed_out: bool,
    writer_join_timed_out: bool,
    input_join_timed_out: bool,
    last_reader_outcome: Option<HostReaderOutcomeSummary>,
    last_writer_outcome: Option<HostWriterOutcomeSummary>,
    last_input_outcome_clean: Option<bool>,
    writer_wake: Option<Arc<dyn Fn() + Send + Sync>>,
    worker_admission_active: bool,
    worker_admission_fact: HostWorkerAdmissionFact,
    local_connect_lane_fact: HostLocalConnectLaneFact,
    local_registry_slot_fact: Option<HostLocalRegistrySlotFact>,
    finalization_in_progress: bool,
    finalizer_epoch: u64,
    finalized_outcome: Option<CheckedFinalizationOutcome>,
}

impl OwnerState {
    fn latch_sticky_unknown(&mut self, reason: StickyUnknownReason) {
        self.no_replay_seal = true;
        if self.sticky_unknown.is_none() {
            if self.first_error_fact.is_none() {
                self.first_error_fact =
                    Some(classify_sticky_unknown_first_error(&reason, self.stage));
            }
            self.sticky_unknown = Some(reason);
        } else if self.first_error_fact.is_none()
            && let Some(ref existing) = self.sticky_unknown
        {
            self.first_error_fact =
                Some(classify_sticky_unknown_first_error(existing, self.stage));
        }
        if let Some(CheckedFinalizationOutcome::HostNativeClosedOuterScopePending {startup_id,native_exit_code,..})
            =self.finalized_outcome.clone() {
            // Physical native closure remains historical, but a later outer
            // failure can never keep returning a cached clean/pending result.
            self.finalized_outcome=Some(CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
                startup_id,native_exit_code:Some(native_exit_code),
                reason:self.sticky_unknown.clone().unwrap(),no_replay_sealed:true,release_eligible:false,
            });
            self.stage=OwnerLifecycleStage::PhysicallyReapedStickyUnknown;
        }
    }
}

#[path="checked_owner_forwarders.rs"]
mod native_forwarders;

/// Exclusive, unwind-safe finalization ownership claim guard across sync `finalize`,
/// async `finalize_async`, and opportunistic `try_poll_input_worker_now`.
///
/// Never holds `owner.state` or `registry.inner` across I/O, native waits/joins, task
/// polling, or callbacks. Any worker-slot loan guard borrows `&mut FinalizerClaimGuard`
/// so loaned slots are always restored to `WorkerSlotState::Installed` before this
/// guard releases `finalization_in_progress` and notifies waiting callers.
pub(crate) struct FinalizerClaimGuard<'a> {
    owner: &'a CheckedRunnerOwner,
}

impl Drop for FinalizerClaimGuard<'_> {
    fn drop(&mut self) {
        let next_epoch = {
            let mut state = self
                .owner
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            state.finalization_in_progress = false;
            state.finalizer_epoch = state.finalizer_epoch.wrapping_add(1);
            state.finalizer_epoch
        };
        let _ = self.owner.finalizer_watch.send(next_epoch);
        self.owner.finalizer_cv.notify_all();
    }
}

struct ReaderSlotLoanGuard<'a, 'b> {
    claim: &'a mut FinalizerClaimGuard<'b>,
    slot: Option<StdWorkerSlot<ReaderWorkerOutcome>>,
}

impl<'a, 'b> ReaderSlotLoanGuard<'a, 'b> {
    fn new(claim: &'a mut FinalizerClaimGuard<'b>) -> io::Result<Self> {
        let mut state = claim
            .owner
            .state
            .lock()
            .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
        let slot = state.reader_worker.loan();
        Ok(Self { claim, slot })
    }
}

impl Drop for ReaderSlotLoanGuard<'_, '_> {
    fn drop(&mut self) {
        if let Some(slot) = self.slot.take() {
            let mut state = self
                .claim
                .owner
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            state.reader_worker.restore(slot);
        }
    }
}

struct WriterSlotLoanGuard<'a, 'b> {
    claim: &'a mut FinalizerClaimGuard<'b>,
    slot: Option<StdWorkerSlot<WriterWorkerOutcome>>,
}

impl<'a, 'b> WriterSlotLoanGuard<'a, 'b> {
    fn new(claim: &'a mut FinalizerClaimGuard<'b>) -> io::Result<Self> {
        let mut state = claim
            .owner
            .state
            .lock()
            .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
        let slot = state.writer_worker.loan();
        Ok(Self { claim, slot })
    }
}

impl Drop for WriterSlotLoanGuard<'_, '_> {
    fn drop(&mut self) {
        if let Some(slot) = self.slot.take() {
            let mut state = self
                .claim
                .owner
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            state.writer_worker.restore(slot);
        }
    }
}

struct InputSlotLoanGuard<'a, 'b> {
    claim: &'a mut FinalizerClaimGuard<'b>,
    slot: Option<AsyncInputSlot>,
}

impl<'a, 'b> InputSlotLoanGuard<'a, 'b> {
    fn new(claim: &'a mut FinalizerClaimGuard<'b>) -> io::Result<Self> {
        let mut state = claim
            .owner
            .state
            .lock()
            .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
        let slot = state.input_worker.loan();
        Ok(Self { claim, slot })
    }
}

impl Drop for InputSlotLoanGuard<'_, '_> {
    fn drop(&mut self) {
        if let Some(slot) = self.slot.take() {
            let mut state = self
                .claim
                .owner
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            state.input_worker.restore(slot);
        }
    }
}

/// Owns a fixed set of pre-admitted worker slots. Spawners run outside owner locks.
pub(crate) struct RuntimeWorkerAdmission<'a> {
    owner: &'a CheckedRunnerOwner,
    reader: bool,
    writer: bool,
    input: bool,
}

impl RuntimeWorkerAdmission<'_> {
    fn lock_for_install(&self) -> owner_observation::StateGuard<'_> {
        match self.owner.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.latch_sticky_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
                state.stage = OwnerLifecycleStage::FailedOrAbandoned;
                state
            }
        }
    }

    pub(crate) fn spawn_writer<F>(&mut self, spawn: F,
        wake: Arc<dyn Fn() + Send + Sync>) -> io::Result<()>
    where F: FnOnce() -> io::Result<StdJoinHandle<WriterWorkerOutcome>> {
        if self.writer { return Err(io::Error::new(io::ErrorKind::AlreadyExists, "writer slot used")); }
        if self.owner.is_no_replay_sealed() {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "owner sealed before worker effect"));
        }
        let worker = spawn()?;
        self.install_writer_worker(worker, wake);
        Ok(())
    }

    pub(crate) fn spawn_reader<F>(&mut self, spawn: F) -> io::Result<()>
    where F: FnOnce() -> io::Result<StdJoinHandle<ReaderWorkerOutcome>> {
        if self.reader { return Err(io::Error::new(io::ErrorKind::AlreadyExists, "reader slot used")); }
        if self.owner.is_no_replay_sealed() {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "owner sealed before worker effect"));
        }
        let worker = spawn()?;
        self.install_reader_worker(worker);
        Ok(())
    }

    pub(crate) fn spawn_input<F>(&mut self, spawn: F) -> io::Result<()>
    where F: FnOnce() -> TokioJoinHandle<InputWorkerOutcome> {
        if self.input { return Err(io::Error::new(io::ErrorKind::AlreadyExists, "input slot used")); }
        if self.owner.is_no_replay_sealed() {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "owner sealed before worker effect"));
        }
        let worker = spawn();
        self.install_input_worker(worker);
        Ok(())
    }

    fn install_writer_worker(
        &mut self,
        writer: StdJoinHandle<WriterWorkerOutcome>,
        writer_wake: Arc<dyn Fn() + Send + Sync>,
    ) {
        self.writer = true;
        let writer_anchor = duplicate_thread_handle_anchor(&writer);
        {
            let mut state = self.lock_for_install();
            state.writer_thread_anchor = writer_anchor;
            state.writer_worker = WorkerSlotState::Installed(StdWorkerSlot::Pending(writer));
            state.writer_wake = Some(writer_wake);
            state.writer_pipe_retained = true;
            state.startup_pipe_write = None;
        }
    }

    fn install_input_worker(&mut self, input: TokioJoinHandle<InputWorkerOutcome>) {
        self.input = true;
        {
            let mut state = self.lock_for_install();
            state.input_worker = WorkerSlotState::Installed(AsyncInputSlot::Pending(input));
        }
    }

    fn install_reader_worker(&mut self, reader: StdJoinHandle<ReaderWorkerOutcome>) {
        self.reader = true;
        let reader_anchor = duplicate_thread_handle_anchor(&reader);
        {
            let mut state = self.lock_for_install();
            state.reader_thread_anchor = reader_anchor;
            state.reader_worker = WorkerSlotState::Installed(StdWorkerSlot::Pending(reader));
            state.reader_pipe_retained = true;
            state.startup_pipe_read = None;
        }
    }


    #[cfg(test)]
    pub(crate) fn install_runtime_workers(&mut self,
        reader: StdJoinHandle<ReaderWorkerOutcome>, writer: StdJoinHandle<WriterWorkerOutcome>,
        input: TokioJoinHandle<InputWorkerOutcome>, wake: Box<dyn Fn() + Send + Sync>) {
        self.install_reader_worker(reader);
        self.install_writer_worker(writer, Arc::from(wake));
        self.install_input_worker(input);
    }
}

impl Drop for RuntimeWorkerAdmission<'_> {
    fn drop(&mut self) {
        let mut state = self.lock_for_install();
        state.reader_worker.release_reservation();
        state.writer_worker.release_reservation();
        state.input_worker.release_reservation();
        state.worker_admission_active = false;
        if self.reader
            && self.writer
            && self.input
            && !state.partial_actor_startup_aborted
            && !state.no_replay_seal
            && state.sticky_unknown.is_none()
        {
            state.worker_admission_fact =
                HostWorkerAdmissionFact::RetiredAllThreeWorkersInstalledAndCommitted;
        } else {
            state.worker_admission_fact =
                HostWorkerAdmissionFact::RetiredAfterPartialAbortOrSeal {
                    reader_installed: self.reader,
                    writer_installed: self.writer,
                    input_installed: self.input,
                };
        }
    }
}

trait ReservedWorkerSlot { fn release_reservation(&mut self); }
impl<S> ReservedWorkerSlot for WorkerSlotState<S> {
    fn release_reservation(&mut self) {
        if matches!(self, Self::ReservedForInstall) { *self = Self::NotInstalled; }
    }
}

pub(crate) struct CheckedRunnerOwner {
    startup_id: StartupId,
    startup_deadline: Instant,
    connect_owner: &'static owned_connect_worker::Owner,
    registry: Option<&'static CheckedOwnerRegistry>,
    state: ObservedOwnerState,
    native_close_runtime: std::sync::OnceLock<Arc<super::native_close_runtime::NativeCloseRuntime>>,
    native_startup_writer: std::sync::OnceLock<super::native_close_startup::NativeStartupWriter>,
    native_startup_reader: std::sync::OnceLock<super::native_close_startup::NativeStartupReader>,
    native_desktop: std::sync::OnceLock<Arc<Mutex<crate::desktop::OwnedLaunchDesktop>>>,
    // A0-only fixed companion on THIS admission, never a business-native fact.
    pub(crate) station_broker: std::sync::OnceLock<crate::station_broker::BrokerCell>,
    asuser_a1: std::sync::OnceLock<crate::asuser_a1_native::Cell>,
    native_desktop_claimed: std::sync::atomic::AtomicBool,
    native_forwarders: Mutex<native_forwarders::NativeForwarderRetention>,
    pub(super) native_completion_taken: std::sync::atomic::AtomicBool,
    native_local_retired:std::sync::atomic::AtomicBool,
    finalizer_cv: Condvar,
    finalizer_watch: tokio::sync::watch::Sender<u64>,
}

impl fmt::Debug for CheckedRunnerOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CheckedRunnerOwner")
            .field("startup_id", &self.startup_id)
            .finish_non_exhaustive()
    }
}

impl CheckedRunnerOwner {
    /// Reserve finite A1 slots on this original admission before any token/native
    /// allocation. Does not mint provenance or authorize an A1 login by itself.
    fn reserve_asuser_a1(&self)->io::Result<()> {
        {
            let state=self.state.try_lock().map_err(|_|io::Error::new(io::ErrorKind::WouldBlock,"A1 owner busy or poisoned"))?;
            if state.no_replay_seal||state.sticky_unknown.is_some()||state.process.is_some()
                ||state.actor_adopted||self.station_broker.get().is_some()
                ||!state.artifact.as_ref().is_some_and(|pin|pin.native_close_v8()) {
                return Err(io::Error::other("A1 requires original unused v8 reservation"));
            }
        }
        // Allocation occurs outside owner/registry locks. This is only a finite
        // local companion; it cannot acquire another lane or execution slot.
        let cell=crate::asuser_a1_native::Cell::new(self.startup_id.0,self.startup_deadline);
        self.asuser_a1.set(cell).map_err(|_|io::Error::other("A1 already reserved; no replay"))
    }
    pub(crate) fn has_asuser_a1(&self)->bool {self.asuser_a1.get().is_some()}
    pub(crate) fn take_asuser_origin_retirement(&self)->io::Result<Option<crate::AsUserOriginRetirement>> {
        let Some(cell)=self.asuser_a1.get() else{return Ok(None);};
        // This is a one-time post-native retirement, not startup admission.
        // Do not reuse the expired startup deadline after arbitrary business.
        if !self.native_local_retired.load(std::sync::atomic::Ordering::Acquire)
            ||self.is_sticky_unknown()||self.is_no_replay_sealed(){return Err(io::Error::other("A1 local retirement not proven"));}
        cell.with(|original|original.take_origin_retirement()).map(Some)
    }
    pub(crate) fn with_asuser_a1<T>(&self,f:impl FnOnce(&mut crate::asuser_a1_native::Attempt)->io::Result<T>)->io::Result<T> {
        self.with_asuser_a1_observed(None,f)
    }
    pub(crate) fn with_asuser_a1_observed<T>(&self,diagnostic:Option<&crate::WindowsStartDiagnostic>,
        f:impl FnOnce(&mut crate::asuser_a1_native::Attempt)->io::Result<T>)->io::Result<T> {
        self.ensure_can_advance()?;
        let result=self.asuser_a1.get().ok_or_else(||io::Error::other("A1 original reservation missing"))?.with_observed(diagnostic,f);
        if result.is_err(){self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);}
        result
    }
    fn check_asuser_native_adopted(&self)->io::Result<()> {
        if let Some(cell)=self.asuser_a1.get() {
            let result=cell.with(|original|if original.native_adopted(){Ok(())}
                else{Err(io::Error::other("A1 original native transfer not proven"))});
            if result.is_err(){self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);}
            result?;
        }
        Ok(())
    }

    pub(crate) fn check_station_broker_owner(&self)->io::Result<()> {
        let state=self.state.try_lock().map_err(|_|io::Error::new(io::ErrorKind::WouldBlock,"A0 owner busy or poisoned"))?;
        if state.no_replay_seal || state.sticky_unknown.is_some() || state.process.is_some()
            || state.actor_adopted || self.station_broker.get().is_none() || self.asuser_a1.get().is_some() {
            return Err(io::Error::other("A0 original owner unavailable or sealed"));
        }
        Ok(())
    }
    /// One synchronous bounded query from the SAME anchored HPROCESS. Dedicated
    /// desktop resource lock only; no state/registry/Core guard crosses Win32.
    pub(crate) fn observe_startup_context(&self, diagnostic: Option<&crate::WindowsStartDiagnostic>) {
        self.observe_startup_context_with_sample(diagnostic,
            crate::start_diagnostic::ContextFact::NotObserved(
                crate::start_diagnostic::ContextUnavailable::NotAttempted));
    }
    pub(crate) fn observe_startup_context_with_sample(&self, diagnostic: Option<&crate::WindowsStartDiagnostic>,
        before:crate::start_diagnostic::ContextFact<crate::WindowsStartRunnerObservation>) {
        use crate::start_diagnostic::ContextUnavailable as U;
        let unavailable=|reason| {
            if let Some(diagnostic)=diagnostic {
                diagnostic.startup_context_unavailable(self.startup_id.0,reason);
            }
        };
        let original=match self.state.try_lock() {
            Ok(state)=>{
                if !state.artifact.as_ref().is_some_and(|pin|pin.native_close_v8()) {return;}
                state.process.as_ref().zip(state.identity)
                    .map(|(process,identity)|(Arc::clone(process),identity))
            }
            Err(std::sync::TryLockError::WouldBlock)=>{unavailable(U::Busy);return;}
            Err(std::sync::TryLockError::Poisoned(error))=>{drop(error);unavailable(U::Poisoned);return;}
        };
        let Some(original)=original else {unavailable(U::OriginalUnavailable);return;};
        let Some(desktop)=self.native_desktop.get() else {unavailable(U::OriginalUnavailable);return;};
        let (context,unresolved)=match desktop.try_lock() {
            Ok(mut desktop)=>{
                let context=desktop.observe_original_token_context_with_sample(original.0.as_raw_handle() as HANDLE,
                    self.startup_id.0,original.1.pid,original.1.creation_filetime,before);
                (context,desktop.has_unresolved_startup_queries())
            }
            Err(std::sync::TryLockError::WouldBlock)=>{unavailable(U::Busy);return;}
            Err(std::sync::TryLockError::Poisoned(error))=>{drop(error);unavailable(U::Poisoned);return;}
        };
        drop(original);
        if let Some(diagnostic)=diagnostic {diagnostic.observe_startup_context(context);}
        if unresolved {
            // Query unavailability is diagnostic only; a real new close debt is
            // an actual ownership failure and cannot permit normal release.
            self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
        }
    }
    /// Original owner pin/identity are copied under the lock; OS observation is
    /// outside it. No reopen, duplication, blocking wait, join/finalize or close.
    pub(crate) fn observe_startup_runner(&self, diagnostic: &crate::WindowsStartDiagnostic) {
        let _=self.sample_startup_runner_for_context(Some(diagnostic));
    }
    /// Return this actual pre-query observation directly; never reconstruct it
    /// from the recorder that later connect watches can update.
    pub(crate) fn sample_startup_runner_for_context(&self, diagnostic:Option<&crate::WindowsStartDiagnostic>)
        ->crate::start_diagnostic::ContextFact<crate::WindowsStartRunnerObservation> {
        use crate::start_diagnostic::{ContextFact as F,ContextUnavailable as U};
        let original=match self.state.try_lock() {
            Ok(state)=>{
                if !state.artifact.as_ref().is_some_and(|pin|pin.native_close_v8()) {
                    return F::NotObserved(U::NotApplicable);
                }
                state.process.as_ref().zip(state.identity).map(|(p,i)|(Arc::clone(p),i))
            }
            Err(std::sync::TryLockError::WouldBlock)=>return F::NotObserved(U::Busy),
            Err(std::sync::TryLockError::Poisoned(error))=>{drop(error);return F::NotObserved(U::Poisoned);}
        };
        let Some((process,identity))=original else {return F::NotObserved(U::OriginalUnavailable);};
        let observed=identity.observe_startup_process(&process,self.startup_id);
        if let Some(diagnostic)=diagnostic {diagnostic.observe_original_runner(observed);}
        F::Observed(observed)
    }

    fn new(
        startup_id: StartupId,
        startup_deadline: Instant,
        artifact: Option<Arc<MatchedRunnerArtifact>>,
        diagnostic: Option<crate::WindowsStartDiagnostic>,
        connect_owner: &'static owned_connect_worker::Owner,
        registry: Option<&'static CheckedOwnerRegistry>,
    ) -> Self {
        let (finalizer_watch, _) = tokio::sync::watch::channel(0u64);
        let has_local_registry = registry.is_some();
        Self {
            startup_id,
            startup_deadline,
            connect_owner,
            registry,
            state: ObservedOwnerState::new(OwnerState {
                stage: OwnerLifecycleStage::ReservedBeforeLaunch,
                artifact,
                diagnostic,
                process: None,
                identity: None,
                identity_failure: None,
                runner_wait_and_exit_fact:
                    HostOriginalRunnerProcessWaitAndExitFact::NoProcessHandlePresent,
                runner_process_handle_released_by_drop: false,
                thread_disposition: None,
                thread_close_debt: None,
                startup_pipe_write: None,
                startup_pipe_read: None,
                reader_pipe_retained: false,
                reader_pipe_released_by_drop: false,
                writer_pipe_retained: false,
                writer_pipe_released_by_drop: false,
                hello: None,
                capabilities: RunnerControlCapabilities::default(),
                negotiated_handshake: None,
                helper_attempts: [None, None],
                no_replay_seal: false,
                sticky_unknown: None,
                first_error_fact: None,
                owner_mutex_poison_observed: false,
                controls_correlation_poison_observed: false,
                late_physical_reap_after_sticky_unknown: false,
                actor_adopted: false,
                partial_actor_startup_aborted: false,
                protocol_exit_code: None,
                reader_physical_eof: false,
                reader_pipe_error_fact: None,
                reader_eof_ordering: HostReaderEofOrderingFact::NotObserved,
                controls_and_writer_facts: HostPendingControlsAndWriterFacts::default(),
                native_process_exit_code: None,
                v8_closed: [false;7],v8_close_debts:[None;7],v8_first_close_debt:None,
                native_desktop_closed:false,native_desktop_close_debt:None,
                native_desktop_debt_target:None,
                reader_thread_anchor: None,
                reader_thread_anchor_released_by_drop: false,
                writer_thread_anchor: None,
                writer_thread_anchor_released_by_drop: false,
                reader_worker: WorkerSlotState::NotInstalled,
                writer_worker: WorkerSlotState::NotInstalled,
                input_worker: WorkerSlotState::NotInstalled,
                reader_join_timed_out: false,
                writer_join_timed_out: false,
                input_join_timed_out: false,
                last_reader_outcome: None,
                last_writer_outcome: None,
                last_input_outcome_clean: None,
                writer_wake: None,
                worker_admission_active: false,
                worker_admission_fact: HostWorkerAdmissionFact::NotReserved,
                local_connect_lane_fact: if has_local_registry {
                    HostLocalConnectLaneFact::ReservedActive
                } else {
                    HostLocalConnectLaneFact::NotReservedScopedOrUnbound
                },
                local_registry_slot_fact: registry.map(|_| {
                    HostLocalRegistrySlotFact::RetainedActiveInLocalRegistry {
                        startup_id: startup_id.0,
                    }
                }),
                finalization_in_progress: false,
                finalizer_epoch: 0,
                finalized_outcome: None,
            }, startup_id, has_local_registry),
            native_close_runtime: std::sync::OnceLock::new(),
            native_startup_writer: std::sync::OnceLock::new(),
            native_startup_reader: std::sync::OnceLock::new(),
            native_desktop: std::sync::OnceLock::new(),
            station_broker: std::sync::OnceLock::new(),
            asuser_a1: std::sync::OnceLock::new(),
            native_desktop_claimed: std::sync::atomic::AtomicBool::new(false),
            native_forwarders:Mutex::new(native_forwarders::NativeForwarderRetention::new()),
            native_completion_taken: std::sync::atomic::AtomicBool::new(false),
            native_local_retired:std::sync::atomic::AtomicBool::new(false),
            finalizer_cv: Condvar::new(),
            finalizer_watch,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new_scoped_for_actor(
        artifact: Arc<MatchedRunnerArtifact>,
        hello: ControlHello,
        capabilities: RunnerControlCapabilities,
        process: OwnedHandle,
        identity: OriginalProcessIdentity,
    ) -> Arc<Self> {
        let now = Instant::now();
        let deadline = now
            .checked_add(RUNNER_STARTUP_DEADLINE_WINDOW)
            .unwrap_or(now);
        let (finalizer_watch, _) = tokio::sync::watch::channel(0u64);
        Arc::new(Self {
            startup_id: StartupId(0),
            startup_deadline: deadline,
            connect_owner: &SCOPED_CONNECT_OWNER,
            registry: None,
            state: ObservedOwnerState::new(OwnerState {
                stage: OwnerLifecycleStage::ActorAdopted,
                artifact: Some(artifact),
                diagnostic: None,
                process: Some(Arc::new(process)),
                identity: Some(identity),
                identity_failure: None,
                runner_wait_and_exit_fact:
                    HostOriginalRunnerProcessWaitAndExitFact::HandleRetainedWaitNotYetAttempted,
                runner_process_handle_released_by_drop: false,
                thread_disposition: Some(OriginalThreadDisposition::ScopedSynthetic),
                thread_close_debt: None,
                startup_pipe_write: None,
                startup_pipe_read: None,
                reader_pipe_retained: false,
                reader_pipe_released_by_drop: false,
                writer_pipe_retained: false,
                writer_pipe_released_by_drop: false,
                hello: Some(hello),
                capabilities,
                negotiated_handshake: None,
                helper_attempts: [None, None],
                no_replay_seal: false,
                sticky_unknown: None,
                first_error_fact: None,
                owner_mutex_poison_observed: false,
                controls_correlation_poison_observed: false,
                late_physical_reap_after_sticky_unknown: false,
                actor_adopted: true,
                partial_actor_startup_aborted: false,
                protocol_exit_code: None,
                reader_physical_eof: false,
                reader_pipe_error_fact: None,
                reader_eof_ordering: HostReaderEofOrderingFact::NotObserved,
                controls_and_writer_facts: HostPendingControlsAndWriterFacts::default(),
                native_process_exit_code: None,
                v8_closed: [false;7],v8_close_debts:[None;7],v8_first_close_debt:None,
                native_desktop_closed:false,native_desktop_close_debt:None,
                native_desktop_debt_target:None,
                reader_thread_anchor: None,
                reader_thread_anchor_released_by_drop: false,
                writer_thread_anchor: None,
                writer_thread_anchor_released_by_drop: false,
                reader_worker: WorkerSlotState::NotInstalled,
                writer_worker: WorkerSlotState::NotInstalled,
                input_worker: WorkerSlotState::NotInstalled,
                reader_join_timed_out: false,
                writer_join_timed_out: false,
                input_join_timed_out: false,
                last_reader_outcome: None,
                last_writer_outcome: None,
                last_input_outcome_clean: None,
                writer_wake: None,
                worker_admission_active: false,
                worker_admission_fact: HostWorkerAdmissionFact::NotReserved,
                local_connect_lane_fact: HostLocalConnectLaneFact::NotReservedScopedOrUnbound,
                local_registry_slot_fact: None,
                finalization_in_progress: false,
                finalizer_epoch: 0,
                finalized_outcome: None,
            }, StartupId(0), false),
            native_close_runtime: std::sync::OnceLock::new(),
            native_startup_writer: std::sync::OnceLock::new(),
            native_startup_reader: std::sync::OnceLock::new(),
            native_desktop: std::sync::OnceLock::new(),
            station_broker: std::sync::OnceLock::new(),
            asuser_a1: std::sync::OnceLock::new(),
            native_desktop_claimed: std::sync::atomic::AtomicBool::new(false),
            native_forwarders:Mutex::new(native_forwarders::NativeForwarderRetention::new()),
            native_completion_taken: std::sync::atomic::AtomicBool::new(false),
            native_local_retired:std::sync::atomic::AtomicBool::new(false),
            finalizer_cv: Condvar::new(),
            finalizer_watch,
        })
    }

    pub(crate) fn native_close_runtime(&self)->Option<Arc<super::native_close_runtime::NativeCloseRuntime>> {
        self.native_close_runtime.get().cloned()
    }
    pub(crate) fn retain_original_forwarders(&self,originals:Vec<TokioJoinHandle<()>>) {
        self.record_runtime_lifecycle_failure();
        let mut slots=self.native_forwarders.lock().unwrap_or_else(|poison|poison.into_inner());
        assert!(originals.len()<=4 && !slots.loaned && slots.originals.iter().all(Option::is_none));
        for (slot,original) in slots.originals.iter_mut().zip(originals){*slot=Some(original);}
    }
    pub(crate) fn create_native_desktop(&self,username:&str,logs:Option<&std::path::Path>)
        ->anyhow::Result<Arc<Mutex<crate::desktop::OwnedLaunchDesktop>>> {
        self.ensure_can_advance()?;
        if self.native_desktop_claimed.swap(true,std::sync::atomic::Ordering::AcqRel) {
            return Err(io::Error::other("original native desktop already claimed; no replay").into());
        }
        // Reserve the sole slot BEFORE creating any desktop. Only this method
        // initializes the OnceLock, so no allocated original can be rejected by
        // a later duplicate setter or dropped out of its retention owner.
        let original=Arc::clone(self.native_desktop.get_or_init(||
            Arc::new(Mutex::new(crate::desktop::OwnedLaunchDesktop::new_attempt()))));
        let created=original.lock().map_err(|_|io::Error::other("original desktop creation owner poisoned"))?.create_once(username,logs);
        if created.is_err(){self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);}
        created?;
        Ok(original)
    }
    pub(super) fn close_owned_native_desktop(&self)->io::Result<()> {
        if let Some(cell)=self.asuser_a1.get() {
            if !self.native_close_runtime.get().is_some_and(|runtime|runtime.terminal_ack_written()) {
                self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
                return Err(io::Error::other("A1 original authenticated terminal ACK absent"));
            }
            let result=cell.with(|original|original.close_gui_after_native());
            {
                let mut state=self.state.lock().map_err(|_|io::Error::other("A1 owner poisoned after GUI close"))?;
                match &result {Ok(())=>state.native_desktop_closed=true,
                    Err(error)=>if state.native_desktop_close_debt.is_none(){state.native_desktop_close_debt=Some(error.raw_os_error().unwrap_or(0));}}
            }
            if result.is_err(){self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);}return result;
        }
        let original=self.native_desktop.get().ok_or_else(||io::Error::other("owned invocation desktop not observed"))?;
        // Dedicated resource lock only: no Core/state/registry lock across OS close.
        let (result,debt_target)=match original.try_lock() {
            Ok(mut desktop)=>{let result=desktop.close_once();(result,desktop.close_debt_target())},
            Err(_)=>{self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
                return Err(io::Error::other("original desktop owner poisoned; original retained"));}
        };
        {
            let mut state=self.state.lock().map_err(|_|io::Error::other("original owner poisoned after desktop close"))?;
            match &result {
                Ok(())=>state.native_desktop_closed=true,
                Err(error)=>{if state.native_desktop_close_debt.is_none(){state.native_desktop_close_debt=Some(error.raw_os_error().unwrap_or(0));
                    state.native_desktop_debt_target=debt_target;}}
            }
        }
        if result.is_err(){self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);}
        result
    }
    pub(super) fn retire_native_local_slot(&self)->io::Result<()> {
        if self.is_sticky_unknown() || self.is_no_replay_sealed(){return Err(io::Error::other("native original owner Unknown"));}
        if let Some(cell)=self.asuser_a1.get() {
            if let Err(error)=cell.with(|original|original.retire_after_native()) {
                self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
                return Err(error);
            }
        }
        let result=self.registry.ok_or_else(||io::Error::other("original local registry absent"))?
            .retire_native_original_checked(self.startup_id);
        match result {
            Ok(())=>{
                // No fallible lock/callback follows registration commit.
                self.native_local_retired.store(true,std::sync::atomic::Ordering::Release);
                self.state.native_local_registry_retired();
                Ok(())
            },
            Err(error)=>{self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);Err(error)}
        }
    }
    pub(crate) fn prepare_native_close_runtime(&self,seed:[u8;32])->io::Result<crate::elevated::native_close_protocol::Version8WireFrame> {
        let (identity,thread_id)={
            let state=self.state.lock().map_err(|_|io::Error::other("original owner poisoned"))?;
            let identity=state.identity.ok_or_else(||io::Error::other("original runner identity absent"))?;
            let Some(OriginalThreadDisposition::ClosedOnCapture{thread_id})=state.thread_disposition else {
                return Err(io::Error::other("original runner primary thread close unproven"));
            };(identity,thread_id)
        };
        let (runtime,bootstrap)=super::native_close_runtime::NativeCloseRuntime::host(seed,self.startup_id.0,
            super::native_close_runtime::RuntimeRunnerIdentity {process_id:identity.pid,thread_id,creation_filetime_100ns:identity.creation_filetime})?;
        self.native_close_runtime.set(Arc::clone(&runtime)).map_err(|_|io::Error::other("v8 attempt already bound; no replay"))?;
        let original={let mut state=self.state.lock().map_err(|_|io::Error::other("original owner poisoned"))?;
            state.hello=Some(runtime.control_hello());
            state.startup_pipe_write.take().ok_or_else(||io::Error::other("original pre-staged startup endpoint absent"))?
        };
        // Session binding can be installed once; exactly one pre-staged alias is
        // moved into this SAME owner before any startup writer can be spawned.
        let installed=self.native_startup_writer.set(super::native_close_startup::NativeStartupWriter::new(original));
        assert!(installed.is_ok(),"single original native startup writer owner");
        let original_read=self.state.lock().map_err(|_|io::Error::other("original owner poisoned"))?
            .startup_pipe_read.take().ok_or_else(||io::Error::other("original pre-staged read endpoint absent"))?;
        let installed=self.native_startup_reader.set(super::native_close_startup::NativeStartupReader::new(original_read));
        assert!(installed.is_ok(),"single original native startup reader owner");
        Ok(bootstrap)
    }
    pub(crate) fn write_native_startup<F>(&self,index:u8,write:F)->io::Result<()>
    where F:FnOnce(&File)->io::Result<()>+Send+'static {
        self.ensure_can_advance()?;
        self.native_startup_writer.get().ok_or_else(||io::Error::other("original startup writer owner absent"))?
            .write(index,self.startup_deadline,write)
    }
    pub(crate) fn read_native_startup<T>(&self,read:impl FnOnce(&File)->io::Result<T>)->io::Result<T> {
        self.ensure_can_advance()?;
        self.native_startup_reader.get().ok_or_else(||io::Error::other("original startup read owner absent"))?
            .read(self.startup_deadline,read)
    }
    // Consume one exact original once. Failed CloseHandle is debt, never wrapped
    // back into an owning handle, retried, or hidden by a later close.
    pub(crate) fn close_native_pipe(&self,file:File,index:usize)->io::Result<()> {
        self.close_native_original(file.into_raw_handle() as HANDLE,index)
    }
    fn close_native_original(&self,raw:HANDLE,index:usize)->io::Result<()> {
        let closed=unsafe {CloseHandle(raw)}!=0;
        let error=if closed {None}else{Some(unsafe {GetLastError()})};
        let mut state=self.state.lock().unwrap_or_else(|poison|poison.into_inner());
        if let Some(error)=error {
            state.v8_close_debts[index]=Some((raw as usize,error));
            if state.v8_first_close_debt.is_none() {state.v8_first_close_debt=Some((index,error));}
            state.latch_sticky_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
            return Err(io::Error::from_raw_os_error(error as i32));
        }
        state.v8_closed[index]=true;
        match index {0=>state.reader_pipe_retained=false,1=>state.writer_pipe_retained=false,_=>{}}
        Ok(())
    }
    fn close_native_process_and_anchors(&self)->io::Result<()> {
        for index in [2usize,5,6] {
            let original={let mut state=self.state.lock().map_err(|_|io::Error::other("original owner poisoned"))?;
                match index {2=>state.process.take(),5=>state.reader_thread_anchor.take(),_=>state.writer_thread_anchor.take()}};
            let Some(original)=original else {continue};
            let original=match Arc::try_unwrap(original) {
                Ok(original)=>original,
                Err(original)=>{
                    let mut state=self.state.lock().unwrap_or_else(|poison|poison.into_inner());
                    match index {2=>state.process=Some(original),5=>state.reader_thread_anchor=Some(original),_=>state.writer_thread_anchor=Some(original)}
                    state.latch_sticky_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
                    return Err(io::Error::new(io::ErrorKind::WouldBlock,"same original native handle still loaned/aliased"));
                }
            };
            self.close_native_original(original.into_raw_handle() as HANDLE,index)?;
        }Ok(())
    }
    pub(crate) fn retire_native_startup_aliases(&self)->io::Result<()> {
        let write=self.native_startup_writer.get().ok_or_else(||io::Error::other("original native startup owner absent"))?
            .take_original_after_both_joins()?;
        self.close_native_pipe(write,3)?;
        // Do not loan/remove the next original until the first checked close
        // succeeded. On Err/unwind this read alias remains owned in State.
        let read=self.native_startup_reader.get().ok_or_else(||io::Error::other("original startup reader owner absent"))?.take_original()?;
        self.close_native_pipe(read,4)?;
        Ok(())
    }
    pub(crate) fn runner_observation(&self) -> codex_utils_pty::RunnerObservationReader {
        self.state.reader()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn host_native_close_facts(&self) -> HostNativeCloseFactsSnapshot {
        let mut snapshot=self.state.host_facts_snapshot();
        if self.native_local_retired.load(std::sync::atomic::Ordering::Acquire) {
            snapshot.slots_and_leases.local_owner_registry_slot=HostLocalRegistrySlotFact::ReleasedAfterPhysicalClose{startup_id:self.startup_id.0};
            snapshot.slots_and_leases.local_owner_registry_retirement=super::native_close_protocol::LeaseRetirementFact::RetiredCleanly;
        }
        snapshot
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn host_native_close_facts_reader(&self) -> HostNativeCloseFactsReader {
        self.state.host_facts_reader()
    }

    pub(crate) fn startup_id(&self) -> StartupId {
        self.startup_id
    }

    pub(crate) fn startup_deadline(&self) -> Instant {
        self.startup_deadline
    }

    pub(crate) fn is_no_replay_sealed(&self) -> bool {
        self.state
            .lock()
            .map_or(true, |state| state.no_replay_seal)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_sticky_unknown(&self) -> bool {
        self.state
            .lock()
            .map_or(true, |state| state.sticky_unknown.is_some())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn sticky_unknown_reason(&self) -> Option<StickyUnknownReason> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.sticky_unknown.clone())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn stage(&self) -> Option<OwnerLifecycleStage> {
        self.state.lock().ok().map(|state| state.stage)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn identity(&self) -> Option<OriginalProcessIdentity> {
        self.state.lock().ok().and_then(|state| state.identity)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn thread_disposition(&self) -> Option<OriginalThreadDisposition> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.thread_disposition)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn thread_close_debt(&self) -> Option<OriginalThreadCloseDebt> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.thread_close_debt)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn has_unresolved_thread_close_debt(&self) -> bool {
        self.state
            .lock()
            .map_or(true, |state| state.thread_close_debt.is_some())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn has_retained_process_handle(&self) -> bool {
        self.state
            .lock()
            .map_or(false, |state| state.process.is_some())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn has_retained_thread_anchors(&self) -> bool {
        self.state.lock().map_or(false, |state| {
            state.reader_thread_anchor.is_some() && state.writer_thread_anchor.is_some()
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn has_installed_runtime_workers(&self) -> bool {
        self.state.lock().map_or(false, |state| {
            state.reader_worker.is_installed()
                && state.writer_worker.is_installed()
                && state.input_worker.is_installed()
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_finalization_in_progress(&self) -> bool {
        self.state
            .lock()
            .map_or(false, |state| state.finalization_in_progress)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn has_retained_startup_pipes(&self) -> bool {
        self.state.lock().map_or(false, |state| {
            state.startup_pipe_write.is_some() && state.startup_pipe_read.is_some()
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn has_retained_hello(&self) -> bool {
        self.state
            .lock()
            .map_or(false, |state| state.hello.is_some())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn protocol_exit_code(&self) -> Option<i32> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.protocol_exit_code)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn reader_physical_eof_observed(&self) -> bool {
        self.state
            .lock()
            .map_or(false, |state| state.reader_physical_eof)
    }

    pub(crate) fn seal_unknown(&self, reason: StickyUnknownReason) {
        if let Ok(mut state) = self.state.lock() {
            state.latch_sticky_unknown(reason);
            if !matches!(
                state.stage,
                OwnerLifecycleStage::ActorAdopted
                    | OwnerLifecycleStage::FinalizedClean
                    | OwnerLifecycleStage::PhysicallyReapedStickyUnknown
            ) {
                state.stage = OwnerLifecycleStage::FailedOrAbandoned;
            }
        }
    }

    pub(crate) fn ensure_can_advance(&self) -> io::Result<()> {
        let state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
        if state.no_replay_seal || state.sticky_unknown.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "checked startup owner is sealed noReplay and cannot advance or replay",
            ));
        }
        if Instant::now() >= self.startup_deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "checked startup deadline expired",
            ));
        }
        Ok(())
    }

    pub(crate) fn record_runtime_lifecycle_failure(&self) {
        self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
    }
    pub(crate) fn record_native_terminal_ack_failure(&self,error:&io::Error) {
        if let Ok(mut state)=self.state.lock() {
            let first=state.first_error_fact.is_none() && state.sticky_unknown.is_none();
            state.latch_sticky_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
            if first {
                if let Some(fact)=state.first_error_fact.as_mut() {
                    fact.protocol_first_error=super::native_close_protocol::FirstErrorFact::Recorded {
                        kind:super::native_close_protocol::ProtocolStickyFailureKind::RuntimeTransportFailed,
                        stage:super::native_close_protocol::ProtocolFailureStage::InputDrainAndJoin,
                        os_error:error.raw_os_error(),
                        replay_policy:super::native_close_protocol::ReplayDisposition::NoReplaySealed,
                    };
                }
            }
        }
    }

    pub(crate) fn record_protocol_exit(&self, exit_code: i32) {
        if let Ok(mut state) = self.state.lock()
            && state.protocol_exit_code.is_none()
        {
            state.protocol_exit_code = Some(exit_code);
        }
    }

    pub(crate) fn record_reader_physical_eof(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.reader_physical_eof = true;
            if state.reader_eof_ordering == HostReaderEofOrderingFact::NotObserved {
                state.reader_eof_ordering = if state.protocol_exit_code.is_some() {
                    HostReaderEofOrderingFact::CleanPhysicalEofAfterBusinessExit
                } else {
                    HostReaderEofOrderingFact::PhysicalEofWithoutBusinessExit
                };
            }
        }
    }

    pub(crate) fn record_controls_runtime_installed(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.controls_and_writer_facts.runtime_installed = true;
            if matches!(
                state.controls_and_writer_facts.resolution_state,
                HostControlsResolutionStateFact::NoControlsRuntimeInstalled
            ) {
                state.controls_and_writer_facts.resolution_state =
                    HostControlsResolutionStateFact::ActiveOrIdleNoUnconfirmed;
            }
        }
    }

    pub(crate) fn record_controls_facts(&self, facts: HostPendingControlsAndWriterFacts) {
        if let Ok(mut state) = self.state.lock() {
            if state.controls_correlation_poison_observed {
                let mut poisoned_facts = facts;
                poisoned_facts.resolution_state =
                    HostControlsResolutionStateFact::CorrelationMutexPoisoned;
                state.controls_and_writer_facts = poisoned_facts;
            } else {
                state.controls_and_writer_facts = facts;
            }
        }
    }

    pub(crate) fn record_controls_correlation_poisoned(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.controls_correlation_poison_observed = true;
            state.controls_and_writer_facts.resolution_state =
                HostControlsResolutionStateFact::CorrelationMutexPoisoned;
            state.latch_sticky_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
        }
    }

    pub(crate) fn record_writer_pipe_dropped(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.writer_pipe_retained = false;
            state.writer_pipe_released_by_drop = true;
        }
    }

    pub(crate) fn record_reader_pipe_dropped(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.reader_pipe_retained = false;
            state.reader_pipe_released_by_drop = true;
        }
    }

    pub(crate) fn record_reader_aborted_before_start(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.reader_pipe_retained = false;
            state.reader_pipe_released_by_drop = true;
            if state.reader_eof_ordering == HostReaderEofOrderingFact::NotObserved {
                state.reader_eof_ordering = HostReaderEofOrderingFact::AbortedBeforeReaderStart;
            }
        }
    }

    pub(crate) fn record_reader_protocol_violation_after_exit(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.reader_eof_ordering =
                HostReaderEofOrderingFact::ProtocolViolationFrameAfterBusinessExit;
            state.reader_pipe_error_fact =
                Some(crate::elevated::native_close_protocol::PipeEofFact::ReadFailedBeforeEof {
                    os_error: None,
                });
            state.latch_sticky_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
        }
    }

    pub(crate) fn record_reader_frame_or_read_failure(&self, os_error: Option<i32>) {
        if let Ok(mut state) = self.state.lock() {
            if state.reader_eof_ordering == HostReaderEofOrderingFact::NotObserved {
                state.reader_eof_ordering =
                    HostReaderEofOrderingFact::ReadFailedOrInvalidFrameBeforeEof;
            }
            if state.reader_pipe_error_fact.is_none() {
                state.reader_pipe_error_fact = Some(
                    crate::elevated::native_close_protocol::PipeEofFact::ReadFailedBeforeEof {
                        os_error,
                    },
                );
            }
        }
    }

    /// Reserve all three slots before invoking any worker spawner. A reservation is
    /// exclusive even while a previously installed slot is loaned to a finalizer.
    pub(crate) fn reserve_runtime_workers(&self) -> io::Result<RuntimeWorkerAdmission<'_>> {
        let mut state = self.state.lock()
            .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
        if state.worker_admission_active || state.finalization_in_progress
            || state.finalized_outcome.is_some()
            || state.reader_worker.is_present() || state.writer_worker.is_present()
            || state.input_worker.is_present()
        {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists,
                "checked worker slots already reserved, installed or loaned"));
        }
        if state.no_replay_seal || state.sticky_unknown.is_some() {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied,
                "checked owner sealed noReplay before worker admission"));
        }
        state.worker_admission_active = true;
        state.worker_admission_fact = HostWorkerAdmissionFact::ReservationActiveInProgress;
        state.reader_worker = WorkerSlotState::ReservedForInstall;
        state.writer_worker = WorkerSlotState::ReservedForInstall;
        state.input_worker = WorkerSlotState::ReservedForInstall;
        Ok(RuntimeWorkerAdmission { owner: self, reader: false, writer: false, input: false })
    }

    pub(crate) fn abort_partial_actor_startup(&self, reason: StickyUnknownReason) {
        let wake = if let Ok(mut state) = self.state.lock() {
            state.partial_actor_startup_aborted = true;
            state.latch_sticky_unknown(reason);
            state.worker_admission_fact =
                HostWorkerAdmissionFact::RetiredAfterPartialAbortOrSeal {
                    reader_installed: state.reader_worker.is_present(),
                    writer_installed: state.writer_worker.is_present(),
                    input_installed: state.input_worker.is_present(),
                };
            state.stage = OwnerLifecycleStage::FailedOrAbandoned;
            state.writer_wake.clone()
        } else {
            None
        };
        if let Some(wake) = wake {
            wake();
        }
    }

    fn try_claim_finalizer_for_poll(&self) -> Option<FinalizerClaimGuard<'_>> {
        let mut state = self.state.lock().ok()?;
        if state.worker_admission_active || state.finalization_in_progress || state.finalized_outcome.is_some() {
            return None;
        }
        state.finalization_in_progress = true;
        Some(FinalizerClaimGuard { owner: self })
    }

    fn claim_finalizer_blocking(
        &self,
        deadline: Instant,
    ) -> io::Result<Result<FinalizerClaimGuard<'_>, CheckedFinalizationOutcome>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
        let mut waited_for_contender = false;
        loop {
            if let Some(ref outcome) = state.finalized_outcome {
                return Ok(Err(outcome.clone()));
            }
            if state.worker_admission_active {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "worker admission in progress"));
            }
            if !state.finalization_in_progress {
                if waited_for_contender && Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "checked finalization already in progress by another caller",
                    ));
                }
                state.finalization_in_progress = true;
                return Ok(Ok(FinalizerClaimGuard { owner: self }));
            }
            waited_for_contender = true;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                // Temporary contention waiting for an active finalizer is not business Unknown.
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "checked finalization already in progress by another caller",
                ));
            }
            let wait_slice = remaining.min(Duration::from_millis(10));
            let (next_state, _) = self
                .state
                .wait_timeout(&self.finalizer_cv, state, wait_slice)
                .map_err(|_| io::Error::other("checked owner finalizer wait poisoned"))?;
            state = next_state;
        }
    }

    async fn claim_finalizer_async(
        &self,
        deadline: Instant,
    ) -> io::Result<Result<FinalizerClaimGuard<'_>, CheckedFinalizationOutcome>> {
        let mut rx = self.finalizer_watch.subscribe();
        let mut waited_for_contender = false;
        loop {
            {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
                if let Some(ref outcome) = state.finalized_outcome {
                    return Ok(Err(outcome.clone()));
                }
                if state.worker_admission_active {
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, "worker admission in progress"));
                }
                if !state.finalization_in_progress {
                    if waited_for_contender && Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            "checked finalization already in progress by another caller",
                        ));
                    }
                    state.finalization_in_progress = true;
                    return Ok(Ok(FinalizerClaimGuard { owner: self }));
                }
                let _ = *rx.borrow_and_update();
            }
            waited_for_contender = true;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                if let Ok(state) = self.state.lock()
                    && let Some(ref outcome) = state.finalized_outcome
                {
                    return Ok(Err(outcome.clone()));
                }
                // Temporary contention waiting for an active finalizer is not business Unknown.
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "checked finalization already in progress by another caller",
                ));
            }
            if tokio::time::timeout(remaining, rx.changed()).await.is_err() {
                if let Ok(state) = self.state.lock()
                    && let Some(ref outcome) = state.finalized_outcome
                {
                    return Ok(Err(outcome.clone()));
                }
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "checked finalization already in progress by another caller",
                ));
            }
        }
    }

    pub(crate) fn try_poll_input_worker_now(&self) {
        let Some(mut claim) = self.try_claim_finalizer_for_poll() else {
            return;
        };
        let Ok(mut loan) = InputSlotLoanGuard::new(&mut claim) else {
            return;
        };
        if let Some(ref mut slot) = loan.slot {
            let _ = slot.poll_once();
        }
    }

    #[cfg(test)]
    pub(crate) fn replace_process_handle_for_test(&self, wrong_handle: OwnedHandle) {
        if let Ok(mut state) = self.state.lock() {
            state.process = Some(Arc::new(wrong_handle));
        }
    }

    #[cfg(test)]
    pub(crate) fn simulate_loan_unwind_for_test(&self) {
        let mut claim = self
            .claim_finalizer_blocking(Instant::now() + Duration::from_secs(1))
            .expect("claim finalizer")
            .expect("not yet finalized");
        let _reader_loan = ReaderSlotLoanGuard::new(&mut claim).expect("loan reader");

        panic!("simulated panic while worker slots are loaned to finalizer");
    }

    fn reap_helpers_outside_state_lock(&self, deadline: Instant) -> io::Result<()> {
        let pending_entries: [Option<HelperAttemptRecord>; 2] = {
            let state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
            state.helper_attempts
        };
        for (slot_idx, maybe_entry) in pending_entries.into_iter().enumerate() {
            let Some(entry) = maybe_entry else {
                continue;
            };
            if entry.acknowledged {
                continue;
            }
            let obs = self.connect_owner.observe(entry.attempt_id, deadline)?;
            match obs {
                Observation::RetainedUnknown { reason, cancel } => {
                    self.seal_unknown(StickyUnknownReason::ConnectRetainedUnknown {
                        attempt_id: entry.attempt_id,
                        reason,
                        cancel,
                    });
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "connect helper has not signaled native thread exit",
                    ));
                }
                Observation::Joined {
                    result,
                    within_deadline,
                    prior_unknown,
                } => {
                    self.connect_owner.acknowledge_joined(entry.attempt_id)?;
                    let post_ack_in_time = Instant::now() < deadline;
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
                    if let Some(ref mut recorded) = state.helper_attempts[slot_idx] {
                        recorded.acknowledged = true;
                        recorded.within_deadline = within_deadline && post_ack_in_time;
                        recorded.had_unknown = prior_unknown.is_some() || !post_ack_in_time;
                    }
                    if !within_deadline || prior_unknown.is_some() || !post_ack_in_time {
                        state.no_replay_seal = true;
                        if state.sticky_unknown.is_none() {
                            state.sticky_unknown = Some(StickyUnknownReason::LateConnectSuccess {
                                attempt_id: entry.attempt_id,
                            });
                        }
                    } else if let Err(message) = result {
                        state.no_replay_seal = true;
                        if state.sticky_unknown.is_none() {
                            state.sticky_unknown = Some(StickyUnknownReason::ConnectFailed {
                                attempt_id: entry.attempt_id,
                                message,
                            });
                        }
                    }
                }
            }
        }
        let _ = self
            .connect_owner
            .release_startup_reservation(self.startup_id);
        Ok(())
    }

    fn finalize_with_claim(
        &self,
        claim: &mut FinalizerClaimGuard<'_>,
        deadline: Instant,
        initial_in_time: bool,
    ) -> io::Result<CheckedFinalizationOutcome> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
            if let Some(ref outcome) = state.finalized_outcome {
                return Ok(outcome.clone());
            }
            if !initial_in_time {
                state.no_replay_seal = true;
                if state.sticky_unknown.is_none() {
                    state.sticky_unknown = Some(StickyUnknownReason::FinalizationDeadlineExpired);
                }
                if !matches!(
                    state.stage,
                    OwnerLifecycleStage::ActorAdopted
                        | OwnerLifecycleStage::FinalizedClean
                        | OwnerLifecycleStage::PhysicallyReapedStickyUnknown
                ) {
                    state.stage = OwnerLifecycleStage::FailedOrAbandoned;
                }
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "checked finalization deadline expired",
                ));
            }
        }

        if let Some(startup)=self.native_startup_writer.get() {
            // Same original join only; never resubmit a startup write.
            startup.reap(deadline)?;
        }
        // 1. Reap and acknowledge any connect helper debt outside self.state lock.
        self.reap_helpers_outside_state_lock(deadline)?;

        // 2. If runtime actor was started, join reader, input, and blocking writer outside self.state lock.
        let (has_runtime_workers, partial_aborted) = {
            let state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
            let has = state.actor_adopted
                || state.reader_worker.is_present()
                || state.writer_worker.is_present()
                || state.input_worker.is_present();
            (has, state.partial_actor_startup_aborted)
        };

        if has_runtime_workers {
            // 2a. Reader std thread: native signal + join + physical pipe EOF check outside self.state lock.
            {
                let mut reader_loan = ReaderSlotLoanGuard::new(claim)?;
                if let Some(ref mut reader_slot) = reader_loan.slot {
                    let (reader_outcome, within_deadline) = match reader_slot
                        .observe_and_join(deadline, "checked host reader")
                    {
                        Ok(res) => res,
                        Err(err) => {
                            drop(reader_loan);
                            self.seal_unknown(StickyUnknownReason::ReaderIncompleteOrFailed);
                            return Err(err);
                        }
                    };
                    drop(reader_loan);
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
                    if !within_deadline {
                        state.no_replay_seal = true;
                        if state.sticky_unknown.is_none() {
                            state.sticky_unknown =
                                Some(StickyUnknownReason::FinalizationDeadlineExpired);
                        }
                    }
                    match reader_outcome {
                        Ok(outcome) => {
                            if outcome.physical_eof {
                                state.reader_physical_eof = true;
                            }
                            if let Some(code) = outcome.protocol_exit
                                && state.protocol_exit_code.is_none()
                            {
                                state.protocol_exit_code = Some(code);
                            }
                            if !outcome.physical_eof {
                                state.no_replay_seal = true;
                                if state.sticky_unknown.is_none() {
                                    state.sticky_unknown =
                                        Some(StickyUnknownReason::ReaderIncompleteOrFailed);
                                }
                                if !partial_aborted {
                                    return Err(io::Error::new(
                                        io::ErrorKind::WouldBlock,
                                        "checked host reader has not observed physical pipe EOF",
                                    ));
                                }
                            }
                            if !outcome.clean {
                                state.no_replay_seal = true;
                                if state.sticky_unknown.is_none() {
                                    state.sticky_unknown =
                                        Some(StickyUnknownReason::ReaderIncompleteOrFailed);
                                }
                            }
                        }
                        Err(_) => {
                            state.no_replay_seal = true;
                            if state.sticky_unknown.is_none() {
                                state.sticky_unknown =
                                    Some(StickyUnknownReason::ReaderIncompleteOrFailed);
                            }
                            if !partial_aborted {
                                return Err(io::Error::new(
                                    io::ErrorKind::WouldBlock,
                                    "checked host reader panicked without physical pipe EOF",
                                ));
                            }
                        }
                    }
                } else if !partial_aborted {
                    drop(reader_loan);
                    self.seal_unknown(StickyUnknownReason::ReaderIncompleteOrFailed);
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "checked host reader not installed",
                    ));
                }

            }

            // 2b. Async input task: poll/wait and join outside self.state lock.
            {
                let mut input_loan = InputSlotLoanGuard::new(claim)?;
                if let Some(ref mut input_slot) = input_loan.slot {
                    let (input_outcome, within_deadline) = match input_slot
                        .observe_and_join_blocking(deadline)
                    {
                        Ok(res) => res,
                        Err(err) => {
                            drop(input_loan);
                            self.seal_unknown(StickyUnknownReason::InputTaskFailedOrAborted);
                            return Err(err);
                        }
                    };
                    drop(input_loan);
                    let mut state = self
                        .state
                        .lock()
                        .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
                    if !within_deadline {
                        state.no_replay_seal = true;
                        if state.sticky_unknown.is_none() {
                            state.sticky_unknown =
                                Some(StickyUnknownReason::FinalizationDeadlineExpired);
                        }
                    }
                    match input_outcome {
                        Ok(outcome) => {
                            if !outcome.clean {
                                state.no_replay_seal = true;
                                if state.sticky_unknown.is_none() {
                                    state.sticky_unknown =
                                        Some(StickyUnknownReason::InputTaskFailedOrAborted);
                                }
                            }
                        }
                        Err(_) => {
                            state.no_replay_seal = true;
                            if state.sticky_unknown.is_none() {
                                state.sticky_unknown =
                                    Some(StickyUnknownReason::InputTaskFailedOrAborted);
                            }
                        }
                    }
                } else if !partial_aborted {
                    drop(input_loan);
                    self.seal_unknown(StickyUnknownReason::InputTaskFailedOrAborted);
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "checked async input task not installed",
                    ));
                }

            }

            // Wake the blocking writer outside self.state lock if sticky_unknown/no_replay is set.
            let wake_cb = {
                let state = self
                    .state
                    .lock()
                    .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
                if state.sticky_unknown.is_some() || state.no_replay_seal {
                    state.writer_wake.clone()
                } else {
                    None
                }
            };
            if let Some(wake) = wake_cb {
                wake();
            }

            // 2c. Dedicated blocking pipe writer std thread: native signal + join outside self.state lock.
            let mut writer_loan = WriterSlotLoanGuard::new(claim)?;
            if let Some(ref mut writer_slot) = writer_loan.slot {
                let (writer_outcome, within_deadline) = match writer_slot
                    .observe_and_join(deadline, "checked blocking pipe writer")
                {
                    Ok(res) => res,
                    Err(err) => {
                        drop(writer_loan);
                        self.seal_unknown(StickyUnknownReason::WriterFailed);
                        return Err(err);
                    }
                };
                drop(writer_loan);
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
                if !within_deadline {
                    state.no_replay_seal = true;
                    if state.sticky_unknown.is_none() {
                        state.sticky_unknown =
                            Some(StickyUnknownReason::FinalizationDeadlineExpired);
                    }
                }
                match writer_outcome {
                    Ok(outcome) => {
                        if !outcome.clean {
                            state.no_replay_seal = true;
                            if state.sticky_unknown.is_none() {
                                state.sticky_unknown = Some(StickyUnknownReason::WriterFailed);
                            }
                        }
                    }
                    Err(_) => {
                        state.no_replay_seal = true;
                        if state.sticky_unknown.is_none() {
                            state.sticky_unknown = Some(StickyUnknownReason::WriterFailed);
                        }
                    }
                }
            } else if !partial_aborted {
                drop(writer_loan);
                self.seal_unknown(StickyUnknownReason::WriterFailed);
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "checked blocking pipe writer not installed",
                ));
            }
        }

        // 3. Original runner OS process: validate creation identity and observe native exit outside self.state lock.
        let (process_arc, maybe_identity) = {
            let state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
            (state.process.clone(), state.identity)
        };
        if let Some(process) = process_arc {
            let Some(identity) = maybe_identity else {
                self.seal_unknown(StickyUnknownReason::IdentityValidationFailed(
                    "missing original process identity".to_owned(),
                ));
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "missing original process identity",
                ));
            };
            if let Err(err) = identity.verify_same_process(&process) {
                self.seal_unknown(StickyUnknownReason::IdentityValidationFailed(
                    err.to_string(),
                ));
                return Err(err);
            }
            let pre_wait_in_time = Instant::now() < deadline;
            if !pre_wait_in_time {
                self.seal_unknown(StickyUnknownReason::FinalizationDeadlineExpired);
            }
            let wait_ms = finite_wait_ms(deadline.saturating_duration_since(Instant::now()));
            let raw = process.as_raw_handle() as HANDLE;
            let wait = unsafe { WaitForSingleObject(raw, wait_ms) };
            if wait != WAIT_OBJECT_0 {
                self.seal_unknown(StickyUnknownReason::ProcessWaitExpiredOrFailed);
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "original runner process has not signaled exit",
                ));
            }
            let post_signal_in_time = pre_wait_in_time && Instant::now() < deadline;
            if !post_signal_in_time {
                self.seal_unknown(StickyUnknownReason::FinalizationDeadlineExpired);
            }
            let mut raw_exit: u32 = 0;
            let ok = unsafe { GetExitCodeProcess(raw, &mut raw_exit) };
            if ok == 0 {
                let err = io::Error::last_os_error();
                self.seal_unknown(StickyUnknownReason::ProcessWaitExpiredOrFailed);
                return Err(err);
            }
            let post_exit_code_in_time = post_signal_in_time && Instant::now() < deadline;
            drop(process);
            let mut state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
            state.native_process_exit_code = Some(raw_exit);
            if !post_exit_code_in_time {
                state.no_replay_seal = true;
                if state.sticky_unknown.is_none() {
                    state.sticky_unknown = Some(StickyUnknownReason::FinalizationDeadlineExpired);
                }
            }
        }

        if self.native_close_runtime.get().is_some() {
            self.close_native_process_and_anchors()?;
        }
        if self.native_desktop.get().is_some() {
            self.close_owned_native_desktop()?;
        }

        // 4. All users have terminated and been joined: enforce original thread close-debt gate
        // before releasing concrete OS handles or the bounded CheckedOwnerRegistry slot.
        let outcome = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
            state.startup_pipe_write = None;
            state.startup_pipe_read = None;
            state.process = None;
            state.reader_thread_anchor = None;
            state.writer_thread_anchor = None;
            state.artifact = None;
            state.writer_wake = None;

            let post_release_in_time = initial_in_time && Instant::now() < deadline;
            if !post_release_in_time {
                state.no_replay_seal = true;
                if state.sticky_unknown.is_none() {
                    state.sticky_unknown = Some(StickyUnknownReason::FinalizationDeadlineExpired);
                }
            }

            if state.v8_close_debts.iter().any(Option::is_some) {
                state.latch_sticky_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
                return Err(io::Error::other("v8 original close debt retained; local slot remains charged"));
            }
            if let Some(debt) = state.thread_close_debt {
                state.no_replay_seal = true;
                if state.sticky_unknown.is_none() {
                    state.sticky_unknown = Some(StickyUnknownReason::ThreadCloseFailed {
                        raw_handle: debt.raw_handle,
                        thread_id: debt.thread_id,
                        os_error: debt.os_error,
                    });
                }
                state.stage = OwnerLifecycleStage::FailedOrAbandoned;
                return Err(io::Error::other(format!(
                    "cannot complete physical reap or release checked owner slot with unresolved original thread close debt (tid={}, handle=0x{:x}, os_error={})",
                    debt.thread_id, debt.raw_handle, debt.os_error
                )));
            }

            let clean = !state.no_replay_seal
                && state.sticky_unknown.is_none()
                && state.actor_adopted
                && !state.partial_actor_startup_aborted
                && state.reader_physical_eof
                && state.protocol_exit_code.is_some()
                && state.native_process_exit_code.is_some()
                && state.thread_close_debt.is_none()
                && state
                    .thread_disposition
                    .is_some_and(|d| d.is_clean_close());

            let outcome = if clean && self.native_close_runtime.get().is_some() {
                  // Actual native/IO and invocation-owned desktop completion
                  // only. SDK/factory obligations remain separate; retain this
                  // local slot until the sealed original SDK consumer retires it.
                CheckedFinalizationOutcome::HostNativeClosedOuterScopePending {
                    startup_id:self.startup_id,protocol_exit_code:state.protocol_exit_code.unwrap(),
                    native_exit_code:state.native_process_exit_code.unwrap(),release_eligible:false,
                }
            }else if clean {
                state.stage = OwnerLifecycleStage::FinalizedClean;
                CheckedFinalizationOutcome::CleanRelease {
                    startup_id: self.startup_id,
                    protocol_exit_code: state.protocol_exit_code.unwrap_or(-1),
                    native_exit_code: state.native_process_exit_code.unwrap_or(0),
                    release_eligible: RELEASE_ELIGIBLE,
                }
            } else {
                state.no_replay_seal = true;
                let reason = state
                    .sticky_unknown
                    .clone()
                    .unwrap_or(StickyUnknownReason::StartupUnwound);
                state.sticky_unknown = Some(reason.clone());
                state.stage = OwnerLifecycleStage::PhysicallyReapedStickyUnknown;
                CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
                    startup_id: self.startup_id,
                    native_exit_code: state.native_process_exit_code,
                    reason,
                    no_replay_sealed: true,
                    release_eligible: RELEASE_ELIGIBLE,
                }
            };
            state.finalized_outcome = Some(outcome.clone());
            outcome
        };

        if let Some(runtime)=self.native_close_runtime.get() {
            // No state/registry lock across this finite protocol callback. Facts
            // come from the same original owner, not decoded/public data.
            let facts=self.host_native_close_facts().to_v8_receiver_physical_close_facts();
            if let Err(error)=runtime.observe_physical_close(facts) {
                self.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
                let mut state=self.state.lock().unwrap_or_else(|poison|poison.into_inner());
                state.finalized_outcome=Some(CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
                    startup_id:self.startup_id,native_exit_code:state.native_process_exit_code,
                    reason:state.sticky_unknown.clone().unwrap_or(StickyUnknownReason::RuntimeLifecycleFailure),
                    no_replay_sealed:true,release_eligible:false,
                });
                return Err(error);
            }
            // Pending outer owner is not Unknown or global clean. No release.
            if matches!(outcome,CheckedFinalizationOutcome::HostNativeClosedOuterScopePending{..}) {
                return Ok(outcome);
            }
        }
        if self.has_asuser_a1() {
            // Physical IO/process reap, including late sticky Unknown, does
            // not discharge private T/GUI/default-template/origin obligations.
            // Keep the SAME registry anchor until the genuine native+SDK local
            // retirement consumes them. Preserve the actual outcome unchanged.
            return Ok(outcome);
        }
        if let Some(registry) = self.registry {
            registry.release_slot_after_physical_close(self.startup_id);
        }

        Ok(outcome)
    }

    /// Explicit bounded finalization/reap operation for the same original resources.
    ///
    /// Executes all native waits, joins, task polls, Condvar waits, callbacks, and
    /// registry slot releases outside `self.state` and `registry.inner` locks while
    /// keeping every original handle strongly anchored across errors and unwinds.
    pub(crate) fn finalize(&self, wait_until: Instant) -> io::Result<CheckedFinalizationOutcome> {
        if self.station_broker.get().is_some() {
            return Err(io::Error::other("A0 requires resource-only retirement; no business completion"));
        }
        self.check_asuser_native_adopted()?;
        let cap = Instant::now()
            .checked_add(CONNECT_TIMEOUT)
            .ok_or_else(|| io::Error::other("finalization deadline overflow"))?;
        let deadline = wait_until.min(cap);
        self.reap_native_forwarders_blocking(deadline)?;
        let initial_in_time = Instant::now() < deadline;

        let mut claim = match self.claim_finalizer_blocking(deadline)? {
            Ok(claim) => claim,
            Err(outcome) => return Ok(outcome),
        };

        self.finalize_with_claim(&mut claim, deadline, initial_in_time)
    }

    pub(crate) async fn finalize_async(
        &self,
        wait_until: Instant,
    ) -> io::Result<CheckedFinalizationOutcome> {
        if self.station_broker.get().is_some() {
            return Err(io::Error::other("A0 requires resource-only retirement; no business completion"));
        }
        self.check_asuser_native_adopted()?;
        let cap = Instant::now()
            .checked_add(CONNECT_TIMEOUT)
            .ok_or_else(|| io::Error::other("finalization deadline overflow"))?;
        let deadline = wait_until.min(cap);
        self.reap_native_forwarders_async(deadline).await?;
        let pre_await_in_time = Instant::now() < deadline;

        let mut claim = match self.claim_finalizer_async(deadline).await? {
            Ok(claim) => claim,
            Err(outcome) => return Ok(outcome),
        };

        if !pre_await_in_time {
            self.seal_unknown(StickyUnknownReason::FinalizationDeadlineExpired);
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "checked finalization deadline expired",
            ));
        }

        // Loan the async input slot with an RAII guard bound to `&mut claim` so dropping
        // or cancelling this future at `.await` automatically restores the original
        // JoinHandle into `self.state` before `claim` releases the finalizer gate.
        let mut input_loan = InputSlotLoanGuard::new(&mut claim)?;
        if let Some(AsyncInputSlot::Pending(ref mut handle)) = input_loan.slot {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match tokio::time::timeout(remaining, handle).await {
                Ok(res) => {
                    let within_deadline = pre_await_in_time && Instant::now() < deadline;
                    let (mapped, error_kind) = match res {
                        Ok(outcome) => (Ok(outcome), None),
                        Err(err) if err.is_cancelled() => (
                            Err("checked async input task was cancelled".to_owned()),
                            Some(HostAsyncInputJoinErrorKind::Cancelled),
                        ),
                        Err(_) => (
                            Err("checked async input task panicked".to_owned()),
                            Some(HostAsyncInputJoinErrorKind::Panicked),
                        ),
                    };
                    input_loan.slot = Some(AsyncInputSlot::Joined {
                        outcome: mapped,
                        within_deadline,
                        error_kind,
                    });
                    drop(input_loan);
                    if !within_deadline {
                        self.seal_unknown(StickyUnknownReason::FinalizationDeadlineExpired);
                    }
                }
                Err(_) => {
                    drop(input_loan);
                    self.seal_unknown(StickyUnknownReason::InputTaskFailedOrAborted);
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "checked async input task has not completed",
                    ));
                }
            }
        } else {
            drop(input_loan);
        }

        self.finalize_with_claim(&mut claim, deadline, pre_await_in_time)
    }
}

struct RegistryInner {
    next_startup_id: u64,
    slots: [Option<Arc<CheckedRunnerOwner>>; LOCAL_CHECKED_OWNER_CAPACITY],
}

pub(crate) struct CheckedOwnerRegistry {
    inner: Mutex<RegistryInner>,
}

impl CheckedOwnerRegistry {
    fn retire_native_original_checked(&self,startup_id:StartupId)->io::Result<()> {
        let mut state=self.inner.lock().map_err(|_|io::Error::other("original registry poisoned; capacity retained"))?;
        let slot=state.slots.iter_mut().find(|slot|slot.as_ref().is_some_and(|owner|owner.startup_id==startup_id))
            .ok_or_else(||io::Error::other("original registry entry absent or already consumed"))?;
        *slot=None;
        Ok(())
    }
    pub(crate) const fn new() -> Self {
        Self {
            inner: Mutex::new(RegistryInner {
                next_startup_id: 1,
                slots: [const { None }; LOCAL_CHECKED_OWNER_CAPACITY],
            }),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn active_count(&self) -> io::Result<usize> {
        let guard = self
            .inner
            .try_lock()
            .map_err(|_| io::Error::other("checked owner registry unavailable"))?;
        Ok(guard.slots.iter().filter(|slot| slot.is_some()).count())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn find_by_startup_id(
        &self,
        startup_id: StartupId,
    ) -> io::Result<Arc<CheckedRunnerOwner>> {
        let guard = self
            .inner
            .try_lock()
            .map_err(|_| io::Error::other("checked owner registry unavailable"))?;
        guard
            .slots
            .iter()
            .flatten()
            .find(|owner| owner.startup_id == startup_id)
            .cloned()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "checked startup owner not found in active slots",
                )
            })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn find_by_attempt_id(
        &self,
        attempt_id: AttemptId,
    ) -> io::Result<Arc<CheckedRunnerOwner>> {
        // Snapshot the 16-slot Arc array and release `self.inner` BEFORE locking any
        // `owner.state` so registry and owner locks are never nested in either order.
        let snapshot = {
            let guard = self
                .inner
                .try_lock()
                .map_err(|_| io::Error::other("checked owner registry unavailable"))?;
            guard.slots.clone()
        };
        for owner in snapshot.into_iter().flatten() {
            let matches = owner.state.lock().map_or(false, |state| {
                state
                    .helper_attempts
                    .iter()
                    .flatten()
                    .any(|entry| entry.attempt_id == attempt_id)
            });
            if matches {
                return Ok(owner);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "checked startup owner not found for helper attempt",
        ))
    }

    fn release_slot_after_physical_close(&self, startup_id: StartupId) {
        if let Ok(mut guard) = self.inner.lock() {
            for slot in &mut guard.slots {
                if slot
                    .as_ref()
                    .is_some_and(|owner| owner.startup_id == startup_id)
                {
                    *slot = None;
                    break;
                }
            }
        }
    }
}

static SCOPED_CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();
static CHECKED_OWNER_REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn global_registry() -> &'static CheckedOwnerRegistry {
    &CHECKED_OWNER_REGISTRY
}

pub(crate) fn reserve_checked_startup_before_launch(
    artifact: Arc<MatchedRunnerArtifact>,
    diagnostic: Option<crate::WindowsStartDiagnostic>,
) -> io::Result<CheckedStartupReservation> {
    reserve_in_registry(
        &CHECKED_OWNER_REGISTRY,
        None,
        artifact,
        diagnostic,
        RUNNER_STARTUP_DEADLINE_WINDOW,
    )
}

pub(crate) fn reserve_in_registry(
    registry: &'static CheckedOwnerRegistry,
    custom_connect_owner: Option<&'static owned_connect_worker::Owner>,
    artifact: Arc<MatchedRunnerArtifact>,
    diagnostic: Option<crate::WindowsStartDiagnostic>,
    timeout_window: Duration,
) -> io::Result<CheckedStartupReservation> {
    let mut guard = registry
        .inner
        .try_lock()
        .map_err(|_| io::Error::other("checked owner registry unavailable"))?;
    let free_index = guard
        .slots
        .iter()
        .position(Option::is_none)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                "checked runner startup owner capacity exhausted",
            )
        })?;
    let startup_id = StartupId(guard.next_startup_id);
    // Reserve the connect helper lane BEFORE committing the startup or launching any process.
    if let Some(connect_owner) = custom_connect_owner {
        connect_owner.reserve_for_startup(startup_id)?;
    } else {
        owned_connect_worker::reserve_for_startup(startup_id)?;
    }
    guard.next_startup_id = match guard.next_startup_id.checked_add(1) {
        Some(next) => next,
        None => {
            if let Some(connect_owner) = custom_connect_owner {
                let _ = connect_owner.release_startup_reservation(startup_id);
            } else {
                let _ = owned_connect_worker::release_startup_reservation(startup_id);
            }
            return Err(io::Error::other("checked startup id exhausted"));
        }
    };
    let startup_deadline = match Instant::now().checked_add(timeout_window) {
        Some(deadline) => deadline,
        None => {
            if let Some(connect_owner) = custom_connect_owner {
                let _ = connect_owner.release_startup_reservation(startup_id);
            } else {
                let _ = owned_connect_worker::release_startup_reservation(startup_id);
            }
            return Err(io::Error::other("checked startup deadline overflow"));
        }
    };
    let owner = Arc::new(CheckedRunnerOwner::new(
        startup_id,
        startup_deadline,
        Some(artifact),
        diagnostic,
        custom_connect_owner.unwrap_or(&super::owned_connect_worker::CONNECT_OWNER),
        Some(registry),
    ));
    guard.slots[free_index] = Some(Arc::clone(&owner));
    Ok(CheckedStartupReservation {
        owner,
        registry,
        custom_connect_owner,
        committed_to_transport: false,
        broker_resource_only_retired: false,
    })
}

/// Caller-side anchor held across `tokio::task::spawn_blocking` in `elevated.rs`.
/// If the caller drops/cancels the `.await` or fails to adopt the returned `RunnerTransport`,
/// this guard seals `noReplay` and keeps the same `CheckedRunnerOwner` retained in the registry.
pub(crate) struct CheckedBlockingCallerGuard {
    owner: Arc<CheckedRunnerOwner>,
    acknowledged: bool,
}

impl CheckedBlockingCallerGuard {
    pub(crate) fn new(owner: Arc<CheckedRunnerOwner>) -> Self {
        Self {
            owner,
            acknowledged: false,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn startup_id(&self) -> StartupId {
        self.owner.startup_id()
    }

    pub(crate) fn acknowledge_returned(
        mut self,
        transport: &crate::runner_client::RunnerTransport,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            transport.startup_id() == Some(self.owner.startup_id()),
            "returned runner transport does not match reserved startup owner"
        );
        anyhow::ensure!(
            !self.owner.is_no_replay_sealed(),
            "returned runner transport owner is already sealed noReplay"
        );
        self.acknowledged = true;
        Ok(())
    }
}

impl Drop for CheckedBlockingCallerGuard {
    fn drop(&mut self) {
        if !self.acknowledged {
            self.owner
                .seal_unknown(StickyUnknownReason::CallerAwaitCancelled);
        }
    }
}

pub(crate) struct CheckedStartupReservation {
    owner: Arc<CheckedRunnerOwner>,
    registry: &'static CheckedOwnerRegistry,
    custom_connect_owner: Option<&'static owned_connect_worker::Owner>,
    committed_to_transport: bool,
    broker_resource_only_retired: bool,
}

impl CheckedStartupReservation {
    pub(crate) fn reserve_asuser_a1(&mut self)->io::Result<()> {
        if self.committed_to_transport||self.broker_resource_only_retired {
            return Err(io::Error::other("A1 original startup reservation already consumed"));
        }
        self.owner.reserve_asuser_a1()
    }
    /// No native calls, wait, callback or I/O while state/registry locks are held.
    /// Only A0's actual checked-closed fixed slots may release this original
    /// local admission. This does not publish a native/business completion.
    pub(crate) fn finish_station_broker_resource_only(&mut self)->io::Result<crate::StationBrokerSnapshot> {
        let mut clock=crate::station_broker::CommitClock::new();
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||self.finish_station_broker_inner(&mut clock)));
        let result=match result {
            Ok(result)=>result,
            Err(_)=>{
                if let Some(cell)=self.owner.station_broker.get() {
                    if let Ok(mut core)=cell.try_borrow(){core.admission_unwound();}
                }
                Err(io::Error::other("A0 admission transaction unwound; original retained"))
            },
        };
        if let Err(error)=&result {
            if let Some(cell)=self.owner.station_broker.get() {
                if let Ok(mut core)=cell.try_borrow() {core.admission_failed(error);}
            }
            self.owner.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);
        }
        result
    }
    fn finish_station_broker_inner(&mut self,clock:&mut crate::station_broker::CommitClock)
        ->io::Result<crate::StationBrokerSnapshot> {
        use crate::station_broker::{budget_available,take_timely,TimelyTakeError,
            StationBrokerConnectRelease as C,StationBrokerRegistryRelease as R};
        let deadline=self.owner.startup_deadline();
        if self.committed_to_transport || self.broker_resource_only_retired || self.owner.has_asuser_a1() {
            return Err(io::Error::other("A0 original reservation already consumed"));
        }
        let broker=self.owner.station_broker.get().ok_or_else(||io::Error::other("A0 unbound"))?;
        let mut core=broker.try_borrow()?;
        if !budget_available(deadline,clock) {
            core.record_registry_release(R::RetainedLate);
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        if !core.resource_only_closed() || core.snapshot().connect_release!=C::NotAttempted {
            return Err(io::Error::other("A0 original resource debt remains"));
        }
        {
            let state=self.owner.state.try_lock().map_err(|_|io::Error::other("A0 owner busy or poisoned"))?;
            if state.process.is_some() || state.identity.is_some() || state.thread_close_debt.is_some()
                || state.helper_attempts.iter().any(Option::is_some) || state.actor_adopted
                || state.worker_admission_active || state.no_replay_seal || state.sticky_unknown.is_some()
                || state.first_error_fact.is_some() || state.hello.is_some() || state.negotiated_handshake.is_some()
                || state.startup_pipe_write.is_some() || state.startup_pipe_read.is_some()
                || state.finalization_in_progress || state.finalized_outcome.is_some()
                || self.owner.native_close_runtime.get().is_some() || self.owner.native_desktop.get().is_some() {
                return Err(io::Error::other("A0 is not an isolated pristine business reservation"));
            }
        }
        if !budget_available(deadline,clock) {
            core.record_registry_release(R::RetainedLate);
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        core.record_connect_release(C::InFlight);
        let connect=self.custom_connect_owner.unwrap_or(&owned_connect_worker::CONNECT_OWNER);
        let released=connect.release_startup_reservation_strict(self.owner.startup_id(),deadline,clock);
        core.record_connect_release(released);
        if released!=C::ReleasedSameStartupId {
            if released==C::LateRetained {core.record_registry_release(R::RetainedLate);}
            return Err(io::Error::from(if released==C::LateRetained {io::ErrorKind::TimedOut}else{io::ErrorKind::WouldBlock}));
        }
        // A genuine same-ID release is now a permanent historical fact, even
        // if later admission remains charged. Never retry or label it active.
        if !budget_available(deadline,clock) {
            core.record_registry_release(R::RetainedLate);
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        let removed={
            let mut registry=match self.registry.inner.try_lock() {
                Ok(registry)=>registry,
                Err(std::sync::TryLockError::WouldBlock)=>{
                    core.record_registry_release(R::RetainedBusy);return Err(io::Error::from(io::ErrorKind::WouldBlock));
                },
                Err(std::sync::TryLockError::Poisoned(_))=>{
                    core.record_registry_release(R::RetainedPoisoned);return Err(io::Error::from(io::ErrorKind::Other));
                },
            };
            let slot=registry.slots.iter_mut().find(|slot|slot.as_ref().is_some_and(|owner|
                Arc::ptr_eq(owner,&self.owner) && owner.startup_id()==self.owner.startup_id()));
            let Some(slot)=slot else {
                core.record_registry_release(R::RetainedMismatch);return Err(io::Error::from(io::ErrorKind::NotFound));
            };
            core.record_registry_release(R::InFlight);
            match take_timely(slot,deadline,clock) {
                Ok(original)=>{
                    core.record_registry_release(R::TimelyCommitted);
                    self.broker_resource_only_retired=true;
                    original
                },
                Err(TimelyTakeError::Late)=>{
                    core.record_registry_release(R::RetainedLate);return Err(io::Error::from(io::ErrorKind::TimedOut));
                },
                Err(TimelyTakeError::Empty)=>{
                    core.record_registry_release(R::RetainedMismatch);return Err(io::Error::from(io::ErrorKind::NotFound));
                },
            }
        }; // No other admission could see the temporary take before its sample.
        drop(removed); // This reservation still anchors the SAME original Arc.
        Ok(core.snapshot())
    }
    pub(crate) fn owner(&self) -> &Arc<CheckedRunnerOwner> {
        &self.owner
    }

    pub(crate) fn startup_id(&self) -> StartupId {
        self.owner.startup_id()
    }

    pub(crate) fn startup_deadline(&self) -> Instant {
        self.owner.startup_deadline()
    }

    pub(crate) fn caller_guard(&self) -> CheckedBlockingCallerGuard {
        CheckedBlockingCallerGuard::new(Arc::clone(&self.owner))
    }

    fn release_connect_lane(&self) {
        if let Some(connect_owner) = self.custom_connect_owner {
            let _ = connect_owner.release_startup_reservation(self.owner.startup_id());
        } else {
            let _ = owned_connect_worker::release_startup_reservation(self.owner.startup_id());
        }
    }

    /// Pre-stage cloned server pipe handles and `ControlHello` in the pre-admitted owner
    /// BEFORE `CreateProcessWithLogonW` so that no fallible pipe clone is required after
    /// OS process creation.
    pub(crate) fn stage_pre_launch_context(
        &mut self,
        pipe_write: &File,
        pipe_read: &File,
        hello: ControlHello,
    ) -> io::Result<()> {
        self.owner.ensure_can_advance()?;
        let cloned_write = pipe_write.try_clone()?;
        let cloned_read = pipe_read.try_clone()?;
        let mut state = self
            .owner
            .state
            .lock()
            .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
        state.startup_pipe_write = Some(cloned_write);
        state.startup_pipe_read = Some(cloned_read);
        state.hello = Some(hello);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn install_launched_process(
        &mut self,
        process: OwnedHandle,
        expected_pid: u32,
        thread: OwnedHandle,
        thread_id: u32,
        pipe_write: &File,
        pipe_read: &File,
        hello: ControlHello,
    ) -> io::Result<(OriginalProcessIdentity, OriginalThreadDisposition)> {
        self.install_launched_process_with_cloner(
            process,
            expected_pid,
            thread,
            thread_id,
            pipe_write,
            pipe_read,
            hello,
            File::try_clone,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn install_launched_process_with_cloner<C>(
        &mut self,
        process: OwnedHandle,
        expected_pid: u32,
        thread: OwnedHandle,
        thread_id: u32,
        pipe_write: &File,
        pipe_read: &File,
        hello: ControlHello,
        clone_pipe: C,
    ) -> io::Result<(OriginalProcessIdentity, OriginalThreadDisposition)>
    where
        C: FnMut(&File) -> io::Result<File>,
    {
        self.install_launched_process_with_hooks(
            process,
            expected_pid,
            thread,
            thread_id,
            pipe_write,
            pipe_read,
            hello,
            clone_pipe,
            OriginalThreadDisposition::close_original_thread,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn install_launched_process_with_hooks<C, T>(
        &mut self,
        process: OwnedHandle,
        expected_pid: u32,
        thread: OwnedHandle,
        thread_id: u32,
        pipe_write: &File,
        pipe_read: &File,
        hello: ControlHello,
        mut clone_pipe: C,
        close_thread: T,
    ) -> io::Result<(OriginalProcessIdentity, OriginalThreadDisposition)>
    where
        C: FnMut(&File) -> io::Result<File>,
        T: FnOnce(OwnedHandle, u32) -> OriginalThreadDisposition,
    {
        // Step 1: Unconditionally anchor the launched process handle, checked thread close
        // disposition, close debt, hello, and live identity in the pre-admitted owner BEFORE
        // any cancellation check, deadline check, pipe clone, or fallible operation.
        let process_arc = Arc::new(process);
        let thread_disposition = close_thread(thread, thread_id);
        let thread_close_debt = thread_disposition.close_debt();
        let identity_result = OriginalProcessIdentity::from_live_process(&process_arc, expected_pid);

        let (already_had_process, needs_pipe_clones) = {
            let mut state = self
                .owner
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.process.is_some() {
                state.no_replay_seal = true;
                if state.thread_close_debt.is_none() {
                    state.thread_close_debt = thread_close_debt;
                }
                if state.sticky_unknown.is_none() {
                    state.sticky_unknown = Some(StickyUnknownReason::StartupUnwound);
                }
                (true, false)
            } else {
                state.stage = OwnerLifecycleStage::ProcessLaunchedInHandshake;
                state.process = Some(Arc::clone(&process_arc));
                state.thread_disposition = Some(thread_disposition);
                state.thread_close_debt = thread_close_debt;
                if let Some(debt) = thread_close_debt {
                    state.no_replay_seal = true;
                    if state.sticky_unknown.is_none() {
                        state.sticky_unknown = Some(StickyUnknownReason::ThreadCloseFailed {
                            raw_handle: debt.raw_handle,
                            thread_id: debt.thread_id,
                            os_error: debt.os_error,
                        });
                    }
                }
                state.hello = Some(hello);
                if let Ok(identity) = identity_result {
                    state.identity = Some(identity);
                }
                let needs_clones =
                    state.startup_pipe_write.is_none() || state.startup_pipe_read.is_none();
                (false, needs_clones)
            }
        };

        if already_had_process {
            self.release_connect_lane();
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "checked startup owner already has an installed process",
            ));
        }

        // Step 2: If pipes were not pre-staged before launch, clone and retain them now while
        // `state.process` is already strongly anchored in `self.owner`.
        if needs_pipe_clones {
            let write_res = clone_pipe(pipe_write);
            let read_res = match write_res {
                Ok(ref cloned_w) => {
                    let mut state = self
                        .owner
                        .state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if let Ok(dup_w) = cloned_w.try_clone() {
                        state.startup_pipe_write = Some(dup_w);
                    }
                    drop(state);
                    clone_pipe(pipe_read)
                }
                Err(err) => {
                    self.owner
                        .seal_unknown(StickyUnknownReason::HandshakeFailed(err.to_string()));
                    self.release_connect_lane();
                    return Err(err);
                }
            };
            let cloned_w = write_res.expect("checked above");
            match read_res {
                Ok(cloned_r) => {
                    let mut state = self
                        .owner
                        .state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.startup_pipe_write = Some(cloned_w);
                    state.startup_pipe_read = Some(cloned_r);
                }
                Err(err) => {
                    let mut state = self
                        .owner
                        .state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.startup_pipe_write = Some(cloned_w);
                    drop(state);
                    self.owner
                        .seal_unknown(StickyUnknownReason::HandshakeFailed(err.to_string()));
                    self.release_connect_lane();
                    return Err(err);
                }
            }
        }

        // Step 3: Validate checked thread close result, process identity, and cancellation/deadline
        // state AFTER the process and startup context are strongly anchored in `self.owner`.
        if let Some(debt) = thread_close_debt {
            self.owner
                .seal_unknown(StickyUnknownReason::ThreadCloseFailed {
                    raw_handle: debt.raw_handle,
                    thread_id: debt.thread_id,
                    os_error: debt.os_error,
                });
            self.release_connect_lane();
            return Err(io::Error::from_raw_os_error(debt.os_error as i32));
        }

        let identity = match identity_result {
            Ok(identity) => identity,
            Err(err) => {
                self.owner
                    .seal_unknown(StickyUnknownReason::IdentityValidationFailed(
                        err.to_string(),
                    ));
                self.release_connect_lane();
                return Err(err);
            }
        };

        if let Err(err) = self.owner.ensure_can_advance() {
            self.owner
                .seal_unknown(StickyUnknownReason::HandshakeFailed(err.to_string()));
            self.release_connect_lane();
            return Err(err);
        }

        Ok((identity, thread_disposition))
    }

    pub(crate) fn connect_pipe_step(
        &mut self,
        pipe: &File,
        pipe_label: &'static str,
        diagnostic: Option<&crate::WindowsStartDiagnostic>,
    ) -> anyhow::Result<AttemptId> {
        let expected_pid = self
            .owner
            .state
            .lock()
            .ok()
            .and_then(|state| state.identity.map(|identity| identity.pid))
            .unwrap_or(0);
        self.connect_pipe_step_with(pipe, pipe_label, diagnostic, move |owned_pipe| {
            crate::runner_pipe::connect_pipe(owned_pipe.as_raw_handle() as HANDLE, expected_pid)
        })
    }

    pub(crate) fn connect_pipe_step_with<F>(
        &mut self,
        pipe: &File,
        pipe_label: &'static str,
        diagnostic: Option<&crate::WindowsStartDiagnostic>,
        connect_fn: F,
    ) -> anyhow::Result<AttemptId>
    where
        F: FnOnce(&File) -> io::Result<()> + Send + 'static,
    {
        let startup_id = self.owner.startup_id();
        let deadline = self.owner.startup_deadline();
        if let Err(err) = self.owner.ensure_can_advance() {
            if let Some(diagnostic) = diagnostic {
                diagnostic.fail_io_kind(err.kind());
            }
            return Err(mark_error_non_refreshable(err.into(), startup_id));
        }

        let (runner_clone, slot_index, original_process, identity, native_watch) = {
            let state = self
                .owner
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("checked owner state lock poisoned"))?;
            let Some(ref proc_handle) = state.process else {
                return Err(mark_error_non_refreshable(
                    anyhow::anyhow!("checked connect requires an installed runner process"),
                    startup_id,
                ));
            };
            let Some(identity) = state.identity else {
                return Err(mark_error_non_refreshable(
                    anyhow::anyhow!("checked connect requires validated runner identity"),
                    startup_id,
                ));
            };
            identity.verify_same_process(proc_handle).map_err(|err| {
                if let Some(diagnostic) = diagnostic {
                    diagnostic.fail_io_kind(err.kind());
                }
                mark_error_non_refreshable(err.into(), startup_id)
            })?;
            let slot_index = state
                .helper_attempts
                .iter()
                .position(Option::is_none)
                .ok_or_else(|| {
                    mark_error_non_refreshable(
                        anyhow::anyhow!("checked startup exceeded two pipe connect attempts"),
                        startup_id,
                    )
                })?;
            let cloned = proc_handle.try_clone().map_err(|err| {
                if let Some(diagnostic) = diagnostic {
                    diagnostic.fail_io_kind(err.kind());
                }
                mark_error_non_refreshable(err.into(), startup_id)
            })?;
            (cloned, slot_index, Arc::clone(proc_handle), identity,
                state.artifact.as_ref().is_some_and(|pin| pin.native_close_v8()))
        };

        let connect_owner = self
            .custom_connect_owner
            .unwrap_or(&owned_connect_worker::CONNECT_OWNER);
        let attempt_id = connect_owner
            .begin_with_for_startup(Some(startup_id), pipe, &runner_clone, deadline, connect_fn)
            .map_err(|err| {
                if let Some(diagnostic) = diagnostic {
                    diagnostic.fail_io_kind(err.kind());
                }
                self.owner
                    .seal_unknown(StickyUnknownReason::HandshakeFailed(err.to_string()));
                self.release_connect_lane();
                mark_error_non_refreshable(err.into(), startup_id)
            })?;

        if let Ok(mut state) = self.owner.state.lock() {
            state.helper_attempts[slot_index] = Some(HelperAttemptRecord {
                attempt_id,
                pipe_label,
                acknowledged: false,
                within_deadline: false,
                had_unknown: false,
            });
        }

        let observed = if native_watch {
            connect_owner.observe_detailed_with_runner(attempt_id, deadline,
                &original_process, identity, startup_id, diagnostic)
        } else {
            connect_owner.observe_detailed(attempt_id, deadline)
        };

        let (observation, error_meta) = match observed {
            Ok(pair) => pair,
            Err(err) => {
                if let Some(diagnostic) = diagnostic {
                    diagnostic.fail_io_kind(err.kind());
                }
                self.owner
                    .seal_unknown(StickyUnknownReason::HandshakeFailed(err.to_string()));
                return Err(mark_error_non_refreshable(err.into(), startup_id));
            }
        };

        self.finish_connect_observation(
            slot_index,
            attempt_id,
            pipe_label,
            observation,
            error_meta,
            diagnostic,
        )
    }

    pub(crate) fn finish_connect_observation(
        &mut self,
        slot_index: usize,
        attempt_id: AttemptId,
        pipe_label: &'static str,
        observation: Observation,
        error_meta: Option<JoinedErrorMeta>,
        diagnostic: Option<&crate::WindowsStartDiagnostic>,
    ) -> anyhow::Result<AttemptId> {
        let startup_id = self.owner.startup_id();
        let ack_helper = |id: AttemptId| -> io::Result<()> {
            if let Some(connect_owner) = self.custom_connect_owner {
                connect_owner.acknowledge_joined(id)
            } else {
                owned_connect_worker::acknowledge_joined(id)
            }
        };

        match observation {
            Observation::Joined {
                result: Ok(()),
                within_deadline: true,
                prior_unknown: None,
            } => {
                ack_helper(attempt_id).map_err(|err| {
                    self.owner
                        .seal_unknown(StickyUnknownReason::HandshakeFailed(err.to_string()));
                    mark_error_non_refreshable(err.into(), startup_id)
                })?;
                if let Ok(mut state) = self.owner.state.lock()
                    && let Some(ref mut entry) = state.helper_attempts[slot_index]
                {
                    entry.acknowledged = true;
                    entry.within_deadline = true;
                    entry.had_unknown = false;
                }
                if self.owner.is_no_replay_sealed() {
                    self.release_connect_lane();
                    if let Some(diagnostic) = diagnostic {
                        diagnostic.fail_io_kind(io::ErrorKind::TimedOut);
                    }
                    return Err(mark_error_non_refreshable(
                        anyhow::anyhow!(
                            "checked startup owner sealed noReplay during {pipe_label} connect"
                        ),
                        startup_id,
                    ));
                }
                Ok(attempt_id)
            }
            Observation::Joined {
                result: Ok(()),
                within_deadline,
                prior_unknown,
            } => {
                let _ = ack_helper(attempt_id);
                if let Ok(mut state) = self.owner.state.lock()
                    && let Some(ref mut entry) = state.helper_attempts[slot_index]
                {
                    entry.acknowledged = true;
                    entry.within_deadline = within_deadline;
                    entry.had_unknown = prior_unknown.is_some();
                }
                self.owner
                    .seal_unknown(StickyUnknownReason::LateConnectSuccess { attempt_id });
                self.release_connect_lane();
                if let Some(diagnostic) = diagnostic {
                    diagnostic.fail_io_kind(io::ErrorKind::TimedOut);
                }
                Err(mark_error_non_refreshable(
                    anyhow::Error::from(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!(
                            "late helper completion after deadline or unknown connecting runner {pipe_label}"
                        ),
                    )),
                    startup_id,
                ))
            }
            Observation::Joined {
                result: Err(message),
                within_deadline,
                prior_unknown,
            } => {
                let _ = ack_helper(attempt_id);
                if let Ok(mut state) = self.owner.state.lock()
                    && let Some(ref mut entry) = state.helper_attempts[slot_index]
                {
                    entry.acknowledged = true;
                    entry.within_deadline = within_deadline;
                    entry.had_unknown = prior_unknown.is_some();
                }
                self.owner.seal_unknown(StickyUnknownReason::ConnectFailed {
                    attempt_id,
                    message: message.clone(),
                });
                self.release_connect_lane();
                let kind = error_meta
                    .as_ref()
                    .map_or(io::ErrorKind::Other, |meta| meta.kind);
                if let Some(diagnostic) = diagnostic {
                    diagnostic.fail_io_kind(kind);
                }
                let io_err = if let Some(raw) = error_meta.and_then(|meta| meta.raw_os_error) {
                    io::Error::from_raw_os_error(raw)
                } else {
                    io::Error::new(kind, message)
                };
                Err(mark_error_non_refreshable(
                    anyhow::Error::from(io_err).context(format!("connect {pipe_label}")),
                    startup_id,
                ))
            }
            Observation::RetainedUnknown { reason, cancel } => {
                self.owner
                    .seal_unknown(StickyUnknownReason::ConnectRetainedUnknown {
                        attempt_id,
                        reason: reason.clone(),
                        cancel,
                    });
                if let Some(diagnostic) = diagnostic {
                    match reason {
                        UnknownReason::OriginalRunnerExited => diagnostic.fail(crate::WindowsStartError::OriginalRunnerExitedBeforeConnect),
                        UnknownReason::OriginalRunnerWaitUnavailable => diagnostic.fail(crate::WindowsStartError::OriginalRunnerWaitUnavailable),
                        UnknownReason::WaitFailed(code) => {
                            diagnostic.fail_os(crate::WindowsOsError::from_code(code));
                        }
                        _ => diagnostic.fail_io_kind(io::ErrorKind::TimedOut),
                    }
                }
                let (kind, detail) = match reason {
                    UnknownReason::OriginalRunnerExited => (io::ErrorKind::BrokenPipe, format!("original runner exited before {pipe_label} connect; same owner retained; no replay")),
                    UnknownReason::OriginalRunnerWaitUnavailable => (io::ErrorKind::Other, format!("original runner wait unavailable during {pipe_label} connect; same owner retained; no replay")),
                    _ => (io::ErrorKind::TimedOut, format!("timed out after {}ms connecting runner {pipe_label}", RUNNER_STARTUP_DEADLINE_WINDOW.as_millis())),
                };
                Err(mark_error_non_refreshable(
                    anyhow::Error::from(io::Error::new(
                        kind,
                        detail,
                    )),
                    startup_id,
                ))
            }
        }
    }

    pub(crate) fn commit_transport_lease(
        &mut self,
        capabilities: RunnerControlCapabilities,
    ) -> anyhow::Result<CheckedTransportLease> {
        let startup_id = self.owner.startup_id();
        self.owner
            .ensure_can_advance()
            .map_err(|err| mark_error_non_refreshable(err.into(), startup_id))?;
        self.release_connect_lane();
        {
            let mut state = self
                .owner
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("checked owner state lock poisoned"))?;
            let Some(ref proc_handle) = state.process else {
                return Err(mark_error_non_refreshable(
                    anyhow::anyhow!("checked startup owner missing process handle"),
                    startup_id,
                ));
            };
            let Some(identity) = state.identity else {
                return Err(mark_error_non_refreshable(
                    anyhow::anyhow!("checked startup owner missing process identity"),
                    startup_id,
                ));
            };
            identity
                .verify_same_process(proc_handle)
                .map_err(|err| mark_error_non_refreshable(err.into(), startup_id))?;
            if !state
                .helper_attempts
                .iter()
                .all(|slot| slot.is_some_and(|h| h.acknowledged && h.within_deadline && !h.had_unknown))
            {
                return Err(mark_error_non_refreshable(
                    anyhow::anyhow!("checked startup requires both pipe helpers timely joined and acknowledged"),
                    startup_id,
                ));
            }
            state.capabilities = capabilities;
            // Historical projection only, after the ORIGINAL handshake/helper checks.
            // Keep schema/capabilities; never retain/export authentication nonce here.
            state.negotiated_handshake = state.hello.as_ref().map(|hello| codex_utils_pty::RunnerHandshakeHistory {
                schema: hello.schema, close_stdin: capabilities.close_stdin, resize: capabilities.resize,
            });
            state.stage = OwnerLifecycleStage::HandshakeSucceededInTransport;
        }
        self.committed_to_transport = true;
        Ok(CheckedTransportLease {
            owner: Arc::clone(&self.owner),
            extracted_to_io: false,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn into_transport_lease(
        mut self,
        capabilities: RunnerControlCapabilities,
    ) -> anyhow::Result<CheckedTransportLease> {
        self.commit_transport_lease(capabilities)
    }

    pub(crate) fn fail_after_launch(&mut self, err: anyhow::Error) -> anyhow::Error {
        let startup_id = self.owner.startup_id();
        self.owner
            .seal_unknown(StickyUnknownReason::HandshakeFailed(err.to_string()));
        self.release_connect_lane();
        mark_error_non_refreshable(err, startup_id)
    }
}

impl Drop for CheckedStartupReservation {
    fn drop(&mut self) {
        if self.committed_to_transport || self.broker_resource_only_retired {
            return;
        }
        if self.owner.has_asuser_a1() {
            // An A1 attempt may own original T, GUI/helper/IO and even partial
            // PI outputs before state.process is installed. That absence is
            // never a prelaunch release proof. Keep THIS original registry and
            // connect admission on cancellation/unwind; only the genuine native
            // plus SDK retirement path may retire the installed A1 cell.
            self.owner.seal_unknown(StickyUnknownReason::StartupUnwound);
            return;
        }
        if let Some(broker)=self.owner.station_broker.get() {
            // No prelaunch shortcut: A0 can own native objects without a
            // business runner. Preserve BOTH original lane and registry charge.
            if let Ok(mut core)=broker.try_borrow() {
                core.fail(crate::station_broker::StationBrokerDebt::Unwind,None);
            }
            self.owner.seal_unknown(StickyUnknownReason::StartupUnwound);
            return;
        }
        let can_release_pre_launch = self.owner.state.lock().map_or(false, |state| {
            state.process.is_none()
                && state.thread_close_debt.is_none()
                && state.helper_attempts.iter().all(Option::is_none)
                && !state.no_replay_seal
                && state.sticky_unknown.is_none()
        });
        self.release_connect_lane();
        if can_release_pre_launch {
            // No launched process/helper exists on this original reservation.
            // Closing its dedicated desktop still requires a real success.
            if self.owner.native_desktop.get().is_none() || self.owner.close_owned_native_desktop().is_ok() {
                self.registry.release_slot_after_physical_close(self.owner.startup_id());
            }
        } else {
            self.owner.seal_unknown(StickyUnknownReason::StartupUnwound);
        }
    }
}

/// Carried inside `RunnerTransport`. Dropping `RunnerTransport` without calling
/// `into_checked_files` seals `noReplay` and keeps the owner retained in the registry.
pub(crate) struct CheckedTransportLease {
    owner: Arc<CheckedRunnerOwner>,
    extracted_to_io: bool,
}

impl CheckedTransportLease {
    pub(crate) fn startup_id(&self) -> StartupId {
        self.owner.startup_id()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn owner(&self) -> &Arc<CheckedRunnerOwner> {
        &self.owner
    }

    pub(crate) fn reject_legacy_conversion(&self) {
        self.owner
            .seal_unknown(StickyUnknownReason::WrongLegacyConversion);
    }

    pub(crate) fn into_io_token(
        mut self,
    ) -> anyhow::Result<(
        CheckedIoOwnerToken,
        OriginalProcessIdentity,
        OriginalThreadDisposition,
    )> {
        let (identity, thread_disposition) = {
            let mut state = self
                .owner
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("checked owner state lock poisoned"))?;
            if state.no_replay_seal || state.sticky_unknown.is_some() {
                anyhow::bail!("checked runner owner is sealed noReplay");
            }
            let Some(ref process) = state.process else {
                anyhow::bail!("checked runner owner missing original process handle");
            };
            let Some(identity) = state.identity else {
                anyhow::bail!("checked runner owner missing original process identity");
            };
            let Some(thread_disposition) = state.thread_disposition else {
                anyhow::bail!("checked runner owner missing original thread disposition");
            };
            identity.verify_same_process(process)?;
            state.stage = OwnerLifecycleStage::CheckedIoExtracted;
            (identity, thread_disposition)
        };
        self.extracted_to_io = true;
        Ok((
            CheckedIoOwnerToken {
                owner: Arc::clone(&self.owner),
                adopted_by_actor: false,
            },
            identity,
            thread_disposition,
        ))
    }
}

impl Drop for CheckedTransportLease {
    fn drop(&mut self) {
        if !self.extracted_to_io {
            self.owner
                .seal_unknown(StickyUnknownReason::TransportDroppedUnadopted);
        }
    }
}

/// Carried inside `CheckedRunnerIo`. Dropping `CheckedRunnerIo` before actor adoption
/// seals `noReplay` and keeps the owner retained in the registry.
pub(crate) struct CheckedIoOwnerToken {
    owner: Arc<CheckedRunnerOwner>,
    adopted_by_actor: bool,
}

impl CheckedIoOwnerToken {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn startup_id(&self) -> StartupId {
        self.owner.startup_id()
    }

    pub(crate) fn owner(&self) -> &Arc<CheckedRunnerOwner> {
        &self.owner
    }

    pub(crate) fn adopt_into_actor(mut self) -> io::Result<Arc<CheckedRunnerOwner>> {
        {
            let mut state = self
                .owner
                .state
                .lock()
                .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
            if state.no_replay_seal || state.sticky_unknown.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "checked runner owner is sealed noReplay before actor adoption",
                ));
            }
            let Some(ref process) = state.process else {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "checked runner owner missing original process handle",
                ));
            };
            let Some(identity) = state.identity else {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "checked runner owner missing original process identity",
                ));
            };
            if let Err(err) = identity.verify_same_process(process) {
                state.no_replay_seal = true;
                if state.sticky_unknown.is_none() {
                    state.sticky_unknown =
                        Some(StickyUnknownReason::IdentityValidationFailed(err.to_string()));
                }
                return Err(err);
            }
            state.stage = OwnerLifecycleStage::ActorAdopted;
            state.actor_adopted = true;
        }
        self.adopted_by_actor = true;
        Ok(Arc::clone(&self.owner))
    }
}

impl Drop for CheckedIoOwnerToken {
    fn drop(&mut self) {
        if !self.adopted_by_actor {
            self.owner
                .seal_unknown(StickyUnknownReason::CheckedIoDroppedUnadopted);
        }
    }
}

/// Private named checked runner handoff bundle replacing the former 5-tuple.
/// Carries both pipes, negotiated `ControlHello` and `RunnerControlCapabilities`,
/// the independent `MatchedRunnerArtifact`, validated `OriginalProcessIdentity`,
/// explicit `OriginalThreadDisposition`, and the same `CheckedIoOwnerToken`
/// holding the original `pi.hProcess` `OwnedHandle`.
pub(crate) struct CheckedRunnerIo {
    pub(crate) pipe_write: File,
    pub(crate) pipe_read: File,
    pub(crate) hello: ControlHello,
    pub(crate) capabilities: RunnerControlCapabilities,
    pub(crate) artifact: Arc<MatchedRunnerArtifact>,
    pub(crate) identity: OriginalProcessIdentity,
    pub(crate) thread_disposition: OriginalThreadDisposition,
    pub(crate) owner: CheckedIoOwnerToken,
}

impl CheckedRunnerIo {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn startup_id(&self) -> StartupId {
        self.owner.startup_id()
    }

    pub(crate) fn verify_original_process(&self) -> io::Result<()> {
        let state = self
            .owner
            .owner()
            .state
            .lock()
            .map_err(|_| io::Error::other("checked owner state lock poisoned"))?;
        let Some(ref process) = state.process else {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "checked runner io missing original process handle",
            ));
        };
        self.identity.verify_same_process(process)
    }
}

#[cfg(test)]
#[path = "checked_owner_tests.rs"]
mod tests;

#[cfg(test)]
mod asuser_preprocess_retention_tests {
    use super::*;
    #[test] fn dropping_installed_a1_before_process_publication_keeps_original_registry_and_connect_charge() {
        static REGISTRY:CheckedOwnerRegistry=CheckedOwnerRegistry::new();
        static CONNECT:owned_connect_worker::Owner=owned_connect_worker::Owner::new();
        let startup=StartupId(47);
        let deadline=Instant::now()+Duration::from_secs(1);
        CONNECT.reserve_for_startup(startup).unwrap();
        let owner=Arc::new(CheckedRunnerOwner::new(startup,deadline,None,None,&CONNECT,Some(&REGISTRY)));
        // Script only the installed A1 phase. No artifact, permit, native T,
        // login, process, IO worker, OS query or release capability is made.
        assert!(owner.asuser_a1.set(crate::asuser_a1_native::Cell::new(startup.0,deadline)).is_ok());
        REGISTRY.inner.lock().unwrap().slots[0]=Some(owner.clone());
        let reservation=CheckedStartupReservation{owner:owner.clone(),registry:&REGISTRY,
            custom_connect_owner:Some(&CONNECT),committed_to_transport:false,broker_resource_only_retired:false};
        assert!(owner.state.lock().unwrap().process.is_none());
        drop(reservation);
        assert!(owner.is_sticky_unknown());assert!(owner.is_no_replay_sealed());
        assert!(REGISTRY.inner.lock().unwrap().slots[0].as_ref().is_some_and(|original|Arc::ptr_eq(original,&owner)));
        assert_eq!(CONNECT.reserve_for_startup(StartupId(48)).unwrap_err().kind(),io::ErrorKind::WouldBlock);
        let state=owner.state.lock().unwrap();
        assert!(state.process.is_none());assert!(state.identity.is_none());
        assert!(state.finalized_outcome.is_none());assert!(!owner.native_local_retired.load(std::sync::atomic::Ordering::Acquire));
        assert!(matches!(state.sticky_unknown,Some(StickyUnknownReason::StartupUnwound)));
    }
}

#[cfg(test)]
mod startup_context_owner_tests {
    use super::*;
    use crate::start_diagnostic::{ContextFact as F,ContextUnavailable as U};
    #[test]
    fn prequery_sampler_never_uses_existing_diagnostic_latest_as_an_original_process() {
        static CONNECT:owned_connect_worker::Owner=owned_connect_worker::Owner::new();
        let owner=CheckedRunnerOwner::new(StartupId(39),Instant::now()+Duration::from_secs(1),
            None,None,&CONNECT,None);
        let diagnostic=crate::WindowsStartDiagnostic::default();
        let labelled=crate::WindowsStartRunnerObservation{startup_id:39,runner_pid:123,
            runner_creation_filetime_100ns:456,state:crate::WindowsStartRunnerState::Exited{raw_dword:7}};
        diagnostic.observe_original_runner(labelled);
        assert_eq!(owner.sample_startup_runner_for_context(Some(&diagnostic)),F::NotObserved(U::NotApplicable));
        assert_eq!(diagnostic.snapshot().unwrap().current.original_runner,Some(labelled));
        {
            let _guard=owner.state.lock().unwrap();
            assert_eq!(owner.sample_startup_runner_for_context(Some(&diagnostic)),F::NotObserved(U::Busy));
        }
        assert_eq!(diagnostic.snapshot().unwrap().current.startup_context,None);
    }
    #[test]
    fn same_owner_busy_and_poison_are_explicit_without_os_queries_or_grant_defaults() {
        static CONNECT:owned_connect_worker::Owner=owned_connect_worker::Owner::new();
        let owner=Arc::new(CheckedRunnerOwner::new(StartupId(38),Instant::now()+Duration::from_secs(1),
            None,None,&CONNECT,None));
        let diagnostic=crate::WindowsStartDiagnostic::default();
        {
            let _busy=owner.state.lock().unwrap();
            owner.observe_startup_context(Some(&diagnostic));
        }
        let context=diagnostic.snapshot().unwrap().current.startup_context.unwrap();
        assert_eq!(context.startup_id,38);
        assert_eq!(context.query_status,F::NotObserved(U::Busy));
        assert_eq!(context.original_runner_identity,None);
        assert_eq!(context.desktop.access,F::NotObserved(U::Busy));
        let other=Arc::clone(&owner);
        assert!(std::thread::spawn(move||{
            let _guard=other.state.lock().unwrap();panic!("labelled owner poison");
        }).join().is_err());
        owner.observe_startup_context(Some(&diagnostic));
        let context=diagnostic.snapshot().unwrap().current.startup_context.unwrap();
        assert_eq!(context.query_status,F::NotObserved(U::Poisoned));
        assert_eq!(context.station.access,F::NotObserved(U::Poisoned));
        assert!(owner.is_sticky_unknown());
    }
}
