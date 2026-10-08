//! Real LocalExecProcess/RunningProcess actors; no OS or sandbox is spawned.
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use codex_sandboxing::SandboxType;
use codex_utils_pty::ProcessDriver;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::super::AcceptedStdinWriteIds;
use super::super::LocalExecProcess;
use super::super::LocalProcess;
use super::super::ProcessEntry;
use super::super::ProcessStart;
use super::super::RetainedOutputChunk;
use super::super::RunningProcess;
use super::super::checked_lifecycle::LifecycleEvidence;
use crate::ExecProcess;
use crate::ExecProcessEvent;
use crate::ProcessId;
use crate::process::ExecProcessEventLog;
use crate::protocol::ExecOutputStream;
use crate::protocol::ProcessSignal;
use crate::protocol::ReadParams;
use crate::protocol::TerminateParams;
use crate::protocol::WriteStatus;

struct Actor {
    object: Arc<dyn ExecProcess>,
    input: mpsc::Receiver<Vec<u8>>,
    kills: Arc<AtomicUsize>,
    accepted: Arc<Mutex<AcceptedStdinWriteIds>>,
    notify: Arc<Notify>,
    network_stop: CancellationToken,
    _output: broadcast::Sender<Vec<u8>>,
    _exit: oneshot::Sender<i32>,
}

async fn insert(local: &LocalProcess, id: &ProcessId) -> Actor {
    let (writer_tx, input) = mpsc::channel(2);
    let (output, stdout_rx) = broadcast::channel(2);
    let (exit, exit_rx) = oneshot::channel();
    let kills = Arc::new(AtomicUsize::new(0));
    let counted = kills.clone();
    let session = codex_utils_pty::spawn_from_driver(ProcessDriver {
        writer_tx,
        stdout_rx,
        stderr_rx: None,
        exit_rx,
        terminator: Some(Box::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        })),
        writer_handle: None,
        resizer: None,
        #[cfg(windows)]
        tty: false,
    })
    .session;
    let (wake, _) = watch::channel(0);
    let events = ExecProcessEventLog::new(4, 1024);
    let object = Arc::new(LocalExecProcess {
        process_id: id.clone(),
        backend: local.clone(),
        wake_tx: wake.clone(),
        events: events.clone(),
    });
    let accepted = Arc::new(Mutex::new(AcceptedStdinWriteIds::default()));
    let notify = Arc::new(Notify::new());
    let network_stop = CancellationToken::new();
    local.inner.processes.lock().await.insert(
        id.clone(),
        ProcessEntry::Running(Box::new(RunningProcess {
            session: Arc::new(session),
            lifecycle: LifecycleEvidence::default(),
            tty: false,
            pipe_stdin: true,
            accepted_stdin_write_ids: accepted.clone(),
            output: VecDeque::new(),
            retained_bytes: 0,
            next_seq: 1,
            exit_code: None,
            wake_tx: wake,
            events,
            output_notify: notify.clone(),
            open_streams: 1,
            closed: false,
            metrics: None,
            termination_requested: false,
            sandbox: SandboxType::None,
            sandbox_denied: false,
            network_proxy_handle: None,
            network_policy_shutdown: Some(network_stop.clone()),
        })),
    );
    Actor {
        object,
        input,
        kills,
        accepted,
        notify,
        network_stop,
        _output: output,
        _exit: exit,
    }
}

#[tokio::test]
async fn stale_four_methods_cannot_read_or_control_replacement() {
    let local = LocalProcess::default();
    let id = ProcessId::from("same-id");
    let old = insert(&local, &id).await;
    // Internal terminal-state fixture; this is not OS exit/EOF evidence.
    {
        let mut map = local.inner.processes.lock().await;
        let Some(ProcessEntry::Running(process)) = map.get_mut(&id) else {
            panic!("missing original")
        };
        process.exit_code = Some(42);
        process.closed = true;
        process.next_seq = 3;
        process.events.publish(ExecProcessEvent::Exited {
            seq: 1,
            exit_code: 42,
            sandbox_denied: Some(false),
        });
        process.events.publish(ExecProcessEvent::Closed { seq: 2 });
    }
    let original = old.object.read(None, None, Some(0)).await.unwrap();
    assert!(original.exited && original.closed);
    assert_eq!(original.exit_code, Some(42));
    let mut old_events = old.object.subscribe_events();
    let mut current = insert(&local, &id).await;
    {
        let mut map = local.inner.processes.lock().await;
        let Some(ProcessEntry::Running(process)) = map.get_mut(&id) else {
            panic!("missing current")
        };
        process.output.push_back(RetainedOutputChunk {
            seq: 1,
            stream: ExecOutputStream::Stdout,
            chunk: b"new-owner-output".to_vec(),
        });
        process.next_seq = 2;
    }
    assert!(old.object.read(None, None, Some(0)).await.is_err());
    assert!(old.object.write(vec![7]).await.is_err());
    assert!(old.object.signal(ProcessSignal::Interrupt).await.is_err());
    assert!(old.object.terminate().await.is_err());
    assert_eq!(current.kills.load(Ordering::SeqCst), 0);
    assert!(!current.network_stop.is_cancelled());
    assert!(current.accepted.lock().await.ids.is_empty());
    assert!(current.input.try_recv().is_err());
    let map = local.inner.processes.lock().await;
    let Some(ProcessEntry::Running(process)) = map.get(&id) else {
        panic!("missing replacement")
    };
    assert!(!process.termination_requested);
    drop(map);
    assert!(matches!(
        old_events.recv().await.unwrap(),
        ExecProcessEvent::Exited { exit_code: 42, .. }
    ));
    assert!(matches!(
        old_events.recv().await.unwrap(),
        ExecProcessEvent::Closed { seq: 2 }
    ));
    // Current-ID RPC remains unchanged, including reading current B output.
    let read = local
        .exec_read(ReadParams {
            process_id: id.clone(),
            after_seq: None,
            max_bytes: None,
            wait_ms: Some(0),
        })
        .await
        .unwrap();
    assert_eq!(read.chunks[0].chunk.0, b"new-owner-output");
    assert_eq!(
        current.object.write(vec![8]).await.unwrap().status,
        WriteStatus::Accepted
    );
    assert_eq!(current.input.recv().await.unwrap(), vec![8]);
    local
        .terminate_process(TerminateParams { process_id: id })
        .await
        .unwrap();
    assert_eq!(current.kills.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stale_terminate_never_removes_replacement_starting_entry() {
    let local = LocalProcess::default();
    let id = ProcessId::from("starting-replacement");
    let old = insert(&local, &id).await;
    let current = Arc::new(ProcessStart);
    local
        .inner
        .processes
        .lock()
        .await
        .insert(id.clone(), ProcessEntry::Starting(current.clone()));
    assert!(old.object.terminate().await.is_err());
    assert!(old.object.signal(ProcessSignal::Interrupt).await.is_err());
    assert!(old.object.write(vec![1]).await.is_err());
    assert!(old.object.read(None, None, Some(0)).await.is_err());
    assert!(
        matches!(local.inner.processes.lock().await.get(&id), Some(ProcessEntry::Starting(found)) if Arc::ptr_eq(found, &current))
    );
    // Old RPC's current-ID cancellation of Starting is explicitly preserved.
    assert!(
        local
            .terminate_process(TerminateParams {
                process_id: id.clone()
            })
            .await
            .unwrap()
            .running
    );
    assert!(!local.inner.processes.lock().await.contains_key(&id));
}

#[tokio::test]
async fn waiting_old_read_rechecks_generation_after_notification() {
    let local = LocalProcess::default();
    let id = ProcessId::from("read-wait");
    let old = insert(&local, &id).await;
    let mut waiting = Box::pin(old.object.read(None, None, Some(1000)));
    tokio::select! { biased;
        result = &mut waiting => panic!("old read must wait: {result:?}"),
        () = std::future::ready(()) => {}
    }
    let _current = insert(&local, &id).await;
    old.notify.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .is_err()
    );
}

#[tokio::test]
async fn admitted_old_write_keeps_original_writer_and_receipt_after_replacement() {
    let local = LocalProcess::default();
    let id = ProcessId::from("write-wait");
    let mut old = insert(&local, &id).await;
    let gate = old.accepted.lock().await;
    let mut waiting = Box::pin(old.object.write(vec![0, 255]));
    tokio::select! { biased;
        result = &mut waiting => panic!("original accepted-write gate must wait: {result:?}"),
        () = std::future::ready(()) => {}
    }
    // The pending write has selected old writer/cache under the map lock, then
    // released that lock and waited on the old accepted-write gate.
    let mut current = insert(&local, &id).await;
    drop(gate);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .unwrap()
            .status,
        WriteStatus::Accepted
    );
    assert_eq!(old.input.recv().await.unwrap(), vec![0, 255]);
    assert_eq!(old.accepted.lock().await.ids.len(), 1);
    assert!(current.accepted.lock().await.ids.is_empty());
    assert!(current.input.try_recv().is_err());
    assert_eq!(current.kills.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn same_generation_signal_and_terminate_use_the_original_session() {
    let local = LocalProcess::default();
    let id = ProcessId::from("live-object");
    let current = insert(&local, &id).await;
    #[cfg(windows)]
    {
        current
            .object
            .signal(ProcessSignal::Interrupt)
            .await
            .unwrap();
        assert_eq!(current.kills.load(Ordering::SeqCst), 1);
    }
    #[cfg(not(windows))]
    assert!(
        current
            .object
            .signal(ProcessSignal::Interrupt)
            .await
            .is_err()
    );
    let before = current.kills.load(Ordering::SeqCst);
    current.object.terminate().await.unwrap();
    assert_eq!(current.kills.load(Ordering::SeqCst), before + 1);
    assert!(current.network_stop.is_cancelled());
    let map = local.inner.processes.lock().await;
    assert!(
        matches!(map.get(&id), Some(ProcessEntry::Running(process)) if process.termination_requested)
    );
}
