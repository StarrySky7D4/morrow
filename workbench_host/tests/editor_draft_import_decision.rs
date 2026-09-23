#![cfg(target_os = "windows")]
//! Audited import abandonment decisions are durable and never inferred from Retired state.
use morrow_workbench_host::{
    editor_draft::model::proto::{TextValue, Values, WriteRequest},
    editor_draft_import_decision::DecisionStatus,
    editor_draft_staging::{proto::ImportRequest, DraftImportPhase},
    Workbench,
};
use sha2::{Digest, Sha256};
use std::path::Path;

const CARD: &str = "decision-new-card";
const DRAFT: &str = "decision-main-draft";
const BYTES: &[u8] = b"reserved import bytes";
fn text() -> TextValue {
    TextValue {
        text: String::new(),
        selection_base: -1,
        selection_extent: -1,
        affinity: 0,
        directional: false,
        composing_start: -1,
        composing_end: -1,
    }
}
fn blank_draft(card: &str, draft: &str, operation: &str) -> WriteRequest {
    WriteRequest {
        schema_version: 1,
        card_id: card.into(),
        draft_id: draft.into(),
        operation_id: operation.into(),
        expected_generation: 0,
        source_revision: 0,
        source_kind: 1,
        predecessor_operation: String::new(),
        predecessor_sha256: vec![],
        assets: vec![],
        values: Some(Values {
            title: Some(text()),
            description: Some(text()),
            hypothesis: Some(text()),
            conclusion: Some(text()),
            todos: Some(text()),
            category: String::new(),
            stage: String::new(),
        }),
    }
}
fn host(path: &Path) -> Workbench {
    let mut host = Workbench::open(path, None).unwrap();
    host.save_editor_draft(&blank_draft(CARD, DRAFT, "decision-create-draft"))
        .unwrap();
    host
}
fn import(operation: &str) -> ImportRequest {
    ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: operation.into(),
        expected_generation: 1,
        name: "reserved.bin".into(),
        kind: "file".into(),
        byte_length: BYTES.len() as u64,
        sha256: Sha256::digest(BYTES).to_vec(),
    }
}

#[test]
fn cancelled_original_operation_cannot_commit_and_a_new_decision_can_replace_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = host(&path);
    let proposal = import("decision-import-one");
    host.begin_editor_draft_import(&proposal).unwrap();
    let pending = host
        .prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-one",
        )
        .unwrap();
    assert_eq!(pending.status, DecisionStatus::Pending);
    assert_eq!(pending.decision_revision, 1);
    assert_eq!(pending.request, proposal);
    let retry = host
        .prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-one",
        )
        .unwrap();
    assert_eq!(retry.status, DecisionStatus::Pending);
    let cancelled = host
        .cancel_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-one",
        )
        .unwrap();
    assert_eq!(cancelled.status, DecisionStatus::Cancelled);
    assert_eq!(cancelled.decision_revision, 2);
    assert_eq!(cancelled.committed_revision, 0);
    assert!(host
        .commit_prepared_editor_draft_import_abandon(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-one",
        )
        .is_err());
    drop(host);

    let mut host = Workbench::open(&path, None).unwrap();
    let loaded = host
        .inspect_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-one",
        )
        .unwrap()
        .unwrap();
    assert_eq!(loaded.status, DecisionStatus::Cancelled);
    assert_eq!(
        host.cancel_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-one",
        )
        .unwrap()
        .status,
        DecisionStatus::Cancelled
    );
    let second = host
        .prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-two",
        )
        .unwrap();
    assert_eq!(second.status, DecisionStatus::Pending);
    let retired = host
        .commit_prepared_editor_draft_import_abandon(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-two",
        )
        .unwrap();
    assert_eq!(retired.phase, DraftImportPhase::Retired);
    let committed = host
        .inspect_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-two",
        )
        .unwrap()
        .unwrap();
    assert_eq!(committed.status, DecisionStatus::Committed);
    assert!(committed.committed_revision > 0);
}

#[test]
fn committed_decision_survives_stage_prune_and_exact_prepare_retry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = host(&path);
    let proposal = import("decision-import-committed");
    host.begin_editor_draft_import(&proposal).unwrap();
    host.prepare_editor_draft_import_decision(
        CARD,
        DRAFT,
        1,
        &proposal.operation_id,
        "decision-abandon-committed",
    )
    .unwrap();
    let committed = host
        .commit_prepared_editor_draft_import_abandon(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-committed",
        )
        .unwrap();
    assert_eq!(committed.phase, DraftImportPhase::Retired);
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    drop(host);

    let mut host = Workbench::open(&path, None).unwrap();
    let observed = host
        .prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-committed",
        )
        .unwrap();
    assert_eq!(observed.status, DecisionStatus::Committed);
    assert_eq!(observed.decision_revision, 1);
    assert!(observed.committed_revision > 0);
    assert!(host
        .cancel_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-committed"
        )
        .is_err());
    let receipt = host
        .commit_prepared_editor_draft_import_abandon(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-abandon-committed",
        )
        .unwrap();
    assert!(receipt.repeated);
    assert_eq!(receipt.phase, DraftImportPhase::Retired);
}

#[test]
fn exact_legacy_stage_commit_can_be_archived_without_reexecuting_abandon() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = host(&path);
    let proposal = import("decision-import-legacy");
    host.begin_editor_draft_import(&proposal).unwrap();
    let original = host
        .abandon_editor_draft_import(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-legacy-abandon",
        )
        .unwrap();
    assert_eq!(original.phase, DraftImportPhase::Retired);
    assert!(host
        .inspect_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-legacy-abandon"
        )
        .unwrap()
        .is_none());
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    let old_receipt = host
        .commit_prepared_editor_draft_import_abandon(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-legacy-abandon",
        )
        .unwrap();
    assert!(old_receipt.repeated);
    let archived = host
        .prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-legacy-abandon",
        )
        .unwrap();
    assert_eq!(archived.status, DecisionStatus::Committed);
    assert_eq!(archived.decision_revision, 1);
    assert!(archived.committed_revision > 0);
    assert_eq!(
        host.list_editor_draft_import_decisions(CARD, DRAFT, "", 32)
            .unwrap()
            .0
            .len(),
        1
    );
}

#[test]
fn foreign_used_operation_and_scope_cursor_cannot_become_decision_receipts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = host(&path);
    let proposal = import("decision-import-pages");
    host.begin_editor_draft_import(&proposal).unwrap();
    host.save_editor_draft(&blank_draft(
        "foreign-card",
        "foreign-draft",
        "foreign-abandon-operation",
    ))
    .unwrap();
    assert!(host
        .prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "foreign-abandon-operation"
        )
        .is_err());
    for index in 0..3 {
        let op = format!("decision-page-{index}");
        host.prepare_editor_draft_import_decision(CARD, DRAFT, 1, &proposal.operation_id, &op)
            .unwrap();
        host.cancel_editor_draft_import_decision(CARD, DRAFT, 1, &proposal.operation_id, &op)
            .unwrap();
    }
    let (first, cursor) = host
        .list_editor_draft_import_decisions(CARD, DRAFT, "", 2)
        .unwrap();
    assert_eq!(first.len(), 2);
    assert!(!cursor.is_empty());
    let (second, end) = host
        .list_editor_draft_import_decisions(CARD, DRAFT, &cursor, 2)
        .unwrap();
    assert_eq!(second.len(), 1);
    assert!(end.is_empty());
    let (foreign, _) = host
        .list_editor_draft_import_decisions("foreign-card", "foreign-draft", "", 2)
        .unwrap();
    assert!(foreign.is_empty());
    assert!(host
        .list_editor_draft_import_decisions("foreign-card", "foreign-draft", &cursor, 2)
        .is_err());
    assert!(host
        .list_editor_draft_import_decisions(CARD, DRAFT, "invalid-cursor", 2)
        .is_err());
}

#[test]
fn full_identity_budget_still_projects_only_exact_legacy_commit_read_only() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = host(&path);
    let proposal = import("decision-capacity-import");
    host.begin_editor_draft_import(&proposal).unwrap();
    for index in 0..256 {
        let op = format!("decision-capacity-cancel-{index}");
        let prepared = host
            .prepare_editor_draft_import_decision(CARD, DRAFT, 1, &proposal.operation_id, &op)
            .unwrap();
        assert_eq!(prepared.status, DecisionStatus::Pending);
        host.cancel_editor_draft_import_decision(CARD, DRAFT, 1, &proposal.operation_id, &op)
            .unwrap();
    }
    assert!(host
        .prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-capacity-fresh",
        )
        .is_err());
    let committed = host
        .abandon_editor_draft_import(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-capacity-legacy",
        )
        .unwrap();
    assert_eq!(committed.phase, DraftImportPhase::Retired);
    let projected = host
        .prepare_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-capacity-legacy",
        )
        .unwrap();
    assert_eq!(projected.status, DecisionStatus::Committed);
    assert_eq!(projected.decision_revision, 0);
    assert!(projected.committed_revision > 0);
    assert!(
        host.inspect_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-capacity-legacy",
        )
        .unwrap()
        .is_none(),
        "unarchived history must not become discoverable"
    );
    let old = host
        .commit_prepared_editor_draft_import_abandon(
            CARD,
            DRAFT,
            1,
            &proposal.operation_id,
            "decision-capacity-legacy",
        )
        .unwrap();
    assert!(old.repeated);
    assert!(host
        .list_editor_draft_import_decisions(CARD, DRAFT, "", 32)
        .unwrap()
        .0
        .iter()
        .all(|decision| decision.decision_revision > 0));
}

#[test]
fn global_scope_page_discovers_inactive_and_terminal_decisions_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = host(&path);
    let original = import("scope-first-import");
    host.begin_editor_draft_import(&original).unwrap();
    host.prepare_editor_draft_import_decision(
        CARD,
        DRAFT,
        1,
        &original.operation_id,
        "scope-first-abandon",
    )
    .unwrap();
    host.discard_editor_draft(CARD, DRAFT, 1, "scope-first-discard")
        .unwrap();
    let second_card = "decision-second-card";
    let second_draft = "decision-second-draft";
    host.save_editor_draft(&blank_draft(
        second_card,
        second_draft,
        "scope-second-create",
    ))
    .unwrap();
    let mut second = import("scope-second-import");
    second.card_id = second_card.into();
    second.draft_id = second_draft.into();
    host.begin_editor_draft_import(&second).unwrap();
    host.prepare_editor_draft_import_decision(
        second_card,
        second_draft,
        1,
        &second.operation_id,
        "scope-second-abandon",
    )
    .unwrap();
    host.cancel_editor_draft_import_decision(
        second_card,
        second_draft,
        1,
        &second.operation_id,
        "scope-second-abandon",
    )
    .unwrap();
    drop(host);

    let host = Workbench::open(&path, None).unwrap();
    let (first, cursor) = host
        .list_editor_draft_import_decision_scopes("", 1)
        .unwrap();
    assert_eq!(first.len(), 1);
    assert!(!cursor.is_empty());
    let (second, end) = host
        .list_editor_draft_import_decision_scopes(&cursor, 1)
        .unwrap();
    assert_eq!(second.len(), 1);
    let mut found = first.into_iter().chain(second).collect::<Vec<_>>();
    found.sort();
    assert_eq!(
        found,
        vec![
            (CARD.into(), DRAFT.into()),
            (second_card.into(), second_draft.into()),
        ]
    );
    // A full page may conservatively return a cursor; the next call is empty.
    if !end.is_empty() {
        assert!(host
            .list_editor_draft_import_decision_scopes(&end, 1)
            .unwrap()
            .0
            .is_empty());
    }
    assert!(host
        .list_editor_draft_import_decision_scopes("invalid-scope", 1)
        .is_err());
    let conflicted = host
        .inspect_editor_draft_import_decision(
            CARD,
            DRAFT,
            1,
            &original.operation_id,
            "scope-first-abandon",
        )
        .unwrap()
        .unwrap();
    assert_eq!(conflicted.status, DecisionStatus::Conflict);
}
