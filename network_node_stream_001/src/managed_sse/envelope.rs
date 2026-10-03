//! Metadata-preserving source payload, independent of the existing channel ABI.
use super::{Error, Result};
use crate::sse_event_capnp as wire;
use capnp::{message::{Builder, ReaderOptions}, serialize};
use sha2::{Digest, Sha256};
pub const VERSION: u16 = 1;
pub const MAX_ENVELOPE_BYTES: usize = 64 * 1024;
pub fn schema_digest() -> [u8; 32] {
    Sha256::digest(include_bytes!("../../schemas/sse_event.capnp")).into()
}
// No Debug: event content and provider IDs may contain private data.
#[derive(Clone, PartialEq, Eq)]
pub struct EventEnvelope {
    pub data: String,
    pub event: String,
    pub id: String,
    pub retry: Option<u64>,
}
impl From<crate::sse::Event> for EventEnvelope {
    fn from(value: crate::sse::Event) -> Self {
        Self { data: value.data, event: value.kind, id: value.id, retry: value.retry }
    }
}
impl EventEnvelope {
    pub fn encode(&self) -> Result<Vec<u8>> {
        // Check aggregate input before constructing a message; encoded overhead
        // is separately bounded below. No metadata is truncated to fit a frame.
        let total = self.data.len().checked_add(self.event.len())
            .and_then(|n| n.checked_add(self.id.len())).ok_or(Error::Limit)?;
        if total > MAX_ENVELOPE_BYTES { return Err(Error::Limit); }
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::event::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&schema_digest());
        root.set_data(&self.data);
        root.set_event(&self.event);
        root.set_id(&self.id);
        root.set_has_retry(self.retry.is_some());
        root.set_retry(self.retry.unwrap_or(0));
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_ENVELOPE_BYTES { return Err(Error::Limit); }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_ENVELOPE_BYTES { return Err(Error::Limit); }
        let mut remaining = bytes;
        let message = serialize::read_message(&mut remaining, ReaderOptions {
            traversal_limit_in_words: Some(2 * MAX_ENVELOPE_BYTES / 8), nesting_limit: 8,
        }).map_err(|_| Error::Invalid)?;
        if !remaining.is_empty() { return Err(Error::Invalid); }
        let root = message.get_root::<wire::event::Reader>().map_err(|_| Error::Invalid)?;
        if root.total_size().map_err(|_| Error::Invalid)?.cap_count != 0
            || root.get_version() != VERSION
            || root.get_schema_sha256().map_err(|_| Error::Invalid)? != schema_digest()
            || (!root.get_has_retry() && root.get_retry() != 0)
        { return Err(Error::Invalid); }
        let data = root.get_data().map_err(|_| Error::Invalid)?;
        let event = root.get_event().map_err(|_| Error::Invalid)?;
        let id = root.get_id().map_err(|_| Error::Invalid)?;
        // Shared pointers cannot force multiple large owned allocations first.
        let total = data.len().checked_add(event.len())
            .and_then(|n| n.checked_add(id.len())).ok_or(Error::Limit)?;
        if total > MAX_ENVELOPE_BYTES { return Err(Error::Limit); }
        Ok(Self {
            data: data.to_str().map_err(|_| Error::Invalid)?.to_owned(),
            event: event.to_str().map_err(|_| Error::Invalid)?.to_owned(),
            id: id.to_str().map_err(|_| Error::Invalid)?.to_owned(),
            retry: root.get_has_retry().then(|| root.get_retry()),
        })
    }
}
