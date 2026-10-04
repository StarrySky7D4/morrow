//! Canonical metadata-only card invalidations. A digest is identity, not authority.
use crate::{Error, Result, identity};
use sha2::{Digest, Sha256};
pub const FEATURE: &str = "changes-metadata-v1";
pub const VERSION: u16 = 1;
pub const HEADER_BYTES: usize = 150;
pub const MAX_PAYLOAD_BYTES: usize = 662;
pub const MAX_CARDS: usize = 32;
pub const WIRE_SPEC: &[u8] = include_bytes!("../schemas/changes_metadata_v1.wire");
const MAGIC: &[u8; 8] = b"MRCHG001";
const SCOPE_DOMAIN: &[u8] = b"Morrow/changes-metadata/scope/v1\0";
const CURSOR_DOMAIN: &[u8] = b"Morrow/changes-metadata/cursor/v1\0";
pub fn profile_digest() -> [u8; 32] {
    Sha256::digest(WIRE_SPEC).into()
}

/// Public approved facts only. The Card hash covers complete Card.encode().
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub card_id: String,
    pub operation_id: String,
    pub revision: u64,
    pub card_sha256: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub scope_digest: [u8; 32],
    pub window_id: [u8; 32],
    pub card_id: String,
    pub operation_id: String,
    pub revision: u64,
    pub card_sha256: [u8; 32],
}
impl Change {
    pub fn notification(self, scope_digest: [u8; 32], window_id: [u8; 32]) -> Result<Metadata> {
        let value = Metadata {
            scope_digest,
            window_id,
            card_id: self.card_id,
            operation_id: self.operation_id,
            revision: self.revision,
            card_sha256: self.card_sha256,
        };
        value.validate()?;
        Ok(value)
    }
}
impl Metadata {
    pub fn validate(&self) -> Result<()> {
        identity(&self.card_id)?;
        identity(&self.operation_id)?;
        if self.scope_digest == [0; 32] || self.window_id == [0; 32] || self.revision == 0 {
            return Err(Error::Invalid("changes metadata identity"));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut b = Vec::with_capacity(HEADER_BYTES + self.card_id.len() + self.operation_id.len());
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&VERSION.to_le_bytes());
        b.extend_from_slice(&profile_digest());
        b.extend_from_slice(&self.scope_digest);
        b.extend_from_slice(&self.window_id);
        b.extend_from_slice(&self.revision.to_le_bytes());
        b.extend_from_slice(&self.card_sha256);
        b.extend_from_slice(&(self.card_id.len() as u16).to_le_bytes());
        b.extend_from_slice(&(self.operation_id.len() as u16).to_le_bytes());
        b.extend_from_slice(self.card_id.as_bytes());
        b.extend_from_slice(self.operation_id.as_bytes());
        Ok(b)
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        if b.len() < HEADER_BYTES || &b[..8] != MAGIC {
            return Err(Error::Invalid("changes metadata header"));
        }
        if u16::from_le_bytes(b[8..10].try_into().unwrap()) != VERSION
            || b[10..42] != profile_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        let c = u16::from_le_bytes(b[146..148].try_into().unwrap()) as usize;
        let o = u16::from_le_bytes(b[148..150].try_into().unwrap()) as usize;
        if !(1..=256).contains(&c) || !(1..=256).contains(&o) || b.len() != HEADER_BYTES + c + o {
            return Err(Error::Invalid("changes metadata length"));
        }
        let value = Self {
            scope_digest: b[42..74].try_into().unwrap(),
            window_id: b[74..106].try_into().unwrap(),
            revision: u64::from_le_bytes(b[106..114].try_into().unwrap()),
            card_sha256: b[114..146].try_into().unwrap(),
            card_id: std::str::from_utf8(&b[150..150 + c])
                .map_err(|_| Error::Invalid("changes metadata UTF-8"))?
                .to_owned(),
            operation_id: std::str::from_utf8(&b[150 + c..])
                .map_err(|_| Error::Invalid("changes metadata UTF-8"))?
                .to_owned(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn cursor(&self) -> Result<[u8; 32]> {
        let mut h = Sha256::new();
        h.update(CURSOR_DOMAIN);
        h.update(self.encode()?);
        Ok(h.finalize().into())
    }
}
pub(crate) fn canonical_cards(cards: &[String]) -> Result<Vec<String>> {
    if cards.is_empty() || cards.len() > MAX_CARDS {
        return Err(Error::Limit);
    }
    for c in cards {
        identity(c)?;
    }
    let mut cards = cards.to_vec();
    cards.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    if cards.windows(2).any(|w| w[0] == w[1]) {
        return Err(Error::Invalid("duplicate changes card"));
    }
    Ok(cards)
}
pub fn scope_digest(store_id: [u8; 32], package: [u8; 32], cards: &[String]) -> Result<[u8; 32]> {
    if store_id == [0; 32] || package == [0; 32] {
        return Err(Error::Invalid("changes scope identity"));
    }
    let cards = canonical_cards(cards)?;
    let mut h = Sha256::new();
    h.update(SCOPE_DOMAIN);
    h.update(profile_digest());
    h.update(store_id);
    h.update(package);
    h.update((cards.len() as u32).to_le_bytes());
    for c in cards {
        h.update((c.len() as u16).to_le_bytes());
        h.update(c.as_bytes());
    }
    Ok(h.finalize().into())
}
