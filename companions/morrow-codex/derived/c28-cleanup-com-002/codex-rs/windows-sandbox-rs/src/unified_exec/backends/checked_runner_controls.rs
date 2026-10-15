//! Checked elevated controls over the exact matched runner transport.
use crate::MatchedRunnerArtifact;
use crate::checked_owner::{
    CheckedFinalizationOutcome, CheckedRunnerIo, CheckedRunnerOwner, InputWorkerOutcome,
    OriginalProcessIdentity, ReaderWorkerOutcome, StickyUnknownReason, WriterWorkerOutcome,
};
use crate::ipc_framed::*;
use codex_utils_pty::{
    CheckedControlCapabilities, CheckedDriverControls, CheckedOwnerFinalization, TerminalSize,
    WindowsTtyInputNormalizer,
};
use std::collections::BTreeMap;
use std::fs::File;
use std::future::Future;
use std::io;
use std::os::windows::io::OwnedHandle;
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle as StdJoinHandle;
use std::time::Instant;
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
    FinalizeWake,
}
struct Correlation {
    hello: ControlHello,
    _artifact: Option<Arc<MatchedRunnerArtifact>>,
    owner: Option<Arc<CheckedRunnerOwner>>,
    next_id: u64,
    failed: bool,
    exit_code: Option<i32>,
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
        if let Some(ref owner) = self.owner {
            owner.record_runtime_lifecycle_failure();
        }
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
        // Protocol Exit is business data + input retirement notification only.
        // It wakes the input/writer actors to close the real input pipe without waiting
        // for runner native exit, while keeping stdout_tx, stderr_tx, and exit_tx open
        // until physical pipe EOF, original IO joins, and original native process exit
        // complete with CleanRelease in finalize_checked_owner.
        if let Some(ref owner) = self.owner {
            owner.record_protocol_exit(exit_code);
        }
        self.exit_code = Some(exit_code);
        self.terminal.send_replace(true);
        self.settle_unconfirmed();
    }
    fn complete_native_lifecycle(&mut self, exit_code: i32) {
        let code = self.exit_code.unwrap_or(exit_code);
        self.stdout_tx.take();
        self.stderr_tx.take();
        if let Some(exit) = self.exit_tx.take() {
            let _ = exit.send(code);
        }
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
    ready_to_finalize: watch::Receiver<bool>,
    input_done: watch::Receiver<bool>,
    termination_requested: Arc<std::sync::atomic::AtomicBool>,
    owner: Arc<CheckedRunnerOwner>,
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
    fn runner_observation(&self) -> codex_utils_pty::RunnerObservationReader {
        self.owner.runner_observation()
    }
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
    fn wait_ready_to_finalize(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let mut rx = self.ready_to_finalize.clone();
        let mut terminal=if self.native_close_enabled() {
            match self.state.lock(){Ok(state)=>Some(state.terminal.subscribe()),Err(_)=>{
                self.owner.record_runtime_lifecycle_failure();return Box::pin(async {});
            }}
        }else{None};
        Box::pin(async move {
            while !*rx.borrow() {
                if let Some(terminal)=terminal.as_mut(){
                    // A v8 Exit starts bounded ORIGINAL finalization even if its
                    // writer or receipt later stalls. It is not an EOF claim.
                    if *terminal.borrow(){break;}
                    tokio::select!{changed=rx.changed()=>{if changed.is_err(){break;}},
                        changed=terminal.changed()=>{if changed.is_err(){break;}}}
                    continue;
                }
                if rx.changed().await.is_err() {
                    break;
                }
            }
        })
    }
    fn finalize_checked_owner(
        &self,
        wait_until: Instant,
    ) -> Pin<Box<dyn Future<Output = io::Result<CheckedOwnerFinalization>> + Send + '_>> {
        Box::pin(async move {
            let outcome = self.finalize_async(wait_until).await?;
            Ok(match outcome {
                CheckedFinalizationOutcome::HostNativeClosedOuterScopePending{..}=>return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,"v8 host native closed; original outer keeper/quota/factory retirement pending")),
                CheckedFinalizationOutcome::CleanRelease {
                    protocol_exit_code, ..
                } => CheckedOwnerFinalization::CleanRelease {
                    exit_code: protocol_exit_code,
                },
                CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
                    native_exit_code,
                    ..
                } => CheckedOwnerFinalization::PhysicallyReapedWithStickyUnknown {
                    exit_code: native_exit_code.map(|code| code as i32),
                },
            })
        })
    }
}
impl RunnerCheckedControls {
    pub(crate) fn native_close_enabled(&self)->bool {self.owner.native_close_runtime().is_some()}
    pub(crate) fn terminate(&self) {
        // Sticky and one-shot, including a full bounded data queue. The wake
        // marker can be full only while already admitted data wakes the actor.
        self.termination_requested
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = self.outbound.try_send(Outbound::Terminate);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn owner(&self) -> &Arc<CheckedRunnerOwner> {
        &self.owner
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn wait_input_stopped(&self) {
        let mut rx = self.input_done.clone();
        while !*rx.borrow() {
            if rx.changed().await.is_err() {
                break;
            }
        }
        self.owner.try_poll_input_worker_now();
    }

    fn apply_finalization_outcome(
        &self,
        result: io::Result<CheckedFinalizationOutcome>,
    ) -> io::Result<CheckedFinalizationOutcome> {
        match result {
            Ok(outcome @ CheckedFinalizationOutcome::HostNativeClosedOuterScopePending{..})=>Ok(outcome),
            Ok(
                ref outcome @ CheckedFinalizationOutcome::CleanRelease {
                    protocol_exit_code, ..
                },
            ) => {
                if let Ok(mut state) = self.state.lock() {
                    state.complete_native_lifecycle(protocol_exit_code);
                }
                Ok(outcome.clone())
            }
            Ok(ref outcome @ CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown { .. }) => {
                if let Ok(mut state) = self.state.lock() {
                    state.fail();
                }
                Ok(outcome.clone())
            }
            Err(err) => {
                if self.owner.is_sticky_unknown() || self.owner.is_no_replay_sealed() {
                    if let Ok(mut state) = self.state.lock() {
                        state.fail();
                    }
                }
                Err(err)
            }
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn finalize(&self, wait_until: Instant) -> io::Result<CheckedFinalizationOutcome> {
        let res = self.owner.finalize(wait_until);
        self.apply_finalization_outcome(res)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn finalize_async(
        &self,
        wait_until: Instant,
    ) -> io::Result<CheckedFinalizationOutcome> {
        let res = self.owner.finalize_async(wait_until).await;
        self.apply_finalization_outcome(res)
    }
}
impl codex_utils_pty::NativeCloseDriver for RunnerCheckedControls {
    fn native_phase(&self,deadline:Instant)->Pin<Box<dyn Future<Output=io::Result<i32>>+Send+'_>> {
        Box::pin(async move {
            let deadline=deadline.min(self.owner.native_close_runtime().ok_or_else(||io::Error::other("original v8 runtime absent"))?.terminal_deadline()?);
            match self.finalize_async(deadline).await? {
                CheckedFinalizationOutcome::HostNativeClosedOuterScopePending{protocol_exit_code,..}=>{
                    // Publish logical business completion only after the genuine
                    // native phase. This grants no outer/factory cleanup right.
                    self.state.lock().map_err(|_|io::Error::other("original correlation poisoned"))?
                        .complete_native_lifecycle(protocol_exit_code);
                    Ok(protocol_exit_code)
                },
                _=>Err(io::Error::other("original v8 native phase not clean")),
            }
        })
    }
    fn take_native_completion(&self,deadline:Instant)->Pin<Box<dyn Future<Output=io::Result<Box<dyn std::any::Any+Send>>>+Send+'_>> {
        Box::pin(async move {
            let completion=self.owner.take_native_host_completion(deadline)?;
            Ok(Box::new(completion) as Box<dyn std::any::Any+Send>)
        })
    }
    fn retain_outer_failure(&self){self.owner.record_runtime_lifecycle_failure();}
    fn retain_original_forwarders(&self,originals:Vec<tokio::task::JoinHandle<()>>){
        self.owner.retain_original_forwarders(originals);
    }
}
pub(crate) struct CheckedRunnerSpawn {
    pub controls: Arc<RunnerCheckedControls>,
    pub owner: Arc<CheckedRunnerOwner>,
}

impl CheckedRunnerSpawn {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn wait_input_stopped(&self) {
        self.controls.wait_input_stopped().await;
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn finalize(&self, wait_until: Instant) -> io::Result<CheckedFinalizationOutcome> {
        self.controls.finalize(wait_until)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn finalize_async(
        &self,
        wait_until: Instant,
    ) -> io::Result<CheckedFinalizationOutcome> {
        self.controls.finalize_async(wait_until).await
    }
}

fn scoped_exited_process_owner(
    artifact: Arc<MatchedRunnerArtifact>,
    hello: ControlHello,
    capabilities: RunnerControlCapabilities,
) -> io::Result<Arc<CheckedRunnerOwner>> {
    let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned());
    let mut child = Command::new(comspec)
        .args(["/C", "exit", "0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let pid = child.id();
    let _ = child.wait()?;
    let process: OwnedHandle = child.into();
    let identity = OriginalProcessIdentity::from_live_process(&process, pid)?;
    Ok(CheckedRunnerOwner::new_scoped_for_actor(
        artifact,
        hello,
        capabilities,
        process,
        identity,
    ))
}

pub(crate) fn start_checked_runner_with_io(
    checked_io: CheckedRunnerIo,
    input: mpsc::Receiver<Vec<u8>>,
    normalize: bool,
    stdout_tx: broadcast::Sender<Vec<u8>>,
    stderr_tx: Option<broadcast::Sender<Vec<u8>>>,
    exit_tx: oneshot::Sender<i32>,
) -> io::Result<CheckedRunnerSpawn> {
    checked_io.verify_original_process()?;
    let CheckedRunnerIo {
        pipe_write,
        pipe_read,
        hello,
        capabilities,
        artifact,
        identity: _,
        thread_disposition: _,
        owner,
    } = checked_io;
    let adopted_owner = owner.adopt_into_actor()?;
    start_checked_runner_with_owner(
        pipe_write,
        pipe_read,
        hello,
        capabilities,
        artifact,
        adopted_owner,
        input,
        normalize,
        stdout_tx,
        stderr_tx,
        exit_tx,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_checked_runner(
    pipe_write: File,
    pipe_read: File,
    hello: ControlHello,
    capabilities: RunnerControlCapabilities,
    artifact: Arc<MatchedRunnerArtifact>,
    input: mpsc::Receiver<Vec<u8>>,
    normalize: bool,
    stdout_tx: broadcast::Sender<Vec<u8>>,
    stderr_tx: Option<broadcast::Sender<Vec<u8>>>,
    exit_tx: oneshot::Sender<i32>,
) -> CheckedRunnerSpawn {
    let owner = match scoped_exited_process_owner(
        Arc::clone(&artifact),
        hello.clone(),
        capabilities,
    ) {
        Ok(owner) => owner,
        Err(_) => unreachable!("spawning short-lived scoped child failed in scoped runner"),
    };
    match start_checked_runner_with_owner(
        pipe_write,
        pipe_read,
        hello,
        capabilities,
        artifact,
        owner,
        input,
        normalize,
        stdout_tx,
        stderr_tx,
        exit_tx,
    ) {
        Ok(spawned) => spawned,
        Err(_) => unreachable!("checked runner worker spawn failed in scoped runner"),
    }
}

struct RuntimeStartupAbortGuard {
    owner: Arc<CheckedRunnerOwner>,
    state: Arc<Mutex<Correlation>>,
    writer_go_tx: Option<std::sync::mpsc::SyncSender<bool>>,
    input_go_tx: Option<oneshot::Sender<bool>>,
    reader_go_tx: Option<std::sync::mpsc::SyncSender<bool>>,
    ready_to_finalize_tx: watch::Sender<bool>,
    committed: bool,
}

impl Drop for RuntimeStartupAbortGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        {
            let mut state = match self.state.lock() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            state.fail();
            state.terminal.send_replace(true);
            // No actor was committed or published. Withdraw these observers even
            // while original workers retain Correlation; dropping a sender sends
            // no exit code and proves no native exit, output EOF, or clean release.
            // Established lifecycle failures still use Correlation::fail alone.
            state.exit_tx.take();
            state.stdout_tx.take();
            state.stderr_tx.take();
        }
        if let Some(tx) = self.writer_go_tx.take() {
            let _ = tx.send(false);
        }
        if let Some(tx) = self.input_go_tx.take() {
            let _ = tx.send(false);
        }
        if let Some(tx) = self.reader_go_tx.take() {
            let _ = tx.send(false);
        }
        self.owner
            .abort_partial_actor_startup(StickyUnknownReason::StartupUnwound);
        self.ready_to_finalize_tx.send_replace(true);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_checked_runner_with_owner(
    pipe_write: File,
    pipe_read: File,
    hello: ControlHello,
    capabilities: RunnerControlCapabilities,
    artifact: Arc<MatchedRunnerArtifact>,
    owner: Arc<CheckedRunnerOwner>,
    input: mpsc::Receiver<Vec<u8>>,
    normalize: bool,
    stdout_tx: broadcast::Sender<Vec<u8>>,
    stderr_tx: Option<broadcast::Sender<Vec<u8>>>,
    exit_tx: oneshot::Sender<i32>,
) -> io::Result<CheckedRunnerSpawn> {
    start_checked_runner_with_owner_and_spawners(
        pipe_write,
        pipe_read,
        hello,
        capabilities,
        artifact,
        owner,
        input,
        normalize,
        stdout_tx,
        stderr_tx,
        exit_tx,
        |f| {
            std::thread::Builder::new()
                .name("codex-checked-runner-writer".to_owned())
                .spawn(f)
        },
        |f| {
            std::thread::Builder::new()
                .name("codex-checked-runner-reader".to_owned())
                .spawn(f)
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_checked_runner_with_owner_and_spawners<WSpawn, RSpawn>(
    mut pipe_write: File,
    mut pipe_read: File,
    hello: ControlHello,
    capabilities: RunnerControlCapabilities,
    artifact: Arc<MatchedRunnerArtifact>,
    owner: Arc<CheckedRunnerOwner>,
    mut input: mpsc::Receiver<Vec<u8>>,
    normalize: bool,
    stdout_tx: broadcast::Sender<Vec<u8>>,
    stderr_tx: Option<broadcast::Sender<Vec<u8>>>,
    exit_tx: oneshot::Sender<i32>,
    spawn_writer: WSpawn,
    spawn_reader: RSpawn,
) -> io::Result<CheckedRunnerSpawn>
where
    WSpawn: FnOnce(
        Box<dyn FnOnce() -> WriterWorkerOutcome + Send + 'static>,
    ) -> io::Result<StdJoinHandle<WriterWorkerOutcome>>,
    RSpawn: FnOnce(
        Box<dyn FnOnce() -> ReaderWorkerOutcome + Send + 'static>,
    ) -> io::Result<StdJoinHandle<ReaderWorkerOutcome>>,
{
    let native_close_runtime=owner.native_close_runtime();
    if native_close_runtime.is_some() {owner.retire_native_startup_aliases()?;}
    let protocol_version=if native_close_runtime.is_some() {8}else{CHECKED_IPC_PROTOCOL_VERSION};
    let mut worker_admission = owner.reserve_runtime_workers()?;
    let (outbound, mut commands) = mpsc::channel(128);
    let (close_tx, mut close_rx) = watch::channel(false);
    let (close_done, close_receiver) = watch::channel(None);
    let (failure, failure_rx) = watch::channel(None);
    let (terminal, mut terminal_rx) = watch::channel(false);
    let (ready_to_finalize_tx, ready_to_finalize_rx) = watch::channel(false);
    let (input_done_tx, input_done_rx) = watch::channel(false);
    let state = Arc::new(Mutex::new(Correlation {
        hello,
        _artifact: Some(artifact.clone()),
        owner: Some(Arc::clone(&owner)),
        next_id: 1,
        failed: false,
        exit_code: None,
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
        ready_to_finalize: ready_to_finalize_rx,
        input_done: input_done_rx,
        termination_requested: termination_requested.clone(),
        owner: Arc::clone(&owner),
        _artifact: artifact,
    });

    let (writer_go_tx, writer_go_rx) = std::sync::mpsc::sync_channel::<bool>(1);
    let (input_go_tx, input_go_rx) = oneshot::channel::<bool>();
    let (reader_go_tx, reader_go_rx) = std::sync::mpsc::sync_channel::<bool>(1);
    let mut abort_guard = RuntimeStartupAbortGuard {
        owner: Arc::clone(&owner),
        state: Arc::clone(&state),
        writer_go_tx: Some(writer_go_tx),
        input_go_tx: Some(input_go_tx),
        reader_go_tx: Some(reader_go_tx),
        ready_to_finalize_tx: ready_to_finalize_tx.clone(),
        committed: false,
    };

    // 1. Dedicated std blocking pipe writer thread: spawn and immediately anchor its
    // original JoinHandle in CheckedRunnerOwner before creating the next worker.
    let writer_state = state.clone();
    let writer_owner = Arc::clone(&owner);
    let writer_runtime=native_close_runtime.clone();
    let writer_guard = FailureGuard {
        state: writer_state.clone(),
        active: true,
    };
    let wake_outbound = outbound.clone();
    let writer_wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let _ = wake_outbound.try_send(Outbound::FinalizeWake);
    });
    worker_admission.spawn_writer(|| spawn_writer(Box::new(move || -> WriterWorkerOutcome {
        let mut guard = writer_guard;
        if !matches!(writer_go_rx.recv(), Ok(true)) || writer_owner.is_no_replay_sealed() {
            commands.close();
            if writer_runtime.is_some() {let _=writer_owner.close_native_pipe(pipe_write,1);}else{drop(pipe_write);}
            guard.active = false;
            drop(guard);
            return WriterWorkerOutcome {
                clean: false,
                termination_sent: false,
            };
        }
        let mut termination_sent = false;
        let mut write_failed = false;
        while let Some(command) = commands.blocking_recv() {
            if writer_state
                .lock()
                .map_or(true, |state| *state.terminal.borrow())
            {
                // Queued control senders drop without an application claim.
                commands.close();
                guard.active = false;
                break;
            }
            if matches!(command, Outbound::FinalizeWake) {
                if writer_state
                    .lock()
                    .map_or(true, |state| state.failed || *state.terminal.borrow())
                {
                    commands.close();
                    break;
                }
                continue;
            }
            if termination_requested.load(std::sync::atomic::Ordering::SeqCst)
                && !termination_sent
            {
                termination_sent = true;
                if crate::elevated::native_close_runtime::write_runtime_message(
                    &mut pipe_write,
                    &FramedMessage {
                        version: protocol_version,
                        message: Message::Terminate {
                            payload: EmptyPayload::default(),
                        },
                    },
                    writer_runtime.as_deref(),
                )
                .is_err()
                {
                    write_failed = true;
                    break;
                }
            }
            if writer_runtime.is_some() {
                if let Outbound::Stdin(ref bytes)=command {
                    if bytes.len()>8192 {
                        for chunk in bytes.chunks(8192) {
                            if writer_state.lock().map_or(true,|s|*s.terminal.borrow()) {break;}
                            let frame=FramedMessage {version:8,message:Message::Stdin {payload:StdinPayload {data_b64:encode_bytes(chunk)}}};
                            if crate::elevated::native_close_runtime::write_runtime_message(&mut pipe_write,&frame,writer_runtime.as_deref()).is_err() {
                                write_failed=true;break;
                            }
                        }
                        if write_failed {break;}
                        continue;
                    }
                }
            }
            let message = match command {
                Outbound::Stdin(bytes) => Message::Stdin {
                    payload: StdinPayload {
                        data_b64: encode_bytes(&bytes),
                    },
                },
                Outbound::Terminate | Outbound::FinalizeWake => continue,
                Outbound::Control {
                    op,
                    rows,
                    cols,
                    done,
                } => {
                    let mut state = match writer_state.lock() {
                        Ok(state) => state,
                        Err(_) => {
                            write_failed = true;
                            break;
                        }
                    };
                    if state.failed
                        || *state.terminal.borrow()
                        || state.records.len() >= MAX_CONTROL_IDS
                    {
                        let request = ControlRequest::new(
                            state.next_id,
                            state.hello.nonce,
                            op,
                            rows,
                            cols,
                        );
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
            if crate::elevated::native_close_runtime::write_runtime_message(
                &mut pipe_write,
                &FramedMessage {
                    version: protocol_version,
                    message,
                },
                writer_runtime.as_deref(),
            )
            .is_err()
            {
                write_failed = true;
                break;
            }
        }
        // Close the real input pipe immediately when the writer loop finishes (including
        // on business Exit / input retirement) without waiting for runner native exit.
        if let Some(runtime)=writer_runtime.as_ref() {
            let terminal=writer_state.lock().is_ok_and(|s|!s.failed && *s.terminal.borrow());
            if terminal && !write_failed {
                // SAME retained writer, no new thread/parallel writer, no retry.
                // ACK confirms input-handshake only, not this worker's own join.
                if let Err(error)=runtime.write_terminal_close_ack(&mut pipe_write){
                    writer_owner.record_native_terminal_ack_failure(&error);write_failed=true;
                }
            }else{write_failed=true;}
            if writer_owner.close_native_pipe(pipe_write,1).is_err() {write_failed=true;}
        }else{drop(pipe_write);}
        if write_failed {writer_owner.record_runtime_lifecycle_failure();}
        let terminal_seen = writer_state
            .lock()
            .map_or(false, |state| *state.terminal.borrow());
        if terminal_seen {
            guard.active = false;
        }
        let clean = !write_failed && !guard.active;
        drop(guard);
        WriterWorkerOutcome {
            clean,
            termination_sent,
        }
    })), writer_wake)?;

    // 2. Original async input worker: spawn and immediately anchor its original
    // JoinHandle<InputWorkerOutcome> in CheckedRunnerOwner before spawning the reader.
    let writer_outbound = outbound.clone();
    let input_state = state.clone();
    let input_owner = Arc::clone(&owner);
    let writer_close_done = close_done.clone();
    worker_admission.spawn_input(|| tokio::spawn(async move {
        let mut guard = FailureGuard {
            state: input_state,
            active: true,
        };
        if !matches!(input_go_rx.await, Ok(true)) || input_owner.is_no_replay_sealed() {
            input.close();
            guard.active = false;
            drop(guard);
            input_done_tx.send_replace(true);
            return InputWorkerOutcome { clean: false };
        }
        let mut closing = false;
        let mut normalizer = WindowsTtyInputNormalizer::default();
        loop {
            if *terminal_rx.borrow() {
                input.close();
                guard.active = false;
                let _ = writer_outbound.try_send(Outbound::FinalizeWake);
                input_done_tx.send_replace(true);
                return InputWorkerOutcome { clean: true };
            }
            if *close_rx.borrow() && !closing {
                input.close();
                closing = true;
            }
            let bytes = tokio::select! { biased;
                changed = terminal_rx.changed() => {
                    if changed.is_err() || *terminal_rx.borrow() {
                        // Business Exit retires input drain and wakes the blocking writer
                        // so the real input pipe closes without waiting for native exit.
                        input.close();
                        guard.active = false;
                        let _ = writer_outbound.try_send(Outbound::FinalizeWake);
                        input_done_tx.send_replace(true);
                        return InputWorkerOutcome { clean: true };
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
                    let _ = writer_outbound.try_send(Outbound::FinalizeWake);
                    input_done_tx.send_replace(true);
                    return InputWorkerOutcome { clean: true };
                }
                sent = writer_outbound.send(Outbound::Stdin(bytes)) => {
                    if sent.is_err() {
                        let clean = !guard.active;
                        drop(guard);
                        input_done_tx.send_replace(true);
                        return InputWorkerOutcome { clean };
                    }
                }
            }
        }
        if capabilities.close_stdin {
            let (done, receiver) = watch::channel(None);
            let admitted = tokio::select! { biased;
                _ = terminal_rx.changed() => {
                    guard.active = false;
                    let _ = writer_outbound.try_send(Outbound::FinalizeWake);
                    input_done_tx.send_replace(true);
                    return InputWorkerOutcome { clean: true };
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
        let terminal_seen = *terminal_rx.borrow();
        if terminal_seen {
            guard.active = false;
        }
        let clean = !guard.active;
        drop(guard);
        input_done_tx.send_replace(true);
        InputWorkerOutcome { clean }
    }))?;

    // 3. Dedicated std host reader thread: spawn and immediately anchor its original
    // JoinHandle in CheckedRunnerOwner. If spawning fails, RuntimeStartupAbortGuard
    // aborts the go gates and retains the already-installed writer and input workers.
    let reader_outbound = outbound;
    let reader_state = state;
    let reader_owner = Arc::clone(&owner);
    let reader_runtime=native_close_runtime;
    let reader_ready_tx = ready_to_finalize_tx;
    let reader_guard = FailureGuard {
        state: reader_state.clone(),
        active: true,
    };
    worker_admission.spawn_reader(|| spawn_reader(Box::new(move || -> ReaderWorkerOutcome {
        let mut guard = reader_guard;
        if !matches!(reader_go_rx.recv(), Ok(true)) || reader_owner.is_no_replay_sealed() {
            if reader_runtime.is_some() {let _=reader_owner.close_native_pipe(pipe_read,0);}else{drop(pipe_read);}
            guard.active = false;
            drop(guard);
            reader_ready_tx.send_replace(true);
            return ReaderWorkerOutcome {
                protocol_exit: None,
                physical_eof: false,
                clean: false,
            };
        }
        let mut protocol_exit = None;
        let mut physical_eof = false;
        let mut clean = false;
        loop {
            let read=if let Some(runtime)=reader_runtime.as_ref() {
                match runtime.read_message(&mut pipe_read) {
                    Ok(Some(None))=>continue, // unique verified receipt; keep same reader to EOF
                    Ok(Some(Some(message)))=>Ok(Some(message)),Ok(None)=>Ok(None),Err(error)=>Err(error.into()),
                }
            }else{read_frame(&mut pipe_read)};
            match read {
                Ok(Some(frame)) => {
                    if protocol_exit.is_some() {
                        // No frames may follow protocol Exit before physical pipe EOF.
                        clean = false;
                        reader_owner.record_runtime_lifecycle_failure();
                        break;
                    }
                    if frame.version != protocol_version {
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
                            // Business Exit retires input admission and wakes the blocking
                            // writer to close the real input pipe, while the reader continues
                            // reading until genuine physical pipe EOF (Ok(None)).
                            protocol_exit = Some(payload.exit_code);
                            state.exited(payload.exit_code);
                            drop(state);
                            let _ = reader_outbound.try_send(Outbound::FinalizeWake);
                            guard.active = false;
                        }
                        _ => break,
                    }
                }
                Ok(None) => {
                    physical_eof = true;
                    reader_owner.record_reader_physical_eof();
                    clean = protocol_exit.is_some();
                    break;
                }
                Err(_) => {
                    if reader_runtime.as_ref().is_some_and(|r|r.physical_eof_seen()) {
                        physical_eof=true;reader_owner.record_reader_physical_eof();
                    }
                    guard.active=true;
                    reader_owner.record_runtime_lifecycle_failure();
                    clean = false;
                    break;
                }
            }
        }
        if reader_runtime.is_some() {
            if reader_owner.close_native_pipe(pipe_read,0).is_err() {clean=false;guard.active=true;}
        }else{drop(pipe_read);}
        drop(guard);
        reader_ready_tx.send_replace(true);
        ReaderWorkerOutcome {
            protocol_exit,
            physical_eof,
            clean,
        }
    })))?;
    drop(worker_admission);

    if owner.is_no_replay_sealed() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "checked runner owner sealed noReplay during worker startup",
        ));
    }

    if let Some(tx) = abort_guard.writer_go_tx.take() {
        let _ = tx.send(true);
    }
    if let Some(tx) = abort_guard.input_go_tx.take() {
        let _ = tx.send(true);
    }
    if let Some(tx) = abort_guard.reader_go_tx.take() {
        let _ = tx.send(true);
    }
    abort_guard.committed = true;

    Ok(CheckedRunnerSpawn { controls, owner })
}

#[cfg(test)]
#[path = "checked_runner_controls_tests.rs"]
mod tests;
