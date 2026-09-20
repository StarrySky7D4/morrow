//! Explicit renewal retains the original instance, grants and cumulative ledger.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]

use morrow_core::{
    dispatch::HostRuntime,
    io as wire,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    service::{self, Invocation, Reply},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Limits,
    io_binding::{Error, IoBinding, ServiceRunBudget, ServiceRunSnapshot},
    io_jobs::{BrokerRouter, IoWorker, JobHandle, JobLimits, Poll, RouteContext, RouterFault},
    manager::{ManagedInstance, Manager},
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Barrier,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.service.run.renewal";
const RUN_MS: u64 = 60_000;
const INITIAL_EXPIRES: u64 = 20_001;
const NEXT_EXPIRES: u64 = 40_001;
const MAX_BYTES: u64 = 65_536;
const WAIT: Duration = Duration::from_secs(10);

fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([
        IoCapability::FileRead,
        IoCapability::HttpListen,
        IoCapability::HttpPublish,
        IoCapability::HttpRequest,
    ])
}
fn budget(jobs: u64, bytes: u64) -> ServiceRunBudget {
    ServiceRunBudget {
        max_jobs: jobs,
        max_bytes: bytes,
    }
}
fn initial_budget() -> ServiceRunBudget {
    budget(2, 8192)
}
fn request() -> service::Request {
    service::Request::encode(
        1,
        &Invocation {
            service: "echo".into(),
            handler: "echo.call".into(),
            principal: "local-user".into(),
            method: "POST".into(),
            target: "/echo".into(),
            headers: vec![],
            body: b"hello".to_vec(),
        },
    )
    .unwrap()
}
fn reply() -> Reply {
    Reply {
        status: 200,
        headers: vec![],
        body: b"same-original-run".to_vec(),
    }
}
fn package(budgeted: bool, run_ms: u64) -> Package {
    let response = service::Response::encode(&request(), &reply()).unwrap();
    let literal: String = response
        .iter()
        .map(|byte| format!("\\{byte:02x}"))
        .collect();
    let wasm = wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
        (memory (export "memory") 3)
        (data (i32.const 131072) "{literal}")
        (func (export "morrow_run") (result i32)
            i32.const 0 i32.const 131072 call $read drop
            i32.const 131072 i32.const {} call $complete drop i32.const 0))"#,
        response.len()
    ))
    .unwrap();
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    let mut declaration = io::declaration(caps().into_iter().collect(), vec!["echo.call".into()]);
    declaration.service_schema_sha256 = service::schema_digest().to_vec();
    let limits = declaration.budget.as_mut().unwrap();
    limits.max_jobs = 2;
    limits.max_resources = 4;
    limits.max_job_bytes = 8192;
    limits.max_bytes = MAX_BYTES;
    declaration.service_run = Some(io::proto::ServiceRunProfile {
        schema_version: io::SERVICE_RUN_VERSION,
        max_duration_ms: run_ms,
        budget: budgeted.then_some(io::proto::ServiceRunBudget {
            schema_version: io::SERVICE_RUN_BUDGET_VERSION,
            max_jobs: 64,
            max_bytes: MAX_BYTES,
        }),
    });
    manifest
        .required_features
        .extend([io::FEATURE.into(), io::SERVICE_RUN_FEATURE.into()]);
    if budgeted {
        manifest
            .required_features
            .push(io::SERVICE_RUN_BUDGET_FEATURE.into());
    }
    manifest.io_declaration = Some(declaration);
    Package::build(manifest, &wasm).unwrap()
}

struct Fixture {
    dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    digest: [u8; 32],
}
impl Fixture {
    fn new(budgeted: bool, run_ms: u64) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let package = package(budgeted, run_ms);
        let digest = package.digest();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, digest, caps(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, digest, true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        Self {
            dir,
            manager,
            host,
            instance,
            digest,
        }
    }
    fn bind(&self, expires: u64, budget: ServiceRunBudget) -> IoBinding {
        self.manager
            .bind_budgeted_service_run(
                &self.host,
                &self.instance,
                self.digest,
                self.manager.revision(),
                &caps(),
                expires,
                1,
                budget,
            )
            .unwrap()
    }
    fn start(self, binding: IoBinding) -> Running {
        let listener =
            ListenerGrant::issue(&self.manager, &self.host, &self.instance, &binding, 1).unwrap();
        listener.activate().unwrap();
        let grant = ServiceGrant::issue(
            &self.manager,
            &self.host,
            &self.instance,
            &binding,
            "echo",
            "echo.call",
            1,
        )
        .unwrap()
        .bound_to_listener(&listener)
        .unwrap();
        let tick = Arc::new(AtomicU64::new(1));
        let samples = Arc::new(AtomicUsize::new(0));
        let clock_tick = tick.clone();
        let clock_samples = samples.clone();
        let caller = thread::current().id();
        let worker = IoWorker::spawn_managed(
            &self.manager,
            self.host,
            self.instance,
            binding,
            move || {
                // The worker monitor samples independently every 10ms. Count
                // this test caller only, without changing the shared clock.
                if thread::current().id() == caller {
                    clock_samples.fetch_add(1, Ordering::SeqCst);
                }
                clock_tick.load(Ordering::SeqCst)
            },
            2,
            JobLimits::new(2, 8192, MAX_BYTES).unwrap(),
        )
        .unwrap();
        Running {
            _dir: self.dir,
            manager: self.manager,
            digest: self.digest,
            worker,
            grant,
            listener,
            tick,
            samples,
        }
    }
}
struct Running {
    _dir: tempfile::TempDir,
    manager: Manager,
    digest: [u8; 32],
    worker: IoWorker,
    grant: ServiceGrant,
    listener: ListenerGrant,
    tick: Arc<AtomicU64>,
    samples: Arc<AtomicUsize>,
}
impl Running {
    fn new() -> Self {
        let fixture = Fixture::new(true, RUN_MS);
        let binding = fixture.bind(INITIAL_EXPIRES, initial_budget());
        fixture.start(binding)
    }
    fn renew(
        &self,
        revision: u64,
        expires: u64,
        budget: ServiceRunBudget,
    ) -> Result<ServiceRunSnapshot, Error> {
        self.worker.renew_service_run(
            &self.manager,
            &self.grant,
            self.manager.revision(),
            revision,
            expires,
            budget,
        )
    }
    fn snapshot(&self) -> ServiceRunSnapshot {
        self.worker.service_run_snapshot().unwrap()
    }
    fn submit(&self) -> JobHandle {
        self.worker
            .submit_service(request(), self.grant.clone(), Box::new(NoIo), WAIT)
            .unwrap()
    }
}
struct NoIo;
impl BrokerRouter for NoIo {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &wire::Request,
    ) -> Result<Vec<u8>, RouterFault> {
        panic!("zero-IO guest must not dispatch a network call")
    }
}
fn ready(job: &mut JobHandle) {
    let end = Instant::now() + WAIT;
    loop {
        match job.poll() {
            Poll::Ready => return,
            Poll::Pending => {
                assert!(Instant::now() < end);
                thread::sleep(Duration::from_millis(1));
            }
            other => panic!("unexpected job status: {other:?}"),
        }
    }
}
fn same_snapshot(a: ServiceRunSnapshot, b: ServiceRunSnapshot) {
    assert_eq!(
        (
            a.revision,
            a.expires,
            a.budget.max_jobs,
            a.budget.max_bytes,
            a.usage.jobs,
            a.usage.bytes
        ),
        (
            b.revision,
            b.expires,
            b.budget.max_jobs,
            b.budget.max_bytes,
            b.usage.jobs,
            b.usage.bytes
        )
    );
}
fn finish(worker: &mut IoWorker) {
    worker.stop();
    let end = Instant::now() + WAIT;
    loop {
        if let Some(host) = worker.try_finish().unwrap() {
            host.store_local().integrity_check().unwrap();
            return;
        }
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn renewal_updates_original_grant_and_listener_copies_without_resetting_usage() {
    let fixture = Fixture::new(true, RUN_MS);
    let binding = fixture.bind(INITIAL_EXPIRES, budget(1, 8192));
    let mut run = fixture.start(binding);
    let original_grant = run.grant.clone();
    let original_listener = run.listener.clone();
    let mut first = run.submit();
    ready(&mut first);
    let before = run.snapshot();
    assert_eq!(before.revision, 1);
    assert_eq!(before.usage.jobs, 1);
    assert!(before.usage.bytes > 0);
    assert!(
        run.worker
            .submit_service(request(), original_grant.clone(), Box::new(NoIo), WAIT)
            .is_err()
    );
    run.tick.store(10_001, Ordering::SeqCst);
    let renewed = run.renew(1, NEXT_EXPIRES, budget(2, 16_384)).unwrap();
    assert_eq!((renewed.revision, renewed.expires), (2, NEXT_EXPIRES));
    assert_eq!(
        (renewed.usage.jobs, renewed.usage.bytes),
        (before.usage.jobs, before.usage.bytes)
    );
    run.tick.store(INITIAL_EXPIRES + 1, Ordering::SeqCst);
    run.worker.check_listener(&original_listener).unwrap();
    run.worker.check_service(&original_grant).unwrap();
    let first_report = first.read(8192).unwrap().unwrap();
    assert_eq!(first_report.task.execution.outcome, Ok(0));
    assert!(first_report.service_response.unwrap() == reply());
    let mut second = run
        .worker
        .submit_service(request(), original_grant, Box::new(NoIo), WAIT)
        .unwrap();
    ready(&mut second);
    assert!(
        second
            .read(8192)
            .unwrap()
            .unwrap()
            .service_response
            .unwrap()
            == reply()
    );
    let after = run.snapshot();
    assert_eq!(after.usage.jobs, 2);
    assert_eq!(after.usage.bytes, before.usage.bytes * 2);
    finish(&mut run.worker);
}

#[test]
fn stale_noop_shrinking_and_over_ceiling_renewals_preserve_snapshot() {
    let mut run = Running::new();
    for (revision, expires, budget) in [
        (0, NEXT_EXPIRES, initial_budget()),
        (2, NEXT_EXPIRES, initial_budget()),
        (1, INITIAL_EXPIRES, initial_budget()),
        (1, INITIAL_EXPIRES - 1, budget(3, 8192)),
        (1, NEXT_EXPIRES, budget(1, 8192)),
        (1, NEXT_EXPIRES, budget(2, 8191)),
        (1, RUN_MS + 2, initial_budget()),
        (1, NEXT_EXPIRES, budget(65, 8192)),
        (1, NEXT_EXPIRES, budget(2, MAX_BYTES + 1)),
        (1, u64::MAX, budget(u64::MAX, u64::MAX)),
    ] {
        let before = run.snapshot();
        assert!(run.renew(revision, expires, budget).is_err());
        same_snapshot(before, run.snapshot());
    }
    run.renew(1, INITIAL_EXPIRES, budget(3, 8192)).unwrap();
    assert_eq!(run.snapshot().revision, 2);
    assert!(run.renew(1, NEXT_EXPIRES, budget(4, 8192)).is_err());
    run.renew(2, NEXT_EXPIRES, budget(3, 8192)).unwrap();
    assert_eq!(run.snapshot().revision, 3);
    finish(&mut run.worker);
}

#[test]
fn foreign_manager_or_grant_and_stale_registry_do_not_sample_callers_clock() {
    let mut run = Running::new();
    let mut foreign = Running::new();
    let samples = run.samples.load(Ordering::SeqCst);
    let before = run.snapshot();
    // Invalid identities must be rejected before this caller samples the
    // original clock. Background monitoring remains free to check liveness.
    for (manager, grant, registry_revision) in [
        (&foreign.manager, &run.grant, foreign.manager.revision()),
        (&run.manager, &foreign.grant, run.manager.revision()),
        (&run.manager, &run.grant, run.manager.revision() - 1),
    ] {
        assert!(
            run.worker
                .renew_service_run(
                    manager,
                    grant,
                    registry_revision,
                    1,
                    NEXT_EXPIRES,
                    initial_budget()
                )
                .is_err()
        );
        assert_eq!(run.samples.load(Ordering::SeqCst), samples);
        same_snapshot(before, run.snapshot());
    }
    run.renew(1, NEXT_EXPIRES, initial_budget()).unwrap();
    run.worker.check_service(&run.grant).unwrap();
    finish(&mut run.worker);
    finish(&mut foreign.worker);
}

#[test]
fn expiration_and_clock_rollback_are_sticky_and_cannot_be_renewed() {
    for rollback in [false, true] {
        let mut run = Running::new();
        run.tick.store(10, Ordering::SeqCst);
        run.worker.check_service(&run.grant).unwrap();
        let before = run.snapshot();
        run.tick
            .store(if rollback { 9 } else { INITIAL_EXPIRES }, Ordering::SeqCst);
        assert!(matches!(
            run.renew(1, NEXT_EXPIRES, initial_budget()),
            // The worker's monitor can observe the same bad clock first and
            // retire the instance before this renewal reaches its time check.
            Err(Error::Clock | Error::Expired | Error::Denied)
        ));
        run.tick.store(11, Ordering::SeqCst);
        assert!(run.renew(1, NEXT_EXPIRES, initial_budget()).is_err());
        assert!(run.worker.check_service(&run.grant).is_err());
        same_snapshot(before, run.snapshot());
        finish(&mut run.worker);
    }
}

#[test]
fn concurrent_same_revision_can_only_renew_once() {
    let mut run = Running::new();
    let barrier = Barrier::new(3);
    let successes = thread::scope(|scope| {
        let attempt = || {
            barrier.wait();
            run.renew(1, NEXT_EXPIRES, initial_budget()).is_ok()
        };
        let a = scope.spawn(attempt);
        let b = scope.spawn(attempt);
        barrier.wait();
        usize::from(a.join().unwrap()) + usize::from(b.join().unwrap())
    });
    assert_eq!(successes, 1);
    assert_eq!(
        (run.snapshot().revision, run.snapshot().expires),
        (2, NEXT_EXPIRES)
    );
    finish(&mut run.worker);
}

#[test]
fn manager_disable_and_approval_withdrawal_cannot_be_undone_by_renewal() {
    for withdraw in [false, true] {
        let mut run = Running::new();
        let before = run.snapshot();
        if withdraw {
            run.manager
                .approve_io(ID, run.digest, BTreeSet::new(), run.manager.revision())
                .unwrap();
        } else {
            run.manager
                .set_enabled(ID, run.digest, false, run.manager.revision())
                .unwrap();
        }
        assert!(run.renew(1, NEXT_EXPIRES, initial_budget()).is_err());
        if withdraw {
            run.manager
                .approve_io(ID, run.digest, caps(), run.manager.revision())
                .unwrap();
        } else {
            run.manager
                .set_enabled(ID, run.digest, true, run.manager.revision())
                .unwrap();
        }
        assert!(run.renew(1, NEXT_EXPIRES, initial_budget()).is_err());
        same_snapshot(before, run.snapshot());
        finish(&mut run.worker);
    }
}

#[test]
fn revoked_original_listener_or_service_and_stopping_or_draining_worker_cannot_be_renewed() {
    for kind in 0..4 {
        let mut run = Running::new();
        let before = run.snapshot();
        match kind {
            0 => run.listener.revoke(),
            1 => run.grant.revoke(),
            2 => run.worker.stop(),
            _ => run.worker.drain(Duration::from_secs(1)).unwrap(),
        }
        assert!(run.renew(1, NEXT_EXPIRES, initial_budget()).is_err());
        same_snapshot(before, run.snapshot());
        finish(&mut run.worker);
    }
}

#[test]
fn frozen_clock_renewal_keeps_first_issuance_real_time_horizon() {
    let fixture = Fixture::new(true, 3000);
    let first_issuance = Instant::now();
    let binding = fixture.bind(2501, initial_budget());
    let mut run = fixture.start(binding);
    thread::sleep(Duration::from_millis(700));
    run.renew(1, 3001, initial_budget()).unwrap();
    // A renewed deadline based on now + duration would incorrectly last until >=3.7s.
    let target = first_issuance + Duration::from_millis(3150);
    if let Some(remaining) = target.checked_duration_since(Instant::now()) {
        thread::sleep(remaining);
    }
    assert!(run.worker.check_service(&run.grant).is_err());
    assert!(run.renew(2, 3001, budget(3, 8192)).is_err());
    finish(&mut run.worker);
}

#[test]
fn expired_ready_completion_cannot_be_restored_by_renewal() {
    let mut run = Running::new();
    let mut first = run.submit();
    ready(&mut first);
    let before = run.snapshot();
    run.tick.store(INITIAL_EXPIRES, Ordering::SeqCst);
    assert_eq!(first.poll(), Poll::Ready);
    run.tick.store(1, Ordering::SeqCst);
    assert!(run.renew(1, NEXT_EXPIRES, budget(4, 16_384)).is_err());
    let report = first.read(8192).unwrap().unwrap();
    assert!(report.task.execution.outcome.is_err());
    assert!(report.service_response.is_none());
    same_snapshot(before, run.snapshot());
    finish(&mut run.worker);
}

#[test]
fn renewal_does_not_extend_an_already_admitted_request_deadline() {
    let mut run = Running::new();
    let mut expired = run
        .worker
        .submit_service(
            request(),
            run.grant.clone(),
            Box::new(NoIo),
            Duration::from_nanos(1),
        )
        .unwrap();
    run.renew(1, NEXT_EXPIRES, budget(3, 16_384)).unwrap();
    ready(&mut expired);
    let report = expired.read(8192).unwrap().unwrap();
    assert_eq!(
        report.task.execution.outcome,
        Err(morrow_plugin_runtime::Fault::Deadline)
    );
    assert!(report.service_response.is_none());
    run.worker.check_service(&run.grant).unwrap();
    // The run is live; only the original per-request deadline has expired.
    let mut current = run.submit();
    ready(&mut current);
    assert!(
        current
            .read(8192)
            .unwrap()
            .unwrap()
            .service_response
            .unwrap()
            == reply()
    );
    assert_eq!(run.snapshot().usage.jobs, 2);
    finish(&mut run.worker);
}

#[test]
fn budgetless_service_run_does_not_gain_renewal_permission() {
    let fixture = Fixture::new(false, RUN_MS);
    let binding = fixture
        .manager
        .bind_service_run(
            &fixture.host,
            &fixture.instance,
            fixture.digest,
            fixture.manager.revision(),
            &caps(),
            INITIAL_EXPIRES,
            1,
        )
        .unwrap();
    let mut run = fixture.start(binding);
    assert!(run.worker.service_run_snapshot().is_none());
    assert!(run.renew(1, NEXT_EXPIRES, initial_budget()).is_err());
    run.worker.check_service(&run.grant).unwrap();
    finish(&mut run.worker);
}

#[test]
fn original_file_resource_survives_old_expiry_and_reaps_at_renewed_expiry() {
    use morrow_plugin_runtime::file_io::FileBroker;
    let fixture = Fixture::new(true, RUN_MS);
    let binding = fixture.bind(INITIAL_EXPIRES, initial_budget());
    let bytes = b"original selected immutable bytes".to_vec();
    let size = bytes.len() as u64;
    let mut broker = FileBroker::new([37; 32]);
    broker
        .grant_file(
            &fixture.manager,
            &fixture.host,
            &fixture.instance,
            &binding,
            bytes,
            1,
        )
        .unwrap();
    let mut run = fixture.start(binding);
    let before = run.snapshot();
    assert_eq!(before.usage.jobs, 1);
    assert_eq!(before.usage.bytes, size);
    run.renew(1, NEXT_EXPIRES, initial_budget()).unwrap();
    run.tick.store(INITIAL_EXPIRES + 1, Ordering::SeqCst);
    run.worker.check_service(&run.grant).unwrap();
    broker.reap(INITIAL_EXPIRES + 1);
    assert_eq!(broker.usage(), (1, size));
    assert_eq!(run.snapshot().usage, before.usage);
    run.tick.store(NEXT_EXPIRES, Ordering::SeqCst);
    assert!(run.worker.check_service(&run.grant).is_err());
    broker.reap(NEXT_EXPIRES);
    assert_eq!(broker.usage(), (0, 0));
    assert_eq!(run.snapshot().usage, before.usage);
    finish(&mut run.worker);
}
