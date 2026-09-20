//! Real inbound HTTP on the original protected Storage and its managed instance.
//! Listener exit and worker reclamation are deliberately observed separately.
use super::Storage;
use morrow_core::{
    io::Request as IoRequest,
    io_intent::Phase,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    service::{self, Invocation, Reply, Request, Response},
    service_record::{self, Policy, RequestRecord},
};
use morrow_network_node::{
    Limits,
    managed_service::{ManagedNode, RouterFactory, ServiceHost},
    server::Principal,
};
use morrow_plugin_runtime::{
    instance_pool::{Pool, Session as PooledSession},
    io_binding::{IoBinding, ServiceRunBudget},
    io_jobs::{
        BrokerRouter, CommandOwner, HostOwner, IoWorker, JobError, JobLimits, ManagedHostOwner,
        RouteContext, RouterFault, WorkerExit,
    },
    manager::{ManagedInstance, Manager},
    service_history::ServiceJournal,
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{collections::BTreeSet, net::SocketAddr, path::Path, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const ID: &str = "org.example.workbench.storage.service";
const SERVICE: &str = "service.protected";
const HANDLER: &str = "serve.protected";
const KEY: &str = "protected-storage-service-request";
const SECOND_KEY: &str = "protected-storage-service-after-renewal";
const TOKEN: &str = "synthetic-storage-service-token-123456789";
const WALL: u64 = 1_000_000;
const WAIT: Duration = Duration::from_secs(10);

struct Deny;
impl BrokerRouter for Deny {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &IoRequest,
    ) -> std::result::Result<Vec<u8>, RouterFault> {
        Err(RouterFault::Denied)
    }
}

fn policy() -> Policy {
    Policy {
        namespace: [8; 32],
        retention_ms: 30_000,
    }
}

fn invocation() -> Invocation {
    Invocation {
        service: SERVICE.into(),
        handler: HANDLER.into(),
        principal: "alice".into(),
        method: "POST".into(),
        target: "/api".into(),
        headers: vec![],
        body: b"protected-input".to_vec(),
    }
}

fn expected_request() -> Request {
    expected_request_for(KEY)
}
fn expected_request_for(key: &str) -> Request {
    let input = invocation();
    let call = service_record::call_id(&policy(), key, &input).unwrap();
    Request::encode(call, &input).unwrap()
}

fn fixture() -> Vec<u8> {
    let request = expected_request();
    let second = expected_request_for(SECOND_KEY);
    let reply = |request: &Request| {
        Response::encode(
            request,
            &Reply {
                status: 202,
                headers: vec![],
                body: b"protected-output".to_vec(),
            },
        )
        .unwrap()
    };
    let response = reply(&request);
    let second_response = reply(&second);
    let quote = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("\\{byte:02x}"))
            .collect::<String>()
    };
    // Compare every original service-frame byte, including the authenticated
    // principal and deterministic durable call ID, before returning a real reply.
    wat::parse_str(format!(
        r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
          (memory (export "memory") 5)
          (data (i32.const 131072) "{input}")
          (data (i32.const 163840) "{second}")
          (data (i32.const 196608) "{output}")
          (data (i32.const 229376) "{second_output}")
          (func $matches (param $base i32) (param $size i32) (param $actual i32) (result i32)
            (local $i i32)
            local.get $actual local.get $size i32.ne if i32.const 0 return end
            (loop $check
              local.get $i i32.load8_u
              local.get $base local.get $i i32.add i32.load8_u i32.ne if i32.const 0 return end
              local.get $i i32.const 1 i32.add local.tee $i local.get $size i32.lt_u br_if $check)
            i32.const 1)
          (func (export "morrow_run") (result i32) (local $size i32)
            i32.const 0 i32.const 131072 call $read local.set $size
            i32.const 131072 i32.const {input_size} local.get $size call $matches
            if
              i32.const 196608 i32.const {output_size} call $done drop
            else
              i32.const 163840 i32.const {second_size} local.get $size call $matches
              i32.eqz if unreachable end
              i32.const 229376 i32.const {second_output_size} call $done drop
            end i32.const 0))"#,
        input = quote(request.bytes()),
        output = quote(&response),
        input_size = request.bytes().len(),
        output_size = response.len(),
        second = quote(second.bytes()),
        second_output = quote(&second_response),
        second_size = second.bytes().len(),
        second_output_size = second_response.len(),
    ))
    .unwrap()
}

struct Setup {
    dir: tempfile::TempDir,
    owner: Storage,
    manager: Manager,
    pool: Pool,
    pooled: PooledSession,
    instance: ManagedInstance,
    binding: IoBinding,
    grant: ServiceGrant,
    listener: ListenerGrant,
    digest: [u8; 32],
}

impl Setup {
    fn new() -> Self {
        Self::with_run(false)
    }
    fn with_run(budgeted: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut owner = Storage::open_managed(dir.path()).unwrap();
        let wasm = fixture();
        let caps = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
        let mut declaration = io::declaration(caps.iter().copied().collect(), vec![HANDLER.into()]);
        declaration.service_schema_sha256 = service::schema_digest().to_vec();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        manifest.required_features.push(io::FEATURE.into());
        if budgeted {
            declaration.service_run = Some(io::proto::ServiceRunProfile {
                schema_version: 1,
                max_duration_ms: 120_000,
                budget: Some(io::proto::ServiceRunBudget {
                    schema_version: 1,
                    max_jobs: 10,
                    max_bytes: io::MAX_BYTES,
                }),
            });
            manifest.required_features.extend([
                io::SERVICE_RUN_FEATURE.into(),
                io::SERVICE_RUN_BUDGET_FEATURE.into(),
            ]);
        }
        manifest.io_declaration = Some(declaration);
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
        let catalog = Catalog::open(&dir.path().join("service-packages")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("service-registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Default::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, digest, caps.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, digest, true, manager.revision())
            .unwrap();
        let mut pool = Pool::new(&owner, Default::default()).unwrap();
        let revision = manager.revision();
        let pooled = pool
            .start(&mut manager, &mut owner, ID, &[], revision)
            .unwrap();
        let instance = manager.connect(ID, &mut owner).unwrap();
        let binding = if budgeted {
            manager.bind_budgeted_service_run(
                &owner,
                &instance,
                digest,
                manager.revision(),
                &caps,
                10_001,
                1,
                ServiceRunBudget {
                    max_jobs: 1,
                    max_bytes: 8192,
                },
            )
        } else {
            manager.bind_io(&owner, &instance, digest, manager.revision(), &caps, 100, 1)
        }
        .unwrap();
        let grant = ServiceGrant::issue(&manager, &owner, &instance, &binding, SERVICE, HANDLER, 2)
            .unwrap();
        let listener = ListenerGrant::issue(&manager, &owner, &instance, &binding, 2).unwrap();
        Self {
            dir,
            owner,
            manager,
            pool,
            pooled,
            instance,
            binding,
            grant,
            listener,
            digest,
        }
    }
}

fn assert_guards(root: &Path, database: &Path) {
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
    assert!(matches!(Sealer::new(key), Err(LeaseError::Busy)));
}

fn host(
    manager: &Manager,
    owner: Storage,
    instance: ManagedInstance,
    binding: IoBinding,
) -> ServiceHost<Storage> {
    let worker = IoWorker::spawn_managed_owned(
        manager,
        owner,
        instance,
        binding,
        || 2,
        1,
        JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let factory: RouterFactory = Arc::new(|| Box::new(Deny));
    ServiceHost::new_owned(worker, Duration::from_secs(2), factory).unwrap()
}

async fn bind<O: HostOwner>(
    host: &ServiceHost<O>,
    grant: ServiceGrant,
    listener: ListenerGrant,
    address: SocketAddr,
) -> morrow_network_node::Result<ManagedNode> {
    let journal = ServiceJournal::new(policy(), || WALL).unwrap();
    let route = host.durable_route(grant, "POST", "/api", journal).unwrap();
    ManagedNode::bind_owned(
        address,
        listener,
        vec![Principal::new("alice", TOKEN, &[SERVICE], Duration::from_secs(10)).unwrap()],
        vec![route],
        Limits::default(),
    )
    .await
}

async fn request(address: SocketAddr) -> Vec<u8> {
    request_with_key(address, KEY).await
}
async fn request_with_key(address: SocketAddr, key: &str) -> Vec<u8> {
    let mut socket = TcpStream::connect(address).await.unwrap();
    let body = invocation().body;
    let headers = format!(
        "POST /api HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nIdempotency-Key: {key}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    socket.write_all(headers.as_bytes()).await.unwrap();
    socket.write_all(&body).await.unwrap();
    let mut response = vec![];
    tokio::time::timeout(WAIT, socket.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(
        response.starts_with(b"HTTP/1.1 202 "),
        "{}",
        String::from_utf8_lossy(&response)
    );
    assert!(response.ends_with(b"protected-output"));
    response
}

async fn reclaim(host: &ServiceHost<Storage>) -> WorkerExit<Storage> {
    tokio::time::timeout(WAIT, async {
        loop {
            if let Some(exit) = host.try_reclaim().unwrap() {
                return exit;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("actual worker exit must return the original Storage")
}

fn actual_service(fail_seal: bool) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let Setup {
            dir,
            mut owner,
            manager,
            mut pool,
            pooled,
            instance,
            binding,
            grant,
            listener,
            digest,
        } = Setup::new();
        let original = owner.binding();
        let trust = owner.session.trust();
        let database = owner
            ._registry
            .as_ref()
            .unwrap()
            .selected_database()
            .unwrap();
        let registry_revision = manager.revision();
        #[cfg(feature = "fault-injection")]
        if fail_seal {
            owner.fail_next_seal_for_test();
        }
        let host = host(&manager, owner, instance, binding);
        assert_guards(dir.path(), &database);
        assert!(host.try_reclaim().unwrap().is_none());
        let node = bind(&host, grant, listener, "127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let response = request(node.local_addr()).await;
        assert_guards(dir.path(), &database);
        node.shutdown().await.unwrap();
        // Closing the socket supervisor is not the same as joining the original
        // owner thread. The live host handle still holds every protected lease.
        assert!(host.try_reclaim().unwrap().is_none());
        assert_guards(dir.path(), &database);
        host.request_stop().unwrap();
        let exit = reclaim(&host).await;
        assert!(exit.result.is_ok());
        assert!(exit.disconnect.is_ok() && exit.instance.is_none());
        assert_eq!(exit.maintenance.is_err(), fail_seal);
        owner = exit.owner;
        assert_eq!(owner.binding(), original);
        assert_eq!(owner.session.trust().id, trust.id);
        assert_eq!(owner.session.trust().key, trust.key);
        assert_eq!(
            owner
                ._registry
                .as_ref()
                .unwrap()
                .selected_database()
                .unwrap(),
            database
        );
        assert_eq!(manager.revision(), registry_revision);
        assert!(pool.root(&pooled).is_ok());
        assert_guards(dir.path(), &database);
        let command = RequestRecord::encode(&policy(), KEY, &expected_request(), WALL)
            .unwrap()
            .command(digest)
            .unwrap();
        let record = owner
            .store_local()
            .lookup_io_intent(&command.subject, &command.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(record.phase(), Phase::Observed);
        if fail_seal {
            assert!(owner.warning().is_some());
            assert!(owner.store_local().pending_usage().unwrap().0 > 0);
            // The observed HTTP result and exact original owner survive a seal
            // failure. Explicit maintenance does not execute the guest again.
            assert!(response.ends_with(b"protected-output"));
            owner.flush_pending().unwrap();
        }
        assert_eq!(owner.store_local().pending_usage().unwrap(), (0, 0));
        morrow_audit::verify(
            &owner.store_local().last_sealed_segment().unwrap().unwrap(),
            &trust,
        )
        .unwrap();
        owner.store_local().integrity_check().unwrap();
        pool.close_all(&mut owner).unwrap();
        drop(owner);
        let reopened = Storage::open_managed(dir.path()).unwrap();
        assert_eq!(reopened.session.trust().id, trust.id);
        assert_eq!(reopened.session.trust().key, trust.key);
        assert_eq!(
            reopened
                .store_local()
                .lookup_io_intent(&command.subject, &command.operation_id)
                .unwrap()
                .unwrap()
                .phase(),
            Phase::Observed
        );
    });
}

#[test]
fn protected_service_returns_original_owner_after_listener_and_worker_exit() {
    actual_service(false);
}

#[cfg(feature = "fault-injection")]
#[test]
fn protected_service_seal_failure_preserves_observed_reply_and_repairable_owner() {
    actual_service(true);
}

#[test]
fn occupied_port_leaves_original_protected_owner_reclaimable() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let Setup {
            dir,
            owner,
            manager,
            mut pool,
            pooled,
            instance,
            binding,
            grant,
            listener,
            ..
        } = Setup::new();
        let original = owner.binding();
        let trust = owner.session.trust();
        let database = owner
            ._registry
            .as_ref()
            .unwrap()
            .selected_database()
            .unwrap();
        let host = host(&manager, owner, instance, binding);
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(
            bind(&host, grant, listener, occupied.local_addr().unwrap())
                .await
                .is_err()
        );
        assert_guards(dir.path(), &database);
        let exit = tokio::time::timeout(WAIT, host.shutdown_owned())
            .await
            .unwrap()
            .unwrap();
        assert!(exit.result.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok());
        assert!(exit.instance.is_none());
        let mut owner = exit.owner;
        assert_eq!(owner.binding(), original);
        assert_eq!(owner.session.trust().id, trust.id);
        assert_eq!(owner.session.trust().key, trust.key);
        assert!(pool.root(&pooled).is_ok());
        assert_eq!(owner.store_local().pending_usage().unwrap(), (0, 0));
        assert_guards(dir.path(), &database);
        pool.close_all(&mut owner).unwrap();
        drop(owner);
        let reopened = Storage::open_managed(dir.path()).unwrap();
        assert_eq!(reopened.session.trust().id, trust.id);
    });
}

/// A qualification owner, not the application WorkbenchState. It moves the
/// real storage guards, pool lease and Manager together without cloning them.
struct CommandStorage {
    storage: Storage,
    manager: Manager,
    pool: Pool,
    pooled: PooledSession,
    digest: [u8; 32],
    commands: usize,
}
impl HostOwner for CommandStorage {
    fn runtime(&self) -> &morrow_core::dispatch::HostRuntime {
        &self.storage
    }
    fn runtime_mut(&mut self) -> &mut morrow_core::dispatch::HostRuntime {
        &mut self.storage
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        HostOwner::prepare_io(&mut self.storage)
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        HostOwner::finish_io(&mut self.storage)
    }
}
impl ManagedHostOwner for CommandStorage {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.manager)
    }
}
impl CommandOwner for CommandStorage {
    fn command(&mut self, input: Vec<u8>) -> Result<Vec<u8>, JobError> {
        if input != b"lookup-original-intent" {
            return Err(JobError::InvalidOptions);
        }
        // This diagnostic checks the actual same pooled root and persisted
        // service outcome. It does not bypass content mutation APIs.
        self.pool
            .root(&self.pooled)
            .map_err(|_| JobError::Unavailable)?;
        let command = RequestRecord::encode(&policy(), KEY, &expected_request(), WALL)
            .and_then(|record| record.command(self.digest))
            .map_err(|_| JobError::Unavailable)?;
        let record = self
            .storage
            .store_local()
            .lookup_io_intent(&command.subject, &command.operation_id)
            .map_err(|_| JobError::Unavailable)?;
        self.commands += 1;
        Ok(match record {
            None => b"missing".to_vec(),
            Some(record) if record.phase() == Phase::Observed => b"observed".to_vec(),
            Some(_) => b"other".to_vec(),
        })
    }
}

#[test]
fn reserved_commands_share_protected_storage_pool_and_manager_with_live_http() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let Setup {
            dir,
            owner,
            manager,
            pool,
            pooled,
            instance,
            binding,
            grant,
            listener,
            digest,
        } = Setup::new();
        let original = owner.binding();
        let trust = owner.session.trust();
        let database = owner
            ._registry
            .as_ref()
            .unwrap()
            .selected_database()
            .unwrap();
        let revision = manager.revision();
        let owner = CommandStorage {
            storage: owner,
            manager,
            pool,
            pooled,
            digest,
            commands: 0,
        };
        let worker = IoWorker::spawn_managed_owner(
            owner,
            instance,
            binding,
            || 2,
            1,
            JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
        )
        .unwrap();
        let factory: RouterFactory = Arc::new(|| Box::new(Deny));
        let host = ServiceHost::new_owned(worker, Duration::from_secs(2), factory).unwrap();
        let node = bind(&host, grant, listener, "127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        for expected in [b"missing".as_slice(), b"observed".as_slice()] {
            let mut handle = host
                .submit_owner_command(b"lookup-original-intent".to_vec(), 64)
                .unwrap();
            let reply = tokio::time::timeout(WAIT, async {
                loop {
                    if let Some(reply) = handle.read().unwrap() {
                        break reply;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(reply, expected);
            assert_guards(dir.path(), &database);
            request(node.local_addr()).await;
        }
        node.shutdown().await.unwrap();
        let exit = tokio::time::timeout(WAIT, host.shutdown_owned())
            .await
            .unwrap()
            .unwrap();
        assert!(exit.result.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok());
        assert!(exit.instance.is_none());
        let mut owner = exit.owner;
        assert_eq!(owner.commands, 2);
        assert_eq!(owner.storage.binding(), original);
        assert_eq!(owner.storage.session.trust().id, trust.id);
        assert_eq!(owner.storage.session.trust().key, trust.key);
        assert_eq!(owner.manager.revision(), revision);
        assert!(owner.pool.root(&owner.pooled).is_ok());
        assert_guards(dir.path(), &database);
        owner.pool.close_all(&mut owner.storage).unwrap();
        drop(owner);
        let reopened = Storage::open_managed(dir.path()).unwrap();
        assert_eq!(reopened.session.trust().id, trust.id);
        reopened.store_local().integrity_check().unwrap();
    });
}

#[test]
fn owned_manager_renews_original_protected_service_before_second_real_execution() {
    use std::sync::atomic::{AtomicU64, Ordering};
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let Setup {
            dir,
            owner,
            manager,
            pool,
            pooled,
            instance,
            binding,
            grant,
            listener,
            digest,
        } = Setup::with_run(true);
        let original = owner.binding();
        let trust = owner.session.trust();
        let database = owner
            ._registry
            .as_ref()
            .unwrap()
            .selected_database()
            .unwrap();
        let registry_revision = manager.revision();
        let owner = CommandStorage {
            storage: owner,
            manager,
            pool,
            pooled,
            digest,
            commands: 0,
        };
        let tick = Arc::new(AtomicU64::new(2));
        let clock = tick.clone();
        let worker = IoWorker::spawn_managed_owner(
            owner,
            instance,
            binding,
            move || clock.load(Ordering::SeqCst),
            1,
            JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
        )
        .unwrap();
        let factory: RouterFactory = Arc::new(|| Box::new(Deny));
        let host = ServiceHost::new_owned(worker, Duration::from_secs(2), factory).unwrap();
        let node = bind(
            &host,
            grant.clone(),
            listener.clone(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .await
        .unwrap();
        let renewal_grant = grant.bound_to_listener(&listener).unwrap();
        request(node.local_addr()).await;
        let before = host.service_run_snapshot().unwrap();
        assert_eq!(before.usage.jobs, 1);
        assert!(before.usage.bytes > 0);
        let mut renewal = host
            .queue_service_run_renewal(
                renewal_grant.clone(),
                registry_revision,
                before.revision,
                60_001,
                ServiceRunBudget {
                    max_jobs: 2,
                    max_bytes: 16384,
                },
            )
            .unwrap();
        let renewed = tokio::time::timeout(WAIT, async {
            loop {
                if let Some(result) = renewal.read().unwrap() {
                    break result.unwrap();
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(renewed.revision, before.revision + 1);
        assert_eq!(renewed.usage, before.usage);
        tick.store(15_001, Ordering::SeqCst);
        // A distinct durable key/call ID forces a second actual Wasm execution;
        // the first response cannot be used as a cache hit for this request.
        assert_ne!(
            expected_request().call_id(),
            expected_request_for(SECOND_KEY).call_id()
        );
        request_with_key(node.local_addr(), SECOND_KEY).await;
        let after = host.service_run_snapshot().unwrap();
        assert_eq!(after.usage.jobs, 2);
        assert!(after.usage.bytes > before.usage.bytes);
        assert_guards(dir.path(), &database);
        // Socket shutdown and authority revocation are separate operations.
        // Explicitly retire the original listener grant before stopping it.
        listener.revoke();
        node.shutdown().await.unwrap();
        // Admission only reserves the owner lane. Listener revocation is also
        // checked against the original grant when the typed command executes.
        match host.queue_service_run_renewal(
            renewal_grant,
            registry_revision,
            after.revision,
            90_001,
            ServiceRunBudget {
                max_jobs: 3,
                max_bytes: 32768,
            },
        ) {
            Err(morrow_plugin_runtime::io_jobs::OwnerCommandError::Closed) => {}
            Ok(mut handle) => {
                tokio::time::timeout(WAIT, async {
                    loop {
                        if let Some(result) = handle.read().unwrap() {
                            assert!(result.is_err());
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(2)).await;
                    }
                })
                .await
                .unwrap();
            }
            Err(error) => panic!("unexpected renewal admission failure: {error:?}"),
        }
        assert_eq!(host.service_run_snapshot(), Some(after));
        let exit = tokio::time::timeout(WAIT, host.shutdown_owned())
            .await
            .unwrap()
            .unwrap();
        assert!(exit.result.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok());
        assert!(exit.instance.is_none());
        let mut owner = exit.owner;
        assert_eq!(owner.commands, 0); // Typed renewal did not call the byte handler.
        assert_eq!(owner.storage.binding(), original);
        assert_eq!(owner.storage.session.trust().id, trust.id);
        assert_eq!(owner.storage.session.trust().key, trust.key);
        assert_eq!(owner.manager.revision(), registry_revision);
        assert!(owner.pool.root(&owner.pooled).is_ok());
        assert_guards(dir.path(), &database);
        for key in [KEY, SECOND_KEY] {
            let command = RequestRecord::encode(&policy(), key, &expected_request_for(key), WALL)
                .unwrap()
                .command(digest)
                .unwrap();
            assert_eq!(
                owner
                    .storage
                    .store_local()
                    .lookup_io_intent(&command.subject, &command.operation_id)
                    .unwrap()
                    .unwrap()
                    .phase(),
                Phase::Observed
            );
        }
        owner.pool.close_all(&mut owner.storage).unwrap();
        drop(owner);
        let reopened = Storage::open_managed(dir.path()).unwrap();
        assert_eq!(reopened.session.trust().id, trust.id);
        reopened.store_local().integrity_check().unwrap();
    });
}
