#![cfg(target_os = "windows")]
use capnp::{message::Builder, serialize};
use morrow_core::outbound_authority::{Record, proto::record::Kind};
use morrow_workbench_host::{Workbench, host_capnp as wire, protocol};

fn endpoint(reference: [u8; 32]) -> Record {
    use morrow_core::outbound_authority::proto;
    Record::encode(proto::Record {
        schema_version: 1,
        reference: reference.to_vec(),
        revision: 1,
        created_ms: 10,
        expires_ms: 1010,
        disabled: false,
        kind: Some(Kind::Endpoint(proto::Endpoint {
            package_id: "test.credential-admin".into(),
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

fn request(action: wire::Action, fill: impl FnOnce(wire::request::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut r = message.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    fill(r);
    serialize::write_message_to_words(&message)
}
#[test]
fn original_store_create_replace_disable_survives_restart_without_plugin_or_plaintext_readback() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("workbench.db");
    let mut w = Workbench::open(&database, None).unwrap();
    assert!(!w.plugin_status().unwrap().enabled);
    let first = w
        .save_credential(&[], 0, "Authorization", "Bearer first-test-secret", 7)
        .unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(first.expires_ms - first.created_ms, 7 * 86_400_000);
    assert!(!first.disabled);
    w.finish().unwrap();
    drop(w);
    let c = rusqlite::Connection::open(&database).unwrap();
    let bytes: Vec<u8> = c
        .query_row(
            "SELECT payload FROM outbound_authorities WHERE reference=?1",
            [first.reference.as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !bytes
            .windows(b"first-test-secret".len())
            .any(|b| b == b"first-test-secret")
    );
    let record = Record::decode(&bytes).unwrap();
    assert_eq!(
        morrow_audit::credentials::open(&record).unwrap().value(),
        "Bearer first-test-secret"
    );
    drop(c);
    let mut w = Workbench::open(&database, None).unwrap();
    let page = w.credential_page(&[], &[]).unwrap();
    assert_eq!(page.entries[0].reference, first.reference);
    let second = w
        .save_credential(&first.reference, 1, "x-api-key", "new-test-secret", 1)
        .unwrap();
    assert_eq!(second.revision, 2);
    assert!(w.disable_credential(&first.reference, 1).is_err());
    let disabled = w.disable_credential(&first.reference, 2).unwrap();
    assert_eq!(disabled.revision, 3);
    assert!(disabled.disabled);
    assert_eq!(
        w.disable_credential(&first.reference, 3).unwrap().revision,
        3
    );
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open(&database, None).unwrap();
    let page = w.credential_page(&[], &[]).unwrap();
    assert_eq!(page.entries[0].revision, 3);
    assert!(page.entries[0].disabled);
    assert!(!w.plugin_status().unwrap().enabled);
    let renewed = w
        .save_credential(
            &first.reference,
            3,
            "authorization",
            "replacement-after-disable",
            30,
        )
        .unwrap();
    assert_eq!(renewed.revision, 4);
    assert!(!renewed.disabled);
}

#[test]
fn invalid_inputs_and_stale_revisions_do_not_overwrite_credentials_or_echo_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open(&dir.path().join("workbench.db"), None).unwrap();
    for (reference, revision, name, secret, days) in [
        (vec![], 0, "authorization", "sentinel", 0),
        (vec![], 0, "authorization", "sentinel", 31),
        (vec![], 1, "authorization", "sentinel", 7),
        (vec![1; 32], 0, "authorization", "sentinel", 7),
        (vec![1; 31], 0, "authorization", "sentinel", 7),
        (vec![], 0, "host", "sentinel", 7),
        (vec![], 0, "x-api-key", "sentinel\r\nInjected: yes", 7),
    ] {
        let error = w
            .save_credential(&reference, revision, name, secret, days)
            .err()
            .unwrap();
        assert!(!error.to_string().contains("sentinel"));
    }
    assert!(w.credential_page(&[], &[]).unwrap().entries.is_empty());
    let original = w
        .save_credential(&[], 0, "x-api-key", "original", 7)
        .unwrap();
    let snapshot = w.credential_page(&[], &[]).unwrap().snapshot;
    assert!(
        w.save_credential(&original.reference, 0, "x-api-key", "bad", 7)
            .is_err()
    );
    assert!(w.disable_credential(&original.reference, 0).is_err());
    assert_eq!(w.credential_page(&[], &[]).unwrap().snapshot, snapshot);
}

#[test]
fn bounded_metadata_pages_reject_drift_and_never_decrypt() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open(&dir.path().join("workbench.db"), None).unwrap();
    for _ in 0..19 {
        w.save_credential(&[], 0, "x-api-key", "temporary-test-value", 7)
            .unwrap();
    }
    let first = w.credential_page(&[], &[]).unwrap();
    assert_eq!(first.entries.len(), 16);
    let next = first.next.unwrap();
    let second = w.credential_page(&next, &first.snapshot).unwrap();
    assert_eq!(second.entries.len(), 3);
    assert!(second.next.is_none());
    assert!(second.entries[0].reference > first.entries[15].reference);
    assert!(w.credential_page(&next, &[]).is_err());
    w.disable_credential(&first.entries[0].reference, 1)
        .unwrap();
    assert!(w.credential_page(&next, &first.snapshot).is_err());
    // Disabled ciphertext is bound to its old metadata and cannot be opened;
    // listing still succeeds because it does not call the credential provider.
    assert_eq!(w.credential_page(&[], &[]).unwrap().entries.len(), 16);
}

#[test]
fn private_wire_returns_only_metadata_and_rejects_trailing_frames() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut w = Workbench::open(&db, None).unwrap();
    let secret = "credential-wire-secret-never-echo";
    let input = request(wire::Action::CredentialSave, |mut r| {
        r.set_credential_header("authorization");
        r.set_credential_secret(secret);
        r.set_credential_days(7);
    });
    let output = protocol::respond(&mut w, &input).unwrap();
    assert!(
        !output
            .windows(secret.len())
            .any(|part| part == secret.as_bytes())
    );
    let message = serialize::read_message(&mut &output[..], Default::default()).unwrap();
    let response = message.get_root::<wire::response::Reader>().unwrap();
    assert!(response.get_error().unwrap().is_empty());
    let row = response.get_credentials().unwrap().get(0);
    assert_eq!(row.get_reference().unwrap().len(), 32);
    assert_eq!(row.get_revision(), 1);
    let c = rusqlite::Connection::open(&db).unwrap();
    let payload: Vec<u8> = c
        .query_row("SELECT payload FROM outbound_authorities", [], |r| r.get(0))
        .unwrap();
    let record = Record::decode(&payload).unwrap();
    let Some(Kind::Credential(value)) = &record.value().kind else {
        panic!()
    };
    assert!(
        !output
            .windows(value.ciphertext.len())
            .any(|part| part == value.ciphertext)
    );
    let mut bad = input;
    bad.extend_from_slice(&[0; 8]);
    let output = protocol::respond(&mut w, &bad).unwrap();
    let message = serialize::read_message(&mut &output[..], Default::default()).unwrap();
    assert!(
        !message
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_error()
            .unwrap()
            .is_empty()
    );
    assert_eq!(w.credential_page(&[], &[]).unwrap().entries.len(), 1);
}

#[test]
fn borrowed_private_frames_accept_every_byte_alignment_without_secret_copies() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open(&dir.path().join("workbench.db"), None).unwrap();
    let request = request(wire::Action::CredentialPage, |_| {});
    for offset in 0..8 {
        let mut storage = vec![0; offset];
        storage.extend_from_slice(&request);
        let response = protocol::respond(&mut w, &storage[offset..]).unwrap();
        let message = serialize::read_message(&mut &response[..], Default::default()).unwrap();
        let root = message.get_root::<wire::response::Reader>().unwrap();
        assert!(root.get_error().unwrap().is_empty(), "alignment {offset}");
        assert_eq!(root.get_credentials().unwrap().len(), 0);
        assert_eq!(root.get_credential_snapshot().unwrap().len(), 32);
    }
}

#[test]
fn endpoint_references_cannot_be_replaced_or_disabled_as_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let endpoint = endpoint([1; 32]);
    {
        let mut store = morrow_core::store::Store::open(&db, Default::default()).unwrap();
        store.save_outbound_authority_local(&endpoint, 0).unwrap();
    }
    let mut w = Workbench::open(&db, None).unwrap();
    let before = w.credential_page(&[], &[]).unwrap();
    assert!(before.entries.is_empty());
    assert!(before.next.is_none());
    for action in [
        wire::Action::CredentialSave,
        wire::Action::CredentialDisable,
    ] {
        // A matching revision must still fail the type check, rather than
        // treating an endpoint's reference as a credential to overwrite.
        let input = request(action, |mut r| {
            r.set_credential_reference(&endpoint.reference());
            r.set_revision(1);
            r.set_credential_header("x-api-key");
            r.set_credential_secret("wrong-kind-secret-sentinel");
            r.set_credential_days(7);
        });
        let output = protocol::respond(&mut w, &input).unwrap();
        let message = serialize::read_message(&mut &output[..], Default::default()).unwrap();
        let response = message.get_root::<wire::response::Reader>().unwrap();
        let error = response.get_error().unwrap().to_str().unwrap();
        assert!(error.contains("different record type"));
        assert!(!error.contains("wrong-kind-secret-sentinel"));
        assert_eq!(response.get_credentials().unwrap().len(), 0);
        assert_eq!(
            w.credential_page(&[], &[]).unwrap().snapshot,
            before.snapshot
        );
    }
    w.finish().unwrap();
    drop(w);
    let connection =
        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let payload: Vec<u8> = connection
        .query_row(
            "SELECT payload FROM outbound_authorities WHERE reference=?1",
            [endpoint.reference().as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(payload, endpoint.container());
}

#[test]
fn filtered_empty_page_preserves_cursor_to_later_credentials_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let credential =
        morrow_audit::credentials::seal([17; 32], 1, 10, 1010, "x-api-key", "filtered-page-secret")
            .unwrap();
    {
        let mut store = morrow_core::store::Store::open(&db, Default::default()).unwrap();
        for byte in 1..=16 {
            store
                .save_outbound_authority_local(&endpoint([byte; 32]), 0)
                .unwrap();
        }
        store.save_outbound_authority_local(&credential, 0).unwrap();
    }
    let mut w = Workbench::open(&db, None).unwrap();
    let input = request(wire::Action::CredentialPage, |_| {});
    let output = protocol::respond(&mut w, &input).unwrap();
    let message = serialize::read_message(&mut &output[..], Default::default()).unwrap();
    let first = message.get_root::<wire::response::Reader>().unwrap();
    assert!(first.get_error().unwrap().is_empty());
    assert_eq!(first.get_credentials().unwrap().len(), 0);
    let cursor = first.get_credential_cursor().unwrap().to_vec();
    let snapshot = first.get_credential_snapshot().unwrap().to_vec();
    assert_eq!(cursor, [16; 32]);
    assert_eq!(snapshot.len(), 32);
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open(&db, None).unwrap();
    let input = request(wire::Action::CredentialPage, |mut r| {
        r.set_credential_cursor(&cursor);
        r.set_credential_snapshot(&snapshot);
    });
    let output = protocol::respond(&mut w, &input).unwrap();
    let message = serialize::read_message(&mut &output[..], Default::default()).unwrap();
    let second = message.get_root::<wire::response::Reader>().unwrap();
    assert!(second.get_error().unwrap().is_empty());
    let entries = second.get_credentials().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries.get(0).get_reference().unwrap(), [17; 32]);
    assert_eq!(entries.get(0).get_revision(), 1);
    assert!(second.get_credential_cursor().unwrap().is_empty());
    assert_eq!(second.get_credential_snapshot().unwrap(), snapshot);
    assert!(
        !output
            .windows(b"filtered-page-secret".len())
            .any(|v| v == b"filtered-page-secret")
    );
    // The credential is expired, but catalog traversal remains a read-only
    // metadata operation and neither refreshes its lifetime nor grants use.
    assert_eq!(entries.get(0).get_created_ms(), 10);
    assert_eq!(entries.get(0).get_expires_ms(), 1010);
    assert_eq!(
        w.credential_page(&[], &[]).unwrap().snapshot.as_slice(),
        snapshot
    );
}
