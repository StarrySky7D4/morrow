use codex_exec_server::ExecProcess;
use morrow_agent_session_exec_v1_r2::{Error, Result};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

pub const MAX_NATIVE_EXECUTIONS: usize = 16;
/// Shared by every native port on the original authority. Uncertain resources
/// remain charged until genuine exit/EOF, no live provider and no pending jobs.
#[derive(Default)]
pub struct NativeExecutionRegistry {
    entries: Mutex<BTreeMap<String, Arc<ExecutionSlot>>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_registry_bounds_pending_and_unknown_work() {
        let registry = Arc::new(NativeExecutionRegistry::default());
        let another_port = registry.clone();
        let mut slots = Vec::new();
        for n in 0..MAX_NATIVE_EXECUTIONS {
            slots.push(
                registry
                    .reserve(format!("operation-{n}"), Arc::new(()))
                    .unwrap(),
            );
        }
        assert!(matches!(
            another_port.reserve("one-more".into(), Arc::new(())),
            Err(Error::Limit)
        ));
        slots[0].no_handle_unknown();
        assert!(another_port.statuses().unwrap()[0].unknown);
        assert!(matches!(
            another_port.reserve("still-full".into(), Arc::new(())),
            Err(Error::Limit)
        ));
        // Other reservations provably did not invoke a backend.
        for slot in &slots[1..] {
            slot.unstarted();
        }
        assert_eq!(registry.statuses().unwrap().len(), 1);
    }
    #[test]
    fn terminal_evidence_does_not_release_live_provider_or_pending_job() {
        let registry = Arc::new(NativeExecutionRegistry::default());
        let slot = registry.reserve("operation".into(), Arc::new(())).unwrap();
        let job = slot.job();
        slot.provider(true);
        slot.unstarted();
        assert_eq!(registry.statuses().unwrap().len(), 1);
        slot.provider(false);
        assert_eq!(registry.statuses().unwrap().len(), 1);
        drop(job);
        assert!(registry.statuses().unwrap().is_empty());
        let replacement = registry.reserve("operation".into(), Arc::new(())).unwrap();
        assert!(slot.job().is_none());
        slot.provider(true);
        slot.unstarted();
        assert_eq!(registry.statuses().unwrap().len(), 1);
        replacement.unstarted();
        drop(replacement);
        let weak_registry = Arc::downgrade(&registry);
        drop(slot);
        drop(registry);
        assert!(weak_registry.upgrade().is_none());
    }
    #[test]
    fn dropping_external_owners_keeps_unknown_occupancy_and_resource() {
        // Registry ownership only: this does not simulate an OS process or EOF.
        let registry = Arc::new(NativeExecutionRegistry::default());
        let weak_registry = Arc::downgrade(&registry);
        let resource = Arc::new(());
        let weak_resource = Arc::downgrade(&resource);
        let slot = registry
            .reserve("unknown".into(), resource.clone())
            .unwrap();
        slot.no_handle_unknown();
        drop(resource);
        drop(slot);
        drop(registry);
        let retained = weak_registry
            .upgrade()
            .expect("unresolved registry retained");
        assert!(weak_resource.upgrade().is_some());
        let status = retained.statuses().unwrap();
        assert_eq!(status.len(), 1);
        assert!(status[0].unknown);
        assert!(!status[0].exited_and_closed);
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeExecutionStatus {
    pub operation: String,
    pub start_pending: bool,
    pub unknown: bool,
    pub exited_and_closed: bool,
    pub provider_active: bool,
    pub pending_jobs: usize,
}
pub(crate) struct ExecutionSlot {
    operation: String,
    // The unresolved entry and this reference deliberately retain each other.
    // Exact terminal retirement removes the entry and breaks the cycle. Trusted
    // callers must keep a cleanup port/runtime reachable for unresolved work.
    registry: Arc<NativeExecutionRegistry>,
    pub process: Mutex<Option<Arc<dyn ExecProcess>>>,
    _keepalive: Arc<dyn Send + Sync>,
    pending: AtomicBool,
    unknown: AtomicBool,
    terminal: AtomicBool,
    provider: AtomicBool,
    jobs: AtomicUsize,
    stop_requested: AtomicBool,
    cleanup_started: AtomicBool,
    cleanup_scheduled: AtomicBool,
}
pub(crate) struct JobGuard {
    slot: Arc<ExecutionSlot>,
}
impl Drop for JobGuard {
    fn drop(&mut self) {
        self.slot.jobs.fetch_sub(1, Ordering::SeqCst);
        self.slot.maybe_retire();
    }
}
impl NativeExecutionRegistry {
    pub fn statuses(&self) -> Result<Vec<NativeExecutionStatus>> {
        let entries = self.entries.lock().map_err(|_| Error::Storage)?;
        Ok(entries
            .values()
            .map(|s| NativeExecutionStatus {
                operation: s.operation.clone(),
                start_pending: s.pending.load(Ordering::SeqCst),
                unknown: s.unknown.load(Ordering::SeqCst),
                exited_and_closed: s.terminal.load(Ordering::SeqCst),
                provider_active: s.provider.load(Ordering::SeqCst),
                pending_jobs: s.jobs.load(Ordering::SeqCst),
            })
            .collect())
    }
    pub(crate) fn reserve(
        self: &Arc<Self>,
        operation: String,
        keepalive: Arc<dyn Send + Sync>,
    ) -> Result<Arc<ExecutionSlot>> {
        let mut entries = self.entries.lock().map_err(|_| Error::Storage)?;
        if entries.contains_key(&operation) {
            return Err(Error::Denied);
        }
        if entries.len() >= MAX_NATIVE_EXECUTIONS {
            return Err(Error::Limit);
        }
        let slot = Arc::new(ExecutionSlot {
            operation: operation.clone(),
            registry: self.clone(),
            process: Mutex::new(None),
            _keepalive: keepalive,
            pending: AtomicBool::new(true),
            unknown: AtomicBool::new(false),
            terminal: AtomicBool::new(false),
            provider: AtomicBool::new(false),
            jobs: AtomicUsize::new(0),
            stop_requested: AtomicBool::new(false),
            cleanup_started: AtomicBool::new(false),
            cleanup_scheduled: AtomicBool::new(false),
        });
        entries.insert(operation, slot.clone());
        Ok(slot)
    }
    pub(crate) fn slots(&self) -> Result<Vec<Arc<ExecutionSlot>>> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| Error::Storage)?
            .values()
            .cloned()
            .collect())
    }
}
impl ExecutionSlot {
    pub(crate) fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
    }
    pub(crate) fn stop_requested(&self) -> bool {
        self.stop_requested.load(Ordering::SeqCst)
    }
    pub(crate) fn claim_cleanup(&self) -> bool {
        !self.cleanup_started.swap(true, Ordering::SeqCst)
    }
    pub(crate) fn claim_cleanup_job(&self) -> bool {
        !self.cleanup_scheduled.swap(true, Ordering::SeqCst)
    }
    pub(crate) fn job(self: &Arc<Self>) -> Option<JobGuard> {
        let registry = &self.registry;
        let entries = registry.entries.lock().ok()?;
        if !entries
            .get(&self.operation)
            .is_some_and(|current| Arc::ptr_eq(current, self))
        {
            return None;
        }
        self.jobs.fetch_add(1, Ordering::SeqCst);
        Some(JobGuard { slot: self.clone() })
    }
    pub(crate) fn started(&self, process: Arc<dyn ExecProcess>) {
        if let Ok(mut p) = self.process.lock() {
            *p = Some(process);
        }
        self.pending.store(false, Ordering::SeqCst);
    }
    pub(crate) fn unstarted(&self) {
        self.pending.store(false, Ordering::SeqCst);
        self.terminal.store(true, Ordering::SeqCst);
        self.maybe_retire();
    }
    pub(crate) fn unknown(&self) {
        self.unknown.store(true, Ordering::SeqCst);
    }
    pub(crate) fn is_unknown(&self) -> bool {
        self.unknown.load(Ordering::SeqCst)
    }
    pub(crate) fn no_handle_unknown(&self) {
        self.pending.store(false, Ordering::SeqCst);
        self.unknown();
    }
    pub(crate) fn terminal(&self) {
        self.terminal.store(true, Ordering::SeqCst);
        self.maybe_retire();
    }
    pub(crate) fn provider(&self, active: bool) {
        {
            let registry = &self.registry;
            if let Ok(entries) = registry.entries.lock()
                && entries
                    .get(&self.operation)
                    .is_some_and(|current| std::ptr::eq(Arc::as_ptr(current), self))
            {
                self.provider.store(active, Ordering::SeqCst);
            }
        }
        self.maybe_retire();
    }
    fn maybe_retire(&self) {
        {
            let registry = &self.registry;
            if let Ok(mut entries) = registry.entries.lock() {
                // Occupancy acquisition and retirement share this exact mutex.
                if self.pending.load(Ordering::SeqCst)
                    || !self.terminal.load(Ordering::SeqCst)
                    || self.provider.load(Ordering::SeqCst)
                    || self.jobs.load(Ordering::SeqCst) != 0
                {
                    return;
                }
                if entries
                    .get(&self.operation)
                    .is_some_and(|current| std::ptr::eq(Arc::as_ptr(current), self))
                {
                    entries.remove(&self.operation);
                }
            }
        }
    }
}
