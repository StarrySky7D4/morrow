//! Development-only raw recovery. Child pins commit before conditional parent
//! retirement. These are two transactions, not a cross-process atomic handoff.
use super::*;

pub(crate) fn validate_link(
    request: &proto::WriteRequest,
    link: &proto::DevelopmentForkLink,
) -> Result<()> {
    if link.schema_version != 1
        || link.parent_draft_id == request.draft_id
        || link.parent_generation == 0
        || link.parent_generation == u64::MAX
        || link.parent_request_sha256.len() != 32
        || link.parent_save_operation == link.child_operation
        || !request.predecessor_operation.is_empty()
        || !request.predecessor_sha256.is_empty()
    {
        return Err("InvalidDevelopmentDraftForkLink".into());
    }
    identity(
        &request.card_id,
        &link.parent_draft_id,
        &link.parent_save_operation,
    )?;
    identity(&request.card_id, &request.draft_id, &link.child_operation)?;
    Ok(())
}
pub fn validate_fork_request(
    request: &proto::WriteRequest,
    link: &proto::DevelopmentForkLink,
) -> Result<()> {
    validate_link(request, link)?;
    if request.expected_generation != 0
        || request.operation_id != link.child_operation
        || request.assets.iter().any(|s| s.origin != 4)
    {
        return Err("InvalidDevelopmentDraftForkRequest".into());
    }
    // Reuse every unchanged original shape/byte/alias rule on a validation-only
    // copy. Origin2 here is never stored, consumed or used as asset authority;
    // actual origin4 resolves solely against the exact audited parent pins.
    let mut shape = request.clone();
    for asset in &mut shape.assets {
        asset.origin = 2;
    }
    validate_request(&shape).map_err(err)
}
pub(crate) fn validate_slot_shape(slot: &proto::Slot) -> Result<()> {
    let request = slot.request.as_ref().ok_or("draft request missing")?;
    if slot.development_fork_link.is_none() && slot.development_business_link.is_none() && request.assets.iter().any(|s| s.origin == 4) {
        return Err("DraftForkPinWithoutLink".into());
    }
    if let Some(link) = &slot.development_fork_link {
        validate_link(request, link)?;
        if request.expected_generation > 0 && request.assets.iter().any(|s| s.origin == 4) {
            return Err("DraftForkPinsMustBeAdopted".into());
        }
        if request.expected_generation == 0 && !slot.consumed_imports.is_empty() {
            return Err("DraftForkCannotConsumeParentImports".into());
        }
    }
    if let Some(marker) = &slot.development_fork_retirement {
        let link = marker
            .fork_link
            .as_ref()
            .ok_or("DraftForkRetirementLinkMissing")?;
        if marker.schema_version != 1
            || slot.active
            || marker.child_draft_id == request.draft_id
            || link.parent_draft_id != request.draft_id
            || link.parent_generation.checked_add(1) != Some(slot.generation)
            || link.parent_save_operation != request.operation_id
            || link.parent_request_sha256 != Sha256::digest(request.encode_to_vec()).as_slice()
        {
            return Err("InvalidDevelopmentDraftForkRetirement".into());
        }
        let mut child = request.clone();
        child.draft_id = marker.child_draft_id.clone();
        validate_link(&child, link)?;
        identity(
            &request.card_id,
            &marker.child_draft_id,
            &marker.operation_id,
        )?;
        if marker.operation_id == link.child_operation
            || marker.operation_id == link.parent_save_operation
        {
            return Err("DraftForkRetirementOperationConflict".into());
        }
    }
    Ok(())
}
fn parent_history(
    host: &HostRuntime,
    request: &proto::WriteRequest,
    link: &proto::DevelopmentForkLink,
) -> Result<proto::Slot> {
    let parent = draft_history(
        host,
        &key(&request.card_id, &link.parent_draft_id),
        &link.parent_save_operation,
    )?
    .ok_or("DraftForkParentSaveMissing")?;
    let original = parent
        .request
        .as_ref()
        .ok_or("DraftForkParentRequestMissing")?;
    if !parent.active
        || parent.generation != link.parent_generation
        || original.card_id != request.card_id
        || original.draft_id != link.parent_draft_id
        || original.operation_id != link.parent_save_operation
        || Sha256::digest(original.encode_to_vec()).as_slice() != link.parent_request_sha256
        || original.source_kind != request.source_kind
        || original.source_revision != request.source_revision
    {
        return Err("DraftForkParentProofMismatch".into());
    }
    Ok(parent)
}
pub(crate) fn selected_parent_pin<'a>(
    parent: &'a proto::Slot,
    selection: &proto::AssetSelection,
) -> Result<&'a proto::StoredAsset> {
    if selection.origin != 4 {
        return Err("DraftForkParentPinOrigin".into());
    }
    let pin = parent
        .assets
        .iter()
        .find(|p| {
            p.selection
                .as_ref()
                .is_some_and(|s| s.asset_id == selection.asset_id)
        })
        .ok_or("DraftForkParentPinMissing")?;
    if pin
        .selection
        .as_ref()
        .is_none_or(|s| s.aliases != selection.aliases)
    {
        return Err("DraftForkParentAliasesChanged".into());
    }
    Ok(pin)
}
fn verify_first(host: &HostRuntime, slot: &proto::Slot) -> Result<proto::Slot> {
    let request = slot.request.as_ref().ok_or("DraftForkRequestMissing")?;
    let link = slot
        .development_fork_link
        .as_ref()
        .ok_or("DraftForkLinkMissing")?;
    let first = draft_history(
        host,
        &key(&request.card_id, &request.draft_id),
        &link.child_operation,
    )?
    .ok_or("DraftForkFirstSaveMissing")?;
    let original = first
        .request
        .as_ref()
        .ok_or("DraftForkFirstRequestMissing")?;
    if !first.active
        || first.generation != 1
        || original.expected_generation != 0
        || original.operation_id != link.child_operation
        || first.development_fork_link.as_ref() != Some(link)
        || original.card_id != request.card_id
        || original.draft_id != request.draft_id
        || original.source_kind != request.source_kind
        || original.source_revision != request.source_revision
        || first.source_card != slot.source_card
    {
        return Err("DraftForkFirstIdentityChanged".into());
    }
    let parent = parent_history(host, original, link)?;
    if first.source_card != parent.source_card {
        return Err("DraftForkFrozenSourceChanged".into());
    }
    let mut previous_index = None;
    for pin in &first.assets {
        let selected = pin.selection.as_ref().ok_or("DraftForkSelectionMissing")?;
        let inherited = selected_parent_pin(&parent, selected)?;
        let index = parent
            .assets
            .iter()
            .position(|p| std::ptr::eq(p, inherited))
            .expect("found parent pin");
        if previous_index.is_some_and(|previous| index <= previous) {
            return Err("DraftForkParentOrderChanged".into());
        }
        previous_index = Some(index);
        if pin.display_name != inherited.display_name
            || pin.media_type != inherited.media_type
            || pin.byte_length != inherited.byte_length
            || pin.sha256 != inherited.sha256
        {
            return Err("DraftForkPinMetadataChanged".into());
        }
    }
    Ok(first)
}
pub(crate) fn verify_slot(host: &HostRuntime, slot: &proto::Slot) -> Result<()> {
    crate::editor_handoff::verify_slot(host, slot)?;
    if slot.development_fork_link.is_some() {
        verify_first(host, slot)?;
    }
    if let Some(marker) = &slot.development_fork_retirement {
        let request = slot
            .request
            .as_ref()
            .ok_or("DraftForkRetirementRequestMissing")?;
        let child = draft_card(host, &request.card_id, &marker.child_draft_id)?
            .ok_or("DraftForkChildMissing")?;
        verify_first(host, &child)?;
        if child.development_fork_link.as_ref() != marker.fork_link.as_ref() {
            return Err("DraftForkRetirementProofChanged".into());
        }
        let history = draft_history(
            host,
            &key(&request.card_id, &request.draft_id),
            &marker.operation_id,
        )?
        .ok_or("DraftForkRetirementHistoryMissing")?;
        if history.development_fork_retirement.as_ref() != Some(marker) {
            return Err("DraftForkRetirementHistoryChanged".into());
        }
    }
    Ok(())
}
pub(crate) fn require_parent_retired(host: &HostRuntime, slot: &proto::Slot) -> Result<()> {
    let link = slot
        .development_fork_link
        .as_ref()
        .ok_or("DraftForkLinkMissing")?;
    let request = slot.request.as_ref().ok_or("DraftForkRequestMissing")?;
    let parent = draft_card(host, &request.card_id, &link.parent_draft_id)?
        .ok_or("DraftForkParentMissing")?;
    verify_slot(host, &parent)?;
    if parent.active
        || parent.development_fork_retirement.as_ref().is_none_or(|m| {
            m.child_draft_id != request.draft_id || m.fork_link.as_ref() != Some(link)
        })
    {
        return Err("DraftForkParentNotRetired".into());
    }
    Ok(())
}
pub(crate) fn successor(
    host: &HostRuntime,
    card: &str,
    parent: &str,
) -> Result<Option<proto::Slot>> {
    let mut found = None;
    for slot in all_draft_metadata(host)? {
        if slot.request.as_ref().is_some_and(|r| r.card_id == card)
            && slot
                .development_fork_link
                .as_ref()
                .is_some_and(|l| l.parent_draft_id == parent)
        {
            if found.is_some() {
                return Err("DraftForkAmbiguousSuccessors".into());
            }
            verify_first(host, &slot)?;
            found = Some(slot);
        }
    }
    Ok(found)
}
fn unresolved_imports(host: &HostRuntime, card: &str, draft: &str) -> Result<()> {
    for entry in crate::editor_draft_staging::list(host, card, draft).map_err(err)? {
        if entry.phase != crate::editor_draft_staging::DraftImportPhase::Retired {
            return Err("DraftForkUnselectedImports".into());
        }
    }
    Ok(())
}
pub fn fork_with_effect_at(
    host: &mut HostRuntime,
    request: &proto::WriteRequest,
    link: &proto::DevelopmentForkLink,
    mut clock: impl FnMut() -> u64,
    unix_ms: i64,
    effect: &mut &'static str,
) -> Result<DraftRecord> {
    *effect = "not_committed";
    validate_fork_request(request, link)?;
    if let Some(first) = draft_history(
        host,
        &key(&request.card_id, &request.draft_id),
        &request.operation_id,
    )? {
        *effect = "committed";
        if first.request.as_ref() != Some(request)
            || first.development_fork_link.as_ref() != Some(link)
        {
            return Err("DraftForkRetryPayloadChanged".into());
        }
        verify_first(host, &first)?;
        let current = draft_card(host, &request.card_id, &request.draft_id)?
            .ok_or("DraftForkCurrentMissing")?;
        verify_slot(host, &current)?;
        return Ok(DraftRecord {
            slot: first,
            current_generation: current.generation,
            current_active: current.active,
            repeated: true,
        });
    }
    super::require_mutable(host, &request.card_id, &link.parent_draft_id)?;
    if successor(host, &request.card_id, &link.parent_draft_id)?.is_some() {
        return Err("DraftForkParentFrozen".into());
    }
    let historical = parent_history(host, request, link)?;
    let parent = draft_card(host, &request.card_id, &link.parent_draft_id)?
        .ok_or("DraftForkParentMissing")?;
    verify_slot(host, &parent)?;
    if !parent.active || parent.generation != link.parent_generation || parent != historical {
        return Err("DraftForkParentGenerationConflict".into());
    }
    if parent.development_fork_link.is_some() { require_parent_retired(host, &parent)?; }
    if parent.development_business_link.is_some() { crate::editor_handoff::require_parent_retired(host, &parent)?; }
    // No loss of an independent unselected import owner; resolve it before
    // freezing the parent. Cleanup effects are separate from this fork write.
    crate::editor_draft_staging::reconcile(
        host,
        &request.card_id,
        &link.parent_draft_id,
        &mut clock,
        unix_ms,
        &mut "not_committed",
    )
    .map_err(err)?;
    unresolved_imports(host, &request.card_id, &link.parent_draft_id)?;
    let mut last = None;
    for selection in &request.assets {
        let pin = selected_parent_pin(&parent, selection)?;
        let index = parent
            .assets
            .iter()
            .position(|p| std::ptr::eq(p, pin))
            .unwrap();
        if last.is_some_and(|previous| index <= previous) {
            return Err("DraftForkParentOrderChanged".into());
        }
        last = Some(index);
    }
    save_internal(
        host,
        request,
        Some(link),
        Some(&parent),
        clock,
        unix_ms,
        effect,
    )
}
pub fn retire_fork_with_effect_at(
    host: &mut HostRuntime,
    card: &str,
    marker: &proto::DevelopmentForkRetirement,
    mut clock: impl FnMut() -> u64,
    unix_ms: i64,
    effect: &mut &'static str,
) -> Result<DraftRecord> {
    *effect = "not_committed";
    let link = marker
        .fork_link
        .as_ref()
        .ok_or("DraftForkRetirementLinkMissing")?;
    identity(card, &marker.child_draft_id, &marker.operation_id)?;
    let id = key(card, &link.parent_draft_id);
    if let Some(old) = draft_history(host, &id, &marker.operation_id)? {
        *effect = "committed";
        if old.active || old.development_fork_retirement.as_ref() != Some(marker) {
            return Err("DraftForkRetirementRetryChanged".into());
        }
        verify_slot(host, &old)?;
        let current =
            draft_card(host, card, &link.parent_draft_id)?.ok_or("DraftForkParentMissing")?;
        verify_slot(host, &current)?;
        crate::editor_draft_staging::reconcile(
            host,
            card,
            &link.parent_draft_id,
            &mut clock,
            unix_ms,
            &mut "not_committed",
        )
        .map_err(err)?;
        return Ok(DraftRecord {
            slot: old,
            current_generation: current.generation,
            current_active: current.active,
            repeated: true,
        });
    }
    if marker.schema_version != 1
        || marker.operation_id == link.child_operation
        || marker.operation_id == link.parent_save_operation
    {
        return Err("InvalidDevelopmentDraftForkRetirement".into());
    }
    let child = draft_card(host, card, &marker.child_draft_id)?.ok_or("DraftForkChildMissing")?;
    validate_link(
        child.request.as_ref().ok_or("DraftForkRequestMissing")?,
        link,
    )?;
    verify_first(host, &child)?;
    if !child.active || child.development_fork_link.as_ref() != Some(link) {
        return Err("DraftForkChildProofMismatch".into());
    }
    let successor =
        successor(host, card, &link.parent_draft_id)?.ok_or("DraftForkSuccessorMissing")?;
    if successor
        .request
        .as_ref()
        .is_none_or(|r| r.draft_id != marker.child_draft_id)
    {
        return Err("DraftForkSuccessorChanged".into());
    }
    let mut parent =
        draft_card(host, card, &link.parent_draft_id)?.ok_or("DraftForkParentMissing")?;
    let historical = parent_history(host, child.request.as_ref().unwrap(), link)?;
    if !parent.active || parent.generation != link.parent_generation || parent != historical {
        return Err("DraftForkRetirementGenerationConflict".into());
    }
    crate::editor_draft_staging::reconcile(
        host,
        card,
        &link.parent_draft_id,
        &mut clock,
        unix_ms,
        &mut "not_committed",
    )
    .map_err(err)?;
    unresolved_imports(host, card, &link.parent_draft_id)?;
    for pin in &child.assets {
        verify_pin(
            host,
            &key(card, &marker.child_draft_id),
            pin,
            &mut std::io::sink(),
        )?;
    }
    parent.generation = link
        .parent_generation
        .checked_add(1)
        .ok_or("draft generation exhausted")?;
    parent.active = false;
    parent.active_bytes = 0;
    parent.development_fork_retirement = Some(marker.clone());
    validate_slot_shape(&parent)?;
    write_draft_journal(
        host,
        &id,
        &marker.operation_id,
        link.parent_generation,
        &parent,
        &mut clock,
        effect,
    )?;
    crate::editor_draft_staging::reconcile(
        host,
        card,
        &link.parent_draft_id,
        &mut clock,
        unix_ms,
        &mut "not_committed",
    )
    .map_err(err)?;
    Ok(DraftRecord {
        current_generation: parent.generation,
        current_active: false,
        slot: parent,
        repeated: false,
    })
}

#[cfg(test)]
#[path = "editor_draft_fork_tests.rs"]
mod tests;
