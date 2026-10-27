//! Single outstanding request with cumulative accounting. This helper cannot
//! grant permission, restore a live owner, retry an uncertain call or prove join.
use crate::{
    ActionKind, ActionRef, Descriptor, Error, MAX_EMBEDDED_FRAME_BYTES, MAX_RESPONSE_BYTES, Phase,
    ReplyRef, Request, Response, Result, State, Status, blob,
};
use morrow_blob_transfer_v1::ActionRef as BlobAction;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug)]
pub struct ClientLimits {
    pub max_calls: u32,
    pub max_wire_bytes: u64,
}
impl Default for ClientLimits {
    fn default() -> Self {
        Self {
            max_calls: 1024,
            max_wire_bytes: 64 * 1024 * 1024,
        }
    }
}
impl ClientLimits {
    fn validate(self) -> Result<()> {
        if self.max_calls == 0
            || self.max_calls > 4096
            || self.max_wire_bytes == 0
            || self.max_wire_bytes > 64 * 1024 * 1024
        {
            Err(Error::Limit)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientPhase {
    Fresh,
    Ready,
    Receiving,
    Staged,
    Released,
    Observed,
    Rejected,
    Closed,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientSnapshot {
    pub phase: ClientPhase,
    pub calls: u32,
    pub wire_bytes: u64,
    pub received_bytes: u64,
    pub next_sequence: u64,
    pub pending: bool,
}
pub struct Client {
    nomination: [u8; 32],
    descriptor: Descriptor,
    limits: ClientLimits,
    snapshot: ClientSnapshot,
    nonces: Vec<[u8; 32]>,
    pending: Option<Request>,
    hash: Sha256,
}
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadClient")
            .field("snapshot", &self.snapshot())
            .finish_non_exhaustive()
    }
}
impl Client {
    pub fn new(nomination: [u8; 32], descriptor: Descriptor, limits: ClientLimits) -> Result<Self> {
        crate::id(&nomination)?;
        descriptor.validate()?;
        limits.validate()?;
        let mut nonces = Vec::new();
        nonces
            .try_reserve_exact(limits.max_calls as usize)
            .map_err(|_| Error::Limit)?;
        if nonces.capacity() > limits.max_calls as usize {
            return Err(Error::Limit);
        }
        Ok(Self {
            nomination,
            descriptor,
            limits,
            snapshot: ClientSnapshot {
                phase: ClientPhase::Fresh,
                calls: 0,
                wire_bytes: 0,
                received_bytes: 0,
                next_sequence: 1,
                pending: false,
            },
            nonces,
            pending: None,
            hash: Sha256::new(),
        })
    }
    pub fn snapshot(&self) -> ClientSnapshot {
        self.snapshot
    }
    /// Original pending bytes are retained after uncertainty for inspection, not
    /// for automatic retransmission. The helper has no retry method.
    pub fn pending(&self) -> Option<&Request> {
        self.pending.as_ref()
    }
    pub fn descriptor(&self) -> Descriptor {
        self.descriptor
    }
    pub(crate) fn nomination_ref(&self) -> [u8; 32] {
        self.nomination
    }
    pub(crate) fn owned_regions(&self) -> [(usize, usize); 2] {
        let pending = self.pending.as_ref().map_or((0, 0), |request| {
            (request.wire.as_ptr().addr(), request.wire.capacity())
        });
        [
            (
                self.nonces.as_ptr().addr(),
                self.nonces.capacity() * size_of::<[u8; 32]>(),
            ),
            pending,
        ]
    }
    fn live(&self) -> Result<()> {
        if matches!(
            self.snapshot.phase,
            ClientPhase::Rejected | ClientPhase::Closed | ClientPhase::Unknown
        ) {
            Err(Error::State)
        } else {
            Ok(())
        }
    }
    fn admit_action(&self, action: ActionRef<'_>) -> Result<()> {
        self.live()?;
        match action {
            ActionRef::Open if self.snapshot.phase == ClientPhase::Fresh => Ok(()),
            ActionRef::Inspect | ActionRef::Release => Ok(()),
            ActionRef::Push(bytes) => {
                let frame = blob(bytes, MAX_EMBEDDED_FRAME_BYTES)?;
                if frame.identity != self.descriptor.identity {
                    return Err(Error::Correlation);
                }
                match frame.action {
                    BlobAction::Descriptor {
                        total_length,
                        whole_sha256,
                    } if self.snapshot.phase == ClientPhase::Ready => {
                        if total_length == self.descriptor.byte_length
                            && whole_sha256 == self.descriptor.sha256
                        {
                            Ok(())
                        } else {
                            Err(Error::Correlation)
                        }
                    }
                    BlobAction::Chunk {
                        sequence,
                        offset,
                        payload,
                        ..
                    } if self.snapshot.phase == ClientPhase::Receiving => {
                        let end = offset
                            .checked_add(payload.len() as u64)
                            .ok_or(Error::Limit)?;
                        if sequence == self.snapshot.next_sequence
                            && offset == self.snapshot.received_bytes
                            && end <= self.descriptor.byte_length
                        {
                            Ok(())
                        } else {
                            Err(Error::Correlation)
                        }
                    }
                    BlobAction::End {
                        total_length,
                        whole_sha256,
                    } if self.snapshot.phase == ClientPhase::Receiving => {
                        if total_length == self.descriptor.byte_length
                            && whole_sha256 == self.descriptor.sha256
                            && self.snapshot.received_bytes == total_length
                            && <[u8; 32]>::from(self.hash.clone().finalize()) == whole_sha256
                        {
                            Ok(())
                        } else {
                            Err(Error::Correlation)
                        }
                    }
                    _ => Err(Error::State),
                }
            }
            _ => Err(Error::State),
        }
    }
    pub fn begin(&mut self, nonce: [u8; 32], action: ActionRef<'_>) -> Result<&Request> {
        if self.pending.is_some() {
            return Err(Error::State);
        }
        self.admit_action(action)?;
        if self.nonces.contains(&nonce) {
            return Err(Error::Correlation);
        }
        if self.snapshot.calls >= self.limits.max_calls {
            return Err(Error::Limit);
        }
        let request = Request::new(self.nomination, nonce, action)?;
        let bytes = self
            .snapshot
            .wire_bytes
            .checked_add(request.wire().len() as u64)
            .ok_or(Error::Limit)?;
        if bytes > self.limits.max_wire_bytes {
            return Err(Error::Limit);
        }
        self.snapshot.calls += 1;
        self.snapshot.wire_bytes = bytes;
        self.nonces.push(nonce);
        self.pending = Some(request);
        self.snapshot.pending = true;
        Ok(self.pending.as_ref().expect("original admitted request"))
    }
    /// Every accepted/invalid response pays its actual bounded wire charge. A
    /// malformed, foreign, over-budget or lost response seals Unknown and leaves
    /// the original request retained. No fresh request is then admitted.
    pub fn accept(&mut self, bytes: &[u8]) -> Result<Response> {
        self.live()?;
        if self.pending.is_none() {
            return Err(Error::State);
        }
        match self.accept_inner(bytes) {
            Ok(response) => {
                if self.snapshot.phase != ClientPhase::Unknown {
                    self.pending = None;
                    self.snapshot.pending = false;
                }
                Ok(response)
            }
            Err(error) => {
                self.snapshot.phase = ClientPhase::Unknown;
                Err(error)
            }
        }
    }
    fn accept_inner(&mut self, bytes: &[u8]) -> Result<Response> {
        crate::bounded(bytes, MAX_RESPONSE_BYTES)?;
        let charged = self
            .snapshot
            .wire_bytes
            .checked_add(bytes.len() as u64)
            .ok_or(Error::Limit)?;
        if charged > self.limits.max_wire_bytes {
            return Err(Error::Limit);
        }
        self.snapshot.wire_bytes = charged;
        let request = self.pending.as_ref().ok_or(Error::State)?;
        let response = Response::decode_for(bytes, request)?;
        match response.reply() {
            ReplyRef::Error(status) => {
                self.snapshot.phase = if status == Status::OutcomeUnknown {
                    ClientPhase::Unknown
                } else {
                    ClientPhase::Rejected
                };
            }
            ReplyRef::ChunkReceipt(_) => {
                let ActionRef::Push(original) = request.action() else {
                    return Err(Error::Correlation);
                };
                let BlobAction::Chunk { payload, .. } =
                    blob(original, MAX_EMBEDDED_FRAME_BYTES)?.action
                else {
                    return Err(Error::Correlation);
                };
                self.hash.update(payload);
                self.snapshot.received_bytes = self
                    .snapshot
                    .received_bytes
                    .checked_add(payload.len() as u64)
                    .ok_or(Error::Limit)?;
                self.snapshot.next_sequence = self
                    .snapshot
                    .next_sequence
                    .checked_add(1)
                    .ok_or(Error::Limit)?;
            }
            ReplyRef::State(state) => {
                if state.descriptor != self.descriptor {
                    return Err(Error::Correlation);
                }
                let action = request.action().kind();
                self.accept_state(action, state)?;
            }
        }
        Ok(response)
    }
    fn accept_state(&mut self, action: ActionKind, state: State) -> Result<()> {
        if (self.snapshot.phase == ClientPhase::Released && state.phase != Phase::Released)
            || (self.snapshot.phase == ClientPhase::Staged
                && !matches!(state.phase, Phase::Staged | Phase::Released))
            || (self.snapshot.phase == ClientPhase::Receiving
                && matches!(state.phase, Phase::Ready | Phase::Absent))
        {
            return Err(Error::Correlation);
        }
        self.snapshot.phase = match state.phase {
            Phase::Staged => ClientPhase::Staged,
            Phase::Released => ClientPhase::Released,
            Phase::Absent => ClientPhase::Observed,
            Phase::Ready if action == ActionKind::Open => ClientPhase::Ready,
            Phase::Receiving if action == ActionKind::Push => ClientPhase::Receiving,
            Phase::Ready
                if action == ActionKind::Inspect && self.snapshot.phase == ClientPhase::Ready =>
            {
                ClientPhase::Ready
            }
            Phase::Receiving
                if action == ActionKind::Inspect
                    && self.snapshot.phase == ClientPhase::Receiving =>
            {
                if state.received_bytes != self.snapshot.received_bytes
                    || state.next_sequence != self.snapshot.next_sequence
                {
                    return Err(Error::Correlation);
                }
                ClientPhase::Receiving
            }
            // History/inspection does not restore upload state or the hash of
            // bytes this client has never sent and acknowledged.
            _ => ClientPhase::Observed,
        };
        self.snapshot.received_bytes = state.received_bytes;
        self.snapshot.next_sequence = state.next_sequence;
        Ok(())
    }
    pub fn transport_unknown(&mut self) -> Result<()> {
        if self.pending.is_none() {
            return Err(Error::State);
        }
        self.snapshot.phase = ClientPhase::Unknown;
        Ok(())
    }
    /// Local intent only. It does not send a cancellation or release resources.
    pub fn abandon(&mut self) {
        self.snapshot.phase = if self.pending.is_some() {
            ClientPhase::Unknown
        } else {
            ClientPhase::Closed
        };
    }
    /// One synchronous, non-reentrant callback. A failure or invalid result is
    /// never retried. The callback cannot retain either borrowed buffer.
    pub fn call_once_with(
        &mut self,
        nonce: [u8; 32],
        action: ActionRef<'_>,
        call: impl FnOnce(&[u8], &mut [u8]) -> i32,
    ) -> Result<Response> {
        self.begin(nonce, action)?;
        let mut output = vec![0; MAX_RESPONSE_BYTES];
        let phase = self.snapshot.phase;
        // If a trusted callback unwinds, the original in-flight request remains
        // Unknown even when its caller catches that panic and retains Client.
        self.snapshot.phase = ClientPhase::Unknown;
        let length = call(
            self.pending
                .as_ref()
                .expect("original admitted request")
                .wire(),
            &mut output,
        );
        if length <= 0 || length as usize > output.len() {
            return Err(Error::State);
        }
        self.snapshot.phase = phase;
        self.accept(&output[..length as usize])
    }
}
