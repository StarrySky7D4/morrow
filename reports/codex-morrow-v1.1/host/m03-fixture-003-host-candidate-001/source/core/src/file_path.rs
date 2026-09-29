use crate::{Error, Result};

const MAX_PATH_BYTES: usize = 4096;
const MAX_SEGMENT_UTF16_UNITS: usize = 255;
const MAX_SEGMENTS: usize = 128;

/// A bounded, unmodified relative path spelling for a future directory capability.
///
/// This is only a platform-independent lexical check. It does not grant access,
/// inspect symlinks, or make a filesystem lookup safe. Callers must not decode
/// or normalize the spelling after validation. A platform adapter must resolve
/// every component relative to a retained root handle before opening it.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RelativeFilePath(String);

impl RelativeFilePath {
    /// Validates the original UTF-8 spelling without decoding or normalization.
    pub fn parse(value: &str) -> Result<Self> {
        if value.len() > MAX_PATH_BYTES {
            return Err(Error::Limit);
        }
        if value.is_empty() {
            return Err(Error::Invalid("relative file path"));
        }

        for (index, segment) in value.split('/').enumerate() {
            if index >= MAX_SEGMENTS || segment.encode_utf16().count() > MAX_SEGMENT_UTF16_UNITS {
                return Err(Error::Limit);
            }
            if segment.is_empty()
                || segment == "."
                || segment == ".."
                || segment.ends_with(' ')
                || segment.ends_with('.')
                || segment.chars().any(|ch| {
                    ch.is_control() || matches!(ch, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
                })
                || is_windows_device(segment)
            {
                return Err(Error::Invalid("relative file path"));
            }
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the exact spelling that was validated.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_windows_device(segment: &str) -> bool {
    // Windows device names remain reserved before an extension. Spaces before
    // that extension can also be ignored by Windows path handling.
    let stem = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "CONIN$"
            | "CONOUT$"
            | "CLOCK$"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
            | "COM¹"
            | "COM²"
            | "COM³"
            | "LPT¹"
            | "LPT²"
            | "LPT³"
    )
}
