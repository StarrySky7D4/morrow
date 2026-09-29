//! Experimental G0 contract. This crate does not link Store, network or an executor.
//! Generated Cap'n Proto types are the public NativeSession/Stream/Event/Tool API.
use capnp::{message, serialize};
use sha2::{Digest, Sha256};

#[allow(clippy::all)]
pub mod agent_host_capnp {
    include!(concat!(env!("OUT_DIR"), "/agent_host_capnp.rs"));
}
include!(concat!(env!("OUT_DIR"), "/identity.rs"));
#[cfg(feature = "qualification")]
pub mod fake;

pub const MAJOR: u16 = 1;
pub const REVISION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 65536;
pub const MAX_CHUNK_BYTES: usize = 4096;
pub const MAX_REQUEST_BYTES: usize = 262144;
pub const MAX_EVENTS: usize = 128;
pub const MAX_BATCH_EVENTS: usize = 8;
pub const MAX_TOOLS: usize = 16;
pub const MAX_STREAMS: usize = 4;
pub const MAX_ID_BYTES: usize = 128;
pub type U64 = u64;
pub type Revision = U64;
pub type AccountEpoch = U64;
pub type OperationId = String;
pub type AttemptId = String;
pub use agent_host_capnp::{ClockDomain, ErrorCode, StreamState, ToolState};
pub use agent_host_capnp::{deadline, error_envelope, resource_ref};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    UnsupportedVersion,
    SchemaMismatch,
    Limit,
    WrongDirection,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<capnp::Error> for Error {
    fn from(_: capnp::Error) -> Self {
        Self::Invalid
    }
}
impl From<capnp::NotInSchema> for Error {
    fn from(_: capnp::NotInSchema) -> Self {
        Self::Invalid
    }
}

/// JSON adapters must call this on a JSON string value, never on a Number.
/// Leading zeros, signs, whitespace and overflow are rejected.
pub fn parse_json_u64(value: &str) -> Result<U64, Error> {
    if value.is_empty()
        || value.len() > 20
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|v| v.is_ascii_digit())
    {
        return Err(Error::Invalid);
    }
    value.parse().map_err(|_| Error::Invalid)
}
pub fn json_u64(value: U64) -> String {
    value.to_string()
}
pub fn digest(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || b"._:/-".contains(&v))
}
pub fn text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String, Error> {
    Ok(value?.to_str().map_err(|_| Error::Invalid)?.to_owned())
}
pub fn id(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String, Error> {
    let result = text(value)?;
    if !valid_id(&result) {
        return Err(Error::Invalid);
    }
    Ok(result)
}

pub fn frame(
    request_id: U64,
    session_id: &str,
    instance_epoch: U64,
) -> message::Builder<message::HeapAllocator> {
    let mut msg = message::Builder::new_default();
    let mut f = msg.init_root::<agent_host_capnp::frame::Builder>();
    f.set_major(MAJOR);
    f.set_revision(REVISION);
    f.set_schema_digest(&SCHEMA_DIGEST);
    f.set_request_id(request_id);
    f.set_session_id(session_id);
    f.set_instance_epoch(instance_epoch);
    msg
}
pub fn encode(msg: &message::Builder<message::HeapAllocator>) -> Result<Vec<u8>, Error> {
    let bytes = serialize::write_message_to_words(msg);
    decode(&bytes)?;
    Ok(bytes)
}
pub type Message = message::Reader<serialize::OwnedSegments>;
/// Parses exactly one unpacked Cap'n Proto message with bounded traversal.
/// Unknown major/revision/digest/discriminant fail before fake business dispatch.
pub fn decode(bytes: &[u8]) -> Result<Message, Error> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let mut options = message::ReaderOptions::new();
    options.traversal_limit_in_words(Some(MAX_FRAME_BYTES / 8));
    options.nesting_limit(24);
    let msg = serialize::read_message(&mut cursor, options)?;
    if cursor.position() != bytes.len() as u64 {
        return Err(Error::Invalid);
    }
    let f = msg.get_root::<agent_host_capnp::frame::Reader>()?;
    if f.get_major() != MAJOR || f.get_revision() != REVISION {
        return Err(Error::UnsupportedVersion);
    }
    if f.get_schema_digest()? != SCHEMA_DIGEST {
        return Err(Error::SchemaMismatch);
    }
    if f.get_request_id() == 0 || f.get_instance_epoch() == 0 {
        return Err(Error::Invalid);
    }
    id(f.get_session_id())?;
    use agent_host_capnp::frame::Which;
    match f.which()? {
        Which::NativeSession(v) => {
            v?.which()?;
        }
        Which::Stream(v) => {
            if let agent_host_capnp::stream::Which::Open(v) = v?.which()? {
                v?.get_deadline()?.get_domain()?;
            }
        }
        Which::Event(v) => {
            v?.which()?;
        }
        Which::Tool(v) => {
            if let agent_host_capnp::tool::Which::Report(v) = v?.which()? {
                v?.get_state()?;
            }
        }
        Which::Reply(v) => {
            validate_reply(v?)?;
        }
    }
    Ok(msg)
}

fn validate_reply(reply: agent_host_capnp::reply::Reader<'_>) -> Result<(), Error> {
    use agent_host_capnp::reply::Which as R;
    if !reply.get_qualification_only() {
        return Err(Error::Invalid);
    }
    match reply.which()? {
        R::NativeSession(v) => {
            let v = v?;
            if !v.get_qualification_only()
                || v.get_capability_bits() & !15 != 0
                || (v.get_capability_bits() == 0 && !v.get_closing_unconfirmed())
            {
                return Err(Error::Invalid);
            }
        }
        R::Stream(v) => {
            let v = v?;
            let state = v.get_state()?;
            if state == StreamState::Unspecified
                || v.get_bytes()?.len() > MAX_CHUNK_BYTES
                || v.get_offset() > MAX_REQUEST_BYTES as u64
            {
                return Err(Error::Invalid);
            }
            if matches!(
                state,
                StreamState::Committed | StreamState::Streaming | StreamState::Eof
            ) && !v.get_request_fixed()
            {
                return Err(Error::Invalid);
            }
        }
        R::Event(v) => {
            let v = v?;
            let events = v.get_events()?;
            if v.get_writer_epoch() == 0 || events.len() as usize > MAX_BATCH_EVENTS {
                return Err(Error::Invalid);
            }
            for event in events.iter() {
                id(event.get_turn_id())?;
                id(event.get_item_id())?;
                id(event.get_part_id())?;
                id(event.get_attempt_id())?;
                id(event.get_semantic_kind())?;
                let payload = event.get_payload()?;
                if payload.len() > MAX_CHUNK_BYTES || event.get_source_digest()? != digest(payload)
                {
                    return Err(Error::Invalid);
                }
            }
        }
        R::Tool(v) => {
            let v = v?;
            let state = v.get_state()?;
            if state == ToolState::Unspecified || (v.get_execute() && state != ToolState::Claimed) {
                return Err(Error::Invalid);
            }
        }
        R::Error(v) => {
            let v = v?;
            if v.get_code()? == ErrorCode::Unspecified {
                return Err(Error::Invalid);
            }
            id(v.get_stage())?;
            id(v.get_recovery())?;
            id(v.get_diagnostic_id())?;
            for optional in [text(v.get_operation_id())?, text(v.get_attempt_id())?] {
                if !optional.is_empty() && !valid_id(&optional) {
                    return Err(Error::Invalid);
                }
            }
        }
    }
    Ok(())
}

/// A transport is deliberately injected. No network/process/store fallback exists.
pub trait Transport {
    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, Error>;
}
pub fn exchange_checked(transport: &mut impl Transport, request: &[u8]) -> Result<Message, Error> {
    let input = decode(request)?;
    let a = input.get_root::<agent_host_capnp::frame::Reader>()?;
    if matches!(a.which()?, agent_host_capnp::frame::Which::Reply(_)) {
        return Err(Error::WrongDirection);
    }
    let bytes = transport.exchange(request)?;
    let output = decode(&bytes)?;
    let b = output.get_root::<agent_host_capnp::frame::Reader>()?;
    if !matches!(b.which()?, agent_host_capnp::frame::Which::Reply(_)) {
        return Err(Error::WrongDirection);
    }
    if a.get_request_id() != b.get_request_id()
        || a.get_instance_epoch() != b.get_instance_epoch()
        || text(a.get_session_id())? != text(b.get_session_id())?
    {
        return Err(Error::Invalid);
    }
    use agent_host_capnp::{frame::Which as F, reply::Which as R};
    let F::Reply(reply) = b.which()? else {
        return Err(Error::WrongDirection);
    };
    match (a.which()?, reply?.which()?) {
        (_, R::Error(_))
        | (F::NativeSession(_), R::NativeSession(_))
        | (F::Stream(_), R::Stream(_))
        | (F::Event(_), R::Event(_))
        | (F::Tool(_), R::Tool(_)) => {}
        _ => return Err(Error::WrongDirection),
    }
    Ok(output)
}
