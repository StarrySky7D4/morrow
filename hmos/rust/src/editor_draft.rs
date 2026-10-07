//! Development adaptation of the original host editor draft journal.
//! The original schema/model and journal transaction/history rules are reused.
//! Selected source assets and durable imports are pinned atomically with the
//! raw snapshot. Captured predecessors, parent handoff and S1 remain rejected.
//! The caller owns one serialized HostRuntime; protected maintenance is absent.
use crate::{Result, err};
use morrow_core::{
    content::{Attachment, CardRecord},
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
#[path = "editor_draft_fork.rs"]
mod fork;
pub use fork::{fork_with_effect_at, retire_fork_with_effect_at, validate_fork_request};
pub fn request_sha256(request: &proto::WriteRequest) -> String {
    crate::hex(&Sha256::digest(request.encode_to_vec()))
}
pub(crate) fn require_mutable(host: &HostRuntime, card: &str, draft: &str) -> Result<()> {
    if fork::successor(host, card, draft)?.is_some() { return Err("DraftForkParentFrozen".into()); }
    Ok(())
}

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
    if request
        .assets
        .iter()
        .any(|asset| !matches!(asset.origin, 0 | 2 | 3))
        || !request.predecessor_operation.is_empty()
        || !request.predecessor_sha256.is_empty()
    {
        return Err("DraftPhaseUnsupported: predecessor and parent assets".into());
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
fn asset_pin(index: usize) -> String {
    format!("draft-asset-{index}")
}
fn attachment(asset: &proto::StoredAsset) -> Result<Attachment> {
    Ok(Attachment {
        id: asset.pin_id.clone(),
        display_name: asset.display_name.clone(),
        media_type: asset.media_type.clone(),
        byte_length: asset.byte_length,
        sha256: asset
            .sha256
            .as_slice()
            .try_into()
            .map_err(|_| "draft asset digest")?,
    })
}
fn charge(slot: &proto::Slot) -> Result<u64> {
    let mut metadata = slot.clone();
    metadata.active_bytes = 0;
    let mut blobs = 0_u64;
    for asset in &slot.assets {
        blobs = blobs
            .checked_add(asset.byte_length)
            .ok_or("draft byte count overflow")?;
    }
    // Original monotonic varint fixed point, including every selected pin.
    for _ in 0..12 {
        let total = (metadata.encoded_len() as u64)
            .checked_add(blobs)
            .ok_or("draft byte count overflow")?;
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
    if let Some(link) = &slot.development_fork_link {
        if request.expected_generation == 0 { validate_fork_request(request, link)?; }
        else { phase_request(request)?; fork::validate_link(request, link)?; }
    } else { phase_request(request)?; }
    if slot.parent_link.is_some()
        || slot.retirement.is_some()
        || !slot.predecessor_card.is_empty()
        || slot.predecessor_evidence_bytes != 0
    {
        return Err("DraftPhaseUnsupported: lineage and captured recovery".into());
    }
    fork::validate_slot_shape(&slot)?;
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
    if slot.consumed_imports.len() > 20 || slot.consumed_imports.len() > slot.assets.len() {
        return Err("draft import consumption limit".into());
    }
    let mut consumed = std::collections::HashSet::new();
    for operation in &slot.consumed_imports {
        identity(&request.card_id, &request.draft_id, operation)?;
        if !consumed.insert(operation) {
            return Err("duplicate draft import consumption".into());
        }
    }
    if slot.assets.len() != request.assets.len() {
        return Err("draft selected assets changed".into());
    }
    for (index, (asset, selected)) in slot.assets.iter().zip(&request.assets).enumerate() {
        if asset.selection.as_ref() != Some(selected) || asset.pin_id != asset_pin(index) {
            return Err("draft asset identity changed".into());
        }
        attachment(asset)?;
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
    let expected = if slot.active {
        slot.assets
            .iter()
            .map(attachment)
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    if card.attachments() != expected {
        return Err("draft pin list changed".into());
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
            {
                return Err("draft history command mismatch".into());
            }
            let slot = decode_body(&change.body)?;
            let expected = if slot.active {
                slot.assets
                    .iter()
                    .map(attachment)
                    .collect::<Result<Vec<_>>>()?
            } else {
                Vec::new()
            };
            let actual = change
                .attachments
                .as_ref()
                .expect("checked")
                .items
                .as_slice();
            if actual.len() != expected.len()
                || actual.iter().zip(&expected).any(|(a, b)| {
                    a.id != b.id
                        || a.display_name != b.display_name
                        || a.media_type != b.media_type
                        || a.byte_length != b.byte_length
                        || a.sha256 != b.sha256
                })
            {
                return Err("draft historical pins mismatch".into());
            }
            slot
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
    let pins = if slot.active {
        slot.assets
            .iter()
            .map(attachment)
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
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
                CardRecord::new_with_attachments(id, TYPE, 1, TITLE, body, &pins).map_err(err)?;
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
                    attachments: Some(pins),
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
    save_with_effect_at(host, request, clock, unix_millis()?, effect)
}
fn unix_millis() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(err)?;
    i64::try_from(elapsed.as_millis()).map_err(err)
}
pub fn save_with_effect_at(
    host: &mut HostRuntime,
    request: &proto::WriteRequest,
    clock: impl FnMut() -> u64,
    blob_unix_ms: i64,
    effect: &mut &'static str,
) -> Result<DraftRecord> {
    save_internal(host, request, None, None, clock, blob_unix_ms, effect)
}
fn save_internal(
    host: &mut HostRuntime, request: &proto::WriteRequest,
    fork_link: Option<&proto::DevelopmentForkLink>, parent: Option<&proto::Slot>,
    mut clock: impl FnMut() -> u64, blob_unix_ms: i64, effect: &mut &'static str,
) -> Result<DraftRecord> {
    *effect = "not_committed";
    if let Some(link) = fork_link { validate_fork_request(request, link)?; } else { phase_request(request)?; }
    let id = key(&request.card_id, &request.draft_id);
    if let Some(slot) = draft_history(host, &id, &request.operation_id)? {
        if !slot.active || slot.request.as_ref() != Some(request) ||
            fork_link.is_some_and(|link| slot.development_fork_link.as_ref() != Some(link)) {
            return Err("draft retry payload changed".into());
        }
        // Exact immutable history proves this operation committed. A failure
        // reading its current journal must not downgrade that proven outcome.
        *effect = "committed";
        fork::verify_slot(host, &slot)?;
        let current = draft_card(host, &request.card_id, &request.draft_id)?
            .ok_or("draft current record missing")?;
        fork::verify_slot(host, &current)?;
        return Ok(DraftRecord {
            slot,
            current_generation: current.generation,
            current_active: current.active,
            repeated: true,
        });
    }
    let previous = draft_card(host, &request.card_id, &request.draft_id)?;
    require_mutable(host, &request.card_id, &request.draft_id)?;
    let generation = previous.as_ref().map_or(0, |slot| slot.generation);
    if request.expected_generation != generation {
        return Err("draft generation conflict".into());
    }
    let source_card = if let Some(slot) = &previous {
        if fork_link.is_some() { return Err("DraftForkChildIdentityExists".into()); }
        fork::verify_slot(host, slot)?;
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
    } else if let Some(parent) = parent {
        parent.source_card.clone()
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
    // Cleanup operations have their own effects. Until the main draft write is
    // issued, a staging cleanup failure must remain not_committed for this save.
    if previous.is_some() {
        crate::editor_draft_staging::reconcile(
            host,
            &request.card_id,
            &request.draft_id,
            &mut clock,
            blob_unix_ms,
            &mut "not_committed",
        )
        .map_err(err)?;
    }
    let source = if source_card.is_empty() {
        None
    } else {
        Some(CardRecord::decode(&source_card).map_err(err)?)
    };
    let source_assets = source
        .as_ref()
        .map_or_else(Vec::new, CardRecord::attachments);
    let mut stored = Vec::new();
    let mut consumed_imports = Vec::new();
    for (index, selection) in request.assets.iter().enumerate() {
        let prior_pin = previous.as_ref().and_then(|slot| {
            slot.assets.iter().find(|asset| {
                asset
                    .selection
                    .as_ref()
                    .is_some_and(|s| s.asset_id == selection.asset_id)
            })
        });
        let selected = match selection.origin {
            0 => {
                let item = source_assets
                    .iter()
                    .find(|a| a.id == selection.asset_id)
                    .cloned()
                    .ok_or("draft attachment does not belong to this source")?;
                if let Some(pin) = prior_pin.filter(|pin| {
                    pin.display_name == item.display_name
                        && pin.media_type == item.media_type
                        && pin.byte_length == item.byte_length
                        && pin.sha256 == item.sha256
                }) {
                    verify_pin(host, &id, pin, &mut std::io::sink())?;
                } else {
                    let live = host
                        .store_local()
                        .card(&request.card_id)
                        .map_err(err)?
                        .ok_or("draft source missing")?;
                    if live.encode() != source_card {
                        return Err("draft exact source changed".into());
                    }
                    let info = host
                        .store_local()
                        .export_attachment_local(
                            &request.card_id,
                            &selection.asset_id,
                            &mut std::io::sink(),
                        )
                        .map_err(err)?;
                    if info.sha256 != item.sha256 || info.byte_length != item.byte_length {
                        return Err("draft source attachment mismatch".into());
                    }
                }
                item
            }
            2 => {
                if let Some((item, operation)) =
                    crate::editor_draft_staging::resolve_durable_draft_import(
                        host,
                        &request.card_id,
                        &request.draft_id,
                        &selection.asset_id,
                        request.expected_generation,
                    )
                    .map_err(err)?
                {
                    consumed_imports.push(operation);
                    item
                } else {
                    let pin = prior_pin.ok_or("draft attachment does not belong to this source")?;
                    verify_pin(host, &id, pin, &mut std::io::sink())?;
                    attachment(pin)?
                }
            }
            3 => {
                let pin = prior_pin.ok_or("draft attachment does not belong to this source")?;
                verify_pin(host, &id, pin, &mut std::io::sink())?;
                attachment(pin)?
            }
            4 if fork_link.is_some() => {
                let parent = parent.ok_or("DraftForkParentMissing")?;
                let link = fork_link.expect("checked");
                let pin = fork::selected_parent_pin(parent, selection)?;
                verify_pin(host, &key(&request.card_id, &link.parent_draft_id), pin, &mut std::io::sink())?;
                attachment(pin)?
            }
            _ => return Err("DraftPhaseUnsupported: asset origin".into()),
        };
        stored.push(proto::StoredAsset {
            selection: Some(selection.clone()),
            pin_id: asset_pin(index),
            display_name: selected.display_name,
            media_type: selected.media_type,
            byte_length: selected.byte_length,
            sha256: selected.sha256.to_vec(),
        });
    }
    let mut slot = proto::Slot {
        schema_version: 1,
        request: Some(request.clone()),
        generation: generation
            .checked_add(1)
            .ok_or("draft generation exhausted")?,
        active: true,
        source_card,
        assets: stored,
        consumed_imports,
        development_fork_link: fork_link.cloned().or_else(|| previous.as_ref().and_then(|p| p.development_fork_link.clone())),
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
    crate::editor_intent::check_capacity(host, &id, slot.active_bytes, previous.is_none())?;
    write_draft_journal(
        host,
        &id,
        &request.operation_id,
        generation,
        &slot,
        &mut clock,
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
    let slot = draft_card(host, card, draft)?;
    if let Some(slot) = &slot { fork::verify_slot(host, slot)?; }
    Ok(slot.map(|slot| DraftRecord {
        current_generation: slot.generation,
        current_active: slot.active,
        slot,
        repeated: false,
    }))
}
/// Immutable save view for a separately verified host intent. This read does
/// not authorize ordinary writes, exports, or reactivation of an old draft.
pub(crate) fn read_history(host: &HostRuntime, card: &str, draft: &str, operation: &str) -> Result<DraftRecord> {
    let slot = draft_history(host, &key(card, draft), operation)?.ok_or("draft history missing")?;
    fork::verify_slot(host, &slot)?;
    let current = draft_card(host, card, draft)?.ok_or("draft current record missing")?;
    fork::verify_slot(host, &current)?;
    Ok(DraftRecord { slot, current_generation: current.generation, current_active: current.active, repeated: true })
}
pub fn list(host: &HostRuntime) -> Result<Vec<DraftRecord>> {
    let slots = all_draft_metadata(host)?;
    for slot in &slots { fork::verify_slot(host, slot)?; }
    Ok(slots
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
fn verify_pin(
    host: &HostRuntime,
    journal: &str,
    pin: &proto::StoredAsset,
    writer: &mut impl std::io::Write,
) -> Result<morrow_core::attachment::BlobInfo> {
    let info = host
        .store_local()
        .export_attachment_local(journal, &pin.pin_id, writer)
        .map_err(err)?;
    if info.byte_length != pin.byte_length || info.sha256.as_slice() != pin.sha256 {
        return Err("draft exported asset mismatch".into());
    }
    Ok(info)
}
/// Stream only an exact current draft pin. The writer may receive partial data
/// before a digest error; callers must publish their temporary output only on Ok.
pub fn export_asset_verified(
    host: &HostRuntime,
    card: &str,
    draft: &str,
    generation: u64,
    asset_id: &str,
    writer: &mut impl std::io::Write,
) -> Result<morrow_core::attachment::BlobInfo> {
    let slot = draft_card(host, card, draft)?.ok_or("draft missing")?;
    fork::verify_slot(host, &slot)?;
    if !slot.active || slot.generation != generation {
        return Err("draft export generation conflict".into());
    }
    let pin = slot
        .assets
        .iter()
        .find(|asset| {
            asset
                .selection
                .as_ref()
                .is_some_and(|s| s.asset_id == asset_id)
        })
        .ok_or("draft asset absent")?;
    verify_pin(host, &key(card, draft), pin, writer)
}
#[derive(Clone, Debug)]
pub struct PublishedAssets {
    pub assets: Vec<morrow_workbench_plugin::Asset>,
    pub attachments: Vec<Attachment>,
    /// The exact raw values whose journal grants this selected pin inventory.
    /// A business save must compare its intended text/category/stage to these.
    pub values: proto::Values,
}
fn kind(media_type: &str) -> &'static str {
    let bare = media_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if bare == "image/gif" {
        "gif"
    } else if bare.starts_with("image/") {
        "image"
    } else if bare.starts_with("video/") {
        "video"
    } else if bare.starts_with("audio/") {
        "audio"
    } else {
        "file"
    }
}
/// Read-only publication validation. All pins are streamed and verified before
/// any metadata is returned. The caller still performs the ordinary business
/// transaction with the same exact source; this confers no capture authority.
/// Do not compare the live business record here: an exact business operation
/// retry may arrive after that operation advanced it. The core transaction
/// checks immutable operation history first, then complete source CAS for any
/// new operation. A caller must not substitute a revision-only content edit.
pub fn publish_assets(
    host: &HostRuntime,
    card: &str,
    draft: &str,
    generation: u64,
    operation: &str,
    source: &[u8],
) -> Result<PublishedAssets> {
    identity(card, draft, operation)?;
    let slot = draft_card(host, card, draft)?.ok_or("draft missing")?;
    fork::verify_slot(host, &slot)?;
    let request = slot.request.as_ref().expect("validated request");
    if !slot.active || slot.generation != generation || request.operation_id != operation {
        return Err("draft publication generation or operation conflict".into());
    }
    let result = project_assets(&slot, source)?;
    for pin in &slot.assets {
        verify_pin(host, &key(card, draft), pin, &mut std::io::sink())?;
    }
    Ok(result)
}
/// Reconstruct metadata only for an already proven committed business retry.
/// The caller must first prove that SAME business operation belongs to this
/// card, retain effect=committed on later errors, and send the fully rebuilt
/// command through the core's immutable history payload comparison. This is
/// never a fallback authority for a new business mutation or a byte export.
pub fn published_assets_history(
    host: &HostRuntime,
    card: &str,
    draft: &str,
    generation: u64,
    operation: &str,
    source: &[u8],
) -> Result<PublishedAssets> {
    identity(card, draft, operation)?;
    let slot =
        draft_history(host, &key(card, draft), operation)?.ok_or("draft save history missing")?;
    fork::verify_slot(host, &slot)?;
    let request = slot.request.as_ref().expect("validated request");
    if !slot.active || slot.generation != generation || request.operation_id != operation {
        return Err("draft publication history generation or operation conflict".into());
    }
    project_assets(&slot, source)
}
fn project_assets(slot: &proto::Slot, source: &[u8]) -> Result<PublishedAssets> {
    let request = slot.request.as_ref().ok_or("draft request missing")?;
    if slot.source_card != source {
        return Err("draft publication exact source changed".into());
    }
    let mut baseline_assets: Vec<morrow_workbench_plugin::Asset> = Vec::new();
    let mut baseline_attachments = Vec::new();
    if request.source_kind == 1 {
        if !source.is_empty() {
            return Err("new-card draft must not invent a source card".into());
        }
    } else {
        let baseline = CardRecord::decode(source).map_err(err)?;
        validate_source(&baseline)?;
        let summary = baseline.summary();
        let properties =
            tasks_v2::decode(&summary.id, &summary.title, &baseline.body()).map_err(err)?;
        if baseline.encode() != source
            || summary.id != request.card_id
            || summary.revision != request.source_revision
            || properties.deleted
        {
            return Err("draft publication source binding conflict".into());
        }
        baseline_attachments = baseline.attachments();
        baseline_assets = properties
            .assets
            .iter()
            .map(|asset| morrow_workbench_plugin::Asset {
                id: asset.id.clone(),
                name: asset.name.clone(),
                kind: asset.kind.clone(),
                bytes: asset.bytes,
            })
            .collect();
    }
    let mut result = PublishedAssets {
        assets: Vec::new(),
        attachments: Vec::new(),
        values: request.values.clone().ok_or("draft values missing")?,
    };
    for pin in &slot.assets {
        let selected = pin.selection.as_ref().expect("validated selection");
        let mut outer = attachment(pin)?;
        outer.id = selected.asset_id.clone();
        let prior = if baseline_attachments.contains(&outer) {
            baseline_assets.iter().find(|asset| asset.id == outer.id)
        } else {
            None
        };
        result.assets.push(morrow_workbench_plugin::Asset {
            id: selected.asset_id.clone(),
            name: pin.display_name.clone(),
            kind: prior.map_or_else(|| kind(&pin.media_type).into(), |asset| asset.kind.clone()),
            bytes: pin.byte_length,
        });
        result.attachments.push(outer);
    }
    Ok(result)
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
    discard_with_effect_at(
        host,
        card,
        draft,
        expected,
        operation,
        clock,
        unix_millis()?,
        effect,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn discard_with_effect_at(
    host: &mut HostRuntime,
    card: &str,
    draft: &str,
    expected: u64,
    operation: &str,
    mut clock: impl FnMut() -> u64,
    blob_unix_ms: i64,
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
        fork::verify_slot(host, &slot)?;
        if slot.development_fork_retirement.is_some() {
            return Err("DraftForkRetirementRequiresDedicatedAction".into());
        }
        let current = draft_card(host, card, draft)?.ok_or("draft record missing")?;
        fork::verify_slot(host, &current)?;
        crate::editor_draft_staging::reconcile(
            host,
            card,
            draft,
            &mut clock,
            blob_unix_ms,
            &mut "not_committed",
        )
        .map_err(err)?;
        return Ok(DraftRecord {
            slot,
            current_generation: current.generation,
            current_active: current.active,
            repeated: true,
        });
    }
    let mut slot = draft_card(host, card, draft)?.ok_or("draft missing")?;
    fork::verify_slot(host, &slot)?;
    require_mutable(host, card, draft)?;
    if slot.development_fork_link.is_some() { fork::require_parent_retired(host, &slot)?; }
    if !slot.active || slot.generation != expected {
        return Err("draft discard generation conflict".into());
    }
    crate::editor_draft_staging::reconcile(
        host,
        card,
        draft,
        &mut clock,
        blob_unix_ms,
        &mut "not_committed",
    )
    .map_err(err)?;
    slot.generation = expected
        .checked_add(1)
        .ok_or("draft generation exhausted")?;
    slot.active = false;
    slot.active_bytes = 0;
    write_draft_journal(host, &id, operation, expected, &slot, &mut clock, effect)?;
    // An inactive main journal releases all its pins atomically. Remaining
    // unselected imports then retire through their original staging journal.
    // Failure preserves the proven committed main outcome for an exact retry.
    crate::editor_draft_staging::reconcile(
        host,
        card,
        draft,
        &mut clock,
        blob_unix_ms,
        &mut "not_committed",
    )
    .map_err(err)?;
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
