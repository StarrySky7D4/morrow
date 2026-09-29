//! Bounded, independent guest mutation wire profile. A valid frame is not an
//! authorization: the runtime must bind the opaque reference to its live owner.
use crate::{Error, Result, mutation_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};

pub use wire::{Effect, Kind, Phase, Status};

pub const VERSION: u16 = wire::VERSION;
pub const MAX_FRAME_BYTES: usize = wire::MAX_FRAME_BYTES as usize;
pub const MAX_CHUNK_BYTES: usize = wire::MAX_CHUNK_BYTES as usize;
pub const MAX_CONTENT_BYTES: u64 = wire::MAX_CONTENT_BYTES;
pub const MAX_OPERATION_BYTES: usize = wire::MAX_OPERATION_BYTES as usize;
pub const MAX_DEADLINE_MS: u32 = wire::MAX_DEADLINE_MS;

pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/mutation.capnp"))
}

fn invalid<T>(_: T) -> Error {
    Error::Invalid("mutation frame")
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

fn token(value: capnp::Result<capnp::data::Reader<'_>>) -> Result<[u8; 32]> {
    let bytes = value.map_err(invalid)?;
    let result: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Invalid("mutation token length"))?;
    if result == [0; 32] {
        return Err(Error::Invalid("mutation zero token"));
    }
    Ok(result)
}

fn operation(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    let text = value.map_err(invalid)?;
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_OPERATION_BYTES {
        return Err(Error::Limit);
    }
    let value = std::str::from_utf8(bytes).map_err(invalid)?;
    crate::identity(value)?;
    Ok(value.to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Create {
        content_length: u64,
        content_sha256: [u8; 32],
    },
    Delete,
    Chunk {
        offset: u64,
        bytes: Vec<u8>,
    },
    Commit,
    Execute,
    Query,
    CancelPlan,
    Release,
}

impl Action {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Create { .. } => Kind::PrepareCreate,
            Self::Delete => Kind::PrepareDelete,
            Self::Chunk { .. } => Kind::Chunk,
            Self::Commit => Kind::Commit,
            Self::Execute => Kind::Execute,
            Self::Query => Kind::Query,
            Self::CancelPlan => Kind::CancelPlan,
            Self::Release => Kind::Release,
        }
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::Create {
                content_length,
                content_sha256,
            } => {
                if *content_length > MAX_CONTENT_BYTES {
                    return Err(Error::Limit);
                }
                if *content_sha256 == [0; 32]
                    || (*content_length == 0 && *content_sha256 != Sha256::digest([]).as_slice())
                {
                    return Err(Error::Invalid("mutation content hash"));
                }
            }
            Self::Chunk { offset, bytes } => {
                if bytes.is_empty()
                    || bytes.len() > MAX_CHUNK_BYTES
                    || offset
                        .checked_add(bytes.len() as u64)
                        .is_none_or(|end| end > MAX_CONTENT_BYTES)
                {
                    return Err(Error::Limit);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub call_id: u64,
    pub reference: [u8; 32],
    pub submission: [u8; 32],
    pub operation_id: String,
    pub deadline_ms: u32,
    pub action: Action,
}

impl Request {
    pub fn validate(&self) -> Result<()> {
        if self.call_id == 0 || self.reference == [0; 32] || self.submission == [0; 32] {
            return Err(Error::Invalid("mutation identity"));
        }
        if self.operation_id.is_empty() || self.operation_id.len() > MAX_OPERATION_BYTES {
            return Err(Error::Limit);
        }
        crate::identity(&self.operation_id)?;
        if !(1..=MAX_DEADLINE_MS).contains(&self.deadline_ms) {
            return Err(Error::Limit);
        }
        self.action.validate()
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::request::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(self.call_id);
        root.set_reference(&self.reference);
        root.set_submission(&self.submission);
        root.set_operation_id(&self.operation_id);
        root.set_deadline_ms(self.deadline_ms);
        match &self.action {
            Action::Create {
                content_length,
                content_sha256,
            } => {
                let mut create = root.init_prepare_create();
                create.set_content_length(*content_length);
                create.set_content_sha256(content_sha256);
            }
            Action::Delete => root.set_prepare_delete(()),
            Action::Chunk { offset, bytes } => {
                let mut chunk = root.init_chunk();
                chunk.set_offset(*offset);
                chunk.set_bytes(bytes);
            }
            Action::Commit => root.set_commit(()),
            Action::Execute => root.set_execute(()),
            Action::Query => root.set_query(()),
            Action::CancelPlan => root.set_cancel_plan(()),
            Action::Release => root.set_release(()),
        }
        finish(&message)
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
        let call_id = root.get_call_id();
        let reference = token(root.get_reference())?;
        let submission = token(root.get_submission())?;
        let operation_id = operation(root.get_operation_id())?;
        let deadline_ms = root.get_deadline_ms();
        if call_id == 0 || !(1..=MAX_DEADLINE_MS).contains(&deadline_ms) {
            return Err(Error::Invalid("mutation identity or deadline"));
        }
        let action = match root.which().map_err(invalid)? {
            wire::request::PrepareCreate(value) => {
                let create = value.map_err(invalid)?;
                Action::Create {
                    content_length: create.get_content_length(),
                    content_sha256: token(create.get_content_sha256())?,
                }
            }
            wire::request::PrepareDelete(()) => Action::Delete,
            wire::request::Chunk(value) => {
                let chunk = value.map_err(invalid)?;
                let bytes = chunk.get_bytes().map_err(invalid)?;
                if bytes.is_empty() || bytes.len() > MAX_CHUNK_BYTES {
                    return Err(Error::Limit);
                }
                let offset = chunk.get_offset();
                if offset
                    .checked_add(bytes.len() as u64)
                    .is_none_or(|end| end > MAX_CONTENT_BYTES)
                {
                    return Err(Error::Limit);
                }
                Action::Chunk {
                    offset,
                    bytes: bytes.to_vec(),
                }
            }
            wire::request::Commit(()) => Action::Commit,
            wire::request::Execute(()) => Action::Execute,
            wire::request::Query(()) => Action::Query,
            wire::request::CancelPlan(()) => Action::CancelPlan,
            wire::request::Release(()) => Action::Release,
        };
        let request = Self {
            call_id,
            reference,
            submission,
            operation_id,
            deadline_ms,
            action,
        };
        request.validate()?;
        Ok(request)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub call_id: u64,
    pub reference: [u8; 32],
    pub submission: [u8; 32],
    pub operation_id: String,
    pub kind: Kind,
    pub status: Status,
    pub phase: Phase,
    pub effect: Effect,
    pub staged_bytes: u64,
    pub durable_content: bool,
}

impl Response {
    pub fn for_request(request: &Request, status: Status, phase: Phase, effect: Effect) -> Self {
        Self {
            call_id: request.call_id,
            reference: request.reference,
            submission: request.submission,
            operation_id: request.operation_id.clone(),
            kind: request.action.kind(),
            status,
            phase,
            effect,
            staged_bytes: 0,
            durable_content: false,
        }
    }

    pub fn validate(&self, request: &Request) -> Result<()> {
        request.validate()?;
        if self.call_id != request.call_id
            || self.reference != request.reference
            || self.submission != request.submission
            || self.operation_id != request.operation_id
            || self.kind != request.action.kind()
        {
            return Err(Error::Integrity);
        }
        if self.status == Status::Invalid || self.kind == Kind::Invalid {
            return Err(Error::Invalid("mutation reply enum"));
        }
        if self.staged_bytes > MAX_CONTENT_BYTES {
            return Err(Error::Limit);
        }
        let no_outcome = self.effect == Effect::Unspecified;
        let no_metadata = self.staged_bytes == 0 && !self.durable_content;
        if self.status == Status::OutcomeUnknown {
            if !matches!(self.kind, Kind::Execute | Kind::Query)
                || self.phase != Phase::OutcomeUnknown
                || !no_outcome
                || !no_metadata
            {
                return Err(Error::Invalid("mutation unknown reply"));
            }
            return Ok(());
        }
        if self.status != Status::Completed {
            if self.phase != Phase::None || !no_outcome || !no_metadata {
                return Err(Error::Invalid("mutation failed reply"));
            }
            return Ok(());
        }
        let valid = match self.kind {
            Kind::PrepareCreate | Kind::PrepareDelete => {
                self.phase == Phase::Prepared && no_outcome && no_metadata
            }
            Kind::Chunk => {
                let expected = match &request.action {
                    Action::Chunk { offset, bytes } => offset + bytes.len() as u64,
                    _ => unreachable!(),
                };
                self.phase == Phase::Prepared
                    && no_outcome
                    && !self.durable_content
                    && self.staged_bytes == expected
            }
            Kind::Commit => self.phase == Phase::Prepared && no_outcome && self.durable_content,
            Kind::Execute => {
                self.phase == Phase::Observed
                    && matches!(self.effect, Effect::OsSucceeded | Effect::OsRejected)
            }
            Kind::Query => match self.phase {
                Phase::Absent | Phase::CancelledBeforeDispatch => no_outcome && no_metadata,
                Phase::Prepared | Phase::OutcomeUnknown => no_outcome,
                Phase::Observed => matches!(self.effect, Effect::OsSucceeded | Effect::OsRejected),
                Phase::None => false,
            },
            Kind::CancelPlan => {
                self.phase == Phase::CancelledBeforeDispatch && no_outcome && no_metadata
            }
            Kind::Release => self.phase == Phase::None && no_outcome && no_metadata,
            Kind::Invalid => false,
        };
        if !valid {
            return Err(Error::Invalid("mutation completed reply"));
        }
        Ok(())
    }

    pub fn encode(&self, request: &Request) -> Result<Vec<u8>> {
        self.validate(request)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::response::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_call_id(self.call_id);
        root.set_reference(&self.reference);
        root.set_submission(&self.submission);
        root.set_operation_id(&self.operation_id);
        root.set_kind(self.kind);
        root.set_status(self.status);
        root.set_phase(self.phase);
        root.set_effect(self.effect);
        root.set_staged_bytes(self.staged_bytes);
        root.set_durable_content(self.durable_content);
        finish(&message)
    }

    pub fn decode(request: &Request, bytes: &[u8]) -> Result<Self> {
        request.validate()?;
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
        // Read and compare borrowed fields before allocating the operation ID.
        let operation_id = root.get_operation_id().map_err(invalid)?;
        if operation_id.as_bytes() != request.operation_id.as_bytes()
            || root.get_call_id() != request.call_id
            || root.get_reference().map_err(invalid)? != request.reference
            || root.get_submission().map_err(invalid)? != request.submission
            || root.get_kind().map_err(invalid)? != request.action.kind()
        {
            return Err(Error::Integrity);
        }
        let response = Self {
            call_id: root.get_call_id(),
            reference: request.reference,
            submission: request.submission,
            operation_id: request.operation_id.clone(),
            kind: root.get_kind().map_err(invalid)?,
            status: root.get_status().map_err(invalid)?,
            phase: root.get_phase().map_err(invalid)?,
            effect: root.get_effect().map_err(invalid)?,
            staged_bytes: root.get_staged_bytes(),
            durable_content: root.get_durable_content(),
        };
        response.validate(request)?;
        Ok(response)
    }
}
