use crate::contract;
use crate::{MAX_MESSAGE_BYTES, runtime_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
pub const PROTOCOL_VERSION: u16 = contract::VERSION;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    Invalid,
    Limit,
    Contract,
    Correlation,
}
type Result<T> = std::result::Result<T, CodecError>;
fn invalid<T>(_: T) -> CodecError {
    CodecError::Invalid
}
fn id(v: &str) -> Result<()> {
    if v.is_empty()
        || v.len() > 256
        || v.chars()
            .any(|c| c.is_control() || matches!(c, '/' | ':') || c == char::from(92))
    {
        Err(CodecError::Invalid)
    } else {
        Ok(())
    }
}
fn text(v: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    Ok(v.map_err(invalid)?.to_str().map_err(invalid)?.into())
}
fn digest(v: capnp::Result<&[u8]>) -> Result<[u8; 32]> {
    v.map_err(invalid)?.try_into().map_err(invalid)
}
#[derive(Debug, Clone)]
pub struct Request {
    pub request_id: String,
    pub card_id: String,
    pub action: Action,
}
#[derive(Debug, Clone)]
pub enum Action {
    Rename {
        revision: u64,
        title: String,
    },
    ReadSummary,
    QueryOperation {
        operation_id: String,
    },
    ReadAttachment {
        attachment_id: String,
        revision: u64,
        offset: u64,
        length: u32,
    },
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        id(&self.request_id)?;
        id(&self.card_id)?;
        match &self.action {
            Action::Rename { revision, title } => {
                if *revision == 0 {
                    return Err(CodecError::Invalid);
                }
                if title.len() > 16384 {
                    return Err(CodecError::Limit);
                }
            }
            Action::QueryOperation { operation_id } => id(operation_id)?,
            Action::ReadAttachment {
                attachment_id,
                revision,
                offset,
                length,
            } => {
                id(attachment_id)?;
                if *revision == 0 || *length == 0 || *length > 32768 || *offset > 200 * 1024 * 1024
                {
                    return Err(CodecError::Limit);
                }
            }
            Action::ReadSummary => {}
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::request::Builder>();
        root.set_protocol_version(contract::VERSION);
        root.set_runtime_digest(&contract::RUNTIME_DIGEST);
        root.set_content_digest(&contract::CONTENT_DIGEST);
        root.set_operation_id(self.request_id.as_str());
        match &self.action {
            Action::Rename { revision, title } => {
                let mut v = root.init_rename_card();
                v.set_card_id(self.card_id.as_str());
                v.set_expected_revision(*revision);
                v.set_title(title.as_str());
            }
            Action::ReadSummary => root.set_read_summary(self.card_id.as_str()),
            Action::QueryOperation { operation_id } => {
                let mut v = root.init_query_operation();
                v.set_card_id(self.card_id.as_str());
                v.set_operation_id(operation_id.as_str());
            }
            Action::ReadAttachment {
                attachment_id,
                revision,
                offset,
                length,
            } => {
                let mut v = root.init_read_attachment();
                v.set_card_id(self.card_id.as_str());
                v.set_attachment_id(attachment_id.as_str());
                v.set_expected_revision(*revision);
                v.set_offset(*offset);
                v.set_length(*length);
            }
        }
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(CodecError::Limit);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(CodecError::Limit);
        }
        let mut input = bytes;
        let message = serialize::read_message_from_flat_slice(
            &mut input,
            ReaderOptions {
                traversal_limit_in_words: Some(MAX_MESSAGE_BYTES / 8),
                nesting_limit: 16,
            },
        )
        .map_err(invalid)?;
        if !input.is_empty() {
            return Err(CodecError::Invalid);
        }
        let root = message
            .get_root::<wire::request::Reader>()
            .map_err(invalid)?;
        if root.get_protocol_version() != contract::VERSION
            || root.get_runtime_digest().map_err(invalid)? != contract::RUNTIME_DIGEST
            || root.get_content_digest().map_err(invalid)? != contract::CONTENT_DIGEST
        {
            return Err(CodecError::Contract);
        }
        let request_id = text(root.get_operation_id())?;
        let (card_id, action) = match root.which().map_err(invalid)? {
            wire::request::RenameCard(v) => {
                let v = v.map_err(invalid)?;
                (
                    text(v.get_card_id())?,
                    Action::Rename {
                        revision: v.get_expected_revision(),
                        title: text(v.get_title())?,
                    },
                )
            }
            wire::request::ReadSummary(v) => (text(v)?, Action::ReadSummary),
            wire::request::QueryOperation(v) => {
                let v = v.map_err(invalid)?;
                (
                    text(v.get_card_id())?,
                    Action::QueryOperation {
                        operation_id: text(v.get_operation_id())?,
                    },
                )
            }
            wire::request::ReadAttachment(v) => {
                let v = v.map_err(invalid)?;
                (
                    text(v.get_card_id())?,
                    Action::ReadAttachment {
                        attachment_id: text(v.get_attachment_id())?,
                        revision: v.get_expected_revision(),
                        offset: v.get_offset(),
                        length: v.get_length(),
                    },
                )
            }
            _ => return Err(CodecError::Invalid),
        };
        let request = Self {
            request_id,
            card_id,
            action,
        };
        request.validate()?;
        Ok(request)
    }
    pub fn decode_reply(&self, bytes: &[u8]) -> Result<Reply> {
        self.validate()?;
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(CodecError::Limit);
        }
        let mut input = bytes;
        let message = serialize::read_message_from_flat_slice(
            &mut input,
            ReaderOptions {
                traversal_limit_in_words: Some(MAX_MESSAGE_BYTES / 8),
                nesting_limit: 16,
            },
        )
        .map_err(invalid)?;
        if !input.is_empty() {
            return Err(CodecError::Invalid);
        }
        let root = message
            .get_root::<wire::response::Reader>()
            .map_err(invalid)?;
        if root.get_protocol_version() != contract::VERSION
            || root.get_runtime_digest().map_err(invalid)? != contract::RUNTIME_DIGEST
            || root.get_content_digest().map_err(invalid)? != contract::CONTENT_DIGEST
        {
            return Err(CodecError::Contract);
        }
        if text(root.get_request_id())? != self.request_id {
            return Err(CodecError::Correlation);
        }
        let reply = match root.which().map_err(invalid)? {
            wire::response::Rejected(v) => Reply::Rejected(v.map_err(invalid)?),
            wire::response::Renamed(v) => Reply::Renamed(receipt(v.map_err(invalid)?)?),
            wire::response::Summary(v) => {
                let v = v.map_err(invalid)?;
                if v.get_protocol_version() != contract::VERSION {
                    return Err(CodecError::Contract);
                }
                let value = Summary {
                    card_id: text(v.get_card_id())?,
                    type_id: text(v.get_type_id())?,
                    revision: v.get_revision(),
                    format_version: v.get_format_version(),
                    title: text(v.get_title())?,
                    preview: text(v.get_preview_text())?,
                };
                id(&value.card_id)?;
                id(&value.type_id)?;
                if value.revision == 0 || value.format_version == 0 {
                    return Err(CodecError::Invalid);
                }
                if value.title.len() > 16384 || value.preview.len() > 16384 {
                    return Err(CodecError::Limit);
                }
                Reply::Summary(value)
            }
            wire::response::OperationResult(v) => {
                let v = v.map_err(invalid)?;
                let card_id = text(v.get_card_id())?;
                let operation_id = text(v.get_operation_id())?;
                id(&card_id)?;
                id(&operation_id)?;
                let result = match v.which().map_err(invalid)? {
                    wire::operation_result::AbsentSnapshot(()) => None,
                    wire::operation_result::LocallyCommitted(v) => {
                        Some(receipt(v.map_err(invalid)?)?)
                    }
                };
                if result
                    .as_ref()
                    .is_some_and(|v| v.card_id != card_id || v.operation_id != operation_id)
                {
                    return Err(CodecError::Correlation);
                }
                Reply::OperationResult {
                    card_id,
                    operation_id,
                    result,
                }
            }
            wire::response::AttachmentChunk(v) => {
                let v = v.map_err(invalid)?;
                let p = AttachmentPart {
                    card_id: text(v.get_card_id())?,
                    attachment_id: text(v.get_attachment_id())?,
                    revision: v.get_revision(),
                    offset: v.get_offset(),
                    total_length: v.get_total_length(),
                    sha256: digest(v.get_content_sha256())?,
                    bytes: v.get_bytes().map_err(invalid)?.to_vec(),
                };
                id(&p.card_id)?;
                id(&p.attachment_id)?;
                if p.revision == 0
                    || p.total_length > 200 * 1024 * 1024
                    || p.offset > p.total_length
                    || p.bytes.len() > 32768
                    || p.bytes.len() as u64 > p.total_length - p.offset
                    || (p.bytes.is_empty() && p.offset != p.total_length)
                {
                    return Err(CodecError::Limit);
                }
                Reply::Attachment(p)
            }
            _ => return Err(CodecError::Invalid),
        };
        let matches = match (&self.action, &reply) {
            (_, Reply::Rejected(_)) => true,
            (Action::Rename { revision, .. }, Reply::Renamed(v)) => {
                v.card_id == self.card_id
                    && v.operation_id == self.request_id
                    && revision.checked_add(1) == Some(v.revision)
            }
            (Action::ReadSummary, Reply::Summary(v)) => v.card_id == self.card_id,
            (
                Action::QueryOperation { operation_id },
                Reply::OperationResult {
                    card_id,
                    operation_id: op,
                    ..
                },
            ) => card_id == &self.card_id && operation_id == op,
            (
                Action::ReadAttachment {
                    attachment_id,
                    revision,
                    offset,
                    length,
                },
                Reply::Attachment(v),
            ) => {
                v.card_id == self.card_id
                    && v.attachment_id == *attachment_id
                    && v.revision == *revision
                    && v.offset == *offset
                    && v.bytes.len() <= *length as usize
            }
            _ => false,
        };
        if !matches {
            return Err(CodecError::Correlation);
        }
        Ok(reply)
    }
}
pub use wire::Failure;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub operation_id: String,
    pub card_id: String,
    pub event_id: String,
    pub revision: u64,
    pub sha256: [u8; 32],
}
fn receipt(v: wire::commit_receipt::Reader<'_>) -> Result<Receipt> {
    let r = Receipt {
        operation_id: text(v.get_operation_id())?,
        card_id: text(v.get_card_id())?,
        event_id: text(v.get_event_id())?,
        revision: v.get_revision(),
        sha256: digest(v.get_content_sha256())?,
    };
    id(&r.operation_id)?;
    id(&r.card_id)?;
    id(&r.event_id)?;
    if r.revision == 0 {
        return Err(CodecError::Invalid);
    }
    Ok(r)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub card_id: String,
    pub type_id: String,
    pub revision: u64,
    pub format_version: u32,
    pub title: String,
    pub preview: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentPart {
    pub card_id: String,
    pub attachment_id: String,
    pub revision: u64,
    pub offset: u64,
    pub total_length: u64,
    pub sha256: [u8; 32],
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Renamed(Receipt),
    Summary(Summary),
    Rejected(Failure),
    OperationResult {
        card_id: String,
        operation_id: String,
        result: Option<Receipt>,
    },
    Attachment(AttachmentPart),
}
