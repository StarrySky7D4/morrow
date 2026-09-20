//! Reserved trusted-owner command lane. This is not a guest ABI or a grant.
//! Byte commands require the original owner to opt in and validate their format.
//! Typed native renewal borrows its manager without granting a guest capability.
//! Commands share the execution thread, so a blocking handler/router cannot be
//! preempted. Cancellation never rolls back or automatically repeats an effect.
use super::{
    BindingError, Cancellation, Control, HostOwner, IoWorker, JobError, ManagedHostOwner, Phase,
    ServiceGrant, ServiceRunBudget, ServiceRunSnapshot, State, checked_runtime,
};
use std::sync::{Arc, Mutex, Weak, mpsc};
use zeroize::Zeroizing;

pub const MAX_OWNER_COMMANDS: usize = 8;
pub const MAX_OWNER_COMMAND_INPUT: usize = 64 * 1024;
pub const MAX_OWNER_COMMAND_REPLY: usize = 1024 * 1024;

/// Trusted application dispatch on the complete, original owner. Implementors
/// must bound allocations and work, preserve runtime identity, and must not
/// re-enter worker handles. Preparation precedes each command; final sealing
/// remains part of normal worker exit, including failure and panic recovery.
/// The queue wipes bytes while it owns them. On invocation the input is moved
/// into the handler, which must protect its own input, copies and intermediate
/// buffers, including on error or panic. Returned bytes are protected again
/// immediately; a successful handle read transfers that responsibility onward.
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

pub(super) struct Status {
    started: bool,
    cancelled: bool,
    finished: bool,
    input: Option<Zeroizing<Vec<u8>>>,
    reply: Option<Reply>,
}

enum Reply {
    Data(Zeroizing<Vec<u8>>),
    Renewal(Result<ServiceRunSnapshot, BindingError>),
}

struct Ticket {
    control: Weak<Control>,
    bytes: usize,
    status: Arc<Mutex<Status>>,
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
            state.owner_command_status.retain(|status| {
                status.as_ptr() != Arc::as_ptr(&self.status) && status.strong_count() != 0
            });
        }
    }
}

// This weak index owns only payload state, never a Ticket. Dropping an upgraded
// status while holding worker state cannot run Ticket::drop and re-enter that
// lock. Sensitive bytes are cleared without releasing any queue reservation.
pub(super) fn clear_sensitive(state: &mut State) {
    state.owner_command_status.retain(|weak| {
        let Some(status) = weak.upgrade() else {
            return false;
        };
        let mut status = status.lock().unwrap_or_else(|error| error.into_inner());
        status.input = None;
        status.reply = None;
        true
    });
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
            status.input = None;
            status.reply = None;
        }
    }

    pub fn poll(&self) -> OwnerCommandPoll {
        let Some(ticket) = &self.ticket else {
            return OwnerCommandPoll::Consumed;
        };
        let mut state = self.control.lock();
        let closed = closed(&self.control, state.phase);
        if closed {
            clear_sensitive(&mut state);
        }
        let status = ticket.lock();
        if status.finished || status.cancelled || closed {
            OwnerCommandPoll::Ready
        } else {
            OwnerCommandPoll::Pending
        }
    }

    /// `None` is still pending. Every terminal response is consumed once.
    /// Stopping, draining, cancellation or losing the owner after start yields
    /// Unknown even when the handler had already produced a response.
    /// Successfully returned bytes belong to the caller and must be wiped by
    /// that caller when they contain secrets; the queue retains no copy.
    pub fn read(&mut self) -> Result<Option<Vec<u8>>, OwnerCommandError> {
        match self.read_reply()? {
            Some(Reply::Data(mut bytes)) => Ok(Some(std::mem::take(&mut *bytes))),
            Some(Reply::Renewal(_)) => Err(OwnerCommandError::Unknown),
            None => Ok(None),
        }
    }

    fn read_reply(&mut self) -> Result<Option<Reply>, OwnerCommandError> {
        let ticket = self.ticket.as_ref().ok_or(OwnerCommandError::Consumed)?;
        let result = {
            let mut state = self.control.lock();
            let closed = closed(&self.control, state.phase);
            if closed {
                clear_sensitive(&mut state);
            }
            let mut status = ticket.lock();
            self.started = status.started;
            if status.cancelled || closed {
                status.input = None;
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

/// One typed native renewal result. Outer errors describe delivery uncertainty;
/// an inner binding error is a definitive rejected renewal while still live.
/// Cancellation after execution starts never proves the CAS did not happen.
pub struct ServiceRunRenewalHandle {
    inner: OwnerCommandHandle,
}
impl ServiceRunRenewalHandle {
    pub fn is_started(&self) -> bool {
        self.inner.is_started()
    }

    pub fn cancel(&self) {
        self.inner.cancel();
    }

    pub fn poll(&self) -> OwnerCommandPoll {
        self.inner.poll()
    }

    pub fn read(
        &mut self,
    ) -> Result<Option<Result<ServiceRunSnapshot, BindingError>>, OwnerCommandError> {
        match self.inner.read_reply()? {
            Some(Reply::Renewal(result)) => Ok(Some(result)),
            Some(Reply::Data(_)) => Err(OwnerCommandError::Unknown),
            None => Ok(None),
        }
    }
}

struct RenewalRequest {
    grant: ServiceGrant,
    expected_registry_revision: u64,
    expected_run_revision: u64,
    expires: u64,
    budget: ServiceRunBudget,
}

enum CommandKind<O: HostOwner> {
    Data {
        max_reply_bytes: usize,
        dispatch: fn(&mut O, Vec<u8>) -> Result<Vec<u8>, JobError>,
    },
    Renewal {
        request: RenewalRequest,
        dispatch: fn(
            &mut O,
            &Control,
            &Ticket,
            RenewalRequest,
        ) -> Result<ServiceRunSnapshot, BindingError>,
    },
}

pub(super) struct Command<O: HostOwner> {
    kind: CommandKind<O>,
    ticket: Arc<Ticket>,
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
        let input = {
            let state = control.lock();
            let mut status = self.ticket.lock();
            if closed(control, state.phase) || status.cancelled {
                return Ok(());
            }
            status.started = true;
            status.input.take()
        };
        // No control/response lock spans application code. A panic propagates
        // to the existing worker recovery boundary and retains the same owner.
        let reply = match self.kind {
            CommandKind::Data {
                max_reply_bytes,
                dispatch,
            } => {
                let mut input = input.ok_or(JobError::Unavailable)?;
                dispatch(owner, std::mem::take(&mut *input))
                    .map(Zeroizing::new)
                    .ok()
                    .filter(|reply| reply.len() <= max_reply_bytes)
                    .map(Reply::Data)
            }
            CommandKind::Renewal { request, dispatch } => Some(Reply::Renewal(dispatch(
                owner,
                control,
                &self.ticket,
                request,
            ))),
        };
        checked_runtime(owner, control.host)?;
        {
            let state = control.lock();
            let mut status = self.ticket.lock();
            status.finished = true;
            if !closed(control, state.phase) && !status.cancelled {
                status.reply = reply;
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

    fn enqueue_owner_command(
        &self,
        kind: CommandKind<O>,
        bytes: usize,
        input: Option<Zeroizing<Vec<u8>>>,
    ) -> Result<OwnerCommandHandle, OwnerCommandError> {
        let mut state = self.control.lock();
        if closed(&self.control, state.phase) {
            clear_sensitive(&mut state);
            return Err(OwnerCommandError::Closed);
        }
        if state.owner_commands >= MAX_OWNER_COMMANDS {
            return Err(OwnerCommandError::Busy);
        }
        state.owner_commands += 1;
        state.owner_command_bytes += bytes;
        let status = Arc::new(Mutex::new(Status {
            started: false,
            cancelled: false,
            finished: false,
            input,
            reply: None,
        }));
        // Reap dead weak entries before adding one of the at most eight live
        // reservations. This index never owns or refunds a reservation itself.
        state
            .owner_command_status
            .retain(|status| status.strong_count() != 0);
        state.owner_command_status.push(Arc::downgrade(&status));
        let ticket = Arc::new(Ticket {
            control: Arc::downgrade(&self.control),
            bytes,
            status,
        });
        let sent = self.owner_sender.try_send(Command {
            kind,
            ticket: Arc::clone(&ticket),
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

impl<O: CommandOwner> IoWorker<O> {
    pub fn submit_owner_command(
        &self,
        input: Vec<u8>,
        max_reply_bytes: usize,
    ) -> Result<OwnerCommandHandle, OwnerCommandError> {
        // Protect even the early size/capacity/closed rejection paths.
        let input = Zeroizing::new(input);
        if input.len() > MAX_OWNER_COMMAND_INPUT || max_reply_bytes > MAX_OWNER_COMMAND_REPLY {
            return Err(OwnerCommandError::Limit);
        }
        let bytes = input.len() + max_reply_bytes;
        self.enqueue_owner_command(
            CommandKind::Data {
                max_reply_bytes,
                dispatch: O::command,
            },
            bytes,
            Some(input),
        )
    }
}

impl<O: ManagedHostOwner> IoWorker<O> {
    /// Explicit, bounded renewal using the manager inside the original owner.
    /// Shares the reserved command lane and cannot preempt a blocking callback.
    /// A queued request is not permission to outlive the original authorization;
    /// all identity, current authorization and CAS checks run again at execution.
    pub fn queue_service_run_renewal(
        &self,
        grant: ServiceGrant,
        expected_registry_revision: u64,
        expected_run_revision: u64,
        expires: u64,
        budget: ServiceRunBudget,
    ) -> Result<ServiceRunRenewalHandle, OwnerCommandError> {
        let authority = self
            .control
            .authority
            .as_ref()
            .ok_or(OwnerCommandError::Closed)?;
        // Before capacity or any liveness check samples the original clock.
        grant
            .validate_binding(&authority.binding)
            .map_err(|_| OwnerCommandError::Closed)?;
        let bytes =
            size_of::<RenewalRequest>() + size_of::<Result<ServiceRunSnapshot, BindingError>>();
        let inner = self.enqueue_owner_command(
            CommandKind::Renewal {
                request: RenewalRequest {
                    grant,
                    expected_registry_revision,
                    expected_run_revision,
                    expires,
                    budget,
                },
                dispatch: renew_with_owner::<O>,
            },
            bytes,
            None,
        )?;
        Ok(ServiceRunRenewalHandle { inner })
    }
}

fn renew_with_owner<O: ManagedHostOwner>(
    owner: &mut O,
    control: &Control,
    ticket: &Ticket,
    request: RenewalRequest,
) -> Result<ServiceRunSnapshot, BindingError> {
    let manager = owner.manager().ok_or(BindingError::Denied)?;
    control.renew_service_run(
        manager,
        &request.grant,
        request.expected_registry_revision,
        request.expected_run_revision,
        request.expires,
        request.budget,
        || !ticket.lock().cancelled,
    )
}

#[cfg(test)]
mod sensitive_tests {
    use super::*;
    use crate::io_jobs::JobLimits;
    use morrow_core::{
        dispatch::{Connection, HostRuntime},
        store::Store,
    };
    use std::{collections::BTreeMap, time::Duration};

    struct Fixture {
        control: Arc<Control>,
        _connection: Connection,
        _host: HostRuntime,
        _dir: tempfile::TempDir,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut host =
                HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                    .unwrap();
            let connection = host.connect().unwrap();
            let control = Arc::new(Control {
                host: host.binding(),
                authority: None,
                revocation: host.revocation(&connection).unwrap(),
                id: 1,
                capacity: 1,
                limits: JobLimits::default(),
                timeout: Duration::from_secs(1),
                state: Mutex::new(State {
                    phase: Phase::Running,
                    next: 1,
                    jobs: BTreeMap::new(),
                    bytes: 0,
                    owner_commands: 0,
                    owner_command_bytes: 0,
                    owner_command_status: Vec::new(),
                }),
            });
            Self {
                control,
                _connection: connection,
                _host: host,
                _dir: dir,
            }
        }

        fn handle(&self, input: Option<Vec<u8>>, reply: Option<Vec<u8>>) -> OwnerCommandHandle {
            let started = reply.is_some();
            let status = Arc::new(Mutex::new(Status {
                started,
                cancelled: false,
                finished: started,
                input: input.map(Zeroizing::new),
                reply: reply.map(Zeroizing::new).map(Reply::Data),
            }));
            let bytes = 128;
            let ticket = Arc::new(Ticket {
                control: Arc::downgrade(&self.control),
                bytes,
                status,
            });
            let mut state = self.control.lock();
            state.owner_commands += 1;
            state.owner_command_bytes += bytes;
            state
                .owner_command_status
                .push(Arc::downgrade(&ticket.status));
            OwnerCommandHandle {
                control: Arc::clone(&self.control),
                ticket: Some(ticket),
                started: false,
            }
        }

        fn usage(&self) -> (usize, usize, usize) {
            let state = self.control.lock();
            (
                state.owner_commands,
                state.owner_command_bytes,
                state.owner_command_status.len(),
            )
        }
    }

    #[test]
    fn queued_cancel_clears_payload_before_dequeue_without_refunding_reservation() {
        let fixture = Fixture::new();
        let mut handle = fixture.handle(Some(b"queued credential".to_vec()), None);
        let queued_ticket = Arc::clone(handle.ticket.as_ref().unwrap());
        handle.cancel();
        assert!(queued_ticket.lock().input.is_none());
        assert_eq!(fixture.usage(), (1, 128, 1));
        assert_eq!(handle.read(), Err(OwnerCommandError::Cancelled));
        assert_eq!(fixture.usage(), (1, 128, 1));
        drop(queued_ticket);
        assert_eq!(fixture.usage(), (0, 0, 0));
    }

    #[test]
    fn stop_clears_unread_ready_payload_before_handle_is_polled_or_dropped() {
        let fixture = Fixture::new();
        let mut handle = fixture.handle(None, Some(b"one-time token".to_vec()));
        let status = Arc::clone(&handle.ticket.as_ref().unwrap().status);
        fixture.control.stop();
        assert!(status.lock().unwrap().reply.is_none());
        assert_eq!(fixture.usage(), (1, 128, 1));
        assert_eq!(handle.read(), Err(OwnerCommandError::Unknown));
        // Even an independent diagnostic Arc to payload state cannot retain
        // the queue ticket or leave a stale weak index after it is reclaimed.
        assert_eq!(fixture.usage(), (0, 0, 0));
    }

    #[test]
    fn polling_revoked_control_clears_all_payloads_but_preserves_tickets() {
        let fixture = Fixture::new();
        let ready = fixture.handle(None, Some(b"ready token".to_vec()));
        let queued = fixture.handle(Some(b"queued secret".to_vec()), None);
        fixture.control.revocation.revoke();
        assert_eq!(ready.poll(), OwnerCommandPoll::Ready);
        assert!(ready.ticket.as_ref().unwrap().lock().reply.is_none());
        assert!(queued.ticket.as_ref().unwrap().lock().input.is_none());
        assert_eq!(fixture.usage(), (2, 256, 2));
        drop(ready);
        drop(queued);
        assert_eq!(fixture.usage(), (0, 0, 0));
    }

    #[test]
    fn successful_read_transfers_original_allocation_to_caller_without_a_copy() {
        let fixture = Fixture::new();
        let bytes = b"caller owns returned secret".to_vec();
        let original = bytes.as_ptr();
        let mut handle = fixture.handle(None, Some(bytes));
        let returned = Zeroizing::new(handle.read().unwrap().unwrap());
        assert_eq!(returned.as_ptr(), original);
        assert_eq!(returned.as_slice(), b"caller owns returned secret");
        assert_eq!(fixture.usage(), (0, 0, 0));
        assert_eq!(handle.read(), Err(OwnerCommandError::Consumed));
    }
}
