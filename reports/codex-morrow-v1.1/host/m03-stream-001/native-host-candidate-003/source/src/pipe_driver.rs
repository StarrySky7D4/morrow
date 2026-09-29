//! Dedicated owned-I/O thread. Never blocks control/Tokio workers on Pipe Drop/reap.
use crate::Result;
use morrow_native_pipe_win::{Kind, Pipe};
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
    Read(usize),
    Write {
        bytes: Vec<u8>,
        body_end: Option<u64>,
    },
}
pub(crate) enum Event {
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
}
struct Write {
    bytes: Vec<u8>,
    offset: usize,
    body_end: Option<u64>,
    last_pending: Instant,
    pending_samples: u8,
    active_id: u64,
}
impl Driver {
    pub(crate) fn spawn(locator: String, expected_pid: u32, gate: Gate) -> Result<Self> {
        let (tx, commands) = mpsc::sync_channel(2);
        let (events, rx) = mpsc::sync_channel(8);
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let join = thread::Builder::new()
            .name("morrow-owned-pipe".into())
            .spawn(move || run(locator, expected_pid, gate, commands, events, stopped))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tx,
            rx,
            cancel,
            join: Some(join),
        })
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
) -> Closed {
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
        let expired = {
            let g = gate.lock().unwrap();
            g.revoked || Instant::now() >= g.deadline
        };
        if (cancel.load(Ordering::SeqCst) || expired) && !cancelling {
            cancelling = true;
            if let Err(e) = pipe.cancel_all() {
                failure.get_or_insert_with(|| format!("cancel unconfirmed: {e}"));
            }
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
            if !pipe.has_operation(kind) {
                continue;
            }
            match pipe.poll(kind) {
                Ok(None) => {
                    if kind == Kind::Write {
                        if let Some(w) = write.as_mut() {
                            if w.pending_samples < 3
                                && w.last_pending.elapsed() >= Duration::from_millis(25)
                            {
                                w.pending_samples += 1;
                                w.last_pending = Instant::now();
                                pending.push_back(Event::WriteIncomplete {
                                    id: w.active_id,
                                    body_end: w.body_end,
                                });
                            }
                        }
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
                    pending.push_back(Event::Reaped {
                        kind,
                        id: c.id,
                        bytes: c.transferred,
                        error: c.error,
                    });
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
                        Kind::Write => {
                            if let Some(w) = write.as_mut() {
                                w.offset += c.transferred;
                                if w.offset == w.bytes.len() {
                                    pending.push_back(Event::WriteCompleted {
                                        body_end: w.body_end,
                                        bytes: w.offset,
                                    });
                                    write = None;
                                } else if c.transferred == 0 || w.offset > w.bytes.len() {
                                    failure = Some("invalid partial write length".into());
                                    cancel.store(true, Ordering::SeqCst);
                                }
                            }
                        }
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
            return Closed { error: failure };
        }
        if !cancelling && pending.is_empty() {
            if write.is_none() {
                match commands.try_recv() {
                    Ok(Command::Read(n)) => {
                        if let Err(e) = pipe.begin_read(n) {
                            failure = Some(e.to_string());
                            cancel.store(true, Ordering::SeqCst);
                        }
                    }
                    Ok(Command::Write { bytes, body_end }) => {
                        write = Some(Write {
                            bytes,
                            offset: 0,
                            body_end,
                            last_pending: Instant::now(),
                            pending_samples: 0,
                            active_id: 0,
                        })
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        cancel.store(true, Ordering::SeqCst);
                    }
                }
            }
            if let Some(w) = write.as_mut() {
                if !pipe.has_operation(Kind::Write) {
                    let bytes = w.bytes[w.offset..].to_vec();
                    match issue_write(&gate, w.body_end, || {
                        pipe.begin_write(bytes).map_err(|e| e.to_string())
                    }) {
                        Ok((ordinal, started)) => {
                            w.active_id = started.id;
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
        }
        thread::sleep(Duration::from_millis(1));
    }
}
