use super::checked_lifecycle;
#[allow(unused_imports)]
use super::checked_lifecycle::{
    CheckedLifecycleLedger, ChildCreationFactState, ChildExitFactState, ChildExitWitness,
    ChildNativeFactsSnapshot, ChildProcessIdentity, ConptyResizeOwner, ExtraJobMemberOriginFact,
    FinalReceiptWriterFactState, HandleCloseDebt, HostOwnedResourceScopeFact, InputFrameReadError,
    JobActiveZeroFactState, JobActiveZeroWitness, JobPreserveDescendantsFactState,
    LockObservationState, NativeHandleCloseFact, OriginalThreadDisposition, OwnedWinHandle,
    RetainedRunnerContext, RetainedWorker, RunnerExitDisposition, SharedChildProcessOwner,
    StdinCloseDebtObservation, StdinHandleOwner, StdinOwnershipObservationState, StickyUnknownKind,
    StickyUnknownLatch, VerifiedChildExitFact, WorkerJoinState,
    evaluate_child_exit_observation_scalar, has_unresolved_attempt_slot_entries,
    hold_until_native_process_exit_allowed, park_timed_out_output_reader,
    park_unresolved_attempt_context, parked_attempt_context_total_count_for_test,
    parked_standalone_output_reader_count_for_test, poison_attempt_slot_for_test,
    read_input_frame_checked, reap_unresolved_attempt_slot_bounded, reserve_attempt_context_slot,
    reserve_output_reader_slot, reset_attempt_slot_for_test,
    snapshot_attempt_slot_child_native_facts, spawn_managed_input_worker,
    take_parked_attempt_context_for_test, try_snapshot_attempt_slot_child_native_facts,
    with_parked_attempt_context_for_test,
};
use codex_utils_pty::JobObject;
use codex_windows_sandbox::{
    CHECKED_IPC_PROTOCOL_VERSION, ControlOperation, ControlRequest, ControlStatus, EmptyPayload,
    FramedMessage, Message, OutputStream, StdinPayload, encode_bytes, read_frame, write_frame,
};
use std::fs::File;
use std::io::{self, Cursor, Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::PathBuf;
use std::ptr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, GetLastError, HANDLE, HANDLE_FLAG_INHERIT,
    INVALID_HANDLE_VALUE, SetHandleInformation, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThread, GetCurrentThreadId,
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess, GetThreadId,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST, ResumeThread,
    UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED,
    EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

fn create_anonymous_pipe_pair() -> io::Result<(OwnedWinHandle, OwnedWinHandle)> {
    let mut read_pipe: HANDLE = 0;
    let mut write_pipe: HANDLE = 0;
    let ok = unsafe { CreatePipe(&mut read_pipe, &mut write_pipe, ptr::null_mut(), 0) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        OwnedWinHandle::new_checked(read_pipe, "anonymous pipe read")?,
        OwnedWinHandle::new_checked(write_pipe, "anonymous pipe write")?,
    ))
}

fn create_anonymous_pipe_files() -> io::Result<(File, File)> {
    let (read_h, write_h) = create_anonymous_pipe_pair()?;
    let read_file = unsafe { File::from_raw_handle(read_h.into_raw() as _) };
    let write_file = unsafe { File::from_raw_handle(write_h.into_raw() as _) };
    Ok((read_file, write_file))
}

fn duplicate_current_process_and_thread()
-> io::Result<(OwnedWinHandle, OwnedWinHandle, ChildProcessIdentity)> {
    let current_proc = unsafe { GetCurrentProcess() };
    let current_thread = unsafe { GetCurrentThread() };

    // These are borrowed pseudo handles. In particular, the process pseudo
    // handle equals INVALID_HANDLE_VALUE numerically; it must never be held
    // by OwnedWinHandle. DuplicateHandle converts each to a real owned handle.
    let duplicate_borrowed = |borrowed: HANDLE, label: &'static str| {
        let mut duplicated: HANDLE = 0;
        let ok = unsafe {
            DuplicateHandle(
                current_proc,
                borrowed,
                current_proc,
                &mut duplicated,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        OwnedWinHandle::new_checked(duplicated, label)
    };
    let dup_proc = duplicate_borrowed(current_proc, "test current process")?;
    // If thread duplication fails, only the real process duplicate is dropped.
    // Neither borrowed pseudo handle ever gains an owning wrapper.
    let dup_thread = duplicate_borrowed(current_thread, "test current thread")?;

    let identity = ChildProcessIdentity {
        process_id: unsafe { GetCurrentProcessId() },
        thread_id: unsafe { GetCurrentThreadId() },
        creation_time_100ns: 1,
    };
    Ok((dup_proc, dup_thread, identity))
}

fn build_fixture_context(
    stdout_read: OwnedWinHandle,
    stderr_read: Option<OwnedWinHandle>,
    stdin_write: Option<OwnedWinHandle>,
) -> io::Result<RetainedRunnerContext> {
    let (dup_proc, dup_thread, identity) = duplicate_current_process_and_thread()?;
    let job = Arc::new(
        JobObject::create()
            .map_err(|err| io::Error::other(format!("create JobObject failed: {err}")))?,
    );
    let process_owner = Arc::new(SharedChildProcessOwner::new(dup_proc, identity));
    let mut context = RetainedRunnerContext {
        log_dir: PathBuf::from("."),
        process_owner,
        thread_disposition: OriginalThreadDisposition::Open(dup_thread),
        job,
        job_present: true,
        conpty_resize: ConptyResizeOwner::from_instance(None),
        stdout_owner: Some(stdout_read),
        stderr_owner: stderr_read,
        stdin_owner: Some(StdinHandleOwner::new(stdin_write)),
        stdout_worker: None,
        stderr_worker: None,
        input_worker: None,
        exit_write_worker: None,
        retained_writers: [None, None],
        frame_write_worker: None,
        error_write_deadline: None,
        native_close_runtime: None,
        native_shutdown_stage: checked_lifecycle::NativeShutdownStage::Launched,
        native_receipt_writer: FinalReceiptWriterFactState::NotStartedPendingFutureReceiptSend,
        native_input_pipe: None, native_input_close: None, native_input_start: None,
        auxiliary_close_debts: [None; 5],
        stdin_close_debt_anchor: None,
        sticky_latch: StickyUnknownLatch::new(),
        ledger: CheckedLifecycleLedger::default(),
        cached_native_facts: ChildNativeFactsSnapshot::unobserved_default(),
        _pipe_handles: None,
        attempt_reservation: None,
        auto_park_on_unresolved_drop: false,
        resolved_for_clean_drop: false,
    };
    context.refresh_native_facts_snapshot();
    Ok(context)
}

// Declare this before every native fixture/local owner, after the test-serial
// lock. Reverse local destruction first parks contexts and drops only this
// case's EOF keepers; only then may this last guard inspect/reap the registry.
// A libtest panic must not return the OS process while its Job has live members.
struct NativeFixtureProcessLifetimeGuard {
    // Own observation pins are separate from the production context's originals.
    // A failed close must outlive every local/thread owner and forbid OS exit.
    observation_close_debt: Arc<StdMutex<Option<HandleCloseDebt>>>,
}
impl NativeFixtureProcessLifetimeGuard {
    fn new() -> Self {
        Self { observation_close_debt: Arc::new(StdMutex::new(None)) }
    }
}
impl Drop for NativeFixtureProcessLifetimeGuard {
    fn drop(&mut self) {
        match std::panic::catch_unwind(hold_until_native_process_exit_allowed) {
            Ok(RunnerExitDisposition::CleanNoUnresolvedAttempt
                | RunnerExitDisposition::PhysicallyReapedStickyUnknown { .. }) => {
                if self.observation_close_debt.lock().map(|debt| debt.is_some()).unwrap_or(true) {
                    // No retry of an uncertain CloseHandle and no debt erasure.
                    loop { std::thread::park(); }
                }
            }
            // Do not double-panic/abort (which would close the Job at OS exit),
            // retry controls, resume a failed launch, clear state or kill.
            _ => loop { std::thread::park(); },
        }
    }
}

// Checked close of a temporary SAME-process observation pin. The real
// context still owns original hProcess/hThread/IO when this pin retires.
// Drop covers observe/assert unwind; uncertain close is recorded, never retried.
struct FixtureObservationPin {
    handle: OwnedWinHandle,
    debt: Arc<StdMutex<Option<HandleCloseDebt>>>,
}
impl FixtureObservationPin {
    fn new(handle: OwnedWinHandle, lifetime: &NativeFixtureProcessLifetimeGuard) -> Self {
        Self { handle, debt: Arc::clone(&lifetime.observation_close_debt) }
    }
    fn remember_debt(&self) {
        if let Some(debt) = self.handle.close_debt() {
            let mut anchor = self.debt.lock().unwrap_or_else(|error| error.into_inner());
            if anchor.is_none() { *anchor = Some(debt); }
        }
    }
    fn close(&mut self) -> io::Result<()> {
        let result = self.handle.close_checked("fixture observation process pin");
        self.remember_debt();
        result
    }
    fn observe_exit_and_close(mut self, identity: ChildProcessIdentity) -> io::Result<u32> {
        let observation = observe_native_fixture_exit(&self.handle, identity);
        // Always retire the no-longer-needed pin, also when observation failed.
        // Keep the original observation error first if both operations failed.
        let closed = self.close();
        observation.and_then(|raw| closed.map(|()| raw))
    }
}
impl Drop for FixtureObservationPin {
    fn drop(&mut self) {
        if self.handle.close_debt().is_some() {
            self.remember_debt();
        } else if self.handle.is_open() {
            let _ = self.close();
        }
    }
}

// A context taken for final observation remains owned across every assertion.
// If post-reap accounting fails or panics, restore the SAME context/Job before
// the last process lifetime guard runs; no last-Arc Job close can kill a member.
struct FixtureReapedContextOwner {
    context: Option<RetainedRunnerContext>,
    observation_accepted: bool,
}
impl FixtureReapedContextOwner {
    fn new(context: RetainedRunnerContext) -> Self {
        Self { context: Some(context), observation_accepted: false }
    }
    fn accept_after_stable_closed_observation(&mut self) {
        self.observation_accepted = true;
    }
}
impl std::ops::Deref for FixtureReapedContextOwner {
    type Target = RetainedRunnerContext;
    fn deref(&self) -> &Self::Target {
        self.context.as_ref().expect("same original reaped context")
    }
}
impl Drop for FixtureReapedContextOwner {
    fn drop(&mut self) {
        if !self.observation_accepted
            && let Some(context) = self.context.take()
        {
            let _ = park_unresolved_attempt_context(context);
        }
    }
}

// This host fixture has directly observed initial suspended total1 and final
// total2. Accept only1/2 HERE; never change the real cumulative value or extend
// production counter rules. Extra member identity/origin remains NOT_OBSERVED.
// Require two consecutive equal actual samples wholly inside the budget;
// an active member, incomplete query, terminated process or out-of-range total
// fails immediately. No sleep, synthetic total, retry of a control or kill.
fn observe_stable_closed_fixture_job(
    job: &JobObject,
    deadline: Instant,
) -> io::Result<(u32, u32, u32, Vec<usize>)> {
    let mut previous = None;
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut,
                "fixture final actual Job observation did not stabilize within budget"));
        }
        let actual = job.query_accounting_and_process_ids()?;
        if !matches!(actual.0, 1 | 2) || actual.1 != 0 || actual.2 != 0 || !actual.3.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData,
                format!("fixture final Job observation outside actual1/2,0,0,empty scope: {actual:?}")));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "fixture Job query completed late"));
        }
        if previous.as_ref() == Some(&actual) {
            eprintln!("fixture final Job actual observation: {actual:?}; extra member origin NOT_OBSERVED");
            return Ok(actual);
        }
        previous = Some(actual);
    }
}

// Every original helper JoinHandle stays owned through normal return/unwind.
// A blocker owns an extra sender for its SAME one-shot release channel: Drop
// releases that own mutex wait before joining; it does not replay a native action.
struct FixtureHelperThread<T> {
    handle: Option<std::thread::JoinHandle<T>>,
    release_on_unwind: Option<std::sync::mpsc::Sender<()>>,
}
impl<T> FixtureHelperThread<T> {
    fn new(handle: std::thread::JoinHandle<T>,
        release_on_unwind: Option<std::sync::mpsc::Sender<()>>) -> Self {
        Self { handle: Some(handle), release_on_unwind }
    }
    fn join(mut self) -> std::thread::Result<T> {
        // The normal caller has already performed the original release step.
        self.release_on_unwind = None;
        self.handle.take().expect("owned original helper JoinHandle").join()
    }
}
impl<T> Drop for FixtureHelperThread<T> {
    fn drop(&mut self) {
        if let Some(release) = self.release_on_unwind.take() { let _ = release.send(()); }
        if let Some(handle) = self.handle.take() { let _ = handle.join(); }
    }
}

// Test-only peer: the real, fixed system cmd executes only builtins, waits for
// this fixture's stdin EOF, and naturally exits with DWORD 7. No private desktop,
// token, account, service, network, PATH lookup or kill is requested. The command
// intentionally launches no descendant; extra actual Job history remains unattributed.
struct NativeReapFixture {
    context: RetainedRunnerContext,
    stdin_release: OwnedWinHandle,
    stdout_write_held: OwnedWinHandle,
}

// Same Windows attribute API/Job-list/handle-list contract as the production
// proc_thread_attr implementation. Kept local because that module is private.
// Word storage preserves native alignment; lists stay alive through CreateProcessW.
struct FixtureSpawnAttributes(Vec<usize>);
impl FixtureSpawnAttributes {
    fn new() -> io::Result<Self> {
        let mut bytes = 0;
        unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), 2, 0, &mut bytes); }
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
        if unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 2, 0, &mut bytes)
        } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(storage))
    }
    fn as_mut_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.0.as_mut_ptr().cast()
    }
    fn set_handles(&mut self, attribute: usize, handles: &mut [HANDLE]) -> io::Result<()> {
        if unsafe {
            UpdateProcThreadAttribute(self.as_mut_ptr(), 0, attribute,
                handles.as_mut_ptr().cast(), std::mem::size_of_val(handles),
                ptr::null_mut(), ptr::null_mut())
        } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
impl Drop for FixtureSpawnAttributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.as_mut_ptr()); }
    }
}

fn build_native_reap_fixture(
    _lifetime: &NativeFixtureProcessLifetimeGuard,
    separate_stderr: bool,
) -> io::Result<NativeReapFixture> {
    // Capacity is reserved before any native creation. Every post-create error
    // leaves the original context/reservation parked; Job kill-on-close cannot
    // silently kill an unresolved member by unwinding the helper.
    let reservation = reserve_attempt_context_slot()?;
    let job = Arc::new(JobObject::create_without_breakaway()?);
    let (mut child_stdin, stdin_write) = create_anonymous_pipe_pair()?;
    let (stdout_read, mut child_stdout) = create_anonymous_pipe_pair()?;
    let (stderr_read, mut child_stderr) = if separate_stderr {
        let (read, write) = create_anonymous_pipe_pair()?;
        (Some(read), Some(write))
    } else {
        // The one-reader timeout test keeps its original one-worker assertion:
        // both real child output handles feed that same actual anonymous pipe.
        (None, None)
    };
    let stdin_release = stdin_write.duplicate_same_access("fixture stdin EOF keeper")?;
    let stdout_write_held = child_stdout.duplicate_same_access("fixture stdout EOF keeper")?;
    let mut inherited = vec![child_stdin.raw(), child_stdout.raw()];
    if let Some(ref handle) = child_stderr { inherited.push(handle.raw()); }
    for &handle in &inherited {
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let mut job_handles = [job.as_raw_handle() as HANDLE];
    let mut attrs = FixtureSpawnAttributes::new()?;
    attrs.set_handles(0x0002_000D, &mut job_handles)?; // atomic JOB_LIST
    attrs.set_handles(0x0002_0002, &mut inherited)?; // only the three child std handles
    let application: Vec<u16> = r"C:\Windows\System32\cmd.exe".encode_utf16().chain(Some(0)).collect();
    let mut command: Vec<u16> =
        r#""C:\Windows\System32\cmd.exe" /d /q /c "set /p c28_fixture_line=& exit /b 7""#
            .encode_utf16().chain(Some(0)).collect();
    let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = child_stdin.raw();
    si.StartupInfo.hStdOutput = child_stdout.raw();
    si.StartupInfo.hStdError = child_stderr.as_ref().map(OwnedWinHandle::raw)
        .unwrap_or_else(|| child_stdout.raw());
    si.lpAttributeList = attrs.as_mut_ptr();
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(application.as_ptr(), command.as_mut_ptr(),
            ptr::null(), ptr::null(), 1,
            CREATE_NO_WINDOW | CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT,
            ptr::null(), ptr::null(), &si.StartupInfo, &mut pi)
    };
    let create_error = if created == 0 { Some(unsafe { GetLastError() } as i32) } else { None };
    // No validation/duplication/resume/other fallible post-launch work precedes
    // capture of the ORIGINAL hProcess, hThread, actual PID/TID, Job and pipe ends.
    let mut context = RetainedRunnerContext::install_raw_after_launch(
        Some(reservation), PathBuf::from("."), pi, Some(Arc::clone(&job)), job,
        stdout_read.into_raw(), stderr_read.map(OwnedWinHandle::into_raw).unwrap_or(INVALID_HANDLE_VALUE),
        Some(stdin_write.into_raw()), None, None,
    );
    if let Some(error) = create_error {
        context.sticky_latch.record_first(StickyUnknownKind::SpawnChildFailed,
            format!("native reap fixture CreateProcessW failed: {error}"));
        // Do not infer absence from zeros or resume/retry/terminate on this path.
        return Err(io::Error::from_raw_os_error(error));
    }
    // Capture child-end close failures in the SAME owner; never lose their debt
    // or retry an uncertain close while unwinding local endpoint wrappers.
    for (index, handle) in [&mut child_stdin, &mut child_stdout]
        .into_iter().chain(child_stderr.iter_mut()).enumerate()
    {
        if let Err(error) = handle.close_checked("fixture parent copy of child stdio") {
            context.auxiliary_close_debts[index] = handle.close_debt();
            context.sticky_latch.record_first(StickyUnknownKind::StartupValidationFailed,
                error.to_string());
            return Err(error);
        }
    }
    let context = context.validate_captured_originals(pi, true)?;
    let identity = context.process_owner.identity()?;
    identity.verify_same_process_handle(pi.hProcess)?;
    if unsafe { GetThreadId(pi.hThread) } != identity.thread_id {
        context.sticky_latch.record_first(StickyUnknownKind::StartupValidationFailed,
            "fixture original primary thread identity mismatch");
        return Err(io::Error::new(io::ErrorKind::InvalidData, "fixture primary thread mismatch"));
    }
    let (total, active, terminated, pids) = context.job.query_accounting_and_process_ids()?;
    if total != 1 || active != 1 || terminated != 0 || pids != vec![identity.process_id as usize] {
        context.sticky_latch.record_first(StickyUnknownKind::StartupValidationFailed,
            "fixture suspended original child is not the single actual Job member");
        return Err(io::Error::new(io::ErrorKind::InvalidData, "fixture populated Job mismatch"));
    }
    // The child was assigned atomically at creation and stays suspended until
    // all originals are retained and verified. Resume only its ORIGINAL hThread.
    if unsafe { ResumeThread(pi.hThread) } != 1 {
        context.sticky_latch.record_first(StickyUnknownKind::StartupValidationFailed,
            "fixture original thread resume failed or had unexpected suspend count");
        return Err(io::Error::other("fixture ResumeThread did not return previous count 1"));
    }
    Ok(NativeReapFixture { context, stdin_release, stdout_write_held })
}

fn observe_native_fixture_exit(
    wait_handle: &OwnedWinHandle,
    identity: ChildProcessIdentity,
) -> io::Result<u32> {
    identity.verify_same_process_handle(wait_handle.raw())?;
    if unsafe { WaitForSingleObject(wait_handle.raw(), 5_000) } != WAIT_OBJECT_0 {
        return Err(io::Error::new(io::ErrorKind::TimedOut, "fixture original child exit not observed"));
    }
    identity.verify_same_process_handle(wait_handle.raw())?;
    let mut raw = 0;
    if unsafe { GetExitCodeProcess(wait_handle.raw(), &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(raw)
}


#[test]
fn business_exit_requires_child_exit_and_both_output_eof_joins_and_remains_separate_from_runner_exit()
 {
    let latch = StickyUnknownLatch::new();
    let mut ledger = CheckedLifecycleLedger::default();

    // 1. Missing child_exit witness rejects business Exit.
    let err = ledger
        .authorize_business_exit(&latch, false)
        .expect_err("missing child exit must reject business Exit");
    assert_eq!(err.kind, StickyUnknownKind::BusinessExitGateViolation);
    assert!(err.no_replay);

    // 2. Child exit alone without stdout/stderr EOF + native join rejects business Exit.
    let latch = StickyUnknownLatch::new();
    let identity = ChildProcessIdentity {
        process_id: 4242,
        thread_id: 4243,
        creation_time_100ns: 123_456,
    };
    ledger.child_exit = Some(ChildExitWitness {
        identity,
        exit_code: 0,
        raw_exit_code: 0,
        timed_out: false,
    });
    assert_eq!(
        ledger
            .authorize_business_exit(&latch, false)
            .expect_err("missing stdout EOF must reject business Exit")
            .kind,
        StickyUnknownKind::BusinessExitGateViolation
    );

    // 3. Stdout EOF without stderr EOF rejects business Exit.
    let latch = StickyUnknownLatch::new();
    ledger.stdout_eof_and_joined = true;
    assert_eq!(
        ledger
            .authorize_business_exit(&latch, false)
            .expect_err("missing stderr EOF must reject business Exit")
            .kind,
        StickyUnknownKind::BusinessExitGateViolation
    );

    // 4. Explicit termination rejects clean business Exit even when outputs joined.
    let latch = StickyUnknownLatch::new();
    ledger.stderr_eof_and_joined = true;
    assert_eq!(
        ledger
            .authorize_business_exit(&latch, true)
            .expect_err("explicit termination must reject clean business Exit")
            .kind,
        StickyUnknownKind::ExplicitUserTerminate
    );

    // 5. Valid child exit + stdout/stderr EOF joins authorizes business Exit,
    // while normal-close witness remains incomplete until input retirement,
    // child handle close, and Job active=0 drain finish.
    let latch = StickyUnknownLatch::new();
    let payload = ledger
        .authorize_business_exit(&latch, false)
        .expect("valid child exit and output joins should authorize business Exit");
    assert_eq!(payload.exit_code, 0);
    assert!(!payload.timed_out);
    ledger.business_exit_sent = true;
    assert!(
        ledger.into_normal_close_witness().is_none(),
        "business Exit alone must never satisfy runner normal-close witness"
    );
}

#[test]
fn missing_stdout_eof_retains_running_join_handle_and_blocks_business_exit() {
    let (stdout_read, stdout_write_held_by_descendant) =
        create_anonymous_pipe_pair().expect("create stdout pipe");
    let mut context = build_fixture_context(stdout_read, None, None).expect("build context");
    let (mut host_out_read, runner_out_write) =
        create_anonymous_pipe_files().expect("create runner output pipe");
    let pipe_write = Arc::new(StdMutex::new(runner_out_write));

    let stdout_raw = context.stdout_owner.as_ref().unwrap().raw();
    let out_join = super::spawn_output_reader(
        Arc::clone(&pipe_write),
        stdout_raw,
        OutputStream::Stdout,
        None,
        CHECKED_IPC_PROTOCOL_VERSION,
    );
    context.stdout_worker = Some(RetainedWorker::new("runner stdout worker", out_join));

    let identity = context.process_owner.identity().unwrap();
    let witness = ChildExitWitness {
        identity,
        exit_code: 0,
        raw_exit_code: 0,
        timed_out: false,
    };
    context
        .process_owner
        .record_supplied_exit_witness_for_test(witness)
        .unwrap();
    context.ledger.child_exit = Some(witness);

    let short_deadline = Instant::now() + Duration::from_millis(25);
    let join_err = context
        .join_output_workers_bounded(short_deadline)
        .expect_err("open descendant stdout pipe must time out bounded join");
    assert_eq!(join_err.kind, StickyUnknownKind::StdoutJoinTimedOut);
    assert!(join_err.no_replay);

    let out_worker = context.stdout_worker.as_ref().unwrap();
    assert_eq!(out_worker.state(), WorkerJoinState::TimedOutStillRunning);
    assert!(
        out_worker.has_retained_join_handle(),
        "timed-out output reader JoinHandle must remain retained, never dropped"
    );

    // Attempting to send business Exit after stdout timeout must be rejected.
    {
        let exit_err = context
            .send_business_exit_checked(&pipe_write, Instant::now() + Duration::from_millis(50))
            .expect_err("business Exit must be blocked after stdout join timeout");
        assert_eq!(exit_err.kind, StickyUnknownKind::StdoutJoinTimedOut);
    }
    // Clean up fixture: close the held write end and reap the retained JoinHandle.
    drop(stdout_write_held_by_descendant);
    let cleanup_deadline = Instant::now() + Duration::from_secs(2);
    context
        .stdout_worker
        .as_mut()
        .unwrap()
        .join_bounded(cleanup_deadline)
        .expect("retained stdout worker should join once write end closes");

    drop(pipe_write);
    assert!(
        read_frame(&mut host_out_read)
            .unwrap()
            .is_none(),
        "no Exit frame may be emitted when stdout EOF was not witnessed"
    );

}

#[test]
fn late_native_worker_completion_reaps_handle_but_still_returns_timed_out() {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        ready_tx.send(()).unwrap();
        Ok::<(), io::Error>(())
    });
    ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    // Wait briefly so the thread finishes, then pass an already-expired deadline.
    std::thread::sleep(Duration::from_millis(15));
    let expired_deadline = Instant::now() - Duration::from_millis(1);

    let mut worker = RetainedWorker::new("late test worker", handle);
    let err = worker
        .join_bounded(expired_deadline)
        .expect_err("completion after deadline must return TimedOut");
    assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    assert_eq!(worker.state(), WorkerJoinState::JoinedLateAfterBudget);
    assert!(
        !worker.has_retained_join_handle(),
        "late native completion should reap the JoinHandle while still reporting TimedOut"
    );
}

#[test]
fn failed_output_forwarding_latches_sticky_unknown_and_blocks_business_exit() {
    let (stdout_read, stdout_write) = create_anonymous_pipe_pair().expect("create stdout pipe");
    {
        let mut child_out = unsafe { File::from_raw_handle(stdout_write.into_raw() as _) };
        child_out.write_all(b"hello from child\n").unwrap();
    } // `child_out` closes here after writing data.

    let mut context = build_fixture_context(stdout_read, None, None).expect("build context");
    // Open a read-only handle to a temporary file so `write_frame` fails with AccessDenied.
    let temp = tempfile::NamedTempFile::new().expect("temp file");
    let read_only_writer = File::open(temp.path()).expect("open read-only file");
    let pipe_write = Arc::new(StdMutex::new(read_only_writer));

    let stdout_raw = context.stdout_owner.as_ref().unwrap().raw();
    let out_join = super::spawn_output_reader(
        Arc::clone(&pipe_write),
        stdout_raw,
        OutputStream::Stdout,
        None,
        CHECKED_IPC_PROTOCOL_VERSION,
    );
    context.stdout_worker = Some(RetainedWorker::new("runner stdout worker", out_join));

    let identity = context.process_owner.identity().unwrap();
    let witness = ChildExitWitness {
        identity,
        exit_code: 0,
        raw_exit_code: 0,
        timed_out: false,
    };
    context
        .process_owner
        .record_supplied_exit_witness_for_test(witness)
        .unwrap();
    context.ledger.child_exit = Some(witness);

    let err = context
        .join_output_workers_bounded(Instant::now() + Duration::from_secs(2))
        .expect_err("forwarding failure must fail output worker join");
    assert_eq!(err.kind, StickyUnknownKind::StdoutForwardingFailed);
    assert!(err.no_replay);

    let sink = Arc::new(StdMutex::new(Vec::<u8>::new()));
    let exit_err = context
        .send_business_exit_checked(&sink, Instant::now() + Duration::from_millis(50))
        .expect_err("business Exit must be rejected after forwarding failure");
    assert_eq!(exit_err.kind, StickyUnknownKind::StdoutForwardingFailed);
    assert!(sink.lock().unwrap().is_empty());
}

#[test]
fn post_exit_input_eof_closes_stdin_once_and_never_calls_terminate_or_uses_closed_handle() {
    let (input_pipe_read, mut host_input_write) =
        create_anonymous_pipe_files().expect("create input pipe");
    let (child_stdin_read, child_stdin_write) =
        create_anonymous_pipe_pair().expect("create child stdin pipe");
    let mut child_stdin_reader = unsafe { File::from_raw_handle(child_stdin_read.into_raw() as _) };
    let (_runner_out_read, runner_out_write) =
        create_anonymous_pipe_files().expect("create output pipe");
    let pipe_write = Arc::new(StdMutex::new(runner_out_write));

    let (dup_proc, _dup_thread, identity) =
        duplicate_current_process_and_thread().expect("duplicate current process");
    let job = Arc::new(JobObject::create().expect("create job"));
    let process_owner = Arc::new(SharedChildProcessOwner::new(dup_proc, identity));
    let stdin_owner = StdinHandleOwner::new(Some(child_stdin_write));
    let conpty_resize = ConptyResizeOwner::from_instance(None);
    let sticky_latch = StickyUnknownLatch::new();
    let nonce = [7u8; 32];

    let mut input_worker = spawn_managed_input_worker(
        input_pipe_read,
        stdin_owner,
        conpty_resize,
        Arc::clone(&job),
        Arc::clone(&process_owner),
        None,
        Some(Arc::clone(&pipe_write)),
        nonce,
        CHECKED_IPC_PROTOCOL_VERSION,
        sticky_latch.clone(),
    );

    // Send a Stdin frame to verify child stdin forwarding works before child exit.
    write_frame(
        &mut host_input_write,
        &FramedMessage {
            version: CHECKED_IPC_PROTOCOL_VERSION,
            message: Message::Stdin {
                payload: StdinPayload {
                    data_b64: encode_bytes(b"ping"),
                },
            },
        },
    )
    .expect("write Stdin frame");

    let mut received = [0u8; 4];
    child_stdin_reader
        .read_exact(&mut received)
        .expect("read forwarded stdin bytes");
    assert_eq!(&received, b"ping");

    // Mark child exit as witnessed, then close host input endpoint (post-Exit cooperative EOF).
    process_owner.mark_child_exit_signaled_for_test();
    drop(host_input_write);

    let outcome = input_worker
        .join_bounded(Instant::now() + Duration::from_secs(2))
        .expect("input worker should natively join after host input EOF")
        .clone();
    assert!(outcome.peer_eof_observed);
    assert!(outcome.retired_after_child_exit);
    assert!(outcome.stdin_initially_open);
    assert_eq!(outcome.stdin_close_count, 1);
    assert!(!outcome.terminated_on_disconnect);
    assert!(!outcome.explicit_terminate_received);
    assert_eq!(outcome.sticky_error, None);
    assert!(!job.terminate_invoked());
    assert!(!process_owner.is_explicit_termination_latched());

    // Child stdin reader must now observe physical EOF because input worker closed stdin once.
    let mut eof_buf = [0u8; 1];
    assert_eq!(
        child_stdin_reader.read(&mut eof_buf).expect("read EOF"),
        0,
        "child stdin must reach physical EOF after input worker retirement"
    );

    // Verify that closing the process handle on the owner prevents any late fallback TerminateProcess.
    process_owner
        .close_process_handle_checked()
        .expect("checked close process handle");
    assert!(process_owner.is_process_handle_closed_verified().unwrap());
}

#[test]
fn input_partial_frame_protocol_violation_and_premature_eof_latch_sticky_unknown() {
    // 1. Clean 0-byte boundary EOF returns Ok(None).
    let mut empty = Cursor::new(Vec::<u8>::new());
    assert!(
        read_input_frame_checked(&mut empty)
            .expect("0-byte EOF should be clean")
            .is_none()
    );

    // 2. Partial 4-byte header (2 bytes before EOF) is rejected as PartialHeader.
    let mut partial_hdr = Cursor::new(vec![0x04, 0x00]);
    match read_input_frame_checked(&mut partial_hdr) {
        Err(InputFrameReadError::PartialHeader { bytes_read: 2 }) => {}
        other => panic!("expected PartialHeader(2), got {other:?}"),
    }

    // 3. Partial payload (header says 10 bytes, only 3 bytes present) is rejected as PartialPayload.
    let mut partial_body = Vec::from(10u32.to_le_bytes());
    partial_body.extend_from_slice(b"abc");
    let mut partial_cursor = Cursor::new(partial_body);
    match read_input_frame_checked(&mut partial_cursor) {
        Err(InputFrameReadError::PartialPayload {
            expected: 10,
            bytes_read: 3,
        }) => {}
        other => panic!("expected PartialPayload(10, 3), got {other:?}"),
    }

    // 4. Premature input EOF before child exit latches InputPrematureDisconnect,
    // and StickyUnknownLatch preserves the first error across subsequent recordings.
    let (input_pipe_read, host_input_write) =
        create_anonymous_pipe_files().expect("create input pipe");
    drop(host_input_write); // Immediate EOF before child exit is signaled.

    let (dup_proc, _dup_thread, identity) =
        duplicate_current_process_and_thread().expect("duplicate current process");
    let job = Arc::new(JobObject::create().expect("create job"));
    let process_owner = Arc::new(SharedChildProcessOwner::new(dup_proc, identity));
    let sticky_latch = StickyUnknownLatch::new();

    let mut worker = spawn_managed_input_worker(
        input_pipe_read,
        StdinHandleOwner::new(None),
        ConptyResizeOwner::from_instance(None),
        Arc::clone(&job),
        Arc::clone(&process_owner),
        None,
        None,
        [1u8; 32],
        CHECKED_IPC_PROTOCOL_VERSION,
        sticky_latch.clone(),
    );
    let outcome = worker
        .join_bounded(Instant::now() + Duration::from_secs(2))
        .expect("worker should join")
        .clone();
    assert!(outcome.peer_eof_observed);
    assert!(!outcome.retired_after_child_exit);
    let first = outcome
        .sticky_error
        .expect("premature EOF must latch sticky error");
    assert_eq!(first.kind, StickyUnknownKind::InputPrematureDisconnect);
    assert!(first.no_replay);

    // Recording a second error must preserve the first error.
    let second = sticky_latch.record_first(StickyUnknownKind::JobDrainFailed, "later error");
    assert_eq!(second, first);
    assert!(!job.terminate_invoked());

    // 5. Uncorrelated raw CloseStdin on v7 latches InputProtocolViolation without calling Job::terminate.
    let (input_pipe_read2, mut host_input_write2) =
        create_anonymous_pipe_files().expect("create second input pipe");
    write_frame(
        &mut host_input_write2,
        &FramedMessage {
            version: CHECKED_IPC_PROTOCOL_VERSION,
            message: Message::CloseStdin {
                payload: EmptyPayload::default(),
            },
        },
    )
    .expect("write raw CloseStdin");
    drop(host_input_write2);

    let sticky_latch2 = StickyUnknownLatch::new();
    let mut worker2 = spawn_managed_input_worker(
        input_pipe_read2,
        StdinHandleOwner::new(None),
        ConptyResizeOwner::from_instance(None),
        Arc::clone(&job),
        Arc::clone(&process_owner),
        None,
        None,
        [2u8; 32],
        CHECKED_IPC_PROTOCOL_VERSION,
        sticky_latch2,
    );
    let outcome2 = worker2
        .join_bounded(Instant::now() + Duration::from_secs(2))
        .expect("worker2 should join")
        .clone();
    assert_eq!(
        outcome2.sticky_error.unwrap().kind,
        StickyUnknownKind::InputProtocolViolation
    );
    assert!(!job.terminate_invoked());
}

#[test]
fn full_checked_normal_close_completes_in_order_without_preserve_descendants_or_forced_kill() {
    let (stdout_read, stdout_write) = create_anonymous_pipe_pair().expect("create stdout pipe");
    let (stderr_read, stderr_write) = create_anonymous_pipe_pair().expect("create stderr pipe");
    let (child_stdin_read, child_stdin_write) =
        create_anonymous_pipe_pair().expect("create child stdin pipe");
    let mut child_stdin_reader = unsafe { File::from_raw_handle(child_stdin_read.into_raw() as _) };

    let (runner_in_read, mut host_in_write) =
        create_anonymous_pipe_files().expect("create runner pipe-in");
    let (mut host_out_read, runner_out_write) =
        create_anonymous_pipe_files().expect("create runner pipe-out");
    let pipe_write = Arc::new(StdMutex::new(runner_out_write));

    let mut context = build_fixture_context(stdout_read, Some(stderr_read), Some(child_stdin_write))
        .expect("build fixture context");
    let nonce = [9u8; 32];
    context
        .start_io_workers(
            runner_in_read,
            Arc::clone(&pipe_write),
            nonce,
            CHECKED_IPC_PROTOCOL_VERSION,
        )
        .expect("start IO workers");

    // Simulate child writing stdout and stderr and then exiting cleanly (closing its stdio ends).
    {
        let mut child_out = unsafe { File::from_raw_handle(stdout_write.into_raw() as _) };
        child_out.write_all(b"stdout-ok").unwrap();
    }
    {
        let mut child_err = unsafe { File::from_raw_handle(stderr_write.into_raw() as _) };
        child_err.write_all(b"stderr-ok").unwrap();
    }

    // Exercise checked CloseStdin control ACK before child exit, or let host close input after Exit.
    // Here we send one correlated CloseStdin control request and verify Applied result before Exit,
    // then keep host_in_write open until host receives Message::Exit.
    let close_req = ControlRequest::new(1, nonce, ControlOperation::CloseStdin, 0, 0);
    write_frame(
        &mut host_in_write,
        &FramedMessage {
            version: CHECKED_IPC_PROTOCOL_VERSION,
            message: Message::ControlRequest {
                payload: close_req.clone(),
            },
        },
    )
    .expect("write CloseStdin control request");

    // Host thread: reads frames from `--pipe-out` until `Message::Exit`, then drops `host_in_write`
    // (closing `--pipe-in`) and continues reading `--pipe-out` until physical pipe EOF.
    let host_thread = std::thread::spawn(move || {
        let mut saw_control_applied = false;
        let mut saw_exit = None;
        let mut output_chunks = 0usize;
        while let Some(frame) =
            read_frame(&mut host_out_read).expect("host read frame")
        {
            assert_eq!(frame.version, CHECKED_IPC_PROTOCOL_VERSION);
            match frame.message {
                Message::ControlResult { payload } => {
                    assert_eq!(payload.request, close_req);
                    assert_eq!(payload.status, ControlStatus::Applied);
                    saw_control_applied = true;
                }
                Message::Output { .. } => {
                    output_chunks += 1;
                }
                Message::Exit { payload } => {
                    saw_exit = Some(payload);
                    // Host contract: drop host input pipe writer ONLY after receiving business Exit.
                    drop(host_in_write);
                    break;
                }
                other => panic!("unexpected frame on host output pipe: {other:?}"),
            }
        }
        // Host contract: keep reading output pipe until physical EOF after Exit.
        let trailing = read_frame(&mut host_out_read).expect("host read EOF");
        assert!(trailing.is_none(), "expected physical pipe EOF after Exit");
        (saw_control_applied, output_chunks, saw_exit.expect("saw Exit"))
    });

    // Wait briefly until child_stdin_reader observes EOF from the Applied CloseStdin control.
    let mut stdin_eof_buf = [0u8; 1];
    assert_eq!(child_stdin_reader.read(&mut stdin_eof_buf).unwrap(), 0);

    // Record verified child exit witness on the original child process owner.
    let identity = context.process_owner.identity().unwrap();
    let child_exit = ChildExitWitness {
        identity,
        exit_code: 0,
        raw_exit_code: 0,
        timed_out: false,
    };
    context
        .process_owner
        .record_supplied_exit_witness_for_test(child_exit)
        .unwrap();
    context.ledger.child_exit = Some(child_exit);

    let deadline = Instant::now() + Duration::from_secs(3);
    let witness = context
        .complete_checked_normal_close_with_drain(&pipe_write, deadline, |job, _deadline| {
            // An unpopulated fixture JobObject has OS TotalProcesses == 0, which strict OS drain rejects.
            // Verify strict OS drain rejects TotalProcesses == 0 before returning valid accounting (2, 0, 0)
            // preserving actual total_processes without faking total=1.
            let empty_err = job
                .drain_until_active_zero(
                    Instant::now() + Duration::from_millis(20),
                    Duration::from_millis(2),
                )
                .expect_err("unpopulated job with TotalProcesses==0 must be rejected");
            assert_eq!(empty_err.kind(), io::ErrorKind::InvalidData);
            Ok((2, 0, 0))
        })
        .expect("checked normal close should succeed");

    assert!(!context.job.descendants_preserved().unwrap());
    assert!(!context.job.terminate_invoked());
    assert!(context.thread_disposition.is_closed_verified());
    assert!(context.process_owner.is_process_handle_closed_verified().unwrap());
    assert_eq!(
        context.process_owner.exit_witness().unwrap(),
        Some(child_exit),
        "independent child exit witness and identity must survive handle close"
    );

    // All original workers/handles/drain are complete. Match production owner release.
    // Drop runner output pipe writer so host reader observes physical EOF.
    drop(pipe_write);
    drop(context);

    let (saw_control_applied, output_chunks, exit_payload) =
        host_thread.join().expect("join host thread");
    assert!(saw_control_applied);
    assert_eq!(output_chunks, 2);
    assert_eq!(exit_payload.exit_code, 0);
    assert!(!exit_payload.timed_out);

    assert_eq!(witness.child_exit, child_exit);
    assert!(witness.stdout_eof_and_joined);
    assert!(witness.stderr_eof_and_joined);
    assert!(witness.business_exit_sent);
    assert!(witness.input_peer_eof_and_joined);
    assert!(witness.stdin_closed_once);
    assert!(witness.child_handles_closed);
    assert_eq!(
        witness.job_active_zero,
        JobActiveZeroWitness {
            total_processes: 2,
            active_processes: 0,
            total_terminated_processes: 0,
        }
    );
    assert!(witness.runner_native_exit_ready);
}

#[test]
fn job_drain_rejects_preserve_descendants_forced_kill_and_late_budget() {
    // 1. Rejects preserve_descendants() on checked normal close
    {
        let (stdout_read, stdout_write) = create_anonymous_pipe_pair().expect("create stdout pipe");
        drop(stdout_write);
        let mut context = build_fixture_context(stdout_read, None, None).expect("build context");
        context.ledger.stdout_eof_and_joined = true;
        context.ledger.stderr_eof_and_joined = true;
        context.ledger.business_exit_sent = true;
        context.ledger.input_peer_eof_and_joined = true;
        context.ledger.stdin_closed_once = true;
        context.close_child_handles_checked().unwrap();
        context.job.preserve_descendants().unwrap();

        let err = context
            .drain_job_active_zero_bounded_with(
                Instant::now() + Duration::from_secs(1),
                |_job, _deadline| Ok((1, 0, 0)),
            )
            .expect_err("preserve_descendants must be rejected on checked normal close");
        assert_eq!(err.kind, StickyUnknownKind::JobPreserveDescendantsForbidden);
    }

    // 2. Rejects forced kill (job.terminate()) on checked normal close
    {
        let (stdout_read, stdout_write) = create_anonymous_pipe_pair().expect("create stdout pipe");
        drop(stdout_write);
        let mut context = build_fixture_context(stdout_read, None, None).expect("build context");
        context.ledger.stdout_eof_and_joined = true;
        context.ledger.stderr_eof_and_joined = true;
        context.ledger.business_exit_sent = true;
        context.ledger.input_peer_eof_and_joined = true;
        context.ledger.stdin_closed_once = true;
        context.close_child_handles_checked().unwrap();
        context.job.terminate().unwrap();

        let err = context
            .drain_job_active_zero_bounded_with(
                Instant::now() + Duration::from_secs(1),
                |_job, _deadline| Ok((1, 0, 0)),
            )
            .expect_err("job.terminate() must be rejected on checked normal close");
        assert_eq!(err.kind, StickyUnknownKind::ExplicitTerminate);
    }

    // 3. Rejects late budget expiry even if drain closure returns Ok after deadline
    {
        let (stdout_read, stdout_write) = create_anonymous_pipe_pair().expect("create stdout pipe");
        drop(stdout_write);
        let mut context = build_fixture_context(stdout_read, None, None).expect("build context");
        context.ledger.stdout_eof_and_joined = true;
        context.ledger.stderr_eof_and_joined = true;
        context.ledger.business_exit_sent = true;
        context.ledger.input_peer_eof_and_joined = true;
        context.ledger.stdin_closed_once = true;
        context.close_child_handles_checked().unwrap();

        let expired_deadline = Instant::now() - Duration::from_millis(5);
        let err = context
            .drain_job_active_zero_bounded_with(expired_deadline, |_job, _deadline| Ok((1, 0, 0)))
            .expect_err("expired deadline must reject late job drain success");
        assert_eq!(err.kind, StickyUnknownKind::JobDrainTimedOut);
    }
}

static ATTEMPT_SLOT_TEST_MUTEX: StdMutex<()> = StdMutex::new(());

fn lock_attempt_slot_test_serial() -> std::sync::MutexGuard<'static, ()> {
    ATTEMPT_SLOT_TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn unresolved_checked_context_is_parked_in_bounded_slot_and_forbids_replay() {
    let _serial = lock_attempt_slot_test_serial();
    reset_attempt_slot_for_test();
    let reservation =
        reserve_attempt_context_slot().expect("empty attempt slot should reserve cleanly");

    let (stdout_read, stdout_write) = create_anonymous_pipe_pair().expect("create stdout pipe");
    drop(stdout_write);
    let mut context = build_fixture_context(stdout_read, None, None).expect("build context");
    context.attempt_reservation = Some(reservation);
    context.sticky_latch.record_first(
        StickyUnknownKind::JobDrainTimedOut,
        "synthetic unresolved job drain timeout",
    );

    assert!(park_unresolved_attempt_context(context));
    let parked_err = with_parked_attempt_context_for_test(|ctx| ctx.sticky_latch.first_error())
        .flatten()
        .expect("parked context should retain sticky error");
    assert_eq!(parked_err.kind, StickyUnknownKind::JobDrainTimedOut);
    assert!(parked_err.no_replay);

    let replay_err = reserve_attempt_context_slot()
        .expect_err("occupied attempt slot must reject automatic replay");
    assert_eq!(replay_err.kind(), io::ErrorKind::AlreadyExists);

    let recovered = take_parked_attempt_context_for_test().expect("take parked context");
    assert_eq!(
        recovered.sticky_latch.first_error().unwrap().kind,
        StickyUnknownKind::JobDrainTimedOut
    );
    let mut clean_res =
        reserve_attempt_context_slot().expect("slot should be available after test cleanup");
    clean_res.release_pre_effect_if_uncommitted();
    drop(clean_res);
    reset_attempt_slot_for_test();
}

#[test]
fn slot_reservation_ownership_across_cancellation_unwind_poison_and_full_slot() {
    let _serial = lock_attempt_slot_test_serial();
    reset_attempt_slot_for_test();

    // 1. Holding a reservation guard blocks concurrent reservation; dropping it without
    // `complete_clean_attempt` or `release_pre_effect_if_uncommitted` (cancellation)
    // latches `ParkedUnresolved` (`no_replay=true`) and forbids replay.
    {
        let reservation =
            reserve_attempt_context_slot().expect("reserve empty slot before effects");
        let concurrent_err = reserve_attempt_context_slot()
            .expect_err("active pre-effect reservation must reject concurrent attempt");
        assert_eq!(concurrent_err.kind(), io::ErrorKind::AlreadyExists);
        drop(reservation); // Simulated cancellation before clean completion.
    }
    assert!(
        has_unresolved_attempt_slot_entries(),
        "cancelled reservation must keep slot marked unresolved"
    );
    let after_cancel_err = reserve_attempt_context_slot()
        .expect_err("cancelled reservation must forbid automatic replay");
    assert_eq!(after_cancel_err.kind(), io::ErrorKind::AlreadyExists);
    reset_attempt_slot_for_test();

    // 2. Unwind while holding a reservation guard and a live `RetainedRunnerContext`
    // auto-parks the context via `Drop` and latches `ParkedUnresolved` (`no_replay=true`).
    let unwind_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut reservation = reserve_attempt_context_slot().expect("reserve before unwind");
        reservation.mark_post_launch_admitted();
        let (stdout_read, stdout_write) = create_anonymous_pipe_pair().expect("create stdout pipe");
        drop(stdout_write);
        let mut ctx = build_fixture_context(stdout_read, None, None).expect("build context");
        ctx.attempt_reservation = Some(reservation);
        ctx.auto_park_on_unresolved_drop = true;
        panic!("simulated post-launch panic unwind");
    }));
    assert!(unwind_res.is_err(), "expected simulated panic unwind");
    assert_eq!(
        parked_attempt_context_total_count_for_test(),
        1,
        "unwound context must be retained in ATTEMPT_SLOT rather than dropped"
    );
    assert!(
        with_parked_attempt_context_for_test(|ctx| ctx.process_owner.has_open_process_handle())
            .unwrap_or(false),
        "original child process handle must remain open and retained across unwind"
    );
    let after_unwind_err = reserve_attempt_context_slot()
        .expect_err("unwound attempt must forbid automatic replay");
    assert_eq!(after_unwind_err.kind(), io::ErrorKind::AlreadyExists);

    // Poison never creates capacity. Reject the next attempt BEFORE another
    // child, Job, pipe or worker is created; the first original remains installed.
    poison_attempt_slot_for_test();
    assert_eq!(reserve_attempt_context_slot().unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(parked_attempt_context_total_count_for_test(), 1);
    // Current-process fixture is intentionally live: no physical reap is claimed.
    assert!(matches!(reap_unresolved_attempt_slot_bounded(Instant::now()),
        RunnerExitDisposition::MustHoldProcessAlive { .. }));
    drop(take_parked_attempt_context_for_test());
    reset_attempt_slot_for_test();

    let mut releases = Vec::new();
    for _ in 0..8 {
        let admission = reserve_output_reader_slot().expect("pre-admit reader");
        let (release, wait) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || { let _ = wait.recv(); Ok(()) });
        releases.push(release);
        assert!(park_timed_out_output_reader(admission, RetainedWorker::new("bounded reader", join)));
    }
    assert!(reserve_output_reader_slot().is_err(), "ninth reader rejected before spawn");
    assert_eq!(parked_standalone_output_reader_count_for_test(), 8);
    for release in releases { release.send(()).unwrap(); }
    assert!(matches!(reap_unresolved_attempt_slot_bounded(Instant::now() + Duration::from_secs(2)),
        RunnerExitDisposition::PhysicallyReapedStickyUnknown { .. }));
    assert_eq!(parked_standalone_output_reader_count_for_test(), 0);
    assert!(reserve_attempt_context_slot().is_err(), "late reap keeps noReplay sticky");
    reset_attempt_slot_for_test();
}

#[test]
fn post_launch_identity_validation_and_clone_failure_retain_all_originals() {
    let _serial = lock_attempt_slot_test_serial();
    reset_attempt_slot_for_test();

    // 1. Post-launch validation failure (`ChildProcessIdentity::validate_from_spawn` fails
    // because the process is not assigned to the fresh `JobObject`, while `stdin_open = false`
    // exercises pre-validation stdin close) must retain all original handles and Job in
    // `ATTEMPT_SLOT` rather than dropping local `OwnedWinHandle` wrappers on `Err`.
    let reservation = reserve_attempt_context_slot().expect("reserve pre-effect slot");
    let (dup_proc, dup_thread, _identity) =
        duplicate_current_process_and_thread().expect("duplicate process/thread");
    let (stdout_read, _stdout_write) = create_anonymous_pipe_pair().expect("stdout pipe");
    let (stderr_read, _stderr_write) = create_anonymous_pipe_pair().expect("stderr pipe");
    let (stdin_read, stdin_write) = create_anonymous_pipe_pair().expect("stdin pipe");
    let mut child_stdin_reader = unsafe { File::from_raw_handle(stdin_read.into_raw() as _) };

    let unassigned_job = Arc::new(JobObject::create().expect("create unassigned job"));
    let fallback_job = Arc::new(JobObject::create().expect("create fallback job"));
    let pi = PROCESS_INFORMATION {
        hProcess: dup_proc.into_raw(),
        hThread: dup_thread.into_raw(),
        dwProcessId: unsafe { GetCurrentProcessId() },
        dwThreadId: unsafe { GetCurrentThreadId() },
    };

    let admit_err = reservation
        .admit_post_launch_originals(
            PathBuf::from("."),
            pi,
            Some(Arc::clone(&unassigned_job)),
            fallback_job,
            stdout_read.into_raw(),
            stderr_read.into_raw(),
            Some(stdin_write.into_raw()),
            false, // `stdin_open = false`: closed through retained `StdinHandleOwner` after install
            None,
            None,
        )
        .expect_err("unassigned job must fail ChildProcessIdentity::validate_from_spawn");
    assert_eq!(admit_err.kind(), io::ErrorKind::InvalidData);
    assert_eq!(
        admit_err.to_string(),
        "spawned child hProcess is not a member of the retained JobObject"
    );

    // Child stdin must observe EOF because `stdin_open = false` closed the installed stdin owner.
    let mut eof_buf = [0u8; 1];
    assert_eq!(child_stdin_reader.read(&mut eof_buf).unwrap(), 0);

    // All other original handles (process, primary thread, stdout, stderr) and the `JobObject`
    // must remain installed and open inside the parked `RetainedRunnerContext`!
    assert_eq!(parked_attempt_context_total_count_for_test(), 1);
    let parked = take_parked_attempt_context_for_test().expect("take parked post-launch context");
    assert!(
        parked.process_owner.has_open_process_handle(),
        "original child process handle must remain open and retained on validation Err"
    );
    assert!(
        !parked.process_owner.is_identity_validated(),
        "identity must remain marked unvalidated when validate_from_spawn fails"
    );
    assert!(
        parked.thread_disposition.is_open(),
        "original primary thread handle must remain open and retained on validation Err"
    );
    assert!(
        parked
            .stdout_owner
            .as_ref()
            .is_some_and(OwnedWinHandle::is_open),
        "original stdout handle must remain open and retained on validation Err"
    );
    assert!(
        parked
            .stderr_owner
            .as_ref()
            .is_some_and(OwnedWinHandle::is_open),
        "original stderr handle must remain open and retained on validation Err"
    );
    assert_eq!(
        parked.sticky_latch.first_error().unwrap().kind,
        StickyUnknownKind::StartupValidationFailed
    );
    assert_eq!(
        parked.sticky_latch.first_error().unwrap().detail,
        "post-launch validation failed: spawned child hProcess is not a member of the retained JobObject"
    );
    assert!(parked.sticky_latch.first_error().unwrap().no_replay);
    drop(parked);
    reset_attempt_slot_for_test();

    // 2. Post-launch handle clone failure (`duplicate_wait_handle` failure) with
    // `auto_park_on_unresolved_drop = true` retains the context and all originals on `Err` drop.
    {
        let (stdout_read2, _stdout_write2) = create_anonymous_pipe_pair().expect("stdout pipe 2");
        let mut ctx = build_fixture_context(stdout_read2, None, None).expect("build context 2");
        ctx.auto_park_on_unresolved_drop = true;
        ctx.process_owner
            .set_force_fail_duplicate_wait_for_test(true);
        let dup_err = ctx
            .process_owner
            .duplicate_wait_handle()
            .expect_err("injected duplicate_wait_handle failure must return Err");
        ctx.sticky_latch
            .record_first(StickyUnknownKind::ChildWaitFailed, dup_err.to_string());
        // Dropping `ctx` on early `Err` return must auto-park it in `ATTEMPT_SLOT`.
    }
    assert_eq!(parked_attempt_context_total_count_for_test(), 1);
    let parked2 = take_parked_attempt_context_for_test().expect("take parked context after clone Err");
    assert!(parked2.process_owner.has_open_process_handle());
    assert!(parked2.thread_disposition.is_open());
    assert_eq!(
        parked2.sticky_latch.first_error().unwrap().kind,
        StickyUnknownKind::ChildWaitFailed
    );
    reset_attempt_slot_for_test();
}

#[test]
fn unresolved_runner_retains_in_process_and_late_physical_reaping_keeps_unknown_sticky() {
    let _serial = lock_attempt_slot_test_serial();
    reset_attempt_slot_for_test();
    let native_lifetime = NativeFixtureProcessLifetimeGuard::new(); // declared before ALL native/local owners

    let NativeReapFixture { mut context, stdin_release, stdout_write_held } =
        build_native_reap_fixture(&native_lifetime, false).expect("create owned native Job member");
    let original_identity = context.process_owner.identity().unwrap();
    let original_job = Arc::clone(&context.job);
    // Keep only this fixture's explicit EOF keeper open until the existing release step.
    context.stdin_owner.as_mut().unwrap().close_once().unwrap();
    let (_host_out_read, runner_out_write) =
        create_anonymous_pipe_files().expect("create runner output pipe");
    let pipe_write = Arc::new(StdMutex::new(runner_out_write));

    let stdout_raw = context.stdout_owner.as_ref().unwrap().raw();
    let out_join = super::spawn_output_reader(
        Arc::clone(&pipe_write),
        stdout_raw,
        OutputStream::Stdout,
        None,
        CHECKED_IPC_PROTOCOL_VERSION,
    );
    context.stdout_worker = Some(RetainedWorker::new("runner stdout worker", out_join));

    // Join times out while `stdout_write_held` is open; park unresolved context.
    let join_err = context
        .join_output_workers_bounded(Instant::now() + Duration::from_millis(20))
        .expect_err("stdout join must time out while pipe write end is open");
    assert_eq!(join_err.kind, StickyUnknownKind::StdoutJoinTimedOut);
    assert!(park_unresolved_attempt_context(context));

    // While the original stdout worker and actual contained child are still running,
    // `reap_unresolved_attempt_slot_bounded` MUST report `MustHoldProcessAlive` so the
    // runner process never returns to OS process exit while originals are active.
    let hold_disposition =
        reap_unresolved_attempt_slot_bounded(Instant::now() + Duration::from_millis(25));
    match hold_disposition {
        RunnerExitDisposition::MustHoldProcessAlive {
            sticky_error,
            running_workers,
            active_job_processes,
        } => {
            assert_eq!(sticky_error.kind, StickyUnknownKind::StdoutJoinTimedOut);
            assert!(sticky_error.no_replay);
            assert_eq!(running_workers, 1);
            assert!(active_job_processes >= 1);
        }
        other => panic!("expected MustHoldProcessAlive while originals are live, got {other:?}"),
    }
    assert!(
        with_parked_attempt_context_for_test(|ctx| ctx.process_owner.has_open_process_handle())
            .unwrap_or(false),
        "original child process handle must not be closed while stdout worker is still running"
    );

    // Spawn a helper thread that releases the actual child's stdin EOF and closes `stdout_write_held`
    // after 35ms, then verify `hold_until_native_process_exit_allowed()` blocks in-process
    // until the SAME originals finish and reaps them while keeping `Unknown`/`no_replay` sticky.
    let process_owner_for_release =
        with_parked_attempt_context_for_test(|ctx| Arc::clone(&ctx.process_owner)).unwrap();
    let wait_pin = FixtureObservationPin::new(
        process_owner_for_release.duplicate_wait_handle().unwrap(), &native_lifetime,
    );
    let releaser = FixtureHelperThread::new(std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(35));
        drop(stdin_release); // real child stdin EOF: cmd exits naturally, never killed
        drop(stdout_write_held);
        wait_pin.observe_exit_and_close(original_identity)
    }), None);

    let final_disposition = hold_until_native_process_exit_allowed();
    assert_eq!(releaser.join().expect("join releaser thread")
        .expect("observe actual original child exit"), 7);
    assert!(!original_job.terminate_invoked());
    assert!(!original_job.descendants_preserved().unwrap());

    match final_disposition {
        RunnerExitDisposition::PhysicallyReapedStickyUnknown { sticky_error } => {
            assert_eq!(sticky_error.kind, StickyUnknownKind::StdoutJoinTimedOut);
            assert!(sticky_error.no_replay);
        }
        other => panic!("expected PhysicallyReapedStickyUnknown after late reaping, got {other:?}"),
    }

    let mut reaped_ctx = FixtureReapedContextOwner::new(
        take_parked_attempt_context_for_test().expect("take reaped context"),
    );
    assert_eq!(
        reaped_ctx.stdout_worker.as_ref().unwrap().state(),
        WorkerJoinState::JoinedLateAfterBudget,
        "physical reaping must join the SAME original worker as JoinedLateAfterBudget"
    );
    assert!(
        !reaped_ctx
            .stdout_worker
            .as_ref()
            .unwrap()
            .has_retained_join_handle()
    );
    assert!(
        reaped_ctx
            .process_owner
            .is_process_handle_closed_verified()
            .unwrap(),
        "original child process handle must be checked-closed after all workers have joined"
    );
    assert!(
        reaped_ctx.thread_disposition.is_closed_verified(),
        "original primary thread handle must be checked-closed after all workers have joined"
    );
    assert!(
        !reaped_ctx.ledger.runner_native_exit_ready,
        "late physical reaping must never mark runner_native_exit_ready"
    );
    assert!(
        reaped_ctx.ledger.into_normal_close_witness().is_none(),
        "late physical reaping must never become normal business success"
    );
    // Only AFTER original helper join and verified original native/worker closes.
    let (total, active, terminated, pids) = observe_stable_closed_fixture_job(
        &original_job, Instant::now() + Duration::from_secs(2),
    ).expect("bounded stable actual Job observation after physical reaping");
    assert!(matches!(total, 1 | 2), "fixture-scoped actual cumulative total1/2 only");
    assert_eq!(active, 0);
    assert_eq!(terminated, 0);
    assert!(pids.is_empty());
    reaped_ctx.accept_after_stable_closed_observation();
    reset_attempt_slot_for_test();
}

#[test]
fn bounded_original_shutdown_writer_enforces_deadline_and_retains_writer_on_timeout() {
    let _serial = lock_attempt_slot_test_serial();
    reset_attempt_slot_for_test();
    let native_lifetime = NativeFixtureProcessLifetimeGuard::new(); // declared before ALL native/local owners

    let NativeReapFixture { mut context, stdin_release, stdout_write_held } =
        build_native_reap_fixture(&native_lifetime, true).expect("create owned native Job member");
    let original_job = Arc::clone(&context.job);
    let identity = context.process_owner.identity().unwrap();
    let (mut host_out_read, runner_out_write) =
        create_anonymous_pipe_files().expect("create runner output pipe");
    let pipe_write = Arc::new(StdMutex::new(runner_out_write));
    let out_join = super::spawn_output_reader(
        Arc::clone(&pipe_write), context.stdout_owner.as_ref().unwrap().raw(),
        OutputStream::Stdout, None, CHECKED_IPC_PROTOCOL_VERSION,
    );
    context.stdout_worker = Some(RetainedWorker::new("runner stdout worker", out_join));
    let err_join = super::spawn_output_reader(
        Arc::clone(&pipe_write), context.stderr_owner.as_ref().unwrap().raw(),
        OutputStream::Stderr, None, CHECKED_IPC_PROTOCOL_VERSION,
    );
    context.stderr_worker = Some(RetainedWorker::new("runner stderr worker", err_join));
    context.stdin_owner.as_mut().unwrap().close_once().unwrap();
    drop(stdin_release); // Only natural EOF of this fixture's own child's stdin.
    drop(stdout_write_held);
    let wait_pin = FixtureObservationPin::new(
        context.process_owner.duplicate_wait_handle().unwrap(), &native_lifetime,
    );
    assert_eq!(wait_pin.observe_exit_and_close(identity).unwrap(), 7);
    let witness = context.process_owner
        .record_verified_exit_from_original(WAIT_OBJECT_0, false).unwrap();
    assert_eq!(witness.identity, identity);
    assert_eq!(witness.raw_exit_code, 7);
    context.ledger.child_exit = Some(witness);
    context.join_output_workers_bounded(Instant::now() + Duration::from_secs(2))
        .expect("actual child stdout/stderr EOF and both original joins");
    assert!(context.ledger.stdout_eof_and_joined && context.ledger.stderr_eof_and_joined);

    // Hold `pipe_write` lock on another thread past the shutdown deadline so
    // `send_business_exit_checked` times out waiting for the mutex rather than blocking
    // indefinitely.
    let pipe_write_blocker = Arc::clone(&pipe_write);
    let (locked_tx, locked_rx) = std::sync::mpsc::channel();
    let (unlock_tx, unlock_rx) = std::sync::mpsc::channel();
    let blocker_thread = FixtureHelperThread::new(std::thread::spawn(move || {
        let _guard = pipe_write_blocker.lock().unwrap();
        locked_tx.send(()).unwrap();
        let _ = unlock_rx.recv();
    }), Some(unlock_tx.clone()));
    locked_rx.recv_timeout(Duration::from_secs(1)).unwrap();

    let start = Instant::now();
    let short_deadline = start + Duration::from_millis(30);
    let exit_err = context
        .send_business_exit_checked(&pipe_write, short_deadline)
        .expect_err("contended pipe_write lock past deadline must fail send_business_exit_checked");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "send_business_exit_checked must be bounded by deadline"
    );
    assert_eq!(exit_err.kind, StickyUnknownKind::BusinessExitWriteFailed);
    assert!(exit_err.no_replay);
    assert!(
        !context.ledger.business_exit_sent,
        "timeout must never be treated as business Exit write success"
    );

    let exit_worker = context
        .exit_write_worker
        .as_ref()
        .expect("anchored exit_write_worker must be installed in context");
    // At the deadline, the final zero-time probe may already join the SAME
    // original worker; either path must retain timeout failure, never success.
    match exit_worker.state() {
        WorkerJoinState::TimedOutStillRunning => {
            assert!(
                exit_worker.has_retained_join_handle(),
                "still-running exit_write_worker must retain its original JoinHandle"
            );
            assert!(
                exit_worker.outcome().is_none(),
                "still-running exit_write_worker must not have a joined outcome"
            );
        }
        WorkerJoinState::JoinedLateAfterBudget => {
            assert!(
                !exit_worker.has_retained_join_handle(),
                "late-joined exit_write_worker must have joined its original JoinHandle"
            );
            match exit_worker.outcome() {
                Some(Err(error)) => assert_eq!(
                    error.kind(),
                    io::ErrorKind::TimedOut,
                    "late-joined original writer must retain its actual timeout error"
                ),
                _ => panic!("late-joined original writer must have Some(Err(TimedOut))"),
            }
        }
        other => panic!("expected retained timeout or late-joined timeout, got {other:?}"),
    }
    assert_eq!(
        context.retained_writers.iter().flatten().count(),
        1,
        "pipe_write Arc must remain anchored in context.retained_writers"
    );

    // Release the blocker thread; the writer sees that `deadline` has already expired
    // after acquiring the mutex and exits without writing a late `Message::Exit` frame.
    let _ = unlock_tx.send(());
    blocker_thread.join().expect("join blocker thread");

    assert!(park_unresolved_attempt_context(context));
    let disposition = hold_until_native_process_exit_allowed();
    match disposition {
        RunnerExitDisposition::PhysicallyReapedStickyUnknown { sticky_error } => {
            assert_eq!(
                sticky_error.kind,
                StickyUnknownKind::BusinessExitWriteFailed
            );
            assert!(sticky_error.no_replay);
        }
        other => panic!("expected PhysicallyReapedStickyUnknown, got {other:?}"),
    }

    let mut reaped = FixtureReapedContextOwner::new(
        take_parked_attempt_context_for_test().expect("take reaped context"),
    );
    assert_eq!(
        reaped.exit_write_worker.as_ref().unwrap().state(),
        WorkerJoinState::JoinedLateAfterBudget
    );
    assert!(!reaped.ledger.business_exit_sent);
    assert!(reaped.process_owner.is_process_handle_closed_verified().unwrap());
    assert!(reaped.thread_disposition.is_closed_verified());
    assert!(!original_job.terminate_invoked());
    assert!(!original_job.descendants_preserved().unwrap());
    // The observer pin is already checked-closed; require the final SAME-Job
    // samples only AFTER original hProcess/hThread and real workers retired.
    let (total, active, terminated, pids) = observe_stable_closed_fixture_job(
        &original_job, Instant::now() + Duration::from_secs(2),
    ).expect("bounded stable actual Job observation after physical reaping");
    assert!(matches!(total, 1 | 2), "fixture-scoped actual cumulative total1/2 only");
    assert_eq!(active, 0);
    assert_eq!(terminated, 0);
    assert!(pids.is_empty());
    reaped.accept_after_stable_closed_observation();
    drop(reaped);
    drop(pipe_write);
    assert!(
        read_frame(&mut host_out_read)
            .unwrap()
            .is_none(),
        "late shutdown writer must not emit a post-deadline Exit frame"
    );
    reset_attempt_slot_for_test();
}

#[test]
fn nonzero_child_exit_status_propagates_across_checked_and_legacy_paths() {
    // Stable native ExitStatus retains every DWORD bit. The runner wrapper now
    // passes this same i32 directly to process::exit after original closure.
    use std::os::windows::process::ExitStatusExt;
    for raw in [0u32, 42, 192, 255, 256, 0xffff_ff00, 0xc000_0142] {
        let status = std::process::ExitStatus::from_raw(raw);
        assert_eq!(super::verified_exit_code(0, true, raw).unwrap(), status.code().unwrap());
    }

    // 2. Verify full checked normal close preserves a nonzero child exit code (256) in both
    // the emitted business `Message::Exit` frame and the returned `NormalCloseWitness`.
    let (stdout_read, stdout_write) = create_anonymous_pipe_pair().expect("create stdout pipe");
    let (stderr_read, stderr_write) = create_anonymous_pipe_pair().expect("create stderr pipe");
    drop(stdout_write);
    drop(stderr_write);

    let (runner_in_read, host_in_write) =
        create_anonymous_pipe_files().expect("create runner pipe-in");
    let (mut host_out_read, runner_out_write) =
        create_anonymous_pipe_files().expect("create runner pipe-out");
    let pipe_write = Arc::new(StdMutex::new(runner_out_write));

    let mut context = build_fixture_context(stdout_read, Some(stderr_read), None)
        .expect("build fixture context");
    let nonce = [11u8; 32];
    context
        .start_io_workers(
            runner_in_read,
            Arc::clone(&pipe_write),
            nonce,
            CHECKED_IPC_PROTOCOL_VERSION,
        )
        .expect("start IO workers");

    let host_thread = std::thread::spawn(move || {
        let frame = read_frame(&mut host_out_read)
            .expect("host read Exit frame")
            .expect("expected Exit frame");
        match frame.message {
            Message::Exit { payload } => {
                drop(host_in_write);
                payload
            }
            other => panic!("expected Exit frame, got {other:?}"),
        }
    });

    let identity = context.process_owner.identity().unwrap();
    let child_exit = ChildExitWitness {
        identity,
        exit_code: 256,
        raw_exit_code: 256,
        timed_out: false,
    };
    context
        .process_owner
        .record_supplied_exit_witness_for_test(child_exit)
        .unwrap();
    context.ledger.child_exit = Some(child_exit);

    let witness = context
        .complete_checked_normal_close_with_drain(
            &pipe_write,
            Instant::now() + Duration::from_secs(2),
            |_job, _deadline| Ok((1, 0, 0)),
        )
        .expect("checked normal close with nonzero exit code should succeed");
    drop(pipe_write);

    let exit_payload = host_thread.join().expect("join host thread");
    assert_eq!(exit_payload.exit_code, 256);
    assert!(!exit_payload.timed_out);
    assert_eq!(witness.child_exit.exit_code, 256);
    assert_eq!(witness.child_exit.raw_exit_code, 256);
}

#[test]
fn close_failure_retains_exact_raw_debt_without_retry_and_vetoes_physical_reap() {
    let (mut handle, peer) = create_anonymous_pipe_pair().unwrap();
    let raw = handle.raw();
    let mut calls = 0;
    let err = handle.close_checked_with("injected pipe", |observed| {
        assert_eq!(observed, raw);
        calls += 1;
        Err(io::Error::from_raw_os_error(6))
    }).unwrap_err();
    assert_eq!(err.raw_os_error(), Some(6));
    let debt = handle.close_debt().unwrap();
    assert_eq!(debt.raw_handle, raw as usize);
    assert_eq!(debt.os_error, Some(6));
    assert!(!handle.is_open());
    assert!(handle.close_checked_with("do not retry", |_| { calls += 1; Ok(()) }).is_err());
    assert_eq!(calls, 1);
    // This seam deliberately did not call CloseHandle. The fixture closes its known
    // still-valid test handle exactly once; production debt is never retried.
    assert_ne!(unsafe { windows_sys::Win32::Foundation::CloseHandle(raw) }, 0);
    drop(peer);
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap();
    drop(peer);
    let mut context = build_fixture_context(stdout, None, None).unwrap();
    context.auxiliary_close_debts[0] = Some(debt);
    let identity = context.process_owner.identity().unwrap();
    context.process_owner.record_supplied_exit_witness_for_test(ChildExitWitness {
        identity, exit_code: 0, raw_exit_code: 0, timed_out: false,
    }).unwrap();
    assert_eq!(context.running_worker_count(), 0);
    assert!(context.has_close_debt());
    let (running, unresolved) = context.reap_unresolved_originals_bounded(Instant::now());
    assert_eq!(running, 0);
    assert_ne!(unresolved, 0, "zero workers and empty Job cannot erase close debt");
    assert!(context.has_close_debt());
}

#[test]
fn checked_error_write_cannot_follow_retained_exit_writer_or_extend_its_deadline() {
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap(); drop(peer);
    let mut context = build_fixture_context(stdout, None, None).unwrap();
    let (release, wait) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || { let _ = wait.recv();
        Err(io::Error::other("retained original writer")) });
    context.exit_write_worker = Some(RetainedWorker::new("original exit writer", worker));
    let sink = Arc::new(StdMutex::new(Vec::<u8>::new()));
    let frame = FramedMessage { version: CHECKED_IPC_PROTOCOL_VERSION,
        message: Message::Exit { payload: codex_windows_sandbox::ExitPayload { exit_code: 256, timed_out: false } } };
    let original_deadline = Instant::now();
    assert!(context.send_frame_checked(&sink, frame, original_deadline).is_err());
    assert!(sink.lock().unwrap().is_empty());
    assert!(context.frame_write_worker.is_none());
    assert_eq!(context.running_worker_count(), 1);
    release.send(()).unwrap();
    context.exit_write_worker.as_mut().unwrap().reap_late_bounded(Instant::now() + Duration::from_secs(1));
}

#[test]
fn registry_callback_and_unwind_restore_the_same_context_without_holding_registry_lock() {
    let _serial = lock_attempt_slot_test_serial();
    reset_attempt_slot_for_test();
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap(); drop(peer);
    let context = build_fixture_context(stdout, None, None).unwrap();
    let original = Arc::as_ptr(&context.process_owner) as usize;
    park_unresolved_attempt_context(context);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_parked_attempt_context_for_test(|context| {
            assert_eq!(Arc::as_ptr(&context.process_owner) as usize, original);
            // Would deadlock if callback retained the ATTEMPT_SLOT mutex.
            assert!(reserve_attempt_context_slot().is_err());
            panic!("unwind outside registry mutex");
        });
    }));
    assert!(panicked.is_err());
    assert_eq!(with_parked_attempt_context_for_test(|ctx| Arc::as_ptr(&ctx.process_owner) as usize), Some(original));
    drop(take_parked_attempt_context_for_test());
    reset_attempt_slot_for_test();
}

#[test]
fn legacy_join_waits_for_real_original_and_its_tls_teardown() {
    let (release, wait) = std::sync::mpsc::channel();
    let (joined, observed) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || { wait.recv().unwrap(); Ok::<_, io::Error>(()) });
    let mut worker = RetainedWorker::new("legacy output", handle);
    let owner = std::thread::spawn(move || { worker.join_legacy().unwrap().as_ref().unwrap(); joined.send(worker.state()).unwrap(); });
    assert!(observed.recv_timeout(Duration::from_millis(20)).is_err());
    release.send(()).unwrap();
    assert_eq!(observed.recv_timeout(Duration::from_secs(1)).unwrap(), WorkerJoinState::JoinedTimely);
    owner.join().unwrap();
}

#[test]
fn checked_frame_actual_blocking_write_retains_same_worker_and_bounds_the_observer() {
    struct BlockOnce {
        entered: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
        blocked: bool,
    }
    impl Write for BlockOnce {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.blocked {
                self.blocked = true;
                self.entered.send(()).unwrap();
                self.release.recv().unwrap();
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap(); drop(peer);
    let mut context = build_fixture_context(stdout, None, None).unwrap();
    let (entered, entered_rx) = std::sync::mpsc::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let sink = Arc::new(StdMutex::new(BlockOnce { entered, release: wait, blocked: false }));
    let frame = || FramedMessage { version: CHECKED_IPC_PROTOCOL_VERSION,
        message: Message::Exit { payload: codex_windows_sandbox::ExitPayload { exit_code: 256, timed_out: false } } };
    let started = Instant::now();
    let deadline = started + Duration::from_millis(200);
    assert!(context.send_frame_checked(&sink, frame(), deadline).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(context.frame_write_worker.as_ref().unwrap().has_retained_join_handle());
    assert!(context.send_frame_checked(&sink, frame(), deadline).is_err());
    assert_eq!(context.running_worker_count(), 1);
    release.send(()).unwrap();
    assert!(context.frame_write_worker.as_mut().unwrap().reap_late_bounded(
        Instant::now() + Duration::from_secs(1)));
    assert!(matches!(context.frame_write_worker.as_ref().unwrap().outcome(), Some(Err(_))),
        "late IO completion is never timely write success");
}

#[test]
fn legacy_terminal_input_cancels_original_blocked_stdin_write_then_observes_real_eof() {
    let (input, mut host_input) = create_anonymous_pipe_files().unwrap();
    let (child_read, child_write) = create_anonymous_pipe_pair().unwrap();
    let (process, _thread, identity) = duplicate_current_process_and_thread().unwrap();
    let process_owner = Arc::new(SharedChildProcessOwner::new(process, identity));
    let job = Arc::new(JobObject::create().unwrap());
    let mut worker = spawn_managed_input_worker(input,
        StdinHandleOwner::new(Some(child_write)), ConptyResizeOwner::from_instance(None),
        Arc::clone(&job), Arc::clone(&process_owner), None, None, [0; 32],
        codex_windows_sandbox::IPC_PROTOCOL_VERSION, StickyUnknownLatch::new());
    write_frame(&mut host_input, &FramedMessage { version: codex_windows_sandbox::IPC_PROTOCOL_VERSION,
        message: Message::Stdin { payload: StdinPayload { data_b64: encode_bytes(&vec![b'x'; 256 * 1024]) } } }).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let mut available = 0;
        let ok = unsafe { windows_sys::Win32::System::Pipes::PeekNamedPipe(child_read.raw(),
            std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut available, std::ptr::null_mut()) };
        assert_ne!(ok, 0);
        if available > 0 { break; }
        assert!(Instant::now() < deadline, "guest never entered original child-stdin write");
        std::thread::sleep(Duration::from_millis(1));
    }
    process_owner.mark_child_exit_signaled_for_test();
    drop(host_input); // Real host transport EOF, independent of retained child stdin reader.
    let outcome = worker.join_legacy_terminal_input().unwrap().clone();
    assert!(outcome.peer_eof_observed && outcome.retired_after_child_exit);
    assert!(!outcome.terminated_on_disconnect && !outcome.explicit_terminate_received);
    assert_eq!(outcome.stdin_close_count, 1);
    assert!(!job.terminate_invoked());
    assert!(!process_owner.is_explicit_termination_latched());
    assert!(!worker.has_retained_join_handle());
    // The still-retained descendant-side endpoint receives EOF and remains usable.
    let mut child_reader = unsafe { File::from_raw_handle(child_read.into_raw() as _) };
    let mut tail = Vec::new();
    child_reader.read_to_end(&mut tail).unwrap();
}

#[test]
fn legacy_partial_terminal_frame_drains_to_real_eof_without_terminating_preserved_processes() {
    let (input, mut host_input) = create_anonymous_pipe_files().unwrap();
    let (process, _thread, identity) = duplicate_current_process_and_thread().unwrap();
    let process_owner = Arc::new(SharedChildProcessOwner::new(process, identity));
    process_owner.mark_child_exit_signaled_for_test();
    let job = Arc::new(JobObject::create().unwrap());
    let mut worker = spawn_managed_input_worker(input, StdinHandleOwner::new(None),
        ConptyResizeOwner::from_instance(None), Arc::clone(&job), Arc::clone(&process_owner),
        None, None, [0; 32], codex_windows_sandbox::IPC_PROTOCOL_VERSION, StickyUnknownLatch::new());
    // Cancellation during a host write can leave a partial header after terminal.
    host_input.write_all(&[0x10, 0x00]).unwrap();
    drop(host_input);
    let outcome = worker.join_legacy_terminal_input().unwrap().clone();
    assert!(outcome.peer_eof_observed && outcome.retired_after_child_exit);
    assert!(!outcome.terminated_on_disconnect);
    assert!(!outcome.explicit_terminate_received);
    assert!(!job.terminate_invoked());
    assert!(!process_owner.is_explicit_termination_latched());
    assert!(!worker.has_retained_join_handle());
}


// Native fixture for the actual retaining pipe API, including its child-side
// closes and Job. Uses an invalid token and invalid executable spelling so no
// child may be launched. Independent execution needs Windows desktop permission.
fn rejected_retaining_pipe_fixture() -> codex_windows_sandbox::PipeSpawnHandles {
    codex_windows_sandbox::spawn_process_with_pipes_retaining(
        0, &[String::from("?:\\c28-never-created.exe")], std::path::Path::new("."),
        &std::collections::HashMap::new(), codex_windows_sandbox::StdinMode::Open,
        codex_windows_sandbox::StderrMode::Separate, codex_windows_sandbox::ConsoleMode::NoWindow,
        codex_windows_sandbox::LaunchDesktop::prepare(None).expect("test private desktop"), None,
    ).expect("retaining API returns the original failed launch bundle")
}

fn capture_rejected_pipe_fixture() -> RetainedRunnerContext {
    let pipes = rejected_retaining_pipe_fixture();
    let pi = pipes.process;
    assert!(pipes.launch_error().is_some());
    assert!(pipes.creation_evidence().not_created_for(&pi).is_some());
    let job = pipes.job();
    let mut context = RetainedRunnerContext::install_raw_after_launch(
        None, PathBuf::from("."), pi, Some(Arc::clone(&job)), job,
        pipes.stdout_read, pipes.stderr_read.unwrap_or(INVALID_HANDLE_VALUE),
        pipes.stdin_write, None, Some(pipes),
    );
    context.auto_park_on_unresolved_drop = false;
    let first = context._pipe_handles.as_ref().unwrap().launch_error().unwrap().to_string();
    context.sticky_latch.record_first(StickyUnknownKind::SpawnChildFailed, first);
    context
}

#[test]
fn rejected_creation_reaps_real_pipe_originals_without_fabricating_exit_or_close_witnesses() {
    let mut context = capture_rejected_pipe_fixture();
    let first = context.sticky_latch.first_error().unwrap();
    assert!(first.no_replay);
    assert!(matches!(context.thread_disposition, OriginalThreadDisposition::NotCreated(_)));
    assert!(!context.thread_disposition.is_closed_verified());
    assert!(!context.process_owner.is_process_handle_closed_verified().unwrap());
    assert!(!context.process_owner.is_identity_validated());
    assert!(context.process_owner.close_process_handle_checked().is_err());
    assert!(context.thread_disposition.close_checked(0).is_err());
    assert!(context.process_owner.exit_witness().unwrap().is_none());
    assert_eq!(context.reap_unresolved_originals_bounded(Instant::now() + Duration::from_secs(2)), (0, 0));
    assert!(!context.stdout_owner.as_ref().unwrap().is_open());
    assert!(!context.stderr_owner.as_ref().unwrap().is_open());
    assert!(!context.stdin_owner.as_ref().unwrap().is_open());
    assert!(!context.has_close_debt());
    assert!(!context.process_owner.is_process_handle_closed_verified().unwrap());
    assert!(!context.thread_disposition.is_closed_verified());
    assert_eq!(context.sticky_latch.first_error().unwrap().detail, first.detail);
    assert!(context.sticky_latch.first_error().unwrap().no_replay);
    assert!(!context.ledger.runner_native_exit_ready);
    assert!(context.ledger.into_normal_close_witness().is_none());
}

#[test]
fn proven_absence_does_not_erase_real_pipe_close_failure_debt_or_first_error() {
    let mut context = capture_rejected_pipe_fixture();
    let first = context.sticky_latch.first_error().unwrap();
    let handle = context.stdout_owner.as_mut().unwrap();
    let raw = handle.raw();
    let mut calls = 0;
    handle.close_checked_with("injected original stdout close", |observed| {
        assert_eq!(observed, raw);
        calls += 1;
        Err(io::Error::from_raw_os_error(6))
    }).unwrap_err();
    assert_eq!(calls, 1);
    let debt = handle.close_debt().unwrap();
    assert_eq!(debt.raw_handle, raw as usize);
    assert_eq!(debt.os_error, Some(6));
    // The seam did not close the test handle. Fixture-only disposal is not a
    // production retry or a close witness; the uncertain debt stays installed.
    assert_ne!(unsafe { windows_sys::Win32::Foundation::CloseHandle(raw) }, 0);
    for _ in 0..2 {
        let (workers, active) = context.reap_unresolved_originals_bounded(Instant::now());
        assert_eq!(workers, 0);
        assert_ne!(active, 0, "absence cannot discharge an actual close debt");
        assert!(context.has_close_debt());
        assert_eq!(context.stdout_owner.as_ref().unwrap().close_debt(), Some(debt));
        assert_eq!(context.sticky_latch.first_error().unwrap().detail, first.detail);
    }
    assert_eq!(calls, 1);
}

#[test]
fn zero_originals_without_operation_evidence_still_veto_physical_reap() {
    let job = Arc::new(JobObject::create().unwrap());
    let (output, peer) = create_anonymous_pipe_pair().unwrap(); drop(peer);
    let pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let mut context = RetainedRunnerContext::install_raw_after_launch(
        None, PathBuf::from("."), pi, Some(Arc::clone(&job)), job,
        output.into_raw(), INVALID_HANDLE_VALUE, None, None, None,
    );
    context.auto_park_on_unresolved_drop = false;
    let (workers, active) = context.reap_unresolved_originals_bounded(Instant::now());
    assert_eq!(workers, 0);
    assert_ne!(active, 0, "zero numbers do not prove process absence");
    assert!(!context.process_owner.is_process_handle_closed_verified().unwrap());
    assert!(!context.thread_disposition.is_closed_verified());
    assert!(matches!(context.thread_disposition, OriginalThreadDisposition::Open(_)));
}

#[test]
fn live_created_originals_cannot_be_reclassified_by_an_absence_proof_from_another_attempt() {
    let mut absent = capture_rejected_pipe_fixture();
    let evidence = absent._pipe_handles.as_ref().unwrap().creation_evidence();
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap(); drop(peer);
    let mut created = build_fixture_context(stdout, None, None).unwrap();
    created.process_owner.set_simulated_running_for_test(true);
    created.capture_creation_evidence(unsafe { std::mem::zeroed() }, evidence);
    assert!(created.process_owner.has_open_process_handle());
    assert!(created.process_owner.is_identity_validated());
    assert!(created.thread_disposition.is_open());
    assert_ne!(created.reap_unresolved_originals_bounded(Instant::now()).1, 0);
    assert_eq!(absent.reap_unresolved_originals_bounded(Instant::now()).1, 0);
}

#[test]
fn bounded_join_error_classification_reads_the_original_panicked_workers() {
    for stderr in [false, true] {
        let (stdout, peer) = create_anonymous_pipe_pair().unwrap(); drop(peer);
        let mut context = build_fixture_context(stdout, None, None).unwrap();
        let panicked = std::thread::spawn(|| -> io::Result<()> { panic!("original output worker panic") });
        while !panicked.is_finished() { std::thread::yield_now(); }
        if stderr {
            context.stdout_worker = Some(RetainedWorker::new("stdout", std::thread::spawn(|| Ok(()))));
            context.stderr_worker = Some(RetainedWorker::new("stderr", panicked));
        } else {
            context.stdout_worker = Some(RetainedWorker::new("stdout", panicked));
        }
        let error = context.join_output_workers_bounded(Instant::now() + Duration::from_secs(2)).unwrap_err();
        assert_eq!(error.kind, if stderr { StickyUnknownKind::StderrWorkerPanicked } else { StickyUnknownKind::StdoutWorkerPanicked });
        let worker = if stderr { context.stderr_worker.as_ref().unwrap() } else { context.stdout_worker.as_ref().unwrap() };
        assert_eq!(worker.state(), WorkerJoinState::Panicked);
    }
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap(); drop(peer);
    let mut context = build_fixture_context(stdout, None, None).unwrap();
    context.ledger.business_exit_sent = true;
    let panicked = std::thread::spawn(|| -> super::checked_lifecycle::InputLoopOutcome { panic!("original input worker panic") });
    while !panicked.is_finished() { std::thread::yield_now(); }
    context.input_worker = Some(RetainedWorker::new("input", panicked));
    assert_eq!(context.join_input_worker_bounded(Instant::now() + Duration::from_secs(2)).unwrap_err().kind,
        StickyUnknownKind::InputWorkerPanicked);
    assert_eq!(context.input_worker.as_ref().unwrap().state(), WorkerJoinState::Panicked);
}

#[test]
fn checked_frame_conversion_preserves_original_write_error_without_retry_or_flush() {
    struct FailingWrite(Arc<std::sync::atomic::AtomicUsize>);
    impl Write for FailingWrite {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "original frame write failure"))
        }
        fn flush(&mut self) -> io::Result<()> { panic!("must not flush after failed frame") }
    }
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let writer = Arc::new(StdMutex::new(FailingWrite(Arc::clone(&calls))));
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap(); drop(peer);
    let mut context = build_fixture_context(stdout, None, None).unwrap();
    let frame = FramedMessage { version: CHECKED_IPC_PROTOCOL_VERSION,
        message: Message::Exit { payload: codex_windows_sandbox::ExitPayload { exit_code: 256, timed_out: false } } };
    let error = context.send_frame_checked(&writer, frame, Instant::now() + Duration::from_secs(2)).unwrap_err();
    assert!(error.to_string().contains("original frame write failure"));
    let original = context.frame_write_worker.as_ref().unwrap().outcome().unwrap().as_ref().unwrap_err();
    assert!(original.get_ref().is_some(), "anyhow error object remains the io error payload");
    assert!(original.to_string().contains("original frame write failure"));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn scalar_exit_witness_mapping_preserves_identity_filetime_and_raw_dword_including_259_after_wait_proof_and_rejects_query_only_259_or_timeout()
 {
    // Pure scalar fixture: no OS handle or kernel claim is made in this block.
    let expected_identity = ChildProcessIdentity {
        process_id: 9401,
        thread_id: 9402,
        creation_time_100ns: 133_700_000_000_000_123,
    };

    for raw_dword in [0u32, 42, 259, 0xffff_ff00, 0xc000_0142] {
        let evaluated = evaluate_child_exit_observation_scalar(
            Some(expected_identity),
            false,
            expected_identity,
            Some(WAIT_OBJECT_0),
            false,
            Some(raw_dword),
            None,
        );
        assert_eq!(
            evaluated,
            ChildExitFactState::VerifiedExit(VerifiedChildExitFact {
                process_id: expected_identity.process_id,
                thread_id: expected_identity.thread_id,
                creation_time_100ns: expected_identity.creation_time_100ns,
                creation_filetime_low: (expected_identity.creation_time_100ns & 0xFFFF_FFFF) as u32,
                creation_filetime_high: (expected_identity.creation_time_100ns >> 32) as u32,
                raw_exit_code: raw_dword,
                exit_code: raw_dword as i32,
                wait_confirmed_signaled: true,
                timed_out: false,
            }),
            "raw DWORD {raw_dword:#x} (including 259 STILL_ACTIVE) must be preserved when original wait confirmed WAIT_OBJECT_0"
        );
    }

    // Query-only 259 (`wait_return == None`) must be rejected and never become an exit witness.
    assert_eq!(
        evaluate_child_exit_observation_scalar(
            Some(expected_identity),
            false,
            expected_identity,
            None,
            false,
            Some(259),
            None,
        ),
        ChildExitFactState::QueryOnlyStillActiveRejected { raw_exit_code: 259 }
    );

    // Query-only default 0 without wait proof must remain NotObserved, never VerifiedExit(0).
    assert_eq!(
        evaluate_child_exit_observation_scalar(
            Some(expected_identity),
            false,
            expected_identity,
            None,
            false,
            Some(0),
            None,
        ),
        ChildExitFactState::NotObserved
    );

    // Timeout (`timed_out == true` or `WAIT_TIMEOUT`) cannot become a verified exit witness.
    assert_eq!(
        evaluate_child_exit_observation_scalar(
            Some(expected_identity),
            false,
            expected_identity,
            Some(0x0000_0102),
            true,
            Some(259),
            None,
        ),
        ChildExitFactState::WaitTimedOut
    );

    // Proven never-created (`not_created_proven == true`) always maps to NeverCreated.
    assert_eq!(
        evaluate_child_exit_observation_scalar(
            Some(expected_identity),
            true,
            expected_identity,
            Some(WAIT_OBJECT_0),
            false,
            Some(0),
            None,
        ),
        ChildExitFactState::NeverCreated
    );

    // Verify owner-level recording on a real duplicated handle fixture.
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap();
    drop(peer);
    let context = build_fixture_context(stdout, None, None).unwrap();
    let owner_identity = context.process_owner.identity().unwrap();

    assert!(
        context
            .process_owner
            .record_observed_exit_candidate(owner_identity, None, false, Some(259))
            .is_err(),
        "query-only 259 without wait proof must be rejected on SharedChildProcessOwner"
    );
    assert_eq!(
        context.child_native_facts_snapshot().owner.exit,
        ChildExitFactState::QueryOnlyStillActiveRejected { raw_exit_code: 259 }
    );
    assert!(context.process_owner.exit_witness().unwrap().is_none());

    context.process_owner.record_wait_timeout_from_original();
    assert_eq!(
        context.child_native_facts_snapshot().owner.exit,
        ChildExitFactState::WaitTimedOut
    );

    let verified_259 = context
        .process_owner
        .record_observed_exit_candidate(owner_identity, Some(WAIT_OBJECT_0), false, Some(259))
        .expect("WAIT_OBJECT_0 proof must accept raw exit code 259");
    assert_eq!(verified_259.raw_exit_code, 259);
    assert_eq!(verified_259.exit_code, 259);
    assert_eq!(
        context.child_native_facts_snapshot().owner.exit,
        ChildExitFactState::VerifiedExit(
            VerifiedChildExitFact::from_verified_witness(verified_259, true).unwrap()
        )
    );
}

#[test]
fn absent_process_not_created_proof_distinguished_from_created_unobserved_and_unvalidated_zero_originals()
 {
    // 1. Proven never-created child via real ProcessNotCreated proof from retaining pipe spawn.
    let absent_ctx = capture_rejected_pipe_fixture();
    let absent_snap = absent_ctx.child_native_facts_snapshot();
    assert_eq!(
        absent_snap.owner.creation,
        ChildCreationFactState::NeverCreatedProven
    );
    assert_eq!(absent_snap.owner.exit, ChildExitFactState::NeverCreated);
    assert_eq!(
        absent_snap.owner.process_handle_close,
        NativeHandleCloseFact::NeverCreated
    );
    assert_eq!(
        absent_snap.primary_thread_handle_close,
        NativeHandleCloseFact::NeverCreated
    );
    assert!(!absent_snap.job.admission_verified);
    assert!(absent_snap.job.admitted_child_identity.is_none());
    assert!(!absent_snap.normal_close_completed);
    assert!(!absent_snap.runner_native_exit_ready);

    // 2. Created and validated child whose exit has not yet been observed.
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap();
    drop(peer);
    let created_ctx = build_fixture_context(stdout, None, None).unwrap();
    let expected_identity = created_ctx.process_owner.identity().unwrap();
    let created_snap = created_ctx.child_native_facts_snapshot();
    assert_eq!(
        created_snap.owner.creation,
        ChildCreationFactState::CreatedValidated(expected_identity)
    );
    assert_eq!(created_snap.owner.exit, ChildExitFactState::NotObserved);
    assert_eq!(
        created_snap.owner.process_handle_close,
        NativeHandleCloseFact::Open
    );
    assert_eq!(
        created_snap.primary_thread_handle_close,
        NativeHandleCloseFact::Open
    );
    assert!(created_snap.job.admission_verified);
    assert_eq!(
        created_snap.job.admitted_child_identity,
        Some(expected_identity)
    );

    // 3. Unvalidated zeroed PROCESS_INFORMATION without ProcessNotCreated proof.
    let job = Arc::new(JobObject::create().unwrap());
    let (output, out_peer) = create_anonymous_pipe_pair().unwrap();
    drop(out_peer);
    let zero_pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let mut unvalidated_ctx = RetainedRunnerContext::install_raw_after_launch(
        None,
        PathBuf::from("."),
        zero_pi,
        Some(Arc::clone(&job)),
        job,
        output.into_raw(),
        INVALID_HANDLE_VALUE,
        None,
        None,
        None,
    );
    unvalidated_ctx.auto_park_on_unresolved_drop = false;
    let unvalidated_snap = unvalidated_ctx.child_native_facts_snapshot();
    assert_eq!(
        unvalidated_snap.owner.creation,
        ChildCreationFactState::NotObserved
    );
    assert_eq!(unvalidated_snap.owner.exit, ChildExitFactState::NotObserved);
    assert_eq!(
        unvalidated_snap.owner.process_handle_close,
        NativeHandleCloseFact::MissingWithoutProof
    );
    assert_eq!(
        unvalidated_snap.primary_thread_handle_close,
        NativeHandleCloseFact::MissingWithoutProof
    );
    assert!(!unvalidated_snap.job.admission_verified);
}

#[test]
fn wrong_original_identity_correlation_rejected_on_pid_tid_or_creation_filetime_mismatch() {
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap();
    drop(peer);
    let context = build_fixture_context(stdout, None, None).unwrap();
    let original_identity = context.process_owner.identity().unwrap();

    let wrong_pid = ChildProcessIdentity {
        process_id: original_identity.process_id.wrapping_add(1),
        ..original_identity
    };
    let wrong_tid = ChildProcessIdentity {
        thread_id: original_identity.thread_id.wrapping_add(1),
        ..original_identity
    };
    let reused_pid_different_filetime = ChildProcessIdentity {
        creation_time_100ns: original_identity.creation_time_100ns.wrapping_add(500),
        ..original_identity
    };

    for mismatched in [wrong_pid, wrong_tid, reused_pid_different_filetime] {
        assert!(
            context
                .process_owner
                .record_supplied_exit_witness_for_test(ChildExitWitness {
                    identity: mismatched,
                    exit_code: 0,
                    raw_exit_code: 0,
                    timed_out: false,
                })
                .is_err(),
            "mismatched identity {mismatched:?} must be rejected"
        );
        let snap = context.child_native_facts_snapshot();
        assert_eq!(
            snap.owner.exit,
            ChildExitFactState::IdentityCorrelationRejected {
                expected: original_identity,
                observed: mismatched,
            }
        );
        assert!(context.process_owner.exit_witness().unwrap().is_none());
        assert!(!context.process_owner.is_child_exit_signaled());
    }
}

#[test]
fn io_join_handle_close_and_job_debts_retained_and_extra_job_members_flagged_unproven_origin() {
    let (stdout_read, stdout_write) = create_anonymous_pipe_pair().unwrap();
    let (stderr_read, stderr_write) = create_anonymous_pipe_pair().unwrap();
    drop(stdout_write);
    drop(stderr_write);

    let (runner_in_read, host_in_write) = create_anonymous_pipe_files().unwrap();
    let (mut host_out_read, runner_out_write) = create_anonymous_pipe_files().unwrap();
    let pipe_write = Arc::new(StdMutex::new(runner_out_write));

    let mut context =
        build_fixture_context(stdout_read, Some(stderr_read), None).expect("build fixture context");
    context
        .start_io_workers(
            runner_in_read,
            Arc::clone(&pipe_write),
            [22u8; 32],
            CHECKED_IPC_PROTOCOL_VERSION,
        )
        .expect("start IO workers");

    let host_thread = std::thread::spawn(move || {
        let frame = read_frame(&mut host_out_read)
            .expect("host read Exit frame")
            .expect("expected Exit frame");
        if matches!(frame.message, Message::Exit { .. }) {
            drop(host_in_write);
        } else {
            panic!("expected Exit frame, got {:?}", frame.message);
        }
    });

    let identity = context.process_owner.identity().unwrap();
    let exit_witness = ChildExitWitness {
        identity,
        exit_code: 0,
        raw_exit_code: 0,
        timed_out: false,
    };
    context
        .process_owner
        .record_supplied_exit_witness_for_test(exit_witness)
        .unwrap();
    context.ledger.child_exit = Some(exit_witness);

    // Complete normal close up to job drain with a 2-process Job accounting result (extra member).
    context
        .complete_checked_normal_close_with_drain(
            &pipe_write,
            Instant::now() + Duration::from_secs(2),
            |_job, _deadline| Ok((2, 0, 0)),
        )
        .expect("normal close with 2 drained processes should succeed");
    drop(pipe_write);
    host_thread.join().unwrap();

    let snap = context.child_native_facts_snapshot();
    assert!(snap.normal_close_completed);
    assert!(snap.runner_native_exit_ready);
    assert_eq!(
        snap.job.active_zero_state,
        JobActiveZeroFactState::ActiveZeroVerified {
            witness: JobActiveZeroWitness {
                total_processes: 2,
                active_processes: 0,
                total_terminated_processes: 0,
            },
            extra_member_origin:
                ExtraJobMemberOriginFact::ExtraMembersObservedOriginNotProvenByJobHelper {
                    total_processes: 2,
                    total_terminated_processes: 0,
                },
        }
    );
    assert_eq!(
        snap.control_and_writers.final_receipt_writer_state,
        FinalReceiptWriterFactState::NotStartedPendingFutureReceiptSend
    );
    assert_eq!(
        snap.host_input_pipe_writer_keeper,
        HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration
    );
    assert_eq!(
        snap.host_output_pipe_reader,
        HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration
    );
    assert_eq!(
        snap.host_private_desktop_owner,
        HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration
    );
    assert_eq!(
        snap.host_quota_and_acl_keepers,
        HostOwnedResourceScopeFact::OutOfRunnerScopeRequiresHostOwnerIntegration
    );

    assert_eq!(snap.pipe_spawn_close_debts, [None; 4]);
    assert_eq!(snap.pipe_spawn_close_debt_count, 0);

    // Test-only fixture seam: inject a process handle close failure on a separate owned fixture
    // and verify debt retention. Do NOT call CloseHandle(raw) behind the owner's back or adopt/reopen
    // the uncertain handle; because no legitimate checked teardown exists after an uncertain failed
    // close, the test process intentionally retains this scoped failure debt in the still-live
    // original owner until its own natural process exit.
    let (stdout2, peer2) = create_anonymous_pipe_pair().unwrap();
    drop(peer2);
    let mut debt_ctx = build_fixture_context(stdout2, None, None).unwrap();
    debt_ctx
        .stdout_owner
        .as_mut()
        .unwrap()
        .close_if_open_checked("fixture stdout before injected process close failure")
        .unwrap();
    let tid2 = debt_ctx.process_owner.identity().unwrap().thread_id;
    debt_ctx.thread_disposition.close_checked(tid2).unwrap();
    let raw_proc = debt_ctx
        .process_owner
        .close_process_handle_checked_with(|_raw| Err(io::Error::from_raw_os_error(6)));
    assert!(raw_proc.is_err());
    assert!(
        !debt_ctx
            .process_owner
            .is_process_handle_closed_verified()
            .unwrap()
    );
    assert!(debt_ctx.has_close_debt());
    let debt_snap = debt_ctx.child_native_facts_snapshot();
    assert_eq!(
        debt_snap.owner.process_handle_close,
        NativeHandleCloseFact::CloseDebtRetained {
            os_error: Some(6),
            label: "original child process",
        }
    );
    assert_eq!(debt_snap.first_close_debt_os_error, Some(6));
    assert!(debt_snap.has_any_handle_close_debt);
    assert!(debt_snap.first_error.no_replay_latched);
    assert!(!debt_snap.normal_close_completed);
    assert!(!debt_snap.runner_native_exit_ready);
}

#[test]
fn fourth_pipe_spawn_close_debt_entry_retained_in_first_debt_debt_present_and_no_replay_facts() {
    // Pure labelled scalar fixture input for the 4-entry PipeSpawnHandles::close_debts array
    // (no fake native pipe close failure is fabricated).
    let (stdout, peer) = create_anonymous_pipe_pair().unwrap();
    drop(peer);
    let mut fixture_ctx = build_fixture_context(stdout, None, None).unwrap();

    // 1. Only the FOURTH pipe close-debt entry (index 3, closed-stdin parent write handle slot)
    // is present: must be retained in pipe_spawn_close_debts, pipe_spawn_close_debt_count,
    // first_close_debt_os_error, has_any_handle_close_debt, and first_error.no_replay_latched.
    fixture_ctx.record_scalar_pipe_spawn_close_debts_for_test([None, None, None, Some(6)]);
    assert!(fixture_ctx.has_close_debt());
    let fourth_only_snap = fixture_ctx.child_native_facts_snapshot();
    assert_eq!(
        fourth_only_snap.pipe_spawn_close_debts,
        [None, None, None, Some(6)]
    );
    assert_eq!(fourth_only_snap.pipe_spawn_close_debt_count, 1);
    assert_eq!(fourth_only_snap.first_close_debt_os_error, Some(6));
    assert!(fourth_only_snap.has_any_handle_close_debt);
    assert!(fourth_only_snap.first_error.no_replay_latched);
    assert!(!fourth_only_snap.normal_close_completed);
    assert!(!fourth_only_snap.runner_native_exit_ready);

    // 2. Earlier pipe close-debt entry plus fourth entry: all four entries remain preserved
    // in the bounded snapshot while first_close_debt_os_error reports the first present debt.
    fixture_ctx.record_scalar_pipe_spawn_close_debts_for_test([None, Some(5), None, Some(19)]);
    let multi_snap = fixture_ctx.child_native_facts_snapshot();
    assert_eq!(
        multi_snap.pipe_spawn_close_debts,
        [None, Some(5), None, Some(19)]
    );
    assert_eq!(multi_snap.pipe_spawn_close_debt_count, 2);
    assert_eq!(multi_snap.first_close_debt_os_error, Some(5));
    assert!(multi_snap.has_any_handle_close_debt);
    assert!(multi_snap.first_error.no_replay_latched);
    assert!(!multi_snap.normal_close_completed);
    assert!(!multi_snap.runner_native_exit_ready);
}

#[test]
fn nonblocking_snapshot_reports_contended_busy_and_recovers_after_poison_without_blocking() {
    let _serial = lock_attempt_slot_test_serial();
    reset_attempt_slot_for_test();

    let (stdout, peer) = create_anonymous_pipe_pair().unwrap();
    drop(peer);
    let context = build_fixture_context(stdout, None, None).unwrap();
    let identity = context.process_owner.identity().unwrap();

    // 1. Contended process_owner slot mutex must return immediately with BusyNotObserved and NotObserved.
    context.process_owner.with_held_slot_lock_for_test(|| {
        let busy_snap = context.child_native_facts_snapshot();
        assert_eq!(
            busy_snap.owner.slot_lock,
            LockObservationState::BusyNotObserved
        );
        assert_eq!(busy_snap.owner.creation, ChildCreationFactState::NotObserved);
        assert_eq!(busy_snap.owner.exit, ChildExitFactState::NotObserved);
        assert!(!busy_snap.normal_close_completed);
        assert!(!busy_snap.runner_native_exit_ready);
    });

    // 2. Contended sticky_latch mutex must return immediately with BusyNotObserved and no_replay_latched.
    context.sticky_latch.with_held_lock_for_test(|| {
        let busy_latch_snap = context.child_native_facts_snapshot();
        assert_eq!(
            busy_latch_snap.first_error.latch_lock,
            LockObservationState::BusyNotObserved
        );
        assert!(busy_latch_snap.first_error.no_replay_latched);
        assert!(!busy_latch_snap.normal_close_completed);
    });

    // 3. Poisoned mutexes must recover the underlying state with PoisonedRecovered.
    context
        .sticky_latch
        .record_first(StickyUnknownKind::ChildWaitFailed, "initial sticky error");
    context.process_owner.poison_slot_for_test();
    context.conpty_resize.poison_for_test();
    context.sticky_latch.poison_for_test();

    let poisoned_snap = context.child_native_facts_snapshot();
    assert_eq!(
        poisoned_snap.owner.slot_lock,
        LockObservationState::PoisonedRecovered
    );
    assert_eq!(
        poisoned_snap.owner.creation,
        ChildCreationFactState::CreatedValidated(identity)
    );
    assert_eq!(
        poisoned_snap.control_and_writers.conpty_lock,
        LockObservationState::PoisonedRecovered
    );
    assert_eq!(
        poisoned_snap.first_error.latch_lock,
        LockObservationState::PoisonedRecovered
    );
    assert_eq!(
        poisoned_snap.first_error.first_error_kind,
        Some(StickyUnknownKind::ChildWaitFailed)
    );

    assert!(park_unresolved_attempt_context(context));
    poison_attempt_slot_for_test();
    let (slot_lock, slot_snap) = try_snapshot_attempt_slot_child_native_facts();
    assert_eq!(slot_lock, LockObservationState::PoisonedRecovered);
    assert!(slot_snap.is_some());
    drop(take_parked_attempt_context_for_test());
    reset_attempt_slot_for_test();
}

#[test]
fn late_normal_facts_and_public_ledger_booleans_cannot_clear_first_error_or_manufacture_native_success()
 {
    let _serial = lock_attempt_slot_test_serial();
    reset_attempt_slot_for_test();

    let native_lifetime = NativeFixtureProcessLifetimeGuard::new();
    // This test exercises the REAL reaper: use its own naturally exiting child,
    // never a duplicate of the still-running libtest process. Public ledger
    // values below deliberately remain synthetic and cannot substitute for it.
    let NativeReapFixture { mut context, stdin_release, stdout_write_held } =
        build_native_reap_fixture(&native_lifetime, false).expect("create own natural-exit fixture");
    let identity = context.process_owner.identity().unwrap();
    let original_job = Arc::clone(&context.job);
    let mut stdin_release = FixtureObservationPin::new(stdin_release, &native_lifetime);
    let mut stdout_write_held = FixtureObservationPin::new(stdout_write_held, &native_lifetime);
    let synthetic_witness = ChildExitWitness {
        identity,
        exit_code: 0,
        raw_exit_code: 0,
        timed_out: false,
    };

    // 1. Mutating public CheckedLifecycleLedger booleans directly without real native worker joins,
    // handle closes, or job drain must NEVER manufacture native success in ChildNativeFactsSnapshot.
    context.ledger = CheckedLifecycleLedger {
        child_exit: Some(synthetic_witness),
        stdout_eof_and_joined: true,
        stderr_eof_and_joined: true,
        business_exit_sent: true,
        input_peer_eof_and_joined: true,
        stdin_closed_once: true,
        child_handles_closed: true,
        job_active_zero: Some(JobActiveZeroWitness {
            total_processes: 1,
            active_processes: 0,
            total_terminated_processes: 0,
        }),
        runner_native_exit_ready: true,
    };
    let unverified_snap = context.child_native_facts_snapshot();
    assert_eq!(unverified_snap.owner.exit, ChildExitFactState::NotObserved);
    assert!(!unverified_snap.stdout.timely_eof_and_joined_verified);
    assert!(!unverified_snap.stderr.timely_eof_and_joined_verified);
    assert!(
        !unverified_snap
            .control_and_writers
            .business_exit_writer_joined_and_acked
    );
    assert!(!unverified_snap.input.timely_peer_eof_and_joined_verified);
    assert_eq!(
        unverified_snap.owner.process_handle_close,
        NativeHandleCloseFact::Open
    );
    assert_eq!(
        unverified_snap.primary_thread_handle_close,
        NativeHandleCloseFact::Open
    );
    assert_eq!(
        unverified_snap.job.active_zero_state,
        JobActiveZeroFactState::NotObserved
    );
    assert!(!unverified_snap.normal_close_completed);
    assert!(!unverified_snap.runner_native_exit_ready);

    // 2. Latch a first sticky error, then let physical workers and handles reap late.
    context.sticky_latch.record_first(
        StickyUnknownKind::StdoutJoinTimedOut,
        "first sticky timeout before late physical completion",
    );
    let (_host_out_read, runner_out_write) = create_anonymous_pipe_files().unwrap();
    let pipe_write = Arc::new(StdMutex::new(runner_out_write));
    // The actual original reader cannot finish while the fixture's stdout EOF
    // keeper is held. No sleep or synthetic successful worker result is used.
    let late_stdout = super::spawn_output_reader(
        pipe_write, context.stdout_owner.as_ref().unwrap().raw(),
        OutputStream::Stdout, None, CHECKED_IPC_PROTOCOL_VERSION,
    );
    context.stdout_worker = Some(RetainedWorker::new("late stdout", late_stdout));
    assert!(
        context
            .stdout_worker
            .as_mut()
            .unwrap()
            .join_bounded(Instant::now() + Duration::from_millis(5))
            .is_err()
    );
    context.stdin_owner.as_mut().unwrap().close_once().unwrap();
    stdin_release.close().expect("checked-close only this fixture's stdin EOF keeper");
    stdout_write_held.close().expect("checked-close only this fixture's stdout EOF keeper");
    let wait_pin = FixtureObservationPin::new(
        context.process_owner.duplicate_wait_handle().unwrap(), &native_lifetime,
    );
    assert_eq!(wait_pin.observe_exit_and_close(identity).unwrap(), 7,
        "actual natural child DWORD is7, distinct from the public synthetic zero");

    let (running, active) =
        context.reap_unresolved_originals_bounded(Instant::now() + Duration::from_secs(2));
    assert_eq!(running, 0);
    assert_eq!(active, 0);

    let mut reaped_context = FixtureReapedContextOwner::new(context);
    let reaped_snap = reaped_context.child_native_facts_snapshot();
    assert_eq!(reaped_snap.owner.exit, ChildExitFactState::LatePhysicalWaitSignaledExitCodeNotQueried,
        "real reaper wait is not the public ledger's fabricated native DWORD");
    // Physical facts update after late reap:
    assert_eq!(
        reaped_snap.stdout.join_state,
        Some(WorkerJoinState::JoinedLateAfterBudget)
    );
    assert!(reaped_snap.stdout.eof_or_completion_observed);
    assert_eq!(
        reaped_snap.owner.process_handle_close,
        NativeHandleCloseFact::ClosedVerified
    );
    assert_eq!(
        reaped_snap.primary_thread_handle_close,
        NativeHandleCloseFact::ClosedVerified
    );
    // Logical failure, sticky first error, and no_replay remain permanently latched:
    assert!(!reaped_snap.stdout.timely_eof_and_joined_verified);
    assert!(reaped_snap.late_reap_observed);
    assert_eq!(
        reaped_snap.first_error.first_error_kind,
        Some(StickyUnknownKind::StdoutJoinTimedOut)
    );
    assert!(reaped_snap.first_error.no_replay_latched);
    assert!(!reaped_snap.normal_close_completed);
    assert!(!reaped_snap.runner_native_exit_ready);
    assert!(!original_job.terminate_invoked());
    assert!(!original_job.descendants_preserved().unwrap());
    let (total, active, terminated, pids) = observe_stable_closed_fixture_job(
        &original_job, Instant::now() + Duration::from_secs(2),
    ).expect("actual stable closed fixture Job after original reader/native reaping");
    assert!(matches!(total, 1 | 2));
    assert_eq!(active, 0);assert_eq!(terminated, 0);assert!(pids.is_empty());
    reaped_context.accept_after_stable_closed_observation();
    reset_attempt_slot_for_test();
}




#[test]
fn v8_error_without_authenticated_runtime_never_uses_legacy_bytes_or_creates_a_writer() {
    let _serial=lock_attempt_slot_test_serial();reset_attempt_slot_for_test();
    let (mut reader,writer)=create_anonymous_pipe_files().unwrap();
    let writer=Arc::new(StdMutex::new(writer));
    assert!(super::send_error(None,&writer,codex_windows_sandbox::ErrorStage::ReadSpawnRequest,None,
        "private detail must not reach a bare frame".into(),8).is_err());
    assert!(super::send_native_precontext_error(&writer,None,codex_windows_sandbox::ErrorStage::ReadSpawnRequest,
        None,Instant::now()+Duration::from_secs(2)).is_err());
    assert!(!has_unresolved_attempt_slot_entries());
    drop(writer);let mut bytes=Vec::new();reader.read_to_end(&mut bytes).unwrap();
    assert!(bytes.is_empty(),"real anonymous pipe received no bare v8 header/payload");
}
#[test]
fn v8_error_is_authenticated_bounded_redacted_and_refuses_an_owned_busy_admission() {
    let _serial=lock_attempt_slot_test_serial();reset_attempt_slot_for_test();
    use codex_windows_sandbox::{NativeCloseRuntime,RuntimeRunnerIdentity,ControlHelloAck,RunnerControlCapabilities};
    let original=RuntimeRunnerIdentity {process_id:271,thread_id:272,creation_filetime_100ns:273};
    let (host,bootstrap)=NativeCloseRuntime::host([27;32],274,original).unwrap();
    let guest=NativeCloseRuntime::runner_bootstrap(bootstrap,original).unwrap();
    // Memory-only peer; same production codec, actual retained native writer.
    let bytes=Arc::new(StdMutex::new(Vec::<u8>::new()));let peer=Arc::clone(&guest);
    checked_lifecycle::send_native_precontext_frame_with(&bytes,Instant::now()+Duration::from_secs(2),move |writer| {
        peer.write_message(writer,&FramedMessage {version:8,message:Message::HelloAck {payload:ControlHelloAck {
            hello:peer.control_hello(),controls:RunnerControlCapabilities {close_stdin:true,resize:true}}}})
    }).unwrap();
    let unused_admission=reserve_output_reader_slot().unwrap();
    let before=bytes.lock().unwrap().len();
    assert!(super::send_native_precontext_error(&bytes,Some(&guest),codex_windows_sandbox::ErrorStage::ReadSpawnRequest,
        Some(5),Instant::now()+Duration::from_secs(2)).is_err());
    assert_eq!(bytes.lock().unwrap().len(),before);drop(unused_admission);
    super::send_native_precontext_error(&bytes,Some(&guest),codex_windows_sandbox::ErrorStage::ReadSpawnRequest,
        Some(5),Instant::now()+Duration::from_secs(2)).unwrap();
    let wire=bytes.lock().unwrap().clone();let mut reader=Cursor::new(wire);
    assert!(host.read_message(&mut reader).unwrap().unwrap().is_some());
    let Some(Some(error))=host.read_message(&mut reader).unwrap() else{panic!("authenticated error")};
    let Message::Error{payload}=error.message else{panic!("error frame")};
    assert_eq!(payload.windows_error_code,Some(5));
    assert_eq!(payload.message,"v8 operation rejected; original owner retained");
    assert!(!has_unresolved_attempt_slot_entries());
}
#[test]
fn failed_v8_precontext_writer_retains_same_memory_owner_after_actual_native_join() {
    let _serial=lock_attempt_slot_test_serial();reset_attempt_slot_for_test();
    struct Fails(Arc<std::sync::atomic::AtomicUsize>);
    impl Write for Fails {
        fn write(&mut self,_:&[u8])->io::Result<usize>{Err(io::Error::from_raw_os_error(5))}
        fn flush(&mut self)->io::Result<()>{Ok(())}
    }
    impl Drop for Fails {fn drop(&mut self){self.0.fetch_add(1,std::sync::atomic::Ordering::SeqCst);}}
    let drops=Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let original=Arc::new(StdMutex::new(Fails(Arc::clone(&drops))));
    let original_memory=Arc::downgrade(&original);
    assert_eq!(checked_lifecycle::send_native_precontext_frame_with(&original,Instant::now()+Duration::from_secs(2),
        |writer|writer.write_all(b"synthetic IO failure")).unwrap_err().raw_os_error(),Some(5));
    drop(original);assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst),0);
    assert!(has_unresolved_attempt_slot_entries());
    let disposition=reap_unresolved_attempt_slot_bounded(Instant::now()+Duration::from_secs(2));
    assert!(matches!(disposition,RunnerExitDisposition::MustHoldProcessAlive{running_workers:0,active_job_processes:0,..}));
    assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst),0);
    // This fixture has only a memory writer and a PROVEN joined native thread,
    // no File/HANDLE/child/Job. Clear only this serialized memory test fixture.
    checked_lifecycle::reset_joined_memory_precontext_writer_for_test(&original_memory);
    assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst),1);
}


#[test]
fn v8_public_ledger_cannot_authorize_a_receipt_without_original_operation_order() {
    let _serial=lock_attempt_slot_test_serial();reset_attempt_slot_for_test();
    use codex_windows_sandbox::{NativeCloseRuntime,RuntimeRunnerIdentity};
    let (stdout,peer)=create_anonymous_pipe_pair().unwrap();drop(peer);
    // Scalar negative fixture only: never wait/reap the current test process,
    // never inject its cached exit as a real child native completion witness.
    let mut context=build_fixture_context(stdout,None,None).unwrap();
    let identity=RuntimeRunnerIdentity {process_id:271,thread_id:272,creation_filetime_100ns:273};
    let (_,bootstrap)=NativeCloseRuntime::host([27;32],275,identity).unwrap();
    context.native_close_runtime=Some(NativeCloseRuntime::runner_bootstrap(bootstrap,identity).unwrap());
    context.ledger.stdout_eof_and_joined=true;context.ledger.stderr_eof_and_joined=true;
    context.ledger.business_exit_sent=true;context.ledger.input_peer_eof_and_joined=true;
    context.ledger.child_handles_closed=true;context.ledger.runner_native_exit_ready=true;
    let (mut read,write)=create_anonymous_pipe_files().unwrap();let write=Arc::new(StdMutex::new(write));
    assert!(context.send_native_close_receipt(&write,Instant::now()+Duration::from_secs(2),false).is_err());
    assert!(context.frame_write_worker.is_none());
    assert_eq!(context.native_receipt_writer,FinalReceiptWriterFactState::FailedOrRetained);
    drop(write);let mut bytes=Vec::new();read.read_to_end(&mut bytes).unwrap();assert!(bytes.is_empty());
}

#[test]
fn v8_actual_child_original_io_and_receipt_preserve_native7_without_claiming_runner_or_outer_exit() {
    let _serial=lock_attempt_slot_test_serial();reset_attempt_slot_for_test();
    let lifetime=NativeFixtureProcessLifetimeGuard::new();
    use codex_windows_sandbox::{NativeCloseRuntime,RuntimeRunnerIdentity,ControlHelloAck,RunnerControlCapabilities,SpawnReady};
    use codex_windows_sandbox::native_close_wire::{self as wire,ReceiverProtocolStage};
    use std::os::windows::io::IntoRawHandle;
    fn own_pipe()->io::Result<(File,File)> {
        let mut read=0;let mut write=0;
        if unsafe{CreatePipe(&mut read,&mut write,ptr::null(),65536)}==0 {return Err(io::Error::last_os_error());}
        Ok((unsafe{File::from_raw_handle(read as _)},unsafe{File::from_raw_handle(write as _)}))
    }
    let NativeReapFixture {mut context,stdin_release,stdout_write_held}=build_native_reap_fixture(&lifetime,false).unwrap();
    let mut stdin_release=FixtureObservationPin::new(stdin_release,&lifetime);
    let mut stdout_keeper=FixtureObservationPin::new(stdout_write_held,&lifetime);
    let child=context.process_owner.identity().unwrap();let job=Arc::clone(&context.job);
    // Simulated HOST, real original Windows child/pipes/IO/native joins/close.
    // This libtest process remains alive and is never supplied as runner exit.
    let runner_identity=RuntimeRunnerIdentity::capture_current_startup().unwrap();
    let (host,bootstrap)=NativeCloseRuntime::host([27;32],276,runner_identity).unwrap();
    let guest=NativeCloseRuntime::runner_bootstrap(bootstrap,runner_identity).unwrap();
    guest.set_child(wire::VerifiedChildProcessIdentity::try_new(child.process_id,child.thread_id,child.creation_time_100ns).unwrap()).unwrap();
    context.native_close_runtime=Some(Arc::clone(&guest));
    let (input_read,mut input_write)=own_pipe().unwrap();
    context.native_input_pipe=Some(Arc::new(input_read));
    let (mut output_read,output_write)=own_pipe().unwrap();let output_write=Arc::new(StdMutex::new(output_write));
    let deadline=Instant::now()+Duration::from_secs(5);
    context.send_frame_checked(&output_write,FramedMessage {version:8,message:Message::HelloAck {payload:ControlHelloAck {
        hello:guest.control_hello(),controls:RunnerControlCapabilities {close_stdin:true,resize:true}}}},deadline).unwrap();
    context.send_frame_checked(&output_write,FramedMessage {version:8,message:Message::SpawnReady {payload:SpawnReady {
        process_id:child.process_id,controls:RunnerControlCapabilities {close_stdin:true,resize:false}}}},deadline).unwrap();
    context.start_native_io_workers(Arc::clone(&output_write),guest.control_hello().nonce).unwrap();
    for _ in 0..2 {
        codex_windows_sandbox::wait_native_runtime_frame(&output_read,deadline).unwrap();
        assert!(host.read_message(&mut output_read).unwrap().unwrap().is_some());
    }
    stdin_release.close().unwrap();
    let request=ControlRequest::new(1,host.control_hello().nonce,ControlOperation::CloseStdin,0,0);
    host.write_message(&mut input_write,&FramedMessage {version:8,message:Message::ControlRequest {payload:request}}).unwrap();
    codex_windows_sandbox::wait_native_runtime_frame(&output_read,deadline).unwrap();
    let Some(Some(control))=host.read_message(&mut output_read).unwrap() else{panic!("original control result")};
    let Message::ControlResult{payload}=control.message else{panic!("control result frame")};
    assert_eq!(payload.status,ControlStatus::Applied);
    let wait=FixtureObservationPin::new(context.process_owner.duplicate_wait_handle().unwrap(),&lifetime);
    assert_eq!(wait.observe_exit_and_close(child).unwrap(),7);
    let exit=context.process_owner.record_verified_exit_from_original(WAIT_OBJECT_0,false).unwrap();
    assert_eq!(exit.raw_exit_code,7);context.ledger.child_exit=Some(exit);
    stdout_keeper.close().unwrap();context.conpty_resize.close_conpty().unwrap();
    context.join_output_workers_bounded(deadline).unwrap();
    context.send_business_exit_checked(&output_write,deadline).unwrap();
    loop {
        codex_windows_sandbox::wait_native_runtime_frame(&output_read,deadline).unwrap();
        let Some(Some(frame))=host.read_message(&mut output_read).unwrap() else{panic!("business output or exit")};
        match frame.message {Message::Exit{payload}=>{assert_eq!(payload.exit_code,7);break;},Message::Output{..}=>{},_=>panic!("unexpected real IO frame")}
    }
    // Simulated host sends the actual authenticated terminal handshake on the
    // SAME original input endpoint, before its real checked CloseHandle/EOF.
    host.write_terminal_close_ack(&mut input_write).unwrap();
    let mut input_close=FixtureObservationPin::new(OwnedWinHandle::new(input_write.into_raw_handle() as HANDLE),&lifetime);
    input_close.close().unwrap();
    context.join_input_worker_bounded(deadline).unwrap();
    assert!(guest.terminal_ack_received());assert!(guest.physical_eof_seen());
    context.close_child_handles_checked().unwrap();let drain=context.drain_job_active_zero_bounded(deadline).unwrap();
    context.send_native_close_receipt(&output_write,deadline,false).unwrap();
    assert_eq!(context.native_receipt_writer,FinalReceiptWriterFactState::JoinedTimelyAfterFlush);
    codex_windows_sandbox::wait_native_runtime_frame(&output_read,deadline).unwrap();
    let receipt=host.read_message(&mut output_read);
    // Preserve the strict origin gate. Actual extra Job members are unproven,
    // even though this same original Job is physically active0 and never killed.
    if drain.total_processes==1 && drain.total_terminated_processes==0 {
        assert!(matches!(receipt,Ok(Some(None))));
        assert_eq!(host.public_projection().unwrap().validated_receipt.unwrap().raw_child_exit_dword(),wire::RawProcessExitCodeDword(7));
    }else{
        assert!(receipt.is_err());assert_eq!(host.public_projection().unwrap().stage,ReceiverProtocolStage::LatchedNotAcceptedStickyUnknown);
    }
    context.mark_clean_completion_for_drop();drop(context);drop(output_write);
    // Read the SAME original pipe to its actual EOF. A protocol rejection never
    // becomes a runner/outer completion capability or a successful observation.
    assert!(wire::read_v8_wire_frame_bounded(&mut output_read).unwrap().is_none());
    assert!(!host.public_projection().unwrap().local_registry_slot_releasable);
    let (total,active,terminated,pids)=observe_stable_closed_fixture_job(&job,deadline).unwrap();
    assert!(matches!(total,1|2));assert_eq!(active,0);assert_eq!(terminated,0);assert!(pids.is_empty());
    assert!(!job.terminate_invoked());assert!(!job.descendants_preserved().unwrap());
    reset_attempt_slot_for_test();
}
