//! Historical preparation of file mutations only. No external file is opened or
//! written, and a prepared intent is never treated as live authorization.
#![cfg(not(target_arch = "wasm32"))]

use morrow_core::{
    Error,
    audit::{self, SigningKey, TrustedLog},
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
    io_evidence::{self, Kind, Material},
    io_intent::{self, Command, Phase, Record, Recovery},
    plugin_package::io::{self, IoCapability},
    store::{EventBudget, Store},
};
use std::path::Path;

const OPERATION: &str = "file-mutation-prepare";
const SUBJECT: &str = "plugin.file-test";

fn request() -> MutationRequest {
    MutationRequest {
        operation_id: OPERATION.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: None,
        },
        disposition: Disposition::Replace,
        expected_identity: Some([4; 32]),
        content_length: 128,
        content_sha256: Some([5; 32]),
    }
}

fn candidate(value: MutationRequest) -> (Record, Material) {
    let request = RequestRecord::new(value).unwrap();
    let prepared = Record::prepared(request.command().unwrap()).unwrap();
    let command = prepared.command();
    let material = Material::encode(
        Kind::Request,
        &command.operation_id,
        &command.subject,
        command.request_sha256,
        request.container(),
    )
    .unwrap();
    (prepared, material)
}

fn counts(path: &Path) -> Vec<i64> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    [
        "io_intents",
        "io_reservations",
        "io_material_reservations",
        "io_evidence",
        "operations",
        "operation_events",
        "outbox",
    ]
    .into_iter()
    .map(|table| {
        connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}

fn empty(store: &Store, path: &Path) {
    assert!(
        store
            .lookup_io_intent(SUBJECT, OPERATION)
            .unwrap()
            .is_none()
    );
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    assert_eq!(counts(path), vec![0; 7]);
    store.integrity_check().unwrap();
}

fn complete(store: &Store, path: &Path, prepared: &Record, material: &Material) {
    let command = prepared.command();
    let stored = store.lookup_io_intent(SUBJECT, OPERATION).unwrap().unwrap();
    assert_eq!(stored.phase(), Phase::Prepared);
    assert_eq!(stored.container(), prepared.container());
    assert_eq!(stored.command(), command);
    let original = store
        .io_material(SUBJECT, OPERATION, Kind::Request)
        .unwrap()
        .unwrap();
    assert_eq!(original.container(), material.container());
    assert_eq!(original.payload(), material.payload());
    assert_eq!(store.io_intent_reservation_usage().unwrap().0, 1);
    assert_eq!(
        store.io_material_reservation_usage().unwrap(),
        (
            1,
            io_evidence::max_container_bytes(command.response_limit).unwrap()
        )
    );
    assert_eq!(counts(path), vec![1; 7]);
    store.integrity_check().unwrap();
}

#[test]
fn preparation_is_atomic_and_exact_retry_preserves_original_across_reopen_and_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file-mutation.db");
    let snapshot = dir.path().join("snapshot.db");
    let (prepared, material) = candidate(request());
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let stored = store
        .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    assert_eq!(stored.container(), prepared.container());
    complete(&store, &path, &prepared, &material);
    let usage = store.pending_usage().unwrap();
    let rows = counts(&path);
    let retried = store
        .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    assert_eq!(retried.container(), prepared.container());
    assert_eq!(counts(&path), rows);
    assert_eq!(store.pending_usage().unwrap(), usage);
    store.snapshot_to(&snapshot, 16 * 1024 * 1024).unwrap();
    drop(store);

    for path in [&path, &snapshot] {
        let store = Store::open_existing(path, EventBudget::default()).unwrap();
        complete(&store, path, &prepared, &material);
        assert_eq!(store.pending_usage().unwrap(), usage);
    }
}

#[test]
fn every_authorization_denial_rolls_back_intent_material_and_reservations() {
    let (prepared, material) = candidate(request());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("count-checks.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let mut checks = 0;
    store
        .prepare_file_mutation_local_authorized(&prepared, &material, || {
            checks += 1;
            Ok(())
        })
        .unwrap();
    assert!(checks >= 2, "preparation needs a final authorization guard");
    drop(store);

    for denied in 1..=checks {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("denied.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let mut seen = 0;
        let result = store.prepare_file_mutation_local_authorized(&prepared, &material, || {
            seen += 1;
            if seen == denied {
                Err(Error::Invalid("test authorization denial"))
            } else {
                Ok(())
            }
        });
        assert!(matches!(
            result,
            Err(Error::Invalid("test authorization denial"))
        ));
        assert_eq!(seen, denied);
        empty(&store, &path);
        store
            .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
            .unwrap();
        complete(&store, &path, &prepared, &material);
    }
}

#[test]
fn event_or_material_capacity_failure_leaves_no_partial_preparation() {
    let (prepared, material) = candidate(request());
    let command = prepared.command();
    let insufficient_material = prepared.container().len() as u64
        + io_intent::MAX_CONTAINER_BYTES as u64
        + io_evidence::max_container_bytes(command.request_bytes).unwrap()
        + io_evidence::max_container_bytes(command.response_limit).unwrap()
        - 1;
    for budget in [
        EventBudget {
            max_count: 1,
            max_bytes: 64 * 1024 * 1024,
        },
        EventBudget {
            max_count: 2,
            max_bytes: 64 * 1024 * 1024,
        },
        EventBudget {
            max_count: 10,
            max_bytes: insufficient_material,
        },
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capacity.db");
        let mut store = Store::open(&path, budget).unwrap();
        assert!(matches!(
            store.prepare_file_mutation_local_authorized(&prepared, &material, || Ok(())),
            Err(Error::EventCapacity)
        ));
        empty(&store, &path);
    }
}

#[test]
fn panicking_authorizer_rolls_back_and_the_same_store_can_prepare_again() {
    let (prepared, material) = candidate(request());
    for panic_at in [2, 5] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("panic.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let mut checks = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.prepare_file_mutation_local_authorized(&prepared, &material, || {
                checks += 1;
                assert_ne!(checks, panic_at, "synthetic authorization panic");
                Ok(())
            })
        }));
        assert!(result.is_err());
        empty(&store, &path);
        store
            .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
            .unwrap();
        complete(&store, &path, &prepared, &material);
    }
}

#[test]
fn changed_target_or_content_cannot_reuse_the_prepared_operation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("conflicts.db");
    let (prepared, material) = candidate(request());
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store
        .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    for change in 0..4 {
        let mut changed = request();
        match change {
            0 => changed.target.reference = [9; 32],
            1 => {
                changed.target.relative_path =
                    Some(RelativeFilePath::parse("nested/other.txt").unwrap())
            }
            2 => changed.content_sha256 = Some([9; 32]),
            _ => changed.content_length += 1,
        }
        let (competing, competing_material) = candidate(changed);
        assert!(matches!(
            store.prepare_file_mutation_local_authorized(
                &competing,
                &competing_material,
                || Ok(())
            ),
            Err(Error::OperationConflict)
        ));
        complete(&store, &path, &prepared, &material);
    }
    assert_eq!(counts(&path), vec![1; 7]);
}

#[test]
fn unbound_request_material_is_rejected_without_partial_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wrong-material.db");
    let (prepared, _material) = candidate(request());
    let command = prepared.command();
    let wrong = Material::encode(
        Kind::Request,
        &command.operation_id,
        &command.subject,
        command.request_sha256,
        b"not the bounded file mutation record",
    )
    .unwrap();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    assert!(
        store
            .prepare_file_mutation_local_authorized(&prepared, &wrong, || Ok(()))
            .is_err()
    );
    empty(&store, &path);
}

#[test]
fn prepared_reopens_read_only_and_both_dispatch_routes_remain_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prepared.db");
    let snapshot = dir.path().join("prepared-snapshot.db");
    let (prepared, material) = candidate(request());
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store
        .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    store.snapshot_to(&snapshot, 16 * 1024 * 1024).unwrap();
    drop(store);

    for path in [&path, &snapshot] {
        let mut reopened = Store::open_existing(path, EventBudget::default()).unwrap();
        let latest = reopened
            .lookup_io_intent(SUBJECT, OPERATION)
            .unwrap()
            .unwrap();
        assert_eq!(latest.container(), prepared.container());
        assert_eq!(latest.recovery(), Recovery::AwaitFreshAuthorization);
        assert_eq!(
            reopened
                .io_material(SUBJECT, OPERATION, Kind::Request)
                .unwrap()
                .unwrap()
                .container(),
            material.container()
        );
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let mut authorized = false;
        assert!(matches!(
            reopened.claim_io_dispatch_local_authorized(&unknown, || {
                authorized = true;
                Ok(())
            }),
            Err(Error::UnsupportedVersion)
        ));
        assert!(!authorized);
        assert!(matches!(
            reopened.append_io_intent_local_authorized(&unknown, || {
                authorized = true;
                Ok(())
            }),
            Err(Error::UnsupportedVersion)
        ));
        assert!(!authorized);
        assert_eq!(
            reopened
                .lookup_io_intent(SUBJECT, OPERATION)
                .unwrap()
                .unwrap()
                .phase(),
            Phase::Prepared
        );
        reopened.integrity_check().unwrap();
    }
}

#[test]
fn cancellation_is_durable_and_cannot_be_reinterpreted_as_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cancelled.db");
    let snapshot = dir.path().join("cancelled-snapshot.db");
    let (prepared, material) = candidate(request());
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store
        .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    let cancelled = prepared.propose_cancel_before_dispatch().unwrap();
    store
        .append_io_intent_local_authorized(&cancelled, || Ok(()))
        .unwrap();
    store.snapshot_to(&snapshot, 16 * 1024 * 1024).unwrap();
    drop(store);

    for path in [&path, &snapshot] {
        let mut reopened = Store::open_existing(path, EventBudget::default()).unwrap();
        let latest = reopened
            .lookup_io_intent(SUBJECT, OPERATION)
            .unwrap()
            .unwrap();
        assert_eq!(latest.container(), cancelled.container());
        assert_eq!(latest.recovery(), Recovery::CancelledBeforeDispatch);
        assert_eq!(
            reopened
                .io_material(SUBJECT, OPERATION, Kind::Request)
                .unwrap()
                .unwrap()
                .container(),
            material.container()
        );
        let mut authorized = false;
        assert!(
            reopened
                .claim_io_dispatch_local_authorized(
                    &prepared.propose_dispatch_boundary().unwrap(),
                    || {
                        authorized = true;
                        Ok(())
                    }
                )
                .is_err()
        );
        assert!(!authorized);
        reopened.integrity_check().unwrap();
    }
}

#[test]
fn missing_or_tampered_request_original_prevents_opening_prepared_history() {
    for damage in ["delete", "tamper"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("damaged.db");
        let (prepared, material) = candidate(request());
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        store
            .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
            .unwrap();
        drop(store);
        let connection = rusqlite::Connection::open(&path).unwrap();
        match damage {
            "delete" => {
                connection
                    .execute(
                        "DELETE FROM io_evidence WHERE operation_id=?1 AND kind=1",
                        [OPERATION],
                    )
                    .unwrap();
            }
            _ => {
                connection
                    .execute(
                        "UPDATE io_evidence SET container=X'00' WHERE operation_id=?1 AND kind=1",
                        [OPERATION],
                    )
                    .unwrap();
            }
        }
        drop(connection);
        assert!(
            Store::open_existing(&path, EventBudget::default()).is_err(),
            "{damage} request original unexpectedly reopened"
        );
    }
}

#[test]
fn legacy_io_digest_cannot_admit_any_file_capability_through_generic_store_routes() {
    for capability in [
        IoCapability::FileCreate,
        IoCapability::FileReplace,
        IoCapability::FileDelete,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy-file.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let prepared = Record::prepared(Command {
            operation_id: OPERATION.into(),
            subject: SUBJECT.into(),
            package_sha256: [1; 32],
            capability,
            protocol_sha256: io::schema_digest(),
            request_sha256: [2; 32],
            approval_sha256: [3; 32],
            target_sha256: [4; 32],
            request_bytes: 128,
            response_limit: 4096,
        })
        .unwrap();
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let mut authorized = false;
        assert!(matches!(
            store.append_io_intent_local_authorized(&prepared, || {
                authorized = true;
                Ok(())
            }),
            Err(Error::UnsupportedVersion)
        ));
        assert!(!authorized);
        empty(&store, &path);
        assert!(matches!(
            store.claim_io_dispatch_local_authorized(&unknown, || {
                authorized = true;
                Ok(())
            }),
            Err(Error::UnsupportedVersion)
        ));
        assert!(!authorized);
        empty(&store, &path);
    }
}

#[test]
fn file_protocol_prepared_cannot_bypass_atomic_request_material_admission() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("generic-file.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let (prepared, _) = candidate(request());
    let mut authorized = false;
    assert!(matches!(
        store.append_io_intent_local_authorized(&prepared, || {
            authorized = true;
            Ok(())
        }),
        Err(Error::UnsupportedVersion)
    ));
    assert!(!authorized);
    empty(&store, &path);
}

#[test]
fn prepared_and_cancelled_file_plans_survive_pending_and_sealed_snapshots() {
    for cancel in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audited.db");
        let before = dir.path().join("before-seal.db");
        let after = dir.path().join("after-seal.db");
        let key = SigningKey::from_bytes(&[73; 32]);
        let trust = TrustedLog {
            id: "file-mutation-test-log".into(),
            key: key.verifying_key(),
        };
        let (prepared, material) = candidate(request());
        let mut store =
            Store::open_audited(&path, EventBudget::default(), true, trust.clone()).unwrap();
        store
            .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
            .unwrap();
        let latest = if cancel {
            let cancelled = prepared.propose_cancel_before_dispatch().unwrap();
            store
                .append_io_intent_local_authorized(&cancelled, || Ok(()))
                .unwrap();
            cancelled
        } else {
            prepared
        };
        store.snapshot_to(&before, 16 * 1024 * 1024).unwrap();
        drop(store);

        for source in [&path, &before] {
            let reopened =
                Store::open_audited(source, EventBudget::default(), false, trust.clone()).unwrap();
            assert_eq!(
                reopened
                    .lookup_io_intent(SUBJECT, OPERATION)
                    .unwrap()
                    .unwrap()
                    .container(),
                latest.container()
            );
            assert_eq!(
                reopened
                    .io_material(SUBJECT, OPERATION, Kind::Request)
                    .unwrap()
                    .unwrap()
                    .container(),
                material.container()
            );
            reopened.integrity_check().unwrap();
        }

        let mut store =
            Store::open_audited(&path, EventBudget::default(), false, trust.clone()).unwrap();
        let pending = store.pending(0, 10).unwrap();
        assert_eq!(pending.len(), if cancel { 2 } else { 1 });
        let signed = audit::sign(
            &audit::from_pending(&trust, 1, [0; 32], &pending).unwrap(),
            &trust,
            &key,
        )
        .unwrap();
        assert!(store.seal_pending(&signed).unwrap());
        assert_eq!(store.pending_usage().unwrap(), (0, 0));
        store.snapshot_to(&after, 16 * 1024 * 1024).unwrap();
        drop(store);

        for source in [&path, &after] {
            let reopened = Store::open_read_only_audited(source, trust.clone()).unwrap();
            assert_eq!(reopened.sealed_segment(1).unwrap().unwrap(), signed);
            assert_eq!(
                reopened
                    .lookup_io_intent(SUBJECT, OPERATION)
                    .unwrap()
                    .unwrap()
                    .container(),
                latest.container()
            );
            assert_eq!(
                reopened
                    .io_material(SUBJECT, OPERATION, Kind::Request)
                    .unwrap()
                    .unwrap()
                    .container(),
                material.container()
            );
            reopened.integrity_check().unwrap();
        }
    }
}
