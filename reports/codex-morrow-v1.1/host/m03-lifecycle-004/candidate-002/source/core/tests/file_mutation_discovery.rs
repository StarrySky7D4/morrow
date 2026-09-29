//! Original-plan discovery is bounded metadata retrieval, never an effect permit.
#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    io_evidence::{Kind, Material},
    io_intent::Record,
    store::{EventBudget, Store},
};
use rusqlite::{Connection, params};
const SUBJECT: &str = "discovery.subject";
fn prepare(
    store: &mut Store,
    id: &str,
    subject: &str,
    package: [u8; 32],
    disposition: Disposition,
) -> RequestRecord {
    let plan = RequestRecord::new(MutationRequest {
        operation_id: id.into(),
        subject: subject.into(),
        package_sha256: package,
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: None,
        },
        disposition,
        expected_identity: Some([4; 32]),
        content_length: if disposition == Disposition::Delete {
            0
        } else {
            1
        },
        content_sha256: if disposition == Disposition::Delete {
            None
        } else {
            Some([5; 32])
        },
    })
    .unwrap();
    let record = Record::prepared(plan.command().unwrap()).unwrap();
    let material = Material::encode(
        Kind::Request,
        id,
        subject,
        record.command().request_sha256,
        plan.container(),
    )
    .unwrap();
    store
        .prepare_file_mutation_local_authorized(&record, &material, || Ok(()))
        .unwrap();
    plan
}
#[test]
fn reopened_and_snapshot_libraries_discover_exact_originals_with_bounded_sparse_pages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let backup = dir.path().join("backup");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(
        &mut store,
        "a-foreign",
        "another.subject",
        [1; 32],
        Disposition::Delete,
    );
    prepare(
        &mut store,
        "b-package",
        SUBJECT,
        [9; 32],
        Disposition::Delete,
    );
    prepare(
        &mut store,
        "c-replace",
        SUBJECT,
        [1; 32],
        Disposition::Replace,
    );
    let first = prepare(&mut store, "d-match", SUBJECT, [1; 32], Disposition::Delete);
    let second = prepare(&mut store, "e-match", SUBJECT, [1; 32], Disposition::Delete);
    let before = store.pending_usage().unwrap();
    store.snapshot_to(&backup, 16 * 1024 * 1024).unwrap();
    drop(store);
    for path in [&path, &backup] {
        let store = Store::open_existing(path, EventBudget::default()).unwrap();
        let mut cursor = store
            .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
            .unwrap();
        let empty = store
            .read_file_mutation_plan_page(&mut cursor, 2, || Ok(()))
            .unwrap();
        assert_eq!(empty.scanned, 2);
        assert!(empty.plans.is_empty());
        assert!(!empty.done);
        let next = store
            .read_file_mutation_plan_page(&mut cursor, 2, || Ok(()))
            .unwrap();
        assert_eq!(next.scanned, 2);
        assert_eq!(next.plans.len(), 1);
        assert!(!next.done);
        assert_eq!(next.plans[0].container(), first.container());
        let last = store
            .read_file_mutation_plan_page(&mut cursor, 2, || Ok(()))
            .unwrap();
        assert_eq!(last.scanned, 1);
        assert!(last.done);
        assert_eq!(last.plans[0].container(), second.container());
        let eof = store
            .read_file_mutation_plan_page(&mut cursor, 2, || Ok(()))
            .unwrap();
        assert!(eof.done);
        assert_eq!(eof.scanned, 0);
        assert!(eof.plans.is_empty());
        assert_eq!(store.pending_usage().unwrap(), before);
        store.integrity_check().unwrap();
    }
}

#[test]
fn checkpoint_resumes_after_sparse_page_only_in_original_store_and_scope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(
        &mut store,
        "a-foreign",
        "another.subject",
        [1; 32],
        Disposition::Delete,
    );
    prepare(
        &mut store,
        "b-foreign",
        SUBJECT,
        [9; 32],
        Disposition::Delete,
    );
    let wanted = prepare(&mut store, "c-match", SUBJECT, [1; 32], Disposition::Delete);

    let mut cursor = store
        .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    let sparse = store
        .read_file_mutation_plan_page(&mut cursor, 1, || Ok(()))
        .unwrap();
    assert_eq!(sparse.scanned, 1);
    assert!(sparse.plans.is_empty());
    assert!(!sparse.done);
    let first_checkpoint = sparse.checkpoint.unwrap();
    assert!(first_checkpoint.retained_bytes() <= 1024);
    drop(cursor);

    let foreign = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert!(
        foreign
            .resume_file_mutation_plan_cursor(
                &first_checkpoint,
                SUBJECT,
                [1; 32],
                Disposition::Delete
            )
            .is_err()
    );
    for (subject, package, disposition) in [
        ("other.subject", [1; 32], Disposition::Delete),
        (SUBJECT, [9; 32], Disposition::Delete),
        (SUBJECT, [1; 32], Disposition::Replace),
    ] {
        assert!(
            store
                .resume_file_mutation_plan_cursor(&first_checkpoint, subject, package, disposition)
                .is_err()
        );
    }

    let mut resumed = store
        .resume_file_mutation_plan_cursor(&first_checkpoint, SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    let second_sparse = store
        .read_file_mutation_plan_page(&mut resumed, 1, || Ok(()))
        .unwrap();
    assert!(second_sparse.plans.is_empty());
    assert!(!second_sparse.done);
    let second_checkpoint = second_sparse.checkpoint.unwrap();

    // A failed later page poisons only its cursor; the last delivered token
    // still starts after the second sparse key in the same Store.
    assert!(
        store
            .read_file_mutation_plan_page(&mut resumed, 1, || Err(Error::Invalid(
                "budget exhausted"
            )))
            .is_err()
    );
    assert!(
        store
            .read_file_mutation_plan_page(&mut resumed, 1, || Ok(()))
            .is_err()
    );
    let mut resumed = store
        .resume_file_mutation_plan_cursor(&second_checkpoint, SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    let final_page = store
        .read_file_mutation_plan_page(&mut resumed, 1, || Ok(()))
        .unwrap();
    assert_eq!(final_page.scanned, 1);
    assert_eq!(final_page.plans[0].container(), wanted.container());
    assert!(final_page.done);
    assert!(final_page.checkpoint.is_none());
}
#[test]
fn foreign_store_and_denied_pages_never_advance_or_reuse_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, "match", SUBJECT, [1; 32], Disposition::Delete);
    let foreign = Store::open(&dir.path().join("foreign"), EventBudget::default()).unwrap();
    let mut cursor = store
        .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    let mut checked = false;
    assert!(
        foreign
            .read_file_mutation_plan_page(&mut cursor, 1, || {
                checked = true;
                Ok(())
            })
            .is_err()
    );
    assert!(!checked);
    let mut cursor = store
        .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    let mut checks = 0;
    assert!(
        store
            .read_file_mutation_plan_page(&mut cursor, 1, || {
                checks += 1;
                if checks >= 2 {
                    Err(Error::Invalid("revoked"))
                } else {
                    Ok(())
                }
            })
            .is_err()
    );
    assert!(
        store
            .read_file_mutation_plan_page(&mut cursor, 1, || Ok(()))
            .is_err()
    );
    let mut cursor = store
        .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    let original = store
        .read_file_mutation_plan_page(&mut cursor, 1, || Ok(()))
        .unwrap();
    assert_eq!(original.plans.len(), 1);
    assert!(
        store
            .open_file_mutation_plan_cursor("bad/id", [1; 32], Disposition::Delete)
            .is_err()
    );
    assert!(
        store
            .open_file_mutation_plan_cursor(SUBJECT, [0; 32], Disposition::Delete)
            .is_err()
    );
    for limit in [0, 9, u16::MAX] {
        let mut cursor = store
            .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
            .unwrap();
        assert!(
            store
                .read_file_mutation_plan_page(&mut cursor, limit, || Ok(()))
                .is_err()
        );
    }
}
#[test]
fn foreign_originals_are_not_decoded_and_matching_corruption_poisons_discovery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(
        &mut store,
        "a-foreign",
        "another.subject",
        [1; 32],
        Disposition::Delete,
    );
    prepare(
        &mut store,
        "b-package",
        SUBJECT,
        [9; 32],
        Disposition::Delete,
    );
    let wanted = prepare(&mut store, "c-match", SUBJECT, [1; 32], Disposition::Delete);
    let connection = Connection::open(&path).unwrap();
    connection.execute("UPDATE io_evidence SET container=zeroblob(200000) WHERE operation_id IN ('a-foreign','b-package')",[]).unwrap();
    let mut cursor = store
        .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    let page = store
        .read_file_mutation_plan_page(&mut cursor, 8, || Ok(()))
        .unwrap();
    assert!(page.done);
    assert_eq!(page.scanned, 3);
    assert_eq!(page.plans[0].container(), wanted.container());
    connection
        .execute(
            "UPDATE io_evidence SET container=zeroblob(200000) WHERE operation_id='c-match'",
            [],
        )
        .unwrap();
    let mut cursor = store
        .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    assert!(matches!(
        store.read_file_mutation_plan_page(&mut cursor, 8, || Ok(())),
        Err(Error::Limit)
    ));
    assert!(
        store
            .read_file_mutation_plan_page(&mut cursor, 8, || Ok(()))
            .is_err()
    );
}
#[test]
fn compressed_material_expansion_is_bounded_before_decoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, "match", SUBJECT, [1; 32], Disposition::Delete);
    let connection = Connection::open(&path).unwrap();
    let mut bytes: Vec<u8> = connection
        .query_row(
            "SELECT container FROM io_evidence WHERE operation_id='match' AND kind=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    bytes[10..14].copy_from_slice(&(16u32 * 1024 * 1024).to_le_bytes());
    connection
        .execute(
            "UPDATE io_evidence SET container=?1 WHERE operation_id='match' AND kind=1",
            params![bytes],
        )
        .unwrap();
    let mut cursor = store
        .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    assert!(matches!(
        store.read_file_mutation_plan_page(&mut cursor, 1, || Ok(())),
        Err(Error::Limit)
    ));
}
#[test]
fn authorization_runs_before_sql_and_after_read_before_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, "match", SUBJECT, [1; 32], Disposition::Delete);
    let mut cursor = store
        .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    let mut calls = 0;
    let page = store
        .read_file_mutation_plan_page(&mut cursor, 1, || {
            calls += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(page.plans.len(), 1);
    assert!(calls >= 3);
    for deny_at in 1..=calls {
        let mut cursor = store
            .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
            .unwrap();
        let mut n = 0;
        assert!(
            store
                .read_file_mutation_plan_page(&mut cursor, 1, || {
                    n += 1;
                    if n == deny_at {
                        Err(Error::Invalid("denied"))
                    } else {
                        Ok(())
                    }
                })
                .is_err()
        );
        assert!(
            store
                .read_file_mutation_plan_page(&mut cursor, 1, || Ok(()))
                .is_err()
        );
    }
    let connection = Connection::open(&path).unwrap();
    connection.execute("DROP TABLE io_intents", []).unwrap();
    let mut cursor = store
        .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Delete)
        .unwrap();
    assert!(matches!(
        store.read_file_mutation_plan_page(&mut cursor, 1, || Err(Error::Invalid("denied"))),
        Err(Error::Invalid("denied"))
    ));
}

#[test]
fn discovery_original_is_not_a_full_content_or_outcome_integrity_claim() {
    use morrow_core::file_content::FileContent;
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let bytes = vec![42; 1024 * 1024];
    let request = RequestRecord::new(MutationRequest {
        operation_id: "content-plan".into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: None,
        },
        disposition: Disposition::Replace,
        expected_identity: Some([4; 32]),
        content_length: bytes.len() as u64,
        content_sha256: Some(Sha256::digest(&bytes).into()),
    })
    .unwrap();
    let prepared = Record::prepared(request.command().unwrap()).unwrap();
    let material = Material::encode(
        Kind::Request,
        "content-plan",
        SUBJECT,
        prepared.command().request_sha256,
        request.container(),
    )
    .unwrap();
    store
        .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    let content = FileContent::new(
        "content-plan",
        SUBJECT,
        prepared.command().request_sha256,
        &bytes,
    )
    .unwrap();
    store
        .stage_file_mutation_content_local_authorized(&content, || Ok(()))
        .unwrap();
    store
        .claim_file_replace_local_authorized(&prepared.propose_dispatch_boundary().unwrap(), || {
            Ok(())
        })
        .unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.execute("UPDATE file_mutation_content SET container=zeroblob(1) WHERE operation_id='content-plan'",[]).unwrap();
    let mut cursor = store
        .open_file_mutation_plan_cursor(SUBJECT, [1; 32], Disposition::Replace)
        .unwrap();
    let page = store
        .read_file_mutation_plan_page(&mut cursor, 1, || Ok(()))
        .unwrap();
    assert!(page.done);
    assert_eq!(page.plans[0].container(), request.container());
    // Finding the exact original never allows it to bypass full reconciliation.
    assert!(
        store
            .lookup_matching_io_intent(&request.command().unwrap())
            .is_err()
    );
}
