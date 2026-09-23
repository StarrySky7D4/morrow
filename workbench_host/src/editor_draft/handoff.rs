//! A child snapshot is durable before its parent may be conditionally retired.
//! This is not a cross-card atomic transaction: recovery may see both active.
use super::*;

#[derive(Clone, Debug)]
pub struct EditorDraftLineage {
    pub card_id: String,
    pub child_draft_id: String,
    pub child_generation: u64,
    pub child_active: bool,
    pub parent_generation: u64,
    pub parent_active: bool,
    pub link: proto::ParentLink,
    pub cursor: String,
}

fn complete_snapshot(parent: &proto::Slot, child: &proto::WriteRequest) -> Result<()> {
    let request = parent.request.as_ref().ok_or("parent request missing")?;
    if child.values != request.values || child.assets.len() != parent.assets.len() {
        return Err("handoff must preserve the complete parent snapshot".into());
    }
    for (selected, pinned) in child.assets.iter().zip(&parent.assets) {
        let original = pinned
            .selection
            .as_ref()
            .ok_or("parent selection missing")?;
        if selected.origin != 4
            || selected.asset_id != original.asset_id
            || selected.aliases != original.aliases
        {
            return Err("handoff must preserve parent attachment order and aliases".into());
        }
    }
    Ok(())
}

impl WorkbenchState {
    /// Include inactive children: discarding one never grants a second fork.
    pub(crate) fn draft_successor(&self, card: &str, parent: &str) -> Result<Option<proto::Slot>> {
        let mut found = None;
        for slot in self.all_draft_metadata()? {
            let request = slot.request.as_ref().ok_or("draft request missing")?;
            if request.card_id == card
                && slot
                    .parent_link
                    .as_ref()
                    .is_some_and(|link| link.parent_draft_id == parent)
            {
                if found.is_some() {
                    return Err("ambiguous durable draft successors".into());
                }
                self.verify_draft_lineage(&slot)?;
                found = Some(slot);
            }
        }
        Ok(found)
    }

    pub(super) fn parent_history(
        &self,
        request: &proto::WriteRequest,
        link: &proto::ParentLink,
    ) -> Result<proto::Slot> {
        model::validate_parent_link(request, link)?;
        let parent = self
            .draft_history(
                &key(&request.card_id, &link.parent_draft_id),
                &link.parent_save_operation,
            )?
            .ok_or("parent save was not committed")?;
        let original = parent.request.as_ref().ok_or("parent request missing")?;
        if !parent.active
            || parent.generation != link.parent_generation
            || original.card_id != request.card_id
            || original.draft_id != link.parent_draft_id
            || original.operation_id != link.parent_save_operation
            || Sha256::digest(original.encode_to_vec()).as_slice() != link.parent_request_sha256
        {
            return Err("parent save identity or digest changed".into());
        }
        Ok(parent)
    }

    pub(super) fn prepare_draft_handoff(
        &self,
        request: &proto::WriteRequest,
        link: &proto::ParentLink,
    ) -> Result<(proto::Slot, CardRecord)> {
        if request.expected_generation != 0 {
            return Err("handoff initial generation must be zero".into());
        }
        if self
            .draft_successor(&request.card_id, &link.parent_draft_id)?
            .is_some()
        {
            return Err("parent already has a durable child".into());
        }
        let parent = self.parent_history(request, link)?;
        let (_, current) = self
            .draft_card(&request.card_id, &link.parent_draft_id)?
            .ok_or("parent draft missing")?;
        if !current.active
            || current.generation != link.parent_generation
            || current.request != parent.request
        {
            return Err("parent draft changed before handoff".into());
        }
        if let Some(ancestor) = &current.parent_link {
            let (_, upstream) = self
                .draft_card(&request.card_id, &ancestor.parent_draft_id)?
                .ok_or("ancestor draft missing")?;
            if upstream.active {
                return Err("retire the ancestor before handing off its child".into());
            }
        }
        for imported in self.list_editor_draft_imports(&request.card_id, &link.parent_draft_id)? {
            if imported.phase != crate::editor_draft_staging::DraftImportPhase::Retired
                && !current
                    .consumed_imports
                    .contains(&imported.request.operation_id)
            {
                return Err("resolve unselected parent imports before handoff".into());
            }
        }
        complete_snapshot(&parent, request)?;
        let source = super::handoff_proof::committed_source(self, request, &parent, link)?;
        let canonical = self
            .host
            .store_local()
            .card(&request.card_id)?
            .ok_or("committed source missing")?;
        if canonical.encode() != source.encode() {
            return Err("business source changed before handoff".into());
        }
        Ok((parent, source))
    }

    /// Check the audited first child and exact parent, not just untrusted links.
    /// Neither a later business edit nor an inactive parent invalidates history.
    pub(super) fn verify_draft_lineage(&self, slot: &proto::Slot) -> Result<()> {
        let request = slot.request.as_ref().ok_or("child request missing")?;
        let link = slot
            .parent_link
            .as_ref()
            .ok_or("child parent link missing")?;
        let initial = self
            .draft_history(
                &key(&request.card_id, &request.draft_id),
                &link.child_operation,
            )?
            .ok_or("first child save missing")?;
        let first = initial
            .request
            .as_ref()
            .ok_or("first child request missing")?;
        if !initial.active
            || initial.generation != 1
            || first.expected_generation != 0
            || initial.parent_link.as_ref() != Some(link)
            || first.operation_id != link.child_operation
            || first.card_id != request.card_id
            || first.draft_id != request.draft_id
            || first.source_revision != request.source_revision
            || first.source_kind != request.source_kind
            || initial.source_card != slot.source_card
        {
            return Err("child lineage changed from first audited save".into());
        }
        let parent = self.parent_history(first, link)?;
        complete_snapshot(&parent, first)?;
        let source = super::handoff_proof::committed_source(self, first, &parent, link)?;
        if source.encode() != initial.source_card {
            return Err("child source proof changed".into());
        }
        for (child, original) in initial.assets.iter().zip(&parent.assets) {
            if child.display_name != original.display_name
                || child.media_type != original.media_type
                || child.byte_length != original.byte_length
                || child.sha256 != original.sha256
            {
                return Err("child inherited pin differs from exact parent".into());
            }
        }
        Ok(())
    }

    pub(crate) fn retire_editor_draft_parent(
        &mut self,
        card: &str,
        child: &str,
        operation: &str,
        prepared_proposal: bool,
    ) -> Result<EditorDraftRecord> {
        identity(card, child, operation)?;
        let (_, child_slot) = self.draft_card(card, child)?.ok_or("child draft missing")?;
        let link = child_slot
            .parent_link
            .as_ref()
            .ok_or("draft is not a handed-off child")?;
        let id = key(card, &link.parent_draft_id);
        let marker = proto::ParentRetirement {
            child_draft_id: child.into(),
            child_operation: link.child_operation.clone(),
            operation_id: operation.into(),
            parent_generation: link.parent_generation,
        };
        // Exact replay remains readable if the child was later discarded.
        if let Some(slot) = self.draft_history(&id, operation)? {
            if slot.retirement.as_ref() != Some(&marker) || slot.active {
                return Err("retirement retry changed".into());
            }
            let (_, current) = self
                .draft_card(card, &link.parent_draft_id)?
                .ok_or("parent missing")?;
            return Ok(EditorDraftRecord {
                slot,
                current_generation: current.generation,
                current_active: current.active,
                repeated: true,
            });
        }
        if !prepared_proposal && self.handoff_proposal_reserves_operation(operation)? {
            return Err("retirement operation reserved by handoff proposal".into());
        }
        if let Some(proposal) = self.proposal_for_parent(card, &link.parent_draft_id)? {
            let exact = proposal
                .request
                .as_ref()
                .is_some_and(|r| r.draft_id == child && r.operation_id == link.child_operation)
                && proposal.parent_link.as_ref() == Some(link)
                && proposal.retirement_operation == operation;
            if !prepared_proposal || !exact {
                return Err("parent retirement requires its durable proposal".into());
            }
        } else if prepared_proposal {
            return Err("prepared retirement proposal missing".into());
        }
        if !child_slot.active {
            return Err("inactive child cannot authorize parent retirement".into());
        }
        let successor = self
            .draft_successor(card, &link.parent_draft_id)?
            .ok_or("successor missing")?;
        if successor
            .request
            .as_ref()
            .map(|request| request.draft_id.as_str())
            != Some(child)
        {
            return Err("parent successor identity changed".into());
        }
        let (_, mut parent) = self
            .draft_card(card, &link.parent_draft_id)?
            .ok_or("parent missing")?;
        if !parent.active || parent.generation != link.parent_generation {
            return Err("parent retirement generation conflict".into());
        }
        parent.generation = link
            .parent_generation
            .checked_add(1)
            .ok_or("parent generation exhausted")?;
        parent.active = false;
        parent.active_bytes = 0;
        parent.retirement = Some(marker);
        self.write_draft_journal(&id, operation, link.parent_generation, &parent, Vec::new())?;
        self.draft_staged
            .retain(|(c, d, _), _| c != card || d != &link.parent_draft_id);
        Ok(EditorDraftRecord {
            current_generation: parent.generation,
            current_active: false,
            slot: parent,
            repeated: false,
        })
    }
}

impl Workbench {
    pub fn handoff_editor_draft(
        &mut self,
        request: &proto::WriteRequest,
        parent: &proto::ParentLink,
    ) -> Result<EditorDraftRecord> {
        self.local_state_mut()?
            .save_editor_draft_linked(request, Some(parent), false)
    }
    pub fn retire_editor_draft_parent(
        &mut self,
        card: &str,
        child: &str,
        operation: &str,
    ) -> Result<EditorDraftRecord> {
        self.local_state_mut()?
            .retire_editor_draft_parent(card, child, operation, false)
    }
    pub fn list_editor_draft_lineages(&self) -> Result<Vec<EditorDraftLineage>> {
        self.local_state()?.list_editor_draft_lineages()
    }
}

impl WorkbenchState {
    pub(crate) fn handoff_editor_draft(
        &mut self,
        request: &proto::WriteRequest,
        parent: &proto::ParentLink,
    ) -> Result<EditorDraftRecord> {
        self.save_editor_draft_linked(request, Some(parent), false)
    }
    /// Read-only discovery includes inactive parents/children and never retries.
    pub(crate) fn list_editor_draft_lineages(&self) -> Result<Vec<EditorDraftLineage>> {
        let mut result = Vec::new();
        let mut parents = std::collections::HashSet::new();
        for slot in self.all_draft_metadata()? {
            let Some(link) = &slot.parent_link else {
                continue;
            };
            self.verify_draft_lineage(&slot)?;
            let request = slot.request.as_ref().ok_or("child request missing")?;
            if !parents.insert((request.card_id.clone(), link.parent_draft_id.clone())) {
                return Err("ambiguous durable successors".into());
            }
            let (_, parent) = self
                .draft_card(&request.card_id, &link.parent_draft_id)?
                .ok_or("parent draft missing")?;
            result.push(EditorDraftLineage {
                card_id: request.card_id.clone(),
                child_draft_id: request.draft_id.clone(),
                child_generation: slot.generation,
                child_active: slot.active,
                parent_generation: parent.generation,
                parent_active: parent.active,
                link: link.clone(),
                cursor: key(&request.card_id, &request.draft_id),
            });
        }
        Ok(result)
    }
}
