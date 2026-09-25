//! Shared bounded sealing; platform adapters own the key and identity lease.
use super::{AuditError, TrustedLog, from_pending, proto, verify};
use crate::store::Store;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Debug, PartialEq, Eq)]
pub struct Progress {
    pub segments: u32,
    pub events: u32,
    pub more_pending: bool,
}
/// A successful seal atomically stores the signature and confirms pending
/// events. The platform callback runs after each committed batch.
pub fn flush(
    store: &mut Store,
    trust: &TrustedLog,
    max_batches: u32,
    mut sign: impl FnMut(&proto::Segment) -> super::Result<Vec<u8>>,
    mut after_batch: impl FnMut(),
) -> Result<Progress> {
    if max_batches == 0 || max_batches > 16 { return Err("sealing batch budget".into()); }
    let mut progress = Progress { segments: 0, events: 0, more_pending: false };
    let tip = store.last_sealed_segment()?;
    let (mut index, mut previous) = if let Some(bytes) = tip {
        let value = verify(&bytes, trust)?;
        (value.segment().index.checked_add(1).ok_or("segment overflow")?, value.digest())
    } else { (1, [0;32]) };
    for _ in 0..max_batches {
        let mut limit = 128;
        let (signed, count) = loop {
            let pending = match store.pending(0, limit) {
                Ok(value) => value,
                Err(crate::Error::Limit) if limit > 1 => { limit /= 2; continue; },
                Err(error) => return Err(error.into()),
            };
            if pending.is_empty() { return Ok(progress); }
            let segment = match from_pending(trust, index, previous, &pending) {
                Ok(value) => value,
                Err(AuditError::Limit) if limit > 1 => { limit /= 2; continue; },
                Err(error) => return Err(error.into()),
            };
            break (sign(&segment)?, pending.len() as u32);
        };
        store.seal_pending(&signed)?;
        after_batch();
        previous = verify(&signed, trust)?.digest();
        index = index.checked_add(1).ok_or("segment overflow")?;
        progress.segments += 1;
        progress.events += count;
    }
    progress.more_pending = !store.pending(0, 1)?.is_empty();
    Ok(progress)
}
