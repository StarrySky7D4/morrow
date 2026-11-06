//! Broker-held original transaction and the exact original resource reservation.
use super::transaction::{Failure, Transaction};
use crate::{
    file_target::{Error, Result},
    io_binding::{IoLease, IoResourceLease},
};
use std::{fs::Metadata, path::Path, time::Instant};

pub(in crate::file_target) struct ReplacementOwner {
    transaction: Transaction<(IoResourceLease, Option<IoLease>)>,
}

/// Root, each parent, target, one temporary file, and one transaction. Reserve
/// the entire group before opening anything; the temporary slot is never reused
/// to authorize a second payload.
pub(in crate::file_target) fn resources(path: &Path) -> Result<u32> {
    crate::file_target::validate_path(path)?;
    let spelling = path.to_str().ok_or(Error::InvalidSelection)?;
    let prefix = if spelling.starts_with(r"\\?\") { 7 } else { 3 };
    let count = spelling[prefix..]
        .split(['\\', '/'])
        .count()
        .checked_add(3)
        .ok_or(Error::Limit)?;
    if count > 128 {
        return Err(Error::Limit);
    }
    u32::try_from(count).map_err(|_| Error::Limit)
}

fn before_claim(error: Failure) -> Error {
    match error {
        Failure::Unsupported(_) | Failure::Os(1 | 50 | 6801 | 6805) => {
            Error::UnsupportedConditionalReplacement
        }
        Failure::Limit => Error::Limit,
        Failure::Inactive => Error::Admission(crate::io_binding::Error::Expired),
        Failure::InvalidBounds | Failure::SelectionMismatch => Error::InvalidSelection,
        Failure::Os(code) => Error::Io(std::io::Error::from_raw_os_error(code as i32).kind()),
        Failure::CloseUnknown | Failure::ResolutionUnknown | Failure::RestartRequired => {
            Error::RestartRequired
        }
    }
}

impl ReplacementOwner {
    pub(in crate::file_target) fn select(
        path: &Path,
        deadline: Instant,
        lease: IoResourceLease,
    ) -> Result<Self> {
        let capacity = resources(path)?;
        if lease.resources() != capacity {
            return Err(Error::Limit);
        }
        let mut transaction =
            Transaction::begin_until(deadline, (capacity - 1) as usize, (lease, None))
                .map_err(before_claim)?;
        transaction.select_local(path).map_err(before_claim)?;
        Ok(Self { transaction })
    }
    pub(in crate::file_target) fn lease(&self) -> &IoResourceLease {
        &self.transaction.reservation().0
    }
    pub(in crate::file_target) fn metadata(&self) -> Result<Metadata> {
        self.transaction.selected_metadata().map_err(before_claim)
    }
    /// Consumed only after the original durable claim and selection tombstone.
    /// Any uncertain resolution retains the entire original resource charge.
    pub(in crate::file_target) fn publish(
        mut self,
        temporary: &str,
        content: &[u8],
        job: IoLease,
        mut authorize: impl FnMut() -> bool,
    ) -> Result<morrow_core::file_effect::ReplaceResult> {
        use morrow_core::file_effect::ReplaceResult;
        let slot = &mut self.transaction.reservation_mut().1;
        assert!(
            slot.is_none(),
            "original replacement job can attach only once"
        );
        *slot = Some(job);
        if let Err(first) = self
            .transaction
            .stage_replacement(temporary, content, &mut authorize)
        {
            // Roll back exactly once, before any attempted commit. This never
            // erases the durable claim or makes the original selection reusable.
            self.transaction
                .rollback()
                .map_err(|_| Error::OutcomeUnknown)?;
            return match first {
                Failure::Os(code) if code != 0 => Ok(ReplaceResult::OsRejected { code }),
                _ => Err(Error::OutcomeUnknown),
            };
        }
        self.transaction
            .commit(authorize)
            .map_err(|_| Error::OutcomeUnknown)?;
        Ok(ReplaceResult::Replaced)
    }
}
