//! Passive mirror of the original owner state. Reading the mirror never borrows
//! or retains the native owner. Existing owner guards publish after unlocking.
use super::*;
use codex_utils_pty::{
    RunnerObservationPublisher, RunnerObservationReader, RunnerOwnerSnapshot,
    RunnerProcessIdentity, RunnerHandshakeHistory, RunnerLifecycle, RunnerFailure,
    RunnerWorkerHistory, RunnerWorkerPlacement, RunnerThreadDisposition,
    RunnerFinalization, RunnerHelperHistory,
};
use std::ops::{Deref,DerefMut};
use std::sync::{LockResult,MutexGuard,PoisonError,WaitTimeoutResult};

fn base<S>(slot: &WorkerSlotState<S>) -> RunnerWorkerHistory {
    let placement=match slot {
        WorkerSlotState::NotInstalled=>RunnerWorkerPlacement::NotInstalled,
        WorkerSlotState::ReservedForInstall=>RunnerWorkerPlacement::Reserved,
        WorkerSlotState::LoanedToFinalizer=>RunnerWorkerPlacement::Borrowed,
        WorkerSlotState::Installed(_)=>RunnerWorkerPlacement::Pending,
    };
    RunnerWorkerHistory { placement,within_deadline:None,result_clean:None,physical_eof:None,
        protocol_exit_i32:None,termination_sent:None,joined_error:false }
}
fn reader(slot:&WorkerSlotState<StdWorkerSlot<ReaderWorkerOutcome>>)->RunnerWorkerHistory {
    let mut value=base(slot);
    if let WorkerSlotState::Installed(StdWorkerSlot::Joined{outcome,within_deadline})=slot {
        value.placement=RunnerWorkerPlacement::Joined;value.within_deadline=Some(*within_deadline);
        match outcome { Ok(v)=>{ value.result_clean=Some(v.clean);value.physical_eof=Some(v.physical_eof);
            value.protocol_exit_i32=v.protocol_exit; },Err(_)=>value.joined_error=true }
    }
    value
}
fn writer(slot:&WorkerSlotState<StdWorkerSlot<WriterWorkerOutcome>>)->RunnerWorkerHistory {
    let mut value=base(slot);
    if let WorkerSlotState::Installed(StdWorkerSlot::Joined{outcome,within_deadline})=slot {
        value.placement=RunnerWorkerPlacement::Joined;value.within_deadline=Some(*within_deadline);
        match outcome { Ok(v)=>{value.result_clean=Some(v.clean);value.termination_sent=Some(v.termination_sent);},
            Err(_)=>value.joined_error=true }
    }
    value
}
fn input(slot:&WorkerSlotState<AsyncInputSlot>)->RunnerWorkerHistory {
    let mut value=base(slot);
    if let WorkerSlotState::Installed(AsyncInputSlot::Joined{outcome,within_deadline,..})=slot {
        value.placement=RunnerWorkerPlacement::Joined;value.within_deadline=Some(*within_deadline);
        match outcome { Ok(v)=>value.result_clean=Some(v.clean),Err(_)=>value.joined_error=true }
    }
    value
}
fn snapshot(s:&OwnerState)->RunnerOwnerSnapshot {
    let lifecycle=match s.stage {
        OwnerLifecycleStage::ReservedBeforeLaunch=>RunnerLifecycle::ReservedBeforeLaunch,
        OwnerLifecycleStage::ProcessLaunchedInHandshake=>RunnerLifecycle::ProcessLaunchedInHandshake,
        OwnerLifecycleStage::HandshakeSucceededInTransport=>RunnerLifecycle::HandshakeSucceededInTransport,
        OwnerLifecycleStage::CheckedIoExtracted=>RunnerLifecycle::CheckedIoExtracted,
        OwnerLifecycleStage::ActorAdopted=>RunnerLifecycle::ActorAdopted,
        OwnerLifecycleStage::FailedOrAbandoned=>RunnerLifecycle::FailedOrAbandoned,
        OwnerLifecycleStage::FinalizedClean=>RunnerLifecycle::FinalizedClean,
        OwnerLifecycleStage::PhysicallyReapedStickyUnknown=>RunnerLifecycle::PhysicallyReapedStickyUnknown,
    };
    let first_error=s.sticky_unknown.as_ref().map(|e|match e {
        StickyUnknownReason::CallerAwaitCancelled=>RunnerFailure::CallerAwaitCancelled,
        StickyUnknownReason::IdentityValidationFailed(_)=>RunnerFailure::IdentityValidationFailed,
        StickyUnknownReason::ThreadCloseFailed{..}=>RunnerFailure::ThreadCloseFailed,
        StickyUnknownReason::ConnectRetainedUnknown{..}=>RunnerFailure::ConnectRetainedUnknown,
        StickyUnknownReason::LateConnectSuccess{..}=>RunnerFailure::LateConnectSuccess,
        StickyUnknownReason::ConnectFailed{..}=>RunnerFailure::ConnectFailed,
        StickyUnknownReason::HandshakeFailed(_)=>RunnerFailure::HandshakeFailed,
        StickyUnknownReason::StartupUnwound=>RunnerFailure::StartupUnwound,
        StickyUnknownReason::TransportDroppedUnadopted=>RunnerFailure::TransportDroppedUnadopted,
        StickyUnknownReason::WrongLegacyConversion=>RunnerFailure::WrongLegacyConversion,
        StickyUnknownReason::CheckedIoDroppedUnadopted=>RunnerFailure::CheckedIoDroppedUnadopted,
        StickyUnknownReason::RuntimeLifecycleFailure=>RunnerFailure::RuntimeLifecycleFailure,
        StickyUnknownReason::ReaderIncompleteOrFailed=>RunnerFailure::ReaderIncompleteOrFailed,
        StickyUnknownReason::WriterFailed=>RunnerFailure::WriterFailed,
        StickyUnknownReason::InputTaskFailedOrAborted=>RunnerFailure::InputTaskFailedOrAborted,
        StickyUnknownReason::FinalizationDeadlineExpired=>RunnerFailure::FinalizationDeadlineExpired,
        StickyUnknownReason::ProcessWaitExpiredOrFailed=>RunnerFailure::ProcessWaitExpiredOrFailed,
    });
    RunnerOwnerSnapshot { lifecycle,
        runner_identity:s.identity.map(|v|RunnerProcessIdentity{pid:v.pid,creation_filetime_100ns:v.creation_filetime}),
        runner_get_exit_code_process_u32:s.native_process_exit_code,
        negotiated_handshake:s.negotiated_handshake,
        protocol_business_exit_i32:s.protocol_exit_code,
        reader_physical_eof_observed:s.reader_physical_eof,no_replay_sealed:s.no_replay_seal,first_error,
        reader:reader(&s.reader_worker),writer:writer(&s.writer_worker),input:input(&s.input_worker),
        thread_disposition:s.thread_disposition.map(|v|match v {
            OriginalThreadDisposition::ClosedOnCapture{..}=>RunnerThreadDisposition::ClosedOnCapture,
            OriginalThreadDisposition::CloseFailedUnknown{..}=>RunnerThreadDisposition::CloseFailedUnknown,
            OriginalThreadDisposition::ScopedSynthetic=>RunnerThreadDisposition::ScopedSynthetic,
        }),
        thread_close_debt_os_error:s.thread_close_debt.map(|v|v.os_error),
        process_handle_retained:s.process.is_some(),
        native_thread_anchors_retained:s.reader_thread_anchor.is_some()||s.writer_thread_anchor.is_some(),
        finalization:match &s.finalized_outcome {
            Some(CheckedFinalizationOutcome::HostNativeClosedOuterScopePending{..})=>RunnerFinalization::InProgress,
            Some(CheckedFinalizationOutcome::CleanRelease{..})=>RunnerFinalization::CleanRelease,
            Some(CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown{..})=>RunnerFinalization::PhysicallyReapedStickyUnknown,
            None if s.finalization_in_progress=>RunnerFinalization::InProgress,
            None=>RunnerFinalization::NotFinalized,
        },
        finalizer_epoch:s.finalizer_epoch,worker_admission_active:s.worker_admission_active,
        helpers:s.helper_attempts.map(|v|v.map(|v|RunnerHelperHistory{acknowledged:v.acknowledged,
            within_deadline:v.within_deadline,had_unknown:v.had_unknown})),
    }
}
/// Same original Mutex and poison semantics, with a bounded post-unlock data
/// publication. No callback, native call or additional blocking history lock.
pub(super) struct ObservedOwnerState {
    inner:Mutex<OwnerState>,
    history:RunnerObservationPublisher,
    host_facts:HostNativeCloseFactsPublisher,
}
pub(super) struct StateGuard<'a> {
    raw:Option<MutexGuard<'a,OwnerState>>,
    state:&'a ObservedOwnerState,
}
impl ObservedOwnerState {
    pub(super) fn new(value:OwnerState,startup_id:StartupId,has_local_registry:bool)->Self {
        let history=RunnerObservationPublisher::new(startup_id.0);
        if let Some(revision)=history.next_revision() { history.publish(revision,snapshot(&value)); }
        let host_facts=HostNativeCloseFactsPublisher::new(&value,startup_id,has_local_registry);
        Self { inner:Mutex::new(value),history,host_facts }
    }
    pub(super) fn reader(&self)->RunnerObservationReader { self.history.reader() }
    pub(super) fn host_facts_reader(&self)->HostNativeCloseFactsReader { self.host_facts.reader() }
    pub(super) fn native_local_registry_retired(&self) { self.host_facts.local_registry_retired(); }
    pub(super) fn host_facts_snapshot(&self)->HostNativeCloseFactsSnapshot {
        self.host_facts.reader().read()
    }
    pub(super) fn lock(&self)->LockResult<StateGuard<'_>> {
        self.history.begin_access();
        self.host_facts.begin_access();
        match self.inner.lock() {
            Ok(raw)=>Ok(StateGuard{raw:Some(raw),state:self}),
            Err(error)=>{
                self.history.poison();
                self.host_facts.mark_poisoned();
                let mut inner=error.into_inner();
                inner.owner_mutex_poison_observed=true;
                inner.no_replay_seal=true;
                Err(PoisonError::new(StateGuard{raw:Some(inner),state:self}))
            }
        }
    }
    pub(super) fn try_lock(&self)->std::sync::TryLockResult<StateGuard<'_>> {
        self.history.begin_access();
        self.host_facts.begin_access();
        match self.inner.try_lock() {
            Ok(raw)=>Ok(StateGuard{raw:Some(raw),state:self}),
            Err(std::sync::TryLockError::WouldBlock)=>{
                self.history.end_access();
                self.host_facts.end_access();
                Err(std::sync::TryLockError::WouldBlock)
            }
            Err(std::sync::TryLockError::Poisoned(error))=>{
                self.history.poison();
                self.host_facts.mark_poisoned();
                let mut inner=error.into_inner();
                inner.owner_mutex_poison_observed=true;
                inner.no_replay_seal=true;
                Err(std::sync::TryLockError::Poisoned(PoisonError::new(
                    StateGuard{raw:Some(inner),state:self})))
            }
        }
    }
    pub(super) fn wait_timeout<'a>(&'a self,cv:&Condvar,mut guard:StateGuard<'a>,duration:Duration)
        ->LockResult<(StateGuard<'a>,WaitTimeoutResult)> {
        // Preserve the original Condvar wait/poison behavior; mirror reads report
        // Busy during the entire existing wait, never observe or perform that wait.
        let raw=guard.raw.take().expect("original owner guard");
        match cv.wait_timeout(raw,duration) {
            Ok((raw,timeout))=>{guard.raw=Some(raw);Ok((guard,timeout))},
            Err(error)=>{
                self.history.poison();
                self.host_facts.mark_poisoned();
                let (mut raw,timeout)=error.into_inner();
                raw.owner_mutex_poison_observed=true;
                raw.no_replay_seal=true;
                guard.raw=Some(raw);
                Err(PoisonError::new((guard,timeout)))
            }
        }
    }
}
impl Deref for StateGuard<'_> {
    type Target=OwnerState;
    fn deref(&self)->&OwnerState { self.raw.as_ref().expect("original owner guard") }
}
impl DerefMut for StateGuard<'_> {
    fn deref_mut(&mut self)->&mut OwnerState { self.raw.as_mut().expect("original owner guard") }
}
impl Drop for StateGuard<'_> {
    fn drop(&mut self) {
        let panicking=std::thread::panicking();
        if panicking {
            self.state.history.poison();
            if let Some(ref mut raw)=self.raw {
                raw.owner_mutex_poison_observed=true;
                raw.no_replay_seal=true;
            }
        }
        let captured=self.raw.as_ref().and_then(|raw|
            self.state.history.next_revision().map(|revision|(revision,snapshot(raw))));
        let host_captured=self.raw.as_ref().map(|raw|
            self.state.host_facts.capture_from_state(raw,panicking));
        // Release the real owner Mutex before touching the concrete history store.
        drop(self.raw.take());
        if panicking { self.state.host_facts.mark_poisoned(); }
        if let Some(host_snapshot)=host_captured { self.state.host_facts.publish(host_snapshot); }
        if let Some((revision,value))=captured { self.state.history.publish(revision,value); }
        else { self.state.history.lost(); }
        self.state.history.end_access();
        self.state.host_facts.end_access();
    }
}
impl Drop for ObservedOwnerState {
    fn drop(&mut self) {
        match self.inner.get_mut() {
            Ok(value)=>{
                if let Some(revision)=self.history.next_revision() {
                    self.history.publish(revision,snapshot(value));
                }
                let host_snapshot=self.host_facts.capture_from_state(value,false);
                self.host_facts.publish(host_snapshot);
            }
            Err(poisoned)=>{
                self.history.poison();
                self.host_facts.mark_poisoned();
                let value=poisoned.into_inner();
                let host_snapshot=self.host_facts.capture_from_state(value,true);
                self.host_facts.publish(host_snapshot);
            }
        }
        // Only data survives; this does not release/reap or prolong any resource.
        self.history.retire();
        self.host_facts.retire();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_utils_pty::RunnerObservationAvailability as A;
    fn owner() -> CheckedRunnerOwner {
        CheckedRunnerOwner::new(StartupId(901),Instant::now()+Duration::from_secs(1),
            None,None,&SCOPED_CONNECT_OWNER,None)
    }
    #[test]
    fn original_owner_guard_is_busy_and_retirement_preserves_only_unknown_data() {
        // No OS handle/process/Job exists: this exercises only the real state guard.
        let owner=owner();let reader=owner.runner_observation();
        let host_reader=owner.host_native_close_facts_reader();
        {
            let mut state=owner.state.lock().unwrap();
            assert_eq!(reader.read().availability,A::Busy);
            assert_eq!(
                owner.host_native_close_facts().observation_availability,
                HostFactsObservationAvailability::BusyOwnerGuardHeld
            );
            state.protocol_exit_code=Some(7);
            state.reader_worker=WorkerSlotState::LoanedToFinalizer;
            state.no_replay_seal=true;
            state.sticky_unknown=Some(StickyUnknownReason::WriterFailed);
        }
        let borrowed=reader.read();
        assert_eq!(borrowed.availability,A::Borrowed);
        assert_eq!(borrowed.snapshot.unwrap().runner_get_exit_code_process_u32,None);
        let host_borrowed=owner.host_native_close_facts();
        assert_eq!(
            host_borrowed.observation_availability,
            HostFactsObservationAvailability::BorrowedByFinalizer
        );
        assert!(host_borrowed.runtime_workers.reader.loaned_to_finalizer);
        drop(owner);
        let retired=reader.read();
        assert_eq!(retired.availability,A::Retired);
        let snapshot=retired.snapshot.unwrap();
        assert_eq!(snapshot.first_error,Some(RunnerFailure::WriterFailed));
        assert!(snapshot.no_replay_sealed);
        assert_eq!(snapshot.runner_identity,None);
        assert_eq!(snapshot.runner_get_exit_code_process_u32,None);
        assert_eq!(snapshot.protocol_business_exit_i32,Some(7));
        let host_retired=host_reader.read();
        assert_eq!(
            host_retired.observation_availability,
            HostFactsObservationAvailability::RetiredAfterOwnerDrop
        );
        assert!(host_retired.debt_poison_and_sticky_unknown.no_replay_sealed);
        assert_eq!(
            host_retired
                .debt_poison_and_sticky_unknown
                .first_sticky_failure
                .map(|f| f.host_kind),
            Some(HostStickyFailureKind::WriterFailed)
        );
    }
    #[test]
    fn actual_owner_mutex_poison_is_not_clean_even_if_original_guard_is_recovered() {
        let owner=owner();let reader=owner.runner_observation();
        let host_reader=owner.host_native_close_facts_reader();
        let _=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            let _state=owner.state.lock().unwrap();
            panic!("pure owner state poison");
        }));
        assert!(owner.state.lock().is_err());
        assert_eq!(reader.read().availability,A::Poisoned);
        assert!(reader.read().snapshot.is_none());
        let host_poisoned=host_reader.read();
        assert_eq!(
            host_poisoned.observation_availability,
            HostFactsObservationAvailability::PoisonedOwnerMutex
        );
        assert!(host_poisoned.debt_poison_and_sticky_unknown.owner_mutex_poisoned);
        assert!(host_poisoned.debt_poison_and_sticky_unknown.no_replay_sealed);
    }
}
