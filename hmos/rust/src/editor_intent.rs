//! Bounded development-unsealed durable original editor requests. Separate
//! Core metadata transactions retain publication pins and issue exact transport
//! plans. None of these phases prove that a business operation committed.
use crate::{
    Engine, Reply, Request, Result, draft_bridge, editor_business, editor_draft, err, hex, unhex,
};
use morrow_core::{
    content::{Attachment, CardRecord},
    content_change::ContentChange,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    transaction::{self, Lookup},
};
use morrow_editor_draft_model::{MAX_BODY_BYTES, intent_proto as proto, proto as draft_proto};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const PREFIX: &str = "morrow-host-editor-intent-";
const TYPE: &str = "org.morrow.host.editor-intent";
const TITLE: &str = "Editor business intent";
const MAX_ACTIVE: usize = 16;
const MAX_IDENTITIES: usize = 256;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const DRAFT_PREFIX: &str = "morrow-host-editor-draft-";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    pub intent_id: String,
    pub prepare_operation: String,
    pub generation: String,
    pub prepared_record_sha256: String,
    pub request_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Prepare {
    pub operation_id: String,
    pub request_json: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Issue {
    pub intent: Proof,
    pub expected_generation: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    pub intent_id: String,
    pub prepare_operation: String,
    pub part: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub after: String,
    pub limit: usize,
    pub card_id: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Close {
    pub request_json: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CloseLiteral {
    schema_version: u32,
    intent: Proof,
    expected_generation: String,
    operation_id: String,
    disposition: String,
}
#[derive(Debug, Serialize)]
pub struct View {
    pub proof: Proof,
    pub card_id: String,
    pub business_operation: String,
    pub expected_revision: String,
    pub phase: &'static str,
    pub current_generation: String,
    pub current_active: bool,
    pub repeated: bool,
    pub part: String,
    pub request_json: String,
    pub publication: Option<draft_bridge::View>,
    pub save_request_json: String,
    pub inspect_request_json: String,
    pub close_request_json: String,
    pub close_disposition: String,
    pub issue_operation: String,
}

fn digest(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}
fn identity(value: &str) -> Result<()> {
    morrow_core::runtime::Command::ReadSummary {
        request_id: value.into(),
        card_id: value.into(),
    }
    .validate()
    .map_err(err)
}
fn domain(prefix: &str, tag: &[u8], parts: &[&str]) -> String {
    let mut sha = Sha256::new();
    sha.update(tag);
    for part in parts {
        sha.update((part.len() as u64).to_le_bytes());
        sha.update(part.as_bytes());
    }
    format!("{prefix}{}", hex(&sha.finalize()))
}
fn key(card: &str, operation: &str) -> String {
    domain(
        PREFIX,
        b"morrow.hmos.editor-intent.v1\0",
        &[card, operation],
    )
}
fn issue_operation(slot: &proto::Slot) -> String {
    domain(
        "morrow-host-intent-issue-",
        b"morrow.hmos.editor-intent-issue.v1\0",
        &[
            &key(&slot.card_id, &slot.business_operation),
            &slot.prepare_operation,
        ],
    )
}
fn attachment(pin: &draft_proto::StoredAsset) -> Result<Attachment> {
    if pin.sha256.len() != 32 {
        return Err("EditorIntentPinDigest".into());
    }
    Ok(Attachment {
        id: pin.pin_id.clone(),
        display_name: pin.display_name.clone(),
        media_type: pin.media_type.clone(),
        byte_length: pin.byte_length,
        sha256: pin
            .sha256
            .as_slice()
            .try_into()
            .map_err(|_| "EditorIntentPinDigest")?,
    })
}
fn charge(slot: &proto::Slot) -> Result<u64> {
    let mut copy = slot.clone();
    copy.active_bytes = 0;
    let blobs = slot.assets.iter().try_fold(0_u64, |n, p| {
        n.checked_add(p.byte_length)
            .ok_or("EditorIntentBytesOverflow")
    })?;
    for _ in 0..12 {
        let n = (copy.encoded_len() as u64)
            .checked_add(blobs)
            .ok_or("EditorIntentBytesOverflow")?;
        if n > MAX_BYTES {
            return Err("EditorIntentBytesLimit".into());
        }
        if n == copy.active_bytes {
            return Ok(n);
        }
        copy.active_bytes = n;
    }
    Err("EditorIntentBytesAccounting".into())
}
fn phase(slot: &proto::Slot) -> Result<&'static str> {
    match slot.phase {
        0 => Ok("prepared"),
        1 => Ok("issued"),
        2 => Ok("closed"),
        _ => Err("EditorIntentPhase".into()),
    }
}
fn first_of(slot: &proto::Slot) -> Result<proto::Slot> {
    let mut first = slot.clone();
    first.generation = 1;
    first.phase = 0;
    first.active = true;
    first.active_bytes = 0;
    first.prepared_record_sha256.clear();
    first.save_request_json.clear();
    first.inspect_request_json.clear();
    first.close_request_json.clear();
    first.mutation_operation = first.prepare_operation.clone();
    first.active_bytes = charge(&first)?;
    Ok(first)
}
fn proof(first: &proto::Slot) -> Proof {
    Proof {
        intent_id: key(&first.card_id, &first.business_operation),
        prepare_operation: first.prepare_operation.clone(),
        generation: "1".into(),
        prepared_record_sha256: hex(&digest(&first.encode_to_vec())),
        request_sha256: hex(&first.request_sha256),
    }
}
// Schema 1 fixes this native transport encoder (serde JSON object keys sorted
// lexically; compact UTF-8). Consumers read and send the persisted literal.
fn plans(first: &proto::Slot) -> Result<(String, String)> {
    let mut save = serde_json::json!({"action":"editor_save","editor_save":{"request_json":first.request_json,"intent":proof(first)}});
    let mut inspect = serde_json::json!({"action":"editor_commit_inspect","editor_commit":{"request_json":first.request_json,"expected_revision":first.expected_revision.to_string()}});
    // Explicitly freeze schema 1 independently of serde_json preserve_order.
    save.sort_all_objects();
    inspect.sort_all_objects();
    let save = serde_json::to_string(&save).map_err(err)?;
    let inspect = serde_json::to_string(&inspect).map_err(err)?;
    if save.len() > crate::LIMIT || inspect.len() > crate::LIMIT {
        return Err("EditorIntentTransportBytesLimit".into());
    }
    Ok((save, inspect))
}
fn validate_body(raw: &[u8]) -> Result<proto::Slot> {
    if raw.len() > MAX_BODY_BYTES {
        return Err("EditorIntentBodyLimit".into());
    }
    let slot = proto::Slot::decode(raw).map_err(err)?;
    if slot.encode_to_vec() != raw
        || slot.schema_version != 1
        || slot.expected_revision == 0
        || slot.request_json.len() > crate::LIMIT
    {
        return Err("EditorIntentSchema".into());
    }
    let (card, op, pubproof) = editor_business::intent_identity(&slot.request_json)?;
    identity(&slot.prepare_operation)?;
    identity(&slot.mutation_operation)?;
    let p = slot
        .publication
        .as_ref()
        .ok_or("EditorIntentPublicationMissing")?;
    if card != slot.card_id
        || op != slot.business_operation
        || slot.prepare_operation == op
        || slot.prepare_operation == p.save_operation
        || slot.request_sha256 != digest(slot.request_json.as_bytes())
        || slot.command_sha256.len() != 32
        || slot.content_sha256.len() != 32
        || slot.publication_sha256.len() != 32
        || p.draft_id != pubproof.draft_id
        || p.generation != draft_bridge::number(&pubproof.generation)?
        || p.save_operation != pubproof.save_operation
        || hex(&p.request_sha256) != pubproof.request_sha256
        || slot.publication_sha256 != editor_business::intent_publication_digest(&card, &pubproof)?
    {
        return Err("EditorIntentIdentity".into());
    }
    if slot.assets.len() > 20 {
        return Err("EditorIntentPinLimit".into());
    }
    let mut ids = std::collections::HashSet::new();
    for (index, pin) in slot.assets.iter().enumerate() {
        let selected = pin
            .selection
            .as_ref()
            .ok_or("EditorIntentSelectionMissing")?;
        identity(&selected.asset_id)?;
        if !ids.insert(&selected.asset_id)
            || pin.pin_id != format!("intent-pin-{index}")
            || selected.aliases.len() > 8
        {
            return Err("EditorIntentPinIdentity".into());
        }
        attachment(pin)?;
    }
    let first = first_of(&slot)?;
    match slot.phase {
        0 if slot.generation == 1
            && slot.active
            && slot.prepared_record_sha256.is_empty()
            && slot.save_request_json.is_empty()
            && slot.inspect_request_json.is_empty()
            && slot.close_request_json.is_empty()
            && slot.mutation_operation == slot.prepare_operation => {}
        1 if slot.generation == 2
            && slot.active
            && slot.prepared_record_sha256 == digest(&first.encode_to_vec())
            && slot.close_request_json.is_empty()
            && slot.mutation_operation == issue_operation(&first) =>
        {
            let (save, inspect) = plans(&first)?;
            if slot.save_request_json != save || slot.inspect_request_json != inspect {
                return Err("EditorIntentTransportChanged".into());
            }
        }
        2 if slot.generation == 2
            && !slot.active
            && slot.prepared_record_sha256 == digest(&first.encode_to_vec())
            && slot.save_request_json.is_empty()
            && slot.inspect_request_json.is_empty() =>
        {
            let close: CloseLiteral = serde_json::from_str(&slot.close_request_json)
                .map_err(|_| "EditorIntentCloseSchema")?;
            if close.schema_version != 1
                || close.intent != proof(&first)
                || close.expected_generation != "1"
                || close.operation_id != slot.mutation_operation
                || close.disposition != "cancel_prepared"
            {
                return Err("EditorIntentCloseChanged".into());
            }
        }
        _ => return Err("EditorIntentPhaseShape".into()),
    }
    if (slot.active && slot.active_bytes != charge(&slot)?)
        || (!slot.active && slot.active_bytes != 0)
    {
        return Err("EditorIntentBytesChanged".into());
    }
    Ok(slot)
}
fn validate_card(card: &CardRecord) -> Result<proto::Slot> {
    let s = card.summary();
    let slot = validate_body(&card.body())?;
    let pins = if slot.active {
        slot.assets
            .iter()
            .map(attachment)
            .collect::<Result<Vec<_>>>()?
    } else {
        vec![]
    };
    if s.type_id != TYPE
        || s.format_version != 1
        || s.title != TITLE
        || s.revision != slot.generation
        || s.id != key(&slot.card_id, &slot.business_operation)
        || card.attachments() != pins
    {
        return Err("EditorIntentCardChanged".into());
    }
    Ok(slot)
}
pub fn is_journal(card: &CardRecord) -> bool {
    let s = card.summary();
    s.type_id == TYPE || s.id.starts_with(PREFIX)
}
pub fn validate_journal(card: &CardRecord) -> Result<()> {
    validate_card(card).map(|_| ())
}
fn current(host: &HostRuntime, id: &str) -> Result<Option<proto::Slot>> {
    host.store_local()
        .card(id)
        .map_err(err)?
        .as_ref()
        .map(validate_card)
        .transpose()
}
fn history(host: &HostRuntime, id: &str, op: &str) -> Result<Option<proto::Slot>> {
    if matches!(host.store_local().lookup(op).map_err(err)?, Lookup::Absent) {
        return Ok(None);
    }
    let (commit, receipt) = host
        .store_local()
        .operation_commit(id, op)
        .map_err(err)?
        .ok_or("EditorIntentOperationOwner")?;
    let command = transaction::decode_command(&commit.command).map_err(err)?;
    let slot = match command.action {
        Some(transaction::proto::command::Action::CreateCard(raw)) => {
            let card = CardRecord::decode(&raw).map_err(err)?;
            if card.summary().id != id {
                return Err("EditorIntentHistoryIdentity".into());
            }
            validate_card(&card)?
        }
        Some(transaction::proto::command::Action::SetContent(change)) => {
            let slot = validate_body(&change.body)?;
            let expected = if slot.active {
                slot.assets
                    .iter()
                    .map(attachment)
                    .collect::<Result<Vec<_>>>()?
            } else {
                vec![]
            };
            let actual = change
                .attachments
                .as_ref()
                .ok_or("EditorIntentHistoryPins")?;
            if change.card_id != id
                || change.title != TITLE
                || !change.preview_text.is_empty()
                || change.expected_revision.checked_add(1) != Some(receipt.revision)
                || actual.items.len() != expected.len()
                || actual.items.iter().zip(&expected).any(|(a, b)| {
                    a.id != b.id
                        || a.display_name != b.display_name
                        || a.media_type != b.media_type
                        || a.byte_length != b.byte_length
                        || a.sha256 != b.sha256
                })
            {
                return Err("EditorIntentHistoryCommand".into());
            }
            slot
        }
        _ => return Err("EditorIntentHistoryCommand".into()),
    };
    if key(&slot.card_id, &slot.business_operation) != id
        || slot.generation != receipt.revision
        || slot.mutation_operation != op
    {
        return Err("EditorIntentHistoryIdentity".into());
    }
    Ok(Some(slot))
}
fn all(host: &HostRuntime) -> Result<Vec<proto::Slot>> {
    let mut after = PREFIX.to_string();
    let mut values = vec![];
    loop {
        let page = host.store_local().card_ids_local(&after, 32).map_err(err)?;
        if page.is_empty() {
            break;
        }
        let count = page.len();
        for id in page {
            if !id.starts_with(PREFIX) {
                return Ok(values);
            }
            let record = current(host, &id)?.ok_or("EditorIntentMissing")?;
            values.push(record);
            if values.len() > MAX_IDENTITIES {
                return Err("EditorIntentIdentityLimit".into());
            }
            after = id;
        }
        if count < 32 {
            break;
        }
    }
    Ok(values)
}
/// Both journal producers call this after computing their exact candidate
/// charge. Old exact-operation retries bypass it because they add no identity.
pub(crate) fn check_capacity(
    host: &HostRuntime,
    replacing: &str,
    candidate: u64,
    new_identity: bool,
) -> Result<()> {
    let intents = all(host)?;
    let mut identities = intents.len();
    let mut active = usize::from(candidate > 0);
    let mut bytes = candidate;
    for slot in intents {
        if key(&slot.card_id, &slot.business_operation) != replacing && slot.active {
            active += 1;
            bytes = bytes
                .checked_add(slot.active_bytes)
                .ok_or("EditorIntentBytesOverflow")?;
        }
    }
    let mut after = DRAFT_PREFIX.to_string();
    loop {
        let page = host.store_local().card_ids_local(&after, 32).map_err(err)?;
        if page.is_empty() {
            break;
        }
        let count = page.len();
        for id in page {
            if !id.starts_with(DRAFT_PREFIX) {
                break;
            }
            let card = host
                .store_local()
                .card(&id)
                .map_err(err)?
                .ok_or("EditorIntentDraftMissing")?;
            editor_draft::validate_journal(&card)?;
            let slot = draft_proto::Slot::decode(card.body().as_slice()).map_err(err)?;
            identities += 1;
            if id != replacing && slot.active {
                active += 1;
                bytes = bytes
                    .checked_add(slot.active_bytes)
                    .ok_or("EditorIntentBytesOverflow")?;
            }
            after = id;
        }
        if count < 32 {
            break;
        } // stop at the reserved range boundary
        let next = host.store_local().card_ids_local(&after, 1).map_err(err)?;
        if next.first().is_none_or(|id| !id.starts_with(DRAFT_PREFIX)) {
            break;
        }
    }
    if identities + usize::from(new_identity) > MAX_IDENTITIES {
        return Err("EditorIntentCombinedIdentityLimit".into());
    }
    if active > MAX_ACTIVE || bytes > MAX_BYTES {
        return Err("EditorIntentCombinedActiveLimit".into());
    }
    Ok(())
}
fn immutable(host: &HostRuntime, proofvalue: &Proof) -> Result<(proto::Slot, proto::Slot)> {
    identity(&proofvalue.intent_id)?;
    identity(&proofvalue.prepare_operation)?;
    let first = history(host, &proofvalue.intent_id, &proofvalue.prepare_operation)?
        .ok_or("EditorIntentPrepareMissing")?;
    if first.phase != 0 || first.generation != 1 || proofvalue != &proof(&first) {
        return Err("EditorIntentProofMismatch".into());
    }
    let live = current(host, &proofvalue.intent_id)?.ok_or("EditorIntentCurrentMissing")?;
    if first_of(&live)? != first {
        return Err("EditorIntentCurrentChanged".into());
    }
    Ok((first, live))
}
fn publication_record(engine: &Engine, first: &proto::Slot) -> Result<editor_draft::DraftRecord> {
    let p = first
        .publication
        .as_ref()
        .ok_or("EditorIntentPublicationMissing")?;
    let record =
        editor_draft::read_history(&engine.host, &first.card_id, &p.draft_id, &p.save_operation)?;
    let request = record
        .slot
        .request
        .as_ref()
        .ok_or("EditorIntentPublicationMissing")?;
    let mut pins = record.slot.assets.clone();
    for (index, pin) in pins.iter_mut().enumerate() {
        pin.pin_id = format!("intent-pin-{index}");
    }
    if !record.slot.active
        || record.slot.generation != p.generation
        || editor_draft::request_sha256(request) != hex(&p.request_sha256)
        || pins != first.assets
    {
        return Err("EditorIntentPublicationChanged".into());
    }
    Ok(record)
}
fn make_view(
    engine: &Engine,
    first: &proto::Slot,
    live: &proto::Slot,
    part: &str,
    repeated: bool,
) -> Result<View> {
    let mut view = View {
        proof: proof(first),
        card_id: first.card_id.clone(),
        business_operation: first.business_operation.clone(),
        expected_revision: first.expected_revision.to_string(),
        phase: phase(live)?,
        current_generation: live.generation.to_string(),
        current_active: live.active,
        repeated,
        part: part.into(),
        request_json: String::new(),
        publication: None,
        save_request_json: String::new(),
        inspect_request_json: String::new(),
        close_request_json: String::new(),
        close_disposition: if live.phase == 2 {
            "cancel_prepared".into()
        } else {
            String::new()
        },
        issue_operation: issue_operation(first),
    };
    match part {
        "summary" => {}
        "submission" => view.request_json = first.request_json.clone(),
        "publication" => {
            view.publication = Some(draft_bridge::View::from_record(publication_record(
                engine, first,
            )?)?)
        }
        "save" => view.save_request_json = live.save_request_json.clone(),
        "inspect" => view.inspect_request_json = live.inspect_request_json.clone(),
        "close" => view.close_request_json = live.close_request_json.clone(),
        _ => return Err("EditorIntentReadPart".into()),
    }
    Ok(view)
}
fn reply(views: Vec<View>, next: String, effect: &'static str, revision: String) -> Result<Reply> {
    let mut result = Reply::failure(String::new());
    result.ok = true;
    result.effect = effect;
    result.receipt_revision = revision;
    result.editor_intents = Some(views);
    result.intent_next_after = Some(next);
    if serde_json::to_vec(&result).map_err(err)?.len() > crate::LIMIT {
        return Err("EditorIntentReplyBytesLimit".into());
    }
    Ok(result)
}
fn preflight(engine: &Engine, first: &proto::Slot, live: &proto::Slot) -> Result<()> {
    for part in [
        "summary",
        "submission",
        "publication",
        "save",
        "inspect",
        "close",
    ] {
        reply(
            vec![make_view(engine, first, live, part, false)?],
            String::new(),
            "committed",
            live.generation.to_string(),
        )?;
    }
    Ok(())
}
fn verify_pins(host: &HostRuntime, slot: &proto::Slot) -> Result<()> {
    let id = key(&slot.card_id, &slot.business_operation);
    for pin in &slot.assets {
        let info = host
            .store_local()
            .export_attachment_local(&id, &pin.pin_id, &mut std::io::sink())
            .map_err(err)?;
        if info.byte_length != pin.byte_length || info.sha256.as_slice() != pin.sha256 {
            return Err("EditorIntentPinBytesChanged".into());
        }
    }
    Ok(())
}
fn write(engine: &mut Engine, slot: &proto::Slot, previous: u64) -> Result<()> {
    let body = slot.encode_to_vec();
    validate_body(&body)?;
    let id = key(&slot.card_id, &slot.business_operation);
    let pins = if slot.active {
        slot.assets
            .iter()
            .map(attachment)
            .collect::<Result<Vec<_>>>()?
    } else {
        vec![]
    };
    let start = engine.start;
    let clock = || {
        u64::try_from(start.elapsed().as_millis())
            .unwrap_or(u64::MAX - 1)
            .saturating_add(1)
    };
    let mut connection = engine.host.connect().map_err(err)?;
    let result = (|| -> Result<()> {
        let now = clock();
        engine
            .host
            .grant(
                &mut connection,
                if previous == 0 {
                    GrantKind::CreateContent
                } else {
                    GrantKind::EditContent
                },
                &id,
                now.saturating_add(30000),
                now,
            )
            .map_err(err)?;
        engine.effect = "unknown";
        let committed = if previous == 0 {
            let card =
                CardRecord::new_with_attachments(&id, TYPE, 1, TITLE, body, &pins).map_err(err)?;
            engine
                .host
                .create_content(&connection, &slot.mutation_operation, &card, clock)
        } else {
            engine.host.edit_content(
                &connection,
                &ContentChange {
                    operation_id: slot.mutation_operation.clone(),
                    card_id: id,
                    expected_revision: previous,
                    title: TITLE.into(),
                    body,
                    preview_text: String::new(),
                    attachments: Some(pins),
                },
                clock,
            )
        };
        engine.commit_result(committed)?;
        Ok(())
    })();
    let disconnected = engine.host.disconnect(&connection).map_err(err);
    result?;
    disconnected?;
    Ok(())
}
fn prepare(engine: &mut Engine, r: Prepare) -> Result<Reply> {
    identity(&r.operation_id)?;
    let (card, op, p) = editor_business::intent_identity(&r.request_json)?;
    let id = key(&card, &op);
    if let Some(first) = history(&engine.host, &id, &r.operation_id)? {
        engine.effect = "committed";
        if first.phase != 0 || first.generation != 1 || first.request_json != r.request_json {
            return Err("EditorIntentPrepareRetryChanged".into());
        }
        let live = current(&engine.host, &id)?.ok_or("EditorIntentCurrentMissing")?;
        if first_of(&live)? != first {
            return Err("EditorIntentCurrentChanged".into());
        }
        return reply(
            vec![make_view(engine, &first, &live, "summary", true)?],
            String::new(),
            engine.effect,
            live.generation.to_string(),
        );
    }
    if r.operation_id == op
        || r.operation_id == p.save_operation
        || current(&engine.host, &id)?.is_some()
    {
        return Err("EditorIntentIdentityAlreadyUsed".into());
    }
    let candidate = editor_business::prepare_intent(engine, &r.request_json)?;
    let record = editor_draft::read(&engine.host, &card, &p.draft_id)?
        .ok_or("EditorIntentPublicationMissing")?;
    let mut assets = record.slot.assets;
    for (index, pin) in assets.iter_mut().enumerate() {
        pin.pin_id = format!("intent-pin-{index}");
    }
    let mut first = proto::Slot {
        schema_version: 1,
        card_id: card,
        business_operation: op,
        prepare_operation: r.operation_id.clone(),
        request_sha256: digest(r.request_json.as_bytes()),
        request_json: r.request_json,
        publication: Some(proto::Publication {
            draft_id: candidate.publication.draft_id,
            generation: draft_bridge::number(&candidate.publication.generation)?,
            save_operation: candidate.publication.save_operation,
            request_sha256: unhex(&candidate.publication.request_sha256)?,
        }),
        publication_sha256: candidate.publication_sha256,
        expected_revision: candidate.expected_revision,
        command_sha256: candidate.command_sha256,
        content_sha256: candidate.content_sha256,
        assets,
        generation: 1,
        active: true,
        mutation_operation: r.operation_id,
        ..Default::default()
    };
    first.active_bytes = charge(&first)?;
    validate_body(&first.encode_to_vec())?;
    let mut issued = first.clone();
    issued.phase = 1;
    issued.generation = 2;
    issued.prepared_record_sha256 = digest(&first.encode_to_vec());
    issued.mutation_operation = issue_operation(&first);
    (issued.save_request_json, issued.inspect_request_json) = plans(&first)?;
    issued.active_bytes = charge(&issued)?;
    preflight(engine, &first, &first)?;
    preflight(engine, &first, &issued)?;
    check_capacity(&engine.host, &id, first.active_bytes, true)?;
    write(engine, &first, 0)?;
    reply(
        vec![make_view(engine, &first, &first, "summary", false)?],
        String::new(),
        engine.effect,
        "1".into(),
    )
}
fn issue(engine: &mut Engine, r: Issue) -> Result<Reply> {
    if r.expected_generation != "1" {
        return Err("EditorIntentExpectedGeneration".into());
    }
    let (first, live) = immutable(&engine.host, &r.intent)?;
    let op = issue_operation(&first);
    let mut issued = first.clone();
    issued.phase = 1;
    issued.generation = 2;
    issued.prepared_record_sha256 = digest(&first.encode_to_vec());
    issued.mutation_operation = op.clone();
    (issued.save_request_json, issued.inspect_request_json) = plans(&first)?;
    issued.active_bytes = charge(&issued)?;
    if let Some(old) = history(&engine.host, &r.intent.intent_id, &op)? {
        engine.effect = "committed";
        if old != issued {
            return Err("EditorIntentIssueRetryChanged".into());
        }
        return reply(
            vec![make_view(engine, &first, &live, "summary", true)?],
            String::new(),
            engine.effect,
            live.generation.to_string(),
        );
    }
    if live.phase != 0 || live.generation != 1 || !live.active {
        return Err("EditorIntentIssueConflict".into());
    }
    verify_pins(&engine.host, &live)?;
    preflight(engine, &first, &issued)?;
    check_capacity(
        &engine.host,
        &r.intent.intent_id,
        issued.active_bytes,
        false,
    )?;
    write(engine, &issued, 1)?;
    reply(
        vec![make_view(engine, &first, &issued, "summary", false)?],
        String::new(),
        engine.effect,
        "2".into(),
    )
}
fn close(engine: &mut Engine, r: Close) -> Result<Reply> {
    if r.request_json.len() > crate::LIMIT {
        return Err("EditorIntentCloseBytesLimit".into());
    }
    let c: CloseLiteral =
        serde_json::from_str(&r.request_json).map_err(|_| "EditorIntentCloseSchema")?;
    if c.schema_version != 1 || c.expected_generation != "1" || c.disposition != "cancel_prepared" {
        return Err("EditorIntentCloseDisposition".into());
    }
    identity(&c.operation_id)?;
    let (first, live) = immutable(&engine.host, &c.intent)?;
    if [
        &first.prepare_operation,
        &first.business_operation,
        &first.publication.as_ref().unwrap().save_operation,
        &issue_operation(&first),
    ]
    .contains(&&c.operation_id)
    {
        return Err("EditorIntentCloseOperation".into());
    }
    let mut closed = first.clone();
    closed.phase = 2;
    closed.generation = 2;
    closed.active = false;
    closed.active_bytes = 0;
    closed.prepared_record_sha256 = digest(&first.encode_to_vec());
    closed.close_request_json = r.request_json;
    closed.mutation_operation = c.operation_id.clone();
    if let Some(old) = history(&engine.host, &c.intent.intent_id, &c.operation_id)? {
        engine.effect = "committed";
        if old != closed {
            return Err("EditorIntentCloseRetryChanged".into());
        }
        return reply(
            vec![make_view(engine, &first, &live, "summary", true)?],
            String::new(),
            engine.effect,
            live.generation.to_string(),
        );
    }
    if live.phase != 0 || live.generation != 1 || !live.active {
        return Err("EditorIntentCancelIssuedOrClosed".into());
    }
    preflight(engine, &first, &closed)?;
    write(engine, &closed, 1)?;
    reply(
        vec![make_view(engine, &first, &closed, "summary", false)?],
        String::new(),
        engine.effect,
        "2".into(),
    )
}
pub(crate) fn envelopes_empty(r: &Request) -> bool {
    r.editor_intent.is_none()
        && r.editor_intent_issue.is_none()
        && r.editor_intent_ref.is_none()
        && r.editor_intent_query.is_none()
        && r.editor_intent_close.is_none()
}
pub(crate) fn reject_other_envelope(r: &Request) -> Result<()> {
    for (present, action) in [
        (r.editor_intent.is_some(), "editor_intent_prepare"),
        (r.editor_intent_issue.is_some(), "editor_intent_issue"),
        (r.editor_intent_ref.is_some(), "editor_intent_read"),
        (r.editor_intent_query.is_some(), "editor_intent_list"),
        (r.editor_intent_close.is_some(), "editor_intent_close"),
    ] {
        if present && r.action != action {
            return Err("EditorIntentEnvelopeActionMismatch".into());
        }
    }
    Ok(())
}
pub fn execute(engine: &mut Engine, mut r: Request) -> Result<Reply> {
    reject_other_envelope(&r)?;
    let encoded = match r.action.as_str() {
        "editor_intent_prepare" => serde_json::to_vec(
            &serde_json::json!({"action":r.action,"editor_intent":r.editor_intent}),
        ),
        "editor_intent_issue" => serde_json::to_vec(
            &serde_json::json!({"action":r.action,"editor_intent_issue":r.editor_intent_issue}),
        ),
        "editor_intent_read" => serde_json::to_vec(
            &serde_json::json!({"action":r.action,"editor_intent_ref":r.editor_intent_ref}),
        ),
        "editor_intent_list" => serde_json::to_vec(
            &serde_json::json!({"action":r.action,"editor_intent_query":r.editor_intent_query}),
        ),
        "editor_intent_close" => serde_json::to_vec(
            &serde_json::json!({"action":r.action,"editor_intent_close":r.editor_intent_close}),
        ),
        _ => return Err("EditorIntentAction".into()),
    }
    .map_err(err)?;
    if encoded.len() > crate::LIMIT || r.transport_json.len() > crate::LIMIT {
        return Err("EditorIntentEnvelopeBytesLimit".into());
    }
    let prepare_value = r.editor_intent.take();
    let issue_value = r.editor_intent_issue.take();
    let read = r.editor_intent_ref.take();
    let query = r.editor_intent_query.take();
    let close_value = r.editor_intent_close.take();
    if !editor_business::route_is_empty(&r) || r.editor_save.is_some() || r.editor_commit.is_some()
    {
        return Err("EditorIntentOuterFields".into());
    }
    match r.action.as_str() {
        "editor_intent_prepare" => {
            prepare(engine, prepare_value.ok_or("EditorIntentPrepareEnvelope")?)
        }
        "editor_intent_issue" => issue(engine, issue_value.ok_or("EditorIntentIssueEnvelope")?),
        "editor_intent_close" => close(engine, close_value.ok_or("EditorIntentCloseEnvelope")?),
        "editor_intent_read" => {
            let read = read.ok_or("EditorIntentReadEnvelope")?;
            let first = history(&engine.host, &read.intent_id, &read.prepare_operation)?
                .ok_or("EditorIntentPrepareMissing")?;
            if first.phase != 0 || first.generation != 1 {
                return Err("EditorIntentReadPrepareIdentity".into());
            }
            let live =
                current(&engine.host, &read.intent_id)?.ok_or("EditorIntentCurrentMissing")?;
            if first_of(&live)? != first {
                return Err("EditorIntentCurrentChanged".into());
            }
            reply(
                vec![make_view(engine, &first, &live, &read.part, false)?],
                String::new(),
                "not_committed",
                String::new(),
            )
        }
        "editor_intent_list" => {
            let q = query.ok_or("EditorIntentListEnvelope")?;
            if q.limit == 0 || q.limit > 16 || !q.after.is_empty() && !q.after.starts_with(PREFIX) {
                return Err("EditorIntentListBounds".into());
            }
            if !q.card_id.is_empty() {
                identity(&q.card_id)?;
            }
            let mut items = all(&engine.host)?.into_iter().filter(|s| {
                key(&s.card_id, &s.business_operation) > q.after
                    && (q.card_id.is_empty() || s.card_id == q.card_id)
            });
            let mut views = vec![];
            for slot in items.by_ref().take(q.limit) {
                let first = first_of(&slot)?;
                let (actual, _) = immutable(&engine.host, &proof(&first))?;
                views.push(make_view(engine, &actual, &slot, "summary", false)?);
            }
            let next = if items.next().is_some() {
                views.last().unwrap().proof.intent_id.clone()
            } else {
                String::new()
            };
            reply(views, next, "not_committed", String::new())
        }
        _ => Err("EditorIntentAction".into()),
    }
}
/// New write authority exists only in the independently retained exact issued
/// journal. Ordinary history never becomes a first-save permission.
pub(crate) fn save_authority(
    engine: &Engine,
    wire: &str,
    p: Option<&Proof>,
    outer: &str,
) -> Result<Option<editor_draft::PublishedAssets>> {
    let (card, op, _) = editor_business::intent_identity(wire)?;
    let id = key(&card, &op);
    let exists = current(&engine.host, &id)?.is_some();
    let Some(p) = p else {
        if exists {
            return Err("EditorIntentProofRequired".into());
        }
        return Ok(None);
    };
    let (first, live) = immutable(&engine.host, p)?;
    if p.intent_id != id
        || first.request_json != wire
        || live.phase != 1
        || !live.active
        || outer != live.save_request_json
    {
        return Err("EditorIntentIssuedWireRequired".into());
    }
    verify_pins(&engine.host, &live)?;
    publication_record(engine, &first)?;
    let published = editor_business::intent_publication(engine, wire)?;
    if published.attachments.len() != live.assets.len()
        || published
            .attachments
            .iter()
            .zip(&live.assets)
            .any(|(a, pin)| {
                pin.selection.as_ref().is_none_or(|s| s.asset_id != a.id)
                    || a.display_name != pin.display_name
                    || a.media_type != pin.media_type
                    || a.byte_length != pin.byte_length
                    || a.sha256.as_slice() != pin.sha256
            })
    {
        return Err("EditorIntentProjectionPins".into());
    }
    Ok(Some(published))
}
pub(crate) fn verify_projection(
    engine: &Engine,
    p: &Proof,
    command: &[u8],
    content: &[u8],
) -> Result<()> {
    let (first, _) = immutable(&engine.host, p)?;
    if digest(command) != first.command_sha256 || digest(content) != first.content_sha256 {
        return Err("EditorIntentProjectionChanged".into());
    }
    Ok(())
}
pub(crate) fn reject_legacy_business(engine: &Engine, r: &Request) -> Result<()> {
    if matches!(r.action.as_str(), "create" | "edit")
        && !r.id.is_empty()
        && !r.operation.is_empty()
        && current(&engine.host, &key(&r.id, &r.operation))?.is_some()
    {
        return Err("EditorIntentProofRequired".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
