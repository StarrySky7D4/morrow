//! Pure state, decision, and call-budget tests for `RetainedJobOwner`.
//! No OS, kernel, process, Job, pipe, filesystem, or token APIs are invoked.

use super::*;
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
};

const TEST_JOB_RAW: isize = 0x1000;
const TEST_STDIN_RAW: isize = 0x1101;
const TEST_STDOUT_RAW: isize = 0x1102;
const TEST_STDERR_RAW: isize = 0x1103;
const TEST_CHILD_PROC_RAW: isize = 0x2001;
const TEST_CHILD_THREAD_RAW: isize = 0x2002;
const TEST_CHILD_PID: u32 = 4242;
const TEST_CHILD_TID: u32 = 4243;
const TEST_CREATION_FT: u64 = 0x01DC_5000_0000_0077;

fn h(raw: isize) -> HANDLE {
    raw as HANDLE
}

fn pure_prepared_env() -> PreparedTrustedHostEnvironment {
    let mut env = BTreeMap::new();
    env.insert("SystemRoot".into(), r"C:\Windows".into());
    prepare_trusted_host_environment_with(&env, || Ok(r"C:\Synthetic\AppData\Local".into()))
        .expect("pure prepared environment")
}

fn default_stdio_handles() -> [HANDLE; 3] {
    [h(TEST_STDIN_RAW), h(TEST_STDOUT_RAW), h(TEST_STDERR_RAW)]
}

struct FakeRetainedNativeOps {
    // Call counters (including forbidden legacy operations that must remain 0)
    pub verify_job_calls: u32,
    pub job_active_calls: u32,
    pub handle_inheritable_calls: u32,
    pub set_inheritable_calls: u32,
    pub create_process_calls: u32,
    pub process_id_calls: u32,
    pub creation_ft_calls: u32,
    pub thread_process_id_calls: u32,
    pub thread_id_calls: u32,
    pub is_in_job_calls: u32,
    pub wait_calls: u32,
    pub exit_code_calls: u32,
    pub resume_thread_calls: u32,
    pub close_handle_calls: u32,
    pub closed_handles_in_order: Vec<isize>,
    pub terminate_job_calls: u32,
    pub terminate_process_calls: u32,
    pub last_environment: Vec<u16>,
    pub last_attr_handles: Vec<isize>,

    // Scripted behaviors
    pub job_containment_ok: bool,
    pub inheritable_handles: BTreeMap<isize, bool>,
    pub set_inheritable_fail_on: Option<isize>,
    pub keep_inheritable_on_readback: Option<isize>,

    pub create_writes_h_process: isize,
    pub create_writes_h_thread: isize,
    pub create_writes_pid: u32,
    pub create_writes_tid: u32,
    pub create_returns_true: bool,
    pub create_last_error: Option<u32>,
    pub create_setup_error: Option<io::ErrorKind>,
    pub create_unwinds_after_writing_pi: bool,

    pub reported_process_id: u32,
    pub reported_creation_ft: u64,
    pub reported_thread_process_id: u32,
    pub reported_thread_id: u32,
    pub reported_in_job: bool,
    pub post_validate_unwinds: bool,

    pub process_signaled_on_post_validate: bool,
    pub process_signaled_on_poll: bool,
    pub thread_signaled_on_poll: bool,
    pub reported_exit_code: u32,
    pub reported_active_processes: VecDeque<u32>,

    pub resume_outcome: Result<u32, u32>,
    pub resume_unwinds: bool,

    pub close_fail_on_handle: BTreeMap<isize, u32>,
    pub close_unwinds_on_handle: Option<isize>,
}

impl Default for FakeRetainedNativeOps {
    fn default() -> Self {
        let mut inheritable_handles = BTreeMap::new();
        inheritable_handles.insert(TEST_STDIN_RAW, true);
        inheritable_handles.insert(TEST_STDOUT_RAW, true);
        inheritable_handles.insert(TEST_STDERR_RAW, true);
        Self {
            verify_job_calls: 0,
            job_active_calls: 0,
            handle_inheritable_calls: 0,
            set_inheritable_calls: 0,
            create_process_calls: 0,
            process_id_calls: 0,
            creation_ft_calls: 0,
            thread_process_id_calls: 0,
            thread_id_calls: 0,
            is_in_job_calls: 0,
            wait_calls: 0,
            exit_code_calls: 0,
            resume_thread_calls: 0,
            close_handle_calls: 0,
            closed_handles_in_order: Vec::new(),
            terminate_job_calls: 0,
            terminate_process_calls: 0,
            last_environment: Vec::new(),
            last_attr_handles: Vec::new(),

            job_containment_ok: true,
            inheritable_handles,
            set_inheritable_fail_on: None,
            keep_inheritable_on_readback: None,

            create_writes_h_process: TEST_CHILD_PROC_RAW,
            create_writes_h_thread: TEST_CHILD_THREAD_RAW,
            create_writes_pid: TEST_CHILD_PID,
            create_writes_tid: TEST_CHILD_TID,
            create_returns_true: true,
            create_last_error: None,
            create_setup_error: None,
            create_unwinds_after_writing_pi: false,

            reported_process_id: TEST_CHILD_PID,
            reported_creation_ft: TEST_CREATION_FT,
            reported_thread_process_id: TEST_CHILD_PID,
            reported_thread_id: TEST_CHILD_TID,
            reported_in_job: true,
            post_validate_unwinds: false,

            process_signaled_on_post_validate: false,
            process_signaled_on_poll: false,
            thread_signaled_on_poll: false,
            reported_exit_code: 0,
            reported_active_processes: VecDeque::from([0]),

            resume_outcome: Ok(1),
            resume_unwinds: false,

            close_fail_on_handle: BTreeMap::new(),
            close_unwinds_on_handle: None,
        }
    }
}

impl RetainedNativeOps for FakeRetainedNativeOps {
    fn verify_job_containment(&mut self, job_handle: HANDLE) -> io::Result<()> {
        self.verify_job_calls += 1;
        if job_handle.is_null() || (job_handle as isize) <= 0 || !self.job_containment_ok {
            return Err(invalid("Job limit readback does not match requested containment"));
        }
        Ok(())
    }

    fn job_active_processes(&mut self, _job_handle: HANDLE) -> io::Result<u32> {
        self.job_active_calls += 1;
        Ok(self.reported_active_processes.pop_front().unwrap_or(0))
    }

    fn handle_inheritable(&mut self, handle: HANDLE) -> io::Result<bool> {
        self.handle_inheritable_calls += 1;
        Ok(*self
            .inheritable_handles
            .get(&(handle as isize))
            .unwrap_or(&false))
    }

    fn set_handle_inheritable(&mut self, handle: HANDLE, inherit: bool) -> io::Result<()> {
        self.set_inheritable_calls += 1;
        let raw = handle as isize;
        if self.set_inheritable_fail_on == Some(raw) {
            return Err(io::Error::from_raw_os_error(5));
        }
        if self.keep_inheritable_on_readback == Some(raw) {
            self.inheritable_handles.insert(raw, !inherit);
        } else {
            self.inheritable_handles.insert(raw, inherit);
        }
        Ok(())
    }

    fn create_process_atomic(
        &mut self,
        _executable: &[u16],
        _command: &mut [u16],
        environment: &[u16],
        _directory: &[u16],
        _attr_storage: &mut Vec<AttributeStorage>,
        attr_initialized: &mut bool,
        attr_deleted: &mut bool,
        _attr_jobs: &[HANDLE; 1],
        attr_handles: &[HANDLE],
        pi_storage: &mut PROCESS_INFORMATION,
    ) -> CreateProcessAtomicResult {
        if let Some(kind) = self.create_setup_error {
            return CreateProcessAtomicResult {
                setup_error: Some(io::Error::new(kind, "synthetic attribute list setup error")),
                create_invoked: false,
                create_ok: false,
                create_last_error: None,
                cleanup_error: None,
            };
        }
        *attr_initialized = true;
        self.create_process_calls += 1;
        self.last_environment = environment.to_vec();
        self.last_attr_handles = attr_handles.iter().map(|&h| h as isize).collect();
        pi_storage.hProcess = h(self.create_writes_h_process);
        pi_storage.hThread = h(self.create_writes_h_thread);
        pi_storage.dwProcessId = self.create_writes_pid;
        pi_storage.dwThreadId = self.create_writes_tid;
        if self.create_unwinds_after_writing_pi {
            panic!("simulated CreateProcessW unwind after writing PROCESS_INFORMATION");
        }
        *attr_initialized = false;
        *attr_deleted = true;
        CreateProcessAtomicResult {
            setup_error: None,
            create_invoked: true,
            create_ok: self.create_returns_true,
            create_last_error: self.create_last_error,
            cleanup_error: None,
        }
    }

    fn process_id(&mut self, _process_handle: HANDLE) -> io::Result<u32> {
        self.process_id_calls += 1;
        if self.post_validate_unwinds {
            panic!("simulated post-create validation unwind");
        }
        Ok(self.reported_process_id)
    }

    fn process_creation_filetime(&mut self, _process_handle: HANDLE) -> io::Result<u64> {
        self.creation_ft_calls += 1;
        if self.reported_creation_ft == 0 {
            return Err(invalid("process creation identity missing"));
        }
        Ok(self.reported_creation_ft)
    }

    fn thread_process_id(&mut self, _thread_handle: HANDLE) -> io::Result<u32> {
        self.thread_process_id_calls += 1;
        Ok(self.reported_thread_process_id)
    }

    fn thread_id(&mut self, _thread_handle: HANDLE) -> io::Result<u32> {
        self.thread_id_calls += 1;
        Ok(self.reported_thread_id)
    }

    fn is_process_in_job(
        &mut self,
        _process_handle: HANDLE,
        _job_handle: HANDLE,
    ) -> io::Result<bool> {
        self.is_in_job_calls += 1;
        Ok(self.reported_in_job)
    }

    fn wait_handle_signaled_nonblocking(&mut self, handle: HANDLE) -> io::Result<bool> {
        self.wait_calls += 1;
        let raw = handle as isize;
        if raw == self.create_writes_h_process {
            if self.wait_calls == 1 {
                Ok(self.process_signaled_on_post_validate)
            } else {
                Ok(self.process_signaled_on_poll)
            }
        } else if raw == self.create_writes_h_thread {
            Ok(self.thread_signaled_on_poll)
        } else {
            Ok(false)
        }
    }

    fn process_exit_code(&mut self, _process_handle: HANDLE) -> io::Result<u32> {
        self.exit_code_calls += 1;
        Ok(self.reported_exit_code)
    }

    fn resume_thread(&mut self, _thread_handle: HANDLE) -> Result<u32, u32> {
        self.resume_thread_calls += 1;
        if self.resume_unwinds {
            panic!("simulated ResumeThread unwind");
        }
        self.resume_outcome
    }

    fn close_handle(&mut self, handle: HANDLE) -> io::Result<()> {
        self.close_handle_calls += 1;
        let raw = handle as isize;
        if self.close_unwinds_on_handle == Some(raw) {
            panic!("simulated CloseHandle unwind");
        }
        if let Some(&err_code) = self.close_fail_on_handle.get(&raw) {
            return Err(io::Error::from_raw_os_error(err_code as i32));
        }
        self.closed_handles_in_order.push(raw);
        Ok(())
    }
}

#[test]
fn precreation_rejection_retains_job_without_calling_create_process_or_allowing_replay() {
    // 1. Relative executable path rejected before CreateProcessW.
    let mut ops = FakeRetainedNativeOps::default();
    let mut owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    let err = owner
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"relative\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops,
        )
        .expect_err("relative executable path must be rejected");
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(ops.create_process_calls, 0);
    assert_eq!(ops.resume_thread_calls, 0);
    assert_eq!(ops.close_handle_calls, 0);
    assert_eq!(ops.terminate_job_calls, 0);
    assert_eq!(ops.terminate_process_calls, 0);
    assert_eq!(
        owner.job_handle_slot().state(),
        RetainedHandleState::Open(TEST_JOB_RAW)
    );
    assert!(owner.no_replay());
    assert_eq!(owner.disposition(), RetainedDisposition::UnknownRetained);
    assert_eq!(
        owner.first_error().unwrap().stage,
        RetainedOperationStage::PreCreationValidation
    );

    // Replay attempt on the same preinstalled owner is rejected without calling CreateProcessW,
    // and first_error remains sticky.
    let first_snapshot = owner.first_error().cloned().unwrap();
    let replay_err = owner
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops,
        )
        .expect_err("replay on faulted owner must be refused");
    assert_eq!(replay_err.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(ops.create_process_calls, 0);
    assert_eq!(owner.first_error(), Some(&first_snapshot));
    assert_eq!(owner.error_count(), 2);

    // 2. Non-inheritable or duplicate HANDLE_LIST entries rejected before CreateProcessW.
    let mut ops2 = FakeRetainedNativeOps::default();
    ops2.inheritable_handles.insert(TEST_STDOUT_RAW, false);
    let mut owner2 = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    let err2 = owner2
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops2,
        )
        .expect_err("non-inheritable HANDLE_LIST handle must be rejected");
    assert_eq!(err2.kind(), io::ErrorKind::InvalidData);
    assert_eq!(ops2.create_process_calls, 0);
    assert_eq!(
        owner2.job_handle_slot().state(),
        RetainedHandleState::Open(TEST_JOB_RAW)
    );

    // 3. Duplicate handle or Job handle in HANDLE_LIST rejected before CreateProcessW.
    let mut ops3 = FakeRetainedNativeOps::default();
    let mut owner3 = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    assert!(
        owner3
            .create_with_raw_borrowed_stdio_and_ops(
                Path::new(r"C:\Program\host.exe"),
                &[],
                Path::new(r"C:\Work"),
                pure_prepared_env(),
                &[h(TEST_STDIN_RAW), h(TEST_STDIN_RAW), h(TEST_STDERR_RAW)],
                &mut ops3,
            )
            .is_err()
    );
    assert_eq!(ops3.create_process_calls, 0);
}

#[test]
fn false_return_with_partial_process_information_captures_both_outputs_and_retains_debt() {
    // Subcase A: CreateProcessW returns FALSE (error 5) but wrote both hProcess and hThread.
    let mut ops = FakeRetainedNativeOps::default();
    ops.create_returns_true = false;
    ops.create_last_error = Some(5);
    ops.create_writes_h_process = 0x2201;
    ops.create_writes_h_thread = 0x2202;
    ops.create_writes_pid = 9001;
    ops.create_writes_tid = 9002;

    let mut owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    let err = owner
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &["--child".into()],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops,
        )
        .expect_err("FALSE return from CreateProcessW must fail");

    assert_eq!(err.raw_os_error(), Some(5));
    assert_eq!(ops.create_process_calls, 1);
    assert_eq!(ops.close_handle_calls, 0);
    assert_eq!(ops.terminate_job_calls, 0);
    assert_eq!(ops.terminate_process_calls, 0);

    // Both partial raw outputs must be captured before any validation or error return.
    let raw_pi = owner.raw_process_information();
    assert!(raw_pi.create_attempted);
    assert!(!raw_pi.create_returned_true);
    assert_eq!(raw_pi.create_last_error, Some(5));
    assert_eq!(raw_pi.h_process, 0x2201);
    assert_eq!(raw_pi.h_thread, 0x2202);
    assert_eq!(raw_pi.dw_process_id, 9001);
    assert_eq!(raw_pi.dw_thread_id, 9002);
    assert_eq!(
        owner.process_handle_slot().state(),
        RetainedHandleState::Open(0x2201)
    );
    assert_eq!(
        owner.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(0x2202)
    );
    assert_eq!(
        owner.job_handle_slot().state(),
        RetainedHandleState::Open(TEST_JOB_RAW)
    );
    assert!(owner.no_replay());
    assert_eq!(owner.disposition(), RetainedDisposition::UnknownRetained);
    assert!(owner.has_unresolved_debt());
    assert_eq!(
        owner.first_error().unwrap().stage,
        RetainedOperationStage::CreateProcess
    );
    assert_eq!(owner.first_error().unwrap().raw_os_error, Some(5));

    // Subcase B: CreateProcessW returns FALSE and wrote a valid hProcess and a pseudo-handle (-1) hThread.
    let mut ops_b = FakeRetainedNativeOps::default();
    ops_b.create_returns_true = false;
    ops_b.create_last_error = Some(87);
    ops_b.create_writes_h_process = 0x2301;
    ops_b.create_writes_h_thread = -1;
    let mut owner_b = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    let _ = owner_b.create_with_raw_borrowed_stdio_and_ops(
        Path::new(r"C:\Program\host.exe"),
        &[],
        Path::new(r"C:\Work"),
        pure_prepared_env(),
        &default_stdio_handles(),
        &mut ops_b,
    );
    assert_eq!(
        owner_b.process_handle_slot().state(),
        RetainedHandleState::Open(0x2301)
    );
    assert_eq!(
        owner_b.primary_thread_handle_slot().state(),
        RetainedHandleState::InvalidCaptured(-1)
    );
    assert_eq!(ops_b.close_handle_calls, 0);
}

#[test]
fn postcreation_validation_errors_retain_job_process_and_thread_without_kill_or_close() {
    // Failure 1: IsProcessInJob returns false.
    let mut ops = FakeRetainedNativeOps::default();
    ops.reported_in_job = false;
    let mut owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    let err = owner
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &["--arg".into()],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops,
        )
        .expect_err("unconfirmed Job membership must fail post-creation validation");
    assert_eq!(err.to_string(), "atomic child Job membership not confirmed");
    assert_eq!(ops.create_process_calls, 1);
    assert_eq!(ops.terminate_job_calls, 0);
    assert_eq!(ops.terminate_process_calls, 0);
    assert_eq!(ops.close_handle_calls, 0);
    assert_eq!(
        owner.job_handle_slot().state(),
        RetainedHandleState::Open(TEST_JOB_RAW)
    );
    assert_eq!(
        owner.process_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_PROC_RAW)
    );
    assert_eq!(
        owner.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_THREAD_RAW)
    );
    assert_eq!(
        owner.first_error().unwrap().stage,
        RetainedOperationStage::PostCreationValidation
    );
    assert!(owner.no_replay());
    assert_eq!(owner.disposition(), RetainedDisposition::UnknownRetained);

    // Resume after post-creation validation failure is rejected without calling ResumeThread.
    assert!(owner.resume_primary_thread_with_ops(&mut ops).is_err());
    assert_eq!(ops.resume_thread_calls, 0);

    // Failure 2: Thread owner PID mismatch.
    let mut ops2 = FakeRetainedNativeOps::default();
    ops2.reported_thread_process_id = TEST_CHILD_PID + 99;
    let mut owner2 = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    let err2 = owner2
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops2,
        )
        .expect_err("thread process ID mismatch must fail");
    assert_eq!(err2.to_string(), "primary thread process identity mismatch");
    assert_eq!(ops2.close_handle_calls, 0);
    assert_eq!(
        owner2.process_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_PROC_RAW)
    );
    assert_eq!(
        owner2.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_THREAD_RAW)
    );

    // Failure 3: Creation FILETIME is zero.
    let mut ops3 = FakeRetainedNativeOps::default();
    ops3.reported_creation_ft = 0;
    let mut owner3 = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    assert!(
        owner3
            .create_with_raw_borrowed_stdio_and_ops(
                Path::new(r"C:\Program\host.exe"),
                &[],
                Path::new(r"C:\Work"),
                pure_prepared_env(),
                &default_stdio_handles(),
                &mut ops3,
            )
            .is_err()
    );
    assert_eq!(ops3.close_handle_calls, 0);
    assert_eq!(
        owner3.process_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_PROC_RAW)
    );
}

#[test]
fn unwind_across_create_validate_resume_and_close_retains_all_original_handles_and_marks_unknown() {
    // 1. Unwind inside CreateProcessW after writing PROCESS_INFORMATION into preinstalled storage.
    let mut ops = FakeRetainedNativeOps::default();
    ops.create_unwinds_after_writing_pi = true;
    ops.create_writes_h_process = 0x4101;
    ops.create_writes_h_thread = 0x4102;
    let mut owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));

    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _ = owner.create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops,
        );
    }));
    assert!(unwound.is_err(), "simulated panic must unwind");
    assert_eq!(
        owner.process_handle_slot().state(),
        RetainedHandleState::Open(0x4101)
    );
    assert_eq!(
        owner.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(0x4102)
    );
    assert_eq!(
        owner.job_handle_slot().state(),
        RetainedHandleState::Open(TEST_JOB_RAW)
    );
    assert!(owner.no_replay());
    assert_eq!(owner.disposition(), RetainedDisposition::UnknownRetained);
    assert_eq!(
        owner.first_error().unwrap().stage,
        RetainedOperationStage::UnwindDuringOperation
    );
    assert_eq!(ops.close_handle_calls, 0);

    // 2. Unwind inside ResumeThread keeps primary thread handle open and marks attempt as Unwound.
    let mut ops2 = FakeRetainedNativeOps::default();
    ops2.resume_unwinds = true;
    let mut owner2 = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    owner2
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops2,
        )
        .expect("create succeeds before resume unwind");
    let resume_unwound = catch_unwind(AssertUnwindSafe(|| {
        let _ = owner2.resume_primary_thread_with_ops(&mut ops2);
    }));
    assert!(resume_unwound.is_err());
    assert_eq!(ops2.resume_thread_calls, 1);
    assert!(owner2.resume_slot().attempted);
    assert_eq!(
        owner2.resume_slot().outcome,
        PrimaryThreadResumeOutcome::Unwound
    );
    assert_eq!(
        owner2.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_THREAD_RAW)
    );
    assert_eq!(owner2.disposition(), RetainedDisposition::UnknownRetained);
    // Second resume attempt is blocked.
    assert!(owner2.resume_primary_thread_with_ops(&mut ops2).is_err());
    assert_eq!(ops2.resume_thread_calls, 1);

    // 3. Unwind inside post-create validation retains both captured handles and Job without kill/close.
    let mut ops3 = FakeRetainedNativeOps::default();
    ops3.post_validate_unwinds = true;
    let mut owner3 = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    let val_unwound = catch_unwind(AssertUnwindSafe(|| {
        let _ = owner3.create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops3,
        );
    }));
    assert!(val_unwound.is_err());
    assert_eq!(
        owner3.process_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_PROC_RAW)
    );
    assert_eq!(
        owner3.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_THREAD_RAW)
    );
    assert_eq!(
        owner3.job_handle_slot().state(),
        RetainedHandleState::Open(TEST_JOB_RAW)
    );
    assert_eq!(
        owner3.first_error().unwrap().stage,
        RetainedOperationStage::UnwindDuringOperation
    );
    assert_eq!(ops3.close_handle_calls, 0);

    // 4. Unwind inside CloseHandle during retirement leaves the slot CloseInFlight and retains Job.
    let mut ops4 = FakeRetainedNativeOps::default();
    ops4.process_signaled_on_poll = true;
    ops4.thread_signaled_on_poll = true;
    ops4.reported_active_processes = VecDeque::from([0]);
    ops4.close_unwinds_on_handle = Some(TEST_CHILD_PROC_RAW);
    let mut owner4 = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    owner4
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops4,
        )
        .unwrap();
    owner4.resume_primary_thread_with_ops(&mut ops4).unwrap();
    owner4
        .poll_lifecycle_observations_with_ops(&mut ops4)
        .unwrap();
    owner4.confirm_borrowed_stdio_discharged().unwrap();
    owner4
        .record_outer_lifecycle_evidence(OuterLifecycleEvidence {
            stdio_reaped_and_closed: true,
            bootstrap_handles_discharged: true,
            protocol_ack_and_sdk_quiescent: true,
            prebirth_locks_retained_until_exit: true,
        })
        .unwrap();
    let close_unwound = catch_unwind(AssertUnwindSafe(|| {
        let _ = owner4.retire_handles_with_ops(&mut ops4);
    }));
    assert!(close_unwound.is_err());
    assert_eq!(
        owner4.primary_thread_handle_slot().state(),
        RetainedHandleState::Closed {
            former_raw: TEST_CHILD_THREAD_RAW
        }
    );
    assert_eq!(
        owner4.process_handle_slot().state(),
        RetainedHandleState::CloseInFlight(TEST_CHILD_PROC_RAW)
    );
    assert_eq!(
        owner4.job_handle_slot().state(),
        RetainedHandleState::Open(TEST_JOB_RAW)
    );
    assert_eq!(
        owner4.first_error().unwrap().stage,
        RetainedOperationStage::UnwindDuringOperation
    );
    assert_eq!(owner4.disposition(), RetainedDisposition::UnknownRetained);
}

#[test]
fn primary_thread_resume_at_most_once_and_exact_primary_handle_retention_on_all_outcomes() {
    // Case 1: Normal resume (previous suspend count == 1).
    let mut ops = FakeRetainedNativeOps::default();
    let mut owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    owner
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops,
        )
        .expect("create succeeds");

    owner
        .resume_primary_thread_with_ops(&mut ops)
        .expect("resume with count 1 succeeds");
    assert_eq!(ops.resume_thread_calls, 1);
    assert_eq!(owner.phase(), RetainedPhase::Running);
    assert_eq!(
        owner.resume_slot(),
        PrimaryThreadResumeSlot {
            attempted: true,
            call_count: 1,
            previous_suspend_count: Some(1),
            outcome: PrimaryThreadResumeOutcome::ResumedFromSuspendCountOne,
        }
    );
    // Primary thread handle MUST remain Open in owner, never taken and dropped.
    assert_eq!(
        owner.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_THREAD_RAW)
    );
    assert_eq!(ops.close_handle_calls, 0);

    // Calling resume a second time is rejected without invoking ResumeThread again.
    let second = owner
        .resume_primary_thread_with_ops(&mut ops)
        .expect_err("second resume must be rejected");
    assert_eq!(second.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(ops.resume_thread_calls, 1);
    assert_eq!(
        owner.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_THREAD_RAW)
    );

    // Case 2: Unexpected suspend count (e.g. 2) does not loop or drop primary thread handle.
    let mut ops2 = FakeRetainedNativeOps::default();
    ops2.resume_outcome = Ok(2);
    let mut owner2 = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    owner2
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops2,
        )
        .unwrap();
    let count_err = owner2
        .resume_primary_thread_with_ops(&mut ops2)
        .expect_err("suspend count 2 must be rejected");
    assert_eq!(count_err.kind(), io::ErrorKind::InvalidData);
    assert_eq!(ops2.resume_thread_calls, 1);
    assert_eq!(
        owner2.resume_slot().outcome,
        PrimaryThreadResumeOutcome::UnexpectedSuspendCount(2)
    );
    assert_eq!(
        owner2.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_THREAD_RAW)
    );
    assert_eq!(ops2.close_handle_calls, 0);

    // Case 3: Native ResumeThread failure (u32::MAX / error 6).
    let mut ops3 = FakeRetainedNativeOps::default();
    ops3.resume_outcome = Err(6);
    let mut owner3 = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    owner3
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops3,
        )
        .unwrap();
    let native_err = owner3
        .resume_primary_thread_with_ops(&mut ops3)
        .expect_err("ResumeThread failure must be reported");
    assert_eq!(native_err.raw_os_error(), Some(6));
    assert_eq!(ops3.resume_thread_calls, 1);
    assert_eq!(
        owner3.resume_slot().outcome,
        PrimaryThreadResumeOutcome::NativeError(6)
    );
    assert_eq!(
        owner3.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_THREAD_RAW)
    );
    assert_eq!(ops3.close_handle_calls, 0);
}

#[test]
fn full_32bit_exit_code_recorded_only_after_signal_and_close_uncertainty_never_retries() {
    let mut ops = FakeRetainedNativeOps::default();
    ops.process_signaled_on_poll = false;
    ops.thread_signaled_on_poll = false;
    // Use 259 (STILL_ACTIVE) as the real exit code after signal to prove signal-gated observation.
    ops.reported_exit_code = 259;
    ops.reported_active_processes = VecDeque::from([1, 0]);

    let mut owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    owner
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops,
        )
        .unwrap();
    owner.resume_primary_thread_with_ops(&mut ops).unwrap();

    // While WAIT_TIMEOUT is returned, GetExitCodeProcess is never called.
    assert_eq!(owner.poll_process_exit_with_ops(&mut ops).unwrap(), None);
    assert_eq!(ops.exit_code_calls, 0);
    assert_eq!(owner.kernel_observations().exit_code, None);

    // Observing active_processes == 0 before process/thread signal does NOT set post_exit_job_empty_observed.
    ops.reported_active_processes = VecDeque::from([0]);
    assert_eq!(
        owner.poll_job_active_processes_with_ops(&mut ops).unwrap(),
        0
    );
    assert!(!owner.kernel_observations().post_exit_job_empty_observed);
    assert!(!owner.can_retire_handles());

    // Now process and primary thread signal, and post-exit active_processes == 0 is observed.
    ops.process_signaled_on_poll = true;
    ops.thread_signaled_on_poll = true;
    ops.reported_active_processes = VecDeque::from([0]);
    let obs = owner
        .poll_lifecycle_observations_with_ops(&mut ops)
        .unwrap();
    assert!(obs.process_signaled);
    assert_eq!(obs.exit_code, Some(259));
    assert!(obs.thread_signaled);
    assert!(obs.post_exit_job_empty_observed);

    owner.confirm_borrowed_stdio_discharged().unwrap();
    owner
        .record_outer_lifecycle_evidence(OuterLifecycleEvidence {
            stdio_reaped_and_closed: true,
            bootstrap_handles_discharged: true,
            protocol_ack_and_sdk_quiescent: true,
            prebirth_locks_retained_until_exit: true,
        })
        .unwrap();
    assert!(owner.can_retire_handles());

    // Inject CloseHandle failure on the child process handle (second close after primary thread).
    ops.close_fail_on_handle.insert(TEST_CHILD_PROC_RAW, 6);
    let close_err = owner
        .retire_handles_with_ops(&mut ops)
        .expect_err("process CloseHandle failure must abort retirement");
    assert_eq!(close_err.raw_os_error(), Some(6));
    assert_eq!(ops.close_handle_calls, 2);
    assert_eq!(ops.closed_handles_in_order, vec![TEST_CHILD_THREAD_RAW]);
    assert_eq!(
        owner.primary_thread_handle_slot().state(),
        RetainedHandleState::Closed {
            former_raw: TEST_CHILD_THREAD_RAW
        }
    );
    assert_eq!(
        owner.process_handle_slot().state(),
        RetainedHandleState::CloseFailed {
            raw: TEST_CHILD_PROC_RAW,
            error_code: 6
        }
    );
    // Job handle was NOT closed because process CloseHandle failed.
    assert_eq!(
        owner.job_handle_slot().state(),
        RetainedHandleState::Open(TEST_JOB_RAW)
    );
    assert_eq!(
        owner.first_error().unwrap().stage,
        RetainedOperationStage::CloseProcess
    );
    assert!(owner.no_replay());
    assert_eq!(owner.disposition(), RetainedDisposition::UnknownRetained);

    // Retrying retire_handles is refused without calling CloseHandle again.
    assert!(owner.retire_handles_with_ops(&mut ops).is_err());
    assert_eq!(ops.close_handle_calls, 2);
}

#[test]
fn successful_finite_retirement_closes_thread_process_and_job_in_exact_order() {
    let mut ops = FakeRetainedNativeOps::default();
    ops.process_signaled_on_poll = true;
    ops.thread_signaled_on_poll = true;
    ops.reported_exit_code = 0;
    ops.reported_active_processes = VecDeque::from([0]);

    let mut owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    let identity = owner
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &["--supervised-owner".into(), "token".into()],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut ops,
        )
        .expect("create succeeds");
    assert_eq!(identity.pid, TEST_CHILD_PID);
    assert_eq!(identity.primary_thread_id, TEST_CHILD_TID);
    assert_eq!(identity.creation_filetime, TEST_CREATION_FT);
    assert!(identity.job_member_confirmed);
    assert!(identity.process_noninherited_confirmed);
    assert!(identity.thread_noninherited_confirmed);
    assert_eq!(
        String::from_utf16(&ops.last_environment).unwrap(),
        "LOCALAPPDATA=C:\\Synthetic\\AppData\\Local\0SystemRoot=C:\\Windows\0\0"
    );

    // Narrow original handle borrow and bootstrap duplication accounting.
    let borrowed_proc = owner.borrow_original_process_handle().unwrap();
    assert_eq!(borrowed_proc.raw_isize(), TEST_CHILD_PROC_RAW);
    let dup_slot = owner.begin_bootstrap_handle_duplication(0x5501).unwrap();
    owner
        .complete_bootstrap_handle_duplication(dup_slot, Ok(()))
        .unwrap();

    owner.resume_primary_thread_with_ops(&mut ops).unwrap();
    owner.poll_lifecycle_observations_with_ops(&mut ops).unwrap();
    owner.confirm_borrowed_stdio_discharged().unwrap();
    owner.confirm_bootstrap_duplications_discharged().unwrap();
    owner
        .record_outer_lifecycle_evidence(OuterLifecycleEvidence {
            stdio_reaped_and_closed: true,
            bootstrap_handles_discharged: true,
            protocol_ack_and_sdk_quiescent: true,
            prebirth_locks_retained_until_exit: true,
        })
        .unwrap();

    assert!(owner.can_retire_handles());
    let receipt = owner
        .retire_handles_with_ops(&mut ops)
        .expect("clean retirement succeeds");
    assert_eq!(receipt.identity, identity);
    assert_eq!(receipt.exit_code, 0);
    assert_eq!(receipt.closed_primary_thread_raw, TEST_CHILD_THREAD_RAW);
    assert_eq!(receipt.closed_process_raw, TEST_CHILD_PROC_RAW);
    assert_eq!(receipt.closed_job_raw, TEST_JOB_RAW);
    assert_eq!(
        ops.closed_handles_in_order,
        vec![TEST_CHILD_THREAD_RAW, TEST_CHILD_PROC_RAW, TEST_JOB_RAW]
    );
    assert_eq!(owner.phase(), RetainedPhase::Retired);
    assert_eq!(owner.disposition(), RetainedDisposition::RetiredCleanly);
    assert!(!owner.has_unresolved_debt());
    assert_eq!(ops.terminate_job_calls, 0);
    assert_eq!(ops.terminate_process_calls, 0);
}

#[test]
fn encoded_environment_block_validator_accepts_nonempty_builder_and_rejects_malformed_blocks() {
    // 1. Valid nonempty single-entry and multi-entry blocks from prepare_trusted_host_environment_with.
    let single_prepared = prepare_trusted_host_environment_with(&BTreeMap::new(), || {
        Ok(r"C:\Users\Admin\AppData\Local".into())
    })
    .expect("single-entry trusted host environment");
    let single_launch = PreparedChildLaunch::prepare(
        Path::new(r"C:\Program\host.exe"),
        &["--flag".into()],
        Path::new(r"C:\Work"),
        single_prepared,
    )
    .expect("PreparedChildLaunch accepts single-entry nonempty block");
    assert_eq!(
        String::from_utf16(single_launch.environment_wide()).unwrap(),
        "LOCALAPPDATA=C:\\Users\\Admin\\AppData\\Local\0\0"
    );

    let mut caller_env = BTreeMap::new();
    caller_env.insert("COMSPEC".into(), r"C:\Windows\System32\cmd.exe".into());
    caller_env.insert("SystemRoot".into(), r"C:\Windows".into());
    caller_env.insert("WINDIR".into(), r"C:\Windows".into());
    let multi_prepared = prepare_trusted_host_environment_with(&caller_env, || {
        Ok(r"C:\Synthetic\AppData\Local".into())
    })
    .expect("multi-entry trusted host environment");
    let multi_launch = PreparedChildLaunch::prepare(
        Path::new(r"C:\Program\host.exe"),
        &[],
        Path::new(r"C:\Work"),
        multi_prepared,
    )
    .expect("PreparedChildLaunch accepts multi-entry nonempty block");
    assert_eq!(
        String::from_utf16(multi_launch.environment_wide()).unwrap(),
        "COMSPEC=C:\\Windows\\System32\\cmd.exe\0LOCALAPPDATA=C:\\Synthetic\\AppData\\Local\0SystemRoot=C:\\Windows\0WINDIR=C:\\Windows\0\0"
    );

    // 2. Empty [0, 0] block remains valid for direct validator calls.
    assert!(validate_encoded_environment_block(&[0, 0]).is_ok());

    // 3. Malformed blocks rejected: out-of-bounds, missing double-NUL, extra terminator,
    // leading empty entry, interior empty entry, missing '=', empty key, invalid UTF-16.
    let encode_raw = |s: &str| -> Vec<u16> { s.encode_utf16().collect() };
    let mut invalid_surrogate = encode_raw("KEY=");
    invalid_surrogate.extend([0xD800, 0, 0]);
    let mut oversized = vec![b'A' as u16; MAX_ENV_BLOCK_UTF16_UNITS - 1];
    oversized[1] = b'=' as u16;
    oversized.extend([0, 0]);

    let malformed_cases: Vec<Vec<u16>> = vec![
        vec![],
        vec![0],
        oversized,
        encode_raw("A=1"),
        encode_raw("A=1\0"),
        vec![0, 0, 0],
        encode_raw("A=1\0\0\0"),
        encode_raw("\0A=1\0\0"),
        encode_raw("A=1\0\0B=2\0\0"),
        encode_raw("A1\0\0"),
        encode_raw("=VAL\0\0"),
        invalid_surrogate,
    ];

    for bad_block in malformed_cases {
        assert!(
            validate_encoded_environment_block(&bad_block).is_err(),
            "malformed block must be rejected: {:?}",
            bad_block
        );
        let mut ops = FakeRetainedNativeOps::default();
        let mut owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
        let err = owner
            .create_with_raw_borrowed_stdio_and_ops(
                Path::new(r"C:\Program\host.exe"),
                &[],
                Path::new(r"C:\Work"),
                PreparedTrustedHostEnvironment::from_raw_block_for_test(bad_block),
                &default_stdio_handles(),
                &mut ops,
            )
            .expect_err("malformed environment block must fail precreation validation");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(ops.create_process_calls, 0);
        assert_eq!(
            owner.job_handle_slot().state(),
            RetainedHandleState::Open(TEST_JOB_RAW)
        );
        assert!(owner.no_replay());
        assert_eq!(
            owner.first_error().unwrap().stage,
            RetainedOperationStage::PreCreationValidation
        );
    }
}

#[test]
fn set_max_observation_polls_validates_bounds_and_phase_in_place_without_dropping_owner() {
    // 1. Valid pre-create configuration in JobInstalledPreCreate.
    let mut owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    owner
        .set_max_observation_polls(64)
        .expect("valid pre-create poll budget in JobInstalledPreCreate");
    assert_eq!(owner.call_budgets().max_observation_polls, 64);
    assert_eq!(owner.phase(), RetainedPhase::JobInstalledPreCreate);
    assert_eq!(
        owner.job_handle_slot().state(),
        RetainedHandleState::Open(TEST_JOB_RAW)
    );

    owner
        .set_max_observation_polls(32)
        .expect("second valid pre-create poll budget update in JobInstalledPreCreate");
    assert_eq!(owner.call_budgets().max_observation_polls, 32);
    assert_eq!(owner.phase(), RetainedPhase::JobInstalledPreCreate);

    // 2. Invalid bounds (0 and MAX_OBSERVATION_POLLS_LIMIT + 1) preserve the same owner and Job slot.
    for invalid_bound in [0u32, MAX_OBSERVATION_POLLS_LIMIT + 1] {
        let mut bad_owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
        let err = bad_owner
            .set_max_observation_polls(invalid_bound)
            .expect_err("out-of-bounds poll budget must fail in place");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(
            bad_owner.job_handle_slot().state(),
            RetainedHandleState::Open(TEST_JOB_RAW)
        );
        assert_eq!(
            bad_owner.call_budgets().max_observation_polls,
            DEFAULT_MAX_OBSERVATION_POLLS
        );
        assert_eq!(bad_owner.phase(), RetainedPhase::FaultedRetained);
        assert_eq!(bad_owner.disposition(), RetainedDisposition::UnknownRetained);
        assert!(bad_owner.no_replay());
        assert_eq!(
            bad_owner.first_error().unwrap().stage,
            RetainedOperationStage::PreCreationValidation
        );

        // Subsequent creation attempt on the same faulted owner is rejected without calling CreateProcessW.
        let mut bad_ops = FakeRetainedNativeOps::default();
        assert!(
            bad_owner
                .create_with_raw_borrowed_stdio_and_ops(
                    Path::new(r"C:\Program\host.exe"),
                    &[],
                    Path::new(r"C:\Work"),
                    pure_prepared_env(),
                    &default_stdio_handles(),
                    &mut bad_ops,
                )
                .is_err()
        );
        assert_eq!(bad_ops.create_process_calls, 0);
        assert_eq!(
            bad_owner.job_handle_slot().state(),
            RetainedHandleState::Open(TEST_JOB_RAW)
        );
    }

    // 3. Attempted late configuration after child creation preserves all original slots and call counters.
    let mut late_ops = FakeRetainedNativeOps::default();
    let mut late_owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    late_owner.set_max_observation_polls(128).unwrap();
    late_owner
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            &default_stdio_handles(),
            &mut late_ops,
        )
        .expect("create succeeds");
    assert_eq!(late_owner.phase(), RetainedPhase::CreatedSuspendedValidated);

    let late_err = late_owner
        .set_max_observation_polls(256)
        .expect_err("post-create configuration must be rejected");
    assert_eq!(late_err.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(late_owner.phase(), RetainedPhase::FaultedRetained);
    assert_eq!(late_owner.disposition(), RetainedDisposition::UnknownRetained);
    assert!(late_owner.no_replay());
    assert_eq!(
        late_owner.job_handle_slot().state(),
        RetainedHandleState::Open(TEST_JOB_RAW)
    );
    assert_eq!(
        late_owner.process_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_PROC_RAW)
    );
    assert_eq!(
        late_owner.primary_thread_handle_slot().state(),
        RetainedHandleState::Open(TEST_CHILD_THREAD_RAW)
    );
    let budgets = late_owner.call_budgets();
    assert_eq!(budgets.create_process_calls, 1);
    assert_eq!(budgets.post_create_validation_calls, 1);
    assert_eq!(budgets.max_observation_polls, 128);
    assert_eq!(late_ops.close_handle_calls, 0);
}

#[test]
fn borrowed_handle_stdio_and_extra_inherited_handles_create_and_retire_cleanly() {
    let mut ops = FakeRetainedNativeOps::default();
    let extra_1_raw: isize = 0x1104;
    let extra_2_raw: isize = 0x1105;
    ops.inheritable_handles.insert(extra_1_raw, true);
    ops.inheritable_handles.insert(extra_2_raw, true);
    ops.process_signaled_on_poll = true;
    ops.thread_signaled_on_poll = true;
    ops.reported_exit_code = 0;
    ops.reported_active_processes = VecDeque::from([0]);

    // SAFETY: synthetic distinct non-null handle values used only with FakeRetainedNativeOps.
    let stdin_bh = unsafe { BorrowedHandle::borrow_raw(h(TEST_STDIN_RAW)) };
    let stdout_bh = unsafe { BorrowedHandle::borrow_raw(h(TEST_STDOUT_RAW)) };
    let stderr_bh = unsafe { BorrowedHandle::borrow_raw(h(TEST_STDERR_RAW)) };
    let extra_1_bh = unsafe { BorrowedHandle::borrow_raw(h(extra_1_raw)) };
    let extra_2_bh = unsafe { BorrowedHandle::borrow_raw(h(extra_2_raw)) };
    let extra_slice = [extra_1_bh, extra_2_bh];

    let stdio =
        RetainedChildStdio::with_extra_inherited(stdin_bh, stdout_bh, stderr_bh, &extra_slice)
            .expect("3 stdio + 2 extra borrowed handles is within 32-handle cap");

    let mut owner = RetainedJobOwner::from_raw_job_for_test(h(TEST_JOB_RAW));
    let identity = owner
        .create_suspended_prepared_host_with_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &["--borrowed-stdio".into()],
            Path::new(r"C:\Work"),
            pure_prepared_env(),
            stdio,
            &mut ops,
        )
        .expect("creation with BorrowedHandle stdio and extra inherited handles succeeds");

    assert_eq!(identity.pid, TEST_CHILD_PID);
    assert_eq!(
        ops.last_attr_handles,
        vec![
            TEST_STDIN_RAW,
            TEST_STDOUT_RAW,
            TEST_STDERR_RAW,
            extra_1_raw,
            extra_2_raw
        ]
    );
    assert_eq!(owner.borrowed_stdio_slots().len(), 5);

    // Also test begin_bootstrap_borrowed_handle_duplication with a BorrowedHandle.
    let dup_source_bh = unsafe { BorrowedHandle::borrow_raw(h(0x6601)) };
    let dup_slot = owner
        .begin_bootstrap_borrowed_handle_duplication(dup_source_bh)
        .expect("bootstrap duplication with BorrowedHandle succeeds");
    assert_eq!(owner.bootstrap_duplication_slots()[dup_slot].source_raw, 0x6601);
    owner
        .complete_bootstrap_handle_duplication(dup_slot, Ok(()))
        .unwrap();

    owner.resume_primary_thread_with_ops(&mut ops).unwrap();
    owner.poll_lifecycle_observations_with_ops(&mut ops).unwrap();
    owner.confirm_borrowed_stdio_discharged().unwrap();
    owner.confirm_bootstrap_duplications_discharged().unwrap();
    owner
        .record_outer_lifecycle_evidence(OuterLifecycleEvidence {
            stdio_reaped_and_closed: true,
            bootstrap_handles_discharged: true,
            protocol_ack_and_sdk_quiescent: true,
            prebirth_locks_retained_until_exit: true,
        })
        .unwrap();
    assert!(owner.can_retire_handles());
    let receipt = owner.retire_handles_with_ops(&mut ops).unwrap();
    assert_eq!(receipt.exit_code, 0);
    assert_eq!(
        ops.closed_handles_in_order,
        vec![TEST_CHILD_THREAD_RAW, TEST_CHILD_PROC_RAW, TEST_JOB_RAW]
    );

    // Exceeding 32 total handles (3 stdio + 30 extra = 33) is rejected by with_extra_inherited.
    let too_many_extra = [extra_1_bh; 30];
    assert!(
        RetainedChildStdio::with_extra_inherited(stdin_bh, stdout_bh, stderr_bh, &too_many_extra)
            .is_err()
    );
}

