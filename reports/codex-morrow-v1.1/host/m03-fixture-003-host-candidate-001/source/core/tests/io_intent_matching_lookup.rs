//! Match small immutable IO intent metadata before verifying a large file body.
#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    file_content::FileContent,
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
    io_evidence::{Kind, Material},
    io_intent::{Command, Phase, Record},
    store::{EventBudget, Store},
};
use sha2::{Digest, Sha256};

const OP: &str = "matching-create";
const SUBJECT: &str = "plugin.matching-lookup";

fn bytes() -> Vec<u8> {
    let mut state = 17u32;
    (0..1024 * 1024)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}
fn request(content: &[u8], target: [u8; 32]) -> RequestRecord {
    RequestRecord::new(MutationRequest {
        operation_id: OP.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: target,
            relative_path: Some(RelativeFilePath::parse("nested/leaf.bin").unwrap()),
        },
        disposition: Disposition::Create,
        expected_identity: None,
        content_length: content.len() as u64,
        content_sha256: Some(Sha256::digest(content).into()),
    })
    .unwrap()
}
fn plan(original: &RequestRecord) -> (Record, Material) {
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
    (prepared, material)
}
fn row_count(path: &std::path::Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("SELECT count(*) FROM io_intents", [], |row| row.get(0))
        .unwrap()
}
fn conflicting_command(original: &Command, variant: &str) -> Command {
    let mut changed = original.clone();
    match variant {
        "request" => changed.request_sha256 = [9; 32],
        "response" => changed.response_limit += 1,
        _ => unreachable!(),
    }
    changed
}

#[test]
fn exact_command_lookup_keeps_full_integrity_while_conflicts_stop_before_large_body() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("matching.db");
    let content = bytes();
    let original = request(&content, [3; 32]);
    let (prepared, material) = plan(&original);
    let command = prepared.command().clone();
    let body = FileContent::new(OP, SUBJECT, command.request_sha256, &content).unwrap();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store
        .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    store
        .stage_file_mutation_content_local_authorized(&body, || Ok(()))
        .unwrap();
    store
        .claim_file_create_local_authorized(&unknown, || Ok(()))
        .unwrap();

    assert_eq!(
        store
            .lookup_matching_io_intent(&command)
            .unwrap()
            .unwrap()
            .phase(),
        Phase::OutcomeUnknown
    );
    let mut other_subject = command.clone();
    other_subject.subject = "plugin.other-subject".into();
    assert!(
        store
            .lookup_matching_io_intent(&other_subject)
            .unwrap()
            .is_none()
    );
    let mut missing = command.clone();
    missing.operation_id = "absent-operation".into();
    assert!(store.lookup_matching_io_intent(&missing).unwrap().is_none());

    // Damage the bound mirror while preserving the 1 MiB encoded body. Full
    // history verification now fails, but mismatching metadata must fail first.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE file_mutation_content SET content_sha256=zeroblob(32) WHERE operation_id=?1",
            [OP],
        )
        .unwrap();
    for variant in ["request", "response"] {
        let wrong = conflicting_command(&command, variant);
        assert!(
            matches!(
                store.lookup_matching_io_intent(&wrong),
                Err(Error::OperationConflict)
            ),
            "{variant}"
        );
    }
    assert!(matches!(
        store.lookup_matching_io_intent(&command),
        Err(Error::Integrity)
    ));
    assert!(matches!(
        store.lookup_io_intent(SUBJECT, OP),
        Err(Error::Integrity)
    ));
}

#[test]
fn preparation_rejects_conflicting_plan_before_damaged_original_body() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("matching-prepare.db");
    let content = bytes();
    let original = request(&content, [3; 32]);
    let (prepared, material) = plan(&original);
    let body = FileContent::new(OP, SUBJECT, prepared.command().request_sha256, &content).unwrap();
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store
        .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    store
        .stage_file_mutation_content_local_authorized(&body, || Ok(()))
        .unwrap();
    store
        .claim_file_create_local_authorized(&unknown, || Ok(()))
        .unwrap();
    assert_eq!(row_count(&path), 2);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE file_mutation_content SET content_sha256=zeroblob(32) WHERE operation_id=?1",
            [OP],
        )
        .unwrap();

    let other = request(&content, [4; 32]);
    let (wrong_prepared, wrong_material) = plan(&other);
    assert!(matches!(
        store.prepare_file_mutation_local_authorized(&wrong_prepared, &wrong_material, || Ok(())),
        Err(Error::OperationConflict)
    ));
    assert!(matches!(
        store.prepare_file_mutation_local_authorized(&prepared, &material, || Ok(())),
        Err(Error::Integrity)
    ));
    assert_eq!(row_count(&path), 2);
}
