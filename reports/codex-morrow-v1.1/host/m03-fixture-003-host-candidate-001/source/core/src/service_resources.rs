//! Independently versioned, opt-in service metadata. Decoding these references
//! never grants IO authority; the original live broker validates every use.
use crate::{Error, Result, service_resources_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};

pub const VERSION: u16 = 1;
pub const FEATURE: &str = "service-resources-v1";
pub const HEADER: &str = "morrow-service-resources-v1";
pub const MAX_ENDPOINTS: usize = 8;
pub const MAX_FRAME_BYTES: usize = 4096;

pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/service_resources.capnp"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub reference: String,
    /// An opaque reference, never a credential value.
    pub credential: Vec<u8>,
    pub methods: Vec<String>,
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
    pub timeout_ms: u64,
    pub response_frame_limit: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory {
    pub scope_sha256: [u8; 32],
    pub endpoints: Vec<Endpoint>,
}

fn invalid<T>(_: T) -> Error {
    Error::Invalid("service resources")
}
fn is_hex_lower(b: u8) -> bool {
    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
}
fn validate(value: &Directory) -> Result<()> {
    if value.scope_sha256 == [0; 32] {
        return Err(invalid(()));
    }
    if value.endpoints.is_empty() || value.endpoints.len() > MAX_ENDPOINTS {
        return Err(Error::Limit);
    }
    let mut previous: Option<&str> = None;
    for e in &value.endpoints {
        if e.reference.len() != 64
            || !e.reference.bytes().all(is_hex_lower)
            || e.reference.bytes().all(|b| b == b'0')
            || previous.is_some_and(|p| p >= e.reference.as_str())
        {
            return Err(invalid(()));
        }
        previous = Some(&e.reference);
        if e.credential.len() > 256 || e.credential.iter().any(u8::is_ascii_control) {
            return Err(invalid(()));
        }
        if e.methods.is_empty()
            || e.methods.len() > 7
            || e.methods.windows(2).any(|w| w[0] >= w[1])
            || e.methods.iter().any(|m| {
                !["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"].contains(&m.as_str())
            })
        {
            return Err(invalid(()));
        }
        let in_range = |v: u64, max: u64| (1..=max).contains(&v);
        if !in_range(e.max_request_bytes, 64 * 1024 * 1024)
            || !in_range(e.max_response_bytes, 64 * 1024 * 1024)
            || !in_range(e.timeout_ms, 300_000)
            || !in_range(e.response_frame_limit, 128 * 1024)
        {
            return Err(Error::Limit);
        }
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = Vec::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize]);
        out.push(HEX[(b & 15) as usize]);
    }
    out
}
fn unhex(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.is_empty()
        || !bytes.len().is_multiple_of(2)
        || bytes.len() > 2 * MAX_FRAME_BYTES
        || !bytes.iter().copied().all(is_hex_lower)
    {
        return Err(invalid(()));
    }
    let val = |c: u8| {
        if c.is_ascii_digit() {
            c - b'0'
        } else {
            c - b'a' + 10
        }
    };
    Ok(bytes
        .chunks_exact(2)
        .map(|p| (val(p[0]) << 4) | val(p[1]))
        .collect())
}
fn text(value: capnp::Result<capnp::text::Reader<'_>>, limit: usize) -> Result<String> {
    let value = value.map_err(invalid)?;
    if value.len() > limit {
        return Err(Error::Limit);
    }
    Ok(std::str::from_utf8(value.as_bytes())
        .map_err(invalid)?
        .to_owned())
}

impl Directory {
    pub fn encode(&self) -> Result<Vec<u8>> {
        validate(self)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::directory::Builder<'_>>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_scope_sha256(&self.scope_sha256);
        let mut entries = root.init_endpoints(self.endpoints.len() as u32);
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let mut item = entries.reborrow().get(index as u32);
            item.set_reference(&endpoint.reference);
            item.set_credential(&endpoint.credential);
            item.set_max_request_bytes(endpoint.max_request_bytes);
            item.set_max_response_bytes(endpoint.max_response_bytes);
            item.set_timeout_ms(endpoint.timeout_ms);
            item.set_response_frame_limit(endpoint.response_frame_limit);
            let mut methods = item.init_methods(endpoint.methods.len() as u32);
            for (i, method) in endpoint.methods.iter().enumerate() {
                methods.set(i as u32, method);
            }
        }
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(Error::Limit);
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(Error::Limit);
        }
        let mut remaining = bytes;
        let message = serialize::read_message(
            &mut remaining,
            ReaderOptions {
                traversal_limit_in_words: Some(2 * MAX_FRAME_BYTES / 8),
                nesting_limit: 8,
            },
        )
        .map_err(invalid)?;
        if !remaining.is_empty() {
            return Err(invalid(()));
        }
        let root = message
            .get_root::<wire::directory::Reader<'_>>()
            .map_err(invalid)?;
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        let scope_sha256 = root
            .get_scope_sha256()
            .map_err(invalid)?
            .try_into()
            .map_err(invalid)?;
        let entries = root.get_endpoints().map_err(invalid)?;
        if entries.len() as usize > MAX_ENDPOINTS {
            return Err(Error::Limit);
        }
        let mut endpoints = Vec::with_capacity(entries.len() as usize);
        for item in entries {
            let credential = item.get_credential().map_err(invalid)?;
            if credential.len() > 256 {
                return Err(Error::Limit);
            }
            let methods = item.get_methods().map_err(invalid)?;
            if methods.len() > 7 {
                return Err(Error::Limit);
            }
            endpoints.push(Endpoint {
                reference: text(item.get_reference(), 64)?,
                credential: credential.to_vec(),
                methods: methods.iter().map(|m| text(m, 7)).collect::<Result<_>>()?,
                max_request_bytes: item.get_max_request_bytes(),
                max_response_bytes: item.get_max_response_bytes(),
                timeout_ms: item.get_timeout_ms(),
                response_frame_limit: item.get_response_frame_limit(),
            });
        }
        let value = Self {
            scope_sha256,
            endpoints,
        };
        // One canonical encoding makes the host context stable for replay and
        // rejects unknown fields, extra segments, padding and wire aliases.
        if value.encode()? != bytes {
            return Err(invalid(()));
        }
        Ok(value)
    }

    pub fn to_header(&self) -> Result<crate::io::Header> {
        Ok(crate::io::Header {
            name: HEADER.into(),
            value: hex(&self.encode()?),
        })
    }

    pub fn from_headers(headers: &[crate::io::Header]) -> Result<Option<Self>> {
        let mut found = headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case(HEADER));
        let Some(header) = found.next() else {
            return Ok(None);
        };
        if found.next().is_some() {
            return Err(invalid(()));
        }
        Self::decode(&unhex(&header.value)?).map(Some)
    }
}
