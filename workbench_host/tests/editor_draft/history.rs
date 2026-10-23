use super::*;
use prost::Message;
use sha2::{Digest, Sha256};

fn hash(record: &morrow_workbench_host::editor_draft::EditorDraftRecord) -> Vec<u8> {
    Sha256::digest(record.slot.request.as_ref().unwrap().encode_to_vec()).to_vec()
}

fn pending_import(
    host: &mut Workbench,
    draft: &str,
    operation: &str,
) -> morrow_workbench_host::editor_draft_staging::proto::ImportRequest {
    let value = morrow_workbench_host::editor_draft_staging::proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: draft.into(),
        operation_id: operation.into(),
        expected_generation: 1,
        name: "original.bin".into(),
        kind: "file".into(),
        byte_length: FILE_BYTES.len() as u64,
        sha256: Sha256::digest(FILE_BYTES).to_vec(),
    };
    host.begin_editor_draft_import(&value).unwrap();
    value
}

#[test]
fn archive_blocks_unresolved_import_decision_and_preserves_its_explicit_cancellation() {
    use morrow_workbench_host::editor_draft_import_decision::DecisionStatus;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workbench.db");
    let mut host = seed(&path);
    let saved = host
        .save_editor_draft(&request("aux-main", 0, "raw"))
        .unwrap();
    let imported = pending_import(&mut host, DRAFT, "aux-import");
    let pending = host
        .prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &imported.operation_id,
            "aux-decision",
        )
        .unwrap();
    assert_eq!(pending.status, DecisionStatus::Pending);
    host.discard_editor_draft(CARD, DRAFT, 1, "aux-retirement")
        .unwrap();
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    assert!(
        host.archive_editor_draft(CARD, DRAFT, 2, &hash(&saved), "aux-archive")
            .is_err()
    );
    assert!(
        host.inspect_editor_draft_archive(CARD, DRAFT)
            .unwrap()
            .is_none()
    );
    let cancelled = host
        .cancel_editor_draft_import_decision(CARD, DRAFT, 1, &imported.operation_id, "aux-decision")
        .unwrap();
    assert_eq!(cancelled.status, DecisionStatus::Cancelled);
    let marker = host
        .archive_editor_draft(CARD, DRAFT, 2, &hash(&saved), "aux-archive")
        .unwrap();
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(
        host.inspect_editor_draft_archive(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot_sha256,
        marker.slot_sha256
    );
    assert_eq!(
        host.prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &imported.operation_id,
            "aux-decision"
        )
        .unwrap()
        .status,
        DecisionStatus::Cancelled
    );
    assert!(
        host.prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &imported.operation_id,
            "late-new-decision"
        )
        .unwrap_err()
        .to_string()
        .contains("archived draft")
    );
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn actual_256_import_journals_require_bound_archive_before_new_import_admission() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workbench.db");
    let mut host = seed(&path);
    let mut first = None;
    for index in 0..256 {
        let mut value = request(&format!("import-cap-main-{index}"), 0, "retained");
        value.draft_id = format!("import-cap-{index}");
        let saved = host.save_editor_draft(&value).unwrap();
        let imported = pending_import(
            &mut host,
            &value.draft_id,
            &format!("import-cap-operation-{index}"),
        );
        host.discard_editor_draft(
            CARD,
            &value.draft_id,
            1,
            &format!("import-cap-retire-{index}"),
        )
        .unwrap();
        host.reconcile_editor_draft_imports(CARD, &value.draft_id)
            .unwrap();
        if index == 0 {
            first = Some((value.draft_id, hash(&saved), imported));
        }
    }
    let (draft, digest, old) = first.unwrap();
    let mut next = request("import-cap-new-main", 0, "new");
    next.draft_id = "import-cap-new".into();
    assert!(
        host.save_editor_draft(&next)
            .unwrap_err()
            .to_string()
            .contains("capacity")
    );
    // Main history must be maintained as well as its independently bounded
    // import journal. One exact archive binds and accounts for both records.
    host.archive_editor_draft(CARD, &draft, 2, &digest, "import-cap-archive")
        .unwrap();
    host.save_editor_draft(&next).unwrap();
    pending_import(&mut host, &next.draft_id, "import-cap-new-operation");
    let old_result = host.begin_editor_draft_import(&old).unwrap();
    assert_eq!(
        old_result.phase,
        morrow_workbench_host::editor_draft_staging::DraftImportPhase::Retired
    );
    assert!(!old_result.bytes_retained);
    drop(host);
    let host = Workbench::open(&path, None).unwrap();
    assert!(
        host.inspect_editor_draft_archive(CARD, &draft)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        host.list_editor_draft_imports(CARD, &next.draft_id)
            .unwrap()
            .len(),
        1
    );
    assert!(
        host.list_editor_draft_imports(CARD, &draft)
            .unwrap()
            .is_empty()
    );
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn actual_256_cancelled_decisions_remain_auditable_after_explicit_archive_releases_admission() {
    use morrow_workbench_host::editor_draft_import_decision::DecisionStatus;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workbench.db");
    let mut host = seed(&path);
    let saved = host
        .save_editor_draft(&request("decision-cap-main", 0, "raw"))
        .unwrap();
    let imported = pending_import(&mut host, DRAFT, "decision-cap-import");
    for index in 0..256 {
        let operation = format!("decision-cap-{index}");
        assert_eq!(
            host.prepare_editor_draft_import_decision(
                CARD,
                DRAFT,
                1,
                &imported.operation_id,
                &operation
            )
            .unwrap()
            .status,
            DecisionStatus::Pending
        );
        assert_eq!(
            host.cancel_editor_draft_import_decision(
                CARD,
                DRAFT,
                1,
                &imported.operation_id,
                &operation
            )
            .unwrap()
            .status,
            DecisionStatus::Cancelled
        );
    }
    assert!(
        host.prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &imported.operation_id,
            "decision-cap-overflow"
        )
        .unwrap_err()
        .to_string()
        .contains("capacity")
    );
    host.discard_editor_draft(CARD, DRAFT, 1, "decision-cap-retire")
        .unwrap();
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    host.archive_editor_draft(CARD, DRAFT, 2, &hash(&saved), "decision-cap-archive")
        .unwrap();
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let mut next = request("decision-cap-new-main", 0, "new");
    next.draft_id = "decision-cap-new".into();
    host.save_editor_draft(&next).unwrap();
    let fresh = pending_import(&mut host, &next.draft_id, "decision-cap-fresh-import");
    assert_eq!(
        host.prepare_editor_draft_import_decision(
            CARD,
            &next.draft_id,
            1,
            &fresh.operation_id,
            "decision-cap-fresh"
        )
        .unwrap()
        .status,
        DecisionStatus::Pending
    );
    for index in 0..256 {
        let original = host
            .inspect_editor_draft_import_decision(
                CARD,
                DRAFT,
                1,
                &imported.operation_id,
                &format!("decision-cap-{index}"),
            )
            .unwrap()
            .unwrap();
        assert_eq!(original.status, DecisionStatus::Cancelled);
    }
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn retired_archive_survives_restart_without_reusing_or_mutating_original_journal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let original = request("history-save", 0, "");
    let saved = host.save_editor_draft(&original).unwrap();
    let digest = hash(&saved);
    assert!(
        host.archive_editor_draft(CARD, DRAFT, 1, &digest, "archive-active")
            .is_err()
    );
    let retired = host
        .discard_editor_draft(CARD, DRAFT, 1, "history-retire")
        .unwrap();
    let before = retired.slot.encode_to_vec();
    assert!(
        host.archive_editor_draft(CARD, DRAFT, 1, &digest, "archive-stale")
            .is_err()
    );
    assert!(
        host.archive_editor_draft(CARD, DRAFT, 2, &[7; 32], "archive-wrong-hash")
            .is_err()
    );
    let archive = host
        .archive_editor_draft(CARD, DRAFT, 2, &digest, "history-archive")
        .unwrap();
    assert!(!archive.repeated);
    assert_eq!(archive.slot_sha256, Sha256::digest(&before).to_vec());
    drop(host); // Lose the delivery, then reconcile in an independent owner.

    let mut host = Workbench::open(&path, None).unwrap();
    let receipt = host
        .inspect_editor_draft_archive(CARD, DRAFT)
        .unwrap()
        .unwrap();
    assert_eq!(receipt.operation, "history-archive");
    assert_eq!(receipt.request_sha256, digest);
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .encode_to_vec(),
        before
    );
    let repeated = host
        .archive_editor_draft(CARD, DRAFT, 2, &digest, "history-archive")
        .unwrap();
    assert_eq!(repeated, receipt);
    assert!(
        host.archive_editor_draft(CARD, DRAFT, 2, &digest, "replacement-archive")
            .is_err()
    );
    let old = host.save_editor_draft(&original).unwrap();
    assert!(old.repeated && !old.current_active);
    let mut reuse = original.clone();
    reuse.operation_id = "reuse-retired-identity".into();
    reuse.expected_generation = 2;
    assert!(host.save_editor_draft(&reuse).is_err());
    assert_eq!(business_revision(&host), 2);
    let history = host.list_editor_draft_history("", 1).unwrap();
    assert_eq!(
        (history.unarchived, history.archived, history.rows.len()),
        (0, 1, 1)
    );
    assert_eq!(history.rows[0].archive_operation, "history-archive");
    assert!(history.next_cursor.is_empty());
    assert!(host.list_editor_draft_history("not-a-cursor", 1).is_err());
    assert!(host.list_editor_draft_history("", 0).is_err());
    assert!(host.list_editor_draft_history("", 33).is_err());
}

#[test]
fn actual_256_journal_capacity_requires_explicit_archive_and_counts_it_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let mut first = None;
    for index in 0..256 {
        let mut value = request(&format!("capacity-save-{index}"), 0, "history");
        value.draft_id = format!("history-{index:03}");
        let record = host.save_editor_draft(&value).unwrap();
        host.discard_editor_draft(
            CARD,
            &value.draft_id,
            1,
            &format!("capacity-retire-{index}"),
        )
        .unwrap();
        if index == 0 {
            first = Some((value, hash(&record)));
        }
    }
    let mut next = request("capacity-next", 0, "new");
    next.draft_id = "history-next".into();
    assert!(
        host.save_editor_draft(&next)
            .unwrap_err()
            .to_string()
            .contains("capacity")
    );
    let (first, digest) = first.unwrap();
    host.archive_editor_draft(CARD, &first.draft_id, 2, &digest, "capacity-archive")
        .unwrap();
    host.save_editor_draft(&next).unwrap();
    host.archive_editor_draft(CARD, &first.draft_id, 2, &digest, "capacity-archive")
        .unwrap();
    let mut overflow = next.clone();
    overflow.draft_id = "history-overflow".into();
    overflow.operation_id = "capacity-overflow".into();
    assert!(host.save_editor_draft(&overflow).is_err());
    drop(host);

    let host = Workbench::open(&path, None).unwrap();
    let mut cursor = String::new();
    let mut seen = std::collections::BTreeSet::new();
    loop {
        let page = host.list_editor_draft_history(&cursor, 17).unwrap();
        assert_eq!((page.unarchived, page.archived), (256, 1));
        assert!(page.rows.len() <= 17);
        for row in page.rows {
            assert!(seen.insert(row.draft_id));
        }
        if page.next_cursor.is_empty() {
            break;
        }
        assert!(page.next_cursor > cursor);
        cursor = page.next_cursor;
    }
    assert_eq!(seen.len(), 256);
    assert!(seen.contains("history-000"));
    assert_eq!(host.list_editor_drafts().unwrap().len(), 1);
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn archive_does_not_expand_the_sixteen_active_editor_limit() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = seed(&directory.path().join("workbench.db"));
    let original = request("active-limit-retired", 0, "retired");
    let record = host.save_editor_draft(&original).unwrap();
    host.discard_editor_draft(CARD, DRAFT, 1, "active-limit-retirement")
        .unwrap();
    host.archive_editor_draft(CARD, DRAFT, 2, &hash(&record), "active-limit-archive")
        .unwrap();
    for index in 0..16 {
        let mut value = request(&format!("active-save-{index}"), 0, "active");
        value.draft_id = format!("active-{index}");
        host.save_editor_draft(&value).unwrap();
    }
    let mut overflow = request("active-overflow", 0, "overflow");
    overflow.draft_id = "active-overflow".into();
    assert!(host.save_editor_draft(&overflow).is_err());
    assert_eq!(host.list_editor_drafts().unwrap().len(), 16);
    let page = host.list_editor_draft_history("", 32).unwrap();
    assert_eq!(
        (page.unarchived, page.archived, page.rows.len()),
        (16, 1, 1)
    );
}

#[test]
fn archive_waits_for_original_pending_and_ready_imports_to_retire() {
    use morrow_workbench_host::editor_draft_staging::{DraftImportPhase, proto::ImportRequest};
    for ready in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workbench.db");
        let mut host = seed(&path);
        let saved = host
            .save_editor_draft(&request("import-archive-draft", 0, ""))
            .unwrap();
        let digest = hash(&saved);
        let import = ImportRequest {
            schema_version: 1,
            card_id: CARD.into(),
            draft_id: DRAFT.into(),
            operation_id: "original-import-before-archive".into(),
            expected_generation: 1,
            name: "selected.bin".into(),
            kind: "file".into(),
            byte_length: FILE_BYTES.len() as u64,
            sha256: Sha256::digest(FILE_BYTES).to_vec(),
        };
        host.begin_editor_draft_import(&import).unwrap();
        if ready {
            host.import_editor_draft_asset_durable(&import, &mut &FILE_BYTES[..])
                .unwrap();
        }
        let original = host
            .discard_editor_draft(CARD, DRAFT, 1, "main-retired-before-import")
            .unwrap();
        let raw = original.slot.encode_to_vec();
        assert!(
            host.archive_editor_draft(CARD, DRAFT, 2, &digest, "archive-after-import-retired")
                .is_err()
        );
        assert!(
            host.inspect_editor_draft_archive(CARD, DRAFT)
                .unwrap()
                .is_none()
        );
        let retained = host
            .inspect_editor_draft_import(CARD, DRAFT, &import.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(retained.bytes_retained, ready);
        // Public phase means no longer selectable; it does not prove that the
        // durable import entry or its Snapshot owner has actually retired.
        assert_eq!(retained.phase, DraftImportPhase::Retired);
        assert_eq!(
            host.list_editor_draft_imports(CARD, DRAFT).unwrap().len(),
            1
        );
        drop(host);

        let mut host = Workbench::open(&path, Some(package())).unwrap();
        assert!(
            host.archive_editor_draft(CARD, DRAFT, 2, &digest, "archive-after-import-retired")
                .is_err()
        );
        host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
        assert!(
            host.list_editor_draft_imports(CARD, DRAFT)
                .unwrap()
                .is_empty()
        );
        let retired = host
            .inspect_editor_draft_import(CARD, DRAFT, &import.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(retired.phase, DraftImportPhase::Retired);
        assert!(!retired.bytes_retained);
        host.archive_editor_draft(CARD, DRAFT, 2, &digest, "archive-after-import-retired")
            .unwrap();
        assert_eq!(
            host.read_editor_draft(CARD, DRAFT)
                .unwrap()
                .unwrap()
                .slot
                .encode_to_vec(),
            raw
        );
        let historical = host
            .import_editor_draft_asset_durable(&import, &mut &[][..])
            .unwrap();
        assert_eq!(historical.phase, DraftImportPhase::Retired);
        assert!(!historical.bytes_retained);
        assert_eq!(host.list_editor_draft_history("", 32).unwrap().archived, 1);
        assert_eq!(business_revision(&host), 2);
    }
}

#[test]
fn archive_never_closes_or_adopts_a_live_original_capture_scope() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = seed(&directory.path().join("workbench.db"));
    let scope = host.open_capture_scope(CARD, 2).unwrap();
    let mut original = request("capture-draft-to-archive", 0, "");
    original.capture_scope = scope.clone();
    original.commit_operation = "capture-original-business".into();
    let saved = host.save_editor_draft(&original).unwrap();
    let digest = hash(&saved);
    let retired = host
        .discard_editor_draft(CARD, DRAFT, 1, "retire-before-capture-close")
        .unwrap();
    assert!(
        host.archive_editor_draft(CARD, DRAFT, 2, &digest, "archive-after-capture-close")
            .is_err()
    );
    assert!(
        host.inspect_editor_draft_archive(CARD, DRAFT)
            .unwrap()
            .is_none()
    );
    host.close_capture_scope(&scope).unwrap();
    host.archive_editor_draft(CARD, DRAFT, 2, &digest, "archive-after-capture-close")
        .unwrap();
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT).unwrap().unwrap().slot,
        retired.slot
    );
    assert!(host.export_editor_draft_capture(CARD, DRAFT, 2).is_err());
    assert_eq!(business_revision(&host), 2);
}
