//! Durable user decision to abandon one exact draft import. This journal is
//! metadata-only; the stage journal remains the authority for imported bytes.
use crate::{Result, Workbench, WorkbenchState, editor_draft_staging as staging, now};
use morrow_core::{
    content::CardRecord,
    content_change::ContentChange,
    lifecycle::GrantKind,
    transaction::{self, Lookup},
};
use prost::Message;
use sha2::{Digest, Sha256};

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.editor_draft_import_decision.v1.rs"
    ));
}
const PREFIX: &str = "morrow-host-editor-import-decision-";
const TYPE: &str = "org.morrow.host.editor-draft-import-decision";
const TITLE: &str = "Editor draft import decision";
const MAX_IDENTITIES: usize = 256;
const MAX_BODY: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionStatus {
    Pending,
    Committed,
    Cancelled,
    Conflict,
}
#[derive(Clone, Debug)]
pub struct DecisionRecord {
    pub request: staging::proto::ImportRequest,
    pub operation_id: String,
    pub expected_generation: u64,
    pub status: DecisionStatus,
    pub current_generation: u64,
    pub main_active: bool,
    pub staging_revision: u64,
    pub decision_revision: u64,
    pub committed_revision: u64,
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
fn scope_key(card: &str, draft: &str) -> String {
    hashed("morrow-host-editor-import-scope-", &[card, draft])
}
fn key(operation: &str) -> String {
    hashed(PREFIX, &[operation])
}
fn prepare_op(operation: &str) -> String {
    hashed("draft-import-decision-prepare-", &[operation])
}
fn validate_operation(operation: &str, imported: &str) -> Result<()> {
    identity(operation)?;
    if operation == imported
        || operation.starts_with("draft-import-stage-")
        || operation.starts_with("draft-import-decision-prepare-")
    {
        return Err("reserved or reused draft import decision operation".into());
    }
    Ok(())
}
fn decode_body(raw: &[u8]) -> Result<proto::Slot> {
    if raw.len() > MAX_BODY {
        return Err("draft import decision metadata limit".into());
    }
    let slot = proto::Slot::decode(raw)?;
    if slot.schema_version != 1
        || slot.encode_to_vec() != raw
        || !matches!(slot.revision, 1 | 2)
        || slot.cancelled != (slot.revision == 2)
        || slot.expected_generation == 0
        || slot.expected_generation == u64::MAX
    {
        return Err("noncanonical draft import decision".into());
    }
    let request = slot
        .request
        .as_ref()
        .ok_or("decision original request missing")?;
    staging::validate_request(request)?;
    validate_operation(&slot.operation_id, &request.operation_id)?;
    Ok(slot)
}
fn decode_card(card: &CardRecord) -> Result<proto::Slot> {
    let summary = card.summary();
    if summary.type_id != TYPE
        || summary.format_version != 1
        || summary.title != TITLE
        || !card.attachments().is_empty()
    {
        return Err("unsupported import decision journal".into());
    }
    let slot = decode_body(&card.body())?;
    if summary.id != key(&slot.operation_id) || summary.revision != slot.revision {
        return Err("import decision journal identity changed".into());
    }
    Ok(slot)
}
impl WorkbenchState {
    fn decision_slot(&self, operation: &str) -> Result<Option<proto::Slot>> {
        identity(operation)?;
        self.host
            .store_local()
            .card(&key(operation))?
            .map(|value| decode_card(&value))
            .transpose()
    }
    fn all_decisions(&self) -> Result<Vec<(String, proto::Slot)>> {
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
                    .ok_or("decision journal missing")?;
                result.push((id.clone(), decode_card(&card)?));
                if result.len() > MAX_IDENTITIES {
                    return Err("import decision identity capacity changed".into());
                }
                after = id;
            }
            if count < 32 {
                break;
            }
        }
        Ok(result)
    }
    fn decision_history(&self, decision_key: &str, operation: &str) -> Result<Option<proto::Slot>> {
        let Some((commit, receipt)) = self
            .host
            .store_local()
            .operation_commit(decision_key, operation)?
        else {
            return Ok(None);
        };
        let command = transaction::decode_command(&commit.command)?;
        let slot = match command.action {
            Some(transaction::proto::command::Action::CreateCard(raw)) => {
                decode_card(&CardRecord::decode(&raw)?)?
            }
            Some(transaction::proto::command::Action::SetContent(change)) => {
                if change.card_id != decision_key
                    || change.title != TITLE
                    || !change.preview_text.is_empty()
                    || change.expected_revision.checked_add(1) != Some(receipt.revision)
                    || change
                        .attachments
                        .as_ref()
                        .is_none_or(|refs| !refs.items.is_empty())
                {
                    return Err("decision history command changed".into());
                }
                decode_body(&change.body)?
            }
            _ => return Err("operation is not a decision mutation".into()),
        };
        if slot.revision != receipt.revision
            || key(&slot.operation_id) != decision_key
            || (slot.revision == 1 && prepare_op(&slot.operation_id) != operation)
            || (slot.revision == 2 && slot.operation_id != operation)
        {
            return Err("decision history binding changed".into());
        }
        Ok(Some(slot))
    }
    fn checked_decision(&self, operation: &str) -> Result<Option<proto::Slot>> {
        let Some(slot) = self.decision_slot(operation)? else {
            return Ok(None);
        };
        let original = self
            .decision_history(&key(operation), &prepare_op(operation))?
            .ok_or("decision lacks audited Prepare")?;
        if original.revision != 1
            || original.cancelled
            || original.request != slot.request
            || original.operation_id != slot.operation_id
            || original.expected_generation != slot.expected_generation
        {
            return Err("decision changed its original proposal".into());
        }
        if slot.cancelled {
            let cancelled = self
                .decision_history(&key(operation), operation)?
                .ok_or("decision cancellation history missing")?;
            if cancelled != slot {
                return Err("decision cancellation history changed".into());
            }
        }
        Ok(Some(slot))
    }
    fn write_decision_slot(
        &mut self,
        slot: &proto::Slot,
        previous: u64,
        operation: &str,
    ) -> Result<()> {
        let body = slot.encode_to_vec();
        decode_body(&body)?;
        if previous.checked_add(1) != Some(slot.revision) {
            return Err("decision revision changed".into());
        }
        self.host.prepare_write()?;
        let id = key(&slot.operation_id);
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
    fn exact_stage_commit(
        &self,
        request: &staging::proto::ImportRequest,
        expected: u64,
        operation: &str,
    ) -> Result<Option<u64>> {
        let stage_key = staging::key(&request.card_id, &request.draft_id);
        let Some((_, receipt)) = self
            .host
            .store_local()
            .operation_commit(&stage_key, operation)?
        else {
            return Ok(None);
        };
        let history = self
            .stage_history(&request.card_id, &request.draft_id, operation)?
            .ok_or("stage commit history missing")?;
        let last = history
            .last_mutation
            .as_ref()
            .ok_or("stage mutation missing")?;
        let retired = history.entries.iter().find(|entry| {
            entry
                .request
                .as_ref()
                .is_some_and(|r| r.operation_id == request.operation_id)
        });
        if last.kind != 4
            || last.operation_id != operation
            || last.import_operation != request.operation_id
            || last.expected_generation != expected
            || retired.is_none_or(|entry| {
                entry.request.as_ref() != Some(request)
                    || entry.phase != staging::DraftImportPhase::Retired as u32
            })
        {
            return Ok(None);
        }
        Ok(Some(receipt.revision))
    }
    fn live_match(&self, slot: &proto::Slot) -> Result<bool> {
        let request = slot.request.as_ref().ok_or("decision request missing")?;
        let Some((_, main)) = self.draft_card(&request.card_id, &request.draft_id)? else {
            return Ok(false);
        };
        if !main.active
            || main.generation != slot.expected_generation
            || main.consumed_imports.contains(&request.operation_id)
        {
            return Ok(false);
        }
        let staged = self.inspect_editor_draft_import(
            &request.card_id,
            &request.draft_id,
            &request.operation_id,
        )?;
        Ok(staged.is_some_and(|record| {
            record.request == *request && record.phase != staging::DraftImportPhase::Retired
        }))
    }
    fn decision_record(&self, slot: &proto::Slot) -> Result<DecisionRecord> {
        let request = slot.request.as_ref().ok_or("decision request missing")?;
        let (current_generation, main_active, staging_revision) =
            self.draft_import_wire_context(&request.card_id, &request.draft_id)?;
        let committed =
            self.exact_stage_commit(request, slot.expected_generation, &slot.operation_id)?;
        let status = if slot.cancelled {
            if committed.is_some() {
                return Err("cancelled decision also committed".into());
            }
            DecisionStatus::Cancelled
        } else if committed.is_some() {
            DecisionStatus::Committed
        } else if matches!(
            self.host.store_local().lookup(&slot.operation_id)?,
            Lookup::Absent
        ) {
            if self.live_match(slot)? {
                DecisionStatus::Pending
            } else {
                DecisionStatus::Conflict
            }
        } else {
            DecisionStatus::Conflict
        };
        Ok(DecisionRecord {
            request: request.clone(),
            operation_id: slot.operation_id.clone(),
            expected_generation: slot.expected_generation,
            status,
            current_generation,
            main_active,
            staging_revision,
            decision_revision: slot.revision,
            committed_revision: committed.unwrap_or(0),
        })
    }
    pub(crate) fn inspect_editor_draft_import_decision(
        &self,
        card: &str,
        draft: &str,
        generation: u64,
        imported: &str,
        operation: &str,
    ) -> Result<Option<DecisionRecord>> {
        identity(card)?;
        identity(draft)?;
        identity(imported)?;
        validate_operation(operation, imported)?;
        let Some(slot) = self.checked_decision(operation)? else {
            return Ok(None);
        };
        let request = slot.request.as_ref().ok_or("decision request missing")?;
        if request.card_id != card
            || request.draft_id != draft
            || request.operation_id != imported
            || slot.expected_generation != generation
        {
            return Err("draft import decision scope changed".into());
        }
        self.decision_record(&slot).map(Some)
    }
    pub(crate) fn prepare_editor_draft_import_decision(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        imported: &str,
        operation: &str,
    ) -> Result<DecisionRecord> {
        identity(card)?;
        identity(draft)?;
        identity(imported)?;
        validate_operation(operation, imported)?;
        if let Some(record) =
            self.inspect_editor_draft_import_decision(card, draft, generation, imported, operation)?
        {
            return Ok(record); // Exact historical retry precedes all current-state checks.
        }
        if self.unresolved_handoff_proposal_for_parent(card, draft)? {
            return Err("parent imports are frozen by a prepared handoff".into());
        }
        let original = self
            .original_entry(card, draft, imported)?
            .ok_or("import decision lacks original Pending proposal")?;
        let request = original.request.ok_or("import original request missing")?;
        let committed = self.exact_stage_commit(&request, generation, operation)?;
        if committed.is_none()
            && !matches!(self.host.store_local().lookup(operation)?, Lookup::Absent)
        {
            return Err("decision operation already belongs to another effect".into());
        }
        for (_, old) in self.all_decisions()? {
            let old_request = old.request.as_ref().ok_or("decision request missing")?;
            if old_request.card_id == card
                && old_request.draft_id == draft
                && old_request.operation_id == imported
                && old.operation_id != operation
            {
                let checked = self
                    .checked_decision(&old.operation_id)?
                    .ok_or("decision disappeared")?;
                if self.decision_record(&checked)?.status != DecisionStatus::Cancelled {
                    return Err("import already has an unresolved abandon decision".into());
                }
            }
        }
        if committed.is_none() {
            let candidate = proto::Slot {
                schema_version: 1,
                request: Some(request.clone()),
                operation_id: operation.into(),
                expected_generation: generation,
                revision: 1,
                cancelled: false,
            };
            if !self.live_match(&candidate)? {
                return Err("import no longer matches decision baseline".into());
            }
        }
        if self.all_decisions()?.len() >= MAX_IDENTITIES {
            if let Some(committed_revision) = committed {
                // The only capacity exception is an already audited legacy stage
                // commit. Revision zero explicitly means no decision journal exists.
                let (current_generation, main_active, staging_revision) =
                    self.draft_import_wire_context(card, draft)?;
                return Ok(DecisionRecord {
                    request,
                    operation_id: operation.into(),
                    expected_generation: generation,
                    status: DecisionStatus::Committed,
                    current_generation,
                    main_active,
                    staging_revision,
                    decision_revision: 0,
                    committed_revision,
                });
            }
            return Err("draft import decision identity capacity reached".into());
        }
        let slot = proto::Slot {
            schema_version: 1,
            request: Some(request),
            operation_id: operation.into(),
            expected_generation: generation,
            revision: 1,
            cancelled: false,
        };
        let body = slot.encode_to_vec();
        decode_body(&body)?;
        self.write_decision_slot(&slot, 0, &prepare_op(operation))?;
        let record = self
            .checked_decision(operation)?
            .ok_or("prepared decision missing")?;
        self.decision_record(&record)
    }
    pub(crate) fn cancel_editor_draft_import_decision(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        imported: &str,
        operation: &str,
    ) -> Result<DecisionRecord> {
        let mut slot = self
            .checked_decision(operation)?
            .ok_or("import decision not prepared")?;
        let request = slot.request.as_ref().ok_or("decision request missing")?;
        if request.card_id != card
            || request.draft_id != draft
            || request.operation_id != imported
            || slot.expected_generation != generation
        {
            return Err("draft import decision scope changed".into());
        }
        let current = self.decision_record(&slot)?;
        if current.status == DecisionStatus::Cancelled {
            return Ok(current);
        }
        if current.status == DecisionStatus::Committed {
            return Err("committed import decision cannot be cancelled".into());
        }
        if self.unresolved_handoff_proposal_for_parent(card, draft)? {
            return Err("parent imports are frozen by a prepared handoff".into());
        }
        if !matches!(self.host.store_local().lookup(operation)?, Lookup::Absent) {
            return Err("decision operation already used outside this cancellation".into());
        }
        slot.revision = 2;
        slot.cancelled = true;
        self.write_decision_slot(&slot, 1, operation)?;
        let confirmed = self
            .checked_decision(operation)?
            .ok_or("cancelled decision missing")?;
        self.decision_record(&confirmed)
    }
    pub(crate) fn list_editor_draft_import_decisions(
        &self,
        card: &str,
        draft: &str,
        cursor: &str,
        limit: u32,
    ) -> Result<(Vec<DecisionRecord>, String)> {
        identity(card)?;
        identity(draft)?;
        if limit == 0 || limit > 32 {
            return Err("decision page limit".into());
        }
        if !cursor.is_empty() {
            identity(cursor)?;
            if !cursor.starts_with(PREFIX) {
                return Err("decision page cursor prefix".into());
            }
            let stored = self
                .host
                .store_local()
                .card(cursor)?
                .ok_or("decision page cursor absent")?;
            let slot = decode_card(&stored)?;
            let request = slot.request.as_ref().ok_or("decision request missing")?;
            if request.card_id != card || request.draft_id != draft {
                return Err("decision page cursor scope changed".into());
            }
            let _ = self
                .checked_decision(&slot.operation_id)?
                .ok_or("decision page cursor history missing")?;
        }
        let mut records = Vec::new();
        let mut next = String::new();
        for (id, slot) in self.all_decisions()? {
            if !cursor.is_empty() && id.as_str() <= cursor {
                continue;
            }
            let request = slot.request.as_ref().ok_or("decision request missing")?;
            if request.card_id != card || request.draft_id != draft {
                continue;
            }
            let checked = self
                .checked_decision(&slot.operation_id)?
                .ok_or("decision disappeared during page")?;
            records.push(self.decision_record(&checked)?);
            next = id;
            if records.len() == limit as usize {
                break;
            }
        }
        if records.len() < limit as usize {
            next.clear();
        }
        Ok((records, next))
    }
    pub(crate) fn list_editor_draft_import_decision_scopes(
        &self,
        cursor: &str,
        limit: u32,
    ) -> Result<(Vec<(String, String)>, String)> {
        if limit == 0 || limit > 32 {
            return Err("decision scope page limit".into());
        }
        let mut scopes = std::collections::BTreeMap::new();
        for (_, slot) in self.all_decisions()? {
            let checked = self
                .checked_decision(&slot.operation_id)?
                .ok_or("decision scope lacks audited Prepare")?;
            if checked != slot {
                return Err("decision scope history changed".into());
            }
            let request = slot.request.as_ref().ok_or("decision request missing")?;
            let name = scope_key(&request.card_id, &request.draft_id);
            let pair = (request.card_id.clone(), request.draft_id.clone());
            if scopes
                .insert(name, pair.clone())
                .is_some_and(|prior| prior != pair)
            {
                return Err("decision scope identity collision".into());
            }
        }
        if !cursor.is_empty() {
            identity(cursor)?;
            if !cursor.starts_with("morrow-host-editor-import-scope-")
                || !scopes.contains_key(cursor)
            {
                return Err("decision scope cursor absent or foreign".into());
            }
        }
        let mut records = Vec::new();
        let mut next = String::new();
        for (name, pair) in scopes {
            if !cursor.is_empty() && name.as_str() <= cursor {
                continue;
            }
            records.push(pair);
            next = name;
            if records.len() == limit as usize {
                break;
            }
        }
        if records.len() < limit as usize {
            next.clear();
        }
        Ok((records, next))
    }
    pub(crate) fn commit_prepared_editor_draft_import_abandon(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        imported: &str,
        operation: &str,
    ) -> Result<staging::DraftImportRecord> {
        if let Some(decision) =
            self.inspect_editor_draft_import_decision(card, draft, generation, imported, operation)?
        {
            match decision.status {
                DecisionStatus::Pending | DecisionStatus::Committed => {
                    let record = self.abandon_editor_draft_import(
                        card, draft, generation, imported, operation,
                    )?;
                    let confirmed = self
                        .inspect_editor_draft_import_decision(
                            card, draft, generation, imported, operation,
                        )?
                        .ok_or("confirmed decision missing")?;
                    if confirmed.status != DecisionStatus::Committed {
                        return Err("abandon lacks exact stage commit".into());
                    }
                    return Ok(record);
                }
                DecisionStatus::Cancelled => {
                    return Err("cancelled import decision cannot commit".into());
                }
                DecisionStatus::Conflict => {
                    return Err("import decision conflicts with current state".into());
                }
            }
        }
        // A historical pre-decision stage receipt may still be confirmed, but
        // absence of a decision never grants authority for a fresh mutation.
        let original = self
            .original_entry(card, draft, imported)?
            .ok_or("import decision not prepared")?;
        let request = original.request.ok_or("import original request missing")?;
        if self
            .exact_stage_commit(&request, generation, operation)?
            .is_none()
        {
            return Err("import decision not prepared".into());
        }
        self.abandon_editor_draft_import(card, draft, generation, imported, operation)
    }
}
impl Workbench {
    pub fn prepare_editor_draft_import_decision(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        imported: &str,
        operation: &str,
    ) -> Result<DecisionRecord> {
        self.local_state_mut()?
            .prepare_editor_draft_import_decision(card, draft, generation, imported, operation)
    }
    pub fn inspect_editor_draft_import_decision(
        &self,
        card: &str,
        draft: &str,
        generation: u64,
        imported: &str,
        operation: &str,
    ) -> Result<Option<DecisionRecord>> {
        self.local_state()?
            .inspect_editor_draft_import_decision(card, draft, generation, imported, operation)
    }
    pub fn list_editor_draft_import_decisions(
        &self,
        card: &str,
        draft: &str,
        cursor: &str,
        limit: u32,
    ) -> Result<(Vec<DecisionRecord>, String)> {
        self.local_state()?
            .list_editor_draft_import_decisions(card, draft, cursor, limit)
    }
    pub fn cancel_editor_draft_import_decision(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        imported: &str,
        operation: &str,
    ) -> Result<DecisionRecord> {
        self.local_state_mut()?
            .cancel_editor_draft_import_decision(card, draft, generation, imported, operation)
    }
    pub fn list_editor_draft_import_decision_scopes(
        &self,
        cursor: &str,
        limit: u32,
    ) -> Result<(Vec<(String, String)>, String)> {
        self.local_state()?
            .list_editor_draft_import_decision_scopes(cursor, limit)
    }
    pub fn commit_prepared_editor_draft_import_abandon(
        &mut self,
        card: &str,
        draft: &str,
        generation: u64,
        imported: &str,
        operation: &str,
    ) -> Result<staging::DraftImportRecord> {
        self.local_state_mut()?
            .commit_prepared_editor_draft_import_abandon(
                card, draft, generation, imported, operation,
            )
    }
}
