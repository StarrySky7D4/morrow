//! Persisted host approvals in the original Store and shared logical quota.
use super::{Store, boundary, byte_room, sql};
use crate::{
    Error, Result,
    service_authority::{MAX_CONTAINER_BYTES, MAX_RECORDS, Record},
};
use prost::Message;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
pub(super) const SCHEMA: &str = "CREATE TABLE service_authorities(reference BLOB PRIMARY KEY CHECK(length(reference)=32),revision INTEGER NOT NULL CHECK(revision>0),kind INTEGER NOT NULL CHECK(kind IN(1,2)),subject TEXT NOT NULL,payload BLOB NOT NULL) STRICT";
pub(super) const IDENTITY_SCHEMA: &str = "CREATE TABLE service_authority_identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1),payload BLOB NOT NULL) STRICT";
const IDENTITY_MAGIC: &[u8; 8] = b"MROWSAI1";
pub(super) fn create_identity(c: &Connection) -> Result<()> {
    let store_id: Vec<u8> = sql(c.query_row("SELECT randomblob(32)", [], |r| r.get(0)))?;
    if store_id == [0; 32] {
        return Err(Error::Integrity);
    }
    let value = crate::service_authority::proto::StoreIdentity {
        schema_version: 1,
        store_id,
    };
    let payload = crate::envelope::pack(IDENTITY_MAGIC, &value.encode_to_vec(), 64)?;
    sql(c.execute(
        "INSERT INTO service_authority_identity(singleton,payload) VALUES(1,?1)",
        [payload],
    ))?;
    Ok(())
}
pub(super) fn store_identity(c: &Connection) -> Result<Option<[u8; 32]>> {
    if version(c)? < 19 {
        return Ok(None);
    }
    let mut statement =
        sql(c.prepare("SELECT singleton,payload FROM service_authority_identity LIMIT 2"))?;
    let mut rows = sql(statement.query([]))?;
    let row = sql(rows.next())?.ok_or(Error::Integrity)?;
    let singleton: i64 = sql(row.get(0))?;
    let payload_ref = sql(row.get_ref(1))?;
    let payload = payload_ref.as_blob().map_err(|_| Error::Integrity)?;
    if singleton != 1 || payload.len() > 192 {
        return Err(Error::Integrity);
    }
    let raw = crate::envelope::unpack(IDENTITY_MAGIC, payload, 64)?;
    // Exact canonical protobuf is fixed to these two fields. Bound before decode.
    if raw.len() != 36 || raw[..4] != [8, 1, 18, 32] {
        return Err(Error::Integrity);
    }
    let value = crate::service_authority::proto::StoreIdentity::decode(raw.as_slice())
        .map_err(|_| Error::Integrity)?;
    if value.encode_to_vec() != raw || value.schema_version != 1 {
        return Err(Error::Integrity);
    }
    let id: [u8; 32] = value
        .store_id
        .as_slice()
        .try_into()
        .map_err(|_| Error::Integrity)?;
    if id == [0; 32] || sql(rows.next())?.is_some() {
        return Err(Error::Integrity);
    }
    Ok(Some(id))
}
fn verify_identity(c: &Connection) -> Result<()> {
    let stored: Option<(String, String)> = sql(c
        .query_row(
            "SELECT type,sql FROM sqlite_schema WHERE name='service_authority_identity'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    if version(c)? < 19 {
        return if stored.is_none() {
            Ok(())
        } else {
            Err(Error::Integrity)
        };
    }
    if stored != Some(("table".into(), IDENTITY_SCHEMA.into())) {
        return Err(Error::Integrity);
    }
    store_identity(c)?;
    Ok(())
}
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
pub(super) fn accounted(c: &Connection) -> Result<u64> {
    if version(c)? < 19 {
        return Ok(0);
    }
    let bytes: i64 = sql(c.query_row(
        "SELECT coalesce(sum(length(payload)),0) FROM service_authorities",
        [],
        |r| r.get(0),
    ))?;
    u64::try_from(bytes).map_err(|_| Error::Integrity)
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    verify_identity(c)?;
    let stored: Option<(String, String)> = sql(c
        .query_row(
            "SELECT type,sql FROM sqlite_schema WHERE name='service_authorities'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    if version(c)? < 19 {
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
        "SELECT reference,revision,kind,subject,payload FROM service_authorities LIMIT ?1",
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
        return Err(Error::Invalid("service authority reference"));
    }
    if version(c)? < 19 {
        return Ok(None);
    }
    let mut statement=sql(c.prepare("SELECT reference,revision,kind,subject,payload FROM service_authorities WHERE reference=?1"))?;
    let mut rows = sql(statement.query([reference.as_slice()]))?;
    sql(rows.next())?.map(decode_row).transpose()
}
impl Store {
    /// Explicit host approval data only. Expected zero creates revision one.
    /// Type and principal/config identity are immutable under a reference; rotate
    /// verifiers/expiry or disable via a new revision. No deletion/reuse API.
    pub fn save_service_authority_local(
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
        if version(&tx)? < 19 {
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
            let count: i64 =
                sql(tx.query_row("SELECT count(*) FROM service_authorities", [], |r| r.get(0)))?;
            if count >= MAX_RECORDS as i64 {
                return Err(Error::Limit);
            }
        }
        self.service_authority_coordinator.control().revoke_all();
        if previous.is_some() {
            sql(tx.execute(
                "DELETE FROM service_authorities WHERE reference=?1 AND revision=?2",
                params![value.reference, expected_revision as i64],
            ))?;
        }
        byte_room(&tx, self.budget, record.container().len() as u64)?;
        let (kind, subject) = record.identity();
        sql(tx.execute("INSERT INTO service_authorities(reference,revision,kind,subject,payload) VALUES(?1,?2,?3,?4,?5)",params![value.reference,value.revision as i64,kind,subject,record.container()]))?;
        // Old leases were revoked before mutation; rollback or an uncertain
        // commit must not revive them. Persistence alone grants nothing.
        boundary("service-authority-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("service-authority-after-commit");
        Ok(())
    }
    pub fn load_service_authority(&self, reference: &[u8; 32]) -> Result<Option<Record>> {
        load(&self.connection, reference)
    }
}
