//! Versioned completed-read observations. The host supplies facts; this is neither
//! a content mutation nor evidence that a user received the response. Hashes detect
//! corruption, not authorship; enclosing audit signatures retain the original container.
use crate::{Error, Result, identity};
use prost::Message;
use sha2::{Digest, Sha256};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.read_journal.v1.rs"));
}
pub const MAGIC: &[u8; 8] = b"MORROWJ1";
pub const VERSION: u32 = 1;
pub const MAX_VALUE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_EVIDENCE: usize = 16;
pub const MAX_RAW_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
const HEADER: usize = 50;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub operation_id: String,
    pub subject: String,
    pub request_type: String,
    pub request: Vec<u8>,
    pub response_type: String,
    pub response: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub operation_id: String,
    pub subject: String,
    pub observation_sha256: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct ReadObservation {
    data: proto::ReadObservation,
    raw: Vec<u8>,
    container: Vec<u8>,
    digest: [u8; 32],
}
impl ReadObservation {
    pub fn data(&self) -> &proto::ReadObservation {
        &self.data
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn receipt(&self) -> Receipt {
        Receipt {
            operation_id: self.data.operation_id.clone(),
            subject: self.data.subject.clone(),
            observation_sha256: self.digest,
        }
    }
}
fn validate_input(value: &Input) -> Result<()> {
    for name in [
        &value.operation_id,
        &value.subject,
        &value.request_type,
        &value.response_type,
    ] {
        identity(name)?;
    }
    if value.request.len() > MAX_VALUE_BYTES || value.response.len() > MAX_VALUE_BYTES {
        return Err(Error::Limit);
    }
    Ok(())
}
fn validate(value: &proto::ReadObservation) -> Result<()> {
    if !matches!(value.schema_version, 1 | 2) {
        return Err(Error::UnsupportedVersion);
    }
    if (value.schema_version == 1 && !value.archive_sha256.is_empty())
        || (value.schema_version == 2 && value.archive_sha256.len() != 32)
    {
        return Err(Error::Integrity);
    }
    for name in [
        &value.operation_id,
        &value.subject,
        &value.request_type,
        &value.response_type,
    ] {
        identity(name)?;
    }
    if value.request.len() > MAX_VALUE_BYTES
        || value.response.len() > MAX_VALUE_BYTES
        || value.task_evidence_sha256.len() > MAX_EVIDENCE
    {
        return Err(Error::Limit);
    }
    if value.request_sha256.as_slice() != Sha256::digest(&value.request).as_slice()
        || value.response_sha256.as_slice() != Sha256::digest(&value.response).as_slice()
        || value.task_evidence_sha256.iter().any(|v| v.len() != 32)
    {
        return Err(Error::Integrity);
    }
    Ok(())
}
// Every owned variable-size field is bounded before prost allocates. Unknown fields
// remain in raw bytes; groups and duplicate known singular fields are rejected.
fn preflight(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field};
    let mut seen = [0usize; 12];
    let mut count = 0usize;
    while !raw.is_empty() {
        count += 1;
        if count > 128 {
            return Err(Error::Limit);
        }
        let (number, wire) =
            decode_key(&mut raw).map_err(|_| Error::Invalid("read journal field"))?;
        if number <= 11 {
            seen[number as usize] += 1;
            if seen[number as usize] > if number == 10 { MAX_EVIDENCE } else { 1 } {
                return Err(Error::Invalid("duplicate read journal field"));
            }
            let expected = if number == 1 {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            };
            if wire != expected {
                return Err(Error::Invalid("read journal wire type"));
            }
        }
        if wire == WireType::LengthDelimited {
            let len = decode_varint(&mut raw).map_err(|_| Error::Invalid("read journal length"))?;
            let max = match number {
                2..=4 | 7 => 256,
                5 | 8 => MAX_VALUE_BYTES,
                6 | 9 | 10 | 11 => 32,
                _ => MAX_RAW_BYTES,
            };
            if len > max as u64 {
                return Err(Error::Limit);
            }
            if len > raw.len() as u64 {
                return Err(Error::Invalid("read journal span"));
            }
            raw = &raw[len as usize..];
        } else if number == 1 {
            let version =
                decode_varint(&mut raw).map_err(|_| Error::Invalid("read journal version"))?;
            if !matches!(version, 1 | 2) {
                return Err(Error::UnsupportedVersion);
            }
        } else {
            if matches!(wire, WireType::StartGroup | WireType::EndGroup) {
                return Err(Error::Invalid("read journal group"));
            }
            skip_field(wire, number, &mut raw, DecodeContext::default())
                .map_err(|_| Error::Invalid("read journal unknown field"))?;
        }
    }
    Ok(())
}
pub fn encode(input: &Input, evidence: &[[u8; 32]]) -> Result<ReadObservation> {
    encode_inner(input, evidence, None)
}
pub fn encode_archived(
    input: &Input,
    evidence: &[[u8; 32]],
    archive: [u8; 32],
) -> Result<ReadObservation> {
    encode_inner(input, evidence, Some(archive))
}
fn encode_inner(
    input: &Input,
    evidence: &[[u8; 32]],
    archive: Option<[u8; 32]>,
) -> Result<ReadObservation> {
    validate_input(input)?;
    if evidence.len() > MAX_EVIDENCE {
        return Err(Error::Limit);
    }
    let data = proto::ReadObservation {
        schema_version: if archive.is_some() { 2 } else { VERSION },
        operation_id: input.operation_id.clone(),
        subject: input.subject.clone(),
        request_type: input.request_type.clone(),
        request: input.request.clone(),
        request_sha256: Sha256::digest(&input.request).to_vec(),
        response_type: input.response_type.clone(),
        response: input.response.clone(),
        response_sha256: Sha256::digest(&input.response).to_vec(),
        task_evidence_sha256: evidence.iter().map(|d| d.to_vec()).collect(),
        archive_sha256: archive.map(|v| v.to_vec()).unwrap_or_default(),
    };
    let raw = data.encode_to_vec();
    if raw.len() > MAX_RAW_BYTES {
        return Err(Error::Limit);
    }
    let digest: [u8; 32] = Sha256::digest(&raw).into();
    let packed = lz4_flex::block::compress(&raw);
    let mut container = Vec::with_capacity(HEADER + packed.len());
    container.extend_from_slice(MAGIC);
    container.extend_from_slice(&1u16.to_le_bytes());
    container.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    container.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    container.extend_from_slice(&digest);
    container.extend_from_slice(&packed);
    if container.len() > MAX_CONTAINER_BYTES {
        return Err(Error::Limit);
    }
    Ok(ReadObservation {
        data,
        raw,
        container,
        digest,
    })
}
pub fn decode(bytes: &[u8]) -> Result<ReadObservation> {
    if bytes.len() > MAX_CONTAINER_BYTES {
        return Err(Error::Limit);
    }
    if bytes.len() < HEADER || &bytes[..8] != MAGIC {
        return Err(Error::Invalid("read journal container"));
    }
    if bytes[8..10] != 1u16.to_le_bytes() {
        return Err(Error::UnsupportedVersion);
    }
    let length =
        u32::from_le_bytes(bytes[10..14].try_into().map_err(|_| Error::Integrity)?) as usize;
    let packed =
        u32::from_le_bytes(bytes[14..18].try_into().map_err(|_| Error::Integrity)?) as usize;
    if length > MAX_RAW_BYTES {
        return Err(Error::Limit);
    }
    if bytes.len() - HEADER != packed {
        return Err(Error::Invalid("read journal container length"));
    }
    let mut raw = vec![0; length];
    let actual = lz4_flex::block::decompress_into(&bytes[HEADER..], &mut raw)
        .map_err(|_| Error::Invalid("read journal compression"))?;
    let digest: [u8; 32] = Sha256::digest(&raw).into();
    if actual != length || bytes[18..50] != digest {
        return Err(Error::Integrity);
    }
    preflight(&raw)?;
    let data = proto::ReadObservation::decode(raw.as_slice())
        .map_err(|_| Error::Invalid("read journal protobuf"))?;
    validate(&data)?;
    Ok(ReadObservation {
        data,
        raw,
        container: bytes.to_vec(),
        digest,
    })
}
