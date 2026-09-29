//! Atomic audit commitment to exact retained file bytes, never OS authority.
use super::{Store, sql};
use crate::{
    Error, Result,
    file_content::FileContent,
    file_content_receipt::{MAX_CONTAINER_BYTES, Receipt, Source},
    identity,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};
pub(super) const SCHEMA: &str = "CREATE TABLE file_content_receipts(operation_id TEXT PRIMARY KEY CHECK(length(CAST(operation_id AS BLOB)) BETWEEN 1 AND 256),event_id TEXT NOT NULL UNIQUE REFERENCES operations(id)) STRICT";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let stored: Option<(String, String)> = sql(c
        .query_row(
            "SELECT type,sql FROM sqlite_schema WHERE name='file_content_receipts'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    if version(c)? < 23 {
        return if stored.is_none() {
            Ok(())
        } else {
            Err(Error::Integrity)
        };
    }
    if stored != Some(("table".into(), SCHEMA.into())) {
        return Err(Error::Integrity);
    }
    Ok(())
}
fn bind(receipt: &Receipt, content: &FileContent) -> Result<()> {
    let container_digest: [u8; 32] = Sha256::digest(content.container()).into();
    if receipt.operation_id() != content.operation_id()
        || receipt.subject() != content.subject()
        || receipt.request_sha256() != content.request_sha256()
        || receipt.content_sha256() != content.content_sha256()
        || receipt.content_container_sha256() != container_digest
        || receipt.content_length() != content.content().len() as u64
    {
        return Err(Error::Integrity);
    }
    Ok(())
}
fn load(c: &Connection, operation: &str) -> Result<Option<Receipt>> {
    if version(c)? < 23 {
        return Ok(None);
    }
    let mut statement=sql(c.prepare("SELECT CASE WHEN length(CAST(r.event_id AS BLOB))<=256 THEN r.event_id ELSE NULL END,CASE WHEN length(CAST(o.card_id AS BLOB))<=256 THEN o.card_id ELSE NULL END,o.object_kind,CASE WHEN length(o.payload)<=?2 THEN o.payload ELSE NULL END,e.sequence FROM file_content_receipts r LEFT JOIN operations o ON o.id=r.event_id LEFT JOIN operation_events e ON e.id=r.event_id WHERE r.operation_id=?1"))?;
    let mut rows = sql(statement.query(params![operation, MAX_CONTAINER_BYTES as i64]))?;
    let Some(row) = sql(rows.next())? else {
        return Ok(None);
    };
    let raw = sql(row.get_ref(3))?;
    if matches!(raw, rusqlite::types::ValueRef::Null) {
        return Err(Error::Integrity);
    }
    let receipt = Receipt::decode(raw.as_blob().map_err(|_| Error::Integrity)?)?;
    let id = sql(row.get_ref(0))?;
    let subject = sql(row.get_ref(1))?;
    let kind: i64 = sql(row.get(2))?;
    let sequence: i64 = sql(row.get(4))?;
    if kind != 6
        || receipt.operation_id() != operation
        || id.as_str().map_err(|_| Error::Integrity)? != receipt.event_id()
        || subject.as_str().map_err(|_| Error::Integrity)? != receipt.subject()
        || sequence <= 0
    {
        return Err(Error::Integrity);
    }
    // Point-in-time queue membership is checked before historical delivery.
    // Signature-chain trust still belongs to the audited Store open/seal checks.
    let mut queue = sql(c.prepare("SELECT e.seal_index,p.sequence,CASE WHEN length(p.payload)<=?2 THEN p.payload ELSE NULL END,EXISTS(SELECT 1 FROM sealed_segments s WHERE s.segment_index=e.seal_index) FROM operation_events e LEFT JOIN outbox p ON p.id=e.id WHERE e.id=?1"))?;
    let mut queue_rows = sql(queue.query(params![receipt.event_id(), MAX_CONTAINER_BYTES as i64]))?;
    let queued = sql(queue_rows.next())?.ok_or(Error::Integrity)?;
    let seal: Option<i64> = sql(queued.get(0))?;
    let pending_sequence: Option<i64> = sql(queued.get(1))?;
    let pending_raw = sql(queued.get_ref(2))?;
    if seal.is_some() {
        let sealed_exists: bool = sql(queued.get(3))?;
        if !sealed_exists
            || pending_sequence.is_some()
            || !matches!(pending_raw, rusqlite::types::ValueRef::Null)
        {
            return Err(Error::Integrity);
        }
    } else if pending_sequence != Some(sequence)
        || pending_raw.as_blob().map_err(|_| Error::Integrity)? != receipt.container()
    {
        return Err(Error::Integrity);
    }
    let initial:i64=sql(c.query_row("SELECT e.sequence FROM io_intents i JOIN operation_events e ON e.id=i.event_id WHERE i.operation_id=?1 AND i.revision=1",[operation],|r|r.get(0)))?;
    if sequence <= initial {
        return Err(Error::Integrity);
    }
    if receipt.source() == Source::LiveStaging {
        let cancelled:Option<i64>=sql(c.query_row("SELECT e.sequence FROM io_intents i JOIN operation_events e ON e.id=i.event_id WHERE i.operation_id=?1 AND i.revision=2",[operation],|r|r.get(0)).optional())?;
        if cancelled.is_some_and(|v| sequence >= v) {
            return Err(Error::Integrity);
        }
    }
    Ok(Some(receipt))
}
/// The caller supplies bytes already verified against the immutable plan.
pub(super) fn verify_optional(
    c: &Connection,
    subject: &str,
    operation: &str,
    content: Option<&FileContent>,
) -> Result<Option<Receipt>> {
    if version(c)? < 23 {
        return Ok(None);
    }
    let receipt = load(c, operation)?;
    match (receipt.as_ref(), content) {
        (None, None) => {}
        (Some(receipt), Some(content)) => {
            if receipt.subject() != subject {
                return Err(Error::Integrity);
            }
            bind(receipt, content)?;
        }
        _ => return Err(Error::Integrity),
    }
    Ok(receipt)
}
pub(super) fn append_in_tx(
    tx: &Transaction<'_>,
    budget: super::EventBudget,
    content: &FileContent,
    source: Source,
) -> Result<()> {
    if !(23..=super::SCHEMA_VERSION).contains(&version(tx)?) {
        return Err(Error::UnsupportedVersion);
    }
    let receipt = Receipt::new(content, source)?;
    if load(tx, content.operation_id())?.is_some() {
        return Err(Error::OperationConflict);
    }
    let event = receipt.event_id();
    super::read_capture::reject_tracked(tx, &event)?;
    let occupied:bool=sql(tx.query_row("SELECT EXISTS(SELECT 1 FROM operations WHERE id=?1) OR EXISTS(SELECT 1 FROM read_archives WHERE operation_id=?1)",[&event],|r|r.get(0)))?;
    if occupied || event == content.operation_id() {
        return Err(Error::OperationConflict);
    }
    super::event_room(tx, budget, receipt.container().len() as u64)?;
    sql(tx.execute(
        "INSERT INTO operations(id,card_id,object_kind,payload) VALUES(?1,?2,6,?3)",
        params![event, content.subject(), receipt.container()],
    ))?;
    sql(tx.execute(
        "INSERT INTO file_content_receipts(operation_id,event_id) VALUES(?1,?2)",
        params![content.operation_id(), event],
    ))?;
    sql(tx.execute(
        "INSERT INTO outbox(id,payload) VALUES(?1,?2)",
        params![event, receipt.container()],
    ))?;
    sql(tx.execute(
        "INSERT INTO operation_events(sequence,id) VALUES(last_insert_rowid(),?1)",
        [&event],
    ))?;
    Ok(())
}
pub(super) fn migrate(tx: &Transaction<'_>, budget: super::EventBudget) -> Result<()> {
    let mut statement = sql(
        tx.prepare("SELECT operation_id,subject FROM file_mutation_content ORDER BY operation_id")
    )?;
    let mut rows = sql(statement.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let operation = sql(row.get_ref(0))?;
        let operation = operation.as_str().map_err(|_| Error::Integrity)?;
        let subject = sql(row.get_ref(1))?;
        let subject = subject.as_str().map_err(|_| Error::Integrity)?;
        let content =
            super::file_content::load_verified(tx, subject, operation)?.ok_or(Error::Integrity)?;
        append_in_tx(tx, budget, &content, Source::LegacyImport)?;
    }
    Ok(())
}
pub(super) fn verify_operation(
    c: &Connection,
    event: &str,
    subject: &str,
    raw: &[u8],
) -> Result<()> {
    if version(c)? < 23 {
        return Err(Error::UnsupportedVersion);
    }
    let candidate = Receipt::decode(raw)?;
    if candidate.event_id() != event || candidate.subject() != subject {
        return Err(Error::Integrity);
    }
    let content = super::file_content::load_verified(c, subject, candidate.operation_id())?
        .ok_or(Error::Integrity)?;
    let stored = verify_optional(c, subject, candidate.operation_id(), Some(&content))?
        .ok_or(Error::Integrity)?;
    if stored.container() != raw {
        return Err(Error::Integrity);
    }
    super::blobs::verify_event(c, event, &[])?;
    super::evidence::verify_event(c, event, &[])?;
    Ok(())
}
pub(super) fn verify(c: &Connection) -> Result<()> {
    verify_schema(c)?;
    if version(c)? < 23 {
        return Ok(());
    }
    let invalid:bool=sql(c.query_row("SELECT EXISTS(SELECT 1 FROM file_content_receipts r LEFT JOIN file_mutation_content f ON f.operation_id=r.operation_id LEFT JOIN operations o ON o.id=r.event_id WHERE f.operation_id IS NULL OR o.id IS NULL OR o.object_kind!=6) OR EXISTS(SELECT 1 FROM file_mutation_content f LEFT JOIN file_content_receipts r ON r.operation_id=f.operation_id WHERE r.operation_id IS NULL) OR EXISTS(SELECT 1 FROM operations o LEFT JOIN file_content_receipts r ON r.event_id=o.id WHERE o.object_kind=6 AND r.event_id IS NULL)",[],|r|r.get(0)))?;
    if invalid {
        return Err(Error::Integrity);
    }
    Ok(())
}
impl Store {
    /// Immutable staging evidence only; even a valid receipt does not authorize IO.
    pub fn file_mutation_content_receipt_local_authorized(
        &self,
        subject: &str,
        operation: &str,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Option<Receipt>> {
        identity(subject)?;
        identity(operation)?;
        authorize()?;
        let tx = sql(self.connection.unchecked_transaction())?;
        if version(&tx)? < 23 {
            authorize()?;
            return Ok(None);
        }
        if super::file_content::plan(&tx, subject, operation)?.is_none() {
            authorize()?;
            return Ok(None);
        }
        let content = super::file_content::load_verified(&tx, subject, operation)?;
        let receipt = verify_optional(&tx, subject, operation, content.as_ref())?;
        authorize()?;
        Ok(receipt)
    }
}
