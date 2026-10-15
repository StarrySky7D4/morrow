//! Same LocalExecProcess instance only; no PID or latest-process lookup.
use super::*;
use std::{io,sync::Mutex as StdMutex,time::Instant};
use tokio::task::JoinHandle;
pub(crate) struct NativeProcessCompletion {
    state:StdMutex<State>,
    session:StdMutex<Option<Arc<codex_utils_pty::ExecCommandSession>>>,
    originals:[StdMutex<Option<JoinHandle<()>>>;4],
}
struct State {installed:[bool;4],joined:[bool;4],outcomes:[Option<OriginalTaskJoin>;4],active:bool,failed:bool,taken:bool,network:Option<bool>}
#[derive(Clone,Copy)]
enum OriginalTaskJoin {JoinedTimely,JoinedLate,Cancelled,Panicked}
impl NativeProcessCompletion {
    pub(super) fn new(session:Arc<codex_utils_pty::ExecCommandSession>)->Arc<Self> {
        Arc::new(Self {state:StdMutex::new(State {installed:[false;4],joined:[false;4],outcomes:[None;4],active:false,failed:false,taken:false,network:None}),
            session:StdMutex::new(Some(session)),originals:std::array::from_fn(|_|StdMutex::new(None))})
    }
    pub(super) fn install(&self,index:usize,original:JoinHandle<()>) {
        // Anchor immediately after the original spawn, before Start is exposed.
        let mut slot=self.originals[index].lock().unwrap_or_else(|p|p.into_inner());
        assert!(slot.is_none());*slot=Some(original);drop(slot);
        let mut state=self.state.lock().unwrap_or_else(|p|p.into_inner());
        if state.installed[index]{state.failed=true;}state.installed[index]=true;
    }
    pub(super) fn network_returned(&self,success:bool) {
        let mut state=self.state.lock().unwrap_or_else(|p|p.into_inner());
        if state.network.is_some() || !success {state.failed=true;}state.network=Some(success);
    }
    pub(super) async fn complete(self:&Arc<Self>,id:&ProcessId,wake:&watch::Sender<u64>,inner:&Arc<Inner>,deadline:Instant)
        ->io::Result<codex_sandboxing::NativeHostCompletion> {
        {
            let mut state=self.state.lock().map_err(|_|io::Error::other("original local completion poisoned"))?;
            if state.failed || state.taken {return Err(io::Error::other("original local completion sealed or consumed"));}
            if state.active || !state.installed.into_iter().all(|v|v) {return Err(io::Error::new(io::ErrorKind::WouldBlock,"original local tasks not yet available"));}
            state.active=true;
        }
        struct Admission<'a>{owner:&'a NativeProcessCompletion,done:bool}
        impl Drop for Admission<'_>{fn drop(&mut self){let mut s=self.owner.state.lock().unwrap_or_else(|p|p.into_inner());s.active=false;
            if !self.done{s.failed=true;drop(s);
                let session=self.owner.session.lock().unwrap_or_else(|p|p.into_inner()).clone();
                if let Some(session)=session{session.record_native_outer_failure();}
            }
        }}
        let mut admission=Admission {owner:self,done:false};
        for index in 0..4 {
            struct Loan<'a>{slot:&'a StdMutex<Option<JoinHandle<()>>>,original:Option<JoinHandle<()>>}
            impl Drop for Loan<'_>{fn drop(&mut self){if let Some(h)=self.original.take(){let mut slot=self.slot.lock().unwrap_or_else(|p|p.into_inner());assert!(slot.is_none());*slot=Some(h);}}}
            let original=self.originals[index].lock().map_err(|_|io::Error::other("original local task slot poisoned"))?.take()
                .ok_or_else(||io::Error::other("original local task missing or loaned"))?;
            let mut loan=Loan {slot:&self.originals[index],original:Some(original)};
            let result=tokio::time::timeout(deadline.saturating_duration_since(Instant::now()),loan.original.as_mut().unwrap()).await
                .map_err(|_|io::Error::new(io::ErrorKind::TimedOut,"original local task retained"))?;
            loan.original.take();
            let timely=Instant::now()<deadline;
            {let mut state=self.state.lock().map_err(|_|io::Error::other("original local task state poisoned"))?;
                state.joined[index]=true;state.outcomes[index]=Some(match &result {
                    Ok(()) if timely=>OriginalTaskJoin::JoinedTimely,Ok(())=>OriginalTaskJoin::JoinedLate,
                    Err(error) if error.is_cancelled()=>OriginalTaskJoin::Cancelled,Err(_)=>OriginalTaskJoin::Panicked,
                });}
            result.map_err(|_|io::Error::other("original local task panicked or cancelled"))?;
            if !timely{return Err(io::Error::new(io::ErrorKind::TimedOut,"original local task joined late"));}
        }
        {
            let state=self.state.lock().map_err(|_|io::Error::other("original local completion poisoned"))?;
            if state.failed || state.network!=Some(true){return Err(io::Error::other("original network shutdown not proven"));}
        }
        let session=self.session.lock().map_err(|_|io::Error::other("original local session poisoned"))?.clone()
            .ok_or_else(||io::Error::other("original local session absent"))?;
        let erased=session.take_native_completion(deadline).await?;
        let completion=erased.downcast::<codex_sandboxing::NativeHostCompletion>()
            .map_err(|_|io::Error::other("unsupported concrete native completion"))?;
        let original={
            // No IO/wait/callback under the process-map lock.
            let mut map=inner.processes.lock().await;
            let same=matches!(map.get(id),Some(ProcessEntry::Running(process)) if process.closed
                && process.open_streams==0 && process.exit_code.is_some()
                && process.wake_tx.same_channel(wake)
                && process.events.native_completion().as_ref().is_some_and(|n|Arc::ptr_eq(n,self)));
            if !same{return Err(io::Error::other("original process map instance changed"));}
            // All fallible resource admission precedes removing this original
            // map entry. Use nonblocking locks under the map guard: contention
            // or poison retains the entry, session and completion obligation.
            let mut held_session=self.session.try_lock()
                .map_err(|_|io::Error::other("original session busy or poisoned before retirement"))?;
            let mut state=self.state.try_lock()
                .map_err(|_|io::Error::other("local completion busy or poisoned before retirement"))?;
            if state.failed || state.taken || !state.active
                || !held_session.as_ref().is_some_and(|actual|Arc::ptr_eq(actual,&session)) {
                return Err(io::Error::other("original local retirement relation changed"));
            }
            let original=map.remove(id);
            let held_session=held_session.take();state.taken=true;
            drop(state);drop(map);
            (original,held_session)
        };
        drop((original,session));
        admission.done=true;Ok(*completion)
    }
}
