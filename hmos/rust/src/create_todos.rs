//! New-card checklist projection in the isolated HMOS development adapter.
//! Flutter's view-row IDs are never business TaskIds. The complete original
//! newline field stays in the raw journal; only this creation projection trims,
//! ignores blank rows and preserves the first occurrence of duplicate labels.
//! No Store write, legacy migration or completion inference happens here.
use morrow_workbench_plugin::tasks_v2::{Completion, Task};
use prost::encoding::{WireType, encode_key, encode_varint};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const MAX_ROWS: usize = 100;
pub const MAX_GRAPHEMES: usize = 1_000;

/// Adapter-owned registration: length-delimited Properties field 50001 holds
/// `morrow.hmos.create-todos.raw.v1\0` + SHA256 of the complete original `todos`
/// UTF8 field. It is outside the shared schema and the protobuf reserved
/// 19000..19999 range. Fresh nonempty raw creation only; shared unknown-field
/// scanners preserve it on later edits. It grants no protected capture or
/// Flutter schema equivalence. Empty/absent old requests keep identical bytes.
pub const RAW_IDENTITY_FIELD: u32 = 50_001;
const RAW_DOMAIN: &[u8] = b"morrow.hmos.create-todos.raw.v1\0";
const TASK_DOMAIN: &[u8] = b"morrow.hmos.create-task-id.v1\0";

pub struct Prepared {
    pub tasks: Vec<Task>,
    raw_identity: Option<[u8; 32]>,
}

fn framed(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn raw_identity(raw: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(RAW_DOMAIN);
    framed(&mut hash, b"todos");
    framed(&mut hash, raw.as_bytes());
    hash.finalize().into()
}

fn task_id(card: &str, operation: &str, raw: &[u8; 32], index: usize) -> String {
    let mut hash = Sha256::new();
    hash.update(TASK_DOMAIN);
    framed(&mut hash, card.as_bytes());
    framed(&mut hash, operation.as_bytes());
    hash.update(raw);
    hash.update((index as u64).to_le_bytes());
    format!("task-hmos-create-{:x}", hash.finalize())
}

// The installed Dart VM's String.trim whitespace set, including BOM. Rust's
// Unicode trim omits U+FEFF, so it cannot silently stand in for this projection.
fn dart_whitespace(value: char) -> bool {
    matches!(value,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}' | '\u{00a0}' |
        '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' |
        '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

pub fn prepare(card: &str, operation: &str, raw: &str) -> Result<Prepared, String> {
    let measurement = crate::editor_field::inspect("todos", raw);
    debug_assert_eq!(measurement.limit, MAX_GRAPHEMES);
    if !measurement.ok {
        return Err(measurement.error);
    }
    // Empty editor text has zero view rows. Other text splits only at LF, just
    // like CardTipsEditor; duplicates and blank rows count before normalization.
    if !raw.is_empty() && raw.split('\n').count() > MAX_ROWS {
        return Err("CreateTodosRowLimit".into());
    }
    let identity = (!raw.is_empty()).then(|| raw_identity(raw));
    let mut seen = BTreeSet::new();
    let mut tasks = Vec::new();
    for line in raw.split('\n') {
        let text = line.trim_matches(dart_whitespace);
        if text.is_empty() || !seen.insert(text) {
            continue;
        }
        tasks.push(Task {
            id: task_id(card, operation, identity.as_ref().expect("nonempty task has raw source"), tasks.len()),
            text: text.into(),
            completion: Completion::Incomplete as i32,
            legacy_completed: false,
            legacy_duplicates: 0,
        });
    }
    Ok(Prepared { tasks, raw_identity: identity })
}

impl Prepared {
    pub fn append_raw_identity(&self, properties: &mut Vec<u8>) {
        if let Some(identity) = self.raw_identity {
            encode_key(RAW_IDENTITY_FIELD, WireType::LengthDelimited, properties);
            encode_varint((RAW_DOMAIN.len() + identity.len()) as u64, properties);
            properties.extend_from_slice(RAW_DOMAIN);
            properties.extend_from_slice(&identity);
        }
    }
}

#[cfg(test)]
mod tests;
