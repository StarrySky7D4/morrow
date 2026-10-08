use super::*;
use crate::MatchedRunnerArtifact;
use sha2::{Digest, Sha256};
use std::time::Duration;

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
            writer_handle: Some(checked.writer),
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
            next_id: 3,
            failed: false,
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
    let (mut state, failure, exit) = correlation();
    let pending = ControlRequest::new(1, state.hello.nonce, ControlOperation::CloseStdin, 0, 0);
    let applied = ControlRequest::new(2, state.hello.nonce, ControlOperation::Resize, 24, 80);
    let (pending_tx, pending_rx) = watch::channel(None);
    let result = ControlResult::new(applied.clone(), ControlStatus::Applied, None);
    let (applied_tx, applied_rx) = watch::channel(Some(result.clone()));
    state.records.insert(1, (pending, pending_tx));
    state.records.insert(2, (applied, applied_tx));
    state.exited(7);
    assert_eq!(exit.await.unwrap(), 7);
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
    let (exit_tx, exit_rx) = oneshot::channel();
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
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), exit_rx)
            .await
            .unwrap()
            .unwrap(),
        9
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
    tokio::time::timeout(Duration::from_secs(2), checked.writer)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        stdout_rx.recv().await,
        Err(broadcast::error::RecvError::Closed)
    ));
    drop(permit);
    assert!(writer_tx.send(vec![1]).await.is_err());
    // The framed fixture exercises demux/input actors; it is not an OS exit witness.
}
