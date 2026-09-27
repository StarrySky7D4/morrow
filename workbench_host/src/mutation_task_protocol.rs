//! Private outer scheduler only. Paths/scopes originate in trusted native UI.
use super::{io_state_reply, text, wire};
use crate::{
    Result, Workbench,
    io_tasks::{
        TaskKey,
        mutation::{
            Action, CommandKind, DiscoveryStart, Failure, MutationSnapshot, ReconciliationStart,
            StartRequest, checkpoint_token,
        },
    },
};
use morrow_core::{
    file_mutation::{Disposition, RequestRecord},
    file_path::RelativeFilePath,
};
use morrow_plugin_runtime::{
    file_target::Error as TargetError,
    io_jobs::{MAX_MUTATION_CHUNK, MutationResponse, OwnerCommandError, OwnerCommandPoll},
};
use zeroize::Zeroizing;

pub(super) fn handle(
    host: &mut Workbench,
    r: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    let action = r.get_action()?;
    let mut command = r.get_mutation_command_id();
    let key = if action == wire::Action::MutationDiscover {
        if !r.get_io_key()?.is_empty()
            || command != 0
            || r.has_mutation_start()
            || r.has_mutation_reconcile()
            || r.has_mutation_command()
        {
            return Err("mixed mutation discovery fields".into());
        }
        let start = r.get_mutation_discover()?;
        let key = host.start_mutation_discovery_request(DiscoveryStart {
            submission: start.get_submission()?.try_into()?,
            package_id: text(start.get_package_id())?,
            digest: start.get_package_digest()?.try_into()?,
            revision: start.get_registry_revision(),
            subject: text(start.get_subject())?,
            disposition: match start.get_disposition() {
                1 => Disposition::Create,
                2 => Disposition::Replace,
                3 => Disposition::Delete,
                _ => return Err("unknown mutation disposition".into()),
            },
            scan_limit: start.get_scan_limit(),
            checkpoint: match start.get_checkpoint()? {
                [] => None,
                bytes => Some(bytes.try_into()?),
            },
            timeout_ms: start.get_timeout_ms(),
        })?;
        command = 1;
        key
    } else if action == wire::Action::MutationReconcile {
        if !r.get_io_key()?.is_empty()
            || command != 0
            || r.has_mutation_start()
            || r.has_mutation_command()
            || r.has_mutation_discover()
        {
            return Err("mixed mutation reconciliation fields".into());
        }
        let start = r.get_mutation_reconcile()?;
        let key = host.start_mutation_reconciliation_request(ReconciliationStart {
            submission: start.get_submission()?.try_into()?,
            package_id: text(start.get_package_id())?,
            digest: start.get_package_digest()?.try_into()?,
            revision: start.get_registry_revision(),
            plan: RequestRecord::decode(start.get_plan()?)?,
            timeout_ms: start.get_timeout_ms(),
        })?;
        command = 1;
        key
    } else if action == wire::Action::MutationStart {
        if !r.get_io_key()?.is_empty()
            || command != 0
            || r.has_mutation_command()
            || r.has_mutation_reconcile()
            || r.has_mutation_discover()
        {
            return Err("mixed mutation start fields".into());
        }
        let start = r.get_mutation_start()?;
        let disposition = match start.get_disposition() {
            1 => Disposition::Create,
            2 => Disposition::Replace,
            3 => Disposition::Delete,
            _ => return Err("unknown mutation disposition".into()),
        };
        let relative = text(start.get_relative_path())?;
        let relative_path = if relative.is_empty() {
            None
        } else {
            Some(RelativeFilePath::parse(&relative)?)
        };
        let key = host.start_selected_mutation(StartRequest {
            submission: start.get_submission()?.try_into()?,
            package_id: text(start.get_package_id())?,
            digest: start.get_package_digest()?.try_into()?,
            revision: start.get_registry_revision(),
            disposition,
            selected_path: text(start.get_selected_path())?.into(),
            relative_path,
            subject: text(start.get_subject())?,
            approval: start.get_approval_sha256()?.try_into()?,
            timeout_ms: start.get_timeout_ms(),
        })?;
        command = 1;
        key
    } else {
        if r.has_mutation_start() || r.has_mutation_reconcile() || r.has_mutation_discover() {
            return Err("mixed mutation command fields".into());
        }
        let key = TaskKey::from_bytes(r.get_io_key()?)?;
        match action {
            wire::Action::MutationSubmit => {
                if command != 0 {
                    return Err("submission cannot supply a host command id".into());
                }
                let c = r.get_mutation_command()?;
                let submission = c.get_submission()?.try_into()?;
                let plan = c.get_plan()?;
                let bytes = c.get_bytes()?;
                let offset = c.get_offset();
                let kind = c.get_kind();
                let operation = c.get_operation_id()?;
                let content_length = c.get_content_length();
                let content_hash = c.get_content_sha256()?;
                if (kind != 10 && c.get_scan_limit() != 0)
                    || (kind != 1 && !plan.is_empty())
                    || (kind != 2 && (!bytes.is_empty() || offset != 0))
                    || (kind != 9
                        && (!operation.is_empty()
                            || content_length != 0
                            || !content_hash.is_empty()))
                {
                    return Err("mixed mutation payload fields".into());
                }
                let action = match kind {
                    10 => Action::NextPlans {
                        scan_limit: c.get_scan_limit(),
                    },
                    9 => {
                        if operation.len() > 256 {
                            return Err("mutation operation identity limit".into());
                        }
                        Action::BuildPlan {
                            operation_id: operation.to_str()?.to_owned(),
                            content_length,
                            content_sha256: if content_hash.is_empty() {
                                None
                            } else {
                                Some(content_hash.try_into()?)
                            },
                        }
                    }
                    1 => Action::Prepare(Box::new(RequestRecord::decode(plan)?)),
                    2 => {
                        if bytes.len() > MAX_MUTATION_CHUNK {
                            return Err("mutation chunk limit".into());
                        }
                        Action::Chunk {
                            offset,
                            bytes: Zeroizing::new(bytes.to_vec()),
                        }
                    }
                    3 => Action::CommitContent,
                    4 => Action::Execute,
                    5 => Action::Query,
                    6 => Action::CancelPlan,
                    7 => Action::Release,
                    _ => return Err("unknown mutation command".into()),
                };
                command = host.submit_mutation(key, submission, action)?;
            }
            wire::Action::MutationStatus
            | wire::Action::MutationRead
            | wire::Action::MutationCancelCommand => {
                if r.has_mutation_command() {
                    return Err("unexpected mutation payload".into());
                }
                if action == wire::Action::MutationRead {
                    result_reply(
                        host.read_mutation_result(key, command)?,
                        out.reborrow().init_mutation_result(),
                        checkpoint_token(key, command),
                    )?;
                } else if action == wire::Action::MutationCancelCommand {
                    host.cancel_mutation_command(key, command)?;
                } else if command != 0 {
                    return Err("status reads current command without an id".into());
                }
            }
            _ => return Err("not a mutation scheduler action".into()),
        }
        key
    };
    state_reply(
        host.mutation_status(key)?,
        out.reborrow().init_mutation_state(),
    );
    out.set_mutation_command_id(command);
    io_state_reply(
        &host.io_status(),
        host.http_submission(),
        out.init_io_state(),
    );
    Ok(())
}
fn state_reply(state: MutationSnapshot, mut out: wire::mutation_state::Builder<'_>) {
    out.set_command(state.command);
    out.set_kind(match state.kind {
        CommandKind::Select => 0,
        CommandKind::Prepare => 1,
        CommandKind::Chunk => 2,
        CommandKind::CommitContent => 3,
        CommandKind::Execute => 4,
        CommandKind::Query => 5,
        CommandKind::CancelPlan => 6,
        CommandKind::Release => 7,
        CommandKind::Reconcile => 8,
        CommandKind::BuildPlan => 9,
        CommandKind::Discover => 10,
    });
    out.set_delivery(match state.delivery {
        OwnerCommandPoll::Pending => 0,
        OwnerCommandPoll::Ready => 1,
        OwnerCommandPoll::Consumed => 2,
    });
    out.set_selected(state.selected);
    out.set_reconcile_required(state.reconcile_required);
    out.set_terminal(state.terminal);
    if let Some(selected) = state.selection {
        out.set_reference(&selected.reference);
        if let Some(identity) = selected.expected_identity {
            out.set_expected_identity(&identity);
        }
    }
}
pub(super) fn result_reply(
    result: Option<std::result::Result<MutationResponse, Failure>>,
    mut out: wire::mutation_result::Builder<'_>,
    checkpoint_token: [u8; 32],
) -> Result<()> {
    match result {
        None => out.set_kind(0),
        Some(Ok(MutationResponse::Plans {
            plans,
            scanned,
            done,
            checkpoint,
        })) => {
            out.set_kind(12);
            if checkpoint.is_some() {
                out.set_checkpoint(&checkpoint_token);
            }
            out.set_scanned(scanned);
            out.set_done(done);
            let mut list = out.reborrow().init_plans(plans.len() as u32);
            for (index, plan) in plans.iter().enumerate() {
                list.set(index as u32, plan.container());
            }
        }
        Some(Ok(MutationResponse::Planned(plan))) => {
            out.set_kind(11);
            out.set_plan(plan.container());
        }
        Some(Ok(MutationResponse::Selected {
            reference,
            expected_identity,
        })) => {
            out.set_kind(1);
            out.set_reference(&reference);
            if let Some(identity) = expected_identity {
                out.set_expected_identity(&identity);
            }
        }
        Some(Ok(MutationResponse::Prepared(record))) => {
            out.set_kind(2);
            record_reply(&record, out.reborrow())?;
        }
        Some(Ok(MutationResponse::Staged { bytes, durable })) => {
            out.set_kind(3);
            out.set_staged_bytes(bytes);
            out.set_durable_content(durable);
        }
        Some(Ok(MutationResponse::Created(outcome))) => {
            out.set_kind(4);
            out.set_phase(3);
            out.set_operation_id(outcome.operation_id());
            match outcome.result() {
                morrow_core::file_effect::CreateResult::Created => out.set_effect(1),
                morrow_core::file_effect::CreateResult::OsRejected { code } => {
                    out.set_effect(2);
                    out.set_os_code(code);
                }
            }
            out.set_outcome(outcome.container());
        }
        Some(Ok(MutationResponse::Deleted(outcome))) => {
            out.set_kind(5);
            out.set_phase(3);
            out.set_operation_id(outcome.operation_id());
            match outcome.result() {
                morrow_core::file_effect::DeleteResult::Deleted => out.set_effect(1),
                morrow_core::file_effect::DeleteResult::OsRejected { code } => {
                    out.set_effect(2);
                    out.set_os_code(code);
                }
            }
            out.set_outcome(outcome.container());
        }
        Some(Ok(MutationResponse::History {
            record,
            staged_bytes,
            durable_content,
        })) => {
            out.set_kind(6);
            if let Some(record) = record {
                record_reply(&record, out.reborrow())?;
            }
            out.set_staged_bytes(staged_bytes);
            out.set_durable_content(durable_content);
        }
        Some(Ok(MutationResponse::Reconciled { record, outcome })) => {
            // Read-only history, never a newly executed effect. Keep its phase
            // separate from the optional durable OS result.
            out.set_kind(10);
            if let Some(record) = record {
                record_reply(&record, out.reborrow())?;
            }
            use morrow_core::file_effect::{CreateResult, DeleteResult, ReplaceResult};
            use morrow_plugin_runtime::io_jobs::MutationOutcome;
            let result = match outcome.map(|value| *value) {
                Some(MutationOutcome::Created(value)) => {
                    out.set_outcome(value.container());
                    Some(match value.result() {
                        CreateResult::Created => None,
                        CreateResult::OsRejected { code } => Some(code),
                    })
                }
                Some(MutationOutcome::Deleted(value)) => {
                    out.set_outcome(value.container());
                    Some(match value.result() {
                        DeleteResult::Deleted => None,
                        DeleteResult::OsRejected { code } => Some(code),
                    })
                }
                Some(MutationOutcome::Replaced(value)) => {
                    out.set_outcome(value.container());
                    Some(match value.result() {
                        ReplaceResult::Replaced => None,
                        ReplaceResult::OsRejected { code } => Some(code),
                    })
                }
                None => None,
            };
            if let Some(code) = result {
                out.set_effect(if code.is_some() { 2 } else { 1 });
                if let Some(code) = code {
                    out.set_os_code(code);
                }
            }
        }
        Some(Ok(
            MutationResponse::GuestApproved(_) | MutationResponse::GuestExecutionAuthorized(_),
        )) => {
            // Trusted runtime-only approvals are not part of the private UI
            // protocol. Never encode them as an old action's success or leak
            // their opaque references through this unnegotiated wire surface.
            return Err("guest mutation approval is not exposed by this protocol".into());
        }
        Some(Ok(MutationResponse::Released)) => out.set_kind(7),
        Some(Ok(MutationResponse::PlanCancelled(record))) => {
            out.set_kind(8);
            record_reply(&record, out.reborrow())?;
        }
        Some(Err(failure)) => {
            let (layer, code) = failure_code(failure);
            out.set_kind(9);
            out.set_failure_layer(layer);
            out.set_failure_code(code);
        }
    }
    Ok(())
}
fn record_reply(
    record: &morrow_core::io_intent::Record,
    mut out: wire::mutation_result::Builder<'_>,
) -> Result<()> {
    use morrow_core::io_intent::Phase;
    out.set_record(record.container());
    out.set_operation_id(&record.command().operation_id);
    out.set_phase(match record.phase() {
        Phase::InvalidPhase => return Err("invalid mutation history phase".into()),
        Phase::Prepared => 1,
        Phase::OutcomeUnknown => 2,
        Phase::Observed => 3,
        Phase::CancelledBeforeDispatch => 4,
    });
    Ok(())
}
fn failure_code(failure: Failure) -> (u16, u16) {
    match failure {
        Failure::Delivery(error) => (
            1,
            match error {
                OwnerCommandError::Busy => 1,
                OwnerCommandError::Closed => 2,
                OwnerCommandError::Limit => 3,
                OwnerCommandError::Cancelled => 4,
                OwnerCommandError::Unknown => 5,
                OwnerCommandError::Consumed => 6,
            },
        ),
        Failure::Target(error) => (
            2,
            match error {
                TargetError::Admission(_) => 1,
                TargetError::CommittedButDeliveryDenied(_) => 2,
                TargetError::RestartRequired => 3,
                TargetError::Busy => 4,
                TargetError::OutcomeUnknown => 5,
                TargetError::AlreadyDispatched => 6,
                TargetError::UnsupportedConditionalReplacement => 7,
                TargetError::CancelledBeforeDispatch => 8,
                TargetError::CommittedButDeliveryCancelled => 9,
                TargetError::InvalidSelection => 10,
                TargetError::Missing => 11,
                TargetError::Mismatch => 12,
                TargetError::Changed => 13,
                TargetError::Io(_) => 14,
                TargetError::Limit => 15,
                TargetError::Persistence(morrow_core::Error::CommitUnknown) => 17,
                TargetError::Persistence(morrow_core::Error::RevisionConflict) => 18,
                TargetError::Persistence(_) => 16,
            },
        ),
    }
}

#[cfg(test)]
mod reconciliation_projection_tests {
    use super::*;
    use morrow_core::{
        file_effect::{DeleteOutcome, DeleteResult},
        file_mutation::{MutationRequest, Target},
        io_intent::{ObservationSource, Record},
    };
    use morrow_plugin_runtime::io_jobs::MutationOutcome;

    #[test]
    fn historical_unknown_and_os_rejection_never_project_as_success() {
        let request = RequestRecord::new(MutationRequest {
            operation_id: "history-projection".into(),
            subject: "plugin.history".into(),
            package_sha256: [1; 32],
            approval_sha256: [2; 32],
            target: Target {
                reference: [3; 32],
                relative_path: None,
            },
            disposition: Disposition::Delete,
            expected_identity: Some([4; 32]),
            content_length: 0,
            content_sha256: None,
        })
        .unwrap();
        let unknown = Record::prepared(request.command().unwrap())
            .unwrap()
            .propose_dispatch_boundary()
            .unwrap();
        let observed = unknown
            .propose_observation([5; 32], ObservationSource::OriginalResponse)
            .unwrap();
        let rejected = DeleteOutcome::new(
            "history-projection",
            "plugin.history",
            request.command().unwrap().request_sha256,
            [3; 32],
            [4; 32],
            DeleteResult::OsRejected { code: 5 },
        )
        .unwrap();
        for (record, outcome, phase, effect, code) in [
            (Some(unknown), None, 2, 0, 0),
            (
                Some(observed),
                Some(Box::new(MutationOutcome::Deleted(rejected))),
                3,
                2,
                5,
            ),
            (None, None, 0, 0, 0),
        ] {
            let mut message = capnp::message::Builder::new_default();
            result_reply(
                Some(Ok(MutationResponse::Reconciled { record, outcome })),
                message.init_root::<wire::mutation_result::Builder>(),
                [0; 32],
            )
            .unwrap();
            let row = message
                .get_root_as_reader::<wire::mutation_result::Reader>()
                .unwrap();
            assert_eq!(row.get_kind(), 10);
            assert_eq!(row.get_phase(), phase);
            assert_eq!(row.get_effect(), effect);
            assert_eq!(row.get_os_code(), code);
            if effect == 2 {
                assert_eq!(
                    DeleteOutcome::decode(row.get_outcome().unwrap())
                        .unwrap()
                        .result(),
                    DeleteResult::OsRejected { code: 5 }
                );
            } else {
                assert!(row.get_outcome().unwrap().is_empty());
            }
        }
    }
}
