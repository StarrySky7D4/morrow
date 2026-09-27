//! Independent, bounded mutation guest wire codec. Valid frames convey no authority.
//! Transport uncertainty is left to the caller; this module never retries or acts.
use crate::{contract, mutation_capnp as wire};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Limit,
    Contract,
    Correlation,
    Unsupported,
}
pub type Result<T> = std::result::Result<T, Error>;
fn invalid<T>(_: T) -> Error {
    Error::Invalid
}
pub fn schema_digest() -> [u8; 32] {
    contract::MUTATION_DIGEST
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
        return Err(Error::Invalid);
    }
    Ok(message)
}
fn finish(message: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(message);
    if bytes.len() > MAX_FRAME_BYTES {
        Err(Error::Limit)
    } else {
        Ok(bytes)
    }
}
fn identity(bytes: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = bytes.try_into().map_err(|_| Error::Invalid)?;
    if value.iter().all(|&b| b == 0) {
        return Err(Error::Invalid);
    }
    Ok(value)
}
fn operation(value: &str) -> Result<()> {
    if value.is_empty()
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(Error::Invalid);
    }
    if value.len() > MAX_OPERATION_BYTES {
        return Err(Error::Limit);
    }
    Ok(())
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
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Create {
                content_length,
                content_sha256,
            } => {
                if *content_length > MAX_CONTENT_BYTES {
                    return Err(Error::Limit);
                }
                identity(content_sha256)?;
                if *content_length == 0 && *content_sha256 != Sha256::digest([]).as_slice() {
                    return Err(Error::Invalid);
                }
            }
            Self::Chunk { offset, bytes } => {
                if bytes.is_empty() {
                    return Err(Error::Invalid);
                }
                if bytes.len() > MAX_CHUNK_BYTES
                    || offset
                        .checked_add(bytes.len() as u64)
                        .is_none_or(|n| n > MAX_CONTENT_BYTES)
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
        if self.call_id == 0 || self.deadline_ms == 0 {
            return Err(Error::Invalid);
        }
        if self.deadline_ms > MAX_DEADLINE_MS {
            return Err(Error::Limit);
        }
        identity(&self.reference)?;
        identity(&self.submission)?;
        operation(&self.operation_id)?;
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
        root.set_operation_id(self.operation_id.as_str());
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
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != schema_digest()
        {
            return Err(Error::Contract);
        }
        let reference = identity(root.get_reference().map_err(invalid)?)?;
        let submission = identity(root.get_submission().map_err(invalid)?)?;
        let text = root.get_operation_id().map_err(invalid)?;
        let op_bytes = text.as_bytes();
        if op_bytes.len() > MAX_OPERATION_BYTES {
            return Err(Error::Limit);
        }
        let operation_id = text.to_str().map_err(invalid)?.to_owned();
        let action = match root.which().map_err(|_| Error::Unsupported)? {
            wire::request::PrepareCreate(v) => {
                let value = v.map_err(invalid)?;
                Action::Create {
                    content_length: value.get_content_length(),
                    content_sha256: identity(value.get_content_sha256().map_err(invalid)?)?,
                }
            }
            wire::request::PrepareDelete(()) => Action::Delete,
            wire::request::Chunk(v) => {
                let value = v.map_err(invalid)?;
                let data = value.get_bytes().map_err(invalid)?;
                if data.len() > MAX_CHUNK_BYTES {
                    return Err(Error::Limit);
                }
                Action::Chunk {
                    offset: value.get_offset(),
                    bytes: data.to_vec(),
                }
            }
            wire::request::Commit(()) => Action::Commit,
            wire::request::Execute(()) => Action::Execute,
            wire::request::Query(()) => Action::Query,
            wire::request::CancelPlan(()) => Action::CancelPlan,
            wire::request::Release(()) => Action::Release,
        };
        let result = Self {
            call_id: root.get_call_id(),
            reference,
            submission,
            operation_id,
            deadline_ms: root.get_deadline_ms(),
            action,
        };
        result.validate()?;
        Ok(result)
    }
    pub fn decode_reply(&self, bytes: &[u8]) -> Result<Response> {
        Response::decode(self, bytes)
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
    pub fn validate(&self, request: &Request) -> Result<()> {
        request.validate()?;
        if self.call_id != request.call_id
            || self.reference != request.reference
            || self.submission != request.submission
            || self.operation_id != request.operation_id
            || self.kind != request.action.kind()
        {
            return Err(Error::Correlation);
        }
        if self.status == Status::Invalid || self.kind == Kind::Invalid {
            return Err(Error::Invalid);
        }
        if self.staged_bytes > MAX_CONTENT_BYTES {
            return Err(Error::Limit);
        }
        if self.status == Status::OutcomeUnknown {
            if matches!(self.kind, Kind::Execute | Kind::Query)
                && self.phase == Phase::OutcomeUnknown
                && self.effect == Effect::Unspecified
                && self.staged_bytes == 0
                && !self.durable_content
            {
                return Ok(());
            }
            return Err(Error::Invalid);
        }
        if self.status != Status::Completed {
            if self.phase == Phase::None
                && self.effect == Effect::Unspecified
                && self.staged_bytes == 0
                && !self.durable_content
            {
                return Ok(());
            }
            return Err(Error::Invalid);
        }
        match self.kind {
            Kind::PrepareCreate | Kind::PrepareDelete => {
                if self.phase != Phase::Prepared
                    || self.effect != Effect::Unspecified
                    || self.staged_bytes != 0
                    || self.durable_content
                {
                    return Err(Error::Invalid);
                }
            }
            Kind::Chunk | Kind::Commit => {
                if self.phase != Phase::Prepared
                    || self.effect != Effect::Unspecified
                    || (self.kind == Kind::Chunk && self.durable_content)
                    || (self.kind == Kind::Commit && !self.durable_content)
                {
                    return Err(Error::Invalid);
                }
                if let Action::Chunk { offset, bytes } = &request.action
                    && self.staged_bytes != offset + bytes.len() as u64
                {
                    return Err(Error::Invalid);
                }
            }
            Kind::Execute => {
                if self.phase != Phase::Observed
                    || !matches!(self.effect, Effect::OsSucceeded | Effect::OsRejected)
                {
                    return Err(Error::Invalid);
                }
            }
            Kind::Query => {
                let valid = match self.phase {
                    Phase::Absent
                    | Phase::Prepared
                    | Phase::OutcomeUnknown
                    | Phase::CancelledBeforeDispatch => self.effect == Effect::Unspecified,
                    Phase::Observed => {
                        matches!(self.effect, Effect::OsSucceeded | Effect::OsRejected)
                    }
                    Phase::None => false,
                };
                if !valid
                    || (matches!(self.phase, Phase::Absent | Phase::CancelledBeforeDispatch)
                        && (self.staged_bytes != 0 || self.durable_content))
                {
                    return Err(Error::Invalid);
                }
            }
            Kind::CancelPlan => {
                if self.phase != Phase::CancelledBeforeDispatch
                    || self.effect != Effect::Unspecified
                    || self.staged_bytes != 0
                    || self.durable_content
                {
                    return Err(Error::Invalid);
                }
            }
            Kind::Release => {
                if self.phase != Phase::None
                    || self.effect != Effect::Unspecified
                    || self.staged_bytes != 0
                    || self.durable_content
                {
                    return Err(Error::Invalid);
                }
            }
            Kind::Invalid => return Err(Error::Invalid),
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
        root.set_operation_id(self.operation_id.as_str());
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
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != schema_digest()
        {
            return Err(Error::Contract);
        }
        let op = root.get_operation_id().map_err(invalid)?;
        if root.get_call_id() != request.call_id
            || root.get_reference().map_err(invalid)? != request.reference
            || root.get_submission().map_err(invalid)? != request.submission
            || op.as_bytes() != request.operation_id.as_bytes()
            || root.get_kind().map_err(|_| Error::Unsupported)? != request.action.kind()
        {
            return Err(Error::Correlation);
        }
        let result = Self {
            call_id: request.call_id,
            reference: request.reference,
            submission: request.submission,
            operation_id: request.operation_id.clone(),
            kind: root.get_kind().map_err(|_| Error::Unsupported)?,
            status: root.get_status().map_err(|_| Error::Unsupported)?,
            phase: root.get_phase().map_err(|_| Error::Unsupported)?,
            effect: root.get_effect().map_err(|_| Error::Unsupported)?,
            staged_bytes: root.get_staged_bytes(),
            durable_content: root.get_durable_content(),
        };
        result.validate(request)?;
        Ok(result)
    }
}

/// A guest call failure is separate from the mutation wire result. In particular,
/// `TransportUnknown` does not establish that an Execute had no OS effect.
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasmCallError {
    Codec(Error),
    NoMemory,
    TransportUnknown,
}

/// Owns the exact, fully correlated reply bytes accepted from the host.
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
pub struct WasmResponse {
    pub response: Response,
    encoded_frame: Vec<u8>,
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
impl WasmResponse {
    pub fn encoded_frame(&self) -> &[u8] {
        &self.encoded_frame
    }
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
#[link(wasm_import_module = "morrow_mutation_v1")]
unsafe extern "C" {
    #[link_name = "call"]
    fn mutation_call(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
#[link(wasm_import_module = "morrow_task_v1")]
unsafe extern "C" {
    #[link_name = "read_input"]
    fn mutation_read_input(output: *mut u8, capacity: u32) -> i32;
    #[link_name = "complete"]
    fn mutation_complete(input: *const u8, length: u32) -> i32;
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
fn wasm_buffer() -> std::result::Result<Vec<u8>, WasmCallError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(MAX_FRAME_BYTES)
        .map_err(|_| WasmCallError::NoMemory)?;
    bytes.resize(MAX_FRAME_BYTES, 0);
    Ok(bytes)
}

/// Read the host-supplied encoded mutation Request once. This does not issue
/// a selection or execution permit. Keep the original frame for the call.
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
pub fn wasm_read_request_frame() -> std::result::Result<Vec<u8>, WasmCallError> {
    let mut frame = wasm_buffer()?;
    // SAFETY: the owned output spans the advertised capacity for this synchronous import.
    let size = unsafe { mutation_read_input(frame.as_mut_ptr(), frame.len() as u32) };
    if size <= 0 || size as usize > frame.len() {
        return Err(WasmCallError::TransportUnknown);
    }
    frame.truncate(size as usize);
    Request::decode(&frame).map_err(WasmCallError::Codec)?;
    Ok(frame)
}

/// Call the host exactly once with a validated frame. The original frame is
/// retained separately from the import buffer so a modified input cannot
/// change reply correlation. No transport failure triggers an automatic retry.
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
pub fn wasm_call_frame(frame: &[u8]) -> std::result::Result<WasmResponse, WasmCallError> {
    let request = Request::decode(frame).map_err(WasmCallError::Codec)?;
    let mut input = Vec::new();
    input
        .try_reserve_exact(frame.len())
        .map_err(|_| WasmCallError::NoMemory)?;
    input.extend_from_slice(frame);
    let mut output = wasm_buffer()?;
    // SAFETY: the live, owned input and output buffers are disjoint and bounded.
    let size = unsafe {
        mutation_call(
            input.as_ptr(),
            input.len() as u32,
            output.as_mut_ptr(),
            output.len() as u32,
        )
    };
    if input != frame || size <= 0 || size as usize > output.len() {
        return Err(WasmCallError::TransportUnknown);
    }
    output.truncate(size as usize);
    let response = request
        .decode_reply(&output)
        .map_err(WasmCallError::Codec)?;
    Ok(WasmResponse {
        response,
        encoded_frame: output,
    })
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
impl Request {
    pub fn wasm_call(&self) -> std::result::Result<WasmResponse, WasmCallError> {
        let frame = self.encode().map_err(WasmCallError::Codec)?;
        wasm_call_frame(&frame)
    }
}

/// Complete with the exact validated response frame. A failed completion also
/// leaves the host outcome uncertain; the SDK never synthesizes a receipt.
#[cfg(all(target_arch = "wasm32", feature = "wasm-guest"))]
pub fn wasm_complete_response(response: &WasmResponse) -> std::result::Result<(), WasmCallError> {
    let frame = response.encoded_frame();
    // SAFETY: the owned reply remains live throughout this synchronous import.
    if unsafe { mutation_complete(frame.as_ptr(), frame.len() as u32) } != 0 {
        return Err(WasmCallError::TransportUnknown);
    }
    Ok(())
}
