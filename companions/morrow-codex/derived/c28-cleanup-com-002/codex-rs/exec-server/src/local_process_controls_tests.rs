//! Pure LocalProcess gate tests. No command, helper or sandbox is spawned.
use std::collections::VecDeque;
use std::sync::Arc;

use codex_sandboxing::SandboxType;
use codex_utils_pty::ProcessDriver;
use pretty_assertions::assert_eq;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::watch;

use super::LocalProcess;
use super::ProcessControlCapabilities;
use super::ProcessControlOutcome;
use super::ProcessEntry;
use crate::ProcessId;
use crate::local_process::AcceptedStdinWriteIds;
use crate::local_process::ProcessStart;
use crate::local_process::RunningProcess;
use crate::process::ExecProcessEventLog;
use crate::protocol::WriteParams;
use crate::protocol::WriteStatus;

// A generic driver has no checked OS controls. The channel below is only a
// test transport, so these assertions do not claim child EOF or sandbox proof.
async fn insert_generic_process(
    local: &LocalProcess,
    id: &ProcessId,
) -> (
    watch::Sender<u64>,
    mpsc::Receiver<Vec<u8>>,
    Arc<Mutex<AcceptedStdinWriteIds>>,
) {
    let (writer_tx, writer_rx) = mpsc::channel(4);
    let (_, stdout_rx) = tokio::sync::broadcast::channel(1);
    let (_, exit_rx) = oneshot::channel();
    let session = codex_utils_pty::spawn_from_driver(ProcessDriver {
        writer_tx,
        stdout_rx,
        stderr_rx: None,
        exit_rx,
        terminator: None,
        writer_handle: None,
        resizer: None,
        #[cfg(windows)]
        tty: false,
    })
    .session;
    let (wake, _) = watch::channel(0);
    let accepted = Arc::new(Mutex::new(AcceptedStdinWriteIds::default()));
    local.inner.processes.lock().await.insert(
        id.clone(),
        ProcessEntry::Running(Box::new(RunningProcess {
            session: Arc::new(session),
            lifecycle: crate::local_process::checked_lifecycle::LifecycleEvidence::default(),
            tty: false,
            pipe_stdin: true,
            accepted_stdin_write_ids: Arc::clone(&accepted),
            output: VecDeque::new(),
            retained_bytes: 0,
            next_seq: 1,
            exit_code: None,
            wake_tx: wake.clone(),
            events: ExecProcessEventLog::new(4, 1024),
            output_notify: Arc::new(Notify::new()),
            open_streams: 1,
            closed: false,
            metrics: None,
            termination_requested: false,
            sandbox: SandboxType::None,
            sandbox_denied: false,
            network_proxy_handle: None,
            network_policy_shutdown: None,
        })),
    );
    (wake, writer_rx, accepted)
}

#[tokio::test]
async fn same_id_different_generation_is_rejected_without_touching_live_process() {
    let local = LocalProcess::default();
    let id = ProcessId::from("reused-id");
    let (current, mut input, _) = insert_generic_process(&local, &id).await;
    let (stale, _) = watch::channel(0);
    assert_eq!(
        local
            .close_process_input_checked(&id, &stale)
            .await
            .unwrap(),
        ProcessControlOutcome::Rejected
    );
    assert_eq!(
        local
            .resize_process_checked(&id, &stale, 24, 80)
            .await
            .unwrap(),
        ProcessControlOutcome::Rejected
    );
    assert_eq!(
        local
            .checked_process_capabilities(&id, &stale)
            .await
            .unwrap(),
        ProcessControlCapabilities::default()
    );
    assert_eq!(
        local
            .close_process_input_checked(&id, &current)
            .await
            .unwrap(),
        ProcessControlOutcome::Unsupported
    );
    assert_eq!(
        local
            .exec_write(WriteParams {
                process_id: id,
                chunk: vec![1, 2].into(),
                write_id: "live-write".into()
            })
            .await
            .unwrap()
            .status,
        WriteStatus::Accepted
    );
    assert_eq!(input.recv().await.unwrap(), vec![1, 2]);
    local.shutdown().await;
}

#[tokio::test]
async fn closing_gate_rejects_new_bytes_and_preserves_accepted_write_receipt() {
    let local = LocalProcess::default();
    let id = ProcessId::from("closing-process");
    let (_, mut input, accepted) = insert_generic_process(&local, &id).await;
    let write = |write_id: &str| WriteParams {
        process_id: id.clone(),
        chunk: vec![0, 255].into(),
        write_id: write_id.into(),
    };
    assert_eq!(
        local.exec_write(write("accepted")).await.unwrap().status,
        WriteStatus::Accepted
    );
    assert_eq!(input.recv().await.unwrap(), vec![0, 255]);
    // Model the sticky transition after a checked close has been admitted;
    // only the lower-level actual-child fixture can prove physical EOF.
    accepted.lock().await.input_closing = true;
    assert_eq!(
        local.exec_write(write("new")).await.unwrap().status,
        WriteStatus::StdinClosed
    );
    assert_eq!(
        local.exec_write(write("accepted")).await.unwrap().status,
        WriteStatus::Accepted
    );
    assert!(matches!(
        input.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    local.shutdown().await;
}

#[tokio::test]
async fn unknown_and_starting_processes_never_accept_a_checked_control() {
    let local = LocalProcess::default();
    let id = ProcessId::from("not-started");
    let (wake, _receiver) = watch::channel(0);
    for starting in [false, true] {
        if starting {
            local
                .inner
                .processes
                .lock()
                .await
                .insert(id.clone(), ProcessEntry::Starting(Arc::new(ProcessStart)));
        }
        assert_eq!(
            local
                .checked_process_capabilities(&id, &wake)
                .await
                .unwrap(),
            ProcessControlCapabilities::default()
        );
        assert_eq!(
            local.close_process_input_checked(&id, &wake).await.unwrap(),
            ProcessControlOutcome::Rejected
        );
        assert_eq!(
            local
                .resize_process_checked(&id, &wake, 24, 80)
                .await
                .unwrap(),
            ProcessControlOutcome::Rejected
        );
    }
    local.shutdown().await;
}

#[tokio::test]
async fn invalid_dimensions_are_definite_pre_effect_rejections() {
    let local = LocalProcess::default();
    let id = ProcessId::from("no-process");
    let (wake, _receiver) = watch::channel(0);
    for (rows, cols) in [(0, 80), (24, 0), (4097, 80), (24, u16::MAX)] {
        assert_eq!(
            local
                .resize_process_checked(&id, &wake, rows, cols)
                .await
                .unwrap(),
            ProcessControlOutcome::Rejected
        );
    }
    assert!(local.inner.processes.lock().await.is_empty());
    local.shutdown().await;
}
