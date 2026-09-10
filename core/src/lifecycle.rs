//! Host-owned policy prototype. No caller identity is accepted from wire data.
//! Ticks must come from the host's monotonic clock, not a plugin request.
use crate::{Error, Result, content::CardRecord, identity, runtime::RenameRequest};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
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
struct GrantRecord {
    card_id: String,
    expires_at: u64,
}
struct InstanceRecord {
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
        if record.phase != InstancePhase::Preparing {
            return Err(Error::Invalid("ready transition"));
        }
        record.phase = InstancePhase::Ready;
        Ok(())
    }
    pub fn phase(&self, instance: Instance) -> Result<InstancePhase> {
        Ok(self.record(instance)?.phase)
    }
    pub fn grant_rename(
        &mut self,
        instance: Instance,
        card_id: &str,
        expires_at: u64,
        now: u64,
    ) -> Result<Grant> {
        self.tick(now)?;
        identity(card_id)?;
        let record = self.record(instance)?;
        if record.phase != InstancePhase::Ready || expires_at <= now {
            return Err(Error::Invalid("grant context"));
        }
        if record.grants.len() >= MAX_GRANTS {
            return Err(Error::Limit);
        }
        let serial = self.serial()?;
        self.record_mut(instance)?.grants.insert(
            serial,
            GrantRecord {
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
        if grant.instance != instance {
            return Err(Error::Invalid("grant owner"));
        }
        let record = self.record(instance)?;
        if record.phase != InstancePhase::Ready
            && !(completing && record.phase == InstancePhase::Draining)
        {
            return Err(Error::Invalid("inactive instance"));
        }
        let scope = record
            .grants
            .get(&grant.serial)
            .ok_or(Error::Invalid("revoked grant"))?;
        if scope.expires_at <= now || scope.card_id != request.card_id {
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
        self.tick(now)?;
        self.expire_drains(now)?;
        self.record(permit.instance)?;
        if !self.record(permit.instance)?.tasks.contains(&permit.serial) {
            return Err(Error::Invalid("stale task"));
        }
        self.record_mut(permit.instance)?
            .tasks
            .remove(&permit.serial);
        let task = self
            .tasks
            .remove(&permit.serial)
            .ok_or(Error::Invalid("stale task"))?;
        self.authorize(permit.instance, task.grant, &task.request, now, true)?;
        task.request.propose(card)
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
        if record.phase != InstancePhase::Ready || deadline <= now {
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
