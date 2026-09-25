//! Bounded host-owned sealing service; key creation is always a separate action.
use crate::{TrustedLog, keys::Key};
use morrow_core::store::Store;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct Sealer {
    key: Key,
    _identity: crate::identity::Lease,
}
pub use morrow_core::audit::sealing::Progress;
impl Sealer {
    pub fn new(key: Key) -> std::result::Result<Self, crate::identity::LeaseError> {
        let identity = crate::identity::Lease::acquire(&key.trust())?;
        Ok(Self {
            key,
            _identity: identity,
        })
    }
    pub fn trust(&self) -> TrustedLog {
        self.key.trust()
    }
    /// Bound work per call; oversized groups shrink without skipping any event.
    pub fn flush(&self, store: &mut Store, max_batches: u32) -> Result<Progress> {
        morrow_core::audit::sealing::flush(store, &self.trust(), max_batches,
            |segment| self.key.sign(segment), || {
                #[cfg(feature = "fault-injection")]
                if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok("sealer-after-batch") {
                    std::process::exit(86);
                }
            })
    }
}
