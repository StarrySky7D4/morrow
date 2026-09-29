//! Bounded bridge into the original unified-exec dispatcher; no synthetic loop.
use crate::exec::{ExecCapturePolicy, ExecExpiration};
use crate::sandboxing::ExecRequest;
use crate::unified_exec::{NoopSpawnLifecycle, UnifiedExecProcessManager};
use codex_protocol::config_types::{WindowsSandboxLevel, WindowsSandboxProxySettingsMode};
use codex_protocol::models::PermissionProfile;
use codex_sandboxing::SandboxType;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::collections::HashMap;

/// Exercises one actual prepared-request dispatch. Callers must supply a refusing backend.
/// A returned error proves only this call's propagation, never global spawn isolation.
pub async fn dispatch_refusal_probe(
    environment: &codex_exec_server::Environment,
    tty: bool,
) -> Result<(), String> {
    if !environment.has_injected_capabilities() {
        return Err("qualification requires explicitly injected capabilities".into());
    }
    let request = ExecRequest::new(
        vec![
            "morrow-qualification-never-launch".into(),
            "fixture-argument".into(),
        ],
        AbsolutePathBuf::try_from(r"C:\morrow-qualification-nonexistent")
            .map_err(|error| error.to_string())?,
        HashMap::new(),
        /*network*/ None,
        /*network_environment_id*/ None,
        ExecExpiration::DefaultTimeout,
        ExecCapturePolicy::ShellTool,
        SandboxType::None,
        vec![],
        WindowsSandboxLevel::Disabled,
        PermissionProfile::read_only(),
        /*arg0*/ None,
    );
    UnifiedExecProcessManager::default()
        .open_session_with_prepared_exec_env(
            /*process_id*/ 2,
            &request,
            /*tool_ctx*/ None,
            WindowsSandboxProxySettingsMode::Preserve,
            /*network_policy_decider*/ None,
            tty,
            Box::new(NoopSpawnLifecycle),
            environment,
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}
