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

use morrow_workbench_host::editor_draft::HandoffProposalStatus as Status;
const RETIRE: &str = "prepared-parent-retirement";

#[test]
fn immutable_proposal_freezes_parent_and_cancellation_cannot_revive_original() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset);
    let binding = link(&parent, &child, &digest);
    let proposal = host
        .prepare_editor_draft_handoff_proposal(&child, &binding, RETIRE)
        .unwrap();
    assert_eq!(proposal.status, Status::Pending);
    assert_eq!(proposal.request, child);
    assert_eq!(proposal.parent_link, binding);
    assert_eq!(proposal.retirement_operation, RETIRE);
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    assert_eq!(business_revision(&host), 3);
    assert_eq!(
        host.prepare_editor_draft_handoff_proposal(&child, &binding, RETIRE)
            .unwrap()
            .status,
        Status::Pending
    );
    assert!(
        host.prepare_editor_draft_handoff_proposal(&child, &binding, "changed-retirement")
            .is_err()
    );
    let mut changed = child.clone();
    changed
        .values
        .as_mut()
        .unwrap()
        .title
        .as_mut()
        .unwrap()
        .text
        .push('!');
    assert!(
        host.prepare_editor_draft_handoff_proposal(&changed, &binding, RETIRE)
            .is_err()
    );
    let mut competitor = child.clone();
    competitor.draft_id = "other-child".into();
    competitor.operation_id = "other-child-operation".into();
    let other_link = link(&parent, &competitor, &digest);
    assert!(
        host.prepare_editor_draft_handoff_proposal(&competitor, &other_link, "other-retirement")
            .is_err()
    );
    assert!(host.handoff_editor_draft(&competitor, &other_link).is_err());
    assert!(
        host.import_editor_draft_asset(
            CARD,
            CHILD,
            0,
            "reserved-child.bin",
            "file",
            &mut &FILE_BYTES[..],
            FILE_BYTES.len() as u64
        )
        .is_err()
    );

    let mut next = parent.clone();
    next.expected_generation = 1;
    next.operation_id = "parent-while-prepared".into();
    next.assets[0].origin = 3;
    assert!(host.save_editor_draft(&next).is_err());
    assert!(
        host.discard_editor_draft(CARD, DRAFT, 1, "discard-prepared-parent")
            .is_err()
    );
    assert!(
        host.import_editor_draft_asset(
            CARD,
            DRAFT,
            1,
            "late.bin",
            "file",
            &mut &FILE_BYTES[..],
            FILE_BYTES.len() as u64
        )
        .is_err()
    );
    let imported = morrow_workbench_host::editor_draft_staging::proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: "late-durable-import".into(),
        expected_generation: 1,
        name: "late.bin".into(),
        kind: "file".into(),
        byte_length: FILE_BYTES.len() as u64,
        sha256: Sha256::digest(FILE_BYTES).to_vec(),
    };
    assert!(host.begin_editor_draft_import(&imported).is_err());
    assert!(
        host.inspect_editor_draft_import(CARD, DRAFT, &imported.operation_id)
            .unwrap()
            .is_none()
    );
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();

    assert!(host.save_editor_draft(&parent).unwrap().repeated);
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 1, &asset)
            .unwrap(),
        FILE_BYTES
    );
    assert_eq!(
        host.cancel_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .status,
        Status::Cancelled
    );
    assert_eq!(
        host.cancel_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .status,
        Status::Cancelled
    );
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    assert_eq!(
        host.inspect_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .unwrap()
            .status,
        Status::Cancelled
    );
    assert!(
        host.complete_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .is_err()
    );
    assert!(host.handoff_editor_draft(&child, &binding).is_err());
    assert_eq!(
        host.prepare_editor_draft_handoff_proposal(&child, &binding, RETIRE)
            .unwrap()
            .status,
        Status::Cancelled
    );
    assert_eq!(host.save_editor_draft(&next).unwrap().slot.generation, 2);
}

#[test]
fn committed_child_and_fixed_retirement_are_recovered_without_duplicate_business() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset);
    let binding = link(&parent, &child, &digest);
    host.prepare_editor_draft_handoff_proposal(&child, &binding, RETIRE)
        .unwrap();
    assert_eq!(
        host.complete_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .status,
        Status::ChildCommitted
    );
    assert!(
        host.cancel_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .is_err()
    );
    assert!(
        host.retire_editor_draft_parent(CARD, CHILD, "different-retirement")
            .is_err()
    );
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    let found = host
        .inspect_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(found.status, Status::ChildCommitted);
    assert_eq!(found.request, child);
    assert_eq!(found.parent_link, binding);
    assert_eq!(found.retirement_operation, RETIRE);
    assert_eq!(
        host.complete_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .status,
        Status::ChildCommitted
    );
    assert_eq!(
        host.retire_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .status,
        Status::ParentRetired
    );
    let old = host.read_editor_draft(CARD, DRAFT).unwrap().unwrap();
    assert_eq!(old.slot.retirement.unwrap().operation_id, RETIRE);
    assert_eq!(
        host.export_editor_draft_asset(CARD, CHILD, 1, &asset)
            .unwrap(),
        FILE_BYTES
    );
    let mut later = child.clone();
    later.expected_generation = 1;
    later.operation_id = "child-later-save".into();
    later.assets[0].origin = 3;
    host.save_editor_draft(&later).unwrap();
    host.discard_editor_draft(CARD, CHILD, 2, "child-later-discard")
        .unwrap();
    assert_eq!(
        host.retire_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .status,
        Status::ParentRetired
    );
    assert_eq!(
        host.complete_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .status,
        Status::ParentRetired
    );
    assert_eq!(business_revision(&host), 3);
}

#[test]
fn failed_child_commit_preserves_durable_proposal_and_parent_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset);
    host.prepare_editor_draft_handoff_proposal(&child, &link(&parent, &child, &digest), RETIRE)
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_child BEFORE INSERT ON cards WHEN NEW.id LIKE 'morrow-host-editor-draft-%' BEGIN SELECT RAISE(ABORT, 'child write failure'); END;").unwrap();
    assert!(
        host.complete_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .is_err()
    );
    assert_eq!(
        host.inspect_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .unwrap()
            .status,
        Status::Pending
    );
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 1, &asset)
            .unwrap(),
        FILE_BYTES
    );
    sql.execute_batch("DROP TRIGGER reject_child;").unwrap();
    drop(sql);
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    assert_eq!(
        host.complete_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .status,
        Status::ChildCommitted
    );
}

#[test]
fn proposal_survives_independent_processes_without_callers_original_request() {
    if let Ok(phase) = std::env::var("MORROW_HANDOFF_PROPOSAL_PHASE") {
        let path =
            std::path::PathBuf::from(std::env::var_os("MORROW_HANDOFF_PROPOSAL_DB").unwrap());
        if phase == "prepare" {
            let mut host = seed(&path);
            let (parent, asset) = parent_with_asset(&mut host);
            let digest = commit_real_s1(&mut host);
            let child = child_from(&parent, &asset);
            host.prepare_editor_draft_handoff_proposal(
                &child,
                &link(&parent, &child, &digest),
                RETIRE,
            )
            .unwrap();
            // Abrupt process exit: no proposal bytes are passed to the next client.
            std::process::exit(73);
        }
        let mut host = Workbench::open(&path, None).unwrap();
        let (records, next) = host.list_editor_draft_handoff_proposals("", 1).unwrap();
        assert_eq!(records.len(), 1);
        assert!(next.is_none());
        let recovered = &records[0];
        assert_eq!(
            recovered
                .request
                .values
                .as_ref()
                .unwrap()
                .title
                .as_ref()
                .unwrap()
                .text,
            "Still unfinished S2"
        );
        assert_eq!(recovered.retirement_operation, RETIRE);
        if phase == "commit" {
            assert_eq!(recovered.status, Status::Pending);
            assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
            host.complete_editor_draft_handoff_proposal(
                &recovered.request.card_id,
                &recovered.parent_link.parent_draft_id,
                &recovered.request.operation_id,
            )
            .unwrap();
            std::process::exit(74); // Child durable; caller never retains its receipt.
        }
        assert_eq!(recovered.status, Status::ChildCommitted);
        let asset = &recovered.request.assets[0].asset_id;
        assert_eq!(
            host.export_editor_draft_asset(CARD, CHILD, 1, asset)
                .unwrap(),
            FILE_BYTES
        );
        assert_eq!(
            host.retire_editor_draft_handoff_proposal(CARD, DRAFT, &recovered.request.operation_id)
                .unwrap()
                .status,
            Status::ParentRetired
        );
        assert_eq!(business_revision(&host), 3);
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    for (phase, code) in [("prepare", 73), ("commit", 74), ("recover", 0)] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "proposal_survives_independent_processes_without_callers_original_request",
                "--nocapture",
            ])
            .env("MORROW_HANDOFF_PROPOSAL_PHASE", phase)
            .env("MORROW_HANDOFF_PROPOSAL_DB", &path)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "phase {phase}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn retirement_failure_and_lost_receipt_keep_original_identity_and_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset);
    host.prepare_editor_draft_handoff_proposal(&child, &link(&parent, &child, &digest), RETIRE)
        .unwrap();
    host.complete_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_retirement BEFORE UPDATE ON cards WHEN OLD.id LIKE 'morrow-host-editor-draft-%' BEGIN SELECT RAISE(ABORT, 'retirement failure'); END;").unwrap();
    assert!(
        host.retire_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .is_err()
    );
    let pending = host
        .inspect_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(pending.status, Status::ChildCommitted);
    assert!(pending.parent_active && pending.child_active);
    sql.execute_batch("DROP TRIGGER reject_retirement;")
        .unwrap();
    drop(sql);
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    host.retire_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
        .unwrap();
    drop(host); // Discard the result, then discover from persisted proposal only.
    let mut host = Workbench::open(&path, None).unwrap();
    let (records, _) = host.list_editor_draft_handoff_proposals("", 32).unwrap();
    assert_eq!(records[0].status, Status::ParentRetired);
    assert_eq!(records[0].retirement_operation, RETIRE);
    assert_eq!(
        host.retire_editor_draft_handoff_proposal(CARD, DRAFT, &records[0].request.operation_id)
            .unwrap()
            .status,
        Status::ParentRetired
    );
    assert_eq!(
        host.export_editor_draft_asset(CARD, CHILD, 1, &asset)
            .unwrap(),
        FILE_BYTES
    );
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        2
    );
}

#[test]
fn cancelled_proposal_keeps_operation_reservations_and_checked_pagination() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset);
    host.prepare_editor_draft_handoff_proposal(&child, &link(&parent, &child, &digest), RETIRE)
        .unwrap();
    host.cancel_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
        .unwrap();
    let mut replacement = child.clone();
    replacement.draft_id = "fresh-child".into();
    replacement.operation_id = "fresh-child-save".into();
    let binding = link(&parent, &replacement, &digest);
    assert!(
        host.prepare_editor_draft_handoff_proposal(&replacement, &binding, RETIRE)
            .is_err()
    );
    replacement.operation_id = RETIRE.into();
    assert!(
        host.prepare_editor_draft_handoff_proposal(
            &replacement,
            &link(&parent, &replacement, &digest),
            "fresh-retirement"
        )
        .is_err()
    );
    replacement.operation_id = "fresh-child-save".into();
    assert!(host.handoff_editor_draft(&replacement, &binding).is_err());
    host.prepare_editor_draft_handoff_proposal(&replacement, &binding, "fresh-retirement")
        .unwrap();
    let (first, next) = host.list_editor_draft_handoff_proposals("", 1).unwrap();
    assert_eq!(first.len(), 1);
    let (second, end) = host
        .list_editor_draft_handoff_proposals(&next.unwrap(), 1)
        .unwrap();
    assert_eq!(second.len(), 1);
    assert!(end.is_none());
    assert_ne!(
        first[0].request.operation_id,
        second[0].request.operation_id
    );
    assert!(host.list_editor_draft_handoff_proposals("", 0).is_err());
    assert!(host.list_editor_draft_handoff_proposals("", 33).is_err());
    assert!(
        host.list_editor_draft_handoff_proposals("morrow-host-editor-handoff-proposal-invalid", 1)
            .is_err()
    );
    assert!(
        host.inspect_editor_draft_handoff_proposal(CARD, "other-parent", &child.operation_id)
            .is_err()
    );
    assert!(
        host.read_editor_draft(CARD, &replacement.draft_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn consumed_durable_import_can_be_cleaned_after_prepared_parent_retirement() {
    use morrow_workbench_host::editor_draft_staging::{DraftImportPhase, proto::ImportRequest};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let initial = request("initial-before-import", 0, "Still unfinished S2");
    host.save_editor_draft(&initial).unwrap();
    let import = ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: "handoff-durable-import".into(),
        expected_generation: 1,
        name: "only-on-disk.bin".into(),
        kind: "file".into(),
        byte_length: FILE_BYTES.len() as u64,
        sha256: Sha256::digest(FILE_BYTES).to_vec(),
    };
    let imported = host
        .import_editor_draft_asset_durable(&import, &mut &FILE_BYTES[..])
        .unwrap();
    let mut parent = initial.clone();
    parent.operation_id = "parent-with-durable-pin".into();
    parent.expected_generation = 1;
    parent.assets.push(AssetSelection {
        origin: 2,
        asset_id: imported.asset_id.clone(),
        aliases: vec!["only-on-disk.bin".into()],
    });
    host.save_editor_draft(&parent).unwrap();
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &imported.asset_id);
    let mut binding = link(&parent, &child, &digest);
    binding.parent_generation = 2;
    host.prepare_editor_draft_handoff_proposal(&child, &binding, RETIRE)
        .unwrap();
    host.complete_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
        .unwrap();
    host.retire_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
        .unwrap();
    host.reconcile_editor_draft_imports(CARD, DRAFT).unwrap();
    let retired = host
        .inspect_editor_draft_import(CARD, DRAFT, &import.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(retired.phase, DraftImportPhase::Retired);
    assert!(!retired.bytes_retained);
    assert_eq!(
        host.export_editor_draft_asset(CARD, CHILD, 1, &imported.asset_id)
            .unwrap(),
        FILE_BYTES
    );
}

#[test]
fn corrupted_proposal_is_not_treated_as_absence_or_executable_pending() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset);
    host.prepare_editor_draft_handoff_proposal(&child, &link(&parent, &child, &digest), RETIRE)
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    let (id, bytes): (String, Vec<u8>) = sql
        .query_row(
            "SELECT id, payload FROM cards WHERE id LIKE 'morrow-host-editor-handoff-proposal-%'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    sql.execute("UPDATE cards SET payload = X'0001' WHERE id = ?1", [&id])
        .unwrap();
    assert!(
        host.inspect_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .is_err()
    );
    assert!(host.list_editor_draft_handoff_proposals("", 32).is_err());
    assert!(
        host.complete_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .is_err()
    );
    assert!(
        host.cancel_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .is_err()
    );
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    sql.execute(
        "UPDATE cards SET payload = ?1 WHERE id = ?2",
        rusqlite::params![bytes, id],
    )
    .unwrap();
    assert_eq!(
        host.inspect_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .unwrap()
            .status,
        Status::Pending
    );
    assert_eq!(
        host.export_editor_draft_asset(CARD, DRAFT, 1, &asset)
            .unwrap(),
        FILE_BYTES
    );
}
