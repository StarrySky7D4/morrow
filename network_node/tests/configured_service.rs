//! Persisted host configuration and approvals -> fresh managed grants -> real HTTP.
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
    service_authority::{self, Record as Authority, proto as authority},
    service_config::{self, Config, proto as config},
    service_record::{self, Policy, RequestRecord},
    store::{EventBudget, Store},
};
use morrow_network_node::{
    Limits,
    managed_service::{ManagedNode, RouterFactory, ServiceHost},
    server::TlsIdentity,
};
use morrow_plugin_runtime::{
    Limits as RuntimeLimits,
    io_jobs::{
        BrokerRouter, IoWorker, JobLimits, RouteContext, RouterFault, ServiceUpdate,
        ServiceUpdateHandle,
    },
    manager::Manager,
    service_authority::{ConfiguredService, ResolvedService},
    service_content::scope_digest,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    net::SocketAddr,
    path::Path,
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
const ID: &str = "org.example.configured.service";
const SERVICE: &str = "service.configured";
const HANDLER: &str = "serve.configured";
const CONFIG: &str = "configured-notes";
const AUTH: [u8; 32] = [42; 32];
const APPROVAL: [u8; 32] = [43; 32];
const TOKEN: &str = "synthetic-configured-principal-bearer-123456789";
const KEY: &str = "configured-operation";
const SECRET: &[u8] = b"retained private configured response";
fn policy() -> Policy {
    Policy {
        namespace: [19; 32],
        retention_ms: 20_000,
    }
}
fn invocation() -> Invocation {
    Invocation {
        service: SERVICE.into(),
        handler: HANDLER.into(),
        principal: "alice".into(),
        method: "POST".into(),
        target: "/approved?q=1".into(),
        headers: vec![Header {
            name: "morrow-content-scope".into(),
            value: scope_digest(&[])
                .unwrap()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into_bytes(),
        }],
        body: b"request".to_vec(),
    }
}
fn request() -> Request {
    Request::encode(
        service_record::call_id(&policy(), KEY, &invocation()).unwrap(),
        &invocation(),
    )
    .unwrap()
}
fn module(slow: bool) -> Vec<u8> {
    let request = request();
    let response = Response::encode(
        &request,
        &Reply {
            status: 200,
            headers: vec![],
            body: SECRET.to_vec(),
        },
    )
    .unwrap();
    let quote = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect::<String>()
    };
    let spin = if slow {
        "i32.const 0 local.set $i (loop $delay local.get $i i32.const 1 i32.add local.tee $i i32.const 5000000 i32.lt_u br_if $delay)"
    } else {
        ""
    };
    wat::parse_str(format!(r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (memory (export "memory") 5)
      (data (i32.const 131072) "{}") (data (i32.const 196608) "{}")
      (func (export "morrow_run") (result i32) (local $i i32)
        i32.const 0 i32.const 131072 call $read i32.const {} i32.ne if unreachable end
        (loop $check
          local.get $i i32.load8_u i32.const 131072 local.get $i i32.add i32.load8_u i32.ne if unreachable end
          local.get $i i32.const 1 i32.add local.tee $i i32.const {} i32.lt_u br_if $check)
        {spin}
        i32.const 196608 i32.const {} call $done drop i32.const 0))"#,
        quote(request.bytes()), quote(&response), request.bytes().len(), request.bytes().len(), response.len())).unwrap()
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
struct Running {
    _manager: Manager,
    host: ServiceHost,
    configured: ConfiguredService,
    router_calls: Arc<AtomicU64>,
    wall: Arc<AtomicU64>,
    digest: [u8; 32],
}
impl Running {
    fn open(path: &Path, fuel: u64, slow: bool, tls_required: bool) -> Self {
        Self::open_with(path, fuel, slow, tls_required, |_, resolved| resolved)
    }
    fn open_with(
        path: &Path,
        fuel: u64,
        slow: bool,
        tls_required: bool,
        restrict: impl FnOnce(&mut Store, ResolvedService) -> ResolvedService,
    ) -> Self {
        let wasm = module(slow);
        let caps = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
        let mut declaration = io::declaration(caps.iter().copied().collect(), vec![HANDLER.into()]);
        declaration.service_schema_sha256 = service::schema_digest().to_vec();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        manifest.required_features.push(io::FEATURE.into());
        manifest.io_declaration = Some(declaration);
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
        let mut store = Store::open(&path.join("db"), EventBudget::default()).unwrap();
        if store.load_service_config(CONFIG).unwrap().is_none() {
            let config = Config::encode(config::Configuration {
                schema_version: service_config::VERSION,
                id: CONFIG.into(),
                revision: 1,
                namespace: policy().namespace.to_vec(),
                retention_ms: policy().retention_ms,
                service: SERVICE.into(),
                handler: HANDLER.into(),
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
            let authentication = Authority::encode(authority::Record {
                schema_version: service_authority::VERSION,
                reference: AUTH.to_vec(),
                revision: 1,
                created_ms: 1000,
                expires_ms: 61_000,
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
                expires_ms: 61_000,
                disabled: false,
                kind: Some(authority::record::Kind::Publication(
                    authority::Publication {
                        config_id: CONFIG.into(),
                        config_sha256: Sha256::digest(config.container()).to_vec(),
                        listen_address: "127.0.0.1:0".into(),
                        tls_required,
                        method: "POST".into(),
                        path: "/approved".into(),
                        query_path: "/approved-history".into(),
                    },
                )),
            })
            .unwrap();
            store
                .save_service_authority_local(&authentication, 0)
                .unwrap();
            store.save_service_authority_local(&publication, 0).unwrap();
        }
        let wall = Arc::new(AtomicU64::new(1000));
        let utc = wall.clone();
        let resolved = ResolvedService::resolve(&mut store, CONFIG, &APPROVAL, move || {
            utc.load(Ordering::SeqCst)
        })
        .unwrap();
        let resolved = restrict(&mut store, resolved);
        let catalog = Catalog::open(&path.join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&path.join("registry"), catalog).unwrap(),
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
        let mut host = HostRuntime::new(store).unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager
            .bind_io(&host, &instance, digest, manager.revision(), &caps, 100, 1)
            .unwrap();
        let configured = resolved
            .issue(&manager, &host, &instance, &binding, 1)
            .unwrap();
        let worker = IoWorker::spawn_managed(
            &manager,
            host,
            instance,
            binding,
            || 1,
            2,
            JobLimits::default(),
        )
        .unwrap();
        let router_calls = Arc::new(AtomicU64::new(0));
        let observed = router_calls.clone();
        let routers: RouterFactory = Arc::new(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Box::new(Deny)
        });
        let host = ServiceHost::new(worker, Duration::from_secs(10), routers).unwrap();
        Self {
            _manager: manager,
            host,
            configured,
            router_calls,
            wall,
            digest,
        }
    }
    async fn bind(&self) -> ManagedNode {
        self.host
            .bind_configured(self.configured.clone(), None, Limits::default())
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
async fn post(address: SocketAddr, path: &str, token: &str) -> Vec<u8> {
    let mut socket = TcpStream::connect(address).await.unwrap();
    let text = format!(
        "POST {path}?q=1 HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nIdempotency-Key: {KEY}\r\nContent-Length: 7\r\nConnection: close\r\n\r\nrequest"
    );
    socket.write_all(text.as_bytes()).await.unwrap();
    let mut output = Vec::new();
    tokio::time::timeout(Duration::from_secs(12), socket.read_to_end(&mut output))
        .await
        .unwrap()
        .unwrap();
    output
}
fn status(bytes: &[u8], expected: u16) {
    assert!(
        bytes.starts_with(format!("HTTP/1.1 {expected} ").as_bytes()),
        "{}",
        String::from_utf8_lossy(bytes)
    );
}
#[tokio::test]
async fn configured_native_factory_uses_persisted_approved_routes_and_recovers_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let run = Running::open(dir.path(), RuntimeLimits::default().fuel, false, false);
    let node = run.bind().await;
    status(&post(node.local_addr(), "/unapproved", TOKEN).await, 404);
    status(
        &post(
            node.local_addr(),
            "/approved",
            "wrong-bearer-token-12345678901234567890",
        )
        .await,
        401,
    );
    status(
        &post(node.local_addr(), "/approved-history", TOKEN).await,
        404,
    );
    let original = post(node.local_addr(), "/approved", TOKEN).await;
    status(&original, 200);
    assert!(original.ends_with(SECRET));
    assert_eq!(run.router_calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
    run.finish().await;
    drop(run);
    let run = Running::open(dir.path(), 1, false, false);
    let node = run.bind().await;
    let response = post(node.local_addr(), "/approved-history", TOKEN).await;
    status(&response, 200);
    assert!(response.ends_with(SECRET));
    assert_eq!(run.router_calls.load(Ordering::SeqCst), 0);
    node.shutdown().await.unwrap();
    run.finish().await;
}
#[tokio::test]
async fn configured_factory_rejects_foreign_worker_and_missing_required_tls_before_consuming_listener()
 {
    let dir = tempfile::tempdir().unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let run = Running::open(dir.path(), RuntimeLimits::default().fuel, false, false);
    let other = Running::open(
        other_dir.path(),
        RuntimeLimits::default().fuel,
        false,
        false,
    );
    assert!(
        other
            .host
            .bind_configured(run.configured.clone(), None, Limits::default())
            .await
            .is_err()
    );
    let node = run.bind().await;
    status(&post(node.local_addr(), "/approved", TOKEN).await, 200);
    node.shutdown().await.unwrap();
    run.finish().await;
    other.finish().await;
    let tls_dir = tempfile::tempdir().unwrap();
    let tls = Running::open(tls_dir.path(), RuntimeLimits::default().fuel, false, true);
    assert!(
        tls.host
            .bind_configured(tls.configured.clone(), None, Limits::default())
            .await
            .is_err()
    );
    tls.finish().await;
}
#[tokio::test]
async fn configured_factory_absolute_expiry_and_queued_edits_revoke_original_authority() {
    for kind in [0, 1, 2] {
        let dir = tempfile::tempdir().unwrap();
        let run = Running::open(dir.path(), RuntimeLimits::default().fuel, false, false);
        assert!(run.configured.check().is_ok());
        if kind == 0 {
            run.wall.store(61_000, Ordering::SeqCst);
        } else {
            let mut writer =
                Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
            if kind == 1 {
                let mut config = writer
                    .load_service_config(CONFIG)
                    .unwrap()
                    .unwrap()
                    .value()
                    .clone();
                config.revision += 1;
                config.disabled = true;
                let value = Config::encode(config).unwrap();
                assert_eq!(
                    writer.save_service_config_local(&value, 1),
                    Err(morrow_core::Error::StorageBusy)
                );
                update_done(
                    run.host
                        .update_service(ServiceUpdate::Configuration {
                            value,
                            expected_revision: 1,
                        })
                        .unwrap(),
                )
                .await;
            } else {
                let mut auth = writer
                    .load_service_authority(&AUTH)
                    .unwrap()
                    .unwrap()
                    .value()
                    .clone();
                auth.revision += 1;
                auth.disabled = true;
                let value = Authority::encode(auth).unwrap();
                assert_eq!(
                    writer.save_service_authority_local(&value, 1),
                    Err(morrow_core::Error::StorageBusy)
                );
                update_done(
                    run.host
                        .update_service(ServiceUpdate::Authority {
                            value,
                            expected_revision: 1,
                        })
                        .unwrap(),
                )
                .await;
            }
        }
        assert!(run.configured.check().is_err());
        assert!(
            run.host
                .bind_configured(run.configured.clone(), None, Limits::default())
                .await
                .is_err()
        );
        run.finish().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_auth_or_config_revocation_after_real_dispatch_never_delivers_late_guest_result()
{
    for change_config in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let run = Running::open(dir.path(), 100_000_000, true, false);
        let node = run.bind().await;
        let address = node.local_addr();
        let mut socket = TcpStream::connect(address).await.unwrap();
        socket.write_all(format!("POST /approved?q=1 HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nIdempotency-Key: {KEY}\r\nContent-Length: 7\r\nConnection: close\r\n\r\nrequest").as_bytes()).await.unwrap();
        let record = RequestRecord::encode(&policy(), KEY, &request(), 1000).unwrap();
        let command = record.command(run.digest).unwrap();
        let writer = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(record) = writer
                    .lookup_io_intent(&command.subject, &command.operation_id)
                    .unwrap()
                {
                    match record.phase() {
                        morrow_core::io_intent::Phase::OutcomeUnknown => break,
                        morrow_core::io_intent::Phase::Prepared => {}
                        phase => {
                            panic!("guest left dispatch boundary before revocation: {phase:?}")
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        let mut auth = writer
            .load_service_authority(&AUTH)
            .unwrap()
            .unwrap()
            .value()
            .clone();
        auth.revision += 1;
        auth.disabled = true;
        let update = if change_config {
            let mut value = run.configured.config().value().clone();
            value.revision += 1;
            value.disabled = true;
            ServiceUpdate::Configuration {
                value: Config::encode(value).unwrap(),
                expected_revision: 1,
            }
        } else {
            ServiceUpdate::Authority {
                value: Authority::encode(auth).unwrap(),
                expected_revision: 1,
            }
        };
        let update = run.host.update_service(update).unwrap();
        assert!(run.configured.check().is_err());
        let mut output = Vec::new();
        // Listener revocation may close an admitted connection instead of replying.
        let _ = tokio::time::timeout(Duration::from_secs(12), socket.read_to_end(&mut output))
            .await
            .unwrap();
        assert!(!output.starts_with(b"HTTP/1.1 200 "));
        assert!(!output.windows(SECRET.len()).any(|part| part == SECRET));
        update_done(update).await;
        node.shutdown().await.unwrap();
        run.finish().await;
    }
}

async fn update_done(mut update: ServiceUpdateHandle) {
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if let Some(result) = update.read().unwrap() {
                result.unwrap();
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn configured_factory_enforces_exact_tls_mode_and_serves_actual_approved_tls_request() {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let identity = || {
        TlsIdentity::from_pem(
            certified.cert.pem().as_bytes(),
            certified.key_pair.serialize_pem().as_bytes(),
        )
        .unwrap()
    };
    let plain_dir = tempfile::tempdir().unwrap();
    let plain = Running::open(
        plain_dir.path(),
        RuntimeLimits::default().fuel,
        false,
        false,
    );
    assert!(
        plain
            .host
            .bind_configured(
                plain.configured.clone(),
                Some(identity()),
                Limits::default()
            )
            .await
            .is_err()
    );
    let plain_node = plain.bind().await;
    status(
        &post(plain_node.local_addr(), "/approved", TOKEN).await,
        200,
    );
    plain_node.shutdown().await.unwrap();
    plain.finish().await;
    let dir = tempfile::tempdir().unwrap();
    let run = Running::open(dir.path(), RuntimeLimits::default().fuel, false, true);
    let node = run
        .host
        .bind_configured(run.configured.clone(), Some(identity()), Limits::default())
        .await
        .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certified.cert.der().clone()).unwrap();
    let tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(tls));
    let socket = TcpStream::connect(node.local_addr()).await.unwrap();
    let mut socket = connector
        .connect(
            rustls::pki_types::ServerName::try_from("localhost").unwrap(),
            socket,
        )
        .await
        .unwrap();
    socket.write_all(format!("POST /approved?q=1 HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nIdempotency-Key: {KEY}\r\nContent-Length: 7\r\nConnection: close\r\n\r\nrequest").as_bytes()).await.unwrap();
    let mut output = Vec::new();
    tokio::time::timeout(Duration::from_secs(12), socket.read_to_end(&mut output))
        .await
        .unwrap()
        .unwrap();
    status(&output, 200);
    assert!(output.ends_with(SECRET));
    node.shutdown().await.unwrap();
    run.finish().await;
}

#[tokio::test]
async fn unrelated_authority_and_configuration_writes_preserve_live_service_and_replay() {
    let dir = tempfile::tempdir().unwrap();
    let run = Running::open(dir.path(), RuntimeLimits::default().fuel, false, false);
    let node = run.bind().await;
    status(&post(node.local_addr(), "/approved", TOKEN).await, 200);
    let reader = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
    let mut auth = reader
        .load_service_authority(&AUTH)
        .unwrap()
        .unwrap()
        .value()
        .clone();
    auth.reference = vec![88; 32];
    auth.disabled = true;
    update_done(
        run.host
            .update_service(ServiceUpdate::Authority {
                value: Authority::encode(auth).unwrap(),
                expected_revision: 0,
            })
            .unwrap(),
    )
    .await;
    let mut config = run.configured.config().value().clone();
    config.id = "unrelated-config".into();
    config.namespace = vec![88; 32];
    config.disabled = true;
    update_done(
        run.host
            .update_service(ServiceUpdate::Configuration {
                value: Config::encode(config).unwrap(),
                expected_revision: 0,
            })
            .unwrap(),
    )
    .await;
    run.configured.check().unwrap();
    let replay = post(node.local_addr(), "/approved-history", TOKEN).await;
    status(&replay, 200);
    assert!(replay.ends_with(SECRET));
    assert_eq!(run.router_calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
    run.finish().await;
}

#[tokio::test]
async fn restrictive_dependency_bounds_and_live_probe_fence_cached_service_results() {
    use morrow_core::store::ServiceAuthorityResource;
    use morrow_plugin_runtime::service_authority::AuthorityDependency;
    use std::sync::atomic::AtomicBool;
    let dir = tempfile::tempdir().unwrap();
    let live = Arc::new(AtomicBool::new(true));
    let probe = live.clone();
    let run = Running::open_with(
        dir.path(),
        RuntimeLimits::default().fuel,
        false,
        false,
        |store, resolved| {
            let guard = store.pin_service_authority().unwrap();
            let lease = store
                .narrow_service_authority(&guard, &[ServiceAuthorityResource::Outbound([91; 32])])
                .unwrap();
            let dependency =
                AuthorityDependency::new(store, lease, move || probe.load(Ordering::SeqCst))
                    .unwrap();
            let bounded = resolved
                .with_dependencies(store, vec![dependency.clone(); 8])
                .unwrap();
            assert!(
                bounded
                    .with_dependencies(store, vec![dependency.clone()])
                    .is_err()
            );
            let resolved = ResolvedService::resolve(store, CONFIG, &APPROVAL, || 1000).unwrap();
            resolved.with_dependencies(store, vec![dependency]).unwrap()
        },
    );
    let node = run.bind().await;
    status(&post(node.local_addr(), "/approved", TOKEN).await, 200);
    status(
        &post(node.local_addr(), "/approved-history", TOKEN).await,
        200,
    );
    assert_eq!(run.router_calls.load(Ordering::SeqCst), 1);
    // Connect before expiry; the listener may close the connection or deny it,
    // but must not deliver the response cached under the formerly valid scope.
    let mut socket = TcpStream::connect(node.local_addr()).await.unwrap();
    live.store(false, Ordering::SeqCst);
    assert!(run.configured.check().is_err());
    assert!(run.configured.grant().check(1).is_err());
    assert!(run.configured.listener().check(1).is_err());
    let _ = socket.write_all(format!("POST /approved-history?q=1 HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nIdempotency-Key: {KEY}\r\nContent-Length: 7\r\nConnection: close\r\n\r\nrequest").as_bytes()).await;
    let mut output = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(12), socket.read_to_end(&mut output))
        .await
        .unwrap();
    assert!(!output.starts_with(b"HTTP/1.1 200 "));
    assert!(!output.windows(SECRET.len()).any(|part| part == SECRET));
    assert_eq!(run.router_calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
    run.finish().await;
}
