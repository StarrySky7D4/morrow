#![cfg(target_os = "windows")]
//! Real host and compiled WASM coverage for the private editor draft journal.
use morrow_core::plugin_package::Package;
use morrow_core::plugin_package::proto::{Capability, TransformHandler};
use morrow_workbench_host::editor_draft::model::proto::{AssetSelection, ParentLink, WriteRequest};
use morrow_workbench_host::{
    Workbench, capture_provenance::EditorSnapshot, versioned_record::VersionedRecord,
};
use morrow_workbench_plugin::{Asset, Idea, PACKAGE_VERSION, cards_v2::Fields};
use prost::Message;
use sha2::{Digest, Sha256};
use std::path::Path;

const CARD: &str = "draft-card";
const DRAFT: &str = "draft-one";
const CHILD: &str = "draft-child";
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

fn parent_with_asset(host: &mut Workbench) -> (WriteRequest, String) {
    let asset = host
        .import_editor_draft_asset(
            CARD,
            DRAFT,
            0,
            "unfinished-only.bin",
            "file",
            &mut &FILE_BYTES[..],
            FILE_BYTES.len() as u64,
        )
        .unwrap();
    let mut parent = request("parent-s2-save", 0, "Still unfinished S2");
    parent.assets.push(AssetSelection {
        origin: 2,
        asset_id: asset.id.clone(),
        aliases: vec!["unfinished-only.bin".into()],
    });
    let saved = host.save_editor_draft(&parent).unwrap();
    assert_eq!(saved.slot.generation, 1);
    assert_eq!(business_revision(host), 2);
    (parent, asset.id)
}

fn commit_real_s1(host: &mut Workbench) -> [u8; 32] {
    let scope = host.open_capture_scope(CARD, 2).unwrap();
    let mut fields = source_fields(host);
    fields.title = "Confirmed S1".into();
    let result = host
        .edit_card_captured(
            "captured-s1-for-handoff",
            CARD,
            2,
            &fields,
            &scope,
            EditorSnapshot {
                title: fields.title.clone(),
                description: fields.description.clone(),
                hypothesis: fields.hypothesis.clone(),
                conclusion: fields.conclusion.clone(),
                todos: String::new(),
                aliases: vec![],
            },
        )
        .unwrap();
    assert!(!result.repeated);
    assert_eq!(business_revision(host), 3);
    let evidence = host.editor_recovery(CARD).unwrap().unwrap();
    assert_eq!(evidence.operation_id, "captured-s1-for-handoff");
    assert_eq!(
        evidence.status,
        morrow_workbench_host::editor_recovery::EditorRecoveryStatus::Committed
    );
    let digest = evidence.evidence_digest;
    host.acknowledge_editor_recovery(CARD, "captured-s1-for-handoff", &digest)
        .unwrap();
    assert!(!host.editor_recovery(CARD).unwrap().unwrap().active);
    digest
}

fn child_from(parent: &WriteRequest, asset_id: &str) -> WriteRequest {
    let mut child = request("child-handoff-save", 0, "unused");
    child.draft_id = CHILD.into();
    child.source_revision = 3;
    child.values = parent.values.clone();
    child.assets = parent
        .assets
        .iter()
        .map(|selected| AssetSelection {
            origin: 4,
            asset_id: selected.asset_id.clone(),
            aliases: selected.aliases.clone(),
        })
        .collect();
    assert_eq!(child.assets.len(), 1);
    assert_eq!(child.assets[0].asset_id, asset_id);
    child
}

fn link(parent: &WriteRequest, child: &WriteRequest, digest: &[u8; 32]) -> ParentLink {
    ParentLink {
        parent_draft_id: parent.draft_id.clone(),
        parent_generation: parent.expected_generation + 1,
        parent_save_operation: parent.operation_id.clone(),
        parent_request_sha256: Sha256::digest(parent.encode_to_vec()).to_vec(),
        committed_operation: "captured-s1-for-handoff".into(),
        committed_sha256: digest.to_vec(),
        child_operation: child.operation_id.clone(),
    }
}
#[test]
fn captured_s1_handoff_retains_parent_pins_across_restart_and_retires_conditionally() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset_id) = parent_with_asset(&mut host);
    let evidence_digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset_id);
    let binding = link(&parent, &child, &evidence_digest);

    // A normal save cannot interpret origin 4 or create a cross-draft pin.
    assert!(host.save_editor_draft(&child).is_err());
    let handed = host.handoff_editor_draft(&child, &binding).unwrap();
    assert!(!handed.repeated);
    assert_eq!(handed.slot.generation, 1);
    assert_eq!(handed.slot.parent_link.as_ref(), Some(&binding));
    assert_eq!(business_revision(&host), 3);
    drop(host);

    let readonly = Workbench::open(&path, None).unwrap();
    let parent_live = readonly.read_editor_draft(CARD, DRAFT).unwrap().unwrap();
    let child_live = readonly.read_editor_draft(CARD, CHILD).unwrap().unwrap();
    assert!(parent_live.slot.active);
    assert!(child_live.slot.active);
    assert_eq!(child_live.slot.parent_link.as_ref(), Some(&binding));
    let lineage = readonly.list_editor_draft_lineages().unwrap();
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0].card_id, CARD);
    assert_eq!(lineage[0].child_draft_id, CHILD);
    assert_eq!(lineage[0].parent_generation, 1);
    assert!(lineage[0].parent_active);
    assert_eq!(lineage[0].child_generation, 1);
    assert!(lineage[0].child_active);
    assert_eq!(lineage[0].link, binding);
    assert_eq!(
        readonly
            .export_editor_draft_asset(CARD, CHILD, 1, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    drop(readonly);

    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let mut forbidden_parent = request("parent-late-save", 1, "Must remain parent");
    forbidden_parent.assets = vec![AssetSelection {
        origin: 3,
        asset_id: asset_id.clone(),
        aliases: parent.assets[0].aliases.clone(),
    }];
    assert!(host.save_editor_draft(&forbidden_parent).is_err());
    let historical_parent = host.save_editor_draft(&parent).unwrap();
    assert!(historical_parent.repeated);
    assert_eq!(historical_parent.slot.generation, 1);
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        1
    );

    // Ordinary discard, even using the later retirement operation ID, cannot
    // impersonate the child-bound conditional retirement.
    assert!(
        host.discard_editor_draft(CARD, DRAFT, 1, "conditional-retire-parent")
            .is_err()
    );
    let retired = host
        .retire_editor_draft_parent(CARD, CHILD, "conditional-retire-parent")
        .unwrap();
    assert!(!retired.slot.active);
    assert_eq!(retired.slot.generation, 2);
    assert_eq!(business_revision(&host), 3);
    let lineage = host.list_editor_draft_lineages().unwrap();
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0].parent_generation, 2);
    assert!(!lineage[0].parent_active);
    assert_eq!(lineage[0].child_generation, 1);
    assert!(lineage[0].child_active);
    assert_eq!(lineage[0].link, binding);
    drop(host);

    let mut host = Workbench::open(&path, Some(package())).unwrap();
    assert!(
        !host
            .read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    assert_eq!(
        host.export_editor_draft_asset(CARD, CHILD, 1, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    let mut next = child.clone();
    next.operation_id = "child-next-save".into();
    next.expected_generation = 1;
    next.assets[0].origin = 3;
    next.values.as_mut().unwrap().title.as_mut().unwrap().text =
        "Next unfinished generation".into();
    let saved = host.save_editor_draft(&next).unwrap();
    assert_eq!(saved.slot.generation, 2);
    let lineage = host.list_editor_draft_lineages().unwrap();
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0].parent_generation, 2);
    assert!(!lineage[0].parent_active);
    assert_eq!(lineage[0].child_generation, 2);
    assert!(lineage[0].child_active);
    assert_eq!(lineage[0].link, binding);
    assert_eq!(
        host.export_editor_draft_asset(CARD, CHILD, 2, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    let old_retry = host.handoff_editor_draft(&child, &binding).unwrap();
    assert!(old_retry.repeated);
    assert_eq!(old_retry.slot.generation, 1);
    assert_eq!(old_retry.current_generation, 2);
    assert_eq!(
        host.read_editor_draft(CARD, CHILD)
            .unwrap()
            .unwrap()
            .slot
            .request,
        Some(next)
    );
}
#[test]
fn handoff_rejects_uncommitted_or_changed_evidence_and_incomplete_child() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset_id) = parent_with_asset(&mut host);
    let child = child_from(&parent, &asset_id);
    let uncommitted = link(&parent, &child, &[1; 32]);
    assert!(host.handoff_editor_draft(&child, &uncommitted).is_err());
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());

    let digest = commit_real_s1(&mut host);
    let valid = link(&parent, &child, &digest);
    let mut wrong_evidence = valid.clone();
    wrong_evidence.committed_sha256[0] ^= 1;
    assert!(host.handoff_editor_draft(&child, &wrong_evidence).is_err());
    let mut wrong_parent = valid.clone();
    wrong_parent.parent_request_sha256[0] ^= 1;
    assert!(host.handoff_editor_draft(&child, &wrong_parent).is_err());
    let mut wrong_generation = valid.clone();
    wrong_generation.parent_generation += 1;
    assert!(
        host.handoff_editor_draft(&child, &wrong_generation)
            .is_err()
    );

    let mut changed_text = child.clone();
    changed_text
        .values
        .as_mut()
        .unwrap()
        .description
        .as_mut()
        .unwrap()
        .text = "S2 lost raw input".into();
    assert!(host.handoff_editor_draft(&changed_text, &valid).is_err());
    let mut missing_attachment = child.clone();
    missing_attachment.assets.clear();
    assert!(
        host.handoff_editor_draft(&missing_attachment, &valid)
            .is_err()
    );
    let mut changed_alias = child.clone();
    changed_alias.assets[0].aliases.push("extra".into());
    assert!(host.handoff_editor_draft(&changed_alias, &valid).is_err());
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 1, &asset_id)
            .unwrap(),
        FILE_BYTES
    );

    host.handoff_editor_draft(&child, &valid).unwrap();
    let mut competitor = child.clone();
    competitor.draft_id = "competing-child".into();
    competitor.operation_id = "competing-handoff".into();
    let competing_link = link(&parent, &competitor, &digest);
    assert!(
        host.handoff_editor_draft(&competitor, &competing_link)
            .is_err()
    );
    assert!(
        host.read_editor_draft(CARD, "competing-child")
            .unwrap()
            .is_none()
    );
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    assert_eq!(business_revision(&host), 3);
}

#[test]
fn parent_generation_drift_rejects_stale_handoff_without_retiring_parent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset_id) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset_id);
    let stale = link(&parent, &child, &digest);

    let mut newer_parent = request("parent-after-s1", 1, "Newer pending S2");
    newer_parent.assets.push(AssetSelection {
        origin: 3,
        asset_id: asset_id.clone(),
        aliases: parent.assets[0].aliases.clone(),
    });
    assert_eq!(
        host.save_editor_draft(&newer_parent)
            .unwrap()
            .slot
            .generation,
        2
    );
    assert!(host.handoff_editor_draft(&child, &stale).is_err());
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    let current = host.read_editor_draft(CARD, DRAFT).unwrap().unwrap();
    assert!(current.slot.active);
    assert_eq!(current.slot.generation, 2);
    assert_eq!(current.slot.request, Some(newer_parent));
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 2, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    assert_eq!(business_revision(&host), 3);
}
#[test]
fn new_card_captured_create_hands_off_complete_s2_to_existing_child() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();

    let mut first = request("new-parent-initial", 0, "Unfinished new card");
    first.source_kind = 1;
    first.source_revision = 0;
    host.save_editor_draft(&first).unwrap();
    let asset = host
        .import_editor_draft_asset(
            CARD,
            DRAFT,
            1,
            "new-card-only.bin",
            "file",
            &mut &FILE_BYTES[..],
            FILE_BYTES.len() as u64,
        )
        .unwrap();
    let mut parent = request("new-parent-with-asset", 1, "New card S2 text");
    parent.source_kind = 1;
    parent.source_revision = 0;
    parent.assets.push(AssetSelection {
        origin: 2,
        asset_id: asset.id.clone(),
        aliases: vec!["new-card-only.bin".into()],
    });
    assert_eq!(host.save_editor_draft(&parent).unwrap().slot.generation, 2);
    assert!(host.read_versioned(CARD).is_err());

    let scope = host.open_capture_scope(CARD, 0).unwrap();
    let created = Idea {
        id: CARD.into(),
        title: "Confirmed new S1".into(),
        description: "First committed content".into(),
        category: "进行中".into(),
        stage: "计划中".into(),
        ..Default::default()
    };
    host.create_captured(
        "newcard-captured-s1",
        created.clone(),
        &scope,
        EditorSnapshot {
            title: created.title.clone(),
            description: created.description.clone(),
            hypothesis: created.hypothesis.clone(),
            conclusion: created.conclusion.clone(),
            todos: String::new(),
            aliases: vec![],
        },
    )
    .unwrap();
    assert_eq!(host.read(CARD).unwrap().revision, 1);
    let evidence = host
        .operation_evidence(CARD, "newcard-captured-s1")
        .unwrap();
    assert_eq!(evidence.len(), 1);
    let digest = evidence[0].digest();

    let mut child = child_from(&parent, &asset.id);
    child.source_revision = 1;
    let mut binding = link(&parent, &child, &digest);
    binding.committed_operation = "newcard-captured-s1".into();
    let handed = host.handoff_editor_draft(&child, &binding).unwrap();
    assert_eq!(handed.slot.parent_link.as_ref(), Some(&binding));
    assert_eq!(handed.slot.request.as_ref().unwrap().source_kind, 0);
    assert_eq!(handed.slot.request.as_ref().unwrap().source_revision, 1);
    drop(host);

    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let lineage = host.list_editor_draft_lineages().unwrap();
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0].card_id, CARD);
    assert_eq!(lineage[0].child_draft_id, CHILD);
    assert_eq!(lineage[0].parent_generation, 2);
    assert!(lineage[0].parent_active && lineage[0].child_active);
    assert_eq!(lineage[0].link, binding);
    assert_eq!(
        host.export_editor_draft_asset(CARD, CHILD, 1, &asset.id)
            .unwrap(),
        FILE_BYTES
    );
    assert_eq!(
        host.retire_editor_draft_parent(CARD, CHILD, "new-parent-retire")
            .unwrap()
            .slot
            .generation,
        3
    );
    assert_eq!(host.read(CARD).unwrap().revision, 1);
    assert!(!host.list_editor_draft_lineages().unwrap()[0].parent_active);
}

#[test]
fn legacy_v1_captured_edit_hands_off_exact_parent_snapshot() {
    use morrow_workbench_host::Mutation;
    use morrow_workbench_plugin::Action;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    host.create(
        "legacy-seed",
        Idea {
            id: CARD.into(),
            title: "Original V1".into(),
            description: "Original body".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(host.read(CARD).unwrap().revision, 1);

    let mut parent = request("legacy-parent-s2", 0, "Unfinished legacy S2");
    parent.source_revision = 1;
    assert_eq!(host.save_editor_draft(&parent).unwrap().slot.generation, 1);
    let scope = host.open_capture_scope(CARD, 1).unwrap();
    let mut proposed = host.read(CARD).unwrap().idea;
    proposed.title = "Confirmed V1 S1".into();
    let snapshot = EditorSnapshot {
        title: proposed.title.clone(),
        description: proposed.description.clone(),
        hypothesis: proposed.hypothesis.clone(),
        conclusion: proposed.conclusion.clone(),
        todos: proposed.todos.join("\n"),
        aliases: vec![],
    };
    host.apply_captured(
        Mutation {
            operation: "legacy-captured-s1",
            id: CARD,
            revision: 1,
            action: Action::Edit,
            proposed: Some(proposed),
            text: "",
            flag: false,
        },
        &scope,
        snapshot,
    )
    .unwrap();
    assert_eq!(host.read(CARD).unwrap().revision, 2);
    let evidence = host.operation_evidence(CARD, "legacy-captured-s1").unwrap();
    assert_eq!(evidence.len(), 1);
    let digest = evidence[0].digest();

    let mut child = request("legacy-child-handoff", 0, "unused");
    child.draft_id = CHILD.into();
    child.source_revision = 2;
    child.values = parent.values.clone();
    let mut binding = link(&parent, &child, &digest);
    binding.committed_operation = "legacy-captured-s1".into();
    let saved = host.handoff_editor_draft(&child, &binding).unwrap();
    assert_eq!(saved.slot.request.as_ref().unwrap().source_kind, 0);
    assert_eq!(saved.slot.request.as_ref().unwrap().source_revision, 2);
    assert_eq!(saved.slot.parent_link.as_ref(), Some(&binding));
    drop(host);

    let host = Workbench::open(&path, None).unwrap();
    let lineage = host.list_editor_draft_lineages().unwrap();
    assert_eq!(lineage.len(), 1);
    assert!(lineage[0].parent_active && lineage[0].child_active);
    assert_eq!(lineage[0].link, binding);
    assert_eq!(
        host.read_editor_draft(CARD, CHILD)
            .unwrap()
            .unwrap()
            .slot
            .request,
        Some(child)
    );
}

#[test]
fn child_discard_requires_retired_parent_and_cannot_authorize_fresh_retirement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset_id) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset_id);
    let binding = link(&parent, &child, &digest);
    host.handoff_editor_draft(&child, &binding).unwrap();

    // Both snapshots remain available until the child-bound parent retirement.
    assert!(
        host.discard_editor_draft(CARD, CHILD, 1, "premature-child-discard")
            .is_err()
    );
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    assert!(
        host.read_editor_draft(CARD, CHILD)
            .unwrap()
            .unwrap()
            .slot
            .active
    );

    let retirement = host
        .retire_editor_draft_parent(CARD, CHILD, "conditional-retire-parent")
        .unwrap();
    assert!(!retirement.slot.active);
    assert_eq!(
        host.discard_editor_draft(CARD, CHILD, 1, "discard-child")
            .unwrap()
            .slot
            .generation,
        2
    );
    let exact_retirement = host
        .retire_editor_draft_parent(CARD, CHILD, "conditional-retire-parent")
        .unwrap();
    assert!(exact_retirement.repeated);
    assert!(
        host.retire_editor_draft_parent(CARD, CHILD, "late-parent-retirement")
            .is_err()
    );
    let historical = host.handoff_editor_draft(&child, &binding).unwrap();
    assert!(historical.repeated);
    assert_eq!(historical.slot.generation, 1);
    assert_eq!(historical.current_generation, 2);
    let mut changed_link = binding.clone();
    changed_link.committed_sha256[0] ^= 1;
    assert!(host.handoff_editor_draft(&child, &changed_link).is_err());
    let lineage = host.list_editor_draft_lineages().unwrap();
    assert_eq!(lineage.len(), 1);
    assert!(!lineage[0].parent_active);
    assert!(!lineage[0].child_active);
    assert_eq!(lineage[0].child_generation, 2);
    assert_eq!(lineage[0].parent_generation, 2);
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .parent_link
            .is_none()
    );
    assert_eq!(business_revision(&host), 3);
}
#[test]
fn full_active_slot_budget_rejects_handoff_without_changing_parent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset_id) = parent_with_asset(&mut host);
    for i in 0..15 {
        let mut filler = request(&format!("filler-save-{i}"), 0, "Independent pending");
        filler.draft_id = format!("filler-draft-{i}");
        host.save_editor_draft(&filler).unwrap();
    }
    assert_eq!(host.list_editor_drafts().unwrap().len(), 16);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset_id);
    let binding = link(&parent, &child, &digest);
    assert!(host.handoff_editor_draft(&child, &binding).is_err());
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    assert_eq!(host.list_editor_drafts().unwrap().len(), 16);
    let remaining = host.read_editor_draft(CARD, DRAFT).unwrap().unwrap();
    assert!(remaining.slot.active);
    assert_eq!(remaining.slot.generation, 1);
    assert_eq!(remaining.slot.request, Some(parent));
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 1, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    assert!(host.list_editor_draft_lineages().unwrap().is_empty());
}

#[test]
fn failed_child_transaction_preserves_parent_and_retries_exactly_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset_id) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset_id);
    let binding = link(&parent, &child, &digest);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER reject_child BEFORE INSERT ON cards
        WHEN NEW.id LIKE 'morrow-host-editor-draft-%'
        BEGIN SELECT RAISE(ABORT, 'child write failure'); END;",
    )
    .unwrap();
    assert!(host.handoff_editor_draft(&child, &binding).is_err());
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    assert!(host.list_editor_draft_lineages().unwrap().is_empty());
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 1, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .request
            .as_ref(),
        Some(&parent)
    );
    sql.execute_batch("DROP TRIGGER reject_child;").unwrap();
    drop(sql);
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let saved = host.handoff_editor_draft(&child, &binding).unwrap();
    assert_eq!(saved.slot.generation, 1);
    assert_eq!(
        host.export_editor_draft_asset(CARD, CHILD, 1, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
}

#[test]
fn failed_parent_retirement_leaves_both_snapshots_and_exact_retry_survives_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset_id) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset_id);
    let binding = link(&parent, &child, &digest);
    host.handoff_editor_draft(&child, &binding).unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER reject_retirement BEFORE UPDATE ON cards
        WHEN OLD.id LIKE 'morrow-host-editor-draft-%'
        BEGIN SELECT RAISE(ABORT, 'parent retirement failure'); END;",
    )
    .unwrap();
    assert!(
        host.retire_editor_draft_parent(CARD, CHILD, "retire-with-failure")
            .is_err()
    );
    let lineage = host.list_editor_draft_lineages().unwrap();
    assert!(lineage[0].parent_active && lineage[0].child_active);
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 1, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    assert_eq!(
        host.export_editor_draft_asset(CARD, CHILD, 1, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    sql.execute_batch("DROP TRIGGER reject_retirement;")
        .unwrap();
    drop(sql);
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    assert!(
        !host
            .retire_editor_draft_parent(CARD, CHILD, "retire-with-failure")
            .unwrap()
            .slot
            .active
    );
    assert!(
        host.retire_editor_draft_parent(CARD, CHILD, "retire-with-failure")
            .unwrap()
            .repeated
    );
    assert_eq!(
        host.export_editor_draft_asset(CARD, CHILD, 1, &asset_id)
            .unwrap(),
        FILE_BYTES
    );
    assert!(
        host.discard_editor_draft(CARD, DRAFT, 1, "retire-with-failure")
            .is_err()
    );
}

#[test]
fn consumed_durable_import_pin_is_inherited_without_reopening_selected_source() {
    use morrow_workbench_host::editor_draft_staging::proto::ImportRequest;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let initial = request("empty-parent-before-durable", 0, "S2 with durable bytes");
    host.save_editor_draft(&initial).unwrap();
    let imported = ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: "durable-parent-import".into(),
        expected_generation: 1,
        name: "durable-only.bin".into(),
        kind: "file".into(),
        byte_length: FILE_BYTES.len() as u64,
        sha256: Sha256::digest(FILE_BYTES).to_vec(),
    };
    let file = directory.path().join("original.bin");
    std::fs::write(&file, FILE_BYTES).unwrap();
    let ready = host
        .import_editor_draft_asset_durable(&imported, &mut std::fs::File::open(&file).unwrap())
        .unwrap();
    std::fs::remove_file(file).unwrap();
    let mut parent = request("select-durable-parent", 1, "S2 with durable bytes");
    parent.assets.push(AssetSelection {
        origin: 2,
        asset_id: ready.asset_id.clone(),
        aliases: vec!["original".into()],
    });
    let selected = host.save_editor_draft(&parent).unwrap();
    assert_eq!(selected.slot.consumed_imports, vec![imported.operation_id]);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &ready.asset_id);
    let binding = link(&parent, &child, &digest);
    host.handoff_editor_draft(&child, &binding).unwrap();
    host.retire_editor_draft_parent(CARD, CHILD, "retire-durable-parent")
        .unwrap();
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    drop(host);
    let host = Workbench::open(&path, None).unwrap();
    assert_eq!(
        host.export_editor_draft_asset(CARD, CHILD, 1, &ready.asset_id)
            .unwrap(),
        FILE_BYTES
    );
    assert_eq!(
        host.read_editor_draft(CARD, CHILD)
            .unwrap()
            .unwrap()
            .slot
            .parent_link,
        Some(binding)
    );
}
