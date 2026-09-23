//! Pre-dispatch follow-up reservation qualification using synthetic IO facts.
//! Reservations are a logical Store quota only: no dispatch happens, no physical
//! disk-space guarantee is claimed, and a held reservation never authorizes IO.
#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    audit::{self, SigningKey, TrustedLog},
    content::CardRecord,
    io_intent::{self, Command, ObservationSource, Phase, Record},
    plugin_package::io,
    store::{EventBudget, Store},
};
use std::path::{Path, PathBuf};

const RESERVATION_BYTES: u64 = io_intent::MAX_CONTAINER_BYTES as u64;

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
fn reserve(store: &mut Store, command: &Command) {
    store.reserve_io_intent_followup(command, || Ok(())).unwrap();
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
fn card(store: &mut Store, id: &str) {
    let record = CardRecord::new(id, "test.card", 1, "Title", vec![]).unwrap();
    store.create_local(id, &record).unwrap();
}

#[test]
fn boundary_refuses_without_reservation_and_consumes_exactly_once() {
    let (_dir, path, mut store) = setup();
    let prepared = candidate("enforce");
    append(&mut store, &prepared);
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    assert!(matches!(
        store.append_io_intent_local_authorized(&unknown, || Ok(())),
        Err(Error::Invalid("IO intent dispatch reservation"))
    ));
    assert_eq!(count(&path, "operations"), 1);
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    reserve(&mut store, prepared.command());
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    append(&mut store, &unknown);
    // The boundary itself does not consume; the reservation is still held for
    // the post-send observation.
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    // From OutcomeUnknown the only legal successor is Observed.
    let competing = prepared.propose_cancel_before_dispatch().unwrap();
    assert!(
        store
            .append_io_intent_local_authorized(&competing, || Ok(()))
            .is_err()
    );
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    let observed = unknown
        .propose_observation([6; 32], ObservationSource::Reconciliation)
        .unwrap();
    append(&mut store, &observed);
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    assert_eq!(latest(&store, "enforce").phase(), Phase::Observed);
    // The freed slot is genuinely available to a brand-new operation again.
    let next = candidate("enforce-2");
    append(&mut store, &next);
    assert_eq!(count(&path, "operations"), 4);
    store.integrity_check().unwrap();
}

#[test]
fn reservation_is_idempotent_command_bound_and_phase_bound() {
    let (_dir, _path, mut store) = setup();
    let prepared = candidate("binding");
    assert!(matches!(
        store.reserve_io_intent_followup(prepared.command(), || Ok(())),
        Err(Error::NotFound)
    ));
    append(&mut store, &prepared);
    let first = store
        .reserve_io_intent_followup(prepared.command(), || Ok(()))
        .unwrap();
    let again = store
        .reserve_io_intent_followup(prepared.command(), || Ok(()))
        .unwrap();
    assert_eq!(first, again);
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    let mut other = prepared.command().clone();
    other.request_sha256 = [77; 32];
    assert!(matches!(
        store.reserve_io_intent_followup(&other, || Ok(())),
        Err(Error::OperationConflict)
    ));
    let mut stranger = prepared.command().clone();
    stranger.subject = "plugin.stranger".into();
    assert!(matches!(
        store.reserve_io_intent_followup(&stranger, || Ok(())),
        Err(Error::OperationConflict)
    ));
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    append(&mut store, &prepared.propose_dispatch_boundary().unwrap());
    assert!(matches!(
        store.reserve_io_intent_followup(prepared.command(), || Ok(())),
        Err(Error::Invalid("IO intent reservation phase"))
    ));
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    store.integrity_check().unwrap();
}

#[test]
fn reservation_counts_against_every_other_writer_until_terminal() {
    let (_dir, path, store) = setup();
    drop(store);
    // max_count=4: one slot is pinned by the reservation, so only three real
    // events fit while it is held.
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 4,
            max_bytes: 64 * 1024 * 1024,
        },
    )
    .unwrap();
    let prepared = candidate("quota");
    append(&mut store, &prepared);
    reserve(&mut store, prepared.command());
    card(&mut store, "c1");
    card(&mut store, "c2");
    // Third real event would collide with the pinned slot.
    let blocked = CardRecord::new("c3", "test.card", 1, "Title", vec![]).unwrap();
    assert!(matches!(
        store.create_local("c3", &blocked),
        Err(Error::EventCapacity)
    ));
    // The boundary event is a real pre-send event and needs real headroom; the
    // reservation only protects the post-send observation.
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    assert!(matches!(
        store.append_io_intent_local_authorized(&unknown, || Ok(())),
        Err(Error::EventCapacity)
    ));
    // Pre-dispatch cancellation always fits: it releases the reservation first.
    append(&mut store, &prepared.propose_cancel_before_dispatch().unwrap());
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    store.integrity_check().unwrap();
}

#[test]
fn byte_reservation_blocks_other_bytes_and_survives_sealing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bytes.db");
    let key = SigningKey::from_bytes(&[61; 32]); // Synthetic qualification key only.
    let trust = TrustedLog {
        id: "io-test-log".into(),
        key: key.verifying_key(),
    };
    let prepared = candidate("bytes");
    let mut store = Store::open_audited(
        &path,
        EventBudget {
            max_count: 1024,
            // Exactly the prepared event plus the reserved follow-up.
            max_bytes: prepared.container().len() as u64 + RESERVATION_BYTES,
        },
        true,
        trust.clone(),
    )
    .unwrap();
    append(&mut store, &prepared);
    reserve(&mut store, prepared.command());
    // Other writers cannot consume the reserved bytes.
    let blocked = CardRecord::new("byte-card", "test.card", 1, "Title", vec![]).unwrap();
    assert!(matches!(
        store.create_local("byte-card", &blocked),
        Err(Error::EventCapacity)
    ));
    // Sealing real pending events does not dissolve the held reservation.
    let pending = store.pending(0, 10).unwrap();
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &pending).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    store.seal_pending(&signed).unwrap();
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    drop(store);
    let mut store = Store::open_audited(&path, Default::default(), false, trust).unwrap();
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    // Pre-dispatch cancellation releases the reservation and fits because the
    // reservation is removed before the cancel event is measured.
    append(&mut store, &prepared.propose_cancel_before_dispatch().unwrap());
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    store.integrity_check().unwrap();
}

#[test]
fn held_reservation_guarantees_observation_even_under_lower_reopened_budget() {
    let (_dir, path, mut store) = setup();
    let prepared = candidate("promise");
    append(&mut store, &prepared);
    reserve(&mut store, prepared.command());
    append(&mut store, &prepared.propose_dispatch_boundary().unwrap());
    drop(store);
    // The budget is per-open configuration, never a persisted quota: existing
    // reservations keep their exactly bounded promise, while new grants and new
    // events must fit the lowered budget.
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 1024,
            max_bytes: 1,
        },
    )
    .unwrap();
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    let unknown = latest(&store, "promise");
    let observed = unknown
        .propose_observation([8; 32], ObservationSource::Reconciliation)
        .unwrap();
    append(&mut store, &observed);
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    let fresh = candidate("promise-2");
    assert!(matches!(
        store.append_io_intent_local_authorized(&fresh, || Ok(())),
        Err(Error::EventCapacity)
    ));
    store.integrity_check().unwrap();
}

#[test]
fn two_connections_observe_one_shared_logical_quota() {
    let (_dir, path, store) = setup();
    drop(store);
    let mut writer = Store::open_existing(
        &path,
        EventBudget {
            max_count: 4,
            max_bytes: 64 * 1024 * 1024,
        },
    )
    .unwrap();
    let prepared = candidate("shared");
    append(&mut writer, &prepared);
    reserve(&mut writer, prepared.command());
    // A completely independent connection sees and obeys the same deduction.
    let mut reader = Store::open_existing(
        &path,
        EventBudget {
            max_count: 4,
            max_bytes: 64 * 1024 * 1024,
        },
    )
    .unwrap();
    assert_eq!(
        reader.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    card(&mut reader, "s1");
    card(&mut reader, "s2");
    let blocked = CardRecord::new("s3", "test.card", 1, "Title", vec![]).unwrap();
    assert!(matches!(
        reader.create_local("s3", &blocked),
        Err(Error::EventCapacity)
    ));
    // Releasing the pre-dispatch reservation frees exactly one slot for the
    // other connection, without any event being written.
    writer
        .release_io_intent_followup("plugin.io-test", "shared")
        .unwrap();
    assert_eq!(writer.io_intent_reservation_usage().unwrap(), (0, 0));
    card(&mut reader, "s3");
    assert_eq!(
        reader.io_intent_reservation_usage().unwrap(),
        writer.io_intent_reservation_usage().unwrap()
    );
    reader.integrity_check().unwrap();
    writer.integrity_check().unwrap();
}

#[test]
fn held_reservation_protects_the_observation_slot_from_competition() {
    let (_dir, path, store) = setup();
    drop(store);
    let mut writer = Store::open_existing(
        &path,
        EventBudget {
            max_count: 6,
            max_bytes: 64 * 1024 * 1024,
        },
    )
    .unwrap();
    let prepared = candidate("race");
    append(&mut writer, &prepared);
    reserve(&mut writer, prepared.command());
    append(&mut writer, &prepared.propose_dispatch_boundary().unwrap());
    let mut reader = Store::open_existing(
        &path,
        EventBudget {
            max_count: 6,
            max_bytes: 64 * 1024 * 1024,
        },
    )
    .unwrap();
    card(&mut reader, "r1");
    card(&mut reader, "r2");
    card(&mut reader, "r3");
    // The last real slot is blocked: one slot stays pinned for the post-send
    // observation and other writers cannot consume it.
    let blocked = CardRecord::new("r4", "test.card", 1, "Title", vec![]).unwrap();
    assert!(matches!(
        reader.create_local("r4", &blocked),
        Err(Error::EventCapacity)
    ));
    let unknown = latest(&writer, "race");
    append(
        &mut writer,
        &unknown
            .propose_observation([15; 32], ObservationSource::Reconciliation)
            .unwrap(),
    );
    assert_eq!(writer.io_intent_reservation_usage().unwrap(), (0, 0));
    // The real queue is now genuinely full; sealing, not reservation tricks,
    // is the way to make room.
    assert!(matches!(
        reader.create_local("r4", &blocked),
        Err(Error::EventCapacity)
    ));
    reader.integrity_check().unwrap();
    writer.integrity_check().unwrap();
}

#[test]
fn release_only_works_before_dispatch_and_cancel_terminates_cleanly() {
    let (_dir, path, mut store) = setup();
    let prepared = candidate("release");
    append(&mut store, &prepared);
    reserve(&mut store, prepared.command());
    store
        .release_io_intent_followup("plugin.io-test", "release")
        .unwrap();
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    // Double release is idempotent.
    store
        .release_io_intent_followup("plugin.io-test", "release")
        .unwrap();
    assert!(matches!(
        store.release_io_intent_followup("plugin.stranger", "release"),
        Err(Error::NotFound)
    ));
    assert!(matches!(
        store.release_io_intent_followup("plugin.io-test", "absent"),
        Err(Error::NotFound)
    ));
    assert!(matches!(
        store.append_io_intent_local_authorized(
            &prepared.propose_dispatch_boundary().unwrap(),
            || Ok(())
        ),
        Err(Error::Invalid("IO intent dispatch reservation"))
    ));
    // After the boundary, release is refused: the observation keeps its quota.
    reserve(&mut store, prepared.command());
    append(&mut store, &prepared.propose_dispatch_boundary().unwrap());
    assert!(matches!(
        store.release_io_intent_followup("plugin.io-test", "release"),
        Err(Error::Invalid("IO intent reservation terminal"))
    ));
    let unknown = latest(&store, "release");
    append(
        &mut store,
        &unknown
            .propose_observation([11; 32], ObservationSource::Reconciliation)
            .unwrap(),
    );
    // Cancellation is terminal for later release attempts too.
    let cancelled = candidate("release-2");
    append(&mut store, &cancelled);
    reserve(&mut store, cancelled.command());
    append(&mut store, &cancelled.propose_cancel_before_dispatch().unwrap());
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    assert!(matches!(
        store.release_io_intent_followup("plugin.io-test", "release-2"),
        Err(Error::Invalid("IO intent reservation terminal"))
    ));
    assert_eq!(count(&path, "io_reservations"), 0);
    store.integrity_check().unwrap();
}

#[test]
fn authorize_failure_rolls_the_reservation_back_atomically() {
    let (_dir, path, mut store) = setup();
    let prepared = candidate("rollback");
    append(&mut store, &prepared);
    let mut calls = 0;
    assert!(matches!(
        store.reserve_io_intent_followup(prepared.command(), || {
            calls += 1;
            Err(Error::Invalid("test authorization denied"))
        }),
        Err(Error::Invalid("test authorization denied"))
    ));
    assert_eq!(calls, 1);
    assert_eq!(count(&path, "io_reservations"), 0);
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    let mut calls = 0;
    store
        .reserve_io_intent_followup(prepared.command(), || {
            calls += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(
        store.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    store.integrity_check().unwrap();
}

#[test]
fn snapshot_at_prepared_phase_restores_reservation_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.db");
    let snapshot = dir.path().join("snapshot.db");
    let mut store = Store::open(&path, Default::default()).unwrap();
    let prepared = candidate("snapshot");
    append(&mut store, &prepared);
    reserve(&mut store, prepared.command());
    store.snapshot_to(&snapshot, 16 * 1024 * 1024).unwrap();
    drop(store);
    let mut restored = Store::open_existing(&snapshot, Default::default()).unwrap();
    assert_eq!(
        restored.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    // Re-reserving the restored operation must not deduct twice.
    reserve(&mut restored, prepared.command());
    assert_eq!(
        restored.io_intent_reservation_usage().unwrap(),
        (1, RESERVATION_BYTES)
    );
    append(&mut restored, &prepared.propose_dispatch_boundary().unwrap());
    let unknown = latest(&restored, "snapshot");
    append(
        &mut restored,
        &unknown
            .propose_observation([12; 32], ObservationSource::Reconciliation)
            .unwrap(),
    );
    assert_eq!(restored.io_intent_reservation_usage().unwrap(), (0, 0));
    restored.integrity_check().unwrap();
}

#[test]
fn version_fifteen_with_prepared_intent_migrates_and_then_reserves() {
    let (_dir, path, mut store) = setup();
    let prepared = candidate("legacy15");
    append(&mut store, &prepared);
    drop(store);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE IF EXISTS tls_identities; DROP TABLE IF EXISTS outbound_authorities; DROP TABLE IF EXISTS service_authority_identity; DROP TABLE IF EXISTS service_authorities; DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; DROP TABLE io_reservations; PRAGMA user_version=15;")
        .unwrap();
    let mut migrated = Store::open_existing(&path, Default::default()).unwrap();
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, morrow_core::store::SCHEMA_VERSION);
    equivalent_latest(&migrated, &prepared);
    reserve(&mut migrated, prepared.command());
    append(&mut migrated, &prepared.propose_dispatch_boundary().unwrap());
    let unknown = latest(&migrated, "legacy15");
    append(
        &mut migrated,
        &unknown
            .propose_observation([13; 32], ObservationSource::Reconciliation)
            .unwrap(),
    );
    assert_eq!(migrated.io_intent_reservation_usage().unwrap(), (0, 0));
    migrated.integrity_check().unwrap();
}
fn equivalent_latest(store: &Store, expected: &Record) {
    let actual = latest(store, expected.command().operation_id.as_str());
    assert_eq!(actual.raw(), expected.raw());
}

#[test]
fn tampered_reservation_state_is_rejected_by_integrity() {
    // Deleting the reservation after the dispatch boundary breaks the closure.
    let (_dir, path, mut store) = setup();
    let prepared = candidate("tamper-1");
    append(&mut store, &prepared);
    reserve(&mut store, prepared.command());
    append(&mut store, &prepared.propose_dispatch_boundary().unwrap());
    drop(store);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute("DELETE FROM io_reservations", [])
        .unwrap();
    assert!(Store::open_existing(&path, Default::default()).is_err());
    // Terminal phases and unknown operations must never hold reservations.
    let (_dir, path, mut store) = setup();
    let prepared = candidate("tamper-2");
    append(&mut store, &prepared);
    reserve(&mut store, prepared.command());
    append(&mut store, &prepared.propose_dispatch_boundary().unwrap());
    let unknown = latest(&store, "tamper-2");
    append(
        &mut store,
        &unknown
            .propose_observation([14; 32], ObservationSource::Reconciliation)
            .unwrap(),
    );
    drop(store);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO io_reservations(operation_id,subject,events,bytes) VALUES('tamper-2','plugin.io-test',1,?1)",
            [RESERVATION_BYTES as i64],
        )
        .unwrap();
    assert!(Store::open_existing(&path, Default::default()).is_err());
    // A reservation for an operation without any history is invalid.
    let (_dir, path, mut store) = setup();
    let prepared = candidate("tamper-3");
    append(&mut store, &prepared);
    reserve(&mut store, prepared.command());
    drop(store);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE io_reservations SET subject='plugin.stranger'",
            [],
        )
        .unwrap();
    assert!(Store::open_existing(&path, Default::default()).is_err());
}
