//! Private trusted UI route. Identifiers correlate one approved operation;
//! neither package approval nor a recovered submission grants another upload.
use super::{io_state_reply, text, wire};
use crate::{
    Result, Workbench,
    io_tasks::{
        TaskKey,
        blob_upload::{MetadataPhase, Observation, UploadStart},
    },
};
use morrow_blob_transfer_v1::{Identity, Limits as FrameLimits};
use morrow_blob_upload_request_v1::Phase;
use morrow_core::task::{Invocation, Transform};
use morrow_plugin_runtime::{Fault, Limits, blob_upload::Spec, blob_upload_worker::Error};
use std::time::Duration;

pub(super) fn is_history_action(action: wire::Action) -> bool {
    matches!(
        action,
        wire::Action::BlobUploadHistoryList | wire::Action::BlobUploadHistoryInspect
    )
}
pub(super) fn is_metadata_action(action: wire::Action) -> bool {
    matches!(
        action,
        wire::Action::BlobUploadMetadataReview | wire::Action::BlobUploadMetadataComplete
    )
}
pub(super) fn is_action(action: wire::Action) -> bool {
    matches!(
        action,
        wire::Action::BlobUploadStart
            | wire::Action::BlobUploadStatus
            | wire::Action::BlobUploadRead
            | wire::Action::BlobUploadInspect
            | wire::Action::BlobUploadConfirm
            | wire::Action::BlobUploadHistoryList
            | wire::Action::BlobUploadHistoryInspect
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
    if is_metadata_action(action) {
        let key = TaskKey::from_bytes(request.get_io_key()?)?;
        if action == wire::Action::BlobUploadMetadataReview {
            if !request.get_blob_upload_metadata_permit()?.is_empty() {
                return Err("review does not accept a completion permit".into());
            }
            let review = host.review_blob_upload_metadata(key)?;
            let mut row = out.init_blob_upload_metadata_review();
            row.set_key(key.as_bytes());
            row.set_submission(&review.submission);
            row.set_permit(&review.permit);
            row.set_phase(match review.phase {
                MetadataPhase::Observation => 1,
                MetadataPhase::Acknowledgement => 2,
            });
            row.set_write_required(review.write_required);
            row.set_original_unknown(review.original.unknown);
            use morrow_core::blob_upload_journal::Outcome;
            match review.original.outcome {
                Outcome::Absent => row.set_original_phase(1),
                Outcome::Staged => row.set_original_phase(2),
                Outcome::Released => row.set_original_phase(3),
                Outcome::Error(code) => row.set_original_error(code),
            }
            row.set_observed_at(review.observed_at);
            row.set_acknowledgement_at(review.acknowledgement_at.unwrap_or(0));
            return Ok(());
        }
        let permit = request.get_blob_upload_metadata_permit()?.try_into()?;
        let acknowledged = host.complete_blob_upload_metadata(key, permit)?;
        out.set_blob_upload_metadata_completed_permit(&permit);
        out.set_blob_upload_metadata_acknowledged(acknowledged);
        if acknowledged {
            io_state_reply(
                &host.io_status(),
                host.http_submission(),
                out.init_io_state(),
            );
            return Ok(());
        }
        let snapshot = host.poll_blob_upload(key)?;
        out.reborrow().init_blob_upload_result();
        let mut state = out.reborrow().init_blob_upload_state();
        state.set_ready(snapshot.ready);
        state.set_delivered(snapshot.delivered);
        state.set_recovery_required(snapshot.recovery_required);
        state.set_observed(snapshot.observed.map_or(0, observation));
        io_state_reply(&snapshot.task, host.http_submission(), out.init_io_state());
        return Ok(());
    }
    if matches!(
        action,
        wire::Action::BlobUploadHistoryList | wire::Action::BlobUploadHistoryInspect
    ) {
        return history(host, request, out);
    }
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

fn history_row(
    value: &morrow_core::blob_upload_journal::Record,
    mut out: wire::blob_upload_history::Builder<'_>,
) {
    use morrow_core::blob_upload_journal::Outcome;
    let request = value.request();
    out.set_submission(&request.submission);
    out.set_package_id(request.package_id.as_str());
    out.set_package_digest(&request.package_digest);
    out.set_registry_revision(request.registry_revision);
    out.set_nomination_ref(&request.nomination_ref);
    out.set_owner_domain(&request.command.owner_domain);
    out.set_operation_id(request.command.operation_id.as_str());
    out.set_byte_length(request.command.byte_length);
    out.set_whole_sha256(&request.command.sha256);
    out.set_handler(request.handler.as_str());
    out.set_input_sha256(&request.input_sha256);
    out.set_binding_sha256(&request.binding_sha256);
    out.set_reserved_at(value.reserved_at_unix_ms());
    out.set_revision(value.revision() as u16);
    out.set_result_recorded(value.original_result().is_some());
    out.set_observed_at(value.observed_at_unix_ms().unwrap_or(0));
    out.set_acknowledged(value.acknowledged_at_unix_ms().is_some());
    out.set_acknowledged_at(value.acknowledged_at_unix_ms().unwrap_or(0));
    if let Some(value) = value.original_result() {
        let mut result = out.init_original_result();
        result.set_kind(if matches!(value.outcome, Outcome::Error(_)) {
            2
        } else {
            1
        });
        match value.outcome {
            Outcome::Absent => result.set_phase(1),
            Outcome::Staged => result.set_phase(2),
            Outcome::Released => result.set_phase(3),
            Outcome::Error(error) => result.set_error(error),
        }
        result.set_unknown(value.unknown);
        result.set_calls(value.calls);
        result.set_wire_bytes(value.wire_bytes);
        result.set_native_work_bytes(value.native_work_bytes);
        if let Some(execution) = value.execution {
            result.set_has_execution(true);
            result.set_host_calls(execution.host_calls);
            match execution.outcome {
                Ok(code) => result.set_exit_code(code),
                Err(fault) => result.set_execution_fault(fault),
            }
        }
    }
}
fn history(
    host: &Workbench,
    request: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    let query = request.get_blob_upload_history_query()?;
    // A history query cannot target a live task or reinterpret a cursor as one.
    if !request.get_io_key()?.is_empty() {
        return Err("history accepts no live task key".into());
    }
    if request.get_action()? == wire::Action::BlobUploadHistoryList {
        if !query.get_submission()?.is_empty() {
            return Err("list accepts no exact submission".into());
        }
        let bytes = query.get_after()?;
        let after = if bytes.is_empty() {
            None
        } else {
            Some(<&[u8; 32]>::try_from(bytes)?)
        };
        let values = host.list_blob_upload_history(after, u32::from(query.get_limit()))?;
        if let Some(last) = values.last() {
            out.set_blob_upload_history_cursor(&last.request().submission);
        }
        let mut rows = out.init_blob_upload_history(values.len() as u32);
        for (i, value) in values.iter().enumerate() {
            history_row(value, rows.reborrow().get(i as u32));
        }
    } else {
        if !query.get_after()?.is_empty() || query.get_limit() != 0 {
            return Err("inspect accepts only an exact submission".into());
        }
        let submission = query.get_submission()?.try_into()?;
        let value = host.inspect_persisted_blob_upload(&submission)?;
        out.set_blob_upload_current_stage(observation(value.current_stage));
        history_row(&value.record, out.init_blob_upload_history(1).get(0));
    }
    Ok(())
}
