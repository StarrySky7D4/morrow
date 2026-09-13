//! Immutable historical task originals and ordered references inside the content transaction.
use super::{Store, sql};
use crate::{
    Error, Result, identity,
    task_evidence::{self, Evidence},
    transaction,
};
use rusqlite::{Connection, OptionalExtension, params};
pub(super) const SCHEMA: &str = "
CREATE TABLE task_evidence (digest BLOB PRIMARY KEY CHECK(length(digest)=32), payload BLOB NOT NULL) STRICT;
CREATE TABLE operation_evidence (operation_id TEXT NOT NULL REFERENCES operations(id), ordinal INTEGER NOT NULL CHECK(ordinal>=0 AND ordinal<16), digest BLOB NOT NULL REFERENCES task_evidence(digest), PRIMARY KEY(operation_id,ordinal)) STRICT;
CREATE INDEX operation_evidence_digest ON operation_evidence(digest);";
const MAX_COUNT: usize = 16;
// Counts each original and its container, including repeated positions, before any write.
const MAX_OPERATION_BYTES: usize = 64 * 1024 * 1024;
fn add_size(total: &mut usize, evidence: &Evidence) -> Result<()> {
    *total = total
        .checked_add(evidence.raw().len())
        .and_then(|n| n.checked_add(evidence.container().len()))
        .ok_or(Error::Limit)?;
    if *total > MAX_OPERATION_BYTES {
        return Err(Error::Limit);
    }
    Ok(())
}
pub(super) fn digests(evidence: &[Evidence]) -> Result<Vec<[u8; 32]>> {
    if evidence.len() > MAX_COUNT {
        return Err(Error::Limit);
    }
    let mut total = 0;
    evidence
        .iter()
        .map(|e| {
            add_size(&mut total, e)?;
            Ok(e.digest())
        })
        .collect()
}
fn payload(connection: &Connection, digest: [u8; 32]) -> Result<Option<Evidence>> {
    let mut statement =
        sql(connection.prepare("SELECT CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM task_evidence WHERE digest=?1"))?;
    let raw = sql(statement
        .query_row(
            params![digest.as_slice(), task_evidence::MAX_CONTAINER_BYTES as i64],
            |row| {
                let value = row.get_ref(0)?;
                if matches!(value, rusqlite::types::ValueRef::Null) {
                    return Ok(None);
                }
                let bytes = value.as_blob()?;
                Ok(if bytes.len() > task_evidence::MAX_CONTAINER_BYTES {
                    None
                } else {
                    Some(bytes.to_vec())
                })
            },
        )
        .optional())?;
    match raw {
        None => Ok(None),
        Some(None) => Err(Error::Limit),
        Some(Some(raw)) => task_evidence::decode(&raw, digest).map(Some),
    }
}
pub(super) fn bind(connection: &Connection, operation: &str, evidence: &[Evidence]) -> Result<()> {
    digests(evidence)?;
    let mut stored_total = 0;
    for (ordinal, item) in evidence.iter().enumerate() {
        match payload(connection, item.digest())? {
            Some(previous) if previous.raw() != item.raw() => return Err(Error::Integrity),
            Some(previous) => {
                let referenced: bool = sql(connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM operation_evidence WHERE digest=?1)",
                    [item.digest().as_slice()],
                    |r| r.get(0),
                ))?;
                if !referenced {
                    return Err(Error::Integrity);
                }
                add_size(&mut stored_total, &previous)?;
            }
            None => {
                add_size(&mut stored_total, item)?;
                sql(connection.execute(
                    "INSERT INTO task_evidence(digest,payload) VALUES(?1,?2)",
                    params![item.digest().as_slice(), item.container()],
                ))?;
            }
        }
        sql(connection.execute(
            "INSERT INTO operation_evidence(operation_id,ordinal,digest) VALUES(?1,?2,?3)",
            params![operation, ordinal as i64, item.digest().as_slice()],
        ))?;
    }
    Ok(())
}
fn read_refs(
    connection: &Connection,
    operation: &str,
    expected: &[Vec<u8>],
) -> Result<Vec<Evidence>> {
    if expected.len() > MAX_COUNT {
        return Err(Error::Limit);
    }
    let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    if version < 7 {
        return if expected.is_empty() {
            Ok(vec![])
        } else {
            Err(Error::Integrity)
        };
    }
    let mut statement = sql(connection.prepare(
        "SELECT ordinal,digest FROM operation_evidence WHERE operation_id=?1 ORDER BY ordinal",
    ))?;
    let mut rows = sql(statement.query([operation]))?;
    let mut result = Vec::with_capacity(expected.len());
    let mut total = 0;
    while let Some(row) = sql(rows.next())? {
        let ordinal: i64 = sql(row.get(0))?;
        let digest = sql(row.get_ref(1))?
            .as_blob()
            .map_err(|_| Error::Integrity)?;
        if result.len() >= expected.len()
            || ordinal != result.len() as i64
            || digest != expected[result.len()]
        {
            return Err(Error::Integrity);
        }
        let digest: [u8; 32] = digest.try_into().map_err(|_| Error::Integrity)?;
        let evidence = payload(connection, digest)?.ok_or(Error::Integrity)?;
        add_size(&mut total, &evidence)?;
        result.push(evidence);
    }
    if result.len() != expected.len() {
        return Err(Error::Integrity);
    }
    Ok(result)
}
pub(super) fn verify_event(
    connection: &Connection,
    operation: &str,
    expected: &[Vec<u8>],
) -> Result<()> {
    read_refs(connection, operation, expected)?;
    Ok(())
}
pub(super) fn verify_retry(
    connection: &Connection,
    operation: &str,
    expected: &[Vec<u8>],
    input: &[Evidence],
) -> Result<()> {
    let stored = read_refs(connection, operation, expected)?;
    if stored.len() != input.len() {
        return Err(Error::Integrity);
    }
    for (stored, input) in stored.iter().zip(input) {
        if stored.raw() != input.raw() {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}
/// Legacy schemas must not smuggle pre-created evidence tables through migration.
pub(super) fn verify_schema(connection: &Connection) -> Result<()> {
    let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    let count:i64=sql(connection.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('task_evidence','operation_evidence','operation_evidence_digest')",[],|r|r.get(0)))?;
    if version < 7 && count != 0 || version >= 7 && count != 3 {
        return Err(Error::Integrity);
    }
    Ok(())
}
pub(super) fn verify(connection: &Connection) -> Result<()> {
    let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    if version < 7 {
        return Ok(());
    }
    let invalid:i64=sql(connection.query_row("SELECT count(*) FROM operation_evidence e LEFT JOIN operations o ON e.operation_id=o.id LEFT JOIN task_evidence t ON e.digest=t.digest WHERE o.id IS NULL OR o.object_kind!=0 OR t.digest IS NULL",[],|r|r.get(0)))?;
    if invalid != 0 {
        return Err(Error::Integrity);
    }
    let mut statement=sql(connection.prepare("SELECT digest,CASE WHEN length(payload)<=?1 THEN payload ELSE NULL END,EXISTS(SELECT 1 FROM operation_evidence e WHERE e.digest=t.digest) FROM task_evidence t"))?;
    let mut rows = sql(statement.query([task_evidence::MAX_CONTAINER_BYTES as i64]))?;
    while let Some(row) = sql(rows.next())? {
        if !sql(row.get::<_, bool>(2))? {
            return Err(Error::Integrity);
        }
        let digest = sql(row.get_ref(0))?
            .as_blob()
            .map_err(|_| Error::Integrity)?;
        let digest: [u8; 32] = digest.try_into().map_err(|_| Error::Integrity)?;
        let value = sql(row.get_ref(1))?;
        if matches!(value, rusqlite::types::ValueRef::Null) {
            return Err(Error::Limit);
        }
        let bytes = value.as_blob().map_err(|_| Error::Integrity)?;
        // decode checks bounds before decompression or owned allocation.
        task_evidence::decode(bytes, digest)?;
    }
    Ok(())
}
impl Store {
    /// Host-local historical evidence. This does not grant plugins access or replay authority.
    /// Missing operations, a different card and non-content operations all return NotFound.
    pub fn operation_evidence(&self, card_id: &str, operation_id: &str) -> Result<Vec<Evidence>> {
        identity(card_id)?;
        identity(operation_id)?;
        let snapshot = sql(self.connection.unchecked_transaction())?;
        let mut statement = sql(snapshot.prepare(
            "SELECT CASE WHEN length(payload)<=?3 THEN payload ELSE NULL END FROM operations WHERE id=?1 AND card_id=?2 AND object_kind=0",
        ))?;
        let raw = sql(statement
            .query_row(params![operation_id, card_id, (transaction::MAX_EVENT_BYTES + transaction::MAX_EVENT_BYTES / 255 + 128) as i64], |row| {
                let value = row.get_ref(0)?;
                if matches!(value, rusqlite::types::ValueRef::Null) {
                    return Ok(None);
                }
                let bytes = value.as_blob()?;
                Ok(
                    if bytes.len()
                        > transaction::MAX_EVENT_BYTES + transaction::MAX_EVENT_BYTES / 255 + 128
                    {
                        None
                    } else {
                        Some(bytes.to_vec())
                    },
                )
            })
            .optional())?
        .ok_or(Error::NotFound)?
        .ok_or(Error::Limit)?;
        let (commit, receipt) = transaction::decode_commit(&raw)?;
        if receipt.card_id != card_id || receipt.operation_id != operation_id {
            return Err(Error::Integrity);
        }
        read_refs(&snapshot, operation_id, &commit.task_evidence_sha256)
    }
}
