//! Declaration/Manager isolation only. No upload Runner, protected owner or device effects.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    lifecycle::InstancePhase,
    plugin_package::{Package, catalog::Catalog, proto, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{Fault, Limits, manager::Manager, package::PreparedPackage};
fn package(upload: bool) -> Package {
    let module=wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"morrow_run\") (result i32) i32.const 0))").unwrap();
    let m = if upload {
        Package::manifest_for_blob_upload(
            "org.example.upload",
            "1.0.0",
            &module,
            vec![proto::TransformHandler {
                handler: "blob.upload".into(),
                input_type: "upload.input.v1".into(),
                output_type: "upload.result.v1".into(),
                max_input_bytes: 1024,
                max_output_bytes: 1024,
            }],
        )
    } else {
        Package::manifest_for("org.example.ordinary", "1.0.0", &module, vec![])
    };
    Package::build(m, &module).unwrap()
}
#[test]
fn independent_upload_declaration_cannot_fall_through_to_pure_execution() {
    assert!(matches!(
        PreparedPackage::new(package(true), Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    assert!(PreparedPackage::new(package(false), Limits::default()).is_ok());
}
#[test]
fn manager_validates_upload_approval_before_revoking_an_unrelated_legacy_instance() {
    let d = tempfile::tempdir().unwrap();
    let p = package(false);
    let catalog = Catalog::open(&d.path().join("packages")).unwrap();
    catalog.install(&p).unwrap();
    let mut manager = Manager::new(
        Registry::open(&d.path().join("registry"), catalog).unwrap(),
        Limits::default(),
    );
    let mut host =
        HostRuntime::new(Store::open(&d.path().join("db"), EventBudget::default()).unwrap())
            .unwrap();
    let id = &p.manifest().package_id;
    manager.select(&p, manager.revision()).unwrap();
    manager
        .set_enabled(id, p.digest(), true, manager.revision())
        .unwrap();
    let instance = manager.connect(id, &mut host).unwrap();
    assert!(manager.connect_blob_upload(id, &mut host, Limits::default()).is_err());
    let before = manager.persisted_snapshot().unwrap();
    let revision = manager.revision();
    assert!(
        manager
            .approve_blob_upload(id, p.digest(), true, revision)
            .is_err()
    );
    assert!(
        manager
            .approve_blob_upload(id, [9; 32], false, revision)
            .is_err()
    );
    manager
        .approve_blob_upload(id, p.digest(), false, revision)
        .unwrap();
    assert_eq!(manager.revision(), revision);
    assert_eq!(manager.persisted_snapshot().unwrap(), before);
    assert_eq!(
        host.connection_phase(instance.connection()),
        Ok(InstancePhase::Ready)
    );
    instance.close(&mut host).unwrap();
}
#[test]
fn manager_approval_is_exact_persistent_ceiling_and_does_not_create_an_upload_instance() {
    let d = tempfile::tempdir().unwrap();
    let p = package(true);
    let catalog = Catalog::open(&d.path().join("packages")).unwrap();
    catalog.install(&p).unwrap();
    let mut manager = Manager::new(
        Registry::open(&d.path().join("registry"), catalog).unwrap(),
        Limits::default(),
    );
    manager.select(&p, 0).unwrap();
    let id = &p.manifest().package_id;
    manager
        .approve_blob_upload(id, p.digest(), true, manager.revision())
        .unwrap();
    let selection = manager.selection(id).unwrap();
    assert!(selection.approved_blob_upload && !selection.enabled);
    assert!(selection.approved.is_empty() && selection.approved_io.is_empty());
    let revision = manager.revision();
    manager
        .approve_blob_upload(id, p.digest(), true, revision)
        .unwrap();
    assert_eq!(manager.revision(), revision);
    manager
        .approve_blob_upload(id, p.digest(), false, revision)
        .unwrap();
    assert!(!manager.selection(id).unwrap().approved_blob_upload);
}
