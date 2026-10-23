//! Transfer already verified journal pins to this live editor's business
//! staging. Preview files and old external paths are never staging authority.
use super::*;

impl WorkbenchState {
    pub(crate) fn stage_editor_draft_assets(&mut self, card: &str, draft: &str,
        generation: u64, operation: &str, scope: &str) -> Result<EditorDraftRecord> {
        identity(card, draft, operation)?;
        self.prepare_write()?;
        let (_, slot) = self.draft_card(card, draft)?.ok_or("draft staging source missing")?;
        let request = slot.request.as_ref().ok_or("draft staging request missing")?;
        if !slot.active || slot.generation != generation || request.commit_operation != operation
            || request.capture_scope != scope || self.draft_successor(card, draft)?.is_some()
            || self.unresolved_handoff_proposal_for_parent(card, draft)?
            || !matches!(self.host.store_local().lookup(operation)?, Lookup::Absent) {
            return Err("draft business staging original identity changed".into());
        }
        if let Some(parent) = draft_parent_id(&slot) {
            if self.draft_card(card, parent)?.ok_or("draft staging ancestor missing")?.1.active {
                return Err("retire the original ancestor before business staging".into());
            }
        }
        match (self.host.store_local().card(card)?, request.source_kind) {
            (None, 1) if slot.source_card.is_empty() => {}
            (Some(source), 0) if source.encode() == slot.source_card => {}
            _ => return Err("business source changed before draft staging".into()),
        }
        self.require_draft_submission_capture(request)?;
        let mut pins = Vec::new();
        for stored in &slot.assets {
            let selected = stored.selection.as_ref().ok_or("draft staging selection missing")?;
            // Resolve and hash the actual durable blob before publishing any
            // staged entries. No partial set becomes visible after a failure.
            let bytes = self.export_editor_draft_asset(card, draft, generation, &selected.asset_id)?;
            if Sha256::digest(&bytes).as_slice() != stored.sha256 {
                return Err("draft business asset digest changed".into());
            }
            let mut pin = attachment(stored)?;
            pin.id = selected.asset_id.clone();
            if self.staged.get(&(card.into(), pin.id.clone())).is_some_and(|old|
                old.display_name != pin.display_name || old.media_type != pin.media_type ||
                old.byte_length != pin.byte_length || old.sha256 != pin.sha256) {
                return Err("draft asset collides with another business stage".into());
            }
            pins.push(pin);
        }
        for pin in pins { self.staged.insert((card.into(), pin.id.clone()), pin); }
        Ok(EditorDraftRecord { current_generation: generation, current_active: true, slot, repeated: false })
    }
}
impl Workbench {
    pub fn stage_editor_draft_assets(&mut self, card: &str, draft: &str, generation: u64,
        operation: &str, scope: &str) -> Result<EditorDraftRecord> {
        self.local_state_mut()?.stage_editor_draft_assets(card, draft, generation, operation, scope)
    }
}
