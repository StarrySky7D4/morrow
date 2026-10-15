//! Starts provisioning helpers only after their retained directory handles are installed.
//! The child owns its duplicates until exit, independently of the service lifetime.

use std::io;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::BorrowedHandle;
use std::os::windows::process::CommandExt;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use windows_sys::Win32::Foundation::DUPLICATE_SAME_ACCESS;
use windows_sys::Win32::Foundation::DuplicateHandle;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::NTSTATUS;
use windows_sys::Win32::Foundation::RtlNtStatusToDosError;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;
use windows_sys::Win32::System::Threading::GetCurrentProcess;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process_handle: HANDLE) -> NTSTATUS;
}

pub(super) fn spawn_with_retained_handles(
    command: &mut Command,
    retained_handles: &[BorrowedHandle<'_>],
) -> io::Result<Child> {
    let mut child = command
        .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let initialized = (|| {
        for handle in retained_handles {
            let mut child_handle = 0;
            if unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    handle.as_raw_handle() as HANDLE,
                    child.as_raw_handle() as HANDLE,
                    &mut child_handle,
                    /*dwdesiredaccess*/ 0,
                    /*binherithandle*/ 0,
                    DUPLICATE_SAME_ACCESS,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            // This value belongs to the child's handle table. Do not close it here
            // or make it inheritable by unrelated processes or helper descendants.
        }
        let status = unsafe { NtResumeProcess(child.as_raw_handle() as HANDLE) };
        if status < 0 {
            return Err(io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(status) } as i32,
            ));
        }
        Ok(())
    })();
    if let Err(error) = initialized {
        // A partially initialized helper must never run after its parent drops
        // the directory protections. Reap it, including any installed duplicates.
        child.kill()?;
        child.wait()?;
        return Err(error);
    }
    Ok(child)
}

pub(super) struct SpawnedOwnedSetupChild {
    pub child: Child,
    pub child_pid: u32,
    pub stdout: std::process::ChildStdout,
    pub stderr: std::process::ChildStderr,
}

fn query_process_image_path(process_handle: HANDLE) -> io::Result<std::path::PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Threading::QueryFullProcessImageNameW;

    let mut buffer = vec![0u16; 32768];
    let mut size = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(process_handle, 0, buffer.as_mut_ptr(), &mut size)
    };
    if ok == 0 || size == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(std::path::PathBuf::from(OsString::from_wide(
        &buffer[..size as usize],
    )))
}

pub(super) fn spawn_owned_setup_with_retained_handles_and_pipes(
    command: &mut Command,
    matched_setup_exe: &crate::checked_runner::MatchedRunnerArtifact,
    retained_handles: &[BorrowedHandle<'_>],
) -> io::Result<SpawnedOwnedSetupChild> {
    let mut child = command
        .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let initialized = (|| {
        let child_pid = child.id();
        if child_pid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "spawned setup helper reported zero PID",
            ));
        }
        let live_image_path = query_process_image_path(child.as_raw_handle() as HANDLE)?;
        matched_setup_exe
            .verify_resolved(&live_image_path)
            .map_err(|err| io::Error::new(io::ErrorKind::PermissionDenied, err.to_string()))?;
        for handle in retained_handles {
            let mut child_handle = 0;
            if unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    handle.as_raw_handle() as HANDLE,
                    child.as_raw_handle() as HANDLE,
                    &mut child_handle,
                    /*dwdesiredaccess*/ 0,
                    /*binherithandle*/ 0,
                    DUPLICATE_SAME_ACCESS,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        let stdout = child.stdout.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "missing child stdout pipe")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "missing child stderr pipe")
        })?;
        let status = unsafe { NtResumeProcess(child.as_raw_handle() as HANDLE) };
        if status < 0 {
            return Err(io::Error::from_raw_os_error(
                unsafe { RtlNtStatusToDosError(status) } as i32,
            ));
        }
        Ok((child_pid, stdout, stderr))
    })();
    match initialized {
        Ok((child_pid, stdout, stderr)) => Ok(SpawnedOwnedSetupChild {
            child,
            child_pid,
            stdout,
            stderr,
        }),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

#[cfg(test)]
#[path = "setup_launch_tests.rs"]
mod tests;
