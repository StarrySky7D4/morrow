#![cfg(target_os = "linux")]
//! Isolated local owned-VFS foundation checks. These are not Store/SecretService
//! or protected product qualification, power-loss evidence, or same-UID isolation.
use morrow_core::{
    Error,
    linux_storage::{GuardedSqliteConnection, PrivateDirectory, SqlitePermissions},
};
use rusqlite::Connection;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
};

fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn contender(path: &Path, read: bool) -> i32 {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "owned_vfs_process_probe", "--nocapture"])
        .env("MORROW_OWNED_VFS_PROBE", path)
        .env("MORROW_OWNED_VFS_READ", if read { "1" } else { "0" })
        .status()
        .unwrap()
        .code()
        .unwrap()
}
#[test]
fn owned_vfs_process_probe() {
    let Some(path) = std::env::var_os("MORROW_OWNED_VFS_PROBE") else {
        return;
    };
    let db = Connection::open(path).unwrap();
    db.busy_timeout(std::time::Duration::ZERO).unwrap();
    let result = if std::env::var("MORROW_OWNED_VFS_READ").as_deref() == Ok("1") {
        db.query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
            .map(|_| ())
    } else {
        db.execute_batch("BEGIN IMMEDIATE")
    };
    std::process::exit(match result {
        Ok(()) => 0,
        Err(error) if error.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy) => 75,
        Err(_) => 79,
    });
}

#[test]
fn owned_vfs_real_transaction_commit_rollback_and_private_journal() {
    let dir = directory();
    let path = dir.path().join("owned.db");
    let mut db = GuardedSqliteConnection::open(&path, true).unwrap();
    db.execute("CREATE TABLE t(x INTEGER)", []).unwrap();
    let value = db
        .transaction(|tx| {
            tx.execute("INSERT INTO t VALUES(?1)", [7])?;
            let journal = dir.path().join("owned.db-journal");
            assert_eq!(
                fs::metadata(journal).unwrap().permissions().mode() & 0o7777,
                0o600
            );
            tx.query_row("SELECT sum(x) FROM t", [], |row| row.get::<_, i64>(0))
        })
        .unwrap();
    assert_eq!(value, 7);
    assert!(!dir.path().join("owned.db-journal").exists());
    assert_eq!(
        db.transaction::<()>(|tx| {
            tx.execute("INSERT INTO t VALUES(99)", [])?;
            Err(Error::Integrity)
        }),
        Err(Error::Integrity)
    );
    assert_eq!(
        db.query_row("SELECT sum(x) FROM t", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        7
    );
    db.verify().unwrap();
    drop(db);
    let mut reopened = GuardedSqliteConnection::open(&path, false).unwrap();
    assert_eq!(
        reopened
            .query_row("SELECT sum(x) FROM t", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        7
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
        0o600
    );
}

#[test]
fn owned_vfs_lifetime_ofd_excludes_real_readers_writers_and_survives_other_close() {
    let dir = directory();
    let path = dir.path().join("lock.db");
    let mut db = GuardedSqliteConnection::open(&path, true).unwrap();
    db.execute("CREATE TABLE t(x)", []).unwrap();
    assert_eq!(contender(&path, true), 75);
    assert_eq!(contender(&path, false), 75);
    let metadata = SqlitePermissions::prepare(&path, false).unwrap();
    metadata.verify().unwrap();
    drop(metadata);
    drop(fs::File::open(&path).unwrap());
    assert_eq!(
        db.transaction::<()>(|_| Err(Error::Integrity)),
        Err(Error::Integrity)
    );
    assert_eq!(contender(&path, true), 75);
    assert_eq!(contender(&path, false), 75);
    drop(db);
    assert_eq!(contender(&path, true), 0);
    assert_eq!(contender(&path, false), 0);
}

#[test]
fn rejected_owned_open_preserves_existing_same_process_sqlite_posix_lock() {
    let dir = directory();
    let path = dir.path().join("legacy.db");
    drop(SqlitePermissions::prepare(&path, true).unwrap());
    let legacy = Connection::open(&path).unwrap();
    legacy
        .execute_batch("CREATE TABLE t(x); BEGIN IMMEDIATE; INSERT INTO t VALUES(3)")
        .unwrap();
    assert_eq!(contender(&path, false), 75);
    assert!(matches!(
        GuardedSqliteConnection::open(&path, false),
        Err(Error::StorageBusy)
    ));
    // The failed actual-fd admission retains its descriptor. Closing it here
    // would release legacy's fcntl lock and incorrectly admit this child.
    assert_eq!(contender(&path, false), 75);
    legacy.execute_batch("ROLLBACK").unwrap();
    drop(legacy);
    let mut owned = GuardedSqliteConnection::open(&path, false).unwrap();
    assert_eq!(
        owned
            .query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(contender(&path, false), 75);
    drop(owned);
    assert_eq!(contender(&path, false), 0);
}

#[test]
fn owned_cached_read_is_rejected_and_poison_stays_after_mode_restored() {
    let dir = directory();
    let path = dir.path().join("cache.db");
    let mut db = GuardedSqliteConnection::open(&path, true).unwrap();
    db.execute("CREATE TABLE t(x)", []).unwrap();
    db.execute("INSERT INTO t VALUES(4)", []).unwrap();
    assert_eq!(
        db.query_row("SELECT x FROM t", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        4
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    assert!(
        db.query_row("SELECT x FROM t", [], |row| row.get::<_, i64>(0))
            .is_err()
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        db.query_row("SELECT x FROM t", [], |row| row.get::<_, i64>(0))
            .is_err()
    );
    assert!(db.execute("INSERT INTO t VALUES(5)", []).is_err());
}

#[test]
fn owned_post_read_guard_withholds_result_changed_inside_row_callback() {
    let dir = directory();
    let path = dir.path().join("postread.db");
    let mut db = GuardedSqliteConnection::open(&path, true).unwrap();
    let result = db.query_row("SELECT 123", [], |row| {
        let value = row.get::<_, i64>(0)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        Ok(value)
    });
    assert!(matches!(result, Err(Error::Invalid(_))));
}

#[test]
fn owned_transaction_checks_object_again_immediately_before_commit() {
    let dir = directory();
    let path = dir.path().join("precommit.db");
    let mut db = GuardedSqliteConnection::open(&path, true).unwrap();
    db.execute("CREATE TABLE t(x)", []).unwrap();
    let result = db.transaction(|tx| {
        tx.execute("INSERT INTO t VALUES(77)", [])?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        Ok(77)
    });
    assert!(matches!(result, Err(Error::Invalid(_))));
    assert!(db.verify().is_err());
}

#[test]
fn owned_post_open_main_and_directory_replacements_hardlinks_and_sidecars_reject() {
    for attack in ["main", "directory", "hardlink", "journal", "wal", "shm"] {
        let outer = directory();
        let dir = outer.path().join("private");
        let private = PrivateDirectory::open(&dir, true).unwrap();
        drop(private);
        let path = dir.join("db");
        let mut db = GuardedSqliteConnection::open(&path, true).unwrap();
        db.execute("CREATE TABLE t(x)", []).unwrap();
        match attack {
            "main" => {
                fs::rename(&path, dir.join("old")).unwrap();
                drop(
                    PrivateDirectory::open(&dir, false)
                        .unwrap()
                        .create_new(Path::new("db"))
                        .unwrap(),
                );
            }
            "directory" => {
                fs::rename(&dir, outer.path().join("old")).unwrap();
                drop(PrivateDirectory::open(&dir, true).unwrap());
            }
            "hardlink" => fs::hard_link(&path, dir.join("alias")).unwrap(),
            suffix => {
                drop(
                    PrivateDirectory::open(&dir, false)
                        .unwrap()
                        .create_new(Path::new(&format!("db-{suffix}")))
                        .unwrap(),
                );
            }
        }
        assert!(
            db.query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
                .is_err(),
            "{attack}"
        );
        assert!(
            db.execute("INSERT INTO t VALUES(8)", []).is_err(),
            "{attack}"
        );
    }
}

#[test]
fn owned_profile_refuses_wal_headers_and_sidecars_without_downgrade() {
    let dir = directory();
    let path = dir.path().join("wal.db");
    drop(SqlitePermissions::prepare(&path, true).unwrap());
    let ordinary = Connection::open(&path).unwrap();
    ordinary
        .execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE t(x); INSERT INTO t VALUES(1)")
        .unwrap();
    drop(ordinary);
    let before = fs::read(&path).unwrap();
    assert!(GuardedSqliteConnection::open(&path, false).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    for suffix in ["wal", "shm"] {
        let other = directory();
        let db = other.path().join("new.db");
        drop(
            PrivateDirectory::open(other.path(), false)
                .unwrap()
                .create_new(Path::new(&format!("new.db-{suffix}")))
                .unwrap(),
        );
        assert!(GuardedSqliteConnection::open(&db, true).is_err());
        assert!(
            !db.exists(),
            "unsupported sidecar admission must precede main creation"
        );
    }
}

#[test]
fn owned_profile_denies_attachment_profile_changes_and_unsafe_admission() {
    let dir = directory();
    let path = dir.path().join("main.db");
    let mut db = GuardedSqliteConnection::open(&path, true).unwrap();
    assert!(
        db.execute(
            "ATTACH DATABASE ?1 AS other",
            [dir.path().join("other.db").to_str().unwrap()]
        )
        .is_err()
    );
    assert!(!dir.path().join("other.db").exists());
    for pragma in [
        "PRAGMA journal_mode=WAL",
        "PRAGMA journal_mode=OFF",
        "PRAGMA journal_mode=MEMORY",
        "PRAGMA synchronous=OFF",
        "PRAGMA mmap_size=1048576",
        "PRAGMA temp_store=FILE",
    ] {
        assert!(db.execute(pragma, []).is_err(), "{pragma}");
    }
    // SQLite marks even a bare journal_mode PRAGMA potentially writable.
    assert!(
        db.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .is_err()
    );
    db.verify().unwrap();
    db.execute("CREATE TABLE t(x)", []).unwrap();
    drop(db);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(GuardedSqliteConnection::open(&path, false).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::rename(&path, dir.path().join("original")).unwrap();
    symlink(dir.path().join("original"), &path).unwrap();
    assert!(GuardedSqliteConnection::open(&path, false).is_err());
}

#[test]
fn owned_public_sql_cannot_escape_typed_transaction_or_mutate_via_query() {
    let dir = directory();
    let path = dir.path().join("control.db");
    let mut db = GuardedSqliteConnection::open(&path, true).unwrap();
    db.execute("CREATE TABLE t(x)", []).unwrap();
    for control in [
        "COMMIT",
        "END",
        "ROLLBACK",
        "BEGIN",
        "BEGIN IMMEDIATE",
        "SAVEPOINT escape",
        "RELEASE escape",
        "ROLLBACK TO escape",
    ] {
        assert!(
            db.execute(control, []).is_err(),
            "public execute: {control}"
        );
        assert!(
            db.query_row(control, [], |row| row.get::<_, i64>(0))
                .is_err(),
            "public query: {control}"
        );
        let outcome = db.transaction::<()>(|tx| {
            tx.execute("INSERT INTO t VALUES(7)", [])?;
            assert!(
                tx.execute(control, []).is_err(),
                "transaction execute: {control}"
            );
            assert!(
                tx.query_row(control, [], |row| row.get::<_, i64>(0))
                    .is_err(),
                "transaction query: {control}"
            );
            Err(Error::Integrity)
        });
        assert_eq!(outcome, Err(Error::Integrity));
        assert_eq!(
            db.query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0,
            "{control}"
        );
    }
    let callback_ran = std::cell::Cell::new(false);
    assert!(
        db.query_row("INSERT INTO t VALUES(33) RETURNING x", [], |row| {
            callback_ran.set(true);
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            row.get::<_, i64>(0)
        })
        .is_err()
    );
    assert!(
        !callback_ran.get(),
        "write-returning must reject before stepping or row callback"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn owned_transaction_panic_requests_real_rollback_and_preserves_connection() {
    let dir = directory();
    let path = dir.path().join("panic.db");
    let mut db = GuardedSqliteConnection::open(&path, true).unwrap();
    db.execute("CREATE TABLE t(x)", []).unwrap();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<(), Error> = db.transaction(|tx| {
            tx.execute("INSERT INTO t VALUES(55)", [])?;
            panic!("isolated transaction panic");
        });
    }));
    assert!(outcome.is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(!dir.path().join("panic.db-journal").exists());
    db.execute("INSERT INTO t VALUES(7)", []).unwrap();
    assert_eq!(
        db.query_row("SELECT x FROM t", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        7
    );
}

#[test]
fn owned_vfs_crash_probe() {
    let Some(path) = std::env::var_os("MORROW_OWNED_VFS_CRASH") else {
        return;
    };
    let mut db = GuardedSqliteConnection::open(Path::new(&path), false).unwrap();
    let _: Result<(), Error> = db.transaction(|tx| {
        for index in 0..64 {
            tx.execute("INSERT INTO t VALUES(?1,zeroblob(131072))", [index])?;
        }
        // Real process termination after writes exceed the default pager cache.
        // No Drop/rollback, no injected fake file or fabricated journal bytes.
        std::process::exit(86);
    });
    std::process::exit(87);
}

#[test]
fn owned_vfs_real_hot_journal_recovery_after_process_exit() {
    let dir = directory();
    let path = dir.path().join("recover.db");
    let mut db = GuardedSqliteConnection::open(&path, true).unwrap();
    db.execute("CREATE TABLE t(x INTEGER, payload BLOB)", [])
        .unwrap();
    db.execute("INSERT INTO t VALUES(100,zeroblob(16))", [])
        .unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "owned_vfs_crash_probe", "--nocapture"])
        .env("MORROW_OWNED_VFS_CRASH", &path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));
    assert!(
        fs::metadata(dir.path().join("recover.db-journal"))
            .unwrap()
            .len()
            > 512
    );
    assert_ne!(
        fs::read(&path).unwrap(),
        before,
        "pager must have spilled real uncommitted pages"
    );
    let mut restored = GuardedSqliteConnection::open(&path, false).unwrap();
    assert_eq!(
        restored
            .query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        restored
            .query_row("SELECT x FROM t", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        100
    );
    assert_eq!(
        restored
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(!dir.path().join("recover.db-journal").exists());
    drop(restored);
    assert_eq!(fs::read(&path).unwrap(), before);
}
