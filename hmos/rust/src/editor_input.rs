//! Pure input formatting from the actual Windows Flutter enforced pipeline.
//! Returns a proposal only: no Store, draft write, input ownership or capture.
use crate::editor_field;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_REQUEST_BYTES: usize = 512 * 1024;
pub const MAX_REPLY_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TextValue {
    pub text: String,
    pub selection_base: i32,
    pub selection_extent: i32,
    // Flutter TextAffinity.index: 0 upstream, 1 downstream.
    pub affinity: u8,
    pub directional: bool,
    pub composing_start: i32,
    pub composing_end: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    field: String,
    #[serde(default = "field_mode")]
    mode: String,
    #[serde(default)]
    limit: Option<usize>,
    old_value: Box<RawValue>,
    new_value: Box<RawValue>,
}
fn field_mode() -> String {
    "field".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reply {
    pub ok: bool,
    pub error: String,
    pub field: String,
    pub mode: String,
    pub limit: usize,
    pub action: String,
    pub format_applied: bool,
    pub old_value: Option<TextValue>,
    pub new_value: Option<TextValue>,
    pub value: Option<TextValue>,
    pub old_grapheme_count: i64,
    pub old_utf16_length: i64,
    pub old_utf8_length: i64,
    pub new_grapheme_count: i64,
    pub new_utf16_length: i64,
    pub new_utf8_length: i64,
    pub grapheme_count: i64,
    pub utf16_length: i64,
    pub utf8_length: i64,
    pub unicode_version: String,
    pub request_sha256: String,
}

impl Reply {
    pub fn failure(field: &str, mode: &str, limit: usize, error: &str, sha: &str) -> Self {
        Self {
            ok: false,
            error: error.into(),
            field: field.into(),
            mode: mode.into(),
            limit,
            action: "error".into(),
            format_applied: false,
            old_value: None,
            new_value: None,
            value: None,
            old_grapheme_count: -1,
            old_utf16_length: -1,
            old_utf8_length: -1,
            new_grapheme_count: -1,
            new_utf16_length: -1,
            new_utf8_length: -1,
            grapheme_count: -1,
            utf16_length: -1,
            utf8_length: -1,
            unicode_version: editor_field::UNICODE_VERSION.into(),
            request_sha256: sha.into(),
        }
    }
}

fn valid_value(value: &TextValue) -> bool {
    let length = value.text.encode_utf16().count();
    let offset = |v: i32| v == -1 || usize::try_from(v).is_ok_and(|v| v <= length);
    value.text.len() <= editor_field::MAX_TEXT_BYTES
        && value.affinity <= 1
        && offset(value.selection_base)
        && offset(value.selection_extent)
        && offset(value.composing_start)
        && offset(value.composing_end)
        && !(value.composing_start >= 0
            && value.composing_end >= 0
            && value.composing_start > value.composing_end)
}

fn truncate(value: &TextValue, limit: usize) -> TextValue {
    let text = value.text.graphemes(true).take(limit).collect::<String>();
    let length = text.encode_utf16().count() as i32;
    let mut result = value.clone();
    result.text = text;
    // Actual Flutter truncate normalizes reversed selection via start/end.
    result.selection_base = value.selection_base.min(value.selection_extent).min(length);
    result.selection_extent = value.selection_base.max(value.selection_extent).min(length);
    if value.composing_start != value.composing_end && length > value.composing_start {
        result.composing_end = value.composing_end.min(length);
    } else {
        result.composing_start = -1;
        result.composing_end = -1;
    }
    result
}

fn row_filter(value: &TextValue) -> TextValue {
    let mut result = value.clone();
    // TipListEditor uses one-character RegExp matches with one-unit replacement,
    // so valid positions do not move. Filtering's finalize still resets invalid
    // selection and collapsed/invalid composing, even without CR/LF matches.
    result.text = value
        .text
        .chars()
        .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
        .collect();
    if value.selection_base < 0 || value.selection_extent < 0 {
        result.selection_base = -1;
        result.selection_extent = -1;
        result.affinity = 1;
        result.directional = false;
    }
    if value.composing_start < 0
        || value.composing_end < 0
        || value.composing_start == value.composing_end
    {
        result.composing_start = -1;
        result.composing_end = -1;
    }
    result
}

pub fn request(input: &str) -> Reply {
    let sha = crate::hex(&Sha256::digest(input.as_bytes()));
    if input.len() > MAX_REQUEST_BYTES {
        return Reply::failure("", "", 0, "EditorInputRequestBytesLimit", &sha);
    }
    let parsed = match serde_json::from_str::<Request>(input) {
        Ok(v) => v,
        Err(_) => return Reply::failure("", "", 0, "EditorInputInvalidRequest", &sha),
    };
    let Some(field_limit) = editor_field::field_limit(&parsed.field) else {
        // Do not echo an unbounded unknown identifier in the bounded reply.
        return Reply::failure("", "", 0, "EditorInputField", &sha);
    };
    let field = parsed.field.as_str();
    let mode = parsed.mode.as_str();
    let limit = match (mode, field, parsed.limit) {
        ("field", _, None) => field_limit,
        ("todo_row", "todos", Some(v)) if v <= field_limit => v,
        _ => return Reply::failure(field, "", field_limit, "EditorInputModeOrLimit", &sha),
    };
    let fail = |error: &str| Reply::failure(field, mode, limit, error, &sha);
    let old = match serde_json::from_str::<TextValue>(parsed.old_value.get()) {
        Ok(v) => v,
        Err(_) => return fail("EditorInputInvalidOldValue"),
    };
    let new = match serde_json::from_str::<TextValue>(parsed.new_value.get()) {
        Ok(v) => v,
        Err(_) => return fail("EditorInputInvalidNewValue"),
    };
    if !valid_value(&old) || !valid_value(&new) {
        return fail("EditorInputValueBounds");
    }
    let old_count = editor_field::grapheme_count(&old.text);
    let new_count = editor_field::grapheme_count(&new.text);
    // EditableText applies inputFormatters only on changed text or a composition
    // commit. Pure selection and non-commit composing updates must remain raw.
    let applied = old.text != new.text
        || (old.composing_start != old.composing_end && new.composing_start == new.composing_end);
    let (mut result, mut action) = (new.clone(), "accepted");
    if applied {
        if mode == "todo_row" {
            if limit == 0 && new_count > old_count {
                result = old.clone();
                action = "retained";
            }
            result = row_filter(&result);
        }
        if limit > 0 && editor_field::grapheme_count(&result.text) > limit {
            if old_count == limit && old.selection_base == old.selection_extent {
                result = old.clone();
                action = "retained";
            } else {
                result = truncate(&result, limit);
                action = "truncated";
            }
        }
    }
    let reply = Reply {
        ok: true,
        error: String::new(),
        field: field.into(),
        mode: mode.into(),
        limit,
        action: action.into(),
        format_applied: applied,
        old_grapheme_count: old_count as i64,
        old_utf16_length: old.text.encode_utf16().count() as i64,
        old_utf8_length: old.text.len() as i64,
        new_grapheme_count: new_count as i64,
        new_utf16_length: new.text.encode_utf16().count() as i64,
        new_utf8_length: new.text.len() as i64,
        grapheme_count: editor_field::grapheme_count(&result.text) as i64,
        utf16_length: result.text.encode_utf16().count() as i64,
        utf8_length: result.text.len() as i64,
        old_value: Some(old),
        new_value: Some(new),
        value: Some(result),
        unicode_version: editor_field::UNICODE_VERSION.into(),
        request_sha256: sha.clone(),
    };
    if serde_json::to_vec(&reply).map_or(true, |v| v.len() > MAX_REPLY_BYTES) {
        return fail("EditorInputReplyBytesLimit");
    }
    reply
}

#[cfg(test)]
mod tests;
