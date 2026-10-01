//! Owned Windows lifecycle handles. Successful queries are current kernel observations,
//! not durable evidence of cleanup after the supervisor or kernel stops.

use std::{io, mem::size_of, os::windows::io::OwnedHandle, ptr};
use windows_sys::Win32::{
    Foundation::{
        DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_NO_MORE_FILES, GetHandleInformation, HANDLE,
        HANDLE_FLAG_INHERIT, FILETIME, INVALID_HANDLE_VALUE, SetHandleInformation, WAIT_FAILED,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::SECURITY_ATTRIBUTES,
    System::{
        Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
            JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
            JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
            TerminateJobObject,
        },
        Threading::{
            GetCurrentProcess, GetProcessTimes, GetProcessId, GetProcessIdOfThread, GetThreadId, OpenProcess,
            OpenThread, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, ResumeThread,
            THREAD_QUERY_LIMITED_INFORMATION, THREAD_SUSPEND_RESUME, TerminateProcess,
            WaitForSingleObject,
        },
    },
};

use super::{own, raw};

pub use super::job_process::{JobChild, JobSpawnOutcome, spawn_error_outcome};
pub use super::job_stdin::{JobStdin, JobStdinProof};

/// Immediate watchdog termination without Rust's process-exit stdio cleanup.
pub fn terminate_current_process(code: u32) -> ! {
    // SAFETY: the current-process pseudo-handle is valid for termination. The OS
    // terminates this process directly; no borrowed storage or destructor is involved.
    unsafe { TerminateProcess(GetCurrentProcess(), code) };
    // A successful self-termination cannot return. Abort is the non-cleanup fallback
    // if the kernel call fails, and also covers an unexpected successful return.
    std::process::abort()
}

/// Seal this supervisor's existing control stdio before spawning children. The caller
/// must serialize this startup step with child creation and standard-handle changes.
/// These handles remain borrowed: this function never owns, replaces or closes them.
pub fn seal_standard_handles_noninherit() -> io::Result<()> {
    for selector in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: GetStdHandle takes no pointers; the process retains handle ownership.
        let handle = unsafe { GetStdHandle(selector) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        if handle.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "supervisor standard handle is absent",
            ));
        }
        // SAFETY: this only changes a handle-table flag; Windows validates the borrowed
        // handle, and no ownership is transferred to Rust.
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut flags = 0;
        // SAFETY: flags is valid writable storage; the API validates handle.
        if unsafe { GetHandleInformation(handle, &mut flags) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if flags & HANDLE_FLAG_INHERIT != 0 {
            return Err(invalid("supervisor standard handle remains inheritable"));
        }
    }
    Ok(())
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn inheritable(handle: &OwnedHandle) -> io::Result<bool> {
    let mut flags = 0;
    // SAFETY: handle remains owned for the call and flags is writable storage.
    if unsafe { GetHandleInformation(raw(handle), &mut flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(flags & HANDLE_FLAG_INHERIT != 0)
}

fn noninherited(handle: OwnedHandle) -> io::Result<OwnedHandle> {
    // SAFETY: handle is owned; only the inheritance bit is changed.
    if unsafe { SetHandleInformation(raw(&handle), HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if inheritable(&handle)? {
        return Err(invalid("kernel returned an inheritable lifecycle handle"));
    }
    Ok(handle)
}

fn verify_process(handle: &OwnedHandle, expected_pid: u32) -> io::Result<()> {
    // SAFETY: the owned handle stays valid; the API validates its object type.
    let actual = unsafe { GetProcessId(raw(handle)) };
    if actual == 0 {
        return Err(io::Error::last_os_error());
    }
    if actual != expected_pid {
        return Err(invalid("process handle does not match expected PID"));
    }
    Ok(())
}

/// Creation identity is read from the held process object, never from a PID scan.
pub(crate) fn process_creation_filetime(handle: &OwnedHandle) -> io::Result<u64> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: handle is owned throughout; all four FILETIMEs are writable storage.
    if unsafe { GetProcessTimes(raw(handle), &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let value = ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64;
    if value == 0 { return Err(invalid("process creation identity missing")); }
    Ok(value)
}

fn exited(handle: &OwnedHandle) -> io::Result<bool> {
    // SAFETY: handle is owned and the zero timeout never blocks.
    match unsafe { WaitForSingleObject(raw(handle), 0) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err(io::Error::last_os_error()),
        _ => Err(invalid("unexpected process wait result")),
    }
}

/// An anonymous Job with one noninherited owner and no breakaway permissions.
/// Closing this handle requests termination of all remaining Job members.
pub struct Job {
    handle: OwnedHandle,
}

impl Job {
    /// Create a child already assigned to this Job, retaining its real primary thread.
    /// Requires an entered Tokio runtime with I/O enabled. The caller keeps this Job
    /// alive while owning the child; this path never falls back to spawn-then-assign.
    pub fn spawn_suspended(
        &self,
        exe: &std::path::Path,
        args: &[String],
        cwd: &std::path::Path,
        environment: &std::collections::BTreeMap<String, String>,
    ) -> io::Result<JobChild> {
        super::job_process::spawn(self, &self.handle, exe, args, cwd, environment, false)
    }

    /// Trusted workbench host only. Adds the current token's native local app
    /// data folder for its existing service-authority locks. Guest launch remains
    /// three-key-only; caller-supplied profile/environment overrides are refused.
    pub fn spawn_suspended_trusted_host(
        &self,exe:&std::path::Path,args:&[String],cwd:&std::path::Path,
        environment:&std::collections::BTreeMap<String,String>,
    )->io::Result<JobChild>{super::job_process::spawn(self,&self.handle,exe,args,cwd,environment,true)}

    pub fn new() -> io::Result<Self> {
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: ptr::null_mut(),
            bInheritHandle: 0,
        };
        // SAFETY: security is valid for the call; a null name creates a fresh anonymous Job.
        let handle = noninherited(own(unsafe { CreateJobObjectW(&security, ptr::null()) })?)?;
        let job = Self { handle };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: limits has the exact information class layout and remains live for the call.
        if unsafe {
            SetInformationJobObject(
                raw(&job.handle),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        job.verify_limits()?;
        Ok(job)
    }

    /// Attach the caller's live CREATE_SUSPENDED child, then resume its sole thread once.
    /// The caller must retain the child handle while entering this method. A duplicate
    /// pins the process object throughout the operation; this method never takes ownership
    /// of the supplied handle. Any error can follow process creation or Job assignment,
    /// and the caller must kill/wait the child rather than classify it as never started.
    pub fn attach_and_resume(
        &self,
        child_process_raw_handle: isize,
        expected_pid: u32,
    ) -> io::Result<()> {
        let borrowed = child_process_raw_handle as HANDLE;
        // Negative values are pseudo-handles, not a spawned child's real process handle.
        if expected_pid == 0 || child_process_raw_handle <= 0 || borrowed == INVALID_HANDLE_VALUE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid child handle or PID",
            ));
        }
        let mut duplicate = ptr::null_mut();
        // SAFETY: no Rust memory is referenced by borrowed. DuplicateHandle validates the
        // handle and returns an independent owned reference, without closing the source.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                borrowed,
                GetCurrentProcess(),
                &mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let process = noninherited(own(duplicate)?)?;
        verify_process(&process, expected_pid)?;
        if exited(&process)? {
            return Err(invalid("suspended child has already exited"));
        }
        self.verify_limits()?;
        if self.inheritable()? {
            return Err(invalid("Job handle became inheritable"));
        }
        // SAFETY: both process and Job handles remain owned throughout the call.
        if unsafe { AssignProcessToJobObject(raw(&self.handle), raw(&process)) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut member = 0;
        // SAFETY: member is writable BOOL storage and both handles remain valid.
        if unsafe { IsProcessInJob(raw(&process), raw(&self.handle), &mut member) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if member == 0 {
            return Err(invalid("assigned child is not a member of this Job"));
        }
        let thread_id = sole_thread(expected_pid)?;
        // SAFETY: OpenThread validates the snapshot ID and returns a new owned handle.
        let thread = noninherited(own(unsafe {
            OpenThread(
                THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
                0,
                thread_id,
            )
        })?)?;
        // SAFETY: thread is owned; these APIs validate its object identity.
        let actual_pid = unsafe { GetProcessIdOfThread(raw(&thread)) };
        if actual_pid == 0 {
            return Err(io::Error::last_os_error());
        }
        if actual_pid != expected_pid {
            return Err(invalid(
                "snapshot thread no longer belongs to expected child",
            ));
        }
        // SAFETY: thread is owned for the call.
        let actual_thread = unsafe { GetThreadId(raw(&thread)) };
        if actual_thread == 0 {
            return Err(io::Error::last_os_error());
        }
        if actual_thread != thread_id || exited(&process)? {
            return Err(invalid("child identity changed before resume"));
        }
        // SAFETY: the owned thread handle has THREAD_SUSPEND_RESUME. Call exactly once:
        // values other than one require caller cleanup, never an automatic resume loop.
        match unsafe { ResumeThread(raw(&thread)) } {
            1 => Ok(()),
            u32::MAX => Err(io::Error::last_os_error()),
            _ => Err(invalid(
                "child primary thread previous suspend count was not one",
            )),
        }
    }

    /// Request termination. Success is not a wait or proof of an empty Job.
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: this owned Job handle grants termination access.
        if unsafe { TerminateJobObject(raw(&self.handle), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Read the kernel's current Job accounting, including descendants. Zero does not
    /// substitute for waiting the original child handle or durable cleanup evidence.
    pub fn active_processes(&self) -> io::Result<u32> {
        let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        let mut returned = 0;
        // SAFETY: accounting and returned are writable, correctly sized storage.
        if unsafe {
            QueryInformationJobObject(
                raw(&self.handle),
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

    pub fn inheritable(&self) -> io::Result<bool> {
        inheritable(&self.handle)
    }

    /// Read actual extended limit flags for local inspection.
    pub fn limit_flags(&self) -> io::Result<u32> {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let mut returned = 0;
        // SAFETY: limits and returned are writable, correctly sized storage.
        if unsafe {
            QueryInformationJobObject(
                raw(&self.handle),
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
        Ok(limits.BasicLimitInformation.LimitFlags)
    }

    fn verify_limits(&self) -> io::Result<()> {
        let flags = self.limit_flags()?;
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
}

fn sole_thread(expected_pid: u32) -> io::Result<u32> {
    // SAFETY: snapshot creation has no borrowed memory and returns a new owned handle.
    let snapshot = noninherited(own(unsafe {
        CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
    })?)?;
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    // SAFETY: entry is writable and advertises its actual size.
    if unsafe { Thread32First(raw(&snapshot), &mut entry) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut found = None;
    loop {
        if entry.dwSize < size_of::<THREADENTRY32>() as u32 {
            return Err(invalid("incomplete thread snapshot entry"));
        }
        if entry.th32OwnerProcessID == expected_pid {
            if found.replace(entry.th32ThreadID).is_some() {
                return Err(invalid(
                    "CREATE_SUSPENDED child does not have exactly one thread",
                ));
            }
        }
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        // SAFETY: snapshot is owned and entry remains writable with its actual size.
        if unsafe { Thread32Next(raw(&snapshot), &mut entry) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
                return Err(error);
            }
            break;
        }
    }
    found
        .filter(|id| *id != 0)
        .ok_or_else(|| invalid("suspended child primary thread was not found"))
}

/// A process identity held by a real kernel handle. After open, PID reuse cannot
/// redirect observations to a different process. An open failure is not exit proof.
pub struct ProcessWatch {
    handle: OwnedHandle,
}

impl ProcessWatch {
    pub fn creation_filetime(&self) -> io::Result<u64> {
        process_creation_filetime(&self.handle)
    }

    pub fn open(pid: u32) -> io::Result<Self> {
        if pid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "watch PID must be nonzero",
            ));
        }
        // SAFETY: OpenProcess validates pid and returns a new, noninherited handle.
        let handle = noninherited(own(unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            )
        })?)?;
        verify_process(&handle, pid)?;
        Ok(Self { handle })
    }

    pub fn is_exited(&self) -> io::Result<bool> {
        exited(&self.handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_job_reads_back_containment_and_empty_accounting() {
        let job = Job::new().expect("create anonymous Job");
        assert!(!job.inheritable().expect("read Job handle flags"));
        let flags = job.limit_flags().expect("read extended Job limits");
        assert_eq!(flags, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE);
        assert_eq!(
            flags & (JOB_OBJECT_LIMIT_BREAKAWAY_OK | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK),
            0
        );
        assert_eq!(job.active_processes().expect("read Job accounting"), 0);
    }

    #[test]
    fn process_watch_holds_noninherited_live_process_handle() {
        let watch = ProcessWatch::open(std::process::id()).expect("open current process watch");
        assert!(!inheritable(&watch.handle).expect("read watch handle flags"));
        assert!(!watch.is_exited().expect("wait current process handle"));
        verify_process(&watch.handle, std::process::id()).expect("validate watched identity");
    }

    #[test]
    fn invalid_pid_and_child_handles_are_rejected_without_assignment() {
        let error = ProcessWatch::open(0).err().expect("reject zero watch PID");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        let job = Job::new().expect("create anonymous Job");
        for handle in [0, -1, -2] {
            assert_eq!(
                job.attach_and_resume(handle, std::process::id())
                    .expect_err("reject invalid or pseudo process handle")
                    .kind(),
                io::ErrorKind::InvalidInput
            );
        }
        let watch = ProcessWatch::open(std::process::id()).expect("open current process watch");
        assert_eq!(
            job.attach_and_resume(raw(&watch.handle) as isize, 0)
                .expect_err("reject zero expected PID")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            job.active_processes().expect("read empty Job accounting"),
            0
        );
    }

    #[test]
    fn wrong_expected_pid_is_rejected_before_job_assignment_or_resume() {
        let pid = std::process::id();
        let wrong_pid = pid.checked_add(1).unwrap_or(pid - 1);
        let watch = ProcessWatch::open(pid).expect("open current process watch");
        let job = Job::new().expect("create anonymous Job");
        let error = job
            .attach_and_resume(raw(&watch.handle) as isize, wrong_pid)
            .expect_err("reject handle identity mismatch before assigning test process");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(
            error.to_string(),
            "process handle does not match expected PID"
        );
        let mut member = 0;
        // SAFETY: both handles are owned and member is writable BOOL storage.
        assert_ne!(
            unsafe { IsProcessInJob(raw(&watch.handle), raw(&job.handle), &mut member) },
            0
        );
        assert_eq!(member, 0, "test process must never enter the test Job");
        assert_eq!(
            job.active_processes().expect("read empty Job accounting"),
            0
        );
        assert!(!watch.is_exited().expect("test process remains running"));
    }
}

/// Irreversible, process-independent business revocation. No reset or guest
/// grant API is exposed. The name is bound to a fresh owner incarnation.
#[derive(Clone)]
pub struct RevocationEvent {handle:std::sync::Arc<OwnedHandle>}
impl RevocationEvent {
 fn name(incarnation:&str)->io::Result<Vec<u16>>{if incarnation.len()!=64||!incarnation.bytes().all(|v|v.is_ascii_hexdigit()){return Err(invalid("revocation event identity"));}Ok(format!("Local\\MorrowWorkbenchRevocation-{incarnation}").encode_utf16().chain(Some(0)).collect())}
 pub fn create(incarnation:&str)->io::Result<Self>{let name=Self::name(incarnation)?;
  // SAFETY: name is a live null-terminated UTF-16 string; no handle inheritance.
  unsafe{windows_sys::Win32::Foundation::SetLastError(0)};
  let handle=unsafe{windows_sys::Win32::System::Threading::CreateEventW(ptr::null(),1,0,name.as_ptr())};
  let last=io::Error::last_os_error();let handle=noninherited(own(handle)?)?;
  if last.raw_os_error()==Some(windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS as i32){return Err(invalid("revocation event already exists"));}
  Ok(Self{handle:std::sync::Arc::new(handle)})
 }
 pub fn open(incarnation:&str)->io::Result<Self>{let name=Self::name(incarnation)?;
  // SAFETY: name is live and access is wait-only; Windows validates the object.
  let handle=unsafe{windows_sys::Win32::System::Threading::OpenEventW(0x00100000,0,name.as_ptr())};
  Ok(Self{handle:std::sync::Arc::new(noninherited(own(handle)?)?)})
 }
 pub fn revoke(&self)->io::Result<()>{
  // SAFETY: handle is owned for this call and denotes a manual-reset event.
  if unsafe{windows_sys::Win32::System::Threading::SetEvent(raw(&self.handle))}==0{Err(io::Error::last_os_error())}else{Ok(())}
 }
 pub fn is_revoked(&self)->io::Result<bool>{exited(&self.handle)}
}
