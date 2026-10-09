//! Durable development business continuation. Intent plan, first child,
//! conditional parent retirement and intent close are separate Core writes.
//! Exact original literals are retained; metadata phases never prove business.
use crate::{
    Engine, Reply, Request, Result, draft_bridge, editor_business, editor_draft as draft,
    editor_intent as intent, err, hex, unhex,
};
use morrow_core::dispatch::HostRuntime;
use morrow_editor_draft_model::{intent_proto, proto};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub request_json: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DraftProof {
    pub draft_id: String,
    pub generation: String,
    pub save_operation: String,
    pub request_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CloseParent {
    pub proof: DraftProof,
    pub discard_operation: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Handoff {
    schema_version: u32,
    intent: intent::Proof,
    parent: DraftProof,
    child: draft_bridge::Write,
    plan_operation: String,
    retirement_operation: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Retirement {
    schema_version: u32,
    intent: intent::Proof,
    plan_operation: String,
    handoff_request_sha256: String,
    parent: DraftProof,
    child_draft_id: String,
    child_operation: String,
    child_request_sha256: String,
    operation_id: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct LinkView {
    schema_version: u32,
    parent: DraftProof,
    intent: intent::Proof,
    plan_operation: String,
    handoff_request_sha256: String,
    child_operation: String,
    committed_operation: String,
    committed_revision: String,
    command_sha256: String,
    content_sha256: String,
    request_sha256: String,
    publication_sha256: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct RetirementView {
    schema_version: u32,
    child_draft_id: String,
    operation_id: String,
    business_link: LinkView,
}
fn hash(raw: &[u8]) -> Vec<u8> {
    Sha256::digest(raw).to_vec()
}
fn sha(s: &str) -> Result<Vec<u8>> {
    let bytes = unhex(s)?;
    if bytes.len() != 32 || hex(&bytes) != s {
        return Err("BusinessHandoffDigest".into());
    }
    Ok(bytes)
}
fn proof_proto(p: &intent::Proof) -> Result<proto::DevelopmentBusinessIntentProof> {
    intent::identity(&p.intent_id)?;
    intent::identity(&p.prepare_operation)?;
    if p.generation != "1" {
        return Err("BusinessHandoffIntentGeneration".into());
    }
    Ok(proto::DevelopmentBusinessIntentProof {
        intent_id: p.intent_id.clone(),
        prepare_operation: p.prepare_operation.clone(),
        generation: 1,
        prepared_record_sha256: sha(&p.prepared_record_sha256)?,
        request_sha256: sha(&p.request_sha256)?,
    })
}
fn proof_view(p: &proto::DevelopmentBusinessIntentProof) -> intent::Proof {
    intent::Proof {
        intent_id: p.intent_id.clone(),
        prepare_operation: p.prepare_operation.clone(),
        generation: p.generation.to_string(),
        prepared_record_sha256: hex(&p.prepared_record_sha256),
        request_sha256: hex(&p.request_sha256),
    }
}
fn parent_proto(p: &DraftProof) -> Result<proto::DevelopmentBusinessDraftProof> {
    intent::identity(&p.draft_id)?;
    intent::identity(&p.save_operation)?;
    let n = draft_bridge::number(&p.generation)?;
    if n == 0 || n == u64::MAX {
        return Err("BusinessHandoffParentGeneration".into());
    }
    Ok(proto::DevelopmentBusinessDraftProof {
        draft_id: p.draft_id.clone(),
        generation: n,
        save_operation: p.save_operation.clone(),
        request_sha256: sha(&p.request_sha256)?,
    })
}
fn parent_view(p: &proto::DevelopmentBusinessDraftProof) -> DraftProof {
    DraftProof {
        draft_id: p.draft_id.clone(),
        generation: p.generation.to_string(),
        save_operation: p.save_operation.clone(),
        request_sha256: hex(&p.request_sha256),
    }
}
impl LinkView {
    pub(crate) fn from_proto(l: &proto::DevelopmentBusinessLink) -> Result<Self> {
        Ok(Self {
            schema_version: l.schema_version,
            parent: parent_view(l.parent.as_ref().ok_or("BusinessLinkParentMissing")?),
            intent: proof_view(l.intent.as_ref().ok_or("BusinessLinkIntentMissing")?),
            plan_operation: l.plan_operation.clone(),
            handoff_request_sha256: hex(&l.handoff_request_sha256),
            child_operation: l.child_operation.clone(),
            committed_operation: l.committed_operation.clone(),
            committed_revision: l.committed_revision.to_string(),
            command_sha256: hex(&l.command_sha256),
            content_sha256: hex(&l.content_sha256),
            request_sha256: hex(&l.request_sha256),
            publication_sha256: hex(&l.publication_sha256),
        })
    }
}
impl RetirementView {
    pub(crate) fn from_proto(r: &proto::DevelopmentBusinessRetirement) -> Result<Self> {
        Ok(Self {
            schema_version: r.schema_version,
            child_draft_id: r.child_draft_id.clone(),
            operation_id: r.operation_id.clone(),
            business_link: LinkView::from_proto(
                r.business_link
                    .as_ref()
                    .ok_or("BusinessRetirementLinkMissing")?,
            )?,
        })
    }
}
fn parse_handoff(wire: &str) -> Result<(Handoff, proto::WriteRequest)> {
    if wire.len() > crate::LIMIT {
        return Err("BusinessHandoffLiteralBytesLimit".into());
    }
    let h: Handoff = serde_json::from_str(wire).map_err(|_| "BusinessHandoffSchema")?;
    let child = h.child.clone().request()?;
    if h.schema_version != 1 || child.draft_id == h.parent.draft_id {
        return Err("BusinessHandoffIdentity".into());
    }
    validate_first(&child)?;
    parent_proto(&h.parent)?;
    proof_proto(&h.intent)?;
    intent::identity(&h.plan_operation)?;
    intent::identity(&h.retirement_operation)?;
    let ops = [
        &h.plan_operation,
        &h.retirement_operation,
        &child.operation_id,
        &h.parent.save_operation,
        &h.intent.prepare_operation,
    ];
    let mut seen = std::collections::HashSet::new();
    if ops.iter().any(|op| !seen.insert(*op)) {
        return Err("BusinessHandoffOperationConflict".into());
    }
    Ok((h, child))
}
pub(crate) fn validate_first(child: &proto::WriteRequest) -> Result<()> {
    morrow_editor_draft_model::validate_request(child).map_err(err)?;
    if child.source_kind != 0
        || child.expected_generation != 0
        || !child.predecessor_operation.is_empty()
        || !child.predecessor_sha256.is_empty()
        || child.assets.iter().any(|p| p.origin != 4)
    {
        return Err("BusinessHandoffFirstShape".into());
    }
    Ok(())
}
pub(crate) fn validate_link(
    r: &proto::WriteRequest,
    l: &proto::DevelopmentBusinessLink,
) -> Result<()> {
    let p = l.parent.as_ref().ok_or("BusinessLinkParentMissing")?;
    let i = l.intent.as_ref().ok_or("BusinessLinkIntentMissing")?;
    parent_proto(&parent_view(p))?;
    proof_proto(&proof_view(i))?;
    for op in [
        &l.plan_operation,
        &l.child_operation,
        &l.committed_operation,
    ] {
        intent::identity(op)?;
    }
    if l.schema_version != 1
        || p.draft_id == r.draft_id
        || r.source_kind != 0
        || l.committed_revision != r.source_revision
        || l.committed_revision == 0
        || [
            &l.handoff_request_sha256,
            &l.command_sha256,
            &l.content_sha256,
            &l.request_sha256,
            &l.publication_sha256,
        ]
        .iter()
        .any(|s| s.len() != 32)
        || !r.predecessor_operation.is_empty()
        || !r.predecessor_sha256.is_empty()
        || r.expected_generation == 0 && r.operation_id != l.child_operation
    {
        return Err("BusinessLinkShape".into());
    }
    Ok(())
}
pub(crate) fn validate_slot_shape(slot: &proto::Slot) -> Result<()> {
    let r = slot.request.as_ref().ok_or("BusinessDraftRequestMissing")?;
    if slot.development_business_link.is_some() && slot.development_fork_link.is_some()
        || slot.development_business_retirement.is_some()
            && slot.development_fork_retirement.is_some()
    {
        return Err("BusinessDraftAmbiguousLineage".into());
    }
    if let Some(l) = &slot.development_business_link {
        validate_link(r, l)?;
        if r.expected_generation > 0 && r.assets.iter().any(|p| p.origin == 4)
            || r.expected_generation == 0 && !slot.consumed_imports.is_empty()
        {
            return Err("BusinessDraftPinAdoption".into());
        }
    }
    if let Some(m) = &slot.development_business_retirement {
        let l = m
            .business_link
            .as_ref()
            .ok_or("BusinessRetirementLinkMissing")?;
        let p = l.parent.as_ref().ok_or("BusinessLinkParentMissing")?;
        if m.schema_version != 1
            || slot.active
            || p.draft_id != r.draft_id
            || p.generation.checked_add(1) != Some(slot.generation)
            || p.save_operation != r.operation_id
            || p.request_sha256 != hash(&r.encode_to_vec())
            || m.operation_id == l.child_operation
            || m.operation_id == p.save_operation
            || m.child_draft_id == r.draft_id
        {
            return Err("BusinessRetirementShape".into());
        }
        intent::identity(&m.operation_id)?;
        intent::identity(&m.child_draft_id)?;
        let mut child = r.clone();
        child.draft_id = m.child_draft_id.clone();
        child.source_kind = 0;
        child.source_revision = l.committed_revision;
        child.expected_generation = 1;
        validate_link(&child, l)?;
    }
    Ok(())
}
fn retirement_literal(h: &Handoff, child: &proto::WriteRequest, wire: &str) -> Result<String> {
    let mut value = serde_json::to_value(Retirement {
        schema_version: 1,
        intent: h.intent.clone(),
        plan_operation: h.plan_operation.clone(),
        handoff_request_sha256: hex(&hash(wire.as_bytes())),
        parent: h.parent.clone(),
        child_draft_id: child.draft_id.clone(),
        child_operation: child.operation_id.clone(),
        child_request_sha256: draft::request_sha256(child),
        operation_id: h.retirement_operation.clone(),
    })
    .map_err(err)?;
    value.sort_all_objects();
    let result = serde_json::to_string(&value).map_err(err)?;
    if result.len() > crate::LIMIT {
        return Err("BusinessRetirementLiteralBytesLimit".into());
    }
    Ok(result)
}
fn link_for(
    first: &intent_proto::Slot,
    h: &Handoff,
    child: &proto::WriteRequest,
    wire: &str,
) -> Result<proto::DevelopmentBusinessLink> {
    if h.intent != intent::proof(first)
        || child.card_id != first.card_id
        || child.source_revision != first.expected_revision
        || [
            &h.plan_operation,
            &h.retirement_operation,
            &child.operation_id,
        ]
        .contains(&&first.business_operation)
        || [
            &h.plan_operation,
            &h.retirement_operation,
            &child.operation_id,
        ]
        .contains(
            &&first
                .publication
                .as_ref()
                .ok_or("BusinessPublicationMissing")?
                .save_operation,
        )
    {
        return Err("BusinessHandoffOriginalIdentity".into());
    }
    Ok(proto::DevelopmentBusinessLink {
        schema_version: 1,
        parent: Some(parent_proto(&h.parent)?),
        intent: Some(proof_proto(&h.intent)?),
        plan_operation: h.plan_operation.clone(),
        handoff_request_sha256: hash(wire.as_bytes()),
        child_operation: child.operation_id.clone(),
        committed_operation: first.business_operation.clone(),
        committed_revision: first.expected_revision,
        command_sha256: first.command_sha256.clone(),
        content_sha256: first.content_sha256.clone(),
        request_sha256: first.request_sha256.clone(),
        publication_sha256: first.publication_sha256.clone(),
    })
}
fn close_plan_op(first: &intent_proto::Slot, op: &str) -> String {
    let mut sha = Sha256::new();
    sha.update(b"morrow.hmos.editor-intent-close-plan.v1\0");
    for p in [&intent::proof(first).intent_id, op] {
        sha.update((p.len() as u64).to_le_bytes());
        sha.update(p.as_bytes());
    }
    format!("morrow-host-intent-close-plan-{}", hex(&sha.finalize()))
}
pub(crate) fn validate_intent_shape(
    first: &intent_proto::Slot,
    live: &intent_proto::Slot,
) -> Result<()> {
    if live.prepared_record_sha256 != hash(&first.encode_to_vec()) {
        return Err("BusinessIntentPreparedDigest".into());
    }
    let (save, inspect) = intent::plans(first)?;
    if live.save_request_json != save || live.inspect_request_json != inspect {
        return Err("BusinessIntentTransportChanged".into());
    }
    let handoff = !live.handoff_request_json.is_empty();
    if handoff {
        let (h, c) = parse_handoff(&live.handoff_request_json)?;
        link_for(first, &h, &c, &live.handoff_request_json)?;
        if live.plan_operation != h.plan_operation
            || live.retirement_request_json
                != retirement_literal(&h, &c, &live.handoff_request_json)?
            || live.plan_assets.len() != c.assets.len()
        {
            return Err("BusinessIntentPlanChanged".into());
        }
        for (n, (p, s)) in live.plan_assets.iter().zip(&c.assets).enumerate() {
            if p.selection.as_ref() != Some(s) || p.pin_id != format!("intent-plan-pin-{n}") {
                return Err("BusinessIntentPlanPinsChanged".into());
            }
            intent::attachment(p)?;
        }
    } else if !live.plan_operation.is_empty()
        || !live.retirement_request_json.is_empty()
        || !live.plan_assets.is_empty()
    {
        return Err("BusinessIntentUnexpectedHandoff".into());
    }
    match live.phase {
        3 if handoff
            && live.generation == 3
            && live.active
            && live.mutation_operation == live.plan_operation
            && live.close_request_json.is_empty()
            && live.close_plan_operation.is_empty()
            && live.close_disposition.is_empty() => {}
        4 | 2 => {
            let c: intent::CloseLiteral = serde_json::from_str(&live.close_request_json)
                .map_err(|_| "BusinessIntentCloseSchema")?;
            let base = if handoff { 3 } else { 2 };
            if c.schema_version != 1
                || c.intent != intent::proof(first)
                || c.expected_generation != base.to_string()
                || c.disposition
                    != if handoff {
                        "handoff_retired"
                    } else {
                        "saved_exact"
                    }
                || c.plan_operation != live.plan_operation
                || live.close_disposition != c.disposition
                || live.close_plan_operation != close_plan_op(first, &c.operation_id)
                || live.phase == 4
                    && (live.generation != base + 1
                        || !live.active
                        || live.mutation_operation != live.close_plan_operation)
                || live.phase == 2
                    && (live.generation != base + 2
                        || live.active
                        || live.mutation_operation != c.operation_id)
                || handoff && c.parent.is_some()
                || !handoff && c.parent.is_none()
            {
                return Err("BusinessIntentCloseShape".into());
            }
        }
        _ => return Err("BusinessIntentPhaseShape".into()),
    }
    Ok(())
}
fn parent_history(host: &HostRuntime, card: &str, p: &DraftProof) -> Result<proto::Slot> {
    let pp = parent_proto(p)?;
    let s = draft::draft_history(host, &draft::key(card, &p.draft_id), &p.save_operation)?
        .ok_or("BusinessParentHistoryMissing")?;
    let r = s.request.as_ref().ok_or("BusinessParentRequestMissing")?;
    if !s.active
        || s.generation != pp.generation
        || r.card_id != card
        || r.draft_id != p.draft_id
        || r.operation_id != p.save_operation
        || hash(&r.encode_to_vec()) != pp.request_sha256
    {
        return Err("BusinessParentProofChanged".into());
    }
    Ok(s)
}
fn current_parent(host: &HostRuntime, card: &str, p: &DraftProof) -> Result<proto::Slot> {
    let old = parent_history(host, card, p)?;
    let live = draft::draft_card(host, card, &p.draft_id)?.ok_or("BusinessParentMissing")?;
    if live != old || !live.active {
        return Err("BusinessParentGenerationConflict".into());
    }
    Ok(live)
}
fn ancestor(host: &HostRuntime, first: &intent_proto::Slot, parent: &proto::Slot) -> Result<()> {
    let pubp = first
        .publication
        .as_ref()
        .ok_or("BusinessPublicationMissing")?;
    let pubslot = draft::draft_history(
        host,
        &draft::key(&first.card_id, &pubp.draft_id),
        &pubp.save_operation,
    )?
    .ok_or("BusinessPublicationHistoryMissing")?;
    if parent.source_card != pubslot.source_card {
        return Err("BusinessParentFrozenSourceChanged".into());
    }
    let mut slot = parent.clone();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..256 {
        let r = slot
            .request
            .as_ref()
            .ok_or("BusinessAncestorRequestMissing")?;
        if !seen.insert(r.draft_id.clone()) {
            return Err("BusinessAncestorLoop".into());
        }
        if r.draft_id == pubp.draft_id {
            if slot.generation < pubp.generation || slot.source_card != pubslot.source_card {
                return Err("BusinessAncestorPublicationChanged".into());
            }
            return Ok(());
        }
        let (p, childop, raw) = if let Some(l) = &slot.development_fork_link {
            (
                DraftProof {
                    draft_id: l.parent_draft_id.clone(),
                    generation: l.parent_generation.to_string(),
                    save_operation: l.parent_save_operation.clone(),
                    request_sha256: hex(&l.parent_request_sha256),
                },
                l.child_operation.clone(),
                true,
            )
        } else if let Some(l) = &slot.development_business_link {
            (
                parent_view(l.parent.as_ref().ok_or("BusinessAncestorParentMissing")?),
                l.child_operation.clone(),
                false,
            )
        } else {
            return Err("BusinessParentNotPublicationDescendant".into());
        };
        let childfirst =
            draft::draft_history(host, &draft::key(&first.card_id, &r.draft_id), &childop)?
                .ok_or("BusinessAncestorFirstMissing")?;
        if childfirst.generation != 1
            || childfirst.source_card != slot.source_card
            || childfirst.development_fork_link != slot.development_fork_link
            || childfirst.development_business_link != slot.development_business_link
        {
            return Err("BusinessAncestorFirstChanged".into());
        }
        let retired = draft::draft_card(host, &first.card_id, &p.draft_id)?
            .ok_or("BusinessAncestorParentMissing")?;
        let matches = if raw {
            retired
                .development_fork_retirement
                .as_ref()
                .is_some_and(|m| {
                    m.child_draft_id == r.draft_id && m.fork_link == slot.development_fork_link
                })
        } else {
            retired
                .development_business_retirement
                .as_ref()
                .is_some_and(|m| {
                    m.child_draft_id == r.draft_id
                        && m.business_link == slot.development_business_link
                })
        };
        if retired.active || !matches {
            return Err("BusinessAncestorParentNotRetired".into());
        }
        let retireop = if raw {
            &retired
                .development_fork_retirement
                .as_ref()
                .unwrap()
                .operation_id
        } else {
            &retired
                .development_business_retirement
                .as_ref()
                .unwrap()
                .operation_id
        };
        if draft::draft_history(host, &draft::key(&first.card_id, &p.draft_id), retireop)?.as_ref()
            != Some(&retired)
        {
            return Err("BusinessAncestorRetirementHistoryChanged".into());
        }
        slot = parent_history(host, &first.card_id, &p)?;
    }
    Err("BusinessAncestorIdentityLimit".into())
}
fn selected_pins(
    parent: &proto::Slot,
    child: &proto::WriteRequest,
    prefix: &str,
) -> Result<Vec<proto::StoredAsset>> {
    let mut last = None;
    let mut pins = vec![];
    for (n, s) in child.assets.iter().enumerate() {
        let (index, p) = parent
            .assets
            .iter()
            .enumerate()
            .find(|(_, p)| {
                p.selection
                    .as_ref()
                    .is_some_and(|a| a.asset_id == s.asset_id)
            })
            .ok_or("BusinessParentPinMissing")?;
        if s.origin != 4
            || p.selection.as_ref().is_none_or(|a| a.aliases != s.aliases)
            || last.is_some_and(|x| index <= x)
        {
            return Err("BusinessParentSelectionChanged".into());
        }
        last = Some(index);
        let mut p = p.clone();
        p.selection = Some(s.clone());
        p.pin_id = format!("{prefix}{n}");
        pins.push(p);
    }
    Ok(pins)
}
fn verify_parent_pins(host: &HostRuntime, parent: &proto::Slot) -> Result<()> {
    let r = parent
        .request
        .as_ref()
        .ok_or("BusinessParentRequestMissing")?;
    for pin in &parent.assets {
        draft::verify_pin(
            host,
            &draft::key(&r.card_id, &r.draft_id),
            pin,
            &mut std::io::sink(),
        )?;
    }
    Ok(())
}
fn resolve_imports(engine: &mut Engine, card: &str, parent: &str) -> Result<()> {
    let start = engine.start;
    let clock = || {
        u64::try_from(start.elapsed().as_millis())
            .unwrap_or(u64::MAX - 1)
            .saturating_add(1)
    };
    crate::editor_draft_staging::reconcile(
        &mut engine.host,
        card,
        parent,
        clock,
        crate::unix_millis()?,
        &mut "not_committed",
    )
    .map_err(err)?;
    for r in crate::editor_draft_staging::list(&engine.host, card, parent).map_err(err)? {
        if r.phase != crate::editor_draft_staging::DraftImportPhase::Retired {
            return Err("BusinessParentUnselectedImports".into());
        }
    }
    Ok(())
}
pub(crate) fn require_mutable(host: &HostRuntime, card: &str, parent: &str) -> Result<()> {
    for s in intent::all(host)? {
        if s.card_id != card {
            continue;
        }
        let planned = if !s.handoff_request_json.is_empty() {
            parse_handoff(&s.handoff_request_json)?.0.parent.draft_id == parent
        } else if !s.close_request_json.is_empty() && s.close_disposition == "saved_exact" {
            let c: intent::CloseLiteral = serde_json::from_str(&s.close_request_json)
                .map_err(|_| "BusinessIntentCloseSchema")?;
            c.parent
                .as_ref()
                .is_some_and(|p| p.proof.draft_id == parent)
        } else {
            false
        };
        if planned {
            return Err("BusinessParentPlanFrozen".into());
        }
    }
    Ok(())
}
fn registered(
    host: &HostRuntime,
    link: &proto::DevelopmentBusinessLink,
) -> Result<(
    intent_proto::Slot,
    intent_proto::Slot,
    Handoff,
    proto::WriteRequest,
)> {
    let (first, live) = intent::immutable(
        host,
        &proof_view(link.intent.as_ref().ok_or("BusinessLinkIntentMissing")?),
    )?;
    let plan = intent::history(host, &intent::proof(&first).intent_id, &link.plan_operation)?
        .ok_or("BusinessPlanHistoryMissing")?;
    if plan.phase != 3 || plan.generation != 3 {
        return Err("BusinessPlanHistoryShape".into());
    }
    let (h, c) = parse_handoff(&plan.handoff_request_json)?;
    if link_for(&first, &h, &c, &plan.handoff_request_json)? != *link
        || live.handoff_request_json != plan.handoff_request_json
        || live.retirement_request_json != plan.retirement_request_json
    {
        return Err("BusinessPlanLinkChanged".into());
    }
    let actual = editor_business::intent_result(host, &first)?;
    if actual.summary().revision != c.source_revision {
        return Err("BusinessPlanRevisionChanged".into());
    }
    Ok((first, live, h, c))
}
fn verified_first(host: &HostRuntime, slot: &proto::Slot) -> Result<proto::Slot> {
    let r = slot.request.as_ref().ok_or("BusinessDraftRequestMissing")?;
    let l = slot
        .development_business_link
        .as_ref()
        .ok_or("BusinessLinkMissing")?;
    let (first, _, h, c) = registered(host, l)?;
    let original = draft::draft_history(
        host,
        &draft::key(&r.card_id, &r.draft_id),
        &l.child_operation,
    )?
    .ok_or("BusinessChildFirstMissing")?;
    if !original.active
        || original.generation != 1
        || original.request.as_ref() != Some(&c)
        || original.development_business_link.as_ref() != Some(l)
        || original.source_card != editor_business::intent_result(host, &first)?.encode()
        || original.source_card != slot.source_card
    {
        return Err("BusinessChildFirstChanged".into());
    }
    let parent = parent_history(host, &r.card_id, &h.parent)?;
    let pins = selected_pins(&parent, &c, "draft-asset-")?;
    if original.assets != pins {
        return Err("BusinessChildPinsChanged".into());
    }
    Ok(original)
}
pub(crate) fn verify_slot(host: &HostRuntime, slot: &proto::Slot) -> Result<()> {
    if slot.development_business_link.is_some() {
        verified_first(host, slot)?;
    }
    if let Some(m) = &slot.development_business_retirement {
        let r = slot.request.as_ref().ok_or("BusinessDraftRequestMissing")?;
        let l = m
            .business_link
            .as_ref()
            .ok_or("BusinessRetirementLinkMissing")?;
        let child = draft::draft_card(host, &r.card_id, &m.child_draft_id)?
            .ok_or("BusinessChildMissing")?;
        verified_first(host, &child)?;
        if child.development_business_link.as_ref() != Some(l) {
            return Err("BusinessRetirementChildChanged".into());
        }
        let old =
            draft::draft_history(host, &draft::key(&r.card_id, &r.draft_id), &m.operation_id)?
                .ok_or("BusinessRetirementHistoryMissing")?;
        if old.development_business_retirement.as_ref() != Some(m) {
            return Err("BusinessRetirementHistoryChanged".into());
        }
    }
    Ok(())
}
pub(crate) fn require_parent_retired(host: &HostRuntime, slot: &proto::Slot) -> Result<()> {
    let l = slot
        .development_business_link
        .as_ref()
        .ok_or("BusinessLinkMissing")?;
    let r = slot.request.as_ref().ok_or("BusinessDraftRequestMissing")?;
    let p = l.parent.as_ref().ok_or("BusinessLinkParentMissing")?;
    let parent =
        draft::draft_card(host, &r.card_id, &p.draft_id)?.ok_or("BusinessParentMissing")?;
    verify_slot(host, &parent)?;
    if parent.active
        || parent
            .development_business_retirement
            .as_ref()
            .is_none_or(|m| m.child_draft_id != r.draft_id || m.business_link.as_ref() != Some(l))
    {
        return Err("BusinessParentNotRetired".into());
    }
    Ok(())
}
fn preflight_reply(
    engine: &Engine,
    first: &intent_proto::Slot,
    live: &intent_proto::Slot,
    record: &draft::DraftRecord,
) -> Result<()> {
    intent::preflight(engine, first, live)?;
    let mut reply = Reply::failure(String::new());
    reply.ok = true;
    reply.effect = "committed";
    reply.receipt_revision = record.slot.generation.to_string();
    reply.drafts = vec![draft_bridge::View::from_record(record.clone())?];
    reply.editor_intents = Some(vec![intent::make_view(
        engine,
        first,
        live,
        "summary",
        record.repeated,
    )?]);
    if serde_json::to_vec(&reply).map_err(err)?.len() > crate::LIMIT {
        return Err("BusinessHandoffReplyBytesLimit".into());
    }
    Ok(())
}
fn record_reply(
    engine: &Engine,
    first: &intent_proto::Slot,
    live: &intent_proto::Slot,
    record: draft::DraftRecord,
) -> Result<Reply> {
    preflight_reply(engine, first, live, &record)?;
    let mut reply = Reply::failure(String::new());
    reply.ok = true;
    reply.effect = engine.effect;
    reply.receipt_revision = record.slot.generation.to_string();
    reply.drafts = vec![draft_bridge::View::from_record(record.clone())?];
    reply.editor_intents = Some(vec![intent::make_view(
        engine,
        first,
        live,
        "summary",
        record.repeated,
    )?]);
    Ok(reply)
}
fn create_child(
    engine: &mut Engine,
    first: &intent_proto::Slot,
    live: &intent_proto::Slot,
    h: &Handoff,
    c: &proto::WriteRequest,
    l: &proto::DevelopmentBusinessLink,
) -> Result<draft::DraftRecord> {
    engine.effect = "not_committed";
    let id = draft::key(&c.card_id, &c.draft_id);
    if let Some(old) = draft::draft_history(&engine.host, &id, &c.operation_id)? {
        engine.effect = "committed";
        if old.request.as_ref() != Some(c) || old.development_business_link.as_ref() != Some(l) {
            return Err("BusinessChildRetryChanged".into());
        }
        verified_first(&engine.host, &old)?;
        let current = draft::draft_card(&engine.host, &c.card_id, &c.draft_id)?
            .ok_or("BusinessChildCurrentMissing")?;
        verify_slot(&engine.host, &current)?;
        return Ok(draft::DraftRecord {
            slot: old,
            current_generation: current.generation,
            current_active: current.active,
            repeated: true,
        });
    }
    if !live.active
        || live.phase != 3
        || draft::draft_card(&engine.host, &c.card_id, &c.draft_id)?.is_some()
    {
        return Err("BusinessChildIdentityOrPlanConflict".into());
    }
    current_parent(&engine.host, &c.card_id, &h.parent)?;
    intent::verify_pins(&engine.host, live)?;
    let mut pins = live.plan_assets.clone();
    for (n, p) in pins.iter_mut().enumerate() {
        p.pin_id = draft::asset_pin(n);
    }
    let mut slot = proto::Slot {
        schema_version: 1,
        request: Some(c.clone()),
        generation: 1,
        active: true,
        source_card: editor_business::intent_result(&engine.host, first)?.encode(),
        assets: pins,
        development_business_link: Some(l.clone()),
        ..Default::default()
    };
    slot.active_bytes = draft::charge(&slot)?;
    draft::decode_body(&slot.encode_to_vec())?;
    let predicted = draft::DraftRecord {
        slot: slot.clone(),
        current_generation: 1,
        current_active: true,
        repeated: false,
    };
    preflight_reply(engine, first, live, &predicted)?;
    intent::check_capacity(&engine.host, &id, slot.active_bytes, true)?;
    let start = engine.start;
    let clock = || {
        u64::try_from(start.elapsed().as_millis())
            .unwrap_or(u64::MAX - 1)
            .saturating_add(1)
    };
    draft::write_draft_journal(
        &mut engine.host,
        &id,
        &c.operation_id,
        0,
        &slot,
        clock,
        &mut engine.effect,
    )?;
    Ok(predicted)
}
fn handoff(engine: &mut Engine, wire: &str) -> Result<Reply> {
    let (h, c) = parse_handoff(wire)?;
    let (first, mut live) = intent::immutable(&engine.host, &h.intent)?;
    let link = link_for(&first, &h, &c, wire)?;
    if let Some(plan) = intent::history(&engine.host, &h.intent.intent_id, &h.plan_operation)? {
        if plan.phase != 3 || plan.handoff_request_json != wire || live.handoff_request_json != wire
        {
            return Err("BusinessPlanRetryChanged".into());
        }
    } else {
        if live.phase != 1 || live.generation != 2 || !live.active {
            return Err("BusinessPlanIntentConflict".into());
        }
        editor_business::intent_result(&engine.host, &first)?;
        draft::require_mutable(&engine.host, &c.card_id, &h.parent.draft_id)?;
        let parent = current_parent(&engine.host, &c.card_id, &h.parent)?;
        ancestor(&engine.host, &first, &parent)?;
        draft::require_incoming_parent_retired(&engine.host, &parent)?;
        let retained = selected_pins(&parent, &c, "intent-plan-pin-")?;
        verify_parent_pins(&engine.host, &parent)?;
        if draft::draft_card(&engine.host, &c.card_id, &c.draft_id)?.is_some() {
            return Err("BusinessChildIdentityAlreadyUsed".into());
        }
        for operation in [&c.operation_id, &h.retirement_operation] {
            if !matches!(
                engine.host.store_local().lookup(operation).map_err(err)?,
                morrow_core::transaction::Lookup::Absent
            ) {
                return Err("BusinessHandoffOperationAlreadyUsed".into());
            }
        }
        let mut plan = live.clone();
        plan.phase = 3;
        plan.generation = 3;
        plan.mutation_operation = h.plan_operation.clone();
        plan.plan_operation = h.plan_operation.clone();
        plan.handoff_request_json = wire.into();
        plan.retirement_request_json = retirement_literal(&h, &c, wire)?;
        plan.plan_assets = retained;
        plan.active_bytes = intent::charge(&plan)?;
        let mut childpins = plan.plan_assets.clone();
        for (n, p) in childpins.iter_mut().enumerate() {
            p.pin_id = draft::asset_pin(n);
        }
        let mut predicted = proto::Slot {
            schema_version: 1,
            request: Some(c.clone()),
            generation: 1,
            active: true,
            source_card: editor_business::intent_result(&engine.host, &first)?.encode(),
            assets: childpins,
            development_business_link: Some(link.clone()),
            ..Default::default()
        };
        predicted.active_bytes = draft::charge(&predicted)?;
        let child_bytes = predicted.active_bytes;
        preflight_reply(
            engine,
            &first,
            &plan,
            &draft::DraftRecord {
                slot: predicted,
                current_generation: 1,
                current_active: true,
                repeated: false,
            },
        )?;
        // Check both the extra identity/active slot and the complete prospective
        // three-way logical charge before freezing the parent. This is a
        // serialized-host preflight, never a global multi-object reservation.
        intent::check_capacity(
            &engine.host,
            &draft::key(&c.card_id, &c.draft_id),
            child_bytes,
            true,
        )?;
        intent::check_capacity(
            &engine.host,
            &h.intent.intent_id,
            plan.active_bytes
                .checked_add(child_bytes)
                .ok_or("BusinessHandoffBytesOverflow")?,
            false,
        )?;
        resolve_imports(engine, &c.card_id, &h.parent.draft_id)?;
        current_parent(&engine.host, &c.card_id, &h.parent)?;
        intent::write(engine, &plan, 2)?;
        live = plan;
    }
    engine.effect = "not_committed";
    let record = create_child(engine, &first, &live, &h, &c, &link)?;
    record_reply(engine, &first, &live, record)
}
fn parse_retire(wire: &str) -> Result<Retirement> {
    if wire.len() > crate::LIMIT {
        return Err("BusinessRetirementBytesLimit".into());
    }
    serde_json::from_str(wire).map_err(|_| "BusinessRetirementSchema".into())
}
fn retire(engine: &mut Engine, wire: &str) -> Result<Reply> {
    let r = parse_retire(wire)?;
    let (first, live) = intent::immutable(&engine.host, &r.intent)?;
    let (h, c) = parse_handoff(&live.handoff_request_json)?;
    if live.retirement_request_json != wire
        || retirement_literal(&h, &c, &live.handoff_request_json)? != wire
    {
        return Err("BusinessRetirementRegisteredLiteralRequired".into());
    }
    let l = link_for(&first, &h, &c, &live.handoff_request_json)?;
    let marker = proto::DevelopmentBusinessRetirement {
        schema_version: 1,
        child_draft_id: c.draft_id.clone(),
        operation_id: r.operation_id.clone(),
        business_link: Some(l.clone()),
    };
    let id = draft::key(&c.card_id, &h.parent.draft_id);
    if let Some(old) = draft::draft_history(&engine.host, &id, &r.operation_id)? {
        engine.effect = "committed";
        if old.active || old.development_business_retirement.as_ref() != Some(&marker) {
            return Err("BusinessRetirementRetryChanged".into());
        }
        verify_slot(&engine.host, &old)?;
        let current = draft::draft_card(&engine.host, &c.card_id, &h.parent.draft_id)?
            .ok_or("BusinessParentMissing")?;
        return record_reply(
            engine,
            &first,
            &live,
            draft::DraftRecord {
                slot: old,
                current_generation: current.generation,
                current_active: current.active,
                repeated: true,
            },
        );
    }
    if live.phase != 3 || !live.active {
        return Err("BusinessRetirementIntentConflict".into());
    }
    let child =
        draft::draft_card(&engine.host, &c.card_id, &c.draft_id)?.ok_or("BusinessChildMissing")?;
    verified_first(&engine.host, &child)?;
    if !child.active || child.development_business_link.as_ref() != Some(&l) {
        return Err("BusinessRetirementChildInactive".into());
    }
    intent::verify_pins(&engine.host, &live)?;
    verify_parent_pins(&engine.host, &child)?;
    let mut parent = current_parent(&engine.host, &c.card_id, &h.parent)?;
    verify_parent_pins(&engine.host, &parent)?;
    resolve_imports(engine, &c.card_id, &h.parent.draft_id)?;
    parent.generation = parent
        .generation
        .checked_add(1)
        .ok_or("BusinessParentGenerationExhausted")?;
    parent.active = false;
    parent.active_bytes = 0;
    parent.development_business_retirement = Some(marker);
    let record = draft::DraftRecord {
        slot: parent.clone(),
        current_generation: parent.generation,
        current_active: false,
        repeated: false,
    };
    preflight_reply(engine, &first, &live, &record)?;
    let start = engine.start;
    let clock = || {
        u64::try_from(start.elapsed().as_millis())
            .unwrap_or(u64::MAX - 1)
            .saturating_add(1)
    };
    draft::write_draft_journal(
        &mut engine.host,
        &id,
        &r.operation_id,
        draft_bridge::number(&h.parent.generation)?,
        &parent,
        clock,
        &mut engine.effect,
    )?;
    record_reply(engine, &first, &live, record)
}
fn exact_snapshot(pubslot: &proto::Slot, parent: &proto::Slot) -> bool {
    let p = pubslot.request.as_ref().unwrap();
    let r = parent.request.as_ref().unwrap();
    p.values == r.values
        && pubslot.source_card == parent.source_card
        && pubslot.assets.len() == parent.assets.len()
        && pubslot.assets.iter().zip(&parent.assets).all(|(a, b)| {
            a.selection
                .as_ref()
                .zip(b.selection.as_ref())
                .is_some_and(|(x, y)| x.asset_id == y.asset_id && x.aliases == y.aliases)
                && a.display_name == b.display_name
                && a.media_type == b.media_type
                && a.byte_length == b.byte_length
                && a.sha256 == b.sha256
        })
}
pub(crate) fn close(engine: &mut Engine, wire: &str, c: intent::CloseLiteral) -> Result<Reply> {
    intent::identity(&c.operation_id)?;
    let (first, mut live) = intent::immutable(&engine.host, &c.intent)?;
    let base = if c.disposition == "saved_exact" {
        2
    } else if c.disposition == "handoff_retired" {
        3
    } else {
        return Err("BusinessCloseDisposition".into());
    };
    if c.schema_version != 1
        || c.expected_generation != base.to_string()
        || c.operation_id == first.business_operation
        || c.operation_id == first.prepare_operation
        || c.operation_id == first.publication.as_ref().unwrap().save_operation
        || c.operation_id == live.plan_operation
    {
        return Err("BusinessCloseIdentity".into());
    }
    if let Some(closed) = intent::history(&engine.host, &c.intent.intent_id, &c.operation_id)? {
        engine.effect = "committed";
        if closed.phase != 2 || closed.close_request_json != wire {
            return Err("BusinessCloseRetryChanged".into());
        }
        return intent::reply(
            vec![intent::make_view(engine, &first, &live, "summary", true)?],
            String::new(),
            engine.effect,
            closed.generation.to_string(),
        );
    }
    let planop = close_plan_op(&first, &c.operation_id);
    if let Some(plan) = intent::history(&engine.host, &c.intent.intent_id, &planop)? {
        if plan.phase != 4
            || plan.close_request_json != wire
            || live.close_request_json != wire
            || live.phase != 4
        {
            return Err("BusinessClosePlanRetryChanged".into());
        }
    } else {
        if !live.active || live.generation != base || live.phase != if base == 2 { 1 } else { 3 } {
            return Err("BusinessCloseIntentConflict".into());
        }
        editor_business::intent_result(&engine.host, &first)?;
        if base == 2 {
            if !c.plan_operation.is_empty() {
                return Err("BusinessSavedExactPlan".into());
            }
            let p = c
                .parent
                .as_ref()
                .ok_or("BusinessSavedExactParentRequired")?;
            intent::identity(&p.discard_operation)?;
            if !matches!(
                engine
                    .host
                    .store_local()
                    .lookup(&p.discard_operation)
                    .map_err(err)?,
                morrow_core::transaction::Lookup::Absent
            ) {
                return Err("BusinessSavedExactDiscardOperationAlreadyUsed".into());
            }
            if p.discard_operation == c.operation_id
                || p.discard_operation == p.proof.save_operation
                || p.discard_operation == first.business_operation
                || p.discard_operation == first.prepare_operation
            {
                return Err("BusinessSavedExactDiscardOperation".into());
            }
            draft::require_mutable(&engine.host, &first.card_id, &p.proof.draft_id)?;
            let parent = current_parent(&engine.host, &first.card_id, &p.proof)?;
            draft::require_incoming_parent_retired(&engine.host, &parent)?;
            ancestor(&engine.host, &first, &parent)?;
            let pubp = first.publication.as_ref().unwrap();
            let pubslot = draft::draft_history(
                &engine.host,
                &draft::key(&first.card_id, &pubp.draft_id),
                &pubp.save_operation,
            )?
            .ok_or("BusinessPublicationHistoryMissing")?;
            if !exact_snapshot(&pubslot, &parent) {
                return Err("BusinessSavedExactChangedRaw".into());
            }
            resolve_imports(engine, &first.card_id, &p.proof.draft_id)?;
            verify_parent_pins(&engine.host, &parent)?;
        } else {
            if c.parent.is_some() || c.plan_operation != live.plan_operation {
                return Err("BusinessCloseHandoffPlan".into());
            }
            let (h, child) = parse_handoff(&live.handoff_request_json)?;
            let l = link_for(&first, &h, &child, &live.handoff_request_json)?;
            let current = draft::draft_card(&engine.host, &first.card_id, &child.draft_id)?
                .ok_or("BusinessChildMissing")?;
            verified_first(&engine.host, &current)?;
            require_parent_retired(&engine.host, &current)?;
            intent::verify_pins(&engine.host, &live)?;
            if current.active {
                verify_parent_pins(&engine.host, &current)?;
            }
            let retired = draft::draft_history(
                &engine.host,
                &draft::key(&first.card_id, &h.parent.draft_id),
                &h.retirement_operation,
            )?
            .ok_or("BusinessRetirementHistoryMissing")?;
            if retired
                .development_business_retirement
                .as_ref()
                .is_none_or(|m| m.business_link.as_ref() != Some(&l))
            {
                return Err("BusinessCloseRetirementChanged".into());
            }
        }
        let mut plan = live.clone();
        plan.generation = base + 1;
        plan.phase = 4;
        plan.close_request_json = wire.into();
        plan.close_plan_operation = planop.clone();
        plan.close_disposition = c.disposition.clone();
        plan.mutation_operation = planop;
        plan.active_bytes = intent::charge(&plan)?;
        let mut predicted = plan.clone();
        predicted.phase = 2;
        predicted.generation = base + 2;
        predicted.active = false;
        predicted.active_bytes = 0;
        predicted.mutation_operation = c.operation_id.clone();
        intent::preflight(engine, &first, &plan)?;
        intent::preflight(engine, &first, &predicted)?;
        intent::check_capacity(&engine.host, &c.intent.intent_id, plan.active_bytes, false)?;
        intent::write(engine, &plan, base)?;
        live = plan;
    }
    engine.effect = "not_committed";
    if base == 2 {
        let p = c
            .parent
            .as_ref()
            .ok_or("BusinessSavedExactParentRequired")?;
        let id = draft::key(&first.card_id, &p.proof.draft_id);
        let old = draft::draft_history(&engine.host, &id, &p.discard_operation)?;
        if let Some(discarded) = old {
            let mut expected = parent_history(&engine.host, &first.card_id, &p.proof)?;
            expected.active = false;
            expected.active_bytes = 0;
            expected.generation += 1;
            if discarded != expected {
                return Err("BusinessSavedExactDiscardRetryChanged".into());
            }
        } else {
            let mut parent = current_parent(&engine.host, &first.card_id, &p.proof)?;
            resolve_imports(engine, &first.card_id, &p.proof.draft_id)?;
            parent.active = false;
            parent.active_bytes = 0;
            parent.generation += 1;
            let start = engine.start;
            let clock = || {
                u64::try_from(start.elapsed().as_millis())
                    .unwrap_or(u64::MAX - 1)
                    .saturating_add(1)
            };
            draft::write_draft_journal(
                &mut engine.host,
                &id,
                &p.discard_operation,
                draft_bridge::number(&p.proof.generation)?,
                &parent,
                clock,
                &mut "not_committed",
            )?;
        }
    }
    engine.effect = "not_committed";
    let mut closed = live.clone();
    closed.phase = 2;
    closed.generation = base + 2;
    closed.active = false;
    closed.active_bytes = 0;
    closed.mutation_operation = c.operation_id;
    intent::write(engine, &closed, base + 1)?;
    intent::reply(
        vec![intent::make_view(
            engine, &first, &closed, "summary", false,
        )?],
        String::new(),
        engine.effect,
        closed.generation.to_string(),
    )
}
pub(crate) fn reject_other_envelope(r: &Request) -> Result<()> {
    if r.business_handoff.is_some() && r.action != "draft_continue_business"
        || r.business_retirement.is_some() && r.action != "draft_continue_business_retire"
    {
        return Err("BusinessHandoffEnvelopeActionMismatch".into());
    }
    Ok(())
}
pub(crate) fn execute(engine: &mut Engine, mut r: Request) -> Result<Reply> {
    reject_other_envelope(&r)?;
    let wire = if r.action == "draft_continue_business" {
        r.business_handoff
            .take()
            .ok_or("BusinessHandoffEnvelopeRequired")?
            .request_json
    } else {
        r.business_retirement
            .take()
            .ok_or("BusinessRetirementEnvelopeRequired")?
            .request_json
    };
    let envelope = if r.action == "draft_continue_business" {
        "business_handoff"
    } else {
        "business_retirement"
    };
    let actual =
        serde_json::to_vec(&serde_json::json!({"action":r.action,envelope:{"request_json":wire}}))
            .map_err(err)?;
    if actual.len() > crate::LIMIT || r.transport_json.len() > crate::LIMIT {
        return Err("BusinessHandoffEnvelopeBytesLimit".into());
    }
    if !editor_business::route_is_empty(&r) || r.editor_save.is_some() || r.editor_commit.is_some()
    {
        return Err("BusinessHandoffOuterFields".into());
    }
    if r.action == "draft_continue_business" {
        handoff(engine, &wire)
    } else {
        retire(engine, &wire)
    }
}

#[cfg(test)]
mod tests;
