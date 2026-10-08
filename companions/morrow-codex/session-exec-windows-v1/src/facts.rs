//! Bounded native receipts, retained independently of guest and executor liveness.
use crate::{
    NativeExecutionRegistry, NativeExecutionStatus, ProvisionedWindowsBackend,
    process::block_on,
    registry::{ExecutionSlot, JobGuard},
};
use morrow_agent_session_exec_v1_r2::{
    Error, ExecutionFacts, Result, authority::SessionExecHost, safe_exec::ToolIdentity,
};
use morrow_core::dispatch::{HostBinding, HostRuntime};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock, Weak},
    time::Duration,
};

type Owners = Vec<(HostBinding, Weak<BorrowedNativeResources>)>;
static OWNERS: OnceLock<Mutex<Owners>> = OnceLock::new();
const MAX_OWNER_BINDINGS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingFactStatus {
    pub identity: ToolIdentity,
    pub facts: Option<ExecutionFacts>,
    pub unknown: bool,
}
#[derive(Default, Debug)]
pub struct FactsDrainReport {
    pub acknowledged: usize,
    pub retained: usize,
    pub errors: Vec<(ToolIdentity, Error)>,
}
struct PendingFact {
    identity: ToolIdentity,
    facts: Option<ExecutionFacts>,
    uncertain_cas: bool,
    invalid_facts: bool,
    slot: Arc<ExecutionSlot>,
    backend: Arc<ProvisionedWindowsBackend>,
    // Never released merely because Exited/Closed was seen: original owner CAS
    // acknowledgement is separately required before quota can retire.
    _job: JobGuard,
}
/// Shared by all borrowed ports attached to exactly the same original Core.
/// This owns no Core, Store, protected session or executor admission.
pub struct BorrowedNativeResources {
    pub(crate) start_diagnostic: crate::borrowed::NativeStartDiagnostic,
    binding: HostBinding,
    pub(crate) registry: Arc<NativeExecutionRegistry>,
    pending: Mutex<BTreeMap<String, PendingFact>>,
    host: Mutex<Option<Arc<SessionExecHost>>>,
}
impl BorrowedNativeResources {
    pub fn for_owner(runtime: &HostRuntime) -> Result<Arc<Self>> {
        let binding = runtime.binding();
        let mut owners = OWNERS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .map_err(|_| Error::Storage)?;
        owners.retain(|(_, owner)| owner.strong_count() != 0);
        if let Some(owner) = owners
            .iter()
            .find(|(key, _)| *key == binding)
            .and_then(|(_, v)| v.upgrade())
        {
            return Ok(owner);
        }
        if owners.len() >= MAX_OWNER_BINDINGS {
            return Err(Error::Limit);
        }
        let owner = Arc::new(Self {
            start_diagnostic: crate::borrowed::NativeStartDiagnostic::default(),
            binding,
            registry: Arc::new(NativeExecutionRegistry::default()),
            pending: Mutex::new(BTreeMap::new()),
            host: Mutex::new(None),
        });
        owners.push((binding, Arc::downgrade(&owner)));
        Ok(owner)
    }
    pub fn owner_binding(&self) -> HostBinding {
        self.binding
    }
    pub(crate) fn bind_host(
        &self,
        host: Arc<SessionExecHost>,
        runtime: &HostRuntime,
    ) -> Result<()> {
        self.check(runtime)?;
        host.generation(runtime)?;
        let mut original = self.host.lock().map_err(|_| Error::Storage)?;
        if let Some(original) = original.as_ref() {
            if !Arc::ptr_eq(original, &host) {
                return Err(Error::Denied);
            }
        } else {
            *original = Some(host);
        }
        Ok(())
    }
    fn check_host(&self, host: &SessionExecHost) -> Result<()> {
        let original = self.host.lock().map_err(|_| Error::Storage)?;
        if !original
            .as_ref()
            .is_some_and(|original| std::ptr::eq(Arc::as_ptr(original), host))
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn check(&self, runtime: &HostRuntime) -> Result<()> {
        if self.binding != runtime.binding() {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub(crate) fn reserve_fact(
        &self,
        identity: ToolIdentity,
        slot: Arc<ExecutionSlot>,
        backend: Arc<ProvisionedWindowsBackend>,
    ) -> Result<()> {
        let job = slot.job().ok_or(Error::Storage)?;
        let mut pending = self.pending.lock().map_err(|_| Error::Storage)?;
        if pending.len() >= crate::MAX_NATIVE_EXECUTIONS {
            return Err(Error::Limit);
        }
        if pending.contains_key(&identity.operation_id) {
            return Err(Error::Denied);
        }
        pending.insert(
            identity.operation_id.clone(),
            PendingFact {
                identity,
                facts: None,
                uncertain_cas: false,
                invalid_facts: false,
                slot,
                backend,
                _job: job,
            },
        );
        Ok(())
    }
    pub(crate) fn cancel_unstarted(&self, identity: &ToolIdentity) {
        // Only a start path with proof that its actual backend callback was not
        // invoked may call this. Removal does not classify an invoked failure.
        if let Ok(mut pending) = self.pending.lock()
            && pending
                .get(&identity.operation_id)
                .is_some_and(|p| p.identity == *identity)
        {
            let entry = pending.remove(&identity.operation_id);
            drop(pending);
            if let Some(entry) = entry {
                entry.slot.unstarted();
                drop(entry);
            }
        }
    }
    pub(crate) fn publish(&self, identity: &ToolIdentity, facts: ExecutionFacts) {
        if let Ok(mut pending) = self.pending.lock()
            && let Some(entry) = pending.get_mut(&identity.operation_id)
            && entry.identity == *identity
        {
            if facts.validate().is_err()
                || !facts.output_closed
                || facts.exit_code.is_none()
                || entry
                    .facts
                    .as_ref()
                    .is_some_and(|previous| previous != &facts)
            {
                entry.invalid_facts = true;
                entry.slot.unknown();
                return;
            }
            if entry.invalid_facts {
                return;
            }
            entry.facts = Some(facts);
        }
    }
    pub fn pending_facts(&self) -> Result<Vec<PendingFactStatus>> {
        Ok(self
            .pending
            .lock()
            .map_err(|_| Error::Storage)?
            .values()
            .map(|entry| PendingFactStatus {
                identity: entry.identity.clone(),
                facts: entry.facts.clone(),
                unknown: entry.invalid_facts
                    || entry.uncertain_cas
                    || entry.slot.is_unknown()
                    || entry.facts.is_none(),
            })
            .collect())
    }
    pub fn cleanup_status(&self) -> Result<Vec<NativeExecutionStatus>> {
        self.registry.statuses()
    }
    pub fn is_clean(&self) -> Result<bool> {
        Ok(self.pending_facts()?.is_empty() && self.cleanup_status()?.is_empty())
    }
    /// Stop callback uses only native slots and scheduler. It cannot reenter
    /// Core, a protected session, R2 fences, or owner persistence locks.
    pub fn request_stop(&self) {
        let Ok(pending) = self.pending.lock() else {
            return;
        };
        for entry in pending.values() {
            entry.slot.request_stop();
            let process = entry.slot.process.lock().ok().and_then(|p| p.clone());
            if let Some(process) = process {
                Self::cleanup_once(entry.backend.clone(), entry.slot.clone(), process);
            }
        }
    }
    pub(crate) fn cleanup_once(
        backend: Arc<ProvisionedWindowsBackend>,
        slot: Arc<ExecutionSlot>,
        process: Arc<dyn codex_exec_server::ExecProcess>,
    ) {
        if !slot.claim_cleanup_job() {
            return;
        }
        let Some(job) = slot.job() else { return };
        let handle = backend.runtime.handle().clone();
        handle.spawn(async move {
            let _backend = backend;
            let _job = job;
            crate::process::cleanup_actual(process, &slot).await;
        });
    }
    /// Trusted read only reconciliation does not retry uncertain termination.
    pub fn reap_cleanup(&self) -> Result<usize> {
        let before = self.cleanup_status()?.len();
        let entries = self
            .pending
            .lock()
            .map_err(|_| Error::Storage)?
            .values()
            .map(|p| (p.slot.clone(), p.backend.clone()))
            .collect::<Vec<_>>();
        for (slot, backend) in entries {
            let Some(job) = slot.job() else { continue };
            let process = slot.process.lock().map_err(|_| Error::Storage)?.clone();
            if let Some(process) = process {
                let result = block_on(backend.runtime.handle(), async {
                    tokio::time::timeout(
                        Duration::from_millis(200),
                        process.read(None, Some(1), Some(0)),
                    )
                    .await
                });
                if matches!(result,Ok(Ok(read)) if read.exited && read.closed) {
                    slot.terminal();
                }
            }
            drop(job);
        }
        // An acknowledged mailbox entry may already be gone, but its provider
        // or asynchronous job remains in registry until their real Drop.
        Ok(before.saturating_sub(self.cleanup_status()?.len()))
    }
    pub fn drain_completed(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
    ) -> Result<FactsDrainReport> {
        self.check(runtime)?;
        // No pending work has bound an R2 issuer yet; this is merely empty
        // native bookkeeping, not proof of host or executor authorization.
        if self.pending_facts()?.is_empty() {
            return Ok(FactsDrainReport::default());
        }
        self.check_host(host)?;
        host.generation(runtime)?;
        let statuses = self.pending_facts()?;
        let mut report = FactsDrainReport::default();
        for status in statuses {
            if status.facts.is_none() {
                continue;
            }
            let revision = match host.inspect_tool_record(runtime, &status.identity.operation_id) {
                Ok(current) => current.record_revision,
                Err(error) => {
                    report.errors.push((status.identity, error));
                    continue;
                }
            };
            match self.drain_completed_at(runtime, host, &status.identity, revision) {
                Ok(true) => report.acknowledged += 1,
                Ok(false) => {}
                Err(error) => report.errors.push((status.identity, error)),
            }
        }
        report.retained = self.pending_facts()?.len();
        Ok(report)
    }
    /// CAS is historical observation, never revived execution permission.
    /// An uncertain previous CAS is only read-reconciled; it is never replayed.
    pub fn drain_completed_at(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        identity: &ToolIdentity,
        expected_revision: u64,
    ) -> Result<bool> {
        self.check(runtime)?;
        self.check_host(host)?;
        host.generation(runtime)?;
        let mut pending = self.pending.lock().map_err(|_| Error::Storage)?;
        let entry = pending
            .get_mut(&identity.operation_id)
            .ok_or(Error::NotFound)?;
        if entry.identity != *identity {
            return Err(Error::Denied);
        }
        // A conflicting or invalid independent observation cannot be silently
        // replaced, CASed or acknowledged using an earlier valid-looking fact.
        if entry.invalid_facts {
            return Err(Error::CommitUnknown);
        }
        let Some(facts) = entry.facts.clone() else {
            return Ok(false);
        };
        let current = host.inspect_tool_record(runtime, &identity.operation_id)?;
        if current.identity != *identity || !current.invocation_started {
            return Err(Error::Denied);
        }
        if current.latest.as_ref() != Some(&facts) {
            if entry.uncertain_cas {
                return Err(Error::CommitUnknown);
            }
            match host.reconcile_tool_observation(runtime, identity, expected_revision, &facts) {
                Ok(ack) if ack.identity == *identity && ack.latest.as_ref() == Some(&facts) => {}
                Ok(_) => {
                    entry.uncertain_cas = true;
                    entry.slot.unknown();
                    return Err(Error::CommitUnknown);
                }
                Err(error) => {
                    if matches!(error, Error::CommitUnknown | Error::Storage) {
                        entry.uncertain_cas = true;
                        entry.slot.unknown();
                    }
                    return Err(error);
                }
            }
        }
        let acknowledged = pending.remove(&identity.operation_id);
        drop(pending);
        drop(acknowledged);
        Ok(true)
    }
}

// Diagnostic view is fixed scalars only, not Debug of authority/resources/facts.
impl std::fmt::Debug for BorrowedNativeResources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.start_diagnostic, f)
    }
}
