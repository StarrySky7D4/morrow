//! Durable host-owned recovery slots for captured editor proposals.
//! A slot records one fixed projected edit, not a second content authority.
use crate::{Result, Workbench, WorkbenchState, captured_cards, now};
use morrow_core::{
    content::{Attachment, CardRecord},
    content_change::ContentChange,
    lifecycle::GrantKind,
    task_evidence::{self, Evidence},
    transaction::Lookup,
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Write},
};

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.editor_recovery.v1.rs"
    ));
}

const TYPE: &str = "org.morrow.host.editor-recovery";
const PREFIX: &str = "morrow-host-editor-recovery-";
const EVIDENCE_ATTACHMENT: &str = "morrow-host-recovery-evidence";
const MAX_SLOTS: usize = 16;
const MAX_ACTIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SLOT_BODY: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorRecoveryStatus {
    Pending,
    Committed,
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorRecovery {
    pub card_id: String,
    pub title: String,
    pub operation_id: String,
    pub evidence_digest: [u8; 32],
    pub source_revision: u64,
    pub current_revision: u64,
    pub status: EditorRecoveryStatus,
    pub active: bool,
}

fn slot_id(card_id: &str) -> String {
    let hash = Sha256::digest(card_id.as_bytes());
    format!(
        "{PREFIX}{}",
        hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

fn validate_identity(card_id: &str, operation: &str) -> Result<()> {
    morrow_core::runtime::Command::ReadSummary {
        request_id: operation.into(),
        card_id: card_id.into(),
    }
    .validate()?;
    Ok(())
}

fn decode_slot(card: &CardRecord) -> Result<proto::Slot> {
    let summary = card.summary();
    let raw = card.body();
    if summary.type_id != TYPE || summary.format_version != 1 || raw.len() > MAX_SLOT_BODY {
        return Err("unsupported editor recovery slot".into());
    }
    let slot = proto::Slot::decode(raw.as_slice())?;
    if slot.schema_version != 1
        || slot.encode_to_vec() != raw
        || slot.card_id.is_empty()
        || slot.operation_id.is_empty()
        || slot.evidence_sha256.len() != 32
        || slot.source_revision == 0
        || slot.evidence_container_bytes == 0
        || slot.evidence_container_bytes > task_evidence::MAX_CONTAINER_BYTES as u64
        || slot.evidence_attachment_id != EVIDENCE_ATTACHMENT
        || slot.title.is_empty()
        || slot.title.len() > 16 * 1024
        || slot_id(&slot.card_id) != summary.id
        || summary.title != "Editor recovery"
    {
        return Err("invalid editor recovery slot".into());
    }
    validate_identity(&slot.card_id, &slot.operation_id)?;
    let attachments = card.attachments();
    if slot.active {
        if slot.abandoned {
            return Err("active editor recovery cannot be abandoned".into());
        }
        let item = attachments
            .iter()
            .find(|a| a.id == EVIDENCE_ATTACHMENT)
            .ok_or("missing editor recovery evidence attachment")?;
        if item.byte_length != slot.evidence_container_bytes {
            return Err("editor recovery evidence length mismatch".into());
        }
    } else if !attachments.is_empty() {
        return Err("inactive editor recovery still has attachments".into());
    }
    Ok(slot)
}

fn evidence_digest(slot: &proto::Slot) -> [u8; 32] {
    slot.evidence_sha256
        .as_slice()
        .try_into()
        .expect("checked digest")
}

fn unix_millis() -> Result<i64> {
    crate::platform::unix_millis()
}

struct BoundedBytes(Vec<u8>);
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > task_evidence::MAX_CONTAINER_BYTES)
        {
            return Err(std::io::Error::other("editor recovery evidence budget"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl WorkbenchState {
    fn editor_slot(&self, card_id: &str) -> Result<Option<proto::Slot>> {
        validate_identity(card_id, "editor-recovery-read")?;
        let Some(card) = self.host.store_local().card(&slot_id(card_id))? else {
            return Ok(None);
        };
        Ok(Some(decode_slot(&card)?))
    }

    fn all_editor_slots(&self) -> Result<Vec<proto::Slot>> {
        let mut after = PREFIX.to_owned();
        let mut slots = Vec::new();
        loop {
            let ids = self.host.store_local().card_ids_local(&after, 32)?;
            if ids.is_empty() {
                break;
            }
            let count = ids.len();
            for id in ids {
                if !id.starts_with(PREFIX) {
                    return Ok(slots);
                }
                let card = self
                    .host
                    .store_local()
                    .card(&id)?
                    .ok_or("editor recovery journal disappeared")?;
                let slot = decode_slot(&card)?;
                if slot.active {
                    slots.push(slot);
                    if slots.len() > MAX_SLOTS {
                        return Err("editor recovery active slot count exceeded".into());
                    }
                }
                after = id;
            }
            if count < 32 {
                break;
            }
        }
        Ok(slots)
    }
    fn slot_evidence(&self, slot: &proto::Slot) -> Result<Evidence> {
        if !slot.active {
            return Err("editor recovery slot is inactive".into());
        }
        let mut bytes = BoundedBytes(Vec::with_capacity(slot.evidence_container_bytes as usize));
        let info = self.host.store_local().export_attachment_local(
            &slot_id(&slot.card_id),
            EVIDENCE_ATTACHMENT,
            &mut bytes,
        )?;
        if info.byte_length != slot.evidence_container_bytes
            || bytes.0.len() as u64 != slot.evidence_container_bytes
        {
            return Err("editor recovery attachment changed".into());
        }
        let evidence = task_evidence::decode(&bytes.0, evidence_digest(slot))?;
        let projected = captured_cards::derive(&evidence)?;
        let source = CardRecord::decode(projected.source_card())?;
        if projected.card().summary().id != slot.card_id
            || projected.card().summary().title != slot.title
            || projected.operation_id() != slot.operation_id
            || source.summary().revision != slot.source_revision
        {
            return Err("editor recovery evidence metadata mismatch".into());
        }
        let journal = self
            .host
            .store_local()
            .card(&slot_id(&slot.card_id))?
            .ok_or("editor recovery journal missing")?;
        let mut expected = projected.card().attachments();
        expected.push(Attachment {
            id: EVIDENCE_ATTACHMENT.into(),
            display_name: "Editor recovery evidence".into(),
            media_type: "application/vnd.morrow.task-evidence".into(),
            byte_length: info.byte_length,
            sha256: info.sha256,
        });
        if journal.attachments() != expected {
            return Err("editor recovery target attachments changed".into());
        }
        Ok(evidence)
    }

    fn confirmed_editor_commit(&self, slot: &proto::Slot) -> Result<bool> {
        let Some((commit, _)) = self
            .host
            .store_local()
            .operation_commit(&slot.card_id, &slot.operation_id)?
        else {
            return Ok(false);
        };
        if commit.task_evidence_sha256 != vec![slot.evidence_sha256.clone()] {
            return Ok(false);
        }
        let historical = self
            .host
            .store_local()
            .operation_evidence(&slot.card_id, &slot.operation_id)?;
        if historical.len() != 1 || historical[0].digest() != evidence_digest(slot) {
            return Ok(false);
        }
        Ok(captured_cards::verify_commit(&commit, &historical[0]).is_ok())
    }

    fn editor_status(&self, slot: &proto::Slot) -> Result<EditorRecovery> {
        let current_revision = self
            .host
            .store_local()
            .card(&slot.card_id)?
            .map_or(0, |card| card.summary().revision);
        let committed = self.confirmed_editor_commit(slot)?;
        let status = if committed {
            EditorRecoveryStatus::Committed
        } else if current_revision != slot.source_revision
            || !matches!(
                self.host.store_local().lookup(&slot.operation_id)?,
                Lookup::Absent
            )
        {
            EditorRecoveryStatus::Conflict
        } else {
            EditorRecoveryStatus::Pending
        };
        Ok(EditorRecovery {
            card_id: slot.card_id.clone(),
            title: slot.title.clone(),
            operation_id: slot.operation_id.clone(),
            evidence_digest: evidence_digest(slot),
            source_revision: slot.source_revision,
            current_revision,
            status,
            active: slot.active,
        })
    }

    pub(crate) fn list_editor_recoveries(&self) -> Result<Vec<EditorRecovery>> {
        self.all_editor_slots()?
            .into_iter()
            .filter(|s| s.active)
            .map(|s| self.editor_status(&s))
            .collect()
    }

    pub(crate) fn editor_recovery(&self, card_id: &str) -> Result<Option<EditorRecovery>> {
        self.editor_slot(card_id)?
            .map(|s| self.editor_status(&s))
            .transpose()
    }

    pub(crate) fn load_editor_recovery(
        &self,
        card_id: &str,
    ) -> Result<Option<captured_cards::ProjectedEdit>> {
        let Some(slot) = self.editor_slot(card_id)? else {
            return Ok(None);
        };
        if !slot.active {
            return Ok(None);
        }
        Ok(Some(captured_cards::derive(&self.slot_evidence(&slot)?)?))
    }

    pub(crate) fn persist_editor_proposal(
        &mut self,
        projected: &captured_cards::ProjectedEdit,
    ) -> Result<EditorRecovery> {
        let card_id = projected.card().summary().id;
        let operation = projected.operation_id();
        validate_identity(&card_id, operation)?;
        let evidence = projected.evidence();
        if evidence.container().len() > task_evidence::MAX_CONTAINER_BYTES {
            return Err("editor recovery evidence budget".into());
        }
        let derived = captured_cards::derive(evidence)?;
        if derived.card().encode() != projected.card().encode()
            || derived.command() != projected.command()
            || derived.source_card() != projected.source_card()
            || derived.operation_id() != operation
        {
            return Err("editor recovery projection changed".into());
        }
        if let Some(slot) = self.editor_slot(&card_id)? {
            if slot.active {
                if slot.operation_id != operation || slot.evidence_sha256 != evidence.digest() {
                    return Err("active editor recovery requires reconciliation".into());
                }
                self.slot_evidence(&slot)?;
                return self.editor_status(&slot);
            }
        }
        if !matches!(self.host.store_local().lookup(operation)?, Lookup::Absent) {
            return Err("editor recovery operation already used".into());
        }
        let source = CardRecord::decode(projected.source_card())?;
        if source.summary().id != card_id
            || source.summary().revision.checked_add(1) != Some(projected.card().summary().revision)
        {
            return Err("editor recovery source mismatch".into());
        }
        let slots = self.all_editor_slots()?;
        let active: Vec<_> = slots.iter().filter(|s| s.active).collect();
        let active_bytes: u64 = active.iter().map(|s| s.evidence_container_bytes).sum();
        if active.len() >= MAX_SLOTS
            || active_bytes
                .checked_add(evidence.container().len() as u64)
                .is_none_or(|n| n > MAX_ACTIVE_BYTES)
        {
            return Err("editor recovery capacity reached".into());
        }
        let mut attachments = projected.card().attachments();
        if attachments.iter().any(|a| a.id == EVIDENCE_ATTACHMENT) {
            return Err("editor recovery evidence attachment ID collision".into());
        }
        self.host.prepare_write()?;
        let blob = self.host.store_local_mut().stage_blob(
            &mut Cursor::new(evidence.container()),
            evidence.container().len() as u64,
            Some(Sha256::digest(evidence.container()).into()),
            unix_millis()?,
        )?;
        attachments.push(Attachment {
            id: EVIDENCE_ATTACHMENT.into(),
            display_name: "Editor recovery evidence".into(),
            media_type: "application/vnd.morrow.task-evidence".into(),
            byte_length: blob.byte_length,
            sha256: blob.sha256,
        });
        let slot = proto::Slot {
            schema_version: 1,
            card_id: card_id.clone(),
            operation_id: operation.into(),
            evidence_sha256: evidence.digest().to_vec(),
            source_revision: source.summary().revision,
            evidence_attachment_id: EVIDENCE_ATTACHMENT.into(),
            evidence_container_bytes: blob.byte_length,
            active: true,
            title: projected.card().summary().title,
            abandoned: false,
        };
        let body = slot.encode_to_vec();
        if body.len() > MAX_SLOT_BODY {
            return Err("editor recovery metadata budget".into());
        }
        let id = slot_id(&card_id);
        let previous = self.host.store_local().card(&id)?;
        let mut random = [0u8; 16];
        crate::platform::random(&mut random).map_err(|_| "editor recovery journal identity unavailable")?;
        let journal_op = format!(
            "editor-journal-{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let kind = if previous.is_some() {
            GrantKind::EditContent
        } else {
            GrantKind::CreateContent
        };
        let mut connection = self.host.connect()?;
        let start = self.start;
        let result = (|| -> Result<()> {
            let tick = now(start);
            self.host.grant(
                &mut connection,
                kind,
                &id,
                tick.saturating_add(30_000),
                tick,
            )?;
            if let Some(previous) = previous {
                self.host.edit_content(
                    &connection,
                    &ContentChange {
                        operation_id: journal_op,
                        card_id: id,
                        expected_revision: previous.summary().revision,
                        title: "Editor recovery".into(),
                        body,
                        preview_text: String::new(),
                        attachments: Some(attachments),
                    },
                    || now(start),
                )?;
            } else {
                let card = CardRecord::new_with_attachments(
                    &id,
                    TYPE,
                    1,
                    "Editor recovery",
                    body,
                    &attachments,
                )?;
                self.host
                    .create_content(&connection, &journal_op, &card, || now(start))?;
            }
            Ok(())
        })();
        let disconnected = self.host.disconnect(&connection);
        result?;
        disconnected?;
        self.editor_status(&slot)
    }

    pub(crate) fn acknowledge_editor_recovery(
        &mut self,
        card_id: &str,
        operation: &str,
        digest: &[u8],
    ) -> Result<()> {
        let Some(mut slot) = self.editor_slot(card_id)? else {
            return Err("editor recovery slot absent".into());
        };
        if slot.operation_id != operation || slot.evidence_sha256 != digest {
            return Err("editor recovery acknowledgement mismatch".into());
        }
        if !slot.active {
            if slot.abandoned || !self.confirmed_editor_commit(&slot)? {
                return Err("editor recovery was abandoned or not committed".into());
            }
            return Ok(()); // Lost acknowledgement reply.
        }
        let fixed = self.slot_evidence(&slot)?;
        let (commit, _) = self
            .host
            .store_local()
            .operation_commit(card_id, operation)?
            .ok_or("editor recovery operation not committed")?;
        if self
            .host
            .store_local()
            .operation_evidence(card_id, operation)?
            .iter()
            .map(Evidence::digest)
            .collect::<Vec<_>>()
            != vec![fixed.digest()]
        {
            return Err("editor recovery committed evidence mismatch".into());
        }
        captured_cards::verify_commit(&commit, &fixed)?;
        slot.active = false;
        let body = slot.encode_to_vec();
        let id = slot_id(card_id);
        let previous = self
            .host
            .store_local()
            .card(&id)?
            .ok_or("editor recovery journal missing")?;
        self.host.prepare_write()?;
        let mut random = [0u8; 16];
        crate::platform::random(&mut random).map_err(|_| "editor recovery journal identity unavailable")?;
        let journal_op = format!(
            "editor-ack-{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let mut connection = self.host.connect()?;
        let start = self.start;
        let result = (|| -> Result<()> {
            let tick = now(start);
            self.host.grant(
                &mut connection,
                GrantKind::EditContent,
                &id,
                tick.saturating_add(30_000),
                tick,
            )?;
            self.host.edit_content(
                &connection,
                &ContentChange {
                    operation_id: journal_op,
                    card_id: id,
                    expected_revision: previous.summary().revision,
                    title: "Editor recovery".into(),
                    body,
                    preview_text: String::new(),
                    attachments: Some(Vec::new()),
                },
                || now(start),
            )?;
            Ok(())
        })();
        let disconnected = self.host.disconnect(&connection);
        result?;
        disconnected?;
        Ok(())
    }

    /// Reserve an uncommitted original operation in the same atomic journal
    /// edit that retires its proposal and clears the current attachment pins.
    /// The reservation prevents a delayed business retry after slot reuse.
    pub(crate) fn abandon_editor_recovery(
        &mut self,
        card_id: &str,
        operation: &str,
        digest: &[u8],
    ) -> Result<()> {
        let Some(mut slot) = self.editor_slot(card_id)? else {
            return Err("editor recovery slot absent".into());
        };
        if slot.operation_id != operation || slot.evidence_sha256 != digest {
            return Err("editor recovery abandonment mismatch".into());
        }
        let id = slot_id(card_id);
        match self.host.store_local().lookup(operation)? {
            Lookup::Committed(receipt)
                if !slot.active && slot.abandoned && receipt.card_id == id =>
            {
                return Ok(()); // Lost abandonment reply.
            }
            Lookup::Absent if slot.active && !slot.abandoned => {}
            _ => return Err("editor recovery operation cannot be abandoned".into()),
        }
        slot.active = false;
        slot.abandoned = true;
        let previous = self
            .host
            .store_local()
            .card(&id)?
            .ok_or("editor recovery journal missing")?;
        self.host.prepare_write()?;
        let mut connection = self.host.connect()?;
        let start = self.start;
        let result = (|| -> Result<()> {
            let tick = now(start);
            self.host.grant(
                &mut connection,
                GrantKind::EditContent,
                &id,
                tick.saturating_add(30_000),
                tick,
            )?;
            self.host.edit_content(
                &connection,
                &ContentChange {
                    operation_id: operation.into(),
                    card_id: id,
                    expected_revision: previous.summary().revision,
                    title: "Editor recovery".into(),
                    body: slot.encode_to_vec(),
                    preview_text: String::new(),
                    attachments: Some(Vec::new()),
                },
                || now(start),
            )?;
            Ok(())
        })();
        let disconnected = self.host.disconnect(&connection);
        result?;
        disconnected?;
        Ok(())
    }
}

impl Workbench {
    pub fn list_editor_recoveries(&self) -> Result<Vec<EditorRecovery>> {
        self.local_state()?.list_editor_recoveries()
    }
    pub fn editor_recovery(&self, card_id: &str) -> Result<Option<EditorRecovery>> {
        self.local_state()?.editor_recovery(card_id)
    }
    pub fn acknowledge_editor_recovery(
        &mut self,
        card_id: &str,
        operation: &str,
        digest: &[u8],
    ) -> Result<()> {
        self.local_state_mut()?
            .acknowledge_editor_recovery(card_id, operation, digest)
    }
    pub fn abandon_editor_recovery(
        &mut self,
        card_id: &str,
        operation: &str,
        digest: &[u8],
    ) -> Result<()> {
        self.local_state_mut()?
            .abandon_editor_recovery(card_id, operation, digest)
    }
}
