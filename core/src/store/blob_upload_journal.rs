//! Bounded, non-evicting upload history. No worker or external side effects.
use super::{Store, boundary, byte_room, sql};
use crate::{
    Error, Result,
    blob_upload_journal::{
        MAX_PAGE_SIZE, MAX_RECORD_BYTES, MAX_SUBMISSIONS, MetadataCompletion, OriginalResult,
        RESERVED_HISTORY_BYTES, Record, Request,
    },
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const REQUESTS: &str = "CREATE TABLE blob_upload_requests(submission BLOB PRIMARY KEY CHECK(length(submission)=32),revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 3)) STRICT";
const HISTORY: &str = "CREATE TABLE blob_upload_history(submission BLOB NOT NULL,revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 3),payload BLOB NOT NULL,PRIMARY KEY(submission,revision),FOREIGN KEY(submission) REFERENCES blob_upload_requests(submission)) STRICT";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
fn supported(c: &Connection) -> Result<()> {
    if version(c)? < 27 {
        Err(Error::UnsupportedVersion)
    } else {
        Ok(())
    }
}
pub(super) fn create(c: &Connection) -> Result<()> {
    sql(c.execute_batch(REQUESTS))?;
    sql(c.execute_batch(HISTORY))
}
pub(super) fn accounted(c: &Connection) -> Result<u64> {
    if version(c)? < 27 {
        return Ok(0);
    }
    let count: i64 = sql(
        c.query_row("SELECT count(*) FROM blob_upload_requests", [], |r| {
            r.get(0)
        }),
    )?;
    let count = u64::try_from(count).map_err(|_| Error::Integrity)?;
    if count > MAX_SUBMISSIONS {
        return Err(Error::Limit);
    }
    Ok(count * RESERVED_HISTORY_BYTES)
}
fn history(c: &Connection, submission: &[u8; 32]) -> Result<Option<Record>> {
    if *submission == [0; 32] {
        return Err(Error::Invalid("upload submission"));
    }
    if version(c)? < 27 {
        return Ok(None);
    }
    let head: Option<i64> = sql(c
        .query_row(
            "SELECT revision FROM blob_upload_requests WHERE submission=?1",
            [submission.as_slice()],
            |r| r.get(0),
        )
        .optional())?;
    let Some(head) = head else {
        return Ok(None);
    };
    if !(1..=3).contains(&head) {
        return Err(Error::Integrity);
    }
    let mut query = sql(c.prepare("SELECT revision,CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM blob_upload_history WHERE submission=?1 ORDER BY revision LIMIT 4"))?;
    let mut rows = sql(query.query(params![submission.as_slice(), MAX_RECORD_BYTES as i64]))?;
    let mut last: Option<Record> = None;
    let mut expected = 1;
    while let Some(row) = sql(rows.next())? {
        let revision: i64 = sql(row.get(0))?;
        let bytes = sql(row.get_ref(1))?;
        let record = Record::decode(bytes.as_blob().map_err(|_| Error::Integrity)?)?;
        if revision != expected
            || revision > head
            || record.revision() != revision as u32
            || record.request().submission != *submission
        {
            return Err(Error::Integrity);
        }
        if let Some(previous) = &last {
            record.follows(previous)?;
        }
        last = Some(record);
        expected += 1;
    }
    if expected != head + 1 {
        return Err(Error::Integrity);
    }
    Ok(Some(last.ok_or(Error::Integrity)?))
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let current = version(c)?;
    for (name, schema) in [
        ("blob_upload_requests", REQUESTS),
        ("blob_upload_history", HISTORY),
    ] {
        let row: Option<(String, String)> = sql(c
            .query_row(
                "SELECT type,sql FROM sqlite_schema WHERE name=?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional())?;
        if current < 27 {
            if row.is_some() {
                return Err(Error::Integrity);
            }
        } else if row != Some(("table".into(), schema.into())) {
            return Err(Error::Integrity);
        }
    }
    if current < 27 {
        return Ok(());
    }
    accounted(c)?;
    let mut query = sql(c.prepare("SELECT CASE WHEN length(submission)=32 THEN submission ELSE NULL END FROM blob_upload_requests LIMIT ?1"))?;
    let mut rows = sql(query.query([MAX_SUBMISSIONS as i64 + 1]))?;
    let mut count = 0;
    while let Some(row) = sql(rows.next())? {
        count += 1;
        if count > MAX_SUBMISSIONS {
            return Err(Error::Limit);
        }
        let bytes = sql(row.get_ref(0))?;
        let submission: &[u8; 32] = bytes
            .as_blob()
            .map_err(|_| Error::Integrity)?
            .try_into()
            .map_err(|_| Error::Integrity)?;
        history(c, submission)?.ok_or(Error::Integrity)?;
    }
    let orphan: bool = sql(c.query_row("SELECT EXISTS(SELECT 1 FROM blob_upload_history h LEFT JOIN blob_upload_requests r ON r.submission=h.submission WHERE r.submission IS NULL)", [], |r| r.get(0)))?;
    if orphan {
        return Err(Error::Integrity);
    }
    Ok(())
}

impl Store {
    /// Reserve one immutable trusted request before dispatch. Repeating even the
    /// exact same submission is refused, including after reopen. A commit error
    /// is uncertain and must not be followed by dispatch or an automatic retry.
    /// Capacity for both later observations is reserved now and never evicted.
    pub fn reserve_blob_upload_local_authorized(
        &mut self,
        request: &Request,
        now: i64,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        authorize()?;
        let record = Record::reserved(request.clone(), now)?;
        let bytes = record.encode()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        supported(&tx)?;
        if history(&tx, &request.submission)?.is_some() {
            return Err(Error::OperationConflict);
        }
        if accounted(&tx)? / RESERVED_HISTORY_BYTES >= MAX_SUBMISSIONS {
            return Err(Error::Limit);
        }
        byte_room(&tx, self.budget, RESERVED_HISTORY_BYTES)?;
        sql(tx.execute(
            "INSERT INTO blob_upload_requests(submission,revision) VALUES(?1,1)",
            [request.submission.as_slice()],
        ))?;
        sql(tx.execute(
            "INSERT INTO blob_upload_history(submission,revision,payload) VALUES(?1,1,?2)",
            params![request.submission.as_slice(), bytes],
        ))?;
        boundary("blob-upload-reserve-before-commit");
        authorize()?;
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("blob-upload-reserve-after-commit");
        Ok(record)
    }
    fn append_blob_upload_local_authorized(
        &mut self,
        request: &Request,
        derive: impl FnOnce(&Record) -> Result<Record>,
        point: &str,
        after: &str,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        authorize()?;
        request.validate()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        supported(&tx)?;
        let original = history(&tx, &request.submission)?.ok_or(Error::NotFound)?;
        if original.request() != request {
            return Err(Error::OperationConflict);
        }
        let next = derive(&original)?;
        next.follows(&original)?;
        sql(tx.execute(
            "INSERT INTO blob_upload_history(submission,revision,payload) VALUES(?1,?2,?3)",
            params![
                request.submission.as_slice(),
                next.revision(),
                next.encode()?
            ],
        ))?;
        let changed = sql(tx.execute(
            "UPDATE blob_upload_requests SET revision=?2 WHERE submission=?1 AND revision=?3",
            params![
                request.submission.as_slice(),
                next.revision(),
                original.revision()
            ],
        ))?;
        if changed != 1 {
            return Err(Error::Integrity);
        }
        boundary(point);
        authorize()?;
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary(after);
        Ok(next)
    }
    /// Record only the original joined owner's result. This method cannot prove
    /// that a join occurred; its trusted caller must retain that evidence and
    /// the original Store binding. Reading a saved request is not such evidence.
    pub fn observe_blob_upload_local_authorized(
        &mut self,
        request: &Request,
        result: OriginalResult,
        now: i64,
        authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        self.append_blob_upload_local_authorized(
            request,
            |r| r.observed(result, now),
            "blob-upload-observe-before-commit",
            "blob-upload-observe-after-commit",
            authorize,
        )
    }
    /// Record the original live task's explicit acknowledgement. This does not
    /// release native resources, restore scheduler capacity or rewrite Unknown.
    pub fn acknowledge_blob_upload_local_authorized(
        &mut self,
        request: &Request,
        now: i64,
        authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        self.append_blob_upload_local_authorized(
            request,
            |r| r.acknowledged(now),
            "blob-upload-ack-before-commit",
            "blob-upload-ack-after-commit",
            authorize,
        )
    }
    /// Append exactly the reviewed predecessor's missing metadata. The exact
    /// record comparison and authority checks occur inside the write transaction;
    /// a stale review, duplicate request or uncertain commit is not retried here.
    pub fn complete_blob_upload_metadata_local_authorized(
        &mut self,
        expected: &Record,
        completion: MetadataCompletion,
        authorize: impl FnMut() -> Result<()>,
    ) -> Result<Record> {
        self.append_blob_upload_local_authorized(
            expected.request(),
            |current| {
                if current != expected {
                    return Err(Error::OperationConflict);
                }
                match completion {
                    MetadataCompletion::Observation { result, at } => current.observed(result, at),
                    MetadataCompletion::Acknowledgement { at } => current.acknowledged(at),
                }
            },
            "blob-upload-complete-before-commit",
            "blob-upload-complete-after-commit",
            authorize,
        )
    }
    pub fn inspect_blob_upload_local_authorized(
        &self,
        submission: &[u8; 32],
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Option<Record>> {
        authorize()?;
        let tx = sql(self.connection.unchecked_transaction())?;
        let result = history(&tx, submission)?;
        authorize()?;
        Ok(result)
    }
    /// One bounded read snapshot. The cursor is only a sort key, never a task or
    /// capability. An empty page is not proof that an old operation did not run.
    pub fn list_blob_uploads_local_authorized(
        &self,
        after: Option<&[u8; 32]>,
        limit: u32,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Vec<Record>> {
        authorize()?;
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(Error::Limit);
        }
        let tx = sql(self.connection.unchecked_transaction())?;
        if version(&tx)? < 27 {
            authorize()?;
            return Ok(vec![]);
        }
        accounted(&tx)?;
        let mut query = sql(tx.prepare("SELECT CASE WHEN length(submission)=32 THEN submission ELSE NULL END FROM blob_upload_requests WHERE submission>?1 ORDER BY submission LIMIT ?2"))?;
        let mut rows = sql(query.query(params![after.map_or(&[][..], |v| v.as_slice()), limit]))?;
        let mut result = Vec::new();
        while let Some(row) = sql(rows.next())? {
            let bytes = sql(row.get_ref(0))?;
            let key: &[u8; 32] = bytes
                .as_blob()
                .map_err(|_| Error::Integrity)?
                .try_into()
                .map_err(|_| Error::Integrity)?;
            result.push(history(&tx, key)?.ok_or(Error::Integrity)?);
        }
        authorize()?;
        Ok(result)
    }
}
