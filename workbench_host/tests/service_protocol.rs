#![cfg(target_os = "windows")]

use capnp::{message::Builder, serialize};
use morrow_core::plugin_package::{
    Package,
    io::{self, IoCapability},
};
use morrow_workbench_host::{Workbench, host_capnp as wire, protocol};
use sha2::{Digest, Sha256};
use std::path::Path;
use zeroize::Zeroizing;

fn frame(action: wire::Action, fill: impl FnOnce(wire::request::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(action);
    fill(request);
    serialize::write_message_to_words(&message)
}
fn read<T>(bytes: &[u8], inspect: impl FnOnce(wire::response::Reader<'_>) -> T) -> T {
    assert!(bytes.len() <= 128 * 1024);
    let message = serialize::read_message(&mut &bytes[..], Default::default()).unwrap();
    let response = message.get_root::<wire::response::Reader>().unwrap();
    assert_eq!(response.get_version(), 1);
    assert_eq!(response.get_digest().unwrap(), protocol::digest());
    inspect(response)
}
fn success(response: wire::response::Reader<'_>) {
    assert!(response.get_error().unwrap().is_empty());
}
fn rejected(bytes: &[u8]) {
    read(bytes, |response| {
        assert!(!response.get_error().unwrap().is_empty());
        assert!(response.get_payload().unwrap().is_empty());
        assert!(response.get_issued_token().unwrap().is_empty());
        assert!(response.get_service_configs().unwrap().is_empty());
        assert!(response.get_service_authorities().unwrap().is_empty());
    });
}
fn install(w: &mut Workbench, root: &Path) -> Package {
    let wasm = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "morrow_run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    let mut manifest = Package::manifest_for_task("test.service-wire", "1.0.0", &wasm, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    let mut declaration = io::declaration(
        vec![IoCapability::HttpListen, IoCapability::HttpPublish],
        vec!["test.service.invoke".into()],
    );
    declaration.service_schema_sha256 = morrow_core::service::schema_digest().to_vec();
    manifest.io_declaration = Some(declaration);
    let package = Package::build(manifest, &wasm).unwrap();
    let path = root.join("incoming.mplugin");
    std::fs::write(&path, package.archive()).unwrap();
    w.import_plugin(&path, &package.digest(), revision(w))
        .unwrap();
    w.configure_external_io(
        &package.manifest().package_id,
        &package.digest(),
        revision(w),
        &["http-listen".into(), "http-publish".into()],
    )
    .unwrap();
    package
}
fn revision(w: &Workbench) -> u64 {
    w.catalog_page("", None).unwrap().revision
}
fn fill_config(
    mut value: wire::service_config_update::Builder<'_>,
    package: &Package,
    registry: u64,
    auth: &[u8],
) {
    value.set_package_id(&package.manifest().package_id);
    value.set_package_digest(&package.digest());
    value.set_registry_revision(registry);
    value.set_service("test.service");
    value.set_handler("test.service.invoke");
    value.set_retention_ms(86_400_000);
    let mut principal = value.init_principals(1).get(0);
    principal.set_id("alice");
    principal.set_authentication_reference(auth);
    let mut scope = principal.init_scopes(1).get(0);
    scope.set_kind(2);
    scope.set_card_id("card.wire");
}

#[test]
fn wire_issue_config_publish_page_disable_and_revision_rejection_are_typed_and_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let package = install(&mut w, dir.path());
    let issue = frame(wire::Action::ServiceAuthenticationIssue, |mut r| {
        r.set_principal_id("alice");
        r.set_service_days(3);
    });
    let issued = Zeroizing::new(protocol::respond(&mut w, &issue).unwrap());
    let (auth, token) = read(&issued, |response| {
        success(response);
        let token = response.get_issued_token().unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.iter().all(u8::is_ascii_hexdigit));
        let infos = response.get_service_authorities().unwrap();
        assert_eq!(infos.len(), 1);
        let info = infos.get(0);
        assert_eq!(info.get_kind(), 1);
        assert_eq!(info.get_principal_id().unwrap().to_str().unwrap(), "alice");
        assert!(!info.has_publication());
        assert_eq!(info.get_revision(), 1);
        (
            info.get_reference().unwrap().to_vec(),
            Zeroizing::new(token.to_vec()),
        )
    });
    let hash = Sha256::digest(&*token);
    let registry = revision(&w);
    let create = frame(wire::Action::ServiceConfigSave, |r| {
        fill_config(r.init_service_config(), &package, registry, &auth)
    });
    let created = protocol::respond(&mut w, &create).unwrap();
    let (id, config_digest, publication) = read(&created, |response| {
        success(response);
        assert!(response.get_issued_token().unwrap().is_empty());
        let configs = response.get_service_configs().unwrap();
        assert_eq!(configs.len(), 1);
        let config = configs.get(0);
        assert_eq!(config.get_revision(), 1);
        assert_eq!(config.get_namespace().unwrap().len(), 32);
        assert_eq!(config.get_package_digest().unwrap(), package.digest());
        assert_eq!(
            config.get_service().unwrap().to_str().unwrap(),
            "test.service"
        );
        assert_eq!(
            config.get_handler().unwrap().to_str().unwrap(),
            "test.service.invoke"
        );
        assert_eq!(config.get_retention_ms(), 86_400_000);
        let principals = config.get_principals().unwrap();
        assert_eq!(principals.len(), 1);
        assert_eq!(
            principals.get(0).get_authentication_reference().unwrap(),
            auth
        );
        assert_eq!(principals.get(0).get_scopes().unwrap().get(0).get_kind(), 2);
        assert_eq!(config.get_approval_references().unwrap().len(), 1);
        (
            config.get_id().unwrap().to_str().unwrap().to_owned(),
            config.get_digest().unwrap().to_vec(),
            config
                .get_approval_references()
                .unwrap()
                .get(0)
                .unwrap()
                .to_vec(),
        )
    });
    assert_eq!(
        w.service_config_page("", &[]).unwrap().configs[0]
            .digest()
            .as_slice(),
        config_digest
    );
    let stale = frame(wire::Action::ServiceConfigSave, |r| {
        let mut c = r.init_service_config();
        fill_config(c.reborrow(), &package, registry, &auth);
        c.set_id(&id);
    });
    rejected(&protocol::respond(&mut w, &stale).unwrap());
    let publish = frame(wire::Action::ServicePublicationSave, |r| {
        let mut value = r.init_service_publication();
        value.set_reference(&publication);
        value.set_config_revision(1);
        value.set_registry_revision(registry);
        value.set_package_id(&package.manifest().package_id);
        value.set_lifetime_days(1);
        let mut policy = value.init_policy();
        policy.set_config_id(&id);
        policy.set_config_digest(&config_digest);
        policy.set_listen_address("127.0.0.1:34567");
        policy.set_method("POST");
        policy.set_path("/invoke");
        policy.set_query_path("/result");
    });
    let published = protocol::respond(&mut w, &publish).unwrap();
    read(&published, |response| {
        success(response);
        let info = response.get_service_authorities().unwrap().get(0);
        assert_eq!(info.get_kind(), 2);
        assert!(info.get_principal_id().unwrap().is_empty());
        let policy = info.get_publication().unwrap();
        assert_eq!(policy.get_config_digest().unwrap(), config_digest);
        assert_eq!(
            policy.get_listen_address().unwrap().to_str().unwrap(),
            "127.0.0.1:34567"
        );
        assert_eq!(policy.get_method().unwrap().to_str().unwrap(), "POST");
        assert_eq!(policy.get_path().unwrap().to_str().unwrap(), "/invoke");
        assert_eq!(
            policy.get_query_path().unwrap().to_str().unwrap(),
            "/result"
        );
        assert!(!policy.get_tls_required());
        assert!(response.get_issued_token().unwrap().is_empty());
    });
    rejected(&protocol::respond(&mut w, &publish).unwrap());
    let wrong_kind = frame(wire::Action::ServiceAuthenticationIssue, |mut r| {
        r.set_service_reference(&publication);
        r.set_revision(1);
        r.set_principal_id("alice");
        r.set_service_days(1);
    });
    rejected(&protocol::respond(&mut w, &wrong_kind).unwrap());
    for action in [
        wire::Action::ServiceConfigPage,
        wire::Action::ServiceAuthorityPage,
    ] {
        let listed = protocol::respond(&mut w, &frame(action, |_| {})).unwrap();
        read(&listed, |response| {
            success(response);
            assert_eq!(response.get_service_snapshot().unwrap().len(), 32);
            assert!(response.get_issued_token().unwrap().is_empty());
        });
        assert!(!listed.windows(token.len()).any(|v| v == token.as_slice()));
        assert!(!listed.windows(hash.len()).any(|v| v == hash.as_slice()));
    }
    for reference in [&auth, &publication] {
        let disabled = protocol::respond(
            &mut w,
            &frame(wire::Action::ServiceAuthorityDisable, |mut r| {
                r.set_service_reference(reference);
                r.set_revision(1);
            }),
        )
        .unwrap();
        read(&disabled, |response| {
            success(response);
            let info = response.get_service_authorities().unwrap().get(0);
            assert!(info.get_disabled());
            assert_eq!(info.get_revision(), 2);
        });
    }
    let disabled = protocol::respond(
        &mut w,
        &frame(wire::Action::ServiceConfigDisable, |mut r| {
            r.set_id(&id);
            r.set_revision(1);
        }),
    )
    .unwrap();
    read(&disabled, |response| {
        success(response);
        let info = response.get_service_configs().unwrap().get(0);
        assert!(info.get_disabled());
        assert_eq!(info.get_revision(), 2);
    });
}

#[test]
fn malformed_oversized_and_nested_overlimit_requests_return_only_errors_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let package = install(&mut w, dir.path());
    let auth = w.issue_service_authentication(&[], 0, "alice", 3).unwrap();
    let registry = revision(&w);
    let before = w.service_config_page("", &[]).unwrap().snapshot;
    for case in 0..8 {
        let malformed = frame(wire::Action::ServiceConfigSave, |r| {
            let mut c = r.init_service_config();
            fill_config(c.reborrow(), &package, registry, &auth.info.reference);
            match case {
                0 => {
                    c.init_principals(65);
                }
                1 => {
                    c.reborrow().init_principals(1).get(0).init_scopes(129);
                }
                2 => {
                    let mut principals = c.init_principals(2);
                    principals.reborrow().get(0).init_scopes(65);
                    principals.get(1).init_scopes(64);
                }
                3 => c.set_package_digest(&[3; 31]),
                4 => c.set_service("x".repeat(257)),
                5 => {
                    c.reborrow()
                        .init_principals(1)
                        .get(0)
                        .set_id("x".repeat(129));
                }
                6 => {
                    let mut p = c.init_principals(1).get(0);
                    p.set_id("alice");
                    p.set_authentication_reference(&auth.info.reference);
                    let mut s = p.init_scopes(1).get(0);
                    s.set_kind(2);
                    s.set_card_id("x".repeat(257));
                }
                7 => c.set_package_digest(&[0; 32]),
                _ => unreachable!(),
            }
        });
        rejected(&protocol::respond(&mut w, &malformed).unwrap());
        assert_eq!(w.service_config_page("", &[]).unwrap().snapshot, before);
    }
    let long = frame(wire::Action::ServiceAuthenticationIssue, |mut r| {
        r.set_principal_id("secret-sentinel".repeat(20));
        r.set_service_days(1);
    });
    let response = protocol::respond(&mut w, &long).unwrap();
    rejected(&response);
    assert!(
        !response
            .windows(b"secret-sentinel".len())
            .any(|v| v == b"secret-sentinel")
    );
    for action in [
        wire::Action::ServiceConfigSave,
        wire::Action::ServicePublicationSave,
        wire::Action::ServiceAuthorityDisable,
    ] {
        rejected(&protocol::respond(&mut w, &frame(action, |_| {})).unwrap());
    }
    let mut truncated = frame(wire::Action::ServiceAuthenticationIssue, |_| {});
    truncated.pop();
    rejected(&protocol::respond(&mut w, &truncated).unwrap());
    let oversized = frame(wire::Action::ServiceAuthenticationIssue, |mut r| {
        r.set_principal_id("alice");
        r.set_service_days(1);
        r.set_payload(&vec![9; 128 * 1024]);
    });
    assert!(oversized.len() > 128 * 1024);
    rejected(&protocol::respond(&mut w, &oversized).unwrap());
    assert_eq!(w.service_authority_page(&[], &[]).unwrap().entries.len(), 1);
}

#[test]
fn maximum_principals_and_scopes_fit_one_config_response_and_roundtrip_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let package = install(&mut w, dir.path());
    let mut auths = Vec::new();
    for i in 0..64 {
        let id = format!("principal-{i:02}-{}", "x".repeat(115));
        assert_eq!(id.len(), 128);
        let issued = w.issue_service_authentication(&[], 0, &id, 3).unwrap();
        auths.push((id, issued.info.reference));
    }
    let registry = revision(&w);
    let request = frame(wire::Action::ServiceConfigSave, |r| {
        let mut c = r.init_service_config();
        fill_config(c.reborrow(), &package, registry, &auths[0].1);
        let mut principals = c.init_principals(64);
        for (i, (id, auth)) in auths.iter().enumerate() {
            let mut p = principals.reborrow().get(i as u32);
            p.set_id(id);
            p.set_authentication_reference(auth);
            let mut scopes = p.init_scopes(2);
            for j in 0..2 {
                let mut s = scopes.reborrow().get(j);
                s.set_kind(4);
                s.set_card_id(format!("card-{j}{}", "c".repeat(250)));
                s.set_attachment_id("a".repeat(256));
            }
        }
    });
    assert!(request.len() <= 128 * 1024);
    let response = protocol::respond(&mut w, &request).unwrap();
    read(&response, |r| {
        success(r);
        let config = r.get_service_configs().unwrap().get(0);
        let principals = config.get_principals().unwrap();
        assert_eq!(principals.len(), 64);
        for (i, p) in principals.iter().enumerate() {
            assert_eq!(p.get_id().unwrap().to_str().unwrap(), auths[i].0);
            assert_eq!(p.get_authentication_reference().unwrap(), auths[i].1);
            assert_eq!(p.get_scopes().unwrap().len(), 2);
            assert_eq!(
                p.get_scopes().unwrap().get(1).get_card_id().unwrap().len(),
                256
            );
            assert_eq!(
                p.get_scopes()
                    .unwrap()
                    .get(1)
                    .get_attachment_id()
                    .unwrap()
                    .len(),
                256
            );
        }
    });
    let listed =
        protocol::respond(&mut w, &frame(wire::Action::ServiceConfigPage, |_| {})).unwrap();
    read(&listed, |r| {
        success(r);
        assert_eq!(
            r.get_service_configs()
                .unwrap()
                .get(0)
                .get_principals()
                .unwrap()
                .len(),
            64
        );
    });
}

#[test]
fn maximum_persisted_configuration_including_64_approval_references_fits_page() {
    use morrow_core::service_config::{Config, proto};
    let dir = tempfile::tempdir().unwrap();
    let initial = Config::encode(proto::Configuration {
        schema_version: 1,
        id: "i".repeat(256),
        revision: 1,
        namespace: vec![1; 32],
        retention_ms: 30 * 86_400_000,
        service: "s".repeat(256),
        handler: "h".repeat(256),
        package_sha256: vec![2; 32],
        disabled: false,
        principals: (0..64)
            .map(|i| proto::Principal {
                id: format!("{i:02}{}", "p".repeat(126)),
                authentication_reference: vec![i + 1; 32],
                content_scopes: (0..2)
                    .map(|j| proto::ContentScope {
                        kind: 4,
                        card_id: format!("{j}{}", "c".repeat(255)),
                        attachment_id: "a".repeat(256),
                    })
                    .collect(),
            })
            .collect(),
        approval_references: (1..=64).map(|i| vec![i; 32]).collect(),
    })
    .unwrap();
    // Seed a maximum legal historical row independently of current package or
    // authentication availability. Listing must return metadata without grants.
    {
        let mut store =
            morrow_core::store::Store::open(&dir.path().join("workbench.db"), Default::default())
                .unwrap();
        store.save_service_config_local(&initial, 0).unwrap();
    }
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let listed =
        protocol::respond(&mut w, &frame(wire::Action::ServiceConfigPage, |_| {})).unwrap();
    read(&listed, |r| {
        success(r);
        let config = r.get_service_configs().unwrap().get(0);
        assert_eq!(config.get_id().unwrap().len(), 256);
        assert_eq!(config.get_digest().unwrap(), initial.digest());
        assert_eq!(config.get_principals().unwrap().len(), 64);
        let refs = config.get_approval_references().unwrap();
        assert_eq!(refs.len(), 64);
        for (i, reference) in refs.iter().enumerate() {
            assert_eq!(reference.unwrap(), vec![i as u8 + 1; 32]);
        }
        assert!(r.get_issued_token().unwrap().is_empty());
    });
}

#[test]
fn wire_pages_preserve_separate_text_and_binary_cursors_and_reject_snapshot_drift() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let package = install(&mut w, dir.path());
    let auth = w.issue_service_authentication(&[], 0, "alice", 3).unwrap();
    let registry = revision(&w);
    for _ in 0..2 {
        let request = frame(wire::Action::ServiceConfigSave, |r| {
            fill_config(
                r.init_service_config(),
                &package,
                registry,
                &auth.info.reference,
            )
        });
        read(&protocol::respond(&mut w, &request).unwrap(), success);
    }
    for name in ["bob", "charlie"] {
        w.issue_service_authentication(&[], 0, name, 3).unwrap();
    }
    let page = protocol::respond(&mut w, &frame(wire::Action::ServiceConfigPage, |_| {})).unwrap();
    let (cursor, snapshot) = read(&page, |r| {
        success(r);
        assert!(r.get_service_cursor().unwrap().is_empty());
        (
            r.get_cursor().unwrap().to_str().unwrap().to_owned(),
            r.get_service_snapshot().unwrap().to_vec(),
        )
    });
    assert!(!cursor.is_empty());
    let next_request = frame(wire::Action::ServiceConfigPage, |mut r| {
        r.set_cursor(&cursor);
        r.set_service_snapshot(&snapshot);
    });
    read(&protocol::respond(&mut w, &next_request).unwrap(), |r| {
        success(r);
        assert!(r.get_cursor().unwrap().is_empty());
        assert_eq!(r.get_service_configs().unwrap().len(), 1);
    });
    let missing_snapshot = frame(wire::Action::ServiceConfigPage, |mut r| {
        r.set_cursor(&cursor)
    });
    rejected(&protocol::respond(&mut w, &missing_snapshot).unwrap());
    w.disable_service_config(&cursor, 1).unwrap();
    rejected(&protocol::respond(&mut w, &next_request).unwrap());
    let page =
        protocol::respond(&mut w, &frame(wire::Action::ServiceAuthorityPage, |_| {})).unwrap();
    let (cursor, snapshot) = read(&page, |r| {
        success(r);
        assert!(r.get_cursor().unwrap().is_empty());
        assert_eq!(r.get_service_authorities().unwrap().len(), 2);
        (
            r.get_service_cursor().unwrap().to_vec(),
            r.get_service_snapshot().unwrap().to_vec(),
        )
    });
    assert_eq!(cursor.len(), 32);
    let next_request = frame(wire::Action::ServiceAuthorityPage, |mut r| {
        r.set_service_cursor(&cursor);
        r.set_service_snapshot(&snapshot);
    });
    read(&protocol::respond(&mut w, &next_request).unwrap(), |r| {
        success(r);
        assert!(r.get_service_cursor().unwrap().is_empty());
        assert_eq!(r.get_service_authorities().unwrap().len(), 1);
    });
    w.disable_service_authority(&auth.info.reference, 1)
        .unwrap();
    rejected(&protocol::respond(&mut w, &next_request).unwrap());
}

#[test]
fn all_service_actions_keep_the_shared_busy_gate_while_a_real_worker_owns_storage() {
    use morrow_core::io::{HttpSubmission, Request};
    use morrow_plugin_runtime::io_jobs::{BrokerRouter, JobLimits, RouteContext, RouterFault};
    use morrow_workbench_host::io_tasks::{AccessError, PreparedJob, StartOptions};
    use std::{
        collections::BTreeSet,
        sync::mpsc,
        time::{Duration, Instant},
    };
    const WAIT: Duration = Duration::from_secs(10);
    struct HeldRouter {
        entered: mpsc::SyncSender<()>,
        release: mpsc::Receiver<()>,
    }
    impl BrokerRouter for HeldRouter {
        fn route(
            &mut self,
            _: &mut RouteContext<'_>,
            _: u32,
            _: &Request,
        ) -> Result<Vec<u8>, RouterFault> {
            self.entered.send(()).map_err(|_| RouterFault::Unknown)?;
            self.release
                .recv_timeout(WAIT)
                .map_err(|_| RouterFault::Unknown)?;
            Err(RouterFault::Denied)
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let wasm = wat::parse_str(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 4)
      (func (export "morrow_run") (result i32) (local $n i32)
        i32.const 0 i32.const 131072 call $read local.set $n
        i32.const 131072 i32.const 0 local.get $n i32.const 131072 i32.const 131072
        call $io call $done drop i32.const 0))"#,
    )
    .unwrap();
    let mut manifest = Package::manifest_for_task("test.service-busy", "1.0.0", &wasm, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    manifest.io_declaration = Some(io::declaration(
        vec![IoCapability::HttpRequest],
        vec!["io.invoke".into()],
    ));
    let package = Package::build(manifest, &wasm).unwrap();
    let path = dir.path().join("busy.mplugin");
    std::fs::write(&path, package.archive()).unwrap();
    w.import_plugin(&path, &package.digest(), revision(&w))
        .unwrap();
    w.configure_external_io(
        &package.manifest().package_id,
        &package.digest(),
        revision(&w),
        &["http-request".into()],
    )
    .unwrap();
    w.configure_external(
        &package.manifest().package_id,
        &package.digest(),
        revision(&w),
        &[],
        true,
    )
    .unwrap();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    w.start_io(
        StartOptions {
            package_id: package.manifest().package_id.clone(),
            digest: package.digest(),
            revision: revision(&w),
            capabilities: BTreeSet::from([IoCapability::HttpRequest]),
            lifetime: WAIT,
            limits: JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
        },
        |_| {
            let request = Request::encode_http_submit(
                1,
                &HttpSubmission {
                    operation_id: b"service-busy".to_vec(),
                    deadline_ms: 0,
                    endpoint: vec![b'1'; 64],
                    method: "GET".into(),
                    relative_target: "/".into(),
                    headers: vec![],
                    body: vec![],
                    credential: vec![],
                },
            )?;
            Ok(PreparedJob {
                input: request.bytes().to_vec(),
                router: Box::new(HeldRouter {
                    entered: entered_tx,
                    release: release_rx,
                }),
                timeout: WAIT,
            })
        },
    )
    .unwrap();
    entered_rx.recv_timeout(WAIT).unwrap();
    for action in [
        wire::Action::ServiceConfigPage,
        wire::Action::ServiceConfigSave,
        wire::Action::ServiceConfigDisable,
        wire::Action::ServiceAuthorityPage,
        wire::Action::ServiceAuthenticationIssue,
        wire::Action::ServiceAuthorityDisable,
        wire::Action::ServicePublicationSave,
    ] {
        let response = protocol::respond(&mut w, &frame(action, |_| {})).unwrap();
        rejected(&response);
        read(&response, |r| assert_eq!(r.get_ui_code(), 110));
    }
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + WAIT;
    loop {
        match w.finish() {
            Err(e) if e.downcast_ref::<AccessError>() == Some(&AccessError::Busy) => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            result => {
                result.unwrap();
                break;
            }
        }
    }
}
