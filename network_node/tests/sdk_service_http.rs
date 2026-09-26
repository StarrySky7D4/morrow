//! Public-SDK-only original guests: real authenticated TCP -> managed HTTP.
//! Explicit synthetic credential provider; this does not qualify Windows DPAPI.
#![cfg(feature = "plugin-adapter")]
use morrow_core::{
    content::CardRecord,
    dispatch::{HostBinding, HostRuntime},
    io::{Action, Request},
    io_intent::Phase,
    outbound_authority::{self, Record, proto as outbound},
    plugin_package::{Package, catalog::Catalog, io::IoCapability, registry::Registry},
    service_record::Policy,
    store::{EventBudget, Store},
};
use morrow_network_node::{
    Limits,
    managed_http::{Credential, HttpEndpoint, HttpRouteSet},
    managed_service::{ManagedNode, ServiceHost},
    server::Principal,
    stored_http::StoredHttpEndpoint,
};
use morrow_plugin_runtime::{
    Limits as RuntimeLimits,
    io_binding::ServiceRunBudget,
    io_jobs::{
        BrokerRouter, CommandOwner, HostOwner, IoWorker, JobError, JobLimits, RouteContext,
        RouteStart, RouterFault,
    },
    manager::Manager,
    service_history::ServiceJournal,
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
const WAIT: Duration = Duration::from_secs(10);
const SERVICE: &str = "service.http.forward";
const TOKEN: &str = "synthetic-sdk-service-token-123456789";
const SECRET: &str = "Bearer synthetic-host-only-credential";
const ENDPOINT: [u8; 32] = [31; 32];
const CREDENTIAL: [u8; 32] = [32; 32];
const BODY: &[u8] = b"\0\xffpublic-sdk-binary";
fn packages() -> Vec<Package> {
    ["RUST", "C", "CPP"]
        .iter()
        .map(|lang| {
            let path = std::env::var_os(format!("MORROW_SERVICE_PACKAGE_{lang}"))
                .expect("all three generated service-http packages required");
            Package::decode(&std::fs::read(path).unwrap()).unwrap()
        })
        .collect()
}
fn status(bytes: &[u8], expected: u16) {
    assert!(
        bytes.starts_with(format!("HTTP/1.1 {expected} ").as_bytes()),
        "{}",
        String::from_utf8_lossy(bytes)
    );
}
async fn call(addr: std::net::SocketAddr, target: &str, key: &str, body: &[u8]) -> Vec<u8> {
    let mut socket = TcpStream::connect(addr).await.unwrap();
    // A forged directory must be stripped before the host inserts its own.
    let head = format!(
        "POST {target} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nCookie: caller-secret\r\nMorrow-Service-Resources-V1: forged\r\nIdempotency-Key: {key}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    socket.write_all(head.as_bytes()).await.unwrap();
    socket.write_all(body).await.unwrap();
    let mut raw = vec![];
    // A deliberate stop may close the socket before writing a response.
    let _ = tokio::time::timeout(WAIT, socket.read_to_end(&mut raw))
        .await
        .unwrap();
    raw
}
struct Upstream {
    origin: String,
    calls: Arc<AtomicUsize>,
    received: oneshot::Receiver<Vec<u8>>,
    release: Option<oneshot::Sender<bool>>,
    task: tokio::task::JoinHandle<()>,
}
impl Upstream {
    async fn open() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (send, received) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let task = tokio::spawn(async move {
            let (mut socket, _) = tokio::time::timeout(WAIT, listener.accept())
                .await
                .unwrap()
                .unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            let mut raw = vec![];
            let mut buf = [0; 1024];
            let end = loop {
                let n = tokio::time::timeout(WAIT, socket.read(&mut buf))
                    .await
                    .unwrap()
                    .unwrap();
                assert_ne!(n, 0);
                raw.extend_from_slice(&buf[..n]);
                assert!(raw.len() < 131072);
                if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = std::str::from_utf8(&raw[..end]).unwrap();
            assert!(headers.starts_with("POST / HTTP/1.1\r\n"));
            let fields: Vec<_> = headers.lines().filter_map(|l| l.split_once(':')).collect();
            assert_eq!(
                fields
                    .iter()
                    .filter(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                    .count(),
                1
            );
            assert!(
                fields
                    .iter()
                    .any(|(k, v)| k.eq_ignore_ascii_case("authorization") && v.trim() == SECRET)
            );
            assert!(!headers.contains(TOKEN));
            assert!(!headers.to_ascii_lowercase().contains("cookie:"));
            assert!(
                !headers
                    .to_ascii_lowercase()
                    .contains("morrow-service-resources-v1:")
            );
            let length: usize = fields
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .unwrap()
                .1
                .trim()
                .parse()
                .unwrap();
            assert!(end + length < 131072);
            while raw.len() < end + length {
                let n = tokio::time::timeout(WAIT, socket.read(&mut buf))
                    .await
                    .unwrap()
                    .unwrap();
                assert_ne!(n, 0);
                raw.extend_from_slice(&buf[..n]);
            }
            assert_eq!(&raw[end..], BODY);
            send.send(raw).unwrap();
            if tokio::time::timeout(WAIT, released).await.unwrap().unwrap() {
                let head = format!(
                    "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    BODY.len()
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(BODY).await;
            }
            let _ = socket.shutdown().await;
            // Keep listening through replay/reopen, so any accidental resend is
            // counted instead of hiding behind a refused connection.
            loop {
                let (mut extra, _) = listener.accept().await.unwrap();
                count.fetch_add(1, Ordering::SeqCst);
                let _ = extra
                    .write_all(
                        b"HTTP/1.1 500 Resent\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            }
        });
        Self {
            origin,
            calls,
            received,
            release: Some(release),
            task,
        }
    }
    async fn entered(&mut self) {
        tokio::time::timeout(WAIT, &mut self.received)
            .await
            .unwrap()
            .unwrap();
    }
    fn finish(&mut self, respond: bool) {
        self.release.take().unwrap().send(respond).unwrap();
    }
}
impl Drop for Upstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
struct Owner {
    runtime: HostRuntime,
    original: HostBinding,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.runtime
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.runtime
    }
}
impl CommandOwner for Owner {
    fn command(&mut self, input: Vec<u8>) -> Result<Vec<u8>, JobError> {
        assert_eq!(self.runtime.binding(), self.original);
        self.runtime
            .store_local_mut()
            .create_local(
                "sdk-http-local-write",
                &CardRecord::new("sdk-http-local", "note", 1, "during HTTP", input.clone())
                    .unwrap(),
            )
            .map_err(|_| JobError::Unavailable)?;
        Ok(input)
    }
}
#[derive(Clone)]
struct Capture {
    routes: HttpRouteSet,
    operations: Arc<Mutex<Vec<String>>>,
}
impl BrokerRouter for Capture {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> Result<Vec<u8>, RouterFault> {
        self.routes.route(context, call, request)
    }
    fn begin(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> RouteStart {
        let Action::SubmitHttp(http) = request.action() else {
            panic!("HTTP submission expected")
        };
        let operation = String::from_utf8(http.operation_id.clone()).unwrap();
        assert!(operation.starts_with("service-http-"));
        assert_eq!(operation.len(), 77);
        assert!(http.headers.is_empty());
        assert!(
            !request
                .bytes()
                .windows(SECRET.len())
                .any(|w| w == SECRET.as_bytes())
        );
        self.operations.lock().unwrap().push(operation);
        self.routes.begin(context, call, request)
    }
}
struct Run {
    package_id: String,
    _manager: Manager,
    host: ServiceHost<Owner>,
    node: ManagedNode,
    endpoint: HttpEndpoint,
    operations: Arc<Mutex<Vec<String>>>,
}
impl Run {
    async fn open(
        path: &Path,
        package: &Package,
        origin: &str,
        directory: bool,
        method: &str,
        request_limit: u64,
    ) -> Self {
        let catalog = Catalog::open(&path.join("catalog")).unwrap();
        catalog.install(package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&path.join("registry"), catalog).unwrap(),
            RuntimeLimits::default(),
        );
        let id = &package.manifest().package_id;
        let digest = package.digest();
        let caps = BTreeSet::from([
            IoCapability::HttpListen,
            IoCapability::HttpPublish,
            IoCapability::HttpRequest,
            IoCapability::CredentialUse,
        ]);
        manager.select(package, manager.revision()).unwrap();
        manager
            .set_enabled(id, digest, true, manager.revision())
            .unwrap();
        manager
            .approve_io(id, digest, caps.clone(), manager.revision())
            .unwrap();
        let mut store = Store::open(&path.join("db"), EventBudget::default()).unwrap();
        if store.load_outbound_authority(&ENDPOINT).unwrap().is_none() {
            for record in [
                Record::encode(outbound::Record {
                    schema_version: outbound_authority::VERSION,
                    reference: CREDENTIAL.to_vec(),
                    revision: 1,
                    created_ms: 1000,
                    expires_ms: 121000,
                    disabled: false,
                    kind: Some(outbound::record::Kind::Credential(outbound::Credential {
                        provider: outbound_authority::WINDOWS_DPAPI_PROVIDER.into(),
                        ciphertext: vec![1, 2, 3],
                    })),
                })
                .unwrap(),
                Record::encode(outbound::Record {
                    schema_version: outbound_authority::VERSION,
                    reference: ENDPOINT.to_vec(),
                    revision: 1,
                    created_ms: 1000,
                    expires_ms: 121000,
                    disabled: false,
                    kind: Some(outbound::record::Kind::Endpoint(outbound::Endpoint {
                        package_id: id.clone(),
                        package_sha256: digest.to_vec(),
                        origin: origin.into(),
                        profile: 2,
                        methods: vec![method.into()],
                        credential_reference: CREDENTIAL.to_vec(),
                        root_certificate: vec![],
                        max_request_bytes: request_limit,
                        max_response_bytes: 65536,
                        max_header_bytes: 16384,
                        max_concurrent: 1,
                        timeout_ms: 10000,
                        max_frame_bytes: 131072,
                    })),
                })
                .unwrap(),
            ] {
                store.save_outbound_authority_local(&record, 0).unwrap();
            }
        }
        let stored = StoredHttpEndpoint::resolve(&mut store, &ENDPOINT, || 1000).unwrap();
        let credential = stored.credential_reference().unwrap();
        let selected = [stored];
        let scope = StoredHttpEndpoint::selection_digest(&selected)
            .unwrap()
            .unwrap();
        let [stored] = selected;
        let mut runtime = HostRuntime::new(store).unwrap();
        let instance = manager.connect(id, &mut runtime).unwrap();
        let binding = manager
            .bind_budgeted_service_run(
                &runtime,
                &instance,
                digest,
                manager.revision(),
                &caps,
                120001,
                1,
                ServiceRunBudget {
                    max_jobs: 64,
                    max_bytes: 4194304,
                },
            )
            .unwrap();
        let endpoint = stored
            .approve_persistent(
                &manager,
                &runtime,
                &instance,
                &binding,
                [23; 32],
                1,
                |record| {
                    assert_eq!(record.reference(), CREDENTIAL);
                    Credential::header(credential, "authorization", SECRET)
                },
            )
            .unwrap();
        let grant =
            ServiceGrant::issue(&manager, &runtime, &instance, &binding, SERVICE, SERVICE, 1)
                .unwrap();
        let listener = ListenerGrant::issue(&manager, &runtime, &instance, &binding, 1).unwrap();
        let original = runtime.binding();
        let worker = IoWorker::spawn_managed_owned(
            &manager,
            Owner { runtime, original },
            instance,
            binding,
            || 1,
            1,
            JobLimits::new(1, 1048576, 1048576).unwrap(),
        )
        .unwrap();
        let routes =
            HttpRouteSet::new(vec![endpoint.clone()], tokio::runtime::Handle::current()).unwrap();
        let resources = directory.then(|| routes.resources(scope).unwrap());
        let operations = Arc::new(Mutex::new(vec![]));
        let capture = Capture {
            routes,
            operations: operations.clone(),
        };
        let host = ServiceHost::new_owned_with_resources(
            worker,
            WAIT,
            Arc::new(move || Box::new(capture.clone())),
            Some(scope),
            resources,
        )
        .unwrap();
        let route = host
            .durable_route(
                grant,
                "POST",
                "/forward",
                ServiceJournal::new(
                    Policy {
                        namespace: [51; 32],
                        retention_ms: 120000,
                    },
                    || 1000,
                )
                .unwrap(),
            )
            .unwrap();
        let query = host.query_route(&route, "/history").unwrap();
        let node = ManagedNode::bind_owned(
            "127.0.0.1:0".parse().unwrap(),
            listener,
            vec![Principal::new("alice", TOKEN, &[SERVICE], Duration::from_secs(60)).unwrap()],
            vec![route, query],
            Limits {
                timeout: WAIT,
                ..Limits::default()
            },
        )
        .await
        .unwrap();
        Self {
            package_id: id.clone(),
            _manager: manager,
            host,
            node,
            endpoint,
            operations,
        }
    }
    async fn local_write(&self) {
        let mut command = self
            .host
            .submit_owner_command(b"local work while HTTP waits".to_vec(), 64)
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(bytes) = command.read().unwrap() {
                    assert_eq!(bytes, b"local work while HTTP waits");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("original owner stays available during HTTP");
    }
    async fn close(self, phase: Option<Phase>) {
        self.node.shutdown().await.unwrap();
        let exit = self.host.shutdown_owned().await.unwrap();
        assert_eq!(exit.result, Ok(()));
        assert_eq!(exit.disconnect, Ok(()));
        assert_eq!(exit.maintenance, Ok(()));
        assert_eq!(exit.owner.runtime.binding(), exit.owner.original);
        let store = exit.owner.runtime.store_local();
        if let Some(phase) = phase {
            let operations = self.operations.lock().unwrap();
            assert_eq!(operations.len(), 1);
            assert_eq!(
                store
                    .lookup_io_intent(&self.package_id, &operations[0])
                    .unwrap()
                    .unwrap()
                    .phase(),
                phase
            );
            assert_eq!(
                store.card("sdk-http-local").unwrap().unwrap().body(),
                b"local work while HTTP waits"
            );
        }
        store.integrity_check().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires original three-language service-http packages; run verify_plugin_service_sdk.py --service-http"]
async fn originals_forward_and_reopen_without_resending_observed_or_unknown() {
    for package in packages() {
        for respond in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let mut upstream = Upstream::open().await;
            let run = Run::open(dir.path(), &package, &upstream.origin, true, "POST", 65536).await;
            let addr = run.node.local_addr();
            let mut incoming =
                tokio::spawn(async move { call(addr, "/forward", "once", BODY).await });
            tokio::select! { _ = upstream.entered() => {}, reply = &mut incoming => panic!("ended before outbound: {}", String::from_utf8_lossy(&reply.unwrap())) }
            run.local_write().await;
            assert!(!incoming.is_finished());
            upstream.finish(respond);
            let reply = tokio::time::timeout(WAIT, incoming).await.unwrap().unwrap();
            status(&reply, if respond { 201 } else { 409 });
            if respond {
                assert!(reply.ends_with(BODY));
            }
            for target in ["/forward", "/history"] {
                status(
                    &call(addr, target, "once", BODY).await,
                    if respond { 201 } else { 409 },
                );
            }
            status(&call(addr, "/forward", "once", b"different").await, 409);
            assert_eq!(upstream.calls.load(Ordering::SeqCst), 1);
            let operation = run.operations.lock().unwrap()[0].clone();
            let phase = if respond {
                Phase::Observed
            } else {
                Phase::OutcomeUnknown
            };
            run.close(Some(phase)).await;
            let run = Run::open(dir.path(), &package, &upstream.origin, true, "POST", 65536).await;
            for target in ["/forward", "/history"] {
                status(
                    &call(run.node.local_addr(), target, "once", BODY).await,
                    if respond { 201 } else { 409 },
                );
            }
            assert!(run.operations.lock().unwrap().is_empty());
            assert_eq!(upstream.calls.load(Ordering::SeqCst), 1);
            run.close(None).await;
            let store =
                Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
            assert_eq!(
                store
                    .lookup_io_intent(&package.manifest().package_id, &operation)
                    .unwrap()
                    .unwrap()
                    .phase(),
                phase
            );
            store.integrity_check().unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires original three-language service-http packages; run verify_plugin_service_sdk.py --service-http"]
async fn originals_stop_or_revoke_during_wait_preserve_unknown_without_resend() {
    for package in packages() {
        for stop in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut upstream = Upstream::open().await;
            let run = Run::open(dir.path(), &package, &upstream.origin, true, "POST", 65536).await;
            let addr = run.node.local_addr();
            let mut incoming =
                tokio::spawn(async move { call(addr, "/forward", "once", BODY).await });
            tokio::select! { _ = upstream.entered() => {}, reply = &mut incoming => panic!("ended before outbound: {}", String::from_utf8_lossy(&reply.unwrap())) }
            run.local_write().await;
            if stop {
                run.host.request_stop().unwrap();
                run.node.request_stop();
            } else {
                run.endpoint.revoke();
            }
            let reply = tokio::time::timeout(WAIT, incoming).await.unwrap().unwrap();
            assert!(!reply.starts_with(b"HTTP/1.1 201 "));
            let operation = run.operations.lock().unwrap()[0].clone();
            run.close(Some(Phase::OutcomeUnknown)).await;
            upstream.finish(true);
            let run = Run::open(dir.path(), &package, &upstream.origin, true, "POST", 65536).await;
            for target in ["/forward", "/history"] {
                status(
                    &call(run.node.local_addr(), target, "once", BODY).await,
                    409,
                );
            }
            assert!(run.operations.lock().unwrap().is_empty());
            assert_eq!(upstream.calls.load(Ordering::SeqCst), 1);
            run.close(None).await;
            let store =
                Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
            assert_eq!(
                store
                    .lookup_io_intent(&package.manifest().package_id, &operation)
                    .unwrap()
                    .unwrap()
                    .phase(),
                Phase::OutcomeUnknown
            );
            store.integrity_check().unwrap();
        }
    }
}

#[tokio::test]
#[ignore = "requires original three-language service-http packages; run verify_plugin_service_sdk.py --service-http"]
async fn originals_reject_missing_directory_method_and_body_limit_without_io() {
    for package in packages() {
        for (directory, method, limit, expected) in [
            (false, "POST", 65536, 503),
            (true, "GET", 65536, 403),
            (true, "POST", 1, 413),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let upstream = Upstream::open().await;
            let run = Run::open(
                dir.path(),
                &package,
                &upstream.origin,
                directory,
                method,
                limit,
            )
            .await;
            status(
                &call(run.node.local_addr(), "/forward", "rejected", BODY).await,
                expected,
            );
            assert!(run.operations.lock().unwrap().is_empty());
            assert_eq!(upstream.calls.load(Ordering::SeqCst), 0);
            run.close(None).await;
        }
    }
}
