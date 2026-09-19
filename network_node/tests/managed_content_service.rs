//! Real authenticated HTTP -> managed Wasm exchange -> current core content.
//! The guest copies the actual core response into its typed service reply; it
//! contains no expected card body or fabricated successful read response.
#![cfg(feature = "plugin-adapter")]
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    io::{Header, Request as IoRequest},
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        proto::Capability,
        registry::Registry,
    },
    response::{Failure, Outcome, Response as CoreResponse},
    runtime::{Command, ReadContent, RenameRequest},
    service::{self, Invocation, Reply, Request, Response},
    service_record::{self, Policy},
    store::{EventBudget, Store},
};
use morrow_network_node::{
    Limits,
    managed_service::{ManagedNode, RouterFactory, ServiceHost},
    server::Principal,
};
use morrow_plugin_runtime::{
    Limits as RuntimeLimits,
    io_jobs::{BrokerRouter, IoWorker, JobLimits, RouteContext, RouterFault},
    manager::Manager,
    service_content::{ContentScope, ServiceContentPolicy, scope_digest},
    service_history::ServiceJournal,
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
const ID: &str = "org.example.content-service";
const SERVICE: &str = "notes.content";
const HANDLER: &str = "notes.read";
const KEY: &str = "stable-content-request";
const ALICE: &str = "synthetic-content-alice-token-123456789";
const BOB: &str = "synthetic-content-bob-token-1234567890";
fn retention() -> Policy {
    Policy {
        namespace: [14; 32],
        retention_ms: 60_000,
    }
}
fn scope(card: &str) -> ContentScope {
    ContentScope {
        kind: GrantKind::ReadContent,
        card_id: card.into(),
        attachment_id: None,
    }
}
fn scopes() -> Vec<ContentScope> {
    vec![scope("card-a"), scope("card-b")]
}
fn expected_request(principal: &str, allowed: &[ContentScope]) -> Request {
    let digest = scope_digest(allowed).unwrap();
    let invocation = Invocation {
        service: SERVICE.into(),
        handler: HANDLER.into(),
        principal: principal.into(),
        method: "POST".into(),
        target: "/content".into(),
        headers: vec![Header {
            name: "morrow-content-scope".into(),
            value: digest
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into_bytes(),
        }],
        body: vec![],
    };
    let call = service_record::call_id(&retention(), KEY, &invocation).unwrap();
    Request::encode(call, &invocation).unwrap()
}
fn escaped(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
fn read_command() -> Command {
    Command::ReadContent(ReadContent {
        request_id: "read-card-a".into(),
        card_id: "card-a".into(),
        expected_revision: 1,
        offset: 0,
        length: 1024,
    })
}
fn module(principal: &str, allowed: &[ContentScope], command: &Command) -> Vec<u8> {
    let request = expected_request(principal, allowed);
    let command = command.encode().unwrap();
    let placeholder = vec![0xa5; 4096];
    let completion = Response::encode(
        &request,
        &Reply {
            status: 200,
            headers: vec![],
            body: placeholder.clone(),
        },
    )
    .unwrap();
    let body_offset = completion
        .windows(placeholder.len())
        .position(|p| p == placeholder)
        .unwrap();
    // Prefix is the actual encoded core-response length. No parsing shortcut in
    // the guest: copy exactly the host exchange's returned bytes into the body.
    wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
        (import "morrow_v1" "exchange" (func $exchange (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 8)
        (data (i32.const 131072) "{}") (data (i32.const 327680) "{}")
        (func (export "morrow_run") (result i32) (local $n i32)
          i32.const 0 i32.const 131072 call $read drop
          i32.const 131072 i32.const {} i32.const 196608 i32.const 65536 call $exchange local.set $n
          local.get $n i32.const 1 i32.lt_s if unreachable end
          local.get $n i32.const 4092 i32.gt_u if unreachable end
          i32.const {} local.get $n i32.store
          i32.const {} i32.const 196608 local.get $n memory.copy
          i32.const 327680 i32.const {} call $done drop i32.const 0))"#,
        escaped(&command),
        escaped(&completion),
        command.len(),
        327680 + body_offset,
        327684 + body_offset,
        completion.len()
    ))
    .unwrap()
}
struct DenyIo;
impl BrokerRouter for DenyIo {
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
    grant: ServiceGrant,
    listener: ListenerGrant,
    policy: ServiceContentPolicy,
    principal_scopes: BTreeMap<String, Vec<ContentScope>>,
}
impl Running {
    fn open(
        path: &Path,
        guest_principal: &str,
        guest_scopes: &[ContentScope],
        principal_scopes: BTreeMap<String, Vec<ContentScope>>,
        seed: Option<&[u8]>,
        fuel: u64,
    ) -> Self {
        Self::with_command(
            path,
            guest_principal,
            guest_scopes,
            principal_scopes,
            seed,
            fuel,
            read_command(),
        )
    }
    fn with_command(
        path: &Path,
        guest_principal: &str,
        guest_scopes: &[ContentScope],
        principal_scopes: BTreeMap<String, Vec<ContentScope>>,
        seed: Option<&[u8]>,
        fuel: u64,
        command: Command,
    ) -> Self {
        let write = matches!(&command, Command::Rename(_));
        let mut content_kinds = BTreeSet::from([GrantKind::ReadContent]);
        let mut capabilities = vec![Capability::ReadContent];
        let mut maximum_scopes = scopes();
        if write {
            content_kinds.insert(GrantKind::Rename);
            capabilities.push(Capability::RenameCard);
            maximum_scopes.push(rename_scope());
        }
        let wasm = module(guest_principal, guest_scopes, &command);
        let io_caps = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, capabilities);
        let mut declaration =
            io::declaration(io_caps.iter().copied().collect(), vec![HANDLER.into()]);
        declaration.service_schema_sha256 = service::schema_digest().to_vec();
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let digest = package.digest();
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
            .approve(ID, digest, content_kinds, manager.revision())
            .unwrap();
        manager
            .approve_io(ID, digest, io_caps.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, digest, true, manager.revision())
            .unwrap();
        let mut store = Store::open(&path.join("db"), EventBudget::default()).unwrap();
        if let Some(body) = seed {
            store
                .create_local(
                    "seed-a",
                    &CardRecord::new("card-a", "text", 1, "A", body.to_vec()).unwrap(),
                )
                .unwrap();
            store
                .create_local(
                    "seed-b",
                    &CardRecord::new("card-b", "text", 1, "B", b"other-private-body".to_vec())
                        .unwrap(),
                )
                .unwrap();
        }
        let mut host = HostRuntime::new(store).unwrap();
        let mut instance = manager.connect(ID, &mut host).unwrap();
        for card in ["card-a", "card-b"] {
            host.grant(instance.parts_mut().1, GrantKind::ReadContent, card, 100, 1)
                .unwrap();
        }
        if write {
            host.grant(instance.parts_mut().1, GrantKind::Rename, "card-a", 100, 1)
                .unwrap();
        }
        let binding = manager
            .bind_io(
                &host,
                &instance,
                digest,
                manager.revision(),
                &io_caps,
                100,
                1,
            )
            .unwrap();
        let grant =
            ServiceGrant::issue(&manager, &host, &instance, &binding, SERVICE, HANDLER, 1).unwrap();
        let listener = ListenerGrant::issue(&manager, &host, &instance, &binding, 1).unwrap();
        let policy =
            ServiceContentPolicy::issue(&host, &instance, &grant, maximum_scopes, 1).unwrap();
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
        let routers: RouterFactory = Arc::new(|| Box::new(DenyIo));
        let host = ServiceHost::new(worker, Duration::from_secs(5), routers).unwrap();
        Self {
            _manager: manager,
            host,
            grant,
            listener,
            policy,
            principal_scopes,
        }
    }
    async fn bind(&self) -> ManagedNode {
        let route = self
            .host
            .content_route(
                self.grant.clone(),
                "POST",
                "/content",
                ServiceJournal::new(retention(), || 1000).unwrap(),
                self.policy.clone(),
                self.principal_scopes.clone(),
            )
            .unwrap();
        ManagedNode::bind(
            "127.0.0.1:0".parse().unwrap(),
            self.listener.clone(),
            vec![
                Principal::new("alice", ALICE, &[SERVICE], Duration::from_secs(60)).unwrap(),
                Principal::new("bob", BOB, &[SERVICE], Duration::from_secs(60)).unwrap(),
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
async fn post(address: SocketAddr, token: &str) -> Vec<u8> {
    let mut socket = TcpStream::connect(address).await.unwrap();
    // Both forged reserved headers must be removed, including mixed casing and
    // Connection nomination. The host then inserts exactly its actual digest.
    let text = format!(
        "POST /content HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nIdempotency-Key: {KEY}\r\nMorrow-Content-Scope: forged\r\nmorrow-content-scope: other\r\nConnection: close, morrow-content-scope\r\nContent-Length: 0\r\n\r\n"
    );
    socket.write_all(text.as_bytes()).await.unwrap();
    let mut output = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), socket.read_to_end(&mut output))
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
fn core_reply(bytes: &[u8]) -> CoreResponse {
    status(bytes, 200);
    let offset = bytes.windows(4).position(|x| x == b"\r\n\r\n").unwrap() + 4;
    let body = &bytes[offset..];
    assert_eq!(body.len(), 4096);
    let n = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
    CoreResponse::decode(&body[4..4 + n]).unwrap()
}
#[tokio::test]
async fn actual_http_wasm_exchange_reads_live_card_bytes_and_strips_forged_scope_headers() {
    let dir = tempfile::tempdir().unwrap();
    let actual = "live DB正文\0after guest module construction".as_bytes();
    let run = Running::open(
        dir.path(),
        "alice",
        &scopes(),
        BTreeMap::from([("alice".into(), scopes())]),
        Some(actual),
        RuntimeLimits::default().fuel,
    );
    let node = run.bind().await;
    match core_reply(&post(node.local_addr(), ALICE).await).outcome {
        Outcome::ContentChunk(chunk) => {
            assert_eq!(chunk.card_id, "card-a");
            assert_eq!(chunk.revision, 1);
            assert_eq!(chunk.bytes, actual);
        }
        other => panic!("not an actual core content response: {other:?}"),
    }
    // Bob has genuine service authentication but no content-route mapping.
    status(&post(node.local_addr(), BOB).await, 403);
    node.shutdown().await.unwrap();
    run.finish().await;
}
#[tokio::test]
async fn authenticated_other_principal_cannot_read_outside_its_effective_card_scope() {
    let dir = tempfile::tempdir().unwrap();
    let narrow = vec![scope("card-b")];
    let run = Running::open(
        dir.path(),
        "bob",
        &narrow,
        BTreeMap::from([("bob".into(), narrow.clone())]),
        Some(b"alice-sensitive-body"),
        RuntimeLimits::default().fuel,
    );
    let node = run.bind().await;
    let reply = post(node.local_addr(), BOB).await;
    assert_eq!(
        core_reply(&reply).outcome,
        Outcome::Rejected(Failure::Denied)
    );
    assert!(
        !reply
            .windows(b"alice-sensitive-body".len())
            .any(|w| w == b"alice-sensitive-body")
    );
    node.shutdown().await.unwrap();
    run.finish().await;
}
#[tokio::test]
async fn narrowed_scope_after_restart_cannot_replay_old_same_key_result() {
    let dir = tempfile::tempdir().unwrap();
    let run = Running::open(
        dir.path(),
        "alice",
        &scopes(),
        BTreeMap::from([("alice".into(), scopes())]),
        Some(b"old-sensitive-body"),
        RuntimeLimits::default().fuel,
    );
    let node = run.bind().await;
    status(&post(node.local_addr(), ALICE).await, 200);
    node.shutdown().await.unwrap();
    run.finish().await;
    drop(run);
    // Keep the exact package bytes and key; only the effective principal scope
    // narrows. One fuel proves the conflict is found before any guest execution.
    let run = Running::open(
        dir.path(),
        "alice",
        &scopes(),
        BTreeMap::from([("alice".into(), vec![scope("card-b")])]),
        None,
        1,
    );
    let node = run.bind().await;
    let reply = post(node.local_addr(), ALICE).await;
    status(&reply, 409);
    assert!(
        !reply
            .windows(b"old-sensitive-body".len())
            .any(|w| w == b"old-sensitive-body")
    );
    node.shutdown().await.unwrap();
    run.finish().await;
}

fn rename_scope() -> ContentScope {
    ContentScope {
        kind: GrantKind::Rename,
        card_id: "card-a".into(),
        attachment_id: None,
    }
}
fn rename_command() -> Command {
    Command::Rename(RenameRequest {
        operation_id: "rename-from-http".into(),
        card_id: "card-a".into(),
        expected_revision: 1,
        title: "Renamed by actual service".into(),
    })
}
#[tokio::test]
async fn actual_http_write_receipt_persists_and_same_key_replay_never_commits_twice() {
    use morrow_core::transaction::Lookup;
    let dir = tempfile::tempdir().unwrap();
    let allowed = vec![rename_scope()];
    let run = Running::with_command(
        dir.path(),
        "alice",
        &allowed,
        BTreeMap::from([("alice".into(), allowed.clone())]),
        Some(b"unchanged original body"),
        RuntimeLimits::default().fuel,
        rename_command(),
    );
    let node = run.bind().await;
    let first = core_reply(&post(node.local_addr(), ALICE).await);
    let receipt = match &first.outcome {
        Outcome::Renamed(receipt) => receipt.clone(),
        other => panic!("not an actual rename receipt: {other:?}"),
    };
    assert_eq!(receipt.operation_id, "rename-from-http");
    assert_eq!(receipt.card_id, "card-a");
    assert_eq!(receipt.revision, 2);
    assert_eq!(core_reply(&post(node.local_addr(), ALICE).await), first);
    node.shutdown().await.unwrap();
    run.finish().await;
    drop(run);
    let store = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
    let card = store.card("card-a").unwrap().unwrap();
    assert_eq!(card.summary().title, "Renamed by actual service");
    assert_eq!(card.summary().revision, 2);
    assert_eq!(card.body(), b"unchanged original body");
    assert_eq!(
        store.lookup_for_card("card-a", "rename-from-http").unwrap(),
        Lookup::Committed(receipt.clone())
    );
    // Two seed writes, one actual rename, and three service-history events.
    let usage = store.pending_usage().unwrap();
    assert_eq!(usage.0, 6);
    store.integrity_check().unwrap();
    drop(store);
    // The package and key are identical across restart. Insufficient guest fuel
    // proves the successful receipt is recovered before a second Wasm execution.
    let run = Running::with_command(
        dir.path(),
        "alice",
        &allowed,
        BTreeMap::from([("alice".into(), allowed.clone())]),
        None,
        1,
        rename_command(),
    );
    let node = run.bind().await;
    assert_eq!(core_reply(&post(node.local_addr(), ALICE).await), first);
    node.shutdown().await.unwrap();
    run.finish().await;
    drop(run);
    let store = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
    assert_eq!(store.pending_usage().unwrap(), usage);
    assert_eq!(store.card("card-a").unwrap().unwrap().summary().revision, 2);
    assert_eq!(
        store.lookup_for_card("card-a", "rename-from-http").unwrap(),
        Lookup::Committed(receipt)
    );
}
#[tokio::test]
async fn read_only_principal_scope_cannot_submit_the_actual_guest_rename() {
    use morrow_core::transaction::Lookup;
    let dir = tempfile::tempdir().unwrap();
    let allowed = vec![scope("card-a")];
    // The original host/instance possesses Rename, but this principal does not.
    let run = Running::with_command(
        dir.path(),
        "alice",
        &allowed,
        BTreeMap::from([("alice".into(), allowed.clone())]),
        Some(b"must remain original"),
        RuntimeLimits::default().fuel,
        rename_command(),
    );
    let node = run.bind().await;
    assert_eq!(
        core_reply(&post(node.local_addr(), ALICE).await).outcome,
        Outcome::Rejected(Failure::Denied)
    );
    node.shutdown().await.unwrap();
    run.finish().await;
    drop(run);
    let store = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
    let card = store.card("card-a").unwrap().unwrap();
    assert_eq!(card.summary().title, "A");
    assert_eq!(card.summary().revision, 1);
    assert_eq!(card.body(), b"must remain original");
    assert_eq!(
        store.lookup_for_card("card-a", "rename-from-http").unwrap(),
        Lookup::Absent
    );
    assert_eq!(store.pending_usage().unwrap().0, 5);
    store.integrity_check().unwrap();
}
