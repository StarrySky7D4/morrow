//! Real HTTP -> authenticated service frame -> original managed Wasm -> HTTP.
#![cfg(feature = "plugin-adapter")]
use morrow_core::{
    dispatch::HostRuntime,
    io::{Header, Request as IoRequest},
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    service::{self, Invocation, Reply, Request, Response},
    service_record::{self, Policy},
    store::{EventBudget, Store},
};
use morrow_network_node::{
    Limits,
    managed_service::{ManagedNode, RouterFactory, ServiceHost},
    server::{Principal, TlsIdentity},
};
use morrow_plugin_runtime::{
    Limits as RuntimeLimits,
    io_jobs::{BrokerRouter, IoWorker, JobLimits, RouteContext, RouterFault},
    manager::Manager,
    service_history::ServiceJournal,
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
const ID: &str = "org.example.managed.service";
const KEY: &str = "synthetic-request-1";
fn policy() -> Policy {
    Policy {
        namespace: [8; 32],
        retention_ms: 1000,
    }
}
const TOKEN: &str = "synthetic-managed-service-token-123456789";
struct Deny;
impl BrokerRouter for Deny {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &IoRequest,
    ) -> Result<Vec<u8>, RouterFault> {
        Err(RouterFault::Denied)
    }
}
fn routers() -> RouterFactory {
    Arc::new(|| Box::new(Deny))
}
fn invocation(method: &str) -> Invocation {
    Invocation {
        service: "service.notes".into(),
        handler: "serve.notes".into(),
        principal: "alice".into(),
        method: method.into(),
        target: "/api?q=one&q=two".into(),
        headers: vec![Header {
            name: "x-claimed-principal".into(),
            value: b"administrator".to_vec(),
        }],
        body: if method == "HEAD" {
            vec![]
        } else {
            b"\0\xffinput".to_vec()
        },
    }
}
fn reply(method: &str) -> Reply {
    Reply {
        status: 202,
        headers: vec![
            Header {
                name: "x-result".into(),
                value: b"one".to_vec(),
            },
            Header {
                name: "x-result".into(),
                value: b"two".to_vec(),
            },
            Header {
                name: "x-raw".into(),
                value: vec![0xe9],
            },
        ],
        body: if method == "HEAD" {
            vec![]
        } else {
            b"\0\xffoutput".to_vec()
        },
    }
}
fn quoted(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
fn fixture(request: &Request, response: &[u8], spin: bool) -> Vec<u8> {
    // The fixture compares every byte, so identity/method/target/header/body changes
    // cannot accidentally pass as a byte-transform demonstration.
    let input = quoted(request.bytes());
    let output = quoted(response);
    let spin_code = if spin {
        "(loop $forever br $forever)"
    } else {
        ""
    };
    wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (memory (export "memory") 5)
      (data (i32.const 131072) "{input}") (data (i32.const 196608) "{output}")
      (func (export "morrow_run") (result i32) (local $i i32)
        i32.const 0 i32.const 131072 call $read i32.const {size} i32.ne if unreachable end
        (loop $check
          local.get $i i32.load8_u
          i32.const 131072 local.get $i i32.add i32.load8_u i32.ne if unreachable end
          local.get $i i32.const 1 i32.add local.tee $i i32.const {size} i32.lt_u br_if $check)
        {spin_code}
        i32.const 196608 i32.const {outsize} call $done drop i32.const 0))"#,
        size = request.bytes().len(),
        outsize = response.len()
    ))
    .unwrap()
}
struct Running {
    _dir: tempfile::TempDir,
    manager: Manager,
    host: ServiceHost,
    grant: ServiceGrant,
    listener: ListenerGrant,
    clock: Arc<AtomicU64>,
    digest: [u8; 32],
    journal: Option<ServiceJournal>,
    wall: Arc<AtomicU64>,
}
impl Running {
    fn new(method: &str, corrupt: bool, spin: bool) -> Self {
        Self::configured(
            tempfile::tempdir().unwrap(),
            method,
            corrupt,
            spin,
            false,
            RuntimeLimits::default().fuel,
        )
    }
    fn durable(corrupt: bool, fuel: u64) -> Self {
        Self::configured(
            tempfile::tempdir().unwrap(),
            "POST",
            corrupt,
            false,
            true,
            fuel,
        )
    }
    fn reopen(self, fuel: u64) -> Self {
        let Self {
            _dir,
            manager,
            host,
            grant,
            listener,
            clock,
            digest: _,
            journal,
            wall,
        } = self;
        drop((manager, host, grant, listener, clock, journal, wall));
        Self::configured(_dir, "POST", false, false, true, fuel)
    }
    fn configured(
        dir: tempfile::TempDir,
        method: &str,
        corrupt: bool,
        spin: bool,
        durable: bool,
        fuel: u64,
    ) -> Self {
        let id = if durable {
            service_record::call_id(&policy(), KEY, &invocation(method)).unwrap()
        } else {
            1
        };
        let request = Request::encode(id, &invocation(method)).unwrap();
        let response_request =
            Request::encode(if corrupt { id + 1 } else { id }, &invocation(method)).unwrap();
        let response = Response::encode(&response_request, &reply(method)).unwrap();
        let wasm = fixture(&request, &response, spin);
        let caps = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
        let mut declaration =
            io::declaration(caps.iter().copied().collect(), vec!["serve.notes".into()]);
        declaration.service_schema_sha256 = service::schema_digest().to_vec();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        manifest.required_features.push(io::FEATURE.into());
        manifest.io_declaration = Some(declaration);
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(
            registry,
            RuntimeLimits {
                fuel,
                ..RuntimeLimits::default()
            },
        );
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, digest, caps.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, digest, true, manager.revision())
            .unwrap();
        let mut runtime =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut runtime).unwrap();
        let binding = manager
            .bind_io(
                &runtime,
                &instance,
                digest,
                manager.revision(),
                &caps,
                100,
                1,
            )
            .unwrap();
        let grant = ServiceGrant::issue(
            &manager,
            &runtime,
            &instance,
            &binding,
            "service.notes",
            "serve.notes",
            2,
        )
        .unwrap();
        let listener = ListenerGrant::issue(&manager, &runtime, &instance, &binding, 2).unwrap();
        let clock = Arc::new(AtomicU64::new(2));
        let tick = clock.clone();
        let worker = IoWorker::spawn_managed(
            &manager,
            runtime,
            instance,
            binding,
            move || tick.load(Ordering::SeqCst),
            1,
            JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
        )
        .unwrap();
        let host = ServiceHost::new(worker, Duration::from_secs(2), routers()).unwrap();
        let wall = Arc::new(AtomicU64::new(1_000_000));
        let utc = wall.clone();
        let journal = durable
            .then(|| ServiceJournal::new(policy(), move || utc.load(Ordering::SeqCst)).unwrap());
        Self {
            _dir: dir,
            manager,
            host,
            grant,
            listener,
            clock,
            digest,
            journal,
            wall,
        }
    }
    async fn bind(&self, method: &str) -> ManagedNode {
        let route = if let Some(journal) = &self.journal {
            self.host
                .durable_route(self.grant.clone(), method, "/api", journal.clone())
                .unwrap()
        } else {
            self.host.route(self.grant.clone(), method, "/api").unwrap()
        };
        ManagedNode::bind(
            "127.0.0.1:0".parse().unwrap(),
            self.listener.clone(),
            vec![
                Principal::new("alice", TOKEN, &["service.notes"], Duration::from_secs(60))
                    .unwrap(),
            ],
            vec![route],
            Limits::default(),
        )
        .await
        .unwrap()
    }
    async fn finish(&self) {
        self.host
            .shutdown()
            .await
            .unwrap()
            .store_local()
            .integrity_check()
            .unwrap();
    }
}
async fn request(address: SocketAddr, method: &str, token: &str) -> Vec<u8> {
    request_extra(address, method, token, "", None).await
}
async fn request_extra(
    address: SocketAddr,
    method: &str,
    token: &str,
    extra: &str,
    body: Option<&[u8]>,
) -> Vec<u8> {
    let mut input = invocation(method);
    if let Some(body) = body {
        input.body = body.to_vec();
    }
    let mut socket = TcpStream::connect(address).await.unwrap();
    // Cookie, Proxy-*, transport headers and Connection-nominated fields must be absent in guest input.
    let headers = format!(
        "{method} {} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nCookie: secret=not-for-plugin\r\nProxy-Metadata: not-for-plugin\r\nConnection: close, x-private\r\nX-Private: not-for-plugin\r\nX-Claimed-Principal: administrator\r\n{extra}Content-Length: {}\r\n\r\n",
        input.target,
        input.body.len()
    );
    socket.write_all(headers.as_bytes()).await.unwrap();
    socket.write_all(&input.body).await.unwrap();
    let mut bytes = vec![];
    tokio::time::timeout(Duration::from_secs(5), socket.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    bytes
}
fn status(response: &[u8], code: u16) {
    assert!(
        response.starts_with(format!("HTTP/1.1 {code} ").as_bytes()),
        "{}",
        String::from_utf8_lossy(response)
    );
}
#[tokio::test]
async fn all_seven_methods_reach_real_typed_managed_wasm() {
    for method in ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"] {
        let run = Running::new(method, false, false);
        let node = run.bind(method).await;
        let response = request(node.local_addr(), method, TOKEN).await;
        status(&response, 202);
        assert!(
            response
                .windows(b"x-result: one\r\nx-result: two".len())
                .any(|v| v == b"x-result: one\r\nx-result: two")
        );
        assert!(response.windows(10).any(|v| v == b"x-raw: \xe9\r\n"));
        assert!(response.ends_with(&reply(method).body));
        node.shutdown().await.unwrap();
        run.finish().await;
    }
}
#[tokio::test]
async fn wrong_auth_does_not_consume_plugin_call_identity() {
    let run = Running::new("POST", false, false);
    let node = run.bind("POST").await;
    status(
        &request(
            node.local_addr(),
            "POST",
            "synthetic-invalid-bearer-0123456789",
        )
        .await,
        401,
    );
    status(&request(node.local_addr(), "POST", TOKEN).await, 202);
    node.shutdown().await.unwrap();
    run.finish().await;
}
#[tokio::test]
async fn completion_for_another_call_cannot_be_delivered() {
    let run = Running::new("POST", true, false);
    let node = run.bind("POST").await;
    let response = request(node.local_addr(), "POST", TOKEN).await;
    status(&response, 502);
    assert!(!response.ends_with(&reply("POST").body));
    node.shutdown().await.unwrap();
    run.finish().await;
}
#[tokio::test]
async fn service_revocation_prevents_existing_route_and_new_registration() {
    let run = Running::new("POST", false, false);
    let node = run.bind("POST").await;
    run.grant.revoke();
    status(&request(node.local_addr(), "POST", TOKEN).await, 403);
    assert!(run.host.route(run.grant.clone(), "POST", "/other").is_err());
    node.shutdown().await.unwrap();
    run.finish().await;
}
async fn wait_closed(address: SocketAddr) {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if TcpStream::connect(address).await.is_err() {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
#[tokio::test]
async fn listener_single_use_and_revocation_stop_actual_socket() {
    let run = Running::new("POST", false, false);
    let node = run.bind("POST").await;
    let other = run.host.route(run.grant.clone(), "POST", "/api").unwrap();
    assert!(
        ManagedNode::bind(
            "127.0.0.1:0".parse().unwrap(),
            run.listener.clone(),
            vec![
                Principal::new("alice", TOKEN, &["service.notes"], Duration::from_secs(60))
                    .unwrap()
            ],
            vec![other],
            Limits::default(),
        )
        .await
        .is_err()
    );
    run.listener.revoke();
    wait_closed(node.local_addr()).await;
    node.shutdown().await.unwrap();
    run.finish().await;
}
#[tokio::test]
async fn expiry_manager_disable_and_drop_close_the_listener() {
    for mode in 0..3 {
        let mut run = Running::new("POST", false, false);
        let node = run.bind("POST").await;
        let address = node.local_addr();
        if mode == 0 {
            run.clock.store(100, Ordering::SeqCst);
        } else if mode == 1 {
            run.manager
                .set_enabled(ID, run.digest, false, run.manager.revision())
                .unwrap();
        }
        if mode == 2 {
            drop(node);
        } else {
            wait_closed(address).await;
            node.shutdown().await.unwrap();
        }
        wait_closed(address).await;
        run.finish().await;
    }
}
#[tokio::test]
async fn tls_routes_use_the_same_typed_managed_execution() {
    let run = Running::new("POST", false, false);
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let der = certified.cert.der().to_vec();
    let identity = TlsIdentity::from_pem(
        certified.cert.pem().as_bytes(),
        certified.key_pair.serialize_pem().as_bytes(),
    )
    .unwrap();
    let route = run.host.route(run.grant.clone(), "POST", "/api").unwrap();
    let node = ManagedNode::bind_tls(
        "127.0.0.1:0".parse().unwrap(),
        run.listener.clone(),
        vec![Principal::new("alice", TOKEN, &["service.notes"], Duration::from_secs(60)).unwrap()],
        vec![route],
        Limits::default(),
        identity,
    )
    .await
    .unwrap();
    // Use raw TLS to keep the byte-exact HTTP fixture (no automatic Accept header).
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(der))
        .unwrap();
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let socket = TcpStream::connect(node.local_addr()).await.unwrap();
    let mut socket = connector
        .connect(
            rustls::pki_types::ServerName::try_from("localhost").unwrap(),
            socket,
        )
        .await
        .unwrap();
    socket.write_all(format!("POST /api?q=one&q=two HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nConnection: close\r\nX-Claimed-Principal: administrator\r\nContent-Length: 7\r\n\r\n").as_bytes()).await.unwrap();
    socket.write_all(b"\0\xffinput").await.unwrap();
    let mut bytes = vec![];
    tokio::time::timeout(Duration::from_secs(5), socket.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    status(&bytes, 202);
    node.shutdown().await.unwrap();
    run.finish().await;
}

#[tokio::test]
async fn foreign_listener_and_mixed_workers_fail_before_binding() {
    let first = Running::new("POST", false, false);
    let second = Running::new("POST", false, false);
    let address = "127.0.0.1:0".parse().unwrap();
    let principals = || {
        vec![Principal::new("alice", TOKEN, &["service.notes"], Duration::from_secs(60)).unwrap()]
    };
    let foreign = second
        .host
        .route(second.grant.clone(), "POST", "/api")
        .unwrap();
    assert!(
        ManagedNode::bind(
            address,
            first.listener.clone(),
            principals(),
            vec![foreign],
            Limits::default()
        )
        .await
        .is_err()
    );
    let own = first
        .host
        .route(first.grant.clone(), "POST", "/api")
        .unwrap();
    let foreign = second
        .host
        .route(second.grant.clone(), "GET", "/api")
        .unwrap();
    assert!(
        ManagedNode::bind(
            address,
            first.listener.clone(),
            principals(),
            vec![own, foreign],
            Limits::default()
        )
        .await
        .is_err()
    );
    // Failed owner validation did not consume the valid single-use listener claim.
    let node = first.bind("POST").await;
    status(&request(node.local_addr(), "POST", TOKEN).await, 202);
    node.shutdown().await.unwrap();
    first.finish().await;
    second.finish().await;
}

#[tokio::test]
async fn durable_retry_survives_restart_without_executing_guest_again() {
    let run = Running::durable(false, RuntimeLimits::default().fuel);
    let node = run.bind("POST").await;
    let extra = format!("Idempotency-Key: {KEY}\r\n");
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        202,
    );
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        202,
    );
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, Some(b"different")).await,
        409,
    );
    node.shutdown().await.unwrap();
    run.finish().await;
    // One fuel unit cannot execute even this fixture's input check. A retained
    // response still succeeds, proving the restarted path skipped Wasm execution.
    let run = run.reopen(1);
    let node = run.bind("POST").await;
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        202,
    );
    let other = "Idempotency-Key: different-operation\r\n";
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, other, None).await,
        409,
    );
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        202,
    );
    node.shutdown().await.unwrap();
    run.finish().await;
}
#[tokio::test]
async fn durable_unknown_expiry_and_revocation_never_reexecute_or_disclose() {
    let run = Running::durable(false, 1);
    let node = run.bind("POST").await;
    let extra = format!("Idempotency-Key: {KEY}\r\n");
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        409,
    );
    node.shutdown().await.unwrap();
    run.finish().await;
    // Restoring enough fuel cannot turn an uncertain prior dispatch into a resend.
    let run = run.reopen(RuntimeLimits::default().fuel);
    let node = run.bind("POST").await;
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        409,
    );
    run.wall.store(1_001_000, Ordering::SeqCst);
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        410,
    );
    run.grant.revoke();
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        403,
    );
    node.shutdown().await.unwrap();
    run.finish().await;
}
#[tokio::test]
async fn durable_key_header_is_required_unique_bounded_and_not_hop_by_hop() {
    let run = Running::durable(false, RuntimeLimits::default().fuel);
    let node = run.bind("POST").await;
    for extra in [
        String::new(),
        format!("Idempotency-Key: {KEY}\r\nIdempotency-Key: {KEY}\r\n"),
        format!("Idempotency-Key: {}\r\n", "x".repeat(129)),
        format!("Idempotency-Key: {KEY}\r\nConnection: idempotency-key\r\n"),
    ] {
        status(
            &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
            400,
        );
    }
    status(
        &request_extra(
            node.local_addr(),
            "POST",
            TOKEN,
            &format!("Idempotency-Key: {KEY}\r\n"),
            None,
        )
        .await,
        202,
    );
    node.shutdown().await.unwrap();
    run.finish().await;
}
#[tokio::test]
async fn observed_service_result_expires_without_reopening_its_key() {
    let run = Running::durable(false, RuntimeLimits::default().fuel);
    let node = run.bind("POST").await;
    let extra = format!("Idempotency-Key: {KEY}\r\n");
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        202,
    );
    run.wall.store(1_001_000, Ordering::SeqCst);
    let response = request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await;
    status(&response, 410);
    assert!(!response.ends_with(&reply("POST").body));
    node.shutdown().await.unwrap();
    run.finish().await;
}

#[tokio::test]
async fn dropped_http_response_is_recovered_after_restart_without_guest_execution() {
    let run = Running::durable(false, RuntimeLimits::default().fuel);
    let node = run.bind("POST").await;
    let mut socket = TcpStream::connect(node.local_addr()).await.unwrap();
    socket.write_all(format!("POST /api?q=one&q=two HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nIdempotency-Key: {KEY}\r\nX-Claimed-Principal: administrator\r\nContent-Length: 7\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    socket.write_all(b"\0\xffinput").await.unwrap();
    let mut first = [0; 1];
    tokio::time::timeout(Duration::from_secs(5), socket.read_exact(&mut first))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&first, b"H");
    // Drop before consuming the status, headers or response body. The host has
    // crossed its response persistence boundary, but the caller has no result.
    drop(socket);
    node.shutdown().await.unwrap();
    run.finish().await;
    let run = run.reopen(1);
    let node = run.bind("POST").await;
    let response = request_extra(
        node.local_addr(),
        "POST",
        TOKEN,
        &format!("Idempotency-Key: {KEY}\r\n"),
        None,
    )
    .await;
    status(&response, 202);
    assert!(response.ends_with(&reply("POST").body));
    node.shutdown().await.unwrap();
    run.finish().await;
}
#[tokio::test]
async fn durable_cached_response_is_scoped_to_the_actual_authenticated_principal() {
    const BOB: &str = "synthetic-principal-bob-token-123456789";
    let run = Running::durable(false, RuntimeLimits::default().fuel);
    let route = run
        .host
        .durable_route(
            run.grant.clone(),
            "POST",
            "/api",
            run.journal.clone().unwrap(),
        )
        .unwrap();
    let alice =
        Principal::new("alice", TOKEN, &["service.notes"], Duration::from_secs(60)).unwrap();
    let node = ManagedNode::bind(
        "127.0.0.1:0".parse().unwrap(),
        run.listener.clone(),
        vec![
            alice.clone(),
            Principal::new("bob", BOB, &["service.notes"], Duration::from_secs(60)).unwrap(),
        ],
        vec![route],
        Limits::default(),
    )
    .await
    .unwrap();
    let extra = format!("Idempotency-Key: {KEY}\r\n");
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        202,
    );
    // Bob is permitted to invoke the service, but the fixture only accepts an
    // Alice request; he cannot receive Alice's saved result under the same key.
    let response = request_extra(node.local_addr(), "POST", BOB, &extra, None).await;
    status(&response, 409);
    assert!(!response.ends_with(&reply("POST").body));
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        202,
    );
    alice.revoke();
    status(
        &request_extra(node.local_addr(), "POST", TOKEN, &extra, None).await,
        401,
    );
    node.shutdown().await.unwrap();
    run.finish().await;
}
