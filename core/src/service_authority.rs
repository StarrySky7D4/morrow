//! Explicit host approval records. Loading data neither restores live grants
//! nor activates a listener. Raw bearer tokens and TLS keys are never retained.
use crate::{Error, Result, envelope, identity};
use prost::Message;
use std::net::SocketAddr;
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.service_authority.v1.rs"));
}
pub const MAGIC: &[u8; 8] = b"MROWSAU1";
pub const VERSION: u32 = 1;
pub const MAX_RAW_BYTES: usize = 32 * 1024;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
pub const MAX_RECORDS: usize = 512;
pub const MAX_LIFETIME_MS: u64 = 30 * 24 * 60 * 60 * 1000;
// Do not derive Debug: verifiers must not leak through ordinary diagnostics.
#[derive(Clone)]
pub struct Record {
    value: proto::Record,
    container: Vec<u8>,
    reference: [u8; 32],
}
impl Record {
    pub fn encode(value: proto::Record) -> Result<Self> {
        let reference = validate(&value)?;
        let container = envelope::pack(MAGIC, &value.encode_to_vec(), MAX_RAW_BYTES)?;
        Ok(Self {
            value,
            container,
            reference,
        })
    }
    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        preflight(&raw, 0)?;
        let value = proto::Record::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("service authority protobuf"))?;
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical service authority"));
        }
        let reference = validate(&value)?;
        Ok(Self {
            value,
            container: container.to_vec(),
            reference,
        })
    }
    pub fn value(&self) -> &proto::Record {
        &self.value
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn reference(&self) -> [u8; 32] {
        self.reference
    }
    /// Half-open approval lifetime. This is a data check, not a live grant check.
    pub fn check_time(&self, now: u64) -> Result<()> {
        if self.value.disabled {
            return Err(Error::Invalid("disabled service authority"));
        }
        if now < self.value.created_ms {
            return Err(Error::Invalid("service authority clock regression"));
        }
        if now >= self.value.expires_ms {
            return Err(Error::Invalid("expired service authority"));
        }
        Ok(())
    }
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub(crate) fn identity(&self) -> (i64, &str) {
        match self.value.kind.as_ref().expect("validated authority kind") {
            proto::record::Kind::Authentication(value) => (1, &value.principal_id),
            proto::record::Kind::Publication(value) => (2, &value.config_id),
        }
    }
}
fn digest(bytes: &[u8]) -> Result<[u8; 32]> {
    let digest: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Invalid("service authority digest"))?;
    if digest == [0; 32] {
        return Err(Error::Invalid("service authority digest"));
    }
    Ok(digest)
}
fn path(value: &str) -> Result<()> {
    // Fixed origin-form paths only; no wildcard, query, fragment, authority or
    // whitespace. Restrict to URI path grammar so core needs no HTTP parser.
    if !value.starts_with('/')
        || value.starts_with("//")
        || value.len() > 8192
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-._~!$&'()+,;=:@%".contains(&b))
    {
        return Err(Error::Invalid("service authority path"));
    }
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(Error::Invalid("service authority path escape"));
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}
fn validate(value: &proto::Record) -> Result<[u8; 32]> {
    if value.schema_version != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    let reference = digest(&value.reference)?;
    if value.revision == 0 || value.revision > i64::MAX as u64 {
        return Err(Error::Invalid("service authority revision"));
    }
    let duration = value
        .expires_ms
        .checked_sub(value.created_ms)
        .filter(|v| *v > 0 && *v <= MAX_LIFETIME_MS);
    if value.created_ms == 0 || duration.is_none() {
        return Err(Error::Invalid("service authority lifetime"));
    }
    match value
        .kind
        .as_ref()
        .ok_or(Error::Invalid("service authority kind"))?
    {
        proto::record::Kind::Authentication(auth) => {
            if auth.principal_id.is_empty()
                || auth.principal_id.len() > 128
                || !auth
                    .principal_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            {
                return Err(Error::Invalid("service authority principal"));
            }
            digest(&auth.token_sha256)?;
        }
        proto::record::Kind::Publication(approval) => {
            identity(&approval.config_id)?;
            digest(&approval.config_sha256)?;
            if approval.listen_address.len() > 64 {
                return Err(Error::Limit);
            }
            let address: SocketAddr = approval
                .listen_address
                .parse()
                .map_err(|_| Error::Invalid("service authority address"))?;
            if address.to_string() != approval.listen_address
                || (!address.ip().is_loopback() && !approval.tls_required)
            {
                return Err(Error::Invalid("service authority address policy"));
            }
            if approval.method.is_empty()
                || approval.method.len() > 32
                || !approval
                    .method
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b == b'-')
            {
                return Err(Error::Invalid("service authority method"));
            }
            path(&approval.path)?;
            if !approval.query_path.is_empty() {
                path(&approval.query_path)?;
                if approval.query_path == approval.path {
                    return Err(Error::Invalid("service authority query path"));
                }
            }
        }
    }
    if value.encoded_len() > MAX_RAW_BYTES {
        return Err(Error::Limit);
    }
    Ok(reference)
}
fn preflight(mut raw: &[u8], level: u8) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut seen = [false; 9];
    while !raw.is_empty() {
        let (tag, wire) =
            decode_key(&mut raw).map_err(|_| Error::Invalid("service authority field"))?;
        let max_tag = match level {
            0 => 8,
            1 => 2,
            _ => 7,
        };
        if tag == 0 || tag > max_tag {
            return Err(Error::UnsupportedVersion);
        }
        if std::mem::replace(&mut seen[tag as usize], true)
            || (level == 0 && matches!(tag, 7 | 8) && seen[7] && seen[8])
        {
            return Err(Error::Invalid("duplicate service authority field"));
        }
        let scalar = matches!((level, tag), (0, 1 | 3 | 4 | 5 | 6) | (2, 4));
        if wire
            != if scalar {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err(Error::Invalid("service authority wire type"));
        }
        let number =
            decode_varint(&mut raw).map_err(|_| Error::Invalid("service authority value"))?;
        if scalar {
            if (level == 0 && tag == 1 && number > u32::MAX as u64)
                || (matches!((level, tag), (0, 6) | (2, 4)) && number > 1)
            {
                return Err(Error::Invalid("service authority scalar"));
            }
            continue;
        }
        let limit = match (level, tag) {
            (0, 7 | 8) => MAX_RAW_BYTES,
            (0, 2) | (1, 2) | (2, 2 | 5) => 32,
            (1, 1) => 128,
            (2, 1) => 256,
            (2, 3) => 64,
            (2, 6 | 7) => 8192,
            _ => return Err(Error::Invalid("service authority field")),
        };
        if number > limit as u64 {
            return Err(Error::Limit);
        }
        if number > raw.len() as u64 {
            return Err(Error::Invalid("service authority span"));
        }
        let (value, tail) = raw.split_at(number as usize);
        raw = tail;
        if level == 0 && matches!(tag, 7 | 8) {
            preflight(value, if tag == 7 { 1 } else { 2 })?;
        }
    }
    Ok(())
}
