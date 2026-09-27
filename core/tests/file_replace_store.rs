//! Replace claim/observation evidence only; database fixtures never modify a target file.
#![cfg(not(target_arch = "wasm32"))]

use morrow_core::{
    Error,
    audit::{self, SigningKey, TrustedLog},
    file_content::FileContent,
    file_effect::{ReplaceOutcome, ReplaceResult},
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
    io_evidence::{Kind, Material},
    io_intent::{ObservationSource, Phase, Record, Recovery},
    store::{EventBudget, Store},
};
use sha2::{Digest, Sha256};
use std::path::Path;

const OP: &str = "file-replace-operation";
const SUBJECT: &str = "plugin.replace-test";
const BYTES: &[u8] = b"replacement bytes retained before claim";

fn request(disposition: Disposition, path: Option<&str>) -> MutationRequest {
    MutationRequest {
        operation_id: OP.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: path.map(|v| RelativeFilePath::parse(v).unwrap()),
        },
        disposition,
        expected_identity: if disposition == Disposition::Create {
            None
        } else {
            Some([4; 32])
        },
        content_length: BYTES.len() as u64,
        content_sha256: Some(Sha256::digest(BYTES).into()),
    }
}

fn plan(value: MutationRequest) -> (Record, Material, FileContent) {
    let original = RequestRecord::new(value).unwrap();
    let prepared = Record::prepared(original.command().unwrap()).unwrap();
    let command = prepared.command();
    let material = Material::encode(
        Kind::Request,
        &command.operation_id,
        &command.subject,
        command.request_sha256,
        original.container(),
    )
    .unwrap();
    let content = FileContent::new(OP, &command.subject, command.request_sha256, BYTES).unwrap();
    (prepared, material, content)
}

fn response(prepared: &Record, result: ReplaceResult) -> Material {
    let outcome = ReplaceOutcome::new(
        OP,
        SUBJECT,
        prepared.command().request_sha256,
        [3; 32],
        [4; 32],
        Sha256::digest(BYTES).into(),
        BYTES.len() as u64,
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

fn observed(unknown: &Record, material: &Material) -> Record {
    unknown
        .propose_observation(material.digest(), ObservationSource::OriginalResponse)
        .unwrap()
}

fn ready(store: &mut Store, prepared: &Record, request: &Material, content: &FileContent) {
    store
        .prepare_file_mutation_local_authorized(prepared, request, || Ok(()))
        .unwrap();
    store
        .stage_file_mutation_content_local_authorized(content, || Ok(()))
        .unwrap();
}

fn phase(store: &Store) -> Phase {
    store
        .lookup_io_intent(SUBJECT, OP)
        .unwrap()
        .unwrap()
        .phase()
}

fn retained(store: &Store, content: &FileContent) {
    assert_eq!(
        store
            .file_mutation_content_local_authorized(SUBJECT, OP, || Ok(()))
            .unwrap()
            .unwrap()
            .container(),
        content.container()
    );
    let receipt = store
        .file_mutation_content_receipt_local_authorized(SUBJECT, OP, || Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(receipt.content_sha256(), content.content_sha256());
    assert_eq!(receipt.request_sha256(), content.request_sha256());
    assert_eq!(receipt.content_length(), BYTES.len() as u64);
}

fn rows(path: &Path) -> (i64, i64, i64) {
    let db = rusqlite::Connection::open(path).unwrap();
    (
        db.query_row("SELECT count(*) FROM io_intents", [], |r| r.get(0))
            .unwrap(),
        db.query_row("SELECT count(*) FROM file_mutation_content", [], |r| {
            r.get(0)
        })
        .unwrap(),
        db.query_row("SELECT count(*) FROM file_content_receipts", [], |r| {
            r.get(0)
        })
        .unwrap(),
    )
}

#[test]
fn selected_single_file_and_relative_entry_claim_once_with_exact_retained_bytes() {
    for relative in [None, Some("notes/existing.txt")] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claim.db");
        let (prepared, material, content) = plan(request(Disposition::Replace, relative));
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        ready(&mut store, &prepared, &material, &content);
        assert!(matches!(
            store.claim_io_dispatch_local_authorized(&unknown, || Ok(())),
            Err(Error::UnsupportedVersion)
        ));
        assert!(matches!(
            store.append_io_intent_local_authorized(&unknown, || Ok(())),
            Err(Error::UnsupportedVersion)
        ));
        let mut checks = 0;
        assert_eq!(
            store
                .claim_file_replace_local_authorized(&unknown, || {
                    checks += 1;
                    Ok(())
                })
                .unwrap()
                .container(),
            unknown.container()
        );
        assert!(checks >= 3);
        assert_eq!(phase(&store), Phase::OutcomeUnknown);
        assert_eq!(
            store
                .lookup_io_intent(SUBJECT, OP)
                .unwrap()
                .unwrap()
                .recovery(),
            Recovery::ReconcileOnly
        );
        assert_eq!(rows(&path), (2, 1, 1));
        retained(&store, &content);
        let valid_response = response(&prepared, ReplaceResult::Replaced);
        let reservation = store.io_material_reservation_usage().unwrap();
        assert!(matches!(
            store.store_io_material(SUBJECT, Kind::Response, &valid_response, || Ok(())),
            Err(Error::UnsupportedVersion)
        ));
        assert!(matches!(
            store.release_io_material_reconciliation(SUBJECT, OP, Kind::Response),
            Err(Error::UnsupportedVersion)
        ));
        assert_eq!(store.io_material_reservation_usage().unwrap(), reservation);
        assert!(matches!(
            store
                .append_io_intent_local_authorized(&observed(&unknown, &valid_response), || Ok(())),
            Err(Error::UnsupportedVersion)
        ));
        assert!(matches!(
            store.claim_file_replace_local_authorized(&unknown, || Ok(())),
            Err(Error::RevisionConflict)
        ));
        assert!(
            store
                .stage_file_mutation_content_local_authorized(&content, || Ok(()))
                .is_err()
        );
        assert!(matches!(
            store.io_material(SUBJECT, OP, Kind::Response),
            Err(Error::EvidenceUnavailable)
        ));
        store.integrity_check().unwrap();
        drop(store);
        let mut reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
        assert_eq!(phase(&reopened), Phase::OutcomeUnknown);
        retained(&reopened, &content);
        assert!(matches!(
            reopened.claim_file_replace_local_authorized(&unknown, || Ok(())),
            Err(Error::RevisionConflict)
        ));
    }
}

#[test]
fn missing_content_receipt_or_response_reservation_prevents_claim() {
    for missing in ["content", "receipt", "reservation"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");
        let (prepared, material, content) = plan(request(Disposition::Replace, None));
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        store
            .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
            .unwrap();
        if missing != "content" {
            store
                .stage_file_mutation_content_local_authorized(&content, || Ok(()))
                .unwrap();
        }
        if missing == "receipt" {
            rusqlite::Connection::open(&path)
                .unwrap()
                .execute(
                    "DELETE FROM file_content_receipts WHERE operation_id=?1",
                    [OP],
                )
                .unwrap();
        }
        if missing == "reservation" {
            store.release_io_materials(SUBJECT, OP).unwrap();
        }
        let before = rows(&path);
        assert!(
            store
                .claim_file_replace_local_authorized(&unknown, || Ok(()))
                .is_err(),
            "{missing}"
        );
        assert_eq!(rows(&path), before, "{missing}");
        if missing != "receipt" {
            assert_eq!(phase(&store), Phase::Prepared);
            store.integrity_check().unwrap();
        }
    }
}

#[test]
fn wrong_disposition_command_subject_target_and_expected_identity_cannot_claim() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("binding.db");
    let (prepared, material, content) = plan(request(Disposition::Replace, None));
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    ready(&mut store, &prepared, &material, &content);
    for value in [
        {
            let mut v = request(Disposition::Replace, None);
            v.subject = "plugin.other".into();
            v
        },
        {
            let mut v = request(Disposition::Replace, None);
            v.target.reference = [9; 32];
            v
        },
        {
            let mut v = request(Disposition::Replace, None);
            v.expected_identity = Some([9; 32]);
            v
        },
        request(Disposition::Replace, Some("notes/other.txt")),
        request(Disposition::Create, None),
    ] {
        let (other, _, _) = plan(value);
        assert!(
            store
                .claim_file_replace_local_authorized(
                    &other.propose_dispatch_boundary().unwrap(),
                    || Ok(())
                )
                .is_err()
        );
        assert_eq!(phase(&store), Phase::Prepared);
    }
    assert!(
        store
            .claim_file_replace_local_authorized(&prepared, || Ok(()))
            .is_err()
    );
    assert_eq!(rows(&path), (1, 1, 1));
    store.integrity_check().unwrap();
}

#[test]
fn claim_authorization_denial_before_read_or_precommit_rolls_back() {
    let (prepared, material, content) = plan(request(Disposition::Replace, None));
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    for denied in [1, 3] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("denied.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        ready(&mut store, &prepared, &material, &content);
        let pending = store.pending(0, 10).unwrap();
        let mut checks = 0;
        assert!(matches!(
            store.claim_file_replace_local_authorized(&unknown, || {
                checks += 1;
                if checks == denied {
                    Err(Error::Invalid("claim denied"))
                } else {
                    Ok(())
                }
            }),
            Err(Error::Invalid("claim denied"))
        ));
        assert_eq!(checks, denied);
        assert_eq!(phase(&store), Phase::Prepared);
        assert_eq!(store.pending(0, 10).unwrap(), pending);
        store.integrity_check().unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deny-before-protected-read.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    ready(&mut store, &prepared, &material, &content);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE file_mutation_content SET content_sha256=zeroblob(32) WHERE operation_id=?1",
            [OP],
        )
        .unwrap();
    assert!(matches!(
        store.claim_file_replace_local_authorized(&unknown, || Err(Error::Invalid("deny first"))),
        Err(Error::Invalid("deny first"))
    ));
    assert!(
        store
            .claim_file_replace_local_authorized(&unknown, || Ok(()))
            .is_err()
    );
    assert_eq!(rows(&path).0, 1);
}

#[test]
fn observed_replace_binds_all_fields_and_survives_sealed_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("observed.db");
    let snapshot = dir.path().join("snapshot.db");
    let key = SigningKey::from_bytes(&[93; 32]);
    let trust = TrustedLog {
        id: "file-replace-log".into(),
        key: key.verifying_key(),
    };
    let (prepared, material, content) = plan(request(Disposition::Replace, None));
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let outcome = response(&prepared, ReplaceResult::OsRejected { code: 5 });
    let candidate = observed(&unknown, &outcome);
    let mut store =
        Store::open_audited(&path, EventBudget::default(), true, trust.clone()).unwrap();
    ready(&mut store, &prepared, &material, &content);
    store
        .claim_file_replace_local_authorized(&unknown, || Ok(()))
        .unwrap();
    let mut checks = 0;
    assert_eq!(
        store
            .observe_file_replace_local_authorized(&candidate, &outcome, || {
                checks += 1;
                Ok(())
            })
            .unwrap()
            .container(),
        candidate.container()
    );
    assert!(checks >= 4);
    assert_eq!(phase(&store), Phase::Observed);
    assert_eq!(
        store
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .recovery(),
        Recovery::AlreadyObserved
    );
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    assert_eq!(
        store
            .io_material(SUBJECT, OP, Kind::Response)
            .unwrap()
            .unwrap()
            .container(),
        outcome.container()
    );
    retained(&store, &content);
    assert!(matches!(
        store.observe_file_replace_local_authorized(&candidate, &outcome, || Ok(())),
        Err(Error::RevisionConflict)
    ));
    let pending = store.pending(0, 10).unwrap();
    assert_eq!(pending.len(), 4);
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &pending).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    assert!(store.seal_pending(&signed).unwrap());
    store.snapshot_to(&snapshot, 16 * 1024 * 1024).unwrap();
    drop(store);
    for source in [&path, &snapshot] {
        let reopened = Store::open_read_only_audited(source, trust.clone()).unwrap();
        assert_eq!(phase(&reopened), Phase::Observed);
        retained(&reopened, &content);
        assert_eq!(
            reopened
                .io_material(SUBJECT, OP, Kind::Response)
                .unwrap()
                .unwrap()
                .container(),
            outcome.container()
        );
        assert_eq!(reopened.sealed_segment(1).unwrap().unwrap(), signed);
        reopened.integrity_check().unwrap();
    }
}

#[test]
fn forged_replace_outcomes_and_observation_digest_do_not_consume_response_slot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("forged.db");
    let (prepared, material, content) = plan(request(Disposition::Replace, None));
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    ready(&mut store, &prepared, &material, &content);
    store
        .claim_file_replace_local_authorized(&unknown, || Ok(()))
        .unwrap();
    let reservation = store.io_material_reservation_usage().unwrap();
    for (op, subject, req, target, expected, digest, length) in [
        (
            "other-op",
            SUBJECT,
            prepared.command().request_sha256,
            [3; 32],
            [4; 32],
            content.content_sha256(),
            BYTES.len() as u64,
        ),
        (
            OP,
            "plugin.other",
            prepared.command().request_sha256,
            [3; 32],
            [4; 32],
            content.content_sha256(),
            BYTES.len() as u64,
        ),
        (
            OP,
            SUBJECT,
            [9; 32],
            [3; 32],
            [4; 32],
            content.content_sha256(),
            BYTES.len() as u64,
        ),
        (
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            [9; 32],
            [4; 32],
            content.content_sha256(),
            BYTES.len() as u64,
        ),
        (
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            [3; 32],
            [9; 32],
            content.content_sha256(),
            BYTES.len() as u64,
        ),
        (
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            [3; 32],
            [4; 32],
            [9; 32],
            BYTES.len() as u64,
        ),
        (
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            [3; 32],
            [4; 32],
            content.content_sha256(),
            BYTES.len() as u64 + 1,
        ),
    ] {
        let bad = ReplaceOutcome::new(
            op,
            subject,
            req,
            target,
            expected,
            digest,
            length,
            ReplaceResult::Replaced,
        )
        .unwrap();
        let outer = Material::encode(
            Kind::Response,
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            bad.container(),
        )
        .unwrap();
        assert!(
            store
                .observe_file_replace_local_authorized(&observed(&unknown, &outer), &outer, || Ok(
                    ()
                ))
                .is_err()
        );
        assert_eq!(phase(&store), Phase::OutcomeUnknown);
        assert_eq!(store.io_material_reservation_usage().unwrap(), reservation);
    }
    let valid = response(&prepared, ReplaceResult::Replaced);
    let wrong_digest = unknown
        .propose_observation([7; 32], ObservationSource::OriginalResponse)
        .unwrap();
    assert!(
        store
            .observe_file_replace_local_authorized(&wrong_digest, &valid, || Ok(()))
            .is_err()
    );
    assert!(matches!(
        store.io_material(SUBJECT, OP, Kind::Response),
        Err(Error::EvidenceUnavailable)
    ));
    store.integrity_check().unwrap();
}

#[test]
fn observation_authorization_denial_rolls_back_both_writes() {
    let (prepared, material, content) = plan(request(Disposition::Replace, None));
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let response = response(&prepared, ReplaceResult::Replaced);
    let candidate = observed(&unknown, &response);
    for denied in [1, 4] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("denied-observe.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        ready(&mut store, &prepared, &material, &content);
        store
            .claim_file_replace_local_authorized(&unknown, || Ok(()))
            .unwrap();
        let pending = store.pending(0, 10).unwrap();
        let reservation = store.io_material_reservation_usage().unwrap();
        let mut checks = 0;
        assert!(matches!(
            store.observe_file_replace_local_authorized(&candidate, &response, || {
                checks += 1;
                if checks == denied {
                    Err(Error::Invalid("observe denied"))
                } else {
                    Ok(())
                }
            }),
            Err(Error::Invalid("observe denied"))
        ));
        assert_eq!(checks, denied);
        assert_eq!(phase(&store), Phase::OutcomeUnknown);
        assert_eq!(store.pending(0, 10).unwrap(), pending);
        assert_eq!(store.io_material_reservation_usage().unwrap(), reservation);
        assert!(matches!(
            store.io_material(SUBJECT, OP, Kind::Response),
            Err(Error::EvidenceUnavailable)
        ));
        store.integrity_check().unwrap();
    }
}

#[test]
fn claimed_history_rejects_removed_content_receipt_or_both_and_observation() {
    for missing in ["content", "receipt", "both"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tampered.db");
        let (prepared, material, content) = plan(request(Disposition::Replace, None));
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let response = response(&prepared, ReplaceResult::Replaced);
        let candidate = observed(&unknown, &response);
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        ready(&mut store, &prepared, &material, &content);
        store
            .claim_file_replace_local_authorized(&unknown, || Ok(()))
            .unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        if matches!(missing, "receipt" | "both") {
            let event: String = db
                .query_row(
                    "SELECT event_id FROM file_content_receipts WHERE operation_id=?1",
                    [OP],
                    |r| r.get(0),
                )
                .unwrap();
            db.execute(
                "DELETE FROM file_content_receipts WHERE operation_id=?1",
                [OP],
            )
            .unwrap();
            for table in ["outbox", "operation_events", "operations"] {
                db.execute(&format!("DELETE FROM {table} WHERE id=?1"), [&event])
                    .unwrap();
            }
        }
        if matches!(missing, "content" | "both") {
            db.execute(
                "DELETE FROM file_mutation_content WHERE operation_id=?1",
                [OP],
            )
            .unwrap();
        }
        drop(db);
        assert!(
            store
                .observe_file_replace_local_authorized(&candidate, &response, || Ok(()))
                .is_err()
        );
        assert_eq!(rows(&path).0, 2);
        assert!(store.integrity_check().is_err());
        drop(store);
        assert!(Store::open_existing(&path, EventBudget::default()).is_err());
    }
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "child harness; parent supplies isolated database and crash point"]
fn file_replace_crash_child() {
    let path = std::env::var_os("MORROW_FILE_REPLACE_CRASH_DB").expect("child database path");
    let mode = std::env::var("MORROW_FILE_REPLACE_CRASH_MODE").expect("child mode");
    let mut store = Store::open_existing(Path::new(&path), EventBudget::default()).unwrap();
    let (prepared, _, _) = plan(request(Disposition::Replace, None));
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    match mode.as_str() {
        "claim" => {
            store
                .claim_file_replace_local_authorized(&unknown, || Ok(()))
                .unwrap();
        }
        "observe" => {
            let response = response(&prepared, ReplaceResult::Replaced);
            store
                .observe_file_replace_local_authorized(
                    &observed(&unknown, &response),
                    &response,
                    || Ok(()),
                )
                .unwrap();
        }
        _ => panic!("invalid child mode"),
    }
    panic!("replace crash boundary was not reached");
}

#[cfg(feature = "fault-injection")]
fn crash(path: &Path, mode: &str, point: &str) {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "file_replace_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env("MORROW_FILE_REPLACE_CRASH_DB", path)
        .env("MORROW_FILE_REPLACE_CRASH_MODE", mode)
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
fn real_process_crashes_preserve_only_whole_replace_claim_and_observation() {
    for (mode, point, expected) in [
        ("claim", "file-replace-claim-before-commit", Phase::Prepared),
        (
            "claim",
            "file-replace-claim-after-commit",
            Phase::OutcomeUnknown,
        ),
        (
            "observe",
            "file-replace-observe-before-commit",
            Phase::OutcomeUnknown,
        ),
        (
            "observe",
            "file-replace-observe-after-commit",
            Phase::Observed,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.db");
        let (prepared, material, content) = plan(request(Disposition::Replace, None));
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let response = response(&prepared, ReplaceResult::Replaced);
        let candidate = observed(&unknown, &response);
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        ready(&mut store, &prepared, &material, &content);
        if mode == "observe" {
            store
                .claim_file_replace_local_authorized(&unknown, || Ok(()))
                .unwrap();
        }
        drop(store);
        crash(&path, mode, point);
        let mut reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
        assert_eq!(phase(&reopened), expected, "{point}");
        retained(&reopened, &content);
        if expected == Phase::Observed {
            assert_eq!(
                reopened
                    .io_material(SUBJECT, OP, Kind::Response)
                    .unwrap()
                    .unwrap()
                    .container(),
                response.container()
            );
            assert!(matches!(
                reopened.observe_file_replace_local_authorized(&candidate, &response, || Ok(())),
                Err(Error::RevisionConflict)
            ));
        } else {
            assert!(matches!(
                reopened.io_material(SUBJECT, OP, Kind::Response),
                Err(Error::EvidenceUnavailable)
            ));
            if expected == Phase::Prepared {
                reopened
                    .claim_file_replace_local_authorized(&unknown, || Ok(()))
                    .unwrap();
            } else if mode == "observe" {
                reopened
                    .observe_file_replace_local_authorized(&candidate, &response, || Ok(()))
                    .unwrap();
            } else {
                assert!(matches!(
                    reopened.claim_file_replace_local_authorized(&unknown, || Ok(())),
                    Err(Error::RevisionConflict)
                ));
            }
        }
        reopened.integrity_check().unwrap();
    }
}
