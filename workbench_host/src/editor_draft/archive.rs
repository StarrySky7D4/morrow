//! Explicit maintenance of retired journals. Archiving changes admission
//! accounting only; the original journal, operation receipts and lineage stay
//! present and cannot be reused. It neither restores released blobs nor grants
//! capture/business authority. Archive storage is separately bounded.
use super::archive_auxiliary::{Inventory, Kind, MAX_ARCHIVED_AUXILIARY};
use super::*;
#[path = "archive_maintenance.rs"]
mod maintenance;
pub use maintenance::{EditorDraftArchiveMaintenance, EditorDraftArchiveMaintenanceRequest};
use std::collections::{BTreeMap, BTreeSet};

const ARCHIVE_PREFIX: &str = "morrow-host-editor-history-";
const ARCHIVE_TYPE: &str = "org.morrow.host.editor-history";
const ARCHIVE_TITLE: &str = "Archived editor draft";
pub(super) const MAX_ARCHIVED_JOURNALS: usize = 4096;
const MAX_ARCHIVED_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, PartialEq, Message)]
struct Marker {
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
    #[prost(bytes = "vec", tag = "6")]
    slot_sha256: Vec<u8>,
    #[prost(string, tag = "7")]
    operation: String,
    #[prost(uint64, tag = "8")]
    retained_bytes: u64,
    #[prost(uint32, tag = "9")]
    auxiliary_count: u32,
    #[prost(bytes = "vec", tag = "10")]
    auxiliary_sha256: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorDraftArchive {
    pub version: u32,
    pub legacy_auxiliary_count: u32,
    pub archive_sha256: Vec<u8>,
    pub card_id: String,
    pub draft_id: String,
    pub generation: u64,
    pub request_sha256: Vec<u8>,
    pub slot_sha256: Vec<u8>,
    pub operation: String,
    pub repeated: bool,
}

#[derive(Clone, Debug)]
pub struct EditorDraftHistoryRow {
    pub card_id: String,
    pub draft_id: String,
    pub generation: u64,
    pub request_sha256: Vec<u8>,
    pub archive_operation: String,
    pub cursor: String,
    pub title: String,
    pub preview: String,
}

#[derive(Clone, Debug)]
pub struct EditorDraftHistoryPage {
    pub rows: Vec<EditorDraftHistoryRow>,
    pub next_cursor: String,
    pub unarchived: u32,
    pub archived: u32,
    pub archived_bytes: u64,
}

fn archive_key(card: &str, draft: &str) -> String {
    key(card, draft).replacen(PREFIX, ARCHIVE_PREFIX, 1)
}
fn receipt(
    marker: &Marker,
    source: &CardRecord,
    auxiliary_count: u32,
    repeated: bool,
) -> EditorDraftArchive {
    EditorDraftArchive {
        version: marker.version,
        legacy_auxiliary_count: if marker.version == 1 {
            auxiliary_count
        } else {
            0
        },
        archive_sha256: Sha256::digest(source.encode()).to_vec(),
        card_id: marker.card_id.clone(),
        draft_id: marker.draft_id.clone(),
        generation: marker.generation,
        request_sha256: marker.request_sha256.clone(),
        slot_sha256: marker.slot_sha256.clone(),
        operation: marker.operation.clone(),
        repeated,
    }
}
struct ArchiveInventory {
    markers: BTreeMap<String, Marker>,
    bytes: u64,
    auxiliary_count: usize,
    covered_auxiliary: BTreeSet<String>,
}

fn cursor_valid(cursor: &str) -> bool {
    cursor.is_empty()
        || cursor.strip_prefix(PREFIX).is_some_and(|suffix| {
            suffix.len() == 64
                && suffix
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

impl WorkbenchState {
    fn require_draft_archive_resources_closed(
        &self,
        slot: &proto::Slot,
        auxiliary: &Inventory,
    ) -> Result<()> {
        let request = slot.request.as_ref().ok_or("draft request missing")?;
        let card = request.card_id.as_str();
        let draft = request.draft_id.as_str();
        self.require_draft_capture_closed(&request.capture_scope)?;
        if self
            .draft_staged
            .keys()
            .any(|(c, d, _)| c == card && d == draft)
            // Import phase is a UI projection: an inactive main draft reports
            // Retired even while its original Pending/Ready entry still owns
            // quota or bytes. Require the original reconcile/prune to finish.
            || !self.list_editor_draft_imports(card, draft)?.is_empty()
        {
            return Err("retire original draft imports before archiving".into());
        }
        if auxiliary
            .scope(card, draft)
            .iter()
            .any(|r| r.kind == Kind::Proposal && !r.closed)
        {
            return Err("complete original draft handoff before archiving".into());
        }
        Ok(())
    }

    fn archive_marker(&self, card: &CardRecord, auxiliary: &Inventory) -> Result<Marker> {
        let raw = card.body();
        if raw.len() > 2048 {
            return Err("draft archive marker budget".into());
        }
        let marker = Marker::decode(raw.as_slice())?;
        identity(&marker.card_id, &marker.draft_id, &marker.operation)?;
        let summary = card.summary();
        if !matches!(marker.version, 1 | 2)
            || (marker.version == 1
                && (marker.auxiliary_count != 0 || !marker.auxiliary_sha256.is_empty()))
            || (marker.version == 2 && marker.auxiliary_sha256.len() != 32)
            || marker.generation == 0
            || marker.operation.is_empty()
            || marker.request_sha256.len() != 32
            || marker.slot_sha256.len() != 32
            || marker.encode_to_vec() != raw
            || summary.revision != 1
            || summary.id != archive_key(&marker.card_id, &marker.draft_id)
            || summary.type_id != ARCHIVE_TYPE
            || summary.title != ARCHIVE_TITLE
            || summary.format_version != 1
            || !summary.preview_text.is_empty()
            || !card.attachments().is_empty()
        {
            return Err("invalid immutable draft archive marker".into());
        }
        let (original, slot) = self
            .draft_card(&marker.card_id, &marker.draft_id)?
            .ok_or("archived draft missing")?;
        let request = slot.request.as_ref().expect("validated draft");
        let mut retained_bytes = original.encode().len() as u64;
        if marker.version == 2 {
            let binding = auxiliary.binding(&marker.card_id, &marker.draft_id)?;
            if marker.auxiliary_count != binding.count || marker.auxiliary_sha256 != binding.digest
            {
                return Err("archived auxiliary journals changed".into());
            }
            retained_bytes = retained_bytes
                .checked_add(binding.bytes)
                .ok_or("archive bytes overflow")?;
        }
        if slot.active
            || slot.generation != marker.generation
            || !original.attachments().is_empty()
            || Sha256::digest(request.encode_to_vec()).as_slice() != marker.request_sha256
            || Sha256::digest(slot.encode_to_vec()).as_slice() != marker.slot_sha256
            || marker.retained_bytes != retained_bytes
            || (slot.discard_operation.is_empty() && slot.retirement.is_none())
        {
            return Err("archived draft no longer matches its retirement".into());
        }
        self.require_draft_archive_resources_closed(&slot, auxiliary)?;
        // A same-shaped card written by another operation is not a receipt.
        let (commit, _) = self
            .host
            .store_local()
            .operation_commit(&summary.id, &marker.operation)?
            .ok_or("draft archive operation missing")?;
        let command = transaction::decode_command(&commit.command)?;
        if !matches!(command.action, Some(transaction::proto::command::Action::CreateCard(ref bytes)) if *bytes == card.encode())
        {
            return Err("draft archive operation changed".into());
        }
        Ok(marker)
    }

    fn draft_archive_inventory(&self) -> Result<ArchiveInventory> {
        let auxiliary = self.draft_archive_auxiliary()?;
        self.draft_archive_inventory_with_auxiliary(&auxiliary)
    }
    fn draft_archive_inventory_with_auxiliary(
        &self,
        auxiliary: &Inventory,
    ) -> Result<ArchiveInventory> {
        let mut after = ARCHIVE_PREFIX.to_owned();
        let mut markers = BTreeMap::new();
        let mut bytes = 0u64;
        let mut auxiliary_count = 0usize;
        'pages: loop {
            let ids = self.host.store_local().card_ids_local(&after, 32)?;
            let count = ids.len();
            for id in ids {
                if !id.starts_with(ARCHIVE_PREFIX) {
                    break 'pages;
                }
                let card = self
                    .host
                    .store_local()
                    .card(&id)?
                    .ok_or("draft archive disappeared")?;
                let marker = self.archive_marker(&card, auxiliary)?;
                auxiliary_count = auxiliary_count
                    .checked_add(marker.auxiliary_count as usize)
                    .ok_or("archived auxiliary count overflow")?;
                bytes = bytes
                    .checked_add(marker.retained_bytes)
                    .and_then(|n| n.checked_add(card.encode().len() as u64))
                    .ok_or("draft archive accounting overflow")?;
                if markers
                    .insert(key(&marker.card_id, &marker.draft_id), marker)
                    .is_some()
                    || markers.len() > MAX_ARCHIVED_JOURNALS
                    || auxiliary_count > MAX_ARCHIVED_AUXILIARY
                    || bytes > MAX_ARCHIVED_BYTES
                {
                    return Err("draft archive capacity exceeded".into());
                }
                after = id;
            }
            if count < 32 {
                break;
            }
        }
        let mut covered_auxiliary: BTreeSet<String> = markers
            .iter()
            .filter(|(_, marker)| marker.version == 2)
            .map(|(id, _)| id.clone())
            .collect();
        let maintenance = self.draft_archive_maintenance_inventory(auxiliary, &markers)?;
        for (id, (marker, encoded_bytes)) in maintenance {
            if !covered_auxiliary.insert(id) {
                return Err("duplicate auxiliary archive accounting".into());
            }
            auxiliary_count = auxiliary_count
                .checked_add(marker.auxiliary_count as usize)
                .ok_or("archived auxiliary count overflow")?;
            bytes = bytes
                .checked_add(marker.auxiliary_bytes)
                .and_then(|n| n.checked_add(encoded_bytes))
                .ok_or("archive accounting overflow")?;
            if auxiliary_count > MAX_ARCHIVED_AUXILIARY || bytes > MAX_ARCHIVED_BYTES {
                return Err("draft archive maintenance capacity exceeded".into());
            }
        }
        Ok(ArchiveInventory {
            markers,
            bytes,
            auxiliary_count,
            covered_auxiliary,
        })
    }

    pub(crate) fn archived_draft_auxiliary_keys(&self, kind: Kind) -> Result<BTreeSet<String>> {
        let auxiliary = self.draft_archive_auxiliary()?;
        let inventory = self.draft_archive_inventory_with_auxiliary(&auxiliary)?;
        let mut result = BTreeSet::new();
        // Version 1 records stay charged until a separate original, immutable
        // maintenance receipt explicitly binds their complete closed inventory.
        for id in inventory.covered_auxiliary {
            for record in auxiliary.scope_key(&id).iter().filter(|r| r.kind == kind) {
                if !result.insert(record.id.clone()) {
                    return Err("duplicate archived auxiliary charge".into());
                }
            }
        }
        Ok(result)
    }

    pub(super) fn unarchived_draft_count(&self, slots: &[proto::Slot]) -> Result<usize> {
        let inventory = self.draft_archive_inventory()?;
        let mut count = 0;
        for slot in slots {
            let request = slot.request.as_ref().ok_or("draft request missing")?;
            if !inventory
                .markers
                .contains_key(&key(&request.card_id, &request.draft_id))
            {
                count += 1;
            }
        }
        Ok(count)
    }

    pub(crate) fn inspect_editor_draft_archive(
        &self,
        card: &str,
        draft: &str,
    ) -> Result<Option<EditorDraftArchive>> {
        identity(card, draft, "draft-archive-read")?;
        let auxiliary = self.draft_archive_auxiliary()?;
        self.host
            .store_local()
            .card(&archive_key(card, draft))?
            .map(|value| {
                self.archive_marker(&value, &auxiliary).map(|marker| {
                    receipt(
                        &marker,
                        &value,
                        auxiliary.scope(card, draft).len() as u32,
                        true,
                    )
                })
            })
            .transpose()
    }

    pub(crate) fn archive_editor_draft(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        request_sha256: &[u8],
        operation: &str,
    ) -> Result<EditorDraftArchive> {
        identity(card, draft, operation)?;
        if generation == 0 || request_sha256.len() != 32 || operation.is_empty() {
            return Err("invalid draft archive request".into());
        }
        if let Some(existing) = self.inspect_editor_draft_archive(card, draft)? {
            if existing.generation != generation
                || existing.request_sha256 != request_sha256
                || existing.operation != operation
            {
                return Err("draft archive retry changed".into());
            }
            return Ok(existing);
        }
        if !matches!(self.host.store_local().lookup(operation)?, Lookup::Absent) {
            return Err("draft archive operation already used".into());
        }
        let (original, slot) = self
            .draft_card(card, draft)?
            .ok_or("draft archive source missing")?;
        let request = slot.request.as_ref().expect("validated request");
        if slot.active
            || slot.generation != generation
            || !original.attachments().is_empty()
            || Sha256::digest(request.encode_to_vec()).as_slice() != request_sha256
            || operation == request.commit_operation
            || operation == request.operation_id
            || (slot.discard_operation.is_empty() && slot.retirement.is_none())
        {
            return Err("only the exact retired draft may be archived".into());
        }
        let auxiliary = self.draft_archive_auxiliary()?;
        self.require_draft_archive_resources_closed(&slot, &auxiliary)?;
        let binding = auxiliary.binding(card, draft)?;
        let inventory = self.draft_archive_inventory_with_auxiliary(&auxiliary)?;
        let marker = Marker {
            version: 2,
            card_id: card.into(),
            draft_id: draft.into(),
            generation,
            request_sha256: request_sha256.into(),
            slot_sha256: Sha256::digest(slot.encode_to_vec()).to_vec(),
            operation: operation.into(),
            retained_bytes: (original.encode().len() as u64)
                .checked_add(binding.bytes)
                .ok_or("archive bytes overflow")?,
            auxiliary_count: binding.count,
            auxiliary_sha256: binding.digest,
        };
        let id = archive_key(card, draft);
        let archived = CardRecord::new_with_attachments(
            &id,
            ARCHIVE_TYPE,
            1,
            ARCHIVE_TITLE,
            marker.encode_to_vec(),
            &[],
        )?;
        if inventory.markers.len() >= MAX_ARCHIVED_JOURNALS
            || inventory.auxiliary_count + marker.auxiliary_count as usize > MAX_ARCHIVED_AUXILIARY
            || inventory
                .bytes
                .checked_add(marker.retained_bytes)
                .and_then(|n| n.checked_add(archived.encode().len() as u64))
                .is_none_or(|n| n > MAX_ARCHIVED_BYTES)
        {
            return Err("draft archive capacity reached; export history for maintenance".into());
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
                .create_content(&connection, operation, &archived, || now(start))?;
            Ok(())
        })();
        let disconnected = self.host.disconnect(&connection);
        result?;
        disconnected?;
        Ok(receipt(&marker, &archived, 0, false))
    }

    pub(crate) fn list_editor_draft_history(
        &self,
        cursor: &str,
        limit: u32,
    ) -> Result<EditorDraftHistoryPage> {
        if !cursor_valid(cursor) || !(1..=32).contains(&limit) {
            return Err("draft history page bounds".into());
        }
        let inventory = self.draft_archive_inventory()?;
        let slots = self.all_draft_metadata()?;
        let unarchived = slots
            .len()
            .checked_sub(inventory.markers.len())
            .ok_or("draft archive inventory mismatch")?;
        if unarchived > MAX_JOURNALS {
            return Err("draft journal identity limit".into());
        }
        let mut rows = Vec::new();
        for slot in slots {
            let request = slot.request.as_ref().expect("validated request");
            let id = key(&request.card_id, &request.draft_id);
            if slot.active || id.as_str() <= cursor {
                continue;
            }
            let values = request.values.as_ref().expect("validated values");
            rows.push(EditorDraftHistoryRow {
                card_id: request.card_id.clone(),
                draft_id: request.draft_id.clone(),
                generation: slot.generation,
                request_sha256: Sha256::digest(request.encode_to_vec()).to_vec(),
                archive_operation: inventory
                    .markers
                    .get(&id)
                    .map_or(String::new(), |m| m.operation.clone()),
                cursor: id,
                title: values
                    .title
                    .as_ref()
                    .expect("validated title")
                    .text
                    .chars()
                    .take(128)
                    .collect(),
                preview: values
                    .description
                    .as_ref()
                    .expect("validated description")
                    .text
                    .chars()
                    .take(256)
                    .collect(),
            });
            if rows.len() > limit as usize {
                break;
            }
        }
        let next_cursor = if rows.len() > limit as usize {
            rows.pop();
            rows.last().expect("nonzero page").cursor.clone()
        } else {
            String::new()
        };
        Ok(EditorDraftHistoryPage {
            rows,
            next_cursor,
            unarchived: unarchived.try_into()?,
            archived: inventory.markers.len().try_into()?,
            archived_bytes: inventory.bytes,
        })
    }

    pub(crate) fn export_editor_draft_history(
        &self,
        card: &str,
        draft: &str,
        generation: u64,
    ) -> Result<Vec<u8>> {
        let (_, slot) = self
            .draft_card(card, draft)?
            .ok_or("retired draft missing")?;
        if slot.active || slot.generation != generation {
            return Err("retired draft export generation conflict".into());
        }
        let request = slot.request.as_ref().expect("validated request");
        let values = request.values.as_ref().expect("validated values");
        let text = |value: &Option<proto::TextValue>| {
            let value = value.as_ref().expect("validated field");
            serde_json::json!({"text": value.text, "selection_base": value.selection_base,
                "selection_extent": value.selection_extent, "affinity": value.affinity,
                "directional": value.directional, "composing_start": value.composing_start,
                "composing_end": value.composing_end})
        };
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let archive = self.inspect_editor_draft_archive(card, draft)?;
        let auxiliary = self.draft_archive_auxiliary()?;
        let auxiliary_exports = auxiliary
            .scope(card, draft)
            .iter()
            .map(|record| record.export(self))
            .collect::<Result<Vec<_>>>()?;
        Ok(serde_json::to_vec_pretty(&serde_json::json!({
            "format": "morrow.retired-editor-draft.v1", "card_id": card, "draft_id": draft,
            "generation": generation.to_string(), "active": false,
            "archive_operation": archive.map(|a| a.operation),
            "request_sha256": hex(&Sha256::digest(request.encode_to_vec())),
            "slot_sha256": hex(&Sha256::digest(slot.encode_to_vec())),
            "values": {"title": text(&values.title), "description": text(&values.description),
                "hypothesis": text(&values.hypothesis), "conclusion": text(&values.conclusion),
                "todos": text(&values.todos), "category": values.category, "stage": values.stage},
            "attachments": slot.assets.iter().map(|asset| serde_json::json!({
                "name": asset.display_name, "media_type": asset.media_type,
                "bytes": asset.byte_length.to_string(), "sha256": hex(&asset.sha256),
                "selection": asset.selection.as_ref().map(|s| serde_json::json!({
                    "origin": s.origin, "asset_id": s.asset_id, "aliases": s.aliases}))
            })).collect::<Vec<_>>(),
            "released_blobs_included": false, "capture_authority": false,
            "business_replay_authority": false,
            "auxiliary_journals": auxiliary_exports,
            "canonical_slot_protobuf_hex": hex(&slot.encode_to_vec())
        }))?)
    }
}

impl Workbench {
    pub fn archive_editor_draft(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        request_sha256: &[u8],
        operation: &str,
    ) -> Result<EditorDraftArchive> {
        self.local_state_mut()?.archive_editor_draft(
            card,
            draft,
            generation,
            request_sha256,
            operation,
        )
    }
    pub fn inspect_editor_draft_archive(
        &self,
        card: &str,
        draft: &str,
    ) -> Result<Option<EditorDraftArchive>> {
        self.local_state()?
            .inspect_editor_draft_archive(card, draft)
    }
    pub fn list_editor_draft_history(
        &self,
        cursor: &str,
        limit: u32,
    ) -> Result<EditorDraftHistoryPage> {
        self.local_state()?.list_editor_draft_history(cursor, limit)
    }
}
