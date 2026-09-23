//! Typed private draft transport. Protobuf journal bytes remain host-owned.
use crate::{Result, WorkbenchState, editor_draft, editor_draft_api_capnp as wire, host_capnp};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use editor_draft::model::{proto, validate_request};
use morrow_core::content::CardRecord;
use morrow_workbench_plugin::Asset;
use prost::Message;
use sha2::{Digest, Sha256};
use std::{io::Cursor, path::Path};

const MAX_WRITE_FRAME: usize = crate::transfer::MAX_DRAFT_BYTES;
const MAX_ENVELOPE: usize = crate::transfer::MAX_DRAFT_BYTES;

pub fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/editor_draft_api.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}

fn text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    Ok(value?.to_str()?.to_owned())
}

fn text_value(value: wire::text_value::Reader<'_>) -> Result<proto::TextValue> {
    Ok(proto::TextValue {
        text: text(value.get_text())?,
        selection_base: value.get_selection_base(),
        selection_extent: value.get_selection_extent(),
        affinity: value.get_affinity().into(),
        directional: value.get_directional(),
        composing_start: value.get_composing_start(),
        composing_end: value.get_composing_end(),
    })
}

fn values(value: wire::values::Reader<'_>) -> Result<proto::Values> {
    if !value.has_title()
        || !value.has_description()
        || !value.has_hypothesis()
        || !value.has_conclusion()
        || !value.has_todos()
    {
        return Err("editor draft text fields missing".into());
    }
    Ok(proto::Values {
        title: Some(text_value(value.get_title()?)?),
        description: Some(text_value(value.get_description()?)?),
        hypothesis: Some(text_value(value.get_hypothesis()?)?),
        conclusion: Some(text_value(value.get_conclusion()?)?),
        todos: Some(text_value(value.get_todos()?)?),
        category: text(value.get_category())?,
        stage: text(value.get_stage())?,
    })
}

fn selected(value: wire::asset_selection::Reader<'_>) -> Result<proto::AssetSelection> {
    let aliases = value.get_aliases()?;
    if aliases.len() > 8 {
        return Err("editor draft alias count".into());
    }
    Ok(proto::AssetSelection {
        origin: u32::from(value.get_origin()),
        asset_id: text(value.get_asset_id())?,
        aliases: aliases.iter().map(text).collect::<Result<Vec<_>>>()?,
    })
}

fn decode_request(value: wire::write_request::Reader<'_>) -> Result<proto::WriteRequest> {
    if value.get_version() != 1 || value.get_digest()? != digest() {
        return Err("editor draft wire contract mismatch".into());
    }
    if !value.has_values() {
        return Err("editor draft values missing".into());
    }
    let assets = value.get_assets()?;
    if assets.len() > 20 {
        return Err("editor draft asset count".into());
    }
    let request = proto::WriteRequest {
        schema_version: 1,
        card_id: text(value.get_card_id())?,
        draft_id: text(value.get_draft_id())?,
        operation_id: text(value.get_operation())?,
        expected_generation: value.get_expected_generation(),
        source_revision: value.get_source_revision(),
        source_kind: u32::from(value.get_source_kind()),
        predecessor_operation: text(value.get_predecessor_operation())?,
        predecessor_sha256: value.get_predecessor_digest()?.to_vec(),
        values: Some(values(value.get_values()?)?),
        assets: assets.iter().map(selected).collect::<Result<Vec<_>>>()?,
    };
    validate_request(&request)?;
    Ok(request)
}

pub(crate) fn decode_write(bytes: &[u8]) -> Result<proto::WriteRequest> {
    if bytes.is_empty() || bytes.len() > MAX_WRITE_FRAME {
        return Err("editor draft wire frame budget".into());
    }
    let mut cursor = Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(4 * 1024 * 1024),
            nesting_limit: 20,
        },
    )?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing editor draft wire bytes".into());
    }
    let request = decode_request(message.get_root::<wire::write_request::Reader>()?)?;
    if request.assets.iter().any(|asset| asset.origin == 4) {
        return Err("parent pins require the explicit handoff upload".into());
    }
    Ok(request)
}

fn decode_parent_link(value: wire::parent_link::Reader<'_>) -> Result<proto::ParentLink> {
    Ok(proto::ParentLink {
        parent_draft_id: text(value.get_parent_draft_id())?,
        parent_generation: value.get_parent_generation(),
        parent_save_operation: text(value.get_parent_save_operation())?,
        parent_request_sha256: value.get_parent_request_sha256()?.to_vec(),
        committed_operation: text(value.get_committed_operation())?,
        committed_sha256: value.get_committed_sha256()?.to_vec(),
        child_operation: text(value.get_child_operation())?,
    })
}

pub(crate) fn decode_handoff(bytes: &[u8]) -> Result<(proto::WriteRequest, proto::ParentLink)> {
    if bytes.is_empty() || bytes.len() > MAX_WRITE_FRAME {
        return Err("editor draft handoff wire frame budget".into());
    }
    let mut cursor = Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(4 * 1024 * 1024),
            nesting_limit: 20,
        },
    )?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing editor draft handoff wire bytes".into());
    }
    let value = message.get_root::<wire::handoff_request::Reader>()?;
    if value.get_version() != 1 || value.get_digest()? != digest() {
        return Err("editor draft handoff wire contract mismatch".into());
    }
    if !value.has_request() || !value.has_parent_link() {
        return Err("editor draft handoff identity missing".into());
    }
    let request = decode_request(value.get_request()?)?;
    let link = decode_parent_link(value.get_parent_link()?)?;
    editor_draft::model::validate_parent_link(&request, &link)?;
    Ok((request, link))
}

fn text_reply(value: &proto::TextValue, mut out: wire::text_value::Builder<'_>) {
    out.set_text(&value.text);
    out.set_selection_base(value.selection_base);
    out.set_selection_extent(value.selection_extent);
    out.set_affinity(value.affinity as u16);
    out.set_directional(value.directional);
    out.set_composing_start(value.composing_start);
    out.set_composing_end(value.composing_end);
}

fn values_reply(value: &proto::Values, mut out: wire::values::Builder<'_>) -> Result<()> {
    text_reply(
        value.title.as_ref().ok_or("draft title missing")?,
        out.reborrow().init_title(),
    );
    text_reply(
        value
            .description
            .as_ref()
            .ok_or("draft description missing")?,
        out.reborrow().init_description(),
    );
    text_reply(
        value
            .hypothesis
            .as_ref()
            .ok_or("draft hypothesis missing")?,
        out.reborrow().init_hypothesis(),
    );
    text_reply(
        value
            .conclusion
            .as_ref()
            .ok_or("draft conclusion missing")?,
        out.reborrow().init_conclusion(),
    );
    text_reply(
        value.todos.as_ref().ok_or("draft todos missing")?,
        out.reborrow().init_todos(),
    );
    out.set_category(&value.category);
    out.set_stage(&value.stage);
    Ok(())
}

fn selected_reply(
    value: &proto::AssetSelection,
    mut out: wire::asset_selection::Builder<'_>,
) -> Result<()> {
    out.set_origin(value.origin.try_into()?);
    out.set_asset_id(&value.asset_id);
    let mut aliases = out.init_aliases(value.aliases.len().try_into()?);
    for (index, alias) in value.aliases.iter().enumerate() {
        aliases.set(index as u32, alias);
    }
    Ok(())
}

fn request_reply(
    value: &proto::WriteRequest,
    mut out: wire::write_request::Builder<'_>,
) -> Result<()> {
    out.set_version(1);
    out.set_digest(&digest());
    out.set_card_id(&value.card_id);
    out.set_draft_id(&value.draft_id);
    out.set_operation(&value.operation_id);
    out.set_expected_generation(value.expected_generation);
    out.set_source_revision(value.source_revision);
    out.set_source_kind(value.source_kind.try_into()?);
    out.set_predecessor_operation(&value.predecessor_operation);
    out.set_predecessor_digest(&value.predecessor_sha256);
    values_reply(
        value.values.as_ref().ok_or("draft values missing")?,
        out.reborrow().init_values(),
    )?;
    let mut assets = out.init_assets(value.assets.len().try_into()?);
    for (index, asset) in value.assets.iter().enumerate() {
        selected_reply(asset, assets.reborrow().get(index as u32))?;
    }
    Ok(())
}

fn parent_link_reply(value: &proto::ParentLink, mut out: wire::parent_link::Builder<'_>) {
    out.set_parent_draft_id(&value.parent_draft_id);
    out.set_parent_generation(value.parent_generation);
    out.set_parent_save_operation(&value.parent_save_operation);
    out.set_parent_request_sha256(&value.parent_request_sha256);
    out.set_committed_operation(&value.committed_operation);
    out.set_committed_sha256(&value.committed_sha256);
    out.set_child_operation(&value.child_operation);
}

fn retirement_reply(
    value: &proto::ParentRetirement,
    mut out: wire::parent_retirement::Builder<'_>,
) {
    out.set_child_draft_id(&value.child_draft_id);
    out.set_child_operation(&value.child_operation);
    out.set_operation(&value.operation_id);
    out.set_parent_generation(value.parent_generation);
}

fn record_reply(
    record: &editor_draft::EditorDraftRecord,
    mut out: wire::record::Builder<'_>,
) -> Result<()> {
    let slot = &record.slot;
    let request = slot.request.as_ref().ok_or("draft request missing")?;
    request_reply(request, out.reborrow().init_request())?;
    out.set_request_sha256(&Sha256::digest(request.encode_to_vec()));
    out.set_generation(slot.generation);
    out.set_active(slot.active);
    out.set_current_generation(record.current_generation);
    out.set_current_active(record.current_active);
    out.set_repeated(record.repeated);
    if request.source_kind == 0 {
        let source = CardRecord::decode(&slot.source_card)?;
        out.set_source_format(source.summary().format_version);
        out.set_source_revision(source.summary().revision);
        out.set_source_sha256(&Sha256::digest(&slot.source_card));
    } else {
        out.set_source_format(0);
        out.set_source_revision(0);
        out.set_source_sha256(&[]);
    }
    if !slot.predecessor_card.is_empty() {
        let predecessor = CardRecord::decode(&slot.predecessor_card)?;
        out.set_predecessor_revision(predecessor.summary().revision);
        out.set_predecessor_sha256(&Sha256::digest(&slot.predecessor_card));
    } else {
        out.set_predecessor_revision(0);
        out.set_predecessor_sha256(&[]);
    }
    let mut assets = out.reborrow().init_assets(slot.assets.len().try_into()?);
    for (index, asset) in slot.assets.iter().enumerate() {
        let mut item = assets.reborrow().get(index as u32);
        selected_reply(
            asset
                .selection
                .as_ref()
                .ok_or("draft asset selection missing")?,
            item.reborrow().init_selection(),
        )?;
        item.set_name(&asset.display_name);
        item.set_media_type(&asset.media_type);
        item.set_bytes(asset.byte_length);
        item.set_sha256(&asset.sha256);
    }
    if let Some(link) = &slot.parent_link {
        parent_link_reply(link, out.reborrow().init_parent_link());
    }
    if let Some(retirement) = &slot.retirement {
        retirement_reply(retirement, out.reborrow().init_retirement());
    }
    Ok(())
}

fn envelope(
    kind: wire::ResultKind,
    card: &str,
    draft: &str,
    operation: &str,
    expected_generation: u64,
    record: Option<&editor_draft::EditorDraftRecord>,
    summaries: &[editor_draft::EditorDraftSummary],
    asset: Option<&Asset>,
) -> Result<Vec<u8>> {
    let mut message = Builder::new_default();
    {
        let mut out = message.init_root::<wire::envelope::Builder>();
        out.set_version(1);
        out.set_digest(&digest());
        out.set_kind(kind);
        out.set_card_id(card);
        out.set_draft_id(draft);
        out.set_operation(operation);
        out.set_expected_generation(expected_generation);
        if let Some(record) = record {
            record_reply(record, out.reborrow().init_record())?;
        }
        if kind == wire::ResultKind::List {
            let mut list = out.reborrow().init_summaries(summaries.len().try_into()?);
            for (index, summary) in summaries.iter().enumerate() {
                let mut item = list.reborrow().get(index as u32);
                item.set_card_id(&summary.card_id);
                item.set_draft_id(&summary.draft_id);
                item.set_generation(summary.generation);
                item.set_active(summary.active);
            }
        }
        if let Some(asset) = asset {
            let mut item = out.init_asset();
            item.set_id(&asset.id);
            item.set_name(&asset.name);
            item.set_kind(&asset.kind);
            item.set_bytes(asset.bytes);
        }
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_ENVELOPE {
        return Err("editor draft envelope budget".into());
    }
    Ok(bytes)
}

fn lineages_envelope(
    lineages: &[editor_draft::EditorDraftLineage],
    request_cursor: &str,
    request_limit: u32,
    next_cursor: &str,
) -> Result<Vec<u8>> {
    let mut message = Builder::new_default();
    {
        let mut out = message.init_root::<wire::envelope::Builder>();
        out.set_version(1);
        out.set_digest(&digest());
        out.set_kind(wire::ResultKind::Lineages);
        out.set_card_id("");
        out.set_draft_id("");
        out.set_operation("");
        out.set_expected_generation(0);
        out.set_request_cursor(request_cursor);
        out.set_request_limit(request_limit);
        out.set_next_cursor(next_cursor);
        let mut items = out.init_lineages(lineages.len().try_into()?);
        for (index, lineage) in lineages.iter().enumerate() {
            let mut item = items.reborrow().get(index as u32);
            item.set_card_id(&lineage.card_id);
            item.set_child_draft_id(&lineage.child_draft_id);
            item.set_child_generation(lineage.child_generation);
            item.set_child_active(lineage.child_active);
            item.set_parent_generation(lineage.parent_generation);
            item.set_parent_active(lineage.parent_active);
            parent_link_reply(&lineage.link, item.reborrow().init_parent_link());
            item.set_cursor(&lineage.cursor);
        }
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_ENVELOPE {
        return Err("editor draft lineage envelope budget".into());
    }
    Ok(bytes)
}

fn valid_lineage_cursor(cursor: &str) -> bool {
    const PREFIX: &str = "morrow-host-editor-draft-";
    cursor.is_empty()
        || cursor.strip_prefix(PREFIX).is_some_and(|suffix| {
            suffix.len() == 64
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

pub(crate) fn chunk(mut out: host_capnp::response::Builder<'_>, part: &crate::transfer::Chunk) {
    out.set_transfer(&part.token);
    out.set_offset(part.offset as u64);
    out.set_total_length(part.total as u64);
    out.set_sha256(&part.sha);
    out.set_payload(&part.bytes);
}

pub(crate) fn envelope_chunk(
    host: &mut WorkbenchState,
    mut out: host_capnp::response::Builder<'_>,
    bytes: Vec<u8>,
    revision: u64,
) -> Result<String> {
    let part = host.draft_transfers.open(bytes, crate::now(host.start))?;
    host.draft_download_revision = revision;
    out.set_revision(revision);
    chunk(out, &part);
    Ok(part.token)
}

fn record_chunk(
    host: &mut WorkbenchState,
    out: host_capnp::response::Builder<'_>,
    card: &str,
    draft: &str,
    operation: &str,
    expected: u64,
    record: &editor_draft::EditorDraftRecord,
) -> Result<String> {
    let bytes = envelope(
        wire::ResultKind::Record,
        card,
        draft,
        operation,
        expected,
        Some(record),
        &[],
        None,
    )?;
    envelope_chunk(host, out, bytes, record.current_generation)
}

pub(crate) fn is_action(action: host_capnp::Action) -> bool {
    use host_capnp::Action;
    matches!(
        action,
        Action::BeginEditorDraft
            | Action::AppendEditorDraft
            | Action::FinishEditorDraft
            | Action::AbortEditorDraftTransfer
            | Action::ReadEditorDraft
            | Action::ReadEditorDraftPart
            | Action::ListEditorDrafts
            | Action::DiscardEditorDraft
            | Action::ImportEditorDraftAsset
            | Action::ExportEditorDraftAsset
            | Action::FinishEditorDraftHandoff
            | Action::RetireEditorDraftParent
            | Action::ListEditorDraftLineages
    )
}

pub(crate) fn reply_correlation(
    host: &mut WorkbenchState,
    action: host_capnp::Action,
    request: host_capnp::request::Reader<'_>,
) -> Result<String> {
    use host_capnp::Action;
    if !matches!(
        action,
        Action::ReadEditorDraft
            | Action::ListEditorDrafts
            | Action::DiscardEditorDraft
            | Action::ImportEditorDraftAsset
            | Action::ExportEditorDraftAsset
            | Action::BeginEditorDraftImport
            | Action::CompleteEditorDraftImport
            | Action::InspectEditorDraftImport
            | Action::ListEditorDraftImports
            | Action::ExportEditorDraftImport
            | Action::AbandonEditorDraftImport
            | Action::ReconcileEditorDraftImports
            | Action::PrepareEditorDraftImportDecision
            | Action::InspectEditorDraftImportDecision
            | Action::ListEditorDraftImportDecisions
            | Action::CancelEditorDraftImportDecision
            | Action::ListEditorDraftImportDecisionScopes
            | Action::RetireEditorDraftParent
            | Action::ListEditorDraftLineages
    ) {
        return Ok(String::new());
    }
    let key = text(request.get_transfer())?;
    if key.is_empty() {
        return Ok(key); // Compatibility for callers predating reply correlations.
    }
    morrow_core::runtime::Command::ReadSummary {
        request_id: key.clone(),
        card_id: "draft-reply".into(),
    }
    .validate()?;
    let suffix = key
        .strip_prefix("draft-request-")
        .ok_or("editor draft reply correlation prefix")?;
    if suffix.is_empty()
        || (suffix.len() > 1 && suffix.starts_with('0'))
        || !suffix.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("editor draft reply correlation sequence".into());
    }
    let sequence: u64 = suffix.parse()?;
    if host
        .draft_reply_sequence
        .is_some_and(|last| sequence <= last)
    {
        return Err("editor draft reply correlation replay".into());
    }
    // Admit before any journal or selected-file effect. A failed request still
    // consumes its sequence, so a stale Abort can never match a later reply.
    host.draft_reply_sequence = Some(sequence);
    Ok(key)
}

pub(crate) fn bind_reply(host: &mut WorkbenchState, key: &str, download: String) {
    if !key.is_empty() {
        host.draft_reply_owner = Some((key.into(), download));
    }
}

pub(crate) fn handle(
    host: &mut WorkbenchState,
    r: host_capnp::request::Reader<'_>,
    mut out: host_capnp::response::Builder<'_>,
) -> Result<()> {
    use host_capnp::Action;
    let action = r.get_action()?;
    let reply_owner = reply_correlation(host, action, r)?;
    let card = text(r.get_id())?;
    let draft = text(r.get_attachment())?;
    let now = crate::now(host.start);
    match action {
        Action::BeginEditorDraft => {
            let token = host.draft_transfers.begin(
                text(r.get_operation())?,
                r.get_total_length(),
                r.get_sha256()?,
                now,
            )?;
            out.set_transfer(&token);
        }
        Action::AppendEditorDraft => {
            let token = text(r.get_transfer())?;
            let offset =
                host.draft_transfers
                    .append(&token, r.get_offset(), r.get_payload()?, now)?;
            out.set_transfer(&token);
            out.set_offset(offset as u64);
        }
        Action::FinishEditorDraft => {
            let operation = text(r.get_operation())?;
            let upload_token = text(r.get_transfer())?;
            let (uploaded_operation, bytes) = host.draft_transfers.finish(&upload_token, now)?;
            let request = decode_write(&bytes)?;
            if uploaded_operation != request.operation_id
                || operation != request.operation_id
                || card != request.card_id
                || draft != request.draft_id
                || r.get_revision() != request.expected_generation
            {
                return Err("editor draft upload outer binding changed".into());
            }
            let record = host.save_editor_draft(&request)?;
            let reply_token = record_chunk(
                host,
                out,
                &card,
                &draft,
                &operation,
                request.expected_generation,
                &record,
            )?;
            // The caller may lose this first reply and know only its upload token.
            // Keep one exact pair; transfer tokens are monotonically unique per lane.
            bind_reply(host, &upload_token, reply_token);
        }
        Action::FinishEditorDraftHandoff => {
            let operation = text(r.get_operation())?;
            let upload_token = text(r.get_transfer())?;
            let (uploaded_operation, bytes) = host.draft_transfers.finish(&upload_token, now)?;
            let (request, link) = decode_handoff(&bytes)?;
            if uploaded_operation != request.operation_id
                || operation != request.operation_id
                || operation != link.child_operation
                || card != request.card_id
                || draft != request.draft_id
                || r.get_revision() != 0
                || !r.get_payload()?.is_empty()
                || !text(r.get_selected_path())?.is_empty()
                || !text(r.get_name())?.is_empty()
                || !text(r.get_kind())?.is_empty()
                || !text(r.get_cursor())?.is_empty()
                || r.get_limit() != 0
            {
                return Err("editor draft handoff outer binding changed".into());
            }
            let record = host.handoff_editor_draft(&request, &link)?;
            let reply_token = record_chunk(host, out, &card, &draft, &operation, 0, &record)?;
            bind_reply(host, &upload_token, reply_token);
        }
        Action::AbortEditorDraftTransfer => {
            let token = text(r.get_transfer())?;
            host.draft_transfers.abort(&token);
            if let Some((upload, download)) = host.draft_reply_owner.as_ref() {
                if upload == &token {
                    host.draft_transfers.abort(download);
                    host.draft_reply_owner = None;
                }
            }
        }
        Action::ReadEditorDraft => {
            if let Some((_, slot)) = host.draft_card(&card, &draft)? {
                let generation = slot.generation;
                let active = slot.active;
                let record = editor_draft::EditorDraftRecord {
                    slot,
                    current_generation: generation,
                    current_active: active,
                    repeated: false,
                };
                let download = record_chunk(host, out, &card, &draft, "", 0, &record)?;
                bind_reply(host, &reply_owner, download);
            } else {
                let bytes = envelope(
                    wire::ResultKind::Absent,
                    &card,
                    &draft,
                    "",
                    0,
                    None,
                    &[],
                    None,
                )?;
                let download = envelope_chunk(host, out, bytes, 0)?;
                bind_reply(host, &reply_owner, download);
            }
        }
        Action::ReadEditorDraftPart => {
            let part = host
                .draft_transfers
                .read(&text(r.get_transfer())?, r.get_offset(), now)?;
            let revision = host.draft_download_revision;
            let mut out = out;
            out.set_revision(revision);
            chunk(out, &part);
        }
        Action::ListEditorDrafts => {
            let summaries = host
                .all_draft_metadata()?
                .into_iter()
                .filter(|slot| slot.active)
                .map(|slot| {
                    let request = slot.request.expect("validated draft metadata");
                    editor_draft::EditorDraftSummary {
                        card_id: request.card_id,
                        draft_id: request.draft_id,
                        generation: slot.generation,
                        active: true,
                    }
                })
                .collect::<Vec<_>>();
            let bytes = envelope(
                wire::ResultKind::List,
                "",
                "",
                "",
                0,
                None,
                &summaries,
                None,
            )?;
            let download = envelope_chunk(host, out, bytes, 0)?;
            bind_reply(host, &reply_owner, download);
        }
        Action::DiscardEditorDraft => {
            let operation = text(r.get_operation())?;
            let expected = r.get_revision();
            let record = host.discard_editor_draft(&card, &draft, expected, &operation)?;
            let download = record_chunk(host, out, &card, &draft, &operation, expected, &record)?;
            bind_reply(host, &reply_owner, download);
        }
        Action::ImportEditorDraftAsset => {
            let path = text(r.get_selected_path())?;
            let mut file = std::fs::File::open(&path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() || metadata.len() != r.get_total_length() {
                return Err("draft selected file length changed".into());
            }
            let name = text(r.get_name())?;
            let kind = text(r.get_kind())?;
            let asset = host.import_editor_draft_asset(
                &card,
                &draft,
                r.get_revision(),
                &name,
                &kind,
                &mut file,
                metadata.len(),
            )?;
            let bytes = envelope(
                wire::ResultKind::Imported,
                &card,
                &draft,
                "",
                r.get_revision(),
                None,
                &[],
                Some(&asset),
            )?;
            let download = envelope_chunk(host, out, bytes, r.get_revision())?;
            bind_reply(host, &reply_owner, download);
        }
        Action::ExportEditorDraftAsset => {
            let asset_id = text(r.get_name())?;
            let bytes =
                host.export_editor_draft_asset(&card, &draft, r.get_revision(), &asset_id)?;
            let selected = text(r.get_selected_path())?;
            let destination = Path::new(&selected);
            if !destination.parent().is_some_and(Path::is_dir) {
                return Err("draft export directory unavailable".into());
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)?;
            use std::io::Write;
            let result = file.write_all(&bytes).and_then(|_| file.sync_all());
            drop(file);
            if result.is_err() {
                let _ = std::fs::remove_file(destination);
            }
            result?;
            let bytes = envelope(
                wire::ResultKind::Exported,
                &card,
                &draft,
                "",
                r.get_revision(),
                None,
                &[],
                None,
            )?;
            let download = envelope_chunk(host, out, bytes, r.get_revision())?;
            bind_reply(host, &reply_owner, download);
        }
        Action::RetireEditorDraftParent => {
            let operation = text(r.get_operation())?;
            let parent = text(r.get_name())?;
            let child_operation = text(r.get_kind())?;
            let expected = r.get_revision();
            if !r.get_payload()?.is_empty()
                || !text(r.get_selected_path())?.is_empty()
                || !text(r.get_cursor())?.is_empty()
                || r.get_limit() != 0
            {
                return Err("editor draft parent retirement outer fields".into());
            }
            let (_, child_slot) = host
                .draft_card(&card, &draft)?
                .ok_or("editor draft child missing")?;
            let link = child_slot
                .parent_link
                .as_ref()
                .ok_or("editor draft child has no parent link")?;
            if parent != link.parent_draft_id
                || child_operation != link.child_operation
                || expected != link.parent_generation
            {
                return Err("editor draft parent retirement outer binding changed".into());
            }
            let record = host.retire_editor_draft_parent(&card, &draft, &operation, false)?;
            let download = record_chunk(host, out, &card, &parent, &operation, expected, &record)?;
            bind_reply(host, &reply_owner, download);
        }
        Action::ListEditorDraftLineages => {
            let cursor = text(r.get_cursor())?;
            let limit = r.get_limit();
            if !card.is_empty()
                || !draft.is_empty()
                || !text(r.get_name())?.is_empty()
                || !text(r.get_operation())?.is_empty()
                || r.get_revision() != 0
                || !r.get_payload()?.is_empty()
                || !text(r.get_selected_path())?.is_empty()
                || limit == 0
                || limit > 32
                || !valid_lineage_cursor(&cursor)
            {
                return Err("editor draft lineage page request".into());
            }
            let mut all = host.list_editor_draft_lineages()?;
            if all
                .iter()
                .any(|item| !valid_lineage_cursor(&item.cursor) || item.cursor.is_empty())
            {
                return Err("editor draft lineage cursor changed".into());
            }
            all.sort_by(|left, right| left.cursor.cmp(&right.cursor));
            let mut page = all
                .into_iter()
                .filter(|item| cursor.is_empty() || item.cursor > cursor)
                .collect::<Vec<_>>();
            let more = page.len() > limit as usize;
            page.truncate(limit as usize);
            let next = if more {
                page.last().map_or("", |item| item.cursor.as_str())
            } else {
                ""
            };
            let bytes = lineages_envelope(&page, &cursor, limit, next)?;
            let download = envelope_chunk(host, out, bytes, 0)?;
            bind_reply(host, &reply_owner, download);
        }
        _ => return Err("not an editor draft action".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    fn text_value() -> proto::TextValue {
        proto::TextValue {
            text: "x".repeat(512 * 1024),
            selection_base: 0,
            selection_extent: 0,
            affinity: 0,
            directional: false,
            composing_start: -1,
            composing_end: -1,
        }
    }

    fn near_limit_request(alias_len: usize) -> proto::WriteRequest {
        let alias = "a".repeat(alias_len);
        proto::WriteRequest {
            schema_version: 1,
            card_id: "card".into(),
            draft_id: "draft".into(),
            operation_id: "save-draft".into(),
            expected_generation: 0,
            source_revision: 1,
            source_kind: 0,
            predecessor_operation: String::new(),
            predecessor_sha256: Vec::new(),
            values: Some(proto::Values {
                title: Some(text_value()),
                description: Some(text_value()),
                hypothesis: Some(text_value()),
                conclusion: Some(text_value()),
                todos: Some(text_value()),
                category: String::new(),
                stage: String::new(),
            }),
            assets: (0..20)
                .map(|i| proto::AssetSelection {
                    origin: 0,
                    asset_id: format!("asset-{i}"),
                    aliases: vec![alias.clone(); 8],
                })
                .collect(),
        }
    }

    #[test]
    fn capnp_write_over_four_mib_still_decodes_when_protobuf_fits() {
        let mut low = 1usize;
        let mut high = 16 * 1024;
        while low < high {
            let mid = (low + high + 1) / 2;
            if near_limit_request(mid).encoded_len() <= editor_draft::model::MAX_BODY_BYTES {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let request = near_limit_request(low);
        validate_request(&request).unwrap();
        let mut message = Builder::new_default();
        request_reply(
            &request,
            message.init_root::<wire::write_request::Builder>(),
        )
        .unwrap();
        let bytes = serialize::write_message_to_words(&message);
        assert!(bytes.len() > 4 * 1024 * 1024);
        assert!(bytes.len() <= MAX_WRITE_FRAME);
        assert_eq!(decode_write(&bytes).unwrap(), request);
    }
    fn small_request() -> proto::WriteRequest {
        let mut request = near_limit_request(0);
        let values = request.values.as_mut().unwrap();
        for value in [
            values.title.as_mut().unwrap(),
            values.description.as_mut().unwrap(),
            values.hypothesis.as_mut().unwrap(),
            values.conclusion.as_mut().unwrap(),
            values.todos.as_mut().unwrap(),
        ] {
            value.text = "raw".into();
        }
        request.assets = vec![proto::AssetSelection {
            origin: 4,
            asset_id: "parent-asset".into(),
            aliases: vec!["original-name".into()],
        }];
        request
    }

    fn link() -> proto::ParentLink {
        proto::ParentLink {
            parent_draft_id: "parent".into(),
            parent_generation: 1,
            parent_save_operation: "parent-save".into(),
            parent_request_sha256: vec![1; 32],
            committed_operation: "business-commit".into(),
            committed_sha256: vec![2; 32],
            child_operation: "save-draft".into(),
        }
    }

    fn handoff_frame(request: &proto::WriteRequest, link: &proto::ParentLink) -> Vec<u8> {
        let mut message = Builder::new_default();
        let mut out = message.init_root::<wire::handoff_request::Builder>();
        out.set_version(1);
        out.set_digest(&digest());
        request_reply(request, out.reborrow().init_request()).unwrap();
        parent_link_reply(link, out.reborrow().init_parent_link());
        serialize::write_message_to_words(&message)
    }

    #[test]
    fn handoff_frame_binds_parent_and_rejects_old_upload_origin_four() {
        let request = small_request();
        let link = link();
        let frame = handoff_frame(&request, &link);
        assert_eq!(
            decode_handoff(&frame).unwrap(),
            (request.clone(), link.clone())
        );

        let mut old = Builder::new_default();
        request_reply(&request, old.init_root::<wire::write_request::Builder>()).unwrap();
        assert!(decode_write(&serialize::write_message_to_words(&old)).is_err());

        let mut changed = link.clone();
        changed.parent_request_sha256.pop();
        assert!(decode_handoff(&handoff_frame(&request, &changed)).is_err());
        changed = link;
        changed.child_operation = "other-operation".into();
        assert!(decode_handoff(&handoff_frame(&request, &changed)).is_err());

        let mut wrong_digest = Builder::new_default();
        let mut out = wrong_digest.init_root::<wire::handoff_request::Builder>();
        out.set_version(1);
        out.set_digest(&[7; 32]);
        request_reply(&request, out.reborrow().init_request()).unwrap();
        parent_link_reply(&changed, out.reborrow().init_parent_link());
        assert!(decode_handoff(&serialize::write_message_to_words(&wrong_digest)).is_err());
    }

    #[test]
    fn record_wire_includes_parent_link_and_distinct_retirement_marker() {
        let mut request = small_request();
        request.source_kind = 1;
        request.source_revision = 0;
        request.assets.clear();
        let request_hash = Sha256::digest(request.encode_to_vec());
        let link = link();
        let retirement = proto::ParentRetirement {
            child_draft_id: "descendant".into(),
            child_operation: "descendant-save".into(),
            operation_id: "retire-operation".into(),
            parent_generation: 1,
        };
        let record = editor_draft::EditorDraftRecord {
            slot: proto::Slot {
                request: Some(request),
                parent_link: Some(link.clone()),
                retirement: Some(retirement.clone()),
                ..Default::default()
            },
            current_generation: 2,
            current_active: false,
            repeated: false,
        };
        let mut message = Builder::new_default();
        record_reply(&record, message.init_root::<wire::record::Builder>()).unwrap();
        let bytes = serialize::write_message_to_words(&message);
        let mut cursor = Cursor::new(bytes);
        let decoded = serialize::read_message(&mut cursor, ReaderOptions::default()).unwrap();
        let reply = decoded.get_root::<wire::record::Reader>().unwrap();
        assert!(reply.has_parent_link());
        assert!(reply.has_retirement());
        assert_eq!(reply.get_request_sha256().unwrap(), request_hash.as_slice());
        assert_eq!(
            decode_parent_link(reply.get_parent_link().unwrap()).unwrap(),
            link
        );
        let retired = reply.get_retirement().unwrap();
        assert_eq!(
            text(retired.get_child_draft_id()).unwrap(),
            retirement.child_draft_id
        );
        assert_eq!(
            text(retired.get_child_operation()).unwrap(),
            retirement.child_operation
        );
        assert_eq!(
            text(retired.get_operation()).unwrap(),
            retirement.operation_id
        );
        assert_eq!(
            retired.get_parent_generation(),
            retirement.parent_generation
        );
    }

    #[test]
    fn lineage_page_echoes_cursor_limit_and_full_parent_binding() {
        let cursor = format!("morrow-host-editor-draft-{}", "a".repeat(64));
        let entry = editor_draft::EditorDraftLineage {
            card_id: "card".into(),
            child_draft_id: "draft".into(),
            child_generation: 2,
            child_active: true,
            parent_generation: 3,
            parent_active: false,
            link: link(),
            cursor: cursor.clone(),
        };
        let bytes = lineages_envelope(std::slice::from_ref(&entry), "", 1, &cursor).unwrap();
        let decoded =
            serialize::read_message(&mut Cursor::new(bytes), ReaderOptions::default()).unwrap();
        let page = decoded.get_root::<wire::envelope::Reader>().unwrap();
        assert_eq!(page.get_kind().unwrap(), wire::ResultKind::Lineages);
        assert_eq!(text(page.get_request_cursor()).unwrap(), "");
        assert_eq!(page.get_request_limit(), 1);
        assert_eq!(text(page.get_next_cursor()).unwrap(), cursor);
        let items = page.get_lineages().unwrap();
        assert_eq!(items.len(), 1);
        let item = items.get(0);
        assert_eq!(
            text(item.get_child_draft_id()).unwrap(),
            entry.child_draft_id
        );
        assert_eq!(item.get_child_generation(), entry.child_generation);
        assert_eq!(item.get_child_active(), entry.child_active);
        assert_eq!(item.get_parent_generation(), entry.parent_generation);
        assert_eq!(item.get_parent_active(), entry.parent_active);
        assert_eq!(
            decode_parent_link(item.get_parent_link().unwrap()).unwrap(),
            entry.link
        );
    }

    #[test]
    fn lineage_cursor_rejects_foreign_and_noncanonical_keys() {
        assert!(valid_lineage_cursor(""));
        assert!(valid_lineage_cursor(&format!(
            "morrow-host-editor-draft-{}",
            "a".repeat(64)
        )));
        assert!(!valid_lineage_cursor("morrow-host-editor-draft-a"));
        assert!(!valid_lineage_cursor(&format!(
            "morrow-host-editor-draft-{}",
            "A".repeat(64)
        )));
        assert!(!valid_lineage_cursor(&format!(
            "morrow-host-editor-imports-{}",
            "a".repeat(64)
        )));
    }
}
