//! Historical audit receipt for retained file bytes. A receipt does not grant
//! file access, prove an external write, or turn imported bytes into a past commit.
use crate::{Error, Result, envelope, file_content, identity};
use prost::Message;
use sha2::{Digest, Sha256};

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.file_content_receipt.v1.rs"
    ));
}

pub const MAGIC: &[u8; 8] = b"MROWFCR1";
pub const VERSION: u32 = 1;
pub const MAX_RAW_BYTES: usize = 2048;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    /// Bytes retained from an explicit live staging operation.
    LiveStaging,
    /// Bytes observed during migration; no past submission or commit is inferred.
    LegacyImport,
}

impl Source {
    fn number(self) -> i32 {
        match self {
            Self::LiveStaging => 1,
            Self::LegacyImport => 2,
        }
    }

    fn from_number(value: i32) -> Result<Self> {
        match value {
            1 => Ok(Self::LiveStaging),
            2 => Ok(Self::LegacyImport),
            _ => Err(Error::UnsupportedVersion),
        }
    }
}

pub struct Receipt {
    value: proto::Receipt,
    request_sha256: [u8; 32],
    content_sha256: [u8; 32],
    content_container_sha256: [u8; 32],
    source: Source,
    event_id: String,
    raw: Vec<u8>,
    container: Vec<u8>,
}

/// Descriptive alias for the audit receipt carried by this module.
pub type FileContentReceipt = Receipt;

impl Receipt {
    pub fn new(content: &file_content::FileContent, source: Source) -> Result<Self> {
        let value = proto::Receipt {
            schema_version: VERSION,
            operation_id: content.operation_id().to_owned(),
            subject: content.subject().to_owned(),
            request_sha256: content.request_sha256().to_vec(),
            content_sha256: content.content_sha256().to_vec(),
            content_container_sha256: Sha256::digest(content.container()).to_vec(),
            content_length: content.content().len() as u64,
            source: source.number(),
        };
        let raw = value.encode_to_vec();
        let container = envelope::pack(MAGIC, &raw, MAX_RAW_BYTES)?;
        Self::from_parts(value, raw, container)
    }

    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        preflight(&raw)?;
        let value = proto::Receipt::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("file content receipt protobuf"))?;
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical file content receipt"));
        }
        Self::from_parts(value, raw, container.to_vec())
    }

    fn from_parts(value: proto::Receipt, raw: Vec<u8>, container: Vec<u8>) -> Result<Self> {
        if value.schema_version != VERSION {
            return Err(Error::UnsupportedVersion);
        }
        identity(&value.operation_id)?;
        identity(&value.subject)?;
        if value.content_length > file_content::MAX_CONTENT_BYTES as u64 {
            return Err(Error::Limit);
        }
        let request_sha256 = fixed_digest(&value.request_sha256)?;
        let content_sha256 = fixed_digest(&value.content_sha256)?;
        if value.content_length == 0 {
            let empty_sha256: [u8; 32] = Sha256::digest([]).into();
            if content_sha256 != empty_sha256 {
                return Err(Error::Integrity);
            }
        }
        let content_container_sha256 = fixed_digest(&value.content_container_sha256)?;
        let source = Source::from_number(value.source)?;
        let event_id = event_id(&value.operation_id, request_sha256);
        Ok(Self {
            value,
            request_sha256,
            content_sha256,
            content_container_sha256,
            source,
            event_id,
            raw,
            container,
        })
    }

    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn operation_id(&self) -> &str {
        &self.value.operation_id
    }
    pub fn subject(&self) -> &str {
        &self.value.subject
    }
    pub fn request_sha256(&self) -> [u8; 32] {
        self.request_sha256
    }
    pub fn content_sha256(&self) -> [u8; 32] {
        self.content_sha256
    }
    pub fn content_container_sha256(&self) -> [u8; 32] {
        self.content_container_sha256
    }
    pub fn content_length(&self) -> u64 {
        self.value.content_length
    }
    pub fn source(&self) -> Source {
        self.source
    }
    pub fn event_id(&self) -> String {
        self.event_id.clone()
    }
}

fn fixed_digest(bytes: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Invalid("file content receipt digest length"))?;
    if value == [0; 32] {
        return Err(Error::Invalid("empty file content receipt digest"));
    }
    Ok(value)
}

fn event_id(operation_id: &str, request_sha256: [u8; 32]) -> String {
    use std::fmt::Write;
    let mut hash = Sha256::new();
    hash.update(b"morrow.file-content-receipt.event.v1\0");
    hash.update((operation_id.len() as u64).to_le_bytes());
    hash.update(operation_id.as_bytes());
    hash.update(request_sha256);
    let digest = hash.finalize();
    let mut result = String::with_capacity("file-content-".len() + 64);
    result.push_str("file-content-");
    for byte in digest {
        write!(&mut result, "{byte:02x}").expect("String write");
    }
    result
}

fn preflight(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut seen = [false; 9];
    while !raw.is_empty() {
        let (tag, wire) =
            decode_key(&mut raw).map_err(|_| Error::Invalid("file content receipt field"))?;
        if !(1..=8).contains(&tag) {
            return Err(Error::UnsupportedVersion);
        }
        if std::mem::replace(&mut seen[tag as usize], true) {
            return Err(Error::Invalid("duplicate file content receipt field"));
        }
        let scalar = matches!(tag, 1 | 7 | 8);
        if wire
            != if scalar {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err(Error::Invalid("file content receipt wire type"));
        }
        let value =
            decode_varint(&mut raw).map_err(|_| Error::Invalid("file content receipt value"))?;
        if scalar {
            if tag != 7 && value > u32::MAX as u64 {
                return Err(Error::Invalid("file content receipt scalar overflow"));
            }
        } else {
            let limit = if matches!(tag, 2 | 3) { 256 } else { 32 };
            if value > limit {
                return Err(Error::Limit);
            }
            if value > raw.len() as u64 {
                return Err(Error::Invalid("file content receipt span"));
            }
            raw = &raw[value as usize..];
        }
    }
    Ok(())
}
