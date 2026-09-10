//! Internal command codec. The host must bind identity and authorize separately.
use crate::{Error, Result, identity, runtime_capnp, title};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
pub const PROTOCOL_VERSION: u16 = 2;
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
        self.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<runtime_capnp::request::Builder>();
        root.set_protocol_version(PROTOCOL_VERSION);
        root.set_runtime_digest(&runtime_digest());
        root.set_content_digest(&content_digest());
        root.set_operation_id(self.operation_id.as_str());
        let mut rename = root.init_rename_card();
        rename.set_card_id(self.card_id.as_str());
        rename.set_expected_revision(self.expected_revision);
        rename.set_title(self.title.as_str());
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Limit);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Limit);
        }
        let mut remaining = bytes;
        let options = ReaderOptions {
            traversal_limit_in_words: Some(MAX_MESSAGE_BYTES / 8),
            nesting_limit: 16,
        };
        let message = serialize::read_message_from_flat_slice(&mut remaining, options)
            .map_err(|_| Error::Invalid("capnp framing"))?;
        if !remaining.is_empty() {
            return Err(Error::Invalid("trailing message"));
        }
        let root = message
            .get_root::<runtime_capnp::request::Reader>()
            .map_err(|_| Error::Invalid("request"))?;
        if root.get_protocol_version() != PROTOCOL_VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if root
            .get_runtime_digest()
            .map_err(|_| Error::Invalid("runtime digest"))?
            != runtime_digest()
            || root
                .get_content_digest()
                .map_err(|_| Error::Invalid("content digest"))?
                != content_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        let operation_id = root
            .get_operation_id()
            .and_then(|s| s.to_str().map_err(Into::into))
            .map_err(|_| Error::Invalid("operation id"))?
            .to_owned();
        let rename = match root.which().map_err(|_| Error::Invalid("operation"))? {
            runtime_capnp::request::Unsupported(()) => return Err(Error::Invalid("operation")),
            runtime_capnp::request::RenameCard(value) => {
                value.map_err(|_| Error::Invalid("rename"))?
            }
        };
        let request = Self {
            operation_id,
            card_id: rename
                .get_card_id()
                .and_then(|s| s.to_str().map_err(Into::into))
                .map_err(|_| Error::Invalid("card id"))?
                .to_owned(),
            expected_revision: rename.get_expected_revision(),
            title: rename
                .get_title()
                .and_then(|s| s.to_str().map_err(Into::into))
                .map_err(|_| Error::Invalid("title"))?
                .to_owned(),
        };
        request.validate()?;
        Ok(request)
    }
}
