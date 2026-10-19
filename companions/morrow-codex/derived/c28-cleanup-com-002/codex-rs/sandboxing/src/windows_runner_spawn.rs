//! Explicit matched Elevated route; default spawn_process remains unchanged.
use std::sync::Arc;
use std::path::{Path, PathBuf};

use anyhow::Context;
use anyhow::Result;
use codex_protocol::config_types::WindowsSandboxLevel;
use codex_utils_pty::SpawnedProcess;

use crate::MatchedRunnerArtifact;
use crate::SandboxType;
use crate::SpawnRequest;
use crate::WindowsStartDiagnostic;
use crate::WindowsStartError;
use crate::WindowsStartStage;
use crate::terminal_queries::respond_to_terminal_queries;

/// Select only a home already carried by the original A1 binding. The later
/// canonical identity checks in prepare_asuser_spawn_context remain mandatory.
/// Ordinary matched runners retain their original resolver, called once.
fn matched_codex_home(
    owned_setup_home: Option<&Path>,
    ambient: impl FnOnce() -> std::io::Result<PathBuf>,
) -> std::io::Result<PathBuf> {
    if let Some(home) = owned_setup_home {
        if !home.is_absolute() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput,
                "matched A1 setup home must be absolute"));
        }
        // Copy data only. Do not create/canonicalize it here or fall back to
        // a different home if the original later identity check fails.
        Ok(home.to_path_buf())
    } else {
        ambient()
    }
}

/// Spawn only through the exact pinned checked runner and the original sandbox.
/// A mismatch is an error, never a legacy, MXC or ordinary-process fallback.
pub async fn spawn_process_with_windows_runner(
    request: SpawnRequest<'_>,
    runner: Arc<MatchedRunnerArtifact>,
) -> Result<SpawnedProcess> {
    spawn_process_with_windows_runner_diagnostics(request, runner, None).await
}

/// Observe the same matched route using data scoped to this invocation.
pub async fn spawn_process_with_windows_runner_diagnostics(
    request: SpawnRequest<'_>,
    runner: Arc<MatchedRunnerArtifact>,
    diagnostic: Option<WindowsStartDiagnostic>,
) -> Result<SpawnedProcess> {
    if let Some(diagnostic) = &diagnostic {
        diagnostic.mark(WindowsStartStage::MatchedRouteCheck);
    }
    let windows = (|| {
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
            matches!(runner.protocol_version(),7|8),
            "matched runner version mismatch"
        );
        anyhow::ensure!(runner.protocol_version()!=8 || !request.tty,
            "v8 native completion currently requires noninteractive pipe mode");
        Ok(windows)
    })()
    .inspect_err(|_| {
        if let Some(diagnostic) = &diagnostic {
            diagnostic.fail(WindowsStartError::RouteRejected);
        }
    })?;
    if let Some(diagnostic) = &diagnostic {
        diagnostic.mark(WindowsStartStage::CodexHomeResolve);
    }
    let codex_home = matched_codex_home(runner.asuser_setup_codex_home(), || {
        codex_utils_home_dir::find_codex_home().map(|home| home.into_path_buf())
    })
        .inspect_err(|error| {
            if let Some(diagnostic) = &diagnostic {
                diagnostic.fail_io_kind(error.kind());
            }
        })
        .context("windows sandbox: failed to resolve codex_home")?;
    let empty_paths = &[];
    let overrides = windows.filesystem_overrides;
    let spawned = codex_windows_sandbox::spawn_windows_sandbox_session_for_level_with_runner_diagnostics(
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
        diagnostic,
    )
    .await?;
    Ok(if request.tty {
        respond_to_terminal_queries(spawned)
    } else {
        spawned
    })
}

#[cfg(test)]
mod home_source_tests2440 {
    use super::matched_codex_home;
    use std::cell::Cell;
    use std::io::{Error, ErrorKind};
    use std::path::{Path, PathBuf};

    #[test]
    fn same_owned_setup_home_is_selected_without_reading_ambient_or_probing_paths() {
        // Memory-only label: neither this helper nor this test stats/creates it.
        let home = Path::new(r"C:\memory-only2440\case\codex-home");
        let calls = Cell::new(0);
        let actual = matched_codex_home(Some(home), || {
            calls.set(calls.get() + 1);
            Err(Error::new(ErrorKind::NotFound, "labelled ambient default unavailable"))
        }).unwrap();
        assert_eq!(actual.as_path(), home);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn ambient_default_cannot_override_same_attempt_owned_home_even_if_successful() {
        let owned = Path::new(r"C:\memory-only2440\case\codex-home");
        let calls = Cell::new(0);
        let actual = matched_codex_home(Some(owned), || {
            calls.set(calls.get() + 1);
            Ok(PathBuf::from(r"C:\memory-only2440\unrelated-profile\.codex"))
        }).unwrap();
        assert_eq!(actual.as_path(), owned);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn relative_owned_home_is_rejected_without_current_dir_resolution_or_fallback() {
        for invalid in ["", "relative-home", r"C:drive-relative", r"\root-relative"] {
            let calls = Cell::new(0);
            let error = matched_codex_home(Some(Path::new(invalid)), || {
                calls.set(calls.get() + 1);
                Ok(PathBuf::from(r"C:\memory-only2440\unrelated-profile\.codex"))
            }).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::InvalidInput);
            assert_eq!(error.to_string(), "matched A1 setup home must be absolute");
            assert_eq!(calls.get(), 0);
        }
    }

    #[test]
    fn ordinary_matched_route_keeps_original_resolver_result_and_exactly_one_call() {
        let calls = Cell::new(0);
        let expected = PathBuf::from(r"C:\memory-only2440\original-default\.codex");
        let result = matched_codex_home(None, || {
            calls.set(calls.get() + 1);
            Ok(expected.clone())
        }).unwrap();
        assert_eq!(result, expected);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn ordinary_resolver_error_is_not_retried_swallowed_or_reclassified() {
        let calls = Cell::new(0);
        let error = matched_codex_home(None, || {
            calls.set(calls.get() + 1);
            Err(Error::new(ErrorKind::NotFound, "labelled original resolver error"))
        }).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert_eq!(error.to_string(), "labelled original resolver error");
        assert_eq!(calls.get(), 1);
    }
}
