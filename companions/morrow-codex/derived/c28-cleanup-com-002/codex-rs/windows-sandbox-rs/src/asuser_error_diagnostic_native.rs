//! One-shot borrowed stderr for the fixed Root qualification source chain.
//! Stable preinstalled IO storage is independent of all data-only Startup Arcs.
//! No owning HANDLE/OVERLAPPED/event/thread, retry, wait, cancel or cleanup.
// No buffer/OVERLAPPED is borrowed from the data-only Startup Arc. The slot is
// independently preallocated, leaked once and NEVER moved, freed or reused.
// Even unexpected pending/async returns retain frame AND out-DWORD addresses.
use crate::asuser_error_diagnostic::{Frame,legacy_frame,startup_frame,
    ErrorDiagnosticRole,StartupFailureTrigger,MAX_ERROR_RECORD_BYTES};
use crate::asuser_parent_diagnostic as legacy;
use crate::asuser_startup_observation::{StartupSnapshot,StartupUnavailable};
use std::{cell::UnsafeCell,sync::atomic::{AtomicBool,AtomicI32,AtomicU8,AtomicU32,Ordering}};
use windows_sys::Win32::{Foundation::GetLastError,
    System::Console::{GetStdHandle,STD_ERROR_HANDLE},Storage::FileSystem::WriteFile};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum ErrorOnlyWriteState {
    NotAttempted,Preparing,InFlight,NoStderr,GetStdHandleFailed,
    ReturnedComplete,ReturnedPartial,ReturnedInvalidCount,ReturnedFailed,
    PendingBufferRetained,ReportChanging,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct ErrorOnlyWriteReport {
    pub state:ErrorOnlyWriteState,
    pub get_std_handle_error:Option<u32>,pub write_bool:Option<i32>,
    pub written:Option<u32>,pub write_error:Option<u32>,
}
struct Slot {
    claimed:AtomicBool,state:AtomicU8,frame:UnsafeCell<[u8;MAX_ERROR_RECORD_BYTES]>,
    written_by_api:UnsafeCell<u32>,get_error:AtomicU32,returned_bool:AtomicI32,
    written_copy:AtomicU32,write_error:AtomicU32,
}
// Only the one successful CAS caller writes the stable frame/out-DWORD.
// Readers NEVER access either UnsafeCell. Failed/pending calls never read
// the potentially still-owned out-DWORD; all reports use separate atomics.
unsafe impl Sync for Slot {}
impl Slot {
    fn new()->Self {Self{claimed:AtomicBool::new(false),state:AtomicU8::new(0),
        frame:UnsafeCell::new([0;MAX_ERROR_RECORD_BYTES]),written_by_api:UnsafeCell::new(0),
        get_error:AtomicU32::new(0),returned_bool:AtomicI32::new(0),
        written_copy:AtomicU32::new(0),write_error:AtomicU32::new(0)}}
}
/// Fixed Root qualification-chain emitter only, not a general stderr-mode
/// validator. No mode is inferred from a numeric handle/public boolean.
/// The reviewed chain uses original anonymous-pipe synchronous stderr.
/// One extra bounded heap allocation BEFORE the existing native attempt/
/// frontend catch. OOM may abort before any diagnostic. This is an explicit
/// retained IO-buffer debt on a failed/in-flight attempt, NOT "zero debt".
/// It never holds/duplicates/closes a HANDLE or retains a business owner.
pub struct RetainedErrorDiagnostic {slot:&'static Slot}
impl RetainedErrorDiagnostic {
    pub fn prepare()->Self {Self{slot:Box::leak(Box::new(Slot::new()))}}
    /// # Safety
    /// Caller must bind the fixed reviewed Root original anonymous stderr
    /// source chain; a numeric handle or external bool cannot establish it.
    pub(crate) unsafe fn legacy(&self,role:ErrorDiagnosticRole,
        encoded:Result<[u8;256],legacy::CodecError>,trigger:StartupFailureTrigger) {
        unsafe{self.emit(legacy_frame(role,encoded,trigger));}
    }
    /// # Safety
    /// Requires the fixed Root CreatePipe->P stderr->retained stdio->C
    /// synchronous contract. This is NOT a general redirected-stderr API.
    /// Stable retention handles unexpected pending buffer lifetime ONLY;
    /// it does not qualify arbitrary async handles or make them synchronous.
    pub unsafe fn startup(&self,snapshot:Result<StartupSnapshot,StartupUnavailable>,trigger:StartupFailureTrigger) {
        if let Some(frame)=startup_frame(snapshot,trigger){unsafe{self.emit(frame);}}
    }
    unsafe fn emit(&self,frame:Frame) {
        let slot=self.slot;
        if slot.claimed.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){return;}
        slot.state.store(1,Ordering::Release);
        // No temporary frame/out-parameter pointer crosses WriteFile.
        unsafe{(&mut *slot.frame.get())[..frame.len].copy_from_slice(frame.bytes());}
        let stderr=unsafe{GetStdHandle(STD_ERROR_HANDLE)};
        if stderr==0 {slot.state.store(3,Ordering::Release);return;}
        if stderr == -1 {
            let error=unsafe{GetLastError()};
            slot.get_error.store(error,Ordering::Relaxed);slot.state.store(4,Ordering::Release);return;
        }
        slot.state.store(2,Ordering::Release);
        // NULL OVERLAPPED is the fixed, Root-reviewed synchronous contract.
        // No new event/OV/wait/query/cancel. Stable slot is retained even if
        // that contract is unexpectedly violated and a call returns pending.
        let ok=unsafe{WriteFile(stderr,slot.frame.get().cast::<u8>().cast(),
            frame.len as u32,slot.written_by_api.get(),std::ptr::null_mut())};
        let error=if ok==0 {Some(unsafe{GetLastError()})}else{None};
        slot.returned_bool.store(ok,Ordering::Relaxed);
        if let Some(error)=error {
            slot.write_error.store(error,Ordering::Relaxed);
            // Do NOT read/reuse the native out-DWORD on ANY false result.
            slot.state.store(if error==997{9}else{8},Ordering::Release);
        } else {
            // Under the original synchronous contract TRUE completes the
            // API's out-DWORD write. Keep its address alive regardless.
            let written=unsafe{*slot.written_by_api.get()};
            slot.written_copy.store(written,Ordering::Relaxed);
            slot.state.store(if written==frame.len as u32{5}else if written<frame.len as u32{6}else{7},Ordering::Release);
        }
        // No flush/retry/Close. Caller continues its SAME original park.
    }
    /// Post-write results are local data ONLY, not bytes in that same
    /// submitted record. ROOT cannot infer them from a received frame.
    pub fn local_quality(&self)->ErrorOnlyWriteReport {
        let slot=self.slot;let first=slot.state.load(Ordering::Acquire);
        let get_error=slot.get_error.load(Ordering::Relaxed);
        let returned_bool=slot.returned_bool.load(Ordering::Relaxed);
        let written=slot.written_copy.load(Ordering::Relaxed);
        let error=slot.write_error.load(Ordering::Relaxed);
        let last=slot.state.load(Ordering::Acquire);
        let state=if first!=last{ErrorOnlyWriteState::ReportChanging}else{match first{
            0=>ErrorOnlyWriteState::NotAttempted,1=>ErrorOnlyWriteState::Preparing,
            2=>ErrorOnlyWriteState::InFlight,3=>ErrorOnlyWriteState::NoStderr,
            4=>ErrorOnlyWriteState::GetStdHandleFailed,5=>ErrorOnlyWriteState::ReturnedComplete,
            6=>ErrorOnlyWriteState::ReturnedPartial,7=>ErrorOnlyWriteState::ReturnedInvalidCount,
            8=>ErrorOnlyWriteState::ReturnedFailed,9=>ErrorOnlyWriteState::PendingBufferRetained,
            _=>ErrorOnlyWriteState::ReportChanging}};
        let stable=first==last;
        ErrorOnlyWriteReport{state,get_std_handle_error:if stable&&first==4{Some(get_error)}else{None},
            write_bool:if stable&&(5..=9).contains(&first){Some(returned_bool)}else{None},
            written:if stable&&(5..=7).contains(&first){Some(written)}else{None},
            write_error:if stable&&matches!(first,8|9){Some(error)}else{None}}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn slot_claim_is_one_shot_without_dispatching_native_io() {
        let slot=Slot::new();
        assert!(slot.claimed.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_ok());
        assert!(slot.claimed.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err());
        assert_eq!(slot.state.load(Ordering::Acquire),0);
        assert!(std::mem::size_of::<Slot>()<=1024);
    }

    #[test] fn pending_or_inflight_quality_never_claims_a_written_count_or_remote_delivery() {
        // Synthetic local slot states ONLY. No native emitter/WriteFile call.
        let pending=RetainedErrorDiagnostic::prepare();
        pending.slot.returned_bool.store(0,Ordering::Relaxed);
        pending.slot.write_error.store(997,Ordering::Relaxed);
        pending.slot.written_copy.store(77,Ordering::Relaxed);
        pending.slot.state.store(9,Ordering::Release);
        let report=pending.local_quality();
        assert_eq!(report.state,ErrorOnlyWriteState::PendingBufferRetained);
        assert_eq!(report.write_bool,Some(0));
        assert_eq!(report.write_error,Some(997));
        assert_eq!(report.written,None);
        let inflight=RetainedErrorDiagnostic::prepare();
        inflight.slot.returned_bool.store(1,Ordering::Relaxed);
        inflight.slot.written_copy.store(537,Ordering::Relaxed);
        inflight.slot.state.store(2,Ordering::Release);
        let report=inflight.local_quality();
        assert_eq!(report.state,ErrorOnlyWriteState::InFlight);
        assert_eq!((report.write_bool,report.written,report.write_error),(None,None,None));
    }
}
