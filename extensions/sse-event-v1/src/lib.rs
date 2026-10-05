//! Independent typed SSE envelopes carried by the existing opaque channel.
//! Codec identity conveys no source, network, Store, grant or replay authority.
#![deny(unsafe_op_in_unsafe_fn)]
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
#[cfg(feature = "c-transport")]
pub use morrow_plugin_sdk as transport;
use std::fmt;
#[allow(unsafe_op_in_unsafe_fn)]
mod sse_event_capnp {
    include!(concat!(env!("OUT_DIR"), "/sse_event_capnp.rs"));
}
pub mod ffi;
pub use ffi::{
    CEvent, mse_event_v1_decode, mse_event_v1_encode, mse_event_v1_schema_digest, mse_event_v1_set,
};
/// Codec identity only; never a required package feature or runtime import.
pub const PROFILE: &str = "sse-event-v1";
pub const VERSION: u16 = 1;
pub const MAX_ENVELOPE_BYTES: usize = 65536;
/// Sum of UTF-8 field byte lengths, independently of serialized wire overhead.
pub const MAX_FIELD_BYTES: usize = 65536;
pub const TRAVERSAL_LIMIT_WORDS: usize = 16384;
pub const NESTING_LIMIT: i32 = 8;
pub const SCHEMA: &[u8] = include_bytes!("../contracts/sse_event.capnp");
pub const SCHEMA_DIGEST: [u8; 32] = [
    190, 193, 225, 116, 183, 88, 110, 70, 21, 91, 96, 71, 24, 103, 144, 11, 249, 31, 31, 177, 24,
    73, 230, 118, 25, 22, 191, 195, 202, 124, 152, 99,
];
pub const fn schema_digest() -> [u8; 32] {
    SCHEMA_DIGEST
}
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid = 1,
    Contract = 2,
    Limit = 3,
    Utf8 = 4,
    Buffer = 5,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SSE envelope {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, PartialEq, Eq)]
pub struct Event {
    pub data: String,
    pub event: String,
    pub id: String,
    pub retry: Option<u64>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EventRef<'a> {
    pub data: &'a str,
    pub event: &'a str,
    pub id: &'a str,
    pub retry: Option<u64>,
}
impl fmt::Debug for EventRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Event")
            .field("data_bytes", &self.data.len())
            .field("event_bytes", &self.event.len())
            .field("id_bytes", &self.id.len())
            .field("has_retry", &self.retry.is_some())
            .finish()
    }
}
impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.as_ref(), f)
    }
}
fn check_lengths(data: usize, event: usize, id: usize) -> Result<()> {
    if data
        .checked_add(event)
        .and_then(|v| v.checked_add(id))
        .filter(|v| *v <= MAX_FIELD_BYTES)
        .is_none()
    {
        return Err(Error::Limit);
    }
    Ok(())
}
impl EventRef<'_> {
    /// Full UTF-8 metadata is preserved. Upstream parser truncation/resume policy is
    /// deliberately absent from this envelope codec.
    pub fn validate(&self) -> Result<()> {
        check_lengths(self.data.len(), self.event.len(), self.id.len())
    }
    /// Uses the same default allocator as native. A valid aggregate, or an accepted
    /// noncanonical wire, may exceed the independent wire cap when encoded.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<sse_event_capnp::event::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&SCHEMA_DIGEST);
        root.set_data(self.data);
        root.set_event(self.event);
        root.set_id(self.id);
        root.set_has_retry(self.retry.is_some());
        root.set_retry(self.retry.unwrap_or(0));
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(Error::Limit);
        }
        Ok(bytes)
    }
}
impl Event {
    pub fn as_ref(&self) -> EventRef<'_> {
        EventRef {
            data: &self.data,
            event: &self.event,
            id: &self.id,
            retry: self.retry,
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.as_ref().validate()
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.as_ref().encode()
    }
    /// Bounded owned decode. Segment-table allocation is bounded by ReaderOptions;
    /// the logical aggregate is checked before any owned field allocation.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(Error::Limit);
        }
        let mut remaining = bytes;
        let message = serialize::read_message(
            &mut remaining,
            ReaderOptions {
                traversal_limit_in_words: Some(TRAVERSAL_LIMIT_WORDS),
                nesting_limit: NESTING_LIMIT,
            },
        )
        .map_err(|_| Error::Invalid)?;
        if !remaining.is_empty() {
            return Err(Error::Invalid);
        }
        let root = message
            .get_root::<sse_event_capnp::event::Reader>()
            .map_err(|_| Error::Invalid)?;
        if root.total_size().map_err(|_| Error::Invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(|_| Error::Invalid)? != SCHEMA_DIGEST
        {
            return Err(Error::Contract);
        }
        if !root.get_has_retry() && root.get_retry() != 0 {
            return Err(Error::Invalid);
        }
        let data = root.get_data().map_err(|_| Error::Invalid)?;
        let event = root.get_event().map_err(|_| Error::Invalid)?;
        let id = root.get_id().map_err(|_| Error::Invalid)?;
        check_lengths(data.len(), event.len(), id.len())?;
        // Validate every string before allocating any owned field.
        let data = data.to_str().map_err(|_| Error::Utf8)?;
        let event = event.to_str().map_err(|_| Error::Utf8)?;
        let id = id.to_str().map_err(|_| Error::Utf8)?;
        Ok(Self {
            data: data.to_owned(),
            event: event.to_owned(),
            id: id.to_owned(),
            retry: root.get_has_retry().then(|| root.get_retry()),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_metadata_utf8_and_retry_extremes_are_preserved_without_debug_leaks() {
        for retry in [None, Some(0), Some(u64::MAX)] {
            let view = EventRef {
                data: "雪\0\n🙂",
                event: "private-event\r\n",
                id: "private-id\0\r\n",
                retry,
            };
            let decoded = Event::decode(&view.encode().unwrap()).unwrap();
            assert_eq!(decoded.as_ref(), view);
            let debug = format!("{decoded:?}");
            assert!(!debug.contains("雪"));
            assert!(!debug.contains("private"));
            assert!(!debug.contains(&u64::MAX.to_string()));
        }
    }
    #[test]
    fn aggregate_and_real_default_allocator_wire_overhead_are_independent() {
        let large = "x".repeat(MAX_FIELD_BYTES - 256);
        let view = EventRef {
            data: &large,
            event: "event",
            id: "id",
            retry: Some(42),
        };
        let raw = view.encode().unwrap();
        assert!(u32::from_le_bytes(raw[..4].try_into().unwrap()) > 0);
        assert_eq!(Event::decode(&raw).unwrap().as_ref(), view);
        let full = "x".repeat(MAX_FIELD_BYTES);
        let view = EventRef {
            data: &full,
            event: "",
            id: "",
            retry: None,
        };
        assert_eq!(view.validate(), Ok(()));
        assert_eq!(view.encode(), Err(Error::Limit));
        assert_eq!(
            EventRef {
                data: &full,
                event: "x",
                id: "",
                retry: None
            }
            .validate(),
            Err(Error::Limit)
        );
    }
    #[test]
    fn stale_retry_contract_trailing_and_utf8_reject() {
        let mut message = Builder::new_default();
        {
            let mut root = message.init_root::<sse_event_capnp::event::Builder>();
            root.set_version(VERSION);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_data("secret");
            root.set_retry(1);
        }
        assert_eq!(
            Event::decode(&serialize::write_message_to_words(&message)),
            Err(Error::Invalid)
        );
        {
            let mut root = message
                .get_root::<sse_event_capnp::event::Builder>()
                .unwrap();
            root.set_has_retry(true);
        }
        let raw = serialize::write_message_to_words(&message);
        assert!(Event::decode(&raw).is_ok());
        let mut trailing = raw.clone();
        trailing.extend([0; 8]);
        assert_eq!(Event::decode(&trailing), Err(Error::Invalid));
        let mut invalid_utf8 = raw.clone();
        let offset = invalid_utf8
            .windows(6)
            .position(|v| v == b"secret")
            .unwrap();
        invalid_utf8[offset] = 255;
        assert_eq!(Event::decode(&invalid_utf8), Err(Error::Utf8));
        let mut altered = raw;
        altered[16] = 2;
        assert_eq!(Event::decode(&altered), Err(Error::Contract));
    }
}
