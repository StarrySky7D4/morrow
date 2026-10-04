//! Explicit, receiver-bound metadata-only finite catch-up from the original Store.
//! Approval is a trusted-host action, never inferred from a content grant, digest,
//! guest cursor, or channel declaration. No Store borrow crosses producer spawn.
use super::{ChannelBroker, Error, ProducerState, Result, Source, SourceGuard};
use crate::{
    manager::{Control, ManagedInstance, Manager},
    monotonic::Instant,
};
use morrow_core::{
    changes_metadata::{self, Metadata},
    channel::{Action, Budget, Frame, Kind, Request, Response, Status},
    dispatch::{ConnectionBinding, HostBinding, HostRuntime},
    lifecycle::{ChangesReceiverLiveness, ContentAuthorization},
    store::{ChangesBatch, ChangesBudget, ChangesStart, ChangesStoreBinding},
};
use std::{
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// Trusted host selects the start. A guest-supplied cursor is deliberately absent.
/// LatestAck opens a newly approved finite window after the latest durable receipt;
/// it does not restore an old grant, reference, or hidden upper bound.
#[derive(Clone, Copy)]
pub enum HistoryStart {
    Beginning,
    LatestAck {
        subscription: [u8; 32],
        source_epoch: [u8; 32],
    },
}

struct ApprovalState {
    manager: Weak<()>,
    revision_lease: Arc<AtomicBool>,
    revision: u64,
    host: HostBinding,
    connection: ConnectionBinding,
    control: Weak<Control>,
    receiver_liveness: ChangesReceiverLiveness,
    store_binding: ChangesStoreBinding,
    package: [u8; 32],
    cards: Vec<String>,
    scope: [u8; 32],
    budget: ChangesBudget,
    issued: Instant,
    issued_tick: u64,
    expires: u64,
    deadline: Instant,
    revoked: AtomicBool,
    restrictions: Vec<ContentAuthorization>,
}
impl ApprovalState {
    // Bounded atomics, Weak upgrades, and monotonic reads only. No Store, queue,
    // cancellation-clock mutex, user callback, or host reentry at final commit.
    fn check(&self) -> Result<()> {
        if self.manager.upgrade().is_none()
            || self.revision_lease.load(Ordering::Acquire)
            || self.revoked.load(Ordering::Acquire)
            || self.receiver_liveness.check().is_err()
            || self.store_binding.check_live().is_err()
            || self
                .control
                .upgrade()
                .is_none_or(|control| !control.changes_active())
        {
            return Err(Error::Denied);
        }
        let now = Instant::now();
        if now >= self.deadline {
            return Err(Error::Expired);
        }
        let elapsed = now
            .checked_duration_since(self.issued)
            .ok_or(Error::Expired)?
            .as_millis();
        let elapsed = u64::try_from(elapsed).map_err(|_| Error::Expired)?;
        let tick = self
            .issued_tick
            .checked_add(elapsed)
            .ok_or(Error::Expired)?;
        if tick >= self.expires {
            return Err(Error::Expired);
        }
        for restriction in &self.restrictions {
            if restriction.check(tick).is_err() {
                self.revoked.store(true, Ordering::Release);
                return Err(Error::Denied);
            }
        }
        // Recheck after all object probes; any single restriction kills the set.
        if self.revoked.load(Ordering::Acquire)
            || self.revision_lease.load(Ordering::Acquire)
            || self.receiver_liveness.check().is_err()
            || self.store_binding.check_live().is_err()
            || self
                .control
                .upgrade()
                .is_none_or(|control| !control.changes_active())
        {
            return Err(Error::Denied);
        }
        if Instant::now() >= self.deadline {
            return Err(Error::Expired);
        }
        Ok(())
    }
    fn owner(
        &self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
    ) -> Result<()> {
        if !Weak::ptr_eq(&self.manager, &manager.identity())
            || self.revision != manager.revision()
            || self.host != host.binding()
            || self.connection != instance.connection().binding()
            || !Weak::ptr_eq(&self.control, &Arc::downgrade(instance.io_control()))
            || self.package != instance.package().package().digest()
            || instance.connection().package_digest() != Some(self.package)
        {
            return Err(Error::Denied);
        }
        manager
            .validate_instance(host, instance)
            .map_err(|_| Error::Denied)?;
        self.validate_store(host)?;
        self.check()
    }
    fn validate_store(&self, host: &HostRuntime) -> Result<()> {
        if host
            .store_local()
            .validate_changes_store_binding(&self.store_binding)
            .is_err()
        {
            // Substitution cannot be undone to revive this original approval.
            self.revoked.store(true, Ordering::Release);
            return Err(Error::Denied);
        }
        Ok(())
    }
}
struct Item {
    bytes: Vec<u8>,
    cursor: [u8; 32],
}
pub(super) struct GrantState {
    approval: Arc<ApprovalState>,
    items: Arc<Vec<Item>>,
    deadline: Instant,
    #[cfg(feature = "fault-injection")]
    final_commit_revoke: AtomicBool,
    #[cfg(feature = "fault-injection")]
    final_commit_wait_deadline: AtomicBool,
    #[cfg(feature = "fault-injection")]
    final_commit_deadline_observed: AtomicBool,
}
impl GrantState {
    // Invoked only from broker owner preflight, outside queue/Store transactions.
    pub(super) fn validate_store(&self, host: &HostRuntime) -> Result<()> {
        self.approval.validate_store(host)
    }
    pub(super) fn check_authority(&self) -> Result<()> {
        if Instant::now() >= self.deadline {
            return Err(Error::Expired);
        }
        self.approval.check()
    }
    pub(super) fn check_frame(&self, sequence: u64, bytes: &[u8], cursor: &[u8]) -> Result<()> {
        self.check_authority()?;
        let offset = sequence.checked_sub(1).ok_or(Error::Invalid)?;
        let item = self
            .items
            .get(usize::try_from(offset).map_err(|_| Error::Invalid)?)
            .ok_or(Error::Invalid)?;
        if item.bytes != bytes || item.cursor != cursor {
            return Err(Error::Invalid);
        }
        Ok(())
    }
    #[cfg(feature = "fault-injection")]
    pub(super) fn before_final_commit(&self) {
        if self
            .final_commit_wait_deadline
            .swap(false, Ordering::AcqRel)
        {
            // Synthetic qualification only, armed for at most one second. This
            // waits for the actual original deadline after both SQL writes.
            let remaining = self
                .deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_default();
            std::thread::sleep(remaining + Duration::from_millis(1));
            self.final_commit_deadline_observed
                .store(true, Ordering::Release);
        }
        if self.final_commit_revoke.swap(false, Ordering::AcqRel) {
            self.approval.revoked.store(true, Ordering::Release);
        }
    }
}

/// Nonserializable, nonclone approval plus one owned bounded Store batch. All
/// fields are private: callers cannot replace its source, range, cards, or bytes.
pub struct ChangesApproval {
    state: Arc<ApprovalState>,
    batch: ChangesBatch,
}
/// A restricting revocation handle. Revoking any fixed card closes the whole set.
/// It cannot authorize another receiver, renew a deadline, or materialize a batch.
#[derive(Clone)]
pub struct ChangesRevocation {
    state: Arc<ApprovalState>,
}
impl ChangesRevocation {
    pub fn revoke(&self) {
        self.state.revoked.store(true, Ordering::Release);
    }
    pub fn revoke_card(&self, card_id: &str) -> Result<()> {
        if !self.state.cards.iter().any(|card| card == card_id) {
            return Err(Error::Denied);
        }
        self.revoke();
        Ok(())
    }
}

fn core_error(error: morrow_core::Error) -> Error {
    match error {
        morrow_core::Error::Limit => Error::Limit,
        _ => Error::Denied,
    }
}
fn check_core(state: &ApprovalState) -> morrow_core::Result<()> {
    state
        .check()
        .map_err(|_| morrow_core::Error::Invalid("inactive metadata source"))
}
fn profile(instance: &ManagedInstance) -> Result<()> {
    let package = instance.package().package();
    let manifest = package.manifest();
    if manifest.guest_abi_version != 2
        || !manifest
            .required_features
            .iter()
            .any(|f| f == changes_metadata::FEATURE)
        || !manifest
            .required_features
            .iter()
            .any(|f| f == morrow_core::channel::FEATURE)
        || !package.capabilities().is_empty()
        || package.io_declaration().is_some()
        || !package.io_capabilities().is_empty()
        || package.mutation_enabled()
        || package.channel_declaration().is_none_or(|d| d.kinds != [2])
        || manifest.required_features.iter().any(|f| {
            matches!(
                f.as_str(),
                "dependencies-v1"
                    | "dependency-calls-v1"
                    | "service-run-v1"
                    | "service-run-budget-v1"
            )
        })
    {
        return Err(Error::Denied);
    }
    Ok(())
}
/// Authenticate every original durable correlation, including latest checkpoint.
/// Returned metadata is only a bounded lookup anchor; Core independently verifies
/// the real permanent card+operation commit before resolving the new lower bound.
fn latest_anchor(
    host: &HostRuntime,
    subscription: [u8; 32],
    epoch: [u8; 32],
    scope: [u8; 32],
) -> Result<Metadata> {
    let store = host.store_local();
    let checkpoint = store
        .channel_checkpoint(&subscription, &epoch)
        .map_err(core_error)?
        .ok_or(Error::Denied)?;
    let receipt = store
        .channel_ack_receipt(&subscription, &epoch, checkpoint.sequence)
        .map_err(core_error)?
        .ok_or(Error::Denied)?;
    if receipt.checkpoint != checkpoint {
        return Err(Error::Denied);
    }
    let frame = Frame::decode(&receipt.frame_wire).map_err(core_error)?;
    let request = Request::decode(&receipt.request_wire).map_err(core_error)?;
    let response = Response::decode(&receipt.response_wire).map_err(core_error)?;
    response.validate_for(&request).map_err(core_error)?;
    let Action::Ack {
        sequence,
        frame_sha256,
        cursor,
    } = &request.action
    else {
        return Err(Error::Denied);
    };
    if response.status != Status::Acked
        || response.last_acked != checkpoint.sequence
        || response.frame.is_some()
        || response.accepted_sequence != 0
        || request.source_epoch != epoch
        || frame.source_epoch != epoch
        || frame.sequence != checkpoint.sequence
        || *sequence != checkpoint.sequence
        || frame.digest().map_err(core_error)? != checkpoint.frame_sha256
        || *frame_sha256 != checkpoint.frame_sha256
        || *cursor != checkpoint.cursor
        || frame.cursor != checkpoint.cursor
    {
        return Err(Error::Denied);
    }
    let metadata = Metadata::decode(&frame.bytes).map_err(core_error)?;
    if metadata.window_id != epoch
        || metadata.scope_digest != scope
        || metadata.cursor().map_err(core_error)?.as_slice() != frame.cursor
    {
        return Err(Error::Denied);
    }
    Ok(metadata)
}
impl ChangesApproval {
    /// This call itself is the new explicit trusted-host metadata history approval
    /// for the actual receiver. Optional original content probes only restrict it;
    /// they never imply permission to share with this package or select a card.
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        expected_revision: u64,
        cards: Vec<String>,
        start: HistoryStart,
        budget: ChangesBudget,
        expires: u64,
        now: u64,
        restrictions: Vec<ContentAuthorization>,
    ) -> Result<Self> {
        profile(instance)?;
        manager
            .validate_instance(host, instance)
            .map_err(|_| Error::Denied)?;
        let package = instance.package().package();
        let selection = manager
            .selection(&package.manifest().package_id)
            .ok_or(Error::Denied)?;
        if manager.revision() != expected_revision
            || !selection.enabled
            || selection.digest != package.digest()
            || instance.connection().package_digest() != Some(package.digest())
            || restrictions.len() > changes_metadata::MAX_CARDS
        {
            return Err(Error::Denied);
        }
        let requested_duration = expires
            .checked_sub(now)
            .filter(|v| *v > 0)
            .ok_or(Error::Expired)?;
        if requested_duration > budget.max_duration_ms {
            return Err(Error::Limit);
        }
        // Restrictions retain their original host-tick expiry. Map the earliest
        // one once onto this issuance's monotonic timeline; no later bind renews it.
        let expires = restrictions
            .iter()
            .fold(expires, |limit, probe| limit.min(probe.expires_at()));
        let duration = expires
            .checked_sub(now)
            .filter(|v| *v > 0)
            .ok_or(Error::Expired)?;
        let issued = Instant::now();
        let deadline = issued
            .checked_add(Duration::from_millis(duration))
            .ok_or(Error::Limit)?;
        let scope = host
            .store_local()
            .changes_scope_digest(package.digest(), &cards)
            .map_err(core_error)?;
        let state = Arc::new(ApprovalState {
            manager: manager.identity(),
            revision_lease: manager.changes_revision_guard(),
            revision: expected_revision,
            host: host.binding(),
            connection: instance.connection().binding(),
            control: Arc::downgrade(instance.io_control()),
            receiver_liveness: host
                .changes_receiver_liveness(instance.connection())
                .map_err(core_error)?,
            store_binding: host
                .store_local()
                .changes_store_binding()
                .map_err(core_error)?,
            package: package.digest(),
            cards,
            scope,
            budget,
            issued,
            issued_tick: now,
            expires,
            deadline,
            revoked: AtomicBool::new(false),
            restrictions,
        });
        state.check()?;
        let start = match start {
            HistoryStart::Beginning => ChangesStart::Beginning,
            HistoryStart::LatestAck {
                subscription,
                source_epoch,
            } => ChangesStart::After(latest_anchor(host, subscription, source_epoch, scope)?),
        };
        state.check()?;
        let window = host
            .store_local()
            .open_changes_window(state.package, &state.cards, start, budget, || {
                check_core(&state)
            })
            .map_err(core_error)?;
        let batch = host
            .store_local()
            .materialize_changes_window(window, || check_core(&state))
            .map_err(core_error)?;
        state.check()?;
        Ok(Self { state, batch })
    }
    pub fn revocation(&self) -> ChangesRevocation {
        ChangesRevocation {
            state: Arc::clone(&self.state),
        }
    }
    /// Consume this exact owned batch and install the source guard before producer
    /// creation. No frame becomes observable before installation and fresh approval.
    #[allow(clippy::too_many_arguments)]
    pub fn bind(
        self,
        manager: &Manager,
        host: &HostRuntime,
        instance: &ManagedInstance,
        subscription: [u8; 32],
        channel_budget: Budget,
        now: u64,
    ) -> Result<ChangesSource> {
        self.state.owner(manager, host, instance)?;
        if now < self.state.issued_tick || now >= self.state.expires || subscription == [0; 32] {
            return Err(Error::Denied);
        }
        host.store_local()
            .validate_changes_batch(&self.batch, self.state.package, &self.state.cards)
            .map_err(core_error)?;
        let changes = self.batch.changes();
        let output_bytes = changes
            .iter()
            .try_fold(0usize, |sum, c| {
                sum.checked_add(
                    changes_metadata::HEADER_BYTES + c.card_id.len() + c.operation_id.len(),
                )
            })
            .ok_or(Error::Limit)?;
        if changes.len() as u64 > channel_budget.max_messages
            || output_bytes as u64 > channel_budget.max_bytes
            || changes.iter().any(|c| {
                changes_metadata::HEADER_BYTES + c.card_id.len() + c.operation_id.len()
                    > channel_budget.max_frame_bytes as usize
            })
        {
            return Err(Error::Limit);
        }
        let broker = manager
            .bind_channel(
                host,
                instance,
                self.state.package,
                self.state.revision,
                Source {
                    kind: Kind::Events,
                    duplex: false,
                    checkpoint_scope: Some(subscription),
                },
                channel_budget,
                self.state.expires,
                now,
            )
            .map_err(|_| Error::Denied)?;
        let deadline = self
            .state
            .deadline
            .min(self.batch.deadline())
            .min(broker.resource.deadline);
        let metadata = self
            .batch
            .into_metadata(broker.endpoint().source_epoch)
            .map_err(core_error)?;
        let items = metadata
            .into_iter()
            .map(|item| {
                Ok(Item {
                    bytes: item.encode().map_err(core_error)?,
                    cursor: item.cursor().map_err(core_error)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let items = Arc::new(items);
        let grant = Arc::new(GrantState {
            approval: self.state,
            items: Arc::clone(&items),
            deadline,
            #[cfg(feature = "fault-injection")]
            final_commit_revoke: AtomicBool::new(false),
            #[cfg(feature = "fault-injection")]
            final_commit_wait_deadline: AtomicBool::new(false),
            #[cfg(feature = "fault-injection")]
            final_commit_deadline_observed: AtomicBool::new(false),
        });
        grant.check_authority()?;
        {
            let queue = broker.resource.queues.lock().map_err(|_| Error::Unknown)?;
            if queue.producer_state != ProducerState::NoProducer
                || queue.cause.is_some()
                || queue.frame.is_some()
                || queue.sent.is_some()
                || queue.eof
                || queue.next_sequence != 0
            {
                return Err(Error::Closed);
            }
            let mut installed = broker
                .resource
                .source_guard
                .lock()
                .map_err(|_| Error::Unknown)?;
            if installed.is_some() {
                return Err(Error::Denied);
            }
            grant.check_authority()?;
            *installed = Some(SourceGuard::Changes(Arc::clone(&grant)));
        }
        let source = ChangesSource { broker, grant };
        source.broker.spawn(move |producer| {
            for item in items.iter() {
                // Producer performs the typed gate and records its terminal cause
                // before every release; an early adapter return must not race it.
                if producer
                    .push_and_wait_acked(item.bytes.clone(), item.cursor.to_vec())
                    .is_err()
                {
                    return;
                }
            }
            let _ = producer.finish();
        })?;
        Ok(source)
    }
}
/// Retains the exact original broker and source approval. Completion/ACK/join are
/// still separate observations on the existing channel snapshot/cleanup handle.
pub struct ChangesSource {
    broker: ChannelBroker,
    grant: Arc<GrantState>,
}
impl ChangesSource {
    pub fn broker(&self) -> &ChannelBroker {
        &self.broker
    }
    pub fn revocation(&self) -> ChangesRevocation {
        ChangesRevocation {
            state: Arc::clone(&self.grant.approval),
        }
    }
    pub fn revoke(&self) {
        self.revocation().revoke();
        self.broker.resource.close(Status::Revoked);
    }
    pub fn scope_digest(&self) -> [u8; 32] {
        self.grant.approval.scope
    }
    pub fn read_budget(&self) -> ChangesBudget {
        self.grant.approval.budget
    }
    /// Synthetic qualification seam: cause actual revocation at the final original
    /// Store callback, after both ACK rows have been written in its transaction.
    #[cfg(feature = "fault-injection")]
    pub fn revoke_at_final_commit_for_fault_test(&self) {
        self.grant
            .final_commit_revoke
            .store(true, Ordering::Release);
    }
    /// Synthetic timing seam only: wait at the real second Store guard until the
    /// original deadline expires. Never changes or renews any source deadline.
    #[cfg(feature = "fault-injection")]
    pub fn expire_at_final_commit_for_fault_test(&self) -> Result<()> {
        let remaining = self
            .grant
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(Error::Expired)?;
        if remaining > Duration::from_secs(1) {
            return Err(Error::Limit);
        }
        self.grant
            .final_commit_wait_deadline
            .store(true, Ordering::Release);
        Ok(())
    }
    #[cfg(feature = "fault-injection")]
    pub fn final_commit_expiry_observed_for_fault_test(&self) -> bool {
        self.grant
            .final_commit_deadline_observed
            .load(Ordering::Acquire)
    }
}
impl Drop for ChangesSource {
    fn drop(&mut self) {
        // Signal before attempting queue acquisition. Cleanup retains real join
        // ownership; dropping this adapter never fabricates a Joined observation.
        self.grant.approval.revoked.store(true, Ordering::Release);
        self.broker.resource.close(Status::Closed);
        let _ = self.broker.resource.reap();
    }
}
