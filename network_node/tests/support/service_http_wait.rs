//! Authenticated inbound -> content import -> real outbound HTTP -> durable reply.
use super::*;
use morrow_core::{
    io::{HttpOutcome, HttpSubmission},
    io_intent::Phase as IntentPhase,
};
use morrow_network_node::managed_http::{EndpointApproval, HttpEndpoint, NetworkProfile};
use morrow_plugin_runtime::io_jobs::{CommandOwner, HostOwner, JobError};
use std::sync::atomic::{AtomicU64, Ordering};
#[path = "gated_http.rs"]
mod gated;
use gated::GatedServer;
const WAIT: Duration = Duration::from_secs(10);
const OUTBOUND: &str = "service-outbound-once";

fn http_module() -> Vec<u8> {
    // Only layouts are known before approval. Actual outbound request bytes are
    // stored in an authorized card after this package has been built/connected.
    let command = read_command().encode().unwrap();
    let encoded = CoreResponse {
        request_id: "read-card-a".into(),
        outcome: Outcome::ContentChunk(morrow_core::runtime::ContentChunk {
            card_id: "card-a".into(),
            revision: 1,
            offset: 0,
            total_length: 1024,
            body_sha256: [7; 32],
            bytes: vec![0xa5; 1024],
        }),
    }
    .encode()
    .unwrap();
    let offset = encoded
        .windows(1024)
        .position(|w| w == [0xa5; 1024])
        .unwrap();
    let request = expected_request("alice", &[scope("card-a")]);
    let completion = Response::encode(
        &request,
        &Reply {
            status: 200,
            headers: vec![],
            body: vec![0xa5; 4096],
        },
    )
    .unwrap();
    let body = completion
        .windows(4096)
        .position(|w| w == [0xa5; 4096])
        .unwrap();
    wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (import "morrow_v1" "exchange" (func $core (param i32 i32 i32 i32) (result i32)))
      (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 8)
      (data (i32.const 131072) "{}") (data (i32.const 393216) "{}")
      (func (export "morrow_run") (result i32) (local $n i32)
        i32.const 0 i32.const 131072 call $read drop
        i32.const 131072 i32.const {} i32.const 196608 i32.const 65536 call $core local.set $n
        local.get $n i32.const {} i32.lt_s if unreachable end
        i32.const {} i32.load local.set $n
        local.get $n i32.const 1 i32.lt_s if unreachable end
        local.get $n i32.const 1020 i32.gt_u if unreachable end
        i32.const {} local.get $n i32.const 262144 i32.const 131072 call $io local.set $n
        local.get $n i32.const 1 i32.lt_s if unreachable end
        local.get $n i32.const 4092 i32.gt_u if unreachable end
        i32.const {} local.get $n i32.store
        i32.const {} i32.const 262144 local.get $n memory.copy
        i32.const 393216 i32.const {} call $done drop i32.const 0))"#,
        escaped(&command),
        escaped(&completion),
        command.len(),
        offset + 1024,
        196608 + offset,
        196612 + offset,
        393216 + body,
        393220 + body,
        completion.len()
    ))
    .unwrap()
}

struct Owner {
    entered: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
    host: HostRuntime,
    original: morrow_core::dispatch::HostBinding,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
}
impl CommandOwner for Owner {
    fn command(&mut self, input: Vec<u8>) -> Result<Vec<u8>, JobError> {
        assert_eq!(self.host.binding(), self.original);
        let card =
            CardRecord::new("local-during-service", "note", 1, "local", input.clone()).unwrap();
        self.host
            .store_local_mut()
            .create_local("local-service-write", &card)
            .map_err(|_| JobError::Unavailable)?;
        if input == b"block" {
            self.entered.send(()).map_err(|_| JobError::Unavailable)?;
            self.release
                .recv_timeout(WAIT)
                .map_err(|_| JobError::Unavailable)?;
        }
        Ok(input)
    }
}
struct Combined {
    blocked: std::sync::mpsc::Receiver<()>,
    release: std::sync::mpsc::SyncSender<()>,
    _manager: Manager,
    host: ServiceHost<Owner>,
    grant: ServiceGrant,
    listener: ListenerGrant,
    policy: ServiceContentPolicy,
    endpoint: HttpEndpoint,
    outbound: IoRequest,
    digest: [u8; 32],
    wall: Arc<AtomicU64>,
}
impl Combined {
    fn open(path: &Path, origin: &str) -> Self {
        let wasm = http_module();
        let caps = BTreeSet::from([
            IoCapability::HttpListen,
            IoCapability::HttpPublish,
            IoCapability::HttpRequest,
        ]);
        let mut manifest =
            Package::manifest_for_task(ID, "1.1.0", &wasm, vec![Capability::ReadContent]);
        let mut declaration = io::declaration(caps.iter().copied().collect(), vec![HANDLER.into()]);
        declaration.service_schema_sha256 = service::schema_digest().to_vec();
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
        let catalog = Catalog::open(&path.join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&path.join("registry"), catalog).unwrap(),
            RuntimeLimits::default(),
        );
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve(
                ID,
                digest,
                BTreeSet::from([GrantKind::ReadContent]),
                manager.revision(),
            )
            .unwrap();
        manager
            .approve_io(ID, digest, caps.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, digest, true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&path.join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let mut instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager
            .bind_io(&host, &instance, digest, manager.revision(), &caps, 100, 1)
            .unwrap();
        let endpoint = HttpEndpoint::approve(
            &manager,
            &host,
            &instance,
            &binding,
            EndpointApproval {
                origin: origin.into(),
                methods: vec!["POST".into()],
                profile: NetworkProfile::LoopbackHttp,
                limits: Limits {
                    max_request_bytes: 65536,
                    max_response_bytes: 65536,
                    max_header_bytes: 16384,
                    max_concurrent: 2,
                    timeout: WAIT,
                },
                response_frame_limit: 65536,
                credential: None,
                root_certificate: None,
            },
            [23; 32],
            1,
        )
        .unwrap();
        let outbound = IoRequest::encode_http_submit(
            1,
            &HttpSubmission {
                operation_id: OUTBOUND.as_bytes().to_vec(),
                deadline_ms: 0,
                endpoint: endpoint.endpoint_reference().into_bytes(),
                method: "POST".into(),
                relative_target: "/upstream".into(),
                headers: vec![],
                body: b"real\0\xffservice".to_vec(),
                credential: vec![],
            },
        )
        .unwrap();
        assert!(outbound.bytes().len() <= 1020);
        let mut body = vec![0; 1024];
        body[..4].copy_from_slice(&(outbound.bytes().len() as u32).to_le_bytes());
        body[4..4 + outbound.bytes().len()].copy_from_slice(outbound.bytes());
        host.store_local_mut()
            .create_local(
                "seed-outbound",
                &CardRecord::new("card-a", "note", 1, "approved request", body).unwrap(),
            )
            .unwrap();
        host.grant(
            instance.parts_mut().1,
            GrantKind::ReadContent,
            "card-a",
            100,
            1,
        )
        .unwrap();
        let grant =
            ServiceGrant::issue(&manager, &host, &instance, &binding, SERVICE, HANDLER, 1).unwrap();
        let listener = ListenerGrant::issue(&manager, &host, &instance, &binding, 1).unwrap();
        let policy =
            ServiceContentPolicy::issue(&host, &instance, &grant, vec![scope("card-a")], 1)
                .unwrap();
        let (entered, blocked) = std::sync::mpsc::sync_channel(1);
        let (release, released) = std::sync::mpsc::sync_channel(1);
        let owner = Owner {
            entered,
            release: released,
            original: host.binding(),
            host,
        };
        let worker = IoWorker::spawn_managed_owned(
            &manager,
            owner,
            instance,
            binding,
            || 1,
            2,
            JobLimits::default(),
        )
        .unwrap();
        let approved = endpoint.clone();
        let runtime = tokio::runtime::Handle::current();
        let routers: RouterFactory = Arc::new(move || Box::new(approved.router(runtime.clone())));
        let host = ServiceHost::new_owned(worker, WAIT, routers).unwrap();
        Self {
            blocked,
            release,
            _manager: manager,
            host,
            grant,
            listener,
            policy,
            endpoint,
            outbound,
            digest,
            wall: Arc::new(AtomicU64::new(1000)),
        }
    }
    async fn bind(&self) -> ManagedNode {
        let wall = self.wall.clone();
        let route = self
            .host
            .content_route(
                self.grant.clone(),
                "POST",
                "/content",
                ServiceJournal::new(retention(), move || wall.load(Ordering::SeqCst)).unwrap(),
                self.policy.clone(),
                BTreeMap::from([("alice".into(), vec![scope("card-a")])]),
            )
            .unwrap();
        let query = self.host.query_route(&route, "/content-history").unwrap();
        ManagedNode::bind_owned(
            "127.0.0.1:0".parse().unwrap(),
            self.listener.clone(),
            vec![Principal::new("alice", ALICE, &[SERVICE], Duration::from_secs(60)).unwrap()],
            vec![route, query],
            Limits {
                timeout: WAIT,
                ..Limits::default()
            },
        )
        .await
        .unwrap()
    }
    fn service_command(&self) -> morrow_core::io_intent::Command {
        service_record::RequestRecord::encode(
            &retention(),
            KEY,
            &expected_request("alice", &[scope("card-a")]),
            1000,
        )
        .unwrap()
        .command(self.digest)
        .unwrap()
    }
    async fn local_write(&self) {
        let mut handle = self
            .host
            .submit_owner_command(b"written while outbound waits".to_vec(), 64)
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Some(reply) = handle.read().unwrap() {
                    assert_eq!(reply, b"written while outbound waits");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("owner available before upstream release");
    }
}
fn actual_http_reply(bytes: &[u8], request: &IoRequest) -> HttpOutcome {
    status(bytes, 200);
    let offset = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let body = &bytes[offset..];
    assert_eq!(body.len(), 4096);
    let len = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
    assert!((1..=4092).contains(&len));
    morrow_core::io::Response::decode_http(request, &body[4..4 + len]).unwrap()
}
fn phase(store: &Store, subject: &str, operation: &str) -> IntentPhase {
    store
        .lookup_io_intent(subject, operation)
        .unwrap()
        .unwrap()
        .phase()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inbound_outbound_wait_keeps_owner_available_and_replay_never_resends() {
    let dir = tempfile::tempdir().unwrap();
    let mut upstream = GatedServer::spawn().await;
    let run = Combined::open(dir.path(), &upstream.origin);
    let node = run.bind().await;
    let address = node.local_addr();
    let mut incoming = tokio::spawn(async move { post(address, ALICE).await });
    tokio::select! { _ = upstream.entered() => {}, reply = &mut incoming => { panic!("service ended before outbound: {}", String::from_utf8_lossy(&reply.unwrap())); } }
    run.local_write().await;
    assert!(!incoming.is_finished());
    upstream.finish().await;
    let first = tokio::time::timeout(WAIT, incoming).await.unwrap().unwrap();
    let outcome = actual_http_reply(&first, &run.outbound);
    assert_eq!(outcome.http_status, 200);
    assert_eq!(outcome.body, b"ok");
    let replay = post(address, ALICE).await;
    assert_eq!(actual_http_reply(&replay, &run.outbound), outcome);
    let queried = post_target(address, ALICE, "/content-history").await;
    assert_eq!(actual_http_reply(&queried, &run.outbound), outcome);
    assert_eq!(upstream.calls.load(Ordering::SeqCst), 1);
    node.shutdown().await.unwrap();
    let exit = run.host.shutdown_owned().await.unwrap();
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
    assert_eq!(exit.owner.host.binding(), exit.owner.original);
    let service = run.service_command();
    assert_eq!(
        phase(exit.owner.host.store_local(), ID, OUTBOUND),
        IntentPhase::Observed
    );
    assert_eq!(
        phase(
            exit.owner.host.store_local(),
            &service.subject,
            &service.operation_id
        ),
        IntentPhase::Observed
    );
    drop(exit);
    let store = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
    assert_eq!(
        store.card("local-during-service").unwrap().unwrap().body(),
        b"written while outbound waits"
    );
    assert_eq!(phase(&store, ID, OUTBOUND), IntentPhase::Observed);
    assert_eq!(
        phase(&store, &service.subject, &service.operation_id),
        IntentPhase::Observed
    );
    store.integrity_check().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn endpoint_revoke_or_retention_expiry_during_outbound_keeps_both_intents_unknown() {
    for expire in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut upstream = GatedServer::spawn().await;
        let run = Combined::open(dir.path(), &upstream.origin);
        let node = run.bind().await;
        let address = node.local_addr();
        let mut incoming = tokio::spawn(async move { post(address, ALICE).await });
        tokio::select! { _ = upstream.entered() => {}, reply = &mut incoming => { panic!("service ended before outbound: {}", String::from_utf8_lossy(&reply.unwrap())); } }
        run.local_write().await;
        if expire {
            run.wall.store(61_000, Ordering::SeqCst);
        } else {
            run.endpoint.revoke();
        }
        let reply = tokio::time::timeout(WAIT, incoming).await.unwrap().unwrap();
        assert!(!reply.starts_with(b"HTTP/1.1 200 "));
        let replay = post(address, ALICE).await;
        status(&replay, if expire { 410 } else { 409 });
        assert_eq!(upstream.calls.load(Ordering::SeqCst), 1);
        upstream.finish().await;
        node.shutdown().await.unwrap();
        let exit = run.host.shutdown_owned().await.unwrap();
        assert_eq!(exit.result, Ok(()));
        let service = run.service_command();
        assert_eq!(
            phase(exit.owner.host.store_local(), ID, OUTBOUND),
            IntentPhase::OutcomeUnknown
        );
        assert_eq!(
            phase(
                exit.owner.host.store_local(),
                &service.subject,
                &service.operation_id
            ),
            IntentPhase::OutcomeUnknown
        );
        assert!(
            exit.owner
                .host
                .store_local()
                .card("local-during-service")
                .unwrap()
                .is_some()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_waits_for_held_owner_callback_during_service_http() {
    let dir = tempfile::tempdir().unwrap();
    let mut upstream = GatedServer::spawn().await;
    let run = Combined::open(dir.path(), &upstream.origin);
    let node = run.bind().await;
    let addr = node.local_addr();

    let incoming = tokio::spawn(async move { post(addr, ALICE).await });
    upstream.entered().await;

    let command = run
        .host
        .submit_owner_command(b"block".to_vec(), 64)
        .unwrap();

    tokio::time::timeout(WAIT, async {
        loop {
            match run.blocked.try_recv() {
                Ok(()) => break,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                }
                Err(e) => panic!("{e}"),
            }
        }
    })
    .await
    .expect("owner reached committed callback gate");

    run.host.request_stop().unwrap();
    node.request_stop();
    assert!(run.host.try_reclaim().unwrap().is_none());

    run.release.send(()).unwrap();

    let exit = tokio::time::timeout(WAIT, run.host.shutdown_owned())
        .await
        .unwrap()
        .unwrap();
    assert!(exit.result.is_ok());
    assert!(exit.maintenance.is_ok());
    assert!(exit.disconnect.is_ok());
    assert_eq!(exit.owner.host.binding(), exit.owner.original);

    let service = run.service_command();
    let store = exit.owner.host.store_local();
    assert_eq!(phase(store, ID, OUTBOUND), IntentPhase::OutcomeUnknown);
    assert_eq!(
        phase(store, &service.subject, &service.operation_id),
        IntentPhase::OutcomeUnknown
    );
    assert_eq!(
        store.card("local-during-service").unwrap().unwrap().body(),
        b"block"
    );

    assert!(command.is_started());
    if let Ok(reply) = tokio::time::timeout(WAIT, incoming).await.unwrap() {
        assert!(!reply.starts_with(b"HTTP/1.1 200 "));
    }
    // normal bytes reply cannot start HTTP/1.1 200; disconnected post helper may panic (JoinError ok)

    node.shutdown().await.unwrap();
    upstream.finish().await;
}
