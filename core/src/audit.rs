//! Host-authored commit evidence. No key generation, queue removal,
//! witness claims or recovered authority are implied by cryptographic verification.
use ed25519_dalek::{Signature, Signer};
pub use ed25519_dalek::{SigningKey, VerifyingKey};
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.audit.v1.rs"));
}
pub const MAX_SEGMENT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_FRAME_BYTES: usize = MAX_SEGMENT_BYTES + 1024;
pub const MAX_CONTAINER_BYTES: usize = MAX_FRAME_BYTES + MAX_FRAME_BYTES / 255 + 128;
const MAGIC: &[u8; 8] = b"MORROWA1";
const DOMAIN: &[u8] = b"Morrow/audit/commit-segment/v1\0";
pub type Result<T> = std::result::Result<T, AuditError>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditError {
    Limit,
    Container,
    Signature,
    Contract,
    Event,
    Chain,
    Checkpoint,
}
impl std::fmt::Display for AuditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "audit {self:?}")
    }
}
impl std::error::Error for AuditError {}
/// Pin this identity through host policy or an independent trusted source.
/// A public key embedded in a segment never establishes its own trust.
#[derive(Clone)]
pub struct TrustedLog {
    pub id: String,
    pub key: VerifyingKey,
}
pub struct VerifiedSegment {
    raw: Vec<u8>,
    segment: proto::Segment,
    digest: [u8; 32],
    container: Vec<u8>,
}
impl VerifiedSegment {
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn segment(&self) -> &proto::Segment {
        &self.segment
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
}
fn signing_bytes(raw: &[u8]) -> Vec<u8> {
    [DOMAIN, &(raw.len() as u64).to_le_bytes(), raw].concat()
}
fn preflight(mut raw: &[u8], level: u8, budget: &mut usize) -> Result<()> {
    use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field};
    let mut seen = std::collections::HashMap::new();
    while !raw.is_empty() {
        *budget = budget.checked_sub(1).ok_or(AuditError::Limit)?;
        let (number, wire) = decode_key(&mut raw).map_err(|_| AuditError::Contract)?;
        let count = seen.entry(number).or_insert(0usize);
        *count += 1;
        if level == 1 && number == 6 {
            if *count > 128 {
                return Err(AuditError::Limit);
            }
        } else if number <= if level == 1 { 5 } else { 3 } && *count > 1 {
            return Err(AuditError::Contract);
        }
        if wire == WireType::LengthDelimited {
            let len = decode_varint(&mut raw).map_err(|_| AuditError::Contract)?;
            if len > raw.len() as u64 {
                return Err(AuditError::Contract);
            }
            let (field, rest) = raw.split_at(len as usize);
            raw = rest;
            if level == 1 && number == 6 {
                preflight(field, 2, budget)?;
            }
        } else {
            if matches!(wire, WireType::StartGroup | WireType::EndGroup) {
                return Err(AuditError::Contract);
            }
            skip_field(wire, number, &mut raw, DecodeContext::default())
                .map_err(|_| AuditError::Contract)?;
        }
    }
    Ok(())
}
fn operation_id(raw: &[u8]) -> Result<String> {
    if raw.starts_with(crate::io_intent::MAGIC) {
        // Each immutable revision has a distinct event identity. The command ID
        // inside the original remains stable across its transition history.
        crate::io_intent::Record::decode(raw).map(|record| record.event_id())
    } else if raw.starts_with(crate::read_journal::MAGIC) {
        crate::read_journal::decode(raw).map(|v| v.data().operation_id.clone())
    } else if raw.starts_with(b"MORROWR1") {
        crate::records::decode_commit(raw).map(|v| v.1.operation_id)
    } else {
        crate::transaction::decode_commit(raw).map(|v| v.1.operation_id)
    }
    .map_err(|_| AuditError::Event)
}
/// Convert the trusted core's immutable pending records; never regenerate commits.
pub fn from_pending(
    trusted: &TrustedLog,
    index: u64,
    previous: [u8; 32],
    events: &[(i64, Vec<u8>)],
) -> Result<proto::Segment> {
    if events.is_empty() || events.len() > 128 {
        return Err(AuditError::Limit);
    }
    let mut total = 0usize;
    let mut entries = Vec::with_capacity(events.len());
    for (sequence, raw) in events {
        total = total.checked_add(raw.len()).ok_or(AuditError::Limit)?;
        if *sequence <= 0 || total > MAX_SEGMENT_BYTES {
            return Err(AuditError::Limit);
        }
        entries.push(proto::Event {
            sequence: *sequence as u64,
            operation_id: operation_id(raw)?,
            original_commit: raw.clone(),
        });
    }
    let segment = proto::Segment {
        version: 1,
        log_id: trusted.id.clone(),
        signer_public_key: trusted.key.as_bytes().to_vec(),
        index,
        previous_sha256: previous.to_vec(),
        events: entries,
    };
    validate(&segment, trusted)?;
    if segment.encoded_len() > MAX_SEGMENT_BYTES {
        return Err(AuditError::Limit);
    }
    Ok(segment)
}
fn validate(segment: &proto::Segment, trusted: &TrustedLog) -> Result<()> {
    if segment.version != 1
        || segment.index == 0
        || segment.log_id != trusted.id
        || (segment.log_id.is_empty()
            || segment.log_id.len() > 256
            || segment
                .log_id
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':')))
        || segment.signer_public_key != trusted.key.as_bytes()
        || segment.previous_sha256.len() != 32
        || trusted.key.is_weak()
    {
        return Err(AuditError::Contract);
    }
    if segment.events.is_empty() || segment.events.len() > 128 {
        return Err(AuditError::Limit);
    }
    let mut ids = HashSet::new();
    let mut prior = 0u64;
    for event in &segment.events {
        if event.sequence == 0
            || (prior != 0 && prior.checked_add(1) != Some(event.sequence))
            || !ids.insert(&event.operation_id)
        {
            return Err(AuditError::Event);
        }
        let id = operation_id(&event.original_commit)?;
        if id != event.operation_id {
            return Err(AuditError::Event);
        }
        prior = event.sequence;
    }
    Ok(())
}
fn pack(raw: &[u8]) -> Result<Vec<u8>> {
    if raw.len() > MAX_FRAME_BYTES {
        return Err(AuditError::Limit);
    }
    let packed = lz4_flex::block::compress(raw);
    let mut result = Vec::with_capacity(18 + packed.len());
    result.extend_from_slice(MAGIC);
    result.extend_from_slice(&1u16.to_le_bytes());
    result.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    result.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    result.extend_from_slice(&packed);
    Ok(result)
}
fn unpack(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() > MAX_CONTAINER_BYTES {
        return Err(AuditError::Limit);
    }
    if bytes.len() < 18 || &bytes[..8] != MAGIC || &bytes[8..10] != 1u16.to_le_bytes().as_slice() {
        return Err(AuditError::Container);
    }
    let raw = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let packed = u32::from_le_bytes(bytes[14..18].try_into().unwrap()) as usize;
    if raw > MAX_FRAME_BYTES {
        return Err(AuditError::Limit);
    }
    if bytes.len() - 18 != packed {
        return Err(AuditError::Container);
    }
    let mut output = vec![0; raw];
    let count = lz4_flex::block::decompress_into(&bytes[18..], &mut output)
        .map_err(|_| AuditError::Container)?;
    if count != raw {
        return Err(AuditError::Container);
    }
    Ok(output)
}
/// Preserve the input encoding, including unknown Protobuf fields. Only a trusted
/// sealer should receive the signing key; never offer this as a guest hostcall.
pub fn sign_raw(raw: &[u8], trusted: &TrustedLog, key: &SigningKey) -> Result<Vec<u8>> {
    if raw.len() > MAX_SEGMENT_BYTES {
        return Err(AuditError::Limit);
    }
    if key.verifying_key() != trusted.key {
        return Err(AuditError::Signature);
    }
    preflight(raw, 1, &mut 4096)?;
    let segment = proto::Segment::decode(raw).map_err(|_| AuditError::Contract)?;
    validate(&segment, trusted)?;
    let signature = key.sign(&signing_bytes(raw));
    pack(
        &proto::SignedFrame {
            version: 1,
            raw_segment: raw.to_vec(),
            signature: signature.to_bytes().to_vec(),
        }
        .encode_to_vec(),
    )
}
pub fn sign(segment: &proto::Segment, trusted: &TrustedLog, key: &SigningKey) -> Result<Vec<u8>> {
    if segment.encoded_len() > MAX_SEGMENT_BYTES {
        return Err(AuditError::Limit);
    }
    sign_raw(&segment.encode_to_vec(), trusted, key)
}
pub fn verify(bytes: &[u8], trusted: &TrustedLog) -> Result<VerifiedSegment> {
    let raw_frame = unpack(bytes)?;
    preflight(&raw_frame, 0, &mut 4096)?;
    let frame =
        proto::SignedFrame::decode(raw_frame.as_slice()).map_err(|_| AuditError::Container)?;
    if frame.version != 1 || frame.raw_segment.len() > MAX_SEGMENT_BYTES {
        return Err(AuditError::Contract);
    }
    let signature = Signature::from_slice(&frame.signature).map_err(|_| AuditError::Signature)?;
    let signed = signing_bytes(&frame.raw_segment);
    trusted
        .key
        .verify_strict(&signed, &signature)
        .map_err(|_| AuditError::Signature)?;
    // Parse only AFTER verifying the exact original bytes; never re-encode them.
    preflight(&frame.raw_segment, 1, &mut 4096)?;
    let segment =
        proto::Segment::decode(frame.raw_segment.as_slice()).map_err(|_| AuditError::Contract)?;
    validate(&segment, trusted)?;
    let mut hash = Sha256::new();
    hash.update(&signed);
    hash.update(signature.to_bytes());
    Ok(VerifiedSegment {
        raw: frame.raw_segment,
        segment,
        digest: hash.finalize().into(),
        container: bytes.to_vec(),
    })
}
/// A checkpoint is useful against rollback only when retained independently of
/// the history being verified. It is not evidence of an independent witness.
#[derive(Clone)]
pub struct Checkpoint {
    log_id: String,
    key: Vec<u8>,
    index: u64,
    digest: [u8; 32],
}
impl Checkpoint {
    pub fn from_verified(segment: &VerifiedSegment) -> Self {
        Self {
            log_id: segment.segment.log_id.clone(),
            key: segment.segment.signer_public_key.clone(),
            index: segment.segment.index,
            digest: segment.digest,
        }
    }
}
pub struct ChainVerifier {
    trusted: TrustedLog,
    next_index: u64,
    next_sequence: u64,
    previous: [u8; 32],
    checkpoint: Option<Checkpoint>,
    checkpoint_seen: bool,
}
impl ChainVerifier {
    pub fn new(trusted: TrustedLog, checkpoint: Option<Checkpoint>) -> Result<Self> {
        if checkpoint
            .as_ref()
            .is_some_and(|c| c.log_id != trusted.id || c.key != trusted.key.as_bytes())
        {
            return Err(AuditError::Checkpoint);
        }
        let checkpoint_seen = checkpoint.is_none();
        Ok(Self {
            trusted,
            checkpoint,
            checkpoint_seen,
            next_index: 1,
            next_sequence: 1,
            previous: [0; 32],
        })
    }
    /// Failed acceptance leaves the verification state unchanged.
    pub fn accept(&mut self, bytes: &[u8]) -> Result<VerifiedSegment> {
        let verified = verify(bytes, &self.trusted)?;
        let segment = &verified.segment;
        if segment.index != self.next_index
            || segment.previous_sha256 != self.previous
            || segment.events[0].sequence != self.next_sequence
        {
            return Err(AuditError::Chain);
        }
        let seen = if let Some(c) = &self.checkpoint {
            if c.index == segment.index {
                if c.digest != verified.digest {
                    return Err(AuditError::Checkpoint);
                }
                true
            } else {
                self.checkpoint_seen
            }
        } else {
            true
        };
        let next_index = self.next_index.checked_add(1).ok_or(AuditError::Limit)?;
        let next_sequence = segment
            .events
            .last()
            .unwrap()
            .sequence
            .checked_add(1)
            .ok_or(AuditError::Limit)?;
        self.previous = verified.digest;
        self.next_index = next_index;
        self.next_sequence = next_sequence;
        self.checkpoint_seen = seen;
        Ok(verified)
    }
    pub fn finish(&self) -> Result<(u64, u64)> {
        if self.next_index == 1 || !self.checkpoint_seen {
            return Err(AuditError::Checkpoint);
        }
        Ok((self.next_index - 1, self.next_sequence - 1))
    }
}
