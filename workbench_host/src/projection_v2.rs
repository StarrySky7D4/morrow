//! Frozen projection v2: adopted converter observations, exact paste replacements and editor
//! endpoints. Literal text, clipboard trees, alias paths and clock facts remain host observations;
//! this is not source authentication or a per-keystroke edit log. V1 projection stays unchanged.
use crate::{
    Result,
    capture_provenance::{EditorSnapshot, PasteEvent},
    projection::{self, proto},
};
use morrow_core::{
    task::Invocation,
    task_evidence::{self, Evidence, proto::Observation},
};
use morrow_workbench_plugin::{Action, Request, capture, capture_capnp};
use prost::{
    Message,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use std::collections::BTreeSet;
pub const INTENT_TYPE: &str = "morrow.workbench.content-projection.v2";
pub const MAX_TEXT: usize = 256 * 1024;
pub const MAX_APPLICATIONS: usize = 1024;
#[derive(Clone, Copy)]
enum Kind {
    Root,
    Parent,
    Paste,
    Part,
    Snapshot,
    Alias,
}
fn preflight(mut raw: &[u8], kind: Kind, fields: &mut usize) -> Result<()> {
    let mut seen = 0u16;
    let mut repeated = 0usize;
    while !raw.is_empty() {
        *fields = fields.checked_sub(1).ok_or("paste facts field limit")?;
        let (n, w) = decode_key(&mut raw)?;
        let max = match kind {
            Kind::Root => 6,
            Kind::Parent => 1,
            Kind::Paste => 7,
            Kind::Part => 3,
            Kind::Snapshot => 6,
            Kind::Alias => 3,
        };
        if n <= max {
            let repeat = matches!(
                (kind, n),
                (Kind::Root, 4 | 5) | (Kind::Paste, 6) | (Kind::Snapshot, 6)
            );
            if repeat {
                repeated += 1;
                if repeated > 2048 {
                    return Err("paste repeated limit".into());
                }
            } else {
                let bit = 1u16 << n;
                if seen & bit != 0 {
                    return Err("duplicate paste field".into());
                }
                seen |= bit;
            }
            let varint = matches!(
                (kind, n),
                (Kind::Root, 1) | (Kind::Parent, 1) | (Kind::Paste, 4 | 5) | (Kind::Part, 1)
            );
            if w != if varint {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            } {
                return Err("paste field wire type".into());
            }
        }
        if w == WireType::LengthDelimited {
            let length = usize::try_from(decode_varint(&mut raw)?)?;
            let limit = match (kind, n) {
                (Kind::Root, 2) => task_evidence::MAX_INTENT_BYTES,
                (Kind::Root, 3) | (Kind::Paste, 1 | 2) | (Kind::Part, 3) | (Kind::Alias, 1) => 256,
                (Kind::Parent, _) => 128,
                (Kind::Alias, 2 | 3) => 16 * 1024,
                (Kind::Paste, 3 | 7) | (Kind::Part, 2) | (Kind::Snapshot, 1..=5) => MAX_TEXT,
                _ => task_evidence::MAX_INTENT_BYTES,
            };
            if length > limit || length > raw.len() {
                return Err("paste facts length".into());
            }
            let (value, tail) = raw.split_at(length);
            raw = tail;
            let nested = match (kind, n) {
                (Kind::Root, 4) => Some(Kind::Parent),
                (Kind::Root, 5) => Some(Kind::Paste),
                (Kind::Root, 6) => Some(Kind::Snapshot),
                (Kind::Paste, 6) => Some(Kind::Part),
                (Kind::Snapshot, 6) => Some(Kind::Alias),
                _ => None,
            };
            if matches!((kind, n), (Kind::Root, 2)) {
                projection::decode_intent_v1(value)?;
            }
            if let Some(next) = nested {
                preflight(value, next, fields)?;
            }
        } else {
            if matches!(w, WireType::StartGroup | WireType::EndGroup) {
                return Err("paste facts group".into());
            }
            if n <= max && w == WireType::Varint {
                if decode_varint(&mut raw)? > u64::from(u32::MAX) {
                    return Err("paste integer limit".into());
                }
            } else {
                skip_field(w, n, &mut raw, DecodeContext::default())?;
            }
        }
    }
    Ok(())
}
pub(super) fn decode(raw: &[u8]) -> Result<proto::ContentProjectionV2> {
    if raw.len() > task_evidence::MAX_INTENT_BYTES {
        return Err("paste intent limit".into());
    }
    preflight(raw, Kind::Root, &mut 32768)?;
    let value = proto::ContentProjectionV2::decode(raw)?;
    if value.schema_version != 2 || value.scope.is_empty() || value.scope.len() > 256 {
        return Err("unsupported paste projection".into());
    }
    Ok(value)
}
fn reader(raw: &[u8]) -> Result<capnp::message::Reader<capnp::serialize::OwnedSegments>> {
    if raw.len() > 65536 {
        return Err("capture frame limit".into());
    }
    let mut cursor = std::io::Cursor::new(raw);
    let r = capnp::serialize::read_message(
        &mut cursor,
        capnp::message::ReaderOptions {
            traversal_limit_in_words: Some(8192),
            nesting_limit: 16,
        },
    )?;
    if cursor.position() != raw.len() as u64 {
        return Err("capture trailing bytes".into());
    }
    Ok(r)
}
pub(super) fn capture_input(raw: &[u8]) -> Result<(String, String)> {
    let r = reader(raw)?;
    let r = r.get_root::<capture_capnp::request::Reader>()?;
    if r.get_version() != 1 || r.get_digest()? != capture::digest() || r.get_nodes()?.len() > 1024 {
        return Err("capture input contract".into());
    }
    let format = r.get_format()?.to_str()?.to_owned();
    if !matches!(format.as_str(), "plain" | "rtf" | "html" | "spreadsheet") {
        return Err("capture format".into());
    }
    Ok((format, r.get_source()?.to_str()?.to_owned()))
}
pub(super) fn capture_output(raw: &[u8]) -> Result<String> {
    let r = reader(raw)?;
    let r = r.get_root::<capture_capnp::response::Reader>()?;
    if r.get_version() != 1 || r.get_digest()? != capture::digest() {
        return Err("capture output contract".into());
    }
    Ok(r.get_markdown()?.to_str()?.to_owned())
}
pub(super) fn captured(observation: &Observation) -> Result<(String, String, String, Vec<u8>)> {
    if observation.fault != 0
        || observation.exit_code != Some(0)
        || observation.observed_host_calls != 0
    {
        return Err("unsuccessful capture observation".into());
    }
    let task = Invocation::decode(&observation.invocation)?;
    let t = task.transform().ok_or("capture is not transform")?;
    if t.handler != "capture.convert"
        || t.input_type != "morrow.capture.request.v1"
        || t.output_type != "morrow.capture.response.v1"
    {
        return Err("capture task route".into());
    }
    let (format, source) = capture_input(&t.input)?;
    let output = task.verify_output(&observation.completion)?;
    let markdown = capture_output(&output.bytes)?;
    Ok((format, source, markdown, output.bytes))
}
fn utf16_offset(value: &str, index: u32) -> Result<usize> {
    let mut units = 0u32;
    for (offset, ch) in value.char_indices() {
        if units == index {
            return Ok(offset);
        }
        units = units
            .checked_add(ch.len_utf16() as u32)
            .ok_or("text index overflow")?;
        if units > index {
            return Err("selection splits UTF16 surrogate pair".into());
        }
    }
    if units == index {
        Ok(value.len())
    } else {
        Err("selection outside text".into())
    }
}
pub(super) fn replacement(
    before: &str,
    start: u32,
    end: u32,
    inserted: &str,
    after: &str,
) -> Result<()> {
    if before.len() > MAX_TEXT || after.len() > MAX_TEXT || inserted.len() > MAX_TEXT || start > end
    {
        return Err("paste text limit".into());
    }
    let a = utf16_offset(before, start)?;
    let b = utf16_offset(before, end)?;
    let expected_len = a
        .checked_add(inserted.len())
        .and_then(|n| n.checked_add(before.len() - b))
        .ok_or("paste length overflow")?;
    if expected_len > MAX_TEXT || expected_len != after.len() {
        return Err("paste replacement length".into());
    }
    let mut expected = String::with_capacity(expected_len);
    expected.push_str(&before[..a]);
    expected.push_str(inserted);
    expected.push_str(&before[b..]);
    if expected != after {
        return Err("paste replacement mismatch".into());
    }
    Ok(())
}
pub(super) fn field(name: &str) -> Result<()> {
    if matches!(
        name,
        "title" | "description" | "hypothesis" | "conclusion" | "todos"
    ) {
        Ok(())
    } else {
        Err("unknown editor field".into())
    }
}
pub(super) fn event_bounds(event: &PasteEvent) -> Result<()> {
    if event.id.is_empty()
        || event.id.len() > 256
        || event.parts.is_empty()
        || event.parts.len() > 1024
        || event.before.len() > MAX_TEXT
        || event.after.len() > MAX_TEXT
    {
        return Err("paste event limit".into());
    }
    field(&event.field)?;
    for p in &event.parts {
        if p.ticket.len() > 256 || p.literal.len() > MAX_TEXT || p.selection.len() > 32 {
            return Err("paste part limit".into());
        }
    }
    Ok(())
}
pub(super) fn snapshot_proto(value: &EditorSnapshot) -> Result<proto::EditorSnapshot> {
    for text in [
        &value.title,
        &value.description,
        &value.hypothesis,
        &value.conclusion,
        &value.todos,
    ] {
        if text.len() > MAX_TEXT {
            return Err("editor snapshot limit".into());
        }
    }
    if value.aliases.len() > 20 {
        return Err("editor alias limit".into());
    }
    let mut ids = BTreeSet::new();
    for a in &value.aliases {
        if a.id.is_empty()
            || a.id.len() > 256
            || a.location.len() > 16384
            || a.name.len() > 16384
            || !ids.insert(&a.id)
        {
            return Err("editor alias bounds".into());
        }
    }
    Ok(proto::EditorSnapshot {
        title: value.title.clone(),
        description: value.description.clone(),
        hypothesis: value.hypothesis.clone(),
        conclusion: value.conclusion.clone(),
        todos: value.todos.clone(),
        aliases: value
            .aliases
            .iter()
            .map(|a| proto::AttachmentAlias {
                id: a.id.clone(),
                location: a.location.clone(),
                name: a.name.clone(),
            })
            .collect(),
    })
}
// Dart String.trim follows Unicode White_Space plus BOM; keep this policy frozen for v2.
fn dart_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{0085}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn trim(s: &str) -> &str {
    s.trim_matches(dart_space)
}
fn component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            use std::fmt::Write;
            write!(&mut out, "%{b:02X}").expect("string write");
        }
    }
    out
}
fn replace_alias(source: &str, name: &str, target: &str) -> String {
    let pattern = format!("attachment:{name}");
    let mut out = String::new();
    let mut last = 0;
    for (at, _) in source.match_indices(&pattern) {
        let end = at + pattern.len();
        let boundary = source[end..]
            .chars()
            .next()
            .is_none_or(|c| matches!(c, ')' | '>') || (dart_space(c) && c != '\u{0085}'));
        if boundary {
            out.push_str(&source[last..at]);
            out.push_str(target);
            last = end;
        }
    }
    out.push_str(&source[last..]);
    out
}
pub(super) fn verify_snapshot(snapshot: &proto::EditorSnapshot, request: &Request) -> Result<()> {
    if !matches!(request.action, Action::Create | Action::Edit) {
        return Err("capture save must create or edit".into());
    }
    let checked = EditorSnapshot {
        title: snapshot.title.clone(),
        description: snapshot.description.clone(),
        hypothesis: snapshot.hypothesis.clone(),
        conclusion: snapshot.conclusion.clone(),
        todos: snapshot.todos.clone(),
        aliases: snapshot
            .aliases
            .iter()
            .map(|a| crate::capture_provenance::AttachmentAlias {
                id: a.id.clone(),
                location: a.location.clone(),
                name: a.name.clone(),
            })
            .collect(),
    };
    snapshot_proto(&checked)?;
    let mut description = trim(&snapshot.description).to_owned();
    if description.is_empty() {
        description = "从一个小小的念头开始。".into();
    }
    for alias in &snapshot.aliases {
        if !request
            .proposed
            .assets
            .iter()
            .any(|a| a.id == alias.id && a.name == alias.name)
        {
            return Err("editor alias outside submitted assets".into());
        }
        let mut names = Vec::new();
        for name in [
            alias.location.clone(),
            alias.name.clone(),
            component(&alias.location),
            component(&alias.name),
        ] {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names.sort_by_key(|n| std::cmp::Reverse(n.encode_utf16().count()));
        for name in names {
            description = replace_alias(&description, &name, &format!("attachment:{}", alias.id));
        }
    }
    let mut todos = Vec::new();
    for line in snapshot.todos.split('\n') {
        let line = trim(line);
        if !line.is_empty() && !todos.iter().any(|v| v == line) {
            todos.push(line.to_owned());
        }
    }
    let p = &request.proposed;
    if p.title != trim(&snapshot.title)
        || p.description != description
        || p.hypothesis != trim(&snapshot.hypothesis)
        || p.conclusion != trim(&snapshot.conclusion)
        || p.todos != todos
    {
        return Err("editor endpoints differ from submitted draft".into());
    }
    Ok(())
}
pub fn derive(evidence: &Evidence) -> Result<projection::Projection> {
    if evidence.data().schema_version != task_evidence::BATCH_VERSION {
        return Err("capture requires batch".into());
    }
    let batch = evidence
        .data()
        .batch
        .as_ref()
        .ok_or("missing capture batch")?;
    if batch.intent_type != INTENT_TYPE || batch.observations.is_empty() {
        return Err("capture projection type".into());
    }
    let facts = decode(&batch.intent)?;
    let n = batch.observations.len() - 1;
    if facts.captures.len() != n || facts.applications.len() > MAX_APPLICATIONS {
        return Err("capture observation count".into());
    }
    let mut values = Vec::with_capacity(n);
    for (i, meta) in facts.captures.iter().enumerate() {
        let value = captured(&batch.observations[i])?;
        if let Some(parent) = meta.parent {
            let parent = parent as usize;
            if parent >= i {
                return Err("capture parent order".into());
            }
            let parent_value: &(String, String, String, Vec<u8>) = &values[parent];
            if value.0 != "plain" || value.1 != parent_value.2 {
                return Err("capture parent source mismatch".into());
            }
        }
        values.push(value);
    }
    let mut used = vec![false; n];
    let mut ids = BTreeSet::new();
    for app in &facts.applications {
        field(&app.field)?;
        if app.id.is_empty()
            || app.id.len() > 256
            || !ids.insert(&app.id)
            || app.parts.is_empty()
            || app.parts.len() > 1024
        {
            return Err("paste application identity".into());
        }
        let mut inserted = String::new();
        for part in &app.parts {
            if let Some(index) = part.observation {
                let index = index as usize;
                let value = values.get(index).ok_or("paste observation index")?;
                if !part.literal.is_empty() {
                    return Err("mixed paste part".into());
                }
                let text = match part.selection.as_str() {
                    "outputMarkdown" => &value.2,
                    "inputPlainText" if value.0 == "plain" => &value.1,
                    _ => return Err("unsupported capture output selection".into()),
                };
                inserted.push_str(text);
                used[index] = true;
            } else {
                if !part.selection.is_empty() {
                    return Err("literal has capture selection".into());
                }
                inserted.push_str(&part.literal);
            }
            if inserted.len() > MAX_TEXT {
                return Err("paste insertion limit".into());
            }
        }
        replacement(
            &app.before,
            app.start_utf16,
            app.end_utf16,
            &inserted,
            &app.after,
        )?;
    }
    for i in (0..n).rev() {
        if used[i]
            && let Some(parent) = facts.captures[i].parent
        {
            used[parent as usize] = true;
        }
    }
    if used.iter().any(|used| !*used) {
        return Err("unadopted capture in saved evidence".into());
    }
    let content = projection::decode_intent_v1(&facts.content)?;
    let projection = projection::derive_v1(content, &batch.observations[n])?;
    verify_snapshot(
        facts.snapshot.as_ref().ok_or("missing editor endpoint")?,
        &projection.original_request,
    )?;
    Ok(projection)
}
