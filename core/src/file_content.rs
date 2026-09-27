//! Bounded persistent file bytes for an already identified mutation request.
//! This codec does not resolve paths, stage an OS write, or grant authority.
use crate::{Error, Result, envelope, identity};
use sha2::{Digest, Sha256};
use std::ops::Range;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.file_content.v1.rs"));
}

pub const MAGIC: &[u8; 8] = b"MROWFCT1";
pub const VERSION: u32 = 1;
pub const MAX_CONTENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_RAW_BYTES: usize = MAX_CONTENT_BYTES + 1024;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;

/// The content slice is stored once in the decoded protobuf frame. The outer
/// container is retained exactly for persistence and binding to its request.
pub struct FileContent {
    operation_id: String,
    subject: String,
    request_sha256: [u8; 32],
    content_sha256: [u8; 32],
    raw: Vec<u8>,
    content_range: Range<usize>,
    container: Vec<u8>,
}

struct ParsedRaw {
    operation_id: String,
    subject: String,
    request_sha256: [u8; 32],
    content_sha256: [u8; 32],
    content_range: Range<usize>,
}

impl FileContent {
    pub fn new(
        operation_id: &str,
        subject: &str,
        request_sha256: [u8; 32],
        content: &[u8],
    ) -> Result<Self> {
        identity(operation_id)?;
        identity(subject)?;
        if request_sha256 == [0; 32] {
            return Err(Error::Invalid("empty file content request digest"));
        }
        if content.len() > MAX_CONTENT_BYTES {
            return Err(Error::Limit);
        }
        let content_sha256: [u8; 32] = Sha256::digest(content).into();
        let mut raw = Vec::with_capacity(content.len() + 1024);
        raw.extend_from_slice(&[0x08, VERSION as u8]);
        append_bytes(&mut raw, 0x12, operation_id.as_bytes());
        append_bytes(&mut raw, 0x1a, subject.as_bytes());
        append_bytes(&mut raw, 0x22, &request_sha256);
        append_bytes(&mut raw, 0x2a, &content_sha256);
        let content_range = if content.is_empty() {
            raw.len()..raw.len()
        } else {
            raw.push(0x32);
            prost::encoding::encode_varint(content.len() as u64, &mut raw);
            let start = raw.len();
            raw.extend_from_slice(content);
            start..raw.len()
        };
        let container = envelope::pack(MAGIC, &raw, MAX_RAW_BYTES)?;
        Ok(Self {
            operation_id: operation_id.to_owned(),
            subject: subject.to_owned(),
            request_sha256,
            content_sha256,
            raw,
            content_range,
            container,
        })
    }

    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        let ParsedRaw {
            operation_id,
            subject,
            request_sha256,
            content_sha256,
            content_range,
        } = parse_raw(&raw)?;
        Ok(Self {
            operation_id,
            subject,
            request_sha256,
            content_sha256,
            raw,
            content_range,
            container: container.to_vec(),
        })
    }

    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn content(&self) -> &[u8] {
        &self.raw[self.content_range.clone()]
    }
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }
    pub fn subject(&self) -> &str {
        &self.subject
    }
    pub fn request_sha256(&self) -> [u8; 32] {
        self.request_sha256
    }
    pub fn content_sha256(&self) -> [u8; 32] {
        self.content_sha256
    }
}

fn append_bytes(raw: &mut Vec<u8>, key: u8, value: &[u8]) {
    raw.push(key);
    prost::encoding::encode_varint(value.len() as u64, raw);
    raw.extend_from_slice(value);
}

// All fields are in ascending canonical order. The only optional field is
// nonempty content; proto3 omits an empty bytes field. This also rejects every
// unknown, duplicate, out-of-order, wrong-wire and overlong-varint spelling.
fn parse_raw(raw: &[u8]) -> Result<ParsedRaw> {
    let mut cursor = 0;
    if take_varint(raw, &mut cursor)? != 0x08 {
        return Err(Error::Invalid("file content version field"));
    }
    if take_varint(raw, &mut cursor)? != VERSION as u64 {
        return Err(Error::UnsupportedVersion);
    }
    let operation = take_field(raw, &mut cursor, 0x12, 256)?;
    let subject = take_field(raw, &mut cursor, 0x1a, 256)?;
    let request = take_field(raw, &mut cursor, 0x22, 32)?;
    let digest = take_field(raw, &mut cursor, 0x2a, 32)?;
    let content = if cursor == raw.len() {
        cursor..cursor
    } else {
        let range = take_field(raw, &mut cursor, 0x32, MAX_CONTENT_BYTES)?;
        if range.is_empty() {
            return Err(Error::Invalid("noncanonical empty file content"));
        }
        range
    };
    if cursor != raw.len() || request.len() != 32 || digest.len() != 32 {
        return Err(Error::Invalid("file content fields"));
    }
    let operation_id = std::str::from_utf8(&raw[operation])
        .map_err(|_| Error::Invalid("file content operation UTF-8"))?;
    let subject = std::str::from_utf8(&raw[subject])
        .map_err(|_| Error::Invalid("file content subject UTF-8"))?;
    identity(operation_id)?;
    identity(subject)?;
    let request_sha256: [u8; 32] = raw[request].try_into().unwrap();
    if request_sha256 == [0; 32] {
        return Err(Error::Invalid("empty file content request digest"));
    }
    let content_sha256: [u8; 32] = raw[digest].try_into().unwrap();
    if Sha256::digest(&raw[content.clone()])[..] != content_sha256 {
        return Err(Error::Integrity);
    }
    Ok(ParsedRaw {
        operation_id: operation_id.to_owned(),
        subject: subject.to_owned(),
        request_sha256,
        content_sha256,
        content_range: content,
    })
}

fn take_field(raw: &[u8], cursor: &mut usize, key: u64, limit: usize) -> Result<Range<usize>> {
    if take_varint(raw, cursor)? != key {
        return Err(Error::Invalid("file content field"));
    }
    let length = take_varint(raw, cursor)?;
    if length > limit as u64 {
        return Err(Error::Limit);
    }
    if length > (raw.len() - *cursor) as u64 {
        return Err(Error::Invalid("file content span"));
    }
    let start = *cursor;
    *cursor += length as usize;
    Ok(start..*cursor)
}

fn take_varint(raw: &[u8], cursor: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..35).step_by(7) {
        let byte = *raw
            .get(*cursor)
            .ok_or(Error::Invalid("file content varint"))?;
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            if shift != 0 && byte == 0 {
                return Err(Error::Invalid("noncanonical file content varint"));
            }
            return Ok(value);
        }
    }
    Err(Error::Invalid("file content varint overflow"))
}
