//! Private application planning controls. A proposal is data; only an explicit
//! decision on the original retained plan can consume one upload admission.
use super::{text, wire};
use crate::{
    Result, Workbench,
    blob_upload_plans::{PlanApproval, PlanError, PlanKey, PlanSnapshot, PlanStart},
};
use morrow_core::task::{Invocation, Transform};
use std::time::Duration;

fn identity(bytes: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = bytes.try_into()?;
    if value == [0; 32] {
        return Err("empty planning identity".into());
    }
    Ok(value)
}
fn error(value: PlanError) -> u16 {
    match value {
        PlanError::Cancelled => 1,
        PlanError::Expired => 2,
        PlanError::StaleSelection => 3,
        PlanError::GateClosed => 4,
        PlanError::Protocol => 5,
        PlanError::Execution => 6,
        PlanError::PluginFailure => 7,
        PlanError::Preparation => 8,
        PlanError::Spawn => 9,
        PlanError::Worker => 10,
    }
}
fn state(value: PlanSnapshot, mut out: wire::blob_plan_state::Builder<'_>) {
    out.set_found(true);
    out.set_key(value.key.as_bytes());
    out.set_submission(&value.submission);
    out.set_running(value.running);
    out.set_original_worker_joined(value.original_worker_joined);
    out.set_ready(value.ready);
    out.set_delivered(value.delivered);
    out.set_consumed(value.consumed);
    out.set_error(value.error.map_or(0, error));
    if let Some(key) = value.upload_key {
        out.set_upload_key(key.as_bytes());
    }
    if let Some(submission) = value.upload_submission {
        out.set_upload_submission(&submission);
    }
    out.set_submission_failed(value.submission_failed);
}
pub(super) fn handle(
    host: &mut Workbench,
    request: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    let action = request.get_action()?;
    // Explicit absent envelopes distinguish discovery/acknowledgement from a
    // malformed or truncated reply. They never claim a past attempt failed.
    out.reborrow().init_blob_plan_state();
    out.reborrow().init_blob_plan_result();
    if action == wire::Action::BlobPlanDiscover {
        if let Some(value) =
            host.discover_blob_upload_plan(identity(request.get_blob_plan_submission()?)?)?
        {
            state(value, out.init_blob_plan_state());
        }
        return Ok(());
    }
    let key = if action == wire::Action::BlobPlanStart {
        let value = request.get_blob_plan_start()?;
        let submission = identity(value.get_submission()?)?;
        let digest = identity(value.get_package_digest()?)?;
        let input = value.get_input()?;
        if input.len()
            > morrow_core::plugin_package::blob_upload_proposal::MAX_PLAN_INPUT_BYTES as usize
        {
            return Err("planning input exceeds the private budget".into());
        }
        let id: String = submission.iter().map(|b| format!("{b:02x}")).collect();
        let invocation = Invocation::new_transform(
            &id,
            Transform {
                handler: text(value.get_handler())?,
                input_type: text(value.get_input_type())?,
                output_type: text(value.get_output_type())?,
                input: input.to_vec(),
            },
        )?;
        host.start_blob_upload_plan(PlanStart {
            submission,
            package_id: text(value.get_package_id())?,
            digest,
            revision: value.get_registry_revision(),
            input: invocation,
            lifetime: Duration::from_millis(u64::from(value.get_timeout_ms())),
        })?
    } else {
        PlanKey::from_bytes(request.get_blob_plan_key()?)?
    };
    match action {
        wire::Action::BlobPlanStart | wire::Action::BlobPlanStatus => {}
        wire::Action::BlobPlanRead => {
            if let Some(value) = host.read_blob_upload_plan(key)? {
                let mut result = out.reborrow().init_blob_plan_result();
                result.set_kind(if value.view.is_some() { 1 } else { 2 });
                result.set_error(value.error.map_or(0, error));
                if let Some(view) = value.view {
                    let mut row = result.reborrow().init_view();
                    row.set_package_id(view.package_id.as_str());
                    row.set_package_digest(&view.package_digest);
                    row.set_registry_revision(view.registry_revision);
                    row.set_planner_handler(view.planner_handler.as_str());
                    row.set_upload_handler(view.upload_handler.as_str());
                    row.set_planning_input_sha256(&view.planning_input_sha256);
                    row.set_label(view.label.as_str());
                    row.set_byte_length(view.byte_length);
                    row.set_whole_sha256(&view.whole_sha256);
                    row.set_proposal_sha256(&view.proposal_sha256);
                }
                if let Some(report) = value.execution {
                    result.set_has_execution(true);
                    result.set_host_calls(report.host_calls);
                    match report.outcome {
                        Ok(code) => result.set_exit_code(code),
                        Err(fault) => result.set_execution_fault(super::blob_upload::fault(fault)),
                    }
                }
                if let Some(failure) = value.failure {
                    result.set_has_plugin_failure(true);
                    result.set_plugin_failure_code(failure.code as u16);
                    // Guest error text may contain input data; do not return it.
                }
            }
        }
        wire::Action::BlobPlanCancel => {
            host.cancel_blob_upload_plan(key)?;
        }
        wire::Action::BlobPlanAcknowledge => {
            host.acknowledge_blob_upload_plan(key)?;
            return Ok(());
        }
        wire::Action::BlobPlanApprove => {
            let value = request.get_blob_plan_approval()?;
            host.approve_blob_upload_plan(PlanApproval {
                key,
                package_digest: identity(value.get_package_digest()?)?,
                registry_revision: value.get_registry_revision(),
                proposal_sha256: identity(value.get_proposal_sha256()?)?,
            })?;
        }
        _ => return Err("not a planning control".into()),
    }
    // Native retained identity survives a missing approval reply and even the
    // later upload acknowledgement; never substitute the currently active IO.
    state(host.poll_blob_upload_plan(key)?, out.init_blob_plan_state());
    Ok(())
}
