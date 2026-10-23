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
    #[cfg(windows)]
    fn normal_terminal_and_unstarted_metadata_cannot_satisfy_native_refusal_retirement() {
        let registry=Arc::new(NativeExecutionRegistry::default());
        let slot=registry.reserve("normal-not-refusal".into(),Arc::new(())).unwrap();
        slot.bind_native_completion().unwrap();
        slot.unstarted();
        assert!(registry.statuses().unwrap().is_empty());
        assert!(slot.require_retired_original().is_ok());
        assert!(matches!(slot.require_retired_refusal(),Err(Error::CommitUnknown)));
    }
    #[test]
    #[cfg(windows)]
    fn unknown_refusal_metadata_cannot_release_original_registry_or_keepalive() {
        // Memory-only negative control: even a synthetic refusal bit cannot
        // make an Unknown slot clean. No native proof is made or accepted.
        let registry=Arc::new(NativeExecutionRegistry::default());
        let resource=Arc::new(());let weak=Arc::downgrade(&resource);
        let slot=registry.reserve("unknown-refusal-metadata".into(),resource).unwrap();
        slot.bind_native_completion().unwrap();slot.no_handle_unknown();
        slot.refused.store(true,Ordering::SeqCst);slot.maybe_retire();
        assert_eq!(registry.statuses().unwrap().len(),1);assert!(weak.upgrade().is_some());
        assert!(matches!(slot.require_retired_refusal(),Err(Error::CommitUnknown)));
        assert!(!slot.original_retired.load(Ordering::SeqCst));
    }
    struct MemoryProcess {id:codex_exec_server::ProcessId,wake:tokio::sync::watch::Sender<u64>}
    fn no_native<'a,T:Send+'a>()->codex_exec_server::ExecProcessFuture<'a,T>{
        Box::pin(async{Err(codex_exec_server::ExecServerError::Protocol("memory ownership fixture has no native effects or EOF".into()))})
    }
    impl ExecProcess for MemoryProcess {
        fn process_id(&self)->&codex_exec_server::ProcessId{&self.id}
        fn subscribe_wake(&self)->tokio::sync::watch::Receiver<u64>{self.wake.subscribe()}
        fn subscribe_events(&self)->codex_exec_server::ExecProcessEventReceiver{codex_exec_server::ExecProcessEventReceiver::empty()}
        fn read(&self,_:Option<u64>,_:Option<usize>,_:Option<u64>)->codex_exec_server::ExecProcessFuture<'_,codex_exec_server::ReadResponse>{no_native()}
        fn write(&self,_:Vec<u8>)->codex_exec_server::ExecProcessFuture<'_,codex_exec_server::WriteResponse>{no_native()}
        fn signal(&self,_:codex_exec_server::ProcessSignal)->codex_exec_server::ExecProcessFuture<'_,()>{no_native()}
        fn terminate(&self)->codex_exec_server::ExecProcessFuture<'_,()>{no_native()}
    }
    fn poison_native_retirement_slot(process_poison:bool) {
        // Registry-only memory fixture. No native process/IO completion is
        // fabricated; even its scalar terminal marker must not release capacity.
        let registry=Arc::new(NativeExecutionRegistry::default());
        let memory=Arc::new(());let original_memory=Arc::downgrade(&memory);
        let slot=registry.reserve("same-native-memory".into(),memory.clone()).unwrap();
        let (wake,_)=tokio::sync::watch::channel(0);
        let process=Arc::new(MemoryProcess {id:codex_exec_server::ProcessId::new("same-native-memory"),wake});
        let original_process=Arc::downgrade(&process);
        slot.started(process.clone());drop(process);
        drop(memory);slot.bind_native_completion().unwrap();
        let job=slot.job().unwrap();slot.provider(true);slot.unstarted();
        let _=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if process_poison {let _guard=slot.process.lock().unwrap();panic!("intentional process-slot poison");}
            else{let _guard=slot._keepalive.lock().unwrap();panic!("intentional keepalive-slot poison");}
        }));
        slot.provider(false);drop(job);
        let statuses=registry.statuses().unwrap();
        assert_eq!(statuses.len(),1);assert!(statuses[0].unknown);
        assert!(!slot.original_retired.load(Ordering::SeqCst));
        assert!(matches!(slot.require_retired_original(),Err(Error::CommitUnknown)));
        assert!(Arc::ptr_eq(registry.entries.lock().unwrap().get("same-native-memory").unwrap(),&slot));
        assert!(original_memory.upgrade().is_some());
        assert!(original_process.upgrade().is_some());
        let mut others=Vec::new();
        for i in 1..MAX_NATIVE_EXECUTIONS{others.push(registry.reserve(format!("other-{i}"),Arc::new(())).unwrap());}
        assert_eq!(registry.statuses().unwrap().len(),MAX_NATIVE_EXECUTIONS);
        assert!(matches!(registry.reserve("over-budget".into(),Arc::new(())),Err(Error::Limit)));
        assert!(original_memory.upgrade().is_some());
        assert!(original_process.upgrade().is_some());
        assert!(registry.entries.lock().unwrap().contains_key("same-native-memory"));
        // Only provably uninvoked independent reservations may retire.
        for other in others{other.unstarted();}
        assert_eq!(registry.statuses().unwrap().len(),1);
        assert!(original_memory.upgrade().is_some());
        assert!(original_process.upgrade().is_some());
    }
    #[test]
    fn native_retirement_keeps_original_entry_capacity_and_resources_when_process_lock_is_poisoned(){poison_native_retirement_slot(true);}
    #[test]
    fn native_retirement_keeps_original_entry_capacity_and_resources_when_keepalive_lock_is_poisoned(){poison_native_retirement_slot(false);}
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
    pub closed_before_dispatch: bool,
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
    _keepalive: Mutex<Option<Arc<dyn Send + Sync>>>,
    native_bound: AtomicBool,
    original_retired: AtomicBool,
    pending: AtomicBool,
    unknown: AtomicBool,
    terminal: AtomicBool,
    refused: AtomicBool,
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
                closed_before_dispatch: s.refused.load(Ordering::SeqCst),
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
            _keepalive: Mutex::new(Some(keepalive)),
            native_bound: AtomicBool::new(false),
            original_retired: AtomicBool::new(false),
            pending: AtomicBool::new(true),
            unknown: AtomicBool::new(false),
            terminal: AtomicBool::new(false),
            refused: AtomicBool::new(false),
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
    #[cfg(windows)]
    pub(crate) fn accept_original_refusal(&self,ack:crate::native_completion::NativeRefusalAck)->Result<()> {
        let result=(||{
            let entries=self.registry.entries.try_lock().map_err(|_|Error::Storage)?;
            if !ack.is_for_slot(self) || !entries.get(&self.operation).is_some_and(|entry|std::ptr::eq(Arc::as_ptr(entry),self))
                || !self.is_native_bound() || !self.pending.load(Ordering::SeqCst) || self.is_unknown()
                || self.stop_requested() || self.provider.load(Ordering::SeqCst) || self.terminal.load(Ordering::SeqCst)
                || self.refused.load(Ordering::SeqCst) || self.original_retired.load(Ordering::SeqCst)
                || self.jobs.load(Ordering::SeqCst)==0 {
                return Err(Error::CommitUnknown);
            }
            let process=self.process.try_lock().map_err(|_|Error::Storage)?;
            if process.is_some(){return Err(Error::CommitUnknown);}
            self.refused.store(true,Ordering::SeqCst);
            self.pending.store(false,Ordering::SeqCst);
            Ok(())
        })();
        if result.is_err(){self.unknown();ack.mark_failed();}
        result
    }
    #[cfg(windows)]
    pub(crate) fn require_retired_refusal(&self)->Result<()> {
        if !self.native_bound.load(Ordering::SeqCst) || !self.original_retired.load(Ordering::SeqCst)
            || self.pending.load(Ordering::SeqCst) || self.is_unknown()
            || !self.refused.load(Ordering::SeqCst) || self.terminal.load(Ordering::SeqCst)
            || self.provider.load(Ordering::SeqCst) || self.jobs.load(Ordering::SeqCst)!=0 {
            return Err(Error::CommitUnknown);
        }
        Ok(())
    }
    pub(crate) fn is_native_bound(&self)->bool {self.native_bound.load(Ordering::SeqCst)}
    pub(crate) fn native_terminal(&self)->bool {self.is_native_bound() && self.terminal.load(Ordering::SeqCst) && !self.is_unknown()}
    pub(crate) fn bind_native_completion(&self)->Result<()> {
        if self.native_bound.swap(true,Ordering::SeqCst) {self.unknown();return Err(Error::Denied);}
        Ok(())
    }
    pub(crate) fn require_retired_original(&self)->Result<()> {
        if !self.native_bound.load(Ordering::SeqCst) || !self.original_retired.load(Ordering::SeqCst)
            || self.pending.load(Ordering::SeqCst) || self.unknown.load(Ordering::SeqCst)
            || !self.terminal.load(Ordering::SeqCst) || self.provider.load(Ordering::SeqCst)
            || self.jobs.load(Ordering::SeqCst)!=0 {
            return Err(Error::CommitUnknown);
        }
        Ok(())
    }
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
        let mut retired_resources=None;
        {
            let registry = &self.registry;
            if let Ok(mut entries) = registry.entries.lock() {
                // Occupancy acquisition and retirement share this exact mutex.
                if self.pending.load(Ordering::SeqCst)
                    || !(self.terminal.load(Ordering::SeqCst) || self.refused.load(Ordering::SeqCst))
                    || self.provider.load(Ordering::SeqCst)
                    || self.jobs.load(Ordering::SeqCst) != 0
                    || (self.native_bound.load(Ordering::SeqCst) && self.unknown.load(Ordering::SeqCst))
                {
                    return;
                }
                if entries
                    .get(&self.operation)
                    .is_some_and(|current| std::ptr::eq(Arc::as_ptr(current), self))
                {
                    if self.native_bound.load(Ordering::SeqCst) {
                        // Acquire BOTH fallible original resource slots before
                        // taking either resource or committing registration.
                        // No IO/wait/callback or resource Drop under these locks.
                        let mut process=match self.process.try_lock(){Ok(p)=>p,Err(_)=>{self.unknown();return;}};
                        let mut keepalive=match self._keepalive.try_lock(){Ok(p)=>p,Err(_)=>{self.unknown();return;}};
                        if keepalive.is_none() || self.unknown.load(Ordering::SeqCst) {self.unknown();return;}
                        retired_resources=Some((process.take(),keepalive.take()));
                        // No fallible step follows before the same entry commits.
                        entries.remove(&self.operation);
                        self.original_retired.store(true,Ordering::SeqCst);
                    }else{
                        // Preserve the old default/v7 retirement and lifetime.
                        entries.remove(&self.operation);
                        self.original_retired.store(true,Ordering::SeqCst);
                    }
                }
            }
        }
        drop(retired_resources);
    }
}
