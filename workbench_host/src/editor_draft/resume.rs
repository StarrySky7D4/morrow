//! Explicit fresh-session recovery of an uncommitted draft. Creating the child
//! and reserving the parent's original business operation are one audited
//! transaction. No old capture grant or uncertain business effect is replayed.
use super::*;

impl WorkbenchState {
    pub(super) fn resume_parent_history(
        &self, request: &proto::WriteRequest, link: &proto::ResumeLink,
    ) -> Result<proto::Slot> {
        model::validate_resume_link(request, link)?;
        let parent = self.draft_history(
            &key(&request.card_id, &link.parent_draft_id), &link.parent_save_operation,
        )?.ok_or("resume parent save was not committed")?;
        let original = parent.request.as_ref().ok_or("resume parent request missing")?;
        if !parent.active || parent.generation != link.parent_generation
            || original.card_id != request.card_id || original.draft_id != link.parent_draft_id
            || original.operation_id != link.parent_save_operation
            || original.commit_operation != link.parent_business_operation
            || Sha256::digest(original.encode_to_vec()).as_slice() != link.parent_request_sha256
            || original.source_kind != request.source_kind || original.source_revision != request.source_revision
            || original.predecessor_operation != request.predecessor_operation
            || original.predecessor_sha256 != request.predecessor_sha256
            || original.capture_scope == request.capture_scope
        { return Err("resume parent identity, baseline or capture boundary changed".into()); }
        Ok(parent)
    }

    pub(super) fn prepare_draft_resume(
        &mut self, request: &proto::WriteRequest, link: &proto::ResumeLink,
    ) -> Result<(proto::Slot, Option<CardRecord>)> {
        self.prepare_write()?;
        if request.expected_generation != 0 {
            return Err("resume must create a fresh journal".into());
        }
        let parent = self.resume_parent_history(request, link)?;
        let (_, current) = self.draft_card(&request.card_id, &link.parent_draft_id)?
            .ok_or("resume parent missing")?;
        if !current.active || current.generation != link.parent_generation || current.request != parent.request
            || self.draft_successor(&request.card_id, &link.parent_draft_id)?.is_some()
            || self.unresolved_handoff_proposal_for_parent(&request.card_id, &link.parent_draft_id)?
            || self.handoff_proposal_reserves_operation(&link.retirement_operation)?
        { return Err("resume parent changed or already has a successor".into()); }
        if let Some(ancestor) = draft_parent_id(&parent) {
            let (_, upstream) = self.draft_card(&request.card_id, ancestor)?.ok_or("resume ancestor missing")?;
            if upstream.active { return Err("retire the original ancestor before resuming its child".into()); }
        }
        let original = parent.request.as_ref().expect("validated parent");
        self.require_draft_capture_closed(&original.capture_scope)?;
        if !self.list_editor_draft_imports(&request.card_id, &link.parent_draft_id)?.is_empty()
            || self.draft_staged.keys().any(|(card, draft, _)| card == &request.card_id && draft == &link.parent_draft_id)
        { return Err("resolve original import resources before resuming".into()); }
        if self.editor_recovery(&request.card_id)?.is_some_and(|recovery| recovery.active) {
            return Err("resolve the original captured business proposal before resuming".into());
        }
        // Actual audited store state, never an exception/diagnostic interpreted
        // as absence. The child Create below atomically consumes the old ID.
        for operation in [&link.parent_business_operation, &link.retirement_operation, &request.commit_operation] {
            if !matches!(self.host.store_local().lookup(operation)?, Lookup::Absent) {
                return Err("resume requires unused original, retirement and new business operations".into());
            }
        }
        for slot in self.all_draft_metadata()? {
            let other = slot.request.as_ref().ok_or("draft metadata missing")?;
            if other.commit_operation == request.commit_operation || other.capture_scope == request.capture_scope
                || (other.commit_operation == link.parent_business_operation
                    && (other.card_id != request.card_id || other.draft_id != link.parent_draft_id))
            { return Err("resume operation or fresh capture belongs to another journal".into()); }
        }
        handoff::complete_snapshot(&parent, request)?;
        let source = self.host.store_local().card(&request.card_id)?;
        match (&source, request.source_kind) {
            (None, 1) if parent.source_card.is_empty() => {}
            (Some(card), 0) if card.encode() == parent.source_card => {}
            _ => return Err("business source changed; review the original draft before rebasing".into()),
        }
        Ok((parent, source))
    }

    pub(super) fn verify_resumed_draft_lineage(&self, slot: &proto::Slot) -> Result<()> {
        let request = slot.request.as_ref().ok_or("resumed draft request missing")?;
        let link = slot.resume_link.as_ref().ok_or("resume link missing")?;
        model::validate_resume_link(request, link)?;
        let initial = self.draft_history(&key(&request.card_id, &request.draft_id), &link.parent_business_operation)?
            .ok_or("original atomic resume fence missing")?;
        let first = initial.request.as_ref().ok_or("first resumed request missing")?;
        if !initial.active || initial.generation != 1 || first.expected_generation != 0
            || initial.parent_link.is_some() || initial.resume_link.as_ref() != Some(link)
            || first.operation_id != link.parent_business_operation
            || first.card_id != request.card_id || first.draft_id != request.draft_id
            || first.source_kind != request.source_kind || first.source_revision != request.source_revision
            || first.commit_operation != request.commit_operation || first.capture_scope != request.capture_scope
            || initial.source_card != slot.source_card
        { return Err("resumed child differs from its original audited fence".into()); }
        let parent = self.resume_parent_history(first, link)?;
        handoff::complete_snapshot(&parent, first)?;
        if initial.source_card != parent.source_card {
            return Err("resumed child changed the parent source snapshot".into());
        }
        for (child, original) in initial.assets.iter().zip(&parent.assets) {
            if child.display_name != original.display_name || child.media_type != original.media_type
                || child.byte_length != original.byte_length || child.sha256 != original.sha256
            { return Err("resumed child changed an original parent pin".into()); }
        }
        Ok(())
    }

    pub(crate) fn resume_editor_draft(
        &mut self, request: &proto::WriteRequest, link: &proto::ResumeLink,
    ) -> Result<EditorDraftRecord> {
        self.save_editor_draft_linked(request, None, Some(link), false)
    }

    pub(crate) fn retire_resumed_editor_draft_parent(
        &mut self, card: &str, child: &str, operation: &str,
    ) -> Result<EditorDraftRecord> {
        identity(card, child, operation)?;
        let (_, child_slot) = self.draft_card(card, child)?.ok_or("resumed child missing")?;
        let link = child_slot.resume_link.as_ref().ok_or("draft is not a resumed child")?;
        if operation != link.retirement_operation { return Err("resume retirement operation changed".into()); }
        let id = key(card, &link.parent_draft_id);
        let marker = proto::ParentRetirement {
            child_draft_id: child.into(), child_operation: link.parent_business_operation.clone(),
            operation_id: operation.into(), parent_generation: link.parent_generation,
        };
        if let Some(slot) = self.draft_history(&id, operation)? {
            if slot.active || slot.retirement.as_ref() != Some(&marker) {
                return Err("resume retirement retry changed".into());
            }
            let (_, current) = self.draft_card(card, &link.parent_draft_id)?.ok_or("resume parent missing")?;
            return Ok(EditorDraftRecord { slot, current_generation: current.generation,
                current_active: current.active, repeated: true });
        }
        if !child_slot.active { return Err("inactive resumed child cannot retire its parent".into()); }
        let successor = self.draft_successor(card, &link.parent_draft_id)?.ok_or("resume successor missing")?;
        if successor.request.as_ref().map(|r| r.draft_id.as_str()) != Some(child) {
            return Err("resume successor changed".into());
        }
        let (_, mut parent) = self.draft_card(card, &link.parent_draft_id)?.ok_or("resume parent missing")?;
        if !parent.active || parent.generation != link.parent_generation {
            return Err("resume parent retirement generation conflict".into());
        }
        parent.generation = link.parent_generation.checked_add(1).ok_or("resume parent generation exhausted")?;
        parent.active = false; parent.active_bytes = 0; parent.retirement = Some(marker);
        self.write_draft_journal(&id, operation, link.parent_generation, &parent, Vec::new())?;
        Ok(EditorDraftRecord { current_generation: parent.generation, current_active: false,
            slot: parent, repeated: false })
    }
}

impl Workbench {
    pub fn resume_editor_draft(&mut self, request: &proto::WriteRequest, link: &proto::ResumeLink) -> Result<EditorDraftRecord> {
        self.local_state_mut()?.resume_editor_draft(request, link)
    }
    pub fn retire_resumed_editor_draft_parent(&mut self, card: &str, child: &str, operation: &str) -> Result<EditorDraftRecord> {
        self.local_state_mut()?.retire_resumed_editor_draft_parent(card, child, operation)
    }
}
