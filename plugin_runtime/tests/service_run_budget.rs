//! Host-selected lifetime quotas use the original managed instance ledger.
//! The Wasm tests execute real guest completions with no external network effects.
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
    io_binding::{Error, IoBinding, ServiceRunBudget},
    io_jobs::{BrokerRouter, IoWorker, JobHandle, JobLimits, Poll, RouteContext, RouterFault},
    manager::{ManagedInstance, Manager},
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{
    collections::BTreeSet,
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.service.run.budget";
const RUN_MS: u64 = 60_000;
const WAIT: Duration = Duration::from_secs(10);
const MAX_BYTES: u64 = 65_536;

fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([
        IoCapability::HttpListen,
        IoCapability::HttpPublish,
        IoCapability::HttpRequest,
    ])
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
        body: b"run-budget-reply".to_vec(),
    }
}
fn literal(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("\\{byte:02x}")).collect()
}
fn package(budgeted: bool) -> Package {
    let response = service::Response::encode(&request(), &reply()).unwrap();
    let wasm = wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
        (memory (export "memory") 3)
        (data (i32.const 131072) "{}")
        (func (export "morrow_run") (result i32)
            i32.const 0 i32.const 131072 call $read drop
            i32.const 131072 i32.const {} call $complete drop i32.const 0))"#,
        literal(&response),
        response.len()
    ))
    .unwrap();
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    let mut declaration = io::declaration(caps().into_iter().collect(), vec!["echo.call".into()]);
    declaration.service_schema_sha256 = service::schema_digest().to_vec();
    let budget = declaration.budget.as_mut().unwrap();
    budget.max_jobs = 2;
    budget.max_resources = 4;
    budget.max_job_bytes = 8192;
    budget.max_bytes = MAX_BYTES;
    declaration.service_run = Some(io::proto::ServiceRunProfile {
        schema_version: io::SERVICE_RUN_VERSION,
        max_duration_ms: RUN_MS,
        budget: budgeted.then_some(io::proto::ServiceRunBudget {
            schema_version: 1,
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
    fn new(budgeted: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let package = package(budgeted);
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
    fn bind(&self, jobs: u64, bytes: u64) -> IoBinding {
        self.manager
            .bind_budgeted_service_run(
                &self.host,
                &self.instance,
                self.digest,
                self.manager.revision(),
                &caps(),
                RUN_MS + 1,
                1,
                ServiceRunBudget {
                    max_jobs: jobs,
                    max_bytes: bytes,
                },
            )
            .unwrap()
    }
    fn start(self, binding: IoBinding) -> Running {
        let grant = ServiceGrant::issue(
            &self.manager,
            &self.host,
            &self.instance,
            &binding,
            "echo",
            "echo.call",
            1,
        )
        .unwrap();
        let worker = IoWorker::spawn_managed(
            &self.manager,
            self.host,
            self.instance,
            binding,
            || 1,
            2,
            JobLimits::new(2, 8192, MAX_BYTES).unwrap(),
        )
        .unwrap();
        Running {
            _dir: self.dir,
            _manager: self.manager,
            worker,
            grant,
        }
    }
}
struct Running {
    _dir: tempfile::TempDir,
    _manager: Manager,
    worker: IoWorker,
    grant: ServiceGrant,
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
fn invalid_budget_host_revision_and_old_api_preserve_first_issuance() {
    let mut f = Fixture::new(true);
    let revision = f.manager.revision();
    let other =
        HostRuntime::new(Store::open(&f.dir.path().join("other"), EventBudget::default()).unwrap())
            .unwrap();
    let good = ServiceRunBudget {
        max_jobs: 2,
        max_bytes: 100,
    };
    for (host, digest, rev, budget) in [
        (&other, f.digest, revision, good),
        (&f.host, [9; 32], revision, good),
        (&f.host, f.digest, revision - 1, good),
        (
            &f.host,
            f.digest,
            revision,
            ServiceRunBudget {
                max_jobs: 0,
                max_bytes: 100,
            },
        ),
        (
            &f.host,
            f.digest,
            revision,
            ServiceRunBudget {
                max_jobs: 65,
                max_bytes: 100,
            },
        ),
        (
            &f.host,
            f.digest,
            revision,
            ServiceRunBudget {
                max_jobs: 2,
                max_bytes: 0,
            },
        ),
        (
            &f.host,
            f.digest,
            revision,
            ServiceRunBudget {
                max_jobs: 2,
                max_bytes: MAX_BYTES + 1,
            },
        ),
    ] {
        assert!(
            f.manager
                .bind_budgeted_service_run(
                    host,
                    &f.instance,
                    digest,
                    rev,
                    &caps(),
                    RUN_MS + 1,
                    1,
                    budget
                )
                .is_err()
        );
    }
    assert!(
        f.manager
            .bind_service_run(
                &f.host,
                &f.instance,
                f.digest,
                revision,
                &caps(),
                RUN_MS + 1,
                1
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_io(&f.host, &f.instance, f.digest, revision, &caps(), 100, 1)
            .is_err()
    );
    let binding = f.bind(2, 100);
    binding.check(&f.manager, &f.host, &f.instance, 1).unwrap();
    let usage = binding.service_run_usage().unwrap();
    assert_eq!((usage.jobs, usage.bytes), (0, 0));
    assert!(
        f.manager
            .bind_budgeted_service_run(
                &f.host,
                &f.instance,
                f.digest,
                revision,
                &caps(),
                RUN_MS + 1,
                1,
                good
            )
            .is_err()
    );
    f.instance.close(&mut f.host).unwrap();
}

#[test]
fn budget_entry_rejects_unbudgeted_profile_without_consuming_legacy_issuance() {
    let mut f = Fixture::new(false);
    assert!(
        f.manager
            .bind_budgeted_service_run(
                &f.host,
                &f.instance,
                f.digest,
                f.manager.revision(),
                &caps(),
                RUN_MS + 1,
                1,
                ServiceRunBudget {
                    max_jobs: 1,
                    max_bytes: 100
                }
            )
            .is_err()
    );
    let binding = f
        .manager
        .bind_service_run(
            &f.host,
            &f.instance,
            f.digest,
            f.manager.revision(),
            &caps(),
            RUN_MS + 1,
            1,
        )
        .unwrap();
    let usage = binding.service_run_usage().unwrap();
    assert_eq!((usage.jobs, usage.bytes), (0, 0));
    f.instance.close(&mut f.host).unwrap();
}

#[test]
fn zero_byte_jobs_are_cumulative_and_drop_does_not_refund_or_invalidate_live_lease() {
    let mut f = Fixture::new(true);
    let binding = f.bind(1, 100);
    let listener = ListenerGrant::issue(&f.manager, &f.host, &f.instance, &binding, 1).unwrap();
    assert_eq!(binding.service_run_usage().unwrap().jobs, 0);
    let lease = binding
        .admit(
            &f.manager,
            &f.host,
            &f.instance,
            IoCapability::HttpRequest,
            0,
            0,
            1,
        )
        .unwrap();
    assert!(matches!(
        binding.admit(
            &f.manager,
            &f.host,
            &f.instance,
            IoCapability::HttpRequest,
            0,
            0,
            1
        ),
        Err(Error::Limit)
    ));
    lease.check(&f.manager, &f.host, &f.instance, 1).unwrap();
    listener.check(1).unwrap();
    drop(lease);
    assert_eq!(binding.usage().jobs, 0);
    let usage = binding.service_run_usage().unwrap();
    assert_eq!((usage.jobs, usage.bytes), (1, 0));
    assert!(matches!(
        binding.admit(
            &f.manager,
            &f.host,
            &f.instance,
            IoCapability::HttpRequest,
            0,
            0,
            1
        ),
        Err(Error::Limit)
    ));
    f.instance.close(&mut f.host).unwrap();
}

#[test]
fn sequential_direct_admission_enforces_host_bytes_without_charging_rejected_jobs() {
    let mut f = Fixture::new(true);
    let binding = f.bind(3, 10);
    drop(
        binding
            .admit(
                &f.manager,
                &f.host,
                &f.instance,
                IoCapability::HttpRequest,
                0,
                6,
                1,
            )
            .unwrap(),
    );
    assert!(matches!(
        binding.admit(
            &f.manager,
            &f.host,
            &f.instance,
            IoCapability::HttpRequest,
            0,
            5,
            1
        ),
        Err(Error::Limit)
    ));
    let usage = binding.service_run_usage().unwrap();
    assert_eq!((usage.jobs, usage.bytes), (1, 6));
    drop(
        binding
            .admit(
                &f.manager,
                &f.host,
                &f.instance,
                IoCapability::HttpRequest,
                0,
                4,
                1,
            )
            .unwrap(),
    );
    assert!(matches!(
        binding.admit(
            &f.manager,
            &f.host,
            &f.instance,
            IoCapability::HttpRequest,
            0,
            1,
            1
        ),
        Err(Error::Limit)
    ));
    let usage = binding.service_run_usage().unwrap();
    assert_eq!((usage.jobs, usage.bytes), (2, 10));
    assert_eq!(binding.usage().bytes, 10);
    f.instance.close(&mut f.host).unwrap();
}

#[test]
fn final_allowed_managed_job_keeps_its_ready_completion_after_next_job_is_denied() {
    let f = Fixture::new(true);
    let binding = f.bind(1, 8192);
    let mut run = f.start(binding);
    let mut first = run
        .worker
        .submit_service(request(), run.grant.clone(), Box::new(NoIo), WAIT)
        .unwrap();
    ready(&mut first);
    assert_eq!(run.worker.service_run_usage().unwrap().jobs, 1);
    assert!(
        run.worker
            .submit_service(request(), run.grant.clone(), Box::new(NoIo), WAIT)
            .is_err()
    );
    run.worker.check_service(&run.grant).unwrap();
    let report = first.read(8192).unwrap().unwrap();
    assert_eq!(report.task.execution.outcome, Ok(0));
    assert!(report.service_response.unwrap() == reply());
    assert_eq!(
        report.bytes,
        (request().bytes().len()
            + service::Response::encode(&request(), &reply())
                .unwrap()
                .len()) as u64
    );
    assert!(
        run.worker
            .submit_service(request(), run.grant.clone(), Box::new(NoIo), WAIT)
            .is_err()
    );
    finish(&mut run.worker);
}

#[test]
fn completion_cannot_exceed_host_run_bytes_even_when_declared_and_job_budgets_allow_it() {
    let input_bytes = request().bytes().len() as u64;
    let complete_bytes = service::Response::encode(&request(), &reply())
        .unwrap()
        .len() as u64;
    let f = Fixture::new(true);
    let binding = f.bind(2, input_bytes + complete_bytes - 1);
    let mut run = f.start(binding);
    let mut first = run
        .worker
        .submit_service(request(), run.grant.clone(), Box::new(NoIo), WAIT)
        .unwrap();
    ready(&mut first);
    let report = first.read(8192).unwrap().unwrap();
    assert!(report.service_response.is_none());
    assert_eq!(
        report.task.execution.outcome,
        Err(morrow_plugin_runtime::Fault::Limits)
    );
    assert_eq!(report.bytes, input_bytes);
    let usage = run.worker.service_run_usage().unwrap();
    assert_eq!((usage.jobs, usage.bytes), (1, input_bytes));
    finish(&mut run.worker);
}

#[test]
fn cancelling_or_dropping_ready_managed_job_never_refunds_run_admission() {
    for cancel in [false, true] {
        let f = Fixture::new(true);
        let binding = f.bind(1, 8192);
        let mut run = f.start(binding);
        let mut first = run
            .worker
            .submit_service(request(), run.grant.clone(), Box::new(NoIo), WAIT)
            .unwrap();
        ready(&mut first);
        if cancel {
            first.cancel();
        }
        drop(first);
        assert_eq!(run.worker.service_run_usage().unwrap().jobs, 1);
        assert!(
            run.worker
                .submit_service(request(), run.grant.clone(), Box::new(NoIo), WAIT)
                .is_err()
        );
        finish(&mut run.worker);
    }
}
