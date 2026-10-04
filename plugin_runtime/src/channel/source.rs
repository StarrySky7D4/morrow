//! Explicit host approval for one network-source command on one local broker.
//! A channel declaration, cached selection, digest or checkpoint is not this grant.
use super::{ChannelBroker, Error, ProducerState, Resource, Result};
use crate::manager::{ManagedInstance, Manager};
use morrow_core::{
    dispatch::HostRuntime,
    io,
    io_intent::{Command, Record},
    plugin_package::io::IoCapability,
};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct GrantState {
    resource: Weak<Resource>,
    manager: Weak<()>,
    deadline: crate::monotonic::Instant,
    expected: Command,
    revoked: AtomicBool,
    live: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}
impl GrantState {
    // Never locks a channel queue, calls a Store or performs external work.
    // The callback is trusted, bounded, pure and non-reentrant, including when
    // invoked under the original queue lock and a final Store commit guard.
    pub(super) fn check_authority(&self) -> Result<()> {
        if self.manager.upgrade().is_none() || self.revoked.load(Ordering::Acquire) {
            return Err(Error::Denied);
        }
        let resource = self.resource.upgrade().ok_or(Error::Denied)?;
        if crate::monotonic::Instant::now() >= self.deadline {
            return Err(Error::Expired);
        }
        if resource.control.upgrade().is_none_or(|control| !control.active()) {
            return Err(Error::Denied);
        }
        if self.live.as_ref().is_some_and(|live| !live()) {
            self.revoked.store(true, Ordering::Release);
            return Err(Error::Denied);
        }
        if self.manager.upgrade().is_none() || self.revoked.load(Ordering::Acquire) {
            return Err(Error::Denied);
        }
        if crate::monotonic::Instant::now() >= self.deadline {
            return Err(Error::Expired);
        }
        if resource.control.upgrade().is_none_or(|control| !control.active()) {
            return Err(Error::Denied);
        }
        Ok(())
    }
    // Business closure additionally denies starting/reading a native transport,
    // but normal channel EOF must remain an authorized Closed reply/completion.
    fn check(&self) -> Result<()> {
        self.check_authority()?;
        let resource = self.resource.upgrade().ok_or(Error::Denied)?;
        if resource.source_closed.load(Ordering::Acquire) {
            return Err(Error::Closed);
        }
        Ok(())
    }
}

/// Nonserializable host-issued source approval; deliberately without Debug.
/// Clones share only the same immutable command, owner and revocation state.
/// This does not authorize a guest HTTP import, perform a dispatch claim, retain
/// an original, reserve transport capacity, or establish any cleanup receipt.
#[derive(Clone)]
pub struct SourceGrant {
    state: Arc<GrantState>,
}
#[derive(Clone, Copy)]
enum SourceProfile {
    Sse,
    WebSocket,
}
impl SourceGrant {
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        broker: &ChannelBroker,
        expected_revision: u64,
        expected: Command,
        now: u64,
    ) -> Result<Self> {
        Self::issue_inner(manager, host, instance, broker, expected_revision, expected, now, SourceProfile::Sse, None, None)
    }
    /// Issue with an immutable restricting probe. This callback must not acquire
    /// channel/Store/clock locks or reenter grant/broker methods. A false probe
    /// permanently revokes authority; it cannot expand or renew the original grant.
    #[allow(clippy::too_many_arguments)]
    pub fn issue_with_live_guard(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        broker: &ChannelBroker,
        expected_revision: u64,
        expected: Command,
        now: u64,
        live: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Self> {
        Self::issue_inner(manager, host, instance, broker, expected_revision, expected, now, SourceProfile::Sse,
            None, Some(Arc::new(live)))
    }
    /// Restrict the source to one immutable absolute monotonic deadline. A later
    /// supplied instant is capped by the original channel deadline, never renewed.
    /// Expiry is typed separately from a failed pure approval probe (revocation).
    #[allow(clippy::too_many_arguments)]
    pub fn issue_with_deadline_and_live_guard(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        broker: &ChannelBroker,
        expected_revision: u64,
        expected: Command,
        now: u64,
        deadline: crate::monotonic::Instant,
        live: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Self> {
        Self::issue_inner(manager, host, instance, broker, expected_revision, expected, now, SourceProfile::Sse,
            Some(deadline), Some(Arc::new(live)))
    }
    /// Explicit host approval for a durable duplex WebSocket source. The old
    /// factories remain HTTP/SSE-only; no guest IO permission is derived here.
    /// The deadline and pure, bounded, non-reentrant probe can only restrict it.
    #[allow(clippy::too_many_arguments)]
    pub fn issue_websocket_with_deadline_and_live_guard(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        broker: &ChannelBroker,
        expected_revision: u64,
        expected: Command,
        now: u64,
        deadline: crate::monotonic::Instant,
        live: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Self> {
        Self::issue_inner(manager, host, instance, broker, expected_revision, expected, now,
            SourceProfile::WebSocket, Some(deadline), Some(Arc::new(live)))
    }
    #[allow(clippy::too_many_arguments)]
    fn issue_inner(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        broker: &ChannelBroker,
        expected_revision: u64,
        expected: Command,
        now: u64,
        profile: SourceProfile,
        deadline: Option<crate::monotonic::Instant>,
        live: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    ) -> Result<Self> {
        let (capability, duplex) = match profile {
            SourceProfile::Sse => (IoCapability::HttpRequest, false),
            SourceProfile::WebSocket => (IoCapability::WebSocketConnect, true),
        };
        let package = instance.package().package();
        // Authenticate without sampling/updating the legitimate broker clock.
        broker.owner(manager, host, instance)?;
        manager.validate_instance(host, instance).map_err(|_| Error::Denied)?;
        let selection = manager.selection(&package.manifest().package_id).ok_or(Error::Denied)?;
        if manager.revision() != expected_revision || !selection.enabled
            || selection.digest != package.digest()
            || expected.subject != package.manifest().package_id
            || expected.package_sha256 != package.digest()
            || expected.capability != capability
            || expected.protocol_sha256 != io::schema_digest()
        {
            return Err(Error::Denied);
        }
        if expected.request_bytes == 0 || expected.request_bytes > io::MAX_FRAME_BYTES as u64
            || expected.response_limit == 0
        {
            return Err(Error::Limit);
        }
        // Complete immutable command validation, including all nonzero digest
        // pins. Historical digests alone never establish fresh source approval.
        Record::prepared(expected.clone()).map_err(|_| Error::Invalid)?;
        // Both explicit network profiles require the original durable event ACK.
        // Old local Source/bind_channel combinations retain their semantics.
        if broker.resource.source.kind != morrow_core::channel::Kind::Events
            || broker.resource.source.duplex != duplex
            || broker.resource.source.checkpoint_scope.is_none()
        {
            return Err(Error::Denied);
        }
        if broker.resource.source_guard.lock().map_err(|_| Error::Unknown)?.is_some() {
            return Err(Error::Denied);
        }
        {
            let queue = broker.resource.queues.lock().map_err(|_| Error::Unknown)?;
            if queue.producer_state != ProducerState::NoProducer || queue.cause.is_some()
                || queue.eof || queue.frame.is_some() || queue.sent.is_some() || queue.next_sequence != 0
                || duplex && (queue.accepted_sequence != 0 || queue.observed_sequence != 0)
            {
                return Err(Error::Closed);
            }
        }
        let deadline = match deadline {
            Some(supplied) if supplied < broker.resource.deadline => supplied,
            _ => broker.resource.deadline,
        };
        if crate::monotonic::Instant::now() >= deadline {
            return Err(Error::Expired);
        }
        // Only an exact owner/selection/fully valid command can advance now.
        broker.gate(manager, host, instance, now)?;
        let state = Arc::new(GrantState {
            resource: Arc::downgrade(&broker.resource),
            manager: manager.identity(),
            deadline,
            expected,
            revoked: AtomicBool::new(false),
            live,
        });
        state.check()?;
        // Same queue -> source-guard lock order as the delivery/ACK check. The
        // factory can attach only before a producer or original frame exists.
        let mut queue = broker.resource.queues.lock().map_err(|_| Error::Unknown)?;
        broker.resource.check_locked(&mut queue)?;
        if queue.producer_state != ProducerState::NoProducer || queue.cause.is_some()
            || queue.eof || queue.frame.is_some() || queue.sent.is_some() || queue.next_sequence != 0
            || duplex && (queue.accepted_sequence != 0 || queue.observed_sequence != 0)
        {
            return Err(Error::Closed);
        }
        state.check()?;
        let mut installed = broker.resource.source_guard.lock().map_err(|_| Error::Unknown)?;
        if installed.is_some() {
            return Err(Error::Denied);
        }
        *installed = Some(super::SourceGuard::Network(Arc::clone(&state)));
        drop(installed);
        drop(queue);
        Ok(Self { state })
    }
    pub fn expected_command(&self) -> &Command {
        &self.state.expected
    }
    /// Pure fresh authority and native-business liveness check, without queue locks.
    /// Native reads reject normal resource closure; channel authorization preserves EOF.
    pub fn check(&self) -> Result<()> {
        self.state.check()
    }
    /// Exact original broker/manager/host/instance authentication. No network
    /// permission is derived from a replacement broker, endpoint or package.
    pub fn validate_owner(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        broker: &ChannelBroker,
    ) -> Result<()> {
        let resource = self.state.resource.upgrade().ok_or(Error::Denied)?;
        if !Arc::ptr_eq(&resource, &broker.resource) {
            return Err(Error::Denied);
        }
        broker.owner(manager, host, instance)?;
        manager.validate_instance(host, instance).map_err(|_| Error::Denied)?;
        self.check()
    }
    /// Publish revocation before waiting for the queue. This does not stop the
    /// guest Control or fabricate a producer/HTTP-worker join receipt.
    pub fn revoke(&self) {
        self.state.revoked.store(true, Ordering::Release);
        if let Some(resource) = self.state.resource.upgrade() {
            resource.close(morrow_core::channel::Status::Revoked);
        }
    }
}
