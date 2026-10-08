//! In-memory obligations of the original owner. No authority is restored from data.
//! A caller drop parks actual objects; only explicit original cleanup retires a slot.
use super::AgentContext;
use crate::Result;
use morrow_agent_session_exec_v1_r2::authority::{Admission, SessionExecHost};
use morrow_codex_session_exec_windows_v1::BorrowedNativeResources;
use morrow_core::dispatch::{Connection, HostBinding, HostRuntime};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub(crate) const MAX_CONTEXTS: usize = 16;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Prepared,
    External,
    Attached,
    Abandoned,
    Cleanup,
}
#[derive(Clone)]
pub(super) struct Original {
    pub host: Arc<SessionExecHost>,
    pub connection: Arc<Connection>,
    pub admission: Admission,
    pub resources: Arc<BorrowedNativeResources>,
}
struct Entry {
    original: Original,
    phase: Phase,
    parked: Option<AgentContext>,
    authority_retired: bool,
    requires_join: bool,
    joined: bool,
}
struct Entries {
    next: u64,
    records: BTreeMap<u64, Entry>,
}
pub(crate) struct ContextLifecycleLedger {
    binding: HostBinding,
    entries: Mutex<Entries>,
}
pub(super) struct Ticket {
    ledger: Arc<ContextLifecycleLedger>,
    id: u64,
    original: Original,
}
impl ContextLifecycleLedger {
    /// Stable original Core identity, including while the owner is moved or cleanup is owed.
    /// Identity alone neither authorizes work nor certifies cleanup.
    pub(crate) const fn owner_binding(&self) -> HostBinding {
        self.binding
    }
    pub(crate) fn new(binding: HostBinding) -> Arc<Self> {
        Arc::new(Self {
            binding,
            entries: Mutex::new(Entries {
                next: 1,
                records: BTreeMap::new(),
            }),
        })
    }
    pub(crate) fn ensure_capacity(&self) -> Result<()> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?;
        if entries.records.len() >= MAX_CONTEXTS || entries.next == u64::MAX {
            return Err("original prepared context quota exhausted".into());
        }
        Ok(())
    }
    pub(crate) fn outstanding(&self) -> bool {
        self.entries
            .lock()
            .map(|e| !e.records.is_empty())
            .unwrap_or(true)
    }
    pub(crate) fn needs_repair(&self) -> bool {
        self.entries
            .lock()
            .map(|e| {
                e.records
                    .values()
                    .any(|r| matches!(r.phase, Phase::Abandoned | Phase::Cleanup))
            })
            .unwrap_or(true)
    }
    pub(super) fn register(self: &Arc<Self>, context: &mut AgentContext) -> Result<()> {
        if context.lifecycle.is_some() || context.resources.owner_binding() != self.binding {
            return Err("foreign context ledger registration".into());
        }
        self.ensure_capacity()?;
        let original = Original {
            host: context.host.clone(),
            connection: context.executor_connection.clone(),
            admission: context.executor_admission.clone(),
            resources: context.resources.clone(),
        };
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?;
        // All registrations happen on the same exclusive owner, but enforce the bound here too.
        if entries.records.len() >= MAX_CONTEXTS || entries.next == u64::MAX {
            return Err("original prepared context quota exhausted".into());
        }
        let id = entries.next;
        entries.next += 1;
        entries.records.insert(
            id,
            Entry {
                original: original.clone(),
                phase: Phase::Prepared,
                parked: None,
                authority_retired: false,
                requires_join: false,
                joined: false,
            },
        );
        context.lifecycle = Some(Ticket {
            ledger: self.clone(),
            id,
            original,
        });
        Ok(())
    }
    pub(crate) fn validate_external(
        self: &Arc<Self>,
        context: &AgentContext,
        binding: HostBinding,
    ) -> Result<()> {
        let ticket = context
            .lifecycle
            .as_ref()
            .ok_or("context was not issued by this owner")?;
        if !Arc::ptr_eq(self, &ticket.ledger) || binding != self.binding {
            return Err("foreign prepared context".into());
        }
        ticket.validate(context, Phase::External)
    }
    /// Take only abandoned actual objects. The slot remains charged while cleanup is attempted.
    pub(crate) fn take_abandoned(
        self: &Arc<Self>,
        binding: HostBinding,
    ) -> Result<Option<AgentContext>> {
        if binding != self.binding {
            return Err("foreign context repair owner".into());
        }
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?;
        let Some((&id, entry)) = entries
            .records
            .iter_mut()
            .find(|(_, r)| r.phase == Phase::Abandoned && r.parked.is_some())
        else {
            return Ok(None);
        };
        let mut context = entry.parked.take().ok_or("abandoned context unavailable")?;
        entry.phase = Phase::Cleanup;
        context.lifecycle = Some(Ticket {
            ledger: self.clone(),
            id,
            original: entry.original.clone(),
        });
        Ok(Some(context))
    }
}
impl Ticket {
    fn validate(&self, context: &AgentContext, phase: Phase) -> Result<()> {
        let entries = self
            .ledger
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?;
        let record = entries
            .records
            .get(&self.id)
            .ok_or("retired context ticket")?;
        if record.phase != phase
            || record.parked.is_some()
            || !Arc::ptr_eq(&context.host, &self.original.host)
            || !Arc::ptr_eq(&context.executor_connection, &self.original.connection)
            || !Arc::ptr_eq(&context.resources, &self.original.resources)
        {
            return Err("prepared context identity changed".into());
        }
        Ok(())
    }
    pub(super) fn publish(&self, context: &AgentContext) -> Result<()> {
        self.validate(context, Phase::Prepared)?;
        self.ledger
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?
            .records
            .get_mut(&self.id)
            .ok_or("retired context ticket")?
            .phase = Phase::External;
        Ok(())
    }
    pub(super) fn attach(&self, context: &AgentContext) -> Result<()> {
        self.validate(context, Phase::External)?;
        self.ledger
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?
            .records
            .get_mut(&self.id)
            .ok_or("retired context ticket")?
            .phase = Phase::Attached;
        Ok(())
    }
    pub(super) fn detach_before_spawn(&self) -> Result<()> {
        let mut entries = self
            .ledger
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?;
        let record = entries
            .records
            .get_mut(&self.id)
            .ok_or("retired context ticket")?;
        if record.phase != Phase::Attached || record.requires_join || record.parked.is_some() {
            return Err("context cannot be returned before spawn".into());
        }
        record.phase = Phase::External;
        Ok(())
    }
    pub(super) fn cleanup(&self) -> Result<()> {
        let mut entries = self
            .ledger
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?;
        let record = entries
            .records
            .get_mut(&self.id)
            .ok_or("retired context ticket")?;
        if record.parked.is_some() {
            return Err("context already parked".into());
        }
        record.phase = Phase::Cleanup;
        Ok(())
    }
    pub(super) fn original(&self) -> Original {
        self.original.clone()
    }
    // Called only after successful issued-token revoke AND original disconnect.
    pub(super) fn authority_retired(&self, runtime: &HostRuntime) -> Result<()> {
        if runtime.binding() != self.ledger.binding {
            return Err("foreign retirement witness".into());
        }
        let mut entries = self
            .ledger
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?;
        entries
            .records
            .get_mut(&self.id)
            .ok_or("retired context ticket")?
            .authority_retired = true;
        Ok(())
    }
    pub(super) fn authority_is_retired(&self) -> bool {
        self.ledger
            .entries
            .lock()
            .map(|e| e.records.get(&self.id).is_some_and(|r| r.authority_retired))
            .unwrap_or(false)
    }
    pub(super) fn worker_started(&self) {
        if let Some(record) = self
            .ledger
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .records
            .get_mut(&self.id)
        {
            record.requires_join = true;
        }
    }
    // Called only by try_reclaim after JoinHandle::join returned the actual owner.
    pub(super) fn worker_joined(&self, runtime: &HostRuntime) -> Result<()> {
        if runtime.binding() != self.ledger.binding {
            return Err("foreign join witness".into());
        }
        let mut entries = self
            .ledger
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?;
        entries
            .records
            .get_mut(&self.id)
            .ok_or("retired context ticket")?
            .joined = true;
        Ok(())
    }
    /// Caller drop is not cleanup. Never perform Core, IO, lease close, or scheduler shutdown here.
    pub(super) fn park(self, context: AgentContext) {
        let mut entries = self
            .ledger
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(record) = entries.records.get_mut(&self.id) {
            record.phase = Phase::Abandoned;
            record.parked = Some(context);
        }
        // Parked objects have no ticket: no ledger/Context Arc cycle. Losing the
        // Workbench itself is not qualified cleanup and remains an explicit limit.
    }
    pub(super) fn settle(&self, runtime: &HostRuntime, context: &AgentContext) -> Result<()> {
        if runtime.binding() != self.ledger.binding
            || context.sealed_proposal.is_some()
            || context.native_session_obligations()
            || !self
                .original
                .resources
                .is_clean()
                .map_err(|e| format!("context facts: {e:?}"))?
        {
            return Err("original context cleanup has not completed".into());
        }
        let mut entries = self
            .ledger
            .entries
            .lock()
            .map_err(|_| "context ledger unavailable")?;
        let record = entries
            .records
            .get(&self.id)
            .ok_or("retired context ticket")?;
        if !matches!(record.phase, Phase::Cleanup | Phase::Attached)
            || record.parked.is_some()
            || !record.authority_retired
            || record.requires_join && !record.joined
        {
            return Err("context cleanup phase is not settled".into());
        }
        entries.records.remove(&self.id);
        Ok(())
    }
}

#[cfg(test)]
#[path = "context_lifecycle_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "identity_tests.rs"]
mod identity_tests;
