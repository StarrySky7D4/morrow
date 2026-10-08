//! Pure lifecycle actors: no child/helper is spawned and no OS facts are claimed.
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use codex_sandboxing::SandboxType;
use codex_utils_pty::ProcessDriver;
use pretty_assertions::assert_eq;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::watch;

use super::*;
use crate::ExecProcessEventReceiver;
use crate::local_process::AcceptedStdinWriteIds;
use crate::local_process::LocalProcess;
use crate::local_process::finish_output_stream;
use crate::local_process::maybe_emit_closed;
use crate::local_process::stream_output;
use crate::local_process::watch_exit;
use crate::process::ExecProcessEventLog;
use crate::process_telemetry::ProcessTelemetry;
use crate::protocol::ReadParams;

struct Fixture {
    local: LocalProcess,
    id: ProcessId,
    wake: watch::Sender<u64>,
    failure: watch::Sender<Option<String>>,
    events: ExecProcessEventReceiver,
    _driver_exit: oneshot::Sender<i32>,
}

impl Fixture {
    async fn new(initial: Option<String>) -> Self {
        let local = LocalProcess::default();
        let id = ProcessId::from("lifecycle-test");
        let (writer, _input) = mpsc::channel(1);
        let (_output, output) = tokio::sync::broadcast::channel(1);
        let (driver_exit, exit) = oneshot::channel();
        let session = codex_utils_pty::spawn_from_driver(ProcessDriver {
            writer_tx: writer,
            stdout_rx: output,
            stderr_rx: None,
            exit_rx: exit,
            terminator: None,
            writer_handle: None,
            resizer: None,
            #[cfg(windows)]
            tty: false,
        })
        .session;
        let (wake, _) = watch::channel(0);
        let (failure, receiver) = watch::channel(initial);
        let log = ExecProcessEventLog::new(16, 4096);
        let events = log.subscribe();
        local.inner.processes.lock().await.insert(
            id.clone(),
            ProcessEntry::Running(Box::new(RunningProcess {
                session: Arc::new(session),
                lifecycle: LifecycleEvidence {
                    receiver: Some(receiver),
                    failed: false,
                },
                tty: false,
                pipe_stdin: true,
                accepted_stdin_write_ids: Arc::new(Mutex::new(AcceptedStdinWriteIds::default())),
                output: VecDeque::new(),
                retained_bytes: 0,
                next_seq: 1,
                exit_code: None,
                wake_tx: wake.clone(),
                events: log,
                output_notify: Arc::new(Notify::new()),
                open_streams: 2,
                closed: false,
                metrics: None,
                termination_requested: false,
                sandbox: SandboxType::None,
                sandbox_denied: false,
                network_proxy_handle: None,
                network_policy_shutdown: None,
            })),
        );
        Self {
            local,
            id,
            wake,
            failure,
            events,
            _driver_exit: driver_exit,
        }
    }

    fn watch(&self) {
        start_watcher(
            self.id.clone(),
            self.wake.clone(),
            Some(self.failure.subscribe()),
            Arc::clone(&self.local.inner),
        );
    }

    async fn failed_event(&mut self) {
        let event = tokio::time::timeout(Duration::from_millis(500), self.events.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event, ExecProcessEvent::Failed(EVIDENCE_LOST.into()));
    }

    async fn exit(&self, generation: &watch::Sender<u64>) {
        let (tx, rx) = oneshot::channel();
        tx.send(7).unwrap();
        tokio::time::timeout(
            Duration::from_millis(500),
            watch_exit(
                self.id.clone(),
                rx,
                generation.clone(),
                Arc::clone(&self.local.inner),
                Arc::new(Notify::new()),
                ProcessTelemetry::default(),
            ),
        )
        .await
        .unwrap();
    }

    async fn assert_unclean(&self) {
        let read = self
            .local
            .exec_read(ReadParams {
                process_id: self.id.clone(),
                after_seq: None,
                max_bytes: None,
                wait_ms: Some(0),
            })
            .await
            .unwrap();
        assert_eq!(
            (read.exited, read.exit_code, read.closed, read.failure),
            (false, None, false, Some(EVIDENCE_LOST.into()))
        );
        assert_eq!(read.next_seq, 1);
        assert!(
            self.local
                .inner
                .processes
                .lock()
                .await
                .contains_key(&self.id)
        );
    }
}

#[tokio::test]
async fn initial_failure_is_reported_before_any_terminal_observation() {
    let mut f = Fixture::new(Some("already failed".into())).await;
    f.watch();
    f.failed_event().await;
    f.exit(&f.wake).await;
    finish_output_stream(f.id.clone(), f.wake.clone(), Arc::clone(&f.local.inner)).await;
    finish_output_stream(f.id.clone(), f.wake.clone(), Arc::clone(&f.local.inner)).await;
    f.assert_unclean().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), f.events.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn independent_failure_watcher_needs_no_exit_or_stream_wakeup() {
    let mut f = Fixture::new(None).await;
    f.watch();
    tokio::task::yield_now().await;
    f.failure
        .send_replace(Some("transport disconnected".into()));
    f.failed_event().await;
    f.assert_unclean().await;
    // Repeated source signals/read fences cannot republish or allocate seq.
    f.failure.send_replace(Some("another failure".into()));
    f.assert_unclean().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), f.events.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn failure_latched_while_exit_waits_for_map_vetoes_exit_commit() {
    let mut f = Fixture::new(None).await;
    let map = f.local.inner.processes.lock().await;
    let (tx, rx) = oneshot::channel();
    tx.send(7).unwrap();
    let job = tokio::spawn(watch_exit(
        f.id.clone(),
        rx,
        f.wake.clone(),
        Arc::clone(&f.local.inner),
        Arc::new(Notify::new()),
        ProcessTelemetry::default(),
    ));
    tokio::task::yield_now().await;
    f.failure.send_replace(Some("lost before precommit".into()));
    drop(map);
    tokio::time::timeout(Duration::from_millis(500), job)
        .await
        .unwrap()
        .unwrap();
    f.failed_event().await;
    f.assert_unclean().await;
}

#[tokio::test]
async fn failure_before_last_stream_does_not_create_total_closed() {
    let mut f = Fixture::new(None).await;
    finish_output_stream(f.id.clone(), f.wake.clone(), Arc::clone(&f.local.inner)).await;
    f.failure
        .send_replace(Some("last stream evidence lost".into()));
    finish_output_stream(f.id.clone(), f.wake.clone(), Arc::clone(&f.local.inner)).await;
    f.exit(&f.wake).await;
    maybe_emit_closed(f.id.clone(), f.wake.clone(), Arc::clone(&f.local.inner)).await;
    f.failed_event().await;
    f.assert_unclean().await;
    let map = f.local.inner.processes.lock().await;
    let Some(ProcessEntry::Running(process)) = map.get(&f.id) else {
        panic!("original entry absent")
    };
    assert_eq!(process.open_streams, 1);
}

#[tokio::test]
async fn stale_generation_callbacks_and_watcher_cannot_poison_replacement() {
    let mut f = Fixture::new(None).await;
    let (stale_wake, _) = watch::channel(0);
    let (old_failure, old_receiver) = watch::channel(Some("old generation failed".into()));
    start_watcher(
        f.id.clone(),
        stale_wake.clone(),
        Some(old_receiver),
        Arc::clone(&f.local.inner),
    );
    f.exit(&stale_wake).await;
    let (output, receiver) = mpsc::channel(1);
    output.send(vec![1, 2, 3]).await.unwrap();
    drop(output);
    stream_output(
        f.id.clone(),
        crate::protocol::ExecOutputStream::Stdout,
        receiver,
        stale_wake.clone(),
        Arc::clone(&f.local.inner),
        Arc::new(Notify::new()),
    )
    .await;
    finish_output_stream(f.id.clone(), stale_wake.clone(), Arc::clone(&f.local.inner)).await;
    maybe_emit_closed(f.id.clone(), stale_wake.clone(), Arc::clone(&f.local.inner)).await;
    lost_exit(&f.id, &stale_wake, &f.local.inner).await;
    drop(old_failure);
    tokio::task::yield_now().await;
    {
        let map = f.local.inner.processes.lock().await;
        let Some(ProcessEntry::Running(process)) = map.get(&f.id) else {
            panic!("replacement absent")
        };
        assert_eq!(
            (
                process.lifecycle.failure(),
                process.exit_code,
                process.closed,
                process.open_streams,
                process.next_seq
            ),
            (None, None, false, 2, 1)
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(20), f.events.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn ended_failure_watch_without_os_exit_retains_original_entry() {
    let mut f = Fixture::new(None).await;
    f.watch();
    // Replace the fixture's sender only; the watched original now has no sender.
    let (unused, _) = watch::channel(None);
    drop(std::mem::replace(&mut f.failure, unused));
    f.failed_event().await;
    f.assert_unclean().await;
}

#[tokio::test]
async fn cancelled_exit_receiver_is_failure_not_synthetic_minus_one() {
    let mut f = Fixture::new(None).await;
    let (tx, rx) = oneshot::channel();
    drop(tx);
    tokio::time::timeout(
        Duration::from_millis(500),
        watch_exit(
            f.id.clone(),
            rx,
            f.wake.clone(),
            Arc::clone(&f.local.inner),
            Arc::new(Notify::new()),
            ProcessTelemetry::default(),
        ),
    )
    .await
    .unwrap();
    f.failed_event().await;
    f.assert_unclean().await;
}

#[tokio::test]
async fn total_closed_precommit_veto_preserves_exit_but_never_claims_clean() {
    let mut f = Fixture::new(None).await;
    f.exit(&f.wake).await;
    let event = tokio::time::timeout(Duration::from_millis(500), f.events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        ExecProcessEvent::Exited { exit_code: 7, .. }
    ));
    {
        // Model the interval after final stream accounting releases the map
        // and before maybe_emit_closed reacquires it; the actual fence is called.
        let mut map = f.local.inner.processes.lock().await;
        let Some(ProcessEntry::Running(process)) = map.get_mut(&f.id) else {
            panic!("original absent")
        };
        process.open_streams = 0;
    }
    f.failure
        .send_replace(Some("lost at total-close precommit".into()));
    maybe_emit_closed(f.id.clone(), f.wake.clone(), Arc::clone(&f.local.inner)).await;
    f.failed_event().await;
    let read = f
        .local
        .exec_read(ReadParams {
            process_id: f.id.clone(),
            after_seq: None,
            max_bytes: None,
            wait_ms: Some(0),
        })
        .await
        .unwrap();
    assert_eq!(
        (read.exit_code, read.closed, read.failure),
        (Some(7), false, Some(EVIDENCE_LOST.into()))
    );
    assert!(f.local.inner.processes.lock().await.contains_key(&f.id));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), f.events.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn confirmed_exit_with_no_lifecycle_failure_keeps_true_terminal_events() {
    let mut f = Fixture::new(None).await;
    f.watch();
    f.exit(&f.wake).await;
    finish_output_stream(f.id.clone(), f.wake.clone(), Arc::clone(&f.local.inner)).await;
    finish_output_stream(f.id.clone(), f.wake.clone(), Arc::clone(&f.local.inner)).await;
    let first = tokio::time::timeout(Duration::from_millis(500), f.events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        first,
        ExecProcessEvent::Exited {
            seq: 1,
            exit_code: 7,
            sandbox_denied: Some(false)
        }
    );
    let second = tokio::time::timeout(Duration::from_millis(500), f.events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second, ExecProcessEvent::Closed { seq: 2 });
}
