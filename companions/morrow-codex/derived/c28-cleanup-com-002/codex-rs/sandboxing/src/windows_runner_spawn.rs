//! Explicit matched Elevated route; default spawn_process remains unchanged.
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use codex_protocol::config_types::WindowsSandboxLevel;
use codex_utils_pty::SpawnedProcess;

use crate::MatchedRunnerArtifact;
use crate::SandboxType;
use crate::SpawnRequest;
use crate::terminal_queries::respond_to_terminal_queries;

/// Spawn only through the exact pinned checked runner and the original sandbox.
/// A mismatch is an error, never a legacy, MXC or ordinary-process fallback.
pub async fn spawn_process_with_windows_runner(
    request: SpawnRequest<'_>,
    runner: Arc<MatchedRunnerArtifact>,
) -> Result<SpawnedProcess> {
    anyhow::ensure!(
        request.sandbox == SandboxType::WindowsRestrictedToken,
        "matched runner requires Windows restricted sandbox"
    );
    let windows = request
        .windows_sandbox
        .context("missing Windows sandbox spawn request")?;
    anyhow::ensure!(
        windows.windows_sandbox_level == WindowsSandboxLevel::Elevated,
        "matched runner requires Elevated selection"
    );
    anyhow::ensure!(
        runner.protocol_version() == 7,
        "matched runner version mismatch"
    );
    let codex_home = codex_utils_home_dir::find_codex_home()
        .context("windows sandbox: failed to resolve codex_home")?;
    let empty_paths = &[];
    let overrides = windows.filesystem_overrides;
    let spawned = codex_windows_sandbox::spawn_windows_sandbox_session_for_level_with_runner(
        codex_windows_sandbox::WindowsSandboxSessionRequest {
            permission_profile: windows.permission_profile,
            workspace_roots: windows.workspace_roots,
            codex_home: codex_home.as_path(),
            command: request.command.to_vec(),
            cwd: request.cwd,
            env_map: request.env.clone(),
            windows_sandbox_level: windows.windows_sandbox_level,
            proxy_enforced: windows.proxy_enforced,
            network_proxy_restricting_sid: windows.network_proxy_restricting_sid.map(str::to_owned),
            proxy_settings_mode: windows.proxy_settings_mode,
            timeout_ms: None,
            read_roots_override: overrides.and_then(|v| v.read_roots_override.as_deref()),
            read_roots_include_platform_defaults: overrides
                .is_some_and(|v| v.read_roots_include_platform_defaults),
            write_roots_override: overrides.and_then(|v| v.write_roots_override.as_deref()),
            deny_read_paths_override: overrides
                .map_or(empty_paths, |v| v.additional_deny_read_paths.as_slice()),
            deny_write_paths_override: overrides
                .map_or(empty_paths, |v| v.additional_deny_write_paths.as_slice()),
            tty: request.tty,
            stdin_open: request.stdin_open,
        },
        runner,
    )
    .await?;
    Ok(if request.tty {
        respond_to_terminal_queries(spawned)
    } else {
        spawned
    })
}
