//! Containing owners survive listener failure and remain exclusive until join.
#![cfg(feature = "plugin-adapter")]

use morrow_core::{
    dispatch::HostRuntime,
    io::Request,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    service,
    store::Store,
};
use morrow_network_node::{
    Error, Limits,
    managed_service::{ManagedNode, RouterFactory, ServiceHost},
    server::Principal,
};
use morrow_plugin_runtime::{
    io_jobs::{BrokerRouter, HostOwner, IoWorker, JobError, JobLimits, RouteContext, RouterFault},
    manager::Manager,
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

const WAIT: Duration = Duration::from_secs(5);
const TOKEN: &str = "synthetic-owned-service-token-123456789";
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
fn routers() -> RouterFactory {
    Arc::new(|| Box::new(Deny))
}

// Deliberately neither Clone nor Debug: a storage owner cannot be duplicated.
struct Owner {
    runtime: HostRuntime,
    marker: Arc<()>,
    dropped: Arc<AtomicUsize>,
    finished: Arc<AtomicUsize>,
    entered: mpsc::SyncSender<()>,
    release: Option<mpsc::Receiver<()>>,
    fail_maintenance: bool,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.runtime
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.runtime
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.finished.fetch_add(1, Ordering::SeqCst);
        let _ = self.entered.send(());
        if let Some(release) = self.release.take() {
            release
                .recv_timeout(Duration::from_secs(15))
                .map_err(|_| JobError::Unavailable)?;
        }
        if self.fail_maintenance {
            Err(JobError::Unavailable)
        } else {
            Ok(())
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    _manager: Manager,
    worker: IoWorker<Owner>,
    grant: ServiceGrant,
    listener: ListenerGrant,
    marker: Arc<()>,
    dropped: Arc<AtomicUsize>,
    finished: Arc<AtomicUsize>,
    entered: mpsc::Receiver<()>,
    release: mpsc::SyncSender<()>,
}
fn fixture(held: bool, fail_maintenance: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let wasm = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "morrow_run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    let caps = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
    let mut manifest = Package::manifest_for_task("test.owned-service", "1.0.0", &wasm, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    let mut declaration = io::declaration(
        caps.iter().copied().collect(),
        vec!["service.invoke".into()],
    );
    declaration.service_schema_sha256 = service::schema_digest().to_vec();
    manifest.io_declaration = Some(declaration);
    let package = Package::build(manifest, &wasm).unwrap();
    let digest = package.digest();
    let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
    catalog.install(&package).unwrap();
    let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
    let mut manager = Manager::new(registry, Default::default());
    manager.select(&package, manager.revision()).unwrap();
    manager
        .approve_io(
            &package.manifest().package_id,
            digest,
            caps.clone(),
            manager.revision(),
        )
        .unwrap();
    manager
        .set_enabled(
            &package.manifest().package_id,
            digest,
            true,
            manager.revision(),
        )
        .unwrap();
    let mut runtime =
        HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap()).unwrap();
    let instance = manager
        .connect(&package.manifest().package_id, &mut runtime)
        .unwrap();
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
        "test.service",
        "service.invoke",
        2,
    )
    .unwrap();
    let listener = ListenerGrant::issue(&manager, &runtime, &instance, &binding, 2).unwrap();
    let marker = Arc::new(());
    let dropped = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicUsize::new(0));
    let (entered_tx, entered) = mpsc::sync_channel(1);
    let (release, release_rx) = mpsc::sync_channel(1);
    let owner = Owner {
        runtime,
        marker: marker.clone(),
        dropped: dropped.clone(),
        finished: finished.clone(),
        entered: entered_tx,
        release: held.then_some(release_rx),
        fail_maintenance,
    };
    let worker = IoWorker::spawn_managed_owned(
        &manager,
        owner,
        instance,
        binding,
        || 2,
        1,
        JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    Fixture {
        _dir: dir,
        _manager: manager,
        worker,
        grant,
        listener,
        marker,
        dropped,
        finished,
        entered,
        release,
    }
}
fn principals() -> Vec<Principal> {
    vec![Principal::new("alice", TOKEN, &["test.service"], Duration::from_secs(60)).unwrap()]
}

#[tokio::test]
async fn stop_is_nonblocking_and_owned_shutdown_waits_for_maintenance_and_preserves_diagnostics() {
    let f = fixture(true, true);
    let host = ServiceHost::new_owned(f.worker, Duration::from_secs(2), routers()).unwrap();
    // This compiles without Owner: Clone and refers to the same original worker.
    let clone = host.clone();
    host.request_stop().unwrap();
    f.entered.recv_timeout(WAIT).unwrap();
    assert!(host.try_reclaim().unwrap().is_none());
    assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
    let mut shutdown = Box::pin(clone.shutdown_owned());
    // Stay blocked beyond the legacy adapter's five-second cutoff. Pending
    // maintenance must not be reported as a completed owner handoff.
    assert!(
        tokio::time::timeout(Duration::from_millis(5200), shutdown.as_mut())
            .await
            .is_err()
    );
    assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
    f.release.send(()).unwrap();
    let exit = tokio::time::timeout(WAIT, shutdown).await.unwrap().unwrap();
    assert!(Arc::ptr_eq(&exit.owner.marker, &f.marker));
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(exit.maintenance, Err(JobError::Unavailable));
    assert!(exit.instance.is_none());
    assert_eq!(f.finished.load(Ordering::SeqCst), 1);
    assert!(matches!(host.try_reclaim(), Err(Error::Closed)));
    drop((host, clone));
    assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
    drop(exit);
    assert_eq!(f.dropped.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancelling_shutdown_future_does_not_consume_the_original_owner() {
    let f = fixture(true, false);
    let host = ServiceHost::new_owned(f.worker, Duration::from_secs(2), routers()).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(25), host.shutdown_owned())
            .await
            .is_err()
    );
    f.entered.recv_timeout(WAIT).unwrap();
    assert!(host.try_reclaim().unwrap().is_none());
    f.release.send(()).unwrap();
    let exit = tokio::time::timeout(WAIT, host.shutdown_owned())
        .await
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&exit.owner.marker, &f.marker));
    assert_eq!(exit.maintenance, Ok(()));
    assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
    drop(exit);
    assert_eq!(f.dropped.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn invalid_adapter_options_return_the_same_live_worker_for_explicit_reclamation() {
    let f = fixture(false, false);
    let failure = match ServiceHost::new_owned(f.worker, Duration::ZERO, routers()) {
        Err(failure) => failure,
        Ok(_) => panic!("zero timeout accepted"),
    };
    assert_eq!(failure.error, Error::Invalid);
    assert_eq!(
        format!("{failure:?}"),
        "ServiceHostFailure { error: Invalid, .. }"
    );
    assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
    assert_eq!(f.finished.load(Ordering::SeqCst), 0);
    let host = ServiceHost::new_owned(failure.worker, Duration::from_secs(2), routers()).unwrap();
    let exit = tokio::time::timeout(WAIT, host.shutdown_owned())
        .await
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&exit.owner.marker, &f.marker));
    assert_eq!(exit.maintenance, Ok(()));
}

#[tokio::test]
async fn port_conflict_does_not_drop_or_replace_the_containing_owner() {
    let f = fixture(false, false);
    let host = ServiceHost::new_owned(f.worker, Duration::from_secs(2), routers()).unwrap();
    let route = host.route(f.grant.clone(), "POST", "/invoke").unwrap();
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let result = ManagedNode::bind_owned(
        occupied.local_addr().unwrap(),
        f.listener.clone(),
        principals(),
        vec![route],
        Limits::default(),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(f.finished.load(Ordering::SeqCst), 0);
    assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
    assert!(host.try_reclaim().unwrap().is_none());
    let exit = tokio::time::timeout(WAIT, host.shutdown_owned())
        .await
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&exit.owner.marker, &f.marker));
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
}

#[tokio::test]
async fn stopping_or_dropping_listener_keeps_host_reclamation_separate() {
    for explicit in [true, false] {
        let f = fixture(false, false);
        let host = ServiceHost::new_owned(f.worker, Duration::from_secs(2), routers()).unwrap();
        let route = host.route(f.grant.clone(), "POST", "/invoke").unwrap();
        let node = ManagedNode::bind_owned(
            "127.0.0.1:0".parse().unwrap(),
            f.listener.clone(),
            principals(),
            vec![route],
            Limits::default(),
        )
        .await
        .unwrap();
        let address = node.local_addr();
        if explicit {
            node.shutdown().await.unwrap();
        } else {
            drop(node);
        }
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            if let Ok(listener) = std::net::TcpListener::bind(address) {
                drop(listener);
                break;
            }
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(f.finished.load(Ordering::SeqCst), 0);
        assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
        assert!(host.try_reclaim().unwrap().is_none());
        let exit = tokio::time::timeout(WAIT, host.shutdown_owned())
            .await
            .unwrap()
            .unwrap();
        assert!(Arc::ptr_eq(&exit.owner.marker, &f.marker));
        assert_eq!(exit.maintenance, Ok(()));
        assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
        drop(exit);
        assert_eq!(f.dropped.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn cancelled_listener_join_retains_supervision_until_explicit_stop_and_rejoin() {
    let f = fixture(false, false);
    let host = ServiceHost::new_owned(f.worker, Duration::from_secs(2), routers()).unwrap();
    let route = host.route(f.grant.clone(), "POST", "/invoke").unwrap();
    let mut node = ManagedNode::bind_owned(
        "127.0.0.1:0".parse().unwrap(),
        f.listener.clone(),
        principals(),
        vec![route],
        Limits::default(),
    )
    .await
    .unwrap();
    let address = node.local_addr();
    assert!(!node.is_finished());
    // This timeout polls and then drops a genuinely pending join future. Join
    // itself must neither request a stop nor detach the supervisor handle.
    assert!(
        tokio::time::timeout(Duration::from_millis(25), node.join())
            .await
            .is_err()
    );
    assert!(!node.is_finished());
    assert!(tokio::net::TcpStream::connect(address).await.is_ok());
    let before = std::time::Instant::now();
    node.request_stop();
    assert!(before.elapsed() < Duration::from_millis(100));
    tokio::time::timeout(WAIT, node.join())
        .await
        .unwrap()
        .unwrap();
    assert!(node.is_finished());
    assert_eq!(node.join().await, Err(Error::Closed));
    let rebound = std::net::TcpListener::bind(address).unwrap();
    drop(rebound);
    // Actual socket shutdown is not evidence of worker or storage-owner exit.
    assert_eq!(f.finished.load(Ordering::SeqCst), 0);
    assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
    assert!(host.try_reclaim().unwrap().is_none());
    let exit = tokio::time::timeout(WAIT, host.shutdown_owned())
        .await
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&exit.owner.marker, &f.marker));
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
    assert_eq!(f.finished.load(Ordering::SeqCst), 1);
    assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
    drop(exit);
    assert_eq!(f.dropped.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn last_host_drop_only_releases_guards_after_the_worker_finishes() {
    let f = fixture(true, false);
    let host = ServiceHost::new_owned(f.worker, Duration::from_secs(2), routers()).unwrap();
    drop(host);
    f.entered.recv_timeout(WAIT).unwrap();
    assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
    f.release.send(()).unwrap();
    let deadline = tokio::time::Instant::now() + WAIT;
    while f.dropped.load(Ordering::SeqCst) == 0 {
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(f.finished.load(Ordering::SeqCst), 1);
    assert_eq!(f.dropped.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn legacy_bind_accepts_an_untyped_empty_route_list_and_returns_invalid() {
    let f = fixture(false, false);
    let host = ServiceHost::new_owned(f.worker, Duration::from_secs(2), routers()).unwrap();
    let result = ManagedNode::bind(
        "127.0.0.1:0".parse().unwrap(),
        f.listener.clone(),
        principals(),
        vec![],
        Limits::default(),
    )
    .await;
    assert!(matches!(result, Err(Error::Invalid)));
    let route = host.route(f.grant.clone(), "POST", "/invoke").unwrap();
    let node = ManagedNode::bind_owned(
        "127.0.0.1:0".parse().unwrap(),
        f.listener.clone(),
        principals(),
        vec![route],
        Limits::default(),
    )
    .await
    .unwrap();
    node.shutdown().await.unwrap();
    let exit = tokio::time::timeout(WAIT, host.shutdown_owned())
        .await
        .unwrap()
        .unwrap();
    assert!(Arc::ptr_eq(&exit.owner.marker, &f.marker));
    assert_eq!(exit.maintenance, Ok(()));
}
