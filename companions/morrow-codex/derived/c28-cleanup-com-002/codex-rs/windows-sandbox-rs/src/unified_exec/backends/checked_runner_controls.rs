//! Checked elevated controls over the exact matched runner transport.
use crate::MatchedRunnerArtifact;
use crate::ipc_framed::*;
use codex_utils_pty::{
    CheckedControlCapabilities, CheckedDriverControls, TerminalSize, WindowsTtyInputNormalizer,
};
use std::collections::BTreeMap;
use std::fs::File;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

enum Outbound {
    Stdin(Vec<u8>),
    Control {
        op: ControlOperation,
        rows: u16,
        cols: u16,
        done: watch::Sender<Option<ControlResult>>,
    },
    Terminate,
}
struct Correlation {
    hello: ControlHello,
    _artifact: Option<Arc<MatchedRunnerArtifact>>,
    next_id: u64,
    failed: bool,
    terminal: watch::Sender<bool>,
    failure: watch::Sender<Option<String>>,
    records: BTreeMap<u64, (ControlRequest, watch::Sender<Option<ControlResult>>)>,
    // Loss of transport is not actual child exit or output EOF. Keep these
    // original observations unresolved under the same native execution quota.
    close_done: watch::Sender<Option<ControlResult>>,
    exit_tx: Option<oneshot::Sender<i32>>,
    stdout_tx: Option<broadcast::Sender<Vec<u8>>>,
    stderr_tx: Option<broadcast::Sender<Vec<u8>>>,
}
impl Correlation {
    fn publish_failure(&mut self) {
        self.failed = true;
        self.failure.send_if_modified(|failure| {
            if failure.is_some() {
                false
            } else {
                *failure = Some("matched runner lifecycle evidence lost".to_string());
                true
            }
        });
    }
    fn settle_unconfirmed(&mut self) {
        if self.close_done.borrow().is_none() {
            let observed_close = self.records.values().find_map(|(request, sender)| {
                (request.op == ControlOperation::CloseStdin)
                    .then(|| sender.borrow().clone())
                    .flatten()
            });
            self.close_done
                .send_replace(Some(observed_close.unwrap_or_else(|| {
                    ControlResult::new(
                        ControlRequest::new(
                            0,
                            self.hello.nonce,
                            ControlOperation::CloseStdin,
                            0,
                            0,
                        ),
                        ControlStatus::Unknown,
                        None,
                    )
                })));
        }
        for (request, sender) in self.records.values() {
            if sender.borrow().is_none() {
                sender.send_replace(Some(ControlResult::new(
                    request.clone(),
                    ControlStatus::Unknown,
                    None,
                )));
            }
        }
    }
    fn fail(&mut self) {
        // Publish lost lifecycle evidence before any control waiter is settled.
        // Retain actual exit/output observations; loss is never an OS terminal.
        self.publish_failure();
        self.settle_unconfirmed();
    }
    fn exited(&mut self, exit_code: i32) {
        // Real Exit has a separate sticky latch. It does not create Failed.
        // Outstanding control application is unproven, even though exit is real.
        self.terminal.send_replace(true);
        self.settle_unconfirmed();
        if let Some(exit) = self.exit_tx.take() {
            let _ = exit.send(exit_code);
        }
        self.stdout_tx.take();
        self.stderr_tx.take();
    }
    fn result(&mut self, result: ControlResult) -> bool {
        let Some((request, sender)) = self.records.get(&result.request.id) else {
            self.fail();
            return false;
        };
        if *request != result.request {
            self.fail();
            return false;
        }
        let prior = sender.borrow().clone();
        if let Some(old) = prior {
            if old != result {
                self.fail();
                return false;
            }
            return true;
        }
        let sender = sender.clone();
        let uncertain = result.status == ControlStatus::Unknown;
        if uncertain {
            // The exact remote result remains intact, but Failed must be visible
            // before its control watch can wake an observer.
            self.publish_failure();
        }
        sender.send_replace(Some(result.clone()));
        if result.request.op == ControlOperation::CloseStdin && self.close_done.borrow().is_none() {
            self.close_done.send_replace(Some(result));
        }
        if uncertain {
            self.settle_unconfirmed();
        }
        true
    }
}
struct FailureGuard {
    state: Arc<Mutex<Correlation>>,
    active: bool,
}
impl Drop for FailureGuard {
    fn drop(&mut self) {
        if self.active
            && let Ok(mut state) = self.state.lock()
            && !*state.terminal.borrow()
        {
            state.fail();
        }
    }
}
pub(crate) struct RunnerCheckedControls {
    capabilities: CheckedControlCapabilities,
    close_tx: watch::Sender<bool>,
    close_done: watch::Receiver<Option<ControlResult>>,
    outbound: mpsc::Sender<Outbound>,
    state: Arc<Mutex<Correlation>>,
    failure: watch::Receiver<Option<String>>,
    termination_requested: Arc<std::sync::atomic::AtomicBool>,
    _artifact: Arc<MatchedRunnerArtifact>,
}
async fn wait_result(mut receiver: watch::Receiver<Option<ControlResult>>) -> io::Result<()> {
    loop {
        if let Some(result) = receiver.borrow().clone() {
            return match result.status {
                ControlStatus::Applied => Ok(()),
                ControlStatus::Unsupported => Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "runner control unsupported",
                )),
                _ => Err(io::Error::other(format!(
                    "runner control did not apply: {:?} {:?}",
                    result.status, result.os_error
                ))),
            };
        }
        receiver
            .changed()
            .await
            .map_err(|_| io::Error::other("runner control outcome unknown"))?;
    }
}
impl CheckedDriverControls for RunnerCheckedControls {
    fn capabilities(&self) -> CheckedControlCapabilities {
        self.capabilities
    }
    fn lifecycle_failure(&self) -> Option<watch::Receiver<Option<String>>> {
        Some(self.failure.clone())
    }
    fn close_stdin(&self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>> {
        if !self.capabilities.close_stdin {
            return Box::pin(async {
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "runner has no actual stdin",
                ))
            });
        }
        self.close_tx.send_if_modified(|closing| {
            if *closing {
                false
            } else {
                *closing = true;
                true
            }
        });
        Box::pin(wait_result(self.close_done.clone()))
    }
    fn resize(
        &self,
        size: TerminalSize,
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async move {
            if !self.capabilities.resize {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "runner has no actual ConPTY",
                ));
            }
            if !(1..=4096).contains(&size.rows) || !(1..=4096).contains(&size.cols) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "runner invalid size",
                ));
            }
            if {
                let state = self
                    .state
                    .lock()
                    .map_err(|_| io::Error::other("runner correlation lock poisoned"))?;
                state.failed || *state.terminal.borrow()
            } {
                return Err(io::Error::other("runner control state is unknown"));
            }
            let (done, receiver) = watch::channel(None);
            self.outbound
                .send(Outbound::Control {
                    op: ControlOperation::Resize,
                    rows: size.rows,
                    cols: size.cols,
                    done,
                })
                .await
                .map_err(|_| io::Error::other("runner command admission unknown"))?;
            wait_result(receiver).await
        })
    }
}
impl RunnerCheckedControls {
    pub(crate) fn terminate(&self) {
        // Sticky and one-shot, including a full bounded data queue. The wake
        // marker can be full only while already admitted data wakes the actor.
        self.termination_requested
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = self.outbound.try_send(Outbound::Terminate);
    }
}
pub(crate) struct CheckedRunnerSpawn {
    pub controls: Arc<RunnerCheckedControls>,
    pub writer: tokio::task::JoinHandle<()>,
}
pub(crate) fn start_checked_runner(
    mut pipe_write: File,
    mut pipe_read: File,
    hello: ControlHello,
    capabilities: RunnerControlCapabilities,
    artifact: Arc<MatchedRunnerArtifact>,
    mut input: mpsc::Receiver<Vec<u8>>,
    normalize: bool,
    stdout_tx: broadcast::Sender<Vec<u8>>,
    stderr_tx: Option<broadcast::Sender<Vec<u8>>>,
    exit_tx: oneshot::Sender<i32>,
) -> CheckedRunnerSpawn {
    let (outbound, mut commands) = mpsc::channel(128);
    let (close_tx, mut close_rx) = watch::channel(false);
    let (close_done, close_receiver) = watch::channel(None);
    let (failure, failure_rx) = watch::channel(None);
    let (terminal, mut terminal_rx) = watch::channel(false);
    let state = Arc::new(Mutex::new(Correlation {
        hello,
        _artifact: Some(artifact.clone()),
        next_id: 1,
        failed: false,
        terminal,
        failure,
        records: BTreeMap::new(),
        close_done: close_done.clone(),
        exit_tx: Some(exit_tx),
        stdout_tx: Some(stdout_tx),
        stderr_tx,
    }));
    let termination_requested = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let controls = Arc::new(RunnerCheckedControls {
        capabilities: CheckedControlCapabilities {
            close_stdin: capabilities.close_stdin,
            resize: capabilities.resize,
        },
        close_tx,
        close_done: close_receiver,
        outbound: outbound.clone(),
        state: state.clone(),
        failure: failure_rx,
        termination_requested: termination_requested.clone(),
        _artifact: artifact,
    });
    // One transport actor assigns IDs in actual FIFO order and installs each
    // pending observation before any bytes are sent to the real runner.
    let writer_state = state.clone();
    let guard = FailureGuard {
        state: writer_state.clone(),
        active: true,
    };
    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        let mut termination_sent = false;
        while let Some(command) = commands.blocking_recv() {
            if writer_state
                .lock()
                .map_or(true, |state| *state.terminal.borrow())
            {
                // Queued control senders drop without an application claim.
                commands.close();
                break;
            }
            if termination_requested.load(std::sync::atomic::Ordering::SeqCst) && !termination_sent
            {
                termination_sent = true;
                if write_frame(
                    &mut pipe_write,
                    &FramedMessage {
                        version: CHECKED_IPC_PROTOCOL_VERSION,
                        message: Message::Terminate {
                            payload: EmptyPayload::default(),
                        },
                    },
                )
                .is_err()
                {
                    break;
                }
            }
            let message = match command {
                Outbound::Stdin(bytes) => Message::Stdin {
                    payload: StdinPayload {
                        data_b64: encode_bytes(&bytes),
                    },
                },
                Outbound::Terminate => continue,
                Outbound::Control {
                    op,
                    rows,
                    cols,
                    done,
                } => {
                    let mut state = match writer_state.lock() {
                        Ok(state) => state,
                        Err(_) => break,
                    };
                    if state.failed
                        || *state.terminal.borrow()
                        || state.records.len() >= MAX_CONTROL_IDS
                    {
                        let request =
                            ControlRequest::new(state.next_id, state.hello.nonce, op, rows, cols);
                        done.send_replace(Some(ControlResult::new(
                            request,
                            ControlStatus::Unknown,
                            None,
                        )));
                        continue;
                    }
                    let id = state.next_id;
                    state.next_id += 1;
                    let request = ControlRequest::new(id, state.hello.nonce, op, rows, cols);
                    state.records.insert(id, (request.clone(), done));
                    Message::ControlRequest { payload: request }
                }
            };
            if write_frame(
                &mut pipe_write,
                &FramedMessage {
                    version: CHECKED_IPC_PROTOCOL_VERSION,
                    message,
                },
            )
            .is_err()
            {
                break;
            }
        }
    });
    // Input receiver closure/drain is owned by this original actor, not by
    // dropping one Sender. Existing permits must finish before remote close.
    let writer_outbound = outbound.clone();
    let writer_state = state.clone();
    let writer_close_done = close_done.clone();
    let writer = tokio::spawn(async move {
        let mut guard = FailureGuard {
            state: writer_state,
            active: true,
        };
        let mut closing = false;
        let mut normalizer = WindowsTtyInputNormalizer::default();
        loop {
            if *terminal_rx.borrow() {
                input.close();
                guard.active = false;
                return;
            }
            if *close_rx.borrow() && !closing {
                input.close();
                closing = true;
            }
            let bytes = tokio::select! { biased;
                changed = terminal_rx.changed() => {
                    if changed.is_err() || *terminal_rx.borrow() {
                        // Actual child exit cancels drain: outstanding permits
                        // cannot deliver input to a live child anymore.
                        input.close();
                        guard.active = false;
                        return;
                    }
                    continue;
                }
                changed = close_rx.changed(), if !closing => {
                    if changed.is_err() { input.close(); }
                    closing = true; input.close(); continue;
                }
                bytes = input.recv() => bytes,
            };
            let Some(bytes) = bytes else {
                break;
            };
            let bytes = if normalize {
                normalizer.normalize(&bytes)
            } else {
                bytes
            };
            tokio::select! { biased;
                _ = terminal_rx.changed() => {
                    input.close();
                    guard.active = false;
                    return;
                }
                sent = writer_outbound.send(Outbound::Stdin(bytes)) => {
                    if sent.is_err() { return; }
                }
            }
        }
        if capabilities.close_stdin {
            let (done, receiver) = watch::channel(None);
            let admitted = tokio::select! { biased;
                _ = terminal_rx.changed() => {
                    guard.active = false;
                    return;
                }
                sent = writer_outbound.send(Outbound::Control {
                    op: ControlOperation::CloseStdin, rows: 0, cols: 0, done,
                }) => sent.is_ok(),
            };
            if admitted {
                let mut receiver = receiver;
                loop {
                    if let Some(result) = receiver.borrow().clone() {
                        guard.active = result.status != ControlStatus::Applied;
                        writer_close_done.send_if_modified(|prior| {
                            if prior.is_some() {
                                false
                            } else {
                                *prior = Some(result.clone());
                                true
                            }
                        });
                        break;
                    }
                    if receiver.changed().await.is_err() {
                        break;
                    }
                }
            }
        } else {
            // SpawnReady proves there is no owned stdin handle. Natural input
            // closure in this mode does not lose child/output lifecycle evidence.
            guard.active = false;
        }
    });
    let reader_outbound = outbound;
    let reader_state = state.clone();
    let guard = FailureGuard {
        state: reader_state.clone(),
        active: true,
    };
    std::thread::spawn(move || {
        let mut guard = guard;
        while let Ok(Some(frame)) = read_frame(&mut pipe_read) {
            if frame.version != CHECKED_IPC_PROTOCOL_VERSION {
                break;
            }
            let mut state = match reader_state.lock() {
                Ok(state) => state,
                Err(_) => break,
            };
            match frame.message {
                Message::ControlResult { payload } => {
                    if !state.result(payload) {
                        break;
                    }
                }
                Message::Output { payload } => {
                    let Ok(bytes) = decode_bytes(&payload.data_b64) else {
                        break;
                    };
                    let output = match payload.stream {
                        OutputStream::Stdout => state.stdout_tx.as_ref(),
                        OutputStream::Stderr => {
                            state.stderr_tx.as_ref().or(state.stdout_tx.as_ref())
                        }
                    };
                    if let Some(output) = output {
                        let _ = output.send(bytes);
                    }
                }
                Message::Exit { payload } => {
                    // Runner joins real output readers before its actual Exit.
                    state.exited(payload.exit_code);
                    // Wake a transport actor parked on an otherwise empty queue.
                    // A full queue already provides the wake, without replay.
                    let _ = reader_outbound.try_send(Outbound::Terminate);
                    guard.active = false;
                    break;
                }
                _ => break,
            }
        }
    });
    CheckedRunnerSpawn { controls, writer }
}

#[cfg(test)]
#[path = "checked_runner_controls_tests.rs"]
mod tests;
