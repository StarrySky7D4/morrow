//! Development adaptation of the original host editor draft journal.
//! The original schema/model and journal transaction/history rules are reused.
//! This phase supports raw text snapshots only. Assets, captured predecessors,
//! parent handoff and S1 recovery are explicitly rejected, never simulated.
//! The caller owns one serialized HostRuntime; protected maintenance is absent.
use crate::{Result, err};
use morrow_core::{
    content::CardRecord,
    content_change::ContentChange,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    transaction::{self, Lookup},
};
pub use morrow_editor_draft_model::proto;
use morrow_editor_draft_model::{MAX_BODY_BYTES, validate_request};
use morrow_workbench_plugin::tasks_v2;
use prost::Message;
use sha2::{Digest, Sha256};

const PREFIX: &str = "morrow-host-editor-draft-";
const TYPE: &str = "org.morrow.host.editor-draft";
const TITLE: &str = "Editor draft";
const MAX_SLOTS: usize = 16;
const MAX_JOURNALS: usize = 256;
const MAX_ACTIVE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct DraftRecord {
    pub slot: proto::Slot,
    /// An exact historical response may precede the current journal generation.
    pub current_generation: u64,
    pub current_active: bool,
    pub repeated: bool,
}
fn identity(card: &str, draft: &str, operation: &str) -> Result<()> {
    for id in [card, draft] {
        morrow_core::runtime::Command::ReadSummary {
            request_id: operation.into(),
            card_id: id.into(),
        }
        .validate()
        .map_err(err)?;
    }
    Ok(())
}
fn key(card: &str, draft: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update((card.len() as u64).to_le_bytes());
    hasher.update(card.as_bytes());
    hasher.update(draft.as_bytes());
    format!(
        "{PREFIX}{}",
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
fn phase_request(request: &proto::WriteRequest) -> Result<()> {
    validate_request(request).map_err(err)?;
    if !request.assets.is_empty()
        || !request.predecessor_operation.is_empty()
        || !request.predecessor_sha256.is_empty()
    {
        return Err("DraftPhaseUnsupported: assets and captured predecessors".into());
    }
    Ok(())
}
fn validate_source(card: &CardRecord) -> Result<()> {
    // Existing HMOS development cards use type `idea`, unlike the production
    // `org.morrow.idea` adapter. This gate accepts only the actual format 2,
    // including complete original properties. It performs no migration.
    let summary = card.summary();
    if summary.type_id != "idea" || summary.format_version != 2 {
        return Err("DraftUnsupportedSource".into());
    }
    let properties = tasks_v2::decode(&summary.id, &summary.title, &card.body()).map_err(err)?;
    let attachments = card.attachments();
    if properties.assets.len() != attachments.len()
        || properties
            .assets
            .iter()
            .zip(&attachments)
            .any(|(asset, outer)| {
                asset.id != outer.id
                    || asset.name != outer.display_name
                    || asset.bytes != outer.byte_length
            })
    {
        return Err("V2 Card attachment metadata mismatch".into());
    }
    Ok(())
}
fn charge(slot: &proto::Slot) -> Result<u64> {
    let mut metadata = slot.clone();
    metadata.active_bytes = 0;
    // Original monotonic varint fixed point. Unsupported blob/pin fields are
    // rejected before this function, so the encoded slot is the entire charge.
    for _ in 0..12 {
        let total = metadata.encoded_len() as u64;
        if total > MAX_ACTIVE_BYTES {
            return Err("draft active byte limit".into());
        }
        if total == metadata.active_bytes {
            return Ok(total);
        }
        metadata.active_bytes = total;
    }
    Err("draft byte accounting did not converge".into())
}
fn decode_body(raw: &[u8]) -> Result<proto::Slot> {
    if raw.len() > MAX_BODY_BYTES {
        return Err("draft body limit".into());
    }
    let slot = proto::Slot::decode(raw).map_err(err)?;
    if slot.schema_version != 1 || slot.encode_to_vec() != raw {
        return Err("unsupported or noncanonical draft".into());
    }
    let request = slot.request.as_ref().ok_or("draft request missing")?;
    phase_request(request)?;
    if slot.parent_link.is_some()
        || slot.retirement.is_some()
        || !slot.assets.is_empty()
        || !slot.consumed_imports.is_empty()
        || !slot.predecessor_card.is_empty()
        || slot.predecessor_evidence_bytes != 0
    {
        return Err("DraftPhaseUnsupported: pins, lineage and captured recovery".into());
    }
    if slot.generation == 0
        || (slot.active && request.expected_generation.checked_add(1) != Some(slot.generation))
    {
        return Err("draft generation mismatch".into());
    }
    if request.source_kind == 0 {
        let source = CardRecord::decode(&slot.source_card).map_err(err)?;
        validate_source(&source)?;
        if source.encode() != slot.source_card
            || source.summary().id != request.card_id
            || source.summary().revision != request.source_revision
        {
            return Err("draft exact source changed".into());
        }
    } else if !slot.source_card.is_empty() {
        return Err("new-card draft must not invent a source card".into());
    }
    if (slot.active && slot.active_bytes != charge(&slot)?)
        || (!slot.active && slot.active_bytes != 0)
    {
        return Err("draft budget metadata changed".into());
    }
    Ok(slot)
}
fn decode_card(card: &CardRecord) -> Result<proto::Slot> {
    let summary = card.summary();
    if summary.type_id != TYPE || summary.format_version != 1 || summary.title != TITLE {
        return Err("unsupported editor draft journal".into());
    }
    let slot = decode_body(&card.body())?;
    let request = slot.request.as_ref().expect("validated request");
    if summary.id != key(&request.card_id, &request.draft_id) || summary.revision != slot.generation
    {
        return Err("draft journal identity changed".into());
    }
    if !card.attachments().is_empty() {
        return Err("DraftPhaseUnsupported: journal attachments".into());
    }
    Ok(slot)
}

/// Recognize reserved identity OR declared type, then validate before skipping
/// it in business enumeration. A malformed private record cannot become absent.
pub fn is_journal(card: &CardRecord) -> bool {
    let summary = card.summary();
    summary.type_id == TYPE || summary.id.starts_with(PREFIX)
}
pub fn validate_journal(card: &CardRecord) -> Result<()> {
    decode_card(card).map(|_| ())
}
fn draft_card(host: &HostRuntime, card: &str, draft: &str) -> Result<Option<proto::Slot>> {
    identity(card, draft, "draft-read")?;
    let Some(journal) = host.store_local().card(&key(card, draft)).map_err(err)? else {
        return Ok(None);
    };
    Ok(Some(decode_card(&journal)?))
}
fn draft_history(host: &HostRuntime, id: &str, operation: &str) -> Result<Option<proto::Slot>> {
    if matches!(
        host.store_local().lookup(operation).map_err(err)?,
        Lookup::Absent
    ) {
        return Ok(None);
    }
    let (commit, receipt) = host
        .store_local()
        .operation_commit(id, operation)
        .map_err(err)?
        .ok_or("draft operation belongs to another object")?;
    let command = transaction::decode_command(&commit.command).map_err(err)?;
    let slot = match command.action {
        Some(transaction::proto::command::Action::CreateCard(raw)) => {
            let card = CardRecord::decode(&raw).map_err(err)?;
            if card.summary().id != id {
                return Err("draft history identity mismatch".into());
            }
            decode_card(&card)?
        }
        Some(transaction::proto::command::Action::SetContent(change)) => {
            if change.card_id != id
                || change.title != TITLE
                || !change.preview_text.is_empty()
                || change.attachments.is_none()
                || change.expected_revision.checked_add(1) != Some(receipt.revision)
                || !change
                    .attachments
                    .as_ref()
                    .expect("checked")
                    .items
                    .is_empty()
            {
                return Err("draft history command mismatch".into());
            }
            decode_body(&change.body)?
        }
        _ => return Err("operation is not a draft mutation".into()),
    };
    let request = slot.request.as_ref().expect("validated request");
    if key(&request.card_id, &request.draft_id) != id || slot.generation != receipt.revision {
        return Err("draft historical generation mismatch".into());
    }
    Ok(Some(slot))
}
fn all_draft_metadata(host: &HostRuntime) -> Result<Vec<proto::Slot>> {
    let mut after = PREFIX.to_owned();
    let mut result = Vec::new();
    loop {
        let ids = host.store_local().card_ids_local(&after, 32).map_err(err)?;
        if ids.is_empty() {
            break;
        }
        let count = ids.len();
        for id in ids {
            if !id.starts_with(PREFIX) {
                return Ok(result);
            }
            let card = host
                .store_local()
                .card(&id)
                .map_err(err)?
                .ok_or("draft journal disappeared")?;
            result.push(decode_card(&card)?);
            if result.len() > MAX_JOURNALS {
                return Err("draft journal identity limit".into());
            }
            after = id;
        }
        if count < 32 {
            break;
        }
    }
    Ok(result)
}
fn write_draft_journal(
    host: &mut HostRuntime,
    id: &str,
    operation: &str,
    previous: u64,
    slot: &proto::Slot,
    mut clock: impl FnMut() -> u64,
    effect: &mut &'static str,
) -> Result<()> {
    let body = slot.encode_to_vec();
    decode_body(&body)?;
    // The production Storage::prepare_write seals protected audit state.
    // This explicit development HostRuntime has no protected maintenance API.
    let mut connection = host.connect().map_err(err)?;
    let result = (|| -> Result<()> {
        let tick = clock();
        host.grant(
            &mut connection,
            if previous == 0 {
                GrantKind::CreateContent
            } else {
                GrantKind::EditContent
            },
            id,
            tick.saturating_add(30_000),
            tick,
        )
        .map_err(err)?;
        *effect = "unknown";
        let committed = if previous == 0 {
            let card =
                CardRecord::new_with_attachments(id, TYPE, 1, TITLE, body, &[]).map_err(err)?;
            host.create_content(&connection, operation, &card, &mut clock)
        } else {
            host.edit_content(
                &connection,
                &ContentChange {
                    operation_id: operation.into(),
                    card_id: id.into(),
                    expected_revision: previous,
                    title: TITLE.into(),
                    body,
                    preview_text: String::new(),
                    attachments: Some(Vec::new()),
                },
                &mut clock,
            )
        };
        match committed {
            Ok(_) => *effect = "committed",
            Err(error) => {
                use morrow_core::Error;
                if matches!(
                    error,
                    Error::RevisionConflict
                        | Error::UnsupportedVersion
                        | Error::Invalid(_)
                        | Error::Limit
                ) {
                    *effect = "not_committed";
                }
                return Err(err(error));
            }
        }
        Ok(())
    })();
    let disconnected = host.disconnect(&connection).map_err(err);
    result?;
    disconnected?;
    Ok(())
}

/// Save the exact complete raw snapshot as a private journal transaction.
/// An error at this boundary may be after commit; callers retain the frozen
/// request and reconcile/retry its original operation rather than replace it.
pub fn save(
    host: &mut HostRuntime,
    request: &proto::WriteRequest,
    clock: impl FnMut() -> u64,
) -> Result<DraftRecord> {
    save_with_effect(host, request, clock, &mut "not_committed")
}
pub fn save_with_effect(
    host: &mut HostRuntime,
    request: &proto::WriteRequest,
    clock: impl FnMut() -> u64,
    effect: &mut &'static str,
) -> Result<DraftRecord> {
    *effect = "not_committed";
    phase_request(request)?;
    let id = key(&request.card_id, &request.draft_id);
    if let Some(slot) = draft_history(host, &id, &request.operation_id)? {
        if !slot.active || slot.request.as_ref() != Some(request) {
            return Err("draft retry payload changed".into());
        }
        // Exact immutable history proves this operation committed. A failure
        // reading its current journal must not downgrade that proven outcome.
        *effect = "committed";
        let current = draft_card(host, &request.card_id, &request.draft_id)?
            .ok_or("draft current record missing")?;
        return Ok(DraftRecord {
            slot,
            current_generation: current.generation,
            current_active: current.active,
            repeated: true,
        });
    }
    let previous = draft_card(host, &request.card_id, &request.draft_id)?;
    let generation = previous.as_ref().map_or(0, |slot| slot.generation);
    if request.expected_generation != generation {
        return Err("draft generation conflict".into());
    }
    let source_card = if let Some(slot) = &previous {
        if !slot.active {
            return Err("discarded draft identity cannot be reused".into());
        }
        let original = slot.request.as_ref().expect("validated request");
        if original.source_kind != request.source_kind
            || original.source_revision != request.source_revision
            || original.predecessor_operation != request.predecessor_operation
            || original.predecessor_sha256 != request.predecessor_sha256
        {
            return Err("draft baseline cannot silently change".into());
        }
        slot.source_card.clone()
    } else if request.source_kind == 1 {
        if host
            .store_local()
            .card(&request.card_id)
            .map_err(err)?
            .is_some()
        {
            return Err("new-card draft target already exists".into());
        }
        Vec::new()
    } else {
        let source = host
            .store_local()
            .card(&request.card_id)
            .map_err(err)?
            .ok_or("draft source missing")?;
        validate_source(&source)?;
        if source.summary().revision != request.source_revision {
            return Err("draft source revision conflict".into());
        }
        source.encode()
    };
    let mut slot = proto::Slot {
        schema_version: 1,
        request: Some(request.clone()),
        generation: generation
            .checked_add(1)
            .ok_or("draft generation exhausted")?,
        active: true,
        source_card,
        ..Default::default()
    };
    slot.active_bytes = charge(&slot)?;
    decode_body(&slot.encode_to_vec())?;
    let journals = all_draft_metadata(host)?;
    if previous.is_none() && journals.len() >= MAX_JOURNALS {
        return Err("draft journal identity capacity reached".into());
    }
    let others = journals
        .into_iter()
        .filter(|other| {
            other.active
                && other
                    .request
                    .as_ref()
                    .is_none_or(|r| key(&r.card_id, &r.draft_id) != id)
        })
        .collect::<Vec<_>>();
    let mut total = slot.active_bytes;
    for other in &others {
        total = total
            .checked_add(other.active_bytes)
            .ok_or("draft total overflow")?;
    }
    if others.len() >= MAX_SLOTS || total > MAX_ACTIVE_BYTES {
        return Err("draft active capacity reached".into());
    }
    write_draft_journal(
        host,
        &id,
        &request.operation_id,
        generation,
        &slot,
        clock,
        effect,
    )?;
    Ok(DraftRecord {
        current_generation: slot.generation,
        current_active: slot.active,
        slot,
        repeated: false,
    })
}
pub fn read(host: &HostRuntime, card: &str, draft: &str) -> Result<Option<DraftRecord>> {
    Ok(draft_card(host, card, draft)?.map(|slot| DraftRecord {
        current_generation: slot.generation,
        current_active: slot.active,
        slot,
        repeated: false,
    }))
}
pub fn list(host: &HostRuntime) -> Result<Vec<DraftRecord>> {
    Ok(all_draft_metadata(host)?
        .into_iter()
        .filter(|slot| slot.active)
        .map(|slot| DraftRecord {
            current_generation: slot.generation,
            current_active: true,
            slot,
            repeated: false,
        })
        .collect())
}
pub fn discard(
    host: &mut HostRuntime,
    card: &str,
    draft: &str,
    expected: u64,
    operation: &str,
    clock: impl FnMut() -> u64,
) -> Result<DraftRecord> {
    discard_with_effect(
        host,
        card,
        draft,
        expected,
        operation,
        clock,
        &mut "not_committed",
    )
}
pub fn discard_with_effect(
    host: &mut HostRuntime,
    card: &str,
    draft: &str,
    expected: u64,
    operation: &str,
    clock: impl FnMut() -> u64,
    effect: &mut &'static str,
) -> Result<DraftRecord> {
    *effect = "not_committed";
    identity(card, draft, operation)?;
    let id = key(card, draft);
    if let Some(slot) = draft_history(host, &id, operation)? {
        if slot.active || expected.checked_add(1) != Some(slot.generation) {
            return Err("draft discard retry changed".into());
        }
        *effect = "committed";
        let current = draft_card(host, card, draft)?.ok_or("draft record missing")?;
        return Ok(DraftRecord {
            slot,
            current_generation: current.generation,
            current_active: current.active,
            repeated: true,
        });
    }
    let mut slot = draft_card(host, card, draft)?.ok_or("draft missing")?;
    if !slot.active || slot.generation != expected {
        return Err("draft discard generation conflict".into());
    }
    slot.generation = expected
        .checked_add(1)
        .ok_or("draft generation exhausted")?;
    slot.active = false;
    slot.active_bytes = 0;
    write_draft_journal(host, &id, operation, expected, &slot, clock, effect)?;
    Ok(DraftRecord {
        current_generation: slot.generation,
        current_active: slot.active,
        slot,
        repeated: false,
    })
}

#[cfg(test)]
#[path = "editor_draft_tests.rs"]
mod tests;
