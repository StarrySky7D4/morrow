//! Real registry approvals and Wasm service jobs. Routers are scripted: these
//! tests do not open a listener or perform network effects.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    io::{self as wire, Header},
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
    Fault, Limits,
    io_binding::{Error, IoBinding, Usage},
    io_jobs::{
        BrokerRouter, IoWorker, JobError, JobHandle, JobLimits, Poll, RouteContext, RouterFault,
    },
    manager::{ManagedInstance, Manager},
    service_io::{ListenerGrant, ServiceGrant},
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
const ID: &str = "org.example.service.jobs";
const SERVICE: &str = "notes";
const HANDLER: &str = "notes.echo";
const WAIT: Duration = Duration::from_secs(10);
use morrow_core::{service_authority as stored_authority, service_config};
use morrow_plugin_runtime::service_authority::{ConfiguredService, ResolvedService};

fn persistent_authority(f: &mut Fixture) -> (service_config::Config, stored_authority::Record) {
    let config = service_config::Config::encode(service_config::proto::Configuration {
        schema_version: 1,
        id: "stored-service".into(),
        revision: 1,
        namespace: vec![51; 32],
        retention_ms: 60_000,
        service: SERVICE.into(),
        handler: HANDLER.into(),
        package_sha256: f.digest.to_vec(),
        disabled: false,
        principals: vec![service_config::proto::Principal {
            id: "local-user".into(),
            authentication_reference: vec![52; 32],
            content_scopes: vec![],
        }],
        approval_references: vec![vec![53; 32]],
    })
    .unwrap();
    let auth = stored_authority::Record::encode(stored_authority::proto::Record {
        schema_version: 1,
        reference: vec![52; 32],
        revision: 1,
        created_ms: 1000,
        expires_ms: 60_000,
        disabled: false,
        kind: Some(stored_authority::proto::record::Kind::Authentication(
            stored_authority::proto::Authentication {
                principal_id: "local-user".into(),
                token_sha256: vec![54; 32],
            },
        )),
    })
    .unwrap();
    let publication = stored_authority::Record::encode(stored_authority::proto::Record {
        schema_version: 1,
        reference: vec![53; 32],
        revision: 1,
        created_ms: 1000,
        expires_ms: 90_000,
        disabled: false,
        kind: Some(stored_authority::proto::record::Kind::Publication(
            stored_authority::proto::Publication {
                config_id: "stored-service".into(),
                config_sha256: config.digest().to_vec(),
                listen_address: "127.0.0.1:0".into(),
                tls_required: false,
                method: "POST".into(),
                path: "/echo".into(),
                query_path: "/status".into(),
            },
        )),
    })
    .unwrap();
    let store = f.host.store_local_mut();
    store.save_service_config_local(&config, 0).unwrap();
    store.save_service_authority_local(&auth, 0).unwrap();
    store.save_service_authority_local(&publication, 0).unwrap();
    (config, auth)
}
fn configured(f: &mut Fixture, clock: Arc<AtomicU64>) -> ConfiguredService {
    ResolvedService::resolve(
        f.host.store_local_mut(),
        "stored-service",
        &[53; 32],
        move || clock.load(Ordering::SeqCst),
    )
    .unwrap()
    .issue(&f.manager, &f.host, &f.instance, &f.binding, 1)
    .unwrap()
}
#[test]
fn restored_authority_requires_original_store_and_current_config_digest() {
    let mut f = Fixture::new();
    let (config, _) = persistent_authority(&mut f);
    let resolved = ResolvedService::resolve(
        f.host.store_local_mut(),
        "stored-service",
        &[53; 32],
        || 2000,
    )
    .unwrap();
    let other = Fixture::new();
    assert!(
        resolved
            .issue(
                &other.manager,
                &other.host,
                &other.instance,
                &other.binding,
                1
            )
            .is_err()
    );
    assert!(
        ResolvedService::resolve(
            f.host.store_local_mut(),
            "stored-service",
            &[52; 32],
            || 2000
        )
        .is_err()
    );
    let mut changed = config.value().clone();
    changed.revision = 2;
    changed.retention_ms += 1;
    f.host
        .store_local_mut()
        .save_service_config_local(&service_config::Config::encode(changed).unwrap(), 1)
        .unwrap();
    assert!(
        ResolvedService::resolve(
            f.host.store_local_mut(),
            "stored-service",
            &[53; 32],
            || 2000
        )
        .is_err()
    );
}
#[test]
fn restored_authority_clock_regression_and_expiry_are_sticky() {
    for expired in [false, true] {
        let mut f = Fixture::new();
        persistent_authority(&mut f);
        let time = Arc::new(AtomicU64::new(2000));
        let service = configured(&mut f, time.clone());
        service.check().unwrap();
        time.store(if expired { 60_000 } else { 1999 }, Ordering::SeqCst);
        assert!(service.check().is_err());
        time.store(2000, Ordering::SeqCst);
        assert!(service.check().is_err());
        assert!(service.grant().check(1).is_err());
        assert!(service.listener().activate().is_err());
    }
}
#[test]
fn persisted_approval_never_bypasses_actual_registry_or_content_grants() {
    let mut f = Fixture::configured(
        0,
        false,
        true,
        BTreeSet::from([IoCapability::HttpListen]),
        4096,
    );
    persistent_authority(&mut f);
    let resolved = ResolvedService::resolve(
        f.host.store_local_mut(),
        "stored-service",
        &[53; 32],
        || 2000,
    )
    .unwrap();
    assert!(
        resolved
            .issue(&f.manager, &f.host, &f.instance, &f.binding, 1)
            .is_err()
    );
    let mut f = Fixture::new();
    let (config, _) = persistent_authority(&mut f);
    let mut config = config.value().clone();
    config.revision += 1;
    config.principals[0]
        .content_scopes
        .push(service_config::proto::ContentScope {
            kind: 7,
            card_id: "unapproved-card".into(),
            attachment_id: String::new(),
        });
    let config = service_config::Config::encode(config).unwrap();
    f.host
        .store_local_mut()
        .save_service_config_local(&config, 1)
        .unwrap();
    let mut publication = f
        .host
        .store_local()
        .load_service_authority(&[53; 32])
        .unwrap()
        .unwrap()
        .value()
        .clone();
    publication.revision += 1;
    if let Some(stored_authority::proto::record::Kind::Publication(ref mut value)) =
        publication.kind
    {
        value.config_sha256 = config.digest().to_vec();
    }
    f.host
        .store_local_mut()
        .save_service_authority_local(&stored_authority::Record::encode(publication).unwrap(), 1)
        .unwrap();
    let resolved = ResolvedService::resolve(
        f.host.store_local_mut(),
        "stored-service",
        &[53; 32],
        || 2000,
    )
    .unwrap();
    assert!(
        resolved
            .issue(&f.manager, &f.host, &f.instance, &f.binding, 1)
            .is_err()
    );
}
#[test]
fn original_worker_updates_revoke_ready_results_even_if_cas_fails() {
    use morrow_plugin_runtime::io_jobs::ServiceUpdate;
    for expected_revision in [0, 1] {
        let mut f = Fixture::new();
        let (_, auth) = persistent_authority(&mut f);
        let service = configured(&mut f, Arc::new(AtomicU64::new(2000)));
        let mut run = f.start();
        let mut job = run
            .worker
            .submit_service(request(), service.grant().clone(), Box::new(NoIo), WAIT)
            .unwrap();
        ready(&mut job);
        let mut updated = auth.value().clone();
        updated.revision = 2;
        updated.disabled = true;
        let mut ack = run
            .worker
            .update_service(ServiceUpdate::Authority {
                value: stored_authority::Record::encode(updated).unwrap(),
                expected_revision,
            })
            .unwrap();
        assert!(service.check().is_err());
        let report = job.read(8192).unwrap().unwrap();
        assert!(report.cancelled && report.service_response.is_none());
        let end = Instant::now() + WAIT;
        let result = loop {
            if let Some(result) = ack.read().unwrap() {
                break result;
            }
            assert!(Instant::now() < end);
            thread::sleep(Duration::from_millis(1));
        };
        if expected_revision == 0 {
            assert_eq!(result, Err(morrow_core::Error::RevisionConflict));
        } else {
            result.unwrap();
        }
        assert_eq!(ack.read().err(), Some(JobError::Consumed));
        finish(&mut run.worker);
    }
}
#[test]
fn outbound_admin_update_uses_original_store_and_revokes_inbound_publications() {
    use morrow_core::outbound_authority::{Record, WINDOWS_DPAPI_PROVIDER, proto};
    use morrow_plugin_runtime::io_jobs::ServiceUpdate;
    let mut f = Fixture::new();
    persistent_authority(&mut f);
    let service = configured(&mut f, Arc::new(AtomicU64::new(2000)));
    let database = f._dir.path().join("db");
    let record = Record::encode(proto::Record {
        schema_version: 1,
        reference: vec![61; 32],
        revision: 1,
        created_ms: 1000,
        expires_ms: 60_000,
        disabled: false,
        kind: Some(proto::record::Kind::Credential(proto::Credential {
            provider: WINDOWS_DPAPI_PROVIDER.into(),
            ciphertext: b"synthetic-opaque-provider-payload".to_vec(),
        })),
    })
    .unwrap();
    let mut run = f.start();
    let mut ack = run
        .worker
        .update_service(ServiceUpdate::Outbound {
            value: record.clone(),
            expected_revision: 0,
        })
        .unwrap();
    assert!(service.check().is_err());
    let end = Instant::now() + WAIT;
    loop {
        if let Some(result) = ack.read().unwrap() {
            result.unwrap();
            break;
        }
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(1));
    }
    finish(&mut run.worker);
    let store = Store::open(&database, EventBudget::default()).unwrap();
    assert_eq!(
        store
            .load_outbound_authority(&[61; 32])
            .unwrap()
            .unwrap()
            .container(),
        record.container()
    );
}
#[test]
fn dropping_original_store_invalidates_configured_clones_and_listener() {
    let mut f = Fixture::new();
    persistent_authority(&mut f);
    let service = configured(&mut f, Arc::new(AtomicU64::new(2000)));
    drop(f.host);
    assert!(service.clone().check().is_err());
    assert!(service.grant().check(1).is_err());
    assert!(service.listener().activate().is_err());
}
fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([
        IoCapability::HttpPublish,
        IoCapability::HttpListen,
        IoCapability::HttpRequest,
    ])
}
fn invocation() -> Invocation {
    Invocation {
        service: SERVICE.into(),
        handler: HANDLER.into(),
        principal: "local-user".into(),
        method: "POST".into(),
        target: "/echo".into(),
        headers: vec![],
        body: b"request".to_vec(),
    }
}
fn request() -> service::Request {
    service::Request::encode(1, &invocation()).unwrap()
}
fn reply() -> Reply {
    Reply {
        status: 201,
        headers: vec![Header {
            name: "x-result".into(),
            value: b"private".to_vec(),
        }],
        body: b"service-secret".to_vec(),
    }
}
fn literal(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("\\{byte:02x}")).collect()
}
fn outbound() -> wire::Request {
    wire::Request::encode_http_submit(
        1,
        &wire::HttpSubmission {
            operation_id: b"service-outbound".to_vec(),
            deadline_ms: 100,
            endpoint: b"trusted-endpoint".to_vec(),
            method: "POST".into(),
            relative_target: "/effect".into(),
            headers: vec![],
            body: vec![],
            credential: vec![],
        },
    )
    .unwrap()
}
fn module(calls: usize, wrong_response: bool) -> Vec<u8> {
    let bound =
        service::Request::encode(if wrong_response { 2 } else { 1 }, &invocation()).unwrap();
    let response = service::Response::encode(&bound, &reply()).unwrap();
    let io = outbound();
    let mut body = String::from("i32.const 0 i32.const 131072 call $read drop");
    for _ in 0..calls {
        body.push_str(&format!(
            " i32.const 131072 i32.const {} i32.const 262144 i32.const 131072 call $io drop",
            io.bytes().len()
        ));
    }
    body.push_str(&format!(
        " i32.const 393216 i32.const {} call $done drop i32.const 0",
        response.len()
    ));
    wat::parse_str(format!(
        r#"(module
 (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
 (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
 (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
 (memory (export "memory") 8)
 (data (i32.const 131072) "{}") (data (i32.const 393216) "{}")
 (func (export "morrow_run") (result i32) {body}))"#,
        literal(io.bytes()),
        literal(&response)
    ))
    .unwrap()
}
struct Fixture {
    _dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    binding: IoBinding,
    observer: IoBinding,
    digest: [u8; 32],
    limit: u64,
}
impl Fixture {
    fn new() -> Self {
        Self::configured(0, false, true, caps(), 4096)
    }
    fn configured(
        calls: usize,
        wrong: bool,
        service_schema: bool,
        approved: BTreeSet<IoCapability>,
        limit: u64,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = module(calls, wrong);
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration = io::declaration(caps().into_iter().collect(), vec![HANDLER.into()]);
        if service_schema {
            declaration.service_schema_sha256 = service::schema_digest().to_vec();
        }
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_jobs = 2;
        budget.max_resources = 4;
        budget.max_job_bytes = limit;
        budget.max_bytes = limit * 4;
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, digest, approved.clone(), manager.revision())
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
                &approved,
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
                &approved,
                100,
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
            limit,
        }
    }
    fn grant(&self) -> ServiceGrant {
        ServiceGrant::issue(
            &self.manager,
            &self.host,
            &self.instance,
            &self.binding,
            SERVICE,
            HANDLER,
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
            limit,
        } = self;
        let time = Arc::new(AtomicU64::new(1));
        let ticks = time.clone();
        let worker = IoWorker::spawn_managed(
            &manager,
            host,
            instance,
            binding,
            move || ticks.load(Ordering::SeqCst),
            2,
            JobLimits::new(2, limit, limit * 4).unwrap(),
        )
        .unwrap();
        Running {
            _dir,
            manager,
            worker,
            observer,
            time,
            digest,
        }
    }
}
struct Running {
    _dir: tempfile::TempDir,
    manager: Manager,
    worker: IoWorker,
    observer: IoBinding,
    time: Arc<AtomicU64>,
    digest: [u8; 32],
}
struct NoIo;
impl BrokerRouter for NoIo {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &wire::Request,
    ) -> Result<Vec<u8>, RouterFault> {
        panic!("zero-IO service must not route IO")
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
            p => panic!("unexpected {p:?}"),
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
fn zero_io_service_returns_exact_typed_completion_and_charges_complete_frame() {
    let fixture = Fixture::new();
    let grant = fixture.grant();
    let mut run = fixture.start();
    let before = run.observer.usage();
    run.worker.check_service(&grant).unwrap();
    assert_eq!(run.observer.usage(), before);
    let cost = (request().bytes().len()
        + service::Response::encode(&request(), &reply())
            .unwrap()
            .len()) as u64;
    let mut job = run
        .worker
        .submit_service(request(), grant.clone(), Box::new(NoIo), WAIT)
        .unwrap();
    ready(&mut job);
    assert_eq!(
        job.read(reply().body.len()).err(),
        Some(JobError::ReadBound)
    );
    assert_eq!(run.observer.usage().jobs, 1);
    let report = job.read(1024).unwrap().unwrap();
    assert_eq!(report.task.execution.outcome, Ok(0));
    assert_eq!(report.calls, 0);
    assert!(!report.unknown);
    assert_eq!(report.bytes, cost);
    assert!(report.response.is_none() && report.http_response.is_none());
    let actual = report.service_response.unwrap();
    assert!(actual == reply());
    assert_eq!(
        run.observer.usage(),
        Usage {
            jobs: 0,
            resources: 1,
            bytes: cost
        }
    );
    finish(&mut run.worker);
}
#[test]
fn publish_approval_schema_and_registered_handler_are_all_required() {
    for (approved, schema, handler) in [
        (BTreeSet::from([IoCapability::HttpListen]), true, HANDLER),
        (caps(), false, HANDLER),
        (caps(), true, "missing.handler"),
    ] {
        let f = Fixture::configured(0, false, schema, approved, 4096);
        let before = f.binding.usage();
        assert_eq!(
            ServiceGrant::issue(
                &f.manager,
                &f.host,
                &f.instance,
                &f.binding,
                SERVICE,
                handler,
                1
            )
            .err(),
            Some(Error::Denied)
        );
        assert_eq!(f.binding.usage(), before);
    }
    let f = Fixture::configured(
        0,
        false,
        true,
        BTreeSet::from([IoCapability::HttpPublish]),
        4096,
    );
    assert_eq!(
        ListenerGrant::issue(&f.manager, &f.host, &f.instance, &f.binding, 1).err(),
        Some(Error::Denied)
    );
    assert_eq!(f.binding.usage(), Usage::default());
}
#[test]
fn listener_claim_is_single_use_across_clones_and_resources_release_only_at_last_drop() {
    let f = Fixture::new();
    let grant = ListenerGrant::issue(&f.manager, &f.host, &f.instance, &f.binding, 1).unwrap();
    let clone = grant.clone();
    assert_eq!(
        f.binding.usage(),
        Usage {
            jobs: 0,
            resources: 1,
            bytes: 0
        }
    );
    grant.check(1).unwrap();
    grant.activate().unwrap();
    assert_eq!(clone.activate(), Err(Error::Denied));
    grant.revoke();
    assert_eq!(clone.check(2), Err(Error::Denied));
    drop(grant);
    assert_eq!(f.binding.usage().resources, 1);
    drop(clone);
    assert_eq!(f.binding.usage(), Usage::default());
}
#[test]
fn wrong_actual_instance_and_request_route_are_rejected_without_job_or_byte_admission() {
    let mut f = Fixture::new();
    let wrong = f.grant();
    let original = f.instance;
    f.instance = f.manager.connect(ID, &mut f.host).unwrap();
    f.binding = f
        .manager
        .bind_io(
            &f.host,
            &f.instance,
            f.digest,
            f.manager.revision(),
            &caps(),
            100,
            1,
        )
        .unwrap();
    f.observer = f
        .manager
        .bind_io(
            &f.host,
            &f.instance,
            f.digest,
            f.manager.revision(),
            &caps(),
            100,
            1,
        )
        .unwrap();
    let valid = f.grant();
    let mut run = f.start();
    let before = run.observer.usage();
    assert_eq!(
        run.worker.check_service(&wrong),
        Err(JobError::InvalidOptions)
    );
    assert!(matches!(
        run.worker
            .submit_service(request(), wrong, Box::new(NoIo), WAIT),
        Err(JobError::InvalidOptions)
    ));
    for field in ["service", "handler"] {
        let mut inv = invocation();
        if field == "service" {
            inv.service = "other".into();
        } else {
            inv.handler = "other".into();
        }
        assert!(matches!(
            run.worker.submit_service(
                service::Request::encode(1, &inv).unwrap(),
                valid.clone(),
                Box::new(NoIo),
                WAIT
            ),
            Err(JobError::InvalidOptions)
        ));
    }
    assert_eq!(run.observer.usage(), before);
    finish(&mut run.worker);
    drop(original);
}
#[test]
fn ready_service_delivery_checks_service_manager_revocation_and_short_grant_expiry() {
    for mode in 0..3 {
        let f = Fixture::new();
        let short = f
            .manager
            .bind_io(
                &f.host,
                &f.instance,
                f.digest,
                f.manager.revision(),
                &caps(),
                40,
                1,
            )
            .unwrap();
        let grant = ServiceGrant::issue(
            &f.manager,
            &f.host,
            &f.instance,
            &short,
            SERVICE,
            HANDLER,
            1,
        )
        .unwrap();
        let mut run = f.start();
        let mut job = run
            .worker
            .submit_service(request(), grant.clone(), Box::new(NoIo), WAIT)
            .unwrap();
        ready(&mut job);
        match mode {
            0 => grant.revoke(),
            1 => run
                .manager
                .approve_io(ID, run.digest, BTreeSet::new(), run.manager.revision())
                .unwrap(),
            _ => run.time.store(40, Ordering::SeqCst),
        }
        let report = job.read(0).unwrap().unwrap();
        assert!(report.cancelled && report.task.execution.outcome.is_err());
        assert!(report.service_response.is_none());
        assert_eq!(report.payload_bytes(), 0);
        assert_eq!(run.observer.usage().jobs, 0);
        finish(&mut run.worker);
    }
}
#[test]
fn service_completion_budget_and_wrong_request_binding_never_deliver_success() {
    let response_len = service::Response::encode(&request(), &reply())
        .unwrap()
        .len();
    for wrong in [false, true] {
        let limit = if wrong {
            4096
        } else {
            (request().bytes().len() + response_len - 1) as u64
        };
        let f = Fixture::configured(0, wrong, true, caps(), limit);
        let grant = f.grant();
        let mut run = f.start();
        let mut job = run
            .worker
            .submit_service(request(), grant, Box::new(NoIo), WAIT)
            .unwrap();
        ready(&mut job);
        let report = job.read(0).unwrap().unwrap();
        assert_eq!(
            report.task.execution.outcome,
            Err(if wrong {
                Fault::TaskProtocol
            } else {
                Fault::Limits
            })
        );
        assert!(report.service_response.is_none());
        assert!(!report.unknown);
        assert_eq!(report.calls, 0);
        if !wrong {
            assert_eq!(report.bytes, request().bytes().len() as u64);
        }
        assert_eq!(run.observer.usage().jobs, 0);
        finish(&mut run.worker);
    }
}
struct BlockingRouter {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    calls: Arc<AtomicUsize>,
}
impl BrokerRouter for BlockingRouter {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &wire::Request,
    ) -> Result<Vec<u8>, RouterFault> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.send(()).unwrap();
        self.release.recv_timeout(WAIT).unwrap();
        Err(RouterFault::Denied)
    }
}
#[test]
fn service_revocation_during_running_router_cancels_before_a_second_guest_import() {
    let f = Fixture::configured(2, false, true, caps(), 4096);
    let grant = f.grant();
    let mut run = f.start();
    let (entered, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut job = run
        .worker
        .submit_service(
            request(),
            grant.clone(),
            Box::new(BlockingRouter {
                entered,
                release: gate,
                calls: calls.clone(),
            }),
            WAIT,
        )
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    grant.revoke();
    assert_eq!(run.observer.usage().jobs, 1);
    release.send(()).unwrap();
    ready(&mut job);
    let report = job.read(0).unwrap().unwrap();
    assert!(report.cancelled && report.task.execution.outcome.is_err());
    assert!(report.service_response.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.observer.usage().jobs, 0);
    finish(&mut run.worker);
}

#[test]
fn listener_bound_ready_result_is_suppressed_without_a_monitor_tick() {
    let f = Fixture::new();
    let original = f.grant();
    let listener = ListenerGrant::issue(&f.manager, &f.host, &f.instance, &f.binding, 1).unwrap();
    let before = f.binding.usage();
    let bound = original.bound_to_listener(&listener).unwrap();
    assert_eq!(f.binding.usage(), before);
    assert_eq!(
        bound.bound_to_listener(&listener).err(),
        Some(Error::Denied)
    );
    let mut run = f.start();
    run.worker.check_listener(&listener).unwrap();
    let mut job = run
        .worker
        .submit_service(request(), bound, Box::new(NoIo), WAIT)
        .unwrap();
    ready(&mut job);
    listener.revoke();
    // No native monitor runs between revoke and read: the slot itself retains
    // and validates the exact listener grant.
    let report = job.read(0).unwrap().unwrap();
    assert!(report.cancelled && report.task.execution.outcome.is_err());
    assert_eq!(report.payload_bytes(), 0);
    assert!(report.service_response.is_none());
    assert_eq!(run.observer.usage().jobs, 0);
    original.check(1).unwrap(); // listener revocation does not revoke sibling services
    finish(&mut run.worker);
}
#[test]
fn listener_binding_and_worker_monitor_reject_another_live_instance() {
    let mut f = Fixture::new();
    let listener = ListenerGrant::issue(&f.manager, &f.host, &f.instance, &f.binding, 1).unwrap();
    let original = f.instance;
    f.instance = f.manager.connect(ID, &mut f.host).unwrap();
    f.binding = f
        .manager
        .bind_io(
            &f.host,
            &f.instance,
            f.digest,
            f.manager.revision(),
            &caps(),
            100,
            1,
        )
        .unwrap();
    f.observer = f
        .manager
        .bind_io(
            &f.host,
            &f.instance,
            f.digest,
            f.manager.revision(),
            &caps(),
            100,
            1,
        )
        .unwrap();
    let service = f.grant();
    let before = f.binding.usage();
    assert_eq!(
        service.bound_to_listener(&listener).err(),
        Some(Error::Denied)
    );
    assert_eq!(f.binding.usage(), before);
    let mut run = f.start();
    assert_eq!(
        run.worker.check_listener(&listener),
        Err(JobError::InvalidOptions)
    );
    assert_eq!(run.observer.usage(), before);
    listener.check(1).unwrap();
    finish(&mut run.worker);
    drop(original);
}
#[test]
fn listener_revocation_cancels_a_bound_service_inside_the_actual_router() {
    let f = Fixture::configured(2, false, true, caps(), 4096);
    let listener = ListenerGrant::issue(&f.manager, &f.host, &f.instance, &f.binding, 1).unwrap();
    let bound = f.grant().bound_to_listener(&listener).unwrap();
    let mut run = f.start();
    let (entered, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut job = run
        .worker
        .submit_service(
            request(),
            bound,
            Box::new(BlockingRouter {
                entered,
                release: gate,
                calls: calls.clone(),
            }),
            WAIT,
        )
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    listener.revoke();
    assert_eq!(run.observer.usage().jobs, 1);
    release.send(()).unwrap();
    ready(&mut job);
    let report = job.read(0).unwrap().unwrap();
    assert!(report.cancelled && report.task.execution.outcome.is_err());
    assert!(report.service_response.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.observer.usage().jobs, 0);
    finish(&mut run.worker);
}

fn desired_config(package_sha256: [u8; 32]) -> morrow_core::service_config::Config {
    use morrow_core::service_config::{Config, VERSION, proto::Configuration};
    Config::encode(Configuration {
        schema_version: VERSION,
        id: "notes-published".into(),
        revision: 1,
        namespace: vec![71; 32],
        retention_ms: 12345,
        service: SERVICE.into(),
        handler: HANDLER.into(),
        package_sha256: package_sha256.to_vec(),
        disabled: false,
        principals: vec![],
        approval_references: vec![vec![73; 32]],
    })
    .unwrap()
}
#[test]
fn journal_config_checks_actual_package_service_handler_disabled_and_shared_revocation() {
    use morrow_core::service_config::Config;
    use morrow_plugin_runtime::{io_execution, service_history::ServiceJournal};
    let fixture = Fixture::new();
    let config = desired_config(fixture.digest);
    let grant = fixture.grant();
    let listener = ListenerGrant::issue(
        &fixture.manager,
        &fixture.host,
        &fixture.instance,
        &fixture.binding,
        1,
    )
    .unwrap();
    let bound = grant.bound_to_listener(&listener).unwrap();
    for actual in [&grant, &bound] {
        let journal = ServiceJournal::from_config(&config, actual, || 100).unwrap();
        assert_eq!(journal.policy().namespace, [71; 32]);
        assert_eq!(journal.policy().retention_ms, 12345);
        for changed_field in 0..4 {
            let mut changed = config.value().clone();
            match changed_field {
                0 => changed.disabled = true,
                1 => changed.package_sha256 = vec![72; 32],
                2 => changed.service = "other-service".into(),
                3 => changed.handler = "other-handler".into(),
                _ => unreachable!(),
            }
            let changed = Config::encode(changed).unwrap();
            assert_eq!(actual.validate_config(&changed), Err(Error::Denied));
            assert_eq!(
                ServiceJournal::from_config(&changed, actual, || 100).err(),
                Some(io_execution::Error::Denied)
            );
        }
    }
    // Binding preserves the actual package and also the original listener's
    // revocation. The unbound service remains independently valid.
    listener.revoke();
    assert_eq!(bound.validate_config(&config), Err(Error::Denied));
    assert!(ServiceJournal::from_config(&config, &grant, || 100).is_ok());
    grant.revoke();
    assert_eq!(
        ServiceJournal::from_config(&config, &grant, || 100).err(),
        Some(io_execution::Error::Denied)
    );
}
#[test]
fn persisted_config_reopen_uses_original_store_policy_with_fresh_actual_instance_grant() {
    use morrow_plugin_runtime::service_history::ServiceJournal;
    let (dir, digest, expected) = {
        let mut fixture = Fixture::new();
        let config = desired_config(fixture.digest);
        fixture
            .host
            .store_local_mut()
            .save_service_config_local(&config, 0)
            .unwrap();
        let old_grant = fixture.grant();
        old_grant.revoke();
        // Keep the original profile directory while every old live owner and
        // approval object is dropped at this scope boundary.
        (fixture._dir, fixture.digest, config.container().to_vec())
    };
    let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
    let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
    let mut manager = Manager::new(registry, Limits::default());
    let mut host = HostRuntime::new(
        Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap(),
    )
    .unwrap();
    let config = host
        .store_local()
        .load_service_config("notes-published")
        .unwrap()
        .unwrap();
    assert_eq!(config.container(), expected);
    let instance = manager.connect(ID, &mut host).unwrap();
    let binding = manager
        .bind_io(
            &host,
            &instance,
            digest,
            manager.revision(),
            &caps(),
            100,
            1,
        )
        .unwrap();
    let grant =
        ServiceGrant::issue(&manager, &host, &instance, &binding, SERVICE, HANDLER, 1).unwrap();
    let before = binding.usage();
    let journal = ServiceJournal::from_config(&config, &grant, || 100).unwrap();
    assert_eq!(journal.policy().namespace, [71; 32]);
    assert_eq!(journal.policy().retention_ms, 12345);
    assert_eq!(binding.usage(), before);
    // Constructing a journal does not create another service/listener resource.
    assert_eq!(binding.usage().resources, 1);
    host.store_local().integrity_check().unwrap();
}
