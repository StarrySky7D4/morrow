//! Experimental IO intent candidates. These immutable records carry historical
//! facts, not live authority, durable-commit receipts, or permission to send.
//! Store/CAS, audit capacity and evidence retention must be integrated before
//! a broker can use a committed transition to perform any external operation.
use crate::{Error, Result, envelope, identity, plugin_package::io};
use prost::Message;
use sha2::{Digest, Sha256};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.io_intent.v1.rs"));
}
pub use proto::{ObservationSource, Phase};
pub const MAGIC: &[u8; 8] = b"MROWIOI1";
pub const VERSION: u32 = 1;
pub const MAX_RAW_BYTES: usize = 4096;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;

/// Digests refer to the exact original request and protected evidence. They do
/// not prove that evidence has been retained or that a target is authorized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub operation_id: String,
    pub subject: String,
    pub package_sha256: [u8; 32],
    pub capability: io::IoCapability,
    pub protocol_sha256: [u8; 32],
    pub request_sha256: [u8; 32],
    pub approval_sha256: [u8; 32],
    pub target_sha256: [u8; 32],
    pub request_bytes: u64,
    pub response_limit: u64,
}
impl Command {
    fn validate(&self) -> Result<()> {
        identity(&self.operation_id)?;
        identity(&self.subject)?;
        if self.protocol_sha256 != io::schema_digest() {
            return Err(Error::UnsupportedVersion);
        }
        if self.request_bytes > io::MAX_JOB_BYTES || self.response_limit > io::MAX_JOB_BYTES {
            return Err(Error::Limit);
        }
        for digest in [
            &self.package_sha256,
            &self.request_sha256,
            &self.approval_sha256,
            &self.target_sha256,
        ] {
            if *digest == [0; 32] {
                return Err(Error::Invalid("empty IO digest"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// No dispatch boundary recorded; still needs new live authorization and CAS.
    AwaitFreshAuthorization,
    /// May have been sent even if transport failed before an observable response.
    ReconcileOnly,
    /// A response/reconciliation was recorded; not necessarily business success.
    AlreadyObserved,
    CancelledBeforeDispatch,
}
#[derive(Clone, Debug)]
pub struct Record {
    value: proto::Record,
    command: Command,
    raw: Vec<u8>,
    container: Vec<u8>,
    digest: [u8; 32],
}
impl Record {
    pub fn prepared(command: Command) -> Result<Self> {
        command.validate()?;
        Self::from_value(proto::Record {
            schema_version: VERSION,
            operation_id: command.operation_id,
            subject: command.subject,
            package_sha256: command.package_sha256.to_vec(),
            capability: command.capability.number() as u32,
            protocol_sha256: command.protocol_sha256.to_vec(),
            request_sha256: command.request_sha256.to_vec(),
            approval_sha256: command.approval_sha256.to_vec(),
            target_sha256: command.target_sha256.to_vec(),
            request_bytes: command.request_bytes,
            response_limit: command.response_limit,
            revision: 1,
            phase: Phase::Prepared as i32,
            ..Default::default()
        })
    }
    pub fn command(&self) -> &Command {
        &self.command
    }
    pub fn phase(&self) -> Phase {
        Phase::try_from(self.value.phase).expect("validated phase")
    }
    pub fn revision(&self) -> u64 {
        self.value.revision
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    /// Each immutable revision has a distinct audit event ID. The command ID
    /// remains stable across revisions and is reserved separately by Store.
    pub fn event_id(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(b"Morrow/io-intent/event/v1\0");
        hash.update(self.digest);
        let mut id = String::from("io-event-");
        for byte in hash.finalize() {
            use std::fmt::Write;
            write!(&mut id, "{byte:02x}").expect("String write");
        }
        id
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn data(&self) -> &proto::Record {
        &self.value
    }
    pub fn matches_command(&self, command: &Command) -> Result<()> {
        command.validate()?;
        if &self.command != command {
            return Err(Error::OperationConflict);
        }
        Ok(())
    }
    pub fn recovery(&self) -> Recovery {
        match self.phase() {
            Phase::Prepared => Recovery::AwaitFreshAuthorization,
            Phase::OutcomeUnknown => Recovery::ReconcileOnly,
            Phase::Observed => Recovery::AlreadyObserved,
            Phase::CancelledBeforeDispatch => Recovery::CancelledBeforeDispatch,
            Phase::InvalidPhase => unreachable!("validated phase"),
        }
    }
    /// This candidate must commit BEFORE contacting the backend. A crash between
    /// that commit and dispatch still leaves Unknown; absence of a response never
    /// proves absence of an external effect. Reusing it cannot authorize a resend.
    pub fn propose_dispatch_boundary(&self) -> Result<Self> {
        self.transition(Phase::OutcomeUnknown, None)
    }
    pub fn propose_cancel_before_dispatch(&self) -> Result<Self> {
        self.transition(Phase::CancelledBeforeDispatch, None)
    }
    pub fn propose_observation(&self, digest: [u8; 32], source: ObservationSource) -> Result<Self> {
        self.transition(Phase::Observed, Some((digest, source)))
    }
    fn transition(
        &self,
        phase: Phase,
        observation: Option<([u8; 32], ObservationSource)>,
    ) -> Result<Self> {
        let mut value = self.value.clone();
        value.phase = phase as i32;
        value.revision = self.revision().checked_add(1).ok_or(Error::Limit)?;
        value.previous_sha256 = self.digest.to_vec();
        if let Some((digest, source)) = observation {
            value.observation_sha256 = digest.to_vec();
            value.observation_source = source as i32;
        }
        let next = Self::from_value(value)?;
        self.verify_successor(&next)?;
        Ok(next)
    }
    /// Used by the future same-transaction ledger/audit CAS; standalone decoding
    /// cannot prove that an alleged predecessor exists in the authoritative store.
    pub fn verify_successor(&self, next: &Self) -> Result<()> {
        self.matches_command(next.command())?;
        if next.revision() != self.revision().checked_add(1).ok_or(Error::Limit)?
            || next.value.previous_sha256 != self.digest
        {
            return Err(Error::RevisionConflict);
        }
        if !matches!(
            (self.phase(), next.phase()),
            (
                Phase::Prepared,
                Phase::OutcomeUnknown | Phase::CancelledBeforeDispatch
            ) | (Phase::OutcomeUnknown, Phase::Observed)
        ) {
            return Err(Error::Invalid("IO intent transition"));
        }
        Ok(())
    }
    fn from_value(value: proto::Record) -> Result<Self> {
        let command = validate(&value)?;
        let raw = value.encode_to_vec();
        let container = envelope::pack(MAGIC, &raw, MAX_RAW_BYTES)?;
        let digest = Sha256::digest(&raw).into();
        Ok(Self {
            value,
            command,
            raw,
            container,
            digest,
        })
    }
    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        preflight(&raw)?;
        let value = proto::Record::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("IO intent protobuf"))?;
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical IO intent"));
        }
        let command = validate(&value)?;
        let digest = Sha256::digest(&raw).into();
        Ok(Self {
            value,
            command,
            raw,
            container: container.to_vec(),
            digest,
        })
    }
}
fn digest(value: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = value
        .try_into()
        .map_err(|_| Error::Invalid("IO digest length"))?;
    if value == [0; 32] {
        return Err(Error::Invalid("empty IO digest"));
    }
    Ok(value)
}
fn validate(value: &proto::Record) -> Result<Command> {
    if value.schema_version != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    let command = Command {
        operation_id: value.operation_id.clone(),
        subject: value.subject.clone(),
        package_sha256: digest(&value.package_sha256)?,
        capability: io::IoCapability::from_number(
            i32::try_from(value.capability).map_err(|_| Error::UnsupportedVersion)?,
        )?,
        protocol_sha256: digest(&value.protocol_sha256)?,
        request_sha256: digest(&value.request_sha256)?,
        approval_sha256: digest(&value.approval_sha256)?,
        target_sha256: digest(&value.target_sha256)?,
        request_bytes: value.request_bytes,
        response_limit: value.response_limit,
    };
    command.validate()?;
    let phase = Phase::try_from(value.phase).map_err(|_| Error::UnsupportedVersion)?;
    let source = ObservationSource::try_from(value.observation_source)
        .map_err(|_| Error::UnsupportedVersion)?;
    let expected_revision = match phase {
        Phase::Prepared => 1,
        Phase::OutcomeUnknown | Phase::CancelledBeforeDispatch => 2,
        Phase::Observed => 3,
        Phase::InvalidPhase => return Err(Error::Invalid("unspecified IO phase")),
    };
    if value.revision != expected_revision {
        return Err(Error::RevisionConflict);
    }
    if phase == Phase::Prepared {
        if !value.previous_sha256.is_empty() {
            return Err(Error::Invalid("initial IO predecessor"));
        }
    } else {
        digest(&value.previous_sha256)?;
    }
    if phase == Phase::Observed {
        digest(&value.observation_sha256)?;
        if source == ObservationSource::NoObservation {
            return Err(Error::Invalid("missing IO observation source"));
        }
    } else if !value.observation_sha256.is_empty() || source != ObservationSource::NoObservation {
        return Err(Error::Invalid("unexpected IO observation"));
    }
    Ok(command)
}
// Strict extension: reject unknown/duplicate fields and oversize owned values
// BEFORE prost allocates. Future record semantics require an explicit new version.
fn preflight(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut seen = [false; 17];
    while !raw.is_empty() {
        let (number, wire) = decode_key(&mut raw).map_err(|_| Error::Invalid("IO intent field"))?;
        if !(1..=16).contains(&number) {
            return Err(Error::UnsupportedVersion);
        }
        if std::mem::replace(&mut seen[number as usize], true) {
            return Err(Error::Invalid("duplicate IO intent field"));
        }
        let scalar = matches!(number, 1 | 5 | 10 | 11 | 12 | 14 | 16);
        if wire
            != if scalar {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err(Error::Invalid("IO intent wire type"));
        }
        let value = decode_varint(&mut raw).map_err(|_| Error::Invalid("IO intent value"))?;
        if scalar {
            continue;
        }
        let limit = if matches!(number, 2 | 3) { 256 } else { 32 };
        if value > limit {
            return Err(Error::Limit);
        }
        if value > raw.len() as u64 {
            return Err(Error::Invalid("IO intent span"));
        }
        raw = &raw[value as usize..];
    }
    Ok(())
}
