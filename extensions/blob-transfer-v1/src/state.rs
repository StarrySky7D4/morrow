//! Synchronous one-receiver byte validation; never an authoritative IO budget/grant.
use crate::{
    Action, ActionRef, Error, Frame, FrameRef, Identity, MAX_FRAME_BYTES, MAX_OBJECT_BYTES,
    ReceiptStatus, Result, digest,
};
use sha2::{Digest, Sha256};
pub const MAX_ACCOUNTED_WIRE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_REQUESTS: u64 = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_payload_bytes: u64,
    pub max_wire_bytes: u64,
    pub max_request_bytes: u64,
    pub max_response_bytes: u64,
    pub max_requests: u64,
    pub max_receipts: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_payload_bytes: MAX_OBJECT_BYTES,
            max_wire_bytes: 36 * 1024 * 1024,
            max_request_bytes: 32 * 1024 * 1024,
            max_response_bytes: 4 * 1024 * 1024,
            max_requests: 1024,
            max_receipts: 1024,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if self.max_payload_bytes > MAX_OBJECT_BYTES
            || self.max_wire_bytes > MAX_ACCOUNTED_WIRE_BYTES
            || self.max_request_bytes > MAX_ACCOUNTED_WIRE_BYTES
            || self.max_response_bytes > MAX_ACCOUNTED_WIRE_BYTES
            || self.max_requests > MAX_REQUESTS
            || self.max_receipts > MAX_REQUESTS
        {
            Err(Error::Limit)
        } else {
            Ok(())
        }
    }
}
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Empty = 0,
    Receiving = 1,
    VerifiedBytes = 2,
    Cancelled = 3,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub phase: Phase,
    pub identity: Option<Identity>,
    pub total_length: u64,
    pub expected_sha256: [u8; 32],
    pub verified_sha256: [u8; 32],
    pub received_bytes: u64,
    pub next_sequence: u64,
    pub request_bytes: u64,
    pub response_bytes: u64,
    pub wire_bytes: u64,
    pub requests: u64,
    pub receipts: u64,
    pub retained_recent_requests: u32,
    pub retained_recent_wire_bytes: u32,
}
#[derive(Clone, PartialEq, Eq)]
pub enum Acceptance {
    DescriptorAccepted,
    Receipt(Frame),
    VerifiedBytes {
        total_length: u64,
        whole_sha256: [u8; 32],
    },
}
impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot")
            .field("phase", &self.phase)
            .field("total_length", &self.total_length)
            .field("received_bytes", &self.received_bytes)
            .field("next_sequence", &self.next_sequence)
            .finish()
    }
}
impl std::fmt::Debug for Acceptance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DescriptorAccepted => f.write_str("DescriptorAccepted"),
            Self::Receipt(frame) => write!(f, "Receipt({{ action: {:?} }})", frame.as_ref().action),
            Self::VerifiedBytes { total_length, .. } => f
                .debug_struct("VerifiedBytes")
                .field("total_length", total_length)
                .finish(),
        }
    }
}
struct Recent {
    wire: Vec<u8>,
    request_digest: [u8; 32],
    sequence: u64,
    offset: u64,
    length: u64,
    chunk_sha256: [u8; 32],
}
pub struct Receiver {
    limits: Limits,
    snapshot: Snapshot,
    hasher: Sha256,
    recent: Option<Recent>,
}
impl Receiver {
    pub fn new(limits: Limits) -> Result<Self> {
        limits.validate()?;
        Ok(Self {
            limits,
            snapshot: Snapshot {
                phase: Phase::Empty,
                identity: None,
                total_length: 0,
                expected_sha256: [0; 32],
                verified_sha256: [0; 32],
                received_bytes: 0,
                next_sequence: 1,
                request_bytes: 0,
                response_bytes: 0,
                wire_bytes: 0,
                requests: 0,
                receipts: 0,
                retained_recent_requests: 0,
                retained_recent_wire_bytes: 0,
            },
            hasher: Sha256::new(),
            recent: None,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
    }
    pub(crate) fn retained_wire(&self) -> &[u8] {
        self.recent.as_ref().map_or(&[], |r| r.wire.as_slice())
    }
    pub fn limits(&self) -> Limits {
        self.limits
    }
    pub fn cancel(&mut self) {
        if matches!(self.snapshot.phase, Phase::Empty | Phase::Receiving) {
            self.snapshot.phase = Phase::Cancelled;
            self.recent = None;
            self.snapshot.retained_recent_requests = 0;
            self.snapshot.retained_recent_wire_bytes = 0;
        }
    }
    fn request_charge(&mut self, length: usize) -> Result<()> {
        if length == 0 || length > MAX_FRAME_BYTES {
            return Err(Error::Limit);
        }
        let bytes = self
            .snapshot
            .request_bytes
            .checked_add(length as u64)
            .filter(|n| *n <= self.limits.max_request_bytes)
            .ok_or(Error::Limit)?;
        let wire = self
            .snapshot
            .wire_bytes
            .checked_add(length as u64)
            .filter(|n| *n <= self.limits.max_wire_bytes)
            .ok_or(Error::Limit)?;
        let requests = self
            .snapshot
            .requests
            .checked_add(1)
            .filter(|n| *n <= self.limits.max_requests)
            .ok_or(Error::Limit)?;
        self.snapshot.request_bytes = bytes;
        self.snapshot.wire_bytes = wire;
        self.snapshot.requests = requests;
        Ok(())
    }
    fn receipt(&mut self, identity: Identity, r: &Recent, status: ReceiptStatus) -> Result<Frame> {
        let frame = Frame {
            identity,
            action: Action::Receipt {
                request_digest: r.request_digest,
                sequence: r.sequence,
                offset: r.offset,
                length: r.length,
                chunk_sha256: r.chunk_sha256,
                status,
            },
        };
        let length = frame.encode()?.len() as u64;
        let response = self
            .snapshot
            .response_bytes
            .checked_add(length)
            .filter(|n| *n <= self.limits.max_response_bytes)
            .ok_or(Error::Limit)?;
        let wire = self
            .snapshot
            .wire_bytes
            .checked_add(length)
            .filter(|n| *n <= self.limits.max_wire_bytes)
            .ok_or(Error::Limit)?;
        let receipts = self
            .snapshot
            .receipts
            .checked_add(1)
            .filter(|n| *n <= self.limits.max_receipts)
            .ok_or(Error::Limit)?;
        self.snapshot.response_bytes = response;
        self.snapshot.wire_bytes = wire;
        self.snapshot.receipts = receipts;
        Ok(frame)
    }
    /// Admission charges bounded original wire and request count before decode.
    /// Every error preserves semantic progress; admitted charges are never refunded.
    /// Same live recent chunk may return Existing only for identical original wire,
    /// digest and all fields. A changed encoding or older receipt is not replayed.
    /// No clock/bool here can reauthorize an original owner/source/lease.
    pub fn accept(&mut self, wire: &[u8]) -> Result<Acceptance> {
        match self.snapshot.phase {
            Phase::Cancelled => return Err(Error::Cancelled),
            Phase::VerifiedBytes => return Err(Error::Terminal),
            _ => {}
        }
        self.request_charge(wire.len())?;
        let frame = FrameRef::decode(wire)?;
        if self
            .snapshot
            .identity
            .is_some_and(|id| id != frame.identity)
        {
            return Err(Error::Identity);
        }
        match frame.action {
            ActionRef::Descriptor {
                total_length,
                whole_sha256,
            } => {
                if self.snapshot.phase != Phase::Empty {
                    return Err(Error::State);
                }
                if total_length > self.limits.max_payload_bytes {
                    return Err(Error::Limit);
                }
                self.snapshot.identity = Some(frame.identity);
                self.snapshot.total_length = total_length;
                self.snapshot.expected_sha256 = whole_sha256;
                self.snapshot.phase = Phase::Receiving;
                Ok(Acceptance::DescriptorAccepted)
            }
            ActionRef::Chunk {
                sequence,
                offset,
                payload,
                chunk_sha256,
            } => {
                if self.snapshot.phase != Phase::Receiving {
                    return Err(Error::State);
                }
                let request_digest = digest(wire);
                if self.recent.as_ref().is_some_and(|r| {
                    r.request_digest == request_digest
                        && r.wire == wire
                        && r.sequence == sequence
                        && r.offset == offset
                        && r.length == payload.len() as u64
                        && r.chunk_sha256 == chunk_sha256
                }) {
                    let last = self.recent.take().unwrap();
                    let result = self.receipt(frame.identity, &last, ReceiptStatus::Existing);
                    self.recent = Some(last);
                    return result.map(Acceptance::Receipt);
                }
                if sequence != self.snapshot.next_sequence || offset != self.snapshot.received_bytes
                {
                    return Err(Error::Sequence);
                }
                let received = offset
                    .checked_add(payload.len() as u64)
                    .filter(|n| {
                        *n <= self.snapshot.total_length && *n <= self.limits.max_payload_bytes
                    })
                    .ok_or(Error::Limit)?;
                let next = sequence.checked_add(1).ok_or(Error::Limit)?;
                // The most recent original wire is bounded; no full-object spool is retained.
                let mut recent = Recent {
                    wire: Vec::new(),
                    request_digest,
                    sequence,
                    offset,
                    length: payload.len() as u64,
                    chunk_sha256,
                };
                let receipt = self.receipt(frame.identity, &recent, ReceiptStatus::Accepted)?;
                recent.wire = wire.to_vec();
                self.hasher.update(payload);
                self.snapshot.received_bytes = received;
                self.snapshot.next_sequence = next;
                self.snapshot.retained_recent_requests = 1;
                self.snapshot.retained_recent_wire_bytes = wire.len() as u32;
                self.recent = Some(recent);
                Ok(Acceptance::Receipt(receipt))
            }
            ActionRef::End {
                total_length,
                whole_sha256,
            } => {
                if self.snapshot.phase != Phase::Receiving {
                    return Err(Error::State);
                }
                if total_length != self.snapshot.total_length
                    || whole_sha256 != self.snapshot.expected_sha256
                {
                    return Err(Error::Digest);
                }
                if self.snapshot.received_bytes != total_length {
                    return Err(Error::Incomplete);
                }
                let actual: [u8; 32] = self.hasher.clone().finalize().into();
                if actual != whole_sha256 {
                    return Err(Error::Digest);
                }
                self.snapshot.verified_sha256 = actual;
                self.snapshot.phase = Phase::VerifiedBytes;
                self.recent = None;
                self.snapshot.retained_recent_requests = 0;
                self.snapshot.retained_recent_wire_bytes = 0;
                Ok(Acceptance::VerifiedBytes {
                    total_length,
                    whole_sha256: actual,
                })
            }
            ActionRef::Receipt { .. } => Err(Error::State),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{chunk, identity};
    fn descriptor(total: u64, hash: [u8; 32]) -> Vec<u8> {
        FrameRef {
            identity: identity(),
            action: ActionRef::Descriptor {
                total_length: total,
                whole_sha256: hash,
            },
        }
        .encode()
        .unwrap()
    }
    fn end(total: u64, hash: [u8; 32]) -> Vec<u8> {
        FrameRef {
            identity: identity(),
            action: ActionRef::End {
                total_length: total,
                whole_sha256: hash,
            },
        }
        .encode()
        .unwrap()
    }
    fn ready(bytes: &[u8]) -> Receiver {
        let mut s = Receiver::new(Limits::default()).unwrap();
        assert_eq!(
            s.accept(&descriptor(bytes.len() as u64, digest(bytes))),
            Ok(Acceptance::DescriptorAccepted)
        );
        s
    }
    #[test]
    fn empty_object_and_binary_stream_verify_actual_bytes_only() {
        for bytes in [b"" as &[u8], b"\0binary\xff"] {
            let mut s = ready(bytes);
            if !bytes.is_empty() {
                assert!(matches!(
                    s.accept(&chunk(1, 0, bytes).encode().unwrap()),
                    Ok(Acceptance::Receipt(_))
                ));
            }
            assert_eq!(
                s.accept(&end(bytes.len() as u64, digest(bytes))),
                Ok(Acceptance::VerifiedBytes {
                    total_length: bytes.len() as u64,
                    whole_sha256: digest(bytes)
                })
            );
            assert_eq!(s.snapshot().phase, Phase::VerifiedBytes);
            assert_eq!(
                s.accept(&end(bytes.len() as u64, digest(bytes))),
                Err(Error::Terminal)
            );
        }
    }
    #[test]
    fn whole_digest_is_computed_not_trusted_descriptor_or_end() {
        let mut s = Receiver::new(Limits::default()).unwrap();
        s.accept(&descriptor(3, digest(b"bad"))).unwrap();
        s.accept(&chunk(1, 0, b"xyz").encode().unwrap()).unwrap();
        let before = s.snapshot();
        assert_eq!(s.accept(&end(3, digest(b"bad"))), Err(Error::Digest));
        assert_eq!(s.snapshot().received_bytes, before.received_bytes);
        assert_eq!(s.snapshot().phase, Phase::Receiving);
        assert_eq!(s.snapshot().verified_sha256, [0; 32]);
        assert!(s.snapshot().request_bytes > before.request_bytes);
    }
    #[test]
    fn exact_recent_existing_charges_wire_response_without_rehash_or_payload() {
        let mut s = ready(b"abc");
        let raw = chunk(1, 0, b"abc").encode().unwrap();
        let Acceptance::Receipt(first) = s.accept(&raw).unwrap() else {
            panic!()
        };
        let before = s.snapshot();
        let Acceptance::Receipt(second) = s.accept(&raw).unwrap() else {
            panic!()
        };
        let Action::Receipt {
            status,
            request_digest,
            ..
        } = second.action
        else {
            panic!()
        };
        assert_eq!(status, ReceiptStatus::Existing);
        assert_eq!(request_digest, digest(&raw));
        assert_eq!(s.snapshot().received_bytes, 3);
        assert_eq!(s.snapshot().next_sequence, 2);
        assert_eq!(s.snapshot().requests, before.requests + 1);
        assert_eq!(
            s.snapshot().wire_bytes,
            before.wire_bytes + raw.len() as u64 + first.encode().unwrap().len() as u64
        );
        assert!(matches!(
            s.accept(&end(3, digest(b"abc"))),
            Ok(Acceptance::VerifiedBytes { .. })
        ));
    }
    #[test]
    fn gap_overlap_changed_duplicate_foreign_ids_and_old_duplicate_do_not_advance() {
        let mut s = ready(b"abcd");
        let first = chunk(1, 0, b"ab").encode().unwrap();
        s.accept(&first).unwrap();
        for raw in [
            chunk(3, 2, b"cd").encode().unwrap(),
            chunk(2, 1, b"cd").encode().unwrap(),
            chunk(1, 0, b"zz").encode().unwrap(),
        ] {
            let before = s.snapshot();
            assert_eq!(s.accept(&raw), Err(Error::Sequence));
            assert_eq!(s.snapshot().received_bytes, before.received_bytes);
            assert_eq!(s.snapshot().next_sequence, before.next_sequence);
            assert!(s.snapshot().request_bytes > before.request_bytes);
        }
        let mut foreign = chunk(2, 2, b"cd");
        foreign.identity.operation_id = [9; 32];
        assert_eq!(s.accept(&foreign.encode().unwrap()), Err(Error::Identity));
        s.accept(&chunk(2, 2, b"cd").encode().unwrap()).unwrap();
        assert_eq!(s.accept(&first), Err(Error::Sequence));
        assert_eq!(s.snapshot().retained_recent_requests, 1);
        assert!(matches!(
            s.accept(&end(4, digest(b"abcd"))),
            Ok(Acceptance::VerifiedBytes { .. })
        ));
    }
    #[test]
    fn early_end_wrong_end_and_bad_chunk_keep_progress_and_charges() {
        let mut s = ready(b"abc");
        assert_eq!(s.accept(&end(3, digest(b"abc"))), Err(Error::Incomplete));
        assert_eq!(s.accept(&end(2, digest(b"ab"))), Err(Error::Digest));
        let mut raw = chunk(1, 0, b"abc").encode().unwrap();
        let n = raw.windows(3).position(|x| x == b"abc").unwrap();
        raw[n] ^= 1;
        assert_eq!(s.accept(&raw), Err(Error::Digest));
        assert_eq!(s.snapshot().received_bytes, 0);
        assert_eq!(s.snapshot().requests, 4);
        s.accept(&chunk(1, 0, b"abc").encode().unwrap()).unwrap();
        assert!(matches!(
            s.accept(&end(3, digest(b"abc"))),
            Ok(Acceptance::VerifiedBytes { .. })
        ));
    }
    #[test]
    fn response_budget_failure_and_cancel_never_refund_admitted_request() {
        let limits = Limits {
            max_response_bytes: 0,
            ..Limits::default()
        };
        let mut s = Receiver::new(limits).unwrap();
        s.accept(&descriptor(1, digest(b"x"))).unwrap();
        let before = s.snapshot();
        assert_eq!(
            s.accept(&chunk(1, 0, b"x").encode().unwrap()),
            Err(Error::Limit)
        );
        let failed = s.snapshot();
        assert_eq!(failed.received_bytes, 0);
        assert_eq!(failed.receipts, 0);
        assert_eq!(failed.response_bytes, 0);
        assert!(failed.request_bytes > before.request_bytes);
        s.cancel();
        assert_eq!(s.snapshot().request_bytes, failed.request_bytes);
        assert_eq!(s.snapshot().wire_bytes, failed.wire_bytes);
        assert_eq!(s.accept(&end(1, digest(b"x"))), Err(Error::Cancelled));
        s.cancel();
        assert_eq!(s.snapshot().request_bytes, failed.request_bytes);
    }
    #[test]
    fn request_and_wire_exact_bound_plus_minus_one_are_independent() {
        let raw = descriptor(0, digest(b""));
        for max in [raw.len() as u64 - 1, raw.len() as u64, raw.len() as u64 + 1] {
            let mut s = Receiver::new(Limits {
                max_request_bytes: max,
                ..Limits::default()
            })
            .unwrap();
            if max < raw.len() as u64 {
                assert_eq!(s.accept(&raw), Err(Error::Limit));
                assert_eq!(s.snapshot().requests, 0);
            } else {
                assert!(s.accept(&raw).is_ok());
            }
        }
        let mut s = Receiver::new(Limits {
            max_wire_bytes: raw.len() as u64,
            ..Limits::default()
        })
        .unwrap();
        s.accept(&raw).unwrap();
        assert_eq!(s.accept(&end(0, digest(b""))), Err(Error::Limit));
        assert_eq!(s.snapshot().requests, 1);
    }
    #[test]
    fn request_receipt_limits_and_checked_math_preserve_semantics() {
        let mut s = Receiver::new(Limits {
            max_requests: 1,
            ..Limits::default()
        })
        .unwrap();
        s.accept(&descriptor(0, digest(b""))).unwrap();
        assert_eq!(s.accept(&end(0, digest(b""))), Err(Error::Limit));
        let mut s = Receiver::new(Limits {
            max_receipts: 1,
            ..Limits::default()
        })
        .unwrap();
        s.accept(&descriptor(2, digest(b"ab"))).unwrap();
        s.accept(&chunk(1, 0, b"a").encode().unwrap()).unwrap();
        assert_eq!(
            s.accept(&chunk(2, 1, b"b").encode().unwrap()),
            Err(Error::Limit)
        );
        assert_eq!(s.snapshot().received_bytes, 1);
        s.snapshot.next_sequence = u64::MAX;
        assert_eq!(
            s.accept(&chunk(u64::MAX, 1, b"b").encode().unwrap()),
            Err(Error::Limit)
        );
        assert_eq!(s.snapshot().received_bytes, 1);
    }
    #[test]
    fn maximum_object_streams_across_chunk_boundary_without_spool() {
        for total in [
            crate::MAX_CHUNK_BYTES - 1,
            crate::MAX_CHUNK_BYTES,
            crate::MAX_CHUNK_BYTES + 1,
            MAX_OBJECT_BYTES as usize - 1,
            MAX_OBJECT_BYTES as usize,
        ] {
            let bytes = vec![0x81; total];
            let mut s = ready(&bytes);
            let mut offset = 0;
            for (sequence, payload) in bytes.chunks(crate::MAX_CHUNK_BYTES).enumerate() {
                s.accept(
                    &chunk(sequence as u64 + 1, offset, payload)
                        .encode()
                        .unwrap(),
                )
                .unwrap();
                offset += payload.len() as u64;
                assert!(s.snapshot().retained_recent_wire_bytes as usize <= MAX_FRAME_BYTES);
            }
            assert!(matches!(
                s.accept(&end(total as u64, digest(&bytes))),
                Ok(Acceptance::VerifiedBytes { .. })
            ));
            assert_eq!(s.snapshot().retained_recent_requests, 0);
        }
        assert_eq!(
            Receiver::new(Limits {
                max_payload_bytes: MAX_OBJECT_BYTES + 1,
                ..Limits::default()
            })
            .err(),
            Some(Error::Limit)
        );
    }
    #[test]
    fn malformed_and_wrong_direction_are_charged_without_descriptor_or_grant() {
        let mut s = Receiver::new(Limits::default()).unwrap();
        assert_eq!(s.accept(&[0; 8]), Err(Error::Invalid));
        assert_eq!(s.snapshot().requests, 1);
        assert_eq!(s.snapshot().phase, Phase::Empty);
        assert_eq!(
            s.accept(&chunk(1, 0, b"").encode().unwrap()),
            Err(Error::State)
        );
        assert_eq!(s.snapshot().identity, None);
        assert_eq!(s.snapshot().received_bytes, 0);
    }
}
