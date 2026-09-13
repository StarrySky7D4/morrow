//! Host-owned policy prototype. No caller identity is accepted from wire data.
//! Ticks must come from the host's monotonic clock, not a plugin request.
use crate::{Error, Result, content::CardRecord, identity, runtime::RenameRequest};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
static NEXT_HOST: AtomicU64 = AtomicU64::new(1);
const MAX_INSTANCES: usize = 128;
const MAX_GRANTS: usize = 1024;
const MAX_TASKS: usize = 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstancePhase {
    Preparing,
    Ready,
    Draining,
    Revoked,
    Stopped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instance {
    host: u64,
    generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grant {
    instance: Instance,
    serial: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permit {
    instance: Instance,
    serial: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GrantKind {
    Rename,
    ReadSummary,
    QueryOperation,
    ReadAttachment,
    CreateContent,
    EditContent,
    ReadContent,
}
/// One-way trusted revocation signal. Never serialized or exported to guest SDKs.
#[derive(Clone)]
pub struct Revocation(Arc<AtomicBool>);
impl Revocation {
    pub fn revoke(&self) {
        self.0.store(true, Ordering::Release);
    }
    /// Read-only observation; this does not renew authority or undo revocation.
    pub fn is_revoked(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
struct GrantRecord {
    attachment_id: Option<String>,
    kind: GrantKind,
    card_id: String,
    expires_at: u64,
}
struct InstanceRecord {
    revocation: Revocation,
    phase: InstancePhase,
    drain_deadline: Option<u64>,
    grants: BTreeMap<u64, GrantRecord>,
    tasks: BTreeSet<u64>,
}
struct Task {
    grant: Grant,
    request: RenameRequest,
}
pub struct HostPolicy {
    id: u64,
    next: u64,
    last_tick: u64,
    instances: BTreeMap<u64, InstanceRecord>,
    tasks: BTreeMap<u64, Task>,
}
impl HostPolicy {
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub(crate) fn identity(&self) -> u64 {
        self.id
    }
    pub fn new() -> Result<Self> {
        let id = NEXT_HOST
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| Error::Limit)?;
        Ok(Self {
            id,
            next: 1,
            last_tick: 0,
            instances: BTreeMap::new(),
            tasks: BTreeMap::new(),
        })
    }
    fn serial(&mut self) -> Result<u64> {
        let value = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::Limit)?;
        Ok(value)
    }
    fn tick(&mut self, now: u64) -> Result<()> {
        if now < self.last_tick {
            return Err(Error::Invalid("clock regression"));
        }
        self.last_tick = now;
        Ok(())
    }
    fn record(&self, instance: Instance) -> Result<&InstanceRecord> {
        if instance.host != self.id {
            return Err(Error::Invalid("foreign instance"));
        }
        self.instances
            .get(&instance.generation)
            .ok_or(Error::Invalid("stale instance"))
    }
    fn record_mut(&mut self, instance: Instance) -> Result<&mut InstanceRecord> {
        self.record(instance)?;
        Ok(self.instances.get_mut(&instance.generation).unwrap())
    }
    pub fn activate(&mut self) -> Result<Instance> {
        if self.instances.len() >= MAX_INSTANCES {
            return Err(Error::Limit);
        }
        let generation = self.serial()?;
        self.instances.insert(
            generation,
            InstanceRecord {
                revocation: Revocation(Arc::new(AtomicBool::new(false))),
                phase: InstancePhase::Preparing,
                drain_deadline: None,
                grants: BTreeMap::new(),
                tasks: BTreeSet::new(),
            },
        );
        Ok(Instance {
            host: self.id,
            generation,
        })
    }
    pub fn ready(&mut self, instance: Instance) -> Result<()> {
        let record = self.record_mut(instance)?;
        if record.phase != InstancePhase::Preparing || record.revocation.is_revoked() {
            return Err(Error::Invalid("ready transition"));
        }
        record.phase = InstancePhase::Ready;
        Ok(())
    }
    pub fn phase(&self, instance: Instance) -> Result<InstancePhase> {
        let record = self.record(instance)?;
        Ok(
            if record.revocation.is_revoked() && record.phase != InstancePhase::Stopped {
                InstancePhase::Revoked
            } else {
                record.phase
            },
        )
    }
    /// Host control plane only. Existing operations recheck this before commit or response release.
    pub fn revocation(&self, instance: Instance) -> Result<Revocation> {
        Ok(self.record(instance)?.revocation.clone())
    }
    pub fn grant_rename(
        &mut self,
        instance: Instance,
        card_id: &str,
        expires_at: u64,
        now: u64,
    ) -> Result<Grant> {
        self.grant(instance, GrantKind::Rename, card_id, expires_at, now)
    }
    pub fn grant(
        &mut self,
        instance: Instance,
        kind: GrantKind,
        card_id: &str,
        expires_at: u64,
        now: u64,
    ) -> Result<Grant> {
        if kind == GrantKind::ReadAttachment {
            return Err(Error::Invalid("attachment scope required"));
        }
        self.grant_to(instance, kind, card_id, None, expires_at, now)
    }
    pub fn grant_attachment(
        &mut self,
        instance: Instance,
        card_id: &str,
        attachment_id: &str,
        expires_at: u64,
        now: u64,
    ) -> Result<Grant> {
        identity(attachment_id)?;
        self.grant_to(
            instance,
            GrantKind::ReadAttachment,
            card_id,
            Some(attachment_id),
            expires_at,
            now,
        )
    }
    fn grant_to(
        &mut self,
        instance: Instance,
        kind: GrantKind,
        card_id: &str,
        attachment_id: Option<&str>,
        expires_at: u64,
        now: u64,
    ) -> Result<Grant> {
        self.tick(now)?;
        identity(card_id)?;
        let record = self.record(instance)?;
        if record.phase != InstancePhase::Ready
            || record.revocation.is_revoked()
            || expires_at <= now
        {
            return Err(Error::Invalid("grant context"));
        }
        if record.grants.len() >= MAX_GRANTS {
            return Err(Error::Limit);
        }
        let serial = self.serial()?;
        self.record_mut(instance)?.grants.insert(
            serial,
            GrantRecord {
                attachment_id: attachment_id.map(str::to_owned),
                kind,
                card_id: card_id.into(),
                expires_at,
            },
        );
        Ok(Grant { instance, serial })
    }
    fn authorize(
        &self,
        instance: Instance,
        grant: Grant,
        request: &RenameRequest,
        now: u64,
        completing: bool,
    ) -> Result<()> {
        self.authorize_scope(
            instance,
            grant,
            (GrantKind::Rename, &request.card_id, None),
            now,
            completing,
        )
    }
    fn authorize_scope(
        &self,
        instance: Instance,
        grant: Grant,
        target: (GrantKind, &str, Option<&str>),
        now: u64,
        completing: bool,
    ) -> Result<()> {
        let (kind, card_id, attachment_id) = target;
        if grant.instance != instance {
            return Err(Error::Invalid("grant owner"));
        }
        let record = self.record(instance)?;
        if record.revocation.is_revoked()
            || (record.phase != InstancePhase::Ready
                && !(completing && record.phase == InstancePhase::Draining))
        {
            return Err(Error::Invalid("inactive instance"));
        }
        let scope = record
            .grants
            .get(&grant.serial)
            .ok_or(Error::Invalid("revoked grant"))?;
        if scope.expires_at <= now
            || scope.card_id != card_id
            || scope.kind != kind
            || scope.attachment_id.as_deref() != attachment_id
        {
            return Err(Error::Invalid("grant scope or expiry"));
        }
        Ok(())
    }
    pub fn begin(
        &mut self,
        instance: Instance,
        grant: Grant,
        request: &RenameRequest,
        now: u64,
    ) -> Result<Permit> {
        self.tick(now)?;
        request.validate()?;
        self.authorize(instance, grant, request, now, false)?;
        if self.tasks.len() >= MAX_TASKS {
            return Err(Error::Limit);
        }
        let serial = self.serial()?;
        self.tasks.insert(
            serial,
            Task {
                grant,
                request: request.clone(),
            },
        );
        self.record_mut(instance)?.tasks.insert(serial);
        Ok(Permit { instance, serial })
    }
    /// Rechecks scope on the same fixed request, consumes the task even on failure.
    /// Returns an in-memory proposal; database commit/audit are not implemented here.
    pub fn complete(&mut self, permit: Permit, card: &CardRecord, now: u64) -> Result<CardRecord> {
        let task = self.take_task(permit, now)?;
        self.authorize(permit.instance, task.grant, &task.request, now, true)?;
        task.request.propose(card)
    }
    fn take_task(&mut self, permit: Permit, now: u64) -> Result<Task> {
        self.tick(now)?;
        self.expire_drains(now)?;
        self.record(permit.instance)?;
        if !self
            .record_mut(permit.instance)?
            .tasks
            .remove(&permit.serial)
        {
            return Err(Error::Invalid("stale task"));
        }
        self.tasks
            .remove(&permit.serial)
            .ok_or(Error::Invalid("stale task"))
    }
    /// Sole plugin rename write entry. Hold this host exclusively until commit returns.
    /// The trusted host clock must be fresh/monotonic; no plugin callback is accepted.
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub fn commit_rename(
        &mut self,
        permit: Permit,
        store: &mut crate::store::Store,
        mut clock: impl FnMut() -> u64,
    ) -> Result<crate::transaction::Receipt> {
        let task = self.take_task(permit, clock())?;
        store.rename(&task.request, || {
            let now = clock();
            self.tick(now)?;
            self.expire_drains(now)?;
            self.authorize(permit.instance, task.grant, &task.request, now, true)
        })
    }
    /// Read permission is checked before lookup and again before releasing the projection.
    /// Caller holds the trusted host exclusively; clock is host-owned, never plugin code.
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub fn read_summary(
        &mut self,
        instance: Instance,
        grant: Grant,
        store: &crate::store::Store,
        card_id: &str,
        mut clock: impl FnMut() -> u64,
    ) -> Result<crate::content::CardSummary> {
        identity(card_id)?;
        let now = clock();
        self.expire_drains(now)?;
        self.authorize_scope(
            instance,
            grant,
            (GrantKind::ReadSummary, card_id, None),
            now,
            false,
        )?;
        let result = store
            .card(card_id)
            .map(|card| card.map(|value| value.summary()));
        let now = clock();
        self.expire_drains(now)?;
        self.authorize_scope(
            instance,
            grant,
            (GrantKind::ReadSummary, card_id, None),
            now,
            false,
        )?;
        result?.ok_or(Error::NotFound)
    }
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub fn query_operation(
        &mut self,
        instance: Instance,
        grant: Grant,
        store: &crate::store::Store,
        target: (&str, &str),
        mut clock: impl FnMut() -> u64,
    ) -> Result<crate::transaction::Lookup> {
        let (card_id, operation_id) = target;
        identity(card_id)?;
        identity(operation_id)?;
        let now = clock();
        self.expire_drains(now)?;
        self.authorize_scope(
            instance,
            grant,
            (GrantKind::QueryOperation, card_id, None),
            now,
            false,
        )?;
        let result = store.lookup_for_card(card_id, operation_id);
        let now = clock();
        self.expire_drains(now)?;
        self.authorize_scope(
            instance,
            grant,
            (GrantKind::QueryOperation, card_id, None),
            now,
            false,
        )?;
        result
    }
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub fn read_attachment(
        &mut self,
        instance: Instance,
        grant: Grant,
        store: &crate::store::Store,
        request: &crate::runtime::ReadAttachment,
        mut clock: impl FnMut() -> u64,
    ) -> Result<crate::attachment::AttachmentChunk> {
        request.validate()?;
        let target = (
            GrantKind::ReadAttachment,
            request.card_id.as_str(),
            Some(request.attachment_id.as_str()),
        );
        let now = clock();
        self.expire_drains(now)?;
        self.authorize_scope(instance, grant, target, now, false)?;
        let result = store.read_attachment_chunk(request);
        let now = clock();
        self.expire_drains(now)?;
        self.authorize_scope(instance, grant, target, now, false)?;
        result
    }
    /// Read complete owned content only for this exact granted object.
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub fn read_content(
        &mut self,
        instance: Instance,
        grant: Grant,
        store: &crate::store::Store,
        card: &str,
        mut clock: impl FnMut() -> u64,
    ) -> Result<CardRecord> {
        self.read_content_with(instance, grant, card, &mut clock, || {
            store.card(card)?.ok_or(Error::NotFound)
        })
    }
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub(crate) fn read_frozen_content(
        &mut self,
        instance: Instance,
        grant: Grant,
        card: &CardRecord,
        clock: impl FnMut() -> u64,
    ) -> Result<CardRecord> {
        let id = card.summary().id;
        self.read_content_with(instance, grant, &id, clock, || Ok(card.clone()))
    }
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    fn read_content_with(
        &mut self,
        instance: Instance,
        grant: Grant,
        card: &str,
        mut clock: impl FnMut() -> u64,
        read: impl FnOnce() -> Result<CardRecord>,
    ) -> Result<CardRecord> {
        identity(card)?;
        let now = clock();
        self.expire_drains(now)?;
        self.authorize_scope(
            instance,
            grant,
            (GrantKind::ReadContent, card, None),
            now,
            false,
        )?;
        let value = read();
        let now = clock();
        self.expire_drains(now)?;
        self.authorize_scope(
            instance,
            grant,
            (GrantKind::ReadContent, card, None),
            now,
            false,
        )?;
        value
    }
    /// Proposal is host-routed after guest completion; no privileged plugin shortcut.
    /// Recheck the grant inside the transaction and immediately before commit.
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub fn edit_content(
        &mut self,
        instance: Instance,
        grant: Grant,
        store: &mut crate::store::Store,
        change: &crate::content_change::ContentChange,
        clock: impl FnMut() -> u64,
    ) -> Result<crate::transaction::Receipt> {
        self.edit_content_guarded(instance, grant, store, change, clock, |_| Ok(()))
    }
    /// Additional trusted proposal constraints, checked at every Store authorization
    /// boundary, including duplicate receipt delivery and the final pre-commit check.
    /// The guard sees the same clock value as scope authorization and cannot replace it.
    /// Once committed, later denial cannot roll back the recorded operation.
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub fn edit_content_guarded(
        &mut self,
        instance: Instance,
        grant: Grant,
        store: &mut crate::store::Store,
        change: &crate::content_change::ContentChange,
        clock: impl FnMut() -> u64,
        guard: impl FnMut(u64) -> Result<()>,
    ) -> Result<crate::transaction::Receipt> {
        self.edit_content_guarded_with_evidence(instance, grant, store, change, &[], clock, guard)
    }
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    #[allow(clippy::too_many_arguments)]
    pub fn edit_content_guarded_with_evidence(
        &mut self,
        instance: Instance,
        grant: Grant,
        store: &mut crate::store::Store,
        change: &crate::content_change::ContentChange,
        evidence: &[crate::task_evidence::Evidence],
        mut clock: impl FnMut() -> u64,
        mut guard: impl FnMut(u64) -> Result<()>,
    ) -> Result<crate::transaction::Receipt> {
        change.validate()?;
        store.edit_content_with_evidence(change, evidence, || {
            let now = clock();
            self.expire_drains(now)?;
            guard(now)?;
            // Retain the original scope check after the guard, so even a trusted guard
            // that triggers connection revocation cannot authorize the content itself.
            self.authorize_scope(
                instance,
                grant,
                (GrantKind::EditContent, &change.card_id, None),
                now,
                false,
            )
        })
    }
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub fn create_content(
        &mut self,
        instance: Instance,
        grant: Grant,
        store: &mut crate::store::Store,
        operation: &str,
        card: &CardRecord,
        clock: impl FnMut() -> u64,
    ) -> Result<crate::transaction::Receipt> {
        self.create_content_with_evidence(instance, grant, store, operation, card, &[], clock)
    }
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    #[allow(clippy::too_many_arguments)]
    pub fn create_content_with_evidence(
        &mut self,
        instance: Instance,
        grant: Grant,
        store: &mut crate::store::Store,
        operation: &str,
        card: &CardRecord,
        evidence: &[crate::task_evidence::Evidence],
        mut clock: impl FnMut() -> u64,
    ) -> Result<crate::transaction::Receipt> {
        identity(operation)?;
        let id = card.summary().id;
        store.create_authorized_with_evidence(operation, card, evidence, || {
            let now = clock();
            self.expire_drains(now)?;
            self.authorize_scope(
                instance,
                grant,
                (GrantKind::CreateContent, &id, None),
                now,
                false,
            )
        })
    }
    pub fn revoke(&mut self, grant: Grant) -> Result<()> {
        self.record_mut(grant.instance)?
            .grants
            .remove(&grant.serial)
            .ok_or(Error::Invalid("stale grant"))?;
        Ok(())
    }
    pub fn drain(&mut self, instance: Instance, deadline: u64, now: u64) -> Result<()> {
        self.tick(now)?;
        let record = self.record_mut(instance)?;
        if record.phase != InstancePhase::Ready || record.revocation.is_revoked() || deadline <= now
        {
            return Err(Error::Invalid("drain transition"));
        }
        record.phase = InstancePhase::Draining;
        record.drain_deadline = Some(deadline);
        Ok(())
    }
    /// The host timer must call this; late completion also checks the deadline.
    pub fn expire_drains(&mut self, now: u64) -> Result<usize> {
        self.tick(now)?;
        let expired: Vec<_> = self
            .instances
            .iter()
            .filter_map(|(&generation, record)| {
                (record.phase == InstancePhase::Draining
                    && record
                        .drain_deadline
                        .is_some_and(|deadline| now >= deadline))
                .then_some(Instance {
                    host: self.id,
                    generation,
                })
            })
            .collect();
        for instance in &expired {
            self.safety_stop(*instance)?;
        }
        Ok(expired.len())
    }
    pub fn cancel(&mut self, permit: Permit) -> Result<()> {
        let record = self.record_mut(permit.instance)?;
        if !record.tasks.remove(&permit.serial) {
            return Err(Error::Invalid("stale task"));
        }
        self.tasks.remove(&permit.serial);
        Ok(())
    }
    pub fn safety_stop(&mut self, instance: Instance) -> Result<()> {
        let record = self.record_mut(instance)?;
        if record.phase == InstancePhase::Stopped {
            return Ok(());
        }
        // Revoke access before releasing task records. No plugin callback is involved.
        record.revocation.revoke();
        record.phase = InstancePhase::Revoked;
        record.grants.clear();
        let tasks = std::mem::take(&mut record.tasks);
        for task in tasks {
            self.tasks.remove(&task);
        }
        Ok(())
    }
    pub fn stop(&mut self, instance: Instance) -> Result<()> {
        let record = self.record_mut(instance)?;
        if !matches!(
            record.phase,
            InstancePhase::Draining | InstancePhase::Revoked
        ) || !record.tasks.is_empty()
        {
            return Err(Error::Invalid("tasks not drained"));
        }
        record.grants.clear();
        record.phase = InstancePhase::Stopped;
        Ok(())
    }
    pub fn retire(&mut self, instance: Instance) -> Result<()> {
        if self.phase(instance)? != InstancePhase::Stopped {
            return Err(Error::Invalid("instance not stopped"));
        }
        self.instances.remove(&instance.generation);
        Ok(())
    }
}

/// Observed milestones for one operation; this is not a durable journal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitState {
    NotSubmitted,
    AwaitingResult,
    LocallyCommitted,
    Sealed,
    Witnessed,
    NotCommitted,
}
impl CommitState {
    pub fn submitted(self) -> Result<Self> {
        if self == Self::NotSubmitted {
            Ok(Self::AwaitingResult)
        } else {
            Err(Error::Invalid("already submitted"))
        }
    }
    pub fn timeout(self) -> Self {
        self
    }
    /// Call only after a storage query proves the result, never just on timeout.
    pub fn resolved(self, committed: bool) -> Result<Self> {
        if self != Self::AwaitingResult {
            return Err(Error::Invalid("not awaiting result"));
        }
        Ok(if committed {
            Self::LocallyCommitted
        } else {
            Self::NotCommitted
        })
    }
    pub fn sealed(self) -> Result<Self> {
        if self == Self::LocallyCommitted {
            Ok(Self::Sealed)
        } else {
            Err(Error::Invalid("not locally committed"))
        }
    }
    pub fn witnessed(self) -> Result<Self> {
        if self == Self::Sealed {
            Ok(Self::Witnessed)
        } else {
            Err(Error::Invalid("not sealed"))
        }
    }
}
