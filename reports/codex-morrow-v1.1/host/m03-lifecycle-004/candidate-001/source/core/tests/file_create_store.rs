//! The durable create claim is a one-way boundary; this suite creates database fixtures only, never a mutation target.
#![cfg(not(target_arch = "wasm32"))]

use morrow_core::{
    Error,
    audit::{self, SigningKey, TrustedLog},
    file_content::FileContent,
    file_content_receipt::{Receipt, Source},
    file_effect::{CreateOutcome, CreateResult},
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
    io_evidence::{Kind, Material},
    io_intent::{ObservationSource, Phase, Record, Recovery},
    store::{EventBudget, Store},
};
use sha2::{Digest, Sha256};
use std::path::Path;

const OP: &str = "file-create-operation";
const SUBJECT: &str = "plugin.create-test";
const BYTES: &[u8] = b"the exact immutable bytes to create";

fn request(operation: &str, disposition: Disposition, path: Option<&str>) -> MutationRequest {
    MutationRequest {
        operation_id: operation.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: path.map(|value| RelativeFilePath::parse(value).unwrap()),
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
    plan_with_bytes(value, BYTES)
}

fn plan_with_bytes(value: MutationRequest, bytes: &[u8]) -> (Record, Material, FileContent) {
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
    let content = FileContent::new(
        &command.operation_id,
        &command.subject,
        command.request_sha256,
        bytes,
    )
    .unwrap();
    (prepared, material, content)
}

fn create_plan() -> (Record, Material, FileContent) {
    plan(request(OP, Disposition::Create, Some("notes/new.txt")))
}

fn prepare(store: &mut Store, prepared: &Record, material: &Material) {
    store
        .prepare_file_mutation_local_authorized(prepared, material, || Ok(()))
        .unwrap();
}

fn stage(store: &mut Store, content: &FileContent) {
    store
        .stage_file_mutation_content_local_authorized(content, || Ok(()))
        .unwrap();
}

fn retained(store: &Store, content: &FileContent) {
    let read = store
        .file_mutation_content_local_authorized(SUBJECT, OP, || Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(read.container(), content.container());
    let receipt = store
        .file_mutation_content_receipt_local_authorized(SUBJECT, OP, || Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(receipt.source(), Source::LiveStaging);
    assert_eq!(receipt.request_sha256(), content.request_sha256());
    assert_eq!(receipt.content_sha256(), content.content_sha256());
    assert_eq!(receipt.content_length(), BYTES.len() as u64);
    assert_eq!(
        receipt.content_container_sha256(),
        <[u8; 32]>::from(Sha256::digest(content.container()))
    );
}

fn phase(store: &Store) -> Phase {
    store
        .lookup_io_intent(SUBJECT, OP)
        .unwrap()
        .unwrap()
        .phase()
}

fn rows(path: &Path) -> (i64, i64, i64) {
    let sql = rusqlite::Connection::open(path).unwrap();
    (
        sql.query_row("SELECT count(*) FROM io_intents", [], |r| r.get(0))
            .unwrap(),
        sql.query_row("SELECT count(*) FROM file_mutation_content", [], |r| {
            r.get(0)
        })
        .unwrap(),
        sql.query_row("SELECT count(*) FROM file_content_receipts", [], |r| {
            r.get(0)
        })
        .unwrap(),
    )
}

#[test]
fn prepared_create_claims_once_and_retains_exact_originals_after_reopen_and_seal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("create.db");
    let pending_copy = dir.path().join("pending.db");
    let sealed_copy = dir.path().join("sealed.db");
    let key = SigningKey::from_bytes(&[91; 32]);
    let trust = TrustedLog {
        id: "file-create-log".into(),
        key: key.verifying_key(),
    };
    let (prepared, material, content) = create_plan();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store =
        Store::open_audited(&path, EventBudget::default(), true, trust.clone()).unwrap();
    prepare(&mut store, &prepared, &material);
    stage(&mut store, &content);
    assert_eq!(rows(&path), (1, 1, 1));
    assert!(matches!(
        store.claim_io_dispatch_local_authorized(&unknown, || Ok(())),
        Err(Error::UnsupportedVersion)
    ));
    assert!(matches!(
        store.append_io_intent_local_authorized(&unknown, || Ok(())),
        Err(Error::UnsupportedVersion)
    ));
    let mut checks = 0;
    let claimed = store
        .claim_file_create_local_authorized(&unknown, || {
            checks += 1;
            Ok(())
        })
        .unwrap();
    assert!(checks >= 2);
    assert_eq!(claimed.container(), unknown.container());
    assert_eq!(phase(&store), Phase::OutcomeUnknown);
    assert_eq!(
        store
            .lookup_io_intent(SUBJECT, OP)
            .unwrap()
            .unwrap()
            .recovery(),
        Recovery::ReconcileOnly
    );
    assert!(matches!(
        store.claim_file_create_local_authorized(&unknown, || Ok(())),
        Err(Error::RevisionConflict)
    ));
    assert!(
        store
            .stage_file_mutation_content_local_authorized(&content, || Ok(()))
            .is_err()
    );
    assert_eq!(rows(&path), (2, 1, 1));
    retained(&store, &content);
    let response = Material::encode(
        Kind::Response,
        OP,
        SUBJECT,
        prepared.command().request_sha256,
        b"untrusted generic response",
    )
    .unwrap();
    let reservation = store.io_material_reservation_usage().unwrap();
    assert!(matches!(
        store.store_io_material(SUBJECT, Kind::Response, &response, || Ok(())),
        Err(Error::UnsupportedVersion)
    ));
    assert_eq!(store.io_material_reservation_usage().unwrap(), reservation);
    assert!(matches!(
        store.release_io_material_reconciliation(SUBJECT, OP, Kind::Response),
        Err(Error::UnsupportedVersion)
    ));
    assert_eq!(store.io_material_reservation_usage().unwrap(), reservation);
    let observed = unknown
        .propose_observation(response.digest(), ObservationSource::OriginalResponse)
        .unwrap();
    assert!(matches!(
        store.append_io_intent_local_authorized(&observed, || Ok(())),
        Err(Error::UnsupportedVersion)
    ));
    assert_eq!(phase(&store), Phase::OutcomeUnknown);
    assert!(matches!(
        store.io_material(SUBJECT, OP, Kind::Response),
        Err(Error::EvidenceUnavailable)
    ));
    let pending = store.pending(0, 10).unwrap();
    assert_eq!(pending.len(), 3);
    store.snapshot_to(&pending_copy, 16 * 1024 * 1024).unwrap();
    store.integrity_check().unwrap();
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &pending).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    assert!(store.seal_pending(&signed).unwrap());
    store.snapshot_to(&sealed_copy, 16 * 1024 * 1024).unwrap();
    drop(store);
    for source in [&path, &pending_copy, &sealed_copy] {
        let reopened =
            Store::open_audited(source, EventBudget::default(), false, trust.clone()).unwrap();
        assert_eq!(phase(&reopened), Phase::OutcomeUnknown);
        assert_eq!(
            reopened
                .lookup_io_intent(SUBJECT, OP)
                .unwrap()
                .unwrap()
                .container(),
            unknown.container()
        );
        retained(&reopened, &content);
        reopened.integrity_check().unwrap();
    }
    let mut reopened = Store::open_audited(&path, EventBudget::default(), false, trust).unwrap();
    assert!(matches!(
        reopened.claim_file_create_local_authorized(&unknown, || Ok(())),
        Err(Error::RevisionConflict)
    ));
    assert!(
        reopened
            .stage_file_mutation_content_local_authorized(&content, || Ok(()))
            .is_err()
    );
}

#[test]
fn missing_original_path_content_or_receipt_cannot_claim_and_does_not_mutate() {
    for missing in ["path", "content", "receipt"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");
        let value = request(
            OP,
            Disposition::Create,
            if missing == "path" {
                None
            } else {
                Some("notes/new.txt")
            },
        );
        let (prepared, material, content) = plan(value);
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &prepared, &material);
        if missing != "content" {
            stage(&mut store, &content);
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
        let before = rows(&path);
        assert!(
            store
                .claim_file_create_local_authorized(&unknown, || Ok(()))
                .is_err(),
            "{missing}"
        );
        assert_eq!(phase(&store), Phase::Prepared, "{missing}");
        assert_eq!(rows(&path), before, "{missing}");
        if missing != "receipt" {
            store.integrity_check().unwrap();
        }
    }
}

#[test]
fn wrong_command_disposition_subject_and_target_are_rejected_without_claim() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("binding.db");
    let (prepared, material, content) = create_plan();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, &prepared, &material);
    stage(&mut store, &content);
    let mut variants = Vec::new();
    let mut wrong_subject = request(OP, Disposition::Create, Some("notes/new.txt"));
    wrong_subject.subject = "plugin.other".into();
    variants.push(wrong_subject);
    let mut wrong_target = request(OP, Disposition::Create, Some("notes/new.txt"));
    wrong_target.target.reference = [9; 32];
    variants.push(wrong_target);
    variants.push(request(OP, Disposition::Create, Some("notes/other.txt")));
    variants.push(request(OP, Disposition::Replace, Some("notes/new.txt")));
    for value in variants {
        let (wrong, _, _) = plan(value);
        let candidate = wrong.propose_dispatch_boundary().unwrap();
        assert!(
            store
                .claim_file_create_local_authorized(&candidate, || Ok(()))
                .is_err()
        );
        assert_eq!(phase(&store), Phase::Prepared);
        assert_eq!(rows(&path), (1, 1, 1));
    }
    let wrong_phase = prepared;
    assert!(
        store
            .claim_file_create_local_authorized(&wrong_phase, || Ok(()))
            .is_err()
    );
    assert_eq!(phase(&store), Phase::Prepared);
    store.integrity_check().unwrap();

    let other_dir = tempfile::tempdir().unwrap();
    let other_path = other_dir.path().join("replace.db");
    let (replace, replacement, bytes) =
        plan(request(OP, Disposition::Replace, Some("notes/new.txt")));
    let mut other = Store::open(&other_path, EventBudget::default()).unwrap();
    prepare(&mut other, &replace, &replacement);
    stage(&mut other, &bytes);
    assert!(
        other
            .claim_file_create_local_authorized(
                &replace.propose_dispatch_boundary().unwrap(),
                || Ok(())
            )
            .is_err()
    );
    assert_eq!(phase(&other), Phase::Prepared);
    other.integrity_check().unwrap();
}

#[test]
fn authorization_denial_precedes_protected_read_and_final_guard_rolls_back() {
    let (prepared, material, content) = create_plan();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    for denied in [1, 2] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("denied.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &prepared, &material);
        stage(&mut store, &content);
        let before = store.pending(0, 10).unwrap();
        let mut checks = 0;
        assert!(matches!(
            store.claim_file_create_local_authorized(&unknown, || {
                checks += 1;
                if checks == denied {
                    Err(Error::Invalid("create authorization denied"))
                } else {
                    Ok(())
                }
            }),
            Err(Error::Invalid("create authorization denied"))
        ));
        assert_eq!(checks, denied);
        assert_eq!(phase(&store), Phase::Prepared);
        assert_eq!(rows(&path), (1, 1, 1));
        assert_eq!(store.pending(0, 10).unwrap(), before);
        store.integrity_check().unwrap();
        store
            .claim_file_create_local_authorized(&unknown, || Ok(()))
            .unwrap();
        assert_eq!(phase(&store), Phase::OutcomeUnknown);
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("denied-before-read.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, &prepared, &material);
    stage(&mut store, &content);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute(
        "UPDATE file_mutation_content SET content_sha256=zeroblob(32) WHERE operation_id=?1",
        [OP],
    )
    .unwrap();
    assert!(matches!(
        store.claim_file_create_local_authorized(&unknown, || Err(Error::Invalid("denied first"))),
        Err(Error::Invalid("denied first"))
    ));
    assert!(
        store
            .claim_file_create_local_authorized(&unknown, || Ok(()))
            .is_err()
    );
    assert_eq!(phase(&store), Phase::Prepared);
}

#[test]
fn removing_claimed_content_receipt_or_both_cannot_erase_the_required_staging() {
    for damage in ["content", "receipt", "both", "mirror"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tampered.db");
        let (prepared, material, content) = create_plan();
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &prepared, &material);
        stage(&mut store, &content);
        store
            .claim_file_create_local_authorized(&unknown, || Ok(()))
            .unwrap();
        let sql = rusqlite::Connection::open(&path).unwrap();
        if matches!(damage, "receipt" | "both") {
            let event: String = sql
                .query_row(
                    "SELECT event_id FROM file_content_receipts WHERE operation_id=?1",
                    [OP],
                    |r| r.get(0),
                )
                .unwrap();
            sql.execute(
                "DELETE FROM file_content_receipts WHERE operation_id=?1",
                [OP],
            )
            .unwrap();
            // Remove the receipt event too. With both originals absent, only the
            // committed OutcomeUnknown invariant can identify the erased staging.
            for table in ["outbox", "operation_events", "operations"] {
                sql.execute(&format!("DELETE FROM {table} WHERE id=?1"), [&event])
                    .unwrap();
            }
        }
        if matches!(damage, "content" | "both") {
            sql.execute(
                "DELETE FROM file_mutation_content WHERE operation_id=?1",
                [OP],
            )
            .unwrap();
        }
        if damage == "mirror" {
            sql.execute(
                "UPDATE file_mutation_content SET content_sha256=zeroblob(32) WHERE operation_id=?1",
                [OP],
            )
            .unwrap();
        }
        drop(sql);
        // Verify the point lookup itself rejects erased staging, independently
        // of a whole-store audit detecting any removed event or sequence gap.
        assert!(store.lookup_io_intent(SUBJECT, OP).is_err(), "{damage}");
        assert!(
            store
                .file_mutation_content_local_authorized(SUBJECT, OP, || Ok(()))
                .is_err(),
            "{damage}"
        );
        assert!(store.integrity_check().is_err(), "{damage}");
        drop(store);
        assert!(
            Store::open_existing(&path, EventBudget::default()).is_err(),
            "{damage}"
        );
    }
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "child harness; parent supplies an isolated database and crash point"]
fn file_create_claim_crash_child() {
    let path = std::env::var_os("MORROW_FILE_CREATE_CLAIM_CRASH_DB").expect("child database path");
    let mut store = Store::open_existing(Path::new(&path), EventBudget::default()).unwrap();
    let (prepared, _, _) = create_plan();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    store
        .claim_file_create_local_authorized(&unknown, || Ok(()))
        .unwrap();
    panic!("file create claim crash boundary was not reached");
}

#[cfg(feature = "fault-injection")]
fn crash(path: &Path, child_name: &str, point: &str) {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", child_name, "--ignored", "--nocapture"])
        .env("MORROW_FILE_CREATE_CLAIM_CRASH_DB", path)
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
fn real_process_crashes_preserve_only_whole_create_claims() {
    for (point, expected) in [
        ("file-create-claim-before-commit", Phase::Prepared),
        ("file-create-claim-after-commit", Phase::OutcomeUnknown),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.db");
        let (prepared, material, content) = create_plan();
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &prepared, &material);
        stage(&mut store, &content);
        drop(store);
        crash(&path, "file_create_claim_crash_child", point);
        let mut reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
        assert_eq!(phase(&reopened), expected, "{point}");
        retained(&reopened, &content);
        assert_eq!(
            rows(&path),
            (if expected == Phase::Prepared { 1 } else { 2 }, 1, 1)
        );
        if expected == Phase::OutcomeUnknown {
            assert!(matches!(
                reopened.claim_file_create_local_authorized(&unknown, || Ok(())),
                Err(Error::RevisionConflict)
            ));
        } else {
            reopened
                .claim_file_create_local_authorized(&unknown, || Ok(()))
                .unwrap();
            assert_eq!(phase(&reopened), Phase::OutcomeUnknown);
        }
        reopened.integrity_check().unwrap();
    }
}

#[test]
fn receipt_from_either_source_must_precede_the_committed_create_claim() {
    for source in [Source::LiveStaging, Source::LegacyImport] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("late-receipt.db");
        let (prepared, material, content) = create_plan();
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &prepared, &material);
        stage(&mut store, &content);
        store
            .claim_file_create_local_authorized(&unknown, || Ok(()))
            .unwrap();
        let sql = rusqlite::Connection::open(&path).unwrap();
        let receipt = Receipt::new(&content, source).unwrap();
        let receipt_event = receipt.event_id();
        let unknown_event = unknown.event_id();
        assert_eq!(
            sql.query_row(
                "SELECT sequence FROM operation_events WHERE id=?1",
                [&receipt_event],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
            2
        );
        assert_eq!(
            sql.query_row(
                "SELECT sequence FROM operation_events WHERE id=?1",
                [&unknown_event],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
            3
        );
        // The receipt event identity is source-independent. Make the imported
        // original internally coherent so this tests its order, not its codec.
        for table in ["operations", "outbox"] {
            sql.execute(
                &format!("UPDATE {table} SET payload=?1 WHERE id=?2"),
                rusqlite::params![receipt.container(), receipt_event],
            )
            .unwrap();
        }
        for table in ["operation_events", "outbox"] {
            sql.execute(
                &format!("UPDATE {table} SET sequence=4 WHERE id=?1"),
                [&receipt_event],
            )
            .unwrap();
            sql.execute(
                &format!("UPDATE {table} SET sequence=2 WHERE id=?1"),
                [&unknown_event],
            )
            .unwrap();
            sql.execute(
                &format!("UPDATE {table} SET sequence=3 WHERE id=?1"),
                [&receipt_event],
            )
            .unwrap();
        }
        assert_eq!(
            sql.query_row(
                "SELECT payload FROM operations WHERE id=?1",
                [&receipt_event],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .unwrap(),
            receipt.container()
        );
        assert_eq!(
            sql.query_row(
                "SELECT sequence FROM operation_events WHERE id=?1",
                [&receipt_event],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
            3
        );
        assert_eq!(
            sql.query_row(
                "SELECT sequence FROM operation_events WHERE id=?1",
                [&unknown_event],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
            2
        );
        drop(sql);
        assert!(store.integrity_check().is_err(), "{source:?}");
        drop(store);
        assert!(
            Store::open_existing(&path, EventBudget::default()).is_err(),
            "{source:?}"
        );
    }
}

#[test]
fn released_prepared_response_reservation_cannot_cross_the_create_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("released.db");
    let (prepared, material, content) = create_plan();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, &prepared, &material);
    stage(&mut store, &content);
    let pending = store.pending(0, 10).unwrap();
    store.release_io_materials(SUBJECT, OP).unwrap();
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    assert!(
        store
            .claim_file_create_local_authorized(&unknown, || Ok(()))
            .is_err()
    );
    assert_eq!(phase(&store), Phase::Prepared);
    assert_eq!(store.pending(0, 10).unwrap(), pending);
    assert_eq!(rows(&path), (1, 1, 1));
    retained(&store, &content);
    store.integrity_check().unwrap();
}

#[test]
fn zero_byte_create_still_requires_and_retains_empty_content_and_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.db");
    let mut value = request(OP, Disposition::Create, Some("notes/empty.txt"));
    value.content_length = 0;
    value.content_sha256 = Some(Sha256::digest([]).into());
    let (prepared, material, content) = plan_with_bytes(value, &[]);
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, &prepared, &material);
    assert!(matches!(
        store.claim_file_create_local_authorized(&unknown, || Ok(())),
        Err(Error::EvidenceUnavailable)
    ));
    assert_eq!(phase(&store), Phase::Prepared);
    stage(&mut store, &content);
    store
        .claim_file_create_local_authorized(&unknown, || Ok(()))
        .unwrap();
    assert_eq!(phase(&store), Phase::OutcomeUnknown);
    assert_eq!(rows(&path), (2, 1, 1));
    let read = store
        .file_mutation_content_local_authorized(SUBJECT, OP, || Ok(()))
        .unwrap()
        .unwrap();
    assert!(read.content().is_empty());
    assert_eq!(read.container(), content.container());
    let receipt = store
        .file_mutation_content_receipt_local_authorized(SUBJECT, OP, || Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(receipt.content_length(), 0);
    assert_eq!(receipt.content_sha256(), content.content_sha256());
    store.integrity_check().unwrap();
    drop(store);
    let reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert_eq!(phase(&reopened), Phase::OutcomeUnknown);
    assert!(
        reopened
            .file_mutation_content_local_authorized(SUBJECT, OP, || Ok(()))
            .unwrap()
            .unwrap()
            .content()
            .is_empty()
    );
    reopened.integrity_check().unwrap();
}

fn create_response(prepared: &Record, result: CreateResult) -> Material {
    let outcome = CreateOutcome::new(
        OP,
        SUBJECT,
        prepared.command().request_sha256,
        [3; 32],
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

fn observed(unknown: &Record, response: &Material) -> Record {
    unknown
        .propose_observation(response.digest(), ObservationSource::OriginalResponse)
        .unwrap()
}

#[test]
fn create_observation_is_atomic_and_survives_reopen_seal_and_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("observed.db");
    let snapshot = dir.path().join("snapshot.db");
    let key = SigningKey::from_bytes(&[92; 32]);
    let trust = TrustedLog {
        id: "file-create-observe-log".into(),
        key: key.verifying_key(),
    };
    let (prepared, material, content) = create_plan();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let response = create_response(&prepared, CreateResult::OsRejected { code: 5 });
    let candidate = observed(&unknown, &response);
    let mut store =
        Store::open_audited(&path, EventBudget::default(), true, trust.clone()).unwrap();
    prepare(&mut store, &prepared, &material);
    stage(&mut store, &content);
    store
        .claim_file_create_local_authorized(&unknown, || Ok(()))
        .unwrap();
    let mut checks = 0;
    let stored = store
        .observe_file_create_local_authorized(&candidate, &response, || {
            checks += 1;
            Ok(())
        })
        .unwrap();
    assert!(checks >= 4);
    assert_eq!(stored.container(), candidate.container());
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
        response.container()
    );
    retained(&store, &content);
    assert!(matches!(
        store.observe_file_create_local_authorized(&candidate, &response, || Ok(())),
        Err(Error::RevisionConflict)
    ));
    let pending = store.pending(0, 10).unwrap();
    assert_eq!(pending.len(), 4);
    store.integrity_check().unwrap();
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
        let reader = Store::open_read_only_audited(source, trust.clone()).unwrap();
        assert_eq!(phase(&reader), Phase::Observed);
        assert_eq!(
            reader
                .io_material(SUBJECT, OP, Kind::Response)
                .unwrap()
                .unwrap()
                .container(),
            response.container()
        );
        retained(&reader, &content);
        assert_eq!(reader.sealed_segment(1).unwrap().unwrap(), signed);
        reader.integrity_check().unwrap();
    }
}

#[test]
fn forged_create_outcomes_and_wrong_observation_leave_unknown_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("forged.db");
    let (prepared, material, content) = create_plan();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, &prepared, &material);
    stage(&mut store, &content);
    store
        .claim_file_create_local_authorized(&unknown, || Ok(()))
        .unwrap();
    let reservation = store.io_material_reservation_usage().unwrap();
    for (operation, subject, request, target, digest, length) in [
        (
            "other-op",
            SUBJECT,
            prepared.command().request_sha256,
            [3; 32],
            content.content_sha256(),
            BYTES.len() as u64,
        ),
        (
            OP,
            "plugin.other",
            prepared.command().request_sha256,
            [3; 32],
            content.content_sha256(),
            BYTES.len() as u64,
        ),
        (
            OP,
            SUBJECT,
            [9; 32],
            [3; 32],
            content.content_sha256(),
            BYTES.len() as u64,
        ),
        (
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            [9; 32],
            content.content_sha256(),
            BYTES.len() as u64,
        ),
        (
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            [3; 32],
            [9; 32],
            BYTES.len() as u64,
        ),
        (
            OP,
            SUBJECT,
            prepared.command().request_sha256,
            [3; 32],
            content.content_sha256(),
            BYTES.len() as u64 + 1,
        ),
    ] {
        let bad = CreateOutcome::new(
            operation,
            subject,
            request,
            target,
            digest,
            length,
            CreateResult::Created,
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
                .observe_file_create_local_authorized(&candidate, &response, || Ok(()))
                .is_err()
        );
        assert_eq!(phase(&store), Phase::OutcomeUnknown);
        assert_eq!(store.io_material_reservation_usage().unwrap(), reservation);
        assert!(matches!(
            store.io_material(SUBJECT, OP, Kind::Response),
            Err(Error::EvidenceUnavailable)
        ));
    }
    let response = create_response(&prepared, CreateResult::Created);
    let wrong_digest = unknown
        .propose_observation([7; 32], ObservationSource::OriginalResponse)
        .unwrap();
    assert!(
        store
            .observe_file_create_local_authorized(&wrong_digest, &response, || Ok(()))
            .is_err()
    );
    assert_eq!(phase(&store), Phase::OutcomeUnknown);
    store.integrity_check().unwrap();

    let replace_dir = tempfile::tempdir().unwrap();
    let replace_path = replace_dir.path().join("replace.db");
    let (replace, request_material, replacement) =
        plan(request(OP, Disposition::Replace, Some("notes/new.txt")));
    let mut other = Store::open(&replace_path, EventBudget::default()).unwrap();
    prepare(&mut other, &replace, &request_material);
    stage(&mut other, &replacement);
    assert!(
        other
            .observe_file_create_local_authorized(
                &replace.propose_dispatch_boundary().unwrap(),
                &response,
                || Ok(())
            )
            .is_err()
    );
    assert_eq!(phase(&other), Phase::Prepared);
}

#[test]
fn observation_authorization_denial_rolls_back_evidence_and_intent() {
    let (prepared, material, content) = create_plan();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let response = create_response(&prepared, CreateResult::Created);
    let candidate = observed(&unknown, &response);
    for denied in [1, 4] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("denied-observe.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &prepared, &material);
        stage(&mut store, &content);
        store
            .claim_file_create_local_authorized(&unknown, || Ok(()))
            .unwrap();
        let before = store.pending(0, 10).unwrap();
        let reservation = store.io_material_reservation_usage().unwrap();
        let mut checks = 0;
        assert!(matches!(
            store.observe_file_create_local_authorized(&candidate, &response, || {
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
        assert_eq!(store.pending(0, 10).unwrap(), before);
        assert_eq!(store.io_material_reservation_usage().unwrap(), reservation);
        assert!(matches!(
            store.io_material(SUBJECT, OP, Kind::Response),
            Err(Error::EvidenceUnavailable)
        ));
        store.integrity_check().unwrap();
        store
            .observe_file_create_local_authorized(&candidate, &response, || Ok(()))
            .unwrap();
        assert_eq!(phase(&store), Phase::Observed);
    }
}

#[test]
fn observation_rejects_missing_claimed_content_or_receipt_without_new_history() {
    for missing in ["content", "receipt"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing-observe.db");
        let (prepared, material, content) = create_plan();
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let response = create_response(&prepared, CreateResult::Created);
        let candidate = observed(&unknown, &response);
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &prepared, &material);
        stage(&mut store, &content);
        store
            .claim_file_create_local_authorized(&unknown, || Ok(()))
            .unwrap();
        let sql = rusqlite::Connection::open(&path).unwrap();
        let table = if missing == "content" {
            "file_mutation_content"
        } else {
            "file_content_receipts"
        };
        sql.execute(&format!("DELETE FROM {table} WHERE operation_id=?1"), [OP])
            .unwrap();
        drop(sql);
        assert!(
            store
                .observe_file_create_local_authorized(&candidate, &response, || Ok(()))
                .is_err(),
            "{missing}"
        );
        // A corrupt claimed history is withheld by lookup; inspect only SQL row count here.
        assert_eq!(rows(&path).0, 2);
        assert!(matches!(
            store.io_material(SUBJECT, OP, Kind::Response),
            Err(Error::Integrity) | Err(Error::EvidenceUnavailable)
        ));
    }
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "child harness; parent supplies an isolated database and crash point"]
fn file_create_observe_crash_child() {
    let path = std::env::var_os("MORROW_FILE_CREATE_CLAIM_CRASH_DB").expect("child database path");
    let mut store = Store::open_existing(Path::new(&path), EventBudget::default()).unwrap();
    let (prepared, _, _) = create_plan();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let response = create_response(&prepared, CreateResult::Created);
    let candidate = observed(&unknown, &response);
    store
        .observe_file_create_local_authorized(&candidate, &response, || Ok(()))
        .unwrap();
    panic!("file create observation crash boundary was not reached");
}

#[cfg(feature = "fault-injection")]
#[test]
fn real_process_crashes_preserve_only_whole_create_observations() {
    for (point, expected) in [
        ("file-create-observe-before-commit", Phase::OutcomeUnknown),
        ("file-create-observe-after-commit", Phase::Observed),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("observe-crash.db");
        let (prepared, material, content) = create_plan();
        let unknown = prepared.propose_dispatch_boundary().unwrap();
        let response = create_response(&prepared, CreateResult::Created);
        let candidate = observed(&unknown, &response);
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &prepared, &material);
        stage(&mut store, &content);
        store
            .claim_file_create_local_authorized(&unknown, || Ok(()))
            .unwrap();
        drop(store);
        crash(&path, "file_create_observe_crash_child", point);
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
                reopened.observe_file_create_local_authorized(&candidate, &response, || Ok(())),
                Err(Error::RevisionConflict)
            ));
        } else {
            assert!(matches!(
                reopened.io_material(SUBJECT, OP, Kind::Response),
                Err(Error::EvidenceUnavailable)
            ));
            reopened
                .observe_file_create_local_authorized(&candidate, &response, || Ok(()))
                .unwrap();
            assert_eq!(phase(&reopened), Phase::Observed);
        }
        reopened.integrity_check().unwrap();
    }
}
