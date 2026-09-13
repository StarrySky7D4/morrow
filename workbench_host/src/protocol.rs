//! Private binary UI transport. The child process and all paths are selected by
//! the trusted Flutter host; plugins receive only the registered business task.
use crate::{Result, Workbench, host_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_workbench_plugin::{Action, Response, codec};
use sha2::{Digest, Sha256};
pub fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/host.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn text(v: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    Ok(v?.to_str()?.to_owned())
}
pub fn respond(host: &mut Workbench, bytes: &[u8]) -> Result<Vec<u8>> {
    let mut output = Builder::new_default();
    let mut out = output.init_root::<wire::response::Builder>();
    out.set_version(1);
    out.set_digest(&digest());
    if let Err(e) = handle(host, bytes, out.reborrow()) {
        out.set_error(e.to_string().as_str());
    }
    out.set_read_only(!host.writable());
    if let Some(warning) = host.maintenance_warning() {
        out.set_maintenance_warning(warning);
    }
    let bytes = serialize::write_message_to_words(&output);
    if bytes.len() > 128 * 1024 {
        // Clearing pointers on the old builder leaves its oversized allocation in
        // the serialized segments. Use a fresh message and deliver no partial result.
        // The request may already have executed; this is a delivery error only.
        let mut bounded = Builder::new_default();
        let mut error = bounded.init_root::<wire::response::Builder>();
        error.set_version(1);
        error.set_digest(&digest());
        error.set_read_only(!host.writable());
        error.set_error("response exceeds 128 KiB frame budget; no result delivered; verify any operation before retrying");
        return Ok(serialize::write_message_to_words(&bounded));
    }
    Ok(bytes)
}
fn handle(host: &mut Workbench, bytes: &[u8], mut out: wire::response::Builder<'_>) -> Result<()> {
    if bytes.len() > 128 * 1024 {
        return Err("host frame budget".into());
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(32768),
            nesting_limit: 20,
        },
    )?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing host bytes".into());
    }
    let r = message.get_root::<wire::request::Reader>()?;
    if r.get_version() != 1 || r.get_digest()? != digest() {
        return Err("host contract mismatch".into());
    }
    let id = text(r.get_id())?;
    match r.get_action()? {
        wire::Action::PluginState => {
            host.refresh_plugin_state();
            plugin_status(host, out.reborrow());
        }
        wire::Action::PluginConfigure => {
            if r.get_limit() > 1 {
                return Err("invalid enable decision".into());
            }
            host.configure_plugin(r.get_revision(), r.get_sha256()?, r.get_limit() == 1)?;
            plugin_status(host, out.reborrow());
        }
        wire::Action::UiOpen => {
            ui_reply(host.ui_open(&text(r.get_name())?)?, out.reborrow());
        }
        wire::Action::UiEvent => {
            ui_reply(
                host.ui_event(r.get_offset(), r.get_payload()?)?,
                out.reborrow(),
            );
        }
        wire::Action::UiClose => {
            host.ui_close(r.get_offset());
        }

        wire::Action::BackupSnapshot => {
            host.backup_snapshot(std::path::Path::new(&text(r.get_selected_path())?))?;
        }
        wire::Action::BackupProtection => {
            host.backup_key(std::path::Path::new(&text(r.get_selected_path())?))?;
        }
        wire::Action::Read => {
            let record = host.read(&id)?;
            out.set_revision(record.revision);
            out.set_payload(&codec::encode_response(&Response {
                idea: record.idea,
                ids: vec![],
            })?);
        }
        wire::Action::Page => {
            let (records, cursor) = host.page(&text(r.get_cursor())?, r.get_limit())?;
            out.set_cursor(cursor.as_str());
            let mut ids = out.reborrow().init_ids(records.len() as u32);
            for (i, record) in records.iter().enumerate() {
                ids.set(i as u32, record.idea.id.as_str());
            }
        }
        wire::Action::Mutate => {
            if !text(r.get_capture_scope())?.is_empty() {
                return Err("captured saves require their complete editor metadata".into());
            }
            let req = codec::decode_request(r.get_payload()?)?;
            let operation = text(r.get_operation())?;
            let record = if req.action == Action::Create {
                if req.proposed.id != id || r.get_revision() != 0 {
                    return Err("create target".into());
                }
                host.create(&operation, req.proposed)?
            } else {
                host.apply(crate::Mutation {
                    operation: &operation,
                    id: &id,
                    revision: r.get_revision(),
                    action: req.action,
                    proposed: Some(req.proposed),
                    text: &req.text,
                    flag: req.flag,
                })?
            };
            out.set_revision(record.revision);
            out.set_payload(&codec::encode_response(&Response {
                idea: record.idea,
                ids: vec![],
            })?);
        }
        wire::Action::Query => {
            let req = codec::decode_request(r.get_payload()?)?;
            if req.action != Action::Query {
                return Err("query route".into());
            }
            let result = host.query(&req.section, &req.filter, &req.text, &req.sort)?;
            if result.len() > 4096 {
                return Err("query response pagination required".into());
            }
            let mut ids = out.reborrow().init_ids(result.len() as u32);
            for (i, id) in result.iter().enumerate() {
                ids.set(i as u32, id.as_str());
            }
        }
        wire::Action::ImportFile => {
            let path = text(r.get_selected_path())?;
            let mut file = std::fs::File::open(path)?;
            let meta = file.metadata()?;
            if !meta.is_file() {
                return Err("selected resource is not a file".into());
            }
            let asset = host.import(
                &id,
                &text(r.get_name())?,
                &text(r.get_kind())?,
                &mut file,
                meta.len(),
            )?;
            let mut idea = morrow_workbench_plugin::Idea::default();
            idea.assets.push(asset);
            out.set_payload(&codec::encode_response(&Response { idea, ids: vec![] })?);
        }
        wire::Action::ExportFile => {
            let path = text(r.get_selected_path())?;
            let path = std::path::Path::new(&path);
            let parent = path.parent().ok_or("export parent")?;
            if !parent.is_dir() {
                return Err("export directory unavailable".into());
            }
            // Exclusive destination: never truncate a file on a failed/partial export.
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            let result = host
                .export(&id, &text(r.get_attachment())?, &mut file)
                .and_then(|_| {
                    file.sync_all()?;
                    Ok(())
                });
            drop(file);
            if result.is_err() {
                let _ = std::fs::remove_file(path);
            }
            result?;
        }
        wire::Action::ReadPreferences => {
            if let Some(bytes) = host.read_preferences()? {
                let part = host.transfers.open(bytes, crate::now(host.start))?;
                write_chunk(out.reborrow(), &part);
            }
        }
        wire::Action::ReadPreferencesPart => {
            let part = host.transfers.read(
                &text(r.get_transfer())?,
                r.get_offset(),
                crate::now(host.start),
            )?;
            write_chunk(out.reborrow(), &part);
        }
        wire::Action::BeginPreferences => {
            if !host.writable() {
                return Err("plugin unavailable".into());
            }
            let token = host.transfers.begin(
                text(r.get_operation())?,
                r.get_total_length(),
                r.get_sha256()?,
                crate::now(host.start),
            )?;
            out.set_transfer(token.as_str());
        }
        wire::Action::AppendPreferences => {
            let token = text(r.get_transfer())?;
            let offset = host.transfers.append(
                &token,
                r.get_offset(),
                r.get_payload()?,
                crate::now(host.start),
            )?;
            out.set_transfer(token.as_str());
            out.set_offset(offset as u64);
        }
        wire::Action::FinishPreferences => {
            let token = text(r.get_transfer())?;
            let (operation, bytes) = host.transfers.finish(&token, crate::now(host.start))?;
            let bytes = host.save_preferences(&operation, bytes)?;
            // A committed result is acknowledged by its full digest; download is a separate snapshot.
            out.set_sha256(&Sha256::digest(&bytes));
            out.set_total_length(bytes.len() as u64);
        }
        wire::Action::AbortPreferences => {
            host.transfers.abort(&text(r.get_transfer())?);
        }
        wire::Action::SavePreferences => {
            // Retain the small-message route for internal callers only.
            let input = r.get_payload()?;
            if input.len() > 65536 {
                return Err("inline preferences budget".into());
            }
            out.set_payload(&host.save_preferences(&text(r.get_operation())?, input.to_vec())?);
        }
        wire::Action::OpenCaptureScope => {
            let scope = host.open_capture_scope(&id, r.get_revision())?;
            out.set_capture_scope(&scope);
        }
        wire::Action::CloseCaptureScope => {
            host.close_capture_scope(&text(r.get_capture_scope())?);
        }
        wire::Action::BeginCaptureUpload => {
            host.prepare_write()?;
            let token = host.capture_transfers.begin(
                text(r.get_operation())?,
                r.get_total_length(),
                r.get_sha256()?,
                crate::now(host.start),
            )?;
            out.set_transfer(&token);
        }
        wire::Action::AppendCaptureUpload => {
            let token = text(r.get_transfer())?;
            let offset = host.capture_transfers.append(
                &token,
                r.get_offset(),
                r.get_payload()?,
                crate::now(host.start),
            )?;
            out.set_transfer(&token);
            out.set_offset(offset as u64);
        }
        wire::Action::AbortCaptureUpload => {
            host.capture_transfers.abort(&text(r.get_transfer())?);
        }
        wire::Action::FinishPaste => {
            let (operation, bytes) = host
                .capture_transfers
                .finish(&text(r.get_transfer())?, crate::now(host.start))?;
            let message = editor_message(&bytes)?;
            let upload = message.get_root::<wire::paste_upload::Reader>()?;
            let event = paste_event(upload.get_event()?)?;
            if event.id != operation {
                return Err("paste upload identity mismatch".into());
            }
            host.record_paste(&text(upload.get_scope())?, event)?;
        }
        wire::Action::FinishCapturedSave => {
            let (operation, bytes) = host
                .capture_transfers
                .finish(&text(r.get_transfer())?, crate::now(host.start))?;
            let message = editor_message(&bytes)?;
            let upload = message.get_root::<wire::captured_save::Reader>()?;
            if text(upload.get_operation())? != operation {
                return Err("save upload identity mismatch".into());
            }
            let target = text(upload.get_target())?;
            let scope = text(upload.get_scope())?;
            let revision = upload.get_revision();
            let req = codec::decode_request(upload.get_payload()?)?;
            let snapshot = editor_snapshot(upload.get_snapshot()?)?;
            let record = match req.action {
                Action::Create if revision == 0 && req.proposed.id == target => {
                    host.create_captured(&operation, req.proposed, &scope, snapshot)?
                }
                Action::Edit if revision > 0 && req.proposed.id == target => host.apply_captured(
                    crate::Mutation {
                        operation: &operation,
                        id: &target,
                        revision,
                        action: req.action,
                        proposed: Some(req.proposed),
                        text: &req.text,
                        flag: req.flag,
                    },
                    &scope,
                    snapshot,
                )?,
                _ => return Err("captured save target or route".into()),
            };
            out.set_revision(record.revision);
            out.set_payload(&codec::encode_response(&Response {
                idea: record.idea,
                ids: vec![],
            })?);
        }
        wire::Action::Capture => {
            let scope = text(r.get_capture_scope())?;
            let parent = text(r.get_capture_parent())?;
            if scope.is_empty() {
                if !parent.is_empty() {
                    return Err("capture parent requires an editor scope".into());
                }
                out.set_payload(&host.capture(r.get_payload()?.to_vec())?);
            } else {
                let (ticket, bytes) =
                    host.capture_scoped(&scope, r.get_payload()?.to_vec(), &parent)?;
                out.set_capture_ticket(&ticket);
                out.set_payload(&bytes);
            }
        }
        wire::Action::Service => {
            out.set_payload(&host.service(r.get_payload()?.to_vec())?);
        }
    }
    Ok(())
}

fn write_chunk(mut out: wire::response::Builder<'_>, part: &crate::transfer::Chunk) {
    out.set_transfer(part.token.as_str());
    out.set_offset(part.offset as u64);
    out.set_total_length(part.total as u64);
    out.set_sha256(&part.sha);
    out.set_payload(&part.bytes);
}

fn plugin_status(host: &Workbench, mut out: wire::response::Builder<'_>) {
    let s = host.plugin_status();
    out.set_revision(s.revision);
    out.set_sha256(&s.digest);
    out.set_plugin_enabled(s.enabled);
    out.set_plugin_approved(s.approved);
    out.set_plugin_available(s.available);
}
fn ui_reply(reply: morrow_plugin_runtime::inline_ui::Reply, mut out: wire::response::Builder<'_>) {
    use morrow_plugin_runtime::inline_ui::Failure;
    out.set_ui_view(reply.view.as_str());
    out.set_ui_generation(reply.generation);
    out.set_revision(reply.revision);
    out.set_ui_serial(reply.serial);
    if let Some(document) = reply.document {
        out.set_payload(&document);
    }
    if let Some(failure) = reply.failure {
        let (code, message) = match failure {
            Failure::Plugin(f) => (2, f.message),
            Failure::Execution(_) => (3, "插件未能完成这次操作，当前内容已保留。".into()),
            Failure::InvalidDocument => (1, "表单操作或返回内容无效，请检查输入。".into()),
            Failure::Revoked => (4, "插件已停用，请重新打开表单。".into()),
        };
        out.set_ui_code(code);
        out.set_ui_failure(message.as_str());
    }
}

// Large editor metadata uses the same bounded ordered transfer primitive in a separate lane.
// It remains runtime Cap'n Proto; the host produces the persistent Protobuf projection itself.
fn editor_message(bytes: &[u8]) -> Result<capnp::message::Reader<capnp::serialize::OwnedSegments>> {
    if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
        return Err("editor upload bounds".into());
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(1024 * 1024),
            nesting_limit: 20,
        },
    )?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing editor upload bytes".into());
    }
    Ok(message)
}
fn paste_event(r: wire::paste_event::Reader<'_>) -> Result<crate::capture_provenance::PasteEvent> {
    use crate::capture_provenance::{PasteEvent, PastePart};
    let raw = r.get_parts()?;
    if raw.len() > 1024 {
        return Err("paste part budget".into());
    }
    let parts = raw
        .iter()
        .map(|v| {
            Ok(PastePart {
                ticket: text(v.get_ticket())?,
                literal: text(v.get_literal())?,
                selection: text(v.get_selection())?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(PasteEvent {
        id: text(r.get_id())?,
        field: text(r.get_field())?,
        before: text(r.get_before())?,
        start_utf16: r.get_start_utf16(),
        end_utf16: r.get_end_utf16(),
        parts,
        after: text(r.get_after())?,
    })
}
fn editor_snapshot(
    r: wire::editor_snapshot::Reader<'_>,
) -> Result<crate::capture_provenance::EditorSnapshot> {
    use crate::capture_provenance::{AttachmentAlias, EditorSnapshot};
    let raw = r.get_aliases()?;
    if raw.len() > 1024 {
        return Err("editor alias budget".into());
    }
    let aliases = raw
        .iter()
        .map(|v| {
            Ok(AttachmentAlias {
                id: text(v.get_id())?,
                location: text(v.get_location())?,
                name: text(v.get_name())?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(EditorSnapshot {
        title: text(r.get_title())?,
        description: text(r.get_description())?,
        hypothesis: text(r.get_hypothesis())?,
        conclusion: text(r.get_conclusion())?,
        todos: text(r.get_todos())?,
        aliases,
    })
}
