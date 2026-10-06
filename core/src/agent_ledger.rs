//! Bounded host-local agent history in the original Store. Decoding and
//! persistence confer no live execution, transport or content authority.
use crate::{Error, Result, envelope, identity};
use prost::Message;

pub const MAX_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_RAW_BYTES: usize = MAX_PAYLOAD_BYTES + 512;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
const MAGIC: &[u8; 8] = b"MROWAGL1";

#[derive(Clone, PartialEq, Eq, Message)]
struct Value {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(bytes = "vec", tag = "2")]
    domain: Vec<u8>,
    #[prost(string, tag = "3")]
    id: String,
    #[prost(uint64, tag = "4")]
    revision: u64,
    #[prost(bytes = "vec", tag = "5")]
    payload: Vec<u8>,
}

/// Immutable Protobuf/LZ4 container with a corruption checksum. Opaque payload
/// bytes remain host-owned data and are deliberately absent from Debug output.
#[derive(Clone, PartialEq, Eq)]
pub struct Record {
    value: Value,
    container: Vec<u8>,
}
impl std::fmt::Debug for Record {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AgentLedgerRecord")
            .field("domain", &self.domain())
            .field("id", &self.id())
            .field("revision", &self.revision())
            .field("payload_length", &self.payload().len())
            .finish_non_exhaustive()
    }
}
impl Record {
    pub fn new(
        domain: [u8; 32],
        id: impl Into<String>,
        revision: u64,
        payload: Vec<u8>,
    ) -> Result<Self> {
        let value = Value {
            version: 1,
            domain: domain.to_vec(),
            id: id.into(),
            revision,
            payload,
        };
        validate(&value)?;
        let container = envelope::pack(MAGIC, &value.encode_to_vec(), MAX_RAW_BYTES)?;
        Ok(Self { value, container })
    }
    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        preflight(&raw)?;
        let value = Value::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
        validate(&value)?;
        if value.encode_to_vec() != raw {
            return Err(Error::Integrity);
        }
        Ok(Self {
            value,
            container: container.to_vec(),
        })
    }
    pub fn domain(&self) -> [u8; 32] {
        self.value
            .domain
            .as_slice()
            .try_into()
            .expect("validated ledger domain")
    }
    pub fn id(&self) -> &str {
        &self.value.id
    }
    pub fn revision(&self) -> u64 {
        self.value.revision
    }
    pub fn payload(&self) -> &[u8] {
        &self.value.payload
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    /// Shared quota reserves worst-case metadata and LZ4 container overhead.
    /// Equal new payload lengths cost the same across revisions/compression,
    /// so fixed-size terminal-state space stays reserved. A decoded alternative
    /// container representation is never charged below its actual byte length.
    pub fn retained_bytes(&self) -> u64 {
        let raw_bound = self.payload().len() + 512;
        (raw_bound + raw_bound / 255 + 128).max(self.container.len()) as u64
    }
}
fn validate(value: &Value) -> Result<()> {
    if value.version != 1 {
        return Err(Error::UnsupportedVersion);
    }
    if value.domain.len() != 32 || value.domain.as_slice() == [0; 32] {
        return Err(Error::Invalid("agent ledger domain"));
    }
    identity(&value.id)?;
    if value.revision == 0 || value.revision > i64::MAX as u64 {
        return Err(Error::Invalid("agent ledger revision"));
    }
    if value.payload.len() > MAX_PAYLOAD_BYTES {
        return Err(Error::Limit);
    }
    Ok(())
}
// The generated decoder only sees strictly ordered bounded fields. Unknown or
// repeated fields cannot become silent payload/identity extensions.
fn preflight(mut bytes: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut previous = 0;
    while !bytes.is_empty() {
        let (tag, wire) = decode_key(&mut bytes).map_err(|_| Error::Integrity)?;
        if tag <= previous || tag > 5 {
            return Err(Error::Integrity);
        }
        previous = tag;
        if matches!(tag, 1 | 4) {
            if wire != WireType::Varint {
                return Err(Error::Integrity);
            }
            decode_varint(&mut bytes).map_err(|_| Error::Integrity)?;
        } else {
            if wire != WireType::LengthDelimited {
                return Err(Error::Integrity);
            }
            let length = decode_varint(&mut bytes).map_err(|_| Error::Integrity)?;
            let limit = match tag {
                2 => 32,
                3 => 256,
                5 => MAX_PAYLOAD_BYTES,
                _ => unreachable!(),
            };
            if length > limit as u64 || length > bytes.len() as u64 {
                return Err(Error::Limit);
            }
            bytes = &bytes[length as usize..];
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_envelope_preserves_exact_payload_and_bounds_metadata() {
        for payload in [Vec::new(), vec![0, 255, 17], vec![7; MAX_PAYLOAD_BYTES]] {
            let record = Record::new([1; 32], "session", 1, payload.clone()).unwrap();
            let reopened = Record::decode(record.container()).unwrap();
            assert_eq!(reopened, record);
            assert_eq!(reopened.payload(), payload);
            assert!(record.container().starts_with(MAGIC));
        }
        for (domain, id, revision) in [
            ([0; 32], "session", 1),
            ([1; 32], "../id", 1),
            ([1; 32], "session", 0),
            ([1; 32], "session", i64::MAX as u64 + 1),
        ] {
            assert!(Record::new(domain, id, revision, vec![]).is_err());
        }
        assert_eq!(
            Record::new([1; 32], "session", 1, vec![0; MAX_PAYLOAD_BYTES + 1]),
            Err(Error::Limit)
        );
    }
    #[test]
    fn quota_reserves_fixed_size_across_revision_and_compression_changes() {
        let mut state = 0x7819ea45u32;
        let noise: Vec<u8> = (0..MAX_PAYLOAD_BYTES)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        let zero = Record::new([1; 32], "ledger", 1, vec![0; MAX_PAYLOAD_BYTES]).unwrap();
        let terminal = Record::new([1; 32], "ledger", i64::MAX as u64, noise).unwrap();
        assert_eq!(zero.retained_bytes(), terminal.retained_bytes());
        assert!(zero.retained_bytes() >= zero.container().len() as u64);
        assert!(terminal.retained_bytes() >= terminal.container().len() as u64);
        assert!(terminal.container().len() > zero.container().len());
    }
    #[test]
    fn damage_unknown_duplicate_overlong_and_oversized_fields_fail_closed() {
        let record = Record::new([1; 32], "session", 1, b"private".to_vec()).unwrap();
        for index in [0, 8, 10, 14, 18, record.container().len() - 1] {
            let mut damaged = record.container().to_vec();
            damaged[index] ^= 1;
            assert!(Record::decode(&damaged).is_err());
        }
        for tail in [vec![8, 1], vec![48, 1], vec![42, 0], vec![0]] {
            let mut raw = record.value.encode_to_vec();
            raw.extend_from_slice(&tail);
            assert!(Record::decode(&envelope::pack(MAGIC, &raw, MAX_RAW_BYTES).unwrap()).is_err());
        }
        let mut raw = record.value.encode_to_vec();
        raw.splice(1..2, [0x81, 0]);
        assert!(Record::decode(&envelope::pack(MAGIC, &raw, MAX_RAW_BYTES).unwrap()).is_err());
        assert_eq!(
            Record::decode(&vec![0; MAX_CONTAINER_BYTES + 1]),
            Err(Error::Limit)
        );
        assert!(!format!("{record:?}").contains("private"));
    }
}
