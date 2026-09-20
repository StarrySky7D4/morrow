//! Application admission and recovery, using the original audited owner and real HTTP.
use super::*;
use crate::{host_capnp as wire, protocol};
use morrow_core::{
    io::{HttpSubmission, Request, Status},
    io_intent::Phase as IntentPhase,
    plugin_package::{
        Package,
        io::{self, IoCapability},
    },
};
use morrow_network_node::{
    Limits,
    managed_http::{EndpointApproval, HttpEndpoint, NetworkProfile},
};
use morrow_plugin_runtime::{
    instance_pool::Session,
    io_jobs::{RouteContext, RouterFault},
};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};
const ID: &str = "org.example.workbench.tasks.io";
const WAIT: Duration = Duration::from_secs(10);
struct Setup {
    app: Workbench,
    pooled: Session,
    digest: [u8; 32],
    dir: tempfile::TempDir,
}
impl Setup {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut app = Workbench::open_managed(dir.path(), None).unwrap();
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
        app.catalog.as_ref().unwrap().install(&package).unwrap();
        let manager = app.manager.as_mut().unwrap();
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, package.digest(), caps, manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let revision = manager.revision();
        let pooled = app
            .pool
            .start(manager, app.host.local_mut().unwrap(), ID, &[], revision)
            .unwrap();
        Self {
            app,
            pooled,
            digest: package.digest(),
            dir,
        }
    }
    fn options(&self) -> StartOptions {
        StartOptions {
            package_id: ID.into(),
            digest: self.digest,
            revision: self.app.manager.as_ref().unwrap().revision(),
            capabilities: BTreeSet::from([IoCapability::HttpRequest]),
            lifetime: WAIT,
            limits: JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
        }
    }
    fn start(
        &mut self,
        server: &Server,
        runtime: &tokio::runtime::Runtime,
        op: &str,
        hold: Option<(mpsc::SyncSender<()>, mpsc::Receiver<()>)>,
    ) -> TaskKey {
        let options = self.options();
        self.app
            .start_io(options, |p| {
                let endpoint = HttpEndpoint::approve(
                    p.manager,
                    p.host,
                    p.instance,
                    p.binding,
                    EndpointApproval {
                        origin: server.origin.clone(),
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
                    p.now,
                )?;
                let request = Request::encode_http_submit(
                    1,
                    &HttpSubmission {
                        operation_id: op.as_bytes().to_vec(),
                        deadline_ms: 0,
                        endpoint: endpoint.endpoint_reference().into_bytes(),
                        method: "GET".into(),
                        relative_target: "/owner".into(),
                        headers: vec![],
                        body: vec![],
                        credential: vec![],
                    },
                )?;
                let mut router: Box<dyn BrokerRouter> =
                    Box::new(endpoint.router(runtime.handle().clone()));
                if let Some((started, release)) = hold {
                    router = Box::new(HeldResult {
                        started,
                        release,
                        inner: router,
                    });
                }
                Ok(PreparedJob {
                    input: request.bytes().to_vec(),
                    router,
                    timeout: WAIT,
                })
            })
            .unwrap()
    }
    fn guards(&self) {
        assert_guards(self.dir.path(), &self.dir.path().join("workbench.db"));
    }
    fn local_pool(&self) {
        assert!(
            self.app.pool.root(&self.pooled).is_ok(),
            "original pooled session retained"
        );
    }
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}
fn wait_ready(app: &mut Workbench, key: TaskKey) {
    let deadline = Instant::now() + WAIT;
    loop {
        let status = app.poll_io(key).unwrap();
        match status.delivery {
            Some(Poll::Ready) => return,
            Some(Poll::Pending) => {}
            other => panic!("unexpected delivery {other:?}"),
        }
        assert!(Instant::now() < deadline, "job did not become ready");
        thread::sleep(Duration::from_millis(2));
    }
}
fn reclaim(app: &mut Workbench, key: TaskKey) -> Snapshot {
    let deadline = Instant::now() + WAIT;
    loop {
        let status = app.poll_io(key).unwrap();
        if status.exit.is_some() {
            return status;
        }
        assert!(Instant::now() < deadline, "owner did not return");
        thread::sleep(Duration::from_millis(2));
    }
}
fn access_error<T>(result: crate::Result<T>, expected: AccessError) {
    match result {
        Err(e) => assert_eq!(e.downcast_ref::<AccessError>(), Some(&expected)),
        Ok(_) => panic!("access unexpectedly permitted"),
    }
}
fn protocol_code(app: &mut Workbench, action: wire::Action, path: &str, token: &str) -> u16 {
    let mut request = capnp::message::Builder::new_default();
    let mut r = request.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    r.set_id("missing-card");
    r.set_selected_path(path);
    r.set_transfer(token);
    let response =
        protocol::respond(app, &capnp::serialize::write_message_to_words(&request)).unwrap();
    let mut bytes = response.as_slice();
    let message =
        capnp::serialize::read_message_from_flat_slice(&mut bytes, Default::default()).unwrap();
    message
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_ui_code()
}
// Hold a real response after its external effect. Cancellation cannot forcibly
// interrupt this trusted synchronous adapter; it must suppress the late delivery.
struct HeldResult {
    started: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
    inner: Box<dyn BrokerRouter>,
}
impl BrokerRouter for HeldResult {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> std::result::Result<Vec<u8>, RouterFault> {
        let result = self.inner.route(context, call, request);
        self.started.send(()).map_err(|_| RouterFault::Unknown)?;
        self.release
            .recv_timeout(WAIT)
            .map_err(|_| RouterFault::Unknown)?;
        result
    }
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

#[test]
fn application_ready_keeps_owner_busy_read_bound_does_not_consume_and_reclaims_original_store() {
    let runtime = runtime();
    let server = Server::new();
    let mut setup = Setup::new();
    let original = setup.app.host.local().unwrap().binding();
    let content = b"complete upload retained while busy";
    let time = now(setup.app.start);
    let token = setup
        .app
        .transfers
        .begin(
            "upload-operation".into(),
            content.len() as u64,
            &Sha256::digest(content),
            time,
        )
        .unwrap();
    setup
        .app
        .transfers
        .append(&token, 0, content, time)
        .unwrap();
    let key = setup.start(&server, &runtime, "application-ready", None);
    wait_ready(&mut setup.app, key);
    assert_eq!(setup.app.io_status().storage, StoragePhase::Running);
    setup.guards();
    access_error(setup.app.page("", 1), AccessError::Busy);
    access_error(setup.app.read_preferences(), AccessError::Busy);
    let export = setup.dir.path().join("must-not-exist.bin");
    assert_eq!(
        protocol_code(
            &mut setup.app,
            wire::Action::ExportFile,
            export.to_str().unwrap(),
            ""
        ),
        110
    );
    assert!(!export.exists());
    let existing = setup.dir.path().join("existing-export.bin");
    std::fs::write(&existing, b"original").unwrap();
    // Without the early gate create_new would fail first with a different error.
    assert_eq!(
        protocol_code(
            &mut setup.app,
            wire::Action::ExportFile,
            existing.to_str().unwrap(),
            ""
        ),
        110
    );
    assert_eq!(std::fs::read(&existing).unwrap(), b"original");
    let revision = setup.app.manager.as_ref().unwrap().revision();
    access_error(
        setup
            .app
            .configure_external_io(ID, &setup.digest, revision, &[]),
        AccessError::Busy,
    );
    assert_eq!(setup.app.manager.as_ref().unwrap().revision(), revision);
    assert!(
        setup
            .app
            .manager
            .as_ref()
            .unwrap()
            .selection(ID)
            .unwrap()
            .approved_io
            .contains(&IoCapability::HttpRequest)
    );
    assert_eq!(
        protocol_code(&mut setup.app, wire::Action::FinishPreferences, "", &token),
        110
    );
    let time = now(setup.app.start);
    assert_eq!(
        setup.app.transfers.finish(&token, time).unwrap(),
        ("upload-operation".into(), content.to_vec())
    );
    let capture = setup
        .app
        .capture_transfers
        .begin(
            "capture-upload".into(),
            content.len() as u64,
            &Sha256::digest(content),
            time,
        )
        .unwrap();
    setup
        .app
        .capture_transfers
        .append(&capture, 0, content, time)
        .unwrap();
    for action in [wire::Action::FinishPaste, wire::Action::FinishCapturedSave] {
        assert_eq!(protocol_code(&mut setup.app, action, "", &capture), 110);
    }
    assert_eq!(
        setup
            .app
            .capture_transfers
            .finish(&capture, time)
            .unwrap()
            .1,
        content
    );
    // Starting another task while Ready cannot invoke its trusted preparer.
    let options = setup.options();
    access_error(
        setup
            .app
            .start_io(options, |_| panic!("busy preparer must not run")),
        AccessError::Busy,
    );
    let bounded = match setup.app.read_io(key, 1) {
        Err(error) => error,
        Ok(_) => panic!("small read must fail"),
    };
    assert_eq!(bounded.to_string(), "IO result: ReadBound");
    assert_eq!(setup.app.poll_io(key).unwrap().delivery, Some(Poll::Ready));
    let report = setup.app.read_io(key, 256 * 1024).unwrap().unwrap();
    assert!(report.task.execution.outcome.is_ok());
    let response = report.http_response.unwrap();
    assert_eq!(response.status, Status::Completed);
    assert_eq!(response.body, b"owned");
    let status = reclaim(&mut setup.app, key);
    assert_eq!(status.storage, StoragePhase::Reclaimed);
    let exit = status.exit.unwrap();
    assert!(exit.execution.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok());
    assert_eq!(setup.app.host.local().unwrap().binding(), original);
    setup.guards();
    setup.local_pool();
    let store = setup.app.host.local().unwrap().store_local();
    assert_eq!(
        store
            .lookup_io_intent(ID, "application-ready")
            .unwrap()
            .unwrap()
            .phase(),
        IntentPhase::Observed
    );
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    store.integrity_check().unwrap();
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    let options = setup.options();
    access_error(
        setup
            .app
            .start_io(options, |_| panic!("unacknowledged preparer")),
        AccessError::UnacknowledgedTask,
    );
    setup.app.acknowledge_io(key).unwrap();
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    access_error(setup.app.cancel_io(key), AccessError::StaleTask);
    setup.app.finish().unwrap();
}

#[test]
fn old_or_foreign_tokens_cannot_stop_new_task_and_cancel_waits_for_late_router_release() {
    let runtime = runtime();
    let server = Server::new();
    let mut setup = Setup::new();
    let original = setup.app.host.local().unwrap().binding();
    let old = setup.start(&server, &runtime, "first-task", None);
    wait_ready(&mut setup.app, old);
    setup.app.read_io(old, 256 * 1024).unwrap().unwrap();
    reclaim(&mut setup.app, old);
    setup.app.acknowledge_io(old).unwrap();
    let mut foreign = Setup::new();
    let foreign_key = foreign.start(&server, &runtime, "foreign-task", None);
    wait_ready(&mut foreign.app, foreign_key);
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let key = setup.start(
        &server,
        &runtime,
        "late-task",
        Some((entered_tx, release_rx)),
    );
    entered_rx.recv_timeout(WAIT).unwrap();
    assert_ne!(old, key);
    access_error(setup.app.cancel_io(old), AccessError::StaleTask);
    access_error(setup.app.cancel_io(foreign_key), AccessError::StaleTask);
    assert_eq!(setup.app.io_status().storage, StoragePhase::Running);
    assert_eq!(
        setup.app.cancel_io(key).unwrap().storage,
        StoragePhase::Stopping
    );
    assert_eq!(
        setup.app.poll_io(key).unwrap().storage,
        StoragePhase::Stopping
    );
    setup.guards();
    access_error(setup.app.acknowledge_io(key), AccessError::Busy);
    access_error(setup.app.finish(), AccessError::Busy);
    setup.local_pool();
    release_tx.send(()).unwrap();
    let status = reclaim(&mut setup.app, key);
    assert_eq!(status.storage, StoragePhase::Reclaimed);
    let suppressed = setup.app.read_io(key, 256 * 1024).unwrap().unwrap();
    assert!(suppressed.cancelled && suppressed.unknown && suppressed.http_response.is_none());
    assert_eq!(setup.app.host.local().unwrap().binding(), original);
    setup.local_pool();
    setup.guards();
    assert_eq!(server.calls.load(Ordering::SeqCst), 3);
    setup.app.acknowledge_io(key).unwrap();
    foreign
        .app
        .read_io(foreign_key, 256 * 1024)
        .unwrap()
        .unwrap();
    reclaim(&mut foreign.app, foreign_key);
    foreign.app.acknowledge_io(foreign_key).unwrap();
    setup.app.finish().unwrap();
    foreign.app.finish().unwrap();
}

#[test]
fn preparation_failure_or_panic_returns_original_owner_and_keeps_existing_pool_usable() {
    let mut setup = Setup::new();
    let original = setup.app.host.local().unwrap().binding();
    let options = setup.options();
    assert!(
        setup
            .app
            .start_io(options, |_| Err("intentional preparation failure".into()))
            .is_err()
    );
    let options = setup.options();
    assert!(
        setup
            .app
            .start_io(options, |_| panic!("intentional preparation panic"))
            .is_err()
    );
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    assert_eq!(setup.app.host.local().unwrap().binding(), original);
    setup.guards();
    setup.local_pool();
    // The next real admission proves the failed preparations left no phantom task.
    let runtime = runtime();
    let server = Server::new();
    let key = setup.start(&server, &runtime, "after-preparation-failure", None);
    wait_ready(&mut setup.app, key);
    let report = setup.app.read_io(key, 256 * 1024).unwrap().unwrap();
    assert_eq!(report.http_response.unwrap().body, b"owned");
    reclaim(&mut setup.app, key);
    setup.app.acknowledge_io(key).unwrap();
    setup.local_pool();
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    setup.app.finish().unwrap();
}

struct NeverRouter;
impl BrokerRouter for NeverRouter {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &Request,
    ) -> std::result::Result<Vec<u8>, RouterFault> {
        panic!("rejected admission must never route an external operation");
    }
}
#[test]
fn rejected_worker_options_and_submission_return_owner_with_inspectable_status() {
    let mut setup = Setup::new();
    let original = setup.app.host.local().unwrap().binding();
    for invalid_worker in [true, false] {
        let mut options = setup.options();
        if invalid_worker {
            options.limits.max_calls = 0;
        } else {
            options.limits = JobLimits::new(1, 1, 1).unwrap();
        }
        assert!(
            setup
                .app
                .start_io(options, |_| Ok(PreparedJob {
                    input: vec![0; 64],
                    router: Box::new(NeverRouter),
                    timeout: WAIT
                }))
                .is_err()
        );
        let key = setup
            .app
            .io_status()
            .key
            .expect("failed admission retains its task identity");
        let status = reclaim(&mut setup.app, key);
        assert_eq!(status.storage, StoragePhase::Reclaimed);
        let exit = status.exit.unwrap();
        assert_eq!(exit.execution.is_err(), invalid_worker);
        assert!(exit.disconnect.is_ok() && exit.maintenance.is_ok());
        assert_eq!(setup.app.host.local().unwrap().binding(), original);
        setup.guards();
        setup.local_pool();
        setup.app.acknowledge_io(key).unwrap();
    }
    setup.app.finish().unwrap();
}

#[test]
fn dropping_application_keeps_protected_storage_until_actual_router_exit() {
    let runtime = runtime();
    let server = Server::new();
    let mut setup = Setup::new();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    setup.start(
        &server,
        &runtime,
        "dropped-application",
        Some((entered_tx, release_rx)),
    );
    entered_rx.recv_timeout(WAIT).unwrap();
    let Setup { app, dir, .. } = setup;
    let before = Instant::now();
    drop(app);
    assert!(
        before.elapsed() < Duration::from_secs(1),
        "Drop must not await the router"
    );
    assert_guards(dir.path(), &dir.path().join("workbench.db"));
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + WAIT;
    let mut registry = loop {
        match morrow_audit::library::Registry::open(dir.path()) {
            Ok(registry) => break registry,
            Err(morrow_audit::library::Error::Busy) => {
                assert!(
                    Instant::now() < deadline,
                    "worker did not release the original library"
                );
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("reopen original library: {error}"),
        }
    };
    let session = registry.open_session(Default::default()).unwrap();
    let store = session.runtime_ref().store_local();
    assert_eq!(
        store
            .lookup_io_intent(ID, "dropped-application")
            .unwrap()
            .unwrap()
            .phase(),
        IntentPhase::Observed
    );
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    store.integrity_check().unwrap();
}

#[cfg(feature = "fault-injection")]
#[test]
fn maintenance_failure_requires_explicit_repair_and_does_not_resend_observed_http() {
    let runtime = runtime();
    let server = Server::new();
    let mut setup = Setup::new();
    let original = setup.app.host.local().unwrap().binding();
    setup
        .app
        .host
        .local_mut()
        .unwrap()
        .fail_next_seal_for_test();
    let key = setup.start(&server, &runtime, "repair-only", None);
    wait_ready(&mut setup.app, key);
    let report = setup.app.read_io(key, 256 * 1024).unwrap().unwrap();
    assert_eq!(report.http_response.unwrap().body, b"owned");
    let failed = reclaim(&mut setup.app, key);
    assert_eq!(failed.storage, StoragePhase::RecoveryRequired);
    assert!(failed.exit.unwrap().maintenance.is_err());
    assert!(
        setup
            .app
            .host
            .local()
            .unwrap()
            .store_local()
            .pending_usage()
            .unwrap()
            .0
            > 0
    );
    access_error(setup.app.acknowledge_io(key), AccessError::RecoveryRequired);
    access_error(
        setup.app.host.prepare_write(),
        AccessError::RecoveryRequired,
    );
    let options = setup.options();
    access_error(
        setup
            .app
            .start_io(options, |_| panic!("repair must precede admission")),
        AccessError::RecoveryRequired,
    );
    let repaired = setup.app.repair_io(key).unwrap();
    assert_eq!(repaired.storage, StoragePhase::Reclaimed);
    assert_eq!(
        repaired.exit, failed.exit,
        "historical maintenance failure remains visible"
    );
    let owner = setup.app.host.local().unwrap();
    assert_eq!(owner.binding(), original);
    assert_eq!(owner.store_local().pending_usage().unwrap(), (0, 0));
    assert_eq!(
        owner
            .store_local()
            .lookup_io_intent(ID, "repair-only")
            .unwrap()
            .unwrap()
            .phase(),
        IntentPhase::Observed
    );
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    setup.guards();
    setup.local_pool();
    setup.app.acknowledge_io(key).unwrap();
    setup.app.finish().unwrap();
}
