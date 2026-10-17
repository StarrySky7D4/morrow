//! Pure unit tests for the public `morrow_native_pipe_win::job` retained owner API,
//! `BorrowedHandle` caller-owned stdio and extra inherited handles, outer-vs-kernel
//! retirement gating, finite call budgets, unwind during close, and non-killing `Drop`
//! semantics. No OS/FFI/kernel calls are executed.

use super::*;
use crate::job_process::{
    CreateProcessAtomicResult, RetainedNativeOps, prepare_trusted_host_environment_with,
};
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    os::windows::io::BorrowedHandle,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
};
use windows_sys::Win32::System::Threading::PROCESS_INFORMATION;

const JOB_HANDLE_VAL: isize = 0x7000;
const PROC_HANDLE_VAL: isize = 0x7001;
const THREAD_HANDLE_VAL: isize = 0x7002;
const CHILD_PID_VAL: u32 = 8100;
const CHILD_TID_VAL: u32 = 8101;
const CHILD_FT_VAL: u64 = 0x01DC_6000_0000_0123;

fn raw_h(v: isize) -> HANDLE {
    v as HANDLE
}

fn synthetic_prepared_env() -> PreparedTrustedHostEnvironment {
    let mut env = BTreeMap::new();
    env.insert("WINDIR".into(), r"C:\Windows".into());
    prepare_trusted_host_environment_with(&env, || Ok(r"C:\Synthetic\Local".into()))
        .expect("synthetic prepared environment")
}

struct PureJobOps {
    pub close_calls: u32,
    pub closed_order: Vec<isize>,
    pub panic_on_close_handle: Option<isize>,
    pub create_calls: u32,
    pub resume_calls: u32,
    pub proc_signaled: bool,
    pub thread_signaled: bool,
    pub active_counts: VecDeque<u32>,
    pub inheritable: BTreeMap<isize, bool>,
}

impl Default for PureJobOps {
    fn default() -> Self {
        Self {
            close_calls: 0,
            closed_order: Vec::new(),
            panic_on_close_handle: None,
            create_calls: 0,
            resume_calls: 0,
            proc_signaled: true,
            thread_signaled: true,
            active_counts: VecDeque::from([0]),
            inheritable: BTreeMap::from([
                (0x7101, true),
                (0x7102, true),
                (0x7103, true),
                (0x7104, true),
            ]),
        }
    }
}

impl RetainedNativeOps for PureJobOps {
    fn verify_job_containment(&mut self, job_handle: HANDLE) -> io::Result<()> {
        if (job_handle as isize) != JOB_HANDLE_VAL {
            return Err(invalid("unexpected Job handle"));
        }
        Ok(())
    }

    fn job_active_processes(&mut self, _job_handle: HANDLE) -> io::Result<u32> {
        Ok(self.active_counts.pop_front().unwrap_or(0))
    }

    fn handle_inheritable(&mut self, handle: HANDLE) -> io::Result<bool> {
        Ok(*self.inheritable.get(&(handle as isize)).unwrap_or(&false))
    }

    fn set_handle_inheritable(&mut self, handle: HANDLE, inherit: bool) -> io::Result<()> {
        self.inheritable.insert(handle as isize, inherit);
        Ok(())
    }

    fn create_process_atomic(
        &mut self,
        _executable: &[u16],
        _command: &mut [u16],
        _environment: &[u16],
        _directory: &[u16],
        _attr_storage: &mut Vec<crate::job_process::AttributeStorage>,
        attr_initialized: &mut bool,
        attr_deleted: &mut bool,
        _attr_jobs: &[HANDLE; 1],
        _attr_handles: &[HANDLE],
        pi_storage: &mut PROCESS_INFORMATION,
    ) -> CreateProcessAtomicResult {
        self.create_calls += 1;
        *attr_initialized = false;
        *attr_deleted = true;
        pi_storage.hProcess = raw_h(PROC_HANDLE_VAL);
        pi_storage.hThread = raw_h(THREAD_HANDLE_VAL);
        pi_storage.dwProcessId = CHILD_PID_VAL;
        pi_storage.dwThreadId = CHILD_TID_VAL;
        CreateProcessAtomicResult {
            setup_error: None,
            create_invoked: true,
            create_ok: true,
            create_last_error: None,
            cleanup_error: None,
        }
    }

    fn process_id(&mut self, _process_handle: HANDLE) -> io::Result<u32> {
        Ok(CHILD_PID_VAL)
    }

    fn process_creation_filetime(&mut self, _process_handle: HANDLE) -> io::Result<u64> {
        Ok(CHILD_FT_VAL)
    }

    fn thread_process_id(&mut self, _thread_handle: HANDLE) -> io::Result<u32> {
        Ok(CHILD_PID_VAL)
    }

    fn thread_id(&mut self, _thread_handle: HANDLE) -> io::Result<u32> {
        Ok(CHILD_TID_VAL)
    }

    fn is_process_in_job(
        &mut self,
        _process_handle: HANDLE,
        _job_handle: HANDLE,
    ) -> io::Result<bool> {
        Ok(true)
    }

    fn wait_handle_signaled_nonblocking(&mut self, handle: HANDLE) -> io::Result<bool> {
        let raw = handle as isize;
        if raw == PROC_HANDLE_VAL {
            if !self.inheritable.contains_key(&-999) {
                // First wait is post-create validation (must be unsignaled while suspended).
                self.inheritable.insert(-999, true);
                Ok(false)
            } else {
                Ok(self.proc_signaled)
            }
        } else if raw == THREAD_HANDLE_VAL {
            Ok(self.thread_signaled)
        } else {
            Ok(false)
        }
    }

    fn process_exit_code(&mut self, _process_handle: HANDLE) -> io::Result<u32> {
        Ok(0)
    }

    fn resume_thread(&mut self, _thread_handle: HANDLE) -> Result<u32, u32> {
        self.resume_calls += 1;
        Ok(1)
    }

    fn close_handle(&mut self, handle: HANDLE) -> io::Result<()> {
        let raw = handle as isize;
        self.close_calls += 1;
        if self.panic_on_close_handle == Some(raw) {
            panic!("synthetic unwind inside close_handle");
        }
        self.closed_order.push(raw);
        Ok(())
    }
}

#[test]
fn retirement_gate_rejects_missing_outer_or_kernel_evidence_and_unresolved_drop_never_closes() {
    let mut ops = PureJobOps {
        proc_signaled: true,
        thread_signaled: false,
        active_counts: VecDeque::from([0, 0]),
        ..Default::default()
    };
    let mut owner = RetainedJobOwner::from_raw_job_for_test(raw_h(JOB_HANDLE_VAL));
    // SAFETY: synthetic handle values used only with PureJobOps.
    let stdin_bh = unsafe { BorrowedHandle::borrow_raw(raw_h(0x7101)) };
    let stdout_bh = unsafe { BorrowedHandle::borrow_raw(raw_h(0x7102)) };
    let stderr_bh = unsafe { BorrowedHandle::borrow_raw(raw_h(0x7103)) };
    let stdio = RetainedChildStdio::new(stdin_bh, stdout_bh, stderr_bh);
    owner
        .create_suspended_prepared_host_with_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            synthetic_prepared_env(),
            stdio,
            &mut ops,
        )
        .unwrap();
    owner.resume_primary_thread_with_ops(&mut ops).unwrap();

    // Process signals, active_processes == 0, but primary thread has NOT signaled yet.
    owner.poll_lifecycle_observations_with_ops(&mut ops).unwrap();
    owner.confirm_borrowed_stdio_discharged().unwrap();
    owner
        .record_outer_lifecycle_evidence(OuterLifecycleEvidence {
            stdio_reaped_and_closed: true,
            bootstrap_handles_discharged: true,
            protocol_ack_and_sdk_quiescent: true,
            prebirth_locks_retained_until_exit: true,
        })
        .unwrap();
    assert!(
        !owner.can_retire_handles(),
        "count==0 and process signal without primary thread signal cannot unlock retirement"
    );

    // Now primary thread signals, but outer protocol_ack_and_sdk_quiescent is false.
    ops.thread_signaled = true;
    owner.poll_lifecycle_observations_with_ops(&mut ops).unwrap();
    owner
        .record_outer_lifecycle_evidence(OuterLifecycleEvidence {
            stdio_reaped_and_closed: true,
            bootstrap_handles_discharged: true,
            protocol_ack_and_sdk_quiescent: false,
            prebirth_locks_retained_until_exit: true,
        })
        .unwrap();
    assert!(
        !owner.can_retire_handles(),
        "missing outer ACK/SDK quiescence must block kernel handle retirement"
    );

    // Dropping an unresolved owner must not close handles or kill C.
    drop(owner);
    assert_eq!(ops.close_calls, 0);
}

#[test]
fn finite_borrow_and_observation_poll_budgets_enforce_strict_upper_bounds() {
    let mut ops = PureJobOps {
        proc_signaled: false,
        thread_signaled: false,
        ..Default::default()
    };
    let mut owner = RetainedJobOwner::from_raw_job_for_test(raw_h(JOB_HANDLE_VAL));
    owner
        .set_max_observation_polls(2)
        .expect("pre-create poll budget configuration succeeds");
    owner
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            synthetic_prepared_env(),
            &[raw_h(0x7101), raw_h(0x7102), raw_h(0x7103)],
            &mut ops,
        )
        .unwrap();

    // Exhaust the 32-borrow budget.
    for _ in 0..MAX_ORIGINAL_HANDLE_BORROWS {
        assert!(owner.borrow_original_process_handle().is_ok());
    }
    let borrow_err = owner
        .borrow_original_process_handle()
        .expect_err("33rd borrow must be rejected");
    assert_eq!(borrow_err.kind(), io::ErrorKind::InvalidData);
    assert_eq!(
        owner.first_error().unwrap().stage,
        RetainedOperationStage::OriginalHandleBorrow
    );

    // Test observation poll budget exhaustion on a separate active owner.
    let mut ops2 = PureJobOps {
        proc_signaled: false,
        thread_signaled: false,
        ..Default::default()
    };
    let mut owner2 = RetainedJobOwner::from_raw_job_for_test(raw_h(JOB_HANDLE_VAL));
    owner2.set_max_observation_polls(2).unwrap();
    owner2
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            synthetic_prepared_env(),
            &[raw_h(0x7101), raw_h(0x7102), raw_h(0x7103)],
            &mut ops2,
        )
        .unwrap();
    owner2.resume_primary_thread_with_ops(&mut ops2).unwrap();
    assert_eq!(owner2.poll_process_exit_with_ops(&mut ops2).unwrap(), None);
    assert_eq!(owner2.poll_process_exit_with_ops(&mut ops2).unwrap(), None);
    let poll_err = owner2
        .poll_process_exit_with_ops(&mut ops2)
        .expect_err("3rd poll beyond max_observation_polls=2 must fail");
    assert_eq!(poll_err.kind(), io::ErrorKind::TimedOut);
    assert_eq!(
        owner2.first_error().unwrap().stage,
        RetainedOperationStage::ObserveProcessWait
    );
}

#[test]
fn bootstrap_duplication_failure_or_in_flight_blocks_resume_and_retirement() {
    let mut ops = PureJobOps::default();
    let mut owner = RetainedJobOwner::from_raw_job_for_test(raw_h(JOB_HANDLE_VAL));
    owner
        .create_with_raw_borrowed_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            synthetic_prepared_env(),
            &[raw_h(0x7101), raw_h(0x7102), raw_h(0x7103)],
            &mut ops,
        )
        .unwrap();

    // Begin a bootstrap duplication and leave it in flight: resume must be refused.
    let slot = owner.begin_bootstrap_handle_duplication(0x7701).unwrap();
    let resume_err = owner
        .resume_primary_thread_with_ops(&mut ops)
        .expect_err("in-flight bootstrap duplication must block resume_primary_thread");
    assert_eq!(resume_err.kind(), io::ErrorKind::InvalidData);
    assert_eq!(ops.resume_calls, 0);
    assert_eq!(
        owner.first_error().unwrap().stage,
        RetainedOperationStage::ResumePrimaryThread
    );
    assert_eq!(owner.bootstrap_duplication_slots()[slot].source_raw, 0x7701);
    assert!(owner.bootstrap_duplication_slots()[slot].in_flight);
    assert!(owner.has_unresolved_debt());
}

#[test]
fn borrowed_stdio_full_retirement_and_close_unwind_preserve_exact_slot_states() {
    // SAFETY: synthetic distinct non-null handle values used only with PureJobOps.
    let stdin_bh = unsafe { BorrowedHandle::borrow_raw(raw_h(0x7101)) };
    let stdout_bh = unsafe { BorrowedHandle::borrow_raw(raw_h(0x7102)) };
    let stderr_bh = unsafe { BorrowedHandle::borrow_raw(raw_h(0x7103)) };
    let extra_bh = unsafe { BorrowedHandle::borrow_raw(raw_h(0x7104)) };
    let extra_slice = [extra_bh];
    let stdio =
        RetainedChildStdio::with_extra_inherited(stdin_bh, stdout_bh, stderr_bh, &extra_slice)
            .expect("valid borrowed stdio with extra inherited handle");

    // 1. Clean retirement on borrowed-handle path requires borrowed stdio + extra discharge
    // and closes hThread, hProcess, then Job in exact order.
    let mut clean_ops = PureJobOps::default();
    let mut clean_owner = RetainedJobOwner::from_raw_job_for_test(raw_h(JOB_HANDLE_VAL));
    clean_owner
        .create_suspended_prepared_host_with_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            synthetic_prepared_env(),
            stdio,
            &mut clean_ops,
        )
        .expect("borrowed-handle create succeeds");
    assert_eq!(clean_owner.borrowed_stdio_slots().len(), 4);
    clean_owner
        .resume_primary_thread_with_ops(&mut clean_ops)
        .unwrap();
    clean_owner
        .poll_lifecycle_observations_with_ops(&mut clean_ops)
        .unwrap();
    clean_owner
        .record_outer_lifecycle_evidence(OuterLifecycleEvidence {
            stdio_reaped_and_closed: true,
            bootstrap_handles_discharged: true,
            protocol_ack_and_sdk_quiescent: true,
            prebirth_locks_retained_until_exit: true,
        })
        .unwrap();
    // Before confirm_borrowed_stdio_discharged(), retirement must be blocked because
    // the 4 borrowed inherited slots have not been discharged yet.
    assert!(!clean_owner.can_retire_handles());
    clean_owner.confirm_borrowed_stdio_discharged().unwrap();
    assert!(clean_owner.can_retire_handles());

    clean_owner.retire_handles_with_ops(&mut clean_ops).unwrap();
    assert_eq!(
        clean_ops.closed_order,
        vec![THREAD_HANDLE_VAL, PROC_HANDLE_VAL, JOB_HANDLE_VAL]
    );
    assert_eq!(clean_owner.phase(), RetainedPhase::Retired);
    assert_eq!(
        clean_owner.disposition(),
        RetainedDisposition::RetiredCleanly
    );
    assert!(!clean_owner.has_unresolved_debt());

    // 2. Unwind during CloseHandle on the child process handle leaves the slot CloseInFlight
    // and retains the open Job handle without retrying.
    let mut ops = PureJobOps::default();
    let mut owner = RetainedJobOwner::from_raw_job_for_test(raw_h(JOB_HANDLE_VAL));
    owner
        .create_suspended_prepared_host_with_stdio_and_ops(
            Path::new(r"C:\Program\host.exe"),
            &[],
            Path::new(r"C:\Work"),
            synthetic_prepared_env(),
            stdio,
            &mut ops,
        )
        .expect("borrowed-handle create succeeds");

    owner.resume_primary_thread_with_ops(&mut ops).unwrap();
    owner.poll_lifecycle_observations_with_ops(&mut ops).unwrap();
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

    ops.panic_on_close_handle = Some(PROC_HANDLE_VAL);
    let unwind_res = catch_unwind(AssertUnwindSafe(|| {
        let _ = owner.retire_handles_with_ops(&mut ops);
    }));
    assert!(unwind_res.is_err());
    assert_eq!(
        owner.first_error().unwrap().stage,
        RetainedOperationStage::UnwindDuringOperation
    );
    assert_eq!(owner.phase(), RetainedPhase::FaultedRetained);
    assert_eq!(owner.disposition(), RetainedDisposition::UnknownRetained);
    assert_eq!(
        owner.primary_thread_handle_slot().state(),
        RetainedHandleState::Closed {
            former_raw: THREAD_HANDLE_VAL
        }
    );
    assert_eq!(
        owner.process_handle_slot().state(),
        RetainedHandleState::CloseInFlight(PROC_HANDLE_VAL)
    );
    assert_eq!(
        owner.job_handle_slot().state(),
        RetainedHandleState::Open(JOB_HANDLE_VAL)
    );
    assert!(owner.retire_handles_with_ops(&mut ops).is_err());
    assert_eq!(ops.close_calls, 2);
}
