//! Internal command codec. The host must bind identity and authorize separately.
use crate::{Error, Result, identity, runtime_capnp, title};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
pub const PROTOCOL_VERSION: u16 = 6;
pub fn schema_digest(source: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    // Git checkout line endings do not change the contract identity.
    let canonical = String::from_utf8_lossy(source).replace("\r\n", "\n");
    Sha256::digest(canonical.as_bytes()).into()
}
pub fn runtime_digest() -> [u8; 32] {
    schema_digest(RUNTIME_SCHEMA)
}
pub fn content_digest() -> [u8; 32] {
    schema_digest(crate::content::CONTENT_SCHEMA)
}
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const RUNTIME_SCHEMA: &[u8] = include_bytes!("../schemas/runtime.capnp");
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameRequest {
    pub operation_id: String,
    pub card_id: String,
    pub expected_revision: u64,
    pub title: String,
}
impl RenameRequest {
    pub fn validate(&self) -> Result<()> {
        identity(&self.operation_id)?;
        identity(&self.card_id)?;
        title(&self.title)?;
        if self.expected_revision == 0 {
            return Err(Error::Invalid("expected revision"));
        }
        Ok(())
    }
    /// Build an edit proposal only; the future host owns authorization and commit.
    pub fn propose(&self, card: &crate::content::CardRecord) -> Result<crate::content::CardRecord> {
        self.validate()?;
        if self.card_id != card.summary().id {
            return Err(Error::Invalid("card identity mismatch"));
        }
        card.with_title(self.expected_revision, &self.title)
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        Command::Rename(self.clone()).encode()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        match Command::decode(bytes)? {
            Command::Rename(request) => Ok(request),
            _ => Err(Error::Invalid("expected rename")),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadAttachment {
    pub request_id: String,
    pub card_id: String,
    pub attachment_id: String,
    pub expected_revision: u64,
    pub offset: u64,
    pub length: u32,
}
impl ReadAttachment {
    pub fn validate(&self) -> Result<()> {
        identity(&self.request_id)?;
        identity(&self.card_id)?;
        identity(&self.attachment_id)?;
        if self.expected_revision == 0
            || self.offset > crate::attachment::MAX_BLOB_BYTES
            || self.length == 0
            || self.length > crate::attachment::MAX_READ_BYTES
        {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Rename(RenameRequest),
    ReadAttachment(ReadAttachment),
    ReadSummary {
        request_id: String,
        card_id: String,
    },
    QueryOperation {
        request_id: String,
        card_id: String,
        operation_id: String,
    },
}
impl Command {
    pub fn request_id(&self) -> &str {
        match self {
            Self::Rename(v) => &v.operation_id,
            Self::ReadAttachment(v) => &v.request_id,
            Self::ReadSummary { request_id, .. } | Self::QueryOperation { request_id, .. } => {
                request_id
            }
        }
    }
    pub fn card_id(&self) -> &str {
        match self {
            Self::Rename(v) => &v.card_id,
            Self::ReadAttachment(v) => &v.card_id,
            Self::ReadSummary { card_id, .. } | Self::QueryOperation { card_id, .. } => card_id,
        }
    }
    pub fn validate(&self) -> Result<()> {
        identity(self.request_id())?;
        identity(self.card_id())?;
        match self {
            Self::Rename(v) => v.validate(),
            Self::ReadAttachment(v) => v.validate(),
            Self::QueryOperation { operation_id, .. } => identity(operation_id),
            _ => Ok(()),
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<runtime_capnp::request::Builder>();
        root.set_protocol_version(PROTOCOL_VERSION);
        root.set_runtime_digest(&runtime_digest());
        root.set_content_digest(&content_digest());
        root.set_operation_id(self.request_id());
        match self {
            Self::Rename(value) => {
                let mut rename = root.init_rename_card();
                rename.set_card_id(value.card_id.as_str());
                rename.set_expected_revision(value.expected_revision);
                rename.set_title(value.title.as_str());
            }
            Self::ReadAttachment(v) => {
                let mut out = root.init_read_attachment();
                out.set_card_id(v.card_id.as_str());
                out.set_attachment_id(v.attachment_id.as_str());
                out.set_expected_revision(v.expected_revision);
                out.set_offset(v.offset);
                out.set_length(v.length);
            }
            Self::ReadSummary { card_id, .. } => root.set_read_summary(card_id.as_str()),
            Self::QueryOperation {
                card_id,
                operation_id,
                ..
            } => {
                let mut out = root.init_query_operation();
                out.set_card_id(card_id.as_str());
                out.set_operation_id(operation_id.as_str());
            }
        }
        bounded_message(&message)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let message = read_message(bytes)?;
        let root = message
            .get_root::<runtime_capnp::request::Reader>()
            .map_err(|_| Error::Invalid("request"))?;
        check_contract(
            root.get_protocol_version(),
            root.get_runtime_digest(),
            root.get_content_digest(),
        )?;
        let request_id = read_text(root.get_operation_id())?;
        let value = match root.which().map_err(|_| Error::Invalid("operation"))? {
            runtime_capnp::request::RenameCard(value) => {
                let value = value.map_err(|_| Error::Invalid("rename"))?;
                Self::Rename(RenameRequest {
                    operation_id: request_id,
                    card_id: read_text(value.get_card_id())?,
                    expected_revision: value.get_expected_revision(),
                    title: read_text(value.get_title())?,
                })
            }
            runtime_capnp::request::ReadSummary(value) => Self::ReadSummary {
                request_id,
                card_id: read_text(value)?,
            },
            runtime_capnp::request::QueryOperation(value) => {
                let value = value.map_err(|_| Error::Invalid("query operation"))?;
                Self::QueryOperation {
                    request_id,
                    card_id: read_text(value.get_card_id())?,
                    operation_id: read_text(value.get_operation_id())?,
                }
            }
            runtime_capnp::request::ReadAttachment(value) => {
                let v = value.map_err(|_| Error::Invalid("attachment read"))?;
                Self::ReadAttachment(ReadAttachment {
                    request_id,
                    card_id: read_text(v.get_card_id())?,
                    attachment_id: read_text(v.get_attachment_id())?,
                    expected_revision: v.get_expected_revision(),
                    offset: v.get_offset(),
                    length: v.get_length(),
                })
            }
            _ => return Err(Error::Invalid("operation")),
        };
        value.validate()?;
        Ok(value)
    }
}
pub(crate) fn read_text(value: capnp::Result<capnp::text::Reader<'_>>) -> Result<String> {
    value
        .and_then(|v| v.to_str().map_err(Into::into))
        .map(str::to_owned)
        .map_err(|_| Error::Invalid("text"))
}
pub(crate) fn check_contract(
    version: u16,
    runtime: capnp::Result<&[u8]>,
    content: capnp::Result<&[u8]>,
) -> Result<()> {
    if version != PROTOCOL_VERSION
        || runtime.map_err(|_| Error::Invalid("runtime digest"))? != runtime_digest()
        || content.map_err(|_| Error::Invalid("content digest"))? != content_digest()
    {
        return Err(Error::UnsupportedVersion);
    }
    Ok(())
}
pub(crate) fn bounded_message(message: &Builder<capnp::message::HeapAllocator>) -> Result<Vec<u8>> {
    let bytes = serialize::write_message_to_words(message);
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
pub(crate) fn read_message(
    bytes: &[u8],
) -> Result<capnp::message::Reader<serialize::BufferSegments<&[u8]>>> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(Error::Limit);
    }
    let mut remaining = bytes;
    let message = serialize::read_message_from_flat_slice(
        &mut remaining,
        ReaderOptions {
            traversal_limit_in_words: Some(MAX_MESSAGE_BYTES / 8),
            nesting_limit: 16,
        },
    )
    .map_err(|_| Error::Invalid("capnp framing"))?;
    if !remaining.is_empty() {
        return Err(Error::Invalid("trailing message"));
    }
    Ok(message)
}
