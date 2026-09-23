//! Frozen format-2 common-card edit projection, separate from TaskId edits.
//! Derivation proves exact correspondence to recorded guest bytes. It does not
//! execute a guest, choose attachments, grant access, or authorize a commit.
//! Cards-edit contract 1 must remain available for historical replay.
use crate::Result;
use morrow_core::{
    content::{Attachment, CardRecord, MAX_RECORD_BYTES},
    dispatch::{Connection, HostRuntime},
    plugin_package::Package,
    task::{Invocation, Transform},
    task_evidence::{
        self, Evidence,
        proto::{Batch, Observation, TaskEvidence},
    },
    transaction::{self, Lookup, Receipt},
    versioned_content_change::VersionedContentChange,
};
use morrow_workbench_plugin::{cards_v2::Command, cards_v2_codec, tasks_v2};
use prost::{
    Message,
    encoding::{WireType, decode_key, decode_varint},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.cards_edit.v1.rs"
    ));
}
pub const INTENT_TYPE: &str = "morrow.workbench.cards-edit.v1";
const FUEL: u64 = 20_000_000;
const MEMORY: u64 = 16 * 1024 * 1024;
const MAX_ATTACHMENTS: usize = 20;
const MAX_REQUEST: usize = 65536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Undo {
    pub revision: u64,
    pub deadline: u64,
}

pub struct Plan {
    intent: Vec<u8>,
    invocation: Invocation,
}
impl Plan {
    pub fn prepare(
        source: &CardRecord,
        package: &Package,
        operation: &str,
        command: &Command,
        attachments: &[Attachment],
        undo: Option<Undo>,
    ) -> Result<Self> {
        let facts = facts_for(
            source,
            package.digest(),
            operation,
            command,
            attachments,
            undo,
        )?;
        let intent = facts.encode_to_vec();
        if intent.len() > task_evidence::MAX_INTENT_BYTES {
            return Err("cards-edit intent budget".into());
        }
        // The same canonical parser is used for freshly prepared and historical
        // facts, so a Plan cannot issue an invocation derive would reject.
        let facts = decode_facts(&intent)?;
        let (invocation, _, _) = expected(&facts)?;
        Ok(Self { intent, invocation })
    }
    pub fn invocation(&self) -> &Invocation {
        &self.invocation
    }
    pub fn operation_id(&self) -> &str {
        self.invocation.task_id()
    }
    /// Wrap a real pool capture; never invent or rewrite execution observations.
    pub fn capture(&self, capture: &Evidence) -> Result<ProjectedEdit> {
        let actual = capture.data();
        if actual.schema_version != task_evidence::VERSION
            || actual.batch.is_some()
            || actual.invocation != self.invocation.bytes()
        {
            return Err("cards-edit capture differs from plan".into());
        }
        let evidence = task_evidence::encode(TaskEvidence {
            schema_version: task_evidence::BATCH_VERSION,
            package_archive: actual.package_archive.clone(),
            batch: Some(Batch {
                intent_type: INTENT_TYPE.into(),
                intent: self.intent.clone(),
                total_fuel: FUEL,
                observations: vec![Observation {
                    invocation: actual.invocation.clone(),
                    budget: actual.budget,
                    backend: actual.backend.clone(),
                    completion: actual.completion.clone(),
                    fault: actual.fault,
                    exit_code: actual.exit_code,
                    observed_host_calls: actual.observed_host_calls,
                    fuel_remaining: actual.fuel_remaining,
                }],
            }),
            ..Default::default()
        })?;
        derive(&evidence)
    }
}

pub struct ProjectedEdit {
    facts: proto::Facts,
    change: VersionedContentChange,
    card: CardRecord,
    command: Vec<u8>,
    evidence: Evidence,
    package_digest: [u8; 32],
    observed_now: u64,
}
impl ProjectedEdit {
    pub fn card(&self) -> &CardRecord {
        &self.card
    }
    pub fn command(&self) -> &[u8] {
        &self.command
    }
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }
    pub fn source_card(&self) -> &[u8] {
        &self.change.source_card
    }
    pub fn observed_now(&self) -> u64 {
        self.observed_now
    }
    pub fn operation_id(&self) -> &str {
        &self.change.operation_id
    }
    /// The caller supplies the original command and source revision, never a
    /// reinterpretation using the current Card or current clock.
    pub fn matches_intent(
        &self,
        operation: &str,
        source_revision: u64,
        command: &Command,
    ) -> Result<bool> {
        let source = self.change.source()?;
        let summary = source.summary();
        if operation != self.change.operation_id || source_revision != summary.revision {
            return Ok(false);
        }
        let canonical =
            cards_v2_codec::encode_request(&summary.id, &summary.title, &source.body(), command)?;
        Ok(canonical == self.facts.request)
    }
    /// A historical receipt may be returned after the original undo window:
    /// the Store will still compare the exact command/evidence and recheck the
    /// current EditContent grant twice. New writes must pass the live clock guard.
    pub fn commit(
        &self,
        host: &mut HostRuntime,
        connection: &Connection,
        clock: impl FnMut() -> u64,
    ) -> Result<Receipt> {
        if connection.package_digest() != Some(self.package_digest) {
            return Err("cards-edit connection package mismatch".into());
        }
        let source = self.change.source()?;
        let id = source.summary().id;
        let existing = matches!(
            host.store_local()
                .lookup_for_card(&id, &self.change.operation_id)?,
            Lookup::Committed(_)
        );
        let request = cards_v2_codec::decode_request(&self.facts.request)?;
        let observed = match &request.command {
            Command::Delete { now_ms } | Command::Restore { now_ms } => Some(*now_ms),
            _ => None,
        };
        let deadline = self.facts.undo.as_ref().map(|u| u.deadline);
        let restore = matches!(&request.command, Command::Restore { .. });
        Ok(host.edit_versioned_content_guarded_with_evidence(
            connection,
            &self.change,
            std::slice::from_ref(&self.evidence),
            clock,
            move |now| {
                if !existing {
                    if observed.is_some_and(|time| now < time) {
                        return Err(morrow_core::Error::Invalid("cards-edit clock rollback"));
                    }
                    if restore && deadline.is_some_and(|limit| now >= limit) {
                        return Err(morrow_core::Error::Invalid("cards-edit undo expired"));
                    }
                }
                Ok(())
            },
        )?)
    }
}

fn identity(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err("invalid cards-edit identity".into());
    }
    Ok(())
}
fn attachment_to_proto(value: &Attachment) -> proto::Attachment {
    proto::Attachment {
        id: value.id.clone(),
        display_name: value.display_name.clone(),
        media_type: value.media_type.clone(),
        byte_length: value.byte_length,
        sha256: value.sha256.to_vec(),
    }
}
fn attachment_from_proto(value: &proto::Attachment) -> Result<Attachment> {
    let item = Attachment {
        id: value.id.clone(),
        display_name: value.display_name.clone(),
        media_type: value.media_type.clone(),
        byte_length: value.byte_length,
        sha256: value.sha256.as_slice().try_into()?,
    };
    item.validate()?;
    Ok(item)
}
fn facts_for(
    source: &CardRecord,
    package_digest: [u8; 32],
    operation: &str,
    command: &Command,
    attachments: &[Attachment],
    undo: Option<Undo>,
) -> Result<proto::Facts> {
    identity(operation)?;
    if attachments.len() > MAX_ATTACHMENTS {
        return Err("cards-edit attachment budget".into());
    }
    let summary = source.summary();
    let request =
        cards_v2_codec::encode_request(&summary.id, &summary.title, &source.body(), command)?;
    if request.len() > MAX_REQUEST {
        return Err("cards-edit request budget".into());
    }
    Ok(proto::Facts {
        schema_version: 1,
        operation_id: operation.into(),
        source_card: source.encode(),
        package_sha256: package_digest.to_vec(),
        request,
        attachments: attachments.iter().map(attachment_to_proto).collect(),
        undo: undo.map(|value| proto::Undo {
            revision: value.revision,
            deadline: value.deadline,
        }),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Facts,
    Attachment,
    Undo,
}
// Bounded, canonical protobuf. Reject unknown, repeated singular, wrong-wire
// and oversized fields before prost owns any source Card or attachment list.
fn preflight(mut raw: &[u8], kind: FieldKind, fields: &mut usize) -> Result<()> {
    let mut seen = 0u16;
    let mut attachments = 0usize;
    while !raw.is_empty() {
        *fields = fields
            .checked_sub(1)
            .ok_or("cards-edit facts field budget")?;
        let (tag, wire) = decode_key(&mut raw)?;
        let max = match kind {
            FieldKind::Facts => 7,
            FieldKind::Attachment => 5,
            FieldKind::Undo => 2,
        };
        if tag == 0
            || tag > max
            || (kind != FieldKind::Facts || tag != 6) && {
                let bit = 1u16 << tag;
                let duplicate = seen & bit != 0;
                seen |= bit;
                duplicate
            }
        {
            return Err("cards-edit facts field".into());
        }
        if kind == FieldKind::Facts && tag == 6 {
            attachments += 1;
            if attachments > MAX_ATTACHMENTS {
                return Err("cards-edit attachment count".into());
            }
        }
        let delimited = match kind {
            FieldKind::Facts => matches!(tag, 2..=7),
            FieldKind::Attachment => tag != 4,
            FieldKind::Undo => false,
        };
        if wire
            != if delimited {
                WireType::LengthDelimited
            } else {
                WireType::Varint
            }
        {
            return Err("cards-edit facts wire".into());
        }
        if delimited {
            let length = usize::try_from(decode_varint(&mut raw)?)?;
            let limit = match (kind, tag) {
                (FieldKind::Facts, 2) | (FieldKind::Attachment, 1) => 256,
                (FieldKind::Facts, 3) => MAX_RECORD_BYTES,
                (FieldKind::Facts, 4) | (FieldKind::Attachment, 5) => 32,
                (FieldKind::Facts, 5) => MAX_REQUEST,
                (FieldKind::Facts, 6) => 20 * 1024,
                (FieldKind::Facts, 7) => 32,
                (FieldKind::Attachment, 2) => 16 * 1024,
                (FieldKind::Attachment, 3) => 1024,
                _ => return Err("cards-edit facts length field".into()),
            };
            if length > limit || length > raw.len() {
                return Err("cards-edit facts length".into());
            }
            let (payload, tail) = raw.split_at(length);
            raw = tail;
            match (kind, tag) {
                (FieldKind::Facts, 6) => preflight(payload, FieldKind::Attachment, fields)?,
                (FieldKind::Facts, 7) => preflight(payload, FieldKind::Undo, fields)?,
                _ => {}
            }
        } else {
            let value = decode_varint(&mut raw)?;
            if kind == FieldKind::Facts && tag == 1 && value > u64::from(u32::MAX) {
                return Err("cards-edit schema integer".into());
            }
        }
    }
    Ok(())
}
fn decode_facts(raw: &[u8]) -> Result<proto::Facts> {
    if raw.len() > task_evidence::MAX_INTENT_BYTES {
        return Err("cards-edit facts budget".into());
    }
    preflight(raw, FieldKind::Facts, &mut 256)?;
    let facts = proto::Facts::decode(raw)?;
    if facts.encode_to_vec() != raw {
        return Err("noncanonical cards-edit facts".into());
    }
    Ok(facts)
}
fn selected(facts: &proto::Facts) -> Result<Vec<Attachment>> {
    if facts.attachments.len() > MAX_ATTACHMENTS {
        return Err("cards-edit attachment count".into());
    }
    let mut ids = BTreeSet::new();
    let mut values = Vec::with_capacity(facts.attachments.len());
    for raw in &facts.attachments {
        let item = attachment_from_proto(raw)?;
        if !ids.insert(item.id.clone()) {
            return Err("duplicate cards-edit attachment".into());
        }
        values.push(item);
    }
    Ok(values)
}
fn observed_now(raw: &[u8]) -> Result<u64> {
    let request = cards_v2_codec::decode_request(raw)?;
    Ok(match &request.command {
        Command::Delete { now_ms } | Command::Restore { now_ms } => *now_ms,
        _ => 0,
    })
}
fn preview(text: &str) -> String {
    let mut value = text.to_owned();
    if value.len() > 16384 {
        let mut n = 16384;
        while !value.is_char_boundary(n) {
            n -= 1;
        }
        value.truncate(n);
    }
    value
}
fn expected(facts: &proto::Facts) -> Result<(Invocation, VersionedContentChange, Vec<u8>)> {
    if facts.schema_version != 1
        || facts.package_sha256.len() != 32
        || facts.request.len() > MAX_REQUEST
    {
        return Err("unsupported cards-edit facts".into());
    }
    identity(&facts.operation_id)?;
    let source = CardRecord::decode(&facts.source_card)?;
    let summary = source.summary();
    if summary.type_id != "org.morrow.idea"
        || summary.format_version != 2
        || source.encode() != facts.source_card
    {
        return Err("cards-edit requires an exact V2 idea".into());
    }
    let source_body = source.body();
    let previous = tasks_v2::decode(&summary.id, &summary.title, &source_body)?;
    let original_assets = source.attachments();
    if previous.assets.len() != original_assets.len()
        || previous
            .assets
            .iter()
            .zip(&original_assets)
            .any(|(asset, outer)| {
                asset.id != outer.id
                    || asset.name != outer.display_name
                    || asset.bytes != outer.byte_length
            })
    {
        return Err("cards-edit source attachment mismatch".into());
    }
    let request = cards_v2_codec::decode_request(&facts.request)?;
    if request.id != summary.id
        || request.title != summary.title
        || request.properties != source_body
        || cards_v2_codec::encode_request(
            &summary.id,
            &summary.title,
            &source_body,
            &request.command,
        )? != facts.request
    {
        return Err("cards-edit request differs from source".into());
    }
    let attachments = selected(facts)?;
    let editing = matches!(&request.command, Command::Edit(_));
    if !editing && attachments != original_assets {
        return Err("non-edit cards action changed attachments".into());
    }
    match &request.command {
        Command::Restore { now_ms } => {
            let undo = facts.undo.as_ref().ok_or("missing cards-edit undo")?;
            let deadline = previous
                .deleted_at
                .checked_add(8000)
                .ok_or("cards-edit undo deadline overflow")?;
            if !previous.deleted
                || *now_ms == 0
                || undo.revision != summary.revision
                || undo.deadline != deadline
                || *now_ms < previous.deleted_at
                || *now_ms >= deadline
            {
                return Err("cards-edit restore window or source".into());
            }
        }
        Command::Delete { now_ms } => {
            if facts.undo.is_some() || *now_ms == 0 {
                return Err("cards-edit delete time or undo".into());
            }
        }
        _ if facts.undo.is_some() => return Err("undo for non-restore cards action".into()),
        _ => {}
    }
    let output_bytes = cards_v2_codec::process(&facts.request)?;
    let output = cards_v2_codec::decode_response(&output_bytes)?;
    let next = tasks_v2::decode(&summary.id, &output.title, &output.properties)?;
    if next.assets.len() != attachments.len()
        || next.assets.iter().zip(&attachments).any(|(asset, outer)| {
            asset.id != outer.id
                || asset.name != outer.display_name
                || asset.bytes != outer.byte_length
        })
    {
        return Err("cards-edit output attachment selection mismatch".into());
    }
    let change = VersionedContentChange {
        operation_id: facts.operation_id.clone(),
        source_card: facts.source_card.clone(),
        title: output.title,
        body: output.properties,
        preview_text: if editing {
            preview(&next.description)
        } else {
            summary.preview_text
        },
        attachments: editing.then_some(attachments),
    };
    change.propose(&source)?;
    transaction::versioned_content_command(&change)?;
    let invocation = Invocation::new_transform(
        &facts.operation_id,
        Transform {
            handler: "workbench.cards.v2".into(),
            input_type: "morrow.workbench.cards.request.v2".into(),
            output_type: "morrow.workbench.cards.response.v2".into(),
            input: facts.request.clone(),
        },
    )?;
    Ok((invocation, change, output_bytes))
}

/// Pure projection from pinned host facts and actual captured bytes.
pub fn derive(evidence: &Evidence) -> Result<ProjectedEdit> {
    let actual = evidence.data();
    let batch = actual.batch.as_ref().ok_or("missing cards-edit batch")?;
    if actual.schema_version != task_evidence::BATCH_VERSION
        || batch.intent_type != INTENT_TYPE
        || batch.observations.len() != 1
        || batch.total_fuel != FUEL
    {
        return Err("unsupported cards-edit batch".into());
    }
    let facts = decode_facts(&batch.intent)?;
    let package = Package::decode(&actual.package_archive)?;
    if facts.package_sha256 != package.digest() {
        return Err("cards-edit package substitution".into());
    }
    let (invocation, change, expected_output) = expected(&facts)?;
    let observation = &batch.observations[0];
    let budget = observation
        .budget
        .as_ref()
        .ok_or("missing cards-edit budget")?;
    if observation.invocation != invocation.bytes()
        || observation.fault != 0
        || observation.exit_code != Some(0)
        || observation.observed_host_calls != 0
        || observation.backend != task_evidence::BACKEND
        || budget.fuel > FUEL
        || budget.memory_bytes > MEMORY
        || budget.host_calls > 16
    {
        return Err("cards-edit observation mismatch".into());
    }
    let output = invocation.verify_output(&observation.completion)?;
    if output.bytes != expected_output {
        return Err("cards-edit output differs from frozen contract".into());
    }
    let card = change.propose(&change.source()?)?;
    let command = transaction::versioned_content_command(&change)?;
    let observed_now = observed_now(&facts.request)?;
    Ok(ProjectedEdit {
        facts,
        change,
        card,
        command,
        evidence: evidence.clone(),
        package_digest: package.digest(),
        observed_now,
    })
}
pub fn verify_commit(
    commit: &transaction::proto::Commit,
    evidence: &Evidence,
) -> Result<ProjectedEdit> {
    let projected = derive(evidence)?;
    let summary = projected.card.summary();
    if commit.schema_version != 2
        || commit.task_evidence_sha256 != vec![evidence.digest().to_vec()]
        || commit.command != projected.command
        || commit.command_sha256 != Sha256::digest(&projected.command).as_slice()
        || commit.operation_id != projected.change.operation_id
        || commit.event_id != projected.change.operation_id
        || commit.card_id != summary.id
        || commit.revision != summary.revision
        || commit.content_sha256 != Sha256::digest(projected.card.encode()).as_slice()
        || commit.attachment_sha256
            != projected
                .card
                .attachments()
                .iter()
                .map(|item| item.sha256.to_vec())
                .collect::<Vec<_>>()
    {
        return Err("cards-edit projection differs from commit".into());
    }
    Ok(projected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use morrow_core::{
        content_migration::ContentMigration, plugin_package::proto::TransformHandler,
        task_evidence::proto::ExecutionBudget,
    };
    use morrow_workbench_plugin::{
        Asset, Idea,
        cards_v2::{self, Fields},
        persistence,
        tasks_v2::Baseline,
    };

    fn fixture() -> (CardRecord, Package) {
        let idea = Idea {
            id: "card".into(),
            title: "Original".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: vec!["one".into()],
            ..Default::default()
        };
        let old_body = persistence::encode(&idea, None).unwrap();
        let original = CardRecord::new(
            &idea.id,
            "org.morrow.idea",
            1,
            &idea.title,
            old_body.clone(),
        )
        .unwrap();
        let base = Baseline::capture("card", 1, &old_body).unwrap();
        let body = tasks_v2::migrate(&base, "card", 1, "Original", &old_body).unwrap();
        let source = ContentMigration {
            operation_id: "migration".into(),
            source_card: original.encode(),
            target_format_version: 2,
            body,
            preview_text: String::new(),
        }
        .propose(&original)
        .unwrap();
        let wasm = b"\0asm\x01\0\0\0";
        let package = Package::build(
            Package::manifest_for_transform(
                "test.card-edit",
                "1.0.0",
                wasm,
                vec![TransformHandler {
                    handler: "workbench.cards.v2".into(),
                    input_type: "morrow.workbench.cards.request.v2".into(),
                    output_type: "morrow.workbench.cards.response.v2".into(),
                    max_input_bytes: 65536,
                    max_output_bytes: 65536,
                }],
            ),
            wasm,
        )
        .unwrap();
        (source, package)
    }
    fn synthetic_single(package: &Package, invocation: &Invocation, output: &[u8]) -> Evidence {
        task_evidence::encode(TaskEvidence {
            schema_version: task_evidence::VERSION,
            package_archive: package.archive().to_vec(),
            invocation: invocation.bytes().to_vec(),
            budget: Some(ExecutionBudget {
                fuel: FUEL,
                memory_bytes: MEMORY,
                host_calls: 16,
            }),
            backend: task_evidence::BACKEND.into(),
            completion: invocation.output_completion(output).unwrap(),
            fault: 0,
            exit_code: Some(0),
            observed_host_calls: 0,
            fuel_remaining: FUEL - 1,
            batch: None,
        })
        .unwrap()
    }

    #[test]
    fn canonical_facts_and_exact_captured_projection() {
        let (source, package) = fixture();
        let action = Command::SetFavorite(true);
        let plan = Plan::prepare(&source, &package, "favorite", &action, &[], None).unwrap();
        let facts = decode_facts(&plan.intent).unwrap();
        assert_eq!(facts.source_card, source.encode());
        let output = cards_v2_codec::process(&facts.request).unwrap();
        // Synthetic completion checks only the pure projection, not guest execution.
        let actual = synthetic_single(&package, plan.invocation(), &output);
        let projected = plan.capture(&actual).unwrap();
        let hex_hash = |raw: &[u8]| -> String {
            Sha256::digest(raw)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        };
        // Contract 1 fixture: these literals pin canonical request, guest
        // response and persistent command bytes for historical derivation.
        assert_eq!(
            hex_hash(&facts.request),
            "34c9950c69c97d707d563bab177c2715301aee2bef0bd0b9500bb691731e0e10"
        );
        assert_eq!(
            hex_hash(&output),
            "76797ffd5aa5c9672fbf7b1063f83937ff90b3d03cd6de4e09ca190eab815292"
        );
        assert_eq!(
            hex_hash(projected.command()),
            "b499053aeeac64c715f245809955fb9f5aea93b9fc911507b4fb038521497446"
        );
        assert_eq!(projected.source_card(), source.encode());
        assert_eq!(projected.observed_now(), 0);
        assert_eq!(projected.card().summary().revision, 3);
        assert!(
            tasks_v2::decode("card", "Original", &projected.card().body())
                .unwrap()
                .favorite
        );
        assert!(projected.matches_intent("favorite", 2, &action).unwrap());
        assert!(!projected.matches_intent("other", 2, &action).unwrap());
        assert!(!projected.matches_intent("favorite", 3, &action).unwrap());
        assert!(
            !projected
                .matches_intent("favorite", 2, &Command::SetFavorite(false))
                .unwrap()
        );
        let event = transaction::encode_commit_with_evidence(
            projected.command().to_vec(),
            projected.card(),
            &[projected.evidence().digest()],
        )
        .unwrap();
        let (commit, _) = transaction::decode_commit(&event).unwrap();
        verify_commit(&commit, projected.evidence()).unwrap();

        let mut duplicate = plan.intent.clone();
        duplicate.extend_from_slice(&[0x12, 1, b'x']);
        assert!(decode_facts(&duplicate).is_err());
        let mut unknown = plan.intent.clone();
        unknown.extend_from_slice(&[0x40, 1]);
        assert!(decode_facts(&unknown).is_err());
        let mut noncanonical = vec![0x08, 0x81, 0x00];
        noncanonical.extend_from_slice(&plan.intent[2..]);
        assert!(decode_facts(&noncanonical).is_err());
    }

    #[test]
    fn attachment_selection_and_restore_facts_are_strict() {
        let (source, package) = fixture();
        let asset = Asset {
            id: "asset-new".into(),
            name: "new.txt".into(),
            kind: "file".into(),
            bytes: 7,
        };
        let fields = Fields {
            title: "Edited".into(),
            description: "new body".into(),
            hypothesis: String::new(),
            conclusion: String::new(),
            icon: 0,
            color: 0,
            assets: vec![asset.clone()],
        };
        let selected = Attachment {
            id: asset.id.clone(),
            display_name: asset.name.clone(),
            media_type: "application/octet-stream".into(),
            byte_length: asset.bytes,
            sha256: [7; 32],
        };
        let action = Command::Edit(fields);
        let plan = Plan::prepare(
            &source,
            &package,
            "edit",
            &action,
            &[selected.clone()],
            None,
        )
        .unwrap();
        let facts = decode_facts(&plan.intent).unwrap();
        assert_eq!(
            expected(&facts).unwrap().1.attachments,
            Some(vec![selected.clone()])
        );
        let mut mismatch = selected.clone();
        mismatch.display_name = "other.txt".into();
        assert!(Plan::prepare(&source, &package, "bad-asset", &action, &[mismatch], None).is_err());
        assert!(
            Plan::prepare(
                &source,
                &package,
                "injected-asset",
                &Command::SetFavorite(true),
                &[selected],
                None,
            )
            .is_err()
        );

        let deletion = cards_v2::apply(
            "card",
            "Original",
            &source.body(),
            &Command::Delete { now_ms: 100 },
        )
        .unwrap();
        let deleted = VersionedContentChange {
            operation_id: "delete".into(),
            source_card: source.encode(),
            title: deletion.title,
            body: deletion.properties,
            preview_text: String::new(),
            attachments: None,
        }
        .propose(&source)
        .unwrap();
        let restore = Command::Restore { now_ms: 150 };
        let valid = Undo {
            revision: 3,
            deadline: 8100,
        };
        let plan =
            Plan::prepare(&deleted, &package, "restore", &restore, &[], Some(valid)).unwrap();
        let facts = decode_facts(&plan.intent).unwrap();
        assert_eq!(expected(&facts).unwrap().1.source_card, deleted.encode());
        assert!(Plan::prepare(&deleted, &package, "no-undo", &restore, &[], None).is_err());
        assert!(
            Plan::prepare(
                &deleted,
                &package,
                "wrong-deadline",
                &restore,
                &[],
                Some(Undo {
                    revision: 3,
                    deadline: 8099
                }),
            )
            .is_err()
        );
        assert!(
            Plan::prepare(
                &deleted,
                &package,
                "wrong-revision",
                &restore,
                &[],
                Some(Undo {
                    revision: 2,
                    deadline: 8100
                }),
            )
            .is_err()
        );
        assert!(
            Plan::prepare(
                &source,
                &package,
                "delete-with-undo",
                &Command::Delete { now_ms: 100 },
                &[],
                Some(valid),
            )
            .is_err()
        );
    }
}
