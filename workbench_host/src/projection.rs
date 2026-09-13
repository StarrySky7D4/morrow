//! Versioned, pure host content projection. This module never reads a Store or grants authority.
//! V1 fixes the host request shaping, persistence encoding, UTF-8 preview truncation and Card
//! mutation rules below. Changes to these rules OR dependency encoding behavior must retain the
//! old V1 implementation and introduce a new version; arbitrary future code is not a replay proof.
//! Unknown optional facts remain in the evidence's original intent bytes and do not change V1
//! semantics. An extension that changes projection semantics requires a new supported version.
use crate::Result;
use morrow_core::{
    content::{Attachment, CardRecord, MAX_RECORD_BYTES},
    content_change::ContentChange,
    task::Invocation,
    task_evidence::{self, Evidence},
    transaction,
};
use morrow_workbench_plugin::{Action, Idea, Request, codec, persistence};
use prost::{
    Message,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.projection.v1.rs"
    ));
}
pub const VERSION: u32 = 1;
pub const INTENT_TYPE: &str = "morrow.workbench.content-projection.v1";
const MAX_FACT_BYTES: usize = 12 * 1024 * 1024;
const MAX_ATTACHMENTS: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Undo {
    pub revision: u64,
    pub deadline: u64,
}
pub struct Projection {
    pub command: Vec<u8>,
    pub card: CardRecord,
    pub original_request: Request,
}
fn identity(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err("invalid projection identity".into());
    }
    Ok(())
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Facts,
    Attachment,
    Undo,
}
// Fixed two-level PB, bounded before owned prost allocation; prior is opaque here and is decoded
// by CardRecord's own bounded parser. Unknown byte extensions are not treated as submessages.
fn preflight(mut input: &[u8], kind: Kind, budget: &mut usize) -> Result<()> {
    let mut seen = 0u16;
    let mut attachments = 0usize;
    while !input.is_empty() {
        *budget = budget.checked_sub(1).ok_or("projection field limit")?;
        let (field, wire) = decode_key(&mut input)?;
        let known = field
            <= match kind {
                Kind::Facts => 7,
                Kind::Attachment => 5,
                Kind::Undo => 2,
            };
        if known {
            if kind == Kind::Facts && field == 5 {
                attachments += 1;
                if attachments > MAX_ATTACHMENTS {
                    return Err("projection attachment limit".into());
                }
            } else {
                let bit = 1u16 << field;
                if seen & bit != 0 {
                    return Err("duplicate projection field".into());
                }
                seen |= bit;
            }
            let delimited = match kind {
                Kind::Facts => matches!(field, 2..=5 | 7),
                Kind::Attachment => field != 4,
                Kind::Undo => false,
            };
            if wire
                != if delimited {
                    WireType::LengthDelimited
                } else {
                    WireType::Varint
                }
            {
                return Err("projection field type".into());
            }
        }
        if wire == WireType::LengthDelimited {
            let len = usize::try_from(decode_varint(&mut input)?)?;
            let max = match (kind, field) {
                (Kind::Facts, 2 | 3) | (Kind::Attachment, 1) => 256,
                (Kind::Facts, 4) => MAX_RECORD_BYTES,
                (Kind::Facts, 5) => 20 * 1024,
                (Kind::Facts, 7) => 128,
                (Kind::Attachment, 2) => 16 * 1024,
                (Kind::Attachment, 3) => 1024,
                (Kind::Attachment, 5) => 32,
                _ => MAX_FACT_BYTES,
            };
            if len > max || len > input.len() {
                return Err("projection field length".into());
            }
            let (value, tail) = input.split_at(len);
            input = tail;
            match (kind, field) {
                (Kind::Facts, 5) => preflight(value, Kind::Attachment, budget)?,
                (Kind::Facts, 7) => preflight(value, Kind::Undo, budget)?,
                _ => {}
            }
        } else if kind == Kind::Facts && field == 1 && wire == WireType::Varint {
            if decode_varint(&mut input)? > u64::from(u32::MAX) {
                return Err("projection version limit".into());
            }
        } else {
            if matches!(wire, WireType::StartGroup | WireType::EndGroup) {
                return Err("projection group".into());
            }
            skip_field(wire, field, &mut input, DecodeContext::default())?;
        }
    }
    Ok(())
}
fn attachment(value: &proto::Attachment) -> Result<Attachment> {
    let item = Attachment {
        id: value.id.clone(),
        display_name: value.display_name.clone(),
        media_type: value.media_type.clone(),
        byte_length: value.byte_length,
        sha256: value.sha256.as_slice().try_into()?,
    };
    item.validate()?;
    Ok(item)
}
fn validate_facts(value: &proto::HostProjection) -> Result<(Option<CardRecord>, Vec<Attachment>)> {
    if value.schema_version != VERSION {
        return Err("unsupported projection version".into());
    }
    identity(&value.operation_id)?;
    identity(&value.target_id)?;
    if value.prior.len() > MAX_RECORD_BYTES || value.attachments.len() > MAX_ATTACHMENTS {
        return Err("projection facts limit".into());
    }
    let prior = if value.prior.is_empty() {
        None
    } else {
        let prior = CardRecord::decode(&value.prior)?;
        let summary = prior.summary();
        if summary.id != value.target_id
            || summary.type_id != "org.morrow.idea"
            || summary.format_version != 1
        {
            return Err("projection prior identity".into());
        }
        persistence::decode(&summary.id, &summary.title, &prior.body())?;
        Some(prior)
    };
    if let Some(undo) = &value.undo {
        let prior = prior.as_ref().ok_or("undo without prior")?;
        if undo.revision != prior.summary().revision
            || value.observed_now == 0
            || value.observed_now >= undo.deadline
        {
            return Err("projection undo expired".into());
        }
    }
    let mut ids = BTreeSet::new();
    let mut attachments = Vec::with_capacity(value.attachments.len());
    for raw in &value.attachments {
        let item = attachment(raw)?;
        if !ids.insert(item.id.clone()) {
            return Err("duplicate projection attachment".into());
        }
        attachments.push(item);
    }
    Ok((prior, attachments))
}
/// Freeze selected host attachment metadata and the complete prior Card bytes. This is historical
/// data, not proof of platform selection, blob availability, clock authenticity, or authorization.
/// The attachments may be preflight candidates here; derive requires the final exact output order.
pub fn prepare_intent(
    operation: &str,
    target: &str,
    prior: Option<&CardRecord>,
    attachments: &[Attachment],
    observed_now: u64,
    undo: Option<Undo>,
) -> Result<Vec<u8>> {
    if attachments.len() > MAX_ATTACHMENTS {
        return Err("projection attachment limit".into());
    }
    let value = proto::HostProjection {
        schema_version: VERSION,
        operation_id: operation.into(),
        target_id: target.into(),
        prior: prior.map(CardRecord::encode).unwrap_or_default(),
        attachments: attachments
            .iter()
            .map(|item| proto::Attachment {
                id: item.id.clone(),
                display_name: item.display_name.clone(),
                media_type: item.media_type.clone(),
                byte_length: item.byte_length,
                sha256: item.sha256.to_vec(),
            })
            .collect(),
        observed_now,
        undo: undo.map(|value| proto::Undo {
            revision: value.revision,
            deadline: value.deadline,
        }),
    };
    if value.encoded_len() > MAX_FACT_BYTES || value.encoded_len() > task_evidence::MAX_INTENT_BYTES
    {
        return Err("projection intent limit".into());
    }
    validate_facts(&value)?;
    Ok(value.encode_to_vec())
}
/// Derive the exact original command and complete resulting Card without a Store or latest state.
/// Evidence integrity/guest replay/signatures are separate checks, not implied by this projection.
pub fn derive(evidence: &Evidence) -> Result<Projection> {
    if evidence
        .data()
        .batch
        .as_ref()
        .is_some_and(|b| b.intent_type == crate::projection_v2::INTENT_TYPE)
    {
        return crate::projection_v2::derive(evidence);
    }
    if evidence.data().schema_version != task_evidence::BATCH_VERSION {
        return Err("projection requires batch evidence".into());
    }
    let batch = evidence
        .data()
        .batch
        .as_ref()
        .ok_or("missing projection batch")?;
    if batch.intent_type != INTENT_TYPE
        || batch.intent.len() > MAX_FACT_BYTES
        || batch.observations.len() != 1
    {
        return Err("unsupported projection batch".into());
    }
    preflight(&batch.intent, Kind::Facts, &mut 8192)?;
    let facts = proto::HostProjection::decode(batch.intent.as_slice())?;
    match facts.schema_version {
        VERSION => derive_v1(facts, &batch.observations[0]),
        _ => Err("unsupported projection version".into()),
    }
}
// Frozen V1 dispatch target. Do not modify its output rules to match a future host version;
// add a new contract/version and retain this implementation and compatible persistence helpers.
pub(super) fn derive_v1(
    facts: proto::HostProjection,
    observation: &task_evidence::proto::Observation,
) -> Result<Projection> {
    let (prior, attachments) = validate_facts(&facts)?;
    let invocation = Invocation::decode(&observation.invocation)?;
    let transform = invocation
        .transform()
        .ok_or("projection task is not transform")?;
    if transform.handler != "workbench.command"
        || transform.input_type != "morrow.workbench.request.v1"
        || transform.output_type != "morrow.workbench.response.v1"
    {
        return Err("projection task route".into());
    }
    if observation.fault != 0
        || observation.exit_code != Some(0)
        || observation.observed_host_calls != 0
    {
        return Err("projection observation did not succeed".into());
    }
    let request = codec::decode_request(&transform.input)?;
    if request.now_ms != facts.observed_now
        || !request.ideas.is_empty()
        || request.section != "概览"
        || request.filter != "全部"
        || request.sort != "最近添加"
    {
        return Err("projection host request facts".into());
    }
    match (&prior, request.action) {
        (None, Action::Create) => {
            if request.current != Idea::default()
                || request.proposed.id != facts.target_id
                || request.now_ms != 0
                || facts.undo.is_some()
            {
                return Err("projection create facts".into());
            }
        }
        (Some(prior), action) if !matches!(action, Action::Create | Action::Query) => {
            let summary = prior.summary();
            let mut expected = persistence::decode(&summary.id, &summary.title, &prior.body())?;
            if action == Action::Edit {
                expected.description.clear();
                expected.hypothesis.clear();
                expected.conclusion.clear();
            }
            if request.current != expected || facts.observed_now == 0 {
                return Err("projection current differs from prior".into());
            }
            if action == Action::Restore {
                let undo = facts.undo.as_ref().ok_or("missing host undo observation")?;
                if undo.revision != summary.revision
                    || facts.observed_now >= undo.deadline
                    || !expected.deleted
                    || facts.observed_now < expected.deleted_at
                    || facts.observed_now - expected.deleted_at >= 8000
                {
                    return Err("projection restore window".into());
                }
            } else if facts.undo.is_some() {
                return Err("undo for non-restore action".into());
            }
        }
        _ => return Err("projection prior or action route".into()),
    }
    let output = invocation.verify_output(&observation.completion)?;
    let response = codec::decode_response(&output.bytes)?;
    let next = response.idea;
    next.validate()?;
    if next.id != facts.target_id || next.assets.len() != attachments.len() {
        return Err("projection output identity or attachments".into());
    }
    for (asset, selected) in next.assets.iter().zip(&attachments) {
        if asset.id != selected.id
            || asset.name != selected.display_name
            || asset.bytes != selected.byte_length
        {
            return Err("projection attachment selection mismatch".into());
        }
    }
    let (command, card) = match prior {
        None => {
            // V1 create intentionally leaves preview empty, matching the original host behavior.
            let card = CardRecord::new_with_attachments(
                &facts.target_id,
                "org.morrow.idea",
                1,
                &next.title,
                persistence::encode(&next, None)?,
                &attachments,
            )?;
            (
                transaction::create_command(&facts.operation_id, &card)?,
                card,
            )
        }
        Some(prior) => {
            let mut preview = next.description.clone();
            if preview.len() > 16384 {
                let mut n = 16384;
                while !preview.is_char_boundary(n) {
                    n -= 1;
                }
                preview.truncate(n);
            }
            let change = ContentChange {
                operation_id: facts.operation_id,
                card_id: facts.target_id,
                expected_revision: prior.summary().revision,
                title: next.title.clone(),
                body: persistence::encode(&next, Some(&prior.body()))?,
                preview_text: preview,
                attachments: Some(attachments),
            };
            let card = change.propose(&prior)?;
            (transaction::content_command(&change)?, card)
        }
    };
    Ok(Projection {
        command,
        card,
        original_request: request,
    })
}
/// Compare against a decoded original Commit. Pin/verify its original container separately;
/// this function never re-encodes the Commit for signatures and grants no replay/write authority.
pub fn verify_commit(
    commit: &transaction::proto::Commit,
    evidence: &Evidence,
) -> Result<Projection> {
    let projection = derive(evidence)?;
    let command = transaction::decode_command(&projection.command)?;
    let summary = projection.card.summary();
    let attachments = projection
        .card
        .attachments()
        .iter()
        .map(|a| a.sha256.to_vec())
        .collect::<Vec<_>>();
    if commit.schema_version != 2
        || commit.task_evidence_sha256 != vec![evidence.digest().to_vec()]
        || commit.command != projection.command
        || commit.command_sha256 != Sha256::digest(&projection.command).as_slice()
        || commit.operation_id != command.operation_id
        || commit.event_id != command.operation_id
        || commit.card_id != summary.id
        || commit.revision != summary.revision
        || commit.content_sha256 != Sha256::digest(projection.card.encode()).as_slice()
        || commit.attachment_sha256 != attachments
    {
        return Err("host projection differs from original commit".into());
    }
    Ok(projection)
}

pub(super) fn decode_intent_v1(raw: &[u8]) -> Result<proto::HostProjection> {
    if raw.len() > MAX_FACT_BYTES {
        return Err("projection intent limit".into());
    }
    preflight(raw, Kind::Facts, &mut 8192)?;
    let facts = proto::HostProjection::decode(raw)?;
    validate_facts(&facts)?;
    Ok(facts)
}
