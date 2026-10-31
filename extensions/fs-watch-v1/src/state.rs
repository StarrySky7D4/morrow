//! Consumer bookkeeping only; the original owner holds every real budget/grant.
use crate::{Batch, End, Error, Result, wire_digest};

pub const MAX_FRAMES: u32 = 4096;
pub const MAX_TOTAL_CHANGES: u32 = 16384;
pub const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub frames: u32,
    pub changes: u32,
    pub wire_bytes: u64,
}
impl Limits {
    pub fn validate(self) -> Result<()> {
        if self.frames == 0
            || self.frames > MAX_FRAMES
            || self.changes > MAX_TOTAL_CHANGES
            || self.wire_bytes == 0
            || self.wire_bytes > MAX_TOTAL_BYTES
        {
            Err(Error::Limit)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub frames: u32,
    pub changes: u32,
    pub wire_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Pending,
    Ended(End),
    Unknown,
}

#[derive(Clone, Copy)]
struct Pending {
    sequence: u64,
    digest: [u8; 32],
    end: End,
}

pub struct ClientState {
    epoch: [u8; 32],
    next: u64,
    limits: Limits,
    usage: Usage,
    phase: Phase,
    pending: Option<Pending>,
}
impl std::fmt::Debug for ClientState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientState")
            .field("phase", &self.phase)
            .field("usage", &self.usage)
            .finish_non_exhaustive()
    }
}
impl ClientState {
    pub fn new(epoch: [u8; 32], limits: Limits) -> Result<Self> {
        limits.validate()?;
        if epoch == [0; 32] {
            return Err(Error::Invalid);
        }
        Ok(Self {
            epoch,
            next: 1,
            limits,
            usage: Usage::default(),
            phase: Phase::Ready,
            pending: None,
        })
    }
    pub const fn phase(&self) -> Phase {
        self.phase
    }
    pub const fn usage(&self) -> Usage {
        self.usage
    }
    /// Accepted bytes remain charged before ACK, including a lost delivery.
    pub fn receive(&mut self, wire: &[u8]) -> Result<Batch> {
        match self.phase {
            Phase::Pending => return Err(Error::Pending),
            Phase::Unknown => return Err(Error::OutcomeUnknown),
            Phase::Ended(_) => return Err(Error::State),
            Phase::Ready => {}
        }
        let batch = match Batch::decode(wire) {
            Ok(batch) => batch,
            Err(error) => {
                self.phase = Phase::Unknown;
                return Err(error);
            }
        };
        if batch.watch_epoch != self.epoch || batch.sequence != self.next {
            self.phase = Phase::Unknown;
            return Err(Error::Correlation);
        }
        let usage = Usage {
            frames: self.usage.frames.checked_add(1).ok_or(Error::Budget)?,
            changes: self
                .usage
                .changes
                .checked_add(batch.changes.len() as u32)
                .ok_or(Error::Budget)?,
            wire_bytes: self
                .usage
                .wire_bytes
                .checked_add(wire.len() as u64)
                .ok_or(Error::Budget)?,
        };
        if usage.frames > self.limits.frames
            || usage.changes > self.limits.changes
            || usage.wire_bytes > self.limits.wire_bytes
        {
            self.phase = Phase::Unknown;
            return Err(Error::Budget);
        }
        self.usage = usage;
        self.pending = Some(Pending {
            sequence: batch.sequence,
            digest: wire_digest(wire),
            end: batch.end,
        });
        self.phase = Phase::Pending;
        Ok(batch)
    }
    /// Delivery acknowledgement only. It cannot certify a filesystem mutation,
    /// flush, source cancellation, checked native handle close, or worker join.
    pub fn acknowledge(&mut self, sequence: u64, digest: [u8; 32]) -> Result<()> {
        if self.phase == Phase::Unknown {
            return Err(Error::OutcomeUnknown);
        }
        if self.phase != Phase::Pending {
            return Err(Error::State);
        }
        let pending = self.pending.ok_or(Error::State)?;
        if pending.sequence != sequence || pending.digest != digest {
            return Err(Error::Correlation);
        }
        self.pending = None;
        if pending.end == End::Continue {
            self.next = pending
                .sequence
                .checked_add(1)
                .ok_or(Error::OutcomeUnknown)?;
            self.phase = Phase::Ready;
        } else {
            self.phase = Phase::Ended(pending.end);
        }
        Ok(())
    }
    /// A transport loss never resets epoch, usage or the original source.
    /// An already acknowledged terminal observation remains a historical fact.
    pub fn transport_lost(&mut self) {
        if !matches!(self.phase, Phase::Ended(_)) {
            self.phase = Phase::Unknown;
        }
    }
}
