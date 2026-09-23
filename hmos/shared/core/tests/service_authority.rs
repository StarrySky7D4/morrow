//! Persisted approval data qualification; no restored runtime privilege.
use morrow_core::{
    Error,
    service_authority::{self, Record, proto},
};
use prost::Message;
use sha2::{Digest, Sha256};
fn auth() -> proto::Record {
    proto::Record {
        schema_version: 1,
        reference: vec![1; 32],
        revision: 1,
        created_ms: 10,
        expires_ms: 1010,
        disabled: false,
        kind: Some(proto::record::Kind::Authentication(proto::Authentication {
            principal_id: "alice".into(),
            token_sha256: Sha256::digest(b"synthetic-bearer-value").to_vec(),
        })),
    }
}
fn publication() -> proto::Record {
    let mut value = auth();
    value.reference = vec![2; 32];
    value.kind = Some(proto::record::Kind::Publication(proto::Publication {
        config_id: "node-api".into(),
        config_sha256: vec![3; 32],
        listen_address: "127.0.0.1:8080".into(),
        tls_required: false,
        method: "POST".into(),
        path: "/api/v1".into(),
        query_path: "/history".into(),
    }));
    value
}
fn pack(raw: &[u8]) -> Vec<u8> {
    let packed = lz4_flex::block::compress(raw);
    let mut out = service_authority::MAGIC.to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend_from_slice(&packed);
    out
}
#[test]
fn roundtrip_checks_half_open_time_and_never_retains_raw_token() {
    for value in [auth(), publication()] {
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
        let raw = decoded.value().encode_to_vec();
        assert!(
            !raw.windows(b"synthetic-bearer-value".len())
                .any(|s| s == b"synthetic-bearer-value")
        );
    }
}
#[test]
fn references_revision_lifetime_principal_and_missing_kind_rejected() {
    for case in 0..12 {
        let mut value = auth();
        match case {
            0 => value.reference = vec![0; 32],
            1 => value.reference = vec![1; 31],
            2 => value.revision = 0,
            3 => value.revision = u64::MAX,
            4 => value.created_ms = 0,
            5 => value.expires_ms = 10,
            6 => value.expires_ms = 9,
            7 => value.expires_ms = value.created_ms + service_authority::MAX_LIFETIME_MS + 1,
            8 => value.kind = None,
            _ => {
                if let Some(proto::record::Kind::Authentication(a)) = &mut value.kind {
                    match case {
                        9 => a.principal_id = "a/b".into(),
                        10 => a.principal_id = "x".repeat(129),
                        _ => a.token_sha256 = vec![0; 32],
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
fn publication_requires_canonical_address_tls_and_fixed_uri_paths() {
    for case in 0..15 {
        let mut value = publication();
        if let Some(proto::record::Kind::Publication(p)) = &mut value.kind {
            match case {
                0 => p.listen_address = "0.0.0.0:443".into(),
                1 => p.listen_address = "[0:0:0:0:0:0:0:1]:8080".into(),
                2 => p.listen_address = "localhost:8080".into(),
                3 => p.method = "post".into(),
                4 => p.method = "X".repeat(33),
                5 => p.path = "/api?x=1".into(),
                6 => p.path = "/api#fragment".into(),
                7 => p.path = "//remote/api".into(),
                8 => p.path = "/api/{wildcard}".into(),
                9 => p.path = "/api space".into(),
                10 => p.path = "/bad%2".into(),
                11 => p.query_path = p.path.clone(),
                12 => p.config_id = "bad/id".into(),
                13 => p.config_sha256 = vec![0; 32],
                _ => p.path = "/".repeat(8193),
            }
        }
        assert!(Record::encode(value.clone()).is_err(), "{case}");
        assert!(
            Record::decode(&pack(&value.encode_to_vec())).is_err(),
            "{case}"
        );
    }
    for address in ["0.0.0.0:443", "[::]:443", "[::1]:443"] {
        let mut value = publication();
        if let Some(proto::record::Kind::Publication(p)) = &mut value.kind {
            p.listen_address = address.into();
            p.tls_required = true;
            p.path = "/api/%E4%B8%AD".into();
        }
        Record::encode(value).unwrap();
    }
}
#[test]
fn unknown_duplicate_oneof_wire_overflow_and_noncanonical_fields_fail_closed() {
    let valid = auth().encode_to_vec();
    let cases = [
        vec![0x48, 1],
        vec![0x18, 1],
        vec![0x30, 2],
        vec![0x30, 0],
        vec![0x3a, 0],
        vec![0x42, 0],
        vec![0x08, 0xff, 0xff, 0xff, 0xff, 0x1f],
    ];
    for suffix in cases {
        let mut raw = valid.clone();
        raw.extend(suffix);
        assert!(Record::decode(&pack(&raw)).is_err());
    }
    let mut nested = auth();
    if let Some(proto::record::Kind::Authentication(a)) = &mut nested.kind {
        a.token_sha256 = vec![1; 33];
    }
    assert!(Record::decode(&pack(&nested.encode_to_vec())).is_err());
}
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use morrow_core::{
        content::CardRecord,
        service_config::{Config, proto as config_proto},
        store::{EventBudget, SCHEMA_VERSION, Store},
    };
    fn setup() -> (tempfile::TempDir, std::path::PathBuf, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority.db");
        let store = Store::open(&path, Default::default()).unwrap();
        (dir, path, store)
    }
    fn config() -> Config {
        Config::encode(config_proto::Configuration {
            schema_version: 1,
            id: "node-api".into(),
            revision: 1,
            namespace: vec![4; 32],
            retention_ms: 1000,
            service: "service.example".into(),
            handler: "api".into(),
            package_sha256: vec![5; 32],
            disabled: false,
            principals: vec![],
            approval_references: vec![vec![2; 32]],
        })
        .unwrap()
    }
    #[test]
    fn cas_reopen_digest_binding_and_disable_survive_without_events() {
        let (_dir, path, mut store) = setup();
        let config = config();
        store.save_service_config_local(&config, 0).unwrap();
        assert_eq!(
            config.digest(),
            <[u8; 32]>::from(Sha256::digest(config.container()))
        );
        let mut v = publication();
        if let Some(proto::record::Kind::Publication(p)) = &mut v.kind {
            p.config_sha256 = config.digest().to_vec();
        }
        let initial = Record::encode(v).unwrap();
        store.save_service_authority_local(&initial, 0).unwrap();
        let mut competing = Store::open_existing(&path, Default::default()).unwrap();
        let mut v = initial.value().clone();
        v.revision = 2;
        v.disabled = true;
        let next = Record::encode(v).unwrap();
        store.save_service_authority_local(&next, 1).unwrap();
        assert_eq!(
            competing.save_service_authority_local(&next, 1),
            Err(Error::RevisionConflict)
        );
        assert_eq!(store.pending_usage().unwrap(), (0, 0));
        drop(competing);
        drop(store);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        let loaded = store.load_service_authority(&[2; 32]).unwrap().unwrap();
        assert_eq!(loaded.value(), next.value());
        assert!(loaded.check_time(10).is_err());
        store.integrity_check().unwrap();
    }
    #[test]
    fn immutable_reference_identity_blocks_type_and_subject_rebinding() {
        let (_dir, _path, mut store) = setup();
        store
            .save_service_authority_local(&Record::encode(auth()).unwrap(), 0)
            .unwrap();
        let mut changed = auth();
        changed.revision = 2;
        if let Some(proto::record::Kind::Authentication(a)) = &mut changed.kind {
            a.principal_id = "bob".into();
        }
        assert_eq!(
            store.save_service_authority_local(&Record::encode(changed).unwrap(), 1),
            Err(Error::OperationConflict)
        );
        let mut changed = publication();
        changed.reference = vec![1; 32];
        changed.revision = 2;
        assert_eq!(
            store.save_service_authority_local(&Record::encode(changed).unwrap(), 1),
            Err(Error::OperationConflict)
        );
        assert_eq!(
            store
                .load_service_authority(&[1; 32])
                .unwrap()
                .unwrap()
                .value()
                .revision,
            1
        );
    }
    #[test]
    fn shared_quota_blocks_events_config_and_authority_with_atomic_rollback() {
        let (_dir, path, store) = setup();
        drop(store);
        let initial = Record::encode(auth()).unwrap();
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: initial.container().len() as u64,
            },
        )
        .unwrap();
        store.save_service_authority_local(&initial, 0).unwrap();
        let config_lease = store.pin_service_authority().unwrap();
        assert_eq!(
            store.save_service_config_local(&config(), 0),
            Err(Error::EventCapacity)
        );
        assert!(config_lease.check().is_err());
        assert_eq!(
            store
                .create_local(
                    "create",
                    &CardRecord::new("one", "test.card", 1, "", b"body".to_vec()).unwrap()
                )
                .err(),
            Some(Error::EventCapacity)
        );
        assert_eq!(
            store.save_service_authority_local(&Record::encode(publication()).unwrap(), 0),
            Err(Error::EventCapacity)
        );
        let mut expanded = auth();
        expanded.revision = 2;
        expanded.expires_ms = 1_012_345;
        let expanded = Record::encode(expanded).unwrap();
        assert!(expanded.container().len() > initial.container().len());
        let authority_lease = store.pin_service_authority().unwrap();
        assert_eq!(
            store.save_service_authority_local(&expanded, 1),
            Err(Error::EventCapacity)
        );
        assert!(authority_lease.check().is_err());
        assert_eq!(
            store
                .load_service_authority(&[1; 32])
                .unwrap()
                .unwrap()
                .container(),
            initial.container()
        );
        store.integrity_check().unwrap();
    }
    #[test]
    fn existing_config_consumes_same_authority_budget_and_replacement_counts_once() {
        let (_dir, path, mut store) = setup();
        let config = config();
        store.save_service_config_local(&config, 0).unwrap();
        drop(store);
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: config.container().len() as u64,
            },
        )
        .unwrap();
        assert_eq!(
            store.save_service_authority_local(&Record::encode(auth()).unwrap(), 0),
            Err(Error::EventCapacity)
        );
        drop(store);
        let initial = Record::encode(auth()).unwrap();
        let mut v = auth();
        v.revision = 2;
        v.disabled = true;
        let next = Record::encode(v).unwrap();
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: (config.container().len()
                    + initial.container().len().max(next.container().len()))
                    as u64,
            },
        )
        .unwrap();
        store.save_service_authority_local(&initial, 0).unwrap();
        store.save_service_authority_local(&next, 1).unwrap();
        assert_eq!(
            store
                .load_service_authority(&[1; 32])
                .unwrap()
                .unwrap()
                .value()
                .revision,
            2
        );
    }
    #[test]
    fn v18_migration_preserves_configs_and_events_and_rejects_false_downgrade() {
        let (_dir, path, mut store) = setup();
        let config = config();
        store.save_service_config_local(&config, 0).unwrap();
        store
            .create_local(
                "create",
                &CardRecord::new("one", "test.card", 1, "", b"old".to_vec()).unwrap(),
            )
            .unwrap();
        let events = store.pending(0, 128).unwrap();
        drop(store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("PRAGMA user_version=18;").unwrap();
        drop(sql);
        assert!(matches!(
            Store::open_existing(&path, Default::default()),
            Err(Error::Integrity)
        ));
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch(
            "DROP TABLE IF EXISTS tls_identities; DROP TABLE IF EXISTS outbound_authorities; DROP TABLE IF EXISTS service_authority_identity; DROP TABLE service_authorities;",
        )
        .unwrap();
        drop(sql);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(
            store
                .load_service_config("node-api")
                .unwrap()
                .unwrap()
                .container(),
            config.container()
        );
        assert_eq!(store.pending(0, 128).unwrap(), events);
        assert!(store.load_service_authority(&[1; 32]).unwrap().is_none());
        let sql = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
    }
    #[test]
    fn store_pin_blocks_other_writers_and_mutations_invalidate_old_leases() {
        let (_dir, path, mut store) = setup();
        let initial = Record::encode(auth()).unwrap();
        store.save_service_authority_local(&initial, 0).unwrap();
        let lease = store.pin_service_authority().unwrap();
        store.validate_service_authority(&lease).unwrap();
        let mut other = Store::open_existing(&path, Default::default()).unwrap();
        assert!(other.validate_service_authority(&lease).is_err());
        let mut v = auth();
        v.revision = 2;
        v.disabled = true;
        let next = Record::encode(v).unwrap();
        assert_eq!(
            other.save_service_authority_local(&next, 1),
            Err(Error::StorageBusy)
        );
        assert_eq!(
            store.save_service_authority_local(&next, 0),
            Err(Error::RevisionConflict)
        );
        lease.check().unwrap();
        store.save_service_authority_local(&next, 1).unwrap();
        assert!(lease.check().is_err());
        let fresh = store.pin_service_authority().unwrap();
        store.save_service_config_local(&config(), 0).unwrap();
        assert!(fresh.check().is_err());
        let last = store.pin_service_authority().unwrap();
        drop(store);
        assert!(last.check().is_err());
        other
            .save_service_authority_local(
                &Record::encode({
                    let mut v = auth();
                    v.revision = 3;
                    v
                })
                .unwrap(),
                2,
            )
            .unwrap();
    }
    #[test]
    fn exclusive_profile_cannot_bypass_a_pin_even_through_a_database_copy() {
        let (dir, path, mut store) = setup();
        let initial = Record::encode(auth()).unwrap();
        store.save_service_authority_local(&initial, 0).unwrap();
        let lease = store.pin_service_authority().unwrap();
        let mut value = auth();
        value.revision = 2;
        value.disabled = true;
        let update = Record::encode(value).unwrap();
        match Store::open_exclusive(&path, Default::default(), false) {
            Ok(mut other) => assert_eq!(
                other.save_service_authority_local(&update, 1),
                Err(Error::StorageBusy)
            ),
            Err(error) => assert_eq!(error, Error::StorageBusy),
        }
        // A separate snapshot has no SQLite lock contention, but deliberately
        // retains the original authority identity and must share its native lock.
        let copy = dir.path().join("exclusive-copy.db");
        store.snapshot_to(&copy, 32 * 1024 * 1024).unwrap();
        let mut exclusive = Store::open_exclusive(&copy, Default::default(), false).unwrap();
        assert_eq!(
            exclusive.save_service_authority_local(&update, 1),
            Err(Error::StorageBusy)
        );
        assert_eq!(
            exclusive.save_service_config_local(&config(), 0),
            Err(Error::StorageBusy)
        );
        assert!(exclusive.pin_service_authority().is_err());
        lease.check().unwrap();
        drop(store);
        let exclusive_lease = exclusive.pin_service_authority().unwrap();
        exclusive.save_service_authority_local(&update, 1).unwrap();
        assert!(exclusive_lease.check().is_err());
    }
    #[test]
    fn bounded_authority_count_and_identity_metadata_corruption_fail_closed() {
        let (_dir, path, mut store) = setup();
        for index in 0..service_authority::MAX_RECORDS {
            let mut value = auth();
            value.reference[..8].copy_from_slice(&(index as u64 + 1).to_le_bytes());
            store
                .save_service_authority_local(&Record::encode(value).unwrap(), 0)
                .unwrap();
        }
        let mut extra = auth();
        extra.reference = vec![99; 32];
        assert_eq!(
            store.save_service_authority_local(&Record::encode(extra).unwrap(), 0),
            Err(Error::Limit)
        );
        drop(store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("UPDATE service_authority_identity SET payload=X'00';")
            .unwrap();
        drop(sql);
        assert!(Store::open_existing(&path, Default::default()).is_err());
    }
    #[test]
    fn corrupt_metadata_payload_or_schema_cannot_be_loaded() {
        for case in 0..5 {
            let (_dir, path, mut store) = setup();
            store
                .save_service_authority_local(&Record::encode(auth()).unwrap(), 0)
                .unwrap();
            drop(store);
            let sql = rusqlite::Connection::open(&path).unwrap();
            sql.execute_batch(match case {
                0 => "UPDATE service_authorities SET revision=2;",
                1 => "UPDATE service_authorities SET kind=2;",
                2 => "UPDATE service_authorities SET subject='bob';",
                3 => "UPDATE service_authorities SET payload=X'00';",
                _ => "DROP TABLE IF EXISTS tls_identities; DROP TABLE IF EXISTS outbound_authorities; DROP TABLE IF EXISTS service_authority_identity; DROP TABLE service_authorities;",
            })
            .unwrap();
            drop(sql);
            assert!(
                Store::open_existing(&path, Default::default()).is_err(),
                "{case}"
            );
        }
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
    #[ignore = "subprocess fault-injection entry; exercised by parent tests"]
    fn service_authority_crash_child() {
        let path = std::env::var_os("MORROW_AUTHORITY_CRASH_DB").expect("child database");
        let mut store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
        if std::env::var("MORROW_AUTHORITY_CRASH_MODE").as_deref() == Ok("replace") {
            let mut v = auth();
            v.revision = 2;
            v.disabled = true;
            store
                .save_service_authority_local(&Record::encode(v).unwrap(), 1)
                .unwrap();
        }
        panic!("expected boundary was not reached");
    }
    fn run(path: &Path, mode: &str, point: &str) {
        let mut process = Command::new(std::env::current_exe().unwrap());
        process
            .args([
                "--exact",
                "crash::service_authority_crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_AUTHORITY_CRASH_DB", path)
            .env("MORROW_AUTHORITY_CRASH_MODE", mode)
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
    fn replacement_crashes_leave_whole_previous_or_new_record() {
        for (point, revision) in [
            ("service-authority-before-commit", 1),
            ("service-authority-after-commit", 2),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("replace.db");
            let mut store = Store::open(&path, Default::default()).unwrap();
            store
                .save_service_authority_local(&Record::encode(auth()).unwrap(), 0)
                .unwrap();
            drop(store);
            run(&path, "replace", point);
            let store = Store::open_existing(&path, Default::default()).unwrap();
            let record = store.load_service_authority(&[1; 32]).unwrap().unwrap();
            assert_eq!(record.value().revision, revision);
            assert_eq!(record.value().disabled, revision == 2);
            store.integrity_check().unwrap();
            assert_eq!(store.pending_usage().unwrap(), (0, 0));
        }
    }
    #[test]
    fn migration_crashes_preserve_old_or_complete_new_schema_and_identity() {
        for (point, version) in [
            ("service-authority-migration-before-commit", 18),
            ("service-authority-migration-after-commit", 19),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("migration.db");
            drop(Store::open(&path, Default::default()).unwrap());
            let sql = rusqlite::Connection::open(&path).unwrap();
            sql.execute_batch("DROP TABLE IF EXISTS tls_identities; DROP TABLE IF EXISTS outbound_authorities; DROP TABLE service_authority_identity; DROP TABLE service_authorities; PRAGMA user_version=18;").unwrap();
            drop(sql);
            run(&path, "migrate", point);
            let sql = rusqlite::Connection::open(&path).unwrap();
            assert_eq!(
                sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                version
            );
            let tables:i64=sql.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('service_authorities','service_authority_identity')",[],|r|r.get(0)).unwrap();
            assert_eq!(tables, 2 * i64::from(version == 19));
            drop(sql);
            let store = Store::open_existing(&path, Default::default()).unwrap();
            store.integrity_check().unwrap();
            drop(store);
            let sql = rusqlite::Connection::open(&path).unwrap();
            let identity: Vec<u8> = sql
                .query_row("SELECT payload FROM service_authority_identity", [], |r| {
                    r.get(0)
                })
                .unwrap();
            drop(sql);
            drop(Store::open_existing(&path, Default::default()).unwrap());
            let sql = rusqlite::Connection::open(&path).unwrap();
            let reopened: Vec<u8> = sql
                .query_row("SELECT payload FROM service_authority_identity", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(identity, reopened);
        }
    }
}
