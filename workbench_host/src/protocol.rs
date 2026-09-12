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
    out.set_read_only(!host.writable());
    if let Err(e) = handle(host, bytes, out.reborrow()) {
        out.set_error(e.to_string().as_str());
    }
    if let Some(warning) = host.maintenance_warning() {
        out.set_maintenance_warning(warning);
    }
    let bytes = serialize::write_message_to_words(&output);
    if bytes.len() > 128 * 1024 {
        return Err("response frame budget".into());
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
        wire::Action::Capture => {
            out.set_payload(&host.capture(r.get_payload()?.to_vec())?);
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
