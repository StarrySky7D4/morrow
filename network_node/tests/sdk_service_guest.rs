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
            .bind_io(
                &runtime,
                &instance,
                digest,
                manager.revision(),
                &caps,
                30_000,
                1,
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
