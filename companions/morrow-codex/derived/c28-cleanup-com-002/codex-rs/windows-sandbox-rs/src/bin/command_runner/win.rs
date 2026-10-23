//! Windows command runner used by the **elevated** sandbox path.
//!
//! The CLI launches this binary under the sandbox user when Windows sandbox level is
//! Elevated. It verifies the control hello before reading `SpawnRequest`, derives a
//! restricted token from the sandbox user, and spawns the child process via ConPTY
//! (`tty=true`) or pipes (`tty=false`). It then streams output frames back to the parent,
//! accepts stdin/terminate frames, and emits a final exit frame. The legacy restricted鈥憈oken
//! path spawns the child directly and does not use this runner.

#![allow(unsafe_op_in_unsafe_fn)]

mod checked_lifecycle;
mod cwd_junction;
#[cfg(test)]
#[path = "win/checked_lifecycle_tests.rs"]
mod checked_lifecycle_tests;
#[cfg(test)]
#[path = "win/input_loop_tests.rs"]
mod input_loop_tests;

use anyhow::Context;
use anyhow::Result;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::ChildCreationFactState;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::ChildExitFactState;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::ChildNativeFactsSnapshot;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::ChildStdinFactSnapshot;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::ControlAndWriterFactSnapshot;
use checked_lifecycle::DEFAULT_SHUTDOWN_BUDGET;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::ExtraJobMemberOriginFact;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::FinalReceiptWriterFactState;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::FirstErrorFactSnapshot;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::HostOwnedResourceScopeFact;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::InputWorkerFactSnapshot;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::JobActiveZeroFactState;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::JobFactSnapshot;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::JobPreserveDescendantsFactState;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::LockObservationState;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::NativeHandleCloseFact;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::OutputStreamFactSnapshot;
use checked_lifecycle::OwnedWinHandle;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::OwnerChildNativeFactsSnapshot;
use checked_lifecycle::RetainedRunnerContext;
use checked_lifecycle::RetainedWorker;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::StdinCloseDebtObservation;
use checked_lifecycle::StdinHandleOwner;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::StdinOwnershipObservationState;
use checked_lifecycle::StickyUnknownKind;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::VerifiedChildExitFact;
use checked_lifecycle::acquire_mutex_bounded;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::evaluate_child_exit_observation_scalar;
pub(crate) use checked_lifecycle::has_unresolved_attempt_slot_entries;
pub(crate) use checked_lifecycle::hold_until_native_process_exit_allowed;
pub(crate) use checked_lifecycle::mark_attempt_slot_unwound;
#[cfg(test)]
use checked_lifecycle::park_timed_out_output_reader;
use checked_lifecycle::park_unresolved_attempt_context;
use checked_lifecycle::read_input_frame_checked;
use checked_lifecycle::reserve_attempt_context_slot;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::snapshot_attempt_slot_child_native_facts;
#[allow(unused_imports)]
pub(crate) use checked_lifecycle::try_snapshot_attempt_slot_child_native_facts;
use codex_windows_sandbox::CHECKED_IPC_PROTOCOL_VERSION;
use codex_windows_sandbox::ConsoleMode;
use codex_windows_sandbox::ControlHelloAck;
use codex_windows_sandbox::ErrorPayload;
use codex_windows_sandbox::ErrorStage;
use codex_windows_sandbox::ExitPayload;
use codex_windows_sandbox::FramedMessage;
use codex_windows_sandbox::IPC_PROTOCOL_VERSION;
use codex_windows_sandbox::LaunchDesktop;
use codex_windows_sandbox::LocalSid;
use codex_windows_sandbox::Message;
use codex_windows_sandbox::OutputPayload;
use codex_windows_sandbox::OutputStream;
use codex_windows_sandbox::PipeSpawnHandles;
use codex_windows_sandbox::RunnerControlCapabilities;
use codex_windows_sandbox::SpawnReady;
use codex_windows_sandbox::SpawnRequest;
use codex_windows_sandbox::StderrMode;
use codex_windows_sandbox::StdinMode;
use codex_windows_sandbox::WindowsSandboxTokenMode;
use codex_windows_sandbox::RunnerStartupSite;
use codex_windows_sandbox::allow_null_device;
use codex_windows_sandbox::{derive_uninitialized_restriction_with_caps_and_user_from, initialize_owned_restricted_token};
use codex_windows_sandbox::encode_bytes;
use codex_windows_sandbox::{open_current_token_for_query_duplicate, duplicate_token_for_restriction};
use codex_windows_sandbox::hide_current_user_profile_dir;
use codex_windows_sandbox::log_note;
use codex_windows_sandbox::read_frame;
use codex_windows_sandbox::spawn_process_with_pipes;
use codex_windows_sandbox::to_wide;
use codex_windows_sandbox::token_mode_for_permission_profile;
use codex_windows_sandbox::write_frame;
use std::ffi::OsStr;
use std::fs::File;
use std::os::windows::io::FromRawHandle;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::CreateFileW;
use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ;
use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE;
use windows_sys::Win32::Storage::FileSystem::OPEN_EXISTING;
use windows_sys::Win32::System::Threading::INFINITE;
use windows_sys::Win32::System::Threading::MUTEX_ALL_ACCESS;
use windows_sys::Win32::System::Threading::OpenMutexW;
use windows_sys::Win32::System::Threading::WaitForSingleObject;

const READ_ACL_MUTEX_NAME: &str = "Local\\CodexSandboxReadAcl";
const TERMINATION_WAIT_MS: u32 = 5_000;
const WAIT_TIMEOUT: u32 = 0x0000_0102;

/// Open a named pipe created by the parent process.
fn open_pipe(name: &str, access: u32) -> Result<HANDLE> {
    let path = to_wide(name);
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            0,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            0,
        )
    };
    if handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
        let err = unsafe { GetLastError() };
        return Err(anyhow::anyhow!("CreateFileW failed for pipe {name}: {err}"));
    }
    Ok(handle)
}

/// Send an error frame back to the parent process using bounded mutex acquisition
/// so an error path never blocks unboundedly if another writer holds `pipe_write`.
fn send_error(
    context: Option<&mut RetainedRunnerContext>,
    writer: &Arc<StdMutex<File>>,
    stage: ErrorStage,
    windows_error_code: Option<u32>,
    message: String,
    protocol_version: u8,
) -> Result<()> {
    if protocol_version==8 {
        anyhow::bail!("v8 error requires the original authenticated retained runtime writer; no legacy fallback");
    }
    let msg = FramedMessage {
        version: protocol_version,
        message: Message::Error {
            payload: ErrorPayload {
                message,
                stage,
                windows_error_code,
            },
        },
    };
    if protocol_version == CHECKED_IPC_PROTOCOL_VERSION {
        let Some(context) = context else {
            return checked_lifecycle::send_precontext_error_frame_checked(writer, msg,
                std::time::Instant::now() + DEFAULT_SHUTDOWN_BUDGET).map_err(Into::into);
        };
        let deadline = *context.error_write_deadline.get_or_insert_with(||
            std::time::Instant::now() + DEFAULT_SHUTDOWN_BUDGET);
        return context.send_frame_checked(writer, msg, deadline).map_err(Into::into);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    if let Ok(mut guard) = acquire_mutex_bounded(writer, deadline, "runner error frame writer") {
        write_frame(&mut *guard, &msg)?;
    }
    Ok(())
}

fn send_native_precontext_error<W: std::io::Write+Send+'static>(
    writer:&Arc<StdMutex<W>>,runtime:Option<&Arc<codex_windows_sandbox::NativeCloseRuntime>>,
    stage:ErrorStage,windows_error_code:Option<u32>,deadline:std::time::Instant,
)->Result<()> {
    let runtime=runtime.ok_or_else(||anyhow::anyhow!("no authenticated original v8 runtime"))?;
    anyhow::ensure!(!runtime.is_sealed() && !has_unresolved_attempt_slot_entries(),
        "v8 error writer unavailable; original Unknown/post-create owner retained");
    let runtime=Arc::clone(runtime);
    let frame=FramedMessage {version:8,message:Message::Error {payload:ErrorPayload {
        message:"v8 startup rejected; original owner retained".into(),stage,windows_error_code,
    }}};
    checked_lifecycle::send_native_precontext_frame_with(writer,deadline,
        move |writer|runtime.write_message(writer,&frame)).map_err(Into::into)
}

fn send_native_retained_startup_error(
    writer:&Arc<StdMutex<File>>,runtime:Option<&Arc<codex_windows_sandbox::NativeCloseRuntime>>,
    windows_error_code:Option<u32>,site:RunnerStartupSite,deadline:std::time::Instant,
)->Result<()> {
    let runtime=runtime.ok_or_else(||anyhow::anyhow!("no authenticated original v8 runtime"))?;
    anyhow::ensure!(!runtime.is_sealed(),"original v8 error channel sealed; no replay");
    let original_runtime=Arc::clone(runtime);
    let frame=FramedMessage {version:8,message:Message::Error {payload:ErrorPayload {
        message:site.wire_message().to_owned(),
        stage:ErrorStage::SpawnChild,windows_error_code,
    }}};
    checked_lifecycle::send_native_startup_failure_frame_with(writer,runtime,deadline,
        move |writer|original_runtime.write_message(writer,&frame)).map_err(Into::into)
}

fn windows_error_code(err: &anyhow::Error) -> Option<u32> {
    err.chain().find_map(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error)
            .and_then(|code| u32::try_from(code).ok())
    })
}

/// Read and validate the initial spawn request frame.
fn read_spawn_request(reader: &mut File, protocol_version: u8) -> Result<SpawnRequest> {
    let msg = if protocol_version == CHECKED_IPC_PROTOCOL_VERSION {
        read_input_frame_checked(reader)
            .map_err(|err| anyhow::anyhow!(err.into_sticky_error().detail))?
    } else {
        read_frame(reader)?
    };
    let Some(msg) = msg else {
        anyhow::bail!("runner: pipe closed before spawn_request");
    };
    if msg.version != protocol_version {
        anyhow::bail!("runner: unsupported protocol version {}", msg.version);
    }
    match msg.message {
        Message::SpawnRequest { payload } => Ok(*payload),
        other => anyhow::bail!("runner: expected spawn_request, got {other:?}"),
    }
}

fn read_acl_mutex_exists() -> Result<bool> {
    let name = to_wide(OsStr::new(READ_ACL_MUTEX_NAME));
    let handle = unsafe { OpenMutexW(MUTEX_ALL_ACCESS, 0, name.as_ptr()) };
    if handle == 0 {
        let err = unsafe { GetLastError() };
        if err == ERROR_FILE_NOT_FOUND {
            return Ok(false);
        }
        return Err(anyhow::anyhow!("OpenMutexW failed: {err}"));
    }
    unsafe {
        CloseHandle(handle);
    }
    Ok(true)
}

/// Pick an effective CWD, using a junction if the ACL helper is active.
fn effective_cwd(req_cwd: &Path, log_dir: Option<&Path>) -> PathBuf {
    let use_junction = match read_acl_mutex_exists() {
        Ok(exists) => exists,
        Err(err) => {
            log_note(
                &format!(
                    "junction: failed to probe ACL mutex state: {err}; defaulting to junction cwd"
                ),
                log_dir,
            );
            true
        }
    };
    if use_junction {
        cwd_junction::create_cwd_junction(req_cwd, log_dir).unwrap_or_else(|| req_cwd.to_path_buf())
    } else {
        req_cwd.to_path_buf()
    }
}

fn close_preparation_after_spawn(mut context: RetainedRunnerContext) -> Result<RetainedRunnerContext> {
    // On any close failure the context Drop parks the SAME admitted originals.
    context.close_preparation_token_after_spawn()
        .map_err(|error| RunnerStartupSite::CloseRestrictedToken.context(error.into()))?;
    Ok(context)
}

fn spawn_ipc_process(req: &SpawnRequest, checked_controls: bool) -> Result<RetainedRunnerContext> {
    // Hold a real bounded pre-effect reservation guard and validate request preconditions
    // before creating any native token, ACL, junction, desktop, Job, or child process effects.
    let mut reservation =
        reserve_attempt_context_slot().context(RunnerStartupSite::ReserveAttempt)?;
    if req.command.is_empty() {
        anyhow::bail!("runner: empty command in spawn_request");
    }
    if req.cap_sids.is_empty() {
        anyhow::bail!("runner: empty capability SID list");
    }
    let private_desktop_name = req
        .private_desktop_name
        .as_deref()
        .context("runner: missing parent-owned private desktop")?;
    let token_mode = token_mode_for_permission_profile(
        &req.permission_profile,
        &req.workspace_roots,
        &req.cwd,
        &req.env,
    )
    .context(RunnerStartupSite::ResolveTokenMode)?;
    let mut cap_psids: Vec<LocalSid> = Vec::with_capacity(req.cap_sids.len());
    for sid in &req.cap_sids {
        cap_psids.push(
            LocalSid::from_string(sid)
                .context(RunnerStartupSite::ParseCapabilitySid)?,
        );
    }
    let network_proxy_restricting_sid = req
        .network_proxy_restricting_sid
        .as_deref()
        .map(LocalSid::from_string)
        .transpose()
        .context(RunnerStartupSite::ParseProxySid)?;

    let fallback_job = Arc::new(
        codex_utils_pty::JobObject::create()
            .context(RunnerStartupSite::CreateFallbackJob)?,
    );

    let log_dir = req.codex_home.clone();
    hide_current_user_profile_dir(req.codex_home.as_path());

    // The token helpers still take raw SID pointers, but we keep ownership in `LocalSid`
    // wrappers for as long as possible. That way any failure after SID parsing but before the
    // child is fully spawned still releases the backing LocalAlloc memory automatically.
    let cap_psid_ptrs: Vec<*mut _> = cap_psids.iter().map(LocalSid::as_ptr).collect();
    let additional_restricting_sid_ptrs: Vec<*mut _> = network_proxy_restricting_sid
        .iter()
        .map(LocalSid::as_ptr)
        .collect();
    let original = reservation.create_preparation_token(0, || unsafe {
        open_current_token_for_query_duplicate()
    }).map_err(|error| RunnerStartupSite::OpenOriginalToken.context(error))?;
    // Windows must see a restriction of this process's actual primary token.
    // Capture the restricted original before duplicating it for initialization;
    // every handle remains with this same attempt owner on any failed step.
    let restricted = reservation.create_preparation_token(1, || unsafe {
        match token_mode {
            WindowsSandboxTokenMode::ReadOnlyCapability
            | WindowsSandboxTokenMode::WritableRootsCapability => {
                derive_uninitialized_restriction_with_caps_and_user_from(
                    original, &cap_psid_ptrs, &additional_restricting_sid_ptrs,
                )
            }
        }
    }).map_err(|error| RunnerStartupSite::CreateCapabilityToken.context(error))?;
    let h_token = reservation.create_preparation_token(2, || unsafe {
        duplicate_token_for_restriction(restricted)
    }).map_err(|error| RunnerStartupSite::DuplicateRestrictionToken.context(error))?;
    unsafe { initialize_owned_restricted_token(h_token) }
        .map_err(|error| RunnerStartupSite::CreateCapabilityToken.context(error))?;
    reservation.close_preparation_token(0).context(RunnerStartupSite::CloseOriginalQueryToken)?;
    reservation.close_preparation_token(1).context(RunnerStartupSite::CloseIntermediateToken)?;
    unsafe {
        // These ACL adjustments need the raw SID values, but ownership stays with `cap_psids`.
        // We do not manually `LocalFree` anything here; the wrappers handle every return path.
        allow_null_device(cap_psid_ptrs[0]);
        for psid in &cap_psid_ptrs {
            allow_null_device(*psid);
        }
    }

    let effective_cwd = effective_cwd(&req.cwd, Some(log_dir.as_path()));
    let desktop = LaunchDesktop::open_private(private_desktop_name)
        .map_err(|error| RunnerStartupSite::OpenPrivateDesktop.context(error))?;

    // Mark entry into the native launch boundary so any error or unwind inside or after
    // the lower-level spawn call latches `ParkedUnresolved` (`no_replay=true`).
    reservation.mark_post_launch_admitted();

    if req.tty {
        if checked_controls {
            return codex_windows_sandbox::spawn_conpty_process_as_user_retaining(
                h_token, &req.command, &effective_cwd, &req.env, desktop,
                |pi, mut conpty, creation_evidence, first_error| {
                    let input = conpty.take_input_write();
                    let output = conpty.take_output_read();
                    let job = conpty.job();
                    let mut context = RetainedRunnerContext::install_raw_after_launch(Some(reservation),
                        log_dir, pi, job, fallback_job, output, INVALID_HANDLE_VALUE,
                        Some(input), Some(conpty), None);
                    context.capture_creation_evidence(pi, creation_evidence);
                    if let Some(error) = first_error {
                        context.sticky_latch.record_first(StickyUnknownKind::SpawnChildFailed, error.to_string());
                        park_unresolved_attempt_context(context);
                        return Err(error);
                    }
                    context.validate_captured_originals(pi, req.stdin_open).map_err(anyhow::Error::from)
                },
            ).map_err(|error| RunnerStartupSite::SpawnConptyChild.context(error))?
                .context(RunnerStartupSite::AdmitChildContext).and_then(close_preparation_after_spawn);
        }
        let (pi, mut conpty) = codex_windows_sandbox::spawn_conpty_process_as_user(
            h_token,
            &req.command,
            &effective_cwd,
            &req.env,
            desktop,
        )?;
        let raw_input_write = conpty.take_input_write();
        let raw_output_read = conpty.take_output_read();
        let job_opt = conpty.job();
        reservation
            .admit_post_launch_originals(
                log_dir,
                pi,
                job_opt,
                fallback_job,
                raw_output_read,
                INVALID_HANDLE_VALUE,
                Some(raw_input_write),
                req.stdin_open,
                Some(conpty),
                None,
            )
            .context(RunnerStartupSite::AdmitChildContext).and_then(close_preparation_after_spawn)
    } else {
        let stdin_mode = if req.stdin_open {
            StdinMode::Open
        } else {
            StdinMode::Closed
        };
        let spawn_pipes = if checked_controls {
            codex_windows_sandbox::spawn_process_with_pipes_retaining
        } else { spawn_process_with_pipes };
        let spawned_pipes: PipeSpawnHandles = spawn_pipes(
            h_token,
            &req.command,
            &effective_cwd,
            &req.env,
            stdin_mode,
            StderrMode::Separate,
            ConsoleMode::NoWindow,
            desktop,
            Some(log_dir.as_path()),
        ).map_err(|error| RunnerStartupSite::SpawnStdioChild.context(error))?;
        let pi = spawned_pipes.process;
        let stdout_handle = spawned_pipes.stdout_read;
        let stderr_handle = spawned_pipes.stderr_read.unwrap_or(INVALID_HANDLE_VALUE);
        let stdin_handle = spawned_pipes.stdin_write;
        let job = spawned_pipes.job();
        reservation
            .admit_post_launch_originals(
                log_dir,
                pi,
                Some(job),
                fallback_job,
                stdout_handle,
                stderr_handle,
                stdin_handle,
                req.stdin_open,
                None,
                Some(spawned_pipes),
            )
            .context(RunnerStartupSite::AdmitChildContext).and_then(close_preparation_after_spawn)
    }
}

/// Stream stdout/stderr from the child into Output frames.
pub(crate) fn spawn_output_reader(
    writer: Arc<StdMutex<File>>,
    handle: HANDLE,
    stream: OutputStream,
    _log_dir: Option<PathBuf>,
    protocol_version: u8,
) -> std::thread::JoinHandle<std::io::Result<()>> {
    spawn_output_reader_with_runtime(writer,handle,stream,_log_dir,protocol_version,None)
}

pub(crate) fn spawn_output_reader_with_runtime(
    writer: Arc<StdMutex<File>>,
    handle: HANDLE,
    stream: OutputStream,
    _log_dir: Option<PathBuf>,
    protocol_version: u8,
    runtime: Option<Arc<codex_windows_sandbox::NativeCloseRuntime>>,
) -> std::thread::JoinHandle<std::io::Result<()>> {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            let mut count = 0;
            let ok = unsafe { windows_sys::Win32::Storage::FileSystem::ReadFile(
                handle, buffer.as_mut_ptr(), buffer.len() as u32, &mut count, std::ptr::null_mut()) };
            if ok == 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE as i32) { return Ok(()); }
                return Err(err);
            }
            if count == 0 { return Ok(()); }
            let chunk = &buffer[..count as usize];
        let msg = FramedMessage {
            version: protocol_version,
            message: Message::Output {
                payload: OutputPayload {
                    data_b64: encode_bytes(chunk),
                    stream,
                },
            },
        };
        let mut guard = match writer.lock() {
            Ok(guard) => guard,
            Err(_) if protocol_version == IPC_PROTOCOL_VERSION => continue,
            Err(_) => return Err(std::io::Error::other("runner output writer poisoned")),
        };
        let forwarded = match codex_windows_sandbox::write_runtime_message(&mut *guard, &msg, runtime.as_deref()) {
            Ok(()) => Ok(()),
            Err(err) if protocol_version == IPC_PROTOCOL_VERSION => {
                log_note(
                    &format!("runner output write failed: {err}"),
                    _log_dir.as_deref(),
                );
                Ok(())
            }
            Err(err) => Err(std::io::Error::other(format!(
                "runner output forwarding failed: {err}"
            ))),
        };
        forwarded?;
        }
    })
}

// Waiting for the process and observing pipe EOF are separate witnesses.
// Descendants may still hold a pipe open, so reader joins must be bounded on the
// native OS thread HANDLE and must never drop or detach a still-running JoinHandle.
#[cfg(test)]
fn join_output_reader(
    admission: checked_lifecycle::OutputReaderAdmission,
    thread: std::thread::JoinHandle<std::io::Result<()>>,
    deadline: std::time::Instant,
) -> std::io::Result<()> {
    let mut worker = RetainedWorker::new("runner output reader", thread);
    match worker.join_bounded(deadline) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(std::io::Error::new(err.kind(), err.to_string())),
        Err(err) => {
            if worker.has_retained_join_handle() {
                let _ = park_timed_out_output_reader(admission, worker);
            }
            Err(err)
        }
    }
}

fn verified_exit_code(wait: u32, succeeded: bool, code: u32) -> std::io::Result<i32> {
    if wait != 0 {
        return Err(std::io::Error::other(
            "runner process exit was not witnessed",
        ));
    }
    if !succeeded {
        return Err(std::io::Error::other(
            "runner actual exit code was not observed",
        ));
    }
    Ok(code as i32)
}

#[cfg(test)]
#[path = "terminal_witness_tests.rs"]
mod terminal_witness_tests;

/// Read stdin/terminate frames and forward to the child process (test helper for legacy v6).
#[cfg(test)]
fn spawn_input_loop(
    reader: File,
    stdin_handle: Option<HANDLE>,
    hpc_handle: Arc<StdMutex<Option<HANDLE>>>,
    job: Arc<codex_utils_pty::JobObject>,
    process: HANDLE,
    log_dir: Option<PathBuf>,
) -> std::thread::JoinHandle<()> {
    let duplicated_process = if process != 0 && process != INVALID_HANDLE_VALUE {
        let borrowed = OwnedWinHandle::new(process);
        let dup = borrowed.duplicate_same_access("input loop child process");
        let _ = borrowed.into_raw();
        dup.unwrap_or_else(|_| OwnedWinHandle::new(0))
    } else {
        OwnedWinHandle::new(0)
    };
    let pid = if duplicated_process.is_open() {
        unsafe { windows_sys::Win32::System::Threading::GetProcessId(duplicated_process.raw()) }
    } else {
        0
    };
    let identity = checked_lifecycle::ChildProcessIdentity {
        process_id: pid,
        thread_id: 0,
        creation_time_100ns: 1,
    };
    let process_owner = Arc::new(checked_lifecycle::SharedChildProcessOwner::new(
        duplicated_process,
        identity,
    ));
    let stdin_guard = match stdin_handle {
        Some(h) if h != 0 && h != INVALID_HANDLE_VALUE => Some(OwnedWinHandle::new(h)),
        _ => None,
    };
    let stdin_owner = StdinHandleOwner::new(stdin_guard);
    let conpty_resize = checked_lifecycle::ConptyResizeOwner::from_raw_mutex(hpc_handle);
    let sticky_latch = checked_lifecycle::StickyUnknownLatch::new();
    std::thread::spawn(move || {
        let _ = checked_lifecycle::run_managed_input_loop(
            reader,
            stdin_owner,
            conpty_resize,
            job,
            process_owner,
            log_dir,
            None,
            [0; 32],
            IPC_PROTOCOL_VERSION,
            sticky_latch,
        );
    })
}

/// Entry point for the Windows command runner process.
///
/// Returns the verified child exit status on normal completion so the binary
/// wrapper can propagate nonzero exit statuses without calling `process::exit`.
/// If any post-launch original resource or worker remains unresolved on error
/// or unwind, the runner stays alive in-process and reaps the SAME originals
/// while keeping `Unknown` / `no_replay` sticky rather than falling through to
/// OS process exit while originals are active.
pub fn main() -> Result<i32> {
    let run_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run_main_inner));
    match run_result {
        Ok(Ok(exit_code)) => {
            if has_unresolved_attempt_slot_entries() {
                let _ = hold_until_native_process_exit_allowed();
                anyhow::bail!(
                    "runner completed with unresolved original resources parked in attempt slot; Unknown/no_replay sticky"
                );
            }
            Ok(exit_code)
        }
        Ok(Err(err)) => {
            if has_unresolved_attempt_slot_entries() {
                let _ = hold_until_native_process_exit_allowed();
            }
            Err(err)
        }
        Err(panic_payload) => {
            mark_attempt_slot_unwound();
            if has_unresolved_attempt_slot_entries() {
                let _ = hold_until_native_process_exit_allowed();
            }
            std::panic::resume_unwind(panic_payload);
        }
    }
}

fn run_main_inner() -> Result<i32> {
    let native_startup_deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
    let mut pipe_in = None;
    let mut pipe_out = None;
    let mut checked_controls = false;
    let mut native_close_v8 = false;
    for arg in std::env::args().skip(1) {
        if let Some(rest) = arg.strip_prefix("--pipe-in=") {
            pipe_in = Some(rest.to_string());
        } else if arg == "--native-close-v8" {
            anyhow::ensure!(!native_close_v8,"duplicate native-close opt-in");
            native_close_v8 = true;
        } else if arg.starts_with("--native-close-") {
            anyhow::bail!("unsupported native-close protocol opt-in");
        } else if arg == "--checked-controls-v7" {
            checked_controls = true;
        } else if let Some(rest) = arg.strip_prefix("--pipe-out=") {
            pipe_out = Some(rest.to_string());
        }
    }

    anyhow::ensure!(!(native_close_v8 && checked_controls), "runner protocol flags conflict");
    let checked_controls = checked_controls || native_close_v8;
    let Some(pipe_in) = pipe_in else {
        anyhow::bail!("runner: no pipe-in provided");
    };
    let Some(pipe_out) = pipe_out else {
        anyhow::bail!("runner: no pipe-out provided");
    };

    // Open both pipe ends under guards first so a failure on the second open cannot leak the
    // first HANDLE. Only after both opens succeed do we transfer ownership into `File`, which
    // then becomes responsible for closing them.
    let h_pipe_in = OwnedWinHandle::new(open_pipe(&pipe_in, FILE_GENERIC_READ)?);
    let h_pipe_out = OwnedWinHandle::new(open_pipe(&pipe_out, FILE_GENERIC_WRITE)?);
    let mut pipe_read = unsafe { File::from_raw_handle(h_pipe_in.into_raw() as _) };
    let pipe_write = Arc::new(StdMutex::new(unsafe {
        File::from_raw_handle(h_pipe_out.into_raw() as _)
    }));

    let protocol_version = if native_close_v8 {8} else if checked_controls {
        CHECKED_IPC_PROTOCOL_VERSION
    } else {
        IPC_PROTOCOL_VERSION
    };
    let native_close_runtime = if native_close_v8 {
        let original=codex_windows_sandbox::RuntimeRunnerIdentity::capture_current_startup()?;
        codex_windows_sandbox::wait_native_runtime_frame(&pipe_read,native_startup_deadline)?;
        let bootstrap=codex_windows_sandbox::native_close_wire::read_v8_wire_frame_bounded(&mut pipe_read)?
            .ok_or_else(||anyhow::anyhow!("runner closed before v8 bootstrap"))?;
        let runtime=codex_windows_sandbox::NativeCloseRuntime::runner_bootstrap(bootstrap,original)?;
        let ack=FramedMessage {version:8,message:Message::HelloAck {payload:ControlHelloAck {
            hello:runtime.control_hello(),controls:RunnerControlCapabilities {close_stdin:true,resize:true}}}};
        let writer_runtime=Arc::clone(&runtime);
        checked_lifecycle::send_native_precontext_frame_with(&pipe_write,
            native_startup_deadline,move |writer| writer_runtime.write_message(writer,&ack))?;
        Some(runtime)
    }else{None};
    let hello = if let Some(runtime)=native_close_runtime.as_ref() {
        Some(runtime.control_hello())
    }else if checked_controls {
        let hello_frame = read_input_frame_checked(&mut pipe_read)
            .map_err(|err| anyhow::anyhow!(err.into_sticky_error().detail))?
            .ok_or_else(|| anyhow::anyhow!("runner closed before control hello"))?;
        anyhow::ensure!(
            hello_frame.version == protocol_version,
            "runner hello protocol mismatch"
        );
        let hello = match hello_frame.message {
            Message::Hello { payload } if payload.valid() => payload,
            _ => anyhow::bail!("runner expected matched control hello before spawn"),
        };
        {
            let mut writer = pipe_write
                .lock()
                .map_err(|_| anyhow::anyhow!("runner hello output lock poisoned"))?;
            write_frame(
                &mut *writer,
                &FramedMessage {
                    version: protocol_version,
                    message: Message::HelloAck {
                        payload: ControlHelloAck {
                            hello: hello.clone(),
                            controls: RunnerControlCapabilities {
                                close_stdin: true,
                                resize: true,
                            },
                        },
                    },
                },
            )?;
        }

        Some(hello)
    } else {
        None
    };

    let spawn_read = if let Some(runtime)=native_close_runtime.as_ref() {
        codex_windows_sandbox::wait_native_runtime_frame(&pipe_read,native_startup_deadline)?;
        runtime.read_message(&mut pipe_read).map_err(anyhow::Error::from).and_then(|frame|match frame {
            Some(Some(FramedMessage {message:Message::SpawnRequest {payload},..}))=>Ok(*payload),
            _=>Err(anyhow::anyhow!("v8 runner expected authenticated spawn request")),
        })
    }else{read_spawn_request(&mut pipe_read,protocol_version)};
    let req = match spawn_read {
        Ok(v) => v,
        Err(err) => {
            if native_close_v8 {
                let _=send_native_precontext_error(&pipe_write,native_close_runtime.as_ref(),
                    ErrorStage::ReadSpawnRequest,None,native_startup_deadline);
            }else{
            let _ = send_error(
                None,
                &pipe_write,
                ErrorStage::ReadSpawnRequest,
                /*windows_error_code*/ None,
                err.to_string(),
                protocol_version,
            );
            }
            return Err(err);
        }
    };

    if native_close_v8 && std::time::Instant::now()>=native_startup_deadline {
        anyhow::bail!("v8 original spawn admission deadline expired; no replay");
    }
    if native_close_v8 && req.timeout_ms.is_some_and(|ms|ms>=u32::MAX as u64) {
        anyhow::bail!("v8 child timeout exceeds finite DWORD bound");
    }
    let mut legacy_pipe_read=Some(pipe_read);
    if let Some(runtime)=native_close_runtime.as_ref() {
        let original=Arc::new(legacy_pipe_read.take().unwrap());
        checked_lifecycle::retain_native_transport_before_child(runtime,original,Arc::clone(&pipe_write))?;
    }
    let mut context = match spawn_ipc_process(&req, checked_controls) {
        Ok(value) => value,
        Err(err) => {
            if native_close_v8 {
                let _=send_native_retained_startup_error(&pipe_write,native_close_runtime.as_ref(),
                    windows_error_code(&err),RunnerStartupSite::from_error(&err),native_startup_deadline);
            }else{
            let _ = send_error(
                None,
                &pipe_write,
                ErrorStage::SpawnChild,
                windows_error_code(&err),
                err.to_string(),
                protocol_version,
            );
            }
            return Err(err);
        }
    };

    // Install the same session in original context before any child-stage frame.
    context.native_close_runtime=native_close_runtime;
    if let Some(runtime)=context.native_close_runtime.clone() {
        if let Err(error)=context.adopt_native_transport(runtime.attempt()) {
            context.sticky_latch.record_first(StickyUnknownKind::StartupValidationFailed,error.to_string());
            let _=park_unresolved_attempt_context(context);return Err(error.into());
        }
    }
    let child_identity = match context.process_owner.identity() {
        Ok(id) => id,
        Err(err) => {
            context.sticky_latch.record_first(
                StickyUnknownKind::StartupValidationFailed,
                err.to_string(),
            );
            let _ = send_error(
                Some(&mut context),
                &pipe_write,
                ErrorStage::SpawnChild,
                None,
                err.to_string(),
                protocol_version,
            );
            let _ = park_unresolved_attempt_context(context);
            return Err(err.into());
        }
    };
    if let Some(runtime)=context.native_close_runtime.as_ref() {
        let identity=codex_windows_sandbox::native_close_wire::VerifiedChildProcessIdentity::try_new(
            child_identity.process_id,child_identity.thread_id,child_identity.creation_time_100ns)?;
        if let Err(error)=runtime.set_child(identity) {
            context.sticky_latch.record_first(StickyUnknownKind::StartupValidationFailed,error.to_string());
            let _=park_unresolved_attempt_context(context);return Err(error.into());
        }
    }
    let close_stdin_cap = checked_controls
        && context
            .stdin_owner
            .as_ref()
            .is_some_and(StdinHandleOwner::is_open);
    let resize_cap = match context.conpty_resize.has_active_conpty() {
        Ok(active) => checked_controls && active,
        Err(err) => {
            context
                .sticky_latch
                .record_first(StickyUnknownKind::ConptyLockPoisoned, err.to_string());
            let _ = send_error(
                Some(&mut context),
                &pipe_write,
                ErrorStage::WriteSpawnReady,
                None,
                err.to_string(),
                protocol_version,
            );
            let _ = park_unresolved_attempt_context(context);
            return Err(err.into());
        }
    };

    let msg = FramedMessage {
        version: protocol_version,
        message: Message::SpawnReady {
            payload: SpawnReady {
                process_id: child_identity.process_id,
                controls: RunnerControlCapabilities {
                    close_stdin: close_stdin_cap,
                    resize: resize_cap,
                },
            },
        },
    };
    let ready_deadline = std::time::Instant::now() + DEFAULT_SHUTDOWN_BUDGET;
    if checked_controls { context.error_write_deadline = Some(ready_deadline); }
    if let Err(err) = if checked_controls {
        context.send_frame_checked(&pipe_write, msg, ready_deadline)
            .map_err(anyhow::Error::from)
    } else if let Ok(mut guard) = pipe_write.lock() {
        write_frame(&mut *guard, &msg)
    } else {
        anyhow::bail!("runner spawn_ready write failed: pipe_write lock poisoned");
    } {
        context
            .sticky_latch
            .record_first(StickyUnknownKind::WriteSpawnReadyFailed, err.to_string());
        let _ = send_error(
                Some(&mut context),
            &pipe_write,
            ErrorStage::WriteSpawnReady,
            /*windows_error_code*/ None,
            err.to_string(),
            protocol_version,
        );
        let _ = park_unresolved_attempt_context(context);
        return Err(err);
    }

    let hello_nonce = hello.map_or([0; 32], |hello| hello.nonce);
    let io_started=if native_close_v8 {
        context.start_native_io_workers(Arc::clone(&pipe_write),hello_nonce)
    }else{context.start_io_workers(legacy_pipe_read.take().unwrap(),Arc::clone(&pipe_write),hello_nonce,protocol_version)};
    if let Err(err) = io_started {
        context
            .sticky_latch
            .record_first(StickyUnknownKind::StartupValidationFailed, err.to_string());
        let _ = send_error(
                Some(&mut context),
            &pipe_write,
            ErrorStage::TerminalWitness,
            None,
            err.to_string(),
            protocol_version,
        );
        let _ = park_unresolved_attempt_context(context);
        return Err(err.into());
    }

    let wait_handle = match context.process_owner.duplicate_wait_handle() {
        Ok(handle) => handle,
        Err(err) => {
            context
                .sticky_latch
                .record_first(StickyUnknownKind::ChildWaitFailed, err.to_string());
            let _ = send_error(
                Some(&mut context),
                &pipe_write,
                ErrorStage::TerminalWitness,
                None,
                err.to_string(),
                protocol_version,
            );
            let _ = park_unresolved_attempt_context(context);
            return Err(err.into());
        }
    };

    let timeout = req.timeout_ms.map(|ms| ms as u32).unwrap_or(INFINITE);
    let wait_res = unsafe { WaitForSingleObject(wait_handle.raw(), timeout) };
    let timed_out = wait_res == WAIT_TIMEOUT;

    if checked_controls {
        let mut wait_handle = wait_handle;
        if timed_out && native_close_v8 {
            context.process_owner.record_wait_timeout_from_original();
            let _=wait_handle.close_if_open_checked("v8 child wait timeout");
            context.auxiliary_close_debts[4]=wait_handle.close_debt();
            context.sticky_latch.record_first(StickyUnknownKind::ChildTimedOut,
                "v8 original child wait expired; no automatic terminate/replay");
            let _=park_unresolved_attempt_context(context);
            anyhow::bail!("v8 native child still unresolved; original owner retained");
        }
        if timed_out {
            context.process_owner.record_wait_timeout_from_original();
            context
                .process_owner
                .terminate_job_or_open_process(&context.job, Some(context.log_dir.as_path()));
            let _ = unsafe { WaitForSingleObject(wait_handle.raw(), TERMINATION_WAIT_MS) };
            let _ = wait_handle.close_if_open_checked("child wait");
            context.auxiliary_close_debts[4] = wait_handle.close_debt();
            let sticky = context.sticky_latch.record_first(
                StickyUnknownKind::ChildTimedOut,
                "runner process exit was not witnessed before timeout",
            );
            context.refresh_native_facts_snapshot();
            let _ = send_error(
                Some(&mut context),
                &pipe_write,
                ErrorStage::TerminalWitness,
                None,
                sticky.detail.clone(),
                protocol_version,
            );
            let _ = park_unresolved_attempt_context(context);
            return Err(anyhow::anyhow!(sticky.detail));
        }

        if let Err(err) = wait_handle.close_checked("child wait") {
            context.auxiliary_close_debts[4] = wait_handle.close_debt();
            let sticky = context.sticky_latch.record_first(
                StickyUnknownKind::ChildHandleCloseFailed,
                format!("closing duplicated child wait handle failed: {err}"),
            );
            context.refresh_native_facts_snapshot();
            let _ = send_error(
                Some(&mut context),
                &pipe_write,
                ErrorStage::TerminalWitness,
                None,
                sticky.detail.clone(),
                protocol_version,
            );
            let _ = park_unresolved_attempt_context(context);
            return Err(err.into());
        }

        let child_exit = match context
            .process_owner
            .record_verified_exit_from_original(wait_res, false)
            .and_then(|witness| {
                verified_exit_code(wait_res, true, witness.raw_exit_code).map(|_| witness)
            }) {
            Ok(witness) => witness,
            Err(err) => {
                let kind = if wait_res != 0 {
                    StickyUnknownKind::ChildWaitFailed
                } else {
                    StickyUnknownKind::ChildExitCodeUnavailable
                };
                context.sticky_latch.record_first(kind, err.to_string());
                context.refresh_native_facts_snapshot();
                let _ = send_error(
                Some(&mut context),
                    &pipe_write,
                    ErrorStage::TerminalWitness,
                    None,
                    err.to_string(),
                    protocol_version,
                );
                let _ = park_unresolved_attempt_context(context);
                return Err(err.into());
            }
        };
        context.ledger.child_exit = Some(child_exit);
        context.refresh_native_facts_snapshot();

        let shutdown_deadline = std::time::Instant::now() + DEFAULT_SHUTDOWN_BUDGET;
        context.error_write_deadline = Some(shutdown_deadline);

        if let Err(sticky) = context
            .conpty_resize
            .close_conpty()
            .map_err(|err| {
                context
                    .sticky_latch
                    .record_first(StickyUnknownKind::ConptyLockPoisoned, err.to_string())
            })
            .and_then(|()| context.join_output_workers_bounded(shutdown_deadline))
        {
            context.refresh_native_facts_snapshot();
            let _ = send_error(
                Some(&mut context),
                &pipe_write,
                ErrorStage::TerminalWitness,
                None,
                sticky.detail.clone(),
                protocol_version,
            );
            let _ = park_unresolved_attempt_context(context);
            return Err(anyhow::anyhow!(sticky.detail));
        }

        if let Err(sticky) = context.send_business_exit_checked(&pipe_write, shutdown_deadline) {
            let _ = park_unresolved_attempt_context(context);
            return Err(anyhow::anyhow!(sticky.detail));
        }

        if let Err(sticky) = context
            .join_input_worker_bounded(shutdown_deadline)
            .and_then(|_| context.close_child_handles_checked())
            .and_then(|()| context.drain_job_active_zero_bounded(shutdown_deadline))
            .and_then(|_| {
                context.ledger.into_normal_close_witness().ok_or_else(|| {
                    context.sticky_latch.record_first(
                        StickyUnknownKind::JobDrainFailed,
                        "normal close witness incomplete after job drain",
                    )
                })
            })
        {
            log_note(
                &format!("runner checked normal-close failed: {}", sticky.detail),
                Some(context.log_dir.as_path()),
            );
            let _ = park_unresolved_attempt_context(context);
            return Err(anyhow::anyhow!(sticky.detail));
        }

        if context.native_close_runtime.is_some() {
            if let Err(error)=context.send_native_close_receipt(&pipe_write,shutdown_deadline,resize_cap) {
                context.sticky_latch.record_first(StickyUnknownKind::InputControlWriteFailed,
                    format!("native-close receipt write/join failed: {error}"));
                let _=park_unresolved_attempt_context(context);return Err(error.into());
            }
        }
        context.mark_clean_completion_for_drop();
        drop(pipe_write);
        drop(context);
        return Ok(child_exit.exit_code);
    }

    // Legacy unchecked (protocol v6) path: preserve historical behavior explicitly.
    let final_wait = if timed_out {
        context.process_owner.record_wait_timeout_from_original();
        context
            .process_owner
            .terminate_job_or_open_process(&context.job, Some(context.log_dir.as_path()));
        unsafe { WaitForSingleObject(wait_handle.raw(), TERMINATION_WAIT_MS) }
    } else {
        let preserve_res = context.job.preserve_descendants();
        context.record_job_preserve_descendants_result(&preserve_res);
        if let Err(err) = preserve_res {
            log_note(
                &format!("runner failed to preserve descendants after root exit: {err}"),
                Some(context.log_dir.as_path()),
            );
        }
        wait_res
    };
    drop(wait_handle);

    let raw_exit = if !timed_out {
        context
            .process_owner
            .record_verified_exit_from_original(final_wait, false)
            .map(|w| w.raw_exit_code)
            .unwrap_or(1)
    } else {
        1
    };
    if let Ok(id) = context.process_owner.identity() {
        let _ = context.thread_disposition.close_checked(id.thread_id);
    }
    let _ = context.process_owner.close_process_handle_checked();
    let exit_code = if timed_out { 128 + 64 } else { raw_exit as i32 };

    let _ = context.conpty_resize.close_conpty();

    if let Some(ref mut out_worker) = context.stdout_worker { let _ = out_worker.join_legacy(); }
    if let Some(ref mut err_worker) = context.stderr_worker { let _ = err_worker.join_legacy(); }
    for handle in [&mut context.stdout_owner, &mut context.stderr_owner].into_iter().flatten() {
        if let Err(err) = handle.close_if_open_checked("legacy child output") {
            context.sticky_latch.record_first(StickyUnknownKind::ChildHandleCloseFailed, err.to_string());
        }
    }
    context.refresh_native_facts_snapshot();

    let exit_msg = FramedMessage {
        version: protocol_version,
        message: Message::Exit {
            payload: ExitPayload {
                exit_code,
                timed_out,
            },
        },
    };
    if let Ok(mut guard) = pipe_write.lock()
        && let Err(err) = write_frame(&mut *guard, &exit_msg)
    {
        log_note(
            &format!("runner exit write failed: {err}"),
            Some(context.log_dir.as_path()),
        );
    }
    let witnessed_root_exit = context.process_owner.is_child_exit_signaled();
    let input_retired = if let Some(ref mut input) = context.input_worker {
        if witnessed_root_exit {
            input.join_legacy_terminal_input().is_ok_and(|outcome|
                outcome.peer_eof_observed && outcome.retired_after_child_exit)
        } else { input.join_legacy().is_ok() }
    } else { false };
    if !input_retired {
        context.sticky_latch.record_first(StickyUnknownKind::InputIoError,
            "legacy terminal input did not join with a real EOF witness");
        let _ = park_unresolved_attempt_context(context);
        anyhow::bail!("legacy terminal input retirement unresolved");
    }
    if context.has_close_debt() {
        let _ = park_unresolved_attempt_context(context);
        anyhow::bail!("legacy original close debt retained; native exit prohibited");
    }
    context.mark_clean_completion_for_drop();
    drop(pipe_write);
    drop(context);
    Ok(exit_code)
}
