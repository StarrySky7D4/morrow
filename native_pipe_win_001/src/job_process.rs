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
pub(crate) struct AttributeStorage([u8; 16]);

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

// Shared current-primary-token candidate; namespace qualification remains OPEN.
// No caller environment value or fallback participates in this resolver.
fn trusted_local_app_data() -> io::Result<String> {
    morrow_windows_user_directories::current_process_local_app_data()
}
// Preserve the existing pure decoder regression names; never calls the native resolver.
#[cfg(test)]
use morrow_windows_user_directories::decode_token_local_app_data;

/// An already encoded trusted-host environment. Private fields prevent caller
/// blocks/profile overrides/markers. Not Clone: moved into one spawn attempt.
pub struct PreparedTrustedHostEnvironment {
    encoded: Vec<u16>,
}
impl PreparedTrustedHostEnvironment {
    fn into_block(self) -> Vec<u16> { self.encoded }
}

/// Prepare the current-process token-derived candidate before profile/Owner/lease/Job effects.
/// Namespace qualification remains OPEN; this one-shot prepared block grants no new authority.
pub fn prepare_trusted_host_environment(environment: &BTreeMap<String, String>)
    -> io::Result<PreparedTrustedHostEnvironment> {
    prepare_trusted_host_environment_with(environment, trusted_local_app_data)
}
pub(crate) fn prepare_trusted_host_environment_with(environment: &BTreeMap<String, String>,
    resolve: impl FnOnce() -> io::Result<String>) -> io::Result<PreparedTrustedHostEnvironment> {
    environment_block(environment, false)?;
    let mut trusted = environment.clone();
    trusted.insert("LOCALAPPDATA".into(), resolve()?);
    Ok(PreparedTrustedHostEnvironment { encoded: environment_block(&trusted, true)? })
}

#[cfg(feature = "c28-product-session-qualification")]
pub fn prepare_qualified_host_environment(environment: &BTreeMap<String, String>,
    plan_digest: [u8; 32]) -> io::Result<PreparedTrustedHostEnvironment> {
    prepare_qualified_host_environment_with(environment, &plan_digest, trusted_local_app_data)
}
#[cfg(feature = "c28-product-session-qualification")]
fn prepare_qualified_host_environment_with(environment: &BTreeMap<String, String>,
    plan_digest: &[u8; 32], resolve: impl FnOnce() -> io::Result<String>)
    -> io::Result<PreparedTrustedHostEnvironment> {
    environment_block(environment, false)?;
    let native_folder = resolve()?;
    Ok(PreparedTrustedHostEnvironment {
        encoded: qualified_environment_block(environment, &native_folder, plan_digest)?,
    })
}

fn child_environment(environment:&BTreeMap<String,String>,trusted_host:bool)->io::Result<Vec<u16>> {
    // Validate the caller's same three-key allowlist before any augmentation.
    let ordinary=environment_block(environment,false)?;
    if !trusted_host{return Ok(ordinary);}
    Ok(prepare_trusted_host_environment(environment)?.into_block())
}

// Internal qualified-host injection; neither ordinary caller allowlist changes.
#[cfg(feature = "c28-product-session-qualification")]
const QUALIFICATION_ENV: &str = "MORROW_C28_SESSION_QUALIFICATION";

#[cfg(feature = "c28-product-session-qualification")]
fn qualification_digest_hex(digest: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(64);
    for byte in digest {
        value.push(HEX[(byte >> 4) as usize] as char);
        value.push(HEX[(byte & 15) as usize] as char);
    }
    value
}

// Pure encoder; native_folder comes only from the current-process token-derived candidate.
#[cfg(feature = "c28-product-session-qualification")]
fn qualified_environment_block(environment: &BTreeMap<String, String>,
    native_folder: &str, plan_digest: &[u8; 32]) -> io::Result<Vec<u16>> {
    environment_block(environment, false)?;
    let mut trusted = environment.clone();
    trusted.insert("LOCALAPPDATA".into(), native_folder.into());
    environment_block(&trusted, true)?;
    trusted.insert(QUALIFICATION_ENV.into(), qualification_digest_hex(plan_digest));
    let mut entries: Vec<_> = trusted.iter().map(|(key, value)|
        (key.to_uppercase(), key, value)).collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut block = Vec::new();
    for (_, key, value) in entries {
        block.extend(key.encode_utf16());
        block.push(b'=' as u16);
        block.extend(value.encode_utf16());
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

#[cfg(feature = "c28-product-session-qualification")]
fn qualified_child_environment(environment: &BTreeMap<String, String>,
    plan_digest: &[u8; 32]) -> io::Result<Vec<u16>> {
    Ok(prepare_qualified_host_environment(environment, *plan_digest)?.into_block())
}

#[cfg(feature = "c28-product-session-qualification")]
pub(crate) fn spawn_qualified_host(job: &Job, job_handle: &OwnedHandle,
    exe: &Path, args: &[String], cwd: &Path,
    environment: &BTreeMap<String, String>, plan_digest: [u8; 32]) -> io::Result<JobChild> {
    spawn_inner(job, job_handle, exe, args, cwd, environment, true, Some(plan_digest))
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
    spawn_inner(job, job_handle, exe, args, cwd, environment, trusted_host, None)
}

pub(crate) fn spawn_prepared_host(job: &Job, job_handle: &OwnedHandle,
    exe: &Path, args: &[String], cwd: &Path, prepared: PreparedTrustedHostEnvironment)
    -> io::Result<JobChild> {
    let executable = path_wide(exe)?;
    let directory = path_wide(cwd)?;
    let command = command_line(&executable, args)?;
    spawn_encoded(job, job_handle, executable, directory, command, prepared.into_block())
}

fn spawn_inner(job: &Job, job_handle: &OwnedHandle, exe: &Path, args: &[String],
    cwd: &Path, environment: &BTreeMap<String, String>, trusted_host: bool,
    qualification_digest: Option<[u8; 32]>) -> io::Result<JobChild> {
    let executable = path_wide(exe)?;
    let directory = path_wide(cwd)?;
    let command = command_line(&executable, args)?;
    let environment = match qualification_digest {
        None => child_environment(environment, trusted_host)?,
        Some(digest) => {
            #[cfg(feature = "c28-product-session-qualification")]
            {
                if !trusted_host { return Err(input("qualification requires trusted host")); }
                qualified_child_environment(environment, &digest)?
            }
            #[cfg(not(feature = "c28-product-session-qualification"))]
            {
                let _ = digest;
                return Err(input("qualification host feature disabled"));
            }
        }
    };
    spawn_encoded(job, job_handle, executable, directory, command, environment)
}

fn spawn_encoded(job: &Job, job_handle: &OwnedHandle, executable: Vec<u16>,
    directory: Vec<u16>, mut command: Vec<u16>, environment: Vec<u16>) -> io::Result<JobChild> {
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

// Pure tests: no KnownFolder, FFI, process, profile, DB or VM call.
#[cfg(test)]
mod qualification_environment_policy_tests {
    use super::*;
    const MARKER: &str = "MORROW_C28_SESSION_QUALIFICATION";
    fn caller(key: &str, value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(key.into(), value.into())])
    }
    #[test]
    fn ordinary_paths_reject_marker_and_folder_overrides() {
        for key in [MARKER, "morrow_c28_session_qualification", "LOCALAPPDATA",
            "localappdata", "PATH", "SYSTEMROOT", "SystemRoot=bad"] {
            let env = caller(key, "0");
            assert!(environment_block(&env, false).is_err());
            assert!(child_environment(&env, true).is_err()); // Before OS lookup.
        }
    }
    #[test]
    fn ordinary_encoder_rejects_invalid_values_and_unknown_keys() {
        assert!(environment_block(&caller("SystemRoot", "bad\0value"), false).is_err());
        for key in ["", "OTHER", "SystemRoot\0", "SystemRoot="] {
            assert!(environment_block(&caller(key, "public"), false).is_err());
        }
        let valid = BTreeMap::from([("SystemRoot".into(), "C:/Windows".into()),
            ("WINDIR".into(), "C:/Windows".into()), ("COMSPEC".into(), "C:/Windows/System32/cmd.exe".into())]);
        let text = String::from_utf16(&environment_block(&valid, false).unwrap()).unwrap();
        assert_eq!(text, "COMSPEC=C:/Windows/System32/cmd.exe\0SystemRoot=C:/Windows\0WINDIR=C:/Windows\0\0");
    }
    #[cfg(not(feature = "c28-product-session-qualification"))]
    #[test]
    fn default_off_trusted_encoder_rejects_marker() {
        assert!(environment_block(&caller(MARKER, &"a".repeat(64)), true).is_err());
    }
    #[cfg(feature = "c28-product-session-qualification")]
    #[test]
    fn typed_digest_has_exact_fixed_width_lower_hex() {
        let mut digest = [0u8; 32]; digest[0] = 0xab; digest[31] = 0xef;
        let value = qualification_digest_hex(&digest);
        assert_eq!(value.len(), 64);
        assert_eq!(value, format!("ab{}ef", "00".repeat(30)));
        assert!(value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }
    #[cfg(feature = "c28-product-session-qualification")]
    #[test]
    fn qualified_encoder_injects_only_typed_digest_and_native_folder() {
        let raw = qualified_environment_block(&caller("SystemRoot", "C:/Windows"),
            "C:/native-known-folder", &[0xab;32]).unwrap();
        let text = String::from_utf16(&raw).unwrap();
        assert_eq!(text, format!("LOCALAPPDATA=C:/native-known-folder\0{MARKER}={}\0SystemRoot=C:/Windows\0\0", "ab".repeat(32)));
        assert!(environment_block(&caller(MARKER, &"ab".repeat(32)), true).is_err());
    }
    #[cfg(feature = "c28-product-session-qualification")]
    #[test]
    fn qualified_encoder_rejects_caller_marker_formats_and_overrides() {
        let values = [String::new(), "a".into(), "a".repeat(63), "a".repeat(65),
            "A".repeat(64), "g".repeat(64), "a".repeat(64)];
        for value in &values {
            assert!(qualified_environment_block(&caller(MARKER, value), "C:/native", &[1;32]).is_err());
        }
        for key in ["LOCALAPPDATA", "localappdata", "PATH", "morrow_c28_session_qualification", "SYSTEMROOT"] {
            assert!(qualified_environment_block(&caller(key,"public"), "C:/native", &[1;32]).is_err());
        }
        assert!(qualified_environment_block(&caller("SystemRoot","bad\0value"), "C:/native", &[1;32]).is_err());
        assert!(qualified_environment_block(&BTreeMap::new(), "bad\0folder", &[1;32]).is_err());
    }
}

// Pure injected-resolver tests: no KnownFolder, FFI, Job, profile or process.
#[cfg(test)]
mod prepared_environment_policy_tests {
    use super::*;
    use std::cell::Cell;
    fn caller(key: &str, value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(key.into(), value.into())])
    }
    #[test]
    fn resolver_failure_returns_once_without_prepared_value_or_fallback() {
        let calls = Cell::new(0);
        let result = prepare_trusted_host_environment_with(&BTreeMap::new(), || {
            calls.set(calls.get() + 1);
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected resolver failure"))
        });
        assert!(matches!(result, Err(ref error) if error.kind() == io::ErrorKind::PermissionDenied));
        assert_eq!(calls.get(), 1);
    }
    #[test]
    fn caller_overrides_and_guest_keys_reject_before_resolver() {
        for key in ["LOCALAPPDATA", "localappdata", "MORROW_C28_SESSION_QUALIFICATION",
            "morrow_c28_session_qualification", "PATH", "SYSTEMROOT"] {
            let calls = Cell::new(0);
            let env = caller(key, "public");
            assert!(environment_block(&env, false).is_err());
            assert!(prepare_trusted_host_environment_with(&env, || {
                calls.set(calls.get() + 1); Ok("C:/synthetic-only".into())
            }).is_err());
            assert_eq!(calls.get(), 0);
        }
        assert_eq!(child_environment(&BTreeMap::new(), false).unwrap(), vec![0, 0]);
    }
    #[test]
    fn owned_prepared_block_consumes_exact_bytes_without_second_lookup() {
        let calls = Cell::new(0);
        let mut env = caller("SystemRoot", "C:/Windows");
        let prepared = prepare_trusted_host_environment_with(&env, || {
            calls.set(calls.get() + 1); Ok("C:/synthetic-only".into())
        }).unwrap();
        env.insert("LOCALAPPDATA".into(), "C:/later-caller-override".into());
        let raw = prepared.into_block(); // Same consume as spawn_prepared_host; no resolver argument.
        assert_eq!(String::from_utf16(&raw).unwrap(),
            "LOCALAPPDATA=C:/synthetic-only\0SystemRoot=C:/Windows\0\0");
        assert_eq!(calls.get(), 1);
    }
    #[test]
    fn invalid_resolver_value_is_rejected_without_fallback() {
        let calls = Cell::new(0);
        assert!(prepare_trusted_host_environment_with(&BTreeMap::new(), || {
            calls.set(calls.get() + 1); Ok("bad\0folder".into())
        }).is_err());
        assert_eq!(calls.get(), 1);
    }
    #[cfg(feature = "c28-product-session-qualification")]
    #[test]
    fn qualified_resolver_failure_returns_once_without_fallback() {
        let calls = Cell::new(0);
        let result = prepare_qualified_host_environment_with(&BTreeMap::new(), &[1; 32], || {
            calls.set(calls.get() + 1);
            Err(io::Error::new(io::ErrorKind::NotFound, "injected resolver failure"))
        });
        assert!(matches!(result, Err(ref error) if error.kind() == io::ErrorKind::NotFound));
        assert_eq!(calls.get(), 1);
    }
    #[cfg(feature = "c28-product-session-qualification")]
    #[test]
    fn qualified_prepared_block_preserves_digest_and_caller_admission() {
        let calls = Cell::new(0);
        let env = caller("SystemRoot", "C:/Windows");
        let prepared = prepare_qualified_host_environment_with(&env, &[0xab; 32], || {
            calls.set(calls.get() + 1); Ok("C:/synthetic-only".into())
        }).unwrap();
        assert_eq!(String::from_utf16(&prepared.into_block()).unwrap(), format!(
            "LOCALAPPDATA=C:/synthetic-only\0MORROW_C28_SESSION_QUALIFICATION={}\0SystemRoot=C:/Windows\0\0", "ab".repeat(32)));
        assert_eq!(calls.get(), 1);
        for key in ["LOCALAPPDATA", "MORROW_C28_SESSION_QUALIFICATION"] {
            assert!(prepare_qualified_host_environment_with(&caller(key, "public"), &[1; 32], || {
                panic!("rejected caller must not reach resolver")
            }).is_err());
        }
    }
}

// Pure UTF-16/path-shape tests: no token/userenv/FS/worker/VM/body call.
#[cfg(test)]
mod token_folder_decoder_tests {
    use super::decode_token_local_app_data;
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    #[test]
    fn rejects_empty_missing_nul_and_oversized_buffers() {
        assert!(decode_token_local_app_data(&[]).is_err());
        assert!(decode_token_local_app_data(&[0]).is_err());
        assert!(decode_token_local_app_data(&vec![b'x' as u16; 32768]).is_err());
        assert!(decode_token_local_app_data(&vec![0; 32769]).is_err());
    }
    #[test]
    fn rejects_invalid_utf16_before_terminator() {
        assert!(decode_token_local_app_data(&[0xd800, 0]).is_err());
        assert!(decode_token_local_app_data(&[0xdc00, 0]).is_err());
    }
    #[test]
    fn rejects_fixed_unresolved_placeholder_and_relative_paths() {
        for value in ["%LOCALAPPDATA%", "%localappdata%", "relative\\Local", "C:relative"] {
            assert!(decode_token_local_app_data(&wide(value)).is_err());
        }
    }
    #[test]
    fn accepts_absolute_literal_percent_and_unicode_without_filesystem_calls() {
        for value in [r"C:\Profiles\100%Ready\AppData\Local",
                      r"C:\Profiles\%literal%\Local", r"C:\Profiles\研究\Local"] {
            assert_eq!(decode_token_local_app_data(&wide(value)).unwrap(), value);
        }
    }
    #[test]
    fn terminator_must_fit_inside_fixed_capacity() {
        let mut value = wide(r"C:\Local");
        value.resize(32768, 0);
        assert_eq!(decode_token_local_app_data(&value).unwrap(), r"C:\Local");
        let mut no_end = vec![b'x' as u16; 32768];
        no_end[..3].copy_from_slice(&[b'C' as u16, b':' as u16, b'\\' as u16]);
        assert!(decode_token_local_app_data(&no_end).is_err());
    }
}

// ============================================================================
// Opt-in Retaining Job-Assigned Process Creation, Observation, and Retirement
// ============================================================================

use std::{
    marker::PhantomData,
    os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle},
};
use windows_sys::Win32::System::JobObjects::{
    JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject,
};

/// Maximum explicit inherited handles allowed in `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`.
pub const MAX_RETAINED_INHERITED_HANDLES: usize = 32;
/// Fixed count of standard I/O handles (`stdin`, `stdout`, `stderr`).
pub const STDIO_HANDLE_COUNT: usize = 3;
/// Fixed count of Job handles in `PROC_THREAD_ATTRIBUTE_JOB_LIST`.
pub const JOB_LIST_HANDLE_COUNT: usize = 1;
/// Finite budget for `CreateProcessW` per `RetainedJobOwner`.
pub const MAX_CREATE_PROCESS_CALLS: u8 = 1;
/// Finite budget for post-`CreateProcessW` validation per `RetainedJobOwner`.
pub const MAX_POST_CREATE_VALIDATION_CALLS: u8 = 1;
/// Finite budget for primary-thread `ResumeThread` per `RetainedJobOwner`.
pub const MAX_RESUME_THREAD_CALLS: u8 = 1;
/// Finite budget for `CloseHandle` per individual handle slot.
pub const MAX_CLOSE_CALLS_PER_HANDLE: u8 = 1;
/// Finite budget for narrow original-handle borrow operations per `RetainedJobOwner`.
pub const MAX_ORIGINAL_HANDLE_BORROWS: u32 = 32;
/// Default finite poll budget for non-blocking wait/accounting observations.
pub const DEFAULT_MAX_OBSERVATION_POLLS: u32 = 4096;
/// Upper bound on configurable non-blocking observation polls.
pub const MAX_OBSERVATION_POLLS_LIMIT: u32 = 65536;
/// Maximum UTF-16 units in a Windows command line (including terminating NUL).
pub const MAX_COMMAND_LINE_UTF16_UNITS: usize = 32767;
/// Maximum UTF-16 units in an absolute executable or working directory path.
pub const MAX_PATH_UTF16_UNITS: usize = 32767;
/// Maximum UTF-16 units in a prepared environment block.
pub const MAX_ENV_BLOCK_UTF16_UNITS: usize = 131072;
/// Maximum bytes for `LPPROC_THREAD_ATTRIBUTE_LIST` allocation.
pub const MAX_ATTRIBUTE_LIST_BYTES: usize = 65536;

#[cfg(test)]
impl PreparedTrustedHostEnvironment {
    pub(crate) fn from_raw_block_for_test(encoded: Vec<u16>) -> Self {
        Self { encoded }
    }
}

/// Validated, immutable UTF-16 launch buffers prepared before native creation.
#[derive(Debug)]
pub struct PreparedChildLaunch {
    executable: Vec<u16>,
    directory: Vec<u16>,
    command: Vec<u16>,
    environment: Vec<u16>,
}

impl PreparedChildLaunch {
    /// Validate and encode executable path, working directory, MSVCRT command line,
    /// and consume the one-shot `PreparedTrustedHostEnvironment`.
    pub fn prepare(
        exe: &Path,
        args: &[String],
        cwd: &Path,
        prepared: PreparedTrustedHostEnvironment,
    ) -> io::Result<Self> {
        let executable = path_wide(exe)?;
        let directory = path_wide(cwd)?;
        let command = command_line(&executable, args)?;
        let environment = prepared.into_block();
        validate_encoded_environment_block(&environment)?;
        Ok(Self {
            executable,
            directory,
            command,
            environment,
        })
    }

    pub fn executable_wide(&self) -> &[u16] {
        &self.executable
    }

    pub fn directory_wide(&self) -> &[u16] {
        &self.directory
    }

    pub fn command_wide(&self) -> &[u16] {
        &self.command
    }

    pub fn environment_wide(&self) -> &[u16] {
        &self.environment
    }
}

pub(crate) fn validate_encoded_environment_block(block: &[u16]) -> io::Result<()> {
    if block.len() < 2 || block.len() > MAX_ENV_BLOCK_UTF16_UNITS {
        return Err(input("prepared environment block size is out of bounds"));
    }
    if block[block.len() - 1] != 0 || block[block.len() - 2] != 0 {
        return Err(input(
            "prepared environment block is missing double-NUL terminator",
        ));
    }
    if block == [0, 0] {
        return Ok(());
    }
    let body = &block[..block.len() - 2];
    for entry in body.split(|&unit| unit == 0) {
        if entry.is_empty() {
            return Err(input("prepared environment block contains empty entry"));
        }
        if char::decode_utf16(entry.iter().copied()).any(|res| res.is_err()) {
            return Err(input("prepared environment entry contains invalid UTF-16"));
        }
        let Some(eq_pos) = entry.iter().position(|&unit| unit == b'=' as u16) else {
            return Err(input("prepared environment entry is missing '=' separator"));
        };
        if eq_pos == 0 {
            return Err(input("prepared environment entry has empty key"));
        }
    }
    Ok(())
}

/// Set and read back the `HANDLE_FLAG_INHERIT` bit on a caller-owned handle.
pub fn set_owned_handle_inheritable(handle: &OwnedHandle, inherit: bool) -> io::Result<()> {
    set_inheritance(handle, inherit)
}

/// Set and read back the `HANDLE_FLAG_INHERIT` bit on a borrowed handle.
pub fn set_borrowed_handle_inheritable(
    handle: BorrowedHandle<'_>,
    inherit: bool,
) -> io::Result<()> {
    RealRetainedNativeOps.set_handle_inheritable(handle.as_raw_handle(), inherit)
}

/// Explicit state of a fixed handle slot inside `RetainedJobOwner`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedHandleState {
    /// No handle has been installed or captured in this slot.
    Empty,
    /// A valid original handle is open and retained in this slot.
    Open(isize),
    /// A raw output value (such as a negative pseudo-handle or `INVALID_HANDLE_VALUE`)
    /// was captured from a native call and is retained as uncloseable raw debt.
    InvalidCaptured(isize),
    /// `CloseHandle` was recorded in-flight before the native call and has not completed.
    /// Ambiguous handles in this state are never retried or claimed closed.
    CloseInFlight(isize),
    /// `CloseHandle` returned `0`; the original raw value and OS error are retained.
    /// Failed closes are never retried or claimed closed.
    CloseFailed { raw: isize, error_code: u32 },
    /// `CloseHandle` succeeded once on the original handle.
    Closed { former_raw: isize },
}

impl RetainedHandleState {
    pub fn is_open(self) -> bool {
        matches!(self, Self::Open(_))
    }

    pub fn is_closed_cleanly(self) -> bool {
        matches!(self, Self::Closed { .. })
    }

    pub fn has_unresolved_debt(self) -> bool {
        matches!(
            self,
            Self::Open(_)
                | Self::InvalidCaptured(_)
                | Self::CloseInFlight(_)
                | Self::CloseFailed { .. }
        )
    }

    pub fn raw_value(self) -> Option<isize> {
        match self {
            Self::Empty => None,
            Self::Open(raw)
            | Self::InvalidCaptured(raw)
            | Self::CloseInFlight(raw)
            | Self::CloseFailed { raw, .. }
            | Self::Closed { former_raw: raw } => Some(raw),
        }
    }
}

/// Fixed-budget slot for a single kernel handle owned by `RetainedJobOwner`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedHandleSlot {
    state: RetainedHandleState,
    close_attempts: u8,
}

impl Default for RetainedHandleSlot {
    fn default() -> Self {
        Self::empty()
    }
}

impl RetainedHandleSlot {
    pub const fn empty() -> Self {
        Self {
            state: RetainedHandleState::Empty,
            close_attempts: 0,
        }
    }

    pub(crate) fn from_open_raw(raw: HANDLE) -> Self {
        let mut slot = Self::empty();
        slot.capture_raw_output(raw);
        slot
    }

    pub fn state(&self) -> RetainedHandleState {
        self.state
    }

    pub fn close_attempts(&self) -> u8 {
        self.close_attempts
    }

    pub fn is_open(&self) -> bool {
        self.state.is_open()
    }

    pub fn is_closed_cleanly(&self) -> bool {
        self.state.is_closed_cleanly()
    }

    pub fn has_unresolved_debt(&self) -> bool {
        self.state.has_unresolved_debt()
    }

    pub(crate) fn open_raw_handle(&self) -> Option<HANDLE> {
        match self.state {
            RetainedHandleState::Open(raw)
                if raw > 0 && (raw as HANDLE) != INVALID_HANDLE_VALUE =>
            {
                Some(raw as HANDLE)
            }
            _ => None,
        }
    }

    pub(crate) fn capture_raw_output(&mut self, raw: HANDLE) {
        let value = raw as isize;
        if raw.is_null() {
            return;
        }
        if raw == INVALID_HANDLE_VALUE || value <= 0 {
            self.state = RetainedHandleState::InvalidCaptured(value);
        } else {
            self.state = RetainedHandleState::Open(value);
        }
    }

    pub(crate) fn close_once(
        &mut self,
        ops: &mut impl RetainedNativeOps,
    ) -> io::Result<isize> {
        if self.close_attempts >= MAX_CLOSE_CALLS_PER_HANDLE {
            return Err(invalid(
                "handle close budget exhausted; ambiguous handle cannot be retried",
            ));
        }
        match self.state {
            RetainedHandleState::Open(raw)
                if raw > 0 && (raw as HANDLE) != INVALID_HANDLE_VALUE =>
            {
                // Record in-flight before invoking CloseHandle so an unwind or
                // failure retains the exact raw debt and prevents any retry.
                self.state = RetainedHandleState::CloseInFlight(raw);
                self.close_attempts = self.close_attempts.saturating_add(1);
                match ops.close_handle(raw as HANDLE) {
                    Ok(()) => {
                        self.state = RetainedHandleState::Closed { former_raw: raw };
                        Ok(raw)
                    }
                    Err(error) => {
                        let code = error.raw_os_error().unwrap_or(0) as u32;
                        self.state = RetainedHandleState::CloseFailed {
                            raw,
                            error_code: code,
                        };
                        Err(error)
                    }
                }
            }
            RetainedHandleState::CloseInFlight(_) | RetainedHandleState::CloseFailed { .. } => {
                Err(invalid(
                    "ambiguous or failed handle close cannot be retried",
                ))
            }
            RetainedHandleState::InvalidCaptured(_) => {
                Err(invalid("invalid captured raw handle cannot be closed"))
            }
            RetainedHandleState::Closed { .. } => Err(invalid("handle slot is already closed")),
            RetainedHandleState::Empty | RetainedHandleState::Open(_) => {
                Err(invalid("handle slot does not contain a valid open handle"))
            }
        }
    }
}

/// Raw `PROCESS_INFORMATION` outputs captured directly from the preinstalled slot
/// before any fallible validation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedRawProcessInformation {
    pub h_process: isize,
    pub h_thread: isize,
    pub dw_process_id: u32,
    pub dw_thread_id: u32,
    pub create_attempted: bool,
    pub create_returned_true: bool,
    pub create_last_error: Option<u32>,
}

/// Outcome of the at-most-once primary-thread `ResumeThread` operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryThreadResumeOutcome {
    NotAttempted,
    InFlight,
    ResumedFromSuspendCountOne,
    UnexpectedSuspendCount(u32),
    NativeError(u32),
    Unwound,
}

/// Preinstalled fixed slot tracking the single primary-thread resume attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimaryThreadResumeSlot {
    pub attempted: bool,
    pub call_count: u8,
    pub previous_suspend_count: Option<u32>,
    pub outcome: PrimaryThreadResumeOutcome,
}

impl Default for PrimaryThreadResumeSlot {
    fn default() -> Self {
        Self {
            attempted: false,
            call_count: 0,
            previous_suspend_count: None,
            outcome: PrimaryThreadResumeOutcome::NotAttempted,
        }
    }
}

/// Lifecycle stage at which an error was recorded in `RetainedJobOwner`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedOperationStage {
    JobTransferValidation,
    PreCreationValidation,
    AttributeListSetup,
    CreateProcess,
    AttributeListCleanup,
    PostCreationValidation,
    OriginalHandleBorrow,
    BootstrapHandleAccounting,
    ResumePrimaryThread,
    ObserveProcessWait,
    ObserveProcessExitCode,
    ObservePrimaryThreadWait,
    ObserveJobAccounting,
    OuterDebtVerification,
    ClosePrimaryThread,
    CloseProcess,
    CloseJob,
    UnwindDuringOperation,
}

/// Sticky error record retained inside `RetainedJobOwner`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedErrorRecord {
    pub stage: RetainedOperationStage,
    pub kind: io::ErrorKind,
    pub raw_os_error: Option<i32>,
    pub message: String,
}

/// High-level lifecycle phase of `RetainedJobOwner`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedPhase {
    JobInstalledPreCreate,
    CreateInFlight,
    CreatedSuspendedValidated,
    Running,
    ChildExitedAwaitingRetirement,
    FaultedRetained,
    Retired,
}

/// Overall disposition of `RetainedJobOwner`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedDisposition {
    /// Owner is active and progressing normally; no error has occurred.
    ActivePending,
    /// An error, unwind, partial output, unexpected count, or close failure occurred.
    /// All open or ambiguous original handles and resources remain retained as debt.
    UnknownRetained,
    /// All kernel and outer lifecycle gates were satisfied and every owned handle
    /// was individually closed with `CloseHandle` succeeding.
    RetiredCleanly,
}

/// Verified child identity read directly from the original created kernel handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedChildIdentity {
    pub pid: u32,
    pub primary_thread_id: u32,
    pub creation_filetime: u64,
    pub job_member_confirmed: bool,
    pub process_noninherited_confirmed: bool,
    pub thread_noninherited_confirmed: bool,
}

/// Kernel observations collected from the original `hProcess`, `hThread`, and `Job` handles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedKernelObservations {
    pub verified_pid: Option<u32>,
    pub verified_tid: Option<u32>,
    pub creation_filetime: Option<u64>,
    pub job_member_confirmed: bool,
    pub process_noninherited_confirmed: bool,
    pub thread_noninherited_confirmed: bool,
    pub process_signaled: bool,
    pub exit_code: Option<u32>,
    pub thread_signaled: bool,
    pub last_active_processes: Option<u32>,
    pub post_exit_job_empty_observed: bool,
}

/// Separate outer P lifecycle evidence required before retiring kernel handles.
/// Kept distinct from `RetainedKernelObservations` so missing caller integration
/// can never be hidden as a kernel-only flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OuterLifecycleEvidence {
    pub stdio_reaped_and_closed: bool,
    pub bootstrap_handles_discharged: bool,
    pub protocol_ack_and_sdk_quiescent: bool,
    pub prebirth_locks_retained_until_exit: bool,
}

impl OuterLifecycleEvidence {
    pub fn all_discharged(&self) -> bool {
        self.stdio_reaped_and_closed
            && self.bootstrap_handles_discharged
            && self.protocol_ack_and_sdk_quiescent
            && self.prebirth_locks_retained_until_exit
    }
}

/// Finite call and slot counters tracked by `RetainedJobOwner`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedCallBudgets {
    pub create_process_calls: u8,
    pub post_create_validation_calls: u8,
    pub resume_thread_calls: u8,
    pub original_handle_borrows: u32,
    pub bootstrap_duplication_count: u32,
    pub observation_polls: u32,
    pub max_observation_polls: u32,
    pub retire_attempts: u8,
    pub close_handle_calls: u32,
}

/// Fixed accounting slot for a caller-owned handle borrowed in `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedBorrowedHandleSlot {
    pub raw: isize,
    pub inherited_in_create_attempt: bool,
    pub caller_parent_copy_discharged: bool,
}

/// Fixed accounting slot for a handle duplicated into C via `DuplicateHandle` during P->C bootstrap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedBootstrapDuplicationSlot {
    pub source_raw: isize,
    pub in_flight: bool,
    pub succeeded: bool,
    pub failed_or_ambiguous: bool,
    pub discharged: bool,
}

/// Borrowed caller-owned inheritable stdio and optional bootstrap handles (`<= 32` total)
/// for atomic `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` child creation.
#[derive(Clone, Copy, Debug)]
pub struct RetainedChildStdio<'a> {
    pub stdin: BorrowedHandle<'a>,
    pub stdout: BorrowedHandle<'a>,
    pub stderr: BorrowedHandle<'a>,
    pub extra_inherited: &'a [BorrowedHandle<'a>],
}

impl<'a> RetainedChildStdio<'a> {
    pub const fn new(
        stdin: BorrowedHandle<'a>,
        stdout: BorrowedHandle<'a>,
        stderr: BorrowedHandle<'a>,
    ) -> Self {
        Self {
            stdin,
            stdout,
            stderr,
            extra_inherited: &[],
        }
    }

    pub fn from_owned_handles(
        stdin: &'a OwnedHandle,
        stdout: &'a OwnedHandle,
        stderr: &'a OwnedHandle,
    ) -> Self {
        Self::new(stdin.as_handle(), stdout.as_handle(), stderr.as_handle())
    }

    pub fn with_extra_inherited(
        stdin: BorrowedHandle<'a>,
        stdout: BorrowedHandle<'a>,
        stderr: BorrowedHandle<'a>,
        extra_inherited: &'a [BorrowedHandle<'a>],
    ) -> io::Result<Self> {
        if STDIO_HANDLE_COUNT + extra_inherited.len() > MAX_RETAINED_INHERITED_HANDLES {
            return Err(input(
                "explicit inherited handle list exceeds 32-handle maximum",
            ));
        }
        Ok(Self {
            stdin,
            stdout,
            stderr,
            extra_inherited,
        })
    }
}

/// Narrow, lifetime-bound borrow of the original created child `hProcess`.
/// Cannot be constructed from a PID, JSON field, or raw integer; the handle
/// remains owned by `RetainedJobOwner` and must never be closed by the caller.
#[derive(Clone, Copy, Debug)]
pub struct OriginalProcessHandleBorrow<'a> {
    raw: HANDLE,
    _owner: PhantomData<&'a OwnedHandle>,
}

impl<'a> OriginalProcessHandleBorrow<'a> {
    pub fn raw_handle(&self) -> HANDLE {
        self.raw
    }

    pub fn raw_isize(&self) -> isize {
        self.raw as isize
    }

    pub fn as_borrowed_handle(&self) -> BorrowedHandle<'a> {
        // SAFETY: `self.raw` is a validated open kernel handle owned by `RetainedJobOwner`
        // for at least `'a`, and cannot be closed while `'a` is borrowed.
        unsafe { BorrowedHandle::borrow_raw(self.raw) }
    }
}

/// Narrow, lifetime-bound borrow of the original created child primary `hThread`.
#[derive(Clone, Copy, Debug)]
pub struct OriginalThreadHandleBorrow<'a> {
    raw: HANDLE,
    _owner: PhantomData<&'a OwnedHandle>,
}

impl<'a> OriginalThreadHandleBorrow<'a> {
    pub fn raw_handle(&self) -> HANDLE {
        self.raw
    }

    pub fn raw_isize(&self) -> isize {
        self.raw as isize
    }

    pub fn as_borrowed_handle(&self) -> BorrowedHandle<'a> {
        // SAFETY: `self.raw` is a validated open kernel handle owned by `RetainedJobOwner`
        // for at least `'a`, and cannot be closed while `'a` is borrowed.
        unsafe { BorrowedHandle::borrow_raw(self.raw) }
    }
}

/// Narrow, lifetime-bound borrow of the original `Job` handle.
#[derive(Clone, Copy, Debug)]
pub struct OriginalJobHandleBorrow<'a> {
    raw: HANDLE,
    _owner: PhantomData<&'a OwnedHandle>,
}

impl<'a> OriginalJobHandleBorrow<'a> {
    pub fn raw_handle(&self) -> HANDLE {
        self.raw
    }

    pub fn raw_isize(&self) -> isize {
        self.raw as isize
    }

    pub fn as_borrowed_handle(&self) -> BorrowedHandle<'a> {
        // SAFETY: `self.raw` is a validated open kernel handle owned by `RetainedJobOwner`
        // for at least `'a`, and cannot be closed while `'a` is borrowed.
        unsafe { BorrowedHandle::borrow_raw(self.raw) }
    }
}

/// Receipt returned only after all kernel observations, outer debt discharges,
/// and individually checked `CloseHandle` calls succeed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedRetirementReceipt {
    pub identity: RetainedChildIdentity,
    pub exit_code: u32,
    pub kernel_observations: RetainedKernelObservations,
    pub outer_evidence: OuterLifecycleEvidence,
    pub closed_primary_thread_raw: isize,
    pub closed_process_raw: isize,
    pub closed_job_raw: isize,
}

pub(crate) struct CreateProcessAtomicResult {
    pub setup_error: Option<io::Error>,
    pub create_invoked: bool,
    pub create_ok: bool,
    pub create_last_error: Option<u32>,
    pub cleanup_error: Option<io::Error>,
}

pub(crate) trait RetainedNativeOps {
    fn verify_job_containment(&mut self, job_handle: HANDLE) -> io::Result<()>;
    fn job_active_processes(&mut self, job_handle: HANDLE) -> io::Result<u32>;
    fn handle_inheritable(&mut self, handle: HANDLE) -> io::Result<bool>;
    fn set_handle_inheritable(&mut self, handle: HANDLE, inherit: bool) -> io::Result<()>;
    fn create_process_atomic(
        &mut self,
        executable: &[u16],
        command: &mut [u16],
        environment: &[u16],
        directory: &[u16],
        attr_storage: &mut Vec<AttributeStorage>,
        attr_initialized: &mut bool,
        attr_deleted: &mut bool,
        attr_jobs: &[HANDLE; 1],
        attr_handles: &[HANDLE],
        pi_storage: &mut PROCESS_INFORMATION,
    ) -> CreateProcessAtomicResult;
    fn process_id(&mut self, process_handle: HANDLE) -> io::Result<u32>;
    fn process_creation_filetime(&mut self, process_handle: HANDLE) -> io::Result<u64>;
    fn thread_process_id(&mut self, thread_handle: HANDLE) -> io::Result<u32>;
    fn thread_id(&mut self, thread_handle: HANDLE) -> io::Result<u32>;
    fn is_process_in_job(
        &mut self,
        process_handle: HANDLE,
        job_handle: HANDLE,
    ) -> io::Result<bool>;
    fn wait_handle_signaled_nonblocking(&mut self, handle: HANDLE) -> io::Result<bool>;
    fn process_exit_code(&mut self, process_handle: HANDLE) -> io::Result<u32>;
    fn resume_thread(&mut self, thread_handle: HANDLE) -> Result<u32, u32>;
    fn close_handle(&mut self, handle: HANDLE) -> io::Result<()>;
}

pub(crate) struct RealRetainedNativeOps;

impl RetainedNativeOps for RealRetainedNativeOps {
    fn verify_job_containment(&mut self, job_handle: HANDLE) -> io::Result<()> {
        if job_handle.is_null()
            || job_handle == INVALID_HANDLE_VALUE
            || (job_handle as isize) <= 0
        {
            return Err(invalid("invalid Job handle in retained owner"));
        }
        if self.handle_inheritable(job_handle)? {
            return Err(invalid("Job handle became inheritable"));
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let mut returned = 0;
        // SAFETY: limits and returned are writable, correctly sized storage.
        if unsafe {
            QueryInformationJobObject(
                job_handle,
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                &mut returned,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if returned != size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32 {
            return Err(invalid("incomplete Job limit readback"));
        }
        let flags = limits.BasicLimitInformation.LimitFlags;
        if flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE == 0
            || flags & (JOB_OBJECT_LIMIT_BREAKAWAY_OK | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK) != 0
            || flags != JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        {
            return Err(invalid(
                "Job limit readback does not match requested containment",
            ));
        }
        Ok(())
    }

    fn job_active_processes(&mut self, job_handle: HANDLE) -> io::Result<u32> {
        let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        let mut returned = 0;
        // SAFETY: accounting and returned are writable, correctly sized storage.
        if unsafe {
            QueryInformationJobObject(
                job_handle,
                JobObjectBasicAccountingInformation,
                (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                &mut returned,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if returned != size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32 {
            return Err(invalid("incomplete Job accounting readback"));
        }
        Ok(accounting.ActiveProcesses)
    }

    fn handle_inheritable(&mut self, handle: HANDLE) -> io::Result<bool> {
        let mut flags = 0;
        // SAFETY: flags is writable DWORD storage; Windows validates handle.
        if unsafe { GetHandleInformation(handle, &mut flags) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(flags & HANDLE_FLAG_INHERIT != 0)
    }

    fn set_handle_inheritable(&mut self, handle: HANDLE, inherit: bool) -> io::Result<()> {
        // SAFETY: only the handle's inheritance flag is modified.
        if unsafe {
            SetHandleInformation(
                handle,
                HANDLE_FLAG_INHERIT,
                if inherit { HANDLE_FLAG_INHERIT } else { 0 },
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if self.handle_inheritable(handle)? != inherit {
            return Err(invalid(
                "stdio/process handle inheritance readback mismatch",
            ));
        }
        Ok(())
    }

    fn create_process_atomic(
        &mut self,
        executable: &[u16],
        command: &mut [u16],
        environment: &[u16],
        directory: &[u16],
        attr_storage: &mut Vec<AttributeStorage>,
        attr_initialized: &mut bool,
        attr_deleted: &mut bool,
        attr_jobs: &[HANDLE; 1],
        attr_handles: &[HANDLE],
        pi_storage: &mut PROCESS_INFORMATION,
    ) -> CreateProcessAtomicResult {
        let mut size = 0;
        // SAFETY: null buffer queries the required attribute list byte size.
        let sizing_ok =
            unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), 2, 0, &mut size) };
        let sizing_err = io::Error::last_os_error();
        if sizing_ok != 0
            || sizing_err.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
            || size == 0
            || size > MAX_ATTRIBUTE_LIST_BYTES
        {
            return CreateProcessAtomicResult {
                setup_error: Some(invalid("could not size process attribute list")),
                create_invoked: false,
                create_ok: false,
                create_last_error: None,
                cleanup_error: None,
            };
        }
        attr_storage.clear();
        attr_storage.resize(size.div_ceil(16), AttributeStorage([0; 16]));
        let list_ptr: LPPROC_THREAD_ATTRIBUTE_LIST = attr_storage.as_mut_ptr().cast();
        // SAFETY: attr_storage is 16-byte aligned and at least `size` bytes.
        if unsafe { InitializeProcThreadAttributeList(list_ptr, 2, 0, &mut size) } == 0 {
            return CreateProcessAtomicResult {
                setup_error: Some(io::Error::last_os_error()),
                create_invoked: false,
                create_ok: false,
                create_last_error: None,
                cleanup_error: None,
            };
        }
        *attr_initialized = true;
        *attr_deleted = false;

        // SAFETY: attr_jobs is stored in `RetainedJobOwner` and outlives DeleteProcThreadAttributeList.
        let job_attr_ok = unsafe {
            UpdateProcThreadAttribute(
                list_ptr,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                attr_jobs.as_ptr().cast(),
                size_of::<[HANDLE; 1]>(),
                ptr::null_mut(),
                ptr::null(),
            )
        } != 0;
        if !job_attr_ok {
            let err = io::Error::last_os_error();
            // SAFETY: list_ptr was initialized above.
            unsafe { DeleteProcThreadAttributeList(list_ptr) };
            *attr_initialized = false;
            *attr_deleted = true;
            return CreateProcessAtomicResult {
                setup_error: Some(err),
                create_invoked: false,
                create_ok: false,
                create_last_error: None,
                cleanup_error: None,
            };
        }

        // SAFETY: attr_handles is stored in `RetainedJobOwner` and outlives DeleteProcThreadAttributeList.
        let handle_attr_ok = unsafe {
            UpdateProcThreadAttribute(
                list_ptr,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                attr_handles.as_ptr().cast(),
                std::mem::size_of_val(attr_handles),
                ptr::null_mut(),
                ptr::null(),
            )
        } != 0;
        if !handle_attr_ok {
            let err = io::Error::last_os_error();
            // SAFETY: list_ptr was initialized above.
            unsafe { DeleteProcThreadAttributeList(list_ptr) };
            *attr_initialized = false;
            *attr_deleted = true;
            return CreateProcessAtomicResult {
                setup_error: Some(err),
                create_invoked: false,
                create_ok: false,
                create_last_error: None,
                cleanup_error: None,
            };
        }

        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = attr_handles[0];
        startup.StartupInfo.hStdOutput = attr_handles[1];
        startup.StartupInfo.hStdError = attr_handles[2];
        startup.lpAttributeList = list_ptr;

        // SAFETY: all input buffers and attribute arrays remain live for the call,
        // and outputs are written directly into the preinstalled `pi_storage` slot.
        let create_ok = unsafe {
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
                pi_storage,
            )
        } != 0;
        let create_last_error = if create_ok {
            None
        } else {
            Some(unsafe { GetLastError() })
        };

        // SAFETY: list_ptr was initialized and CreateProcessW has returned.
        unsafe { DeleteProcThreadAttributeList(list_ptr) };
        *attr_initialized = false;
        *attr_deleted = true;

        CreateProcessAtomicResult {
            setup_error: None,
            create_invoked: true,
            create_ok,
            create_last_error,
            cleanup_error: None,
        }
    }

    fn process_id(&mut self, process_handle: HANDLE) -> io::Result<u32> {
        // SAFETY: caller holds the open original process handle.
        let pid = unsafe { GetProcessId(process_handle) };
        if pid == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(pid)
    }

    fn process_creation_filetime(&mut self, process_handle: HANDLE) -> io::Result<u64> {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        // SAFETY: process_handle is owned by `RetainedJobOwner` and all FILETIMEs are writable.
        if unsafe {
            GetProcessTimes(
                process_handle,
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let value = ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64;
        if value == 0 {
            return Err(invalid("process creation identity missing"));
        }
        Ok(value)
    }

    fn thread_process_id(&mut self, thread_handle: HANDLE) -> io::Result<u32> {
        // SAFETY: caller holds the open original thread handle.
        let pid = unsafe { GetProcessIdOfThread(thread_handle) };
        if pid == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(pid)
    }

    fn thread_id(&mut self, thread_handle: HANDLE) -> io::Result<u32> {
        // SAFETY: caller holds the open original thread handle.
        let tid = unsafe { GetThreadId(thread_handle) };
        if tid == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(tid)
    }

    fn is_process_in_job(
        &mut self,
        process_handle: HANDLE,
        job_handle: HANDLE,
    ) -> io::Result<bool> {
        let mut member = 0;
        // SAFETY: both handles remain open in `RetainedJobOwner` and `member` is writable.
        if unsafe { IsProcessInJob(process_handle, job_handle, &mut member) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(member != 0)
    }

    fn wait_handle_signaled_nonblocking(&mut self, handle: HANDLE) -> io::Result<bool> {
        // SAFETY: zero timeout is non-blocking; handle is owned by `RetainedJobOwner`.
        match unsafe { WaitForSingleObject(handle, 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            _ => Err(invalid("unexpected wait result on retained handle")),
        }
    }

    fn process_exit_code(&mut self, process_handle: HANDLE) -> io::Result<u32> {
        let mut code = 0;
        // SAFETY: process_handle is owned by `RetainedJobOwner` and `code` is writable DWORD storage.
        if unsafe { GetExitCodeProcess(process_handle, &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(code)
    }

    fn resume_thread(&mut self, thread_handle: HANDLE) -> Result<u32, u32> {
        // SAFETY: thread_handle is the original primary thread handle from CreateProcessW.
        let prev = unsafe { ResumeThread(thread_handle) };
        if prev == u32::MAX {
            Err(unsafe { GetLastError() })
        } else {
            Ok(prev)
        }
    }

    fn close_handle(&mut self, handle: HANDLE) -> io::Result<()> {
        // SAFETY: handle is uniquely owned by `RetainedJobOwner` and closed at most once.
        if unsafe { CloseHandle(handle) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// Preinstallable retained owner for atomic Job-assigned child creation, narrow
/// original-handle observation/borrowing, at-most-once primary-thread resume,
/// and individually checked handle retirement.
///
/// The caller must install this struct into its bounded parent (P) reservation
/// before invoking any effectful method (`&mut self`). Across every error,
/// timeout, or unwind, the same instance retains the original `Job`, child
/// `hProcess`, primary `hThread`, raw `PROCESS_INFORMATION`, stdio/bootstrap
/// handle accounting, sticky `first_error`, and `no_replay` state without
/// invoking `TerminateProcess`, `TerminateJobObject`, or unchecked `Drop` closes.
pub struct RetainedJobOwner {
    job: RetainedHandleSlot,
    pi_storage: PROCESS_INFORMATION,
    raw_pi: RetainedRawProcessInformation,
    process: RetainedHandleSlot,
    primary_thread: RetainedHandleSlot,
    resume_slot: PrimaryThreadResumeSlot,
    borrowed_stdio: [RetainedBorrowedHandleSlot; MAX_RETAINED_INHERITED_HANDLES],
    borrowed_stdio_count: usize,
    bootstrap_duplications: [RetainedBootstrapDuplicationSlot; MAX_RETAINED_INHERITED_HANDLES],
    bootstrap_duplication_count: usize,
    attribute_storage: Vec<AttributeStorage>,
    attribute_list_initialized: bool,
    attribute_list_deleted: bool,
    attribute_jobs: [HANDLE; JOB_LIST_HANDLE_COUNT],
    attribute_handles: [HANDLE; MAX_RETAINED_INHERITED_HANDLES],
    attribute_handle_count: usize,
    observations: RetainedKernelObservations,
    outer_evidence: Option<OuterLifecycleEvidence>,
    first_error: Option<RetainedErrorRecord>,
    last_error: Option<RetainedErrorRecord>,
    error_count: u32,
    no_replay: bool,
    create_attempted: bool,
    post_create_validated: bool,
    phase: RetainedPhase,
    disposition: RetainedDisposition,
    budgets: RetainedCallBudgets,
}

// SAFETY: All kernel handles and heap allocations inside `RetainedJobOwner` are
// uniquely owned and mutated only through `&mut self`; Windows kernel handles
// are process-wide.
unsafe impl Send for RetainedJobOwner {}

impl RetainedJobOwner {
    /// Transfer the original `Job` into a preinstallable `RetainedJobOwner` before
    /// any child `CreateProcessW` call. Consumes `job` without closing its handle,
    /// preventing ordinary `Job` `Drop` from closing the final `KILL_ON_JOB_CLOSE` handle.
    pub fn from_job(job: Job) -> Self {
        let raw_job = job.into_raw_handle();
        Self::new_with_raw_job(raw_job)
    }

    #[cfg(test)]
    pub(crate) fn from_raw_job_for_test(raw_job: HANDLE) -> Self {
        Self::new_with_raw_job(raw_job)
    }

    fn new_with_raw_job(raw_job: HANDLE) -> Self {
        let job_slot = RetainedHandleSlot::from_open_raw(raw_job);
        let valid_job = job_slot.is_open();
        let mut owner = Self {
            job: job_slot,
            pi_storage: PROCESS_INFORMATION::default(),
            raw_pi: RetainedRawProcessInformation::default(),
            process: RetainedHandleSlot::empty(),
            primary_thread: RetainedHandleSlot::empty(),
            resume_slot: PrimaryThreadResumeSlot::default(),
            borrowed_stdio: [RetainedBorrowedHandleSlot::default(); MAX_RETAINED_INHERITED_HANDLES],
            borrowed_stdio_count: 0,
            bootstrap_duplications: [RetainedBootstrapDuplicationSlot::default();
                MAX_RETAINED_INHERITED_HANDLES],
            bootstrap_duplication_count: 0,
            attribute_storage: Vec::new(),
            attribute_list_initialized: false,
            attribute_list_deleted: false,
            attribute_jobs: [raw_job],
            attribute_handles: [ptr::null_mut(); MAX_RETAINED_INHERITED_HANDLES],
            attribute_handle_count: 0,
            observations: RetainedKernelObservations::default(),
            outer_evidence: None,
            first_error: None,
            last_error: None,
            error_count: 0,
            no_replay: !valid_job,
            create_attempted: false,
            post_create_validated: false,
            phase: if valid_job {
                RetainedPhase::JobInstalledPreCreate
            } else {
                RetainedPhase::FaultedRetained
            },
            disposition: if valid_job {
                RetainedDisposition::ActivePending
            } else {
                RetainedDisposition::UnknownRetained
            },
            budgets: RetainedCallBudgets {
                create_process_calls: 0,
                post_create_validation_calls: 0,
                resume_thread_calls: 0,
                original_handle_borrows: 0,
                bootstrap_duplication_count: 0,
                observation_polls: 0,
                max_observation_polls: DEFAULT_MAX_OBSERVATION_POLLS,
                retire_attempts: 0,
                close_handle_calls: 0,
            },
        };
        if !valid_job {
            let err = invalid("transferred Job handle is null or invalid");
            owner.record_error(RetainedOperationStage::JobTransferValidation, &err);
        }
        owner
    }

    /// Configure the finite non-blocking observation poll budget (`1..=65536`)
    /// in place on an installed pre-creation owner without consuming `self`.
    ///
    /// Restricted to the genuine valid pre-creation phase (`JobInstalledPreCreate`)
    /// with no prior error or creation attempt.
    /// On invalid bounds or late/invalid phase, records the error on `self`,
    /// transitions to `FaultedRetained` (`UnknownRetained`, `no_replay = true`),
    /// and preserves all original handle slots and call counters in the same owner.
    pub fn set_max_observation_polls(&mut self, max_polls: u32) -> io::Result<()> {
        if self.no_replay
            || self.create_attempted
            || self.first_error.is_some()
            || self.disposition != RetainedDisposition::ActivePending
            || self.phase != RetainedPhase::JobInstalledPreCreate
            || !self.job.is_open()
        {
            let err = input(
                "observation poll budget configuration requires a valid pre-creation owner",
            );
            self.record_error(RetainedOperationStage::PreCreationValidation, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        if max_polls == 0 || max_polls > MAX_OBSERVATION_POLLS_LIMIT {
            let err = input("observation poll budget must be within 1..=65536");
            self.record_error(RetainedOperationStage::PreCreationValidation, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        self.budgets.max_observation_polls = max_polls;
        Ok(())
    }

    fn record_error(&mut self, stage: RetainedOperationStage, error: &io::Error) {
        self.no_replay = true;
        self.disposition = RetainedDisposition::UnknownRetained;
        self.error_count = self.error_count.saturating_add(1);
        let record = RetainedErrorRecord {
            stage,
            kind: error.kind(),
            raw_os_error: error.raw_os_error(),
            message: error.to_string(),
        };
        if self.first_error.is_none() {
            self.first_error = Some(record.clone());
        }
        self.last_error = Some(record);
    }

    fn capture_pi_outputs_immediately(
        &mut self,
        create_returned_true: bool,
        create_last_error: Option<u32>,
    ) {
        let raw_proc = self.pi_storage.hProcess;
        let raw_thread = self.pi_storage.hThread;
        self.raw_pi = RetainedRawProcessInformation {
            h_process: raw_proc as isize,
            h_thread: raw_thread as isize,
            dw_process_id: self.pi_storage.dwProcessId,
            dw_thread_id: self.pi_storage.dwThreadId,
            create_attempted: true,
            create_returned_true,
            create_last_error,
        };
        self.process.capture_raw_output(raw_proc);
        self.primary_thread.capture_raw_output(raw_thread);
    }

    /// Create the suspended child atomically inside the retained `Job` using
    /// caller-owned borrowed stdio and optional extra inherited originals
    /// (`RetainedChildStdio`) and a `PreparedTrustedHostEnvironment`.
    pub fn create_suspended_prepared_host_with_stdio(
        &mut self,
        exe: &Path,
        args: &[String],
        cwd: &Path,
        prepared: PreparedTrustedHostEnvironment,
        stdio: RetainedChildStdio<'_>,
    ) -> io::Result<RetainedChildIdentity> {
        self.create_suspended_prepared_host_with_stdio_and_ops(
            exe,
            args,
            cwd,
            prepared,
            stdio,
            &mut RealRetainedNativeOps,
        )
    }

    pub(crate) fn create_suspended_prepared_host_with_stdio_and_ops(
        &mut self,
        exe: &Path,
        args: &[String],
        cwd: &Path,
        prepared: PreparedTrustedHostEnvironment,
        stdio: RetainedChildStdio<'_>,
        ops: &mut impl RetainedNativeOps,
    ) -> io::Result<RetainedChildIdentity> {
        let mut raw_handles = [ptr::null_mut(); MAX_RETAINED_INHERITED_HANDLES];
        let count = STDIO_HANDLE_COUNT + stdio.extra_inherited.len();
        if count > MAX_RETAINED_INHERITED_HANDLES {
            let err = input("explicit inherited handle list exceeds 32-handle maximum");
            self.record_error(RetainedOperationStage::PreCreationValidation, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        raw_handles[0] = stdio.stdin.as_raw_handle();
        raw_handles[1] = stdio.stdout.as_raw_handle();
        raw_handles[2] = stdio.stderr.as_raw_handle();
        for (idx, extra) in stdio.extra_inherited.iter().enumerate() {
            raw_handles[STDIO_HANDLE_COUNT + idx] = extra.as_raw_handle();
        }
        self.create_with_raw_borrowed_stdio_and_ops(
            exe,
            args,
            cwd,
            prepared,
            &raw_handles[..count],
            ops,
        )
    }

    pub(crate) fn create_with_raw_borrowed_stdio_and_ops(
        &mut self,
        exe: &Path,
        args: &[String],
        cwd: &Path,
        prepared: PreparedTrustedHostEnvironment,
        borrowed_handles: &[HANDLE],
        ops: &mut impl RetainedNativeOps,
    ) -> io::Result<RetainedChildIdentity> {
        if self.phase != RetainedPhase::JobInstalledPreCreate || self.borrowed_stdio_count != 0 {
            let err = io::Error::new(
                io::ErrorKind::AlreadyExists,
                "retained child creation cannot be replayed on this owner",
            );
            self.record_error(RetainedOperationStage::PreCreationValidation, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        self.create_inner_with_raw_handles(exe, args, cwd, prepared, borrowed_handles, ops)
    }

    fn create_inner_with_raw_handles(
        &mut self,
        exe: &Path,
        args: &[String],
        cwd: &Path,
        prepared: PreparedTrustedHostEnvironment,
        inherited_handles: &[HANDLE],
        ops: &mut impl RetainedNativeOps,
    ) -> io::Result<RetainedChildIdentity> {
        if self.no_replay || self.create_attempted || self.first_error.is_some() {
            let err = io::Error::new(
                io::ErrorKind::AlreadyExists,
                "retained child creation cannot be replayed on this owner",
            );
            self.record_error(RetainedOperationStage::PreCreationValidation, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        if self.budgets.create_process_calls >= MAX_CREATE_PROCESS_CALLS {
            let err = invalid("retained CreateProcessW call budget exhausted");
            self.record_error(RetainedOperationStage::PreCreationValidation, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }

        let Some(raw_job) = self.job.open_raw_handle() else {
            let err = invalid("retained Job handle is not open before creation");
            self.record_error(RetainedOperationStage::JobTransferValidation, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        };

        // Validate inputs and Job containment before touching any process creation API.
        let mut launch = match PreparedChildLaunch::prepare(exe, args, cwd, prepared) {
            Ok(v) => v,
            Err(err) => {
                self.record_error(RetainedOperationStage::PreCreationValidation, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
        };

        if inherited_handles.len() < STDIO_HANDLE_COUNT
            || inherited_handles.len() > MAX_RETAINED_INHERITED_HANDLES
        {
            let err = input("inherited handle list must contain between 3 and 32 handles");
            self.record_error(RetainedOperationStage::PreCreationValidation, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }

        for (i, &h) in inherited_handles.iter().enumerate() {
            if h.is_null() || h == INVALID_HANDLE_VALUE || (h as isize) <= 0 {
                let err = input("inherited stdio/bootstrap handle is null, invalid, or pseudo");
                self.record_error(RetainedOperationStage::PreCreationValidation, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
            if h == raw_job {
                let err = input("Job handle must never appear in inherited HANDLE_LIST");
                self.record_error(RetainedOperationStage::PreCreationValidation, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
            if inherited_handles[..i].contains(&h) {
                let err = input("duplicate handle in explicit PROC_THREAD_ATTRIBUTE_HANDLE_LIST");
                self.record_error(RetainedOperationStage::PreCreationValidation, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
        }

        if let Err(err) = ops.verify_job_containment(raw_job) {
            self.record_error(RetainedOperationStage::JobTransferValidation, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }

        for &h in inherited_handles {
            match ops.handle_inheritable(h) {
                Ok(true) => {}
                Ok(false) => {
                    let err = invalid("explicit HANDLE_LIST entry is not marked inheritable");
                    self.record_error(RetainedOperationStage::PreCreationValidation, &err);
                    self.phase = RetainedPhase::FaultedRetained;
                    return Err(err);
                }
                Err(err) => {
                    self.record_error(RetainedOperationStage::PreCreationValidation, &err);
                    self.phase = RetainedPhase::FaultedRetained;
                    return Err(err);
                }
            }
        }

        self.borrowed_stdio_count = inherited_handles.len();
        for (idx, &h) in inherited_handles.iter().enumerate() {
            self.borrowed_stdio[idx] = RetainedBorrowedHandleSlot {
                raw: h as isize,
                inherited_in_create_attempt: true,
                caller_parent_copy_discharged: false,
            };
        }

        self.attribute_jobs = [raw_job];
        self.attribute_handle_count = inherited_handles.len();
        for (idx, &h) in inherited_handles.iter().enumerate() {
            self.attribute_handles[idx] = h;
        }

        // Preinstall in-flight creation state before invoking native CreateProcessW.
        self.pi_storage = PROCESS_INFORMATION::default();
        self.raw_pi = RetainedRawProcessInformation {
            create_attempted: true,
            ..Default::default()
        };
        self.create_attempted = true;
        self.no_replay = true;
        self.phase = RetainedPhase::CreateInFlight;
        self.budgets.create_process_calls = self.budgets.create_process_calls.saturating_add(1);

        struct CreateInFlightGuard<'a> {
            owner: &'a mut RetainedJobOwner,
            captured: bool,
        }
        impl Drop for CreateInFlightGuard<'_> {
            fn drop(&mut self) {
                if !self.captured {
                    self.owner.capture_pi_outputs_immediately(false, None);
                    let err = io::Error::other("CreateProcessW unwound after in-flight attempt");
                    self.owner
                        .record_error(RetainedOperationStage::UnwindDuringOperation, &err);
                    self.owner.phase = RetainedPhase::FaultedRetained;
                }
            }
        }

        let mut guard = CreateInFlightGuard {
            owner: self,
            captured: false,
        };
        let handle_count = guard.owner.attribute_handle_count;
        let call_result = ops.create_process_atomic(
            &launch.executable,
            &mut launch.command,
            &launch.environment,
            &launch.directory,
            &mut guard.owner.attribute_storage,
            &mut guard.owner.attribute_list_initialized,
            &mut guard.owner.attribute_list_deleted,
            &guard.owner.attribute_jobs,
            &guard.owner.attribute_handles[..handle_count],
            &mut guard.owner.pi_storage,
        );
        // Capture both raw PI outputs immediately before any fallible error check or validation,
        // including when CreateProcessW returns FALSE with partial outputs.
        if call_result.create_invoked
            || !guard.owner.pi_storage.hProcess.is_null()
            || !guard.owner.pi_storage.hThread.is_null()
            || guard.owner.pi_storage.dwProcessId != 0
            || guard.owner.pi_storage.dwThreadId != 0
        {
            guard.owner.capture_pi_outputs_immediately(
                call_result.create_ok,
                call_result.create_last_error,
            );
        }
        guard.captured = true;
        drop(guard);

        if let Some(setup_err) = call_result.setup_error {
            self.record_error(RetainedOperationStage::AttributeListSetup, &setup_err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(setup_err);
        }
        if !call_result.create_ok {
            let code = call_result.create_last_error.unwrap_or(0) as i32;
            let create_err = io::Error::from_raw_os_error(code);
            self.record_error(RetainedOperationStage::CreateProcess, &create_err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(create_err);
        }
        if let Some(cleanup_err) = call_result.cleanup_error {
            self.record_error(RetainedOperationStage::AttributeListCleanup, &cleanup_err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(cleanup_err);
        }

        // Post-CreateProcess validation on the captured original handles.
        // Never calls job.terminate(), TerminateProcess, or CloseHandle on error.
        if self.budgets.post_create_validation_calls >= MAX_POST_CREATE_VALIDATION_CALLS {
            let err = invalid("post-create validation budget exhausted");
            self.record_error(RetainedOperationStage::PostCreationValidation, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        self.budgets.post_create_validation_calls =
            self.budgets.post_create_validation_calls.saturating_add(1);

        struct PostValidateGuard<'a> {
            owner: &'a mut RetainedJobOwner,
            completed: bool,
        }
        impl Drop for PostValidateGuard<'_> {
            fn drop(&mut self) {
                if !self.completed {
                    let err = io::Error::other("post-CreateProcess validation unwound");
                    self.owner
                        .record_error(RetainedOperationStage::UnwindDuringOperation, &err);
                    self.owner.phase = RetainedPhase::FaultedRetained;
                }
            }
        }

        let mut val_guard = PostValidateGuard {
            owner: self,
            completed: false,
        };
        let validation_result = (|| -> io::Result<RetainedChildIdentity> {
            let raw_proc = val_guard
                .owner
                .process
                .open_raw_handle()
                .ok_or_else(|| invalid("CreateProcessW returned invalid process handle"))?;
            let raw_thread = val_guard
                .owner
                .primary_thread
                .open_raw_handle()
                .ok_or_else(|| invalid("CreateProcessW returned invalid primary-thread handle"))?;
            if raw_proc == raw_thread {
                return Err(invalid(
                    "CreateProcessW returned identical process and primary-thread handles",
                ));
            }
            let expected_pid = val_guard.owner.raw_pi.dw_process_id;
            let expected_tid = val_guard.owner.raw_pi.dw_thread_id;
            if expected_pid == 0 || expected_tid == 0 {
                return Err(invalid("CreateProcessW returned zero PID or TID"));
            }

            ops.set_handle_inheritable(raw_proc, false)?;
            if ops.handle_inheritable(raw_proc)? {
                return Err(invalid("created process handle remains inheritable"));
            }
            val_guard.owner.observations.process_noninherited_confirmed = true;

            ops.set_handle_inheritable(raw_thread, false)?;
            if ops.handle_inheritable(raw_thread)? {
                return Err(invalid("created primary-thread handle remains inheritable"));
            }
            val_guard.owner.observations.thread_noninherited_confirmed = true;

            let actual_pid = ops.process_id(raw_proc)?;
            if actual_pid != expected_pid {
                return Err(invalid("created process PID mismatch"));
            }
            let creation_ft = ops.process_creation_filetime(raw_proc)?;
            if creation_ft == 0 {
                return Err(invalid("process creation identity missing"));
            }
            val_guard.owner.observations.creation_filetime = Some(creation_ft);

            let owner_pid = ops.thread_process_id(raw_thread)?;
            if owner_pid != actual_pid {
                return Err(invalid("primary thread process identity mismatch"));
            }
            let actual_tid = ops.thread_id(raw_thread)?;
            if actual_tid != expected_tid {
                return Err(invalid("created primary thread ID mismatch"));
            }
            if !ops.is_process_in_job(raw_proc, raw_job)? {
                return Err(invalid("atomic child Job membership not confirmed"));
            }
            val_guard.owner.observations.job_member_confirmed = true;

            ops.verify_job_containment(raw_job)?;
            if ops.wait_handle_signaled_nonblocking(raw_proc)? {
                val_guard.owner.observations.process_signaled = true;
                return Err(invalid("suspended child exited before validation completed"));
            }

            val_guard.owner.observations.verified_pid = Some(actual_pid);
            val_guard.owner.observations.verified_tid = Some(actual_tid);
            Ok(RetainedChildIdentity {
                pid: actual_pid,
                primary_thread_id: actual_tid,
                creation_filetime: creation_ft,
                job_member_confirmed: true,
                process_noninherited_confirmed: true,
                thread_noninherited_confirmed: true,
            })
        })();
        val_guard.completed = true;
        drop(val_guard);

        let identity = match validation_result {
            Ok(id) => id,
            Err(err) => {
                self.record_error(RetainedOperationStage::PostCreationValidation, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
        };

        self.post_create_validated = true;
        self.phase = RetainedPhase::CreatedSuspendedValidated;
        Ok(identity)
    }

    /// Read the verified child identity (PID, TID, full 64-bit creation FILETIME,
    /// Job membership, and noninheritance) established from the original handles.
    pub fn verified_identity(&self) -> io::Result<RetainedChildIdentity> {
        if !self.post_create_validated {
            return Err(invalid("child identity has not completed post-create validation"));
        }
        let pid = self
            .observations
            .verified_pid
            .ok_or_else(|| invalid("verified child PID missing"))?;
        let tid = self
            .observations
            .verified_tid
            .ok_or_else(|| invalid("verified primary thread ID missing"))?;
        let creation_filetime = self
            .observations
            .creation_filetime
            .ok_or_else(|| invalid("verified child creation FILETIME missing"))?;
        Ok(RetainedChildIdentity {
            pid,
            primary_thread_id: tid,
            creation_filetime,
            job_member_confirmed: self.observations.job_member_confirmed,
            process_noninherited_confirmed: self.observations.process_noninherited_confirmed,
            thread_noninherited_confirmed: self.observations.thread_noninherited_confirmed,
        })
    }

    /// Borrow the original created child `hProcess` within the finite borrow budget.
    /// Originals remain owned by `RetainedJobOwner`.
    pub fn borrow_original_process_handle(
        &mut self,
    ) -> io::Result<OriginalProcessHandleBorrow<'_>> {
        if !self.post_create_validated || self.first_error.is_some() {
            let err = invalid("original process handle borrow requires validated active owner");
            self.record_error(RetainedOperationStage::OriginalHandleBorrow, &err);
            return Err(err);
        }
        if self.budgets.original_handle_borrows >= MAX_ORIGINAL_HANDLE_BORROWS {
            let err = invalid("original handle borrow budget exhausted");
            self.record_error(RetainedOperationStage::OriginalHandleBorrow, &err);
            return Err(err);
        }
        let Some(raw) = self.process.open_raw_handle() else {
            let err = invalid("original process handle is not open");
            self.record_error(RetainedOperationStage::OriginalHandleBorrow, &err);
            return Err(err);
        };
        self.budgets.original_handle_borrows =
            self.budgets.original_handle_borrows.saturating_add(1);
        Ok(OriginalProcessHandleBorrow {
            raw,
            _owner: PhantomData,
        })
    }

    /// Borrow the original created child primary `hThread` within the finite borrow budget.
    pub fn borrow_original_primary_thread_handle(
        &mut self,
    ) -> io::Result<OriginalThreadHandleBorrow<'_>> {
        if !self.post_create_validated || self.first_error.is_some() {
            let err = invalid("original thread handle borrow requires validated active owner");
            self.record_error(RetainedOperationStage::OriginalHandleBorrow, &err);
            return Err(err);
        }
        if self.budgets.original_handle_borrows >= MAX_ORIGINAL_HANDLE_BORROWS {
            let err = invalid("original handle borrow budget exhausted");
            self.record_error(RetainedOperationStage::OriginalHandleBorrow, &err);
            return Err(err);
        }
        let Some(raw) = self.primary_thread.open_raw_handle() else {
            let err = invalid("original primary-thread handle is not open");
            self.record_error(RetainedOperationStage::OriginalHandleBorrow, &err);
            return Err(err);
        };
        self.budgets.original_handle_borrows =
            self.budgets.original_handle_borrows.saturating_add(1);
        Ok(OriginalThreadHandleBorrow {
            raw,
            _owner: PhantomData,
        })
    }

    /// Borrow the original `Job` handle within the finite borrow budget.
    pub fn borrow_original_job_handle(&mut self) -> io::Result<OriginalJobHandleBorrow<'_>> {
        if self.first_error.is_some() {
            let err = invalid("original Job handle borrow requires non-faulted owner");
            self.record_error(RetainedOperationStage::OriginalHandleBorrow, &err);
            return Err(err);
        }
        if self.budgets.original_handle_borrows >= MAX_ORIGINAL_HANDLE_BORROWS {
            let err = invalid("original handle borrow budget exhausted");
            self.record_error(RetainedOperationStage::OriginalHandleBorrow, &err);
            return Err(err);
        }
        let Some(raw) = self.job.open_raw_handle() else {
            let err = invalid("original Job handle is not open");
            self.record_error(RetainedOperationStage::OriginalHandleBorrow, &err);
            return Err(err);
        };
        self.budgets.original_handle_borrows =
            self.budgets.original_handle_borrows.saturating_add(1);
        Ok(OriginalJobHandleBorrow {
            raw,
            _owner: PhantomData,
        })
    }

    /// Pre-register an in-flight `DuplicateHandle` capability transfer into C
    /// from a lifetime-bound `BorrowedHandle` before invoking `DuplicateHandle`.
    /// Bounded to `<= 32` total slots.
    pub fn begin_bootstrap_borrowed_handle_duplication(
        &mut self,
        source_handle: BorrowedHandle<'_>,
    ) -> io::Result<usize> {
        self.begin_bootstrap_handle_duplication(source_handle.as_raw_handle() as isize)
    }

    /// Pre-register an in-flight `DuplicateHandle` capability transfer into C
    /// before the caller invokes `DuplicateHandle`. Bounded to `<= 32` total slots.
    pub fn begin_bootstrap_handle_duplication(
        &mut self,
        source_raw_handle: isize,
    ) -> io::Result<usize> {
        if !self.post_create_validated || self.first_error.is_some() {
            let err = invalid("bootstrap duplication requires validated created child");
            self.record_error(RetainedOperationStage::BootstrapHandleAccounting, &err);
            return Err(err);
        }
        if source_raw_handle <= 0 || (source_raw_handle as HANDLE) == INVALID_HANDLE_VALUE {
            let err = input("bootstrap duplication source handle is invalid or pseudo");
            self.record_error(RetainedOperationStage::BootstrapHandleAccounting, &err);
            return Err(err);
        }
        if self.bootstrap_duplication_count >= MAX_RETAINED_INHERITED_HANDLES {
            let err = input("bootstrap handle duplication exceeds 32-handle maximum");
            self.record_error(RetainedOperationStage::BootstrapHandleAccounting, &err);
            return Err(err);
        }
        let idx = self.bootstrap_duplication_count;
        self.bootstrap_duplication_count += 1;
        self.budgets.bootstrap_duplication_count =
            self.budgets.bootstrap_duplication_count.saturating_add(1);
        self.bootstrap_duplications[idx] = RetainedBootstrapDuplicationSlot {
            source_raw: source_raw_handle,
            in_flight: true,
            succeeded: false,
            failed_or_ambiguous: false,
            discharged: false,
        };
        Ok(idx)
    }

    /// Complete a pre-registered `DuplicateHandle` bootstrap slot. On failure,
    /// retains the remote duplication debt and marks `UnknownRetained` / `no_replay`.
    pub fn complete_bootstrap_handle_duplication(
        &mut self,
        slot_index: usize,
        outcome: io::Result<()>,
    ) -> io::Result<()> {
        if slot_index >= self.bootstrap_duplication_count {
            let err = input("bootstrap duplication slot index out of bounds");
            self.record_error(RetainedOperationStage::BootstrapHandleAccounting, &err);
            return Err(err);
        }
        let slot = &mut self.bootstrap_duplications[slot_index];
        if !slot.in_flight {
            let err = invalid("bootstrap duplication slot is not in flight");
            self.record_error(RetainedOperationStage::BootstrapHandleAccounting, &err);
            return Err(err);
        }
        slot.in_flight = false;
        match outcome {
            Ok(()) => {
                slot.succeeded = true;
                Ok(())
            }
            Err(err) => {
                slot.failed_or_ambiguous = true;
                self.record_error(RetainedOperationStage::BootstrapHandleAccounting, &err);
                self.phase = RetainedPhase::FaultedRetained;
                Err(err)
            }
        }
    }

    /// Resume the original primary thread at most once, recording the attempt in
    /// `resume_slot` before invoking `ResumeThread`. Keeps the original primary
    /// thread handle in `self.primary_thread` across success, failure, unexpected
    /// suspend count, or unwind; never takes-and-drops it.
    pub fn resume_primary_thread(&mut self) -> io::Result<()> {
        self.resume_primary_thread_with_ops(&mut RealRetainedNativeOps)
    }

    pub(crate) fn resume_primary_thread_with_ops(
        &mut self,
        ops: &mut impl RetainedNativeOps,
    ) -> io::Result<()> {
        if self.resume_slot.attempted || self.budgets.resume_thread_calls >= MAX_RESUME_THREAD_CALLS
        {
            let err = io::Error::new(
                io::ErrorKind::AlreadyExists,
                "primary thread resume already attempted",
            );
            self.record_error(RetainedOperationStage::ResumePrimaryThread, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        if !self.post_create_validated
            || self.first_error.is_some()
            || self.phase != RetainedPhase::CreatedSuspendedValidated
        {
            let err = io::Error::new(
                io::ErrorKind::InvalidInput,
                "primary thread resume requires a validated suspended child with no prior error",
            );
            self.record_error(RetainedOperationStage::ResumePrimaryThread, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        for i in 0..self.bootstrap_duplication_count {
            let slot = &self.bootstrap_duplications[i];
            if slot.in_flight || slot.failed_or_ambiguous {
                let err = invalid("unresolved bootstrap duplication debt blocks child resume");
                self.record_error(RetainedOperationStage::ResumePrimaryThread, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
        }
        let Some(raw_thread) = self.primary_thread.open_raw_handle() else {
            let err = invalid("original primary-thread handle is not open for resume");
            self.record_error(RetainedOperationStage::ResumePrimaryThread, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        };

        // Record attempt before invoking native ResumeThread; never take the handle out of `self`.
        self.resume_slot.attempted = true;
        self.resume_slot.call_count = self.resume_slot.call_count.saturating_add(1);
        self.resume_slot.outcome = PrimaryThreadResumeOutcome::InFlight;
        self.budgets.resume_thread_calls = self.budgets.resume_thread_calls.saturating_add(1);

        struct ResumeInFlightGuard<'a> {
            owner: &'a mut RetainedJobOwner,
            completed: bool,
        }
        impl Drop for ResumeInFlightGuard<'_> {
            fn drop(&mut self) {
                if !self.completed {
                    self.owner.resume_slot.outcome = PrimaryThreadResumeOutcome::Unwound;
                    let err = io::Error::other("ResumeThread unwound after in-flight attempt");
                    self.owner
                        .record_error(RetainedOperationStage::UnwindDuringOperation, &err);
                    self.owner.phase = RetainedPhase::FaultedRetained;
                }
            }
        }

        let mut guard = ResumeInFlightGuard {
            owner: self,
            completed: false,
        };
        let resume_result = ops.resume_thread(raw_thread);
        guard.completed = true;
        drop(guard);

        match resume_result {
            Ok(1) => {
                self.resume_slot.previous_suspend_count = Some(1);
                self.resume_slot.outcome =
                    PrimaryThreadResumeOutcome::ResumedFromSuspendCountOne;
                self.phase = RetainedPhase::Running;
                Ok(())
            }
            Ok(other_count) => {
                self.resume_slot.previous_suspend_count = Some(other_count);
                self.resume_slot.outcome =
                    PrimaryThreadResumeOutcome::UnexpectedSuspendCount(other_count);
                let err = invalid("primary thread previous suspend count was not one");
                self.record_error(RetainedOperationStage::ResumePrimaryThread, &err);
                self.phase = RetainedPhase::FaultedRetained;
                Err(err)
            }
            Err(os_code) => {
                self.resume_slot.previous_suspend_count = Some(u32::MAX);
                self.resume_slot.outcome = PrimaryThreadResumeOutcome::NativeError(os_code);
                let err = io::Error::from_raw_os_error(os_code as i32);
                self.record_error(RetainedOperationStage::ResumePrimaryThread, &err);
                self.phase = RetainedPhase::FaultedRetained;
                Err(err)
            }
        }
    }

    fn consume_observation_poll_budget(&mut self, stage: RetainedOperationStage) -> io::Result<()> {
        if self.budgets.observation_polls >= self.budgets.max_observation_polls {
            let err = io::Error::new(
                io::ErrorKind::TimedOut,
                "retained observation poll budget exhausted",
            );
            self.record_error(stage, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        self.budgets.observation_polls = self.budgets.observation_polls.saturating_add(1);
        Ok(())
    }

    /// Non-blocking observation of the original child `hProcess`.
    /// Preserves the full 32-bit exit status only after `WAIT_OBJECT_0` is observed
    /// on the original `hProcess` handle (so exit code `259` / `STILL_ACTIVE` is
    /// never confused with a running process).
    pub fn poll_process_exit(&mut self) -> io::Result<Option<u32>> {
        self.poll_process_exit_with_ops(&mut RealRetainedNativeOps)
    }

    pub(crate) fn poll_process_exit_with_ops(
        &mut self,
        ops: &mut impl RetainedNativeOps,
    ) -> io::Result<Option<u32>> {
        if self.first_error.is_some() {
            let err = invalid("cannot poll process exit on a faulted retained owner");
            self.record_error(RetainedOperationStage::ObserveProcessWait, &err);
            return Err(err);
        }
        if !matches!(
            self.phase,
            RetainedPhase::Running | RetainedPhase::ChildExitedAwaitingRetirement
        ) {
            let err = invalid("process exit observation requires a resumed running child");
            self.record_error(RetainedOperationStage::ObserveProcessWait, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        if let Some(code) = self.observations.exit_code {
            return Ok(Some(code));
        }
        self.consume_observation_poll_budget(RetainedOperationStage::ObserveProcessWait)?;
        let Some(raw_proc) = self.process.open_raw_handle() else {
            let err = invalid("original process handle is not open for wait observation");
            self.record_error(RetainedOperationStage::ObserveProcessWait, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        };
        let signaled = match ops.wait_handle_signaled_nonblocking(raw_proc) {
            Ok(v) => v,
            Err(err) => {
                self.record_error(RetainedOperationStage::ObserveProcessWait, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
        };
        if !signaled {
            return Ok(None);
        }
        self.observations.process_signaled = true;
        let code = match ops.process_exit_code(raw_proc) {
            Ok(c) => c,
            Err(err) => {
                self.record_error(RetainedOperationStage::ObserveProcessExitCode, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
        };
        self.observations.exit_code = Some(code);
        self.phase = RetainedPhase::ChildExitedAwaitingRetirement;
        Ok(Some(code))
    }

    /// Convenience wrapper returning `ExitStatus::from_raw(code)` only after signal observation.
    pub fn poll_process_exit_status(&mut self) -> io::Result<Option<ExitStatus>> {
        Ok(self.poll_process_exit()?.map(ExitStatus::from_raw))
    }

    /// Non-blocking observation of whether the original primary `hThread` is signaled.
    pub fn poll_primary_thread_signaled(&mut self) -> io::Result<bool> {
        self.poll_primary_thread_signaled_with_ops(&mut RealRetainedNativeOps)
    }

    pub(crate) fn poll_primary_thread_signaled_with_ops(
        &mut self,
        ops: &mut impl RetainedNativeOps,
    ) -> io::Result<bool> {
        if self.first_error.is_some() {
            let err = invalid("cannot poll primary thread on a faulted retained owner");
            self.record_error(RetainedOperationStage::ObservePrimaryThreadWait, &err);
            return Err(err);
        }
        if !matches!(
            self.phase,
            RetainedPhase::Running | RetainedPhase::ChildExitedAwaitingRetirement
        ) {
            let err = invalid("primary thread wait observation requires a resumed child");
            self.record_error(RetainedOperationStage::ObservePrimaryThreadWait, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        if self.observations.thread_signaled {
            return Ok(true);
        }
        self.consume_observation_poll_budget(RetainedOperationStage::ObservePrimaryThreadWait)?;
        let Some(raw_thread) = self.primary_thread.open_raw_handle() else {
            let err = invalid("original primary-thread handle is not open for wait observation");
            self.record_error(RetainedOperationStage::ObservePrimaryThreadWait, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        };
        match ops.wait_handle_signaled_nonblocking(raw_thread) {
            Ok(signaled) => {
                if signaled {
                    self.observations.thread_signaled = true;
                }
                Ok(signaled)
            }
            Err(err) => {
                self.record_error(RetainedOperationStage::ObservePrimaryThreadWait, &err);
                self.phase = RetainedPhase::FaultedRetained;
                Err(err)
            }
        }
    }

    /// Query the original `Job`'s current `ActiveProcesses` count.
    /// `post_exit_job_empty_observed` becomes `true` only when `active == 0` is
    /// observed after both the original `hProcess` and `hThread` have signaled.
    pub fn poll_job_active_processes(&mut self) -> io::Result<u32> {
        self.poll_job_active_processes_with_ops(&mut RealRetainedNativeOps)
    }

    pub(crate) fn poll_job_active_processes_with_ops(
        &mut self,
        ops: &mut impl RetainedNativeOps,
    ) -> io::Result<u32> {
        if self.first_error.is_some() {
            let err = invalid("cannot query Job accounting on a faulted retained owner");
            self.record_error(RetainedOperationStage::ObserveJobAccounting, &err);
            return Err(err);
        }
        self.consume_observation_poll_budget(RetainedOperationStage::ObserveJobAccounting)?;
        let Some(raw_job) = self.job.open_raw_handle() else {
            let err = invalid("original Job handle is not open for accounting observation");
            self.record_error(RetainedOperationStage::ObserveJobAccounting, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        };
        match ops.job_active_processes(raw_job) {
            Ok(active) => {
                self.observations.last_active_processes = Some(active);
                if active == 0
                    && self.observations.process_signaled
                    && self.observations.exit_code.is_some()
                    && self.observations.thread_signaled
                {
                    self.observations.post_exit_job_empty_observed = true;
                }
                Ok(active)
            }
            Err(err) => {
                self.record_error(RetainedOperationStage::ObserveJobAccounting, &err);
                self.phase = RetainedPhase::FaultedRetained;
                Err(err)
            }
        }
    }

    /// Perform one combined non-blocking observation pass of process exit,
    /// primary thread signal, and Job active-process accounting.
    pub fn poll_lifecycle_observations(&mut self) -> io::Result<RetainedKernelObservations> {
        self.poll_lifecycle_observations_with_ops(&mut RealRetainedNativeOps)
    }

    pub(crate) fn poll_lifecycle_observations_with_ops(
        &mut self,
        ops: &mut impl RetainedNativeOps,
    ) -> io::Result<RetainedKernelObservations> {
        let _ = self.poll_process_exit_with_ops(ops)?;
        let _ = self.poll_primary_thread_signaled_with_ops(ops)?;
        let _ = self.poll_job_active_processes_with_ops(ops)?;
        Ok(self.observations)
    }

    /// Record that the caller has discharged its parent-side obligations for all
    /// borrowed `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` entries (`stdin`, `stdout`,
    /// `stderr`, and any `extra_inherited` prebirth/bootstrap handles) supplied to
    /// `create_suspended_prepared_host_with_stdio`.
    pub fn confirm_borrowed_stdio_discharged(&mut self) -> io::Result<()> {
        if self.first_error.is_some() {
            let err = invalid("cannot discharge borrowed stdio on a faulted owner");
            self.record_error(RetainedOperationStage::OuterDebtVerification, &err);
            return Err(err);
        }
        if !self.post_create_validated || self.borrowed_stdio_count < STDIO_HANDLE_COUNT {
            let err = invalid("owner has no validated borrowed stdio handles to discharge");
            self.record_error(RetainedOperationStage::OuterDebtVerification, &err);
            return Err(err);
        }
        for i in 0..self.borrowed_stdio_count {
            self.borrowed_stdio[i].caller_parent_copy_discharged = true;
        }
        Ok(())
    }

    /// Record that all succeeded `DuplicateHandle` bootstrap transfers into C
    /// have completed their lifecycle after C exited.
    pub fn confirm_bootstrap_duplications_discharged(&mut self) -> io::Result<()> {
        if self.first_error.is_some() {
            let err = invalid("cannot discharge bootstrap duplications on a faulted owner");
            self.record_error(RetainedOperationStage::OuterDebtVerification, &err);
            return Err(err);
        }
        if !self.observations.process_signaled || self.observations.exit_code.is_none() {
            let err = invalid(
                "bootstrap handle duplication debt can only be discharged after child exit signal",
            );
            self.record_error(RetainedOperationStage::OuterDebtVerification, &err);
            return Err(err);
        }
        for i in 0..self.bootstrap_duplication_count {
            let slot = &mut self.bootstrap_duplications[i];
            if slot.in_flight || slot.failed_or_ambiguous || !slot.succeeded {
                let err = invalid("unresolved or failed bootstrap duplication cannot be discharged");
                self.record_error(RetainedOperationStage::OuterDebtVerification, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
            slot.discharged = true;
        }
        Ok(())
    }

    /// Record outer P lifecycle evidence (kept distinct from kernel observations).
    pub fn record_outer_lifecycle_evidence(
        &mut self,
        evidence: OuterLifecycleEvidence,
    ) -> io::Result<()> {
        if self.first_error.is_some() {
            let err = invalid("cannot record outer lifecycle evidence on a faulted owner");
            self.record_error(RetainedOperationStage::OuterDebtVerification, &err);
            return Err(err);
        }
        self.outer_evidence = Some(evidence);
        Ok(())
    }

    fn all_stdio_and_bootstrap_debt_discharged(&self) -> bool {
        for i in 0..self.bootstrap_duplication_count {
            let slot = &self.bootstrap_duplications[i];
            if slot.in_flight || slot.failed_or_ambiguous || !slot.succeeded || !slot.discharged {
                return false;
            }
        }
        self.borrowed_stdio_count >= STDIO_HANDLE_COUNT
            && (0..self.borrowed_stdio_count).all(|i| {
                self.borrowed_stdio[i].inherited_in_create_attempt
                    && self.borrowed_stdio[i].caller_parent_copy_discharged
            })
    }

    /// True only when all real kernel observations, borrowed/bootstrap handle accounting,
    /// and separate outer P lifecycle evidence permit retiring the retained handles.
    pub fn can_retire_handles(&self) -> bool {
        self.first_error.is_none()
            && self.disposition == RetainedDisposition::ActivePending
            && self.phase == RetainedPhase::ChildExitedAwaitingRetirement
            && self.create_attempted
            && self.post_create_validated
            && self.resume_slot.outcome == PrimaryThreadResumeOutcome::ResumedFromSuspendCountOne
            && self.observations.process_signaled
            && self.observations.exit_code.is_some()
            && self.observations.thread_signaled
            && self.observations.post_exit_job_empty_observed
            && self.observations.last_active_processes == Some(0)
            && self.primary_thread.is_open()
            && self.process.is_open()
            && self.job.is_open()
            && self.all_stdio_and_bootstrap_debt_discharged()
            && self
                .outer_evidence
                .as_ref()
                .is_some_and(OuterLifecycleEvidence::all_discharged)
    }

    /// Retire all owned kernel handles (`primary_thread`, `process`, then `job`)
    /// using individually checked `CloseHandle` calls.
    ///
    /// Before each `CloseHandle`, the slot records `CloseInFlight`. On any close
    /// failure or unwind, the exact raw handle debt, `first_error`, and `no_replay`
    /// are retained and no remaining or ambiguous handles are retried.
    pub fn retire_handles(&mut self) -> io::Result<RetainedRetirementReceipt> {
        self.retire_handles_with_ops(&mut RealRetainedNativeOps)
    }

    pub(crate) fn retire_handles_with_ops(
        &mut self,
        ops: &mut impl RetainedNativeOps,
    ) -> io::Result<RetainedRetirementReceipt> {
        if self.budgets.retire_attempts >= 1 {
            let err = io::Error::new(
                io::ErrorKind::AlreadyExists,
                "retained handle retirement already attempted",
            );
            self.record_error(RetainedOperationStage::OuterDebtVerification, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }
        if !self.can_retire_handles() {
            let err = io::Error::new(
                io::ErrorKind::WouldBlock,
                "retirement preconditions (signaled process/thread, post-exit empty Job, and outer debt discharge) are not satisfied",
            );
            self.record_error(RetainedOperationStage::OuterDebtVerification, &err);
            self.phase = RetainedPhase::FaultedRetained;
            return Err(err);
        }

        self.budgets.retire_attempts = self.budgets.retire_attempts.saturating_add(1);
        let identity = self.verified_identity()?;
        let exit_code = self.observations.exit_code.unwrap();
        let outer_evidence = self.outer_evidence.unwrap();

        struct RetireInFlightGuard<'a> {
            owner: &'a mut RetainedJobOwner,
            completed: bool,
        }
        impl Drop for RetireInFlightGuard<'_> {
            fn drop(&mut self) {
                if !self.completed {
                    let err = io::Error::other("CloseHandle unwound during handle retirement");
                    self.owner
                        .record_error(RetainedOperationStage::UnwindDuringOperation, &err);
                    self.owner.phase = RetainedPhase::FaultedRetained;
                }
            }
        }

        let mut guard = RetireInFlightGuard {
            owner: self,
            completed: false,
        };

        // 1. Close original primary thread handle first.
        guard.owner.budgets.close_handle_calls =
            guard.owner.budgets.close_handle_calls.saturating_add(1);
        let closed_primary_thread_raw = match guard.owner.primary_thread.close_once(ops) {
            Ok(raw) => raw,
            Err(err) => {
                guard.completed = true;
                drop(guard);
                self.record_error(RetainedOperationStage::ClosePrimaryThread, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
        };

        // 2. Close original child process handle second.
        guard.owner.budgets.close_handle_calls =
            guard.owner.budgets.close_handle_calls.saturating_add(1);
        let closed_process_raw = match guard.owner.process.close_once(ops) {
            Ok(raw) => raw,
            Err(err) => {
                guard.completed = true;
                drop(guard);
                self.record_error(RetainedOperationStage::CloseProcess, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
        };

        // 3. Close original Job handle last, after child process/thread are closed and Job is empty.
        guard.owner.budgets.close_handle_calls =
            guard.owner.budgets.close_handle_calls.saturating_add(1);
        let closed_job_raw = match guard.owner.job.close_once(ops) {
            Ok(raw) => raw,
            Err(err) => {
                guard.completed = true;
                drop(guard);
                self.record_error(RetainedOperationStage::CloseJob, &err);
                self.phase = RetainedPhase::FaultedRetained;
                return Err(err);
            }
        };

        guard.completed = true;
        drop(guard);

        self.phase = RetainedPhase::Retired;
        self.disposition = RetainedDisposition::RetiredCleanly;
        Ok(RetainedRetirementReceipt {
            identity,
            exit_code,
            kernel_observations: self.observations,
            outer_evidence,
            closed_primary_thread_raw,
            closed_process_raw,
            closed_job_raw,
        })
    }

    pub fn phase(&self) -> RetainedPhase {
        self.phase
    }

    pub fn disposition(&self) -> RetainedDisposition {
        self.disposition
    }

    pub fn no_replay(&self) -> bool {
        self.no_replay
    }

    pub fn first_error(&self) -> Option<&RetainedErrorRecord> {
        self.first_error.as_ref()
    }

    pub fn last_error(&self) -> Option<&RetainedErrorRecord> {
        self.last_error.as_ref()
    }

    pub fn error_count(&self) -> u32 {
        self.error_count
    }

    pub fn job_handle_slot(&self) -> RetainedHandleSlot {
        self.job
    }

    pub fn process_handle_slot(&self) -> RetainedHandleSlot {
        self.process
    }

    pub fn primary_thread_handle_slot(&self) -> RetainedHandleSlot {
        self.primary_thread
    }

    pub fn raw_process_information(&self) -> RetainedRawProcessInformation {
        self.raw_pi
    }

    pub fn resume_slot(&self) -> PrimaryThreadResumeSlot {
        self.resume_slot
    }

    pub fn kernel_observations(&self) -> RetainedKernelObservations {
        self.observations
    }

    pub fn outer_lifecycle_evidence(&self) -> Option<OuterLifecycleEvidence> {
        self.outer_evidence
    }

    pub fn call_budgets(&self) -> RetainedCallBudgets {
        self.budgets
    }

    pub fn borrowed_stdio_slots(&self) -> &[RetainedBorrowedHandleSlot] {
        &self.borrowed_stdio[..self.borrowed_stdio_count]
    }

    pub fn bootstrap_duplication_slots(&self) -> &[RetainedBootstrapDuplicationSlot] {
        &self.bootstrap_duplications[..self.bootstrap_duplication_count]
    }

    /// True if any owned handle slot, borrowed inherited handle, or bootstrap duplication
    /// has unresolved kernel debt.
    pub fn has_unresolved_debt(&self) -> bool {
        self.job.has_unresolved_debt()
            || self.process.has_unresolved_debt()
            || self.primary_thread.has_unresolved_debt()
            || (0..self.borrowed_stdio_count).any(|i| {
                self.borrowed_stdio[i].inherited_in_create_attempt
                    && !self.borrowed_stdio[i].caller_parent_copy_discharged
            })
            || (0..self.bootstrap_duplication_count).any(|i| {
                let s = &self.bootstrap_duplications[i];
                s.in_flight || s.failed_or_ambiguous || (s.succeeded && !s.discharged)
            })
    }
}

impl Drop for RetainedJobOwner {
    fn drop(&mut self) {
        // Never invoke TerminateProcess, TerminateJobObject, or implicit CloseHandle
        // on any open, in-flight, or failed handle slot.
        // If an unwind occurred while the attribute list was initialized before
        // CreateProcessW returned, retain the attribute storage and handle arrays.
        if self.attribute_list_initialized && !self.attribute_list_deleted {
            let storage = std::mem::take(&mut self.attribute_storage);
            std::mem::forget(storage);
        }
    }
}

#[cfg(test)]
#[path = "job_process_retained_tests.rs"]
mod job_process_retained_tests;

