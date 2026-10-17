//! Private same-invocation capability lane. Diagnostic/history readers never
//! participate. Cloning a review binding does not clone a completion capability.
use std::sync::{Arc,Mutex,Weak};
use morrow_agent_session_exec_v1_r2::{Error,Result,safe_exec::ToolIdentity};
use morrow_core::dispatch::HostBinding;
use codex_sandboxing::{MatchedRunnerArtifact,NativeHostCompletion};
use crate::{BorrowedNativeResources,ProvisionedWindowsBackend};

struct State {
    identity:Option<ToolIdentity>,
    native:Option<NativeHostCompletion>,
    slot:Option<Arc<crate::registry::ExecutionSlot>>,
    failed:bool,
    taken:bool,
    monitor:Option<tokio::task::JoinHandle<()>>,
    monitor_joined:bool,
    monitor_loaned:bool,
    monitor_outcome:Option<MonitorJoin>,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum MonitorJoin {JoinedTimely,JoinedLate,Cancelled,Panicked}
impl State {
    fn accepts_original_native_identity(&self,identity:&ToolIdentity)->bool {
        !self.failed && !self.taken && self.native.is_none() && self.identity.as_ref()==Some(identity)
    }
}
struct Bound {
    owner:HostBinding,
    operation:String,
    artifact:Weak<MatchedRunnerArtifact>,
    backend:Weak<ProvisionedWindowsBackend>,
    resources:Weak<BorrowedNativeResources>,
    state:Mutex<State>,
}
/// Only a pre-start instance relation. No process operation or cleanup right.
#[derive(Clone)]
pub struct ReviewedNativeCompletion {bound:Arc<Bound>}
impl std::fmt::Debug for ReviewedNativeCompletion {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("ReviewedNativeCompletion([BOUND_NOT_AUTHORITY])")}
}
/// The real SDK normal completion, not constructible from observations, JSON,
/// cloned reviews, PID/path/hash values or booleans. Intentionally not Clone.
pub struct NativeExecutionCompletion {
    binding:ReviewedNativeCompletion,
    runner_pid:u32,
    runner_creation_filetime:u64,
    runner_raw_dword:u32,
    attempt:u64,
    terminal_close_ack_verified:bool,
    pub(crate) asuser_origin:Option<codex_sandboxing::AsUserOriginRetirement>,
}
impl std::fmt::Debug for NativeExecutionCompletion {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("NativeExecutionCompletion([CONSUME_ONLY])")}
}
impl ReviewedNativeCompletion {
    pub(crate) fn new(resources:&Arc<BorrowedNativeResources>,backend:&Arc<ProvisionedWindowsBackend>,operation:&str)
        ->Option<Self> {
        let artifact=backend.native_close_artifact()?;
        Some(Self {bound:Arc::new(Bound {owner:resources.owner_binding(),operation:operation.to_owned(),
            artifact:Arc::downgrade(&artifact),backend:Arc::downgrade(backend),resources:Arc::downgrade(resources),
            state:Mutex::new(State {identity:None,native:None,slot:None,failed:false,taken:false,
                monitor:None,monitor_joined:false,monitor_loaned:false,monitor_outcome:None})})})
    }
    /// Must be used before invocation, while the original lifecycle still owns
    /// its setup proof and materialized pin. Path/hash equality is insufficient.
    pub fn is_for_original_runner(&self,runner:&MatchedRunnerArtifact)->bool {
        self.bound.artifact.upgrade().is_some_and(|actual|actual.shares_original_pin(runner))
    }
    pub fn same_review(&self,other:&Self)->bool {Arc::ptr_eq(&self.bound,&other.bound)}
    pub(crate) fn bind_started(&self,identity:&ToolIdentity,slot:&Arc<crate::registry::ExecutionSlot>,
        resources:&Arc<BorrowedNativeResources>,backend:&Arc<ProvisionedWindowsBackend>)->Result<()> {
        let mut state=self.bound.state.lock().map_err(|_|Error::Storage)?;
        if state.failed || state.identity.is_some() || state.taken || self.bound.operation!=identity.operation_id
            || self.bound.owner!=resources.owner_binding()
            || !self.bound.resources.upgrade().is_some_and(|original|Arc::ptr_eq(&original,resources))
            || !self.bound.backend.upgrade().is_some_and(|original|Arc::ptr_eq(&original,backend)) {
            state.failed=true;return Err(Error::Denied);
        }
        slot.bind_native_completion()?;
        state.identity=Some(identity.clone());state.slot=Some(Arc::clone(slot));Ok(())
    }
    pub(crate) fn retain_actual_native(&self,identity:&ToolIdentity,native:NativeHostCompletion)->Result<()> {
        let mut state=self.bound.state.lock().map_err(|_|Error::Storage)?;
        if !state.accepts_original_native_identity(identity) {
            state.failed=true;return Err(Error::CommitUnknown);
        }
        state.native=Some(native);Ok(())
    }
    pub(crate) fn fail(&self) {if let Ok(mut state)=self.bound.state.lock(){state.failed=true;}}
    pub(crate) fn install_original_monitor(&self,original:tokio::task::JoinHandle<()>) {
        let mut state=self.bound.state.lock().unwrap_or_else(|p|p.into_inner());
        assert!(state.monitor.is_none() && !state.monitor_joined && !state.monitor_loaned);
        state.monitor=Some(original);
    }
    /// Call outside any Core/HostRuntime borrow or owner lock. Cancellation
    /// restores this same original handle and seals this invocation Unknown.
    pub async fn join_original_monitor(&self,deadline:std::time::Instant)->Result<()> {
        let original={let mut state=self.bound.state.lock().map_err(|_|Error::Storage)?;
            if state.failed || state.monitor_loaned || state.monitor_joined {return Err(Error::CommitUnknown);}
            let original=state.monitor.take().ok_or(Error::CommitUnknown)?;
            state.monitor_loaned=true;original
        };
        struct Loan<'a>{bound:&'a Bound,original:Option<tokio::task::JoinHandle<()>>,outcome:Option<MonitorJoin>}
        impl Drop for Loan<'_>{fn drop(&mut self){let mut s=self.bound.state.lock().unwrap_or_else(|p|p.into_inner());
            s.monitor_loaned=false;s.monitor_outcome=self.outcome;
            if self.outcome.is_some(){s.monitor_joined=true;if self.outcome!=Some(MonitorJoin::JoinedTimely){s.failed=true;}}
            else{s.failed=true;s.monitor=self.original.take();}}}
        let mut loan=Loan {bound:&self.bound,original:Some(original),outcome:None};
        let result=tokio::time::timeout(deadline.saturating_duration_since(std::time::Instant::now()),loan.original.as_mut().unwrap()).await
            .map_err(|_|Error::CommitUnknown)?;
        loan.original.take();let timely=std::time::Instant::now()<deadline;
        loan.outcome=Some(match &result {Ok(()) if timely=>MonitorJoin::JoinedTimely,Ok(())=>MonitorJoin::JoinedLate,
            Err(error) if error.is_cancelled()=>MonitorJoin::Cancelled,Err(_)=>MonitorJoin::Panicked});
        result.map_err(|_|Error::CommitUnknown)?;
        if !timely {return Err(Error::CommitUnknown);}Ok(())
    }
    pub fn join_original_monitor_blocking(&self,deadline:std::time::Instant)->Result<()> {
        let backend=self.bound.backend.upgrade().ok_or(Error::CommitUnknown)?;
        crate::process::block_on(backend.runtime.handle(),self.join_original_monitor(deadline))
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    fn identity()->ToolIdentity {ToolIdentity {operation_id:"same-reviewed-operation".into(),proposal_sha256:[1;32],
        intent_sha256:[2;32],execution_domain:"labelled-memory-only".into(),session_id:"original".into(),
        session_epoch:3,generation:4,original_issuer_nonce:[5;32]}}
    fn uncompleted(identity:ToolIdentity)->State {State {identity:Some(identity),native:None,slot:None,
        failed:false,taken:false,monitor:None,monitor_joined:false,monitor_loaned:false,monitor_outcome:None}}
    #[test]
    fn native_capability_lane_requires_full_original_operation_generation_and_issuer() {
        let original=identity();let state=uncompleted(original.clone());
        assert!(state.accepts_original_native_identity(&original));
        for field in 0..8 {
            let mut changed=original.clone();match field {
                0=>changed.operation_id.push('x'),1=>changed.generation+=1,2=>changed.session_epoch+=1,
                3=>changed.original_issuer_nonce[0]^=1,4=>changed.session_id.push('x'),
                5=>changed.execution_domain.push('x'),6=>changed.intent_sha256[0]^=1,
                _=>changed.proposal_sha256[0]^=1,
            }
            assert!(!state.accepts_original_native_identity(&changed));
        }
        // Matching scalar identity is only admission to receive a REAL sealed
        // native value; it creates none, no slot, no monitor join or permission.
        assert!(state.native.is_none());assert!(state.slot.is_none());assert!(!state.monitor_joined);
    }
    #[test]
    fn consumed_failed_and_unbound_identity_states_cannot_receive_a_new_native_capability() {
        let original=identity();let mut state=uncompleted(original.clone());
        state.taken=true;assert!(!state.accepts_original_native_identity(&original));
        state.taken=false;state.failed=true;assert!(!state.accepts_original_native_identity(&original));
        state.failed=false;state.identity=None;assert!(!state.accepts_original_native_identity(&original));
        assert!(state.native.is_none());
    }
}
impl NativeExecutionCompletion {
    pub(crate) fn is_original_factory(&self,backend:&Weak<ProvisionedWindowsBackend>)->bool {Weak::ptr_eq(&self.binding.bound.backend,backend)}
    pub fn is_for_owner(&self,owner:HostBinding)->bool {self.binding.bound.owner==owner}
    pub fn is_for_review(&self,binding:&ReviewedNativeCompletion)->bool {self.binding.same_review(binding)}
    pub fn runner_pid(&self)->u32 {self.runner_pid}
    pub fn runner_creation_filetime(&self)->u64 {self.runner_creation_filetime}
    pub fn runner_raw_dword(&self)->u32 {self.runner_raw_dword}
    pub fn startup_attempt(&self)->u64 {self.attempt}
    pub fn terminal_close_ack_verified(&self)->bool {self.terminal_close_ack_verified}
}
impl ProvisionedWindowsBackend {
    pub(crate) fn native_close_artifact(&self)->Option<Arc<MatchedRunnerArtifact>> {
        let artifact=self._runner.as_ref()?.artifact();
        (artifact.protocol_version()==8).then_some(artifact)
    }
}
impl BorrowedNativeResources {
    /// Invoked on the synchronous original owner after real task/IO return and
    /// acknowledged facts. A review binding alone can never make this succeed.
    pub fn take_native_execution_completion(&self,binding:&ReviewedNativeCompletion)
        ->Result<NativeExecutionCompletion> {
        // Original HostRuntime was checked at review/start; the durable original
        // ACK path alone retires this entry. No Core borrow crosses native waits.
        if binding.bound.owner!=self.owner_binding()
            || !binding.bound.resources.upgrade().is_some_and(|original|std::ptr::eq(Arc::as_ptr(&original),self)) {
            return Err(Error::Denied);
        }
        let (identity,slot)={
            let state=binding.bound.state.lock().map_err(|_|Error::Storage)?;
            if state.failed || state.taken || !state.monitor_joined || state.monitor_outcome!=Some(MonitorJoin::JoinedTimely) {return Err(Error::CommitUnknown);}
            (state.identity.clone().ok_or(Error::Denied)?,state.slot.clone().ok_or(Error::CommitUnknown)?)
        };
        // Scope-specific receipt retirement, not global-empty inference.
        if self.pending_facts()?.iter().any(|entry|entry.identity==identity) {
            return Err(Error::CommitUnknown);
        }
        slot.require_retired_original()?;
        let native={
            let mut state=binding.bound.state.lock().map_err(|_|Error::Storage)?;
            if state.failed || state.taken {return Err(Error::CommitUnknown);}
            let native=state.native.take().ok_or(Error::CommitUnknown)?;
            state.taken=true;native
        };
        let runner=native.runner_identity();let runner_raw_dword=native.runner_raw_dword();let attempt=native.startup_attempt();
        let terminal_close_ack_verified=native.terminal_close_ack_verified();
        // This real lower obligation is consumed only after this SDK slot and
        // durable original facts have actually retired. No outer boolean input.
        let asuser_origin=native.retire_local_with_asuser_origin().map_err(|_|Error::CommitUnknown)?;
        Ok(NativeExecutionCompletion {binding:binding.clone(),runner_pid:runner.process_id,
            runner_creation_filetime:runner.creation_filetime_100ns,runner_raw_dword,attempt,terminal_close_ack_verified,asuser_origin})
    }
}
