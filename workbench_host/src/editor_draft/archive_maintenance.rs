//! Explicit accounting of auxiliary journals belonging to a legacy v1 archive.
//! The original archive and every journal remain immutable. A separate durable
//! command binds their exact bytes; merely reading old data never grants credit.
use super::*;

#[cfg(all(test, target_os = "windows"))]
#[path = "archive_maintenance_tests.rs"]
mod tests;

const MAINTENANCE_PREFIX: &str = "morrow-host-editor-maintenance-";
const MAINTENANCE_TYPE: &str = "org.morrow.host.editor-history-maintenance";
const MAINTENANCE_TITLE: &str = "Archived editor auxiliary journals";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorDraftArchiveMaintenanceRequest {
    pub card_id: String,
    pub draft_id: String,
    pub generation: u64,
    pub request_sha256: Vec<u8>,
    pub archive_operation: String,
    pub archive_sha256: Vec<u8>,
    pub operation: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorDraftArchiveMaintenance {
    pub request: EditorDraftArchiveMaintenanceRequest,
    pub auxiliary_count: u32,
    pub auxiliary_bytes: u64,
    pub auxiliary_sha256: Vec<u8>,
    pub repeated: bool,
}

#[derive(Clone, PartialEq, Message)]
pub(super) struct MaintenanceMarker {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(string, tag = "2")]
    card_id: String,
    #[prost(string, tag = "3")]
    draft_id: String,
    #[prost(uint64, tag = "4")]
    generation: u64,
    #[prost(bytes = "vec", tag = "5")]
    request_sha256: Vec<u8>,
    #[prost(string, tag = "6")]
    archive_operation: String,
    #[prost(bytes = "vec", tag = "7")]
    archive_sha256: Vec<u8>,
    #[prost(string, tag = "8")]
    operation: String,
    #[prost(uint32, tag = "9")]
    pub(super) auxiliary_count: u32,
    #[prost(uint64, tag = "10")]
    pub(super) auxiliary_bytes: u64,
    #[prost(bytes = "vec", tag = "11")]
    auxiliary_sha256: Vec<u8>,
}

fn maintenance_key(card: &str, draft: &str) -> String {
    key(card, draft).replacen(PREFIX, MAINTENANCE_PREFIX, 1)
}

fn maintenance_receipt(
    marker: &MaintenanceMarker,
    repeated: bool,
) -> EditorDraftArchiveMaintenance {
    EditorDraftArchiveMaintenance {
        request: EditorDraftArchiveMaintenanceRequest {
            card_id: marker.card_id.clone(),
            draft_id: marker.draft_id.clone(),
            generation: marker.generation,
            request_sha256: marker.request_sha256.clone(),
            archive_operation: marker.archive_operation.clone(),
            archive_sha256: marker.archive_sha256.clone(),
            operation: marker.operation.clone(),
        },
        auxiliary_count: marker.auxiliary_count,
        auxiliary_bytes: marker.auxiliary_bytes,
        auxiliary_sha256: marker.auxiliary_sha256.clone(),
        repeated,
    }
}

impl WorkbenchState {
    fn maintenance_marker(
        &self,
        source: &CardRecord,
        original: &CardRecord,
        archive: &Marker,
        auxiliary: &Inventory,
    ) -> Result<MaintenanceMarker> {
        let raw = source.body();
        if raw.len() > 2048 {
            return Err("draft maintenance marker budget".into());
        }
        let marker = MaintenanceMarker::decode(raw.as_slice())?;
        identity(&marker.card_id, &marker.draft_id, &marker.operation)?;
        let summary = source.summary();
        if marker.version != 1
            || marker.encode_to_vec() != raw
            || summary.id != maintenance_key(&marker.card_id, &marker.draft_id)
            || summary.revision != 1
            || summary.format_version != 1
            || summary.type_id != MAINTENANCE_TYPE
            || summary.title != MAINTENANCE_TITLE
            || !summary.preview_text.is_empty()
            || !source.attachments().is_empty()
            || archive.version != 1
            || marker.card_id != archive.card_id
            || marker.draft_id != archive.draft_id
            || marker.generation != archive.generation
            || marker.request_sha256 != archive.request_sha256
            || marker.archive_operation != archive.operation
            || marker.archive_sha256 != Sha256::digest(original.encode()).as_slice()
            || marker.operation.is_empty()
            || marker.operation == archive.operation
            || marker.auxiliary_count == 0
        {
            return Err("invalid immutable draft maintenance marker".into());
        }
        let binding = auxiliary.binding(&marker.card_id, &marker.draft_id)?;
        if marker.auxiliary_count != binding.count
            || marker.auxiliary_bytes != binding.bytes
            || marker.auxiliary_sha256 != binding.digest
        {
            return Err("maintained auxiliary journals changed".into());
        }
        let (commit, _) = self
            .host
            .store_local()
            .operation_commit(&summary.id, &marker.operation)?
            .ok_or("draft maintenance operation missing")?;
        let command = transaction::decode_command(&commit.command)?;
        if !matches!(command.action,
            Some(transaction::proto::command::Action::CreateCard(ref bytes)) if *bytes == source.encode())
        {
            return Err("draft maintenance operation changed".into());
        }
        Ok(marker)
    }

    pub(super) fn draft_archive_maintenance_inventory(
        &self,
        auxiliary: &Inventory,
        archives: &BTreeMap<String, Marker>,
    ) -> Result<BTreeMap<String, (MaintenanceMarker, u64)>> {
        let mut after = MAINTENANCE_PREFIX.to_owned();
        let mut result = BTreeMap::new();
        'pages: loop {
            let ids = self.host.store_local().card_ids_local(&after, 32)?;
            let count = ids.len();
            for id in ids {
                if !id.starts_with(MAINTENANCE_PREFIX) {
                    break 'pages;
                }
                let source = self
                    .host
                    .store_local()
                    .card(&id)?
                    .ok_or("maintenance disappeared")?;
                // Bound before decoding. No orphan or malformed marker can be
                // silently omitted from the finite retained-storage inventory.
                let raw = source.body();
                if raw.len() > 2048 {
                    return Err("draft maintenance marker budget".into());
                }
                let raw_marker = MaintenanceMarker::decode(raw.as_slice())?;
                let main_key = key(&raw_marker.card_id, &raw_marker.draft_id);
                let archive = archives
                    .get(&main_key)
                    .ok_or("maintenance original archive missing")?;
                let original = self
                    .host
                    .store_local()
                    .card(&archive_key(&archive.card_id, &archive.draft_id))?
                    .ok_or("maintenance original archive disappeared")?;
                let marker = self.maintenance_marker(&source, &original, archive, auxiliary)?;
                if result
                    .insert(main_key, (marker, source.encode().len() as u64))
                    .is_some()
                    || result.len() > MAX_ARCHIVED_JOURNALS
                {
                    return Err("draft maintenance inventory limit".into());
                }
                after = id;
            }
            if count < 32 {
                break;
            }
        }
        Ok(result)
    }

    pub(crate) fn inspect_editor_draft_archive_maintenance(
        &self,
        card: &str,
        draft: &str,
    ) -> Result<Option<EditorDraftArchiveMaintenance>> {
        identity(card, draft, "draft-maintenance-read")?;
        let Some(source) = self
            .host
            .store_local()
            .card(&maintenance_key(card, draft))?
        else {
            return Ok(None);
        };
        let auxiliary = self.draft_archive_auxiliary()?;
        let original = self
            .host
            .store_local()
            .card(&archive_key(card, draft))?
            .ok_or("maintenance original archive missing")?;
        let archive = self.archive_marker(&original, &auxiliary)?;
        let marker = self.maintenance_marker(&source, &original, &archive, &auxiliary)?;
        Ok(Some(maintenance_receipt(&marker, true)))
    }

    pub(crate) fn maintain_editor_draft_archive(
        &mut self,
        request: &EditorDraftArchiveMaintenanceRequest,
    ) -> Result<EditorDraftArchiveMaintenance> {
        let card = request.card_id.as_str();
        let draft = request.draft_id.as_str();
        identity(card, draft, &request.operation)?;
        if request.generation == 0
            || request.request_sha256.len() != 32
            || request.archive_sha256.len() != 32
            || request.operation.is_empty()
            || request.archive_operation.is_empty()
            || request.operation == request.archive_operation
        {
            return Err("invalid draft archive maintenance request".into());
        }
        if let Some(previous) = self.inspect_editor_draft_archive_maintenance(card, draft)? {
            if previous.request != *request {
                return Err("draft maintenance retry changed".into());
            }
            return Ok(previous);
        }
        if !matches!(
            self.host.store_local().lookup(&request.operation)?,
            Lookup::Absent
        ) {
            return Err("draft maintenance operation already used".into());
        }
        let auxiliary = self.draft_archive_auxiliary()?;
        let original = self
            .host
            .store_local()
            .card(&archive_key(card, draft))?
            .ok_or("maintenance requires original archive")?;
        let archive = self.archive_marker(&original, &auxiliary)?;
        if archive.version != 1
            || archive.generation != request.generation
            || archive.request_sha256 != request.request_sha256
            || archive.operation != request.archive_operation
            || Sha256::digest(original.encode()).as_slice() != request.archive_sha256
        {
            return Err("maintenance changed original version 1 archive".into());
        }
        let binding = auxiliary.binding(card, draft)?;
        if binding.count == 0 {
            return Err("no legacy auxiliary journals to maintain".into());
        }
        let inventory = self.draft_archive_inventory_with_auxiliary(&auxiliary)?;
        if inventory.covered_auxiliary.contains(&key(card, draft)) {
            return Err("auxiliary journals already accounted".into());
        }
        let marker = MaintenanceMarker {
            version: 1,
            card_id: card.into(),
            draft_id: draft.into(),
            generation: request.generation,
            request_sha256: request.request_sha256.clone(),
            archive_operation: request.archive_operation.clone(),
            archive_sha256: request.archive_sha256.clone(),
            operation: request.operation.clone(),
            auxiliary_count: binding.count,
            auxiliary_bytes: binding.bytes,
            auxiliary_sha256: binding.digest,
        };
        let id = maintenance_key(card, draft);
        let source = CardRecord::new_with_attachments(
            &id,
            MAINTENANCE_TYPE,
            1,
            MAINTENANCE_TITLE,
            marker.encode_to_vec(),
            &[],
        )?;
        if inventory
            .auxiliary_count
            .checked_add(marker.auxiliary_count as usize)
            .is_none_or(|n| n > MAX_ARCHIVED_AUXILIARY)
            || inventory
                .bytes
                .checked_add(marker.auxiliary_bytes)
                .and_then(|n| n.checked_add(source.encode().len() as u64))
                .is_none_or(|n| n > MAX_ARCHIVED_BYTES)
        {
            return Err(
                "draft archive capacity reached; preserve original legacy accounting".into(),
            );
        }
        self.host.prepare_write()?;
        let mut connection = self.host.connect()?;
        let start = self.start;
        let result = (|| -> Result<()> {
            let tick = now(start);
            self.host.grant(
                &mut connection,
                GrantKind::CreateContent,
                &id,
                tick.saturating_add(30_000),
                tick,
            )?;
            self.host
                .create_content(&connection, &request.operation, &source, || now(start))?;
            Ok(())
        })();
        let disconnected = self.host.disconnect(&connection);
        result?;
        disconnected?;
        Ok(maintenance_receipt(&marker, false))
    }
}

impl Workbench {
    pub fn maintain_editor_draft_archive(
        &mut self,
        request: &EditorDraftArchiveMaintenanceRequest,
    ) -> Result<EditorDraftArchiveMaintenance> {
        self.local_state_mut()?
            .maintain_editor_draft_archive(request)
    }
    pub fn inspect_editor_draft_archive_maintenance(
        &self,
        card: &str,
        draft: &str,
    ) -> Result<Option<EditorDraftArchiveMaintenance>> {
        self.local_state()?
            .inspect_editor_draft_archive_maintenance(card, draft)
    }
}
