#![cfg(target_os = "windows")]
use morrow_audit::{
    keys::Key,
    recovery::restore_key,
    session::{OpenMode, Session, SessionError, key_path},
};
use morrow_core::{content::CardRecord, store::EventBudget};
use std::path::Path;
fn open(db: &Path) -> Session {
    Session::open(db, EventBudget::default(), OpenMode::Initialize).unwrap()
}
#[test]
fn live_session_backup_keeps_original_ciphertext_pending_events_and_existing_files() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("workbench.db");
    let mut session = open(&db);
    session
        .runtime()
        .store_local_mut()
        .create_local(
            "op",
            &CardRecord::new("card", "text", 1, "kept", vec![1, 2, 3]).unwrap(),
        )
        .unwrap();
    let key = key_path(&db).unwrap();
    let original = std::fs::read(&key).unwrap();
    let before = std::fs::read(&db).unwrap();
    let backup = d.path().join("original.backup");
    session.backup_key(&backup).unwrap();
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    assert_eq!(std::fs::read(&db).unwrap(), before);
    assert_eq!(session.store().pending_usage().unwrap().0, 1);
    assert!(matches!(
        session.backup_key(&backup),
        Err(SessionError::BackupAlreadyExists)
    ));
    assert!(matches!(
        session.backup_key(&key),
        Err(SessionError::BackupAlreadyExists)
    ));
    let unrelated = d.path().join("unrelated");
    std::fs::write(&unrelated, b"preserve me").unwrap();
    assert!(session.backup_key(&unrelated).is_err());
    assert_eq!(std::fs::read(unrelated).unwrap(), b"preserve me");
    drop(session);
    std::fs::remove_file(&key).unwrap();
    restore_key(&db, &backup).unwrap();
    assert_eq!(std::fs::read(&key).unwrap(), original);
    let session = open(&db);
    assert_eq!(
        session
            .store()
            .card("card")
            .unwrap()
            .unwrap()
            .summary()
            .title,
        "kept"
    );
}
#[test]
fn live_identity_refuses_replaced_damaged_or_missing_source_and_invalid_destination() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("workbench.db");
    let session = open(&db);
    let key = key_path(&db).unwrap();
    let original = std::fs::read(&key).unwrap();
    let target = d.path().join("backup");
    let foreign = d.path().join("foreign");
    Key::create(&foreign).unwrap();
    std::fs::copy(&foreign, &key).unwrap();
    assert!(matches!(
        session.backup_key(&target),
        Err(SessionError::KeyMismatch)
    ));
    assert!(!target.exists());
    std::fs::write(&key, b"damaged").unwrap();
    assert!(session.backup_key(&target).is_err());
    assert!(!target.exists());
    std::fs::remove_file(&key).unwrap();
    assert!(session.backup_key(&target).is_err());
    assert!(!target.exists());
    std::fs::write(&key, &original).unwrap();
    assert!(
        session
            .backup_key(&d.path().join("absent/folder/backup"))
            .is_err()
    );
    assert_eq!(std::fs::read(&key).unwrap(), original);
}
#[test]
#[cfg(feature = "fault-injection")]
fn backup_child() {
    let Ok(root) = std::env::var("MORROW_BACKUP_TEST_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    open(&root.join("workbench.db"))
        .backup_key(&root.join("backup"))
        .unwrap();
    panic!("fault hook missed");
}
#[test]
#[cfg(feature = "fault-injection")]
fn publication_crashes_leave_no_partial_backup_and_never_change_identity() {
    for point in ["backup-before-publish", "backup-after-publish"] {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("workbench.db");
        drop(open(&db));
        let before = std::fs::read(&db).unwrap();
        let original = std::fs::read(key_path(&db).unwrap()).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "backup_child", "--nocapture"])
            .env("MORROW_BACKUP_TEST_ROOT", d.path())
            .env("MORROW_AUDIT_CRASH_AT", point)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86));
        assert_eq!(std::fs::read(&db).unwrap(), before);
        assert_eq!(std::fs::read(key_path(&db).unwrap()).unwrap(), original);
        let target = d.path().join("backup");
        if point == "backup-before-publish" {
            assert!(!target.exists());
            open(&db).backup_key(&target).unwrap();
        } else {
            assert_eq!(std::fs::read(&target).unwrap(), original);
        }
        assert_eq!(
            Key::load(&target).unwrap().trust().key,
            Key::load(&key_path(&db).unwrap()).unwrap().trust().key
        );
    }
}
