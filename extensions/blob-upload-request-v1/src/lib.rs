//! Independent upload request candidate. Byte validation is not authorization.
//! A host must separately nominate and approve the exact original operation.
//! Chunk receipts never establish persistence, worker return, or business commit.
#![deny(unsafe_op_in_unsafe_fn)]

use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
    traits::{HasStructSize, IntoInternalStructReader},
};
use morrow_blob_transfer_v1::{ActionRef as BlobAction, FrameRef, Identity};
use sha2::{Digest, Sha256};
use std::{fmt, ops::Range};

mod client;
pub use client::{Client, ClientLimits, ClientPhase, ClientSnapshot};
pub mod ffi;
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
pub mod wasm;

#[allow(unsafe_op_in_unsafe_fn)]
mod blob_upload_request_capnp {
    include!(concat!(env!("OUT_DIR"), "/blob_upload_request_capnp.rs"));
}
use blob_upload_request_capnp as wire;
include!(concat!(env!("OUT_DIR"), "/schema_digest.rs"));

pub const PROFILE: &str = "blob-upload-request-v1";
pub const VERSION: u16 = 1;
pub const MAX_REQUEST_BYTES: usize = 65536;
pub const MAX_RESPONSE_BYTES: usize = 4096;
/// The outer envelope must fit without widening any legacy 64 KiB call limit.
pub const MAX_EMBEDDED_FRAME_BYTES: usize = MAX_REQUEST_BYTES - 1024;
pub const MAX_EMBEDDED_RECEIPT_BYTES: usize = 1024;
pub const MAX_OBJECT_BYTES: u64 = morrow_blob_transfer_v1::MAX_OBJECT_BYTES;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid = 1,
    Contract = 2,
    Limit = 3,
    Correlation = 4,
    Buffer = 5,
    State = 6,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blob-upload-request-v1: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
fn invalid<T>(_: T) -> Error {
    Error::Invalid
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn id(bytes: &[u8]) -> Result<[u8; 32]> {
    let value = bytes.try_into().map_err(invalid)?;
    if value == [0; 32] {
        Err(Error::Invalid)
    } else {
        Ok(value)
    }
}
fn exact_layout<'a>(
    root: impl IntoInternalStructReader<'a>,
    data: u16,
    pointers: u16,
) -> Result<()> {
    let root = root.into_internal_struct_reader();
    if root.get_data_section_size() != u32::from(data) * 64
        || root.get_pointer_section_size() != pointers
    {
        return Err(Error::Contract);
    }
    Ok(())
}
fn bounded(bytes: &[u8], max: usize) -> Result<()> {
    if bytes.is_empty() || bytes.len() > max {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
fn range(original: &[u8], field: &[u8]) -> Result<Range<usize>> {
    if field.is_empty() {
        return Ok(0..0);
    }
    let start = field
        .as_ptr()
        .addr()
        .checked_sub(original.as_ptr().addr())
        .ok_or(Error::Invalid)?;
    let end = start.checked_add(field.len()).ok_or(Error::Invalid)?;
    if end > original.len() {
        return Err(Error::Invalid);
    }
    Ok(start..end)
}
fn encode(message: &Builder<capnp::message::HeapAllocator>, max: usize) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(message);
    bounded(&bytes, max)?;
    Ok(bytes)
}
fn blob(bytes: &[u8], max: usize) -> Result<FrameRef<'_>> {
    bounded(bytes, max)?;
    FrameRef::decode(bytes).map_err(|e| match e {
        morrow_blob_transfer_v1::Error::Contract => Error::Contract,
        morrow_blob_transfer_v1::Error::Limit => Error::Limit,
        _ => Error::Invalid,
    })
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Open = 1,
    Push = 2,
    Inspect = 3,
    Release = 4,
}
impl TryFrom<u16> for ActionKind {
    type Error = Error;
    fn try_from(value: u16) -> Result<Self> {
        match value {
            1 => Ok(Self::Open),
            2 => Ok(Self::Push),
            3 => Ok(Self::Inspect),
            4 => Ok(Self::Release),
            _ => Err(Error::Invalid),
        }
    }
}
#[derive(Clone, Copy)]
pub enum ActionRef<'a> {
    Open,
    Push(&'a [u8]),
    Inspect,
    Release,
}
impl<'a> ActionRef<'a> {
    pub fn kind(self) -> ActionKind {
        match self {
            Self::Open => ActionKind::Open,
            Self::Push(_) => ActionKind::Push,
            Self::Inspect => ActionKind::Inspect,
            Self::Release => ActionKind::Release,
        }
    }
    fn frame(self) -> &'a [u8] {
        match self {
            Self::Push(bytes) => bytes,
            _ => &[],
        }
    }
    pub fn validate(self) -> Result<()> {
        if let Self::Push(bytes) = self
            && matches!(
                blob(bytes, MAX_EMBEDDED_FRAME_BYTES)?.action,
                BlobAction::Receipt { .. }
            )
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
impl fmt::Debug for ActionRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadAction")
            .field("kind", &self.kind())
            .field("frame_bytes", &self.frame().len())
            .finish()
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct Request {
    wire: Vec<u8>,
    nomination: [u8; 32],
    nonce: [u8; 32],
    kind: ActionKind,
    frame: Range<usize>,
    digest: [u8; 32],
}
impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadRequest")
            .field("action", &self.action())
            .field("wire_bytes", &self.wire.len())
            .finish_non_exhaustive()
    }
}
impl Request {
    pub fn new(nomination: [u8; 32], nonce: [u8; 32], action: ActionRef<'_>) -> Result<Self> {
        id(&nomination)?;
        id(&nonce)?;
        action.validate()?;
        let mut message = Builder::new_default();
        {
            let mut root = message.init_root::<wire::request::Builder<'_>>();
            root.set_version(VERSION);
            root.set_action(action.kind() as u16);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_frame_schema_sha256(&morrow_blob_transfer_v1::SCHEMA_DIGEST);
            root.set_nomination_ref(&nomination);
            root.set_nonce(&nonce);
            root.set_frame(action.frame());
        }
        Self::decode(&encode(&message, MAX_REQUEST_BYTES)?)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        bounded(bytes, MAX_REQUEST_BYTES)?;
        let mut remaining = bytes;
        let message = serialize::read_message_from_flat_slice(
            &mut remaining,
            ReaderOptions {
                traversal_limit_in_words: Some(16384),
                nesting_limit: 8,
            },
        )
        .map_err(invalid)?;
        if !remaining.is_empty() {
            return Err(Error::Invalid);
        }
        let root = message
            .get_root::<wire::request::Reader<'_>>()
            .map_err(invalid)?;
        let size = <wire::request::Builder<'_> as HasStructSize>::STRUCT_SIZE;
        exact_layout(root, size.data, size.pointers)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != SCHEMA_DIGEST
            || root.get_frame_schema_sha256().map_err(invalid)?
                != morrow_blob_transfer_v1::SCHEMA_DIGEST
        {
            return Err(Error::Contract);
        }
        if root.get_reserved() != 0 {
            return Err(Error::Invalid);
        }
        let nomination = id(root.get_nomination_ref().map_err(invalid)?)?;
        let nonce = id(root.get_nonce().map_err(invalid)?)?;
        let kind = ActionKind::try_from(root.get_action())?;
        let frame = root.get_frame().map_err(invalid)?;
        if kind == ActionKind::Push {
            ActionRef::Push(frame).validate()?;
        } else if !frame.is_empty() {
            return Err(Error::Invalid);
        }
        Ok(Self {
            wire: bytes.to_vec(),
            nomination,
            nonce,
            kind,
            frame: range(bytes, frame)?,
            digest: digest(bytes),
        })
    }
    pub fn wire(&self) -> &[u8] {
        &self.wire
    }
    pub fn nomination_ref(&self) -> [u8; 32] {
        self.nomination
    }
    pub fn nonce(&self) -> [u8; 32] {
        self.nonce
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn action(&self) -> ActionRef<'_> {
        match self.kind {
            ActionKind::Open => ActionRef::Open,
            ActionKind::Push => ActionRef::Push(&self.wire[self.frame.clone()]),
            ActionKind::Inspect => ActionRef::Inspect,
            ActionKind::Release => ActionRef::Release,
        }
    }
}

/// Exact host-nominated object. It contains no Store address or grant.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    pub identity: Identity,
    pub byte_length: u64,
    pub sha256: [u8; 32],
}
impl Descriptor {
    pub fn validate(self) -> Result<()> {
        self.identity.validate().map_err(invalid)?;
        if self.byte_length > MAX_OBJECT_BYTES {
            return Err(Error::Limit);
        }
        if self.byte_length == 0 && self.sha256 != digest(&[]) {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
impl fmt::Debug for Descriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadDescriptor")
            .field("byte_length", &self.byte_length)
            .finish_non_exhaustive()
    }
}
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Ready = 1,
    Receiving = 2,
    Staged = 3,
    Released = 4,
    Absent = 5,
}
impl TryFrom<u16> for Phase {
    type Error = Error;
    fn try_from(value: u16) -> Result<Self> {
        match value {
            1 => Ok(Self::Ready),
            2 => Ok(Self::Receiving),
            3 => Ok(Self::Staged),
            4 => Ok(Self::Released),
            5 => Ok(Self::Absent),
            _ => Err(Error::Invalid),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub descriptor: Descriptor,
    pub phase: Phase,
    pub received_bytes: u64,
    pub next_sequence: u64,
}
impl State {
    pub fn validate(self) -> Result<()> {
        self.descriptor.validate()?;
        let valid = match self.phase {
            Phase::Ready => self.received_bytes == 0 && self.next_sequence == 1,
            Phase::Receiving => {
                self.received_bytes <= self.descriptor.byte_length
                    && (1..=4096).contains(&self.next_sequence)
                    && (self.next_sequence != 1 || self.received_bytes == 0)
            }
            Phase::Staged | Phase::Released => {
                self.received_bytes == self.descriptor.byte_length && self.next_sequence == 0
            }
            Phase::Absent => self.received_bytes == 0 && self.next_sequence == 0,
        };
        if valid { Ok(()) } else { Err(Error::Invalid) }
    }
}
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Denied = 1,
    Closed = 2,
    Busy = 3,
    Invalid = 4,
    Limit = 5,
    Unsupported = 6,
    OutcomeUnknown = 7,
}
impl TryFrom<u16> for Status {
    type Error = Error;
    fn try_from(value: u16) -> Result<Self> {
        match value {
            1 => Ok(Self::Denied),
            2 => Ok(Self::Closed),
            3 => Ok(Self::Busy),
            4 => Ok(Self::Invalid),
            5 => Ok(Self::Limit),
            6 => Ok(Self::Unsupported),
            7 => Ok(Self::OutcomeUnknown),
            _ => Err(Error::Invalid),
        }
    }
}
#[derive(Clone, Copy)]
pub enum ReplyRef<'a> {
    State(State),
    ChunkReceipt(&'a [u8]),
    Error(Status),
}
impl fmt::Debug for ReplyRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::State(s) => s.fmt(f),
            Self::ChunkReceipt(bytes) => f
                .debug_struct("ChunkReceipt")
                .field("wire_bytes", &bytes.len())
                .finish(),
            Self::Error(e) => f.debug_tuple("Error").field(e).finish(),
        }
    }
}
impl ReplyRef<'_> {
    pub fn validate_for(self, request: &Request) -> Result<()> {
        match self {
            Self::Error(_) => Ok(()),
            Self::State(state) => {
                state.validate()?;
                let allowed = match request.action() {
                    ActionRef::Open => {
                        matches!(state.phase, Phase::Ready | Phase::Staged | Phase::Released)
                    }
                    ActionRef::Inspect => true,
                    ActionRef::Release => matches!(state.phase, Phase::Released | Phase::Absent),
                    ActionRef::Push(bytes) => {
                        let frame = blob(bytes, MAX_EMBEDDED_FRAME_BYTES)?;
                        let (length, hash, expected) = match frame.action {
                            BlobAction::Descriptor {
                                total_length,
                                whole_sha256,
                            } => (total_length, whole_sha256, Phase::Receiving),
                            BlobAction::End {
                                total_length,
                                whole_sha256,
                            } => (total_length, whole_sha256, Phase::Staged),
                            _ => return Err(Error::Correlation),
                        };
                        frame.identity == state.descriptor.identity
                            && length == state.descriptor.byte_length
                            && hash == state.descriptor.sha256
                            && state.phase == expected
                            && (expected != Phase::Receiving
                                || (state.received_bytes == 0 && state.next_sequence == 1))
                    }
                };
                if allowed {
                    Ok(())
                } else {
                    Err(Error::Correlation)
                }
            }
            Self::ChunkReceipt(bytes) => {
                let ActionRef::Push(original) = request.action() else {
                    return Err(Error::Correlation);
                };
                let chunk = blob(original, MAX_EMBEDDED_FRAME_BYTES)?;
                let receipt = blob(bytes, MAX_EMBEDDED_RECEIPT_BYTES)?;
                let BlobAction::Chunk {
                    sequence,
                    offset,
                    payload,
                    chunk_sha256,
                } = chunk.action
                else {
                    return Err(Error::Correlation);
                };
                let BlobAction::Receipt {
                    request_digest,
                    sequence: r_sequence,
                    offset: r_offset,
                    length,
                    chunk_sha256: r_hash,
                    ..
                } = receipt.action
                else {
                    return Err(Error::Correlation);
                };
                if receipt.identity != chunk.identity
                    || request_digest != digest(original)
                    || sequence != r_sequence
                    || offset != r_offset
                    || payload.len() as u64 != length
                    || chunk_sha256 != r_hash
                {
                    return Err(Error::Correlation);
                }
                Ok(())
            }
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
enum Reply {
    State(State),
    ChunkReceipt(Range<usize>),
    Error(Status),
}
#[derive(Clone, PartialEq, Eq)]
pub struct Response {
    wire: Vec<u8>,
    reply: Reply,
}
impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadResponse")
            .field("reply", &self.reply())
            .finish_non_exhaustive()
    }
}
impl Response {
    pub fn new(request: &Request, reply: ReplyRef<'_>) -> Result<Self> {
        reply.validate_for(request)?;
        let mut message = Builder::new_default();
        {
            let mut root = message.init_root::<wire::response::Builder<'_>>();
            root.set_version(VERSION);
            root.set_action(request.kind as u16);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_frame_schema_sha256(&morrow_blob_transfer_v1::SCHEMA_DIGEST);
            root.set_nomination_ref(&request.nomination);
            root.set_nonce(&request.nonce);
            root.set_request_digest(&request.digest);
            match reply {
                ReplyRef::State(state) => {
                    root.set_kind(1);
                    root.set_phase(state.phase as u16);
                    root.set_transfer_epoch(&state.descriptor.identity.transfer_epoch);
                    root.set_object_ref(&state.descriptor.identity.object_ref);
                    root.set_operation_id(&state.descriptor.identity.operation_id);
                    root.set_whole_sha256(&state.descriptor.sha256);
                    root.set_byte_length(state.descriptor.byte_length);
                    root.set_received_bytes(state.received_bytes);
                    root.set_next_sequence(state.next_sequence);
                }
                ReplyRef::ChunkReceipt(bytes) => {
                    root.set_kind(2);
                    root.set_receipt(bytes);
                }
                ReplyRef::Error(status) => {
                    root.set_kind(3);
                    root.set_status(status as u16);
                }
            }
        }
        Self::decode_for(&encode(&message, MAX_RESPONSE_BYTES)?, request)
    }
    pub fn decode_for(bytes: &[u8], request: &Request) -> Result<Self> {
        bounded(bytes, MAX_RESPONSE_BYTES)?;
        let mut remaining = bytes;
        let message = serialize::read_message_from_flat_slice(
            &mut remaining,
            ReaderOptions {
                traversal_limit_in_words: Some(1024),
                nesting_limit: 8,
            },
        )
        .map_err(invalid)?;
        if !remaining.is_empty() {
            return Err(Error::Invalid);
        }
        let root = message
            .get_root::<wire::response::Reader<'_>>()
            .map_err(invalid)?;
        let size = <wire::response::Builder<'_> as HasStructSize>::STRUCT_SIZE;
        exact_layout(root, size.data, size.pointers)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != SCHEMA_DIGEST
            || root.get_frame_schema_sha256().map_err(invalid)?
                != morrow_blob_transfer_v1::SCHEMA_DIGEST
        {
            return Err(Error::Contract);
        }
        if root.get_reserved() != 0 || root.get_reserved2() != 0 {
            return Err(Error::Invalid);
        }
        if root.get_action() != request.kind as u16
            || root.get_nomination_ref().map_err(invalid)? != request.nomination
            || root.get_nonce().map_err(invalid)? != request.nonce
            || root.get_request_digest().map_err(invalid)? != request.digest
        {
            return Err(Error::Correlation);
        }
        let epoch = root.get_transfer_epoch().map_err(invalid)?;
        let object = root.get_object_ref().map_err(invalid)?;
        let operation = root.get_operation_id().map_err(invalid)?;
        let hash = root.get_whole_sha256().map_err(invalid)?;
        let receipt = root.get_receipt().map_err(invalid)?;
        let reply = if root.get_kind() == 1 {
            if root.get_status() != 0 || !receipt.is_empty() {
                return Err(Error::Invalid);
            }
            Reply::State(State {
                descriptor: Descriptor {
                    identity: Identity {
                        transfer_epoch: id(epoch)?,
                        object_ref: id(object)?,
                        operation_id: id(operation)?,
                    },
                    byte_length: root.get_byte_length(),
                    sha256: hash.try_into().map_err(invalid)?,
                },
                phase: Phase::try_from(root.get_phase())?,
                received_bytes: root.get_received_bytes(),
                next_sequence: root.get_next_sequence(),
            })
        } else {
            if !epoch.is_empty()
                || !object.is_empty()
                || !operation.is_empty()
                || !hash.is_empty()
                || root.get_byte_length() != 0
                || root.get_received_bytes() != 0
                || root.get_next_sequence() != 0
                || root.get_phase() != 0
            {
                return Err(Error::Invalid);
            }
            match root.get_kind() {
                2 if root.get_status() == 0 => Reply::ChunkReceipt(range(bytes, receipt)?),
                3 if receipt.is_empty() => Reply::Error(Status::try_from(root.get_status())?),
                _ => return Err(Error::Invalid),
            }
        };
        let response = Self {
            wire: bytes.to_vec(),
            reply,
        };
        response.reply().validate_for(request)?;
        Ok(response)
    }
    pub fn wire(&self) -> &[u8] {
        &self.wire
    }
    pub fn reply(&self) -> ReplyRef<'_> {
        match &self.reply {
            Reply::State(state) => ReplyRef::State(*state),
            Reply::ChunkReceipt(range) => ReplyRef::ChunkReceipt(&self.wire[range.clone()]),
            Reply::Error(status) => ReplyRef::Error(*status),
        }
    }
}

#[cfg(test)]
mod tests;
