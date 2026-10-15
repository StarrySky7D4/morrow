use filedescriptor::OwnedHandle;
use std::collections::BTreeSet;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::FromRawHandle;
use std::os::windows::io::RawHandle;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use tokio::process::Child;
use tokio::process::Command;
use winapi::shared::ntdef::NT_SUCCESS;
use winapi::shared::ntdef::NTSTATUS;
use winapi::shared::winerror::ERROR_MORE_DATA;
use winapi::um::jobapi::IsProcessInJob;
use winapi::um::jobapi2::AssignProcessToJobObject;
use winapi::um::jobapi2::CreateJobObjectW;
use winapi::um::jobapi2::QueryInformationJobObject;
use winapi::um::jobapi2::SetInformationJobObject;
use winapi::um::jobapi2::TerminateJobObject;
use winapi::um::processthreadsapi::OpenProcess;
use winapi::um::processthreadsapi::TerminateProcess;
use winapi::um::winbase::CREATE_SUSPENDED;
use winapi::um::winnt::HANDLE;
use winapi::um::winnt::JOB_OBJECT_LIMIT_BREAKAWAY_OK;
use winapi::um::winnt::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
use winapi::um::winnt::JOBOBJECT_BASIC_ACCOUNTING_INFORMATION;
use winapi::um::winnt::JOBOBJECT_BASIC_PROCESS_ID_LIST;
use winapi::um::winnt::JOBOBJECT_EXTENDED_LIMIT_INFORMATION;
use winapi::um::winnt::JobObjectBasicAccountingInformation;
use winapi::um::winnt::JobObjectBasicProcessIdList;
use winapi::um::winnt::JobObjectExtendedLimitInformation;
use winapi::um::winnt::PROCESS_SET_QUOTA;
use winapi::um::winnt::PROCESS_SUSPEND_RESUME;
use winapi::um::winnt::PROCESS_TERMINATE;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process_handle: HANDLE) -> NTSTATUS;
}

const INITIAL_JOB_PID_CAPACITY: usize = 64;
const MAX_JOB_PID_CAPACITY: usize = 1024;
const PROCESS_ID_LIST_HEADER_BYTES: usize =
    std::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList);

/// Raw observation from `QueryInformationJobObject` for accounting and process ID list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawJobObservation {
    pub accounting_ok: bool,
    pub accounting_return_len: usize,
    pub total_processes: u32,
    pub active_processes: u32,
    pub total_terminated_processes: u32,
    pub process_list_ok: bool,
    pub process_list_return_len: usize,
    pub assigned_processes: u32,
    pub listed_processes: u32,
    pub process_ids: Vec<usize>,
}

/// Validated observation of a Job Object's accounting and active member list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedJobObservation {
    pub total_processes: u32,
    pub active_processes: u32,
    pub total_terminated_processes: u32,
    pub process_ids: Vec<usize>,
}

impl RawJobObservation {
    /// Strict post-launch API: a launched Job must retain a nonzero actual total.
    pub(crate) fn validate(self) -> io::Result<ValidatedJobObservation> {
        let observed = self.validate_shape()?;
        if observed.total_processes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "job accounting reported zero total processes after launch",
            ));
        }
        observed.validate_counters()
    }

    /// Separate operation for callers holding positive producer evidence that no
    /// original process was created. No nonempty or previously used Job qualifies.
    pub(crate) fn validate_not_created(self) -> io::Result<ValidatedJobObservation> {
        let observed = self.validate_shape()?.validate_counters()?;
        if observed.total_processes != 0 || observed.active_processes != 0
            || observed.total_terminated_processes != 0 || !observed.process_ids.is_empty()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not-created Job observation contains actual process history or members",
            ));
        }
        Ok(observed)
    }

    fn validate_shape(self) -> io::Result<ValidatedJobObservation> {
        if !self.accounting_ok {
            return Err(io::Error::other(
                "QueryInformationJobObject(JobObjectBasicAccountingInformation) failed",
            ));
        }
        if self.accounting_return_len != std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "malformed job accounting return length: {}",
                    self.accounting_return_len
                ),
            ));
        }
        if !self.process_list_ok {
            return Err(io::Error::other(
                "QueryInformationJobObject(JobObjectBasicProcessIdList) failed",
            ));
        }
        if self.assigned_processes != self.listed_processes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "job process list count mismatch: assigned={} listed={}",
                    self.assigned_processes, self.listed_processes
                ),
            ));
        }
        let expected_list_len = PROCESS_ID_LIST_HEADER_BYTES
            .checked_add(
                (self.listed_processes as usize)
                    .checked_mul(std::mem::size_of::<usize>())
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "job process list size overflow")
                    })?,
            )
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "job process list size overflow")
            })?;
        let valid_list_return_len = if self.listed_processes == 0 {
            self.process_list_return_len == PROCESS_ID_LIST_HEADER_BYTES
                || self.process_list_return_len
                    == std::mem::size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>()
        } else {
            self.process_list_return_len == expected_list_len
        };
        if !valid_list_return_len || self.process_ids.len() != self.listed_processes as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "malformed job process list return shape: return_len={} listed={} vec_len={}",
                    self.process_list_return_len,
                    self.listed_processes,
                    self.process_ids.len()
                ),
            ));
        }
        let mut unique_pids = BTreeSet::new();
        for &pid in &self.process_ids {
            if pid == 0 || !unique_pids.insert(pid) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("malformed or duplicate process ID in job list: {pid}"),
                ));
            }
        }
        Ok(ValidatedJobObservation {
            total_processes: self.total_processes,
            active_processes: self.active_processes,
            total_terminated_processes: self.total_terminated_processes,
            process_ids: self.process_ids,
        })
    }
}

// Count checks follow the launched API's zero-total check, preserving its
// original diagnostic ordering. Shape validation already equates assigned,
// listed and vector lengths, so the actual vector length is the assigned count.
impl ValidatedJobObservation {
    fn validate_counters(self) -> io::Result<Self> {
        if self.active_processes > self.total_processes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "job accounting active_processes ({}) exceeds total_processes ({})",
                    self.active_processes, self.total_processes
                ),
            ));
        }
        if self.total_terminated_processes > self.total_processes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "job accounting total_terminated_processes ({}) exceeds total_processes ({})",
                    self.total_terminated_processes, self.total_processes
                ),
            ));
        }
        if self.active_processes != (self.process_ids.len() as u32) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "job accounting active_processes ({}) disagrees with process list ({})",
                    self.active_processes, self.process_ids.len() as u32
                ),
            ));
        }
        Ok(self)
    }
}

/// Owns a Windows Job Object used to terminate a spawned process tree.
#[derive(Debug)]
pub struct JobObject {
    handle: OwnedHandle,
    // A mutex makes the state check, Job Object API call, and state update
    // atomic with respect to concurrent preserve and terminate requests.
    preserve_descendants: Mutex<bool>,
    terminate_invoked: AtomicBool,
}

impl JobObject {
    /// Creates a Job Object configured to terminate all members when its last handle closes.
    pub fn create() -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle.cast()) };

        Self::set_limit_flags(
            &handle,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK,
        )?;

        Ok(Self {
            handle,
            preserve_descendants: Mutex::new(false),
            terminate_invoked: AtomicBool::new(false),
        })
    }

    /// Creates a Job Object whose owned descendants cannot explicitly break away.
    pub fn create_without_breakaway() -> io::Result<Self> {
        let job = Self::create()?;
        Self::set_limit_flags(&job.handle, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)?;
        Ok(job)
    }

    /// Captures an owned process handle before its numeric identifier can be reused.
    pub fn open_process_handle(process_id: u32) -> io::Result<std::os::windows::io::OwnedHandle> {
        let handle = unsafe {
            OpenProcess(PROCESS_TERMINATE, /*bInheritHandle*/ 0, process_id)
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(handle.cast()) })
    }

    /// Terminates the exact process identified by a previously captured handle.
    pub fn terminate_process_handle(handle: &std::os::windows::io::OwnedHandle) -> io::Result<()> {
        let terminated = unsafe {
            TerminateProcess(handle.as_raw_handle().cast(), /*uExitCode*/ 1)
        };
        if terminated == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn set_limit_flags(handle: &OwnedHandle, flags: u32) -> io::Result<()> {
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = flags;
        let configured = unsafe {
            SetInformationJobObject(
                handle.as_raw_handle().cast(),
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of_mut!(limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Assigns a running process to this job.
    ///
    /// Assignment is not retroactive: descendants created before this call
    /// completes are not guaranteed to become members of the job.
    pub(crate) fn assign_process(&self, process_handle: RawHandle) -> io::Result<()> {
        let assigned = unsafe {
            AssignProcessToJobObject(self.handle.as_raw_handle().cast(), process_handle.cast())
        };
        if assigned == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Prevents a child from running before it can be assigned to this job.
    pub fn prepare_suspended_spawn(&self, command: &mut Command) {
        command.creation_flags(CREATE_SUSPENDED).kill_on_drop(true);
    }

    /// Assigns and resumes a suspended child by numeric PID, returning whether assignment succeeded.
    ///
    /// Nested jobs can reject assignment. Such a child is resumed without
    /// containment so legacy callers can preserve their compatibility fallback.
    /// Note: this legacy helper reopens by numeric `process_id` via `OpenProcess`
    /// and drops its temporary `OwnedHandle` on return; checked callers must use
    /// the original launched process `HANDLE` (`assign_and_resume_process_handle`
    /// or `PROC_THREAD_ATTRIBUTE_JOB_LIST` + retained `PROCESS_INFORMATION`).
    pub fn assign_and_resume_process(&self, process_id: u32) -> io::Result<bool> {
        let process = unsafe {
            OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_SUSPEND_RESUME,
                /*bInheritHandle*/ 0,
                process_id,
            )
        };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let process = unsafe { OwnedHandle::from_raw_handle(process.cast()) };
        let assignment = self.assign_process(process.as_raw_handle());

        let status = unsafe { NtResumeProcess(process.as_raw_handle().cast()) };
        if !NT_SUCCESS(status) {
            unsafe {
                TerminateProcess(process.as_raw_handle().cast(), /*uExitCode*/ 1)
            };
            return Err(io::Error::other(format!(
                "failed to resume suspended process: NTSTATUS {status:#x}"
            )));
        }

        match assignment {
            Ok(()) => Ok(true),
            Err(error) => {
                log::warn!(
                    "Windows process job assignment unavailable for pid {process_id}: {error}"
                );
                Ok(false)
            }
        }
    }

    /// Assigns and resumes a suspended child using the caller's already-retained
    /// original `process_handle`, without reopening by numeric PID or dropping
    /// or terminating the caller's handle on error.
    pub fn assign_and_resume_process_handle(&self, process_handle: RawHandle) -> io::Result<()> {
        if process_handle.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "process handle is null",
            ));
        }
        self.assign_process(process_handle)?;
        if !self.contains_process_handle(process_handle)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "process handle is not a member of the target JobObject after assignment",
            ));
        }
        let status = unsafe { NtResumeProcess(process_handle.cast()) };
        if !NT_SUCCESS(status) {
            return Err(io::Error::other(format!(
                "failed to resume contained process handle: NTSTATUS {status:#x}"
            )));
        }
        Ok(())
    }

    /// Starts a process only after assigning it to this Job Object, retaining and
    /// returning the spawned `Child` inside `Err((Some(child), err))` if any
    /// post-spawn handle/assign/resume step fails so the caller retains ownership.
    pub fn spawn_contained_retaining_child(
        &self,
        command: &mut Command,
    ) -> Result<Child, (Option<Child>, io::Error)> {
        self.prepare_suspended_spawn(command);
        let child = command.spawn().map_err(|err| (None, err))?;
        let Some(process_handle) = child.raw_handle() else {
            return Err((
                Some(child),
                io::Error::other("missing child process handle"),
            ));
        };
        if let Err(err) = self.assign_and_resume_process_handle(process_handle) {
            return Err((Some(child), err));
        }
        Ok(child)
    }

    /// Starts a process only after assigning it to this Job Object.
    ///
    /// Note: because this legacy signature returns `io::Result<Child>`, a failure
    /// in `assign_process` or `NtResumeProcess` after `command.spawn()` drops the
    /// local `Child` (which has `kill_on_drop(true)`). Callers requiring retained
    /// ownership on post-spawn error should use [`Self::spawn_contained_retaining_child`].
    pub fn spawn_contained(&self, command: &mut Command) -> io::Result<Child> {
        self.spawn_contained_retaining_child(command)
            .map_err(|(_child, err)| err)
    }

    /// Allows contained descendants to keep running after the root exits normally.
    ///
    /// This disables both explicit job termination and kill-on-close for this
    /// object. Calls race safely with [`Self::terminate`]: whichever operation
    /// acquires the state lock first determines whether the process tree is
    /// preserved or terminated.
    pub fn preserve_descendants(&self) -> io::Result<()> {
        let mut preserve_descendants = self
            .preserve_descendants
            .lock()
            .map_err(|_| io::Error::other("job state lock poisoned"))?;
        if *preserve_descendants {
            return Ok(());
        }

        Self::set_limit_flags(&self.handle, JOB_OBJECT_LIMIT_BREAKAWAY_OK)?;
        *preserve_descendants = true;
        Ok(())
    }

    /// Terminates every process currently assigned to the job.
    pub fn terminate(&self) -> io::Result<()> {
        self.terminate_invoked.store(true, Ordering::SeqCst);
        let preserve_descendants = self
            .preserve_descendants
            .lock()
            .map_err(|_| io::Error::other("job state lock poisoned"))?;
        if *preserve_descendants {
            return Ok(());
        }

        let terminated = unsafe {
            TerminateJobObject(self.handle.as_raw_handle().cast(), /*uExitCode*/ 1)
        };
        if terminated == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Returns whether [`Self::preserve_descendants`] was invoked on this Job Object
    /// or whether `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` is no longer active on the kernel handle.
    pub fn descendants_preserved(&self) -> io::Result<bool> {
        let preserve_descendants = self
            .preserve_descendants
            .lock()
            .map_err(|_| io::Error::other("job state lock poisoned"))?;
        if *preserve_descendants {
            return Ok(true);
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        let mut return_len: u32 = 0;
        let ok = unsafe {
            QueryInformationJobObject(
                self.handle.as_raw_handle().cast(),
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of_mut!(limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                &mut return_len,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let kill_on_close_active =
            (limits.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE) != 0;
        Ok(!kill_on_close_active)
    }

    /// Returns whether [`Self::terminate`] was invoked on this Job Object.
    pub fn terminate_invoked(&self) -> bool {
        self.terminate_invoked.load(Ordering::SeqCst)
    }

    /// Verifies that the exact `process_handle` belongs to this Job Object without reopening by PID.
    pub fn contains_process_handle(&self, process_handle: RawHandle) -> io::Result<bool> {
        if process_handle.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "process handle is null",
            ));
        }
        let mut in_job = 0;
        let ok = unsafe {
            IsProcessInJob(
                process_handle.cast(),
                self.handle.as_raw_handle().cast(),
                &mut in_job,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(in_job != 0)
    }

    /// Queries the Job Object's accounting counters and active process ID list with strict shape checks.
    ///
    /// Note: Windows exposes `JobObjectBasicAccountingInformation` and `JobObjectBasicProcessIdList`
    /// as two separately sampled `QueryInformationJobObject` calls, not a single joint atomic kernel
    /// snapshot. `RawJobObservation::validate` requires both samples to have valid return shapes and
    /// mutually consistent active/assigned/listed counts without inferring cross-call kernel atomicity.
    pub fn query_accounting_and_process_ids(&self) -> io::Result<(u32, u32, u32, Vec<usize>)> {
        let observed = self.query_raw_accounting_and_process_ids()?.validate()?;
        Ok((observed.total_processes, observed.active_processes,
            observed.total_terminated_processes, observed.process_ids))
    }

    /// Read-only observation of the SAME owned Job for a positively proven
    /// not-created original process. The caller must hold matching producer
    /// ProcessNotCreated evidence; a zero PI or failed wait is not authority.
    /// All OS queries and shape checks are shared with the strict launched API.
    /// Only actual 0/0/0 accounting and an empty, consistent PID list are accepted.
    pub fn query_not_created_accounting_and_process_ids(
        &self,
    ) -> io::Result<(u32, u32, u32, Vec<usize>)> {
        let observed = self.query_raw_accounting_and_process_ids()?.validate_not_created()?;
        Ok((observed.total_processes, observed.active_processes,
            observed.total_terminated_processes, observed.process_ids))
    }

    fn query_raw_accounting_and_process_ids(&self) -> io::Result<RawJobObservation> {
        let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        let mut accounting_return_len: u32 = 0;
        let accounting_ok = unsafe {
            QueryInformationJobObject(
                self.handle.as_raw_handle().cast(),
                JobObjectBasicAccountingInformation,
                std::ptr::addr_of_mut!(accounting).cast(),
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                &mut accounting_return_len,
            )
        };
        if accounting_ok == 0 {
            return Err(io::Error::last_os_error());
        }

        let mut capacity = INITIAL_JOB_PID_CAPACITY;
        let (process_list_return_len, assigned_processes, listed_processes, process_ids) = loop {
            let byte_len = PROCESS_ID_LIST_HEADER_BYTES
                + capacity * std::mem::size_of::<usize>();
            let word_len = byte_len.div_ceil(std::mem::size_of::<usize>());
            let mut buffer = vec![0usize; word_len];
            let mut return_len: u32 = 0;
            let ok = unsafe {
                QueryInformationJobObject(
                    self.handle.as_raw_handle().cast(),
                    JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr().cast(),
                    byte_len as u32,
                    &mut return_len,
                )
            };
            if ok == 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(ERROR_MORE_DATA as i32)
                    && capacity < MAX_JOB_PID_CAPACITY
                {
                    capacity = (capacity * 2).min(MAX_JOB_PID_CAPACITY);
                    continue;
                }
                return Err(err);
            }
            let header_ptr = buffer.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
            let assigned = unsafe { (*header_ptr).NumberOfAssignedProcesses };
            let listed = unsafe { (*header_ptr).NumberOfProcessIdsInList };
            let pids_ptr = unsafe {
                buffer
                    .as_ptr()
                    .cast::<u8>()
                    .add(PROCESS_ID_LIST_HEADER_BYTES)
                    .cast::<usize>()
            };
            let count = (listed as usize).min(capacity);
            let mut pids = Vec::with_capacity(count);
            for idx in 0..count {
                pids.push(unsafe { std::ptr::read_unaligned(pids_ptr.add(idx)) });
            }
            break (return_len as usize, assigned, listed, pids);
        };

        Ok(RawJobObservation {
            accounting_ok: true,
            accounting_return_len: accounting_return_len as usize,
            total_processes: accounting.TotalProcesses,
            active_processes: accounting.ActiveProcesses,
            total_terminated_processes: accounting.TotalTerminatedProcesses,
            process_list_ok: true,
            process_list_return_len,
            assigned_processes,
            listed_processes,
            process_ids,
        })
    }

    /// Bounded-drains this Job Object until `active_processes == 0` and the active PID list is empty.
    ///
    /// Rejects `preserve_descendants`, `terminate`, malformed/inconsistent observations, changed
    /// `total_processes` / `total_terminated_processes` across polls, and observations completed
    /// at or after `deadline`. Preserves actual `(total_processes, active_processes, total_terminated_processes)`.
    pub fn drain_until_active_zero(
        &self,
        deadline: Instant,
        poll_interval: Duration,
    ) -> io::Result<(u32, u32, u32)> {
        self.drain_until_active_zero_with(deadline, poll_interval, || {
            let (total, active, terminated, pids) = self.query_accounting_and_process_ids()?;
            Ok(ValidatedJobObservation {
                total_processes: total,
                active_processes: active,
                total_terminated_processes: terminated,
                process_ids: pids,
            })
        })
    }

    pub(crate) fn drain_until_active_zero_with<F>(
        &self,
        deadline: Instant,
        poll_interval: Duration,
        mut observe: F,
    ) -> io::Result<(u32, u32, u32)>
    where
        F: FnMut() -> io::Result<ValidatedJobObservation>,
    {
        if self.descendants_preserved()? {
            return Err(io::Error::other(
                "checked job drain rejected: preserve_descendants was invoked on JobObject",
            ));
        }
        if self.terminate_invoked() {
            return Err(io::Error::other(
                "checked job drain rejected: terminate was invoked on JobObject",
            ));
        }

        let mut initial_totals: Option<(u32, u32)> = None;
        loop {
            let before = Instant::now();
            if before >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "job active-zero drain budget expired before observation",
                ));
            }
            let sample = observe()?;
            let after = Instant::now();
            match initial_totals {
                None => {
                    initial_totals =
                        Some((sample.total_processes, sample.total_terminated_processes));
                }
                Some((expected_total, expected_terminated)) => {
                    if sample.total_processes != expected_total {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "job total_processes changed during drain: initial={expected_total} observed={}",
                                sample.total_processes
                            ),
                        ));
                    }
                    if sample.total_terminated_processes != expected_terminated {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "job total_terminated_processes changed during drain: initial={expected_terminated} observed={}",
                                sample.total_terminated_processes
                            ),
                        ));
                    }
                }
            }

            if sample.active_processes == 0 && sample.process_ids.is_empty() {
                if after >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!(
                            "job reached active=0 after shutdown budget expired (total={})",
                            sample.total_processes
                        ),
                    ));
                }
                return Ok((
                    sample.total_processes,
                    sample.active_processes,
                    sample.total_terminated_processes,
                ));
            }

            let now = Instant::now();
            if now >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "job drain timed out with active={} total={} pids={:?}",
                        sample.active_processes, sample.total_processes, sample.process_ids
                    ),
                ));
            }
            let remaining = deadline.saturating_duration_since(now);
            std::thread::sleep(remaining.min(poll_interval));
        }
    }
}

impl AsRawHandle for JobObject {
    fn as_raw_handle(&self) -> RawHandle {
        self.handle.as_raw_handle()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_raw_observation(
        total_processes: u32,
        active_processes: u32,
        process_ids: Vec<usize>,
    ) -> RawJobObservation {
        let listed_processes = process_ids.len() as u32;
        let process_list_return_len = if listed_processes == 0 {
            PROCESS_ID_LIST_HEADER_BYTES
        } else {
            PROCESS_ID_LIST_HEADER_BYTES + process_ids.len() * std::mem::size_of::<usize>()
        };
        RawJobObservation {
            accounting_ok: true,
            accounting_return_len: std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>(),
            total_processes,
            active_processes,
            total_terminated_processes: 0,
            process_list_ok: true,
            process_list_return_len,
            assigned_processes: listed_processes,
            listed_processes,
            process_ids,
        }
    }

    #[test]
    fn not_created_observation_accepts_only_actual_empty_job_and_keeps_strict_launch_contract() {
        let empty = valid_raw_observation(0, 0, vec![]);
        assert_eq!(empty.clone().validate().unwrap_err().kind(), io::ErrorKind::InvalidData);
        let actual = empty.clone().validate_not_created().unwrap();
        assert_eq!((actual.total_processes, actual.active_processes,
            actual.total_terminated_processes, actual.process_ids), (0, 0, 0, vec![]));
        let mut padded_empty = empty;
        padded_empty.process_list_return_len = std::mem::size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
        assert!(padded_empty.validate_not_created().is_ok());

        // A real launch, including a completed launch, cannot be reclassified as absence.
        assert!(valid_raw_observation(1, 0, vec![]).validate_not_created().is_err());
        assert!(valid_raw_observation(1, 1, vec![123]).validate_not_created().is_err());
        let mut terminated = valid_raw_observation(0, 0, vec![]);
        terminated.total_terminated_processes = 1;
        assert!(terminated.validate_not_created().is_err());
        let mut past_terminated_member = valid_raw_observation(1, 0, vec![]);
        past_terminated_member.total_terminated_processes = 1;
        assert!(past_terminated_member.clone().validate().is_ok());
        assert!(past_terminated_member.validate_not_created().is_err());
        let mut impossible_active = valid_raw_observation(0, 1, vec![]);
        assert!(impossible_active.clone().validate_not_created().is_err());
        impossible_active.active_processes = 0;
        impossible_active.total_terminated_processes = 2;
        assert!(impossible_active.validate_not_created().is_err());
    }

    #[test]
    fn not_created_observation_does_not_hide_query_shape_or_pid_failures() {
        let empty = valid_raw_observation(0, 0, vec![]);
        let mut bad = empty.clone(); bad.accounting_ok = false;
        assert!(bad.validate_not_created().is_err());
        let mut bad = empty.clone(); bad.process_list_ok = false;
        assert!(bad.validate_not_created().is_err());
        for length in [0, std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() - 1,
            std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() + 1] {
            let mut bad = empty.clone(); bad.accounting_return_len = length;
            assert!(bad.validate_not_created().is_err());
        }
        for length in [0, PROCESS_ID_LIST_HEADER_BYTES - 1,
            std::mem::size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() + 1] {
            let mut bad = empty.clone(); bad.process_list_return_len = length;
            assert!(bad.validate_not_created().is_err());
        }
        let mut bad = empty.clone(); bad.assigned_processes = 1;
        assert!(bad.validate_not_created().is_err());
        let mut bad = empty.clone(); bad.listed_processes = 1;
        assert!(bad.validate_not_created().is_err());
        let mut bad = empty; bad.process_ids.push(55);
        assert!(bad.validate_not_created().is_err());
        for pids in [vec![0], vec![77, 77], vec![77]] {
            assert!(valid_raw_observation(0, pids.len() as u32, pids)
                .validate_not_created().is_err());
        }
    }

    #[test]
    fn raw_observation_preserves_actual_total_counts_and_rejects_malformed_shapes() {
        let total1 = valid_raw_observation(1, 0, vec![]).validate().unwrap();
        assert_eq!(total1.total_processes, 1);
        assert_eq!(total1.active_processes, 0);

        // Actual total=2 is preserved, never faked as total=1.
        let total2 = valid_raw_observation(2, 0, vec![]).validate().unwrap();
        assert_eq!(total2.total_processes, 2);
        assert_eq!(total2.active_processes, 0);

        // Zero total after launch is rejected.
        assert!(valid_raw_observation(0, 0, vec![]).validate().is_err());

        // Active > total is rejected.
        assert!(valid_raw_observation(1, 2, vec![10, 20]).validate().is_err());

        // Mismatch between accounting active and process list count is rejected.
        assert!(valid_raw_observation(2, 0, vec![42]).validate().is_err());
        assert!(valid_raw_observation(2, 1, vec![]).validate().is_err());

        // Malformed accounting return length is rejected.
        let mut bad_acct_len = valid_raw_observation(1, 0, vec![]);
        bad_acct_len.accounting_return_len -= 1;
        assert!(bad_acct_len.validate().is_err());

        // Malformed process list return length is rejected.
        let mut bad_list_len = valid_raw_observation(1, 1, vec![100]);
        bad_list_len.process_list_return_len = PROCESS_ID_LIST_HEADER_BYTES;
        assert!(bad_list_len.validate().is_err());

        // Duplicate or zero PID in list is rejected.
        assert!(valid_raw_observation(2, 2, vec![0, 100]).validate().is_err());
        assert!(
            valid_raw_observation(2, 2, vec![100, 100])
                .validate()
                .is_err()
        );
    }

    #[test]
    fn drain_rejects_preserve_descendants_terminate_changed_totals_and_late_budget() {
        let job = JobObject::create_without_breakaway().unwrap();
        let mut polls = Vec::from([
            valid_raw_observation(2, 1, vec![777]).validate().unwrap(),
            valid_raw_observation(2, 0, vec![]).validate().unwrap(),
        ]);
        let outcome = job
            .drain_until_active_zero_with(
                Instant::now() + Duration::from_secs(1),
                Duration::from_millis(1),
                || Ok(polls.remove(0)),
            )
            .unwrap();
        assert_eq!(outcome, (2, 0, 0));

        // Changed total_processes during drain is rejected.
        let mut changed_polls = Vec::from([
            valid_raw_observation(1, 1, vec![777]).validate().unwrap(),
            valid_raw_observation(2, 0, vec![]).validate().unwrap(),
        ]);
        assert!(
            job.drain_until_active_zero_with(
                Instant::now() + Duration::from_secs(1),
                Duration::from_millis(1),
                || Ok(changed_polls.remove(0)),
            )
            .is_err()
        );

        // Extra active member remaining until deadline is rejected with TimedOut.
        let err = job
            .drain_until_active_zero_with(
                Instant::now() + Duration::from_millis(15),
                Duration::from_millis(5),
                || valid_raw_observation(2, 1, vec![888]).validate(),
            )
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);

        // Late observation after budget is rejected even if active=0.
        let late_deadline = Instant::now() + Duration::from_millis(5);
        let late_err = job
            .drain_until_active_zero_with(late_deadline, Duration::from_millis(1), || {
                std::thread::sleep(Duration::from_millis(15));
                valid_raw_observation(1, 0, vec![]).validate()
            })
            .unwrap_err();
        assert_eq!(late_err.kind(), io::ErrorKind::TimedOut);

        // preserve_descendants disqualifies clean job drain.
        let preserved_job = JobObject::create_without_breakaway().unwrap();
        preserved_job.preserve_descendants().unwrap();
        assert!(preserved_job.descendants_preserved().unwrap());
        assert!(
            preserved_job
                .drain_until_active_zero_with(
                    Instant::now() + Duration::from_secs(1),
                    Duration::from_millis(1),
                    || valid_raw_observation(1, 0, vec![]).validate(),
                )
                .is_err()
        );

        // terminate disqualifies clean job drain.
        let terminated_job = JobObject::create_without_breakaway().unwrap();
        terminated_job.terminate().unwrap();
        assert!(terminated_job.terminate_invoked());
        assert!(
            terminated_job
                .drain_until_active_zero_with(
                    Instant::now() + Duration::from_secs(1),
                    Duration::from_millis(1),
                    || valid_raw_observation(1, 0, vec![]).validate(),
                )
                .is_err()
        );

        // Null handle is rejected by assign_and_resume_process_handle without mutating job state.
        let fresh_job = JobObject::create_without_breakaway().unwrap();
        assert!(!fresh_job.descendants_preserved().unwrap());
        assert_eq!(
            fresh_job
                .assign_and_resume_process_handle(std::ptr::null_mut())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(!fresh_job.terminate_invoked());
        assert!(!fresh_job.descendants_preserved().unwrap());
    }
}

