use super::*;
use crate::ipc_framed::{CHECKED_CONTROL_SCHEMA, ControlHello, RunnerControlCapabilities};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentThread};

fn dummy_artifact() -> Arc<MatchedRunnerArtifact> {
    let exe = std::env::current_exe().expect("current_exe");
    let sha: [u8; 32] = Sha256::digest(std::fs::read(&exe).expect("read current_exe")).into();
    Arc::new(MatchedRunnerArtifact::acquire(&exe, sha).expect("acquire MatchedRunnerArtifact"))
}

fn dummy_hello() -> ControlHello {
    ControlHello {
        nonce: [0x28; 32],
        schema: CHECKED_CONTROL_SCHEMA,
    }
}

fn duplicate_current_process_handle() -> (OwnedHandle, u32) {
    let mut raw: HANDLE = 0;
    let current = unsafe { GetCurrentProcess() };
    let ok = unsafe {
        DuplicateHandle(
            current,
            current,
            current,
            &mut raw,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    assert_ne!(ok, 0, "DuplicateHandle(GetCurrentProcess) must succeed");
    let owned = unsafe { OwnedHandle::from_raw_handle(raw as _) };
    (owned, std::process::id())
}

fn duplicate_current_thread_handle() -> (OwnedHandle, u32) {
    let mut raw: HANDLE = 0;
    let current_proc = unsafe { GetCurrentProcess() };
    let current_thread = unsafe { GetCurrentThread() };
    let ok = unsafe {
        DuplicateHandle(
            current_proc,
            current_thread,
            current_proc,
            &mut raw,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    assert_ne!(ok, 0, "DuplicateHandle(GetCurrentThread) must succeed");
    let owned = unsafe { OwnedHandle::from_raw_handle(raw as _) };
    (owned, 1)
}

fn spawn_exited_child_process() -> (OwnedHandle, u32) {
    let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned());
    let mut child = Command::new(comspec)
        .args(["/C", "exit", "0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn short-lived cmd.exe child");
    let pid = child.id();
    let _ = child.wait().expect("wait for short-lived child");
    let handle: OwnedHandle = child.into();
    (handle, pid)
}

fn spawn_controllable_child_process() -> (OwnedHandle, u32, std::process::ChildStdin) {
    let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned());
    let mut child = Command::new(comspec)
        .args(["/C", "set /p x= & exit /b 0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn controllable cmd.exe child");
    let pid = child.id();
    let stdin = child.stdin.take().expect("piped stdin");
    let handle: OwnedHandle = child.into();
    (handle, pid, stdin)
}

fn temp_pipe_stub_pair() -> (File, File) {
    (
        tempfile::tempfile().expect("open temp file a"),
        tempfile::tempfile().expect("open temp file b"),
    )
}

// Ordinary owned process/pipe fixtures only; no sandbox logon, desktop, v8
// bootstrap/ACK, business process or cleanup capability is manufactured here.
fn native_start_test_server(name: &str) -> File {
    let wide = crate::winutil::to_wide(name);
    let raw = unsafe { windows_sys::Win32::System::Pipes::CreateNamedPipeW(
        wide.as_ptr(), crate::runner_pipe::PIPE_ACCESS_INBOUND,
        windows_sys::Win32::System::Pipes::PIPE_TYPE_BYTE
            | windows_sys::Win32::System::Pipes::PIPE_READMODE_BYTE
            | windows_sys::Win32::System::Pipes::PIPE_WAIT,
        1, 4096, 4096, 0, std::ptr::null_mut()) };
    assert!(raw != 0 && raw != INVALID_HANDLE_VALUE);
    unsafe { File::from_raw_handle(raw as _) }
}

#[test]
fn v8_original_runner_exit_wakes_connect_and_retains_owner_and_real_helper_until_explicit_reap() {
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();
    let diagnostic = crate::WindowsStartDiagnostic::default();
    diagnostic.mark(crate::WindowsStartStage::PipeConnectIn);
    let (name, _) = crate::runner_pipe::pipe_pair();
    let pipe = native_start_test_server(&name);
    let unused_other_pipe = tempfile::tempfile().unwrap();
    let mut reservation = reserve_in_registry(&REGISTRY, Some(&CONNECT_OWNER),
        dummy_artifact().native_close_view(), Some(diagnostic.clone()), Duration::from_secs(5)).unwrap();
    let (process, pid, mut stdin) = spawn_controllable_child_process();
    let (thread, tid) = duplicate_current_thread_handle();
    reservation.install_launched_process(process, pid, thread, tid,
        &pipe, &unused_other_pipe, dummy_hello()).unwrap();
    let owner = Arc::clone(reservation.owner());
    owner.observe_startup_runner(&diagnostic);
    let live = diagnostic.snapshot().unwrap().current.original_runner.unwrap();
    assert_eq!(live.startup_id, owner.startup_id().0);
    assert_eq!(live.runner_pid, pid);
    assert_ne!(live.runner_creation_filetime_100ns, 0);
    assert_eq!(live.state, crate::WindowsStartRunnerState::RunningAtObservation);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let controller = std::thread::spawn(move || {
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // Natural exit of this fixture's own process, not termination/cleanup.
        stdin.write_all(b"done\r\n").unwrap();
        drop(stdin);
    });
    let peer_pid = std::process::id();
    let err = reservation.connect_pipe_step_with(&pipe, "pipe-in", Some(&diagnostic), move |owned| {
        entered_tx.send(()).map_err(io::Error::other)?;
        release_rx.recv().map_err(io::Error::other)?;
        // Actual named-pipe/PID verification after an explicit fixture release.
        // This is an ordinary test peer, not authenticated runner acceptance.
        crate::runner_pipe::connect_pipe(owned.as_raw_handle() as HANDLE, peer_pid)
    }).expect_err("original runner exit must not await the whole connect deadline");
    controller.join().unwrap();
    assert!(is_error_marked_non_refreshable(&err));
    assert!(owner.is_no_replay_sealed() && owner.is_sticky_unknown());
    assert!(owner.has_retained_process_handle());
    assert_eq!(REGISTRY.active_count().unwrap(), 1);
    let snapshot = diagnostic.snapshot().unwrap();
    assert_eq!(snapshot.current.stage, crate::WindowsStartStage::PipeConnectIn);
    assert_eq!(snapshot.current.error, crate::WindowsStartError::OriginalRunnerExitedBeforeConnect);
    let exited = snapshot.current.original_runner.unwrap();
    assert_eq!(exited.runner_pid, live.runner_pid);
    assert_eq!(exited.runner_creation_filetime_100ns, live.runner_creation_filetime_100ns);
    assert_eq!(exited.state, crate::WindowsStartRunnerState::Exited { raw_dword: 0 });
    let attempt = {
        let state = owner.state.lock().unwrap();
        // A diagnostic query must not install a native/finalization receipt.
        assert!(state.native_process_exit_code.is_none());
        state.helper_attempts[0].as_ref().unwrap().attempt_id
    };
    assert_eq!(CONNECT_OWNER.acknowledge_joined(attempt).unwrap_err().kind(), io::ErrorKind::WouldBlock);
    assert!(reserve_in_registry(&REGISTRY, Some(&CONNECT_OWNER), dummy_artifact(), None,
        Duration::from_secs(5)).is_err());
    drop(reservation);
    // No inferred join: release the SAME original worker through a real pipe.
    let _client = std::fs::OpenOptions::new().write(true).open(&name).unwrap();
    release_tx.send(()).unwrap();
    owner.finalize(Instant::now() + Duration::from_secs(5)).unwrap();
    assert_eq!(REGISTRY.active_count().unwrap(), 0);
    assert!(owner.is_no_replay_sealed() && owner.is_sticky_unknown());
    assert!(owner.native_close_runtime.get().is_none());
    assert_eq!(diagnostic.snapshot().unwrap().current.original_runner, Some(exited));
}

#[test]
fn v8_connect_watch_joins_real_pipe_helper_and_rejects_a_different_startup() {
    let connect_owner = owned_connect_worker::Owner::new();
    let startup = StartupId(351);
    connect_owner.reserve_for_startup(startup).unwrap();
    let (process, pid) = duplicate_current_process_handle();
    let identity = OriginalProcessIdentity::from_live_process(&process, pid).unwrap();
    let diagnostic = crate::WindowsStartDiagnostic::default();
    diagnostic.mark(crate::WindowsStartStage::PipeConnectOut);
    let (name, _) = crate::runner_pipe::pipe_pair();
    let pipe = native_start_test_server(&name);
    let _client = std::fs::OpenOptions::new().write(true).open(&name).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let attempt = connect_owner.begin_with_for_startup(Some(startup), &pipe, &process, deadline,
        move |owned| crate::runner_pipe::connect_pipe(owned.as_raw_handle() as HANDLE, pid)).unwrap();
    assert_eq!(connect_owner.observe_detailed_with_runner(attempt, deadline, &process, identity,
        StartupId(startup.0 + 1), Some(&diagnostic)).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    let (foreign_process, foreign_pid) = spawn_exited_child_process();
    let foreign_identity = OriginalProcessIdentity::from_live_process(&foreign_process, foreign_pid).unwrap();
    assert_eq!(connect_owner.observe_detailed_with_runner(attempt, deadline, &foreign_process,
        foreign_identity, startup, Some(&diagnostic)).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    let (observed, meta) = connect_owner.observe_detailed_with_runner(attempt, deadline,
        &process, identity, startup, Some(&diagnostic)).unwrap();
    assert!(matches!(observed, Observation::Joined { result: Ok(()), within_deadline: true, prior_unknown: None }));
    assert!(meta.is_none());
    let original = diagnostic.snapshot().unwrap().current.original_runner.unwrap();
    assert_eq!(original.startup_id, startup.0);
    assert_eq!(original.runner_pid, pid);
    assert_eq!(original.state, crate::WindowsStartRunnerState::RunningAtObservation);
    connect_owner.acknowledge_joined(attempt).unwrap();
    connect_owner.release_startup_reservation(startup).unwrap();
}

#[test]
fn original_startup_process_probe_keeps_full_dword_and_signaled_exit259_is_not_running() {
    for code in [259i32, i32::MIN + 7] {
        let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned());
        let mut child = Command::new(comspec).args(["/D", "/C", "exit", "/b", &code.to_string()])
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        let pid = child.id();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(code));
        let original: OwnedHandle = child.into();
        let identity = OriginalProcessIdentity::from_live_process(&original, pid).unwrap();
        let observed = identity.observe_startup_process(&original, StartupId(352));
        assert_eq!(observed.runner_pid, pid);
        assert_eq!(observed.state, crate::WindowsStartRunnerState::Exited { raw_dword: code as u32 });
    }
}

#[test]
fn capacity_rejection_before_launch_leaves_zero_process_side_effects() {
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();

    let (pipe_w, pipe_r) = temp_pipe_stub_pair();
    let mut retained_owners = Vec::with_capacity(LOCAL_CHECKED_OWNER_CAPACITY);

    for _ in 0..LOCAL_CHECKED_OWNER_CAPACITY {
        let mut res = reserve_in_registry(
            &REGISTRY,
            Some(&CONNECT_OWNER),
            dummy_artifact(),
            None,
            Duration::from_secs(5),
        )
        .expect("reserve slot within 16-slot budget");
        let (proc_handle, pid) = spawn_exited_child_process();
        let (thread_handle, tid) = duplicate_current_thread_handle();
        res.install_launched_process(
            proc_handle,
            pid,
            thread_handle,
            tid,
            &pipe_w,
            &pipe_r,
            dummy_hello(),
        )
        .expect("install launched process");
        res.connect_pipe_step_with(&pipe_w, "pipe-in", None, |_| Ok(()))
            .expect("connect pipe-in");
        res.connect_pipe_step_with(&pipe_r, "pipe-out", None, |_| Ok(()))
            .expect("connect pipe-out");
        let lease = res
            .into_transport_lease(RunnerControlCapabilities::default())
            .expect("into transport lease");
        let owner = Arc::clone(lease.owner());
        // Drop the transport lease without adopting into CheckedRunnerIo so the owner
        // remains retained in the registry while the connect lane is free.
        drop(lease);
        assert!(owner.is_no_replay_sealed());
        retained_owners.push(owner);
    }

    assert_eq!(REGISTRY.active_count().expect("active count"), LOCAL_CHECKED_OWNER_CAPACITY);

    let launch_invocations = AtomicUsize::new(0);
    let over_budget = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    );
    if over_budget.is_ok() {
        launch_invocations.fetch_add(1, Ordering::SeqCst);
    }
    let err = over_budget.err().expect("17th reservation must be rejected before launch");
    assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(
        launch_invocations.load(Ordering::SeqCst),
        0,
        "launch callback must never execute when startup owner capacity is exhausted"
    );

    // Physically reap all 16 retained owners and verify their slots free up while noReplay stays sticky.
    for owner in &retained_owners {
        let outcome = owner
            .finalize(Instant::now() + Duration::from_secs(1))
            .expect("finalize retained owner");
        assert!(matches!(
            outcome,
            CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
                no_replay_sealed: true,
                release_eligible: false,
                ..
            }
        ));
        assert!(owner.is_no_replay_sealed());
        assert!(owner.is_sticky_unknown());
    }
    assert_eq!(REGISTRY.active_count().expect("active count after reap"), 0);
}

#[test]
fn first_or_second_pipe_connect_failure_retains_owner_seals_no_replay_and_marks_non_refreshable() {
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();

    let (pipe_w, pipe_r) = temp_pipe_stub_pair();

    // Case 1: First pipe (`pipe-in`) connect fails.
    let mut res1 = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup 1");
    let (proc1, pid1) = spawn_exited_child_process();
    let (thread1, tid1) = duplicate_current_thread_handle();
    res1.install_launched_process(
        proc1,
        pid1,
        thread1,
        tid1,
        &pipe_w,
        &pipe_r,
        dummy_hello(),
    )
    .expect("install process 1");
    let owner1 = Arc::clone(res1.owner());
    let err1 = res1
        .connect_pipe_step_with(&pipe_w, "pipe-in", None, |_| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "pipe-in client pid mismatch",
            ))
        })
        .expect_err("pipe-in failure must return Err");
    assert!(is_error_marked_non_refreshable(&err1));
    assert!(owner1.is_no_replay_sealed());
    assert!(owner1.is_sticky_unknown());
    assert!(owner1.has_retained_process_handle());
    drop(res1);
    assert_eq!(REGISTRY.active_count().expect("owner1 retained"), 1);

    // Case 2: First pipe (`pipe-in`) succeeds, second pipe (`pipe-out`) fails.
    let mut res2 = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup 2");
    let (proc2, pid2) = spawn_exited_child_process();
    let (thread2, tid2) = duplicate_current_thread_handle();
    res2.install_launched_process(
        proc2,
        pid2,
        thread2,
        tid2,
        &pipe_w,
        &pipe_r,
        dummy_hello(),
    )
    .expect("install process 2");
    let owner2 = Arc::clone(res2.owner());
    res2.connect_pipe_step_with(&pipe_w, "pipe-in", None, |_| Ok(()))
        .expect("pipe-in succeeds");
    let err2 = res2
        .connect_pipe_step_with(&pipe_r, "pipe-out", None, |_| {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "pipe-out broken before connect",
            ))
        })
        .expect_err("pipe-out failure must return Err");
    assert!(is_error_marked_non_refreshable(&err2));
    assert!(owner2.is_no_replay_sealed());
    assert!(owner2.is_sticky_unknown());
    assert!(owner2.has_retained_process_handle());
    drop(res2);
    assert_eq!(REGISTRY.active_count().expect("both owners retained"), 2);

    // Reap both retained owners.
    owner1
        .finalize(Instant::now() + Duration::from_secs(1))
        .expect("reap owner 1");
    owner2
        .finalize(Instant::now() + Duration::from_secs(1))
        .expect("reap owner 2");
    assert_eq!(REGISTRY.active_count().expect("both reaped"), 0);
}

#[test]
fn late_connect_success_after_timeout_reaps_helper_debt_without_clearing_no_replay() {
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();

    let (pipe_w, pipe_r) = temp_pipe_stub_pair();
    let mut res = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_millis(40),
    )
    .expect("reserve startup");
    let (proc_handle, pid, mut child_stdin) = spawn_controllable_child_process();
    let (thread_handle, tid) = duplicate_current_thread_handle();
    res.install_launched_process(
        proc_handle,
        pid,
        thread_handle,
        tid,
        &pipe_w,
        &pipe_r,
        dummy_hello(),
    )
    .expect("install controllable process");
    let owner = Arc::clone(res.owner());

    let (release_tx, release_rx) = mpsc::channel();
    let err = res
        .connect_pipe_step_with(&pipe_w, "pipe-in", None, move |_| {
            let _ = release_rx.recv();
            Ok(())
        })
        .expect_err("connect exceeding startup deadline must time out");
    assert!(is_error_marked_non_refreshable(&err));
    assert!(owner.is_no_replay_sealed());
    assert!(matches!(
        owner.sticky_unknown_reason(),
        Some(StickyUnknownReason::ConnectRetainedUnknown { .. })
    ));
    drop(res);

    // While helper thread is still blocked, a new startup reservation on the same connect lane is rejected.
    let blocked_second = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(1),
    );
    assert!(blocked_second.is_err());

    // Finalize while helper is still running must block with WouldBlock.
    let premature = owner
        .finalize(Instant::now() + Duration::from_millis(20))
        .expect_err("finalize must block while helper is still running");
    assert_eq!(premature.kind(), io::ErrorKind::WouldBlock);

    // Let the helper finish late with Ok(()).
    release_tx.send(()).expect("release helper");

    // Even after the helper thread finishes with Ok(()), the startup owner cannot advance or clear noReplay.
    assert!(owner.ensure_can_advance().is_err());
    assert!(owner.is_no_replay_sealed());

    // Finalize still blocks until the original runner process actually exits.
    let still_running = owner
        .finalize(Instant::now() + Duration::from_millis(30))
        .expect_err("finalize must block while original runner process is still alive");
    assert_eq!(still_running.kind(), io::ErrorKind::WouldBlock);
    assert!(owner.has_retained_process_handle());

    // Allow the original child process to exit cleanly.
    let _ = child_stdin.write_all(b"\r\n");
    drop(child_stdin);

    let outcome = owner
        .finalize(Instant::now() + Duration::from_secs(2))
        .expect("finalize after late helper join and native process exit");
    assert!(matches!(
        outcome,
        CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
            no_replay_sealed: true,
            release_eligible: false,
            ..
        }
    ));
    assert!(
        owner.is_no_replay_sealed(),
        "noReplay must remain permanently sticky after physical reap"
    );
    assert!(
        owner.is_sticky_unknown(),
        "sticky Unknown must remain permanently set after physical reap"
    );
    assert!(!owner.has_retained_process_handle());
    assert_eq!(REGISTRY.active_count().expect("slot released after reap"), 0);
}

#[test]
fn dropped_transport_wrong_legacy_conversion_cancelled_caller_guard_and_dropped_io_retain_owner() {
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();

    let (pipe_w, pipe_r) = temp_pipe_stub_pair();

    let make_lease = || {
        let mut res = reserve_in_registry(
            &REGISTRY,
            Some(&CONNECT_OWNER),
            dummy_artifact(),
            None,
            Duration::from_secs(5),
        )
        .expect("reserve startup");
        let caller_guard = res.caller_guard();
        let (proc_handle, pid) = spawn_exited_child_process();
        let (thread_handle, tid) = duplicate_current_thread_handle();
        res.install_launched_process(
            proc_handle,
            pid,
            thread_handle,
            tid,
            &pipe_w,
            &pipe_r,
            dummy_hello(),
        )
        .expect("install process");
        res.connect_pipe_step_with(&pipe_w, "pipe-in", None, |_| Ok(()))
            .expect("connect pipe-in");
        res.connect_pipe_step_with(&pipe_r, "pipe-out", None, |_| Ok(()))
            .expect("connect pipe-out");
        let lease = res
            .into_transport_lease(RunnerControlCapabilities::default())
            .expect("into transport lease");
        (caller_guard, lease)
    };

    // 1. Cancelled spawn_blocking caller guard seals CallerAwaitCancelled and blocks handoff.
    let (caller_guard1, lease1) = make_lease();
    let owner1 = Arc::clone(lease1.owner());
    drop(caller_guard1);
    assert_eq!(
        owner1.sticky_unknown_reason(),
        Some(StickyUnknownReason::CallerAwaitCancelled)
    );
    assert!(lease1.into_io_token().is_err());

    // 2. Wrong legacy conversion (`into_files`) seals WrongLegacyConversion and retains owner.
    let (mut caller_guard2, lease2) = make_lease();
    caller_guard2.acknowledged = true;
    let owner2 = Arc::clone(lease2.owner());
    lease2.reject_legacy_conversion();
    assert_eq!(
        owner2.sticky_unknown_reason(),
        Some(StickyUnknownReason::WrongLegacyConversion)
    );
    drop(lease2);

    // 3. Dropped CheckedTransportLease (`RunnerTransport`) seals TransportDroppedUnadopted.
    let (mut caller_guard3, lease3) = make_lease();
    caller_guard3.acknowledged = true;
    let owner3 = Arc::clone(lease3.owner());
    drop(lease3);
    assert_eq!(
        owner3.sticky_unknown_reason(),
        Some(StickyUnknownReason::TransportDroppedUnadopted)
    );

    // 4. Dropped CheckedIoOwnerToken (`CheckedRunnerIo`) before actor adoption seals CheckedIoDroppedUnadopted.
    let (mut caller_guard4, lease4) = make_lease();
    caller_guard4.acknowledged = true;
    let owner4 = Arc::clone(lease4.owner());
    let (io_token, _identity, _thread_disp) = lease4.into_io_token().expect("into io token");
    drop(io_token);
    assert_eq!(
        owner4.sticky_unknown_reason(),
        Some(StickyUnknownReason::CheckedIoDroppedUnadopted)
    );

    assert_eq!(
        REGISTRY.active_count().expect("all 4 owners retained"),
        4
    );

    for owner in [owner1, owner2, owner3, owner4] {
        let outcome = owner
            .finalize(Instant::now() + Duration::from_secs(1))
            .expect("reap retained owner");
        assert!(matches!(
            outcome,
            CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
                no_replay_sealed: true,
                release_eligible: false,
                ..
            }
        ));
        assert!(owner.is_no_replay_sealed());
    }
    assert_eq!(REGISTRY.active_count().expect("all 4 reaped"), 0);
}

#[test]
fn wrong_original_process_identity_blocks_install_handoff_and_finalize() {
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();

    let (pipe_w, pipe_r) = temp_pipe_stub_pair();

    // 1. Mismatched expected PID at install time rejects and seals IdentityValidationFailed.
    let mut res_bad_pid = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup");
    let (proc_bad, actual_pid) = spawn_exited_child_process();
    let (thread_bad, tid_bad) = duplicate_current_thread_handle();
    let owner_bad_pid = Arc::clone(res_bad_pid.owner());
    let err = res_bad_pid
        .install_launched_process(
            proc_bad,
            actual_pid.wrapping_add(1),
            thread_bad,
            tid_bad,
            &pipe_w,
            &pipe_r,
            dummy_hello(),
        )
        .expect_err("mismatched expected PID must fail");
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    assert!(
        owner_bad_pid.has_retained_process_handle(),
        "launched process handle must be retained even when identity PID check fails"
    );
    assert!(owner_bad_pid.is_no_replay_sealed());
    assert!(matches!(
        owner_bad_pid.sticky_unknown_reason(),
        Some(StickyUnknownReason::IdentityValidationFailed(_))
    ));
    drop(res_bad_pid);

    // 2. Replaced process handle after valid install is caught by verify_same_process during handoff/finalize.
    let mut res_tampered = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup 2");
    let (proc_orig, pid_orig) = spawn_exited_child_process();
    let (thread_orig, tid_orig) = duplicate_current_thread_handle();
    res_tampered
        .install_launched_process(
            proc_orig,
            pid_orig,
            thread_orig,
            tid_orig,
            &pipe_w,
            &pipe_r,
            dummy_hello(),
        )
        .expect("install valid process");
    res_tampered
        .connect_pipe_step_with(&pipe_w, "pipe-in", None, |_| Ok(()))
        .expect("connect pipe-in");
    res_tampered
        .connect_pipe_step_with(&pipe_r, "pipe-out", None, |_| Ok(()))
        .expect("connect pipe-out");
    let lease = res_tampered
        .into_transport_lease(RunnerControlCapabilities::default())
        .expect("into transport lease");
    let owner_tampered = Arc::clone(lease.owner());
    let (io_token, identity, thread_disposition) = lease.into_io_token().expect("into io token");
    let checked_io = CheckedRunnerIo {
        pipe_write: pipe_w,
        pipe_read: pipe_r,
        hello: dummy_hello(),
        capabilities: RunnerControlCapabilities::default(),
        artifact: dummy_artifact(),
        identity,
        thread_disposition,
        owner: io_token,
    };

    // Swap in a different process handle (current process) so PID/creation_filetime no longer match.
    let (wrong_proc, _) = duplicate_current_process_handle();
    owner_tampered.replace_process_handle_for_test(wrong_proc);

    let verify_err = checked_io
        .verify_original_process()
        .expect_err("verify_original_process must reject mismatched process handle");
    assert_eq!(verify_err.kind(), io::ErrorKind::PermissionDenied);

    let adopt_err = checked_io
        .owner
        .adopt_into_actor()
        .expect_err("adopt_into_actor must reject mismatched process handle");
    assert_eq!(adopt_err.kind(), io::ErrorKind::PermissionDenied);
    assert!(owner_tampered.is_no_replay_sealed());

    let finalize_err = owner_tampered
        .finalize(Instant::now() + Duration::from_millis(50))
        .expect_err("finalize must reject mismatched process handle");
    assert_eq!(finalize_err.kind(), io::ErrorKind::PermissionDenied);

    // Restore a matching process handle or clean up test registry state.
    let (clean_proc, clean_pid) = spawn_exited_child_process();
    let clean_identity =
        OriginalProcessIdentity::from_live_process(&clean_proc, clean_pid).expect("clean identity");
    if let Ok(mut state) = owner_bad_pid.state.lock() {
        state.identity = Some(clean_identity);
        state.process = Some(Arc::new(clean_proc.try_clone().expect("clone")));
    }
    if let Ok(mut state) = owner_tampered.state.lock() {
        state.identity = Some(clean_identity);
        state.process = Some(Arc::new(clean_proc));
    }
    owner_bad_pid
        .finalize(Instant::now() + Duration::from_secs(1))
        .expect("reap bad pid owner");
    owner_tampered
        .finalize(Instant::now() + Duration::from_secs(1))
        .expect("reap tampered owner");
    assert_eq!(REGISTRY.active_count().expect("all reaped"), 0);
}

#[test]
fn cancellation_or_deadline_immediately_before_process_capture_retains_launched_process_and_context()
{
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();

    let (pipe_w, pipe_r) = temp_pipe_stub_pair();

    // Case 1: Caller cancellation (`drop(caller_guard)`) wins immediately before `install_launched_process`.
    let mut res_cancel = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup for cancellation race");
    let caller_guard = res_cancel.caller_guard();
    res_cancel
        .stage_pre_launch_context(&pipe_w, &pipe_r, dummy_hello())
        .expect("pre-stage pipes and hello before launch");
    let (proc_cancel, pid_cancel) = spawn_exited_child_process();
    let (thread_cancel, tid_cancel) = duplicate_current_thread_handle();
    let owner_cancel = Arc::clone(res_cancel.owner());

    // Cancellation happens after OS process launch but before `install_launched_process` runs.
    drop(caller_guard);
    assert!(owner_cancel.is_no_replay_sealed());
    assert_eq!(
        owner_cancel.sticky_unknown_reason(),
        Some(StickyUnknownReason::CallerAwaitCancelled)
    );

    let err_cancel = res_cancel
        .install_launched_process(
            proc_cancel,
            pid_cancel,
            thread_cancel,
            tid_cancel,
            &pipe_w,
            &pipe_r,
            dummy_hello(),
        )
        .expect_err("install_launched_process must fail after caller cancellation");
    assert_eq!(err_cancel.kind(), io::ErrorKind::PermissionDenied);
    assert!(
        owner_cancel.has_retained_process_handle(),
        "original launched process handle must be retained despite pre-capture cancellation"
    );
    assert_eq!(owner_cancel.identity().map(|id| id.pid), Some(pid_cancel));
    assert_eq!(
        owner_cancel.thread_disposition(),
        Some(OriginalThreadDisposition::ClosedOnCapture {
            thread_id: tid_cancel,
        })
    );
    assert!(owner_cancel.has_retained_startup_pipes());
    assert!(owner_cancel.has_retained_hello());
    drop(res_cancel);

    // Case 2: Startup deadline expires immediately before `install_launched_process`.
    let mut res_expired = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::ZERO,
    )
    .expect("reserve startup with zero deadline");
    let (proc_exp, pid_exp) = spawn_exited_child_process();
    let (thread_exp, tid_exp) = duplicate_current_thread_handle();
    let owner_exp = Arc::clone(res_expired.owner());
    let err_exp = res_expired
        .install_launched_process(
            proc_exp,
            pid_exp,
            thread_exp,
            tid_exp,
            &pipe_w,
            &pipe_r,
            dummy_hello(),
        )
        .expect_err("install_launched_process must fail when startup deadline has expired");
    assert_eq!(err_exp.kind(), io::ErrorKind::TimedOut);
    assert!(
        owner_exp.has_retained_process_handle(),
        "original launched process handle must be retained despite expired startup deadline"
    );
    assert_eq!(owner_exp.identity().map(|id| id.pid), Some(pid_exp));
    assert_eq!(
        owner_exp.thread_disposition(),
        Some(OriginalThreadDisposition::ClosedOnCapture { thread_id: tid_exp })
    );
    assert!(owner_exp.has_retained_startup_pipes());
    assert!(owner_exp.has_retained_hello());
    drop(res_expired);

    assert_eq!(
        REGISTRY
            .active_count()
            .expect("both owners retained in registry"),
        2
    );

    for owner in [owner_cancel, owner_exp] {
        let outcome = owner
            .finalize(Instant::now() + Duration::from_secs(1))
            .expect("reap retained pre-capture owner");
        assert!(matches!(
            outcome,
            CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
                no_replay_sealed: true,
                release_eligible: false,
                ..
            }
        ));
        assert!(!owner.has_retained_process_handle());
        assert!(owner.is_no_replay_sealed());
        assert!(owner.is_sticky_unknown());
    }
    assert_eq!(REGISTRY.active_count().expect("all reaped"), 0);
}

#[test]
fn pipe_clone_failure_after_launch_retains_launched_process_and_preserves_primary_error() {
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();

    let (pipe_w, pipe_r) = temp_pipe_stub_pair();

    // Case 1: First pipe clone fails during post-launch fallback cloning.
    let mut res1 = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup 1");
    let (proc1, pid1) = spawn_exited_child_process();
    let (thread1, tid1) = duplicate_current_thread_handle();
    let owner1 = Arc::clone(res1.owner());
    let err1 = res1
        .install_launched_process_with_cloner(
            proc1,
            pid1,
            thread1,
            tid1,
            &pipe_w,
            &pipe_r,
            dummy_hello(),
            |_| {
                Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "simulated first pipe clone failure",
                ))
            },
        )
        .expect_err("first pipe clone failure must fail install_launched_process");
    assert_eq!(err1.kind(), io::ErrorKind::OutOfMemory);
    assert!(owner1.has_retained_process_handle());
    assert_eq!(owner1.identity().map(|id| id.pid), Some(pid1));
    assert_eq!(
        owner1.thread_disposition(),
        Some(OriginalThreadDisposition::ClosedOnCapture { thread_id: tid1 })
    );
    assert!(owner1.has_retained_hello());
    assert!(owner1.is_no_replay_sealed());
    assert!(owner1.is_sticky_unknown());
    drop(res1);

    // Case 2: First pipe clone succeeds, second pipe clone fails.
    let mut res2 = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup 2");
    let (proc2, pid2) = spawn_exited_child_process();
    let (thread2, tid2) = duplicate_current_thread_handle();
    let owner2 = Arc::clone(res2.owner());
    let mut clone_attempt = 0usize;
    let err2 = res2
        .install_launched_process_with_cloner(
            proc2,
            pid2,
            thread2,
            tid2,
            &pipe_w,
            &pipe_r,
            dummy_hello(),
            |f| {
                clone_attempt += 1;
                if clone_attempt == 1 {
                    f.try_clone()
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "simulated second pipe clone failure",
                    ))
                }
            },
        )
        .expect_err("second pipe clone failure must fail install_launched_process");
    assert_eq!(err2.kind(), io::ErrorKind::InvalidData);
    assert!(owner2.has_retained_process_handle());
    assert_eq!(owner2.identity().map(|id| id.pid), Some(pid2));
    assert_eq!(
        owner2.thread_disposition(),
        Some(OriginalThreadDisposition::ClosedOnCapture { thread_id: tid2 })
    );
    assert!(owner2.has_retained_hello());
    assert!(owner2.is_no_replay_sealed());
    assert!(owner2.is_sticky_unknown());
    drop(res2);

    assert_eq!(REGISTRY.active_count().expect("both retained"), 2);
    for owner in [owner1, owner2] {
        let outcome = owner
            .finalize(Instant::now() + Duration::from_secs(1))
            .expect("reap retained clone-failure owner");
        assert!(matches!(
            outcome,
            CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
                no_replay_sealed: true,
                release_eligible: false,
                ..
            }
        ));
    }
    assert_eq!(REGISTRY.active_count().expect("both reaped"), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reader_completion_recording_eof_through_owner_state_while_finalize_waits_does_not_deadlock()
{
    let (proc_handle, pid) = spawn_exited_child_process();
    let identity = OriginalProcessIdentity::from_live_process(&proc_handle, pid).unwrap();
    let owner = CheckedRunnerOwner::new_scoped_for_actor(
        dummy_artifact(),
        dummy_hello(),
        RunnerControlCapabilities::default(),
        proc_handle,
        identity,
    );

    let (finalize_entered_tx, finalize_entered_rx) = mpsc::channel::<()>();
    let reader_owner = Arc::clone(&owner);
    let mut admission = owner.reserve_runtime_workers().expect("reserve before worker effects");
    let reader_handle = std::thread::spawn(move || {
        // Wait until the finalizer is actively running and waiting on the reader native handle.
        let _ = finalize_entered_rx.recv();
        std::thread::sleep(Duration::from_millis(25));
        // Both calls acquire `owner.state.lock()`; if `finalize` held `owner.state` across
        // the native thread wait/join, this would deadlock.
        reader_owner.record_protocol_exit(0);
        reader_owner.record_reader_physical_eof();
        ReaderWorkerOutcome {
            protocol_exit: Some(0),
            physical_eof: true,
            clean: true,
        }
    });
    let writer_handle = std::thread::spawn(|| WriterWorkerOutcome {
        clean: true,
        termination_sent: false,
    });
    let input_handle = tokio::spawn(async { InputWorkerOutcome { clean: true } });

    admission.install_runtime_workers(reader_handle, writer_handle, input_handle, Box::new(|| {}));
    drop(admission);
    let _ = finalize_entered_tx.send(());

    let outcome = owner
        .finalize_async(Instant::now() + Duration::from_secs(2))
        .await
        .expect("finalize must complete without lock inversion or deadlock");
    assert_eq!(
        outcome,
        CheckedFinalizationOutcome::CleanRelease {
            startup_id: owner.startup_id(),
            protocol_exit_code: 0,
            native_exit_code: 0,
            release_eligible: false,
        }
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finalizer_concurrent_with_registry_lookup_preserves_lock_ordering_and_completes() {
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();

    let (pipe_w, pipe_r) = temp_pipe_stub_pair();
    let mut res = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup");
    let (proc_handle, pid) = spawn_exited_child_process();
    let (thread_handle, tid) = duplicate_current_thread_handle();
    res.install_launched_process(
        proc_handle,
        pid,
        thread_handle,
        tid,
        &pipe_w,
        &pipe_r,
        dummy_hello(),
    )
    .expect("install launched process");
    let attempt_in = res
        .connect_pipe_step_with(&pipe_w, "pipe-in", None, |_| Ok(()))
        .expect("connect pipe-in");
    res.connect_pipe_step_with(&pipe_r, "pipe-out", None, |_| Ok(()))
        .expect("connect pipe-out");
    let lease = res
        .into_transport_lease(RunnerControlCapabilities::default())
        .expect("into transport lease");
    let (io_token, _identity, _thread_disp) = lease.into_io_token().expect("into io token");
    let owner = io_token.adopt_into_actor().expect("adopt into actor");
    let startup_id = owner.startup_id();

    let reader_owner = Arc::clone(&owner);
    let mut admission = owner.reserve_runtime_workers().expect("reserve before worker effects");
    let reader_handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(35));
        reader_owner.record_protocol_exit(0);
        reader_owner.record_reader_physical_eof();
        ReaderWorkerOutcome {
            protocol_exit: Some(0),
            physical_eof: true,
            clean: true,
        }
    });
    let writer_handle = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(20));
        WriterWorkerOutcome {
            clean: true,
            termination_sent: false,
        }
    });
    let input_handle = tokio::spawn(async { InputWorkerOutcome { clean: true } });
    admission.install_runtime_workers(reader_handle, writer_handle, input_handle, Box::new(|| {}));
    drop(admission);

    let lookup_thread = std::thread::spawn(move || {
        for _ in 0..64 {
            let _ = REGISTRY.find_by_attempt_id(attempt_in);
            let _ = REGISTRY.find_by_startup_id(startup_id);
            let _ = REGISTRY.active_count();
            std::thread::yield_now();
        }
    });

    let outcome = owner
        .finalize_async(Instant::now() + Duration::from_secs(2))
        .await
        .expect("concurrent finalize must succeed");
    lookup_thread.join().expect("lookup thread join");

    assert_eq!(
        outcome,
        CheckedFinalizationOutcome::CleanRelease {
            startup_id,
            protocol_exit_code: 0,
            native_exit_code: 0,
            release_eligible: false,
        }
    );
    assert_eq!(REGISTRY.active_count().expect("slot released"), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn late_native_observation_after_expired_finalization_deadline_latches_sticky_unknown() {
    let (proc_handle, pid) = spawn_exited_child_process();
    let identity = OriginalProcessIdentity::from_live_process(&proc_handle, pid).unwrap();
    let owner = CheckedRunnerOwner::new_scoped_for_actor(
        dummy_artifact(),
        dummy_hello(),
        RunnerControlCapabilities::default(),
        proc_handle,
        identity,
    );
    owner.record_protocol_exit(0);
    owner.record_reader_physical_eof();

    let mut admission = owner.reserve_runtime_workers().expect("reserve before worker effects");
    let reader_handle = std::thread::spawn(|| ReaderWorkerOutcome {
        protocol_exit: Some(0),
        physical_eof: true,
        clean: true,
    });
    let writer_handle = std::thread::spawn(|| WriterWorkerOutcome {
        clean: true,
        termination_sent: false,
    });
    let input_handle = tokio::spawn(async { InputWorkerOutcome { clean: true } });
    admission.install_runtime_workers(reader_handle, writer_handle, input_handle, Box::new(|| {}));
    drop(admission);

    // Allow all workers to finish so their native handles are already signaled, then call
    // `finalize_async` with an already-expired deadline. Even though the OS handles are signaled,
    // the fresh deadline admission check must reject timely completion and latch sticky Unknown.
    tokio::time::sleep(Duration::from_millis(20)).await;
    let expired_deadline = Instant::now() - Duration::from_millis(1);
    let err = owner
        .finalize_async(expired_deadline)
        .await
        .expect_err("expired finalization deadline must not produce a clean timely release");
    assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
    assert!(owner.is_no_replay_sealed());
    assert!(owner.is_sticky_unknown());

    // Subsequent physical reap with a fresh deadline may reap the handles, but MUST preserve
    // sticky Unknown/noReplay and never return CleanRelease.
    let reaped = owner
        .finalize_async(Instant::now() + Duration::from_secs(2))
        .await
        .expect("late physical reap succeeds with sticky Unknown");
    assert!(matches!(
        reaped,
        CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
            native_exit_code: Some(0),
            no_replay_sealed: true,
            release_eligible: false,
            ..
        }
    ));
    assert!(owner.is_no_replay_sealed());
    assert!(owner.is_sticky_unknown());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn thread_close_failure_retains_exact_close_debt_never_retries_and_blocks_physical_reap_or_slot_release()
{
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();

    let (pipe_w, pipe_r) = temp_pipe_stub_pair();
    let mut res = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup");

    let (proc_handle, pid) = spawn_exited_child_process();
    let (thread_handle, tid) = duplicate_current_thread_handle();
    let expected_raw = thread_handle.as_raw_handle() as usize;
    assert_ne!(expected_raw, 0);

    let close_calls = Arc::new(AtomicUsize::new(0));
    let close_calls_hook = Arc::clone(&close_calls);
    let owner = Arc::clone(res.owner());
    let startup_id = owner.startup_id();

    let err = res
        .install_launched_process_with_hooks(
            proc_handle,
            pid,
            thread_handle,
            tid,
            &pipe_w,
            &pipe_r,
            dummy_hello(),
            File::try_clone,
            move |th, thread_id| {
                OriginalThreadDisposition::close_original_thread_with(th, thread_id, |raw| {
                    close_calls_hook.fetch_add(1, Ordering::SeqCst);
                    // Close the real duplicated OS handle once so the test process does not leak,
                    // then inject a CloseHandle failure result without fabricating an invalid OwnedHandle.
                    let ok = unsafe { CloseHandle(raw) };
                    assert_ne!(ok, 0);
                    Err(ERROR_INVALID_HANDLE)
                })
            },
        )
        .expect_err("injected CloseHandle failure must fail install_launched_process");

    assert_eq!(err.raw_os_error(), Some(ERROR_INVALID_HANDLE as i32));
    assert_eq!(close_calls.load(Ordering::SeqCst), 1);

    let expected_debt = OriginalThreadCloseDebt {
        raw_handle: expected_raw,
        thread_id: tid,
        os_error: ERROR_INVALID_HANDLE,
    };
    assert_eq!(owner.thread_close_debt(), Some(expected_debt));
    assert!(owner.has_unresolved_thread_close_debt());
    assert_eq!(
        owner.thread_disposition(),
        Some(OriginalThreadDisposition::CloseFailedUnknown {
            raw_handle: expected_raw,
            thread_id: tid,
            os_error: ERROR_INVALID_HANDLE,
        })
    );
    assert_eq!(
        owner.sticky_unknown_reason(),
        Some(StickyUnknownReason::ThreadCloseFailed {
            raw_handle: expected_raw,
            thread_id: tid,
            os_error: ERROR_INVALID_HANDLE,
        })
    );
    assert!(owner.is_no_replay_sealed());
    assert!(owner.is_sticky_unknown());
    drop(res);

    // Neither sync `finalize` nor async `finalize_async` may report CleanRelease or
    // PhysicallyReapedWithStickyUnknown, retry CloseHandle, or release the registry slot.
    let sync_err = owner
        .finalize(Instant::now() + Duration::from_secs(1))
        .expect_err("sync finalize must reject unresolved original thread close debt");
    assert!(
        sync_err
            .to_string()
            .contains("unresolved original thread close debt")
    );

    let async_err = owner
        .finalize_async(Instant::now() + Duration::from_secs(1))
        .await
        .expect_err("async finalize must reject unresolved original thread close debt");
    assert!(
        async_err
            .to_string()
            .contains("unresolved original thread close debt")
    );

    assert_eq!(
        close_calls.load(Ordering::SeqCst),
        1,
        "uncertain raw HANDLE must never be reconstructed, dropped, or retried with CloseHandle"
    );
    assert_eq!(
        owner.stage(),
        Some(OwnerLifecycleStage::FailedOrAbandoned)
    );
    assert_eq!(owner.thread_close_debt(), Some(expected_debt));
    assert_eq!(
        REGISTRY
            .active_count()
            .expect("slot remains charged with unresolved thread close debt"),
        1
    );
    let retained = REGISTRY
        .find_by_startup_id(startup_id)
        .expect("owner with unresolved thread close debt remains reachable in registry");
    assert_eq!(retained.thread_close_debt(), Some(expected_debt));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_sync_and_async_finalizers_and_input_poll_share_outcome_without_missing_slot_or_false_unknown()
{
    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: owned_connect_worker::Owner = owned_connect_worker::Owner::new();

    let (pipe_w, pipe_r) = temp_pipe_stub_pair();
    let mut res = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        dummy_artifact(),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup");
    let (proc_handle, pid) = spawn_exited_child_process();
    let (thread_handle, tid) = duplicate_current_thread_handle();
    res.install_launched_process(
        proc_handle,
        pid,
        thread_handle,
        tid,
        &pipe_w,
        &pipe_r,
        dummy_hello(),
    )
    .expect("install launched process");
    res.connect_pipe_step_with(&pipe_w, "pipe-in", None, |_| Ok(()))
        .expect("connect pipe-in");
    res.connect_pipe_step_with(&pipe_r, "pipe-out", None, |_| Ok(()))
        .expect("connect pipe-out");
    let lease = res
        .into_transport_lease(RunnerControlCapabilities::default())
        .expect("into transport lease");
    let (io_token, _identity, _thread_disp) = lease.into_io_token().expect("into io token");
    let owner = io_token.adopt_into_actor().expect("adopt into actor");
    let startup_id = owner.startup_id();

    let (reader_release_tx, reader_release_rx) = mpsc::channel::<()>();
    let reader_owner = Arc::clone(&owner);
    let mut admission = owner.reserve_runtime_workers().expect("reserve before worker effects");
    let reader_handle = std::thread::spawn(move || {
        let _ = reader_release_rx.recv();
        reader_owner.record_protocol_exit(23);
        reader_owner.record_reader_physical_eof();
        ReaderWorkerOutcome {
            protocol_exit: Some(23),
            physical_eof: true,
            clean: true,
        }
    });
    let writer_handle = std::thread::spawn(|| WriterWorkerOutcome {
        clean: true,
        termination_sent: false,
    });
    let input_handle = tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        InputWorkerOutcome { clean: true }
    });
    admission.install_runtime_workers(reader_handle, writer_handle, input_handle, Box::new(|| {}));
    drop(admission);
    assert!(owner.has_retained_thread_anchors());
    assert!(owner.has_installed_runtime_workers());

    // 1. Start the primary async finalizer, which claims the exclusive FinalizerClaimGuard.
    let async_owner = Arc::clone(&owner);
    let async_finalizer = tokio::spawn(async move {
        async_owner
            .finalize_async(Instant::now() + Duration::from_secs(2))
            .await
    });

    // Wait until the primary finalizer holds the gate.
    for _ in 0..50 {
        if owner.is_finalization_in_progress() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert!(owner.is_finalization_in_progress());
    assert!(owner.has_retained_thread_anchors());
    assert!(owner.has_retained_process_handle());

    // 2. Concurrent input polling while finalizer holds the gate is a safe no-op.
    let poll_owner = Arc::clone(&owner);
    let poller = std::thread::spawn(move || {
        for _ in 0..64 {
            poll_owner.try_poll_input_worker_now();
            std::thread::yield_now();
        }
    });

    // 3. A short-budget contender timing out on the finalizer gate returns WouldBlock
    // WITHOUT latching sticky Unknown or noReplay.
    let short_contender = owner
        .finalize(Instant::now() + Duration::from_millis(5))
        .expect_err("short contender must time out with WouldBlock while gate is held");
    assert_eq!(short_contender.kind(), io::ErrorKind::WouldBlock);
    assert!(
        !owner.is_sticky_unknown(),
        "temporary finalizer contention must not latch sticky Unknown"
    );
    assert!(
        !owner.is_no_replay_sealed(),
        "temporary finalizer contention must not seal noReplay"
    );

    // 4. Start a concurrent sync finalizer and a second async finalizer with sufficient budgets;
    // both must wait on the gate and observe the same CleanRelease outcome.
    let sync_owner = Arc::clone(&owner);
    let sync_finalizer = std::thread::spawn(move || {
        sync_owner.finalize(Instant::now() + Duration::from_secs(2))
    });
    let async_owner2 = Arc::clone(&owner);
    let async_finalizer2 = tokio::spawn(async move {
        async_owner2
            .finalize_async(Instant::now() + Duration::from_secs(2))
            .await
    });

    tokio::time::sleep(Duration::from_millis(15)).await;
    let _ = reader_release_tx.send(());

    poller.join().expect("poller join");
    let outcome_async1 = async_finalizer
        .await
        .expect("async finalizer task")
        .expect("async finalizer 1 outcome");
    let outcome_sync = sync_finalizer
        .join()
        .expect("sync finalizer thread")
        .expect("sync finalizer outcome");
    let outcome_async2 = async_finalizer2
        .await
        .expect("async finalizer 2 task")
        .expect("async finalizer 2 outcome");

    let expected = CheckedFinalizationOutcome::CleanRelease {
        startup_id,
        protocol_exit_code: 23,
        native_exit_code: 0,
        release_eligible: false,
    };
    assert_eq!(outcome_async1, expected);
    assert_eq!(outcome_sync, expected);
    assert_eq!(outcome_async2, expected);
    assert!(!owner.is_sticky_unknown());
    assert!(!owner.is_no_replay_sealed());
    assert_eq!(REGISTRY.active_count().expect("slot released"), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_slots_restore_after_panic_unwind_and_cancelled_finalize_async_future() {
    let (proc_handle, pid) = spawn_exited_child_process();
    let identity = OriginalProcessIdentity::from_live_process(&proc_handle, pid).unwrap();
    let owner = CheckedRunnerOwner::new_scoped_for_actor(
        dummy_artifact(),
        dummy_hello(),
        RunnerControlCapabilities::default(),
        proc_handle,
        identity,
    );
    owner.record_protocol_exit(0);
    owner.record_reader_physical_eof();

    let (input_gate_tx, input_gate_rx) = tokio::sync::oneshot::channel::<()>();
    let mut admission = owner.reserve_runtime_workers().expect("reserve before worker effects");
    let reader_handle = std::thread::spawn(|| ReaderWorkerOutcome {
        protocol_exit: Some(0),
        physical_eof: true,
        clean: true,
    });
    let writer_handle = std::thread::spawn(|| WriterWorkerOutcome {
        clean: true,
        termination_sent: false,
    });
    let input_handle = tokio::spawn(async move {
        let _ = input_gate_rx.await;
        InputWorkerOutcome { clean: true }
    });
    admission.install_runtime_workers(reader_handle, writer_handle, input_handle, Box::new(|| {}));
    drop(admission);
    assert!(owner.has_installed_runtime_workers());

    // 1. Panic unwind while holding ReaderSlotLoanGuard + WriterSlotLoanGuard under FinalizerClaimGuard
    // must restore both slots to WorkerSlotState::Installed and release the finalizer gate.
    let unwind_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        owner.simulate_loan_unwind_for_test();
    }));
    assert!(unwind_res.is_err());
    assert!(!owner.is_finalization_in_progress());
    assert!(owner.has_installed_runtime_workers());
    assert!(!owner.is_sticky_unknown());

    // 2. Dropping/cancelling `finalize_async` at `.await` while `InputSlotLoanGuard` is held
    // must restore the async input slot and release the finalizer gate without sealing Unknown.
    {
        let fut = owner.finalize_async(Instant::now() + Duration::from_secs(2));
        tokio::pin!(fut);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut fut)
                .await
                .is_err(),
            "finalize_async should suspend at input task await while input_gate_rx is open"
        );
        assert!(owner.is_finalization_in_progress());
    }
    assert!(
        !owner.is_finalization_in_progress(),
        "dropping cancelled finalize_async future must release FinalizerClaimGuard"
    );
    assert!(
        owner.has_installed_runtime_workers(),
        "dropping cancelled finalize_async future must restore input_worker to Installed"
    );
    assert!(!owner.is_sticky_unknown());

    // 3. Once the input worker completes, a subsequent finalizer cleanly reaps all workers and the process.
    let _ = input_gate_tx.send(());
    let outcome = owner
        .finalize_async(Instant::now() + Duration::from_secs(2))
        .await
        .expect("subsequent finalize_async after unwind and cancellation must succeed");
    assert_eq!(
        outcome,
        CheckedFinalizationOutcome::CleanRelease {
            startup_id: owner.startup_id(),
            protocol_exit_code: 0,
            native_exit_code: 0,
            release_eligible: false,
        }
    );
}



// Admission/retention regressions. These are source additions only in the coding
// handoff; the main session must compile and execute them on Windows.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_admission_retains_exact_originals_across_owner_poison_and_first_error() {
    let owner = CheckedRunnerOwner::new(StartupId(991),
        Instant::now() + Duration::from_secs(5), None, None, &SCOPED_CONNECT_OWNER, None);
    let mut admission = owner.reserve_runtime_workers().unwrap();
    let (release, wait) = mpsc::channel();
    let writer = std::thread::spawn(move || {
        let _ = wait.recv();
        WriterWorkerOutcome { clean: true, termination_sent: false }
    });
    let original_writer = writer.as_raw_handle() as usize;
    let reader = std::thread::spawn(|| ReaderWorkerOutcome {
        protocol_exit: None, physical_eof: true, clean: true,
    });
    let original_reader = reader.as_raw_handle() as usize;
    let input = tokio::spawn(async { InputWorkerOutcome { clean: true } });
    let original_input = input.id();
    let first = StickyUnknownReason::HandshakeFailed("first failure".into());
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut state = owner.state.lock().unwrap();
        state.no_replay_seal = true;
        state.sticky_unknown = Some(first.clone());
        panic!("poison after original worker creation and before installation");
    }));
    // Test injection exercises the infallible install boundary for all three
    // already-created originals. Production factories reject any subsequent effects.
    admission.install_runtime_workers(reader, writer, input, Box::new(|| {}));
    drop(admission);
    let (mut writer, mut reader, mut input) = {
        let mut state = owner.state.lock().unwrap_or_else(|e| e.into_inner());
        assert!(state.no_replay_seal);
        assert_eq!(state.sticky_unknown, Some(first));
        match &state.writer_worker {
            WorkerSlotState::Installed(StdWorkerSlot::Pending(handle)) =>
                assert_eq!(handle.as_raw_handle() as usize, original_writer),
            _ => panic!("original writer missing"),
        }
        match &state.reader_worker {
            WorkerSlotState::Installed(StdWorkerSlot::Pending(handle)) =>
                assert_eq!(handle.as_raw_handle() as usize, original_reader),
            _ => panic!("original reader missing"),
        }
        match &state.input_worker {
            WorkerSlotState::Installed(AsyncInputSlot::Pending(handle)) => assert_eq!(handle.id(), original_input),
            _ => panic!("original input missing"),
        }
        (state.writer_worker.loan().unwrap(), state.reader_worker.loan().unwrap(), state.input_worker.loan().unwrap())
    };
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    writer.observe_and_join(deadline, "poisoned writer").unwrap();
    reader.observe_and_join(deadline, "poisoned reader").unwrap();
    input.observe_and_join_blocking(deadline).unwrap();
    let mut state = owner.state.lock().unwrap_or_else(|e| e.into_inner());
    state.writer_worker.restore(writer);
    state.reader_worker.restore(reader);
    state.input_worker.restore(input);
}

#[test]
fn worker_admission_refuses_reserved_and_loaned_slots_before_spawner_effects() {
    let owner = CheckedRunnerOwner::new(StartupId(992),
        Instant::now() + Duration::from_secs(5), None, None, &SCOPED_CONNECT_OWNER, None);
    let mut admission = owner.reserve_runtime_workers().unwrap();
    assert!(owner.reserve_runtime_workers().is_err());
    let calls = AtomicUsize::new(0);
    admission.spawn_writer(|| {
        calls.fetch_add(1, Ordering::SeqCst);
        assert!(owner.state.try_lock().is_ok(), "spawner must run outside owner lock");
        Ok(std::thread::spawn(|| WriterWorkerOutcome { clean: true, termination_sent: false }))
    }, Arc::new(|| {})).unwrap();
    assert!(admission.spawn_writer(|| {
        calls.fetch_add(1, Ordering::SeqCst);
        panic!("second writer must never be created")
    }, Arc::new(|| {})).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(admission);
    let mut claim = owner.claim_finalizer_blocking(Instant::now() + Duration::from_secs(1))
        .unwrap().unwrap();
    {
        let mut loan = WriterSlotLoanGuard::new(&mut claim).unwrap();
        assert!(owner.reserve_runtime_workers().is_err(), "loan remains charged");
        loan.slot.as_mut().unwrap().observe_and_join(
            Instant::now() + Duration::from_secs(1), "admitted writer").unwrap();
    }
    drop(claim);
    assert!(owner.reserve_runtime_workers().is_err(), "installed slot cannot be overwritten");
}

// No OS process/handle is installed: only the original Tokio input task is real.
async fn assert_async_finalize_input_kind(
    handle: TokioJoinHandle<InputWorkerOutcome>,
    expected_kind: Option<HostAsyncInputJoinErrorKind>,
    expected_clean: Option<bool>,
) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let owner = CheckedRunnerOwner::new(StartupId(2501), deadline, None, None, &SCOPED_CONNECT_OWNER, None);
    {
        let mut state = owner.state.lock().unwrap();
        // Model partial actor startup with only the actual input task installed:
        // missing std workers must not mask its real join outcome/first error.
        state.partial_actor_startup_aborted = true;
        state.input_worker = WorkerSlotState::Installed(AsyncInputSlot::Pending(handle));
    }
    let _ = owner.finalize_async(deadline).await;
    let state = owner.state.lock().unwrap();
    let WorkerSlotState::Installed(AsyncInputSlot::Joined { outcome, error_kind, .. }) = &state.input_worker else {
        panic!("original input join result must be retained");
    };
    assert_eq!(*error_kind, expected_kind);
    match expected_clean {
        Some(clean) => assert_eq!(outcome.as_ref().unwrap().clean, clean),
        None => assert!(outcome.is_err()),
    }
    assert!(state.native_process_exit_code.is_none(), "a task result is not native DWORD proof");
    if expected_kind.is_some() || expected_clean == Some(false) {
        assert!(state.no_replay_seal);
        assert!(matches!(state.sticky_unknown, Some(StickyUnknownReason::InputTaskFailedOrAborted)));
    }
}
#[tokio::test]
async fn finalize_async_records_original_cancelled_input_join_kind_without_native_success() {
    let task = tokio::spawn(std::future::pending::<InputWorkerOutcome>());
    task.abort();
    assert_async_finalize_input_kind(task, Some(HostAsyncInputJoinErrorKind::Cancelled), None).await;
}
#[tokio::test]
async fn finalize_async_records_original_panicked_input_join_kind_without_native_success() {
    async fn original_input() -> InputWorkerOutcome { panic!("test input panic"); }
    let task = tokio::spawn(original_input());
    assert_async_finalize_input_kind(task, Some(HostAsyncInputJoinErrorKind::Panicked), None).await;
}
#[tokio::test]
async fn finalize_async_keeps_joined_but_unclean_input_distinct_from_join_failure() {
    let task = tokio::spawn(async { InputWorkerOutcome { clean: false } });
    assert_async_finalize_input_kind(task, None, Some(false)).await;
}


// These fixtures exercise only the original process wait/exit projection.
// They create ordinary short-lived test children, no accounts, policy or GUI.
// Missing runner/worker/protocol resources never manufacture a completion.
fn original_finalizer_wait_fixture(code: i32, wrong_identity: bool) -> CheckedRunnerOwner {
    use std::os::windows::process::CommandExt;
    let command = std::path::PathBuf::from(crate::native_system_windows_directory().unwrap())
        .join("System32").join("cmd.exe");
    let mut child = Command::new(command)
        .args(["/D", "/C", "exit", "/b", &code.to_string()])
        .creation_flags(0x08000000)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let pid = child.id();
    assert_eq!(child.wait().unwrap().code(), Some(code));
    let original: OwnedHandle = child.into();
    let mut identity = OriginalProcessIdentity::from_live_process(&original, pid).unwrap();
    if wrong_identity { identity.creation_filetime ^= 1; }
    let owner = CheckedRunnerOwner::new(StartupId(2837), Instant::now()+Duration::from_secs(10),
        None, None, &SCOPED_CONNECT_OWNER, None);
    {
        let mut state = owner.state.lock().unwrap();
        state.process = Some(Arc::new(original));
        state.identity = Some(identity);
        state.runner_wait_and_exit_fact = HostOriginalRunnerProcessWaitAndExitFact::HandleRetainedWaitNotYetAttempted;
    }
    owner
}

#[test]
fn original_finalizer_wait_records_real_exit_dword_without_minting_completion() {
    for code in [0, 259, i32::MIN + 7] {
        let owner = original_finalizer_wait_fixture(code, false);
        let outcome = owner.finalize(Instant::now()+Duration::from_secs(5)).unwrap();
        assert!(matches!(outcome, CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown { .. }));
        let snapshot = owner.host_native_close_facts();
        assert_eq!(snapshot.runner_process_wait_and_exit.verified_exited_raw_dword(),
            Some(super::super::native_close_protocol::RawProcessExitCodeDword(code as u32)));
        let projected = snapshot.to_v8_receiver_physical_close_facts();
        assert_eq!(projected.runner_exit_code_dword,
            Some(super::super::native_close_protocol::RawProcessExitCodeDword(code as u32)));
        assert_ne!(projected.runner_handle_closed_verified,
            super::super::native_close_protocol::ObservationProof::ProvenOnOriginal);
        assert!(owner.is_no_replay_sealed());
    }
}

#[test]
fn original_finalizer_wait_rejects_foreign_identity_before_exit_proof() {
    let owner = original_finalizer_wait_fixture(0, true);
    assert!(owner.finalize(Instant::now()+Duration::from_secs(5)).is_err());
    let snapshot = owner.host_native_close_facts();
    assert!(matches!(snapshot.runner_process_wait_and_exit,
        HostOriginalRunnerProcessWaitAndExitFact::IdentityVerificationFailedBeforeWait { .. }));
    assert_eq!(snapshot.to_v8_receiver_physical_close_facts().runner_exit_code_dword, None);
    assert!(owner.is_no_replay_sealed());
}

#[test]
fn original_finalizer_wait_absence_stays_unobserved() {
    let owner = CheckedRunnerOwner::new(StartupId(2838), Instant::now()+Duration::from_secs(10),
        None, None, &SCOPED_CONNECT_OWNER, None);
    assert!(owner.finalize(Instant::now()+Duration::from_secs(5)).is_ok());
    let snapshot = owner.host_native_close_facts();
    assert_eq!(snapshot.runner_process_wait_and_exit, HostOriginalRunnerProcessWaitAndExitFact::NoProcessHandlePresent);
    assert_eq!(snapshot.to_v8_receiver_physical_close_facts().runner_exit_code_dword, None);
}
