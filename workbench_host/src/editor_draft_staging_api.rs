//! Typed private wire for durable editor-draft imports. The journal remains Protobuf/Core-owned.
use crate::editor_draft_import_decision::{DecisionRecord, DecisionStatus};
use crate::{
    editor_draft_api, editor_draft_staging, editor_draft_staging_api_capnp as wire, host_capnp,
    Result, WorkbenchState,
};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use editor_draft_staging::{proto, DraftImportPhase, DraftImportRecord};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read, Write},
    path::Path,
};

const MAX_REQUEST: usize = 64 * 1024;
const MAX_ENVELOPE: usize = crate::transfer::MAX_DRAFT_BYTES;

pub(crate) fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/editor_draft_staging_api.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    Ok(value?.to_str()?.to_owned())
}
fn identity(value: &str) -> Result<()> {
    morrow_core::runtime::Command::ReadSummary {
        request_id: value.into(),
        card_id: value.into(),
    }
    .validate()?;
    Ok(())
}
fn decode_import(bytes: &[u8]) -> Result<proto::ImportRequest> {
    if bytes.is_empty() || bytes.len() > MAX_REQUEST {
        return Err("draft import request wire budget".into());
    }
    let mut cursor = Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(1_000_000),
            nesting_limit: 16,
        },
    )?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing draft import wire bytes".into());
    }
    let value = message.get_root::<wire::import_request::Reader>()?;
    if value.get_version() != 1 || value.get_digest()? != digest() {
        return Err("draft import wire contract mismatch".into());
    }
    let request = proto::ImportRequest {
        schema_version: 1,
        card_id: text(value.get_card_id())?,
        draft_id: text(value.get_draft_id())?,
        operation_id: text(value.get_operation())?,
        expected_generation: value.get_expected_generation(),
        name: text(value.get_name())?,
        kind: text(value.get_kind())?,
        byte_length: value.get_bytes(),
        sha256: value.get_sha256()?.to_vec(),
    };
    editor_draft_staging::validate_request(&request)?;
    Ok(request)
}
fn request_reply(value: &proto::ImportRequest, mut out: wire::import_request::Builder<'_>) {
    out.set_version(1);
    out.set_digest(&digest());
    out.set_card_id(&value.card_id);
    out.set_draft_id(&value.draft_id);
    out.set_operation(&value.operation_id);
    out.set_expected_generation(value.expected_generation);
    out.set_name(&value.name);
    out.set_kind(&value.kind);
    out.set_bytes(value.byte_length);
    out.set_sha256(&value.sha256);
}
fn record_reply(
    record: &DraftImportRecord,
    current: (u64, bool, u64),
    mut out: wire::record::Builder<'_>,
) {
    request_reply(&record.request, out.reborrow().init_request());
    out.set_asset_id(&record.asset_id);
    out.set_phase(match record.phase {
        DraftImportPhase::Pending => wire::Phase::Pending,
        DraftImportPhase::Ready => wire::Phase::Ready,
        DraftImportPhase::Retired => wire::Phase::Retired,
    });
    out.set_current_active(record.current_active);
    out.set_bytes_retained(record.bytes_retained);
    out.set_staging_revision(record.staging_revision);
    out.set_repeated(record.repeated);
    out.set_current_generation(current.0);
    out.set_main_active(current.1);
}
fn envelope(
    kind: wire::ResultKind,
    card: &str,
    draft: &str,
    operation: &str,
    expected: u64,
    import_op: &str,
    current: (u64, bool, u64),
    record: Option<&DraftImportRecord>,
    records: &[DraftImportRecord],
    exported: Option<(&[u8], u64)>,
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
        out.set_expected_generation(expected);
        out.set_import_operation(import_op);
        out.set_current_generation(current.0);
        out.set_main_active(current.1);
        out.set_staging_revision(current.2);
        if let Some(record) = record {
            record_reply(record, current, out.reborrow().init_record());
        }
        if matches!(kind, wire::ResultKind::List | wire::ResultKind::Reconciled) {
            let mut list = out.reborrow().init_records(records.len().try_into()?);
            for (index, record) in records.iter().enumerate() {
                record_reply(record, current, list.reborrow().get(index as u32));
            }
        }
        if let Some((sha, length)) = exported {
            out.set_export_sha256(sha);
            out.set_export_bytes(length);
        }
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_ENVELOPE {
        return Err("draft import envelope budget".into());
    }
    Ok(bytes)
}

pub(crate) fn is_action(action: host_capnp::Action) -> bool {
    use host_capnp::Action;
    matches!(
        action,
        Action::BeginEditorDraftImport
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
    )
}
fn send(
    host: &mut WorkbenchState,
    out: host_capnp::response::Builder<'_>,
    correlation: &str,
    kind: wire::ResultKind,
    card: &str,
    draft: &str,
    operation: &str,
    expected: u64,
    import_op: &str,
    record: Option<&DraftImportRecord>,
    records: &[DraftImportRecord],
    exported: Option<(&[u8], u64)>,
) -> Result<()> {
    let current = host.draft_import_wire_context(card, draft)?;
    let bytes = envelope(
        kind, card, draft, operation, expected, import_op, current, record, records, exported,
    )?;
    let download = editor_draft_api::envelope_chunk(host, out, bytes, current.0)?;
    editor_draft_api::bind_reply(host, correlation, download);
    Ok(())
}
fn selected_path(value: host_capnp::request::Reader<'_>) -> Result<String> {
    let selected = text(value.get_selected_path())?;
    let path = Path::new(&selected);
    if !path.is_absolute() || !path.parent().is_some_and(Path::is_dir) {
        return Err("draft import selected path unavailable".into());
    }
    Ok(selected)
}
fn checked_request(
    r: host_capnp::request::Reader<'_>,
    card: &str,
    draft: &str,
    operation: &str,
    name: &str,
) -> Result<proto::ImportRequest> {
    let request = decode_import(r.get_payload()?)?;
    if request.card_id != card
        || request.draft_id != draft
        || request.operation_id != operation
        || request.operation_id != name
        || request.expected_generation != r.get_revision()
    {
        return Err("draft import outer binding changed".into());
    }
    Ok(request)
}
fn empty_payload(r: host_capnp::request::Reader<'_>) -> Result<()> {
    if !r.get_payload()?.is_empty() {
        return Err("unexpected draft import payload".into());
    }
    Ok(())
}
fn confirm_export(path: &Path, bytes: &[u8], sha: &[u8]) -> Result<()> {
    if !path.is_absolute() || !path.parent().is_some_and(Path::is_dir) {
        return Err("draft import export directory unavailable".into());
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            let result = file.write_all(bytes).and_then(|_| file.sync_all());
            drop(file);
            // Keep a failed partial destination for explicit user recovery; a concurrent
            // replacement must never be removed by this process.
            result?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let link = std::fs::symlink_metadata(path)?;
            if link.file_type().is_symlink() || !link.file_type().is_file() {
                return Err("existing draft export is not a regular file".into());
            }
            let file = std::fs::File::open(path)?;
            if !file.metadata()?.is_file() || file.metadata()?.len() != bytes.len() as u64 {
                return Err("existing draft export differs".into());
            }
            let mut observed = Vec::with_capacity(bytes.len() + 1);
            file.take(bytes.len() as u64 + 1)
                .read_to_end(&mut observed)?;
            if observed.len() != bytes.len() || Sha256::digest(&observed).as_slice() != sha {
                return Err("existing draft export differs".into());
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

pub(crate) fn handle(
    host: &mut WorkbenchState,
    r: host_capnp::request::Reader<'_>,
    out: host_capnp::response::Builder<'_>,
) -> Result<()> {
    use host_capnp::Action;
    let action = r.get_action()?;
    if !is_action(action) {
        return Err("not a draft import action".into());
    }
    // Correlation admission happens before every journal or selected-path effect.
    let correlation = editor_draft_api::reply_correlation(host, action, r)?;
    if correlation.is_empty() {
        return Err("draft import reply correlation required".into());
    }
    let card = text(r.get_id())?;
    let draft = text(r.get_attachment())?;
    let operation = text(r.get_operation())?;
    let import_op = text(r.get_name())?;
    if action == Action::ListEditorDraftImportDecisionScopes {
        empty_payload(r)?;
        if !card.is_empty()
            || !draft.is_empty()
            || !operation.is_empty()
            || !import_op.is_empty()
            || r.get_revision() != 0
            || !text(r.get_selected_path())?.is_empty()
            || r.get_total_length() != 0
            || !r.get_sha256()?.is_empty()
            || r.get_offset() != 0
        {
            return Err("decision scope list outer binding".into());
        }
        let cursor = text(r.get_cursor())?;
        let limit = r.get_limit();
        let (scopes, next) = host.list_editor_draft_import_decision_scopes(&cursor, limit)?;
        send_scopes(host, out, &correlation, &scopes, &cursor, limit, &next)?;
        return Ok(());
    }
    identity(&card)?;
    identity(&draft)?;
    match action {
        Action::BeginEditorDraftImport | Action::CompleteEditorDraftImport => {
            let request = checked_request(r, &card, &draft, &operation, &import_op)?;
            let record = if action == Action::BeginEditorDraftImport {
                if !text(r.get_selected_path())?.is_empty() {
                    return Err("begin import cannot open a selected path".into());
                }
                host.begin_editor_draft_import(&request)?
            } else {
                let admitted = host.begin_editor_draft_import(&request)?;
                if admitted.phase != DraftImportPhase::Pending {
                    admitted
                } else if admitted.bytes_retained {
                    host.import_editor_draft_asset_durable(&request, &mut Cursor::new(&[]))?
                } else {
                    let (generation, active, _) = host.draft_import_wire_context(&card, &draft)?;
                    if !active || generation != request.expected_generation {
                        return Err("draft import source generation changed".into());
                    }
                    let selected = selected_path(r)?;
                    let mut file = std::fs::File::open(&selected)?;
                    let metadata = file.metadata()?;
                    if !metadata.is_file() || metadata.len() != request.byte_length {
                        return Err("draft import selected file length changed".into());
                    }
                    host.import_editor_draft_asset_durable(&request, &mut file)?
                }
            };
            send(
                host,
                out,
                &correlation,
                wire::ResultKind::Record,
                &card,
                &draft,
                &operation,
                r.get_revision(),
                &import_op,
                Some(&record),
                &[],
                None,
            )?;
        }
        Action::InspectEditorDraftImport => {
            empty_payload(r)?;
            if r.get_revision() != 0
                || operation != import_op
                || !text(r.get_selected_path())?.is_empty()
            {
                return Err("draft import inspection outer binding".into());
            }
            identity(&import_op)?;
            let record = host.inspect_editor_draft_import(&card, &draft, &import_op)?;
            send(
                host,
                out,
                &correlation,
                if record.is_some() {
                    wire::ResultKind::Record
                } else {
                    wire::ResultKind::Absent
                },
                &card,
                &draft,
                &operation,
                0,
                &import_op,
                record.as_ref(),
                &[],
                None,
            )?;
        }
        Action::ListEditorDraftImports | Action::ReconcileEditorDraftImports => {
            empty_payload(r)?;
            if r.get_revision() != 0
                || !operation.is_empty()
                || !import_op.is_empty()
                || !text(r.get_selected_path())?.is_empty()
            {
                return Err("draft import list outer binding".into());
            }
            if action == Action::ReconcileEditorDraftImports {
                host.reconcile_editor_draft_imports(&card, &draft)?;
            }
            let records = host.list_editor_draft_imports(&card, &draft)?;
            send(
                host,
                out,
                &correlation,
                if action == Action::ReconcileEditorDraftImports {
                    wire::ResultKind::Reconciled
                } else {
                    wire::ResultKind::List
                },
                &card,
                &draft,
                "",
                0,
                "",
                None,
                &records,
                None,
            )?;
        }
        Action::ExportEditorDraftImport => {
            empty_payload(r)?;
            if r.get_revision() == 0 || operation != import_op {
                return Err("draft import export outer binding".into());
            }
            identity(&import_op)?;
            let record = host
                .inspect_editor_draft_import(&card, &draft, &import_op)?
                .ok_or("draft import absent")?;
            let bytes =
                host.export_editor_draft_import(&card, &draft, r.get_revision(), &import_op)?;
            if bytes.len() as u64 != record.request.byte_length
                || Sha256::digest(&bytes).as_slice() != record.request.sha256.as_slice()
            {
                return Err("draft import export identity changed".into());
            }
            let selected = selected_path(r)?;
            confirm_export(Path::new(&selected), &bytes, &record.request.sha256)?;
            send(
                host,
                out,
                &correlation,
                wire::ResultKind::Exported,
                &card,
                &draft,
                &operation,
                r.get_revision(),
                &import_op,
                None,
                &[],
                Some((&record.request.sha256, record.request.byte_length)),
            )?;
        }
        Action::AbandonEditorDraftImport => {
            empty_payload(r)?;
            if r.get_revision() == 0
                || operation.is_empty()
                || import_op.is_empty()
                || operation == import_op
                || !text(r.get_selected_path())?.is_empty()
            {
                return Err("draft import abandon outer binding".into());
            }
            let record = host.commit_prepared_editor_draft_import_abandon(
                &card,
                &draft,
                r.get_revision(),
                &import_op,
                &operation,
            )?;
            send(
                host,
                out,
                &correlation,
                wire::ResultKind::Record,
                &card,
                &draft,
                &operation,
                r.get_revision(),
                &import_op,
                Some(&record),
                &[],
                None,
            )?;
        }
        Action::PrepareEditorDraftImportDecision
        | Action::InspectEditorDraftImportDecision
        | Action::CancelEditorDraftImportDecision => {
            empty_payload(r)?;
            if r.get_revision() == 0
                || operation.is_empty()
                || import_op.is_empty()
                || operation == import_op
                || !text(r.get_selected_path())?.is_empty()
                || !text(r.get_cursor())?.is_empty()
                || r.get_limit() != 0
            {
                return Err("draft import decision outer binding".into());
            }
            let decision = match action {
                Action::PrepareEditorDraftImportDecision => {
                    Some(host.prepare_editor_draft_import_decision(
                        &card,
                        &draft,
                        r.get_revision(),
                        &import_op,
                        &operation,
                    )?)
                }
                Action::InspectEditorDraftImportDecision => host
                    .inspect_editor_draft_import_decision(
                        &card,
                        &draft,
                        r.get_revision(),
                        &import_op,
                        &operation,
                    )?,
                Action::CancelEditorDraftImportDecision => {
                    Some(host.cancel_editor_draft_import_decision(
                        &card,
                        &draft,
                        r.get_revision(),
                        &import_op,
                        &operation,
                    )?)
                }
                _ => unreachable!(),
            };
            send_decision(
                host,
                out,
                &correlation,
                if decision.is_some() {
                    wire::ResultKind::Decision
                } else {
                    wire::ResultKind::Absent
                },
                &card,
                &draft,
                &operation,
                r.get_revision(),
                &import_op,
                decision.as_ref(),
                &[],
                "",
                0,
                "",
            )?;
        }
        Action::ListEditorDraftImportDecisions => {
            empty_payload(r)?;
            if r.get_revision() != 0
                || !operation.is_empty()
                || !import_op.is_empty()
                || !text(r.get_selected_path())?.is_empty()
            {
                return Err("draft import decision list outer binding".into());
            }
            let cursor = text(r.get_cursor())?;
            let limit = r.get_limit();
            let (decisions, next) =
                host.list_editor_draft_import_decisions(&card, &draft, &cursor, limit)?;
            send_decision(
                host,
                out,
                &correlation,
                wire::ResultKind::Decisions,
                &card,
                &draft,
                "",
                0,
                "",
                None,
                &decisions,
                &cursor,
                limit,
                &next,
            )?;
        }
        _ => return Err("not a draft import action".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> proto::ImportRequest {
        proto::ImportRequest {
            schema_version: 1,
            card_id: "card".into(),
            draft_id: "draft".into(),
            operation_id: "import-one".into(),
            expected_generation: 2,
            name: "pinned.bin".into(),
            kind: "file".into(),
            byte_length: 4,
            sha256: Sha256::digest(b"data").to_vec(),
        }
    }
    fn encode_request(request: &proto::ImportRequest) -> Vec<u8> {
        let mut message = Builder::new_default();
        request_reply(
            request,
            message.init_root::<wire::import_request::Builder>(),
        );
        serialize::write_message_to_words(&message)
    }

    #[test]
    fn typed_import_request_roundtrip_and_frame_contract() {
        let request = request();
        let bytes = encode_request(&request);
        assert_eq!(decode_import(&bytes).unwrap(), request);
        let mut trailing = bytes.clone();
        trailing.extend_from_slice(&[0; 8]);
        assert!(decode_import(&trailing).is_err());
        assert!(decode_import(&vec![0; MAX_REQUEST + 1]).is_err());
        let mut bad = Builder::new_default();
        let mut root = bad.init_root::<wire::import_request::Builder>();
        root.set_version(1);
        root.set_digest(&[0; 32]);
        root.set_card_id("card");
        root.set_draft_id("draft");
        root.set_operation("import-one");
        root.set_expected_generation(2);
        root.set_name("pinned.bin");
        root.set_kind("file");
        root.set_bytes(4);
        root.set_sha256(&request.sha256);
        assert!(decode_import(&serialize::write_message_to_words(&bad)).is_err());
    }

    #[test]
    fn empty_list_is_explicit_and_record_has_exact_current_context() {
        let empty = envelope(
            wire::ResultKind::List,
            "card",
            "draft",
            "",
            0,
            "",
            (2, true, 9),
            None,
            &[],
            None,
        )
        .unwrap();
        let list = serialize::read_message(&mut Cursor::new(empty), ReaderOptions::new()).unwrap();
        let value = list.get_root::<wire::envelope::Reader>().unwrap();
        assert_eq!(value.get_kind().unwrap(), wire::ResultKind::List);
        assert!(value.has_records());
        assert_eq!(value.get_records().unwrap().len(), 0);
        assert_eq!(value.get_current_generation(), 2);
        assert_eq!(value.get_staging_revision(), 9);
        assert!(!value.has_record());

        let record = DraftImportRecord {
            request: request(),
            asset_id: "asset-one".into(),
            phase: DraftImportPhase::Ready,
            current_active: true,
            bytes_retained: true,
            staging_revision: 9,
            repeated: false,
        };
        let bytes = envelope(
            wire::ResultKind::Record,
            "card",
            "draft",
            "import-one",
            2,
            "import-one",
            (2, true, 9),
            Some(&record),
            &[],
            None,
        )
        .unwrap();
        let message =
            serialize::read_message(&mut Cursor::new(bytes), ReaderOptions::new()).unwrap();
        let value = message.get_root::<wire::envelope::Reader>().unwrap();
        assert!(value.has_record());
        assert!(!value.has_records());
        assert_eq!(
            value.get_record().unwrap().get_phase().unwrap(),
            wire::Phase::Ready
        );
        assert_eq!(value.get_record().unwrap().get_current_generation(), 2);
        assert_eq!(value.get_record().unwrap().get_staging_revision(), 9);
        assert_eq!(
            value
                .get_record()
                .unwrap()
                .get_request()
                .unwrap()
                .get_sha256()
                .unwrap(),
            record.request.sha256
        );
    }

    #[test]
    fn export_retry_accepts_only_exact_existing_regular_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("export.bin");
        let data = b"data";
        let sha = Sha256::digest(data);
        confirm_export(&path, data, &sha).unwrap();
        confirm_export(&path, data, &sha).unwrap();
        std::fs::write(&path, b"evil").unwrap();
        assert!(confirm_export(&path, data, &sha).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"evil");
    }
}

fn decision_reply(value: &DecisionRecord, mut out: wire::decision::Builder<'_>) {
    request_reply(&value.request, out.reborrow().init_request());
    out.set_operation(&value.operation_id);
    out.set_expected_generation(value.expected_generation);
    out.set_status(match value.status {
        DecisionStatus::Pending => wire::DecisionStatus::Pending,
        DecisionStatus::Committed => wire::DecisionStatus::Committed,
        DecisionStatus::Cancelled => wire::DecisionStatus::Cancelled,
        DecisionStatus::Conflict => wire::DecisionStatus::Conflict,
    });
    out.set_current_generation(value.current_generation);
    out.set_main_active(value.main_active);
    out.set_staging_revision(value.staging_revision);
    out.set_decision_revision(value.decision_revision);
    out.set_committed_revision(value.committed_revision);
}
fn decision_envelope(
    kind: wire::ResultKind,
    card: &str,
    draft: &str,
    operation: &str,
    expected: u64,
    imported: &str,
    current: (u64, bool, u64),
    decision: Option<&DecisionRecord>,
    decisions: &[DecisionRecord],
    request_cursor: &str,
    request_limit: u32,
    next_cursor: &str,
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
        out.set_expected_generation(expected);
        out.set_import_operation(imported);
        out.set_current_generation(current.0);
        out.set_main_active(current.1);
        out.set_staging_revision(current.2);
        out.set_request_cursor(request_cursor);
        out.set_request_limit(request_limit);
        out.set_next_cursor(next_cursor);
        if let Some(decision) = decision {
            if (
                decision.current_generation,
                decision.main_active,
                decision.staging_revision,
            ) != current
            {
                return Err("decision reply context changed".into());
            }
            decision_reply(decision, out.reborrow().init_decision());
        }
        if kind == wire::ResultKind::Decisions {
            let mut list = out.reborrow().init_decisions(decisions.len().try_into()?);
            for (index, decision) in decisions.iter().enumerate() {
                if (
                    decision.current_generation,
                    decision.main_active,
                    decision.staging_revision,
                ) != current
                {
                    return Err("decision page context changed".into());
                }
                decision_reply(decision, list.reborrow().get(index as u32));
            }
        }
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_ENVELOPE {
        return Err("decision envelope budget".into());
    }
    Ok(bytes)
}
fn send_decision(
    host: &mut WorkbenchState,
    out: host_capnp::response::Builder<'_>,
    correlation: &str,
    kind: wire::ResultKind,
    card: &str,
    draft: &str,
    operation: &str,
    expected: u64,
    imported: &str,
    decision: Option<&DecisionRecord>,
    decisions: &[DecisionRecord],
    request_cursor: &str,
    request_limit: u32,
    next_cursor: &str,
) -> Result<()> {
    let current = host.draft_import_wire_context(card, draft)?;
    let bytes = decision_envelope(
        kind,
        card,
        draft,
        operation,
        expected,
        imported,
        current,
        decision,
        decisions,
        request_cursor,
        request_limit,
        next_cursor,
    )?;
    let download = editor_draft_api::envelope_chunk(host, out, bytes, current.0)?;
    editor_draft_api::bind_reply(host, correlation, download);
    Ok(())
}

fn scopes_envelope(
    scopes: &[(String, String)],
    request_cursor: &str,
    request_limit: u32,
    next_cursor: &str,
) -> Result<Vec<u8>> {
    let mut message = Builder::new_default();
    {
        let mut out = message.init_root::<wire::envelope::Builder>();
        out.set_version(1);
        out.set_digest(&digest());
        out.set_kind(wire::ResultKind::Scopes);
        out.set_card_id("");
        out.set_draft_id("");
        out.set_operation("");
        out.set_expected_generation(0);
        out.set_import_operation("");
        out.set_current_generation(0);
        out.set_main_active(false);
        out.set_staging_revision(0);
        out.set_request_cursor(request_cursor);
        out.set_request_limit(request_limit);
        out.set_next_cursor(next_cursor);
        let mut list = out.reborrow().init_scopes(scopes.len().try_into()?);
        for (index, (card, draft)) in scopes.iter().enumerate() {
            let mut item = list.reborrow().get(index as u32);
            item.set_card_id(card);
            item.set_draft_id(draft);
        }
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_ENVELOPE {
        return Err("decision scope envelope budget".into());
    }
    Ok(bytes)
}
fn send_scopes(
    host: &mut WorkbenchState,
    out: host_capnp::response::Builder<'_>,
    correlation: &str,
    scopes: &[(String, String)],
    request_cursor: &str,
    request_limit: u32,
    next_cursor: &str,
) -> Result<()> {
    let bytes = scopes_envelope(scopes, request_cursor, request_limit, next_cursor)?;
    let download = editor_draft_api::envelope_chunk(host, out, bytes, 0)?;
    editor_draft_api::bind_reply(host, correlation, download);
    Ok(())
}
