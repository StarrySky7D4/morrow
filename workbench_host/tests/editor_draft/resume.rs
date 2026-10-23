use super::*;
use morrow_workbench_host::editor_draft::{EditorDraftRecord, model::proto};
use prost::Message;
use sha2::{Digest, Sha256};

fn parent_with_capture(host: &mut Workbench) -> EditorDraftRecord {
    use capnp::{message::Builder, serialize};
    use morrow_workbench_plugin::{capture, capture_capnp};
    use morrow_workbench_host::capture_provenance::{PasteEvent, PastePart};
    let scope = host.open_capture_scope(CARD, 2).unwrap();
    let mut message = Builder::new_default();
    let mut input = message.init_root::<capture_capnp::request::Builder>();
    input.set_version(1); input.set_digest(&capture::digest());
    input.set_format("plain"); input.set_source("captured text");
    let (ticket, _) = host.capture_scoped(&scope, serialize::write_message_to_words(&message), "").unwrap();
    host.record_paste(&scope, PasteEvent {
        id: "resume-original-paste".into(), field: "description".into(), before: "raw body".into(),
        start_utf16: 0, end_utf16: 8,
        parts: vec![PastePart { ticket, literal: String::new(), selection: "outputMarkdown".into() }],
        after: "captured text".into(),
    }).unwrap();
    let asset = host.import_editor_draft_asset(CARD, DRAFT, 0, "resume.bin", "file",
        &mut &FILE_BYTES[..], FILE_BYTES.len() as u64).unwrap();
    let mut value = request("resume-original-save", 0, "");
    value.commit_operation = "resume-original-business".into();
    value.capture_scope = scope; value.capture_events = 1;
    let raw = value.values.as_mut().unwrap().description.as_mut().unwrap();
    raw.text = "captured text edited 😀".into(); raw.selection_base = 21; raw.selection_extent = 23;
    raw.composing_start = 21; raw.composing_end = 23; raw.affinity = 1; raw.directional = true;
    value.assets.push(proto::AssetSelection { origin: 2, asset_id: asset.id, aliases: vec!["deleted-source.bin".into()] });
    host.save_editor_draft(&value).unwrap()
}

fn child(host: &mut Workbench, parent: &EditorDraftRecord, suffix: &str) -> (proto::WriteRequest, proto::ResumeLink) {
    let original = parent.slot.request.as_ref().unwrap();
    let mut value = original.clone();
    value.draft_id = format!("resumed-{suffix}"); value.expected_generation = 0;
    value.operation_id = original.commit_operation.clone();
    value.commit_operation = format!("resumed-business-{suffix}");
    value.capture_scope = host.open_capture_scope(&original.card_id, original.source_revision).unwrap();
    value.capture_events = 0;
    for asset in &mut value.assets { asset.origin = 4; }
    let link = proto::ResumeLink { parent_draft_id: original.draft_id.clone(), parent_generation: parent.slot.generation,
        parent_save_operation: original.operation_id.clone(), parent_request_sha256: Sha256::digest(original.encode_to_vec()).to_vec(),
        parent_business_operation: original.commit_operation.clone(), retirement_operation: format!("resume-retirement-{suffix}") };
    (value, link)
}

fn snapshot(fields: &Fields) -> EditorSnapshot {
    EditorSnapshot { title: fields.title.clone(), description: fields.description.clone(),
        hypothesis: fields.hypothesis.clone(), conclusion: fields.conclusion.clone(), todos: String::new(), aliases: vec![] }
}

#[test]
fn fresh_resume_preserves_actual_pins_raw_values_and_archive_and_fences_old_business() {
    let directory = tempfile::tempdir().unwrap(); let path = directory.path().join("db");
    let mut host = seed(&path); let parent = parent_with_capture(&mut host);
    let original = parent.slot.request.as_ref().unwrap();
    let raw = host.export_editor_draft_capture(CARD, DRAFT, 1).unwrap();
    let (premature, premature_link) = child(&mut host, &parent, "premature");
    assert!(host.resume_editor_draft(&premature, &premature_link).is_err());
    assert!(host.read_editor_draft(CARD, &premature.draft_id).unwrap().is_none());
    host.close_capture_scope(&original.capture_scope).unwrap();
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let (value, link) = child(&mut host, &parent, "first");
    let saved = host.resume_editor_draft(&value, &link).unwrap();
    assert_eq!(saved.slot.generation, 1); assert_eq!(saved.slot.resume_link.as_ref(), Some(&link));
    assert_eq!(saved.slot.request.as_ref().unwrap().values, original.values);
    let archive = proto::CaptureArchive::decode(host.export_editor_draft_capture(CARD, &value.draft_id, 1).unwrap().as_slice()).unwrap();
    assert_eq!(archive.parent_archive, raw); assert!(archive.applications.is_empty());
    assert_ne!(archive.scope, original.capture_scope);
    assert_eq!(host.export_editor_draft_asset(CARD, &value.draft_id, 1, &value.assets[0].asset_id).unwrap(), FILE_BYTES);
    let fields = source_fields(&host);
    let error = host.edit_card_captured(&original.commit_operation, CARD, 2, &fields, &value.capture_scope, snapshot(&fields)).err().unwrap();
    assert!(error.to_string().contains("operation already used"));
    assert!(host.editor_recovery(CARD).unwrap().is_none()); assert_eq!(business_revision(&host), 2);
    let mut late = original.clone(); late.operation_id = "late-original-save".into(); late.expected_generation = 1;
    assert!(host.save_editor_draft(&late).is_err());
    assert!(host.retire_resumed_editor_draft_parent(CARD, &value.draft_id, "wrong-retirement").is_err());
    let retired = host.retire_resumed_editor_draft_parent(CARD, &value.draft_id, &link.retirement_operation).unwrap();
    assert!(!retired.slot.active); assert_eq!(retired.slot.generation, 2);
    assert!(host.export_editor_draft_asset(CARD, DRAFT, 2, &value.assets[0].asset_id).is_err());
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    assert_eq!(host.read_editor_draft(CARD, &value.draft_id).unwrap().unwrap().slot, saved.slot);
    assert!(host.resume_editor_draft(&value, &link).unwrap().repeated);
    assert!(host.retire_resumed_editor_draft_parent(CARD, &value.draft_id, &link.retirement_operation).unwrap().repeated);
    assert_eq!(host.export_editor_draft_asset(CARD, &value.draft_id, 1, &value.assets[0].asset_id).unwrap(), FILE_BYTES);
    let mut changed = value.clone(); changed.values.as_mut().unwrap().title.as_mut().unwrap().text = "changed retry".into();
    assert!(host.resume_editor_draft(&changed, &link).is_err());
}

#[test]
fn failed_resume_transaction_neither_fences_business_nor_consumes_a_successor() {
    let directory = tempfile::tempdir().unwrap(); let path = directory.path().join("db");
    let mut host = seed(&path); let parent = parent_with_capture(&mut host);
    host.close_capture_scope(&parent.slot.request.as_ref().unwrap().capture_scope).unwrap();
    let (value, link) = child(&mut host, &parent, "write-failure");
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_resume BEFORE INSERT ON cards
        WHEN NEW.id LIKE 'morrow-host-editor-draft-%'
        BEGIN SELECT RAISE(ABORT, 'resume child write failure'); END;").unwrap();
    assert!(host.resume_editor_draft(&value, &link).is_err());
    assert!(host.read_editor_draft(CARD, &value.draft_id).unwrap().is_none());
    assert_eq!(host.read_editor_draft(CARD, DRAFT).unwrap().unwrap().slot, parent.slot);
    assert_eq!(host.export_editor_draft_asset(CARD, DRAFT, 1, &value.assets[0].asset_id).unwrap(), FILE_BYTES);
    sql.execute_batch("DROP TRIGGER reject_resume;").unwrap();
    // A known atomic storage abort: the exact operation can still commit once.
    // This is not permission for clients to replay an ambiguous transport call.
    let saved = host.resume_editor_draft(&value, &link).unwrap();
    assert!(!saved.repeated); assert_eq!(saved.slot.generation, 1);
    assert!(host.resume_editor_draft(&value, &link).unwrap().repeated);
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn failed_resume_parent_retirement_survives_restart_and_archiving_does_not_allow_siblings() {
    let directory = tempfile::tempdir().unwrap(); let path = directory.path().join("db");
    let mut host = seed(&path); let parent = parent_with_capture(&mut host);
    host.close_capture_scope(&parent.slot.request.as_ref().unwrap().capture_scope).unwrap();
    let (value, link) = child(&mut host, &parent, "retire-failure");
    let saved = host.resume_editor_draft(&value, &link).unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_resume_retire BEFORE UPDATE ON cards
        WHEN OLD.id LIKE 'morrow-host-editor-draft-%'
        BEGIN SELECT RAISE(ABORT, 'resume parent retirement failure'); END;").unwrap();
    assert!(host.retire_resumed_editor_draft_parent(CARD, &value.draft_id, &link.retirement_operation).is_err());
    assert!(host.read_editor_draft(CARD, DRAFT).unwrap().unwrap().slot.active);
    assert!(host.read_editor_draft(CARD, &value.draft_id).unwrap().unwrap().slot.active);
    sql.execute_batch("DROP TRIGGER reject_resume_retire;").unwrap(); drop(sql); drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let (grandchild, next_link) = child(&mut host, &saved, "after-second-restart");
    assert!(host.resume_editor_draft(&grandchild, &next_link).is_err());
    host.retire_resumed_editor_draft_parent(CARD, &value.draft_id, &link.retirement_operation).unwrap();
    host.archive_editor_draft(CARD, DRAFT, 2, &link.parent_request_sha256, "archive-resume-original").unwrap();
    let next = host.resume_editor_draft(&grandchild, &next_link).unwrap();
    assert_eq!(next.slot.request.as_ref().unwrap().values, value.values);
    host.retire_resumed_editor_draft_parent(CARD, &grandchild.draft_id, &next_link.retirement_operation).unwrap();
    host.archive_editor_draft(CARD, &value.draft_id, 2, &next_link.parent_request_sha256, "archive-resume-child").unwrap();
    let (sibling, sibling_link) = child(&mut host, &parent, "forbidden-sibling");
    assert!(host.resume_editor_draft(&sibling, &sibling_link).is_err());
    assert_eq!(host.export_editor_draft_asset(CARD, &grandchild.draft_id, 1, &value.assets[0].asset_id).unwrap(), FILE_BYTES);
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn resume_refuses_source_changes_and_incomplete_raw_snapshots_without_retiring_parent() {
    let directory = tempfile::tempdir().unwrap(); let mut host = seed(&directory.path().join("db"));
    let parent = parent_with_capture(&mut host);
    host.close_capture_scope(&parent.slot.request.as_ref().unwrap().capture_scope).unwrap();
    let (value, link) = child(&mut host, &parent, "reject");
    let mut changed = value.clone(); changed.assets.clear();
    assert!(host.resume_editor_draft(&changed, &link).is_err());
    changed = value.clone(); changed.values.as_mut().unwrap().description.as_mut().unwrap().directional = false;
    assert!(host.resume_editor_draft(&changed, &link).is_err());
    let mut forged = link.clone(); forged.parent_request_sha256[0] ^= 1;
    assert!(host.resume_editor_draft(&value, &forged).is_err());
    host.edit_card("source-changed-before-resume", CARD, 2, &CardAction::SetFavorite(true)).unwrap();
    assert!(host.resume_editor_draft(&value, &link).is_err());
    assert!(host.read_editor_draft(CARD, &value.draft_id).unwrap().is_none());
    assert_eq!(host.read_editor_draft(CARD, DRAFT).unwrap().unwrap().slot, parent.slot);
    assert_eq!(business_revision(&host), 3);
}

#[test]
fn new_card_resume_pins_assets_and_fences_late_legacy_create_before_business_effects() {
    let directory = tempfile::tempdir().unwrap(); let path = directory.path().join("db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let mut original = request("new-card-parent-save", 0, "");
    original.card_id = "new-resume-card".into(); original.source_kind = 1; original.source_revision = 0;
    original.commit_operation = "new-card-original-business".into();
    original.capture_scope = host.open_capture_scope(&original.card_id, 0).unwrap();
    host.save_editor_draft(&original).unwrap();
    original.expected_generation = 1; original.operation_id = "new-card-parent-with-asset".into();
    let asset = host.import_editor_draft_asset(&original.card_id, DRAFT, 1, "new.bin", "file",
        &mut &FILE_BYTES[..], FILE_BYTES.len() as u64).unwrap();
    original.assets.push(proto::AssetSelection { origin: 2, asset_id: asset.id, aliases: vec!["new.bin".into()] });
    let parent = host.save_editor_draft(&original).unwrap();
    host.close_capture_scope(&original.capture_scope).unwrap(); drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let (value, link) = child(&mut host, &parent, "new-card");
    // An ordinary save cannot claim another draft's pins.
    assert!(host.save_editor_draft(&value).is_err());
    let saved = host.resume_editor_draft(&value, &link).unwrap();
    assert!(saved.slot.source_card.is_empty());
    assert_eq!(host.export_editor_draft_asset(&value.card_id, &value.draft_id, 1, &value.assets[0].asset_id).unwrap(), FILE_BYTES);
    let late = Idea { id: value.card_id.clone(), title: "late old Create".into(), category: "灵感".into(),
        stage: "待整理".into(), ..Default::default() };
    let fields = EditorSnapshot { title: late.title.clone(), description: String::new(), hypothesis: String::new(),
        conclusion: String::new(), todos: String::new(), aliases: vec![] };
    assert!(host.create_captured(&original.commit_operation, late, &value.capture_scope, fields).is_err());
    assert!(host.read_versioned(&value.card_id).is_err());
    assert!(host.editor_recovery(&value.card_id).unwrap().is_none());
    host.retire_resumed_editor_draft_parent(&value.card_id, &value.draft_id, &link.retirement_operation).unwrap();
    let mut edited = value.clone(); edited.operation_id = "new-resume-second-save".into(); edited.expected_generation = 1;
    edited.assets[0].origin = 3;
    edited.values.as_mut().unwrap().title = Some(text_value("edited after restart"));
    let second = host.save_editor_draft(&edited).unwrap();
    assert_eq!(second.slot.generation, 2); assert_eq!(second.slot.resume_link.as_ref(), Some(&link));
    assert_eq!(host.export_editor_draft_asset(&value.card_id, &value.draft_id, 2, &value.assets[0].asset_id).unwrap(), FILE_BYTES);
    for (generation, operation, scope) in [(1, value.commit_operation.as_str(), value.capture_scope.as_str()),
        (2, "foreign-business", value.capture_scope.as_str()), (2, value.commit_operation.as_str(), "foreign-scope")] {
        assert!(host.stage_editor_draft_assets(&value.card_id, &value.draft_id, generation, operation, scope).is_err());
    }
    host.stage_editor_draft_assets(&value.card_id, &value.draft_id, 2, &value.commit_operation, &value.capture_scope).unwrap();
    let actual = Idea { id: value.card_id.clone(), title: "edited after restart".into(), description: "raw body".into(),
        category: "进行中".into(), stage: "计划中".into(), assets: vec![Asset { id: value.assets[0].asset_id.clone(),
        name: "new.bin".into(), kind: "file".into(), bytes: FILE_BYTES.len() as u64 }], ..Default::default() };
    let fields = EditorSnapshot { title: actual.title.clone(), description: actual.description.clone(),
        hypothesis: String::new(), conclusion: String::new(), todos: String::new(), aliases: vec![] };
    let committed = host.create_captured(&value.commit_operation, actual, &value.capture_scope, fields).unwrap();
    assert_eq!(committed.revision, 1);
    assert_eq!(committed.idea.assets[0].id, value.assets[0].asset_id);
    assert!(host.stage_editor_draft_assets(&value.card_id, &value.draft_id, 2, &value.commit_operation, &value.capture_scope).is_err());
}

#[test]
fn resumed_v2_business_uses_durable_draft_pins_without_reopening_preview_files() {
    let directory = tempfile::tempdir().unwrap(); let mut host = seed(&directory.path().join("db"));
    let parent = parent_with_capture(&mut host);
    host.close_capture_scope(&parent.slot.request.as_ref().unwrap().capture_scope).unwrap();
    let (value, link) = child(&mut host, &parent, "v2-business");
    host.resume_editor_draft(&value, &link).unwrap();
    assert!(host.stage_editor_draft_assets(CARD, &value.draft_id, 1, &value.commit_operation, &value.capture_scope).is_err());
    host.retire_resumed_editor_draft_parent(CARD, &value.draft_id, &link.retirement_operation).unwrap();
    host.stage_editor_draft_assets(CARD, &value.draft_id, 1, &value.commit_operation, &value.capture_scope).unwrap();
    let mut fields = source_fields(&host); fields.title = "Resumed V2 business".into();
    fields.description = value.values.as_ref().unwrap().description.as_ref().unwrap().text.clone();
    fields.assets.push(Asset { id: value.assets[0].asset_id.clone(), name: "resume.bin".into(), kind: "file".into(), bytes: FILE_BYTES.len() as u64 });
    let committed = host.edit_card_captured(&value.commit_operation, CARD, 2, &fields, &value.capture_scope, snapshot(&fields)).unwrap();
    assert_eq!(committed.receipt.revision, 3); assert_eq!(business_revision(&host), 3);
    assert!(host.stage_editor_draft_assets(CARD, &value.draft_id, 1, &value.commit_operation, &value.capture_scope).is_err());
    assert_eq!(host.export_editor_draft_asset(CARD, &value.draft_id, 1, &value.assets[0].asset_id).unwrap(), FILE_BYTES);
}
