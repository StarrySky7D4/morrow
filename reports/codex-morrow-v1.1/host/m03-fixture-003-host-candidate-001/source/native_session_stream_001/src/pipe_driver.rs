//! Dedicated owned-I/O thread. Never blocks control/Tokio workers on Pipe Drop/reap.
use crate::Result;
#[cfg(feature = "qualification-pipe-fault")]
use crate::qualification_pipe::{BoundPlan, CutFrame, PartialFrameWitness};
use crate::write_state::{FrameWrite, Outcome};
use morrow_native_pipe_win::{Kind, Pipe};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) struct EffectGate {
    pub revoked: bool,
    pub deadline: Instant,
    pub ordinal: u64,
    pub last_write: u64,
    pub issued_body_end: u64,
    pub network_pending: bool,
}
pub(crate) type Gate = Arc<Mutex<EffectGate>>;
pub(crate) struct TicketOwnership(Gate);
impl Drop for TicketOwnership {
    fn drop(&mut self) {
        if let Ok(mut g) = self.0.lock() {
            g.network_pending = false;
        }
    }
}
pub(crate) fn network_ticket(gate: &Gate) -> Result<(u64, TicketOwnership)> {
    let mut g = gate.lock().map_err(|_| "effect gate poisoned")?;
    if g.revoked || Instant::now() >= g.deadline || g.network_pending {
        return Err("network fence revoked/occupied".into());
    }
    g.ordinal = g.ordinal.checked_add(1).ok_or("effect ordinal exhausted")?;
    g.network_pending = true;
    Ok((g.ordinal, TicketOwnership(gate.clone())))
}
fn issue_write<T>(
    gate: &Gate,
    body_end: Option<u64>,
    action: impl FnOnce() -> Result<T>,
) -> Result<(u64, T)> {
    let mut g = gate.lock().map_err(|_| "effect gate poisoned")?;
    if g.revoked || Instant::now() >= g.deadline {
        return Err("write fence revoked/expired".into());
    }
    let ordinal = g.ordinal.checked_add(1).ok_or("effect ordinal exhausted")?;
    let value = action()?;
    g.ordinal = ordinal;
    g.last_write = ordinal;
    if let Some(end) = body_end {
        g.issued_body_end = g.issued_body_end.max(end);
    }
    Ok((ordinal, value))
}
pub(crate) enum Command {
    #[cfg(feature = "qualification-pipe-fault")]
    PrefixFrame {
        bytes: Vec<u8>,
        body_end: Option<u64>,
    },
    Read(usize),
    Write {
        bytes: Vec<u8>,
        body_end: Option<u64>,
    },
}
pub(crate) enum Event {
    #[cfg(feature = "qualification-pipe-fault")]
    QualificationObservation {
        event: &'static str,
        detail: Value,
    },
    OwnerObservation(Value),
    Created {
        inbound: u32,
        outbound: u32,
        dacl: String,
        noninherited: bool,
    },
    Connected {
        pid: u32,
    },
    Read(Vec<u8>),
    WriteIssued {
        id: u64,
        ordinal: u64,
        body_end: Option<u64>,
        bytes: usize,
        pending: bool,
    },
    WriteIncomplete {
        id: u64,
        body_end: Option<u64>,
    },
    WriteCompleted {
        body_end: Option<u64>,
        bytes: usize,
    },
    Reaped {
        kind: Kind,
        id: u64,
        bytes: usize,
        error: Option<u32>,
    },
    Error(String),
}
pub(crate) struct Closed {
    pub error: Option<String>,
}
pub(crate) struct Driver {
    tx: mpsc::SyncSender<Command>,
    rx: mpsc::Receiver<Event>,
    cancel: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<Closed>>,
    #[cfg(feature = "qualification-pipe-fault")]
    fault_close: mpsc::SyncSender<FaultClose>,
}
#[cfg(feature = "qualification-pipe-fault")]
struct FaultClose {
    witness: PartialFrameWitness,
    ack: tokio::sync::oneshot::Sender<Result<Value>>,
}
#[cfg(feature = "qualification-pipe-fault")]
struct FaultState {
    bound: BoundPlan,
    frame: Option<CutFrame>,
    active_id: Option<u64>,
    issued_id: Option<u64>,
    issue_ordinal: u64,
    prefix_reaped: bool,
    witness: Option<PartialFrameWitness>,
    gate_sample: Option<Value>,
    close_source: Option<&'static str>,
}
#[cfg(feature = "qualification-pipe-fault")]
impl FaultState {
    fn new(bound: BoundPlan) -> Self {
        Self {
            bound,
            frame: None,
            active_id: None,
            issued_id: None,
            issue_ordinal: 0,
            prefix_reaped: false,
            witness: None,
            gate_sample: None,
            close_source: None,
        }
    }
    fn event(
        &self,
        clock: &OwnerClock,
        event: &'static str,
        before: u128,
        after: u128,
        outcome: Value,
    ) -> Event {
        Event::QualificationObservation {
            event,
            detail: json!({"binding":self.bound.description(),
            "frame":self.frame.as_ref().map(CutFrame::description),"clock_domain":clock.domain,
            "before_ns":before,"after_ns":after,"duration_unit":"ns","resolution":"not_measured",
            "operation_id":self.issued_id,"issue_ordinal":self.issue_ordinal,"actual_requested_bytes":12,
            "prefix_reaped":self.prefix_reaped,"witness":self.witness,"gate_sample":self.gate_sample,
            "close_source":self.close_source,"outcome":outcome}),
        }
    }
}
struct Write {
    bytes: Vec<u8>,
    progress: FrameWrite,
    body_end: Option<u64>,
    last_sample_after_ns: u128,
    pending_samples: u8,
    active_id: u64,
    active_ordinal: u64,
    active_pending: bool,
}

// Bounded observations only: issue, three spaced incomplete samples, cancellation
// probe and actual reap. This clock is owned by the I/O thread, not its consumer.
struct OwnerClock {
    origin: Instant,
    domain: String,
}
impl OwnerClock {
    fn now(&self) -> u128 {
        self.origin.elapsed().as_nanos()
    }
    fn record(
        &self,
        stage: &str,
        start: u128,
        end: u128,
        write: Option<&Write>,
        outcome: Value,
    ) -> Event {
        Event::OwnerObservation(json!({
            "clock_domain": self.domain, "clock": "std::time::Instant",
            "duration_unit": "ns", "duration_units_per_second": 1_000_000_000u64,
            "resolution": "not_measured", "stage": stage,
            "before_ns": start, "after_ns": end,
            "operation": write.map(|w|json!({"id":w.active_id,"issue_ordinal":w.active_ordinal,
                "body_end":w.body_end,"frame_offset":w.progress.offset(),"requested_bytes":w.bytes.len()-w.progress.offset(),
                "initial_pending":w.active_pending,"incomplete_samples":w.pending_samples})),
            "outcome":outcome
        }))
    }
}
fn poll_outcome(result: &std::io::Result<Option<morrow_native_pipe_win::Completed>>) -> Value {
    match result {
        Ok(None) => json!({"kind":"incomplete","win32_error":996}),
        Ok(Some(c)) => json!({"kind":"completed","id":c.id,"bytes":c.transferred,"error":c.error}),
        Err(e) => {
            json!({"kind":"unconfirmed_error","error":e.to_string(),"win32_error":e.raw_os_error()})
        }
    }
}
impl Driver {
    pub(crate) fn spawn(locator: String, expected_pid: u32, gate: Gate) -> Result<Self> {
        Self::spawn_inner(
            locator,
            expected_pid,
            gate,
            #[cfg(feature = "qualification-pipe-fault")]
            None,
        )
    }
    #[cfg(feature = "qualification-pipe-fault")]
    pub(crate) fn spawn_with_fault(
        locator: String,
        expected_pid: u32,
        gate: Gate,
        bound: BoundPlan,
    ) -> Result<Self> {
        Self::spawn_inner(locator, expected_pid, gate, Some(bound))
    }
    fn spawn_inner(
        locator: String,
        expected_pid: u32,
        gate: Gate,
        #[cfg(feature = "qualification-pipe-fault")] bound: Option<BoundPlan>,
    ) -> Result<Self> {
        let (tx, commands) = mpsc::sync_channel(2);
        let (events, rx) = mpsc::sync_channel(8);
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        #[cfg(feature = "qualification-pipe-fault")]
        let (fault_close, fault_commands) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("morrow-owned-pipe".into())
            .spawn(move || {
                run(
                    locator,
                    expected_pid,
                    gate,
                    commands,
                    events,
                    stopped,
                    #[cfg(feature = "qualification-pipe-fault")]
                    bound,
                    #[cfg(feature = "qualification-pipe-fault")]
                    fault_commands,
                )
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tx,
            rx,
            cancel,
            join: Some(join),
            #[cfg(feature = "qualification-pipe-fault")]
            fault_close,
        })
    }
    #[cfg(feature = "qualification-pipe-fault")]
    pub(crate) fn close_after_witness(
        &self,
        witness: PartialFrameWitness,
        ack: tokio::sync::oneshot::Sender<Result<Value>>,
    ) {
        if let Err(error) = self.fault_close.try_send(FaultClose { witness, ack }) {
            let command = match error {
                mpsc::TrySendError::Full(c) | mpsc::TrySendError::Disconnected(c) => c,
            };
            let _ = command
                .ack
                .send(Err("qualification owner close queue unavailable".into()));
        }
    }
    pub(crate) fn send(&self, c: Command) -> std::result::Result<(), mpsc::TrySendError<Command>> {
        self.tx.try_send(c)
    }
    pub(crate) fn event(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }
    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub(crate) fn join_if_finished(&mut self) -> Option<Result<Closed>> {
        if !self.join.as_ref()?.is_finished() {
            return None;
        }
        Some(
            self.join
                .take()
                .unwrap()
                .join()
                .map_err(|_| "owned-I/O thread panicked; cleanup unconfirmed".into()),
        )
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        self.cancel(); /* Detached thread retains every live OS buffer; no join credit. */
    }
}
fn run(
    locator: String,
    expected_pid: u32,
    gate: Gate,
    commands: mpsc::Receiver<Command>,
    events: mpsc::SyncSender<Event>,
    cancel: Arc<AtomicBool>,
    #[cfg(feature = "qualification-pipe-fault")] bound: Option<BoundPlan>,
    #[cfg(feature = "qualification-pipe-fault")] fault_commands: mpsc::Receiver<FaultClose>,
) -> Closed {
    let clock = OwnerClock {
        origin: Instant::now(),
        domain: format!("{}:pipe-owner", locator),
    };
    let mut pipe = match Pipe::create_private(&locator) {
        Ok(p) => p,
        Err(e) => {
            return Closed {
                error: Some(format!("pipe create: {e}")),
            };
        }
    };
    let mut pending = VecDeque::new();
    let mut failure = None;
    let mut cancelling = false;
    let mut write: Option<Write> = None;
    #[cfg(feature = "qualification-pipe-fault")]
    let mut fault = bound.map(FaultState::new);
    let init = (|| -> Result<Event> {
        let info = pipe.buffer_info().map_err(|e| e.to_string())?;
        let dacl = pipe.security_sddl().map_err(|e| e.to_string())?;
        let noninherited = !pipe.inheritable().map_err(|e| e.to_string())?;
        if !noninherited {
            return Err("pipe handle inherited".into());
        }
        pipe.begin_connect().map_err(|e| e.to_string())?;
        Ok(Event::Created {
            inbound: info.inbound,
            outbound: info.outbound,
            dacl,
            noninherited,
        })
    })();
    match init {
        Ok(e) => pending.push_back(e),
        Err(e) => {
            failure = Some(e);
            cancel.store(true, Ordering::SeqCst);
        }
    }
    loop {
        #[cfg(feature = "qualification-pipe-fault")]
        if let Ok(command) = fault_commands.try_recv() {
            let accepted = (|| -> Result<Value> {
                let state = fault.as_mut().ok_or("qualification plan not bound")?;
                let gate_state = gate.lock().map_err(|_| "gate poison")?;
                let sample = Instant::now();
                if cancelling
                    || cancel.load(Ordering::SeqCst)
                    || gate_state.revoked
                    || sample >= gate_state.deadline
                    || sample >= state.bound.deadline
                    || gate_state.deadline != state.bound.deadline
                    || !state.prefix_reaped
                    || state.witness.is_some()
                {
                    return Err("qualification witness not live/ready or replayed".into());
                }
                state
                    .frame
                    .as_ref()
                    .ok_or("qualification target not issued")?
                    .validate_witness(&state.bound, &command.witness)?;
                state.gate_sample = Some(json!({"clock_domain":clock.domain,
                    "sample_offset_ns":sample.saturating_duration_since(clock.origin).as_nanos(),
                    "original_deadline_offset_ns":state.bound.deadline.saturating_duration_since(clock.origin).as_nanos(),
                    "gate_revoked":gate_state.revoked,"original_deadline_reached":sample>=state.bound.deadline,
                    "remaining_ns":state.bound.deadline.saturating_duration_since(sample).as_nanos(),
                    "gate_deadline_matches_original":gate_state.deadline==state.bound.deadline}));
                state.witness = Some(command.witness);
                state.close_source = Some("matched-passive-partial-frame-witness");
                cancel.store(true, Ordering::SeqCst);
                drop(gate_state);
                let at = clock.now();
                pending.push_back(state.event(
                    &clock,
                    "qualification_pipe_witness_matched",
                    at,
                    at,
                    json!({"close_requested":true,"data_close_proven":false}),
                ));
                // This requested endpoint close is not an invented OS failure or
                // revocation reason. The incomplete original frame stays explicit
                // in every fault observation; actual I/O errors remain in failure.
                Ok(
                    json!({"accepted":true,"data_close_proven":false,"identity_sha256":state.bound.identity_sha256}),
                )
            })();
            let _ = command.ack.send(accepted);
        }
        let mut cancellation_probe = None;
        let expired = {
            let g = gate.lock().unwrap();
            g.revoked || Instant::now() >= g.deadline
        };
        if (cancel.load(Ordering::SeqCst) || expired) && !cancelling {
            cancelling = true;
            #[cfg(feature = "qualification-pipe-fault")]
            if let Some(state) = fault.as_mut() {
                state.close_source.get_or_insert(if expired {
                    "original-effect-gate-revoked-or-expired"
                } else if failure.is_some() {
                    "pipe-error"
                } else {
                    "owner-cancel-requested"
                });
            }
            let request_at = clock.now();
            pending.push_back(clock.record(
                "cancel_request_observed",
                request_at,
                request_at,
                write.as_ref(),
                json!({"gate_or_deadline":expired}),
            ));
            // Poll exactly the existing operation BEFORE CancelIoEx. Retain a
            // returned completion for the normal reap path; never poll it twice.
            if pipe.has_operation(Kind::Write) {
                let before = clock.now();
                let result = pipe.poll(Kind::Write);
                let after = clock.now();
                pending.push_back(clock.record(
                    "cancel_probe",
                    before,
                    after,
                    write.as_ref(),
                    poll_outcome(&result),
                ));
                cancellation_probe = Some((result, before, after));
            } else {
                pending.push_back(clock.record(
                    "cancel_probe",
                    request_at,
                    clock.now(),
                    None,
                    json!({"kind":"no_operation"}),
                ));
            }
            let before = clock.now();
            if let Err(e) = pipe.cancel_all() {
                failure.get_or_insert_with(|| format!("cancel unconfirmed: {e}"));
            }
            pending.push_back(clock.record(
                "cancel_requested",
                before,
                clock.now(),
                write.as_ref(),
                json!({"completion_claimed":false}),
            ));
        }
        while let Some(event) = pending.pop_front() {
            match events.try_send(event) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(event)) => {
                    pending.push_front(event);
                    break;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    cancel.store(true, Ordering::SeqCst);
                    pending.clear();
                    break;
                }
            }
        }
        for kind in [Kind::Connect, Kind::Read, Kind::Write] {
            if !(pipe.has_operation(kind) || kind == Kind::Write && cancellation_probe.is_some()) {
                continue;
            }
            let (result, before, after) = if kind == Kind::Write && cancellation_probe.is_some() {
                cancellation_probe.take().unwrap()
            } else {
                let before = clock.now();
                let result = pipe.poll(kind);
                (result, before, clock.now())
            };
            match result {
                Ok(None) => {
                    if kind == Kind::Write
                        && !cancelling
                        && let Some(w) = write.as_mut()
                        && w.pending_samples < 3
                        && before.saturating_sub(w.last_sample_after_ns) >= 25_000_000
                    {
                        w.pending_samples += 1;
                        w.last_sample_after_ns = after;
                        pending.push_back(clock.record(
                            "incomplete_sample",
                            before,
                            after,
                            Some(w),
                            json!({"kind":"incomplete","win32_error":996}),
                        ));
                        pending.push_back(Event::WriteIncomplete {
                            id: w.active_id,
                            body_end: w.body_end,
                        });
                    }
                }
                Err(e) => {
                    if failure.is_none() {
                        let message = format!("pipe completion unconfirmed: {e}");
                        pending.push_back(Event::Error(message.clone()));
                        failure = Some(message);
                    }
                    cancel.store(true, Ordering::SeqCst);
                }
                Ok(Some(c)) => {
                    if kind == Kind::Write {
                        pending.push_back(clock.record("write_reaped", before, after, write.as_ref(), json!({"kind":"completed","id":c.id,"bytes":c.transferred,"error":c.error})));
                    }
                    pending.push_back(Event::Reaped {
                        kind,
                        id: c.id,
                        bytes: c.transferred,
                        error: c.error,
                    });
                    #[cfg(feature = "qualification-pipe-fault")]
                    if kind == Kind::Write
                        && fault.as_ref().is_some_and(|s| s.active_id == Some(c.id))
                    {
                        let state = fault.as_mut().unwrap();
                        state.active_id = None;
                        state.prefix_reaped = c.error.is_none() && c.transferred == 12;
                        pending.push_back(state.event(&clock,"qualification_pipe_prefix_reaped",before,after,
                            json!({"id":c.id,"bytes":c.transferred,"error":c.error,"cancelling":cancelling,"original_frame_complete":false})));
                        if !state.prefix_reaped {
                            let error = format!(
                                "qualification prefix completion: bytes={} error={:?}",
                                c.transferred, c.error
                            );
                            failure.get_or_insert_with(|| error.clone());
                            pending.push_back(Event::Error(error));
                            cancel.store(true, Ordering::SeqCst);
                        }
                        continue;
                    }
                    if kind == Kind::Write {
                        let result = write
                            .as_mut()
                            .ok_or_else(|| "untracked write completion".to_owned())
                            .and_then(|w| {
                                w.progress
                                    .complete(c.id, c.transferred, c.error, cancelling)
                                    .map_err(|e| format!("write frame completion: {e:?}"))
                            });
                        pending.push_back(clock.record("write_frame_state", before, after, write.as_ref(),
                            json!({"result":format!("{result:?}"),"cancelling":cancelling,
                                "confirmed_frame_prefix":write.as_ref().map(|w| w.progress.offset())})));
                        match result {
                            Ok(Outcome::Complete) => {
                                let w = write.take().expect("tracked completed frame");
                                pending.push_back(Event::WriteCompleted {
                                    body_end: w.body_end,
                                    bytes: w.progress.offset(),
                                });
                            }
                            Ok(Outcome::Continue | Outcome::Cancelled) => {}
                            Err(message) => {
                                failure.get_or_insert_with(|| message.clone());
                                pending.push_back(Event::Error(message));
                                cancel.store(true, Ordering::SeqCst);
                            }
                        }
                        continue;
                    }
                    if let Some(error) = c.error {
                        if !cancelling {
                            let message = format!("pipe {kind:?} error {error}");
                            failure.get_or_insert_with(|| message.clone());
                            pending.push_back(Event::Error(message));
                            cancel.store(true, Ordering::SeqCst);
                        }
                        continue;
                    }
                    match kind {
                        Kind::Connect => match pipe.client_pid() {
                            Ok(pid) if pid == expected_pid => {
                                pending.push_back(Event::Connected { pid })
                            }
                            _ => {
                                failure = Some("data peer PID mismatch".into());
                                pending.push_back(Event::Error("data peer PID mismatch".into()));
                                cancel.store(true, Ordering::SeqCst);
                            }
                        },
                        Kind::Read => {
                            if !cancelling {
                                if c.bytes.is_empty() {
                                    failure = Some("data EOF".into());
                                    cancel.store(true, Ordering::SeqCst);
                                } else {
                                    pending.push_back(Event::Read(c.bytes));
                                }
                            }
                        }
                        Kind::Write => unreachable!("writes handled before error dispatch"),
                    }
                }
            }
        }
        if cancelling
            && pending.is_empty()
            && !pipe.has_operation(Kind::Connect)
            && !pipe.has_operation(Kind::Read)
            && !pipe.has_operation(Kind::Write)
        {
            // Explicit operations reaped above. Dropping this now has no blocking work.
            drop(pipe);
            #[cfg(feature = "qualification-pipe-fault")]
            if let Some(state) = fault.as_ref().filter(|s| s.frame.is_some()) {
                let at = clock.now();
                let event=state.event(&clock,"qualification_pipe_closed",at,at,json!({"data_endpoint_closed":true,"all_operations_reaped":true,"matched_witness":state.witness.is_some(),"original_frame_complete":false,"platform_io_error":failure,"owner_thread_join_proven_here":false}));
                let until = Instant::now() + Duration::from_millis(100);
                let mut observation = event;
                loop {
                    match events.try_send(observation) {
                        Ok(()) => break,
                        Err(mpsc::TrySendError::Full(event)) if Instant::now() < until => {
                            observation = event;
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(_) => {
                            failure.get_or_insert_with(|| {
                                "qualification closed observation undelivered".into()
                            });
                            break;
                        }
                    }
                }
            }
            return Closed { error: failure };
        }
        if !cancelling && pending.is_empty() {
            if write.is_none() {
                match commands.try_recv() {
                    #[cfg(feature = "qualification-pipe-fault")]
                    Ok(Command::PrefixFrame { bytes, body_end }) => {
                        let result = (|| -> Result<()> {
                            let state = fault.as_mut().ok_or("qualification plan absent")?;
                            if state.frame.is_some() {
                                return Err("qualification prefix replay".into());
                            }
                            let frame = CutFrame::new(&state.bound, bytes, body_end)?;
                            let prefix = frame.original[..12].to_vec();
                            state.frame = Some(frame);
                            let before = clock.now();
                            let (ordinal, started) = issue_write(&gate, None, || {
                                pipe.begin_write(prefix).map_err(|e| e.to_string())
                            })?;
                            state.active_id = Some(started.id);
                            state.issued_id = Some(started.id);
                            state.issue_ordinal = ordinal;
                            let after = clock.now();
                            pending.push_back(state.event(&clock,"qualification_pipe_prefix_issued",before,after,
                                json!({"id":started.id,"requested":started.requested,"pending":started.pending,"body_end_advanced":false})));
                            pending.push_back(Event::WriteIssued {
                                id: started.id,
                                ordinal,
                                body_end: None,
                                bytes: started.requested,
                                pending: started.pending,
                            });
                            Ok(())
                        })();
                        if let Err(error) = result {
                            failure.get_or_insert_with(|| error.clone());
                            pending.push_back(Event::Error(error));
                            cancel.store(true, Ordering::SeqCst);
                        }
                    }
                    Ok(Command::Read(n)) => match pipe.begin_read(n) {
                        Ok(started) => {
                            #[cfg(feature = "qualification-pipe-fault")]
                            if let Some(state) = fault.as_ref().filter(|s| s.frame.is_some()) {
                                let at = clock.now();
                                pending.push_back(state.event(&clock,"qualification_pipe_read_issued",at,at,
                                        json!({"read_operation_id":started.id,"requested":started.requested,"pending":started.pending})));
                            }
                            #[cfg(not(feature = "qualification-pipe-fault"))]
                            let _ = started;
                        }
                        Err(e) => {
                            failure = Some(e.to_string());
                            cancel.store(true, Ordering::SeqCst);
                        }
                    },
                    Ok(Command::Write { bytes, body_end }) => {
                        #[cfg(feature = "qualification-pipe-fault")]
                        if fault.as_ref().is_some_and(|s| s.frame.is_some()) {
                            let error = "qualification no new write/tail after prefix".to_owned();
                            failure.get_or_insert_with(|| error.clone());
                            pending.push_back(Event::Error(error));
                            cancel.store(true, Ordering::SeqCst);
                            continue;
                        }
                        let progress = match FrameWrite::new(bytes.len()) {
                            Ok(progress) => progress,
                            Err(error) => {
                                failure = Some(format!("invalid write frame: {error:?}"));
                                cancel.store(true, Ordering::SeqCst);
                                continue;
                            }
                        };
                        write = Some(Write {
                            bytes,
                            progress,
                            body_end,
                            last_sample_after_ns: clock.now(),
                            pending_samples: 0,
                            active_id: 0,
                            active_ordinal: 0,
                            active_pending: false,
                        })
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        cancel.store(true, Ordering::SeqCst);
                    }
                }
            }
            if let Some(w) = write.as_mut()
                && !pipe.has_operation(Kind::Write)
            {
                let bytes = w.bytes[w.progress.offset()..].to_vec();
                let before = clock.now();
                match issue_write(&gate, w.body_end, || {
                    pipe.begin_write(bytes).map_err(|e| e.to_string())
                }) {
                    Ok((ordinal, started)) => {
                        if let Err(error) = w.progress.issue(started.id, started.requested) {
                            failure = Some(format!("invalid write issue: {error:?}"));
                            cancel.store(true, Ordering::SeqCst);
                        }
                        let after = clock.now();
                        w.active_id = started.id;
                        w.active_ordinal = ordinal;
                        w.active_pending = started.pending;
                        w.pending_samples = 0;
                        w.last_sample_after_ns = after;
                        pending.push_back(clock.record("write_issue", before, clock.now(), Some(w), json!({"kind":if started.pending {"io_pending"} else {"completed_inline"},"win32_error":if started.pending {Some(997)} else {None}})));
                        pending.push_back(Event::WriteIssued {
                            id: started.id,
                            ordinal,
                            body_end: w.body_end,
                            bytes: started.requested,
                            pending: started.pending,
                        });
                    }
                    Err(e) => {
                        failure.get_or_insert(e);
                        cancel.store(true, Ordering::SeqCst);
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(test)]
#[path = "pipe_observation_tests.rs"]
mod observation_tests;
