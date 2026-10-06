#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    channel::{Action, Frame, Request, Response, Status},
    store::{ChannelCommit, EventBudget, Store},
};
fn material(sequence: u64, call: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let frame = Frame {
        sequence,
        source_epoch: [3; 32],
        bytes: format!("event-{sequence}").into_bytes(),
        cursor: sequence.to_le_bytes().to_vec(),
    };
    let request = Request {
        call_id: [call; 32],
        reference: [2; 32],
        source_epoch: [3; 32],
        action: Action::Ack {
            sequence,
            frame_sha256: frame.digest().unwrap(),
            cursor: frame.cursor.clone(),
        },
    };
    let response = Response {
        call_id: request.call_id,
        request_sha256: request.digest().unwrap(),
        reference: request.reference,
        source_epoch: request.source_epoch,
        status: Status::Acked,
        frame: None,
        last_acked: sequence,
        accepted_sequence: 0,
        resource_reclaimed: false,
    };
    (
        frame.encode().unwrap(),
        request.encode().unwrap(),
        response.encode().unwrap(),
    )
}
#[test]
fn ack_receipt_and_checkpoint_are_atomic_durable_and_duplicate_ack_does_not_commit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("subscriptions.sqlite");
    let (frame, request, response) = material(1, 1);
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let result = store
        .commit_channel_ack(&[9; 32], None, &frame, &request, &response)
        .unwrap();
    let ChannelCommit::Committed(first) = result else {
        panic!("first ACK must commit")
    };
    assert_eq!(first.sequence, 1);
    store.integrity_check().unwrap();
    drop(store);
    let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert_eq!(
        store.channel_checkpoint(&[9; 32], &[3; 32]).unwrap(),
        Some(first.clone())
    );
    let receipt = store
        .channel_ack_receipt(&[9; 32], &[3; 32], 1)
        .unwrap()
        .unwrap();
    assert_eq!(receipt.frame_wire, frame);
    assert_eq!(receipt.request_wire, request);
    assert_eq!(receipt.response_wire, response);
    let (_, request2, response2) = material(1, 4);
    assert_eq!(
        store
            .commit_channel_ack(&[9; 32], None, &frame, &request2, &response2)
            .unwrap(),
        ChannelCommit::Duplicate(first)
    );
    assert_eq!(
        store
            .channel_ack_receipt(&[9; 32], &[3; 32], 1)
            .unwrap()
            .unwrap(),
        receipt
    );
    store.integrity_check().unwrap();
    assert_eq!(store.channel_checkpoint(&[9; 32], &[8; 32]).unwrap(), None);
}
#[test]
fn stale_cas_bad_frame_unknown_ack_and_wrong_cursor_never_advance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("subscriptions.sqlite");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let (a, b, c) = material(1, 1);
    store
        .commit_channel_ack(&[9; 32], None, &a, &b, &c)
        .unwrap();
    let checkpoint = store
        .channel_checkpoint(&[9; 32], &[3; 32])
        .unwrap()
        .unwrap();
    let (a, b, c) = material(2, 2);
    assert!(matches!(
        store.commit_channel_ack(&[9; 32], None, &a, &b, &c),
        Err(Error::RevisionConflict)
    ));
    let mut unknown = Response::decode(&c).unwrap();
    unknown.status = Status::Unknown;
    assert!(
        store
            .commit_channel_ack(
                &[9; 32],
                Some(&checkpoint),
                &a,
                &b,
                &unknown.encode().unwrap()
            )
            .is_err()
    );
    let mut req = Request::decode(&b).unwrap();
    let Action::Ack { cursor, .. } = &mut req.action else {
        unreachable!()
    };
    *cursor = b"wrong".to_vec();
    let mut resp = Response::decode(&c).unwrap();
    resp.request_sha256 = req.digest().unwrap();
    assert!(
        store
            .commit_channel_ack(
                &[9; 32],
                Some(&checkpoint),
                &a,
                &req.encode().unwrap(),
                &resp.encode().unwrap()
            )
            .is_err()
    );
    assert_eq!(
        store.channel_checkpoint(&[9; 32], &[3; 32]).unwrap(),
        Some(checkpoint.clone())
    );
    assert!(
        store
            .channel_ack_receipt(&[9; 32], &[3; 32], 2)
            .unwrap()
            .is_none()
    );
    store
        .commit_channel_ack(&[9; 32], Some(&checkpoint), &a, &b, &c)
        .unwrap();
    let (a, b, c) = material(3, 3);
    assert!(matches!(
        store.commit_channel_ack(&[9; 32], Some(&checkpoint), &a, &b, &c),
        Err(Error::RevisionConflict)
    ));
    assert_eq!(
        store
            .channel_checkpoint(&[9; 32], &[3; 32])
            .unwrap()
            .unwrap()
            .sequence,
        2
    );
}
#[test]
fn shared_store_capacity_failure_keeps_cursor_and_original_receipt_unwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("subscriptions.sqlite");
    let mut store = Store::open(
        &path,
        EventBudget {
            max_count: 1024,
            max_bytes: 1,
        },
    )
    .unwrap();
    let (a, b, c) = material(1, 1);
    assert!(matches!(
        store.commit_channel_ack(&[9; 32], None, &a, &b, &c),
        Err(Error::EventCapacity)
    ));
    assert!(
        store
            .channel_checkpoint(&[9; 32], &[3; 32])
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .channel_ack_receipt(&[9; 32], &[3; 32], 1)
            .unwrap()
            .is_none()
    );
    store.integrity_check().unwrap();
}
#[test]
fn sqlite_busy_and_tampered_receipt_are_visible_without_a_cursor_advance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("subscriptions.sqlite");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    let (a, b, c) = material(1, 1);
    assert!(matches!(
        store.commit_channel_ack(&[9; 32], None, &a, &b, &c),
        Err(Error::StorageBusy)
    ));
    db.execute_batch("ROLLBACK").unwrap();
    assert!(
        store
            .channel_checkpoint(&[9; 32], &[3; 32])
            .unwrap()
            .is_none()
    );
    store
        .commit_channel_ack(&[9; 32], None, &a, &b, &c)
        .unwrap();
    db.execute("UPDATE channel_ack_receipts SET payload=x'00'", [])
        .unwrap();
    assert!(store.integrity_check().is_err());
    assert!(store.channel_ack_receipt(&[9; 32], &[3; 32], 1).is_err());
}

#[test]
fn commit_guard_rejection_after_rows_are_written_rolls_back_cursor_and_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("guard-rejected.sqlite");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let (frame, request, response) = material(1, 1);
    let mut calls = 0;
    let result =
        store.commit_channel_ack_guarded(&[9; 32], None, &frame, &request, &response, || {
            calls += 1;
            if calls == 2 {
                Err(Error::Invalid("source authority revoked"))
            } else {
                Ok(())
            }
        });
    assert_eq!(result, Err(Error::Invalid("source authority revoked")));
    assert_eq!(calls, 2);
    assert!(
        store
            .channel_checkpoint(&[9; 32], &[3; 32])
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .channel_ack_receipt(&[9; 32], &[3; 32], 1)
            .unwrap()
            .is_none()
    );
    store.integrity_check().unwrap();
    store
        .commit_channel_ack(&[9; 32], None, &frame, &request, &response)
        .unwrap();
    let original = store
        .channel_ack_receipt(&[9; 32], &[3; 32], 1)
        .unwrap()
        .unwrap();
    calls = 0;
    let result =
        store.commit_channel_ack_guarded(&[9; 32], None, &frame, &request, &response, || {
            calls += 1;
            if calls == 2 {
                Err(Error::Invalid("source expired"))
            } else {
                Ok(())
            }
        });
    assert_eq!(result, Err(Error::Invalid("source expired")));
    assert_eq!(
        store
            .channel_ack_receipt(&[9; 32], &[3; 32], 1)
            .unwrap()
            .unwrap(),
        original
    );
}

#[test]
fn real_sqlite_busy_is_immediate_and_later_explicit_guard_rejects_cancel_or_expiry() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };
    for expired in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("busy-authority.sqlite");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let (frame, request, response) = material(1, 1);
        store
            .commit_channel_ack(&[9; 32], None, &frame, &request, &response)
            .unwrap();
        let original = store
            .channel_checkpoint(&[9; 32], &[3; 32])
            .unwrap()
            .unwrap();
        let receipt = store
            .channel_ack_receipt(&[9; 32], &[3; 32], 1)
            .unwrap()
            .unwrap();
        let active = Arc::new(AtomicBool::new(true));
        let worker_active = active.clone();
        let worker_path = path.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (start_tx, start_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let db = rusqlite::Connection::open(worker_path).unwrap();
            db.execute_batch("BEGIN IMMEDIATE").unwrap();
            ready_tx.send(()).unwrap();
            start_rx.recv().unwrap();
            std::thread::sleep(Duration::from_millis(50));
            if !expired {
                worker_active.store(false, Ordering::Release);
            }
            // Keep the actual SQLite write lock held after cancellation/expiry.
            std::thread::sleep(Duration::from_millis(100));
            db.execute_batch("ROLLBACK").unwrap();
        });
        ready_rx.recv().unwrap();
        let start = Instant::now();
        let deadline = start + Duration::from_millis(40);
        assert!(active.load(Ordering::Acquire));
        let (frame, request, response) = material(2, 2);
        start_tx.send(()).unwrap();
        let mut checks = 0;
        let result = store.commit_channel_ack_guarded(
            &[9; 32],
            Some(&original),
            &frame,
            &request,
            &response,
            || {
                checks += 1;
                if !active.load(Ordering::Acquire) {
                    return Err(Error::Invalid("source authority revoked"));
                }
                if expired && Instant::now() >= deadline {
                    return Err(Error::Invalid("source expired"));
                }
                Ok(())
            },
        );
        assert_eq!(result, Err(Error::StorageBusy));
        assert_eq!(
            checks, 0,
            "busy_timeout is zero: no transaction acquired and no guard called"
        );
        assert_eq!(
            store.channel_checkpoint(&[9; 32], &[3; 32]).unwrap(),
            Some(original.clone())
        );
        assert!(
            store
                .channel_ack_receipt(&[9; 32], &[3; 32], 2)
                .unwrap()
                .is_none()
        );
        worker.join().unwrap();
        // This is a separate explicit trusted-host validation attempt, after
        // cancellation/expiry and lock release. Store never retries automatically.
        let result = store.commit_channel_ack_guarded(
            &[9; 32],
            Some(&original),
            &frame,
            &request,
            &response,
            || {
                checks += 1;
                if !active.load(Ordering::Acquire) {
                    return Err(Error::Invalid("source authority revoked"));
                }
                if expired && Instant::now() >= deadline {
                    return Err(Error::Invalid("source expired"));
                }
                Ok(())
            },
        );
        assert_eq!(
            result,
            Err(Error::Invalid(if expired {
                "source expired"
            } else {
                "source authority revoked"
            }))
        );
        assert_eq!(
            checks, 1,
            "the separate explicit host call checks original authority"
        );
        assert!(start.elapsed() >= Duration::from_millis(100));
        assert_eq!(
            store.channel_checkpoint(&[9; 32], &[3; 32]).unwrap(),
            Some(original)
        );
        assert_eq!(
            store.channel_ack_receipt(&[9; 32], &[3; 32], 1).unwrap(),
            Some(receipt)
        );
        assert!(
            store
                .channel_ack_receipt(&[9; 32], &[3; 32], 2)
                .unwrap()
                .is_none()
        );
        store.integrity_check().unwrap();
        println!(
            "actual-sqlite-busy: expired={expired} busy-timeout-ms=0 first-call-storage-busy=true first-call-guard-checks=0 later-explicit-call-rejected=true cursor-unchanged=true receipt-unchanged=true"
        );
    }
}

#[test]
fn native_cancel_and_real_deadline_at_final_transaction_guard_roll_back_written_rows() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };
    for expired in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("final-gate.sqlite");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let (frame, request, response) = material(1, 1);
        let active = Arc::new(AtomicBool::new(true));
        let worker_active = active.clone();
        let (gate_tx, gate_rx) = mpsc::channel::<Instant>();
        let (changed_tx, changed_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let deadline = gate_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            if expired {
                while Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }
            } else {
                worker_active.store(false, Ordering::Release);
            }
            changed_tx.send(()).unwrap();
        });
        let mut checks = 0;
        let mut deadline = None;
        let result =
            store.commit_channel_ack_guarded(&[9; 32], None, &frame, &request, &response, || {
                checks += 1;
                if checks == 1 {
                    assert!(active.load(Ordering::Acquire));
                    deadline = Some(Instant::now() + Duration::from_millis(30));
                    return Ok(());
                }
                // The Store calls this second guard after writing both rows and
                // before COMMIT. Test-only rendezvous makes the actual native
                // cancellation/monotonic expiry deterministic at that exact gate.
                let deadline = deadline.unwrap();
                gate_tx.send(deadline).unwrap();
                changed_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                if !active.load(Ordering::Acquire) {
                    return Err(Error::Invalid("source authority revoked"));
                }
                if expired && Instant::now() >= deadline {
                    return Err(Error::Invalid("source expired"));
                }
                Ok(())
            });
        worker.join().unwrap();
        assert_eq!(checks, 2);
        assert_eq!(
            result,
            Err(Error::Invalid(if expired {
                "source expired"
            } else {
                "source authority revoked"
            }))
        );
        assert!(
            store
                .channel_checkpoint(&[9; 32], &[3; 32])
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .channel_ack_receipt(&[9; 32], &[3; 32], 1)
                .unwrap()
                .is_none()
        );
        store.integrity_check().unwrap();
        println!(
            "final-transaction-gate: expired={expired} native-transition=true checks={checks} committed=false cursor-absent=true receipt-absent=true"
        );
    }
}

// The normal build excludes Store's process-exit fault hooks. This child is
// invoked explicitly by the parent crash test under the existing feature.
#[cfg(feature = "fault-injection")]
#[test]
#[ignore]
fn channel_checkpoint_crash_child() {
    let path = std::path::PathBuf::from(
        std::env::var_os("MORROW_CHANNEL_CRASH_DB").expect("parent supplied DB"),
    );
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    if std::env::var("MORROW_CHANNEL_MIGRATION_ONLY").as_deref() == Ok("1") {
        return;
    }
    let (frame, request, response) = material(1, 1);
    store
        .commit_channel_ack(&[9; 32], None, &frame, &request, &response)
        .unwrap();
    panic!("selected crash boundary did not terminate the child");
}

#[cfg(feature = "fault-injection")]
#[test]
fn checkpoint_process_crash_before_and_after_commit_recovers_exact_history() {
    for (boundary, committed) in [
        ("channel-checkpoint-before-commit", false),
        ("channel-checkpoint-after-commit", true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checkpoint-crash.sqlite");
        // Create the existing database before the child; the crash concerns only
        // the new ACK transaction, not a create/migration boundary.
        drop(Store::open(&path, EventBudget::default()).unwrap());
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "channel_checkpoint_crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_CHANNEL_CRASH_DB", &path)
            .env("MORROW_TEST_CRASH_AT", boundary)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(86),
            "{boundary}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let store = Store::open_existing(&path, EventBudget::default()).unwrap();
        store.integrity_check().unwrap();
        let checkpoint = store.channel_checkpoint(&[9; 32], &[3; 32]).unwrap();
        let receipt = store.channel_ack_receipt(&[9; 32], &[3; 32], 1).unwrap();
        assert_eq!(checkpoint.is_some(), committed, "{boundary}");
        assert_eq!(receipt.is_some(), committed, "{boundary}");
        if committed {
            let receipt = receipt.unwrap();
            assert_eq!(Some(receipt.checkpoint.clone()), checkpoint);
            let (frame, request, response) = material(1, 1);
            assert_eq!(receipt.frame_wire, frame);
            assert_eq!(receipt.request_wire, request);
            assert_eq!(receipt.response_wire, response);
        }
        assert!(
            store
                .channel_checkpoint(&[9; 32], &[8; 32])
                .unwrap()
                .is_none()
        );
        println!(
            "{boundary}: child-exit=86 committed={committed} exact-history=true new-epoch-unbound=true"
        );
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn journal_migration_process_crash_before_and_after_commit_reopens_same_store() {
    for (boundary, migration_committed) in [
        ("channel-journal-migration-before-commit", false),
        ("channel-journal-migration-after-commit", true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("migration-crash.sqlite");
        drop(Store::open(&path, EventBudget::default()).unwrap());
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("DROP TABLE IF EXISTS agent_ledger; DROP TABLE channel_ack_receipts; DROP TABLE channel_checkpoints; PRAGMA user_version=23;").unwrap();
        drop(db);
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "channel_checkpoint_crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_CHANNEL_CRASH_DB", &path)
            .env("MORROW_CHANNEL_MIGRATION_ONLY", "1")
            .env("MORROW_TEST_CRASH_AT", boundary)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(86),
            "{boundary}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let db = rusqlite::Connection::open(&path).unwrap();
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let tables: i64 = db.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('channel_checkpoints','channel_ack_receipts')", [], |row| row.get(0)).unwrap();
        assert_eq!(version, if migration_committed { 24 } else { 23 });
        assert_eq!(tables, if migration_committed { 2 } else { 0 });
        drop(db);
        let store = Store::open_existing(&path, EventBudget::default()).unwrap();
        store.integrity_check().unwrap();
        assert!(
            store
                .channel_checkpoint(&[9; 32], &[3; 32])
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .channel_ack_receipt(&[9; 32], &[3; 32], 1)
                .unwrap()
                .is_none()
        );
        println!(
            "{boundary}: child-exit=86 pre-reopen-version={version} journal-tables={tables} reopened-same-store=true cursor-absent=true"
        );
    }
}
