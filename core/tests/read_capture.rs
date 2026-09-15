#![cfg(not(target_arch = "wasm32"))]
// These are synthetic trusted-local lifecycle records, not guest execution proofs.
use morrow_core::{
    Error,
    read_archive::{Budget, Finish, Plan},
    read_capture::{Phase, Token},
    store::Store,
};
use std::path::{Path, PathBuf};
fn plan(op: &str) -> Plan {
    Plan {
        operation_id: op.into(),
        subject: "review.query".into(),
        request_type: "review.request".into(),
        request: b"stable request".to_vec(),
        response_type: "review.response".into(),
        budget: Budget::default(),
    }
}
fn setup() -> (tempfile::TempDir, PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture.db");
    let store = Store::open(&path, Default::default()).unwrap();
    (dir, path, store)
}
fn count(path: &Path, table: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn completion(store: &Store, op: &str) -> Finish {
    let s = store
        .lookup_read_archive("review.query", op)
        .unwrap()
        .unwrap();
    Finish {
        response: b"result".to_vec(),
        part_count: s.count,
        logical_bytes: s.logical_bytes,
        chain_sha256: s.chain_sha256,
        metadata_type: "review.metadata".into(),
        metadata: b"trusted local facts".to_vec(),
    }
}

#[test]
fn preparing_is_not_a_read_and_cas_cannot_be_bypassed_by_generic_archive_writes() {
    let (_dir, path, mut store) = setup();
    let p = plan("operation");
    let initial = store
        .begin_read_capture(&p, "review.context", b"source readpoint", [1; 32])
        .unwrap();
    assert_eq!(initial.phase(), Phase::Preparing);
    assert_eq!(initial.revision(), 1);
    assert_eq!(count(&path, "read_captures"), 1);
    assert_eq!(count(&path, "read_archives"), 1);
    assert_eq!(count(&path, "operations"), 0);
    assert!(
        store
            .lookup_read("review.query", "operation")
            .unwrap()
            .is_none()
    );
    let retry = store
        .begin_read_capture(&p, "review.context", b"source readpoint", [1; 32])
        .unwrap();
    assert_eq!(retry.container(), initial.container());
    assert!(
        store
            .begin_read_capture(&p, "review.context", b"changed", [1; 32])
            .is_err()
    );
    assert!(
        store
            .begin_read_capture(&p, "review.context", b"source readpoint", [2; 32])
            .is_err()
    );
    assert!(
        store
            .begin_read_capture(&plan("zero-owner"), "review.context", b"", [0; 32])
            .is_err()
    );
    assert!(store.begin_read_archive(&p).is_err());
    assert!(
        store
            .append_read_archive("review.query", "operation", 0, "review.part", b"bypass")
            .is_err()
    );
    assert!(
        store
            .abort_read_archive("review.query", "operation")
            .is_err()
    );
    assert!(
        store
            .finish_read_archive_local_authorized(
                "review.query",
                "operation",
                &completion(&store, "operation"),
                &[],
                || Ok(())
            )
            .is_err()
    );
    let wrong = Token {
        owner: [2; 32],
        revision: 1,
    };
    assert!(
        store
            .append_read_capture(
                "review.query",
                "operation",
                &wrong,
                0,
                "review.part",
                b"first"
            )
            .is_err()
    );
    let next = store
        .append_read_capture(
            "review.query",
            "operation",
            &initial.token(),
            0,
            "review.part",
            b"first",
        )
        .unwrap();
    assert_eq!(next.revision(), 2);
    assert!(
        store
            .append_read_capture(
                "review.query",
                "operation",
                &initial.token(),
                1,
                "review.part",
                b"second"
            )
            .is_err()
    );
    let retry = store
        .append_read_capture(
            "review.query",
            "operation",
            &next.token(),
            0,
            "review.part",
            b"first",
        )
        .unwrap();
    assert_eq!(retry.container(), next.container());
    for (ordinal, kind, data) in [
        (0, "review.part", b"changed".as_slice()),
        (0, "other", b"first".as_slice()),
        (2, "review.part", b"gap".as_slice()),
    ] {
        assert!(
            store
                .append_read_capture(
                    "review.query",
                    "operation",
                    &next.token(),
                    ordinal,
                    kind,
                    data
                )
                .is_err()
        );
    }
    assert_eq!(count(&path, "read_archive_parts"), 1);
    assert_eq!(count(&path, "operations"), 0);
    store.integrity_check().unwrap();
}
#[test]
fn terminal_failure_cancel_and_restart_interrupt_keep_intent_but_release_unpublished_bytes() {
    for phase in [Phase::Failed, Phase::Cancelled, Phase::Interrupted] {
        let (_dir, path, mut store) = setup();
        let initial = store
            .begin_read_capture(
                &plan("operation"),
                "review.context",
                b"original context",
                [3; 32],
            )
            .unwrap();
        let next = store
            .append_read_capture(
                "review.query",
                "operation",
                &initial.token(),
                0,
                "review.part",
                b"partial source",
            )
            .unwrap();
        drop(store);
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        let after_restart = store
            .lookup_read_capture("review.query", "operation")
            .unwrap()
            .unwrap();
        assert_eq!(after_restart.phase(), Phase::Preparing);
        assert_eq!(after_restart.container(), next.container());
        assert!(
            store
                .lookup_read("review.query", "operation")
                .unwrap()
                .is_none()
        );
        let terminal = store
            .end_read_capture(
                "review.query",
                "operation",
                &after_restart.token(),
                phase,
                "source stopped before EOF",
            )
            .unwrap();
        assert_eq!(terminal.phase(), phase);
        assert_eq!(terminal.revision(), 3);
        assert_eq!(terminal.context(), b"original context");
        assert!(terminal.archive_root().is_none());
        assert_eq!(count(&path, "read_captures"), 1);
        assert_eq!(count(&path, "read_archives"), 0);
        assert_eq!(count(&path, "read_archive_parts"), 0);
        assert_eq!(count(&path, "operations"), 0);
        assert!(
            store
                .append_read_capture(
                    "review.query",
                    "operation",
                    &terminal.token(),
                    0,
                    "review.part",
                    b"late"
                )
                .is_err()
        );
        assert!(store.begin_read_archive(&plan("operation")).is_err());
        assert!(
            store
                .begin_read_capture(&plan("operation"), "review.context", b"changed", [4; 32])
                .is_err()
        );
        store.integrity_check().unwrap();
    }
}
#[test]
fn ready_publishes_exact_root_and_read_together_and_retry_rechecks_current_guard() {
    let (_dir, path, mut store) = setup();
    let initial = store
        .begin_read_capture(&plan("operation"), "review.context", b"facts", [4; 32])
        .unwrap();
    let next = store
        .append_read_capture(
            "review.query",
            "operation",
            &initial.token(),
            0,
            "review.part",
            b"complete source",
        )
        .unwrap();
    let done = completion(&store, "operation");
    assert!(
        store
            .finish_read_capture_local_authorized(
                "review.query",
                "operation",
                &next.token(),
                &done,
                &[],
                || Err(Error::Integrity)
            )
            .is_err()
    );
    assert_eq!(
        store
            .lookup_read_capture("review.query", "operation")
            .unwrap()
            .unwrap()
            .container(),
        next.container()
    );
    assert_eq!(count(&path, "operations"), 0);
    let (ready, receipt) = store
        .finish_read_capture_local_authorized(
            "review.query",
            "operation",
            &next.token(),
            &done,
            &[],
            || Ok(()),
        )
        .unwrap();
    assert_eq!(ready.phase(), Phase::Ready);
    assert_eq!(ready.revision(), 3);
    let read = store
        .lookup_read("review.query", "operation")
        .unwrap()
        .unwrap();
    assert_eq!(read.data().schema_version, 2);
    assert_eq!(read.data().archive_sha256, ready.archive_root().unwrap());
    assert_eq!(read.digest(), receipt.observation_sha256);
    assert_eq!(count(&path, "outbox"), 1);
    assert_eq!(count(&path, "cards"), 0);
    assert!(
        store
            .finish_read_capture_local_authorized(
                "review.query",
                "operation",
                &ready.token(),
                &done,
                &[],
                || Err(Error::Integrity)
            )
            .is_err()
    );
    let (retry, receipt2) = store
        .finish_read_capture_local_authorized(
            "review.query",
            "operation",
            &ready.token(),
            &done,
            &[],
            || Ok(()),
        )
        .unwrap();
    assert_eq!(retry.container(), ready.container());
    assert_eq!(receipt2.observation_sha256, receipt.observation_sha256);
    let mut changed = done.clone();
    changed.response.push(1);
    assert!(
        store
            .finish_read_capture_local_authorized(
                "review.query",
                "operation",
                &ready.token(),
                &changed,
                &[],
                || Ok(())
            )
            .is_err()
    );
    assert!(
        store
            .end_read_capture(
                "review.query",
                "operation",
                &ready.token(),
                Phase::Cancelled,
                "too late"
            )
            .is_err()
    );
    drop(store);
    let store = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        store
            .lookup_read_capture("review.query", "operation")
            .unwrap()
            .unwrap()
            .container(),
        ready.container()
    );
    assert_eq!(
        store
            .lookup_read("review.query", "operation")
            .unwrap()
            .unwrap()
            .container(),
        read.container()
    );
}

fn install_commit_failure(path: &Path, table: &str) {
    rusqlite::Connection::open(path).unwrap().execute_batch(&format!("CREATE TABLE review_parent(id INTEGER PRIMARY KEY);CREATE TABLE review_deferred(id INTEGER PRIMARY KEY,parent INTEGER REFERENCES review_parent(id) DEFERRABLE INITIALLY DEFERRED);CREATE TRIGGER review_failure AFTER INSERT ON {table} BEGIN INSERT INTO review_deferred(id,parent) VALUES(1,99);END;")).unwrap();
}
fn remove_commit_failure(path: &Path) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch(
            "DROP TRIGGER review_failure;DROP TABLE review_deferred;DROP TABLE review_parent;",
        )
        .unwrap();
}
#[test]
fn actual_commit_unknown_rolls_back_begin_append_and_ready_without_partial_state() {
    for table in ["read_captures", "read_archive_parts", "outbox"] {
        let (_dir, path, mut store) = setup();
        let p = plan("operation");
        if table != "read_captures" {
            store
                .begin_read_capture(&p, "review.context", b"facts", [5; 32])
                .unwrap();
        }
        install_commit_failure(&path, table);
        let before = store
            .lookup_read_capture("review.query", "operation")
            .unwrap();
        let result = match table {
            "read_captures" => store
                .begin_read_capture(&p, "review.context", b"facts", [5; 32])
                .map(|_| ()),
            "read_archive_parts" => store
                .append_read_capture(
                    "review.query",
                    "operation",
                    &before.as_ref().unwrap().token(),
                    0,
                    "review.part",
                    b"input",
                )
                .map(|_| ()),
            _ => store
                .finish_read_capture_local_authorized(
                    "review.query",
                    "operation",
                    &before.as_ref().unwrap().token(),
                    &completion(&store, "operation"),
                    &[],
                    || Ok(()),
                )
                .map(|_| ()),
        };
        assert!(
            matches!(result, Err(Error::CommitUnknown)),
            "{table}: {result:?}"
        );
        let after = store
            .lookup_read_capture("review.query", "operation")
            .unwrap();
        assert_eq!(
            after.as_ref().map(|v| v.container()),
            before.as_ref().map(|v| v.container())
        );
        assert_eq!(count(&path, "read_archive_parts"), 0);
        assert_eq!(count(&path, "operations"), 0);
        assert_eq!(count(&path, "outbox"), 0);
        assert_eq!(count(&path, "read_archives"), i64::from(before.is_some()));
        remove_commit_failure(&path);
        match table {
            "read_captures" => {
                store
                    .begin_read_capture(&p, "review.context", b"facts", [5; 32])
                    .unwrap();
            }
            "read_archive_parts" => {
                store
                    .append_read_capture(
                        "review.query",
                        "operation",
                        &after.unwrap().token(),
                        0,
                        "review.part",
                        b"input",
                    )
                    .unwrap();
            }
            _ => {
                store
                    .finish_read_capture_local_authorized(
                        "review.query",
                        "operation",
                        &after.unwrap().token(),
                        &completion(&store, "operation"),
                        &[],
                        || Ok(()),
                    )
                    .unwrap();
            }
        }
        store.integrity_check().unwrap();
    }
}
#[test]
fn two_store_handles_cannot_append_with_one_revision_or_end_another_owner() {
    let (_dir, path, mut store) = setup();
    let initial = store
        .begin_read_capture(&plan("operation"), "review.context", b"facts", [6; 32])
        .unwrap();
    let mut other = Store::open_existing(&path, Default::default()).unwrap();
    let updated = other
        .append_read_capture(
            "review.query",
            "operation",
            &initial.token(),
            0,
            "review.part",
            b"winner",
        )
        .unwrap();
    assert!(
        store
            .append_read_capture(
                "review.query",
                "operation",
                &initial.token(),
                1,
                "review.part",
                b"stale"
            )
            .is_err()
    );
    assert!(
        store
            .end_read_capture(
                "review.query",
                "operation",
                &initial.token(),
                Phase::Cancelled,
                "stale"
            )
            .is_err()
    );
    assert!(
        store
            .end_read_capture(
                "review.query",
                "operation",
                &Token {
                    owner: [7; 32],
                    revision: updated.revision()
                },
                Phase::Cancelled,
                "wrong owner"
            )
            .is_err()
    );
    assert!(
        store
            .end_read_capture(
                "review.query",
                "operation",
                &updated.token(),
                Phase::Ready,
                "invalid transition"
            )
            .is_err()
    );
    assert!(
        store
            .lookup_read_capture("other", "operation")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .lookup_read_capture("review.query", "operation")
            .unwrap()
            .unwrap()
            .container(),
        updated.container()
    );
    assert_eq!(
        store
            .read_archive_part("review.query", "operation", 0)
            .unwrap()
            .unwrap()
            .data(),
        b"winner"
    );
}
#[test]
fn listing_is_stable_bounded_and_terminal_intents_remain_visible() {
    let (_dir, _path, mut store) = setup();
    for id in ["c", "a", "b"] {
        let s = store
            .begin_read_capture(&plan(id), "review.context", id.as_bytes(), [7; 32])
            .unwrap();
        store
            .end_read_capture(
                "review.query",
                id,
                &s.token(),
                Phase::Cancelled,
                "not selected",
            )
            .unwrap();
    }
    assert!(store.list_read_captures("", 0).is_err());
    let first = store.list_read_captures("", 2).unwrap();
    assert_eq!(
        first
            .iter()
            .map(|s| s.plan().operation_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    let last = store.list_read_captures("b", 2).unwrap();
    assert_eq!(last.len(), 1);
    assert_eq!(last[0].plan().operation_id, "c");
    assert_eq!(last[0].phase(), Phase::Cancelled);
    assert!(store.list_read_captures("c", 2).unwrap().is_empty());
}

fn packed_state(raw: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let compressed = lz4_flex::block::compress(raw);
    let mut out = morrow_core::read_capture::MAGIC.to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend_from_slice(&compressed);
    out
}
#[test]
fn malformed_state_and_broken_archive_association_are_rejected_without_repair() {
    use prost::Message;
    for damage in 0..6 {
        let (_dir, path, mut store) = setup();
        let initial = store
            .begin_read_capture(&plan("operation"), "review.context", b"facts", [8; 32])
            .unwrap();
        let (ready, _) = store
            .finish_read_capture_local_authorized(
                "review.query",
                "operation",
                &initial.token(),
                &completion(&store, "operation"),
                &[],
                || Ok(()),
            )
            .unwrap();
        let sql = rusqlite::Connection::open(&path).unwrap();
        match damage {
            0 => {
                sql.execute_batch("PRAGMA foreign_keys=OFF;DELETE FROM operation_read_archives;DELETE FROM read_archives;").unwrap();
            }
            1 => {
                let mut value = ready.data().clone();
                value.archive_root = vec![99; 32];
                sql.execute(
                    "UPDATE read_captures SET payload=?1",
                    [packed_state(&value.encode_to_vec())],
                )
                .unwrap();
            }
            2 => {
                let mut value = ready.data().clone();
                value.phase = Phase::Preparing as i32;
                value.archive_root.clear();
                sql.execute(
                    "UPDATE read_captures SET phase=1,payload=?1",
                    [packed_state(&value.encode_to_vec())],
                )
                .unwrap();
            }
            3 => {
                let mut value = ready.data().clone();
                let mut p =
                    morrow_core::read_archive::proto::Plan::decode(value.plan.as_slice()).unwrap();
                p.request = b"forged request".to_vec();
                value.plan = p.encode_to_vec();
                sql.execute(
                    "UPDATE read_captures SET payload=?1",
                    [packed_state(&value.encode_to_vec())],
                )
                .unwrap();
            }
            4 => {
                sql.execute_batch("UPDATE read_captures SET subject='other'")
                    .unwrap();
            }
            _ => {
                sql.execute_batch("INSERT INTO read_captures(operation_id,subject,phase,charged_bytes,payload) SELECT 'orphan','review.query',phase,charged_bytes,payload FROM read_captures LIMIT 1").unwrap();
            }
        }
        assert!(
            store
                .lookup_read_capture("review.query", "operation")
                .is_err()
                || damage >= 4,
            "damage {damage}"
        );
        assert!(store.integrity_check().is_err(), "damage {damage}");
        drop(store);
        assert!(
            Store::open_existing(&path, Default::default()).is_err(),
            "damage {damage}"
        );
        assert!(count(&path, "read_captures") >= 1);
    }
}
#[test]
fn state_codec_rejects_duplicate_fields_future_phase_zero_owner_and_oversized_sql() {
    use morrow_core::read_capture::{MAX_CONTAINER_BYTES, State};
    use prost::Message;
    let (_dir, path, mut store) = setup();
    let initial = store
        .begin_read_capture(&plan("operation"), "review.context", b"facts", [9; 32])
        .unwrap();
    assert_eq!(
        State::decode(initial.container()).unwrap().raw(),
        initial.raw()
    );
    let mut duplicate = initial.raw().to_vec();
    duplicate.extend_from_slice(&[8, 1]);
    assert!(State::decode(&packed_state(&duplicate)).is_err());
    for damage in 0..5 {
        let mut value = initial.data().clone();
        match damage {
            0 => value.schema_version = 2,
            1 => value.phase = 99,
            2 => value.owner = vec![0; 32],
            3 => value.revision = 0,
            _ => value.archive_root = vec![1; 32],
        };
        assert!(
            State::decode(&packed_state(&value.encode_to_vec())).is_err(),
            "damage {damage}"
        );
    }
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE read_captures SET payload=zeroblob(?1)",
            [(MAX_CONTAINER_BYTES + 1) as i64],
        )
        .unwrap();
    assert!(matches!(
        store.lookup_read_capture("review.query", "operation"),
        Err(Error::Limit)
    ));
    assert!(store.list_read_captures("", 1).is_err());
}

#[test]
fn capture_operation_identity_remains_reserved_after_cancellation() {
    use morrow_core::{content::CardRecord, read_journal::Input};
    let (_dir, path, mut store) = setup();
    let initial = store
        .begin_read_capture(&plan("operation"), "review.context", b"facts", [10; 32])
        .unwrap();
    for terminal in [false, true] {
        if terminal {
            store
                .end_read_capture(
                    "review.query",
                    "operation",
                    &initial.token(),
                    Phase::Cancelled,
                    "cancelled",
                )
                .unwrap();
        }
        let card = CardRecord::new("card", "test.card", 1, "Title", vec![]).unwrap();
        assert!(store.create_local("operation", &card).is_err());
        assert!(
            store
                .record_read_local_authorized(
                    &Input {
                        operation_id: "operation".into(),
                        subject: "other".into(),
                        request_type: "review.request".into(),
                        request: vec![],
                        response_type: "review.response".into(),
                        response: vec![]
                    },
                    &[],
                    || Ok(())
                )
                .is_err()
        );
        assert_eq!(count(&path, "operations"), 0);
        assert_eq!(count(&path, "cards"), 0);
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn read_capture_crash_child() {
    let Some(path) = std::env::var_os("MORROW_CAPTURE_REVIEW_DB") else {
        return;
    };
    let mut store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
    match std::env::var("MORROW_CAPTURE_REVIEW_MODE")
        .unwrap()
        .as_str()
    {
        "begin" => {
            store
                .begin_read_capture(
                    &plan("operation"),
                    "review.context",
                    b"crash facts",
                    [11; 32],
                )
                .unwrap();
        }
        "append" => {
            let s = store
                .lookup_read_capture("review.query", "operation")
                .unwrap()
                .unwrap();
            store
                .append_read_capture(
                    "review.query",
                    "operation",
                    &s.token(),
                    0,
                    "review.part",
                    b"crash part",
                )
                .unwrap();
        }
        "final" => {
            let s = store
                .lookup_read_capture("review.query", "operation")
                .unwrap()
                .unwrap();
            store
                .finish_read_capture_local_authorized(
                    "review.query",
                    "operation",
                    &s.token(),
                    &completion(&store, "operation"),
                    &[],
                    || Ok(()),
                )
                .unwrap();
        }
        "end" => {
            let s = store
                .lookup_read_capture("review.query", "operation")
                .unwrap()
                .unwrap();
            store
                .end_read_capture(
                    "review.query",
                    "operation",
                    &s.token(),
                    Phase::Interrupted,
                    "source process gone",
                )
                .unwrap();
        }
        "migrate" => {}
        other => panic!("unexpected child mode {other}"),
    }
}
#[cfg(feature = "fault-injection")]
fn crash(path: &Path, mode: &str, point: &str) {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "read_capture_crash_child", "--nocapture"])
        .env("MORROW_CAPTURE_REVIEW_DB", path)
        .env("MORROW_CAPTURE_REVIEW_MODE", mode)
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
            Ok(Some(status)) => {
                assert_eq!(status.code(), Some(86), "{point}");
                return;
            }
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe child {point}: {e}");
            }
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timeout {point}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn failed_terminal_commit_keeps_original_parts_and_retry_releases_them_once() {
    let (_dir, path, mut store) = setup();
    let s = store
        .begin_read_capture(&plan("operation"), "review.context", b"facts", [12; 32])
        .unwrap();
    let s = store
        .append_read_capture(
            "review.query",
            "operation",
            &s.token(),
            0,
            "review.part",
            b"original partial bytes",
        )
        .unwrap();
    let part = store
        .read_archive_part("review.query", "operation", 0)
        .unwrap()
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TABLE review_parent(id INTEGER PRIMARY KEY);CREATE TABLE review_deferred(id INTEGER PRIMARY KEY,parent INTEGER REFERENCES review_parent(id) DEFERRABLE INITIALLY DEFERRED);CREATE TRIGGER review_failure AFTER UPDATE ON read_captures BEGIN INSERT INTO review_deferred(id,parent) VALUES(1,99);END;").unwrap();
    let result = store.end_read_capture(
        "review.query",
        "operation",
        &s.token(),
        Phase::Failed,
        "source failed",
    );
    assert!(matches!(result, Err(Error::CommitUnknown)), "{result:?}");
    assert_eq!(
        store
            .lookup_read_capture("review.query", "operation")
            .unwrap()
            .unwrap()
            .container(),
        s.container()
    );
    assert_eq!(
        store
            .read_archive_part("review.query", "operation", 0)
            .unwrap()
            .unwrap()
            .container(),
        part.container()
    );
    remove_commit_failure(&path);
    let end = store
        .end_read_capture(
            "review.query",
            "operation",
            &s.token(),
            Phase::Failed,
            "source failed",
        )
        .unwrap();
    let retry = store
        .end_read_capture(
            "review.query",
            "operation",
            &end.token(),
            Phase::Failed,
            "source failed",
        )
        .unwrap();
    assert_eq!(retry.container(), end.container());
    assert_eq!(retry.charged_bytes(), s.charged_bytes());
    assert!(
        store
            .end_read_capture(
                "review.query",
                "operation",
                &end.token(),
                Phase::Cancelled,
                "changed outcome"
            )
            .is_err()
    );
    assert_eq!(count(&path, "read_archive_parts"), 0);
    assert_eq!(count(&path, "read_archives"), 0);
    assert_eq!(count(&path, "operations"), 0);
    store.integrity_check().unwrap();
}

#[test]
fn lower_state_and_preparation_quotas_leave_no_half_capture_and_exact_retry_is_free() {
    use morrow_core::{read_archive::PreparationBudget, read_capture::Budget as CaptureBudget};
    let (_dir, path, mut store) = setup();
    let s = store
        .begin_read_capture(&plan("sample"), "review.context", b"facts", [13; 32])
        .unwrap();
    let charge = s.charged_bytes();
    assert!(charge >= (s.raw().len() + s.container().len()) as u64 + 4096);
    store
        .end_read_capture(
            "review.query",
            "sample",
            &s.token(),
            Phase::Cancelled,
            "done",
        )
        .unwrap();
    let budget = CaptureBudget {
        max_states: 1,
        max_bytes: 256 * 1024 * 1024,
        preparation: PreparationBudget::default(),
    };
    assert!(matches!(
        store.begin_read_capture_with_budget(
            &plan("other"),
            "review.context",
            b"facts",
            [13; 32],
            budget
        ),
        Err(Error::Limit)
    ));
    assert_eq!(count(&path, "read_captures"), 1);
    assert_eq!(count(&path, "read_archives"), 0);
    let budget = CaptureBudget {
        max_states: 3,
        max_bytes: charge,
        preparation: PreparationBudget::default(),
    };
    assert!(matches!(
        store.begin_read_capture_with_budget(
            &plan("other"),
            "review.context",
            b"facts",
            [13; 32],
            budget
        ),
        Err(Error::Limit)
    ));
    let budget = CaptureBudget {
        max_states: 3,
        max_bytes: 256 * 1024 * 1024,
        preparation: PreparationBudget {
            max_archives: 1,
            max_bytes: 1,
        },
    };
    assert!(matches!(
        store.begin_read_capture_with_budget(
            &plan("other"),
            "review.context",
            b"facts",
            [13; 32],
            budget
        ),
        Err(Error::Limit)
    ));
    assert_eq!(count(&path, "read_captures"), 1);
    assert_eq!(count(&path, "read_archives"), 0);
    let s = store
        .begin_read_capture(&plan("active"), "review.context", b"facts", [13; 32])
        .unwrap();
    let retry = store
        .begin_read_capture_with_budget(
            &plan("active"),
            "review.context",
            b"facts",
            [13; 32],
            CaptureBudget {
                max_states: 1,
                max_bytes: 1,
                preparation: PreparationBudget {
                    max_archives: 1,
                    max_bytes: 1,
                },
            },
        )
        .unwrap();
    assert_eq!(s.container(), retry.container());
    store.integrity_check().unwrap();
}
fn version(path: &Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}
fn downgrade_to_twelve(path: &Path) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch("DROP TABLE io_reservations; DROP TABLE io_intents; DROP TABLE read_archive_costs; DROP TABLE read_archive_totals; DROP TABLE read_captures;PRAGMA user_version=12;")
        .unwrap();
}
#[test]
fn migration_preserves_legacy_read_archive_originals_and_sealed_signature() {
    use morrow_core::audit::{self, SigningKey, TrustedLog};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("signed.db");
    let key = SigningKey::from_bytes(&[46; 32]);
    let trust = TrustedLog {
        id: "capture-migration".into(),
        key: key.verifying_key(),
    };
    let mut store = Store::open_audited(&path, Default::default(), true, trust.clone()).unwrap();
    store.begin_read_archive(&plan("legacy")).unwrap();
    store
        .append_read_archive(
            "review.query",
            "legacy",
            0,
            "review.part",
            b"legacy exact data",
        )
        .unwrap();
    store
        .finish_read_archive_local_authorized(
            "review.query",
            "legacy",
            &completion(&store, "legacy"),
            &[],
            || Ok(()),
        )
        .unwrap();
    let read = store
        .lookup_read("review.query", "legacy")
        .unwrap()
        .unwrap();
    let part = store
        .read_archive_part("review.query", "legacy", 0)
        .unwrap()
        .unwrap();
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &store.pending(0, 10).unwrap()).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    store.seal_pending(&signed).unwrap();
    drop(store);
    downgrade_to_twelve(&path);
    let old = Store::open_read_only_audited(&path, trust.clone()).unwrap();
    assert_eq!(
        old.lookup_read("review.query", "legacy")
            .unwrap()
            .unwrap()
            .container(),
        read.container()
    );
    drop(old);
    assert_eq!(version(&path), 12);
    let mut store = Store::open_audited(&path, Default::default(), false, trust.clone()).unwrap();
    assert_eq!(version(&path), 16);
    assert_eq!(
        store
            .lookup_read("review.query", "legacy")
            .unwrap()
            .unwrap()
            .raw(),
        read.raw()
    );
    assert_eq!(
        store
            .read_archive_part("review.query", "legacy", 0)
            .unwrap()
            .unwrap()
            .container(),
        part.container()
    );
    assert_eq!(store.sealed_segment(1).unwrap().unwrap(), signed);
    store
        .begin_read_capture(&plan("new"), "review.context", b"facts", [14; 32])
        .unwrap();
    drop(store);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE io_reservations; DROP TABLE io_intents; PRAGMA user_version=12;")
        .unwrap();
    assert!(Store::open_audited(&path, Default::default(), false, trust).is_err());
    assert_eq!(version(&path), 12);
}
#[test]
fn deleted_state_cannot_turn_tracked_archive_into_an_untracked_preparation() {
    let (_dir, path, mut store) = setup();
    store
        .begin_read_capture(&plan("operation"), "review.context", b"facts", [15; 32])
        .unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DELETE FROM read_captures")
        .unwrap();
    assert!(
        store
            .append_read_archive("review.query", "operation", 0, "review.part", b"bypass")
            .is_err()
    );
    assert!(
        store
            .abort_read_archive("review.query", "operation")
            .is_err()
    );
    assert!(store.integrity_check().is_err());
    drop(store);
    assert!(Store::open_existing(&path, Default::default()).is_err());
    assert_eq!(count(&path, "read_archives"), 1);
}
#[cfg(feature = "fault-injection")]
#[test]
fn process_crashes_do_not_publish_partial_sources_and_committed_ready_is_retrievable() {
    for (mode, point, committed) in [
        ("begin", "capture-after-begin-state", false),
        ("begin", "capture-before-begin-commit", false),
        ("begin", "capture-after-begin-commit", true),
        ("append", "capture-after-append-state", false),
        ("append", "capture-before-append-commit", false),
        ("append", "capture-after-append-commit", true),
        ("final", "capture-after-ready-state", false),
        ("final", "capture-before-finish-commit", false),
        ("final", "capture-after-finish-commit", true),
        ("end", "capture-after-end-state", false),
        ("end", "capture-before-end-commit", false),
        ("end", "capture-after-end-commit", true),
    ] {
        let (_dir, path, mut store) = setup();
        if mode != "begin" {
            let s = store
                .begin_read_capture(
                    &plan("operation"),
                    "review.context",
                    b"crash facts",
                    [11; 32],
                )
                .unwrap();
            if mode == "end" {
                store
                    .append_read_capture(
                        "review.query",
                        "operation",
                        &s.token(),
                        0,
                        "review.part",
                        b"partial source",
                    )
                    .unwrap();
            }
        }
        drop(store);
        crash(&path, mode, point);
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        let state = store
            .lookup_read_capture("review.query", "operation")
            .unwrap();
        if mode == "begin" && !committed {
            assert!(state.is_none());
            assert_eq!(count(&path, "read_archives"), 0);
            continue;
        }
        let state = state.unwrap();
        let expected = if mode == "final" && committed {
            Phase::Ready
        } else if mode == "end" && committed {
            Phase::Interrupted
        } else {
            Phase::Preparing
        };
        assert_eq!(state.phase(), expected, "{point}");
        assert_eq!(
            count(&path, "operations"),
            i64::from(expected == Phase::Ready),
            "{point}"
        );
        assert_eq!(
            count(&path, "outbox"),
            i64::from(expected == Phase::Ready),
            "{point}"
        );
        if mode == "append" {
            assert_eq!(count(&path, "read_archive_parts"), i64::from(committed));
        }
        if mode == "end" {
            assert_eq!(count(&path, "read_archive_parts"), i64::from(!committed));
            assert_eq!(state.context(), b"crash facts");
        }
        if expected == Phase::Ready {
            let original = store
                .lookup_read("review.query", "operation")
                .unwrap()
                .unwrap();
            let (retry, receipt) = store
                .finish_read_capture_local_authorized(
                    "review.query",
                    "operation",
                    &state.token(),
                    &completion(&store, "operation"),
                    &[],
                    || Ok(()),
                )
                .unwrap();
            assert_eq!(retry.container(), state.container());
            assert_eq!(receipt.observation_sha256, original.digest());
        } else {
            assert!(
                store
                    .lookup_read("review.query", "operation")
                    .unwrap()
                    .is_none()
            );
        }
        store.integrity_check().unwrap();
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn migration_crashes_keep_either_complete_database_version() {
    for (point, expected) in [
        ("capture-migration-before-commit", 12),
        ("capture-migration-after-commit", 13),
    ] {
        let (_dir, path, mut store) = setup();
        store.begin_read_archive(&plan("legacy")).unwrap();
        store
            .append_read_archive("review.query", "legacy", 0, "review.part", b"old original")
            .unwrap();
        store
            .finish_read_archive_local_authorized(
                "review.query",
                "legacy",
                &completion(&store, "legacy"),
                &[],
                || Ok(()),
            )
            .unwrap();
        let original = store
            .lookup_read("review.query", "legacy")
            .unwrap()
            .unwrap();
        drop(store);
        downgrade_to_twelve(&path);
        crash(&path, "migrate", point);
        assert_eq!(version(&path), expected);
        let tables: i64 = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='read_captures'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, i64::from(expected == 13));
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(
            store
                .lookup_read("review.query", "legacy")
                .unwrap()
                .unwrap()
                .container(),
            original.container()
        );
        store.integrity_check().unwrap();
    }
}
