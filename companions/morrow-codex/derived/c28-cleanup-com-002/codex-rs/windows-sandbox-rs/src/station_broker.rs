//! Explicit A0 resource-only broker. No production runner/credentials/target ACE.
//! The only retention root is the existing CheckedRunnerOwner admission. History
//! cannot mint a HANDLE, business completion, terminal ACK or setup authority.
use std::{io,marker::PhantomData,rc::Rc,sync::Arc};
use crate::MatchedRunnerArtifact;
use crate::elevated::checked_owner::{CheckedStartupReservation,reserve_checked_startup_before_launch,StickyUnknownReason};
use std::time::Instant;

#[path="station_broker_native.rs"]
mod native;
pub(crate) use native::BrokerCore;
#[path="station_broker_diagnostic.rs"]
mod diagnostic;
pub use diagnostic::{StationBrokerDiagnosticRead,StationBrokerDiagnosticSnapshot,
    StationBrokerHelperDiagnosticClose,STATION_BROKER_DIAGNOSTIC_BYTES,STATION_BROKER_DIAGNOSTIC_FIXTURE_SWITCH};

/// Sealed metadata clock; production cannot install an arbitrary callback under
/// a registry/connect lock. Test scripts are finite and perform no native work.
pub(crate) struct CommitClock {
    #[cfg(test)] script:Option<([Instant;8],usize,Option<usize>)>,
}
impl CommitClock {
    pub(crate) fn new()->Self {Self {#[cfg(test)] script:None}}
    pub(crate) fn sample(&mut self)->Instant {
        #[cfg(test)] if let Some((times,index,panic_at))=&mut self.script {
            let current=*index;*index=(*index+1).min(8);
            if *panic_at==Some(current) {panic!("injected metadata clock unwind");}
            return times[current.min(7)];
        }
        Instant::now()
    }
    #[cfg(test)]
    pub(crate) fn scripted(times:[Instant;8],panic_at:Option<usize>)->Self {Self {script:Some((times,0,panic_at))}}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum TimelyTakeError {Empty,Late}
/// Caller holds the original metadata mutex throughout this transaction. A
/// removed entry is never published free before the post-transition clock.
/// Err/unwind restores the SAME value before that mutex can be released.
pub(crate) fn take_timely<T>(slot:&mut Option<T>,deadline:Instant,clock:&mut CommitClock)
    ->Result<T,TimelyTakeError> {
    if clock.sample()>=deadline {return Err(TimelyTakeError::Late);}
    struct Restore<'a,T>{slot:&'a mut Option<T>,original:Option<T>}
    impl<T> Drop for Restore<'_,T> {
        fn drop(&mut self){if let Some(original)=self.original.take(){*self.slot=Some(original);}}
    }
    let original=slot.take().ok_or(TimelyTakeError::Empty)?;
    let mut restore=Restore {slot,original:Some(original)};
    if clock.sample()>=deadline {return Err(TimelyTakeError::Late);}
    // Linearization point: the post-mutation timely clock observation, while
    // other admission is still excluded. No allocation/resource API/callback.
    Ok(restore.original.take().expect("original held by restoration guard"))
}
pub(crate) fn budget_available(deadline:Instant,clock:&mut CommitClock)->bool {clock.sample()<deadline}

/// Fixed same-owner storage. An exclusive finite borrow is an atomic flag, not
/// a Core/state/registry mutex held across a native operation. The original Box
/// and all pending OVERLAPPED/reader buffers never move out of this cell.
pub(crate) struct BrokerCell {
    borrowed:std::sync::atomic::AtomicBool,
    core:std::cell::UnsafeCell<BrokerCore>,
}
// SAFETY: every access, including history reads, requires the acquire/release
// borrow below. No reference escapes its guard. Native I/O targets separately
// boxed stable buffers; the public operating facade is !Send/!Sync and checks
// the original calling thread. Dropping a cell never closes native resources.
unsafe impl Send for BrokerCell {}
unsafe impl Sync for BrokerCell {}
impl BrokerCell {
    fn new(core:BrokerCore)->Self {Self {borrowed:std::sync::atomic::AtomicBool::new(false),
        core:std::cell::UnsafeCell::new(core)}}
    pub(crate) fn try_borrow(&self)->io::Result<BrokerBorrow<'_>> {
        claim_borrow(&self.borrowed)?;
        Ok(BrokerBorrow {cell:self,_same_thread:PhantomData})
    }
}
fn claim_borrow(flag:&std::sync::atomic::AtomicBool)->io::Result<()> {
    flag.compare_exchange(false,true,std::sync::atomic::Ordering::Acquire,
        std::sync::atomic::Ordering::Relaxed)
        .map_err(|_|io::Error::new(io::ErrorKind::WouldBlock,"A0 original borrowed"))?;
    Ok(())
}
pub(crate) struct BrokerBorrow<'a> {cell:&'a BrokerCell,_same_thread:PhantomData<Rc<()>>}
impl std::ops::Deref for BrokerBorrow<'_> {
    type Target=BrokerCore;
    fn deref(&self)->&BrokerCore {unsafe{&*self.cell.core.get()}}
}
impl std::ops::DerefMut for BrokerBorrow<'_> {
    fn deref_mut(&mut self)->&mut BrokerCore {unsafe{&mut *self.cell.core.get()}}
}
impl Drop for BrokerBorrow<'_> {
    fn drop(&mut self){self.cell.borrowed.store(false,std::sync::atomic::Ordering::Release);}
}

pub const STATION_BROKER_A0_SWITCH:&str="--c28-station-broker-a0-v1";
pub const STATION_BROKER_A0_FIXTURE_SWITCH:&str="--c28-station-broker-a0-self-fixture-v1";
const FRAME_BYTES:usize=256;
const MAGIC:[u8;8]=*b"C28A0V1\0";
const HELLO:u8=1;
const OFFER:u8=2;
const ACCEPT:u8=3;
const CLOSED:u8=4;
const STDIO_CAP:usize=4096;

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum StationBrokerPhase {
    #[default] Reserved,Launching,Authenticating,Offered,AliasesHeld,HelperRetired,
    ResourceOnlyClosed,Unknown,
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum StationBrokerHandle {
    #[default] NotAllocated,InFlight,Held,ClosedVerified,Rejected,
    Uncertain,CloseFailed{error:u32},
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum StationBrokerDebt {
    Process,MainThread,Stdout,Stderr,StdoutReader,StderrReader,
    Control,ReadEvent,WriteEvent,InheritedParent,StdinNull,ChildStdout,ChildStderr,
    SecurityDescriptor,WindowStation,Desktop,OwnToken,PeerToken,CallerThreadToken,Association,
    Authentication,Protocol,Deadline,Unwind,Admission,FixtureDelivery,DiagnosticMapping,DiagnosticView,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum StationBrokerErrorKind {Win32,NtStatus,TimedOut,InvalidData,Unsupported,Unwind,Other}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct StationBrokerFailure {
    pub target:StationBrokerDebt,pub kind:StationBrokerErrorKind,
    pub win32_error:Option<u32>,pub native_ntstatus:Option<i32>,
}
#[derive(Debug)]
struct BrokerNtStatus(i32);
impl std::fmt::Display for BrokerNtStatus {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {write!(f,"A0 creator query NTSTATUS 0x{:08x}",self.0 as u32)}
}
impl std::error::Error for BrokerNtStatus {}
impl StationBrokerFailure {
    fn from_io(target:StationBrokerDebt,error:&io::Error)->Self {
        let win32_error=error.raw_os_error().map(|v|v as u32);
        let native_ntstatus=error.get_ref().and_then(|v|v.downcast_ref::<BrokerNtStatus>()).map(|v|v.0);
        let kind=if native_ntstatus.is_some() {StationBrokerErrorKind::NtStatus}
            else if win32_error.is_some() {StationBrokerErrorKind::Win32}
            else {match error.kind() {
                io::ErrorKind::TimedOut=>StationBrokerErrorKind::TimedOut,
                io::ErrorKind::InvalidData=>StationBrokerErrorKind::InvalidData,
                io::ErrorKind::Unsupported=>StationBrokerErrorKind::Unsupported,
                _=>StationBrokerErrorKind::Other,
            }};
        Self {target,kind,win32_error,native_ntstatus}
    }
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum StationBrokerConnectRelease {
    #[default] NotAttempted,InFlight,ReleasedSameStartupId,Missing,Mismatch,Busy,Poisoned,Active,LateRetained,
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum StationBrokerRegistryRelease {
    #[default] Retained,InFlight,RetainedLate,RetainedBusy,RetainedPoisoned,RetainedMismatch,
    RetainedUnwind,TimelyCommitted,
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum StationBrokerUnavailable {
    #[default] HelperNoCommittedErrorRecord,ParentBorrowed,Unbound,PendingOriginalIo,
    BudgetExpired,IdentityNotCaptured,TransportUnqualified,
    DiagnosticExternalObserverOnly,
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum StationBrokerOutput {
    #[default] NotAttempted,InFlight,WrittenWithinBudget,WrittenLate,Failed,Unwound,
    Unavailable(StationBrokerUnavailable),
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct StationBrokerDelivery {
    pub aliases:StationBrokerOutput,pub closed:StationBrokerOutput,
    pub outer_unknown:bool,pub first_failure:Option<StationBrokerFailure>,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct StationBrokerReportedCreation {pub api_returned_success:bool,pub process_id:u32,pub thread_id:u32}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum StationBrokerParentObservation {Observed(StationBrokerSnapshot),Unavailable(StationBrokerUnavailable)}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct StationBrokerObservation {
    pub parent:StationBrokerParentObservation,
    /// v1 has NO authenticated helper error receipt. Never synthesize its cause.
    pub helper_first_failure:StationBrokerUnavailable,
    pub delivery:StationBrokerDelivery,pub no_replay:Option<bool>,
}
/// Finite data-only closing states. No numeric HANDLE, name, SID or nonce.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct StationBrokerSlots {
    pub process:StationBrokerHandle,pub main_thread:StationBrokerHandle,
    pub stdout:StationBrokerHandle,pub stderr:StationBrokerHandle,
    pub child_stdout:StationBrokerHandle,pub child_stderr:StationBrokerHandle,
    pub stdin_null:StationBrokerHandle,pub inherited_parent:StationBrokerHandle,
    pub own_token:StationBrokerHandle,pub peer_token:StationBrokerHandle,
    pub caller_thread_token:StationBrokerHandle,
    pub station_sd:StationBrokerHandle,pub desktop_sd:StationBrokerHandle,
    pub readers:[StationBrokerHandle;2],pub control:StationBrokerHandle,
    pub read_event:StationBrokerHandle,pub write_event:StationBrokerHandle,
    pub read_pending:bool,pub write_pending:bool,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct StationBrokerSnapshot {
    pub generation:u64,pub phase:StationBrokerPhase,
    pub helper_pid:Option<u32>,pub helper_creation_filetime_100ns:Option<u64>,
    pub helper_raw_dword:Option<u32>,
    pub original_eof:[bool;2],pub original_reader_joined:[bool;2],
    pub helper_objects_closed_receipt:bool,
    pub station_alias:StationBrokerHandle,pub desktop_alias:StationBrokerHandle,
    pub host_association_unchanged:bool,
    pub first_failure:Option<StationBrokerFailure>,pub no_replay:bool,
    pub slots:StationBrokerSlots,
    /// Raw PI data is not a verified identity or authority. Existing PID/FT above
    /// remain None until same-original-hProcess verification actually succeeds.
    pub reported_creation:Option<StationBrokerReportedCreation>,
    pub connect_release:StationBrokerConnectRelease,pub registry_release:StationBrokerRegistryRelease,
    pub deadline_expired_observed:bool,pub delivery:StationBrokerDelivery,
    /// Additional opt-in lease debt, never counted as one of the old20 slots.
    pub diagnostic:StationBrokerDiagnosticSnapshot,
}
impl StationBrokerSnapshot {
    fn new(generation:u64)->Self {Self {generation,phase:StationBrokerPhase::Reserved,
        helper_pid:None,helper_creation_filetime_100ns:None,helper_raw_dword:None,
        original_eof:[false;2],original_reader_joined:[false;2],
        helper_objects_closed_receipt:false,station_alias:StationBrokerHandle::NotAllocated,
        desktop_alias:StationBrokerHandle::NotAllocated,host_association_unchanged:false,
        first_failure:None,no_replay:false,slots:StationBrokerSlots::default(),reported_creation:None,
        connect_release:StationBrokerConnectRelease::NotAttempted,registry_release:StationBrokerRegistryRelease::Retained,
        deadline_expired_observed:false,delivery:StationBrokerDelivery::default(),
        diagnostic:StationBrokerDiagnosticSnapshot::default()}}
}
impl StationBrokerObservation {
    fn unavailable(reason:StationBrokerUnavailable)->Self {
        Self {parent:StationBrokerParentObservation::Unavailable(reason),helper_first_failure:reason,
            delivery:StationBrokerDelivery::default(),no_replay:None}
    }
    fn from_parent(parent:StationBrokerSnapshot)->Self {
        let reason=if parent.diagnostic.enabled {StationBrokerUnavailable::DiagnosticExternalObserverOnly}
            else if parent.slots.read_pending || parent.slots.write_pending {StationBrokerUnavailable::PendingOriginalIo}
            else if parent.deadline_expired_observed {StationBrokerUnavailable::BudgetExpired}
            else if parent.helper_pid.is_none() || parent.helper_creation_filetime_100ns.is_none() {StationBrokerUnavailable::IdentityNotCaptured}
            else {StationBrokerUnavailable::HelperNoCommittedErrorRecord};
        Self {parent:StationBrokerParentObservation::Observed(parent),helper_first_failure:reason,
            delivery:parent.delivery,no_replay:Some(parent.no_replay)}
    }
}

#[derive(Clone,Copy)]
enum ReportStage {Aliases,Closed}
fn delivery_slot(h:&mut StationBrokerSnapshot,stage:ReportStage)->&mut StationBrokerOutput {
    match stage {ReportStage::Aliases=>&mut h.delivery.aliases,ReportStage::Closed=>&mut h.delivery.closed}
}
fn latch_delivery(h:&mut StationBrokerSnapshot,stage:ReportStage,state:StationBrokerOutput,failure:StationBrokerFailure) {
    *delivery_slot(h,stage)=state;h.delivery.outer_unknown=true;
    if failure.kind==StationBrokerErrorKind::TimedOut {h.deadline_expired_observed=true;}
    if h.delivery.first_failure.is_none(){h.delivery.first_failure=Some(failure);}
    h.no_replay=true;
    if h.registry_release!=StationBrokerRegistryRelease::TimelyCommitted {
        if h.first_failure.is_none(){h.first_failure=Some(failure);}
        h.phase=StationBrokerPhase::Unknown;
    } // An actual timely local commit/physical Closed is never rolled back.
}
/// Not Clone/Copy and no conversion to any business/native/setup completion type.
#[derive(Debug)]
pub struct StationBrokerResourceOnlyClosed {generation:u64}
impl StationBrokerResourceOnlyClosed {pub fn generation(&self)->u64 {self.generation}}

/// Same-thread facade: GetThreadDesktop comparisons must concern the actual
/// original calling thread, never an executor's replacement thread. The owner
/// itself remains retained in the existing bounded registry if this is dropped.
pub struct StationBrokerA0 {
    reservation:Option<CheckedStartupReservation>,history:Option<StationBrokerSnapshot>,
    deadline:Instant,delivery:StationBrokerDelivery,
    _same_thread:PhantomData<Rc<()>>,
}
impl StationBrokerA0 {
    /// ROOT fixture supplies its independently reviewed current setup path+SHA
    /// through an acquired exact artifact pin. No environment/PATH selection.
    /// None rejects BEFORE admission or any broker native allocation.
    pub(crate) fn reserve_reviewed_setup(setup:Option<Arc<MatchedRunnerArtifact>>)->io::Result<Self> {
        let setup=setup.ok_or_else(||io::Error::new(io::ErrorKind::PermissionDenied,"A0 unbound"))?;
        let reservation=reserve_checked_startup_before_launch(Arc::clone(&setup),None)?;
        let owner=reservation.owner();
        owner.station_broker.set(BrokerCell::new(BrokerCore::new(
            owner.startup_id().0,owner.startup_deadline(),setup)))
            .map_err(|_|io::Error::other("A0 original slot already installed"))?;
        let deadline=owner.startup_deadline();
        Ok(Self {reservation:Some(reservation),history:None,deadline,
            delivery:StationBrokerDelivery::default(),_same_thread:PhantomData})
    }
    /// Exactly one helper and handoff. On success helper originals/IO are retired,
    /// while the parent's actual station/desktop aliases remain Held for inspection.
    pub fn construct_and_transfer(&mut self)->io::Result<StationBrokerSnapshot> {
        self.operate(|core|core.construct_and_transfer())
    }
    fn enable_diagnostic(&mut self,case:diagnostic::Case)->io::Result<()> {
        self.operate(|core|core.enable_diagnostic(case)).map(|_|())
    }
    pub fn snapshot(&self)->io::Result<StationBrokerSnapshot> {
        if let Some(history)=self.history {return Ok(history);}
        let res=self.reservation.as_ref().ok_or_else(||io::Error::other("A0 retired"))?;
        let lock=res.owner().station_broker.get().ok_or_else(||io::Error::other("A0 unbound"))?;
        let core=lock.try_borrow()?;
        Ok(core.snapshot())
    }
    /// One nonblocking copy of already-captured data. No clock, process query,
    /// finalize, close, I/O or retry. In v1 the helper error remains unavailable.
    pub fn observation(&self)->StationBrokerObservation {
        let mut observed=match self.snapshot() {
            Ok(history)=>StationBrokerObservation::from_parent(history),
            Err(error)=>StationBrokerObservation::unavailable(if error.kind()==io::ErrorKind::WouldBlock {
                StationBrokerUnavailable::ParentBorrowed
            } else {StationBrokerUnavailable::Unbound}),
        };
        if self.delivery.aliases!=StationBrokerOutput::NotAttempted {observed.delivery.aliases=self.delivery.aliases;}
        if self.delivery.closed!=StationBrokerOutput::NotAttempted {observed.delivery.closed=self.delivery.closed;}
        observed.delivery.outer_unknown|=self.delivery.outer_unknown;
        if observed.delivery.first_failure.is_none(){observed.delivery.first_failure=self.delivery.first_failure;}
        if self.delivery.outer_unknown {observed.no_replay=Some(true);}
        observed
    }
    fn record_output(&mut self,stage:ReportStage,state:StationBrokerOutput,failure:Option<StationBrokerFailure>)
        ->io::Result<StationBrokerSnapshot> {
        *match stage {ReportStage::Aliases=>&mut self.delivery.aliases,ReportStage::Closed=>&mut self.delivery.closed}=state;
        if let Some(failure)=failure {
            self.delivery.outer_unknown=true;
            if self.delivery.first_failure.is_none(){self.delivery.first_failure=Some(failure);}
        }
        let history=if let Some(history)=self.history.as_mut() {
            if let Some(failure)=failure {latch_delivery(history,stage,state,failure);}
            else {*delivery_slot(history,stage)=state;}
            Ok(*history)
        } else if let Some(res)=self.reservation.as_ref() {
            (|| {
                let cell=res.owner().station_broker.get().ok_or_else(||io::Error::other("A0 unbound"))?;
                let mut core=cell.try_borrow()?;
                Ok(core.delivery_record(stage,state,failure))
            })()
        } else {Err(io::Error::other("A0 parent history unavailable"))};
        if failure.is_some() {
            if let Some(res)=self.reservation.as_ref() {res.owner().seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);}
        }
        history
    }
    fn retain_fixture_failure(&mut self,error:&io::Error,unwound:bool) {
        let stage=if self.delivery.closed!=StationBrokerOutput::NotAttempted
            || self.history.as_ref().is_some_and(|h|h.registry_release==StationBrokerRegistryRelease::TimelyCommitted) {
            ReportStage::Closed
        } else {ReportStage::Aliases};
        let prior=match stage {ReportStage::Aliases=>self.delivery.aliases,ReportStage::Closed=>self.delivery.closed};
        let state=if matches!(prior,StationBrokerOutput::Failed|StationBrokerOutput::Unwound|StationBrokerOutput::WrittenLate) {prior}
            else if unwound {StationBrokerOutput::Unwound}
            else {StationBrokerOutput::Unavailable(if error.kind()==io::ErrorKind::TimedOut {
                StationBrokerUnavailable::BudgetExpired
            } else {StationBrokerUnavailable::TransportUnqualified})};
        let failure=if unwound {StationBrokerFailure {target:StationBrokerDebt::FixtureDelivery,
            kind:StationBrokerErrorKind::Unwind,win32_error:None,native_ntstatus:None}}
            else {StationBrokerFailure::from_io(StationBrokerDebt::FixtureDelivery,error)};
        // In-memory latch only; failed observation cannot authorize another I/O.
        let _=self.record_output(stage,state,Some(failure));
    }
    fn report_with(&mut self,stage:ReportStage,write:impl FnOnce(StationBrokerSnapshot)->io::Result<()>,
        clock:&mut CommitClock)->io::Result<()> {
        let before=self.snapshot()?;
        let expected=match stage {
            ReportStage::Aliases=>before.phase==StationBrokerPhase::HelperRetired,
            ReportStage::Closed=>before.phase==StationBrokerPhase::ResourceOnlyClosed
                && before.registry_release==StationBrokerRegistryRelease::TimelyCommitted,
        };
        let prior=match stage {ReportStage::Aliases=>before.delivery.aliases,ReportStage::Closed=>before.delivery.closed};
        if !expected || before.no_replay || self.delivery.outer_unknown
            || prior!=StationBrokerOutput::NotAttempted {
            return Err(io::Error::other("A0 report phase already used or unavailable"));
        }
        if !budget_available(self.deadline,clock) {
            let error=io::Error::from(io::ErrorKind::TimedOut);
            self.retain_fixture_failure(&error,false);return Err(error);
        }
        // This is an invocation interval, not a claim that sync stdout is
        // cancellable/bounded. If it never returns, the original stays InFlight.
        let report=self.record_output(stage,StationBrokerOutput::InFlight,None)?;
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||write(report)));
        match result {
            Ok(Ok(())) if budget_available(self.deadline,clock)=>{
                self.record_output(stage,StationBrokerOutput::WrittenWithinBudget,None)?;Ok(())
            },
            Ok(Ok(()))=>{
                let error=io::Error::from(io::ErrorKind::TimedOut);
                let failure=StationBrokerFailure::from_io(StationBrokerDebt::FixtureDelivery,&error);
                let _=self.record_output(stage,StationBrokerOutput::WrittenLate,Some(failure));Err(error)
            },
            Ok(Err(error))=>{
                let failure=StationBrokerFailure::from_io(StationBrokerDebt::FixtureDelivery,&error);
                let _=self.record_output(stage,StationBrokerOutput::Failed,Some(failure));Err(error)
            },
            Err(_)=>{
                let failure=StationBrokerFailure {target:StationBrokerDebt::FixtureDelivery,
                    kind:StationBrokerErrorKind::Unwind,win32_error:None,native_ntstatus:None};
                let _=self.record_output(stage,StationBrokerOutput::Unwound,Some(failure));
                Err(io::Error::other("A0 original report unwound; no further I/O"))
            },
        }
    }
    fn operate(&mut self,operation:impl FnOnce(&mut BrokerCore)->io::Result<()>)->io::Result<StationBrokerSnapshot> {
        let res=self.reservation.as_ref().ok_or_else(||io::Error::other("A0 retired"))?;
        let owner=Arc::clone(res.owner());
        owner.check_station_broker_owner()?;
        let lock=owner.station_broker.get().ok_or_else(||io::Error::other("A0 unbound"))?;
        let result={
            let mut core=lock.try_borrow()?;
            // Keep the entire original core outside the unwind boundary. No raw
            // resource/OVERLAPPED/buffer/reader context can escape on Err or panic.
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||operation(&mut core)));
            match result {
                Ok(Ok(()))=>Ok(core.snapshot()),
                Ok(Err(error))=>{core.fail_unclassified(&error);Err(error)},
                Err(_)=>{core.fail(StationBrokerDebt::Unwind,None);Err(io::Error::other("A0 original unwound; retained"))},
            }
        };
        if result.is_err() {owner.seal_unknown(StickyUnknownReason::RuntimeLifecycleFailure);}
        result
    }
    pub fn retire_resource_only(&mut self)->io::Result<StationBrokerResourceOnlyClosed> {
        self.operate(|core|core.retire_aliases())?;
        let res=self.reservation.as_mut().ok_or_else(||io::Error::other("A0 retired"))?;
        let history=res.finish_station_broker_resource_only()?;
        self.history=Some(history);
        self.reservation.take();
        if Instant::now()>=self.deadline {
            let error=io::Error::from(io::ErrorKind::TimedOut);
            self.retain_fixture_failure(&error,false);return Err(error);
        }
        Ok(StationBrokerResourceOnlyClosed {generation:history.generation})
    }
}

// One fixed-size private protocol, separate from all v6/v7/v8 wires/goldens.
// First HELLO trust is from kernel peer PID + inherited original parent handle
// + actual creator/fingerprint checks, NOT a MAC made with a key in that HELLO.
// Later frames bind the same generation/identities and fresh secret nonce.
#[derive(Clone,Copy,PartialEq,Eq)]
struct Binding {generation:u64,parent_pid:u32,parent_time:u64,helper_pid:u32,helper_time:u64,
    deadline_ticks:u64,nonce:[u8;32]}
#[derive(Clone,Copy)]
struct Frame {kind:u8,binding:Binding,station:u64,desktop:u64,station_sd:[u8;32],desktop_sd:[u8;32],image:[u8;32]}
fn put64(b:&mut [u8],offset:usize,value:u64){b[offset..offset+8].copy_from_slice(&value.to_le_bytes());}
fn get64(b:&[u8],offset:usize)->u64 {u64::from_le_bytes(b[offset..offset+8].try_into().unwrap())}
fn mac(key:&[u8;32],data:&[u8])->[u8;32] {
    use sha2::{Digest,Sha256};
    let mut inner=[0x36u8;64];let mut outer=[0x5cu8;64];
    for i in 0..32 {inner[i]^=key[i];outer[i]^=key[i];}
    let mut h=Sha256::new();h.update(inner);h.update(data);let first=h.finalize();
    let mut h=Sha256::new();h.update(outer);h.update(first);h.finalize().into()
}
fn encode(frame:Frame)->[u8;FRAME_BYTES] {
    let mut b=[0u8;FRAME_BYTES];b[..8].copy_from_slice(&MAGIC);b[8]=frame.kind;
    put64(&mut b,16,frame.binding.generation);put64(&mut b,24,u64::from(frame.binding.parent_pid));
    put64(&mut b,32,frame.binding.parent_time);put64(&mut b,40,u64::from(frame.binding.helper_pid));
    put64(&mut b,48,frame.binding.helper_time);b[56..88].copy_from_slice(&frame.binding.nonce);
    put64(&mut b,88,frame.station);put64(&mut b,96,frame.desktop);
    b[104..136].copy_from_slice(&frame.station_sd);b[136..168].copy_from_slice(&frame.desktop_sd);
    b[168..200].copy_from_slice(&frame.image);
    put64(&mut b,200,frame.binding.deadline_ticks);
    let tag=mac(&frame.binding.nonce,&b[..224]);b[224..].copy_from_slice(&tag);b
}
fn decode(b:&[u8],kind:u8,expected:Option<Binding>)->io::Result<Frame> {
    let bad=||io::Error::new(io::ErrorKind::InvalidData,"A0 bounded/authenticated frame rejected");
    if !matches!(kind,HELLO|OFFER|ACCEPT|CLOSED) || b.len()!=FRAME_BYTES || b[..8]!=MAGIC || b[8]!=kind || b[9..16].iter().any(|v|*v!=0)
        || b[208..224].iter().any(|v|*v!=0) {return Err(bad());}
    let parent_pid=u32::try_from(get64(b,24)).map_err(|_|bad())?;
    let helper_pid=u32::try_from(get64(b,40)).map_err(|_|bad())?;
    let binding=Binding {generation:get64(b,16),parent_pid,parent_time:get64(b,32),helper_pid,
        helper_time:get64(b,48),deadline_ticks:get64(b,200),nonce:b[56..88].try_into().unwrap()};
    if binding.generation==0 || parent_pid==0 || helper_pid==0 || binding.parent_time==0
        || binding.helper_time==0 || binding.deadline_ticks==0 || binding.nonce==[0;32]
        || expected.is_some_and(|old|old!=binding) {return Err(bad());}
    let tag=mac(&binding.nonce,&b[..224]);
    if tag.iter().zip(&b[224..]).fold(0u8,|v,(a,b)|v|(a^b))!=0 {return Err(bad());}
    let station=get64(b,88);let desktop=get64(b,96);
    if kind==OFFER {if station==0 || desktop==0 || station==desktop {return Err(bad());}}
    else if station!=0 || desktop!=0 || b[104..168].iter().any(|v|*v!=0) {return Err(bad());}
    Ok(Frame {kind,binding,station,desktop,station_sd:b[104..136].try_into().unwrap(),
        desktop_sd:b[136..168].try_into().unwrap(),image:b[168..200].try_into().unwrap()})
}
fn message(kind:u8,binding:Binding,image:[u8;32])->Frame {
    Frame {kind,binding,station:0,desktop:0,station_sd:[0;32],desktop_sd:[0;32],image}
}
fn endpoint(nonce:&[u8;32])->String {
    use sha2::{Digest,Sha256};
    // The command line carries only a one-way endpoint correlation, never the
    // HELLO/MAC nonce itself. The authenticated original pipe carries that key.
    let digest=Sha256::digest(nonce);
    let suffix=digest.iter().map(|b|format!("{b:02x}")).collect::<String>();
    format!(r"\\.\pipe\C28StationBrokerA0-{suffix}")
}
fn valid_endpoint(value:&str)->bool {
    value.strip_prefix(r"\\.\pipe\C28StationBrokerA0-").is_some_and(|v|
        v.len()==64 && v.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}
fn names(binding:Binding)->(String,String) {
    use sha2::{Digest,Sha256};
    let digest=Sha256::digest(binding.nonce);
    let suffix=digest[..16].iter().map(|v|format!("{v:02x}")).collect::<String>();
    (format!("C28StationA0-{}-{suffix}",binding.generation),format!("C28DesktopA0-{suffix}"))
}
/// Earliest setup-binary dispatch, before provisioning/payload/log paths. A bad
/// A0 command exits78; it can never fall through to the normal setup parser.
pub fn station_broker_a0_dispatch()->Option<i32> {
    let args=std::env::args_os().collect::<Vec<_>>();
    let raw_mode=args.get(1)?;
    let mode=match raw_mode.to_str() {
        Some(v)=>v,None=>return raw_mode.to_string_lossy().starts_with("--c28-station-broker-").then_some(78),
    };
    if !mode.starts_with("--c28-station-broker-") {return None;}
    if mode==STATION_BROKER_A0_FIXTURE_SWITCH {
        if args.len()!=3 {return Some(78);}
        return Some(match args[2].to_str().and_then(parse_sha) {
            Some(sha)=>self_fixture(sha),None=>78,
        });
    }
    if mode==STATION_BROKER_DIAGNOSTIC_FIXTURE_SWITCH {
        if args.len()!=4 {return Some(78);}
        return Some(match (args[2].to_str().and_then(parse_sha),args[3].to_str().and_then(diagnostic::Case::parse)) {
            (Some(sha),Some(case))=>fixture_with_diagnostic(sha,Some(case)),_=>78,
        });
    }
    if mode==diagnostic::HELPER_SWITCH {
        if args.len()!=5 {return Some(78);}
        let Some(endpoint)=args[2].to_str().filter(|v|valid_endpoint(v)) else{return Some(78);};
        let parent=args[3].to_str().filter(|v|!v.is_empty() && v.len()<=20 && !v.starts_with('0')
            && v.bytes().all(|b|b.is_ascii_digit())).and_then(|v|v.parse::<usize>().ok())
            .filter(|v|*v>0 && *v<=isize::MAX as usize);
        let case=args[4].to_str().and_then(diagnostic::Case::parse);
        return Some(match (parent,case) {(Some(parent),Some(case))=>native::helper_entry_diagnostic(endpoint,parent as isize,Some(case)),_=>78});
    }
    if mode!=STATION_BROKER_A0_SWITCH || args.len()!=4 {return Some(78);}
    let Some(endpoint)=args[2].to_str().filter(|v|valid_endpoint(v)) else{return Some(78);};
    let parent=match args[3].to_str().and_then(|v|v.parse::<usize>().ok()) {
        Some(v) if v>0 && v<=isize::MAX as usize=>v as isize,_=>return Some(78),
    };
    Some(native::helper_entry(endpoint,parent))
}
fn parse_sha(value:&str)->Option<[u8;32]> {
    if value.len()!=64 || !value.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)) {return None;}
    let mut sha=[0;32];
    for (i,pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let digit=|b:u8|if b.is_ascii_digit(){b-b'0'}else{b-b'a'+10};
        sha[i]=(digit(pair[0])<<4)|digit(pair[1]);
    }
    (sha!=[0;32]).then_some(sha)
}
/// ROOT-callable native fixture in the selected setup PE itself. Only a SHA
/// may be supplied: no arbitrary executable path, command, environment or SID.
/// Qualification rejection is before admission; any later failure parks this
/// existing parent, retaining the same owner and originals for ROOT's review.
fn self_fixture(sha:[u8;32])->i32 {
    fixture_with_diagnostic(sha,None)
}
fn fixture_with_diagnostic(sha:[u8;32],case:Option<diagnostic::Case>)->i32 {
    let setup=match std::env::current_exe().ok().and_then(|p|MatchedRunnerArtifact::acquire(&p,sha).ok()) {
        Some(pin)=>Arc::new(pin),None=>return 78,
    };
    let mut fixture=match StationBrokerA0::reserve_reviewed_setup(Some(setup)) {Ok(v)=>v,Err(_)=>return 78};
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||->io::Result<()> {
        use std::io::Write;
        if let Some(case)=case {fixture.enable_diagnostic(case)?;}
        let held=fixture.construct_and_transfer()?;
        if !native::helper_retired_gate(&held) || held.station_alias!=StationBrokerHandle::Held
            || held.desktop_alias!=StationBrokerHandle::Held {
            return Err(io::Error::other("A0 real fixture alias/native/IO facts incomplete"));
        }
        fixture.report_with(ReportStage::Aliases,|report|
            writeln!(std::io::stdout(),"A0_RESOURCE_ONLY_ALIASES {report:?}"),&mut CommitClock::new())?;
        let local=fixture.retire_resource_only()?;let history=fixture.snapshot()?;
        if local.generation()!=held.generation || history.generation!=held.generation
            || history.phase!=StationBrokerPhase::ResourceOnlyClosed || !native::helper_closed_facts(&history)
            || history.station_alias!=StationBrokerHandle::ClosedVerified
            || history.desktop_alias!=StationBrokerHandle::ClosedVerified {
            return Err(io::Error::other("A0 real fixture local closing facts incomplete"));
        }
        fixture.report_with(ReportStage::Closed,|report|
            writeln!(std::io::stdout(),"A0_RESOURCE_ONLY_CLOSED {report:?}"),&mut CommitClock::new())?;
        if Instant::now()>=fixture.deadline {return Err(io::Error::from(io::ErrorKind::TimedOut));}
        Ok(())
    }));
    if matches!(result,Ok(Ok(()))) {return 0;}
    match result {
        Ok(Err(error))=>fixture.retain_fixture_failure(&error,false),
        Err(_)=>fixture.retain_fixture_failure(&io::Error::from(io::ErrorKind::Other),true),
        Ok(Ok(()))=>unreachable!(),
    }
    // No synchronous error stdout/stderr after failure/Unknown. Ordinary
    // inherited streams are not qualified bounded/cancellable transports.
    // This finite data-only observation is not a guaranteed external receipt.
    let observation=fixture.observation();
    loop {std::hint::black_box((&fixture,&observation));std::thread::park();}
}
