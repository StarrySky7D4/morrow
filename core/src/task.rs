//! Runtime task envelopes. A completion is trusted only when matched to the actual core reply.
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
pub const VERSION: u16 = 1;
pub const MAX_TASK_BYTES: usize = 128 * 1024;
pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/task.capnp"))
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid("task message")
}
#[derive(Debug, Clone)]
pub struct Invocation {
    bytes: Vec<u8>,
    task_id: String,
    command: Command,
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
        root.set_command(&bytes);
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
        let command = Command::decode(&command_bytes)?;
        Ok(Self {
            bytes: bytes.into(),
            task_id,
            command,
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
    pub fn command(&self) -> &Command {
        &self.command
    }
    pub fn command_bytes(&self) -> &[u8] {
        &self.command_bytes
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    fn response(&self, bytes: &[u8]) -> Result<Response> {
        let response = Response::decode(bytes)?;
        if response.request_id != self.command.request_id() {
            return Err(Error::Invalid("task correlation"));
        }
        let matched = match (&self.command, &response.outcome) {
            (_, Outcome::Rejected(_)) => true,
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
            || root.get_response().map_err(invalid)? != actual_response
        {
            return Err(Error::Invalid("task correlation"));
        }
        self.response(actual_response)
    }
}
