//! Persisted host approvals in the original Store and shared logical quota.
use super::{Store, boundary, byte_room, sql};
use crate::{
    Error, Result,
    outbound_authority::{MAX_CONTAINER_BYTES, MAX_RECORDS, Record},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
/// A bounded host-local view of persisted approvals, never executable authority.
/// Contains protected credential records; UI/transport callers must redact them.
/// Deliberately no Debug, to avoid diagnostic export of stored secret metadata.
pub struct OutboundAuthorityPage {
    pub records: Vec<Record>,
    pub snapshot: [u8; 32],
    pub next: Option<[u8; 32]>,
}
pub(super) const SCHEMA: &str = "CREATE TABLE outbound_authorities(reference BLOB PRIMARY KEY CHECK(length(reference)=32),revision INTEGER NOT NULL CHECK(revision>0),kind INTEGER NOT NULL CHECK(kind IN(1,2)),subject TEXT NOT NULL,payload BLOB NOT NULL) STRICT";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
pub(super) fn accounted(c: &Connection) -> Result<u64> {
    if version(c)? < 20 {
        return Ok(0);
    }
    let bytes: i64 = sql(c.query_row(
        "SELECT coalesce(sum(length(payload)),0) FROM outbound_authorities",
        [],
        |r| r.get(0),
    ))?;
    u64::try_from(bytes).map_err(|_| Error::Integrity)
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let stored: Option<(String, String)> = sql(c
        .query_row(
            "SELECT type,sql FROM sqlite_schema WHERE name='outbound_authorities'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    if version(c)? < 20 {
        return if stored.is_none() {
            Ok(())
        } else {
            Err(Error::Integrity)
        };
    }
    if stored != Some(("table".into(), SCHEMA.into())) {
        return Err(Error::Integrity);
    }
    let mut statement = sql(c.prepare(
        "SELECT reference,revision,kind,subject,payload FROM outbound_authorities LIMIT ?1",
    ))?;
    let mut rows = sql(statement.query([MAX_RECORDS as i64 + 1]))?;
    let mut count = 0;
    while let Some(row) = sql(rows.next())? {
        count += 1;
        if count > MAX_RECORDS {
            return Err(Error::Limit);
        }
        decode_row(row)?;
    }
    Ok(())
}
fn decode_row(row: &rusqlite::Row<'_>) -> Result<Record> {
    let reference_ref = sql(row.get_ref(0))?;
    let reference = reference_ref.as_blob().map_err(|_| Error::Integrity)?;
    let revision: i64 = sql(row.get(1))?;
    let kind: i64 = sql(row.get(2))?;
    let subject_ref = sql(row.get_ref(3))?;
    let subject = subject_ref.as_str().map_err(|_| Error::Integrity)?;
    let payload_ref = sql(row.get_ref(4))?;
    let payload = payload_ref.as_blob().map_err(|_| Error::Integrity)?;
    if reference.len() != 32 || revision <= 0 || !matches!(kind, 1 | 2) || subject.len() > 256 {
        return Err(Error::Integrity);
    }
    if payload.len() > MAX_CONTAINER_BYTES {
        return Err(Error::Limit);
    }
    let record = Record::decode(payload)?;
    if record.value().reference != reference
        || record.value().revision != revision as u64
        || record.identity() != (kind, subject)
    {
        return Err(Error::Integrity);
    }
    Ok(record)
}
fn load(c: &Connection, reference: &[u8; 32]) -> Result<Option<Record>> {
    if *reference == [0; 32] {
        return Err(Error::Invalid("outbound authority reference"));
    }
    if version(c)? < 20 {
        return Ok(None);
    }
    let mut statement=sql(c.prepare("SELECT reference,revision,kind,subject,payload FROM outbound_authorities WHERE reference=?1"))?;
    let mut rows = sql(statement.query([reference.as_slice()]))?;
    sql(rows.next())?.map(decode_row).transpose()
}
impl Store {
    /// Read persisted credentials/endpoints in stable reference order. Later pages
    /// require the prior snapshot and an exact existing cursor. Any added, removed
    /// or changed row invalidates continuation. This does not decrypt, pin, revoke,
    /// renew authority, or change database state. At most 512 bounded rows are
    /// inspected and at most 16 records retained, in one SQLite read snapshot.
    pub fn list_outbound_authorities_local(
        &mut self,
        after: Option<[u8; 32]>,
        expected_snapshot: Option<[u8; 32]>,
        limit: u32,
    ) -> Result<OutboundAuthorityPage> {
        if !(1..=16).contains(&limit) {
            return Err(Error::Limit);
        }
        if after.is_some() && expected_snapshot.is_none() {
            return Err(Error::Invalid("outbound authority snapshot required"));
        }
        if after == Some([0; 32]) {
            return Err(Error::Invalid("outbound authority cursor"));
        }
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred))?;
        if !matches!(version(&tx)?, 20..=super::SCHEMA_VERSION) {
            return Err(Error::UnsupportedVersion);
        }
        // SQL avoids returning an oversized payload/metadata allocation. The
        // decoder validates types and lengths before copying any business bytes.
        let mut statement = sql(tx.prepare(
            "SELECT CASE WHEN typeof(reference)='blob' AND length(reference)=32 THEN reference ELSE NULL END,revision,kind,CASE WHEN typeof(subject)='text' AND length(CAST(subject AS BLOB))<=256 THEN subject ELSE NULL END,CASE WHEN typeof(payload)='blob' AND length(payload)<=?2 THEN payload ELSE NULL END FROM outbound_authorities ORDER BY reference LIMIT ?1",
        ))?;
        let mut rows =
            sql(statement.query(params![MAX_RECORDS as i64 + 1, MAX_CONTAINER_BYTES as i64]))?;
        let mut hash = Sha256::new();
        hash.update(b"Morrow/outbound-authority/list-snapshot/v1\0");
        let mut records = Vec::with_capacity(limit as usize);
        let mut count = 0u64;
        let mut found_cursor = after.is_none();
        let mut more = false;
        let mut previous = None;
        while let Some(row) = sql(rows.next())? {
            count += 1;
            if count > MAX_RECORDS as u64 {
                return Err(Error::Limit);
            }
            let record = decode_row(row)?;
            let reference = record.reference();
            if previous.is_some_and(|old| old >= reference) {
                return Err(Error::Integrity);
            }
            previous = Some(reference);
            hash.update(reference);
            hash.update(record.value().revision.to_le_bytes());
            hash.update(Sha256::digest(record.container()));
            if after == Some(reference) {
                found_cursor = true;
            }
            if after.is_none_or(|cursor| reference > cursor) {
                if records.len() < limit as usize {
                    records.push(record);
                } else {
                    more = true;
                }
            }
        }
        hash.update(count.to_le_bytes());
        let snapshot: [u8; 32] = hash.finalize().into();
        if expected_snapshot.is_some_and(|expected| expected != snapshot) {
            return Err(Error::RevisionConflict);
        }
        if !found_cursor {
            return Err(Error::Invalid("outbound authority cursor"));
        }
        let next = if more {
            records.last().map(Record::reference)
        } else {
            None
        };
        Ok(OutboundAuthorityPage {
            records,
            snapshot,
            next,
        })
    }
    /// Explicit host approval data only. Expected zero creates revision one.
    /// Type and provider/package identity are immutable under a reference; rotate
    /// ciphertext/approval/expiry or disable via a new revision. No deletion/reuse API.
    pub fn save_outbound_authority_local(
        &mut self,
        record: &Record,
        expected_revision: u64,
    ) -> Result<()> {
        let value = record.value();
        if expected_revision.checked_add(1) != Some(value.revision) {
            return Err(Error::RevisionConflict);
        }
        let _writer = self.service_authority_coordinator.writer()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if version(&tx)? < 20 {
            return Err(Error::UnsupportedVersion);
        }
        let previous = load(&tx, &record.reference())?;
        if previous.as_ref().map_or(0, |old| old.value().revision) != expected_revision {
            return Err(Error::RevisionConflict);
        }
        if let Some(old) = &previous {
            if old.identity() != record.identity() {
                return Err(Error::OperationConflict);
            }
        } else {
            let count: i64 = sql(tx.query_row(
                "SELECT count(*) FROM outbound_authorities",
                [],
                |r| r.get(0),
            ))?;
            if count >= MAX_RECORDS as i64 {
                return Err(Error::Limit);
            }
        }
        self.service_authority_coordinator.control().revoke_all();
        if previous.is_some() {
            sql(tx.execute(
                "DELETE FROM outbound_authorities WHERE reference=?1 AND revision=?2",
                params![value.reference, expected_revision as i64],
            ))?;
        }
        byte_room(&tx, self.budget, record.container().len() as u64)?;
        let (kind, subject) = record.identity();
        sql(tx.execute("INSERT INTO outbound_authorities(reference,revision,kind,subject,payload) VALUES(?1,?2,?3,?4,?5)",params![value.reference,value.revision as i64,kind,subject,record.container()]))?;
        // Old leases were revoked before mutation; rollback or an uncertain
        // commit must not revive them. Persistence alone grants nothing.
        boundary("outbound-authority-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("outbound-authority-after-commit");
        Ok(())
    }
    pub fn load_outbound_authority(&self, reference: &[u8; 32]) -> Result<Option<Record>> {
        load(&self.connection, reference)
    }
}
