#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    blob_stage::Command,
    blob_upload_journal::{
        Execution, MAX_PAGE_SIZE, MAX_SUBMISSIONS, MetadataCompletion, OriginalResult, Outcome,
        RESERVED_HISTORY_BYTES, Request,
    },
    store::{EventBudget, SCHEMA_VERSION, Store},
};
use sha2::{Digest, Sha256};
use std::{cell::Cell, io::Cursor, path::PathBuf};

fn request(n: u64) -> Request {
    let mut submission = [5; 32];
    submission[24..].copy_from_slice(&n.to_be_bytes());
    Request {
        submission,
        package_id: "org.example.upload-history".into(),
        package_digest: [12; 32],
        registry_revision: 7,
        nomination_ref: [13; 32],
        command: Command {
            owner_domain: [14; 32],
            operation_id: format!("operation-{n}"),
            byte_length: 5,
            sha256: Sha256::digest(b"value").into(),
        },
        handler: "upload".into(),
        input_sha256: Sha256::digest(b"input").into(),
        binding_sha256: [15; 32],
    }
}
fn outcome(unknown: bool) -> OriginalResult {
    OriginalResult {
        outcome: if unknown {
            Outcome::Error(7)
        } else {
            Outcome::Staged
        },
        unknown,
        calls: 3,
        wire_bytes: 1000,
        native_work_bytes: 10,
        execution: Some(Execution {
            outcome: Ok(0),
            host_calls: 3,
        }),
    }
}
fn fixture() -> (tempfile::TempDir, PathBuf, Store) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("upload-history.db");
    let store = Store::open(&path, EventBudget::default()).unwrap();
    (root, path, store)
}
fn inspect(store: &Store, request: &Request) -> morrow_core::blob_upload_journal::Record {
    store
        .inspect_blob_upload_local_authorized(&request.submission, || Ok(()))
        .unwrap()
        .unwrap()
}
fn reserve(store: &mut Store, r: &Request) {
    store
        .reserve_blob_upload_local_authorized(r, 10, || Ok(()))
        .unwrap();
}

#[test]
fn metadata_completion_requires_exact_predecessor_and_preserves_unknown() {
    let (_root, path, mut store) = fixture();
    let (_other_root, _other_path, mut other) = fixture();
    let r = request(401);
    reserve(&mut store, &r);
    let before = inspect(&store, &r);
    let foreign = other
        .reserve_blob_upload_local_authorized(&r, 9, || Ok(()))
        .unwrap();
    let observation = MetadataCompletion::Observation {
        result: outcome(true),
        at: 11,
    };
    assert_eq!(
        store.complete_blob_upload_metadata_local_authorized(&foreign, observation, || Ok(())),
        Err(Error::OperationConflict)
    );
    assert_eq!(inspect(&store, &r), before);
    let recorded = store
        .complete_blob_upload_metadata_local_authorized(&before, observation, || Ok(()))
        .unwrap();
    assert!(recorded.original_unknown());
    assert_eq!(recorded.observed_at_unix_ms(), Some(11));
    assert_eq!(
        store.complete_blob_upload_metadata_local_authorized(&before, observation, || Ok(())),
        Err(Error::OperationConflict)
    );
    let ack = MetadataCompletion::Acknowledgement { at: 12 };
    let complete = store
        .complete_blob_upload_metadata_local_authorized(&recorded, ack, || Ok(()))
        .unwrap();
    assert_eq!(
        store.complete_blob_upload_metadata_local_authorized(&recorded, ack, || Ok(())),
        Err(Error::OperationConflict)
    );
    drop(store);
    let reopened = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(inspect(&reopened, &r), complete);
    assert!(complete.original_unknown());
    assert_eq!(complete.original_result(), Some(outcome(true)));
    assert_eq!(complete.revision(), 3);
    assert!(reopened.list_blobs_local("", 128).unwrap().is_empty());
    reopened.integrity_check().unwrap();
}

#[test]
fn metadata_completion_checks_authority_again_before_commit() {
    let (_root, _path, mut store) = fixture();
    let r = request(402);
    reserve(&mut store, &r);
    for completion in [
        MetadataCompletion::Observation {
            result: outcome(false),
            at: 11,
        },
        MetadataCompletion::Acknowledgement { at: 12 },
    ] {
        let before = inspect(&store, &r);
        let calls = Cell::new(0);
        let veto = || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                Err(Error::Invalid("revoked"))
            } else {
                Ok(())
            }
        };
        assert_eq!(
            store.complete_blob_upload_metadata_local_authorized(&before, completion, veto),
            Err(Error::Invalid("revoked"))
        );
        assert_eq!(calls.get(), 2);
        assert_eq!(
            inspect(&store, &r),
            before,
            "the partial append rolled back"
        );
        store
            .complete_blob_upload_metadata_local_authorized(&before, completion, || Ok(()))
            .unwrap();
    }
    assert_eq!(inspect(&store, &r).revision(), 3);
}

#[test]
fn reservation_survives_reopen_without_becoming_a_result_or_retry_permission() {
    let (_root, path, mut store) = fixture();
    let r = request(1);
    reserve(&mut store, &r);
    drop(store);
    let mut next = Store::open_existing(&path, Default::default()).unwrap();
    let history = inspect(&next, &r);
    assert!(history.original_unknown());
    assert_eq!(history.original_result(), None);
    assert_eq!(history.acknowledged_at_unix_ms(), None);
    assert!(next.inspect_blob_stage_local(&r.command).unwrap().is_none());
    assert_eq!(
        next.reserve_blob_upload_local_authorized(&r, 11, || Ok(())),
        Err(Error::OperationConflict)
    );
    assert_eq!(
        next.acknowledge_blob_upload_local_authorized(&r, 12, || Ok(())),
        Err(Error::OperationConflict)
    );
    assert_eq!(inspect(&next, &r), history);
}

#[test]
fn original_unknown_is_immutable_even_when_blob_exists_and_original_task_was_acknowledged() {
    let (_root, path, mut store) = fixture();
    let r = request(2);
    reserve(&mut store, &r);
    store
        .stage_blob_operation_local(&mut Cursor::new(b"value"), &r.command, 11)
        .unwrap();
    let original = outcome(true);
    store
        .observe_blob_upload_local_authorized(&r, original, 12, || Ok(()))
        .unwrap();
    store
        .acknowledge_blob_upload_local_authorized(&r, 13, || Ok(()))
        .unwrap();
    assert_eq!(
        store.observe_blob_upload_local_authorized(&r, outcome(false), 14, || Ok(())),
        Err(Error::OperationConflict)
    );
    assert_eq!(
        store.acknowledge_blob_upload_local_authorized(&r, 15, || Ok(())),
        Err(Error::OperationConflict)
    );
    drop(store);
    let reopened = Store::open_existing(&path, Default::default()).unwrap();
    let history = inspect(&reopened, &r);
    assert!(history.original_unknown());
    assert_eq!(history.original_result(), Some(original));
    assert_eq!(history.acknowledged_at_unix_ms(), Some(13));
    assert!(
        reopened
            .inspect_blob_stage_local(&r.command)
            .unwrap()
            .is_some()
    );
    assert_eq!(reopened.list_blobs_local("", 128).unwrap().len(), 1);
    assert_eq!(reopened.pending_usage().unwrap(), (0, 0));
    reopened.integrity_check().unwrap();
}

#[test]
fn append_requires_the_exact_original_package_domain_spec_and_submission() {
    let (_root, _path, mut store) = fixture();
    let r = request(3);
    reserve(&mut store, &r);
    for field in 0..12 {
        let mut wrong = r.clone();
        match field {
            0 => wrong.package_id.push('x'),
            1 => wrong.package_digest[0] ^= 1,
            2 => wrong.registry_revision += 1,
            3 => wrong.nomination_ref[0] ^= 1,
            4 => wrong.command.owner_domain[0] ^= 1,
            5 => wrong.command.operation_id.push('x'),
            6 => wrong.command.byte_length += 1,
            7 => wrong.command.sha256[0] ^= 1,
            8 => wrong.handler.push('x'),
            9 => wrong.input_sha256[0] ^= 1,
            10 => wrong.binding_sha256[0] ^= 1,
            _ => wrong.submission[0] ^= 1,
        }
        assert!(
            store
                .observe_blob_upload_local_authorized(&wrong, outcome(false), 11, || Ok(()))
                .is_err()
        );
        assert_eq!(inspect(&store, &r).revision(), 1);
    }
    store
        .observe_blob_upload_local_authorized(&r, outcome(false), 11, || Ok(()))
        .unwrap();
}

#[test]
fn invalid_result_and_clock_do_not_consume_the_original_observation_slot() {
    let (_root, _path, mut store) = fixture();
    let r = request(4);
    reserve(&mut store, &r);
    for bad in [
        OriginalResult {
            outcome: Outcome::Error(0),
            ..outcome(false)
        },
        OriginalResult {
            outcome: Outcome::Error(11),
            ..outcome(false)
        },
        OriginalResult {
            outcome: Outcome::Error(7),
            unknown: false,
            ..outcome(false)
        },
        OriginalResult {
            execution: Some(Execution {
                outcome: Err(10),
                host_calls: 0,
            }),
            ..outcome(false)
        },
    ] {
        assert!(
            store
                .observe_blob_upload_local_authorized(&r, bad, 11, || Ok(()))
                .is_err()
        );
    }
    assert!(
        store
            .observe_blob_upload_local_authorized(&r, outcome(false), 9, || Ok(()))
            .is_err()
    );
    assert_eq!(inspect(&store, &r).revision(), 1);
    store
        .observe_blob_upload_local_authorized(&r, outcome(false), 11, || Ok(()))
        .unwrap();
    assert!(
        store
            .acknowledge_blob_upload_local_authorized(&r, 10, || Ok(()))
            .is_err()
    );
    assert_eq!(inspect(&store, &r).revision(), 2);
}

#[test]
fn final_authority_veto_rolls_back_reservation_observation_and_acknowledgement() {
    let (_root, path, mut store) = fixture();
    let r = request(5);
    let calls = Cell::new(0);
    let veto = || {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            Err(Error::Invalid("revoked"))
        } else {
            Ok(())
        }
    };
    assert_eq!(
        store.reserve_blob_upload_local_authorized(&r, 10, veto),
        Err(Error::Invalid("revoked"))
    );
    assert!(
        store
            .inspect_blob_upload_local_authorized(&r.submission, || Ok(()))
            .unwrap()
            .is_none()
    );
    reserve(&mut store, &r);
    calls.set(0);
    assert_eq!(
        store.observe_blob_upload_local_authorized(&r, outcome(false), 11, veto),
        Err(Error::Invalid("revoked"))
    );
    assert_eq!(inspect(&store, &r).revision(), 1);
    store
        .observe_blob_upload_local_authorized(&r, outcome(false), 11, || Ok(()))
        .unwrap();
    calls.set(0);
    assert_eq!(
        store.acknowledge_blob_upload_local_authorized(&r, 12, veto),
        Err(Error::Invalid("revoked"))
    );
    assert_eq!(inspect(&store, &r).revision(), 2);
    drop(store);
    assert_eq!(
        inspect(
            &Store::open_existing(&path, Default::default()).unwrap(),
            &r
        )
        .revision(),
        2
    );
}

#[test]
fn bounded_read_pages_are_separate_from_live_bindings_and_cannot_free_capacity() {
    let (_root, _path, mut store) = fixture();
    for n in 1..=3 {
        reserve(&mut store, &request(n));
    }
    let first = store
        .list_blob_uploads_local_authorized(None, 2, || Ok(()))
        .unwrap();
    assert_eq!(first.len(), 2);
    let next = store
        .list_blob_uploads_local_authorized(Some(&first[1].request().submission), 2, || Ok(()))
        .unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].request(), &request(3));
    for limit in [0, MAX_PAGE_SIZE + 1] {
        assert_eq!(
            store.list_blob_uploads_local_authorized(None, limit, || Ok(())),
            Err(Error::Limit)
        );
    }
    let calls = Cell::new(0);
    let veto = || {
        calls.set(calls.get() + 1);
        if calls.get() == 2 {
            Err(Error::Invalid("revoked"))
        } else {
            Ok(())
        }
    };
    assert_eq!(
        store.list_blob_uploads_local_authorized(None, 2, veto),
        Err(Error::Invalid("revoked"))
    );
    calls.set(0);
    assert_eq!(
        store.inspect_blob_upload_local_authorized(&request(1).submission, veto),
        Err(Error::Invalid("revoked"))
    );
    let (_foreign_root, _foreign_path, other) = fixture();
    assert!(
        other
            .inspect_blob_upload_local_authorized(&request(1).submission, || Ok(()))
            .unwrap()
            .is_none()
    );
    assert!(
        other
            .list_blob_uploads_local_authorized(None, 2, || Ok(()))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store.reserve_blob_upload_local_authorized(&request(1), 11, || Ok(())),
        Err(Error::OperationConflict)
    );
}

#[test]
fn all_terminal_slots_are_precharged_and_acknowledgement_does_not_evict() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("quota.db");
    let mut store = Store::open(
        &path,
        EventBudget {
            max_count: 2,
            max_bytes: RESERVED_HISTORY_BYTES,
        },
    )
    .unwrap();
    let r = request(6);
    reserve(&mut store, &r);
    assert_eq!(
        store.reserve_blob_upload_local_authorized(&request(7), 10, || Ok(())),
        Err(Error::EventCapacity)
    );
    store
        .observe_blob_upload_local_authorized(&r, outcome(true), 11, || Ok(()))
        .unwrap();
    store
        .acknowledge_blob_upload_local_authorized(&r, 12, || Ok(()))
        .unwrap();
    assert_eq!(
        store.reserve_blob_upload_local_authorized(&request(7), 13, || Ok(())),
        Err(Error::EventCapacity)
    );
    assert!(inspect(&store, &r).original_unknown());
}

#[test]
fn the_submission_limit_remains_charged_after_terminal_records() {
    let (_root, _path, mut store) = fixture();
    for n in 0..MAX_SUBMISSIONS {
        reserve(&mut store, &request(n));
    }
    store
        .observe_blob_upload_local_authorized(&request(0), outcome(false), 11, || Ok(()))
        .unwrap();
    store
        .acknowledge_blob_upload_local_authorized(&request(0), 12, || Ok(()))
        .unwrap();
    assert_eq!(
        store.reserve_blob_upload_local_authorized(&request(MAX_SUBMISSIONS), 13, || Ok(())),
        Err(Error::Limit)
    );
    store.integrity_check().unwrap();
}

#[test]
fn concurrent_connections_cannot_reserve_the_same_submission_twice() {
    let (_root, path, mut store) = fixture();
    let mut second = Store::open_existing(&path, Default::default()).unwrap();
    let r = request(8);
    reserve(&mut store, &r);
    assert_eq!(
        second.reserve_blob_upload_local_authorized(&r, 10, || Ok(())),
        Err(Error::OperationConflict)
    );
    assert_eq!(inspect(&second, &r), inspect(&store, &r));
}

#[test]
fn malformed_oversized_or_truncated_history_is_rejected_on_reopen() {
    for mutation in [
        "UPDATE blob_upload_history SET payload=x'00' WHERE revision=2",
        "UPDATE blob_upload_history SET payload=zeroblob(8193) WHERE revision=2",
        "DELETE FROM blob_upload_history WHERE revision=2",
        "UPDATE blob_upload_requests SET revision=3",
        "UPDATE blob_upload_history SET submission=zeroblob(32)",
    ] {
        let (_root, path, mut store) = fixture();
        let r = request(9);
        reserve(&mut store, &r);
        store
            .observe_blob_upload_local_authorized(&r, outcome(false), 11, || Ok(()))
            .unwrap();
        drop(store);
        let raw = rusqlite::Connection::open(&path).unwrap();
        // Model offline damage, including an orphan that ordinary writers cannot
        // create. Production connections keep their foreign-key enforcement.
        raw.pragma_update(None, "foreign_keys", false).unwrap();
        raw.execute_batch(mutation).unwrap();
        drop(raw);
        assert!(
            Store::open_existing(&path, Default::default()).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn additive_v26_migration_preserves_blob_and_stage_history_without_inventing_a_task() {
    let (_root, path, mut store) = fixture();
    let r = request(10);
    let blob = store
        .stage_blob_operation_local(&mut Cursor::new(b"value"), &r.command, 1)
        .unwrap();
    drop(store);
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "DROP TABLE blob_upload_history; DROP TABLE blob_upload_requests; PRAGMA user_version=26;",
    )
    .unwrap();
    drop(raw);
    let store = Store::open_existing(&path, Default::default()).unwrap();
    assert!(
        store
            .list_blob_uploads_local_authorized(None, 1, || Ok(()))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .inspect_blob_stage_local(&r.command)
            .unwrap()
            .unwrap()
            .blob(),
        &blob
    );
    let mut bytes = Vec::new();
    store.export_blob_local(&blob.id, &mut bytes).unwrap();
    assert_eq!(bytes, b"value");
    store.integrity_check().unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        raw.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        SCHEMA_VERSION
    );
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "Only the bounded parent subprocess tests invoke this fresh fixture"]
fn upload_journal_fault_child() {
    let path = PathBuf::from(std::env::var_os("MORROW_UPLOAD_JOURNAL_DB").unwrap());
    let mode = std::env::var("MORROW_UPLOAD_JOURNAL_MODE").unwrap();
    let mut store = Store::open_existing(&path, Default::default()).unwrap();
    match mode.as_str() {
        "reserve" => {
            store
                .reserve_blob_upload_local_authorized(&request(11), 10, || Ok(()))
                .unwrap();
        }
        "observe" => {
            store
                .observe_blob_upload_local_authorized(&request(11), outcome(true), 11, || Ok(()))
                .unwrap();
        }
        "ack" => {
            store
                .acknowledge_blob_upload_local_authorized(&request(11), 12, || Ok(()))
                .unwrap();
        }
        "migrate" => {}
        _ => panic!("unexpected bounded child mode"),
    }
    panic!("the configured crash boundary did not execute");
}

#[cfg(feature = "fault-injection")]
fn crash(path: &std::path::Path, mode: &str, point: &str) {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "upload_journal_fault_child",
            "--ignored",
            "--nocapture",
        ])
        .env("MORROW_UPLOAD_JOURNAL_DB", path)
        .env("MORROW_UPLOAD_JOURNAL_MODE", mode)
        .env("MORROW_TEST_CRASH_AT", point)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(86));
}
#[cfg(feature = "fault-injection")]
#[test]
fn abrupt_exit_before_and_after_each_commit_retains_exact_original_progress() {
    for (mode, before_revision, after_revision) in
        [("reserve", 0, 1), ("observe", 1, 2), ("ack", 2, 3)]
    {
        for (side, expected) in [("before", before_revision), ("after", after_revision)] {
            let (_root, path, mut store) = fixture();
            let r = request(11);
            if before_revision >= 1 {
                reserve(&mut store, &r);
            }
            if before_revision >= 2 {
                store
                    .observe_blob_upload_local_authorized(&r, outcome(true), 11, || Ok(()))
                    .unwrap();
            }
            drop(store);
            crash(&path, mode, &format!("blob-upload-{mode}-{side}-commit"));
            let mut next = Store::open_existing(&path, Default::default()).unwrap();
            let record = next
                .inspect_blob_upload_local_authorized(&r.submission, || Ok(()))
                .unwrap();
            assert_eq!(record.as_ref().map_or(0, |r| r.revision()), expected);
            if let Some(record) = record {
                assert!(record.original_unknown());
                assert_eq!(
                    next.reserve_blob_upload_local_authorized(&r, 13, || Ok(())),
                    Err(Error::OperationConflict)
                );
            }
            assert!(next.inspect_blob_stage_local(&r.command).unwrap().is_none());
            next.integrity_check().unwrap();
        }
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn v27_migration_crash_is_atomic_and_never_fabricates_upload_history() {
    for (side, version) in [("before", 26), ("after", 27)] {
        let (_root, path, store) = fixture();
        drop(store);
        let raw = rusqlite::Connection::open(&path).unwrap();
        raw.execute_batch("DROP TABLE blob_upload_history; DROP TABLE blob_upload_requests; PRAGMA user_version=26;").unwrap();
        drop(raw);
        crash(
            &path,
            "migrate",
            &format!("blob-upload-journal-migration-{side}-commit"),
        );
        let raw = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            raw.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            version
        );
        drop(raw);
        let next = Store::open_existing(&path, Default::default()).unwrap();
        assert!(
            next.list_blob_uploads_local_authorized(None, 1, || Ok(()))
                .unwrap()
                .is_empty()
        );
        next.integrity_check().unwrap();
    }
}
