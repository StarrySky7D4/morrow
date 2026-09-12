//! Fixed dependency-call codec. Successful decoding grants no authority and commits no content.
//! Rejections and task failures travel through the host failure boundary, never a success response.
use crate::{dependency_call_capnp as wire, protocol::CodecError as Error};
type Result<T> = std::result::Result<T, Error>;
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub const VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
pub fn schema_digest() -> [u8; 32] {
    crate::contract::DEPENDENCY_CALL_DIGEST
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid
}
fn identity(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | ':' | '\\'))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn message() -> Builder<capnp::message::HeapAllocator> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::message::Builder>();
    root.set_version(VERSION);
    root.set_schema_digest(&schema_digest());
    message
}
fn finish(message: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(message);
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
fn read(bytes: &[u8]) -> Result<capnp::message::Reader<serialize::OwnedSegments>> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(Error::Limit);
    }
    let mut remaining = bytes;
    // Owned aligned words accept arbitrary transport byte alignment without hand-written unsafe.
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
        .get_root::<wire::message::Reader>()
        .map_err(invalid)?;
    // Traverse the complete object, including unfamiliar pointer fields, within a fixed budget.
    // The second budget half permits reading the known payload after this structural check.
    root.total_size().map_err(invalid)?;
    if root.get_version() != VERSION
        || root.get_schema_digest().map_err(invalid)? != schema_digest()
    {
        return Err(Error::Contract);
    }
    Ok(message)
}
fn id(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    let value = value.map_err(invalid)?;
    if value.len() > 256 {
        return Err(Error::Limit);
    }
    let value = value.to_str().map_err(invalid)?;
    identity(value)?;
    Ok(value.into())
}
/// Immutable validated fields plus the exact frame used for response correlation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    bytes: Vec<u8>,
    call_id: String,
    slot: String,
    input: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub output_type: String,
    pub bytes: Vec<u8>,
}
impl Request {
    pub fn new(call_id: &str, slot: &str, input: &[u8]) -> Result<Self> {
        identity(call_id)?;
        identity(slot)?;
        if input.is_empty() || input.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        let mut message = message();
        let root = message
            .get_root::<wire::message::Builder>()
            .map_err(invalid)?;
        let mut request = root.init_request();
        request.set_call_id(call_id);
        request.set_slot(slot);
        request.set_input(input);
        Ok(Self {
            bytes: finish(&message)?,
            call_id: call_id.into(),
            slot: slot.into(),
            input: input.to_vec(),
        })
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::message::Reader>()
            .map_err(invalid)?;
        let wire::message::Request(request) = root.which().map_err(invalid)? else {
            return Err(invalid(()));
        };
        let request = request.map_err(invalid)?;
        let call_id = id(request.get_call_id())?;
        let slot = id(request.get_slot())?;
        let input = request.get_input().map_err(invalid)?;
        if input.is_empty() || input.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        Ok(Self {
            bytes: bytes.into(),
            call_id,
            slot,
            input: input.into(),
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn call_id(&self) -> &str {
        &self.call_id
    }
    pub fn slot(&self) -> &str {
        &self.slot
    }
    pub fn input(&self) -> &[u8] {
        &self.input
    }
    pub fn encode_response(&self, output_type: &str, output: &[u8]) -> Result<Vec<u8>> {
        identity(output_type)?;
        if output.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        let mut message = message();
        let root = message
            .get_root::<wire::message::Builder>()
            .map_err(invalid)?;
        let mut response = root.init_response();
        response.set_call_id(&self.call_id);
        response.set_request_sha256(&Sha256::digest(&self.bytes));
        response.set_output_type(output_type);
        response.set_output(output);
        finish(&message)
    }
    pub fn verify_response(&self, bytes: &[u8]) -> Result<Output> {
        let message = read(bytes)?;
        let root = message
            .get_root::<wire::message::Reader>()
            .map_err(invalid)?;
        let wire::message::Response(response) = root.which().map_err(invalid)? else {
            return Err(invalid(()));
        };
        let response = response.map_err(invalid)?;
        let call_id = id(response.get_call_id())?;
        let digest = response.get_request_sha256().map_err(invalid)?;
        if call_id != self.call_id || digest != Sha256::digest(&self.bytes).as_slice() {
            return Err(Error::Correlation);
        }
        let output_type = id(response.get_output_type())?;
        let output = response.get_output().map_err(invalid)?;
        if output.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        Ok(Output {
            output_type,
            bytes: output.into(),
        })
    }
}
