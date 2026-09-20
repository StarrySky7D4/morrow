//! Actual audited Storage, managed Wasm and loopback HTTP. No alternate Store.
use super::Storage;
use morrow_core::{
    io::{HttpSubmission, Request, Status},
    io_intent::Phase as IntentPhase,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
};
use morrow_network_node::{
    Limits,
    managed_http::{EndpointApproval, HttpEndpoint, NetworkProfile},
};
use morrow_plugin_runtime::{
    instance_pool::{Pool, Session},
    io_binding::IoBinding,
    io_jobs::{
        BrokerRouter, IoWorker, JobHandle, JobLimits, JobReport, Poll, RouteContext, RouterFault,
        WorkerExit,
    },
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.workbench.storage.io";
const WAIT: Duration = Duration::from_secs(10);
const OP: &str = "audited-owner-request";

struct Setup {
    dir: tempfile::TempDir,
    owner: Storage,
    manager: Manager,
    pool: Pool,
    pooled: Session,
    instance: ManagedInstance,
    binding: IoBinding,
}
impl Setup {
    fn new() -> Self {
        fn assert_send<T: Send>() {}
        assert_send::<Storage>();
        assert_send::<morrow_audit::session::Session>();
        let dir = tempfile::tempdir().unwrap();
        let mut owner = Storage::open_managed(dir.path()).unwrap();
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
        let caps = BTreeSet::from([IoCapability::HttpRequest]);
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration =
            io::declaration(caps.iter().copied().collect(), vec!["io.invoke".into()]);
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_jobs = 1;
        budget.max_resources = 2;
        budget.max_job_bytes = 1024 * 1024;
        budget.max_bytes = 4 * 1024 * 1024;
        manifest.required_features.push(io::FEATURE.into());
        manifest.io_declaration = Some(declaration);
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("test-packages")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("test-registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Default::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, package.digest(), caps.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        // Existing pooled instance stays owned by Pool. The network job uses a
        // separate explicit admission to the very same original runtime.
        let mut pool = Pool::new(&owner, Default::default()).unwrap();
        let revision = manager.revision();
        let pooled = pool
            .start(&mut manager, &mut owner, ID, &[], revision)
            .unwrap();
        let instance = manager.connect(ID, &mut owner).unwrap();
        let binding = manager
            .bind_io(
                &owner,
                &instance,
                package.digest(),
                manager.revision(),
                &caps,
                100,
                1,
            )
            .unwrap();
        Self {
            dir,
            owner,
            manager,
            pool,
            pooled,
            instance,
            binding,
        }
    }
    fn endpoint(&self, origin: String) -> HttpEndpoint {
        HttpEndpoint::approve(
            &self.manager,
            &self.owner,
            &self.instance,
            &self.binding,
            EndpointApproval {
                origin,
                methods: vec!["GET".into()],
                profile: NetworkProfile::LoopbackHttp,
                limits: Limits {
                    max_request_bytes: 65536,
                    max_response_bytes: 65536,
                    max_header_bytes: 16384,
                    max_concurrent: 1,
                    timeout: Duration::from_secs(2),
                },
                response_frame_limit: 128 * 1024,
                credential: None,
                root_certificate: None,
            },
            [7; 32],
            2,
        )
        .unwrap()
    }
}
fn request(endpoint: &HttpEndpoint) -> Request {
    Request::encode_http_submit(
        1,
        &HttpSubmission {
            operation_id: OP.as_bytes().to_vec(),
            deadline_ms: 0,
            endpoint: endpoint.endpoint_reference().into_bytes(),
            method: "GET".into(),
            relative_target: "/owner".into(),
            headers: vec![],
            body: vec![],
            credential: vec![],
        },
    )
    .unwrap()
}
fn limits() -> JobLimits {
    JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap()
}
fn assert_guards(root: &std::path::Path, database: &std::path::Path) {
    use morrow_audit::{
        identity::LeaseError,
        library,
        sealer::Sealer,
        session::{self, OpenMode, SessionError},
    };
    assert!(matches!(
        library::Registry::open(root),
        Err(library::Error::Busy)
    ));
    assert!(matches!(
        session::Session::open(database, Default::default(), OpenMode::Existing),
        Err(SessionError::Busy)
    ));
    let key = morrow_audit::keys::Key::load(&session::key_path(database).unwrap()).unwrap();
    assert!(
        matches!(Sealer::new(key), Err(LeaseError::Busy)),
        "original signing identity lease must also remain held"
    );
}
fn report(job: &mut JobHandle) -> JobReport {
    let end = Instant::now() + WAIT;
    loop {
        match job.poll() {
            Poll::Ready => return job.read(256 * 1024).unwrap().unwrap(),
            Poll::Pending => {
                assert!(Instant::now() < end);
                thread::sleep(Duration::from_millis(2));
            }
            other => panic!("unexpected job status: {other:?}"),
        }
    }
}
fn reclaim(worker: &mut IoWorker<Storage>) -> WorkerExit<Storage> {
    let end = Instant::now() + WAIT;
    loop {
        if let Some(exit) = worker.try_reclaim().unwrap() {
            return exit;
        }
        assert!(Instant::now() < end, "owner must be returned");
        thread::sleep(Duration::from_millis(2));
    }
}
struct Server {
    origin: String,
    calls: Arc<AtomicUsize>,
    join: Option<thread::JoinHandle<()>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = stop.clone();
        let join = thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        // Windows accepted sockets inherit the listener mode.
                        // The listener polls; each accepted request uses bounded
                        // blocking reads/writes, including delayed first bytes.
                        socket.set_nonblocking(false).unwrap();
                        socket
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        socket
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut bytes = vec![];
                        let mut next = [0; 1024];
                        while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                            let n = socket.read(&mut next).unwrap();
                            assert!(n > 0 && bytes.len() + n <= 65536);
                            bytes.extend_from_slice(&next[..n]);
                        }
                        assert!(bytes.starts_with(b"GET /owner HTTP/1.1\r\n"));
                        count.fetch_add(1, Ordering::SeqCst);
                        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nowned").unwrap();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("listener: {e}"),
                }
            }
        });
        Self {
            origin,
            calls,
            join: Some(join),
            stop,
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Preserve a primary test failure instead of aborting during unwinding
        // if a disconnected client also caused the server thread to panic.
        if let Err(failure) = self.join.take().unwrap().join()
            && !thread::panicking()
        {
            std::panic::resume_unwind(failure);
        }
    }
}

fn actual_http(fail_finish: bool) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let setup = Setup::new();
    let server = Server::new();
    let endpoint = setup.endpoint(server.origin.clone());
    let input = request(&endpoint);
    let Setup {
        dir,
        mut owner,
        manager,
        mut pool,
        pooled,
        instance,
        binding,
    } = setup;
    let original = owner.binding();
    let trust = owner.session.trust();
    let database = owner
        ._registry
        .as_ref()
        .unwrap()
        .selected_database()
        .unwrap();
    #[cfg(feature = "fault-injection")]
    if fail_finish {
        owner.fail_next_seal_for_test();
    }
    let mut worker =
        IoWorker::spawn_managed_owned(&manager, owner, instance, binding, || 2, 1, limits())
            .unwrap_or_else(|_| panic!("protected owner admission"));
    assert!(
        Storage::open_managed(dir.path()).is_err(),
        "original leases must remain held"
    );
    assert_guards(dir.path(), &database);
    let mut job = worker
        .submit_brokered(
            input.bytes().to_vec(),
            Box::new(endpoint.router(runtime.handle().clone())),
            WAIT,
        )
        .unwrap();
    let result = report(&mut job);
    assert!(result.task.execution.outcome.is_ok());
    assert_eq!(
        result.http_response.as_ref().unwrap().status,
        Status::Completed
    );
    assert_eq!(result.http_response.as_ref().unwrap().body, b"owned");
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    worker.stop();
    let exit = reclaim(&mut worker);
    assert!(exit.result.is_ok());
    assert!(exit.disconnect.is_ok() && exit.instance.is_none());
    assert_eq!(exit.maintenance.is_err(), fail_finish);
    owner = exit.owner;
    assert_eq!(owner.binding(), original);
    assert_eq!(owner.session.trust().id, trust.id);
    assert_eq!(owner.session.trust().key, trust.key);
    assert!(
        Storage::open_managed(dir.path()).is_err(),
        "reclaimed owner still owns leases"
    );
    assert_guards(dir.path(), &database);
    assert!(
        pool.root(&pooled).is_ok(),
        "network ownership did not steal or revoke the pool root"
    );
    assert_eq!(
        owner
            .store_local()
            .lookup_io_intent(ID, OP)
            .unwrap()
            .unwrap()
            .phase(),
        IntentPhase::Observed
    );
    if fail_finish {
        assert!(owner.warning().is_some());
        assert!(owner.store_local().pending_usage().unwrap().0 > 0);
        owner.flush_pending().unwrap();
    }
    assert_eq!(owner.store_local().pending_usage().unwrap(), (0, 0));
    let sealed = owner.store_local().last_sealed_segment().unwrap().unwrap();
    morrow_audit::verify(&sealed, &trust).unwrap();
    pool.close_all(&mut owner).unwrap();
    drop(owner);
    let reopened = Storage::open_managed(dir.path()).unwrap();
    assert_eq!(reopened.session.trust().id, trust.id);
    assert_eq!(
        reopened
            .store_local()
            .lookup_io_intent(ID, OP)
            .unwrap()
            .unwrap()
            .phase(),
        IntentPhase::Observed
    );
    reopened.store_local().integrity_check().unwrap();
}

#[test]
fn protected_owner_keeps_original_pool_identity_leases_and_seals_real_http() {
    actual_http(false);
}

#[cfg(feature = "fault-injection")]
#[test]
fn finish_maintenance_failure_preserves_observed_http_and_returns_original_owner_for_retry() {
    actual_http(true);
}

#[cfg(feature = "fault-injection")]
#[test]
fn failed_prepare_returns_protected_owner_without_external_effect() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let setup = Setup::new();
    let server = Server::new();
    let endpoint = setup.endpoint(server.origin.clone());
    let input = request(&endpoint);
    let Setup {
        dir,
        mut owner,
        manager,
        mut pool,
        pooled,
        instance,
        binding,
    } = setup;
    let original = owner.binding();
    owner.warning = Some("pending maintenance".into());
    owner.fail_next_seal_for_test();
    let mut worker =
        IoWorker::spawn_managed_owned(&manager, owner, instance, binding, || 2, 1, limits())
            .unwrap_or_else(|_| panic!("protected owner admission"));
    let _job = worker
        .submit_brokered(
            input.bytes().to_vec(),
            Box::new(endpoint.router(runtime.handle().clone())),
            WAIT,
        )
        .unwrap();
    let exit = reclaim(&mut worker);
    assert!(exit.result.is_err());
    assert!(exit.maintenance.is_ok());
    let mut owner = exit.owner;
    assert_eq!(owner.binding(), original);
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    assert!(
        owner
            .store_local()
            .lookup_io_intent(ID, OP)
            .unwrap()
            .is_none()
    );
    assert!(Storage::open_managed(dir.path()).is_err());
    assert!(pool.root(&pooled).is_ok());
    pool.close_all(&mut owner).unwrap();
}

struct HeldRouter {
    started: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
    inner: Box<dyn BrokerRouter>,
}
impl BrokerRouter for HeldRouter {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> Result<Vec<u8>, RouterFault> {
        self.started.send(()).unwrap();
        self.release.recv_timeout(WAIT).unwrap();
        self.inner.route(context, call, request)
    }
}

#[test]
fn dropping_worker_retains_protected_locks_until_blocked_router_really_exits() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let setup = Setup::new();
    let server = Server::new();
    let endpoint = setup.endpoint(server.origin.clone());
    let input = request(&endpoint);
    let Setup {
        dir,
        owner,
        manager,
        pool,
        pooled,
        instance,
        binding,
    } = setup;
    let trust = owner.session.trust();
    let database = owner
        ._registry
        .as_ref()
        .unwrap()
        .selected_database()
        .unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let worker =
        IoWorker::spawn_managed_owned(&manager, owner, instance, binding, || 2, 1, limits())
            .unwrap();
    let mut job = worker
        .submit_brokered(
            input.bytes().to_vec(),
            Box::new(HeldRouter {
                started: started_tx,
                release: release_rx,
                inner: Box::new(endpoint.router(runtime.handle().clone())),
            }),
            WAIT,
        )
        .unwrap();
    started_rx.recv_timeout(WAIT).unwrap();
    let before = Instant::now();
    drop(worker);
    assert!(
        before.elapsed() < Duration::from_secs(1),
        "Drop cannot wait for synchronous IO"
    );
    assert!(
        Storage::open_managed(dir.path()).is_err(),
        "a stop request is not thread completion"
    );
    assert_guards(dir.path(), &database);
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    release_tx.send(()).unwrap();
    let end = Instant::now() + WAIT;
    let reopened = loop {
        match Storage::open_managed(dir.path()) {
            Ok(owner) => break owner,
            Err(_) => {
                assert!(Instant::now() < end, "exited worker must release leases");
                thread::sleep(Duration::from_millis(2));
            }
        }
    };
    assert_eq!(reopened.session.trust().id, trust.id);
    assert_eq!(
        server.calls.load(Ordering::SeqCst),
        0,
        "cancelled callback cannot send after release"
    );
    match job.read(256 * 1024) {
        Ok(Some(report)) => assert!(report.cancelled && report.http_response.is_none()),
        Err(_) => assert_eq!(job.poll(), Poll::Consumed),
        Ok(None) => panic!("completed worker cannot retain pending result"),
    }
    drop((pooled, pool));
}
