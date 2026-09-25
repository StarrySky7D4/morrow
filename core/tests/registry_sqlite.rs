#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        catalog::Catalog,
        proto::Capability,
        registry::{Registry, RegistryStorage, sqlite::SqliteRegistryStorage},
    },
};
use prost::Message;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};
mod wire {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.registry.v1.rs"));
}
// Independent fixture writer for unknown schema and maximum-revision inputs.
fn pack(magic: &[u8; 8], raw: &[u8], limit: usize) -> morrow_core::Result<Vec<u8>> {
    assert!(raw.len() <= limit);
    let compressed = lz4_flex::block::compress(raw);
    let mut bytes = magic.to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend(compressed);
    Ok(bytes)
}
fn open(path: &Path, create: bool) -> morrow_core::Result<Registry> {
    Registry::from_storage(Box::new(SqliteRegistryStorage::open(path, create)?))
}
fn package(version: &str, caps: Vec<Capability>) -> Package {
    let wasm = b"\0asm\x01\0\0\0";
    Package::build(
        Package::manifest_for("org.example.test", version, wasm, caps),
        wasm,
    )
    .unwrap()
}
#[test]
fn sqlite_and_native_decisions_persist_identical_envelopes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plugins.sqlite3");
    let mut sqlite = open(&path, true).unwrap();
    let mut native = Registry::open(
        &dir.path().join("registry"),
        Catalog::open(&dir.path().join("packages")).unwrap(),
    )
    .unwrap();
    let first = package(
        "1.0.0",
        vec![Capability::RenameCard, Capability::ReadSummary],
    );
    let next = package(
        "2.0.0",
        vec![Capability::RenameCard, Capability::ReadAttachment],
    );
    for r in [&mut native, &mut sqlite] {
        r.install_package(first.archive()).unwrap();
        r.install_package(next.archive()).unwrap();
        r.select(first.digest(), 0).unwrap();
        r.approve(
            "org.example.test",
            first.digest(),
            BTreeSet::from([GrantKind::Rename, GrantKind::ReadSummary]),
            1,
        )
        .unwrap();
        r.set_enabled("org.example.test", first.digest(), true, 2)
            .unwrap();
    }
    assert_eq!(
        native.persisted_snapshot().unwrap(),
        sqlite.persisted_snapshot().unwrap()
    );
    for r in [&mut native, &mut sqlite] {
        r.select(next.digest(), 3).unwrap();
        let selected = r.selection("org.example.test").unwrap();
        assert!(!selected.enabled);
        assert_eq!(selected.approved, BTreeSet::from([GrantKind::Rename]));
        assert_eq!(
            r.set_enabled("org.example.test", first.digest(), true, 4),
            Err(Error::RevisionConflict)
        );
    }
    let expected = native.persisted_snapshot().unwrap();
    assert_eq!(expected, sqlite.persisted_snapshot().unwrap());
    drop(sqlite);
    let reopened = open(&path, false).unwrap();
    assert_eq!(expected, reopened.persisted_snapshot().unwrap());
    assert_eq!(reopened.revision(), 4);
}
#[test]
fn exclusive_owner_and_corrupt_archive_are_never_bypassed_or_repaired() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plugins.sqlite3");
    let r = open(&path, true).unwrap();
    assert!(matches!(open(&path, false), Err(Error::StorageBusy)));
    let package = package("1.0.0", vec![]);
    r.install_package(package.archive()).unwrap();
    drop(r);
    let raw = Connection::open(&path).unwrap();
    raw.execute("UPDATE packages SET archive=x'00'", [])
        .unwrap();
    drop(raw);
    let mut r = open(&path, false).unwrap();
    assert!(r.install_package(package.archive()).is_err());
    assert!(r.select(package.digest(), 0).is_err());
    assert_eq!(r.revision(), 0);
    drop(r);
    let raw = Connection::open(&path).unwrap();
    let bytes: Vec<u8> = raw
        .query_row("SELECT archive FROM packages", [], |row| row.get(0))
        .unwrap();
    assert_eq!(bytes, vec![0]);
}
#[test]
fn unrelated_future_and_invalid_snapshots_fail_without_rewriting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plugins.sqlite3");
    let raw = Connection::open(&path).unwrap();
    raw.execute_batch("CREATE TABLE unrelated(value TEXT); INSERT INTO unrelated VALUES('keep');")
        .unwrap();
    drop(raw);
    let before = fs::read(&path).unwrap();
    assert!(open(&path, true).is_err());
    assert_eq!(before, fs::read(&path).unwrap());
    let path = dir.path().join("future.sqlite3");
    let mut storage = SqliteRegistryStorage::open(&path, true).unwrap();
    let bytes = pack(
        b"MORROWG1",
        &wire::Registry {
            schema_version: 3,
            revision: 1,
            selections: vec![],
            dependency_locks: vec![],
        }
        .encode_to_vec(),
        512 * 1024,
    )
    .unwrap();
    storage.publish(&bytes).unwrap();
    drop(storage);
    assert!(matches!(open(&path, false), Err(Error::UnsupportedVersion)));
    let storage = SqliteRegistryStorage::open(&path, false).unwrap();
    assert_eq!(storage.read().unwrap().unwrap(), bytes);
}
#[test]
fn full_u64_revision_survives_sqlite_and_overflow_never_publishes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plugins.sqlite3");
    let mut storage = SqliteRegistryStorage::open(&path, true).unwrap();
    let bytes = pack(
        b"MORROWG1",
        &wire::Registry {
            schema_version: 2,
            revision: u64::MAX,
            selections: vec![],
            dependency_locks: vec![],
        }
        .encode_to_vec(),
        512 * 1024,
    )
    .unwrap();
    storage.publish(&bytes).unwrap();
    let mut r = Registry::from_storage(Box::new(storage)).unwrap();
    let package = package("1.0.0", vec![]);
    r.install_package(package.archive()).unwrap();
    assert_eq!(r.revision(), u64::MAX);
    assert_eq!(r.select(package.digest(), u64::MAX), Err(Error::Limit));
    assert_eq!(r.persisted_snapshot().unwrap().unwrap(), bytes);
}

#[test]
fn unknown_commit_blocks_even_noop_confirmation_until_reopen() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Uncertain {
        storage: SqliteRegistryStorage,
        fail: Arc<AtomicBool>,
    }
    impl RegistryStorage for Uncertain {
        fn read(&self) -> morrow_core::Result<Option<Vec<u8>>> {
            self.storage.read()
        }
        fn load_package(&self, digest: [u8; 32]) -> morrow_core::Result<Package> {
            self.storage.load_package(digest)
        }
        fn install_package(&self, package: &Package) -> morrow_core::Result<()> {
            self.storage.install_package(package)
        }
        fn publish(&mut self, bytes: &[u8]) -> morrow_core::Result<()> {
            self.storage.publish(bytes)?;
            if self.fail.load(Ordering::SeqCst) {
                Err(Error::CommitUnknown)
            } else {
                Ok(())
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unknown.sqlite3");
    let fail = Arc::new(AtomicBool::new(false));
    let mut registry = Registry::from_storage(Box::new(Uncertain {
        storage: SqliteRegistryStorage::open(&path, true).unwrap(),
        fail: fail.clone(),
    }))
    .unwrap();
    let package = package("1.0.0", vec![]);
    registry.install_package(package.archive()).unwrap();
    registry.select(package.digest(), 0).unwrap();
    registry
        .set_enabled("org.example.test", package.digest(), true, 1)
        .unwrap();
    fail.store(true, Ordering::SeqCst);
    assert_eq!(
        registry.set_enabled("org.example.test", package.digest(), false, 2),
        Err(Error::CommitUnknown)
    );
    assert_eq!(
        registry.set_enabled("org.example.test", package.digest(), true, 2),
        Err(Error::CommitUnknown)
    );
    assert_eq!(
        registry.select(package.digest(), 2),
        Err(Error::CommitUnknown)
    );
    assert!(matches!(
        registry.resolve_enabled("org.example.test"),
        Err(Error::CommitUnknown)
    ));
    drop(registry);
    let reopened = open(&path, false).unwrap();
    assert_eq!(reopened.revision(), 3);
    assert!(!reopened.selection("org.example.test").unwrap().enabled);
}
