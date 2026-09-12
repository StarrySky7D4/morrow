//! Bounded host-owned sealing service; key creation is always a separate action.
use crate::{TrustedLog, keys::Key};
use morrow_core::store::Store;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct Sealer {
    key: Key,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Progress {
    pub segments: u32,
    pub events: u32,
    pub more_pending: bool,
}
impl Sealer {
    pub fn new(key: Key) -> Self {
        Self { key }
    }
    pub fn trust(&self) -> TrustedLog {
        self.key.trust()
    }
    /// Bound work per call; oversized groups shrink without skipping any event.
    pub fn flush(&self, store: &mut Store, max_batches: u32) -> Result<Progress> {
        if max_batches == 0 || max_batches > 16 {
            return Err("sealing batch budget".into());
        }
        let mut progress = Progress {
            segments: 0,
            events: 0,
            more_pending: false,
        };
        let tip = store.last_sealed_segment()?;
        let (mut index, mut previous) = if let Some(bytes) = tip {
            let v = crate::verify(&bytes, &self.trust())?;
            (
                v.segment().index.checked_add(1).ok_or("segment overflow")?,
                v.digest(),
            )
        } else {
            (1, [0; 32])
        };
        for _ in 0..max_batches {
            let mut limit = 128;
            let (signed, count) = loop {
                let pending = match store.pending(0, limit) {
                    Ok(v) => v,
                    Err(morrow_core::Error::Limit) if limit > 1 => {
                        limit /= 2;
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                };
                if pending.is_empty() {
                    return Ok(progress);
                }
                let segment = match crate::from_pending(&self.trust(), index, previous, &pending) {
                    Ok(v) => v,
                    Err(crate::AuditError::Limit) if limit > 1 => {
                        limit /= 2;
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                };
                break (self.key.sign(&segment)?, pending.len() as u32);
            };
            store.seal_pending(&signed)?;
            #[cfg(feature = "fault-injection")]
            if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok("sealer-after-batch") {
                std::process::exit(86);
            }
            previous = crate::verify(&signed, &self.trust())?.digest();
            index = index.checked_add(1).ok_or("segment overflow")?;
            progress.segments += 1;
            progress.events += count;
        }
        progress.more_pending = !store.pending(0, 1)?.is_empty();
        Ok(progress)
    }
}
