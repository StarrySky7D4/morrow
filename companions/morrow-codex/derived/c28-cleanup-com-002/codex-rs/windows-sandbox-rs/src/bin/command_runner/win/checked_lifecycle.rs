//! Checked normal-close lifecycle, retained native context, and witness separation
//! for `codex-command-runner`.
//!
//! This module owns the original launched child process `HANDLE`, validated
//! `(PID, creation_time)` identity, original primary thread disposition, original
//! `JobObject`, stdout/stderr/stdin handle owners, ConPTY owner, and original
//! `std::thread::JoinHandle`s in one bounded context per runner attempt.

#![allow(dead_code)]

use codex_utils_pty::JobObject;
use codex_windows_sandbox::CHECKED_IPC_PROTOCOL_VERSION;
use codex_windows_sandbox::ConptyInstance;
use codex_windows_sandbox::ControlAdmission;
use codex_windows_sandbox::ControlLedger;
use codex_windows_sandbox::ControlOperation;
use codex_windows_sandbox::ControlResult;
use codex_windows_sandbox::ControlStatus;
use codex_windows_sandbox::ExitPayload;
use codex_windows_sandbox::FramedMessage;
use codex_windows_sandbox::IPC_PROTOCOL_VERSION;
use codex_windows_sandbox::Message;
use codex_windows_sandbox::PipeSpawnHandles;
use codex_windows_sandbox::{ProcessCreationEvidence, ProcessNotCreated};
use codex_windows_sandbox::ResizePayload;
use codex_windows_sandbox::decode_bytes;
use codex_windows_sandbox::log_note;
use codex_windows_sandbox::write_frame;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Write;
use std::os::windows::io::{AsRawHandle, IntoRawHandle};
use std::path::Path;
use std::path::PathBuf;
use std::ptr;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::DUPLICATE_SAME_ACCESS;
use windows_sys::Win32::Foundation::DuplicateHandle;
use windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE;
use windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED;
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::Storage::FileSystem::WriteFile;
use windows_sys::Win32::System::Console::COORD;
use windows_sys::Win32::System::Console::ResizePseudoConsole;
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::System::Threading::GetExitCodeProcess;
use windows_sys::Win32::System::Threading::GetProcessId;
use windows_sys::Win32::System::Threading::GetProcessTimes;
use windows_sys::Win32::System::Threading::INFINITE;
use windows_sys::Win32::System::Threading::PROCESS_INFORMATION;
use windows_sys::Win32::System::Threading::TerminateProcess;
use windows_sys::Win32::System::Threading::WaitForSingleObject;

pub(crate) const WAIT_TIMEOUT_CODE: u32 = 0x0000_0102;
pub(crate) const DEFAULT_SHUTDOWN_BUDGET: Duration = Duration::from_millis(5_000);
pub(crate) const JOB_DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(2);
pub(crate) const PIPE_WRITE_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(1);
/// Windows `STILL_ACTIVE` (`STATUS_PENDING`, `0x0000_0103` = `259`) raw exit code `DWORD`.
///
/// Valid as an actual child exit status ONLY after `WaitForSingleObject == WAIT_OBJECT_0`
/// on the verified original child process handle confirms the process exited.
pub(crate) const STILL_ACTIVE_RAW_EXIT_CODE: u32 = 259;
const MAX_IPC_FRAME_LEN: usize = 8 * 1024 * 1024;

/// RAII wrapper for an owned Win32 `HANDLE` with explicit checked close and
/// same-object `DuplicateHandle` support.
#[derive(Debug)]
pub(crate) struct OwnedWinHandle(HANDLE, Option<HandleCloseDebt>, bool);

/// Numeric evidence only. Never reconstruct, duplicate, close or retry this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HandleCloseDebt {
    pub raw_handle: usize,
    pub os_error: Option<i32>,
    pub label: &'static str,
}

impl OwnedWinHandle {
    pub(crate) fn new(handle: HANDLE) -> Self {
        Self(handle, None, false)
    }

    pub(crate) fn new_checked(handle: HANDLE, label: &'static str) -> io::Result<Self> {
        if handle == 0 || handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{label} handle is null or invalid"),
            ));
        }
        Ok(Self(handle, None, false))
    }

    pub(crate) fn raw(&self) -> HANDLE {
        self.0
    }

    pub(crate) fn is_open(&self) -> bool {
        self.0 != 0 && self.0 != INVALID_HANDLE_VALUE
    }

    pub(crate) fn is_closed_verified(&self) -> bool {
        self.2 && !self.is_open() && self.1.is_none()
    }

    pub(crate) fn into_raw(mut self) -> HANDLE {
        let handle = self.0;
        self.0 = 0;
        self.2 = false;
        handle
    }

    /// Creates a lifetime-safe duplicate handle to the SAME underlying kernel object.
    /// Never reopens by numeric PID.
    pub(crate) fn duplicate_same_access(&self, label: &'static str) -> io::Result<Self> {
        if !self.is_open() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("cannot duplicate closed {label} handle"),
            ));
        }
        let current_process = unsafe { GetCurrentProcess() };
        let mut duplicated: HANDLE = 0;
        let ok = unsafe {
            DuplicateHandle(
                current_process,
                self.0,
                current_process,
                &mut duplicated,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Self::new_checked(duplicated, label)
    }

    /// Explicitly closes the owned handle and verifies `CloseHandle != 0`.
    pub(crate) fn close_debt(&self) -> Option<HandleCloseDebt> { self.1 }

    pub(crate) fn close_checked(&mut self, label: &'static str) -> io::Result<()> {
        self.close_checked_with(label, |raw| {
            if unsafe { CloseHandle(raw) } == 0 { Err(io::Error::last_os_error()) }
            else { Ok(()) }
        })
    }

    pub(crate) fn close_checked_with<F>(&mut self, label: &'static str, close: F) -> io::Result<()>
    where F: FnOnce(HANDLE) -> io::Result<()> {
        if let Some(debt) = self.1 {
            return Err(io::Error::other(format!("uncertain close must not retry: {debt:?}")));
        }
        let raw = std::mem::replace(&mut self.0, 0);
        if raw == 0 || raw == INVALID_HANDLE_VALUE {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "handle already closed or invalid"));
        }
        self.2 = false;
        // Install conservative debt before the native boundary, including unwind.
        self.1 = Some(HandleCloseDebt { raw_handle: raw as usize, os_error: None, label });
        match close(raw) {
            Ok(()) => { self.1 = None; self.2 = true; Ok(()) }
            Err(err) => {
                self.1.as_mut().unwrap().os_error = err.raw_os_error();
                Err(err)
            }
        }
    }

    pub(crate) fn close_if_open_checked(&mut self, label: &'static str) -> io::Result<bool> {
        if self.1.is_some() {
            return Err(io::Error::other("uncertain close debt retained"));
        }
        if !self.is_open() {
            return Ok(false);
        }
        self.close_checked(label)?;
        Ok(true)
    }
}

impl Drop for OwnedWinHandle {
    fn drop(&mut self) {
        if self.0 != 0 && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

fn filetime_to_u64(ft: FILETIME) -> u64 {
    ((u64::from(ft.dwHighDateTime)) << 32) | u64::from(ft.dwLowDateTime)
}

fn query_process_creation_time_100ns(process_handle: HANDLE) -> io::Result<u64> {
    if process_handle == 0 || process_handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "process handle is null or invalid",
        ));
    }
    let mut creation: FILETIME = unsafe { std::mem::zeroed() };
    let mut exit: FILETIME = unsafe { std::mem::zeroed() };
    let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
    let mut user: FILETIME = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        GetProcessTimes(
            process_handle,
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let creation_100ns = filetime_to_u64(creation);
    if creation_100ns == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "process creation time is zero",
        ));
    }
    Ok(creation_100ns)
}

/// Immutable identity of the original launched child process, captured directly
/// from the original `PROCESS_INFORMATION` handle immediately after launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChildProcessIdentity {
    pub process_id: u32,
    pub thread_id: u32,
    pub creation_time_100ns: u64,
}

impl ChildProcessIdentity {
    pub(crate) fn validate_from_spawn(
        pi: &PROCESS_INFORMATION,
        job: &JobObject,
    ) -> io::Result<Self> {
        if pi.hProcess == 0 || pi.hProcess == INVALID_HANDLE_VALUE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "spawned child hProcess is invalid",
            ));
        }
        if pi.hThread == 0 || pi.hThread == INVALID_HANDLE_VALUE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "spawned child hThread is invalid",
            ));
        }
        if pi.dwProcessId == 0 || pi.dwThreadId == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "spawned child PROCESS_INFORMATION has zero PID or TID",
            ));
        }
        let handle_pid = unsafe { GetProcessId(pi.hProcess) };
        if handle_pid == 0 || handle_pid != pi.dwProcessId {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "spawned child PID mismatch: PROCESS_INFORMATION={} GetProcessId={handle_pid}",
                    pi.dwProcessId
                ),
            ));
        }
        let creation_time_100ns = query_process_creation_time_100ns(pi.hProcess)?;
        if !job.contains_process_handle(pi.hProcess as _)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "spawned child hProcess is not a member of the retained JobObject",
            ));
        }
        Ok(Self {
            process_id: pi.dwProcessId,
            thread_id: pi.dwThreadId,
            creation_time_100ns,
        })
    }

    pub(crate) fn verify_same_process_handle(&self, process_handle: HANDLE) -> io::Result<()> {
        let observed_pid = unsafe { GetProcessId(process_handle) };
        if observed_pid != self.process_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "process handle PID changed: expected={} observed={observed_pid}",
                    self.process_id
                ),
            ));
        }
        let observed_creation = query_process_creation_time_100ns(process_handle)?;
        if observed_creation != self.creation_time_100ns {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "process handle creation time changed",
            ));
        }
        Ok(())
    }
}

/// Tracks the original primary thread `HANDLE` from `PROCESS_INFORMATION`.
#[derive(Debug)]
pub(crate) enum OriginalThreadDisposition {
    Open(OwnedWinHandle),
    NotCreated(ProcessNotCreated),
    /// Moved-from shell only; never a native close witness.
    Transferred,
    ClosedVerified { thread_id: u32 },
    CloseFailed { thread_id: u32, os_error: Option<i32>, raw_handle: usize },
}

impl OriginalThreadDisposition {
    pub(crate) fn is_open(&self) -> bool {
        matches!(self, Self::Open(handle) if handle.is_open())
    }

    pub(crate) fn is_closed_verified(&self) -> bool {
        matches!(self, Self::ClosedVerified { .. })
    }

    fn retired_for_physical_reap(&self) -> bool {
        self.is_closed_verified() || matches!(self, Self::NotCreated(_))
    }

    pub(crate) fn close_checked(&mut self, thread_id: u32) -> io::Result<()> {
        match self {
            Self::Open(handle) => match handle.close_checked("child primary thread") {
                Ok(()) => {
                    *self = Self::ClosedVerified { thread_id };
                    Ok(())
                }
                Err(err) => {
                    let code = err.raw_os_error();
                    let raw_handle = handle.close_debt().map_or(0, |debt| debt.raw_handle);
                    *self = Self::CloseFailed {
                        raw_handle,
                        thread_id,
                        os_error: code,
                    };
                    Err(err)
                }
            },
            Self::Transferred => Err(io::Error::other("original primary thread ownership was transferred")),
            Self::NotCreated(_) => Err(io::Error::other("no primary thread was created; no close witness exists")),
            Self::ClosedVerified { .. } => Ok(()),
            Self::CloseFailed { os_error, .. } => Err(io::Error::other(format!(
                "child primary thread close previously failed: {os_error:?}"
            ))),
        }
    }
}

/// Verified exit witness of the original child process, retained even after the
/// original child process handle is closed before final Job drain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChildExitWitness {
    pub identity: ChildProcessIdentity,
    pub exit_code: i32,
    pub raw_exit_code: u32,
    pub timed_out: bool,
}

/// Non-blocking observation state of an internal synchronization lock when
/// assembling a read-only native facts snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LockObservationState {
    AcquiredClean,
    BusyNotObserved,
    PoisonedRecovered,
}

/// Creation and identity state of the original child process in this runner attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChildCreationFactState {
    /// No creation evidence or validated identity has been recorded.
    NotObserved,
    /// Proven never created via `ProcessNotCreated` matched against the original `PROCESS_INFORMATION`.
    NeverCreatedProven,
    /// Raw post-launch handles were installed without `ProcessNotCreated` proof, but
    /// `ChildProcessIdentity::validate_from_spawn` has not succeeded.
    CreatedUnvalidated {
        raw_process_id: u32,
        raw_thread_id: u32,
    },
    /// Validated original child process `(process_id, thread_id, creation_time_100ns)`.
    CreatedValidated(ChildProcessIdentity),
}

impl ChildCreationFactState {
    pub(crate) fn validated_identity(&self) -> Option<ChildProcessIdentity> {
        match *self {
            Self::CreatedValidated(identity) => Some(identity),
            _ => None,
        }
    }
}

/// Verified native child exit facts captured from the original child process handle
/// after `WaitForSingleObject == WAIT_OBJECT_0` confirmed the process exited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedChildExitFact {
    pub process_id: u32,
    pub thread_id: u32,
    pub creation_time_100ns: u64,
    pub creation_filetime_low: u32,
    pub creation_filetime_high: u32,
    pub raw_exit_code: u32,
    pub exit_code: i32,
    pub wait_confirmed_signaled: bool,
    pub timed_out: bool,
}

impl VerifiedChildExitFact {
    pub(crate) fn from_verified_witness(
        witness: ChildExitWitness,
        wait_confirmed_signaled: bool,
    ) -> Option<Self> {
        if !wait_confirmed_signaled
            || witness.timed_out
            || witness.identity.process_id == 0
            || witness.identity.creation_time_100ns == 0
        {
            return None;
        }
        Some(Self {
            process_id: witness.identity.process_id,
            thread_id: witness.identity.thread_id,
            creation_time_100ns: witness.identity.creation_time_100ns,
            creation_filetime_low: (witness.identity.creation_time_100ns & 0xFFFF_FFFF) as u32,
            creation_filetime_high: (witness.identity.creation_time_100ns >> 32) as u32,
            raw_exit_code: witness.raw_exit_code,
            exit_code: witness.exit_code,
            wait_confirmed_signaled: true,
            timed_out: false,
        })
    }
}

/// Native exit observation state for the original child process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChildExitFactState {
    /// Child exit has not been witnessed yet (e.g. created but still running or unwaited).
    NotObserved,
    /// Child was proven never created (`ProcessNotCreated`); no exit witness can exist.
    NeverCreated,
    /// Original wait timed out (`WAIT_TIMEOUT`); timeout is never an exit witness.
    WaitTimedOut,
    /// Original wait failed with a non-signaled return code.
    WaitFailed { wait_return: u32 },
    /// Original wait confirmed signaled (`WAIT_OBJECT_0`), but `GetExitCodeProcess` failed.
    ExitCodeQueryFailed { os_error: Option<i32> },
    /// An unwaited / query-only exit code (such as query-only `259` / `STILL_ACTIVE`) was
    /// rejected because original wait did not confirm the process handle was signaled.
    QueryOnlyStillActiveRejected { raw_exit_code: u32 },
    /// An exit witness candidate was rejected because its `ChildProcessIdentity` did not
    /// match the validated original child process identity.
    IdentityCorrelationRejected {
        expected: ChildProcessIdentity,
        observed: ChildProcessIdentity,
    },
    /// During late physical reaping, `WaitForSingleObject` observed `WAIT_OBJECT_0` on the
    /// original child handle, while `raw_exit_code` was not queried on the normal path.
    LatePhysicalWaitSignaledExitCodeNotQueried,
    /// Verified child exit witness backed by `WaitForSingleObject == WAIT_OBJECT_0` and
    /// `GetExitCodeProcess` on the same validated original child process handle.
    VerifiedExit(VerifiedChildExitFact),
}

/// Pure scalar evaluator for mapping an observed child exit candidate against a
/// validated original `ChildProcessIdentity` and wait proof.
///
/// Explicitly retains whether original wait confirmed signaled (`wait_return == Some(WAIT_OBJECT_0)`
/// and `!timed_out`). Raw `259` (`STILL_ACTIVE_RAW_EXIT_CODE`) is accepted ONLY after such wait proof;
/// query-only `259` or timeout can never become a `VerifiedExit`.
pub(crate) fn evaluate_child_exit_observation_scalar(
    validated_identity: Option<ChildProcessIdentity>,
    not_created_proven: bool,
    candidate_identity: ChildProcessIdentity,
    wait_return: Option<u32>,
    timed_out: bool,
    raw_exit_code: Option<u32>,
    query_os_error: Option<i32>,
) -> ChildExitFactState {
    if not_created_proven {
        return ChildExitFactState::NeverCreated;
    }
    let Some(expected) = validated_identity else {
        return ChildExitFactState::NotObserved;
    };
    if expected.process_id == 0 || expected.creation_time_100ns == 0 {
        return ChildExitFactState::NotObserved;
    }
    if candidate_identity != expected {
        return ChildExitFactState::IdentityCorrelationRejected {
            expected,
            observed: candidate_identity,
        };
    }
    if timed_out || wait_return == Some(WAIT_TIMEOUT_CODE) {
        return ChildExitFactState::WaitTimedOut;
    }
    let Some(wait_code) = wait_return else {
        return match raw_exit_code {
            Some(raw) if raw == STILL_ACTIVE_RAW_EXIT_CODE => {
                ChildExitFactState::QueryOnlyStillActiveRejected { raw_exit_code: raw }
            }
            Some(_) | None => ChildExitFactState::NotObserved,
        };
    };
    if wait_code != WAIT_OBJECT_0 {
        return ChildExitFactState::WaitFailed {
            wait_return: wait_code,
        };
    }
    let Some(raw) = raw_exit_code else {
        return ChildExitFactState::ExitCodeQueryFailed {
            os_error: query_os_error,
        };
    };
    let witness = ChildExitWitness {
        identity: expected,
        exit_code: raw as i32,
        raw_exit_code: raw,
        timed_out: false,
    };
    match VerifiedChildExitFact::from_verified_witness(witness, true) {
        Some(fact) => ChildExitFactState::VerifiedExit(fact),
        None => ChildExitFactState::NotObserved,
    }
}

/// Data-only close disposition of an original native handle (contains no raw `HANDLE` or pointer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeHandleCloseFact {
    NotObserved,
    NotConfigured,
    NeverCreated,
    Open,
    Transferred,
    ClosedVerified,
    CloseDebtRetained {
        os_error: Option<i32>,
        label: &'static str,
    },
    MissingWithoutProof,
}

/// Bounded POD snapshot of the native child process owner (`SharedChildProcessOwner`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnerChildNativeFactsSnapshot {
    pub slot_lock: LockObservationState,
    pub creation: ChildCreationFactState,
    pub exit: ChildExitFactState,
    pub wait_confirmed_signaled: bool,
    pub wait_timed_out: bool,
    pub late_physical_wait_signaled: bool,
    pub process_handle_close: NativeHandleCloseFact,
    pub child_exit_signaled_flag: bool,
    pub explicit_termination_latched: bool,
    pub late_terminate_skipped_count: usize,
    pub process_terminate_fallback_count: usize,
}

impl OwnerChildNativeFactsSnapshot {
    pub(crate) const fn unobserved_default() -> Self {
        Self {
            slot_lock: LockObservationState::AcquiredClean,
            creation: ChildCreationFactState::NotObserved,
            exit: ChildExitFactState::NotObserved,
            wait_confirmed_signaled: false,
            wait_timed_out: false,
            late_physical_wait_signaled: false,
            process_handle_close: NativeHandleCloseFact::NotObserved,
            child_exit_signaled_flag: false,
            explicit_termination_latched: false,
            late_terminate_skipped_count: 0,
            process_terminate_fallback_count: 0,
        }
    }
}

#[derive(Debug)]
struct ChildProcessSlot {
    process_handle: Option<OwnedWinHandle>,
    identity: ChildProcessIdentity,
    identity_validated: bool,
    exit_witness: Option<ChildExitWitness>,
    process_handle_closed_verified: bool,
    not_created: Option<ProcessNotCreated>,
    cached_child_facts: OwnerChildNativeFactsSnapshot,
}

/// Lifetime-safe owner for the original launched child process `HANDLE`.
///
/// Workers never hold a raw `pi.hProcess` that could be closed behind their back.
#[derive(Debug)]
pub(crate) struct SharedChildProcessOwner {
    slot: StdMutex<ChildProcessSlot>,
    child_exit_signaled: AtomicBool,
    explicit_termination_latched: AtomicBool,
    simulated_running_for_test: AtomicBool,
    force_fail_duplicate_wait_for_test: AtomicBool,
    late_terminate_skipped_count: AtomicUsize,
    process_terminate_fallback_count: AtomicUsize,
}

impl SharedChildProcessOwner {
    pub(crate) fn new(process_handle: OwnedWinHandle, identity: ChildProcessIdentity) -> Self {
        let handle_close = if process_handle.is_open() {
            NativeHandleCloseFact::Open
        } else {
            NativeHandleCloseFact::MissingWithoutProof
        };
        let identity_valid = identity.process_id != 0 && identity.creation_time_100ns != 0;
        let creation = if identity_valid {
            ChildCreationFactState::CreatedValidated(identity)
        } else if identity.process_id != 0 || identity.thread_id != 0 || process_handle.is_open() {
            ChildCreationFactState::CreatedUnvalidated {
                raw_process_id: identity.process_id,
                raw_thread_id: identity.thread_id,
            }
        } else {
            ChildCreationFactState::NotObserved
        };
        Self {
            slot: StdMutex::new(ChildProcessSlot {
                process_handle: Some(process_handle),
                identity,
                identity_validated: identity_valid,
                exit_witness: None,
                process_handle_closed_verified: false,
                not_created: None,
                cached_child_facts: OwnerChildNativeFactsSnapshot {
                    slot_lock: LockObservationState::AcquiredClean,
                    creation,
                    exit: ChildExitFactState::NotObserved,
                    wait_confirmed_signaled: false,
                    wait_timed_out: false,
                    late_physical_wait_signaled: false,
                    process_handle_close: handle_close,
                    child_exit_signaled_flag: false,
                    explicit_termination_latched: false,
                    late_terminate_skipped_count: 0,
                    process_terminate_fallback_count: 0,
                },
            }),
            child_exit_signaled: AtomicBool::new(false),
            explicit_termination_latched: AtomicBool::new(false),
            simulated_running_for_test: AtomicBool::new(false),
            force_fail_duplicate_wait_for_test: AtomicBool::new(false),
            late_terminate_skipped_count: AtomicUsize::new(0),
            process_terminate_fallback_count: AtomicUsize::new(0),
        }
    }

    /// Installs the raw launched process handle into its owner before any fallible
    /// handle or identity validation runs.
    pub(crate) fn new_unvalidated(
        process_handle: OwnedWinHandle,
        raw_process_id: u32,
        raw_thread_id: u32,
    ) -> Self {
        let handle_open = process_handle.is_open();
        let handle_close = if handle_open {
            NativeHandleCloseFact::Open
        } else {
            NativeHandleCloseFact::MissingWithoutProof
        };
        let creation = if handle_open || raw_process_id != 0 || raw_thread_id != 0 {
            ChildCreationFactState::CreatedUnvalidated {
                raw_process_id,
                raw_thread_id,
            }
        } else {
            ChildCreationFactState::NotObserved
        };
        Self {
            slot: StdMutex::new(ChildProcessSlot {
                process_handle: Some(process_handle),
                identity: ChildProcessIdentity {
                    process_id: raw_process_id,
                    thread_id: raw_thread_id,
                    creation_time_100ns: 0,
                },
                identity_validated: false,
                exit_witness: None,
                process_handle_closed_verified: false,
                not_created: None,
                cached_child_facts: OwnerChildNativeFactsSnapshot {
                    slot_lock: LockObservationState::AcquiredClean,
                    creation,
                    exit: ChildExitFactState::NotObserved,
                    wait_confirmed_signaled: false,
                    wait_timed_out: false,
                    late_physical_wait_signaled: false,
                    process_handle_close: handle_close,
                    child_exit_signaled_flag: false,
                    explicit_termination_latched: false,
                    late_terminate_skipped_count: 0,
                    process_terminate_fallback_count: 0,
                },
            }),
            child_exit_signaled: AtomicBool::new(false),
            explicit_termination_latched: AtomicBool::new(false),
            simulated_running_for_test: AtomicBool::new(false),
            force_fail_duplicate_wait_for_test: AtomicBool::new(false),
            late_terminate_skipped_count: AtomicUsize::new(0),
            process_terminate_fallback_count: AtomicUsize::new(0),
        }
    }

    fn lock_slot_recovering(&self) -> std::sync::MutexGuard<'_, ChildProcessSlot> {
        match self.slot.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                guard.cached_child_facts.slot_lock = LockObservationState::PoisonedRecovered;
                guard
            }
        }
    }

    pub(crate) fn set_validated_identity(&self, identity: ChildProcessIdentity) -> io::Result<()> {
        let mut guard = self.lock_slot_recovering();
        if guard.not_created.is_some() {
            return Err(io::Error::other("absent child cannot acquire a created-process identity"));
        }
        if identity.process_id == 0 || identity.creation_time_100ns == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "validated child identity must have non-zero PID and creation_time_100ns",
            ));
        }
        guard.identity = identity;
        guard.identity_validated = true;
        guard.cached_child_facts.creation = ChildCreationFactState::CreatedValidated(identity);
        Ok(())
    }

    pub(crate) fn is_identity_validated(&self) -> bool {
        let guard = self.lock_slot_recovering();
        guard.identity_validated
    }

    pub(crate) fn has_open_process_handle(&self) -> bool {
        let guard = self.lock_slot_recovering();
        guard
            .process_handle
            .as_ref()
            .is_some_and(OwnedWinHandle::is_open)
    }

    pub(crate) fn set_simulated_running_for_test(&self, running: bool) {
        self.simulated_running_for_test
            .store(running, Ordering::SeqCst);
    }

    pub(crate) fn set_force_fail_duplicate_wait_for_test(&self, fail: bool) {
        self.force_fail_duplicate_wait_for_test
            .store(fail, Ordering::SeqCst);
    }

    /// Checks whether the retained original native child process is still running
    /// without calling `TerminateProcess` or reopening by PID.
    pub(crate) fn is_native_process_still_running(&self) -> bool {
        if self.simulated_running_for_test.load(Ordering::SeqCst) {
            return true;
        }
        if self.child_exit_signaled.load(Ordering::SeqCst) {
            return false;
        }
        let guard = self.lock_slot_recovering();
        if guard.exit_witness.is_some() {
            return false;
        }
        let Some(ref handle) = guard.process_handle else {
            return false;
        };
        if !handle.is_open() {
            return false;
        }
        let handle_pid = unsafe { GetProcessId(handle.raw()) };
        let current_pid = unsafe { GetCurrentProcessId() };
        if handle_pid != 0 && handle_pid != current_pid {
            let wait_res = unsafe { WaitForSingleObject(handle.raw(), 0) };
            wait_res != WAIT_OBJECT_0
        } else {
            false
        }
    }

    /// Waits boundedly on the original child process handle (if it is a real external
    /// child process) without terminating it, returning `true` if the child has exited.
    pub(crate) fn wait_for_native_exit_bounded(&self, deadline: Instant) -> bool {
        if self.simulated_running_for_test.load(Ordering::SeqCst) {
            while Instant::now() < deadline {
                if !self.simulated_running_for_test.load(Ordering::SeqCst) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            return !self.simulated_running_for_test.load(Ordering::SeqCst);
        }
        if self.child_exit_signaled.load(Ordering::SeqCst) {
            return true;
        }
        let raw_handle = {
            let guard = self.lock_slot_recovering();
            if guard.exit_witness.is_some() {
                return true;
            }
            let Some(ref handle) = guard.process_handle else {
                return true;
            };
            if !handle.is_open() {
                return true;
            }
            let handle_pid = unsafe { GetProcessId(handle.raw()) };
            let current_pid = unsafe { GetCurrentProcessId() };
            if handle_pid == 0 || handle_pid == current_pid {
                return true;
            }
            handle.raw()
        };
        let now = Instant::now();
        let wait_ms = if now >= deadline {
            0
        } else {
            deadline
                .saturating_duration_since(now)
                .as_millis()
                .clamp(1, u128::from(INFINITE - 1)) as u32
        };
        let wait_res = unsafe { WaitForSingleObject(raw_handle, wait_ms) };
        if wait_res == WAIT_OBJECT_0 {
            let mut guard = self.lock_slot_recovering();
            guard.cached_child_facts.late_physical_wait_signaled = true;
            if matches!(guard.cached_child_facts.exit, ChildExitFactState::NotObserved) {
                guard.cached_child_facts.exit =
                    ChildExitFactState::LatePhysicalWaitSignaledExitCodeNotQueried;
            }
            return true;
        }
        false
    }

    pub(crate) fn identity(&self) -> io::Result<ChildProcessIdentity> {
        let guard = self
            .slot
            .lock()
            .map_err(|_| io::Error::other("child process owner lock poisoned"))?;
        if !guard.identity_validated {
            return Err(io::Error::other("child process identity was not validated"));
        }
        Ok(guard.identity)
    }

    pub(crate) fn is_child_exit_signaled(&self) -> bool {
        self.child_exit_signaled.load(Ordering::SeqCst)
    }

    pub(crate) fn mark_child_exit_signaled_for_test(&self) {
        self.child_exit_signaled.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_explicit_termination_latched(&self) -> bool {
        self.explicit_termination_latched.load(Ordering::SeqCst)
    }

    pub(crate) fn late_terminate_skipped_count(&self) -> usize {
        self.late_terminate_skipped_count.load(Ordering::SeqCst)
    }

    pub(crate) fn process_terminate_fallback_count(&self) -> usize {
        self.process_terminate_fallback_count.load(Ordering::SeqCst)
    }

    pub(crate) fn exit_witness(&self) -> io::Result<Option<ChildExitWitness>> {
        let guard = self
            .slot
            .lock()
            .map_err(|_| io::Error::other("child process owner lock poisoned"))?;
        Ok(guard.exit_witness)
    }

    pub(crate) fn is_process_handle_closed_verified(&self) -> io::Result<bool> {
        let guard = self
            .slot
            .lock()
            .map_err(|_| io::Error::other("child process owner lock poisoned"))?;
        Ok(guard.process_handle_closed_verified && guard.process_handle.is_none())
    }

    /// Records that the original wait on the child process handle timed out (`WAIT_TIMEOUT`).
    /// A timeout never produces an exit witness.
    pub(crate) fn record_wait_timeout_from_original(&self) {
        let mut guard = self.lock_slot_recovering();
        guard.cached_child_facts.wait_timed_out = true;
        if guard.exit_witness.is_none() && guard.not_created.is_none() {
            guard.cached_child_facts.wait_confirmed_signaled = false;
            guard.cached_child_facts.exit = ChildExitFactState::WaitTimedOut;
        }
    }

    /// Evaluates and records a child exit observation candidate against the validated
    /// original `ChildProcessIdentity` and wait proof (`wait_return == Some(WAIT_OBJECT_0)`).
    ///
    /// - Rejects absent (`ProcessNotCreated`) or unvalidated owners.
    /// - Rejects wrong-original identity correlation (`candidate_identity != guard.identity`).
    /// - Rejects timeouts (`timed_out` or `WAIT_TIMEOUT_CODE`) and query-only `259` (`wait_return == None`).
    /// - Accepts raw `259` (`STILL_ACTIVE_RAW_EXIT_CODE`) ONLY when `wait_return == Some(WAIT_OBJECT_0)`
    ///   and `!timed_out` confirm the original child process exited.
    pub(crate) fn record_observed_exit_candidate(
        &self,
        candidate_identity: ChildProcessIdentity,
        wait_return: Option<u32>,
        timed_out: bool,
        raw_exit_code: Option<u32>,
    ) -> io::Result<ChildExitWitness> {
        let mut guard = self
            .slot
            .lock()
            .map_err(|_| io::Error::other("child process owner lock poisoned"))?;
        let validated_id = guard.identity_validated.then_some(guard.identity);
        let evaluated = evaluate_child_exit_observation_scalar(
            validated_id,
            guard.not_created.is_some(),
            candidate_identity,
            wait_return,
            timed_out,
            raw_exit_code,
            None,
        );
        match evaluated {
            ChildExitFactState::VerifiedExit(fact) => {
                let witness = ChildExitWitness {
                    identity: guard.identity,
                    exit_code: fact.exit_code,
                    raw_exit_code: fact.raw_exit_code,
                    timed_out: false,
                };
                guard.exit_witness = Some(witness);
                guard.cached_child_facts.wait_confirmed_signaled = true;
                guard.cached_child_facts.exit = ChildExitFactState::VerifiedExit(fact);
                self.child_exit_signaled.store(true, Ordering::SeqCst);
                Ok(witness)
            }
            ChildExitFactState::NeverCreated => {
                guard.cached_child_facts.exit = ChildExitFactState::NeverCreated;
                Err(io::Error::other("child process was never created"))
            }
            ChildExitFactState::IdentityCorrelationRejected { expected, observed } => {
                if guard.exit_witness.is_none() {
                    guard.cached_child_facts.wait_confirmed_signaled = false;
                }
                guard.cached_child_facts.exit =
                    ChildExitFactState::IdentityCorrelationRejected { expected, observed };
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "exit candidate identity does not match validated original child identity",
                ))
            }
            ChildExitFactState::WaitTimedOut => {
                guard.cached_child_facts.wait_timed_out = true;
                if guard.exit_witness.is_none() {
                    guard.cached_child_facts.wait_confirmed_signaled = false;
                    guard.cached_child_facts.exit = ChildExitFactState::WaitTimedOut;
                }
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "runner process exit was not witnessed before timeout",
                ))
            }
            ChildExitFactState::QueryOnlyStillActiveRejected { raw_exit_code } => {
                if guard.exit_witness.is_none() {
                    guard.cached_child_facts.wait_confirmed_signaled = false;
                    guard.cached_child_facts.exit =
                        ChildExitFactState::QueryOnlyStillActiveRejected { raw_exit_code };
                }
                Err(io::Error::other(
                    "query-only exit code without wait proof cannot become an exit witness",
                ))
            }
            ChildExitFactState::WaitFailed { wait_return } => {
                if guard.exit_witness.is_none() {
                    guard.cached_child_facts.wait_confirmed_signaled = false;
                    guard.cached_child_facts.exit = ChildExitFactState::WaitFailed { wait_return };
                }
                Err(io::Error::other("runner process exit was not witnessed"))
            }
            ChildExitFactState::ExitCodeQueryFailed { os_error } => {
                if guard.exit_witness.is_none() {
                    guard.cached_child_facts.exit =
                        ChildExitFactState::ExitCodeQueryFailed { os_error };
                }
                Err(io::Error::other("runner actual exit code was not observed"))
            }
            ChildExitFactState::NotObserved
            | ChildExitFactState::LatePhysicalWaitSignaledExitCodeNotQueried => {
                Err(io::Error::other("child process identity was not validated"))
            }
        }
    }

    /// Duplicates a temporary wait handle to the SAME original child process so waiting
    /// does not hold the mutex while also preventing raw-handle use-after-close.
    pub(crate) fn duplicate_wait_handle(&self) -> io::Result<OwnedWinHandle> {
        if self.force_fail_duplicate_wait_for_test.load(Ordering::SeqCst) {
            return Err(io::Error::other(
                "injected duplicate_wait_handle failure for post-launch retention test",
            ));
        }
        let guard = self
            .slot
            .lock()
            .map_err(|_| io::Error::other("child process owner lock poisoned"))?;
        if !guard.identity_validated {
            return Err(io::Error::other("child process identity was not validated"));
        }
        let Some(ref handle) = guard.process_handle else {
            return Err(io::Error::other("child process handle already closed"));
        };
        guard.identity.verify_same_process_handle(handle.raw())?;
        handle.duplicate_same_access("child wait")
    }

    /// Queries and records the child exit witness from the original process handle
    /// after an actual wait witness (`final_wait == WAIT_OBJECT_0`).
    pub(crate) fn record_verified_exit_from_original(
        &self,
        final_wait: u32,
        timed_out: bool,
    ) -> io::Result<ChildExitWitness> {
        let mut guard = self
            .slot
            .lock()
            .map_err(|_| io::Error::other("child process owner lock poisoned"))?;
        if timed_out || final_wait == WAIT_TIMEOUT_CODE {
            guard.cached_child_facts.wait_timed_out = true;
            if guard.exit_witness.is_none() {
                guard.cached_child_facts.wait_confirmed_signaled = false;
                guard.cached_child_facts.exit = ChildExitFactState::WaitTimedOut;
            }
            return Err(io::Error::other("runner process exit was not witnessed"));
        }
        if final_wait != WAIT_OBJECT_0 {
            if guard.exit_witness.is_none() {
                guard.cached_child_facts.wait_confirmed_signaled = false;
                guard.cached_child_facts.exit =
                    ChildExitFactState::WaitFailed { wait_return: final_wait };
            }
            return Err(io::Error::other("runner process exit was not witnessed"));
        }
        if guard.not_created.is_some() {
            guard.cached_child_facts.exit = ChildExitFactState::NeverCreated;
            return Err(io::Error::other("child process was never created"));
        }
        if !guard.identity_validated {
            return Err(io::Error::other("child process identity was not validated"));
        }
        if let Some(existing) = guard.exit_witness {
            return Ok(existing);
        }
        let Some(ref handle) = guard.process_handle else {
            return Err(io::Error::other(
                "child process handle closed before exit witness capture",
            ));
        };
        guard.identity.verify_same_process_handle(handle.raw())?;
        let mut raw_exit: u32 = 1;
        let ok = unsafe { GetExitCodeProcess(handle.raw(), &mut raw_exit) };
        if ok == 0 {
            let os_error = io::Error::last_os_error().raw_os_error();
            guard.cached_child_facts.exit = ChildExitFactState::ExitCodeQueryFailed { os_error };
            return Err(io::Error::other("runner actual exit code was not observed"));
        }
        let witness = ChildExitWitness {
            identity: guard.identity,
            exit_code: raw_exit as i32,
            raw_exit_code: raw_exit,
            timed_out: false,
        };
        guard.exit_witness = Some(witness);
        guard.cached_child_facts.wait_confirmed_signaled = true;
        if let Some(fact) = VerifiedChildExitFact::from_verified_witness(witness, true) {
            guard.cached_child_facts.exit = ChildExitFactState::VerifiedExit(fact);
        }
        self.child_exit_signaled.store(true, Ordering::SeqCst);
        Ok(witness)
    }

    pub(crate) fn record_supplied_exit_witness_for_test(
        &self,
        witness: ChildExitWitness,
    ) -> io::Result<()> {
        self.record_observed_exit_candidate(
            witness.identity,
            Some(WAIT_OBJECT_0),
            witness.timed_out,
            Some(witness.raw_exit_code),
        )
        .map(|_| ())
    }

    /// Terminates the Job Object (or the open original child process handle if Job
    /// termination fails), never touching a closed or already-exited handle.
    pub(crate) fn terminate_job_or_open_process(&self, job: &JobObject, log_dir: Option<&Path>) {
        self.explicit_termination_latched
            .store(true, Ordering::SeqCst);
        if let Err(job_err) = job.terminate() {
            log_note(
                &format!("runner failed to terminate process tree: {job_err}"),
                log_dir,
            );
            let Ok(guard) = self.slot.lock() else {
                self.late_terminate_skipped_count
                    .fetch_add(1, Ordering::SeqCst);
                return;
            };
            if guard.exit_witness.is_some() || self.child_exit_signaled.load(Ordering::SeqCst) {
                self.late_terminate_skipped_count
                    .fetch_add(1, Ordering::SeqCst);
                return;
            }
            let Some(ref handle) = guard.process_handle else {
                self.late_terminate_skipped_count
                    .fetch_add(1, Ordering::SeqCst);
                return;
            };
            if !handle.is_open() {
                self.late_terminate_skipped_count
                    .fetch_add(1, Ordering::SeqCst);
                return;
            };
            self.process_terminate_fallback_count
                .fetch_add(1, Ordering::SeqCst);
            if unsafe { TerminateProcess(handle.raw(), 1) } == 0 {
                log_note(
                    &format!("runner failed to terminate root process: {}", unsafe {
                        GetLastError()
                    }),
                    log_dir,
                );
            }
        }
    }

    /// Checked-closes the original child process handle while retaining `identity`
    /// and `exit_witness` in the slot.
    pub(crate) fn close_process_handle_checked(&self) -> io::Result<()> {
        self.close_process_handle_checked_with(|raw| {
            if unsafe { CloseHandle(raw) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    }

    pub(crate) fn close_process_handle_checked_with<F>(&self, close: F) -> io::Result<()>
    where
        F: FnOnce(HANDLE) -> io::Result<()>,
    {
        let mut guard = self.lock_slot_recovering();
        if guard.process_handle_closed_verified && guard.process_handle.is_none() {
            guard.cached_child_facts.process_handle_close = NativeHandleCloseFact::ClosedVerified;
            return Ok(());
        }
        let Some(handle) = guard.process_handle.as_mut() else {
            guard.cached_child_facts.process_handle_close = if guard.not_created.is_some() {
                NativeHandleCloseFact::NeverCreated
            } else {
                NativeHandleCloseFact::MissingWithoutProof
            };
            return Err(io::Error::other(
                "child process handle missing before checked close",
            ));
        };
        if let Err(err) = handle.close_checked_with("original child process", close) {
            let os_error = handle
                .close_debt()
                .and_then(|d| d.os_error)
                .or_else(|| err.raw_os_error());
            guard.cached_child_facts.process_handle_close =
                NativeHandleCloseFact::CloseDebtRetained {
                    os_error,
                    label: "original child process",
                };
            return Err(err);
        }
        guard.process_handle = None;
        guard.process_handle_closed_verified = true;
        guard.cached_child_facts.process_handle_close = NativeHandleCloseFact::ClosedVerified;
        Ok(())
    }

    /// Returns a bounded, non-blocking, read-only POD snapshot of the native child
    /// facts cached in this `SharedChildProcessOwner`.
    ///
    /// Never blocks on mutex contention (`TryLockError::WouldBlock` reports `BusyNotObserved`)
    /// and never performs additional OS queries or handle operations.
    pub(crate) fn child_native_facts_snapshot(&self) -> OwnerChildNativeFactsSnapshot {
        let child_exit_signaled_flag = self.child_exit_signaled.load(Ordering::SeqCst);
        let explicit_termination_latched = self.explicit_termination_latched.load(Ordering::SeqCst);
        let late_terminate_skipped_count = self.late_terminate_skipped_count.load(Ordering::SeqCst);
        let process_terminate_fallback_count =
            self.process_terminate_fallback_count.load(Ordering::SeqCst);

        let (slot_lock, guard) = match self.slot.try_lock() {
            Ok(guard) => (LockObservationState::AcquiredClean, guard),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                (LockObservationState::PoisonedRecovered, poisoned.into_inner())
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                return OwnerChildNativeFactsSnapshot {
                    slot_lock: LockObservationState::BusyNotObserved,
                    creation: ChildCreationFactState::NotObserved,
                    exit: ChildExitFactState::NotObserved,
                    wait_confirmed_signaled: false,
                    wait_timed_out: false,
                    late_physical_wait_signaled: false,
                    process_handle_close: NativeHandleCloseFact::NotObserved,
                    child_exit_signaled_flag,
                    explicit_termination_latched,
                    late_terminate_skipped_count,
                    process_terminate_fallback_count,
                };
            }
        };

        let mut snap = guard.cached_child_facts;
        snap.slot_lock = if guard.cached_child_facts.slot_lock == LockObservationState::PoisonedRecovered {
            LockObservationState::PoisonedRecovered
        } else {
            slot_lock
        };
        if guard.not_created.is_some() {
            snap.creation = ChildCreationFactState::NeverCreatedProven;
            snap.exit = ChildExitFactState::NeverCreated;
            snap.process_handle_close = NativeHandleCloseFact::NeverCreated;
        } else {
            if guard.identity_validated
                && guard.identity.process_id != 0
                && guard.identity.creation_time_100ns != 0
            {
                snap.creation = ChildCreationFactState::CreatedValidated(guard.identity);
            }
            if let Some(ref handle) = guard.process_handle {
                if let Some(debt) = handle.close_debt() {
                    snap.process_handle_close = NativeHandleCloseFact::CloseDebtRetained {
                        os_error: debt.os_error,
                        label: debt.label,
                    };
                } else if handle.is_closed_verified() {
                    snap.process_handle_close = NativeHandleCloseFact::ClosedVerified;
                } else if handle.is_open() {
                    snap.process_handle_close = NativeHandleCloseFact::Open;
                } else {
                    snap.process_handle_close = NativeHandleCloseFact::MissingWithoutProof;
                }
            } else if guard.process_handle_closed_verified {
                snap.process_handle_close = NativeHandleCloseFact::ClosedVerified;
            } else {
                snap.process_handle_close = NativeHandleCloseFact::MissingWithoutProof;
            }
        }
        snap.child_exit_signaled_flag = child_exit_signaled_flag;
        snap.explicit_termination_latched = explicit_termination_latched;
        snap.late_terminate_skipped_count = late_terminate_skipped_count;
        snap.process_terminate_fallback_count = process_terminate_fallback_count;
        snap
    }

    #[cfg(test)]
    pub(crate) fn with_held_slot_lock_for_test<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = self.lock_slot_recovering();
        f()
    }

    #[cfg(test)]
    pub(crate) fn poison_slot_for_test(&self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = self.slot.lock().unwrap();
            panic!("intentional process_owner slot poison for test");
        }));
    }
}

/// Single-owner wrapper for the child's stdin write handle, ensuring child stdin is
/// closed at most once across `CloseStdin` control requests, write errors, and input EOF.
#[derive(Debug)]
pub(crate) struct StdinHandleOwner {
    handle: Option<OwnedWinHandle>,
    initially_open: bool,
    close_count: usize,
    write_failed: bool,
    closed_verified: bool,
    close_debt_anchor: Arc<StdMutex<Option<HandleCloseDebt>>>,
}

impl StdinHandleOwner {
    pub(crate) fn new(handle: Option<OwnedWinHandle>) -> Self {
        let initially_open = handle.as_ref().is_some_and(OwnedWinHandle::is_open);
        Self {
            handle,
            initially_open,
            close_count: 0,
            write_failed: false,
            closed_verified: false,
            close_debt_anchor: Arc::new(StdMutex::new(None)),
        }
    }

    pub(crate) fn initially_open(&self) -> bool {
        self.initially_open
    }

    pub(crate) fn is_open(&self) -> bool {
        self.handle.as_ref().is_some_and(OwnedWinHandle::is_open)
    }

    pub(crate) fn is_closed_verified(&self) -> bool {
        self.closed_verified && !self.is_open()
    }

    pub(crate) fn close_count(&self) -> usize {
        self.close_count
    }

    pub(crate) fn write_failed(&self) -> bool {
        self.write_failed
    }

    pub(crate) fn mark_write_failed(&mut self) {
        self.write_failed = true;
    }

    pub(crate) fn raw_if_open(&self) -> Option<HANDLE> {
        self.handle
            .as_ref()
            .filter(|h| h.is_open())
            .map(OwnedWinHandle::raw)
    }

    /// Closes the child stdin handle if it is still open. Never closes more than once.
    pub(crate) fn close_once(&mut self) -> io::Result<bool> {
        let Some(handle) = self.handle.as_mut() else {
            return Ok(false);
        };
        let mut anchor = self.close_debt_anchor.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(debt) = handle.close_debt() { *anchor = Some(debt); return Err(io::Error::other("stdin close debt retained")); }
        self.close_count += 1;
        *anchor = Some(HandleCloseDebt { raw_handle: handle.raw() as usize, os_error: None, label: "child stdin" });
        let closed = handle.close_checked("child stdin");
        *anchor = handle.close_debt();
        closed?;
        self.closed_verified = true;
        self.handle = None;
        Ok(true)
    }
}

/// Lifetime-safe owner for ConPTY resize operations so raw `HPCON` handles never
/// outlive `ConptyInstance`.
pub(crate) enum ConptyResizeOwner {
    Owned(StdMutex<Option<ConptyInstance>>),
    BorrowedRaw(Arc<StdMutex<Option<HANDLE>>>),
}

impl ConptyResizeOwner {
    pub(crate) fn from_instance(instance: Option<ConptyInstance>) -> Arc<Self> {
        Arc::new(Self::Owned(StdMutex::new(instance)))
    }

    pub(crate) fn from_raw_mutex(raw: Arc<StdMutex<Option<HANDLE>>>) -> Arc<Self> {
        Arc::new(Self::BorrowedRaw(raw))
    }

    pub(crate) fn try_observe_active_conpty(&self) -> (LockObservationState, Option<bool>) {
        match self {
            Self::Owned(mutex) => match mutex.try_lock() {
                Ok(guard) => (
                    LockObservationState::AcquiredClean,
                    Some(
                        guard
                            .as_ref()
                            .and_then(ConptyInstance::raw_handle)
                            .is_some(),
                    ),
                ),
                Err(std::sync::TryLockError::Poisoned(poisoned)) => (
                    LockObservationState::PoisonedRecovered,
                    Some(
                        poisoned
                            .into_inner()
                            .as_ref()
                            .and_then(ConptyInstance::raw_handle)
                            .is_some(),
                    ),
                ),
                Err(std::sync::TryLockError::WouldBlock) => {
                    (LockObservationState::BusyNotObserved, None)
                }
            },
            Self::BorrowedRaw(mutex) => match mutex.try_lock() {
                Ok(guard) => (LockObservationState::AcquiredClean, Some(guard.is_some())),
                Err(std::sync::TryLockError::Poisoned(poisoned)) => (
                    LockObservationState::PoisonedRecovered,
                    Some(poisoned.into_inner().is_some()),
                ),
                Err(std::sync::TryLockError::WouldBlock) => {
                    (LockObservationState::BusyNotObserved, None)
                }
            },
        }
    }

    pub(crate) fn has_active_conpty(&self) -> io::Result<bool> {
        match self {
            Self::Owned(mutex) => {
                let guard = mutex
                    .lock()
                    .map_err(|_| io::Error::other("runner ConPTY lifecycle lock poisoned"))?;
                Ok(guard
                    .as_ref()
                    .and_then(ConptyInstance::raw_handle)
                    .is_some())
            }
            Self::BorrowedRaw(mutex) => {
                let guard = mutex
                    .lock()
                    .map_err(|_| io::Error::other("runner ConPTY lifecycle lock poisoned"))?;
                Ok(guard.is_some())
            }
        }
    }

    pub(crate) fn resize(&self, rows: u16, cols: u16) -> Result<ControlStatus, Option<i32>> {
        let coord = COORD {
            X: cols as i16,
            Y: rows as i16,
        };
        match self {
            Self::Owned(mutex) => {
                let Ok(guard) = mutex.lock() else {
                    return Err(None);
                };
                let Some(conpty) = guard.as_ref() else {
                    return Ok(ControlStatus::Unsupported);
                };
                let Some(hpc) = conpty.raw_handle() else {
                    return Ok(ControlStatus::Unsupported);
                };
                let hr = unsafe { ResizePseudoConsole(hpc, coord) };
                if hr == 0 {
                    Ok(ControlStatus::Applied)
                } else {
                    Err(Some(hr))
                }
            }
            Self::BorrowedRaw(mutex) => {
                let Ok(guard) = mutex.lock() else {
                    return Err(None);
                };
                let Some(hpc) = guard.as_ref() else {
                    return Ok(ControlStatus::Unsupported);
                };
                let hr = unsafe { ResizePseudoConsole(*hpc, coord) };
                if hr == 0 {
                    Ok(ControlStatus::Applied)
                } else {
                    Err(Some(hr))
                }
            }
        }
    }

    pub(crate) fn close_conpty(&self) -> io::Result<()> {
        match self {
            Self::Owned(mutex) => {
                let mut guard = mutex
                    .lock()
                    .map_err(|_| io::Error::other("runner ConPTY lifecycle lock poisoned"))?;
                let _ = guard.take();
                Ok(())
            }
            Self::BorrowedRaw(mutex) => {
                let mut guard = mutex
                    .lock()
                    .map_err(|_| io::Error::other("runner ConPTY lifecycle lock poisoned"))?;
                let _ = guard.take();
                Ok(())
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        match self {
            Self::Owned(mutex) => {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _guard = mutex.lock().unwrap();
                    panic!("intentional ConptyResizeOwner poison for test");
                }));
            }
            Self::BorrowedRaw(mutex) => {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _guard = mutex.lock().unwrap();
                    panic!("intentional ConptyResizeOwner raw poison for test");
                }));
            }
        }
    }
}

/// Native thread join disposition for a retained `std::thread::JoinHandle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkerJoinState {
    Running,
    JoinedTimely,
    JoinedLateAfterBudget,
    TimedOutStillRunning,
    WaitFailed(Option<i32>),
    Panicked,
}

/// Retains an original `std::thread::JoinHandle<T>` and joins it only after
/// waiting on its native OS thread `HANDLE` via `WaitForSingleObject`.
///
/// - Never calls `JoinHandle::is_finished`.
/// - Never drops, detaches, or forgets a running `JoinHandle` on timeout or error.
/// - Reaps late native completions via `JoinHandle::join`, while recording
///   `JoinedLateAfterBudget` so late completion cannot be mistaken for timely release.
#[derive(Debug)]
pub(crate) struct RetainedWorker<T> {
    label: &'static str,
    join_handle: Option<std::thread::JoinHandle<T>>,
    outcome: Option<T>,
    state: WorkerJoinState,
}

impl<T> RetainedWorker<T> {
    pub(crate) fn new(label: &'static str, join_handle: std::thread::JoinHandle<T>) -> Self {
        Self {
            label,
            join_handle: Some(join_handle),
            outcome: None,
            state: WorkerJoinState::Running,
        }
    }

    /// v6 only, after the actual root exit witness and business Exit: interrupt a
    /// blocked child-stdin write/read on this original input thread, then join it.
    /// The input loop still drains the real host endpoint to OS EOF; cancellation
    /// is never an EOF witness and never terminates a child/descendant process.
    pub(crate) fn join_legacy_terminal_input(&mut self) -> io::Result<&T> {
        if let Some(original) = self.join_handle.as_ref() {
            let thread = original.as_raw_handle() as HANDLE;
            loop {
                let wait = unsafe { WaitForSingleObject(thread, 10) };
                if wait == WAIT_OBJECT_0 { break; }
                unsafe { windows_sys::Win32::System::IO::CancelSynchronousIo(thread); }
                if wait != WAIT_TIMEOUT_CODE {
                    // A failed native observation is still unresolved ownership.
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        self.join_legacy()
    }

    pub(crate) fn join_legacy(&mut self) -> io::Result<&T> {
        if let Some(handle) = self.join_handle.take() {
            match handle.join() {
                Ok(value) => { self.outcome = Some(value); self.state = WorkerJoinState::JoinedTimely; }
                Err(_) => { self.state = WorkerJoinState::Panicked; return Err(io::Error::other("legacy output panicked")); }
            }
        }
        self.outcome.as_ref().ok_or_else(|| io::Error::other("legacy worker outcome missing"))
    }

    pub(crate) fn state(&self) -> WorkerJoinState {
        self.state
    }

    pub(crate) fn has_retained_join_handle(&self) -> bool {
        self.join_handle.is_some()
    }

    pub(crate) fn outcome(&self) -> Option<&T> {
        self.outcome.as_ref()
    }

    /// Waits on the native OS thread `HANDLE` within `deadline`, then completes
    /// `JoinHandle::join` (including thread TLS teardown).
    pub(crate) fn join_bounded(&mut self, deadline: Instant) -> io::Result<&T> {
        if self.join_handle.is_none() {
            return match self.state {
                WorkerJoinState::JoinedTimely => self
                    .outcome
                    .as_ref()
                    .ok_or_else(|| io::Error::other("joined worker missing outcome")),
                WorkerJoinState::JoinedLateAfterBudget => Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("{} native join completed after shutdown budget", self.label),
                )),
                WorkerJoinState::Panicked => {
                    Err(io::Error::other(format!("{} panicked", self.label)))
                }
                _ => Err(io::Error::other(format!(
                    "{} join handle unavailable in state {:?}",
                    self.label, self.state
                ))),
            };
        }

        loop {
            let now = Instant::now();
            let expired_before_wait = now >= deadline;
            let wait_ms = if expired_before_wait {
                0
            } else {
                let remaining = deadline.saturating_duration_since(now);
                remaining.as_millis().clamp(1, u128::from(INFINITE - 1)) as u32
            };

            let raw_thread_handle = self
                .join_handle
                .as_ref()
                .expect("checked above")
                .as_raw_handle() as HANDLE;
            if raw_thread_handle == 0 || raw_thread_handle == INVALID_HANDLE_VALUE {
                self.state = WorkerJoinState::WaitFailed(None);
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{} native thread handle is invalid", self.label),
                ));
            }

            let wait_res = unsafe { WaitForSingleObject(raw_thread_handle, wait_ms) };
            let after_wait = Instant::now();

            if wait_res == WAIT_OBJECT_0 {
                let handle = self
                    .join_handle
                    .take()
                    .expect("join_handle present when WAIT_OBJECT_0 observed");
                match handle.join() {
                    Ok(value) => {
                        self.outcome = Some(value);
                        if expired_before_wait || after_wait >= deadline {
                            self.state = WorkerJoinState::JoinedLateAfterBudget;
                            return Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                format!(
                                    "{} native join completed after shutdown budget",
                                    self.label
                                ),
                            ));
                        }
                        self.state = WorkerJoinState::JoinedTimely;
                        return Ok(self.outcome.as_ref().expect("outcome stored"));
                    }
                    Err(_) => {
                        self.state = WorkerJoinState::Panicked;
                        return Err(io::Error::other(format!("{} panicked", self.label)));
                    }
                }
            }

            if wait_res == WAIT_TIMEOUT_CODE {
                if expired_before_wait || after_wait >= deadline {
                    // Probe once with 0ms in case the thread signaled right at deadline expiry:
                    // late native success can reap, but still returns TimedOut.
                    let final_probe = unsafe { WaitForSingleObject(raw_thread_handle, 0) };
                    if final_probe == WAIT_OBJECT_0 {
                        let handle = self.join_handle.take().expect("join_handle present");
                        match handle.join() {
                            Ok(value) => {
                                self.outcome = Some(value);
                                self.state = WorkerJoinState::JoinedLateAfterBudget;
                            }
                            Err(_) => {
                                self.state = WorkerJoinState::Panicked;
                                return Err(io::Error::other(format!("{} panicked", self.label)));
                            }
                        }
                    } else {
                        self.state = WorkerJoinState::TimedOutStillRunning;
                    }
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("{} native join timed out", self.label),
                    ));
                }
                continue;
            }

            let os_err = io::Error::last_os_error();
            self.state = WorkerJoinState::WaitFailed(os_err.raw_os_error());
            return Err(os_err);
        }
    }

    /// Physical reaping helper for unresolved parked workers.
    ///
    /// Waits on the original `JoinHandle` native OS thread `HANDLE` within `deadline`.
    /// If `WAIT_OBJECT_0` is observed, completes `JoinHandle::join` (including TLS
    /// teardown) and records `WorkerJoinState::JoinedLateAfterBudget` (or `Panicked`)
    /// so late physical closure can never become `JoinedTimely` or normal business success.
    pub(crate) fn reap_late_bounded(&mut self, deadline: Instant) -> bool {
        if self.join_handle.is_none() {
            return true;
        }
        let now = Instant::now();
        let wait_ms = if now >= deadline {
            0
        } else {
            deadline
                .saturating_duration_since(now)
                .as_millis()
                .clamp(1, u128::from(INFINITE - 1)) as u32
        };
        let raw_thread_handle = self
            .join_handle
            .as_ref()
            .expect("checked above")
            .as_raw_handle() as HANDLE;
        if raw_thread_handle == 0 || raw_thread_handle == INVALID_HANDLE_VALUE {
            self.state = WorkerJoinState::WaitFailed(None);
            return false;
        }
        let wait_res = unsafe { WaitForSingleObject(raw_thread_handle, wait_ms) };
        if wait_res == WAIT_OBJECT_0 {
            let handle = self
                .join_handle
                .take()
                .expect("join_handle present when WAIT_OBJECT_0 observed");
            match handle.join() {
                Ok(value) => {
                    self.outcome = Some(value);
                    self.state = WorkerJoinState::JoinedLateAfterBudget;
                }
                Err(_) => {
                    self.state = WorkerJoinState::Panicked;
                }
            }
            return true;
        }
        if wait_res == WAIT_TIMEOUT_CODE {
            self.state = WorkerJoinState::TimedOutStillRunning;
        } else {
            let os_err = io::Error::last_os_error();
            self.state = WorkerJoinState::WaitFailed(os_err.raw_os_error());
        }
        false
    }
}

/// Acquires a `std::sync::Mutex` within a finite `deadline` using bounded `try_lock`
/// polling so no shutdown or control write ever blocks on an unbounded `Mutex::lock`.
pub(crate) fn acquire_mutex_bounded<'a, T>(
    mutex: &'a StdMutex<T>,
    deadline: Instant,
    label: &'static str,
) -> io::Result<std::sync::MutexGuard<'a, T>> {
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("{label} mutex lock acquisition timed out before deadline"),
            ));
        }
        match mutex.try_lock() {
            Ok(guard) => {
                if Instant::now() >= deadline {
                    drop(guard);
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("{label} mutex lock acquired after deadline expired"),
                    ));
                }
                return Ok(guard);
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(io::Error::other(format!("{label} mutex lock poisoned")));
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                let now = Instant::now();
                if now >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("{label} mutex lock acquisition timed out"),
                    ));
                }
                let remaining = deadline.saturating_duration_since(now);
                std::thread::sleep(remaining.min(PIPE_WRITE_LOCK_POLL_INTERVAL));
            }
        }
    }
}

/// Explicit completion acknowledgement from the anchored native business `Exit` writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BusinessExitWriteAck {
    pub exit_code: i32,
    pub completed_at: Instant,
}

/// Typed sticky `Unknown` failure categories for checked runner lifecycles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StickyUnknownKind {
    StartupValidationFailed,
    SpawnChildFailed,
    WriteSpawnReadyFailed,
    ChildWaitFailed,
    ChildExitCodeUnavailable,
    ChildTimedOut,
    ExplicitUserTerminate,
    ConptyLockPoisoned,
    StdoutForwardingFailed,
    StdoutJoinTimedOut,
    StdoutWorkerPanicked,
    StderrForwardingFailed,
    StderrJoinTimedOut,
    StderrWorkerPanicked,
    BusinessExitGateViolation,
    BusinessExitWriteFailed,
    InputPartialFrame,
    InputFrameTooLarge,
    InputMalformedFrame,
    InputProtocolViolation,
    InputIoError,
    InputPrematureDisconnect,
    InputControlWriteFailed,
    InputStdinCloseFailed,
    InputJoinTimedOut,
    InputWorkerPanicked,
    ChildHandleCloseFailed,
    JobPreserveDescendantsInvoked,
    JobTerminateInvoked,
    JobDrainFailed,
    JobDrainTimedOut,
}

#[allow(non_upper_case_globals)]
impl StickyUnknownKind {
    pub(crate) const JobPreserveDescendantsForbidden: Self = Self::JobPreserveDescendantsInvoked;
    pub(crate) const ExplicitTerminate: Self = Self::JobTerminateInvoked;
}

/// Sticky `Unknown` error record preserving the first failure and `no_replay: true`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StickyUnknownError {
    pub kind: StickyUnknownKind,
    pub detail: String,
    pub no_replay: bool,
}

impl StickyUnknownError {
    pub(crate) fn new(kind: StickyUnknownKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            no_replay: true,
        }
    }
}

/// Thread-safe latch that preserves the FIRST sticky `Unknown` error across
/// the main thread and IO workers, recovering across mutex poison.
#[derive(Debug, Default, Clone)]
pub(crate) struct StickyUnknownLatch {
    inner: Arc<StdMutex<Option<StickyUnknownError>>>,
}

impl StickyUnknownLatch {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record_first(
        &self,
        kind: StickyUnknownKind,
        detail: impl Into<String>,
    ) -> StickyUnknownError {
        let candidate = StickyUnknownError::new(kind, detail);
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(ref existing) = *guard {
            return existing.clone();
        }
        *guard = Some(candidate.clone());
        candidate
    }

    pub(crate) fn first_error(&self) -> Option<StickyUnknownError> {
        let guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.clone()
    }

    /// Non-blocking observation of the sticky latch for read-only native fact snapshots.
    pub(crate) fn try_observe_first_error(
        &self,
    ) -> (LockObservationState, Option<StickyUnknownError>) {
        match self.inner.try_lock() {
            Ok(guard) => (LockObservationState::AcquiredClean, guard.clone()),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => (
                LockObservationState::PoisonedRecovered,
                poisoned.into_inner().clone(),
            ),
            Err(std::sync::TryLockError::WouldBlock) => {
                (LockObservationState::BusyNotObserved, None)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn with_held_lock_for_test<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        f()
    }

    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        let inner = Arc::clone(&self.inner);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard = inner.lock().unwrap();
            panic!("intentional sticky latch poison for test");
        }));
    }
}

/// Strict frame reader for the runner's input pipe that distinguishes clean 0-byte
/// frame-boundary EOF from partial headers, truncated payloads, oversized frames,
/// cancelled IO (`ERROR_OPERATION_ABORTED`), and malformed JSON.
#[derive(Debug)]
pub(crate) enum InputFrameReadError {
    PartialHeader { bytes_read: usize },
    PartialPayload { expected: usize, bytes_read: usize },
    FrameTooLarge { len: usize },
    MalformedJson(String),
    OperationAborted(io::Error),
    Io(io::Error),
}

impl InputFrameReadError {
    pub(crate) fn into_sticky_error(self) -> StickyUnknownError {
        match self {
            Self::PartialHeader { bytes_read } => StickyUnknownError::new(
                StickyUnknownKind::InputPartialFrame,
                format!("input pipe closed after partial 4-byte header ({bytes_read} bytes)"),
            ),
            Self::PartialPayload {
                expected,
                bytes_read,
            } => StickyUnknownError::new(
                StickyUnknownKind::InputPartialFrame,
                format!(
                    "input pipe closed after partial frame payload ({bytes_read}/{expected} bytes)"
                ),
            ),
            Self::FrameTooLarge { len } => StickyUnknownError::new(
                StickyUnknownKind::InputFrameTooLarge,
                format!("input frame exceeds maximum length: {len}"),
            ),
            Self::MalformedJson(err) => StickyUnknownError::new(
                StickyUnknownKind::InputMalformedFrame,
                format!("input frame JSON decode failed: {err}"),
            ),
            Self::OperationAborted(err) => StickyUnknownError::new(
                StickyUnknownKind::InputIoError,
                format!("input pipe read aborted (not clean EOF): {err}"),
            ),
            Self::Io(err) => StickyUnknownError::new(
                StickyUnknownKind::InputIoError,
                format!("input pipe read failed: {err}"),
            ),
        }
    }
}

pub(crate) fn read_input_frame_checked<R: Read>(
    reader: &mut R,
) -> Result<Option<FramedMessage>, InputFrameReadError> {
    let mut len_buf = [0u8; 4];
    let mut header_read = 0usize;
    while header_read < len_buf.len() {
        match reader.read(&mut len_buf[header_read..]) {
            Ok(0) => {
                if header_read == 0 {
                    return Ok(None);
                }
                return Err(InputFrameReadError::PartialHeader {
                    bytes_read: header_read,
                });
            }
            Ok(n) => {
                header_read += n;
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) if err.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) => {
                if header_read == 0 {
                    return Ok(None);
                }
                return Err(InputFrameReadError::PartialHeader {
                    bytes_read: header_read,
                });
            }
            Err(err) if err.raw_os_error() == Some(ERROR_OPERATION_ABORTED as i32) => {
                return Err(InputFrameReadError::OperationAborted(err));
            }
            Err(err) => return Err(InputFrameReadError::Io(err)),
        }
    }

    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_IPC_FRAME_LEN {
        return Err(InputFrameReadError::FrameTooLarge { len });
    }

    let mut payload = vec![0u8; len];
    let mut body_read = 0usize;
    while body_read < len {
        match reader.read(&mut payload[body_read..]) {
            Ok(0) => {
                return Err(InputFrameReadError::PartialPayload {
                    expected: len,
                    bytes_read: body_read,
                });
            }
            Ok(n) => {
                body_read += n;
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) if err.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) => {
                return Err(InputFrameReadError::PartialPayload {
                    expected: len,
                    bytes_read: body_read,
                });
            }
            Err(err) if err.raw_os_error() == Some(ERROR_OPERATION_ABORTED as i32) => {
                return Err(InputFrameReadError::OperationAborted(err));
            }
            Err(err) => return Err(InputFrameReadError::Io(err)),
        }
    }

    let frame = serde_json::from_slice::<FramedMessage>(&payload)
        .map_err(|err| InputFrameReadError::MalformedJson(err.to_string()))?;
    Ok(Some(frame))
}

/// Outcome returned by the runner's input worker thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InputLoopOutcome {
    pub peer_eof_observed: bool,
    pub retired_after_child_exit: bool,
    pub stdin_initially_open: bool,
    pub stdin_close_count: usize,
    pub stdin_close_debt: Option<HandleCloseDebt>,
    pub terminated_on_disconnect: bool,
    pub explicit_terminate_received: bool,
    pub sticky_error: Option<StickyUnknownError>,
}

/// Spawns the input/control worker with lifetime-safe owners and cooperative
/// post-Exit input EOF retirement on the checked v7 path.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_managed_input_worker(
    reader: File,
    stdin_owner: StdinHandleOwner,
    conpty_resize: Arc<ConptyResizeOwner>,
    job: Arc<JobObject>,
    process_owner: Arc<SharedChildProcessOwner>,
    log_dir: Option<PathBuf>,
    pipe_write: Option<Arc<StdMutex<File>>>,
    nonce: [u8; 32],
    protocol_version: u8,
    sticky_latch: StickyUnknownLatch,
) -> RetainedWorker<InputLoopOutcome> {
    let join_handle = std::thread::spawn(move || {
        run_managed_input_loop(
            reader,
            stdin_owner,
            conpty_resize,
            job,
            process_owner,
            log_dir,
            pipe_write,
            nonce,
            protocol_version,
            sticky_latch,
        )
    });
    RetainedWorker::new("runner input worker", join_handle)
}

/// v6 terminal transport retirement may interrupt a frame already being written.
/// Discard its now-irrelevant tail while witnessing actual pipe EOF. A cancellation
/// or partial frame by itself is never promoted to EOF or a checked v7 success.
fn drain_legacy_terminal_input_eof<R: Read>(reader: &mut R) -> bool {
    let mut bytes = [0u8; 8192];
    loop {
        match reader.read(&mut bytes) {
            Ok(0) => return true,
            Ok(_) => {},
            Err(error) if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) => return true,
            Err(error) if error.raw_os_error() == Some(ERROR_OPERATION_ABORTED as i32)
                || error.kind() == io::ErrorKind::Interrupted => {},
            Err(_) => return false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_managed_input_loop(
    reader: File,
    stdin_owner: StdinHandleOwner,
    conpty_resize: Arc<ConptyResizeOwner>,
    job: Arc<JobObject>,
    process_owner: Arc<SharedChildProcessOwner>,
    log_dir: Option<PathBuf>,
    pipe_write: Option<Arc<StdMutex<File>>>,
    nonce: [u8; 32],
    protocol_version: u8,
    sticky_latch: StickyUnknownLatch,
) -> InputLoopOutcome {
    run_managed_input_loop_with_runtime(reader,stdin_owner,conpty_resize,job,process_owner,
        log_dir,pipe_write,nonce,protocol_version,sticky_latch,None)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_managed_input_loop_with_runtime<R: Read>(
    mut reader: R,
    mut stdin_owner: StdinHandleOwner,
    conpty_resize: Arc<ConptyResizeOwner>,
    job: Arc<JobObject>,
    process_owner: Arc<SharedChildProcessOwner>,
    log_dir: Option<PathBuf>,
    pipe_write: Option<Arc<StdMutex<File>>>,
    nonce: [u8; 32],
    protocol_version: u8,
    sticky_latch: StickyUnknownLatch,
    runtime: Option<Arc<codex_windows_sandbox::NativeCloseRuntime>>,
) -> InputLoopOutcome {
    let mut ledger = ControlLedger::new(nonce);
    let mut peer_eof_observed = false;
    let mut retired_after_child_exit = false;
    let mut terminated_on_disconnect = false;
    let mut explicit_terminate_received = false;

        loop {
            let next = if let Some(runtime)=runtime.as_ref() {
                match runtime.read_message(&mut reader) {
                    Ok(None)=>Ok(None),Ok(Some(Some(message)))=>Ok(Some(message)),
                    // Authenticated closeACK is terminal metadata only. Keep
                    // THIS original reader until a subsequent real pipe EOF.
                    Ok(Some(None))=>continue,
                    Err(error)=>Err(InputFrameReadError::Io(error)),
                }
            } else { read_input_frame_checked(&mut reader) };
            let msg = match next {
                Ok(Some(frame)) => frame,
                Ok(None) => {
                    peer_eof_observed = true;
                    if protocol_version >= CHECKED_IPC_PROTOCOL_VERSION {
                        if process_owner.is_child_exit_signaled() {
                            // Normal cooperative retirement after known child exit:
                            // never terminate the Job or process on post-Exit input EOF.
                            retired_after_child_exit = true;
                        } else {
                            sticky_latch.record_first(
                                StickyUnknownKind::InputPrematureDisconnect,
                                "checked input pipe reached EOF before child exit was witnessed",
                            );
                        }
                    } else if !process_owner.is_child_exit_signaled() {
                        terminated_on_disconnect = true;
                        process_owner.terminate_job_or_open_process(&job, log_dir.as_deref());
                    } else {
                        retired_after_child_exit = true;
                    }
                    break;
                }
                Err(read_err) => {
                    if protocol_version >= CHECKED_IPC_PROTOCOL_VERSION {
                        let sticky = read_err.into_sticky_error();
                        sticky_latch.record_first(sticky.kind, sticky.detail);
                    } else if process_owner.is_child_exit_signaled() {
                        peer_eof_observed = drain_legacy_terminal_input_eof(&mut reader);
                        retired_after_child_exit = peer_eof_observed;
                    } else {
                        terminated_on_disconnect = true;
                        process_owner.terminate_job_or_open_process(&job, log_dir.as_deref());
                    }
                    break;
                }
            };

            if protocol_version == IPC_PROTOCOL_VERSION && process_owner.is_child_exit_signaled() {
                peer_eof_observed = drain_legacy_terminal_input_eof(&mut reader);
                retired_after_child_exit = peer_eof_observed;
                break;
            }

            if msg.version != protocol_version {
                if protocol_version >= CHECKED_IPC_PROTOCOL_VERSION {
                    sticky_latch.record_first(
                        StickyUnknownKind::InputProtocolViolation,
                        format!(
                            "input frame version mismatch: expected={protocol_version} got={}",
                            msg.version
                        ),
                    );
                } else {
                    terminated_on_disconnect = true;
                    process_owner.terminate_job_or_open_process(&job, log_dir.as_deref());
                }
                break;
            }

            let mut checked = None;
            let mut control_status = ControlStatus::Unknown;
            let mut os_error = None;
            let message = match msg.message {
                Message::ControlRequest { payload }
                    if protocol_version >= CHECKED_IPC_PROTOCOL_VERSION =>
                {
                    match ledger.admit(&payload) {
                        ControlAdmission::Observe(result) => {
                            let Some(pipe_write) = &pipe_write else {
                                sticky_latch.record_first(
                                    StickyUnknownKind::InputControlWriteFailed,
                                    "missing output writer for observed control result",
                                );
                                break;
                            };
                            let lock_deadline = Instant::now() + DEFAULT_SHUTDOWN_BUDGET;
                            let sent = acquire_mutex_bounded(
                                pipe_write,
                                lock_deadline,
                                "runner observed control result writer",
                            )
                            .ok()
                            .is_some_and(|mut writer| {
                                codex_windows_sandbox::write_runtime_message(
                                    &mut *writer,
                                    &FramedMessage {
                                        version: protocol_version,
                                        message: Message::ControlResult { payload: result },
                                    },
                                    runtime.as_deref(),
                                )
                                .is_ok()
                            });
                            if !sent {
                                sticky_latch.record_first(
                                    StickyUnknownKind::InputControlWriteFailed,
                                    "failed to write observed control result frame",
                                );
                                break;
                            }
                            continue;
                        }
                        ControlAdmission::Apply => {}
                    }
                    let operation = payload.op;
                    checked = Some(payload.clone());
                    match operation {
                        ControlOperation::CloseStdin => Message::CloseStdin {
                            payload: Default::default(),
                        },
                        ControlOperation::Resize => Message::Resize {
                            payload: ResizePayload {
                                rows: payload.rows,
                                cols: payload.cols,
                            },
                        },
                    }
                }
                message => message,
            };

            if protocol_version >= CHECKED_IPC_PROTOCOL_VERSION
                && checked.is_none()
                && matches!(message, Message::CloseStdin { .. } | Message::Resize { .. })
            {
                sticky_latch.record_first(
                    StickyUnknownKind::InputProtocolViolation,
                    "uncorrelated raw CloseStdin/Resize frame on checked control pipe",
                );
                break;
            }

            match message {
                Message::Stdin { payload } => {
                    let Ok(bytes) = decode_bytes(&payload.data_b64) else {
                        if protocol_version >= CHECKED_IPC_PROTOCOL_VERSION {
                            sticky_latch.record_first(
                                StickyUnknownKind::InputMalformedFrame,
                                "invalid base64 in Stdin payload",
                            );
                            break;
                        }
                        continue;
                    };
                    if let Some(handle) = stdin_owner.raw_if_open() {
                        let mut offset = 0usize;
                        while offset < bytes.len() {
                            let chunk = &bytes[offset..];
                            let chunk_len = chunk.len().min(u32::MAX as usize);
                            let mut written = 0u32;
                            let ok = unsafe {
                                WriteFile(
                                    handle,
                                    chunk.as_ptr(),
                                    chunk_len as u32,
                                    &mut written,
                                    ptr::null_mut(),
                                )
                            };
                            if ok == 0 {
                                log_note(
                                    &format!(
                                        "runner stdin write failed after {offset} bytes: {}",
                                        unsafe { GetLastError() }
                                    ),
                                    log_dir.as_deref(),
                                );
                                stdin_owner.mark_write_failed();
                                if let Err(err) = stdin_owner.close_once()
                                    && protocol_version >= CHECKED_IPC_PROTOCOL_VERSION
                                {
                                    sticky_latch.record_first(
                                        StickyUnknownKind::InputStdinCloseFailed,
                                        format!("closing child stdin after write failure failed: {err}"),
                                    );
                                }
                                break;
                            }
                            if written == 0 {
                                log_note(
                                    "runner stdin write made no progress; closing child stdin",
                                    log_dir.as_deref(),
                                );
                                stdin_owner.mark_write_failed();
                                if let Err(err) = stdin_owner.close_once()
                                    && protocol_version >= CHECKED_IPC_PROTOCOL_VERSION
                                {
                                    sticky_latch.record_first(
                                        StickyUnknownKind::InputStdinCloseFailed,
                                        format!("closing child stdin after zero-progress write failed: {err}"),
                                    );
                                }
                                break;
                            }
                            offset += written as usize;
                        }
                    }
                }
                Message::CloseStdin { .. } => {
                    if stdin_owner.is_open() {
                        match stdin_owner.close_once() {
                            Ok(true) => {
                                control_status = if stdin_owner.write_failed() {
                                    ControlStatus::Unknown
                                } else {
                                    ControlStatus::Applied
                                };
                            }
                            Ok(false) => {
                                if !stdin_owner.write_failed() {
                                    control_status = ControlStatus::Unsupported;
                                }
                            }
                            Err(err) => {
                                os_error = err.raw_os_error();
                                if protocol_version >= CHECKED_IPC_PROTOCOL_VERSION {
                                    sticky_latch.record_first(
                                        StickyUnknownKind::InputStdinCloseFailed,
                                        format!("CloseStdin CloseHandle failed: {err}"),
                                    );
                                }
                            }
                        }
                    } else if !stdin_owner.write_failed() {
                        control_status = ControlStatus::Unsupported;
                    }
                }
                Message::Resize {
                    payload: ResizePayload { rows, cols },
                } => match conpty_resize.resize(rows, cols) {
                    Ok(status) => {
                        control_status = status;
                    }
                    Err(err_code) => {
                        os_error = err_code;
                    }
                },
                Message::Terminate { .. } => {
                    explicit_terminate_received = true;
                    if protocol_version >= CHECKED_IPC_PROTOCOL_VERSION {
                        sticky_latch.record_first(
                            StickyUnknownKind::ExplicitUserTerminate,
                            "explicit Terminate control frame received",
                        );
                    }
                    process_owner.terminate_job_or_open_process(&job, log_dir.as_deref());
                }
                Message::Hello { .. }
                | Message::HelloAck { .. }
                | Message::ControlRequest { .. }
                | Message::ControlResult { .. } => {
                    if protocol_version >= CHECKED_IPC_PROTOCOL_VERSION {
                        sticky_latch.record_first(
                            StickyUnknownKind::InputProtocolViolation,
                            "unexpected handshake/control frame during active input loop",
                        );
                    } else {
                        terminated_on_disconnect = true;
                        process_owner.terminate_job_or_open_process(&job, log_dir.as_deref());
                    }
                    break;
                }
                Message::SpawnRequest { .. }
                | Message::SpawnReady { .. }
                | Message::Output { .. }
                | Message::Exit { .. }
                | Message::Error { .. } => {
                    if protocol_version >= CHECKED_IPC_PROTOCOL_VERSION {
                        sticky_latch.record_first(
                            StickyUnknownKind::InputProtocolViolation,
                            "unexpected server/startup frame on checked runner input pipe",
                        );
                        break;
                    }
                }
            }

            if let Some(request) = checked {
                let result = ControlResult::new(request, control_status, os_error);
                if !ledger.finish(result.clone()) {
                    sticky_latch.record_first(
                        StickyUnknownKind::InputProtocolViolation,
                        "control ledger rejected finish transition",
                    );
                    break;
                }
                let Some(pipe_write) = &pipe_write else {
                    sticky_latch.record_first(
                        StickyUnknownKind::InputControlWriteFailed,
                        "missing output writer for finished control result",
                    );
                    break;
                };
                let lock_deadline = Instant::now() + DEFAULT_SHUTDOWN_BUDGET;
                let sent = acquire_mutex_bounded(
                    pipe_write,
                    lock_deadline,
                    "runner finished control result writer",
                )
                .ok()
                .is_some_and(|mut writer| {
                    codex_windows_sandbox::write_runtime_message(
                        &mut *writer,
                        &FramedMessage {
                            version: protocol_version,
                            message: Message::ControlResult { payload: result },
                        },
                        runtime.as_deref(),
                    )
                    .is_ok()
                });
                if !sent {
                    sticky_latch.record_first(
                        StickyUnknownKind::InputControlWriteFailed,
                        "failed to write finished control result frame",
                    );
                    break;
                }
            }
        }

        if stdin_owner.is_open()
            && let Err(err) = stdin_owner.close_once()
            && protocol_version >= CHECKED_IPC_PROTOCOL_VERSION
        {
            sticky_latch.record_first(
                StickyUnknownKind::InputStdinCloseFailed,
                format!("closing child stdin during input worker retirement failed: {err}"),
            );
        }

    InputLoopOutcome {
        peer_eof_observed,
        retired_after_child_exit,
        stdin_initially_open: stdin_owner.initially_open(),
        stdin_close_count: stdin_owner.close_count(),
        stdin_close_debt: stdin_owner.handle.as_ref().and_then(OwnedWinHandle::close_debt),
        terminated_on_disconnect,
        explicit_terminate_received,
        sticky_error: sticky_latch.first_error(),
    }
}

/// Final active-zero witness on the SAME retained original `JobObject`.
/// Preserves the actual `total_processes` count without faking `total = 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JobActiveZeroWitness {
    pub total_processes: u32,
    pub active_processes: u32,
    pub total_terminated_processes: u32,
}

impl JobActiveZeroWitness {
    pub(crate) fn is_single_process_only(&self) -> bool {
        self.total_processes == 1
            && self.active_processes == 0
            && self.total_terminated_processes == 0
    }
}

/// Complete normal-close witness proving every separated lifecycle stage succeeded
/// in order without forced kill or `preserve_descendants`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NormalCloseWitness {
    pub child_exit: ChildExitWitness,
    pub stdout_eof_and_joined: bool,
    pub stderr_eof_and_joined: bool,
    pub business_exit_sent: bool,
    pub input_peer_eof_and_joined: bool,
    pub stdin_closed_once: bool,
    pub child_handles_closed: bool,
    pub job_active_zero: JobActiveZeroWitness,
    pub runner_native_exit_ready: bool,
}

/// Explicit state machine tracking the 7 separated witnesses of a checked runner lifecycle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CheckedLifecycleLedger {
    pub child_exit: Option<ChildExitWitness>,
    pub stdout_eof_and_joined: bool,
    pub stderr_eof_and_joined: bool,
    pub business_exit_sent: bool,
    pub input_peer_eof_and_joined: bool,
    pub stdin_closed_once: bool,
    pub child_handles_closed: bool,
    pub job_active_zero: Option<JobActiveZeroWitness>,
    pub runner_native_exit_ready: bool,
}

impl CheckedLifecycleLedger {
    /// Verifies that business `Message::Exit` is strictly gated on valid child native exit
    /// and real stdout/stderr EOF + native joins, with no sticky failure or enforced termination.
    pub(crate) fn authorize_business_exit(
        &self,
        sticky_latch: &StickyUnknownLatch,
        explicit_termination: bool,
    ) -> Result<ExitPayload, StickyUnknownError> {
        if let Some(err) = sticky_latch.first_error() {
            return Err(err);
        }
        if explicit_termination {
            return Err(sticky_latch.record_first(
                StickyUnknownKind::ExplicitUserTerminate,
                "cannot authorize clean business Exit after explicit termination",
            ));
        }
        let Some(child_exit) = self.child_exit else {
            return Err(sticky_latch.record_first(
                StickyUnknownKind::BusinessExitGateViolation,
                "business Exit rejected: child native exit witness missing",
            ));
        };
        if child_exit.timed_out {
            return Err(sticky_latch.record_first(
                StickyUnknownKind::ChildTimedOut,
                "business Exit rejected: child wait timed out",
            ));
        }
        if !self.stdout_eof_and_joined {
            return Err(sticky_latch.record_first(
                StickyUnknownKind::BusinessExitGateViolation,
                "business Exit rejected: stdout EOF and native join not witnessed",
            ));
        }
        if !self.stderr_eof_and_joined {
            return Err(sticky_latch.record_first(
                StickyUnknownKind::BusinessExitGateViolation,
                "business Exit rejected: stderr EOF and native join not witnessed",
            ));
        }
        Ok(ExitPayload {
            exit_code: child_exit.exit_code,
            timed_out: false,
        })
    }

    pub(crate) fn into_normal_close_witness(&self) -> Option<NormalCloseWitness> {
        let child_exit = self.child_exit?;
        let job_active_zero = self.job_active_zero?;
        if !self.stdout_eof_and_joined
            || !self.stderr_eof_and_joined
            || !self.business_exit_sent
            || !self.input_peer_eof_and_joined
            || !self.stdin_closed_once
            || !self.child_handles_closed
            || !self.runner_native_exit_ready
        {
            return None;
        }
        Some(NormalCloseWitness {
            child_exit,
            stdout_eof_and_joined: self.stdout_eof_and_joined,
            stderr_eof_and_joined: self.stderr_eof_and_joined,
            business_exit_sent: self.business_exit_sent,
            input_peer_eof_and_joined: self.input_peer_eof_and_joined,
            stdin_closed_once: self.stdin_closed_once,
            child_handles_closed: self.child_handles_closed,
            job_active_zero,
            runner_native_exit_ready: self.runner_native_exit_ready,
        })
    }
}

fn observe_owned_handle_close_fact(handle_opt: Option<&OwnedWinHandle>) -> NativeHandleCloseFact {
    match handle_opt {
        Some(handle) => {
            if let Some(debt) = handle.close_debt() {
                NativeHandleCloseFact::CloseDebtRetained {
                    os_error: debt.os_error,
                    label: debt.label,
                }
            } else if handle.is_open() {
                NativeHandleCloseFact::Open
            } else if handle.is_closed_verified() {
                NativeHandleCloseFact::ClosedVerified
            } else {
                NativeHandleCloseFact::MissingWithoutProof
            }
        }
        None => NativeHandleCloseFact::MissingWithoutProof,
    }
}

/// Non-blocking observation of child stdin close-debt state across context and input worker.
/// Contains zero raw handle values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StdinCloseDebtObservation {
    NoAnchorInstalled,
    ObservedNone,
    ObservedDebt {
        os_error: Option<i32>,
        label: &'static str,
    },
    ObservedAfterPoison {
        has_debt: bool,
        os_error: Option<i32>,
    },
    BusyNotObserved,
}

/// Ownership and observation provenance of the original child stdin write handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StdinOwnershipObservationState {
    HeldInContext,
    TransferredToRunningInputWorker,
    ObservedFromJoinedInputWorker,
    MissingWithoutWorkerOutcome,
}

/// Plain-Old-Data (`Copy`) snapshot of original child stdin closure and close-debt facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChildStdinFactSnapshot {
    pub ownership_state: StdinOwnershipObservationState,
    pub initially_open: Option<bool>,
    pub close_count: Option<usize>,
    pub closed_once_verified: bool,
    pub write_failed: Option<bool>,
    pub close_debt: StdinCloseDebtObservation,
}

/// Plain-Old-Data (`Copy`) snapshot of a single child output stream (`stdout` or `stderr`).
///
/// Reader EOF or thread completion alone never sets `timely_eof_and_joined_verified`;
/// `JoinedTimely` with `Ok(())` on the retained worker is required.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputStreamFactSnapshot {
    pub configured: bool,
    pub join_state: Option<WorkerJoinState>,
    pub eof_or_completion_observed: bool,
    pub timely_eof_and_joined_verified: bool,
    pub reader_io_error_observed: bool,
    pub reader_handle_close: NativeHandleCloseFact,
}

/// Plain-Old-Data (`Copy`) snapshot of the runner's managed input/control worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InputWorkerFactSnapshot {
    pub join_state: Option<WorkerJoinState>,
    pub peer_eof_observed: bool,
    pub retired_after_child_exit: bool,
    pub timely_peer_eof_and_joined_verified: bool,
    pub terminated_on_disconnect: bool,
    pub explicit_terminate_received: bool,
    pub worker_sticky_error_kind: Option<StickyUnknownKind>,
}

/// Completion state for the future final V8 receipt writer in this runner process.
///
/// Must NEVER claim joined while a future receipt is pending or about to be sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FinalReceiptWriterFactState {
    NotStartedPendingFutureReceiptSend,
    PendingCurrentReceiptFlushAndClose,
    JoinedTimelyAfterFlush,
    FailedOrRetained,
}

/// Plain-Old-Data (`Copy`) snapshot of ConPTY control state and runner frame/Exit writers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ControlAndWriterFactSnapshot {
    pub conpty_lock: LockObservationState,
    pub conpty_active: Option<bool>,
    pub frame_write_join_state: Option<WorkerJoinState>,
    pub frame_write_succeeded: Option<bool>,
    pub business_exit_write_join_state: Option<WorkerJoinState>,
    pub business_exit_writer_joined_and_acked: bool,
    pub business_exit_ack_exit_code: Option<i32>,
    pub final_receipt_writer_state: FinalReceiptWriterFactState,
}

/// Recorded state of `JobObject::descendants_preserved()` at original lifecycle checkpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobPreserveDescendantsFactState {
    NotObserved,
    VerifiedNotPreserved,
    PreserveDescendantsInvoked,
    QueryFailed,
}

/// Classifies whether extra processes beyond the single launched root child were
/// observed in `JobObject` accounting.
///
/// Note: `codex_utils_pty::JobObject` exposes aggregate accounting counts and PID list,
/// not per-descendant creation FILETIME or parent-child lineage proof. When
/// `total_processes > 1` or `total_terminated_processes > 0`, origin remains explicitly
/// unproven (`ExtraMembersObservedOriginNotProvenByJobHelper`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExtraJobMemberOriginFact {
    SingleProcessOnlyVerified,
    ExtraMembersObservedOriginNotProvenByJobHelper {
        total_processes: u32,
        total_terminated_processes: u32,
    },
}

/// Recorded state of the original `JobObject` active-zero drain or late physical reap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobActiveZeroFactState {
    NotObserved,
    ActiveZeroVerified {
        witness: JobActiveZeroWitness,
        extra_member_origin: ExtraJobMemberOriginFact,
    },
    LatePhysicalActiveZeroObserved {
        total_processes: u32,
        total_terminated_processes: u32,
        extra_member_origin: ExtraJobMemberOriginFact,
    },
    DrainTimedOut,
    DrainFailed {
        last_total_processes: Option<u32>,
        last_active_processes: Option<u32>,
        last_terminated_processes: Option<u32>,
    },
}

/// Plain-Old-Data (`Copy`) snapshot of the retained original `JobObject` facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JobFactSnapshot {
    pub job_present: bool,
    pub admission_verified: bool,
    pub admitted_child_identity: Option<ChildProcessIdentity>,
    pub preserve_descendants_state: JobPreserveDescendantsFactState,
    pub terminate_invoked: bool,
    pub active_zero_state: JobActiveZeroFactState,
}

/// Plain-Old-Data (`Copy`) snapshot of the sticky first-error latch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FirstErrorFactSnapshot {
    pub latch_lock: LockObservationState,
    pub first_error_kind: Option<StickyUnknownKind>,
    pub no_replay_latched: bool,
}

/// Resources owned exclusively by the host process (host-side input pipe writer/keeper,
/// host-side output pipe reader, parent-owned private desktop lifetime owner, and host
/// quota/ACL keepers) are explicitly out of scope of this runner-side snapshot and
/// require host-owner integration. They are never marked retired here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostOwnedResourceScopeFact {
    OutOfRunnerScopeRequiresHostOwnerIntegration,
}

/// Finite, bounded, Plain-Old-Data (`Copy`) snapshot of child native lifecycle facts
/// attached to the SAME original `RetainedRunnerContext` / `SharedChildProcessOwner`.
///
/// Contains zero handles, tokens, paths, heap strings, or cleanup authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChildNativeFactsSnapshot {
    pub owner: OwnerChildNativeFactsSnapshot,
    pub primary_thread_handle_close: NativeHandleCloseFact,
    pub stdout: OutputStreamFactSnapshot,
    pub stderr: OutputStreamFactSnapshot,
    pub input: InputWorkerFactSnapshot,
    pub stdin: ChildStdinFactSnapshot,
    pub control_and_writers: ControlAndWriterFactSnapshot,
    pub job: JobFactSnapshot,
    pub first_error: FirstErrorFactSnapshot,
    pub auxiliary_close_debt_count: usize,
    pub pipe_spawn_close_debts: [Option<i32>; 4],
    pub pipe_spawn_close_debt_count: usize,
    pub first_close_debt_os_error: Option<i32>,
    pub has_any_handle_close_debt: bool,
    pub running_worker_count: usize,
    pub late_reap_observed: bool,
    pub normal_close_completed: bool,
    pub runner_native_exit_ready: bool,
    pub host_input_pipe_writer_keeper: HostOwnedResourceScopeFact,
    pub host_output_pipe_reader: HostOwnedResourceScopeFact,
    pub host_private_desktop_owner: HostOwnedResourceScopeFact,
    pub host_quota_and_acl_keepers: HostOwnedResourceScopeFact,
}

impl ChildNativeFactsSnapshot {
    pub(crate) const fn unobserved_default() -> Self {
        Self {
            owner: OwnerChildNativeFactsSnapshot::unobserved_default(),
            primary_thread_handle_close: NativeHandleCloseFact::MissingWithoutProof,
            stdout: OutputStreamFactSnapshot {
                configured: false,
                join_state: None,
                eof_or_completion_observed: false,
                timely_eof_and_joined_verified: false,
                reader_io_error_observed: false,
                reader_handle_close: NativeHandleCloseFact::MissingWithoutProof,
            },
            stderr: OutputStreamFactSnapshot {
                configured: false,
                join_state: None,
                eof_or_completion_observed: false,
                timely_eof_and_joined_verified: false,
                reader_io_error_observed: false,
                reader_handle_close: NativeHandleCloseFact::NotConfigured,
            },
            input: InputWorkerFactSnapshot {
                join_state: None,
                peer_eof_observed: false,
                retired_after_child_exit: false,
                timely_peer_eof_and_joined_verified: false,
                terminated_on_disconnect: false,
                explicit_terminate_received: false,
                worker_sticky_error_kind: None,
            },
            stdin: ChildStdinFactSnapshot {
                ownership_state: StdinOwnershipObservationState::MissingWithoutWorkerOutcome,
                initially_open: None,
                close_count: None,
                closed_once_verified: false,
                write_failed: None,
                close_debt: StdinCloseDebtObservation::NoAnchorInstalled,
            },
            control_and_writers: ControlAndWriterFactSnapshot {
                conpty_lock: LockObservationState::AcquiredClean,
                conpty_active: None,
                frame_write_join_state: None,
                frame_write_succeeded: None,
                business_exit_write_join_state: None,
                business_exit_writer_joined_and_acked: false,
                business_exit_ack_exit_code: None,
                final_receipt_writer_state:
                    FinalReceiptWriterFactState::NotStartedPendingFutureReceiptSend,
            },
            job: JobFactSnapshot {
                job_present: false,
                admission_verified: false,
                admitted_child_identity: None,
                preserve_descendants_state: JobPreserveDescendantsFactState::NotObserved,
                terminate_invoked: false,
                active_zero_state: JobActiveZeroFactState::NotObserved,
            },
            first_error: FirstErrorFactSnapshot {
                latch_lock: LockObservationState::AcquiredClean,
                first_error_kind: None,
                no_replay_latched: false,
            },
            auxiliary_close_debt_count: 0,
            pipe_spawn_close_debts: [None; 4],
            pipe_spawn_close_debt_count: 0,
            first_close_debt_os_error: None,
            has_any_handle_close_debt: false,
            running_worker_count: 0,
            late_reap_observed: false,
            normal_close_completed: false,
            runner_native_exit_ready: false,
            host_input_pipe_writer_keeper:
                HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration,
            host_output_pipe_reader:
                HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration,
            host_private_desktop_owner:
                HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration,
            host_quota_and_acl_keepers:
                HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration,
        }
    }
}

impl Default for ChildNativeFactsSnapshot {
    fn default() -> Self {
        Self::unobserved_default()
    }
}

/// Bounded context for a single launched runner attempt.
///
/// Retains all original native owners and `std::thread::JoinHandle`s so no raw
/// handle can outlive its owner and no `JoinHandle` is ever dropped on timeout/error/unwind.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum NativeShutdownStage { Launched, OutputsJoined, ExitAcked, InputJoined, ChildClosed, JobDrained }

pub(crate) struct RetainedRunnerContext {
    pub log_dir: PathBuf,
    pub process_owner: Arc<SharedChildProcessOwner>,
    pub thread_disposition: OriginalThreadDisposition,
    pub job: Arc<JobObject>,
    pub job_present: bool,
    pub conpty_resize: Arc<ConptyResizeOwner>,
    pub stdout_owner: Option<OwnedWinHandle>,
    pub stderr_owner: Option<OwnedWinHandle>,
    pub stdin_owner: Option<StdinHandleOwner>,
    pub stdout_worker: Option<RetainedWorker<io::Result<()>>>,
    pub stderr_worker: Option<RetainedWorker<io::Result<()>>>,
    pub input_worker: Option<RetainedWorker<InputLoopOutcome>>,
    pub exit_write_worker: Option<RetainedWorker<io::Result<BusinessExitWriteAck>>>,
    pub retained_writers: [Option<Arc<dyn std::any::Any + Send + Sync>>; 2],
    pub frame_write_worker: Option<RetainedWorker<io::Result<()>>>,
    pub error_write_deadline: Option<Instant>,
    pub native_close_runtime: Option<Arc<codex_windows_sandbox::NativeCloseRuntime>>,
    pub(super) native_shutdown_stage: NativeShutdownStage,
    pub(super) native_receipt_writer: FinalReceiptWriterFactState,
    pub(super) native_input_pipe: Option<Arc<File>>,
    pub(super) native_input_close: Option<OwnedWinHandle>,
    pub(super) native_input_start: Option<Arc<StdMutex<Option<StdinHandleOwner>>>>,
    pub auxiliary_close_debts: [Option<HandleCloseDebt>; 5],
    pub stdin_close_debt_anchor: Option<Arc<StdMutex<Option<HandleCloseDebt>>>>,
    pub sticky_latch: StickyUnknownLatch,
    pub ledger: CheckedLifecycleLedger,
    pub cached_native_facts: ChildNativeFactsSnapshot,
    pub _pipe_handles: Option<PipeSpawnHandles>,
    pub attempt_reservation: Option<AttemptReservationGuard>,
    pub auto_park_on_unresolved_drop: bool,
    pub resolved_for_clean_drop: bool,
}

impl std::fmt::Debug for RetainedRunnerContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetainedRunnerContext").field("log_dir", &self.log_dir)
            .field("sticky", &self.sticky_latch.first_error()).finish_non_exhaustive()
    }
}

impl RetainedRunnerContext {
    fn require_native_shutdown(&self,expected:NativeShutdownStage)->Result<(),StickyUnknownError> {
        if self.native_close_runtime.is_some() && self.native_shutdown_stage!=expected {
            return Err(self.sticky_latch.record_first(StickyUnknownKind::BusinessExitGateViolation,"v8 original shutdown order not proven"));
        }Ok(())
    }
    fn advance_native_shutdown(&mut self, expected:NativeShutdownStage, next:NativeShutdownStage)->Result<(),StickyUnknownError> {
        if self.native_close_runtime.is_some() {
            if self.native_shutdown_stage!=expected {return Err(self.sticky_latch.record_first(
                StickyUnknownKind::BusinessExitGateViolation,"v8 original shutdown operation order violated"));}
            self.native_shutdown_stage=next;
        }Ok(())
    }
    /// Installs all returned native launch handles, JobObject, ConPTY, and pipe owners into
    /// `RetainedRunnerContext` BEFORE any fallible validation, handle close, or duplication runs.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn install_raw_after_launch(
        mut reservation: Option<AttemptReservationGuard>,
        log_dir: PathBuf,
        pi: PROCESS_INFORMATION,
        job_opt: Option<Arc<JobObject>>,
        fallback_job: Arc<JobObject>,
        stdout_handle: HANDLE,
        stderr_handle: HANDLE,
        stdin_handle: Option<HANDLE>,
        conpty_owner: Option<ConptyInstance>,
        pipe_handles: Option<PipeSpawnHandles>,
    ) -> Self {
        if let Some(ref mut res) = reservation {
            res.mark_post_launch_admitted();
        }
        let creation_evidence = pipe_handles.as_ref().map(PipeSpawnHandles::creation_evidence);
        let process_guard = OwnedWinHandle::new(pi.hProcess);
        let thread_guard = OwnedWinHandle::new(pi.hThread);
        let stdout_guard = OwnedWinHandle::new(stdout_handle);
        let stderr_guard = if stderr_handle != 0 && stderr_handle != INVALID_HANDLE_VALUE {
            Some(OwnedWinHandle::new(stderr_handle))
        } else {
            None
        };
        let stdin_guard = match stdin_handle {
            Some(h) if h != 0 && h != INVALID_HANDLE_VALUE => Some(OwnedWinHandle::new(h)),
            _ => None,
        };
        let (job, job_present) = match job_opt {
            Some(j) => (j, true),
            None => (fallback_job, false),
        };
        let conpty_resize = ConptyResizeOwner::from_instance(conpty_owner);
        let process_owner = Arc::new(SharedChildProcessOwner::new_unvalidated(
            process_guard,
            pi.dwProcessId,
            pi.dwThreadId,
        ));

        let mut context = Self {
            log_dir,
            process_owner,
            thread_disposition: OriginalThreadDisposition::Open(thread_guard),
            job,
            job_present,
            conpty_resize,
            stdout_owner: Some(stdout_guard),
            stderr_owner: stderr_guard,
            stdin_owner: Some(StdinHandleOwner::new(stdin_guard)),
            stdout_worker: None,
            stderr_worker: None,
            input_worker: None,
            exit_write_worker: None,
            retained_writers: [None, None],
            frame_write_worker: None,
            error_write_deadline: None,
            native_close_runtime: None,
            native_shutdown_stage: NativeShutdownStage::Launched,
            native_receipt_writer: FinalReceiptWriterFactState::NotStartedPendingFutureReceiptSend,
            native_input_pipe: None, native_input_close: None, native_input_start: None,
            auxiliary_close_debts: [None; 5],
            stdin_close_debt_anchor: None,
            sticky_latch: StickyUnknownLatch::new(),
            ledger: CheckedLifecycleLedger::default(),
            cached_native_facts: ChildNativeFactsSnapshot::unobserved_default(),
            _pipe_handles: pipe_handles,
            attempt_reservation: reservation,
            auto_park_on_unresolved_drop: true,
            resolved_for_clean_drop: false,
        };
        if let Some(evidence) = creation_evidence { context.capture_creation_evidence(pi, evidence); }
        context.refresh_native_facts_snapshot();
        context
    }

    /// Used only immediately after original-handle capture. Absence is not a close
    /// or exit witness and cannot authorize business success or replay.
    pub(crate) fn capture_creation_evidence(&mut self, pi: PROCESS_INFORMATION, evidence: ProcessCreationEvidence) {
        let Some(proof) = evidence.not_created_for(&pi) else { return; };
        {
            let mut slot = self.process_owner.lock_slot_recovering();
            if slot.identity_validated || slot.exit_witness.is_some()
                || slot.identity.process_id != pi.dwProcessId || slot.identity.thread_id != pi.dwThreadId
                || slot.process_handle.as_ref().is_some_and(|h| h.is_open() || h.close_debt().is_some())
                || !matches!(&self.thread_disposition, OriginalThreadDisposition::Open(h) if h.raw() == 0 && h.close_debt().is_none())
            { return; }
            slot.process_handle = None;
            slot.not_created = Some(proof);
            // process_handle_closed_verified remains false: there was nothing to close.
            slot.cached_child_facts.creation = ChildCreationFactState::NeverCreatedProven;
            slot.cached_child_facts.exit = ChildExitFactState::NeverCreated;
            slot.cached_child_facts.process_handle_close = NativeHandleCloseFact::NeverCreated;
            self.thread_disposition = OriginalThreadDisposition::NotCreated(proof);
        }
        self.refresh_native_facts_snapshot();
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_after_launch_with_optional_job(
        reservation: Option<AttemptReservationGuard>,
        log_dir: PathBuf,
        pi: PROCESS_INFORMATION,
        job_opt: Option<Arc<JobObject>>,
        fallback_job: Arc<JobObject>,
        stdout_handle: HANDLE,
        stderr_handle: HANDLE,
        stdin_handle: Option<HANDLE>,
        stdin_open: bool,
        conpty_owner: Option<ConptyInstance>,
        pipe_handles: Option<PipeSpawnHandles>,
    ) -> io::Result<Self> {
        let context = Self::install_raw_after_launch(
            reservation,
            log_dir,
            pi,
            job_opt,
            fallback_job,
            stdout_handle,
            stderr_handle,
            stdin_handle,
            conpty_owner,
            pipe_handles,
        );

        context.validate_captured_originals(pi, stdin_open)
    }

    pub(crate) fn validate_captured_originals(self, pi: PROCESS_INFORMATION, stdin_open: bool) -> io::Result<Self> {
        let mut context = self;
        let validation_res: io::Result<()> = (|| {
            if let Some(error) = context._pipe_handles.as_ref().and_then(PipeSpawnHandles::launch_error) {
                return Err(io::Error::other(error.to_string()));
            }
            if let Some(code) = context._pipe_handles.as_ref().and_then(PipeSpawnHandles::primary_close_error) {
                return Err(io::Error::from_raw_os_error(code));
            }
            if !stdin_open {
                if let Some(ref mut stdin_owner) = context.stdin_owner
                    && stdin_owner.is_open()
                {
                    stdin_owner.close_once()?;
                }
            }
            if !context.job_present {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "spawned ConPTY is missing its process job",
                ));
            }
            if !context.process_owner.has_open_process_handle() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "child process handle is null or invalid",
                ));
            }
            if !context.thread_disposition.is_open() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "child primary thread handle is null or invalid",
                ));
            }
            if !context
                .stdout_owner
                .as_ref()
                .is_some_and(OwnedWinHandle::is_open)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "child stdout handle is null or invalid",
                ));
            }
            let identity = ChildProcessIdentity::validate_from_spawn(&pi, &context.job)?;
            context.process_owner.set_validated_identity(identity)?;
            Ok(())
        })();

        if let Err(err) = validation_res {
            context.sticky_latch.record_first(
                StickyUnknownKind::StartupValidationFailed,
                format!("post-launch validation failed: {err}"),
            );
            context.refresh_native_facts_snapshot();
            let _ = park_unresolved_attempt_context(context);
            return Err(err);
        }

        context.refresh_native_facts_snapshot();
        Ok(context)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_after_launch(
        log_dir: PathBuf,
        pi: PROCESS_INFORMATION,
        job: Arc<JobObject>,
        stdout_handle: HANDLE,
        stderr_handle: HANDLE,
        stdin_handle: Option<HANDLE>,
        conpty_owner: Option<ConptyInstance>,
        pipe_handles: Option<PipeSpawnHandles>,
    ) -> io::Result<Self> {
        let fallback_job = Arc::clone(&job);
        Self::new_after_launch_with_optional_job(
            None,
            log_dir,
            pi,
            Some(job),
            fallback_job,
            stdout_handle,
            stderr_handle,
            stdin_handle,
            true,
            conpty_owner,
            pipe_handles,
        )
    }

    /// Passive recorder for legacy or explicit `JobObject::preserve_descendants()` results.
    pub(crate) fn record_job_preserve_descendants_result(&mut self, res: &io::Result<()>) {
        self.cached_native_facts.job.preserve_descendants_state = match res {
            Ok(()) => JobPreserveDescendantsFactState::PreserveDescendantsInvoked,
            Err(_) => JobPreserveDescendantsFactState::QueryFailed,
        };
        self.refresh_native_facts_snapshot();
    }

    fn compute_native_facts_snapshot(&self) -> ChildNativeFactsSnapshot {
        let owner = self.process_owner.child_native_facts_snapshot();
        let primary_thread_handle_close = match &self.thread_disposition {
            OriginalThreadDisposition::Open(handle) => {
                if let Some(debt) = handle.close_debt() {
                    NativeHandleCloseFact::CloseDebtRetained {
                        os_error: debt.os_error,
                        label: debt.label,
                    }
                } else if handle.is_open() {
                    NativeHandleCloseFact::Open
                } else if handle.is_closed_verified() {
                    NativeHandleCloseFact::ClosedVerified
                } else {
                    NativeHandleCloseFact::MissingWithoutProof
                }
            }
            OriginalThreadDisposition::ClosedVerified { .. } => {
                NativeHandleCloseFact::ClosedVerified
            }
            OriginalThreadDisposition::NotCreated(_) => NativeHandleCloseFact::NeverCreated,
            OriginalThreadDisposition::CloseFailed { os_error, .. } => {
                NativeHandleCloseFact::CloseDebtRetained {
                    os_error: *os_error,
                    label: "child primary thread",
                }
            }
            OriginalThreadDisposition::Transferred => NativeHandleCloseFact::Transferred,
        };

        let stdout_configured = self.stdout_owner.is_some() || self.stdout_worker.is_some();
        let stdout_join_state = self.stdout_worker.as_ref().map(RetainedWorker::state);
        let stdout_eof_observed = self
            .stdout_worker
            .as_ref()
            .and_then(RetainedWorker::outcome)
            .is_some_and(Result::is_ok);
        let stdout_io_error = self
            .stdout_worker
            .as_ref()
            .and_then(RetainedWorker::outcome)
            .is_some_and(Result::is_err);
        let stdout_timely_verified = stdout_join_state == Some(WorkerJoinState::JoinedTimely)
            && stdout_eof_observed
            && self.ledger.stdout_eof_and_joined;
        let stdout_handle_close = observe_owned_handle_close_fact(self.stdout_owner.as_ref());
        let stdout = OutputStreamFactSnapshot {
            configured: stdout_configured,
            join_state: stdout_join_state,
            eof_or_completion_observed: stdout_eof_observed,
            timely_eof_and_joined_verified: stdout_timely_verified,
            reader_io_error_observed: stdout_io_error,
            reader_handle_close: stdout_handle_close,
        };

        let stderr_configured = self.stderr_owner.is_some() || self.stderr_worker.is_some();
        let stderr_join_state = self.stderr_worker.as_ref().map(RetainedWorker::state);
        let stderr_eof_observed = if stderr_configured {
            self.stderr_worker
                .as_ref()
                .and_then(RetainedWorker::outcome)
                .is_some_and(Result::is_ok)
        } else {
            stdout_eof_observed
        };
        let stderr_io_error = self
            .stderr_worker
            .as_ref()
            .and_then(RetainedWorker::outcome)
            .is_some_and(Result::is_err);
        let stderr_timely_verified = if stderr_configured {
            stderr_join_state == Some(WorkerJoinState::JoinedTimely)
                && stderr_eof_observed
                && self.ledger.stderr_eof_and_joined
        } else {
            stdout_timely_verified && self.ledger.stderr_eof_and_joined
        };
        let stderr_handle_close = if stderr_configured {
            observe_owned_handle_close_fact(self.stderr_owner.as_ref())
        } else {
            NativeHandleCloseFact::NotConfigured
        };
        let stderr = OutputStreamFactSnapshot {
            configured: stderr_configured,
            join_state: stderr_join_state,
            eof_or_completion_observed: stderr_eof_observed,
            timely_eof_and_joined_verified: stderr_timely_verified,
            reader_io_error_observed: stderr_io_error,
            reader_handle_close: stderr_handle_close,
        };

        let input_join_state = self.input_worker.as_ref().map(RetainedWorker::state);
        let input_outcome = self.input_worker.as_ref().and_then(RetainedWorker::outcome);
        let input_timely_verified = input_join_state == Some(WorkerJoinState::JoinedTimely)
            && self.ledger.input_peer_eof_and_joined
            && input_outcome.is_some_and(|o| {
                o.peer_eof_observed
                    && o.retired_after_child_exit
                    && !o.terminated_on_disconnect
                    && !o.explicit_terminate_received
                    && o.sticky_error.is_none()
                    && o.stdin_close_debt.is_none()
                    && o.stdin_close_count == usize::from(o.stdin_initially_open)
            });
        let input = InputWorkerFactSnapshot {
            join_state: input_join_state,
            peer_eof_observed: input_outcome.is_some_and(|o| o.peer_eof_observed),
            retired_after_child_exit: input_outcome.is_some_and(|o| o.retired_after_child_exit),
            timely_peer_eof_and_joined_verified: input_timely_verified,
            terminated_on_disconnect: input_outcome.is_some_and(|o| o.terminated_on_disconnect),
            explicit_terminate_received: input_outcome
                .is_some_and(|o| o.explicit_terminate_received),
            worker_sticky_error_kind: input_outcome
                .and_then(|o| o.sticky_error.as_ref().map(|e| e.kind)),
        };

        let stdin_close_debt = if let Some(ref stdin_owner) = self.stdin_owner {
            match stdin_owner.handle.as_ref().and_then(OwnedWinHandle::close_debt) {
                Some(debt) => StdinCloseDebtObservation::ObservedDebt {
                    os_error: debt.os_error,
                    label: debt.label,
                },
                None => StdinCloseDebtObservation::ObservedNone,
            }
        } else if let Some(outcome) = input_outcome {
            match outcome.stdin_close_debt {
                Some(debt) => StdinCloseDebtObservation::ObservedDebt {
                    os_error: debt.os_error,
                    label: debt.label,
                },
                None => StdinCloseDebtObservation::ObservedNone,
            }
        } else if let Some(ref anchor) = self.stdin_close_debt_anchor {
            match anchor.try_lock() {
                Ok(guard) => match *guard {
                    Some(debt) => StdinCloseDebtObservation::ObservedDebt {
                        os_error: debt.os_error,
                        label: debt.label,
                    },
                    None => StdinCloseDebtObservation::ObservedNone,
                },
                Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                    let debt = *poisoned.into_inner();
                    StdinCloseDebtObservation::ObservedAfterPoison {
                        has_debt: debt.is_some(),
                        os_error: debt.and_then(|d| d.os_error),
                    }
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    StdinCloseDebtObservation::BusyNotObserved
                }
            }
        } else {
            StdinCloseDebtObservation::NoAnchorInstalled
        };

        let stdin = if let Some(ref stdin_owner) = self.stdin_owner {
            let closed_once_verified = matches!(
                stdin_close_debt,
                StdinCloseDebtObservation::ObservedNone
            ) && ((!stdin_owner.initially_open()
                && stdin_owner.close_count() == 0
                && !stdin_owner.is_open())
                || (stdin_owner.initially_open()
                    && stdin_owner.close_count() == 1
                    && stdin_owner.is_closed_verified()));
            ChildStdinFactSnapshot {
                ownership_state: StdinOwnershipObservationState::HeldInContext,
                initially_open: Some(stdin_owner.initially_open()),
                close_count: Some(stdin_owner.close_count()),
                closed_once_verified,
                write_failed: Some(stdin_owner.write_failed()),
                close_debt: stdin_close_debt,
            }
        } else if let Some(outcome) = input_outcome {
            let closed_once_verified = outcome.stdin_close_debt.is_none()
                && ((!outcome.stdin_initially_open && outcome.stdin_close_count == 0)
                    || (outcome.stdin_initially_open && outcome.stdin_close_count == 1));
            ChildStdinFactSnapshot {
                ownership_state: StdinOwnershipObservationState::ObservedFromJoinedInputWorker,
                initially_open: Some(outcome.stdin_initially_open),
                close_count: Some(outcome.stdin_close_count),
                closed_once_verified,
                write_failed: None,
                close_debt: stdin_close_debt,
            }
        } else if self.input_worker.is_some() {
            ChildStdinFactSnapshot {
                ownership_state: StdinOwnershipObservationState::TransferredToRunningInputWorker,
                initially_open: None,
                close_count: None,
                closed_once_verified: false,
                write_failed: None,
                close_debt: stdin_close_debt,
            }
        } else {
            ChildStdinFactSnapshot {
                ownership_state: StdinOwnershipObservationState::MissingWithoutWorkerOutcome,
                initially_open: None,
                close_count: None,
                closed_once_verified: false,
                write_failed: None,
                close_debt: stdin_close_debt,
            }
        };

        let (conpty_lock, conpty_active) = self.conpty_resize.try_observe_active_conpty();
        let frame_write_join_state = self.frame_write_worker.as_ref().map(RetainedWorker::state);
        let frame_write_succeeded = self
            .frame_write_worker
            .as_ref()
            .and_then(RetainedWorker::outcome)
            .map(Result::is_ok);
        let business_exit_write_join_state =
            self.exit_write_worker.as_ref().map(RetainedWorker::state);
        let business_exit_ack = self.exit_write_worker.as_ref().and_then(|worker| {
            if worker.state() == WorkerJoinState::JoinedTimely {
                worker
                    .outcome()
                    .and_then(|res| res.as_ref().ok().copied())
            } else {
                None
            }
        });
        let business_exit_writer_joined_and_acked =
            business_exit_ack.is_some() && self.ledger.business_exit_sent;
        let control_and_writers = ControlAndWriterFactSnapshot {
            conpty_lock,
            conpty_active,
            frame_write_join_state,
            frame_write_succeeded,
            business_exit_write_join_state,
            business_exit_writer_joined_and_acked,
            business_exit_ack_exit_code: business_exit_ack.map(|ack| ack.exit_code),
            final_receipt_writer_state:
                self.native_receipt_writer,
        };

        let admitted_child_identity = if self.job_present {
            match owner.creation {
                ChildCreationFactState::CreatedValidated(id) => Some(id),
                _ => None,
            }
        } else {
            None
        };
        let job = JobFactSnapshot {
            job_present: self.job_present,
            admission_verified: admitted_child_identity.is_some(),
            admitted_child_identity,
            preserve_descendants_state: self.cached_native_facts.job.preserve_descendants_state,
            terminate_invoked: self.cached_native_facts.job.terminate_invoked
                || self.job.terminate_invoked()
                || owner.explicit_termination_latched,
            active_zero_state: self.cached_native_facts.job.active_zero_state,
        };

        let (latch_lock, latched_err) = self.sticky_latch.try_observe_first_error();
        let first_error_kind = latched_err
            .as_ref()
            .map(|e| e.kind)
            .or(self.cached_native_facts.first_error.first_error_kind);
        let auxiliary_close_debt_count =
            self.auxiliary_close_debts.iter().filter(|d| d.is_some()).count();
        let raw_pipe_spawn_close_debts = self
            ._pipe_handles
            .as_ref()
            .map_or([None; 4], |p| *p.close_debts());
        let pipe_spawn_close_debts = std::array::from_fn(|idx| {
            raw_pipe_spawn_close_debts[idx]
                .map(|d| d.os_error)
                .or(self.cached_native_facts.pipe_spawn_close_debts[idx])
        });
        let pipe_spawn_close_debt_count =
            pipe_spawn_close_debts.iter().filter(|d| d.is_some()).count();
        let first_close_debt_os_error = match owner.process_handle_close {
            NativeHandleCloseFact::CloseDebtRetained { os_error, .. } => os_error,
            _ => None,
        }
        .or(match primary_thread_handle_close {
            NativeHandleCloseFact::CloseDebtRetained { os_error, .. } => os_error,
            _ => None,
        })
        .or(match stdout.reader_handle_close {
            NativeHandleCloseFact::CloseDebtRetained { os_error, .. } => os_error,
            _ => None,
        })
        .or(match stderr.reader_handle_close {
            NativeHandleCloseFact::CloseDebtRetained { os_error, .. } => os_error,
            _ => None,
        })
        .or(match stdin.close_debt {
            StdinCloseDebtObservation::ObservedDebt { os_error, .. } => os_error,
            StdinCloseDebtObservation::ObservedAfterPoison { os_error, .. } => os_error,
            _ => None,
        })
        .or_else(|| {
            self.auxiliary_close_debts
                .iter()
                .flatten()
                .find_map(|d| d.os_error)
        })
        .or_else(|| {
            pipe_spawn_close_debts
                .iter()
                .copied()
                .flatten()
                .next()
        });
        let has_any_handle_close_debt = matches!(
            stdin.close_debt,
            StdinCloseDebtObservation::ObservedDebt { .. }
                | StdinCloseDebtObservation::ObservedAfterPoison { has_debt: true, .. }
                | StdinCloseDebtObservation::BusyNotObserved
        ) || auxiliary_close_debt_count > 0
            || pipe_spawn_close_debt_count > 0
            || matches!(
                primary_thread_handle_close,
                NativeHandleCloseFact::CloseDebtRetained { .. }
            )
            || matches!(
                owner.process_handle_close,
                NativeHandleCloseFact::CloseDebtRetained { .. }
            )
            || matches!(
                stdout.reader_handle_close,
                NativeHandleCloseFact::CloseDebtRetained { .. }
            )
            || matches!(
                stderr.reader_handle_close,
                NativeHandleCloseFact::CloseDebtRetained { .. }
            );

        let running_worker_count = self.running_worker_count();
        let late_reap_observed = self.cached_native_facts.late_reap_observed
            || matches!(stdout.join_state, Some(WorkerJoinState::JoinedLateAfterBudget))
            || matches!(stderr.join_state, Some(WorkerJoinState::JoinedLateAfterBudget))
            || matches!(input.join_state, Some(WorkerJoinState::JoinedLateAfterBudget))
            || matches!(
                control_and_writers.frame_write_join_state,
                Some(WorkerJoinState::JoinedLateAfterBudget)
            )
            || matches!(
                control_and_writers.business_exit_write_join_state,
                Some(WorkerJoinState::JoinedLateAfterBudget)
            );
        let no_replay_latched = first_error_kind.is_some()
            || late_reap_observed
            || has_any_handle_close_debt
            || latch_lock == LockObservationState::BusyNotObserved;
        let first_error = FirstErrorFactSnapshot {
            latch_lock,
            first_error_kind,
            no_replay_latched,
        };

        let normal_close_completed = first_error_kind.is_none()
            && latch_lock != LockObservationState::BusyNotObserved
            && !late_reap_observed
            && !has_any_handle_close_debt
            && running_worker_count == 0
            && owner.slot_lock != LockObservationState::BusyNotObserved
            && matches!(owner.creation, ChildCreationFactState::CreatedValidated(_))
            && matches!(owner.exit, ChildExitFactState::VerifiedExit(_))
            && !owner.explicit_termination_latched
            && stdout.timely_eof_and_joined_verified
            && stdout.reader_handle_close == NativeHandleCloseFact::ClosedVerified
            && stderr.timely_eof_and_joined_verified
            && ((!stderr.configured
                && stderr.reader_handle_close == NativeHandleCloseFact::NotConfigured)
                || stderr.reader_handle_close == NativeHandleCloseFact::ClosedVerified)
            && control_and_writers.business_exit_writer_joined_and_acked
            && control_and_writers.conpty_active == Some(false)
            && input.timely_peer_eof_and_joined_verified
            && stdin.closed_once_verified
            && owner.process_handle_close == NativeHandleCloseFact::ClosedVerified
            && primary_thread_handle_close == NativeHandleCloseFact::ClosedVerified
            && job.admission_verified
            && !job.terminate_invoked
            && job.preserve_descendants_state
                == JobPreserveDescendantsFactState::VerifiedNotPreserved
            && matches!(
                job.active_zero_state,
                JobActiveZeroFactState::ActiveZeroVerified { .. }
            )
            && self.ledger.runner_native_exit_ready
            && (self.native_close_runtime.is_none()
                || self.native_receipt_writer==FinalReceiptWriterFactState::JoinedTimelyAfterFlush);

        ChildNativeFactsSnapshot {
            owner,
            primary_thread_handle_close,
            stdout,
            stderr,
            input,
            stdin,
            control_and_writers,
            job,
            first_error,
            auxiliary_close_debt_count,
            pipe_spawn_close_debts,
            pipe_spawn_close_debt_count,
            first_close_debt_os_error,
            has_any_handle_close_debt,
            running_worker_count,
            late_reap_observed,
            normal_close_completed,
            runner_native_exit_ready: normal_close_completed,
            host_input_pipe_writer_keeper:
                HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration,
            host_output_pipe_reader:
                HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration,
            host_private_desktop_owner:
                HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration,
            host_quota_and_acl_keepers:
                HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration,
        }
    }

    /// Recomputes and caches `self.cached_native_facts` from the current retained originals.
    pub(crate) fn refresh_native_facts_snapshot(&mut self) -> ChildNativeFactsSnapshot {
        let snapshot = self.compute_native_facts_snapshot();
        self.cached_native_facts = snapshot;
        snapshot
    }

    /// Test-only pure scalar fixture seam: records supplied 4-entry pipe-spawn close-debt
    /// OS errors (`[Option<i32>; 4]`) without fabricating native pipe handle operations.
    #[cfg(test)]
    pub(crate) fn record_scalar_pipe_spawn_close_debts_for_test(
        &mut self,
        pipe_spawn_close_debts: [Option<i32>; 4],
    ) {
        self.cached_native_facts.pipe_spawn_close_debts = pipe_spawn_close_debts;
        self.refresh_native_facts_snapshot();
    }

    /// Read-only, non-blocking accessor returning a bounded Plain-Old-Data (`Copy`)
    /// snapshot of the child native facts from this SAME original `RetainedRunnerContext`.
    ///
    /// Performs zero OS queries, zero blocking locks, and carries zero handles/cleanup authority.
    pub(crate) fn child_native_facts_snapshot(&self) -> ChildNativeFactsSnapshot {
        self.compute_native_facts_snapshot()
    }

    /// Marks the context as having completed all required normal/legacy worker joins
    /// and handle finalization so dropping `self` releases the owner cleanly rather
    /// than parking an unresolved context.
    pub(crate) fn mark_clean_completion_for_drop(&mut self) {
        if self.native_close_runtime.is_some() {
            assert_eq!(self.native_receipt_writer,FinalReceiptWriterFactState::JoinedTimelyAfterFlush,
                "v8 cannot retire the original attempt before final receipt writer joined");
        }

        let snapshot = self.refresh_native_facts_snapshot();
        self.resolved_for_clean_drop = true;
        self.auto_park_on_unresolved_drop = false;
        lock_attempt_slot().last_snapshot = Some(snapshot);
        if let Some(ref mut reservation) = self.attempt_reservation {
            reservation.complete_clean_attempt();
        }
    }

    fn native_transport_unsettled(&self)->bool {
        self.native_close_runtime.is_some() && (
            self.native_input_pipe.is_some()
            || self.native_input_start.as_ref().is_some_and(|start|start.try_lock().map_or(true,|s|s.is_some()))
            || self.native_receipt_writer!=FinalReceiptWriterFactState::JoinedTimelyAfterFlush)
    }
    pub(crate) fn has_close_debt(&self) -> bool {
        if self.native_input_close.as_ref().is_some_and(|h| h.close_debt().is_some()) { return true; }

        self.stdin_close_debt_anchor.as_ref().is_some_and(|anchor| anchor.try_lock().map_or(true, |debt| debt.is_some()))
            || self.auxiliary_close_debts.iter().any(Option::is_some)
            || self._pipe_handles.as_ref().is_some_and(|p| p.close_debts().iter().any(Option::is_some))
            || self.cached_native_facts.pipe_spawn_close_debts.iter().any(Option::is_some)
            || matches!(self.thread_disposition, OriginalThreadDisposition::CloseFailed { .. })
            || self.process_owner.lock_slot_recovering().process_handle.as_ref()
                .is_some_and(|h| h.close_debt().is_some())
            || self.stdout_owner.as_ref().is_some_and(|h| h.close_debt().is_some())
            || self.stderr_owner.as_ref().is_some_and(|h| h.close_debt().is_some())
            || self.stdin_owner.as_ref().is_some_and(|o| o.handle.as_ref().is_some_and(|h| h.close_debt().is_some()))
            || self.input_worker.as_ref().and_then(RetainedWorker::outcome)
                .is_some_and(|outcome| outcome.stdin_close_debt.is_some())
    }

    pub(crate) fn running_worker_count(&self) -> usize {
        usize::from(self.frame_write_worker.as_ref().is_some_and(RetainedWorker::has_retained_join_handle)) + usize::from(
            self.stdout_worker
                .as_ref()
                .is_some_and(RetainedWorker::has_retained_join_handle),
        ) + usize::from(
            self.stderr_worker
                .as_ref()
                .is_some_and(RetainedWorker::has_retained_join_handle),
        ) + usize::from(
            self.input_worker
                .as_ref()
                .is_some_and(RetainedWorker::has_retained_join_handle),
        ) + usize::from(
            self.exit_write_worker
                .as_ref()
                .is_some_and(RetainedWorker::has_retained_join_handle),
        )
    }

    /// Physical reaping helper that inspects and bounded-joins the SAME retained
    /// original workers, child process handle, and `JobObject` while keeping sticky
    /// `Unknown` (`no_replay=true`) permanently latched and never setting
    /// `ledger.runner_native_exit_ready`.
    pub(crate) fn reap_unresolved_originals_bounded(&mut self, deadline: Instant) -> (usize, u32) {
        if self.sticky_latch.first_error().is_none() {
            self.sticky_latch.record_first(
                StickyUnknownKind::JobDrainFailed,
                "runner attempt parked in unresolved Unknown state",
            );
        }
        // Late physical reaping can never become normal business success.
        self.ledger.runner_native_exit_ready = false;
        self.cached_native_facts.late_reap_observed = true;

        if let Some(ref mut out_worker) = self.stdout_worker {
            let _ = out_worker.reap_late_bounded(deadline);
        }
        if let Some(ref mut err_worker) = self.stderr_worker {
            let _ = err_worker.reap_late_bounded(deadline);
        }
        if let Some(ref mut in_worker) = self.input_worker {
            let _ = in_worker.reap_late_bounded(deadline);
        }
        if let Some(ref mut frame_worker) = self.frame_write_worker {
            let _ = frame_worker.reap_late_bounded(deadline);
        }
        if let Some(ref mut exit_worker) = self.exit_write_worker {
            let _ = exit_worker.reap_late_bounded(deadline);
        }

        let running_workers = self.running_worker_count();
        let was_not_created = self.process_owner.lock_slot_recovering().not_created.is_some();
        if !was_not_created {
            let _ = self.process_owner.wait_for_native_exit_bounded(deadline);
        }
        let child_still_running = self.process_owner.is_native_process_still_running();

        if running_workers == 0 && !child_still_running {
            let _ = self.conpty_resize.close_conpty();
            if let Some(ref mut out_handle) = self.stdout_owner {
                let _ = out_handle.close_if_open_checked("parked child stdout");
            }
            if let Some(ref mut err_handle) = self.stderr_owner {
                let _ = err_handle.close_if_open_checked("parked child stderr");
            }
            if let Some(ref mut stdin_owner) = self.stdin_owner
                && stdin_owner.is_open()
            {
                let _ = stdin_owner.close_once();
            }
            if self.thread_disposition.is_open() {
                let tid = self
                    .process_owner
                    .identity()
                    .map(|id| id.thread_id)
                    .unwrap_or(0);
                let _ = self.thread_disposition.close_checked(tid);
            }
            if self.process_owner.has_open_process_handle() {
                let _ = self.process_owner.close_process_handle_checked();
            }
        }

        let job_active_processes = if self.job_present {
            loop {
                // Only capture_creation_evidence can install this typed proof,
                // after matching it against the same original PROCESS_INFORMATION.
                // Absence changes the Job observation contract, never close/exit debt.
                let observation = if was_not_created {
                    self.job.query_not_created_accounting_and_process_ids()
                } else {
                    self.job.query_accounting_and_process_ids()
                };
                match observation {
                    Ok((total, active, terminated, pids)) => {
                        let effective_active = active.max(pids.len() as u32);
                        if !matches!(
                            self.cached_native_facts.job.active_zero_state,
                            JobActiveZeroFactState::ActiveZeroVerified { .. }
                        ) {
                            if effective_active == 0 && (!was_not_created && total > 0) {
                                let extra_member_origin = if total == 1 && terminated == 0 {
                                    ExtraJobMemberOriginFact::SingleProcessOnlyVerified
                                } else {
                                    ExtraJobMemberOriginFact::ExtraMembersObservedOriginNotProvenByJobHelper {
                                        total_processes: total,
                                        total_terminated_processes: terminated,
                                    }
                                };
                                self.cached_native_facts.job.active_zero_state =
                                    JobActiveZeroFactState::LatePhysicalActiveZeroObserved {
                                        total_processes: total,
                                        total_terminated_processes: terminated,
                                        extra_member_origin,
                                    };
                            } else if effective_active > 0 {
                                self.cached_native_facts.job.active_zero_state =
                                    JobActiveZeroFactState::DrainFailed {
                                        last_total_processes: Some(total),
                                        last_active_processes: Some(effective_active),
                                        last_terminated_processes: Some(terminated),
                                    };
                            }
                        }
                        if effective_active == 0 || Instant::now() >= deadline {
                            break effective_active;
                        }
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        std::thread::sleep(remaining.min(JOB_DRAIN_POLL_INTERVAL));
                    }
                    Err(_) => {
                        if !matches!(
                            self.cached_native_facts.job.active_zero_state,
                            JobActiveZeroFactState::ActiveZeroVerified { .. }
                        ) {
                            self.cached_native_facts.job.active_zero_state =
                                JobActiveZeroFactState::DrainFailed {
                                    last_total_processes: None,
                                    last_active_processes: None,
                                    last_terminated_processes: None,
                                };
                        }
                        break 1;
                    }
                }
            }
        } else {
            0
        };

        let process_unclosed = {
            let slot = self.process_owner.lock_slot_recovering();
            !slot.process_handle_closed_verified && slot.not_created.is_none()
        };
        let unclosed = self.conpty_resize.has_active_conpty().unwrap_or(true) || self.has_close_debt() || process_unclosed || !self.thread_disposition.retired_for_physical_reap()
            || self.stdout_owner.as_ref().is_some_and(|h| h.is_open() || h.close_debt().is_some())
            || self.stderr_owner.as_ref().is_some_and(|h| h.is_open() || h.close_debt().is_some())
            || self.stdin_owner.as_ref().is_some_and(|owner| owner.handle.is_some())
            || self.auxiliary_close_debts.iter().any(Option::is_some)
            || self._pipe_handles.as_ref().is_some_and(|pipes| pipes.close_debts().iter().any(Option::is_some))
            || self.input_worker.as_ref().is_some_and(|worker| {
                matches!(worker.state(), WorkerJoinState::Panicked)
                    || worker.outcome().is_some_and(|outcome| outcome.stdin_close_debt.is_some())
            });
        let effective_active = job_active_processes.max(u32::from(child_still_running || unclosed));
        self.refresh_native_facts_snapshot();
        (running_workers, effective_active)
    }

    /// Starts stdout/stderr workers borrowing context-owned raw handles, and the
    /// input worker with its shared close-debt anchor, while retaining the original
    /// `std::thread::JoinHandle`s in `self`.
    pub(crate) fn start_io_workers(
        &mut self,
        pipe_read: File,
        pipe_write: Arc<StdMutex<File>>,
        nonce: [u8; 32],
        protocol_version: u8,
    ) -> io::Result<()> {
        self.start_io_workers_impl(Some(pipe_read),pipe_write,nonce,protocol_version)
    }
    pub(crate) fn start_native_io_workers(
        &mut self,pipe_write:Arc<StdMutex<File>>,nonce:[u8;32],
    )->io::Result<()> {
        if self.native_close_runtime.is_none() || self.native_input_pipe.is_none() {
            return Err(io::Error::other("original native IPC owners absent"));
        }
        self.start_io_workers_impl(None,pipe_write,nonce,8)
    }
    fn start_io_workers_impl(
        &mut self,mut legacy_input:Option<File>,pipe_write:Arc<StdMutex<File>>,nonce:[u8;32],protocol_version:u8,
    )->io::Result<()> {
        if self.stdout_worker.is_some() || self.stderr_worker.is_some() || self.input_worker.is_some() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists,
                "runner IO slots already installed; no additional worker may start"));
        }
        let stdout_guard = self.stdout_owner.as_ref().ok_or_else(|| {
            io::Error::other("child stdout handle missing when starting output worker")
        })?;
        self.retained_writers[0] = Some(Arc::new(Arc::clone(&pipe_write)));
        let log_dir_owned = Some(self.log_dir.clone());
        let out_join = super::spawn_output_reader_with_runtime(
            Arc::clone(&pipe_write),
            stdout_guard.raw(),
            codex_windows_sandbox::OutputStream::Stdout,
            log_dir_owned.clone(),
            protocol_version,
            self.native_close_runtime.clone(),
        );
        self.stdout_worker = Some(RetainedWorker::new("runner stdout worker", out_join));

        if let Some(stderr_guard) = self.stderr_owner.as_ref() {
            let err_join = super::spawn_output_reader_with_runtime(
                Arc::clone(&pipe_write),
                stderr_guard.raw(),
                codex_windows_sandbox::OutputStream::Stderr,
                log_dir_owned.clone(),
                protocol_version,
                self.native_close_runtime.clone(),
            );
            self.stderr_worker = Some(RetainedWorker::new("runner stderr worker", err_join));
        }

        let stdin_owner = self
            .stdin_owner
            .take()
            .unwrap_or_else(|| StdinHandleOwner::new(None));
        self.stdin_close_debt_anchor = Some(Arc::clone(&stdin_owner.close_debt_anchor));
        let input_worker = if let Some(runtime)=self.native_close_runtime.clone() {
            // Install BOTH original users in the context BEFORE the fallible
            // spawner. Closure captures only Arcs; failed spawn cannot drop them.
            let pipe=Arc::clone(self.native_input_pipe.as_ref().expect("pre-owned original native input"));
            let start=Arc::new(StdMutex::new(Some(stdin_owner)));
            self.native_input_start=Some(Arc::clone(&start));
            let conpty=Arc::clone(&self.conpty_resize);let job=Arc::clone(&self.job);
            let process=Arc::clone(&self.process_owner);let sticky=self.sticky_latch.clone();
            let join=std::thread::Builder::new().name("v8-original-input".into()).spawn(move || {
                let stdin=start.lock().unwrap_or_else(|p| p.into_inner()).take().expect("same original stdin once");
                run_managed_input_loop_with_runtime(&*pipe,stdin,conpty,job,process,log_dir_owned,
                    Some(pipe_write),nonce,protocol_version,sticky,Some(runtime))
            })?;
            RetainedWorker::new("runner v8 original input",join)
        } else {
            spawn_managed_input_worker(legacy_input.take().expect("original legacy input"),stdin_owner,Arc::clone(&self.conpty_resize),
                Arc::clone(&self.job),Arc::clone(&self.process_owner),log_dir_owned,Some(pipe_write),
                nonce,protocol_version,self.sticky_latch.clone())
        };
        self.input_worker = Some(input_worker);
        self.refresh_native_facts_snapshot();
        Ok(())
    }

    /// Executes the full checked post-wait normal-close sequence under one finite
    /// `shutdown_deadline`:
    /// 1. Closes ConPTY (if active) so merged output can reach physical EOF.
    /// 2. Bounded native-joins `stdout_worker` and `stderr_worker`, verifying real EOF.
    /// 3. Authorizes and writes business `Message::Exit` via the anchored bounded writer
    ///    (only after child exit + output joins, bounded by `shutdown_deadline`).
    /// 4. Bounded native-joins `input_worker` after host closes input pipe, verifying real EOF
    ///    and single child-stdin closure with zero forced termination.
    /// 5. Checked-closes original child thread and process handles in safe order.
    /// 6. Bounded-drains the SAME retained original `JobObject` until `active_processes == 0`.
    pub(crate) fn complete_checked_normal_close(
        &mut self,
        pipe_write: &Arc<StdMutex<File>>,
        shutdown_deadline: Instant,
    ) -> Result<NormalCloseWitness, StickyUnknownError> {
        self.complete_checked_normal_close_with_drain(
            pipe_write,
            shutdown_deadline,
            |job, deadline| job.drain_until_active_zero(deadline, JOB_DRAIN_POLL_INTERVAL),
        )
    }

    /// Joins `stdout_worker` and `stderr_worker` within `deadline`, recording real EOF
    /// and forwarding completion in `self.ledger`.
    pub(crate) fn join_output_workers_bounded(
        &mut self,
        deadline: Instant,
    ) -> Result<(), StickyUnknownError> {
        self.require_native_shutdown(NativeShutdownStage::Launched)?;
        let res: Result<(), StickyUnknownError> = (|| {
            let Some(ref mut out_worker) = self.stdout_worker else {
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::StdoutForwardingFailed,
                    "stdout worker was not started",
                ));
            };
            match out_worker
                .join_bounded(deadline)
                .map(|result| result.as_ref().map(|_| ()).map_err(|err| err.to_string()))
            {
                Ok(Ok(())) => {
                    self.ledger.stdout_eof_and_joined = true;
                }
                Ok(Err(err)) => {
                    return Err(self.sticky_latch.record_first(
                        StickyUnknownKind::StdoutForwardingFailed,
                        format!("stdout reader failed before EOF: {err}"),
                    ));
                }
                Err(err) if out_worker.state() == WorkerJoinState::Panicked => {
                    return Err(self.sticky_latch.record_first(
                        StickyUnknownKind::StdoutWorkerPanicked,
                        err.to_string(),
                    ));
                }
                Err(err) => {
                    return Err(self.sticky_latch.record_first(
                        StickyUnknownKind::StdoutJoinTimedOut,
                        format!("runner output EOF not observed: {err}"),
                    ));
                }
            }

            if let Some(ref mut err_worker) = self.stderr_worker {
                match err_worker
                    .join_bounded(deadline)
                    .map(|result| result.as_ref().map(|_| ()).map_err(|err| err.to_string()))
                {
                    Ok(Ok(())) => {
                        self.ledger.stderr_eof_and_joined = true;
                    }
                    Ok(Err(err)) => {
                        return Err(self.sticky_latch.record_first(
                            StickyUnknownKind::StderrForwardingFailed,
                            format!("stderr reader failed before EOF: {err}"),
                        ));
                    }
                    Err(err) if err_worker.state() == WorkerJoinState::Panicked => {
                        return Err(self.sticky_latch.record_first(
                            StickyUnknownKind::StderrWorkerPanicked,
                            err.to_string(),
                        ));
                    }
                    Err(err) => {
                        return Err(self.sticky_latch.record_first(
                            StickyUnknownKind::StderrJoinTimedOut,
                            format!("runner stderr EOF not observed: {err}"),
                        ));
                    }
                }
            } else {
                self.ledger.stderr_eof_and_joined = true;
            }

            for handle in [&mut self.stdout_owner, &mut self.stderr_owner]
                .into_iter()
                .flatten()
            {
                handle.close_if_open_checked("child output").map_err(|err| {
                    self.sticky_latch.record_first(
                        StickyUnknownKind::ChildHandleCloseFailed,
                        err.to_string(),
                    )
                })?;
            }
            Ok(())
        })();
        if res.is_ok() {
            self.advance_native_shutdown(NativeShutdownStage::Launched,NativeShutdownStage::OutputsJoined)?;
        }
        self.refresh_native_facts_snapshot();
        res
    }

    /// Checked startup/error frames use an anchored native writer. Observation is
    /// bounded by the supplied original deadline; a retained write forbids a new one.
    pub(crate) fn send_frame_checked<W: Write + Send + 'static>(&mut self,
        pipe: &Arc<StdMutex<W>>, frame: FramedMessage, deadline: Instant) -> io::Result<()> {
        if Instant::now() >= deadline || self.exit_write_worker.is_some()
            || self.frame_write_worker.as_ref().is_some_and(RetainedWorker::has_retained_join_handle)
        { return Err(io::Error::new(io::ErrorKind::TimedOut, "checked frame writer unavailable or expired")); }
        self.retained_writers[1] = Some(Arc::new(Arc::clone(pipe)));
        let pipe = Arc::clone(pipe);
        let runtime=self.native_close_runtime.clone();
        let worker = std::thread::Builder::new().name("runner-checked-frame-writer".into()).spawn(move || {
            let mut writer = acquire_mutex_bounded(&pipe, deadline, "checked frame writer")?;
            if Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "frame deadline expired")); }
            codex_windows_sandbox::write_runtime_message(&mut *writer, &frame, runtime.as_deref()).map_err(runtime_frame_io_error)?;
            writer.flush()?;
            if Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "late frame completion")); }
            Ok(())
        })?;
        self.frame_write_worker = Some(RetainedWorker::new("checked frame writer", worker));
        let res = self.frame_write_worker.as_mut().unwrap().join_bounded(deadline)
            .and_then(|outcome| outcome.as_ref().map(|_| ()).map_err(|err| io::Error::new(err.kind(), err.to_string())));
        self.refresh_native_facts_snapshot();
        res
    }

    /// Writes and flushes the business `Message::Exit` frame ONLY after child native exit
    /// and stdout/stderr EOF + native joins have been verified, bounding both mutex lock
    /// acquisition and frame write/flush by `deadline` via an owned, anchored native
    /// `RetainedWorker<io::Result<BusinessExitWriteAck>>`.
    pub(crate) fn send_business_exit_checked<W: Write + Send + 'static>(
        &mut self,
        pipe_write: &Arc<StdMutex<W>>,
        deadline: Instant,
    ) -> Result<BusinessExitWriteAck, StickyUnknownError> {
        self.require_native_shutdown(NativeShutdownStage::OutputsJoined)?;
        let res: Result<BusinessExitWriteAck, StickyUnknownError> = (|| {
            let payload = self.ledger.authorize_business_exit(
                &self.sticky_latch,
                self.process_owner.is_explicit_termination_latched(),
            )?;
            if self.exit_write_worker.is_some()
                || self.ledger.business_exit_sent
                || self
                    .frame_write_worker
                    .as_ref()
                    .is_some_and(RetainedWorker::has_retained_join_handle)
            {
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::BusinessExitGateViolation,
                    "business Exit write already attempted on this context",
                ));
            }
            if Instant::now() >= deadline {
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::BusinessExitWriteFailed,
                    "shutdown deadline expired before business Exit write",
                ));
            }
            self.retained_writers[1] = Some(Arc::new(Arc::clone(pipe_write)));
            let writer_arc = Arc::clone(pipe_write);
            let exit_code = payload.exit_code;
            let exit_msg = FramedMessage {
                version: CHECKED_IPC_PROTOCOL_VERSION,
                message: Message::Exit { payload },
            };

            let runtime=self.native_close_runtime.clone();
            if let Some(runtime)=runtime.as_ref(){runtime.arm_terminal_deadline(deadline).map_err(|error|
                self.sticky_latch.record_first(StickyUnknownKind::BusinessExitWriteFailed,error.to_string()))?;}
            let join_handle = std::thread::Builder::new()
                .name("runner-business-exit-writer".to_string())
                .spawn(move || -> io::Result<BusinessExitWriteAck> {
                    let mut guard =
                        acquire_mutex_bounded(&writer_arc, deadline, "runner output pipe")?;
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "shutdown deadline expired after acquiring output pipe lock before Exit write",
                        ));
                    }
                    codex_windows_sandbox::write_runtime_message(&mut *guard, &exit_msg, runtime.as_deref()).map_err(runtime_frame_io_error)?;
                    guard.flush()?;
                    let completed_at = Instant::now();
                    if completed_at >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "business Exit write/flush completed after shutdown deadline",
                        ));
                    }
                    Ok(BusinessExitWriteAck {
                        exit_code,
                        completed_at,
                    })
                })
                .map_err(|err| {
                    self.sticky_latch.record_first(
                        StickyUnknownKind::BusinessExitWriteFailed,
                        format!("failed to spawn bounded business Exit writer thread: {err}"),
                    )
                })?;

            self.exit_write_worker = Some(RetainedWorker::new(
                "runner business exit writer",
                join_handle,
            ));
            let worker = self
                .exit_write_worker
                .as_mut()
                .expect("exit_write_worker just installed");
            match worker.join_bounded(deadline) {
                Ok(Ok(ack)) => {
                    let ack = *ack;
                    if ack.completed_at >= deadline {
                        return Err(self.sticky_latch.record_first(
                            StickyUnknownKind::BusinessExitWriteFailed,
                            "business Exit writer acknowledged completion after shutdown deadline",
                        ));
                    }
                    self.ledger.business_exit_sent = true;
                    Ok(ack)
                }
                Ok(Err(write_err)) => Err(self.sticky_latch.record_first(
                    StickyUnknownKind::BusinessExitWriteFailed,
                    format!("runner exit write failed: {write_err}"),
                )),
                Err(join_err) => Err(self.sticky_latch.record_first(
                    StickyUnknownKind::BusinessExitWriteFailed,
                    format!(
                        "runner exit write did not complete within shutdown budget: {join_err}"
                    ),
                )),
            }
        })();
        if res.is_ok() {
            self.advance_native_shutdown(NativeShutdownStage::OutputsJoined,NativeShutdownStage::ExitAcked)?;
        }
        self.refresh_native_facts_snapshot();
        res
    }

    /// Joins `input_worker` within `deadline` after the host closes its input pipe endpoint,
    /// verifying real input peer EOF, single stdin close, and zero forced termination.
    pub(crate) fn join_input_worker_bounded(
        &mut self,
        deadline: Instant,
    ) -> Result<InputLoopOutcome, StickyUnknownError> {
        self.require_native_shutdown(NativeShutdownStage::ExitAcked)?;
        let res: Result<InputLoopOutcome, StickyUnknownError> = (|| {
            if !self.ledger.business_exit_sent {
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::BusinessExitGateViolation,
                    "cannot retire checked input worker before business Exit notification",
                ));
            }
            let Some(ref mut input_worker) = self.input_worker else {
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::InputIoError,
                    "input worker was not started",
                ));
            };
            let outcome = match input_worker.join_bounded(deadline).cloned() {
                Ok(outcome) => outcome,
                Err(err) if input_worker.state() == WorkerJoinState::Panicked => {
                    return Err(self.sticky_latch.record_first(
                        StickyUnknownKind::InputWorkerPanicked,
                        err.to_string(),
                    ));
                }
                Err(err) => {
                    return Err(self.sticky_latch.record_first(
                        StickyUnknownKind::InputJoinTimedOut,
                        format!("runner input worker did not retire within budget: {err}"),
                    ));
                }
            };

            if let Some(err) = outcome.sticky_error.clone() {
                return Err(self.sticky_latch.record_first(err.kind, err.detail));
            }
            if outcome.explicit_terminate_received || outcome.terminated_on_disconnect {
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::ExplicitUserTerminate,
                    "input worker invoked termination; cannot classify as normal close",
                ));
            }
            if !outcome.peer_eof_observed || !outcome.retired_after_child_exit {
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::InputPrematureDisconnect,
                    "input worker exited without post-exit peer EOF witness",
                ));
            }
            let expected_closes = usize::from(outcome.stdin_initially_open);
            if outcome.stdin_close_count != expected_closes {
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::InputStdinCloseFailed,
                    format!(
                        "child stdin close count mismatch: expected={expected_closes} observed={}",
                        outcome.stdin_close_count
                    ),
                ));
            }
            self.ledger.input_peer_eof_and_joined = true;
            self.ledger.stdin_closed_once = true;
            Ok(outcome)
        })();
        if res.is_ok() {
            if let Some(pipe)=self.native_input_pipe.take() {
                match Arc::try_unwrap(pipe) {
                    Ok(file)=>{
                        self.native_input_close=Some(OwnedWinHandle::new(file.into_raw_handle() as HANDLE));
                        self.native_input_close.as_mut().unwrap().close_checked("v8 original input pipe").map_err(|error|
                            self.sticky_latch.record_first(StickyUnknownKind::InputStdinCloseFailed,error.to_string()))?;
                    }
                    Err(pipe)=>{self.native_input_pipe=Some(pipe);return Err(self.sticky_latch.record_first(
                        StickyUnknownKind::InputJoinTimedOut,"original input pipe still has an original user"));}
                }
            }
            self.advance_native_shutdown(NativeShutdownStage::ExitAcked,NativeShutdownStage::InputJoined)?;
        }
        self.refresh_native_facts_snapshot();
        res
    }

    /// Checked-closes the original child thread and process handles in safe order
    /// ONLY after all IO workers have natively joined, retaining `ChildProcessIdentity`
    /// and `ChildExitWitness` for subsequent inspection.
    pub(crate) fn close_child_handles_checked(&mut self) -> Result<(), StickyUnknownError> {
        self.require_native_shutdown(NativeShutdownStage::InputJoined)?;
        let res: Result<(), StickyUnknownError> = (|| {
            if !self.ledger.stdout_eof_and_joined
                || !self.ledger.stderr_eof_and_joined
                || !self.ledger.input_peer_eof_and_joined
            {
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::ChildHandleCloseFailed,
                    "cannot close original child handles before all IO workers have joined",
                ));
            }
            if let Some(ref mut stdin_owner) = self.stdin_owner
                && stdin_owner.is_open()
            {
                stdin_owner.close_once().map_err(|err| {
                    self.sticky_latch.record_first(
                        StickyUnknownKind::InputStdinCloseFailed,
                        format!("remaining child stdin close failed: {err}"),
                    )
                })?;
            }
            let identity = self.process_owner.identity().map_err(|err| {
                self.sticky_latch
                    .record_first(StickyUnknownKind::ChildHandleCloseFailed, err.to_string())
            })?;
            self.thread_disposition
                .close_checked(identity.thread_id)
                .map_err(|err| {
                    self.sticky_latch.record_first(
                        StickyUnknownKind::ChildHandleCloseFailed,
                        format!("closing child primary thread handle failed: {err}"),
                    )
                })?;
            self.process_owner
                .close_process_handle_checked()
                .map_err(|err| {
                    self.sticky_latch.record_first(
                        StickyUnknownKind::ChildHandleCloseFailed,
                        format!("closing child process handle failed: {err}"),
                    )
                })?;
            self.ledger.child_handles_closed = true;
            Ok(())
        })();
        if res.is_ok() {
            self.advance_native_shutdown(NativeShutdownStage::InputJoined,NativeShutdownStage::ChildClosed)?;
        }
        self.refresh_native_facts_snapshot();
        res
    }

    /// Bounded-drains the SAME retained original `JobObject` until `active_processes == 0`,
    /// preserving actual `total_processes` and rejecting `preserve_descendants`, `terminate`,
    /// malformed/changed observations, and late budget expiry.
    pub(crate) fn drain_job_active_zero_bounded(
        &mut self,
        deadline: Instant,
    ) -> Result<JobActiveZeroWitness, StickyUnknownError> {
        self.drain_job_active_zero_bounded_with(deadline, |job, dl| {
            job.drain_until_active_zero(dl, JOB_DRAIN_POLL_INTERVAL)
        })
    }

    pub(crate) fn drain_job_active_zero_bounded_with<F>(
        &mut self,
        deadline: Instant,
        drain: F,
    ) -> Result<JobActiveZeroWitness, StickyUnknownError>
    where
        F: FnOnce(&JobObject, Instant) -> io::Result<(u32, u32, u32)>,
    {
        self.require_native_shutdown(NativeShutdownStage::ChildClosed)?;
        let res: Result<JobActiveZeroWitness, StickyUnknownError> = (|| {
            if !self.ledger.child_handles_closed {
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::JobDrainFailed,
                    "cannot drain JobObject before original child handles are closed",
                ));
            }
            match self.job.descendants_preserved() {
                Ok(true) => {
                    self.cached_native_facts.job.preserve_descendants_state =
                        JobPreserveDescendantsFactState::PreserveDescendantsInvoked;
                    return Err(self.sticky_latch.record_first(
                        StickyUnknownKind::JobPreserveDescendantsInvoked,
                        "JobObject::preserve_descendants was invoked on checked path",
                    ));
                }
                Ok(false) => {
                    self.cached_native_facts.job.preserve_descendants_state =
                        JobPreserveDescendantsFactState::VerifiedNotPreserved;
                }
                Err(err) => {
                    self.cached_native_facts.job.preserve_descendants_state =
                        JobPreserveDescendantsFactState::QueryFailed;
                    return Err(self
                        .sticky_latch
                        .record_first(StickyUnknownKind::JobDrainFailed, err.to_string()));
                }
            }
            if self.job.terminate_invoked() || self.process_owner.is_explicit_termination_latched() {
                self.cached_native_facts.job.terminate_invoked = true;
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::JobTerminateInvoked,
                    "JobObject::terminate or explicit termination was invoked on checked path",
                ));
            }
            if Instant::now() >= deadline {
                self.cached_native_facts.job.active_zero_state =
                    JobActiveZeroFactState::DrainTimedOut;
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::JobDrainTimedOut,
                    "shutdown deadline expired before JobObject active-zero drain",
                ));
            }

            let (total_processes, active_processes, total_terminated_processes) =
                drain(&self.job, deadline).map_err(|err| {
                    let kind = if err.kind() == io::ErrorKind::TimedOut {
                        self.cached_native_facts.job.active_zero_state =
                            JobActiveZeroFactState::DrainTimedOut;
                        StickyUnknownKind::JobDrainTimedOut
                    } else {
                        self.cached_native_facts.job.active_zero_state =
                            JobActiveZeroFactState::DrainFailed {
                                last_total_processes: None,
                                last_active_processes: None,
                                last_terminated_processes: None,
                            };
                        StickyUnknownKind::JobDrainFailed
                    };
                    self.sticky_latch.record_first(kind, err.to_string())
                })?;
            if Instant::now() >= deadline {
                self.cached_native_facts.job.active_zero_state =
                    JobActiveZeroFactState::DrainTimedOut;
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::JobDrainTimedOut,
                    "JobObject active-zero drain completed after shutdown deadline expired",
                ));
            }
            if total_processes == 0 || active_processes != 0 {
                self.cached_native_facts.job.active_zero_state =
                    JobActiveZeroFactState::DrainFailed {
                        last_total_processes: Some(total_processes),
                        last_active_processes: Some(active_processes),
                        last_terminated_processes: Some(total_terminated_processes),
                    };
                return Err(self.sticky_latch.record_first(
                    StickyUnknownKind::JobDrainFailed,
                    format!(
                        "invalid final job drain counts: total={total_processes} active={active_processes}"
                    ),
                ));
            }

            let witness = JobActiveZeroWitness {
                total_processes,
                active_processes,
                total_terminated_processes,
            };
            let extra_member_origin = if witness.is_single_process_only() {
                ExtraJobMemberOriginFact::SingleProcessOnlyVerified
            } else {
                ExtraJobMemberOriginFact::ExtraMembersObservedOriginNotProvenByJobHelper {
                    total_processes,
                    total_terminated_processes,
                }
            };
            self.cached_native_facts.job.active_zero_state =
                JobActiveZeroFactState::ActiveZeroVerified {
                    witness,
                    extra_member_origin,
                };
            self.ledger.job_active_zero = Some(witness);
            self.ledger.runner_native_exit_ready = true;
            Ok(witness)
        })();
        if res.is_ok() {
            self.advance_native_shutdown(NativeShutdownStage::ChildClosed,NativeShutdownStage::JobDrained)?;
        }
        self.refresh_native_facts_snapshot();
        res
    }

    /// Executes the complete post-child-exit checked normal-close sequence in order:
    /// 1. Close ConPTY resize owner and join stdout/stderr workers after real EOF.
    /// 2. Emit business `Message::Exit` via the bounded, anchored original shutdown writer.
    /// 3. Join input worker after host closes its original input pipe endpoint and verify single stdin close.
    /// 4. Checked-close original child primary thread and process handles.
    /// 5. Drain the SAME original `JobObject` to `active_processes == 0` within `deadline`.
    /// 6. Produce `NormalCloseWitness` and mark the context clean for normal owner release.
    pub(crate) fn complete_checked_normal_close_with_drain<W, F>(
        &mut self,
        pipe_write: &Arc<StdMutex<W>>,
        deadline: Instant,
        drain: F,
    ) -> Result<NormalCloseWitness, StickyUnknownError>
    where
        W: Write + Send + 'static,
        F: FnOnce(&JobObject, Instant) -> io::Result<(u32, u32, u32)>,
    {
        self.conpty_resize.close_conpty().map_err(|err| {
            let sticky = self
                .sticky_latch
                .record_first(StickyUnknownKind::ConptyLockPoisoned, err.to_string());
            self.refresh_native_facts_snapshot();
            sticky
        })?;
        self.join_output_workers_bounded(deadline)?;
        self.send_business_exit_checked(pipe_write, deadline)?;
        self.join_input_worker_bounded(deadline)?;
        self.close_child_handles_checked()?;
        self.drain_job_active_zero_bounded_with(deadline, drain)?;
        let witness = self.ledger.into_normal_close_witness().ok_or_else(|| {
            let sticky = self.sticky_latch.record_first(
                StickyUnknownKind::JobDrainFailed,
                "normal close witness incomplete after job drain",
            );
            self.refresh_native_facts_snapshot();
            sticky
        })?;
        self.mark_clean_completion_for_drop();
        Ok(witness)
    }

    fn take_for_unresolved_parking(&mut self) -> RetainedRunnerContext {
        self.refresh_native_facts_snapshot();
        self.auto_park_on_unresolved_drop = false;
        self.resolved_for_clean_drop = true;
        let thread_disposition = std::mem::replace(
            &mut self.thread_disposition,
            OriginalThreadDisposition::Transferred,
        );
        RetainedRunnerContext {
            log_dir: std::mem::take(&mut self.log_dir),
            process_owner: Arc::clone(&self.process_owner),
            thread_disposition,
            job: Arc::clone(&self.job),
            job_present: self.job_present,
            conpty_resize: Arc::clone(&self.conpty_resize),
            stdout_owner: self.stdout_owner.take(),
            stderr_owner: self.stderr_owner.take(),
            stdin_owner: self.stdin_owner.take(),
            stdout_worker: self.stdout_worker.take(),
            stderr_worker: self.stderr_worker.take(),
            input_worker: self.input_worker.take(),
            exit_write_worker: self.exit_write_worker.take(),
            retained_writers: std::mem::replace(&mut self.retained_writers, [None, None]),
            frame_write_worker: self.frame_write_worker.take(),
            error_write_deadline: self.error_write_deadline,
            native_close_runtime: self.native_close_runtime.take(),
            native_shutdown_stage: self.native_shutdown_stage,
            native_receipt_writer: self.native_receipt_writer,
            native_input_pipe: self.native_input_pipe.take(), native_input_close: self.native_input_close.take(),
            native_input_start: self.native_input_start.take(),
            auxiliary_close_debts: self.auxiliary_close_debts,
            stdin_close_debt_anchor: self.stdin_close_debt_anchor.take(),
            sticky_latch: self.sticky_latch.clone(),
            ledger: self.ledger.clone(),
            cached_native_facts: self.cached_native_facts,
            _pipe_handles: self._pipe_handles.take(),
            attempt_reservation: self.attempt_reservation.take(),
            auto_park_on_unresolved_drop: false,
            resolved_for_clean_drop: true,
        }
    }
}

impl Drop for RetainedRunnerContext {
    fn drop(&mut self) {
        if self.auto_park_on_unresolved_drop && !self.resolved_for_clean_drop {
            let parked = self.take_for_unresolved_parking();
            let _ = park_unresolved_attempt_context(parked);
        }
    }
}

/// Lifecycle state of the process-wide single-attempt reservation and retention slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttemptSlotState {
    Empty,
    ReservedPreEffect,
    ActiveAdmitted,
    ParkedUnresolved,
}

/// Disposition of the runner process's unresolved-attempt retention and physical reaping loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RunnerExitDisposition {
    /// No unresolved attempt was parked; clean normal or pre-effect exit is allowed.
    CleanNoUnresolvedAttempt,
    /// Unresolved originals (running worker, live child, or active Job members) are still
    /// live; the native runner process MUST remain alive and MUST NOT exit to the OS.
    MustHoldProcessAlive {
        sticky_error: StickyUnknownError,
        running_workers: usize,
        active_job_processes: u32,
    },
    /// Originals were parked in sticky `Unknown` (`no_replay=true`) and have now all
    /// physically terminated/joined (`JoinedLateAfterBudget`, `active_processes == 0`),
    /// while remaining sticky `Unknown` (never normal business success).
    PhysicallyReapedStickyUnknown {
        sticky_error: StickyUnknownError,
    },
}

const MAX_PARKED_OUTPUT_READERS: usize = 8;

/// Fixed pre-admitted slots for the three token originals. An admitted native
/// call or uncertain close is never retried, inferred closed, or dropped away.
#[derive(Debug)]
enum PreparationToken {
    Unused,
    Creating,
    Owned(OwnedWinHandle),
}
impl PreparationToken {
    fn settled(&self) -> bool {
        matches!(self, Self::Unused)
            || matches!(self, Self::Owned(handle) if handle.is_closed_verified())
    }
}

struct AttemptSlotInner {
    state: AttemptSlotState,
    reservation_generation: u64,
    reap_in_progress: bool,
    unanchored_native_boundary: bool,
    preparation_tokens: [PreparationToken; 3],
    standalone_reserved: [bool; MAX_PARKED_OUTPUT_READERS],
    precontext_writer_reserved: bool,
    precontext_pipe_anchor: Option<Arc<dyn std::any::Any+Send+Sync>>,
    native_transport: Option<NativeTransportEndpoints>,
    precontext_writer: Option<RetainedWorker<io::Result<()>>>,
    unresolved_sticky_error: Option<StickyUnknownError>,
    primary_context: Option<RetainedRunnerContext>,
    last_snapshot: Option<ChildNativeFactsSnapshot>,
    standalone_readers: [Option<RetainedWorker<io::Result<()>>>; MAX_PARKED_OUTPUT_READERS],
}

static ATTEMPT_SLOT: OnceLock<StdMutex<AttemptSlotInner>> = OnceLock::new();

fn lock_attempt_slot() -> std::sync::MutexGuard<'static, AttemptSlotInner> {
    let slot = ATTEMPT_SLOT.get_or_init(|| {
        StdMutex::new(AttemptSlotInner {
            state: AttemptSlotState::Empty,
            reservation_generation: 0,
            reap_in_progress: false,
            unanchored_native_boundary: false,
            preparation_tokens: std::array::from_fn(|_| PreparationToken::Unused),
            standalone_reserved: [false; MAX_PARKED_OUTPUT_READERS],
            precontext_writer_reserved: false,
            precontext_pipe_anchor: None,
            native_transport: None,
            precontext_writer: None,
            unresolved_sticky_error: None,
            primary_context: None,
            last_snapshot: None,
            standalone_readers: std::array::from_fn(|_| None),
        })
    });
    match slot.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Real pre-effect reservation and ownership guard for a single runner launch attempt.
#[derive(Debug)]
pub(crate) struct AttemptReservationGuard {
    generation: u64,
    post_launch_admitted: bool,
    completed_cleanly: bool,
    released_pre_effect: bool,
    cancelled_unresolved: bool,
}

impl AttemptReservationGuard {
    /// The fixed slot is charged before the native call. A failure/unwind keeps
    /// Creating, while a successful result is installed before further checks.
    pub(crate) fn create_preparation_token(
        &mut self, index: usize, create: impl FnOnce() -> anyhow::Result<HANDLE>,
    ) -> anyhow::Result<HANDLE> {
        let mut slot = lock_attempt_slot();
        if slot.reservation_generation != self.generation
            || slot.state != AttemptSlotState::ReservedPreEffect
            || slot.unresolved_sticky_error.is_some()
            || !matches!(slot.preparation_tokens.get(index), Some(PreparationToken::Unused)) {
            return Err(anyhow::anyhow!("original token preparation slot unavailable; no replay"));
        }
        slot.preparation_tokens[index] = PreparationToken::Creating;
        let raw = create()?;
        // The API success contract supplies one original handle, owned here.
        slot.preparation_tokens[index] = PreparationToken::Owned(OwnedWinHandle::new(raw));
        if raw == 0 || raw == INVALID_HANDLE_VALUE {
            return Err(anyhow::anyhow!("successful token preparation returned an invalid original"));
        }
        Ok(raw)
    }

    pub(crate) fn close_preparation_token(&mut self, index: usize) -> io::Result<()> {
        self.close_preparation_token_with(index, |handle| {
            if unsafe { CloseHandle(handle) } == 0 { Err(io::Error::last_os_error()) }
            else { Ok(()) }
        })
    }

    fn close_preparation_token_with(
        &mut self, index: usize, close: impl FnOnce(HANDLE) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut slot = lock_attempt_slot();
        if slot.reservation_generation != self.generation || slot.reap_in_progress
            || !matches!(slot.state, AttemptSlotState::ReservedPreEffect | AttemptSlotState::ActiveAdmitted)
            || slot.unresolved_sticky_error.is_some() {
            return Err(io::Error::other("original token preparation owner unavailable; no retry"));
        }
        let Some(PreparationToken::Owned(handle)) = slot.preparation_tokens.get_mut(index) else {
            return Err(io::Error::other("original token preparation handle unavailable"));
        };
        let result = handle.close_checked_with("runner preparation token", close);
        if let Err(error) = &result {
            slot.state = AttemptSlotState::ParkedUnresolved;
            if slot.unresolved_sticky_error.is_none() {
                slot.unresolved_sticky_error = Some(StickyUnknownError::new(
                    StickyUnknownKind::StartupValidationFailed,
                    format!("original preparation token close unresolved: {error}"),
                ));
            }
        }
        result
    }


    /// Marks that the attempt is entering or has crossed the native child/ConPTY/Job launch
    /// boundary so any subsequent drop, error, or unwind without `complete_clean_attempt`
    /// latches sticky `Unknown` (`no_replay=true`) in `ATTEMPT_SLOT`.
    pub(crate) fn mark_post_launch_admitted(&mut self) {
        self.post_launch_admitted = true;
        let mut guard = lock_attempt_slot();
        if guard.reservation_generation == self.generation
            && guard.state == AttemptSlotState::ReservedPreEffect
        {
            guard.state = AttemptSlotState::ActiveAdmitted;
        }
    }

    /// Explicitly latches an unresolved cancellation on the reserved attempt slot so
    /// automatic replay is forbidden (`no_replay=true`).
    pub(crate) fn latch_unresolved_cancellation(&mut self, detail: impl Into<String>) {
        self.cancelled_unresolved = true;
        let sticky = StickyUnknownError::new(StickyUnknownKind::StartupValidationFailed, detail);
        let mut guard = lock_attempt_slot();
        guard.state = AttemptSlotState::ParkedUnresolved;
        if guard.unresolved_sticky_error.is_none() {
            guard.unresolved_sticky_error = Some(sticky);
        }
    }

    /// Marks the attempt as cleanly completed after all required worker joins, handle
    /// closes, and Job active-zero drain have succeeded.
    pub(crate) fn complete_clean_attempt(&mut self) {
        let mut guard = lock_attempt_slot();
        if !guard.preparation_tokens.iter().all(PreparationToken::settled) { return; }
        self.completed_cleanly = true;
        if guard.reservation_generation == self.generation
            && guard.primary_context.is_none()
            && !guard.reap_in_progress
            && guard.unresolved_sticky_error.is_none()
            && guard.standalone_readers.iter().all(Option::is_none)
            && !guard.standalone_reserved.iter().any(|reserved| *reserved)
        {
            guard.state = AttemptSlotState::Empty;
        }
    }

    /// Releases a pre-effect reservation when pre-launch validation fails before any
    /// native process/ConPTY/Job effect is created.
    pub(crate) fn release_pre_effect_if_uncommitted(&mut self) {
        if self.post_launch_admitted || self.cancelled_unresolved {
            return;
        }
        let mut guard = lock_attempt_slot();
        if !guard.preparation_tokens.iter().all(PreparationToken::settled) { return; }
        self.released_pre_effect = true;
        if guard.reservation_generation == self.generation
            && guard.state == AttemptSlotState::ReservedPreEffect
            && guard.primary_context.is_none()
            && !guard.reap_in_progress
            && guard.unresolved_sticky_error.is_none()
            && guard.standalone_readers.iter().all(Option::is_none)
            && !guard.standalone_reserved.iter().any(|reserved| *reserved)
        {
            guard.state = AttemptSlotState::Empty;
        }
    }

    /// Installs all returned native post-launch originals into `RetainedRunnerContext`
    /// under this pre-admitted reservation before any fallible validation runs.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit_post_launch_originals(
        mut self,
        log_dir: PathBuf,
        pi: PROCESS_INFORMATION,
        job_opt: Option<Arc<JobObject>>,
        fallback_job: Arc<JobObject>,
        stdout_handle: HANDLE,
        stderr_handle: HANDLE,
        stdin_handle: Option<HANDLE>,
        stdin_open: bool,
        conpty_owner: Option<ConptyInstance>,
        pipe_handles: Option<PipeSpawnHandles>,
    ) -> io::Result<RetainedRunnerContext> {
        self.mark_post_launch_admitted();
        RetainedRunnerContext::new_after_launch_with_optional_job(
            Some(self),
            log_dir,
            pi,
            job_opt,
            fallback_job,
            stdout_handle,
            stderr_handle,
            stdin_handle,
            stdin_open,
            conpty_owner,
            pipe_handles,
        )
    }
}

impl Drop for AttemptReservationGuard {
    fn drop(&mut self) {
        if self.completed_cleanly || self.released_pre_effect {
            return;
        }
        let panicking = std::thread::panicking();
        let mut guard = lock_attempt_slot();
        if self.post_launch_admitted && guard.primary_context.is_none() && !guard.reap_in_progress {
            guard.unanchored_native_boundary = true;
        }
        guard.state = AttemptSlotState::ParkedUnresolved;
        if guard.unresolved_sticky_error.is_none() {
            let kind = if self.post_launch_admitted {
                StickyUnknownKind::SpawnChildFailed
            } else {
                StickyUnknownKind::StartupValidationFailed
            };
            let detail = if panicking {
                "runner attempt reservation unwound during panic"
            } else if self.cancelled_unresolved {
                "runner attempt reservation cancelled"
            } else if self.post_launch_admitted {
                "runner attempt reservation dropped after launch boundary without clean completion"
            } else {
                "runner attempt reservation dropped or cancelled before clean completion"
            };
            guard.unresolved_sticky_error = Some(StickyUnknownError::new(kind, detail));
        }
    }
}

/// Reserves the bounded pre-effect attempt slot before any native child/ConPTY/Job
/// effects are created.
///
/// While the returned `AttemptReservationGuard` is held (or after any unresolved
/// context/worker/unwind/cancellation is latched), any concurrent or subsequent
/// reservation attempt fails closed with `ErrorKind::AlreadyExists` (`no_replay=true`).
pub(crate) fn reserve_attempt_context_slot() -> io::Result<AttemptReservationGuard> {
    let mut guard = lock_attempt_slot();
    if guard.state != AttemptSlotState::Empty
        || guard.primary_context.is_some()
        || guard.precontext_writer_reserved || guard.precontext_writer.is_some()
        || guard.reap_in_progress
        || guard.unanchored_native_boundary
        || guard.unresolved_sticky_error.is_some()
        || guard.standalone_readers.iter().any(Option::is_some)
        || guard.standalone_reserved.iter().any(|reserved| *reserved)
    {
        let detail = guard
            .unresolved_sticky_error
            .as_ref()
            .cloned()
            .or_else(|| {
                guard
                    .primary_context
                    .as_ref()
                    .and_then(|ctx| ctx.sticky_latch.first_error())
            })
            .map(|err| format!("{:?}: {}", err.kind, err.detail))
            .unwrap_or_else(|| format!("attempt slot in state {:?}", guard.state));
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "runner attempt slot occupied or reserved ({detail}); automatic replay forbidden (no_replay=true)"
            ),
        ));
    }
    if !guard.preparation_tokens.iter().all(PreparationToken::settled) {
        return Err(io::Error::other("original preparation token debt; no replay"));
    }
    guard.preparation_tokens = std::array::from_fn(|_| PreparationToken::Unused);
    guard.reservation_generation = guard.reservation_generation.wrapping_add(1);
    let generation = guard.reservation_generation;
    guard.state = AttemptSlotState::ReservedPreEffect;
    Ok(AttemptReservationGuard {
        generation,
        post_launch_admitted: false,
        completed_cleanly: false,
        released_pre_effect: false,
        cancelled_unresolved: false,
    })
}

/// Error frames before a child context exists still need a bounded original owner.
/// A single pre-admitted writer slot is independent of the single child attempt.
struct NativeTransportEndpoints {
    attempt:u64,generation:u64,input:Arc<File>,output:Arc<StdMutex<File>>,
    startup_failure_reported:bool,runtime:Arc<codex_windows_sandbox::NativeCloseRuntime>,
}
pub(crate) fn retain_native_transport_before_child(
    runtime:&Arc<codex_windows_sandbox::NativeCloseRuntime>,input:Arc<File>,output:Arc<StdMutex<File>>,
)->io::Result<()> {
    let attempt=runtime.attempt();
    let mut slot=lock_attempt_slot();
    if attempt==0 || runtime.is_sealed() || slot.native_transport.is_some() || slot.primary_context.is_some()
        || slot.state!=AttemptSlotState::Empty || slot.precontext_writer_reserved
        || slot.precontext_writer.is_some() || slot.unresolved_sticky_error.is_some() {
        return Err(io::Error::other("v8 original transport admission unavailable; no replay"));
    }
    let generation=slot.reservation_generation.wrapping_add(1);
    slot.native_transport=Some(NativeTransportEndpoints {attempt,generation,input,output,startup_failure_reported:false,runtime:Arc::clone(runtime)});Ok(())
}
impl RetainedRunnerContext {
    pub(crate) fn close_preparation_token_after_spawn(&mut self) -> io::Result<()> {
        self.attempt_reservation.as_mut()
            .ok_or_else(|| io::Error::other("original token preparation reservation absent"))?
            .close_preparation_token(2)
    }

    pub(crate) fn adopt_native_transport(&mut self,attempt:u64)->io::Result<()> {
        let generation=self.attempt_reservation.as_ref().ok_or_else(||io::Error::other("same original attempt admission absent"))?.generation;
        let mut slot=lock_attempt_slot();
        let anchors=slot.native_transport.as_ref().ok_or_else(||io::Error::other("same original IPC anchors absent"))?;
        if anchors.attempt!=attempt || anchors.generation!=generation
            || self.native_close_runtime.as_ref().is_none_or(|runtime| !Arc::ptr_eq(runtime,&anchors.runtime)) {
            return Err(io::Error::other("v8 original IPC attempt/generation mismatch"));
        }
        let anchors=slot.native_transport.take().unwrap();
        self.native_input_pipe=Some(anchors.input);
        self.retained_writers[0]=Some(Arc::new(anchors.output));Ok(())
    }
}

struct PrecontextWriterGuard {
    worker: Option<RetainedWorker<io::Result<()>>>,
    native: bool,failed: bool,anchor: Option<Arc<dyn std::any::Any+Send+Sync>>,
}
impl Drop for PrecontextWriterGuard {
    fn drop(&mut self) {
        let mut slot = lock_attempt_slot();
        if self.worker.as_ref().is_some_and(RetainedWorker::has_retained_join_handle) || (self.native && self.failed) {
            slot.precontext_pipe_anchor=self.anchor.take();
            slot.precontext_writer = self.worker.take();
            slot.state = AttemptSlotState::ParkedUnresolved;
            if slot.unresolved_sticky_error.is_none() {
                slot.unresolved_sticky_error = Some(StickyUnknownError::new(
                    StickyUnknownKind::BusinessExitWriteFailed, "pre-context error writer retained"));
            }
        }
        slot.precontext_writer_reserved = false;
    }
}

pub(crate) fn send_precontext_error_frame_checked<W: Write + Send + 'static>(
    pipe: &Arc<StdMutex<W>>, frame: FramedMessage, deadline: Instant,
) -> io::Result<()> {
    send_precontext_frame_with(pipe,deadline,move |writer| write_frame(writer,&frame).map_err(runtime_frame_io_error))
}

pub(crate) fn send_precontext_frame_with<W: Write + Send + 'static, F>(
    pipe: &Arc<StdMutex<W>>, deadline: Instant, write: F,
) -> io::Result<()> where F: FnOnce(&mut W)->io::Result<()> + Send + 'static {
    send_precontext_frame_owned(pipe,deadline,false,None,write)
}
pub(crate) fn send_native_precontext_frame_with<W: Write+Send+'static,F>(
    pipe:&Arc<StdMutex<W>>,deadline:Instant,write:F,
)->io::Result<()> where F:FnOnce(&mut W)->io::Result<()>+Send+'static {
    send_precontext_frame_owned(pipe,deadline,true,None,write)
}
/// Diagnostic only: the exact pre-child transport remains owned and charged.
/// Reserving this one error frame cannot retire, replay or release its attempt.
struct NativeStartupFailureBinding {
    attempt:u64,output:std::sync::Weak<StdMutex<File>>,
    runtime:std::sync::Weak<codex_windows_sandbox::NativeCloseRuntime>,
}
pub(crate) fn send_native_startup_failure_frame_with<F>(
    pipe:&Arc<StdMutex<File>>,runtime:&Arc<codex_windows_sandbox::NativeCloseRuntime>,deadline:Instant,write:F,
)->io::Result<()> where F:FnOnce(&mut File)->io::Result<()>+Send+'static {
    let binding=NativeStartupFailureBinding {attempt:runtime.attempt(),output:Arc::downgrade(pipe),runtime:Arc::downgrade(runtime)};
    send_precontext_frame_owned(pipe,deadline,true,Some(binding),write)
}
fn send_precontext_frame_owned<W: Write+Send+'static,F>(
    pipe:&Arc<StdMutex<W>>,deadline:Instant,native:bool,binding:Option<NativeStartupFailureBinding>,write:F,
)->io::Result<()> where F:FnOnce(&mut W)->io::Result<()>+Send+'static {
    {
        let mut slot = lock_attempt_slot();
        if Instant::now() >= deadline || slot.reap_in_progress || slot.precontext_writer_reserved
            || slot.precontext_writer.is_some()
        { return Err(io::Error::new(io::ErrorKind::WouldBlock, "pre-context writer unavailable")); }
        if let Some(binding)=binding {
            // A failure returned from the same original spawn reservation. The
            // still-held endpoints supply identity; a scalar attempt is not enough.
            let original=slot.native_transport.as_ref().ok_or_else(||
                io::Error::other("original pre-child transport absent"))?;
            if binding.attempt==0 || original.attempt!=binding.attempt
                || original.generation!=slot.reservation_generation
                || !std::sync::Weak::ptr_eq(&Arc::downgrade(&original.output),&binding.output)
                || !std::sync::Weak::ptr_eq(&Arc::downgrade(&original.runtime),&binding.runtime)
                || original.runtime.is_sealed() || original.startup_failure_reported
                || slot.state!=AttemptSlotState::ParkedUnresolved
                || slot.unresolved_sticky_error.is_none()
                || slot.precontext_pipe_anchor.is_some()
                || slot.standalone_readers.iter().any(Option::is_some)
                || slot.standalone_reserved.iter().any(|value|*value) {
                return Err(io::Error::other("original startup failure writer binding rejected; no replay"));
            }
            // Reserve before spawning or any fallible write. A failed or late
            // report cannot obtain a second attempt and never clears first error.
            slot.native_transport.as_mut().unwrap().startup_failure_reported=true;
        }
        slot.precontext_writer_reserved = true;
    }
    // Construct the owner before invoking the native spawner so unwind clears only
    // unused admission; any returned original is held in this same guard.
    let mut owner = PrecontextWriterGuard { worker: None,native,failed:true,
        anchor:Some(Arc::new(Arc::clone(pipe))) };
    let pipe = Arc::clone(pipe);
    let join = std::thread::Builder::new().name("runner-startup-error-writer".into()).spawn(move || {
        let mut writer = acquire_mutex_bounded(&pipe, deadline, "startup error writer")?;
        if Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "error frame expired")); }
        write(&mut *writer)?;
        writer.flush()?;
        if Instant::now() >= deadline { return Err(io::Error::new(io::ErrorKind::TimedOut, "late error frame")); }
        Ok(())
    })?;
    owner.worker = Some(RetainedWorker::new("pre-context error writer", join));
    let result=owner.worker.as_mut().unwrap().join_bounded(deadline)?
        .as_ref().map(|_| ()).map_err(|err| err.raw_os_error()
            .map(io::Error::from_raw_os_error)
            .unwrap_or_else(||io::Error::new(err.kind(),err.to_string())));
    owner.failed=result.is_err();result
}

/// One real attempt is admitted before launch. The same owner is retained on Err,
/// unwind and poison. No overflow allocation is available to admit another effect.
pub(crate) fn park_unresolved_attempt_context(mut context: RetainedRunnerContext) -> bool {
    context.auto_park_on_unresolved_drop = false;
    context.resolved_for_clean_drop = true;
    if let Some(ref mut res) = context.attempt_reservation { res.completed_cleanly = true; }
    let sticky = context.sticky_latch.first_error().unwrap_or_else(|| {
        context.sticky_latch.record_first(StickyUnknownKind::JobDrainFailed,
            "runner attempt parked in unresolved Unknown state")
    });
    let snapshot = context.refresh_native_facts_snapshot();
    loop {
        let mut slot = lock_attempt_slot();
        if slot.primary_context.is_none() && !slot.reap_in_progress {
            slot.state = AttemptSlotState::ParkedUnresolved;
            if slot.unresolved_sticky_error.is_none() { slot.unresolved_sticky_error = Some(sticky); }
            slot.last_snapshot = Some(snapshot);
            slot.primary_context = Some(context);
            return true;
        }
        // A concurrent loan can temporarily occupy the one admitted slot. Keep the
        // actual original in this owner until it can be restored, outside the mutex.
        // A second launch cannot reach here through reserve_attempt_context_slot.
        drop(slot);
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Read-only, non-blocking accessor for the child native facts snapshot retained in
/// `ATTEMPT_SLOT` (either on the active/parked `RetainedRunnerContext` or the last
/// completed/reaped snapshot).
pub(crate) fn try_snapshot_attempt_slot_child_native_facts(
) -> (LockObservationState, Option<ChildNativeFactsSnapshot>) {
    let Some(slot_mutex) = ATTEMPT_SLOT.get() else {
        return (LockObservationState::AcquiredClean, None);
    };
    match slot_mutex.try_lock() {
        Ok(guard) => {
            let snapshot = guard
                .primary_context
                .as_ref()
                .map(RetainedRunnerContext::child_native_facts_snapshot)
                .or(guard.last_snapshot);
            (LockObservationState::AcquiredClean, snapshot)
        }
        Err(std::sync::TryLockError::Poisoned(poisoned)) => {
            let guard = poisoned.into_inner();
            let snapshot = guard
                .primary_context
                .as_ref()
                .map(RetainedRunnerContext::child_native_facts_snapshot)
                .or(guard.last_snapshot);
            (LockObservationState::PoisonedRecovered, snapshot)
        }
        Err(std::sync::TryLockError::WouldBlock) => (LockObservationState::BusyNotObserved, None),
    }
}

pub(crate) fn snapshot_attempt_slot_child_native_facts() -> Option<ChildNativeFactsSnapshot> {
    try_snapshot_attempt_slot_child_native_facts().1
}

/// Standalone reader seam used by supplied-worker tests only. Admission happens
/// BEFORE creating the thread; production output workers use their attempt context.
#[cfg(test)]
pub(crate) struct OutputReaderAdmission { index: usize, parked: bool }

#[cfg(test)]
pub(crate) fn reserve_output_reader_slot() -> io::Result<OutputReaderAdmission> {
    let mut slot = lock_attempt_slot();
    if slot.reap_in_progress { return Err(io::Error::new(io::ErrorKind::WouldBlock, "reader slots loaned")); }
    let index = (0..MAX_PARKED_OUTPUT_READERS)
        .find(|&i| !slot.standalone_reserved[i] && slot.standalone_readers[i].is_none())
        .ok_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "standalone reader capacity exhausted"))?;
    slot.standalone_reserved[index] = true;
    Ok(OutputReaderAdmission { index, parked: false })
}

#[cfg(test)]
impl Drop for OutputReaderAdmission {
    fn drop(&mut self) {
        if !self.parked { lock_attempt_slot().standalone_reserved[self.index] = false; }
    }
}

#[cfg(test)]
pub(crate) fn park_timed_out_output_reader(mut admission: OutputReaderAdmission,
    worker: RetainedWorker<io::Result<()>>) -> bool {
    let mut slot = lock_attempt_slot();
    // A non-cloneable admission capability owns precisely this empty fixed slot.
    slot.standalone_readers[admission.index] = Some(worker);
    slot.standalone_reserved[admission.index] = false;
    admission.parked = true;
    slot.state = AttemptSlotState::ParkedUnresolved;
    if slot.unresolved_sticky_error.is_none() {
        slot.unresolved_sticky_error = Some(StickyUnknownError::new(
            StickyUnknownKind::StdoutJoinTimedOut, "standalone reader parked"));
    }
    true
}

pub(crate) fn has_unresolved_attempt_slot_entries() -> bool {
    let guard = lock_attempt_slot();
    guard.state == AttemptSlotState::ParkedUnresolved
        || guard.primary_context.is_some()
        || guard.native_transport.is_some() || guard.precontext_pipe_anchor.is_some()
        || !guard.preparation_tokens.iter().all(PreparationToken::settled)
        || guard.precontext_writer_reserved || guard.precontext_writer.is_some()
        || guard.reap_in_progress
        || guard.unanchored_native_boundary
        || guard.unresolved_sticky_error.is_some()
        || guard.standalone_readers.iter().any(Option::is_some)
        || guard.standalone_reserved.iter().any(|reserved| *reserved)
}

pub(crate) fn mark_attempt_slot_unwound() {
    let mut guard = lock_attempt_slot();
    guard.state = AttemptSlotState::ParkedUnresolved;
    if guard.unresolved_sticky_error.is_none() {
        guard.unresolved_sticky_error = Some(StickyUnknownError::new(
            StickyUnknownKind::JobDrainFailed,
            "runner main unwound during panic",
        ));
    }
}

/// Inspects and bounded-reaps the SAME retained originals in `ATTEMPT_SLOT` while
/// keeping `Unknown` / `no_replay=true` permanently sticky.
///
/// Returns `RunnerExitDisposition::MustHoldProcessAlive` whenever any retained worker,
/// original child process, or original `JobObject` member is still active so the
/// native runner process never exits into the OS while originals are unresolved.
/// A loan keeps admission charged while native observations occur outside the registry
/// mutex. Drop restores the same originals on every return and unwind.
struct AttemptReapLoan {
    context: Option<RetainedRunnerContext>,
    precontext_writer: Option<RetainedWorker<io::Result<()>>>,
    readers: [Option<RetainedWorker<io::Result<()>>>; MAX_PARKED_OUTPUT_READERS],
}

impl Drop for AttemptReapLoan {
    fn drop(&mut self) {
        let mut slot = lock_attempt_slot();
        if let Some(ref ctx) = self.context {
            slot.last_snapshot = Some(ctx.cached_native_facts);
        }
        slot.primary_context = self.context.take();
        if self.precontext_writer.is_some() { slot.precontext_writer = self.precontext_writer.take(); }
        for (index, reader) in self.readers.iter_mut().enumerate() {
            if reader.is_some() { slot.standalone_readers[index] = reader.take(); }
        }
        slot.reap_in_progress = false;
    }
}

pub(crate) fn reap_unresolved_attempt_slot_bounded(deadline: Instant) -> RunnerExitDisposition {
    let (mut loan, sticky_error, reserved) = {
        let mut slot = lock_attempt_slot();
        if slot.state == AttemptSlotState::Empty && slot.primary_context.is_none()
            && slot.standalone_readers.iter().all(Option::is_none)
            && slot.unresolved_sticky_error.is_none() && !slot.reap_in_progress && !slot.unanchored_native_boundary
            && !slot.precontext_writer_reserved && slot.precontext_writer.is_none()
            && slot.native_transport.is_none() && slot.precontext_pipe_anchor.is_none()
            && slot.preparation_tokens.iter().all(PreparationToken::settled)
            && !slot.standalone_reserved.iter().any(|v| *v)
        { return RunnerExitDisposition::CleanNoUnresolvedAttempt; }
        let sticky = slot.unresolved_sticky_error.clone().unwrap_or_else(|| {
            StickyUnknownError::new(StickyUnknownKind::JobDrainFailed, "unresolved runner attempt")
        });
        if slot.reap_in_progress {
            return RunnerExitDisposition::MustHoldProcessAlive {
                sticky_error: sticky, running_workers: 1, active_job_processes: 1,
            };
        }
        let reserved = usize::from(slot.precontext_writer_reserved) + slot.standalone_reserved.iter().filter(|v| **v).count()
            + usize::from(matches!(slot.state, AttemptSlotState::ReservedPreEffect | AttemptSlotState::ActiveAdmitted));
        slot.state = AttemptSlotState::ParkedUnresolved;
        if slot.unresolved_sticky_error.is_none() { slot.unresolved_sticky_error = Some(sticky.clone()); }
        slot.reap_in_progress = true;
        let loan = AttemptReapLoan {
            context: slot.primary_context.take(),
            precontext_writer: slot.precontext_writer.take(),
            readers: std::mem::replace(&mut slot.standalone_readers, std::array::from_fn(|_| None)),
        };
        (loan, sticky, reserved)
    };
    let mut running_workers = reserved;
    if let Some(writer) = loan.precontext_writer.as_mut() {
        writer.reap_late_bounded(deadline);
        running_workers += usize::from(writer.has_retained_join_handle());
    }
    for reader in loan.readers.iter_mut().flatten() {
        reader.reap_late_bounded(deadline);
        running_workers += usize::from(reader.has_retained_join_handle());
    }
    let mut active_job_processes = u32::from(lock_attempt_slot().unanchored_native_boundary);
    let mut native_io_unsettled={let slot=lock_attempt_slot();slot.native_transport.is_some()
        || slot.precontext_pipe_anchor.is_some()
        || !slot.preparation_tokens.iter().all(PreparationToken::settled)};
    if let Some(context) = loan.context.as_mut() {
        let (running, active) = context.reap_unresolved_originals_bounded(deadline);
        running_workers += running;
        active_job_processes = active_job_processes.max(active);
        native_io_unsettled |= context.native_transport_unsettled();
    }
    drop(loan);
    if running_workers != 0 || active_job_processes != 0 || native_io_unsettled {
        RunnerExitDisposition::MustHoldProcessAlive { sticky_error, running_workers, active_job_processes }
    } else { RunnerExitDisposition::PhysicallyReapedStickyUnknown { sticky_error } }
}

/// Holds the native runner process alive as long as any parked attempt has unresolved
/// worker threads, child process handles, or active `JobObject` members.
///
/// Only returns once all retained originals have physically terminated and joined
/// (`JoinedLateAfterBudget`, `active_processes == 0`), keeping `Unknown` / `no_replay=true`
/// sticky so late closure never becomes normal business success.
pub(crate) fn hold_until_native_process_exit_allowed() -> RunnerExitDisposition {
    loop {
        let step_deadline = Instant::now() + Duration::from_millis(50);
        match reap_unresolved_attempt_slot_bounded(step_deadline) {
            disposition @ (RunnerExitDisposition::CleanNoUnresolvedAttempt
            | RunnerExitDisposition::PhysicallyReapedStickyUnknown { .. }) => {
                return disposition;
            }
            RunnerExitDisposition::MustHoldProcessAlive { .. } => {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn with_parked_attempt_context_for_test<R>(f: impl FnOnce(&RetainedRunnerContext) -> R) -> Option<R> {
    let loan = {
        let mut slot = lock_attempt_slot();
        if slot.reap_in_progress { return None; }
        slot.reap_in_progress = true;
        AttemptReapLoan { context: slot.primary_context.take(),
            precontext_writer: slot.precontext_writer.take(),
            readers: std::mem::replace(&mut slot.standalone_readers, std::array::from_fn(|_| None)) }
    };
    loan.context.as_ref().map(f)
}

#[cfg(test)]
pub(crate) fn take_parked_attempt_context_for_test() -> Option<RetainedRunnerContext> {
    let mut slot = lock_attempt_slot();
    assert!(!slot.reap_in_progress);
    let taken = slot.primary_context.take();
    slot.state = AttemptSlotState::Empty;
    slot.unresolved_sticky_error = None;
    slot.last_snapshot = None;
    taken
}

#[cfg(test)]
pub(crate) fn reset_attempt_slot_for_test() {
    let (context, readers, writer) = {
        let mut slot = lock_attempt_slot();
        assert!(!slot.reap_in_progress);
        assert!(slot.preparation_tokens.iter().all(PreparationToken::settled));
        slot.preparation_tokens = std::array::from_fn(|_| PreparationToken::Unused);
        let taken = (slot.primary_context.take(),
            std::mem::replace(&mut slot.standalone_readers, std::array::from_fn(|_| None)), slot.precontext_writer.take());
        slot.state = AttemptSlotState::Empty;
        slot.unanchored_native_boundary = false;
        slot.unresolved_sticky_error = None;
        slot.last_snapshot = None;
        slot.standalone_reserved = [false; MAX_PARKED_OUTPUT_READERS];
        slot.precontext_writer_reserved = false;
        taken
    };
    // Supplied fixtures must quiesce their threads before calling this helper.
    drop((context, readers, writer));
}

/// Release only this test's pipe-only fixture after its original writer joined.
/// No process, Job, native admission, business owner or completion is fabricated.
#[cfg(test)]
pub(crate) fn release_startup_failure_pipe_fixture_for_test(input:&Arc<File>,output:&Arc<StdMutex<File>>) {
    let held={
        let mut slot=lock_attempt_slot();
        assert!(!slot.reap_in_progress && !slot.precontext_writer_reserved);
        assert!(!slot.unanchored_native_boundary && slot.primary_context.is_none());
        assert!(slot.standalone_readers.iter().all(Option::is_none));
        assert!(!slot.standalone_reserved.iter().any(|value|*value));
        let original=slot.native_transport.as_ref().expect("exact pipe-only fixture");
        assert!(Arc::ptr_eq(&original.input,input) && Arc::ptr_eq(&original.output,output));
        if let Some(writer)=slot.precontext_writer.as_ref(){assert!(!writer.has_retained_join_handle());}
        if let Some(anchor)=slot.precontext_pipe_anchor.as_ref(){
            let same=anchor.downcast_ref::<Arc<StdMutex<File>>>().expect("original File writer");
            assert!(Arc::ptr_eq(same,output));
        }
        (slot.native_transport.take(),slot.precontext_writer.take(),slot.precontext_pipe_anchor.take())
    };
    drop(held);
    reset_attempt_slot_for_test();
}

/// Only a test-owned memory writer of the exact supplied concrete type. This
/// helper cannot release a retained native worker, File, child, Job or pipe.
#[cfg(test)]
pub(crate) fn reset_joined_memory_precontext_writer_for_test<W:Write+Send+'static>(original:&std::sync::Weak<StdMutex<W>>) {
    assert_ne!(std::any::TypeId::of::<W>(),std::any::TypeId::of::<File>());
    let (anchor,worker)={
        let mut slot=lock_attempt_slot();
        assert!(!slot.reap_in_progress && !slot.precontext_writer_reserved);
        assert!(slot.primary_context.is_none() && slot.native_transport.is_none());
        assert!(slot.standalone_readers.iter().all(Option::is_none));
        let worker=slot.precontext_writer.as_ref().expect("same original memory writer");
        assert!(!worker.has_retained_join_handle());
        assert!(matches!(worker.outcome(),Some(Err(_))));
        let anchor=slot.precontext_pipe_anchor.as_ref().expect("same original memory anchor");
        let typed=anchor.downcast_ref::<Arc<StdMutex<W>>>().expect("exact supplied memory fixture type");
        assert!(std::sync::Weak::ptr_eq(&Arc::downgrade(typed),original));
        (slot.precontext_pipe_anchor.take(),slot.precontext_writer.take())
    };
    drop((worker,anchor));
    reset_attempt_slot_for_test();
}

#[cfg(test)]
pub(crate) fn parked_attempt_context_total_count_for_test() -> usize {
    usize::from(lock_attempt_slot().primary_context.is_some())
}

#[cfg(test)]
pub(crate) fn parked_standalone_output_reader_count_for_test() -> usize {
    lock_attempt_slot().standalone_readers.iter().flatten()
        .filter(|worker| worker.has_retained_join_handle()).count()
}

#[cfg(test)]
pub(crate) fn poison_attempt_slot_for_test() {
    drop(lock_attempt_slot());
    let _ = std::panic::catch_unwind(|| {
        let _guard = lock_attempt_slot();
        panic!("intentional attempt slot poison for test");
    });
}


#[path="native_close_sender.rs"]
mod native_close_sender;

fn runtime_frame_io_error(error:anyhow::Error)->io::Error {
    match error.downcast::<io::Error>() {Ok(original)=>original,Err(original)=>io::Error::other(original)}
}

#[cfg(test)]
mod preparation_token_tests {
    use super::*;

    #[test]
    fn twenty_owned_token_preparations_close_originals_before_local_slot_reuse() {
        reset_attempt_slot_for_test();
        for _ in 0..20 {
            let mut reservation = reserve_attempt_context_slot().unwrap();
            let source = reservation.create_preparation_token(0, || unsafe {
                codex_windows_sandbox::open_current_token_for_query_duplicate()
            }).unwrap();
            let base = reservation.create_preparation_token(1, || unsafe {
                codex_windows_sandbox::duplicate_token_for_restriction(source)
            }).unwrap();
            let capability = codex_windows_sandbox::LocalSid::from_string("S-1-5-21-10-20-30-40").unwrap();
            reservation.create_preparation_token(2, || unsafe {
                codex_windows_sandbox::create_readonly_token_with_caps_and_user_from(
                    base, &[capability.as_ptr()], &[],
                )
            }).unwrap();
            assert!(has_unresolved_attempt_slot_entries());
            for index in 0..3 { reservation.close_preparation_token(index).unwrap(); }
            assert!(lock_attempt_slot().preparation_tokens.iter().all(PreparationToken::settled));
            reservation.release_pre_effect_if_uncommitted();
            assert_eq!(lock_attempt_slot().state, AttemptSlotState::Empty);
            drop(reservation);
            assert!(!has_unresolved_attempt_slot_entries());
        }
        reset_attempt_slot_for_test();
    }

    #[test]
    fn synthetic_preparation_close_debt_retains_charge_and_never_retries() {
        reset_attempt_slot_for_test();
        let mut reservation = reserve_attempt_context_slot().unwrap();
        const FIXTURE: HANDLE = 42;
        reservation.create_preparation_token(0, || Ok(FIXTURE)).unwrap();
        let mut calls = 0;
        let result = reservation.close_preparation_token_with(0, |raw| {
            assert_eq!(raw, FIXTURE); calls += 1;
            Err(io::Error::from_raw_os_error(5))
        });
        assert_eq!(result.unwrap_err().raw_os_error(), Some(5));
        assert!(reservation.close_preparation_token_with(0, |_| {
            calls += 1; Ok(())
        }).is_err());
        assert_eq!(calls, 1);
        assert!(reserve_attempt_context_slot().is_err());
        assert!(matches!(reap_unresolved_attempt_slot_bounded(Instant::now()),
            RunnerExitDisposition::MustHoldProcessAlive { .. }));
        reservation.release_pre_effect_if_uncommitted();
        assert!(!reservation.released_pre_effect);
        drop(reservation);
        // This is a numeric mock only: no native API was called for FIXTURE.
        // Do not use this removal for an actual uncertain original handle.
        {
            let mut slot = lock_attempt_slot();
            let PreparationToken::Owned(handle) = &slot.preparation_tokens[0] else { panic!() };
            assert_eq!(handle.raw(), 0);
            assert_eq!(handle.close_debt().unwrap().raw_handle, FIXTURE as usize);
            slot.preparation_tokens[0] = PreparationToken::Unused;
        }
        reset_attempt_slot_for_test();
    }

    #[test]
    fn synthetic_preparation_unwind_keeps_pre_native_charge() {
        reset_attempt_slot_for_test();
        let mut reservation = reserve_attempt_context_slot().unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = reservation.create_preparation_token(0, || panic!("synthetic before API"));
        }));
        assert!(result.is_err());
        reservation.release_pre_effect_if_uncommitted();
        assert!(!reservation.released_pre_effect);
        drop(reservation);
        assert!(reserve_attempt_context_slot().is_err());
        assert!(matches!(reap_unresolved_attempt_slot_bounded(Instant::now()),
            RunnerExitDisposition::MustHoldProcessAlive { .. }));
        {
            let mut slot = lock_attempt_slot();
            assert!(matches!(slot.preparation_tokens[0], PreparationToken::Creating));
            // The synthetic closure panicked before issuing any native call.
            slot.preparation_tokens[0] = PreparationToken::Unused;
        }
        reset_attempt_slot_for_test();
    }
}
