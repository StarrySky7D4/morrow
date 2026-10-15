use super::*;
use crate::MatchedRunnerArtifact;
use sha2::{Digest, Sha256};
use std::os::windows::io::FromRawHandle;
use std::time::Duration;
use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentThread};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn framed_eof_without_exit_reports_failure_and_keeps_exit_and_eof_unresolved() {
    let exe = std::env::current_exe().unwrap();
    let sha: [u8; 32] = Sha256::digest(std::fs::read(&exe).unwrap()).into();
    let artifact = Arc::new(MatchedRunnerArtifact::acquire(&exe, sha).unwrap());
    let input_transport = tempfile::tempfile().unwrap();
    let output_transport = tempfile::tempfile().unwrap(); // actual reader observes EOF
    let (writer_tx, input_rx) = mpsc::channel(1);
    let (stdout_tx, stdout_rx) = broadcast::channel(1);
    let (exit_tx, mut exit_rx) = oneshot::channel();
    let checked = start_checked_runner(
        input_transport,
        output_transport,
        ControlHello {
            nonce: [1; 32],
            schema: CHECKED_CONTROL_SCHEMA,
        },
        RunnerControlCapabilities {
            close_stdin: true,
            resize: false,
        },
        artifact,
        input_rx,
        false,
        stdout_tx,
        None,
        exit_tx,
    );
    let mut failure = checked.controls.lifecycle_failure().unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if failure.borrow().is_some() {
                break;
            }
            failure.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        exit_rx.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    let spawned = codex_utils_pty::spawn_from_driver_with_checked_controls(
        codex_utils_pty::ProcessDriver {
            writer_tx,
            stdout_rx,
            stderr_rx: None,
            exit_rx,
            terminator: None,
            writer_handle: None,
            resizer: None,
            tty: false,
        },
        checked.controls,
    );
    assert!(
        spawned
            .session
            .checked_driver_failure()
            .unwrap()
            .borrow()
            .is_some()
    );
    assert!(!spawned.session.has_exited());
    let mut stdout = spawned.stdout_rx;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), stdout.recv())
            .await
            .is_err()
    );
    let mut exit = spawned.exit_rx;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut exit)
            .await
            .is_err()
    );
    // No LocalExecProcess/native CAS is involved in this pure adapter test.
    // End-to-end failure-to-facts qualification is a separate integration gate.
}

fn correlation() -> (
    Correlation,
    watch::Receiver<Option<String>>,
    oneshot::Receiver<i32>,
) {
    let (failure, failure_rx) = watch::channel(None);
    let (terminal, _) = watch::channel(false);
    let (close_done, _) = watch::channel(None);
    let (exit_tx, exit_rx) = oneshot::channel();
    let (stdout_tx, _) = broadcast::channel(1);
    (
        Correlation {
            hello: ControlHello {
                nonce: [2; 32],
                schema: CHECKED_CONTROL_SCHEMA,
            },
            _artifact: None,
            owner: None,
            next_id: 3,
            failed: false,
            exit_code: None,
            terminal,
            failure,
            records: BTreeMap::new(),
            close_done,
            exit_tx: Some(exit_tx),
            stdout_tx: Some(stdout_tx),
            stderr_tx: None,
        },
        failure_rx,
        exit_rx,
    )
}

#[tokio::test]
async fn real_exit_settles_pending_but_preserves_applied_without_lifecycle_failure() {
    let (mut state, failure, mut exit) = correlation();
    let pending = ControlRequest::new(1, state.hello.nonce, ControlOperation::CloseStdin, 0, 0);
    let applied = ControlRequest::new(2, state.hello.nonce, ControlOperation::Resize, 24, 80);
    let (pending_tx, pending_rx) = watch::channel(None);
    let result = ControlResult::new(applied.clone(), ControlStatus::Applied, None);
    let (applied_tx, applied_rx) = watch::channel(Some(result.clone()));
    state.records.insert(1, (pending, pending_tx));
    state.records.insert(2, (applied, applied_tx));
    state.exited(7);
    assert!(
        matches!(exit.try_recv(), Err(oneshot::error::TryRecvError::Empty)),
        "protocol Exit is business data only and must not publish exit_tx before native finalization"
    );
    assert!(
        state.stdout_tx.is_some(),
        "protocol Exit must not close output channels before native finalization"
    );
    assert!(failure.borrow().is_none());
    assert!(!state.failed);
    assert!(*state.terminal.borrow());
    assert!(
        tokio::time::timeout(Duration::from_millis(200), wait_result(pending_rx))
            .await
            .unwrap()
            .is_err()
    );
    assert_eq!(*applied_rx.borrow(), Some(result));
    assert_eq!(state.next_id, 3); // Exit does not allocate/replay controls.

    state.complete_native_lifecycle(7);
    assert_eq!(exit.await.unwrap(), 7);
    assert!(state.stdout_tx.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_unknown_publishes_failure_before_control_observer_wakes() {
    // Observe the real independent watches from another scheduler worker.
    for _ in 0..128 {
        let (mut state, failure, _) = correlation();
        let request = ControlRequest::new(1, state.hello.nonce, ControlOperation::Resize, 24, 80);
        let (done, mut control) = watch::channel(None);
        state.records.insert(1, (request.clone(), done));
        let observer = tokio::spawn(async move {
            control.changed().await.unwrap();
            assert!(
                failure.borrow().is_some(),
                "control woke before lifecycle evidence loss"
            );
            let observed = control.borrow().clone().unwrap();
            observed
        });
        tokio::task::yield_now().await;
        let remote = ControlResult::new(request, ControlStatus::Unknown, Some(5));
        assert!(state.result(remote.clone()));
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), observer)
                .await
                .unwrap()
                .unwrap(),
            remote
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn framed_exit_before_ack_stops_input_actor_even_with_owned_permit() {
    use std::io::{Seek, SeekFrom};
    let exe = std::env::current_exe().unwrap();
    let sha: [u8; 32] = Sha256::digest(std::fs::read(&exe).unwrap()).into();
    let artifact = Arc::new(MatchedRunnerArtifact::acquire(&exe, sha).unwrap());
    let input_transport = tempfile::tempfile().unwrap();
    let mut output_transport = tempfile::tempfile().unwrap();
    write_frame(
        &mut output_transport,
        &FramedMessage {
            version: CHECKED_IPC_PROTOCOL_VERSION,
            message: Message::Exit {
                payload: ExitPayload {
                    exit_code: 9,
                    timed_out: false,
                },
            },
        },
    )
    .unwrap();
    output_transport.seek(SeekFrom::Start(0)).unwrap();
    let (writer_tx, input_rx) = mpsc::channel(1);
    let permit = writer_tx.clone().reserve_owned().await.unwrap();
    let (stdout_tx, mut stdout_rx) = broadcast::channel(1);
    let (exit_tx, mut exit_rx) = oneshot::channel();
    let checked = start_checked_runner(
        input_transport,
        output_transport,
        ControlHello {
            nonce: [3; 32],
            schema: CHECKED_CONTROL_SCHEMA,
        },
        RunnerControlCapabilities {
            close_stdin: true,
            resize: true,
        },
        artifact,
        input_rx,
        false,
        stdout_tx,
        None,
        exit_tx,
    );
    tokio::time::timeout(Duration::from_secs(2), checked.wait_input_stopped())
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(2),
        checked.controls.wait_ready_to_finalize(),
    )
    .await
    .unwrap();
    assert!(
        matches!(exit_rx.try_recv(), Err(oneshot::error::TryRecvError::Empty)),
        "exit_rx must not fire before native owner finalization"
    );
    assert!(
        checked
            .controls
            .lifecycle_failure()
            .unwrap()
            .borrow()
            .is_none()
    );
    let canceled_close = checked.controls.close_stdin();
    drop(canceled_close);
    assert!(
        tokio::time::timeout(Duration::from_millis(200), checked.controls.close_stdin())
            .await
            .unwrap()
            .is_err()
    );
    assert!(
        checked
            .controls
            .resize(TerminalSize { rows: 24, cols: 80 })
            .await
            .is_err()
    );
    assert_eq!(checked.controls.state.lock().unwrap().next_id, 1);

    let outcome = checked
        .finalize_async(Instant::now() + Duration::from_secs(2))
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        CheckedFinalizationOutcome::CleanRelease {
            protocol_exit_code: 9,
            ..
        }
    ));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), exit_rx)
            .await
            .unwrap()
            .unwrap(),
        9
    );
    assert!(matches!(
        stdout_rx.recv().await,
        Err(broadcast::error::RecvError::Closed)
    ));
    drop(permit);
    assert!(writer_tx.send(vec![1]).await.is_err());
}

fn anonymous_pipe_pair() -> (File, File) {
    let mut read_handle: HANDLE = 0;
    let mut write_handle: HANDLE = 0;
    let ok = unsafe {
        windows_sys::Win32::System::Pipes::CreatePipe(
            &mut read_handle,
            &mut write_handle,
            std::ptr::null(),
            0,
        )
    };
    assert_ne!(ok, 0, "CreatePipe must succeed");
    let read_file = unsafe { File::from_raw_handle(read_handle as _) };
    let write_file = unsafe { File::from_raw_handle(write_handle as _) };
    (read_file, write_file)
}

fn spawn_controllable_runner_stub() -> (OwnedHandle, u32, std::process::ChildStdin) {
    let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned());
    let mut child = std::process::Command::new(comspec)
        .args(["/C", "set /p x= & exit /b 0"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn controllable child");
    let pid = child.id();
    let stdin = child.stdin.take().expect("piped stdin");
    let handle: OwnedHandle = child.into();
    (handle, pid, stdin)
}

fn duplicate_current_thread() -> (OwnedHandle, u32) {
    let mut raw: HANDLE = 0;
    let proc_h = unsafe { GetCurrentProcess() };
    let thread_h = unsafe { windows_sys::Win32::System::Threading::GetCurrentThread() };
    let ok = unsafe {
        DuplicateHandle(
            proc_h,
            thread_h,
            proc_h,
            &mut raw,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    assert_ne!(ok, 0);
    (unsafe { OwnedHandle::from_raw_handle(raw as _) }, 1)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn protocol_exit_frame_does_not_finish_owner_before_physical_pipe_eof_and_native_process_exit()
{
    use std::io::{Read, Write};
    static REGISTRY: crate::checked_owner::CheckedOwnerRegistry =
        crate::checked_owner::CheckedOwnerRegistry::new();
    static CONNECT_OWNER: crate::owned_connect_worker::Owner =
        crate::owned_connect_worker::Owner::new();

    let exe = std::env::current_exe().unwrap();
    let sha: [u8; 32] = Sha256::digest(std::fs::read(&exe).unwrap()).into();
    let artifact = Arc::new(MatchedRunnerArtifact::acquire(&exe, sha).unwrap());

    let mut res = crate::checked_owner::reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        Arc::clone(&artifact),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup");

    let (proc_handle, pid, mut child_stdin) = spawn_controllable_runner_stub();
    let (thread_handle, tid) = duplicate_current_thread();
    let (mut pipe_in_read, pipe_in_write) = anonymous_pipe_pair();
    let (pipe_out_read, mut pipe_out_write) = anonymous_pipe_pair();
    let hello = ControlHello {
        nonce: [5; 32],
        schema: CHECKED_CONTROL_SCHEMA,
    };
    let capabilities = RunnerControlCapabilities {
        close_stdin: true,
        resize: true,
    };

    res.install_launched_process(
        proc_handle,
        pid,
        thread_handle,
        tid,
        &pipe_in_write,
        &pipe_out_read,
        hello.clone(),
    )
    .expect("install launched process");
    res.connect_pipe_step_with(&pipe_in_write, "pipe-in", None, |_| Ok(()))
        .expect("connect pipe-in");
    res.connect_pipe_step_with(&pipe_out_read, "pipe-out", None, |_| Ok(()))
        .expect("connect pipe-out");
    let lease = res
        .commit_transport_lease(capabilities)
        .expect("commit transport lease");
    let (owner_token, identity, thread_disposition) =
        lease.into_io_token().expect("into io token");
    let checked_io = CheckedRunnerIo {
        pipe_write: pipe_in_write,
        pipe_read: pipe_out_read,
        hello,
        capabilities,
        artifact,
        identity,
        thread_disposition,
        owner: owner_token,
    };

    let (_writer_tx, input_rx) = mpsc::channel(1);
    let (stdout_tx, mut stdout_rx) = broadcast::channel(1);
    let (exit_tx, mut exit_rx) = oneshot::channel();
    let checked =
        start_checked_runner_with_io(checked_io, input_rx, false, stdout_tx, None, exit_tx)
            .expect("start_checked_runner_with_io");

    // 1. Send protocol Message::Exit while keeping pipe_out_write open and the child process alive.
    write_frame(
        &mut pipe_out_write,
        &FramedMessage {
            version: CHECKED_IPC_PROTOCOL_VERSION,
            message: Message::Exit {
                payload: ExitPayload {
                    exit_code: 0,
                    timed_out: false,
                },
            },
        },
    )
    .expect("write protocol Exit frame");

    // Business Exit retires the input worker and wakes the blocking writer to close the real
    // input pipe immediately without waiting for runner native exit or pipe-out EOF.
    tokio::time::timeout(Duration::from_secs(2), checked.wait_input_stopped())
        .await
        .unwrap();
    let eof_bytes = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 8];
        pipe_in_read.read(&mut buf)
    })
    .await
    .unwrap()
    .expect("read runner pipe-in after business Exit");
    assert_eq!(
        eof_bytes, 0,
        "blocking writer must close real input pipe on business Exit without waiting for runner native exit"
    );

    // Protocol Exit was observed, but physical pipe EOF on pipe-out has NOT occurred yet,
    // and lifecycle exit/output closure must NOT be published yet.
    assert_eq!(checked.owner.protocol_exit_code(), Some(0));
    assert!(!checked.owner.reader_physical_eof_observed());
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            checked.controls.wait_ready_to_finalize()
        )
        .await
        .is_err(),
        "ready_to_finalize must remain pending until physical pipe-out EOF"
    );
    assert!(matches!(
        exit_rx.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    assert!(checked.owner.has_retained_process_handle());
    assert_eq!(REGISTRY.active_count().unwrap(), 1);

    // 2. Close the runner's write end of pipe-out so the host reader thread observes physical EOF.
    drop(pipe_out_write);
    tokio::time::timeout(
        Duration::from_secs(2),
        checked.controls.wait_ready_to_finalize(),
    )
    .await
    .expect("reader must reach physical EOF after pipe_out_write closes");
    assert!(checked.owner.reader_physical_eof_observed());
    assert!(
        matches!(exit_rx.try_recv(), Err(oneshot::error::TryRecvError::Empty)),
        "exit_rx must still remain pending before native runner process exit and finalization"
    );
    assert!(checked.owner.has_retained_process_handle());
    assert_eq!(REGISTRY.active_count().unwrap(), 1);

    // 3. Start finalization while the original runner OS process is still alive, then release the
    // child process after 40ms and verify CleanRelease publishes exit_rx and closes stdout_rx.
    let release_child = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        let _ = child_stdin.write_all(b"\r\n");
        drop(child_stdin);
    });

    let outcome = checked
        .finalize_async(Instant::now() + Duration::from_secs(2))
        .await
        .expect("finalize after reader physical EOF, all IO joins, and native process exit");
    release_child.join().expect("join child release helper");

    assert_eq!(
        outcome,
        CheckedFinalizationOutcome::CleanRelease {
            startup_id: checked.owner.startup_id(),
            protocol_exit_code: 0,
            native_exit_code: 0,
            release_eligible: false,
        }
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), exit_rx)
            .await
            .unwrap()
            .unwrap(),
        0
    );
    assert!(matches!(
        stdout_rx.recv().await,
        Err(broadcast::error::RecvError::Closed)
    ));
    assert!(!checked.owner.has_retained_process_handle());
    assert_eq!(REGISTRY.active_count().unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_missing_io_join_or_helper_debt_blocks_checked_owner_finalize_and_latches_sticky_unknown()
{
    use std::io::Write;
    use std::sync::mpsc as std_mpsc;

    let exe = std::env::current_exe().unwrap();
    let sha: [u8; 32] = Sha256::digest(std::fs::read(&exe).unwrap()).into();
    let artifact = Arc::new(MatchedRunnerArtifact::acquire(&exe, sha).unwrap());
    let (proc_handle, pid, mut child_stdin) = spawn_controllable_runner_stub();
    let identity = OriginalProcessIdentity::from_live_process(&proc_handle, pid).unwrap();

    // Gate 1: Missing host reader native thread signal blocks finalize and latches sticky Unknown.
    let owner = CheckedRunnerOwner::new_scoped_for_actor(
        Arc::clone(&artifact),
        ControlHello {
            nonce: [9; 32],
            schema: CHECKED_CONTROL_SCHEMA,
        },
        RunnerControlCapabilities {
            close_stdin: true,
            resize: true,
        },
        proc_handle,
        identity,
    );
    owner.record_protocol_exit(0);

    let (reader_release_tx, reader_release_rx) = std_mpsc::channel::<()>();
    let (writer_release_tx, writer_release_rx) = std_mpsc::channel::<()>();
    let (input_release_tx, input_release_rx) = oneshot::channel::<()>();

    let reader_owner = Arc::clone(&owner);
    let mut admission = owner.reserve_runtime_workers().expect("reserve before worker effects");
    let reader_handle = std::thread::spawn(move || {
        let _ = reader_release_rx.recv();
        reader_owner.record_reader_physical_eof();
        ReaderWorkerOutcome {
            protocol_exit: Some(0),
            physical_eof: true,
            clean: true,
        }
    });
    let writer_handle = std::thread::spawn(move || {
        let _ = writer_release_rx.recv();
        WriterWorkerOutcome {
            clean: true,
            termination_sent: false,
        }
    });
    let input_handle = tokio::spawn(async move {
        let _ = input_release_rx.await;
        InputWorkerOutcome { clean: true }
    });

    admission.install_runtime_workers(reader_handle, writer_handle, input_handle, Box::new(|| {}));
    drop(admission);

    // Reader is still running -> finalize blocks with WouldBlock and latches sticky Unknown.
    let err_reader = owner
        .finalize(Instant::now() + Duration::from_millis(25))
        .expect_err("missing reader join must block finalize");
    assert_eq!(err_reader.kind(), io::ErrorKind::WouldBlock);
    assert!(owner.is_no_replay_sealed());
    assert!(owner.is_sticky_unknown());

    // Release reader; now input task is still pending -> finalize blocks with WouldBlock.
    reader_release_tx.send(()).unwrap();
    let err_input = owner
        .finalize(Instant::now() + Duration::from_millis(25))
        .expect_err("missing async input join must block finalize");
    assert_eq!(err_input.kind(), io::ErrorKind::WouldBlock);

    // Release input task; now blocking writer std thread is still running -> finalize blocks with WouldBlock.
    input_release_tx.send(()).unwrap();
    let err_writer = owner
        .finalize_async(Instant::now() + Duration::from_millis(25))
        .await
        .expect_err("missing blocking pipe writer join must block finalize");
    assert_eq!(err_writer.kind(), io::ErrorKind::WouldBlock);

    // Release blocking writer; now native runner process is still running -> finalize blocks with WouldBlock.
    writer_release_tx.send(()).unwrap();
    let err_process = owner
        .finalize_async(Instant::now() + Duration::from_millis(25))
        .await
        .expect_err("missing native runner process exit must block finalize");
    assert_eq!(err_process.kind(), io::ErrorKind::WouldBlock);

    // Release native runner process -> late physical reap succeeds but preserves sticky Unknown/noReplay.
    let _ = child_stdin.write_all(b"\r\n");
    drop(child_stdin);
    let outcome = owner
        .finalize_async(Instant::now() + Duration::from_secs(2))
        .await
        .expect("all IO workers joined and native runner exited");
    assert!(matches!(
        outcome,
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
async fn reader_spawn_failure_anchors_writer_and_input_handles_in_owner_and_reaps_them() {
    let exe = std::env::current_exe().unwrap();
    let sha: [u8; 32] = Sha256::digest(std::fs::read(&exe).unwrap()).into();
    let artifact = Arc::new(MatchedRunnerArtifact::acquire(&exe, sha).unwrap());
    let hello = ControlHello {
        nonce: [11; 32],
        schema: CHECKED_CONTROL_SCHEMA,
    };
    let capabilities = RunnerControlCapabilities {
        close_stdin: true,
        resize: true,
    };
    let owner = scoped_exited_process_owner(Arc::clone(&artifact), hello.clone(), capabilities)
        .expect("create scoped owner");
    let input_transport = tempfile::tempfile().unwrap();
    let output_transport = tempfile::tempfile().unwrap();
    let (_writer_tx, input_rx) = mpsc::channel(1);
    let (stdout_tx, _stdout_rx) = broadcast::channel(1);
    let (exit_tx, mut exit_rx) = oneshot::channel();

    let err = match start_checked_runner_with_owner_and_spawners(
        input_transport,
        output_transport,
        hello,
        capabilities,
        artifact,
        Arc::clone(&owner),
        input_rx,
        false,
        stdout_tx,
        None,
        exit_tx,
        |f| Ok(std::thread::spawn(f)),
        |_f| {
            Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "simulated reader thread spawn failure",
            ))
        },
    ) {
        Ok(_) => panic!("reader spawn failure must fail start_checked_runner_with_owner_and_spawners"),
        Err(err) => err,
    };
    assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
    assert!(owner.is_no_replay_sealed());
    assert!(owner.is_sticky_unknown());
    assert!(
        matches!(exit_rx.try_recv(), Err(oneshot::error::TryRecvError::Closed)),
        "exit_tx must be dropped without sending when actor startup aborts"
    );

    // Both already-spawned originals (writer std thread and async input task) must be strongly
    // anchored in `owner` and joined cleanly during `finalize_async`.
    let outcome = owner
        .finalize_async(Instant::now() + Duration::from_secs(2))
        .await
        .expect("reap aborted partial actor startup owner");
    assert!(matches!(
        outcome,
        CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
            native_exit_code: Some(0),
            no_replay_sealed: true,
            release_eligible: false,
            ..
        }
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manual_concurrent_finalizer_and_automatic_process_driver_completion_both_observe_clean_release()
{
    use crate::checked_owner::{CheckedOwnerRegistry, reserve_in_registry};
    use crate::ipc_framed::{CHECKED_IPC_PROTOCOL_VERSION, ExitPayload, FramedMessage, Message};
    use std::io::Write;

    static REGISTRY: CheckedOwnerRegistry = CheckedOwnerRegistry::new();
    static CONNECT_OWNER: crate::owned_connect_worker::Owner =
        crate::owned_connect_worker::Owner::new();

    let exe = std::env::current_exe().unwrap();
    let sha: [u8; 32] = Sha256::digest(std::fs::read(&exe).unwrap()).into();
    let artifact = Arc::new(MatchedRunnerArtifact::acquire(&exe, sha).unwrap());
    let mut res = reserve_in_registry(
        &REGISTRY,
        Some(&CONNECT_OWNER),
        Arc::clone(&artifact),
        None,
        Duration::from_secs(5),
    )
    .expect("reserve startup");

    let (proc_handle, pid, mut child_stdin) = spawn_controllable_runner_stub();
    let (thread_handle, tid) = duplicate_current_thread();
    let (_pipe_in_read, pipe_in_write) = anonymous_pipe_pair();
    let (pipe_out_read, mut pipe_out_write) = anonymous_pipe_pair();
    let hello = ControlHello {
        nonce: [12; 32],
        schema: CHECKED_CONTROL_SCHEMA,
    };
    let capabilities = RunnerControlCapabilities {
        close_stdin: true,
        resize: true,
    };

    res.install_launched_process(
        proc_handle,
        pid,
        thread_handle,
        tid,
        &pipe_in_write,
        &pipe_out_read,
        hello.clone(),
    )
    .expect("install launched process");
    res.connect_pipe_step_with(&pipe_in_write, "pipe-in", None, |_| Ok(()))
        .expect("connect pipe-in");
    res.connect_pipe_step_with(&pipe_out_read, "pipe-out", None, |_| Ok(()))
        .expect("connect pipe-out");
    let lease = res
        .commit_transport_lease(capabilities)
        .expect("commit transport lease");
    let (owner_token, identity, thread_disposition) =
        lease.into_io_token().expect("into io token");
    let checked_io = CheckedRunnerIo {
        pipe_write: pipe_in_write,
        pipe_read: pipe_out_read,
        hello,
        capabilities,
        artifact,
        identity,
        thread_disposition,
        owner: owner_token,
    };

    let (writer_tx, input_rx) = mpsc::channel(1);
    let (stdout_tx, stdout_rx) = broadcast::channel(1);
    let (exit_tx, exit_rx) = oneshot::channel();
    let checked =
        start_checked_runner_with_io(checked_io, input_rx, false, stdout_tx, None, exit_tx)
            .expect("start_checked_runner_with_io");
    let startup_id = checked.owner.startup_id();

    let spawned = codex_utils_pty::spawn_from_driver_with_checked_controls(
        codex_utils_pty::ProcessDriver {
            writer_tx,
            stdout_rx,
            stderr_rx: None,
            exit_rx,
            terminator: None,
            writer_handle: None,
            resizer: None,
            tty: false,
        },
        Arc::clone(&checked.controls) as Arc<dyn codex_utils_pty::CheckedDriverControls>,
    );

    // Send protocol Message::Exit(13) and close pipe_out_write so wait_ready_to_finalize triggers
    // automatic ProcessDriver completion while the original runner OS process is still alive.
    write_frame(
        &mut pipe_out_write,
        &FramedMessage {
            version: CHECKED_IPC_PROTOCOL_VERSION,
            message: Message::Exit {
                payload: ExitPayload {
                    exit_code: 13,
                    timed_out: false,
                },
            },
        },
    )
    .expect("write protocol Exit frame");
    drop(pipe_out_write);

    // Contend a manual finalizer concurrently with automatic ProcessDriver completion while
    // the original child process exits after 35ms.
    let manual_checked = Arc::clone(&checked.controls);
    let manual_task = tokio::spawn(async move {
        manual_checked
            .finalize_async(Instant::now() + Duration::from_secs(2))
            .await
    });

    let release_child = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(35));
        let _ = child_stdin.write_all(b"\r\n");
        drop(child_stdin);
    });

    let manual_outcome = manual_task
        .await
        .expect("manual finalizer task")
        .expect("manual finalizer must succeed");
    release_child.join().expect("join child release helper");

    assert_eq!(
        manual_outcome,
        CheckedFinalizationOutcome::CleanRelease {
            startup_id,
            protocol_exit_code: 13,
            native_exit_code: 0,
            release_eligible: false,
        }
    );

    let auto_code = tokio::time::timeout(Duration::from_secs(2), spawned.exit_rx)
        .await
        .expect("automatic ProcessDriver completion must finish within original deadline")
        .expect("automatic ProcessDriver completion must publish exit code");
    assert_eq!(auto_code, 13);
    assert!(spawned.session.has_exited());
    assert_eq!(spawned.session.exit_code(), Some(13));
    assert!(!checked.owner.is_sticky_unknown());
    assert_eq!(REGISTRY.active_count().unwrap(), 0);
}





#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_abort_withdraws_observers_while_original_writer_still_holds_correlation_and_quota() {
    use std::io::Write;
    static REGISTRY: crate::checked_owner::CheckedOwnerRegistry =
        crate::checked_owner::CheckedOwnerRegistry::new();
    static CONNECT_OWNER: crate::owned_connect_worker::Owner =
        crate::owned_connect_worker::Owner::new();

    let exe = std::env::current_exe().unwrap();
    let sha: [u8; 32] = Sha256::digest(std::fs::read(&exe).unwrap()).into();
    let artifact = Arc::new(MatchedRunnerArtifact::acquire(&exe, sha).unwrap());
    let mut reservation = crate::checked_owner::reserve_in_registry(
        &REGISTRY, Some(&CONNECT_OWNER), Arc::clone(&artifact), None, Duration::from_secs(5),
    ).expect("reserve actual owner quota");
    let (process, pid, mut child_stdin) = spawn_controllable_runner_stub();
    let (thread, tid) = duplicate_current_thread();
    let (_input_peer, pipe_write) = anonymous_pipe_pair();
    let (pipe_read, _output_peer) = anonymous_pipe_pair();
    let hello = ControlHello { nonce: [14; 32], schema: CHECKED_CONTROL_SCHEMA };
    let capabilities = RunnerControlCapabilities { close_stdin: true, resize: true };
    reservation.install_launched_process(
        process, pid, thread, tid, &pipe_write, &pipe_read, hello.clone(),
    ).unwrap();
    reservation.connect_pipe_step_with(&pipe_write, "pipe-in", None, |_| Ok(())).unwrap();
    reservation.connect_pipe_step_with(&pipe_read, "pipe-out", None, |_| Ok(())).unwrap();
    let lease = reservation.commit_transport_lease(capabilities).unwrap();
    let (token, _, _) = lease.into_io_token().unwrap();
    let owner = token.adopt_into_actor().unwrap();
    let (_input_tx, input_rx) = mpsc::channel(1);
    let (stdout_tx, mut stdout_rx) = broadcast::channel(1);
    let (stderr_tx, mut stderr_rx) = broadcast::channel(1);
    let (exit_tx, mut exit_rx) = oneshot::channel();
    let (release_writer, wait_writer) = std::sync::mpsc::channel();
    let writer_completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let completed = Arc::clone(&writer_completed);
    let result = start_checked_runner_with_owner_and_spawners(
        pipe_write, pipe_read, hello, capabilities, artifact, Arc::clone(&owner),
        input_rx, false, stdout_tx, Some(stderr_tx), exit_tx,
        move |original| Ok(std::thread::spawn(move || {
            // Hold the SAME original writer closure and its Correlation reference.
            // Sender withdrawal must happen before this barrier is released.
            wait_writer.recv().expect("release original writer");
            let outcome = original();
            completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            outcome
        })),
        |_original| Err(io::Error::new(io::ErrorKind::WouldBlock, "reader spawn rejected")),
    );
    let error = match result { Ok(_) => panic!("startup must fail"), Err(error) => error };
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(error.to_string(), "reader spawn rejected");
    assert!(matches!(exit_rx.try_recv(), Err(oneshot::error::TryRecvError::Closed)));
    assert!(matches!(stdout_rx.try_recv(), Err(broadcast::error::TryRecvError::Closed)));
    assert!(matches!(stderr_rx.try_recv(), Err(broadcast::error::TryRecvError::Closed)));
    assert_eq!(writer_completed.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(owner.is_sticky_unknown() && owner.is_no_replay_sealed());
    let first_reason = owner.sticky_unknown_reason();
    assert!(!owner.reader_physical_eof_observed(), "no reader ever observed EOF");
    assert_eq!(REGISTRY.active_count().unwrap(), 1);
    assert!(owner.has_retained_process_handle());
    assert!(owner.reserve_runtime_workers().is_err(), "aborted owner cannot admit new workers");
    let pending = owner.finalize_async(Instant::now() + Duration::from_millis(25))
        .await.expect_err("held original writer must prevent physical release");
    assert_eq!(pending.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(REGISTRY.active_count().unwrap(), 1);
    assert!(owner.has_retained_process_handle());
    assert_eq!(owner.sticky_unknown_reason(), first_reason);

    release_writer.send(()).unwrap();
    child_stdin.write_all(b"\r\n").unwrap();
    drop(child_stdin);
    let reaped = owner.finalize_async(Instant::now() + Duration::from_secs(2)).await
        .expect("join the same original writer/input and observe actual native process exit");
    assert!(matches!(reaped, CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
        native_exit_code: Some(0), no_replay_sealed: true, release_eligible: false, ..
    }));
    assert_eq!(writer_completed.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(REGISTRY.active_count().unwrap(), 0);
    assert!(!owner.has_retained_process_handle());
    assert!(!owner.reader_physical_eof_observed());
    assert_eq!(owner.sticky_unknown_reason(), first_reason);
    assert!(owner.is_sticky_unknown() && owner.is_no_replay_sealed());
    assert!(matches!(exit_rx.try_recv(), Err(oneshot::error::TryRecvError::Closed)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_reader_disconnect_keeps_observers_unresolved_after_real_sticky_reap() {
    let exe = std::env::current_exe().unwrap();
    let sha: [u8; 32] = Sha256::digest(std::fs::read(&exe).unwrap()).into();
    let artifact = Arc::new(MatchedRunnerArtifact::acquire(&exe, sha).unwrap());
    let hello = ControlHello { nonce: [15; 32], schema: CHECKED_CONTROL_SCHEMA };
    let capabilities = RunnerControlCapabilities { close_stdin: false, resize: false };
    let owner = scoped_exited_process_owner(Arc::clone(&artifact), hello.clone(), capabilities).unwrap();
    let (_input_peer, pipe_write) = anonymous_pipe_pair();
    let (pipe_read, output_peer) = anonymous_pipe_pair();
    let (input_tx, input_rx) = mpsc::channel(1);
    let (stdout_tx, mut stdout_rx) = broadcast::channel(1);
    let (stderr_tx, mut stderr_rx) = broadcast::channel(1);
    let (exit_tx, mut exit_rx) = oneshot::channel();
    let checked = start_checked_runner_with_owner(
        pipe_write, pipe_read, hello, capabilities, artifact, Arc::clone(&owner),
        input_rx, false, stdout_tx, Some(stderr_tx), exit_tx,
    ).expect("commit and publish the actual actors before transport failure");
    // Actual kernel EOF after successful startup, with no protocol Exit frame.
    drop(output_peer);
    tokio::time::timeout(Duration::from_secs(2), checked.controls.wait_ready_to_finalize())
        .await.expect("original reader must finish after genuine pipe EOF");
    assert!(owner.reader_physical_eof_observed());
    assert!(owner.is_sticky_unknown() && owner.is_no_replay_sealed());
    let first_reason = owner.sticky_unknown_reason();
    assert!(checked.controls.lifecycle_failure().unwrap().borrow().is_some());
    assert!(checked.controls.state.lock().unwrap().exit_code.is_none());
    assert!(matches!(exit_rx.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
    assert!(matches!(stdout_rx.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
    assert!(matches!(stderr_rx.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
    drop(input_tx);
    tokio::time::timeout(Duration::from_secs(2), checked.wait_input_stopped()).await.unwrap();
    let reaped = checked.finalize_async(Instant::now() + Duration::from_secs(2)).await
        .expect("physically join original workers without claiming normal completion");
    assert!(matches!(reaped, CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
        native_exit_code: Some(0), no_replay_sealed: true, release_eligible: false, ..
    }));
    assert_eq!(owner.sticky_unknown_reason(), first_reason);
    assert!(owner.is_sticky_unknown() && owner.is_no_replay_sealed());
    assert!(!owner.has_retained_process_handle());
    // Even late physical reap cannot turn this published Unknown into success/EOF.
    assert!(matches!(exit_rx.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
    assert!(matches!(stdout_rx.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
    assert!(matches!(stderr_rx.try_recv(), Err(broadcast::error::TryRecvError::Empty)));
}

#[tokio::test]
async fn unpublished_startup_abort_recovers_poisoned_correlation_without_losing_first_failure() {
    let exe = std::env::current_exe().unwrap();
    let sha: [u8; 32] = Sha256::digest(std::fs::read(&exe).unwrap()).into();
    let artifact = Arc::new(MatchedRunnerArtifact::acquire(&exe, sha).unwrap());
    let (mut correlation, failure, mut exit_rx) = correlation();
    let owner = scoped_exited_process_owner(artifact, correlation.hello.clone(),
        RunnerControlCapabilities { close_stdin: false, resize: false }).unwrap();
    correlation.owner = Some(Arc::clone(&owner));
    let (stdout_tx, mut stdout_rx) = broadcast::channel(1);
    let (stderr_tx, mut stderr_rx) = broadcast::channel(1);
    correlation.stdout_tx = Some(stdout_tx);
    correlation.stderr_tx = Some(stderr_tx);
    correlation.fail();
    let first_failure = failure.borrow().clone();
    let first_reason = owner.sticky_unknown_reason();
    assert!(matches!(exit_rx.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
    let state = Arc::new(Mutex::new(correlation));
    let (writer_go_tx, writer_go_rx) = std::sync::mpsc::sync_channel(1);
    let (input_go_tx, input_go_rx) = oneshot::channel();
    let (reader_go_tx, reader_go_rx) = std::sync::mpsc::sync_channel(1);
    let (ready_tx, ready_rx) = watch::channel(false);
    let abort = RuntimeStartupAbortGuard {
        owner: Arc::clone(&owner), state: Arc::clone(&state),
        writer_go_tx: Some(writer_go_tx), input_go_tx: Some(input_go_tx),
        reader_go_tx: Some(reader_go_tx), ready_to_finalize_tx: ready_tx, committed: false,
    };
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = state.lock().unwrap();
        panic!("injected correlation unwind before actor commit");
    }));
    assert!(poisoned.is_err());
    drop(abort);
    assert!(!writer_go_rx.recv().unwrap());
    assert!(!input_go_rx.await.unwrap());
    assert!(!reader_go_rx.recv().unwrap());
    assert!(*ready_rx.borrow());
    assert!(matches!(exit_rx.try_recv(), Err(oneshot::error::TryRecvError::Closed)));
    assert!(matches!(stdout_rx.try_recv(), Err(broadcast::error::TryRecvError::Closed)));
    assert!(matches!(stderr_rx.try_recv(), Err(broadcast::error::TryRecvError::Closed)));
    assert_eq!(*failure.borrow(), first_failure);
    let retained = match state.lock() { Ok(state) => state, Err(poisoned) => poisoned.into_inner() };
    assert!(retained.exit_tx.is_none() && retained.stdout_tx.is_none() && retained.stderr_tx.is_none());
    assert!(retained.exit_code.is_none());
    assert!(retained.failed && *retained.terminal.borrow());
    assert_eq!(retained.close_done.borrow().as_ref().unwrap().status, ControlStatus::Unknown);
    drop(retained);
    assert_eq!(owner.sticky_unknown_reason(), first_reason);
    assert!(!owner.reader_physical_eof_observed());
    let reaped = owner.finalize_async(Instant::now() + Duration::from_secs(2)).await.unwrap();
    assert!(matches!(reaped, CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
        native_exit_code: Some(0), no_replay_sealed: true, release_eligible: false, ..
    }));
    assert_eq!(owner.sticky_unknown_reason(), first_reason);
    assert!(owner.is_sticky_unknown() && owner.is_no_replay_sealed());
}
