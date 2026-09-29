//! Bounded inbound service contract. Principal is a trusted host assertion;
//! decoding a frame grants neither publication nor authenticated access.
use crate::{Error, Result, identity, io::Header, service_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub const VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_HEADERS: usize = 64;
pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/service.capnp"))
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid("service frame")
}
fn read(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::OwnedSegments>> {
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
    Ok(message)
}
fn finish(message: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(message);
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
fn text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    std::str::from_utf8(value.map_err(invalid)?.as_bytes())
        .map(str::to_owned)
        .map_err(invalid)
}
fn headers_valid(headers: &[Header], inbound: bool) -> Result<()> {
    if headers.len() > MAX_HEADERS {
        return Err(Error::Limit);
    }
    let mut bytes = 0usize;
    for header in headers {
        if header.name.is_empty()
            || header.name.len() > crate::io::MAX_HEADER_NAME_BYTES
            || !header
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || header.value.len() > crate::io::MAX_HEADER_VALUE_BYTES
            || header
                .value
                .iter()
                .any(|&b| (b < 0x20 && b != b'\t') || b == 0x7f)
        {
            return Err(invalid(()));
        }
        let name = header.name.to_ascii_lowercase();
        if name.starts_with("proxy-")
            || matches!(
                name.as_str(),
                "host"
                    | "content-length"
                    | "transfer-encoding"
                    | "connection"
                    | "keep-alive"
                    | "upgrade"
                    | "te"
                    | "trailer"
                    | "expect"
            )
            || (inbound && matches!(name.as_str(), "authorization" | "cookie"))
        {
            return Err(Error::Invalid("service transport header"));
        }
        bytes = bytes
            .checked_add(header.name.len() + header.value.len() + 4)
            .ok_or(Error::Limit)?;
        if bytes > MAX_HEADER_BYTES {
            return Err(Error::Limit);
        }
    }
    Ok(())
}
fn headers_read(
    reader: capnp::Result<capnp::struct_list::Reader<'_, wire::header::Owned>>,
) -> Result<Vec<Header>> {
    let reader = reader.map_err(invalid)?;
    if reader.len() as usize > MAX_HEADERS {
        return Err(Error::Limit);
    }
    reader
        .iter()
        .map(|header| {
            let value = header.get_value().map_err(invalid)?;
            if value.len() > crate::io::MAX_HEADER_VALUE_BYTES {
                return Err(Error::Limit);
            }
            Ok(Header {
                name: text(header.get_name())?,
                value: value.to_vec(),
            })
        })
        .collect()
}
fn headers_write(
    mut output: capnp::struct_list::Builder<'_, wire::header::Owned>,
    headers: &[Header],
) {
    for (index, header) in headers.iter().enumerate() {
        let mut item = output.reborrow().get(index as u32);
        item.set_name(&header.name);
        item.set_value(&header.value);
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct Invocation {
    pub service: String,
    pub handler: String,
    pub principal: String,
    pub method: String,
    pub target: String,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
}
fn validate(invocation: &Invocation) -> Result<()> {
    identity(&invocation.service)?;
    identity(&invocation.handler)?;
    identity(&invocation.principal)?;
    if !["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]
        .contains(&invocation.method.as_str())
    {
        return Err(Error::Invalid("service method"));
    }
    let target = &invocation.target;
    if target.len() > crate::io::MAX_TARGET_BYTES
        || !target.starts_with('/')
        || target.starts_with("//")
        || target.contains("://")
        || target
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '\\' | '#'))
    {
        return Err(Error::Invalid("service target"));
    }
    let raw = target.as_bytes();
    for (i, byte) in raw.iter().enumerate() {
        if *byte == b'%'
            && (raw.get(i + 1).is_none_or(|b| !b.is_ascii_hexdigit())
                || raw.get(i + 2).is_none_or(|b| !b.is_ascii_hexdigit()))
        {
            return Err(Error::Invalid("service target escape"));
        }
    }
    if invocation.body.len() > MAX_BODY_BYTES {
        return Err(Error::Limit);
    }
    headers_valid(&invocation.headers, true)
}
/// Immutable original request; its complete frame digest binds each response.
pub struct Request {
    bytes: Vec<u8>,
    digest: [u8; 32],
    call_id: u64,
    invocation: Invocation,
}
impl Request {
    pub fn encode(call_id: u64, invocation: &Invocation) -> Result<Self> {
        validate(invocation)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::request::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(call_id);
        root.set_service(&invocation.service);
        root.set_handler(&invocation.handler);
        root.set_principal(&invocation.principal);
        root.set_method(&invocation.method);
        root.set_target(&invocation.target);
        root.set_body(&invocation.body);
        headers_write(
            root.init_headers(invocation.headers.len() as u32),
            &invocation.headers,
        );
        Self::decode(&finish(&message)?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::request::Reader>()
            .map_err(invalid)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(invalid(()));
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        let body = root.get_body().map_err(invalid)?;
        if body.len() > MAX_BODY_BYTES {
            return Err(Error::Limit);
        }
        let invocation = Invocation {
            service: text(root.get_service())?,
            handler: text(root.get_handler())?,
            principal: text(root.get_principal())?,
            method: text(root.get_method())?,
            target: text(root.get_target())?,
            headers: headers_read(root.get_headers())?,
            body: body.to_vec(),
        };
        validate(&invocation)?;
        Ok(Self {
            bytes: bytes.to_vec(),
            digest: Sha256::digest(bytes).into(),
            call_id: root.get_call_id(),
            invocation,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn call_id(&self) -> u64 {
        self.call_id
    }
    pub fn invocation(&self) -> &Invocation {
        &self.invocation
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<Header>,
    pub body: Vec<u8>,
}
fn validate_reply(reply: &Reply) -> Result<()> {
    if !(200..=599).contains(&reply.status)
        || (matches!(reply.status, 204 | 205 | 304) && !reply.body.is_empty())
    {
        return Err(Error::Invalid("service response status/body"));
    }
    if reply.body.len() > MAX_BODY_BYTES {
        return Err(Error::Limit);
    }
    headers_valid(&reply.headers, false)
}
pub struct Response;
impl Response {
    pub fn encode(request: &Request, reply: &Reply) -> Result<Vec<u8>> {
        validate_reply(reply)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::response::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(request.call_id());
        root.set_request_sha256(&request.digest());
        root.set_status(reply.status);
        root.set_body(&reply.body);
        headers_write(
            root.init_headers(reply.headers.len() as u32),
            &reply.headers,
        );
        finish(&message)
    }
    pub fn decode(request: &Request, bytes: &[u8]) -> Result<Reply> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::response::Reader>()
            .map_err(invalid)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(invalid(()));
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        if root.get_call_id() != request.call_id()
            || root.get_request_sha256().map_err(invalid)? != request.digest()
        {
            return Err(Error::Integrity);
        }
        let body = root.get_body().map_err(invalid)?;
        if body.len() > MAX_BODY_BYTES {
            return Err(Error::Limit);
        }
        let reply = Reply {
            status: root.get_status(),
            headers: headers_read(root.get_headers())?,
            body: body.to_vec(),
        };
        validate_reply(&reply)?;
        Ok(reply)
    }
}
