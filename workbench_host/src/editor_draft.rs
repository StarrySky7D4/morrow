//! Host-local durable editor drafts. These records never submit business edits
//! or confer plugin/capture authority. The captured S1 proposal has its own slot.
#[cfg(test)]
mod accounting_tests;
mod handoff;
mod handoff_proof;
mod handoff_proposal;
pub mod model;
use crate::{Result, Workbench, WorkbenchState, captured_cards, now};
pub use handoff::EditorDraftLineage;
pub use handoff_proposal::{HandoffProposalRecord, HandoffProposalStatus};
use model::{MAX_BODY_BYTES, proto, validate_request};
use morrow_core::{
    content::{Attachment, CardRecord},
    content_change::ContentChange,
    lifecycle::GrantKind,
    task_evidence,
    transaction::{self, Lookup},
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    io::Cursor,
};

const PREFIX: &str = "morrow-host-editor-draft-";
const TYPE: &str = "org.morrow.host.editor-draft";
const TITLE: &str = "Editor draft";
const EVIDENCE: &str = "draft-predecessor-evidence";
const MAX_SLOTS: usize = 16;
const MAX_JOURNALS: usize = 256;
const MAX_STAGED_ASSETS: usize = MAX_SLOTS * 20;

pub(crate) struct DraftStagedAsset {
    generation: u64,
    attachment: Attachment,
}
const MAX_ACTIVE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct EditorDraftRecord {
    pub slot: proto::Slot,
    /// May exceed the returned historical slot generation after an exact retry.
    pub current_generation: u64,
    pub current_active: bool,
    pub repeated: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorDraftSummary {
    pub card_id: String,
    pub draft_id: String,
    pub generation: u64,
    pub active: bool,
}
fn identity(card: &str, draft: &str, op: &str) -> Result<()> {
    for id in [card, draft] {
        morrow_core::runtime::Command::ReadSummary {
            request_id: op.into(),
            card_id: id.into(),
        }
        .validate()?;
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
    let mut blobs = slot.predecessor_evidence_bytes;
    for asset in &slot.assets {
        blobs = blobs
            .checked_add(asset.byte_length)
            .ok_or("draft byte count overflow")?;
    }
    // Include the accounting field itself, whose protobuf varint width depends
    // on the total. This monotonic fixed point converges in a handful of steps.
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
    let slot = proto::Slot::decode(raw)?;
    if slot.schema_version != 1 || slot.encode_to_vec() != raw {
        return Err("unsupported or noncanonical draft".into());
    }
    let request = slot.request.as_ref().ok_or("draft request missing")?;
    validate_request(request)?;
    if let Some(link) = &slot.parent_link {
        model::validate_parent_link(request, link)?;
        if request.expected_generation != 0 && request.assets.iter().any(|asset| asset.origin == 4)
        {
            return Err("parent pins must be adopted by later child generations".into());
        }
    } else if request.assets.iter().any(|asset| asset.origin == 4) {
        return Err("parent asset has no durable lineage".into());
    }
    if let Some(retired) = &slot.retirement {
        identity(
            &request.card_id,
            &retired.child_draft_id,
            &retired.operation_id,
        )?;
        identity(
            &request.card_id,
            &retired.child_draft_id,
            &retired.child_operation,
        )?;
        if slot.active
            || retired.child_draft_id == request.draft_id
            || retired.parent_generation == 0
            || retired.parent_generation.checked_add(1) != Some(slot.generation)
        {
            return Err("invalid conditional draft retirement".into());
        }
    }

    if slot.generation == 0
        || (slot.active && request.expected_generation.checked_add(1) != Some(slot.generation))
    {
        return Err("draft generation mismatch".into());
    }
    if request.source_kind == 0 {
        let source = CardRecord::decode(&slot.source_card)?;
        crate::versioned_record::decode(&source)?;
        if source.encode() != slot.source_card
            || source.summary().id != request.card_id
            || source.summary().revision != request.source_revision
        {
            return Err("draft exact source changed".into());
        }
    } else if !slot.source_card.is_empty() {
        return Err("new-card draft must not invent a source card".into());
    }
    if request.predecessor_operation.is_empty() {
        if !slot.predecessor_card.is_empty() || slot.predecessor_evidence_bytes != 0 {
            return Err("unexpected draft predecessor".into());
        }
    } else {
        let projected = CardRecord::decode(&slot.predecessor_card)?;
        crate::versioned_record::decode(&projected)?;
        if projected.encode() != slot.predecessor_card
            || projected.summary().id != request.card_id
            || CardRecord::decode(&slot.source_card)?
                .summary()
                .revision
                .checked_add(1)
                != Some(projected.summary().revision)
            || slot.predecessor_evidence_bytes == 0
            || slot.predecessor_evidence_bytes > task_evidence::MAX_CONTAINER_BYTES as u64
        {
            return Err("draft predecessor metadata changed".into());
        }
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
    let mut expected = Vec::new();
    if slot.active {
        expected = slot
            .assets
            .iter()
            .map(attachment)
            .collect::<Result<Vec<_>>>()?;
        if slot.predecessor_evidence_bytes != 0 {
            let evidence = card
                .attachments()
                .into_iter()
                .find(|a| a.id == EVIDENCE)
                .ok_or("draft predecessor evidence missing")?;
            if evidence.display_name != "Draft predecessor evidence"
                || evidence.media_type != "application/vnd.morrow.task-evidence"
                || evidence.byte_length != slot.predecessor_evidence_bytes
            {
                return Err("draft predecessor evidence metadata changed".into());
            }
            expected.push(evidence);
        }
    }
    if card.attachments() != expected {
        return Err("draft pin list changed".into());
    }
    Ok(slot)
}
impl WorkbenchState {
    pub(crate) fn draft_card(
        &self,
        card_id: &str,
        draft_id: &str,
    ) -> Result<Option<(CardRecord, proto::Slot)>> {
        identity(card_id, draft_id, "draft-read")?;
        let Some(card) = self.host.store_local().card(&key(card_id, draft_id))? else {
            return Ok(None);
        };
        let slot = decode_card(&card)?;
        if slot.active && slot.predecessor_evidence_bytes != 0 {
            let request = slot.request.as_ref().expect("validated request");
            let mut bytes = Vec::with_capacity(slot.predecessor_evidence_bytes as usize);
            self.host.store_local().export_attachment_local(
                &card.summary().id,
                EVIDENCE,
                &mut bytes,
            )?;
            if bytes.len() as u64 != slot.predecessor_evidence_bytes {
                return Err("draft predecessor length".into());
            }
            let evidence = task_evidence::decode(
                &bytes,
                request
                    .predecessor_sha256
                    .as_slice()
                    .try_into()
                    .map_err(|_| "draft predecessor digest")?,
            )?;
            let projected = captured_cards::derive(&evidence)?;
            if projected.source_card() != slot.source_card
                || projected.card().encode() != slot.predecessor_card
                || projected.operation_id() != request.predecessor_operation
            {
                return Err("draft predecessor proof changed".into());
            }
        }
        if slot.parent_link.is_some() {
            self.verify_draft_lineage(&slot)?;
        }
        Ok(Some((card, slot)))
    }
    fn retire_draft_imports(&mut self, request: &proto::WriteRequest) {
        // Unselected imports intentionally stay with this draft until selected
        // or discarded. An old receipt must not retire later-generation input.
        for selected in &request.assets {
            let owner = (
                request.card_id.clone(),
                request.draft_id.clone(),
                selected.asset_id.clone(),
            );
            if self
                .draft_staged
                .get(&owner)
                .is_some_and(|v| v.generation <= request.expected_generation)
            {
                self.draft_staged.remove(&owner);
            }
        }
    }
    fn draft_history(&self, id: &str, op: &str) -> Result<Option<proto::Slot>> {
        if matches!(self.host.store_local().lookup(op)?, Lookup::Absent) {
            return Ok(None);
        }
        let (commit, receipt) = self
            .host
            .store_local()
            .operation_commit(id, op)?
            .ok_or("draft operation belongs to another object")?;
        let command = transaction::decode_command(&commit.command)?;
        let slot = match command.action {
            Some(transaction::proto::command::Action::CreateCard(raw)) => {
                let card = CardRecord::decode(&raw)?;
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
                let mut refs = slot
                    .assets
                    .iter()
                    .map(attachment)
                    .collect::<Result<Vec<_>>>()?;
                if !slot.active {
                    refs.clear();
                }
                let values = change.attachments.expect("checked").items;
                let expected_len =
                    refs.len() + usize::from(slot.active && slot.predecessor_evidence_bytes != 0);
                if values.len() != expected_len {
                    return Err("draft historical pins mismatch".into());
                }
                for (value, expected) in values.iter().zip(&refs) {
                    if value.id != expected.id
                        || value.display_name != expected.display_name
                        || value.media_type != expected.media_type
                        || value.byte_length != expected.byte_length
                        || value.sha256 != expected.sha256
                    {
                        return Err("draft historical asset mismatch".into());
                    }
                }
                if expected_len > refs.len() {
                    let value = values.last().expect("evidence ref");
                    if value.id != EVIDENCE
                        || value.byte_length != slot.predecessor_evidence_bytes
                        || value.display_name != "Draft predecessor evidence"
                        || value.media_type != "application/vnd.morrow.task-evidence"
                        || value.sha256.len() != 32
                    {
                        return Err("draft historical evidence mismatch".into());
                    }
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
    pub(crate) fn all_draft_metadata(&self) -> Result<Vec<proto::Slot>> {
        let mut after = PREFIX.to_owned();
        let mut result = Vec::new();
        loop {
            let ids = self.host.store_local().card_ids_local(&after, 32)?;
            if ids.is_empty() {
                break;
            }
            let count = ids.len();
            for id in ids {
                if !id.starts_with(PREFIX) {
                    return Ok(result);
                }
                let card = self
                    .host
                    .store_local()
                    .card(&id)?
                    .ok_or("draft journal disappeared")?;
                let slot = decode_card(&card)?;
                result.push(slot);
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
        &mut self,
        id: &str,
        operation: &str,
        previous: u64,
        slot: &proto::Slot,
        attachments: Vec<Attachment>,
    ) -> Result<()> {
        let body = slot.encode_to_vec();
        decode_body(&body)?;
        self.host.prepare_write()?;
        let mut connection = self.host.connect()?;
        let start = self.start;
        let result = (|| -> Result<()> {
            let tick = now(start);
            self.host.grant(
                &mut connection,
                if previous == 0 {
                    GrantKind::CreateContent
                } else {
                    GrantKind::EditContent
                },
                id,
                tick.saturating_add(30_000),
                tick,
            )?;
            if previous == 0 {
                let card =
                    CardRecord::new_with_attachments(id, TYPE, 1, TITLE, body, &attachments)?;
                self.host
                    .create_content(&connection, operation, &card, || now(start))?;
            } else {
                self.host.edit_content(
                    &connection,
                    &ContentChange {
                        operation_id: operation.into(),
                        card_id: id.into(),
                        expected_revision: previous,
                        title: TITLE.into(),
                        body,
                        preview_text: String::new(),
                        attachments: Some(attachments),
                    },
                    || now(start),
                )?;
            }
            Ok(())
        })();
        let disconnected = self.host.disconnect(&connection);
        result?;
        disconnected?;
        Ok(())
    }
    pub(crate) fn save_editor_draft(
        &mut self,
        request: &proto::WriteRequest,
    ) -> Result<EditorDraftRecord> {
        self.save_editor_draft_linked(request, None, false)
    }
    fn save_editor_draft_linked(
        &mut self,
        request: &proto::WriteRequest,
        parent_link: Option<&proto::ParentLink>,
        prepared_proposal: bool,
    ) -> Result<EditorDraftRecord> {
        validate_request(request)?;
        if let Some(link) = parent_link {
            model::validate_parent_link(request, link)?;
        }
        let id = key(&request.card_id, &request.draft_id);
        // Historical delivery is read-only and never re-executes the write.
        if let Some(slot) = self.draft_history(&id, &request.operation_id)? {
            if !slot.active
                || slot.request.as_ref() != Some(request)
                || parent_link.is_some_and(|link| slot.parent_link.as_ref() != Some(link))
            {
                return Err("draft retry payload changed".into());
            }
            let current = self
                .draft_card(&request.card_id, &request.draft_id)?
                .ok_or("draft current record missing")?;
            self.retire_draft_imports(request);
            return Ok(EditorDraftRecord {
                slot,
                current_generation: current.1.generation,
                current_active: current.1.active,
                repeated: true,
            });
        }
        if self.unresolved_handoff_proposal_for_parent(&request.card_id, &request.draft_id)? {
            return Err("parent draft frozen by handoff proposal".into());
        }
        if let Some(link) = parent_link {
            if let Some(proposal) =
                self.proposal_for_parent(&request.card_id, &link.parent_draft_id)?
            {
                let exact = proposal.request.as_ref() == Some(request)
                    && proposal.parent_link.as_ref() == Some(link);
                if !prepared_proposal || !exact {
                    return Err("handoff requires its durable proposal".into());
                }
            } else if prepared_proposal {
                return Err("prepared handoff proposal missing".into());
            } else if self.handoff_proposal_seen_parent(&request.card_id, &link.parent_draft_id)? {
                return Err("parent handoff requires a new durable proposal".into());
            }
        }
        if !prepared_proposal && self.handoff_proposal_reserves_operation(&request.operation_id)? {
            return Err("draft operation reserved by handoff proposal".into());
        }
        let previous = self.draft_card(&request.card_id, &request.draft_id)?;
        if !prepared_proposal
            && previous.is_none()
            && self.handoff_proposal_reserves_child(&request.card_id, &request.draft_id)?
        {
            return Err("child identity reserved by durable handoff proposal".into());
        }
        if self
            .draft_successor(&request.card_id, &request.draft_id)?
            .is_some()
        {
            return Err("draft already has a durable successor".into());
        }
        if parent_link.is_some() && (previous.is_some() || request.expected_generation != 0) {
            return Err("handoff must create a new child identity".into());
        }
        let handoff = parent_link
            .map(|link| self.prepare_draft_handoff(request, link))
            .transpose()?;
        if parent_link.is_none() && request.assets.iter().any(|asset| asset.origin == 4) {
            return Err("parent pins require an explicit first handoff".into());
        }
        let inherited_link = parent_link.cloned().or_else(|| {
            previous
                .as_ref()
                .and_then(|(_, slot)| slot.parent_link.clone())
        });
        if let Some(link) = &inherited_link {
            model::validate_parent_link(request, link)?;
        }
        let generation = previous.as_ref().map_or(0, |(_, slot)| slot.generation);
        if request.expected_generation != generation {
            return Err("draft generation conflict".into());
        }
        let mut evidence_bytes = None;
        let (source, predecessor, evidence_pin) = if let Some((card, slot)) = &previous {
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
            (
                if original.source_kind == 0 {
                    Some(CardRecord::decode(&slot.source_card)?)
                } else {
                    None
                },
                if slot.predecessor_card.is_empty() {
                    None
                } else {
                    Some(CardRecord::decode(&slot.predecessor_card)?)
                },
                card.attachments().into_iter().find(|a| a.id == EVIDENCE),
            )
        } else if let Some((_, source)) = &handoff {
            (Some(source.clone()), None, None)
        } else if request.source_kind == 1 {
            if self.host.store_local().card(&request.card_id)?.is_some() {
                return Err("new-card draft target already exists".into());
            }
            (None, None, None)
        } else if !request.predecessor_operation.is_empty() {
            let projected = if let Some(projected) =
                self.load_editor_recovery(&request.card_id)?.filter(|p| {
                    p.operation_id() == request.predecessor_operation
                        && p.evidence().digest().as_slice() == request.predecessor_sha256
                }) {
                projected
            } else {
                let (commit, _) = self
                    .host
                    .store_local()
                    .operation_commit(&request.card_id, &request.predecessor_operation)?
                    .ok_or("draft predecessor is not a known editor proposal")?;
                let evidence = self
                    .host
                    .store_local()
                    .operation_evidence(&request.card_id, &request.predecessor_operation)?;
                if evidence.len() != 1
                    || evidence[0].digest().as_slice() != request.predecessor_sha256
                {
                    return Err("draft predecessor evidence mismatch".into());
                }
                captured_cards::verify_commit(&commit, &evidence[0])?
            };
            evidence_bytes = Some(projected.evidence().container().to_vec());
            (
                Some(CardRecord::decode(projected.source_card())?),
                Some(projected.card().clone()),
                None,
            )
        } else {
            let source = self
                .host
                .store_local()
                .card(&request.card_id)?
                .ok_or("draft source missing")?;
            (Some(source), None, None)
        };
        if let Some(source) = &source {
            crate::versioned_record::decode(source)?;
            if source.summary().id != request.card_id
                || source.summary().revision != request.source_revision
            {
                return Err("draft source revision conflict".into());
            }
        } else if request.source_kind != 1 {
            return Err("draft source missing".into());
        }
        // Clear the previous generation's consumption obligations before
        // replacing its markers. A failed cleanup leaves that generation and
        // all pending input intact; it cannot silently reactivate an import.
        if previous.is_some() {
            self.reconcile_draft_imports(&request.card_id, &request.draft_id)?;
        }
        let source_assets = source
            .as_ref()
            .map_or_else(Vec::new, CardRecord::attachments);
        let predecessor_assets = predecessor
            .as_ref()
            .map_or_else(Vec::new, |card| card.attachments());
        let mut stored = Vec::new();
        let mut consumed_imports = Vec::new();
        for (index, selection) in request.assets.iter().enumerate() {
            let selected = match selection.origin {
                0 => source_assets
                    .iter()
                    .find(|a| a.id == selection.asset_id)
                    .cloned(),
                1 => predecessor_assets
                    .iter()
                    .find(|a| a.id == selection.asset_id)
                    .cloned(),
                2 => {
                    if let Some((attachment, import_operation)) = self
                        .resolve_durable_draft_import(
                            &request.card_id,
                            &request.draft_id,
                            &selection.asset_id,
                            request.expected_generation,
                        )?
                    {
                        consumed_imports.push(import_operation);
                        Some(attachment)
                    } else {
                        // The older in-memory importer remains a separate
                        // compatibility path. Prior confirmed pins are also
                        // usable without reopening any source or import.
                        self.draft_staged
                            .get(&(
                                request.card_id.clone(),
                                request.draft_id.clone(),
                                selection.asset_id.clone(),
                            ))
                            .filter(|v| v.generation <= request.expected_generation)
                            .map(|v| v.attachment.clone())
                            .or(previous
                                .as_ref()
                                .and_then(|(_, slot)| {
                                    slot.assets.iter().find(|a| {
                                        a.selection
                                            .as_ref()
                                            .is_some_and(|s| s.asset_id == selection.asset_id)
                                    })
                                })
                                .map(attachment)
                                .transpose()?)
                    }
                }
                3 => previous
                    .as_ref()
                    .and_then(|(_, slot)| {
                        slot.assets.iter().find(|a| {
                            a.selection
                                .as_ref()
                                .is_some_and(|s| s.asset_id == selection.asset_id)
                        })
                    })
                    .map(attachment)
                    .transpose()?,
                4 => handoff
                    .as_ref()
                    .and_then(|(parent, _)| {
                        parent.assets.iter().find(|asset| {
                            asset
                                .selection
                                .as_ref()
                                .is_some_and(|item| item.asset_id == selection.asset_id)
                        })
                    })
                    .map(attachment)
                    .transpose()?,
                _ => None,
            }
            .ok_or("draft attachment does not belong to this source")?;
            stored.push(proto::StoredAsset {
                selection: Some(selection.clone()),
                pin_id: asset_pin(index),
                display_name: selected.display_name,
                media_type: selected.media_type,
                byte_length: selected.byte_length,
                sha256: selected.sha256.to_vec(),
            });
        }
        let evidence_len = evidence_bytes
            .as_ref()
            .map(|b| b.len() as u64)
            .or_else(|| evidence_pin.as_ref().map(|a| a.byte_length))
            .unwrap_or(0);
        let mut slot = proto::Slot {
            schema_version: 1,
            request: Some(request.clone()),
            generation: generation
                .checked_add(1)
                .ok_or("draft generation exhausted")?,
            active: true,
            source_card: source.map_or_else(Vec::new, |card| card.encode()),
            predecessor_card: predecessor.map_or_else(Vec::new, |c| c.encode()),
            predecessor_evidence_bytes: evidence_len,
            assets: stored,
            active_bytes: 0,
            consumed_imports,
            parent_link: inherited_link,
            retirement: None,
        };
        slot.active_bytes = charge(&slot)?;
        decode_body(&slot.encode_to_vec())?;
        let journals = self.all_draft_metadata()?;
        if previous.is_none() && journals.len() >= MAX_JOURNALS {
            return Err("draft journal identity capacity reached".into());
        }
        let others = journals
            .into_iter()
            .filter(|slot| {
                slot.active
                    && slot
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
        let mut pins = slot
            .assets
            .iter()
            .map(attachment)
            .collect::<Result<Vec<_>>>()?;
        if let Some(bytes) = evidence_bytes {
            self.host.prepare_write()?;
            let timestamp =
                crate::platform::unix_millis()?;
            let blob = self.host.store_local_mut().stage_blob(
                &mut Cursor::new(&bytes),
                bytes.len() as u64,
                Some(Sha256::digest(&bytes).into()),
                timestamp,
            )?;
            pins.push(Attachment {
                id: EVIDENCE.into(),
                display_name: "Draft predecessor evidence".into(),
                media_type: "application/vnd.morrow.task-evidence".into(),
                byte_length: blob.byte_length,
                sha256: blob.sha256,
            });
        } else if let Some(pin) = evidence_pin {
            pins.push(pin);
        }
        self.write_draft_journal(&id, &request.operation_id, generation, &slot, pins)?;
        self.retire_draft_imports(request);
        Ok(EditorDraftRecord {
            current_generation: slot.generation,
            current_active: slot.active,
            slot,
            repeated: false,
        })
    }
    pub(crate) fn discard_editor_draft(
        &mut self,
        card: &str,
        draft: &str,
        expected: u64,
        op: &str,
    ) -> Result<EditorDraftRecord> {
        identity(card, draft, op)?;
        let id = key(card, draft);
        if let Some(slot) = self.draft_history(&id, op)? {
            if slot.active
                || slot.retirement.is_some()
                || expected.checked_add(1) != Some(slot.generation)
            {
                return Err("draft discard retry changed".into());
            }
            let current = self
                .draft_card(card, draft)?
                .ok_or("draft record missing")?;
            self.draft_staged
                .retain(|(c, d, _), _| c != card || d != draft);
            return Ok(EditorDraftRecord {
                slot,
                current_generation: current.1.generation,
                current_active: current.1.active,
                repeated: true,
            });
        }
        if self.unresolved_handoff_proposal_for_parent(card, draft)? {
            return Err("parent draft frozen by handoff proposal".into());
        }
        if self.draft_successor(card, draft)?.is_some() {
            return Err("parent draft requires conditional retirement".into());
        }
        let (_, mut slot) = self.draft_card(card, draft)?.ok_or("draft missing")?;
        if !slot.active || slot.generation != expected {
            return Err("draft discard generation conflict".into());
        }
        if let Some(link) = &slot.parent_link {
            let (_, parent) = self
                .draft_card(card, &link.parent_draft_id)?
                .ok_or("linked parent missing")?;
            if parent.active {
                return Err("retire the linked parent before discarding its child".into());
            }
        }
        slot.generation = expected
            .checked_add(1)
            .ok_or("draft generation exhausted")?;
        slot.active = false;
        slot.active_bytes = 0;
        self.write_draft_journal(&id, op, expected, &slot, Vec::new())?;
        self.draft_staged
            .retain(|(c, d, _), _| c != card || d != draft);
        Ok(EditorDraftRecord {
            current_generation: slot.generation,
            current_active: slot.active,
            slot,
            repeated: false,
        })
    }
}
impl WorkbenchState {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn import_editor_draft_asset(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        name: &str,
        kind: &str,
        reader: &mut impl std::io::Read,
        size: u64,
    ) -> Result<morrow_workbench_plugin::Asset> {
        identity(card, draft, "draft-import")?;
        if self.draft_card(card, draft)?.is_none()
            && self.handoff_proposal_reserves_child(card, draft)?
        {
            return Err("child draft identity reserved by handoff proposal".into());
        }
        if self.unresolved_handoff_proposal_for_parent(card, draft)? {
            return Err("parent draft frozen by handoff proposal".into());
        }
        if self.draft_successor(card, draft)?.is_some() {
            return Err("parent draft is frozen after handoff".into());
        }
        let prior = self.draft_card(card, draft)?;
        if prior.as_ref().map_or(0, |(_, s)| s.generation) != generation
            || prior.as_ref().is_some_and(|(_, s)| !s.active)
        {
            return Err("draft import generation conflict".into());
        }
        if !prior.as_ref().is_some_and(|(_, slot)| {
            slot.request
                .as_ref()
                .is_some_and(|request| request.source_kind == 1)
        }) {
            let source = self
                .host
                .store_local()
                .card(card)?
                .ok_or("draft import source missing")?;
            crate::versioned_record::decode(&source)?;
        }
        if prior.is_none() && self.all_draft_metadata()?.len() >= MAX_JOURNALS {
            return Err("draft journal identity capacity reached".into());
        }
        let used = self
            .draft_staged
            .values()
            .try_fold(0u64, |n, a| n.checked_add(a.attachment.byte_length))
            .ok_or("draft staging count overflow")?;
        if self.draft_staged.len() >= MAX_STAGED_ASSETS
            || self
                .draft_staged
                .keys()
                .filter(|(c, d, _)| c == card && d == draft)
                .count()
                >= 20
            || used.checked_add(size).is_none_or(|n| n > MAX_ACTIVE_BYTES)
        {
            return Err("draft import staging capacity reached".into());
        }
        let probe = morrow_workbench_plugin::Idea {
            id: card.into(),
            title: "attachment".into(),
            category: "灵感".into(),
            stage: "待整理".into(),
            assets: vec![morrow_workbench_plugin::Asset {
                id: "pending".into(),
                name: name.into(),
                kind: kind.into(),
                bytes: size,
            }],
            ..Default::default()
        };
        probe.validate()?;
        self.prepare_write()?;
        let clock = crate::platform::unix_millis()?;
        let blob = self
            .host
            .store_local_mut()
            .stage_blob(reader, size, None, clock)?;
        let asset_id = format!("asset-{}", blob.id);
        let item = Attachment {
            id: asset_id.clone(),
            display_name: name.into(),
            media_type: match kind {
                "image" => "image/*",
                "gif" => "image/gif",
                "video" => "video/*",
                "audio" => "audio/*",
                _ => "application/octet-stream",
            }
            .into(),
            byte_length: size,
            sha256: blob.sha256,
        };
        let owner = (card.into(), draft.into(), asset_id.clone());
        if self
            .draft_staged
            .get(&owner)
            .is_some_and(|old| old.attachment != item)
        {
            return Err("draft imported attachment metadata changed".into());
        }
        self.draft_staged.insert(
            owner,
            DraftStagedAsset {
                generation,
                attachment: item,
            },
        );
        Ok(morrow_workbench_plugin::Asset {
            id: asset_id,
            name: name.into(),
            kind: kind.into(),
            bytes: size,
        })
    }
}
impl Workbench {
    /// Stage under one exact local draft owner. This is not durable until its
    /// selected blob is atomically pinned by save_editor_draft.
    #[allow(clippy::too_many_arguments)]
    pub fn import_editor_draft_asset(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        name: &str,
        kind: &str,
        reader: &mut impl std::io::Read,
        size: u64,
    ) -> Result<morrow_workbench_plugin::Asset> {
        self.local_state_mut()?
            .import_editor_draft_asset(card, draft, generation, name, kind, reader, size)
    }
    pub fn save_editor_draft(
        &mut self,
        request: &proto::WriteRequest,
    ) -> Result<EditorDraftRecord> {
        self.local_state_mut()?.save_editor_draft(request)
    }
    pub fn read_editor_draft(&self, card: &str, draft: &str) -> Result<Option<EditorDraftRecord>> {
        Ok(self
            .local_state()?
            .draft_card(card, draft)?
            .map(|(_, slot)| EditorDraftRecord {
                current_generation: slot.generation,
                current_active: slot.active,
                slot,
                repeated: false,
            }))
    }
    pub fn list_editor_drafts(&self) -> Result<Vec<EditorDraftSummary>> {
        Ok(self
            .local_state()?
            .all_draft_metadata()?
            .into_iter()
            .filter(|slot| slot.active)
            .map(|slot| {
                let request = slot.request.expect("validated request");
                EditorDraftSummary {
                    card_id: request.card_id,
                    draft_id: request.draft_id,
                    generation: slot.generation,
                    active: slot.active,
                }
            })
            .collect())
    }
    pub fn discard_editor_draft(
        &mut self,
        card: &str,
        draft: &str,
        expected: u64,
        operation: &str,
    ) -> Result<EditorDraftRecord> {
        self.local_state_mut()?
            .discard_editor_draft(card, draft, expected, operation)
    }
    pub fn export_editor_draft_asset(
        &self,
        card: &str,
        draft: &str,
        generation: u64,
        asset: &str,
    ) -> Result<Vec<u8>> {
        self.local_state()?
            .export_editor_draft_asset(card, draft, generation, asset)
    }
}
impl WorkbenchState {
    pub(crate) fn export_editor_draft_asset(
        &self,
        card: &str,
        draft: &str,
        generation: u64,
        asset: &str,
    ) -> Result<Vec<u8>> {
        let (journal, slot) = self.draft_card(card, draft)?.ok_or("draft missing")?;
        if !slot.active || slot.generation != generation {
            return Err("draft export generation conflict".into());
        }
        let pin = slot
            .assets
            .iter()
            .find(|a| a.selection.as_ref().is_some_and(|s| s.asset_id == asset))
            .ok_or("draft asset absent")?;
        let mut bytes = Vec::with_capacity(usize::try_from(pin.byte_length)?);
        let info = self.host.store_local().export_attachment_local(
            &journal.summary().id,
            &pin.pin_id,
            &mut bytes,
        )?;
        if info.byte_length != pin.byte_length
            || info.sha256.as_slice() != pin.sha256
            || bytes.len() as u64 != pin.byte_length
        {
            return Err("draft exported asset mismatch".into());
        }
        Ok(bytes)
    }
}
