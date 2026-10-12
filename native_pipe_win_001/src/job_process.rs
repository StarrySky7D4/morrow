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
fn prepare_trusted_host_environment_with(environment: &BTreeMap<String, String>,
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
