//! Format-2 card edits with adopted editor captures. This is a distinct frozen
//! intent; old content-projection.v2 and cards-edit.v1 evidence keep their rules.
//! The editor snapshot and paste facts are host observations, not proof of every
//! keystroke, source authenticity, authorization, or actual guest execution.
use crate::{Result, capture_provenance::EditorSnapshot, cards_edit, projection, projection_v2};
use morrow_core::{
    content::{Attachment, CardRecord},
    dispatch::{Connection, HostRuntime},
    plugin_package::Package,
    task_evidence::{
        self, Evidence,
        proto::{Batch, Observation, TaskEvidence},
    },
    transaction::{self, Lookup, Receipt},
    versioned_content_change::VersionedContentChange,
};
use morrow_workbench_plugin::{Action, Idea, Request, cards_v2::Command};
use prost::{
    Message,
    encoding::{WireType, decode_key, decode_varint},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const INTENT_TYPE: &str = "morrow.workbench.cards-capture.v1";
const TOTAL_FUEL: u64 = crate::capture_provenance::TOTAL_FUEL;
const MAX_APPLICATIONS: usize = projection_v2::MAX_APPLICATIONS;
const MAX_TEXT: usize = projection_v2::MAX_TEXT;

type Facts = projection::proto::ContentProjectionV2;
type CaptureParent = projection::proto::CaptureParent;
type PasteApplication = projection::proto::PasteApplication;

/// Attach the exact final cards-edit observation to the selected prior capture
/// observations. The final guest call has already happened in `final_edit`.
pub fn compose(
    final_edit: &cards_edit::ProjectedEdit,
    scope: &str,
    snapshot: &EditorSnapshot,
    captures: Vec<CaptureParent>,
    applications: Vec<PasteApplication>,
    mut observations: Vec<Observation>,
) -> Result<ProjectedEdit> {
    let original = final_edit.evidence().data();
    let batch = original
        .batch
        .as_ref()
        .ok_or("missing final cards-edit batch")?;
    if original.schema_version != task_evidence::BATCH_VERSION
        || batch.intent_type != cards_edit::INTENT_TYPE
        || batch.observations.len() != 1
        || observations.len() != captures.len()
        || observations.len() >= task_evidence::MAX_BATCH_OBSERVATIONS
    {
        return Err("invalid final cards-edit capture composition".into());
    }
    if snapshot.todos != "" {
        return Err("TaskId editor content is not a card capture".into());
    }
    let facts = Facts {
        schema_version: 2,
        content: batch.intent.clone(),
        scope: scope.into(),
        captures,
        applications,
        snapshot: Some(projection_v2::snapshot_proto(snapshot)?),
    };
    if facts.encoded_len() > task_evidence::MAX_INTENT_BYTES {
        return Err("cards-capture intent budget".into());
    }
    observations.push(batch.observations[0].clone());
    let evidence = task_evidence::encode(TaskEvidence {
        schema_version: task_evidence::BATCH_VERSION,
        package_archive: original.package_archive.clone(),
        batch: Some(Batch {
            intent_type: INTENT_TYPE.into(),
            intent: facts.encode_to_vec(),
            total_fuel: TOTAL_FUEL,
            observations,
        }),
        ..Default::default()
    })?;
    let result = derive(&evidence)?;
    if result.command() != final_edit.command()
        || result.card().encode() != final_edit.card().encode()
        || result.source_card() != final_edit.source_card()
    {
        return Err("cards-capture final edit changed during composition".into());
    }
    Ok(result)
}

pub struct ProjectedEdit {
    final_edit: cards_edit::ProjectedEdit,
    change: VersionedContentChange,
    evidence: Evidence,
    facts: Facts,
    package_digest: [u8; 32],
    undo_deadline: Option<u64>,
    restore: bool,
}
impl ProjectedEdit {
    pub fn card(&self) -> &CardRecord {
        self.final_edit.card()
    }
    pub fn command(&self) -> &[u8] {
        self.final_edit.command()
    }
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }
    pub fn source_card(&self) -> &[u8] {
        self.final_edit.source_card()
    }
    pub fn operation_id(&self) -> &str {
        self.final_edit.operation_id()
    }
    pub fn observed_now(&self) -> u64 {
        self.final_edit.observed_now()
    }
    pub fn matches_intent(
        &self,
        operation: &str,
        source_revision: u64,
        command: &Command,
        scope: &str,
        snapshot: &EditorSnapshot,
    ) -> Result<bool> {
        Ok(self.facts.scope == scope
            && self.facts.snapshot.as_ref() == Some(&projection_v2::snapshot_proto(snapshot)?)
            && self
                .final_edit
                .matches_intent(operation, source_revision, command)?)
    }
    /// Submit only the complete cards-capture evidence. Historical receipts
    /// still pass current Store grants; only a new write needs the undo clock.
    pub fn commit(
        &self,
        host: &mut HostRuntime,
        connection: &Connection,
        clock: impl FnMut() -> u64,
    ) -> Result<Receipt> {
        if connection.package_digest() != Some(self.package_digest) {
            return Err("cards-capture connection package mismatch".into());
        }
        let source = self.change.source()?;
        let id = source.summary().id;
        let existing = matches!(
            host.store_local()
                .lookup_for_card(&id, &self.change.operation_id)?,
            Lookup::Committed(_)
        );
        let observed = self.observed_now();
        let deadline = self.undo_deadline;
        let restore = self.restore;
        Ok(host.edit_versioned_content_guarded_with_evidence(
            connection,
            &self.change,
            std::slice::from_ref(&self.evidence),
            clock,
            move |now| {
                if !existing {
                    if observed != 0 && now < observed {
                        return Err(morrow_core::Error::Invalid("cards-capture clock rollback"));
                    }
                    if restore && deadline.is_some_and(|limit| now >= limit) {
                        return Err(morrow_core::Error::Invalid("cards-capture undo expired"));
                    }
                }
                Ok(())
            },
        )?)
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Root,
    Parent,
    Paste,
    Part,
    Snapshot,
    Alias,
}
// Preflight the bounded protobuf before prost allocates nested strings/lists.
// The frozen V2 schema is reused as a fact container, never dispatched as V2.
fn preflight(mut raw: &[u8], kind: Kind, fields: &mut usize) -> Result<()> {
    let mut seen = 0u16;
    let mut repeated = 0usize;
    while !raw.is_empty() {
        *fields = fields.checked_sub(1).ok_or("cards-capture field budget")?;
        let (tag, wire) = decode_key(&mut raw)?;
        let max = match kind {
            Kind::Root => 6,
            Kind::Parent => 1,
            Kind::Paste => 7,
            Kind::Part => 3,
            Kind::Snapshot => 6,
            Kind::Alias => 3,
        };
        if tag == 0 || tag > max {
            return Err("unknown cards-capture field".into());
        }
        let multiple = matches!(
            (kind, tag),
            (Kind::Root, 4 | 5) | (Kind::Paste, 6) | (Kind::Snapshot, 6)
        );
        if multiple {
            repeated += 1;
            if repeated > 2048 {
                return Err("cards-capture repeated budget".into());
            }
        } else {
            let bit = 1u16 << tag;
            if seen & bit != 0 {
                return Err("duplicate cards-capture field".into());
            }
            seen |= bit;
        }
        let varint = matches!(
            (kind, tag),
            (Kind::Root, 1) | (Kind::Parent, 1) | (Kind::Paste, 4 | 5) | (Kind::Part, 1)
        );
        if wire
            != if varint {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err("cards-capture field wire".into());
        }
        if varint {
            let value = decode_varint(&mut raw)?;
            if value > u64::from(u32::MAX) {
                return Err("cards-capture integer budget".into());
            }
            continue;
        }
        let len = usize::try_from(decode_varint(&mut raw)?)?;
        let limit = match (kind, tag) {
            (Kind::Root, 2) => task_evidence::MAX_INTENT_BYTES,
            (Kind::Root, 3) | (Kind::Paste, 1 | 2) | (Kind::Part, 3) | (Kind::Alias, 1) => 256,
            (Kind::Parent, _) => 128,
            (Kind::Alias, 2 | 3) => 16 * 1024,
            (Kind::Paste, 3 | 7) | (Kind::Part, 2) | (Kind::Snapshot, 1..=5) => MAX_TEXT,
            _ => task_evidence::MAX_INTENT_BYTES,
        };
        if len > limit || len > raw.len() {
            return Err("cards-capture field length".into());
        }
        let (value, tail) = raw.split_at(len);
        raw = tail;
        let nested = match (kind, tag) {
            (Kind::Root, 4) => Some(Kind::Parent),
            (Kind::Root, 5) => Some(Kind::Paste),
            (Kind::Root, 6) => Some(Kind::Snapshot),
            (Kind::Paste, 6) => Some(Kind::Part),
            (Kind::Snapshot, 6) => Some(Kind::Alias),
            _ => None,
        };
        if let Some(next) = nested {
            preflight(value, next, fields)?;
        }
    }
    Ok(())
}
fn decode(raw: &[u8]) -> Result<Facts> {
    if raw.len() > task_evidence::MAX_INTENT_BYTES {
        return Err("cards-capture intent budget".into());
    }
    preflight(raw, Kind::Root, &mut 32768)?;
    let facts = Facts::decode(raw)?;
    if facts.schema_version != 2
        || facts.scope.is_empty()
        || facts.scope.len() > 256
        || facts.encode_to_vec() != raw
    {
        return Err("unsupported or noncanonical cards-capture facts".into());
    }
    Ok(facts)
}
fn editor_field(name: &str) -> Result<()> {
    match name {
        "title" | "description" | "hypothesis" | "conclusion" => Ok(()),
        _ => Err("cards-capture supports only common text fields".into()),
    }
}
fn verify_capture_chain(facts: &Facts, observations: &[Observation]) -> Result<()> {
    let count = facts.captures.len();
    if count != observations.len() || facts.applications.len() > MAX_APPLICATIONS {
        return Err("cards-capture observation count".into());
    }
    let mut values = Vec::with_capacity(count);
    for (i, parent) in facts.captures.iter().enumerate() {
        let value = projection_v2::captured(&observations[i])?;
        if let Some(index) = parent.parent {
            let index = index as usize;
            if index >= i {
                return Err("cards-capture parent order".into());
            }
            let preceding: &(String, String, String, Vec<u8>) = &values[index];
            if value.0 != "plain" || value.1 != preceding.2 {
                return Err("cards-capture parent source".into());
            }
        }
        values.push(value);
    }
    let mut used = vec![false; count];
    let mut ids = BTreeSet::new();
    for app in &facts.applications {
        editor_field(&app.field)?;
        if app.id.is_empty()
            || app.id.len() > 256
            || !ids.insert(&app.id)
            || app.parts.is_empty()
            || app.parts.len() > 1024
        {
            return Err("cards-capture paste identity".into());
        }
        let mut inserted = String::new();
        for part in &app.parts {
            if let Some(index) = part.observation {
                let index = index as usize;
                let value = values.get(index).ok_or("cards-capture observation index")?;
                if !part.literal.is_empty() {
                    return Err("mixed cards-capture part".into());
                }
                let selected = match part.selection.as_str() {
                    "outputMarkdown" => &value.2,
                    "inputPlainText" if value.0 == "plain" => &value.1,
                    _ => return Err("unsupported cards-capture selection".into()),
                };
                inserted.push_str(selected);
                used[index] = true;
            } else {
                if !part.selection.is_empty() {
                    return Err("literal has capture selection".into());
                }
                inserted.push_str(&part.literal);
            }
            if inserted.len() > MAX_TEXT {
                return Err("cards-capture insertion budget".into());
            }
        }
        projection_v2::replacement(
            &app.before,
            app.start_utf16,
            app.end_utf16,
            &inserted,
            &app.after,
        )?;
    }
    for i in (0..count).rev() {
        if used[i]
            && let Some(parent) = facts.captures[i].parent
        {
            used[parent as usize] = true;
        }
    }
    if used.iter().any(|used| !used) {
        return Err("unadopted cards-capture observation".into());
    }
    Ok(())
}
fn verify_editor_snapshot(facts: &Facts, final_edit: &cards_edit::ProjectedEdit) -> Result<()> {
    let snapshot = facts
        .snapshot
        .as_ref()
        .ok_or("missing cards-capture snapshot")?;
    if !snapshot.todos.is_empty() {
        return Err("cards-capture cannot edit TaskIds".into());
    }
    let source = CardRecord::decode(final_edit.source_card())?;
    let summary = source.summary();
    let final_batch = final_edit
        .evidence()
        .data()
        .batch
        .as_ref()
        .ok_or("missing cards-edit facts")?;
    let cards_facts = cards_edit::proto::Facts::decode(final_batch.intent.as_slice())?;
    let request = morrow_workbench_plugin::cards_v2_codec::decode_request(&cards_facts.request)?;
    let Command::Edit(fields) = request.command else {
        return Err("cards-capture final action must edit common fields".into());
    };
    if request.id != summary.id
        || request.title != summary.title
        || request.properties != source.body()
    {
        return Err("cards-capture final request/source mismatch".into());
    }
    let proposed = Idea {
        id: summary.id,
        title: fields.title,
        description: fields.description,
        hypothesis: fields.hypothesis,
        conclusion: fields.conclusion,
        assets: fields.assets,
        todos: Vec::new(),
        ..Default::default()
    };
    // A synthetic V1 Request is used only by the frozen pure snapshot normalizer;
    // it is never inserted into evidence or described as a guest observation.
    let check = Request {
        action: Action::Edit,
        current: Idea::default(),
        proposed,
        text: String::new(),
        flag: false,
        now_ms: 0,
        ideas: Vec::new(),
        section: "概览".into(),
        filter: "全部".into(),
        sort: "最近添加".into(),
    };
    projection_v2::verify_snapshot(snapshot, &check)
}
fn final_child(
    evidence: &Evidence,
    facts: &Facts,
    final_observation: &Observation,
) -> Result<cards_edit::ProjectedEdit> {
    let child = task_evidence::encode(TaskEvidence {
        schema_version: task_evidence::BATCH_VERSION,
        package_archive: evidence.data().package_archive.clone(),
        batch: Some(Batch {
            intent_type: cards_edit::INTENT_TYPE.into(),
            intent: facts.content.clone(),
            total_fuel: 20_000_000,
            observations: vec![final_observation.clone()],
        }),
        ..Default::default()
    })?;
    cards_edit::derive(&child)
}
fn change_from_command(final_edit: &cards_edit::ProjectedEdit) -> Result<VersionedContentChange> {
    let decoded = transaction::decode_command(final_edit.command())?;
    let Some(transaction::proto::command::Action::SetVersionedContent(value)) = decoded.action
    else {
        return Err("cards-capture final action is not versioned content".into());
    };
    let attachments = value
        .attachments
        .map(|list| {
            list.items
                .into_iter()
                .map(|raw| {
                    let item = Attachment {
                        id: raw.id,
                        display_name: raw.display_name,
                        media_type: raw.media_type,
                        byte_length: raw.byte_length,
                        sha256: raw
                            .sha256
                            .try_into()
                            .map_err(|_| "attachment sha256 length")?,
                    };
                    item.validate()?;
                    Ok(item)
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;
    let change = VersionedContentChange {
        operation_id: decoded.operation_id,
        source_card: value.source_card,
        title: value.title,
        body: value.body,
        preview_text: value.preview_text,
        attachments,
    };
    if transaction::versioned_content_command(&change)? != final_edit.command() {
        return Err("cards-capture command changed during derivation".into());
    }
    let source = change.source()?;
    if change.source_card != final_edit.source_card()
        || change.propose(&source)?.encode() != final_edit.card().encode()
    {
        return Err("cards-capture command/card mismatch".into());
    }
    Ok(change)
}
/// Derive all adopted conversion observations and the final frozen cards-edit
/// projection from one immutable batch. This never invokes a guest.
pub fn derive(evidence: &Evidence) -> Result<ProjectedEdit> {
    let actual = evidence.data();
    let batch = actual.batch.as_ref().ok_or("missing cards-capture batch")?;
    if actual.schema_version != task_evidence::BATCH_VERSION
        || batch.intent_type != INTENT_TYPE
        || batch.total_fuel != TOTAL_FUEL
        || batch.observations.is_empty()
    {
        return Err("unsupported cards-capture batch".into());
    }
    let facts = decode(&batch.intent)?;
    let captures = &batch.observations[..batch.observations.len() - 1];
    verify_capture_chain(&facts, captures)?;
    let final_edit = final_child(evidence, &facts, batch.observations.last().unwrap())?;
    verify_editor_snapshot(&facts, &final_edit)?;
    let change = change_from_command(&final_edit)?;
    let package_digest = Package::decode(&actual.package_archive)?.digest();
    let cards_facts = cards_edit::proto::Facts::decode(facts.content.as_slice())?;
    let request = morrow_workbench_plugin::cards_v2_codec::decode_request(&cards_facts.request)?;
    let restore = matches!(request.command, Command::Restore { .. });
    let undo_deadline = cards_facts.undo.map(|undo| undo.deadline);
    Ok(ProjectedEdit {
        final_edit,
        change,
        evidence: evidence.clone(),
        facts,
        package_digest,
        undo_deadline,
        restore,
    })
}
/// Check one original Commit against the complete new evidence digest.
pub fn verify_commit(
    commit: &transaction::proto::Commit,
    evidence: &Evidence,
) -> Result<ProjectedEdit> {
    let projected = derive(evidence)?;
    let summary = projected.card().summary();
    if commit.schema_version != 2
        || commit.task_evidence_sha256 != vec![evidence.digest().to_vec()]
        || commit.command != projected.command()
        || commit.command_sha256 != Sha256::digest(projected.command()).as_slice()
        || commit.operation_id != projected.operation_id()
        || commit.event_id != projected.operation_id()
        || commit.card_id != summary.id
        || commit.revision != summary.revision
        || commit.content_sha256 != Sha256::digest(projected.card().encode()).as_slice()
        || commit.attachment_sha256
            != projected
                .card()
                .attachments()
                .iter()
                .map(|a| a.sha256.to_vec())
                .collect::<Vec<_>>()
    {
        return Err("cards-capture projection differs from original commit".into());
    }
    Ok(projected)
}
