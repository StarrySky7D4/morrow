//! Derived accounting for every archive in the same SQLite writer transaction.
//! Admission reads one total and one affected row. Full integrity separately verifies
//! these caches against original manifests and the already-verified ordered parts.
//! This is neither an arbitrary-SQL tamper boundary nor a physical disk reservation.
use super::{Store, boundary, read_archive, sql};
use crate::{
    Error, Result,
    read_archive::{Manifest, RetentionBudget, RetentionUsage},
};
use rusqlite::{Connection, OptionalExtension, params};

const SCHEMA: &str = "CREATE TABLE read_archive_costs(operation_id TEXT PRIMARY KEY REFERENCES read_archives(operation_id),logical_bytes INTEGER NOT NULL CHECK(logical_bytes>=0)) STRICT;
CREATE TABLE read_archive_totals(id INTEGER PRIMARY KEY CHECK(id=1),archives INTEGER NOT NULL CHECK(archives>=0),logical_bytes INTEGER NOT NULL CHECK(logical_bytes>=0)) STRICT;
INSERT INTO read_archive_totals(id,archives,logical_bytes) VALUES(1,0,0);";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
fn integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::Integrity)
}
fn cost(m: &Manifest) -> Result<u64> {
    m.status()
        .logical_bytes
        .checked_add(m.raw().len() as u64)
        .and_then(|v| v.checked_add(m.container().len() as u64))
        .ok_or(Error::Integrity)
}
fn totals(c: &Connection) -> Result<RetentionUsage> {
    let values: Option<(i64, i64)> = sql(c
        .query_row(
            "SELECT archives,logical_bytes FROM read_archive_totals WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    let (archives, bytes) = values.ok_or(Error::Integrity)?;
    Ok(RetentionUsage {
        archives: archives.try_into().map_err(|_| Error::Integrity)?,
        logical_bytes: bytes.try_into().map_err(|_| Error::Integrity)?,
    })
}
fn row_cost(c: &Connection, operation: &str) -> Result<Option<u64>> {
    let value: Option<i64> = sql(c
        .query_row(
            "SELECT logical_bytes FROM read_archive_costs WHERE operation_id=?1",
            [operation],
            |r| r.get(0),
        )
        .optional())?;
    value
        .map(|n| n.try_into().map_err(|_| Error::Integrity))
        .transpose()
}
fn write_totals(c: &Connection, total: RetentionUsage) -> Result<()> {
    if sql(c.execute(
        "UPDATE read_archive_totals SET archives=?1,logical_bytes=?2 WHERE id=1",
        params![integer(total.archives)?, integer(total.logical_bytes)?],
    ))? != 1
    {
        return Err(Error::Integrity);
    }
    Ok(())
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let count:i64=sql(c.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('read_archive_costs','read_archive_totals')",[],|r|r.get(0)))?;
    if count != if version(c)? < 14 { 0 } else { 2 } {
        return Err(Error::Integrity);
    }
    Ok(())
}
/// Streaming legacy calculation and rebuild: never applies admission ceilings.
fn scan(c: &Connection, mut visit: impl FnMut(&str, u64) -> Result<()>) -> Result<RetentionUsage> {
    let mut total = RetentionUsage::default();
    if version(c)? < 12 {
        return Ok(total);
    }
    let mut statement =
        sql(c.prepare("SELECT operation_id,subject FROM read_archives ORDER BY operation_id"))?;
    let mut rows = sql(statement.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let op = sql(row.get_ref(0))?;
        let subject = sql(row.get_ref(1))?;
        let op = op.as_str().map_err(|_| Error::Integrity)?;
        let subject = subject.as_str().map_err(|_| Error::Integrity)?;
        let manifest = read_archive::load(c, subject, op)?.ok_or(Error::Integrity)?;
        let bytes = cost(&manifest)?;
        visit(op, bytes)?;
        total.archives = total.archives.checked_add(1).ok_or(Error::Integrity)?;
        total.logical_bytes = total
            .logical_bytes
            .checked_add(bytes)
            .ok_or(Error::Integrity)?;
    }
    Ok(total)
}
pub(super) fn migrate(c: &Connection) -> Result<()> {
    // Caller has verified the complete old database in this same IMMEDIATE transaction.
    sql(c.execute_batch(SCHEMA))?;
    let total = scan(c, |op, bytes| {
        sql(c.execute(
            "INSERT INTO read_archive_costs(operation_id,logical_bytes) VALUES(?1,?2)",
            params![op, integer(bytes)?],
        ))?;
        Ok(())
    })?;
    write_totals(c, total)
}
pub(super) fn verify(c: &Connection) -> Result<()> {
    if version(c)? < 14 {
        return Ok(());
    }
    let rows: i64 = sql(c.query_row("SELECT count(*) FROM read_archive_totals", [], |r| r.get(0)))?;
    if rows != 1 {
        return Err(Error::Integrity);
    }
    let orphan:bool=sql(c.query_row("SELECT EXISTS(SELECT 1 FROM read_archive_costs l LEFT JOIN read_archives a ON a.operation_id=l.operation_id WHERE a.operation_id IS NULL)",[],|r|r.get(0)))?;
    if orphan {
        return Err(Error::Integrity);
    }
    let actual = scan(c, |op, bytes| {
        if row_cost(c, op)? != Some(bytes) {
            return Err(Error::Integrity);
        }
        Ok(())
    })?;
    if totals(c)? != actual {
        return Err(Error::Integrity);
    }
    Ok(())
}
/// Updates only accounting; the caller owns the same archive transaction. New rows
/// must already exist for the FK. Deletions remove accounting before removing archives.
pub(super) fn change(
    c: &Connection,
    old: Option<&Manifest>,
    next: Option<&Manifest>,
    budget: RetentionBudget,
) -> Result<()> {
    if !matches!(version(c)?, 14..=super::SCHEMA_VERSION) {
        return Err(Error::UnsupportedVersion);
    }
    budget.validate()?;
    let manifest = old.or(next).ok_or(Error::Integrity)?;
    let operation = &manifest.status().plan.operation_id;
    if let Some(next) = next
        && next.status().plan.operation_id != *operation
    {
        return Err(Error::Integrity);
    }
    let previous = old.map(cost).transpose()?;
    if row_cost(c, operation)? != previous {
        return Err(Error::Integrity);
    }
    let current = totals(c)?;
    let mut updated = current;
    if let Some(bytes) = previous {
        updated.archives = updated.archives.checked_sub(1).ok_or(Error::Integrity)?;
        updated.logical_bytes = updated
            .logical_bytes
            .checked_sub(bytes)
            .ok_or(Error::Integrity)?;
    }
    let replacement = next.map(cost).transpose()?;
    if let Some(bytes) = replacement {
        updated.archives = updated.archives.checked_add(1).ok_or(Error::Integrity)?;
        updated.logical_bytes = updated
            .logical_bytes
            .checked_add(bytes)
            .ok_or(Error::Integrity)?;
    }
    // Low policies and migrated over-cap libraries do not obstruct no-growth retries
    // or pending cleanup. Any growth must fit both configured (and thus hard) caps.
    if (updated.archives > current.archives || updated.logical_bytes > current.logical_bytes)
        && (updated.archives > u64::from(budget.max_archives)
            || updated.logical_bytes > budget.max_bytes)
    {
        return Err(Error::ArchiveCapacity);
    }
    match (previous, replacement) {
        (None, Some(bytes)) => {
            sql(c.execute(
                "INSERT INTO read_archive_costs(operation_id,logical_bytes) VALUES(?1,?2)",
                params![operation, integer(bytes)?],
            ))?;
        }
        (Some(_), Some(bytes)) => {
            sql(c.execute(
                "UPDATE read_archive_costs SET logical_bytes=?2 WHERE operation_id=?1",
                params![operation, integer(bytes)?],
            ))?;
        }
        (Some(_), None) => {
            sql(c.execute(
                "DELETE FROM read_archive_costs WHERE operation_id=?1",
                [operation],
            ))?;
        }
        (None, None) => return Err(Error::Integrity),
    }
    write_totals(c, updated)?;
    boundary("retention-after-accounting");
    Ok(())
}
impl Store {
    /// Current-Store admission policy, never persisted or transferable as permission.
    /// Opening another Store restores the hard policy; all Stores share the same ledger.
    pub fn set_read_archive_retention_budget(&mut self, budget: RetentionBudget) -> Result<()> {
        budget.validate()?;
        self.retention_budget = budget;
        Ok(())
    }
    /// Actual archive originals+containers, including published and migrated over-cap data.
    /// This cached usage is not an integrity attestation; Store integrity checks its originals.
    pub fn read_archive_retention_usage(&self) -> Result<RetentionUsage> {
        let tx = sql(self.connection.unchecked_transaction())?;
        match version(&tx)? {
            14..=super::SCHEMA_VERSION => totals(&tx),
            5..=13 => scan(&tx, |_, _| Ok(())),
            _ => Err(Error::UnsupportedVersion),
        }
    }
}
