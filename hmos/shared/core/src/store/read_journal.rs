//! Non-content read observations share the transaction, evidence store and signed outbox.
use super::{Store, boundary, sql};
use crate::{
    Error, Result, identity,
    read_journal::{self, Input, ReadObservation, Receipt},
    task_evidence::Evidence,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
const KIND: i64 = 4;
fn version(connection: &Connection) -> Result<i64> {
    sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
fn read(
    connection: &Connection,
    subject: &str,
    operation: &str,
) -> Result<Option<ReadObservation>> {
    let mut statement = sql(connection.prepare("SELECT CASE WHEN length(payload)<=?3 THEN payload ELSE NULL END FROM operations WHERE id=?1 AND card_id=?2 AND object_kind=4"))?;
    let raw: Option<Option<Vec<u8>>> = sql(statement
        .query_row(
            params![operation, subject, read_journal::MAX_CONTAINER_BYTES as i64],
            |r| r.get(0),
        )
        .optional())?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    if version(connection)? < 11 {
        return Err(Error::UnsupportedVersion);
    }
    let observed = read_journal::decode(&raw.ok_or(Error::Limit)?)?;
    verify_observation(connection, operation, subject, &observed)?;
    Ok(Some(observed))
}
fn verify_observation(
    connection: &Connection,
    operation: &str,
    subject: &str,
    observed: &ReadObservation,
) -> Result<()> {
    if observed.data().operation_id != operation || observed.data().subject != subject {
        return Err(Error::Integrity);
    }
    super::read_archive::verify_observation(connection, observed)?;
    super::blobs::verify_event(connection, operation, &[])?;
    super::evidence::verify_event(connection, operation, &observed.data().task_evidence_sha256)
}
pub(super) fn verify_operation(
    connection: &Connection,
    operation: &str,
    subject: &str,
    raw: &[u8],
) -> Result<()> {
    if version(connection)? < 11 {
        return Err(Error::UnsupportedVersion);
    }
    let observed = read_journal::decode(raw)?;
    verify_observation(connection, operation, subject, &observed)
}
pub(super) fn verify_operation_for_open(
    connection: &Connection,
    verified: &super::open_verification::OpenVerification<'_>,
    operation: &str,
    subject: &str,
    raw: &[u8],
) -> Result<()> {
    if version(connection)? < 11 {
        return Err(Error::UnsupportedVersion);
    }
    let observed = read_journal::decode(raw)?;
    if observed.data().operation_id != operation || observed.data().subject != subject {
        return Err(Error::Integrity);
    }
    verified.verify_observation(&observed)?;
    super::blobs::verify_event(connection, operation, &[])?;
    verified.verify_event(operation, &observed.data().task_evidence_sha256)
}
impl Store {
    /// Trusted-local completed computation facts. The final callback must enforce the
    /// host's current authorization; this API does not establish plugin grants or delivery.
    /// The callback executes once after all preparation and again on each explicit retry.
    pub fn record_read_local_authorized(
        &mut self,
        input: &Input,
        evidence: &[Evidence],
        authorize: impl FnOnce() -> Result<()>,
    ) -> Result<Receipt> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        boundary("read-after-begin");
        super::read_capture::reject_tracked(&tx, &input.operation_id)?;
        if !matches!(version(&tx)?, 12..=super::SCHEMA_VERSION) {
            return Err(Error::UnsupportedVersion);
        }
        let digests = super::evidence::digests(evidence)?;
        let observed = read_journal::encode(input, &digests)?;
        let metadata: Option<(String, i64)> = sql(tx
            .query_row(
                "SELECT card_id,object_kind FROM operations WHERE id=?1",
                [&input.operation_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional())?;
        if let Some((subject, kind)) = metadata {
            if kind != KIND || subject != input.subject {
                return Err(Error::OperationConflict);
            }
            let previous =
                read(&tx, &input.subject, &input.operation_id)?.ok_or(Error::Integrity)?;
            if previous.raw() != observed.raw() {
                return Err(Error::OperationConflict);
            }
            super::evidence::verify_retry(
                &tx,
                &input.operation_id,
                &previous.data().task_evidence_sha256,
                evidence,
            )?;
            authorize()?;
            return Ok(previous.receipt());
        }
        super::event_room(&tx, self.budget, observed.container().len() as u64)?;
        sql(tx.execute(
            "INSERT INTO operations(id,card_id,object_kind,payload) VALUES(?1,?2,4,?3)",
            params![input.operation_id, input.subject, observed.container()],
        ))?;
        boundary("read-after-operation");
        super::evidence::bind(&tx, &input.operation_id, evidence)?;
        boundary("read-after-task-evidence");
        sql(tx.execute(
            "INSERT INTO outbox(id,payload) VALUES(?1,?2)",
            params![input.operation_id, observed.container()],
        ))?;
        sql(tx.execute(
            "INSERT INTO operation_events(sequence,id) VALUES(last_insert_rowid(),?1)",
            [&input.operation_id],
        ))?;
        boundary("read-after-event");
        authorize()?;
        boundary("read-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("read-after-commit");
        Ok(observed.receipt())
    }
    /// One transaction snapshot. Missing, different-subject and non-read operations are absent.
    pub fn lookup_read(&self, subject: &str, operation: &str) -> Result<Option<ReadObservation>> {
        identity(subject)?;
        identity(operation)?;
        let tx = sql(self.connection.unchecked_transaction())?;
        read(&tx, subject, operation)
    }
    /// Ordered immutable originals linked by this read observation, never recovered authority.
    pub fn read_evidence(&self, subject: &str, operation: &str) -> Result<Option<Vec<Evidence>>> {
        identity(subject)?;
        identity(operation)?;
        let tx = sql(self.connection.unchecked_transaction())?;
        let Some(observed) = read(&tx, subject, operation)? else {
            return Ok(None);
        };
        Ok(Some(super::evidence::read_refs(
            &tx,
            operation,
            &observed.data().task_evidence_sha256,
        )?))
    }
}
