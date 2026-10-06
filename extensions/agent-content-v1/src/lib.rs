//! Independent, bounded content requests. Decoding never issues a content grant.
//! Replies retain current Core projections; proposals require trusted host approval.
#![deny(unsafe_code)]

use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
    traits::{HasStructSize, IntoInternalStructReader},
};
use morrow_core::{response, runtime};
use sha2::{Digest, Sha256};

#[allow(clippy::all, unsafe_code)]
pub(crate) mod agent_content_capnp {
    include!(concat!(env!("OUT_DIR"), "/agent_content_capnp.rs"));
}
use agent_content_capnp as wire;

pub mod client;

#[cfg(not(target_arch = "wasm32"))]
pub mod host;

pub const PROFILE: &str = "agent-content-v1";
pub const VERSION: u16 = 1;
pub const REVISION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_QUERY_CARDS: usize = 16;
pub const MAX_PROPOSALS: usize = 32;
pub const MAX_READ_BYTES: u32 = 32 * 1024;
pub const MAX_CONTENT_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_COMMAND_BYTES: usize = runtime::MAX_MESSAGE_BYTES;
pub const SCHEMA: &[u8] = include_bytes!("../contracts/agent_content.capnp");
include!(concat!(env!("OUT_DIR"), "/schema_digest.rs"));

pub const fn schema_digest() -> [u8; 32] {
    SCHEMA_DIGEST
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Contract,
    Limit,
    Correlation,
}
pub type Result<T> = std::result::Result<T, Error>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "agent content {self:?}")
    }
}
impl std::error::Error for Error {}

fn invalid<T>(_: T) -> Error {
    Error::Invalid
}
fn core_error(error: morrow_core::Error) -> Error {
    match error {
        morrow_core::Error::Limit => Error::Limit,
        morrow_core::Error::UnsupportedVersion => Error::Contract,
        _ => Error::Invalid,
    }
}
fn identity(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    let value = value.map_err(invalid)?.to_str().map_err(invalid)?;
    identity(value)?;
    Ok(value.to_owned())
}
fn hash32(value: capnp::Result<&[u8]>) -> Result<[u8; 32]> {
    value.map_err(invalid)?.try_into().map_err(invalid)
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
fn read(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::OwnedSegments>> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    let mut remaining = bytes;
    let message = serialize::read_message(
        &mut remaining,
        ReaderOptions {
            traversal_limit_in_words: Some(MAX_FRAME_BYTES / 8 * 4),
            nesting_limit: 16,
        },
    )
    .map_err(invalid)?;
    if !remaining.is_empty() {
        return Err(Error::Invalid);
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
fn contract(
    version: u16,
    revision: u16,
    schema: &[u8],
    core_version: u16,
    rt: &[u8],
    content: &[u8],
) -> Result<()> {
    if version != VERSION
        || revision != REVISION
        || schema != SCHEMA_DIGEST
        || core_version != runtime::PROTOCOL_VERSION
        || rt != runtime::runtime_digest()
        || content != runtime::content_digest()
    {
        return Err(Error::Contract);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentRef {
    pub card_id: String,
    pub revision: u64,
    pub total_length: u64,
    pub body_sha256: [u8; 32],
}
impl ContentRef {
    pub fn validate(&self) -> Result<()> {
        identity(&self.card_id)?;
        if self.revision == 0 || self.total_length > MAX_CONTENT_BYTES {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
fn set_reference(mut out: wire::content_ref::Builder<'_>, reference: &ContentRef) {
    out.set_card_id(reference.card_id.as_str());
    out.set_revision(reference.revision);
    out.set_total_length(reference.total_length);
    out.set_body_sha256(&reference.body_sha256);
}
fn get_reference(value: capnp::Result<wire::content_ref::Reader<'_>>) -> Result<ContentRef> {
    let root = value.map_err(invalid)?;
    let size = <wire::content_ref::Builder<'_> as HasStructSize>::STRUCT_SIZE;
    exact_layout(root, size.data, size.pointers)?;
    let reference = ContentRef {
        card_id: text(root.get_card_id())?,
        revision: root.get_revision(),
        total_length: root.get_total_length(),
        body_sha256: hash32(root.get_body_sha256())?,
    };
    reference.validate()?;
    Ok(reference)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Query {
        cards: Vec<String>,
    },
    ReadRef {
        reference: ContentRef,
        offset: u64,
        length: u32,
    },
    ProposeMutation {
        reference: ContentRef,
        command: Vec<u8>,
    },
    InspectOperation {
        card_id: String,
        operation_id: String,
    },
}
impl Action {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Query { cards } => {
                if cards.is_empty() || cards.len() > MAX_QUERY_CARDS {
                    return Err(Error::Limit);
                }
                let mut unique = std::collections::BTreeSet::new();
                for card in cards {
                    identity(card)?;
                    if !unique.insert(card) {
                        return Err(Error::Invalid);
                    }
                }
            }
            Self::ReadRef {
                reference,
                offset,
                length,
            } => {
                reference.validate()?;
                if *offset > reference.total_length
                    || *length == 0
                    || *length > MAX_READ_BYTES
                    || offset.checked_add(u64::from(*length)).is_none()
                {
                    return Err(Error::Limit);
                }
            }
            Self::ProposeMutation { reference, command } => {
                reference.validate()?;
                if command.is_empty() || command.len() > MAX_COMMAND_BYTES {
                    return Err(Error::Limit);
                }
                let runtime::Command::EditContent(change) =
                    runtime::Command::decode(command).map_err(core_error)?
                else {
                    return Err(Error::Invalid);
                };
                if change.card_id != reference.card_id
                    || change.expected_revision != reference.revision
                {
                    return Err(Error::Correlation);
                }
            }
            Self::InspectOperation {
                card_id,
                operation_id,
            } => {
                identity(card_id)?;
                identity(operation_id)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    request_id: String,
    action: Action,
    bytes: Vec<u8>,
    digest: [u8; 32],
}
impl Request {
    pub fn new(request_id: impl Into<String>, action: Action) -> Result<Self> {
        let request_id = request_id.into();
        identity(&request_id)?;
        action.validate()?;
        let mut message = Builder::new_default();
        {
            let mut root = message.init_root::<wire::request::Builder<'_>>();
            root.set_version(VERSION);
            root.set_revision(REVISION);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_runtime_digest(&runtime::runtime_digest());
            root.set_content_digest(&runtime::content_digest());
            root.set_core_runtime_version(runtime::PROTOCOL_VERSION);
            root.set_request_id(request_id.as_str());
            match &action {
                Action::Query { cards } => {
                    let mut out = root.init_query().init_cards(cards.len() as u32);
                    for (i, card) in cards.iter().enumerate() {
                        out.set(i as u32, card.as_str());
                    }
                }
                Action::ReadRef {
                    reference,
                    offset,
                    length,
                } => {
                    let mut out = root.init_read_ref();
                    out.set_offset(*offset);
                    out.set_length(*length);
                    set_reference(out.init_reference(), reference);
                }
                Action::ProposeMutation { reference, command } => {
                    let mut out = root.init_propose_mutation();
                    out.set_command(command);
                    set_reference(out.init_reference(), reference);
                }
                Action::InspectOperation {
                    card_id,
                    operation_id,
                } => {
                    let mut out = root.init_inspect_operation();
                    out.set_card_id(card_id.as_str());
                    out.set_operation_id(operation_id.as_str());
                }
            }
        }
        let bytes = finish(&message)?;
        let digest = Sha256::digest(&bytes).into();
        Ok(Self {
            request_id,
            action,
            bytes,
            digest,
        })
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::request::Reader<'_>>()
            .map_err(invalid)?;
        let size = <wire::request::Builder<'_> as HasStructSize>::STRUCT_SIZE;
        exact_layout(root, size.data, size.pointers)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        contract(
            root.get_version(),
            root.get_revision(),
            root.get_schema_sha256().map_err(invalid)?,
            root.get_core_runtime_version(),
            root.get_runtime_digest().map_err(invalid)?,
            root.get_content_digest().map_err(invalid)?,
        )?;
        let request_id = text(root.get_request_id())?;
        let action = match root.which().map_err(invalid)? {
            wire::request::Query(value) => {
                let value = value.map_err(invalid)?;
                let size = <wire::query::Builder<'_> as HasStructSize>::STRUCT_SIZE;
                exact_layout(value, size.data, size.pointers)?;
                let cards = value.get_cards().map_err(invalid)?;
                if cards.is_empty() || cards.len() as usize > MAX_QUERY_CARDS {
                    return Err(Error::Limit);
                }
                Action::Query {
                    cards: cards.iter().map(text).collect::<Result<Vec<_>>>()?,
                }
            }
            wire::request::ReadRef(value) => {
                let value = value.map_err(invalid)?;
                let size = <wire::read_ref::Builder<'_> as HasStructSize>::STRUCT_SIZE;
                exact_layout(value, size.data, size.pointers)?;
                Action::ReadRef {
                    reference: get_reference(value.get_reference())?,
                    offset: value.get_offset(),
                    length: value.get_length(),
                }
            }
            wire::request::ProposeMutation(value) => {
                let value = value.map_err(invalid)?;
                let size = <wire::propose_mutation::Builder<'_> as HasStructSize>::STRUCT_SIZE;
                exact_layout(value, size.data, size.pointers)?;
                let command = value.get_command().map_err(invalid)?;
                if command.is_empty() || command.len() > MAX_COMMAND_BYTES {
                    return Err(Error::Limit);
                }
                Action::ProposeMutation {
                    reference: get_reference(value.get_reference())?,
                    command: command.to_vec(),
                }
            }
            wire::request::InspectOperation(value) => {
                let value = value.map_err(invalid)?;
                let size = <wire::inspect_operation::Builder<'_> as HasStructSize>::STRUCT_SIZE;
                exact_layout(value, size.data, size.pointers)?;
                Action::InspectOperation {
                    card_id: text(value.get_card_id())?,
                    operation_id: text(value.get_operation_id())?,
                }
            }
        };
        action.validate()?;
        Ok(Self {
            request_id,
            action,
            bytes: bytes.to_vec(),
            digest: Sha256::digest(bytes).into(),
        })
    }
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    pub fn action(&self) -> &Action {
        &self.action
    }
    pub fn wire(&self) -> &[u8] {
        &self.bytes
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        Ok(self.bytes.clone())
    }
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Query {
        responses: Vec<Vec<u8>>,
    },
    ReadRef {
        response: Vec<u8>,
    },
    Proposed {
        operation_id: String,
        proposal_sha256: [u8; 32],
    },
    Operation {
        response: Vec<u8>,
    },
    Rejected {
        failure: response::Failure,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub request_id: String,
    pub request_sha256: [u8; 32],
    pub outcome: Outcome,
}
fn core_response(bytes: &[u8]) -> Result<response::Response> {
    if bytes.is_empty() || bytes.len() > runtime::MAX_MESSAGE_BYTES {
        return Err(Error::Limit);
    }
    response::Response::decode(bytes).map_err(core_error)
}
fn failure_to_wire(value: response::Failure) -> wire::Failure {
    match value {
        response::Failure::Denied => wire::Failure::Denied,
        response::Failure::NotFound => wire::Failure::NotFound,
        response::Failure::RevisionConflict => wire::Failure::RevisionConflict,
        response::Failure::OperationConflict => wire::Failure::OperationConflict,
        response::Failure::Capacity => wire::Failure::Capacity,
        response::Failure::Busy => wire::Failure::Busy,
        response::Failure::Storage => wire::Failure::Storage,
        response::Failure::CommitUnknown => wire::Failure::CommitUnknown,
        response::Failure::Limit => wire::Failure::Limit,
    }
}
fn failure_from_wire(value: wire::Failure) -> response::Failure {
    match value {
        wire::Failure::Denied => response::Failure::Denied,
        wire::Failure::NotFound => response::Failure::NotFound,
        wire::Failure::RevisionConflict => response::Failure::RevisionConflict,
        wire::Failure::OperationConflict => response::Failure::OperationConflict,
        wire::Failure::Capacity => response::Failure::Capacity,
        wire::Failure::Busy => response::Failure::Busy,
        wire::Failure::Storage => response::Failure::Storage,
        wire::Failure::CommitUnknown => response::Failure::CommitUnknown,
        wire::Failure::Limit => response::Failure::Limit,
    }
}
impl Reply {
    pub fn new(request: &Request, outcome: Outcome) -> Result<Self> {
        let reply = Self {
            request_id: request.request_id.clone(),
            request_sha256: request.digest,
            outcome,
        };
        reply.validate_for(request)?;
        Ok(reply)
    }
    fn validate(&self) -> Result<()> {
        identity(&self.request_id)?;
        let responses: &[Vec<u8>] = match &self.outcome {
            Outcome::Query { responses } => {
                if responses.is_empty() || responses.len() > MAX_QUERY_CARDS {
                    return Err(Error::Limit);
                }
                responses
            }
            Outcome::ReadRef { response } | Outcome::Operation { response } => {
                std::slice::from_ref(response)
            }
            Outcome::Proposed { operation_id, .. } => {
                identity(operation_id)?;
                return Ok(());
            }
            Outcome::Rejected { .. } => return Ok(()),
        };
        let mut total = 0usize;
        for bytes in responses {
            total = total.checked_add(bytes.len()).ok_or(Error::Limit)?;
            if total > MAX_FRAME_BYTES {
                return Err(Error::Limit);
            }
            let decoded = core_response(bytes)?;
            if decoded.request_id != self.request_id {
                return Err(Error::Correlation);
            }
            let allowed = matches!(decoded.outcome, response::Outcome::Rejected(_))
                || match &self.outcome {
                    Outcome::Query { .. } => {
                        matches!(decoded.outcome, response::Outcome::Summary(_))
                    }
                    Outcome::ReadRef { .. } => {
                        matches!(decoded.outcome, response::Outcome::ContentChunk(_))
                    }
                    Outcome::Operation { .. } => {
                        matches!(decoded.outcome, response::Outcome::OperationResult { .. })
                    }
                    _ => false,
                };
            if !allowed {
                return Err(Error::Correlation);
            }
        }
        Ok(())
    }
    pub fn validate_for(&self, request: &Request) -> Result<()> {
        self.validate()?;
        if self.request_id != request.request_id || self.request_sha256 != request.digest {
            return Err(Error::Correlation);
        }
        match (&request.action, &self.outcome) {
            (_, Outcome::Rejected { .. }) => Ok(()),
            (Action::Query { cards }, Outcome::Query { responses }) => {
                if cards.len() != responses.len() {
                    return Err(Error::Correlation);
                }
                for (card, bytes) in cards.iter().zip(responses) {
                    if let response::Outcome::Summary(summary) = core_response(bytes)?.outcome
                        && summary.id != *card
                    {
                        return Err(Error::Correlation);
                    }
                }
                Ok(())
            }
            (
                Action::ReadRef {
                    reference,
                    offset,
                    length,
                },
                Outcome::ReadRef { response },
            ) => {
                if let response::Outcome::ContentChunk(chunk) = core_response(response)?.outcome {
                    let expected_length = u64::from(*length).min(reference.total_length - offset);
                    if chunk.card_id != reference.card_id
                        || chunk.revision != reference.revision
                        || chunk.offset != *offset
                        || chunk.total_length != reference.total_length
                        || chunk.body_sha256 != reference.body_sha256
                        || chunk.bytes.len() as u64 != expected_length
                    {
                        return Err(Error::Correlation);
                    }
                }
                Ok(())
            }
            (
                Action::ProposeMutation { command, .. },
                Outcome::Proposed {
                    operation_id,
                    proposal_sha256,
                },
            ) => {
                if runtime::Command::decode(command)
                    .map_err(core_error)?
                    .request_id()
                    != operation_id
                    || *proposal_sha256 != request.digest
                {
                    return Err(Error::Correlation);
                }
                Ok(())
            }
            (
                Action::InspectOperation {
                    card_id,
                    operation_id,
                },
                Outcome::Operation { response },
            ) => {
                if let response::Outcome::OperationResult {
                    card_id: actual_card,
                    operation_id: actual_op,
                    ..
                } = core_response(response)?.outcome
                    && (actual_card != *card_id || actual_op != *operation_id)
                {
                    return Err(Error::Correlation);
                }
                Ok(())
            }
            _ => Err(Error::Correlation),
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        {
            let mut root = message.init_root::<wire::reply::Builder<'_>>();
            root.set_version(VERSION);
            root.set_revision(REVISION);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_runtime_digest(&runtime::runtime_digest());
            root.set_content_digest(&runtime::content_digest());
            root.set_core_runtime_version(runtime::PROTOCOL_VERSION);
            root.set_request_id(self.request_id.as_str());
            root.set_request_sha256(&self.request_sha256);
            match &self.outcome {
                Outcome::Query { responses } => {
                    let mut out = root.init_query(responses.len() as u32);
                    for (i, response) in responses.iter().enumerate() {
                        out.set(i as u32, response);
                    }
                }
                Outcome::ReadRef { response } => root.set_read_ref(response),
                Outcome::Operation { response } => root.set_operation(response),
                Outcome::Proposed {
                    operation_id,
                    proposal_sha256,
                } => {
                    let mut out = root.init_proposed();
                    out.set_operation_id(operation_id.as_str());
                    out.set_proposal_sha256(proposal_sha256);
                }
                Outcome::Rejected { failure } => root.set_rejected(failure_to_wire(*failure)),
            }
        }
        finish(&message)
    }
    pub fn decode_for(bytes: &[u8], request: &Request) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::reply::Reader<'_>>()
            .map_err(invalid)?;
        let size = <wire::reply::Builder<'_> as HasStructSize>::STRUCT_SIZE;
        exact_layout(root, size.data, size.pointers)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        contract(
            root.get_version(),
            root.get_revision(),
            root.get_schema_sha256().map_err(invalid)?,
            root.get_core_runtime_version(),
            root.get_runtime_digest().map_err(invalid)?,
            root.get_content_digest().map_err(invalid)?,
        )?;
        let request_id = text(root.get_request_id())?;
        let request_sha256 = hash32(root.get_request_sha256())?;
        let outcome = match root.which().map_err(invalid)? {
            wire::reply::Query(value) => {
                let value = value.map_err(invalid)?;
                if value.is_empty() || value.len() as usize > MAX_QUERY_CARDS {
                    return Err(Error::Limit);
                }
                let mut total = 0usize;
                let mut responses = Vec::with_capacity(value.len() as usize);
                for response in value {
                    let response = response.map_err(invalid)?;
                    total = total.checked_add(response.len()).ok_or(Error::Limit)?;
                    if response.is_empty()
                        || response.len() > MAX_COMMAND_BYTES
                        || total > MAX_FRAME_BYTES
                    {
                        return Err(Error::Limit);
                    }
                    responses.push(response.to_vec());
                }
                Outcome::Query { responses }
            }
            wire::reply::ReadRef(value) => {
                let value = value.map_err(invalid)?;
                if value.is_empty() || value.len() > MAX_COMMAND_BYTES {
                    return Err(Error::Limit);
                }
                Outcome::ReadRef {
                    response: value.to_vec(),
                }
            }
            wire::reply::Operation(value) => {
                let value = value.map_err(invalid)?;
                if value.is_empty() || value.len() > MAX_COMMAND_BYTES {
                    return Err(Error::Limit);
                }
                Outcome::Operation {
                    response: value.to_vec(),
                }
            }
            wire::reply::Proposed(value) => {
                let value = value.map_err(invalid)?;
                let size = <wire::proposed::Builder<'_> as HasStructSize>::STRUCT_SIZE;
                exact_layout(value, size.data, size.pointers)?;
                Outcome::Proposed {
                    operation_id: text(value.get_operation_id())?,
                    proposal_sha256: hash32(value.get_proposal_sha256())?,
                }
            }
            wire::reply::Rejected(value) => Outcome::Rejected {
                failure: failure_from_wire(value.map_err(invalid)?),
            },
        };
        let reply = Self {
            request_id,
            request_sha256,
            outcome,
        };
        reply.validate_for(request)?;
        Ok(reply)
    }
}
