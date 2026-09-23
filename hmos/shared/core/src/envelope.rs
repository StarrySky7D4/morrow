//! Versioned Protobuf + LZ4 container. SHA-256 detects corruption, not authorship.
use crate::{
    Error, Result,
    content::{CardRecord, MAX_RECORD_BYTES},
};
use sha2::{Digest, Sha256};
const MAGIC: &[u8; 8] = b"MORROWC1";
const HEADER: usize = 50; // magic + version(u16) + raw/packed lengths(u32) + SHA-256
pub const MAX_CONTAINER_BYTES: usize = MAX_RECORD_BYTES + MAX_RECORD_BYTES / 255 + 128;
pub fn encode(card: &CardRecord) -> Result<Vec<u8>> {
    pack(MAGIC, &card.encode(), MAX_RECORD_BYTES)
}
pub(crate) fn pack(magic: &[u8; 8], raw: &[u8], limit: usize) -> Result<Vec<u8>> {
    if raw.len() > limit {
        return Err(Error::Limit);
    }
    let packed = lz4_flex::block::compress(raw);
    let mut result = Vec::with_capacity(HEADER + packed.len());
    result.extend_from_slice(magic);
    result.extend_from_slice(&1u16.to_le_bytes());
    result.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    result.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    result.extend_from_slice(&Sha256::digest(raw));
    result.extend_from_slice(&packed);
    Ok(result)
}
pub fn decode(bytes: &[u8]) -> Result<CardRecord> {
    CardRecord::decode(&unpack(MAGIC, bytes, MAX_RECORD_BYTES)?)
}
pub(crate) fn unpack(magic: &[u8; 8], bytes: &[u8], limit: usize) -> Result<Vec<u8>> {
    if bytes.len() > limit + limit / 255 + 128 {
        return Err(Error::Limit);
    }
    if bytes.len() < HEADER || &bytes[..8] != magic {
        return Err(Error::Invalid("container header"));
    }
    if u16::from_le_bytes(bytes[8..10].try_into().unwrap()) != 1 {
        return Err(Error::UnsupportedVersion);
    }
    let raw_len = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let packed_len = u32::from_le_bytes(bytes[14..18].try_into().unwrap()) as usize;
    if raw_len > limit {
        return Err(Error::Limit);
    }
    if bytes.len() - HEADER != packed_len {
        return Err(Error::Invalid("container length"));
    }
    let mut raw = vec![0; raw_len];
    let written = lz4_flex::block::decompress_into(&bytes[HEADER..], &mut raw)
        .map_err(|_| Error::Invalid("lz4"))?;
    if written != raw_len {
        return Err(Error::Invalid("decoded length"));
    }
    if Sha256::digest(&raw)[..] != bytes[18..HEADER] {
        return Err(Error::Integrity);
    }
    Ok(raw)
}
