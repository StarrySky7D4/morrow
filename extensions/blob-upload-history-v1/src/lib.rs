//! Selected upload history, independent of the private host protocol and Store.
//! Decoding correlates data; it never authenticates the sender, restores a live
//! grant, completes metadata, starts an upload, or proves resource retirement.
#![deny(unsafe_op_in_unsafe_fn)]

use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
    traits::{HasStructSize, IntoInternalStructReader},
};
use std::fmt;

#[allow(unsafe_op_in_unsafe_fn)]
mod blob_upload_history_capnp {
    include!(concat!(env!("OUT_DIR"), "/blob_upload_history_capnp.rs"));
}
use blob_upload_history_capnp as wire;
include!(concat!(env!("OUT_DIR"), "/schema_digest.rs"));
pub mod ffi;
mod task;
pub use task::{TASK_ID_BYTES, task_id};

pub const PROFILE: &str = "blob-upload-history-v1";
pub const INPUT_TYPE: &str = "morrow.blob.upload.history.v1";
pub const VERSION: u16 = 1;
pub const MAX_PACKET_BYTES: usize = 8192;
pub const MAX_ID_BYTES: usize = 256;
pub const MAX_OBJECT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    Invalid = 1,
    Contract = 2,
    Limit = 3,
    Correlation = 4,
    Buffer = 5,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blob-upload-history-v1: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
fn invalid<T>(_: T) -> Error {
    Error::Invalid
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Absent,
    Staged,
    Released,
    Error(u16),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Execution {
    pub outcome: std::result::Result<i32, u16>,
    pub host_calls: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OriginalResult {
    pub outcome: Outcome,
    pub unknown: bool,
    pub calls: u32,
    pub wire_bytes: u64,
    pub native_work_bytes: u64,
    pub execution: Option<Execution>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metadata {
    Reserved,
    Observed {
        result: OriginalResult,
        at: i64,
    },
    Acknowledged {
        result: OriginalResult,
        observed_at: i64,
        at: i64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Stage {
    Absent = 0,
    Staged = 1,
    Released = 2,
}

/// Historical correlations only. Private owner domains, nomination references,
/// live task keys and metadata-completion permits are deliberately absent.
#[derive(Clone, PartialEq, Eq)]
pub struct Record {
    pub submission: [u8; 32],
    pub package_id: String,
    pub package_digest: [u8; 32],
    pub registry_revision: u64,
    pub operation_id: String,
    pub byte_length: u64,
    pub whole_sha256: [u8; 32],
    pub handler: String,
    pub input_sha256: [u8; 32],
    pub request_binding_sha256: [u8; 32],
    pub reserved_at: i64,
    pub metadata: Metadata,
    /// Separate present-time observation. It cannot overwrite original Unknown.
    pub current_stage: Stage,
}
impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadHistoryRecord")
            .field("revision", &self.revision())
            .field("byte_length", &self.byte_length)
            .field("original_unknown", &self.original_unknown())
            .finish_non_exhaustive()
    }
}
fn identity(s: &str) -> Result<()> {
    if s.len() > MAX_ID_BYTES {
        return Err(Error::Limit);
    }
    if s.is_empty()
        || s.chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn id(value: &[u8]) -> Result<[u8; 32]> {
    let value = fixed(value)?;
    if value == [0; 32] {
        Err(Error::Invalid)
    } else {
        Ok(value)
    }
}
fn fixed(value: &[u8]) -> Result<[u8; 32]> {
    value.try_into().map_err(invalid)
}
impl Record {
    pub fn revision(&self) -> u16 {
        match self.metadata {
            Metadata::Reserved => 1,
            Metadata::Observed { .. } => 2,
            Metadata::Acknowledged { .. } => 3,
        }
    }
    pub fn original(&self) -> Option<OriginalResult> {
        match self.metadata {
            Metadata::Reserved => None,
            Metadata::Observed { result, .. } | Metadata::Acknowledged { result, .. } => {
                Some(result)
            }
        }
    }
    pub fn original_unknown(&self) -> bool {
        self.original().is_none_or(|r| r.unknown)
    }
    pub fn validate(&self) -> Result<()> {
        id(&self.submission)?;
        id(&self.package_digest)?;
        id(&self.request_binding_sha256)?;
        identity(&self.package_id)?;
        identity(&self.operation_id)?;
        identity(&self.handler)?;
        if self.byte_length > MAX_OBJECT_BYTES {
            return Err(Error::Limit);
        }
        if let Some(result) = self.original()
            && (matches!(result.outcome, Outcome::Error(e) if !(1..=10).contains(&e))
                || (result.outcome == Outcome::Error(7) && !result.unknown)
                || result
                    .execution
                    .is_some_and(|e| matches!(e.outcome, Err(v) if !(1..=9).contains(&v))))
        {
            return Err(Error::Invalid);
        }
        match self.metadata {
            Metadata::Observed { at, .. } if at < self.reserved_at => Err(Error::Invalid),
            Metadata::Acknowledged {
                observed_at, at, ..
            } if observed_at < self.reserved_at || at < observed_at => Err(Error::Invalid),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Packet {
    nonce: [u8; 32],
    record: Record,
}
impl fmt::Debug for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadHistoryPacket")
            .field("record", &self.record)
            .finish_non_exhaustive()
    }
}
impl Packet {
    /// Correlate to the separately delivered task ID. The task decoder must
    /// already validate its envelope and input type; this is not authentication
    /// or permission to read, deliver, retry, or acknowledge an upload.
    pub fn decode_task(bytes: &[u8], task_id: &str) -> Result<Self> {
        let (submission, nonce) = task::selection(task_id)?;
        Self::decode_for(bytes, submission, nonce)
    }
    pub fn new(nonce: [u8; 32], record: Record) -> Result<Self> {
        id(&nonce)?;
        record.validate()?;
        Ok(Self { nonce, record })
    }
    pub fn nonce(&self) -> [u8; 32] {
        self.nonce
    }
    pub fn record(&self) -> &Record {
        &self.record
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.record.validate()?;
        let r = &self.record;
        let mut message = Builder::new_default();
        {
            let mut p = message.init_root::<wire::packet::Builder>();
            p.set_version(VERSION);
            p.set_schema_sha256(&SCHEMA_DIGEST);
            p.set_query_nonce(&self.nonce);
            p.set_submission(&r.submission);
            p.set_package_id(r.package_id.as_str());
            p.set_package_digest(&r.package_digest);
            p.set_registry_revision(r.registry_revision);
            p.set_operation_id(r.operation_id.as_str());
            p.set_byte_length(r.byte_length);
            p.set_whole_sha256(&r.whole_sha256);
            p.set_handler(r.handler.as_str());
            p.set_input_sha256(&r.input_sha256);
            p.set_request_binding_sha256(&r.request_binding_sha256);
            p.set_reserved_at(r.reserved_at);
            p.set_revision(r.revision());
            p.set_original_unknown(r.original_unknown());
            p.set_current_stage(match r.current_stage {
                Stage::Absent => wire::Stage::Absent,
                Stage::Staged => wire::Stage::Staged,
                Stage::Released => wire::Stage::Released,
            });
            if let Some(result) = r.original() {
                p.set_outcome(match result.outcome {
                    Outcome::Absent => wire::Outcome::Absent,
                    Outcome::Staged => wire::Outcome::Staged,
                    Outcome::Released => wire::Outcome::Released,
                    Outcome::Error(_) => wire::Outcome::Error,
                });
                if let Outcome::Error(e) = result.outcome {
                    p.set_error_code(e);
                }
                p.set_calls(result.calls);
                p.set_wire_bytes(result.wire_bytes);
                p.set_native_work_bytes(result.native_work_bytes);
                if let Some(e) = result.execution {
                    p.set_host_calls(e.host_calls);
                    match e.outcome {
                        Ok(code) => {
                            p.set_execution(wire::Execution::Returned);
                            p.set_exit_code(code);
                        }
                        Err(code) => {
                            p.set_execution(wire::Execution::Rejected);
                            p.set_execution_error(code);
                        }
                    }
                }
            }
            match r.metadata {
                Metadata::Reserved => (),
                Metadata::Observed { at, .. } => p.set_observed_at(at),
                Metadata::Acknowledged {
                    observed_at, at, ..
                } => {
                    p.set_observed_at(observed_at);
                    p.set_acknowledged_at(at);
                }
            }
        }
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_PACKET_BYTES {
            return Err(Error::Limit);
        }
        Ok(bytes)
    }
    pub fn decode_for(bytes: &[u8], submission: [u8; 32], nonce: [u8; 32]) -> Result<Self> {
        id(&submission)?;
        id(&nonce)?;
        if bytes.is_empty() || bytes.len() > MAX_PACKET_BYTES {
            return Err(Error::Limit);
        }
        let mut remaining = bytes;
        let message = serialize::read_message_from_flat_slice(
            &mut remaining,
            ReaderOptions {
                traversal_limit_in_words: Some(4096),
                nesting_limit: 16,
            },
        )
        .map_err(invalid)?;
        if !remaining.is_empty() {
            return Err(Error::Invalid);
        }
        let p = message
            .get_root::<wire::packet::Reader>()
            .map_err(invalid)?;
        let layout = p.into_internal_struct_reader();
        let expected = wire::packet::Builder::STRUCT_SIZE;
        if layout.get_data_section_size() != u32::from(expected.data) * 64
            || layout.get_pointer_section_size() != expected.pointers
            || p.total_size().map_err(invalid)?.cap_count != 0
            || p.get_version() != VERSION
            || p.get_schema_sha256().map_err(invalid)? != SCHEMA_DIGEST
        {
            return Err(Error::Contract);
        }
        if id(p.get_query_nonce().map_err(invalid)?)? != nonce
            || id(p.get_submission().map_err(invalid)?)? != submission
        {
            return Err(Error::Correlation);
        }
        let text = |value: capnp::Result<capnp::text::Reader<'_>>| -> Result<String> {
            let value = value.map_err(invalid)?;
            if value.as_bytes().len() > MAX_ID_BYTES {
                return Err(Error::Limit);
            }
            let text = value.to_str().map_err(invalid)?;
            identity(text)?;
            Ok(text.to_owned())
        };
        let outcome = match p.get_outcome().map_err(invalid)? {
            wire::Outcome::NotObserved => None,
            wire::Outcome::Absent => Some(Outcome::Absent),
            wire::Outcome::Staged => Some(Outcome::Staged),
            wire::Outcome::Released => Some(Outcome::Released),
            wire::Outcome::Error => Some(Outcome::Error(p.get_error_code())),
        };
        if !matches!(outcome, Some(Outcome::Error(_))) && p.get_error_code() != 0 {
            return Err(Error::Invalid);
        }
        let execution = match p.get_execution().map_err(invalid)? {
            wire::Execution::NotObserved
                if p.get_exit_code() == 0
                    && p.get_execution_error() == 0
                    && p.get_host_calls() == 0 =>
            {
                None
            }
            wire::Execution::Returned if p.get_execution_error() == 0 => Some(Execution {
                outcome: Ok(p.get_exit_code()),
                host_calls: p.get_host_calls(),
            }),
            wire::Execution::Rejected if p.get_exit_code() == 0 => Some(Execution {
                outcome: Err(p.get_execution_error()),
                host_calls: p.get_host_calls(),
            }),
            _ => return Err(Error::Invalid),
        };
        let result = outcome.map(|outcome| OriginalResult {
            outcome,
            unknown: p.get_original_unknown(),
            calls: p.get_calls(),
            wire_bytes: p.get_wire_bytes(),
            native_work_bytes: p.get_native_work_bytes(),
            execution,
        });
        let metadata = match (p.get_revision(), result) {
            (1, None)
                if p.get_original_unknown()
                    && p.get_calls() == 0
                    && p.get_wire_bytes() == 0
                    && p.get_native_work_bytes() == 0
                    && execution.is_none()
                    && p.get_observed_at() == 0
                    && p.get_acknowledged_at() == 0 =>
            {
                Metadata::Reserved
            }
            (2, Some(result)) if p.get_acknowledged_at() == 0 => Metadata::Observed {
                result,
                at: p.get_observed_at(),
            },
            (3, Some(result)) => Metadata::Acknowledged {
                result,
                observed_at: p.get_observed_at(),
                at: p.get_acknowledged_at(),
            },
            _ => return Err(Error::Invalid),
        };
        let current_stage = match p.get_current_stage().map_err(invalid)? {
            wire::Stage::Absent => Stage::Absent,
            wire::Stage::Staged => Stage::Staged,
            wire::Stage::Released => Stage::Released,
        };
        Self::new(
            nonce,
            Record {
                submission,
                package_id: text(p.get_package_id())?,
                package_digest: fixed(p.get_package_digest().map_err(invalid)?)?,
                registry_revision: p.get_registry_revision(),
                operation_id: text(p.get_operation_id())?,
                byte_length: p.get_byte_length(),
                whole_sha256: fixed(p.get_whole_sha256().map_err(invalid)?)?,
                handler: text(p.get_handler())?,
                input_sha256: fixed(p.get_input_sha256().map_err(invalid)?)?,
                request_binding_sha256: fixed(p.get_request_binding_sha256().map_err(invalid)?)?,
                reserved_at: p.get_reserved_at(),
                metadata,
                current_stage,
            },
        )
    }
}

#[cfg(test)]
mod tests;
