#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    blob_stage::{Command, MAX_STAGE_BYTES, RESERVED_HISTORY_BYTES},
    store::{EventBudget, SCHEMA_VERSION, Store},
};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};

fn command(id: &str, bytes: &[u8]) -> Command {
    Command {
        owner_domain: [37; 32],
        operation_id: id.into(),
        byte_length: bytes.len() as u64,
        sha256: Sha256::digest(bytes).into(),
    }
}
struct NeverRead;
impl Read for NeverRead {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("must not consume replacement source")
    }
}
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("blob-stage.db");
    let store = Store::open(&db, EventBudget::default()).unwrap();
    (dir, db, store)
}
#[test]
fn original_store_binding_refuses_a_second_connection_and_does_not_keep_the_store_alive() {
    let (_dir, path, store) = fixture();
    let binding = store.blob_stage_store_binding();
    store.validate_blob_stage_store_binding(&binding).unwrap();
    let second = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert!(second.validate_blob_stage_store_binding(&binding).is_err());
    binding.check_live().unwrap();
    drop(store);
    assert!(binding.check_live().is_err());
    assert!(second.validate_blob_stage_store_binding(&binding).is_err());
}

struct RevokeAtEof<'a> {
    bytes: Cursor<&'a [u8]>,
    live: &'a std::cell::Cell<bool>,
}
impl Read for RevokeAtEof<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let size = self.bytes.read(out)?;
        if size == 0 && !out.is_empty() {
            self.live.set(false);
        }
        Ok(size)
    }
}
fn live_check(live: &std::cell::Cell<bool>) -> morrow_core::Result<()> {
    if live.get() {
        Ok(())
    } else {
        Err(Error::Invalid("test revoked"))
    }
}

#[test]
fn live_guard_refuses_before_reading_or_consuming_stage_capacity() {
    let (_dir, _path, mut store) = fixture();
    let cmd = command("denied", b"bytes");
    let error = store.stage_blob_operation_local_authorized(&mut NeverRead, &cmd, 10, || {
        Err(Error::Invalid("test revoked"))
    });
    assert_eq!(error, Err(Error::Invalid("test revoked")));
    assert!(store.inspect_blob_stage_local(&cmd).unwrap().is_none());
    assert!(store.list_blobs_local("", 128).unwrap().is_empty());
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    let next = command("next", b"next");
    store
        .stage_blob_operation_local(&mut Cursor::new(b"next"), &next, 1)
        .unwrap();
    store.integrity_check().unwrap();
}

#[test]
fn final_guard_after_actual_eof_rolls_back_empty_multichunk_and_deduplicated_stages() {
    for (bytes, seed) in [
        (vec![], false),
        (vec![13; 150_017], false),
        (vec![13; 150_017], true),
    ] {
        let (_dir, path, mut store) = fixture();
        let original = if seed {
            Some(
                store
                    .stage_blob_operation_local(
                        &mut Cursor::new(&bytes),
                        &command("seed", &bytes),
                        1,
                    )
                    .unwrap(),
            )
        } else {
            None
        };
        let live = std::cell::Cell::new(true);
        let mut source = RevokeAtEof {
            bytes: Cursor::new(bytes.as_slice()),
            live: &live,
        };
        let cmd = command("vetoed", &bytes);
        assert_eq!(
            store
                .stage_blob_operation_local_authorized(&mut source, &cmd, 50, || live_check(&live)),
            Err(Error::Invalid("test revoked")),
        );
        assert!(!live.get());
        assert!(store.inspect_blob_stage_local(&cmd).unwrap().is_none());
        assert_eq!(
            store.list_blobs_local("", 128).unwrap().len(),
            usize::from(seed)
        );
        if let Some(blob) = &original {
            assert_eq!(store.retire_blob_local(&blob.id, 2), Err(Error::Retained));
        }
        drop(store);
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        assert!(store.inspect_blob_stage_local(&cmd).unwrap().is_none());
        // A different operation proves rollback of the failed clock advance.
        store
            .stage_blob_operation_local(&mut Cursor::new(b"later"), &command("later", b"later"), 2)
            .unwrap();
        store.integrity_check().unwrap();
    }
}

#[test]
fn guard_panic_after_actual_eof_rolls_back_without_losing_original_store() {
    let (_dir, path, mut store) = fixture();
    let live = std::cell::Cell::new(true);
    let mut source = RevokeAtEof {
        bytes: Cursor::new(b"value".as_slice()),
        live: &live,
    };
    let cmd = command("panic", b"value");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        store.stage_blob_operation_local_authorized(&mut source, &cmd, 10, || {
            assert!(live.get(), "injected final veto panic");
            Ok(())
        })
    }));
    assert!(result.is_err());
    assert!(store.inspect_blob_stage_local(&cmd).unwrap().is_none());
    assert!(store.list_blobs_local("", 128).unwrap().is_empty());
    store.integrity_check().unwrap();
    drop(store);
    Store::open_existing(&path, Default::default())
        .unwrap()
        .integrity_check()
        .unwrap();
}

#[test]
fn original_history_and_absence_are_not_delivery_authority_after_guard_revocation() {
    let (_dir, _path, mut store) = fixture();
    let cmd = command("original", b"original");
    let blob = store
        .stage_blob_operation_local(&mut Cursor::new(b"original"), &cmd, 1)
        .unwrap();
    let mut calls = 0;
    assert_eq!(
        store.stage_blob_operation_local_authorized(&mut NeverRead, &cmd, -1, || {
            calls += 1;
            if calls == 1 {
                Ok(())
            } else {
                Err(Error::Invalid("test revoked"))
            }
        }),
        Err(Error::Invalid("test revoked"))
    );
    assert_eq!(calls, 2);
    for query in [&cmd, &command("absent", b"absent")] {
        let mut calls = 0;
        assert_eq!(
            store.inspect_blob_stage_local_authorized(query, || {
                calls += 1;
                if calls == 1 {
                    Ok(())
                } else {
                    Err(Error::Invalid("test revoked"))
                }
            }),
            Err(Error::Invalid("test revoked"))
        );
        assert_eq!(calls, 2);
    }
    assert_eq!(
        store
            .inspect_blob_stage_local(&cmd)
            .unwrap()
            .unwrap()
            .blob(),
        &blob
    );
    store.integrity_check().unwrap();
}

#[test]
fn final_release_guard_keeps_original_retention_history_and_clock_until_commit() {
    let (_dir, path, mut store) = fixture();
    let cmd = command("original", b"original");
    let blob = store
        .stage_blob_operation_local(&mut Cursor::new(b"original"), &cmd, 1)
        .unwrap();
    for panic in [false, true] {
        let mut calls = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.release_blob_stage_local_authorized(&cmd, 100, || {
                calls += 1;
                if calls == 1 {
                    return Ok(());
                }
                assert!(!panic, "injected release guard panic");
                Err(Error::Invalid("test revoked"))
            })
        }));
        if panic {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), Err(Error::Invalid("test revoked")));
        }
        assert_eq!(calls, 2);
        assert!(
            !store
                .inspect_blob_stage_local(&cmd)
                .unwrap()
                .unwrap()
                .is_released()
        );
        assert_eq!(store.retire_blob_local(&blob.id, 2), Err(Error::Retained));
    }
    drop(store);
    let mut store = Store::open_existing(&path, Default::default()).unwrap();
    let released = store.release_blob_stage_local(&cmd, 2).unwrap();
    assert!(released.is_released());
    let mut calls = 0;
    assert_eq!(
        store.release_blob_stage_local_authorized(&cmd, -1, || {
            calls += 1;
            if calls == 1 {
                Ok(())
            } else {
                Err(Error::Invalid("test revoked"))
            }
        }),
        Err(Error::Invalid("test revoked"))
    );
    assert_eq!(calls, 2);
    assert_eq!(
        store.inspect_blob_stage_local(&cmd).unwrap().unwrap(),
        released
    );
    store.integrity_check().unwrap();
}

#[test]
fn multichunk_reopen_retry_returns_exact_original_without_reading_or_clock_mutation() {
    let (_dir, path, mut store) = fixture();
    let bytes = vec![91; 150_017];
    let cmd = command("original", &bytes);
    let blob = store
        .stage_blob_operation_local(&mut Cursor::new(&bytes), &cmd, 7)
        .unwrap();
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    assert_eq!(store.retire_blob_local(&blob.id, 8), Err(Error::Retained));
    drop(store);
    let mut store = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        store
            .stage_blob_operation_local(&mut NeverRead, &cmd, -1)
            .unwrap(),
        blob
    );
    let record = store.inspect_blob_stage_local(&cmd).unwrap().unwrap();
    assert!(!record.is_released());
    assert_eq!(record.staged_at_unix_ms(), 7);
    let mut actual = vec![];
    store.export_blob_local(&blob.id, &mut actual).unwrap();
    assert_eq!(actual, bytes);
    store.integrity_check().unwrap();
}
#[test]
fn release_tombstone_survives_reopen_gc_and_rejects_old_stage() {
    let (_dir, path, mut store) = fixture();
    let cmd = command("old", b"payload");
    let blob = store
        .stage_blob_operation_local(&mut Cursor::new(b"payload"), &cmd, 1)
        .unwrap();
    let done = store.release_blob_stage_local(&cmd, 2).unwrap();
    assert!(done.is_released());
    assert_eq!(store.release_blob_stage_local(&cmd, -10).unwrap(), done);
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    assert_eq!(
        store.collect_retired_local(60_002, 60_000).unwrap(),
        vec![blob.id.clone()]
    );
    drop(store);
    let mut store = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(store.inspect_blob_stage_local(&cmd).unwrap().unwrap(), done);
    assert_eq!(
        store.stage_blob_operation_local(&mut NeverRead, &cmd, 60_003),
        Err(Error::OperationConflict)
    );
    assert_eq!(store.blob_info_local(&blob.id), Err(Error::NotFound));
    store.integrity_check().unwrap();
}
#[test]
fn foreign_command_cannot_read_or_release_original_but_distinct_domain_is_independent() {
    let (_dir, _path, mut store) = fixture();
    let cmd = command("operation", b"one");
    let blob = store
        .stage_blob_operation_local(&mut Cursor::new(b"one"), &cmd, 1)
        .unwrap();
    let conflict = command("operation", b"two");
    assert_eq!(
        store.stage_blob_operation_local(&mut NeverRead, &conflict, 2),
        Err(Error::OperationConflict)
    );
    assert_eq!(
        store.inspect_blob_stage_local(&conflict),
        Err(Error::OperationConflict)
    );
    assert_eq!(
        store.release_blob_stage_local(&conflict, 2),
        Err(Error::OperationConflict)
    );
    let mut other = cmd.clone();
    other.owner_domain = [38; 32];
    assert!(store.inspect_blob_stage_local(&other).unwrap().is_none());
    assert_eq!(
        store.release_blob_stage_local(&other, 2),
        Err(Error::NotFound)
    );
    assert_eq!(
        store
            .stage_blob_operation_local(&mut Cursor::new(b"one"), &other, 2)
            .unwrap(),
        blob
    );
    store.release_blob_stage_local(&cmd, 3).unwrap();
    assert_eq!(store.retire_blob_local(&blob.id, 4), Err(Error::Retained));
    store.release_blob_stage_local(&other, 4).unwrap();
    store.integrity_check().unwrap();
}
#[test]
fn admission_reserves_release_history_before_consuming_reader_and_never_refunds_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("small.db");
    let budget = EventBudget {
        max_count: 1024,
        max_bytes: RESERVED_HISTORY_BYTES,
    };
    let mut store = Store::open(&path, budget).unwrap();
    let cmd = command("one", b"");
    store
        .stage_blob_operation_local(&mut Cursor::new([]), &cmd, 1)
        .unwrap();
    assert_eq!(
        store.stage_blob_operation_local(&mut NeverRead, &command("two", b""), 2),
        Err(Error::EventCapacity)
    );
    store.release_blob_stage_local(&cmd, 2).unwrap();
    assert_eq!(
        store.stage_blob_operation_local(&mut NeverRead, &command("three", b""), 3),
        Err(Error::EventCapacity)
    );
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
}
#[test]
fn bad_reader_digest_length_and_empty_descriptor_leave_no_receipt_or_blob() {
    let (_dir, _path, mut store) = fixture();
    for (data, cmd) in [
        (b"short".as_slice(), command("short", b"longer")),
        (b"longer".as_slice(), command("long", b"tiny")),
        (b"wrong".as_slice(), command("hash", b"exact")),
    ] {
        assert!(
            store
                .stage_blob_operation_local(&mut Cursor::new(data), &cmd, 1)
                .is_err()
        );
        assert!(store.inspect_blob_stage_local(&cmd).unwrap().is_none());
    }
    let mut invalid = command("too-large", b"");
    invalid.byte_length = MAX_STAGE_BYTES + 1;
    assert_eq!(
        store.stage_blob_operation_local(&mut NeverRead, &invalid, 1),
        Err(Error::Limit)
    );
    invalid = command("zero-domain", b"");
    invalid.owner_domain = [0; 32];
    assert!(
        store
            .stage_blob_operation_local(&mut NeverRead, &invalid, 1)
            .is_err()
    );
    assert!(store.list_blobs_local("", 128).unwrap().is_empty());
    assert_eq!(store.pending_usage().unwrap().1, 0);
}
#[test]
fn clock_regression_does_not_release_or_change_history() {
    let (_dir, _path, mut store) = fixture();
    let cmd = command("clock", b"x");
    store
        .stage_blob_operation_local(&mut Cursor::new(b"x"), &cmd, 10)
        .unwrap();
    store
        .stage_blob(&mut Cursor::new(b"y"), 1, None, 20)
        .unwrap();
    assert!(store.release_blob_stage_local(&cmd, 15).is_err());
    assert!(
        !store
            .inspect_blob_stage_local(&cmd)
            .unwrap()
            .unwrap()
            .is_released()
    );
    assert_eq!(
        store
            .release_blob_stage_local(&cmd, 21)
            .unwrap()
            .released_at_unix_ms(),
        Some(21)
    );
}
#[test]
fn mismatched_active_index_and_modified_payload_fail_closed() {
    for corruption in [
        "UPDATE blob_stage_operations SET active_blob=NULL",
        "UPDATE blob_stage_history SET payload=x'00'",
        "UPDATE blobs SET payload=zeroblob(size)",
    ] {
        let (_dir, path, mut store) = fixture();
        let cmd = command("corrupt", b"data");
        store
            .stage_blob_operation_local(&mut Cursor::new(b"data"), &cmd, 1)
            .unwrap();
        drop(store);
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(corruption)
            .unwrap();
        assert!(
            Store::open_existing(&path, Default::default()).is_err(),
            "{corruption}"
        );
    }
}
#[test]
fn additive_v25_migration_keeps_existing_blob_bytes_and_creates_no_operation() {
    let (_dir, path, mut store) = fixture();
    let blob = store
        .stage_blob(&mut Cursor::new(b"prior"), 5, None, 1)
        .unwrap();
    drop(store);
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "DROP TABLE blob_stage_history; DROP TABLE blob_stage_operations; PRAGMA user_version=25;",
    )
    .unwrap();
    drop(raw);
    let store = Store::open_existing(&path, Default::default()).unwrap();
    let mut out = vec![];
    store.export_blob_local(&blob.id, &mut out).unwrap();
    assert_eq!(out, b"prior");
    assert!(
        store
            .inspect_blob_stage_local(&command("absent", b"prior"))
            .unwrap()
            .is_none()
    );
    store.integrity_check().unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}
#[test]
fn terminal_history_cannot_be_reopened_by_replacing_only_the_active_index() {
    let (_dir, path, mut store) = fixture();
    let cmd = command("terminal", b"x");
    let blob = store
        .stage_blob_operation_local(&mut Cursor::new(b"x"), &cmd, 1)
        .unwrap();
    store.release_blob_stage_local(&cmd, 2).unwrap();
    drop(store);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute("UPDATE blob_stage_operations SET active_blob=?1", [blob.id])
        .unwrap();
    assert!(Store::open_existing(&path, Default::default()).is_err());
}

#[test]
fn operation_count_exhaustion_is_not_reset_by_release() {
    let (_dir, _path, mut store) = fixture();
    for i in 0..morrow_core::blob_stage::MAX_OPERATIONS {
        store
            .stage_blob_operation_local(
                &mut Cursor::new([]),
                &command(&format!("slot-{i}"), b""),
                1,
            )
            .unwrap();
    }
    assert_eq!(
        store.stage_blob_operation_local(&mut NeverRead, &command("overflow", b""), 2),
        Err(Error::Limit)
    );
    store
        .release_blob_stage_local(&command("slot-0", b""), 2)
        .unwrap();
    assert_eq!(
        store.stage_blob_operation_local(&mut NeverRead, &command("still-full", b""), 3),
        Err(Error::Limit)
    );
    store.integrity_check().unwrap();
}

#[test]
fn stage_history_and_existing_content_share_the_same_writer_quota() {
    use morrow_core::content::CardRecord;
    let (_dir, path, mut store) = fixture();
    let card = CardRecord::new("one", "example.note", 1, "title", b"body".to_vec()).unwrap();
    store.create_local("first-event", &card).unwrap();
    let original_bytes = store.pending_usage().unwrap().1;
    drop(store);
    let budget = EventBudget {
        max_count: 1024,
        max_bytes: original_bytes + RESERVED_HISTORY_BYTES,
    };
    let mut store = Store::open_existing(&path, budget).unwrap();
    let cmd = command("reserved", b"");
    store
        .stage_blob_operation_local(&mut Cursor::new([]), &cmd, 1)
        .unwrap();
    let mut other = Store::open_existing(&path, budget).unwrap();
    let second = CardRecord::new("two", "example.note", 1, "title", b"body".to_vec()).unwrap();
    assert_eq!(
        other.create_local("second-event", &second),
        Err(Error::EventCapacity)
    );
    store.release_blob_stage_local(&cmd, 2).unwrap();
    assert_eq!(
        other.create_local("second-event", &second),
        Err(Error::EventCapacity)
    );
    assert_eq!(other.pending_usage().unwrap().1, original_bytes);
    other.integrity_check().unwrap();
}

#[test]
fn concurrent_exact_stage_on_two_connections_consumes_one_original_reader() {
    use std::sync::{Arc, Barrier};
    let (_dir, path, store) = fixture();
    drop(store);
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = vec![];
    for _ in 0..2 {
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            struct Source {
                bytes: Cursor<&'static [u8]>,
                reads: u32,
            }
            impl Read for Source {
                fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                    self.reads += 1;
                    self.bytes.read(out)
                }
            }
            let mut source = Source {
                bytes: Cursor::new(b"same".as_slice()),
                reads: 0,
            };
            barrier.wait();
            let blob = store.stage_blob_operation_local(&mut source, &command("race", b"same"), 1);
            (blob, source.reads)
        }));
    }
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let committed = results.iter().find_map(|r| r.0.as_ref().ok()).unwrap();
    for (result, reads) in &results {
        match result {
            Ok(blob) => assert_eq!(blob, committed),
            // The original Store deliberately has a zero busy timeout. A
            // concurrent writer may refuse admission; it must not read bytes.
            Err(Error::StorageBusy) => assert_eq!(*reads, 0),
            Err(error) => panic!("unexpected stage refusal: {error}"),
        }
    }
    assert_eq!(results.iter().filter(|r| r.1 > 0).count(), 1);
    let store = Store::open_existing(&path, Default::default()).unwrap();
    let fact = store
        .inspect_blob_stage_local(&command("race", b"same"))
        .unwrap()
        .unwrap();
    assert_eq!(fact.blob(), committed);
    assert!(!fact.is_released());
    assert_eq!(store.list_blobs_local("", 128).unwrap().len(), 1);
    store.integrity_check().unwrap();
}

#[cfg(feature = "fault-injection")]
#[test]
fn v26_migration_abrupt_exit_preserves_prior_blob_and_recovers_atomically() {
    for (point, expected) in [
        ("blob-stage-migration-before-commit", 25),
        ("blob-stage-migration-after-commit", 26),
    ] {
        let (_dir, path, mut store) = fixture();
        let blob = store
            .stage_blob(&mut Cursor::new(b"prior"), 5, None, 1)
            .unwrap();
        drop(store);
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("DROP TABLE blob_stage_history; DROP TABLE blob_stage_operations; PRAGMA user_version=25;").unwrap();
        drop(db);
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "blob_stage_fault_child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_BLOB_STAGE_TEST_DB", &path)
            .env("MORROW_BLOB_STAGE_TEST_MODE", "migrate")
            .env("MORROW_TEST_CRASH_AT", point)
            .output()
            .unwrap();
        assert_eq!(child.status.code(), Some(86), "{point}: {child:?}");
        let db = rusqlite::Connection::open(&path).unwrap();
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, expected);
        drop(db);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        let mut actual = vec![];
        store.export_blob_local(&blob.id, &mut actual).unwrap();
        assert_eq!(actual, b"prior");
        store.integrity_check().unwrap();
    }
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "owned subprocess for explicit abrupt-exit boundaries"]
fn blob_stage_fault_child() {
    let db = std::env::var("MORROW_BLOB_STAGE_TEST_DB").unwrap();
    let mode = std::env::var("MORROW_BLOB_STAGE_TEST_MODE").unwrap();
    let mut store = Store::open_existing(std::path::Path::new(&db), Default::default()).unwrap();
    let cmd = command("crash", b"crash bytes");
    if mode == "stage" {
        store
            .stage_blob_operation_local(&mut Cursor::new(b"crash bytes"), &cmd, 1)
            .unwrap();
    } else {
        assert_eq!(mode, "release");
        store.release_blob_stage_local(&cmd, 2).unwrap();
    }
    panic!("fault boundary not reached");
}
#[cfg(feature = "fault-injection")]
#[test]
fn abrupt_exit_before_and_after_stage_or_release_recovers_only_original_committed_fact() {
    for (mode, point, staged, released) in [
        ("stage", "blob-stage-before-commit", false, false),
        ("stage", "blob-stage-after-commit", true, false),
        ("release", "blob-stage-release-before-commit", true, false),
        ("release", "blob-stage-release-after-commit", true, true),
    ] {
        let (_dir, path, mut store) = fixture();
        let cmd = command("crash", b"crash bytes");
        if mode == "release" {
            store
                .stage_blob_operation_local(&mut Cursor::new(b"crash bytes"), &cmd, 1)
                .unwrap();
        }
        drop(store);
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "blob_stage_fault_child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_BLOB_STAGE_TEST_DB", &path)
            .env("MORROW_BLOB_STAGE_TEST_MODE", mode)
            .env("MORROW_TEST_CRASH_AT", point)
            .output()
            .unwrap();
        assert_eq!(child.status.code(), Some(86), "{point}: {child:?}");
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        let fact = store.inspect_blob_stage_local(&cmd).unwrap();
        assert_eq!(fact.is_some(), staged, "{point}");
        assert_eq!(
            store.list_blobs_local("", 128).unwrap().len(),
            usize::from(staged)
        );
        if let Some(fact) = fact {
            assert_eq!(fact.is_released(), released, "{point}");
            if released {
                assert_eq!(
                    store.stage_blob_operation_local(&mut NeverRead, &cmd, 3),
                    Err(Error::OperationConflict)
                );
            } else {
                assert_eq!(
                    store
                        .stage_blob_operation_local(&mut NeverRead, &cmd, 3)
                        .unwrap(),
                    *fact.blob()
                );
            }
        }
        store.integrity_check().unwrap();
    }
}
