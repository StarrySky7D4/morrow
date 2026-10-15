//! Actual Tokio-handle transport tests with an explicitly simulated driver.
//! The unit payload cannot become the concrete sealed native-owner capability;
//! these tests make no Windows process, native-close, EOF or cleanup claim.
use super::*;
use std::{future::Future, pin::Pin, sync::atomic::{AtomicUsize,Ordering}, task::Poll, time::{Duration,Instant}};

#[derive(Default)]
struct SimulatedDriver {
    failures:AtomicUsize,
    originals:StdMutex<Vec<JoinHandle<()>>>,
}
impl NativeCloseDriver for SimulatedDriver {
    fn native_phase(&self,_:Instant)->Pin<Box<dyn Future<Output=io::Result<i32>>+Send+'_>> {
        Box::pin(async {Err(io::Error::from(io::ErrorKind::Unsupported))})
    }
    fn take_native_completion(&self,_:Instant)->Pin<Box<dyn Future<Output=io::Result<Box<dyn std::any::Any+Send>>>+Send+'_>> {
        Box::pin(async {Ok(Box::new(()) as Box<dyn std::any::Any+Send>)})
    }
    fn retain_outer_failure(&self){self.failures.fetch_add(1,Ordering::SeqCst);}
    fn retain_original_forwarders(&self,originals:Vec<JoinHandle<()>>){
        let mut retained=self.originals.lock().unwrap();assert!(retained.is_empty());*retained=originals;
    }
}
struct NoAutomaticControl;
impl ChildTerminator for NoAutomaticControl {
    fn signal(&mut self,_:ProcessSignal)->io::Result<()>{panic!("simulated original must not receive signal")}
    fn kill(&mut self)->io::Result<()>{panic!("native transport Drop must not invoke control")}
}
fn simulated_session()->(ProcessHandle,Arc<SimulatedDriver>,oneshot::Sender<()>) {
    let (release,receive)=oneshot::channel();
    let reader=tokio::spawn(async move {receive.await.unwrap();});
    let writer=tokio::spawn(async {});
    let wait=tokio::spawn(async {});
    let (writer_tx,_)=mpsc::channel(1);
    let mut session=ProcessHandle::new(writer_tx,Box::new(NoAutomaticControl),reader,Vec::new(),writer,wait,
        Arc::new(AtomicBool::new(false)),Arc::new(StdMutex::new(None)),None,None);
    let driver=Arc::new(SimulatedDriver::default());
    session.native_driver=Some(driver.clone());
    session.native_forward_state.get_mut().unwrap().configured=[true,false,true,true];
    (session,driver,release)
}
async fn reap_simulated_originals(driver:&SimulatedDriver) {
    let originals=std::mem::take(&mut *driver.originals.lock().unwrap());
    assert_eq!(originals.len(),3);
    for original in originals {original.await.unwrap();}
}
#[tokio::test]
async fn ordinary_driver_does_not_gain_native_completion_from_its_join_slots() {
    let (mut session,driver,release)=simulated_session();
    let native=session.native_driver.take();
    let result=session.take_native_completion(Instant::now()+Duration::from_secs(10)).await;
    // Restore only the explicitly simulated retention owner for test teardown;
    // the attempted ordinary call above had no driver and performed no join.
    session.native_driver=native;
    assert_eq!(match result {Err(error)=>error.kind(),Ok(_)=>panic!("ordinary driver minted native transport")},io::ErrorKind::Unsupported);
    assert!(session.reader_handle.lock().unwrap().is_some());
    assert!(!session.native_forward_state.lock().unwrap().active);
    release.send(()).unwrap();drop(session);reap_simulated_originals(&driver).await;
}
#[tokio::test]
async fn native_completion_cancellation_restores_exact_original_and_seals_no_replay() {
    let (session,driver,release)=simulated_session();
    let id=session.reader_handle.lock().unwrap().as_ref().unwrap().id();
    let mut future=Box::pin(session.take_native_completion(Instant::now()+Duration::from_secs(10)));
    std::future::poll_fn(|cx|{assert!(future.as_mut().poll(cx).is_pending());Poll::Ready(())}).await;
    assert!(session.reader_handle.lock().unwrap().is_none());
    drop(future);
    assert_eq!(session.reader_handle.lock().unwrap().as_ref().unwrap().id(),id);
    assert!(session.native_forward_state.lock().unwrap().failed);
    assert!(session.take_native_completion(Instant::now()+Duration::from_secs(10)).await.is_err());
    assert_eq!(driver.failures.load(Ordering::SeqCst),1);
    release.send(()).unwrap();drop(session);
    assert_eq!(driver.failures.load(Ordering::SeqCst),2);
    reap_simulated_originals(&driver).await;
}
#[tokio::test]
async fn native_transport_joins_originals_once_but_erased_unit_is_only_transport() {
    let (session,driver,release)=simulated_session();release.send(()).unwrap();
    let payload=session.take_native_completion(Instant::now()+Duration::from_secs(10)).await.unwrap();
    assert!(payload.is::<()>());
    assert!(session.reader_handle.lock().unwrap().is_none());
    assert!(session.writer_handle.lock().unwrap().is_none());
    assert!(session.wait_handle.lock().unwrap().is_none());
    let state=session.native_forward_state.lock().unwrap();
    assert!(state.completed);assert_eq!(state.joined,[true,false,true,true]);
    assert_eq!(state.outcomes,[Some(NativeForwardOutcome::JoinedTimely),None,
        Some(NativeForwardOutcome::JoinedTimely),Some(NativeForwardOutcome::JoinedTimely)]);drop(state);
    assert!(session.take_native_completion(Instant::now()+Duration::from_secs(10)).await.is_err());
    assert_eq!(driver.native_phase(Instant::now()).await.unwrap_err().kind(),io::ErrorKind::Unsupported);
    drop(session);assert_eq!(driver.failures.load(Ordering::SeqCst),0);
    assert!(driver.originals.lock().unwrap().is_empty());
}
#[tokio::test]
async fn poisoned_forwarding_state_keeps_originals_and_never_reports_completion() {
    let (session,driver,release)=simulated_session();
    let id=session.reader_handle.lock().unwrap().as_ref().unwrap().id();
    let poisoned=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
        let _held=session.native_forward_state.lock().unwrap();panic!("intentional model poison");
    }));assert!(poisoned.is_err());
    assert!(session.take_native_completion(Instant::now()+Duration::from_secs(10)).await.is_err());
    assert_eq!(session.reader_handle.lock().unwrap().as_ref().unwrap().id(),id);
    release.send(()).unwrap();drop(session);
    assert_eq!(driver.failures.load(Ordering::SeqCst),1);reap_simulated_originals(&driver).await;
}
#[tokio::test]
async fn actual_join_error_is_recorded_once_and_is_not_restored_as_an_unpolled_original() {
    let (session,driver,release)=simulated_session();
    let first=session.reader_handle.lock().unwrap().take().unwrap();
    release.send(()).unwrap();first.await.unwrap();
    let original=tokio::spawn(async {panic!("intentional simulated forwarding panic");});
    let id=original.id();*session.reader_handle.lock().unwrap()=Some(original);
    assert!(session.take_native_completion(Instant::now()+Duration::from_secs(10)).await.is_err());
    assert!(session.reader_handle.lock().unwrap().is_none());
    {let state=session.native_forward_state.lock().unwrap();assert!(state.failed);assert!(!state.completed);
        assert!(state.joined[0]);assert_eq!(state.outcomes[0],Some(NativeForwardOutcome::Panicked));}
    drop(session);
    let originals=std::mem::take(&mut *driver.originals.lock().unwrap());assert_eq!(originals.len(),2);
    for original in originals{assert_ne!(original.id(),id);original.await.unwrap();}
}
