//! Private Cap'n Proto directory commands; original worker owns every OS call.
use super::{io_state_reply, text, wire};
use crate::{
    Result, Workbench,
    io_tasks::{TaskKey, directory::DirectoryStart},
};
use morrow_plugin_runtime::{Fault, io_jobs::DirectoryResponse};
use std::{ffi::OsString, os::windows::ffi::OsStringExt, time::Duration};

pub(super) fn handle(
    host: &mut Workbench,
    r: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    match r.get_action()? {
        wire::Action::DirectoryStart => {
            host.local_state_mut()?;
            let start = r.get_directory_start()?;
            let units = start.get_selected_path_utf16()?;
            if units.is_empty() || units.len() > 4096 {
                return Err("invalid selected directory spelling".into());
            }
            let units: Vec<u16> = units.iter().collect();
            host.start_selected_directory(DirectoryStart {
                submission: start.get_submission()?.try_into()?,
                package_id: text(start.get_package_id())?,
                digest: start.get_package_digest()?.try_into()?,
                revision: start.get_registry_revision(),
                handler: text(start.get_handler())?,
                selected_path: OsString::from_wide(&units).into(),
                max_entries: start.get_max_entries().try_into()?,
                max_metadata_bytes: start.get_max_metadata_bytes(),
                timeout_ms: start.get_timeout_ms(),
            })?;
        }
        wire::Action::DirectoryRunGuest => {
            let timeout = r.get_directory_timeout_ms();
            if timeout == 0 || timeout > 30_000 {
                return Err("invalid directory guest timeout".into());
            }
            host.request_directory_guest_frame(
                TaskKey::from_bytes(r.get_io_key()?)?,
                Duration::from_millis(u64::from(timeout)),
            )?;
        }
        wire::Action::DirectoryFinish => {
            host.finish_directory(TaskKey::from_bytes(r.get_io_key()?)?)?;
        }
        wire::Action::DirectoryRead => {
            let value = host.read_directory_result(TaskKey::from_bytes(r.get_io_key()?)?)?;
            let mut row = out.reborrow().init_directory_result();
            match value {
                None => row.set_kind(0),
                Some(DirectoryResponse::Captured(value)) => {
                    row.set_kind(1);
                    row.set_entries(value.entries.try_into()?);
                    row.set_metadata_bytes(value.metadata_bytes);
                }
                Some(DirectoryResponse::Finished) => row.set_kind(3),
                // This private route admits capture/finish only. Host-page
                // requests must not be mixed into this original guest lane.
                Some(DirectoryResponse::Page(_)) => {
                    return Err("unexpected directory page delivery".into());
                }
            }
        }
        wire::Action::DirectoryReadGuest => {
            let value = host.read_directory_guest_result(TaskKey::from_bytes(r.get_io_key()?)?)?;
            let mut row = out.reborrow().init_directory_result();
            if let Some(value) = value {
                row.set_kind(2);
                row.set_unknown(value.unknown);
                row.set_host_calls(value.execution.host_calls);
                if let Some(frame) = value.response {
                    row.set_frame(&frame);
                }
                match value.execution.outcome {
                    Ok(code) => row.set_exit_code(code),
                    Err(fault) => row.set_execution_fault(match fault {
                        Fault::TaskProtocol => 1,
                        Fault::Deadline => 2,
                        Fault::PackageBinding => 3,
                        Fault::InactiveConnection => 4,
                        Fault::InvalidModule => 5,
                        Fault::UnsupportedAbi => 6,
                        Fault::Limits => 7,
                        Fault::Cancelled => 8,
                        Fault::Trap => 9,
                    }),
                }
            }
        }
        _ => return Err("not a directory scheduler action".into()),
    }
    io_state_reply(
        &host.io_status(),
        host.http_submission(),
        out.init_io_state(),
    );
    Ok(())
}
