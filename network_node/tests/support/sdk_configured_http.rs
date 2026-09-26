//! Same original public SDK guests through saved configuration and restrictive
//! endpoint/credential dependencies. No synthetic bypass of the application wiring.
use super::*;
use morrow_core::{
    service_authority::{self, Record as Authority, proto as authority},
    service_config::{self, Config, proto as config},
};
use morrow_network_node::{
    Error,
    service_outbound::{PreparedService, SelectedService, ServiceEndpointSelection},
};
use morrow_plugin_runtime::{
    io_jobs::{ServiceUpdate, ServiceUpdateHandle},
    service_authority::{ConfiguredService, ResolvedService},
};
use sha2::{Digest, Sha256};
const CONFIG: &str = "public-sdk-configured";
const AUTH: [u8; 32] = [42; 32];
const PUBLICATION: [u8; 32] = [43; 32];
fn choice(revision: u64) -> ServiceEndpointSelection {
    ServiceEndpointSelection {
        reference: ENDPOINT,
        revision,
    }
}
fn endpoint_record(package: &Package, origin: &str) -> Record {
    Record::encode(outbound::Record {
        schema_version: outbound_authority::VERSION,
        reference: ENDPOINT.to_vec(),
        revision: 1,
        created_ms: 1000,
        expires_ms: 121000,
        disabled: false,
        kind: Some(outbound::record::Kind::Endpoint(outbound::Endpoint {
            package_id: package.manifest().package_id.clone(),
            package_sha256: package.digest().to_vec(),
            origin: origin.into(),
            profile: 2,
            methods: vec!["POST".into()],
            credential_reference: CREDENTIAL.to_vec(),
            root_certificate: vec![],
            max_request_bytes: 65536,
            max_response_bytes: 65536,
            max_header_bytes: 16384,
            max_concurrent: 1,
            timeout_ms: 10000,
            max_frame_bytes: 131072,
        })),
    })
    .unwrap()
}
fn seed(store: &mut Store, package: &Package, origin: &str) {
    if store.load_service_config(CONFIG).unwrap().is_some() {
        return;
    }
    let config = Config::encode(config::Configuration {
        schema_version: service_config::VERSION,
        id: CONFIG.into(),
        revision: 1,
        namespace: vec![51; 32],
        retention_ms: 120000,
        service: SERVICE.into(),
        handler: SERVICE.into(),
        package_sha256: package.digest().to_vec(),
        disabled: false,
        principals: vec![config::Principal {
            id: "alice".into(),
            authentication_reference: AUTH.to_vec(),
            content_scopes: vec![],
        }],
        approval_references: vec![PUBLICATION.to_vec()],
    })
    .unwrap();
    store.save_service_config_local(&config, 0).unwrap();
    for (reference, kind) in [
        (
            AUTH,
            authority::record::Kind::Authentication(authority::Authentication {
                principal_id: "alice".into(),
                token_sha256: Sha256::digest(TOKEN.as_bytes()).to_vec(),
            }),
        ),
        (
            PUBLICATION,
            authority::record::Kind::Publication(authority::Publication {
                config_id: CONFIG.into(),
                config_sha256: config.digest().to_vec(),
                listen_address: "127.0.0.1:0".into(),
                tls_required: false,
                method: "POST".into(),
                path: "/forward".into(),
                query_path: "/history".into(),
            }),
        ),
    ] {
        store
            .save_service_authority_local(
                &Authority::encode(authority::Record {
                    schema_version: service_authority::VERSION,
                    reference: reference.to_vec(),
                    revision: 1,
                    created_ms: 1000,
                    expires_ms: 121000,
                    disabled: false,
                    kind: Some(kind),
                })
                .unwrap(),
                0,
            )
            .unwrap();
    }
    let credential = Record::encode(outbound::Record {
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
    .unwrap();
    store.save_outbound_authority_local(&credential, 0).unwrap();
    store
        .save_outbound_authority_local(&endpoint_record(package, origin), 0)
        .unwrap();
}
struct Setup {
    manager: Manager,
    runtime: HostRuntime,
    package: Package,
}
impl Setup {
    fn open(path: &Path, package: &Package, origin: &str, fuel: u64) -> Self {
        let mut store = Store::open(&path.join("db"), EventBudget::default()).unwrap();
        seed(&mut store, package, origin);
        let catalog = Catalog::open(&path.join("catalog")).unwrap();
        catalog.install(package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&path.join("registry"), catalog).unwrap(),
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
        manager
            .approve_io(
                id,
                digest,
                BTreeSet::from([
                    IoCapability::HttpListen,
                    IoCapability::HttpPublish,
                    IoCapability::HttpRequest,
                    IoCapability::CredentialUse,
                ]),
                manager.revision(),
            )
            .unwrap();
        Self {
            manager,
            runtime: HostRuntime::new(store).unwrap(),
            package: Package::decode(package.archive()).unwrap(),
        }
    }
    fn selected(&mut self, choices: &[ServiceEndpointSelection]) -> Result<SelectedService, Error> {
        let store = self.runtime.store_local_mut();
        let resolved = ResolvedService::resolve(store, CONFIG, &PUBLICATION, || 1000)
            .map_err(|_| Error::Denied)?;
        SelectedService::resolve(store, resolved, choices, || 1000)
    }
    fn prepare(
        &mut self,
        choices: &[ServiceEndpointSelection],
        provider_calls: &Arc<AtomicUsize>,
    ) -> (
        PreparedService,
        morrow_plugin_runtime::manager::ManagedInstance,
        morrow_plugin_runtime::io_binding::IoBinding,
    ) {
        let selected = self.selected(choices).unwrap();
        let caps = selected.capabilities();
        let instance = self
            .manager
            .connect(&self.package.manifest().package_id, &mut self.runtime)
            .unwrap();
        let binding = self
            .manager
            .bind_budgeted_service_run(
                &self.runtime,
                &instance,
                self.package.digest(),
                self.manager.revision(),
                &caps,
                120001,
                1,
                ServiceRunBudget {
                    max_jobs: 64,
                    max_bytes: 4194304,
                },
            )
            .unwrap();
        let calls = provider_calls.clone();
        let service = selected
            .approve(
                &self.manager,
                &self.runtime,
                &instance,
                &binding,
                1,
                tokio::runtime::Handle::current(),
                || Ok([23; 32]),
                move |record| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(record.reference(), CREDENTIAL);
                    Credential::header("20".repeat(32).into_bytes(), "authorization", SECRET)
                },
            )
            .unwrap();
        (service, instance, binding)
    }
    fn worker(
        self,
        instance: morrow_plugin_runtime::manager::ManagedInstance,
        binding: morrow_plugin_runtime::io_binding::IoBinding,
    ) -> (Manager, IoWorker<Owner>) {
        let original = self.runtime.binding();
        let worker = IoWorker::spawn_managed_owned(
            &self.manager,
            Owner {
                runtime: self.runtime,
                original,
            },
            instance,
            binding,
            || 1,
            1,
            JobLimits::new(1, 1048576, 1048576).unwrap(),
        )
        .unwrap();
        (self.manager, worker)
    }
}
struct ConfiguredRun {
    _manager: Manager,
    host: ServiceHost<Owner>,
    configured: ConfiguredService,
    node: ManagedNode,
}
impl ConfiguredRun {
    async fn open(path: &Path, package: &Package, origin: &str, fuel: u64, revision: u64) -> Self {
        let mut setup = Setup::open(path, package, origin, fuel);
        let calls = Arc::new(AtomicUsize::new(0));
        let (service, instance, binding) = setup.prepare(&[choice(revision)], &calls);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let (manager, worker) = setup.worker(instance, binding);
        let (host, configured) = service.attach(worker, WAIT).unwrap();
        let node = host
            .bind_configured(
                configured.clone(),
                None,
                Limits {
                    timeout: WAIT,
                    ..Limits::default()
                },
            )
            .await
            .unwrap();
        Self {
            _manager: manager,
            host,
            configured,
            node,
        }
    }
    async fn close(self) {
        self.node.shutdown().await.unwrap();
        let exit = self.host.shutdown_owned().await.unwrap();
        assert_eq!(exit.result, Ok(()));
        assert_eq!(exit.maintenance, Ok(()));
        assert_eq!(exit.disconnect, Ok(()));
        assert_eq!(exit.owner.runtime.binding(), exit.owner.original);
        exit.owner.runtime.store_local().integrity_check().unwrap();
    }
}
async fn update_done(mut update: ServiceUpdateHandle) {
    tokio::time::timeout(WAIT, async {
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
fn revised(store: &Store, reference: &[u8; 32], disabled: bool) -> Record {
    let mut record = store
        .load_outbound_authority(reference)
        .unwrap()
        .unwrap()
        .value()
        .clone();
    record.revision += 1;
    record.disabled = disabled;
    Record::encode(record).unwrap()
}
async fn denied_socket(mut socket: TcpStream, target: &str) {
    let request = format!(
        "POST {target} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nIdempotency-Key: once\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        BODY.len()
    );
    let _ = socket.write_all(request.as_bytes()).await;
    let _ = socket.write_all(BODY).await;
    let mut response = vec![];
    let _ = tokio::time::timeout(WAIT, socket.read_to_end(&mut response))
        .await
        .unwrap();
    assert!(!response.starts_with(b"HTTP/1.1 201 "));
    assert!(!response.windows(BODY.len()).any(|w| w == BODY));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires three original service-http packages"]
async fn configured_originals_cached_results_obey_selected_endpoint_and_credential_revocation() {
    for package in packages() {
        for reference in [ENDPOINT, CREDENTIAL] {
            let dir = tempfile::tempdir().unwrap();
            let mut upstream = Upstream::open().await;
            let run = ConfiguredRun::open(
                dir.path(),
                &package,
                &upstream.origin,
                RuntimeLimits::default().fuel,
                1,
            )
            .await;
            let addr = run.node.local_addr();
            let mut incoming =
                tokio::spawn(async move { call(addr, "/forward", "once", BODY).await });
            tokio::select! {_=upstream.entered()=>{}, reply=&mut incoming=>panic!("ended before outbound: {}",String::from_utf8_lossy(&reply.unwrap()))}
            upstream.finish(true);
            let reply = incoming.await.unwrap();
            status(&reply, 201);
            assert!(reply.ends_with(BODY));
            run.close().await;
            // Too little fuel to execute any guest, yet original stored result is available.
            let run = ConfiguredRun::open(dir.path(), &package, &upstream.origin, 1, 1).await;
            for target in ["/forward", "/history"] {
                let reply = call(run.node.local_addr(), target, "once", BODY).await;
                status(&reply, 201);
                assert!(reply.ends_with(BODY));
            }
            let writer =
                Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
            // Updating an unselected record must leave this service and cached result live.
            let mut unrelated = endpoint_record(&package, &upstream.origin).value().clone();
            unrelated.reference = vec![99; 32];
            update_done(
                run.host
                    .update_service(ServiceUpdate::Outbound {
                        value: Record::encode(unrelated).unwrap(),
                        expected_revision: 0,
                    })
                    .unwrap(),
            )
            .await;
            assert!(run.configured.check().is_ok());
            status(
                &call(run.node.local_addr(), "/history", "once", BODY).await,
                201,
            );
            let cached = TcpStream::connect(run.node.local_addr()).await.unwrap();
            let direct = TcpStream::connect(run.node.local_addr()).await.unwrap();
            let changed = revised(&writer, &reference, true);
            let update = run
                .host
                .update_service(ServiceUpdate::Outbound {
                    value: changed,
                    expected_revision: 1,
                })
                .unwrap();
            assert!(
                run.configured.check().is_err(),
                "queue admission immediately fences cached material"
            );
            update_done(update).await;
            denied_socket(cached, "/history").await;
            denied_socket(direct, "/forward").await;
            assert!(
                run.host
                    .bind_configured(run.configured.clone(), None, Limits::default())
                    .await
                    .is_err()
            );
            assert_eq!(upstream.calls.load(Ordering::SeqCst), 1);
            drop(writer);
            run.close().await;
            let mut setup = Setup::open(
                dir.path(),
                &package,
                &upstream.origin,
                RuntimeLimits::default().fuel,
            );
            assert!(
                setup
                    .selected(&[choice(if reference == ENDPOINT { 2 } else { 1 })])
                    .is_err(),
                "disabled resource cannot revive on reopen"
            );
            setup.runtime.store_local().integrity_check().unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires three original service-http packages"]
async fn configured_originals_waiting_revocation_or_stop_never_redelivers_or_resends() {
    for package in packages() {
        for reference in [Some(ENDPOINT), Some(CREDENTIAL), None] {
            let dir = tempfile::tempdir().unwrap();
            let mut upstream = Upstream::open().await;
            let run = ConfiguredRun::open(
                dir.path(),
                &package,
                &upstream.origin,
                RuntimeLimits::default().fuel,
                1,
            )
            .await;
            let addr = run.node.local_addr();
            let mut incoming =
                tokio::spawn(async move { call(addr, "/forward", "once", BODY).await });
            tokio::select! {_=upstream.entered()=>{}, reply=&mut incoming=>panic!("ended before outbound: {}",String::from_utf8_lossy(&reply.unwrap()))}
            let mut local = run
                .host
                .submit_owner_command(b"configured owner remains available".to_vec(), 64)
                .unwrap();
            tokio::time::timeout(WAIT, async {
                loop {
                    if let Some(bytes) = local.read().unwrap() {
                        assert_eq!(bytes, b"configured owner remains available");
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await
            .unwrap();
            if let Some(reference) = reference {
                let writer =
                    Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
                update_done(
                    run.host
                        .update_service(ServiceUpdate::Outbound {
                            value: revised(&writer, &reference, true),
                            expected_revision: 1,
                        })
                        .unwrap(),
                )
                .await;
                assert!(run.configured.check().is_err());
            } else {
                run.host.request_stop().unwrap();
                run.node.request_stop();
            }
            let reply = tokio::time::timeout(WAIT, incoming).await.unwrap().unwrap();
            assert!(!reply.starts_with(b"HTTP/1.1 201 "));
            assert!(!reply.windows(BODY.len()).any(|w| w == BODY));
            run.close().await;
            upstream.finish(true);
            {
                let mut store =
                    Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
                assert_eq!(
                    store.card("sdk-http-local").unwrap().unwrap().body(),
                    b"configured owner remains available"
                );
                if let Some(reference) = reference {
                    store
                        .save_outbound_authority_local(&revised(&store, &reference, false), 2)
                        .unwrap();
                }
            }
            let run = ConfiguredRun::open(
                dir.path(),
                &package,
                &upstream.origin,
                RuntimeLimits::default().fuel,
                if reference == Some(ENDPOINT) { 3 } else { 1 },
            )
            .await;
            // Unchanged policy retains Unknown; changed revision conflicts with the
            // old scope. Neither is permission to dispatch the old key again.
            for target in ["/forward", "/history"] {
                status(
                    &call(run.node.local_addr(), target, "once", BODY).await,
                    409,
                );
            }
            assert_eq!(upstream.calls.load(Ordering::SeqCst), 1);
            run.close().await;
        }
    }
}

#[tokio::test]
#[ignore = "requires three original service-http packages"]
async fn configured_selection_rejects_stale_or_foreign_wiring_and_returns_original_worker() {
    for package in packages() {
        // Saved policy and broad registry approval never imply endpoint selection.
        let empty_dir = tempfile::tempdir().unwrap();
        let mut empty = Setup::open(
            empty_dir.path(),
            &package,
            "http://127.0.0.1:9",
            RuntimeLimits::default().fuel,
        );
        let calls = Arc::new(AtomicUsize::new(0));
        let (service, instance, binding) = empty.prepare(&[], &calls);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let (manager, worker) = empty.worker(instance, binding);
        let (host, configured) = service.attach(worker, WAIT).unwrap();
        let node = host
            .bind_configured(configured.clone(), None, Limits::default())
            .await
            .unwrap();
        status(
            &call(node.local_addr(), "/forward", "unselected", BODY).await,
            503,
        );
        ConfiguredRun {
            _manager: manager,
            host,
            configured,
            node,
        }
        .close()
        .await;

        let dir = tempfile::tempdir().unwrap();
        let mut setup = Setup::open(
            dir.path(),
            &package,
            "http://127.0.0.1:9",
            RuntimeLimits::default().fuel,
        );
        for choices in [
            vec![choice(1); 9],
            vec![choice(1); 2],
            vec![choice(0)],
            vec![choice(2)],
            vec![ServiceEndpointSelection {
                reference: [0; 32],
                revision: 1,
            }],
            vec![ServiceEndpointSelection {
                reference: [99; 32],
                revision: 1,
            }],
        ] {
            assert!(setup.selected(&choices).is_err());
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let (prepared, instance, binding) = setup.prepare(&[choice(1)], &calls);
        let original = setup.runtime.binding();
        let (manager, worker) = setup.worker(instance, binding);
        let other_dir = tempfile::tempdir().unwrap();
        let mut other = Setup::open(
            other_dir.path(),
            &package,
            "http://127.0.0.1:9",
            RuntimeLimits::default().fuel,
        );
        let (other_service, other_instance, other_binding) = other.prepare(&[choice(1)], &calls);
        let other_original = other.runtime.binding();
        let (other_manager, other_worker) = other.worker(other_instance, other_binding);
        let failure = match prepared.attach(other_worker, WAIT) {
            Err(failure) => failure,
            Ok(_) => panic!("foreign worker accepted"),
        };
        assert_eq!(failure.error, Error::Denied);
        let (host, configured) = other_service.attach(failure.worker, WAIT).unwrap();
        assert!(configured.check().is_ok());
        let exit = host.shutdown_owned().await.unwrap();
        assert_eq!(exit.owner.original, other_original);
        assert_eq!(exit.disconnect, Ok(()));
        let host = ServiceHost::new_owned(worker, WAIT, Arc::new(|| Box::new(Reject))).unwrap();
        let exit = host.shutdown_owned().await.unwrap();
        assert_eq!(exit.owner.original, original);
        assert_eq!(exit.disconnect, Ok(()));
        drop((manager, other_manager));
    }
}
struct Reject;
impl BrokerRouter for Reject {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &Request,
    ) -> Result<Vec<u8>, RouterFault> {
        Err(RouterFault::Denied)
    }
}
