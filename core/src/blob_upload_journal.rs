//! Immutable upload correlations and original observations, without live authority.
//! A reservation with no observation remains uncertain after restart. Reading it
//! cannot recreate a Worker, TaskKey, Store lifetime or permission to retry.
use crate::{Error, Result, blob_stage::Command, envelope, identity};
use prost::Message;

mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.blob_upload_journal.v1.rs"
    ));
}
pub const MAX_RECORD_BYTES: usize = 8192;
pub const MAX_SUBMISSIONS: u64 = 1024;
pub const RESERVED_HISTORY_BYTES: u64 = 3 * MAX_RECORD_BYTES as u64;
pub const MAX_PAGE_SIZE: u32 = 128;
const MAGIC: &[u8; 8] = b"MORRBU01";

#[derive(Clone, PartialEq, Eq)]
pub struct Request {
    pub submission: [u8; 32],
    pub package_id: String,
    pub package_digest: [u8; 32],
    pub registry_revision: u64,
    pub nomination_ref: [u8; 32],
    pub command: Command,
    pub handler: String,
    pub input_sha256: [u8; 32],
    /// Digest of the original trusted request including its bounds. Correlation
    /// only; the caller still checks the current package and original owner.
    pub binding_sha256: [u8; 32],
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadRequestHistory")
            .field("byte_length", &self.command.byte_length)
            .finish_non_exhaustive()
    }
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        identity(&self.package_id)?;
        identity(&self.handler)?;
        self.command.validate()?;
        if self.submission == [0; 32]
            || self.package_digest == [0; 32]
            || self.nomination_ref == [0; 32]
            || self.binding_sha256 == [0; 32]
        {
            return Err(Error::Invalid("upload history identity"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Absent,
    Staged,
    Released,
    /// Original private host error, 1..=10. No replacement error is synthesized.
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
/// A trusted original owner's retained metadata attempt. A saved Record alone
/// is never authority to construct one or to resume an upload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetadataCompletion {
    Observation { result: OriginalResult, at: i64 },
    Acknowledgement { at: i64 },
}
impl OriginalResult {
    fn validate(&self) -> Result<()> {
        if matches!(self.outcome, Outcome::Error(code) if !(1..=10).contains(&code))
            || (self.outcome == Outcome::Error(7) && !self.unknown)
            || self
                .execution
                .is_some_and(|e| matches!(e.outcome, Err(code) if !(1..=9).contains(&code)))
        {
            return Err(Error::Invalid("upload history result"));
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Record {
    request: Request,
    reserved_at_unix_ms: i64,
    original_result: Option<OriginalResult>,
    observed_at_unix_ms: Option<i64>,
    acknowledged_at_unix_ms: Option<i64>,
}
impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadHistory")
            .field("revision", &self.revision())
            .field("original_unknown", &self.original_unknown())
            .finish_non_exhaustive()
    }
}
impl Record {
    pub fn request(&self) -> &Request {
        &self.request
    }
    pub fn reserved_at_unix_ms(&self) -> i64 {
        self.reserved_at_unix_ms
    }
    pub fn original_result(&self) -> Option<OriginalResult> {
        self.original_result
    }
    pub fn observed_at_unix_ms(&self) -> Option<i64> {
        self.observed_at_unix_ms
    }
    pub fn acknowledged_at_unix_ms(&self) -> Option<i64> {
        self.acknowledged_at_unix_ms
    }
    /// A saved result is an original-owner observation, not proof of UI delivery.
    /// Acknowledgement never changes an original Unknown into a known outcome.
    pub fn original_unknown(&self) -> bool {
        self.original_result.is_none_or(|r| r.unknown)
    }
    pub fn revision(&self) -> u32 {
        if self.acknowledged_at_unix_ms.is_some() {
            3
        } else if self.original_result.is_some() {
            2
        } else {
            1
        }
    }
    pub(crate) fn reserved(request: Request, now: i64) -> Result<Self> {
        let result = Self {
            request,
            reserved_at_unix_ms: now,
            original_result: None,
            observed_at_unix_ms: None,
            acknowledged_at_unix_ms: None,
        };
        result.validate()?;
        Ok(result)
    }
    pub(crate) fn observed(&self, result: OriginalResult, now: i64) -> Result<Self> {
        if self.revision() != 1 {
            return Err(Error::OperationConflict);
        }
        let mut next = self.clone();
        next.original_result = Some(result);
        next.observed_at_unix_ms = Some(now);
        next.validate()?;
        Ok(next)
    }
    pub(crate) fn acknowledged(&self, now: i64) -> Result<Self> {
        if self.revision() != 2 {
            return Err(Error::OperationConflict);
        }
        let mut next = self.clone();
        next.acknowledged_at_unix_ms = Some(now);
        next.validate()?;
        Ok(next)
    }
    pub(crate) fn follows(&self, previous: &Self) -> Result<()> {
        if self.request != previous.request
            || self.reserved_at_unix_ms != previous.reserved_at_unix_ms
            || self.revision() != previous.revision() + 1
            || (previous.original_result.is_some()
                && (self.original_result != previous.original_result
                    || self.observed_at_unix_ms != previous.observed_at_unix_ms))
        {
            return Err(Error::Integrity);
        }
        self.validate()
    }
    fn validate(&self) -> Result<()> {
        self.request.validate()?;
        if self.original_result.is_some() != self.observed_at_unix_ms.is_some()
            || self
                .observed_at_unix_ms
                .is_some_and(|v| v < self.reserved_at_unix_ms)
            || self
                .acknowledged_at_unix_ms
                .is_some_and(|v| self.observed_at_unix_ms.is_none_or(|observed| v < observed))
        {
            return Err(Error::Integrity);
        }
        if let Some(result) = self.original_result {
            result.validate()?;
        }
        Ok(())
    }
    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let r = &self.request;
        let original_result = self.original_result.map(|v| {
            let (kind, error) = match v.outcome {
                Outcome::Absent => (1, 0),
                Outcome::Staged => (2, 0),
                Outcome::Released => (3, 0),
                Outcome::Error(code) => (4, u32::from(code)),
            };
            let (has_execution, execution_fault, exit_code, host_calls) = match v.execution {
                None => (false, 0, 0, 0),
                Some(e) => match e.outcome {
                    Ok(code) => (true, 0, code, e.host_calls),
                    Err(fault) => (true, u32::from(fault), 0, e.host_calls),
                },
            };
            proto::OriginalResult {
                kind,
                error,
                unknown: v.unknown,
                calls: v.calls,
                wire_bytes: v.wire_bytes,
                native_work_bytes: v.native_work_bytes,
                has_execution,
                execution_fault,
                exit_code,
                host_calls,
            }
        });
        let value = proto::Record {
            schema_version: 1,
            request: Some(proto::Request {
                submission: r.submission.to_vec(),
                package_id: r.package_id.clone(),
                package_digest: r.package_digest.to_vec(),
                registry_revision: r.registry_revision,
                nomination_ref: r.nomination_ref.to_vec(),
                owner_domain: r.command.owner_domain.to_vec(),
                operation_id: r.command.operation_id.clone(),
                byte_length: r.command.byte_length,
                whole_sha256: r.command.sha256.to_vec(),
                handler: r.handler.clone(),
                input_sha256: r.input_sha256.to_vec(),
                binding_sha256: r.binding_sha256.to_vec(),
            }),
            reserved_at_unix_ms: self.reserved_at_unix_ms,
            original_result,
            observed_at_unix_ms: self.observed_at_unix_ms,
            acknowledged_at_unix_ms: self.acknowledged_at_unix_ms,
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
        if value.encode_to_vec() != raw {
            return Err(Error::Integrity);
        }
        let array = |v: Vec<u8>| v.try_into().map_err(|_| Error::Integrity);
        let r = value.request.ok_or(Error::Integrity)?;
        let original_result = value
            .original_result
            .map(|v| -> Result<OriginalResult> {
                let outcome = match (v.kind, v.error) {
                    (1, 0) => Outcome::Absent,
                    (2, 0) => Outcome::Staged,
                    (3, 0) => Outcome::Released,
                    (4, code @ 1..=10) => Outcome::Error(code as u16),
                    _ => return Err(Error::Integrity),
                };
                let execution = match (
                    v.has_execution,
                    v.execution_fault,
                    v.exit_code,
                    v.host_calls,
                ) {
                    (false, 0, 0, 0) => None,
                    (true, 0, code, host_calls) => Some(Execution {
                        outcome: Ok(code),
                        host_calls,
                    }),
                    (true, code @ 1..=9, 0, host_calls) => Some(Execution {
                        outcome: Err(code as u16),
                        host_calls,
                    }),
                    _ => return Err(Error::Integrity),
                };
                Ok(OriginalResult {
                    outcome,
                    unknown: v.unknown,
                    calls: v.calls,
                    wire_bytes: v.wire_bytes,
                    native_work_bytes: v.native_work_bytes,
                    execution,
                })
            })
            .transpose()?;
        let result = Self {
            request: Request {
                submission: array(r.submission)?,
                package_id: r.package_id,
                package_digest: array(r.package_digest)?,
                registry_revision: r.registry_revision,
                nomination_ref: array(r.nomination_ref)?,
                command: Command {
                    owner_domain: array(r.owner_domain)?,
                    operation_id: r.operation_id,
                    byte_length: r.byte_length,
                    sha256: array(r.whole_sha256)?,
                },
                handler: r.handler,
                input_sha256: array(r.input_sha256)?,
                binding_sha256: array(r.binding_sha256)?,
            },
            reserved_at_unix_ms: value.reserved_at_unix_ms,
            original_result,
            observed_at_unix_ms: value.observed_at_unix_ms,
            acknowledged_at_unix_ms: value.acknowledged_at_unix_ms,
        };
        result.validate()?;
        Ok(result)
    }
}
