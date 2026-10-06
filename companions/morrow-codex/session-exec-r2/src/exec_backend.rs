//! Real upstream `ExecBackend` seam. No local spawn or unapproved fallback.
use std::sync::Arc;

use codex_exec_server::{
    ExecBackend, ExecBackendFuture, ExecParams, ExecProcess, ExecProcessEventReceiver,
    ExecProcessFuture, ExecServerError, ProcessId, ProcessSignal, ReadResponse, StartedExecProcess,
    WriteResponse,
};
use morrow_agent_session_exec_v1_r2::{Environment, Intent, MAX_RUNTIME_MS, hash};
use tokio::sync::watch;

/// Selected only by the trusted reviewed artifact/domain registry. A pathname
/// or guest-supplied digest is not an artifact admission.
pub struct ReviewedArtifact {
    pub artifact_sha256: [u8; 32],
    pub execution_domain: String,
    pub max_runtime_ms: u64,
}

/// The native connection must perform canonical Propose -> trusted exact-intent
/// review/approval -> Claim -> durable consume -> real backend start -> Report.
/// It must return the genuine process supplied by that admitted backend, with
/// retained reads and process-owned event subscriptions. SDK facts alone cannot
/// construct a `StartedExecProcess` or grant execution authority.
pub trait ReviewedExecConnection: Send + Sync {
    /// Receives the complete original sandbox/network/env policy as well as
    /// argv/cwd, so an artifact cannot be selected while silently dropping policy.
    fn review_artifact(&self, params: &ExecParams) -> Result<ReviewedArtifact, ExecServerError>;
    /// Must recheck original connection/admission/domain and atomically consume
    /// the claim before the actual backend callback. On uncertainty, never replay.
    fn start_claimed(&self, intent: Intent, params: ExecParams) -> ExecBackendFuture<'_>;
}

pub struct MorrowExecBackend {
    connection: Arc<dyn ReviewedExecConnection>,
}
impl MorrowExecBackend {
    pub fn new(connection: Arc<dyn ReviewedExecConnection>) -> Self {
        Self { connection }
    }
}
impl ExecBackend for MorrowExecBackend {
    fn start(&self, params: ExecParams) -> ExecBackendFuture<'_> {
        Box::pin(async move {
            // Fixed input is empty at this trait seam. Interactive stdin/PTY,
            // argv0 overrides and executor-owned shell-state restoration need
            // the explicitly deferred extended execution contract.
            if params.tty
                || params.pipe_stdin
                || params.arg0.is_some()
                || params.shell_snapshot.is_some()
            {
                return Err(protocol(
                    "R2 fixed execution does not admit PTY, interactive stdin, argv0 override or shell snapshot",
                ));
            }
            let (program, argv) = params
                .argv
                .split_first()
                .ok_or_else(|| protocol("execution argv is empty"))?;
            let artifact = self.connection.review_artifact(&params)?;
            if artifact.max_runtime_ms == 0 || artifact.max_runtime_ms > MAX_RUNTIME_MS {
                return Err(protocol("reviewed execution runtime exceeds profile bound"));
            }
            let mut env = params
                .env
                .iter()
                .map(|(name, value)| Environment {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect::<Vec<_>>();
            env.sort_by(|a, b| a.name.cmp(&b.name));
            // A logical process key cannot acquire a second execution simply
            // by changing argv or HashMap iteration order. The connection must
            // bind its reviewed domain to the complete original policy below.
            let operation_id = hash(params.process_id.as_ref().as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let cwd = params
                .cwd
                .to_path_buf()
                .into_os_string()
                .into_string()
                .map_err(|_| protocol("execution cwd is not UTF-8"))?;
            let intent = Intent {
                operation_id: format!("codex-{operation_id}"),
                artifact_sha256: artifact.artifact_sha256,
                program: program.clone(),
                argv: argv.to_vec(),
                cwd,
                env,
                input: Vec::new(),
                execution_domain: artifact.execution_domain,
                max_runtime_ms: artifact.max_runtime_ms,
            };
            intent
                .validate()
                .map_err(|error| protocol(error.to_string()))?;
            let started = self.connection.start_claimed(intent, params).await?;
            // Preserve real backend reads, event sources and sandbox provenance.
            Ok(StartedExecProcess {
                process: Arc::new(FixedExecProcess {
                    original: started.process,
                }),
                sandbox_type: started.sandbox_type,
            })
        })
    }
}
struct FixedExecProcess {
    original: Arc<dyn ExecProcess>,
}
impl ExecProcess for FixedExecProcess {
    fn process_id(&self) -> &ProcessId {
        self.original.process_id()
    }
    fn subscribe_wake(&self) -> watch::Receiver<u64> {
        self.original.subscribe_wake()
    }
    fn subscribe_events(&self) -> ExecProcessEventReceiver {
        self.original.subscribe_events()
    }
    fn read(
        &self,
        after_seq: Option<u64>,
        max_bytes: Option<usize>,
        wait_ms: Option<u64>,
    ) -> ExecProcessFuture<'_, ReadResponse> {
        self.original.read(after_seq, max_bytes, wait_ms)
    }
    fn write(&self, _chunk: Vec<u8>) -> ExecProcessFuture<'_, WriteResponse> {
        Box::pin(async { Err(protocol("interactive stdin is outside R2 fixed execution")) })
    }
    fn signal(&self, _signal: ProcessSignal) -> ExecProcessFuture<'_, ()> {
        Box::pin(async {
            Err(protocol(
                "caller process signals are outside R2 fixed execution",
            ))
        })
    }
    fn terminate(&self) -> ExecProcessFuture<'_, ()> {
        Box::pin(async {
            Err(protocol(
                "caller termination is outside R2 fixed execution; the admitted domain enforces the runtime bound",
            ))
        })
    }
}
fn protocol(message: impl Into<String>) -> ExecServerError {
    ExecServerError::Protocol(message.into())
}
