//! Bounded outbound approvals and protected credential envelopes. Neither
//! loading one nor checking its lifetime creates a live resource/IO grant.
use crate::{Error, Result, envelope, identity, io};
use prost::Message;
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.outbound_authority.v1.rs"));
}
pub const MAGIC: &[u8; 8] = b"MROWOAU1";
pub const VERSION: u32 = 1;
pub const MAX_RAW_BYTES: usize = 64 * 1024;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
pub const MAX_RECORDS: usize = 512;
pub const MAX_LIFETIME_MS: u64 = 30 * 24 * 60 * 60 * 1000;
pub const MAX_CIPHERTEXT_BYTES: usize = 32 * 1024;
pub const MAX_CERTIFICATE_BYTES: usize = 32 * 1024;
pub const WINDOWS_DPAPI_PROVIDER: &str = "windows-current-user-dpapi-http-v1";
// No Debug: ciphertext and endpoint metadata must not appear in diagnostics.
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
            .map_err(|_| Error::Invalid("outbound authority protobuf"))?;
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical outbound authority"));
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
    /// Complete canonical record identity, independent of envelope compression.
    /// A digest identifies saved policy; it never restores live authority.
    pub fn canonical_digest(&self) -> [u8; 32] {
        use sha2::Digest;
        sha2::Sha256::digest(self.value.encode_to_vec()).into()
    }
    pub fn check_time(&self, now: u64) -> Result<()> {
        if self.value.disabled {
            return Err(Error::Invalid("disabled outbound authority"));
        }
        if now < self.value.created_ms {
            return Err(Error::Invalid("outbound authority clock regression"));
        }
        if now >= self.value.expires_ms {
            return Err(Error::Invalid("expired outbound authority"));
        }
        Ok(())
    }
    #[cfg(any(not(target_arch = "wasm32"), feature = "web-storage"))]
    pub(crate) fn identity(&self) -> (i64, &str) {
        match self.value.kind.as_ref().expect("validated outbound kind") {
            proto::record::Kind::Credential(value) => (1, &value.provider),
            proto::record::Kind::Endpoint(value) => (2, &value.package_id),
        }
    }
}
fn digest(bytes: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Invalid("outbound authority digest"))?;
    if value == [0; 32] {
        return Err(Error::Invalid("outbound authority digest"));
    }
    Ok(value)
}
// Lexical canonical origin only. DNS/address safety, URL parser equivalence,
// certificate parsing and the selected network profile are checked by transport.
fn origin(value: &str) -> Result<&str> {
    if value.len() > 2048 {
        return Err(Error::Limit);
    }
    let (scheme, authority) = value
        .split_once("://")
        .ok_or(Error::Invalid("outbound origin"))?;
    if !matches!(scheme, "http" | "https")
        || authority.is_empty()
        || !authority.is_ascii()
        || authority
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace() || b"/?#@\\".contains(&b))
    {
        return Err(Error::Invalid("outbound origin"));
    }
    let port = if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or(Error::Invalid("outbound IPv6 origin"))?;
        let host = &authority[1..end];
        let ip: std::net::Ipv6Addr = host
            .parse()
            .map_err(|_| Error::Invalid("outbound IPv6 origin"))?;
        if ip.to_string() != host {
            return Err(Error::Invalid("noncanonical outbound IPv6 origin"));
        }
        let tail = &authority[end + 1..];
        if tail.is_empty() {
            None
        } else {
            Some(
                tail.strip_prefix(':')
                    .ok_or(Error::Invalid("outbound origin port"))?,
            )
        }
    } else {
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        if host.is_empty()
            || host.len() > 253
            || host.bytes().any(|b| {
                !b.is_ascii_lowercase() && !b.is_ascii_digit() && !matches!(b, b'.' | b'-')
            })
        {
            return Err(Error::Invalid("outbound origin host"));
        }
        if host.split('.').any(|label| {
            label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-')
        }) {
            return Err(Error::Invalid("outbound origin host"));
        }
        if let Ok(ip) = host.parse::<std::net::Ipv4Addr>()
            && ip.to_string() != host
        {
            return Err(Error::Invalid("noncanonical outbound IPv4 origin"));
        }
        port
    };
    if let Some(port) = port {
        let parsed: u16 = port
            .parse()
            .map_err(|_| Error::Invalid("outbound origin port"))?;
        if parsed == 0
            || parsed.to_string() != port
            || (scheme == "http" && parsed == 80)
            || (scheme == "https" && parsed == 443)
        {
            return Err(Error::Invalid("noncanonical outbound origin port"));
        }
    }
    Ok(scheme)
}
fn validate(value: &proto::Record) -> Result<[u8; 32]> {
    if value.schema_version != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    let reference = digest(&value.reference)?;
    if value.revision == 0 || value.revision > i64::MAX as u64 {
        return Err(Error::Invalid("outbound authority revision"));
    }
    if value.created_ms == 0
        || value
            .expires_ms
            .checked_sub(value.created_ms)
            .is_none_or(|duration| duration == 0 || duration > MAX_LIFETIME_MS)
    {
        return Err(Error::Invalid("outbound authority lifetime"));
    }
    match value
        .kind
        .as_ref()
        .ok_or(Error::Invalid("outbound authority kind"))?
    {
        proto::record::Kind::Credential(credential) => {
            if credential.provider != WINDOWS_DPAPI_PROVIDER {
                return Err(Error::Invalid("outbound credential provider"));
            }
            if credential.ciphertext.is_empty()
                || credential.ciphertext.len() > MAX_CIPHERTEXT_BYTES
            {
                return Err(Error::Limit);
            }
        }
        proto::record::Kind::Endpoint(endpoint) => {
            identity(&endpoint.package_id)?;
            digest(&endpoint.package_sha256)?;
            let scheme = origin(&endpoint.origin)?;
            if !(1..=3).contains(&endpoint.profile)
                || (endpoint.profile == 2 && scheme != "http")
                || (endpoint.profile != 2 && scheme != "https")
            {
                return Err(Error::Invalid("outbound network profile"));
            }
            if endpoint.methods.is_empty() || endpoint.methods.len() > 16 {
                return Err(Error::Limit);
            }
            let mut previous: Option<&str> = None;
            for method in &endpoint.methods {
                if method.is_empty()
                    || method.len() > 32
                    || !method.bytes().all(|b| b.is_ascii_uppercase() || b == b'-')
                    || previous.is_some_and(|old| old >= method.as_str())
                {
                    return Err(Error::Invalid("outbound allowed method"));
                }
                previous = Some(method);
            }
            if !endpoint.credential_reference.is_empty() {
                digest(&endpoint.credential_reference)?;
            }
            if endpoint.root_certificate.len() > MAX_CERTIFICATE_BYTES
                || endpoint.max_request_bytes > io::MAX_PAYLOAD_BYTES as u64
                || endpoint.max_response_bytes > io::MAX_PAYLOAD_BYTES as u64
                || endpoint.max_header_bytes > io::MAX_HEADER_BYTES as u64
                || endpoint.max_request_bytes == 0
                || endpoint.max_response_bytes == 0
                || endpoint.max_header_bytes == 0
                || !(1..=128).contains(&endpoint.max_concurrent)
                || endpoint.timeout_ms == 0
                || endpoint.timeout_ms > io::MAX_SUBMIT_DEADLINE_MS
                || endpoint.max_frame_bytes == 0
                || endpoint.max_frame_bytes > io::MAX_FRAME_BYTES as u64
            {
                return Err(Error::Limit);
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
    let mut seen = [0u8; 14];
    while !raw.is_empty() {
        let (tag, wire) =
            decode_key(&mut raw).map_err(|_| Error::Invalid("outbound authority field"))?;
        let max_tag = match level {
            0 => 8,
            1 => 2,
            _ => 13,
        };
        if tag == 0 || tag > max_tag {
            return Err(Error::UnsupportedVersion);
        }
        seen[tag as usize] += 1;
        let limit = if level == 2 && tag == 5 { 16 } else { 1 };
        if seen[tag as usize] > limit || (level == 0 && seen[7] > 0 && seen[8] > 0) {
            return Err(Error::Invalid("duplicate outbound authority field"));
        }
        let scalar = matches!((level, tag), (0, 1 | 3 | 4 | 5 | 6) | (2, 4 | 8..=13));
        if wire
            != if scalar {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err(Error::Invalid("outbound authority wire type"));
        }
        let number =
            decode_varint(&mut raw).map_err(|_| Error::Invalid("outbound authority value"))?;
        if scalar {
            if (matches!((level, tag), (0, 1) | (2, 11)) && number > u32::MAX as u64)
                || (level == 0 && tag == 6 && number > 1)
                || (level == 2 && tag == 4 && number > 3)
            {
                return Err(Error::Invalid("outbound authority scalar"));
            }
            continue;
        }
        let limit = match (level, tag) {
            (0, 7 | 8) => MAX_RAW_BYTES,
            (0, 2) | (2, 2 | 6) => 32,
            (1, 1) => WINDOWS_DPAPI_PROVIDER.len(),
            (1, 2) => MAX_CIPHERTEXT_BYTES,
            (2, 1) => 256,
            (2, 3) => 2048,
            (2, 5) => 32,
            (2, 7) => MAX_CERTIFICATE_BYTES,
            _ => return Err(Error::Invalid("outbound authority field")),
        };
        if number > limit as u64 {
            return Err(Error::Limit);
        }
        if number > raw.len() as u64 {
            return Err(Error::Invalid("outbound authority span"));
        }
        let (value, tail) = raw.split_at(number as usize);
        raw = tail;
        if level == 0 && matches!(tag, 7 | 8) {
            preflight(value, if tag == 7 { 1 } else { 2 })?;
        }
    }
    Ok(())
}
