//! Durable draft import metadata. Bytes use Core Snapshot retentions, never
//! CardRecord attachments, so discarded unselected imports can be released.
use crate::{Result, Workbench, WorkbenchState, now};
use morrow_core::{
    attachment::{BlobInfo, RetentionKind},
    content::{Attachment, CardRecord},
    content_change::ContentChange,
    lifecycle::GrantKind,
    transaction::{self, Lookup},
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) mod v1 {
    pub use super::proto::ImportRequest;
}
pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.editor_draft_staging.v1.rs"
    ));
}

const PREFIX: &str = "morrow-host-editor-imports-";
const TYPE: &str = "org.morrow.host.editor-draft-imports";
const TITLE: &str = "Editor draft imports";
const MAX_JOURNALS: usize = 256;
const MAX_PER_DRAFT: usize = 20;
const MAX_CURRENT: usize = 320;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BODY: usize = 1024 * 1024;
const MAX_REVISION: u64 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftImportPhase {
    Pending,
    Ready,
    Retired,
}
impl DraftImportPhase {
    fn parse(raw: u32) -> Result<Self> {
        match raw {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Ready),
            2 => Ok(Self::Retired),
            _ => Err("invalid draft import phase".into()),
        }
    }
    fn raw(self) -> u32 {
        match self {
            Self::Pending => 0,
            Self::Ready => 1,
            Self::Retired => 2,
        }
    }
}
#[derive(Clone, Debug)]
pub struct DraftImportRecord {
    pub request: proto::ImportRequest,
    pub asset_id: String,
    pub phase: DraftImportPhase,
    pub current_active: bool,
    pub bytes_retained: bool,
    pub staging_revision: u64,
    pub repeated: bool,
}

fn identity(value: &str) -> Result<()> {
    morrow_core::runtime::Command::ReadSummary {
        request_id: value.into(),
        card_id: value.into(),
    }
    .validate()?;
    Ok(())
}
fn hashed(prefix: &str, parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part.as_bytes());
    }
    format!(
        "{prefix}{}",
        hash.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
pub(crate) fn key(card: &str, draft: &str) -> String {
    hashed(PREFIX, &[card, draft])
}
fn owner(r: &proto::ImportRequest) -> String {
    hashed(
        "morrow-draft-import-owner-",
        &[&r.card_id, &r.draft_id, &r.operation_id],
    )
}
fn asset_id(r: &proto::ImportRequest) -> String {
    hashed(
        "asset-draft-import-",
        &[&r.card_id, &r.draft_id, &r.operation_id],
    )
}
fn internal_op(phase: &str, r: &proto::ImportRequest) -> String {
    hashed(
        "draft-import-stage-",
        &[phase, &r.card_id, &r.draft_id, &r.operation_id],
    )
}
fn media_type(kind: &str) -> &'static str {
    match kind {
        "image" => "image/*",
        "gif" => "image/gif",
        "video" => "video/*",
        "audio" => "audio/*",
        _ => "application/octet-stream",
    }
}
pub(crate) fn validate_request(r: &proto::ImportRequest) -> Result<()> {
    if r.schema_version != 1
        || r.encoded_len() > 64 * 1024
        || r.expected_generation == 0
        || r.expected_generation == u64::MAX
        || r.name.is_empty()
        || r.name.len() > 16 * 1024
        || r.kind.is_empty()
        || r.kind.len() > 1024
        || r.byte_length > MAX_BYTES
        || r.sha256.len() != 32
    {
        return Err("invalid draft import request".into());
    }
    for id in [&r.card_id, &r.draft_id, &r.operation_id] {
        identity(id)?;
    }
    if r.operation_id.starts_with("draft-import-stage-") {
        return Err("reserved draft import operation prefix".into());
    }
    Ok(())
}
fn mutation(op: &str, imported: &str, generation: u64, kind: u32) -> proto::LastMutation {
    proto::LastMutation {
        operation_id: op.into(),
        import_operation: imported.into(),
        expected_generation: generation,
        kind,
    }
}
fn decode_body(raw: &[u8]) -> Result<proto::Slot> {
    if raw.len() > MAX_BODY {
        return Err("draft import metadata limit".into());
    }
    let slot = proto::Slot::decode(raw)?;
    if slot.schema_version != 1
        || slot.revision == 0
        || slot.revision > MAX_REVISION
        || slot.encode_to_vec() != raw
        || slot.entries.len() > MAX_PER_DRAFT
    {
        return Err("noncanonical draft import journal".into());
    }
    identity(&slot.card_id)?;
    identity(&slot.draft_id)?;
    let last = slot
        .last_mutation
        .as_ref()
        .ok_or("draft import mutation missing")?;
    identity(&last.operation_id)?;
    identity(&last.import_operation)?;
    if last.kind > 4 {
        return Err("draft import mutation kind".into());
    }
    let mut operations = std::collections::HashSet::new();
    let mut assets = std::collections::HashSet::new();
    for entry in &slot.entries {
        let r = entry
            .request
            .as_ref()
            .ok_or("draft import request missing")?;
        validate_request(r)?;
        if r.card_id != slot.card_id
            || r.draft_id != slot.draft_id
            || entry.asset_id != asset_id(r)
            || entry.owner != owner(r)
            || !operations.insert(r.operation_id.as_str())
            || !assets.insert(entry.asset_id.as_str())
        {
            return Err("draft import identity changed".into());
        }
        match DraftImportPhase::parse(entry.phase)? {
            DraftImportPhase::Pending if !entry.blob_id.is_empty() => {
                return Err("pending import has a published blob".into());
            }
            DraftImportPhase::Ready if entry.blob_id.is_empty() => {
                return Err("ready import has no blob".into());
            }
            _ => {}
        }
        if !entry.blob_id.is_empty() {
            identity(&entry.blob_id)?;
        }
    }
    Ok(slot)
}
fn remaining_transitions(slot: &proto::Slot) -> Result<u64> {
    slot.entries.iter().try_fold(0u64, |total, entry| {
        let remaining = match DraftImportPhase::parse(entry.phase)? {
            DraftImportPhase::Pending => 3,
            DraftImportPhase::Ready => 2,
            DraftImportPhase::Retired => 1,
        };
        total
            .checked_add(remaining)
            .ok_or_else(|| "staging revision budget overflow".into())
    })
}
fn decode_card(card: &CardRecord) -> Result<proto::Slot> {
    let summary = card.summary();
    if summary.type_id != TYPE
        || summary.format_version != 1
        || summary.title != TITLE
        || !card.attachments().is_empty()
    {
        return Err("unsupported draft import journal".into());
    }
    let slot = decode_body(&card.body())?;
    if summary.id != key(&slot.card_id, &slot.draft_id) || summary.revision != slot.revision {
        return Err("draft import journal identity changed".into());
    }
    Ok(slot)
}
impl WorkbenchState {
    fn staging_slot(&self, card: &str, draft: &str) -> Result<Option<proto::Slot>> {
        identity(card)?;
        identity(draft)?;
        self.host
            .store_local()
            .card(&key(card, draft))?
            .map(|value| decode_card(&value))
            .transpose()
    }
    fn all_staging_slots(&self) -> Result<Vec<proto::Slot>> {
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
                    .ok_or("staging journal missing")?;
                result.push(decode_card(&card)?);
                if result.len() > MAX_JOURNALS {
                    return Err("draft import journal capacity changed".into());
                }
                after = id;
            }
            if count < 32 {
                break;
            }
        }
        Ok(result)
    }
    pub(crate) fn stage_history(
        &self,
        card: &str,
        draft: &str,
        op: &str,
    ) -> Result<Option<proto::Slot>> {
        identity(op)?;
        if matches!(self.host.store_local().lookup(op)?, Lookup::Absent) {
            return Ok(None);
        }
        let id = key(card, draft);
        let (commit, receipt) = self
            .host
            .store_local()
            .operation_commit(&id, op)?
            .ok_or("draft import operation belongs to another object")?;
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
                        .is_none_or(|refs| !refs.items.is_empty())
                {
                    return Err("draft import history command changed".into());
                }
                decode_body(&change.body)?
            }
            _ => return Err("operation is not a draft import mutation".into()),
        };
        if slot.card_id != card
            || slot.draft_id != draft
            || slot.revision != receipt.revision
            || slot
                .last_mutation
                .as_ref()
                .is_none_or(|last| last.operation_id != op)
        {
            return Err("draft import history binding changed".into());
        }
        Ok(Some(slot))
    }
    fn write_staging_slot(&mut self, slot: &proto::Slot, previous: u64, op: &str) -> Result<()> {
        let body = slot.encode_to_vec();
        decode_body(&body)?;
        if previous.checked_add(1) != Some(slot.revision) {
            return Err("draft import staging revision changed".into());
        }
        self.host.prepare_write()?;
        let id = key(&slot.card_id, &slot.draft_id);
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
                    .create_content(&connection, op, &card, || now(start))?;
            } else {
                self.host.edit_content(
                    &connection,
                    &ContentChange {
                        operation_id: op.into(),
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
    pub(crate) fn main_draft(
        &self,
        card: &str,
        draft: &str,
    ) -> Result<crate::editor_draft::model::proto::Slot> {
        self.draft_card(card, draft)?
            .map(|(_, slot)| slot)
            .ok_or_else(|| "draft import requires an existing draft journal".into())
    }
    fn retained_for(&self, entry: &proto::Entry) -> Result<Option<BlobInfo>> {
        let r = entry.request.as_ref().ok_or("import request missing")?;
        let retained = self.host.store_local().retained_blob_local(&entry.owner)?;
        if let Some(blob) = &retained {
            if blob.byte_length != r.byte_length
                || blob.sha256.as_slice() != r.sha256
                || (!entry.blob_id.is_empty() && entry.blob_id != blob.id)
            {
                return Err("draft import retained bytes changed".into());
            }
        } else if entry.phase == DraftImportPhase::Ready.raw() {
            return Err("ready draft import lost its Snapshot owner".into());
        }
        Ok(retained)
    }
    fn import_record(
        &self,
        entry: &proto::Entry,
        revision: u64,
        repeated: bool,
    ) -> Result<DraftImportRecord> {
        let r = entry
            .request
            .as_ref()
            .ok_or("draft import request missing")?
            .clone();
        let stored_phase = DraftImportPhase::parse(entry.phase)?;
        let retained = self.retained_for(entry)?.is_some();
        let main = self.draft_card(&r.card_id, &r.draft_id)?.map(|(_, s)| s);
        let blocked = main
            .as_ref()
            .is_none_or(|slot| !slot.active || slot.consumed_imports.contains(&r.operation_id));
        let phase = if blocked {
            DraftImportPhase::Retired
        } else {
            stored_phase
        };
        // current_active means selectable now, not counted in staging quota.
        let current_active = phase == DraftImportPhase::Ready
            && main
                .as_ref()
                .is_some_and(|slot| slot.generation >= r.expected_generation);
        Ok(DraftImportRecord {
            request: r,
            asset_id: entry.asset_id.clone(),
            phase,
            current_active,
            bytes_retained: retained,
            staging_revision: revision,
            repeated,
        })
    }
    pub(crate) fn original_entry(
        &self,
        card: &str,
        draft: &str,
        op: &str,
    ) -> Result<Option<proto::Entry>> {
        let Some(history) = self.stage_history(card, draft, op)? else {
            return Ok(None);
        };
        let last = history
            .last_mutation
            .as_ref()
            .ok_or("import history missing")?;
        if last.kind != 0 || last.import_operation != op {
            return Err("import operation was not its Pending intent".into());
        }
        let entry = history
            .entries
            .into_iter()
            .find(|entry| entry.request.as_ref().is_some_and(|r| r.operation_id == op))
            .ok_or("import Pending history missing")?;
        if entry.phase != DraftImportPhase::Pending.raw()
            || last.expected_generation
                != entry
                    .request
                    .as_ref()
                    .ok_or("missing original import request")?
                    .expected_generation
        {
            return Err("import operation did not record Pending".into());
        }
        Ok(Some(entry))
    }
    pub(crate) fn inspect_editor_draft_import(
        &self,
        card: &str,
        draft: &str,
        op: &str,
    ) -> Result<Option<DraftImportRecord>> {
        identity(card)?;
        identity(draft)?;
        identity(op)?;
        let Some(original) = self.original_entry(card, draft, op)? else {
            return Ok(None);
        };
        let slot = self
            .staging_slot(card, draft)?
            .ok_or("import journal missing")?;
        if let Some(entry) = slot
            .entries
            .iter()
            .find(|entry| entry.request.as_ref().is_some_and(|r| r.operation_id == op))
        {
            if entry.request != original.request
                || entry.owner != original.owner
                || entry.asset_id != original.asset_id
            {
                return Err("draft import original identity changed".into());
            }
            return self.import_record(entry, slot.revision, false).map(Some);
        }
        let request = original.request.as_ref().expect("validated").clone();
        let prune = self
            .stage_history(card, draft, &internal_op("prune", &request))?
            .ok_or("import disappeared without audited cleanup")?;
        let last = prune
            .last_mutation
            .as_ref()
            .ok_or("prune mutation missing")?;
        if last.kind != 3
            || last.import_operation != op
            || prune
                .entries
                .iter()
                .any(|e| e.request.as_ref().is_some_and(|r| r.operation_id == op))
        {
            return Err("import cleanup history changed".into());
        }
        Ok(Some(DraftImportRecord {
            request,
            asset_id: original.asset_id,
            phase: DraftImportPhase::Retired,
            current_active: false,
            bytes_retained: false,
            staging_revision: slot.revision,
            repeated: false,
        }))
    }
    pub(crate) fn list_editor_draft_imports(
        &self,
        card: &str,
        draft: &str,
    ) -> Result<Vec<DraftImportRecord>> {
        let Some(slot) = self.staging_slot(card, draft)? else {
            return Ok(Vec::new());
        };
        slot.entries
            .iter()
            .map(|entry| self.import_record(entry, slot.revision, false))
            .collect()
    }
    pub(crate) fn begin_editor_draft_import(
        &mut self,
        request: &proto::ImportRequest,
    ) -> Result<DraftImportRecord> {
        validate_request(request)?;
        if let Some(mut old) = self.inspect_editor_draft_import(
            &request.card_id,
            &request.draft_id,
            &request.operation_id,
        )? {
            if old.request != *request {
                return Err("draft import operation payload changed".into());
            }
            old.repeated = true;
            return Ok(old);
        }
        if self.unresolved_handoff_proposal_for_parent(&request.card_id, &request.draft_id)? {
            return Err("parent imports are frozen by a prepared handoff".into());
        }
        if self
            .draft_successor(&request.card_id, &request.draft_id)?
            .is_some()
        {
            return Err("parent draft is frozen after handoff".into());
        }
        let main = self.main_draft(&request.card_id, &request.draft_id)?;
        if !main.active || main.generation != request.expected_generation {
            return Err("draft import generation conflict".into());
        }
        let previous = self.staging_slot(&request.card_id, &request.draft_id)?;
        let mut slot = previous.unwrap_or_else(|| proto::Slot {
            schema_version: 1,
            card_id: request.card_id.clone(),
            draft_id: request.draft_id.clone(),
            revision: 0,
            entries: Vec::new(),
            last_mutation: None,
        });
        // Reserve every still-possible Ready, Retired and Pruned transition
        // before accepting a Pending intent. Cleanup cannot be stranded by
        // reaching the bounded audit revision limit.
        let reserved = remaining_transitions(&slot)?;
        if slot.entries.len() >= MAX_PER_DRAFT
            || slot
                .revision
                .checked_add(1)
                .and_then(|n| n.checked_add(reserved))
                .and_then(|n| n.checked_add(3))
                .is_none_or(|n| n > MAX_REVISION)
        {
            return Err("draft import journal capacity reached".into());
        }
        let all = self.all_staging_slots()?;
        if slot.revision == 0 && all.len() >= MAX_JOURNALS {
            return Err("draft import journal identity capacity reached".into());
        }
        let mut count = 1usize;
        let mut bytes = request.byte_length;
        for journal in &all {
            count = count
                .checked_add(journal.entries.len())
                .ok_or("import count overflow")?;
            for entry in &journal.entries {
                let length = entry.request.as_ref().ok_or("import missing")?.byte_length;
                bytes = bytes.checked_add(length).ok_or("import byte overflow")?;
            }
        }
        if count > MAX_CURRENT || bytes > MAX_BYTES {
            return Err("draft import active capacity reached".into());
        }
        let entry = proto::Entry {
            request: Some(request.clone()),
            asset_id: asset_id(request),
            owner: owner(request),
            blob_id: String::new(),
            phase: DraftImportPhase::Pending.raw(),
        };
        slot.entries.push(entry.clone());
        let previous = slot.revision;
        slot.revision += 1;
        slot.last_mutation = Some(mutation(
            &request.operation_id,
            &request.operation_id,
            request.expected_generation,
            0,
        ));
        self.write_staging_slot(&slot, previous, &request.operation_id)?;
        self.import_record(&entry, slot.revision, false)
    }
    pub(crate) fn import_editor_draft_asset_durable(
        &mut self,
        request: &proto::ImportRequest,
        reader: &mut impl std::io::Read,
    ) -> Result<DraftImportRecord> {
        let record = self.begin_editor_draft_import(request)?;
        if record.phase != DraftImportPhase::Pending {
            return Ok(record);
        }
        if self.unresolved_handoff_proposal_for_parent(&request.card_id, &request.draft_id)? {
            return Err("parent imports are frozen by a prepared handoff".into());
        }
        let main = self.main_draft(&request.card_id, &request.draft_id)?;
        if !main.active || main.consumed_imports.contains(&request.operation_id) {
            self.reconcile_draft_imports(&request.card_id, &request.draft_id)?;
            return self
                .inspect_editor_draft_import(
                    &request.card_id,
                    &request.draft_id,
                    &request.operation_id,
                )?
                .ok_or_else(|| "draft import disappeared".into());
        }
        let retained = self
            .host
            .store_local()
            .retained_blob_local(&owner(request))?;
        if main.generation != request.expected_generation && retained.is_none() {
            return Err("draft generation changed before import bytes were retained".into());
        }
        // A matching existing Snapshot owner makes this a read-only Core retry:
        // the supplied reader is not touched, even when the source path vanished.
        self.host.prepare_write()?;
        let clock = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        let blob = self.host.store_local_mut().stage_blob_retained(
            reader,
            request.byte_length,
            request.sha256.as_slice().try_into()?,
            &owner(request),
            clock,
        )?;
        let mut slot = self
            .staging_slot(&request.card_id, &request.draft_id)?
            .ok_or("import journal missing")?;
        let entry = slot
            .entries
            .iter_mut()
            .find(|entry| {
                entry
                    .request
                    .as_ref()
                    .is_some_and(|r| r.operation_id == request.operation_id)
            })
            .ok_or("import Pending entry missing")?;
        if entry.phase != DraftImportPhase::Pending.raw()
            || entry.request.as_ref() != Some(request)
            || entry.owner != owner(request)
        {
            return Err("draft import Pending state changed".into());
        }
        entry.blob_id = blob.id;
        entry.phase = DraftImportPhase::Ready.raw();
        let ready = entry.clone();
        let previous = slot.revision;
        slot.revision = previous
            .checked_add(1)
            .ok_or("staging revision exhausted")?;
        let op = internal_op("ready", request);
        slot.last_mutation = Some(mutation(
            &op,
            &request.operation_id,
            request.expected_generation,
            1,
        ));
        if let Some(history) = self.stage_history(&request.card_id, &request.draft_id, &op)? {
            let old = history
                .entries
                .iter()
                .find(|entry| {
                    entry
                        .request
                        .as_ref()
                        .is_some_and(|r| r.operation_id == request.operation_id)
                })
                .ok_or("ready history entry missing")?;
            if old != &ready || history != slot {
                return Err("draft import ready history changed".into());
            }
            return self
                .inspect_editor_draft_import(
                    &request.card_id,
                    &request.draft_id,
                    &request.operation_id,
                )?
                .ok_or_else(|| "ready import missing".into());
        }
        self.write_staging_slot(&slot, previous, &op)?;
        self.import_record(&ready, slot.revision, record.repeated)
    }
    pub(crate) fn resolve_durable_draft_import(
        &self,
        card: &str,
        draft: &str,
        asset: &str,
        expected_generation: u64,
    ) -> Result<Option<(Attachment, String)>> {
        let Some(slot) = self.staging_slot(card, draft)? else {
            return Ok(None);
        };
        let Some(entry) = slot.entries.iter().find(|entry| entry.asset_id == asset) else {
            return Ok(None);
        };
        let r = entry.request.as_ref().ok_or("import request missing")?;
        let main = self.main_draft(card, draft)?;
        if entry.phase != DraftImportPhase::Ready.raw()
            || !main.active
            || main.generation != expected_generation
            || r.expected_generation > expected_generation
            || main.consumed_imports.contains(&r.operation_id)
        {
            return Err("draft import is not selectable".into());
        }
        let blob = self
            .retained_for(entry)?
            .ok_or("draft import retention missing")?;
        Ok(Some((
            Attachment {
                id: entry.asset_id.clone(),
                display_name: r.name.clone(),
                media_type: media_type(&r.kind).into(),
                byte_length: blob.byte_length,
                sha256: blob.sha256,
            },
            r.operation_id.clone(),
        )))
    }
    pub(crate) fn export_editor_draft_import(
        &self,
        card: &str,
        draft: &str,
        generation: u64,
        import_op: &str,
    ) -> Result<Vec<u8>> {
        let record = self
            .inspect_editor_draft_import(card, draft, import_op)?
            .ok_or("draft import missing")?;
        let main = self.main_draft(card, draft)?;
        if !record.current_active
            || main.generation != generation
            || record.phase != DraftImportPhase::Ready
        {
            return Err("draft import is not exportable".into());
        }
        let slot = self
            .staging_slot(card, draft)?
            .ok_or("import journal missing")?;
        let entry = slot
            .entries
            .iter()
            .find(|entry| {
                entry
                    .request
                    .as_ref()
                    .is_some_and(|r| r.operation_id == import_op)
            })
            .ok_or("ready import entry missing")?;
        let blob = self
            .retained_for(entry)?
            .ok_or("draft import retention missing")?;
        let mut bytes = Vec::with_capacity(usize::try_from(blob.byte_length)?);
        let exported = self
            .host
            .store_local()
            .export_blob_local(&blob.id, &mut bytes)?;
        if exported.sha256 != blob.sha256
            || exported.byte_length != blob.byte_length
            || bytes.len() as u64 != blob.byte_length
        {
            return Err("draft import export changed".into());
        }
        Ok(bytes)
    }
    pub(crate) fn abandon_editor_draft_import(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        import_op: &str,
        abandon_op: &str,
    ) -> Result<DraftImportRecord> {
        identity(abandon_op)?;
        if abandon_op.starts_with("draft-import-stage-") {
            return Err("reserved draft import operation prefix".into());
        }
        let original = self
            .original_entry(card, draft, import_op)?
            .ok_or("draft import missing")?;
        if let Some(history) = self.stage_history(card, draft, abandon_op)? {
            let last = history
                .last_mutation
                .as_ref()
                .ok_or("draft abandon mutation missing")?;
            let retired = history
                .entries
                .iter()
                .find(|entry| {
                    entry
                        .request
                        .as_ref()
                        .is_some_and(|r| r.operation_id == import_op)
                })
                .ok_or("draft abandon entry missing")?;
            if last.kind != 4
                || last.import_operation != import_op
                || last.expected_generation != generation
                || retired.request != original.request
                || retired.phase != DraftImportPhase::Retired.raw()
            {
                return Err("draft abandon replay changed".into());
            }
            let mut result = self
                .inspect_editor_draft_import(card, draft, import_op)?
                .ok_or("draft import missing")?;
            result.repeated = true;
            return Ok(result);
        }
        if self.unresolved_handoff_proposal_for_parent(card, draft)? {
            return Err("parent imports are frozen by a prepared handoff".into());
        }
        let main = self.main_draft(card, draft)?;
        if !main.active
            || main.generation != generation
            || main
                .consumed_imports
                .iter()
                .any(|operation| operation == import_op)
        {
            return Err("draft abandon generation conflict".into());
        }
        let mut slot = self
            .staging_slot(card, draft)?
            .ok_or("draft import journal missing")?;
        let entry = slot
            .entries
            .iter_mut()
            .find(|entry| {
                entry
                    .request
                    .as_ref()
                    .is_some_and(|r| r.operation_id == import_op)
            })
            .ok_or("draft import already retired")?;
        if entry.request != original.request || entry.phase == DraftImportPhase::Retired.raw() {
            return Err("draft import abandon state changed".into());
        }
        entry.phase = DraftImportPhase::Retired.raw();
        let retired = entry.clone();
        let previous = slot.revision;
        slot.revision = previous
            .checked_add(1)
            .ok_or("staging revision exhausted")?;
        slot.last_mutation = Some(mutation(abandon_op, import_op, generation, 4));
        self.write_staging_slot(&slot, previous, abandon_op)?;
        self.import_record(&retired, slot.revision, false)
    }
    fn verify_consumed(
        &self,
        main: &crate::editor_draft::model::proto::Slot,
        entry: &proto::Entry,
    ) -> Result<()> {
        let r = entry.request.as_ref().ok_or("consumed request missing")?;
        if !main.consumed_imports.contains(&r.operation_id)
            || main
                .assets
                .iter()
                .filter(|asset| {
                    asset.selection.as_ref().is_some_and(|selection| {
                        selection.origin == 2 && selection.asset_id == entry.asset_id
                    }) && asset.byte_length == r.byte_length
                        && asset.sha256 == r.sha256
                        && asset.display_name == r.name
                        && asset.media_type == media_type(&r.kind)
                })
                .count()
                != 1
        {
            return Err("draft consumed import does not match selected pin".into());
        }
        Ok(())
    }
    pub(crate) fn reconcile_draft_imports(&mut self, card: &str, draft: &str) -> Result<()> {
        // Cleanup verifies consumed or inactive ownership below; a prepared
        // handoff must not strand the independent staging owner after retirement.
        let Some(mut slot) = self.staging_slot(card, draft)? else {
            return Ok(());
        };
        let main = self.main_draft(card, draft)?;
        // A current entry must remain bound to its original audited Pending request.
        for entry in &slot.entries {
            let request = entry.request.as_ref().ok_or("import request missing")?;
            let original = self
                .original_entry(card, draft, &request.operation_id)?
                .ok_or("current import lacks Pending history")?;
            if entry.request != original.request
                || entry.owner != original.owner
                || entry.asset_id != original.asset_id
            {
                return Err("current import changed its original identity".into());
            }
        }
        // Validate every consumption marker before any retention is released.
        // A corrupt marker can never release bytes that were not selected.
        for op in &main.consumed_imports {
            let original = self
                .original_entry(card, draft, op)?
                .ok_or("consumed import has no Pending history")?;
            self.verify_consumed(&main, &original)?;
            if !slot.entries.iter().any(|entry| {
                entry
                    .request
                    .as_ref()
                    .is_some_and(|r| r.operation_id == *op)
            }) {
                let request = original.request.as_ref().expect("validated");
                let pruned = self
                    .stage_history(card, draft, &internal_op("prune", request))?
                    .ok_or("consumed import lacks cleanup history")?;
                if pruned.last_mutation.as_ref().is_none_or(|last| {
                    last.kind != 3
                        || last.import_operation != *op
                        || pruned.entries.iter().any(|entry| {
                            entry
                                .request
                                .as_ref()
                                .is_some_and(|request| request.operation_id == *op)
                        })
                }) {
                    return Err("consumed cleanup history changed".into());
                }
            }
        }
        let requests = slot
            .entries
            .iter()
            .filter_map(|entry| entry.request.clone())
            .collect::<Vec<_>>();
        for request in requests {
            let Some(current) = slot
                .entries
                .iter()
                .find(|entry| {
                    entry
                        .request
                        .as_ref()
                        .is_some_and(|r| r.operation_id == request.operation_id)
                })
                .cloned()
            else {
                continue;
            };
            let should_retire =
                !main.active || main.consumed_imports.contains(&request.operation_id);
            if should_retire || current.phase == DraftImportPhase::Retired.raw() {
                let _ = self.retained_for(&current)?;
            }
            if should_retire && current.phase != DraftImportPhase::Retired.raw() {
                let op = internal_op("retire", &request);
                let mut next = slot.clone();
                let entry = next
                    .entries
                    .iter_mut()
                    .find(|entry| {
                        entry
                            .request
                            .as_ref()
                            .is_some_and(|r| r.operation_id == request.operation_id)
                    })
                    .ok_or("retire entry missing")?;
                entry.phase = DraftImportPhase::Retired.raw();
                let previous = next.revision;
                next.revision = previous
                    .checked_add(1)
                    .ok_or("staging revision exhausted")?;
                next.last_mutation = Some(mutation(&op, &request.operation_id, main.generation, 2));
                if let Some(history) = self.stage_history(card, draft, &op)? {
                    if history != next {
                        return Err("retire history changed".into());
                    }
                    if self.staging_slot(card, draft)? != Some(next) {
                        return Err("retire current slot changed".into());
                    }
                } else {
                    self.write_staging_slot(&next, previous, &op)?;
                }
                slot = self
                    .staging_slot(card, draft)?
                    .ok_or("retired journal missing")?;
            }
            let Some(current) = slot
                .entries
                .iter()
                .find(|entry| {
                    entry
                        .request
                        .as_ref()
                        .is_some_and(|r| r.operation_id == request.operation_id)
                })
                .cloned()
            else {
                continue;
            };
            if current.phase != DraftImportPhase::Retired.raw() {
                continue;
            }
            if let Some(blob) = self.retained_for(&current)? {
                let clock =
                    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
                self.host.store_local_mut().release_retention_local(
                    &blob.id,
                    &current.owner,
                    RetentionKind::Snapshot,
                    clock,
                )?;
            }
            let op = internal_op("prune", &request);
            let mut next = slot.clone();
            next.entries.retain(|entry| {
                entry
                    .request
                    .as_ref()
                    .is_none_or(|r| r.operation_id != request.operation_id)
            });
            let previous = next.revision;
            next.revision = previous
                .checked_add(1)
                .ok_or("staging revision exhausted")?;
            next.last_mutation = Some(mutation(&op, &request.operation_id, main.generation, 3));
            if let Some(history) = self.stage_history(card, draft, &op)? {
                if history != next {
                    return Err("prune history changed".into());
                }
                if self.staging_slot(card, draft)? != Some(next) {
                    return Err("prune current slot changed".into());
                }
            } else {
                self.write_staging_slot(&next, previous, &op)?;
            }
            slot = self
                .staging_slot(card, draft)?
                .ok_or("pruned journal missing")?;
        }
        Ok(())
    }
    pub(crate) fn draft_import_wire_context(
        &self,
        card: &str,
        draft: &str,
    ) -> Result<(u64, bool, u64)> {
        let main = self.draft_card(card, draft)?.map(|(_, slot)| slot);
        let revision = self
            .staging_slot(card, draft)?
            .map_or(0, |slot| slot.revision);
        Ok((
            main.as_ref().map_or(0, |slot| slot.generation),
            main.as_ref().is_some_and(|slot| slot.active),
            revision,
        ))
    }
    pub(crate) fn reconcile_editor_draft_imports(&mut self, card: &str, draft: &str) -> Result<()> {
        self.reconcile_draft_imports(card, draft)
    }
}
impl Workbench {
    pub fn begin_editor_draft_import(
        &mut self,
        request: &proto::ImportRequest,
    ) -> Result<DraftImportRecord> {
        self.local_state_mut()?.begin_editor_draft_import(request)
    }
    pub fn import_editor_draft_asset_durable(
        &mut self,
        request: &proto::ImportRequest,
        reader: &mut impl std::io::Read,
    ) -> Result<DraftImportRecord> {
        self.local_state_mut()?
            .import_editor_draft_asset_durable(request, reader)
    }
    pub fn inspect_editor_draft_import(
        &self,
        card: &str,
        draft: &str,
        op: &str,
    ) -> Result<Option<DraftImportRecord>> {
        self.local_state()?
            .inspect_editor_draft_import(card, draft, op)
    }
    pub fn list_editor_draft_imports(
        &self,
        card: &str,
        draft: &str,
    ) -> Result<Vec<DraftImportRecord>> {
        self.local_state()?.list_editor_draft_imports(card, draft)
    }
    pub fn export_editor_draft_import(
        &self,
        card: &str,
        draft: &str,
        generation: u64,
        import_op: &str,
    ) -> Result<Vec<u8>> {
        self.local_state()?
            .export_editor_draft_import(card, draft, generation, import_op)
    }
    pub fn abandon_editor_draft_import(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        import_op: &str,
        abandon_op: &str,
    ) -> Result<DraftImportRecord> {
        self.local_state_mut()?
            .abandon_editor_draft_import(card, draft, generation, import_op, abandon_op)
    }
    pub fn reconcile_editor_draft_imports(&mut self, card: &str, draft: &str) -> Result<()> {
        self.local_state_mut()?
            .reconcile_editor_draft_imports(card, draft)
    }
}
