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

fn durable_request(
    operation: &str,
    bytes: &[u8],
) -> morrow_workbench_host::editor_draft_staging::proto::ImportRequest {
    use morrow_workbench_host::editor_draft_staging::proto::ImportRequest;
    ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: operation.into(),
        expected_generation: 1,
        name: "unselected.bin".into(),
        kind: "file".into(),
        byte_length: bytes.len() as u64,
        sha256: Sha256::digest(bytes).to_vec(),
    }
}

fn commit_next_s2(host: &mut Workbench) -> [u8; 32] {
    let scope = host.open_capture_scope(CARD, 3).unwrap();
    let mut fields = source_fields(host);
    fields.title = "Confirmed second edit".into();
    let result = host
        .edit_card_captured(
            "captured-s2-for-grandchild",
            CARD,
            3,
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
    assert_eq!(business_revision(host), 4);
    let recovery = host.editor_recovery(CARD).unwrap().unwrap();
    let digest = recovery.evidence_digest;
    host.acknowledge_editor_recovery(CARD, "captured-s2-for-grandchild", &digest)
        .unwrap();
    digest
}

#[test]
fn unresolved_parent_imports_block_handoff_and_successor_freezes_new_imports() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset_id) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset_id);
    let binding = link(&parent, &child, &digest);

    let pending = durable_request("unselected-pending-import", b"pending bytes");
    host.begin_editor_draft_import(&pending).unwrap();
    assert!(host.handoff_editor_draft(&child, &binding).is_err());
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    host.abandon_editor_draft_import(
        CARD,
        DRAFT,
        1,
        &pending.operation_id,
        "abandon-pending-import",
    )
    .unwrap();

    let ready = durable_request("unselected-ready-import", b"ready bytes");
    host.import_editor_draft_asset_durable(&ready, &mut &b"ready bytes"[..])
        .unwrap();
    assert!(host.handoff_editor_draft(&child, &binding).is_err());
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    host.abandon_editor_draft_import(CARD, DRAFT, 1, &ready.operation_id, "abandon-ready-import")
        .unwrap();

    assert_eq!(
        host.handoff_editor_draft(&child, &binding)
            .unwrap()
            .slot
            .generation,
        1
    );
    assert_eq!(business_revision(&host), 3);
    assert!(
        host.import_editor_draft_asset(
            CARD,
            DRAFT,
            1,
            "late-legacy.bin",
            "file",
            &mut &b"late legacy"[..],
            b"late legacy".len() as u64,
        )
        .is_err()
    );
    let late = durable_request("late-durable-import", b"late durable");
    assert!(host.begin_editor_draft_import(&late).is_err());
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
}

#[test]
fn linked_child_cannot_handoff_again_until_ancestor_is_retired() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset_id) = parent_with_asset(&mut host);
    let s1_digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset_id);
    let first_link = link(&parent, &child, &s1_digest);
    host.handoff_editor_draft(&child, &first_link).unwrap();

    let s2_digest = commit_next_s2(&mut host);
    let mut grandchild = child_from(&child, &asset_id);
    grandchild.draft_id = "draft-grandchild".into();
    grandchild.operation_id = "grandchild-handoff-save".into();
    grandchild.source_revision = 4;
    let mut second_link = link(&child, &grandchild, &s2_digest);
    second_link.committed_operation = "captured-s2-for-grandchild".into();

    assert!(
        host.handoff_editor_draft(&grandchild, &second_link)
            .is_err()
    );
    assert!(
        host.read_editor_draft(CARD, "draft-grandchild")
            .unwrap()
            .is_none()
    );
    assert_eq!(business_revision(&host), 4);
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );

    host.retire_editor_draft_parent(CARD, CHILD, "retire-ancestor-before-next-handoff")
        .unwrap();
    assert_eq!(
        host.handoff_editor_draft(&grandchild, &second_link)
            .unwrap()
            .slot
            .generation,
        1
    );
    assert_eq!(business_revision(&host), 4);
    assert!(
        host.read_editor_draft(CARD, "draft-grandchild")
            .unwrap()
            .unwrap()
            .slot
            .active
    );
}

#[test]
fn same_s1_predecessor_handoff_uses_source_revision_n_plus_one() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let digest = commit_real_s1(&mut host);
    let mut parent = request("parent-projected-s1-save", 0, "Unfinished after S1");
    parent.predecessor_operation = "captured-s1-for-handoff".into();
    parent.predecessor_sha256 = digest.to_vec();
    assert_eq!(host.save_editor_draft(&parent).unwrap().slot.generation, 1);

    let mut child = request("child-from-same-s1", 0, "unused");
    child.draft_id = CHILD.into();
    child.source_revision = 3;
    child.values = parent.values.clone();
    let binding = link(&parent, &child, &digest);
    assert_eq!(business_revision(&host), 3);
    let handed = host.handoff_editor_draft(&child, &binding).unwrap();
    assert_eq!(handed.slot.generation, 1);
    assert_eq!(handed.slot.request.as_ref().unwrap().source_revision, 3);
    assert_eq!(business_revision(&host), 3);
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
}
