use super::windows_common::finish_driver_spawn;
use super::windows_common::make_runner_resizer;
use super::windows_common::start_runner_pipe_writer;
use super::windows_common::start_runner_stdin_writer;
use super::windows_common::start_runner_stdout_reader;
use crate::desktop::DesktopPolicy;
use crate::identity::SandboxCreds;
use crate::identity::refresh_logon_sandbox_creds_with_diagnostics;
use crate::ipc_framed::EmptyPayload;
use crate::ipc_framed::FramedMessage;
use crate::ipc_framed::IPC_PROTOCOL_VERSION;
use crate::ipc_framed::Message;
use crate::ipc_framed::SpawnRequest;
use crate::resolved_permissions::ResolvedWindowsSandboxPermissions;
use crate::runner_client::RunnerTransport;
use crate::runner_client::retry_runner_spawn_once_with_diagnostics;
use crate::spawn_prep::prepare_elevated_spawn_context_for_permissions_with_diagnostics;
use anyhow::Result;
use codex_protocol::models::PermissionProfile;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_pty::ProcessDriver;
use codex_utils_pty::SpawnedProcess;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

struct RunnerTransportRequest {
    permissions: ResolvedWindowsSandboxPermissions,
    codex_home: PathBuf,
    cwd: PathBuf,
    env_map: HashMap<String, String>,
    logs_base_dir: Option<PathBuf>,
    spawn_request: SpawnRequest,
    read_roots_override: Option<Vec<PathBuf>>,
    read_roots_include_platform_defaults: bool,
    write_roots_override: Option<Vec<PathBuf>>,
    deny_read_paths_override: Vec<PathBuf>,
    deny_write_paths_override: Vec<PathBuf>,
    proxy_enforced: bool,
    proxy_settings_mode: crate::WindowsSandboxProxySettingsMode,
}

fn spawn_runner_transport_with_retry<T>(
    sandbox_creds: SandboxCreds,
    request: &RunnerTransportRequest,
    spawn: impl FnMut(&Path, &Path, &SandboxCreds, Option<&Path>, SpawnRequest) -> Result<T>,
    refresh: impl FnOnce(
        &ResolvedWindowsSandboxPermissions,
        &Path,
        &HashMap<String, String>,
        &Path,
        Option<&[PathBuf]>,
        bool,
        Option<&[PathBuf]>,
        &[PathBuf],
        &[PathBuf],
        bool,
        crate::WindowsSandboxProxySettingsMode,
    ) -> Result<SandboxCreds>,
) -> Result<T> {
    spawn_runner_transport_with_retry_diagnostics(sandbox_creds, request, spawn, refresh, None)
}

fn spawn_runner_transport_with_retry_diagnostics<T>(
    sandbox_creds: SandboxCreds,
    request: &RunnerTransportRequest,
    mut spawn: impl FnMut(&Path, &Path, &SandboxCreds, Option<&Path>, SpawnRequest) -> Result<T>,
    refresh: impl FnOnce(
        &ResolvedWindowsSandboxPermissions,
        &Path,
        &HashMap<String, String>,
        &Path,
        Option<&[PathBuf]>,
        bool,
        Option<&[PathBuf]>,
        &[PathBuf],
        &[PathBuf],
        bool,
        crate::WindowsSandboxProxySettingsMode,
    ) -> Result<SandboxCreds>,
    diagnostic: Option<&crate::WindowsStartDiagnostic>,
) -> Result<T> {
    retry_runner_spawn_once_with_diagnostics(
        sandbox_creds,
        &request.spawn_request.command,
        |sandbox_creds| {
            spawn(
                &request.codex_home,
                &request.cwd,
                &sandbox_creds,
                request.logs_base_dir.as_deref(),
                request.spawn_request.clone(),
            )
        },
        || {
            refresh(
                &request.permissions,
                &request.cwd,
                &request.env_map,
                &request.codex_home,
                request.read_roots_override.as_deref(),
                request.read_roots_include_platform_defaults,
                request.write_roots_override.as_deref(),
                &request.deny_read_paths_override,
                &request.deny_write_paths_override,
                request.proxy_enforced,
                request.proxy_settings_mode,
            )
        },
        diagnostic,
    )
}

async fn spawn_runner_transport_task(
    sandbox_creds: SandboxCreds,
    request: RunnerTransportRequest,
    matched_runner: Option<std::sync::Arc<crate::MatchedRunnerArtifact>>,
    diagnostic: Option<crate::WindowsStartDiagnostic>,
) -> Result<RunnerTransport> {
    if let Some(diagnostic) = &diagnostic {
        diagnostic.mark(crate::WindowsStartStage::BlockingTransportTask);
    }
    let task_diagnostic = diagnostic.clone();
    tokio::task::spawn_blocking(move || -> Result<_> {
        let diagnostic = task_diagnostic.as_ref();
        if request.spawn_request.private_desktop_name.is_none() {
            if let Some(diagnostic) = diagnostic {
                diagnostic.mark(crate::WindowsStartStage::DesktopPolicy);
            }
        }
        let desktop_policy = request
            .spawn_request
            .private_desktop_name
            .is_none()
            .then(|| {
                DesktopPolicy::elevated(
                    crate::setup::SandboxSetupRequest {
                        permissions: &request.permissions,
                        command_cwd: &request.cwd,
                        env_map: &request.env_map,
                        codex_home: &request.codex_home,
                        proxy_enforced: request.proxy_enforced,
                    },
                    crate::setup::SetupRootOverrides {
                        read_roots: request.read_roots_override.clone(),
                        read_roots_include_platform_defaults: request
                            .read_roots_include_platform_defaults,
                        write_roots: request.write_roots_override.clone(),
                        deny_read_paths: Some(request.deny_read_paths_override.clone()),
                        deny_write_paths: Some(request.deny_write_paths_override.clone()),
                    },
                    &request.spawn_request.cap_sids,
                    request
                        .spawn_request
                        .network_proxy_restricting_sid
                        .as_deref(),
                )
            })
            .transpose().inspect_err(|err| observe_backend_error(diagnostic, err))?;
        spawn_runner_transport_with_retry_diagnostics(
            sandbox_creds,
            &request,
            |codex_home, cwd, sandbox_creds, log_dir, spawn_request| {
                crate::runner_client::spawn_runner_transport_with_identity_diagnostics(
                    codex_home,
                    cwd,
                    sandbox_creds,
                    log_dir,
                    spawn_request,
                    desktop_policy.as_ref(),
                    matched_runner.clone(),
                    diagnostic,
                )
            },
            |permissions, cwd, env_map, codex_home, read_roots, read_defaults, write_roots,
             deny_read, deny_write, proxy, proxy_mode| {
                refresh_logon_sandbox_creds_with_diagnostics(
                    permissions, cwd, env_map, codex_home, read_roots, read_defaults,
                    write_roots, deny_read, deny_write, proxy, proxy_mode, diagnostic,
                )
            },
            diagnostic,
        )
    })
    .await
    .map_err(|err| {
        if let Some(diagnostic) = &diagnostic {
            diagnostic.fail(if err.is_cancelled() {
                crate::WindowsStartError::BlockingCancelled
            } else if err.is_panic() {
                crate::WindowsStartError::BlockingPanic
            } else {
                crate::WindowsStartError::Unclassified
            });
        }
        anyhow::anyhow!("runner handshake task failed: {err}")
    })?
}

fn observe_backend_error(diagnostic: Option<&crate::WindowsStartDiagnostic>, err: &anyhow::Error) {
    if let Some(diagnostic) = diagnostic {
        if let Some(err) = err.downcast_ref::<std::io::Error>() {
            diagnostic.fail_io_kind(err.kind());
        } else {
            diagnostic.fail(crate::WindowsStartError::Unclassified);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn spawn_windows_sandbox_session_elevated_for_permission_profile(
    permission_profile: &PermissionProfile,
    workspace_roots: &[AbsolutePathBuf],
    codex_home: &Path,
    command: Vec<String>,
    cwd: &Path,
    mut env_map: HashMap<String, String>,
    proxy_enforced: bool,
    network_proxy_restricting_sid: Option<String>,
    proxy_settings_mode: crate::WindowsSandboxProxySettingsMode,
    timeout_ms: Option<u64>,
    read_roots_override: Option<&[PathBuf]>,
    read_roots_include_platform_defaults: bool,
    write_roots_override: Option<&[PathBuf]>,
    deny_read_paths_override: &[AbsolutePathBuf],
    deny_write_paths_override: &[AbsolutePathBuf],
    tty: bool,
    stdin_open: bool,
    private_desktop_name: Option<String>,
    matched_runner: Option<std::sync::Arc<crate::MatchedRunnerArtifact>>,
    diagnostic: Option<crate::WindowsStartDiagnostic>,
) -> Result<SpawnedProcess> {
    let deny_read_paths_override = deny_read_paths_override
        .iter()
        .map(AbsolutePathBuf::to_path_buf)
        .collect::<Vec<_>>();
    let deny_write_paths_override = deny_write_paths_override
        .iter()
        .map(AbsolutePathBuf::to_path_buf)
        .collect::<Vec<_>>();
    if let Some(diagnostic) = &diagnostic {
        diagnostic.mark(crate::WindowsStartStage::PermissionResolve);
    }
    let permissions =
        ResolvedWindowsSandboxPermissions::try_from_permission_profile_for_workspace_roots(
            permission_profile,
            workspace_roots,
        ).inspect_err(|err| observe_backend_error(diagnostic.as_ref(), err))?;
    let elevated = prepare_elevated_spawn_context_for_permissions_with_diagnostics(
        permissions.clone(),
        codex_home,
        cwd,
        &mut env_map,
        &command,
        read_roots_override,
        read_roots_include_platform_defaults,
        write_roots_override,
        &deny_read_paths_override,
        &deny_write_paths_override,
        proxy_enforced,
        proxy_settings_mode,
        diagnostic.as_ref(),
    )?;

    let sandbox_creds = elevated.sandbox_creds;
    let request = RunnerTransportRequest {
        permissions,
        codex_home: codex_home.to_path_buf(),
        cwd: cwd.to_path_buf(),
        env_map: env_map.clone(),
        logs_base_dir: elevated.logs_base_dir,
        spawn_request: SpawnRequest {
            command,
            cwd: cwd.to_path_buf(),
            env: env_map,
            permission_profile: permission_profile.clone(),
            workspace_roots: workspace_roots.to_vec(),
            codex_home: elevated.sandbox_base,
            real_codex_home: codex_home.to_path_buf(),
            cap_sids: elevated.cap_sids,
            network_proxy_restricting_sid,
            timeout_ms,
            tty,
            stdin_open,
            private_desktop_name,
        },
        read_roots_override: read_roots_override.map(<[PathBuf]>::to_vec),
        read_roots_include_platform_defaults,
        write_roots_override: write_roots_override.map(<[PathBuf]>::to_vec),
        deny_read_paths_override,
        deny_write_paths_override,
        proxy_enforced,
        proxy_settings_mode,
    };
    let checked = matched_runner.is_some();
    let transport = spawn_runner_transport_task(sandbox_creds, request, matched_runner, diagnostic.clone()).await?;
    if checked {
        if let Some(diagnostic) = &diagnostic {
            diagnostic.mark(crate::WindowsStartStage::CheckedDriverAssemble);
        }
        let (pipe_write, pipe_read, hello, capabilities, artifact) =
            transport.into_checked_files().inspect_err(|err| observe_backend_error(diagnostic.as_ref(), err))?;
        let (writer_tx, writer_rx) = mpsc::channel::<Vec<u8>>(128);
        let (stdout_tx, stdout_rx) = broadcast::channel(256);
        let (stderr_tx, stderr_rx) = if tty {
            (None, None)
        } else {
            let (tx, rx) = broadcast::channel(256);
            (Some(tx), Some(rx))
        };
        let (exit_tx, exit_rx) = oneshot::channel();
        let checked = super::windows_common::start_checked_runner(
            pipe_write,
            pipe_read,
            hello,
            capabilities,
            artifact,
            writer_rx,
            tty,
            stdout_tx,
            stderr_tx,
            exit_tx,
        );
        let terminator_controls = checked.controls.clone();
        let driver = ProcessDriver {
            writer_tx,
            stdout_rx,
            stderr_rx,
            exit_rx,
            terminator: Some(Box::new(move || terminator_controls.terminate())),
            writer_handle: Some(checked.writer),
            resizer: None,
            tty,
        };
        let spawned =
            codex_utils_pty::spawn_from_driver_with_checked_controls(driver, checked.controls);
        if !stdin_open {
            spawned.session.close_stdin();
        }
        if let Some(diagnostic) = &diagnostic {
            diagnostic.mark(crate::WindowsStartStage::SpawnReturned);
        }
        return Ok(spawned);
    }
    let (pipe_write, pipe_read) = transport.into_files();

    let (writer_tx, writer_rx) = mpsc::channel::<Vec<u8>>(128);
    let (stdout_tx, stdout_rx) = broadcast::channel::<Vec<u8>>(256);
    let stderr_rx = if tty {
        None
    } else {
        Some(broadcast::channel::<Vec<u8>>(256))
    };
    let (exit_tx, exit_rx) = oneshot::channel::<i32>();

    let outbound_tx = start_runner_pipe_writer(pipe_write);
    let writer_handle = start_runner_stdin_writer(writer_rx, outbound_tx.clone(), tty, stdin_open);
    let terminator = {
        let outbound_tx = outbound_tx.clone();
        Some(Box::new(move || {
            let _ = outbound_tx.send(FramedMessage {
                version: IPC_PROTOCOL_VERSION,
                message: Message::Terminate {
                    payload: EmptyPayload::default(),
                },
            });
        }) as Box<dyn FnMut() + Send + Sync>)
    };

    start_runner_stdout_reader(
        pipe_read,
        stdout_tx,
        stderr_rx.as_ref().map(|(tx, _rx)| tx.clone()),
        exit_tx,
    );

    Ok(finish_driver_spawn(
        ProcessDriver {
            writer_tx,
            stdout_rx,
            stderr_rx: stderr_rx.map(|(_tx, rx)| rx),
            exit_rx,
            terminator,
            writer_handle: Some(writer_handle),
            resizer: if tty {
                Some(make_runner_resizer(outbound_tx))
            } else {
                None
            },
            tty,
        },
        stdin_open,
    ))
}

#[cfg(test)]
#[path = "elevated_tests.rs"]
mod tests;
