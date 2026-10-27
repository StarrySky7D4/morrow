//! Atomic staging receipts and non-evicting release tombstones in the same Store.
use super::{Store, blobs, boundary, byte_room, sql};
use crate::{
    Error, Result,
    blob_stage::{Command, MAX_OPERATIONS, MAX_RECORD_BYTES, RESERVED_HISTORY_BYTES, Record},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Weak};

/// Original in-process Store identity and lifetime, not permission or persistence proof.
/// No strong Store ownership, path, serialized identity or replacement lookup is exposed.
#[cfg(not(target_arch = "wasm32"))]
pub struct BlobStageStoreBinding {
    liveness: Weak<()>,
}
#[cfg(not(target_arch = "wasm32"))]
impl BlobStageStoreBinding {
    /// Atomic lifetime observation suitable for a final commit guard.
    pub fn check_live(&self) -> Result<()> {
        if self.liveness.upgrade().is_none() {
            Err(Error::Invalid("inactive blob Store"))
        } else {
            Ok(())
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl Store {
    pub fn blob_stage_store_binding(&self) -> BlobStageStoreBinding {
        // This private token is the Store lifetime marker, not a changes grant.
        BlobStageStoreBinding {
            liveness: Arc::downgrade(&self.changes_liveness),
        }
    }
    pub fn validate_blob_stage_store_binding(&self, binding: &BlobStageStoreBinding) -> Result<()> {
        binding.check_live()?;
        if !Weak::ptr_eq(&Arc::downgrade(&self.changes_liveness), &binding.liveness) {
            return Err(Error::Invalid("foreign blob Store"));
        }
        Ok(())
    }
}

const OPERATIONS: &str = "CREATE TABLE blob_stage_operations(domain BLOB NOT NULL CHECK(length(domain)=32),id TEXT NOT NULL,active_blob TEXT REFERENCES blobs(id),PRIMARY KEY(domain,id)) STRICT";
const HISTORY: &str = "CREATE TABLE blob_stage_history(domain BLOB NOT NULL,id TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision IN (1,2)),payload BLOB NOT NULL,PRIMARY KEY(domain,id,revision),FOREIGN KEY(domain,id) REFERENCES blob_stage_operations(domain,id)) STRICT";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
pub(super) fn create(c: &Connection) -> Result<()> {
    sql(c.execute_batch(OPERATIONS))?;
    sql(c.execute_batch(HISTORY))
}
pub(super) fn accounted(c: &Connection) -> Result<u64> {
    if version(c)? < 26 {
        return Ok(0);
    }
    let n: i64 = sql(
        c.query_row("SELECT count(*) FROM blob_stage_operations", [], |r| {
            r.get(0)
        }),
    )?;
    let n = u64::try_from(n).map_err(|_| Error::Integrity)?;
    if n > MAX_OPERATIONS {
        return Err(Error::Limit);
    }
    Ok(n * RESERVED_HISTORY_BYTES)
}
pub(super) fn retains(c: &Connection, blob: &str) -> Result<bool> {
    if version(c)? < 26 {
        return Ok(false);
    }
    sql(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM blob_stage_operations WHERE active_blob=?1)",
        [blob],
        |r| r.get(0),
    ))
}
pub(super) fn reserve(c: &Connection, budget: super::EventBudget) -> Result<()> {
    if version(c)? < 26 {
        return Err(Error::UnsupportedVersion);
    }
    if accounted(c)? / RESERVED_HISTORY_BYTES >= MAX_OPERATIONS {
        return Err(Error::Limit);
    }
    // Both immutable revisions are charged before the reader is consumed.
    // Release never depends on future availability in the shared logical quota.
    byte_room(c, budget, RESERVED_HISTORY_BYTES)
}
fn history(c: &Connection, domain: &[u8; 32], id: &str) -> Result<Option<Record>> {
    if version(c)? < 26 {
        return Ok(None);
    }
    let active:Option<Option<String>>=sql(c.query_row(
        "SELECT CASE WHEN active_blob IS NULL OR length(CAST(active_blob AS BLOB))<=256 THEN active_blob ELSE '' END FROM blob_stage_operations WHERE domain=?1 AND id=?2",
        params![domain.as_slice(),id],|r|r.get(0)).optional())?;
    let Some(active) = active else {
        return Ok(None);
    };
    let mut q=sql(c.prepare("SELECT revision,CASE WHEN length(payload)<=?3 THEN payload ELSE NULL END FROM blob_stage_history WHERE domain=?1 AND id=?2 ORDER BY revision LIMIT 3"))?;
    let mut rows = sql(q.query(params![domain.as_slice(), id, MAX_RECORD_BYTES as i64]))?;
    let mut first: Option<Record> = None;
    let mut final_record = None;
    let mut expected = 1;
    while let Some(row) = sql(rows.next())? {
        let revision: i64 = sql(row.get(0))?;
        let value = sql(row.get_ref(1))?;
        let record = Record::decode(value.as_blob().map_err(|_| Error::Integrity)?)?;
        if revision != expected
            || expected > 2
            || record.command().owner_domain != *domain
            || record.command().operation_id != id
            || record.is_released() != (revision == 2)
        {
            return Err(Error::Integrity);
        }
        if let Some(first) = &first {
            if first.command() != record.command()
                || first.blob() != record.blob()
                || first.staged_at_unix_ms() != record.staged_at_unix_ms()
            {
                return Err(Error::Integrity);
            }
        } else {
            first = Some(record.clone())
        }
        final_record = Some(record);
        expected += 1;
    }
    let record = final_record.ok_or(Error::Integrity)?;
    if record.is_released() {
        if active.is_some() {
            return Err(Error::Integrity);
        }
    } else {
        if active.as_deref() != Some(record.blob().id.as_str()) {
            return Err(Error::Integrity);
        }
        blobs::verify_staged(c, record.blob())?;
    }
    Ok(Some(record))
}
pub(super) fn matching(c: &Connection, command: &Command) -> Result<Option<Record>> {
    command.validate()?;
    let record = history(c, &command.owner_domain, &command.operation_id)?;
    if record.as_ref().is_some_and(|r| r.command() != command) {
        return Err(Error::OperationConflict);
    }
    Ok(record)
}
pub(super) fn staged(
    c: &Connection,
    command: &Command,
    blob: crate::attachment::BlobInfo,
    now: i64,
) -> Result<()> {
    let record = Record::staged(command.clone(), blob, now)?;
    sql(c.execute(
        "INSERT INTO blob_stage_operations(domain,id,active_blob) VALUES(?1,?2,?3)",
        params![
            command.owner_domain.as_slice(),
            command.operation_id,
            record.blob().id
        ],
    ))?;
    sql(c.execute(
        "INSERT INTO blob_stage_history(domain,id,revision,payload) VALUES(?1,?2,1,?3)",
        params![
            command.owner_domain.as_slice(),
            command.operation_id,
            record.encode()?
        ],
    ))?;
    Ok(())
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let current = version(c)?;
    for (name, schema) in [
        ("blob_stage_operations", OPERATIONS),
        ("blob_stage_history", HISTORY),
    ] {
        let row: Option<(String, String)> = sql(c
            .query_row(
                "SELECT type,sql FROM sqlite_schema WHERE name=?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional())?;
        if current < 26 {
            if row.is_some() {
                return Err(Error::Integrity);
            }
        } else if row != Some(("table".into(), schema.into())) {
            return Err(Error::Integrity);
        }
    }
    if current < 26 {
        return Ok(());
    }
    accounted(c)?;
    let mut q=sql(c.prepare("SELECT CASE WHEN length(domain)=32 THEN domain ELSE NULL END,CASE WHEN length(CAST(id AS BLOB))<=256 THEN id ELSE NULL END FROM blob_stage_operations LIMIT ?1"))?;
    let mut rows = sql(q.query([MAX_OPERATIONS as i64 + 1]))?;
    let mut count = 0;
    while let Some(row) = sql(rows.next())? {
        count += 1;
        if count > MAX_OPERATIONS {
            return Err(Error::Limit);
        }
        let d = sql(row.get_ref(0))?;
        let domain: &[u8; 32] = d
            .as_blob()
            .map_err(|_| Error::Integrity)?
            .try_into()
            .map_err(|_| Error::Integrity)?;
        let i = sql(row.get_ref(1))?;
        let id = i.as_str().map_err(|_| Error::Integrity)?;
        history(c, domain, id)?.ok_or(Error::Integrity)?;
    }
    let orphan:bool=sql(c.query_row("SELECT EXISTS(SELECT 1 FROM blob_stage_history h LEFT JOIN blob_stage_operations o ON o.domain=h.domain AND o.id=h.id WHERE o.id IS NULL)",[],|r|r.get(0)))?;
    if orphan {
        return Err(Error::Integrity);
    }
    Ok(())
}

impl Store {
    /// Original operation inspection only; no reader, dispatch or fresh authority.
    pub fn inspect_blob_stage_local(&self, command: &Command) -> Result<Option<Record>> {
        self.inspect_blob_stage_local_authorized(command, || Ok(()))
    }
    /// The caller retains original authority. This bounded, effect-free callback
    /// may veto before access and after verification; it must not re-enter Store.
    pub fn inspect_blob_stage_local_authorized(
        &self,
        command: &Command,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Option<Record>> {
        authorize()?;
        let tx = sql(self.connection.unchecked_transaction())?;
        let result = matching(&tx, command)?;
        authorize()?;
        Ok(result)
    }
    /// Atomically release this operation's retention and append its terminal fact.
    /// Exact repeated release returns the original fact without mutating its clock.
    pub fn release_blob_stage_local(&mut self, command: &Command, now: i64) -> Result<Record> {
        self.release_blob_stage_local_authorized(command, now, || Ok(()))
    }
    /// Veto before access and before the atomic release/history commit. The
    /// callback has no authority of its own and must not re-enter Store or have
    /// effects. Post-commit delivery checks remain the original owner's duty.
    pub fn release_blob_stage_local_authorized(
        &mut self,
        command: &Command,
        now: i64,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        authorize()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        let original = matching(&tx, command)?.ok_or(Error::NotFound)?;
        if original.is_released() {
            authorize()?;
            return Ok(original);
        }
        let next = original.released(now)?;
        sql(tx.execute(
            "INSERT INTO blob_stage_history(domain,id,revision,payload) VALUES(?1,?2,2,?3)",
            params![
                command.owner_domain.as_slice(),
                command.operation_id,
                next.encode()?
            ],
        ))?;
        let changed=sql(tx.execute("UPDATE blob_stage_operations SET active_blob=NULL WHERE domain=?1 AND id=?2 AND active_blob=?3",params![command.owner_domain.as_slice(),command.operation_id,original.blob().id]))?;
        if changed != 1 {
            return Err(Error::Integrity);
        }
        blobs::retire_stage_if_unreferenced(&tx, &original.blob().id, now)?;
        boundary("blob-stage-release-before-commit");
        authorize()?;
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("blob-stage-release-after-commit");
        Ok(next)
    }
}
