//! Fixed-size RUNNER history only. This module owns no process, Job, task,
//! handle, lease, Core, callback, OS query or asynchronous operation.
use std::sync::{Arc, Mutex, TryLockError};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
// A reader or an earlier post-unlock publication may briefly hold one slot.
// Keep a fixed number of alternatives; exhausting every slot remains a loss.
const HISTORY_SLOTS: usize = 4;
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
    published_revision: AtomicU64,
    slots: [Mutex<Option<(u64, RunnerOwnerSnapshot)>>; HISTORY_SLOTS],
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
            retired: AtomicBool::new(false), published_revision: AtomicU64::new(0),
            slots: std::array::from_fn(|_| Mutex::new(None)) }) }
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
        if self.store.published_revision.load(Ordering::SeqCst) >= revision { return; }
        for offset in 0..HISTORY_SLOTS {
            let index = ((revision % HISTORY_SLOTS as u64) as usize + offset) % HISTORY_SLOTS;
            match self.store.slots[index].try_lock() {
                Ok(mut slot) => {
                    // Another original guard may have published a newer capture
                    // first. Neither its slot nor the revision may move back.
                    if self.store.published_revision.load(Ordering::SeqCst) >= revision { return; }
                    *slot=Some((revision,snapshot));
                    self.store.published_revision.fetch_max(revision,Ordering::SeqCst);
                    return;
                }
                Err(TryLockError::WouldBlock) => continue,
                Err(TryLockError::Poisoned(_)) => { self.poison(); return; }
            }
        }
        // No waiting, spinning, allocation or native resource operation. A true
        // capacity loss is permanent and cannot be repaired by later snapshots.
        self.lost();
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
        let wanted=store.published_revision.load(Ordering::SeqCst);
        let mut captured=None;
        for slot in &store.slots {
            match slot.try_lock() {
                Ok(slot)=>if let Some((revision,snapshot))=*slot {
                    if revision==wanted { captured=Some((revision,snapshot)); break; }
                },
                Err(TryLockError::WouldBlock)=>continue,
                Err(TryLockError::Poisoned(_))=>{
                    store.poisoned.store(true,Ordering::SeqCst);
                    return result(A::Poisoned,store.key);
                },
            }
        }
        if store.active_accesses.load(Ordering::SeqCst)!=0 { return result(A::Busy,store.key); }
        if store.poisoned.load(Ordering::SeqCst) { return result(A::Poisoned,store.key); }
        if store.lost.load(Ordering::SeqCst) { return result(A::HistoryLost,store.key); }
        if store.published_revision.load(Ordering::SeqCst)!=wanted { return result(A::Busy,store.key); }
        let Some((revision,snapshot))=captured else {
            return result(if wanted==0 { A::NotObserved } else { A::Busy },store.key);
        };
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
    fn reader_contention_does_not_destroy_a_later_original_snapshot() {
        let publisher=RunnerObservationPublisher::new(101);
        let reader=publisher.reader();publisher.publish(1,sample());
        let hold=publisher.store.slots[1].lock().unwrap();
        assert_eq!(reader.read().availability,RunnerObservationAvailability::Busy);
        let mut later=sample();later.runner_get_exit_code_process_u32=Some(0xffff_ff07);
        // Revision 5 starts at the held slot, then uses a free fixed-size slot.
        std::thread::scope(|scope| {
            scope.spawn(|| publisher.publish(5,later)).join().unwrap();
        });
        assert_eq!(reader.read().revision,Some(5));
        drop(hold);
        publisher.publish(4,sample());
        let observed=reader.read();
        assert_eq!(observed.availability,RunnerObservationAvailability::Observed);
        assert_eq!(observed.revision,Some(5));
        assert_eq!(observed.snapshot,Some(later));
    }
    #[test]
    fn exhausted_cache_and_poison_never_become_clean_history() {
        let publisher=RunnerObservationPublisher::new(104);
        let reader=publisher.reader();publisher.publish(1,sample());
        let holds=publisher.store.slots.iter().map(|s|s.lock().unwrap()).collect::<Vec<_>>();
        publisher.publish(2,sample());
        drop(holds);
        publisher.publish(3,sample());
        assert_eq!(reader.read().availability,RunnerObservationAvailability::HistoryLost);
        assert!(reader.read().snapshot.is_none());
        let other=RunnerObservationPublisher::new(102);let read=other.reader();
        let _=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
            let _guard=other.store.slots[0].lock().unwrap();
            panic!("pure history cache poison");
        }));
        assert_eq!(read.read().availability,RunnerObservationAvailability::Poisoned);
        other.publish(1,sample());
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

    #[test]
    fn busy_latest_slot_never_falls_back_to_an_older_complete_snapshot() {
        let publisher=RunnerObservationPublisher::new(105);let reader=publisher.reader();
        publisher.publish(1,sample());
        let mut later=sample();later.no_replay_sealed=true;
        later.runner_get_exit_code_process_u32=Some(0xffff_ff07);
        publisher.publish(2,later);
        let hold=publisher.store.slots[2].lock().unwrap();
        assert_eq!(reader.read().availability,RunnerObservationAvailability::Busy);
        assert!(reader.read().snapshot.is_none());
        drop(hold);publisher.retire();drop(publisher);
        assert_eq!(reader.read().availability,RunnerObservationAvailability::Retired);
        assert_eq!(reader.read().revision,Some(2));
        assert_eq!(reader.read().snapshot,Some(later));
    }

}
