#![cfg(target_os = "windows")]

use morrow_core::{
    plugin_package::{
        Package,
        io::{self, IoCapability},
    },
    service_authority::{Record, proto as authority},
    service_config::{Config, proto as config},
};
use morrow_workbench_host::{
    Workbench,
    service_control::{AuthorityInfo, AuthorityKind, PublicationUpdate, ServiceConfigUpdate},
};
use sha2::{Digest, Sha256};
use std::{net::TcpListener, path::Path};

const DAY: u64 = 86_400_000;
const HANDLER: &str = "test.service.invoke";

fn revision(w: &Workbench) -> u64 {
    w.catalog_page("", None).unwrap().revision
}

fn install(w: &mut Workbench, root: &Path, id: &str, version: &str, service: bool) -> Package {
    // A real ABI-v2 module, deliberately never executed by administration.
    let wasm = wat::parse_str(
        r#"(module
        (memory (export "memory") 1)
        (func (export "morrow_run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    let mut manifest = Package::manifest_for_task(id, version, &wasm, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    let mut declaration = io::declaration(
        vec![IoCapability::HttpListen, IoCapability::HttpPublish],
        vec![HANDLER.into()],
    );
    if service {
        declaration.service_schema_sha256 = morrow_core::service::schema_digest().to_vec();
    }
    manifest.io_declaration = Some(declaration);
    let package = Package::build(manifest, &wasm).unwrap();
    let incoming = root.join("service-incoming.mplugin");
    std::fs::write(&incoming, package.archive()).unwrap();
    w.import_plugin(&incoming, &package.digest(), revision(w))
        .unwrap();
    package
}

fn approve(w: &mut Workbench, package: &Package, caps: &[&str]) {
    w.configure_external_io(
        &package.manifest().package_id,
        &package.digest(),
        revision(w),
        &caps.iter().map(|v| (*v).to_owned()).collect::<Vec<_>>(),
    )
    .unwrap();
}

fn principal(id: &str, reference: &[u8]) -> config::Principal {
    config::Principal {
        id: id.into(),
        authentication_reference: reference.to_vec(),
        content_scopes: vec![config::ContentScope {
            kind: 2,
            card_id: "card.service-test".into(),
            attachment_id: String::new(),
        }],
    }
}

fn update(
    w: &Workbench,
    package: &Package,
    previous: Option<&Config>,
    auth: &[u8],
) -> ServiceConfigUpdate {
    ServiceConfigUpdate {
        id: previous.map_or_else(String::new, |v| v.value().id.clone()),
        expected_revision: previous.map_or(0, |v| v.value().revision),
        registry_revision: revision(w),
        package_id: package.manifest().package_id.clone(),
        package_digest: package.digest(),
        service: "test.service".into(),
        handler: HANDLER.into(),
        retention_ms: DAY,
        principals: vec![principal("alice", auth)],
    }
}

fn publication(
    w: &Workbench,
    package: &Package,
    config: &Config,
    address: &str,
    previous_revision: u64,
) -> PublicationUpdate {
    PublicationUpdate {
        reference: config.value().approval_references[0]
            .as_slice()
            .try_into()
            .unwrap(),
        expected_revision: previous_revision,
        config_id: config.value().id.clone(),
        config_revision: config.value().revision,
        config_digest: config.digest(),
        registry_revision: revision(w),
        package_id: package.manifest().package_id.clone(),
        lifetime_days: 1,
        listen_address: address.into(),
        tls_required: false,
        method: "POST".into(),
        path: "/invoke".into(),
        query_path: "/result".into(),
    }
}

fn assert_disabled_package(w: &Workbench, package: &Package) {
    assert!(
        !w.catalog_page("", None)
            .unwrap()
            .entries
            .iter()
            .find(|v| v.id == package.manifest().package_id)
            .unwrap()
            .enabled
    );
}

fn assert_auth_metadata(info: &AuthorityInfo, expected_principal: &str) {
    // Exhaustive public-shape patterns guard against exposing the verifier or
    // bearer token through either the list item or its authentication variant.
    let AuthorityInfo {
        reference,
        revision,
        created_ms,
        expires_ms,
        disabled,
        kind,
    } = info;
    assert_ne!(*reference, [0; 32]);
    assert!(*revision > 0);
    assert!(*created_ms > 0 && *expires_ms > *created_ms);
    assert!(!disabled);
    match kind {
        AuthorityKind::Authentication { principal_id } => {
            assert_eq!(principal_id, expected_principal)
        }
        AuthorityKind::Publication(_) => panic!("expected authentication metadata"),
    }
}

fn stored_authority(root: &Path, reference: &[u8; 32]) -> Record {
    let registry = morrow_audit::library::Registry::open(root).unwrap();
    let selected = registry.selected_database().unwrap();
    let db =
        rusqlite::Connection::open_with_flags(selected, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let payload: Vec<u8> = db
        .query_row(
            "SELECT payload FROM service_authorities WHERE reference=?1",
            [reference.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    Record::decode(&payload).unwrap()
}

#[test]
fn authentication_returns_one_token_persists_only_its_hash_and_rotates_with_cas() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let first = w.issue_service_authentication(&[], 0, "alice", 3).unwrap();
    assert_auth_metadata(&first.info, "alice");
    assert_eq!(first.info.expires_ms - first.info.created_ms, 3 * DAY);
    assert!(!first.token.is_empty());
    let first_hash: [u8; 32] = Sha256::digest(first.token.as_bytes()).into();
    let key = first.info.reference;
    let before = w.service_authority_page(&[], &[]).unwrap().snapshot;
    for (reference, expected, principal_id, days) in [
        (&[][..], 1, "alice", 1),
        (&key[..], 0, "alice", 1),
        (&key[..], 1, "bob", 1),
        (&[][..], 0, "alice", 0),
        (&[][..], 0, "alice", 31),
        (&[][..], 0, "invalid principal", 1),
    ] {
        assert!(
            w.issue_service_authentication(reference, expected, principal_id, days)
                .is_err()
        );
    }
    assert_eq!(w.service_authority_page(&[], &[]).unwrap().snapshot, before);
    let page = w.service_authority_page(&[], &[]).unwrap();
    assert_eq!(page.entries.len(), 1);
    assert_auth_metadata(&page.entries[0], "alice");
    w.finish().unwrap();
    drop(w);
    let stored = stored_authority(dir.path(), &key);
    let Some(authority::record::Kind::Authentication(auth)) = &stored.value().kind else {
        panic!("wrong record type")
    };
    assert_eq!(auth.token_sha256, first_hash);
    assert!(
        !stored
            .container()
            .windows(first.token.len())
            .any(|v| v == first.token.as_bytes())
    );

    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let rotated = w.issue_service_authentication(&key, 1, "alice", 2).unwrap();
    assert_eq!(rotated.info.reference, key);
    assert_eq!(rotated.info.revision, 2);
    assert_ne!(rotated.token.as_str(), first.token.as_str());
    let rotated_hash: [u8; 32] = Sha256::digest(rotated.token.as_bytes()).into();
    assert_ne!(rotated_hash, first_hash);
    assert_auth_metadata(
        &w.service_authority_page(&[], &[]).unwrap().entries[0],
        "alice",
    );
    w.finish().unwrap();
    drop(w);
    let stored = stored_authority(dir.path(), &key);
    let Some(authority::record::Kind::Authentication(auth)) = &stored.value().kind else {
        panic!("wrong record type")
    };
    assert_eq!(auth.token_sha256, rotated_hash);
    assert!(
        !stored
            .container()
            .windows(rotated.token.len())
            .any(|v| v == rotated.token.as_bytes())
    );
}

#[test]
fn configuration_generates_stable_identity_requires_approval_and_survives_upgrade_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.service", "1.0.0", true);
    let auth = w.issue_service_authentication(&[], 0, "alice", 3).unwrap();
    assert!(
        w.save_service_config(update(&w, &p, None, &auth.info.reference))
            .is_err()
    );
    approve(&mut w, &p, &["http-listen"]);
    assert!(
        w.save_service_config(update(&w, &p, None, &auth.info.reference))
            .is_err()
    );
    approve(&mut w, &p, &["http-listen", "http-publish"]);
    let first = w
        .save_service_config(update(&w, &p, None, &auth.info.reference))
        .unwrap();
    assert!(!first.value().id.is_empty());
    assert_eq!(first.value().revision, 1);
    assert_eq!(first.value().namespace.len(), 32);
    assert_ne!(first.value().namespace, [0; 32]);
    assert_eq!(first.value().approval_references.len(), 1);
    assert_eq!(first.value().approval_references[0].len(), 32);
    assert_ne!(first.value().approval_references[0], [0; 32]);
    assert_eq!(
        first.value().principals,
        vec![principal("alice", &auth.info.reference)]
    );
    assert_disabled_package(&w, &p);
    let before = w.service_config_page("", &[]).unwrap().snapshot;
    let mut stale = update(&w, &p, Some(&first), &auth.info.reference);
    stale.expected_revision = 0;
    assert!(w.save_service_config(stale).is_err());
    assert!(w.disable_service_config(&first.value().id, 0).is_err());
    assert_eq!(w.service_config_page("", &[]).unwrap().snapshot, before);
    let p2 = install(&mut w, dir.path(), "test.service", "2.0.0", true);
    approve(&mut w, &p2, &["http-listen", "http-publish"]);
    assert!(
        w.save_service_config(update(&w, &p, Some(&first), &auth.info.reference))
            .is_err()
    );
    let mut replacement = update(&w, &p2, Some(&first), &auth.info.reference);
    replacement.retention_ms = 2 * DAY;
    let second = w.save_service_config(replacement).unwrap();
    assert_eq!(second.value().id, first.value().id);
    assert_eq!(second.value().namespace, first.value().namespace);
    assert_eq!(
        second.value().approval_references,
        first.value().approval_references
    );
    assert_eq!(second.value().revision, 2);
    assert_eq!(second.value().package_sha256, p2.digest());
    assert_eq!(second.value().retention_ms, 2 * DAY);
    assert_disabled_package(&w, &p2);
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let page = w.service_config_page("", &[]).unwrap();
    assert_eq!(page.configs.len(), 1);
    assert_eq!(page.configs[0].container(), second.container());
    assert_disabled_package(&w, &p2);
}

#[test]
fn configuration_rejects_stale_registry_digest_handler_identity_and_auth_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.service", "1.0.0", true);
    approve(&mut w, &p, &["http-listen", "http-publish"]);
    let auth = w.issue_service_authentication(&[], 0, "alice", 3).unwrap();
    let first = w
        .save_service_config(update(&w, &p, None, &auth.info.reference))
        .unwrap();
    let before = w.service_config_page("", &[]).unwrap().snapshot;
    for case in 0..9 {
        let mut value = update(&w, &p, Some(&first), &auth.info.reference);
        match case {
            0 => value.registry_revision -= 1,
            1 => value.package_digest = [9; 32],
            2 => value.handler = "not.declared".into(),
            3 => value.service = "different.service".into(),
            4 => value.principals[0].id = "bob".into(),
            5 => value.principals[0].authentication_reference = vec![19; 32],
            6 => value.retention_ms = 0,
            7 => value.retention_ms = 30 * DAY + 1,
            8 => value.package_id = "not.installed".into(),
            _ => unreachable!(),
        }
        assert!(w.save_service_config(value).is_err(), "case {case}");
        assert_eq!(w.service_config_page("", &[]).unwrap().snapshot, before);
    }
    w.disable_service_authority(&auth.info.reference, 1)
        .unwrap();
    assert!(
        w.save_service_config(update(&w, &p, Some(&first), &auth.info.reference))
            .is_err()
    );
    assert_eq!(w.service_config_page("", &[]).unwrap().snapshot, before);
    let p2 = install(
        &mut w,
        dir.path(),
        "test.no-service-contract",
        "1.0.0",
        false,
    );
    approve(&mut w, &p2, &["http-listen", "http-publish"]);
    let auth2 = w.issue_service_authentication(&[], 0, "alice", 3).unwrap();
    assert!(
        w.save_service_config(update(&w, &p2, None, &auth2.info.reference))
            .is_err()
    );
}

#[test]
fn expired_authentication_cannot_authorize_a_new_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let key = [91; 32];
    {
        let mut store =
            morrow_core::store::Store::open(&dir.path().join("workbench.db"), Default::default())
                .unwrap();
        let record = Record::encode(authority::Record {
            schema_version: 1,
            reference: key.to_vec(),
            revision: 1,
            created_ms: 1,
            expires_ms: 1 + DAY,
            disabled: false,
            kind: Some(authority::record::Kind::Authentication(
                authority::Authentication {
                    principal_id: "alice".into(),
                    token_sha256: vec![5; 32],
                },
            )),
        })
        .unwrap();
        store.save_service_authority_local(&record, 0).unwrap();
    }
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.service", "1.0.0", true);
    approve(&mut w, &p, &["http-listen", "http-publish"]);
    assert!(w.save_service_config(update(&w, &p, None, &key)).is_err());
    assert!(w.service_config_page("", &[]).unwrap().configs.is_empty());
    let disabled = w.disable_service_authority(&key, 1).unwrap();
    assert!(disabled.disabled);
    assert_eq!(disabled.expires_ms, 1 + DAY);
}

#[test]
fn publication_binds_complete_current_configuration_and_does_not_open_a_listener() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.service", "1.0.0", true);
    approve(&mut w, &p, &["http-listen", "http-publish"]);
    let auth = w.issue_service_authentication(&[], 0, "alice", 3).unwrap();
    let first = w
        .save_service_config(update(&w, &p, None, &auth.info.reference))
        .unwrap();
    // An occupied port makes accidental activation fail deterministically.
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reserved.local_addr().unwrap().to_string();
    let before = w.service_authority_page(&[], &[]).unwrap().snapshot;
    for case in 0..12 {
        let mut value = publication(&w, &p, &first, &address, 0);
        match case {
            0 => value.reference = [9; 32],
            1 => value.config_revision += 1,
            2 => value.config_digest = [8; 32],
            3 => value.registry_revision -= 1,
            4 => value.package_id = "missing.package".into(),
            5 => value.lifetime_days = 0,
            6 => value.lifetime_days = 31,
            7 => value.method = "TRACE".into(),
            8 => value.listen_address = "0.0.0.0:34567".into(),
            9 => value.path = "/invoke?unexpected=1".into(),
            10 => value.query_path = value.path.clone(),
            11 => value.reference = auth.info.reference,
            _ => unreachable!(),
        }
        assert!(w.save_service_publication(value).is_err(), "case {case}");
        assert_eq!(w.service_authority_page(&[], &[]).unwrap().snapshot, before);
    }
    let saved = w
        .save_service_publication(publication(&w, &p, &first, &address, 0))
        .unwrap();
    assert_eq!(
        saved.reference.as_slice(),
        first.value().approval_references[0]
    );
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.expires_ms - saved.created_ms, DAY);
    let AuthorityKind::Publication(value) = &saved.kind else {
        panic!("wrong authority kind")
    };
    assert_eq!(value.config_id, first.value().id);
    assert_eq!(value.config_sha256, first.digest());
    assert_eq!(value.listen_address, address);
    assert_eq!(value.method, "POST");
    assert_eq!(value.path, "/invoke");
    assert_eq!(value.query_path, "/result");
    assert!(!value.tls_required);
    assert_disabled_package(&w, &p);
    assert!(
        w.issue_service_authentication(&saved.reference, 1, "alice", 2)
            .is_err()
    );
    let second = w
        .save_service_config(update(&w, &p, Some(&first), &auth.info.reference))
        .unwrap();
    assert!(
        w.save_service_publication(publication(&w, &p, &first, &address, 1))
            .is_err()
    );
    let refreshed = w
        .save_service_publication(publication(&w, &p, &second, &address, 1))
        .unwrap();
    assert_eq!(refreshed.reference, saved.reference);
    assert_eq!(refreshed.revision, 2);
    let mut tls = publication(&w, &p, &second, "0.0.0.0:34567", 2);
    tls.tls_required = true;
    let external = w.save_service_publication(tls).unwrap();
    assert_eq!(external.revision, 3);
    let AuthorityKind::Publication(value) = &external.kind else {
        panic!("wrong authority kind")
    };
    assert!(value.tls_required);
    assert_eq!(value.listen_address, "0.0.0.0:34567");
    // Keep the occupied loopback address as the persisted desired address so
    // reopening tests the same no-activation guarantee as initial saving.
    let restored = w
        .save_service_publication(publication(&w, &p, &second, &address, 3))
        .unwrap();
    assert_eq!(restored.revision, 4);
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let page = w.service_authority_page(&[], &[]).unwrap();
    assert!(
        page.entries
            .iter()
            .any(|v| v.reference == saved.reference && v.revision == 4)
    );
    assert_disabled_package(&w, &p);
    drop(reserved);
    // Reopening desired state also leaves the address available to its owner.
    let _still_available = TcpListener::bind(&address).unwrap();
}

#[test]
fn publication_rechecks_authentication_lifetime_and_missing_package_does_not_block_disable() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.service", "1.0.0", true);
    approve(&mut w, &p, &["http-listen", "http-publish"]);
    let auth = w.issue_service_authentication(&[], 0, "alice", 3).unwrap();
    let short = w.issue_service_authentication(&[], 0, "bob", 1).unwrap();
    let mut value = update(&w, &p, None, &auth.info.reference);
    value
        .principals
        .push(principal("bob", &short.info.reference));
    let first = w.save_service_config(value).unwrap();
    let one_day = w
        .save_service_publication(publication(&w, &p, &first, "127.0.0.1:34567", 0))
        .unwrap();
    assert_eq!(one_day.expires_ms, short.info.expires_ms);
    let mut longer_request = publication(&w, &p, &first, "127.0.0.1:34567", 1);
    longer_request.lifetime_days = 2;
    let bounded = w.save_service_publication(longer_request).unwrap();
    assert_eq!(bounded.expires_ms, short.info.expires_ms);
    assert_eq!(bounded.revision, 2);
    // Requested duration is an upper bound. Extending authentication still
    // requires this separate, explicit rotation and never happens on save.
    let renewed = w
        .issue_service_authentication(&short.info.reference, 1, "bob", 3)
        .unwrap();
    let saved = w
        .save_service_publication(publication(&w, &p, &first, "127.0.0.1:34567", 2))
        .unwrap();
    w.disable_service_authority(&renewed.info.reference, 2)
        .unwrap();
    assert!(
        w.save_service_publication(publication(&w, &p, &first, "127.0.0.1:34567", 3))
            .is_err()
    );
    let digest: String = p.digest().iter().map(|b| format!("{b:02x}")).collect();
    std::fs::remove_file(
        dir.path()
            .join(format!("plugin-manager/packages/{digest}.mplugin")),
    )
    .unwrap();
    assert!(
        w.save_service_config(update(&w, &p, Some(&first), &auth.info.reference))
            .is_err()
    );
    assert!(w.disable_service_authority(&saved.reference, 0).is_err());
    let disabled = w.disable_service_authority(&saved.reference, 3).unwrap();
    assert!(disabled.disabled);
    assert_eq!(disabled.expires_ms, saved.expires_ms);
    assert_eq!(disabled.created_ms, saved.created_ms);
    assert!(
        w.disable_service_authority(&auth.info.reference, 1)
            .unwrap()
            .disabled
    );
    let disabled = w.disable_service_config(&first.value().id, 1).unwrap();
    assert!(disabled.value().disabled);
    assert_eq!(disabled.value().namespace, first.value().namespace);
    assert_eq!(
        disabled.value().approval_references,
        first.value().approval_references
    );
}

#[test]
fn authority_pages_are_bounded_ordered_snapshot_checked_and_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let mut keys = Vec::new();
    for i in 0..19 {
        keys.push(
            w.issue_service_authentication(&[], 0, &format!("principal-{i:02}"), 3)
                .unwrap()
                .info
                .reference,
        );
    }
    keys.sort();
    let first = w.service_authority_page(&[], &[]).unwrap();
    assert!(!first.entries.is_empty());
    assert!(first.entries.len() < keys.len());
    let next = first.next.unwrap();
    let snapshot = first.snapshot;
    assert!(w.service_authority_page(&next, &[]).is_err());
    assert!(w.service_authority_page(&next[..31], &snapshot).is_err());
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let mut found: Vec<_> = first.entries.iter().map(|v| v.reference).collect();
    let mut cursor = next;
    for _ in 0..512 {
        let page = w.service_authority_page(&cursor, &snapshot).unwrap();
        assert_eq!(page.snapshot, snapshot);
        assert!(page.entries.iter().all(|v| v.reference > cursor));
        found.extend(page.entries.iter().map(|v| v.reference));
        let Some(next) = page.next else { break };
        assert!(next > cursor);
        cursor = next;
    }
    assert_eq!(found, keys);
    w.disable_service_authority(&keys[0], 1).unwrap();
    assert!(w.service_authority_page(&next, &snapshot).is_err());
}

#[test]
fn configuration_pages_bind_cursor_to_snapshot_and_disabled_config_cannot_publish() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.service", "1.0.0", true);
    approve(&mut w, &p, &["http-listen", "http-publish"]);
    let auth = w.issue_service_authentication(&[], 0, "alice", 3).unwrap();
    let mut originals = Vec::new();
    for _ in 0..3 {
        originals.push(
            w.save_service_config(update(&w, &p, None, &auth.info.reference))
                .unwrap(),
        );
    }
    originals.sort_by(|a, b| a.value().id.cmp(&b.value().id));
    assert!(originals.windows(2).all(|pair| pair[0].value().namespace
        != pair[1].value().namespace
        && pair[0].value().approval_references != pair[1].value().approval_references));
    let first = w.service_config_page("", &[]).unwrap();
    assert_eq!(first.configs.len(), 1);
    assert_eq!(first.configs[0].container(), originals[0].container());
    let cursor = first.next.unwrap();
    assert!(w.service_config_page(&cursor, &[]).is_err());
    assert!(
        w.service_config_page("missing.config", &first.snapshot)
            .is_err()
    );
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let second = w.service_config_page(&cursor, &first.snapshot).unwrap();
    assert_eq!(second.snapshot, first.snapshot);
    assert_eq!(second.configs[0].container(), originals[1].container());
    let third = w
        .service_config_page(second.next.as_deref().unwrap(), &first.snapshot)
        .unwrap();
    assert_eq!(third.configs[0].container(), originals[2].container());
    assert!(third.next.is_none());
    let disabled = w
        .disable_service_config(&originals[0].value().id, 1)
        .unwrap();
    assert!(disabled.value().disabled);
    assert_eq!(disabled.value().revision, 2);
    assert!(w.service_config_page(&cursor, &first.snapshot).is_err());
    assert!(
        w.save_service_publication(publication(&w, &p, &disabled, "127.0.0.1:34567", 0))
            .is_err()
    );
    // An idempotent repeat of the explicit disable does not invent a revision.
    assert_eq!(
        w.disable_service_config(&disabled.value().id, 2)
            .unwrap()
            .value()
            .revision,
        2
    );
}
