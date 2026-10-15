//! Fixed-size RUNNER history only. This module owns no process, Job, task,
//! handle, lease, Core, callback, OS query or asynchronous operation.
use std::sync::{Arc, Mutex, TryLockError};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunnerObservationKey { pub instance: u64, pub startup_attempt: u64 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerObservationAvailability {
    Unsupported, NotObserved, Busy, Poisoned, HistoryLost, WrongInstance,
    Observed, Borrowed, Retired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerLifecycle {
    ReservedBeforeLaunch, ProcessLaunchedInHandshake, HandshakeSucceededInTransport,
    CheckedIoExtracted, ActorAdopted, FailedOrAbandoned, FinalizedClean,
    PhysicallyReapedStickyUnknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerFailure {
    CallerAwaitCancelled, IdentityValidationFailed, ThreadCloseFailed,
    ConnectRetainedUnknown, LateConnectSuccess, ConnectFailed, HandshakeFailed,
    StartupUnwound, TransportDroppedUnadopted, WrongLegacyConversion,
    CheckedIoDroppedUnadopted, RuntimeLifecycleFailure, ReaderIncompleteOrFailed,
    WriterFailed, InputTaskFailedOrAborted, FinalizationDeadlineExpired,
    ProcessWaitExpiredOrFailed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerWorkerPlacement { NotInstalled, Reserved, Pending, Joined, Borrowed }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunnerWorkerHistory {
    pub placement: RunnerWorkerPlacement,
    pub within_deadline: Option<bool>,
    pub result_clean: Option<bool>,
    pub physical_eof: Option<bool>,
    pub protocol_exit_i32: Option<i32>,
    pub termination_sent: Option<bool>,
    pub joined_error: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunnerProcessIdentity { pub pid: u32, pub creation_filetime_100ns: u64 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunnerHandshakeHistory {
    pub schema: [u8; 32],
    pub close_stdin: bool,
    pub resize: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerThreadDisposition { ClosedOnCapture, CloseFailedUnknown, ScopedSynthetic }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerFinalization { NotFinalized, InProgress, CleanRelease, PhysicallyReapedStickyUnknown }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunnerHelperHistory {
    pub acknowledged: bool, pub within_deadline: bool, pub had_unknown: bool,
}
/// No raw HANDLE or nonce, and no variable-size payload. All values are copied
/// from the same owner, never inferred from a protocol Exit/empty resource map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunnerOwnerSnapshot {
    pub lifecycle: RunnerLifecycle,
    pub runner_identity: Option<RunnerProcessIdentity>,
    pub runner_get_exit_code_process_u32: Option<u32>,
    pub negotiated_handshake: Option<RunnerHandshakeHistory>,
    pub protocol_business_exit_i32: Option<i32>,
    pub reader_physical_eof_observed: bool,
    pub no_replay_sealed: bool,
    pub first_error: Option<RunnerFailure>,
    pub reader: RunnerWorkerHistory,
    pub writer: RunnerWorkerHistory,
    pub input: RunnerWorkerHistory,
    pub thread_disposition: Option<RunnerThreadDisposition>,
    pub thread_close_debt_os_error: Option<u32>,
    pub process_handle_retained: bool,
    pub native_thread_anchors_retained: bool,
    pub finalization: RunnerFinalization,
    pub finalizer_epoch: u64,
    pub worker_admission_active: bool,
    pub helpers: [Option<RunnerHelperHistory>; 2],
}
impl RunnerOwnerSnapshot {
    fn borrowed(&self) -> bool {
        self.worker_admission_active || self.finalization == RunnerFinalization::InProgress ||
        [self.reader, self.writer, self.input].iter().any(|w| w.placement == RunnerWorkerPlacement::Borrowed)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunnerObservation {
    pub availability: RunnerObservationAvailability,
    pub key: Option<RunnerObservationKey>,
    pub revision: Option<u64>,
    pub snapshot: Option<RunnerOwnerSnapshot>,
}
struct Store {
    key: Option<RunnerObservationKey>,
    revision: AtomicU64,
    active_accesses: AtomicUsize,
    poisoned: AtomicBool,
    lost: AtomicBool,
    retired: AtomicBool,
    latest: Mutex<Option<(u64, RunnerOwnerSnapshot)>>,
}
/// Trusted owner-side data publisher. It cannot operate on native resources.
pub struct RunnerObservationPublisher { store: Arc<Store> }
/// Clones retain ONLY fixed-size data, not the owner/session/backend/Job/lease.
#[derive(Clone, Default)]
pub struct RunnerObservationReader { store: Option<Arc<Store>> }
impl std::fmt::Debug for RunnerObservationReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerObservationReader").field("key", &self.key()).finish()
    }
}
impl RunnerObservationPublisher {
    pub fn new(startup_attempt: u64) -> Self {
        let instance=NEXT_INSTANCE.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|v|v.checked_add(1)).ok();
        Self { store: Arc::new(Store { key: instance.map(|instance| RunnerObservationKey { instance, startup_attempt }),
            revision: AtomicU64::new(0), active_accesses: AtomicUsize::new(0),
            poisoned: AtomicBool::new(false), lost: AtomicBool::new(false),
            retired: AtomicBool::new(false), latest: Mutex::new(None) }) }
    }
    pub fn reader(&self) -> RunnerObservationReader { RunnerObservationReader { store: Some(self.store.clone()) } }
    pub fn begin_access(&self) {
        if self.store.active_accesses.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|v|v.checked_add(1)).is_err() {self.lost();}
    }
    pub fn end_access(&self) {
        if self.store.active_accesses.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|v|v.checked_sub(1)).is_err() {self.lost();}
    }
    pub fn poison(&self) { self.store.poisoned.store(true,Ordering::SeqCst); }
    pub fn lost(&self) { self.store.lost.store(true,Ordering::SeqCst); }
    /// Called under the existing owner-state guard, to order publications even
    /// if two completed guards publish outside that lock in reverse order.
    pub fn next_revision(&self) -> Option<u64> {
        match self.store.revision.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|v|v.checked_add(1)) {
            Ok(v) => Some(v+1), Err(_) => { self.lost(); None }
        }
    }
    /// Nonblocking concrete data copy, never a callback under the owner lock.
    pub fn publish(&self, revision: u64, snapshot: RunnerOwnerSnapshot) {
        match self.store.latest.try_lock() {
            Ok(mut slot) => if slot.as_ref().is_none_or(|(old,_)|revision>*old) { *slot=Some((revision,snapshot)); },
            Err(TryLockError::WouldBlock) => self.lost(),
            Err(TryLockError::Poisoned(_)) => self.poison(),
        }
    }
    pub fn retire(&self) { self.store.retired.store(true,Ordering::SeqCst); }
}
impl RunnerObservationReader {
    pub fn unsupported() -> Self { Self::default() }
    pub fn key(&self) -> Option<RunnerObservationKey> { self.store.as_ref().and_then(|s|s.key) }
    pub fn read(&self) -> RunnerObservation { self.read_for(self.key()) }
    pub fn read_for(&self, expected: Option<RunnerObservationKey>) -> RunnerObservation {
        use RunnerObservationAvailability as A;
        let result=|availability,key|RunnerObservation { availability,key,revision:None,snapshot:None };
        let Some(store)=&self.store else { return result(A::Unsupported,None) };
        if store.key.is_none() { return result(A::NotObserved,None); }
        if store.key!=expected { return result(A::WrongInstance,store.key); }
        if store.poisoned.load(Ordering::SeqCst) { return result(A::Poisoned,store.key); }
        if store.lost.load(Ordering::SeqCst) { return result(A::HistoryLost,store.key); }
        if store.active_accesses.load(Ordering::SeqCst)!=0 { return result(A::Busy,store.key); }
        let slot=match store.latest.try_lock() {
            Ok(slot)=>slot,
            Err(TryLockError::WouldBlock)=>return result(A::Busy,store.key),
            Err(TryLockError::Poisoned(_))=>return result(A::Poisoned,store.key),
        };
        if store.active_accesses.load(Ordering::SeqCst)!=0 { return result(A::Busy,store.key); }
        if store.poisoned.load(Ordering::SeqCst) { return result(A::Poisoned,store.key); }
        if store.lost.load(Ordering::SeqCst) { return result(A::HistoryLost,store.key); }
        let Some((revision,snapshot))=*slot else { return result(A::NotObserved,store.key) };
        let availability=if store.retired.load(Ordering::SeqCst) { A::Retired }
            else if snapshot.borrowed() { A::Borrowed } else { A::Observed };
        RunnerObservation { availability,key:store.key,revision:Some(revision),snapshot:Some(snapshot) }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_busy_poison_and_wrong_instance_never_return_clean_zeros() {
        assert_eq!(RunnerObservationReader::unsupported().read().availability,RunnerObservationAvailability::Unsupported);
        let first=RunnerObservationPublisher::new(11);
        let second=RunnerObservationPublisher::new(11);
        let reader=first.reader();
        assert_ne!(reader.key(),second.reader().key());
        assert_eq!(reader.read_for(second.reader().key()).availability,RunnerObservationAvailability::WrongInstance);
        first.begin_access();
        assert_eq!(reader.read().availability,RunnerObservationAvailability::Busy);
        assert!(reader.read().snapshot.is_none());
        first.end_access();first.poison();
        assert_eq!(reader.read().availability,RunnerObservationAvailability::Poisoned);
        assert!(reader.read().snapshot.is_none());
    }
    #[test]
    fn retired_unobserved_history_is_not_native_exit() {
        let publisher=RunnerObservationPublisher::new(12);
        let reader=publisher.reader();publisher.retire();drop(publisher);
        assert_eq!(reader.read().availability,RunnerObservationAvailability::NotObserved);
        assert!(reader.read().snapshot.is_none());
    }
    fn sample() -> RunnerOwnerSnapshot {
        let worker=RunnerWorkerHistory{placement:RunnerWorkerPlacement::Pending,
            within_deadline:None,result_clean:None,physical_eof:None,protocol_exit_i32:None,
            termination_sent:None,joined_error:false};
        RunnerOwnerSnapshot{lifecycle:RunnerLifecycle::FailedOrAbandoned,runner_identity:None,
            runner_get_exit_code_process_u32:None,negotiated_handshake:None,
            protocol_business_exit_i32:Some(7),reader_physical_eof_observed:false,
            no_replay_sealed:true,first_error:Some(RunnerFailure::WriterFailed),
            reader:worker,writer:worker,input:worker,thread_disposition:None,
            thread_close_debt_os_error:Some(5),process_handle_retained:true,
            native_thread_anchors_retained:true,finalization:RunnerFinalization::NotFinalized,
            finalizer_epoch:2,worker_admission_active:false,helpers:[None,None]}
    }
    #[test]
    fn retired_history_and_borrowed_workers_keep_real_unknown_not_protocol_exit_as_dword() {
        assert!(std::mem::size_of::<RunnerOwnerSnapshot>()<=1024);
        let publisher=RunnerObservationPublisher::new(100);
        let reader=publisher.reader();
        let mut snapshot=sample();snapshot.reader.placement=RunnerWorkerPlacement::Borrowed;
        publisher.publish(1,snapshot);
        assert_eq!(reader.read().availability,RunnerObservationAvailability::Borrowed);
        assert_eq!(reader.read().snapshot.unwrap().runner_get_exit_code_process_u32,None);
        publisher.retire();drop(publisher);
        let saved=reader.read();
        assert_eq!(saved.availability,RunnerObservationAvailability::Retired);
        let value=saved.snapshot.unwrap();
        assert_eq!(value.first_error,Some(RunnerFailure::WriterFailed));
        assert!(value.no_replay_sealed);
        assert_eq!(value.thread_close_debt_os_error,Some(5));
        assert_eq!(value.protocol_business_exit_i32,Some(7));
        assert_eq!(value.runner_get_exit_code_process_u32,None);
    }
    #[test]
    fn busy_poisoned_cache_and_lost_publication_never_become_clean_history() {
        let publisher=RunnerObservationPublisher::new(101);
        let reader=publisher.reader();publisher.publish(1,sample());
        let hold=publisher.store.latest.lock().unwrap();
        assert_eq!(reader.read().availability,RunnerObservationAvailability::Busy);
        publisher.publish(2,sample()); // Deliberate test contention, no native work.
        drop(hold);
        assert_eq!(reader.read().availability,RunnerObservationAvailability::HistoryLost);
        assert!(reader.read().snapshot.is_none());
        let other=RunnerObservationPublisher::new(102);let read=other.reader();
        let _=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            let _guard=other.store.latest.lock().unwrap();
            panic!("pure history cache poison");
        }));
        assert_eq!(read.read().availability,RunnerObservationAvailability::Poisoned);
        assert!(read.read().snapshot.is_none());
    }
    #[test]
    fn wrong_attempt_and_out_of_order_publication_cannot_replace_same_instance() {
        let publisher=RunnerObservationPublisher::new(103);let reader=publisher.reader();
        let mut wrong=reader.key().unwrap();wrong.startup_attempt+=1;
        assert_eq!(reader.read_for(Some(wrong)).availability,RunnerObservationAvailability::WrongInstance);
        let first=sample();let mut second=first;second.runner_get_exit_code_process_u32=Some(0xffff_ff07);
        publisher.publish(2,second);publisher.publish(1,first);
        assert_eq!(reader.read().revision,Some(2));
        assert_eq!(reader.read().snapshot.unwrap().runner_get_exit_code_process_u32,Some(0xffff_ff07));
    }

}
