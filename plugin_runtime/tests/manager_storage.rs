#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    Error,
    dispatch::HostRuntime,
    plugin_package::{
        Package,
        registry::{Registry, RegistryStorage, sqlite::SqliteRegistryStorage},
    },
    store::Store,
};
use morrow_plugin_runtime::{
    Limits,
    manager::{Manager, ManagerError},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn unknown_publication_revokes_live_instance_and_blocks_noop_confirmation() {
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
    let path = dir.path().join("registry.sqlite3");
    let fail = Arc::new(AtomicBool::new(false));
    let registry = Registry::from_storage(Box::new(Uncertain {
        storage: SqliteRegistryStorage::open(&path, true).unwrap(),
        fail: fail.clone(),
    }))
    .unwrap();
    let mut manager = Manager::new(registry, Limits::default());
    let wasm = wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
    let package = Package::build(
        Package::manifest_for("test.unknown", "1.0.0", &wasm, vec![]),
        &wasm,
    )
    .unwrap();
    manager.install_package(package.archive()).unwrap();
    manager.select(&package, 0).unwrap();
    manager
        .set_enabled("test.unknown", package.digest(), true, 1)
        .unwrap();
    let mut host =
        HostRuntime::new(Store::open(&dir.path().join("content.db"), Default::default()).unwrap())
            .unwrap();
    let instance = manager.connect("test.unknown", &mut host).unwrap();
    fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        manager.set_enabled("test.unknown", package.digest(), false, 2),
        Err(ManagerError::Core(Error::CommitUnknown))
    ));
    assert!(instance.run(&mut host, || 1).outcome.is_err());
    // Both of these are no-ops against the old memory snapshot. They still must fail.
    assert!(matches!(
        manager.set_enabled("test.unknown", package.digest(), true, 2),
        Err(ManagerError::Core(Error::CommitUnknown))
    ));
    assert!(matches!(
        manager.select(&package, 2),
        Err(ManagerError::Core(Error::CommitUnknown))
    ));
    assert!(matches!(
        manager.connect("test.unknown", &mut host),
        Err(ManagerError::Core(Error::CommitUnknown))
    ));
    instance.close(&mut host).unwrap();
    drop(manager);
    let registry =
        Registry::from_storage(Box::new(SqliteRegistryStorage::open(&path, false).unwrap()))
            .unwrap();
    assert_eq!(registry.revision(), 3);
    assert!(!registry.selection("test.unknown").unwrap().enabled);
}
