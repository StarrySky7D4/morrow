//! Persistent local capture control. A nonce is an owner/CAS fact, never a plugin
//! grant. Restarted hosts must interrupt stale preparations rather than bind a new
//! source. Only Ready is linked to an immutable read observation; states are not a
//! signed history of transitions and Ready does not mean delivered.
use crate::{Error, Result, identity, read_archive::Plan};
use prost::Message;
use sha2::{Digest, Sha256};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.read_capture.v1.rs"));
}
pub use proto::Phase;
pub const MAGIC: &[u8; 8] = b"MRWCAP01";
pub const MAX_CONTEXT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_RAW_BYTES: usize = 8 * 1024 * 1024 + 4096;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
pub const MAX_STATES: u32 = 4096;
pub const MAX_STATE_BYTES: u64 = 256 * 1024 * 1024;
pub const TERMINAL_RESERVE: u64 = 4096;
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub max_states: u32,
    pub max_bytes: u64,
    pub preparation: crate::read_archive::PreparationBudget,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_states: MAX_STATES,
            max_bytes: MAX_STATE_BYTES,
            preparation: Default::default(),
        }
    }
}
impl Budget {
    pub(crate) fn validate(self) -> Result<()> {
        if self.max_states == 0
            || self.max_states > MAX_STATES
            || self.max_bytes == 0
            || self.max_bytes > MAX_STATE_BYTES
        {
            return Err(Error::Limit);
        }
        self.preparation.validate()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub owner: [u8; 32],
    pub revision: u64,
}
#[derive(Clone, Debug)]
pub struct State {
    value: proto::State,
    plan: Plan,
    raw: Vec<u8>,
    container: Vec<u8>,
    digest: [u8; 32],
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub count: u32,
    pub charged_bytes: u64,
}
impl State {
    pub fn data(&self) -> &proto::State {
        &self.value
    }
    pub fn plan(&self) -> &Plan {
        &self.plan
    }
    pub fn phase(&self) -> Phase {
        Phase::try_from(self.value.phase).expect("validated phase")
    }
    pub fn revision(&self) -> u64 {
        self.value.revision
    }
    pub fn owner(&self) -> [u8; 32] {
        self.value
            .owner
            .as_slice()
            .try_into()
            .expect("validated owner")
    }
    pub fn token(&self) -> Token {
        Token {
            owner: self.owner(),
            revision: self.revision(),
        }
    }
    pub fn context_type(&self) -> &str {
        &self.value.context_type
    }
    pub fn context(&self) -> &[u8] {
        &self.value.context
    }
    pub fn charged_bytes(&self) -> u64 {
        self.value.charged_bytes
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
    pub fn archive_root(&self) -> Option<[u8; 32]> {
        if self.phase() == Phase::Ready {
            Some(
                self.value
                    .archive_root
                    .as_slice()
                    .try_into()
                    .expect("validated root"),
            )
        } else {
            None
        }
    }
    pub fn reason(&self) -> &str {
        &self.value.reason
    }
    pub fn binding(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"Morrow/read-capture/v1\0");
        hash.update((self.value.plan.len() as u64).to_le_bytes());
        hash.update(&self.value.plan);
        hash.update((self.value.context_type.len() as u32).to_le_bytes());
        hash.update(self.value.context_type.as_bytes());
        hash.update((self.value.context.len() as u64).to_le_bytes());
        hash.update(&self.value.context);
        hash.update(&self.value.owner);
        hash.update(self.value.charged_bytes.to_le_bytes());
        hash.finalize().into()
    }
    pub(crate) fn new(
        plan: &Plan,
        context_type: &str,
        context: &[u8],
        owner: [u8; 32],
    ) -> Result<Self> {
        identity(context_type)?;
        if context.len() > MAX_CONTEXT_BYTES {
            return Err(Error::Limit);
        }
        if owner == [0; 32] {
            return Err(Error::Invalid("capture owner"));
        }
        let mut value = proto::State {
            schema_version: 1,
            plan: plan.raw()?,
            context_type: context_type.into(),
            context: context.to_vec(),
            charged_bytes: 1,
            owner: owner.to_vec(),
            revision: 1,
            phase: Phase::Preparing as i32,
            archive_root: vec![],
            reason: String::new(),
        };
        // Monotone sizing includes the persisted charge field itself. Any tiny encoding
        // rounding remains held with the fixed 4 KiB transition allowance.
        for _ in 0..32 {
            let raw = value.encode_to_vec();
            let container = crate::envelope::pack(MAGIC, &raw, MAX_RAW_BYTES)?;
            let required = (raw.len() + container.len()) as u64 + TERMINAL_RESERVE;
            if value.charged_bytes >= required {
                return Self::from_value(value);
            }
            value.charged_bytes = required;
        }
        Err(Error::Limit)
    }
    pub(crate) fn advance(
        &self,
        phase: Phase,
        root: Option<[u8; 32]>,
        reason: &str,
    ) -> Result<Self> {
        if self.phase() != Phase::Preparing {
            return Err(Error::OperationConflict);
        }
        if reason.len() > 256 {
            return Err(Error::Limit);
        }
        let mut value = self.value.clone();
        value.revision = value.revision.checked_add(1).ok_or(Error::Limit)?;
        value.phase = phase as i32;
        value.archive_root = root.map(|v| v.to_vec()).unwrap_or_default();
        value.reason = reason.into();
        Self::from_value(value)
    }
    pub(crate) fn check(&self, token: &Token) -> Result<()> {
        if self.owner() != token.owner {
            return Err(Error::OperationConflict);
        }
        if self.revision() != token.revision {
            return Err(Error::RevisionConflict);
        }
        Ok(())
    }
    fn validate(value: &proto::State) -> Result<Plan> {
        if value.schema_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        let plan = Plan::decode(&value.plan)?;
        identity(&value.context_type)?;
        if value.context.len() > MAX_CONTEXT_BYTES || value.reason.len() > 256 {
            return Err(Error::Limit);
        }
        if value.owner.len() != 32 || value.owner.iter().all(|v| *v == 0) || value.revision == 0 {
            return Err(Error::Integrity);
        }
        let phase = Phase::try_from(value.phase).map_err(|_| Error::UnsupportedVersion)?;
        if phase == Phase::Unspecified
            || (phase == Phase::Ready && value.archive_root.len() != 32)
            || (phase != Phase::Ready && !value.archive_root.is_empty())
            || (matches!(phase, Phase::Ready | Phase::Preparing) && !value.reason.is_empty())
        {
            return Err(Error::Integrity);
        }
        if value.charged_bytes == 0
            || value.charged_bytes
                > (MAX_RAW_BYTES + MAX_CONTAINER_BYTES) as u64 + TERMINAL_RESERVE + 128
        {
            return Err(Error::Limit);
        }
        Ok(plan)
    }
    fn from_value(value: proto::State) -> Result<Self> {
        let plan = Self::validate(&value)?;
        let raw = value.encode_to_vec();
        let container = crate::envelope::pack(MAGIC, &raw, MAX_RAW_BYTES)?;
        if (raw.len() + container.len()) as u64 > value.charged_bytes {
            return Err(Error::Limit);
        }
        Ok(Self {
            value,
            plan,
            digest: Sha256::digest(&raw).into(),
            raw,
            container,
        })
    }
    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = crate::envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        preflight(&raw)?;
        let value = proto::State::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
        let plan = Self::validate(&value)?;
        if (raw.len() + container.len()) as u64 > value.charged_bytes {
            return Err(Error::Limit);
        }
        Ok(Self {
            value,
            plan,
            digest: Sha256::digest(&raw).into(),
            raw,
            container: container.into(),
        })
    }
}
fn preflight(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut seen = [false; 11];
    while !raw.is_empty() {
        let (n, w) = decode_key(&mut raw).map_err(|_| Error::Integrity)?;
        if n > 10 {
            return Err(Error::UnsupportedVersion);
        }
        if seen[n as usize] {
            return Err(Error::Invalid("duplicate capture field"));
        }
        seen[n as usize] = true;
        let expected = match n {
            1 | 8 => WireType::Varint,
            5 | 7 => WireType::SixtyFourBit,
            _ => WireType::LengthDelimited,
        };
        if w != expected {
            return Err(Error::Integrity);
        }
        match w {
            WireType::Varint => {
                let v = decode_varint(&mut raw).map_err(|_| Error::Integrity)?;
                if v > u64::from(u32::MAX) {
                    return Err(Error::Limit);
                }
            }
            WireType::SixtyFourBit => {
                if raw.len() < 8 {
                    return Err(Error::Integrity);
                }
                raw = &raw[8..];
            }
            WireType::LengthDelimited => {
                let nbytes = decode_varint(&mut raw).map_err(|_| Error::Integrity)?;
                let max = match n {
                    2 => MAX_CONTEXT_BYTES + 2048,
                    3 | 10 => 256,
                    4 => MAX_CONTEXT_BYTES,
                    6 | 9 => 32,
                    _ => 0,
                };
                if nbytes > max as u64 {
                    return Err(Error::Limit);
                }
                if nbytes > raw.len() as u64 {
                    return Err(Error::Integrity);
                }
                if n == 2 {
                    Plan::decode(&raw[..nbytes as usize])?;
                }
                raw = &raw[nbytes as usize..];
            }
            _ => return Err(Error::Integrity),
        }
    }
    Ok(())
}
