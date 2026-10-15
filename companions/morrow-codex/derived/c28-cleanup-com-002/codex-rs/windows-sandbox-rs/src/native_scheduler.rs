//! Captures only the original threads at THIS scheduler's startup callbacks.
//! No PID reopen, global thread lookup, forced shutdown or Drop-as-proof.
use std::{io,sync::{Arc,Mutex},time::Instant};
use windows_sys::Win32::Foundation::{CloseHandle,DuplicateHandle,DUPLICATE_SAME_ACCESS,HANDLE,WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{GetCurrentProcess,GetCurrentThread,GetCurrentThreadId,GetExitCodeThread,WaitForSingleObject};
const LIMIT:usize=64;
struct Original {id:u32,raw:Option<usize>,stopped:bool,close_attempted:bool,closed:bool,exit:Option<u32>}
struct State {entries:[Option<Original>;LIMIT],failed:bool,first_error:Option<io::Error>,loaned:bool,consumed:bool}
fn copy_error(error:&io::Error)->io::Error {error.raw_os_error().map(io::Error::from_raw_os_error)
    .unwrap_or_else(||io::Error::new(error.kind(),error.to_string()))}
pub struct NativeSchedulerThreads {state:Mutex<State>}
/// Produced only after original OS waits and checked closes, not callback counts.
pub struct NativeSchedulerClosed { _private:() }
impl NativeSchedulerThreads {
    pub fn new()->Arc<Self> {Arc::new(Self {state:Mutex::new(State {entries:std::array::from_fn(|_|None),failed:false,first_error:None,loaned:false,consumed:false})})}
    pub fn thread_start(&self) {
        let id=unsafe{GetCurrentThreadId()};
        // Reserve bounded storage BEFORE any duplicate-handle effect.
        let index={let mut state=self.state.lock().unwrap_or_else(|p|{let mut s=p.into_inner();s.failed=true;s});
            if state.failed || state.loaned || state.consumed {state.failed=true;return;}
            let Some(index)=state.entries.iter().position(Option::is_none) else{state.failed=true;return;};
            state.entries[index]=Some(Original {id,raw:None,stopped:false,close_attempted:false,closed:false,exit:None});index
        };
        let mut raw:HANDLE=0;
        let ok=unsafe{DuplicateHandle(GetCurrentProcess(),GetCurrentThread(),GetCurrentProcess(),&mut raw,0,0,DUPLICATE_SAME_ACCESS)};
        let capture_error=(ok==0).then(io::Error::last_os_error);
        let mut state=self.state.lock().unwrap_or_else(|p|{let mut s=p.into_inner();s.failed=true;s});
        if let Some(error)=capture_error {state.failed=true;if state.first_error.is_none(){state.first_error=Some(error);}return;}
        state.entries[index].as_mut().expect("reserved original scheduler thread").raw=Some(raw as usize);
    }
    pub fn thread_stop(&self) {
        let id=unsafe{GetCurrentThreadId()};
        let mut state=self.state.lock().unwrap_or_else(|p|{let mut s=p.into_inner();s.failed=true;s});
        match state.entries.iter_mut().flatten().find(|entry|entry.id==id && !entry.stopped) {
            Some(entry)=>entry.stopped=true,None=>state.failed=true,
        }
    }
    /// Runtime shutdown must have returned before this is called. SDK additionally
    /// checks its original runtime/factory identity. Cancellation/unwind retains
    /// all original anchors in this same instance; no close attempt is repeated.
    pub fn verify_native_shutdown(&self,deadline:Instant)->io::Result<NativeSchedulerClosed> {
        let entries={let mut state=self.state.lock().map_err(|_|io::Error::other("original scheduler tracker poisoned"))?;
            if state.failed || state.loaned || state.consumed {return Err(state.first_error.as_ref().map(copy_error)
                .unwrap_or_else(||io::Error::other("original scheduler completion unavailable")));}
            if state.entries.iter().flatten().count()<2 || state.entries.iter().flatten().any(|entry|!entry.stopped || entry.raw.is_none()) {
                return Err(io::Error::new(io::ErrorKind::WouldBlock,"original scheduler stop callbacks incomplete"));
            }
            state.loaned=true;std::mem::replace(&mut state.entries,std::array::from_fn(|_|None))
        };
        struct Loan<'a>{owner:&'a NativeSchedulerThreads,entries:Option<[Option<Original>;LIMIT]>,done:bool,error:Option<io::Error>}
        impl Drop for Loan<'_>{fn drop(&mut self){let mut state=self.owner.state.lock().unwrap_or_else(|p|p.into_inner());
            state.entries=self.entries.take().unwrap();state.loaned=false;
            if self.done{state.consumed=true;}else{state.failed=true;if state.first_error.is_none(){state.first_error=Some(self.error.take()
                .unwrap_or_else(||io::Error::other("original scheduler completion unwound")));}}
        }}
        let mut loan=Loan {owner:self,entries:Some(entries),done:false,error:None};
        let result:io::Result<()>=(|| {
        for entry in loan.entries.as_mut().unwrap().iter_mut().flatten() {
            if entry.close_attempted {return Err(io::Error::other("original scheduler thread close already attempted"));}
            let raw=entry.raw.unwrap() as HANDLE;
            let remaining=deadline.saturating_duration_since(Instant::now());
            let ms=u32::try_from(remaining.as_millis()).unwrap_or(u32::MAX-1).min(u32::MAX-1);
            if remaining.is_zero() {
                return Err(io::Error::new(io::ErrorKind::TimedOut,"original scheduler native wait retained"));
            }
            let wait=unsafe{WaitForSingleObject(raw,ms)};
            if wait!=WAIT_OBJECT_0 {return Err(if wait==0x102 {io::Error::new(io::ErrorKind::TimedOut,"original scheduler native wait retained")}
                else{io::Error::last_os_error()});}
            let mut exit=0u32;
            if unsafe{GetExitCodeThread(raw,&mut exit)}==0 {return Err(io::Error::last_os_error());}
            entry.exit=Some(exit);
            entry.close_attempted=true;
            if unsafe{CloseHandle(raw)}==0 {return Err(io::Error::last_os_error());}
            entry.closed=true;entry.raw=None;
            if Instant::now()>=deadline{return Err(io::Error::new(io::ErrorKind::TimedOut,"original scheduler native close late"));}
        }
        Ok(())
        })();
        if let Err(error)=result {loan.error=Some(copy_error(&error));return Err(error);}
        loan.done=true;Ok(NativeSchedulerClosed {_private:()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_or_callback_only_scheduler_history_cannot_mint_native_shutdown() {
        let tracker=NativeSchedulerThreads::new();
        assert_eq!(tracker.verify_native_shutdown(Instant::now()).err().unwrap().kind(),io::ErrorKind::WouldBlock);
        {
            let mut state=tracker.state.lock().unwrap();
            for index in 0..2 {state.entries[index]=Some(Original {id:index as u32+1,raw:None,
                stopped:true,close_attempted:false,closed:false,exit:None});}
        }
        // Labelled callback-only model: no HANDLE is provided or queried.
        assert_eq!(tracker.verify_native_shutdown(Instant::now()).err().unwrap().kind(),io::ErrorKind::WouldBlock);
        let state=tracker.state.lock().unwrap();assert!(!state.consumed);assert!(!state.loaned);
        assert!(state.entries.iter().flatten().all(|entry|entry.raw.is_none() && !entry.closed));
    }
    #[test]
    fn sticky_scheduler_first_error_and_consumed_state_never_become_clean_empty_defaults() {
        let tracker=NativeSchedulerThreads::new();
        {let mut state=tracker.state.lock().unwrap();state.failed=true;state.first_error=Some(io::Error::from_raw_os_error(5));}
        for _ in 0..2 {assert_eq!(tracker.verify_native_shutdown(Instant::now()).err().unwrap().raw_os_error(),Some(5));}
        assert!(!tracker.state.lock().unwrap().consumed);
        let consumed=NativeSchedulerThreads::new();consumed.state.lock().unwrap().consumed=true;
        assert!(consumed.verify_native_shutdown(Instant::now()).is_err());
    }
}
