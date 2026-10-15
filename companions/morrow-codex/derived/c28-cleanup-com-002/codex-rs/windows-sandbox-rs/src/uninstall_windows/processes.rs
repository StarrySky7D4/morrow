//! Stops sandbox processes and drains logins except exact, pinned cleanup-owned tokens.

use std::ffi::c_void;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::FromRawHandle;
use std::os::windows::io::OwnedHandle;
use std::ptr::null_mut;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;
use windows_sys::Win32::Foundation::ERROR_NO_SUCH_LOGON_SESSION;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::LUID;
use windows_sys::Win32::Foundation::STATUS_SUCCESS;
use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::Foundation::WAIT_TIMEOUT;
use windows_sys::Win32::Security::Authentication::Identity::LsaEnumerateLogonSessions;
use windows_sys::Win32::Security::Authentication::Identity::LsaFreeReturnBuffer;
use windows_sys::Win32::Security::Authentication::Identity::LsaGetLogonSessionData;
use windows_sys::Win32::Security::Authentication::Identity::LsaNtStatusToWinError;
use windows_sys::Win32::Security::Authentication::Identity::SECURITY_LOGON_SESSION_DATA;
use windows_sys::Win32::Security::EqualSid;
use windows_sys::Win32::Security::TOKEN_QUERY;
use windows_sys::Win32::System::RemoteDesktop::WTS_CURRENT_SERVER_HANDLE;
use windows_sys::Win32::System::RemoteDesktop::WTS_PROCESS_INFOW;
use windows_sys::Win32::System::RemoteDesktop::WTSEnumerateProcessesW;
use windows_sys::Win32::System::RemoteDesktop::WTSFreeMemory;
use windows_sys::Win32::System::Threading::OpenProcess;
use windows_sys::Win32::System::Threading::OpenProcessToken;
use windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;
use windows_sys::Win32::System::Threading::PROCESS_SYNCHRONIZE;
use windows_sys::Win32::System::Threading::PROCESS_TERMINATE;
use windows_sys::Win32::System::Threading::TerminateProcess;
use windows_sys::Win32::System::Threading::WaitForSingleObject;

use super::principals::DisabledSandboxUsers;
use super::retained_logons::RetainedLogons;
use crate::token::get_user_sid_bytes;

pub(crate) const NORMAL_CLEANUP_PROCESS_ACCESS: u32 =
    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ObservedProcessCandidate {
    ExitedBeforeOpen { pid: u32 },
    OpenFailedUnconfirmed { pid: u32, raw_os_error: Option<i32> },
    TokenQueryFailed { pid: u32, signaled_exited: bool },
    ForeignProcessReuse { pid: u32 },
    SandboxProcess { pid: u32, signaled_exited: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NormalProcessAndLogonObservation {
    pub(crate) requested_access_mask: u32,
    pub(crate) retained_tokens_empty: bool,
    pub(crate) read_acl_mutex_active: bool,
    pub(crate) candidates: Vec<ObservedProcessCandidate>,
    pub(crate) blocking_logon: Option<String>,
}

pub(crate) fn verify_normal_process_and_logon_observation(
    observation: &NormalProcessAndLogonObservation,
) -> Result<()> {
    if (observation.requested_access_mask & PROCESS_TERMINATE) != 0 {
        bail!("normal cleanup must never request PROCESS_TERMINATE");
    }
    if observation.requested_access_mask != NORMAL_CLEANUP_PROCESS_ACCESS {
        bail!(
            "unexpected process access mask {:#x} in normal cleanup",
            observation.requested_access_mask
        );
    }
    if !observation.retained_tokens_empty {
        bail!("normal cleanup does not permit retained sandbox logon tokens");
    }
    if observation.read_acl_mutex_active {
        bail!("background read-ACL helper mutex is still active; refusing normal cleanup");
    }
    for candidate in &observation.candidates {
        match candidate {
            ObservedProcessCandidate::ExitedBeforeOpen { .. }
            | ObservedProcessCandidate::ForeignProcessReuse { .. } => {}
            ObservedProcessCandidate::OpenFailedUnconfirmed { pid, raw_os_error } => {
                bail!(
                    "cannot confirm exit of sandbox process pid={pid} (os_error={raw_os_error:?}); refusing normal cleanup"
                );
            }
            ObservedProcessCandidate::TokenQueryFailed {
                pid,
                signaled_exited: true,
            } => {
                let _ = pid;
            }
            ObservedProcessCandidate::TokenQueryFailed {
                pid,
                signaled_exited: false,
            } => {
                bail!(
                    "cannot inspect token of non-exited candidate process pid={pid}; refusing normal cleanup"
                );
            }
            ObservedProcessCandidate::SandboxProcess {
                pid,
                signaled_exited: true,
            } => {
                let _ = pid;
            }
            ObservedProcessCandidate::SandboxProcess {
                pid,
                signaled_exited: false,
            } => {
                bail!(
                    "active sandbox process pid={pid} is still running; normal cleanup forbids TerminateProcess"
                );
            }
        }
    }
    if let Some(blocker) = &observation.blocking_logon {
        bail!("remaining or unconfirmable sandbox logon session blocks normal cleanup: {blocker}");
    }
    Ok(())
}

/// Verifies that no sandbox processes, logon sessions, or read-ACL helpers remain active
/// without requesting `PROCESS_TERMINATE` or calling `TerminateProcess`.
pub(super) fn verify_no_running_processes_or_logons(
    users: &DisabledSandboxUsers,
    retained: &RetainedLogons,
) -> Result<()> {
    retained.verify_empty_for_normal_cleanup()?;
    let read_acl_mutex_active = crate::setup_provisioning::read_acl_mutex_exists()
        .context("check read-ACL helper mutex before normal cleanup")?;
    let candidates = observe_sandbox_processes_without_terminate(users)?;
    let blocking_logon = blocking_logon_session(users, retained)?;
    let observation = NormalProcessAndLogonObservation {
        requested_access_mask: NORMAL_CLEANUP_PROCESS_ACCESS,
        retained_tokens_empty: true,
        read_acl_mutex_active,
        candidates,
        blocking_logon,
    };
    verify_normal_process_and_logon_observation(&observation)
}

fn observe_sandbox_processes_without_terminate(
    users: &DisabledSandboxUsers,
) -> Result<Vec<ObservedProcessCandidate>> {
    let mut process_info = null_mut();
    let mut count = 0;
    if unsafe {
        WTSEnumerateProcessesW(
            WTS_CURRENT_SERVER_HANDLE,
            /*reserved*/ 0,
            /*version*/ 1,
            &mut process_info,
            &mut count,
        )
    } == 0
    {
        return Err(io::Error::last_os_error())
            .context("enumerate sandbox processes for normal cleanup");
    }
    let process_info = ProcessList(process_info);
    let mut candidates = Vec::new();
    if count == 0 {
        return Ok(candidates);
    }
    for process in unsafe { std::slice::from_raw_parts(process_info.0, count as usize) } {
        if !is_sandbox_user(users, process.pUserSid) {
            continue;
        }
        let pid = process.ProcessId;
        let handle = unsafe {
            OpenProcess(
                NORMAL_CLEANUP_PROCESS_ACCESS,
                /*binherithandle*/ 0,
                pid,
            )
        };
        if handle == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                candidates.push(ObservedProcessCandidate::ExitedBeforeOpen { pid });
            } else {
                candidates.push(ObservedProcessCandidate::OpenFailedUnconfirmed {
                    pid,
                    raw_os_error: error.raw_os_error(),
                });
            }
            continue;
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle as *mut c_void) };
        let mut token = 0;
        if unsafe { OpenProcessToken(handle.as_raw_handle() as HANDLE, TOKEN_QUERY, &mut token) }
            == 0
        {
            let signaled_exited = unsafe {
                WaitForSingleObject(handle.as_raw_handle() as HANDLE, /*dwmilliseconds*/ 0)
            } == WAIT_OBJECT_0;
            candidates.push(ObservedProcessCandidate::TokenQueryFailed {
                pid,
                signaled_exited,
            });
            continue;
        }
        let token = unsafe { OwnedHandle::from_raw_handle(token as *mut c_void) };
        let sid = unsafe { get_user_sid_bytes(token.as_raw_handle() as HANDLE) }?;
        if users.sids().any(|user_sid| user_sid == sid.as_slice()) {
            let signaled_exited = unsafe {
                WaitForSingleObject(handle.as_raw_handle() as HANDLE, /*dwmilliseconds*/ 0)
            } == WAIT_OBJECT_0;
            candidates.push(ObservedProcessCandidate::SandboxProcess {
                pid,
                signaled_exited,
            });
        } else {
            candidates.push(ObservedProcessCandidate::ForeignProcessReuse { pid });
        }
    }
    Ok(candidates)
}

pub(super) fn stop(users: &DisabledSandboxUsers, retained: &RetainedLogons) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let processes = sandbox_processes(users)?;
        for process in &processes {
            let handle = process.as_raw_handle() as HANDLE;
            if unsafe {
                TerminateProcess(handle, /*uexitcode*/ 1)
            } == 0
            {
                let error = io::Error::last_os_error();
                if unsafe {
                    WaitForSingleObject(handle, /*dwmilliseconds*/ 0)
                } != WAIT_OBJECT_0
                {
                    return Err(error).context("stop sandbox process before uninstall");
                }
            }
        }
        for process in &processes {
            let remaining = deadline.saturating_duration_since(Instant::now());
            // The five-second deadline keeps milliseconds within a DWORD.
            match unsafe {
                WaitForSingleObject(
                    process.as_raw_handle() as HANDLE,
                    remaining.as_millis() as u32,
                )
            } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT => bail!("sandbox process did not exit before the uninstall deadline"),
                _ => {
                    return Err(io::Error::last_os_error())
                        .context("wait for sandbox process exit");
                }
            }
        }
        drop(processes);

        // A token can outlive its process or exist before a runner starts. Account disable does
        // not revoke it, so check for remaining logins before removing protections. Only
        // explicitly pinned, private SYSTEM-finalizer tokens may remain; no process is exempt.
        let Some(blocker) = blocking_logon_session(users, retained)? else {
            return Ok(());
        };
        if Instant::now() >= deadline {
            bail!("sandbox cleanup blocked after the uninstall deadline: {blocker}");
        }
        // Repeat to catch descendants created during the previous snapshot.
        std::thread::sleep(
            Duration::from_millis(50).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

fn sandbox_processes(users: &DisabledSandboxUsers) -> Result<Vec<OwnedHandle>> {
    let mut process_info = null_mut();
    let mut count = 0;
    if unsafe {
        WTSEnumerateProcessesW(
            WTS_CURRENT_SERVER_HANDLE,
            /*reserved*/ 0,
            /*version*/ 1,
            &mut process_info,
            &mut count,
        )
    } == 0
    {
        return Err(io::Error::last_os_error())
            .context("enumerate sandbox processes for uninstall");
    }
    let process_info = ProcessList(process_info);
    let mut handles = Vec::new();
    if count == 0 {
        return Ok(handles);
    }
    for process in unsafe { std::slice::from_raw_parts(process_info.0, count as usize) } {
        if !is_sandbox_user(users, process.pUserSid) {
            continue;
        }
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
                /*binherithandle*/ 0,
                process.ProcessId,
            )
        };
        if handle == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                continue;
            }
            return Err(error).context("open sandbox process for uninstall");
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle as *mut c_void) };
        let mut token = 0;
        if unsafe { OpenProcessToken(handle.as_raw_handle() as HANDLE, TOKEN_QUERY, &mut token) }
            == 0
        {
            let error = io::Error::last_os_error();
            if unsafe {
                WaitForSingleObject(handle.as_raw_handle() as HANDLE, /*dwmilliseconds*/ 0)
            } == WAIT_OBJECT_0
            {
                continue;
            }
            return Err(error).context("identify sandbox process before uninstall");
        }
        let token = unsafe { OwnedHandle::from_raw_handle(token as *mut c_void) };
        let sid = unsafe { get_user_sid_bytes(token.as_raw_handle() as HANDLE) }?;
        // A PID can be reused after enumeration. Check the opened process before terminating it.
        if users.sids().any(|user_sid| user_sid == sid.as_slice()) {
            handles.push(handle);
        }
    }
    Ok(handles)
}

fn blocking_logon_session(
    users: &DisabledSandboxUsers,
    retained: &RetainedLogons,
) -> Result<Option<String>> {
    let mut count = 0;
    let mut logons = null_mut();
    let status = unsafe { LsaEnumerateLogonSessions(&mut count, &mut logons) };
    if status != STATUS_SUCCESS {
        bail!("enumerate sandbox logon sessions for uninstall: {status:#x}");
    }
    let logons = LsaBuffer(logons.cast());
    if count == 0 {
        return Ok(None);
    }
    for logon in unsafe { std::slice::from_raw_parts(logons.0.cast::<LUID>(), count as usize) } {
        // LocalSystem uses the reserved logon ID 0:0x3e7 and has no normal logon data.
        if logon.LowPart == 0x3e7 && logon.HighPart == 0 {
            continue;
        }
        let mut data = null_mut();
        let status = unsafe { LsaGetLogonSessionData(logon, &mut data) };
        if status != STATUS_SUCCESS {
            if unsafe { LsaNtStatusToWinError(status) } == ERROR_NO_SUCH_LOGON_SESSION {
                continue;
            }
            bail!("read sandbox logon session for uninstall: {status:#x}");
        }
        // Keep protections when LSA returns no session data.
        if data.is_null() {
            return Ok(Some(format!(
                "LSA returned no data for logon {:08x}:{:08x}",
                logon.HighPart, logon.LowPart
            )));
        }
        let data = LsaBuffer(data.cast());
        let data = unsafe { &*data.0.cast::<SECURITY_LOGON_SESSION_DATA>() };
        // LSA also lists sessions without a user SID, even before sandbox accounts exist.
        // Only sessions identified as sandbox users are evidence of remaining sandbox tokens.
        if is_sandbox_user(users, data.Sid) && !retained.contains(*logon) {
            return Ok(Some(format!(
                "sandbox logon {:08x}:{:08x} remains (logon type {}, session {})",
                logon.HighPart, logon.LowPart, data.LogonType, data.Session
            )));
        }
    }
    Ok(None)
}

fn is_sandbox_user(users: &DisabledSandboxUsers, sid: *mut c_void) -> bool {
    !sid.is_null()
        && users
            .sids()
            .any(|user_sid| unsafe { EqualSid(sid, user_sid.as_ptr() as *mut c_void) } != 0)
}

struct ProcessList(*mut WTS_PROCESS_INFOW);

impl Drop for ProcessList {
    fn drop(&mut self) {
        unsafe { WTSFreeMemory(self.0.cast()) };
    }
}

struct LsaBuffer(*mut c_void);

impl Drop for LsaBuffer {
    fn drop(&mut self) {
        unsafe { LsaFreeReturnBuffer(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::NORMAL_CLEANUP_PROCESS_ACCESS;
    use super::NormalProcessAndLogonObservation;
    use super::ObservedProcessCandidate;
    use super::PROCESS_TERMINATE;
    use super::verify_normal_process_and_logon_observation;

    fn clean_observation() -> NormalProcessAndLogonObservation {
        NormalProcessAndLogonObservation {
            requested_access_mask: NORMAL_CLEANUP_PROCESS_ACCESS,
            retained_tokens_empty: true,
            read_acl_mutex_active: false,
            candidates: vec![
                ObservedProcessCandidate::ExitedBeforeOpen { pid: 100 },
                ObservedProcessCandidate::SandboxProcess {
                    pid: 101,
                    signaled_exited: true,
                },
            ],
            blocking_logon: None,
        }
    }

    #[test]
    fn normal_cleanup_access_mask_excludes_process_terminate() {
        assert_eq!(NORMAL_CLEANUP_PROCESS_ACCESS & PROCESS_TERMINATE, 0);
        assert!(verify_normal_process_and_logon_observation(&clean_observation()).is_ok());

        let mut with_terminate = clean_observation();
        with_terminate.requested_access_mask |= PROCESS_TERMINATE;
        assert!(verify_normal_process_and_logon_observation(&with_terminate).is_err());
    }

    #[test]
    fn normal_cleanup_rejects_live_process_unconfirmed_open_active_logon_and_read_acl_mutex() {
        let mut live_proc = clean_observation();
        live_proc
            .candidates
            .push(ObservedProcessCandidate::SandboxProcess {
                pid: 4242,
                signaled_exited: false,
            });
        assert!(verify_normal_process_and_logon_observation(&live_proc).is_err());

        let mut unconfirmed_open = clean_observation();
        unconfirmed_open
            .candidates
            .push(ObservedProcessCandidate::OpenFailedUnconfirmed {
                pid: 4243,
                raw_os_error: Some(5),
            });
        assert!(verify_normal_process_and_logon_observation(&unconfirmed_open).is_err());

        let mut unconfirmed_token = clean_observation();
        unconfirmed_token
            .candidates
            .push(ObservedProcessCandidate::TokenQueryFailed {
                pid: 4244,
                signaled_exited: false,
            });
        assert!(verify_normal_process_and_logon_observation(&unconfirmed_token).is_err());

        let mut active_logon = clean_observation();
        active_logon.blocking_logon =
            Some("sandbox logon 00000000:00012345 remains".to_string());
        assert!(verify_normal_process_and_logon_observation(&active_logon).is_err());

        let mut active_mutex = clean_observation();
        active_mutex.read_acl_mutex_active = true;
        assert!(verify_normal_process_and_logon_observation(&active_mutex).is_err());

        let mut retained = clean_observation();
        retained.retained_tokens_empty = false;
        assert!(verify_normal_process_and_logon_observation(&retained).is_err());
    }
}
