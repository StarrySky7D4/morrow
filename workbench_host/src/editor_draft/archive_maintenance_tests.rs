//! Generated v1 compatibility fixtures, separate from the original-library GUI
//! qualification. Only the old archive marker is installed as a test fixture;
//! draft/import/decision effects and maintenance use the real production APIs.
use super::*;
use crate::editor_draft_import_decision::DecisionStatus;

const CARD: &str = "legacy-maintenance-card";
const DRAFT: &str = "legacy-maintenance-draft";

fn write_request(draft: &str, operation: &str) -> proto::WriteRequest {
    let text = || {
        Some(proto::TextValue {
            text: "retained original".into(),
            composing_start: -1,
            composing_end: -1,
            ..Default::default()
        })
    };
    proto::WriteRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: draft.into(),
        operation_id: operation.into(),
        source_kind: 1,
        commit_operation: format!("business-{draft}"),
        values: Some(proto::Values {
            title: text(),
            description: text(),
            hypothesis: text(),
            conclusion: text(),
            todos: text(),
            category: "进行中".into(),
            stage: "计划中".into(),
        }),
        ..Default::default()
    }
}

fn imported(
    host: &mut Workbench,
    draft: &str,
    operation: &str,
) -> crate::editor_draft_staging::proto::ImportRequest {
    let request = crate::editor_draft_staging::proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: draft.into(),
        operation_id: operation.into(),
        expected_generation: 1,
        name: "original.bin".into(),
        kind: "file".into(),
        byte_length: 4,
        sha256: Sha256::digest(b"kept").to_vec(),
    };
    host.begin_editor_draft_import(&request).unwrap();
    request
}

fn legacy_fixture(
    host: &mut Workbench,
    decisions: usize,
    cancel: bool,
) -> (EditorDraftArchiveMaintenanceRequest, Vec<u8>) {
    host.save_editor_draft(&write_request(DRAFT, "legacy-main-save"))
        .unwrap();
    let import = imported(host, DRAFT, "legacy-import");
    for index in 0..decisions {
        let operation = format!("legacy-decision-{index}");
        host.prepare_editor_draft_import_decision(CARD, DRAFT, 1, &import.operation_id, &operation)
            .unwrap();
        if cancel {
            host.cancel_editor_draft_import_decision(
                CARD,
                DRAFT,
                1,
                &import.operation_id,
                &operation,
            )
            .unwrap();
        }
    }
    host.discard_editor_draft(CARD, DRAFT, 1, "legacy-retire")
        .unwrap();
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    let state = host.local_state_mut().unwrap();
    let (original, slot) = state.draft_card(CARD, DRAFT).unwrap().unwrap();
    let marker = Marker {
        version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        generation: slot.generation,
        request_sha256: Sha256::digest(slot.request.as_ref().unwrap().encode_to_vec()).to_vec(),
        slot_sha256: Sha256::digest(slot.encode_to_vec()).to_vec(),
        operation: "legacy-archive".into(),
        retained_bytes: original.encode().len() as u64,
        auxiliary_count: 0,
        auxiliary_sha256: vec![],
    };
    let archived = CardRecord::new_with_attachments(
        &archive_key(CARD, DRAFT),
        ARCHIVE_TYPE,
        1,
        ARCHIVE_TITLE,
        marker.encode_to_vec(),
        &[],
    )
    .unwrap();
    // Test-only construction of the exact legacy format and its own CreateCard
    // transaction. This is not evidence that a historical binary wrote it.
    state
        .host
        .store_local_mut()
        .create_local(&marker.operation, &archived)
        .unwrap();
    let request = EditorDraftArchiveMaintenanceRequest {
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        generation: marker.generation,
        request_sha256: marker.request_sha256,
        archive_operation: marker.operation,
        archive_sha256: Sha256::digest(archived.encode()).to_vec(),
        operation: "maintain-original".into(),
    };
    (request, archived.encode())
}

#[test]
fn generated_v1_256_decision_capacity_retires_only_through_explicit_receipt_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let mut host = Workbench::open(&path, Some(crate::test_common::package())).unwrap();
    let (request, original_marker) = legacy_fixture(&mut host, 256, true);
    host.save_editor_draft(&write_request("fresh-draft", "fresh-main-save"))
        .unwrap();
    let fresh = imported(&mut host, "fresh-draft", "fresh-import");
    assert!(
        host.prepare_editor_draft_import_decision(
            CARD,
            "fresh-draft",
            1,
            &fresh.operation_id,
            "fresh-decision"
        )
        .unwrap_err()
        .to_string()
        .contains("capacity")
    );
    let before = host.list_editor_draft_history("", 32).unwrap();
    let original_export = host
        .local_state()
        .unwrap()
        .export_editor_draft_history(CARD, DRAFT, 2)
        .unwrap();
    assert!(
        host.local_state()
            .unwrap()
            .archived_draft_auxiliary_keys(Kind::Decision)
            .unwrap()
            .is_empty()
    );
    assert!(
        host.inspect_editor_draft_archive_maintenance(CARD, DRAFT)
            .unwrap()
            .is_none()
    );
    for mutation in 0..6 {
        let mut changed = request.clone();
        match mutation {
            0 => changed.generation += 1,
            1 => changed.request_sha256[0] ^= 1,
            2 => changed.archive_sha256[0] ^= 1,
            3 => changed.archive_operation.push_str("-wrong"),
            4 => changed.operation = changed.archive_operation.clone(),
            _ => changed.operation = "legacy-main-save".into(),
        }
        assert!(host.maintain_editor_draft_archive(&changed).is_err());
        assert!(
            host.inspect_editor_draft_archive_maintenance(CARD, DRAFT)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            host.list_editor_draft_history("", 32)
                .unwrap()
                .archived_bytes,
            before.archived_bytes
        );
    }
    let receipt = host.maintain_editor_draft_archive(&request).unwrap();
    assert!(!receipt.repeated);
    assert_eq!(receipt.auxiliary_count, 257);
    let after = host.list_editor_draft_history("", 32).unwrap();
    assert_eq!(after.archived, before.archived);
    assert_eq!(after.unarchived, before.unarchived);
    assert!(after.archived_bytes > before.archived_bytes + receipt.auxiliary_bytes);
    let repeated = host.maintain_editor_draft_archive(&request).unwrap();
    assert!(repeated.repeated);
    assert_eq!(repeated.auxiliary_sha256, receipt.auxiliary_sha256);
    let mut changed = request.clone();
    changed.operation = "second-credit".into();
    assert!(host.maintain_editor_draft_archive(&changed).is_err());
    assert_eq!(
        host.list_editor_draft_history("", 32)
            .unwrap()
            .archived_bytes,
        after.archived_bytes
    );
    assert_eq!(
        host.local_state()
            .unwrap()
            .archived_draft_auxiliary_keys(Kind::Decision)
            .unwrap()
            .len(),
        256
    );
    assert_eq!(
        host.prepare_editor_draft_import_decision(
            CARD,
            "fresh-draft",
            1,
            &fresh.operation_id,
            "fresh-decision"
        )
        .unwrap()
        .status,
        DecisionStatus::Pending
    );
    host.cancel_editor_draft_import_decision(
        CARD,
        "fresh-draft",
        1,
        &fresh.operation_id,
        "fresh-decision",
    )
    .unwrap();
    drop(host);
    let mut host = Workbench::open(&path, Some(crate::test_common::package())).unwrap();
    let state = host.local_state().unwrap();
    assert_eq!(
        state
            .host
            .store_local()
            .card(&archive_key(CARD, DRAFT))
            .unwrap()
            .unwrap()
            .encode(),
        original_marker
    );
    assert_eq!(
        state.export_editor_draft_history(CARD, DRAFT, 2).unwrap(),
        original_export
    );
    assert_eq!(
        host.inspect_editor_draft_archive(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .version,
        1
    );
    let restarted = host
        .inspect_editor_draft_archive_maintenance(CARD, DRAFT)
        .unwrap()
        .unwrap();
    assert_eq!(restarted, repeated);
    assert_eq!(
        host.list_editor_draft_history("", 32)
            .unwrap()
            .archived_bytes,
        after.archived_bytes
    );
    for index in 0..256 {
        assert_eq!(
            host.inspect_editor_draft_import_decision(
                CARD,
                DRAFT,
                1,
                "legacy-import",
                &format!("legacy-decision-{index}")
            )
            .unwrap()
            .unwrap()
            .status,
            DecisionStatus::Cancelled
        );
    }
    assert!(
        host.maintain_editor_draft_archive(&request)
            .unwrap()
            .repeated
    );
}

#[test]
fn legacy_pending_decision_blocks_maintenance_until_its_original_explicit_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let mut host =
        Workbench::open(&dir.path().join("db"), Some(crate::test_common::package())).unwrap();
    let (request, original) = legacy_fixture(&mut host, 1, false);
    assert!(
        host.maintain_editor_draft_archive(&request)
            .unwrap_err()
            .to_string()
            .contains("resolve original auxiliary")
    );
    assert!(
        host.inspect_editor_draft_archive_maintenance(CARD, DRAFT)
            .unwrap()
            .is_none()
    );
    host.cancel_editor_draft_import_decision(CARD, DRAFT, 1, "legacy-import", "legacy-decision-0")
        .unwrap();
    assert_eq!(
        host.maintain_editor_draft_archive(&request)
            .unwrap()
            .auxiliary_count,
        2
    );
    assert_eq!(
        host.local_state()
            .unwrap()
            .host
            .store_local()
            .card(&archive_key(CARD, DRAFT))
            .unwrap()
            .unwrap()
            .encode(),
        original
    );
}

#[test]
fn same_shaped_maintenance_without_its_exact_operation_receipt_never_releases_quota() {
    let dir = tempfile::tempdir().unwrap();
    let mut host =
        Workbench::open(&dir.path().join("db"), Some(crate::test_common::package())).unwrap();
    let (request, _) = legacy_fixture(&mut host, 1, true);
    let state = host.local_state_mut().unwrap();
    let auxiliary = state.draft_archive_auxiliary().unwrap();
    let binding = auxiliary.binding(CARD, DRAFT).unwrap();
    let marker = MaintenanceMarker {
        version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        generation: 2,
        request_sha256: request.request_sha256,
        archive_operation: request.archive_operation,
        archive_sha256: request.archive_sha256,
        operation: request.operation,
        auxiliary_count: binding.count,
        auxiliary_bytes: binding.bytes,
        auxiliary_sha256: binding.digest,
    };
    let card = CardRecord::new_with_attachments(
        &maintenance_key(CARD, DRAFT),
        MAINTENANCE_TYPE,
        1,
        MAINTENANCE_TITLE,
        marker.encode_to_vec(),
        &[],
    )
    .unwrap();
    state
        .host
        .store_local_mut()
        .create_local("different-actual-operation", &card)
        .unwrap();
    assert!(
        state
            .inspect_editor_draft_archive_maintenance(CARD, DRAFT)
            .unwrap_err()
            .to_string()
            .contains("operation missing")
    );
    assert!(state.archived_draft_auxiliary_keys(Kind::Decision).is_err());
    assert!(state.list_editor_draft_history("", 32).is_err());
}
