//! Bounded historical results of file Create, Replace, and Delete attempts. These records
//! grant no access or path authority; the platform must bind an opened target.
use crate::{Error, Result, envelope, identity};
use prost::Message;

mod create;
mod replace;
pub use create::{CreateOutcome, CreateResult, MAX_CONTAINER_BYTES as MAX_CREATE_CONTAINER_BYTES};
pub use replace::{
    MAX_CONTAINER_BYTES as MAX_REPLACE_CONTAINER_BYTES, ReplaceOutcome, ReplaceResult,
};

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.file_effect.v1.rs"));
}

pub const MAGIC: &[u8; 8] = b"MROWFED1";
pub const VERSION: u32 = 1;
pub const MAX_RAW_BYTES: usize = 1024;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteResult {
    /// The selected directory entry was deleted. Other hard links may remain,
    /// and this does not assert sudden-power-loss durability.
    Deleted,
    /// The OS API reported failure. This does not prove that no underlying
    /// side effect occurred; an uncertain outcome still needs reconciliation.
    OsRejected { code: u32 },
}

pub struct DeleteOutcome {
    value: proto::DeleteOutcome,
    request_sha256: [u8; 32],
    target_reference: [u8; 32],
    expected_identity: [u8; 32],
    result: DeleteResult,
    raw: Vec<u8>,
    container: Vec<u8>,
}

impl DeleteOutcome {
    pub fn new(
        operation_id: &str,
        subject: &str,
        request_sha256: [u8; 32],
        target_reference: [u8; 32],
        expected_identity: [u8; 32],
        result: DeleteResult,
    ) -> Result<Self> {
        // Reject oversized/invalid caller strings before cloning or protobuf allocation.
        identity(operation_id)?;
        identity(subject)?;
        if [request_sha256, target_reference, expected_identity].contains(&[0; 32]) {
            return Err(Error::Invalid("empty file delete digest"));
        }
        if result == (DeleteResult::OsRejected { code: 0 }) {
            return Err(Error::Invalid("invalid file delete error code"));
        }
        let (result_number, os_error_code) = match result {
            DeleteResult::Deleted => (1, 0),
            DeleteResult::OsRejected { code } => (2, code),
        };
        let value = proto::DeleteOutcome {
            schema_version: VERSION,
            operation_id: operation_id.to_owned(),
            subject: subject.to_owned(),
            request_sha256: request_sha256.to_vec(),
            target_reference: target_reference.to_vec(),
            expected_identity: expected_identity.to_vec(),
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
        let value = proto::DeleteOutcome::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("file delete outcome protobuf"))?;
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical file delete outcome"));
        }
        Self::from_parts(value, raw, container.to_vec())
    }

    fn from_parts(value: proto::DeleteOutcome, raw: Vec<u8>, container: Vec<u8>) -> Result<Self> {
        if value.schema_version != VERSION {
            return Err(Error::UnsupportedVersion);
        }
        identity(&value.operation_id)?;
        identity(&value.subject)?;
        let request_sha256 = fixed_digest(&value.request_sha256)?;
        let target_reference = fixed_digest(&value.target_reference)?;
        let expected_identity = fixed_digest(&value.expected_identity)?;
        let result = match (value.result, value.os_error_code) {
            (1, 0) => DeleteResult::Deleted,
            (2, code @ 1..) => DeleteResult::OsRejected { code },
            (0, _) => return Err(Error::Invalid("missing file delete result")),
            (1 | 2, _) => return Err(Error::Invalid("invalid file delete error code")),
            _ => return Err(Error::UnsupportedVersion),
        };
        Ok(Self {
            value,
            request_sha256,
            target_reference,
            expected_identity,
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
    pub fn result(&self) -> DeleteResult {
        self.result
    }
}

fn fixed_digest(bytes: &[u8]) -> Result<[u8; 32]> {
    let digest: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Invalid("file delete digest length"))?;
    if digest == [0; 32] {
        return Err(Error::Invalid("empty file delete digest"));
    }
    Ok(digest)
}

fn preflight(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut seen = [false; 9];
    while !raw.is_empty() {
        let (tag, wire) =
            decode_key(&mut raw).map_err(|_| Error::Invalid("file delete outcome field"))?;
        if !(1..=8).contains(&tag) {
            return Err(Error::UnsupportedVersion);
        }
        if std::mem::replace(&mut seen[tag as usize], true) {
            return Err(Error::Invalid("duplicate file delete outcome field"));
        }
        let scalar = matches!(tag, 1 | 7 | 8);
        if wire
            != if scalar {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err(Error::Invalid("file delete outcome wire type"));
        }
        let value =
            decode_varint(&mut raw).map_err(|_| Error::Invalid("file delete outcome value"))?;
        if scalar {
            if value > u32::MAX as u64 {
                return Err(Error::Invalid("file delete scalar overflow"));
            }
        } else {
            let limit = if matches!(tag, 2 | 3) { 256 } else { 32 };
            if value > limit {
                return Err(Error::Limit);
            }
            if value > raw.len() as u64 {
                return Err(Error::Invalid("file delete outcome span"));
            }
            raw = &raw[value as usize..];
        }
    }
    Ok(())
}
