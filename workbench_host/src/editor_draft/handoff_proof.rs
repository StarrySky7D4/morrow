//! Historical business-commit proof for one draft handoff. This never reads
//! current card state, replays a guest, submits an edit, or revives a grant.
use super::proto;
use crate::{Result, WorkbenchState, captured_cards, projection, projection_v2};
use morrow_core::{
    content::CardRecord,
    transaction::{self, proto::command::Action as StoredAction},
};
use morrow_workbench_plugin::Action as PluginAction;

pub(super) fn committed_source(
    state: &WorkbenchState,
    request: &proto::WriteRequest,
    parent: &proto::Slot,
    link: &proto::ParentLink,
) -> Result<CardRecord> {
    let original = parent
        .request
        .as_ref()
        .ok_or("handoff parent request missing")?;
    if request.card_id != original.card_id
        || request.source_kind != 0
        || request.source_revision == 0
        || !request.predecessor_operation.is_empty()
        || !request.predecessor_sha256.is_empty()
        || link.committed_operation.is_empty()
        || link.committed_operation == request.operation_id
        || link.committed_sha256.len() != 32
    {
        return Err("handoff committed source identity".into());
    }

    let (commit, _) = state
        .host
        .store_local()
        .operation_commit(&request.card_id, &link.committed_operation)?
        .ok_or("handoff predecessor was not committed for this card")?;
    let evidence = state
        .host
        .store_local()
        .operation_evidence(&request.card_id, &link.committed_operation)?;
    if evidence.len() != 1 || evidence[0].digest().as_slice() != link.committed_sha256.as_slice() {
        return Err("handoff committed evidence changed".into());
    }
    let batch = evidence[0]
        .data()
        .batch
        .as_ref()
        .ok_or("handoff business operation has no capture batch")?;

    // A parent created while this same operation was pending already stores
    // its projected result in predecessor_card. For a later commit, that
    // projected card is instead the source of the next operation.
    let same_predecessor = !original.predecessor_operation.is_empty()
        && original.predecessor_operation == link.committed_operation;
    if same_predecessor
        && (original.predecessor_sha256 != link.committed_sha256
            || parent.predecessor_card.is_empty())
    {
        return Err("handoff parent predecessor evidence changed".into());
    }
    let effective = if original.source_kind == 1 {
        if !parent.source_card.is_empty() || !parent.predecessor_card.is_empty() {
            return Err("new-card parent invented an earlier source".into());
        }
        None
    } else if original.source_kind == 0 {
        let raw = if same_predecessor || parent.predecessor_card.is_empty() {
            &parent.source_card
        } else {
            &parent.predecessor_card
        };
        let source = CardRecord::decode(raw)?;
        if source.encode().as_slice() != raw.as_slice() || source.summary().id != request.card_id {
            return Err("handoff parent effective source changed".into());
        }
        Some(source)
    } else {
        return Err("unknown handoff parent source kind".into());
    };
    let expected_revision = effective
        .as_ref()
        .map_or(Some(1), |source| source.summary().revision.checked_add(1))
        .ok_or("handoff source revision exhausted")?;
    if request.source_revision != expected_revision {
        return Err("handoff committed revision changed".into());
    }

    let target = match batch.intent_type.as_str() {
        captured_cards::INTENT_TYPE => {
            let source = effective
                .as_ref()
                .ok_or("new-card handoff cannot use a V2 edit capture")?;
            let projected = captured_cards::verify_commit(&commit, &evidence[0])?;
            if projected.source_card() != source.encode().as_slice()
                || projected.operation_id() != link.committed_operation
            {
                return Err("handoff V2 editor source changed".into());
            }
            projected.card().clone()
        }
        projection_v2::INTENT_TYPE => {
            let facts = projection_v2::decode(&batch.intent)?;
            let content = projection::decode_intent_v1(&facts.content)?;
            let projected = projection::verify_commit(&commit, &evidence[0])?;
            let command = transaction::decode_command(&projected.command)?;
            if command.operation_id != link.committed_operation {
                return Err("handoff legacy operation changed".into());
            }
            match effective.as_ref() {
                None => {
                    if !content.prior.is_empty()
                        || projected.original_request.action != PluginAction::Create
                        || !matches!(command.action, Some(StoredAction::CreateCard(_)))
                    {
                        return Err("handoff new-card capture was not Create".into());
                    }
                }
                Some(source) => {
                    if content.prior != source.encode()
                        || projected.original_request.action != PluginAction::Edit
                        || !matches!(
                            command.action,
                            Some(StoredAction::SetContent(ref change))
                                if change.expected_revision == source.summary().revision
                        )
                    {
                        return Err("handoff legacy editor source changed".into());
                    }
                }
            }
            projected.card
        }
        _ => return Err("handoff operation is not an editor capture".into()),
    };
    let summary = target.summary();
    if summary.id != request.card_id
        || summary.revision != request.source_revision
        || (effective.is_none() && summary.format_version != 1)
    {
        return Err("handoff committed card identity changed".into());
    }
    if same_predecessor
        && (CardRecord::decode(&parent.predecessor_card)?
            .encode()
            .as_slice()
            != parent.predecessor_card.as_slice()
            || target.encode().as_slice() != parent.predecessor_card.as_slice())
    {
        return Err("handoff committed card differs from parent predecessor".into());
    }
    Ok(target)
}
