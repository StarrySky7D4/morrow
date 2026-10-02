//! Windows native executor qualification using the production host helper.
//! Core Store::open is an unprotected temporary SQLite fixture. No Workbench,
//! audit Session, DPAPI provider, audit key, or pre-existing database is opened.
//! Maintenance errors/panics are injected HostOwner hooks, not seal qualification.
#![cfg(all(windows, feature = "packages", feature = "fault-injection"))]

#[path = "../../workbench_host/src/channel_executor.rs"]
mod executor;

use morrow_core::{
    channel::{Action, Budget, Kind, Request, Status},
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto::TransformHandler, registry::Registry},
    store::{EventBudget, Store},
    task::{FailureCode, Invocation, PluginFailure, Transform},
};
use morrow_plugin_runtime::{
    Fault, Limits,
    channel::{ChannelBroker, CleanupProof, Error, ProducerOutcome, Source},
    io_jobs::{HostOwner, JobError, ManagedHostOwner},
    manager::{ManagedInstance, Manager},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const ID: &str = "org.example.windows.channel.executor";
const WAIT: Duration = Duration::from_secs(5);
const OUTPUT: &[u8] = b"valid-output";

#[derive(Clone, Copy)]
enum Maintenance {
    Ok,
    Error,
    Panic,
}
#[derive(Clone, Copy)]
enum Guest {
    Output,
    Trap,
    Failure,
}

struct TestOwner {
    host: HostRuntime,
    manager: Manager,
    maintenance: Maintenance,
    finish_calls: Arc<AtomicUsize>,
    maintenance_entry: Arc<Mutex<Vec<Status>>>,
    maintenance_broker: Arc<ChannelBroker>,
    original_instance: Arc<ManagedInstance>,
    identity: Arc<()>,
    _directory: tempfile::TempDir,
}
impl HostOwner for TestOwner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        let endpoint = self.maintenance_broker.endpoint();
        let status = self
            .maintenance_broker
            .dispatch(
                &self.manager,
                &mut self.host,
                &self.original_instance,
                &Request {
                    call_id: [41; 32],
                    reference: endpoint.reference,
                    source_epoch: endpoint.source_epoch,
                    action: Action::Query,
                },
                2,
            )
            .unwrap()
            .status;
        self.maintenance_entry.lock().unwrap().push(status);
        self.finish_calls.fetch_add(1, Ordering::SeqCst);
        match self.maintenance {
            Maintenance::Ok => Ok(()),
            Maintenance::Error => Err(JobError::Unavailable),
            Maintenance::Panic => panic!("injected original owner maintenance panic"),
        }
    }
}
impl ManagedHostOwner for TestOwner {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.manager)
    }
    fn with_managed_runtime<T>(
        &mut self,
        action: impl FnOnce(&Manager, &mut HostRuntime) -> T,
    ) -> Option<T> {
        Some(action(&self.manager, &mut self.host))
    }
}

fn invocation() -> Invocation {
    Invocation::new_transform(
        "windows-executor-original",
        Transform {
            handler: "executor.test".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: vec![0; 65],
        },
    )
    .unwrap()
}

struct Fixture {
    owner: Option<TestOwner>,
    instance: Arc<ManagedInstance>,
    broker: Arc<ChannelBroker>,
    sibling: Arc<ChannelBroker>,
    foreign: ManagedInstance,
    foreign_broker: ChannelBroker,
    identity: Arc<()>,
    finish_calls: Arc<AtomicUsize>,
    maintenance_entry: Arc<Mutex<Vec<Status>>>,
}
impl Fixture {
    fn new(maintenance: Maintenance, guest: Guest) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("morrow-executor-unprotected-")
            .tempdir()
            .unwrap();
        let completion = match guest {
            Guest::Failure => invocation()
                .failure_completion(&PluginFailure {
                    code: FailureCode::Failed,
                    message: "owned plugin failure".into(),
                })
                .unwrap(),
            _ => invocation().output_completion(OUTPUT).unwrap(),
        };
        let encoded: String = completion
            .iter()
            .map(|byte| format!("\\{byte:02x}"))
            .collect();
        let body = if matches!(guest, Guest::Trap) {
            "unreachable".to_owned()
        } else {
            format!(
                "i32.const 131072 i32.const {} call $complete drop i32.const 0",
                completion.len()
            )
        };
        let wasm = wat::parse_str(format!(
            r#"(module
          (import "morrow_channel_v1" "call" (func $channel (param i32 i32 i32 i32) (result i32)))
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
          (memory (export "memory") 4)
          (data (i32.const 131072) "{encoded}")
          (func (export "morrow_run") (result i32)
            i32.const 0 i32.const 131072 call $read drop
            {body}))"#
        ))
        .unwrap();
        let mut manifest = Package::manifest_for_transform(
            ID,
            "1.0.0",
            &wasm,
            vec![TransformHandler {
                handler: "executor.test".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 65,
                max_output_bytes: 64,
            }],
        );
        manifest
            .required_features
            .push(morrow_core::channel::FEATURE.into());
        manifest.channel_declaration = Some(morrow_core::channel::declaration(
            vec!["executor.test".into()],
            vec![Kind::ByteStream],
        ));
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&directory.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&directory.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        // This constructor calls Core SQLite directly, with no audit trust/provider.
        let mut host = HostRuntime::new(
            Store::open(
                &directory.path().join("unprotected.sqlite3"),
                EventBudget::default(),
            )
            .unwrap(),
        )
        .unwrap();
        let instance = Arc::new(manager.connect(ID, &mut host).unwrap());
        let foreign = manager.connect(ID, &mut host).unwrap();
        let bind = |instance: &ManagedInstance| {
            manager
                .bind_channel(
                    &host,
                    instance,
                    package.digest(),
                    manager.revision(),
                    Source {
                        kind: Kind::ByteStream,
                        duplex: true,
                        checkpoint_scope: None,
                    },
                    Budget {
                        max_channels: 2,
                        max_frame_bytes: 32768,
                        max_bytes: 1048576,
                        max_messages: 128,
                        max_requests: 4096,
                        max_duration_ms: 10_000,
                    },
                    10_001,
                    1,
                )
                .unwrap()
        };
        let broker = Arc::new(bind(&instance));
        let sibling = Arc::new(bind(&instance));
        let foreign_broker = bind(&foreign);
        let identity = Arc::new(());
        let finish_calls = Arc::new(AtomicUsize::new(0));
        let maintenance_entry = Arc::new(Mutex::new(Vec::new()));
        let owner = TestOwner {
            host,
            manager,
            maintenance,
            identity: identity.clone(),
            finish_calls: finish_calls.clone(),
            _directory: directory,
            maintenance_entry: maintenance_entry.clone(),
            maintenance_broker: sibling.clone(),
            original_instance: instance.clone(),
        };
        Self {
            owner: Some(owner),
            instance,
            broker,
            sibling,
            foreign,
            foreign_broker,
            identity,
            finish_calls,
            maintenance_entry,
        }
    }
    fn waiting_source(&self) -> mpsc::Receiver<Result<(u64, Vec<u8>), Error>> {
        let (started, ready) = mpsc::channel();
        let (finished, result) = mpsc::channel();
        self.broker
            .spawn(move |producer| {
                started.send(()).unwrap();
                let outcome = producer.wait_sent();
                finished.send(outcome).unwrap();
                drop(producer);
            })
            .unwrap();
        ready.recv_timeout(WAIT).unwrap();
        result
    }
    fn start(
        &mut self,
        bound: u32,
        clock: impl FnMut() -> u64 + Send + 'static,
    ) -> JoinHandle<executor::Exit<TestOwner>> {
        executor::spawn(
            self.owner.take().unwrap(),
            self.broker.clone(),
            self.instance.clone(),
            invocation(),
            clock,
            bound,
        )
        .unwrap_or_else(|failure| panic!("unexpected native spawn failure: {}", failure.error))
    }
    fn assert_restored(&self, owner: &mut TestOwner, maintenance_calls: usize) {
        assert!(Arc::ptr_eq(&owner.identity, &self.identity));
        assert_eq!(self.finish_calls.load(Ordering::SeqCst), maintenance_calls);
        // An authentic dispatch using the returned manager/host proves that the
        // owner was preserved. This foreign Control must remain independently live.
        assert_eq!(
            query(&self.foreign_broker, owner, &self.foreign, 31),
            Status::Ready
        );
        owner.host.store_local().integrity_check().unwrap();
    }
    fn assert_maintenance_entry(&self, expected: Status) {
        assert_eq!(*self.maintenance_entry.lock().unwrap(), vec![expected]);
    }
    fn assert_stopped(
        &self,
        owner: &mut TestOwner,
        stopped: mpsc::Receiver<Result<(u64, Vec<u8>), Error>>,
    ) {
        let outcome = stopped.recv_timeout(WAIT);
        if outcome.is_err() {
            // Even a red candidate explicitly stops its original source before failing.
            self.instance.request_stop();
        }
        reap(&self.broker);
        assert_eq!(outcome, Ok(Err(Error::Denied)));
        // A sibling broker can only be revoked by the actual shared Control;
        // closing a single resource or toggling a test callback cannot satisfy this.
        assert_eq!(
            query(&self.sibling, owner, &self.instance, 32),
            Status::Revoked
        );
        assert_eq!(self.sibling.cleanup_handle().try_cleanup(), Status::Revoked);
        assert_eq!(
            self.sibling.try_snapshot().unwrap().cleanup_proof,
            CleanupProof::NoProducer
        );
        let snapshot = self.broker.try_snapshot().unwrap();
        assert_eq!(snapshot.terminal_cause, Some(Status::Revoked));
        assert_eq!(snapshot.producer_outcome, ProducerOutcome::Unknown);
        assert_eq!(snapshot.cleanup_proof, CleanupProof::Joined);
        assert!(snapshot.resource_reclaimed);
        assert_eq!(snapshot.last_acked, 0);
        assert_eq!(snapshot.accepted_sequence, 0);
    }
}
fn query(
    broker: &ChannelBroker,
    owner: &mut TestOwner,
    instance: &ManagedInstance,
    serial: u8,
) -> Status {
    let endpoint = broker.endpoint();
    broker
        .dispatch(
            &owner.manager,
            &mut owner.host,
            instance,
            &Request {
                call_id: [serial; 32],
                reference: endpoint.reference,
                source_epoch: endpoint.source_epoch,
                action: Action::Query,
            },
            2,
        )
        .unwrap()
        .status
}
fn join(worker: JoinHandle<executor::Exit<TestOwner>>) -> executor::Exit<TestOwner> {
    let deadline = Instant::now() + WAIT;
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "original native executor did not finish"
        );
        thread::sleep(Duration::from_millis(1));
    }
    worker
        .join()
        .unwrap_or_else(|_| panic!("helper lost its original owner through an uncaught panic"))
}
fn reap(broker: &ChannelBroker) {
    let deadline = Instant::now() + WAIT;
    loop {
        let _ = broker.cleanup_handle().try_reap();
        if broker
            .try_snapshot()
            .is_some_and(|snapshot| snapshot.resource_reclaimed)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual native source was not joined"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        broker.try_snapshot().unwrap().cleanup_proof,
        CleanupProof::Joined
    );
}

#[test]
fn private_output_bound_revokes_actual_control_and_preserves_first_error_after_maintenance_failure()
{
    for maintenance in [Maintenance::Ok, Maintenance::Error] {
        let mut fixture = Fixture::new(maintenance, Guest::Output);
        let stopped = fixture.waiting_source();
        let mut exit = join(fixture.start(4, || 2));
        assert_eq!(
            exit.error,
            "channel output exceeds the private result bound"
        );
        assert!(exit.report.is_none());
        assert_eq!(exit.repair, matches!(maintenance, Maintenance::Error));
        assert!(exit.requires_cleanup());
        fixture.assert_maintenance_entry(Status::Revoked);
        fixture.assert_restored(&mut exit.owner, 1);
        fixture.assert_stopped(&mut exit.owner, stopped);
    }
}

#[test]
fn maintenance_error_stops_original_control_without_rewriting_validated_task_report() {
    let mut fixture = Fixture::new(Maintenance::Error, Guest::Output);
    let stopped = fixture.waiting_source();
    let mut exit = join(fixture.start(64, || 2));
    let report = exit.report.as_ref().unwrap();
    assert_eq!(report.execution.outcome, Ok(0));
    assert_eq!(report.output.as_ref().unwrap().bytes, OUTPUT);
    assert!(report.failure.is_none());
    assert_eq!(
        exit.error,
        "original storage maintenance failed; outcome Unknown"
    );
    assert!(exit.repair);
    fixture.assert_maintenance_entry(Status::Ready);
    fixture.assert_restored(&mut exit.owner, 1);
    fixture.assert_stopped(&mut exit.owner, stopped);
}

#[test]
fn guest_trap_first_cause_survives_later_maintenance_error_or_panic_and_real_executor_join() {
    for maintenance in [Maintenance::Error, Maintenance::Panic] {
        let mut fixture = Fixture::new(maintenance, Guest::Trap);
        let stopped = fixture.waiting_source();
        let mut exit = join(fixture.start(64, || 2));
        let report = exit.report.as_ref().unwrap();
        assert_eq!(report.execution.outcome, Err(Fault::Trap));
        assert!(report.output.is_none());
        assert!(report.failure.is_none());
        assert!(
            exit.error.is_empty(),
            "the retained failed TaskReport remains primary"
        );
        assert!(exit.repair);
        fixture.assert_maintenance_entry(Status::Revoked);
        fixture.assert_restored(&mut exit.owner, 1);
        fixture.assert_stopped(&mut exit.owner, stopped);
        assert_eq!(
            exit.report.as_ref().unwrap().execution.outcome,
            Err(Fault::Trap)
        );
    }
}

#[test]
fn correlated_guest_failure_is_primary_after_maintenance_error() {
    let mut fixture = Fixture::new(Maintenance::Error, Guest::Failure);
    let stopped = fixture.waiting_source();
    let mut exit = join(fixture.start(64, || 2));
    let report = exit.report.as_ref().unwrap();
    assert_eq!(report.execution.outcome, Ok(0));
    assert_eq!(report.failure.as_ref().unwrap().code, FailureCode::Failed);
    assert_eq!(
        report.failure.as_ref().unwrap().message,
        "owned plugin failure"
    );
    assert!(report.output.is_none());
    assert!(exit.error.is_empty());
    assert!(exit.repair);
    fixture.assert_maintenance_entry(Status::Revoked);
    fixture.assert_restored(&mut exit.owner, 1);
    fixture.assert_stopped(&mut exit.owner, stopped);
}

#[test]
fn actual_executor_panic_remains_primary_after_maintenance_error_or_panic() {
    for maintenance in [Maintenance::Error, Maintenance::Panic] {
        let mut fixture = Fixture::new(maintenance, Guest::Output);
        let stopped = fixture.waiting_source();
        let mut exit = join(fixture.start(64, || panic!("actual original executor clock panic")));
        assert_eq!(
            exit.error,
            "channel executor panicked; business outcome Unknown"
        );
        assert!(exit.report.is_none());
        assert!(exit.repair);
        fixture.assert_maintenance_entry(Status::Revoked);
        fixture.assert_restored(&mut exit.owner, 1);
        fixture.assert_stopped(&mut exit.owner, stopped);
    }
}

#[test]
fn maintenance_panic_retains_original_owner_and_successful_execution_report_but_blocks_delivery() {
    let mut fixture = Fixture::new(Maintenance::Panic, Guest::Output);
    let stopped = fixture.waiting_source();
    let mut exit = join(fixture.start(64, || 2));
    assert_eq!(
        exit.error,
        "original storage maintenance panicked; outcome Unknown"
    );
    assert!(exit.repair);
    fixture.assert_maintenance_entry(Status::Ready);
    assert_eq!(exit.report.as_ref().unwrap().execution.outcome, Ok(0));
    assert_eq!(
        exit.report.as_ref().unwrap().output.as_ref().unwrap().bytes,
        OUTPUT
    );
    fixture.assert_restored(&mut exit.owner, 1);
    fixture.assert_stopped(&mut exit.owner, stopped);
}

#[test]
fn successful_executor_keeps_actual_controls_live_until_explicit_stop() {
    let mut fixture = Fixture::new(Maintenance::Ok, Guest::Output);
    let stopped = fixture.waiting_source();
    let mut exit = join(fixture.start(64, || 2));
    assert!(exit.error.is_empty());
    assert!(!exit.repair);
    assert!(!exit.requires_cleanup());
    fixture.assert_maintenance_entry(Status::Ready);
    assert_eq!(
        exit.report.as_ref().unwrap().output.as_ref().unwrap().bytes,
        OUTPUT
    );
    fixture.assert_restored(&mut exit.owner, 1);
    assert_eq!(
        query(&fixture.sibling, &mut exit.owner, &fixture.instance, 33),
        Status::Ready
    );
    assert!(matches!(stopped.try_recv(), Err(mpsc::TryRecvError::Empty)));
    fixture.instance.request_stop();
    fixture.assert_stopped(&mut exit.owner, stopped);
}

#[test]
fn actual_windows_executor_spawn_failure_returns_original_owner_without_running_hooks() {
    let mut fixture = Fixture::new(Maintenance::Panic, Guest::Output);
    let stopped = fixture.waiting_source();
    let clock_calls = Arc::new(AtomicUsize::new(0));
    let observed = clock_calls.clone();
    // Real Windows native stack reservation failure, not a mocked spawn result.
    let failure = executor::spawn_with_stack_size(
        fixture.owner.take().unwrap(),
        fixture.broker.clone(),
        fixture.instance.clone(),
        invocation(),
        move || {
            observed.fetch_add(1, Ordering::SeqCst);
            2
        },
        64,
        usize::MAX,
    )
    .err()
    .expect("impossible native stack reservation must fail without creating a worker");
    assert!(failure.error.raw_os_error().is_some());
    assert_eq!(clock_calls.load(Ordering::SeqCst), 0);
    let mut owner = failure.owner;
    // Probe before any host restoration or foreign-control check. The failed
    // spawn must already have revoked its actual original shared Control.
    assert_eq!(
        query(&fixture.sibling, &mut owner, &fixture.instance, 35),
        Status::Revoked
    );
    assert!(fixture.maintenance_entry.lock().unwrap().is_empty());
    fixture.assert_restored(&mut owner, 0);
    fixture.assert_stopped(&mut owner, stopped);
}

#[test]
fn actual_producer_panic_first_cause_is_not_replaced_by_later_executor_maintenance_stop() {
    let mut fixture = Fixture::new(Maintenance::Error, Guest::Output);
    fixture
        .broker
        .spawn(|_producer| panic!("actual original producer panic"))
        .unwrap();
    reap(&fixture.broker);
    let original = fixture.broker.try_snapshot().unwrap();
    assert_eq!(original.terminal_cause, Some(Status::Unknown));
    assert_eq!(original.producer_outcome, ProducerOutcome::Unknown);
    let mut exit = join(fixture.start(64, || 2));
    assert_eq!(exit.report.as_ref().unwrap().execution.outcome, Ok(0));
    assert!(exit.repair);
    fixture.assert_restored(&mut exit.owner, 1);
    assert_eq!(
        query(&fixture.sibling, &mut exit.owner, &fixture.instance, 34),
        Status::Revoked
    );
    let _ = fixture.broker.cleanup_handle().try_cleanup();
    let after = fixture.broker.try_snapshot().unwrap();
    assert_eq!(after.terminal_cause, original.terminal_cause);
    assert_eq!(after.producer_outcome, ProducerOutcome::Unknown);
    assert_eq!(after.cleanup_proof, CleanupProof::Joined);
    assert_eq!(after.last_acked, 0);
    assert_eq!(after.accepted_sequence, 0);
}
