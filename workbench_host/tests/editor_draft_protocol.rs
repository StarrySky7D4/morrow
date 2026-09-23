#![cfg(target_os = "windows")]
//! Exercises only the trusted desktop draft wire route against a real host.
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_core::plugin_package::{
    proto::{Capability, TransformHandler},
    Package,
};
use morrow_workbench_host::{
    editor_draft_api_capnp as draft_wire, host_capnp as wire, protocol,
    versioned_record::VersionedRecord, Workbench,
};
use morrow_workbench_plugin::{Idea, PACKAGE_VERSION};
use sha2::{Digest, Sha256};
use std::path::Path;

const CARD: &str = "draft-wire-card";
const DRAFT: &str = "draft-wire-one";
const BYTES: &[u8] = b"private draft export survives removed source\x00\xff";

fn package() -> Package {
    let module =
        std::fs::read(std::env::var("MORROW_WORKBENCH_WASM").expect("compiled Rust guest path"))
            .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "test.editor-draft-protocol",
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
        "draft-wire-seed",
        Idea {
            id: CARD.into(),
            title: "Business title".into(),
            description: "Business body".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: vec!["TaskId row".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let migration = host.plan_tasks_migration(CARD).unwrap();
    host.migrate_tasks(&migration.operation, CARD, migration.source_revision)
        .unwrap();
    host
}

fn business_revision(host: &Workbench) -> u64 {
    match host.read_versioned(CARD).unwrap() {
        VersionedRecord::Tasks(card) => card.revision,
        VersionedRecord::Legacy(_) => panic!("expected migrated V2 card"),
    }
}

fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/editor_draft_api.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
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
    serialize::read_message(&mut bytes.as_ref(), ReaderOptions::new()).unwrap()
}
fn error(bytes: &[u8]) -> String {
    response(bytes)
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_error()
        .unwrap()
        .to_str()
        .unwrap()
        .into()
}
fn assert_ok(bytes: &[u8]) {
    assert!(error(bytes).is_empty(), "host error: {}", error(bytes));
}
fn set_text(
    mut out: draft_wire::text_value::Builder<'_>,
    text: &str,
    base: i32,
    extent: i32,
    composing: (i32, i32),
) {
    out.set_text(text);
    out.set_selection_base(base);
    out.set_selection_extent(extent);
    out.set_affinity(0);
    out.set_directional(false);
    out.set_composing_start(composing.0);
    out.set_composing_end(composing.1);
}
fn write_request(draft: &str, operation: &str, generation: u64, description: &str) -> Vec<u8> {
    write_request_custom(
        CARD,
        draft,
        operation,
        generation,
        description,
        None,
        true,
        None,
    )
}
fn write_request_custom(
    card: &str,
    draft: &str,
    operation: &str,
    generation: u64,
    description: &str,
    asset: Option<(u16, &str)>,
    valid_digest: bool,
    selection_override: Option<i32>,
) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<draft_wire::write_request::Builder>();
    request.set_version(1);
    let schema_digest = if valid_digest { digest() } else { [0u8; 32] };
    request.set_digest(&schema_digest);
    request.set_card_id(card);
    request.set_draft_id(draft);
    request.set_operation(operation);
    request.set_expected_generation(generation);
    request.set_source_revision(2);
    request.set_source_kind(0);
    let mut values = request.reborrow().init_values();
    set_text(values.reborrow().init_title(), "", 0, 0, (-1, -1));
    let extent = description.encode_utf16().count() as i32;
    set_text(
        values.reborrow().init_description(),
        description,
        selection_override.unwrap_or(extent),
        extent,
        (-1, -1),
    );
    set_text(values.reborrow().init_hypothesis(), "", 0, 0, (-1, -1));
    set_text(values.reborrow().init_conclusion(), "", 0, 0, (-1, -1));
    set_text(values.reborrow().init_todos(), "", 0, 0, (-1, -1));
    values.set_category("进行中");
    values.set_stage("计划中");
    if let Some((origin, id)) = asset {
        let mut items = request.reborrow().init_assets(1);
        let mut selected = items.reborrow().get(0);
        selected.set_origin(origin);
        selected.set_asset_id(id);
    }
    serialize::write_message_to_words(&message)
}
fn begin_upload(host: &mut Workbench, operation: &str, bytes: &[u8]) -> String {
    let reply = frame(host, wire::Action::BeginEditorDraft, |mut r| {
        r.set_operation(operation);
        r.set_total_length(bytes.len() as u64);
        r.set_sha256(&Sha256::digest(bytes));
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
    assert!(!token.is_empty());
    for (index, piece) in bytes.chunks(32768).enumerate() {
        let appended = frame(host, wire::Action::AppendEditorDraft, |mut r| {
            r.set_transfer(&token);
            r.set_offset((index * 32768) as u64);
            r.set_payload(piece);
        });
        assert_ok(&appended);
    }
    token
}
fn finish(
    host: &mut Workbench,
    token: &str,
    draft: &str,
    operation: &str,
    generation: u64,
) -> Vec<u8> {
    finish_outer(host, token, CARD, draft, operation, generation)
}
fn finish_outer(
    host: &mut Workbench,
    token: &str,
    card: &str,
    draft: &str,
    operation: &str,
    generation: u64,
) -> Vec<u8> {
    frame(host, wire::Action::FinishEditorDraft, |mut r| {
        r.set_transfer(token);
        r.set_id(card);
        r.set_attachment(draft);
        r.set_operation(operation);
        r.set_revision(generation);
    })
}
fn envelope(bytes: &[u8]) -> capnp::message::Reader<serialize::OwnedSegments> {
    serialize::read_message(&mut bytes.as_ref(), ReaderOptions::new()).unwrap()
}

fn download(host: &mut Workbench, first: &[u8]) -> Vec<u8> {
    assert_ok(first);
    let message = response(first);
    let head = message.get_root::<wire::response::Reader>().unwrap();
    let token = head.get_transfer().unwrap().to_str().unwrap().to_owned();
    let total = head.get_total_length() as usize;
    let sha = head.get_sha256().unwrap().to_vec();
    assert_eq!(head.get_offset(), 0);
    let revision = head.get_revision();
    let mut all = head.get_payload().unwrap().to_vec();
    assert!(!all.is_empty());
    while all.len() < total {
        let offset = all.len();
        let next = frame(host, wire::Action::ReadEditorDraftPart, |mut r| {
            r.set_transfer(&token);
            r.set_offset(offset as u64);
        });
        assert_ok(&next);
        let part_message = response(&next);
        let part = part_message.get_root::<wire::response::Reader>().unwrap();
        assert_eq!(part.get_transfer().unwrap().to_str().unwrap(), token);
        assert_eq!(part.get_offset(), offset as u64);
        assert_eq!(part.get_total_length() as usize, total);
        assert_eq!(part.get_revision(), revision);
        assert_eq!(part.get_sha256().unwrap(), sha);
        let piece = part.get_payload().unwrap();
        assert!(!piece.is_empty());
        assert!(piece.len() <= 32768);
        all.extend_from_slice(piece);
    }
    assert_eq!(all.len(), total);
    assert_eq!(Sha256::digest(&all).as_slice(), sha);
    all
}
fn decoded_record(bytes: &[u8]) -> capnp::message::Reader<serialize::OwnedSegments> {
    envelope(bytes)
}

fn read_draft(host: &mut Workbench, draft: &str) -> Vec<u8> {
    let first = frame(host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(draft);
    });
    download(host, &first)
}
fn list_drafts(host: &mut Workbench) -> Vec<u8> {
    let first = frame(host, wire::Action::ListEditorDrafts, |_| {});
    download(host, &first)
}

#[test]
fn segmented_large_draft_reads_lists_retries_discards_and_abort_cannot_rewind() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let raw = format!("raw😀{}", "S2 unfinished. ".repeat(12000));
    let first_request = write_request(DRAFT, "wire-large-first", 0, &raw);
    assert!(first_request.len() > 128 * 1024);
    let first_token = begin_upload(&mut host, "wire-large-first", &first_request);
    let first_reply = finish(&mut host, &first_token, DRAFT, "wire-large-first", 0);
    let first_bytes = download(&mut host, &first_reply);
    let message = decoded_record(&first_bytes);
    let result = message.get_root::<draft_wire::envelope::Reader>().unwrap();
    assert_eq!(result.get_kind().unwrap(), draft_wire::ResultKind::Record);
    assert_eq!(result.get_card_id().unwrap().to_str().unwrap(), CARD);
    assert_eq!(result.get_draft_id().unwrap().to_str().unwrap(), DRAFT);
    let record = result.get_record().unwrap();
    assert_eq!(record.get_generation(), 1);
    assert_eq!(record.get_current_generation(), 1);
    assert!(record.get_active());
    assert!(!record.get_repeated());
    let fields = record.get_request().unwrap().get_values().unwrap();
    assert_eq!(
        fields
            .get_title()
            .unwrap()
            .get_text()
            .unwrap()
            .to_str()
            .unwrap(),
        ""
    );
    assert_eq!(
        fields
            .get_description()
            .unwrap()
            .get_text()
            .unwrap()
            .to_str()
            .unwrap(),
        raw
    );
    assert_eq!(business_revision(&host), 2);

    let started_read = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
    });
    assert_ok(&started_read);
    let overlapping_list = frame(&mut host, wire::Action::ListEditorDrafts, |_| {});
    assert!(
        !error(&overlapping_list).is_empty(),
        "a second download must not replace the first transfer's revision"
    );
    let read = download(&mut host, &started_read);
    let message = envelope(&read);
    let result = message.get_root::<draft_wire::envelope::Reader>().unwrap();
    assert_eq!(result.get_kind().unwrap(), draft_wire::ResultKind::Record);
    assert_eq!(result.get_record().unwrap().get_generation(), 1);
    let listed = list_drafts(&mut host);
    let message = envelope(&listed);
    let result = message.get_root::<draft_wire::envelope::Reader>().unwrap();
    assert_eq!(result.get_kind().unwrap(), draft_wire::ResultKind::List);
    let summaries = result.get_summaries().unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(
        summaries.get(0).get_draft_id().unwrap().to_str().unwrap(),
        DRAFT
    );

    let second_request = write_request(DRAFT, "wire-second", 1, "S2 newer raw");
    let second_token = begin_upload(&mut host, "wire-second", &second_request);
    let second_reply = finish(&mut host, &second_token, DRAFT, "wire-second", 1);
    download(&mut host, &second_reply);
    let old_token = begin_upload(&mut host, "wire-large-first", &first_request);
    let old_reply = finish(&mut host, &old_token, DRAFT, "wire-large-first", 0);
    let old_bytes = download(&mut host, &old_reply);
    let message = envelope(&old_bytes);
    let historical = message
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap()
        .get_record()
        .unwrap();
    assert!(historical.get_repeated());
    assert_eq!(historical.get_generation(), 1);
    assert_eq!(historical.get_current_generation(), 2);
    assert!(historical.get_current_active());
    let current = read_draft(&mut host, DRAFT);
    assert_eq!(
        envelope(&current)
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_record()
            .unwrap()
            .get_generation(),
        2
    );

    let third_request = write_request(DRAFT, "wire-aborted-third", 2, "never saved");
    let third_token = begin_upload(&mut host, "wire-aborted-third", &third_request);
    let aborted = frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer(&third_token),
    );
    assert_ok(&aborted);
    let current = read_draft(&mut host, DRAFT);
    assert_eq!(
        envelope(&current)
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_record()
            .unwrap()
            .get_generation(),
        2
    );

    let discarded = frame(&mut host, wire::Action::DiscardEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_operation("wire-discard");
        r.set_revision(2);
    });
    let discarded = download(&mut host, &discarded);
    let message = envelope(&discarded);
    let inactive = message
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap()
        .get_record()
        .unwrap();
    assert!(!inactive.get_active());
    assert!(!inactive.get_current_active());
    assert_eq!(inactive.get_generation(), 3);
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn owner_route_rejects_outer_inner_binding_digest_and_transfer_offset() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    for case in 0..4 {
        let outer_operation = format!("wire-identity-{case}");
        let inner_card = if case == 0 { "foreign-card" } else { CARD };
        let inner_draft = if case == 1 { "foreign-draft" } else { DRAFT };
        let inner_operation = if case == 2 {
            "foreign-operation"
        } else {
            &outer_operation
        };
        let inner_generation = if case == 3 { 1 } else { 0 };
        let raw = write_request_custom(
            inner_card,
            inner_draft,
            inner_operation,
            inner_generation,
            "raw",
            None,
            true,
            None,
        );
        let token = begin_upload(&mut host, &outer_operation, &raw);
        let reply = finish_outer(&mut host, &token, CARD, DRAFT, &outer_operation, 0);
        assert!(!error(&reply).is_empty(), "mismatch case {case} accepted");
    }
    let bad_digest =
        write_request_custom(CARD, DRAFT, "wire-bad-digest", 0, "raw", None, false, None);
    let token = begin_upload(&mut host, "wire-bad-digest", &bad_digest);
    assert!(!error(&finish(&mut host, &token, DRAFT, "wire-bad-digest", 0)).is_empty());
    let bad_offset = write_request_custom(
        CARD,
        DRAFT,
        "wire-bad-text-offset",
        0,
        "A😀B",
        None,
        true,
        Some(99),
    );
    let token = begin_upload(&mut host, "wire-bad-text-offset", &bad_offset);
    assert!(!error(&finish(&mut host, &token, DRAFT, "wire-bad-text-offset", 0)).is_empty());

    let valid = write_request(DRAFT, "wire-bad-offset", 0, "raw");
    let reply = frame(&mut host, wire::Action::BeginEditorDraft, |mut r| {
        r.set_operation("wire-bad-offset");
        r.set_total_length(valid.len() as u64);
        r.set_sha256(&Sha256::digest(&valid));
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
    let wrong = frame(&mut host, wire::Action::AppendEditorDraft, |mut r| {
        r.set_transfer(&token);
        r.set_offset(1);
        r.set_payload(&valid[..valid.len().min(1024)]);
    });
    assert!(!error(&wrong).is_empty());
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer(&token),
    ));
    let absent = read_draft(&mut host, DRAFT);
    let message = envelope(&absent);
    assert_eq!(
        message
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_kind()
            .unwrap(),
        draft_wire::ResultKind::Absent
    );
    assert_eq!(business_revision(&host), 2);
}

fn import_asset(host: &mut Workbench, draft: &str, source: &Path, name: &str) -> String {
    let reply = frame(host, wire::Action::ImportEditorDraftAsset, |mut r| {
        r.set_id(CARD);
        r.set_attachment(draft);
        r.set_revision(0);
        r.set_name(name);
        r.set_kind("file");
        r.set_selected_path(source.to_str().unwrap());
        r.set_total_length(std::fs::metadata(source).unwrap().len());
    });
    let bytes = download(host, &reply);
    let message = envelope(&bytes);
    let result = message.get_root::<draft_wire::envelope::Reader>().unwrap();
    assert_eq!(result.get_kind().unwrap(), draft_wire::ResultKind::Imported);
    assert_eq!(result.get_draft_id().unwrap().to_str().unwrap(), draft);
    result
        .get_asset()
        .unwrap()
        .get_id()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn scoped_import_cannot_cross_draft_and_package_free_export_uses_pinned_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let source = directory.path().join("selected-original.bin");
    std::fs::write(&source, BYTES).unwrap();
    let mut host = seed(&path);
    let foreign = import_asset(&mut host, "other-draft", &source, "other.bin");
    let wrong = write_request_custom(
        CARD,
        DRAFT,
        "wire-cross-draft",
        0,
        "raw",
        Some((2, &foreign)),
        true,
        None,
    );
    let token = begin_upload(&mut host, "wire-cross-draft", &wrong);
    assert!(!error(&finish(&mut host, &token, DRAFT, "wire-cross-draft", 0)).is_empty());
    let absence = read_draft(&mut host, DRAFT);
    assert_eq!(
        envelope(&absence)
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_kind()
            .unwrap(),
        draft_wire::ResultKind::Absent
    );

    let asset = import_asset(&mut host, DRAFT, &source, "selected.bin");
    let good = write_request_custom(
        CARD,
        DRAFT,
        "wire-asset-save",
        0,
        "attachment selected",
        Some((2, &asset)),
        true,
        None,
    );
    let token = begin_upload(&mut host, "wire-asset-save", &good);
    let saved = finish(&mut host, &token, DRAFT, "wire-asset-save", 0);
    let saved = download(&mut host, &saved);
    let message = envelope(&saved);
    let record = message
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap()
        .get_record()
        .unwrap();
    assert_eq!(record.get_assets().unwrap().len(), 1);
    assert_eq!(
        record
            .get_assets()
            .unwrap()
            .get(0)
            .get_selection()
            .unwrap()
            .get_asset_id()
            .unwrap()
            .to_str()
            .unwrap(),
        asset
    );
    assert_eq!(business_revision(&host), 2);
    drop(host);
    std::fs::remove_file(&source).unwrap();

    let mut host = Workbench::open(&path, None).unwrap();
    let read = read_draft(&mut host, DRAFT);
    assert_eq!(
        envelope(&read)
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_record()
            .unwrap()
            .get_generation(),
        1
    );
    let exported_path = directory.path().join("exported-copy.bin");
    let exported = frame(&mut host, wire::Action::ExportEditorDraftAsset, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_revision(1);
        r.set_name(&asset);
        r.set_selected_path(exported_path.to_str().unwrap());
    });
    let exported = download(&mut host, &exported);
    assert_eq!(
        envelope(&exported)
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_kind()
            .unwrap(),
        draft_wire::ResultKind::Exported
    );
    assert_eq!(std::fs::read(&exported_path).unwrap(), BYTES);
    assert_eq!(business_revision(&host), 2);
}

#[test]
fn lost_finish_reply_can_be_aborted_by_original_upload_token_without_killing_later_read() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let raw = "unfinished S2 ".repeat(9000);
    let body = write_request(DRAFT, "wire-lost-finish", 0, &raw);
    let upload = begin_upload(&mut host, "wire-lost-finish", &body);
    let lost_reply = finish(&mut host, &upload, DRAFT, "wire-lost-finish", 0);
    assert_ok(&lost_reply);
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        1
    );
    assert_eq!(business_revision(&host), 2);
    // The first response chunk is deliberately lost. A foreign token cannot
    // cancel its pending download; it remains busy until the original owner
    // explicitly aborts the paired reply transfer.
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer("unknown-upload-token"),
    ));
    let busy = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
    });
    assert!(!error(&busy).is_empty());
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer(&upload),
    ));
    let current = read_draft(&mut host, DRAFT);
    let message = envelope(&current);
    let record = message
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap()
        .get_record()
        .unwrap();
    assert_eq!(record.get_generation(), 1);
    assert_eq!(
        record
            .get_request()
            .unwrap()
            .get_values()
            .unwrap()
            .get_description()
            .unwrap()
            .get_text()
            .unwrap()
            .to_str()
            .unwrap(),
        raw
    );

    // Once the paired transfer is retired, the old upload token must never
    // become authority over a subsequent read transfer in the same session.
    let later = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
    });
    assert_ok(&later);
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer(&upload),
    ));
    let later_bytes = download(&mut host, &later);
    assert_eq!(
        envelope(&later_bytes)
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_record()
            .unwrap()
            .get_generation(),
        1
    );
}

#[test]
fn lost_read_reply_correlation_is_exact_single_use_and_monotonic() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let raw = "unfinished S2 ".repeat(9000);
    let body = write_request(DRAFT, "wire-correlated-read-save", 0, &raw);
    let upload = begin_upload(&mut host, "wire-correlated-read-save", &body);
    let saved = finish(&mut host, &upload, DRAFT, "wire-correlated-read-save", 0);
    download(&mut host, &saved);

    // The first chunk is lost while its large reply remains open. Only the
    // exact request correlation can release that download in this connection.
    let lost = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_transfer("draft-request-0");
    });
    assert_ok(&lost);
    assert!(
        response(&lost)
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_total_length()
            > 32768
    );
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer("draft-request-999"),
    ));
    let busy = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_transfer("draft-request-1");
    });
    assert!(!error(&busy).is_empty());
    // A failed request consumes its sequence but owns no reply.
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer("draft-request-1"),
    ));
    let still_busy = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_transfer("draft-request-2");
    });
    assert!(!error(&still_busy).is_empty());
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer("draft-request-0"),
    ));

    let later = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_transfer("draft-request-3");
    });
    assert_ok(&later);
    // Replaying an old key cannot replace or abort the newer download.
    for stale in ["draft-request-3", "draft-request-2", "draft-request-0"] {
        let replay = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
            r.set_id(CARD);
            r.set_attachment(DRAFT);
            r.set_transfer(stale);
        });
        assert!(
            error(&replay).contains("replay"),
            "stale {stale}: {}",
            error(&replay)
        );
    }
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer("draft-request-0"),
    ));
    let latest = download(&mut host, &later);
    let latest_envelope = envelope(&latest);
    let record = latest_envelope
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap()
        .get_record()
        .unwrap();
    assert_eq!(record.get_generation(), 1);
    assert_eq!(
        record
            .get_request()
            .unwrap()
            .get_values()
            .unwrap()
            .get_description()
            .unwrap()
            .get_text()
            .unwrap()
            .to_str()
            .unwrap(),
        raw
    );
    assert_eq!(business_revision(&host), 2);

    for invalid in [
        "upload-1",
        "draft-request-04",
        "draft-request-",
        "draft-request-18446744073709551616",
    ] {
        let bad = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
            r.set_id(CARD);
            r.set_attachment(DRAFT);
            r.set_transfer(invalid);
        });
        assert!(
            !error(&bad).is_empty(),
            "invalid key {invalid} was accepted"
        );
    }
    // The invalid keys did not consume the next canonical sequence.
    let valid = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_transfer("draft-request-4");
    });
    assert_eq!(
        envelope(&download(&mut host, &valid))
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_record()
            .unwrap()
            .get_generation(),
        1
    );
}

#[test]
fn lost_discard_reply_correlation_reconciles_original_without_mutating_business() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let raw = "draft to discard ".repeat(9000);
    let body = write_request(DRAFT, "wire-correlated-discard-save", 0, &raw);
    let upload = begin_upload(&mut host, "wire-correlated-discard-save", &body);
    let saved = finish(&mut host, &upload, DRAFT, "wire-correlated-discard-save", 0);
    download(&mut host, &saved);

    let lost = frame(&mut host, wire::Action::DiscardEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_operation("wire-correlated-discard");
        r.set_revision(1);
        r.set_transfer("draft-request-10");
    });
    assert_ok(&lost);
    assert!(
        response(&lost)
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_total_length()
            > 32768
    );
    assert_eq!(
        host.read_editor_draft(CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        2
    );
    assert_eq!(business_revision(&host), 2);

    // A collision with upload-token syntax is rejected before it can affect
    // the live transfer or the already committed discard.
    let collision = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_transfer("upload-1");
    });
    assert!(!error(&collision).is_empty());
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer("draft-request-99"),
    ));
    let busy = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_transfer("draft-request-11");
    });
    assert!(!error(&busy).is_empty());
    assert_ok(&frame(
        &mut host,
        wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer("draft-request-10"),
    ));

    let retry = frame(&mut host, wire::Action::DiscardEditorDraft, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_operation("wire-correlated-discard");
        r.set_revision(1);
        r.set_transfer("draft-request-12");
    });
    let bytes = download(&mut host, &retry);
    let bytes_envelope = envelope(&bytes);
    let record = bytes_envelope
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap()
        .get_record()
        .unwrap();
    assert!(!record.get_active());
    assert!(!record.get_current_active());
    assert!(record.get_repeated());
    assert_eq!(record.get_generation(), 2);
    assert_eq!(record.get_current_generation(), 2);
    assert_eq!(business_revision(&host), 2);
}

const NEW_CARD: &str = "draft-wire-new-card";

fn write_new_card_request(operation: &str, generation: u64, origin: Option<u16>) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<draft_wire::write_request::Builder>();
    request.set_version(1);
    request.set_digest(&digest());
    request.set_card_id(NEW_CARD);
    request.set_draft_id(DRAFT);
    request.set_operation(operation);
    request.set_expected_generation(generation);
    request.set_source_revision(0);
    request.set_source_kind(1);
    let mut values = request.reborrow().init_values();
    set_text(values.reborrow().init_title(), "", 0, 0, (-1, -1));
    set_text(values.reborrow().init_description(), "A😀B", 2, 3, (1, 3));
    set_text(values.reborrow().init_hypothesis(), "", 0, 0, (-1, -1));
    set_text(values.reborrow().init_conclusion(), "", 0, 0, (-1, -1));
    set_text(values.reborrow().init_todos(), "", 0, 0, (-1, -1));
    values.set_category("进行中");
    values.set_stage("计划中");
    if let Some(origin) = origin {
        let mut assets = request.reborrow().init_assets(1);
        let mut selected = assets.reborrow().get(0);
        selected.set_origin(origin);
        selected.set_asset_id("forbidden-source-asset");
    }
    serialize::write_message_to_words(&message)
}

#[test]
fn new_card_wire_save_is_private_and_exact_retry_cannot_resurrect_discarded_slot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let invalid = write_new_card_request("wire-new-card-origin-zero", 0, Some(0));
    let token = begin_upload(&mut host, "wire-new-card-origin-zero", &invalid);
    assert!(!error(&finish_outer(
        &mut host,
        &token,
        NEW_CARD,
        DRAFT,
        "wire-new-card-origin-zero",
        0
    ))
    .is_empty());
    assert!(host.read_editor_draft(NEW_CARD, DRAFT).unwrap().is_none());

    let original = write_new_card_request("wire-new-card-original", 0, None);
    let wrong_outer = begin_upload(&mut host, "wire-new-card-original", &original);
    assert!(!error(&finish_outer(
        &mut host,
        &wrong_outer,
        CARD,
        DRAFT,
        "wire-new-card-original",
        0
    ))
    .is_empty());
    assert!(host.read_editor_draft(NEW_CARD, DRAFT).unwrap().is_none());
    let token = begin_upload(&mut host, "wire-new-card-original", &original);
    let reply = finish_outer(
        &mut host,
        &token,
        NEW_CARD,
        DRAFT,
        "wire-new-card-original",
        0,
    );
    let bytes = download(&mut host, &reply);
    let parsed = envelope(&bytes);
    let result = parsed.get_root::<draft_wire::envelope::Reader>().unwrap();
    assert_eq!(result.get_kind().unwrap(), draft_wire::ResultKind::Record);
    assert_eq!(result.get_card_id().unwrap().to_str().unwrap(), NEW_CARD);
    let record = result.get_record().unwrap();
    assert_eq!(record.get_generation(), 1);
    assert_eq!(record.get_source_format(), 0);
    assert!(record.get_source_sha256().unwrap().is_empty());
    let request = record.get_request().unwrap();
    assert_eq!(request.get_source_kind(), 1);
    assert_eq!(request.get_source_revision(), 0);
    assert_eq!(
        request
            .get_values()
            .unwrap()
            .get_description()
            .unwrap()
            .get_selection_base(),
        2
    );
    assert!(host.read_versioned(NEW_CARD).is_err());
    assert!(host.page_versioned("", 100).unwrap().0.is_empty());

    let read = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(NEW_CARD);
        r.set_attachment(DRAFT);
    });
    let bytes = download(&mut host, &read);
    let parsed = envelope(&bytes);
    let record = parsed
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap()
        .get_record()
        .unwrap();
    assert_eq!(record.get_request().unwrap().get_source_kind(), 1);
    assert_eq!(record.get_generation(), 1);
    let discard = frame(&mut host, wire::Action::DiscardEditorDraft, |mut r| {
        r.set_id(NEW_CARD);
        r.set_attachment(DRAFT);
        r.set_operation("wire-new-card-discard");
        r.set_revision(1);
    });
    let bytes = download(&mut host, &discard);
    let parsed = envelope(&bytes);
    assert!(!parsed
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap()
        .get_record()
        .unwrap()
        .get_active());
    let old_token = begin_upload(&mut host, "wire-new-card-original", &original);
    let old = finish_outer(
        &mut host,
        &old_token,
        NEW_CARD,
        DRAFT,
        "wire-new-card-original",
        0,
    );
    let bytes = download(&mut host, &old);
    let parsed = envelope(&bytes);
    let old = parsed
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap()
        .get_record()
        .unwrap();
    assert!(old.get_repeated());
    assert_eq!(old.get_generation(), 1);
    assert_eq!(old.get_current_generation(), 2);
    assert!(!old.get_current_active());
    assert!(host.read_versioned(NEW_CARD).is_err());
    drop(host);

    let mut host = Workbench::open(&path, None).unwrap();
    let read = frame(&mut host, wire::Action::ReadEditorDraft, |mut r| {
        r.set_id(NEW_CARD);
        r.set_attachment(DRAFT);
    });
    let bytes = download(&mut host, &read);
    let parsed = envelope(&bytes);
    assert!(!parsed
        .get_root::<draft_wire::envelope::Reader>()
        .unwrap()
        .get_record()
        .unwrap()
        .get_active());
    assert!(host.page_versioned("", 100).unwrap().0.is_empty());
}
