//! Independent guest task codec. No authority is derived from IDs or task bytes.
use crate::contract;
use crate::{
    protocol::{CodecError, Request},
    task_capnp as wire,
};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub const MAX_TASK_BYTES: usize = 128 * 1024;
type Result<T> = std::result::Result<T, CodecError>;
fn invalid<T>(_: T) -> CodecError {
    CodecError::Invalid
}
#[derive(Debug)]
pub struct Invocation {
    task_id: String,
    request: Request,
    command: Vec<u8>,
    digest: [u8; 32],
}
impl Invocation {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_TASK_BYTES {
            return Err(CodecError::Limit);
        }
        let mut rest = bytes;
        let reader = serialize::read_message_from_flat_slice(
            &mut rest,
            ReaderOptions {
                traversal_limit_in_words: Some(MAX_TASK_BYTES / 8),
                nesting_limit: 16,
            },
        )
        .map_err(invalid)?;
        if !rest.is_empty() {
            return Err(CodecError::Invalid);
        }
        let root = reader
            .get_root::<wire::invocation::Reader>()
            .map_err(invalid)?;
        if root.get_version() != 1
            || root.get_schema_digest().map_err(invalid)? != contract::TASK_DIGEST
        {
            return Err(CodecError::Contract);
        }
        let task_id = root
            .get_task_id()
            .map_err(invalid)?
            .to_str()
            .map_err(invalid)?
            .to_owned();
        if task_id.is_empty()
            || task_id.len() > 256
            || task_id
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | ':') || c == char::from(92))
        {
            return Err(CodecError::Invalid);
        }
        let command = root.get_command().map_err(invalid)?.to_vec();
        let request = Request::decode(&command)?;
        Ok(Self {
            task_id,
            request,
            command,
            digest: Sha256::digest(bytes).into(),
        })
    }
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
    pub fn request(&self) -> &Request {
        &self.request
    }
    pub fn command_bytes(&self) -> &[u8] {
        &self.command
    }
    pub fn completion(&self, response: &[u8]) -> Result<Vec<u8>> {
        self.request.decode_reply(response)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::completion::Builder>();
        root.set_version(1);
        root.set_schema_digest(&contract::TASK_DIGEST);
        root.set_task_id(self.task_id());
        root.set_input_digest(&self.digest);
        root.set_response(response);
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_TASK_BYTES {
            return Err(CodecError::Limit);
        }
        Ok(bytes)
    }
}
