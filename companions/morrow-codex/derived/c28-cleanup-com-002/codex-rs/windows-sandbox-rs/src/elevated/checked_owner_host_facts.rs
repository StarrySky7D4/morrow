//! Bounded, secret-free, read-only host-side native and IO finalization facts
//! captured on the original `CheckedRunnerOwner` at its existing native/IO
//! operation points.
//!
//! This module does not perform any OS queries, waits, closes, or control
//! operations, never retains OS handles, paths, nonces, or raw error strings,
//! and does not claim global quota or outer keeper release authority.

use super::{
    AsyncInputSlot, HelperAttemptRecord, OriginalProcessIdentity, OriginalThreadCloseDebt,
    OriginalThreadDisposition, OwnerLifecycleStage, OwnerState, RELEASE_ELIGIBLE, StartupId,
    StdWorkerSlot, StickyUnknownReason, WorkerSlotState,
};
use crate::elevated::native_close_protocol::{
    BusinessExitCodeI32, ExternalQuotaScopeDisposition, CloseDebtFact, CloseDebtTarget, FirstErrorFact, HandleCloseFact,
    LeaseRetirementFact, ObservationProof, OriginalProcessWaitProof, PipeEofFact, PoisonStateFact,
    PoisonTarget, ProductCloseScopeDisposition, ProtocolFailureStage, ProtocolStickyFailureKind,
    RawProcessExitCodeDword, ReceiverPhysicalCloseFacts, ReplayDisposition, StickyUnknownStateFact,
    TimeoutDisposition, VerifiedChildProcessIdentity, WIN32_STILL_ACTIVE_EXIT_CODE_DWORD,
    WorkerNativeJoinFact,
};
use crate::ipc_framed::ControlStatus;
use crate::owned_connect_worker::{CancelObservation, UnknownReason};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Availability state of the non-blocking host-facts observation mirror on `CheckedRunnerOwner`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostFactsObservationAvailability {
    /// Owner state is idle and not poisoned; snapshot reflects the latest committed state.
    Live,
    /// At least one worker slot is currently loaned to an active finalizer (`LoanedToFinalizer`).
    BorrowedByFinalizer,
    /// The owner `Mutex<OwnerState>` is currently locked by an active `StateGuard`;
    /// reading returns the latest committed post-unlock snapshot without blocking.
    BusyOwnerGuardHeld,
    /// `CheckedRunnerOwner` has been dropped; the final committed snapshot remains readable.
    RetiredAfterOwnerDrop,
    /// The owner `Mutex<OwnerState>` was poisoned; the snapshot retains all prior facts
    /// plus sticky `PoisonStateFact::PoisonObserved` and `noReplay`.
    PoisonedOwnerMutex,
}

/// Bounded scalar record of an identity validation failure on the original runner process handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostIdentityFailureRecord {
    pub(crate) expected_pid: u32,
    pub(crate) os_error: Option<i32>,
}

/// Host-side fact for the original runner OS process creation identity (`pi.hProcess`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostOriginalRunnerIdentityFact {
    /// No runner process handle has been installed in this owner yet.
    NotInstalledPreLaunch,
    /// Original runner process identity (`pid`, `creation_filetime_100ns`) was queried from the
    /// live `pi.hProcess` handle via `GetProcessId` + `GetProcessTimes`.
    /// `verified_identity` is populated when a real non-zero `primary_thread_id` was also
    /// captured (never fabricated for `ScopedSynthetic` owners).
    InstalledLiveVerified {
        pid: u32,
        creation_filetime_100ns: u64,
        primary_thread_id: Option<u32>,
        verified_identity: Option<VerifiedChildProcessIdentity>,
    },
    /// Identity query or `verify_same_process` check failed on the installed process handle.
    IdentityValidationFailed {
        expected_pid: u32,
        os_error: Option<i32>,
    },
}

/// Host-side fact for the original runner OS process wait (`WaitForSingleObject`) and
/// raw exit code (`GetExitCodeProcess` DWORD) on the original `pi.hProcess` handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostOriginalRunnerProcessWaitAndExitFact {
    /// No runner process handle was ever installed.
    NoProcessHandlePresent,
    /// Original runner process handle is installed, but finalization has not yet executed
    /// `WaitForSingleObject` on it.
    HandleRetainedWaitNotYetAttempted,
    /// `verify_same_process` failed on the retained handle before `WaitForSingleObject`.
    IdentityVerificationFailedBeforeWait { os_error: Option<i32> },
    /// `WaitForSingleObject` on the original process handle did not return `WAIT_OBJECT_0`.
    WaitTimedOutOrFailed {
        wait_return_dword: u32,
        os_error: Option<i32>,
    },
    /// `WaitForSingleObject` returned `WAIT_OBJECT_0`, but `GetExitCodeProcess` failed (`0`).
    GetExitCodeFailedAfterWaitSignal {
        os_error: Option<i32>,
        signal_timeout_disposition: TimeoutDisposition,
    },
    /// `WaitForSingleObject` returned `WAIT_OBJECT_0` on the verified original process handle
    /// and `GetExitCodeProcess` succeeded, yielding the exact raw `DWORD` (including legitimate
    /// `259` / `STILL_ACTIVE` disambiguated by the positive original wait signal).
    SignaledAndExitCodeObservedOnOriginalHandle {
        raw_exit_code_dword: RawProcessExitCodeDword,
        wait_proof: OriginalProcessWaitProof,
        timeout_disposition: TimeoutDisposition,
    },
}

impl HostOriginalRunnerProcessWaitAndExitFact {
    /// Returns the raw `DWORD` exit code (including legitimate `259` / `STILL_ACTIVE`) ONLY
    /// when `WaitForSingleObject` positively signaled exit (`WAIT_OBJECT_0`) on the verified
    /// original process handle within the finalization budget.
    pub(crate) const fn verified_exited_raw_dword(self) -> Option<RawProcessExitCodeDword> {
        match self {
            Self::SignaledAndExitCodeObservedOnOriginalHandle {
                raw_exit_code_dword,
                wait_proof: OriginalProcessWaitProof::SignaledExitedOnOriginalHandle,
                timeout_disposition: TimeoutDisposition::CompletedWithinBudget,
            } => Some(raw_exit_code_dword),
            _ => None,
        }
    }

    /// Returns true if the process exited with raw code `259` (`WIN32_STILL_ACTIVE_EXIT_CODE_DWORD`)
    /// after positive `WAIT_OBJECT_0` wait proof on the original handle.
    pub(crate) const fn is_legitimate_exited_still_active_259(self) -> bool {
        matches!(
            self.verified_exited_raw_dword(),
            Some(RawProcessExitCodeDword(WIN32_STILL_ACTIVE_EXIT_CODE_DWORD))
        )
    }
}

/// Honest disposition for host handles (`pi.hProcess`, startup/worker `File` pipes, thread
/// `OwnedHandle` anchors) whose release in `CheckedRunnerOwner` / `CheckedRunnerControls`
/// occurs via implicit Rust `Drop` rather than an explicit checked `CloseHandle` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostImplicitDropHandleFact {
    /// Handle was never created or installed in this slot.
    NoHandlePresent,
    ClosedVerifiedOnOriginal,
    CloseFailedRetained { os_error: i32 },
    /// Handle is still retained open in `CheckedRunnerOwner` or an active runtime worker.
    RetainedOpen,
    /// Handle slot was cleared via implicit `Drop` of `File` / `Arc<OwnedHandle>` without a
    /// checked `CloseHandle` return value; never upgraded to `ClosedVerifiedOnce`.
    ReleasedViaImplicitDropNotCheckedCloseProof,
}

impl HostImplicitDropHandleFact {
    pub(crate) const fn to_protocol_handle_close_fact(self) -> HandleCloseFact {
        match self {
            Self::NoHandlePresent => HandleCloseFact::NotObserved,
            Self::ClosedVerifiedOnOriginal=>HandleCloseFact::ClosedVerifiedOnce,
            Self::CloseFailedRetained{os_error}=>HandleCloseFact::CloseDebtRetained{os_error:Some(os_error)},
            Self::RetainedOpen => HandleCloseFact::StillOpen,
            Self::ReleasedViaImplicitDropNotCheckedCloseProof => HandleCloseFact::NotProven,
        }
    }

    pub(crate) const fn to_observation_proof(self) -> ObservationProof {
        match self {
            Self::NoHandlePresent => ObservationProof::NotObserved,
            Self::ClosedVerifiedOnOriginal=>ObservationProof::ProvenOnOriginal,
            Self::CloseFailedRetained{..}=>ObservationProof::NotProven,
            Self::RetainedOpen | Self::ReleasedViaImplicitDropNotCheckedCloseProof => {
                ObservationProof::NotProven
            }
        }
    }
}

/// Host-side fact for the original primary thread handle (`pi.hThread`), which DOES use
/// an explicit checked `CloseHandle` call at capture time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostOriginalPrimaryThreadDispositionFact {
    /// No primary thread handle was installed (pre-launch).
    NotInstalledPreLaunch,
    /// Checked `CloseHandle(pi.hThread)` succeeded once on capture.
    ClosedVerifiedOnCapture { thread_id: u32 },
    /// Checked `CloseHandle(pi.hThread)` failed or raw handle was invalid; close debt is
    /// permanently retained without exposing the raw kernel handle value.
    CloseFailedDebtRetained { thread_id: u32, os_error: u32 },
    /// Scoped test/actor constructor (`new_scoped_for_actor`) where no original `pi.hThread`
    /// was passed to `CheckedRunnerOwner`.
    ScopedSyntheticNotObservedOnOriginalHandle,
}

impl HostOriginalPrimaryThreadDispositionFact {
    pub(crate) const fn to_protocol_handle_close_fact(self) -> HandleCloseFact {
        match self {
            Self::NotInstalledPreLaunch | Self::ScopedSyntheticNotObservedOnOriginalHandle => {
                HandleCloseFact::NotObserved
            }
            Self::ClosedVerifiedOnCapture { .. } => HandleCloseFact::ClosedVerifiedOnce,
            Self::CloseFailedDebtRetained { os_error, .. } => HandleCloseFact::CloseDebtRetained {
                os_error: Some(os_error as i32),
            },
        }
    }
}

/// Host-side fact for `RuntimeWorkerAdmission` reservation and retirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostWorkerAdmissionFact {
    /// `reserve_runtime_workers` has not been called on this owner.
    NotReserved,
    /// `RuntimeWorkerAdmission` is currently active and holding the worker slot reservations.
    ReservationActiveInProgress,
    /// `RuntimeWorkerAdmission` installed all three original workers (writer, input, reader)
    /// and retired cleanly before the go-gates committed.
    RetiredAllThreeWorkersInstalledAndCommitted,
    /// `RuntimeWorkerAdmission` retired after partial startup abort, spawner error, or
    /// `noReplay` seal; records which original worker slots were installed and anchored.
    RetiredAfterPartialAbortOrSeal {
        reader_installed: bool,
        writer_installed: bool,
        input_installed: bool,
    },
}

/// Detailed host-side worker slot placement and native join fact distinguishing empty slots,
/// reserved slots, pending unjoined handles, active finalizer loans, timeouts, panics,
/// cancellations, and timely vs late completions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostWorkerSlotJoinFact {
    /// Slot is empty (`WorkerSlotState::NotInstalled`); worker was never started.
    NotInstalled,
    /// Slot is reserved during `RuntimeWorkerAdmission` (`WorkerSlotState::ReservedForInstall`).
    ReservedNotInstalled,
    /// Worker `JoinHandle` is installed in the slot, but native join has not been attempted yet.
    PendingUnjoined,
    /// Worker `JoinHandle` is currently loaned to an active finalizer (`LoanedToFinalizer`).
    LoanedToFinalizerInProgress,
    /// Native thread wait or async task timeout expired while worker was still running.
    TimedOutStillRunning,
    /// Worker joined within the finalization deadline and reported a clean outcome.
    JoinedTimelyClean,
    /// Worker joined within the finalization deadline, but reported an unclean worker outcome.
    JoinedWithWorkerError,
    /// Worker joined, but the finalization deadline expired before or during the join.
    JoinedLateAfterBudget,
    /// Worker std thread or async task panicked during execution.
    Panicked,
    /// Async input task was cancelled (`JoinError::is_cancelled()`).
    AsyncTaskCancelled,
}

impl HostWorkerSlotJoinFact {
    pub(crate) const fn to_protocol_worker_join_fact(self) -> WorkerNativeJoinFact {
        match self {
            Self::NotInstalled | Self::ReservedNotInstalled => WorkerNativeJoinFact::NotObserved,
            Self::PendingUnjoined | Self::LoanedToFinalizerInProgress => {
                WorkerNativeJoinFact::StillRunning
            }
            Self::TimedOutStillRunning => WorkerNativeJoinFact::TimedOutStillRunning,
            Self::JoinedTimelyClean => WorkerNativeJoinFact::JoinedTimelyClean,
            Self::JoinedWithWorkerError | Self::AsyncTaskCancelled => {
                WorkerNativeJoinFact::JoinedWithWorkerError
            }
            Self::JoinedLateAfterBudget => WorkerNativeJoinFact::JoinedLateAfterBudget,
            Self::Panicked => WorkerNativeJoinFact::Panicked,
        }
    }
}

/// Bounded scalar summary of `ReaderWorkerOutcome` after the original reader `JoinHandle` joins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostReaderOutcomeSummary {
    pub(crate) protocol_exit: Option<BusinessExitCodeI32>,
    pub(crate) physical_eof: bool,
    pub(crate) clean: bool,
}

/// Host-side fact for the dedicated std host reader thread (`StdJoinHandle<ReaderWorkerOutcome>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostStdReaderWorkerFact {
    pub(crate) slot_join: HostWorkerSlotJoinFact,
    pub(crate) protocol_join: WorkerNativeJoinFact,
    pub(crate) loaned_to_finalizer: bool,
    pub(crate) thread_anchor_close: HostImplicitDropHandleFact,
    pub(crate) pipe_read_handle_close: HostImplicitDropHandleFact,
    pub(crate) outcome: Option<HostReaderOutcomeSummary>,
}

/// Bounded scalar summary of `WriterWorkerOutcome` after the original writer `JoinHandle` joins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostWriterOutcomeSummary {
    pub(crate) clean: bool,
    pub(crate) termination_sent: bool,
}

/// Host-side fact for the dedicated std blocking pipe writer thread (`StdJoinHandle<WriterWorkerOutcome>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostStdWriterWorkerFact {
    pub(crate) slot_join: HostWorkerSlotJoinFact,
    pub(crate) protocol_join: WorkerNativeJoinFact,
    pub(crate) loaned_to_finalizer: bool,
    pub(crate) thread_anchor_close: HostImplicitDropHandleFact,
    pub(crate) pipe_write_handle_close: HostImplicitDropHandleFact,
    pub(crate) outcome: Option<HostWriterOutcomeSummary>,
}

/// Typed classification of an async input `JoinError` without relying on raw string messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostAsyncInputJoinErrorKind {
    Cancelled,
    Panicked,
}

/// Host-side fact for the async input worker task (`TokioJoinHandle<InputWorkerOutcome>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostAsyncInputWorkerFact {
    pub(crate) task_completion: HostWorkerSlotJoinFact,
    pub(crate) protocol_join: WorkerNativeJoinFact,
    pub(crate) loaned_to_finalizer: bool,
    pub(crate) outcome_clean: Option<bool>,
}

/// Aggregated host runtime worker facts (reader std thread, writer std thread, async input task).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostRuntimeWorkersFacts {
    pub(crate) reader: HostStdReaderWorkerFact,
    pub(crate) writer: HostStdWriterWorkerFact,
    pub(crate) input: HostAsyncInputWorkerFact,
}

impl HostRuntimeWorkersFacts {
    pub(crate) const fn is_any_loaned(&self) -> bool {
        self.reader.loaned_to_finalizer
            || self.writer.loaned_to_finalizer
            || self.input.loaned_to_finalizer
    }
}

/// Ordering and cause of reader loop termination relative to protocol `Message::Exit` and physical pipe EOF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostReaderEofOrderingFact {
    NotObserved,
    AbortedBeforeReaderStart,
    CleanPhysicalEofAfterBusinessExit,
    PhysicalEofWithoutBusinessExit,
    ProtocolViolationFrameAfterBusinessExit,
    ReadFailedOrInvalidFrameBeforeEof,
}

/// Host-side facts for genuine physical pipe EOF on `pipe-out` and protocol `Message::Exit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostReaderPipeEofAndProtocolExitFacts {
    pub(crate) reader_pipe_eof: PipeEofFact,
    pub(crate) eof_ordering: HostReaderEofOrderingFact,
    pub(crate) business_protocol_exit_code: Option<BusinessExitCodeI32>,
}

/// Resolution disposition of the `CloseStdin` control operation on the host side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostCloseStdinResolutionFact {
    NotRequested,
    ConfirmedStatus(ControlStatus),
    SettledUnconfirmedToUnknown,
}

/// High-level state of host-side control request correlation and pending control settlement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostControlsResolutionStateFact {
    NoControlsRuntimeInstalled,
    ActiveOrIdleNoUnconfirmed,
    AllAdmittedConfirmedClean,
    UnconfirmedSettledToUnknownOnExitOrFailure { settled_count: u16 },
    RemoteUnknownOrProtocolFailureObserved,
    CorrelationMutexPoisoned,
}

/// Bounded scalar facts for host control requests and writer correlation in `CheckedRunnerControls`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostPendingControlsAndWriterFacts {
    pub(crate) runtime_installed: bool,
    pub(crate) admitted_requests_count: u16,
    pub(crate) confirmed_applied_count: u16,
    pub(crate) confirmed_non_applied_count: u16,
    pub(crate) remote_unknown_count: u16,
    pub(crate) unconfirmed_settled_to_unknown_count: u16,
    pub(crate) rejected_after_terminal_or_cap_count: u16,
    pub(crate) pending_unsettled_count: u16,
    pub(crate) close_stdin_resolution: HostCloseStdinResolutionFact,
    pub(crate) resolution_state: HostControlsResolutionStateFact,
}

impl Default for HostPendingControlsAndWriterFacts {
    fn default() -> Self {
        Self {
            runtime_installed: false,
            admitted_requests_count: 0,
            confirmed_applied_count: 0,
            confirmed_non_applied_count: 0,
            remote_unknown_count: 0,
            unconfirmed_settled_to_unknown_count: 0,
            rejected_after_terminal_or_cap_count: 0,
            pending_unsettled_count: 0,
            close_stdin_resolution: HostCloseStdinResolutionFact::NotRequested,
            resolution_state: HostControlsResolutionStateFact::NoControlsRuntimeInstalled,
        }
    }
}

/// Host-side fact for one of the two bounded pipe-connect helper attempt slots (`pipe-in`, `pipe-out`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostConnectHelperSlotFact {
    Unused,
    PendingUnacknowledged {
        attempt_id: u64,
    },
    AcknowledgedTimelyClean {
        attempt_id: u64,
    },
    AcknowledgedLateOrUnknown {
        attempt_id: u64,
        within_deadline: bool,
        had_unknown: bool,
    },
}

impl HostConnectHelperSlotFact {
    pub(crate) const fn from_record(record: Option<HelperAttemptRecord>) -> Self {
        match record {
            None => Self::Unused,
            Some(entry) if !entry.acknowledged => Self::PendingUnacknowledged {
                attempt_id: entry.attempt_id.0,
            },
            Some(entry) if entry.within_deadline && !entry.had_unknown => {
                Self::AcknowledgedTimelyClean {
                    attempt_id: entry.attempt_id.0,
                }
            }
            Some(entry) => Self::AcknowledgedLateOrUnknown {
                attempt_id: entry.attempt_id.0,
                within_deadline: entry.within_deadline,
                had_unknown: entry.had_unknown,
            },
        }
    }
}

/// Disposition of the local `owned_connect_worker::Owner` startup reservation for this owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostLocalConnectLaneFact {
    NotReservedScopedOrUnbound,
    ReservedActive,
    ReleasedOnTransportCommit,
    ReleasedOnFinalizeReap,
    ReleasedOnStartupFailureOrDrop,
}

/// Disposition of the local 16-slot `CheckedOwnerRegistry` entry for this owner.
///
/// Explicitly distinct from outer session keeper and global native execution quota.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostLocalRegistrySlotFact {
    /// Owner was created via `new_scoped_for_actor` or without a `CheckedOwnerRegistry`.
    NotRegisteredScopedOrUnbound,
    /// Slot is currently occupied in the local 16-slot `CheckedOwnerRegistry`.
    RetainedActiveInLocalRegistry { startup_id: u64 },
    /// Slot was released in `CheckedOwnerRegistry` after physical close in `finalize_with_claim`.
    ReleasedAfterPhysicalClose { startup_id: u64 },
    /// Slot was released in `CheckedOwnerRegistry` on pre-launch `CheckedStartupReservation` drop
    /// before any process launch, thread close debt, helper attempt, or `noReplay` seal.
    ReleasedPreLaunchNoProcess { startup_id: u64 },
    /// Slot remains permanently charged in `CheckedOwnerRegistry` because original primary thread
    /// `CloseHandle` failed (`OriginalThreadCloseDebt`).
    RetainedDueToThreadCloseDebt { startup_id: u64 },
    /// `CheckedOwnerRegistry` mutex was poisoned when `release_slot_after_physical_close` ran.
    ReleaseFailedRegistryLockPoisoned { startup_id: u64 },
    /// `release_slot_after_physical_close` ran on a live registry lock, but no matching slot
    /// with `startup_id` was present.
    ReleaseAttemptedSlotNotFound { startup_id: u64 },
}

impl HostLocalRegistrySlotFact {
    pub(crate) const fn to_local_slot_retirement_fact(self) -> LeaseRetirementFact {
        match self {
            Self::NotRegisteredScopedOrUnbound => LeaseRetirementFact::NotObserved,
            Self::RetainedActiveInLocalRegistry { .. } => LeaseRetirementFact::ActiveOrLoaned,
            Self::ReleasedAfterPhysicalClose { .. } | Self::ReleasedPreLaunchNoProcess { .. } => {
                LeaseRetirementFact::RetiredCleanly
            }
            Self::RetainedDueToThreadCloseDebt { .. }
            | Self::ReleaseFailedRegistryLockPoisoned { .. } => {
                LeaseRetirementFact::ParkedUnresolved
            }
            Self::ReleaseAttemptedSlotNotFound { .. } => LeaseRetirementFact::NotProven,
        }
    }
}

/// Explicit boundary fact proving that local `CheckedOwnerRegistry` release is NOT outer
/// keeper/process-map retirement or global `ExternalNativeQuotaAdapter` (`session-exec-windows-v1`)
/// quota release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostExternalAdapterScopeFact {
    /// Outer adapter (`companions/morrow-codex/session-exec-windows-v1/src/registry.rs`,
    /// `NativeExecutionRegistry` / `ExecutionSlot`) and runner-side child/job keeper
    /// (`command_runner_win.rs`) are outside this host owner and unwired here.
    UnwiredOuterAdapterNotProvenByLocalRegistry { release_eligible: bool },
}

/// Host-side slot and lease facts separating local owner reservations from unwired outer
/// keeper/process-map and global quota authorities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostSlotAndLeaseFacts {
    pub(crate) host_input_writer_lease: LeaseRetirementFact,
    pub(crate) local_connect_helpers: [HostConnectHelperSlotFact; 2],
    pub(crate) local_connect_lane: HostLocalConnectLaneFact,
    pub(crate) local_owner_registry_slot: HostLocalRegistrySlotFact,
    pub(crate) local_owner_registry_retirement: LeaseRetirementFact,
    pub(crate) local_worker_admission: HostWorkerAdmissionFact,
    /// Runner-side child/job keeper and outer session process-map lease are outside
    /// `CheckedRunnerOwner` and never fabricated as retired here (`LeaseRetirementFact::NotObserved`).
    pub(crate) external_keeper_lease: LeaseRetirementFact,
    /// Outer `ExternalNativeQuotaAdapter` (`session-exec-windows-v1/src/registry.rs`) is not
    /// wired to `CheckedOwnerRegistry` and never fabricated as retired here (`LeaseRetirementFact::NotObserved`).
    pub(crate) external_global_quota_lease: LeaseRetirementFact,
    pub(crate) external_adapter_scope: HostExternalAdapterScopeFact,
}

/// Secret-free scalar classification of the first host `StickyUnknownReason` latched on `CheckedRunnerOwner`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostStickyFailureKind {
    CallerAwaitCancelled,
    IdentityValidationFailed,
    ThreadCloseFailed {
        thread_id: u32,
        os_error: u32,
    },
    ConnectRetainedUnknown {
        attempt_id: u64,
        wait_os_error: Option<u32>,
        cancel_os_error: Option<u32>,
    },
    LateConnectSuccess {
        attempt_id: u64,
    },
    ConnectFailed {
        attempt_id: u64,
    },
    HandshakeFailed,
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
    OwnerMutexPoisoned,
}

/// First sticky failure record latched on `CheckedRunnerOwner`.
/// Once recorded, later failures or late physical reaping never overwrite or clear this record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostFirstStickyFailureFact {
    pub(crate) host_kind: HostStickyFailureKind,
    pub(crate) stage_when_latched: OwnerLifecycleStage,
    pub(crate) protocol_first_error: FirstErrorFact,
}

/// Host-side close debt, first error, mutex poison, and sticky `Unknown` / `noReplay` facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostDebtPoisonAndStickyUnknownFacts {
    pub(crate) close_debt: CloseDebtFact,
    pub(crate) first_sticky_failure: Option<HostFirstStickyFailureFact>,
    pub(crate) first_error: FirstErrorFact,
    pub(crate) poison_state: PoisonStateFact,
    pub(crate) owner_mutex_poisoned: bool,
    pub(crate) controls_correlation_mutex_poisoned: bool,
    pub(crate) sticky_unknown_state: StickyUnknownStateFact,
    pub(crate) no_replay_sealed: bool,
    pub(crate) late_physical_reap_after_sticky_unknown: bool,
}

/// Complete, bounded, `Copy`, secret-free read-only snapshot of host-side native and IO
/// finalization facts from the original `CheckedRunnerOwner`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostNativeCloseFactsSnapshot {
    pub(crate) startup_id: StartupId,
    pub(crate) observation_availability: HostFactsObservationAvailability,
    pub(crate) observation_revision: u64,
    pub(crate) stage: OwnerLifecycleStage,
    pub(crate) runner_identity: HostOriginalRunnerIdentityFact,
    pub(crate) runner_process_wait_and_exit: HostOriginalRunnerProcessWaitAndExitFact,
    pub(crate) runner_process_handle_close: HostImplicitDropHandleFact,
    pub(crate) primary_thread_disposition: HostOriginalPrimaryThreadDispositionFact,
    pub(crate) worker_admission: HostWorkerAdmissionFact,
    pub(crate) runtime_workers: HostRuntimeWorkersFacts,
    pub(crate) reader_eof_and_exit: HostReaderPipeEofAndProtocolExitFacts,
    pub(crate) controls_and_writer: HostPendingControlsAndWriterFacts,
    pub(crate) slots_and_leases: HostSlotAndLeaseFacts,
    pub(crate) debt_poison_and_sticky_unknown: HostDebtPoisonAndStickyUnknownFacts,
    pub(crate) product_scope: ProductCloseScopeDisposition,
}

impl HostNativeCloseFactsSnapshot {
    const fn cached_input_writer_lease(&self) -> LeaseRetirementFact {
        if let LeaseRetirementFact::RetiredCleanly=self.slots_and_leases.host_input_writer_lease {
            if !self.debt_poison_and_sticky_unknown.no_replay_sealed
                && !self.debt_poison_and_sticky_unknown.owner_mutex_poisoned
                && !self.debt_poison_and_sticky_unknown.controls_correlation_mutex_poisoned {
                return LeaseRetirementFact::RetiredCleanly;
            }
        }
        if self.debt_poison_and_sticky_unknown.no_replay_sealed
            || self.debt_poison_and_sticky_unknown.owner_mutex_poisoned
            || self.debt_poison_and_sticky_unknown.controls_correlation_mutex_poisoned {
            return LeaseRetirementFact::ParkedUnresolved;
        }
        if self.runtime_workers.writer.loaned_to_finalizer || self.runtime_workers.input.loaned_to_finalizer {
            return LeaseRetirementFact::ActiveOrLoaned;
        }
        match (self.runtime_workers.writer.slot_join, self.runtime_workers.input.task_completion) {
            (HostWorkerSlotJoinFact::NotInstalled | HostWorkerSlotJoinFact::ReservedNotInstalled,
             HostWorkerSlotJoinFact::NotInstalled | HostWorkerSlotJoinFact::ReservedNotInstalled) => LeaseRetirementFact::NotObserved,
            (HostWorkerSlotJoinFact::JoinedTimelyClean, HostWorkerSlotJoinFact::JoinedTimelyClean) =>
                LeaseRetirementFact::NotProven, // no separately captured lease retirement witness
            (HostWorkerSlotJoinFact::TimedOutStillRunning | HostWorkerSlotJoinFact::JoinedWithWorkerError
                | HostWorkerSlotJoinFact::JoinedLateAfterBudget | HostWorkerSlotJoinFact::Panicked
                | HostWorkerSlotJoinFact::AsyncTaskCancelled, _)
            | (_, HostWorkerSlotJoinFact::TimedOutStillRunning | HostWorkerSlotJoinFact::JoinedWithWorkerError
                | HostWorkerSlotJoinFact::JoinedLateAfterBudget | HostWorkerSlotJoinFact::Panicked
                | HostWorkerSlotJoinFact::AsyncTaskCancelled) => LeaseRetirementFact::ParkedUnresolved,
            _ => LeaseRetirementFact::ActiveOrLoaned,
        }
    }
    /// Projects the host-side worker joins and runner process observations into the
    /// frozen `native_close_protocol::ReceiverPhysicalCloseFacts` structure.
    ///
    /// Note that `runner_handle_closed_verified` is honestly reported as
    /// `ObservationProof::NotProven` (or `NotObserved` before release) because
    /// `CheckedRunnerOwner` releases `state.process` (`Arc<OwnedHandle>`) via implicit
    /// Rust `Drop` rather than an explicit checked `CloseHandle` call.
    pub(crate) const fn to_v8_receiver_physical_close_facts(&self) -> ReceiverPhysicalCloseFacts {
        let reader_worker_joined_timely_clean = match (
            self.runtime_workers.reader.slot_join,
            self.runtime_workers.reader.outcome,
        ) {
            (
                HostWorkerSlotJoinFact::JoinedTimelyClean,
                Some(HostReaderOutcomeSummary {
                    physical_eof: true,
                    clean: true,
                    ..
                }),
            ) => ObservationProof::ProvenOnOriginal,
            (
                HostWorkerSlotJoinFact::NotInstalled | HostWorkerSlotJoinFact::ReservedNotInstalled,
                _,
            ) => ObservationProof::NotObserved,
            _ => ObservationProof::NotProven,
        };

        let writer_worker_joined_timely_clean = match (
            self.runtime_workers.writer.slot_join,
            self.runtime_workers.writer.outcome,
        ) {
            (
                HostWorkerSlotJoinFact::JoinedTimelyClean,
                Some(HostWriterOutcomeSummary { clean: true, .. }),
            ) => ObservationProof::ProvenOnOriginal,
            (
                HostWorkerSlotJoinFact::NotInstalled | HostWorkerSlotJoinFact::ReservedNotInstalled,
                _,
            ) => ObservationProof::NotObserved,
            _ => ObservationProof::NotProven,
        };

        let input_worker_joined_timely_clean = match (
            self.runtime_workers.input.task_completion,
            self.runtime_workers.input.outcome_clean,
        ) {
            (HostWorkerSlotJoinFact::JoinedTimelyClean, Some(true)) => {
                ObservationProof::ProvenOnOriginal
            }
            (
                HostWorkerSlotJoinFact::NotInstalled | HostWorkerSlotJoinFact::ReservedNotInstalled,
                _,
            ) => ObservationProof::NotObserved,
            _ => ObservationProof::NotProven,
        };

        let runner_process_identity = match self.runner_identity {
            HostOriginalRunnerIdentityFact::InstalledLiveVerified {
                verified_identity, ..
            } => verified_identity,
            _ => None,
        };

        let runner_exit_code_dword = self
            .runner_process_wait_and_exit
            .verified_exited_raw_dword();

        ReceiverPhysicalCloseFacts {
            reader_worker_joined_timely_clean,
            writer_worker_joined_timely_clean,
            input_worker_joined_timely_clean,
            runner_process_identity,
            runner_exit_code_dword,
            runner_handle_closed_verified: self.runner_process_handle_close.to_observation_proof(),
            // Worker completion alone does not capture a separate lease
            // retirement/checked pipe-close witness. Never mint RetiredCleanly.
            host_input_writer_lease: self.cached_input_writer_lease(),
            host_keeper_and_desktop_lease: self.slots_and_leases.external_keeper_lease,
            host_local_registry_quota_lease: self.slots_and_leases.local_owner_registry_retirement,
            external_quota_scope: match self.slots_and_leases.external_adapter_scope {
                HostExternalAdapterScopeFact::UnwiredOuterAdapterNotProvenByLocalRegistry { .. } =>
                    ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter,
            },
        }
    }
}

pub(crate) fn classify_sticky_unknown_first_error(
    reason: &StickyUnknownReason,
    stage: OwnerLifecycleStage,
) -> HostFirstStickyFailureFact {
    let mapped_stage = match stage {
        OwnerLifecycleStage::ReservedBeforeLaunch
        | OwnerLifecycleStage::ProcessLaunchedInHandshake
        | OwnerLifecycleStage::HandshakeSucceededInTransport
        | OwnerLifecycleStage::CheckedIoExtracted => ProtocolFailureStage::StartupAndHandshake,
        OwnerLifecycleStage::ActorAdopted => ProtocolFailureStage::InputDrainAndJoin,
        OwnerLifecycleStage::FinalizedClean
        | OwnerLifecycleStage::PhysicallyReapedStickyUnknown
        | OwnerLifecycleStage::FailedOrAbandoned => ProtocolFailureStage::LeaseRetirement,
    };

    let (host_kind, proto_kind, stage_override, os_error) = match reason {
        StickyUnknownReason::ConnectRetainedUnknown {
            attempt_id,
            reason: unknown_reason,
            cancel,
        } => {
            let wait_os_error = match unknown_reason {
                UnknownReason::WaitFailed(code) | UnknownReason::UnexpectedWait(code) => {
                    Some(*code)
                }
                UnknownReason::Deadline | UnknownReason::GoNotDelivered => None,
                // These describe the original runner observation, not a helper
                // wait OS error. Its DWORD remains in same-attempt start data.
                UnknownReason::OriginalRunnerExited
                | UnknownReason::OriginalRunnerWaitUnavailable => None,
            };
            let cancel_os_error = match cancel {
                CancelObservation::Failed(code) => Some(*code),
                CancelObservation::Requested => None,
            };
            let first_os = wait_os_error.or(cancel_os_error).map(|c| c as i32);
            (
                HostStickyFailureKind::ConnectRetainedUnknown {
                    attempt_id: attempt_id.0,
                    wait_os_error,
                    cancel_os_error,
                },
                ProtocolStickyFailureKind::StartupValidationFailed,
                Some(ProtocolFailureStage::StartupAndHandshake),
                first_os,
            )
        }
        StickyUnknownReason::LateConnectSuccess { attempt_id } => (
            HostStickyFailureKind::LateConnectSuccess {
                attempt_id: attempt_id.0,
            },
            ProtocolStickyFailureKind::StartupValidationFailed,
            Some(ProtocolFailureStage::StartupAndHandshake),
            None,
        ),
        StickyUnknownReason::ConnectFailed { attempt_id, .. } => (
            HostStickyFailureKind::ConnectFailed {
                attempt_id: attempt_id.0,
            },
            ProtocolStickyFailureKind::StartupValidationFailed,
            Some(ProtocolFailureStage::StartupAndHandshake),
            None,
        ),
        StickyUnknownReason::HandshakeFailed(_) => (
            HostStickyFailureKind::HandshakeFailed,
            ProtocolStickyFailureKind::StartupValidationFailed,
            Some(ProtocolFailureStage::StartupAndHandshake),
            None,
        ),
        StickyUnknownReason::CallerAwaitCancelled => (
            HostStickyFailureKind::CallerAwaitCancelled,
            ProtocolStickyFailureKind::StartupValidationFailed,
            Some(ProtocolFailureStage::StartupAndHandshake),
            None,
        ),
        StickyUnknownReason::StartupUnwound => (
            HostStickyFailureKind::StartupUnwound,
            ProtocolStickyFailureKind::StartupValidationFailed,
            Some(ProtocolFailureStage::StartupAndHandshake),
            None,
        ),
        StickyUnknownReason::TransportDroppedUnadopted => (
            HostStickyFailureKind::TransportDroppedUnadopted,
            ProtocolStickyFailureKind::StartupValidationFailed,
            Some(ProtocolFailureStage::StartupAndHandshake),
            None,
        ),
        StickyUnknownReason::WrongLegacyConversion => (
            HostStickyFailureKind::WrongLegacyConversion,
            ProtocolStickyFailureKind::StartupValidationFailed,
            Some(ProtocolFailureStage::StartupAndHandshake),
            None,
        ),
        StickyUnknownReason::CheckedIoDroppedUnadopted => (
            HostStickyFailureKind::CheckedIoDroppedUnadopted,
            ProtocolStickyFailureKind::StartupValidationFailed,
            Some(ProtocolFailureStage::StartupAndHandshake),
            None,
        ),
        StickyUnknownReason::IdentityValidationFailed(_) => (
            HostStickyFailureKind::IdentityValidationFailed,
            ProtocolStickyFailureKind::StartupValidationFailed,
            Some(ProtocolFailureStage::StartupAndHandshake),
            None,
        ),
        StickyUnknownReason::ThreadCloseFailed {
            thread_id,
            os_error,
            ..
        } => (
            HostStickyFailureKind::ThreadCloseFailed {
                thread_id: *thread_id,
                os_error: *os_error,
            },
            ProtocolStickyFailureKind::ChildHandleCloseFailed,
            Some(ProtocolFailureStage::HandleClosure),
            Some(*os_error as i32),
        ),
        StickyUnknownReason::RuntimeLifecycleFailure => (
            HostStickyFailureKind::RuntimeLifecycleFailure,
            ProtocolStickyFailureKind::InputProtocolViolation,
            Some(ProtocolFailureStage::InputDrainAndJoin),
            None,
        ),
        StickyUnknownReason::ReaderIncompleteOrFailed => (
            HostStickyFailureKind::ReaderIncompleteOrFailed,
            ProtocolStickyFailureKind::StdoutForwardingFailed,
            Some(ProtocolFailureStage::OutputDrainAndJoin),
            None,
        ),
        StickyUnknownReason::WriterFailed => (
            HostStickyFailureKind::WriterFailed,
            ProtocolStickyFailureKind::InputControlWriteFailed,
            Some(ProtocolFailureStage::InputDrainAndJoin),
            None,
        ),
        StickyUnknownReason::InputTaskFailedOrAborted => (
            HostStickyFailureKind::InputTaskFailedOrAborted,
            ProtocolStickyFailureKind::InputJoinTimedOut,
            Some(ProtocolFailureStage::InputDrainAndJoin),
            None,
        ),
        StickyUnknownReason::ProcessWaitExpiredOrFailed => (
            HostStickyFailureKind::ProcessWaitExpiredOrFailed,
            ProtocolStickyFailureKind::ChildWaitFailed,
            Some(ProtocolFailureStage::ChildWait),
            None,
        ),
        StickyUnknownReason::FinalizationDeadlineExpired => (
            HostStickyFailureKind::FinalizationDeadlineExpired,
            ProtocolStickyFailureKind::ChildTimedOut,
            Some(mapped_stage),
            None,
        ),
    };

    HostFirstStickyFailureFact {
        host_kind,
        stage_when_latched: stage,
        protocol_first_error: FirstErrorFact::Recorded {
            kind: proto_kind,
            stage: stage_override.unwrap_or(mapped_stage),
            os_error,
            replay_policy: ReplayDisposition::NoReplaySealed,
        },
    }
}

pub(crate) fn build_host_native_close_facts_snapshot(
    state: &OwnerState,
    startup_id: StartupId,
    has_local_registry: bool,
    owner_mutex_poisoned: bool,
) -> HostNativeCloseFactsSnapshot {
    let primary_thread_id = match state.thread_disposition {
        Some(
            OriginalThreadDisposition::ClosedOnCapture { thread_id }
            | OriginalThreadDisposition::CloseFailedUnknown { thread_id, .. },
        ) if thread_id != 0 => Some(thread_id),
        _ => None,
    };

    let runner_identity = if let Some(failure) = state.identity_failure {
        HostOriginalRunnerIdentityFact::IdentityValidationFailed {
            expected_pid: failure.expected_pid,
            os_error: failure.os_error,
        }
    } else if let Some(OriginalProcessIdentity {
        pid,
        creation_filetime,
    }) = state.identity
    {
        let verified_identity = primary_thread_id.and_then(|tid| {
            VerifiedChildProcessIdentity::try_new(pid, tid, creation_filetime).ok()
        });
        HostOriginalRunnerIdentityFact::InstalledLiveVerified {
            pid,
            creation_filetime_100ns: creation_filetime,
            primary_thread_id,
            verified_identity,
        }
    } else {
        HostOriginalRunnerIdentityFact::NotInstalledPreLaunch
    };

    let runner_process_handle_close = if state.v8_closed[2] {
        HostImplicitDropHandleFact::ClosedVerifiedOnOriginal
    }else if let Some((_,error))=state.v8_close_debts[2] {
        HostImplicitDropHandleFact::CloseFailedRetained{os_error:error as i32}
    }else if state.process.is_some() {
        HostImplicitDropHandleFact::RetainedOpen
    } else if state.runner_process_handle_released_by_drop {
        HostImplicitDropHandleFact::ReleasedViaImplicitDropNotCheckedCloseProof
    } else {
        HostImplicitDropHandleFact::NoHandlePresent
    };

    let primary_thread_disposition = match state.thread_disposition {
        None => HostOriginalPrimaryThreadDispositionFact::NotInstalledPreLaunch,
        Some(OriginalThreadDisposition::ClosedOnCapture { thread_id }) => {
            HostOriginalPrimaryThreadDispositionFact::ClosedVerifiedOnCapture { thread_id }
        }
        Some(OriginalThreadDisposition::CloseFailedUnknown {
            thread_id,
            os_error,
            ..
        }) => HostOriginalPrimaryThreadDispositionFact::CloseFailedDebtRetained {
            thread_id,
            os_error,
        },
        Some(OriginalThreadDisposition::ScopedSynthetic) => {
            HostOriginalPrimaryThreadDispositionFact::ScopedSyntheticNotObservedOnOriginalHandle
        }
    };

    let reader_thread_anchor_close = if state.v8_closed[5] {HostImplicitDropHandleFact::ClosedVerifiedOnOriginal}
        else if let Some((_,error))=state.v8_close_debts[5] {HostImplicitDropHandleFact::CloseFailedRetained{os_error:error as i32}}
        else if state.reader_thread_anchor.is_some() {
        HostImplicitDropHandleFact::RetainedOpen
    } else if state.reader_thread_anchor_released_by_drop {
        HostImplicitDropHandleFact::ReleasedViaImplicitDropNotCheckedCloseProof
    } else {
        HostImplicitDropHandleFact::NoHandlePresent
    };

    let writer_thread_anchor_close = if state.v8_closed[6] {HostImplicitDropHandleFact::ClosedVerifiedOnOriginal}
        else if let Some((_,error))=state.v8_close_debts[6] {HostImplicitDropHandleFact::CloseFailedRetained{os_error:error as i32}}
        else if state.writer_thread_anchor.is_some() {
        HostImplicitDropHandleFact::RetainedOpen
    } else if state.writer_thread_anchor_released_by_drop {
        HostImplicitDropHandleFact::ReleasedViaImplicitDropNotCheckedCloseProof
    } else {
        HostImplicitDropHandleFact::NoHandlePresent
    };

    let pipe_read_handle_close = if state.v8_closed[0] {HostImplicitDropHandleFact::ClosedVerifiedOnOriginal}
        else if let Some((_,error))=state.v8_close_debts[0] {HostImplicitDropHandleFact::CloseFailedRetained{os_error:error as i32}}
        else if state.startup_pipe_read.is_some() || state.reader_pipe_retained
    {
        HostImplicitDropHandleFact::RetainedOpen
    } else if state.reader_pipe_released_by_drop {
        HostImplicitDropHandleFact::ReleasedViaImplicitDropNotCheckedCloseProof
    } else {
        HostImplicitDropHandleFact::NoHandlePresent
    };

    let pipe_write_handle_close =
        if state.v8_closed[1] {HostImplicitDropHandleFact::ClosedVerifiedOnOriginal}
        else if let Some((_,error))=state.v8_close_debts[1] {HostImplicitDropHandleFact::CloseFailedRetained{os_error:error as i32}}
        else if state.startup_pipe_write.is_some() || state.writer_pipe_retained {
            HostImplicitDropHandleFact::RetainedOpen
        } else if state.writer_pipe_released_by_drop {
            HostImplicitDropHandleFact::ReleasedViaImplicitDropNotCheckedCloseProof
        } else {
            HostImplicitDropHandleFact::NoHandlePresent
        };

    let (reader_slot_join, reader_loaned, reader_outcome) = match &state.reader_worker {
        WorkerSlotState::NotInstalled => (HostWorkerSlotJoinFact::NotInstalled, false, None),
        WorkerSlotState::ReservedForInstall => {
            (HostWorkerSlotJoinFact::ReservedNotInstalled, false, None)
        }
        WorkerSlotState::LoanedToFinalizer => {
            let join_fact = if state.reader_join_timed_out {
                HostWorkerSlotJoinFact::TimedOutStillRunning
            } else {
                HostWorkerSlotJoinFact::LoanedToFinalizerInProgress
            };
            (join_fact, true, state.last_reader_outcome)
        }
        WorkerSlotState::Installed(StdWorkerSlot::Pending(_)) => {
            let join_fact = if state.reader_join_timed_out {
                HostWorkerSlotJoinFact::TimedOutStillRunning
            } else {
                HostWorkerSlotJoinFact::PendingUnjoined
            };
            (join_fact, false, None)
        }
        WorkerSlotState::Installed(StdWorkerSlot::Joined {
            outcome,
            within_deadline,
        }) => match outcome {
            Ok(out) => {
                let summary = HostReaderOutcomeSummary {
                    protocol_exit: out.protocol_exit.map(BusinessExitCodeI32),
                    physical_eof: out.physical_eof,
                    clean: out.clean,
                };
                let join_fact = if !*within_deadline {
                    HostWorkerSlotJoinFact::JoinedLateAfterBudget
                } else if out.clean && out.physical_eof {
                    HostWorkerSlotJoinFact::JoinedTimelyClean
                } else {
                    HostWorkerSlotJoinFact::JoinedWithWorkerError
                };
                (join_fact, false, Some(summary))
            }
            Err(_) => (HostWorkerSlotJoinFact::Panicked, false, None),
        },
    };

    let (writer_slot_join, writer_loaned, writer_outcome) = match &state.writer_worker {
        WorkerSlotState::NotInstalled => (HostWorkerSlotJoinFact::NotInstalled, false, None),
        WorkerSlotState::ReservedForInstall => {
            (HostWorkerSlotJoinFact::ReservedNotInstalled, false, None)
        }
        WorkerSlotState::LoanedToFinalizer => {
            let join_fact = if state.writer_join_timed_out {
                HostWorkerSlotJoinFact::TimedOutStillRunning
            } else {
                HostWorkerSlotJoinFact::LoanedToFinalizerInProgress
            };
            (join_fact, true, state.last_writer_outcome)
        }
        WorkerSlotState::Installed(StdWorkerSlot::Pending(_)) => {
            let join_fact = if state.writer_join_timed_out {
                HostWorkerSlotJoinFact::TimedOutStillRunning
            } else {
                HostWorkerSlotJoinFact::PendingUnjoined
            };
            (join_fact, false, None)
        }
        WorkerSlotState::Installed(StdWorkerSlot::Joined {
            outcome,
            within_deadline,
        }) => match outcome {
            Ok(out) => {
                let summary = HostWriterOutcomeSummary {
                    clean: out.clean,
                    termination_sent: out.termination_sent,
                };
                let join_fact = if !*within_deadline {
                    HostWorkerSlotJoinFact::JoinedLateAfterBudget
                } else if out.clean {
                    HostWorkerSlotJoinFact::JoinedTimelyClean
                } else {
                    HostWorkerSlotJoinFact::JoinedWithWorkerError
                };
                (join_fact, false, Some(summary))
            }
            Err(_) => (HostWorkerSlotJoinFact::Panicked, false, None),
        },
    };

    let (input_completion, input_loaned, input_clean) = match &state.input_worker {
        WorkerSlotState::NotInstalled => (HostWorkerSlotJoinFact::NotInstalled, false, None),
        WorkerSlotState::ReservedForInstall => {
            (HostWorkerSlotJoinFact::ReservedNotInstalled, false, None)
        }
        WorkerSlotState::LoanedToFinalizer => {
            let join_fact = if state.input_join_timed_out {
                HostWorkerSlotJoinFact::TimedOutStillRunning
            } else {
                HostWorkerSlotJoinFact::LoanedToFinalizerInProgress
            };
            (join_fact, true, state.last_input_outcome_clean)
        }
        WorkerSlotState::Installed(AsyncInputSlot::Pending(_)) => {
            let join_fact = if state.input_join_timed_out {
                HostWorkerSlotJoinFact::TimedOutStillRunning
            } else {
                HostWorkerSlotJoinFact::PendingUnjoined
            };
            (join_fact, false, None)
        }
        WorkerSlotState::Installed(AsyncInputSlot::Joined {
            outcome,
            within_deadline,
            error_kind,
        }) => match outcome {
            Ok(out) => {
                let join_fact = if !*within_deadline {
                    HostWorkerSlotJoinFact::JoinedLateAfterBudget
                } else if out.clean {
                    HostWorkerSlotJoinFact::JoinedTimelyClean
                } else {
                    HostWorkerSlotJoinFact::JoinedWithWorkerError
                };
                (join_fact, false, Some(out.clean))
            }
            Err(_) => {
                let join_fact = match error_kind {
                    Some(HostAsyncInputJoinErrorKind::Cancelled) => {
                        HostWorkerSlotJoinFact::AsyncTaskCancelled
                    }
                    Some(HostAsyncInputJoinErrorKind::Panicked) | None => {
                        HostWorkerSlotJoinFact::Panicked
                    }
                };
                (join_fact, false, None)
            }
        },
    };

    let runtime_workers = HostRuntimeWorkersFacts {
        reader: HostStdReaderWorkerFact {
            slot_join: reader_slot_join,
            protocol_join: reader_slot_join.to_protocol_worker_join_fact(),
            loaned_to_finalizer: reader_loaned,
            thread_anchor_close: reader_thread_anchor_close,
            pipe_read_handle_close,
            outcome: reader_outcome,
        },
        writer: HostStdWriterWorkerFact {
            slot_join: writer_slot_join,
            protocol_join: writer_slot_join.to_protocol_worker_join_fact(),
            loaned_to_finalizer: writer_loaned,
            thread_anchor_close: writer_thread_anchor_close,
            pipe_write_handle_close,
            outcome: writer_outcome,
        },
        input: HostAsyncInputWorkerFact {
            task_completion: input_completion,
            protocol_join: input_completion.to_protocol_worker_join_fact(),
            loaned_to_finalizer: input_loaned,
            outcome_clean: input_clean,
        },
    };

    let reader_pipe_eof = if state.reader_physical_eof {
        PipeEofFact::CleanEofObserved
    } else if let Some(eof_err) = state.reader_pipe_error_fact {
        eof_err
    } else {
        PipeEofFact::NotObserved
    };

    let eof_ordering = if state.reader_eof_ordering != HostReaderEofOrderingFact::NotObserved {
        state.reader_eof_ordering
    } else if state.reader_physical_eof {
        if state.protocol_exit_code.is_some() {
            HostReaderEofOrderingFact::CleanPhysicalEofAfterBusinessExit
        } else {
            HostReaderEofOrderingFact::PhysicalEofWithoutBusinessExit
        }
    } else {
        HostReaderEofOrderingFact::NotObserved
    };

    let reader_eof_and_exit = HostReaderPipeEofAndProtocolExitFacts {
        reader_pipe_eof,
        eof_ordering,
        business_protocol_exit_code: state.protocol_exit_code.map(BusinessExitCodeI32),
    };

    let local_owner_registry_slot = if !has_local_registry {
        HostLocalRegistrySlotFact::NotRegisteredScopedOrUnbound
    } else if let Some(explicit_slot_fact) = state.local_registry_slot_fact {
        explicit_slot_fact
    } else if state.thread_close_debt.is_some() {
        HostLocalRegistrySlotFact::RetainedDueToThreadCloseDebt {
            startup_id: startup_id.0,
        }
    } else {
        HostLocalRegistrySlotFact::RetainedActiveInLocalRegistry {
            startup_id: startup_id.0,
        }
    };

    let slots_and_leases = HostSlotAndLeaseFacts {
        // Exact original host input task/writer joins plus checked original
        // input endpoint and startup aliases; no separate public bool setter.
        host_input_writer_lease: if state.v8_close_debts.iter().any(Option::is_some) {
            LeaseRetirementFact::ParkedUnresolved
        }else if state.v8_closed[1] && state.v8_closed[3] && state.v8_closed[4]
            && writer_slot_join==HostWorkerSlotJoinFact::JoinedTimelyClean
            && input_completion==HostWorkerSlotJoinFact::JoinedTimelyClean {
            LeaseRetirementFact::RetiredCleanly
        }else{LeaseRetirementFact::NotObserved},
        local_connect_helpers: [
            HostConnectHelperSlotFact::from_record(state.helper_attempts[0]),
            HostConnectHelperSlotFact::from_record(state.helper_attempts[1]),
        ],
        local_connect_lane: state.local_connect_lane_fact,
        local_owner_registry_slot,
        local_owner_registry_retirement: local_owner_registry_slot.to_local_slot_retirement_fact(),
        local_worker_admission: state.worker_admission_fact,
        external_keeper_lease: if state.native_desktop_close_debt.is_some() {
            LeaseRetirementFact::ParkedUnresolved
        }else if state.native_desktop_closed {LeaseRetirementFact::RetiredCleanly}
        else{LeaseRetirementFact::NotObserved},
        external_global_quota_lease: LeaseRetirementFact::NotObserved,
        external_adapter_scope:
            HostExternalAdapterScopeFact::UnwiredOuterAdapterNotProvenByLocalRegistry {
                release_eligible: RELEASE_ELIGIBLE,
            },
    };

    let native_debt_count=state.v8_close_debts.iter().filter(|d|d.is_some()).count()
        +usize::from(state.native_desktop_close_debt.is_some());
    let close_debt = if let Some(OriginalThreadCloseDebt {os_error,..})=state.thread_close_debt {
        CloseDebtFact::DebtRetained{debt_count:(1+native_debt_count) as u8,
            first_target:CloseDebtTarget::ChildPrimaryThreadHandle,first_os_error:Some(os_error as i32)}
    }else if let Some((index,error))=state.v8_first_close_debt {
        CloseDebtFact::DebtRetained{debt_count:native_debt_count as u8,first_target:match index {
            0=>CloseDebtTarget::HostOutputPipeReader,1=>CloseDebtTarget::HostInputPipeWriter,
            2=>CloseDebtTarget::HostRunnerProcessHandle,3|4=>CloseDebtTarget::HostStartupPipeAlias,
            _=>CloseDebtTarget::HostWorkerThreadAnchor,
        },first_os_error:Some(error as i32)}
    }else if let Some(error)=state.native_desktop_close_debt {
        CloseDebtFact::DebtRetained{debt_count:native_debt_count as u8,
            first_target:state.native_desktop_debt_target.unwrap_or(CloseDebtTarget::HostDesktopHandle),first_os_error:Some(error)}
    }else if matches!(state.thread_disposition,Some(OriginalThreadDisposition::ClosedOnCapture{..})) {
        CloseDebtFact::ZeroDebtProven
    }else{CloseDebtFact::NotObserved};

    let effective_owner_poisoned = owner_mutex_poisoned || state.owner_mutex_poison_observed;
    let controls_poisoned = state.controls_correlation_poison_observed;
    let any_poisoned = effective_owner_poisoned || controls_poisoned;

    let poison_state = if any_poisoned {
        PoisonStateFact::PoisonObserved {
            target: PoisonTarget::HostOwnerMutex,
        }
    } else {
        PoisonStateFact::NoPoisonObserved
    };

    let first_sticky_failure = if let Some(recorded) = state.first_error_fact {
        Some(recorded)
    } else if let Some(ref reason) = state.sticky_unknown {
        Some(classify_sticky_unknown_first_error(reason, state.stage))
    } else if any_poisoned {
        Some(HostFirstStickyFailureFact {
            host_kind: HostStickyFailureKind::OwnerMutexPoisoned,
            stage_when_latched: state.stage,
            protocol_first_error: FirstErrorFact::Recorded {
                kind: ProtocolStickyFailureKind::OwnerMutexPoisoned,
                stage: ProtocolFailureStage::LeaseRetirement,
                os_error: None,
                replay_policy: ReplayDisposition::NoReplaySealed,
            },
        })
    } else {
        None
    };

    let first_error = first_sticky_failure.map_or(FirstErrorFact::NoneObserved, |f| {
        f.protocol_first_error
    });

    let no_replay_sealed =
        state.no_replay_seal || state.sticky_unknown.is_some() || any_poisoned;

    let sticky_unknown_state = if no_replay_sealed {
        StickyUnknownStateFact::LatchedStickyUnknown {
            replay_policy: ReplayDisposition::NoReplaySealed,
        }
    } else {
        StickyUnknownStateFact::Clear
    };

    let debt_poison_and_sticky_unknown = HostDebtPoisonAndStickyUnknownFacts {
        close_debt,
        first_sticky_failure,
        first_error,
        poison_state,
        owner_mutex_poisoned: effective_owner_poisoned,
        controls_correlation_mutex_poisoned: controls_poisoned,
        sticky_unknown_state,
        no_replay_sealed,
        late_physical_reap_after_sticky_unknown: state.late_physical_reap_after_sticky_unknown,
    };

    let observation_availability = if effective_owner_poisoned {
        HostFactsObservationAvailability::PoisonedOwnerMutex
    } else if runtime_workers.is_any_loaned() {
        HostFactsObservationAvailability::BorrowedByFinalizer
    } else {
        HostFactsObservationAvailability::Live
    };

    HostNativeCloseFactsSnapshot {
        startup_id,
        observation_availability,
        observation_revision: 1,
        stage: state.stage,
        runner_identity,
        runner_process_wait_and_exit: state.runner_wait_and_exit_fact,
        runner_process_handle_close,
        primary_thread_disposition,
        worker_admission: state.worker_admission_fact,
        runtime_workers,
        reader_eof_and_exit,
        controls_and_writer: state.controls_and_writer_facts,
        slots_and_leases,
        debt_poison_and_sticky_unknown,
        product_scope:
            ProductCloseScopeDisposition::SingleAttemptEvidenceOnlyNotProductGlobalClose,
    }
}

fn apply_owner_poison_latch(snapshot: &mut HostNativeCloseFactsSnapshot) {
    snapshot.observation_availability = HostFactsObservationAvailability::PoisonedOwnerMutex;
    snapshot.debt_poison_and_sticky_unknown.owner_mutex_poisoned = true;
    snapshot.debt_poison_and_sticky_unknown.poison_state = PoisonStateFact::PoisonObserved {
        target: PoisonTarget::HostOwnerMutex,
    };
    snapshot.debt_poison_and_sticky_unknown.no_replay_sealed = true;
    snapshot.debt_poison_and_sticky_unknown.sticky_unknown_state =
        StickyUnknownStateFact::LatchedStickyUnknown {
            replay_policy: ReplayDisposition::NoReplaySealed,
        };
    if snapshot
        .debt_poison_and_sticky_unknown
        .first_sticky_failure
        .is_none()
    {
        let poisoned_first = HostFirstStickyFailureFact {
            host_kind: HostStickyFailureKind::OwnerMutexPoisoned,
            stage_when_latched: snapshot.stage,
            protocol_first_error: FirstErrorFact::Recorded {
                kind: ProtocolStickyFailureKind::OwnerMutexPoisoned,
                stage: ProtocolFailureStage::LeaseRetirement,
                os_error: None,
                replay_policy: ReplayDisposition::NoReplaySealed,
            },
        };
        snapshot
            .debt_poison_and_sticky_unknown
            .first_sticky_failure = Some(poisoned_first);
        snapshot.debt_poison_and_sticky_unknown.first_error = poisoned_first.protocol_first_error;
    }
}

const HOST_FACTS_MIRROR_SLOTS: usize = 8;

struct HostFactsMirrorInner {
    startup_id: StartupId,
    has_local_registry: bool,
    initial: HostNativeCloseFactsSnapshot,
    slots: [Mutex<HostNativeCloseFactsSnapshot>; HOST_FACTS_MIRROR_SLOTS],
    latest_slot: AtomicUsize,
    write_cursor: AtomicUsize,
    revision: AtomicU64,
    active_guards: AtomicUsize,
    poisoned: AtomicBool,
    retired: AtomicBool,
    local_registry_retired: AtomicBool,
}

impl HostFactsMirrorInner {
    fn read_raw_latest(&self) -> HostNativeCloseFactsSnapshot {
        let latest = self.latest_slot.load(Ordering::Acquire) % HOST_FACTS_MIRROR_SLOTS;
        if let Ok(guard) = self.slots[latest].try_lock() {
            return *guard;
        }
        for offset in 1..HOST_FACTS_MIRROR_SLOTS {
            let idx = (latest + HOST_FACTS_MIRROR_SLOTS - offset) % HOST_FACTS_MIRROR_SLOTS;
            if let Ok(guard) = self.slots[idx].try_lock() {
                return *guard;
            }
        }
        self.initial
    }

    fn publish_non_blocking(&self, mut snapshot: HostNativeCloseFactsSnapshot) {
        if self.poisoned.load(Ordering::Acquire) {
            apply_owner_poison_latch(&mut snapshot);
        }
        let rev = self.revision.fetch_add(1, Ordering::AcqRel).saturating_add(1);
        snapshot.observation_revision = rev;
        let current_latest = self.latest_slot.load(Ordering::Acquire) % HOST_FACTS_MIRROR_SLOTS;
        let start = self.write_cursor.fetch_add(1, Ordering::AcqRel);
        for offset in 0..HOST_FACTS_MIRROR_SLOTS {
            let idx = start.wrapping_add(offset) % HOST_FACTS_MIRROR_SLOTS;
            if offset + 1 < HOST_FACTS_MIRROR_SLOTS && idx == current_latest {
                continue;
            }
            if let Ok(mut guard) = self.slots[idx].try_lock() {
                *guard = snapshot;
                drop(guard);
                self.latest_slot.store(idx, Ordering::Release);
                return;
            }
        }
    }
}

/// Non-blocking publisher embedded in `ObservedOwnerState` to publish `HostNativeCloseFactsSnapshot`
/// after releasing the owner `Mutex<OwnerState>`.
pub(super) struct HostNativeCloseFactsPublisher {
    inner: Arc<HostFactsMirrorInner>,
}

impl HostNativeCloseFactsPublisher {
    pub(super) fn new(
        initial_state: &OwnerState,
        startup_id: StartupId,
        has_local_registry: bool,
    ) -> Self {
        let mut initial = build_host_native_close_facts_snapshot(
            initial_state,
            startup_id,
            has_local_registry,
            false,
        );
        initial.observation_revision = 1;
        let slots = std::array::from_fn(|_| Mutex::new(initial));
        Self {
            inner: Arc::new(HostFactsMirrorInner {
                startup_id,
                has_local_registry,
                initial,
                slots,
                latest_slot: AtomicUsize::new(0),
                write_cursor: AtomicUsize::new(1),
                revision: AtomicU64::new(1),
                active_guards: AtomicUsize::new(0),
                poisoned: AtomicBool::new(false),
                retired: AtomicBool::new(false),
                local_registry_retired: AtomicBool::new(false),
            }),
        }
    }

    pub(super) fn reader(&self) -> HostNativeCloseFactsReader {
        HostNativeCloseFactsReader {
            inner: Arc::clone(&self.inner),
        }
    }

    pub(super) fn begin_access(&self) {
        self.inner.active_guards.fetch_add(1, Ordering::AcqRel);
    }

    pub(super) fn end_access(&self) {
        self.inner.active_guards.fetch_sub(1, Ordering::AcqRel);
    }

    pub(super) fn capture_from_state(
        &self,
        state: &OwnerState,
        panicking: bool,
    ) -> HostNativeCloseFactsSnapshot {
        let poisoned = panicking
            || self.inner.poisoned.load(Ordering::Acquire)
            || state.owner_mutex_poison_observed;
        build_host_native_close_facts_snapshot(
            state,
            self.inner.startup_id,
            self.inner.has_local_registry,
            poisoned,
        )
    }

    pub(super) fn publish(&self, snapshot: HostNativeCloseFactsSnapshot) {
        self.inner.publish_non_blocking(snapshot);
    }

    pub(super) fn mark_poisoned(&self) {
        self.inner.poisoned.store(true, Ordering::Release);
        let mut latest = self.inner.read_raw_latest();
        apply_owner_poison_latch(&mut latest);
        self.inner.publish_non_blocking(latest);
    }

    pub(super) fn retire(&self) {
        self.inner.retired.store(true, Ordering::Release);
    }

    /// Called only after removing this original registry entry. Publication is
    /// infallible and cannot replace the checked retirement operation itself.
    pub(super) fn local_registry_retired(&self) {
        self.inner.local_registry_retired.store(true, Ordering::Release);
    }
}

/// Passive, cloneable, non-blocking reader for `HostNativeCloseFactsSnapshot`.
///
/// Never retains `CheckedRunnerOwner` or any OS handle, never blocks on `Mutex<OwnerState>`,
/// and remains readable after `CheckedRunnerOwner` is finalized or dropped.
#[derive(Clone)]
pub(crate) struct HostNativeCloseFactsReader {
    inner: Arc<HostFactsMirrorInner>,
}

impl fmt::Debug for HostNativeCloseFactsReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostNativeCloseFactsReader")
            .field("startup_id", &self.inner.startup_id)
            .finish_non_exhaustive()
    }
}

impl HostNativeCloseFactsReader {
    pub(crate) fn read(&self) -> HostNativeCloseFactsSnapshot {
        let mut snapshot = self.inner.read_raw_latest();
        if self.inner.local_registry_retired.load(Ordering::Acquire) {
            snapshot.slots_and_leases.local_owner_registry_slot =
                HostLocalRegistrySlotFact::ReleasedAfterPhysicalClose { startup_id:self.inner.startup_id.0 };
            snapshot.slots_and_leases.local_owner_registry_retirement = LeaseRetirementFact::RetiredCleanly;
        }
        let poisoned = self.inner.poisoned.load(Ordering::Acquire)
            || snapshot.debt_poison_and_sticky_unknown.owner_mutex_poisoned;
        let retired = self.inner.retired.load(Ordering::Acquire);
        let busy = self.inner.active_guards.load(Ordering::Acquire) > 0;
        let borrowed = snapshot.runtime_workers.is_any_loaned();

        if poisoned {
            apply_owner_poison_latch(&mut snapshot);
        } else if retired {
            snapshot.observation_availability =
                HostFactsObservationAvailability::RetiredAfterOwnerDrop;
        } else if busy {
            snapshot.observation_availability =
                HostFactsObservationAvailability::BusyOwnerGuardHeld;
        } else if borrowed {
            snapshot.observation_availability =
                HostFactsObservationAvailability::BorrowedByFinalizer;
        } else {
            snapshot.observation_availability = HostFactsObservationAvailability::Live;
        }
        snapshot
    }
}

#[cfg(test)]
mod scope_projection_tests {
    use super::*;
    use super::super::{CheckedRunnerOwner, SCOPED_CONNECT_OWNER};
    use std::time::{Duration, Instant};
    fn unlaunched_snapshot() -> HostNativeCloseFactsSnapshot {
        CheckedRunnerOwner::new(StartupId(2601), Instant::now()+Duration::from_secs(2),
            None, None, &SCOPED_CONNECT_OWNER, None).host_native_close_facts()
    }
    #[test]
    fn unobserved_input_keeper_and_external_quota_never_project_as_clean() {
        let snapshot = unlaunched_snapshot();
        let projected = snapshot.to_v8_receiver_physical_close_facts();
        assert_eq!(projected.host_input_writer_lease, LeaseRetirementFact::NotObserved);
        assert_eq!(projected.host_keeper_and_desktop_lease, LeaseRetirementFact::NotObserved);
        assert_eq!(projected.host_local_registry_quota_lease, LeaseRetirementFact::NotObserved);
        assert_eq!(projected.external_quota_scope, ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter);
        assert!(projected.runner_process_identity.is_none());assert!(projected.runner_exit_code_dword.is_none());
    }
    #[test]
    fn cached_clean_workers_and_local_retirement_do_not_invent_input_keeper_or_global_release() {
        let mut snapshot = unlaunched_snapshot();
        // Labelled cached projection fixture, no original process or native fact.
        snapshot.runtime_workers.writer.slot_join = HostWorkerSlotJoinFact::JoinedTimelyClean;
        snapshot.runtime_workers.writer.outcome = Some(HostWriterOutcomeSummary { clean:true, termination_sent:false });
        snapshot.runtime_workers.input.task_completion = HostWorkerSlotJoinFact::JoinedTimelyClean;
        snapshot.runtime_workers.input.outcome_clean = Some(true);
        snapshot.slots_and_leases.local_owner_registry_retirement = LeaseRetirementFact::RetiredCleanly;
        let projected = snapshot.to_v8_receiver_physical_close_facts();
        assert_eq!(projected.host_input_writer_lease, LeaseRetirementFact::NotProven);
        assert_eq!(projected.host_keeper_and_desktop_lease, LeaseRetirementFact::NotObserved);
        assert_eq!(projected.host_local_registry_quota_lease, LeaseRetirementFact::RetiredCleanly);
        assert_eq!(projected.external_quota_scope, ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter);
        assert!(projected.runner_exit_code_dword.is_none());
    }
    #[test]
    fn cached_loan_timeout_poison_and_sticky_input_scope_remain_unsettled() {
        let mut snapshot = unlaunched_snapshot();
        snapshot.runtime_workers.input.loaned_to_finalizer = true;
        assert_eq!(snapshot.to_v8_receiver_physical_close_facts().host_input_writer_lease, LeaseRetirementFact::ActiveOrLoaned);
        snapshot.runtime_workers.input.loaned_to_finalizer = false;
        for state in [HostWorkerSlotJoinFact::TimedOutStillRunning, HostWorkerSlotJoinFact::JoinedLateAfterBudget,
            HostWorkerSlotJoinFact::JoinedWithWorkerError, HostWorkerSlotJoinFact::Panicked, HostWorkerSlotJoinFact::AsyncTaskCancelled] {
            snapshot.runtime_workers.input.task_completion = state;
            assert_eq!(snapshot.to_v8_receiver_physical_close_facts().host_input_writer_lease, LeaseRetirementFact::ParkedUnresolved);
        }
        snapshot.runtime_workers.input.task_completion = HostWorkerSlotJoinFact::NotInstalled;
        snapshot.debt_poison_and_sticky_unknown.owner_mutex_poisoned = true;
        assert_eq!(snapshot.to_v8_receiver_physical_close_facts().host_input_writer_lease, LeaseRetirementFact::ParkedUnresolved);
        snapshot.debt_poison_and_sticky_unknown.owner_mutex_poisoned = false;
        snapshot.debt_poison_and_sticky_unknown.no_replay_sealed = true;
        assert_eq!(snapshot.to_v8_receiver_physical_close_facts().host_input_writer_lease, LeaseRetirementFact::ParkedUnresolved);
    }
    #[test]
    fn cached_local_registry_debt_is_forwarded_without_external_scope_promotion() {
        let mut snapshot = unlaunched_snapshot();
        for state in [HostLocalRegistrySlotFact::RetainedActiveInLocalRegistry {startup_id:2601},
            HostLocalRegistrySlotFact::RetainedDueToThreadCloseDebt {startup_id:2601},
            HostLocalRegistrySlotFact::ReleaseFailedRegistryLockPoisoned {startup_id:2601},
            HostLocalRegistrySlotFact::ReleaseAttemptedSlotNotFound {startup_id:2601}] {
            snapshot.slots_and_leases.local_owner_registry_retirement = state.to_local_slot_retirement_fact();
            let projected = snapshot.to_v8_receiver_physical_close_facts();
            assert_eq!(projected.host_local_registry_quota_lease, state.to_local_slot_retirement_fact());
            assert_ne!(projected.host_local_registry_quota_lease, LeaseRetirementFact::RetiredCleanly);
            assert_eq!(projected.host_keeper_and_desktop_lease, LeaseRetirementFact::NotObserved);
        }
    }
}
