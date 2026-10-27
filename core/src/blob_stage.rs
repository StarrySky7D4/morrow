//! Durable host-local staging identity, distinct from a live upload grant.
use crate::{Error, Result, attachment::BlobInfo, envelope, identity};
use prost::Message;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.blob_stage.v1.rs"));
}

pub const MAX_STAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_RECORD_BYTES: usize = 8192;
pub const MAX_OPERATIONS: u64 = 1024;
pub const RESERVED_HISTORY_BYTES: u64 = 2 * MAX_RECORD_BYTES as u64;
const MAGIC: &[u8; 8] = b"MORRBS01";

/// The trusted original host selects the domain and operation identity.
/// Knowing these values is not permission to read, stage or release bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Command {
    pub owner_domain: [u8; 32],
    pub operation_id: String,
    pub byte_length: u64,
    pub sha256: [u8; 32],
}
impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlobStageCommand")
            .field("byte_length", &self.byte_length)
            .finish_non_exhaustive()
    }
}
impl Command {
    pub fn validate(&self) -> Result<()> {
        identity(&self.operation_id)?;
        if self.owner_domain == [0; 32] {
            return Err(Error::Invalid("blob stage domain"));
        }
        if self.byte_length > MAX_STAGE_BYTES {
            return Err(Error::Limit);
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Record {
    command: Command,
    blob: BlobInfo,
    staged_at_unix_ms: i64,
    released_at_unix_ms: Option<i64>,
}
impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlobStageRecord")
            .field("byte_length", &self.command.byte_length)
            .field("released", &self.is_released())
            .finish_non_exhaustive()
    }
}
impl Record {
    pub fn command(&self) -> &Command {
        &self.command
    }
    /// A historical blob identity; after release it need not still exist.
    pub fn blob(&self) -> &BlobInfo {
        &self.blob
    }
    pub fn staged_at_unix_ms(&self) -> i64 {
        self.staged_at_unix_ms
    }
    pub fn released_at_unix_ms(&self) -> Option<i64> {
        self.released_at_unix_ms
    }
    pub fn is_released(&self) -> bool {
        self.released_at_unix_ms.is_some()
    }
    pub(crate) fn staged(command: Command, blob: BlobInfo, now: i64) -> Result<Self> {
        let value = Self {
            command,
            blob,
            staged_at_unix_ms: now,
            released_at_unix_ms: None,
        };
        value.validate()?;
        Ok(value)
    }
    pub(crate) fn released(&self, now: i64) -> Result<Self> {
        if self.is_released() {
            return Err(Error::OperationConflict);
        }
        let mut next = self.clone();
        next.released_at_unix_ms = Some(now);
        next.validate()?;
        Ok(next)
    }
    fn validate(&self) -> Result<()> {
        self.command.validate()?;
        self.blob.encode()?;
        if self.blob.byte_length != self.command.byte_length
            || self.blob.sha256 != self.command.sha256
            || self.blob.retired_at_unix_ms.is_some()
            || self.staged_at_unix_ms < self.blob.created_at_unix_ms
            || self
                .released_at_unix_ms
                .is_some_and(|v| v < self.staged_at_unix_ms)
        {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let value = proto::Record {
            schema_version: 1,
            owner_domain: self.command.owner_domain.to_vec(),
            operation_id: self.command.operation_id.clone(),
            byte_length: self.command.byte_length,
            sha256: self.command.sha256.to_vec(),
            blob_info: self.blob.encode()?,
            staged_at_unix_ms: self.staged_at_unix_ms,
            released_at_unix_ms: self.released_at_unix_ms,
        };
        let bytes = envelope::pack(MAGIC, &value.encode_to_vec(), 4096)?;
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(Error::Limit);
        }
        Ok(bytes)
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(Error::Limit);
        }
        let raw = envelope::unpack(MAGIC, bytes, 4096)?;
        let value = proto::Record::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
        if value.schema_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        let command = Command {
            owner_domain: value
                .owner_domain
                .as_slice()
                .try_into()
                .map_err(|_| Error::Integrity)?,
            operation_id: value.operation_id,
            byte_length: value.byte_length,
            sha256: value
                .sha256
                .as_slice()
                .try_into()
                .map_err(|_| Error::Integrity)?,
        };
        let record = Self {
            command,
            blob: BlobInfo::decode(&value.blob_info)?,
            staged_at_unix_ms: value.staged_at_unix_ms,
            released_at_unix_ms: value.released_at_unix_ms,
        };
        record.validate()?;
        Ok(record)
    }
}
