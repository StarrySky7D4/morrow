//! Private Cap'n Proto UI route for one original bounded observation.
//! The raw nomination is host input and must still pass native confirmation.
use super::{io_error, io_state_reply, text, wire};
use crate::{
    Result, Workbench,
    io_tasks::{TaskKey, fs_watch::WatchStart},
};
use morrow_plugin_runtime::fs_watch_worker::{Error, Phase};
use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};

fn nomination(bytes: &[u8]) -> Result<PathBuf> {
    if !(6..=8192).contains(&bytes.len()) || bytes.len() % 2 != 0 {
        return Err("invalid original directory spelling".into());
    }
    let units: Vec<_> = bytes
        .chunks_exact(2)
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .collect();
    if units.contains(&0) {
        return Err("directory spelling contains a terminator".into());
    }
    Ok(PathBuf::from(OsString::from_wide(&units)))
}
fn error(value: &Error) -> u16 {
    match value {
        Error::Approval(_) => 1,
        Error::Native(_) => 2,
        Error::Guest(_) => 3,
        Error::Owner => 4,
        Error::Protocol => 5,
        Error::ObservationEnded(_) => 6,
        Error::Cancelled => 7,
        Error::Expired => 8,
        Error::Spawn => 9,
        Error::Unknown => 10,
        Error::Consumed => 11,
    }
}
pub(super) fn handle(
    host: &mut Workbench,
    request: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    let action = request.get_action()?;
    let key = if action == wire::Action::FsWatchStart {
        if !request.get_io_key()?.is_empty() {
            return Err("a new observation cannot adopt an existing key".into());
        }
        let r = request.get_fs_watch_start()?;
        let value = WatchStart {
            submission: r.get_submission()?.try_into()?,
            package_id: text(r.get_package_id())?,
            digest: r.get_package_digest()?.try_into()?,
            revision: r.get_registry_revision(),
            handler: text(r.get_handler())?,
            nominated_path: nomination(r.get_nominated_path_utf16_le()?)?,
            max_frames: r.get_max_frames(),
            max_wire_bytes: r.get_max_wire_bytes(),
            window_ms: u64::from(r.get_window_ms()),
        };
        host.start_fs_watch(value)?
    } else {
        TaskKey::from_bytes(request.get_io_key()?)?
    };
    match action {
        wire::Action::FsWatchStart | wire::Action::FsWatchStatus => {}
        wire::Action::FsWatchRead => {
            if let Some(value) = host.read_fs_watch(key)? {
                let mut r = out.reborrow().init_fs_watch_result();
                r.set_present(true);
                r.set_success(value.outcome.is_ok());
                if let Err(ref failure) = value.outcome {
                    r.set_error(error(failure));
                    r.set_detail(&format!("{failure:?}"));
                }
                r.set_unknown(value.unknown);
                r.set_original_worker_joined(value.original_worker_joined);
                r.set_native_retirement_complete(value.native_retirement_complete);
                if let Some(disconnect) = value.disconnect {
                    r.set_has_disconnect(true);
                    r.set_disconnect(io_error(disconnect));
                }
                if let Some(maintenance) = value.maintenance {
                    r.set_has_maintenance(true);
                    r.set_maintenance(io_error(maintenance));
                }
                r.set_resources(value.usage.resources);
                r.set_jobs(value.usage.jobs);
                r.set_charged_bytes(value.usage.bytes);
                if let Some(native) = value.snapshot.and_then(|v| v.native) {
                    r.set_has_snapshot(true);
                    r.set_picker_started(native.ancestors.picker_started);
                    r.set_picker_closed(native.ancestors.picker_closed);
                    r.set_ancestor_objects(native.ancestors.objects.try_into()?);
                    r.set_ancestor_closed(native.ancestors.confirmed_closed.try_into()?);
                    if let Some(handles) = native.native {
                        r.set_has_native(true);
                        r.set_native_handles(handles.owned_handles);
                        r.set_native_closed(handles.confirmed_closed);
                        r.set_cancel_requested(handles.cancel_called);
                    }
                }
                let mut deliveries = r.init_deliveries(value.deliveries.len().try_into()?);
                for (index, value) in value.deliveries.iter().enumerate() {
                    let mut row = deliveries.reborrow().get(index.try_into()?);
                    row.set_sequence(value.sequence);
                    row.set_digest(&value.digest);
                    row.set_host_calls(value.execution.host_calls);
                    row.set_fuel_remaining(value.execution.fuel_remaining);
                    match &value.execution.outcome {
                        Ok(code) => row.set_exit_code(*code),
                        Err(fault) => {
                            row.set_execution_fault(super::blob_upload::fault(fault.clone()))
                        }
                    }
                }
            }
        }
        _ => return Err("not an observation scheduler action".into()),
    }
    let value = host.poll_fs_watch(key)?;
    let mut state = out.reborrow().init_fs_watch_state();
    state.set_ready(value.ready);
    state.set_delivered(value.delivered);
    if let Some(progress) = value.progress {
        state.set_phase(match progress.phase {
            Phase::Starting => 1,
            Phase::Observing => 2,
            Phase::Finishing => 3,
            Phase::Finished => 4,
        });
        state.set_frames(progress.delivered);
    }
    io_state_reply(&value.task, host.http_submission(), out.init_io_state());
    Ok(())
}
