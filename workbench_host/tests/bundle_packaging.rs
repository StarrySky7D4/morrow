use morrow_core::plugin_package::catalog;
use std::process::Command;

#[test]
fn immutable_bundle_version_belongs_to_guest_and_cannot_silently_change_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let module = temp.path().join("guest.wasm");
    let archive = temp.path().join("workbench.morrowplugin");
    let original = std::fs::read(std::env::var("MORROW_WORKBENCH_WASM").unwrap()).unwrap();
    std::fs::write(&module, &original).unwrap();
    let package = || {
        Command::new(env!("CARGO_BIN_EXE_package"))
            .arg(&module)
            .arg(&archive)
            .output()
            .unwrap()
    };
    assert!(package().status.success());
    let before = std::fs::read(&archive).unwrap();
    let parsed = catalog::read_file(&archive).unwrap();
    assert_eq!(
        parsed.manifest().package_version,
        morrow_workbench_plugin::PACKAGE_VERSION
    );
    assert!(
        package().status.success(),
        "identical rebuild is idempotent"
    );
    assert_eq!(std::fs::read(&archive).unwrap(), before);
    let mut changed = original;
    // A valid, harmless Wasm custom section still changes immutable package identity.
    changed.extend([0, 2, 1, b'x']);
    std::fs::write(&module, changed).unwrap();
    let rejected = package();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("version bump"));
    assert_eq!(std::fs::read(&archive).unwrap(), before);
}
