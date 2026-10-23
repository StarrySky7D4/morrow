//! Trusted local matched-runner construction; no guest handle or new owner.
use super::*;
use codex_sandboxing::MatchedRunnerArtifact;

const CHECKED_V7_SCHEMA: [u8; 32] = codex_sandboxing::WINDOWS_RUNNER_SCHEMA;
const NATIVE_CLOSE_V8_SCHEMA: [u8; 32] = codex_sandboxing::WINDOWS_NATIVE_CLOSE_SCHEMA;

impl Environment {
    /// Builds one local execution environment with an independently locked runner.
    /// This is provisioning, not approval or OS sandbox acceptance. Requests must
    /// still pass their original owner/admission fences and Elevated preflight.
    pub fn create_with_windows_runner(
        local_runtime_paths: ExecServerRuntimeOptions,
        http_client_factory: HttpClientFactory,
        runner: Arc<MatchedRunnerArtifact>,
    ) -> Result<Self, ExecServerError> {
        if runner.protocol_version() != 7 || runner.control_schema_sha256() != CHECKED_V7_SCHEMA {
            return Err(ExecServerError::Protocol(
                "matched runner version mismatch".into(),
            ));
        }
        Ok(Self::create_with_pinned_windows_runner(local_runtime_paths, http_client_factory, runner))
    }

    /// Explicit v8 opt-in on the same retained artifact. The v7 constructor
    /// never upgrades, and neither constructor falls back to the default route.
    /// This is local provisioning, not a startup/handshake or native-exit witness.
    pub fn create_with_native_close_windows_runner(
        local_runtime_paths: ExecServerRuntimeOptions,
        http_client_factory: HttpClientFactory,
        runner: Arc<MatchedRunnerArtifact>,
    ) -> Result<Self, ExecServerError> {
        if runner.protocol_version() != 8 || runner.control_schema_sha256() != NATIVE_CLOSE_V8_SCHEMA {
            return Err(ExecServerError::Protocol(
                "native-close runner version/schema mismatch".into(),
            ));
        }
        Ok(Self::create_with_pinned_windows_runner(local_runtime_paths, http_client_factory, runner))
    }

    fn create_with_pinned_windows_runner(
        local_runtime_paths: ExecServerRuntimeOptions,
        http_client_factory: HttpClientFactory,
        runner: Arc<MatchedRunnerArtifact>,
    ) -> Self {
        Self {
            injected_backend: None,
            remote_client: None,
            ready_info: Arc::new(ArcSwapOption::empty()),
            provisioning_status_tx: None,
            startup_task: Arc::new(Mutex::new(None)),
            exec_backend: Arc::new(LocalProcess::with_windows_runner(
                local_runtime_paths.clone(),
                runner,
            )),
            filesystem: Arc::new(LocalFileSystem::with_runtime_paths(
                local_runtime_paths.clone(),
            )),
            http_client: Arc::new(RouteAwareHttpClient::new(http_client_factory)),
            local_runtime_paths: Some(local_runtime_paths),
        }
    }
}
