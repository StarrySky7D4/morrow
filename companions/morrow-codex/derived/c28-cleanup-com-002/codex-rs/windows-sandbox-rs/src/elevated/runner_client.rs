use crate::app_package::registered_core_requested;
use crate::app_package::verify_registered_core_runner;
use crate::desktop::DesktopPolicy;
use crate::desktop::LaunchDesktop;
use crate::desktop::shared_private_desktop_for_user;
use crate::identity::SandboxCreds;
use crate::ipc_framed::CHECKED_IPC_PROTOCOL_VERSION;
use crate::ipc_framed::ErrorPayload;
use crate::ipc_framed::ErrorStage;
use crate::ipc_framed::FramedMessage;
use crate::ipc_framed::IPC_PROTOCOL_VERSION;
use crate::ipc_framed::Message;
use crate::ipc_framed::SpawnRequest;
use crate::ipc_framed::read_frame;
use crate::ipc_framed::write_frame;
use crate::runner_pipe::PIPE_ACCESS_INBOUND;
use crate::runner_pipe::PIPE_ACCESS_OUTBOUND;
use crate::runner_pipe::connect_pipe;
use crate::runner_pipe::create_named_pipe;
use crate::runner_pipe::find_runner_exe;
use crate::runner_pipe::pipe_pair;
use crate::winutil::quote_windows_arg;
use crate::winutil::to_wide;
use anyhow::Context;
use anyhow::Result;
use std::fs::File;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::FromRawHandle;
use std::os::windows::io::OwnedHandle;
use std::path::Path;
use std::ptr;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::DUPLICATE_SAME_ACCESS;
use windows_sys::Win32::Foundation::DuplicateHandle;
use windows_sys::Win32::Foundation::ERROR_ACCOUNT_DISABLED;
use windows_sys::Win32::Foundation::ERROR_LOGON_FAILURE;
use windows_sys::Win32::Foundation::ERROR_NO_SUCH_LOGON_SESSION;
use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode;
use windows_sys::Win32::System::IO::CancelSynchronousIo;
use windows_sys::Win32::System::Threading::CreateProcessWithLogonW;
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::System::Threading::GetCurrentThread;
use windows_sys::Win32::System::Threading::GetExitCodeProcess;
use windows_sys::Win32::System::Threading::LOGON_WITH_PROFILE;
use windows_sys::Win32::System::Threading::PROCESS_INFORMATION;
use windows_sys::Win32::System::Threading::STARTF_FORCEOFFFEEDBACK;
use windows_sys::Win32::System::Threading::STARTUPINFOW;
use windows_sys::Win32::System::Threading::TerminateProcess;
use windows_sys::Win32::System::Threading::WaitForSingleObject;

const RUNNER_SPAWN_READY_TIMEOUT: Duration = Duration::from_secs(15);
const RUNNER_PIPE_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RUNNER_ERROR_MODE_FLAGS: u32 = 0x0001 | 0x0002;
const WAIT_OBJECT_0: u32 = 0;

#[derive(Debug)]
struct RunnerLogonError {
    code: u32,
}

impl std::fmt::Display for RunnerLogonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CreateProcessWithLogonW failed: {}", self.code)
    }
}

impl std::error::Error for RunnerLogonError {}

#[derive(Debug)]
pub(crate) struct RunnerStartupError {
    payload: ErrorPayload,
}

impl RunnerStartupError {
    pub(crate) fn new(payload: ErrorPayload) -> Self {
        Self { payload }
    }
}

impl std::fmt::Display for RunnerStartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "runner failed during {:?}: {}",
            self.payload.stage, self.payload.message
        )?;
        if let Some(code) = self.payload.windows_error_code {
            write!(f, " (Windows error {code})")?;
        }
        Ok(())
    }
}

impl std::error::Error for RunnerStartupError {}

pub(crate) struct RunnerTransport {
    pipe_write: File,
    pipe_read: File,
    hello: crate::ipc_framed::ControlHello,
    controls: crate::ipc_framed::RunnerControlCapabilities,
    matched_runner: Option<std::sync::Arc<crate::MatchedRunnerArtifact>>,
    checked_lease: Option<super::checked_owner::CheckedTransportLease>,
    startup_deadline: Instant,
    native_close_runtime: Option<std::sync::Arc<super::native_close_runtime::NativeCloseRuntime>>,
    native_startup_owner: Option<std::sync::Arc<super::checked_owner::CheckedRunnerOwner>>,
}

fn is_refreshable_windows_error(code: u32) -> bool {
    matches!(
        code,
        ERROR_ACCOUNT_DISABLED | ERROR_LOGON_FAILURE | ERROR_NO_SUCH_LOGON_SESSION
    )
}

fn command_targets_windows_apps(command: &[String]) -> bool {
    command.first().is_some_and(|program| {
        Path::new(program).components().any(|component| {
            component
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case("WindowsApps")
        })
    })
}

pub(crate) fn is_refreshable_sandbox_creds_error(err: &anyhow::Error, command: &[String]) -> bool {
    if super::checked_owner::is_error_marked_non_refreshable(err) {
        return false;
    }

    if err
        .downcast_ref::<RunnerLogonError>()
        .is_some_and(|err| is_refreshable_windows_error(err.code))
    {
        return true;
    }

    err.downcast_ref::<RunnerStartupError>().is_some_and(|err| {
        err.payload.stage == ErrorStage::SpawnChild
            && err.payload.windows_error_code.is_some_and(|code| {
                // AppX activation can return 1312 for a healthy sandbox token. Rotating the
                // account password cannot make the same WindowsApps command launch.
                is_refreshable_windows_error(code)
                    && (code != ERROR_NO_SUCH_LOGON_SESSION
                        || !command_targets_windows_apps(command))
            })
    })
}

pub(crate) fn retry_runner_spawn_once<T>(
    sandbox_creds: SandboxCreds,
    command: &[String],
    spawn: impl FnMut(SandboxCreds) -> Result<T>,
    refresh: impl FnOnce() -> Result<SandboxCreds>,
) -> Result<T> {
    retry_runner_spawn_once_with_diagnostics(sandbox_creds, command, spawn, refresh, None)
}

pub(crate) fn retry_runner_spawn_once_with_diagnostics<T>(
    sandbox_creds: SandboxCreds,
    command: &[String],
    mut spawn: impl FnMut(SandboxCreds) -> Result<T>,
    refresh: impl FnOnce() -> Result<SandboxCreds>,
    diagnostic: Option<&crate::WindowsStartDiagnostic>,
) -> Result<T> {
    let result = match spawn(sandbox_creds) {
        Ok(result) => Ok(result),
        Err(err) if is_refreshable_sandbox_creds_error(&err, command) => {
            if let Some(diagnostic) = diagnostic {
                diagnostic.begin_existing_retry();
            }
            refresh().and_then(spawn)
        }
        Err(err) => Err(err),
    };
    super::runner_metrics::record("startup", if result.is_ok() { "success" } else { "error" });
    result
}

fn mark_start(diagnostic: Option<&crate::WindowsStartDiagnostic>, stage: crate::WindowsStartStage) {
    if let Some(diagnostic) = diagnostic {
        diagnostic.mark(stage);
    }
}

fn observe_start_error(
    diagnostic: Option<&crate::WindowsStartDiagnostic>,
    err: &anyhow::Error,
    fallback: crate::WindowsStartError,
) {
    if let Some(diagnostic) = diagnostic {
        if let Some(err) = err.downcast_ref::<RunnerLogonError>() {
            diagnostic.fail_os(crate::WindowsOsError::from_code(err.code));
        } else if let Some(err) = err.downcast_ref::<RunnerStartupError>() {
            let stage = match err.payload.stage {
                ErrorStage::ReadSpawnRequest => crate::WindowsRunnerErrorStage::ReadSpawnRequest,
                ErrorStage::SpawnChild => crate::WindowsRunnerErrorStage::SpawnChild,
                ErrorStage::WriteSpawnReady => crate::WindowsRunnerErrorStage::WriteSpawnReady,
                ErrorStage::TerminalWitness => crate::WindowsRunnerErrorStage::TerminalWitness,
            };
            diagnostic.fail_runner(stage, err.payload.windows_error_code
                .map(crate::WindowsOsError::from_code)
                .unwrap_or(crate::WindowsOsError::NotObserved));
        } else if let Some(err) = err.downcast_ref::<std::io::Error>() {
            diagnostic.fail_io_kind(err.kind());
        } else {
            diagnostic.fail(fallback);
        }
    }
}

impl RunnerTransport {
    pub(crate) fn startup_id(&self) -> Option<super::checked_owner::StartupId> {
        self.checked_lease
            .as_ref()
            .map(super::checked_owner::CheckedTransportLease::startup_id)
    }

    fn protocol_version(&self) -> u8 {
        if self.native_close_runtime.is_some() {8} else if self.matched_runner.is_some() {
            CHECKED_IPC_PROTOCOL_VERSION
        } else {
            IPC_PROTOCOL_VERSION
        }
    }
    fn negotiate_controls(&mut self, diagnostic: Option<&crate::WindowsStartDiagnostic>) -> Result<()> {
        let request = FramedMessage {
            version: CHECKED_IPC_PROTOCOL_VERSION,
            message: Message::Hello {
                payload: self.hello.clone(),
            },
        };
        if self.native_close_runtime.is_none() {
            write_frame(&mut self.pipe_write,&request)
                .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::ControlRejected))?;
        }
        (if self.native_close_runtime.is_some() {Ok(())}else{crate::framed_io::wait_for_complete_frame(
            &self.pipe_read,
            self.startup_deadline,
        )})
        .context("wait for matched runner control hello")
        .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::ControlRejected))?;
        let read = if let Some(runtime)=self.native_close_runtime.as_ref() {
            self.native_startup_owner.as_ref().context("original native startup reader owner absent")?
                .read_native_startup(|file| {let mut reader=file;runtime.read_message(&mut reader)})?.flatten()
        }else{read_frame(&mut self.pipe_read)?};
        let msg = read.ok_or_else(|| anyhow::anyhow!("runner closed before control hello"))
            .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::ControlRejected))?;
        anyhow::ensure!(
            msg.version == self.protocol_version(),
            "runner protocol mismatch before spawn"
        );
        match msg.message {
            Message::HelloAck { payload }
                if payload.hello == self.hello
                    && payload.hello.valid()
                    && payload.controls.close_stdin
                    && payload.controls.resize =>
            {
                self.controls = payload.controls;
                Ok(())
            }
            _ => Err(anyhow::anyhow!(
                "runner control identity mismatch before spawn"
            )),
        }
    }
    pub(crate) fn send_spawn_request(&mut self, request: SpawnRequest) -> Result<()> {
        let spawn_request = FramedMessage {
            version: self.protocol_version(),
            message: Message::SpawnRequest {
                payload: Box::new(request),
            },
        };
        if let Some(runtime)=self.native_close_runtime.clone() {
            self.native_startup_owner.as_ref().context("same original native startup owner absent")?
                .write_native_startup(1,move |file| {let mut writer=file;runtime.write_message(&mut writer,&spawn_request)})?;
            Ok(())
        }else{write_frame(&mut self.pipe_write,&spawn_request)}
    }

    pub(crate) fn read_spawn_ready(&mut self) -> Result<()> {
        self.read_spawn_ready_with_diagnostics(None)
    }

    fn read_spawn_ready_with_diagnostics(&mut self, diagnostic: Option<&crate::WindowsStartDiagnostic>) -> Result<()> {
        (if self.native_close_runtime.is_some() {Ok(())}else{crate::framed_io::wait_for_complete_frame(
            &self.pipe_read,
            self.startup_deadline,
        )})
        .context("wait for runner spawn_ready")
        .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::ControlRejected))?;
        let read = if let Some(runtime)=self.native_close_runtime.as_ref() {
            self.native_startup_owner.as_ref().context("original native startup reader owner absent")?
                .read_native_startup(|file| {let mut reader=file;runtime.read_message(&mut reader)})?.flatten()
        }else{read_frame(&mut self.pipe_read)?};
        let msg = read.ok_or_else(|| anyhow::anyhow!("runner pipe closed before spawn_ready"))?;
        anyhow::ensure!(
            msg.version == self.protocol_version(),
            "runner spawn_ready version mismatch"
        );
        match msg.message {
            Message::SpawnReady { payload } => {
                if self.matched_runner.is_some() {
                    // Hello proves the implementation contract; SpawnReady declares
                    // the actual child's handles. Legacy packets default false.
                    anyhow::ensure!(
                        (!payload.controls.close_stdin || self.controls.close_stdin)
                            && (!payload.controls.resize || self.controls.resize),
                        "runner actual controls exceed negotiated implementation"
                    );
                    self.controls = payload.controls;
                }
                Ok(())
            }
            Message::Error { payload } => {
                let err = anyhow::Error::from(RunnerStartupError::new(payload));
                observe_start_error(diagnostic, &err, crate::WindowsStartError::RunnerReported);
                Err(err)
            }
            other => Err(anyhow::anyhow!(
                "expected spawn_ready from runner, got {other:?}"
            )),
        }
    }

    pub(crate) fn into_checked_files(
        self,
    ) -> Result<super::checked_owner::CheckedRunnerIo> {
        let pin = self
            .matched_runner
            .context("checked runner requires retained independent artifact")?;
        let lease = self
            .checked_lease
            .context("checked runner requires retained startup owner lease")?;
        let (owner, identity, thread_disposition) = lease.into_io_token()?;
        Ok(super::checked_owner::CheckedRunnerIo {
            pipe_write: self.pipe_write,
            pipe_read: self.pipe_read,
            hello: self.hello,
            capabilities: self.controls,
            artifact: pin,
            identity,
            thread_disposition,
            owner,
        })
    }

    pub(crate) fn into_files(self) -> Result<(File, File)> {
        if let Some(ref lease) = self.checked_lease {
            lease.reject_legacy_conversion();
            anyhow::bail!("checked runner transport cannot be converted into legacy raw files");
        }
        if self.matched_runner.is_some() {
            anyhow::bail!("matched runner transport cannot be converted into legacy raw files");
        }
        Ok((self.pipe_write, self.pipe_read))
    }
}

fn try_take_completed_connect_result(
    connect_thread: &mut Option<thread::JoinHandle<()>>,
    connect_result_rx: &mpsc::Receiver<Result<()>>,
    thread_handle: HANDLE,
    pipe_label: &str,
) -> Result<Option<Result<()>>> {
    let thread_wait = unsafe { WaitForSingleObject(thread_handle, 0) };
    if thread_wait != WAIT_OBJECT_0 {
        return Ok(None);
    }

    if let Some(connect_thread) = connect_thread.take() {
        let _ = connect_thread.join();
    }

    let result = connect_result_rx.recv().map_err(|_| {
        anyhow::anyhow!("runner {pipe_label} connect thread exited before reporting its result")
    })?;
    Ok(Some(result))
}

fn connect_pipe_with_timeout(
    h_pipe: HANDLE,
    expected_runner_pid: u32,
    pipe_label: &str,
    diagnostic: Option<&crate::WindowsStartDiagnostic>,
) -> Result<()> {
    let pipe_label = pipe_label.to_string();
    let pipe_label_for_thread = pipe_label.clone();
    let (thread_handle_tx, thread_handle_rx) = mpsc::sync_channel(1);
    let (connect_result_tx, connect_result_rx) = mpsc::sync_channel(1);
    let connect_diagnostic = diagnostic.cloned();
    let mut connect_thread = Some(
        thread::Builder::new()
            .name(format!("codex-runner-connect-{pipe_label}"))
            .spawn(move || {
                let current_process = unsafe { GetCurrentProcess() };
                let mut thread_handle = 0;
                let duplicate_ok = unsafe {
                    DuplicateHandle(
                        current_process,
                        GetCurrentThread(),
                        current_process,
                        &mut thread_handle,
                        0,
                        0,
                        DUPLICATE_SAME_ACCESS,
                    )
                };
                if duplicate_ok == 0 {
                    let code = unsafe { GetLastError() };
                    if let Some(diagnostic) = &connect_diagnostic {
                        diagnostic.fail_os(crate::WindowsOsError::from_code(code));
                    }
                    let _ = thread_handle_tx.send(Err(anyhow::anyhow!(
                        "DuplicateHandle failed for runner {pipe_label_for_thread} connect thread: {}",
                        code
                    )));
                    return;
                }

                // Publish the helper thread HANDLE before the blocking pipe connect so the
                // parent can cancel this specific operation if it times out.
                let _ = thread_handle_tx.send(Ok(thread_handle));

                let result = connect_pipe(h_pipe, expected_runner_pid)
                    .map_err(anyhow::Error::from)
                    .context(format!("connect {pipe_label_for_thread}"))
                    .inspect_err(|err| observe_start_error(connect_diagnostic.as_ref(), err, crate::WindowsStartError::Unclassified));
                let _ = connect_result_tx.send(result);
            })?,
    );
    let thread_handle = thread_handle_rx.recv().map_err(|_| {
        anyhow::anyhow!("runner {pipe_label} connect thread exited before publishing its handle")
    })??;

    let result = match connect_result_rx.recv_timeout(RUNNER_PIPE_CONNECT_TIMEOUT) {
        Ok(result) => {
            if let Some(connect_thread) = connect_thread.take() {
                let _ = connect_thread.join();
            }
            result
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            if let Some(result) = try_take_completed_connect_result(
                &mut connect_thread,
                &connect_result_rx,
                thread_handle,
                &pipe_label,
            )? {
                result
            } else {
                let cancel_ok = unsafe { CancelSynchronousIo(thread_handle) };
                if cancel_ok == 0 {
                    let err = unsafe { GetLastError() };
                    if err != ERROR_NOT_FOUND {
                        if let Some(diagnostic) = diagnostic {
                            diagnostic.fail_os(crate::WindowsOsError::from_code(err));
                        }
                        Err(anyhow::anyhow!(
                            "CancelSynchronousIo failed for runner {pipe_label} connect thread: {err}"
                        ))
                    } else if let Some(result) = try_take_completed_connect_result(
                        &mut connect_thread,
                        &connect_result_rx,
                        thread_handle,
                        &pipe_label,
                    )? {
                        result
                    } else {
                        if let Some(diagnostic) = diagnostic {
                            diagnostic.fail_io_kind(std::io::ErrorKind::TimedOut);
                        }
                        Err(anyhow::anyhow!(
                            "timed out after {}ms connecting runner {pipe_label}",
                            RUNNER_PIPE_CONNECT_TIMEOUT.as_millis()
                        ))
                    }
                } else {
                    // Do not join the helper thread on the timeout path. Parent-side cleanup will
                    // close the pipe handles, which lets the blocked connect unwind without
                    // risking another indefinite wait here.
                    if let Some(diagnostic) = diagnostic {
                        diagnostic.fail_io_kind(std::io::ErrorKind::TimedOut);
                    }
                    Err(anyhow::anyhow!(
                        "timed out after {}ms connecting runner {pipe_label}",
                        RUNNER_PIPE_CONNECT_TIMEOUT.as_millis()
                    ))
                }
            }
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            if let Some(connect_thread) = connect_thread.take() {
                let _ = connect_thread.join();
            }
            Err(anyhow::anyhow!(
                "runner {pipe_label} connect thread exited before reporting its result"
            ))
        }
    };

    unsafe {
        CloseHandle(thread_handle);
    }

    result
}

pub(crate) fn spawn_runner_transport(
    codex_home: &Path,
    cwd: &Path,
    sandbox_creds: &SandboxCreds,
    log_dir: Option<&Path>,
    spawn_request: SpawnRequest,
    desktop_policy: Option<&DesktopPolicy>,
) -> Result<RunnerTransport> {
    spawn_runner_transport_with_identity(
        codex_home,
        cwd,
        sandbox_creds,
        log_dir,
        spawn_request,
        desktop_policy,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_runner_transport_with_identity(
    codex_home: &Path,
    cwd: &Path,
    sandbox_creds: &SandboxCreds,
    log_dir: Option<&Path>,
    spawn_request: SpawnRequest,
    desktop_policy: Option<&DesktopPolicy>,
    matched_runner: Option<std::sync::Arc<crate::MatchedRunnerArtifact>>,
) -> Result<RunnerTransport> {
    spawn_runner_transport_with_identity_diagnostics(
        codex_home, cwd, sandbox_creds, log_dir, spawn_request, desktop_policy, matched_runner, None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_runner_transport_with_identity_diagnostics(
    codex_home: &Path,
    cwd: &Path,
    sandbox_creds: &SandboxCreds,
    log_dir: Option<&Path>,
    spawn_request: SpawnRequest,
    desktop_policy: Option<&DesktopPolicy>,
    matched_runner: Option<std::sync::Arc<crate::MatchedRunnerArtifact>>,
    diagnostic: Option<&crate::WindowsStartDiagnostic>,
) -> Result<RunnerTransport> {
    let mut local_reservation = match &matched_runner {
        Some(pin) => Some(
            super::checked_owner::reserve_checked_startup_before_launch(
                std::sync::Arc::clone(pin),
                diagnostic.cloned(),
            )
            .inspect_err(|err| {
                if let Some(diagnostic) = diagnostic {
                    diagnostic.fail_io_kind(err.kind());
                }
            })?,
        ),
        None => None,
    };
    spawn_runner_transport_with_checked_reservation(
        codex_home,
        cwd,
        sandbox_creds,
        log_dir,
        spawn_request,
        desktop_policy,
        matched_runner,
        diagnostic,
        local_reservation.as_mut(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_runner_transport_with_checked_reservation(
    codex_home: &Path,
    cwd: &Path,
    sandbox_creds: &SandboxCreds,
    log_dir: Option<&Path>,
    mut spawn_request: SpawnRequest,
    desktop_policy: Option<&DesktopPolicy>,
    matched_runner: Option<std::sync::Arc<crate::MatchedRunnerArtifact>>,
    diagnostic: Option<&crate::WindowsStartDiagnostic>,
    mut checked_reservation: Option<&mut super::checked_owner::CheckedStartupReservation>,
) -> Result<RunnerTransport> {
    if matched_runner.is_some() {
        let Some(res) = checked_reservation.as_deref() else {
            anyhow::bail!("checked runner launch requires a pre-launch startup owner reservation");
        };
        res.owner().ensure_can_advance().inspect_err(|err| {
            if let Some(diagnostic) = diagnostic {
                diagnostic.fail_io_kind(err.kind());
            }
        })?;
    }
    let startup_deadline = checked_reservation
        .as_deref()
        .map_or_else(
            || Instant::now() + RUNNER_SPAWN_READY_TIMEOUT,
            super::checked_owner::CheckedStartupReservation::startup_deadline,
        );

    mark_start(diagnostic, crate::WindowsStartStage::RunnerResolve);
    let runner_exe = find_runner_exe(codex_home, log_dir)
        .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::Unclassified))?;
    if let Some(pin) = &matched_runner {
        mark_start(diagnostic, crate::WindowsStartStage::RunnerPinVerify);
        pin.verify_resolved(&runner_exe)
            .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::PinRejected))?;
    }
    let registered_alias = if registered_core_requested() {
        mark_start(diagnostic, crate::WindowsStartStage::RegisteredAlias);
        Some(crate::app_package::registered_runner_alias(
            codex_home,
            &sandbox_creds.username,
        ).inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::IdentityRejected))?)
    } else {
        None
    };
    // Hold both the private desktop and its UTF-16 startup name through runner startup.
    // The None policy path retains its previous inherited-desktop behavior.
    let native_runner_desktop = if matched_runner.as_ref().is_some_and(|runner|runner.native_close_v8()) {
        anyhow::ensure!(desktop_policy.is_some(),"v8 requires an owned original desktop policy");
        mark_start(diagnostic,crate::WindowsStartStage::PrivateDesktop);
        let owner=checked_reservation.as_deref().context("v8 original startup reservation absent")?.owner();
        let desktop=owner.create_native_desktop(&sandbox_creds.username,log_dir)?;
        spawn_request.private_desktop_name=Some(desktop.lock().map_err(|_|anyhow::anyhow!("original desktop poisoned"))?.name().to_owned());
        Some(desktop)
    }else{None};
    let runner_desktop = if native_runner_desktop.is_some() {None} else if let Some(policy) = desktop_policy {
        mark_start(diagnostic, crate::WindowsStartStage::PrivateDesktop);
        let desktop_name = shared_private_desktop_for_user(
            &sandbox_creds.username,
            policy,
            log_dir,
        ).inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::Unclassified))?;
        let desktop = LaunchDesktop::open_private(&desktop_name)
            .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::Unclassified))?;
        spawn_request.private_desktop_name = Some(desktop_name);
        Some(desktop)
    } else {
        None
    };
    let (pipe_in_name, pipe_out_name) = pipe_pair();
    mark_start(diagnostic, crate::WindowsStartStage::PipeCreateIn);
    let pipe_write = unsafe {
        File::from_raw_handle(create_named_pipe(
            &pipe_in_name,
            PIPE_ACCESS_OUTBOUND,
            &sandbox_creds.username,
        ).inspect_err(|err| {
            if let Some(diagnostic) = diagnostic {
                diagnostic.fail_io_kind(err.kind());
            }
        })? as _)
    };
    mark_start(diagnostic, crate::WindowsStartStage::PipeCreateOut);
    let pipe_read = unsafe {
        File::from_raw_handle(create_named_pipe(
            &pipe_out_name,
            PIPE_ACCESS_INBOUND,
            &sandbox_creds.username,
        ).inspect_err(|err| {
            if let Some(diagnostic) = diagnostic {
                diagnostic.fail_io_kind(err.kind());
            }
        })? as _)
    };
    let h_pipe_in = pipe_write.as_raw_handle() as HANDLE;
    let h_pipe_out = pipe_read.as_raw_handle() as HANDLE;

    let runner_cmdline = registered_alias.as_deref().unwrap_or(&runner_exe).to_str();
    let runner_cmdline = if matched_runner.is_some() {
        runner_cmdline
            .context("matched runner launch path is not UTF-8")
            .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::PinRejected))?
            .to_owned()
    } else {
        runner_cmdline
            .map(str::to_owned)
            .unwrap_or_else(|| "codex-command-runner.exe".to_string())
    };
    let mut runner_full_cmd = format!(
        "{} {} {}",
        quote_windows_arg(&runner_cmdline),
        quote_windows_arg(&format!("--pipe-in={pipe_in_name}")),
        quote_windows_arg(&format!("--pipe-out={pipe_out_name}"))
    );
    if matched_runner.is_some() {
        runner_full_cmd.push_str(if matched_runner.as_ref().is_some_and(|pin|pin.native_close_v8()) {
            " --native-close-v8"
        }else{" --checked-controls-v7"});
    }
    let mut cmdline_vec = to_wide(&runner_full_cmd);
    let exe_w = to_wide(&runner_cmdline);
    let cwd_w = to_wide(cwd);
    let user_w = to_wide(&sandbox_creds.username);
    let domain_w = to_wide(".");
    let password_w = to_wide(&sandbox_creds.password);
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    si.dwFlags = STARTF_FORCEOFFFEEDBACK;
    if let Some(desktop) = &runner_desktop {
        si.lpDesktop = desktop.startup_info_desktop();
    }
    if let Some(desktop)=&native_runner_desktop {
        si.lpDesktop=desktop.lock().map_err(|_|anyhow::anyhow!("original desktop poisoned"))?.startup_info_desktop();
    }
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    let mut nonce = [0u8; 32];
    if matched_runner.is_some() {
        use rand::{RngCore, SeedableRng};
        if matched_runner.as_ref().is_some_and(|pin|pin.native_close_v8()) {
            rand::rngs::OsRng.try_fill_bytes(&mut nonce).map_err(|_|anyhow::anyhow!("v8 startup entropy unavailable"))?;
        }else{rand::rngs::SmallRng::from_entropy().fill_bytes(&mut nonce);}
    }
    let hello = crate::ipc_framed::ControlHello {
        nonce,
        schema: crate::ipc_framed::CHECKED_CONTROL_SCHEMA,
    };
    if let Some(res) = checked_reservation.as_deref_mut() {
        res.stage_pre_launch_context(&pipe_write, &pipe_read, hello.clone())
            .inspect_err(|err| {
                if let Some(diagnostic) = diagnostic {
                    diagnostic.fail_io_kind(err.kind());
                }
            })?;
    }

    let previous_error_mode = unsafe { SetErrorMode(RUNNER_ERROR_MODE_FLAGS) };
    // Execution aliases require the registered account's profile.
    // Other launches retain their existing profile-free behavior.
    mark_start(diagnostic, crate::WindowsStartStage::RunnerLogon);
    let spawn_res = unsafe {
        CreateProcessWithLogonW(
            user_w.as_ptr(),
            domain_w.as_ptr(),
            password_w.as_ptr(),
            if registered_alias.is_some() {
                LOGON_WITH_PROFILE
            } else {
                0
            },
            exe_w.as_ptr(),
            cmdline_vec.as_mut_ptr(),
            windows_sys::Win32::System::Threading::CREATE_NO_WINDOW
                | windows_sys::Win32::System::Threading::CREATE_UNICODE_ENVIRONMENT,
            ptr::null(),
            cwd_w.as_ptr(),
            &si,
            &mut pi,
        )
    };
    let spawn_err = if spawn_res == 0 {
        Some(unsafe { GetLastError() })
    } else {
        None
    };
    unsafe {
        SetErrorMode(previous_error_mode);
    }
    if let Some(err) = spawn_err {
        if let Some(diagnostic) = diagnostic {
            diagnostic.fail_os(crate::WindowsOsError::from_code(err));
        }
        return Err(RunnerLogonError { code: err }.into());
    }
    // Immediately capture and anchor the original process and thread handles in the
    // pre-admitted owner before any logging, cancellation check, or fallible operation.
    let runner_process = unsafe { OwnedHandle::from_raw_handle(pi.hProcess as _) };
    let runner_thread = unsafe { OwnedHandle::from_raw_handle(pi.hThread as _) };
    let expected_runner_pid = pi.dwProcessId;
    let expected_runner_tid = pi.dwThreadId;
    let mut _legacy_runner_process = None;
    let mut _legacy_runner_thread = None;
    let install_result = if let Some(res) = checked_reservation.as_deref_mut() {
        Some(res.install_launched_process(
            runner_process,
            expected_runner_pid,
            runner_thread,
            expected_runner_tid,
            &pipe_write,
            &pipe_read,
            hello.clone(),
        ))
    } else {
        _legacy_runner_process = Some(runner_process);
        _legacy_runner_thread = Some(runner_thread);
        None
    };
    crate::logging::log_note(
        &format!("runner_process_created pid={expected_runner_pid}"),
        log_dir,
    );
    if let (Some(res), Some(diagnostic)) = (checked_reservation.as_deref(), diagnostic) {
        // Publish only an identity already captured by this anchored owner,
        // including when a later installation check reports an error.
        res.owner().observe_startup_runner(diagnostic);
    }
    if let (Some(res), Some(Err(err))) = (checked_reservation.as_deref_mut(), install_result) {
        if let Some(diagnostic) = diagnostic {
            diagnostic.fail_io_kind(err.kind());
        }
        return Err(super::checked_owner::mark_error_non_refreshable(
            err.into(),
            res.startup_id(),
        ));
    }

    mark_start(diagnostic, crate::WindowsStartStage::ControlHello);
    if matched_runner.is_some() && nonce == [0; 32] {
        if let Some(diagnostic) = diagnostic {
            diagnostic.fail(crate::WindowsStartError::ControlRejected);
        }
        let err = anyhow::anyhow!("runner hello nonce unavailable");
        if let Some(res) = checked_reservation.as_deref_mut() {
            return Err(res.fail_after_launch(err));
        }
        return Err(err);
    }
    let mut transport = RunnerTransport {
        pipe_write,
        pipe_read,
        hello,
        controls: crate::ipc_framed::RunnerControlCapabilities::default(),
        matched_runner,
        checked_lease: None,
        startup_deadline,
        native_close_runtime: None,
        native_startup_owner: None,
    };
    let startup_result = (|| -> Result<()> {
        // An update can retarget the alias after setup. Check the selected image
        // and authenticate its pipes before sending it the command.
        if registered_alias.is_some() {
            mark_start(diagnostic, crate::WindowsStartStage::RegisteredImageVerify);
            verify_registered_core_runner(pi.hProcess, &runner_exe)
                .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::IdentityRejected))?;
        }
        mark_start(diagnostic, crate::WindowsStartStage::PipeConnectIn);
        if let Some(res) = checked_reservation.as_deref_mut() {
            res.connect_pipe_step(&transport.pipe_write, "pipe-in", diagnostic)?;
        } else {
            connect_pipe_with_timeout(h_pipe_in, expected_runner_pid, "pipe-in", diagnostic)
                .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::Unclassified))?;
        }
        mark_start(diagnostic, crate::WindowsStartStage::PipeConnectOut);
        if let Some(res) = checked_reservation.as_deref_mut() {
            res.connect_pipe_step(&transport.pipe_read, "pipe-out", diagnostic)?;
        } else {
            connect_pipe_with_timeout(h_pipe_out, expected_runner_pid, "pipe-out", diagnostic)
                .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::Unclassified))?;
        }
        if transport.matched_runner.as_ref().is_some_and(|pin|pin.native_close_v8()) {
            let owner=checked_reservation.as_deref().ok_or_else(||anyhow::anyhow!("v8 original reservation absent"))?.owner();
            let bootstrap=owner.prepare_native_close_runtime(nonce)?;
            let runtime=owner.native_close_runtime().ok_or_else(||anyhow::anyhow!("v8 original binding absent"))?;
            transport.hello=runtime.control_hello();
            // Initial Hello is the explicit private bootstrap, never a v7 Hello.
            owner.write_native_startup(0,move |file| {
                let mut writer=file;
                super::native_close_protocol::write_v8_wire_frame_bounded(&mut writer,&bootstrap)
                    .map(|_|()).map_err(std::io::Error::other)
            })?;
            transport.native_close_runtime=Some(runtime);
            transport.native_startup_owner=Some(std::sync::Arc::clone(owner));
        }
        if transport.matched_runner.is_some() {
            mark_start(diagnostic, crate::WindowsStartStage::ControlHello);
            transport.negotiate_controls(diagnostic)
                .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::ControlRejected))?;
        }
        mark_start(diagnostic, crate::WindowsStartStage::SpawnRequestWrite);
        transport.send_spawn_request(spawn_request)
            .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::Unclassified))?;
        mark_start(diagnostic, crate::WindowsStartStage::SpawnReadyRead);
        transport.read_spawn_ready_with_diagnostics(diagnostic)
            .inspect_err(|err| observe_start_error(diagnostic, err, crate::WindowsStartError::ControlRejected))?;
        Ok(())
    })();
    if let Err(err) = startup_result {
        unsafe {
            // Observe only the original process before cleanup or retention below.
            let wait = WaitForSingleObject(pi.hProcess, 0);
            if wait == WAIT_OBJECT_0 {
                let mut raw_exit = 0;
                if GetExitCodeProcess(pi.hProcess, &mut raw_exit) != 0 {
                    crate::logging::log_note(
                        &format!("runner_pre_cleanup_state=exited pid={expected_runner_pid} raw_exit=0x{raw_exit:08x}"),
                        log_dir,
                    );
                } else {
                    crate::logging::log_note("runner_pre_cleanup_state=unknown reason=exit_query_failed", log_dir);
                }
            } else if wait == 0x0000_0102 { // WAIT_TIMEOUT: not signaled at this observation.
                crate::logging::log_note("runner_pre_cleanup_state=still_running exit=unknown", log_dir);
            } else {
                crate::logging::log_note("runner_pre_cleanup_state=unknown reason=wait_not_signaled", log_dir);
            }
        }
        if let Some(res) = checked_reservation.as_deref_mut() {
            // Checked startup failure retains the original process handle and owner lease
            // without calling TerminateProcess and claiming normal cleanup.
            return Err(res.fail_after_launch(err));
        }
        unsafe {
            let _ = TerminateProcess(pi.hProcess, 1);
        }
        return Err(err);
    }

    if let Some(res) = checked_reservation {
        let lease = res.commit_transport_lease(transport.controls)?;
        transport.checked_lease = Some(lease);
    }

    if registered_alias.is_some() {
        crate::logging::debug_log(
            &format!(
                "registered_core_runner_ready pid={expected_runner_pid} package_verified=true pipes_authenticated=true"
            ),
            log_dir,
        );
    }

    Ok(transport)
}

#[cfg(test)]
mod tests {
    use super::RunnerLogonError;
    use super::RunnerStartupError;
    use super::is_refreshable_sandbox_creds_error;
    use crate::ipc_framed::ErrorPayload;
    use crate::ipc_framed::ErrorStage;
    use pretty_assertions::assert_eq;
    use std::os::windows::io::FromRawHandle;
    use std::time::Instant;
    use windows_sys::Win32::Foundation::ERROR_ACCOUNT_DISABLED;
    use windows_sys::Win32::Foundation::ERROR_LOGON_FAILURE;
    use windows_sys::Win32::Foundation::ERROR_NO_SUCH_LOGON_SESSION;
    use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;

    #[test]
    fn unprovisioned_transport_keeps_legacy_version_and_false_capabilities() {
        let transport = super::RunnerTransport {
            pipe_write: tempfile::tempfile().unwrap(),
            pipe_read: tempfile::tempfile().unwrap(),
            hello: crate::ipc_framed::ControlHello {
                nonce: [0; 32],
                schema: crate::ipc_framed::CHECKED_CONTROL_SCHEMA,
            },
            controls: crate::ipc_framed::RunnerControlCapabilities::default(),
            matched_runner: None,
            checked_lease: None,
            native_close_runtime: None,
        native_startup_owner: None,
            startup_deadline: Instant::now() + super::RUNNER_SPAWN_READY_TIMEOUT,
        };
        assert_eq!(transport.protocol_version(), 6);
        assert!(!transport.controls.close_stdin && !transport.controls.resize);
        assert!(transport.into_checked_files().is_err());
    }

    fn fixture_creds() -> crate::identity::SandboxCreds {
        crate::identity::SandboxCreds {
            username: String::new(),
            password: String::new(),
        }
    }

    #[test]
    fn observed_existing_retry_preserves_first_failure_and_recovery() {
        let diagnostic = crate::WindowsStartDiagnostic::default();
        let mut calls = 0;
        let result = super::retry_runner_spawn_once_with_diagnostics(
            fixture_creds(),
            &[],
            |_| {
                calls += 1;
                diagnostic.mark(crate::WindowsStartStage::RunnerLogon);
                if calls == 1 {
                    let err = anyhow::Error::from(RunnerLogonError { code: ERROR_LOGON_FAILURE });
                    super::observe_start_error(Some(&diagnostic), &err, crate::WindowsStartError::Unclassified);
                    Err(err)
                } else {
                    Ok(17)
                }
            },
            || {
                assert_eq!(diagnostic.snapshot().unwrap().attempt, 2);
                Ok(fixture_creds())
            },
            Some(&diagnostic),
        );
        assert_eq!(result.unwrap(), 17);
        assert_eq!(calls, 2);
        let snapshot = diagnostic.snapshot().unwrap();
        assert_eq!(snapshot.first_attempt.unwrap().os_class, crate::WindowsOsError::LogonFailure);
        assert_eq!(snapshot.current.error, crate::WindowsStartError::NotObserved);
        assert_eq!(snapshot.completion, crate::WindowsStartCompletion::InProgressOrUnwound);
    }

    #[test]
    fn observed_existing_retry_stops_on_refresh_failure() {
        let diagnostic = crate::WindowsStartDiagnostic::default();
        let mut calls = 0;
        let result: anyhow::Result<()> = super::retry_runner_spawn_once_with_diagnostics(
            fixture_creds(),
            &[],
            |_| {
                calls += 1;
                diagnostic.mark(crate::WindowsStartStage::RunnerLogon);
                let err = anyhow::Error::from(RunnerLogonError { code: ERROR_LOGON_FAILURE });
                super::observe_start_error(Some(&diagnostic), &err, crate::WindowsStartError::Unclassified);
                Err(err)
            },
            || {
                let err = anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
                super::observe_start_error(Some(&diagnostic), &err, crate::WindowsStartError::Unclassified);
                Err(err)
            },
            Some(&diagnostic),
        );
        assert!(result.unwrap_err().downcast_ref::<std::io::Error>().is_some());
        assert_eq!(calls, 1);
        let snapshot = diagnostic.snapshot().unwrap();
        assert_eq!(snapshot.current.stage, crate::WindowsStartStage::SetupRefresh);
        assert_eq!(snapshot.current.error, crate::WindowsStartError::IoAccessDenied);
        assert_eq!(snapshot.first_attempt.unwrap().error, crate::WindowsStartError::OsReported);
    }

    #[test]
    fn observed_runner_error_does_not_read_message_or_retry_non_spawn_stage() {
        let diagnostic = crate::WindowsStartDiagnostic::default();
        let result: anyhow::Result<()> = super::retry_runner_spawn_once_with_diagnostics(
            fixture_creds(),
            &[],
            |_| {
                diagnostic.mark(crate::WindowsStartStage::SpawnReadyRead);
                let err = anyhow::Error::from(RunnerStartupError::new(ErrorPayload {
                    message: "fixture text must never become diagnostic data".to_owned(),
                    stage: ErrorStage::WriteSpawnReady,
                    windows_error_code: Some(ERROR_NO_SUCH_LOGON_SESSION),
                }));
                super::observe_start_error(Some(&diagnostic), &err, crate::WindowsStartError::Unclassified);
                Err(err)
            },
            || panic!("non-spawn runner failure must not refresh"),
            Some(&diagnostic),
        );
        assert!(result.unwrap_err().downcast_ref::<RunnerStartupError>().is_some());
        let snapshot = diagnostic.snapshot().unwrap();
        assert_eq!(snapshot.attempt, 1);
        assert_eq!(snapshot.first_attempt, None);
        assert_eq!(snapshot.current.runner_stage, crate::WindowsRunnerErrorStage::WriteSpawnReady);
        assert_eq!(snapshot.current.os_class, crate::WindowsOsError::NoLogonSession);
    }

    #[test]
    fn refreshable_sandbox_creds_error_recognizes_credential_and_child_start_failures() {
        assert_eq!(
            [
                ERROR_ACCOUNT_DISABLED,
                ERROR_LOGON_FAILURE,
                ERROR_NO_SUCH_LOGON_SESSION,
                ERROR_NOT_FOUND,
            ]
            .map(|code| {
                let err =
                    anyhow::Error::new(RunnerLogonError { code }).context("runner launch failed");
                is_refreshable_sandbox_creds_error(&err, &[])
            }),
            [true, true, true, false]
        );

        assert_eq!(
            [
                (ErrorStage::SpawnChild, ERROR_NO_SUCH_LOGON_SESSION),
                (ErrorStage::SpawnChild, ERROR_NOT_FOUND),
                (ErrorStage::ReadSpawnRequest, ERROR_NO_SUCH_LOGON_SESSION),
            ]
            .map(|(stage, windows_error_code)| {
                let err = anyhow::Error::new(RunnerStartupError::new(ErrorPayload {
                    message: "runner startup failed".to_string(),
                    stage,
                    windows_error_code: Some(windows_error_code),
                }));
                is_refreshable_sandbox_creds_error(&err, &["cmd.exe".to_string()])
            }),
            [true, false, false]
        );

        let windows_apps_commands = [
            vec![
                r"C:\Users\user\AppData\Local\Microsoft\WindowsApps\pwsh.exe".to_string(),
            ],
            vec![
                r"C:\Program Files\WindowsApps\Microsoft.PowerShell_7.6.3.0_x64__8wekyb3d8bbwe\pwsh.exe"
                    .to_string(),
            ],
        ];
        assert_eq!(
            windows_apps_commands.map(|command| {
                let err = anyhow::Error::new(RunnerStartupError::new(ErrorPayload {
                    message: "runner startup failed".to_string(),
                    stage: ErrorStage::SpawnChild,
                    windows_error_code: Some(ERROR_NO_SUCH_LOGON_SESSION),
                }));
                is_refreshable_sandbox_creds_error(&err, &command)
            }),
            [false, false]
        );
    }

    #[test]
    fn checked_unknown_or_post_launch_error_is_non_refreshable_and_preserves_primary_error() {
        let primary = anyhow::Error::new(RunnerStartupError::new(ErrorPayload {
            message: "spawn child failed after checked launch".to_owned(),
            stage: ErrorStage::SpawnChild,
            windows_error_code: Some(ERROR_NO_SUCH_LOGON_SESSION),
        }));
        assert!(is_refreshable_sandbox_creds_error(
            &primary,
            &["cmd.exe".to_owned()]
        ));

        let non_refreshable = super::super::checked_owner::mark_error_non_refreshable(
            primary,
            super::super::checked_owner::StartupId(42),
        );
        assert!(
            !is_refreshable_sandbox_creds_error(&non_refreshable, &["cmd.exe".to_owned()]),
            "checked post-launch or Unknown startup errors must never trigger credential refresh"
        );
        assert!(
            non_refreshable
                .downcast_ref::<RunnerStartupError>()
                .is_some(),
            "primary RunnerStartupError must remain downcastable for diagnostics"
        );
    }

    #[test]
    fn checked_transport_rejects_legacy_into_files_without_dropping_owner() {
        static REGISTRY: super::super::checked_owner::CheckedOwnerRegistry =
            super::super::checked_owner::CheckedOwnerRegistry::new();
        static CONNECT_OWNER: super::super::owned_connect_worker::Owner =
            super::super::owned_connect_worker::Owner::new();

        use sha2::Digest;
        let exe = std::env::current_exe().expect("current_exe");
        let sha: [u8; 32] = sha2::Sha256::digest(std::fs::read(&exe).expect("read exe")).into();
        let artifact = std::sync::Arc::new(
            crate::MatchedRunnerArtifact::acquire(&exe, sha).expect("acquire matched artifact"),
        );
        let mut res = super::super::checked_owner::reserve_in_registry(
            &REGISTRY,
            Some(&CONNECT_OWNER),
            std::sync::Arc::clone(&artifact),
            None,
            std::time::Duration::from_secs(5),
        )
        .expect("reserve startup");
        let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned());
        let mut child = std::process::Command::new(comspec)
            .args(["/C", "exit", "0"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn child");
        let pid = child.id();
        let _ = child.wait().expect("wait child");
        let proc_handle: std::os::windows::io::OwnedHandle = child.into();
        let mut raw_thread: windows_sys::Win32::Foundation::HANDLE = 0;
        let current_proc = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
        let current_thread = unsafe { windows_sys::Win32::System::Threading::GetCurrentThread() };
        let dup_ok = unsafe {
            windows_sys::Win32::Foundation::DuplicateHandle(
                current_proc,
                current_thread,
                current_proc,
                &mut raw_thread,
                0,
                0,
                windows_sys::Win32::Foundation::DUPLICATE_SAME_ACCESS,
            )
        };
        assert_ne!(dup_ok, 0);
        let thread_handle =
            unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(raw_thread as _) };
        let pipe_w = tempfile::tempfile().expect("pipe_w");
        let pipe_r = tempfile::tempfile().expect("pipe_r");
        let hello = crate::ipc_framed::ControlHello {
            nonce: [7; 32],
            schema: crate::ipc_framed::CHECKED_CONTROL_SCHEMA,
        };
        res.install_launched_process(proc_handle, pid, thread_handle, 9, &pipe_w, &pipe_r, hello.clone())
            .expect("install process");
        res.connect_pipe_step_with(&pipe_w, "pipe-in", None, |_| Ok(()))
            .expect("connect pipe-in");
        res.connect_pipe_step_with(&pipe_r, "pipe-out", None, |_| Ok(()))
            .expect("connect pipe-out");
        let lease = res
            .commit_transport_lease(crate::ipc_framed::RunnerControlCapabilities {
                close_stdin: true,
                resize: true,
            })
            .expect("commit transport lease");
        let owner = std::sync::Arc::clone(lease.owner());
        let transport = super::RunnerTransport {
            pipe_write: pipe_w,
            pipe_read: pipe_r,
            hello,
            controls: crate::ipc_framed::RunnerControlCapabilities {
                close_stdin: true,
                resize: true,
            },
            matched_runner: Some(artifact),
            checked_lease: Some(lease),
            startup_deadline: res.startup_deadline(),
            native_close_runtime: None,
            native_startup_owner: None,
        };

        assert!(
            transport.into_files().is_err(),
            "checked transport must reject legacy into_files()"
        );
        assert!(owner.is_no_replay_sealed());
        assert_eq!(
            owner.sticky_unknown_reason(),
            Some(super::super::checked_owner::StickyUnknownReason::WrongLegacyConversion)
        );
        assert_eq!(REGISTRY.active_count().expect("retained in registry"), 1);

        owner
            .finalize(Instant::now() + std::time::Duration::from_secs(1))
            .expect("reap retained owner");
        assert_eq!(REGISTRY.active_count().expect("reaped from registry"), 0);
    }
}

