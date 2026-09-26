//! Bounded private scheduler adapter. All paths originate at the trusted UI.
use super::{io_state_reply, text, wire};
use crate::{
    Result, Workbench,
    io_tasks::{TaskKey, file::FileStart},
};
use morrow_plugin_runtime::io_jobs::FileResponse;

pub(super) fn handle(
    host: &mut Workbench,
    r: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    match r.get_action()? {
        wire::Action::FileStart => {
            host.local_state_mut()?;
            let start = r.get_file_start()?;
            host.start_selected_file(FileStart {
                submission: start.get_submission()?.try_into()?,
                package_id: text(start.get_package_id())?,
                digest: start.get_package_digest()?.try_into()?,
                revision: start.get_registry_revision(),
                handler: text(start.get_handler())?,
                selected_path: text(start.get_selected_path())?.into(),
                max_bytes: start.get_max_bytes(),
                timeout_ms: start.get_timeout_ms(),
            })?;
        }
        wire::Action::FileChunk => host.request_file_chunk(
            TaskKey::from_bytes(r.get_io_key()?)?,
            r.get_offset(),
            r.get_limit(),
        )?,
        wire::Action::FileFinish => host.finish_file(TaskKey::from_bytes(r.get_io_key()?)?)?,
        wire::Action::FileRead => {
            let result = host.read_file_result(TaskKey::from_bytes(r.get_io_key()?)?)?;
            let mut row = out.reborrow().init_file_result();
            match result {
                None => row.set_kind(0),
                Some(FileResponse::Captured(value)) => {
                    row.set_kind(1);
                    row.set_length(value.length);
                    row.set_sha256(&value.sha256);
                }
                Some(FileResponse::Chunk { offset, bytes, eof }) => {
                    row.set_kind(2);
                    row.set_offset(offset);
                    row.set_bytes(&bytes);
                    row.set_eof(eof);
                }
                Some(FileResponse::Finished) => row.set_kind(3),
            }
        }
        _ => return Err("not a file scheduler action".into()),
    }
    io_state_reply(
        &host.io_status(),
        host.http_submission(),
        out.init_io_state(),
    );
    Ok(())
}
