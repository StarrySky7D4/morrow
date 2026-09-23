//! V2 checklist semantics. V1 execution/encoding remains frozen for replay.
//! Pure transformations are not commits: the owner must additionally enforce CAS,
//! format migration authority and evidence before storing any returned bytes.
use crate::{Idea, persistence};
use prost::Message;
use prost::encoding::{
    DecodeContext, WireType, decode_key, decode_varint, encode_key, encode_varint, skip_field,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod generated {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/morrow.workbench.v1.rs"));
    }
    pub mod tasks {
        pub mod v2 {
            include!(concat!(env!("OUT_DIR"), "/morrow.workbench.tasks.v2.rs"));
        }
    }
}
pub use generated::tasks::v2::{Completion, Mapping, Origin, Properties, Task};
pub const MAX_BYTES: usize = 65536;
const MIGRATOR: u32 = 1;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Baseline {
    pub card_id: String,
    pub revision: u64,
    pub sha256: [u8; 32],
}
impl Baseline {
    pub fn capture(card_id: &str, revision: u64, bytes: &[u8]) -> Result<Self, &'static str> {
        if !crate::valid_id(card_id) || revision == 0 || bytes.len() > MAX_BYTES {
            return Err("invalid migration baseline");
        }
        Ok(Self {
            card_id: card_id.into(),
            revision,
            sha256: Sha256::digest(bytes).into(),
        })
    }
    pub fn operation_id(&self) -> String {
        format!("migrate-tasks-{}", hash(self, None))
    }
}
fn hash(base: &Baseline, index: Option<u32>) -> String {
    let mut h = Sha256::new();
    h.update(b"morrow.tasks.migration.v2\0");
    h.update(MIGRATOR.to_le_bytes());
    h.update((base.card_id.len() as u64).to_le_bytes());
    h.update(base.card_id.as_bytes());
    h.update(base.revision.to_le_bytes());
    h.update(base.sha256);
    if let Some(index) = index {
        h.update(b"task\0");
        h.update(index.to_le_bytes());
    }
    const HEX: &[u8] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for b in h.finalize() {
        result.push(HEX[(b >> 4) as usize] as char);
        result.push(HEX[(b & 15) as usize] as char);
    }
    result
}
fn legacy_counts(old: &Idea) -> BTreeMap<&str, u32> {
    let mut counts = BTreeMap::new();
    for text in &old.todos {
        *counts.entry(text.as_str()).or_insert(0) += 1;
    }
    counts
}
// Preserve uninterpreted fields verbatim, including extensions inside tasks.
// This scanner runs only after the complete bounded message was validated.
fn fields(mut bytes: &[u8]) -> Result<Vec<(u32, &[u8], &[u8])>, &'static str> {
    let mut result = Vec::new();
    while !bytes.is_empty() {
        let start = bytes;
        let (tag, wire) = decode_key(&mut bytes).map_err(|_| "field key")?;
        let after_key = bytes;
        skip_field(wire, tag, &mut bytes, DecodeContext::default()).map_err(|_| "field value")?;
        let raw = &start[..start.len() - bytes.len()];
        let mut payload = &after_key[..after_key.len() - bytes.len()];
        if wire == WireType::LengthDelimited {
            let length = decode_varint(&mut payload).map_err(|_| "field length")?;
            if length != payload.len() as u64 {
                return Err("field length mismatch");
            }
        }
        result.push((tag, raw, payload));
    }
    Ok(result)
}
fn length_field(tag: u32, value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() + 8);
    encode_key(tag, WireType::LengthDelimited, &mut out);
    encode_varint(value.len() as u64, &mut out);
    out.extend_from_slice(value);
    out
}
fn replace_field(bytes: &[u8], tag: u32, values: &[Vec<u8>]) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::with_capacity(bytes.len());
    for (field, raw, _) in fields(bytes)? {
        if field != tag {
            out.extend_from_slice(raw);
        }
    }
    for value in values {
        out.extend_from_slice(value);
    }
    Ok(out)
}
fn completion_field(complete: bool) -> Vec<u8> {
    let mut out = Vec::new();
    encode_key(3, WireType::Varint, &mut out);
    encode_varint(u64::from(complete), &mut out);
    out
}

pub fn migrate(
    base: &Baseline,
    current_id: &str,
    current_revision: u64,
    title: &str,
    original: &[u8],
) -> Result<Vec<u8>, &'static str> {
    migrate_with_view(base, current_id, current_revision, title, original).map(|v| v.0)
}
pub(crate) fn migrate_with_view(
    base: &Baseline,
    current_id: &str,
    current_revision: u64,
    title: &str,
    original: &[u8],
) -> Result<(Vec<u8>, Properties), &'static str> {
    if &Baseline::capture(current_id, current_revision, original)? != base {
        return Err("migration baseline changed");
    }
    if current_revision == u64::MAX {
        return Err("revision exhausted");
    }
    let old = persistence::decode(current_id, title, original)?;
    let typed = generated::v1::Properties::decode(original).map_err(|_| "legacy properties")?;
    let mut tasks = Vec::new();
    let mut mapping = Vec::new();
    let counts = legacy_counts(&old);
    let completed: BTreeSet<_> = old.completed.iter().collect();
    for (index, text) in old.todos.iter().enumerate() {
        let duplicates = counts[text.as_str()];
        let completed = completed.contains(text);
        let id = format!("task-{}", hash(base, Some(index as u32)));
        tasks.push(Task {
            id: id.clone(),
            text: text.clone(),
            completion: if completed && duplicates > 1 {
                Completion::LegacyAmbiguous
            } else if completed {
                Completion::Complete
            } else {
                Completion::Incomplete
            } as i32,
            legacy_completed: completed,
            legacy_duplicates: duplicates,
        });
        mapping.push(Mapping {
            source_index: index as u32,
            task_id: id,
        });
    }
    let value = Properties {
        version: 2,
        description: typed.description,
        category: typed.category,
        stage: typed.stage,
        hypothesis: typed.hypothesis,
        conclusion: typed.conclusion,
        favorite: typed.favorite,
        assets: typed.assets,
        icon: typed.icon,
        color: typed.color,
        deleted: typed.deleted,
        deleted_at: typed.deleted_at,
        tasks,
        origin: Some(Origin {
            card_id: current_id.into(),
            source_revision: current_revision,
            source_sha256: base.sha256.to_vec(),
            migrator_version: MIGRATOR,
            target_version: 2,
            original_properties: original.to_vec(),
            mapping,
            historical_project_stage: old.project_stage().into(),
            original_title: title.into(),
        }),
        retired_task_ids: vec![],
    };
    let bytes = value.encode_to_vec();
    let verified = decode(current_id, title, &bytes)?;
    Ok((bytes, verified))
}

pub fn decode(id: &str, title: &str, bytes: &[u8]) -> Result<Properties, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("V2 properties budget");
    }
    let p = Properties::decode(bytes).map_err(|_| "V2 properties")?;
    if p.version != 2 || p.tasks.len() > 128 {
        return Err("V2 version or task budget");
    }
    // Reuse unchanged common-field validation, never V1 task/stage inference.
    Idea {
        id: id.into(),
        title: title.into(),
        description: p.description.clone(),
        category: p.category.clone(),
        stage: p.stage.clone(),
        hypothesis: p.hypothesis.clone(),
        conclusion: p.conclusion.clone(),
        favorite: p.favorite,
        icon: u16::try_from(p.icon).map_err(|_| "icon")?,
        color: p.color,
        deleted: p.deleted,
        deleted_at: p.deleted_at,
        assets: p
            .assets
            .iter()
            .map(|a| crate::Asset {
                id: a.id.clone(),
                name: a.name.clone(),
                kind: a.kind.clone(),
                bytes: a.bytes,
            })
            .collect(),
        ..Default::default()
    }
    .validate()?;
    let mut ids = BTreeSet::new();
    for task in &p.tasks {
        if !crate::valid_id(&task.id)
            || !ids.insert(&task.id)
            || task.text.is_empty()
            || task.text.len() > 2048
            || Completion::try_from(task.completion).is_err()
        {
            return Err("invalid V2 task");
        }
        if task.completion == Completion::LegacyAmbiguous as i32
            && (!task.legacy_completed || task.legacy_duplicates < 2)
        {
            return Err("ambiguity requires legacy evidence");
        }
    }
    if p.retired_task_ids.len() > 4096 {
        return Err("retired TaskId budget");
    }
    for retired in &p.retired_task_ids {
        if !crate::valid_id(retired) || !ids.insert(retired) {
            return Err("reused TaskId");
        }
    }
    if let Some(origin) = &p.origin {
        if origin.card_id != id || origin.migrator_version != MIGRATOR || origin.target_version != 2
        {
            return Err("unsupported migration origin");
        }
        let base = Baseline::capture(id, origin.source_revision, &origin.original_properties)?;
        if origin.source_sha256 != base.sha256 {
            return Err("migration source digest");
        }
        let old = persistence::decode(id, &origin.original_title, &origin.original_properties)?;
        if origin.mapping.len() != old.todos.len()
            || origin.historical_project_stage != old.project_stage()
        {
            return Err("migration history mismatch");
        }
        for (index, entry) in origin.mapping.iter().enumerate() {
            if entry.source_index != index as u32
                || entry.task_id != format!("task-{}", hash(&base, Some(index as u32)))
            {
                return Err("migration mapping mismatch");
            }
        }
        let by_id: BTreeMap<_, _> = origin
            .mapping
            .iter()
            .map(|m| (m.task_id.as_str(), m))
            .collect();
        let counts = legacy_counts(&old);
        let completed: BTreeSet<_> = old.completed.iter().collect();
        for t in &p.tasks {
            if let Some(m) = by_id.get(t.id.as_str()) {
                let text = &old.todos[m.source_index as usize];
                if t.legacy_completed != completed.contains(text)
                    || t.legacy_duplicates != counts[text.as_str()]
                {
                    return Err("task provenance changed");
                }
            } else if t.legacy_completed || t.legacy_duplicates != 0 {
                return Err("invented task provenance");
            }
        }
    } else if p
        .tasks
        .iter()
        .any(|t| t.legacy_completed || t.legacy_duplicates != 0)
    {
        return Err("missing task provenance");
    }
    Ok(p)
}

#[derive(Clone, Debug)]
pub enum Command {
    SetCompletion { id: String, complete: bool },
    Rename { id: String, text: String },
    Reorder(Vec<String>),
    SetStage(String),
    CompleteAllAndSetStage(String),
    Add { id: String, text: String },
    Remove(String),
}
#[derive(Debug, PartialEq, Eq)]
pub struct Projection {
    pub stage: String,
    pub complete: u32,
    pub incomplete: u32,
    pub ambiguous: u32,
}
pub fn project(id: &str, title: &str, bytes: &[u8]) -> Result<Projection, &'static str> {
    let p = decode(id, title, bytes)?;
    Ok(project_validated(&p))
}
pub(crate) fn project_validated(p: &Properties) -> Projection {
    let count = |state: Completion| {
        p.tasks
            .iter()
            .filter(|t| t.completion == state as i32)
            .count() as u32
    };
    Projection {
        stage: p.stage.clone(),
        complete: count(Completion::Complete),
        incomplete: count(Completion::Incomplete),
        ambiguous: count(Completion::LegacyAmbiguous),
    }
}
pub fn apply(
    id: &str,
    title: &str,
    bytes: &[u8],
    command: Command,
) -> Result<Vec<u8>, &'static str> {
    apply_with_view(id, title, bytes, command).map(|v| v.0)
}
pub(crate) fn apply_with_view(
    id: &str,
    title: &str,
    bytes: &[u8],
    command: Command,
) -> Result<(Vec<u8>, Properties), &'static str> {
    let previous = decode(id, title, bytes)?;
    // Fully validate incoming provenance once. These commands cannot edit it;
    // validate the only mutable arguments before touching the preserved tree.
    match &command {
        Command::Rename { text, .. } | Command::Add { text, .. }
            if text.is_empty() || text.len() > 2048 =>
        {
            return Err("task text budget");
        }
        Command::Add { id, .. } if !crate::valid_id(id) || previous.tasks.len() >= 128 => {
            return Err("task identity or count");
        }
        Command::Remove(_) if previous.retired_task_ids.len() >= 4096 => {
            return Err("retired TaskId budget");
        }
        Command::SetStage(stage) | Command::CompleteAllAndSetStage(stage)
            if !crate::stages(&previous.category).contains(&stage.as_str()) =>
        {
            return Err("invalid stage");
        }
        _ => {}
    }
    let mut root = bytes.to_vec();
    let mut tasks: Vec<Vec<u8>> = fields(bytes)?
        .into_iter()
        .filter(|f| f.0 == 20)
        .map(|f| f.2.to_vec())
        .collect();
    if tasks.len() != previous.tasks.len() {
        return Err("task field count");
    }
    let index = |id: &str| {
        previous
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or("unknown TaskId")
    };
    match command {
        Command::SetCompletion { id, complete } => {
            let i = index(&id)?;
            tasks[i] = replace_field(&tasks[i], 3, &[completion_field(complete)])?;
        }
        Command::Rename { id, text } => {
            let i = index(&id)?;
            tasks[i] = replace_field(&tasks[i], 2, &[length_field(2, text.as_bytes())])?;
        }
        Command::Reorder(order) => {
            if order.len() != tasks.len()
                || order.iter().collect::<BTreeSet<_>>().len() != tasks.len()
            {
                return Err("invalid task permutation");
            }
            tasks = order
                .iter()
                .map(|id| Ok(tasks[index(id)?].clone()))
                .collect::<Result<_, &'static str>>()?;
        }
        Command::SetStage(stage) => {
            root = replace_field(&root, 4, &[length_field(4, stage.as_bytes())])?
        }
        Command::CompleteAllAndSetStage(stage) => {
            for task in &mut tasks {
                *task = replace_field(task, 3, &[completion_field(true)])?;
            }
            root = replace_field(&root, 4, &[length_field(4, stage.as_bytes())])?;
        }
        Command::Remove(id) => {
            tasks.remove(index(&id)?);
            root.extend(length_field(22, id.as_bytes()));
        }
        Command::Add { id, text } => {
            // New IDs are supplied by the owner as part of the fixed command.
            // Reusing an identity from the original mapping is also forbidden.
            if previous.tasks.iter().any(|t| t.id == id)
                || previous.retired_task_ids.contains(&id)
                || previous
                    .origin
                    .as_ref()
                    .is_some_and(|o| o.mapping.iter().any(|m| m.task_id == id))
            {
                return Err("TaskId already used");
            }
            let value = Task {
                id,
                text,
                ..Default::default()
            };
            tasks.push(value.encode_to_vec());
        }
    }
    let result = replace_field(
        &root,
        20,
        &tasks
            .iter()
            .map(|t| length_field(20, t))
            .collect::<Vec<_>>(),
    )?;
    if result.len() > MAX_BYTES {
        return Err("V2 properties budget");
    }
    // No second provenance hash pass: origin/common fields were never changed,
    // and identity, permutation, retirement and text checks above preserve the
    // validated invariants. Decode once for the authoritative output projection.
    let view = Properties::decode(result.as_slice()).map_err(|_| "V2 properties")?;
    Ok((result, view))
}
