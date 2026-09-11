//! Portable attachment metadata; payload bytes are never transcoded or compressed here.
use crate::{Error, Result, envelope, identity};
use prost::Message;
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.attachment.v1.rs"));
}
pub use proto::RetentionKind;
pub const MAX_BLOB_BYTES: u64 = 200 * 1024 * 1024;
pub const MIN_RETIREMENT_MS: i64 = 60_000;
const MAX_METADATA: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobInfo {
    pub id: String,
    pub sha256: [u8; 32],
    pub byte_length: u64,
    pub created_at_unix_ms: i64,
    pub retired_at_unix_ms: Option<i64>,
}
impl BlobInfo {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let value = proto::Blob {
            schema_version: 1,
            id: self.id.clone(),
            sha256: self.sha256.to_vec(),
            byte_length: self.byte_length,
            created_at_unix_ms: self.created_at_unix_ms,
            retired_at_unix_ms: self.retired_at_unix_ms,
        };
        let bytes = envelope::pack(b"MORROWB1", &value.encode_to_vec(), MAX_METADATA)?;
        Self::decode(&bytes)?;
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(b"MORROWB1", bytes, MAX_METADATA)?;
        let value = proto::Blob::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
        if value.schema_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        identity(&value.id)?;
        if value.sha256.len() != 32
            || value.byte_length > MAX_BLOB_BYTES
            || value.created_at_unix_ms < 0
            || value
                .retired_at_unix_ms
                .is_some_and(|v| v < value.created_at_unix_ms)
        {
            return Err(Error::Integrity);
        }
        Ok(Self {
            id: value.id,
            sha256: value.sha256.as_slice().try_into().unwrap(),
            byte_length: value.byte_length,
            created_at_unix_ms: value.created_at_unix_ms,
            retired_at_unix_ms: value.retired_at_unix_ms,
        })
    }
}
pub fn encode_retention(owner: &str, blob: &str, kind: RetentionKind) -> Result<Vec<u8>> {
    identity(owner)?;
    identity(blob)?;
    if kind == RetentionKind::Unspecified {
        return Err(Error::Invalid("retention kind"));
    }
    envelope::pack(
        b"MORROWP1",
        &proto::Retention {
            schema_version: 1,
            owner_id: owner.into(),
            blob_id: blob.into(),
            kind: kind as i32,
        }
        .encode_to_vec(),
        MAX_METADATA,
    )
}
pub fn decode_retention(bytes: &[u8]) -> Result<proto::Retention> {
    let raw = envelope::unpack(b"MORROWP1", bytes, MAX_METADATA)?;
    let value = proto::Retention::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
    if value.schema_version != 1 {
        return Err(Error::UnsupportedVersion);
    }
    identity(&value.owner_id)?;
    identity(&value.blob_id)?;
    if value.kind() == RetentionKind::Unspecified {
        return Err(Error::Invalid("retention kind"));
    }
    Ok(value)
}
pub fn encode_clock(now: i64) -> Result<Vec<u8>> {
    if now < 0 {
        return Err(Error::Invalid("clock"));
    }
    envelope::pack(
        b"MORROWH1",
        &proto::Clock {
            schema_version: 1,
            last_unix_ms: now,
        }
        .encode_to_vec(),
        MAX_METADATA,
    )
}
pub fn decode_clock(bytes: &[u8]) -> Result<i64> {
    let raw = envelope::unpack(b"MORROWH1", bytes, MAX_METADATA)?;
    let value = proto::Clock::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
    if value.schema_version != 1 {
        return Err(Error::UnsupportedVersion);
    }
    if value.last_unix_ms < 0 {
        return Err(Error::Integrity);
    }
    Ok(value.last_unix_ms)
}

// Mutate only the requested field; future metadata survives retirement and clock updates.
fn update_field(
    magic: &[u8; 8],
    bytes: &[u8],
    type_name: &str,
    field: &str,
    value: Option<prost_reflect::Value>,
) -> Result<Vec<u8>> {
    static POOL: std::sync::OnceLock<prost_reflect::DescriptorPool> = std::sync::OnceLock::new();
    let pool = POOL.get_or_init(|| {
        prost_reflect::DescriptorPool::decode(
            include_bytes!(concat!(env!("OUT_DIR"), "/attachment.descriptor.bin")).as_slice(),
        )
        .expect("compiled attachment descriptor")
    });
    let raw = envelope::unpack(magic, bytes, MAX_METADATA)?;
    let mut message = prost_reflect::DynamicMessage::decode(
        pool.get_message_by_name(type_name)
            .expect("compiled message"),
        raw.as_slice(),
    )
    .map_err(|_| Error::Integrity)?;
    if let Some(value) = value {
        message.set_field_by_name(field, value);
    } else {
        message.clear_field_by_name(field);
    }
    envelope::pack(magic, &message.encode_to_vec(), MAX_METADATA)
}
pub fn with_retirement(bytes: &[u8], retired: Option<i64>) -> Result<Vec<u8>> {
    BlobInfo::decode(bytes)?;
    let result = update_field(
        b"MORROWB1",
        bytes,
        "morrow.attachment.v1.Blob",
        "retired_at_unix_ms",
        retired.map(prost_reflect::Value::I64),
    )?;
    BlobInfo::decode(&result)?;
    Ok(result)
}
pub fn with_clock(bytes: &[u8], now: i64) -> Result<Vec<u8>> {
    if now < decode_clock(bytes)? {
        return Err(Error::Invalid("attachment clock regression"));
    }
    update_field(
        b"MORROWH1",
        bytes,
        "morrow.attachment.v1.Clock",
        "last_unix_ms",
        Some(prost_reflect::Value::I64(now)),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn future_blob_fields_survive_retirement_changes() {
        let value = BlobInfo {
            id: "blob".into(),
            sha256: [7; 32],
            byte_length: 3,
            created_at_unix_ms: 1,
            retired_at_unix_ms: None,
        };
        let mut raw =
            envelope::unpack(b"MORROWB1", &value.encode().unwrap(), MAX_METADATA).unwrap();
        raw.extend_from_slice(&[0xa0, 0x06, 0x7b]);
        let future = envelope::pack(b"MORROWB1", &raw, MAX_METADATA).unwrap();
        let edited = with_retirement(&future, Some(2)).unwrap();
        let restored = with_retirement(&edited, None).unwrap();
        let raw = envelope::unpack(b"MORROWB1", &restored, MAX_METADATA).unwrap();
        assert!(raw.windows(3).any(|v| v == [0xa0, 0x06, 0x7b]));
        assert_eq!(BlobInfo::decode(&restored).unwrap(), value);
    }
    #[test]
    fn metadata_corruption_and_future_versions_are_rejected() {
        let value = BlobInfo {
            id: "blob".into(),
            sha256: [7; 32],
            byte_length: 3,
            created_at_unix_ms: 1,
            retired_at_unix_ms: None,
        }
        .encode()
        .unwrap();
        for end in 0..value.len() {
            assert!(BlobInfo::decode(&value[..end]).is_err());
        }
        let raw = envelope::unpack(b"MORROWB1", &value, MAX_METADATA).unwrap();
        let mut future = proto::Blob::decode(raw.as_slice()).unwrap();
        future.schema_version = 2;
        let unsupported =
            envelope::pack(b"MORROWB1", &future.encode_to_vec(), MAX_METADATA).unwrap();
        assert!(matches!(
            BlobInfo::decode(&unsupported),
            Err(Error::UnsupportedVersion)
        ));
        let mut changed = value.clone();
        changed[18] ^= 1;
        assert!(BlobInfo::decode(&changed).is_err());
        assert!(with_retirement(&value, Some(0)).is_err());
        assert!(encode_retention("owner", "blob", RetentionKind::Unspecified).is_err());
        let clock = encode_clock(4).unwrap();
        assert!(with_clock(&clock, 3).is_err());
    }
}
