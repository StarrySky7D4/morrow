//! Optional routing of the original LocalProcess to the exact retained runner.
use super::*;
use crate::WindowsSandboxSelection;
use codex_sandboxing::MatchedRunnerArtifact;
use codex_sandboxing::WindowsStartDiagnostic;

impl LocalProcess {
    pub(crate) fn with_windows_runner(
        runtime_paths: ExecServerRuntimeOptions,
        runner: Arc<MatchedRunnerArtifact>,
    ) -> Self {
        let mut local = Self::with_discarded_notifications(Some(runtime_paths));
        local.windows_runner = Some(runner);
        local
    }

    pub(super) fn validate_windows_runner_selection(
        &self,
        params: &ExecParams,
    ) -> Result<(), JSONRPCErrorError> {
        if self.windows_runner.is_some()
            && params.sandbox.as_ref().is_none_or(|context| {
                context.windows_sandbox_selection != WindowsSandboxSelection::Elevated
            })
        {
            return Err(invalid_request(
                "matched runner requires Elevated Windows sandbox".into(),
            ));
        }
        Ok(())
    }

    pub(super) async fn spawn_with_windows_runner(
        &self,
        request: codex_sandboxing::SpawnRequest<'_>,
        diagnostic: Option<WindowsStartDiagnostic>,
    ) -> anyhow::Result<codex_utils_pty::SpawnedProcess> {
        match &self.windows_runner {
            Some(runner) => {
                codex_sandboxing::spawn_process_with_windows_runner_diagnostics(
                    request,
                    Arc::clone(runner),
                    diagnostic,
                )
                .await
            }
            None => codex_sandboxing::spawn_process(request).await,
        }
    }
}
