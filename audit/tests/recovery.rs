#![cfg(target_os = "windows")]
use morrow_audit::{
    keys::Key,
    recovery::{Recovery, restore_key},
    session::{OpenMode, Session, SessionError, key_path},
};
use morrow_core::{
    content::CardRecord,
    store::{EventBudget, Store},
};
use std::path::Path;
fn seed(root: &Path) -> (std::path::PathBuf, Vec<u8>) {
    let db = root.join("workbench.db");
    let mut session = Session::open(&db, EventBudget::default(), OpenMode::Initialize).unwrap();
    let record =
        CardRecord::new("original", "org.example.note", 1, "original", vec![1, 2, 3]).unwrap();
    session
        .runtime()
        .store_local_mut()
        .create_local("original-op", &record)
        .unwrap();
    session.flush(16).unwrap();
    drop(session);
    let bytes = std::fs::read(key_path(&db).unwrap()).unwrap();
    std::fs::write(root.join("backup"), &bytes).unwrap();
    (db, bytes)
}
#[test]
fn missing_and_damaged_protection_restore_exact_original_bytes_without_database_changes() {
    for damage in [false, true] {
        let d = tempfile::tempdir().unwrap();
        let (db, original) = seed(d.path());
        let key = key_path(&db).unwrap();
        let before = std::fs::read(&db).unwrap();
        if damage {
            std::fs::write(&key, b"damaged original").unwrap();
        } else {
            std::fs::remove_file(&key).unwrap();
        }
        let Recovery::Restored { preserved } = restore_key(&db, &d.path().join("backup")).unwrap()
        else {
            panic!()
        };
        if damage {
            assert_eq!(
                std::fs::read(preserved.unwrap()).unwrap(),
                b"damaged original"
            );
        } else {
            assert!(preserved.is_none());
        }
        assert_eq!(std::fs::read(&db).unwrap(), before);
        assert_eq!(std::fs::read(&key).unwrap(), original);
        assert_eq!(
            restore_key(&db, &d.path().join("backup")).unwrap(),
            Recovery::AlreadyPresent
        );
        let session = Session::open(&db, EventBudget::default(), OpenMode::Existing).unwrap();
        assert_eq!(
            session
                .store()
                .card("original")
                .unwrap()
                .unwrap()
                .summary()
                .title,
            "original"
        );
    }
}
#[test]
fn foreign_keys_corrupt_history_unbound_databases_and_busy_sessions_do_not_publish() {
    let d = tempfile::tempdir().unwrap();
    let (db, original) = seed(d.path());
    let key = key_path(&db).unwrap();
    Key::create(&d.path().join("foreign")).unwrap();
    assert!(matches!(
        restore_key(&db, &d.path().join("foreign")),
        Err(SessionError::KeyMismatch)
    ));
    assert_eq!(std::fs::read(&key).unwrap(), original);
    let session = Session::open(&db, EventBudget::default(), OpenMode::Existing).unwrap();
    assert!(matches!(
        restore_key(&db, &d.path().join("backup")),
        Err(SessionError::Busy)
    ));
    drop(session);
    std::fs::remove_file(&key).unwrap();
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute("UPDATE cards SET payload=x'00' WHERE id='original'", [])
        .unwrap();
    drop(connection);
    let before = std::fs::read(&db).unwrap();
    assert!(restore_key(&db, &d.path().join("backup")).is_err());
    assert!(!key.exists());
    assert_eq!(std::fs::read(&db).unwrap(), before);
    let unbound = d.path().join("unbound.db");
    drop(Store::open(&unbound, EventBudget::default()).unwrap());
    assert!(matches!(
        restore_key(&unbound, &d.path().join("backup")),
        Err(SessionError::RecoveryRequiresBinding)
    ));
    assert!(!key_path(&unbound).unwrap().exists());
}
#[test]
fn invalid_selection_and_oversized_existing_file_remain_untouched() {
    let d = tempfile::tempdir().unwrap();
    let (db, original) = seed(d.path());
    let key = key_path(&db).unwrap();
    std::fs::write(d.path().join("invalid"), b"not a protected key").unwrap();
    assert!(restore_key(&db, &d.path().join("invalid")).is_err());
    assert_eq!(std::fs::read(&key).unwrap(), original);
    let oversized = vec![42; 65537];
    std::fs::write(&key, &oversized).unwrap();
    assert!(restore_key(&db, &d.path().join("backup")).is_err());
    assert_eq!(std::fs::read(&key).unwrap(), oversized);
}
#[test]
#[cfg(feature = "fault-injection")]
fn restore_child() {
    let Ok(root) = std::env::var("MORROW_RESTORE_TEST_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    restore_key(&root.join("workbench.db"), &root.join("backup")).unwrap();
    panic!("fault hook missed");
}
#[test]
#[cfg(feature = "fault-injection")]
fn recovery_process_crashes_keep_original_identity_and_preserved_damage() {
    for point in [
        "restore-after-backup",
        "restore-before-publish",
        "restore-after-publish",
    ] {
        for damage in [false, true] {
            let d = tempfile::tempdir().unwrap();
            let (db, original) = seed(d.path());
            let before = std::fs::read(&db).unwrap();
            let key = key_path(&db).unwrap();
            if damage {
                std::fs::write(&key, b"damaged original").unwrap();
            } else {
                std::fs::remove_file(&key).unwrap();
            }
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "restore_child", "--nocapture"])
                .env("MORROW_RESTORE_TEST_ROOT", d.path())
                .env("MORROW_AUDIT_CRASH_AT", point)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(86));
            assert_eq!(std::fs::read(&db).unwrap(), before);
            if point == "restore-after-publish" {
                assert_eq!(std::fs::read(&key).unwrap(), original);
            } else if damage {
                assert_eq!(std::fs::read(&key).unwrap(), b"damaged original");
            } else {
                assert!(!key.exists());
            }
            restore_key(&db, &d.path().join("backup")).unwrap();
            assert_eq!(std::fs::read(&key).unwrap(), original);
            assert_eq!(std::fs::read(&db).unwrap(), before);
            if damage {
                let backups = std::fs::read_dir(d.path())
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .starts_with("morrow-audit-key-before-restore-")
                    })
                    .map(|e| std::fs::read(e.path()).unwrap())
                    .collect::<Vec<_>>();
                assert!(!backups.is_empty());
                assert!(backups.iter().all(|b| b == b"damaged original"));
            }
        }
    }
}

#[test]
fn an_existing_foreign_identity_is_preserved_before_restoring_the_original() {
    let d = tempfile::tempdir().unwrap();
    let (db, original) = seed(d.path());
    let before = std::fs::read(&db).unwrap();
    let key = key_path(&db).unwrap();
    let foreign = d.path().join("foreign");
    Key::create(&foreign).unwrap();
    let wrong = std::fs::read(&foreign).unwrap();
    std::fs::write(&key, &wrong).unwrap();
    let Recovery::Restored {
        preserved: Some(backup),
    } = restore_key(&db, &d.path().join("backup")).unwrap()
    else {
        panic!("missing backup")
    };
    assert_eq!(std::fs::read(backup).unwrap(), wrong);
    assert_eq!(std::fs::read(&key).unwrap(), original);
    assert_eq!(std::fs::read(&db).unwrap(), before);
}

#[test]
fn bound_empty_and_legacy_signed_databases_require_the_original_identity_without_migration() {
    for legacy in [false, true] {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("workbench.db");
        if legacy {
            seed(d.path());
            let connection = rusqlite::Connection::open(&db).unwrap();
            connection
                .execute_batch("DROP TABLE task_evidence_chunks; DROP TABLE evidence_chunks; DROP TABLE operation_evidence; DROP TABLE task_evidence; DROP TABLE audit_identity; PRAGMA user_version=5;")
                .unwrap();
        } else {
            drop(Session::open(&db, EventBudget::default(), OpenMode::Initialize).unwrap());
        }
        let key = key_path(&db).unwrap();
        let original = std::fs::read(&key).unwrap();
        std::fs::write(d.path().join("backup"), &original).unwrap();
        std::fs::remove_file(&key).unwrap();
        let before = std::fs::read(&db).unwrap();
        Key::create(&d.path().join("foreign")).unwrap();
        assert!(restore_key(&db, &d.path().join("foreign")).is_err());
        assert!(!key.exists());
        restore_key(&db, &d.path().join("backup")).unwrap();
        assert_eq!(std::fs::read(&key).unwrap(), original);
        assert_eq!(std::fs::read(&db).unwrap(), before);
    }
}
