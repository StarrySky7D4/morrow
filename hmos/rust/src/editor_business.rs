//! Strict editor submissions in the isolated, trusted development adapter.
//! One ordinary Core transaction owns the final body, tasks, assets and compact
//! marker. This is neither protected capture nor a business-draft handoff.
use crate::{
    CardView, Engine, Reply, Request, Result, TaskView, attachment_bridge, create_todos,
    draft_bridge, editor_draft, editor_field, err, hex, unhex,
};
use morrow_core::{
    content::{Attachment, CardRecord},
    lifecycle::GrantKind,
    transaction::{self, Receipt},
    versioned_content_change::VersionedContentChange,
};
use morrow_editor_draft_model::proto as draft_proto;
use morrow_workbench_plugin::{cards_v2, tasks_v2};
use prost::{
    Message,
    encoding::{
        DecodeContext, WireType, decode_key, decode_varint, encode_key, encode_varint, skip_field,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Fresh source inventory checked before registration: no shared or adapter
/// Rust/proto used 50003. 50001 and every unrelated field remain untouched.
const MARKER_FIELD: u32 = 50_003;
const MARKER_DOMAIN: &[u8] = b"morrow.hmos.editor-wire.v1\0";
const PUBLICATION_DOMAIN: &[u8] = b"morrow.hmos.editor-publication.v1\0";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SaveEnvelope {
    pub request_json: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InspectEnvelope {
    pub request_json: String,
    pub expected_revision: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Create,
    Edit,
    ContinuedTodos,
}
impl Mode {
    fn byte(self) -> u8 {
        match self {
            Self::Create => 1,
            Self::Edit => 2,
            Self::ContinuedTodos => 3,
        }
    }
    fn from_byte(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::Create),
            2 => Ok(Self::Edit),
            3 => Ok(Self::ContinuedTodos),
            _ => Err("EditorMarkerMode".into()),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Business {
    action: String,
    id: String,
    operation: String,
    source: String,
    title: String,
    description: String,
    hypothesis: String,
    conclusion: String,
    todos: String,
    category: String,
    stage: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub draft_id: String,
    pub generation: String,
    pub save_operation: String,
    pub request_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Baseline {
    operation: String,
    revision: String,
    command_sha256: String,
    content_sha256: String,
    request_sha256: String,
    publication_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Continuation {
    root_request_json: String,
    baseline: Baseline,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Submission {
    schema_version: u32,
    mode: Mode,
    business: Business,
    publication: Publication,
    continuation: Option<Continuation>,
}
#[derive(Debug, Serialize)]
pub struct CommitView {
    pub commit_status: &'static str,
    pub qualification: &'static str,
    pub card_id: String,
    pub operation: String,
    pub source_revision: String,
    pub revision: String,
    pub event_id: String,
    pub command_sha256: String,
    pub content_sha256: String,
    pub request_sha256: String,
    pub publication_sha256: String,
    pub publication: Publication,
    pub historical_card: Option<CardView>,
    pub live_matches: bool,
    pub live_revision: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Marker {
    mode: Mode,
    wire: [u8; 32],
    publication: [u8; 32],
}
struct Historical {
    command: Vec<u8>,
    receipt: Receipt,
    result: CardRecord,
    source: Option<CardRecord>,
}
struct Prepared {
    command: Vec<u8>,
    result: CardRecord,
    change: Option<VersionedContentChange>,
    marker: Marker,
}

fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn frame(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}
fn digest(text: &str) -> Result<[u8; 32]> {
    unhex(text)?
        .try_into()
        .map_err(|_| "EditorProofDigest".into())
}
fn positive(text: &str) -> Result<u64> {
    let n = draft_bridge::number(text)?;
    if n == 0 {
        Err("EditorProofRevision".into())
    } else {
        Ok(n)
    }
}
fn identity(value: &str) -> Result<()> {
    // Reuse public Core validation instead of relaxing its private identity gate.
    morrow_core::runtime::Command::ReadSummary {
        request_id: value.into(),
        card_id: value.into(),
    }
    .validate()
    .map_err(err)
}
fn parse(wire: &str) -> Result<Submission> {
    if wire.len() > crate::LIMIT {
        return Err("EditorSubmissionBytesLimit".into());
    }
    let value: Submission = serde_json::from_str(wire).map_err(|_| "EditorSubmissionSchema")?;
    identity(&value.business.id)?;
    identity(&value.business.operation)?;
    Ok(value)
}
fn validate_submission(value: &Submission) -> Result<()> {
    let r = &value.business;
    if value.schema_version != 1
        || r.id.starts_with("morrow-host-")
        || r.id.is_empty()
        || r.operation.is_empty()
        || (value.mode == Mode::Create
            && (r.action != "create" || !r.source.is_empty() || value.continuation.is_some()))
        || (value.mode != Mode::Create && (r.action != "edit" || r.source.is_empty()))
        || (value.mode == Mode::Edit && (!r.todos.is_empty() || value.continuation.is_some()))
        || (value.mode == Mode::ContinuedTodos && value.continuation.is_none())
    {
        return Err("EditorSubmissionMode".into());
    }
    identity(&r.id)?;
    identity(&r.operation)?;
    identity(&value.publication.draft_id)?;
    identity(&value.publication.save_operation)?;
    positive(&value.publication.generation)?;
    digest(&value.publication.request_sha256)?;
    Ok(())
}
fn publication_hash(card: &str, proof: &Publication) -> Result<[u8; 32]> {
    let mut sha = Sha256::new();
    sha.update(PUBLICATION_DOMAIN);
    frame(&mut sha, card.as_bytes());
    frame(&mut sha, proof.draft_id.as_bytes());
    sha.update(positive(&proof.generation)?.to_le_bytes());
    frame(&mut sha, proof.save_operation.as_bytes());
    sha.update(digest(&proof.request_sha256)?);
    Ok(sha.finalize().into())
}
// Bounded scanner preserves the exact raw bytes of unrelated Properties fields.
fn fields(mut input: &[u8]) -> Result<Vec<(u32, WireType, &[u8], &[u8])>> {
    let mut result = Vec::new();
    while !input.is_empty() {
        let start = input;
        let (tag, wire) = decode_key(&mut input).map_err(err)?;
        let after_key = input;
        skip_field(wire, tag, &mut input, DecodeContext::default()).map_err(err)?;
        let raw = &start[..start.len() - input.len()];
        let mut payload = &after_key[..after_key.len() - input.len()];
        if wire == WireType::LengthDelimited {
            let length = decode_varint(&mut payload).map_err(err)?;
            if length != payload.len() as u64 {
                return Err("EditorPropertiesFieldLength".into());
            }
        }
        result.push((tag, wire, raw, payload));
    }
    Ok(result)
}
fn unique_field(body: &[u8], wanted: u32) -> Result<Option<Vec<u8>>> {
    let mut found = None;
    for (tag, wire, _, payload) in fields(body)? {
        if tag != wanted {
            continue;
        }
        if found.is_some() || wire != WireType::LengthDelimited {
            return Err("EditorRegisteredFieldCollision".into());
        }
        found = Some(payload.to_vec());
    }
    Ok(found)
}
fn marker(body: &[u8]) -> Result<Option<Marker>> {
    let Some(bytes) = unique_field(body, MARKER_FIELD)? else {
        return Ok(None);
    };
    if bytes.len() != MARKER_DOMAIN.len() + 66
        || !bytes.starts_with(MARKER_DOMAIN)
        || bytes[MARKER_DOMAIN.len()] != 1
    {
        return Err("EditorMarkerSchema".into());
    }
    let at = MARKER_DOMAIN.len() + 2;
    Ok(Some(Marker {
        mode: Mode::from_byte(bytes[at - 1])?,
        wire: bytes[at..at + 32].try_into().unwrap(),
        publication: bytes[at + 32..].try_into().unwrap(),
    }))
}
fn set_marker(body: &[u8], identity: &Marker) -> Result<Vec<u8>> {
    marker(body)?;
    let mut out = Vec::with_capacity(body.len() + 128);
    for (tag, _, raw, _) in fields(body)? {
        if tag != MARKER_FIELD {
            out.extend_from_slice(raw);
        }
    }
    encode_key(MARKER_FIELD, WireType::LengthDelimited, &mut out);
    encode_varint((MARKER_DOMAIN.len() + 66) as u64, &mut out);
    out.extend_from_slice(MARKER_DOMAIN);
    out.push(1);
    out.push(identity.mode.byte());
    out.extend_from_slice(&identity.wire);
    out.extend_from_slice(&identity.publication);
    Ok(out)
}
fn historical(engine: &Engine, card: &str, operation: &str) -> Result<Option<Historical>> {
    let Some((commit, receipt)) = engine
        .host
        .store_local()
        .operation_commit(card, operation)
        .map_err(err)?
    else {
        return Ok(None);
    };
    let command = transaction::decode_command(&commit.command).map_err(err)?;
    let (result, source) = match command.action.ok_or("EditorHistoricalCommand")? {
        transaction::proto::command::Action::CreateCard(raw) => {
            let card = CardRecord::decode(&raw).map_err(err)?;
            if card.encode() != raw {
                return Err("EditorHistoricalEncoding".into());
            }
            (card, None)
        }
        transaction::proto::command::Action::SetVersionedContent(value) => {
            let source = CardRecord::decode(&value.source_card).map_err(err)?;
            let attachments = value
                .attachments
                .map(|list| {
                    list.items
                        .into_iter()
                        .map(|v| {
                            Ok(Attachment {
                                id: v.id,
                                display_name: v.display_name,
                                media_type: v.media_type,
                                byte_length: v.byte_length,
                                sha256: v
                                    .sha256
                                    .try_into()
                                    .map_err(|_| "EditorHistoricalAttachment")?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?;
            let change = VersionedContentChange {
                operation_id: operation.into(),
                source_card: value.source_card,
                title: value.title,
                body: value.body,
                preview_text: value.preview_text,
                attachments,
            };
            (change.propose(&source).map_err(err)?, Some(source))
        }
        _ => return Err("EditorHistoricalCommand".into()),
    };
    if result.summary().id != card
        || result.summary().revision != receipt.revision
        || hash(&result.encode()) != receipt.content_sha256
    {
        return Err("EditorHistoricalResultIntegrity".into());
    }
    Ok(Some(Historical {
        command: commit.command,
        receipt,
        result,
        source,
    }))
}
fn publication(
    engine: &Engine,
    value: &Submission,
    source: &[u8],
    historical_only: bool,
) -> Result<editor_draft::PublishedAssets> {
    let r = &value.business;
    let proof = &value.publication;
    let published = if historical_only {
        editor_draft::published_assets_history(
            &engine.host,
            &r.id,
            &proof.draft_id,
            positive(&proof.generation)?,
            &proof.save_operation,
            source,
        )?
    } else {
        editor_draft::publish_assets(
            &engine.host,
            &r.id,
            &proof.draft_id,
            positive(&proof.generation)?,
            &proof.save_operation,
            source,
        )?
    };
    // The public publication gate validates the canonical complete historical
    // Slot (including fork lineage). Read its request identity from that SAME
    // immutable save, never from a newer live draft. This key is the unchanged
    // editor_draft.rs host-owned journal derivation, not a guest identity.
    let mut key = Sha256::new();
    key.update((r.id.len() as u64).to_le_bytes());
    key.update(r.id.as_bytes());
    key.update(proof.draft_id.as_bytes());
    let journal = format!("morrow-host-editor-draft-{:x}", key.finalize());
    let (commit, _) = engine
        .host
        .store_local()
        .operation_commit(&journal, &proof.save_operation)
        .map_err(err)?
        .ok_or("EditorPublicationHistoryMissing")?;
    let command = transaction::decode_command(&commit.command).map_err(err)?;
    let body = match command.action.ok_or("EditorPublicationCommand")? {
        transaction::proto::command::Action::CreateCard(raw) => {
            CardRecord::decode(&raw).map_err(err)?.body()
        }
        transaction::proto::command::Action::SetContent(value) => value.body,
        _ => return Err("EditorPublicationCommand".into()),
    };
    let slot = draft_proto::Slot::decode(body.as_slice()).map_err(err)?;
    let original = slot.request.as_ref().ok_or("EditorPublicationRequest")?;
    if slot.encode_to_vec() != body
        || original.card_id != r.id
        || original.draft_id != proof.draft_id
        || original.operation_id != proof.save_operation
        || slot.generation != positive(&proof.generation)?
        || editor_draft::request_sha256(original) != proof.request_sha256
        || slot.source_card != source
        || original.values.as_ref() != Some(&published.values)
    {
        return Err("EditorPublicationProofMismatch".into());
    }
    let values = &published.values;
    for (field, raw, actual) in [
        ("title", &r.title, &values.title),
        ("description", &r.description, &values.description),
        ("hypothesis", &r.hypothesis, &values.hypothesis),
        ("conclusion", &r.conclusion, &values.conclusion),
        ("todos", &r.todos, &values.todos),
    ] {
        let actual = actual.as_ref().ok_or("EditorPublicationTextMissing")?;
        if actual.text != *raw {
            return Err("EditorPublicationValuesMismatch".into());
        }
        if actual.composing_start != -1 || actual.composing_end != -1 {
            return Err("EditorPublicationComposing".into());
        }
        let measurement = editor_field::inspect(field, raw);
        if !measurement.ok {
            return Err(measurement.error);
        }
    }
    if values.category != r.category || values.stage != r.stage {
        return Err("EditorPublicationValuesMismatch".into());
    }
    Ok(published)
}

fn continuation_root(engine: &Engine, value: &Submission, source: &CardRecord) -> Result<String> {
    let context = value
        .continuation
        .as_ref()
        .ok_or("EditorContinuationRequired")?;
    let root = parse(&context.root_request_json)?;
    validate_submission(&root)?;
    if root.mode != Mode::Create
        || root.business.id != value.business.id
        || root.continuation.is_some()
    {
        return Err("EditorContinuationRootMode".into());
    }
    let original = historical(engine, &root.business.id, &root.business.operation)?
        .ok_or("EditorContinuationRootMissing")?;
    let prepared = prepare(engine, &root, &context.root_request_json, true, true)?;
    if prepared.command != original.command
        || marker(&original.result.body())?.as_ref() != Some(&prepared.marker)
    {
        return Err("EditorContinuationRootMismatch".into());
    }
    let proof = &context.baseline;
    identity(&proof.operation)?;
    let base = historical(engine, &root.business.id, &proof.operation)?
        .ok_or("EditorContinuationBaselineMissing")?;
    let identity = marker(&base.result.body())?.ok_or("EditorContinuationBaselineUnqualified")?;
    if !matches!(identity.mode, Mode::Create | Mode::ContinuedTodos)
        || base.receipt.revision != positive(&proof.revision)?
        || hash(&base.command) != digest(&proof.command_sha256)?
        || base.receipt.content_sha256 != digest(&proof.content_sha256)?
        || identity.wire != digest(&proof.request_sha256)?
        || identity.publication != digest(&proof.publication_sha256)?
        || base.result.encode() != source.encode()
        || unique_field(&source.body(), create_todos::RAW_IDENTITY_FIELD)?
            != unique_field(&original.result.body(), create_todos::RAW_IDENTITY_FIELD)?
    {
        return Err("EditorContinuationBaselineMismatch".into());
    }
    match identity.mode {
        Mode::Create
            if proof.operation == root.business.operation && base.command == original.command => {}
        Mode::ContinuedTodos => {
            let prior = base.source.as_ref().ok_or("EditorContinuationCommand")?;
            let previous = marker(&prior.body())?.ok_or("EditorContinuationSourceUnqualified")?;
            if !matches!(previous.mode, Mode::Create | Mode::ContinuedTodos)
                || previous == identity
                || unique_field(&prior.body(), create_todos::RAW_IDENTITY_FIELD)?
                    != unique_field(&original.result.body(), create_todos::RAW_IDENTITY_FIELD)?
            {
                return Err("EditorContinuationCopiedMarker".into());
            }
        }
        _ => return Err("EditorContinuationCommand".into()),
    }
    let p = tasks_v2::decode(&value.business.id, &source.summary().title, &source.body())
        .map_err(err)?;
    if p.origin.is_some()
        || p.tasks.len() > create_todos::MAX_ROWS
        || p.tasks
            .iter()
            .map(|t| &t.text)
            .collect::<BTreeSet<_>>()
            .len()
            != p.tasks.len()
    {
        return Err("EditorContinuationTasksUnqualified".into());
    }
    Ok(root.business.operation)
}
fn continued_id(card: &str, root: &str, operation: &str, raw: &str, index: usize) -> String {
    let mut sha = Sha256::new();
    sha.update(b"morrow.hmos.continued-task-id.v1\0");
    frame(&mut sha, card.as_bytes());
    frame(&mut sha, root.as_bytes());
    frame(&mut sha, operation.as_bytes());
    frame(&mut sha, raw.as_bytes());
    sha.update((index as u64).to_le_bytes());
    format!("task-hmos-continued-{:x}", sha.finalize())
}
fn length_field(tag: u32, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    encode_key(tag, WireType::LengthDelimited, &mut out);
    encode_varint(bytes.len() as u64, &mut out);
    out.extend_from_slice(bytes);
    out
}
fn varint_field(tag: u32, value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    encode_key(tag, WireType::Varint, &mut out);
    encode_varint(value, &mut out);
    out
}
struct TodoProjection {
    tasks: Vec<Vec<u8>>,
    newly_retired: Vec<String>,
}
fn project_todos(
    card: &str,
    title: &str,
    body: &[u8],
    root: &str,
    operation: &str,
    raw: &str,
) -> Result<TodoProjection> {
    let labels = create_todos::normalized_labels(raw)?;
    let p = tasks_v2::decode(card, title, body).map_err(err)?;
    if p.origin.is_some()
        || p.tasks
            .iter()
            .map(|t| &t.text)
            .collect::<BTreeSet<_>>()
            .len()
            != p.tasks.len()
    {
        return Err("EditorContinuationTasksUnqualified".into());
    }
    let by_label: BTreeMap<_, _> = p
        .tasks
        .iter()
        .map(|t| (t.text.as_str(), t.id.as_str()))
        .collect();
    let keep: BTreeSet<_> = labels.iter().map(String::as_str).collect();
    let newly_retired = p
        .tasks
        .iter()
        .filter(|task| !keep.contains(task.text.as_str()))
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    let existing: Vec<_> = fields(body)?
        .into_iter()
        .filter(|field| field.0 == 20)
        .map(|field| field.3.to_vec())
        .collect();
    if existing.len() != p.tasks.len() {
        return Err("EditorTaskFieldCount".into());
    }
    let by_id: BTreeMap<_, _> = p
        .tasks
        .iter()
        .zip(existing)
        .map(|(task, bytes)| (task.id.as_str(), bytes))
        .collect();
    let mut used: BTreeSet<_> = p
        .tasks
        .iter()
        .map(|task| task.id.clone())
        .chain(p.retired_task_ids.iter().cloned())
        .collect();
    let mut tasks = Vec::new();
    for (index, label) in labels.into_iter().enumerate() {
        let bytes = if let Some(id) = by_label.get(label.as_str()) {
            by_id[*id].clone()
        } else {
            let id = continued_id(card, root, operation, raw, index);
            if !used.insert(id.clone()) {
                return Err("EditorTaskIdAlreadyUsed".into());
            }
            tasks_v2::Task {
                id,
                text: label,
                ..Default::default()
            }
            .encode_to_vec()
        };
        tasks.push(bytes);
    }
    Ok(TodoProjection {
        tasks,
        newly_retired,
    })
}
fn final_properties(
    old: &CardRecord,
    r: &Business,
    selected: &[morrow_workbench_plugin::Asset],
    todos: Option<TodoProjection>,
) -> Result<Vec<u8>> {
    // Compose the COMPLETE final projection before applying its 64KiB budget.
    // Applying task/text changes one at a time can reject a final-fit request
    // while it temporarily still contains fields that this SAME save removes.
    let original = old.body();
    let p = tasks_v2::decode(&r.id, &old.summary().title, &original).map_err(err)?;
    let raw_assets: Vec<_> = fields(&original)?
        .into_iter()
        .filter(|field| field.0 == 10)
        .map(|field| field.3.to_vec())
        .collect();
    if raw_assets.len() != p.assets.len() {
        return Err("EditorAssetFieldCount".into());
    }
    let mut out = Vec::new();
    for (tag, _, raw, _) in fields(&original)? {
        if matches!(tag, 2 | 3 | 4 | 5 | 6 | 10 | 11 | 12)
            || tag == MARKER_FIELD
            || tag == 20 && todos.is_some()
        {
            continue;
        }
        out.extend_from_slice(raw);
    }
    for (tag, text) in [
        (2, &r.description),
        (3, &r.category),
        (4, &r.stage),
        (5, &r.hypothesis),
        (6, &r.conclusion),
    ] {
        out.extend(length_field(tag, text.as_bytes()));
    }
    for asset in selected {
        let prior = p.assets.iter().position(|value| value.id == asset.id);
        let bytes = if let Some(index) = prior {
            let previous = &p.assets[index];
            if previous.name == asset.name
                && previous.kind == asset.kind
                && previous.bytes == asset.bytes
            {
                raw_assets[index].clone()
            } else {
                let mut bytes = Vec::new();
                for (tag, _, raw, _) in fields(&raw_assets[index])? {
                    if !matches!(tag, 1..=4) {
                        bytes.extend_from_slice(raw);
                    }
                }
                bytes.extend(length_field(1, asset.id.as_bytes()));
                bytes.extend(length_field(2, asset.name.as_bytes()));
                bytes.extend(length_field(3, asset.kind.as_bytes()));
                bytes.extend(varint_field(4, asset.bytes));
                bytes
            }
        } else {
            [
                length_field(1, asset.id.as_bytes()),
                length_field(2, asset.name.as_bytes()),
                length_field(3, asset.kind.as_bytes()),
                varint_field(4, asset.bytes),
            ]
            .concat()
        };
        out.extend(length_field(10, &bytes));
    }
    out.extend(varint_field(11, p.icon as u64));
    out.extend(varint_field(12, p.color as u64));
    if let Some(todos) = todos {
        for bytes in todos.tasks {
            out.extend(length_field(20, &bytes));
        }
        for id in todos.newly_retired {
            out.extend(length_field(22, id.as_bytes()));
        }
    }
    Ok(out)
}
fn prepare(
    engine: &Engine,
    value: &Submission,
    wire: &str,
    historical_only: bool,
    with_marker: bool,
) -> Result<Prepared> {
    let r = &value.business;
    let source = unhex(&r.source)?;
    let published = publication(engine, value, &source, historical_only)?;
    let identity = Marker {
        mode: value.mode,
        wire: hash(wire.as_bytes()),
        publication: publication_hash(&r.id, &value.publication)?,
    };
    if value.mode == Mode::Create {
        let todos = create_todos::prepare(&r.id, &r.operation, &r.todos)?;
        let p = tasks_v2::Properties {
            version: 2,
            description: r.description.clone(),
            category: r.category.clone(),
            stage: r.stage.clone(),
            hypothesis: r.hypothesis.clone(),
            conclusion: r.conclusion.clone(),
            tasks: todos.tasks.clone(),
            ..Default::default()
        };
        let mut body = cards_v2::apply(
            &r.id,
            &r.title,
            &p.encode_to_vec(),
            &cards_v2::Command::Edit(cards_v2::Fields {
                title: r.title.clone(),
                description: r.description.clone(),
                hypothesis: r.hypothesis.clone(),
                conclusion: r.conclusion.clone(),
                icon: p.icon as u16,
                color: p.color,
                assets: published.assets,
            }),
        )
        .map_err(err)?
        .properties;
        todos.append_raw_identity(&mut body);
        if with_marker {
            body = set_marker(&body, &identity)?;
        }
        tasks_v2::decode(&r.id, &r.title, &body).map_err(err)?;
        let result = CardRecord::new_with_attachments(
            &r.id,
            "idea",
            2,
            &r.title,
            body,
            &published.attachments,
        )
        .map_err(err)?;
        return Ok(Prepared {
            command: transaction::create_command(&r.operation, &result).map_err(err)?,
            result,
            change: None,
            marker: identity,
        });
    }
    let old = CardRecord::decode(&source).map_err(err)?;
    let summary = old.summary();
    if old.encode() != source
        || summary.id != r.id
        || summary.type_id != "idea"
        || summary.format_version != 2
    {
        return Err("EditorSourceIdentity".into());
    }
    let p = tasks_v2::decode(&r.id, &summary.title, &old.body()).map_err(err)?;
    if p.deleted {
        return Err("EditorSourceDeleted".into());
    }
    let todos = if value.mode == Mode::ContinuedTodos {
        let root = continuation_root(engine, value, &old)?;
        Some(project_todos(
            &r.id,
            &summary.title,
            &old.body(),
            &root,
            &r.operation,
            &r.todos,
        )?)
    } else {
        if marker(&old.body())?
            .is_some_and(|m| matches!(m.mode, Mode::Create | Mode::ContinuedTodos))
        {
            return Err("EditorOwnedTodosRequireContinuation".into());
        }
        None
    };
    // Legacy inspection rebuilds the OLD route, whose edit preserved category
    // and stage. Strict saves publish every submitted common field atomically.
    let mut body = if with_marker {
        final_properties(&old, r, &published.assets, todos)?
    } else {
        cards_v2::apply(
            &r.id,
            &summary.title,
            &old.body(),
            &cards_v2::Command::Edit(cards_v2::Fields {
                title: r.title.clone(),
                description: r.description.clone(),
                hypothesis: r.hypothesis.clone(),
                conclusion: r.conclusion.clone(),
                icon: p.icon as u16,
                color: p.color,
                assets: published.assets,
            }),
        )
        .map_err(err)?
        .properties
    };
    if with_marker {
        body = set_marker(&body, &identity)?;
    }
    tasks_v2::decode(&r.id, &r.title, &body).map_err(err)?;
    let change = VersionedContentChange {
        operation_id: r.operation.clone(),
        source_card: source,
        title: r.title.clone(),
        body,
        preview_text: String::new(),
        attachments: Some(published.attachments),
    };
    let result = change.propose(&old).map_err(err)?;
    Ok(Prepared {
        command: transaction::versioned_content_command(&change).map_err(err)?,
        result,
        change: Some(change),
        marker: identity,
    })
}

fn card_view(card: &CardRecord) -> Result<CardView> {
    let s = card.summary();
    if s.type_id != "idea" || s.format_version != 2 {
        return Err("EditorHistoricalFormat".into());
    }
    let p = tasks_v2::decode(&s.id, &s.title, &card.body()).map_err(err)?;
    let attachments = card.attachments();
    if p.assets.len() != attachments.len() {
        return Err("AttachmentMetadataMismatch".into());
    }
    let assets = p
        .assets
        .iter()
        .zip(&attachments)
        .map(|(asset, outer)| {
            if asset.id != outer.id
                || outer.display_name != asset.name
                || outer.byte_length != asset.bytes
            {
                return Err("AttachmentMetadataMismatch".into());
            }
            Ok(attachment_bridge::AssetView {
                id: asset.id.clone(),
                name: asset.name.clone(),
                kind: asset.kind.clone(),
                byte_length: asset.bytes.to_string(),
                sha256: hex(&outer.sha256),
                media_type: outer.media_type.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CardView {
        id: s.id,
        revision: s.revision.to_string(),
        source: hex(&card.encode()),
        title: s.title,
        description: p.description,
        hypothesis: p.hypothesis,
        conclusion: p.conclusion,
        category: p.category,
        stage: p.stage,
        favorite: p.favorite,
        deleted: p.deleted,
        deleted_at: p.deleted_at.to_string(),
        assets,
        tasks: p
            .tasks
            .into_iter()
            .map(|t| TaskView {
                id: t.id,
                text: t.text,
                completion: t.completion,
            })
            .collect(),
    })
}
fn view(
    value: &Submission,
    historical: &Historical,
    qualification: &'static str,
    identity: &Marker,
    live: Option<&CardRecord>,
) -> Result<CommitView> {
    Ok(CommitView {
        commit_status: "committed",
        qualification,
        card_id: value.business.id.clone(),
        operation: value.business.operation.clone(),
        source_revision: historical
            .source
            .as_ref()
            .map_or(0, |s| s.summary().revision)
            .to_string(),
        revision: historical.receipt.revision.to_string(),
        event_id: historical.receipt.event_id.clone(),
        command_sha256: hex(&hash(&historical.command)),
        content_sha256: hex(&historical.receipt.content_sha256),
        request_sha256: if qualification == "development_editor_wire_v1" {
            hex(&identity.wire)
        } else {
            String::new()
        },
        publication_sha256: if qualification == "development_editor_wire_v1" {
            hex(&identity.publication)
        } else {
            String::new()
        },
        publication: value.publication.clone(),
        historical_card: Some(card_view(&historical.result)?),
        live_matches: live.is_some_and(|card| card.encode() == historical.result.encode()),
        live_revision: live
            .map(|card| card.summary().revision.to_string())
            .unwrap_or_default(),
    })
}
fn reply(commit: CommitView, effect: &'static str) -> Result<Reply> {
    let result = Reply {
        ok: true,
        error: String::new(),
        cards: vec![],
        ids: vec![],
        drafts: vec![],
        markdown: crate::markdown::MarkdownDoc::default(),
        paste_text: String::new(),
        receipt_revision: commit.revision.clone(),
        profile: "development-unsealed",
        effect,
        imports: vec![],
        editor_commit: Some(commit),
    };
    if serde_json::to_vec(&result).map_err(err)?.len() > crate::LIMIT {
        return Err("EditorReplyBytesLimit".into());
    }
    Ok(result)
}
fn committed_reply(
    engine: &Engine,
    value: &Submission,
    original: &Historical,
    prepared: &Prepared,
    qualification: &'static str,
) -> Result<Reply> {
    let current = engine
        .host
        .store_local()
        .card(&value.business.id)
        .map_err(err)?;
    reply(
        view(
            value,
            original,
            qualification,
            &prepared.marker,
            current.as_ref(),
        )?,
        "committed",
    )
}
pub fn reject_other_envelope(r: &Request) -> Result<()> {
    if r.editor_save.is_some() && r.action != "editor_save"
        || r.editor_commit.is_some() && r.action != "editor_commit_inspect"
    {
        return Err("EditorEnvelopeActionMismatch".into());
    }
    Ok(())
}
fn route_is_empty(r: &Request) -> bool {
    r.path.is_empty()
        && r.operation.is_empty()
        && r.id.is_empty()
        && r.source.is_empty()
        && r.title.is_empty()
        && r.description.is_empty()
        && r.hypothesis.is_empty()
        && r.conclusion.is_empty()
        && r.todos.is_empty()
        && r.category.is_empty()
        && r.stage.is_empty()
        && r.task_id.is_empty()
        && r.order.is_empty()
        && r.text.is_empty()
        && r.section.is_empty()
        && r.filter.is_empty()
        && r.sort.is_empty()
        && !r.flag
        && r.now_ms.is_empty()
        && r.draft.is_none()
        && r.fork.is_none()
        && r.fork_retirement.is_none()
        && r.draft_id.is_empty()
        && r.generation.is_empty()
        && r.draft_operation.is_empty()
        && r.attachment_id.is_empty()
        && r.import_request.is_none()
}
pub fn execute(engine: &mut Engine, r: Request) -> Result<Reply> {
    reject_other_envelope(&r)?;
    if !route_is_empty(&r) {
        return Err("EditorOuterFields".into());
    }
    let inspect = r.action == "editor_commit_inspect";
    let (wire, expected_revision, outer_size) = if inspect {
        let envelope = r.editor_commit.ok_or("EditorInspectEnvelopeRequired")?;
        let revision = positive(&envelope.expected_revision)?;
        let size = serde_json::to_vec(
            &serde_json::json!({"action":"editor_commit_inspect", "editor_commit":&envelope}),
        )
        .map_err(err)?
        .len();
        (envelope.request_json, Some(revision), size)
    } else {
        let envelope = r.editor_save.ok_or("EditorSaveEnvelopeRequired")?;
        let size = serde_json::to_vec(
            &serde_json::json!({"action":"editor_save", "editor_save":&envelope}),
        )
        .map_err(err)?
        .len();
        (envelope.request_json, None, size)
    };
    if outer_size > crate::LIMIT {
        return Err("EditorEnvelopeBytesLimit".into());
    }
    engine.effect = "unknown";
    let value = parse(&wire)?;
    // Establish immutable commitment BEFORE publication/connection/current
    // admission; a later malformed proof never reclassifies it as absent.
    engine.effect = "unknown";
    let original = historical(engine, &value.business.id, &value.business.operation)?;
    engine.effect = if original.is_some() {
        "committed"
    } else {
        "not_committed"
    };
    validate_submission(&value)?;
    if let Some(original) = original {
        if expected_revision.is_some_and(|revision| revision != original.receipt.revision) {
            return Err("EditorReceiptRevisionMismatch".into());
        }
        let identity = marker(&original.result.body())?;
        let prepared = prepare(engine, &value, &wire, true, identity.is_some())?;
        if prepared.command != original.command {
            return Err("EditorOriginalCommandMismatch".into());
        }
        if identity
            .as_ref()
            .is_some_and(|stored| stored != &prepared.marker)
        {
            return Err("EditorOriginalWireMismatch".into());
        }
        if !inspect && identity.is_none() {
            return Err("EditorLegacyCommitUnqualified".into());
        }
        return committed_reply(
            engine,
            &value,
            &original,
            &prepared,
            if identity.is_some() {
                "development_editor_wire_v1"
            } else {
                "legacy_semantic_only"
            },
        );
    }
    if inspect {
        return reply(
            CommitView {
                commit_status: "absent",
                qualification: "absent",
                card_id: value.business.id,
                operation: value.business.operation,
                source_revision: String::new(),
                revision: String::new(),
                event_id: String::new(),
                command_sha256: String::new(),
                content_sha256: String::new(),
                request_sha256: String::new(),
                publication_sha256: String::new(),
                publication: value.publication,
                historical_card: None,
                live_matches: false,
                live_revision: String::new(),
            },
            "not_committed",
        );
    }
    let prepared = prepare(engine, &value, &wire, false, true)?;
    if value.mode == Mode::Create
        && engine.ids()?.len() >= 256
        && engine
            .host
            .store_local()
            .card(&value.business.id)
            .map_err(err)?
            .is_none()
    {
        return Err("DevelopmentCardLimit".into());
    }
    // Bound the full success JSON before admitting the one business mutation.
    let predicted = Historical {
        command: prepared.command.clone(),
        receipt: Receipt {
            operation_id: value.business.operation.clone(),
            card_id: value.business.id.clone(),
            revision: prepared.result.summary().revision,
            content_sha256: hash(&prepared.result.encode()),
            event_id: value.business.operation.clone(),
        },
        result: prepared.result.clone(),
        source: prepared
            .change
            .as_ref()
            .map(|change| change.source().map_err(err))
            .transpose()?,
    };
    reply(
        view(
            &value,
            &predicted,
            "development_editor_wire_v1",
            &prepared.marker,
            Some(&prepared.result),
        )?,
        "committed",
    )?;
    let start = engine.start;
    let clock = || {
        u64::try_from(start.elapsed().as_millis())
            .unwrap_or(u64::MAX - 1)
            .saturating_add(1)
    };
    let now = clock();
    let mut connection = engine.host.connect().map_err(err)?;
    let result = (|| -> Result<()> {
        let kind = if prepared.change.is_some() {
            GrantKind::EditContent
        } else {
            GrantKind::CreateContent
        };
        engine
            .host
            .grant(
                &mut connection,
                kind,
                &value.business.id,
                now.saturating_add(60_000),
                now,
            )
            .map_err(err)?;
        engine.effect = "unknown";
        let committed = if let Some(change) = &prepared.change {
            engine
                .host
                .edit_versioned_content(&connection, change, clock)
        } else {
            engine.host.create_content(
                &connection,
                &value.business.operation,
                &prepared.result,
                clock,
            )
        };
        engine.commit_result(committed)?;
        Ok(())
    })();
    let disconnected = engine.host.disconnect(&connection).map_err(err);
    result?;
    disconnected?;
    let original = historical(engine, &value.business.id, &value.business.operation)?
        .ok_or("EditorCommittedHistoryMissing")?;
    if original.command != prepared.command {
        return Err("EditorCommittedCommandMismatch".into());
    }
    committed_reply(
        engine,
        &value,
        &original,
        &prepared,
        "development_editor_wire_v1",
    )
}

#[cfg(test)]
mod tests;
