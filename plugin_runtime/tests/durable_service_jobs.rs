//! Actual managed Wasm plus durable Store history. The backend is scripted;
//! these tests do not claim a real socket, remote effect or content commit.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    io::{self as wire, Header},
    io_intent::{Command, Phase},
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    service::{self, Invocation, Reply},
    service_record::{self, Policy, RequestRecord},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Limits,
    io_binding::IoBinding,
    io_execution,
    io_jobs::{
        BrokerRouter, IoWorker, JobHandle, JobLimits, Poll, RouteContext, RouterFault,
        ServicePersistenceStatus,
    },
    manager::Manager,
    service_content::{
        ContentScope, SCOPE_HEADER, ServiceContentAccess, ServiceContentPolicy, scope_digest,
    },
    service_history::ServiceJournal,
    service_io::ServiceGrant,
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
const ID: &str = "org.example.durable-service";
const KEY: &str = "same-inbound-operation";
const CREATED: u64 = 1000;
const RETENTION: u64 = 60_000;
const WAIT: Duration = Duration::from_secs(10);
fn policy() -> Policy {
    Policy {
        namespace: [3; 32],
        retention_ms: RETENTION,
    }
}
fn request() -> service::Request {
    let invocation = Invocation {
        service: "notes".into(),
        handler: "notes.echo".into(),
        principal: "alice".into(),
        method: "POST".into(),
        target: "/notes".into(),
        headers: vec![],
        body: b"input".to_vec(),
    };
    let call = service_record::call_id(&policy(), KEY, &invocation).unwrap();
    service::Request::encode(call, &invocation).unwrap()
}
fn completion() -> Vec<u8> {
    completion_for(&request())
}
fn completion_for(request: &service::Request) -> Vec<u8> {
    service::Response::encode(
        request,
        &Reply {
            status: 201,
            headers: vec![Header {
                name: "x-result".into(),
                value: b"protected".to_vec(),
            }],
            body: b"durable-secret".to_vec(),
        },
    )
    .unwrap()
}
fn outbound() -> wire::Request {
    wire::Request::encode_http_submit(
        1,
        &wire::HttpSubmission {
            operation_id: b"outbound-child".to_vec(),
            endpoint: b"trusted-script-endpoint".to_vec(),
            deadline_ms: 100,
            method: "POST".into(),
            relative_target: "/effect".into(),
            headers: vec![],
            body: vec![],
            credential: vec![],
        },
    )
    .unwrap()
}
fn literal(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
fn module(io_call: bool, request: &service::Request) -> Vec<u8> {
    let complete = completion_for(request);
    let outbound = outbound();
    let call = if io_call {
        format!(
            "i32.const 131072 i32.const {} i32.const 262144 i32.const 131072 call $io drop",
            outbound.bytes().len()
        )
    } else {
        String::new()
    };
    wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
        (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 8)
        (data (i32.const 131072) "{}") (data (i32.const 393216) "{}")
        (func (export "morrow_run") (result i32)
          i32.const 0 i32.const 131072 call $read drop
          {call}
          i32.const 393216 i32.const {} call $complete drop i32.const 0))"#,
        literal(outbound.bytes()),
        literal(&complete),
        complete.len()
    ))
    .unwrap()
}
struct Running {
    _dir: tempfile::TempDir,
    _manager: Manager,
    worker: IoWorker,
    binding: IoBinding,
    grant: ServiceGrant,
    journal: ServiceJournal,
    utc: Arc<AtomicU64>,
    digest: [u8; 32],
    fuel: u64,
    request: service::Request,
    access: Option<ServiceContentAccess>,
    content_policy: Option<ServiceContentPolicy>,
    principal_active: Arc<AtomicBool>,
    ticks: Arc<AtomicU64>,
}
impl Running {
    fn new(io_call: bool) -> Self {
        Self::new_with_content(io_call, false)
    }
    fn new_with_content(io_call: bool, content: bool) -> Self {
        let scopes = vec![ContentScope {
            kind: GrantKind::ReadContent,
            card_id: "card.allowed".into(),
            attachment_id: None,
        }];
        let mut request = request();
        if content {
            let mut invocation = request.invocation().clone();
            invocation.headers.push(Header {
                name: SCOPE_HEADER.into(),
                value: scope_digest(&scopes)
                    .unwrap()
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>()
                    .into_bytes(),
            });
            request = service::Request::encode(request.call_id(), &invocation).unwrap();
        }

        let dir = tempfile::tempdir().unwrap();
        let wasm = module(io_call, &request);
        let caps = BTreeSet::from([IoCapability::HttpPublish, IoCapability::HttpRequest]);
        let mut manifest = Package::manifest_for_task(
            ID,
            "1.0.0",
            &wasm,
            if content {
                vec![morrow_core::plugin_package::proto::Capability::ReadContent]
            } else {
                vec![]
            },
        );
        let mut declaration =
            io::declaration(caps.iter().copied().collect(), vec!["notes.echo".into()]);
        declaration.service_schema_sha256 = service::schema_digest().to_vec();
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_jobs = 2;
        budget.max_resources = 4;
        budget.max_job_bytes = 32_768;
        budget.max_bytes = 1_048_576;
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&dir.path().join("selection"), catalog).unwrap(),
            Limits::default(),
        );
        manager.select(&package, manager.revision()).unwrap();
        if content {
            manager
                .approve(
                    ID,
                    digest,
                    BTreeSet::from([GrantKind::ReadContent]),
                    manager.revision(),
                )
                .unwrap();
        }
        manager
            .approve_io(ID, digest, caps.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, digest, true, manager.revision())
            .unwrap();
        let mut host = HostRuntime::new(
            Store::open(&dir.path().join("history.db"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        let mut instance = manager.connect(ID, &mut host).unwrap();
        if content {
            host.grant(
                instance.parts_mut().1,
                GrantKind::ReadContent,
                "card.allowed",
                5,
                1,
            )
            .unwrap();
        }
        let fuel = instance.package().limits().fuel;
        let binding = manager
            .bind_io(&host, &instance, digest, manager.revision(), &caps, 100, 1)
            .unwrap();
        let observer = manager
            .bind_io(&host, &instance, digest, manager.revision(), &caps, 100, 1)
            .unwrap();
        let grant = ServiceGrant::issue(
            &manager,
            &host,
            &instance,
            &binding,
            "notes",
            "notes.echo",
            1,
        )
        .unwrap();
        let content_policy = content.then(|| {
            ServiceContentPolicy::issue(&host, &instance, &grant, scopes.clone(), 1).unwrap()
        });
        let principal_active = Arc::new(AtomicBool::new(true));
        let active = principal_active.clone();
        let access = content_policy.as_ref().map(|policy| {
            policy
                .authorize("alice", scopes, move || active.load(Ordering::SeqCst))
                .unwrap()
        });
        let ticks = Arc::new(AtomicU64::new(1));
        let original_clock = ticks.clone();
        let worker = IoWorker::spawn_managed(
            &manager,
            host,
            instance,
            binding,
            move || original_clock.load(Ordering::SeqCst),
            2,
            JobLimits::new(2, 32_768, 1_048_576).unwrap(),
        )
        .unwrap();
        let utc = Arc::new(AtomicU64::new(CREATED));
        let clock = utc.clone();
        let journal = ServiceJournal::new(policy(), move || clock.load(Ordering::SeqCst)).unwrap();
        Self {
            _dir: dir,
            _manager: manager,
            worker,
            binding: observer,
            grant,
            journal,
            utc,
            digest,
            fuel,
            request,
            access,
            content_policy,
            principal_active,
            ticks,
        }
    }
    fn submit(&self, router: Box<dyn BrokerRouter>) -> JobHandle {
        if let Some(access) = &self.access {
            return self
                .worker
                .submit_service_content(
                    service::Request::decode(self.request.bytes()).unwrap(),
                    self.grant.clone(),
                    self.journal.clone(),
                    KEY,
                    access.clone(),
                    router,
                    WAIT,
                )
                .unwrap();
        }
        self.worker
            .submit_service_durable(
                request(),
                self.grant.clone(),
                self.journal.clone(),
                KEY,
                router,
                WAIT,
            )
            .unwrap()
    }
    fn finish(&mut self) -> HostRuntime {
        self.worker.stop();
        let until = Instant::now() + WAIT;
        loop {
            if let Some(host) = self.worker.try_finish().unwrap() {
                host.store_local().integrity_check().unwrap();
                return host;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn service_command(&self) -> Command {
        RequestRecord::encode(&policy(), KEY, &self.request, CREATED)
            .unwrap()
            .command(self.digest)
            .unwrap()
    }
    fn outgoing_command(&self) -> Command {
        let request = outbound();
        Command {
            operation_id: "outbound-child".into(),
            subject: "script-scope".into(),
            package_sha256: self.digest,
            capability: IoCapability::HttpRequest,
            protocol_sha256: wire::schema_digest(),
            request_sha256: request.digest(),
            approval_sha256: [4; 32],
            target_sha256: [5; 32],
            request_bytes: request.bytes().len() as u64,
            response_limit: 1024,
        }
    }
}
fn ready(job: &mut JobHandle) {
    let until = Instant::now() + WAIT;
    loop {
        match job.poll() {
            Poll::Ready => return,
            Poll::Pending => {
                assert!(Instant::now() < until);
                thread::sleep(Duration::from_millis(1));
            }
            state => panic!("unexpected job state {state:?}"),
        }
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
        panic!("guest must perform no routed IO")
    }
}
#[test]
fn ready_utc_expiry_suppresses_body_but_keeps_the_actual_observed_history() {
    let mut run = Running::new(false);
    let mut job = run.submit(Box::new(NoIo));
    ready(&mut job);
    run.utc.store(CREATED + RETENTION, Ordering::SeqCst);
    let report = job.read(0).unwrap().unwrap();
    assert!(report.cancelled && report.task.execution.outcome.is_err());
    assert!(report.service_response.is_none());
    assert_eq!(report.payload_bytes(), 0);
    assert!(!report.service_retention_valid());
    assert_eq!(run.binding.usage().jobs, 0);
    let command = run.service_command();
    let host = run.finish();
    assert_eq!(
        host.store_local()
            .lookup_io_intent(&command.subject, &command.operation_id)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Observed
    );
}
#[test]
fn exact_retry_is_replayed_without_guest_fuel_and_charges_the_shared_input_completion_budget() {
    let mut run = Running::new(false);
    let cost = (request().bytes().len() + completion().len()) as u64;
    let mut first = run.submit(Box::new(NoIo));
    ready(&mut first);
    let first = first.read(4096).unwrap().unwrap();
    assert_eq!(
        first.service_persistence,
        Some(ServicePersistenceStatus::Completed)
    );
    assert!(first.task.execution.fuel_remaining < run.fuel);
    assert_eq!(first.bytes, cost);
    assert_eq!(run.binding.usage().bytes, cost);
    let mut retry = run.submit(Box::new(NoIo));
    ready(&mut retry);
    let replay = retry.read(4096).unwrap().unwrap();
    assert_eq!(
        replay.service_persistence,
        Some(ServicePersistenceStatus::Replayed)
    );
    assert_eq!(replay.task.execution.outcome, Ok(0));
    assert_eq!(replay.task.execution.fuel_remaining, run.fuel);
    assert_eq!(replay.calls, 0);
    assert_eq!(replay.task.execution.host_calls, 0);
    assert!(replay.service_response == first.service_response);
    assert_eq!(replay.bytes, cost);
    assert_eq!(run.binding.usage().bytes, 2 * cost);
    assert_eq!(run.binding.usage().jobs, 0);
    let host = run.finish();
    assert_eq!(host.store_local().pending_usage().unwrap().0, 3);
}
struct ExpireBeforeDispatch {
    utc: Arc<AtomicU64>,
    calls: Arc<AtomicUsize>,
    backend: Arc<AtomicUsize>,
    command: Command,
    error: Arc<Mutex<Option<io_execution::Error>>>,
}
impl BrokerRouter for ExpireBeforeDispatch {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        _: u32,
        _: &wire::Request,
    ) -> Result<Vec<u8>, RouterFault> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.utc.store(CREATED + RETENTION, Ordering::SeqCst);
        let result = context.dispatch(&self.command, |_| {
            self.backend.fetch_add(1, Ordering::SeqCst);
            Ok(vec![])
        });
        *self.error.lock().unwrap() = result.err();
        Err(RouterFault::Denied)
    }
}
#[test]
fn actual_guest_route_expiring_before_dispatch_never_calls_backend_and_remains_unknown() {
    let mut run = Running::new(true);
    let calls = Arc::new(AtomicUsize::new(0));
    let backend = Arc::new(AtomicUsize::new(0));
    let outgoing = run.outgoing_command();
    let error = Arc::new(Mutex::new(None));
    let mut job = run.submit(Box::new(ExpireBeforeDispatch {
        utc: run.utc.clone(),
        calls: calls.clone(),
        backend: backend.clone(),
        command: outgoing.clone(),
        error: error.clone(),
    }));
    ready(&mut job);
    let report = job.read(0).unwrap().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(backend.load(Ordering::SeqCst), 0);
    assert_eq!(*error.lock().unwrap(), Some(io_execution::Error::Expired));
    assert!(report.unknown && report.task.execution.outcome.is_err());
    assert!(report.service_response.is_none());
    let command = run.service_command();
    let host = run.finish();
    assert_eq!(
        host.store_local()
            .lookup_io_intent(&command.subject, &command.operation_id)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::OutcomeUnknown
    );
    assert!(
        host.store_local()
            .lookup_io_intent(&outgoing.subject, &outgoing.operation_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn content_ready_and_replay_recheck_principal_policy_and_original_object_expiry() {
    for cause in 0..3 {
        let mut run = Running::new_with_content(false, true);
        let mut first = run.submit(Box::new(NoIo));
        ready(&mut first);
        assert_eq!(
            first.read(4096).unwrap().unwrap().service_persistence,
            Some(ServicePersistenceStatus::Completed)
        );
        let mut replay = run.submit(Box::new(NoIo));
        ready(&mut replay);
        match cause {
            0 => run.principal_active.store(false, Ordering::SeqCst),
            1 => run.content_policy.as_ref().unwrap().revoke(),
            _ => run.ticks.store(5, Ordering::SeqCst),
        }
        let report = replay.read(0).unwrap().unwrap();
        assert!(report.cancelled && report.service_response.is_none());
        assert_eq!(report.payload_bytes(), 0);
        let command = run.service_command();
        let host = run.finish();
        assert_eq!(
            host.store_local()
                .lookup_io_intent(&command.subject, &command.operation_id)
                .unwrap()
                .unwrap()
                .phase(),
            Phase::Observed
        );
    }
}
struct RevokeContentBeforeDispatch {
    active: Arc<AtomicBool>,
    backend: Arc<AtomicUsize>,
    command: Command,
}
impl BrokerRouter for RevokeContentBeforeDispatch {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        _: u32,
        _: &wire::Request,
    ) -> Result<Vec<u8>, RouterFault> {
        self.active.store(false, Ordering::SeqCst);
        let result = context.dispatch(&self.command, |_| {
            self.backend.fetch_add(1, Ordering::SeqCst);
            Ok(vec![])
        });
        assert!(result.is_err());
        Err(RouterFault::Denied)
    }
}
#[test]
fn content_principal_revocation_inside_router_prevents_the_actual_backend() {
    let mut run = Running::new_with_content(true, true);
    let backend = Arc::new(AtomicUsize::new(0));
    let outgoing = run.outgoing_command();
    let mut job = run.submit(Box::new(RevokeContentBeforeDispatch {
        active: run.principal_active.clone(),
        backend: backend.clone(),
        command: outgoing.clone(),
    }));
    ready(&mut job);
    assert!(job.read(0).unwrap().unwrap().service_response.is_none());
    assert_eq!(backend.load(Ordering::SeqCst), 0);
    let host = run.finish();
    assert!(
        host.store_local()
            .lookup_io_intent(&outgoing.subject, &outgoing.operation_id)
            .unwrap()
            .is_none()
    );
}
#[test]
fn content_history_cannot_be_opened_through_the_unscoped_service_submit_api() {
    let mut run = Running::new_with_content(false, true);
    let mut first = run.submit(Box::new(NoIo));
    ready(&mut first);
    assert_eq!(
        first.read(4096).unwrap().unwrap().service_persistence,
        Some(ServicePersistenceStatus::Completed)
    );
    assert!(
        run.worker
            .submit_service_durable(
                service::Request::decode(run.request.bytes()).unwrap(),
                run.grant.clone(),
                run.journal.clone(),
                KEY,
                Box::new(NoIo),
                WAIT
            )
            .is_err()
    );
    assert!(
        run.worker
            .submit_service(
                service::Request::decode(run.request.bytes()).unwrap(),
                run.grant.clone(),
                Box::new(NoIo),
                WAIT
            )
            .is_err()
    );
    run.finish();
}
