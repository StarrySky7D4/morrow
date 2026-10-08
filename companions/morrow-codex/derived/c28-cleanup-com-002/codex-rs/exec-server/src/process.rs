use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use codex_network_proxy::NetworkPolicyDecider;
use codex_sandboxing::SandboxType;
#[cfg(windows)]
use codex_sandboxing::WindowsStartDiagnostic;
use tokio::sync::broadcast;
use tokio::sync::watch;

use crate::ExecServerError;
use crate::ProcessId;
use crate::protocol::ExecParams;
use crate::protocol::ProcessOutputChunk;
use crate::protocol::ProcessSandboxType;
use crate::protocol::ProcessSignal;
use crate::protocol::ReadResponse;
use crate::protocol::WriteResponse;

pub struct StartedExecProcess {
    pub process: Arc<dyn ExecProcess>,
    /// `None` means the exec-server peer did not report its sandbox type.
    pub sandbox_type: Option<SandboxType>,
}

pub(crate) fn sandbox_type_from_protocol(
    sandbox_type: Option<ProcessSandboxType>,
) -> Option<SandboxType> {
    match sandbox_type {
        None => None,
        Some(ProcessSandboxType::None) => Some(SandboxType::None),
        Some(ProcessSandboxType::MacosSeatbelt) => Some(SandboxType::MacosSeatbelt),
        Some(ProcessSandboxType::LinuxSeccomp) => Some(SandboxType::LinuxSeccomp),
        Some(ProcessSandboxType::WindowsRestrictedToken) => {
            Some(SandboxType::WindowsRestrictedToken)
        }
        Some(ProcessSandboxType::WindowsMxc) => Some(SandboxType::WindowsMxc),
    }
}

/// Pushed process events for consumers that want to follow process output as it
/// arrives instead of polling retained output with [`ExecProcess::read`].
///
/// The stream is scoped to one [`ExecProcess`] handle. `Output` events carry
/// stdout, stderr, or pty bytes. `Exited` reports the process exit status, while
/// `Closed` means all output streams have ended and no more output events will
/// arrive. `Failed` is used when the process session cannot continue, for
/// example because the remote environment connection disconnected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecProcessEvent {
    Output(ProcessOutputChunk),
    Exited {
        seq: u64,
        exit_code: i32,
        sandbox_denied: Option<bool>,
    },
    Closed {
        seq: u64,
    },
    Failed(String),
}

/// Replay buffer plus live fan-out for pushed process events.
///
/// New subscribers first drain a bounded replay history, then continue on the
/// live broadcast channel. The history is bounded by event count and retained
/// output bytes: count protects against many tiny events, while bytes protects
/// against a few very large output chunks.
#[derive(Clone)]
pub(crate) struct ExecProcessEventLog {
    inner: Arc<ExecProcessEventLogInner>,
}

struct ExecProcessEventLogInner {
    history: StdMutex<ExecProcessEventHistory>,
    live_tx: broadcast::Sender<ExecProcessEvent>,
    event_capacity: usize,
    byte_capacity: usize,
}

#[derive(Default)]
struct ExecProcessEventHistory {
    events: VecDeque<ExecProcessEvent>,
    retained_bytes: usize,
}

impl ExecProcessEvent {
    /// Sequence number used to order process-owned events.
    ///
    /// `Failed` is intentionally unsequenced because it is synthesized by the
    /// client when the session or transport fails, not emitted by the process.
    pub(crate) fn seq(&self) -> Option<u64> {
        match self {
            ExecProcessEvent::Output(chunk) => Some(chunk.seq),
            ExecProcessEvent::Exited { seq, .. } | ExecProcessEvent::Closed { seq } => Some(*seq),
            ExecProcessEvent::Failed(_) => None,
        }
    }

    fn retained_len(&self) -> usize {
        match self {
            ExecProcessEvent::Output(chunk) => chunk.chunk.0.len(),
            ExecProcessEvent::Failed(message) => message.len(),
            ExecProcessEvent::Exited { .. } | ExecProcessEvent::Closed { .. } => 0,
        }
    }
}

impl ExecProcessEventLog {
    pub(crate) fn new(event_capacity: usize, byte_capacity: usize) -> Self {
        let (live_tx, _live_rx) = broadcast::channel(event_capacity);
        Self {
            inner: Arc::new(ExecProcessEventLogInner {
                history: StdMutex::new(ExecProcessEventHistory::default()),
                live_tx,
                event_capacity,
                byte_capacity,
            }),
        }
    }

    pub(crate) fn publish(&self, event: ExecProcessEvent) {
        let mut history = self
            .inner
            .history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        history.retained_bytes += event.retained_len();
        history.events.push_back(event.clone());
        while history.events.len() > self.inner.event_capacity
            || history.retained_bytes > self.inner.byte_capacity
        {
            let Some(evicted) = history.events.pop_front() else {
                break;
            };
            history.retained_bytes = history
                .retained_bytes
                .saturating_sub(evicted.retained_len());
        }

        let _ = self.inner.live_tx.send(event);
    }

    pub(crate) fn subscribe(&self) -> ExecProcessEventReceiver {
        let history = self
            .inner
            .history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let live_rx = self.inner.live_tx.subscribe();
        let replay = history.events.iter().cloned().collect();

        ExecProcessEventReceiver {
            replay,
            live_rx,
            _keepalive: None,
        }
    }
}

pub struct ExecProcessEventReceiver {
    replay: VecDeque<ExecProcessEvent>,
    live_rx: broadcast::Receiver<ExecProcessEvent>,
    _keepalive: Option<broadcast::Sender<ExecProcessEvent>>,
}

impl ExecProcessEventReceiver {
    /// Returns a receiver that remains open without yielding events.
    pub fn empty() -> Self {
        let (live_tx, live_rx) = broadcast::channel(1);
        Self {
            replay: VecDeque::new(),
            live_rx,
            _keepalive: Some(live_tx),
        }
    }

    /// Returns the next replayed or live event.
    ///
    /// `Lagged` means this receiver fell behind the bounded live channel. The
    /// caller should recover through [`ExecProcess::read`] using the last
    /// delivered sequence number, then continue receiving pushed events.
    pub async fn recv(&mut self) -> Result<ExecProcessEvent, broadcast::error::RecvError> {
        if let Some(event) = self.replay.pop_front() {
            return Ok(event);
        }

        self.live_rx.recv().await
    }
}

/// Controls actually implemented by this exact process adapter.
///
/// These capabilities are not execution permission. Callers must still apply
/// their owner, admission and approval fences before requesting an effect.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessControlCapabilities {
    pub close_input: bool,
    pub resize_pty: bool,
}

/// A definite result of a checked process-control request.
///
/// `Rejected` means that this invocation did not issue a new low-level effect.
/// An OS, transport or acknowledgement failure must instead return an error;
/// callers must treat that error as unknown and must not automatically replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessControlOutcome {
    Applied,
    Unsupported,
    Rejected,
}

/// Handle for an executor-managed process.
///
/// Implementations must support both retained-output reads and pushed events:
/// `read` is the request/response API for callers that want to page through
/// buffered output, while `subscribe_events` is the streaming API for callers
/// that want output and lifecycle changes delivered as they happen.
pub trait ExecProcess: Send + Sync {
    fn process_id(&self) -> &ProcessId;

    fn subscribe_wake(&self) -> watch::Receiver<u64>;

    fn subscribe_events(&self) -> ExecProcessEventReceiver;

    fn read(
        &self,
        after_seq: Option<u64>,
        max_bytes: Option<usize>,
        wait_ms: Option<u64>,
    ) -> ExecProcessFuture<'_, ReadResponse>;

    fn write(&self, chunk: Vec<u8>) -> ExecProcessFuture<'_, WriteResponse>;

    fn signal(&self, signal: ProcessSignal) -> ExecProcessFuture<'_, ()>;

    fn terminate(&self) -> ExecProcessFuture<'_, ()>;

    /// Discover checked controls on this adapter. Remote and decorated processes
    /// remain unsupported unless they explicitly implement or delegate them.
    /// This is a read-only query with no effects. Dropping its future must not
    /// change process state or issue a native control.
    fn checked_control_capabilities(&self) -> ExecProcessFuture<'_, ProcessControlCapabilities> {
        Box::pin(async { Ok(ProcessControlCapabilities::default()) })
    }

    /// Close input only after the same process confirms the physical close.
    /// Dropping the wait must not cause a subsequent request to repeat the effect.
    fn close_input_checked(&self) -> ExecProcessFuture<'_, ProcessControlOutcome> {
        Box::pin(async { Ok(ProcessControlOutcome::Unsupported) })
    }

    /// Resize a real PTY and wait for its applied result. No remote RPC is added.
    fn resize_checked(
        &self,
        _rows: u16,
        _cols: u16,
    ) -> ExecProcessFuture<'_, ProcessControlOutcome> {
        Box::pin(async { Ok(ProcessControlOutcome::Unsupported) })
    }
}

#[cfg(test)]
#[path = "process_controls_tests.rs"]
mod checked_control_tests;

pub type ExecProcessFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ExecServerError>> + Send + 'a>>;

pub trait ExecBackend: Send + Sync {
    fn start(&self, params: ExecParams) -> ExecBackendFuture<'_>;

    /// Observe one matched Windows start when the backend supports it.
    /// The default still starts exactly once, without claiming observation.
    #[cfg(windows)]
    fn start_with_windows_diagnostics(
        &self,
        params: ExecParams,
        diagnostic: WindowsStartDiagnostic,
    ) -> ExecBackendFuture<'_> {
        diagnostic.unsupported();
        self.start(params)
    }

    /// Captures a local shell snapshot without starting the requested command.
    /// Failures must remain retryable by real commands. Remote backends do not
    /// support this operation; callers should leave them on the lazy path.
    fn prewarm_shell_snapshot(&self, _params: ExecParams) -> ExecProcessFuture<'_, ()> {
        Box::pin(async {
            Err(ExecServerError::Protocol(
                "exec backend does not support shell snapshot prewarming".to_string(),
            ))
        })
    }

    /// Starts a process with an authoritative controller-side policy decider.
    fn start_with_network_policy_decider(
        &self,
        _params: ExecParams,
        _decider: Arc<dyn NetworkPolicyDecider>,
    ) -> ExecBackendFuture<'_> {
        Box::pin(async {
            Err(ExecServerError::Protocol(
                "exec backend does not support remote network policy decisions".to_string(),
            ))
        })
    }
}

pub type ExecBackendFuture<'a> =
    Pin<Box<dyn Future<Output = Result<StartedExecProcess, ExecServerError>> + Send + 'a>>;

#[cfg(all(test, windows))]
mod windows_start_diagnostic_tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use codex_sandboxing::WindowsStartAttempt;
    use codex_sandboxing::WindowsStartCompletion;
    use codex_sandboxing::WindowsStartDiagnostic;
    use codex_utils_path_uri::PathUri;

    use super::ExecBackend;
    use super::ExecBackendFuture;
    use crate::ExecServerError;
    use crate::protocol::ExecParams;

    struct DefaultDiagnosticBackend {
        starts: AtomicUsize,
        error: Mutex<Option<ExecServerError>>,
    }

    impl ExecBackend for DefaultDiagnosticBackend {
        fn start(&self, _params: ExecParams) -> ExecBackendFuture<'_> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            let error = self.error.lock().expect("fake error lock").take();
            Box::pin(async move { Err(error.expect("only one start")) })
        }
    }

    #[tokio::test]
    async fn default_diagnostic_entry_starts_once_and_preserves_error_without_observation() {
        let backend = DefaultDiagnosticBackend {
            starts: AtomicUsize::new(0),
            error: Mutex::new(Some(ExecServerError::Server {
                code: -32602,
                message: "fake original error".to_string(),
            })),
        };
        let params = ExecParams {
            process_id: "fake-diagnostic-start".into(),
            metadata: None,
            argv: Vec::new(),
            cwd: PathUri::parse("file:///C:/fake-diagnostic-cwd").expect("fixed fake URI"),
            env_policy: None,
            shell_snapshot: None,
            env: HashMap::new(),
            tty: false,
            pipe_stdin: false,
            arg0: None,
            sandbox: None,
            enforce_managed_network: false,
            managed_network: None,
            network_proxy: None,
        };
        let diagnostic = WindowsStartDiagnostic::default();
        let error = backend
            .start_with_windows_diagnostics(params, diagnostic.clone())
            .await
            .err()
            .expect("fake backend error must be preserved");
        assert_eq!(backend.starts.load(Ordering::SeqCst), 1);
        assert!(matches!(
            error,
            ExecServerError::Server { code: -32602, message }
                if message == "fake original error"
        ));
        let snapshot = diagnostic.snapshot().expect("unpoisoned fixed snapshot");
        assert_eq!(snapshot.completion, WindowsStartCompletion::UnsupportedBackend);
        assert_eq!(snapshot.current, WindowsStartAttempt::default());
        assert_eq!(snapshot.first_attempt, None);
        assert_eq!(snapshot.attempt, 0);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tokio::time::Duration;
    use tokio::time::timeout;

    use super::ExecProcessEvent;
    use super::ExecProcessEventLog;
    use super::ExecProcessEventReceiver;
    use crate::protocol::ExecOutputStream;
    use crate::protocol::ProcessOutputChunk;

    #[tokio::test]
    async fn empty_event_receiver_stays_open() {
        let mut events = ExecProcessEventReceiver::empty();

        assert!(
            timeout(Duration::from_millis(10), events.recv())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn event_history_replay_is_bounded_by_retained_bytes() {
        let log = ExecProcessEventLog::new(/*event_capacity*/ 8, /*byte_capacity*/ 3);

        log.publish(ExecProcessEvent::Output(ProcessOutputChunk {
            seq: 1,
            stream: ExecOutputStream::Stdout,
            chunk: b"large".to_vec().into(),
        }));
        log.publish(ExecProcessEvent::Exited {
            seq: 2,
            exit_code: 0,
            sandbox_denied: Some(false),
        });
        log.publish(ExecProcessEvent::Closed { seq: 3 });

        let mut events = log.subscribe();
        let replay = vec![
            timeout(Duration::from_secs(1), events.recv())
                .await
                .expect("exit event replay should not time out")
                .expect("exit event replay should be available"),
            timeout(Duration::from_secs(1), events.recv())
                .await
                .expect("closed event replay should not time out")
                .expect("closed event replay should be available"),
        ];

        assert_eq!(
            replay,
            vec![
                ExecProcessEvent::Exited {
                    seq: 2,
                    exit_code: 0,
                    sandbox_denied: Some(false),
                },
                ExecProcessEvent::Closed { seq: 3 },
            ]
        );
    }
}
