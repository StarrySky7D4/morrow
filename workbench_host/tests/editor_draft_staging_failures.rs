#![cfg(target_os = "windows")]
//! Failed secondary journal writes must not lose durable bytes or advance drafts.
use morrow_workbench_host::{
    Workbench,
    editor_draft::model::proto::{AssetSelection, TextValue, Values, WriteRequest},
    editor_draft_staging::proto::ImportRequest,
};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};

const CARD: &str = "not-created-yet";
const DRAFT: &str = "failure-draft";
const BYTES: &[u8] = b"bytes retained before ready acknowledgment";
struct Unreadable;
impl Read for Unreadable {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("an admitted import with verified retained bytes must not reread input")
    }
}
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
fn draft(op: &str, generation: u64, assets: Vec<AssetSelection>) -> WriteRequest {
    WriteRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: op.into(),
        expected_generation: generation,
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
        assets,
    }
}
fn request(op: &str) -> ImportRequest {
    ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: op.into(),
        expected_generation: 1,
        name: "held.bin".into(),
        kind: "file".into(),
        byte_length: BYTES.len() as u64,
        sha256: Sha256::digest(BYTES).to_vec(),
    }
}
fn deny_staging_updates(sql: &rusqlite::Connection) {
    sql.execute_batch(
        "CREATE TRIGGER reject_staging_update BEFORE UPDATE ON cards
        WHEN OLD.id LIKE 'morrow-host-editor-imports-%'
        BEGIN SELECT RAISE(ABORT, 'test staging transition blocked'); END;",
    )
    .unwrap();
}
fn allow_staging_updates(sql: &rusqlite::Connection) {
    sql.execute_batch("DROP TRIGGER reject_staging_update;")
        .unwrap();
}

#[test]
fn retained_bytes_survive_ready_failure_and_later_draft_generation_without_reread() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let source = directory.path().join("source.bin");
    std::fs::write(&source, BYTES).unwrap();
    let mut host = Workbench::open(&path, None).unwrap();
    host.save_editor_draft(&draft("first-draft", 0, vec![]))
        .unwrap();
    let original = request("import-with-ready-failure");
    host.begin_editor_draft_import(&original).unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    deny_staging_updates(&sql);
    assert!(
        host.import_editor_draft_asset_durable(
            &original,
            &mut std::fs::File::open(&source).unwrap()
        )
        .is_err()
    );
    let pending = host
        .inspect_editor_draft_import(CARD, DRAFT, &original.operation_id)
        .unwrap()
        .unwrap();
    assert!(pending.bytes_retained);
    assert!(
        !pending.current_active,
        "Pending bytes are not yet selectable"
    );
    assert_eq!(format!("{:?}", pending.phase), "Pending");
    allow_staging_updates(&sql);
    // Only the draft body advances. Original admitted byte identity is fixed.
    host.save_editor_draft(&draft("second-draft", 1, vec![]))
        .unwrap();
    drop(sql);
    drop(host);
    std::fs::remove_file(source).unwrap();
    let mut host = Workbench::open(&path, None).unwrap();
    let ready = host
        .import_editor_draft_asset_durable(&original, &mut Unreadable)
        .unwrap();
    assert!(ready.bytes_retained && ready.current_active);
    assert_eq!(ready.request, original);
    assert_eq!(format!("{:?}", ready.phase), "Ready");
    assert_eq!(
        host.export_editor_draft_import(CARD, DRAFT, 2, &original.operation_id)
            .unwrap(),
        BYTES
    );
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        2
    );
    assert!(host.page_versioned("", 100).unwrap().0.is_empty());
}

#[test]
fn cleanup_failure_keeps_consumption_marker_and_blocks_later_draft_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = Workbench::open(&path, None).unwrap();
    host.save_editor_draft(&draft("first-draft", 0, vec![]))
        .unwrap();
    let original = request("selected-import");
    let imported = host
        .import_editor_draft_asset_durable(&original, &mut Cursor::new(BYTES))
        .unwrap();
    let selected = draft(
        "select-import",
        1,
        vec![AssetSelection {
            origin: 2,
            asset_id: imported.asset_id.clone(),
            aliases: vec![],
        }],
    );
    let committed = host.save_editor_draft(&selected).unwrap();
    assert_eq!(
        committed.slot.consumed_imports,
        vec![original.operation_id.clone()]
    );
    let sql = rusqlite::Connection::open(&path).unwrap();
    deny_staging_updates(&sql);
    assert!(
        host.save_editor_draft(&draft("remove-selection", 2, vec![]))
            .is_err()
    );
    let retained = host.read_editor_draft(CARD, DRAFT).unwrap().unwrap();
    assert_eq!(retained.slot.generation, 2);
    assert_eq!(retained.slot.assets.len(), 1);
    let old = host
        .import_editor_draft_asset_durable(&original, &mut Unreadable)
        .unwrap();
    assert!(!old.current_active);
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 2, &imported.asset_id)
            .unwrap(),
        BYTES
    );
    allow_staging_updates(&sql);
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    let removed = host
        .save_editor_draft(&draft("remove-selection", 2, vec![]))
        .unwrap();
    assert_eq!(removed.slot.generation, 3);
    assert!(removed.slot.consumed_imports.is_empty());
    drop(sql);
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    assert!(
        !host
            .import_editor_draft_asset_durable(&original, &mut Unreadable)
            .unwrap()
            .current_active
    );
    assert!(
        host.export_editor_draft_import(CARD, DRAFT, 3, &original.operation_id)
            .is_err()
    );
    assert!(
        host.save_editor_draft(&draft("reclaim-old-stage", 3, selected.assets))
            .is_err()
    );
}

#[test]
fn failed_release_and_failed_prune_remain_visible_until_explicit_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = Workbench::open(&path, None).unwrap();
    host.save_editor_draft(&draft("first-draft", 0, vec![]))
        .unwrap();
    let original = request("abandoned-import");
    host.import_editor_draft_asset_durable(&original, &mut Cursor::new(BYTES))
        .unwrap();
    let abandoned = host
        .abandon_editor_draft_import(CARD, DRAFT, 1, &original.operation_id, "explicit-abandon")
        .unwrap();
    assert!(!abandoned.current_active && abandoned.bytes_retained);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER reject_snapshot_release BEFORE DELETE ON retentions
         WHEN OLD.owner LIKE 'morrow-draft-import-owner-%'
         BEGIN SELECT RAISE(ABORT, 'test Snapshot release blocked'); END;",
    )
    .unwrap();
    assert!(host.reconcile_editor_draft_imports(CARD, DRAFT).is_err());
    let still_held = host.list_editor_draft_imports(CARD, DRAFT).unwrap();
    assert_eq!(still_held.len(), 1);
    assert!(still_held[0].bytes_retained && !still_held[0].current_active);
    sql.execute_batch("DROP TRIGGER reject_snapshot_release;")
        .unwrap();
    deny_staging_updates(&sql);
    // Snapshot release can succeed while the audited prune fails. The entry
    // must still be visible and charged until that second write is confirmed.
    assert!(host.reconcile_editor_draft_imports(CARD, DRAFT).is_err());
    let awaiting_prune = host.list_editor_draft_imports(CARD, DRAFT).unwrap();
    assert_eq!(awaiting_prune.len(), 1);
    assert!(!awaiting_prune[0].bytes_retained && !awaiting_prune[0].current_active);
    let retired_revision = awaiting_prune[0].staging_revision;
    allow_staging_updates(&sql);
    drop(sql);
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    // Reopening and an exact old import retry cannot silently clean or revive it.
    let before = host
        .import_editor_draft_asset_durable(&original, &mut Unreadable)
        .unwrap();
    assert!(!before.current_active && !before.bytes_retained);
    assert_eq!(before.staging_revision, retired_revision);
    assert_eq!(
        host.list_editor_draft_imports(CARD, DRAFT).unwrap().len(),
        1
    );
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    assert!(
        host.list_editor_draft_imports(CARD, DRAFT)
            .unwrap()
            .is_empty()
    );
    let replay = host
        .abandon_editor_draft_import(CARD, DRAFT, 1, &original.operation_id, "explicit-abandon")
        .unwrap();
    assert!(replay.repeated && !replay.current_active && !replay.bytes_retained);
    assert_eq!(replay.staging_revision, retired_revision + 1);
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        1
    );
}

#[test]
fn pending_commit_failure_never_reads_source_or_leaves_retained_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = Workbench::open(&path, None).unwrap();
    host.save_editor_draft(&draft("first-draft", 0, vec![]))
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER reject_import_pending BEFORE INSERT ON cards
         WHEN NEW.id LIKE 'morrow-host-editor-imports-%'
         BEGIN SELECT RAISE(ABORT, 'test Pending admission blocked'); END;",
    )
    .unwrap();
    let original = request("pending-failure");
    assert!(
        host.import_editor_draft_asset_durable(&original, &mut Unreadable)
            .is_err()
    );
    assert!(
        host.inspect_editor_draft_import(CARD, DRAFT, &original.operation_id)
            .unwrap()
            .is_none()
    );
    assert!(
        host.list_editor_draft_imports(CARD, DRAFT)
            .unwrap()
            .is_empty()
    );
    let owners: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM retentions WHERE owner LIKE 'morrow-draft-import-owner-%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(owners, 0);
    sql.execute_batch("DROP TRIGGER reject_import_pending;")
        .unwrap();
    drop(sql);
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    let ready = host
        .import_editor_draft_asset_durable(&original, &mut Cursor::new(BYTES))
        .unwrap();
    assert!(ready.current_active && ready.bytes_retained);
    assert_eq!(ready.request, original);
    assert_eq!(ready.staging_revision, 2);
}

#[test]
fn selected_pin_survives_release_then_prune_failure_and_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let mut host = Workbench::open(&path, None).unwrap();
    host.save_editor_draft(&draft("first-draft", 0, vec![]))
        .unwrap();
    let original = request("selected-prune-failure");
    let ready = host
        .import_editor_draft_asset_durable(&original, &mut Cursor::new(BYTES))
        .unwrap();
    let selection = AssetSelection {
        origin: 2,
        asset_id: ready.asset_id.clone(),
        aliases: vec![],
    };
    host.save_editor_draft(&draft("select-import", 1, vec![selection]))
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    // Retire is allowed while the Snapshot exists. Once release succeeds,
    // reject the following metadata prune without touching the selected pin.
    sql.execute_batch(
        "CREATE TRIGGER reject_selected_prune BEFORE UPDATE ON cards
         WHEN OLD.id LIKE 'morrow-host-editor-imports-%'
          AND NOT EXISTS(SELECT 1 FROM retentions WHERE owner LIKE 'morrow-draft-import-owner-%')
         BEGIN SELECT RAISE(ABORT, 'test selected import prune blocked'); END;",
    )
    .unwrap();
    assert!(host.reconcile_editor_draft_imports(CARD, DRAFT).is_err());
    let pending_cleanup = host.list_editor_draft_imports(CARD, DRAFT).unwrap();
    assert_eq!(pending_cleanup.len(), 1);
    assert!(!pending_cleanup[0].current_active && !pending_cleanup[0].bytes_retained);
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 2, &ready.asset_id)
            .unwrap(),
        BYTES
    );
    assert!(
        host.save_editor_draft(&draft("remove-selection", 2, vec![]))
            .is_err()
    );
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        2
    );
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    let old = host
        .import_editor_draft_asset_durable(&original, &mut Unreadable)
        .unwrap();
    assert!(!old.current_active && !old.bytes_retained);
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 2, &ready.asset_id)
            .unwrap(),
        BYTES
    );
    sql.execute_batch("DROP TRIGGER reject_selected_prune;")
        .unwrap();
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    assert!(
        host.list_editor_draft_imports(CARD, DRAFT)
            .unwrap()
            .is_empty()
    );
    // Cleaning temporary ownership still cannot revoke the confirmed draft pin.
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 2, &ready.asset_id)
            .unwrap(),
        BYTES
    );
    let removed = host
        .save_editor_draft(&draft("remove-selection", 2, vec![]))
        .unwrap();
    assert_eq!(removed.slot.generation, 3);
    assert!(removed.slot.consumed_imports.is_empty());
}
