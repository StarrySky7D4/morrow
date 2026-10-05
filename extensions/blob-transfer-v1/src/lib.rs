//! Pure byte codec/state. VerifiedBytes is neither storage nor business commit.
//! No source/lease/grant, clock, reconnect or import authority is provided here.
#![deny(unsafe_op_in_unsafe_fn)]
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
use std::fmt;
#[allow(unsafe_op_in_unsafe_fn)]
mod blob_transfer_capnp {
    include!(concat!(env!("OUT_DIR"), "/blob_transfer_capnp.rs"));
}
pub mod ffi;
pub mod state;
pub use state::{Acceptance, Limits, Phase, Receiver, Snapshot};
pub const PROFILE: &str = "blob-transfer-v1";
pub const VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 65536;
pub const MAX_CHUNK_BYTES: usize = 60 * 1024;
pub const MAX_OBJECT_BYTES: u64 = 16 * 1024 * 1024;
pub const SCHEMA_DIGEST: [u8; 32] = [
    148, 29, 183, 198, 98, 129, 95, 137, 99, 180, 107, 187, 101, 165, 193, 67, 217, 15, 158, 114,
    23, 147, 126, 132, 192, 93, 27, 143, 17, 2, 69, 67,
];
pub const TRAVERSAL_LIMIT_WORDS: usize = 32768;
pub const NESTING_LIMIT: i32 = 16;
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid = 1,
    Contract = 2,
    Limit = 3,
    Digest = 4,
    Buffer = 5,
    State = 6,
    Identity = 7,
    Sequence = 8,
    Incomplete = 9,
    Cancelled = 10,
    Terminal = 11,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blob-transfer-v1: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub transfer_epoch: [u8; 32],
    pub object_ref: [u8; 32],
    pub operation_id: [u8; 32],
}
impl Identity {
    pub fn validate(&self) -> Result<()> {
        if self.transfer_epoch == [0; 32]
            || self.object_ref == [0; 32]
            || self.operation_id == [0; 32]
        {
            Err(Error::Identity)
        } else {
            Ok(())
        }
    }
}
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptStatus {
    Accepted = 0,
    Existing = 1,
}
#[derive(Clone, PartialEq, Eq)]
pub enum Action {
    Descriptor {
        total_length: u64,
        whole_sha256: [u8; 32],
    },
    Chunk {
        sequence: u64,
        offset: u64,
        payload: Vec<u8>,
        chunk_sha256: [u8; 32],
    },
    Receipt {
        request_digest: [u8; 32],
        sequence: u64,
        offset: u64,
        length: u64,
        chunk_sha256: [u8; 32],
        status: ReceiptStatus,
    },
    End {
        total_length: u64,
        whole_sha256: [u8; 32],
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ActionRef<'a> {
    Descriptor {
        total_length: u64,
        whole_sha256: [u8; 32],
    },
    Chunk {
        sequence: u64,
        offset: u64,
        payload: &'a [u8],
        chunk_sha256: [u8; 32],
    },
    Receipt {
        request_digest: [u8; 32],
        sequence: u64,
        offset: u64,
        length: u64,
        chunk_sha256: [u8; 32],
        status: ReceiptStatus,
    },
    End {
        total_length: u64,
        whole_sha256: [u8; 32],
    },
}
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub identity: Identity,
    pub action: Action,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FrameRef<'a> {
    pub identity: Identity,
    pub action: ActionRef<'a>,
}
// Debug exposes only bounded shape metadata, never user bytes, identities or digests.
impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Identity { redacted }")
    }
}
impl fmt::Debug for ActionRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Descriptor { total_length, .. } => f
                .debug_struct("Descriptor")
                .field("total_length", total_length)
                .finish(),
            Self::Chunk {
                sequence,
                offset,
                payload,
                ..
            } => f
                .debug_struct("Chunk")
                .field("sequence", sequence)
                .field("offset", offset)
                .field("payload_length", &payload.len())
                .finish(),
            Self::Receipt {
                sequence,
                offset,
                length,
                ..
            } => f
                .debug_struct("Receipt")
                .field("sequence", sequence)
                .field("offset", offset)
                .field("length", length)
                .finish(),
            Self::End { total_length, .. } => f
                .debug_struct("End")
                .field("total_length", total_length)
                .finish(),
        }
    }
}
impl fmt::Debug for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Descriptor { total_length, .. } => f
                .debug_struct("Descriptor")
                .field("total_length", total_length)
                .finish(),
            Self::Chunk {
                sequence,
                offset,
                payload,
                ..
            } => f
                .debug_struct("Chunk")
                .field("sequence", sequence)
                .field("offset", offset)
                .field("payload_length", &payload.len())
                .finish(),
            Self::Receipt {
                sequence,
                offset,
                length,
                ..
            } => f
                .debug_struct("Receipt")
                .field("sequence", sequence)
                .field("offset", offset)
                .field("length", length)
                .finish(),
            Self::End { total_length, .. } => f
                .debug_struct("End")
                .field("total_length", total_length)
                .finish(),
        }
    }
}
impl fmt::Debug for FrameRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameRef")
            .field("action", &self.action)
            .finish()
    }
}
impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("action", &self.as_ref().action)
            .finish()
    }
}
fn range(offset: u64, length: u64) -> Result<()> {
    if offset
        .checked_add(length)
        .is_none_or(|v| v > MAX_OBJECT_BYTES)
    {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
impl FrameRef<'_> {
    pub fn validate(&self) -> Result<()> {
        self.identity.validate()?;
        match self.action {
            ActionRef::Descriptor { total_length, .. } | ActionRef::End { total_length, .. } => {
                if total_length > MAX_OBJECT_BYTES {
                    return Err(Error::Limit);
                }
            }
            ActionRef::Chunk {
                sequence,
                offset,
                payload,
                chunk_sha256,
            } => {
                if sequence == 0 {
                    return Err(Error::Sequence);
                }
                if payload.len() > MAX_CHUNK_BYTES {
                    return Err(Error::Limit);
                }
                range(offset, payload.len() as u64)?;
                if digest(payload) != chunk_sha256 {
                    return Err(Error::Digest);
                }
            }
            ActionRef::Receipt {
                sequence,
                offset,
                length,
                ..
            } => {
                if sequence == 0 {
                    return Err(Error::Sequence);
                }
                if length > MAX_CHUNK_BYTES as u64 {
                    return Err(Error::Limit);
                }
                range(offset, length)?;
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        {
            let mut root = message.init_root::<blob_transfer_capnp::frame::Builder>();
            root.set_version(VERSION);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_transfer_epoch(&self.identity.transfer_epoch);
            root.set_object_ref(&self.identity.object_ref);
            root.set_operation_id(&self.identity.operation_id);
            match self.action {
                ActionRef::Descriptor {
                    total_length,
                    whole_sha256,
                } => {
                    let mut a = root.init_descriptor();
                    a.set_total_length(total_length);
                    a.set_whole_sha256(&whole_sha256);
                }
                ActionRef::Chunk {
                    sequence,
                    offset,
                    payload,
                    chunk_sha256,
                } => {
                    let mut a = root.init_chunk();
                    a.set_sequence(sequence);
                    a.set_offset(offset);
                    a.set_payload(payload);
                    a.set_chunk_sha256(&chunk_sha256);
                }
                ActionRef::Receipt {
                    request_digest,
                    sequence,
                    offset,
                    length,
                    chunk_sha256,
                    status,
                } => {
                    let mut a = root.init_receipt();
                    a.set_request_digest(&request_digest);
                    a.set_sequence(sequence);
                    a.set_offset(offset);
                    a.set_length(length);
                    a.set_chunk_sha256(&chunk_sha256);
                    a.set_status(match status {
                        ReceiptStatus::Accepted => blob_transfer_capnp::ReceiptStatus::Accepted,
                        ReceiptStatus::Existing => blob_transfer_capnp::ReceiptStatus::Existing,
                    });
                }
                ActionRef::End {
                    total_length,
                    whole_sha256,
                } => {
                    let mut a = root.init_end();
                    a.set_total_length(total_length);
                    a.set_whole_sha256(&whole_sha256);
                }
            }
        }
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(Error::Limit);
        }
        Ok(bytes)
    }
    /// Validate before the only payload allocation; metadata remains fixed size.
    pub fn to_owned(&self) -> Result<Frame> {
        self.validate()?;
        Ok(Frame {
            identity: self.identity,
            action: match self.action {
                ActionRef::Descriptor {
                    total_length,
                    whole_sha256,
                } => Action::Descriptor {
                    total_length,
                    whole_sha256,
                },
                ActionRef::Chunk {
                    sequence,
                    offset,
                    payload,
                    chunk_sha256,
                } => Action::Chunk {
                    sequence,
                    offset,
                    payload: payload.to_vec(),
                    chunk_sha256,
                },
                ActionRef::Receipt {
                    request_digest,
                    sequence,
                    offset,
                    length,
                    chunk_sha256,
                    status,
                } => Action::Receipt {
                    request_digest,
                    sequence,
                    offset,
                    length,
                    chunk_sha256,
                    status,
                },
                ActionRef::End {
                    total_length,
                    whole_sha256,
                } => Action::End {
                    total_length,
                    whole_sha256,
                },
            },
        })
    }
}
fn array(data: capnp::Result<&[u8]>) -> Result<[u8; 32]> {
    data.map_err(|_| Error::Invalid)?
        .try_into()
        .map_err(|_| Error::Invalid)
}
impl<'a> FrameRef<'a> {
    /// No-allocator segment parsing with finite traversal/nesting. The returned
    /// payload is a checked sub-slice of the caller's immutable original wire.
    /// Unknown bounded fields/far pointers may be compatible; caps/tail are not.
    pub fn decode(bytes: &'a [u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
            return Err(Error::Limit);
        }
        let mut rest = bytes;
        let message = serialize::read_message_from_flat_slice_no_alloc(
            &mut rest,
            ReaderOptions {
                traversal_limit_in_words: Some(TRAVERSAL_LIMIT_WORDS),
                nesting_limit: NESTING_LIMIT,
            },
        )
        .map_err(|_| Error::Invalid)?;
        if !rest.is_empty() {
            return Err(Error::Invalid);
        }
        let root = message
            .get_root::<blob_transfer_capnp::frame::Reader>()
            .map_err(|_| Error::Invalid)?;
        if root.total_size().map_err(|_| Error::Invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(|_| Error::Invalid)? != SCHEMA_DIGEST
        {
            return Err(Error::Contract);
        }
        let identity = Identity {
            transfer_epoch: array(root.get_transfer_epoch())?,
            object_ref: array(root.get_object_ref())?,
            operation_id: array(root.get_operation_id())?,
        };
        identity.validate()?;
        use blob_transfer_capnp::frame::Which;
        let action = match root.which().map_err(|_| Error::Invalid)? {
            Which::Descriptor(a) => {
                let a = a.map_err(|_| Error::Invalid)?;
                ActionRef::Descriptor {
                    total_length: a.get_total_length(),
                    whole_sha256: array(a.get_whole_sha256())?,
                }
            }
            Which::Chunk(a) => {
                let a = a.map_err(|_| Error::Invalid)?;
                let payload = a.get_payload().map_err(|_| Error::Invalid)?;
                if payload.len() > MAX_CHUNK_BYTES {
                    return Err(Error::Limit);
                }
                let data = if payload.is_empty() {
                    &[] as &[u8]
                } else {
                    let start = (payload.as_ptr() as usize)
                        .checked_sub(bytes.as_ptr() as usize)
                        .ok_or(Error::Invalid)?;
                    let end = start.checked_add(payload.len()).ok_or(Error::Invalid)?;
                    bytes.get(start..end).ok_or(Error::Invalid)?
                };
                ActionRef::Chunk {
                    sequence: a.get_sequence(),
                    offset: a.get_offset(),
                    payload: data,
                    chunk_sha256: array(a.get_chunk_sha256())?,
                }
            }
            Which::Receipt(a) => {
                let a = a.map_err(|_| Error::Invalid)?;
                ActionRef::Receipt {
                    request_digest: array(a.get_request_digest())?,
                    sequence: a.get_sequence(),
                    offset: a.get_offset(),
                    length: a.get_length(),
                    chunk_sha256: array(a.get_chunk_sha256())?,
                    status: match a.get_status().map_err(|_| Error::Invalid)? {
                        blob_transfer_capnp::ReceiptStatus::Accepted => ReceiptStatus::Accepted,
                        blob_transfer_capnp::ReceiptStatus::Existing => ReceiptStatus::Existing,
                    },
                }
            }
            Which::End(a) => {
                let a = a.map_err(|_| Error::Invalid)?;
                ActionRef::End {
                    total_length: a.get_total_length(),
                    whole_sha256: array(a.get_whole_sha256())?,
                }
            }
        };
        let result = Self { identity, action };
        result.validate()?;
        Ok(result)
    }
}
impl Frame {
    pub fn as_ref(&self) -> FrameRef<'_> {
        FrameRef {
            identity: self.identity,
            action: match &self.action {
                Action::Descriptor {
                    total_length,
                    whole_sha256,
                } => ActionRef::Descriptor {
                    total_length: *total_length,
                    whole_sha256: *whole_sha256,
                },
                Action::Chunk {
                    sequence,
                    offset,
                    payload,
                    chunk_sha256,
                } => ActionRef::Chunk {
                    sequence: *sequence,
                    offset: *offset,
                    payload,
                    chunk_sha256: *chunk_sha256,
                },
                Action::Receipt {
                    request_digest,
                    sequence,
                    offset,
                    length,
                    chunk_sha256,
                    status,
                } => ActionRef::Receipt {
                    request_digest: *request_digest,
                    sequence: *sequence,
                    offset: *offset,
                    length: *length,
                    chunk_sha256: *chunk_sha256,
                    status: *status,
                },
                Action::End {
                    total_length,
                    whole_sha256,
                } => ActionRef::End {
                    total_length: *total_length,
                    whole_sha256: *whole_sha256,
                },
            },
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.as_ref().validate()
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.as_ref().encode()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        FrameRef::decode(bytes)?.to_owned()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn identity() -> Identity {
        Identity {
            transfer_epoch: [1; 32],
            object_ref: [2; 32],
            operation_id: [3; 32],
        }
    }
    pub(crate) fn chunk<'a>(sequence: u64, offset: u64, payload: &'a [u8]) -> FrameRef<'a> {
        FrameRef {
            identity: identity(),
            action: ActionRef::Chunk {
                sequence,
                offset,
                payload,
                chunk_sha256: digest(payload),
            },
        }
    }
    #[test]
    fn debug_redacts_payload_ids_and_hashes_for_owned_borrowed_and_state() {
        let payload = b"private-blob-content\0\xff";
        let id = Identity {
            transfer_epoch: [171; 32],
            object_ref: [172; 32],
            operation_id: [173; 32],
        };
        let cases = [
            (
                ActionRef::Descriptor {
                    total_length: payload.len() as u64,
                    whole_sha256: [174; 32],
                },
                format!("Descriptor {{ total_length: {} }}", payload.len()),
            ),
            (
                ActionRef::Chunk {
                    sequence: 7,
                    offset: 9,
                    payload,
                    chunk_sha256: digest(payload),
                },
                format!(
                    "Chunk {{ sequence: 7, offset: 9, payload_length: {} }}",
                    payload.len()
                ),
            ),
            (
                ActionRef::Receipt {
                    request_digest: [175; 32],
                    sequence: 7,
                    offset: 9,
                    length: payload.len() as u64,
                    chunk_sha256: [176; 32],
                    status: ReceiptStatus::Existing,
                },
                format!(
                    "Receipt {{ sequence: 7, offset: 9, length: {} }}",
                    payload.len()
                ),
            ),
            (
                ActionRef::End {
                    total_length: payload.len() as u64,
                    whole_sha256: [177; 32],
                },
                format!("End {{ total_length: {} }}", payload.len()),
            ),
        ];
        for (action, expected) in cases {
            let borrowed = FrameRef {
                identity: id,
                action,
            };
            let owned = borrowed.to_owned().unwrap();
            assert_eq!(format!("{action:?}"), expected);
            assert_eq!(format!("{:?}", owned.action), expected);
            assert_eq!(
                format!("{borrowed:?}"),
                format!("FrameRef {{ action: {expected} }}")
            );
            assert_eq!(
                format!("{owned:?}"),
                format!("Frame {{ action: {expected} }}")
            );
            assert_eq!(
                format!("{:?}", Acceptance::Receipt(owned)),
                format!("Receipt({{ action: {expected} }})")
            );
        }
        assert_eq!(format!("{id:?}"), "Identity { redacted }");
        let mut snapshot = Receiver::new(Limits::default()).unwrap().snapshot();
        snapshot.identity = Some(id);
        snapshot.expected_sha256 = [178; 32];
        snapshot.verified_sha256 = [179; 32];
        assert_eq!(
            format!("{snapshot:?}"),
            "Snapshot { phase: Empty, total_length: 0, received_bytes: 0, next_sequence: 1 }"
        );
        assert_eq!(
            format!(
                "{:?}",
                Acceptance::VerifiedBytes {
                    total_length: 3,
                    whole_sha256: [180; 32]
                }
            ),
            "VerifiedBytes { total_length: 3 }"
        );
    }
    #[test]
    fn four_actions_roundtrip_complete_binary_identity_and_digest() {
        let payload = [0, 255, 1, 128];
        let id = identity();
        let actions = [
            ActionRef::Descriptor {
                total_length: 4,
                whole_sha256: digest(&payload),
            },
            chunk(1, 0, &payload).action,
            ActionRef::Receipt {
                request_digest: [8; 32],
                sequence: 1,
                offset: 0,
                length: 4,
                chunk_sha256: digest(&payload),
                status: ReceiptStatus::Existing,
            },
            ActionRef::End {
                total_length: 4,
                whole_sha256: digest(&payload),
            },
        ];
        for action in actions {
            let view = FrameRef {
                identity: id,
                action,
            };
            let bytes = view.encode().unwrap();
            assert_eq!(FrameRef::decode(&bytes).unwrap(), view);
            assert_eq!(Frame::decode(&bytes).unwrap().as_ref(), view);
        }
    }
    #[test]
    fn borrowed_unaligned_multisegment_and_owned_input_lifetime() {
        let payload = vec![0x7d; MAX_CHUNK_BYTES];
        let original = chunk(1, 0, &payload).encode().unwrap();
        assert!(
            u32::from_le_bytes(original[..4].try_into().unwrap()) > 0,
            "real default allocator multi-segment"
        );
        let mut unaligned = vec![0];
        unaligned.extend(&original);
        let borrowed = FrameRef::decode(&unaligned[1..]).unwrap();
        let ActionRef::Chunk { payload, .. } = borrowed.action else {
            panic!()
        };
        assert!(payload.as_ptr() as usize >= unaligned.as_ptr() as usize);
        assert_eq!(payload.len(), MAX_CHUNK_BYTES);
        let owned = borrowed.to_owned().unwrap();
        unaligned.fill(0);
        assert_eq!(owned, Frame::decode(&original).unwrap());
    }
    #[test]
    fn chunk_object_sequence_bounds_and_real_chunk_hash() {
        for length in [MAX_CHUNK_BYTES - 1, MAX_CHUNK_BYTES] {
            let payload = vec![1; length];
            assert!(
                chunk(1, MAX_OBJECT_BYTES - length as u64, &payload)
                    .encode()
                    .is_ok()
            );
        }
        assert_eq!(
            chunk(1, 0, &vec![0; MAX_CHUNK_BYTES + 1]).validate(),
            Err(Error::Limit)
        );
        assert_eq!(chunk(0, 0, b"x").validate(), Err(Error::Sequence));
        assert_eq!(chunk(1, u64::MAX, b"x").validate(), Err(Error::Limit));
        let mut bad = chunk(1, 0, b"x");
        if let ActionRef::Chunk {
            ref mut chunk_sha256,
            ..
        } = bad.action
        {
            chunk_sha256[0] ^= 1;
        }
        assert_eq!(bad.validate(), Err(Error::Digest));
        for total in [0, MAX_OBJECT_BYTES - 1, MAX_OBJECT_BYTES] {
            assert!(
                FrameRef {
                    identity: identity(),
                    action: ActionRef::Descriptor {
                        total_length: total,
                        whole_sha256: [0; 32]
                    }
                }
                .validate()
                .is_ok()
            );
        }
        assert_eq!(
            FrameRef {
                identity: identity(),
                action: ActionRef::End {
                    total_length: MAX_OBJECT_BYTES + 1,
                    whole_sha256: [0; 32]
                }
            }
            .validate(),
            Err(Error::Limit)
        );
    }
    #[test]
    fn malformed_tail_contract_ids_digest_payload_and_wire_limit_reject() {
        let raw = chunk(1, 0, b"distinct-payload").encode().unwrap();
        let mut tail = raw.clone();
        tail.extend([0; 8]);
        assert_eq!(Frame::decode(&tail), Err(Error::Invalid));
        assert_eq!(
            FrameRef::decode(&vec![0; MAX_FRAME_BYTES + 1]),
            Err(Error::Limit)
        );
        let mut corrupt = raw.clone();
        let offset = corrupt
            .windows(16)
            .position(|x| x == b"distinct-payload")
            .unwrap();
        corrupt[offset] ^= 1;
        assert_eq!(Frame::decode(&corrupt), Err(Error::Digest));
        let mut message = Builder::new_default();
        let mut root = message.init_root::<blob_transfer_capnp::frame::Builder>();
        root.set_version(VERSION + 1);
        root.set_schema_sha256(&SCHEMA_DIGEST);
        assert_eq!(
            Frame::decode(&serialize::write_message_to_words(&message)),
            Err(Error::Contract)
        );
        root = message.get_root().unwrap();
        root.set_version(VERSION);
        assert_eq!(
            Frame::decode(&serialize::write_message_to_words(&message)),
            Err(Error::Invalid)
        );
        let mut id = identity();
        id.object_ref = [0; 32];
        assert_eq!(
            FrameRef {
                identity: id,
                action: ActionRef::End {
                    total_length: 0,
                    whole_sha256: digest(b"")
                }
            }
            .validate(),
            Err(Error::Identity)
        );
    }
    #[test]
    fn receipt_lengths_and_empty_chunk_are_explicit() {
        assert!(chunk(1, 0, b"").encode().is_ok());
        for length in [MAX_CHUNK_BYTES as u64 + 1, u64::MAX] {
            assert_eq!(
                FrameRef {
                    identity: identity(),
                    action: ActionRef::Receipt {
                        request_digest: [7; 32],
                        sequence: 1,
                        offset: 0,
                        length,
                        chunk_sha256: [0; 32],
                        status: ReceiptStatus::Accepted
                    }
                }
                .validate(),
                Err(Error::Limit)
            );
        }
    }
}
