#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    store::{EventBudget, MAX_TLS_IDENTITIES, ServiceAuthorityResource as Resource, Store},
    tls_identity::{Record, WINDOWS_PROVIDER, proto},
};
fn setup() -> (tempfile::TempDir, std::path::PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let store = Store::open(&path, Default::default()).unwrap();
    (dir, path, store)
}
fn record(store: &Store, id: u8, revision: u64) -> Record {
    Record::encode(proto::Record {
        schema_version: 1,
        store_id: store.tls_store_identity().unwrap().to_vec(),
        reference: vec![id; 32],
        revision,
        disabled: false,
        certificate_sha256: vec![9; 32],
        provider: WINDOWS_PROVIDER.into(),
        ciphertext: vec![7; 64],
    })
    .unwrap()
}
fn scoped(store: &mut Store, resource: Resource) -> morrow_core::store::ServiceAuthorityLease {
    let guard = store.pin_service_authority().unwrap();
    store.narrow_service_authority(&guard, &[resource]).unwrap()
}
#[test]
fn cas_library_binding_tombstone_and_snapshot_survive_reopen() {
    let (dir, path, mut store) = setup();
    let first = record(&store, 1, 1);
    let (_, _, foreign) = setup();
    let foreign = record(&foreign, 2, 1);
    assert_eq!(
        store.save_tls_identity_local(&foreign, 0),
        Err(Error::Integrity)
    );
    store.save_tls_identity_local(&first, 0).unwrap();
    assert_eq!(
        store.save_tls_identity_local(&first, 0),
        Err(Error::RevisionConflict)
    );
    let snapshot = dir.path().join("snapshot.db");
    store.snapshot_to(&snapshot, 16 * 1024 * 1024).unwrap();
    let mut disabled = first.value().clone();
    disabled.revision = 2;
    disabled.disabled = true;
    let disabled = Record::encode(disabled).unwrap();
    store.save_tls_identity_local(&disabled, 1).unwrap();
    assert_eq!(
        store.save_tls_identity_local(&first, 0),
        Err(Error::RevisionConflict)
    );
    store.integrity_check().unwrap();
    drop(store);
    let store = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        store
            .load_tls_identity(&[1; 32])
            .unwrap()
            .unwrap()
            .container(),
        disabled.container()
    );
    let snap = Store::open_existing(&snapshot, Default::default()).unwrap();
    assert_eq!(
        snap.tls_store_identity().unwrap(),
        store.tls_store_identity().unwrap()
    );
    assert_eq!(
        snap.load_tls_identity(&[1; 32])
            .unwrap()
            .unwrap()
            .container(),
        first.container()
    );
    snap.integrity_check().unwrap();
}
#[test]
fn exact_revocation_cas_rejection_and_foreign_writer_keep_namespaces_separate() {
    let (_dir, path, mut store) = setup();
    store
        .save_tls_identity_local(&record(&store, 1, 1), 0)
        .unwrap();
    let target = scoped(&mut store, Resource::TlsIdentity([1; 32]));
    let other = scoped(&mut store, Resource::TlsIdentity([2; 32]));
    let outbound = scoped(&mut store, Resource::Outbound([1; 32]));
    let incoming = scoped(&mut store, Resource::Inbound([1; 32]));
    let update = record(&store, 1, 2);
    let mut foreign = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        foreign.save_tls_identity_local(&update, 1),
        Err(Error::StorageBusy)
    );
    assert_eq!(
        store.save_tls_identity_local(&update, 0),
        Err(Error::RevisionConflict)
    );
    target.check().unwrap();
    store.save_tls_identity_local(&update, 1).unwrap();
    assert!(target.check().is_err());
    other.check().unwrap();
    outbound.check().unwrap();
    incoming.check().unwrap();
    let new = scoped(&mut store, Resource::TlsIdentity([1; 32]));
    new.check().unwrap();
    assert!(target.check().is_err());
}
#[test]
fn quota_failure_rolls_back_data_but_never_revives_revoked_lease() {
    let (_dir, path, store) = setup();
    let first = record(&store, 1, 1);
    drop(store);
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 1024,
            max_bytes: first.container().len() as u64,
        },
    )
    .unwrap();
    store.save_tls_identity_local(&first, 0).unwrap();
    let lease = scoped(&mut store, Resource::TlsIdentity([1; 32]));
    let mut expanded = first.value().clone();
    expanded.revision = 2;
    expanded.ciphertext = (0..4096).map(|i| ((i * 31 + i / 17) % 256) as u8).collect();
    let expanded = Record::encode(expanded).unwrap();
    assert!(expanded.container().len() > first.container().len());
    assert_eq!(
        store.save_tls_identity_local(&expanded, 1),
        Err(Error::EventCapacity)
    );
    assert!(lease.check().is_err());
    assert_eq!(
        store
            .load_tls_identity(&[1; 32])
            .unwrap()
            .unwrap()
            .container(),
        first.container()
    );
    assert_eq!(
        store.save_tls_identity_local(&record(&store, 2, 1), 0),
        Err(Error::EventCapacity)
    );
    store.integrity_check().unwrap();
}
#[test]
fn pagination_snapshots_reject_mutation_and_bad_cursors() {
    let (_dir, _path, mut store) = setup();
    for id in (1..=18).rev() {
        store
            .save_tls_identity_local(&record(&store, id, 1), 0)
            .unwrap();
    }
    let first = store.list_tls_identities_local(None, None, 16).unwrap();
    assert_eq!(first.records.len(), 16);
    assert_eq!(first.next, Some([16; 32]));
    let second = store
        .list_tls_identities_local(first.next, Some(first.snapshot), 16)
        .unwrap();
    assert_eq!(second.records.len(), 2);
    assert_eq!(second.next, None);
    assert!(
        store
            .list_tls_identities_local(Some([19; 32]), Some(first.snapshot), 16)
            .is_err()
    );
    assert!(
        store
            .list_tls_identities_local(first.next, None, 16)
            .is_err()
    );
    assert!(store.list_tls_identities_local(None, None, 0).is_err());
    assert!(store.list_tls_identities_local(None, None, 17).is_err());
    store
        .save_tls_identity_local(&record(&store, 18, 2), 1)
        .unwrap();
    assert!(matches!(
        store.list_tls_identities_local(first.next, Some(first.snapshot), 16),
        Err(Error::RevisionConflict)
    ));
}
#[test]
fn record_count_and_corrupt_metadata_are_detected() {
    let (_dir, path, mut store) = setup();
    for id in 1..=MAX_TLS_IDENTITIES {
        store
            .save_tls_identity_local(&record(&store, id as u8, 1), 0)
            .unwrap();
    }
    assert_eq!(
        store.save_tls_identity_local(&record(&store, 200, 1), 0),
        Err(Error::Limit)
    );
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute(
        "UPDATE tls_identities SET revision=2 WHERE reference=?1",
        [&[1u8; 32][..]],
    )
    .unwrap();
    assert!(store.load_tls_identity(&[1; 32]).is_err());
    assert!(store.list_tls_identities_local(None, None, 16).is_err());
    assert!(store.integrity_check().is_err());
    drop(sql);
    drop(store);
    assert!(Store::open_existing(&path, Default::default()).is_err());
}

#[test]
fn malformed_oversized_and_foreign_library_rows_cannot_load_list_or_snapshot() {
    for case in 0..3 {
        let (dir, path, mut store) = setup();
        let first = record(&store, 1, 1);
        store.save_tls_identity_local(&first, 0).unwrap();
        let bytes = match case {
            0 => vec![0; morrow_core::tls_identity::MAX_CONTAINER_BYTES + 1],
            1 => vec![0],
            2 => {
                let mut value = first.value().clone();
                value.store_id = vec![255; 32];
                Record::encode(value).unwrap().container().to_vec()
            }
            _ => unreachable!(),
        };
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute("UPDATE tls_identities SET payload=?1", [bytes])
            .unwrap();
        drop(sql);
        assert!(store.load_tls_identity(&[1; 32]).is_err());
        assert!(store.list_tls_identities_local(None, None, 16).is_err());
        assert!(store.integrity_check().is_err());
        assert!(
            store
                .snapshot_to(&dir.path().join("bad-snapshot.db"), 16 * 1024 * 1024)
                .is_err()
        );
        drop(store);
        assert!(Store::open_existing(&path, Default::default()).is_err());
    }
}
#[test]
fn v20_migration_preserves_original_identity_and_refuses_premature_table() {
    let (_dir, path, store) = setup();
    let id = store.tls_store_identity().unwrap();
    drop(store);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.pragma_update(None, "user_version", 20).unwrap();
    drop(sql);
    assert!(Store::open_existing(&path, Default::default()).is_err());
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("DROP TABLE tls_identities;").unwrap();
    drop(sql);
    let mut store = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(store.tls_store_identity().unwrap(), id);
    assert!(
        store
            .list_tls_identities_local(None, None, 16)
            .unwrap()
            .records
            .is_empty()
    );
    store.integrity_check().unwrap();
}

#[cfg(feature = "fault-injection")]
mod crash {
    use super::*;
    #[test]
    #[ignore = "child harness; parent supplies isolated database and crash boundary"]
    fn child() {
        let path = std::env::var_os("MORROW_TLS_STORE_TEST_DB").unwrap();
        let mut store =
            Store::open_existing(std::path::Path::new(&path), Default::default()).unwrap();
        if std::env::var("MORROW_TLS_STORE_TEST_MODE").as_deref() == Ok("replace") {
            store
                .save_tls_identity_local(&record(&store, 1, 2), 1)
                .unwrap();
        }
        panic!("crash boundary was not reached");
    }
    fn run(path: &std::path::Path, mode: &str, point: &str) {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "crash::child", "--ignored", "--nocapture"])
            .env("MORROW_TLS_STORE_TEST_DB", path)
            .env("MORROW_TLS_STORE_TEST_MODE", mode)
            .env("MORROW_TEST_CRASH_AT", point)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert_eq!(status.code(), Some(86));
                break;
            }
            if std::time::Instant::now() > deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("crash child timed out");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    #[test]
    fn migration_crash_boundaries_keep_identity_and_atomic_schema() {
        for (point, version) in [
            ("tls-identity-migration-before-commit", 20),
            ("tls-identity-migration-after-commit", 21),
        ] {
            let (_dir, path, store) = setup();
            let id = store.tls_store_identity().unwrap();
            drop(store);
            let sql = rusqlite::Connection::open(&path).unwrap();
            sql.execute_batch("DROP TABLE tls_identities; PRAGMA user_version=20;")
                .unwrap();
            drop(sql);
            run(&path, "migrate", point);
            let sql = rusqlite::Connection::open(&path).unwrap();
            assert_eq!(
                sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                version
            );
            drop(sql);
            let store = Store::open_existing(&path, Default::default()).unwrap();
            assert_eq!(store.tls_store_identity().unwrap(), id);
            store.integrity_check().unwrap();
        }
    }
    #[test]
    fn replacement_crash_boundaries_preserve_exact_committed_revision() {
        for (point, revision) in [
            ("tls-identity-before-commit", 1),
            ("tls-identity-after-commit", 2),
        ] {
            let (_dir, path, mut store) = setup();
            store
                .save_tls_identity_local(&record(&store, 1, 1), 0)
                .unwrap();
            drop(store);
            run(&path, "replace", point);
            let store = Store::open_existing(&path, Default::default()).unwrap();
            assert_eq!(
                store
                    .load_tls_identity(&[1; 32])
                    .unwrap()
                    .unwrap()
                    .value()
                    .revision,
                revision
            );
            store.integrity_check().unwrap();
        }
    }
}
