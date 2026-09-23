//! A durable, immutable first-child request and fixed conditional retirement.
//! The proposal card is metadata-only; all attachment bytes stay on audited drafts.
use super::*;
use morrow_core::content_change::ContentChange;

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.editor_draft_handoff_proposal.v1.rs"
    ));
}
const PREFIX: &str = "morrow-host-editor-handoff-proposal-";
const TYPE: &str = "org.morrow.host.editor-draft-handoff-proposal";
const TITLE: &str = "Editor draft handoff proposal";
const MAX_IDENTITIES: usize = 256;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffProposalStatus {
    Pending,
    ChildCommitted,
    ParentRetired,
    Cancelled,
    Conflict,
}
#[derive(Clone, Debug)]
pub struct HandoffProposalRecord {
    pub request: model::proto::WriteRequest,
    pub parent_link: model::proto::ParentLink,
    pub retirement_operation: String,
    pub status: HandoffProposalStatus,
    pub revision: u64,
    pub parent_generation: u64,
    pub parent_active: bool,
    pub child_generation: u64,
    pub child_active: bool,
    pub cursor: String,
}
fn proposal_key(operation: &str) -> String {
    let mut hash = Sha256::new();
    hash.update((operation.len() as u64).to_le_bytes());
    hash.update(operation.as_bytes());
    format!(
        "{PREFIX}{}",
        hash.finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
fn prepare_operation(operation: &str) -> String {
    let mut hash = Sha256::new();
    hash.update((operation.len() as u64).to_le_bytes());
    hash.update(operation.as_bytes());
    format!(
        "draft-handoff-prepare-{}",
        hash.finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
fn checked_shape(slot: &proto::Slot) -> Result<()> {
    if slot.schema_version != 1
        || !matches!(slot.revision, 1 | 2)
        || slot.cancelled != (slot.revision == 2)
    {
        return Err("invalid handoff proposal revision".into());
    }
    let request = slot
        .request
        .as_ref()
        .ok_or("handoff child request missing")?;
    let link = slot
        .parent_link
        .as_ref()
        .ok_or("handoff parent link missing")?;
    validate_request(request)?;
    model::validate_parent_link(request, link)?;
    if request.expected_generation != 0 || request.operation_id != link.child_operation {
        return Err("handoff proposal is not the first child operation".into());
    }
    identity(
        &request.card_id,
        &link.parent_draft_id,
        &slot.retirement_operation,
    )?;
    let prepare = prepare_operation(&request.operation_id);
    if slot.retirement_operation == request.operation_id
        || slot.retirement_operation == link.parent_save_operation
        || slot.retirement_operation == link.committed_operation
        || slot.retirement_operation == prepare
        || request.operation_id.starts_with("draft-handoff-prepare-")
        || slot
            .retirement_operation
            .starts_with("draft-handoff-prepare-")
    {
        return Err("handoff operation identity reused".into());
    }
    Ok(())
}
fn decode_body(raw: &[u8]) -> Result<proto::Slot> {
    if raw.len() > MAX_BODY_BYTES + 4096 {
        return Err("handoff proposal body limit".into());
    }
    let slot = proto::Slot::decode(raw)?;
    if slot.encode_to_vec() != raw {
        return Err("noncanonical handoff proposal".into());
    }
    checked_shape(&slot)?;
    Ok(slot)
}
fn decode_card(card: &CardRecord) -> Result<proto::Slot> {
    let summary = card.summary();
    if summary.type_id != TYPE
        || summary.format_version != 1
        || summary.title != TITLE
        || !card.attachments().is_empty()
    {
        return Err("unsupported handoff proposal journal".into());
    }
    let slot = decode_body(&card.body())?;
    let request = slot.request.as_ref().expect("validated");
    if summary.id != proposal_key(&request.operation_id) || summary.revision != slot.revision {
        return Err("handoff proposal journal identity changed".into());
    }
    Ok(slot)
}
impl WorkbenchState {
    fn proposal_slot(&self, operation: &str) -> Result<Option<proto::Slot>> {
        identity(operation, operation, operation)?;
        self.host
            .store_local()
            .card(&proposal_key(operation))?
            .map(|c| decode_card(&c))
            .transpose()
    }
    fn proposal_history(
        &self,
        operation: &str,
        commit_operation: &str,
    ) -> Result<Option<proto::Slot>> {
        let id = proposal_key(operation);
        let Some((commit, receipt)) = self
            .host
            .store_local()
            .operation_commit(&id, commit_operation)?
        else {
            return Ok(None);
        };
        let command = transaction::decode_command(&commit.command)?;
        let slot = match command.action {
            Some(transaction::proto::command::Action::CreateCard(raw)) => {
                decode_card(&CardRecord::decode(&raw)?)?
            }
            Some(transaction::proto::command::Action::SetContent(change)) => {
                if change.card_id != id
                    || change.title != TITLE
                    || !change.preview_text.is_empty()
                    || change.expected_revision.checked_add(1) != Some(receipt.revision)
                    || change
                        .attachments
                        .as_ref()
                        .is_none_or(|a| !a.items.is_empty())
                {
                    return Err("handoff proposal history command changed".into());
                }
                decode_body(&change.body)?
            }
            _ => return Err("operation is not a handoff proposal mutation".into()),
        };
        if slot.revision != receipt.revision
            || slot
                .request
                .as_ref()
                .is_none_or(|r| r.operation_id != operation)
            || (slot.revision == 1 && commit_operation != prepare_operation(operation))
            || (slot.revision == 2 && commit_operation != operation)
        {
            return Err("handoff proposal history binding changed".into());
        }
        Ok(Some(slot))
    }
    fn checked_proposal(&self, operation: &str) -> Result<Option<proto::Slot>> {
        let Some(slot) = self.proposal_slot(operation)? else {
            return Ok(None);
        };
        let original = self
            .proposal_history(operation, &prepare_operation(operation))?
            .ok_or("handoff proposal lacks audited Prepare")?;
        if original.revision != 1
            || original.cancelled
            || original.request != slot.request
            || original.parent_link != slot.parent_link
            || original.retirement_operation != slot.retirement_operation
        {
            return Err("handoff proposal changed its frozen request".into());
        }
        if slot.cancelled && self.proposal_history(operation, operation)?.as_ref() != Some(&slot) {
            return Err("handoff proposal cancellation history changed".into());
        }
        Ok(Some(slot))
    }
    fn all_proposals(&self) -> Result<Vec<(String, proto::Slot)>> {
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
                    .ok_or("handoff proposal disappeared")?;
                let slot = decode_card(&card)?;
                result.push((id.clone(), slot));
                if result.len() > MAX_IDENTITIES {
                    return Err("handoff proposal identity capacity changed".into());
                }
                after = id;
            }
            if count < 32 {
                break;
            }
        }
        Ok(result)
    }
    pub(super) fn proposal_for_parent(
        &self,
        card: &str,
        parent: &str,
    ) -> Result<Option<proto::Slot>> {
        let mut found = None;
        for (_, raw) in self.all_proposals()? {
            let request = raw.request.as_ref().ok_or("proposal request missing")?;
            let link = raw.parent_link.as_ref().ok_or("proposal link missing")?;
            if request.card_id == card && link.parent_draft_id == parent && !raw.cancelled {
                let slot = self
                    .checked_proposal(&request.operation_id)?
                    .ok_or("proposal disappeared")?;
                if found.replace(slot).is_some() {
                    return Err("ambiguous handoff proposals for parent".into());
                }
            }
        }
        Ok(found)
    }
    pub(crate) fn handoff_proposal_seen_parent(&self, card: &str, parent: &str) -> Result<bool> {
        Ok(self.all_proposals()?.into_iter().any(|(_, slot)| {
            slot.request.as_ref().is_some_and(|r| r.card_id == card)
                && slot
                    .parent_link
                    .as_ref()
                    .is_some_and(|link| link.parent_draft_id == parent)
        }))
    }
    pub(crate) fn unresolved_handoff_proposal_for_parent(
        &self,
        card: &str,
        parent: &str,
    ) -> Result<bool> {
        Ok(self.proposal_for_parent(card, parent)?.is_some())
    }
    pub(crate) fn handoff_proposal_reserves_operation(&self, operation: &str) -> Result<bool> {
        Ok(self.all_proposals()?.into_iter().any(|(_, slot)| {
            slot.request
                .as_ref()
                .is_some_and(|r| r.operation_id == operation)
                || slot.retirement_operation == operation
        }))
    }
    pub(crate) fn handoff_proposal_reserves_child(&self, card: &str, child: &str) -> Result<bool> {
        for (_, slot) in self.all_proposals()? {
            let request = slot.request.as_ref().ok_or("proposal request missing")?;
            if request.card_id == card && request.draft_id == child {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn write_handoff_proposal(
        &mut self,
        slot: &proto::Slot,
        previous: u64,
        operation: &str,
    ) -> Result<()> {
        let body = slot.encode_to_vec();
        decode_body(&body)?;
        if previous.checked_add(1) != Some(slot.revision) {
            return Err("handoff proposal revision changed".into());
        }
        self.host.prepare_write()?;
        let id = proposal_key(&slot.request.as_ref().expect("validated").operation_id);
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
                &id,
                tick.saturating_add(30_000),
                tick,
            )?;
            if previous == 0 {
                let card = CardRecord::new(&id, TYPE, 1, TITLE, body)?;
                self.host
                    .create_content(&connection, operation, &card, || now(start))?;
            } else {
                self.host.edit_content(
                    &connection,
                    &ContentChange {
                        operation_id: operation.into(),
                        card_id: id,
                        expected_revision: previous,
                        title: TITLE.into(),
                        body,
                        preview_text: String::new(),
                        attachments: Some(Vec::new()),
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
    fn handoff_proposal_record(&self, slot: &proto::Slot) -> Result<HandoffProposalRecord> {
        let request = slot.request.as_ref().ok_or("proposal request missing")?;
        let link = slot.parent_link.as_ref().ok_or("proposal link missing")?;
        let parent = self
            .draft_card(&request.card_id, &link.parent_draft_id)?
            .map(|(_, s)| s);
        let child = self
            .draft_card(&request.card_id, &request.draft_id)?
            .map(|(_, s)| s);
        let child_key = key(&request.card_id, &request.draft_id);
        let parent_key = key(&request.card_id, &link.parent_draft_id);
        let status = if slot.cancelled {
            // checked_proposal already proves the exact child operation was
            // atomically used for the audited cancellation revision.
            if child.is_none()
                && matches!(
                    self.host.store_local().lookup(&slot.retirement_operation)?,
                    Lookup::Absent
                )
            {
                HandoffProposalStatus::Cancelled
            } else {
                HandoffProposalStatus::Conflict
            }
        } else {
            let first = if matches!(
                self.host.store_local().lookup(&request.operation_id)?,
                Lookup::Absent
            ) {
                None
            } else {
                self.draft_history(&child_key, &request.operation_id)?
            };
            let matching_child = first.as_ref().is_some_and(|initial| {
                initial.generation == 1
                    && initial.request.as_ref() == Some(request)
                    && initial.parent_link.as_ref() == Some(link)
            }) && child.as_ref().is_some_and(|current| {
                current.generation >= 1
                    && current.parent_link.as_ref() == Some(link)
                    && current.request.as_ref().is_some_and(|r| {
                        r.card_id == request.card_id && r.draft_id == request.draft_id
                    })
            });
            let retirement_lookup = self.host.store_local().lookup(&slot.retirement_operation)?;
            let retired = if matches!(retirement_lookup, Lookup::Absent) {
                None
            } else {
                self.draft_history(&parent_key, &slot.retirement_operation)?
            };
            let matching_retirement = retired.as_ref().is_some_and(|history| {
                !history.active
                    && history.retirement.as_ref().is_some_and(|marker| {
                        marker.child_draft_id == request.draft_id
                            && marker.child_operation == request.operation_id
                            && marker.operation_id == slot.retirement_operation
                            && marker.parent_generation == link.parent_generation
                    })
            }) && parent.as_ref().is_some_and(|current| {
                !current.active
                    && current.generation == link.parent_generation + 1
                    && current.retirement == retired.as_ref().and_then(|s| s.retirement.clone())
            });
            let historical_parent = if matching_child {
                Some(self.parent_history(request, link)?)
            } else {
                None
            };
            let parent_ready = parent.as_ref().zip(historical_parent.as_ref()).is_some_and(
                |(current, historical)| {
                    current.active
                        && current.generation == link.parent_generation
                        && current.request == historical.request
                },
            );
            if matching_child && matching_retirement {
                HandoffProposalStatus::ParentRetired
            } else if matching_child
                && child.as_ref().is_some_and(|current| current.active)
                && parent_ready
                && retired.is_none()
                && matches!(retirement_lookup, Lookup::Absent)
            {
                HandoffProposalStatus::ChildCommitted
            } else if first.is_some()
                || child.is_some()
                || retired.is_some()
                || !matches!(retirement_lookup, Lookup::Absent)
            {
                HandoffProposalStatus::Conflict
            } else if self.prepare_draft_handoff(request, link).is_ok() {
                HandoffProposalStatus::Pending
            } else {
                HandoffProposalStatus::Conflict
            }
        };
        Ok(HandoffProposalRecord {
            request: request.clone(),
            parent_link: link.clone(),
            retirement_operation: slot.retirement_operation.clone(),
            status,
            revision: slot.revision,
            parent_generation: parent.as_ref().map_or(0, |s| s.generation),
            parent_active: parent.as_ref().is_some_and(|s| s.active),
            child_generation: child.as_ref().map_or(0, |s| s.generation),
            child_active: child.as_ref().is_some_and(|s| s.active),
            cursor: proposal_key(&request.operation_id),
        })
    }
    pub(crate) fn inspect_editor_draft_handoff_proposal(
        &self,
        card: &str,
        parent: &str,
        child_operation: &str,
    ) -> Result<Option<HandoffProposalRecord>> {
        identity(card, parent, child_operation)?;
        let Some(slot) = self.checked_proposal(child_operation)? else {
            return Ok(None);
        };
        let request = slot.request.as_ref().expect("validated");
        let link = slot.parent_link.as_ref().expect("validated");
        if request.card_id != card || link.parent_draft_id != parent {
            return Err("handoff proposal scope changed".into());
        }
        self.handoff_proposal_record(&slot).map(Some)
    }
    pub(crate) fn list_editor_draft_handoff_proposals(
        &self,
        cursor: &str,
        limit: usize,
    ) -> Result<(Vec<HandoffProposalRecord>, Option<String>)> {
        if limit == 0 || limit > 32 || (!cursor.is_empty() && !cursor.starts_with(PREFIX)) {
            return Err("handoff proposal page bounds".into());
        }
        let all = self.all_proposals()?;
        if !cursor.is_empty() {
            let (_, preceding) = all
                .iter()
                .find(|(key, _)| key == cursor)
                .ok_or("handoff proposal cursor missing")?;
            let operation = &preceding
                .request
                .as_ref()
                .ok_or("proposal request missing")?
                .operation_id;
            self.checked_proposal(operation)?
                .ok_or("handoff proposal cursor changed")?;
        }
        let mut values = Vec::new();
        for (key, raw) in all {
            if key.as_str() <= cursor {
                continue;
            }
            let request = raw.request.as_ref().ok_or("proposal request missing")?;
            let slot = self
                .checked_proposal(&request.operation_id)?
                .ok_or("proposal disappeared")?;
            values.push(self.handoff_proposal_record(&slot)?);
        }
        let next = if values.len() > limit {
            Some(values[limit - 1].cursor.clone())
        } else {
            None
        };
        values.truncate(limit);
        Ok((values, next))
    }
    pub(crate) fn prepare_editor_draft_handoff_proposal(
        &mut self,
        request: &model::proto::WriteRequest,
        link: &model::proto::ParentLink,
        retirement_operation: &str,
    ) -> Result<HandoffProposalRecord> {
        let candidate = proto::Slot {
            schema_version: 1,
            request: Some(request.clone()),
            parent_link: Some(link.clone()),
            retirement_operation: retirement_operation.into(),
            revision: 1,
            cancelled: false,
        };
        checked_shape(&candidate)?;
        if let Some(existing) = self.checked_proposal(&request.operation_id)? {
            if existing.request != candidate.request
                || existing.parent_link != candidate.parent_link
                || existing.retirement_operation != candidate.retirement_operation
            {
                return Err("handoff proposal retry changed".into());
            }
            return self.handoff_proposal_record(&existing);
        }
        if self
            .proposal_for_parent(&request.card_id, &link.parent_draft_id)?
            .is_some()
        {
            return Err("parent already has an unresolved handoff proposal".into());
        }
        if self.handoff_proposal_reserves_child(&request.card_id, &request.draft_id)?
            || self
                .draft_card(&request.card_id, &request.draft_id)?
                .is_some()
        {
            return Err("child identity already used".into());
        }
        if self.handoff_proposal_reserves_operation(&request.operation_id)?
            || self.handoff_proposal_reserves_operation(retirement_operation)?
            || !matches!(
                self.host.store_local().lookup(&request.operation_id)?,
                Lookup::Absent
            )
            || !matches!(
                self.host.store_local().lookup(retirement_operation)?,
                Lookup::Absent
            )
        {
            return Err("handoff operation already used".into());
        }
        let (parent, source) = self.prepare_draft_handoff(request, link)?;
        let metadata = self.all_draft_metadata()?;
        if metadata.len() >= MAX_JOURNALS
            || metadata.iter().filter(|s| s.active).count() >= MAX_SLOTS
        {
            return Err("draft handoff capacity reached".into());
        }
        let mut estimated = model::proto::Slot {
            schema_version: 1,
            request: Some(request.clone()),
            generation: 1,
            active: true,
            source_card: source.encode(),
            predecessor_card: Vec::new(),
            predecessor_evidence_bytes: 0,
            assets: Vec::new(),
            active_bytes: 0,
            consumed_imports: Vec::new(),
            parent_link: Some(link.clone()),
            retirement: None,
        };
        for (index, (selection, original)) in request.assets.iter().zip(&parent.assets).enumerate()
        {
            estimated.assets.push(model::proto::StoredAsset {
                selection: Some(selection.clone()),
                pin_id: asset_pin(index),
                display_name: original.display_name.clone(),
                media_type: original.media_type.clone(),
                byte_length: original.byte_length,
                sha256: original.sha256.clone(),
            });
        }
        let child_charge = charge(&estimated)?;
        estimated.active_bytes = child_charge;
        super::decode_body(&estimated.encode_to_vec())?;
        let active_total =
            metadata
                .iter()
                .filter(|s| s.active)
                .try_fold(child_charge, |sum, s| {
                    sum.checked_add(s.active_bytes)
                        .ok_or("draft active bytes overflow")
                })?;
        if active_total > MAX_ACTIVE_BYTES {
            return Err("draft active capacity reached".into());
        }
        let existing = self.all_proposals()?;
        if existing.len() >= MAX_IDENTITIES {
            return Err("handoff proposal identity capacity reached".into());
        }
        // Reserve space for every proposal's cancellation revision as well.
        let proposal_total =
            existing
                .iter()
                .try_fold(candidate.encoded_len() as u64 + 2, |n, (_, s)| {
                    n.checked_add(s.encoded_len() as u64 + u64::from(!s.cancelled) * 2)
                        .ok_or("proposal bytes overflow")
                })?;
        if proposal_total > MAX_TOTAL_BYTES {
            return Err("handoff proposal byte capacity reached".into());
        }
        self.write_handoff_proposal(&candidate, 0, &prepare_operation(&request.operation_id))?;
        let saved = self
            .checked_proposal(&request.operation_id)?
            .ok_or("prepared proposal missing")?;
        self.handoff_proposal_record(&saved)
    }
    pub(crate) fn complete_editor_draft_handoff_proposal(
        &mut self,
        card: &str,
        parent: &str,
        child_operation: &str,
    ) -> Result<HandoffProposalRecord> {
        let slot = self
            .checked_proposal(child_operation)?
            .ok_or("handoff proposal missing")?;
        let record = self
            .inspect_editor_draft_handoff_proposal(card, parent, child_operation)?
            .ok_or("handoff proposal missing")?;
        match record.status {
            HandoffProposalStatus::Pending => {
                self.save_editor_draft_linked(&record.request, Some(&record.parent_link), true)?;
                self.handoff_proposal_record(&slot)
            }
            HandoffProposalStatus::ChildCommitted | HandoffProposalStatus::ParentRetired => {
                Ok(record)
            }
            _ => Err("handoff proposal cannot complete".into()),
        }
    }
    pub(crate) fn retire_editor_draft_handoff_proposal(
        &mut self,
        card: &str,
        parent: &str,
        child_operation: &str,
    ) -> Result<HandoffProposalRecord> {
        let slot = self
            .checked_proposal(child_operation)?
            .ok_or("handoff proposal missing")?;
        let record = self
            .inspect_editor_draft_handoff_proposal(card, parent, child_operation)?
            .ok_or("handoff proposal missing")?;
        match record.status {
            HandoffProposalStatus::ChildCommitted => {
                self.retire_editor_draft_parent(
                    card,
                    &record.request.draft_id,
                    &record.retirement_operation,
                    true,
                )?;
                self.handoff_proposal_record(&slot)
            }
            HandoffProposalStatus::ParentRetired => Ok(record),
            _ => Err("handoff proposal cannot retire parent".into()),
        }
    }
    pub(crate) fn cancel_editor_draft_handoff_proposal(
        &mut self,
        card: &str,
        parent: &str,
        child_operation: &str,
    ) -> Result<HandoffProposalRecord> {
        let mut slot = self
            .checked_proposal(child_operation)?
            .ok_or("handoff proposal missing")?;
        let record = self
            .inspect_editor_draft_handoff_proposal(card, parent, child_operation)?
            .ok_or("handoff proposal missing")?;
        if record.status == HandoffProposalStatus::Cancelled {
            return Ok(record);
        }
        if record.status != HandoffProposalStatus::Pending
            && record.status != HandoffProposalStatus::Conflict
        {
            return Err("committed handoff proposal cannot be cancelled".into());
        }
        if !matches!(
            self.host.store_local().lookup(child_operation)?,
            Lookup::Absent
        ) || !matches!(
            self.host.store_local().lookup(&slot.retirement_operation)?,
            Lookup::Absent
        ) || self.draft_card(card, &record.request.draft_id)?.is_some()
        {
            return Err("handoff operation may already be committed".into());
        }
        slot.revision = 2;
        slot.cancelled = true;
        // This audited transaction claims the child operation in the core's
        // globally unique operation table. A late child write cannot revive it.
        self.write_handoff_proposal(&slot, 1, child_operation)?;
        let saved = self
            .checked_proposal(child_operation)?
            .ok_or("cancelled proposal missing")?;
        self.handoff_proposal_record(&saved)
    }
}
impl Workbench {
    pub fn prepare_editor_draft_handoff_proposal(
        &mut self,
        request: &model::proto::WriteRequest,
        parent: &model::proto::ParentLink,
        retirement_operation: &str,
    ) -> Result<HandoffProposalRecord> {
        self.local_state_mut()?
            .prepare_editor_draft_handoff_proposal(request, parent, retirement_operation)
    }
    pub fn inspect_editor_draft_handoff_proposal(
        &self,
        card: &str,
        parent: &str,
        child_operation: &str,
    ) -> Result<Option<HandoffProposalRecord>> {
        self.local_state()?
            .inspect_editor_draft_handoff_proposal(card, parent, child_operation)
    }
    pub fn list_editor_draft_handoff_proposals(
        &self,
        cursor: &str,
        limit: usize,
    ) -> Result<(Vec<HandoffProposalRecord>, Option<String>)> {
        self.local_state()?
            .list_editor_draft_handoff_proposals(cursor, limit)
    }
    pub fn complete_editor_draft_handoff_proposal(
        &mut self,
        card: &str,
        parent: &str,
        child_operation: &str,
    ) -> Result<HandoffProposalRecord> {
        self.local_state_mut()?
            .complete_editor_draft_handoff_proposal(card, parent, child_operation)
    }
    pub fn retire_editor_draft_handoff_proposal(
        &mut self,
        card: &str,
        parent: &str,
        child_operation: &str,
    ) -> Result<HandoffProposalRecord> {
        self.local_state_mut()?
            .retire_editor_draft_handoff_proposal(card, parent, child_operation)
    }
    pub fn cancel_editor_draft_handoff_proposal(
        &mut self,
        card: &str,
        parent: &str,
        child_operation: &str,
    ) -> Result<HandoffProposalRecord> {
        self.local_state_mut()?
            .cancel_editor_draft_handoff_proposal(card, parent, child_operation)
    }
}
