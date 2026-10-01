//! Public bounded channel wire contract. Decoding is never source approval.
//! ACK proves consumption/credit release; Accepted proves send admission only.
use crate::{Error, Result, channel_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.channel.v1.rs"));
}
pub const VERSION: u32 = 1;
pub const FEATURE: &str = "channel-v1";
pub const MAX_WIRE_BYTES: usize = 128 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_CURSOR_BYTES: usize = 256;
pub const MAX_CHANNELS: u32 = 8;
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_MESSAGES: u64 = 1_000_000;
pub const MAX_REQUESTS: u64 = 1_000_000;
pub const MAX_DURATION_MS: u64 = 3_600_000;
pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/channel.capnp"))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    ByteStream,
    Events,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub max_channels: u32,
    pub max_frame_bytes: u32,
    pub max_bytes: u64,
    pub max_messages: u64,
    pub max_requests: u64,
    pub max_duration_ms: u64,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_channels: MAX_CHANNELS,
            max_frame_bytes: MAX_PAYLOAD_BYTES as u32,
            max_bytes: MAX_BYTES,
            max_messages: MAX_MESSAGES,
            max_requests: MAX_REQUESTS,
            max_duration_ms: MAX_DURATION_MS,
        }
    }
}
impl Budget {
    pub fn validate(&self) -> Result<()> {
        if self.max_channels == 0
            || self.max_channels > MAX_CHANNELS
            || self.max_frame_bytes == 0
            || self.max_frame_bytes as usize > MAX_PAYLOAD_BYTES
            || self.max_bytes < u64::from(self.max_frame_bytes)
            || self.max_bytes > MAX_BYTES
            || self.max_messages == 0
            || self.max_messages > MAX_MESSAGES
            || self.max_requests == 0
            || self.max_requests > MAX_REQUESTS
            || self.max_duration_ms == 0
            || self.max_duration_ms > MAX_DURATION_MS
        {
            return Err(Error::Limit);
        }
        Ok(())
    }
    pub fn to_proto(&self) -> proto::ChannelBudget {
        proto::ChannelBudget {
            max_channels: self.max_channels,
            max_frame_bytes: self.max_frame_bytes,
            max_bytes: self.max_bytes,
            max_messages: self.max_messages,
            max_requests: self.max_requests,
            max_duration_ms: self.max_duration_ms,
        }
    }
    pub fn from_proto(value: &proto::ChannelBudget) -> Result<Self> {
        let budget = Self {
            max_channels: value.max_channels,
            max_frame_bytes: value.max_frame_bytes,
            max_bytes: value.max_bytes,
            max_messages: value.max_messages,
            max_requests: value.max_requests,
            max_duration_ms: value.max_duration_ms,
        };
        budget.validate()?;
        Ok(budget)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub reference: [u8; 32],
    pub source_epoch: [u8; 32],
    pub kind: Kind,
    pub budget: Budget,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory {
    pub scope_sha256: [u8; 32],
    pub channels: Vec<Endpoint>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub sequence: u64,
    pub source_epoch: [u8; 32],
    pub bytes: Vec<u8>,
    pub cursor: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Receive {
        last_acked: u64,
        credit_bytes: u32,
    },
    Ack {
        sequence: u64,
        frame_sha256: [u8; 32],
        cursor: Vec<u8>,
    },
    Send {
        sequence: u64,
        bytes: Vec<u8>,
    },
    Close,
    Query,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub call_id: [u8; 32],
    pub reference: [u8; 32],
    pub source_epoch: [u8; 32],
    pub action: Action,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ready,
    Frame,
    Acked,
    Accepted,
    Idle,
    Closed,
    ClosingUnconfirmed,
    Revoked,
    Expired,
    Limit,
    Invalid,
    Unknown,
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub call_id: [u8; 32],
    pub request_sha256: [u8; 32],
    pub reference: [u8; 32],
    pub source_epoch: [u8; 32],
    pub status: Status,
    pub frame: Option<Frame>,
    pub last_acked: u64,
    pub accepted_sequence: u64,
    pub resource_reclaimed: bool,
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid("channel wire")
}
fn id(value: &[u8; 32]) -> Result<()> {
    if *value == [0; 32] {
        Err(invalid(()))
    } else {
        Ok(())
    }
}
fn fixed(bytes: capnp::Result<&[u8]>) -> Result<[u8; 32]> {
    let value = bytes.map_err(invalid)?.try_into().map_err(invalid)?;
    id(&value)?;
    Ok(value)
}
fn bounded(bytes: capnp::Result<&[u8]>, max: usize) -> Result<Vec<u8>> {
    let bytes = bytes.map_err(invalid)?;
    if bytes.len() > max {
        return Err(Error::Limit);
    }
    Ok(bytes.to_vec())
}
fn message(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::OwnedSegments>> {
    if bytes.len() > MAX_WIRE_BYTES {
        return Err(Error::Limit);
    }
    let mut remaining = bytes;
    let message = serialize::read_message(
        &mut remaining,
        ReaderOptions {
            traversal_limit_in_words: Some(2 * MAX_WIRE_BYTES / 8),
            nesting_limit: 12,
        },
    )
    .map_err(invalid)?;
    if !remaining.is_empty() {
        return Err(invalid(()));
    }
    Ok(message)
}
fn output(builder: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(builder);
    if bytes.len() > MAX_WIRE_BYTES {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
fn builder() -> Builder<capnp::message::HeapAllocator> {
    // One segment gives C/C++ the same canonical allocation order at every permitted size.
    Builder::new(
        capnp::message::HeapAllocator::new().first_segment_words((MAX_WIRE_BYTES / 8) as u32),
    )
}
fn version(version: u32, digest: capnp::Result<&[u8]>) -> Result<()> {
    if version != VERSION || digest.map_err(invalid)? != schema_digest() {
        return Err(Error::UnsupportedVersion);
    }
    Ok(())
}
fn write_budget(mut root: wire::budget::Builder<'_>, value: &Budget) {
    root.set_max_channels(value.max_channels);
    root.set_max_frame_bytes(value.max_frame_bytes);
    root.set_max_bytes(value.max_bytes);
    root.set_max_messages(value.max_messages);
    root.set_max_requests(value.max_requests);
    root.set_max_duration_ms(value.max_duration_ms);
}
fn read_budget(root: wire::budget::Reader<'_>) -> Result<Budget> {
    let value = Budget {
        max_channels: root.get_max_channels(),
        max_frame_bytes: root.get_max_frame_bytes(),
        max_bytes: root.get_max_bytes(),
        max_messages: root.get_max_messages(),
        max_requests: root.get_max_requests(),
        max_duration_ms: root.get_max_duration_ms(),
    };
    value.validate()?;
    Ok(value)
}
fn write_frame(mut root: wire::frame::Builder<'_>, value: &Frame) {
    root.set_sequence(value.sequence);
    root.set_source_epoch(&value.source_epoch);
    root.set_bytes(&value.bytes);
    root.set_cursor(&value.cursor);
}
fn read_frame(root: wire::frame::Reader<'_>) -> Result<Frame> {
    let value = Frame {
        sequence: root.get_sequence(),
        source_epoch: fixed(root.get_source_epoch())?,
        bytes: bounded(root.get_bytes(), MAX_PAYLOAD_BYTES)?,
        cursor: bounded(root.get_cursor(), MAX_CURSOR_BYTES)?,
    };
    value.validate()?;
    Ok(value)
}
impl Frame {
    pub fn validate(&self) -> Result<()> {
        id(&self.source_epoch)?;
        if self.sequence == 0 {
            return Err(invalid(()));
        }
        if self.bytes.len() > MAX_PAYLOAD_BYTES || self.cursor.len() > MAX_CURSOR_BYTES {
            return Err(Error::Limit);
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut builder = builder();
        write_frame(builder.init_root::<wire::frame::Builder<'_>>(), self);
        output(&builder)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = message(bytes)?;
        let value = read_frame(
            message
                .get_root::<wire::frame::Reader<'_>>()
                .map_err(invalid)?,
        )?;
        if value.encode()? != bytes {
            return Err(invalid(()));
        }
        Ok(value)
    }
    pub fn digest(&self) -> Result<[u8; 32]> {
        Ok(Sha256::digest(self.encode()?).into())
    }
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        id(&self.call_id)?;
        id(&self.reference)?;
        id(&self.source_epoch)?;
        match &self.action {
            Action::Receive { credit_bytes, .. }
                if *credit_bytes == 0 || *credit_bytes as usize > MAX_PAYLOAD_BYTES =>
            {
                Err(Error::Limit)
            }
            Action::Ack {
                sequence,
                frame_sha256,
                cursor,
            } => {
                id(frame_sha256)?;
                if *sequence == 0 {
                    return Err(invalid(()));
                }
                if cursor.len() > MAX_CURSOR_BYTES {
                    return Err(Error::Limit);
                }
                Ok(())
            }
            Action::Send { sequence, bytes } => {
                if *sequence == 0 {
                    return Err(invalid(()));
                }
                if bytes.len() > MAX_PAYLOAD_BYTES {
                    return Err(Error::Limit);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut builder = builder();
        let mut root = builder.init_root::<wire::request::Builder<'_>>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(&self.call_id);
        root.set_reference(&self.reference);
        root.set_source_epoch(&self.source_epoch);
        match &self.action {
            Action::Receive {
                last_acked,
                credit_bytes,
            } => {
                let mut action = root.init_receive();
                action.set_last_acked(*last_acked);
                action.set_credit_bytes(*credit_bytes);
            }
            Action::Ack {
                sequence,
                frame_sha256,
                cursor,
            } => {
                let mut action = root.init_ack();
                action.set_sequence(*sequence);
                action.set_frame_sha256(frame_sha256);
                action.set_cursor(cursor);
            }
            Action::Send { sequence, bytes } => {
                let mut action = root.init_send();
                action.set_sequence(*sequence);
                action.set_bytes(bytes);
            }
            Action::Close => root.set_close(()),
            Action::Query => root.set_query(()),
        }
        output(&builder)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = message(bytes)?;
        let root = message
            .get_root::<wire::request::Reader<'_>>()
            .map_err(invalid)?;
        version(root.get_version(), root.get_schema_sha256())?;
        let action = match root.which().map_err(invalid)? {
            wire::request::Receive(v) => {
                let v = v.map_err(invalid)?;
                Action::Receive {
                    last_acked: v.get_last_acked(),
                    credit_bytes: v.get_credit_bytes(),
                }
            }
            wire::request::Ack(v) => {
                let v = v.map_err(invalid)?;
                Action::Ack {
                    sequence: v.get_sequence(),
                    frame_sha256: fixed(v.get_frame_sha256())?,
                    cursor: bounded(v.get_cursor(), MAX_CURSOR_BYTES)?,
                }
            }
            wire::request::Send(v) => {
                let v = v.map_err(invalid)?;
                Action::Send {
                    sequence: v.get_sequence(),
                    bytes: bounded(v.get_bytes(), MAX_PAYLOAD_BYTES)?,
                }
            }
            wire::request::Close(()) => Action::Close,
            wire::request::Query(()) => Action::Query,
        };
        let value = Self {
            call_id: fixed(root.get_call_id())?,
            reference: fixed(root.get_reference())?,
            source_epoch: fixed(root.get_source_epoch())?,
            action,
        };
        if value.encode()? != bytes {
            return Err(invalid(()));
        }
        Ok(value)
    }
    pub fn digest(&self) -> Result<[u8; 32]> {
        Ok(Sha256::digest(self.encode()?).into())
    }
}
fn write_status(status: Status) -> wire::Status {
    match status {
        Status::Ready => wire::Status::Ready,
        Status::Frame => wire::Status::Frame,
        Status::Acked => wire::Status::Acked,
        Status::Accepted => wire::Status::Accepted,
        Status::Idle => wire::Status::Idle,
        Status::Closed => wire::Status::Closed,
        Status::ClosingUnconfirmed => wire::Status::ClosingUnconfirmed,
        Status::Revoked => wire::Status::Revoked,
        Status::Expired => wire::Status::Expired,
        Status::Limit => wire::Status::Limit,
        Status::Invalid => wire::Status::Invalid,
        Status::Unknown => wire::Status::Unknown,
        Status::Unsupported => wire::Status::Unsupported,
    }
}
fn read_status(status: wire::Status) -> Status {
    match status {
        wire::Status::Ready => Status::Ready,
        wire::Status::Frame => Status::Frame,
        wire::Status::Acked => Status::Acked,
        wire::Status::Accepted => Status::Accepted,
        wire::Status::Idle => Status::Idle,
        wire::Status::Closed => Status::Closed,
        wire::Status::ClosingUnconfirmed => Status::ClosingUnconfirmed,
        wire::Status::Revoked => Status::Revoked,
        wire::Status::Expired => Status::Expired,
        wire::Status::Limit => Status::Limit,
        wire::Status::Invalid => Status::Invalid,
        wire::Status::Unknown => Status::Unknown,
        wire::Status::Unsupported => Status::Unsupported,
    }
}
impl Response {
    pub fn validate(&self) -> Result<()> {
        id(&self.call_id)?;
        id(&self.request_sha256)?;
        id(&self.reference)?;
        id(&self.source_epoch)?;
        if (self.status == Status::Frame) != self.frame.is_some()
            || (self.status == Status::Accepted) != (self.accepted_sequence != 0)
            || self.resource_reclaimed
                && !matches!(
                    self.status,
                    Status::Closed | Status::Revoked | Status::Expired | Status::Unknown
                )
        {
            return Err(invalid(()));
        }
        if let Some(frame) = &self.frame {
            frame.validate()?;
            if frame.source_epoch != self.source_epoch || frame.sequence <= self.last_acked {
                return Err(invalid(()));
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut builder = builder();
        let mut root = builder.init_root::<wire::response::Builder<'_>>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(&self.call_id);
        root.set_request_sha256(&self.request_sha256);
        root.set_reference(&self.reference);
        root.set_source_epoch(&self.source_epoch);
        root.set_status(write_status(self.status));
        root.set_last_acked(self.last_acked);
        root.set_accepted_sequence(self.accepted_sequence);
        root.set_resource_reclaimed(self.resource_reclaimed);
        if let Some(frame) = &self.frame {
            write_frame(root.init_frame(), frame);
        }
        output(&builder)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = message(bytes)?;
        let root = message
            .get_root::<wire::response::Reader<'_>>()
            .map_err(invalid)?;
        version(root.get_version(), root.get_schema_sha256())?;
        let value = Self {
            call_id: fixed(root.get_call_id())?,
            request_sha256: fixed(root.get_request_sha256())?,
            reference: fixed(root.get_reference())?,
            source_epoch: fixed(root.get_source_epoch())?,
            status: read_status(root.get_status().map_err(invalid)?),
            frame: if root.has_frame() {
                Some(read_frame(root.get_frame().map_err(invalid)?)?)
            } else {
                None
            },
            last_acked: root.get_last_acked(),
            accepted_sequence: root.get_accepted_sequence(),
            resource_reclaimed: root.get_resource_reclaimed(),
        };
        if value.encode()? != bytes {
            return Err(invalid(()));
        }
        Ok(value)
    }
    pub fn validate_for(&self, request: &Request) -> Result<()> {
        self.validate()?;
        if self.call_id != request.call_id
            || self.reference != request.reference
            || self.source_epoch != request.source_epoch
            || self.request_sha256 != request.digest()?
        {
            return Err(Error::Integrity);
        }
        match (&request.action, self.status) {
            (
                Action::Receive {
                    last_acked,
                    credit_bytes,
                },
                Status::Frame,
            ) => {
                let frame = self.frame.as_ref().ok_or(Error::Integrity)?;
                if self.last_acked != *last_acked
                    || frame.sequence != last_acked.checked_add(1).ok_or(Error::Limit)?
                    || frame.bytes.len() > *credit_bytes as usize
                {
                    return Err(Error::Integrity);
                }
            }
            (Action::Ack { sequence, .. }, Status::Acked) if self.last_acked == *sequence => {}
            (Action::Send { sequence, .. }, Status::Accepted)
                if self.accepted_sequence == *sequence => {}
            (
                Action::Query,
                Status::Ready | Status::Idle | Status::Frame | Status::Acked | Status::Accepted,
            ) => {}
            (Action::Receive { .. }, Status::Idle) => {}
            (
                _,
                Status::Closed
                | Status::ClosingUnconfirmed
                | Status::Revoked
                | Status::Expired
                | Status::Limit
                | Status::Invalid
                | Status::Unknown
                | Status::Unsupported,
            ) => {}
            _ => return Err(Error::Integrity),
        }
        Ok(())
    }
}
impl Directory {
    pub fn encode(&self) -> Result<Vec<u8>> {
        id(&self.scope_sha256)?;
        if self.channels.is_empty() || self.channels.len() > MAX_CHANNELS as usize {
            return Err(Error::Limit);
        }
        let mut seen = BTreeSet::new();
        let mut builder = builder();
        let mut root = builder.init_root::<wire::directory::Builder<'_>>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_scope_sha256(&self.scope_sha256);
        let mut channels = root.init_channels(self.channels.len() as u32);
        for (i, endpoint) in self.channels.iter().enumerate() {
            id(&endpoint.reference)?;
            id(&endpoint.source_epoch)?;
            endpoint.budget.validate()?;
            if !seen.insert(endpoint.reference) {
                return Err(invalid(()));
            }
            let mut item = channels.reborrow().get(i as u32);
            item.set_reference(&endpoint.reference);
            item.set_source_epoch(&endpoint.source_epoch);
            item.set_kind(match endpoint.kind {
                Kind::ByteStream => wire::Kind::ByteStream,
                Kind::Events => wire::Kind::Events,
            });
            write_budget(item.init_budget(), &endpoint.budget);
        }
        output(&builder)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = message(bytes)?;
        let root = message
            .get_root::<wire::directory::Reader<'_>>()
            .map_err(invalid)?;
        version(root.get_version(), root.get_schema_sha256())?;
        let entries = root.get_channels().map_err(invalid)?;
        if entries.len() > MAX_CHANNELS {
            return Err(Error::Limit);
        }
        let mut channels = Vec::with_capacity(entries.len() as usize);
        for item in entries {
            channels.push(Endpoint {
                reference: fixed(item.get_reference())?,
                source_epoch: fixed(item.get_source_epoch())?,
                kind: match item.get_kind().map_err(invalid)? {
                    wire::Kind::ByteStream => Kind::ByteStream,
                    wire::Kind::Events => Kind::Events,
                },
                budget: read_budget(item.get_budget().map_err(invalid)?)?,
            });
        }
        let value = Self {
            scope_sha256: fixed(root.get_scope_sha256())?,
            channels,
        };
        if value.encode()? != bytes {
            return Err(invalid(()));
        }
        Ok(value)
    }
}
/// Convenience metadata only. Package admission still validates it; this grants nothing.
pub fn declaration(handlers: Vec<String>, kinds: Vec<Kind>) -> proto::ChannelDeclaration {
    proto::ChannelDeclaration {
        schema_version: VERSION,
        channel_version: VERSION,
        channel_schema_sha256: schema_digest().to_vec(),
        handlers,
        kinds: kinds
            .into_iter()
            .map(|kind| match kind {
                Kind::ByteStream => 1,
                Kind::Events => 2,
            })
            .collect(),
        budget: Some(Budget::default().to_proto()),
    }
}
pub(crate) fn validate_declaration(value: &proto::ChannelDeclaration) -> Result<()> {
    if value.schema_version != VERSION
        || value.channel_version != VERSION
        || value.channel_schema_sha256 != schema_digest()
    {
        return Err(Error::UnsupportedVersion);
    }
    if value.handlers.is_empty()
        || value.handlers.len() > 16
        || value.kinds.is_empty()
        || value.kinds.len() > 2
    {
        return Err(Error::Limit);
    }
    let mut handlers = BTreeSet::new();
    for handler in &value.handlers {
        crate::identity(handler)?;
        if !handlers.insert(handler) {
            return Err(Error::Invalid("duplicate channel handler"));
        }
    }
    let mut kinds = BTreeSet::new();
    for kind in &value.kinds {
        if !matches!(kind, 1 | 2) || !kinds.insert(*kind) {
            return Err(Error::Invalid("channel kind"));
        }
    }
    Budget::from_proto(
        value
            .budget
            .as_ref()
            .ok_or(Error::Invalid("missing channel budget"))?,
    )?;
    Ok(())
}
pub(crate) fn validate_manifest_wire(
    mut manifest: &[u8],
    value: Option<&proto::ChannelDeclaration>,
) -> Result<()> {
    use prost::{
        Message,
        encoding::{
            DecodeContext, WireType, decode_key, decode_varint, encode_key, encode_varint,
            skip_field,
        },
    };
    let mut seen = false;
    while !manifest.is_empty() {
        let original = manifest;
        let (number, wire) =
            decode_key(&mut manifest).map_err(|_| Error::Invalid("manifest key"))?;
        if number != 21 {
            skip_field(wire, number, &mut manifest, DecodeContext::default())
                .map_err(|_| Error::Invalid("manifest field"))?;
            continue;
        }
        if seen || wire != WireType::LengthDelimited {
            return Err(Error::Invalid("duplicate or malformed channel declaration"));
        }
        seen = true;
        let length = decode_varint(&mut manifest)
            .map_err(|_| Error::Invalid("channel declaration length"))?;
        if length > manifest.len() as u64 {
            return Err(Error::Invalid("channel declaration length"));
        }
        let (bytes, rest) = manifest.split_at(length as usize);
        manifest = rest;
        let mut canonical_field = Vec::new();
        encode_key(21, WireType::LengthDelimited, &mut canonical_field);
        encode_varint(length, &mut canonical_field);
        canonical_field.extend_from_slice(bytes);
        if canonical_field != original[..original.len() - rest.len()] {
            return Err(Error::Invalid("nonminimal channel declaration field"));
        }
        if value
            .ok_or(Error::Invalid("missing channel declaration"))?
            .encode_to_vec()
            != bytes
        {
            return Err(Error::Invalid("noncanonical channel declaration"));
        }
    }
    if seen != value.is_some() {
        return Err(Error::Invalid("channel declaration presence"));
    }
    Ok(())
}
