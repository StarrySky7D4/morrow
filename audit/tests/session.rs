#![cfg(target_os = "windows")]
use morrow_audit::{
    keys::Key,
    session::{OpenMode, Session, SessionError, key_path},
};
use morrow_core::{
    content::CardRecord,
    store::{AuditBindingState, EventBudget, Store},
};
fn open(path: &std::path::Path, mode: OpenMode) -> Result<Session, SessionError> {
    Session::open(path, EventBudget::default(), mode)
}
#[test]
fn first_binding_is_permanent_even_without_events_and_key_loss_is_not_first_use() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("db");
    let session = open(&db, OpenMode::Initialize).unwrap();
    let trust = session.trust();
    let key = key_path(&db).unwrap();
    assert!(matches!(
        Store::audit_binding_status(&db).unwrap(),
        AuditBindingState::Bound(_)
    ));
    assert!(matches!(
        open(&db, OpenMode::Existing),
        Err(SessionError::Busy)
    ));
    drop(session);
    let session = open(&db, OpenMode::Existing).unwrap();
    assert_eq!(session.trust().key, trust.key);
    drop(session);
    let before = std::fs::read(&db).unwrap();
    std::fs::rename(&key, d.path().join("retained-key")).unwrap();
    for mode in [OpenMode::Initialize, OpenMode::Existing] {
        assert!(matches!(open(&db, mode), Err(SessionError::MissingKey)));
        assert!(!key.exists());
        assert_eq!(std::fs::read(&db).unwrap(), before);
    }
    std::fs::rename(d.path().join("retained-key"), &key).unwrap();
    let restored = open(&db, OpenMode::Existing).unwrap();
    assert_eq!(restored.trust().key, trust.key);
}
#[test]
fn missing_database_wrong_key_and_damaged_database_are_not_reinitialized() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("db");
    let key = key_path(&db).unwrap();
    assert!(matches!(
        open(&db, OpenMode::Existing),
        Err(SessionError::MissingDatabase)
    ));
    assert!(!db.exists());
    drop(open(&db, OpenMode::Initialize).unwrap());
    let original = std::fs::read(&key).unwrap();
    let before = std::fs::read(&db).unwrap();
    let foreign = d.path().join("foreign");
    Key::create(&foreign).unwrap();
    std::fs::copy(&foreign, &key).unwrap();
    assert!(matches!(
        open(&db, OpenMode::Initialize),
        Err(SessionError::KeyMismatch)
    ));
    assert_eq!(std::fs::read(&db).unwrap(), before);
    std::fs::write(&key, &original).unwrap();
    std::fs::rename(&db, d.path().join("retained-db")).unwrap();
    assert!(matches!(
        open(&db, OpenMode::Initialize),
        Err(SessionError::KeyWithoutDatabase)
    ));
    assert!(!db.exists());
    assert_eq!(std::fs::read(&key).unwrap(), original);
    std::fs::write(&db, []).unwrap();
    assert!(matches!(
        open(&db, OpenMode::Initialize),
        Err(SessionError::KeyWithoutDatabase)
    ));
}
#[test]
fn explicit_initialization_preserves_legacy_content_and_recovers_empty_file() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("db");
    let mut store = Store::open(&db, EventBudget::default()).unwrap();
    store
        .create_local(
            "create",
            &CardRecord::new("card", "text", 1, "Legacy", vec![1]).unwrap(),
        )
        .unwrap();
    drop(store);
    assert!(matches!(
        open(&db, OpenMode::Existing),
        Err(SessionError::InitializationRequired)
    ));
    assert!(!key_path(&db).unwrap().exists());
    let mut session = open(&db, OpenMode::Initialize).unwrap();
    assert_eq!(session.flush(1).unwrap().events, 1);
    assert!(session.store().card("card").unwrap().is_some());
    drop(session);
    let empty = d.path().join("empty");
    std::fs::write(&empty, []).unwrap();
    drop(open(&empty, OpenMode::Initialize).unwrap());
    assert!(matches!(
        Store::audit_binding_status(&empty).unwrap(),
        AuditBindingState::Bound(_)
    ));
    let foreign = d.path().join("not-sqlite");
    std::fs::write(&foreign, b"keep").unwrap();
    assert!(open(&foreign, OpenMode::Initialize).is_err());
    assert_eq!(std::fs::read(&foreign).unwrap(), b"keep");
    assert!(!key_path(&foreign).unwrap().exists());
}
#[test]
fn bound_database_cannot_be_opened_without_independent_trust_before_first_event() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("db");
    drop(open(&db, OpenMode::Initialize).unwrap());
    assert!(Store::open_existing(&db, EventBudget::default()).is_err());
    let unrelated = Key::create(&d.path().join("other")).unwrap();
    assert!(Store::open_audited(&db, EventBudget::default(), false, unrelated.trust()).is_err());
}
#[cfg(feature = "fault-injection")]
#[test]
fn bootstrap_child() {
    let Ok(root) = std::env::var("MORROW_BOOTSTRAP_ROOT") else {
        return;
    };
    open(
        &std::path::Path::new(&root).join("db"),
        OpenMode::Initialize,
    )
    .unwrap();
    panic!("bootstrap crash missing");
}
#[cfg(feature = "fault-injection")]
#[test]
fn initialization_crashes_resume_published_key_and_keep_binding_atomic() {
    for point in [
        "bootstrap-after-database",
        "bootstrap-after-key",
        "binding-before-commit",
        "binding-after-commit",
        "bootstrap-after-binding",
    ] {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("db");
        // Start with a valid unbound DB so the binding failpoints target identity binding.
        drop(Store::open(&db, EventBudget::default()).unwrap());
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "bootstrap_child", "--nocapture"])
            .env("MORROW_BOOTSTRAP_ROOT", d.path());
        if point == "bootstrap-after-database" {
            std::fs::remove_file(&db).unwrap();
        }
        if point.starts_with("binding-") {
            command.env("MORROW_TEST_CRASH_AT", point);
        } else {
            command.env("MORROW_AUDIT_CRASH_AT", point);
        }
        let status = command.status().unwrap();
        assert_eq!(status.code(), Some(86), "{point}");
        let key = key_path(&db).unwrap();
        let original = if key.exists() {
            Some(Key::load(&key).unwrap().trust())
        } else {
            None
        };
        let bound = matches!(
            Store::audit_binding_status(&db).unwrap(),
            AuditBindingState::Bound(_)
        );
        assert_eq!(
            bound,
            matches!(point, "binding-after-commit" | "bootstrap-after-binding")
        );
        let recovered = open(&db, OpenMode::Initialize).unwrap();
        if let Some(original) = original {
            assert_eq!(original.key, recovered.trust().key);
            assert_eq!(original.id, recovered.trust().id);
        }
        recovered.store().integrity_check().unwrap();
        drop(recovered);
        drop(open(&db, OpenMode::Existing).unwrap());
        println!("PASS bootstrap crash {point}");
    }
}

#[test]
fn legacy_signed_store_requires_original_key_and_migrates_binding_atomically() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("db");
    let mut session = open(&db, OpenMode::Initialize).unwrap();
    let trust = session.trust();
    session
        .runtime()
        .store_local_mut()
        .create_local(
            "create",
            &CardRecord::new("card", "text", 1, "Legacy signed", vec![]).unwrap(),
        )
        .unwrap();
    session.flush(1).unwrap();
    drop(session);
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute_batch("DROP TABLE task_evidence_chunks; DROP TABLE evidence_chunks; DROP TABLE operation_evidence; DROP TABLE task_evidence; DROP TABLE audit_identity; PRAGMA user_version=5;")
        .unwrap();
    drop(connection);
    assert!(matches!(
        Store::audit_binding_status(&db).unwrap(),
        AuditBindingState::LegacySealed
    ));
    let key = key_path(&db).unwrap();
    let backup = d.path().join("original-key");
    std::fs::rename(&key, &backup).unwrap();
    assert!(matches!(
        open(&db, OpenMode::Initialize),
        Err(SessionError::MissingKey)
    ));
    assert!(!key.exists());
    Key::create(&key).unwrap();
    let before = std::fs::read(&db).unwrap();
    assert!(open(&db, OpenMode::Initialize).is_err());
    assert_eq!(std::fs::read(&db).unwrap(), before);
    std::fs::copy(&backup, &key).unwrap();
    let session = open(&db, OpenMode::Existing).unwrap();
    assert_eq!(session.trust().key, trust.key);
    assert!(session.store().card("card").unwrap().is_some());
    assert!(matches!(
        Store::audit_binding_status(&db).unwrap(),
        AuditBindingState::Bound(_)
    ));
}
#[test]
fn actual_cli_honors_the_session_lease_and_reopens_after_release() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("db");
    let session = open(&db, OpenMode::Initialize).unwrap();
    let exe = env!("CARGO_BIN_EXE_morrow-audit-seal");
    let output = std::process::Command::new(exe)
        .arg("--open-bound")
        .arg(&db)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr).unwrap().contains("Busy"));
    drop(session);
    let output = std::process::Command::new(exe)
        .arg("--open-bound")
        .arg(&db)
        .output()
        .unwrap();
    assert!(output.status.success());
}
