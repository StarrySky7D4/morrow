//! Persistent claim qualification across independent SQLite connections.
//! Synthetic IO facts only: these tests execute no network operation and do not
//! claim that a database commit proves a remote effect or exactly-once HTTP.
#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    io_intent::{Command, ObservationSource, Phase, Record, Recovery},
    plugin_package::io,
    store::{EventBudget, Store},
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
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
fn prepare(store: &mut Store, operation: &str) -> Record {
    let prepared = candidate(operation);
    store
        .append_io_intent_local_authorized(&prepared, || Ok(()))
        .unwrap();
    store
        .reserve_io_intent_followup(prepared.command(), || Ok(()))
        .unwrap();
    prepared
}
fn latest(store: &Store, operation: &str) -> Record {
    store
        .lookup_io_intent("plugin.io-test", operation)
        .unwrap()
        .unwrap()
}
fn same(actual: &Record, expected: &Record) {
    assert_eq!(actual.raw(), expected.raw());
    assert_eq!(actual.container(), expected.container());
    assert_eq!(actual.digest(), expected.digest());
    assert_eq!(actual.event_id(), expected.event_id());
}
fn rows(path: &Path, table: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn simultaneous_independent_stores_have_exactly_one_committed_claim() {
    let (_dir, path, mut initial) = setup();
    for round in 0..4 {
        let operation = format!("race-{round}");
        let prepared = prepare(&mut initial, &operation);
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let stores = [
            Store::open_existing(&path, EventBudget::default()).unwrap(),
            Store::open_existing(&path, EventBudget::default()).unwrap(),
        ];
        let start = Arc::new(Barrier::new(2));
        let authorized = Arc::new(AtomicUsize::new(0));
        let workers = stores
            .into_iter()
            .map(|mut store| {
                let start = start.clone();
                let authorized = authorized.clone();
                let unknown = unknown.clone();
                std::thread::spawn(move || {
                    start.wait();
                    let deadline = Instant::now() + Duration::from_secs(5);
                    loop {
                        let result = store.claim_io_dispatch_local_authorized(&unknown, || {
                            authorized.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        });
                        match result {
                            // Store has a zero busy timeout. Only an explicitly busy
                            // transaction can be retried here; CommitUnknown cannot.
                            Err(Error::StorageBusy) => {
                                assert!(Instant::now() < deadline, "claim lock did not clear");
                                std::thread::sleep(Duration::from_millis(1));
                            }
                            other => return other,
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|r| matches!(r, Err(Error::RevisionConflict)))
                .count(),
            1
        );
        assert_eq!(authorized.load(Ordering::SeqCst), 1);
        same(
            results.iter().find_map(|r| r.as_ref().ok()).unwrap(),
            &unknown,
        );
        same(&latest(&initial, &operation), &unknown);
        assert_eq!(rows(&path, "io_intents"), (round + 1) * 2);
        assert_eq!(rows(&path, "operations"), (round + 1) * 2);
        assert_eq!(rows(&path, "operation_events"), (round + 1) * 2);
        assert_eq!(rows(&path, "outbox"), (round + 1) * 2);
    }
    initial.integrity_check().unwrap();
}
#[test]
fn committed_claim_cannot_be_won_again_after_reopen() {
    let (_dir, path, mut store) = setup();
    let prepared = prepare(&mut store, "reopen");
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    same(
        &store
            .claim_io_dispatch_local_authorized(&unknown, || Ok(()))
            .unwrap(),
        &unknown,
    );
    let pending = store.pending(0, 10).unwrap();
    let reserved = store.io_intent_reservation_usage().unwrap();
    drop(store);
    let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
    let mut called = false;
    assert!(matches!(
        store.claim_io_dispatch_local_authorized(&unknown, || {
            called = true;
            Ok(())
        }),
        Err(Error::RevisionConflict)
    ));
    assert!(!called);
    let original = latest(&store, "reopen");
    same(&original, &unknown);
    assert_eq!(original.recovery(), Recovery::ReconcileOnly);
    assert_eq!(store.pending(0, 10).unwrap(), pending);
    assert_eq!(store.io_intent_reservation_usage().unwrap(), reserved);
    store.integrity_check().unwrap();
}
#[test]
fn changed_command_never_claims_an_existing_operation() {
    let (_dir, _path, mut store) = setup();
    let prepared = prepare(&mut store, "bound-command");
    let pending = store.pending(0, 10).unwrap();
    for field in 0..7 {
        let mut command = prepared.command().clone();
        match field {
            0 => command.subject = "other.plugin".into(),
            1 => command.package_sha256 = [8; 32],
            2 => command.request_sha256 = [8; 32],
            3 => command.approval_sha256 = [8; 32],
            4 => command.target_sha256 = [8; 32],
            5 => command.request_bytes += 1,
            _ => command.response_limit += 1,
        }
        let unknown = Record::prepared(command)
            .unwrap()
            .propose_dispatch_boundary()
            .unwrap();
        let mut called = false;
        assert!(matches!(
            store.claim_io_dispatch_local_authorized(&unknown, || {
                called = true;
                Ok(())
            }),
            Err(Error::OperationConflict)
        ));
        assert!(!called);
        same(&latest(&store, "bound-command"), &prepared);
    }
    assert_eq!(store.pending(0, 10).unwrap(), pending);
    store.integrity_check().unwrap();
}
#[test]
fn rejected_final_authorization_rolls_back_history_outbox_and_claim() {
    let (_dir, path, mut store) = setup();
    let prepared = prepare(&mut store, "guard");
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let pending = store.pending(0, 10).unwrap();
    let reserved = store.io_intent_reservation_usage().unwrap();
    let mut called = 0;
    assert!(matches!(
        store.claim_io_dispatch_local_authorized(&unknown, || {
            called += 1;
            // The separate reader still sees only committed Prepared while the
            // writer has reached the final callback in its Immediate transaction.
            assert_eq!(rows(&path, "operations"), 1);
            Err(Error::Invalid("test current authority denied"))
        }),
        Err(Error::Invalid("test current authority denied"))
    ));
    assert_eq!(called, 1);
    same(&latest(&store, "guard"), &prepared);
    assert_eq!(store.pending(0, 10).unwrap(), pending);
    assert_eq!(store.io_intent_reservation_usage().unwrap(), reserved);
    for table in ["io_intents", "operations", "outbox", "operation_events"] {
        assert_eq!(rows(&path, table), 1);
    }
    // A rolled-back attempt consumed no claim.
    same(
        &store
            .claim_io_dispatch_local_authorized(&unknown, || Ok(()))
            .unwrap(),
        &unknown,
    );
    store.integrity_check().unwrap();
}
#[test]
fn terminal_observation_or_cancellation_cannot_reopen_the_dispatch_boundary() {
    let (_dir, _path, mut store) = setup();
    for observe in [false, true] {
        let operation = if observe { "observed" } else { "cancelled" };
        let prepared = prepare(&mut store, operation);
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let terminal = if observe {
            store
                .claim_io_dispatch_local_authorized(&unknown, || Ok(()))
                .unwrap();
            unknown
                .propose_observation([5; 32], ObservationSource::OriginalResponse)
                .unwrap()
        } else {
            prepared.propose_cancel_before_dispatch().unwrap()
        };
        store
            .append_io_intent_local_authorized(&terminal, || Ok(()))
            .unwrap();
        let pending = store.pending(0, 10).unwrap();
        assert!(matches!(
            store.claim_io_dispatch_local_authorized(&unknown, || Ok(())),
            Err(Error::RevisionConflict)
        ));
        assert!(
            store
                .claim_io_dispatch_local_authorized(&terminal, || Ok(()))
                .is_err()
        );
        same(&latest(&store, operation), &terminal);
        assert_eq!(store.pending(0, 10).unwrap(), pending);
    }
    store.integrity_check().unwrap();
}
#[test]
fn claim_requires_prepared_history_phase_and_complete_followup_reservation() {
    let (_dir, _path, mut store) = setup();
    let prepared = candidate("missing");
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    assert!(matches!(
        store.claim_io_dispatch_local_authorized(&unknown, || Ok(())),
        Err(Error::RevisionConflict)
    ));
    assert!(
        store
            .claim_io_dispatch_local_authorized(&prepared, || Ok(()))
            .is_err()
    );
    assert!(
        store
            .lookup_io_intent("plugin.io-test", "missing")
            .unwrap()
            .is_none()
    );
    store
        .append_io_intent_local_authorized(&prepared, || Ok(()))
        .unwrap();
    assert!(matches!(
        store.claim_io_dispatch_local_authorized(&unknown, || Ok(())),
        Err(Error::Invalid("IO intent dispatch reservation"))
    ));
    same(&latest(&store, "missing"), &prepared);
    store
        .reserve_io_intent_followup(prepared.command(), || Ok(()))
        .unwrap();
    assert_eq!(
        store
            .claim_io_dispatch_local_authorized(&unknown, || Ok(()))
            .unwrap()
            .phase(),
        Phase::OutcomeUnknown
    );
    store.integrity_check().unwrap();
}
#[test]
fn ordinary_append_remains_idempotent_without_conferring_a_second_claim() {
    let (_dir, _path, mut store) = setup();
    let prepared = prepare(&mut store, "ordinary-retry");
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    store
        .append_io_intent_local_authorized(&unknown, || Ok(()))
        .unwrap();
    let pending = store.pending(0, 10).unwrap();
    let usage = store.pending_usage().unwrap();
    for original in [&prepared, &unknown] {
        let mut called = 0;
        let result = store
            .append_io_intent_local_authorized(original, || {
                called += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(called, 1);
        same(&result, original);
    }
    assert!(matches!(
        store.claim_io_dispatch_local_authorized(&unknown, || Ok(())),
        Err(Error::RevisionConflict)
    ));
    same(&latest(&store, "ordinary-retry"), &unknown);
    assert_eq!(store.pending(0, 10).unwrap(), pending);
    assert_eq!(store.pending_usage().unwrap(), usage);
    store.integrity_check().unwrap();
}
