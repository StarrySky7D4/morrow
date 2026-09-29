#![cfg(not(target_arch = "wasm32"))]

use morrow_core::{
    Error,
    audit::{self, SigningKey, TrustedLog},
    file_effect::{DeleteOutcome, DeleteResult},
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    io_evidence::{Kind, Material},
    io_intent::{ObservationSource, Phase, Record, Recovery},
    store::{EventBudget, Store},
};

const OP: &str = "file-delete-operation";
const SUBJECT: &str = "plugin.delete-test";

fn prepared() -> (Record, Material) {
    let request = RequestRecord::new(MutationRequest {
        operation_id: OP.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: None,
        },
        disposition: Disposition::Delete,
        expected_identity: Some([4; 32]),
        content_length: 0,
        content_sha256: None,
    })
    .unwrap();
    let prepared = Record::prepared(request.command().unwrap()).unwrap();
    let command = prepared.command();
    let material = Material::encode(
        Kind::Request,
        OP,
        SUBJECT,
        command.request_sha256,
        request.container(),
    )
    .unwrap();
    (prepared, material)
}

fn response(prepared: &Record, result: DeleteResult) -> Material {
    let outcome = DeleteOutcome::new(
        OP,
        SUBJECT,
        prepared.command().request_sha256,
        [3; 32],
        [4; 32],
        result,
    )
    .unwrap();
    Material::encode(
        Kind::Response,
        OP,
        SUBJECT,
        prepared.command().request_sha256,
        outcome.container(),
    )
    .unwrap()
}

fn observed(unknown: &Record, response: &Material) -> Record {
    unknown
        .propose_observation(response.digest(), ObservationSource::OriginalResponse)
        .unwrap()
}

#[test]
fn claim_is_one_way_and_reopening_unknown_never_reclaims() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("delete.db");
    let (prepared, request) = prepared();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store
        .prepare_file_mutation_local_authorized(&prepared, &request, || Ok(()))
        .unwrap();
    assert!(matches!(
        store.claim_io_dispatch_local_authorized(&unknown, || Ok(())),
        Err(Error::UnsupportedVersion)
    ));
    assert!(matches!(
        store.append_io_intent_local_authorized(&unknown, || Ok(())),
        Err(Error::UnsupportedVersion)
    ));
    let mut checks = 0;
    store
        .claim_file_delete_local_authorized(&unknown, || {
            checks += 1;
            Ok(())
        })
        .unwrap();
    assert!(checks >= 2);
    assert!(matches!(
        store.claim_file_delete_local_authorized(&unknown, || Ok(())),
        Err(Error::RevisionConflict)
    ));
    assert_eq!(
        store
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::OutcomeUnknown
    );
    assert_eq!(
        store
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .recovery(),
        Recovery::ReconcileOnly
    );
    assert!(matches!(
        store.io_material(SUBJECT, OP, Kind::Response),
        Err(Error::EvidenceUnavailable)
    ));
    store.integrity_check().unwrap();
    drop(store);
    let mut reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert_eq!(
        reopened
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .container(),
        unknown.container()
    );
    assert!(matches!(
        reopened.claim_file_delete_local_authorized(&unknown, || Ok(())),
        Err(Error::RevisionConflict)
    ));
    reopened.integrity_check().unwrap();
}

#[test]
fn authorization_denial_rolls_back_claim_and_observation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("denied.db");
    let (prepared, request) = prepared();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store
        .prepare_file_mutation_local_authorized(&prepared, &request, || Ok(()))
        .unwrap();
    assert!(
        store
            .claim_file_delete_local_authorized(&unknown, || Err(Error::Invalid("denied")))
            .is_err()
    );
    assert_eq!(
        store
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Prepared
    );
    store
        .claim_file_delete_local_authorized(&unknown, || Ok(()))
        .unwrap();
    let material = response(&prepared, DeleteResult::Deleted);
    let candidate = observed(&unknown, &material);
    assert!(
        store
            .observe_file_delete_local_authorized(&candidate, &material, || Err(Error::Invalid(
                "denied"
            )))
            .is_err()
    );
    assert_eq!(
        store
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::OutcomeUnknown
    );
    assert!(matches!(
        store.io_material(SUBJECT, OP, Kind::Response),
        Err(Error::EvidenceUnavailable)
    ));
    store.integrity_check().unwrap();
    store
        .observe_file_delete_local_authorized(&candidate, &material, || Ok(()))
        .unwrap();
    assert_eq!(
        store
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .container(),
        candidate.container()
    );
    assert_eq!(
        store
            .io_material(SUBJECT, OP, Kind::Response)
            .unwrap()
            .unwrap()
            .container(),
        material.container()
    );
    store.integrity_check().unwrap();
}

#[test]
fn forged_outcomes_cannot_be_observed_and_valid_rejection_is_auditable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audited.db");
    let before = dir.path().join("before.db");
    let after = dir.path().join("after.db");
    let key = SigningKey::from_bytes(&[61; 32]);
    let trust = TrustedLog {
        id: "file-delete-log".into(),
        key: key.verifying_key(),
    };
    let (prepared, request) = prepared();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store =
        Store::open_audited(&path, EventBudget::default(), true, trust.clone()).unwrap();
    store
        .prepare_file_mutation_local_authorized(&prepared, &request, || Ok(()))
        .unwrap();
    store
        .claim_file_delete_local_authorized(&unknown, || Ok(()))
        .unwrap();
    for (operation, subject, request_hash, target, expected) in [
        (
            "other-op",
            SUBJECT,
            prepared.command().request_sha256,
            [3; 32],
            [4; 32],
        ),
        (
            OP,
            "other-subject",
            prepared.command().request_sha256,
            [3; 32],
            [4; 32],
        ),
        (OP, SUBJECT, [9; 32], [3; 32], [4; 32]),
        (
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            [9; 32],
            [4; 32],
        ),
        (
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            [3; 32],
            [9; 32],
        ),
    ] {
        let bad = DeleteOutcome::new(
            operation,
            subject,
            request_hash,
            target,
            expected,
            DeleteResult::Deleted,
        )
        .unwrap();
        let response = Material::encode(
            Kind::Response,
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            bad.container(),
        )
        .unwrap();
        let candidate = observed(&unknown, &response);
        assert!(
            store
                .observe_file_delete_local_authorized(&candidate, &response, || Ok(()))
                .is_err()
        );
        assert!(matches!(
            store.io_material(SUBJECT, OP, Kind::Response),
            Err(Error::EvidenceUnavailable)
        ));
    }
    let good = response(&prepared, DeleteResult::OsRejected { code: 5 });
    let candidate = observed(&unknown, &good);
    let wrong_observation = unknown
        .propose_observation([7; 32], ObservationSource::OriginalResponse)
        .unwrap();
    assert!(
        store
            .observe_file_delete_local_authorized(&wrong_observation, &good, || Ok(()))
            .is_err()
    );
    store
        .observe_file_delete_local_authorized(&candidate, &good, || Ok(()))
        .unwrap();
    assert!(matches!(
        store.observe_file_delete_local_authorized(&candidate, &good, || Ok(())),
        Err(Error::RevisionConflict)
    ));
    assert!(matches!(
        store.append_io_intent_local_authorized(&candidate, || Ok(())),
        Err(Error::UnsupportedVersion)
    ));
    assert_eq!(
        store
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .recovery(),
        Recovery::AlreadyObserved
    );
    store.snapshot_to(&before, 16 * 1024 * 1024).unwrap();
    store.integrity_check().unwrap();
    let pending = store.pending(0, 10).unwrap();
    assert_eq!(pending.len(), 3);
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &pending).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    assert!(store.seal_pending(&signed).unwrap());
    store.snapshot_to(&after, 16 * 1024 * 1024).unwrap();
    drop(store);
    for source in [&path, &before, &after] {
        let reopened =
            Store::open_audited(source, EventBudget::default(), false, trust.clone()).unwrap();
        assert_eq!(
            reopened
                .lookup_io_intent(SUBJECT, OP)
                .unwrap()
                .unwrap()
                .container(),
            candidate.container()
        );
        assert_eq!(
            reopened
                .io_material(SUBJECT, OP, Kind::Response)
                .unwrap()
                .unwrap()
                .container(),
            good.container()
        );
        reopened.integrity_check().unwrap();
    }
}

#[test]
fn live_claim_rejects_delete_with_injected_content_or_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unexpected-content.db");
    let (prepared, request) = prepared();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store
        .prepare_file_mutation_local_authorized(&prepared, &request, || Ok(()))
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "INSERT INTO file_mutation_content(operation_id,subject,request_sha256,content_sha256,content_length,container) VALUES(?1,?2,?3,?4,1,?5)",
        rusqlite::params![OP, SUBJECT, prepared.command().request_sha256.as_slice(), [8u8;32].as_slice(), &[42u8][..]],
    ).unwrap();
    assert!(matches!(
        store.claim_file_delete_local_authorized(&unknown, || Err(Error::Invalid(
            "authorization denied"
        ))),
        Err(Error::Invalid("authorization denied"))
    ));
    assert!(matches!(
        store.claim_file_delete_local_authorized(&unknown, || Ok(())),
        Err(Error::Integrity)
    ));
    db.execute(
        "DELETE FROM file_mutation_content WHERE operation_id=?1",
        [OP],
    )
    .unwrap();
    db.execute(
        "INSERT INTO file_content_receipts(operation_id,event_id) VALUES(?1,?2)",
        rusqlite::params![OP, prepared.event_id()],
    )
    .unwrap();
    assert!(matches!(
        store.claim_file_delete_local_authorized(&unknown, || Ok(())),
        Err(Error::Integrity)
    ));
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "child harness; parent supplies an isolated database and crash point"]
fn file_delete_crash_child() {
    let path = std::env::var_os("MORROW_FILE_DELETE_CRASH_DB").expect("child database path");
    let mode = std::env::var("MORROW_FILE_DELETE_CRASH_MODE").expect("child mode");
    let mut store =
        Store::open_existing(std::path::Path::new(&path), EventBudget::default()).unwrap();
    let (prepared, _) = prepared();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    match mode.as_str() {
        "claim" => {
            store
                .claim_file_delete_local_authorized(&unknown, || Ok(()))
                .unwrap();
        }
        "observe" => {
            let response = response(&prepared, DeleteResult::Deleted);
            let candidate = observed(&unknown, &response);
            store
                .observe_file_delete_local_authorized(&candidate, &response, || Ok(()))
                .unwrap();
        }
        _ => panic!("invalid child mode"),
    }
    panic!("file delete crash boundary was not reached");
}

#[cfg(feature = "fault-injection")]
fn crash(path: &std::path::Path, mode: &str, point: &str) {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "file_delete_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env("MORROW_FILE_DELETE_CRASH_DB", path)
        .env("MORROW_FILE_DELETE_CRASH_MODE", mode)
        .env("MORROW_TEST_CRASH_AT", point)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
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
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe {point}: {error}");
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child timeout: {point}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn real_process_exits_preserve_only_whole_claim_and_observation_transactions() {
    for (mode, point) in [
        ("claim", "file-delete-claim-before-commit"),
        ("claim", "file-delete-claim-after-commit"),
        ("observe", "file-delete-observe-before-commit"),
        ("observe", "file-delete-observe-after-commit"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.db");
        let (prepared, request) = prepared();
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        store
            .prepare_file_mutation_local_authorized(&prepared, &request, || Ok(()))
            .unwrap();
        if mode == "observe" {
            store
                .claim_file_delete_local_authorized(&unknown, || Ok(()))
                .unwrap();
        }
        drop(store);
        crash(&path, mode, point);
        let mut reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
        let latest = reopened.lookup_io_intent(SUBJECT, OP).unwrap().unwrap();
        let expected_phase = match point {
            "file-delete-claim-before-commit" => Phase::Prepared,
            "file-delete-claim-after-commit" | "file-delete-observe-before-commit" => {
                Phase::OutcomeUnknown
            }
            "file-delete-observe-after-commit" => Phase::Observed,
            _ => unreachable!(),
        };
        assert_eq!(latest.phase(), expected_phase, "{point}");
        if expected_phase == Phase::Observed {
            let material = reopened
                .io_material(SUBJECT, OP, Kind::Response)
                .unwrap()
                .unwrap();
            assert_eq!(
                material.container(),
                response(&prepared, DeleteResult::Deleted).container()
            );
        } else {
            assert!(matches!(
                reopened.io_material(SUBJECT, OP, Kind::Response),
                Err(Error::EvidenceUnavailable)
            ));
        }
        if expected_phase == Phase::OutcomeUnknown {
            assert!(matches!(
                reopened.claim_file_delete_local_authorized(&unknown, || Ok(())),
                Err(Error::RevisionConflict)
            ));
        }
        reopened.integrity_check().unwrap();
    }
}
