//! Generated original packages through actual TCP, authentication and managed worker.
#![cfg(feature = "plugin-adapter")]
use morrow_core::{
    dispatch::HostRuntime,
    io::Request,
    plugin_package::{Package, catalog::Catalog, io::IoCapability, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_network_node::{
    Limits,
    managed_service::{ManagedNode, ServiceHost},
    server::Principal,
};
use morrow_plugin_runtime::{
    Limits as RuntimeLimits,
    io_jobs::{BrokerRouter, IoWorker, JobLimits, RouteContext, RouterFault},
    manager::Manager,
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
const TOKEN: &str = "synthetic-sdk-service-token-123456789";
struct Deny;
impl BrokerRouter for Deny {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &Request,
    ) -> Result<Vec<u8>, RouterFault> {
        Err(RouterFault::Denied)
    }
}
async fn call(address: std::net::SocketAddr, method: &str, token: &str, body: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let headers = format!(
        "{method} /echo?q=one&q=two HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nCookie: secret\r\nX-Repeat: one\r\nX-Repeat: two\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(body).await.unwrap();
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    bytes
}
fn status(bytes: &[u8], expected: u16) {
    assert!(
        bytes.starts_with(format!("HTTP/1.1 {expected} ").as_bytes()),
        "{}",
        String::from_utf8_lossy(bytes)
    );
}
#[tokio::test]
#[ignore = "requires all three generated service packages; run tool/verify_plugin_service_sdk.py"]
async fn generated_original_packages_serve_seven_methods_and_enforce_grants() {
    for key in [
        "MORROW_SERVICE_PACKAGE_RUST",
        "MORROW_SERVICE_PACKAGE_C",
        "MORROW_SERVICE_PACKAGE_CPP",
    ] {
        let package = Package::decode(
            &std::fs::read(std::env::var_os(key).expect("service package required")).unwrap(),
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, RuntimeLimits::default());
        let id = &package.manifest().package_id;
        let digest = package.digest();
        manager.select(&package, manager.revision()).unwrap();
        manager
            .set_enabled(id, digest, true, manager.revision())
            .unwrap();
        let caps = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
        let mut runtime =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(id, &mut runtime).unwrap();
        assert!(
            manager
                .bind_io(
                    &runtime,
                    &instance,
                    digest,
                    manager.revision(),
                    &caps,
                    30_000,
                    1
                )
                .is_err(),
            "declaration is not approval"
        );
        instance.close(&mut runtime).unwrap();
        manager
            .approve_io(id, digest, caps.clone(), manager.revision())
            .unwrap();
        let instance = manager.connect(id, &mut runtime).unwrap();
        let binding = manager
            .bind_budgeted_service_run(
                &runtime,
                &instance,
                digest,
                manager.revision(),
                &caps,
                30_000,
                1,
                morrow_plugin_runtime::io_binding::ServiceRunBudget {
                    max_jobs: 16,
                    max_bytes: 1024 * 1024,
                },
            )
            .unwrap();
        let grant = ServiceGrant::issue(
            &manager,
            &runtime,
            &instance,
            &binding,
            "service.echo",
            "service.echo",
            2,
        )
        .unwrap();
        let listener = ListenerGrant::issue(&manager, &runtime, &instance, &binding, 2).unwrap();
        let worker = IoWorker::spawn_managed(
            &manager,
            runtime,
            instance,
            binding,
            || 2,
            1,
            JobLimits::new(1, 1024 * 1024, 1024 * 1024).unwrap(),
        )
        .unwrap();
        let host =
            ServiceHost::new(worker, Duration::from_secs(5), Arc::new(|| Box::new(Deny))).unwrap();
        let methods = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];
        let routes = methods
            .iter()
            .map(|method| host.route(grant.clone(), method, "/echo").unwrap())
            .collect();
        let node = ManagedNode::bind(
            "127.0.0.1:0".parse().unwrap(),
            listener,
            vec![
                Principal::new("alice", TOKEN, &["service.echo"], Duration::from_secs(60)).unwrap(),
            ],
            routes,
            Limits::default(),
        )
        .await
        .unwrap();
        status(
            &call(node.local_addr(), "POST", "wrong-token", b"secret").await,
            401,
        );
        for method in methods {
            let body: &[u8] = if method == "HEAD" {
                b""
            } else {
                b"\0\xffbinary"
            };
            let bytes = call(node.local_addr(), method, TOKEN, body).await;
            status(&bytes, 200);
            let split = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
            assert_eq!(&bytes[split..], body, "{key} {method}");
        }
        grant.revoke();
        status(
            &call(node.local_addr(), "POST", TOKEN, b"after revoke").await,
            403,
        );
        node.shutdown().await.unwrap();
        host.shutdown()
            .await
            .unwrap()
            .store_local()
            .integrity_check()
            .unwrap();
    }
}

struct DurableRun {
    manager: Manager,
    host: ServiceHost,
    grant: ServiceGrant,
    node: ManagedNode,
    tick: Arc<std::sync::atomic::AtomicU64>,
}
impl DurableRun {
    async fn open(dir: &std::path::Path, package: &Package, fuel: u64, max_jobs: u64) -> Self {
        use morrow_plugin_runtime::{
            io_binding::ServiceRunBudget, service_history::ServiceJournal,
        };
        use std::sync::atomic::{AtomicU64, Ordering};
        let catalog = Catalog::open(&dir.join("catalog")).unwrap();
        catalog.install(package).unwrap();
        let registry = Registry::open(&dir.join("registry"), catalog).unwrap();
        let mut manager = Manager::new(
            registry,
            RuntimeLimits {
                fuel,
                ..RuntimeLimits::default()
            },
        );
        let id = &package.manifest().package_id;
        let digest = package.digest();
        manager.select(package, manager.revision()).unwrap();
        manager
            .set_enabled(id, digest, true, manager.revision())
            .unwrap();
        let caps = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
        manager
            .approve_io(id, digest, caps.clone(), manager.revision())
            .unwrap();
        let runtime = Store::open(&dir.join("db"), EventBudget::default()).unwrap();
        let mut runtime = HostRuntime::new(runtime).unwrap();
        let instance = manager.connect(id, &mut runtime).unwrap();
        // Each reopen explicitly creates new instance/run/listener grants.
        let binding = manager
            .bind_budgeted_service_run(
                &runtime,
                &instance,
                digest,
                manager.revision(),
                &caps,
                10_001,
                1,
                ServiceRunBudget {
                    max_jobs,
                    max_bytes: 1024 * 1024,
                },
            )
            .unwrap();
        let grant = ServiceGrant::issue(
            &manager,
            &runtime,
            &instance,
            &binding,
            "service.echo",
            "service.echo",
            2,
        )
        .unwrap();
        let listener = ListenerGrant::issue(&manager, &runtime, &instance, &binding, 2).unwrap();
        let tick = Arc::new(AtomicU64::new(2));
        let clock = tick.clone();
        let worker = IoWorker::spawn_managed(
            &manager,
            runtime,
            instance,
            binding,
            move || clock.load(Ordering::SeqCst),
            1,
            JobLimits::new(1, 1024 * 1024, 1024 * 1024).unwrap(),
        )
        .unwrap();
        let host =
            ServiceHost::new(worker, Duration::from_secs(5), Arc::new(|| Box::new(Deny))).unwrap();
        let journal = ServiceJournal::new(
            morrow_core::service_record::Policy {
                namespace: [31; 32],
                retention_ms: 120_000,
            },
            || 1000,
        )
        .unwrap();
        let route = host
            .durable_route(grant.clone(), "POST", "/echo", journal)
            .unwrap();
        let query = host.query_route(&route, "/history").unwrap();
        let node = ManagedNode::bind(
            "127.0.0.1:0".parse().unwrap(),
            listener,
            vec![
                Principal::new("alice", TOKEN, &["service.echo"], Duration::from_secs(60)).unwrap(),
            ],
            vec![route, query],
            Limits::default(),
        )
        .await
        .unwrap();
        Self {
            manager,
            host,
            grant,
            node,
            tick,
        }
    }
    async fn call(&self, path: &str, key: &str, body: &[u8]) -> Vec<u8> {
        let mut stream = TcpStream::connect(self.node.local_addr()).await.unwrap();
        stream.write_all(format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nIdempotency-Key: {key}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        bytes
    }
    async fn close(self) {
        self.node.shutdown().await.unwrap();
        self.host
            .shutdown()
            .await
            .unwrap()
            .store_local()
            .integrity_check()
            .unwrap();
    }
}
#[tokio::test]
#[ignore = "requires generated service-run packages; run tool/verify_plugin_service_sdk.py"]
async fn generated_runs_renew_without_reset_and_reopen_observed_and_unknown_without_resend() {
    use morrow_plugin_runtime::io_binding::ServiceRunBudget;
    use std::sync::atomic::Ordering;
    for key in [
        "MORROW_SERVICE_PACKAGE_RUST",
        "MORROW_SERVICE_PACKAGE_C",
        "MORROW_SERVICE_PACKAGE_CPP",
    ] {
        let bytes =
            std::fs::read(std::env::var_os(key).expect("service package required")).unwrap();
        let package = Package::decode(&bytes).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let run = DurableRun::open(dir.path(), &package, RuntimeLimits::default().fuel, 2).await;
        let body = b"\0\xffretained";
        let response = run.call("/echo", "observed", body).await;
        status(&response, 200);
        assert!(response.ends_with(body));
        status(&run.call("/echo", "observed", body).await, 200);
        let old = run.host.service_run_snapshot().unwrap();
        assert_eq!(old.usage.jobs, 2); // Replay is still an admitted, charged job.
        let budget = ServiceRunBudget {
            max_jobs: 4,
            max_bytes: 2 * 1024 * 1024,
        };
        let renewed = run
            .host
            .renew_service_run(
                &run.manager,
                &run.grant,
                run.manager.revision(),
                old.revision,
                60_001,
                budget,
            )
            .unwrap();
        assert_eq!(old.usage, renewed.usage);
        assert!(
            run.host
                .renew_service_run(
                    &run.manager,
                    &run.grant,
                    run.manager.revision(),
                    old.revision,
                    60_001,
                    budget
                )
                .is_err()
        );
        run.tick.store(31_001, Ordering::SeqCst);
        status(&run.call("/echo", "second", body).await, 200);
        status(&run.call("/echo", "third", body).await, 200);
        let full = run.host.service_run_snapshot().unwrap();
        assert_eq!(full.usage.jobs, 4);
        status(&run.call("/echo", "quota", body).await, 429);
        assert_eq!(run.host.service_run_snapshot(), Some(full));
        run.grant.revoke();
        status(&run.call("/history", "observed", body).await, 403);
        assert!(
            run.host
                .renew_service_run(
                    &run.manager,
                    &run.grant,
                    run.manager.revision(),
                    full.revision,
                    90_001,
                    budget
                )
                .is_err()
        );
        run.close().await;
        // One unit of fuel cannot execute any generated guest. Replay still succeeds.
        let run = DurableRun::open(dir.path(), &package, 1, 16).await;
        let response = run.call("/echo", "observed", body).await;
        status(&response, 200);
        assert!(response.ends_with(body));
        status(&run.call("/echo", "uncertain", body).await, 409);
        run.close().await;
        let run = DurableRun::open(dir.path(), &package, RuntimeLimits::default().fuel, 16).await;
        status(&run.call("/echo", "uncertain", body).await, 409);
        let query = run.call("/history", "uncertain", body).await;
        status(&query, 409);
        assert!(
            String::from_utf8_lossy(&query)
                .to_ascii_lowercase()
                .contains("morrow-service-state: unknown")
        );
        status(&run.call("/echo", "observed", body).await, 200);
        status(
            &run.call("/echo", "observed", b"conflicting body").await,
            409,
        );
        run.close().await;
        let store = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
        assert_eq!(store.pending_usage().unwrap().0, 11); // Three observed histories + one unknown; no resend.
        assert_eq!(
            std::fs::read(std::env::var_os(key).unwrap()).unwrap(),
            bytes
        );
    }
}
