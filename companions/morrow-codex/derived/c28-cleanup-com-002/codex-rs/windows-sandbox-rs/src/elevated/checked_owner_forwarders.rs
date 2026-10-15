//! Failure retention/reap of the exact v8 forwarding handles moved back from
//! utils/pty on Drop. These physical joins NEVER clear prior Unknown/noReplay,
//! release capacity, manufacture EOF or produce a completion capability.
use super::*;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum ForwarderJoin {PhysicallyJoined,PhysicallyJoinedLate,Cancelled,Panicked}
pub(super) struct NativeForwarderRetention {
    pub(super) originals:[Option<TokioJoinHandle<()>>;4],
    outcomes:[Option<ForwarderJoin>;4],
    pub(super) loaned:bool,
}
impl NativeForwarderRetention {
    pub(super) fn new()->Self {Self {originals:std::array::from_fn(|_|None),outcomes:[None;4],loaned:false}}
}
struct Loan<'a>{owner:&'a CheckedRunnerOwner,originals:[Option<TokioJoinHandle<()>>;4],outcomes:[Option<ForwarderJoin>;4],done:bool}
impl Drop for Loan<'_> {
    fn drop(&mut self){
        let mut retained=self.owner.native_forwarders.lock().unwrap_or_else(|p|p.into_inner());
        assert!(retained.loaned && retained.originals.iter().all(Option::is_none));
        retained.originals=std::mem::replace(&mut self.originals,std::array::from_fn(|_|None));
        retained.outcomes=self.outcomes;retained.loaned=false;drop(retained);
        if !self.done {self.owner.record_runtime_lifecycle_failure();}
    }
}
impl CheckedRunnerOwner {
    fn loan_native_forwarders(&self)->io::Result<Option<Loan<'_>>> {
        let mut retained=self.native_forwarders.try_lock().map_err(|_|io::Error::other("original forwarding retention busy or poisoned"))?;
        if retained.loaned{return Err(io::Error::new(io::ErrorKind::WouldBlock,"same original forwarding handles already loaned"));}
        if retained.originals.iter().all(Option::is_none){return Ok(None);}
        retained.loaned=true;
        Ok(Some(Loan {owner:self,originals:std::mem::replace(&mut retained.originals,std::array::from_fn(|_|None)),
            outcomes:retained.outcomes,done:false}))
    }
    fn record_forwarder_result(loan:&mut Loan<'_>,index:usize,result:Result<(),tokio::task::JoinError>,deadline:Instant)->io::Result<()> {
        // Ready is an actual await/poll result of THIS original JoinHandle.
        loan.originals[index].take();
        let timely=Instant::now()<deadline;
        loan.outcomes[index]=Some(match &result {
            Ok(()) if timely=>ForwarderJoin::PhysicallyJoined,
            Ok(())=>ForwarderJoin::PhysicallyJoinedLate,
            Err(error) if error.is_cancelled()=>ForwarderJoin::Cancelled,
            Err(_)=>ForwarderJoin::Panicked,
        });
        result.map_err(|_|io::Error::other("original forwarding task cancelled or panicked"))?;
        if !timely{return Err(io::Error::new(io::ErrorKind::TimedOut,"original forwarding task joined late"));}Ok(())
    }
    pub(super) async fn reap_native_forwarders_async(&self,deadline:Instant)->io::Result<()> {
        let Some(mut loan)=self.loan_native_forwarders()? else{return Ok(())};
        for index in 0..4 {
            let Some(original)=loan.originals[index].as_mut() else{continue};
            let result=tokio::time::timeout(deadline.saturating_duration_since(Instant::now()),original).await
                .map_err(|_|io::Error::new(io::ErrorKind::TimedOut,"original forwarding handle retained"))?;
            Self::record_forwarder_result(&mut loan,index,result,deadline)?;
        }
        loan.done=true;Ok(())
    }
    pub(super) fn reap_native_forwarders_blocking(&self,deadline:Instant)->io::Result<()> {
        let Some(mut loan)=self.loan_native_forwarders()? else{return Ok(())};
        let signal=Arc::new(SignalWaker {notified:Mutex::new(false),cv:Condvar::new()});
        let waker=Waker::from(Arc::clone(&signal));
        for index in 0..4 {
            loop {
                let Some(original)=loan.originals[index].as_mut() else{break};
                let mut context=TaskContext::from_waker(&waker);
                if let Poll::Ready(result)=Pin::new(original).poll(&mut context) {
                    Self::record_forwarder_result(&mut loan,index,result,deadline)?;break;
                }
                let remaining=deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero(){return Err(io::Error::new(io::ErrorKind::TimedOut,"original forwarding handle retained"));}
                let notified=signal.notified.lock().map_err(|_|io::Error::other("forwarder waker poisoned"))?;
                if *notified {drop(notified);*signal.notified.lock().map_err(|_|io::Error::other("forwarder waker poisoned"))?=false;continue;}
                let (mut notified,_)=signal.cv.wait_timeout(notified,remaining.min(Duration::from_millis(10)))
                    .map_err(|_|io::Error::other("forwarder waker wait poisoned"))?;
                *notified=false;
            }
        }
        loan.done=true;Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_outer_failure_downgrades_cached_pending_and_keeps_first_ack_os_error() {
        let owner=Arc::new(CheckedRunnerOwner::new(StartupId(2802),Instant::now()+Duration::from_secs(10),None,None,&SCOPED_CONNECT_OWNER,None));
        {
            // Cached-state negative fixture only. No original process, native
            // wait, EOF, completion capability or physical close is fabricated.
            let mut state=owner.state.lock().unwrap();
            state.finalized_outcome=Some(CheckedFinalizationOutcome::HostNativeClosedOuterScopePending {
                startup_id:StartupId(2802),protocol_exit_code:7,native_exit_code:7,release_eligible:false});
        }
        owner.record_native_terminal_ack_failure(&io::Error::from_raw_os_error(5));
        owner.record_native_terminal_ack_failure(&io::Error::from_raw_os_error(6));
        owner.record_runtime_lifecycle_failure();
        let state=owner.state.lock().unwrap();
        assert!(matches!(state.finalized_outcome,Some(CheckedFinalizationOutcome::PhysicallyReapedWithStickyUnknown {
            native_exit_code:Some(7),no_replay_sealed:true,release_eligible:false,..})));
        assert!(matches!(state.first_error_fact.as_ref().unwrap().protocol_first_error,
            super::super::super::native_close_protocol::FirstErrorFact::Recorded {os_error:Some(5),..}));
        assert!(state.no_replay_seal);drop(state);
        assert!(owner.take_native_host_completion(Instant::now()+Duration::from_secs(1)).is_err());
    }
    #[tokio::test]
    async fn cancelled_forwarder_reap_restores_same_original_and_physical_join_keeps_unknown() {
        let owner=CheckedRunnerOwner::new(StartupId(2801),Instant::now()+Duration::from_secs(10),None,None,&SCOPED_CONNECT_OWNER,None);
        let (release,receive)=tokio::sync::oneshot::channel();
        let original=tokio::spawn(async move{receive.await.unwrap();});let id=original.id();
        owner.retain_original_forwarders(vec![original]);
        let mut future=Box::pin(owner.reap_native_forwarders_async(Instant::now()+Duration::from_secs(10)));
        std::future::poll_fn(|cx|{assert!(future.as_mut().poll(cx).is_pending());Poll::Ready(())}).await;
        assert!(owner.native_forwarders.lock().unwrap().loaned);drop(future);
        {let retained=owner.native_forwarders.lock().unwrap();assert!(!retained.loaned);
            assert_eq!(retained.originals[0].as_ref().unwrap().id(),id);assert!(retained.outcomes[0].is_none());}
        release.send(()).unwrap();owner.reap_native_forwarders_async(Instant::now()+Duration::from_secs(10)).await.unwrap();
        let retained=owner.native_forwarders.lock().unwrap();assert!(retained.originals.iter().all(Option::is_none));
        assert_eq!(retained.outcomes[0],Some(ForwarderJoin::PhysicallyJoined));drop(retained);
        assert!(owner.is_sticky_unknown());assert!(owner.is_no_replay_sealed());
    }
}
