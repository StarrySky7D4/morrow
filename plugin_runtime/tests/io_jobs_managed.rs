//! Managed admission and delivery with real registry controls and a scripted router.
//! These tests execute Wasm, but do not claim real file or network effects.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    io::{Header, HttpOutcome, HttpSubmission, Request, Response, Status},
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
    io_binding::{IoBinding, IoLease},
    io_jobs::{IoWorker, JobError, JobHandle, JobLimits, Poll, Router, RouterFault},
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
const ID: &str = "org.example.managed.jobs";
const WAIT: Duration = Duration::from_secs(10);
const EXPIRES: u64 = 100;
fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileRead])
}
fn input() -> Vec<u8> {
    Request::encode_read(1, &[5; 32], 0, 64)
        .unwrap()
        .bytes()
        .to_vec()
}
fn reply(raw: &[u8]) -> Vec<u8> {
    Response::encode(
        &Request::decode(raw).unwrap(),
        Status::Completed,
        b"private",
        0,
        true,
    )
    .unwrap()
}
struct Script(Arc<AtomicUsize>);
impl Router for Script {
    fn route(&mut self, call: u32, request: &[u8]) -> Result<Vec<u8>, RouterFault> {
        assert_eq!(call, 0);
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(reply(request))
    }
}
fn script() -> (Arc<AtomicUsize>, Box<dyn Router>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (calls.clone(), Box::new(Script(calls)))
}
struct Fixture {
    _dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    binding: IoBinding,
    observer: IoBinding,
    digest: [u8; 32],
    limits: JobLimits,
}
impl Fixture {
    fn new() -> Self {
        Self::budget(4096, 16384)
    }
    fn budget(per_job: u64, total: u64) -> Self {
        Self::configured(per_job, total, caps())
    }
    fn configured(per_job: u64, total: u64, capabilities: BTreeSet<IoCapability>) -> Self {
        let dir = tempfile::tempdir().unwrap();
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
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration = io::declaration(
            capabilities.iter().copied().collect(),
            vec!["io.invoke".into()],
        );
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_jobs = 2;
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
                EXPIRES,
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
                EXPIRES,
                1,
            )
            .unwrap();
        Self {
            _dir: dir,
            manager,
            host,
            instance,
            binding,
            observer,
            digest,
            limits: JobLimits::new(1, per_job, total).unwrap(),
        }
    }
    fn reserve(&self) -> IoLease {
        self.observer
            .admit(
                &self.manager,
                &self.host,
                &self.instance,
                IoCapability::FileRead,
                0,
                1,
                1,
            )
            .unwrap()
    }
    fn start(self) -> Running {
        let Self {
            _dir,
            manager,
            host,
            instance,
            binding,
            observer,
            digest,
            limits,
        } = self;
        let time = Arc::new(AtomicU64::new(2));
        let ticks = time.clone();
        let worker = IoWorker::spawn_managed(
            &manager,
            host,
            instance,
            binding,
            move || ticks.load(Ordering::SeqCst),
            2,
            limits,
        )
        .unwrap();
        Running {
            _dir,
            manager: Some(manager),
            worker,
            observer,
            time,
            digest,
        }
    }
}
struct Running {
    _dir: tempfile::TempDir,
    manager: Option<Manager>,
    worker: IoWorker,
    observer: IoBinding,
    time: Arc<AtomicU64>,
    digest: [u8; 32],
}
fn ready(handle: &mut JobHandle) {
    let end = Instant::now() + WAIT;
    loop {
        match handle.poll() {
            Poll::Ready => return,
            Poll::Pending => {
                assert!(Instant::now() < end);
                thread::sleep(Duration::from_millis(1));
            }
            phase => panic!("unexpected poll: {phase:?}"),
        }
    }
}
fn finish(worker: &mut IoWorker) {
    worker.stop();
    let end = Instant::now() + WAIT;
    loop {
        match worker.try_finish().unwrap() {
            Some(host) => {
                host.store_local().integrity_check().unwrap();
                return;
            }
            None => {
                assert!(Instant::now() < end);
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
#[test]
fn managed_read_uses_one_shared_job_and_exact_cumulative_bytes() {
    let mut run = Fixture::new().start();
    let raw = input();
    let cost = (raw.len() * 2 + reply(&raw).len()) as u64;
    let (calls, router) = script();
    let mut job = run.worker.submit(raw, router, WAIT).unwrap();
    ready(&mut job);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.observer.usage().jobs, 1);
    assert_eq!(run.observer.usage().bytes, cost);
    assert_eq!(job.read(1).err(), Some(JobError::ReadBound));
    assert_eq!(run.observer.usage().jobs, 1);
    let report = job.read(64).unwrap().unwrap();
    assert!(report.task.execution.outcome.is_ok());
    assert_eq!(report.response.unwrap().payload, b"private");
    assert_eq!(report.bytes, cost);
    assert_eq!(run.observer.usage().jobs, 0);
    assert_eq!(run.observer.usage().bytes, cost);
    assert_eq!(job.read(64).err(), Some(JobError::Consumed));
    finish(&mut run.worker);
}
#[test]
fn ready_result_is_not_delivered_after_io_approval_is_removed() {
    let mut run = Fixture::new().start();
    let (_, router) = script();
    let mut job = run.worker.submit(input(), router, WAIT).unwrap();
    ready(&mut job);
    let manager = run.manager.as_mut().unwrap();
    manager
        .approve_io(ID, run.digest, BTreeSet::new(), manager.revision())
        .unwrap();
    let report = job.read(0).unwrap().unwrap();
    assert!(report.task.execution.outcome.is_err());
    assert!(report.cancelled && report.response.is_none());
    assert_eq!(run.observer.usage().jobs, 0);
    finish(&mut run.worker);
}
#[test]
fn dropping_manager_revokes_already_ready_delivery() {
    let mut run = Fixture::new().start();
    let (_, router) = script();
    let mut job = run.worker.submit(input(), router, WAIT).unwrap();
    ready(&mut job);
    drop(run.manager.take());
    let report = job.read(0).unwrap().unwrap();
    assert!(report.task.execution.outcome.is_err());
    assert!(report.cancelled && report.response.is_none());
    finish(&mut run.worker);
}
#[test]
fn read_samples_current_trusted_clock_after_ready() {
    let mut run = Fixture::new().start();
    let (_, router) = script();
    let mut job = run.worker.submit(input(), router, WAIT).unwrap();
    ready(&mut job);
    run.time.store(EXPIRES, Ordering::SeqCst);
    let report = job.read(0).unwrap().unwrap();
    assert!(report.task.execution.outcome.is_err());
    assert!(report.cancelled && report.response.is_none());
    finish(&mut run.worker);
}
#[test]
fn ready_slots_compete_with_other_bindings_until_read_or_drop() {
    for consume in [false, true] {
        let fixture = Fixture::new();
        let held = fixture.reserve();
        let mut run = fixture.start();
        let (_, router) = script();
        let mut first = run.worker.submit(input(), router, WAIT).unwrap();
        ready(&mut first);
        assert_eq!(run.observer.usage().jobs, 2);
        let before = run.observer.usage().bytes;
        let (_, router) = script();
        assert!(matches!(
            run.worker.submit(input(), router, WAIT),
            Err(JobError::Limit)
        ));
        assert_eq!(run.observer.usage().bytes, before);
        if consume {
            assert!(
                first
                    .read(64)
                    .unwrap()
                    .unwrap()
                    .task
                    .execution
                    .outcome
                    .is_ok()
            );
        }
        drop(first);
        assert_eq!(run.observer.usage().jobs, 1);
        assert_eq!(run.observer.usage().bytes, before);
        let (_, router) = script();
        let mut second = run.worker.submit(input(), router, WAIT).unwrap();
        ready(&mut second);
        assert!(
            second
                .read(64)
                .unwrap()
                .unwrap()
                .task
                .execution
                .outcome
                .is_ok()
        );
        drop(held);
        assert_eq!(run.observer.usage().jobs, 0);
        finish(&mut run.worker);
    }
}
#[test]
fn cumulative_shared_bytes_are_not_refunded_after_consumption() {
    let raw = input();
    let cost = (2 * raw.len() + reply(&raw).len()) as u64;
    let mut run = Fixture::budget(cost, cost + raw.len() as u64 - 1).start();
    let (_, router) = script();
    let mut job = run.worker.submit(raw.clone(), router, WAIT).unwrap();
    ready(&mut job);
    assert!(
        job.read(64)
            .unwrap()
            .unwrap()
            .task
            .execution
            .outcome
            .is_ok()
    );
    assert_eq!(run.observer.usage().bytes, cost);
    let (_, router) = script();
    assert!(matches!(
        run.worker.submit(raw, router, WAIT),
        Err(JobError::Limit)
    ));
    assert_eq!(run.observer.usage().bytes, cost);
    finish(&mut run.worker);
}
#[test]
fn wrong_manager_host_and_instance_are_rejected_before_clock() {
    for mismatch in 0..3 {
        let mut fixture = Fixture::new();
        let other = Fixture::new();
        let ticks = Arc::new(AtomicUsize::new(0));
        let observed = ticks.clone();
        let replacement = if mismatch == 2 {
            Some(fixture.manager.connect(ID, &mut fixture.host).unwrap())
        } else {
            None
        };
        let manager = if mismatch == 0 {
            &other.manager
        } else {
            &fixture.manager
        };
        let host = if mismatch == 1 {
            other.host
        } else {
            fixture.host
        };
        let (instance, _original) = match replacement {
            Some(instance) => (instance, Some(fixture.instance)),
            None => (fixture.instance, None),
        };
        let result = IoWorker::spawn_managed(
            manager,
            host,
            instance,
            fixture.binding,
            move || {
                observed.fetch_add(1, Ordering::SeqCst);
                2
            },
            2,
            fixture.limits,
        );
        assert!(matches!(result, Err(JobError::InvalidOptions)));
        assert_eq!(ticks.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn an_unapproved_action_never_enters_the_trusted_router() {
    let mut run = Fixture::new().start();
    let raw = Request::encode_http_submit(
        2,
        &HttpSubmission {
            operation_id: b"http-unapproved".to_vec(),
            deadline_ms: 10,
            endpoint: b"approved-endpoint".to_vec(),
            method: "GET".into(),
            relative_target: "/".into(),
            headers: vec![],
            body: vec![],
            credential: vec![],
        },
    )
    .unwrap()
    .bytes()
    .to_vec();
    let (calls, router) = script();
    let mut job = run.worker.submit(raw, router, WAIT).unwrap();
    ready(&mut job);
    let report = job.read(0).unwrap().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(report.task.execution.outcome.is_err());
    assert!(report.response.is_none());
    finish(&mut run.worker);
}

struct HttpScript;
impl Router for HttpScript {
    fn route(&mut self, call: u32, request: &[u8]) -> Result<Vec<u8>, RouterFault> {
        assert_eq!(call, 0);
        let request = Request::decode(request).map_err(|_| RouterFault::Denied)?;
        Response::encode_http(
            &request,
            &HttpOutcome {
                status: Status::Completed,
                http_status: 201,
                headers: vec![Header {
                    name: "x-private".into(),
                    value: b"metadata".to_vec(),
                }],
                body: b"created-secret".to_vec(),
            },
        )
        .map_err(|_| RouterFault::Denied)
    }
}
#[test]
fn approved_http_script_preserves_status_and_bounds_or_suppresses_all_response_data() {
    for revoke in [false, true] {
        let mut run =
            Fixture::configured(4096, 16384, BTreeSet::from([IoCapability::HttpRequest])).start();
        let raw = Request::encode_http_submit(
            3,
            &HttpSubmission {
                operation_id: b"http-created".to_vec(),
                deadline_ms: 10,
                endpoint: b"approved-endpoint".to_vec(),
                method: "POST".into(),
                relative_target: "/objects".into(),
                headers: vec![],
                body: b"input".to_vec(),
                credential: vec![],
            },
        )
        .unwrap()
        .bytes()
        .to_vec();
        let mut job = run.worker.submit(raw, Box::new(HttpScript), WAIT).unwrap();
        ready(&mut job);
        if revoke {
            let manager = run.manager.as_mut().unwrap();
            manager
                .approve_io(ID, run.digest, BTreeSet::new(), manager.revision())
                .unwrap();
            let report = job.read(0).unwrap().unwrap();
            assert!(report.task.execution.outcome.is_err());
            assert!(report.cancelled);
            assert!(report.http_response.is_none() && report.response.is_none());
            assert_eq!(report.payload_bytes(), 0);
        } else {
            // Body alone is insufficient: returned headers also consume read allowance.
            assert_eq!(
                job.read(b"created-secret".len()).err(),
                Some(JobError::ReadBound)
            );
            let report = job.read(128).unwrap().unwrap();
            assert!(report.task.execution.outcome.is_ok());
            assert_eq!(
                report.payload_bytes(),
                b"created-secret".len() + "x-private".len() + b"metadata".len()
            );
            let http = report.http_response.unwrap();
            assert_eq!(http.http_status, 201);
            assert_eq!(http.body, b"created-secret");
            assert_eq!(http.headers[0].name, "x-private");
            assert_eq!(http.headers[0].value, b"metadata");
            assert!(report.response.is_none());
        }
        finish(&mut run.worker);
    }
}

struct BlockingScript {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    calls: Arc<AtomicUsize>,
}
impl Router for BlockingScript {
    fn route(&mut self, call: u32, request: &[u8]) -> Result<Vec<u8>, RouterFault> {
        assert_eq!(call, 0);
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.send(()).unwrap();
        self.release
            .recv_timeout(WAIT)
            .map_err(|_| RouterFault::Unknown)?;
        Ok(reply(request))
    }
}
#[test]
fn manager_revocation_is_nonblocking_and_abandoned_running_job_holds_shared_slot() {
    let mut run = Fixture::new().start();
    let (entered, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let running = run
        .worker
        .submit(
            input(),
            Box::new(BlockingScript {
                entered,
                release: gate,
                calls: calls.clone(),
            }),
            WAIT,
        )
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    let (queued_calls, router) = script();
    let mut queued = run.worker.submit(input(), router, WAIT).unwrap();
    assert_eq!(run.observer.usage().jobs, 2);
    let charged = run.observer.usage().bytes;
    drop(running);
    assert_eq!(
        run.observer.usage().jobs,
        2,
        "callback is still blocked, so its reservation is still live"
    );
    let mut manager = run.manager.take().unwrap();
    let digest = run.digest;
    let (finished, done) = mpsc::channel();
    let management = thread::spawn(move || {
        let result = manager.approve_io(ID, digest, BTreeSet::new(), manager.revision());
        finished.send(result.is_ok()).unwrap();
        manager
    });
    let revoked = done.recv_timeout(Duration::from_secs(2));
    // Always release the callback before asserting the concurrent result so a
    // broken implementation cannot strand the worker or management test thread.
    assert_eq!(run.observer.usage().jobs, 2);
    release.send(()).unwrap();
    run.manager = Some(management.join().unwrap());
    assert!(
        revoked.unwrap(),
        "approval must complete before the blocked router returns"
    );
    ready(&mut queued);
    let report = queued.read(0).unwrap().unwrap();
    assert!(report.task.execution.outcome.is_err());
    assert!(report.cancelled && report.response.is_none() && report.http_response.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(queued_calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.observer.usage().jobs, 0);
    assert_eq!(run.observer.usage().bytes, charged);
    finish(&mut run.worker);
}
#[test]
fn response_exceeding_shared_instance_bytes_is_unknown_without_success_or_refund() {
    let raw = input();
    let cost = (2 * raw.len() + reply(&raw).len()) as u64;
    // Worker-local and per-job limits fit the entire response. Another binding
    // reserves two bytes, leaving the shared instance exactly one byte short.
    let fixture = Fixture::budget(cost, cost + 1);
    let held = fixture
        .observer
        .admit(
            &fixture.manager,
            &fixture.host,
            &fixture.instance,
            IoCapability::FileRead,
            0,
            2,
            1,
        )
        .unwrap();
    let mut run = fixture.start();
    let (calls, router) = script();
    let mut job = run.worker.submit(raw.clone(), router, WAIT).unwrap();
    ready(&mut job);
    let report = job.read(0).unwrap().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        report.task.execution.outcome,
        Err(morrow_plugin_runtime::Fault::Limits)
    );
    assert!(report.unknown);
    assert!(report.response.is_none() && report.http_response.is_none());
    assert_eq!(report.payload_bytes(), 0);
    assert_eq!(report.bytes, (2 * raw.len()) as u64);
    let charged = 2 + (2 * raw.len()) as u64;
    assert_eq!(run.observer.usage().bytes, charged);
    assert_eq!(run.observer.usage().jobs, 1);
    drop(held);
    assert_eq!(run.observer.usage().jobs, 0);
    assert_eq!(run.observer.usage().bytes, charged);
    finish(&mut run.worker);
}

struct CountedHttpScript(Arc<AtomicUsize>);
impl Router for CountedHttpScript {
    fn route(&mut self, call: u32, request: &[u8]) -> Result<Vec<u8>, RouterFault> {
        self.0.fetch_add(1, Ordering::SeqCst);
        HttpScript.route(call, request)
    }
}
#[test]
fn http_credential_reference_requires_both_approved_capabilities_before_routing() {
    for allow_credential in [false, true] {
        let mut capabilities = BTreeSet::from([IoCapability::HttpRequest]);
        if allow_credential {
            capabilities.insert(IoCapability::CredentialUse);
        }
        let mut run = Fixture::configured(4096, 16384, capabilities).start();
        let request = Request::encode_http_submit(
            4,
            &HttpSubmission {
                operation_id: b"credential-call".to_vec(),
                deadline_ms: 10,
                endpoint: b"approved-endpoint".to_vec(),
                method: "GET".into(),
                relative_target: "/private".into(),
                headers: vec![],
                body: vec![],
                credential: b"host-credential-reference".to_vec(),
            },
        )
        .unwrap()
        .bytes()
        .to_vec();
        let input_bytes = request.len() as u64;
        let calls = Arc::new(AtomicUsize::new(0));
        let mut job = run
            .worker
            .submit(request, Box::new(CountedHttpScript(calls.clone())), WAIT)
            .unwrap();
        ready(&mut job);
        let report = job.read(128).unwrap().unwrap();
        if allow_credential {
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert!(report.task.execution.outcome.is_ok());
            assert_eq!(report.http_response.unwrap().http_status, 201);
        } else {
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert!(report.task.execution.outcome.is_err());
            assert!(report.http_response.is_none() && report.response.is_none());
            // Rejected capability admission charges neither the call request nor
            // response; the original queued task input remains permanently charged.
            assert_eq!(run.observer.usage().bytes, input_bytes);
            assert_eq!(run.worker.bytes(), input_bytes);
        }
        assert_eq!(run.observer.usage().jobs, 0);
        finish(&mut run.worker);
    }
}
