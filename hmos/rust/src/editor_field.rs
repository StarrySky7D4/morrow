//! Pure Flutter Characters-compatible field measurement. No normalization,
//! selection conversion, draft publication, Store access or capture authority.
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_REQUEST_BYTES: usize = 512 * 1024;
pub const MAX_TEXT_BYTES: usize = 512 * 1024;
pub const UNICODE_VERSION: &str = "16.0.0";
pub const MAX_DESCRIPTION_GRAPHEMES: usize = 20_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    field: String,
    // Retain the JSON text token until the field name has been validated. This
    // lets a lone escaped surrogate fail explicitly with the known field/limit,
    // instead of replacing it or discarding that field identity.
    text: Box<RawValue>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reply {
    pub ok: bool,
    pub error: String,
    pub field: String,
    pub grapheme_count: i64,
    pub utf16_length: i64,
    pub utf8_length: i64,
    pub limit: usize,
    pub unicode_version: String,
}

pub fn field_limit(field: &str) -> Option<usize> {
    match field {
        "title" => Some(60),
        "todos" => Some(1_000),
        "hypothesis" => Some(5_000),
        "conclusion" => Some(10_000),
        "description" => Some(MAX_DESCRIPTION_GRAPHEMES),
        _ => None,
    }
}

pub fn grapheme_count(text: &str) -> usize {
    text.graphemes(true).count()
}

impl Reply {
    pub fn failure(field: &str, error: &str) -> Self {
        Self {
            ok: false,
            error: error.into(),
            field: field.into(),
            grapheme_count: -1,
            utf16_length: -1,
            utf8_length: -1,
            limit: field_limit(field).unwrap_or(0),
            unicode_version: UNICODE_VERSION.into(),
        }
    }
}

/// Complete measurement of the unchanged scalar-valid string. Empty strings,
/// whitespace, NUL and deliberate U+FFFD have ordinary Characters counts; this
/// does not assert that a later backend publication accepts those values.
pub fn inspect(field: &str, text: &str) -> Reply {
    let Some(limit) = field_limit(field) else {
        return Reply::failure(field, "EditorFieldName");
    };
    if text.len() > MAX_TEXT_BYTES {
        return Reply::failure(field, "EditorFieldTextBytesLimit");
    }
    let count = grapheme_count(text);
    Reply {
        ok: count <= limit,
        error: if count <= limit {
            ""
        } else {
            "EditorFieldGraphemeLimit"
        }
        .into(),
        field: field.into(),
        grapheme_count: count as i64,
        utf16_length: text.encode_utf16().count() as i64,
        utf8_length: text.len() as i64,
        limit,
        unicode_version: UNICODE_VERSION.into(),
    }
}

pub fn request(input: &str) -> Reply {
    if input.len() > MAX_REQUEST_BYTES {
        return Reply::failure("", "EditorFieldRequestBytesLimit");
    }
    let request = match serde_json::from_str::<Request>(input) {
        Ok(value) => value,
        Err(_) => return Reply::failure("", "EditorFieldInvalidRequest"),
    };
    if field_limit(&request.field).is_none() {
        return Reply::failure(&request.field, "EditorFieldName");
    }
    if !request.text.get().starts_with('"') {
        return Reply::failure(&request.field, "EditorFieldTextRequired");
    }
    let text = match serde_json::from_str::<String>(request.text.get()) {
        Ok(value) => value,
        Err(_) => return Reply::failure(&request.field, "EditorFieldInvalidUnicode"),
    };
    inspect(&request.field, &text)
}

#[cfg(test)]
mod tests;
