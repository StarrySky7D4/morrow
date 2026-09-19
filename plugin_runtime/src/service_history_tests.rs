//! Storage-level service-history tests. No network or guest execution is claimed.
use super::*;
use morrow_core::{io::Header, service::Invocation, store::EventBudget};
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicU64, Ordering},
};

const PACKAGE: [u8; 32] = [7; 32];
const KEY: &str = "stable-client-key";
fn policy() -> Policy {
    Policy {
        namespace: [8; 32],
        retention_ms: 1000,
    }
}
fn request_with(body: &[u8]) -> Request {
    Request::encode(
        1,
        &Invocation {
            service: "notes".into(),
            handler: "notes.echo".into(),
            principal: "alice".into(),
            method: "POST".into(),
            target: "/notes".into(),
            headers: vec![],
            body: body.to_vec(),
        },
    )
    .unwrap()
}
fn request() -> Request {
    request_with(b"original")
}
fn response(request: &Request, body: &[u8]) -> Vec<u8> {
    Response::encode(
        request,
        &service::Reply {
            status: 201,
            headers: vec![Header {
                name: "x-original".into(),
                value: b"yes".to_vec(),
            }],
            body: body.to_vec(),
        },
    )
    .unwrap()
}
fn journal(time: &Arc<AtomicU64>) -> ServiceJournal {
    let clock = time.clone();
    ServiceJournal::new(policy(), move || clock.load(Ordering::SeqCst)).unwrap()
}
fn prepare(store: &mut Store, with_material: bool) -> Record {
    let retained = RequestRecord::encode(&policy(), KEY, &request(), 100).unwrap();
    let prepared = Record::prepared(retained.command(PACKAGE).unwrap()).unwrap();
    store
        .append_io_intent_local_authorized(&prepared, || Ok(()))
        .unwrap();
    if with_material {
        store
            .reserve_io_materials(prepared.command(), || Ok(()))
            .unwrap();
        let c = prepared.command();
        let original = Material::encode(
            Kind::Request,
            &c.operation_id,
            &c.subject,
            c.request_sha256,
            retained.container(),
        )
        .unwrap();
        store
            .store_io_material(&c.subject, Kind::Request, &original, || Ok(()))
            .unwrap();
    }
    prepared
}
fn execute(store: &mut Store, journal: &ServiceJournal) -> (Record, Validity) {
    match begin(store, journal, KEY, &request(), PACKAGE, || Ok(())).unwrap() {
        Begin::Execute { record, validity } => (record, validity),
        _ => panic!("expected a newly committed unique claim"),
    }
}
#[test]
fn observed_reopen_replays_exact_completion_without_another_event_and_rechecks_authority() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let time = Arc::new(AtomicU64::new(100));
    let journal = journal(&time);
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let (record, _) = execute(&mut store, &journal);
    let completion = response(&request(), b"private-result");
    finish(&mut store, &record, &completion).unwrap();
    finish(&mut store, &record, &completion).unwrap();
    let before = store.pending_usage().unwrap();
    assert_eq!(before.0, 3);
    store.integrity_check().unwrap();
    drop(store);
    time.store(400, Ordering::SeqCst);
    let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
    let mut guards = 0;
    match begin(&mut store, &journal, KEY, &request(), PACKAGE, || {
        guards += 1;
        Ok(())
    })
    .unwrap()
    {
        Begin::Replay {
            reply,
            completion: actual,
            validity,
        } => {
            assert_eq!(reply.status, 201);
            assert_eq!(reply.body, b"private-result");
            assert_eq!(actual, completion);
            validity.check().unwrap();
        }
        _ => panic!("expected retained response"),
    }
    assert!(guards >= 2);
    assert_eq!(store.pending_usage().unwrap(), before);
    assert_eq!(
        begin(&mut store, &journal, KEY, &request(), PACKAGE, || Err(
            morrow_core::Error::Integrity
        ))
        .err(),
        Some(Error::Denied)
    );
    assert_eq!(store.pending_usage().unwrap(), before);
}
#[test]
fn changed_input_package_or_retention_cannot_reuse_the_original_key() {
    let dir = tempfile::tempdir().unwrap();
    let time = Arc::new(AtomicU64::new(100));
    let journal = journal(&time);
    let mut store = Store::open(&dir.path().join("history.db"), EventBudget::default()).unwrap();
    let (record, _) = execute(&mut store, &journal);
    finish(&mut store, &record, &response(&request(), b"result")).unwrap();
    let before = store.pending_usage().unwrap();
    assert_eq!(
        begin(
            &mut store,
            &journal,
            KEY,
            &request_with(b"changed"),
            PACKAGE,
            || Ok(())
        )
        .err(),
        Some(Error::Conflict)
    );
    assert_eq!(
        begin(&mut store, &journal, KEY, &request(), [9; 32], || Ok(())).err(),
        Some(Error::Conflict)
    );
    let different = ServiceJournal::new(
        Policy {
            retention_ms: 999,
            ..policy()
        },
        || 100,
    )
    .unwrap();
    assert_eq!(
        begin(&mut store, &different, KEY, &request(), PACKAGE, || Ok(())).err(),
        Some(Error::Conflict)
    );
    assert_eq!(store.pending_usage().unwrap(), before);
}
#[test]
fn prepared_recovery_keeps_original_creation_and_unknown_never_reexecutes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let prepared = prepare(&mut store, true);
    drop(store);
    let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
    let time = Arc::new(AtomicU64::new(500));
    let journal = journal(&time);
    let (record, validity) = execute(&mut store, &journal);
    assert_eq!(record.command(), prepared.command());
    assert_eq!(original(&store, &record).unwrap().created_ms(), 100);
    let before = store.pending_usage().unwrap();
    assert!(matches!(
        begin(&mut store, &journal, KEY, &request(), PACKAGE, || Ok(())).unwrap(),
        Begin::Unknown
    ));
    assert_eq!(store.pending_usage().unwrap(), before);
    time.store(1100, Ordering::SeqCst);
    assert_eq!(validity.check(), Err(Error::Expired));
    assert!(matches!(
        begin(&mut store, &journal, KEY, &request(), PACKAGE, || Ok(())).unwrap(),
        Begin::Expired
    ));
    assert_eq!(store.pending_usage().unwrap(), before);
}
#[test]
fn validity_is_retained_and_clone_clock_rejects_regression_even_above_creation() {
    let dir = tempfile::tempdir().unwrap();
    let time = Arc::new(AtomicU64::new(100));
    let journal = journal(&time);
    let clone = journal.clone();
    let mut store = Store::open(&dir.path().join("history.db"), EventBudget::default()).unwrap();
    let (record, validity) = execute(&mut store, &journal);
    finish(&mut store, &record, &response(&request(), b"result")).unwrap();
    time.store(600, Ordering::SeqCst);
    let replay_validity =
        match begin(&mut store, &clone, KEY, &request(), PACKAGE, || Ok(())).unwrap() {
            Begin::Replay { validity, .. } => validity,
            _ => panic!("expected replay"),
        };
    time.store(599, Ordering::SeqCst);
    assert_eq!(validity.check(), Err(Error::Clock));
    assert_eq!(replay_validity.check(), Err(Error::Clock));
    time.store(1100, Ordering::SeqCst);
    assert_eq!(replay_validity.check(), Err(Error::Expired));
    assert!(matches!(
        begin(&mut store, &clone, KEY, &request(), PACKAGE, || Ok(())).unwrap(),
        Begin::Expired
    ));
}
#[test]
fn missing_original_or_observed_without_response_fails_closed() {
    for observed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut store =
            Store::open(&dir.path().join("history.db"), EventBudget::default()).unwrap();
        let time = Arc::new(AtomicU64::new(100));
        let journal = journal(&time);
        if observed {
            let (record, _) = execute(&mut store, &journal);
            let c = record.command();
            store
                .release_io_material_reconciliation(&c.subject, &c.operation_id, Kind::Response)
                .unwrap();
            let reconciled = record
                .propose_observation([2; 32], ObservationSource::Reconciliation)
                .unwrap();
            store
                .append_io_intent_local_authorized(&reconciled, || Ok(()))
                .unwrap();
        } else {
            prepare(&mut store, false);
        }
        let before = store.pending_usage().unwrap();
        assert_eq!(
            begin(&mut store, &journal, KEY, &request(), PACKAGE, || Ok(())).err(),
            Some(Error::EvidenceUnavailable)
        );
        assert_eq!(store.pending_usage().unwrap(), before);
        store.integrity_check().unwrap();
    }
}
#[test]
fn final_claim_guard_failure_leaves_prepared_and_quota_failure_never_dispatches() {
    let dir = tempfile::tempdir().unwrap();
    let time = Arc::new(AtomicU64::new(100));
    let journal = journal(&time);
    let path = dir.path().join("history.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, true);
    let mut guards = 0;
    let result = begin(&mut store, &journal, KEY, &request(), PACKAGE, || {
        guards += 1;
        if guards == 6 {
            Err(morrow_core::Error::Integrity)
        } else {
            Ok(())
        }
    });
    assert_eq!(guards, 6);
    assert_eq!(result.err(), Some(Error::Denied));
    let proposed = RequestRecord::encode(&policy(), KEY, &request(), 100)
        .unwrap()
        .command(PACKAGE)
        .unwrap();
    let stored = store
        .lookup_io_intent(&proposed.subject, &proposed.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.phase(), Phase::Prepared);
    execute(&mut store, &journal);
    let mut limited = Store::open(
        &dir.path().join("limited.db"),
        EventBudget {
            max_count: 100,
            max_bytes: 100_000,
        },
    )
    .unwrap();
    assert_eq!(
        begin(&mut limited, &journal, KEY, &request(), PACKAGE, || Ok(())).err(),
        Some(Error::Limit)
    );
    assert!(
        limited
            .lookup_io_intent(&proposed.subject, &proposed.operation_id)
            .unwrap()
            .is_none()
    );
    limited.integrity_check().unwrap();
}
#[test]
fn finish_rejects_bad_frame_and_conflicting_completion_without_overwriting_history() {
    let dir = tempfile::tempdir().unwrap();
    let time = Arc::new(AtomicU64::new(100));
    let journal = journal(&time);
    let mut store = Store::open(&dir.path().join("history.db"), EventBudget::default()).unwrap();
    let (record, _) = execute(&mut store, &journal);
    let wrong = response(&request_with(b"different"), b"result");
    assert_eq!(finish(&mut store, &record, &wrong), Err(Error::Integrity));
    assert_eq!(
        store
            .lookup_io_intent(&record.command().subject, &record.command().operation_id)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::OutcomeUnknown
    );
    let completion = response(&request(), b"original-result");
    finish(&mut store, &record, &completion).unwrap();
    let before = store.pending_usage().unwrap();
    assert_eq!(
        finish(
            &mut store,
            &record,
            &response(&request(), b"different-result")
        ),
        Err(Error::Conflict)
    );
    assert_eq!(store.pending_usage().unwrap(), before);
    let material = store
        .io_material(
            &record.command().subject,
            &record.command().operation_id,
            Kind::Response,
        )
        .unwrap()
        .unwrap();
    assert_eq!(material.payload(), completion);
}
#[test]
fn two_connections_racing_the_same_prepared_request_have_exactly_one_execute() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let mut first = Store::open(&path, EventBudget::default()).unwrap();
    let prepared = prepare(&mut first, true);
    let second = Store::open_existing(&path, EventBudget::default()).unwrap();
    let gate = Arc::new(Barrier::new(3));
    let mut threads = Vec::new();
    for mut store in [first, second] {
        let gate = gate.clone();
        threads.push(std::thread::spawn(move || {
            let journal = ServiceJournal::new(policy(), || 100).unwrap();
            // The only synchronization is before any Store transaction starts.
            gate.wait();
            begin(&mut store, &journal, KEY, &request(), PACKAGE, || Ok(()))
        }));
    }
    gate.wait();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Ok(Begin::Execute { .. })))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(
                r,
                Ok(Begin::Unknown) | Err(Error::Conflict | Error::Storage)
            ))
            .count(),
        1
    );
    let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert!(matches!(
        begin(
            &mut store,
            &ServiceJournal::new(policy(), || 100).unwrap(),
            KEY,
            &request(),
            PACKAGE,
            || Ok(())
        )
        .unwrap(),
        Begin::Unknown
    ));
    assert_eq!(
        store
            .lookup_io_intent(
                &prepared.command().subject,
                &prepared.command().operation_id
            )
            .unwrap()
            .unwrap()
            .phase(),
        Phase::OutcomeUnknown
    );
    assert_eq!(store.pending_usage().unwrap().0, 2);
    store.integrity_check().unwrap();
}
#[test]
fn invalid_policy_and_zero_utc_fail_without_any_persisted_record() {
    assert_eq!(
        ServiceJournal::new(
            Policy {
                namespace: [0; 32],
                retention_ms: 1
            },
            || 100
        )
        .err(),
        Some(Error::Conflict)
    );
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("history.db"), EventBudget::default()).unwrap();
    let journal = ServiceJournal::new(policy(), || 0).unwrap();
    assert_eq!(
        begin(&mut store, &journal, KEY, &request(), PACKAGE, || Ok(())).err(),
        Some(Error::Clock)
    );
    assert_eq!(store.pending_usage().unwrap().0, 0);
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "child harness; parent supplies isolated database and crash point"]
fn claim_commit_crash_child() {
    let path = std::env::var_os("MORROW_SERVICE_HISTORY_CRASH_DB").expect("child database");
    let mut store =
        Store::open_existing(std::path::Path::new(&path), EventBudget::default()).unwrap();
    let journal = ServiceJournal::new(policy(), || 100).unwrap();
    let _ = begin(&mut store, &journal, KEY, &request(), PACKAGE, || Ok(()));
    panic!("expected real process exit after the dispatch claim committed");
}
#[cfg(feature = "fault-injection")]
#[test]
fn crash_after_committed_claim_reopens_unknown_without_executing_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("claim-crash.db");
    let store = Store::open(&path, EventBudget::default()).unwrap();
    drop(store);
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "service_history::tests::claim_commit_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env("MORROW_SERVICE_HISTORY_CRASH_DB", &path)
        .env("MORROW_TEST_CRASH_AT", "io-intent-after-commit")
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                assert_eq!(status.code(), Some(86));
                break;
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("crash child did not exit within deadline: {result:?}");
            }
        }
    }
    let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
    let journal = ServiceJournal::new(policy(), || 100).unwrap();
    assert!(matches!(
        begin(&mut store, &journal, KEY, &request(), PACKAGE, || Ok(())).unwrap(),
        Begin::Unknown
    ));
    assert_eq!(store.pending_usage().unwrap().0, 2);
    let retained = RequestRecord::encode(&policy(), KEY, &request(), 100).unwrap();
    let command = retained.command(PACKAGE).unwrap();
    assert_eq!(
        store
            .lookup_io_intent(&command.subject, &command.operation_id)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::OutcomeUnknown
    );
    assert_eq!(
        store
            .io_material(&command.subject, &command.operation_id, Kind::Request)
            .unwrap()
            .unwrap()
            .payload(),
        retained.container()
    );
    store.integrity_check().unwrap();
}
