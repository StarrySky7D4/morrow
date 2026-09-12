//! Runtime task envelopes. Core receipts require actual reply matching; transform outputs remain plugin data.
use crate::{
    Error, Result, identity,
    response::{Outcome, Response},
    runtime::Command,
    task_capnp as wire,
};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub const VERSION: u16 = 3;
pub const MAX_FAILURE_MESSAGE_BYTES: usize = 1024;
pub use wire::FailureCode;
pub const MAX_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_TASK_BYTES: usize = 128 * 1024;
pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/task.capnp"))
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid("task message")
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginFailure {
    pub code: FailureCode,
    pub message: String,
}
impl PluginFailure {
    fn validate(&self) -> Result<()> {
        if self.message.is_empty()
            || self.message.len() > MAX_FAILURE_MESSAGE_BYTES
            || self.message.chars().any(char::is_control)
        {
            return Err(Error::Invalid("plugin failure message"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformResult {
    Output(TransformOutput),
    Failure(PluginFailure),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transform {
    pub handler: String,
    pub input_type: String,
    pub output_type: String,
    pub input: Vec<u8>,
}
impl Transform {
    fn validate(&self) -> Result<()> {
        identity(&self.handler)?;
        identity(&self.input_type)?;
        identity(&self.output_type)?;
        if self.input.len() > MAX_VALUE_BYTES {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
/// Correlated plugin-produced data, not a core receipt or proof of semantic correctness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformOutput {
    pub type_id: String,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone)]
pub struct Invocation {
    bytes: Vec<u8>,
    task_id: String,
    command: Option<Command>,
    transform: Option<Transform>,
    command_bytes: Vec<u8>,
    digest: [u8; 32],
}
impl Invocation {
    pub fn new(task_id: &str, command: &Command) -> Result<Self> {
        identity(task_id)?;
        let bytes = command.encode()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::invocation::Builder>();
        root.set_version(VERSION);
        root.set_schema_digest(&schema_digest());
        root.set_task_id(task_id);
        root.set_kind(wire::Kind::ContentCommand);
        root.set_command(&bytes);
        Self::decode(&serialize::write_message_to_words(&message))
    }
    pub fn new_transform(task_id: &str, transform: Transform) -> Result<Self> {
        identity(task_id)?;
        transform.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::invocation::Builder>();
        root.set_version(VERSION);
        root.set_schema_digest(&schema_digest());
        root.set_task_id(task_id);
        root.set_kind(wire::Kind::Transform);
        let mut value = root.init_transform();
        value.set_handler(transform.handler.as_str());
        value.set_input_type(transform.input_type.as_str());
        value.set_output_type(transform.output_type.as_str());
        value.set_input(&transform.input);
        Self::decode(&serialize::write_message_to_words(&message))
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_TASK_BYTES {
            return Err(Error::Limit);
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
            return Err(invalid(()));
        }
        let root = reader
            .get_root::<wire::invocation::Reader>()
            .map_err(invalid)?;
        if root.get_version() != VERSION
            || root.get_schema_digest().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        let task_id = root
            .get_task_id()
            .map_err(invalid)?
            .to_str()
            .map_err(invalid)?
            .to_owned();
        identity(&task_id)?;
        let command_bytes = root.get_command().map_err(invalid)?.to_vec();
        let (command, transform) = match root.get_kind().map_err(invalid)? {
            wire::Kind::ContentCommand => {
                if root.has_transform() {
                    return Err(invalid(()));
                }
                (Some(Command::decode(&command_bytes)?), None)
            }
            wire::Kind::Transform => {
                if !command_bytes.is_empty() || !root.has_transform() {
                    return Err(invalid(()));
                }
                let t = root.get_transform().map_err(invalid)?;
                let value = Transform {
                    handler: t
                        .get_handler()
                        .map_err(invalid)?
                        .to_str()
                        .map_err(invalid)?
                        .into(),
                    input_type: t
                        .get_input_type()
                        .map_err(invalid)?
                        .to_str()
                        .map_err(invalid)?
                        .into(),
                    output_type: t
                        .get_output_type()
                        .map_err(invalid)?
                        .to_str()
                        .map_err(invalid)?
                        .into(),
                    input: t.get_input().map_err(invalid)?.to_vec(),
                };
                value.validate()?;
                (None, Some(value))
            }
        };
        Ok(Self {
            bytes: bytes.into(),
            task_id,
            command,
            transform,
            command_bytes,
            digest: Sha256::digest(bytes).into(),
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn task_id(&self) -> &str {
        &self.task_id
    }
    pub fn command(&self) -> Option<&Command> {
        self.command.as_ref()
    }
    pub fn transform(&self) -> Option<&Transform> {
        self.transform.as_ref()
    }
    pub fn command_bytes(&self) -> &[u8] {
        &self.command_bytes
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    fn response(&self, bytes: &[u8]) -> Result<Response> {
        let command = self.command().ok_or(Error::Invalid("wrong task kind"))?;
        let response = Response::decode(bytes)?;
        if response.request_id != command.request_id() {
            return Err(Error::Invalid("task correlation"));
        }
        let matched = match (command, &response.outcome) {
            (_, Outcome::Rejected(_)) => true,
            (Command::CreateContent(q), Outcome::ContentCommitted(r)) => {
                q.card_id == r.card_id && q.operation_id == r.operation_id && r.revision == 1
            }
            (Command::EditContent(q), Outcome::ContentCommitted(r)) => {
                q.card_id == r.card_id
                    && q.operation_id == r.operation_id
                    && q.expected_revision.checked_add(1) == Some(r.revision)
            }
            (Command::ReadContent(q), Outcome::ContentChunk(r)) => {
                q.card_id == r.card_id
                    && q.expected_revision == r.revision
                    && q.offset == r.offset
                    && r.bytes.len() <= q.length as usize
            }
            (Command::Rename(q), Outcome::Renamed(r)) => {
                q.card_id == r.card_id
                    && q.operation_id == r.operation_id
                    && q.expected_revision.checked_add(1) == Some(r.revision)
            }
            (Command::ReadSummary { card_id, .. }, Outcome::Summary(r)) => *card_id == r.id,
            (
                Command::QueryOperation {
                    card_id,
                    operation_id,
                    ..
                },
                Outcome::OperationResult {
                    card_id: c,
                    operation_id: o,
                    ..
                },
            ) => card_id == c && operation_id == o,
            (Command::ReadAttachment(q), Outcome::AttachmentChunk(r)) => {
                q.card_id == r.card_id
                    && q.attachment_id == r.attachment_id
                    && q.expected_revision == r.revision
                    && q.offset == r.offset
                    && r.bytes.len() <= q.length as usize
            }
            _ => false,
        };
        if !matched {
            return Err(Error::Invalid("task correlation"));
        }
        Ok(response)
    }
    /// Host fixture helper. Encoding a completion alone is not proof of submission.
    pub fn completion(&self, response: &[u8]) -> Result<Vec<u8>> {
        self.response(response)?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::completion::Builder>();
        root.set_version(VERSION);
        root.set_schema_digest(&schema_digest());
        root.set_task_id(self.task_id());
        root.set_input_digest(&self.digest);
        root.set_kind(wire::Kind::ContentCommand);
        root.set_response(response);
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_TASK_BYTES {
            return Err(Error::Limit);
        }
        Ok(bytes)
    }
    pub fn verify_completion(&self, bytes: &[u8], actual_response: &[u8]) -> Result<Response> {
        if bytes.len() > MAX_TASK_BYTES {
            return Err(Error::Limit);
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
            return Err(invalid(()));
        }
        let root = reader
            .get_root::<wire::completion::Reader>()
            .map_err(invalid)?;
        if root.get_version() != VERSION
            || root.get_schema_digest().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        if root
            .get_task_id()
            .map_err(invalid)?
            .to_str()
            .map_err(invalid)?
            != self.task_id
            || root.get_input_digest().map_err(invalid)? != self.digest
            || root.get_kind().map_err(invalid)? != wire::Kind::ContentCommand
            || root.has_output()
            || root.has_failure()
            || root.get_response().map_err(invalid)? != actual_response
        {
            return Err(Error::Invalid("task correlation"));
        }
        self.response(actual_response)
    }
    /// Host fixture helper; output bytes remain untrusted plugin data.
    pub fn output_completion(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        let t = self.transform().ok_or(Error::Invalid("wrong task kind"))?;
        if bytes.len() > MAX_VALUE_BYTES {
            return Err(Error::Limit);
        }
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::completion::Builder>();
        root.set_version(VERSION);
        root.set_schema_digest(&schema_digest());
        root.set_task_id(self.task_id());
        root.set_input_digest(&self.digest);
        root.set_kind(wire::Kind::Transform);
        let mut output = root.init_output();
        output.set_type_id(t.output_type.as_str());
        output.set_bytes(bytes);
        let result = serialize::write_message_to_words(&message);
        if result.len() > MAX_TASK_BYTES {
            return Err(Error::Limit);
        }
        Ok(result)
    }
    /// Host fixture helper; the message is plugin-supplied, not a core failure receipt.
    pub fn failure_completion(&self, failure: &PluginFailure) -> Result<Vec<u8>> {
        self.transform().ok_or(Error::Invalid("wrong task kind"))?;
        failure.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::completion::Builder>();
        root.set_version(VERSION);
        root.set_schema_digest(&schema_digest());
        root.set_task_id(self.task_id());
        root.set_input_digest(&self.digest);
        root.set_kind(wire::Kind::Transform);
        let mut f = root.init_failure();
        f.set_code(failure.code);
        f.set_message(failure.message.as_str());
        Ok(serialize::write_message_to_words(&message))
    }
    pub fn verify_output(&self, bytes: &[u8]) -> Result<TransformOutput> {
        match self.verify_transform_result(bytes)? {
            TransformResult::Output(output) => Ok(output),
            TransformResult::Failure(_) => Err(Error::Invalid("plugin reported failure")),
        }
    }
    pub fn verify_transform_result(&self, bytes: &[u8]) -> Result<TransformResult> {
        let t = self.transform().ok_or(Error::Invalid("wrong task kind"))?;
        if bytes.len() > MAX_TASK_BYTES {
            return Err(Error::Limit);
        }
        let mut rest = bytes;
        let message = serialize::read_message_from_flat_slice(
            &mut rest,
            ReaderOptions {
                traversal_limit_in_words: Some(MAX_TASK_BYTES / 8),
                nesting_limit: 16,
            },
        )
        .map_err(invalid)?;
        if !rest.is_empty() {
            return Err(invalid(()));
        }
        let r = message
            .get_root::<wire::completion::Reader>()
            .map_err(invalid)?;
        if r.get_version() != VERSION || r.get_schema_digest().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        if r.get_kind().map_err(invalid)? != wire::Kind::Transform
            || r.has_output() == r.has_failure()
            || !r.get_response().map_err(invalid)?.is_empty()
            || r.get_task_id()
                .map_err(invalid)?
                .to_str()
                .map_err(invalid)?
                != self.task_id
            || r.get_input_digest().map_err(invalid)? != self.digest
        {
            return Err(Error::Invalid("task correlation"));
        }
        if r.has_failure() {
            let f = r.get_failure().map_err(invalid)?;
            let failure = PluginFailure {
                code: f.get_code().map_err(invalid)?,
                message: f
                    .get_message()
                    .map_err(invalid)?
                    .to_str()
                    .map_err(invalid)?
                    .into(),
            };
            failure.validate()?;
            return Ok(TransformResult::Failure(failure));
        }
        let output = r.get_output().map_err(invalid)?;
        let type_id = output
            .get_type_id()
            .map_err(invalid)?
            .to_str()
            .map_err(invalid)?;
        let bytes = output.get_bytes().map_err(invalid)?;
        if type_id != t.output_type {
            return Err(Error::Invalid("output type"));
        }
        if bytes.len() > MAX_VALUE_BYTES {
            return Err(Error::Limit);
        }
        Ok(TransformResult::Output(TransformOutput {
            type_id: type_id.into(),
            bytes: bytes.into(),
        }))
    }
}
