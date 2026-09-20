#![cfg(target_os = "windows")]
use capnp::{message::Builder, serialize};
use morrow_core::{
    outbound_authority::{
        Record,
        proto::{self, record::Kind},
    },
    plugin_package::{
        Package, catalog,
        io::{self, IoCapability},
    },
};
use morrow_workbench_host::{Workbench, endpoint_control::EndpointUpdate};
use morrow_workbench_host::{host_capnp as wire, protocol};
use std::path::Path;

fn request(action: wire::Action, fill: impl FnOnce(wire::request::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut r = message.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    fill(r);
    serialize::write_message_to_words(&message)
}
fn write_policy(value: &proto::Endpoint, mut p: wire::endpoint_policy::Builder<'_>) {
    p.set_package_id(&value.package_id);
    p.set_package_digest(&value.package_sha256);
    p.set_origin(&value.origin);
    p.set_profile(value.profile.try_into().unwrap());
    p.set_credential_reference(&value.credential_reference);
    p.set_root_certificate(&value.root_certificate);
    p.set_max_request_bytes(value.max_request_bytes.try_into().unwrap());
    p.set_max_response_bytes(value.max_response_bytes.try_into().unwrap());
    p.set_max_header_bytes(value.max_header_bytes.try_into().unwrap());
    p.set_max_concurrent(value.max_concurrent.try_into().unwrap());
    p.set_timeout_ms(value.timeout_ms.try_into().unwrap());
    p.set_max_frame_bytes(value.max_frame_bytes.try_into().unwrap());
    let mut methods = p.init_methods(value.methods.len().try_into().unwrap());
    for (i, method) in value.methods.iter().enumerate() {
        methods.set(i as u32, method);
    }
}
fn assert_policy(p: wire::endpoint_policy::Reader<'_>, value: &proto::Endpoint) {
    assert_eq!(
        p.get_package_id().unwrap().to_str().unwrap(),
        value.package_id
    );
    assert_eq!(p.get_package_digest().unwrap(), value.package_sha256);
    assert_eq!(p.get_origin().unwrap().to_str().unwrap(), value.origin);
    assert_eq!(i32::from(p.get_profile()), value.profile);
    assert_eq!(
        p.get_credential_reference().unwrap(),
        value.credential_reference
    );
    assert_eq!(p.get_root_certificate().unwrap(), value.root_certificate);
    assert_eq!(
        u64::from(p.get_max_request_bytes()),
        value.max_request_bytes
    );
    assert_eq!(
        u64::from(p.get_max_response_bytes()),
        value.max_response_bytes
    );
    assert_eq!(u64::from(p.get_max_header_bytes()), value.max_header_bytes);
    assert_eq!(u32::from(p.get_max_concurrent()), value.max_concurrent);
    assert_eq!(u64::from(p.get_timeout_ms()), value.timeout_ms);
    assert_eq!(u64::from(p.get_max_frame_bytes()), value.max_frame_bytes);
    let methods: Vec<_> = p
        .get_methods()
        .unwrap()
        .iter()
        .map(|v| v.unwrap().to_str().unwrap().to_owned())
        .collect();
    assert_eq!(methods, value.methods);
}

#[test]
fn private_protocol_save_page_disable_roundtrips_complete_policy_and_cas() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.endpoint-wire", "1.0.0", true);
    approve(&mut w, &p, true);
    let credential = w
        .save_credential(&[], 0, "x-api-key", "wire-secret-sentinel", 3)
        .unwrap();
    let mut expected = policy(&p);
    expected.methods = vec!["DELETE".into(), "GET".into(), "PATCH".into()];
    expected.credential_reference = credential.reference.to_vec();
    expected.max_request_bytes = 3137;
    expected.max_response_bytes = 8193;
    expected.max_header_bytes = 6145;
    expected.max_concurrent = 7;
    expected.timeout_ms = 4513;
    expected.max_frame_bytes = 16385;
    let registry_revision = revision(&w);
    let input = request(wire::Action::EndpointSave, |mut r| {
        r.set_endpoint_registry_revision(registry_revision);
        r.set_endpoint_days(1);
        write_policy(&expected, r.init_endpoint_policy());
    });
    let output = protocol::respond(&mut w, &input).unwrap();
    let message = serialize::read_message(&mut &output[..], Default::default()).unwrap();
    let response = message.get_root::<wire::response::Reader>().unwrap();
    assert!(response.get_error().unwrap().is_empty());
    let saved = response.get_endpoints().unwrap().get(0);
    let reference = saved.get_reference().unwrap().to_vec();
    let created = saved.get_created_ms();
    let expires = saved.get_expires_ms();
    assert_eq!(saved.get_revision(), 1);
    assert!(!saved.get_disabled());
    assert_policy(saved.get_policy().unwrap(), &expected);
    for (key, expected_revision) in [
        (reference.as_slice(), 0),
        (credential.reference.as_slice(), 1),
    ] {
        let input = request(wire::Action::EndpointSave, |mut r| {
            r.set_endpoint_reference(key);
            r.set_revision(expected_revision);
            r.set_endpoint_registry_revision(registry_revision);
            r.set_endpoint_days(1);
            write_policy(&expected, r.init_endpoint_policy());
        });
        let output = protocol::respond(&mut w, &input).unwrap();
        let message = serialize::read_message(&mut &output[..], Default::default()).unwrap();
        let response = message.get_root::<wire::response::Reader>().unwrap();
        assert!(!response.get_error().unwrap().is_empty());
        assert!(response.get_endpoints().unwrap().is_empty());
    }
    let output = protocol::respond(&mut w, &request(wire::Action::EndpointPage, |_| {})).unwrap();
    assert!(
        !output
            .windows(b"wire-secret-sentinel".len())
            .any(|v| v == b"wire-secret-sentinel")
    );
    let message = serialize::read_message(&mut &output[..], Default::default()).unwrap();
    let response = message.get_root::<wire::response::Reader>().unwrap();
    assert!(response.get_error().unwrap().is_empty());
    assert_eq!(response.get_endpoint_snapshot().unwrap().len(), 32);
    assert!(response.get_endpoint_cursor().unwrap().is_empty());
    let entries = response.get_endpoints().unwrap();
    assert_eq!(entries.len(), 1);
    assert_policy(entries.get(0).get_policy().unwrap(), &expected);
    assert_eq!(entries.get(0).get_reference().unwrap(), reference);
    for (key, expected_revision, success) in [
        (credential.reference.as_slice(), 1, false),
        (reference.as_slice(), 0, false),
        (reference.as_slice(), 1, true),
    ] {
        let output = protocol::respond(
            &mut w,
            &request(wire::Action::EndpointDisable, |mut r| {
                r.set_endpoint_reference(key);
                r.set_revision(expected_revision);
            }),
        )
        .unwrap();
        let message = serialize::read_message(&mut &output[..], Default::default()).unwrap();
        let response = message.get_root::<wire::response::Reader>().unwrap();
        assert_eq!(response.get_error().unwrap().is_empty(), success);
        if success {
            let disabled = response.get_endpoints().unwrap().get(0);
            assert!(disabled.get_disabled());
            assert_eq!(disabled.get_revision(), 2);
            assert_eq!(disabled.get_created_ms(), created);
            assert_eq!(disabled.get_expires_ms(), expires);
            assert_policy(disabled.get_policy().unwrap(), &expected);
        }
    }
    let credential = w.credential_page(&[], &[]).unwrap().entries.remove(0);
    assert_eq!(credential.revision, 1);
    assert!(!credential.disabled);
}

fn revision(w: &Workbench) -> u64 {
    w.catalog_page("", None).unwrap().revision
}
fn install(w: &mut Workbench, dir: &Path, id: &str, version: &str, credential: bool) -> Package {
    let base = catalog::read_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("sdk/compat/guest-v1-rc1/rust-task.mplugin"),
    )
    .unwrap();
    let mut manifest = base.manifest().clone();
    manifest.package_id = id.into();
    manifest.package_version = version.into();
    manifest.required_features.push(io::FEATURE.into());
    let mut caps = vec![IoCapability::HttpRequest];
    if credential {
        caps.push(IoCapability::CredentialUse);
    }
    manifest.io_declaration = Some(io::declaration(caps, vec!["api.invoke".into()]));
    let package = Package::build(manifest, base.module()).unwrap();
    let path = dir.join("incoming.mplugin");
    std::fs::write(&path, package.archive()).unwrap();
    w.import_plugin(&path, &package.digest(), revision(w))
        .unwrap();
    package
}
fn approve(w: &mut Workbench, p: &Package, credential: bool) {
    let mut caps = vec!["http-request".into()];
    if credential {
        caps.push("credential-use".into());
    }
    w.configure_external_io(&p.manifest().package_id, &p.digest(), revision(w), &caps)
        .unwrap();
}
fn policy(p: &Package) -> proto::Endpoint {
    proto::Endpoint {
        package_id: p.manifest().package_id.clone(),
        package_sha256: p.digest().to_vec(),
        origin: "https://api.example".into(),
        profile: 1,
        methods: vec!["GET".into()],
        credential_reference: vec![],
        root_certificate: vec![],
        max_request_bytes: 1024,
        max_response_bytes: 2048,
        max_header_bytes: 4096,
        max_concurrent: 1,
        timeout_ms: 1000,
        max_frame_bytes: 8192,
    }
}
fn update(
    w: &Workbench,
    policy: proto::Endpoint,
    reference: &[u8],
    expected_revision: u64,
) -> EndpointUpdate {
    EndpointUpdate {
        reference: reference.to_vec(),
        expected_revision,
        registry_revision: revision(w),
        lifetime_days: 1,
        policy,
    }
}
fn seeded_endpoint(reference: [u8; 32]) -> Record {
    Record::encode(proto::Record {
        schema_version: 1,
        reference: reference.to_vec(),
        revision: 1,
        created_ms: 10,
        expires_ms: 1010,
        disabled: false,
        kind: Some(Kind::Endpoint(proto::Endpoint {
            package_id: "test.missing".into(),
            package_sha256: vec![3; 32],
            origin: "https://api.example".into(),
            profile: 1,
            methods: vec!["GET".into()],
            credential_reference: vec![],
            root_certificate: vec![],
            max_request_bytes: 1024,
            max_response_bytes: 2048,
            max_header_bytes: 4096,
            max_concurrent: 1,
            timeout_ms: 1000,
            max_frame_bytes: 8192,
        })),
    })
    .unwrap()
}

#[test]
fn explicit_approval_cas_restart_and_package_upgrade_never_enable_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.endpoint", "1.0.0", false);
    assert!(w.save_endpoint(update(&w, policy(&p), &[], 0)).is_err());
    approve(&mut w, &p, false);
    let first = w.save_endpoint(update(&w, policy(&p), &[], 0)).unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(first.expires_ms - first.created_ms, 86_400_000);
    let before = w.endpoint_page(&[], &[]).unwrap().snapshot;
    assert!(
        w.save_endpoint(update(&w, policy(&p), &first.reference, 0))
            .is_err()
    );
    assert!(w.disable_endpoint(&first.reference, 0).is_err());
    assert_eq!(w.endpoint_page(&[], &[]).unwrap().snapshot, before);
    let p2 = install(&mut w, dir.path(), "test.endpoint", "2.0.0", false);
    approve(&mut w, &p2, false);
    assert!(
        w.save_endpoint(update(&w, policy(&p), &first.reference, 1))
            .is_err()
    );
    let second = w
        .save_endpoint(update(&w, policy(&p2), &first.reference, 1))
        .unwrap();
    assert_eq!(second.revision, 2);
    assert_eq!(second.policy.package_sha256, p2.digest());
    assert!(
        !w.catalog_page("", None)
            .unwrap()
            .entries
            .iter()
            .find(|v| v.id == "test.endpoint")
            .unwrap()
            .enabled
    );
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let page = w.endpoint_page(&[], &[]).unwrap();
    assert_eq!(page.entries[0].revision, 2);
    assert_eq!(page.entries[0].reference, first.reference);
    let disabled = w.disable_endpoint(&first.reference, 2).unwrap();
    assert!(disabled.disabled);
    assert_eq!(disabled.created_ms, second.created_ms);
    assert_eq!(disabled.expires_ms, second.expires_ms);
    assert_eq!(w.disable_endpoint(&first.reference, 3).unwrap().revision, 3);
}

#[test]
fn policy_and_registry_validation_leave_store_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.endpoint", "1.0.0", false);
    approve(&mut w, &p, false);
    let before = w.endpoint_page(&[], &[]).unwrap().snapshot;
    let mut invalid = Vec::new();
    for origin in [
        "https://api.example/",
        "https://API.example",
        "https://api.example:443",
        "https://127.0.0.1",
        "http://api.example",
        "https://user@api.example",
        "https://api.example/path",
    ] {
        let mut value = policy(&p);
        value.origin = origin.into();
        invalid.push(value);
    }
    let mut value = policy(&p);
    value.methods = vec!["TRACE".into()];
    invalid.push(value);
    let mut value = policy(&p);
    value.methods = vec!["POST".into(), "GET".into()];
    invalid.push(value);
    let mut value = policy(&p);
    value.root_certificate =
        b"-----BEGIN CERTIFICATE-----\ninvalid\n-----END CERTIFICATE-----".to_vec();
    invalid.push(value);
    let mut value = policy(&p);
    value.max_concurrent = 129;
    invalid.push(value);
    let mut value = policy(&p);
    value.max_request_bytes = 0;
    invalid.push(value);
    let mut value = policy(&p);
    value.profile = 2;
    invalid.push(value);
    let mut value = policy(&p);
    value.package_sha256 = vec![5; 32];
    invalid.push(value);
    for value in invalid {
        assert!(w.save_endpoint(update(&w, value, &[], 0)).is_err());
    }
    for days in [0, 31] {
        let mut value = update(&w, policy(&p), &[], 0);
        value.lifetime_days = days;
        assert!(w.save_endpoint(value).is_err());
    }
    let mut stale = update(&w, policy(&p), &[], 0);
    stale.registry_revision -= 1;
    assert!(w.save_endpoint(stale).is_err());
    assert_eq!(w.endpoint_page(&[], &[]).unwrap().snapshot, before);
    let mut local = policy(&p);
    local.origin = "http://127.0.0.1:32991".into();
    local.profile = 2;
    // No listener exists: saving validates policy without making a request.
    assert!(w.save_endpoint(update(&w, local, &[], 0)).is_ok());
}

#[test]
fn wrong_record_kind_and_changed_package_identity_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.first", "1.0.0", true);
    approve(&mut w, &p, true);
    let credential = w
        .save_credential(&[], 0, "x-api-key", "wrong-kind-sentinel", 7)
        .unwrap();
    assert!(
        w.save_endpoint(update(&w, policy(&p), &credential.reference, 1))
            .is_err()
    );
    assert!(w.disable_endpoint(&credential.reference, 1).is_err());
    let first = w.save_endpoint(update(&w, policy(&p), &[], 0)).unwrap();
    let p2 = install(&mut w, dir.path(), "test.second", "1.0.0", true);
    approve(&mut w, &p2, true);
    assert!(
        w.save_endpoint(update(&w, policy(&p2), &first.reference, 1))
            .is_err()
    );
    let mut wrong = policy(&p);
    wrong.credential_reference = first.reference.to_vec();
    assert!(w.save_endpoint(update(&w, wrong, &[], 0)).is_err());
    assert_eq!(w.credential_page(&[], &[]).unwrap().entries[0].revision, 1);
}

#[test]
fn credential_approval_reference_and_expiry_are_checked_without_decrypting() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(
        &mut w,
        dir.path(),
        "test.credential-endpoint",
        "1.0.0",
        true,
    );
    approve(&mut w, &p, false);
    let credential = w
        .save_credential(&[], 0, "x-api-key", "test-secret", 2)
        .unwrap();
    let mut value = policy(&p);
    value.credential_reference = credential.reference.to_vec();
    assert!(w.save_endpoint(update(&w, value.clone(), &[], 0)).is_err());
    approve(&mut w, &p, true);
    let mut too_long = update(&w, value.clone(), &[], 0);
    too_long.lifetime_days = 3;
    assert!(w.save_endpoint(too_long).is_err());
    assert!(w.save_endpoint(update(&w, value.clone(), &[], 0)).is_ok());
    w.disable_credential(&credential.reference, 1).unwrap();
    assert!(w.save_endpoint(update(&w, value, &[], 0)).is_err());
    let mut missing = policy(&p);
    missing.credential_reference = vec![33; 32];
    assert!(w.save_endpoint(update(&w, missing, &[], 0)).is_err());
}

#[test]
fn missing_package_does_not_block_explicit_disable() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let p = install(&mut w, dir.path(), "test.endpoint", "1.0.0", false);
    approve(&mut w, &p, false);
    let first = w.save_endpoint(update(&w, policy(&p), &[], 0)).unwrap();
    let digest: String = p.digest().iter().map(|b| format!("{b:02x}")).collect();
    std::fs::remove_file(
        dir.path()
            .join(format!("plugin-manager/packages/{digest}.mplugin")),
    )
    .unwrap();
    assert!(
        w.save_endpoint(update(&w, policy(&p), &first.reference, 1))
            .is_err()
    );
    assert!(w.save_endpoint(update(&w, policy(&p), &[], 0)).is_err());
    assert!(w.disable_endpoint(&first.reference, 1).unwrap().disabled);
}

#[test]
fn mixed_empty_page_retains_cursor_snapshot_across_restart_and_detects_drift() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    {
        let mut store = morrow_core::store::Store::open(&db, Default::default()).unwrap();
        for byte in [1, 2] {
            let record = Record::encode(proto::Record {
                schema_version: 1,
                reference: vec![byte; 32],
                revision: 1,
                created_ms: 10,
                expires_ms: 1010,
                disabled: false,
                kind: Some(Kind::Credential(proto::Credential {
                    provider: morrow_core::outbound_authority::WINDOWS_DPAPI_PROVIDER.into(),
                    ciphertext: vec![9; 32],
                })),
            })
            .unwrap();
            store.save_outbound_authority_local(&record, 0).unwrap();
        }
        store
            .save_outbound_authority_local(&seeded_endpoint([3; 32]), 0)
            .unwrap();
    }
    let mut w = Workbench::open(&db, None).unwrap();
    let first = w.endpoint_page(&[], &[]).unwrap();
    assert!(first.entries.is_empty());
    assert_eq!(first.next, Some([2; 32]));
    assert!(w.endpoint_page(&[2; 32], &[]).is_err());
    assert!(w.endpoint_page(&[4; 32], &first.snapshot).is_err());
    assert!(w.endpoint_page(&[0; 31], &first.snapshot).is_err());
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open(&db, None).unwrap();
    let next = w
        .endpoint_page(&first.next.unwrap(), &first.snapshot)
        .unwrap();
    assert_eq!(next.entries.len(), 1);
    assert_eq!(next.entries[0].reference, [3; 32]);
    assert!(next.next.is_none());
    assert_eq!(next.entries[0].created_ms, 10);
    // Expired metadata stays visible; disabling does not renew its lifetime.
    let disabled = w.disable_endpoint(&[3; 32], 1).unwrap();
    assert_eq!(disabled.expires_ms, 1010);
    assert!(w.endpoint_page(&[2; 32], &first.snapshot).is_err());
}
