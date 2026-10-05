//! Independent typed payloads for the existing opaque channel transport.
//! No socket, Store, network, runtime, grant, replay or source approval API.
//! A schema digest establishes byte identity, never authority or qualification.
#![deny(unsafe_op_in_unsafe_fn)]
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
#[cfg(feature = "c-transport")]
pub use morrow_plugin_sdk as transport;
use std::fmt;

#[allow(unsafe_op_in_unsafe_fn)]
mod ws_message_capnp {
    include!(concat!(env!("OUT_DIR"), "/ws_message_capnp.rs"));
}
pub mod ffi;
pub use ffi::{
    CMessage, mws_message_v1_decode, mws_message_v1_encode, mws_message_v1_schema_digest,
    mws_message_v1_set,
};

/// Codec identity only: this is NOT a required package feature or import.
pub const PROFILE: &str = "ws-message-v1";
pub const VERSION: u16 = 1;
pub const MAX_ENVELOPE_BYTES: usize = 64 * 1024;
/// Semantic payload ceiling. Encoding must ALSO fit the serialized envelope cap.
pub const MAX_PAYLOAD_BYTES: usize = MAX_ENVELOPE_BYTES;
pub const MAX_CONTROL_BYTES: usize = 125;
pub const MAX_CLOSE_BYTES: usize = 123;
pub const TRAVERSAL_LIMIT_WORDS: usize = 2 * MAX_ENVELOPE_BYTES / 8;
pub const NESTING_LIMIT: i32 = 8;
pub const SCHEMA: &[u8] = include_bytes!("../contracts/ws_message.capnp");
pub const SCHEMA_DIGEST: [u8; 32] = [
    45, 47, 59, 25, 19, 6, 11, 212, 16, 211, 188, 96, 131, 98, 192, 29, 40, 207, 125, 14, 60, 28,
    220, 130, 165, 41, 58, 188, 170, 105, 110, 109,
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
        write!(f, "WebSocket payload {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Text = 0,
    Binary = 1,
    Ping = 2,
    Pong = 3,
    Close = 4,
}
impl TryFrom<u32> for MessageKind {
    type Error = Error;
    fn try_from(value: u32) -> Result<Self> {
        match value {
            0 => Ok(Self::Text),
            1 => Ok(Self::Binary),
            2 => Ok(Self::Ping),
            3 => Ok(Self::Pong),
            4 => Ok(Self::Close),
            _ => Err(Error::Invalid),
        }
    }
}

/// A bounded owned message. Debug exposes kind, length and code only.
#[derive(Clone, PartialEq, Eq)]
pub struct Message {
    pub kind: MessageKind,
    pub payload: Vec<u8>,
    pub close_code: Option<u16>,
}
/// Borrowed outbound payload. Encoding owns the generated wire independently.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MessageRef<'a> {
    pub kind: MessageKind,
    pub payload: &'a [u8],
    pub close_code: Option<u16>,
}
impl fmt::Debug for MessageRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field("kind", &self.kind)
            .field("payload_bytes", &self.payload.len())
            .field("close_code", &self.close_code)
            .finish()
    }
}
impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.as_ref(), f)
    }
}
/// Matches the existing native tungstenite 0.29 close-code policy exactly.
pub const fn close_code_allowed(code: u16) -> bool {
    matches!(code, 1000..=1003 | 1007..=1013 | 3000..=4999)
}

impl MessageRef<'_> {
    /// Checks semantic fields, not the eventual serialized allocator overhead.
    pub fn validate(&self) -> Result<()> {
        if self.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::Limit);
        }
        if self.kind != MessageKind::Close && self.close_code.is_some() {
            return Err(Error::Invalid);
        }
        match self.kind {
            MessageKind::Text => {
                std::str::from_utf8(self.payload).map_err(|_| Error::Utf8)?;
            }
            MessageKind::Ping | MessageKind::Pong if self.payload.len() > MAX_CONTROL_BYTES => {
                return Err(Error::Limit);
            }
            MessageKind::Close => {
                std::str::from_utf8(self.payload).map_err(|_| Error::Utf8)?;
                if self.payload.len() > MAX_CLOSE_BYTES
                    || (self.close_code.is_none() && !self.payload.is_empty())
                    || self
                        .close_code
                        .is_some_and(|code| !close_code_allowed(code))
                {
                    return Err(Error::Invalid);
                }
            }
            _ => {}
        }
        Ok(())
    }
    /// Uses the native default Capnp allocator, including real multi-segment
    /// overhead. A payload fitting MAX_PAYLOAD_BYTES may still exceed the wire cap.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<ws_message_capnp::ws_message::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&SCHEMA_DIGEST);
        root.set_kind(match self.kind {
            MessageKind::Text => ws_message_capnp::Kind::Text,
            MessageKind::Binary => ws_message_capnp::Kind::Binary,
            MessageKind::Ping => ws_message_capnp::Kind::Ping,
            MessageKind::Pong => ws_message_capnp::Kind::Pong,
            MessageKind::Close => ws_message_capnp::Kind::Close,
        });
        root.set_payload(self.payload);
        root.set_has_close_code(self.close_code.is_some());
        root.set_close_code(self.close_code.unwrap_or(0));
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(Error::Limit);
        }
        Ok(bytes)
    }
}
impl Message {
    pub fn as_ref(&self) -> MessageRef<'_> {
        MessageRef {
            kind: self.kind,
            payload: &self.payload,
            close_code: self.close_code,
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.as_ref().validate()
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.as_ref().encode()
    }
    /// Bounded owned decode. Accepts native-compatible multi-segment/far-pointer
    /// shapes; rejects trailing bytes, reachable capabilities and malformed fields.
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
            .get_root::<ws_message_capnp::ws_message::Reader>()
            .map_err(|_| Error::Invalid)?;
        if root.total_size().map_err(|_| Error::Invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(|_| Error::Invalid)? != SCHEMA_DIGEST
        {
            return Err(Error::Contract);
        }
        if !root.get_has_close_code() && root.get_close_code() != 0 {
            return Err(Error::Invalid);
        }
        let kind = match root.get_kind().map_err(|_| Error::Invalid)? {
            ws_message_capnp::Kind::Text => MessageKind::Text,
            ws_message_capnp::Kind::Binary => MessageKind::Binary,
            ws_message_capnp::Kind::Ping => MessageKind::Ping,
            ws_message_capnp::Kind::Pong => MessageKind::Pong,
            ws_message_capnp::Kind::Close => MessageKind::Close,
        };
        let payload = root.get_payload().map_err(|_| Error::Invalid)?;
        let view = MessageRef {
            kind,
            payload,
            close_code: root.get_has_close_code().then(|| root.get_close_code()),
        };
        view.validate()?;
        Ok(Self {
            kind,
            payload: payload.to_vec(),
            close_code: view.close_code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_messages_preserve_all_five_kinds_without_private_debug() {
        for (kind, payload, close_code) in [
            (MessageKind::Text, "雪 🙂".as_bytes(), None),
            (MessageKind::Binary, &[0, 255, 128, 0][..], None),
            (MessageKind::Ping, &[255, 0][..], None),
            (MessageKind::Pong, &[][..], None),
            (MessageKind::Close, "再见".as_bytes(), Some(1000)),
        ] {
            let value = MessageRef {
                kind,
                payload,
                close_code,
            };
            let raw = value.encode().unwrap();
            let decoded = Message::decode(&raw).unwrap();
            assert_eq!(decoded.as_ref(), value);
            assert!(!format!("{decoded:?}").contains("雪"));
            assert!(!format!("{decoded:?}").contains("再见"));
        }
    }
    #[test]
    fn default_allocator_overhead_and_payload_cap_are_distinct() {
        let near = vec![0x7a; MAX_PAYLOAD_BYTES - 256];
        let view = MessageRef {
            kind: MessageKind::Binary,
            payload: &near,
            close_code: None,
        };
        let raw = view.encode().unwrap();
        assert!(raw.len() <= MAX_ENVELOPE_BYTES);
        assert!(
            u32::from_le_bytes(raw[..4].try_into().unwrap()) > 0,
            "exercise actual multiple segments"
        );
        assert_eq!(Message::decode(&raw).unwrap().payload, near);
        let full = vec![0; MAX_PAYLOAD_BYTES];
        let view = MessageRef {
            kind: MessageKind::Binary,
            payload: &full,
            close_code: None,
        };
        assert_eq!(view.validate(), Ok(()));
        assert_eq!(view.encode(), Err(Error::Limit));
    }
    #[test]
    fn unsupported_and_malformed_envelopes_are_not_payloads() {
        let raw = MessageRef {
            kind: MessageKind::Text,
            payload: b"unique",
            close_code: None,
        }
        .encode()
        .unwrap();
        let mut altered = raw.clone();
        altered[16] = 2;
        assert_eq!(Message::decode(&altered), Err(Error::Contract));
        let mut altered = raw.clone();
        altered[40] ^= 1;
        assert_eq!(Message::decode(&altered), Err(Error::Contract));
        let mut altered = raw;
        altered.extend([0; 8]);
        assert_eq!(Message::decode(&altered), Err(Error::Invalid));
    }
}
