#![cfg(target_os = "windows")]
//! Real host and compiled WASM coverage for the private editor draft journal.
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_core::plugin_package::Package;
use morrow_core::plugin_package::proto::{Capability, TransformHandler};
use morrow_workbench_host::editor_draft::model::proto::{AssetSelection, ParentLink, WriteRequest};
use morrow_workbench_host::{
    Workbench, capture_provenance::EditorSnapshot, versioned_record::VersionedRecord,
};
use morrow_workbench_host::{editor_draft_api_capnp as draft_wire, host_capnp as wire, protocol};
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

fn frame(
    host: &mut Workbench,
    action: wire::Action,
    fill: impl FnOnce(wire::request::Builder<'_>),
) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut out = message.init_root::<wire::request::Builder>();
    out.set_version(1);
    out.set_digest(&protocol::digest());
    out.set_action(action);
    fill(out);
    protocol::respond(host, &serialize::write_message_to_words(&message)).unwrap()
}
fn response(bytes: &[u8]) -> capnp::message::Reader<serialize::OwnedSegments> {
    serialize::read_message(&mut bytes.as_ref(), ReaderOptions::default()).unwrap()
}
fn error(bytes: &[u8]) -> String {
    response(bytes)
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_error()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}
fn assert_ok(bytes: &[u8]) {
    assert!(error(bytes).is_empty(), "host error: {}", error(bytes));
}
fn draft_digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/editor_draft_api.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn wire_text(
    value: &morrow_workbench_host::editor_draft::model::proto::TextValue,
    mut out: draft_wire::text_value::Builder<'_>,
) {
    out.set_text(&value.text);
    out.set_selection_base(value.selection_base);
    out.set_selection_extent(value.selection_extent);
    out.set_affinity(value.affinity as u16);
    out.set_directional(value.directional);
    out.set_composing_start(value.composing_start);
    out.set_composing_end(value.composing_end);
}
fn wire_request(value: &WriteRequest, mut out: draft_wire::write_request::Builder<'_>) {
    out.set_version(1);
    out.set_digest(&draft_digest());
    out.set_card_id(&value.card_id);
    out.set_draft_id(&value.draft_id);
    out.set_operation(&value.operation_id);
    out.set_expected_generation(value.expected_generation);
    out.set_source_revision(value.source_revision);
    out.set_source_kind(value.source_kind as u16);
    out.set_predecessor_operation(&value.predecessor_operation);
    out.set_predecessor_digest(&value.predecessor_sha256);
    let source = value.values.as_ref().unwrap();
    let mut values = out.reborrow().init_values();
    wire_text(
        source.title.as_ref().unwrap(),
        values.reborrow().init_title(),
    );
    wire_text(
        source.description.as_ref().unwrap(),
        values.reborrow().init_description(),
    );
    wire_text(
        source.hypothesis.as_ref().unwrap(),
        values.reborrow().init_hypothesis(),
    );
    wire_text(
        source.conclusion.as_ref().unwrap(),
        values.reborrow().init_conclusion(),
    );
    wire_text(
        source.todos.as_ref().unwrap(),
        values.reborrow().init_todos(),
    );
    values.set_category(&source.category);
    values.set_stage(&source.stage);
    let mut assets = out.reborrow().init_assets(value.assets.len() as u32);
    for (index, selection) in value.assets.iter().enumerate() {
        let mut item = assets.reborrow().get(index as u32);
        item.set_origin(selection.origin as u16);
        item.set_asset_id(&selection.asset_id);
        let mut aliases = item.init_aliases(selection.aliases.len() as u32);
        for (alias_index, alias) in selection.aliases.iter().enumerate() {
            aliases.set(alias_index as u32, alias);
        }
    }
}
fn wire_link(value: &ParentLink, mut out: draft_wire::parent_link::Builder<'_>) {
    out.set_parent_draft_id(&value.parent_draft_id);
    out.set_parent_generation(value.parent_generation);
    out.set_parent_save_operation(&value.parent_save_operation);
    out.set_parent_request_sha256(&value.parent_request_sha256);
    out.set_committed_operation(&value.committed_operation);
    out.set_committed_sha256(&value.committed_sha256);
    out.set_child_operation(&value.child_operation);
}
fn handoff_body(request: &WriteRequest, link: &ParentLink) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut out = message.init_root::<draft_wire::handoff_request::Builder>();
    out.set_version(1);
    out.set_digest(&draft_digest());
    wire_request(request, out.reborrow().init_request());
    wire_link(link, out.reborrow().init_parent_link());
    serialize::write_message_to_words(&message)
}
fn upload(host: &mut Workbench, operation: &str, body: &[u8]) -> String {
    let reply = frame(host, wire::Action::BeginEditorDraft, |mut r| {
        r.set_operation(operation);
        r.set_total_length(body.len() as u64);
        r.set_sha256(&Sha256::digest(body));
    });
    assert_ok(&reply);
    let token = response(&reply)
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_transfer()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    for (index, piece) in body.chunks(32768).enumerate() {
        let appended = frame(host, wire::Action::AppendEditorDraft, |mut r| {
            r.set_transfer(&token);
            r.set_offset((index * 32768) as u64);
            r.set_payload(piece);
        });
        assert_ok(&appended);
    }
    token
}
fn finish_handoff(host: &mut Workbench, token: &str, draft: &str, operation: &str) -> Vec<u8> {
    frame(host, wire::Action::FinishEditorDraftHandoff, |mut r| {
        r.set_transfer(token);
        r.set_id(CARD);
        r.set_attachment(draft);
        r.set_operation(operation);
        r.set_revision(0);
    })
}
fn download(host: &mut Workbench, first: &[u8]) -> Vec<u8> {
    assert_ok(first);
    let reply = response(first);
    let head = reply.get_root::<wire::response::Reader>().unwrap();
    let token = head.get_transfer().unwrap().to_str().unwrap().to_owned();
    let total = head.get_total_length() as usize;
    let digest = head.get_sha256().unwrap().to_vec();
    let revision = head.get_revision();
    let mut bytes = head.get_payload().unwrap().to_vec();
    assert_eq!(head.get_offset(), 0);
    assert!(!bytes.is_empty());
    while bytes.len() < total {
        let next = frame(host, wire::Action::ReadEditorDraftPart, |mut r| {
            r.set_transfer(&token);
            r.set_offset(bytes.len() as u64);
        });
        assert_ok(&next);
        let part = response(&next);
        let value = part.get_root::<wire::response::Reader>().unwrap();
        assert_eq!(value.get_revision(), revision);
        assert_eq!(value.get_offset(), bytes.len() as u64);
        assert_eq!(value.get_total_length() as usize, total);
        bytes.extend_from_slice(value.get_payload().unwrap());
    }
    assert_eq!(bytes.len(), total);
    assert_eq!(Sha256::digest(&bytes).as_slice(), digest);
    bytes
}
fn record(bytes: &[u8]) -> capnp::message::Reader<serialize::OwnedSegments> {
    serialize::read_message(&mut bytes.as_ref(), ReaderOptions::default()).unwrap()
}

#[test]
fn handoff_and_retirement_bind_outer_identity_and_reconcile_lost_first_replies() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (first_parent, asset_id) = parent_with_asset(&mut host);
    let mut parent = first_parent.clone();
    parent.operation_id = "parent-large-snapshot".into();
    parent.expected_generation = 1;
    parent.assets[0].origin = 3;
    parent
        .values
        .as_mut()
        .unwrap()
        .description
        .as_mut()
        .unwrap()
        .text = "unfinished😀".repeat(8000);
    assert_eq!(host.save_editor_draft(&parent).unwrap().slot.generation, 2);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset_id);
    let binding = link(&parent, &child, &digest);
    let body = handoff_body(&child, &binding);

    let wrong = upload(&mut host, &child.operation_id, &body);
    let refused = frame(
        &mut host,
        wire::Action::FinishEditorDraftHandoff,
        |mut r| {
            r.set_transfer(&wrong);
            r.set_id(CARD);
            r.set_attachment("wrong-child");
            r.set_operation(&child.operation_id);
        },
    );
    assert!(!error(&refused).is_empty());
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        2
    );

    let token = upload(&mut host, &child.operation_id, &body);
    let lost = finish_handoff(&mut host, &token, CHILD, &child.operation_id);
    assert_ok(&lost);
    assert!(
        response(&lost)
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_total_length()
            > 32768
    );
    assert_eq!(
        host.read_editor_draft(CARD, CHILD)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        1
    );
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| {
            r.set_transfer("wrong-upload-token");
        },
    ));
    let busy = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(CHILD);
    });
    assert!(!error(&busy).is_empty());
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| {
            r.set_transfer(&token);
        },
    ));
    let readable = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(CHILD);
    });
    let decoded = download(&mut host, &readable);
    let message = record(&decoded);
    let envelope = message.get_root::<draft_wire::envelope::Reader>().unwrap();
    let stored = envelope.get_record().unwrap();
    assert!(stored.has_parent_link());
    let parent_link = stored.get_parent_link().unwrap();
    assert_eq!(
        parent_link.get_parent_draft_id().unwrap().to_str().unwrap(),
        DRAFT
    );
    assert_eq!(parent_link.get_parent_generation(), 2);
    assert_eq!(
        stored.get_request_sha256().unwrap(),
        Sha256::digest(child.encode_to_vec()).as_slice()
    );
    assert_eq!(business_revision(&host), 3);

    let retry_token = upload(&mut host, &child.operation_id, &body);
    let retry = finish_handoff(&mut host, &retry_token, CHILD, &child.operation_id);
    let retry_bytes = download(&mut host, &retry);
    let repeated = record(&retry_bytes);
    assert!(
        repeated
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_record()
            .unwrap()
            .get_repeated()
    );

    let wrong_retire = frame(&mut host, wire::Action::RetireEditorDraftParent, |mut r| {
        r.set_id(CARD);
        r.set_attachment(CHILD);
        r.set_name("wrong-parent");
        r.set_kind(&binding.child_operation);
        r.set_revision(binding.parent_generation);
        r.set_operation("wire-parent-retire");
        r.set_transfer("draft-request-1");
    });
    assert!(!error(&wrong_retire).is_empty());
    assert!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );

    let lost_retire = frame(&mut host, wire::Action::RetireEditorDraftParent, |mut r| {
        r.set_id(CARD);
        r.set_attachment(CHILD);
        r.set_name(DRAFT);
        r.set_kind(&binding.child_operation);
        r.set_revision(binding.parent_generation);
        r.set_operation("wire-parent-retire");
        r.set_transfer("draft-request-2");
    });
    assert_ok(&lost_retire);
    assert!(
        response(&lost_retire)
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_total_length()
            > 32768
    );
    assert!(
        !host
            .read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| {
            r.set_transfer("draft-request-2");
        },
    ));
    let retirement_retry = frame(&mut host, wire::Action::RetireEditorDraftParent, |mut r| {
        r.set_id(CARD);
        r.set_attachment(CHILD);
        r.set_name(DRAFT);
        r.set_kind(&binding.child_operation);
        r.set_revision(binding.parent_generation);
        r.set_operation("wire-parent-retire");
        r.set_transfer("draft-request-3");
    });
    let retired_bytes = download(&mut host, &retirement_retry);
    let retired_message = record(&retired_bytes);
    let retired_envelope = retired_message
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap();
    assert_eq!(
        retired_envelope.get_draft_id().unwrap().to_str().unwrap(),
        DRAFT
    );
    let retired = retired_envelope.get_record().unwrap();
    assert!(retired.get_repeated());
    assert!(retired.has_retirement());
    assert_eq!(retired.get_retirement().unwrap().get_parent_generation(), 2);
    assert_eq!(business_revision(&host), 3);
}

#[test]
fn lineage_pages_are_read_only_across_restart_and_echo_cursor() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset_id) = parent_with_asset(&mut host);
    let mut second_parent = request("parallel-parent-save", 0, "Parallel unfinished draft");
    second_parent.draft_id = "parallel-parent".into();
    host.save_editor_draft(&second_parent).unwrap();
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset_id);
    let binding = link(&parent, &child, &digest);
    host.handoff_editor_draft(&child, &binding).unwrap();
    let mut second_child = request("parallel-child-save", 0, "unused");
    second_child.draft_id = "parallel-child".into();
    second_child.source_revision = 3;
    second_child.values = second_parent.values.clone();
    let mut second_link = link(&second_parent, &second_child, &digest);
    second_link.child_operation = second_child.operation_id.clone();
    host.handoff_editor_draft(&second_child, &second_link)
        .unwrap();
    assert_eq!(host.list_editor_draft_lineages().unwrap().len(), 2);
    drop(host);

    let mut readonly = Workbench::open(&path, None).unwrap();
    let first = frame(
        &mut readonly,
        wire::Action::ListEditorDraftLineages,
        |mut r| {
            r.set_limit(1);
            r.set_transfer("draft-request-1");
        },
    );
    let bytes = download(&mut readonly, &first);
    let message = record(&bytes);
    let page = message.get_root::<draft_wire::envelope::Reader>().unwrap();
    assert_eq!(page.get_kind().unwrap(), draft_wire::ResultKind::Lineages);
    assert_eq!(page.get_request_cursor().unwrap().to_str().unwrap(), "");
    assert_eq!(page.get_request_limit(), 1);
    assert_eq!(page.get_lineages().unwrap().len(), 1);
    let next = page.get_next_cursor().unwrap().to_str().unwrap().to_owned();
    assert!(!next.is_empty());
    assert_eq!(
        page.get_lineages()
            .unwrap()
            .get(0)
            .get_cursor()
            .unwrap()
            .to_str()
            .unwrap(),
        next
    );
    let second = frame(
        &mut readonly,
        wire::Action::ListEditorDraftLineages,
        |mut r| {
            r.set_cursor(&next);
            r.set_limit(1);
            r.set_transfer("draft-request-2");
        },
    );
    let second_bytes = download(&mut readonly, &second);
    let second_message = record(&second_bytes);
    let second_page = second_message
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap();
    assert_eq!(
        second_page.get_kind().unwrap(),
        draft_wire::ResultKind::Lineages
    );
    assert_eq!(
        second_page.get_request_cursor().unwrap().to_str().unwrap(),
        next
    );
    assert_eq!(second_page.get_request_limit(), 1);
    assert_eq!(second_page.get_lineages().unwrap().len(), 1);
    assert_eq!(second_page.get_next_cursor().unwrap().to_str().unwrap(), "");
    assert_ne!(
        page.get_lineages()
            .unwrap()
            .get(0)
            .get_child_draft_id()
            .unwrap()
            .to_str()
            .unwrap(),
        second_page
            .get_lineages()
            .unwrap()
            .get(0)
            .get_child_draft_id()
            .unwrap()
            .to_str()
            .unwrap()
    );
    let invalid = frame(
        &mut readonly,
        wire::Action::ListEditorDraftLineages,
        |mut r| {
            r.set_cursor("foreign-cursor");
            r.set_limit(1);
            r.set_transfer("draft-request-3");
        },
    );
    assert!(!error(&invalid).is_empty());
    assert_eq!(business_revision(&readonly), 3);
    assert_eq!(readonly.list_editor_draft_lineages().unwrap().len(), 2);
}

fn proposal_body(request: &WriteRequest, link: &ParentLink) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut out = message.init_root::<draft_wire::handoff_proposal::Builder>();
    out.set_version(1);
    out.set_digest(&draft_digest());
    wire_request(request, out.reborrow().init_request());
    wire_link(link, out.reborrow().init_parent_link());
    out.set_retirement_operation("wire-retirement");
    serialize::write_message_to_words(&message)
}

fn proposal_action(
    host: &mut Workbench,
    action: wire::Action,
    operation: &str,
    correlation: &str,
) -> Vec<u8> {
    frame(host, action, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_operation(operation);
        r.set_transfer(correlation);
    })
}

#[test]
fn durable_proposal_wire_segments_full_record_and_lists_only_summary() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = seed(&directory.path().join("workbench.db"));
    let (mut parent, asset) = parent_with_asset(&mut host);
    parent.operation_id = "large-parent-snapshot".into();
    parent.expected_generation = 1;
    parent.assets[0].origin = 3;
    parent
        .values
        .as_mut()
        .unwrap()
        .description
        .as_mut()
        .unwrap()
        .text = "large S2".repeat(12000);
    host.save_editor_draft(&parent).unwrap();
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset);
    let binding = link(&parent, &child, &digest);
    let body = proposal_body(&child, &binding);
    assert!(body.len() > 32768);

    let wrong_token = upload(&mut host, &child.operation_id, &body);
    let wrong = frame(
        &mut host,
        wire::Action::PrepareEditorDraftHandoffProposal,
        |mut r| {
            r.set_id(CARD);
            r.set_attachment("wrong-parent");
            r.set_operation(&child.operation_id);
            r.set_transfer(&wrong_token);
        },
    );
    assert!(!error(&wrong).is_empty());
    assert!(
        host.inspect_editor_draft_handoff_proposal(CARD, DRAFT, &child.operation_id)
            .unwrap()
            .is_none()
    );

    let upload_token = upload(&mut host, &child.operation_id, &body);
    let prepared = proposal_action(
        &mut host,
        wire::Action::PrepareEditorDraftHandoffProposal,
        &child.operation_id,
        &upload_token,
    );
    let prepared_bytes = download(&mut host, &prepared);
    let prepared_message = record(&prepared_bytes);
    let envelope = prepared_message
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap();
    assert_eq!(
        envelope.get_kind().unwrap(),
        draft_wire::ResultKind::HandoffProposal
    );
    assert_eq!(envelope.get_card_id().unwrap().to_str().unwrap(), CARD);
    assert_eq!(envelope.get_draft_id().unwrap().to_str().unwrap(), DRAFT);
    assert_eq!(
        envelope.get_operation().unwrap().to_str().unwrap(),
        child.operation_id
    );
    assert_eq!(envelope.get_expected_generation(), 0);
    assert!(!envelope.has_request_cursor());
    assert_eq!(envelope.get_request_limit(), 0);
    assert!(!envelope.has_next_cursor());
    let item = envelope.get_handoff_proposal().unwrap();
    let summary = item.get_summary().unwrap();
    assert_eq!(summary.get_status(), 0);
    assert_eq!(summary.get_revision(), 1);
    assert_eq!(
        summary.get_child_draft_id().unwrap().to_str().unwrap(),
        CHILD
    );
    assert_eq!(
        item.get_proposal()
            .unwrap()
            .get_request()
            .unwrap()
            .get_values()
            .unwrap()
            .get_description()
            .unwrap()
            .get_text()
            .unwrap()
            .to_str()
            .unwrap()
            .len(),
        96000
    );
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
    assert_eq!(business_revision(&host), 3);

    let malformed = frame(
        &mut host,
        wire::Action::InspectEditorDraftHandoffProposal,
        |mut r| {
            r.set_id(CARD);
            r.set_attachment(DRAFT);
            r.set_operation(&child.operation_id);
            r.set_transfer("draft-request-1");
            r.set_name("smuggled");
        },
    );
    assert!(!error(&malformed).is_empty());
    let replay = proposal_action(
        &mut host,
        wire::Action::InspectEditorDraftHandoffProposal,
        &child.operation_id,
        "draft-request-1",
    );
    assert!(!error(&replay).is_empty());
    let inspected = proposal_action(
        &mut host,
        wire::Action::InspectEditorDraftHandoffProposal,
        &child.operation_id,
        "draft-request-2",
    );
    assert_ok(&inspected);
    assert_eq!(
        record(&download(&mut host, &inspected))
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_handoff_proposal()
            .unwrap()
            .get_summary()
            .unwrap()
            .get_status(),
        0
    );

    let page = frame(
        &mut host,
        wire::Action::ListEditorDraftHandoffProposals,
        |mut r| {
            r.set_transfer("draft-request-3");
            r.set_limit(1);
        },
    );
    let page_bytes = download(&mut host, &page);
    assert!(page_bytes.len() < 4096);
    let page_message = record(&page_bytes);
    let page = page_message
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap();
    assert_eq!(
        page.get_kind().unwrap(),
        draft_wire::ResultKind::HandoffProposals
    );
    assert!(!page.has_handoff_proposal());
    assert_eq!(page.get_request_limit(), 1);
    assert_eq!(page.get_handoff_proposal_summaries().unwrap().len(), 1);

    let completed = proposal_action(
        &mut host,
        wire::Action::CompleteEditorDraftHandoffProposal,
        &child.operation_id,
        "draft-request-4",
    );
    assert_eq!(
        record(&download(&mut host, &completed))
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_handoff_proposal()
            .unwrap()
            .get_summary()
            .unwrap()
            .get_status(),
        1
    );
    let retired = proposal_action(
        &mut host,
        wire::Action::RetireEditorDraftHandoffProposal,
        &child.operation_id,
        "draft-request-5",
    );
    assert_eq!(
        record(&download(&mut host, &retired))
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_handoff_proposal()
            .unwrap()
            .get_summary()
            .unwrap()
            .get_status(),
        2
    );
    assert_eq!(business_revision(&host), 3);
    assert!(
        !error(&proposal_action(
            &mut host,
            wire::Action::CancelEditorDraftHandoffProposal,
            &child.operation_id,
            "draft-request-6"
        ))
        .is_empty()
    );
}

#[test]
fn proposal_wire_cancel_and_exact_upload_reply_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let (parent, asset) = parent_with_asset(&mut host);
    let digest = commit_real_s1(&mut host);
    let child = child_from(&parent, &asset);
    let body = proposal_body(&child, &link(&parent, &child, &digest));
    let upload_token = upload(&mut host, &child.operation_id, &body);
    let prepared = proposal_action(
        &mut host,
        wire::Action::PrepareEditorDraftHandoffProposal,
        &child.operation_id,
        &upload_token,
    );
    assert_ok(&prepared);
    let download_token = response(&prepared)
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_transfer()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| {
            r.set_transfer("draft-request-999");
        },
    ));
    assert!(!download(&mut host, &prepared).is_empty());
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| {
            r.set_transfer(&upload_token);
        },
    ));
    let stale = frame(&mut host, wire::Action::ReadEditorDraftPart, |mut r| {
        r.set_transfer(&download_token);
    });
    assert!(!error(&stale).is_empty());
    let cancelled = proposal_action(
        &mut host,
        wire::Action::CancelEditorDraftHandoffProposal,
        &child.operation_id,
        "draft-request-1",
    );
    assert_eq!(
        record(&download(&mut host, &cancelled))
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_handoff_proposal()
            .unwrap()
            .get_summary()
            .unwrap()
            .get_status(),
        3
    );
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    let inspected = proposal_action(
        &mut host,
        wire::Action::InspectEditorDraftHandoffProposal,
        &child.operation_id,
        "draft-request-1",
    );
    assert_eq!(
        record(&download(&mut host, &inspected))
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_handoff_proposal()
            .unwrap()
            .get_summary()
            .unwrap()
            .get_status(),
        3
    );
    assert!(host.read_editor_draft(CARD, CHILD).unwrap().is_none());
}
