//! Bounded codec for the existing experimental IO schema. Decoding is not a grant.
//! This first profile handles fixed-resource read/finish/cancel, never a submit or
//! external side effect. Known unsupported operations retain exact correlation.
use crate::{Error, Result, io_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub use wire::Status;
pub const VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub fn schema_digest() -> [u8; 32] {
    crate::plugin_package::io::schema_digest()
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid("IO frame")
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
fn reference(value: capnp::Result<capnp::data::Reader<'_>>) -> Result<[u8; 32]> {
    value
        .map_err(invalid)?
        .try_into()
        .map_err(|_| Error::Invalid("IO reference"))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedAction {
    Submit,
    Poll,
    Write,
    QueryOperation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Read {
        reference: [u8; 32],
        offset: u64,
        limit: u32,
    },
    Finish {
        reference: [u8; 32],
    },
    Cancel {
        reference: [u8; 32],
    },
    Unsupported(UnsupportedAction),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    bytes: Vec<u8>,
    call_id: u64,
    action: Action,
    digest: [u8; 32],
}
impl Request {
    pub fn encode_read(
        call_id: u64,
        reference: &[u8; 32],
        offset: u64,
        limit: u32,
    ) -> Result<Self> {
        if limit as usize > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        Self::encode(
            call_id,
            Action::Read {
                reference: *reference,
                offset,
                limit,
            },
        )
    }
    pub fn encode_finish(call_id: u64, reference: &[u8; 32]) -> Result<Self> {
        Self::encode(
            call_id,
            Action::Finish {
                reference: *reference,
            },
        )
    }
    pub fn encode_cancel(call_id: u64, reference: &[u8; 32]) -> Result<Self> {
        Self::encode(
            call_id,
            Action::Cancel {
                reference: *reference,
            },
        )
    }
    fn encode(call_id: u64, action: Action) -> Result<Self> {
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::request::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(call_id);
        match action {
            Action::Read {
                reference,
                offset,
                limit,
            } => {
                let mut r = root.init_read();
                r.set_reference(&reference);
                r.set_offset(offset);
                r.set_limit(limit);
            }
            Action::Finish { reference } => root.set_finish(&reference),
            Action::Cancel { reference } => root.set_cancel(&reference),
            Action::Unsupported(_) => return Err(Error::Invalid("unsupported IO encoder")),
        }
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
        let action = match root.which().map_err(invalid)? {
            wire::request::Read(value) => {
                let r = value.map_err(invalid)?;
                let limit = r.get_limit();
                if limit as usize > MAX_PAYLOAD_BYTES {
                    return Err(Error::Limit);
                }
                Action::Read {
                    reference: reference(r.get_reference())?,
                    offset: r.get_offset(),
                    limit,
                }
            }
            wire::request::Finish(value) => Action::Finish {
                reference: reference(value)?,
            },
            wire::request::Cancel(value) => Action::Cancel {
                reference: reference(value)?,
            },
            wire::request::Submit(value) => {
                // Even unimplemented submissions must reject unknown nested tags.
                match value.map_err(invalid)?.which().map_err(invalid)? {
                    wire::submission::FileRead(v)
                    | wire::submission::FileList(v)
                    | wire::submission::HttpListen(v)
                    | wire::submission::HttpUnpublish(v)
                    | wire::submission::ServiceAccept(v) => {
                        v.map_err(invalid)?;
                    }
                    wire::submission::FileCreate(v)
                    | wire::submission::FileReplace(v)
                    | wire::submission::FileDelete(v) => {
                        v.map_err(invalid)?;
                    }
                    wire::submission::HttpRequest(v) | wire::submission::WebSocketConnect(v) => {
                        v.map_err(invalid)?;
                    }
                    wire::submission::HttpPublish(v) => {
                        v.map_err(invalid)?;
                    }
                    wire::submission::ServiceReply(v) => {
                        v.map_err(invalid)?;
                    }
                }
                Action::Unsupported(UnsupportedAction::Submit)
            }
            wire::request::Poll(value) => {
                value.map_err(invalid)?;
                Action::Unsupported(UnsupportedAction::Poll)
            }
            wire::request::Write(value) => {
                value.map_err(invalid)?;
                Action::Unsupported(UnsupportedAction::Write)
            }
            wire::request::QueryOperation(value) => {
                value.map_err(invalid)?;
                Action::Unsupported(UnsupportedAction::QueryOperation)
            }
        };
        Ok(Self {
            bytes: bytes.to_vec(),
            call_id: root.get_call_id(),
            action,
            digest: Sha256::digest(bytes).into(),
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn action(&self) -> &Action {
        &self.action
    }
    pub fn call_id(&self) -> u64 {
        self.call_id
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: Status,
    pub payload: Vec<u8>,
    pub offset: u64,
    pub eof: bool,
}
fn validate_response(
    request: &Request,
    status: Status,
    payload: &[u8],
    offset: u64,
    eof: bool,
) -> Result<()> {
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(Error::Limit);
    }
    if status == Status::Invalid {
        return Err(Error::Invalid("invalid IO status"));
    }
    if status != Status::Completed {
        if !payload.is_empty() || offset != 0 || !eof {
            return Err(Error::Invalid("IO status payload"));
        }
        return Ok(());
    }
    match request.action() {
        Action::Read {
            offset: start,
            limit,
            ..
        } => {
            let maximum = if *limit == 0 {
                MAX_PAYLOAD_BYTES
            } else {
                *limit as usize
            };
            if offset != *start
                || payload.len() > maximum
                || offset.checked_add(payload.len() as u64).is_none()
                || (payload.is_empty() && !eof)
            {
                return Err(Error::Invalid("IO read result"));
            }
        }
        Action::Finish { .. } | Action::Cancel { .. } => {
            if !payload.is_empty() || offset != 0 || !eof {
                return Err(Error::Invalid("IO close result"));
            }
        }
        Action::Unsupported(_) => return Err(Error::Invalid("unsupported IO success")),
    }
    Ok(())
}
impl Response {
    pub fn encode(
        request: &Request,
        status: Status,
        payload: &[u8],
        offset: u64,
        eof: bool,
    ) -> Result<Vec<u8>> {
        validate_response(request, status, payload, offset, eof)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::response::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(request.call_id());
        root.set_request_sha256(&request.digest());
        root.set_status(status);
        root.set_bytes(payload);
        root.set_offset(offset);
        root.set_eof(eof);
        finish(&message)
    }
    pub fn decode(request: &Request, bytes: &[u8]) -> Result<Self> {
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
        if !root.get_reference().map_err(invalid)?.is_empty()
            || root.get_http_status() != 0
            || !root.get_headers().map_err(invalid)?.is_empty()
        {
            return Err(Error::Invalid("unsupported IO result fields"));
        }
        let status = root.get_status().map_err(invalid)?;
        let payload = root.get_bytes().map_err(invalid)?;
        let offset = root.get_offset();
        let eof = root.get_eof();
        validate_response(request, status, payload, offset, eof)?;
        Ok(Self {
            status,
            payload: payload.to_vec(),
            offset,
            eof,
        })
    }
}
