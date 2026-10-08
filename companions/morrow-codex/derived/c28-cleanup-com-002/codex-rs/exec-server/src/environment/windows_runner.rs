//! Trusted local matched-runner construction; no guest handle or new owner.
use super::*;
use codex_sandboxing::MatchedRunnerArtifact;

impl Environment {
    /// Builds one local execution environment with an independently locked runner.
    /// This is provisioning, not approval or OS sandbox acceptance. Requests must
    /// still pass their original owner/admission fences and Elevated preflight.
    pub fn create_with_windows_runner(
        local_runtime_paths: ExecServerRuntimeOptions,
        http_client_factory: HttpClientFactory,
        runner: Arc<MatchedRunnerArtifact>,
    ) -> Result<Self, ExecServerError> {
        if runner.protocol_version() != 7 {
            return Err(ExecServerError::Protocol(
                "matched runner version mismatch".into(),
            ));
        }
        Ok(Self {
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
        })
    }
}
