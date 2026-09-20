//! Desired configuration persistence, not authority restoration or node startup.
use morrow_core::{
    Error,
    service_config::{self, Config, proto},
};
use prost::Message;
use sha2::{Digest, Sha256};
fn value() -> proto::Configuration {
    proto::Configuration {
        schema_version: 1,
        id: "node-api".into(),
        revision: 1,
        namespace: vec![7; 32],
        retention_ms: 1000,
        service: "service.example".into(),
        handler: "api.invoke".into(),
        package_sha256: vec![8; 32],
        disabled: false,
        principals: vec![proto::Principal {
            id: "alice".into(),
            authentication_reference: vec![9; 32],
            content_scopes: vec![proto::ContentScope {
                kind: 7,
                card_id: "card-one".into(),
                attachment_id: String::new(),
            }],
        }],
        approval_references: vec![vec![10; 32]],
    }
}
fn pack(raw: &[u8]) -> Vec<u8> {
    let packed = lz4_flex::block::compress(raw);
    let mut out = service_config::MAGIC.to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend_from_slice(&packed);
    out
}
#[test]
fn exact_codec_preserves_policy_and_opaque_references() {
    let config = Config::encode(value()).unwrap();
    let decoded = Config::decode(config.container()).unwrap();
    assert_eq!(decoded.value(), &value());
    assert_eq!(decoded.container(), config.container());
    assert_eq!(decoded.policy().unwrap().namespace, [7; 32]);
    assert_eq!(decoded.policy().unwrap().retention_ms, 1000);
    let schema = include_str!("../schemas/service_config.proto");
    assert!(!schema.contains("string token"));
    assert!(!schema.contains("bytes credential"));
}
#[test]
fn malformed_refs_revisions_scopes_and_namespace_rejected() {
    for case in 0..11 {
        let mut v = value();
        match case {
            0 => v.namespace = vec![0; 32],
            1 => v.revision = 0,
            2 => v.revision = u64::MAX,
            3 => v.retention_ms = 0,
            4 => v.package_sha256 = vec![1; 31],
            5 => v.principals[0].authentication_reference.clear(),
            6 => v.principals[0].content_scopes[0].kind = 8,
            7 => v.principals[0].content_scopes[0].attachment_id = "attachment".into(),
            8 => v.principals[0].content_scopes[0].kind = 4,
            9 => v.approval_references[0] = vec![0; 32],
            _ => v.id = "../outside".into(),
        }
        assert!(Config::encode(v.clone()).is_err(), "case {case}");
        assert!(
            Config::decode(&pack(&v.encode_to_vec())).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn duplicates_unknown_fields_noncanonical_and_nested_counts_rejected() {
    for case in 0..7 {
        let mut v = value();
        let mut raw = match case {
            0 => {
                v.principals.push(v.principals[0].clone());
                v.encode_to_vec()
            }
            1 => {
                v.approval_references.push(v.approval_references[0].clone());
                v.encode_to_vec()
            }
            2 => {
                let scope = v.principals[0].content_scopes[0].clone();
                v.principals[0].content_scopes.push(scope);
                v.encode_to_vec()
            }
            3 => {
                v.principals = vec![v.principals[0].clone(); 65];
                v.encode_to_vec()
            }
            4 => {
                v.principals[0].content_scopes =
                    vec![v.principals[0].content_scopes[0].clone(); 129];
                v.encode_to_vec()
            }
            _ => v.encode_to_vec(),
        };
        if case == 5 {
            raw.extend_from_slice(&[0x60, 1]);
        }
        if case == 6 {
            raw.extend_from_slice(&[0x48, 0]);
        } // explicit default bool
        assert!(Config::decode(&pack(&raw)).is_err(), "case {case}");
    }
}
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use morrow_core::{
        content::CardRecord,
        store::{EventBudget, Store},
    };
    fn setup() -> (tempfile::TempDir, std::path::PathBuf, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("service.db");
        let store = Store::open(&path, Default::default()).unwrap();
        (dir, path, store)
    }
    #[test]
    fn cas_persist_disable_reopen_and_page_with_no_outbox_side_effect() {
        let (_dir, path, mut store) = setup();
        let initial = Config::encode(value()).unwrap();
        store.save_service_config_local(&initial, 0).unwrap();
        assert_eq!(
            store.save_service_config_local(&initial, 0),
            Err(Error::RevisionConflict)
        );
        let mut updated = value();
        updated.revision = 2;
        updated.disabled = true;
        let updated = Config::encode(updated).unwrap();
        let mut competing = Store::open_existing(&path, Default::default()).unwrap();
        store.save_service_config_local(&updated, 1).unwrap();
        assert_eq!(
            competing.save_service_config_local(&updated, 1),
            Err(Error::RevisionConflict)
        );
        assert_eq!(store.pending_usage().unwrap(), (0, 0));
        drop(store);
        drop(competing);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(
            store
                .load_service_config("node-api")
                .unwrap()
                .unwrap()
                .value(),
            updated.value()
        );
        assert_eq!(store.list_service_configs(None, 1).unwrap().len(), 1);
        assert!(
            store
                .list_service_configs(Some("node-api"), 1)
                .unwrap()
                .is_empty()
        );
        assert!(store.list_service_configs(None, 33).is_err());
        store.integrity_check().unwrap();
    }
    #[test]
    fn historical_namespace_and_service_identity_cannot_be_rebound() {
        let (_dir, _path, mut store) = setup();
        store
            .save_service_config_local(&Config::encode(value()).unwrap(), 0)
            .unwrap();
        for case in 0..3 {
            let mut v = value();
            let expected = if case == 2 {
                v.id = "other".into();
                0
            } else {
                v.revision = 2;
                1
            };
            if case == 0 {
                v.namespace = vec![22; 32];
            }
            if case == 1 {
                v.service = "service.other".into();
            }
            assert_eq!(
                store.save_service_config_local(&Config::encode(v).unwrap(), expected),
                Err(Error::OperationConflict)
            );
        }
        assert_eq!(
            store
                .load_service_config("node-api")
                .unwrap()
                .unwrap()
                .value()
                .revision,
            1
        );
    }
    #[test]
    fn shared_quota_blocks_other_events_and_failed_replacement_rolls_back() {
        let (_dir, path, store) = setup();
        drop(store);
        let initial = Config::encode(value()).unwrap();
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: initial.container().len() as u64,
            },
        )
        .unwrap();
        store.save_service_config_local(&initial, 0).unwrap();
        let card = CardRecord::new("one", "test.card", 1, "", b"body".to_vec()).unwrap();
        assert_eq!(
            store.create_local("create", &card).err(),
            Some(Error::EventCapacity)
        );
        let mut v = value();
        v.revision = 2;
        for index in 0..32 {
            v.approval_references.push(vec![20 + index; 32]);
        }
        let bigger = Config::encode(v).unwrap();
        assert_eq!(
            store.save_service_config_local(&bigger, 1),
            Err(Error::EventCapacity)
        );
        assert_eq!(
            store
                .load_service_config("node-api")
                .unwrap()
                .unwrap()
                .container(),
            initial.container()
        );
        assert!(store.card("one").unwrap().is_none());
        store.integrity_check().unwrap();
    }
    #[test]
    fn replacement_is_charged_once_and_outbox_also_limits_config_admission() {
        let (_dir, path, mut store) = setup();
        let initial = Config::encode(value()).unwrap();
        let mut next = value();
        next.revision = 2;
        next.disabled = true;
        let next = Config::encode(next).unwrap();
        store.save_service_config_local(&initial, 0).unwrap();
        drop(store);
        let budget = EventBudget {
            max_count: 1024,
            max_bytes: initial.container().len().max(next.container().len()) as u64,
        };
        let mut store = Store::open_existing(&path, budget).unwrap();
        store.save_service_config_local(&next, 1).unwrap();
        assert_eq!(
            store
                .load_service_config("node-api")
                .unwrap()
                .unwrap()
                .value()
                .revision,
            2
        );
        let (_other, other_path, mut other) = setup();
        other
            .create_local(
                "create",
                &CardRecord::new("one", "test.card", 1, "", b"body".to_vec()).unwrap(),
            )
            .unwrap();
        let used = other.pending_usage().unwrap().1;
        drop(other);
        let mut other = Store::open_existing(
            &other_path,
            EventBudget {
                max_count: 1024,
                max_bytes: used,
            },
        )
        .unwrap();
        assert_eq!(
            other.save_service_config_local(&initial, 0),
            Err(Error::EventCapacity)
        );
        assert!(other.load_service_config("node-api").unwrap().is_none());
    }
    #[test]
    fn migration_preserves_old_event_bytes_and_rejects_false_downgrade() {
        let (_dir, path, mut store) = setup();
        store
            .create_local(
                "create",
                &CardRecord::new("one", "test.card", 1, "", b"old".to_vec()).unwrap(),
            )
            .unwrap();
        let original = store.pending(0, 128).unwrap();
        drop(store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("PRAGMA user_version=17;").unwrap();
        drop(sql);
        assert!(matches!(
            Store::open_existing(&path, Default::default()),
            Err(Error::Integrity)
        ));
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("DROP TABLE service_configs;").unwrap();
        drop(sql);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(store.pending(0, 128).unwrap(), original);
        assert!(store.list_service_configs(None, 32).unwrap().is_empty());
        let sql = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            18
        );
    }
    #[test]
    fn corrupted_index_identity_payload_and_schema_fail_closed() {
        for case in 0..4 {
            let (_dir, path, mut store) = setup();
            store
                .save_service_config_local(&Config::encode(value()).unwrap(), 0)
                .unwrap();
            drop(store);
            let sql = rusqlite::Connection::open(&path).unwrap();
            sql.execute_batch(match case {
                0 => "UPDATE service_configs SET id='swapped';",
                1 => "UPDATE service_configs SET revision=2;",
                2 => "UPDATE service_configs SET payload=X'00';",
                _ => "DROP TABLE service_configs;",
            })
            .unwrap();
            drop(sql);
            assert!(
                Store::open_existing(&path, Default::default()).is_err(),
                "case {case}"
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
    #[ignore = "child process entry; parents exercise the crash boundaries"]
    fn service_config_crash_child() {
        let path = std::env::var_os("MORROW_CONFIG_CRASH_DB").expect("child database");
        let mut store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
        if std::env::var("MORROW_CONFIG_CRASH_MODE").as_deref() == Ok("replace") {
            let mut v = value();
            v.revision = 2;
            v.disabled = true;
            store
                .save_service_config_local(&Config::encode(v).unwrap(), 1)
                .unwrap();
        }
        panic!("expected crash boundary was not reached");
    }
    fn run(path: &Path, mode: &str, point: &str) {
        let mut process = Command::new(std::env::current_exe().unwrap());
        process
            .args([
                "--exact",
                "crash::service_config_crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_CONFIG_CRASH_DB", path)
            .env("MORROW_CONFIG_CRASH_MODE", mode)
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
    fn replacement_crash_recovers_whole_old_or_new_configuration() {
        for (point, revision) in [
            ("service-config-before-commit", 1),
            ("service-config-after-commit", 2),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("replace.db");
            let mut store = Store::open(&path, Default::default()).unwrap();
            store
                .save_service_config_local(&Config::encode(value()).unwrap(), 0)
                .unwrap();
            drop(store);
            run(&path, "replace", point);
            let store = Store::open_existing(&path, Default::default()).unwrap();
            let restored = store.load_service_config("node-api").unwrap().unwrap();
            assert_eq!(restored.value().revision, revision);
            assert_eq!(restored.value().disabled, revision == 2);
            assert_eq!(store.pending_usage().unwrap(), (0, 0));
            store.integrity_check().unwrap();
        }
    }
    #[test]
    fn migration_crash_preserves_whole_old_or_new_schema() {
        for (point, version) in [
            ("service-config-migration-before-commit", 17),
            ("service-config-migration-after-commit", 18),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("migrate.db");
            drop(Store::open(&path, Default::default()).unwrap());
            let sql = rusqlite::Connection::open(&path).unwrap();
            sql.execute_batch("DROP TABLE service_configs; PRAGMA user_version=17;")
                .unwrap();
            drop(sql);
            run(&path, "migrate", point);
            let sql = rusqlite::Connection::open(&path).unwrap();
            assert_eq!(
                sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                version
            );
            let present: i64 = sql
                .query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name='service_configs'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(present, i64::from(version == 18));
            drop(sql);
            let store = Store::open_existing(&path, Default::default()).unwrap();
            assert!(store.list_service_configs(None, 32).unwrap().is_empty());
            store.integrity_check().unwrap();
        }
    }
}
