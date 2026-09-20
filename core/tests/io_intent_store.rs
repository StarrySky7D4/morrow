//! Store and signed-history qualification using synthetic IO facts only.
//! No backend dispatch, external effect, response authenticity or recovered grant
//! is established by these tests or by replaying a historical committed record.
#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    audit::{self, SigningKey, TrustedLog},
    content::CardRecord,
    io_intent::{self, Command, ObservationSource, Phase, Record, Recovery},
    plugin_package::io,
    store::{EventBudget, Store},
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn candidate(operation: &str) -> Record {
    Record::prepared(Command {
        operation_id: operation.into(),
        subject: "plugin.io-test".into(),
        package_sha256: [1; 32],
        capability: io::IoCapability::HttpRequest,
        protocol_sha256: io::schema_digest(),
        request_sha256: [2; 32],
        approval_sha256: [3; 32],
        target_sha256: [4; 32],
        request_bytes: 27,
        response_limit: 4096,
    })
    .unwrap()
}
fn setup() -> (tempfile::TempDir, PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("io.db");
    let store = Store::open(&path, EventBudget::default()).unwrap();
    (dir, path, store)
}
fn append(store: &mut Store, record: &Record) -> Record {
    store
        .append_io_intent_local_authorized(record, || Ok(()))
        .unwrap()
}
fn latest(store: &Store, operation: &str) -> Record {
    store
        .lookup_io_intent("plugin.io-test", operation)
        .unwrap()
        .unwrap()
}
fn count(path: &Path, table: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
fn equivalent(actual: &Record, expected: &Record) {
    assert_eq!(actual.raw(), expected.raw());
    assert_eq!(actual.container(), expected.container());
    assert_eq!(actual.digest(), expected.digest());
    assert_eq!(actual.command(), expected.command());
    assert_eq!(actual.event_id(), expected.event_id());
}
fn decoded(value: &io_intent::proto::Record) -> Record {
    let raw = value.encode_to_vec();
    let compressed = lz4_flex::block::compress(&raw);
    let mut container = io_intent::MAGIC.to_vec();
    container.extend_from_slice(&1u16.to_le_bytes());
    container.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    container.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    container.extend_from_slice(&Sha256::digest(&raw));
    container.extend_from_slice(&compressed);
    Record::decode(&container).unwrap()
}

#[test]
fn reopening_and_exact_historical_retry_preserve_originals_without_new_events() {
    let (_dir, path, mut store) = setup();
    let prepared = candidate("restart");
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let observed = unknown
        .propose_observation([5; 32], ObservationSource::OriginalResponse)
        .unwrap();
    let records = [&prepared, &unknown, &observed];
    for (index, record) in records.iter().enumerate() {
        if index == 1 {
            let reservation = store
                .reserve_io_intent_followup(prepared.command(), || Ok(()))
                .unwrap();
            assert_eq!(reservation.events, 1);
            assert_eq!(reservation.bytes, io_intent::MAX_CONTAINER_BYTES as u64);
        }
        equivalent(&append(&mut store, record), record);
        drop(store);
        store = Store::open_existing(&path, Default::default()).unwrap();
        equivalent(&latest(&store, "restart"), record);
    }
    // The terminal Observed revision consumed the held follow-up quota.
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    let original_pending = store.pending(0, 10).unwrap();
    assert_eq!(original_pending.len(), 3);
    for (event, record) in original_pending.iter().zip(records) {
        assert_eq!(event.1, record.container());
    }
    let usage = store.pending_usage().unwrap();
    for record in records {
        equivalent(&append(&mut store, record), record);
        equivalent(&latest(&store, "restart"), &observed);
    }
    assert_eq!(store.pending_usage().unwrap(), usage);
    assert_eq!(store.pending(0, 10).unwrap(), original_pending);
    assert_eq!(count(&path, "operations"), 3);
    assert_eq!(count(&path, "cards"), 0);
    assert_ne!(prepared.event_id(), unknown.event_id());
    assert_ne!(unknown.event_id(), observed.event_id());
    store.integrity_check().unwrap();
}

#[test]
fn immutable_command_predecessor_and_first_prepared_are_checked_before_publication() {
    let (_dir, path, mut store) = setup();
    let prepared = candidate("cas");
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    assert!(
        store
            .append_io_intent_local_authorized(&unknown, || Ok(()))
            .is_err()
    );
    assert!(
        store
            .lookup_io_intent("plugin.io-test", "cas")
            .unwrap()
            .is_none()
    );
    assert_eq!(count(&path, "operations"), 0);
    append(&mut store, &prepared);
    let baseline = store.pending(0, 10).unwrap();
    let mut changed = prepared.command().clone();
    changed.request_sha256 = [77; 32];
    assert!(
        store
            .append_io_intent_local_authorized(&Record::prepared(changed).unwrap(), || Ok(()))
            .is_err()
    );
    let mut changed = prepared.command().clone();
    changed.subject = "other.plugin".into();
    assert!(
        store
            .append_io_intent_local_authorized(&Record::prepared(changed).unwrap(), || Ok(()))
            .is_err()
    );
    assert!(!matches!(
        store.lookup_io_intent("other.plugin", "cas"),
        Ok(Some(_))
    ));
    let mut forged = unknown.data().clone();
    forged.previous_sha256 = vec![88; 32];
    assert!(
        store
            .append_io_intent_local_authorized(&decoded(&forged), || Ok(()))
            .is_err()
    );
    let observed = unknown
        .propose_observation([5; 32], ObservationSource::OriginalResponse)
        .unwrap();
    assert!(
        store
            .append_io_intent_local_authorized(&observed, || Ok(()))
            .is_err()
    );
    equivalent(&latest(&store, "cas"), &prepared);
    assert_eq!(store.pending(0, 10).unwrap(), baseline);
    store
        .reserve_io_intent_followup(prepared.command(), || Ok(()))
        .unwrap();
    append(&mut store, &unknown);
    let competing = prepared.propose_cancel_before_dispatch().unwrap();
    assert!(
        store
            .append_io_intent_local_authorized(&competing, || Ok(()))
            .is_err()
    );
    equivalent(&latest(&store, "cas"), &unknown);
    assert_eq!(count(&path, "operations"), 2);
    store.integrity_check().unwrap();
}

#[test]
fn every_commit_and_historical_retry_require_current_authorization() {
    let (_dir, path, mut store) = setup();
    let prepared = candidate("authorization");
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    assert!(matches!(
        store.append_io_intent_local_authorized(&prepared, || Err(Error::Invalid(
            "test authorization denied"
        ))),
        Err(Error::Invalid("test authorization denied"))
    ));
    assert!(
        store
            .lookup_io_intent("plugin.io-test", "authorization")
            .unwrap()
            .is_none()
    );
    assert_eq!(count(&path, "operations"), 0);
    append(&mut store, &prepared);
    let pending = store.pending(0, 10).unwrap();
    // Reserving the post-dispatch quota is itself a current-authorization event.
    let mut calls = 0;
    assert!(matches!(
        store.reserve_io_intent_followup(prepared.command(), || {
            calls += 1;
            Err(Error::Invalid("test authorization denied"))
        }),
        Err(Error::Invalid("test authorization denied"))
    ));
    assert_eq!(calls, 1);
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    store
        .reserve_io_intent_followup(prepared.command(), || Ok(()))
        .unwrap();
    for record in [&unknown, &prepared] {
        let mut calls = 0;
        assert!(matches!(
            store.append_io_intent_local_authorized(record, || {
                calls += 1;
                Err(Error::Invalid("test authorization denied"))
            }),
            Err(Error::Invalid("test authorization denied"))
        ));
        assert_eq!(calls, 1);
        equivalent(&latest(&store, "authorization"), &prepared);
        assert_eq!(store.pending(0, 10).unwrap(), pending);
    }
    append(&mut store, &unknown);
    assert!(
        store
            .append_io_intent_local_authorized(&prepared, || Err(Error::Invalid(
                "test authorization denied"
            )))
            .is_err()
    );
    equivalent(&latest(&store, "authorization"), &unknown);
    assert_eq!(count(&path, "operations"), 2);
    store.integrity_check().unwrap();
}

#[test]
fn event_count_and_byte_capacity_roll_back_and_do_not_block_exact_retry() {
    let (_dir, path, store) = setup();
    drop(store);
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 1,
            max_bytes: 1_000_000,
        },
    )
    .unwrap();
    let prepared = candidate("bounded");
    append(&mut store, &prepared);
    let baseline = store.pending(0, 10).unwrap();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    // The reservation is drawn from the same logical budget; without room it
    // fails atomically and never holds a partial deduction.
    assert!(matches!(
        store.reserve_io_intent_followup(prepared.command(), || Ok(())),
        Err(Error::EventCapacity)
    ));
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    assert!(matches!(
        store.append_io_intent_local_authorized(&unknown, || Ok(())),
        Err(Error::Invalid("IO intent dispatch reservation"))
    ));
    assert!(matches!(
        store.append_io_intent_local_authorized(&candidate("other"), || Ok(())),
        Err(Error::EventCapacity)
    ));
    equivalent(&latest(&store, "bounded"), &prepared);
    assert!(
        store
            .lookup_io_intent("plugin.io-test", "other")
            .unwrap()
            .is_none()
    );
    equivalent(&append(&mut store, &prepared), &prepared);
    assert_eq!(store.pending(0, 10).unwrap(), baseline);
    drop(store);
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 100,
            max_bytes: baseline[0].1.len() as u64,
        },
    )
    .unwrap();
    assert!(matches!(
        store.reserve_io_intent_followup(prepared.command(), || Ok(())),
        Err(Error::EventCapacity)
    ));
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    equivalent(&append(&mut store, &prepared), &prepared);
    assert_eq!(store.pending(0, 10).unwrap(), baseline);
    store.integrity_check().unwrap();
}

#[test]
fn recovered_unknown_is_reconcile_only_and_pre_dispatch_cancellation_is_terminal() {
    let (_dir, path, mut store) = setup();
    let prepared = candidate("unknown");
    append(&mut store, &prepared);
    store
        .reserve_io_intent_followup(prepared.command(), || Ok(()))
        .unwrap();
    append(&mut store, &prepared.propose_dispatch_boundary().unwrap());
    let cancelled = candidate("cancelled");
    append(&mut store, &cancelled);
    store
        .reserve_io_intent_followup(cancelled.command(), || Ok(()))
        .unwrap();
    append(&mut store, &cancelled.propose_cancel_before_dispatch().unwrap());
    // Pre-dispatch cancellation released its reservation; the dispatched
    // operation still holds exactly its own.
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, io_intent::MAX_CONTAINER_BYTES as u64)
    );
    drop(store);
    let mut store = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, io_intent::MAX_CONTAINER_BYTES as u64)
    );
    let unknown = latest(&store, "unknown");
    assert_eq!(unknown.recovery(), Recovery::ReconcileOnly);
    assert!(unknown.propose_dispatch_boundary().is_err());
    assert!(unknown.propose_cancel_before_dispatch().is_err());
    let receipt = append(&mut store, &unknown);
    assert_eq!(receipt.recovery(), Recovery::ReconcileOnly); // History is not dispatch permission.
    let observed = unknown
        .propose_observation([6; 32], ObservationSource::Reconciliation)
        .unwrap();
    append(&mut store, &observed);
    assert_eq!(latest(&store, "unknown").phase(), Phase::Observed);
    // Terminal observation consumed the last held quota.
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    let cancelled = latest(&store, "cancelled");
    assert_eq!(cancelled.recovery(), Recovery::CancelledBeforeDispatch);
    assert!(cancelled.propose_dispatch_boundary().is_err());
    assert!(
        cancelled
            .propose_observation([7; 32], ObservationSource::Reconciliation)
            .is_err()
    );
    store.integrity_check().unwrap();
}

#[test]
fn ordinary_operation_ids_and_revision_event_ids_cannot_collide_in_either_direction() {
    for event_id in [false, true] {
        let (_dir, path, mut store) = setup();
        let prepared = candidate("collision");
        let id = if event_id {
            prepared.event_id().to_owned()
        } else {
            prepared.command().operation_id.clone()
        };
        let card = CardRecord::new("card", "test.card", 1, "Title", vec![]).unwrap();
        store.create_local(&id, &card).unwrap();
        assert!(
            store
                .append_io_intent_local_authorized(&prepared, || Ok(()))
                .is_err()
        );
        assert!(
            store
                .lookup_io_intent("plugin.io-test", "collision")
                .unwrap()
                .is_none()
        );
        assert_eq!(count(&path, "operations"), 1);
        store.integrity_check().unwrap();
    }
    for event_id in [false, true] {
        let (_dir, path, mut store) = setup();
        let prepared = candidate("collision");
        append(&mut store, &prepared);
        let id = if event_id {
            prepared.event_id().to_owned()
        } else {
            prepared.command().operation_id.clone()
        };
        let card = CardRecord::new("card", "test.card", 1, "Title", vec![]).unwrap();
        assert!(store.create_local(&id, &card).is_err());
        assert!(store.card("card").unwrap().is_none());
        equivalent(&latest(&store, "collision"), &prepared);
        assert_eq!(count(&path, "operations"), 1);
        store.integrity_check().unwrap();
    }
}

#[test]
fn signed_events_snapshot_and_reopen_keep_every_original_container() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("signed.db");
    let snapshot = dir.path().join("snapshot.db");
    let key = SigningKey::from_bytes(&[59; 32]); // Synthetic qualification key only.
    let trust = TrustedLog {
        id: "io-test-log".into(),
        key: key.verifying_key(),
    };
    let mut store = Store::open_audited(&path, Default::default(), true, trust.clone()).unwrap();
    let first = candidate("signed");
    let second = first.propose_dispatch_boundary().unwrap();
    let third = second
        .propose_observation([9; 32], ObservationSource::Reconciliation)
        .unwrap();
    let records = [&first, &second, &third];
    append(&mut store, &first);
    store
        .reserve_io_intent_followup(first.command(), || Ok(()))
        .unwrap();
    append(&mut store, &second);
    append(&mut store, &third);
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    let pending = store.pending(0, 10).unwrap();
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &pending).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    let verified = audit::verify(&signed, &trust).unwrap();
    assert_eq!(verified.segment().events.len(), 3);
    for (event, record) in verified.segment().events.iter().zip(records) {
        assert_eq!(event.operation_id, record.event_id());
        assert_eq!(event.original_commit, record.container());
    }
    store.seal_pending(&signed).unwrap();
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    store.snapshot_to(&snapshot, 16 * 1024 * 1024).unwrap();
    drop(store);
    let mut restored =
        Store::open_audited(&snapshot, Default::default(), false, trust.clone()).unwrap();
    let saved_signed = restored.sealed_segment(1).unwrap().unwrap();
    assert_eq!(saved_signed, signed);
    audit::verify(&saved_signed, &trust).unwrap();
    // No reservation may survive into a snapshot after terminal consumption.
    assert_eq!(restored.io_intent_reservation_usage().unwrap(), (0, 0));
    equivalent(&latest(&restored, "signed"), &third);
    for record in records {
        equivalent(&append(&mut restored, record), record);
    }
    assert_eq!(restored.pending_usage().unwrap(), (0, 0));
    assert_eq!(count(&snapshot, "operations"), 3);
    restored.integrity_check().unwrap();
}

#[test]
fn missing_predecessor_and_orphan_io_event_reject_integrity_and_snapshot() {
    for remove_predecessor in [false, true] {
        let (_dir, path, mut store) = setup();
        let prepared = candidate("corrupted");
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        append(&mut store, &prepared);
        store
            .reserve_io_intent_followup(prepared.command(), || Ok(()))
            .unwrap();
        append(&mut store, &unknown);
        let external = rusqlite::Connection::open(&path).unwrap();
        if remove_predecessor {
            // A synthetic on-disk corruption removes the predecessor completely,
            // leaving a structurally decodable Unknown with no historical basis.
            external
                .execute(
                    "DELETE FROM io_intents WHERE operation_id=?1 AND revision=1",
                    ["corrupted"],
                )
                .unwrap();
            for table in ["outbox", "operation_events", "operations"] {
                external
                    .execute(
                        &format!("DELETE FROM {table} WHERE id=?1"),
                        [prepared.event_id()],
                    )
                    .unwrap();
            }
        } else {
            // The kind-5 operation remains, but its required history link is gone.
            external
                .execute(
                    "DELETE FROM io_intents WHERE operation_id=?1 AND revision=2",
                    ["corrupted"],
                )
                .unwrap();
        }
        drop(external);
        assert!(store.integrity_check().is_err());
        assert!(
            store
                .snapshot_to(
                    &path.with_file_name("invalid-snapshot.db"),
                    16 * 1024 * 1024
                )
                .is_err()
        );
        drop(store);
        assert!(Store::open_existing(&path, Default::default()).is_err());
    }
}

#[test]
fn version_fourteen_migrates_without_rewriting_existing_content_or_pending_bytes() {
    let (_dir, path, mut store) = setup();
    let card = CardRecord::new("legacy", "test.card", 1, "Legacy", b"body".to_vec()).unwrap();
    store.create_local("old-create", &card).unwrap();
    let original = store.pending(0, 10).unwrap();
    drop(store);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE IF EXISTS service_authority_identity; DROP TABLE IF EXISTS service_authorities; DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; DROP TABLE io_reservations; DROP TABLE io_intents; PRAGMA user_version=14;")
        .unwrap();
    let mut migrated = Store::open_existing(&path, Default::default()).unwrap();
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, morrow_core::store::SCHEMA_VERSION);
    assert_eq!(migrated.pending(0, 10).unwrap(), original);
    assert_eq!(
        migrated.card("legacy").unwrap().unwrap().encode(),
        card.encode()
    );
    append(&mut migrated, &candidate("new-io"));
    assert_eq!(migrated.pending(0, 10).unwrap()[0], original[0]);
    migrated.integrity_check().unwrap();
}
