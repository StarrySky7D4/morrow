//! Real StateSlot and AgentWorker stop/reclaim routes with a synthetic file Core.
//! No protected WorkbenchState is constructed. The join thread deliberately
//! panics only after release; this fixture cannot prove healthy owner return.
use super::*;
use crate::agent_tasks::{AgentError, AgentExitStatus, AgentPhase, AgentWorker};
use morrow_core::store::{EventBudget, Store};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}, mpsc};
use std::time::{Duration, Instant};

struct Fixture {
    app: Workbench,
    _runtime: HostRuntime,
    _fixture_root: std::path::PathBuf,
    release: Option<mpsc::Sender<()>>,
    stop_calls: Arc<AtomicUsize>,
}

fn key(n: u8) -> TaskKey { TaskKey::from_bytes(&[n; 32]).unwrap() }

impl Fixture {
    fn new(agent: bool) -> Self {
        // ROOT supplies an owned TEMP scratch. CreateNew directory admission
        // refuses an existing name; retain the synthetic database as evidence.
        // No Storage, Session, protected WorkbenchState or DPAPI provider is used.
        static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);
        let fixture_root = std::env::temp_dir().join(format!(
            "stage345-stop-state-{}-{}",
            std::process::id(), NEXT_FIXTURE.fetch_add(1, Ordering::SeqCst),
        ));
        std::fs::create_dir(&fixture_root).unwrap();
        eprintln!("stage345 synthetic fixture retained: {}", fixture_root.display());
        let mut runtime = HostRuntime::new(
            Store::open(&fixture_root.join("synthetic.sqlite3"), EventBudget::default()).unwrap(),
        ).unwrap();
        let (worker, release, stop_calls) = if agent {
            let (worker, release, calls) = AgentWorker::test_pending_join(&mut runtime);
            (Some(Executor::Agent(Box::new(worker))), Some(release), calls)
        } else {
            (None, None, Arc::new(AtomicUsize::new(0)))
        };
        let state = StateSlot {
            agent_preparation_debt: None,
            agent_contexts: crate::agent_tasks::context_lifecycle::ContextLifecycleLedger::new(runtime.binding()),
            product_gate: Default::default(),
            owner: None,
            task: Some(Task {
                key: key(1), commands: Default::default(), worker,
                handle: None, stopping: false, exit: None, service: None,
                file: None, directory: None, mutation: None,
            }),
            cleanup: None,
            agent_exit: None,
            repair_needed: false,
            native_retirement: None,
            lost: false,
            service_submissions: Default::default(),
            file_submissions: Default::default(),
            upload_submissions: Default::default(),
            upload_task: None,
            mutation_submissions: Default::default(),
            mutation_checkpoints: Default::default(),
            submission: None,
        };
        Self {
            app: Workbench {
            upload_plans: Default::default(), state, channel_tasks: Default::default(),
                http_tasks: Default::default(), product_session_wire: Default::default() },
            _runtime: runtime,
            _fixture_root: fixture_root, release, stop_calls,
        }
    }

    fn release_failed_join(&mut self) {
        self.release.take().unwrap().send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.app.state.try_reclaim() {
                Err(error) => {
                    assert_eq!(error.downcast_ref::<AccessError>(), Some(&AccessError::OwnerUnavailable));
                    break;
                }
                Ok(false) => {
                    assert!(Instant::now() < deadline, "controlled join did not complete");
                    std::thread::yield_now();
                }
                Ok(true) => panic!("panic-only worker cannot return an owner"),
            }
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // An assertion failure also releases the waiting thread. Its channel
        // has no native work, and no replacement owner is ever constructed.
        if let Some(release) = self.release.take() { let _ = release.send(()); }
    }
}

#[test]
fn cancel_pending_join_latches_stopping_and_keeps_ack_closed() {
    let mut f = Fixture::new(true);
    assert_eq!(f.app.state.snapshot().storage, StoragePhase::Running);
    let snapshot = f.app.cancel_agent(key(1)).unwrap();
    assert_eq!(snapshot.task.storage, StoragePhase::Stopping);
    assert_eq!(snapshot.progress.unwrap().phase, AgentPhase::Stopping);
    assert!(snapshot.exit.is_none() && snapshot.task.exit.is_none());
    assert_eq!(f.stop_calls.load(Ordering::SeqCst), 1);
    for _ in 0..3 {
        assert_eq!(f.app.poll_agent(key(1)).unwrap().task.storage, StoragePhase::Stopping);
    }
    assert!(f.app.acknowledge_agent(key(1)).is_err());
    assert!(f.app.state.task.as_ref().unwrap().worker.is_some());
    assert!(f.app.state.owner.is_none());
    f.release_failed_join();
}

#[test]
fn cancel_wrong_key_preserves_running_worker_without_stop() {
    let mut f = Fixture::new(true);
    let error = f.app.cancel_agent(key(2)).unwrap_err();
    assert_eq!(error.downcast_ref::<AccessError>(), Some(&AccessError::StaleTask));
    assert_eq!(f.stop_calls.load(Ordering::SeqCst), 0);
    assert!(!f.app.state.task.as_ref().unwrap().stopping);
    let snapshot = f.app.poll_agent(key(1)).unwrap();
    assert_eq!(snapshot.task.storage, StoragePhase::Running);
    assert_eq!(snapshot.progress.unwrap().phase, AgentPhase::Running);
    assert!(snapshot.exit.is_none());
    f.release_failed_join();
}

#[test]
fn cancel_non_agent_and_retained_exit_do_not_latch_stop() {
    let mut f = Fixture::new(false);
    assert!(f.app.state.check_agent_task(key(1)).is_err());
    assert!(f.app.cancel_agent(key(1)).is_err());
    assert!(!f.app.state.task.as_ref().unwrap().stopping);
    // The broader lookup accepts this synthetic retained exit. Cancellation
    // must still use the unchanged exact live AgentWorker guard.
    f.app.state.agent_exit = Some((key(1), AgentExitStatus {
        execution: Err(AgentError::Unavailable),
        disconnect: Err(AgentError::Disconnect),
        maintenance: Err(AgentError::Maintenance),
    }));
    assert!(f.app.state.check_agent_task(key(1)).is_ok());
    let before = f.app.state.snapshot();
    let error = f.app.cancel_agent(key(1)).unwrap_err();
    assert_eq!(error.downcast_ref::<AccessError>(), Some(&AccessError::StaleTask));
    assert_eq!(f.app.state.snapshot(), before);
    assert!(!f.app.state.task.as_ref().unwrap().stopping);
    assert_eq!(f.stop_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn cancelled_failed_join_is_unavailable_never_reclaimed_or_acknowledged() {
    let mut f = Fixture::new(true);
    assert_eq!(f.app.cancel_agent(key(1)).unwrap().task.storage, StoragePhase::Stopping);
    f.release_failed_join();
    let snapshot = f.app.state.snapshot();
    assert_eq!(snapshot.storage, StoragePhase::Unavailable);
    let exit = snapshot.exit.unwrap();
    assert!(exit.execution.is_err() && exit.disconnect.is_err() && exit.maintenance.is_err());
    assert!(f.app.state.lost && f.app.state.owner.is_none());
    assert!(f.app.state.agent_exit.is_none());
    assert!(f.app.state.task.as_ref().unwrap().worker.is_none());
    assert!(f.app.acknowledge_agent(key(1)).is_err());
    assert_eq!(f.app.state.snapshot().storage, StoragePhase::Unavailable);
}
