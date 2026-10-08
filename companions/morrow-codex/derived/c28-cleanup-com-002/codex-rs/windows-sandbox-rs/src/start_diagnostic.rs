//! Per-invocation data-only observations of the existing Windows start path.
//! A returned backend handle is not a child exit, EOF, acknowledgment or join.
use std::fmt;
use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsStartStage {
    #[default]
    NotObserved,
    RunnerSelection,
    RequestPrepare,
    ProcessMapReserve,
    MatchedRouteCheck,
    CodexHomeResolve,
    PermissionResolve,
    SandboxBaseReady,
    AccountSelect,
    SetupRefresh,
    CapabilityResolve,
    BlockingTransportTask,
    DesktopPolicy,
    RunnerResolve,
    RunnerPinVerify,
    RegisteredAlias,
    PrivateDesktop,
    PipeCreateIn,
    PipeCreateOut,
    RunnerLogon,
    RegisteredImageVerify,
    PipeConnectIn,
    PipeConnectOut,
    ControlHello,
    SpawnRequestWrite,
    SpawnReadyRead,
    CheckedDriverAssemble,
    SpawnReturned,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsStartError {
    #[default]
    NotObserved,
    InvalidParams,
    InvalidRequest,
    ServerInternal,
    OtherRpc,
    RouteRejected,
    PinRejected,
    IdentityRejected,
    ControlRejected,
    IoAccessDenied,
    IoNotFound,
    IoInvalidInput,
    IoTimedOut,
    IoInterrupted,
    IoBrokenPipe,
    IoOther,
    OsReported,
    RunnerReported,
    BlockingCancelled,
    BlockingPanic,
    Unclassified,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsRunnerErrorStage {
    #[default]
    NotObserved,
    ReadSpawnRequest,
    SpawnChild,
    WriteSpawnReady,
    TerminalWitness,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsOsError {
    #[default]
    NotObserved,
    AccessDenied,
    FileNotFound,
    PathNotFound,
    LogonFailure,
    AccountDisabled,
    NoLogonSession,
    OperationAborted,
    TimedOut,
    Other,
}

impl WindowsOsError {
    /// Consume an already observed code; never retain or print the raw number.
    pub fn from_code(code: u32) -> Self {
        match code {
            5 => Self::AccessDenied,
            2 => Self::FileNotFound,
            3 => Self::PathNotFound,
            1326 => Self::LogonFailure,
            1331 => Self::AccountDisabled,
            1312 => Self::NoLogonSession,
            995 => Self::OperationAborted,
            258 | 1460 => Self::TimedOut,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsStartCompletion {
    #[default]
    NotObserved,
    InProgressOrUnwound,
    ReturnedError,
    ReturnedBackendHandle,
    UnsupportedBackend,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowsStartAttempt {
    pub stage: WindowsStartStage,
    pub error: WindowsStartError,
    pub runner_stage: WindowsRunnerErrorStage,
    pub os_class: WindowsOsError,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowsStartSnapshot {
    pub current: WindowsStartAttempt,
    /// Only the existing credential refresh may begin a second subattempt.
    pub first_attempt: Option<WindowsStartAttempt>,
    pub attempt: u8,
    pub completion: WindowsStartCompletion,
}

/// Contains finite enum data only, with no callback, executable or resource owner.
#[derive(Clone, Default)]
pub struct WindowsStartDiagnostic(Arc<Mutex<WindowsStartSnapshot>>);

impl WindowsStartDiagnostic {
    fn record(&self, change: impl FnOnce(&mut WindowsStartSnapshot)) {
        if let Ok(mut state) = self.0.lock() {
            if matches!(
                state.completion,
                WindowsStartCompletion::NotObserved | WindowsStartCompletion::InProgressOrUnwound
            ) {
                change(&mut state);
            }
        }
    }

    pub fn snapshot(&self) -> Option<WindowsStartSnapshot> {
        self.0.lock().ok().map(|state| *state)
    }

    pub fn mark(&self, stage: WindowsStartStage) {
        self.record(|state| {
            if state.current.error == WindowsStartError::NotObserved {
                state.current.stage = stage;
            }
            state.attempt = state.attempt.max(1);
            state.completion = WindowsStartCompletion::InProgressOrUnwound;
        });
    }

    fn failure(
        &self,
        error: WindowsStartError,
        runner_stage: WindowsRunnerErrorStage,
        os_class: WindowsOsError,
    ) {
        if error == WindowsStartError::NotObserved {
            return;
        }
        self.record(|state| {
            if state.current.error == WindowsStartError::NotObserved {
                state.current.error = error;
                state.current.runner_stage = runner_stage;
                state.current.os_class = os_class;
            }
            state.attempt = state.attempt.max(1);
            state.completion = WindowsStartCompletion::InProgressOrUnwound;
        });
    }

    pub fn fail(&self, error: WindowsStartError) {
        self.failure(
            error,
            WindowsRunnerErrorStage::NotObserved,
            WindowsOsError::NotObserved,
        );
    }

    pub fn fail_io_kind(&self, kind: ErrorKind) {
        self.fail(match kind {
            ErrorKind::PermissionDenied => WindowsStartError::IoAccessDenied,
            ErrorKind::NotFound => WindowsStartError::IoNotFound,
            ErrorKind::InvalidInput => WindowsStartError::IoInvalidInput,
            ErrorKind::TimedOut => WindowsStartError::IoTimedOut,
            ErrorKind::Interrupted => WindowsStartError::IoInterrupted,
            ErrorKind::BrokenPipe => WindowsStartError::IoBrokenPipe,
            _ => WindowsStartError::IoOther,
        });
    }

    pub fn fail_os(&self, class: WindowsOsError) {
        self.failure(
            WindowsStartError::OsReported,
            WindowsRunnerErrorStage::NotObserved,
            class,
        );
    }

    pub fn fail_runner(&self, stage: WindowsRunnerErrorStage, os_class: WindowsOsError) {
        self.failure(WindowsStartError::RunnerReported, stage, os_class);
    }

    /// Observe an already selected original refresh branch; does not invoke it.
    pub fn begin_existing_retry(&self) {
        self.record(|state| {
            if state.first_attempt.is_none() {
                state.first_attempt = Some(state.current);
                state.current = WindowsStartAttempt::default();
                state.current.stage = WindowsStartStage::SetupRefresh;
                state.attempt = 2;
                state.completion = WindowsStartCompletion::InProgressOrUnwound;
            }
        });
    }

    pub fn finish_failed(&self) {
        self.record(|state| state.completion = WindowsStartCompletion::ReturnedError);
    }

    pub fn finish_returned_handle(&self) {
        self.record(|state| state.completion = WindowsStartCompletion::ReturnedBackendHandle);
    }

    pub fn unsupported(&self) {
        self.record(|state| state.completion = WindowsStartCompletion::UnsupportedBackend);
    }
}

impl fmt::Debug for WindowsStartDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.snapshot() {
            Some(snapshot) => snapshot.fmt(f),
            None => f.write_str("WindowsStartDiagnosticUnavailable"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_invocations_do_not_share_failure_or_stage() {
        let a = WindowsStartDiagnostic::default();
        let b = WindowsStartDiagnostic::default();
        a.mark(WindowsStartStage::RunnerLogon);
        a.fail_os(WindowsOsError::LogonFailure);
        assert_eq!(b.snapshot(), Some(WindowsStartSnapshot::default()));
        b.mark(WindowsStartStage::ControlHello);
        assert_eq!(
            a.snapshot().unwrap().current.stage,
            WindowsStartStage::RunnerLogon
        );
    }

    #[test]
    fn lower_first_failure_survives_outer_error_mapping() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::PipeConnectIn);
        d.fail_io_kind(ErrorKind::TimedOut);
        d.mark(WindowsStartStage::SpawnReturned);
        d.fail(WindowsStartError::ServerInternal);
        d.finish_failed();
        let s = d.snapshot().unwrap();
        assert_eq!(s.current.stage, WindowsStartStage::PipeConnectIn);
        assert_eq!(s.current.error, WindowsStartError::IoTimedOut);
        assert_eq!(s.completion, WindowsStartCompletion::ReturnedError);
    }

    #[test]
    fn existing_retry_preserves_prior_failure_without_turning_recovery_into_failure() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::RunnerLogon);
        d.fail_os(WindowsOsError::AccountDisabled);
        d.begin_existing_retry();
        d.mark(WindowsStartStage::SpawnReturned);
        d.finish_returned_handle();
        let s = d.snapshot().unwrap();
        assert_eq!(s.attempt, 2);
        assert_eq!(
            s.first_attempt.unwrap().os_class,
            WindowsOsError::AccountDisabled
        );
        assert_eq!(s.current.error, WindowsStartError::NotObserved);
        assert_eq!(s.completion, WindowsStartCompletion::ReturnedBackendHandle);
    }

    #[test]
    fn completed_observation_is_frozen_and_does_not_claim_exit_or_join() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::SpawnReturned);
        d.finish_returned_handle();
        let before = d.snapshot();
        d.mark(WindowsStartStage::RunnerLogon);
        d.fail(WindowsStartError::Unclassified);
        d.begin_existing_retry();
        assert_eq!(d.snapshot(), before);
        let text = format!("{d:?}");
        assert!(
            !text.contains("Joined") && !text.contains("Exited") && !text.contains("Acknowledged")
        );
    }

    #[test]
    fn fixed_classes_never_retain_os_codes_or_error_messages() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::SpawnReadyRead);
        d.fail_runner(
            WindowsRunnerErrorStage::SpawnChild,
            WindowsOsError::from_code(0xdeadbeef),
        );
        let s = d.snapshot().unwrap();
        assert_eq!(s.current.os_class, WindowsOsError::Other);
        let text = format!("{d:?}");
        assert!(!text.contains("deadbeef") && !text.contains("3735928559"));
        for (kind, class) in [
            (
                ErrorKind::PermissionDenied,
                WindowsStartError::IoAccessDenied,
            ),
            (ErrorKind::NotFound, WindowsStartError::IoNotFound),
            (ErrorKind::InvalidInput, WindowsStartError::IoInvalidInput),
            (ErrorKind::TimedOut, WindowsStartError::IoTimedOut),
            (ErrorKind::Interrupted, WindowsStartError::IoInterrupted),
            (ErrorKind::BrokenPipe, WindowsStartError::IoBrokenPipe),
            (ErrorKind::Other, WindowsStartError::IoOther),
        ] {
            let d = WindowsStartDiagnostic::default();
            d.fail_io_kind(kind);
            assert_eq!(d.snapshot().unwrap().current.error, class);
        }
    }

    #[test]
    fn unsupported_backend_is_distinct_from_observed_failure_or_success() {
        let d = WindowsStartDiagnostic::default();
        d.unsupported();
        d.finish_returned_handle();
        let s = d.snapshot().unwrap();
        assert_eq!(s.completion, WindowsStartCompletion::UnsupportedBackend);
        assert_eq!(s.current, WindowsStartAttempt::default());
        assert_eq!(s.attempt, 0);
    }

    #[test]
    fn poisoned_recorder_is_unavailable_without_panicking_in_execution_observers() {
        let d = WindowsStartDiagnostic::default();
        let copy = d.clone();
        assert!(
            std::thread::spawn(move || {
                let _lock = copy.0.lock().unwrap();
                panic!("synthetic poison");
            })
            .join()
            .is_err()
        );
        d.mark(WindowsStartStage::RunnerLogon);
        d.fail(WindowsStartError::Unclassified);
        d.finish_failed();
        assert_eq!(d.snapshot(), None);
        assert_eq!(format!("{d:?}"), "WindowsStartDiagnosticUnavailable");
    }

    #[test]
    fn concurrent_failure_updates_leave_one_coherent_first_error() {
        let d = WindowsStartDiagnostic::default();
        d.mark(WindowsStartStage::SpawnReadyRead);
        let a = d.clone();
        let b = d.clone();
        let x = std::thread::spawn(move || a.fail_os(WindowsOsError::LogonFailure));
        let y = std::thread::spawn(move || {
            b.fail_runner(
                WindowsRunnerErrorStage::SpawnChild,
                WindowsOsError::AccessDenied,
            )
        });
        x.join().unwrap();
        y.join().unwrap();
        let s = d.snapshot().unwrap();
        assert!(matches!(
            (s.current.error, s.current.runner_stage, s.current.os_class),
            (
                WindowsStartError::OsReported,
                WindowsRunnerErrorStage::NotObserved,
                WindowsOsError::LogonFailure
            ) | (
                WindowsStartError::RunnerReported,
                WindowsRunnerErrorStage::SpawnChild,
                WindowsOsError::AccessDenied
            )
        ));
        assert_eq!(s.current.stage, WindowsStartStage::SpawnReadyRead);
    }
}
