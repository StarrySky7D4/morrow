#![cfg(target_os = "windows")]
//! Real host and compiled WASM coverage for the private editor draft journal.
use morrow_core::plugin_package::proto::{Capability, TransformHandler};
use morrow_core::{content::CardRecord, plugin_package::Package};
use morrow_workbench_host::{
    capture_provenance::EditorSnapshot, cards_content::CardAction,
    versioned_record::VersionedRecord, Workbench,
};
use morrow_workbench_plugin::{cards_v2::Fields, Asset, Idea, PACKAGE_VERSION};
use std::path::Path;

const CARD: &str = "draft-card";
const DRAFT: &str = "draft-one";
const FILE_BYTES: &[u8] = b"draft file survives removed source\x00\xff";

fn package() -> Package {
    let module = std::fs::read(
        std::env::var("MORROW_WORKBENCH_WASM").expect("real compiled Rust guest path"),
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "test.editor-draft",
        PACKAGE_VERSION,
        &module,
        [
            (
                "studio.command",
                "morrow.studio.request.v1",
                "morrow.studio.response.v1",
            ),
            (
                "workbench.command",
                "morrow.workbench.request.v1",
                "morrow.workbench.response.v1",
            ),
            (
                "workbench.tasks.v2",
                "morrow.workbench.tasks.request.v2",
                "morrow.workbench.tasks.response.v2",
            ),
            (
                "workbench.cards.v2",
                "morrow.workbench.cards.request.v2",
                "morrow.workbench.cards.response.v2",
            ),
            (
                "capture.convert",
                "morrow.capture.request.v1",
                "morrow.capture.response.v1",
            ),
        ]
        .into_iter()
        .map(|(handler, input_type, output_type)| TransformHandler {
            handler: handler.into(),
            input_type: input_type.into(),
            output_type: output_type.into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        })
        .collect(),
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    Package::build(manifest, &module).unwrap()
}

fn seed(path: &Path) -> Workbench {
    let mut host = Workbench::open(path, Some(package())).unwrap();
    host.create(
        "draft-seed",
        Idea {
            id: CARD.into(),
            title: "Business title".into(),
            description: "Business body".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: vec!["first task".into(), "second task".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let migration = host.plan_tasks_migration(CARD).unwrap();
    host.migrate_tasks(&migration.operation, CARD, migration.source_revision)
        .unwrap();
    assert_eq!(business_revision(&host), 2);
    host
}

fn business_revision(host: &Workbench) -> u64 {
    match host.read_versioned(CARD).unwrap() {
        VersionedRecord::Tasks(record) => record.revision,
        VersionedRecord::Legacy(_) => panic!("expected migrated V2 card"),
    }
}
fn text_value(text: &str) -> morrow_workbench_host::editor_draft::model::proto::TextValue {
    use morrow_workbench_host::editor_draft::model::proto::TextValue;
    TextValue {
        text: text.into(),
        selection_base: 0,
        selection_extent: 0,
        affinity: 0,
        directional: false,
        composing_start: -1,
        composing_end: -1,
    }
}

fn request(
    operation: &str,
    expected_generation: u64,
    title: &str,
) -> morrow_workbench_host::editor_draft::model::proto::WriteRequest {
    use morrow_workbench_host::editor_draft::model::proto::{Values, WriteRequest};
    WriteRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: operation.into(),
        expected_generation,
        source_revision: 2,
        source_kind: 0,
        predecessor_operation: String::new(),
        predecessor_sha256: vec![],
        values: Some(Values {
            title: Some(text_value(title)),
            description: Some(text_value("raw body")),
            hypothesis: Some(text_value("")),
            conclusion: Some(text_value("")),
            todos: Some(text_value("")),
            category: "进行中".into(),
            stage: "计划中".into(),
        }),
        assets: vec![],
    }
}

#[test]
fn raw_invalid_text_and_emoji_selection_survive_restart_without_business_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let mut original = request("draft-write-raw", 0, "");
    let description = original
        .values
        .as_mut()
        .unwrap()
        .description
        .as_mut()
        .unwrap();
    description.text = "x😀y raw, unsubmitted".into();
    description.selection_base = 1;
    description.selection_extent = 3;
    description.composing_start = 1;
    description.composing_end = 3;
    let saved = host.save_editor_draft(&original).unwrap();
    assert!(!saved.repeated);
    assert_eq!(saved.slot.generation, 1);
    assert!(saved.slot.active);
    assert_eq!(business_revision(&host), 2);
    drop(host);

    // Missing package is intentionally sufficient for read-only inspection.
    let host = Workbench::open(&path, None).unwrap();
    let loaded = host.read_editor_draft(CARD, DRAFT).unwrap().unwrap();
    assert_eq!(loaded.slot.request.as_ref(), Some(&original));
    assert_eq!(loaded.slot.generation, 1);
    assert!(loaded.slot.active);
    let listed = host.list_editor_drafts().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].card_id, CARD);
    assert_eq!(listed[0].draft_id, DRAFT);
    assert_eq!(listed[0].generation, 1);
    assert!(listed[0].active);
    // The legacy-only page cannot decode a V2 business card. Start after it,
    // where the private journal IDs occur, to assert that none leak as V1 ideas.
    let (ordinary, _) = host.page(CARD, 100).unwrap();
    assert!(
        ordinary.is_empty(),
        "draft journal leaked into ordinary content"
    );
    let (mixed, _) = host.page_versioned("", 100).unwrap();
    assert_eq!(
        mixed.len(),
        1,
        "draft journal must remain hidden from mixed content"
    );
    match &mixed[0] {
        VersionedRecord::Tasks(card) => assert_eq!(card.id, CARD),
        VersionedRecord::Legacy(_) => panic!("expected the one business V2 card"),
    }
    drop(host);
    assert_eq!(
        business_revision(&Workbench::open(&path, Some(package())).unwrap()),
        2
    );
}

#[test]
fn historical_retry_never_restores_old_generation_and_mutated_retry_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let first = request("draft-write-one", 0, "First raw title");
    let one = host.save_editor_draft(&first).unwrap();
    assert_eq!(one.slot.generation, 1);
    let second = request("draft-write-two", 1, "Second raw title");
    let two = host.save_editor_draft(&second).unwrap();
    assert_eq!(two.slot.generation, 2);
    assert!(!two.repeated);
    let repeated = host.save_editor_draft(&first).unwrap();
    assert!(repeated.repeated);
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .request,
        Some(second.clone())
    );
    let mut changed = first.clone();
    changed
        .values
        .as_mut()
        .unwrap()
        .title
        .as_mut()
        .unwrap()
        .text = "tampered".into();
    assert!(host.save_editor_draft(&changed).is_err());
    assert!(host
        .save_editor_draft(&request("draft-stale-generation", 0, "stale"))
        .is_err());
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        2
    );
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn later_draft_generation_remains_anchored_after_business_source_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    host.save_editor_draft(&request("draft-before-source-change", 0, "one"))
        .unwrap();
    host.edit_card(
        "business-source-change",
        CARD,
        2,
        &CardAction::SetFavorite(true),
    )
    .unwrap();
    assert_eq!(business_revision(&host), 3);
    let second = request("draft-after-source-change", 1, "two");
    let saved = host.save_editor_draft(&second).unwrap();
    assert_eq!(saved.slot.generation, 2);
    assert_eq!(saved.slot.request, Some(second));
    assert_eq!(business_revision(&host), 3);
}

#[test]
fn staged_asset_is_pinned_across_restart_and_can_be_inherited_by_next_generation() {
    use morrow_workbench_host::editor_draft::model::proto::AssetSelection;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let source = directory.path().join("selected-original.bin");
    std::fs::write(&source, FILE_BYTES).unwrap();
    let mut host = seed(&path);
    let ordinary = host
        .import(CARD, "ordinary.bin", "file", &mut &[7_u8, 8][..], 2)
        .unwrap();
    let other_draft = host
        .import_editor_draft_asset(
            CARD,
            "other-draft",
            0,
            "other-draft.bin",
            "file",
            &mut &[9_u8, 10][..],
            2,
        )
        .unwrap();
    for (operation, forbidden) in [
        ("draft-claim-ordinary-import", &ordinary),
        ("draft-claim-other-draft-import", &other_draft),
    ] {
        let mut claim = request(operation, 0, "cross-scope claim");
        claim.assets.push(AssetSelection {
            origin: 2,
            asset_id: forbidden.id.clone(),
            aliases: vec![],
        });
        assert!(host.save_editor_draft(&claim).is_err());
        assert!(host.read_editor_draft(CARD, DRAFT).unwrap().is_none());
    }
    let asset = host
        .import_editor_draft_asset(
            CARD,
            DRAFT,
            0,
            "selected-original.bin",
            "file",
            &mut std::fs::File::open(&source).unwrap(),
            FILE_BYTES.len() as u64,
        )
        .unwrap();
    let mut first = request("draft-with-asset", 0, "asset selected");
    first.assets.push(AssetSelection {
        origin: 2,
        asset_id: asset.id.clone(),
        aliases: vec![],
    });
    let saved = host.save_editor_draft(&first).unwrap();
    assert_eq!(saved.slot.generation, 1);
    // The successful write retires the staging record. A still-frozen local
    // selection may keep origin 2 on the next generation; its exact prior pin
    // provides the bytes without reopening the removed source file.
    let mut second = request("draft-repeat-origin-two", 1, "asset still selected");
    second.assets.push(AssetSelection {
        origin: 2,
        asset_id: asset.id.clone(),
        aliases: vec![],
    });
    let saved = host.save_editor_draft(&second).unwrap();
    assert_eq!(saved.slot.generation, 2);
    drop(host);
    std::fs::remove_file(&source).unwrap();
    assert!(!source.exists());

    let mut host = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 2, &asset.id)
            .unwrap(),
        FILE_BYTES
    );
    let mut third = request("draft-inherit-asset", 2, "asset persisted through restart");
    third.assets.push(AssetSelection {
        origin: 3,
        asset_id: asset.id.clone(),
        aliases: vec![],
    });
    let saved = host.save_editor_draft(&third).unwrap();
    assert_eq!(saved.slot.generation, 3);
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 3, &asset.id)
            .unwrap(),
        FILE_BYTES
    );
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn discard_is_exact_idempotent_and_foreign_operation_cannot_clear_slot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    host.save_editor_draft(&request("draft-to-discard", 0, "unsafe raw title"))
        .unwrap();
    assert!(host
        .discard_editor_draft(CARD, DRAFT, 0, "discard-stale")
        .is_err());
    let mut foreign = Idea::default();
    foreign.id = "foreign-business-card".into();
    foreign.title = "foreign".into();
    foreign.category = "灵感".into();
    foreign.stage = "待整理".into();
    host.create("foreign-business-operation", foreign).unwrap();
    assert!(host
        .discard_editor_draft(CARD, DRAFT, 1, "foreign-business-operation")
        .is_err());
    let discarded = host
        .discard_editor_draft(CARD, DRAFT, 1, "discard-exact")
        .unwrap();
    assert!(!discarded.slot.active);
    assert!(!discarded.current_active);
    assert_eq!(business_revision(&host), 2);
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let repeated = host
        .discard_editor_draft(CARD, DRAFT, 1, "discard-exact")
        .unwrap();
    assert!(repeated.repeated);
    assert!(!repeated.slot.active);
    assert!(!repeated.current_active);
    let historical_save = host
        .save_editor_draft(&request("draft-to-discard", 0, "unsafe raw title"))
        .unwrap();
    assert!(historical_save.repeated);
    assert!(historical_save.slot.active); // Immutable old receipt, never restored.
    assert!(!historical_save.current_active);
    assert_eq!(historical_save.current_generation, 2);
    assert!(host
        .discard_editor_draft(CARD, DRAFT, 1, "discard-other")
        .is_err());
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn active_slot_limit_is_enforced_and_discard_frees_capacity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    for index in 0..16 {
        let mut draft = request(&format!("draft-slot-write-{index}"), 0, "");
        draft.draft_id = format!("draft-slot-{index:02}");
        let saved = host.save_editor_draft(&draft).unwrap();
        assert_eq!(saved.slot.generation, 1);
        assert!(saved.slot.active);
    }
    assert_eq!(host.list_editor_drafts().unwrap().len(), 16);
    let mut overflow = request("draft-slot-write-17", 0, "");
    overflow.draft_id = "draft-slot-17".into();
    assert!(host.save_editor_draft(&overflow).is_err());
    assert!(host
        .read_editor_draft(CARD, &overflow.draft_id)
        .unwrap()
        .is_none());
    host.discard_editor_draft(CARD, "draft-slot-00", 1, "draft-slot-discard-00")
        .unwrap();
    let saved = host.save_editor_draft(&overflow).unwrap();
    assert_eq!(saved.slot.generation, 1);
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn failed_journal_update_keeps_prior_draft_and_original_operation_retryable() {
    use morrow_workbench_host::editor_draft::model::proto::AssetSelection;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let first = request("draft-before-trigger", 0, "first raw");
    host.save_editor_draft(&first).unwrap();
    let asset = host
        .import_editor_draft_asset(
            CARD,
            DRAFT,
            1,
            "rollback.bin",
            "file",
            &mut &FILE_BYTES[..],
            FILE_BYTES.len() as u64,
        )
        .unwrap();
    let mut second = request("draft-blocked-by-trigger", 1, "second raw");
    second.assets.push(AssetSelection {
        origin: 2,
        asset_id: asset.id.clone(),
        aliases: vec![],
    });
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER review_draft_update_blocker BEFORE UPDATE OF payload ON cards
         WHEN OLD.id LIKE 'morrow-host-editor-draft-%'
         BEGIN SELECT RAISE(ABORT,'draft journal update blocked'); END;",
    )
    .unwrap();
    assert!(host.save_editor_draft(&second).is_err());
    let unchanged = host.read_editor_draft(CARD, DRAFT).unwrap().unwrap();
    assert_eq!(unchanged.slot.generation, 1);
    assert_eq!(unchanged.slot.request, Some(first));
    assert!(unchanged.slot.assets.is_empty());
    assert!(host
        .export_editor_draft_asset(CARD, DRAFT, 1, &asset.id)
        .is_err());
    assert_eq!(business_revision(&host), 2);
    sql.execute_batch("DROP TRIGGER review_draft_update_blocker;")
        .unwrap();
    let saved = host.save_editor_draft(&second).unwrap();
    assert!(!saved.repeated);
    assert_eq!(saved.slot.generation, 2);
    assert_eq!(saved.slot.request, Some(second));
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 2, &asset.id)
            .unwrap(),
        FILE_BYTES
    );
    assert_eq!(business_revision(&host), 2);
}

fn source_fields(host: &Workbench) -> Fields {
    let record = match host.read_versioned(CARD).unwrap() {
        VersionedRecord::Tasks(record) => record,
        VersionedRecord::Legacy(_) => panic!("expected migrated V2 card"),
    };
    let p = &record.properties;
    Fields {
        title: record.title,
        description: p.description.clone(),
        hypothesis: p.hypothesis.clone(),
        conclusion: p.conclusion.clone(),
        icon: p.icon as u16,
        color: p.color,
        assets: p
            .assets
            .iter()
            .map(|asset| Asset {
                id: asset.id.clone(),
                name: asset.name.clone(),
                kind: asset.kind.clone(),
                bytes: asset.bytes,
            })
            .collect(),
    }
}

#[test]
fn acknowledged_s1_history_still_authorizes_exact_predecessor_draft_and_asset_pin() {
    use morrow_workbench_host::editor_draft::model::proto::AssetSelection;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let asset = host
        .import(
            CARD,
            "s1-selected.bin",
            "file",
            &mut &FILE_BYTES[..],
            FILE_BYTES.len() as u64,
        )
        .unwrap();
    let scope = host.open_capture_scope(CARD, 2).unwrap();
    let mut s1_fields = source_fields(&host);
    s1_fields.title = "Confirmed S1".into();
    s1_fields.assets.push(asset.clone());
    let s1 = host
        .edit_card_captured(
            "confirmed-s1-original",
            CARD,
            2,
            &s1_fields,
            &scope,
            EditorSnapshot {
                title: s1_fields.title.clone(),
                description: s1_fields.description.clone(),
                hypothesis: s1_fields.hypothesis.clone(),
                conclusion: s1_fields.conclusion.clone(),
                todos: String::new(),
                aliases: vec![],
            },
        )
        .unwrap();
    assert!(!s1.repeated);
    assert_eq!(business_revision(&host), 3);
    let recovery = host.editor_recovery(CARD).unwrap().unwrap();
    assert_eq!(recovery.operation_id, "confirmed-s1-original");
    let digest = recovery.evidence_digest;
    host.acknowledge_editor_recovery(CARD, "confirmed-s1-original", &digest)
        .unwrap();
    assert!(!host.editor_recovery(CARD).unwrap().unwrap().active);

    let mut draft = request("draft-after-confirmed-s1", 0, "unfinished S2");
    draft.predecessor_operation = "confirmed-s1-original".into();
    draft.predecessor_sha256 = digest.to_vec();
    draft.assets.push(AssetSelection {
        origin: 1,
        asset_id: asset.id.clone(),
        aliases: vec![],
    });
    let saved = host.save_editor_draft(&draft).unwrap();
    assert!(!saved.repeated);
    assert_eq!(saved.slot.generation, 1);
    assert_eq!(
        CardRecord::decode(&saved.slot.source_card)
            .unwrap()
            .summary()
            .revision,
        2
    );
    assert_eq!(
        CardRecord::decode(&saved.slot.predecessor_card)
            .unwrap()
            .summary()
            .revision,
        3
    );
    assert_eq!(business_revision(&host), 3); // Draft save did not submit S2.
    drop(host);

    let host = Workbench::open(&path, None).unwrap();
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .request,
        Some(draft)
    );
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 1, &asset.id)
            .unwrap(),
        FILE_BYTES
    );
    let (mixed, _) = host.page_versioned("", 100).unwrap();
    assert_eq!(mixed.len(), 1);
}

#[test]
fn editor_draft_child_prepare() {
    use morrow_workbench_host::editor_draft::model::proto::AssetSelection;
    let Ok(path) = std::env::var("MORROW_DRAFT_CHILD_DB") else {
        return;
    };
    let path = Path::new(&path);
    let mut host = seed(path);
    let source = path.with_extension("selected-original.bin");
    std::fs::write(&source, FILE_BYTES).unwrap();
    let asset = host
        .import_editor_draft_asset(
            CARD,
            DRAFT,
            0,
            "child-selected.bin",
            "file",
            &mut std::fs::File::open(&source).unwrap(),
            FILE_BYTES.len() as u64,
        )
        .unwrap();
    let mut raw = request("draft-child-write", 0, "");
    let description = raw.values.as_mut().unwrap().description.as_mut().unwrap();
    description.text = "child x😀y unfinished".into();
    description.selection_base = 7;
    description.selection_extent = 9;
    description.composing_start = 7;
    description.composing_end = 9;
    raw.assets.push(AssetSelection {
        origin: 2,
        asset_id: asset.id.clone(),
        aliases: vec![],
    });
    host.save_editor_draft(&raw).unwrap();
    assert_eq!(business_revision(&host), 2);
    std::fs::write(path.with_extension("asset-id"), asset.id).unwrap();
    std::fs::remove_file(source).unwrap();
}

#[test]
fn fresh_os_process_reads_raw_draft_and_pinned_asset_without_package() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .arg("--exact")
        .arg("editor_draft_child_prepare")
        .arg("--nocapture")
        .env("MORROW_DRAFT_CHILD_DB", &path);
    use std::os::windows::process::CommandExt;
    child.creation_flags(0x08000000);
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let asset_id = std::fs::read_to_string(path.with_extension("asset-id")).unwrap();
    assert!(!path.with_extension("selected-original.bin").exists());
    let host = Workbench::open(&path, None).unwrap();
    let loaded = host.read_editor_draft(CARD, DRAFT).unwrap().unwrap();
    assert!(loaded.slot.active);
    assert_eq!(loaded.slot.generation, 1);
    let raw = loaded.slot.request.unwrap();
    assert!(raw
        .values
        .as_ref()
        .unwrap()
        .title
        .as_ref()
        .unwrap()
        .text
        .is_empty());
    let description = raw.values.unwrap().description.unwrap();
    assert_eq!(description.text, "child x😀y unfinished");
    assert_eq!(
        (description.selection_base, description.selection_extent),
        (7, 9)
    );
    assert_eq!(
        (description.composing_start, description.composing_end),
        (7, 9)
    );
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 1, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    assert_eq!(business_revision(&host), 2);
}

const NEW_CARD: &str = "draft-new-card";

fn new_card_request(
    operation: &str,
    expected_generation: u64,
    title: &str,
) -> morrow_workbench_host::editor_draft::model::proto::WriteRequest {
    let mut request = request(operation, expected_generation, title);
    request.card_id = NEW_CARD.into();
    request.source_kind = 1;
    request.source_revision = 0;
    request
}

#[test]
fn new_card_draft_has_no_business_side_effect_and_cannot_change_source_kind() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let first = new_card_request("new-card-draft-one", 0, "");
    assert!(host.read_versioned(NEW_CARD).is_err());
    let (before, _) = host.page_versioned("", 100).unwrap();
    assert!(before.is_empty());
    let saved = host.save_editor_draft(&first).unwrap();
    assert_eq!(saved.slot.generation, 1);
    assert!(saved.slot.source_card.is_empty());
    assert!(saved.slot.predecessor_card.is_empty());
    assert!(!saved.repeated);
    assert!(host.read_versioned(NEW_CARD).is_err());
    assert!(host.page_versioned("", 100).unwrap().0.is_empty());
    assert_eq!(host.list_editor_drafts().unwrap().len(), 1);

    // A different actor may create the formal card while the unsent draft is
    // still open; later draft generations retain their original empty source.
    let mut foreign = Idea::default();
    foreign.id = NEW_CARD.into();
    foreign.title = "Foreign formal card".into();
    foreign.category = "灵感".into();
    foreign.stage = "待整理".into();
    host.create("new-card-foreign-formal", foreign).unwrap();
    let second = new_card_request("new-card-draft-two", 1, "still unsent");
    let saved = host.save_editor_draft(&second).unwrap();
    assert_eq!(saved.slot.generation, 2);
    assert!(saved.slot.source_card.is_empty());
    let formal = host.read_versioned(NEW_CARD).unwrap();
    match formal {
        VersionedRecord::Legacy(record) => assert_eq!(record.idea.title, "Foreign formal card"),
        VersionedRecord::Tasks(_) => panic!("foreign formal card should remain legacy"),
    }
    let old = host.save_editor_draft(&first).unwrap();
    assert!(old.repeated);
    assert_eq!(old.slot.generation, 1);
    assert_eq!(old.current_generation, 2);
    assert!(old.current_active);
    let mut changed_kind = request("new-card-kind-change", 2, "wrong baseline");
    changed_kind.card_id = NEW_CARD.into();
    changed_kind.source_revision = 1;
    assert!(host.save_editor_draft(&changed_kind).is_err());
    assert_eq!(
        host.read_editor_draft(NEW_CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .request,
        Some(second)
    );

    let abandoned = host
        .discard_editor_draft(NEW_CARD, DRAFT, 2, "new-card-discard")
        .unwrap();
    assert!(!abandoned.slot.active);
    assert_eq!(abandoned.slot.generation, 3);
    let old_after_discard = host.save_editor_draft(&first).unwrap();
    assert!(old_after_discard.repeated);
    assert!(!old_after_discard.current_active);
    assert_eq!(old_after_discard.current_generation, 3);
    assert!(host
        .save_editor_draft(&new_card_request("new-card-after-discard", 3, "resurrect"))
        .is_err());
    drop(host);

    let host = Workbench::open(&path, None).unwrap();
    let restored = host.read_editor_draft(NEW_CARD, DRAFT).unwrap().unwrap();
    assert!(!restored.slot.active);
    assert!(restored.slot.source_card.is_empty());
    let (ordinary, _) = host.page_versioned("", 100).unwrap();
    assert_eq!(
        ordinary.len(),
        1,
        "private new-card journal leaked as a formal card"
    );
}

#[test]
fn new_card_first_write_rejects_existing_or_tombstoned_target_and_source_assets() {
    use morrow_workbench_host::editor_draft::model::proto::AssetSelection;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let mut existing = new_card_request("new-card-existing", 0, "unsubmitted");
    existing.card_id = CARD.into();
    assert!(host.save_editor_draft(&existing).is_err());
    assert!(host.read_editor_draft(CARD, DRAFT).unwrap().is_none());
    host.edit_card("new-card-tombstone", CARD, 2, &CardAction::Delete)
        .unwrap();
    let mut tombstoned = existing.clone();
    tombstoned.operation_id = "new-card-tombstoned".into();
    assert!(host.save_editor_draft(&tombstoned).is_err());
    assert!(host.read_editor_draft(CARD, DRAFT).unwrap().is_none());

    let mut new_card = new_card_request("new-card-source-asset", 0, "");
    new_card.assets.push(AssetSelection {
        origin: 0,
        asset_id: "forbidden-source-asset".into(),
        aliases: vec![],
    });
    assert!(morrow_workbench_host::editor_draft::model::validate_request(&new_card).is_err());
    assert!(host.save_editor_draft(&new_card).is_err());
    assert!(host.read_editor_draft(NEW_CARD, DRAFT).unwrap().is_none());
    new_card.assets[0].origin = 1;
    assert!(morrow_workbench_host::editor_draft::model::validate_request(&new_card).is_err());
    assert!(host.save_editor_draft(&new_card).is_err());
    assert!(host.read_editor_draft(NEW_CARD, DRAFT).unwrap().is_none());
}

#[test]
fn new_card_import_requires_active_journal_and_pins_selected_bytes_after_restart() {
    use morrow_workbench_host::editor_draft::model::proto::AssetSelection;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let source = directory.path().join("new-card-selected.bin");
    std::fs::write(&source, FILE_BYTES).unwrap();
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    assert!(host
        .import_editor_draft_asset(
            NEW_CARD,
            DRAFT,
            0,
            "early.bin",
            "file",
            &mut &FILE_BYTES[..],
            FILE_BYTES.len() as u64,
        )
        .is_err());
    let first = new_card_request("new-card-initial-journal", 0, "");
    host.save_editor_draft(&first).unwrap();
    assert!(host
        .import_editor_draft_asset(
            NEW_CARD,
            "other-draft",
            0,
            "cross-draft.bin",
            "file",
            &mut &FILE_BYTES[..],
            FILE_BYTES.len() as u64,
        )
        .is_err());
    let asset = host
        .import_editor_draft_asset(
            NEW_CARD,
            DRAFT,
            1,
            "new-card-selected.bin",
            "file",
            &mut std::fs::File::open(&source).unwrap(),
            FILE_BYTES.len() as u64,
        )
        .unwrap();
    let mut second = new_card_request("new-card-select-import", 1, "raw selected");
    second.assets.push(AssetSelection {
        origin: 2,
        asset_id: asset.id.clone(),
        aliases: vec!["local:new-card".into()],
    });
    let saved = host.save_editor_draft(&second).unwrap();
    assert_eq!(saved.slot.generation, 2);
    assert_eq!(saved.slot.assets.len(), 1);
    assert!(saved.slot.source_card.is_empty());
    assert!(host.read_versioned(NEW_CARD).is_err());
    drop(host);
    std::fs::remove_file(&source).unwrap();

    let mut host = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(
        host.export_editor_draft_asset(NEW_CARD, DRAFT, 2, &asset.id)
            .unwrap(),
        FILE_BYTES
    );
    let mut third = new_card_request("new-card-inherit-import", 2, "more raw");
    third.assets.push(AssetSelection {
        origin: 3,
        asset_id: asset.id.clone(),
        aliases: vec!["local:new-card".into()],
    });
    host.save_editor_draft(&third).unwrap();
    let old = host.save_editor_draft(&first).unwrap();
    assert!(old.repeated);
    assert_eq!(old.current_generation, 3);
    assert!(host.read_versioned(NEW_CARD).is_err());
    drop(host);

    let host = Workbench::open(&path, None).unwrap();
    let restored = host.read_editor_draft(NEW_CARD, DRAFT).unwrap().unwrap();
    assert_eq!(restored.slot.generation, 3);
    assert!(restored.slot.source_card.is_empty());
    assert_eq!(
        host.export_editor_draft_asset(NEW_CARD, DRAFT, 3, &asset.id)
            .unwrap(),
        FILE_BYTES
    );
    assert!(host.page_versioned("", 100).unwrap().0.is_empty());
}
