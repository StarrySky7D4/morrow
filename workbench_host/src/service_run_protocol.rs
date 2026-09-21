//! Private application scheduler frames. No listener or command is restored
//! from persisted desired state without a fresh explicit start.
use super::{io_state_reply, text, wire};
use crate::io_tasks::{
    service::{ServiceEndpointSelection, ServicePhase, ServiceSnapshot, ServiceStart},
    service_commands::{CommandKey, CommandSnapshot},
};
use crate::{Result, Workbench, io_tasks::TaskKey};
use morrow_plugin_runtime::{
    io_binding::ServiceRunBudget,
    io_jobs::{JobLimits, OwnerCommandError, OwnerCommandPoll},
};
use std::time::Duration;
use zeroize::Zeroizing;

pub(super) fn is_action(action: wire::Action) -> bool {
    matches!(
        action,
        wire::Action::ServiceRunStart
            | wire::Action::ServiceRunStatus
            | wire::Action::CommandSubmit
            | wire::Action::CommandStatus
            | wire::Action::CommandRead
            | wire::Action::CommandCancel
    )
}

fn network_outcome(value: Option<std::result::Result<(), morrow_network_node::Error>>) -> u16 {
    use morrow_network_node::Error::*;
    match value {
        None => 0,
        Some(Ok(())) => 1,
        Some(Err(Invalid)) => 2,
        Some(Err(Denied)) => 3,
        Some(Err(Limit)) => 4,
        Some(Err(Cancelled)) => 5,
        Some(Err(Timeout)) => 6,
        Some(Err(Transport)) => 7,
        Some(Err(Closed)) => 8,
    }
}

fn service_reply(value: ServiceSnapshot, mut out: wire::service_run_state::Builder<'_>) {
    io_state_reply(
        &value.task,
        Some(value.submission),
        out.reborrow().init_task(),
    );
    out.set_submission(&value.submission);
    out.set_phase(match value.phase {
        ServicePhase::Starting => 0,
        ServicePhase::Running => 1,
        ServicePhase::Stopping => 2,
        ServicePhase::Exited => 3,
    });
    if let Some(address) = value.address {
        out.set_address(address.to_string().as_str());
    }
    out.set_bind(network_outcome(value.bind));
    out.set_listener(network_outcome(value.listener));
    out.set_supervision(network_outcome(value.supervision));
}

fn command_reply(value: CommandSnapshot, mut out: wire::owner_command_state::Builder<'_>) {
    out.set_key(value.key.as_bytes());
    out.set_submission(&value.submission);
    out.set_delivery(match value.delivery {
        OwnerCommandPoll::Pending => 0,
        OwnerCommandPoll::Ready => 1,
        OwnerCommandPoll::Consumed => 2,
    });
    out.set_started(value.started);
    out.set_terminal(match value.terminal {
        None => 0,
        Some(OwnerCommandError::Busy) => 1,
        Some(OwnerCommandError::Closed) => 2,
        Some(OwnerCommandError::Limit) => 3,
        Some(OwnerCommandError::Cancelled) => 4,
        Some(OwnerCommandError::Unknown) => 5,
        Some(OwnerCommandError::Consumed) => 6,
    });
}

pub(super) fn handle(
    host: &mut Workbench,
    r: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    let action = r.get_action()?;
    if action == wire::Action::ServiceRunStart {
        let s = r.get_service_run()?;
        let selected = s.get_outbound()?;
        if selected.len() as usize > morrow_network_node::managed_http::MAX_SERVICE_ENDPOINTS {
            return Err("too many service outbound endpoints".into());
        }
        let outbound = selected
            .iter()
            .map(|endpoint| -> Result<_> {
                Ok(ServiceEndpointSelection {
                    reference: endpoint.get_reference()?.try_into()?,
                    revision: endpoint.get_revision(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let tls = if s.has_tls() {
            let selected = s.get_tls()?;
            Some(crate::service_tls::TlsSelection::from_expected(
                crate::service_protocol::tls_path(selected.get_certificate_path())?.into(),
                crate::service_protocol::tls_path(selected.get_private_key_path())?.into(),
                selected.get_certificate_sha256()?.try_into()?,
            )?)
        } else {
            None
        };
        let protected = if s.has_protected_tls() {
            let choice = s.get_protected_tls()?;
            Some(crate::tls_identity_control::ProtectedTlsChoice {
                reference: choice.get_reference()?.try_into()?,
                revision: choice.get_revision(),
                certificate_sha256: choice.get_certificate_sha256()?.try_into()?,
            })
        } else {
            None
        };
        let key = host.start_service_with_tls_choice(
            ServiceStart {
                submission: s.get_submission()?.try_into()?,
                config_id: text(s.get_config_id())?,
                config_digest: s.get_config_digest()?.try_into()?,
                config_revision: s.get_config_revision(),
                publication: s.get_publication()?.try_into()?,
                publication_revision: s.get_publication_revision(),
                package_id: text(s.get_package_id())?,
                package_digest: s.get_package_digest()?.try_into()?,
                registry_revision: s.get_registry_revision(),
                lifetime: Duration::from_millis(s.get_lifetime_ms().into()),
                budget: ServiceRunBudget {
                    max_jobs: s.get_max_jobs(),
                    max_bytes: s.get_max_bytes(),
                },
                limits: JobLimits::new(
                    s.get_max_calls(),
                    s.get_max_job_bytes(),
                    s.get_max_total_bytes(),
                )
                .map_err(|_| "invalid service job limits")?,
                network_limits: morrow_network_node::Limits {
                    max_request_bytes: s.get_max_request_bytes().try_into()?,
                    max_response_bytes: s.get_max_response_bytes().try_into()?,
                    max_header_bytes: s.get_max_header_bytes().try_into()?,
                    max_concurrent: s.get_max_concurrent().into(),
                    timeout: Duration::from_millis(s.get_timeout_ms().into()),
                },
            },
            &outbound,
            tls.as_ref(),
            protected.as_ref(),
        )?;
        service_reply(host.service_status(key)?, out.reborrow().init_service_run());
        return Ok(());
    }
    // A lost start receipt can recover the current identity, without starting
    // anything: an empty status key means observe the single current task.
    if action == wire::Action::ServiceRunStatus {
        let key = if r.get_io_key()?.is_empty() {
            host.io_status()
                .key
                .ok_or(crate::io_tasks::AccessError::StaleTask)?
        } else {
            TaskKey::from_bytes(r.get_io_key()?)?
        };
        service_reply(host.service_status(key)?, out.reborrow().init_service_run());
        return Ok(());
    }
    let task = TaskKey::from_bytes(r.get_io_key()?)?;
    let snapshot = match action {
        wire::Action::CommandSubmit => {
            // Reject recursive scheduling and malformed frames before taking a
            // queue reservation. The borrowed outer frame owns the input here.
            let input = r.get_payload()?;
            super::validate_command_frame(input)?;
            host.enqueue_service_command(
                task,
                r.get_command_submission()?.try_into()?,
                input.to_vec(),
            )?
        }
        wire::Action::CommandStatus => {
            if r.get_command_key()?.is_empty() {
                host.service_command_by_submission(task, r.get_command_submission()?.try_into()?)?
            } else {
                host.service_command_status(task, CommandKey::from_bytes(r.get_command_key()?)?)?
            }
        }
        wire::Action::CommandCancel => {
            host.cancel_service_command(task, CommandKey::from_bytes(r.get_command_key()?)?)?
        }
        wire::Action::CommandRead => {
            let (snapshot, payload) =
                host.read_service_command(task, CommandKey::from_bytes(r.get_command_key()?)?)?;
            if let Some(payload) = payload {
                // The wrapper and builder both have explicit wiping owners.
                let payload: Zeroizing<Vec<u8>> = payload;
                if payload.len() > 128 * 1024 {
                    return Err("owner command response budget".into());
                }
                out.set_payload(&payload);
            }
            snapshot
        }
        _ => return Err("not a service scheduler action".into()),
    };
    command_reply(snapshot, out.reborrow().init_owner_command());
    Ok(())
}
