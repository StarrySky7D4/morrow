//! Host-owned outbound authority codec and original Store qualification.
use morrow_core::{
    Error, io,
    outbound_authority::{self, Record, proto},
};
use prost::Message;
use sha2::{Digest, Sha256};
fn credential() -> proto::Record {
    proto::Record {
        schema_version: 1,
        reference: vec![1; 32],
        revision: 1,
        created_ms: 10,
        expires_ms: 1010,
        disabled: false,
        kind: Some(proto::record::Kind::Credential(proto::Credential {
            provider: outbound_authority::WINDOWS_DPAPI_PROVIDER.into(),
            ciphertext: vec![42; 64],
        })),
    }
}
fn endpoint() -> proto::Record {
    let mut v = credential();
    v.reference = vec![2; 32];
    v.kind = Some(proto::record::Kind::Endpoint(proto::Endpoint {
        package_id: "test.plugin".into(),
        package_sha256: vec![3; 32],
        origin: "https://api.example".into(),
        profile: 1,
        methods: vec!["GET".into(), "POST".into()],
        credential_reference: vec![1; 32],
        root_certificate: vec![],
        max_request_bytes: 1024,
        max_response_bytes: 2048,
        max_header_bytes: 4096,
        max_concurrent: 4,
        timeout_ms: 1000,
        max_frame_bytes: 8192,
    }));
    v
}
fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut bytes = outbound_authority::MAGIC.to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend(compressed);
    bytes
}
#[test]
fn roundtrip_lifetime_and_ciphertext_only_provider_record() {
    for value in [credential(), endpoint()] {
        let record = Record::encode(value.clone()).unwrap();
        let decoded = Record::decode(record.container()).unwrap();
        assert_eq!(decoded.value(), &value);
        assert_eq!(decoded.reference().as_slice(), value.reference);
        assert_eq!(decoded.container(), record.container());
        assert!(decoded.check_time(9).is_err());
        decoded.check_time(10).unwrap();
        decoded.check_time(1009).unwrap();
        assert!(decoded.check_time(1010).is_err());
        let mut disabled = value;
        disabled.disabled = true;
        assert!(Record::encode(disabled).unwrap().check_time(10).is_err());
    }
}
#[test]
fn strict_record_and_credential_bounds_reject_before_decode_allocation() {
    for case in 0..12 {
        let mut value = credential();
        match case {
            0 => value.schema_version = 2,
            1 => value.reference = vec![0; 32],
            2 => value.revision = 0,
            3 => value.revision = u64::MAX,
            4 => value.created_ms = 0,
            5 => value.expires_ms = 10,
            6 => value.expires_ms = 9,
            7 => value.expires_ms = value.created_ms + outbound_authority::MAX_LIFETIME_MS + 1,
            8 => value.kind = None,
            _ => {
                if let Some(proto::record::Kind::Credential(c)) = &mut value.kind {
                    match case {
                        9 => c.provider = "plaintext".into(),
                        10 => c.ciphertext.clear(),
                        _ => c.ciphertext = vec![1; outbound_authority::MAX_CIPHERTEXT_BYTES + 1],
                    }
                }
            }
        }
        assert!(Record::encode(value.clone()).is_err(), "{case}");
        assert!(
            Record::decode(&pack(&value.encode_to_vec())).is_err(),
            "{case}"
        );
    }
}
#[test]
fn endpoint_limits_methods_and_references_are_strict() {
    for case in 0..24 {
        let mut value = endpoint();
        if let Some(proto::record::Kind::Endpoint(e)) = &mut value.kind {
            match case {
                0 => e.package_id = "bad/id".into(),
                1 => e.package_sha256 = vec![0; 32],
                2 => e.profile = 0,
                3 => e.profile = 4,
                4 => e.methods.clear(),
                5 => e.methods = vec!["GET".into(); 17],
                6 => e.methods = vec!["GET".into(), "GET".into()],
                7 => e.methods = vec!["POST".into(), "GET".into()],
                8 => e.methods = vec!["get".into()],
                9 => e.methods = vec!["X".repeat(33)],
                10 => e.credential_reference = vec![0; 32],
                11 => e.credential_reference = vec![1; 31],
                12 => e.root_certificate = vec![1; outbound_authority::MAX_CERTIFICATE_BYTES + 1],
                13 => e.max_request_bytes = io::MAX_PAYLOAD_BYTES as u64 + 1,
                14 => e.max_response_bytes = io::MAX_PAYLOAD_BYTES as u64 + 1,
                15 => e.max_header_bytes = io::MAX_HEADER_BYTES as u64 + 1,
                16 => e.max_concurrent = 129,
                17 => e.max_concurrent = 0,
                18 => e.timeout_ms = io::MAX_SUBMIT_DEADLINE_MS + 1,
                19 => e.timeout_ms = 0,
                20 => e.max_frame_bytes = io::MAX_FRAME_BYTES as u64 + 1,
                21 => e.max_frame_bytes = 0,
                22 => e.max_request_bytes = 0,
                _ => e.max_header_bytes = 0,
            }
        }
        assert!(Record::encode(value.clone()).is_err(), "{case}");
        assert!(
            Record::decode(&pack(&value.encode_to_vec())).is_err(),
            "{case}"
        );
    }
}
#[test]
fn lexical_origin_rejects_credentials_paths_queries_and_noncanonical_ports() {
    for origin in [
        "HTTPS://api.example",
        "https://api.example/",
        "https://api.example?q=1",
        "https://api.example#x",
        "https://user:secret@api.example",
        "https://api.example:443",
        "https://api.example:0443",
        "https://api.example:0",
        "https://api.example:65536",
        "https://API.example",
        "https://api..example",
        "https://[0:0:0:0:0:0:0:1]",
        "https://api.example\\bad",
        "http://api.example",
    ] {
        let mut value = endpoint();
        if let Some(proto::record::Kind::Endpoint(e)) = &mut value.kind {
            e.origin = origin.into();
        }
        assert!(Record::encode(value.clone()).is_err(), "{origin}");
        assert!(
            Record::decode(&pack(&value.encode_to_vec())).is_err(),
            "{origin}"
        );
    }
    for (origin, profile) in [
        ("https://api.example", 1),
        ("https://127.0.0.1:8443", 3),
        ("http://127.0.0.1:8080", 2),
        ("https://[::1]:8443", 3),
    ] {
        let mut value = endpoint();
        if let Some(proto::record::Kind::Endpoint(e)) = &mut value.kind {
            e.origin = origin.into();
            e.profile = profile;
        }
        Record::encode(value).unwrap();
    }
}
#[test]
fn duplicate_oneof_unknown_and_noncanonical_wire_fields_fail_closed() {
    for suffix in [
        vec![0x48, 1],
        vec![0x18, 1],
        vec![0x30, 0],
        vec![0x30, 2],
        vec![0x3a, 0],
        vec![0x42, 0],
        vec![0x08, 0xff, 0xff, 0xff, 0xff, 0x1f],
    ] {
        let mut raw = credential().encode_to_vec();
        raw.extend(suffix);
        assert!(Record::decode(&pack(&raw)).is_err());
    }
}
#[test]
fn endpoint_and_ciphertext_hard_maximums_roundtrip() {
    let mut value = credential();
    if let Some(proto::record::Kind::Credential(c)) = &mut value.kind {
        c.ciphertext = vec![42; outbound_authority::MAX_CIPHERTEXT_BYTES];
    }
    let record = Record::encode(value).unwrap();
    Record::decode(record.container()).unwrap();
    let mut value = endpoint();
    if let Some(proto::record::Kind::Endpoint(e)) = &mut value.kind {
        e.root_certificate = vec![42; outbound_authority::MAX_CERTIFICATE_BYTES];
        e.max_request_bytes = io::MAX_PAYLOAD_BYTES as u64;
        e.max_response_bytes = io::MAX_PAYLOAD_BYTES as u64;
        e.max_header_bytes = io::MAX_HEADER_BYTES as u64;
        e.max_frame_bytes = io::MAX_FRAME_BYTES as u64;
        e.max_concurrent = 128;
        e.timeout_ms = io::MAX_SUBMIT_DEADLINE_MS;
    }
    let record = Record::encode(value).unwrap();
    Record::decode(record.container()).unwrap();
}
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use morrow_core::{
        content::CardRecord,
        service_authority::{Record as Inbound, proto as inbound},
        store::{EventBudget, SCHEMA_VERSION, Store},
    };
    fn setup() -> (tempfile::TempDir, std::path::PathBuf, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("outbound.db");
        let store = Store::open(&path, Default::default()).unwrap();
        (dir, path, store)
    }
    fn inbound() -> Inbound {
        Inbound::encode(inbound::Record {
            schema_version: 1,
            reference: vec![7; 32],
            revision: 1,
            created_ms: 10,
            expires_ms: 1010,
            disabled: false,
            kind: Some(inbound::record::Kind::Authentication(
                inbound::Authentication {
                    principal_id: "alice".into(),
                    token_sha256: vec![8; 32],
                },
            )),
        })
        .unwrap()
    }
    #[test]
    fn cas_reopen_and_immutable_kind_package_identity() {
        let (_dir, path, mut store) = setup();
        let first = Record::encode(endpoint()).unwrap();
        store.save_outbound_authority_local(&first, 0).unwrap();
        assert_eq!(
            store.save_outbound_authority_local(&first, 0),
            Err(Error::RevisionConflict)
        );
        let mut bad = endpoint();
        bad.revision = 2;
        if let Some(proto::record::Kind::Endpoint(e)) = &mut bad.kind {
            e.package_id = "other.plugin".into();
        }
        assert_eq!(
            store.save_outbound_authority_local(&Record::encode(bad).unwrap(), 1),
            Err(Error::OperationConflict)
        );
        let mut bad = credential();
        bad.reference = vec![2; 32];
        bad.revision = 2;
        assert_eq!(
            store.save_outbound_authority_local(&Record::encode(bad).unwrap(), 1),
            Err(Error::OperationConflict)
        );
        let mut value = endpoint();
        value.revision = 2;
        value.disabled = true;
        let next = Record::encode(value).unwrap();
        store.save_outbound_authority_local(&next, 1).unwrap();
        assert_eq!(store.pending_usage().unwrap(), (0, 0));
        drop(store);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(
            store
                .load_outbound_authority(&[2; 32])
                .unwrap()
                .unwrap()
                .value(),
            next.value()
        );
        store.integrity_check().unwrap();
    }
    #[test]
    fn original_pin_blocks_foreign_outbound_writer_and_all_mutations_revoke() {
        let (_dir, path, mut store) = setup();
        store
            .save_outbound_authority_local(&Record::encode(credential()).unwrap(), 0)
            .unwrap();
        let lease = store.pin_service_authority().unwrap();
        let mut other = Store::open_existing(&path, Default::default()).unwrap();
        let mut value = credential();
        value.revision = 2;
        value.disabled = true;
        let update = Record::encode(value).unwrap();
        assert_eq!(
            other.save_outbound_authority_local(&update, 1),
            Err(Error::StorageBusy)
        );
        lease.check().unwrap();
        store.save_outbound_authority_local(&update, 1).unwrap();
        assert!(lease.check().is_err());
        let new = store.pin_service_authority().unwrap();
        store.save_service_authority_local(&inbound(), 0).unwrap();
        assert!(new.check().is_err());
        drop(store);
        other
            .save_outbound_authority_local(&Record::encode(endpoint()).unwrap(), 0)
            .unwrap();
    }
    #[test]
    fn shared_quota_rollback_and_replacement_count_once() {
        let (_dir, path, store) = setup();
        drop(store);
        let original = Record::encode(credential()).unwrap();
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: original.container().len() as u64,
            },
        )
        .unwrap();
        store.save_outbound_authority_local(&original, 0).unwrap();
        let lease = store.pin_service_authority().unwrap();
        assert_eq!(
            store.save_service_authority_local(&inbound(), 0),
            Err(Error::EventCapacity)
        );
        assert!(lease.check().is_err());
        assert_eq!(
            store
                .create_local(
                    "create",
                    &CardRecord::new("one", "test.card", 1, "", b"body".to_vec()).unwrap()
                )
                .err(),
            Some(Error::EventCapacity)
        );
        let mut value = credential();
        value.revision = 2;
        if let Some(proto::record::Kind::Credential(c)) = &mut value.kind {
            c.ciphertext = (0..256).map(|i| i as u8).collect();
        }
        let expanded = Record::encode(value).unwrap();
        let lease = store.pin_service_authority().unwrap();
        assert_eq!(
            store.save_outbound_authority_local(&expanded, 1),
            Err(Error::EventCapacity)
        );
        assert!(lease.check().is_err());
        assert_eq!(
            store
                .load_outbound_authority(&[1; 32])
                .unwrap()
                .unwrap()
                .container(),
            original.container()
        );
        drop(store);
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: expanded.container().len() as u64,
            },
        )
        .unwrap();
        store.save_outbound_authority_local(&expanded, 1).unwrap();
        store.integrity_check().unwrap();
    }
    #[test]
    fn v19_migration_preserves_inbound_identity_and_event_bytes() {
        let (_dir, path, mut store) = setup();
        let inbound = inbound();
        store.save_service_authority_local(&inbound, 0).unwrap();
        store
            .create_local(
                "create",
                &CardRecord::new("one", "test.card", 1, "", b"old".to_vec()).unwrap(),
            )
            .unwrap();
        let before = store.pending(0, 128).unwrap();
        drop(store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        let identity: Vec<u8> = sql
            .query_row("SELECT payload FROM service_authority_identity", [], |r| {
                r.get(0)
            })
            .unwrap();
        sql.execute_batch("PRAGMA user_version=19;").unwrap();
        drop(sql);
        assert!(matches!(
            Store::open_existing(&path, Default::default()),
            Err(Error::Integrity)
        ));
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("DROP TABLE outbound_authorities;")
            .unwrap();
        drop(sql);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(store.pending(0, 128).unwrap(), before);
        assert_eq!(
            store
                .load_service_authority(&[7; 32])
                .unwrap()
                .unwrap()
                .container(),
            inbound.container()
        );
        assert!(store.load_outbound_authority(&[1; 32]).unwrap().is_none());
        let sql = rusqlite::Connection::open(&path).unwrap();
        let after: Vec<u8> = sql
            .query_row("SELECT payload FROM service_authority_identity", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(identity, after);
        assert_eq!(
            sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
    }
    #[test]
    fn existing_inbound_material_consumes_outbound_byte_capacity() {
        let (_dir, path, mut store) = setup();
        let value = inbound();
        store.save_service_authority_local(&value, 0).unwrap();
        drop(store);
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: value.container().len() as u64,
            },
        )
        .unwrap();
        assert_eq!(
            store.save_outbound_authority_local(&Record::encode(credential()).unwrap(), 0),
            Err(Error::EventCapacity)
        );
        assert!(store.load_outbound_authority(&[1; 32]).unwrap().is_none());
        store.integrity_check().unwrap();
    }
    #[test]
    fn count_limit_and_corrupted_payload_metadata_fail_closed() {
        let (_dir, path, mut store) = setup();
        for index in 0..outbound_authority::MAX_RECORDS {
            let mut value = credential();
            value.reference[..8].copy_from_slice(&(index as u64 + 1).to_le_bytes());
            store
                .save_outbound_authority_local(&Record::encode(value).unwrap(), 0)
                .unwrap();
        }
        let mut extra = credential();
        extra.reference = vec![99; 32];
        assert_eq!(
            store.save_outbound_authority_local(&Record::encode(extra).unwrap(), 0),
            Err(Error::Limit)
        );
        drop(store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("UPDATE outbound_authorities SET subject='other-provider';")
            .unwrap();
        drop(sql);
        assert!(Store::open_existing(&path, Default::default()).is_err());
    }
}

#[cfg(all(feature = "fault-injection", not(target_arch = "wasm32")))]
mod crash {
    use super::*;
    use morrow_core::store::Store;
    use std::{
        path::Path,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    #[test]
    #[ignore = "subprocess entry; exercised by parent fault-injection tests"]
    fn outbound_authority_crash_child() {
        let path = std::env::var_os("MORROW_OUTBOUND_CRASH_DB").expect("child database");
        let mut store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
        if std::env::var("MORROW_OUTBOUND_CRASH_MODE").as_deref() == Ok("replace") {
            let mut value = credential();
            value.revision = 2;
            value.disabled = true;
            store
                .save_outbound_authority_local(&Record::encode(value).unwrap(), 1)
                .unwrap();
        }
        panic!("expected boundary was not reached");
    }
    fn run(path: &Path, mode: &str, point: &str) {
        let mut process = Command::new(std::env::current_exe().unwrap());
        process
            .args([
                "--exact",
                "crash::outbound_authority_crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_OUTBOUND_CRASH_DB", path)
            .env("MORROW_OUTBOUND_CRASH_MODE", mode)
            .env("MORROW_TEST_CRASH_AT", point)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            process.creation_flags(0x08000000);
        }
        let mut child = process.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    assert_eq!(status.code(), Some(86), "{point}");
                    return;
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("observe {point}: {error}");
                }
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("timeout {point}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn replacement_process_exit_preserves_whole_old_or_new_credential() {
        for (point, revision) in [
            ("outbound-authority-before-commit", 1),
            ("outbound-authority-after-commit", 2),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("replacement.db");
            let mut store = Store::open(&path, Default::default()).unwrap();
            store
                .save_outbound_authority_local(&Record::encode(credential()).unwrap(), 0)
                .unwrap();
            drop(store);
            run(&path, "replace", point);
            let store = Store::open_existing(&path, Default::default()).unwrap();
            let record = store.load_outbound_authority(&[1; 32]).unwrap().unwrap();
            assert_eq!(record.value().revision, revision);
            assert_eq!(record.value().disabled, revision == 2);
            assert_eq!(store.pending_usage().unwrap(), (0, 0));
            store.integrity_check().unwrap();
        }
    }
    #[test]
    fn migration_process_exit_preserves_original_authority_identity_and_schema() {
        for (point, version) in [
            ("outbound-authority-migration-before-commit", 19),
            ("outbound-authority-migration-after-commit", 20),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("migration.db");
            drop(Store::open(&path, Default::default()).unwrap());
            let sql = rusqlite::Connection::open(&path).unwrap();
            let original: Vec<u8> = sql
                .query_row("SELECT payload FROM service_authority_identity", [], |r| {
                    r.get(0)
                })
                .unwrap();
            sql.execute_batch("DROP TABLE outbound_authorities; PRAGMA user_version=19;")
                .unwrap();
            drop(sql);
            run(&path, "migrate", point);
            let sql = rusqlite::Connection::open(&path).unwrap();
            assert_eq!(
                sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                version
            );
            let tables: i64 = sql
                .query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name='outbound_authorities'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(tables, i64::from(version == 20));
            let retained: Vec<u8> = sql
                .query_row("SELECT payload FROM service_authority_identity", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(retained, original);
            drop(sql);
            let store = Store::open_existing(&path, Default::default()).unwrap();
            store.integrity_check().unwrap();
            assert!(store.load_outbound_authority(&[1; 32]).unwrap().is_none());
        }
    }
}
