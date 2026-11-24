//! Short controls on the original native workbench owner; no service forwarding.
//! A source grant still requires the platform's protected and supervised owner.
use super::{text, wire};
use crate::{
    Result, Workbench,
    channel_tasks::{ChangesSelection, Prepare, SourceFrame, State},
};
use morrow_core::channel::{Budget, Kind};

pub(super) fn is_action(action: wire::Action) -> bool {
    matches!(
        action,
        wire::Action::ChannelPrepare
            | wire::Action::ChannelPrepareWithCompatibilityBudget
            | wire::Action::ChannelPrepareChanges
            | wire::Action::ChannelChangesCheckpoint
            | wire::Action::ChannelAppend
            | wire::Action::ChannelRun
            | wire::Action::ChannelStatus
            | wire::Action::ChannelClose
            | wire::Action::ChannelReadSent
            | wire::Action::PluginApprove
            | wire::Action::PluginRemove
    )
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
fn reviewed_cards(
    selection: wire::changes_window_review::Reader<'_>,
) -> Result<(
    Vec<String>,
    Option<morrow_core::changes_metadata::ChangesCursor>,
)> {
    let list = selection.get_cards()?;
    if list.is_empty() || list.len() as usize > morrow_core::changes_metadata::MAX_CARDS {
        return Err("choose 1 to 32 cards".into());
    }
    let cards = list
        .iter()
        .map(|item| {
            let value = item?.to_str()?;
            if value.len() > 256 {
                return Err("invalid card identity".into());
            }
            Ok(value.to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    let bytes = selection.get_previous_cursor()?;
    let previous_cursor = if bytes.is_empty() {
        None
    } else {
        Some(morrow_plugin_runtime::channel::changes::ChangesCursor::decode(bytes)?)
    };
    Ok((cards, previous_cursor))
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
    out.set_changes_cursor(&value.changes_cursor);
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
    if request.get_action()? == wire::Action::ChannelChangesCheckpoint {
        use morrow_core::changes_metadata::BookmarkState;
        if !request.has_changes_checkpoint_query() {
            return Err("missing checkpoint selection".into());
        }
        let selection = request.get_changes_checkpoint_query()?;
        let (cards, bookmark) = reviewed_cards(selection)?;
        let progress = host.inspect_changes_checkpoint(
            &text(request.get_id())?,
            id(request.get_sha256()?)?,
            request.get_revision(),
            &cards,
            bookmark,
        )?;
        let mut reply = out.init_changes_checkpoint();
        reply.set_bookmark_state(match progress.bookmark {
            BookmarkState::NotSupplied => 0,
            BookmarkState::Current => 1,
            BookmarkState::Retained => 2,
            BookmarkState::Unavailable => 3,
        });
        if let Some(cursor) = progress.latest {
            reply.set_latest(&cursor.encode());
        }
        reply.set_pending(progress.pending);
        return Ok(());
    }
    let state = match request.get_action()? {
        action @ (wire::Action::ChannelPrepare
        | wire::Action::ChannelPrepareWithCompatibilityBudget
        | wire::Action::ChannelPrepareChanges) => {
            let p = request.get_channel_prepare()?;
            let prepared = Prepare {
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
            };
            if action == wire::Action::ChannelPrepareChanges {
                let selection = p.get_changes()?;
                let (cards, previous_cursor) = reviewed_cards(selection)?;
                host.prepare_changes_channel(
                    prepared,
                    ChangesSelection {
                        cards,
                        previous_cursor,
                        page_size: selection.get_page_size(),
                    },
                    selection.get_compatibility_budget(),
                )?
            } else if p.has_changes() {
                return Err("changes require explicit source approval".into());
            } else if action == wire::Action::ChannelPrepareWithCompatibilityBudget {
                host.prepare_channel_with_compatibility_budget(prepared)?
            } else {
                host.prepare_channel(prepared)?
            }
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
