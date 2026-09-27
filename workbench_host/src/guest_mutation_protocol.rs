//! Separate trusted private wire for original-owner guest mutation jobs.
//! The outer protocol never sends a guest lease or one-time effect permit.
use super::{io_state_reply, text, wire};
use crate::{
    Result, Workbench,
    io_tasks::{
        TaskKey,
        mutation::{
            StartRequest,
            guest::{
                GuestAction, GuestCommandKind, GuestMutationFailure, GuestMutationReply,
                GuestMutationSnapshot,
            },
        },
    },
};
use morrow_core::{
    file_mutation::Disposition, file_path::RelativeFilePath, mutation as guest_wire,
};
use morrow_plugin_runtime::{
    Fault,
    io_binding::MutationBudget,
    io_jobs::{JobError, MAX_MUTATION_CHUNK, Poll},
};
use sha2::Digest;
use zeroize::Zeroizing;

fn start_request(value: wire::mutation_start::Reader<'_>) -> Result<StartRequest> {
    let disposition = match value.get_disposition() {
        1 => Disposition::Create,
        3 => Disposition::Delete,
        _ => return Err("guest mutation only supports Create/Delete".into()),
    };
    let relative = text(value.get_relative_path())?;
    Ok(StartRequest {
        submission: value.get_submission()?.try_into()?,
        package_id: text(value.get_package_id())?,
        digest: value.get_package_digest()?.try_into()?,
        revision: value.get_registry_revision(),
        disposition,
        selected_path: text(value.get_selected_path())?.into(),
        relative_path: if relative.is_empty() {
            None
        } else {
            Some(RelativeFilePath::parse(&relative)?)
        },
        subject: text(value.get_subject())?,
        approval: value.get_approval_sha256()?.try_into()?,
        timeout_ms: value.get_timeout_ms(),
    })
}

fn command(value: wire::guest_mutation_command::Reader<'_>) -> Result<([u8; 32], GuestAction)> {
    let submission = value.get_submission()?.try_into()?;
    let kind = value.get_kind();
    let plan = value.get_plan_sha256()?;
    let offset = value.get_offset();
    let bytes = value.get_bytes()?;
    let operation = value.get_operation_id()?;
    let length = value.get_content_length();
    let hash = value.get_content_sha256()?;
    if (kind != 2 && kind != 5 && !plan.is_empty())
        || (kind != 3 && (offset != 0 || !bytes.is_empty()))
        || (kind != 1 && (!operation.is_empty() || length != 0 || !hash.is_empty()))
    {
        return Err("mixed guest mutation command fields".into());
    }
    let action = match kind {
        1 => {
            if operation.is_empty()
                || operation.len() > guest_wire::MAX_OPERATION_BYTES
                || (length > 0 && hash.is_empty())
                || (length == 0 && !hash.is_empty() && hash != sha2::Sha256::digest([]).as_slice())
                || length > guest_wire::MAX_CONTENT_BYTES
            {
                return Err("invalid guest draft content".into());
            }
            GuestAction::BuildPlan {
                operation_id: operation.to_str()?.to_owned(),
                content_length: length,
                content_sha256: if hash.is_empty() {
                    None
                } else {
                    Some(hash.try_into()?)
                },
            }
        }
        2 | 5 => {
            let digest: [u8; 32] = plan.try_into()?;
            if digest == [0; 32] {
                return Err("missing reviewed guest plan digest".into());
            }
            if kind == 2 {
                GuestAction::Prepare {
                    plan_sha256: digest,
                }
            } else {
                GuestAction::Execute {
                    plan_sha256: digest,
                }
            }
        }
        3 => {
            if bytes.is_empty()
                || bytes.len() > MAX_MUTATION_CHUNK
                || offset
                    .checked_add(bytes.len() as u64)
                    .is_none_or(|end| end > guest_wire::MAX_CONTENT_BYTES)
            {
                return Err("guest chunk limit".into());
            }
            GuestAction::Chunk {
                offset,
                bytes: Zeroizing::new(bytes.to_vec()),
            }
        }
        4 => GuestAction::CommitContent,
        6 => GuestAction::Query,
        7 => GuestAction::CancelPlan,
        8 => GuestAction::Release,
        9 => GuestAction::HostQuery,
        10 => GuestAction::HostCancelPlan,
        11 => GuestAction::HostRelease,
        _ => return Err("unknown guest mutation command".into()),
    };
    Ok((submission, action))
}

fn job_code(value: JobError) -> u16 {
    match value {
        JobError::InvalidOptions => 1,
        JobError::Busy => 2,
        JobError::Closed => 3,
        JobError::Unavailable => 4,
        JobError::Consumed => 5,
        JobError::ReadBound => 6,
        JobError::Limit => 7,
        JobError::Spawn => 8,
        JobError::Disconnect => 9,
    }
}
fn fault_code(value: Fault) -> u16 {
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

fn state_reply(state: GuestMutationSnapshot, mut out: wire::guest_mutation_state::Builder<'_>) {
    out.set_command(state.command);
    out.set_kind(match state.kind {
        GuestCommandKind::Select => 0,
        GuestCommandKind::BuildPlan => 1,
        GuestCommandKind::Prepare => 2,
        GuestCommandKind::Chunk => 3,
        GuestCommandKind::CommitContent => 4,
        GuestCommandKind::Execute => 5,
        GuestCommandKind::Query => 6,
        GuestCommandKind::CancelPlan => 7,
        GuestCommandKind::Release => 8,
        GuestCommandKind::HostQuery => 9,
        GuestCommandKind::HostCancelPlan => 10,
        GuestCommandKind::HostRelease => 11,
    });
    out.set_delivery(match state.delivery {
        Poll::Pending => 0,
        Poll::Ready => 1,
        Poll::Consumed => 2,
        Poll::Unavailable => 3,
    });
    out.set_selected(state.selected);
    if let Some(selection) = state.selection {
        out.set_reference(&selection.reference);
        if let Some(identity) = selection.expected_identity {
            out.set_expected_identity(&identity);
        }
    }
    if let Some(digest) = state.reviewed_plan_sha256 {
        out.set_reviewed_plan_sha256(&digest);
    }
    out.set_approval_delivered(state.approval_delivered);
    out.set_permit_delivered(state.permit_delivered);
    out.set_staged_bytes(state.staged_bytes);
    out.set_durable_content(state.durable_content);
    out.set_effect_attempted(state.effect_attempted);
    out.set_reconcile_required(state.reconcile_required);
    out.set_terminal(state.terminal);
}

fn result_reply(
    value: Option<std::result::Result<GuestMutationReply, GuestMutationFailure>>,
    mut out: wire::guest_mutation_result::Builder<'_>,
) -> Result<()> {
    match value {
        None => out.set_kind(0),
        Some(Ok(GuestMutationReply::Owner(value))) => {
            out.set_kind(1);
            super::mutation_task::result_reply(
                Some(Ok(value)),
                out.reborrow().init_owner(),
                [0; 32],
            )?;
        }
        Some(Ok(GuestMutationReply::Frame(value))) => {
            out.set_kind(2);
            out.set_frame(value.as_bytes());
        }
        Some(Err(GuestMutationFailure::Owner(value))) => {
            out.set_kind(1);
            super::mutation_task::result_reply(
                Some(Err(value)),
                out.reborrow().init_owner(),
                [0; 32],
            )?;
        }
        Some(Err(GuestMutationFailure::Job(value))) => {
            out.set_kind(3);
            out.set_failure_kind(1);
            out.set_failure_code(job_code(value));
        }
        Some(Err(GuestMutationFailure::Execution(value))) => {
            out.set_kind(3);
            out.set_failure_kind(2);
            out.set_failure_code(fault_code(value));
        }
        Some(Err(GuestMutationFailure::Protocol)) => {
            out.set_kind(3);
            out.set_failure_kind(3);
        }
        Some(Err(GuestMutationFailure::Cancelled)) => {
            out.set_kind(3);
            out.set_failure_kind(4);
        }
    }
    Ok(())
}

pub(super) fn handle(
    host: &mut Workbench,
    r: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    let action = r.get_action()?;
    let mut command_id = r.get_mutation_command_id();
    if r.has_mutation_start()
        || r.has_mutation_reconcile()
        || r.has_mutation_discover()
        || r.has_mutation_command()
    {
        return Err("native mutation fields on guest route".into());
    }
    let key = if action == wire::Action::GuestMutationStart {
        if !r.get_io_key()?.is_empty() || command_id != 0 || r.has_guest_mutation_command() {
            return Err("mixed guest start fields".into());
        }
        let start = r.get_guest_mutation_start()?;
        if !start.has_selection() {
            return Err("missing guest selection".into());
        }
        let selection = start_request(start.get_selection()?)?;
        let approved = if start.has_approved_budget() {
            let budget = start.get_approved_budget()?;
            Some(MutationBudget {
                max_job_bytes: budget.get_max_job_bytes(),
                max_bytes: budget.get_max_bytes(),
            })
        } else {
            None
        };
        command_id = 1;
        host.start_selected_guest_mutation(selection, approved)?
    } else {
        if r.has_guest_mutation_start() {
            return Err("guest start fields on command".into());
        }
        let key = TaskKey::from_bytes(r.get_io_key()?)?;
        match action {
            wire::Action::GuestMutationSubmit => {
                if command_id != 0 {
                    return Err("guest submit cannot supply command id".into());
                }
                let (submission, action) = command(r.get_guest_mutation_command()?)?;
                command_id = host.submit_guest_mutation(key, submission, action)?;
            }
            wire::Action::GuestMutationStatus
            | wire::Action::GuestMutationRead
            | wire::Action::GuestMutationCancelCommand => {
                if r.has_guest_mutation_command() {
                    return Err("unexpected guest command payload".into());
                }
                if action == wire::Action::GuestMutationStatus {
                    if command_id != 0 {
                        return Err("guest status uses current command".into());
                    }
                } else if command_id == 0 {
                    return Err("guest command id required".into());
                } else if action == wire::Action::GuestMutationRead {
                    result_reply(
                        host.read_guest_mutation_result(key, command_id)?,
                        out.reborrow().init_guest_mutation_result(),
                    )?;
                } else {
                    host.cancel_guest_mutation_command(key, command_id)?;
                }
            }
            _ => return Err("not a guest mutation scheduler action".into()),
        }
        key
    };
    state_reply(
        host.guest_mutation_status(key)?,
        out.reborrow().init_guest_mutation_state(),
    );
    out.set_mutation_command_id(command_id);
    io_state_reply(
        &host.io_status(),
        host.http_submission(),
        out.init_io_state(),
    );
    Ok(())
}
