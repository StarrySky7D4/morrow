use morrow_core::{content::CardRecord, envelope};
use std::process::Command;

#[test]
fn verifier_accepts_valid_container_without_modifying_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("card.morrow");
    let card = CardRecord::new("card-1", "unknown.plugin", 1, "title", vec![255]).unwrap();
    let bytes = envelope::encode(&card).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_morrow-core-check"))
        .arg("verify")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn verifier_rejects_corrupt_input_and_does_not_create_missing_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bad.morrow");
    std::fs::write(&path, b"corrupt").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_morrow-core-check"))
        .arg("verify")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(path).unwrap(), b"corrupt");
    let missing = directory.path().join("missing.morrow");
    let output = Command::new(env!("CARGO_BIN_EXE_morrow-core-check"))
        .arg("verify")
        .arg(&missing)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!missing.exists());
}
