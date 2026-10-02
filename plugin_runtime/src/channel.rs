//! Explicit trusted-host grants for bounded local-memory channels.
//! A declaration never selects a source or grants content, filesystem or network access.
use crate::{
    Fault, Report, TaskRun,
    continuation::Kind as CallKind,
    manager::{Control, ManagedInstance, Manager},
    monotonic::Instant,
};
use morrow_core::{
    channel::{Action, Budget, Endpoint, Frame, Kind, Request, Response, Status},
    dispatch::{ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::InstancePhase,
};
use sha2::{Digest, Sha256};
use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    sync::{Arc, Condvar, Mutex, Weak},
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Denied,
    Expired,
    Limit,
    Invalid,
    Closed,
    Unknown,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "channel: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

/// Host-selected source properties; this value is never accepted from the guest.
#[derive(Clone, Copy, Debug)]
pub struct Source {
    pub kind: Kind,
    pub duplex: bool,
    /// Durable cursor scope chosen by the trusted supervisor. A checkpoint is not a grant.
    pub checkpoint_scope: Option<[u8; 32]>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub channels: u32,
    pub requests: u64,
    pub messages: u64,
    pub bytes: u64,
}
/// Source completion is independent of terminal cause, cursor ACK and resource join proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProducerOutcome {
    Pending,
    Eof,
    Unknown,
}
/// Positive absence of a created thread is distinct from a successful actual join.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupProof {
    Pending,
    NoProducer,
    Joined,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProducerState {
    NoProducer,
    Starting,
    Running,
    JoinCompleted,
    StartFailed,
}
struct Ledger {
    approved: Option<Budget>,
    deadline: Option<Instant>,
    expires: u64,
    last_tick: u64,
    usage: Usage,
    counter: u64,
    resources: Vec<Arc<Resource>>,
}
/// One ledger per managed instance. Rebinding, ACK and cleanup never refund totals.
pub(crate) struct ChannelContext {
    ceiling: Budget,
    secret: [u8; 32],
    ledger: Mutex<Ledger>,
}
impl ChannelContext {
    pub(crate) fn new(ceiling: Budget) -> Self {
        // Standard-library per-process keyed entropy; neither pointers nor PIDs enter the wire.
        let random = RandomState::new();
        let mut secret = [0; 32];
        for (i, chunk) in secret.chunks_exact_mut(8).enumerate() {
            let mut hash = random.build_hasher();
            hash.write_usize(i);
            chunk.copy_from_slice(&hash.finish().to_le_bytes());
        }
        Self {
            ceiling,
            secret,
            ledger: Mutex::new(Ledger {
                approved: None,
                deadline: None,
                expires: 0,
                last_tick: 0,
                usage: Usage::default(),
                counter: 0,
                resources: Vec::new(),
            }),
        }
    }
    pub(crate) fn revoke(&self) {
        let resources = self
            .ledger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .resources
            .clone();
        for resource in resources {
            resource.close(Status::Revoked);
        }
    }
    pub(crate) fn reap(&self) -> usize {
        let resources = self
            .ledger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .resources
            .clone();
        for resource in &resources {
            let _ = resource.gate();
            let _ = resource.reap();
        }
        let reclaimed: Vec<_> = resources
            .into_iter()
            .filter(|resource| {
                let queue = resource.queues.lock().unwrap_or_else(|e| e.into_inner());
                queue.cause.is_some() && queue.reclaimed
            })
            .collect();
        let mut ledger = self.ledger.lock().unwrap_or_else(|e| e.into_inner());
        ledger
            .resources
            .retain(|resource| !reclaimed.iter().any(|done| Arc::ptr_eq(done, resource)));
        ledger.resources.len()
    }
    fn reserve(&self, messages: u64, bytes: u64, request: bool) -> Result<()> {
        let mut state = self.ledger.lock().map_err(|_| Error::Unknown)?;
        let budget = state.approved.ok_or(Error::Denied)?;
        if state
            .deadline
            .is_none_or(|deadline| Instant::now() >= deadline)
        {
            return Err(Error::Expired);
        }
        let next = Usage {
            channels: state.usage.channels,
            requests: state
                .usage
                .requests
                .checked_add(u64::from(request))
                .ok_or(Error::Limit)?,
            messages: state
                .usage
                .messages
                .checked_add(messages)
                .ok_or(Error::Limit)?,
            bytes: state.usage.bytes.checked_add(bytes).ok_or(Error::Limit)?,
        };
        if next.requests > budget.max_requests
            || next.messages > budget.max_messages
            || next.bytes > budget.max_bytes
        {
            return Err(Error::Limit);
        }
        state.usage = next;
        Ok(())
    }
}
impl Drop for ChannelContext {
    fn drop(&mut self) {
        let ledger = self.ledger.get_mut().unwrap_or_else(|e| e.into_inner());
        for resource in &ledger.resources {
            resource.close(Status::Revoked);
            let _ = resource.reap();
        }
        // Unfinished handles have no positive reclamation proof. The trusted host
        // retains ChannelCleanup when it needs a later actual join after owner exit.
    }
}
struct Queues {
    frame: Option<Frame>,
    delivered: bool,
    sent: Option<(u64, Vec<u8>)>,
    next_sequence: u64,
    last_acked: u64,
    accepted_sequence: u64,
    observed_sequence: u64,
    cause: Option<Status>,
    eof: bool,
    producer_state: ProducerState,
    reclaimed: bool,
    producer_outcome: ProducerOutcome,
    cleanup_proof: CleanupProof,
}
struct Resource {
    context: Weak<ChannelContext>,
    control: Weak<Control>,
    source: Source,
    reference: [u8; 32],
    epoch: [u8; 32],
    scope: [u8; 32],
    deadline: Instant,
    max_frame_bytes: u32,
    queues: Mutex<Queues>,
    changed: Condvar,
    producer: Mutex<Option<JoinHandle<()>>>,
}
impl Resource {
    // No queue/Store borrow: safe at the durable transaction's final commit boundary.
    fn commit_gate(&self) -> morrow_core::Result<()> {
        if Instant::now() >= self.deadline
            || self
                .control
                .upgrade()
                .is_none_or(|control| !control.active())
        {
            return Err(morrow_core::Error::Invalid("inactive channel commit"));
        }
        Ok(())
    }
    fn check_locked(&self, queue: &mut Queues) -> Result<()> {
        if self.commit_gate().is_err() {
            let expired = Instant::now() >= self.deadline;
            if queue.cause.is_none() {
                queue.cause = Some(if expired {
                    Status::Expired
                } else {
                    Status::Revoked
                });
            }
            queue.frame = None;
            queue.sent = None;
            self.changed.notify_all();
            return Err(if expired {
                Error::Expired
            } else {
                Error::Denied
            });
        }
        Ok(())
    }
    fn close_locked(&self, queue: &mut Queues, cause: Status) {
        if cause == Status::Closed {
            // Cleanup has no authority to mask an original stop or deadline.
            // Observe that gate while owning the same queue lock that records
            // the first cause, rather than checking it before lock acquisition.
            let _ = self.check_locked(queue);
        }
        if queue.cause.is_none() {
            queue.cause = Some(cause);
        }
        queue.frame = None;
        queue.sent = None;
        self.changed.notify_all();
    }
    fn close(&self, cause: Status) {
        let mut queue = self.queues.lock().unwrap_or_else(|e| e.into_inner());
        self.close_locked(&mut queue, cause);
    }
    fn try_close(&self, cause: Status) -> bool {
        let mut queue = match self.queues.try_lock() {
            Ok(queue) => queue,
            Err(_) => return false,
        };
        self.close_locked(&mut queue, cause);
        true
    }
    fn gate(&self) -> Result<()> {
        if Instant::now() >= self.deadline {
            self.close(Status::Expired);
            return Err(Error::Expired);
        }
        if self
            .control
            .upgrade()
            .is_none_or(|control| !control.active())
        {
            self.close(Status::Revoked);
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn reap(&self) -> Status {
        let mut handle = match self.producer.try_lock() {
            Ok(handle) => handle,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return Status::ClosingUnconfirmed,
        };
        // Never wait for a producer or an ACK storage transaction on a control caller.
        let mut queue = match self.queues.try_lock() {
            Ok(queue) => queue,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return Status::ClosingUnconfirmed,
        };
        let finished = handle.as_ref().is_some_and(JoinHandle::is_finished);
        if finished {
            let joined = handle.take().expect("finished real producer").join();
            queue.reclaimed = true;
            queue.producer_state = ProducerState::JoinCompleted;
            queue.cleanup_proof = CleanupProof::Joined;
            if joined.is_err() {
                queue.producer_outcome = ProducerOutcome::Unknown;
                // A failed real join makes the source outcome Unknown, but
                // cannot rewrite an already observed terminal first cause.
                queue.cause.get_or_insert(Status::Unknown);
                queue.frame = None;
                queue.sent = None;
            }
        }
        if queue.cause.is_some()
            && handle.is_none()
            && matches!(
                queue.producer_state,
                ProducerState::NoProducer | ProducerState::StartFailed
            )
        {
            queue.reclaimed = true;
            queue.cleanup_proof = CleanupProof::NoProducer;
        }
        if queue.cause.is_some() && !queue.reclaimed {
            Status::ClosingUnconfirmed
        } else if let Some(cause) = queue.cause {
            cause
        } else {
            Status::Ready
        }
    }
}
/// Producer operations run on the actual trusted source thread, never inside a Wasm callback.
pub struct Producer {
    resource: Arc<Resource>,
    finished: bool,
}
impl Producer {
    /// At most one unacknowledged inbound frame. Waiting is bounded by the original deadline.
    pub fn push(&self, bytes: Vec<u8>, cursor: Vec<u8>) -> Result<()> {
        if bytes.is_empty()
            || bytes.len() > self.resource.max_frame_bytes as usize
            || cursor.len() > morrow_core::channel::MAX_CURSOR_BYTES
            || (self.resource.source.kind == Kind::ByteStream && !cursor.is_empty())
        {
            return Err(Error::Invalid);
        }
        // Keep the context alive outside every queue lock; its final Drop closes
        // resources and must never run while this producer holds its own queue.
        let context = self.resource.context.upgrade().ok_or(Error::Denied)?;
        loop {
            self.resource.gate()?;
            let mut queue = self.resource.queues.lock().map_err(|_| Error::Unknown)?;
            self.resource.check_locked(&mut queue)?;
            if queue.cause.is_some() || queue.eof {
                return Err(Error::Closed);
            }
            if queue.frame.is_none() {
                let sequence = queue.next_sequence.checked_add(1).ok_or(Error::Limit)?;
                context.reserve(1, bytes.len() as u64, false)?;
                self.resource.check_locked(&mut queue)?;
                queue.next_sequence = sequence;
                queue.frame = Some(Frame {
                    sequence,
                    source_epoch: self.resource.epoch,
                    bytes,
                    cursor,
                });
                queue.delivered = false;
                self.resource.changed.notify_all();
                return Ok(());
            }
            let wait = self
                .resource
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(Error::Expired)?
                .min(Duration::from_millis(25));
            let (next, _) = self
                .resource
                .changed
                .wait_timeout(queue, wait)
                .map_err(|_| Error::Unknown)?;
            drop(next);
        }
    }
    /// Accepted by the local broker is not proof that this trusted peer observed it.
    pub fn receive_sent(&self) -> Result<Option<(u64, Vec<u8>)>> {
        self.resource.gate()?;
        let mut queue = self.resource.queues.lock().map_err(|_| Error::Unknown)?;
        self.resource.check_locked(&mut queue)?;
        if queue.cause.is_some() {
            return Err(Error::Closed);
        }
        let sent = queue.sent.take();
        if let Some((sequence, _)) = &sent {
            queue.observed_sequence = *sequence;
        }
        self.resource.changed.notify_all();
        Ok(sent)
    }
    /// Blocking source-thread read, bounded by the original grant's deadline and revocation.
    /// This method is never called from a Wasm host callback.
    pub fn wait_sent(&self) -> Result<(u64, Vec<u8>)> {
        loop {
            self.resource.gate()?;
            let mut queue = self.resource.queues.lock().map_err(|_| Error::Unknown)?;
            self.resource.check_locked(&mut queue)?;
            if queue.cause.is_some() {
                return Err(Error::Closed);
            }
            if let Some(sent) = queue.sent.take() {
                queue.observed_sequence = sent.0;
                self.resource.changed.notify_all();
                return Ok(sent);
            }
            let wait = self
                .resource
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(Error::Expired)?
                .min(Duration::from_millis(25));
            let (next, _) = self
                .resource
                .changed
                .wait_timeout(queue, wait)
                .map_err(|_| Error::Unknown)?;
            drop(next);
        }
    }
    /// Explicit source EOF. Pending inbound data remains available for its original ACK.
    pub fn finish(mut self) -> Result<()> {
        self.resource.gate()?;
        let mut queue = self.resource.queues.lock().map_err(|_| Error::Unknown)?;
        self.resource.check_locked(&mut queue)?;
        if queue.cause.is_some() {
            return Err(Error::Closed);
        }
        queue.eof = true;
        queue.producer_outcome = ProducerOutcome::Eof;
        if queue.frame.is_none() {
            queue.cause = Some(Status::Closed);
            queue.sent = None;
        }
        self.finished = true;
        self.resource.changed.notify_all();
        Ok(())
    }
}
impl Drop for Producer {
    fn drop(&mut self) {
        if !self.finished {
            let mut queue = self
                .resource
                .queues
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            // Record the uncertain source outcome and its first terminal cause
            // atomically. Cleanup must not insert Closed between these facts.
            queue.producer_outcome = ProducerOutcome::Unknown;
            self.resource.close_locked(&mut queue, Status::Unknown);
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub terminal_cause: Option<Status>,
    pub resource_reclaimed: bool,
    pub last_acked: u64,
    pub accepted_sequence: u64,
    pub usage: Usage,
    pub producer_outcome: ProducerOutcome,
    pub cleanup_proof: CleanupProof,
}
/// Nonserializable authority bound to one manager, host, instance and immutable package.
pub struct ChannelBroker {
    manager: Weak<()>,
    host: HostBinding,
    connection: ConnectionBinding,
    control: Weak<Control>,
    digest: [u8; 32],
    budget: Budget,
    context: Arc<ChannelContext>,
    resource: Arc<Resource>,
}
/// A host-retained cleanup owner. It cannot exchange data or advance a checkpoint.
pub struct ChannelCleanup {
    context: Arc<ChannelContext>,
    resource: Arc<Resource>,
}
impl ChannelCleanup {
    /// Cleanup-only control path. A busy original queue keeps its handle pending.
    /// The original cancellation/deadline gate is still checked synchronously.
    pub fn try_cleanup(&self) -> Status {
        if !self.resource.try_close(Status::Closed) {
            return Status::ClosingUnconfirmed;
        }
        self.resource.reap()
    }
    pub fn try_reap(&self) -> Status {
        if self.resource.commit_gate().is_err() {
            let cause = if Instant::now() >= self.resource.deadline {
                Status::Expired
            } else {
                Status::Revoked
            };
            if !self.resource.try_close(cause) {
                return Status::ClosingUnconfirmed;
            }
        }
        self.resource.reap()
    }
    pub fn cleanup(&self) -> Status {
        self.resource.close(Status::Closed);
        self.resource.reap()
    }
    pub fn reap(&self) -> Status {
        let _ = self.resource.gate();
        self.resource.reap()
    }
    pub fn resource_reclaimed(&self) -> bool {
        self.resource
            .queues
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .reclaimed
    }
    pub fn cleanup_proof(&self) -> CleanupProof {
        self.resource
            .queues
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cleanup_proof
    }
    pub fn terminal_cause(&self) -> Option<Status> {
        self.resource
            .queues
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cause
    }
    pub fn outstanding_resources(&self) -> usize {
        self.context.reap()
    }
}
impl ChannelBroker {
    /// Hold the actual original queue mutex for a deterministic native lock test.
    /// This seam changes no state, authorization, reply or producer outcome.
    #[cfg(feature = "fault-injection")]
    pub fn with_queue_lock_for_fault_test(&self, hold: impl FnOnce()) -> Result<()> {
        let _guard = self.resource.queues.lock().map_err(|_| Error::Unknown)?;
        hold();
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bind(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        source: Source,
        budget: Budget,
        expires: u64,
        now: u64,
    ) -> Result<Self> {
        budget.validate().map_err(|_| Error::Limit)?;
        let control = instance.io_control();
        let context = control.channel.as_ref().ok_or(Error::Denied)?.clone();
        let ceiling = context.ceiling;
        if budget.max_channels == 0
            || budget.max_frame_bytes == 0
            || budget.max_bytes == 0
            || budget.max_messages == 0
            || budget.max_requests == 0
            || budget.max_duration_ms == 0
            || budget.max_channels > ceiling.max_channels
            || budget.max_frame_bytes > ceiling.max_frame_bytes
            || budget.max_bytes > ceiling.max_bytes
            || budget.max_messages > ceiling.max_messages
            || budget.max_requests > ceiling.max_requests
            || budget.max_duration_ms > ceiling.max_duration_ms
            || source
                .checkpoint_scope
                .is_some_and(|scope| scope == [0; 32])
            || source.checkpoint_scope.is_some() && source.kind != Kind::Events
        {
            return Err(Error::Invalid);
        }
        let duration = expires
            .checked_sub(now)
            .filter(|duration| *duration > 0)
            .ok_or(Error::Expired)?;
        if duration > budget.max_duration_ms {
            return Err(Error::Limit);
        }
        let mut ledger = context.ledger.lock().map_err(|_| Error::Unknown)?;
        if let Some(approved) = ledger.approved {
            if approved != budget || expires != ledger.expires || now < ledger.last_tick {
                return Err(Error::Denied);
            }
        } else {
            ledger.deadline = Instant::now().checked_add(Duration::from_millis(duration));
            if ledger.deadline.is_none() {
                return Err(Error::Limit);
            }
            ledger.approved = Some(budget);
            ledger.expires = expires;
        }
        let deadline = ledger.deadline.ok_or(Error::Unknown)?;
        if Instant::now() >= deadline || now >= ledger.expires {
            return Err(Error::Expired);
        }
        let channels = ledger.usage.channels.checked_add(1).ok_or(Error::Limit)?;
        if channels > budget.max_channels {
            return Err(Error::Limit);
        }
        let counter = ledger.counter.checked_add(1).ok_or(Error::Limit)?;
        let derive = |domain: &[u8]| -> [u8; 32] {
            let mut hash = Sha256::new();
            hash.update(domain);
            hash.update(context.secret);
            hash.update(instance.package().package().digest());
            hash.update(counter.to_le_bytes());
            hash.finalize().into()
        };
        let resource = Arc::new(Resource {
            context: Arc::downgrade(&context),
            control: Arc::downgrade(control),
            source,
            reference: derive(b"morrow/channel/reference/v1"),
            epoch: derive(b"morrow/channel/epoch/v1"),
            scope: derive(b"morrow/channel/scope/v1"),
            deadline,
            max_frame_bytes: budget.max_frame_bytes,
            queues: Mutex::new(Queues {
                frame: None,
                delivered: false,
                sent: None,
                next_sequence: 0,
                last_acked: 0,
                accepted_sequence: 0,
                observed_sequence: 0,
                cause: None,
                eof: false,
                producer_state: ProducerState::NoProducer,
                reclaimed: false,
                producer_outcome: ProducerOutcome::Pending,
                cleanup_proof: CleanupProof::Pending,
            }),
            changed: Condvar::new(),
            producer: Mutex::new(None),
        });
        ledger.counter = counter;
        ledger.last_tick = now;
        ledger.usage.channels = channels;
        ledger.resources.push(resource.clone());
        drop(ledger);
        instance.cancellation().limit_deadline(deadline);
        Ok(Self {
            manager: manager.identity(),
            host: host.binding(),
            connection: instance.connection().binding(),
            control: Arc::downgrade(control),
            digest: instance.package().package().digest(),
            budget,
            context,
            resource,
        })
    }
    pub fn endpoint(&self) -> Endpoint {
        Endpoint {
            reference: self.resource.reference,
            source_epoch: self.resource.epoch,
            kind: self.resource.source.kind,
            budget: self.budget,
        }
    }
    pub fn cleanup_handle(&self) -> ChannelCleanup {
        ChannelCleanup {
            context: self.context.clone(),
            resource: self.resource.clone(),
        }
    }
    pub fn directory(&self) -> morrow_core::channel::Directory {
        morrow_core::channel::Directory {
            scope_sha256: self.resource.scope,
            channels: vec![self.endpoint()],
        }
    }
    /// Install exactly one actual native producer. A callback state is never a cleanup proof.
    pub fn spawn(&self, produce: impl FnOnce(Producer) + Send + 'static) -> Result<()> {
        self.spawn_with_options(None, produce)
    }
    /// Fault qualification only: request a real native stack reservation, never a mocked error.
    #[cfg(feature = "fault-injection")]
    pub fn spawn_with_stack_size(
        &self,
        stack_bytes: usize,
        produce: impl FnOnce(Producer) + Send + 'static,
    ) -> Result<()> {
        self.spawn_with_options(Some(stack_bytes), produce)
    }
    fn spawn_with_options(
        &self,
        stack_bytes: Option<usize>,
        produce: impl FnOnce(Producer) + Send + 'static,
    ) -> Result<()> {
        self.resource.gate()?;
        let mut handle = self.resource.producer.lock().map_err(|_| Error::Unknown)?;
        let mut queue = self.resource.queues.lock().map_err(|_| Error::Unknown)?;
        if queue.producer_state != ProducerState::NoProducer || queue.cause.is_some() {
            return Err(Error::Closed);
        }
        queue.producer_state = ProducerState::Starting;
        drop(queue);
        let resource = self.resource.clone();
        let mut builder = thread::Builder::new().name("morrow-local-channel".into());
        if let Some(stack_bytes) = stack_bytes {
            builder = builder.stack_size(stack_bytes);
        }
        let spawned = builder
            .spawn(move || {
                produce(Producer {
                    resource,
                    finished: false,
                })
            })
            .map_err(|_| Error::Unknown);
        match spawned {
            Ok(join) => {
                *handle = Some(join);
                self.resource
                    .queues
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .producer_state = ProducerState::Running;
                Ok(())
            }
            Err(error) => {
                let mut queue = self
                    .resource
                    .queues
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                queue.producer_state = ProducerState::StartFailed;
                queue.producer_outcome = ProducerOutcome::Unknown;
                drop(queue);
                self.resource.close(Status::Unknown);
                Err(error)
            }
        }
    }
    fn owner(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
    ) -> Result<()> {
        if !Weak::ptr_eq(&self.manager, &manager.identity())
            || self.manager.upgrade().is_none()
            || self.host != host.binding()
            || self.connection != instance.connection().binding()
            || !Weak::ptr_eq(&self.control, &Arc::downgrade(instance.io_control()))
            || self.digest != instance.package().package().digest()
            || instance.connection().package_digest() != Some(self.digest)
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn gate(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        now: u64,
    ) -> Result<()> {
        self.owner(manager, host, instance)?;
        if manager.validate_instance(host, instance).is_err()
            || host.connection_phase(instance.connection()) != Ok(InstancePhase::Ready)
        {
            self.resource.close(Status::Revoked);
            return Err(Error::Denied);
        }
        self.resource.gate()?;
        let mut ledger = self.context.ledger.lock().map_err(|_| Error::Unknown)?;
        if now < ledger.last_tick {
            drop(ledger);
            self.resource.close(Status::Unknown);
            return Err(Error::Invalid);
        }
        if now >= ledger.expires {
            drop(ledger);
            self.resource.close(Status::Expired);
            return Err(Error::Expired);
        }
        ledger.last_tick = now;
        Ok(())
    }
    pub fn snapshot(&self) -> Snapshot {
        let queue = self
            .resource
            .queues
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let usage = self
            .context
            .ledger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .usage;
        Snapshot {
            terminal_cause: queue.cause,
            resource_reclaimed: queue.reclaimed,
            last_acked: queue.last_acked,
            accepted_sequence: queue.accepted_sequence,
            usage,
            producer_outcome: queue.producer_outcome,
            cleanup_proof: queue.cleanup_proof,
        }
    }
    /// A busy queue/ledger is a pending observation, never a joined resource.
    pub fn try_snapshot(&self) -> Option<Snapshot> {
        let queue = self.resource.queues.try_lock().ok()?;
        let usage = self.context.ledger.try_lock().ok()?.usage;
        Some(Snapshot {
            terminal_cause: queue.cause,
            resource_reclaimed: queue.reclaimed,
            last_acked: queue.last_acked,
            accepted_sequence: queue.accepted_sequence,
            usage,
            producer_outcome: queue.producer_outcome,
            cleanup_proof: queue.cleanup_proof,
        })
    }
    /// Trusted host cleanup carries no permission to receive, send or advance a cursor.
    pub fn cleanup(&self) -> Status {
        self.resource.close(Status::Closed);
        self.resource.reap()
    }
    pub fn reap(&self) -> Status {
        self.resource.reap()
    }
    pub fn dispatch(
        &self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        request: &Request,
        now: u64,
    ) -> Result<Response> {
        self.owner(manager, host, instance)?;
        let digest = request.digest().map_err(|_| Error::Invalid)?;
        if request.reference != self.resource.reference
            || request.source_epoch != self.resource.epoch
        {
            return Err(Error::Denied);
        }
        let cleanup = matches!(request.action, Action::Close | Action::Query);
        let gate = self.gate(manager, host, instance, now);
        let mut frame = None;
        let mut status = if let Err(error) = gate {
            if !cleanup {
                return self.response(
                    request,
                    digest,
                    match error {
                        Error::Expired => Status::Expired,
                        Error::Invalid => Status::Unknown,
                        _ => Status::Revoked,
                    },
                    None,
                );
            }
            self.resource.reap()
        } else {
            match self.context.reserve(0, 0, true) {
                Ok(()) => Status::Ready,
                Err(Error::Limit) => Status::Limit,
                Err(Error::Expired) => {
                    self.resource.close(Status::Expired);
                    Status::Expired
                }
                Err(_) => Status::Unknown,
            }
        };
        if cleanup {
            status = if matches!(request.action, Action::Close) {
                self.cleanup()
            } else {
                self.resource.reap()
            };
        } else if status == Status::Ready {
            // Authorization is rechecked before accessing data and before delivering it.
            self.resource.gate()?;
            let mut queue = self.resource.queues.lock().map_err(|_| Error::Unknown)?;
            self.resource.check_locked(&mut queue)?;
            if let Some(cause) = queue.cause {
                status = cause;
            } else {
                match &request.action {
                    Action::Receive {
                        last_acked,
                        credit_bytes,
                    } => {
                        if *last_acked != queue.last_acked {
                            status = Status::Invalid;
                        } else if let Some(pending) = &queue.frame {
                            if *credit_bytes < pending.bytes.len() as u32 {
                                status = Status::Limit;
                            } else {
                                frame = Some(pending.clone());
                                queue.delivered = true;
                                status = Status::Frame;
                            }
                        } else {
                            status = Status::Idle;
                        }
                    }
                    Action::Ack {
                        sequence,
                        frame_sha256,
                        cursor,
                    } => {
                        let valid = queue.delivered
                            && queue.frame.as_ref().is_some_and(|pending| {
                                pending.sequence == *sequence
                                    && pending.digest().ok().as_ref() == Some(frame_sha256)
                                    && &pending.cursor == cursor
                            });
                        if !valid {
                            status = Status::Invalid;
                        } else {
                            if let Some(subscription) = self.resource.source.checkpoint_scope {
                                let durable = (|| -> Result<()> {
                                    let pending =
                                        queue.frame.as_ref().expect("validated original frame");
                                    let checkpoint = host
                                        .store_local()
                                        .channel_checkpoint(&subscription, &self.resource.epoch)
                                        .map_err(|_| Error::Unknown)?;
                                    let ack = Response {
                                        call_id: request.call_id,
                                        request_sha256: digest,
                                        reference: request.reference,
                                        source_epoch: request.source_epoch,
                                        status: Status::Acked,
                                        frame: None,
                                        last_acked: *sequence,
                                        accepted_sequence: 0,
                                        resource_reclaimed: false,
                                    };
                                    host.store_local_mut()
                                        .commit_channel_ack_guarded(
                                            &subscription,
                                            checkpoint.as_ref(),
                                            &pending.encode().map_err(|_| Error::Invalid)?,
                                            &request.encode().map_err(|_| Error::Invalid)?,
                                            &ack.encode().map_err(|_| Error::Invalid)?,
                                            || self.resource.commit_gate(),
                                        )
                                        .map_err(|_| Error::Unknown)?;
                                    Ok(())
                                })();
                                if durable.is_err() {
                                    queue.cause = Some(Status::Unknown);
                                    queue.frame = None;
                                    queue.sent = None;
                                    self.resource.changed.notify_all();
                                    return Err(Error::Unknown);
                                }
                            }
                            queue.last_acked = *sequence;
                            queue.frame = None;
                            if queue.eof {
                                queue.cause = Some(Status::Closed);
                                queue.sent = None;
                            }
                            self.resource.changed.notify_all();
                            status = Status::Acked;
                        }
                    }
                    Action::Send { sequence, bytes } => {
                        if !self.resource.source.duplex {
                            status = Status::Unsupported;
                        } else if queue.eof
                            || *sequence
                                != queue.accepted_sequence.checked_add(1).ok_or(Error::Limit)?
                        {
                            status = Status::Invalid;
                        } else if queue.sent.is_some() {
                            status = Status::Limit;
                        } else if bytes.len() > self.resource.max_frame_bytes as usize {
                            status = Status::Limit;
                        } else {
                            match self.context.reserve(1, bytes.len() as u64, false) {
                                Ok(()) => {
                                    self.resource.check_locked(&mut queue)?;
                                    queue.sent = Some((*sequence, bytes.clone()));
                                    queue.accepted_sequence = *sequence;
                                    status = Status::Accepted;
                                }
                                Err(_) => status = Status::Limit,
                            }
                        }
                    }
                    Action::Close | Action::Query => unreachable!("cleanup handled"),
                }
            }
            drop(queue);
            if self.resource.gate().is_err() {
                frame = None;
                status = self.snapshot().terminal_cause.unwrap_or(Status::Unknown);
            }
        }
        self.response(request, digest, status, frame)
    }
    fn response(
        &self,
        request: &Request,
        request_sha256: [u8; 32],
        status: Status,
        frame: Option<Frame>,
    ) -> Result<Response> {
        let snapshot = self.snapshot();
        Ok(Response {
            call_id: request.call_id,
            request_sha256,
            reference: request.reference,
            source_epoch: request.source_epoch,
            status,
            frame,
            last_acked: snapshot.last_acked,
            accepted_sequence: if status == Status::Accepted {
                snapshot.accepted_sequence
            } else {
                0
            },
            resource_reclaimed: snapshot.resource_reclaimed
                && matches!(
                    status,
                    Status::Closed | Status::Revoked | Status::Expired | Status::Unknown
                ),
        })
    }
    pub fn exchange(
        &self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        bytes: &[u8],
        now: u64,
    ) -> Result<Vec<u8>> {
        self.owner(manager, host, instance)?;
        let request = match Request::decode(bytes) {
            Ok(request) => request,
            Err(_) => {
                self.gate(manager, host, instance, now)?;
                self.context.reserve(0, 0, true)?;
                return Err(Error::Invalid);
            }
        };
        let response = self.dispatch(manager, host, instance, &request, now)?;
        response
            .validate_for(&request)
            .map_err(|_| Error::Invalid)?;
        response.encode().map_err(|_| Error::Invalid)
    }
    // Scheduling occurs while the original Wasm import is suspended, before the
    // single callback dispatch. It neither retries an action nor resets credit/budgets.
    fn wait_ready(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        request: &Request,
        clock: &mut impl FnMut() -> u64,
    ) -> Result<()> {
        loop {
            self.gate(manager, host, instance, clock())?;
            if request.reference != self.resource.reference
                || request.source_epoch != self.resource.epoch
            {
                return Ok(()); // the one dispatch rejects the original mismatched request
            }
            let usage = self
                .context
                .ledger
                .lock()
                .map_err(|_| Error::Unknown)?
                .usage;
            if usage.requests >= self.budget.max_requests {
                return Ok(());
            }
            let queue = self.resource.queues.lock().map_err(|_| Error::Unknown)?;
            if queue.cause.is_some() || queue.producer_state == ProducerState::NoProducer {
                return Ok(());
            }
            let ready = match &request.action {
                Action::Receive { last_acked, .. } => {
                    *last_acked != queue.last_acked || queue.frame.is_some() || queue.eof
                }
                Action::Send { sequence, bytes } => {
                    !self.resource.source.duplex
                        || queue.eof
                        || *sequence
                            != queue.accepted_sequence.checked_add(1).ok_or(Error::Limit)?
                        || bytes.len() > self.resource.max_frame_bytes as usize
                        || usage.messages >= self.budget.max_messages
                        || usage
                            .bytes
                            .checked_add(bytes.len() as u64)
                            .is_none_or(|bytes| bytes > self.budget.max_bytes)
                        || queue.sent.is_none()
                }
                _ => true,
            };
            if ready {
                return Ok(());
            }
            let wait = self
                .resource
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(Error::Expired)?
                .min(Duration::from_millis(25));
            let (next, _) = self
                .resource
                .changed
                .wait_timeout(queue, wait)
                .map_err(|_| Error::Unknown)?;
            drop(next);
        }
    }
    // Synchronize only the admitted send with the trusted source's actual queue read.
    // This is continuation-driver scheduling outside dispatch/exchange. Accepted
    // still means admission, not peer business success; unknown waits are not replayed.
    fn wait_sent_read(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        sequence: u64,
        clock: &mut impl FnMut() -> u64,
    ) -> Result<()> {
        loop {
            self.gate(manager, host, instance, clock())?;
            let queue = self.resource.queues.lock().map_err(|_| Error::Unknown)?;
            if queue.observed_sequence >= sequence {
                return Ok(());
            }
            if queue.cause.is_some() {
                return Err(Error::Unknown);
            }
            let wait = self
                .resource
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(Error::Expired)?
                .min(Duration::from_millis(25));
            let (next, _) = self
                .resource
                .changed
                .wait_timeout(queue, wait)
                .map_err(|_| Error::Unknown)?;
            drop(next);
        }
    }
    /// Execute only through an authentic managed binding. Core calls retain their own host grants.
    pub fn run_task(
        &self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        input: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> TaskRun {
        let fail = |fault| TaskRun {
            report: Report {
                outcome: Err(fault),
                host_calls: 0,
                fuel_remaining: instance.package().limits().fuel,
            },
            completion: None,
        };
        if self.gate(manager, host, instance, clock()).is_err() {
            return fail(Fault::PackageBinding);
        }
        let cancel = instance.cancellation();
        let mut execution = match instance.package().start_channel(input, cancel) {
            Ok(execution) => execution,
            Err(fault) => return fail(fault),
        };
        while let Some(call) = execution.pending() {
            let token = call.token.clone();
            let readiness = if call.kind == CallKind::Channel {
                match Request::decode(&call.bytes) {
                    Ok(request) => self.wait_ready(manager, host, instance, &request, &mut clock),
                    Err(_) => Ok(()),
                }
            } else {
                Ok(())
            };
            let response =
                if readiness.is_err() || self.gate(manager, host, instance, clock()).is_err() {
                    Err(())
                } else {
                    match call.kind {
                        CallKind::Channel => self
                            .exchange(manager, host, instance, &call.bytes, clock())
                            .map_err(|_| ()),
                        CallKind::Core => host
                            .dispatch(instance.connection(), &call.bytes, &mut clock)
                            .map_err(|_| ()),
                        _ => Err(()),
                    }
                };
            let response = if self.gate(manager, host, instance, clock()).is_err() {
                Err(())
            } else {
                response
            };
            if call.kind == CallKind::Channel {
                if let Ok(bytes) = &response {
                    if let Ok(reply) = Response::decode(bytes) {
                        if reply.status == Status::Accepted {
                            if let Err(error) = self.wait_sent_read(
                                manager,
                                host,
                                instance,
                                reply.accepted_sequence,
                                &mut clock,
                            ) {
                                execution.abort(match error {
                                    Error::Expired => Fault::Deadline,
                                    Error::Denied => Fault::Cancelled,
                                    _ => Fault::TaskProtocol,
                                });
                                break;
                            }
                        }
                    }
                }
            }
            execution
                .resume(&token, response)
                .expect("original managed channel call");
        }
        let mut run = execution.finish();
        if let Err(error) = self.gate(manager, host, instance, clock()) {
            run.completion = None;
            run.report.outcome = Err(if error == Error::Expired {
                Fault::Deadline
            } else {
                Fault::InactiveConnection
            });
        }
        run
    }
    /// Validate the existing TaskInput/TaskResult envelope on the separate channel handler path.
    /// Channel handlers cannot enter the ordinary pure-transform capture/replay path.
    pub fn run_invocation(
        &self,
        manager: &Manager,
        host: &mut HostRuntime,
        instance: &ManagedInstance,
        input: &morrow_core::task::Invocation,
        clock: impl FnMut() -> u64,
    ) -> crate::package::TaskReport {
        // Revocation belongs only to this authentic broker/host/manager and
        // its original instance. A caller-supplied foreign instance must not be
        // stopped merely because its task receives a rejected report here.
        let original_owner = self.owner(manager, host, instance).is_ok();
        let registration = input
            .transform()
            .and_then(|transform| instance.package().package().channel_handler(transform).ok());
        let Some(registration) = registration else {
            return self.finish_invocation(
                instance,
                original_owner,
                crate::package::TaskReport {
                    execution: Report {
                        outcome: Err(Fault::TaskProtocol),
                        host_calls: 0,
                        fuel_remaining: instance.package().limits().fuel,
                    },
                    response: None,
                    output: None,
                    failure: None,
                },
            );
        };
        let mut run = self.run_task(manager, host, instance, input.bytes(), clock);
        let mut output = None;
        let mut failure = None;
        if run.report.outcome.is_ok() {
            match run
                .completion
                .as_deref()
                .and_then(|bytes| input.verify_transform_result(bytes).ok())
            {
                Some(morrow_core::task::TransformResult::Output(value))
                    if value.bytes.len() <= registration.max_output_bytes as usize =>
                {
                    output = Some(value)
                }
                Some(morrow_core::task::TransformResult::Failure(value)) => failure = Some(value),
                _ => run.report.outcome = Err(Fault::TaskProtocol),
            }
        }
        self.finish_invocation(
            instance,
            original_owner,
            crate::package::TaskReport {
                execution: run.report,
                response: None,
                output,
                failure,
            },
        )
    }
    fn finish_invocation(
        &self,
        instance: &ManagedInstance,
        original_owner: bool,
        report: crate::package::TaskReport,
    ) -> crate::package::TaskReport {
        if original_owner
            && (report.execution.outcome.is_err()
                || report.failure.is_some()
                || report.output.is_none())
        {
            // First finalize the owned report, then revoke the exact original
            // shared Control. Its fault/completion is not rewritten as cancel,
            // Close, rollback or successful resource reclamation.
            instance.request_stop();
            self.resource.changed.notify_all();
        }
        report
    }
}
impl Drop for ChannelBroker {
    fn drop(&mut self) {
        self.resource.close(Status::Closed);
        let _ = self.resource.reap();
    }
}
