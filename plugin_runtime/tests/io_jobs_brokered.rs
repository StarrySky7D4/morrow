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
    io_binding::IoBinding,
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
    manager: Manager,
    worker: IoWorker,
    observer: IoBinding,
    digest: [u8; 32],
}
impl Running {
    fn new(per_job: u64, total: u64) -> Self {
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
        let capabilities = BTreeSet::from([IoCapability::HttpRequest]);
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
            .bind_io(
                &host,
                &instance,
                digest,
                manager.revision(),
                &capabilities,
                100,
                1,
            )
            .unwrap();
        let observer = manager
            .bind_io(
                &host,
                &instance,
                digest,
                manager.revision(),
                &capabilities,
                100,
                1,
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
            manager,
            worker,
            observer,
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
fn one_managed_job_uses_one_slot_and_reserves_response_bytes_once() {
    let request = request("one-slot");
    let cost = 2 * request.bytes().len() as u64 + RESPONSE_LIMIT;
    assert!((response(&request).len() as u64) < RESPONSE_LIMIT);
    let mut run = Running::new(cost, cost);
    let command = run.command(&request, "one-slot", RESPONSE_LIMIT);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut job = submit(&mut run, &request, script(&command, &calls, false));
    ready(&mut job);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.observer.usage().jobs, 1);
    assert_eq!(run.observer.usage().resources, 0);
    assert_eq!(run.observer.usage().bytes, cost);
    assert_eq!(run.worker.bytes(), cost);
    let report = job.read(131072).unwrap().unwrap();
    assert!(report.task.execution.outcome.is_ok());
    assert_eq!(report.http_response.unwrap().body, b"synthetic-result");
    assert_eq!(report.bytes, cost);
    assert_eq!(run.observer.usage().jobs, 0);
    assert_eq!(run.observer.usage().bytes, cost);
    observed(&run.finish(), &request, "one-slot");
}
#[test]
fn observed_operation_is_not_dispatched_again() {
    let request = request("once");
    let mut run = Running::new(4096, 16384);
    let command = run.command(&request, "once", RESPONSE_LIMIT);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut first = submit(&mut run, &request, script(&command, &calls, false));
    assert!(consume(&mut first).task.execution.outcome.is_ok());
    let mut second = submit(&mut run, &request, script(&command, &calls, false));
    let denied = consume(&mut second);
    assert!(denied.task.execution.outcome.is_err());
    assert!(denied.http_response.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    observed(&run.finish(), &request, "once");
}
#[test]
fn mismatched_command_never_calls_backend_or_creates_history() {
    for mismatch in 0..6 {
        let request = request("bound-command");
        let mut run = Running::new(4096, 16384);
        let mut command = run.command(&request, "bound-command", RESPONSE_LIMIT);
        match mismatch {
            0 => command.operation_id = "foreign-operation".into(),
            1 => command.package_sha256 = [9; 32],
            2 => command.request_sha256 = [9; 32],
            3 => command.request_bytes += 1,
            4 => command.protocol_sha256 = [9; 32],
            5 => command.capability = IoCapability::FileRead,
            _ => unreachable!(),
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let mut job = submit(&mut run, &request, script(&command, &calls, false));
        let report = consume(&mut job);
        assert!(
            report.task.execution.outcome.is_err(),
            "mismatch {mismatch}"
        );
        assert!(report.http_response.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 0, "mismatch {mismatch}");
        let host = run.finish();
        for operation in ["bound-command", "foreign-operation"] {
            assert!(
                host.store_local()
                    .lookup_io_intent(ID, operation)
                    .unwrap()
                    .is_none()
            );
        }
        assert_eq!(
            host.store_local().io_material_reservation_usage().unwrap(),
            (0, 0)
        );
    }
}
#[test]
fn response_reservation_is_admitted_before_backend_for_both_budget_domains() {
    for shared in [false, true] {
        let request = request("preflight-limit");
        let cost = 2 * request.bytes().len() as u64 + RESPONSE_LIMIT;
        // Per-job and instance totals must each contain the entire response bound.
        // To exhaust only the cumulative shared domain, first spend another job.
        let mut run = if shared {
            Running::new(cost, 2 * cost - 1)
        } else {
            Running::new(cost - 1, cost * 4)
        };
        if shared {
            let prior = request_for_prior();
            assert_eq!(prior.bytes().len(), request.bytes().len());
            let prior_cmd = run.command(&prior, "previous-limits", RESPONSE_LIMIT);
            let prior_calls = Arc::new(AtomicUsize::new(0));
            let mut job = submit(&mut run, &prior, script(&prior_cmd, &prior_calls, false));
            assert!(consume(&mut job).task.execution.outcome.is_ok());
        }
        let command = run.command(&request, "preflight-limit", RESPONSE_LIMIT);
        let calls = Arc::new(AtomicUsize::new(0));
        let mut job = submit(&mut run, &request, script(&command, &calls, false));
        assert!(consume(&mut job).task.execution.outcome.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let host = run.finish();
        if let Some(record) = host
            .store_local()
            .lookup_io_intent(ID, "preflight-limit")
            .unwrap()
        {
            assert_eq!(record.phase(), Phase::Prepared);
        }
    }
}
fn request_for_prior() -> Request {
    request("previous-limits")
}
#[test]
fn backend_error_retains_unknown_and_repeat_cannot_resend() {
    unknown_cannot_resend(true);
}
#[test]
fn actual_response_over_reserved_bound_is_unknown_not_safe_to_retry() {
    unknown_cannot_resend(false);
}
fn unknown_cannot_resend(fail: bool) {
    let request = request("unknown");
    let mut run = Running::new(4096, 16384);
    let limit = if fail {
        RESPONSE_LIMIT
    } else {
        (response(&request).len() - 1) as u64
    };
    let command = run.command(&request, "unknown", limit);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut first = submit(&mut run, &request, script(&command, &calls, fail));
    let report = consume(&mut first);
    assert!(report.task.execution.outcome.is_err());
    assert!(report.http_response.is_none());
    let mut second = submit(&mut run, &request, script(&command, &calls, false));
    assert!(consume(&mut second).task.execution.outcome.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let host = run.finish();
    let record = host
        .store_local()
        .lookup_io_intent(ID, "unknown")
        .unwrap()
        .unwrap();
    assert_eq!(record.phase(), Phase::OutcomeUnknown);
    assert_eq!(record.recovery(), Recovery::ReconcileOnly);
    assert_eq!(
        host.store_local()
            .io_material(ID, "unknown", Kind::Request)
            .unwrap()
            .unwrap()
            .payload(),
        request.bytes()
    );
    assert!(matches!(
        host.store_local()
            .io_material(ID, "unknown", Kind::Response),
        Err(morrow_core::Error::EvidenceUnavailable)
    ));
}
#[test]
fn revocation_during_callback_keeps_observed_history_without_delivering_result() {
    let request = request("revoke-running");
    let mut run = Running::new(4096, 16384);
    let command = run.command(&request, "revoke-running", RESPONSE_LIMIT);
    let calls = Arc::new(AtomicUsize::new(0));
    let (entered, seen) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut router = script(&command, &calls, false);
    router.gate = Some(Gate {
        entered,
        release: gate,
    });
    let mut job = submit(&mut run, &request, router);
    seen.recv_timeout(WAIT).unwrap();
    run.manager
        .approve_io(ID, run.digest, BTreeSet::new(), run.manager.revision())
        .unwrap();
    release.send(()).unwrap();
    let report = consume(&mut job);
    assert!(report.task.execution.outcome.is_err());
    assert!(report.cancelled);
    assert!(report.http_response.is_none() && report.response.is_none());
    assert_eq!(report.payload_bytes(), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    observed(&run.finish(), &request, "revoke-running");
}

#[derive(Clone, Copy)]
enum ProtocolAttempt {
    WithoutDispatch,
    AlterResponse,
    DenyAfterDispatch,
    DispatchTwice,
}
struct ProtocolRouter {
    command: Command,
    calls: Arc<AtomicUsize>,
    attempt: ProtocolAttempt,
}
impl BrokerRouter for ProtocolRouter {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> Result<Vec<u8>, RouterFault> {
        assert_eq!(call, 0);
        if matches!(self.attempt, ProtocolAttempt::WithoutDispatch) {
            // Valid wire framing alone cannot manufacture a broker execution.
            return Ok(response(request));
        }
        let expected = request.bytes().to_vec();
        let original = response(request);
        let reply = context
            .dispatch(&self.command, |raw| {
                assert_eq!(raw, expected);
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(original)
            })
            .expect("first genuine dispatch must succeed");
        match self.attempt {
            ProtocolAttempt::AlterResponse => Response::encode_http(
                request,
                &HttpOutcome {
                    status: Status::Completed,
                    http_status: 201,
                    headers: vec![],
                    body: b"forged-after-commit".to_vec(),
                },
            )
            .map_err(|_| RouterFault::Denied),
            ProtocolAttempt::DenyAfterDispatch => Err(RouterFault::Denied),
            ProtocolAttempt::DispatchTwice => {
                let repeated = context.dispatch(&self.command, |_| {
                    self.calls.fetch_add(1, Ordering::SeqCst);
                    Ok(response(request))
                });
                assert_eq!(repeated.err(), Some(io_execution::Error::Duplicate));
                Ok(reply)
            }
            ProtocolAttempt::WithoutDispatch => unreachable!(),
        }
    }
}
fn protocol_job(
    run: &mut Running,
    request: &Request,
    operation: &str,
    attempt: ProtocolAttempt,
    calls: &Arc<AtomicUsize>,
) -> JobHandle {
    let router = ProtocolRouter {
        command: run.command(request, operation, RESPONSE_LIMIT),
        calls: calls.clone(),
        attempt,
    };
    run.worker
        .submit_brokered(request.bytes().to_vec(), Box::new(router), WAIT)
        .unwrap()
}
#[test]
fn valid_response_without_dispatch_cannot_be_delivered_as_broker_output() {
    let request = request("forged-success");
    let mut run = Running::new(4096, 16384);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut job = protocol_job(
        &mut run,
        &request,
        "forged-success",
        ProtocolAttempt::WithoutDispatch,
        &calls,
    );
    let report = consume(&mut job);
    assert!(report.task.execution.outcome.is_err());
    assert!(report.http_response.is_none() && report.response.is_none());
    assert_eq!(report.payload_bytes(), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let host = run.finish();
    assert!(
        host.store_local()
            .lookup_io_intent(ID, "forged-success")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        host.store_local().io_material_reservation_usage().unwrap(),
        (0, 0)
    );
}
#[test]
fn changed_or_denied_reply_after_dispatch_is_unknown_but_keeps_original_fact() {
    for attempt in [
        ProtocolAttempt::AlterResponse,
        ProtocolAttempt::DenyAfterDispatch,
    ] {
        let request = request("changed-delivery");
        let mut run = Running::new(4096, 16384);
        let calls = Arc::new(AtomicUsize::new(0));
        let mut job = protocol_job(&mut run, &request, "changed-delivery", attempt, &calls);
        let report = consume(&mut job);
        assert!(report.task.execution.outcome.is_err());
        assert!(report.unknown);
        assert!(report.http_response.is_none() && report.response.is_none());
        assert_eq!(report.payload_bytes(), 0);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Delivery is unknown, but the completed backend observation is not
        // rewritten as failure and the substituted bytes are never persisted.
        observed(&run.finish(), &request, "changed-delivery");
    }
}
#[test]
fn repeated_dispatch_in_same_route_never_reenters_backend() {
    let request = request("double-dispatch");
    let mut run = Running::new(4096, 16384);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut job = protocol_job(
        &mut run,
        &request,
        "double-dispatch",
        ProtocolAttempt::DispatchTwice,
        &calls,
    );
    let report = consume(&mut job);
    assert!(report.task.execution.outcome.is_ok());
    assert_eq!(report.http_response.unwrap().body, b"synthetic-result");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    observed(&run.finish(), &request, "double-dispatch");
}

struct DeniedUnknownRouter {
    command: Command,
    calls: Arc<AtomicUsize>,
}
impl BrokerRouter for DeniedUnknownRouter {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> Result<Vec<u8>, RouterFault> {
        assert_eq!(call, 0);
        let result = context.dispatch(&self.command, |_| {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(response(request))
        });
        assert_eq!(result.err(), Some(io_execution::Error::OutcomeUnknown));
        // Deliberately lossy host adapter: it must not hide the durable fact.
        Err(RouterFault::Denied)
    }
}
#[test]
fn persisted_unknown_cannot_be_downgraded_to_denied_by_router() {
    let request = request("unknown-demotion");
    let mut run = Running::new(4096, 16384);
    let command = run.command(&request, "unknown-demotion", RESPONSE_LIMIT);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut first = submit(&mut run, &request, script(&command, &calls, true));
    let failed = consume(&mut first);
    assert!(failed.unknown && failed.task.execution.outcome.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let mut second = run
        .worker
        .submit_brokered(
            request.bytes().to_vec(),
            Box::new(DeniedUnknownRouter {
                command,
                calls: calls.clone(),
            }),
            WAIT,
        )
        .unwrap();
    let report = consume(&mut second);
    assert!(report.unknown);
    assert!(report.task.execution.outcome.is_err());
    assert!(report.http_response.is_none() && report.response.is_none());
    assert_eq!(report.payload_bytes(), 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let host = run.finish();
    let record = host
        .store_local()
        .lookup_io_intent(ID, "unknown-demotion")
        .unwrap()
        .unwrap();
    assert_eq!(record.phase(), Phase::OutcomeUnknown);
    assert_eq!(record.recovery(), Recovery::ReconcileOnly);
}
