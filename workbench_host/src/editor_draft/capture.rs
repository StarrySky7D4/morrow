//! Durable archival evidence is data, never a revived scope or replay grant.
use super::*;
pub(super) const CAPTURE: &str = "draft-capture-archive";

pub(super) fn capture_attachment(pin: &proto::CapturePin) -> Result<Attachment> {
    Ok(Attachment {
        id: CAPTURE.into(), display_name: "Draft capture archive".into(),
        media_type: "application/vnd.morrow.draft-capture".into(),
        byte_length: pin.byte_length,
        sha256: pin.sha256.as_slice().try_into().map_err(|_| "draft capture digest")?,
    })
}

pub(super) fn validate_pin(slot: &proto::Slot) -> Result<()> {
    let request = slot.request.as_ref().ok_or("draft request missing")?;
    match &slot.capture {
        None if request.capture_scope.is_empty() && request.capture_events == 0 => Ok(()),
        Some(pin) if !request.capture_scope.is_empty()
            && pin.scope == request.capture_scope && pin.events == request.capture_events
            && pin.byte_length > 0 && pin.byte_length <= MAX_ACTIVE_BYTES
            && pin.sha256.len() == 32 => Ok(()),
        _ => Err("draft capture pin does not match original request".into()),
    }
}

fn validate_archive_part(raw: &[u8], slot: &proto::Slot) -> Result<proto::CaptureArchive> {
    let pin = slot.capture.as_ref().ok_or("draft capture missing")?;
    let request = slot.request.as_ref().ok_or("draft request missing")?;
    if raw.len() as u64 != pin.byte_length || Sha256::digest(raw).as_slice() != pin.sha256 {
        return Err("draft capture archive bytes changed".into());
    }
    let archive = proto::CaptureArchive::decode(raw)?;
    if archive.encode_to_vec() != raw || archive.version != 1
        || archive.scope != pin.scope || archive.card_id != request.card_id
        || archive.source_revision != request.source_revision
        || archive.applications.len() as u64 != pin.events
        || archive.observations.len() != archive.parents.len()
        || archive.observations.len() >= task_evidence::MAX_BATCH_OBSERVATIONS
    {
        return Err("draft capture archive identity changed".into());
    }
    morrow_core::plugin_package::Package::decode(&archive.package_archive)?;
    let expected = if request.source_kind == 1 {
        (1, Vec::new())
    } else {
        let source = CardRecord::decode(&slot.source_card)?;
        (source.summary().format_version, Sha256::digest(&slot.source_card).to_vec())
    };
    if (archive.source_format, archive.source_sha256.clone()) != expected {
        return Err("draft capture source identity changed".into());
    }
    for (index, (observation, parent)) in archive.observations.iter().zip(&archive.parents).enumerate() {
        let decoded = task_evidence::proto::Observation::decode(observation.as_slice())?;
        if decoded.encode_to_vec() != *observation {
            return Err("noncanonical archived observation".into());
        }
        crate::projection_v2::captured(&decoded)?;
        let decoded = crate::projection::proto::CaptureParent::decode(parent.as_slice())?;
        if decoded.encode_to_vec() != *parent || decoded.parent.is_some_and(|p| p as usize >= index) {
            return Err("invalid archived capture parent".into());
        }
    }
    let mut ids = std::collections::HashSet::new();
    for application in &archive.applications {
        let event = crate::projection::proto::PasteApplication::decode(application.as_slice())?;
        if event.encode_to_vec() != *application || !ids.insert(event.id.clone())
            || event.parts.iter().any(|part| part.observation.is_some_and(|i| i as usize >= archive.observations.len()))
        {
            return Err("invalid archived paste application".into());
        }
    }
    Ok(archive)
}

impl WorkbenchState {
    // Iterative traversal retains at most two archive buffers. Parent links and
    // original journal hashes are checked at every step, including retired
    // parents whose independent pins have already been released.
    fn validate_capture_lineage(&self, raw: &[u8], slot: &proto::Slot) -> Result<()> {
        let mut bytes = raw.to_vec();
        let mut current = slot.clone();
        for _ in 0..(MAX_JOURNALS + archive::MAX_ARCHIVED_JOURNALS) {
            let archive = validate_archive_part(&bytes, &current)?;
            let request = current.request.as_ref().ok_or("capture lineage request missing")?;
            let parent = if let Some(link) = &current.resume_link {
                Some(self.resume_parent_history(request, link)?)
            } else { current.parent_link.as_ref()
                .map(|link| self.parent_history(request, link)).transpose()? };
            if let Some(parent) = parent.filter(|parent| parent.capture.is_some()) {
                if archive.parent_archive.is_empty() {
                    return Err("parent capture archive missing from child".into());
                }
                bytes = archive.parent_archive;
                current = parent;
            } else {
                return if archive.parent_archive.is_empty() { Ok(()) }
                    else { Err("unexpected parent capture archive".into()) };
            }
        }
        Err("capture lineage identity limit".into())
    }

    pub(super) fn compose_draft_capture(&mut self, request: &proto::WriteRequest,
        parent_bytes: Vec<u8>) -> Result<Option<Vec<u8>>> {
        if request.capture_scope.is_empty() {
            if !parent_bytes.is_empty() { return Err("capture lineage cannot be omitted".into()); }
            return Ok(None);
        }
        let actual = self.archive_draft_capture(request)?;
        let mut archive = proto::CaptureArchive::decode(actual.as_slice())?;
        archive.parent_archive = parent_bytes;
        let bytes = archive.encode_to_vec();
        if bytes.len() as u64 > MAX_ACTIVE_BYTES { return Err("capture lineage exceeds active budget".into()); }
        Ok(Some(bytes))
    }

    pub(crate) fn export_editor_draft_capture(&self, card: &str, draft: &str, generation: u64) -> Result<Vec<u8>> {
        let (record, slot) = self.draft_card(card, draft)?.ok_or("draft missing")?;
        if slot.generation != generation { return Err("draft capture generation changed".into()); }
        self.read_draft_capture(&record, &slot)
    }

    pub(super) fn read_draft_capture(&self, card: &CardRecord, slot: &proto::Slot) -> Result<Vec<u8>> {
        if !slot.active { return Err("draft capture has retired".into()); }
        let pin = slot.capture.as_ref().ok_or("draft has no capture archive")?;
        let mut bytes = Vec::with_capacity(pin.byte_length as usize);
        self.host.store_local().export_attachment_local(&card.summary().id, CAPTURE, &mut bytes)?;
        self.validate_capture_lineage(&bytes, slot)?;
        Ok(bytes)
    }
}

impl Workbench {
    /// Export the exact durable bytes without reopening a scope or executing
    /// the archived package. The caller must select a current draft generation.
    pub fn export_editor_draft_capture(&self, card: &str, draft: &str, generation: u64) -> Result<Vec<u8>> {
        self.local_state()?.export_editor_draft_capture(card, draft, generation)
    }
}
