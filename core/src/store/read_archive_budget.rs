//! Admission accounting over bounded unpublished manifests, inside the writer transaction.
//! Complete data integrity is verified by Store opening/publication/cursor paths; this
//! counter is not a tamper-proof database or a filesystem free-space guarantee.
use super::{Store, read_archive, sql};
use crate::{
    Error, Result,
    read_archive::{
        MAX_CONTAINER_BYTES, MAX_PREPARATIONS, Manifest, PreparationBudget, PreparationUsage,
    },
};
use rusqlite::{Connection, params};

pub(super) const INDEX: &str =
    "CREATE INDEX IF NOT EXISTS read_archive_preparations ON read_archives(published,operation_id)";
fn cost(manifest: &Manifest) -> Result<u64> {
    manifest
        .status()
        .logical_bytes
        .checked_add(manifest.raw().len() as u64)
        .and_then(|v| v.checked_add(manifest.container().len() as u64))
        .ok_or(Error::Limit)
}
fn usage(connection: &Connection) -> Result<PreparationUsage> {
    let mut statement = sql(connection.prepare("SELECT operation_id,subject,CASE WHEN length(payload)<=?1 THEN payload ELSE NULL END FROM read_archives WHERE published=0 ORDER BY operation_id LIMIT ?2"))?;
    let mut rows = sql(statement.query(params![MAX_CONTAINER_BYTES as i64, MAX_PREPARATIONS + 1]))?;
    let mut total = PreparationUsage::default();
    while let Some(row) = sql(rows.next())? {
        // Existing older data over the new cap is preserved: only further growth is refused.
        if total.archives >= MAX_PREPARATIONS {
            return Err(Error::Limit);
        }
        let operation = sql(row.get_ref(0))?;
        let subject = sql(row.get_ref(1))?;
        let operation = operation.as_str().map_err(|_| Error::Integrity)?;
        let subject = subject.as_str().map_err(|_| Error::Integrity)?;
        let raw = sql(row.get_ref(2))?;
        if matches!(raw, rusqlite::types::ValueRef::Null) {
            return Err(Error::Limit);
        }
        let manifest = Manifest::decode(raw.as_blob().map_err(|_| Error::Integrity)?)?;
        let status = manifest.status();
        if status.plan.operation_id != operation
            || status.plan.subject != subject
            || status.root.is_some()
        {
            return Err(Error::Integrity);
        }
        read_archive::verify_association(connection, &manifest)?;
        total.archives += 1;
        total.logical_bytes = total
            .logical_bytes
            .checked_add(cost(&manifest)?)
            .ok_or(Error::Limit)?;
    }
    total.logical_bytes = total
        .logical_bytes
        .checked_add(super::read_capture::preparing_charge(connection)?)
        .ok_or(Error::Limit)?;
    Ok(total)
}
pub(super) fn admit(
    connection: &Connection,
    old: Option<&Manifest>,
    next: &Manifest,
    budget: PreparationBudget,
) -> Result<()> {
    budget.validate()?;
    let mut total = usage(connection)?;
    if let Some(old) = old {
        total.logical_bytes = total
            .logical_bytes
            .checked_sub(cost(old)?)
            .ok_or(Error::Integrity)?;
    } else {
        total.archives = total.archives.checked_add(1).ok_or(Error::Limit)?;
    }
    total.logical_bytes = total
        .logical_bytes
        .checked_add(cost(next)?)
        .ok_or(Error::Limit)?;
    if total.archives > budget.max_archives || total.logical_bytes > budget.max_bytes {
        return Err(Error::Limit);
    }
    Ok(())
}
impl Store {
    /// Bounded admission counters, not an integrity attestation or a persisted policy.
    /// A legacy database with more than MAX_PREPARATIONS requires explicit cleanup;
    /// its data is retained and growth counters return Limit until it is within count bounds.
    pub fn read_archive_preparation_usage(&self) -> Result<PreparationUsage> {
        let tx = sql(self.connection.unchecked_transaction())?;
        let version: i64 = sql(tx.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
        if !matches!(version, 12..=17) {
            return Err(Error::UnsupportedVersion);
        }
        usage(&tx)
    }
}
