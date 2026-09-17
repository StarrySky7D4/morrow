//! Protected IO material storage qualification using synthetic facts only.
//! Stored request/response originals are historical material: no dispatch,
//! remote success, credential storage or recovered grant is established by
//! these tests or by reading a stored original back.
#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    io_evidence::{self, Kind, Material},
    io_intent::{Command, ObservationSource, Record},
    plugin_package::io,
    store::{EventBudget, Store},
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const SUBJECT: &str = "plugin.io-test";

fn request_payload() -> Vec<u8> {
    (0..27u32).map(|i| i as u8).collect()
}
fn request_digest() -> [u8; 32] {
    Sha256::digest(request_payload()).into()
}
fn command(operation: &str) -> Command {
    Command {
        operation_id: operation.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        capability: io::IoCapability::HttpRequest,
        protocol_sha256: io::schema_digest(),
        request_sha256: request_digest(),
        approval_sha256: [3; 32],
        target_sha256: [4; 32],
        request_bytes: request_payload().len() as u64,
        response_limit: 4096,
    }
}
fn prepared(operation: &str) -> Record {
    Record::prepared(command(operation)).unwrap()
}
fn material(kind: Kind, operation: &str, payload: &[u8]) -> Material {
    Material::encode(kind, operation, SUBJECT, request_digest(), payload).unwrap()
}
fn setup() -> (tempfile::TempDir, PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("io-material.db");
    let store = Store::open(&path, EventBudget::default()).unwrap();
    (dir, path, store)
}
fn append(store: &mut Store, record: &Record) {
    store
        .append_io_intent_local_authorized(record, || Ok(()))
        .unwrap();
}
fn count(path: &Path, table: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row.get(0))
        .unwrap()
}
// Recompute the legitimate outer hash so malformed protobuf/semantics reach the
// corresponding decoder checks instead of merely failing checksum validation.
fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(io_evidence::MAGIC);
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend_from_slice(&compressed);
    bytes
}

#[test]
fn codec_roundtrip_tampering_and_limits_are_bounded() {
    for kind in [Kind::Request, Kind::Response] {
        let payload: Vec<u8> = (0..1000u32).map(|i| (i * 7 % 251) as u8).collect();
        let original = material(kind, "io-operation-1", &payload);
        let container = original.container().to_vec();
        let restored = Material::decode(&container).unwrap();
        assert_eq!(restored.kind(), kind);
        assert_eq!(restored.operation_id(), "io-operation-1");
        assert_eq!(restored.subject(), SUBJECT);
        assert_eq!(restored.request_sha256(), request_digest());
        assert_eq!(restored.payload(), payload.as_slice());
        assert_eq!(
            restored.payload_sha256(),
            <[u8; 32]>::from(Sha256::digest(&payload))
        );
        assert_eq!(restored.digest(), original.digest());
        assert_eq!(restored.raw(), original.raw());
        // Every single-byte tamper and every truncation must fail.
        for position in 0..container.len() {
            let mut bytes = container.clone();
            bytes[position] ^= 0x40;
            assert!(Material::decode(&bytes).is_err(), "tampered {position}");
        }
        for length in 0..container.len() {
            assert!(
                Material::decode(&container[..length]).is_err(),
                "truncated {length}"
            );
        }
    }
    // The maximum material size itself stays encodable and decodable.
    let full = vec![7u8; io_evidence::MAX_MATERIAL_BYTES];
    let bounded = material(Kind::Request, "io-operation-1", &full);
    assert!(Material::decode(bounded.container()).is_ok());
    let oversized = vec![7u8; io_evidence::MAX_MATERIAL_BYTES + 1];
    assert!(matches!(
        Material::encode(Kind::Request, "io-operation-1", SUBJECT, [2; 32], &oversized),
        Err(Error::Limit)
    ));
    assert!(matches!(
        io_evidence::max_container_bytes(io::MAX_JOB_BYTES + 1),
        Err(Error::Limit)
    ));
    assert_eq!(
        io_evidence::max_container_bytes(io::MAX_JOB_BYTES).unwrap(),
        io_evidence::MAX_CONTAINER_BYTES as u64
    );
    assert_eq!(
        io_evidence::max_container_bytes(0).unwrap(),
        4096 + 4096 / 255 + 128
    );
}

#[test]
fn noncanonical_unknown_and_duplicate_field_encodings_are_rejected() {
    let raw = material(Kind::Request, "io-operation-1", b"payload")
        .raw()
        .to_vec();
    // Canonical schema_version=1 rewritten as a non-minimal varint.
    let mut overlong = raw.clone();
    assert_eq!(&overlong[..2], &[8, 1]);
    overlong.splice(1..2, [0x81, 0]);
    assert!(Material::decode(&pack(&overlong)).is_err());
    // Unknown field numbers demand an explicit schema version.
    for tag in [0x48u8, 0x50] {
        assert!(matches!(
            Material::decode(&pack(&[tag, 1])),
            Err(Error::UnsupportedVersion)
        ));
    }
    // Duplicate known fields are rejected before allocation.
    let mut duplicate = raw.clone();
    duplicate.extend_from_slice(&[0x12, 1, b'x']);
    assert!(matches!(
        Material::decode(&pack(&duplicate)),
        Err(Error::Invalid("duplicate IO evidence field"))
    ));
    // Wrong wire type on the kind scalar.
    assert!(matches!(
        Material::decode(&pack(&[0x22, 1, 1])),
        Err(Error::Invalid("IO evidence wire type"))
    ));
    // Oversize and out-of-span lengths.
    let mut long = vec![0x2a, 33];
    long.extend([1u8; 33]);
    assert!(matches!(
        Material::decode(&pack(&long)),
        Err(Error::Limit)
    ));
    assert!(matches!(
        Material::decode(&pack(&[0x12, 40])),
        Err(Error::Invalid("IO evidence span"))
    ));
    // A container whose payload bytes contradict its digest field is rejected.
    let mut forged = raw;
    let last = forged.len() - 1;
    forged[last] ^= 0x01;
    assert!(matches!(
        Material::decode(&pack(&forged)),
        Err(Error::Invalid("IO evidence payload digest"))
    ));
}

#[test]
fn protected_material_lifecycle_roundtrips_both_directions_exactly() {
    let (_dir, path, mut store) = setup();
    let initial = prepared("lifecycle");
    append(&mut store, &initial);
    let reservation = store
        .reserve_io_materials(initial.command(), || Ok(()))
        .unwrap();
    let expected_request = io_evidence::max_container_bytes(27).unwrap();
    let expected_response = io_evidence::max_container_bytes(4096).unwrap();
    assert_eq!(reservation.request_bytes, expected_request);
    assert_eq!(reservation.response_bytes, expected_response);
    assert_eq!(
        store.io_material_reservation_usage().unwrap(),
        (2, expected_request + expected_response)
    );
    let request_payload: Vec<u8> = (0..27u32).map(|i| i as u8).collect();
    let request = material(Kind::Request, "lifecycle", &request_payload);
    store
        .store_io_material(SUBJECT, Kind::Request, &request, || Ok(()))
        .unwrap();
    assert_eq!(
        store.io_material_reservation_usage().unwrap(),
        (1, expected_response)
    );
    store
        .reserve_io_intent_followup(initial.command(), || Ok(()))
        .unwrap();
    let unknown = initial.propose_dispatch_boundary().unwrap();
    append(&mut store, &unknown);
    let response_payload = b"protected-response".to_vec();
    let response = material(Kind::Response, "lifecycle", &response_payload);
    store
        .store_io_material(SUBJECT, Kind::Response, &response, || Ok(()))
        .unwrap();
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    let observed = unknown
        .propose_observation([5; 32], ObservationSource::OriginalResponse)
        .unwrap();
    append(&mut store, &observed);
    drop(store);
    let store = Store::open_existing(&path, Default::default()).unwrap();
    let stored_request = store
        .io_material(SUBJECT, "lifecycle", Kind::Request)
        .unwrap()
        .unwrap();
    assert_eq!(stored_request.payload(), request_payload.as_slice());
    assert_eq!(stored_request.container(), request.container());
    assert_eq!(stored_request.digest(), request.digest());
    assert_eq!(stored_request.request_sha256(), request_digest());
    assert_eq!(stored_request.kind(), Kind::Request);
    let stored_response = store
        .io_material(SUBJECT, "lifecycle", Kind::Response)
        .unwrap()
        .unwrap();
    assert_eq!(stored_response.payload(), response_payload.as_slice());
    assert_eq!(stored_response.container(), response.container());
    assert_eq!(
        stored_response.payload_sha256(),
        <[u8; 32]>::from(Sha256::digest(&response_payload))
    );
    assert_eq!(stored_response.kind(), Kind::Response);
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    store.integrity_check().unwrap();
}

#[test]
fn repeats_are_idempotent_and_binding_mismatches_fail_explicitly() {
    let (_dir, path, mut store) = setup();
    let initial = prepared("idem");
    append(&mut store, &initial);
    let first = store
        .reserve_io_materials(initial.command(), || Ok(()))
        .unwrap();
    let second = store
        .reserve_io_materials(initial.command(), || Ok(()))
        .unwrap();
    assert_eq!(first, second);
    let mut competing = command("idem");
    competing.request_sha256 = [9; 32];
    assert!(matches!(
        store.reserve_io_materials(&competing, || Ok(())),
        Err(Error::OperationConflict)
    ));
    let mut stranger = command("idem");
    stranger.subject = "plugin.stranger".into();
    assert!(matches!(
        store.reserve_io_materials(&stranger, || Ok(())),
        Err(Error::OperationConflict)
    ));
    let request = material(Kind::Request, "idem", &request_payload());
    store
        .store_io_material(SUBJECT, Kind::Request, &request, || Ok(()))
        .unwrap();
    // Exact re-admission is idempotent and consumes no second slot.
    store
        .store_io_material(SUBJECT, Kind::Request, &request, || Ok(()))
        .unwrap();
    // A request original that contradicts the command binding is refused even
    // for the same slot; the request slot is fixed by the command digest.
    let conflicting = material(Kind::Request, "idem", b"two");
    assert!(matches!(
        store.store_io_material(SUBJECT, Kind::Request, &conflicting, || Ok(())),
        Err(Error::OperationConflict)
    ));
    // The caller subject must own the history.
    assert!(matches!(
        store.store_io_material("plugin.stranger", Kind::Request, &request, || Ok(())),
        Err(Error::NotFound)
    ));
    // Material bound to a different subject is refused.
    let foreign =
        Material::encode(Kind::Request, "idem", "plugin.stranger", request_digest(), b"one")
            .unwrap();
    assert!(matches!(
        store.store_io_material(SUBJECT, Kind::Request, &foreign, || Ok(())),
        Err(Error::OperationConflict)
    ));
    // Material bound to a different request digest is refused.
    let other_binding =
        Material::encode(Kind::Request, "idem", SUBJECT, [3; 32], &request_payload()).unwrap();
    assert!(matches!(
        store.store_io_material(SUBJECT, Kind::Request, &other_binding, || Ok(())),
        Err(Error::OperationConflict)
    ));
    // A declared slot that disagrees with the encoded record is refused before
    // any admission could make the store inconsistent.
    let response_record = material(Kind::Response, "idem", b"r");
    assert!(matches!(
        store.store_io_material(SUBJECT, Kind::Request, &response_record, || Ok(())),
        Err(Error::Invalid("IO material kind"))
    ));
    // Material for an unknown operation has no history at all.
    let unknown_operation = material(Kind::Request, "idem-2", b"one");
    assert!(matches!(
        store.store_io_material(SUBJECT, Kind::Request, &unknown_operation, || Ok(())),
        Err(Error::NotFound)
    ));
    // Response material is refused before the dispatch boundary.
    let response = material(Kind::Response, "idem", b"r");
    assert!(matches!(
        store.store_io_material(SUBJECT, Kind::Response, &response, || Ok(())),
        Err(Error::Invalid("IO material phase"))
    ));
    // Release is idempotent, owner-scoped and only valid before dispatch.
    let releasable = prepared("releasable");
    append(&mut store, &releasable);
    store
        .reserve_io_materials(releasable.command(), || Ok(()))
        .unwrap();
    store.release_io_materials(SUBJECT, "releasable").unwrap();
    store.release_io_materials(SUBJECT, "releasable").unwrap();
    assert!(matches!(
        store.release_io_materials("plugin.stranger", "releasable"),
        Err(Error::NotFound)
    ));
    assert!(matches!(
        store.release_io_materials(SUBJECT, "absent"),
        Err(Error::NotFound)
    ));
    // Storing without any reservation is refused explicitly.
    let unreserved = prepared("unreserved");
    append(&mut store, &unreserved);
    let stray = material(Kind::Request, "unreserved", &request_payload());
    assert!(matches!(
        store.store_io_material(SUBJECT, Kind::Request, &stray, || Ok(())),
        Err(Error::Invalid("IO material reservation"))
    ));
    // A response original may differ per observation, so re-use conflicts are
    // exercised in the same slot after the dispatch boundary is crossed.
    store
        .reserve_io_intent_followup(initial.command(), || Ok(()))
        .unwrap();
    let unknown = initial.propose_dispatch_boundary().unwrap();
    append(&mut store, &unknown);
    let response = material(Kind::Response, "idem", b"response-one");
    store
        .store_io_material(SUBJECT, Kind::Response, &response, || Ok(()))
        .unwrap();
    store
        .store_io_material(SUBJECT, Kind::Response, &response, || Ok(()))
        .unwrap();
    let other_response = material(Kind::Response, "idem", b"response-two");
    assert!(matches!(
        store.store_io_material(SUBJECT, Kind::Response, &other_response, || Ok(())),
        Err(Error::OperationConflict)
    ));
    // After the boundary the reservation can no longer be released.
    assert!(matches!(
        store.release_io_materials(SUBJECT, "idem"),
        Err(Error::Invalid("IO material reservation terminal"))
    ));
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    assert_eq!(count(&path, "io_material_reservations"), 0);
    assert_eq!(count(&path, "io_evidence"), 2);
    store.integrity_check().unwrap();
}

#[test]
fn material_reservations_respect_the_shared_byte_budget_atomically() {
    let (_dir, path, store) = setup();
    drop(store);
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 1024,
            max_bytes: 4096,
        },
    )
    .unwrap();
    let initial = prepared("capacity");
    append(&mut store, &initial);
    let planned = io_evidence::max_container_bytes(27).unwrap()
        + io_evidence::max_container_bytes(4096).unwrap();
    assert!(planned > 4096);
    assert!(matches!(
        store.reserve_io_materials(initial.command(), || Ok(())),
        Err(Error::EventCapacity)
    ));
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    assert_eq!(count(&path, "io_material_reservations"), 0);
    assert_eq!(count(&path, "io_evidence"), 0);
    store.integrity_check().unwrap();
}

#[test]
fn authorization_failures_leave_neither_reservations_nor_material() {
    let (_dir, path, mut store) = setup();
    let initial = prepared("authz");
    append(&mut store, &initial);
    let mut calls = 0;
    assert!(matches!(
        store.reserve_io_materials(initial.command(), || {
            calls += 1;
            Err(Error::Invalid("test authorization denied"))
        }),
        Err(Error::Invalid("test authorization denied"))
    ));
    assert_eq!(calls, 1);
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    store
        .reserve_io_materials(initial.command(), || Ok(()))
        .unwrap();
    let request = material(Kind::Request, "authz", &request_payload());
    let before = store.io_material_reservation_usage().unwrap();
    let mut calls = 0;
    assert!(matches!(
        store.store_io_material(SUBJECT, Kind::Request, &request, || {
            calls += 1;
            Err(Error::Invalid("test authorization denied"))
        }),
        Err(Error::Invalid("test authorization denied"))
    ));
    assert_eq!(calls, 1);
    assert_eq!(store.io_material_reservation_usage().unwrap(), before);
    assert_eq!(count(&path, "io_evidence"), 0);
    assert!(matches!(
        store.io_material(SUBJECT, "authz", Kind::Request),
        Err(Error::EvidenceUnavailable)
    ));
    store.integrity_check().unwrap();
}

#[test]
fn missing_material_is_classified_and_foreign_history_is_absence() {
    let (_dir, _path, mut store) = setup();
    let initial = prepared("classified");
    append(&mut store, &initial);
    assert!(matches!(
        store.io_material(SUBJECT, "classified", Kind::Request),
        Err(Error::EvidenceUnavailable)
    ));
    assert!(matches!(
        store.io_material(SUBJECT, "classified", Kind::Response),
        Err(Error::EvidenceUnavailable)
    ));
    assert!(
        store
            .io_material("plugin.stranger", "classified", Kind::Request)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .io_material(SUBJECT, "absent", Kind::Request)
            .unwrap()
            .is_none()
    );
    store.integrity_check().unwrap();
}

#[test]
fn terminal_transitions_close_material_quota() {
    let (_dir, _path, mut store) = setup();
    // Pre-dispatch cancellation releases the still-held response quota while the
    // admitted request original stays retained for the command history.
    let cancelled = prepared("cancelled");
    append(&mut store, &cancelled);
    store
        .reserve_io_materials(cancelled.command(), || Ok(()))
        .unwrap();
    store
        .store_io_material(
            SUBJECT,
            Kind::Request,
            &material(Kind::Request, "cancelled", &request_payload()),
            || Ok(()),
        )
        .unwrap();
    let cancel = cancelled.propose_cancel_before_dispatch().unwrap();
    append(&mut store, &cancel);
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    assert!(
        store
            .io_material(SUBJECT, "cancelled", Kind::Request)
            .unwrap()
            .is_some()
    );
    // Observing while the response quota is still held would strand it, so the
    // transition is refused until the original is admitted.
    let observed = prepared("observed");
    append(&mut store, &observed);
    store
        .reserve_io_materials(observed.command(), || Ok(()))
        .unwrap();
    store
        .store_io_material(
            SUBJECT,
            Kind::Request,
            &material(Kind::Request, "observed", &request_payload()),
            || Ok(()),
        )
        .unwrap();
    store
        .reserve_io_intent_followup(observed.command(), || Ok(()))
        .unwrap();
    let unknown = observed.propose_dispatch_boundary().unwrap();
    append(&mut store, &unknown);
    let terminal = unknown
        .propose_observation([7; 32], ObservationSource::OriginalResponse)
        .unwrap();
    assert!(matches!(
        store.append_io_intent_local_authorized(&terminal, || Ok(())),
        Err(Error::Invalid("IO material reservation"))
    ));
    store
        .store_io_material(
            SUBJECT,
            Kind::Response,
            &material(Kind::Response, "observed", b"protected-response"),
            || Ok(()),
        )
        .unwrap();
    append(&mut store, &terminal);
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    store.integrity_check().unwrap();
}

#[test]
fn reconciliation_releases_only_never_admitted_quota() {
    let (_dir, _path, mut store) = setup();
    let initial = prepared("reconcile");
    append(&mut store, &initial);
    store
        .reserve_io_materials(initial.command(), || Ok(()))
        .unwrap();
    store
        .store_io_material(
            SUBJECT,
            Kind::Request,
            &material(Kind::Request, "reconcile", &request_payload()),
            || Ok(()),
        )
        .unwrap();
    store
        .reserve_io_intent_followup(initial.command(), || Ok(()))
        .unwrap();
    let unknown = initial.propose_dispatch_boundary().unwrap();
    append(&mut store, &unknown);
    // An admitted original is retained; only the never-admitted response quota
    // can be released for a reconciliation observation.
    assert!(matches!(
        store.release_io_material_reconciliation(SUBJECT, "reconcile", Kind::Request),
        Err(Error::Invalid("IO material already admitted"))
    ));
    store
        .release_io_material_reconciliation(SUBJECT, "reconcile", Kind::Response)
        .unwrap();
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    let observed = unknown
        .propose_observation([11; 32], ObservationSource::Reconciliation)
        .unwrap();
    append(&mut store, &observed);
    // Once terminal the reconciliation path is closed as well.
    assert!(matches!(
        store.release_io_material_reconciliation(SUBJECT, "reconcile", Kind::Response),
        Err(Error::Invalid("IO material reconciliation phase"))
    ));
    // Before the boundary the pre-dispatch release remains the only path.
    let early = prepared("early-release");
    append(&mut store, &early);
    store
        .reserve_io_materials(early.command(), || Ok(()))
        .unwrap();
    assert!(matches!(
        store.release_io_material_reconciliation(SUBJECT, "early-release", Kind::Response),
        Err(Error::Invalid("IO material reconciliation phase"))
    ));
    store.release_io_materials(SUBJECT, "early-release").unwrap();
    store.integrity_check().unwrap();
}

#[test]
fn downgrade_with_new_objects_is_refused_and_honest_v16_migrates_cleanly() {
    let (_dir, path, mut store) = setup();
    let initial = prepared("migration");
    append(&mut store, &initial);
    let originals = store.pending(0, 10).unwrap();
    drop(store);
    // A v17 database downgraded to v16 while keeping the new namespace must be
    // rejected, never silently repaired.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version=16;")
        .unwrap();
    assert!(matches!(
        Store::open_existing(&path, Default::default()),
        Err(Error::Integrity)
    ));
    // Dropping the new objects produces an honest v16 database; reopening
    // migrates to v17 without rewriting any original.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "DROP TABLE io_evidence; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_material_reservations; PRAGMA user_version=16;",
        )
        .unwrap();
    let migrated = Store::open_existing(&path, Default::default()).unwrap();
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 17);
    assert_eq!(migrated.pending(0, 10).unwrap(), originals);
    assert_eq!(migrated.io_material_reservation_usage().unwrap(), (0, 0));
    migrated.integrity_check().unwrap();
}
