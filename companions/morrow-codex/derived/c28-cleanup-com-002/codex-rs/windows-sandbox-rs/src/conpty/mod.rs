//! ConPTY helpers for spawning sandboxed processes with a PTY on Windows.
//!
//! This module encapsulates ConPTY creation and process spawn with the required
//! `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE` plumbing. It is shared by both the legacy
//! restricted闁炽儲鍞竜ken path and the elevated runner path when unified_exec runs with
//! `tty=true`. The helpers are not tied to the IPC layer and can be reused by other
//! Windows sandbox flows that need a PTY.

use crate::desktop::LaunchDesktop;
use crate::process::ProcessCreationEvidence;
use crate::proc_thread_attr::ProcThreadAttributeList;
use crate::winutil::format_last_error;
use crate::winutil::quote_windows_arg;
use crate::winutil::to_wide;
use anyhow::Context;
use anyhow::Result;
use codex_utils_pty::JobObject;
use codex_utils_pty::PsuedoCon;
use codex_utils_pty::RawConPty;
use std::collections::HashMap;
use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::IntoRawHandle;
use std::path::Path;
use std::sync::Arc;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Threading::CREATE_UNICODE_ENVIRONMENT;
use windows_sys::Win32::System::Threading::CreateProcessAsUserW;
use windows_sys::Win32::System::Threading::EXTENDED_STARTUPINFO_PRESENT;
use windows_sys::Win32::System::Threading::PROCESS_INFORMATION;
use windows_sys::Win32::System::Threading::STARTF_USESTDHANDLES;
use windows_sys::Win32::System::Threading::STARTUPINFOEXW;

use crate::process::make_env_block;

/// Owns a ConPTY handle and its backing pipe handles.
pub struct ConptyInstance {
    pseudoconsole: Option<PsuedoCon>,
    input_write: HANDLE,
    output_read: HANDLE,
    job: Option<Arc<JobObject>>,
    _desktop: Option<LaunchDesktop>,
}

impl Drop for ConptyInstance {
    fn drop(&mut self) {
        unsafe {
            if self.input_write != 0 && self.input_write != INVALID_HANDLE_VALUE {
                CloseHandle(self.input_write);
            }
            if self.output_read != 0 && self.output_read != INVALID_HANDLE_VALUE {
                CloseHandle(self.output_read);
            }
        }
        let _ = self.pseudoconsole.take();
    }
}

impl ConptyInstance {
    pub fn raw_handle(&self) -> Option<HANDLE> {
        self.pseudoconsole
            .as_ref()
            .map(|pseudoconsole| pseudoconsole.raw_handle() as HANDLE)
    }

    pub fn take_input_write(&mut self) -> HANDLE {
        std::mem::replace(&mut self.input_write, 0)
    }

    pub fn take_output_read(&mut self) -> HANDLE {
        std::mem::replace(&mut self.output_read, 0)
    }

    /// Returns the Job Object containing the spawned process, if this instance owns one.
    pub fn job(&self) -> Option<Arc<JobObject>> {
        self.job.as_ref().map(Arc::clone)
    }
}

/// Create a ConPTY with backing pipes.
///
/// This is public so callers that need lower-level PTY setup can build on the same
/// primitive, although the common entry point is `spawn_conpty_process_as_user`.
#[allow(dead_code)]
pub fn create_conpty(cols: i16, rows: i16) -> Result<ConptyInstance> {
    let raw = RawConPty::new(cols, rows)?;
    let (pseudoconsole, input_write, output_read) = raw.into_handles();

    Ok(ConptyInstance {
        pseudoconsole: Some(pseudoconsole),
        input_write: input_write.into_raw_handle() as HANDLE,
        output_read: output_read.into_raw_handle() as HANDLE,
        job: None,
        _desktop: None,
    })
}

/// Checked launch uses atomic Job + ConPTY attributes and transfers the live
/// primary thread. No post-create assign/resume or CloseHandle boundary is hidden
/// inside this API. The receiver installs the originals before validating them.
pub fn spawn_conpty_process_as_user_retaining<R>(
    h_token: HANDLE, argv: &[String], cwd: &Path, env_map: &HashMap<String, String>,
    desktop: LaunchDesktop,
    receive: impl FnOnce(PROCESS_INFORMATION, ConptyInstance, ProcessCreationEvidence, Option<anyhow::Error>) -> R,
) -> Result<R> {
    let (process, console, evidence, first_error) = spawn_conpty_process_as_user_impl(h_token, argv, cwd, env_map, desktop, true)?;
    Ok(receive(process, console, evidence, first_error))
}

/// Spawn a process under `h_token` with ConPTY attached.
///
/// This is the main shared ConPTY entry point and is used by both the legacy/direct path
/// and the elevated runner path whenever a PTY-backed sandboxed process is needed.
pub fn spawn_conpty_process_as_user(
    h_token: HANDLE,
    argv: &[String],
    cwd: &Path,
    env_map: &HashMap<String, String>,
    desktop: LaunchDesktop,
) -> Result<(PROCESS_INFORMATION, ConptyInstance)> {
    let (pi, console, _, _) = spawn_conpty_process_as_user_impl(h_token, argv, cwd, env_map, desktop, false)?;
    Ok((pi, console))
}

fn spawn_conpty_process_as_user_impl(
    h_token: HANDLE,
    argv: &[String],
    cwd: &Path,
    env_map: &HashMap<String, String>,
    desktop: LaunchDesktop,
    retain_on_launch_error: bool,
) -> Result<(PROCESS_INFORMATION, ConptyInstance, ProcessCreationEvidence, Option<anyhow::Error>)> {
    let cmdline_str = argv
        .iter()
        .map(|arg| quote_windows_arg(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let mut cmdline: Vec<u16> = to_wide(&cmdline_str);
    let env_block = make_env_block(env_map);
    let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
    si.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
    si.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
    si.StartupInfo.lpDesktop = desktop.startup_info_desktop();
    let job = Arc::new(JobObject::create().context("create process job")?);

    let raw = RawConPty::new(/*cols*/ 80, /*rows*/ 24)?;
    let (pseudoconsole, input_write, output_read) = raw.into_handles();
    let hpc = pseudoconsole.raw_handle() as HANDLE;
    let conpty = ConptyInstance {
        pseudoconsole: Some(pseudoconsole),
        input_write: input_write.into_raw_handle() as HANDLE,
        output_read: output_read.into_raw_handle() as HANDLE,
        job: Some(Arc::clone(&job)),
        _desktop: Some(desktop),
    };
    let attributes: Result<ProcThreadAttributeList> = (|| {
        let preserve_app_context = crate::app_package::current_process_has_package_identity()?;
        let mut attrs = ProcThreadAttributeList::new(2 + u32::from(preserve_app_context))?;
        attrs.set_pseudoconsole(hpc)?;
        attrs.set_job(job.as_raw_handle() as HANDLE)?;
        if preserve_app_context { attrs.preserve_desktop_app_context()?; }
        Ok(attrs)
    })();
    with_conpty_launch_attributes(attributes, conpty, retain_on_launch_error, |mut attrs, conpty| {
        si.lpAttributeList = attrs.as_mut_ptr();

        let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            CreateProcessAsUserW(
                h_token,
                std::ptr::null(),
                cmdline.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                env_block.as_ptr() as *mut c_void,
                to_wide(cwd).as_ptr(),
                &si.StartupInfo,
                &mut pi,
            )
        };
        if ok == 0 {
            let err = unsafe { GetLastError() } as i32;
            let message = format!(
                "CreateProcessAsUserW failed: {} ({}) | cwd={} | cmd={} | env_u16_len={}",
                err,
                format_last_error(err),
                cwd.display(),
                cmdline_str,
                env_block.len()
            );
            let first_error = anyhow::Error::from(std::io::Error::from_raw_os_error(err)).context(message);
            if retain_on_launch_error { return Ok((pi, conpty, ProcessCreationEvidence::returned(ok, &pi, err), Some(first_error))); }
            return Err(first_error);
        }
        Ok((pi, conpty, ProcessCreationEvidence::returned(ok, &pi, 0), None))
    })
}

// This seam is also used by the regression: an attribute error cannot invoke
// `launch`, and the original allocated resources plus first error leave together.
type RetainedConptyLaunch = (PROCESS_INFORMATION, ConptyInstance, ProcessCreationEvidence, Option<anyhow::Error>);
fn with_conpty_launch_attributes(
    attributes: Result<ProcThreadAttributeList>,
    conpty: ConptyInstance,
    retain_on_launch_error: bool,
    launch: impl FnOnce(ProcThreadAttributeList, ConptyInstance) -> Result<RetainedConptyLaunch>,
) -> Result<RetainedConptyLaunch> {
    match attributes {
        Ok(attrs) => launch(attrs, conpty),
        Err(error) if retain_on_launch_error => Ok((unsafe { std::mem::zeroed() }, conpty,
            ProcessCreationEvidence::not_invoked(), Some(error))),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod retaining_launch_tests {
    use super::*;
    use std::os::windows::io::FromRawHandle;

    #[test]
    fn attribute_failure_retains_real_conpty_job_and_first_error_without_invoking_create() {
        let mut console = create_conpty(80, 24).expect("real ConPTY");
        let job = Arc::new(JobObject::create().expect("real empty Job"));
        console.job = Some(Arc::clone(&job));
        let first = anyhow::Error::from(std::io::Error::from_raw_os_error(87))
            .context("injected attribute setup failure before CreateProcessAsUserW");
        let (pi, mut retained, evidence, error) = with_conpty_launch_attributes(
            Err(first), console, true, |_, _| panic!("CreateProcess branch must remain unreachable"),
        ).expect("retain the original resources");
        let proof = evidence.not_created_for(&pi).expect("explicit not-invoked proof");
        assert_eq!(proof.rejection_os_error(), None);
        assert!(proof.reason().contains("not invoked"));
        let error = error.expect("original first error");
        assert_eq!(error.downcast_ref::<std::io::Error>().unwrap().raw_os_error(), Some(87));
        assert!(error.to_string().contains("attribute setup failure"));
        assert!(Arc::ptr_eq(&retained.job().unwrap(), &job));
        assert_eq!(job.query_accounting_and_process_ids().unwrap().1, 0);
        // Retire the SAME real endpoints and ConPTY. No process/thread close
        // witness can exist on this branch. No account/token/desktop is needed.
        let input = unsafe { std::fs::File::from_raw_handle(retained.take_input_write() as _) };
        let output = unsafe { std::fs::File::from_raw_handle(retained.take_output_read() as _) };
        drop(input);
        drop(retained);
        drop(output);
        assert_eq!(Arc::strong_count(&job), 1);
    }
}
