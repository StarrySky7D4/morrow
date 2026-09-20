//! Reserved trusted-owner command lane. This is not a guest ABI or a grant.
//! The original owner must explicitly opt in and validate its command format.
//! Commands share the execution thread, so a blocking handler/router cannot be
//! preempted. Cancellation never rolls back or automatically repeats an effect.
use super::{Cancellation, Control, HostOwner, IoWorker, JobError, Phase, checked_runtime};
use std::sync::{Arc, Mutex, Weak, mpsc};

pub const MAX_OWNER_COMMANDS: usize = 8;
pub const MAX_OWNER_COMMAND_INPUT: usize = 64 * 1024;
pub const MAX_OWNER_COMMAND_REPLY: usize = 1024 * 1024;

/// Trusted application dispatch on the complete, original owner. Implementors
/// must bound allocations and work, preserve runtime identity, and must not
/// re-enter worker handles. Preparation precedes each command; final sealing
/// remains part of normal worker exit, including failure and panic recovery.
pub trait CommandOwner: HostOwner {
    fn command(&mut self, input: Vec<u8>) -> Result<Vec<u8>, JobError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerCommandError {
    Busy,
    Closed,
    Limit,
    Cancelled,
    /// The handler started; an effect may exist even without a valid response.
    Unknown,
    Consumed,
}
impl std::fmt::Display for OwnerCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Busy => "owner command capacity exhausted",
            Self::Closed => "owner command executor closed before execution",
            Self::Limit => "owner command byte limit exceeded",
            Self::Cancelled => "owner command cancelled before execution",
            Self::Unknown => "owner command may have produced effects",
            Self::Consumed => "owner command response already consumed",
        })
    }
}
impl std::error::Error for OwnerCommandError {}

// Call under worker state: keep state -> clock -> IO context ordering.
// Original lifetime checks remain independent from guest admission quotas.
fn closed(control: &Control, phase: Phase) -> bool {
    phase != Phase::Running || control.fault(&Cancellation::default()).is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerCommandPoll {
    Pending,
    Ready,
    Consumed,
}

struct Status {
    started: bool,
    cancelled: bool,
    finished: bool,
    reply: Option<Vec<u8>>,
}

struct Ticket {
    control: Weak<Control>,
    bytes: usize,
    status: Mutex<Status>,
}
impl Ticket {
    fn lock(&self) -> std::sync::MutexGuard<'_, Status> {
        self.status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}
impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(control) = self.control.upgrade() {
            let mut state = control.lock();
            state.owner_commands -= 1;
            state.owner_command_bytes -= self.bytes;
        }
    }
}

/// A nonblocking response handle. Unread Ready responses retain their full
/// reservation. Dropping a queued/running handle cancels delivery but its queue
/// ticket retains the reservation until actual dequeue/completion.
pub struct OwnerCommandHandle {
    control: Arc<Control>,
    ticket: Option<Arc<Ticket>>,
    started: bool,
}
impl OwnerCommandHandle {
    pub fn is_started(&self) -> bool {
        self.started
            || self
                .ticket
                .as_ref()
                .is_some_and(|ticket| ticket.lock().started)
    }

    pub fn cancel(&self) {
        if let Some(ticket) = &self.ticket {
            let _state = self.control.lock();
            let mut status = ticket.lock();
            status.cancelled = true;
            status.reply = None;
        }
    }

    pub fn poll(&self) -> OwnerCommandPoll {
        let Some(ticket) = &self.ticket else {
            return OwnerCommandPoll::Consumed;
        };
        let state = self.control.lock();
        let status = ticket.lock();
        if status.finished || status.cancelled || closed(&self.control, state.phase) {
            OwnerCommandPoll::Ready
        } else {
            OwnerCommandPoll::Pending
        }
    }

    /// `None` is still pending. Every terminal response is consumed once.
    /// Stopping, draining, cancellation or losing the owner after start yields
    /// Unknown even when the handler had already produced a response.
    pub fn read(&mut self) -> Result<Option<Vec<u8>>, OwnerCommandError> {
        let ticket = self.ticket.as_ref().ok_or(OwnerCommandError::Consumed)?;
        let result = {
            let state = self.control.lock();
            let mut status = ticket.lock();
            self.started = status.started;
            let closed = closed(&self.control, state.phase);
            if status.cancelled || closed {
                status.reply = None;
                Err(if status.started {
                    OwnerCommandError::Unknown
                } else if status.cancelled {
                    OwnerCommandError::Cancelled
                } else {
                    OwnerCommandError::Closed
                })
            } else if status.finished {
                status
                    .reply
                    .take()
                    .map(Some)
                    .ok_or(OwnerCommandError::Unknown)
            } else {
                return Ok(None);
            }
        };
        // A terminal early read must prevent a not-yet-started queued handler.
        // Release the last reservation only after releasing the control lock.
        self.cancel();
        self.ticket.take();
        result
    }
}
impl Drop for OwnerCommandHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(super) struct Command<O: HostOwner> {
    input: Vec<u8>,
    max_reply_bytes: usize,
    ticket: Arc<Ticket>,
    dispatch: fn(&mut O, Vec<u8>) -> Result<Vec<u8>, JobError>,
}
impl<O: HostOwner> Command<O> {
    pub(super) fn execute(self, owner: &mut O, control: &Arc<Control>) -> Result<(), JobError> {
        {
            let state = control.lock();
            let status = self.ticket.lock();
            if closed(control, state.phase) || status.cancelled {
                return Ok(());
            }
        }
        checked_runtime(owner, control.host)?;
        owner.prepare_io()?;
        checked_runtime(owner, control.host)?;
        {
            let state = control.lock();
            let mut status = self.ticket.lock();
            if closed(control, state.phase) || status.cancelled {
                return Ok(());
            }
            status.started = true;
        }
        // No control/response lock spans application code. A panic propagates
        // to the existing worker recovery boundary and retains the same owner.
        let result = (self.dispatch)(owner, self.input);
        checked_runtime(owner, control.host)?;
        {
            let state = control.lock();
            let mut status = self.ticket.lock();
            status.finished = true;
            if !closed(control, state.phase) && !status.cancelled {
                status.reply = result
                    .ok()
                    .filter(|reply| reply.len() <= self.max_reply_bytes);
            }
        }
        Ok(())
    }
}

impl<O: HostOwner> IoWorker<O> {
    /// Current queued/running/unread command reservations, independent of guest
    /// quotas. Bytes reserve the complete input plus the approved reply bound.
    pub fn owner_command_usage(&self) -> (usize, usize) {
        let state = self.control.lock();
        (state.owner_commands, state.owner_command_bytes)
    }
}

impl<O: CommandOwner> IoWorker<O> {
    pub fn submit_owner_command(
        &self,
        input: Vec<u8>,
        max_reply_bytes: usize,
    ) -> Result<OwnerCommandHandle, OwnerCommandError> {
        if input.len() > MAX_OWNER_COMMAND_INPUT || max_reply_bytes > MAX_OWNER_COMMAND_REPLY {
            return Err(OwnerCommandError::Limit);
        }
        let bytes = input.len() + max_reply_bytes;
        let mut state = self.control.lock();
        if closed(&self.control, state.phase) {
            return Err(OwnerCommandError::Closed);
        }
        if state.owner_commands >= MAX_OWNER_COMMANDS {
            return Err(OwnerCommandError::Busy);
        }
        state.owner_commands += 1;
        state.owner_command_bytes += bytes;
        let ticket = Arc::new(Ticket {
            control: Arc::downgrade(&self.control),
            bytes,
            status: Mutex::new(Status {
                started: false,
                cancelled: false,
                finished: false,
                reply: None,
            }),
        });
        let sent = self.owner_sender.try_send(Command {
            input,
            max_reply_bytes,
            ticket: Arc::clone(&ticket),
            dispatch: O::command,
        });
        drop(state);
        // A failed enqueue may drop its last ticket, so inspect/drop only after
        // unlocking admission. Neither failure nor cancellation replays input.
        match sent {
            Ok(()) => Ok(OwnerCommandHandle {
                control: Arc::clone(&self.control),
                ticket: Some(ticket),
                started: false,
            }),
            Err(mpsc::TrySendError::Full(_)) => Err(OwnerCommandError::Busy),
            Err(mpsc::TrySendError::Disconnected(_)) => Err(OwnerCommandError::Closed),
        }
    }
}
