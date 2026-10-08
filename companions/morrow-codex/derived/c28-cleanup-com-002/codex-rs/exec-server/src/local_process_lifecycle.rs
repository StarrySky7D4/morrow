//! Checked driver evidence loss belongs to the original LocalProcess generation.
//! Failure is unsequenced and sticky; it never creates OS exit or output EOF.
use std::sync::Arc;

use codex_utils_pty::ExecCommandSession;
use tokio::sync::watch;

use super::Inner;
use super::ProcessEntry;
use super::RunningProcess;
use crate::ExecProcessEvent;
use crate::ProcessId;

const EVIDENCE_LOST: &str = "checked driver lifecycle evidence lost";

#[derive(Default)]
pub(super) struct LifecycleEvidence {
    receiver: Option<watch::Receiver<Option<String>>>,
    failed: bool,
}

impl LifecycleEvidence {
    pub(super) fn from_session(session: &ExecCommandSession) -> Self {
        #[cfg(windows)]
        let receiver = session.checked_driver_failure();
        #[cfg(not(windows))]
        let receiver = {
            let _ = session;
            None
        };
        Self {
            receiver,
            failed: false,
        }
    }

    pub(super) fn receiver(&self) -> Option<watch::Receiver<Option<String>>> {
        self.receiver.clone()
    }

    pub(super) fn failure(&self) -> Option<&'static str> {
        self.failed.then_some(EVIDENCE_LOST)
    }

    fn evidence_lost(&self) -> bool {
        self.receiver
            .as_ref()
            .is_some_and(|receiver| receiver.borrow().is_some())
    }

    fn source_ended(&self) -> bool {
        self.receiver
            .as_ref()
            .is_some_and(|receiver| receiver.has_changed().is_err())
    }
}

fn fail(process: &mut RunningProcess) {
    if !process.lifecycle.failed {
        process.lifecycle.failed = true;
        // Keep session/network/cleanup ownership and seq unchanged. Readers
        // and native observers see failure, never a fabricated terminal event.
        process
            .events
            .publish(ExecProcessEvent::Failed(EVIDENCE_LOST.into()));
        process.wake_tx.send_replace(process.next_seq);
        process.output_notify.notify_waiters();
    }
}

/// Synchronous fence used while holding the original process-map guard.
/// A stale callback must not even inspect or latch failure on a replacement.
pub(super) fn veto(process: &mut RunningProcess, expected: &watch::Sender<u64>) -> bool {
    if !process.wake_tx.same_channel(expected) {
        return false;
    }
    if process.lifecycle.evidence_lost() || (!process.closed && process.lifecycle.source_ended()) {
        fail(process);
    }
    !process.lifecycle.failed
}

pub(super) async fn lost_exit(id: &ProcessId, expected: &watch::Sender<u64>, inner: &Inner) {
    let mut processes = inner.processes.lock().await;
    if let Some(ProcessEntry::Running(process)) = processes.get_mut(id)
        && process.wake_tx.same_channel(expected)
        && !process.closed
    {
        // Even an unchecked cancelled exit receiver is not an OS witness.
        fail(process);
    }
}

pub(super) fn start_watcher(
    id: ProcessId,
    expected: watch::Sender<u64>,
    failure: Option<watch::Receiver<Option<String>>>,
    inner: Arc<Inner>,
) {
    let Some(mut failure) = failure else { return };
    tokio::spawn(async move {
        let mut wake = expected.subscribe();
        loop {
            {
                let mut processes = inner.processes.lock().await;
                let Some(ProcessEntry::Running(process)) = processes.get_mut(&id) else {
                    return;
                };
                if !veto(process, &expected) || process.closed {
                    return;
                }
            }
            // Initial Some is inspected above before the first changed wait.
            // Neither this task nor any await acquires a Core/owner/store lock.
            tokio::select! {
                changed = failure.changed() => {
                    if changed.is_err() {
                        lost_exit(&id, &expected, &inner).await;
                        return;
                    }
                }
                changed = wake.changed() => {
                    if changed.is_err() { return; }
                }
            }
        }
    });
}

#[cfg(test)]
#[path = "local_process_lifecycle_tests.rs"]
mod tests;
