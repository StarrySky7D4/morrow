//! Atomic Job-assigned process creation. No fallback to spawn-then-assign is permitted.

use std::{
    collections::BTreeMap,
    io,
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        io::{FromRawHandle, IntoRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    path::Path,
    process::ExitStatus,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::net::windows::named_pipe::NamedPipeServer;
use windows_sys::Win32::{
    Foundation::*,
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::*,
    System::{IO::*, JobObjects::IsProcessInJob, Pipes::*, Threading::*},
};

use super::{descriptor, job::Job, job_stdin::JobStdin, own, raw};

/// Distinguish failure before process creation from failures after kernel creation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobSpawnOutcome {
    NotCreated,
    CreatedExited,
    CleanupUnconfirmed,
}

#[derive(Debug)]
struct SpawnFailure {
    outcome: JobSpawnOutcome,
    detail: String,
}

impl std::fmt::Display for SpawnFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "atomic child {:?}: {}", self.outcome, self.detail)
    }
}

impl std::error::Error for SpawnFailure {}

/// Inspect an error returned directly by Job::spawn_suspended. This classification
/// is preserved inside io::Error, without parsing its human-readable text.
pub fn spawn_error_outcome(error: &io::Error) -> JobSpawnOutcome {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<SpawnFailure>())
        .map_or(JobSpawnOutcome::NotCreated, |failure| failure.outcome)
}

/// Child process created inside a Job before any child code can execute.
/// Dropping this value requests direct-child termination; it does not claim that
/// termination or pipe EOF was observed. The caller must retain its Job and wait.
pub struct JobChild {
    pub stdin: Option<JobStdin>,
    pub stdout: Option<NamedPipeServer>,
    pub stderr: Option<NamedPipeServer>,
    process: OwnedHandle,
    primary_thread: Option<OwnedHandle>,
    pid: u32,
    exit_status: Option<ExitStatus>,
}

impl JobChild {
    /// Original CreateProcess handle, before resume; no PID reopen is involved.
    pub fn creation_filetime(&self) -> io::Result<u64> {
        super::job::process_creation_filetime(&self.process)
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Resume the original CreateProcess primary thread exactly once.
    /// Any error requires caller kill/wait; this never retries a suspend count.
    pub fn resume(&mut self) -> io::Result<()> {
        let thread = self.primary_thread.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "primary thread resume already attempted",
            )
        })?;
        // SAFETY: CreateProcess supplied this owned primary-thread handle.
        match unsafe { ResumeThread(raw(&thread)) } {
            1 => Ok(()),
            u32::MAX => Err(io::Error::last_os_error()),
            _ => Err(invalid("primary thread previous suspend count was not one")),
        }
    }

    pub fn start_kill(&mut self) -> io::Result<()> {
        if self.try_wait()?.is_some() {
            return Ok(());
        }
        // SAFETY: process is the owned handle returned by CreateProcess.
        if unsafe { TerminateProcess(raw(&self.process), 1) } == 0 {
            let error = io::Error::last_os_error();
            // Process exit may race the termination request. Only its handle wait
            // can establish that this particular process has actually exited.
            if self.try_wait()?.is_none() {
                return Err(error);
            }
        }
        Ok(())
    }

    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.exit_status {
            return Ok(Some(status));
        }
        // SAFETY: process is owned and zero timeout does not block.
        match unsafe { WaitForSingleObject(raw(&self.process), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                // SAFETY: process is owned and code is writable DWORD storage.
                if unsafe { GetExitCodeProcess(raw(&self.process), &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                let status = ExitStatus::from_raw(code);
                self.exit_status = Some(status);
                Ok(Some(status))
            }
            _ => Err(invalid("unexpected child process wait result")),
        }
    }
}

impl Drop for JobChild {
    fn drop(&mut self) {
        let _ = self.start_kill();
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn input(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn set_inheritance(handle: &OwnedHandle, inherit: bool) -> io::Result<()> {
    // SAFETY: the owned handle remains valid and only its inheritance bit changes.
    if unsafe {
        SetHandleInformation(
            raw(handle),
            HANDLE_FLAG_INHERIT,
            if inherit { HANDLE_FLAG_INHERIT } else { 0 },
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut flags = 0;
    // SAFETY: handle is owned and flags is writable storage.
    if unsafe { GetHandleInformation(raw(handle), &mut flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if (flags & HANDLE_FLAG_INHERIT != 0) != inherit {
        return Err(invalid(
            "stdio/process handle inheritance readback mismatch",
        ));
    }
    Ok(())
}

fn path_wide(path: &Path) -> io::Result<Vec<u16>> {
    if !path.is_absolute() {
        return Err(input("executable and working directory must be absolute"));
    }
    let mut value: Vec<u16> = path.as_os_str().encode_wide().collect();
    if value.is_empty() || value.contains(&0) || value.len() >= 32767 {
        return Err(input("invalid executable or working-directory path"));
    }
    value.push(0);
    Ok(value)
}

// MSVCRT argument rules: quote each argument, double runs of backslashes before
// a quote or the terminating quote, and escape literal quotes with a backslash.
fn quote_argument(value: &[u16], command: &mut Vec<u16>) {
    command.push(b'"' as u16);
    let mut slashes = 0;
    for &unit in value {
        if unit == b'\\' as u16 {
            slashes += 1;
        } else {
            let count = if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            };
            command.extend(std::iter::repeat_n(b'\\' as u16, count));
            command.push(unit);
            slashes = 0;
        }
    }
    command.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    command.push(b'"' as u16);
}

fn command_line(exe: &[u16], args: &[String]) -> io::Result<Vec<u16>> {
    let mut command = Vec::new();
    quote_argument(&exe[..exe.len() - 1], &mut command);
    for arg in args {
        if arg.contains('\0') {
            return Err(input("command argument contains NUL"));
        }
        command.push(b' ' as u16);
        quote_argument(&arg.encode_utf16().collect::<Vec<_>>(), &mut command);
        if command.len() >= 32767 {
            return Err(input("Windows command line exceeds 32767 UTF-16 units"));
        }
    }
    command.push(0);
    if command.len() > 32767 {
        return Err(input("Windows command line exceeds 32767 UTF-16 units"));
    }
    Ok(command)
}

fn environment_block(environment: &BTreeMap<String, String>, trusted_host: bool) -> io::Result<Vec<u16>> {
    let mut entries = Vec::with_capacity(environment.len());
    for (key, value) in environment {
        if !matches!(key.as_str(), "SystemRoot" | "WINDIR" | "COMSPEC") && !(trusted_host && key == "LOCALAPPDATA") {
            return Err(input("environment key is outside the child allowlist"));
        }
        if key.is_empty() || key.contains(['=', '\0']) || value.contains('\0') {
            return Err(input("invalid environment key or value"));
        }
        entries.push((key.to_uppercase(), key, value));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    if entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(input("case-insensitive duplicate environment key"));
    }
    let mut block = Vec::new();
    for (_, key, value) in entries {
        block.extend(key.encode_utf16());
        block.push(b'=' as u16);
        block.extend(value.encode_utf16());
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct AttributeStorage([u8; 16]);

struct Attributes {
    storage: Vec<AttributeStorage>,
    initialized: bool,
    handles: Box<[HANDLE; 3]>,
    jobs: Box<[HANDLE; 1]>,
}

impl Attributes {
    fn new(handles: [HANDLE; 3], job: HANDLE) -> io::Result<Self> {
        let mut size = 0;
        // SAFETY: null storage requests the required attribute-list allocation size.
        let result = unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), 2, 0, &mut size) };
        let error = io::Error::last_os_error();
        if result != 0
            || error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
            || size == 0
            || size > 65536
        {
            return Err(invalid("could not size process attribute list"));
        }
        let mut list = Self {
            storage: vec![AttributeStorage([0; 16]); size.div_ceil(16)],
            initialized: false,
            handles: Box::new(handles),
            jobs: Box::new([job]),
        };
        // SAFETY: the storage is aligned and at least size bytes long, with exclusive access.
        if unsafe { InitializeProcThreadAttributeList(list.as_ptr(), 2, 0, &mut size) } == 0 {
            return Err(io::Error::last_os_error());
        }
        list.initialized = true;
        // SAFETY: both value arrays are pinned by Box and outlive DeleteAttributeList.
        // Job assignment is part of CreateProcess, while the Job itself is never inherited.
        if unsafe {
            UpdateProcThreadAttribute(
                list.as_ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                list.jobs.as_ptr().cast(),
                size_of::<[HANDLE; 1]>(),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: exactly these three inheritable stdio handles are listed; all other
        // process handles, including the anonymous Job, are excluded.
        if unsafe {
            UpdateProcThreadAttribute(
                list.as_ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                list.handles.as_ptr().cast(),
                size_of::<[HANDLE; 3]>(),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(list)
    }

    fn as_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: initialization succeeded and storage/value arrays are still live.
            unsafe { DeleteProcThreadAttributeList(self.as_ptr()) };
        }
    }
}

static PIPE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn stdio_pipe(parent_writes: bool) -> io::Result<(OwnedHandle, OwnedHandle)> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let sequence = PIPE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let locator = format!(
        r"\\.\pipe\morrow-m03-job-{}-{stamp}-{sequence}",
        std::process::id()
    );
    let name: Vec<u16> = locator.encode_utf16().chain(Some(0)).collect();
    let sd = descriptor()?;
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    // SAFETY: name/security are valid for the call; the returned server is owned.
    let server = own(unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            (if parent_writes {
                PIPE_ACCESS_OUTBOUND
            } else {
                PIPE_ACCESS_INBOUND
            }) | FILE_FLAG_OVERLAPPED
                | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            4096,
            0,
            &security,
        )
    })?;
    set_inheritance(&server, false)?;
    let child_security = SECURITY_ATTRIBUTES {
        bInheritHandle: 1,
        ..security
    };
    // SAFETY: opens a synchronous client end, compatible with ordinary child stdio.
    // Only this owned client handle is intentionally inheritable.
    let client = own(unsafe {
        CreateFileW(
            name.as_ptr(),
            if parent_writes {
                GENERIC_READ
            } else {
                GENERIC_WRITE
            },
            0,
            &child_security,
            OPEN_EXISTING,
            SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            ptr::null_mut(),
        )
    })?;
    set_inheritance(&client, true)?;
    // Connect after opening our own client: normally ERROR_PIPE_CONNECTED. Supply
    // valid stable OVERLAPPED/event storage even on that synchronous success path.
    let event = own(unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) })?;
    let mut operation = Box::new(OVERLAPPED {
        hEvent: raw(&event),
        ..Default::default()
    });
    // SAFETY: server is overlapped and the stable operation/event remain owned until reap.
    if unsafe { ConnectNamedPipe(raw(&server), operation.as_mut()) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_IO_PENDING as i32) {
            let mut transferred = 0;
            // SAFETY: wait reaps the one pending connect while its storage stays alive.
            if unsafe { GetOverlappedResult(raw(&server), operation.as_mut(), &mut transferred, 1) }
                == 0
            {
                let error = io::Error::last_os_error();
                if !matches!(error.raw_os_error(), Some(code) if code == ERROR_OPERATION_ABORTED as i32
                    || code == ERROR_BROKEN_PIPE as i32 || code == ERROR_PIPE_NOT_CONNECTED as i32)
                {
                    // An unexpected failure is not proof that the kernel released storage.
                    std::mem::forget(operation);
                    std::mem::forget(event);
                    std::mem::forget(server);
                }
                return Err(error);
            }
        } else if error.raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32) {
            return Err(error);
        }
    }
    Ok((server, client))
}

/// Read the current token's registered known folder; never trust an environment
/// override to choose the cross-process service-authority lock namespace.
fn trusted_local_app_data() -> io::Result<String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{UI::Shell::{SHGetKnownFolderPath,FOLDERID_LocalAppData},System::Com::CoTaskMemFree};
    let mut raw=ptr::null_mut();
    // SAFETY: known-folder ID and writable output pointer are valid. A null token
    // selects this process's existing token; this grants no new rights.
    let result=unsafe{SHGetKnownFolderPath(&FOLDERID_LocalAppData,0,ptr::null_mut(),&mut raw)};
    struct Folder(*mut u16);
    impl Drop for Folder {fn drop(&mut self){ // SAFETY: the API output is owned and must be freed on success or failure; null is accepted.
        unsafe{CoTaskMemFree(self.0.cast())};}}
    let folder=Folder(raw);
    if result<0 {return Err(io::Error::other(format!("Native local app data lookup failed: {result:#x}")));}
    if raw.is_null(){return Err(invalid("native local app data pointer"));}
    let mut value=Vec::new();
    // SAFETY: successful SHGetKnownFolderPath returns a null-terminated UTF-16
    // allocation owned by Folder; traversal stops at its terminator.
    for index in 0..32768 {let ch=unsafe{*folder.0.add(index)};if ch==0{break;}value.push(ch);}
    if value.is_empty()||value.len()==32768{return Err(invalid("native local app data length"));}
    let path=std::path::PathBuf::from(std::ffi::OsString::from_wide(&value));
    if !path.is_absolute()||!path.is_dir(){return Err(invalid("native local app data directory"));}
    Ok(path.to_str().ok_or_else(||invalid("native local app data encoding"))?.to_owned())
}
fn child_environment(environment:&BTreeMap<String,String>,trusted_host:bool)->io::Result<Vec<u16>> {
    // Validate the caller's same three-key allowlist before any augmentation.
    let ordinary=environment_block(environment,false)?;
    if !trusted_host{return Ok(ordinary);}
    let mut trusted=environment.clone();trusted.insert("LOCALAPPDATA".into(),trusted_local_app_data()?);
    environment_block(&trusted,true)
}

pub(crate) fn spawn(
    job: &Job,
    job_handle: &OwnedHandle,
    exe: &Path,
    args: &[String],
    cwd: &Path,
    environment: &BTreeMap<String, String>,
    trusted_host: bool,
) -> io::Result<JobChild> {
    let executable = path_wide(exe)?;
    let directory = path_wide(cwd)?;
    let mut command = command_line(&executable, args)?;
    let environment = child_environment(environment,trusted_host)?;
    // Revalidate immutable Job policy before preparing any child process.
    if job.inheritable()?
        || job.limit_flags()?
            != windows_sys::Win32::System::JobObjects::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    {
        return Err(invalid("Job policy changed before atomic process creation"));
    }
    // Every fallible reactor/pipe/attribute setup step precedes process creation.
    let (stdin, child_stdin) = stdio_pipe(true)?;
    let (stdout, child_stdout) = stdio_pipe(false)?;
    let (stderr, child_stderr) = stdio_pipe(false)?;
    let stdin = JobStdin::new(stdin);
    // SAFETY: native connects are completed/reaped and ownership is uniquely transferred
    // to Tokio. Only the read ends use Tokio's buffering; stdin uses explicit native reap.
    let stdout = unsafe { NamedPipeServer::from_raw_handle(stdout.into_raw_handle()) }?;
    let stderr = unsafe { NamedPipeServer::from_raw_handle(stderr.into_raw_handle()) }?;
    let handles = [raw(&child_stdin), raw(&child_stdout), raw(&child_stderr)];
    let mut attributes = Attributes::new(handles, raw(job_handle))?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.as_ptr();
    let mut information = PROCESS_INFORMATION::default();
    // SAFETY: all buffers and attribute value arrays remain live for this call;
    // command is writable and terminated; application name is fixed independently.
    // The kernel atomically assigns the new suspended process to the Job via JOB_LIST.
    if unsafe {
        CreateProcessW(
            executable.as_ptr(),
            command.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            CREATE_NO_WINDOW
                | CREATE_SUSPENDED
                | EXTENDED_STARTUPINFO_PRESENT
                | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ptr().cast(),
            directory.as_ptr(),
            &startup.StartupInfo,
            &mut information,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a successful CreateProcessW returns these two distinct, valid owned
    // handles. Neither is borrowed or ever exposed for external closing.
    let process = unsafe { OwnedHandle::from_raw_handle(information.hProcess) };
    let primary_thread = unsafe { OwnedHandle::from_raw_handle(information.hThread) };
    // Attribute pointers must remain valid until DeleteAttributeList, then the parent
    // closes all client ends immediately. The child owns only its inherited copies.
    drop(attributes);
    drop((child_stdin, child_stdout, child_stderr));
    let mut child = JobChild {
        stdin: Some(stdin),
        stdout: Some(stdout),
        stderr: Some(stderr),
        process,
        primary_thread: Some(primary_thread),
        pid: information.dwProcessId,
        exit_status: None,
    };
    let validation = (|| -> io::Result<()> {
        set_inheritance(&child.process, false)?;
        let thread = child.primary_thread.as_ref().unwrap();
        set_inheritance(thread, false)?;
        // SAFETY: CreateProcess returned both owned handles and their expected IDs.
        let pid = unsafe { GetProcessId(raw(&child.process)) };
        if pid == 0 {
            return Err(io::Error::last_os_error());
        }
        if pid != information.dwProcessId {
            return Err(invalid("created process PID mismatch"));
        }
        let owner = unsafe { GetProcessIdOfThread(raw(thread)) };
        if owner == 0 {
            return Err(io::Error::last_os_error());
        }
        if owner != pid {
            return Err(invalid("primary thread process identity mismatch"));
        }
        let tid = unsafe { GetThreadId(raw(thread)) };
        if tid == 0 {
            return Err(io::Error::last_os_error());
        }
        if tid != information.dwThreadId {
            return Err(invalid("created primary thread ID mismatch"));
        }
        let mut member = 0;
        // SAFETY: owned process/Job handles and writable member BOOL storage.
        if unsafe { IsProcessInJob(raw(&child.process), raw(job_handle), &mut member) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if member == 0 {
            return Err(invalid("atomic child Job membership not confirmed"));
        }
        Ok(())
    })();
    if let Err(error) = validation {
        let job_error = job.terminate().err();
        let kill_error = child.start_kill().err();
        // SAFETY: process remains owned during bounded synchronous cleanup wait.
        let wait = unsafe { WaitForSingleObject(raw(&child.process), 2000) };
        if wait != WAIT_OBJECT_0 {
            // Retain all handles/reactor objects rather than claiming successful cleanup.
            std::mem::forget(child);
            return Err(io::Error::other(SpawnFailure {
                outcome: JobSpawnOutcome::CleanupUnconfirmed,
                detail: format!("{error}; job={job_error:?}; kill={kill_error:?}; wait={wait}"),
            }));
        }
        // Prevent Drop from issuing another request after the real exit was observed.
        let _ = child.try_wait();
        return Err(io::Error::new(
            error.kind(),
            SpawnFailure {
                outcome: JobSpawnOutcome::CreatedExited,
                detail: error.to_string(),
            },
        ));
    }
    Ok(child)
}

#[cfg(test)]
mod trusted_environment_tests {
 use super::*;
 #[test]
 fn guest_allowlist_stays_closed_and_host_folder_is_kernel_selected() {
  let empty=BTreeMap::new();let guest=child_environment(&empty,false).unwrap();
  assert_eq!(guest,vec![0,0]);
  let host=child_environment(&empty,true).unwrap();let text=String::from_utf16_lossy(&host);
  assert!(text.starts_with("LOCALAPPDATA="));assert!(text.ends_with("\0\0"));
  let mut override_env=BTreeMap::new();override_env.insert("LOCALAPPDATA".into(),"C:\\untrusted-namespace".into());
  assert!(child_environment(&override_env,false).is_err());assert!(child_environment(&override_env,true).is_err());
  assert!(std::path::Path::new(&trusted_local_app_data().unwrap()).is_dir());
 }
}
