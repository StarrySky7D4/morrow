//! Correlations carried by the existing task envelope. This does not grant a
//! package access to history: the trusted host must approve disclosure first.
use crate::{Error, Result, id};

const PREFIX: &str = "history-v1.";
pub const TASK_ID_BYTES: usize = PREFIX.len() + 64 + 1 + 64;

/// Canonical task correlation for one selected submission and one fresh query.
/// A new query must not be inferred from an old Unknown or replayed completion.
pub fn task_id(submission: [u8; 32], nonce: [u8; 32]) -> Result<String> {
    id(&submission)?;
    id(&nonce)?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(TASK_ID_BYTES);
    result.push_str(PREFIX);
    for (index, value) in [submission, nonce].into_iter().enumerate() {
        if index != 0 {
            result.push('.');
        }
        for byte in value {
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 15) as usize] as char);
        }
    }
    Ok(result)
}

pub(crate) fn selection(value: &str) -> Result<([u8; 32], [u8; 32])> {
    if value.len() != TASK_ID_BYTES || !value.starts_with(PREFIX) {
        return Err(Error::Correlation);
    }
    let bytes = value.as_bytes();
    if bytes[PREFIX.len() + 64] != b'.' {
        return Err(Error::Correlation);
    }
    fn nibble(value: u8) -> Result<u8> {
        match value {
            b'0'..=b'9' => Ok(value - b'0'),
            b'a'..=b'f' => Ok(value - b'a' + 10),
            _ => Err(Error::Correlation),
        }
    }
    let mut ids = [[0; 32]; 2];
    for (index, result) in ids.iter_mut().enumerate() {
        let start = PREFIX.len() + index * 65;
        for (i, byte) in result.iter_mut().enumerate() {
            *byte = (nibble(bytes[start + i * 2])? << 4) | nibble(bytes[start + i * 2 + 1])?;
        }
        id(result).map_err(|_| Error::Correlation)?;
    }
    Ok((ids[0], ids[1]))
}
