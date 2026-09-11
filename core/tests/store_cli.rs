#![cfg(not(target_arch = "wasm32"))]
use std::process::Command;
const EXE: &str = env!("CARGO_BIN_EXE_morrow-core-store");
#[test]
fn cli_create_query_export_import_and_check() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test.db");
    let db = db.to_str().unwrap();
    let call = |args: &[&str]| Command::new(EXE).args(args).output().unwrap();
    assert!(call(&["init", db]).status.success());
    assert!(
        call(&["create-local", db, "create", "card", "old"])
            .status
            .success()
    );
    assert!(
        call(&["rename-local", db, "edit", "card", "1", "new"])
            .status
            .success()
    );
    let query = call(&["query", db, "edit"]);
    assert!(query.status.success());
    assert!(
        String::from_utf8(query.stdout)
            .unwrap()
            .contains("revision=2")
    );
    let path = dir.path().join("export.morrow");
    let path = path.to_str().unwrap();
    assert!(call(&["export", db, "card", path]).status.success());
    assert!(!call(&["export", db, "card", path]).status.success());
    let second = dir.path().join("second.db");
    let second = second.to_str().unwrap();
    assert!(call(&["init", second]).status.success());
    assert!(
        call(&["import-container-local", second, "import", path])
            .status
            .success()
    );
    assert!(call(&["check", second]).status.success());
    let saved = morrow_core::envelope::decode(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(saved.summary().revision, 2);
}
#[test]
fn query_missing_database_is_not_a_creation_command() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent.db");
    let result = Command::new(EXE)
        .arg("query")
        .arg(&path)
        .arg("op")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!path.exists());
}

#[test]
fn cli_attachment_export_preserves_original_and_refuses_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db");
    let source = dir.path().join("source.bin");
    let destination = dir.path().join("export.bin");
    let bytes: Vec<u8> = (0..100_000).map(|i| (i * 37) as u8).collect();
    std::fs::write(&source, &bytes).unwrap();
    let call = |action: &str, args: &[&str]| {
        Command::new(EXE)
            .arg(action)
            .arg(&db)
            .args(args)
            .output()
            .unwrap()
    };
    assert!(call("init", &[]).status.success());
    let result = call("stage-file-local", &[source.to_str().unwrap()]);
    assert!(result.status.success());
    let id = String::from_utf8(result.stdout).unwrap();
    let id = id.trim();
    assert!(
        call(
            "create-attachment-local",
            &["create", "card", "title", id, "source.bin"]
        )
        .status
        .success()
    );
    assert!(
        call(
            "export-attachment-local",
            &["card", "attachment-1", destination.to_str().unwrap()]
        )
        .status
        .success()
    );
    assert_eq!(std::fs::read(&destination).unwrap(), bytes);
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    assert!(
        !call(
            "export-attachment-local",
            &["card", "attachment-1", destination.to_str().unwrap()]
        )
        .status
        .success()
    );
    assert_eq!(std::fs::read(&destination).unwrap(), bytes);
    assert!(call("check", &[]).status.success());
}
