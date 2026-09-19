//! Policy and original grant probes with real Manager/HostRuntime instances.
//! No network, content dispatch, or historical replay is performed by this suite.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    content_change::ContentChange,
    dispatch::HostRuntime,
    io::Header,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        proto::Capability,
        registry::Registry,
    },
    runtime::{Command, CreateContent, ReadAttachment, ReadContent, RenameRequest},
    service::{self, Invocation},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Limits,
    io_binding::{Error, IoBinding},
    manager::{ManagedInstance, Manager},
    service_content::{
        ContentScope, SCOPE_HEADER, ServiceContentAccess, ServiceContentPolicy, scope_digest,
    },
    service_io::{ListenerGrant, ServiceGrant},
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
const ID: &str = "org.example.service.content";
const SERVICE: &str = "notes";
const HANDLER: &str = "notes.read";
fn scope(kind: GrantKind, card: &str) -> ContentScope {
    ContentScope {
        kind,
        card_id: card.into(),
        attachment_id: None,
    }
}
fn attachment(card: &str, id: &str) -> ContentScope {
    ContentScope {
        kind: GrantKind::ReadAttachment,
        card_id: card.into(),
        attachment_id: Some(id.into()),
    }
}
fn all_scopes() -> Vec<ContentScope> {
    vec![
        scope(GrantKind::Rename, "card-a"),
        scope(GrantKind::ReadSummary, "card-a"),
        scope(GrantKind::QueryOperation, "card-a"),
        attachment("card-a", "attachment-a"),
        scope(GrantKind::CreateContent, "card-a"),
        scope(GrantKind::EditContent, "card-a"),
        scope(GrantKind::ReadContent, "card-a"),
    ]
}
struct Fixture {
    _dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    binding: IoBinding,
    service: ServiceGrant,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
        let mut manifest = Package::manifest_for_task(
            ID,
            "1.0.0",
            &wasm,
            vec![
                Capability::RenameCard,
                Capability::ReadSummary,
                Capability::QueryOperation,
                Capability::ReadAttachment,
                Capability::CreateContent,
                Capability::EditContent,
                Capability::ReadContent,
            ],
        );
        let caps = BTreeSet::from([IoCapability::HttpPublish, IoCapability::HttpListen]);
        let mut declaration = io::declaration(caps.iter().copied().collect(), vec![HANDLER.into()]);
        declaration.service_schema_sha256 = service::schema_digest().to_vec();
        declaration.budget.as_mut().unwrap().max_resources = 8;
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve(
                ID,
                package.digest(),
                package.capabilities().clone(),
                manager.revision(),
            )
            .unwrap();
        manager
            .approve_io(ID, package.digest(), caps.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager
            .bind_io(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                &caps,
                100,
                1,
            )
            .unwrap();
        let service =
            ServiceGrant::issue(&manager, &host, &instance, &binding, SERVICE, HANDLER, 1).unwrap();
        Self {
            _dir: dir,
            manager,
            host,
            instance,
            binding,
            service,
        }
    }
    fn grant(&mut self, scope: &ContentScope, expires: u64, now: u64) {
        let (_, connection) = self.instance.parts_mut();
        if let Some(attachment) = &scope.attachment_id {
            self.host
                .grant_attachment(connection, &scope.card_id, attachment, expires, now)
                .unwrap();
        } else {
            self.host
                .grant(connection, scope.kind, &scope.card_id, expires, now)
                .unwrap();
        }
    }
    fn policy(&mut self, scopes: Vec<ContentScope>, expires: u64) -> ServiceContentPolicy {
        for scope in &scopes {
            self.grant(scope, expires, 1);
        }
        ServiceContentPolicy::issue(&self.host, &self.instance, &self.service, scopes, 1).unwrap()
    }
}
fn access(policy: &ServiceContentPolicy, scopes: Vec<ContentScope>) -> ServiceContentAccess {
    policy.authorize("alice", scopes, || true).unwrap()
}
fn hex(digest: [u8; 32]) -> Vec<u8> {
    digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .into_bytes()
}
fn request(access: &ServiceContentAccess) -> Invocation {
    Invocation {
        service: SERVICE.into(),
        handler: HANDLER.into(),
        principal: "alice".into(),
        method: "POST".into(),
        target: "/content".into(),
        headers: vec![Header {
            name: SCOPE_HEADER.into(),
            value: hex(access.scope_digest()),
        }],
        body: b"user text is not permission".to_vec(),
    }
}
fn summary(card: &str) -> Command {
    Command::ReadSummary {
        request_id: "read-1".into(),
        card_id: card.into(),
    }
}
#[test]
fn scope_digest_is_canonical_and_independent_of_runtime_identity() {
    let mut scopes = all_scopes();
    let expected = scope_digest(&scopes).unwrap();
    scopes.reverse();
    assert_eq!(scope_digest(&scopes).unwrap(), expected);
    for _ in 0..2 {
        let mut f = Fixture::new();
        let policy = f.policy(scopes.clone(), 50);
        assert_eq!(access(&policy, scopes.clone()).scope_digest(), expected);
    }
    assert_ne!(scope_digest(&[]).unwrap(), expected);
    assert_ne!(
        scope_digest(&[attachment("ab", "c")]).unwrap(),
        scope_digest(&[attachment("a", "bc")]).unwrap()
    );
    assert_ne!(
        scope_digest(&[scope(GrantKind::ReadContent, "card-a")]).unwrap(),
        scope_digest(&[scope(GrantKind::ReadSummary, "card-a")]).unwrap()
    );
}
#[test]
fn issue_rejects_missing_grants_foreign_instance_and_foreign_host() {
    let mut f = Fixture::new();
    let desired = scope(GrantKind::ReadContent, "card-a");
    assert!(
        ServiceContentPolicy::issue(&f.host, &f.instance, &f.service, vec![desired.clone()], 1)
            .is_err()
    );
    f.grant(&desired, 50, 1);
    let other = f.manager.connect(ID, &mut f.host).unwrap();
    assert!(
        ServiceContentPolicy::issue(&f.host, &other, &f.service, vec![desired.clone()], 99)
            .is_err()
    );
    let foreign = Fixture::new();
    assert!(
        ServiceContentPolicy::issue(&foreign.host, &f.instance, &f.service, vec![], 99).is_err()
    );
    // Rejected foreign ownership cannot advance the original binding's clock.
    assert!(
        ServiceContentPolicy::issue(&f.host, &f.instance, &f.service, vec![desired], 1).is_ok()
    );
}
#[test]
fn invalid_scopes_fail_before_sampling_authority_and_limits_are_exact() {
    let f = Fixture::new();
    let invalid = [
        vec![scope(GrantKind::ReadContent, "")],
        vec![scope(GrantKind::ReadContent, "a/b")],
        vec![scope(GrantKind::ReadContent, &"a".repeat(257))],
        vec![scope(GrantKind::ReadAttachment, "card-a")],
        vec![attachment("card-a", "")],
        vec![ContentScope {
            kind: GrantKind::ReadContent,
            card_id: "card-a".into(),
            attachment_id: Some("file".into()),
        }],
        vec![scope(GrantKind::ReadContent, "card-a"); 2],
    ];
    for scopes in invalid {
        assert!(scope_digest(&scopes).is_err());
        assert!(
            ServiceContentPolicy::issue(&f.host, &f.instance, &f.service, scopes, 999).is_err()
        );
    }
    f.service.check(1).unwrap();
    let scopes = (0..128)
        .map(|i| scope(GrantKind::ReadSummary, &format!("card-{i}")))
        .collect::<Vec<_>>();
    assert!(scope_digest(&scopes).is_ok());
    let mut extra = scopes;
    extra.push(scope(GrantKind::ReadSummary, "over-limit"));
    assert_eq!(scope_digest(&extra), Err(Error::Limit));
}
#[test]
fn effective_subset_is_exact_and_empty_scope_never_authorizes_content() {
    let mut f = Fixture::new();
    let scopes = vec![
        scope(GrantKind::ReadSummary, "card-a"),
        scope(GrantKind::ReadContent, "card-b"),
    ];
    let policy = f.policy(scopes.clone(), 50);
    let limited = access(&policy, vec![scopes[0].clone()]);
    limited.check_command(&summary("card-a"), 2).unwrap();
    assert!(limited.check_command(&summary("card-b"), 99).is_err());
    limited.check_at(2).unwrap(); // rejected command did not poison clock
    assert!(
        policy
            .authorize("alice", vec![scope(GrantKind::Rename, "card-a")], || true)
            .is_err()
    );
    assert!(
        policy
            .authorize("alice", vec![scopes[0].clone(); 2], || true)
            .is_err()
    );
    let empty = access(&policy, vec![]);
    empty.check_at(2).unwrap();
    assert!(empty.check_command(&summary("card-a"), 2).is_err());
    assert_ne!(empty.scope_digest(), limited.scope_digest());
}
#[test]
fn every_command_kind_and_attachment_selects_only_its_exact_scope() {
    let mut f = Fixture::new();
    let scopes = all_scopes();
    let policy = f.policy(scopes.clone(), 50);
    let commands = [
        Command::Rename(RenameRequest {
            operation_id: "rename".into(),
            card_id: "card-a".into(),
            expected_revision: 1,
            title: "name".into(),
        }),
        summary("card-a"),
        Command::QueryOperation {
            request_id: "query".into(),
            card_id: "card-a".into(),
            operation_id: "history".into(),
        },
        Command::ReadAttachment(ReadAttachment {
            request_id: "attachment".into(),
            card_id: "card-a".into(),
            attachment_id: "attachment-a".into(),
            expected_revision: 1,
            offset: 0,
            length: 1,
        }),
        Command::CreateContent(CreateContent {
            operation_id: "create".into(),
            card_id: "card-a".into(),
            type_id: "note".into(),
            format_version: 1,
            title: "name".into(),
            body: vec![],
        }),
        Command::EditContent(ContentChange {
            operation_id: "edit".into(),
            card_id: "card-a".into(),
            expected_revision: 1,
            title: "name".into(),
            body: vec![],
            preview_text: String::new(),
            attachments: None,
        }),
        Command::ReadContent(ReadContent {
            request_id: "body".into(),
            card_id: "card-a".into(),
            expected_revision: 1,
            offset: 0,
            length: 1,
        }),
    ];
    for (i, scope) in scopes.iter().enumerate() {
        let access = access(&policy, vec![scope.clone()]);
        for (j, command) in commands.iter().enumerate() {
            assert_eq!(access.check_command(command, 2).is_ok(), i == j);
        }
    }
    let access = access(&policy, vec![attachment("card-a", "attachment-a")]);
    let mut other = match &commands[3] {
        Command::ReadAttachment(v) => v.clone(),
        _ => unreachable!(),
    };
    other.attachment_id = "attachment-b".into();
    assert!(
        access
            .check_command(&Command::ReadAttachment(other), 2)
            .is_err()
    );
}
#[test]
fn original_probe_rejects_revoke_replacement_and_expiry_without_renewal() {
    for cause in 0..3 {
        let mut f = Fixture::new();
        let desired = scope(GrantKind::ReadContent, "card-a");
        let policy = f.policy(vec![desired.clone()], 10);
        let original = access(&policy, vec![desired.clone()]);
        let clone = original.clone();
        original.check_at(2).unwrap();
        match cause {
            0 => {
                let (_, conn) = f.instance.parts_mut();
                f.host.revoke(conn, desired.kind, "card-a").unwrap();
            }
            1 => f.grant(&desired, 50, 3),
            _ => {}
        }
        assert!(clone.check_at(if cause == 2 { 10 } else { 3 }).is_err());
        if cause == 1 {
            let fresh = ServiceContentPolicy::issue(
                &f.host,
                &f.instance,
                &f.service,
                vec![desired.clone()],
                3,
            )
            .unwrap();
            access(&fresh, vec![desired]).check_at(3).unwrap();
            assert!(original.check_at(3).is_err());
        }
    }
}
#[test]
fn removing_unused_policy_scope_does_not_revoke_the_effective_subset() {
    let mut f = Fixture::new();
    let read = scope(GrantKind::ReadContent, "card-a");
    let summary_scope = scope(GrantKind::ReadSummary, "card-a");
    let policy = f.policy(vec![read.clone(), summary_scope.clone()], 50);
    let narrowed = access(&policy, vec![summary_scope]);
    let full = access(
        &policy,
        vec![read.clone(), scope(GrantKind::ReadSummary, "card-a")],
    );
    let (_, conn) = f.instance.parts_mut();
    f.host.revoke(conn, read.kind, &read.card_id).unwrap();
    narrowed.check_at(2).unwrap();
    assert!(full.check_at(2).is_err());
}
#[test]
fn policy_and_current_principal_revocation_reach_existing_clones() {
    let mut f = Fixture::new();
    let scopes = vec![scope(GrantKind::ReadSummary, "card-a")];
    let policy = f.policy(scopes.clone(), 50);
    let live = Arc::new(AtomicBool::new(true));
    let observed = live.clone();
    let allowed = policy
        .authorize("alice", scopes.clone(), move || {
            observed.load(Ordering::Acquire)
        })
        .unwrap();
    let clone = allowed.clone();
    live.store(false, Ordering::Release);
    assert!(clone.check().is_err());
    assert!(clone.check_at(2).is_err());
    live.store(true, Ordering::Release);
    clone.check_at(2).unwrap();
    let independent = access(&policy, scopes.clone());
    policy.clone().revoke();
    assert!(allowed.check().is_err());
    assert!(independent.check_at(2).is_err());
    assert!(policy.authorize("alice", scopes, || true).is_err());
}
#[test]
fn service_revocation_and_disconnection_invalidate_effective_access() {
    for cause in 0..3 {
        let mut f = Fixture::new();
        let scopes = vec![scope(GrantKind::ReadContent, "card-a")];
        let policy = f.policy(scopes.clone(), 50);
        let access = access(&policy, scopes);
        match cause {
            0 => f.service.revoke(),
            1 => f.instance.close(&mut f.host).unwrap(),
            _ => f.instance.stop(),
        }
        assert!(access.check_at(2).is_err());
    }
}
#[test]
fn same_service_resource_accepts_listener_binding_but_rejects_fresh_grant() {
    let mut f = Fixture::new();
    let scopes = vec![scope(GrantKind::ReadSummary, "card-a")];
    let policy = f.policy(scopes.clone(), 50);
    let access = access(&policy, scopes);
    let listener = ListenerGrant::issue(&f.manager, &f.host, &f.instance, &f.binding, 1).unwrap();
    let bound = f.service.bound_to_listener(&listener).unwrap();
    policy.validate_grant(&bound).unwrap();
    access.validate_grant(&bound).unwrap();
    let replacement = ServiceGrant::issue(
        &f.manager,
        &f.host,
        &f.instance,
        &f.binding,
        SERVICE,
        HANDLER,
        1,
    )
    .unwrap();
    assert!(policy.validate_grant(&replacement).is_err());
    assert!(access.validate_grant(&replacement).is_err());
}
#[test]
fn reserved_metadata_and_authenticated_route_must_match_exactly() {
    let mut f = Fixture::new();
    let scopes = vec![scope(GrantKind::ReadContent, "card-a")];
    let policy = f.policy(scopes.clone(), 50);
    let access = access(&policy, scopes);
    let original = request(&access);
    access
        .validate_request(&service::Request::encode(1, &original).unwrap())
        .unwrap();
    for case in 0..8 {
        let mut altered = original.clone();
        match case {
            0 => altered.principal = "bob".into(),
            1 => altered.service = "other".into(),
            2 => altered.handler = "notes.other".into(),
            3 => altered.headers.clear(),
            4 => altered.headers.push(altered.headers[0].clone()),
            5 => altered.headers.push(Header {
                name: "MORROW-CONTENT-SCOPE".into(),
                value: hex(access.scope_digest()),
            }),
            6 => altered.headers[0].value = b"wrong digest".to_vec(),
            _ => altered.headers[0].value = vec![b'0'; 64],
        }
        assert!(
            access
                .validate_request(&service::Request::encode(1, &altered).unwrap())
                .is_err()
        );
    }
    let mut altered = original;
    altered.headers[0].value.make_ascii_uppercase();
    assert_ne!(altered.headers[0].value, hex(access.scope_digest()));
    assert!(
        access
            .validate_request(&service::Request::encode(1, &altered).unwrap())
            .is_err()
    );
}
