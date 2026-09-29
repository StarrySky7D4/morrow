//! Bounded historical result of one FileReplace attempt. It grants no path or OS authority.
use super::proto;
use crate::{Error, Result, envelope, file_content::MAX_CONTENT_BYTES, identity};
use prost::Message;
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 8] = b"MROWFER1";
pub const VERSION: u32 = 1;
pub const MAX_RAW_BYTES: usize = 1024;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplaceResult {
    /// Historical host observation only; grants no OS authority and does not
    /// imply durability through sudden power loss.
    Replaced,
    /// An OS API rejected the attempt; uncertain effects require reconciliation.
    OsRejected { code: u32 },
}

pub struct ReplaceOutcome {
    value: proto::ReplaceOutcome,
    request_sha256: [u8; 32],
    target_reference: [u8; 32],
    expected_identity: [u8; 32],
    content_sha256: [u8; 32],
    result: ReplaceResult,
    raw: Vec<u8>,
    container: Vec<u8>,
}

impl ReplaceOutcome {
    // All eight fields are independent domain bindings of one immutable outcome.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        operation_id: &str,
        subject: &str,
        request_sha256: [u8; 32],
        target_reference: [u8; 32],
        expected_identity: [u8; 32],
        content_sha256: [u8; 32],
        content_length: u64,
        result: ReplaceResult,
    ) -> Result<Self> {
        identity(operation_id)?;
        identity(subject)?;
        if [
            request_sha256,
            target_reference,
            expected_identity,
            content_sha256,
        ]
        .contains(&[0; 32])
        {
            return Err(Error::Invalid("empty file replace digest"));
        }
        if content_length > MAX_CONTENT_BYTES as u64 {
            return Err(Error::Limit);
        }
        if content_length == 0 && content_sha256 != <[u8; 32]>::from(Sha256::digest([])) {
            return Err(Error::Invalid("invalid empty replace content digest"));
        }
        let (result_number, os_error_code) = match result {
            ReplaceResult::Replaced => (1, 0),
            ReplaceResult::OsRejected { code: 0 } => {
                return Err(Error::Invalid("invalid file replace error code"));
            }
            ReplaceResult::OsRejected { code } => (2, code),
        };
        let value = proto::ReplaceOutcome {
            schema_version: VERSION,
            operation_id: operation_id.to_owned(),
            subject: subject.to_owned(),
            request_sha256: request_sha256.to_vec(),
            target_reference: target_reference.to_vec(),
            expected_identity: expected_identity.to_vec(),
            content_sha256: content_sha256.to_vec(),
            content_length,
            result: result_number,
            os_error_code,
        };
        let raw = value.encode_to_vec();
        let container = envelope::pack(MAGIC, &raw, MAX_RAW_BYTES)?;
        Self::from_parts(value, raw, container)
    }

    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        preflight(&raw)?;
        let value = proto::ReplaceOutcome::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("file replace outcome protobuf"))?;
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical file replace outcome"));
        }
        Self::from_parts(value, raw, container.to_vec())
    }

    fn from_parts(value: proto::ReplaceOutcome, raw: Vec<u8>, container: Vec<u8>) -> Result<Self> {
        if value.schema_version != VERSION {
            return Err(Error::UnsupportedVersion);
        }
        identity(&value.operation_id)?;
        identity(&value.subject)?;
        let request_sha256 = fixed_digest(&value.request_sha256)?;
        let target_reference = fixed_digest(&value.target_reference)?;
        let expected_identity = fixed_digest(&value.expected_identity)?;
        let content_sha256 = fixed_digest(&value.content_sha256)?;
        if value.content_length > MAX_CONTENT_BYTES as u64 {
            return Err(Error::Limit);
        }
        if value.content_length == 0 && content_sha256 != <[u8; 32]>::from(Sha256::digest([])) {
            return Err(Error::Invalid("invalid empty replace content digest"));
        }
        let result = match (value.result, value.os_error_code) {
            (1, 0) => ReplaceResult::Replaced,
            (2, code @ 1..) => ReplaceResult::OsRejected { code },
            (0, _) => return Err(Error::Invalid("missing file replace result")),
            (1 | 2, _) => return Err(Error::Invalid("invalid file replace error code")),
            _ => return Err(Error::UnsupportedVersion),
        };
        Ok(Self {
            value,
            request_sha256,
            target_reference,
            expected_identity,
            content_sha256,
            result,
            raw,
            container,
        })
    }

    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn operation_id(&self) -> &str {
        &self.value.operation_id
    }
    pub fn subject(&self) -> &str {
        &self.value.subject
    }
    pub fn request_sha256(&self) -> [u8; 32] {
        self.request_sha256
    }
    pub fn target_reference(&self) -> [u8; 32] {
        self.target_reference
    }
    pub fn expected_identity(&self) -> [u8; 32] {
        self.expected_identity
    }
    pub fn content_sha256(&self) -> [u8; 32] {
        self.content_sha256
    }
    pub fn content_length(&self) -> u64 {
        self.value.content_length
    }
    pub fn result(&self) -> ReplaceResult {
        self.result
    }
}

fn fixed_digest(bytes: &[u8]) -> Result<[u8; 32]> {
    let digest: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Invalid("file replace digest length"))?;
    if digest == [0; 32] {
        return Err(Error::Invalid("empty file replace digest"));
    }
    Ok(digest)
}

fn preflight(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut seen = [false; 11];
    while !raw.is_empty() {
        let (tag, wire) =
            decode_key(&mut raw).map_err(|_| Error::Invalid("file replace outcome field"))?;
        if !(1..=10).contains(&tag) {
            return Err(Error::UnsupportedVersion);
        }
        if std::mem::replace(&mut seen[tag as usize], true) {
            return Err(Error::Invalid("duplicate file replace outcome field"));
        }
        let scalar = matches!(tag, 1 | 8 | 9 | 10);
        if wire
            != if scalar {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err(Error::Invalid("file replace outcome wire type"));
        }
        let value =
            decode_varint(&mut raw).map_err(|_| Error::Invalid("file replace outcome value"))?;
        if scalar {
            if tag != 8 && value > u32::MAX as u64 {
                return Err(Error::Invalid("file replace scalar overflow"));
            }
        } else {
            let limit = if matches!(tag, 2 | 3) { 256 } else { 32 };
            if value > limit {
                return Err(Error::Limit);
            }
            if value > raw.len() as u64 {
                return Err(Error::Invalid("file replace outcome span"));
            }
            raw = &raw[value as usize..];
        }
    }
    Ok(())
}
