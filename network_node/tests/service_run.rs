//! Explicit long-lived service binding keeps one instance and short request deadlines.
#![cfg(feature = "plugin-adapter")]

use morrow_core::{
    dispatch::HostRuntime,
    io::Request as IoRequest,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    service::{self, Invocation, Reply, Request, Response},
    service_authority::{self, Record as Authority, proto as authority},
    service_config::{self, Config, proto as config},
    store::Store,
};
use morrow_network_node::{
    Limits,
    managed_service::{ManagedNode, RouterFactory, ServiceHost},
    server::Principal,
};
use morrow_plugin_runtime::{
    Limits as RuntimeLimits,
    io_binding::ServiceRunBudget,
    io_jobs::{
        BrokerRouter, IoWorker, JobError, JobLimits, RouteContext, RouterFault, ServiceUpdate,
    },
    manager::Manager,
    service_authority::{ConfiguredService, ResolvedService},
    service_io::{ListenerGrant, ServiceGrant},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const ID: &str = "org.example.service-run";
const TOKEN: &str = "synthetic-long-service-token-123456789";
const AUTH: [u8; 32] = [42; 32];
const APPROVAL: [u8; 32] = [43; 32];

fn configure(
    store: &mut Store,
    digest: [u8; 32],
    earlier_auth: bool,
    wall: Arc<AtomicU64>,
) -> ResolvedService {
    let config = Config::encode(config::Configuration {
        schema_version: service_config::VERSION,
        id: "long-run".into(),
        revision: 1,
        namespace: vec![19; 32],
        retention_ms: 20_000,
        service: "service.run".into(),
        handler: "serve.run".into(),
        package_sha256: digest.to_vec(),
        disabled: false,
        principals: vec![config::Principal {
            id: "alice".into(),
            authentication_reference: AUTH.to_vec(),
            content_scopes: vec![],
        }],
        approval_references: vec![APPROVAL.to_vec()],
    })
    .unwrap();
    store.save_service_config_local(&config, 0).unwrap();
    let auth = Authority::encode(authority::Record {
        schema_version: service_authority::VERSION,
        reference: AUTH.to_vec(),
        revision: 1,
        created_ms: 1000,
        expires_ms: if earlier_auth { 11_000 } else { 61_000 },
        disabled: false,
        kind: Some(authority::record::Kind::Authentication(
            authority::Authentication {
                principal_id: "alice".into(),
                token_sha256: Sha256::digest(TOKEN.as_bytes()).to_vec(),
            },
        )),
    })
    .unwrap();
    let publication = Authority::encode(authority::Record {
        schema_version: service_authority::VERSION,
        reference: APPROVAL.to_vec(),
        revision: 1,
        created_ms: 1000,
        expires_ms: if earlier_auth { 61_000 } else { 11_000 },
        disabled: false,
        kind: Some(authority::record::Kind::Publication(
            authority::Publication {
                config_id: "long-run".into(),
                config_sha256: config.digest().to_vec(),
                listen_address: "127.0.0.1:0".into(),
                tls_required: false,
                method: "POST".into(),
                path: "/api".into(),
                query_path: "/history".into(),
            },
        )),
    })
    .unwrap();
    store.save_service_authority_local(&auth, 0).unwrap();
    store.save_service_authority_local(&publication, 0).unwrap();
    ResolvedService::resolve(store, "long-run", &APPROVAL, move || {
        wall.load(Ordering::SeqCst)
    })
    .unwrap()
}

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

fn invocation(body: &[u8]) -> Invocation {
    Invocation {
        service: "service.run".into(),
        handler: "serve.run".into(),
        principal: "alice".into(),
        method: "POST".into(),
        target: "/api".into(),
        headers: vec![],
        body: body.to_vec(),
    }
}

fn quoted(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("\\{byte:02x}")).collect()
}

fn wasm(spin: bool) -> Vec<u8> {
    let first = Request::encode(1, &invocation(b"before")).unwrap();
    let second = Request::encode(2, &invocation(b"after")).unwrap();
    let response = |request: &Request, body: &[u8]| {
        Response::encode(
            request,
            &Reply {
                status: 202,
                headers: vec![],
                body: body.to_vec(),
            },
        )
        .unwrap()
    };
    let first_response = response(&first, b"executed-before");
    let second_response = response(&second, b"executed-after");
    let spin = if spin { "(loop $spin br $spin)" } else { "" };
    // Match both complete authenticated frames, including distinct call IDs.
    // The second response cannot be replayed from the first execution.
    wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (memory (export "memory") 5)
      (data (i32.const 131072) "{first}")
      (data (i32.const 163840) "{second}")
      (data (i32.const 196608) "{first_response}")
      (data (i32.const 229376) "{second_response}")
      (func $matches (param $base i32) (param $size i32) (param $actual i32) (result i32)
        (local $i i32)
        local.get $actual local.get $size i32.ne if i32.const 0 return end
        (loop $check
          local.get $i i32.load8_u
          local.get $base local.get $i i32.add i32.load8_u
          i32.ne if i32.const 0 return end
          local.get $i i32.const 1 i32.add local.tee $i local.get $size i32.lt_u br_if $check)
        i32.const 1)
      (func (export "morrow_run") (result i32) (local $size i32)
        i32.const 0 i32.const 131072 call $read local.set $size
        {spin}
        i32.const 131072 i32.const {first_size} local.get $size call $matches
        if
          i32.const 196608 i32.const {first_response_size} call $done drop
        else
          i32.const 163840 i32.const {second_size} local.get $size call $matches
          i32.eqz if unreachable end
          i32.const 229376 i32.const {second_response_size} call $done drop
        end
        i32.const 0))"#,
        first = quoted(first.bytes()),
        second = quoted(second.bytes()),
        first_response = quoted(&first_response),
        second_response = quoted(&second_response),
        first_size = first.bytes().len(),
        second_size = second.bytes().len(),
        first_response_size = first_response.len(),
        second_response_size = second_response.len(),
    ))
    .unwrap()
}

struct Running {
    _dir: tempfile::TempDir,
    manager: Manager,
    host: ServiceHost,
    grant: ServiceGrant,
    listener: ListenerGrant,
    started: Instant,
    digest: [u8; 32],
    router_factories: Arc<AtomicUsize>,
    configured: Option<ConfiguredService>,
    wall: Arc<AtomicU64>,
}

impl Running {
    fn new(spin: bool, timeout: Duration) -> Self {
        Self::with_configuration(spin, timeout, None)
    }

    fn with_configuration(spin: bool, timeout: Duration, earlier_auth: Option<bool>) -> Self {
        Self::with_options(spin, timeout, earlier_auth, None)
    }
    fn with_options(
        spin: bool,
        timeout: Duration,
        earlier_auth: Option<bool>,
        run_budget: Option<ServiceRunBudget>,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wasm(spin);
        let caps = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
        let mut declaration =
            io::declaration(caps.iter().copied().collect(), vec!["serve.run".into()]);
        declaration.service_schema_sha256 = service::schema_digest().to_vec();
        declaration.budget.as_mut().unwrap().max_duration_ms = timeout.as_millis() as u64;
        declaration.service_run = Some(io::proto::ServiceRunProfile {
            budget: run_budget.map(|_| io::proto::ServiceRunBudget {
                schema_version: 1,
                max_jobs: 10,
                max_bytes: io::MAX_BYTES,
            }),
            schema_version: 1,
            max_duration_ms: 120_000,
        });
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        manifest
            .required_features
            .extend([io::FEATURE.into(), "service-run-v1".into()]);
        if run_budget.is_some() {
            manifest
                .required_features
                .push(io::SERVICE_RUN_BUDGET_FEATURE.into());
        }
        manifest.io_declaration = Some(declaration);
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(
            registry,
            RuntimeLimits {
                fuel: 100_000_000,
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
        let mut store = Store::open(&dir.path().join("db"), Default::default()).unwrap();
        let wall = Arc::new(AtomicU64::new(1000));
        let resolved = earlier_auth
            .map(|earlier_auth| configure(&mut store, digest, earlier_auth, wall.clone()));
        let mut runtime = HostRuntime::new(store).unwrap();
        let instance = manager.connect(ID, &mut runtime).unwrap();
        let started = Instant::now();
        let binding = if let Some(budget) = run_budget {
            manager.bind_budgeted_service_run(
                &runtime,
                &instance,
                digest,
                manager.revision(),
                &caps,
                120_001,
                1,
                budget,
            )
        } else {
            manager.bind_service_run(
                &runtime,
                &instance,
                digest,
                manager.revision(),
                &caps,
                120_001,
                1,
            )
        }
        .unwrap();
        let now = || u64::try_from(started.elapsed().as_millis()).unwrap() + 1;
        let grant = ServiceGrant::issue(
            &manager,
            &runtime,
            &instance,
            &binding,
            "service.run",
            "serve.run",
            now(),
        )
        .unwrap();
        let listener =
            ListenerGrant::issue(&manager, &runtime, &instance, &binding, now()).unwrap();
        let configured = resolved.map(|resolved| {
            resolved
                .issue(&manager, &runtime, &instance, &binding, now())
                .unwrap()
        });
        let worker = IoWorker::spawn_managed_owned(
            &manager,
            runtime,
            instance,
            binding,
            move || u64::try_from(started.elapsed().as_millis()).unwrap() + 1,
            1,
            JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
        )
        .unwrap();
        // A 120-second run must still reject requests above this package's short
        // per-request declaration, before enqueueing or consuming the first call.
        assert!(matches!(
            worker.submit_service(
                Request::encode(1, &invocation(b"before")).unwrap(),
                grant.clone(),
                Box::new(Deny),
                timeout + Duration::from_millis(1)
            ),
            Err(JobError::InvalidOptions)
        ));
        let router_factories = Arc::new(AtomicUsize::new(0));
        let factories = router_factories.clone();
        let factory: RouterFactory = Arc::new(move || {
            factories.fetch_add(1, Ordering::SeqCst);
            Box::new(Deny)
        });
        let host = ServiceHost::new_owned(worker, timeout, factory).unwrap();
        Self {
            _dir: dir,
            manager,
            host,
            grant,
            listener,
            started,
            digest,
            router_factories,
            configured,
            wall,
        }
    }

    async fn bind(&self) -> ManagedNode {
        ManagedNode::bind_owned(
            "127.0.0.1:0".parse().unwrap(),
            self.listener.clone(),
            vec![
                Principal::new("alice", TOKEN, &["service.run"], Duration::from_secs(120)).unwrap(),
            ],
            vec![self.host.route(self.grant.clone(), "POST", "/api").unwrap()],
            Limits::default(),
        )
        .await
        .unwrap()
    }

    async fn finish(&self) {
        let exit = self.host.shutdown_owned().await.unwrap();
        assert!(exit.result.is_ok());
        assert!(exit.disconnect.is_ok());
        assert!(exit.maintenance.is_ok());
        exit.owner.store_local().integrity_check().unwrap();
    }
}

async fn request(address: SocketAddr, body: &[u8]) -> Vec<u8> {
    let mut socket = TcpStream::connect(address).await.unwrap();
    socket.write_all(format!(
        "POST /api HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n", body.len()
    ).as_bytes()).await.unwrap();
    socket.write_all(body).await.unwrap();
    let mut response = vec![];
    tokio::time::timeout(Duration::from_secs(10), socket.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    response
}

fn status(response: &[u8], code: u16) {
    assert!(
        response.starts_with(format!("HTTP/1.1 {code} ").as_bytes()),
        "{}",
        String::from_utf8_lossy(response)
    );
}

#[tokio::test]
async fn original_listener_executes_distinct_authenticated_request_after_thirty_seconds() {
    let run = Running::new(false, Duration::from_secs(2));
    let node = run.bind().await;
    let before = request(node.local_addr(), b"before").await;
    status(&before, 202);
    assert!(before.ends_with(b"executed-before"));
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        run.started + Duration::from_secs(31),
    ))
    .await;
    assert!(run.started.elapsed() >= Duration::from_secs(31));
    let after = request(node.local_addr(), b"after").await;
    status(&after, 202);
    assert!(after.ends_with(b"executed-after"));
    assert_eq!(run.router_factories.load(Ordering::SeqCst), 2);
    node.shutdown().await.unwrap();
    run.finish().await;
}

#[tokio::test]
async fn long_run_does_not_extend_a_short_request_deadline() {
    let run = Running::new(true, Duration::from_millis(1));
    let node = run.bind().await;
    let response = request(node.local_addr(), b"before").await;
    // The guest deliberately burns fuel; late results remain suppressed even
    // though the 120-second service binding itself is still live.
    status(&response, 503);
    assert!(
        run.host
            .route(run.grant.clone(), "POST", "/still-live")
            .is_ok()
    );
    node.shutdown().await.unwrap();
    run.finish().await;
}

#[tokio::test]
async fn configured_earlier_authority_expiry_and_config_change_stop_long_run_publication() {
    for mode in 0..3 {
        let run = Running::with_configuration(false, Duration::from_secs(2), Some(mode == 0));
        let configured = run.configured.as_ref().unwrap();
        let node = run
            .host
            .bind_configured(configured.clone(), None, Limits::default())
            .await
            .unwrap();
        assert!(TcpStream::connect(node.local_addr()).await.is_ok());
        if mode < 2 {
            // Authentication and publication are each tested as the earlier
            // authority. The service's separate 120-second run remains live.
            run.wall.store(11_000, Ordering::SeqCst);
        } else {
            let mut value = configured.config().value().clone();
            value.revision += 1;
            value.disabled = true;
            let mut update = run
                .host
                .update_service(ServiceUpdate::Configuration {
                    value: Config::encode(value).unwrap(),
                    expected_revision: 1,
                })
                .unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Some(result) = update.read().unwrap() {
                        result.unwrap();
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await
            .unwrap();
        }
        assert!(configured.check().is_err());
        let deadline = Instant::now() + Duration::from_secs(3);
        while TcpStream::connect(node.local_addr()).await.is_ok() {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            run.host
                .route(run.grant.clone(), "POST", "/still-live")
                .is_ok()
        );
        node.shutdown().await.unwrap();
        run.finish().await;
    }
}

#[tokio::test]
async fn manager_revocation_closes_long_running_listener() {
    let mut run = Running::new(false, Duration::from_secs(2));
    let node = run.bind().await;
    status(&request(node.local_addr(), b"before").await, 202);
    run.manager
        .set_enabled(ID, run.digest, false, run.manager.revision())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while TcpStream::connect(node.local_addr()).await.is_ok() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    node.shutdown().await.unwrap();
    run.finish().await;
}

#[tokio::test]
async fn budgeted_http_node_delivers_final_allowed_response_then_returns_quota() {
    let first = Request::encode(1, &invocation(b"before")).unwrap();
    let encoded = Response::encode(
        &first,
        &Reply {
            status: 202,
            headers: vec![],
            body: b"executed-before".to_vec(),
        },
    )
    .unwrap();
    let exact_bytes = (first.bytes().len() + encoded.len()) as u64;
    // Exhaust cumulative jobs with spare bytes, then exhaust bytes with spare jobs.
    for budget in [
        ServiceRunBudget {
            max_jobs: 1,
            max_bytes: 8192,
        },
        ServiceRunBudget {
            max_jobs: 10,
            max_bytes: exact_bytes,
        },
    ] {
        let run = Running::with_options(false, Duration::from_secs(2), None, Some(budget));
        let node = run.bind().await;
        let response = request(node.local_addr(), b"before").await;
        status(&response, 202);
        assert!(response.ends_with(b"executed-before"));
        status(&request(node.local_addr(), b"after").await, 429);
        assert!(
            run.host
                .route(run.grant.clone(), "POST", "/still-live")
                .is_ok()
        );
        node.shutdown().await.unwrap();
        run.finish().await;
    }
}
