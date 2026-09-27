//! Trusted clock/cancellation boundary for one synchronous mutation operation.
use super::{Error, Result};

/// A host adapter must sample its original monotonic clock and execute `action`
/// under the SAME serialization used by other users of that clock. Cancellation
/// is operation-local; it never changes time or revokes a whole instance.
/// Implementations call the action exactly once with a current cancellation
/// snapshot. Once cancelled, a command must keep cancellation latched; a new
/// operation uses a new control. The action only validates authority/admits budget, never performs
/// filesystem IO, waits for a worker, or executes guest code.
///
/// OS calls between checks are synchronous and cannot be forcibly interrupted.
/// Cancellation after a durable claim means Unknown, not rollback or permission
/// to replay. An already observed effect is persisted even if delivery is cancelled.
pub trait TargetControl {
    fn with<T>(&mut self, action: impl FnOnce(u64, bool) -> T) -> T;
}

pub(super) struct LocalControl<F>(pub(super) F);
impl<F: FnMut() -> u64> TargetControl for LocalControl<F> {
    fn with<T>(&mut self, action: impl FnOnce(u64, bool) -> T) -> T {
        action((self.0)(), false)
    }
}
pub(super) fn running(cancelled: bool) -> Result<()> {
    if cancelled {
        Err(Error::CancelledBeforeDispatch)
    } else {
        Ok(())
    }
}
pub(super) fn controlled<T>(
    control: &mut impl TargetControl,
    action: impl FnOnce(u64) -> Result<T>,
) -> Result<T> {
    control.with(|now, cancelled| {
        running(cancelled)?;
        action(now)
    })
}
pub(super) fn delivery(result: Result<()>) -> Result<()> {
    result.map_err(|error| match error {
        Error::CancelledBeforeDispatch => Error::CommittedButDeliveryCancelled,
        Error::Admission(error) => Error::CommittedButDeliveryDenied(error),
        error => error,
    })
}
