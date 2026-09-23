#![cfg(target_os = "windows")]
//! The staging revision cap reserves completion and cleanup for accepted imports.
use morrow_workbench_host::{
    editor_draft::model::proto::{TextValue, Values, WriteRequest},
    editor_draft_staging::{proto::ImportRequest, DraftImportPhase},
    Workbench,
};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};

const CARD: &str = "staging-capacity-new-card";
const DRAFT: &str = "staging-capacity-draft";
const BYTES: &[u8] = b"one selected asset at the final staging revision";

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
fn blank_draft() -> WriteRequest {
    WriteRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: "staging-capacity-draft-create".into(),
        expected_generation: 0,
        source_revision: 0,
        source_kind: 1,
        predecessor_operation: String::new(),
        predecessor_sha256: vec![],
        values: Some(Values {
            title: Some(text()),
            description: Some(text()),
            hypothesis: Some(text()),
            conclusion: Some(text()),
            todos: Some(text()),
            category: String::new(),
            stage: String::new(),
        }),
        assets: vec![],
    }
}
fn request(index: usize, bytes: &[u8]) -> ImportRequest {
    ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: format!("staging-capacity-import-{index}"),
        expected_generation: 1,
        name: format!("asset-{index}.bin"),
        kind: "file".into(),
        byte_length: bytes.len() as u64,
        sha256: Sha256::digest(bytes).to_vec(),
    }
}
struct Unreadable;
impl Read for Unreadable {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("capacity or historical retry must not consume source bytes")
    }
}

#[test]
fn admitted_imports_keep_ready_retire_prune_budget_at_revision_256() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = Workbench::open(&path, None).unwrap();
    host.save_editor_draft(&blank_draft()).unwrap();

    // Each Pending -> explicit Retired -> Pruned cycle consumes three revisions.
    // The 84 cycles leave revision 252 while retaining 4 for a final Ready cycle.
    for index in 0..84 {
        let proposal = request(index, BYTES);
        let pending = host.begin_editor_draft_import(&proposal).unwrap();
        assert_eq!(pending.staging_revision, 3 * index as u64 + 1);
        assert_eq!(pending.phase, DraftImportPhase::Pending);
        let retired = host
            .abandon_editor_draft_import(
                CARD,
                DRAFT,
                1,
                &proposal.operation_id,
                &format!("staging-capacity-abandon-{index}"),
            )
            .unwrap();
        assert_eq!(retired.phase, DraftImportPhase::Retired);
        host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
        assert!(host
            .list_editor_draft_imports(CARD, DRAFT)
            .unwrap()
            .is_empty());
    }

    let final_request = request(84, BYTES);
    let pending = host.begin_editor_draft_import(&final_request).unwrap();
    assert_eq!(pending.staging_revision, 253);
    let ready = host
        .import_editor_draft_asset_durable(&final_request, &mut Cursor::new(BYTES))
        .unwrap();
    assert_eq!(ready.staging_revision, 254);
    assert_eq!(ready.phase, DraftImportPhase::Ready);
    assert!(ready.bytes_retained && ready.current_active);
    let retired = host
        .abandon_editor_draft_import(
            CARD,
            DRAFT,
            1,
            &final_request.operation_id,
            "staging-capacity-final-abandon",
        )
        .unwrap();
    assert_eq!(retired.staging_revision, 255);
    assert_eq!(retired.phase, DraftImportPhase::Retired);
    assert!(
        retired.bytes_retained,
        "release follows the audited Retired transition"
    );
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    let final_record = host
        .inspect_editor_draft_import(CARD, DRAFT, &final_request.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(final_record.staging_revision, 256);
    assert_eq!(final_record.phase, DraftImportPhase::Retired);
    assert!(!final_record.bytes_retained);
    assert!(host
        .list_editor_draft_imports(CARD, DRAFT)
        .unwrap()
        .is_empty());

    // An unadmitted operation cannot read the chosen file or add a history row.
    let overflow = request(85, BYTES);
    assert!(host
        .import_editor_draft_asset_durable(&overflow, &mut Unreadable)
        .is_err());
    assert!(host
        .inspect_editor_draft_import(CARD, DRAFT, &overflow.operation_id)
        .unwrap()
        .is_none());
    let old = host
        .import_editor_draft_asset_durable(&final_request, &mut Unreadable)
        .unwrap();
    assert!(old.repeated);
    assert_eq!(old.phase, DraftImportPhase::Retired);
    assert_eq!(old.staging_revision, 256);
    assert!(!old.bytes_retained);
    assert!(host
        .list_editor_draft_imports(CARD, DRAFT)
        .unwrap()
        .is_empty());
}

#[test]
fn cleanup_rejects_ready_entry_with_missing_snapshot_owner_without_pruning_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = Workbench::open(&path, None).unwrap();
    host.save_editor_draft(&blank_draft()).unwrap();
    let proposal = request(901, BYTES);
    let ready = host
        .import_editor_draft_asset_durable(&proposal, &mut Cursor::new(BYTES))
        .unwrap();
    assert_eq!(ready.staging_revision, 2);
    assert_eq!(ready.phase, DraftImportPhase::Ready);
    host.discard_editor_draft(CARD, DRAFT, 1, "staging-capacity-discard")
        .unwrap();

    // Simulate a corrupt missing Snapshot owner after the main draft is
    // inactive. Keep the original row so the test can repair it afterward.
    let sql = rusqlite::Connection::open(&path).unwrap();
    let (owner, kind, blob, metadata): (String, i64, String, Vec<u8>) = sql
        .query_row(
            "SELECT owner,kind,blob_id,metadata FROM retentions LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        sql.execute(
            "DELETE FROM retentions WHERE owner=?1 AND kind=?2 AND blob_id=?3",
            rusqlite::params![owner, kind, blob],
        )
        .unwrap(),
        1
    );
    assert!(host.reconcile_editor_draft_imports(CARD, DRAFT).is_err());
    sql.execute(
        "INSERT INTO retentions(owner,kind,blob_id,metadata) VALUES(?1,?2,?3,?4)",
        rusqlite::params![owner, kind, blob, metadata],
    )
    .unwrap();
    let preserved = host
        .inspect_editor_draft_import(CARD, DRAFT, &proposal.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        preserved.staging_revision, 2,
        "no Retired or Prune audit was written"
    );
    assert_eq!(preserved.phase, DraftImportPhase::Retired);
    assert!(preserved.bytes_retained);

    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    let cleaned = host
        .inspect_editor_draft_import(CARD, DRAFT, &proposal.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(cleaned.staging_revision, 4);
    assert_eq!(cleaned.phase, DraftImportPhase::Retired);
    assert!(!cleaned.bytes_retained);
}
