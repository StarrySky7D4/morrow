#![cfg(target_os = "linux")]
use morrow_core::linux_storage::{PrivateDirectory, SqlitePermissions};
use rusqlite::Connection;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
};

#[test]
fn sqlite_lock_probe() {
    let Some(path) = std::env::var_os("MORROW_SQLITE_LOCK_PROBE") else {
        return;
    };
    let db = Connection::open(path).unwrap();
    db.busy_timeout(std::time::Duration::ZERO).unwrap();
    std::process::exit(if db.execute_batch("BEGIN IMMEDIATE").is_err() {
        75
    } else {
        0
    });
}
fn contender(path: &Path) -> i32 {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "sqlite_lock_probe", "--nocapture"])
        .env("MORROW_SQLITE_LOCK_PROBE", path)
        .status()
        .unwrap()
        .code()
        .unwrap()
}
#[test]
fn metadata_checks_preserve_real_sqlite_process_record_lock() {
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = dir.path().join("store.db");
    let permissions = SqlitePermissions::prepare(&path, true).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "PRAGMA journal_mode=DELETE; CREATE TABLE t(x); BEGIN IMMEDIATE; INSERT INTO t VALUES(1)",
    )
    .unwrap();
    assert_eq!(
        contender(&path),
        75,
        "control: actual SQLite writer must block child"
    );
    permissions.verify().unwrap();
    assert_eq!(
        contender(&path),
        75,
        "metadata inspection must preserve the actual SQLite lock"
    );
    drop(permissions);
    assert_eq!(
        contender(&path),
        75,
        "dropping O_PATH guards must preserve SQLite lock"
    );
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        contender(&path),
        0,
        "control: released SQLite transaction admits child"
    );
}
#[test]
fn private_storage_rejects_modes_links_sidecars_and_replacement() {
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = dir.path().join("db");
    let permissions = SqlitePermissions::prepare(&path, true).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    assert!(permissions.verify().is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::hard_link(&path, dir.path().join("hard")).unwrap();
    assert!(permissions.verify().is_err());
    fs::remove_file(dir.path().join("hard")).unwrap();
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = dir.path().join(format!("db{suffix}"));
        symlink(&path, &sidecar).unwrap();
        assert!(permissions.verify().is_err());
        fs::remove_file(&sidecar).unwrap();
        fs::write(&sidecar, b"bad").unwrap();
        fs::set_permissions(&sidecar, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(permissions.verify().is_err());
        fs::remove_file(&sidecar).unwrap();
        fs::hard_link(&path, &sidecar).unwrap();
        assert!(permissions.verify().is_err());
        fs::remove_file(&sidecar).unwrap();
    }
    fs::rename(&path, dir.path().join("old")).unwrap();
    drop(
        PrivateDirectory::open(dir.path(), false)
            .unwrap()
            .create_new(Path::new("db"))
            .unwrap(),
    );
    assert!(permissions.verify().is_err());
    fs::remove_file(&path).unwrap();
    symlink(dir.path().join("old"), &path).unwrap();
    assert!(SqlitePermissions::prepare(&path, false).is_err());
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(PrivateDirectory::open(dir.path(), false).is_err());
}
#[test]
fn ancestor_symlinks_and_parent_components_are_rejected() {
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let private = dir.path().join("private");
    let anchored = PrivateDirectory::open(&private, true).unwrap();
    drop(anchored);
    symlink(&private, dir.path().join("alias")).unwrap();
    assert!(PrivateDirectory::open(&dir.path().join("alias"), false).is_err());
    assert!(PrivateDirectory::open(&private.join(".."), false).is_err());
}
#[test]
fn unsafe_sidecar_is_rejected_before_database_creation() {
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    fs::write(dir.path().join("new.db-wal"), b"unsafe").unwrap();
    assert!(SqlitePermissions::prepare(&dir.path().join("new.db"), true).is_err());
    assert!(!dir.path().join("new.db").exists());
    let anchored = PrivateDirectory::open(dir.path(), false).unwrap();
    assert!(anchored.create_new(Path::new("../escape")).is_err());
    assert!(anchored.open_read(Path::new("../escape")).is_err());
}
#[test]
fn metadata_checks_preserve_real_wal_writer_record_lock() {
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = dir.path().join("wal.db");
    let permissions = SqlitePermissions::prepare(&path, true).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "PRAGMA journal_mode=WAL; CREATE TABLE t(x); BEGIN IMMEDIATE; INSERT INTO t VALUES(1)",
    )
    .unwrap();
    assert_eq!(contender(&path), 75);
    permissions.verify().unwrap();
    assert_eq!(contender(&path), 75);
    drop(permissions);
    assert_eq!(contender(&path), 75);
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(contender(&path), 0);
}
#[test]
fn unavailable_private_store_routes_have_no_filesystem_or_sql_side_effects() {
    use morrow_core::{
        Error,
        audit::{SigningKey, TrustedLog},
        store::Store,
    };
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let trust = TrustedLog {
        id: "synthetic-private-store".into(),
        key: SigningKey::from_bytes(&[17; 32]).verifying_key(),
    };
    for existing in [false, true] {
        let path = dir.path().join(if existing {
            "existing.db"
        } else {
            "missing.db"
        });
        let mut files = vec![path.clone()];
        for suffix in ["-wal", "-shm", "-journal"] {
            files.push(dir.path().join(format!(
                "{}{suffix}",
                path.file_name().unwrap().to_string_lossy()
            )));
        }
        if existing {
            for file in &files {
                fs::write(file, b"immutable synthetic bytes").unwrap();
            }
        }
        let expected = Error::Invalid("protected Linux SQLite admission unavailable");
        assert!(
            matches!(Store::open_private(&path, Default::default(), true), Err(e) if e == expected)
        );
        assert!(
            matches!(Store::open_private_audited(&path, Default::default(), true, trust.clone()), Err(e) if e == expected)
        );
        assert!(
            matches!(Store::open_private_read_only_audited(&path, trust.clone()), Err(e) if e == expected)
        );
        assert!(matches!(Store::private_audit_binding_status(&path), Err(e) if e == expected));
        for file in &files {
            if existing {
                assert_eq!(fs::read(file).unwrap(), b"immutable synthetic bytes");
            } else {
                assert!(!file.exists());
            }
        }
    }
}
