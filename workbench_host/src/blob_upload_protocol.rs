//! Private trusted UI route. Identifiers correlate one approved operation;
//! neither package approval nor a recovered submission grants another upload.
use super::{io_state_reply, text, wire};
use crate::{
    Result, Workbench,
    io_tasks::{
        TaskKey,
        blob_upload::{Observation, UploadStart},
    },
};
use morrow_blob_transfer_v1::{Identity, Limits as FrameLimits};
use morrow_blob_upload_request_v1::Phase;
use morrow_core::task::{Invocation, Transform};
use morrow_plugin_runtime::{Fault, Limits, blob_upload::Spec, blob_upload_worker::Error};
use std::time::Duration;

pub(super) fn is_action(action: wire::Action) -> bool {
    matches!(
        action,
        wire::Action::BlobUploadStart
            | wire::Action::BlobUploadStatus
            | wire::Action::BlobUploadRead
            | wire::Action::BlobUploadInspect
            | wire::Action::BlobUploadConfirm
    )
}
fn observation(value: Observation) -> u16 {
    match value {
        Observation::Absent => 1,
        Observation::Staged => 2,
        Observation::Released => 3,
    }
}
pub(super) fn fault(value: Fault) -> u16 {
    match value {
        Fault::TaskProtocol => 1,
        Fault::Deadline => 2,
        Fault::PackageBinding => 3,
        Fault::InactiveConnection => 4,
        Fault::InvalidModule => 5,
        Fault::UnsupportedAbi => 6,
        Fault::Limits => 7,
        Fault::Cancelled => 8,
        Fault::Trap => 9,
    }
}
fn error(value: Error) -> u16 {
    match value {
        Error::Denied => 1,
        Error::Limit => 2,
        Error::Protocol => 3,
        Error::Cancelled => 4,
        Error::Expired => 5,
        Error::Storage => 6,
        Error::Unknown => 7,
        Error::Owner => 8,
        Error::Spawn => 9,
        Error::Consumed => 10,
    }
}

pub(super) fn handle(
    host: &mut Workbench,
    request: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    let action = request.get_action()?;
    let key = if action == wire::Action::BlobUploadStart {
        let value = request.get_blob_upload_start()?;
        // Validate all bounded data before connecting or moving the original owner.
        let submission: [u8; 32] = value.get_submission()?.try_into()?;
        let input = value.get_input()?;
        if input.len() > morrow_core::task::MAX_VALUE_BYTES {
            return Err("upload input exceeds the private task budget".into());
        }
        let task_id: String = submission.iter().map(|b| format!("{b:02x}")).collect();
        let invocation = Invocation::new_transform(
            &task_id,
            Transform {
                handler: text(value.get_handler())?,
                input_type: text(value.get_input_type())?,
                output_type: text(value.get_output_type())?,
                input: input.to_vec(),
            },
        )?;
        host.start_blob_upload(UploadStart {
            submission,
            package_id: text(value.get_package_id())?,
            digest: value.get_package_digest()?.try_into()?,
            revision: value.get_registry_revision(),
            nomination_ref: value.get_nomination_ref()?.try_into()?,
            spec: Spec {
                owner_domain: value.get_owner_domain()?.try_into()?,
                identity: Identity {
                    transfer_epoch: value.get_transfer_epoch()?.try_into()?,
                    object_ref: value.get_object_ref()?.try_into()?,
                    operation_id: value.get_operation_id()?.try_into()?,
                },
                byte_length: value.get_byte_length(),
                sha256: value.get_whole_sha256()?.try_into()?,
                limits: FrameLimits::default(),
            },
            input: invocation,
            lifetime: Duration::from_millis(u64::from(value.get_timeout_ms())),
            limits: Limits {
                fuel: morrow_plugin_runtime::MAX_BLOB_UPLOAD_FUEL,
                host_calls: 1024,
                ..Limits::default()
            },
        })?
    } else {
        TaskKey::from_bytes(request.get_io_key()?)?
    };
    let mut result = out.reborrow().init_blob_upload_result();
    match action {
        wire::Action::BlobUploadStart | wire::Action::BlobUploadStatus => {}
        wire::Action::BlobUploadRead => {
            if let Some(value) = host.read_blob_upload(key)? {
                result.set_kind(if value.outcome.is_ok() { 1 } else { 2 });
                match value.outcome {
                    Ok(phase) => result.set_phase(match phase {
                        Phase::Absent => 1,
                        Phase::Staged => 2,
                        Phase::Released => 3,
                        Phase::Ready | Phase::Receiving => {
                            return Err("nonterminal upload outcome".into());
                        }
                    }),
                    Err(value) => result.set_error(error(value)),
                }
                result.set_unknown(value.unknown);
                result.set_calls(value.usage.calls);
                result.set_wire_bytes(value.usage.wire_bytes);
                result.set_native_work_bytes(value.usage.native_work_bytes);
                if let Some(report) = value.execution {
                    result.set_has_execution(true);
                    result.set_host_calls(report.host_calls);
                    match report.outcome {
                        Ok(code) => result.set_exit_code(code),
                        Err(value) => result.set_execution_fault(fault(value)),
                    }
                }
            }
        }
        wire::Action::BlobUploadInspect => {
            host.inspect_blob_upload_history(key)?;
        }
        wire::Action::BlobUploadConfirm => {
            let expected = match request.get_blob_upload_observation() {
                1 => Observation::Absent,
                2 => Observation::Staged,
                3 => Observation::Released,
                _ => return Err("upload recovery requires an explicit original observation".into()),
            };
            host.confirm_blob_upload_recovery(key, expected)?;
        }
        _ => return Err("not an upload scheduler action".into()),
    }
    let snapshot = host.poll_blob_upload(key)?;
    let mut state = out.reborrow().init_blob_upload_state();
    state.set_ready(snapshot.ready);
    state.set_delivered(snapshot.delivered);
    state.set_recovery_required(snapshot.recovery_required);
    state.set_observed(snapshot.observed.map_or(0, observation));
    io_state_reply(&snapshot.task, host.http_submission(), out.init_io_state());
    Ok(())
}
