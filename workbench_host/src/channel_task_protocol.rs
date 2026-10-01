//! Short controls on the original Windows workbench owner; no service forwarding.
use super::{text, wire};
use crate::{
    Result, Workbench,
    channel_tasks::{Prepare, SourceFrame, State},
};
use morrow_core::channel::{Budget, Kind};

pub(super) fn is_action(action: wire::Action) -> bool {
    matches!(
        action,
        wire::Action::ChannelPrepare
            | wire::Action::ChannelAppend
            | wire::Action::ChannelRun
            | wire::Action::ChannelStatus
            | wire::Action::ChannelClose
            | wire::Action::ChannelReadSent
            | wire::Action::PluginApprove
            | wire::Action::PluginRemove
    )
}
pub(super) fn budget_reply(value: Budget, mut out: wire::channel_budget::Builder<'_>) {
    out.set_max_channels(value.max_channels);
    out.set_max_frame_bytes(value.max_frame_bytes);
    out.set_max_bytes(value.max_bytes);
    out.set_max_messages(value.max_messages);
    out.set_max_requests(value.max_requests);
    out.set_max_duration_ms(value.max_duration_ms);
}
fn budget(value: wire::channel_budget::Reader<'_>) -> Budget {
    Budget {
        max_channels: value.get_max_channels(),
        max_frame_bytes: value.get_max_frame_bytes(),
        max_bytes: value.get_max_bytes(),
        max_messages: value.get_max_messages(),
        max_requests: value.get_max_requests(),
        max_duration_ms: value.get_max_duration_ms(),
    }
}
fn id(bytes: &[u8]) -> Result<[u8; 32]> {
    Ok(bytes.try_into().map_err(|_| "channel identity length")?)
}
fn state_reply(value: State, mut out: wire::channel_state::Builder<'_>) {
    out.set_key(&value.key);
    out.set_submission(&value.submission);
    out.set_directory(&value.directory);
    out.set_reference(&value.reference);
    out.set_source_epoch(&value.source_epoch);
    out.set_phase(value.phase);
    out.set_status(value.status as u16);
    out.set_last_acked(value.last_acked);
    out.set_accepted_sequence(value.accepted_sequence);
    out.set_observed_sequence(value.observed_sequence);
    out.set_cleanup_proof(value.cleanup_proof as u16);
    out.set_producer_outcome(value.producer_outcome as u16);
    out.set_task_state(value.task_state);
    out.set_task_error(&value.task_error);
    out.set_output_type(&value.output_type);
    out.set_output(&value.output);
    out.set_input_sha256(&value.input_sha256);
    out.set_uploaded_frames(value.uploaded_frames);
    out.set_uploaded_bytes(value.uploaded_bytes);
    out.set_source_frames(value.source_frames);
    out.set_source_bytes(value.source_bytes);
    out.set_resource_reclaimed(value.resource_reclaimed);
    out.set_close_requested(value.close_requested);
    out.set_observed_bytes(value.observed_bytes);
    out.set_observed_sha256(&value.observed_sha256);
    out.set_worker_joined(value.worker_joined);
    out.set_snapshot_pending(value.snapshot_pending);
}
pub(super) fn handle(
    host: &mut Workbench,
    request: wire::request::Reader<'_>,
    mut out: wire::response::Builder<'_>,
) -> Result<()> {
    let state = match request.get_action()? {
        wire::Action::ChannelPrepare => {
            let p = request.get_channel_prepare()?;
            host.prepare_channel(Prepare {
                submission: id(p.get_submission()?)?,
                package_id: text(p.get_package_id())?,
                package_digest: id(p.get_package_digest()?)?,
                registry_revision: p.get_registry_revision(),
                handler: text(p.get_handler())?,
                kind: match p.get_kind() {
                    1 => Kind::ByteStream,
                    2 => Kind::Events,
                    _ => return Err("channel kind".into()),
                },
                duplex: p.get_duplex(),
                budget: budget(p.get_budget()?),
                lifetime_ms: p.get_lifetime_ms(),
                frame_count: p.get_frame_count(),
                total_bytes: p.get_total_bytes(),
            })?
        }
        wire::Action::ChannelAppend => {
            let p = request.get_channel_append()?;
            host.append_channel(
                p.get_key()?,
                SourceFrame {
                    sequence: p.get_sequence(),
                    bytes: p.get_bytes()?.to_vec(),
                    cursor: p.get_cursor()?.to_vec(),
                },
            )?
        }
        wire::Action::ChannelRun => {
            let p = request.get_channel_run()?;
            host.run_channel(p.get_key()?, p.get_input()?.to_vec())?
        }
        wire::Action::ChannelStatus => host.channel_status(request.get_channel_key()?)?,
        wire::Action::ChannelClose => host.close_channel(request.get_channel_key()?)?,
        wire::Action::ChannelReadSent => {
            let key = request.get_channel_key()?;
            let sent = host.read_channel_sent(
                key,
                request.get_channel_sequence(),
                request.get_channel_offset(),
                request.get_channel_limit(),
            )?;
            let state = host.channel_status(key)?;
            let mut reply = out.reborrow().init_channel_sent();
            reply.set_present(sent.present);
            reply.set_sequence(sent.sequence);
            reply.set_bytes(&sent.bytes);
            reply.set_offset(sent.offset);
            reply.set_total_bytes(sent.total_bytes);
            reply.set_bytes_sha256(&sent.bytes_sha256);
            state
        }
        wire::Action::PluginApprove => {
            if request.get_limit() > 1 {
                return Err("invalid enable decision".into());
            }
            let values = request.get_approved_capabilities()?;
            if values.len() > 7 {
                return Err("capability decision budget".into());
            }
            let approved = values.iter().map(text).collect::<Result<Vec<_>>>()?;
            host.configure_external(
                &text(request.get_id())?,
                request.get_sha256()?,
                request.get_revision(),
                &approved,
                request.get_limit() == 1,
            )?;
            return Ok(());
        }
        wire::Action::PluginRemove => {
            host.remove_external(
                &text(request.get_id())?,
                request.get_sha256()?,
                request.get_revision(),
            )?;
            return Ok(());
        }
        _ => return Err("not a local channel control".into()),
    };
    state_reply(state, out.init_channel_state());
    Ok(())
}
