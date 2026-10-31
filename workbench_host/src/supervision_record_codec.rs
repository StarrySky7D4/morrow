//! Typed durable owner records. Checksums detect corruption, not authorship.
//! No decoder can establish process exit, resource release or fresh admission.
use crate::Result;
use prost::Message;
use sha2::{Digest, Sha256};

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.supervision.v1.rs"
    ));
}
use proto::{BusinessOutcome, ObservedTruth, ProtocolAck};
pub use proto::{Identity, OwnerPhase, OwnerRecord, ReaderJoins, ResourceFacts};

const OWNER_MAGIC: &[u8; 8] = b"MOROWN01";
const RECEIPT_MAGIC: &[u8; 8] = b"MORREC01";
const HEADER: usize = 50;
pub const MAX_RAW: usize = 65536;
pub const MAX_CONTAINER: usize = MAX_RAW + MAX_RAW / 255 + 128;

fn pack(magic: &[u8; 8], raw: &[u8]) -> Result<Vec<u8>> {
    if raw.is_empty() || raw.len() > MAX_RAW {
        return Err("Supervisor record size limit".into());
    }
    let compressed = lz4_flex::block::compress(raw);
    let mut out = Vec::with_capacity(HEADER + compressed.len());
    out.extend_from_slice(magic);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend_from_slice(&compressed);
    if out.len() > MAX_CONTAINER {
        return Err("Supervisor container size limit".into());
    }
    Ok(out)
}

fn unpack(magic: &[u8; 8], bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() < HEADER || bytes.len() > MAX_CONTAINER || &bytes[..8] != magic {
        return Err("Supervisor binary container header".into());
    }
    if u16::from_le_bytes(bytes[8..10].try_into()?) != 1 {
        return Err("Unsupported supervisor container version".into());
    }
    let raw_len = u32::from_le_bytes(bytes[10..14].try_into()?) as usize;
    let packed_len = u32::from_le_bytes(bytes[14..18].try_into()?) as usize;
    if raw_len == 0 || raw_len > MAX_RAW || bytes.len() - HEADER != packed_len {
        return Err("Supervisor binary container lengths".into());
    }
    let mut raw = vec![0; raw_len];
    if lz4_flex::block::decompress_into(&bytes[HEADER..], &mut raw)? != raw_len
        || Sha256::digest(&raw)[..] != bytes[18..HEADER]
    {
        return Err("Supervisor binary container integrity".into());
    }
    // Our writer is canonical. Refuse extra compressed bytes and alternative
    // representations before they can enter an exact-row CAS or hash chain.
    if pack(magic, &raw)? != bytes {
        return Err("Noncanonical supervisor binary container".into());
    }
    Ok(raw)
}

fn decode_canonical<T: Message + Default>(raw: &[u8]) -> Result<T> {
    let value = T::decode(raw)?;
    if value.encode_to_vec() != raw {
        return Err("Unknown, repeated or noncanonical supervisor record fields".into());
    }
    Ok(value)
}

pub fn identity(record: &OwnerRecord) -> Result<&Identity> {
    record
        .identity
        .as_ref()
        .ok_or_else(|| "Supervisor identity missing".into())
}

pub fn facts(record: &OwnerRecord) -> Result<&ResourceFacts> {
    record
        .resources
        .as_ref()
        .ok_or_else(|| "Supervisor resource facts missing".into())
}

pub fn resources_closed(f: &ResourceFacts) -> bool {
    f.resource_reclaimed
        && f.gate_closed
        && f.tree_empty
        && f.child_exit
        && f.stdout_eof
        && f.stderr_eof
        && f.control_reaped
}

pub fn normal_release(record: &OwnerRecord) -> bool {
    record.resources.as_ref().is_some_and(|f| {
        resources_closed(f)
            && f.normal_shutdown == ObservedTruth::Yes as i32
            && f.child_exit_code == Some(0)
    })
}

pub fn complete_abnormal_proof(record: &OwnerRecord) -> bool {
    record.model_version == 2
        && record.business_outcome == BusinessOutcome::Unknown as i32
        && record.protocol_ack == ProtocolAck::NotApplicable as i32
        && record.resources.as_ref().is_some_and(|f| {
            resources_closed(f)
                && f.business_gate_revoked
                && f.normal_shutdown == ObservedTruth::No as i32
                && f.child_exit_code.is_some()
        })
}

pub fn joined(joins: &ReaderJoins) -> bool {
    joins.version == 1
        && joins.stdout_reader_joined
        && joins.stderr_reader_joined
        && joins.controller_reader_joined
}

pub fn validate(record: &OwnerRecord) -> Result<()> {
    if record.model_version != 2
        || record.phase == 0
        || OwnerPhase::try_from(record.phase).is_err()
        || record.business_outcome != BusinessOutcome::Unknown as i32
        || record.protocol_ack != ProtocolAck::NotApplicable as i32
    {
        return Err("Unsupported or invalid supervisor owner record".into());
    }
    let id = identity(record)?;
    if id.generation == 0
        || id.supervisor_pid == 0
        || id.supervisor_creation_filetime == 0
        || [
            &id.profile,
            &id.incarnation,
            &id.host_sha256,
            &id.package_sha256,
            &id.supervisor_sha256,
        ]
        .iter()
        .any(|v| v.len() != 32)
        || ![0, 32].contains(&id.recovery_history_head.len())
        || (record.phase != OwnerPhase::Pending as i32
            && (id.child_pid == 0 || id.child_creation_filetime == 0))
    {
        return Err("Supervisor owner identity invalid".into());
    }
    let f = facts(record)?;
    if ObservedTruth::try_from(f.normal_shutdown).is_err() {
        return Err("Supervisor shutdown observation invalid".into());
    }
    if record.reader_joins.as_ref().is_some_and(|j| j.version != 1) {
        return Err("Supervisor reader-join version invalid".into());
    }
    if record.phase == OwnerPhase::Released as i32 && !normal_release(record) {
        return Err("Supervisor normal release lacks original resource proof".into());
    }
    if record.phase == OwnerPhase::Recovered as i32 {
        let binding = record
            .recovery
            .as_ref()
            .ok_or("Recovered owner binding missing")?;
        if !complete_abnormal_proof(record)
            || binding.version != 1
            || [
                &binding.original_digest,
                &binding.receipt_digest,
                &binding.history_head,
            ]
            .iter()
            .any(|v| v.len() != 32)
        {
            return Err("Recovered owner lacks original abnormal proof".into());
        }
    } else if record.recovery.is_some() {
        return Err("Recovery binding on an unrecovered owner".into());
    }
    Ok(())
}

pub fn validate_transition(previous: &OwnerRecord, next: &OwnerRecord) -> Result<()> {
    validate(previous)?;
    validate(next)?;
    let p = OwnerPhase::try_from(previous.phase)?;
    let n = OwnerPhase::try_from(next.phase)?;
    if !matches!(
        (p, n),
        (OwnerPhase::Pending, OwnerPhase::Active)
            | (OwnerPhase::Active, OwnerPhase::Closing)
            | (OwnerPhase::Closing, OwnerPhase::Closing)
            | (OwnerPhase::Closing, OwnerPhase::Released)
    ) {
        return Err("Supervisor owner phase transition refused".into());
    }
    let prior = identity(previous)?;
    let mut expected = prior.clone();
    if p == OwnerPhase::Pending {
        expected.child_pid = identity(next)?.child_pid;
        expected.child_creation_filetime = identity(next)?.child_creation_filetime;
    }
    if previous.model_version != next.model_version || identity(next)? != &expected {
        return Err("Supervisor immutable owner binding changed".into());
    }
    Ok(())
}

pub fn encode_owner(record: &OwnerRecord) -> Result<Vec<u8>> {
    validate(record)?;
    pack(OWNER_MAGIC, &record.encode_to_vec())
}

pub fn decode_owner(container: &[u8]) -> Result<OwnerRecord> {
    let record = decode_canonical(&unpack(OWNER_MAGIC, container)?)?;
    validate(&record)?;
    Ok(record)
}

pub fn encode_receipt(receipt: &proto::RecoveryReceipt) -> Result<Vec<u8>> {
    validate_receipt(receipt)?;
    pack(RECEIPT_MAGIC, &receipt.encode_to_vec())
}

pub fn decode_receipt(container: &[u8]) -> Result<proto::RecoveryReceipt> {
    let receipt = decode_canonical(&unpack(RECEIPT_MAGIC, container)?)?;
    validate_receipt(&receipt)?;
    Ok(receipt)
}

fn validate_receipt(r: &proto::RecoveryReceipt) -> Result<()> {
    if r.version != 1
        || !r.acknowledged_unknown
        || !r.original_supervisor_ended
        || r.generation == 0
        || r.manager_pid == 0
        || r.manager_creation_filetime == 0
        || [
            &r.original_digest,
            &r.profile,
            &r.incarnation,
            &r.manager_sha256,
        ]
        .iter()
        .any(|v| v.len() != 32)
    {
        return Err("Supervisor recovery receipt invalid".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "supervision_record_codec_tests.rs"]
mod tests;
