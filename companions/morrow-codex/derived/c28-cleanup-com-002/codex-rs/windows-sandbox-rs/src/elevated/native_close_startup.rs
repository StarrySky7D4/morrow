//! Two ordered startup writes using the original pre-staged pipe alias and one
//! bounded owned native worker at a time. No extra execution slot or global map.
use super::native_wait::{self, NativeWaitMode};
use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::sync::{Arc,Mutex,MutexGuard};
use std::thread::JoinHandle;
use std::time::Instant;
use windows_sys::Win32::Foundation::{HANDLE,WAIT_OBJECT_0};
fn copy(error:&io::Error)->io::Error {
    error.raw_os_error().map(io::Error::from_raw_os_error)
        .unwrap_or_else(||io::Error::new(error.kind(),error.to_string()))
}
struct State {
    original:Option<Arc<File>>,
    worker:Option<JoinHandle<io::Result<()>>>,
    completed:u8,
    in_flight:bool,
    reaping:bool,
    first_error:Option<io::Error>,
}
pub(super) struct NativeStartupWriter {state:Mutex<State>}
impl NativeStartupWriter {
    pub(super) fn new(original:File)->Self {Self {state:Mutex::new(State {
        original:Some(Arc::new(original)),worker:None,completed:0,in_flight:false,reaping:false,first_error:None})}}
    fn lock(&self)->MutexGuard<'_,State> {
        match self.state.lock() {Ok(state)=>state,Err(poison)=>{
            let mut state=poison.into_inner();
            if state.first_error.is_none() {state.first_error=Some(io::Error::other("v8 startup writer bookkeeping poisoned"));}
            state
        }}
    }
    fn fail(&self,error:io::Error)->io::Error {
        let mut state=self.lock();
        if state.first_error.is_none() {state.first_error=Some(error);}
        copy(state.first_error.as_ref().unwrap())
    }
    pub(super) fn write<F>(&self,index:u8,deadline:Instant,write:F)->io::Result<()>
    where F:FnOnce(&File)->io::Result<()>+Send+'static {
        let original={
            let mut state=self.lock();
            if let Some(error)=&state.first_error {return Err(copy(error));}
            if state.in_flight || state.worker.is_some() || state.completed!=index || index>=2 {
                drop(state);
                return Err(self.fail(io::Error::other("v8 startup write already admitted or out of order; no replay")));
            }
            if Instant::now()>=deadline {drop(state);return Err(self.fail(io::Error::new(io::ErrorKind::TimedOut,"v8 startup deadline expired")));}
            let original=Arc::clone(state.original.as_ref().ok_or_else(||io::Error::other("original startup endpoint absent"))?);
            state.in_flight=true;original
        };
        // The parent State owns the original BEFORE the spawner; closure owns
        // only an Arc. Spawn error/panic never drops the original endpoint.
        let worker=match std::thread::Builder::new().name("v8-original-startup-writer".into()).spawn(move || write(&original)) {
            Ok(worker)=>worker,Err(error)=>return Err(self.fail(error)),
        };
        // Private bookkeeping recovery anchors this exact returned handle even
        // if poison was noticed meanwhile. No IO/wait/callback under this lock.
        self.lock().worker=Some(worker);
        self.reap(deadline)
    }
    pub(super) fn reap(&self,deadline:Instant)->io::Result<()> {
        native_wait::run_blocking(self.reap_with_mode(deadline, NativeWaitMode::Blocking))
    }
    pub(super) async fn reap_with_mode(&self,deadline:Instant,mode:NativeWaitMode)->io::Result<()> {
        let raw={let mut state=self.lock();
            if state.reaping || (state.in_flight && state.worker.is_none() && state.first_error.is_none()) {
                return Err(io::Error::new(io::ErrorKind::WouldBlock,"same original startup writer is loaned/spawning"));
            }
            state.reaping=true;state.worker.as_ref().map(|w|w.as_raw_handle() as usize)
        };
        struct ReapLoan<'a>(&'a NativeStartupWriter);
        impl Drop for ReapLoan<'_> {fn drop(&mut self){self.0.lock().reaping=false;}}
        let _loan=ReapLoan(self);
        if let Some(raw)=raw {
            let observed=native_wait::wait_with(mode,deadline,|millis|native_wait::observe_original(raw as HANDLE,millis)).await;
            if observed.return_value!=WAIT_OBJECT_0 {
                return Err(self.fail(if observed.return_value==0x102 {io::Error::new(io::ErrorKind::TimedOut,"original v8 startup writer retained")}
                    else{observed.os_error.map(io::Error::from_raw_os_error)
                        .unwrap_or_else(||io::Error::other("unexpected original startup worker wait result"))}));
            }
            let worker=self.lock().worker.take().expect("same original startup worker");
            let outcome=worker.join().unwrap_or_else(|_|Err(io::Error::other("original v8 startup writer panicked")));
            if let Err(error)=outcome {return Err(self.fail(error));}
            if Instant::now()>=deadline {return Err(self.fail(io::Error::new(io::ErrorKind::TimedOut,"original v8 startup writer joined late")));}
            let mut state=self.lock();
            if state.first_error.is_none() {state.completed+=1;state.in_flight=false;}
        }
        let state=self.lock();
        if let Some(error)=&state.first_error {return Err(copy(error));}Ok(())
    }
    pub(super) fn take_original_after_both_joins(&self)->io::Result<File> {
        let original={let mut state=self.lock();
            if let Some(error)=&state.first_error {return Err(copy(error));}
            if state.completed!=2 || state.in_flight || state.worker.is_some() {
                return Err(io::Error::new(io::ErrorKind::WouldBlock,"original v8 startup users not joined"));
            }
            state.original.take().ok_or_else(||io::Error::other("original startup endpoint already transferred"))?
        };
        match Arc::try_unwrap(original) {Ok(file)=>Ok(file),Err(original)=>{
            self.lock().original=Some(original);
            Err(self.fail(io::Error::new(io::ErrorKind::WouldBlock,"original startup endpoint still aliased")))
        }}
    }
}


/// Existing pre-staged READ alias stays in the same owner across every startup
/// error/unwind. A complete-frame peek and post-read original deadline check
/// bound this synchronous reader without holding a Core/owner mutex across IO.
pub(super) struct NativeStartupReader {original:Mutex<Option<Arc<File>>>}
impl NativeStartupReader {
    pub(super) fn new(original:File)->Self {Self {original:Mutex::new(Some(Arc::new(original)))}}
    pub(super) fn read<T>(&self,deadline:Instant,read:impl FnOnce(&File)->io::Result<T>)->io::Result<T> {
        let original=self.original.lock().map_err(|_|io::Error::other("original startup read bookkeeping poisoned"))?
            .as_ref().cloned().ok_or_else(||io::Error::other("original startup reader already retired"))?;
        crate::framed_io::wait_for_complete_frame_with_cap(&original,deadline,super::native_close_protocol::MAX_NATIVE_CLOSE_FRAME_BYTES).map_err(io::Error::other)?;
        let value=read(&original)?;
        if Instant::now()>=deadline {return Err(io::Error::new(io::ErrorKind::TimedOut,"original v8 startup read returned late"));}
        Ok(value)
    }
    pub(super) fn take_original(&self)->io::Result<File> {
        let original=self.original.lock().map_err(|_|io::Error::other("original startup reader poisoned"))?
            .take().ok_or_else(||io::Error::other("original startup reader already retired"))?;
        match Arc::try_unwrap(original) {Ok(file)=>Ok(file),Err(original)=>{
            *self.original.lock().unwrap_or_else(|poison|poison.into_inner())=Some(original);
            Err(io::Error::new(io::ErrorKind::WouldBlock,"same original startup reader still loaned"))
        }}
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read,Seek,SeekFrom,Write};
    use std::time::Duration;
    #[test]
    fn two_ordered_native_startup_workers_join_then_transfer_the_exact_original_file() {
        let original=tempfile::tempfile().unwrap();let identity=original.as_raw_handle();
        let owner=NativeStartupWriter::new(original);
        owner.write(0,Instant::now()+Duration::from_secs(2),|file|{let mut writer=file;writer.write_all(b"hello")}).unwrap();
        {let state=owner.lock();assert_eq!(state.completed,1);assert!(state.worker.is_none());
            assert_eq!(state.original.as_ref().unwrap().as_raw_handle(),identity);}
        owner.write(1,Instant::now()+Duration::from_secs(2),|file|{let mut writer=file;writer.write_all(b"spawn")}).unwrap();
        let mut original=owner.take_original_after_both_joins().unwrap();
        assert_eq!(original.as_raw_handle(),identity);original.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes=Vec::new();original.read_to_end(&mut bytes).unwrap();assert_eq!(bytes,b"hellospawn");
        assert!(owner.lock().worker.is_none());assert!(owner.lock().original.is_none());
    }
    #[test]
    fn expired_startup_budget_keeps_original_and_never_spawns_or_retries_a_write() {
        let original=tempfile::tempfile().unwrap();let identity=original.as_raw_handle();
        let owner=NativeStartupWriter::new(original);
        let attempted=Arc::new(std::sync::atomic::AtomicBool::new(false));let mark=Arc::clone(&attempted);
        assert_eq!(owner.write(0,Instant::now()-Duration::from_secs(1),move |_| {
            mark.store(true,std::sync::atomic::Ordering::SeqCst);Ok(())
        }).unwrap_err().kind(),io::ErrorKind::TimedOut);
        let mark=Arc::clone(&attempted);
        assert_eq!(owner.write(0,Instant::now()+Duration::from_secs(2),move |_| {
            mark.store(true,std::sync::atomic::Ordering::SeqCst);Ok(())
        }).unwrap_err().kind(),io::ErrorKind::TimedOut);
        assert!(!attempted.load(std::sync::atomic::Ordering::SeqCst));
        let state=owner.lock();assert!(state.worker.is_none());assert_eq!(state.completed,0);
        assert_eq!(state.original.as_ref().unwrap().as_raw_handle(),identity);
        assert!(state.original.as_ref().unwrap().metadata().is_ok());
    }
    #[test]
    fn original_startup_write_error_is_native_joined_but_endpoint_and_first_error_stay_owned() {
        let original=tempfile::tempfile().unwrap();let identity=original.as_raw_handle();
        let owner=NativeStartupWriter::new(original);
        let error=owner.write(0,Instant::now()+Duration::from_secs(2),|_|Err(io::Error::from_raw_os_error(5))).unwrap_err();
        assert_eq!(error.raw_os_error(),Some(5));
        {let state=owner.lock();assert!(state.worker.is_none());assert_eq!(state.completed,0);
            assert_eq!(state.original.as_ref().unwrap().as_raw_handle(),identity);
            assert!(state.original.as_ref().unwrap().metadata().is_ok());}
        let attempted=Arc::new(std::sync::atomic::AtomicBool::new(false));let mark=Arc::clone(&attempted);
        assert_eq!(owner.write(0,Instant::now()+Duration::from_secs(2),move |_| {
            mark.store(true,std::sync::atomic::Ordering::SeqCst);Ok(())
        }).unwrap_err().raw_os_error(),Some(5));
        assert!(!attempted.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(owner.take_original_after_both_joins().unwrap_err().raw_os_error(),Some(5));
    }
    #[tokio::test(flavor = "current_thread")]
    async fn cooperative_startup_reap_keeps_original_worker_and_yields_to_release_timer() {
        let owner=NativeStartupWriter::new(tempfile::tempfile().unwrap());
        let original=owner.lock().original.as_ref().unwrap().as_raw_handle() as usize;
        let (tx,rx)=std::sync::mpsc::channel::<()>();
        let worker=std::thread::spawn(move || {rx.recv().unwrap();Ok(())});
        {let mut state=owner.lock();state.in_flight=true;state.worker=Some(worker);}
        let release=async move {tokio::time::sleep(Duration::from_millis(15)).await;tx.send(()).unwrap();};
        let (result,())=tokio::join!(owner.reap_with_mode(Instant::now()+Duration::from_secs(2),NativeWaitMode::Cooperative),release);
        result.unwrap();let state=owner.lock();assert_eq!(state.completed,1);assert!(!state.in_flight);
        assert!(state.worker.is_none());assert!(state.first_error.is_none());
        assert_eq!(state.original.as_ref().unwrap().as_raw_handle() as usize,original);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelling_cooperative_startup_reap_restores_loan_and_original_worker() {
        let owner=NativeStartupWriter::new(tempfile::tempfile().unwrap());
        let (tx,rx)=std::sync::mpsc::channel::<()>();
        let worker=std::thread::spawn(move || {rx.recv().unwrap();Ok(())});
        let original_worker=worker.as_raw_handle() as usize;
        {let mut state=owner.lock();state.in_flight=true;state.worker=Some(worker);}
        {
            let future=owner.reap_with_mode(Instant::now()+Duration::from_secs(2),NativeWaitMode::Cooperative);
            tokio::pin!(future);
            assert!(tokio::time::timeout(Duration::from_millis(20),&mut future).await.is_err());
            assert!(owner.lock().reaping);
        }
        {let state=owner.lock();assert!(!state.reaping);assert!(state.in_flight);
            assert!(state.first_error.is_none());assert_eq!(state.completed,0);
            assert_eq!(state.worker.as_ref().unwrap().as_raw_handle() as usize,original_worker);}
        tx.send(()).unwrap();owner.reap_with_mode(Instant::now()+Duration::from_secs(2),NativeWaitMode::Cooperative).await.unwrap();
        assert_eq!(owner.lock().completed,1);assert!(owner.lock().worker.is_none());
    }

}
