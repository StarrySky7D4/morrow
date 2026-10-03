//! Real worker owns SQLite/Wasmi/registry. Native watch driver never takes these
//! resources' locks or calls SQL. A bounded hold is explicitly a fixture barrier.
use crate::{Result, state::State, transport};
use morrow_plugin_runtime::channel::{ChannelCleanup, CleanupProof};
use morrow_workbench_host::product_gate::ProductGate;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub enum Job {
    Start,
    Call { tag: u8, bytes: Vec<u8> },
    Stop,
}
pub enum Event {
    Ready {
        directory: Vec<u8>,
        cleanup: ChannelCleanup,
    },
    Reply(Vec<u8>),
    Held,
    Proof(String),
    Failure(String),
    SourceFailure(String),
    SourceTerminal(String),
    ChannelClosed(morrow_plugin_runtime::channel::Snapshot),
}
fn report_source_wait_error(
    sender: &mpsc::SyncSender<Event>,
    phase: &str,
    error: morrow_plugin_runtime::channel::Error,
) {
    let text = format!(
        "Producer.wait_sent phase={phase};actual-error={error:?};producer-outcome=Unknown;Accepted-does-not-prove-observation=true"
    );
    match error {
        morrow_plugin_runtime::channel::Error::Closed
        | morrow_plugin_runtime::channel::Error::Denied
        | morrow_plugin_runtime::channel::Error::Expired => {
            // These actual local terminal results do not retire the native session.
            // Close qualification still needs the exact broker's Joined snapshot.
            eprintln!("ACTUAL-PRODUCER-TERMINAL: {text}");
            let _ = sender.try_send(Event::SourceTerminal(text));
        }
        _ => {
            let _ = sender.try_send(Event::SourceFailure(text));
        }
    }
}
pub struct Worker {
    pub sender: mpsc::SyncSender<Job>,
    pub events: mpsc::Receiver<Event>,
    pub released: Arc<AtomicBool>,
    handle: Option<JoinHandle<Result<()>>>,
}
impl Worker {
    pub fn start(
        repo: PathBuf,
        root: PathBuf,
        language: String,
        gate: ProductGate,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(2);
        let (etx, erx) = mpsc::sync_channel(4);
        let released = Arc::new(AtomicBool::new(false));
        let release = released.clone();
        let handle=thread::Builder::new().name("sdk-bridge-original-worker".into()).spawn(move||{
   let mut state=State::new(&repo,&root,&language,gate)?;
   let directory=state.broker.directory().encode()?;let cleanup=state.broker.cleanup_handle();
   etx.send(Event::Ready{directory,cleanup}).map_err(|_|"native driver gone")?;
   let mut started=false;let mut closed_snapshot:Option<morrow_plugin_runtime::channel::Snapshot>=None;
   while let Ok(job)=rx.recv(){match job{
    Job::Start=>{
     if started||state.gate.closed(){return Err("duplicate/closed worker admission".into());}started=true;
     let source_errors=etx.clone();
     state.broker.spawn(move|producer|{
      if let Err(error)=producer.push(b"actual broker producer bytes".to_vec(),vec![]) {
       let _=source_errors.try_send(Event::SourceFailure(format!("Producer.push primary error: {error:?}")));
       return;
      }
      match producer.wait_sent(){
       Ok((sequence,bytes)) if sequence==1&&bytes==b"actual SDK send"=>{
        match producer.wait_sent(){
         Err(error)=>report_source_wait_error(&source_errors,"after-correlated-send",error),
         Ok(_)=>{let _=source_errors.try_send(Event::SourceFailure("unexpected second source data; primary correlation failure".into()));}
        }
       }
       Ok(_)=>{let _=source_errors.try_send(Event::SourceFailure("source send correlation mismatch".into()));}
       Err(error)=>report_source_wait_error(&source_errors,"before-correlated-send",error),
      }
     })?;
    }
    Job::Call{tag,bytes}=>{
     if !started{return Err("call before actual decoded heartbeat admission".into());}
     if state.gate.closed(){etx.send(Event::Failure("original control closed; Unknown; no replay".into())).map_err(|_|"driver gone")?;continue;}
     let result=match tag{
      transport::CHANNEL=>(|| -> Result<Vec<u8>> {
       let now=state.tick();let output=state.broker.exchange(&state.manager,&mut state.host,&state.channel,&bytes,now)?;
       let request=morrow_core::channel::Request::decode(&bytes)?;let response=morrow_core::channel::Response::decode(&output)?;
       let snapshot=state.broker.snapshot();
       if let Some(original)=closed_snapshot {
        if snapshot.last_acked!=original.last_acked||snapshot.accepted_sequence!=original.accepted_sequence||snapshot.usage.messages!=original.usage.messages||snapshot.usage.bytes!=original.usage.bytes||snapshot.terminal_cause!=Some(morrow_core::channel::Status::Closed)||!snapshot.resource_reclaimed||snapshot.cleanup_proof!=CleanupProof::Joined { return Err("closed original channel advanced or lost real join".into()); }
       }
       if response.resource_reclaimed&&matches!(request.action,morrow_core::channel::Action::Close|morrow_core::channel::Action::Query)&&closed_snapshot.is_none(){
        if response.status!=morrow_core::channel::Status::Closed||snapshot.terminal_cause!=Some(morrow_core::channel::Status::Closed)||!snapshot.resource_reclaimed||snapshot.cleanup_proof!=CleanupProof::Joined||snapshot.producer_outcome!=morrow_plugin_runtime::channel::ProducerOutcome::Unknown||state.gate.closed(){return Err("SDK Close lacks exact original live-gate Joined/Unknown witness".into());}
        closed_snapshot=Some(snapshot);etx.send(Event::ChannelClosed(snapshot)).map_err(|_|"driver gone before actual channel-close witness")?;
       }
       Ok(output)
      })(),
      transport::RUNTIME=>state.runtime_call(&bytes),
      transport::TRANSFORM=>state.transform_call(&bytes),
      transport::PAUSE=>{
       etx.send(Event::Held).map_err(|_|"driver gone")?;
       let began=Instant::now();while !release.load(Ordering::Acquire)&&began.elapsed()<Duration::from_secs(3){thread::sleep(Duration::from_millis(1));}
       if !release.load(Ordering::Acquire){return Err("bounded actual-worker hold deadline".into());}
       Err("held call cancelled; business outcome Unknown; no replay".into())
      },
      _=>Err("unknown standalone fixture operation".into()),
     };
     let event=match result{Ok(bytes)=>Event::Reply(bytes),Err(e)=>Event::Failure(e.to_string())};
     etx.send(event).map_err(|_|"driver gone")?;
    }
    Job::Stop=>{
     let proof=state.stop_proof()?;let cleanup=state.broker.cleanup_handle();let began=Instant::now();
     while !cleanup.resource_reclaimed()&&began.elapsed()<Duration::from_secs(2){cleanup.try_reap();thread::sleep(Duration::from_millis(1));}
     if !cleanup.resource_reclaimed()||cleanup.cleanup_proof()!=CleanupProof::Joined{return Err("producer join still unconfirmed".into());}
     let snapshot=state.broker.snapshot();
     etx.send(Event::Proof(format!("{proof};actual-broker-join=Joined;broker-resource-reclaimed=true;ACK-is-not-OS-cleanup=true;producer-outcome={:?}",snapshot.producer_outcome))).map_err(|_|"driver gone")?;
     return Ok(());
    }
   }}
   Err("worker command stream ended without explicit bounded cleanup".into())
  })?;
        Ok(Self {
            sender: tx,
            events: erx,
            released,
            handle: Some(handle),
        })
    }
    pub fn stop_and_join_bounded(&mut self) -> Result<Option<String>> {
        if self.handle.is_none() {
            return Ok(None);
        }
        self.released.store(true, Ordering::Release);
        let began = Instant::now();
        let mut sent = false;
        let mut proof = None;
        loop {
            if !sent {
                match self.sender.try_send(Job::Stop) {
                    Ok(()) => sent = true,
                    Err(mpsc::TrySendError::Full(_)) => (),
                    Err(mpsc::TrySendError::Disconnected(_)) => sent = true,
                }
            }
            // Draining is required: a real worker must not block on its bounded
            // receipt queue while the error path waits for a join.
            for _ in 0..4 {
                match self.events.try_recv() {
                    Ok(Event::Proof(value)) => proof = Some(value),
                    Ok(Event::SourceFailure(error)) => eprintln!("ACTUAL-PRODUCER-ERROR: {error}"),
                    Ok(Event::Failure(error)) => eprintln!("WORKER-ERROR-CLEANUP: {error}"),
                    Ok(Event::SourceTerminal(error)) => {
                        eprintln!("ACTUAL-PRODUCER-TERMINAL-RECEIPT: {error}")
                    }
                    Ok(_) => (),
                    Err(_) => break,
                }
            }
            if self.finished() {
                // A worker can finish between the preceding Empty and this check.
                // Its bounded queue is now stable; retain its last real receipt.
                for _ in 0..4 {
                    match self.events.try_recv() {
                        Ok(Event::Proof(value)) => proof = Some(value),
                        Ok(Event::SourceFailure(error)) => {
                            eprintln!("ACTUAL-PRODUCER-ERROR: {error}")
                        }
                        Ok(Event::Failure(error)) => eprintln!("WORKER-ERROR-CLEANUP: {error}"),
                        Ok(Event::SourceTerminal(error)) => {
                            eprintln!("ACTUAL-PRODUCER-TERMINAL-RECEIPT: {error}")
                        }
                        Ok(_) => (),
                        Err(_) => break,
                    }
                }
                self.join_finished()?;
                return Ok(proof);
            }
            if began.elapsed() >= Duration::from_secs(2) {
                return Err("bounded error-path worker join unconfirmed".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn finished(&self) -> bool {
        self.handle.as_ref().is_none_or(|h| h.is_finished())
    }
    pub fn join_finished(&mut self) -> Result<()> {
        let handle = self.handle.take().ok_or("already joined original worker")?;
        if !handle.is_finished() {
            self.handle = Some(handle);
            return Err("actual worker still pending".into());
        }
        handle
            .join()
            .map_err(|_| "actual original worker panicked")??;
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.released.store(true, Ordering::Release);
        let _ = self.sender.try_send(Job::Stop);
        let began = Instant::now();
        while !self.finished() && began.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(1));
        }
        if self.finished() {
            let _ = self.join_finished();
        }
        // A non-finished real worker is unconfirmed, never represented as joined.
    }
}
