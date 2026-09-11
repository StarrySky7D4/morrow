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
pub const MAX_VALUE_BYTES: usize = 64 * 1024;
type Result<T> = std::result::Result<T, CodecError>;
fn invalid<T>(_: T) -> CodecError {
    CodecError::Invalid
}
#[derive(Debug)]
pub struct Transform {
    pub handler: String,
    pub input_type: String,
    pub output_type: String,
    pub input: Vec<u8>,
}
fn id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | ':') || c == char::from(92))
    {
        Err(CodecError::Invalid)
    } else {
        Ok(())
    }
}
#[derive(Debug)]
pub struct Invocation {
    task_id: String,
    request: Option<Request>,
    transform: Option<Transform>,
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
        if root.get_version() != contract::TASK_VERSION
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
        let (request, transform) = match root.get_kind().map_err(invalid)? {
            wire::Kind::ContentCommand => {
                if root.has_transform() {
                    return Err(CodecError::Invalid);
                }
                (Some(Request::decode(&command)?), None)
            }
            wire::Kind::Transform => {
                if !command.is_empty() || !root.has_transform() {
                    return Err(CodecError::Invalid);
                }
                let v = root.get_transform().map_err(invalid)?;
                let t = Transform {
                    handler: v
                        .get_handler()
                        .map_err(invalid)?
                        .to_str()
                        .map_err(invalid)?
                        .into(),
                    input_type: v
                        .get_input_type()
                        .map_err(invalid)?
                        .to_str()
                        .map_err(invalid)?
                        .into(),
                    output_type: v
                        .get_output_type()
                        .map_err(invalid)?
                        .to_str()
                        .map_err(invalid)?
                        .into(),
                    input: v.get_input().map_err(invalid)?.to_vec(),
                };
                id(&t.handler)?;
                id(&t.input_type)?;
                id(&t.output_type)?;
                if t.input.len() > MAX_VALUE_BYTES {
                    return Err(CodecError::Limit);
                }
                (None, Some(t))
            }
        };
        Ok(Self {
            task_id,
            request,
            transform,
            command,
            digest: Sha256::digest(bytes).into(),
        })
    }
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
    pub fn request(&self) -> Option<&Request> {
        self.request.as_ref()
    }
    pub fn transform(&self) -> Option<&Transform> {
        self.transform.as_ref()
    }
    pub fn command_bytes(&self) -> &[u8] {
        &self.command
    }
    pub fn completion(&self, response: &[u8]) -> Result<Vec<u8>> {
        self.request()
            .ok_or(CodecError::Invalid)?
            .decode_reply(response)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::completion::Builder>();
        root.set_version(contract::TASK_VERSION);
        root.set_schema_digest(&contract::TASK_DIGEST);
        root.set_task_id(self.task_id());
        root.set_input_digest(&self.digest);
        root.set_kind(wire::Kind::ContentCommand);
        root.set_response(response);
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_TASK_BYTES {
            return Err(CodecError::Limit);
        }
        Ok(bytes)
    }
    pub fn output(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        let t = self.transform().ok_or(CodecError::Invalid)?;
        if bytes.len() > MAX_VALUE_BYTES {
            return Err(CodecError::Limit);
        }
        let mut m = Builder::new_default();
        let mut r = m.init_root::<wire::completion::Builder>();
        r.set_version(contract::TASK_VERSION);
        r.set_schema_digest(&contract::TASK_DIGEST);
        r.set_task_id(self.task_id());
        r.set_input_digest(&self.digest);
        r.set_kind(wire::Kind::Transform);
        let mut value = r.init_output();
        value.set_type_id(t.output_type.as_str());
        value.set_bytes(bytes);
        let result = serialize::write_message_to_words(&m);
        if result.len() > MAX_TASK_BYTES {
            return Err(CodecError::Limit);
        }
        Ok(result)
    }
}
