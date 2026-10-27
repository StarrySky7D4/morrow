//! Trusted-host streaming adapter from existing Cap'n Proto blob frames to the
//! original Store. This is not a guest capability, file selector or attachment
//! commit. Chunk receipts are byte-validation facts, never durable receipts.
use morrow_blob_transfer_v1::{
    Acceptance, ActionRef, FrameRef, Identity, Limits, MAX_FRAME_BYTES, Phase, ReceiptStatus,
    Receiver, Snapshot,
};
use morrow_core::{attachment::BlobInfo, blob_stage::Command, store::Store};
use sha2::{Digest, Sha256};
use std::io::{self, Read};
use zeroize::Zeroizing;

/// The caller supplies at most one original frame per invocation. Zero denotes
/// the actual end of this source, not temporary unavailability. Source callbacks
/// run synchronously on the original owner and must not re-enter that owner.
/// Their native work cannot be preempted; checks run before and after each call.
/// The source owns/bounds its own state; this adapter never opens a path.
pub trait FrameSource: Send {
    fn read_frame(&mut self, destination: &mut [u8]) -> io::Result<usize>;
}

#[derive(Clone)]
pub struct Spec {
    pub owner_domain: [u8; 32],
    pub identity: Identity,
    pub byte_length: u64,
    pub sha256: [u8; 32],
    pub limits: Limits,
}
impl std::fmt::Debug for Spec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlobUploadSpec")
            .field("byte_length", &self.byte_length)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}
impl Spec {
    pub fn validate(&self) -> Result<(), Error> {
        self.identity.validate().map_err(Error::Protocol)?;
        self.limits.validate().map_err(Error::Protocol)?;
        self.command().validate().map_err(Error::Storage)?;
        if self.byte_length == 0 && self.sha256 != <[u8; 32]>::from(Sha256::digest([])) {
            return Err(Error::Protocol(morrow_blob_transfer_v1::Error::Digest));
        }
        if self.byte_length > self.limits.max_payload_bytes {
            return Err(Error::Protocol(morrow_blob_transfer_v1::Error::Limit));
        }
        Ok(())
    }
    /// Exact durable lookup key. All three original wire identities contribute;
    /// it never grants permission or restores a live transfer after restart.
    pub fn command(&self) -> Command {
        let mut hash = Sha256::new();
        hash.update(b"morrow.local-blob-stage.v1\0");
        hash.update(self.identity.transfer_epoch);
        hash.update(self.identity.object_ref);
        hash.update(self.identity.operation_id);
        let operation_id = hash.finalize().iter().map(|v| format!("{v:02x}")).collect();
        Command {
            owner_domain: self.owner_domain,
            operation_id,
            byte_length: self.byte_length,
            sha256: self.sha256,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Protocol(morrow_blob_transfer_v1::Error),
    Source,
    Vetoed,
    Storage(morrow_core::Error),
    /// The commit may exist, but the original delivery/liveness check failed.
    /// Inspect the exact original receipt; do not start another upload.
    DeliveryUnknown,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "blob upload: {self:?}")
    }
}
impl std::error::Error for Error {}

pub struct Staged {
    pub blob: BlobInfo,
    pub existing: bool,
    pub received: Snapshot,
}
impl std::fmt::Debug for Staged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Staged")
            .field("byte_length", &self.blob.byte_length)
            .field("existing", &self.existing)
            .field("received", &self.received)
            .finish()
    }
}

struct Stream<'a, F> {
    source: &'a mut dyn FrameSource,
    check: F,
    spec: &'a Spec,
    receiver: Receiver,
    wire: Zeroizing<Vec<u8>>,
    pending: std::ops::Range<usize>,
    eof: bool,
    failure: Option<Error>,
}
impl<F: FnMut() -> bool> Stream<'_, F> {
    fn next(&mut self) -> Result<(), Error> {
        if !(self.check)() {
            return Err(Error::Vetoed);
        }
        let length = self
            .source
            .read_frame(&mut self.wire)
            .map_err(|_| Error::Source)?;
        if !(self.check)() {
            return Err(Error::Vetoed);
        }
        if length > MAX_FRAME_BYTES {
            return Err(Error::Protocol(morrow_blob_transfer_v1::Error::Limit));
        }
        if length == 0 {
            if self.receiver.snapshot().phase != Phase::VerifiedBytes {
                return Err(Error::Protocol(morrow_blob_transfer_v1::Error::Incomplete));
            }
            self.eof = true;
            return Ok(());
        }
        let wire = &self.wire[..length];
        let accepted = self.receiver.accept(wire).map_err(Error::Protocol)?;
        let frame = FrameRef::decode(wire).map_err(Error::Protocol)?;
        if frame.identity != self.spec.identity {
            return Err(Error::Protocol(morrow_blob_transfer_v1::Error::Identity));
        }
        match (accepted, frame.action) {
            (
                Acceptance::DescriptorAccepted,
                ActionRef::Descriptor {
                    total_length,
                    whole_sha256,
                },
            ) => {
                if total_length != self.spec.byte_length || whole_sha256 != self.spec.sha256 {
                    return Err(Error::Protocol(morrow_blob_transfer_v1::Error::Digest));
                }
            }
            (Acceptance::Receipt(receipt), ActionRef::Chunk { payload, .. }) => {
                if matches!(
                    receipt.action,
                    morrow_blob_transfer_v1::Action::Receipt {
                        status: ReceiptStatus::Accepted,
                        ..
                    }
                ) {
                    // The codec has already checked that this is a sub-slice of
                    // this original wire. Keep offsets, never a self-reference.
                    let start = (payload.as_ptr() as usize)
                        .checked_sub(wire.as_ptr() as usize)
                        .ok_or(Error::Protocol(morrow_blob_transfer_v1::Error::Buffer))?;
                    let end = start
                        .checked_add(payload.len())
                        .filter(|n| *n <= length)
                        .ok_or(Error::Protocol(morrow_blob_transfer_v1::Error::Buffer))?;
                    self.pending = start..end;
                }
            }
            (Acceptance::VerifiedBytes { .. }, ActionRef::End { .. }) => {}
            _ => return Err(Error::Protocol(morrow_blob_transfer_v1::Error::State)),
        }
        Ok(())
    }
}
impl<F: FnMut() -> bool> Read for Stream<'_, F> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.failure.is_some() {
            return Err(io::Error::other("blob source closed"));
        }
        loop {
            if !self.pending.is_empty() {
                if !(self.check)() {
                    self.failure = Some(Error::Vetoed);
                    return Err(io::Error::other("blob source vetoed"));
                }
                let length = output.len().min(self.pending.len());
                output[..length]
                    .copy_from_slice(&self.wire[self.pending.start..self.pending.start + length]);
                self.pending.start += length;
                return Ok(length);
            }
            if self.eof {
                return Ok(0);
            }
            if let Err(error) = self.next() {
                self.failure = Some(error);
                return Err(io::Error::other("blob source rejected"));
            }
        }
    }
}

/// One original synchronous Store transaction, using bounded frames rather than
/// a whole-object spool. Only a validated End followed by actual source EOF can
/// commit. Exact prior success does not read the replacement source. The check
/// can veto, never authorize; the trusted caller retains all original authority.
pub fn stage_local(
    store: &mut Store,
    source: &mut dyn FrameSource,
    spec: &Spec,
    now: i64,
    mut check: impl FnMut() -> bool,
) -> Result<Staged, Error> {
    spec.validate()?;
    if !check() {
        return Err(Error::Vetoed);
    }
    let check = std::cell::RefCell::new(check);
    let vetoed = std::cell::Cell::new(false);
    let mut stream = Stream {
        source,
        check: || (check.borrow_mut())(),
        spec,
        receiver: Receiver::new(spec.limits).map_err(Error::Protocol)?,
        wire: Zeroizing::new(vec![0; MAX_FRAME_BYTES]),
        pending: 0..0,
        eof: false,
        failure: None,
    };
    let command = spec.command();
    let result = store.stage_blob_operation_local_authorized(&mut stream, &command, now, || {
        if (check.borrow_mut())() {
            Ok(())
        } else {
            vetoed.set(true);
            Err(morrow_core::Error::Invalid("blob upload veto"))
        }
    });
    let blob = match result {
        Ok(blob) => blob,
        Err(_) if vetoed.get() => return Err(Error::Vetoed),
        Err(morrow_core::Error::Io) if stream.failure.is_some() => {
            return Err(stream.failure.take().unwrap());
        }
        Err(error) => return Err(Error::Storage(error)),
    };
    if !(stream.check)() {
        return Err(Error::DeliveryUnknown);
    }
    let received = stream.receiver.snapshot();
    Ok(Staged {
        blob,
        existing: received.phase == Phase::Empty,
        received,
    })
}
