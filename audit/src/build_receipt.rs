//! Bounded, unsigned build evidence. Hashes bind recorded bytes, not their author,
//! truth, reproducibility, or relation to a successful product/device qualification.
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.build_receipt.v1.rs"));
}
pub const VERSION: u32 = 1;
pub const MAX_RAW_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CONTAINER_BYTES: usize = HEADER + MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 16;
pub const MAX_SOURCES: usize = 10_000;
pub const MAX_COMMANDS: usize = 64;
pub const MAX_ARTIFACTS: usize = 256;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_ARGUMENTS: usize = 256;
pub const MAX_ARGUMENT_BYTES: usize = 8192;
pub const MAX_FEATURES: usize = 64;
const MAGIC: &[u8; 8] = b"MORROWB1";
const HEADER: usize = 50;
const MAX_FIELDS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Version,
    Limit,
    Integrity,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "build receipt: {self:?}")
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

fn text(value: &str, maximum: usize, empty: bool) -> Result<()> {
    if value.len() > maximum {
        return Err(Error::Limit);
    }
    if (!empty && value.is_empty()) || value.contains('\0') {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn identifier(value: &str) -> Result<()> {
    text(value, 128, false)?;
    if !value.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn file(entry: &proto::FileEntry) -> Result<()> {
    text(&entry.path, MAX_PATH_BYTES, false)?;
    if entry.sha256.len() != 32
        || entry.path.chars().any(|c| c.is_control())
        || entry.path.contains(['\\', ':'])
        || entry
            .path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.ends_with([' ', '.']))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
/// Sources use paths relative to run/source. Input must already be in strictly
/// increasing UTF-8 path order; this function never silently sorts or deduplicates.
/// This semantic digest deliberately does not use Protobuf serialization.
pub fn source_digest(sources: &[proto::FileEntry]) -> Result<[u8; 32]> {
    if sources.is_empty() {
        return Err(Error::Invalid);
    }
    if sources.len() > MAX_SOURCES {
        return Err(Error::Limit);
    }
    let mut hash = Sha256::new();
    hash.update(b"Morrow/build-sources/v1\0");
    hash.update((sources.len() as u64).to_le_bytes());
    let mut previous: Option<&str> = None;
    for entry in sources {
        file(entry)?;
        if previous.is_some_and(|p| p >= entry.path.as_str()) {
            return Err(Error::Invalid);
        }
        previous = Some(&entry.path);
        hash.update((entry.path.len() as u32).to_le_bytes());
        hash.update(entry.path.as_bytes());
        hash.update(entry.size.to_le_bytes());
        hash.update(&entry.sha256);
    }
    Ok(hash.finalize().into())
}
fn validate(value: &proto::BuildReceipt) -> Result<()> {
    if value.version != VERSION {
        return Err(Error::Version);
    }
    if !matches!(value.source_head.len(), 40 | 64)
        || !value
            .source_head
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || value.collector_sha256.len() != 32
        || value.source_root_sha256.len() != 32
        || value.started_unix_ms == 0
        || value.finished_unix_ms < value.started_unix_ms
    {
        return Err(Error::Invalid);
    }
    if source_digest(&value.sources)?.as_slice() != value.source_root_sha256 {
        return Err(Error::Integrity);
    }
    identifier(&value.profile)?;
    identifier(&value.target)?;
    text(&value.rustc_version, MAX_ARGUMENT_BYTES, false)?;
    text(&value.cargo_version, MAX_ARGUMENT_BYTES, false)?;
    if value.features.len() > MAX_FEATURES
        || value.commands.len() > MAX_COMMANDS
        || value.artifacts.len() > MAX_ARTIFACTS
    {
        return Err(Error::Limit);
    }
    let mut features = BTreeSet::new();
    for feature in &value.features {
        identifier(feature)?;
        if !features.insert(feature) {
            return Err(Error::Invalid);
        }
    }
    if value.commands.is_empty() {
        return Err(Error::Invalid);
    }
    let mut labels = BTreeSet::new();
    let mut run_paths = BTreeSet::new();
    let mut has_build = false;
    for command in &value.commands {
        identifier(&command.label)?;
        text(&command.program, MAX_PATH_BYTES, false)?;
        if !labels.insert(&command.label) {
            return Err(Error::Invalid);
        }
        if command.args.len() > MAX_ARGUMENTS {
            return Err(Error::Limit);
        }
        for argument in &command.args {
            text(argument, MAX_ARGUMENT_BYTES, true)?;
        }
        for log in [&command.stdout, &command.stderr] {
            let log = log.as_ref().ok_or(Error::Invalid)?;
            file(log)?;
            if !run_paths.insert(&log.path) {
                return Err(Error::Invalid);
            }
        }
        if (!command.completed && command.exit_code != 0)
            || (value.build_succeeded && (!command.completed || command.exit_code != 0))
        {
            return Err(Error::Invalid);
        }
        has_build |= command.label == "build" && command.completed && command.exit_code == 0;
    }
    for artifact in &value.artifacts {
        file(artifact)?;
        if !run_paths.insert(&artifact.path) {
            return Err(Error::Invalid);
        }
    }
    if value.build_succeeded && (!has_build || value.artifacts.is_empty()) {
        return Err(Error::Invalid);
    }
    Ok(())
}

pub fn encode(value: &proto::BuildReceipt) -> Result<Vec<u8>> {
    validate(value)?;
    if value.encoded_len() > MAX_RAW_BYTES {
        return Err(Error::Limit);
    }
    let raw = value.encode_to_vec();
    let packed = lz4_flex::block::compress(&raw);
    let mut bytes = Vec::with_capacity(HEADER + packed.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(VERSION as u16).to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(&raw));
    bytes.extend_from_slice(&packed);
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<proto::BuildReceipt> {
    if bytes.len() > MAX_CONTAINER_BYTES {
        return Err(Error::Limit);
    }
    if bytes.len() < HEADER || &bytes[..8] != MAGIC {
        return Err(Error::Invalid);
    }
    if u16::from_le_bytes(bytes[8..10].try_into().map_err(|_| Error::Invalid)?) != VERSION as u16 {
        return Err(Error::Version);
    }
    let raw_len =
        u32::from_le_bytes(bytes[10..14].try_into().map_err(|_| Error::Invalid)?) as usize;
    let packed_len =
        u32::from_le_bytes(bytes[14..18].try_into().map_err(|_| Error::Invalid)?) as usize;
    if raw_len == 0 || raw_len > MAX_RAW_BYTES {
        return Err(Error::Limit);
    }
    if packed_len != bytes.len() - HEADER {
        return Err(Error::Invalid);
    }
    // Never pass an attacker-provided unbounded size to allocating decompression.
    let mut raw = vec![0; raw_len];
    let written =
        lz4_flex::block::decompress_into(&bytes[HEADER..], &mut raw).map_err(|_| Error::Invalid)?;
    if written != raw_len {
        return Err(Error::Invalid);
    }
    if Sha256::digest(&raw).as_slice() != &bytes[18..50] {
        return Err(Error::Integrity);
    }
    preflight(&raw, Shape::Receipt, &mut 0)?;
    let value = proto::BuildReceipt::decode(raw.as_slice()).map_err(|_| Error::Invalid)?;
    validate(&value)?;
    Ok(value)
}

#[derive(Clone, Copy)]
enum Shape {
    Receipt,
    Command,
    File,
}
fn varint(bytes: &[u8], position: &mut usize) -> Result<u64> {
    let mut result = 0u64;
    for index in 0..10 {
        let byte = *bytes.get(*position).ok_or(Error::Invalid)?;
        *position += 1;
        if index == 9 && byte > 1 {
            return Err(Error::Invalid);
        }
        result |= u64::from(byte & 0x7f) << (index * 7);
        if byte < 0x80 {
            if index != 0 && byte == 0 {
                return Err(Error::Invalid);
            }
            return Ok(result);
        }
    }
    Err(Error::Invalid)
}
fn preflight(bytes: &[u8], shape: Shape, fields: &mut usize) -> Result<()> {
    let mut position = 0;
    let mut seen = 0u32;
    let mut repeated = [0usize; 17];
    while position < bytes.len() {
        *fields += 1;
        if *fields > MAX_FIELDS {
            return Err(Error::Limit);
        }
        let key = varint(bytes, &mut position)?;
        let number = usize::try_from(key >> 3).map_err(|_| Error::Invalid)?;
        let wire = key & 7;
        // Unknown fields are unsupported by this version, not silently discarded.
        let (expected, maximum, nested, scalar_max) = match (shape, number) {
            (Shape::Receipt, 1) => (0, 1, None, u32::MAX as u64),
            (Shape::Receipt, 3 | 14) | (Shape::Command, 5) => (0, 1, None, 1),
            (Shape::Receipt, 15 | 16) | (Shape::File, 2) => (0, 1, None, u64::MAX),
            (Shape::Command, 4) => (0, 1, None, u32::MAX as u64),
            (Shape::Receipt, 5) => (2, MAX_SOURCES, Some(Shape::File), 0),
            (Shape::Receipt, 8) => (2, MAX_FEATURES, None, 0),
            (Shape::Receipt, 12) => (2, MAX_COMMANDS, Some(Shape::Command), 0),
            (Shape::Receipt, 13) => (2, MAX_ARTIFACTS, Some(Shape::File), 0),
            (Shape::Command, 3) => (2, MAX_ARGUMENTS, None, 0),
            (Shape::Command, 6 | 7) => (2, 1, Some(Shape::File), 0),
            (Shape::Receipt, 2 | 4 | 6 | 7 | 9 | 10 | 11)
            | (Shape::Command, 1 | 2)
            | (Shape::File, 1 | 3) => (2, 1, None, 0),
            _ => return Err(Error::Invalid),
        };
        if expected != wire {
            return Err(Error::Invalid);
        }
        if maximum == 1 {
            let bit = 1u32 << number;
            if seen & bit != 0 {
                return Err(Error::Invalid);
            }
            seen |= bit;
        } else {
            repeated[number] += 1;
            if repeated[number] > maximum {
                return Err(Error::Limit);
            }
        }
        if wire == 0 {
            if varint(bytes, &mut position)? > scalar_max {
                return Err(Error::Invalid);
            }
        } else {
            let length =
                usize::try_from(varint(bytes, &mut position)?).map_err(|_| Error::Limit)?;
            let end = position.checked_add(length).ok_or(Error::Limit)?;
            let field = bytes.get(position..end).ok_or(Error::Invalid)?;
            if let Some(nested) = nested {
                preflight(field, nested, fields)?;
            }
            position = end;
        }
    }
    Ok(())
}
