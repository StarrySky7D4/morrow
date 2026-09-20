//! Real managed Wasm calls with durable IO transitions and a synthetic backend.
//! No actual HTTP request, endpoint permission or credential use is established.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    io::{HttpOutcome, HttpSubmission, Request, Response, Status},
    io_evidence::Kind,
    io_intent::{Command, Phase, Recovery},
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Limits,
    io_binding::ServiceRunBudget,
    io_execution,
    io_jobs::{
        BrokerRouter, IoWorker, JobHandle, JobLimits, JobReport, Poll, RouteContext, RouterFault,
    },
    manager::Manager,
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
const ID: &str = "org.example.brokered.jobs";
const WAIT: Duration = Duration::from_secs(10);
const RESPONSE_LIMIT: u64 = 1024;
fn request(operation: &str) -> Request {
    Request::encode_http_submit(
        1,
        &HttpSubmission {
            operation_id: operation.as_bytes().to_vec(),
            deadline_ms: 50,
            endpoint: b"trusted-test-endpoint".to_vec(),
            method: "POST".into(),
            relative_target: "/objects".into(),
            headers: vec![],
            body: b"test-input".to_vec(),
            credential: vec![],
        },
    )
    .unwrap()
}
fn response(request: &Request) -> Vec<u8> {
    Response::encode_http(
        request,
        &HttpOutcome {
            status: Status::Completed,
            http_status: 201,
            headers: vec![],
            body: b"synthetic-result".to_vec(),
        },
    )
    .unwrap()
}
struct Running {
    _dir: tempfile::TempDir,
    _manager: Manager,
    worker: IoWorker,
    digest: [u8; 32],
}
impl Running {
    fn new(total: u64, approved_bytes: u64) -> Self {
        let per_job = total;
        let dir = tempfile::tempdir().unwrap();
        // Pass the actual task bytes to the IO import and return its real response.
        let wasm = wat::parse_str(
            r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
          (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 4)
          (func (export "morrow_run") (result i32) (local $n i32)
            i32.const 0 i32.const 131072 call $read local.set $n
            i32.const 131072 i32.const 0 local.get $n i32.const 131072 i32.const 131072
            call $io call $done drop i32.const 0))"#,
        )
        .unwrap();
        let capabilities = BTreeSet::from([
            IoCapability::HttpRequest,
            IoCapability::HttpListen,
            IoCapability::HttpPublish,
        ]);
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration = io::declaration(
            capabilities.iter().copied().collect(),
            vec!["io.invoke".into()],
        );
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_jobs = 1;
        budget.max_resources = 1;
        budget.max_job_bytes = per_job;
        budget.max_bytes = total;
        manifest.required_features.push(io::FEATURE.into());
        declaration.service_schema_sha256 = morrow_core::service::schema_digest().to_vec();
        declaration.service_run = Some(io::proto::ServiceRunProfile {
            schema_version: 1,
            max_duration_ms: 60_000,
            budget: Some(io::proto::ServiceRunBudget {
                schema_version: 1,
                max_jobs: 1,
                max_bytes: total,
            }),
        });
        manifest.required_features.extend([
            io::SERVICE_RUN_FEATURE.into(),
            io::SERVICE_RUN_BUDGET_FEATURE.into(),
        ]);
        manifest.io_declaration = Some(declaration);
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, digest, capabilities.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, digest, true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager
            .bind_budgeted_service_run(
                &host,
                &instance,
                digest,
                manager.revision(),
                &capabilities,
                60_001,
                1,
                ServiceRunBudget {
                    max_jobs: 1,
                    max_bytes: approved_bytes,
                },
            )
            .unwrap();
        let worker = IoWorker::spawn_managed(
            &manager,
            host,
            instance,
            binding,
            || 2,
            1,
            JobLimits::new(1, per_job, total).unwrap(),
        )
        .unwrap();
        Self {
            _dir: dir,
            _manager: manager,
            worker,
            digest,
        }
    }
    fn command(&self, request: &Request, operation: &str, response_limit: u64) -> Command {
        Command {
            operation_id: operation.into(),
            subject: ID.into(),
            package_sha256: self.digest,
            capability: IoCapability::HttpRequest,
            protocol_sha256: morrow_core::io::schema_digest(),
            request_sha256: request.digest(),
            approval_sha256: [3; 32],
            target_sha256: [4; 32],
            request_bytes: request.bytes().len() as u64,
            response_limit,
        }
    }
    fn finish(&mut self) -> HostRuntime {
        self.worker.stop();
        let end = Instant::now() + WAIT;
        loop {
            if let Some(host) = self.worker.try_finish().unwrap() {
                host.store_local().integrity_check().unwrap();
                return host;
            }
            assert!(Instant::now() < end, "worker did not stop");
            thread::sleep(Duration::from_millis(1));
        }
    }
}
struct Gate {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
struct Script {
    command: Command,
    calls: Arc<AtomicUsize>,
    errors: Arc<Mutex<Vec<io_execution::Error>>>,
    fail: bool,
    gate: Option<Gate>,
}
impl BrokerRouter for Script {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> Result<Vec<u8>, RouterFault> {
        assert_eq!(call, 0);
        let expected = request.bytes().to_vec();
        let result = response(request);
        let calls = self.calls.clone();
        let fail = self.fail;
        let gate = self.gate.take();
        context
            .dispatch(&self.command, move |actual| {
                assert_eq!(
                    actual, expected,
                    "backend must receive the exact protected guest frame"
                );
                calls.fetch_add(1, Ordering::SeqCst);
                if let Some(gate) = gate {
                    gate.entered.send(()).unwrap();
                    gate.release.recv_timeout(WAIT).map_err(|_| ())?;
                }
                if fail { Err(()) } else { Ok(result) }
            })
            .map_err(|error| {
                self.errors.lock().unwrap().push(error);
                match error {
                    io_execution::Error::Limit => RouterFault::Limit,
                    io_execution::Error::OutcomeUnknown | io_execution::Error::CommitUnknown => {
                        RouterFault::Unknown
                    }
                    _ => RouterFault::Denied,
                }
            })
    }
}
fn script(command: &Command, calls: &Arc<AtomicUsize>, fail: bool) -> Script {
    Script {
        command: command.clone(),
        calls: calls.clone(),
        errors: Arc::new(Mutex::new(vec![])),
        fail,
        gate: None,
    }
}
fn ready(job: &mut JobHandle) {
    let end = Instant::now() + WAIT;
    loop {
        match job.poll() {
            Poll::Ready => return,
            Poll::Pending => {
                assert!(Instant::now() < end, "job did not finish");
                thread::sleep(Duration::from_millis(1));
            }
            other => panic!("unexpected job state {other:?}"),
        }
    }
}
fn consume(job: &mut JobHandle) -> JobReport {
    ready(job);
    job.read(131072).unwrap().unwrap()
}
fn submit(run: &mut Running, request: &Request, router: Script) -> JobHandle {
    run.worker
        .submit_brokered(request.bytes().to_vec(), Box::new(router), WAIT)
        .unwrap()
}
fn observed(host: &HostRuntime, request: &Request, operation: &str) {
    let store = host.store_local();
    let record = store.lookup_io_intent(ID, operation).unwrap().unwrap();
    assert_eq!(record.phase(), Phase::Observed);
    assert_eq!(record.recovery(), Recovery::AlreadyObserved);
    assert_eq!(
        store
            .io_material(ID, operation, Kind::Request)
            .unwrap()
            .unwrap()
            .payload(),
        request.bytes()
    );
    assert_eq!(
        store
            .io_material(ID, operation, Kind::Response)
            .unwrap()
            .unwrap()
            .payload(),
        response(request)
    );
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
}
#[test]
fn exact_run_byte_ceiling_preserves_reserved_response_and_final_job_delivery() {
    let request = request("run-exact");
    let cost = 2 * request.bytes().len() as u64 + RESPONSE_LIMIT;
    let mut run = Running::new(cost * 2, cost);
    let command = run.command(&request, "run-exact", RESPONSE_LIMIT);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut job = submit(&mut run, &request, script(&command, &calls, false));
    ready(&mut job);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let usage = run.worker.service_run_usage().unwrap();
    assert_eq!(usage.jobs, 1);
    assert_eq!(usage.bytes, cost);
    let report = consume(&mut job);
    assert!(report.task.execution.outcome.is_ok());
    assert_eq!(report.http_response.unwrap().body, b"synthetic-result");
    assert_eq!(run.worker.service_run_usage().unwrap(), usage);
    assert!(matches!(
        run.worker.submit_brokered(
            request.bytes().to_vec(),
            Box::new(script(&command, &calls, false)),
            WAIT
        ),
        Err(morrow_plugin_runtime::io_jobs::JobError::Limit)
    ));
    assert_eq!(run.worker.service_run_usage().unwrap(), usage);
    observed(&run.finish(), &request, "run-exact");
}

#[test]
fn host_run_response_sublimit_rejects_before_any_backend_effect() {
    let request = request("run-denied");
    let cost = 2 * request.bytes().len() as u64 + RESPONSE_LIMIT;
    let mut run = Running::new(cost * 2, cost - 1);
    let command = run.command(&request, "run-denied", RESPONSE_LIMIT);
    let calls = Arc::new(AtomicUsize::new(0));
    let router = script(&command, &calls, false);
    let errors = router.errors.clone();
    let mut job = submit(&mut run, &request, router);
    let report = consume(&mut job);
    assert!(report.task.execution.outcome.is_err());
    assert!(report.http_response.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(*errors.lock().unwrap(), vec![io_execution::Error::Limit]);
    let usage = run.worker.service_run_usage().unwrap();
    assert_eq!(usage.jobs, 1);
    assert_eq!(usage.bytes, 2 * request.bytes().len() as u64);
    let host = run.finish();
    if let Some(record) = host
        .store_local()
        .lookup_io_intent(ID, "run-denied")
        .unwrap()
    {
        assert_eq!(record.phase(), Phase::Prepared);
    }
    assert_eq!(
        host.store_local().io_material_reservation_usage().unwrap(),
        (0, 0)
    );
}
