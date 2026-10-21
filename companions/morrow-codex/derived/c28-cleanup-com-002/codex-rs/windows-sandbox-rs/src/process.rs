#[path = "process/asuser_stdio.rs"]
mod asuser_stdio;

use crate::desktop::LaunchDesktop;
use crate::logging;
use crate::proc_thread_attr::ProcThreadAttributeList;
use crate::winutil::argv_to_command_line;
use crate::winutil::format_last_error;
use crate::winutil::to_wide;
use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use codex_utils_pty::JobObject;
use std::collections::HashMap;
use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr;
use std::sync::Arc;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::HANDLE_FLAG_INHERIT;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Foundation::SetHandleInformation;
use windows_sys::Win32::Storage::FileSystem::ReadFile;
use windows_sys::Win32::System::Console::GetStdHandle;
use windows_sys::Win32::System::Console::STD_ERROR_HANDLE;
use windows_sys::Win32::System::Console::STD_INPUT_HANDLE;
use windows_sys::Win32::System::Console::STD_OUTPUT_HANDLE;
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
use windows_sys::Win32::System::Threading::CREATE_UNICODE_ENVIRONMENT;
use windows_sys::Win32::System::Threading::CreateProcessAsUserW;
use windows_sys::Win32::System::Threading::EXTENDED_STARTUPINFO_PRESENT;
use windows_sys::Win32::System::Threading::PROCESS_INFORMATION;
use windows_sys::Win32::System::Threading::STARTF_USESTDHANDLES;
use windows_sys::Win32::System::Threading::STARTUPINFOEXW;
use windows_sys::Win32::System::Threading::STARTUPINFOW;

/// Operation evidence captured at the CreateProcessAsUserW boundary. The private
/// representation prevents a caller from interpreting an arbitrary zero handle as
/// proof that no process was created.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessCreationEvidence(ProcessCreationDisposition);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessCreationDisposition {
    NotInvoked,
    Rejected { os_error: i32 },
    Created,
    Uncertain,
}

/// Positive absence evidence, distinct from an exit or CloseHandle witness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessNotCreated(ProcessCreationDisposition);

impl ProcessCreationEvidence {
    pub(crate) fn not_invoked() -> Self {
        Self(ProcessCreationDisposition::NotInvoked)
    }

    // Called only with the original return value and original zero-initialized PI.
    // An unsuccessful call with unexpected outputs is retained as uncertain.
    pub(crate) fn returned(ok: i32, pi: &PROCESS_INFORMATION, os_error: i32) -> Self {
        Self(if ok != 0 {
            ProcessCreationDisposition::Created
        } else if Self::empty_originals(pi) {
            ProcessCreationDisposition::Rejected { os_error }
        } else {
            ProcessCreationDisposition::Uncertain
        })
    }

    fn empty_originals(pi: &PROCESS_INFORMATION) -> bool {
        pi.hProcess == 0 && pi.hThread == 0 && pi.dwProcessId == 0 && pi.dwThreadId == 0
    }

    pub fn not_created_for(&self, pi: &PROCESS_INFORMATION) -> Option<ProcessNotCreated> {
        if Self::empty_originals(pi) && matches!(self.0,
            ProcessCreationDisposition::NotInvoked | ProcessCreationDisposition::Rejected { .. }) {
            Some(ProcessNotCreated(self.0))
        } else { None }
    }
}

impl ProcessNotCreated {
    pub fn reason(&self) -> &'static str {
        match self.0 {
            ProcessCreationDisposition::NotInvoked => "CreateProcessAsUserW was not invoked",
            ProcessCreationDisposition::Rejected { .. } => "CreateProcessAsUserW explicitly rejected creation",
            _ => unreachable!("absence proof has a private, checked constructor"),
        }
    }

    pub fn rejection_os_error(&self) -> Option<i32> {
        match self.0 { ProcessCreationDisposition::Rejected { os_error } => Some(os_error), _ => None }
    }
}

pub struct CreatedProcess {
    pub process_info: PROCESS_INFORMATION,
    pub startup_info: STARTUPINFOW,
    pub(crate) job: Arc<JobObject>,
    _desktop: LaunchDesktop,
    launch_error: Option<anyhow::Error>,
    creation_evidence: ProcessCreationEvidence,
}

/// Controls console creation for pipe-backed child processes.
pub enum ConsoleMode {
    Inherit,
    NoWindow,
}

pub fn make_env_block(env: &HashMap<String, String>) -> Vec<u16> {
    let mut items: Vec<(String, String)> =
        env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    items.sort_by(|a, b| {
        a.0.to_uppercase()
            .cmp(&b.0.to_uppercase())
            .then(a.0.cmp(&b.0))
    });
    let mut w: Vec<u16> = Vec::new();
    for (k, v) in items {
        let mut s = to_wide(format!("{k}={v}"));
        s.pop();
        w.extend_from_slice(&s);
        w.push(0);
    }
    w.push(0);
    w
}

unsafe fn ensure_inheritable_stdio(si: &mut STARTUPINFOW) -> Result<()> {
    for kind in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        let h = GetStdHandle(kind);
        if h == 0 || h == INVALID_HANDLE_VALUE {
            return Err(anyhow!("GetStdHandle failed: {}", GetLastError()));
        }
        if SetHandleInformation(h, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) == 0 {
            return Err(anyhow!("SetHandleInformation failed: {}", GetLastError()));
        }
    }
    si.dwFlags |= STARTF_USESTDHANDLES;
    si.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
    si.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
    si.hStdError = GetStdHandle(STD_ERROR_HANDLE);
    Ok(())
}

/// # Safety
/// Caller must provide a valid primary token handle (`h_token`) with appropriate access,
/// and the `argv`, `cwd`, and `env_map` must remain valid for the duration of the call.
// Low-level CreateProcessAsUserW wrapper mirrors the Windows API shape.
#[allow(clippy::too_many_arguments)]
pub unsafe fn create_process_as_user(
    h_token: HANDLE,
    argv: &[String],
    cwd: &Path,
    env_map: &HashMap<String, String>,
    logs_base_dir: Option<&Path>,
    stdio: Option<(HANDLE, HANDLE, HANDLE)>,
    console_mode: ConsoleMode,
    desktop: LaunchDesktop,
) -> Result<CreatedProcess> {
    create_process_as_user_impl(h_token, argv, cwd, env_map, logs_base_dir, stdio, console_mode, desktop, false)
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_process_as_user_impl(
    h_token: HANDLE,
    argv: &[String],
    cwd: &Path,
    env_map: &HashMap<String, String>,
    logs_base_dir: Option<&Path>,
    stdio: Option<(HANDLE, HANDLE, HANDLE)>,
    console_mode: ConsoleMode,
    desktop: LaunchDesktop,
    retain_on_launch_error: bool,
) -> Result<CreatedProcess> {
    let cmdline_str = argv_to_command_line(argv);
    let mut cmdline: Vec<u16> = to_wide(&cmdline_str);
    let env_block = make_env_block(env_map);
    let job = Arc::new(JobObject::create().context(crate::RunnerStartupSite::CreateProcessJob)?);
    let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
    let cwd_wide = to_wide(cwd);
    let env_block_len = env_block.len();
    let console_flags = match (&stdio, console_mode) {
        (Some(_), ConsoleMode::NoWindow) => CREATE_NO_WINDOW,
        (Some(_), ConsoleMode::Inherit)
        | (None, ConsoleMode::Inherit)
        | (None, ConsoleMode::NoWindow) => 0,
    };
    let preserve_app_context = crate::app_package::current_process_has_package_identity()
        .context(crate::RunnerStartupSite::QueryAppPackage)?;
    let attr_count = if stdio.is_some() { 2 } else { 1 } + u32::from(preserve_app_context);
    let mut attrs = ProcThreadAttributeList::new(attr_count)
        .context(crate::RunnerStartupSite::AllocateProcessAttributes)?;
    attrs.set_job(job.as_raw_handle() as HANDLE).context(crate::RunnerStartupSite::SetProcessJobAttribute)?;
    if preserve_app_context {
        attrs.preserve_desktop_app_context().context(crate::RunnerStartupSite::PreserveDesktopAppContext)?;
    }

    let mut si: STARTUPINFOEXW = std::mem::zeroed();
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    // Some processes (e.g., PowerShell) can fail with STATUS_DLL_INIT_FAILED
    // if lpDesktop is not set when launching with a restricted token.
    // Point explicitly at the interactive desktop or a private desktop.
    si.StartupInfo.lpDesktop = desktop.startup_info_desktop();
    match stdio {
        Some((stdin_h, stdout_h, stderr_h)) => {
            si.StartupInfo.dwFlags |= STARTF_USESTDHANDLES;
            si.StartupInfo.hStdInput = stdin_h;
            si.StartupInfo.hStdOutput = stdout_h;
            si.StartupInfo.hStdError = stderr_h;
            let mut inherited_handles = vec![stdin_h, stdout_h];
            if !inherited_handles.contains(&stderr_h) {
                inherited_handles.push(stderr_h);
            }
            for &handle in &inherited_handles {
                if SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) == 0 {
                    return Err(anyhow::Error::from(std::io::Error::last_os_error())
                        .context(crate::RunnerStartupSite::SetStdioInherit));
                }
            }
            attrs.set_handle_list(inherited_handles).context(crate::RunnerStartupSite::SetInheritedHandleList)?;
        }
        None => {
            ensure_inheritable_stdio(&mut si.StartupInfo)?;
        }
    }
    si.lpAttributeList = attrs.as_mut_ptr();

    let creation_flags = CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT | console_flags;
    let ok = CreateProcessAsUserW(
        h_token,
        std::ptr::null(),
        cmdline.as_mut_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        1,
        creation_flags,
        env_block.as_ptr() as *mut c_void,
        cwd_wide.as_ptr(),
        &si.StartupInfo,
        &mut pi,
    );
    if ok == 0 {
        let err = GetLastError() as i32;
        let msg = format!(
            "CreateProcessAsUserW failed: {} ({}) | cwd={} | cmd={} | env_u16_len={} | si_flags={} | creation_flags={}",
            err,
            format_last_error(err),
            cwd.display(),
            cmdline_str,
            env_block_len,
            si.StartupInfo.dwFlags,
            creation_flags,
        );
        if !retain_on_launch_error { logging::debug_log(&msg, logs_base_dir); }
        let first_error = anyhow::Error::from(std::io::Error::from_raw_os_error(err))
            .context(msg).context(crate::RunnerStartupSite::CreateProcessAsUser);
        if retain_on_launch_error {
            return Ok(CreatedProcess { process_info: pi, startup_info: si.StartupInfo,
                job, _desktop: desktop, launch_error: Some(first_error),
                creation_evidence: ProcessCreationEvidence::returned(ok, &pi, err) });
        }
        return Err(first_error);
    }

    Ok(CreatedProcess {
        process_info: pi,
        startup_info: si.StartupInfo,
        job,
        _desktop: desktop,
        launch_error: None,
        creation_evidence: ProcessCreationEvidence::returned(ok, &pi, 0),
    })
}

/// Controls whether the child's stdin handle is kept open for writing.
#[allow(dead_code)]
pub enum StdinMode {
    Closed,
    Open,
}

/// Controls how stderr is wired for a pipe-spawned process.
#[allow(dead_code)]
pub enum StderrMode {
    MergeStdout,
    Separate,
}

/// Handles returned by `spawn_process_with_pipes`.
#[allow(dead_code)]
/// Uncertain numeric close evidence, never an owned/retryable HANDLE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipeHandleCloseDebt { pub raw_handle: usize, pub os_error: i32 }

pub struct PipeSpawnHandles {
    pub process: PROCESS_INFORMATION,
    job: Arc<JobObject>,
    pub stdin_write: Option<HANDLE>,
    pub stdout_read: HANDLE,
    pub stderr_read: Option<HANDLE>,
    pub(crate) desktop: LaunchDesktop,
    close_debts: [Option<PipeHandleCloseDebt>; 4],
    launch_error: Option<anyhow::Error>,
    creation_evidence: ProcessCreationEvidence,
}

impl PipeSpawnHandles {
    pub fn creation_evidence(&self) -> ProcessCreationEvidence { self.creation_evidence }

    pub fn launch_error(&self) -> Option<&anyhow::Error> { self.launch_error.as_ref() }

    pub fn close_debts(&self) -> &[Option<PipeHandleCloseDebt>; 4] { &self.close_debts }
    pub fn primary_close_error(&self) -> Option<i32> {
        self.close_debts.iter().flatten().next().map(|debt| debt.os_error)
    }

    /// Returns the Job Object containing the spawned process.
    pub fn job(&self) -> Arc<JobObject> {
        Arc::clone(&self.job)
    }
}

/// Spawns a process with anonymous pipes and returns the relevant handles.
#[allow(clippy::too_many_arguments)]
pub fn spawn_process_with_pipes(
    h_token: HANDLE,
    argv: &[String],
    cwd: &Path,
    env_map: &HashMap<String, String>,
    stdin_mode: StdinMode,
    stderr_mode: StderrMode,
    console_mode: ConsoleMode,
    desktop: LaunchDesktop,
    logs_base_dir: Option<&Path>,
) -> Result<PipeSpawnHandles> {
    spawn_process_with_pipes_impl(h_token, argv, cwd, env_map, stdin_mode, stderr_mode, console_mode, desktop, logs_base_dir, false)
}

/// Checked path: atomic Job admission via PROC_THREAD_ATTRIBUTE_JOB_LIST avoids
/// fallible post-CreateProcess assign/resume cleanup. Every returned process/thread,
/// parent pipe, desktop and Job original plus post-launch child-pipe close debt is
/// transferred together. The caller must anchor before fallible validation.
#[allow(clippy::too_many_arguments)]
pub fn spawn_process_with_pipes_retaining(
    h_token: HANDLE,
    argv: &[String],
    cwd: &Path,
    env_map: &HashMap<String, String>,
    stdin_mode: StdinMode,
    stderr_mode: StderrMode,
    console_mode: ConsoleMode,
    desktop: LaunchDesktop,
    logs_base_dir: Option<&Path>,
) -> Result<PipeSpawnHandles> {
    spawn_process_with_pipes_impl(h_token, argv, cwd, env_map, stdin_mode, stderr_mode, console_mode, desktop, logs_base_dir, true)
}

#[allow(clippy::too_many_arguments)]
fn spawn_process_with_pipes_impl(
    h_token: HANDLE,
    argv: &[String],
    cwd: &Path,
    env_map: &HashMap<String, String>,
    stdin_mode: StdinMode,
    stderr_mode: StderrMode,
    console_mode: ConsoleMode,
    desktop: LaunchDesktop,
    logs_base_dir: Option<&Path>,
    retain_close_debt: bool,
) -> Result<PipeSpawnHandles> {
    // A1 deliberately keeps the trusted runner token's default DACL RC-only.
    // CreatePipe opens both anonymous endpoints and cannot use that template
    // for data IO. Supply creation-only rights for this original child's logon;
    // the token template and legacy/default routes remain unchanged.
    let mut security = if desktop.uses_a1_stdio_security() {
        Some(asuser_stdio::StdioSecurity::for_original_token(h_token)
            .map_err(|error| crate::RunnerStartupSite::PrepareStdioSecurity.context(error))?)
    } else { None };
    let attributes = security.as_mut().map(|value| value.attributes());
    let pipe_attributes = attributes.as_ref().map_or(ptr::null(), |value| value as *const _);
    let mut in_r: HANDLE = 0;
    let mut in_w: HANDLE = 0;
    let mut out_r: HANDLE = 0;
    let mut out_w: HANDLE = 0;
    let mut err_r: HANDLE = 0;
    let mut err_w: HANDLE = 0;
    unsafe {
        if CreatePipe(&mut in_r, &mut in_w, pipe_attributes, 0) == 0 {
            return Err(anyhow::Error::from(std::io::Error::last_os_error()).context(crate::RunnerStartupSite::CreateStdinPipe));
        }
        if CreatePipe(&mut out_r, &mut out_w, pipe_attributes, 0) == 0 {
            let error = std::io::Error::last_os_error();
            CloseHandle(in_r);
            CloseHandle(in_w);
            return Err(anyhow::Error::from(error).context(crate::RunnerStartupSite::CreateStdoutPipe));
        }
        if matches!(stderr_mode, StderrMode::Separate)
            && CreatePipe(&mut err_r, &mut err_w, pipe_attributes, 0) == 0
        {
            let error = std::io::Error::last_os_error();
            CloseHandle(in_r);
            CloseHandle(in_w);
            CloseHandle(out_r);
            CloseHandle(out_w);
            return Err(anyhow::Error::from(error).context(crate::RunnerStartupSite::CreateStderrPipe));
        }
    }

    let stderr_handle = match stderr_mode {
        StderrMode::MergeStdout => out_w,
        StderrMode::Separate => err_w,
    };

    let stdio = Some((in_r, out_w, stderr_handle));
    let spawn_result = unsafe {
        create_process_as_user_impl(
            h_token,
            argv,
            cwd,
            env_map,
            logs_base_dir,
            stdio,
            console_mode,
            desktop,
            retain_close_debt,
        )
    };
    let created = match spawn_result {
        Ok(v) => v,
        Err(err) => {
            unsafe {
                CloseHandle(in_r);
                CloseHandle(in_w);
                CloseHandle(out_r);
                CloseHandle(out_w);
                if matches!(stderr_mode, StderrMode::Separate) {
                    CloseHandle(err_r);
                    CloseHandle(err_w);
                }
            }
            return Err(err);
        }
    };
    let CreatedProcess {
        process_info: pi,
        job,
        _desktop: desktop,
        launch_error,
        creation_evidence,
        ..
    } = created;

    let mut close_debts = [None; 4];
    let handles = [in_r, out_w,
        if matches!(stderr_mode, StderrMode::Separate) { err_w } else { 0 },
        if matches!(stdin_mode, StdinMode::Closed) { in_w } else { 0 }];
    for (index, handle) in handles.into_iter().enumerate() {
        if handle != 0 && unsafe { CloseHandle(handle) } == 0 && retain_close_debt {
            close_debts[index] = Some(PipeHandleCloseDebt {
                raw_handle: handle as usize, os_error: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
            });
        }
    }

    Ok(PipeSpawnHandles {
        process: pi,
        job,
        stdin_write: match stdin_mode {
            StdinMode::Open => Some(in_w),
            StdinMode::Closed => None,
        },
        stdout_read: out_r,
        stderr_read: match stderr_mode {
            StderrMode::Separate => Some(err_r),
            StderrMode::MergeStdout => None,
        },
        desktop,
        close_debts,
        launch_error,
        creation_evidence,
    })
}

/// Reads a HANDLE until EOF and invokes on_chunk for each read.
/// The historical entry point deliberately discards read errors.
pub fn read_handle_loop<F>(handle: HANDLE, mut on_chunk: F) -> std::thread::JoinHandle<()>
where
    F: FnMut(&[u8]) + Send + 'static,
{
    std::thread::spawn(move || {
        let _ = read_handle_chunks(handle, |chunk| {
            on_chunk(chunk);
            Ok(())
        });
    })
}

/// Checked runner only: distinguish pipe EOF from OS/forwarding failure.
/// Panics remain join errors, never an EOF acknowledgement.
pub fn read_handle_loop_checked<F>(
    handle: HANDLE,
    on_chunk: F,
) -> std::thread::JoinHandle<std::io::Result<()>>
where
    F: FnMut(&[u8]) -> std::io::Result<()> + Send + 'static,
{
    std::thread::spawn(move || read_handle_chunks(handle, on_chunk))
}

fn read_handle_chunks<F>(handle: HANDLE, mut on_chunk: F) -> std::io::Result<()>
where
    F: FnMut(&[u8]) -> std::io::Result<()>,
{
    let mut buf = [0u8; 8192];
    let result = loop {
        let mut read_bytes: u32 = 0;
        // This is the original ReadFile call site, shared by both entry points.
        let ok = unsafe {
            ReadFile(
                handle,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut read_bytes,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            let err = std::io::Error::last_os_error();
            // Anonymous/ConPTY pipe peer closure is Windows' actual pipe EOF.
            if err.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                break Ok(());
            }
            break Err(err);
        }
        if read_bytes == 0 {
            break Ok(());
        }
        if let Err(err) = on_chunk(&buf[..read_bytes as usize]) {
            break Err(err);
        }
    };
    // Reuse the original handle close site on both normal and IO error paths.
    let closed = unsafe { CloseHandle(handle) };
    if closed == 0 && result.is_ok() {
        return Err(std::io::Error::last_os_error());
    }
    result
}


#[cfg(test)]
mod creation_evidence_tests {
    use super::*;

    #[test]
    fn creation_evidence_distinguishes_not_invoked_rejected_created_and_uncertain() {
        let empty: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let before = ProcessCreationEvidence::not_invoked();
        assert!(before.not_created_for(&empty).is_some());
        let rejected = ProcessCreationEvidence::returned(0, &empty, 5);
        assert_eq!(rejected.not_created_for(&empty).unwrap().rejection_os_error(), Some(5));
        // Even a successful call with missing originals must never become absence.
        assert!(ProcessCreationEvidence::returned(1, &empty, 0).not_created_for(&empty).is_none());
        for changed in 0..4 {
            let mut unexpected = empty;
            match changed {
                0 => unexpected.hProcess = 123,
                1 => unexpected.hThread = 456,
                2 => unexpected.dwProcessId = 789,
                _ => unexpected.dwThreadId = 101,
            }
            assert!(before.not_created_for(&unexpected).is_none());
            assert!(rejected.not_created_for(&unexpected).is_none());
            let uncertain = ProcessCreationEvidence::returned(0, &unexpected, 5);
            assert!(uncertain.not_created_for(&unexpected).is_none());
            assert!(uncertain.not_created_for(&empty).is_none(), "cannot erase uncertain original outputs");
        }
    }
}
