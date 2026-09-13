#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::CardRecord,
    read_archive::{Budget, Finish, Plan, Status},
    store::Store,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
// Opaque deterministic storage fixtures, not actual TaskEvidence or guest execution proofs.
fn bytes(size: usize) -> Vec<u8> {
    let mut x = 0x1938abcdu32;
    (0..size)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}
fn plan(op: &str) -> Plan {
    Plan {
        operation_id: op.into(),
        subject: "query".into(),
        request_type: "test.request".into(),
        request: b"request".to_vec(),
        response_type: "test.response".into(),
        budget: Budget {
            max_parts: 32,
            max_bytes: 64 * 1024 * 1024,
        },
    }
}
fn finish(status: &Status) -> Finish {
    Finish {
        response: b"result".to_vec(),
        part_count: status.count,
        logical_bytes: status.logical_bytes,
        chain_sha256: status.chain_sha256,
        metadata_type: "test.metadata".into(),
        metadata: b"opaque manifest facts".to_vec(),
    }
}
fn setup() -> (tempfile::TempDir, PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("archive.db");
    let store = Store::open(&path, Default::default()).unwrap();
    (dir, path, store)
}
fn prepared(store: &mut Store) -> Status {
    store.begin_read_archive(&plan("operation")).unwrap();
    store
        .append_read_archive("query", "operation", 0, "test.bytes", b"first")
        .unwrap();
    store
        .append_read_archive("query", "operation", 1, "test.bytes", b"second")
        .unwrap()
}
fn publish(store: &mut Store) -> Status {
    let status = prepared(store);
    store
        .finish_read_archive_local_authorized(
            "query",
            "operation",
            &finish(&status),
            &[],
            || Ok(()),
        )
        .unwrap();
    store
        .lookup_read_archive("query", "operation")
        .unwrap()
        .unwrap()
}
fn count(path: &Path, table: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn pending_archive_is_not_published_read_and_exact_append_is_idempotent() {
    let (_dir, path, mut store) = setup();
    let initial = store.begin_read_archive(&plan("operation")).unwrap();
    assert_eq!(initial.count, 0);
    assert!(initial.root.is_none());
    let data = bytes(32769);
    let status = store
        .append_read_archive("query", "operation", 0, "test.bytes", &data)
        .unwrap();
    let retry = store
        .append_read_archive("query", "operation", 0, "test.bytes", &data)
        .unwrap();
    assert_eq!(retry.count, 1);
    assert!(retry.logical_bytes > data.len() as u64);
    assert_eq!(retry.chain_sha256, status.chain_sha256);
    assert!(store.lookup_read("query", "operation").unwrap().is_none());
    assert!(store.read_evidence("query", "operation").unwrap().is_none());
    assert_eq!(count(&path, "operations"), 0);
    assert_eq!(count(&path, "outbox"), 0);
    let part = store
        .read_archive_part("query", "operation", 0)
        .unwrap()
        .unwrap();
    assert_eq!(part.data(), data);
    assert_eq!(
        status.logical_bytes,
        (part.raw().len() + part.container().len()) as u64
    );
    assert_eq!(part.ordinal(), 0);
    assert_eq!(part.type_id(), "test.bytes");
    assert_eq!(part.digest(), <[u8; 32]>::from(Sha256::digest(part.raw())));
    assert!(
        store
            .lookup_read_archive("other", "operation")
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .read_archive_part("other", "operation", 0)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .read_archive_part("query", "operation", 1)
            .unwrap()
            .is_none()
    );
    for (ordinal, kind, value) in [
        (2, "test.bytes", data.as_slice()),
        (0, "other", data.as_slice()),
        (0, "test.bytes", b"changed".as_slice()),
    ] {
        assert!(
            store
                .append_read_archive("query", "operation", ordinal, kind, value)
                .is_err()
        );
    }
    let mut changed = plan("operation");
    changed.request.push(1);
    assert!(store.begin_read_archive(&changed).is_err());
    store.integrity_check().unwrap();
    drop(store);
    let reopened = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        reopened
            .read_archive_part("query", "operation", 0)
            .unwrap()
            .unwrap()
            .container(),
        part.container()
    );
    assert!(
        reopened
            .lookup_read("query", "operation")
            .unwrap()
            .is_none()
    );
}
#[test]
fn exact_publication_retry_still_checks_guard_and_completed_archive_cannot_be_changed_or_aborted() {
    let (_dir, path, mut store) = setup();
    let status = prepared(&mut store);
    let value = finish(&status);
    let receipt = store
        .finish_read_archive_local_authorized("query", "operation", &value, &[], || Ok(()))
        .unwrap();
    let original = store.lookup_read("query", "operation").unwrap().unwrap();
    assert_eq!(original.data().schema_version, 2);
    assert!(
        store
            .lookup_read_archive("query", "operation")
            .unwrap()
            .unwrap()
            .root
            .is_some()
    );
    assert_eq!(
        store
            .finish_read_archive_local_authorized("query", "operation", &value, &[], || Ok(()))
            .unwrap(),
        receipt
    );
    assert_eq!(
        store.finish_read_archive_local_authorized("query", "operation", &value, &[], || Err(
            Error::NotFound
        )),
        Err(Error::NotFound)
    );
    for field in 0..6 {
        let mut changed = finish(&status);
        match field {
            0 => changed.response.push(1),
            1 => changed.part_count -= 1,
            2 => changed.logical_bytes += 1,
            3 => changed.chain_sha256[0] ^= 1,
            4 => changed.metadata_type.push('x'),
            _ => changed.metadata.push(1),
        };
        assert!(
            store
                .finish_read_archive_local_authorized(
                    "query",
                    "operation",
                    &changed,
                    &[],
                    || Ok(())
                )
                .is_err()
        );
    }
    assert!(
        store
            .append_read_archive("query", "operation", 0, "test.bytes", b"first")
            .is_err()
    );
    assert_eq!(
        store.abort_read_archive("query", "operation"),
        Err(Error::OperationConflict)
    );
    assert!(
        store
            .begin_read_archive(&plan("operation"))
            .unwrap()
            .root
            .is_some()
    );
    assert_eq!(count(&path, "outbox"), 1);
    assert_eq!(count(&path, "cards"), 0);
    assert_eq!(
        store
            .lookup_read("query", "operation")
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
    store.integrity_check().unwrap();
}
#[test]
fn provisional_does_not_reserve_global_operation_and_cannot_replace_winning_content() {
    let (_dir, path, mut store) = setup();
    let status = prepared(&mut store);
    let mut winner = Store::open_existing(&path, Default::default()).unwrap();
    let card = CardRecord::new("card", "type", 1, "winner", vec![]).unwrap();
    winner.create_local("operation", &card).unwrap();
    assert_eq!(
        store.finish_read_archive_local_authorized(
            "query",
            "operation",
            &finish(&status),
            &[],
            || Ok(())
        ),
        Err(Error::OperationConflict)
    );
    assert!(store.lookup_read("query", "operation").unwrap().is_none());
    assert!(store.abort_read_archive("query", "operation").unwrap());
    assert_eq!(
        winner.card("card").unwrap().unwrap().encode(),
        card.encode()
    );
    assert_eq!(count(&path, "operations"), 1);
    assert_eq!(count(&path, "read_archive_parts"), 0);
    store.integrity_check().unwrap();
}
#[test]
fn rejected_guard_and_real_commit_unknown_leave_preparation_retryable_without_partial_publish() {
    for unknown in [false, true] {
        let (_dir, path, mut store) = setup();
        let status = prepared(&mut store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        if unknown {
            sql.execute_batch("CREATE TABLE review_parent(id INTEGER PRIMARY KEY);CREATE TABLE review_deferred(id INTEGER PRIMARY KEY,parent INTEGER REFERENCES review_parent(id) DEFERRABLE INITIALLY DEFERRED);CREATE TRIGGER review_failure AFTER INSERT ON outbox BEGIN INSERT INTO review_deferred(id,parent) VALUES(1,99);END;").unwrap();
        }
        let result = store.finish_read_archive_local_authorized(
            "query",
            "operation",
            &finish(&status),
            &[],
            || {
                if unknown {
                    Ok(())
                } else {
                    Err(Error::NotFound)
                }
            },
        );
        assert_eq!(
            result,
            Err(if unknown {
                Error::CommitUnknown
            } else {
                Error::NotFound
            })
        );
        assert!(store.lookup_read("query", "operation").unwrap().is_none());
        assert!(
            store
                .lookup_read_archive("query", "operation")
                .unwrap()
                .unwrap()
                .root
                .is_none()
        );
        assert_eq!(count(&path, "operations"), 0);
        assert_eq!(count(&path, "operation_read_archives"), 0);
        if unknown {
            sql.execute_batch(
                "DROP TRIGGER review_failure;DROP TABLE review_deferred;DROP TABLE review_parent;",
            )
            .unwrap();
        }
        store
            .finish_read_archive_local_authorized(
                "query",
                "operation",
                &finish(&status),
                &[],
                || Ok(()),
            )
            .unwrap();
        assert_eq!(
            store
                .read_archive_part("query", "operation", 0)
                .unwrap()
                .unwrap()
                .data(),
            b"first"
        );
        store.integrity_check().unwrap();
    }
}
#[test]
fn eight_mib_parts_exceed_old_single_operation_capacity_and_preserve_fixed_random_offsets() {
    let (_dir, path, mut store) = setup();
    let mut data = bytes(8 * 1024 * 1024);
    let expected = data.clone();
    let mut large_plan = plan("operation");
    large_plan.budget.max_bytes = 256 * 1024 * 1024;
    store.begin_read_archive(&large_plan).unwrap();
    let mut status = store
        .append_read_archive("query", "operation", 0, "test.random", &data)
        .unwrap();
    data[0] ^= 1;
    data[32768] ^= 1;
    for ordinal in 1..8 {
        status = store
            .append_read_archive("query", "operation", ordinal, "test.random", &expected)
            .unwrap();
    }
    let charged: u64 = (0..8)
        .map(|ordinal| {
            let p = store
                .read_archive_part("query", "operation", ordinal)
                .unwrap()
                .unwrap();
            (p.raw().len() + p.container().len()) as u64
        })
        .sum();
    assert_eq!(status.logical_bytes, charged);
    assert!(charged > 128 * 1024 * 1024);
    store
        .finish_read_archive_local_authorized(
            "query",
            "operation",
            &finish(&status),
            &[],
            || Ok(()),
        )
        .unwrap();
    for ordinal in [0, 3, 7] {
        let part = store
            .read_archive_part("query", "operation", ordinal)
            .unwrap()
            .unwrap();
        assert_eq!(part.data(), expected);
        for offset in [0, 1, 32767, 32768, 65535, 1048577, 8388607] {
            assert_eq!(part.data()[offset], expected[offset]);
        }
    }
    assert_eq!(count(&path, "cards"), 0);
    assert_eq!(count(&path, "task_evidence"), 0);
    store.integrity_check().unwrap();
}
#[test]
fn part_and_count_limits_reject_before_advancing_pending_chain() {
    let (_dir, _path, mut store) = setup();
    let mut p = plan("operation");
    p.budget.max_parts = 1;
    store.begin_read_archive(&p).unwrap();
    assert!(
        store
            .append_read_archive(
                "query",
                "operation",
                0,
                "test.bytes",
                &vec![1; 8 * 1024 * 1024 + 1]
            )
            .is_err()
    );
    assert_eq!(
        store
            .lookup_read_archive("query", "operation")
            .unwrap()
            .unwrap()
            .count,
        0
    );
    let status = store
        .append_read_archive("query", "operation", 0, "test.bytes", b"one")
        .unwrap();
    assert!(
        store
            .append_read_archive("query", "operation", 1, "test.bytes", b"two")
            .is_err()
    );
    assert_eq!(
        store
            .lookup_read_archive("query", "operation")
            .unwrap()
            .unwrap()
            .chain_sha256,
        status.chain_sha256
    );
}
#[test]
fn corrupt_missing_swapped_root_or_orphan_parts_are_not_repaired() {
    for damage in 0..5 {
        let (_dir, path, mut store) = setup();
        publish(&mut store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        sql.execute_batch(match damage{0=>"DELETE FROM read_archive_parts WHERE ordinal=1",1=>"UPDATE read_archive_parts SET payload=x'00' WHERE ordinal=0",2=>"UPDATE operation_read_archives SET root=zeroblob(32)",3=>"UPDATE read_archive_parts SET payload=(SELECT payload FROM read_archive_parts WHERE ordinal=1) WHERE ordinal=0",_=>"INSERT INTO read_archive_parts(operation_id,ordinal,payload) VALUES('orphan',0,x'00')"}).unwrap();
        assert!(store.integrity_check().is_err(), "damage {damage}");
        drop(store);
        assert!(
            Store::open_existing(&path, Default::default()).is_err(),
            "damage {damage}"
        );
    }
}
#[test]
fn backup_remains_self_contained_after_original_database_is_removed() {
    let (_dir, path, mut store) = setup();
    let status = publish(&mut store);
    let originals: (Vec<_>, Vec<_>) = (0..2)
        .map(|ordinal| {
            let part = store
                .read_archive_part("query", "operation", ordinal)
                .unwrap()
                .unwrap();
            (part.container().to_vec(), part.raw().to_vec())
        })
        .unzip();
    let backup = tempfile::tempdir().unwrap();
    let target = backup.path().join("snapshot.db");
    store.snapshot_to(&target, 16 * 1024 * 1024).unwrap();
    drop(store);
    std::fs::remove_file(path).unwrap();
    let restored = Store::open_existing(&target, Default::default()).unwrap();
    assert_eq!(
        restored
            .lookup_read_archive("query", "operation")
            .unwrap()
            .unwrap()
            .root,
        status.root
    );
    for ordinal in 0..2 {
        let part = restored
            .read_archive_part("query", "operation", ordinal)
            .unwrap()
            .unwrap();
        assert_eq!(part.container(), originals.0[ordinal as usize]);
        assert_eq!(part.raw(), originals.1[ordinal as usize]);
    }
    restored.integrity_check().unwrap();
}

fn downgrade_to_eleven(path: &Path) {
    rusqlite::Connection::open(path).unwrap().execute_batch("DROP TABLE read_captures; DROP TABLE operation_read_archives;DROP TABLE read_archive_parts;DROP TABLE read_archives;PRAGMA user_version=11;").unwrap();
}
fn version(path: &Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}
#[test]
fn migration_preserves_original_legacy_read_and_signature_and_rejects_old_format_masquerade() {
    use morrow_core::{
        audit::{self, SigningKey, TrustedLog},
        read_journal::Input,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    let key = SigningKey::from_bytes(&[44; 32]);
    let trust = TrustedLog {
        id: "archive-migration".into(),
        key: key.verifying_key(),
    };
    let mut store = Store::open_audited(&path, Default::default(), true, trust.clone()).unwrap();
    store
        .record_read_local_authorized(
            &Input {
                operation_id: "legacy".into(),
                subject: "query".into(),
                request_type: "req".into(),
                request: bytes(1024),
                response_type: "res".into(),
                response: bytes(2048),
            },
            &[],
            || Ok(()),
        )
        .unwrap();
    let original = store.lookup_read("query", "legacy").unwrap().unwrap();
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &store.pending(0, 10).unwrap()).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    store.seal_pending(&signed).unwrap();
    drop(store);
    downgrade_to_eleven(&path);
    let old = Store::open_read_only_audited(&path, trust.clone()).unwrap();
    assert_eq!(
        old.lookup_read("query", "legacy")
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
    drop(old);
    assert_eq!(version(&path), 11);
    let mut migrated =
        Store::open_audited(&path, Default::default(), false, trust.clone()).unwrap();
    assert_eq!(version(&path), 13);
    assert_eq!(
        migrated
            .lookup_read("query", "legacy")
            .unwrap()
            .unwrap()
            .raw(),
        original.raw()
    );
    assert_eq!(migrated.sealed_segment(1).unwrap().unwrap(), signed);
    publish(&mut migrated);
    drop(migrated);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version=11;")
        .unwrap();
    assert!(Store::open_audited(&path, Default::default(), false, trust).is_err());
    assert_eq!(version(&path), 11);
}
#[cfg(feature = "fault-injection")]
#[test]
fn read_archive_crash_child() {
    let Some(path) = std::env::var_os("MORROW_ARCHIVE_REVIEW_DB") else {
        return;
    };
    let mut store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
    match std::env::var("MORROW_ARCHIVE_REVIEW_MODE")
        .unwrap()
        .as_str()
    {
        "begin" => {
            store.begin_read_archive(&plan("operation")).unwrap();
        }
        "append" => {
            store
                .append_read_archive("query", "operation", 0, "test.bytes", b"crash-part")
                .unwrap();
        }
        "final" => {
            let status = store
                .lookup_read_archive("query", "operation")
                .unwrap()
                .unwrap();
            store
                .finish_read_archive_local_authorized(
                    "query",
                    "operation",
                    &finish(&status),
                    &[],
                    || Ok(()),
                )
                .unwrap();
        }
        "abort" => {
            store.abort_read_archive("query", "operation").unwrap();
        }
        "migrate" => {}
        _ => panic!("invalid private child mode"),
    }
}
#[cfg(feature = "fault-injection")]
fn child(path: &Path, mode: &str, point: &str) -> std::process::ExitStatus {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "read_archive_crash_child", "--nocapture"])
        .env("MORROW_ARCHIVE_REVIEW_DB", path)
        .env("MORROW_ARCHIVE_REVIEW_MODE", mode)
        .env("MORROW_TEST_CRASH_AT", point)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe child: {error}")
            }
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("archive child timeout: {point}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn append_crashes_preserve_whole_previous_or_new_chain_without_published_read() {
    for point in [
        "archive-after-part",
        "archive-before-append-commit",
        "archive-after-append-commit",
    ] {
        let (_dir, path, mut store) = setup();
        store.begin_read_archive(&plan("operation")).unwrap();
        drop(store);
        assert_eq!(child(&path, "append", point).code(), Some(86), "{point}");
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        let status = store
            .lookup_read_archive("query", "operation")
            .unwrap()
            .unwrap();
        assert_eq!(
            status.count,
            u32::from(point == "archive-after-append-commit")
        );
        assert!(store.lookup_read("query", "operation").unwrap().is_none());
        let appended = store
            .append_read_archive("query", "operation", 0, "test.bytes", b"crash-part")
            .unwrap();
        assert_eq!(appended.count, 1);
        store.integrity_check().unwrap();
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn final_crashes_never_publish_partial_archive_and_after_commit_retry_preserves_original() {
    for point in [
        "archive-after-publish",
        "archive-before-commit",
        "archive-after-commit",
    ] {
        let (_dir, path, mut store) = setup();
        let status = prepared(&mut store);
        drop(store);
        assert_eq!(child(&path, "final", point).code(), Some(86), "{point}");
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        let before = store.lookup_read("query", "operation").unwrap();
        assert_eq!(before.is_some(), point == "archive-after-commit");
        let receipt = store
            .finish_read_archive_local_authorized(
                "query",
                "operation",
                &finish(&status),
                &[],
                || Ok(()),
            )
            .unwrap();
        let original = store.lookup_read("query", "operation").unwrap().unwrap();
        if let Some(before) = before {
            assert_eq!(before.container(), original.container());
        }
        assert_eq!(receipt, original.receipt());
        assert_eq!(count(&path, "outbox"), 1);
        assert_eq!(count(&path, "read_archive_parts"), 2);
        store.integrity_check().unwrap();
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn abort_and_migration_crashes_are_atomic_and_do_not_delete_completed_archives() {
    for point in [
        "read-archive-abort-before-commit",
        "read-archive-abort-after-commit",
    ] {
        let (_dir, path, mut store) = setup();
        prepared(&mut store);
        drop(store);
        assert_eq!(child(&path, "abort", point).code(), Some(86), "{point}");
        let store = Store::open_existing(&path, Default::default()).unwrap();
        let expected = if point.ends_with("after-commit") {
            0
        } else {
            2
        };
        assert_eq!(count(&path, "read_archive_parts"), expected);
        assert_eq!(
            store
                .lookup_read_archive("query", "operation")
                .unwrap()
                .is_none(),
            expected == 0
        );
        store.integrity_check().unwrap();
    }
    for point in [
        "read-archive-migration-before-commit",
        "read-archive-migration-after-commit",
    ] {
        let (_dir, path, mut store) = setup();
        let card = CardRecord::new("card", "type", 1, "C", vec![]).unwrap();
        store.create_local("original", &card).unwrap();
        let events = store.pending(0, 10).unwrap();
        drop(store);
        downgrade_to_eleven(&path);
        assert_eq!(child(&path, "migrate", point).code(), Some(86), "{point}");
        assert_eq!(
            version(&path),
            if point.ends_with("after-commit") {
                12
            } else {
                11
            }
        );
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(store.pending(0, 10).unwrap(), events);
        assert_eq!(store.card("card").unwrap().unwrap().encode(), card.encode());
        store.integrity_check().unwrap();
    }
}

#[test]
fn actual_append_commit_unknown_rolls_back_part_and_manifest_together() {
    let (_dir, path, mut store) = setup();
    let initial = store.begin_read_archive(&plan("operation")).unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TABLE review_parent(id INTEGER PRIMARY KEY);CREATE TABLE review_deferred(id INTEGER PRIMARY KEY,parent INTEGER REFERENCES review_parent(id) DEFERRABLE INITIALLY DEFERRED);CREATE TRIGGER review_append_failure AFTER INSERT ON read_archive_parts BEGIN INSERT INTO review_deferred(id,parent) VALUES(1,99);END;").unwrap();
    assert_eq!(
        store.append_read_archive("query", "operation", 0, "test.bytes", b"part"),
        Err(Error::CommitUnknown)
    );
    assert_eq!(count(&path, "read_archive_parts"), 0);
    let status = store
        .lookup_read_archive("query", "operation")
        .unwrap()
        .unwrap();
    assert_eq!(status.chain_sha256, initial.chain_sha256);
    assert_eq!(status.count, 0);
    sql.execute_batch(
        "DROP TRIGGER review_append_failure;DROP TABLE review_deferred;DROP TABLE review_parent;",
    )
    .unwrap();
    assert_eq!(
        store
            .append_read_archive("query", "operation", 0, "test.bytes", b"part")
            .unwrap()
            .count,
        1
    );
    store.integrity_check().unwrap();
}
#[test]
fn abort_only_removes_its_own_pending_parts_and_keeps_other_completed_roots() {
    let (_dir, path, mut store) = setup();
    let published = publish(&mut store);
    store.begin_read_archive(&plan("unfinished")).unwrap();
    store
        .append_read_archive("query", "unfinished", 0, "test.bytes", b"private pending")
        .unwrap();
    assert!(!store.abort_read_archive("foreign", "unfinished").unwrap());
    assert!(store.abort_read_archive("query", "unfinished").unwrap());
    assert!(!store.abort_read_archive("query", "unfinished").unwrap());
    assert_eq!(
        store
            .lookup_read_archive("query", "operation")
            .unwrap()
            .unwrap()
            .root,
        published.root
    );
    assert_eq!(
        store
            .read_archive_part("query", "operation", 1)
            .unwrap()
            .unwrap()
            .data(),
        b"second"
    );
    assert_eq!(count(&path, "read_archive_parts"), 2);
    store.integrity_check().unwrap();
}
fn packed(magic: &[u8; 8], raw: &[u8]) -> Vec<u8> {
    let data = lz4_flex::block::compress(raw);
    let mut out = magic.to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend(data);
    out
}
#[test]
fn part_codec_preserves_unknown_bytes_and_rejects_duplicate_bad_hash_and_future_version() {
    use morrow_core::read_archive::{Part, proto};
    use prost::Message;
    let (_dir, _path, mut store) = setup();
    let initial = store.begin_read_archive(&plan("operation")).unwrap();
    let status = store
        .append_read_archive("query", "operation", 0, "test.bytes", b"part")
        .unwrap();
    let part = store
        .read_archive_part("query", "operation", 0)
        .unwrap()
        .unwrap();
    assert_eq!(part.previous_sha256(), initial.chain_sha256);
    assert_eq!(part.digest(), status.chain_sha256);
    let mut unknown = part.raw().to_vec();
    unknown.extend([0xa0, 0x06, 42]);
    let encoded = packed(b"MRWAPRT1", &unknown);
    let restored = Part::decode(&encoded).unwrap();
    assert_eq!(restored.raw(), unknown);
    assert_eq!(restored.container(), encoded);
    let mut duplicate = part.raw().to_vec();
    duplicate.extend([8, 1]);
    assert!(Part::decode(&packed(b"MRWAPRT1", &duplicate)).is_err());
    for damage in 0..4 {
        let mut value = proto::Part::decode(part.raw()).unwrap();
        match damage {
            0 => value.schema_version = 2,
            1 => value.data.push(1),
            2 => {
                value.previous_sha256.pop();
            }
            _ => value.ordinal = 65536,
        };
        assert!(Part::decode(&packed(b"MRWAPRT1", &value.encode_to_vec())).is_err());
    }
}

#[test]
fn sixty_four_mib_budget_charges_raw_and_container_not_just_data() {
    let (_dir, _path, mut store) = setup();
    let data = bytes(8 * 1024 * 1024);
    store.begin_read_archive(&plan("operation")).unwrap();
    let mut charged = 0;
    for ordinal in 0..3 {
        let status = store
            .append_read_archive("query", "operation", ordinal, "test.bytes", &data)
            .unwrap();
        let part = store
            .read_archive_part("query", "operation", ordinal)
            .unwrap()
            .unwrap();
        charged += (part.raw().len() + part.container().len()) as u64;
        assert_eq!(status.logical_bytes, charged);
    }
    assert!(charged < 64 * 1024 * 1024);
    assert_eq!(
        store.append_read_archive("query", "operation", 3, "test.bytes", &data),
        Err(Error::Limit)
    );
    assert_eq!(
        store
            .lookup_read_archive("query", "operation")
            .unwrap()
            .unwrap()
            .logical_bytes,
        charged
    );
}
#[test]
fn self_consistent_tampered_nonfinal_part_is_rejected_by_single_part_read() {
    use morrow_core::read_archive::proto;
    use prost::Message;
    let (_dir, path, mut store) = setup();
    publish(&mut store);
    let part = store
        .read_archive_part("query", "operation", 0)
        .unwrap()
        .unwrap();
    let mut value = proto::Part::decode(part.raw()).unwrap();
    value.data = b"different but well-encoded".to_vec();
    value.data_sha256 = Sha256::digest(&value.data).to_vec();
    let forged = packed(b"MRWAPRT1", &value.encode_to_vec());
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE read_archive_parts SET payload=?1 WHERE ordinal=0",
            [forged],
        )
        .unwrap();
    assert!(store.read_archive_part("query", "operation", 0).is_err());
}

#[cfg(feature = "fault-injection")]
#[test]
fn preparation_crashes_leave_absent_or_whole_unpublished_manifest_and_retry_is_exact() {
    for point in ["archive-after-begin", "archive-after-begin-commit"] {
        let (_dir, path, store) = setup();
        drop(store);
        assert_eq!(child(&path, "begin", point).code(), Some(86), "{point}");
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(
            store
                .lookup_read_archive("query", "operation")
                .unwrap()
                .is_some(),
            point == "archive-after-begin-commit"
        );
        let status = store.begin_read_archive(&plan("operation")).unwrap();
        assert_eq!(status.count, 0);
        assert!(status.root.is_none());
        assert!(store.lookup_read("query", "operation").unwrap().is_none());
        assert_eq!(count(&path, "outbox"), 0);
        store.integrity_check().unwrap();
    }
}

#[test]
fn published_archive_disguised_as_pending_cannot_be_deleted_by_abort() {
    use morrow_core::read_archive::proto;
    use prost::Message;
    let (_dir, path, mut store) = setup();
    publish(&mut store);
    let manifest = store
        .read_archive_manifest("query", "operation")
        .unwrap()
        .unwrap();
    let mut value = proto::Manifest::decode(manifest.raw()).unwrap();
    value.finalized = None;
    let disguised = packed(b"MRWAMNF1", &value.encode_to_vec());
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute(
        "UPDATE read_archives SET published=0,payload=?1 WHERE operation_id='operation'",
        [&disguised],
    )
    .unwrap();
    sql.execute_batch("DELETE FROM operation_read_archives WHERE operation_id='operation'")
        .unwrap();
    let original_parts: Vec<Vec<u8>> = sql
        .prepare("SELECT payload FROM read_archive_parts ORDER BY ordinal")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(store.abort_read_archive("query", "operation").is_err());
    assert_eq!(count(&path, "read_archive_parts"), 2);
    assert_eq!(count(&path, "read_archives"), 1);
    assert_eq!(count(&path, "operations"), 1);
    let after: Vec<Vec<u8>> = sql
        .prepare("SELECT payload FROM read_archive_parts ORDER BY ordinal")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(after, original_parts);
    let retained: Vec<u8> = sql
        .query_row(
            "SELECT payload FROM read_archives WHERE operation_id='operation'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, disguised);
}
#[test]
fn provisional_cleanup_preserves_winning_legacy_read_or_auxiliary_record() {
    use morrow_core::{read_journal::Input, records::Record};
    for legacy_read in [false, true] {
        let (_dir, path, mut store) = setup();
        prepared(&mut store);
        if legacy_read {
            store
                .record_read_local_authorized(
                    &Input {
                        operation_id: "operation".into(),
                        subject: "query".into(),
                        request_type: "request".into(),
                        request: b"legacy".to_vec(),
                        response_type: "response".into(),
                        response: b"saved".to_vec(),
                    },
                    &[],
                    || Ok(()),
                )
                .unwrap();
        } else {
            store
                .create_record_local("operation", &Record::workspace("workspace", "W").unwrap())
                .unwrap();
        }
        let before = store.pending(0, 10).unwrap();
        assert_eq!(before.len(), 1);
        assert!(store.abort_read_archive("query", "operation").unwrap());
        assert_eq!(store.pending(0, 10).unwrap(), before);
        assert_eq!(count(&path, "operations"), 1);
        assert_eq!(count(&path, "read_archive_parts"), 0);
        assert!(
            store
                .lookup_read_archive("query", "operation")
                .unwrap()
                .is_none()
        );
        if legacy_read {
            assert_eq!(
                store
                    .lookup_read("query", "operation")
                    .unwrap()
                    .unwrap()
                    .data()
                    .schema_version,
                1
            );
        }
        store.integrity_check().unwrap();
    }
}
