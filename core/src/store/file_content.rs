//! Immutable, quota-accounted write bytes. No filesystem authority or effects.
use super::{Store, boundary, byte_room, sql};
use crate::{
    Error, Result,
    file_content::{FileContent, MAX_CONTAINER_BYTES},
    file_mutation::{Disposition, RequestRecord},
    identity,
    io_evidence::Kind,
    io_intent::{Phase, Record},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(super) const SCHEMA: &str = "CREATE TABLE file_mutation_content(operation_id TEXT PRIMARY KEY CHECK(length(CAST(operation_id AS BLOB)) BETWEEN 1 AND 256),subject TEXT NOT NULL CHECK(length(CAST(subject AS BLOB)) BETWEEN 1 AND 256),request_sha256 BLOB NOT NULL CHECK(length(request_sha256)=32),content_sha256 BLOB NOT NULL CHECK(length(content_sha256)=32),content_length INTEGER NOT NULL CHECK(content_length>=0 AND content_length<=16777216),container BLOB NOT NULL) STRICT";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let stored: Option<(String, String)> = sql(c
        .query_row(
            "SELECT type,sql FROM sqlite_schema WHERE name='file_mutation_content'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    if version(c)? < 22 {
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
pub(super) fn accounted(c: &Connection) -> Result<u64> {
    if version(c)? < 22 {
        return Ok(0);
    }
    let bytes: i64 = sql(c.query_row(
        "SELECT coalesce(sum(max(content_length,length(container))),0) FROM file_mutation_content",
        [],
        |r| r.get(0),
    ))?;
    u64::try_from(bytes).map_err(|_| Error::Integrity)
}
pub(super) fn plan(
    c: &Connection,
    subject: &str,
    operation: &str,
) -> Result<Option<(Record, RequestRecord)>> {
    let history = super::io_intent::history(c, operation)?;
    let Some(first) = history.first() else {
        return Ok(None);
    };
    if first.command().subject != subject {
        return Ok(None);
    }
    if first.command().protocol_sha256 != crate::file_mutation::schema_digest() {
        return Err(Error::Invalid("not a file mutation plan"));
    }
    super::file_mutation::verify_history(c, &history)?;
    let material = super::io_evidence::load_material(c, operation, Kind::Request, &history)?
        .ok_or(Error::Integrity)?;
    let request = RequestRecord::decode(material.payload())?;
    if request.request().disposition == Disposition::Delete {
        return Err(Error::Invalid("delete has no staged write bytes"));
    }
    Ok(Some((
        history.last().ok_or(Error::Integrity)?.clone(),
        request,
    )))
}
fn bind(content: &FileContent, record: &Record, request: &RequestRecord) -> Result<()> {
    let command = record.command();
    if content.operation_id() != command.operation_id
        || content.subject() != command.subject
        || content.request_sha256() != command.request_sha256
        || content.content().len() as u64 != request.request().content_length
        || Some(content.content_sha256()) != request.request().content_sha256
    {
        return Err(Error::OperationConflict);
    }
    Ok(())
}
fn load(c: &Connection, operation: &str) -> Result<Option<FileContent>> {
    let mut statement = sql(c.prepare("SELECT CASE WHEN length(CAST(subject AS BLOB))<=256 THEN subject ELSE NULL END,request_sha256,content_sha256,content_length,CASE WHEN length(container)<=?2 THEN container ELSE NULL END FROM file_mutation_content WHERE operation_id=?1"))?;
    let mut rows = sql(statement.query(params![operation, MAX_CONTAINER_BYTES as i64]))?;
    let Some(row) = sql(rows.next())? else {
        return Ok(None);
    };
    let container = sql(row.get_ref(4))?;
    if matches!(container, rusqlite::types::ValueRef::Null) {
        return Err(Error::Limit);
    }
    let content = FileContent::decode(container.as_blob().map_err(|_| Error::Integrity)?)?;
    let subject = sql(row.get_ref(0))?;
    let request = sql(row.get_ref(1))?;
    let digest = sql(row.get_ref(2))?;
    let length: i64 = sql(row.get(3))?;
    if content.operation_id() != operation
        || subject.as_str().map_err(|_| Error::Integrity)? != content.subject()
        || request.as_blob().map_err(|_| Error::Integrity)? != content.request_sha256()
        || digest.as_blob().map_err(|_| Error::Integrity)? != content.content_sha256()
        || length < 0
        || length as u64 != content.content().len() as u64
    {
        return Err(Error::Integrity);
    }
    Ok(Some(content))
}
/// Check exact staged bytes and their audit receipt without calling plan/history
/// validation. This is also used FROM history validation: calling load_verified
/// here would recurse through file_mutation::verify_history.
pub(super) fn require_for_dispatch(
    c: &Connection,
    record: &Record,
    request: &RequestRecord,
) -> Result<()> {
    if !(23..=super::SCHEMA_VERSION).contains(&version(c)?) {
        return Err(Error::UnsupportedVersion);
    }
    let command = record.command();
    let content = load(c, &command.operation_id)?.ok_or(Error::EvidenceUnavailable)?;
    bind(&content, record, request)?;
    let receipt = super::file_content_receipt::verify_optional(
        c,
        &command.subject,
        &command.operation_id,
        Some(&content),
    )?
    .ok_or(Error::Integrity)?;
    // All sources, including legacy migration receipts, must precede dispatch.
    // LiveStaging already enforces this in its receipt loader. Do not let a
    // legacy provenance label bypass the execution history's time ordering.
    if matches!(record.phase(), Phase::OutcomeUnknown | Phase::Observed) {
        let dispatch: Option<i64> = sql(c.query_row(
            "SELECT e.sequence FROM io_intents i JOIN operation_events e ON e.id=i.event_id WHERE i.operation_id=?1 AND i.revision=2",
            [&command.operation_id], |row| row.get(0),
        ).optional())?;
        let staged: Option<i64> = sql(c
            .query_row(
                "SELECT sequence FROM operation_events WHERE id=?1",
                [receipt.event_id()],
                |row| row.get(0),
            )
            .optional())?;
        if !matches!((staged, dispatch), (Some(s), Some(d)) if s > 0 && s < d) {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}
pub(super) fn load_verified(
    c: &Connection,
    subject: &str,
    operation: &str,
) -> Result<Option<FileContent>> {
    let Some((record, request)) = plan(c, subject, operation)? else {
        return Ok(None);
    };
    let content = load(c, operation)?;
    if let Some(content) = content.as_ref() {
        bind(content, &record, &request)?;
    }
    Ok(content)
}
pub(super) fn verify(c: &Connection) -> Result<()> {
    verify_schema(c)?;
    if version(c)? < 22 {
        return Ok(());
    }
    let mut statement = sql(c.prepare("SELECT CASE WHEN length(CAST(operation_id AS BLOB))<=256 THEN operation_id ELSE NULL END,CASE WHEN length(CAST(subject AS BLOB))<=256 THEN subject ELSE NULL END FROM file_mutation_content"))?;
    let mut rows = sql(statement.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let operation = sql(row.get_ref(0))?;
        let subject = sql(row.get_ref(1))?;
        let operation = operation.as_str().map_err(|_| Error::Integrity)?;
        let subject = subject.as_str().map_err(|_| Error::Integrity)?;
        identity(operation)?;
        identity(subject)?;
        let (record, request) = plan(c, subject, operation)?.ok_or(Error::Integrity)?;
        let content = load(c, operation)?.ok_or(Error::Integrity)?;
        bind(&content, &record, &request).map_err(|_| Error::Integrity)?;
        super::file_content_receipt::verify_optional(c, subject, operation, Some(&content))?;
    }
    Ok(())
}
impl Store {
    /// Stages immutable bytes for an existing Prepared create/replace plan.
    /// Cancellation retains admitted originals for audit. No dispatch is enabled.
    /// The closure must only check live authorization, never perform effects.
    pub fn stage_file_mutation_content_local_authorized(
        &mut self,
        content: &FileContent,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        authorize()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if !(23..=super::SCHEMA_VERSION).contains(&version(&tx)?) {
            return Err(Error::UnsupportedVersion);
        }
        let (record, request) =
            plan(&tx, content.subject(), content.operation_id())?.ok_or(Error::NotFound)?;
        if record.phase() != Phase::Prepared {
            return Err(Error::Invalid("file content staging phase"));
        }
        bind(content, &record, &request)?;
        if let Some(existing) = load(&tx, content.operation_id())? {
            bind(&existing, &record, &request)?;
            if existing.container() != content.container() {
                return Err(Error::OperationConflict);
            }
            super::file_content_receipt::verify_optional(
                &tx,
                content.subject(),
                content.operation_id(),
                Some(&existing),
            )?;
            authorize()?;
            return Ok(());
        }
        super::file_content_receipt::verify_optional(
            &tx,
            content.subject(),
            content.operation_id(),
            None,
        )?;
        let charge = content.content().len().max(content.container().len()) as u64;
        byte_room(&tx, self.budget, charge)?;
        sql(tx.execute("INSERT INTO file_mutation_content(operation_id,subject,request_sha256,content_sha256,content_length,container) VALUES(?1,?2,?3,?4,?5,?6)",
            params![content.operation_id(), content.subject(), content.request_sha256().as_slice(), content.content_sha256().as_slice(), content.content().len() as i64, content.container()]))?;
        boundary("file-content-after-bytes");
        super::file_content_receipt::append_in_tx(
            &tx,
            self.budget,
            content,
            crate::file_content_receipt::Source::LiveStaging,
        )?;
        boundary("file-content-after-receipt");
        authorize()?;
        boundary("file-content-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("file-content-after-commit");
        Ok(())
    }
    /// Host-local read with fresh authorization before access and delivery.
    /// Historical bytes are not current authority or proof of a file effect.
    pub fn file_mutation_content_local_authorized(
        &self,
        subject: &str,
        operation: &str,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Option<FileContent>> {
        identity(subject)?;
        identity(operation)?;
        authorize()?;
        let tx = sql(self.connection.unchecked_transaction())?;
        if version(&tx)? < 22 {
            authorize()?;
            return Ok(None);
        }
        let Some((record, request)) = plan(&tx, subject, operation)? else {
            authorize()?;
            return Ok(None);
        };
        let content = load(&tx, operation)?;
        if let Some(content) = content.as_ref() {
            bind(content, &record, &request)?;
        }
        super::file_content_receipt::verify_optional(&tx, subject, operation, content.as_ref())?;
        authorize()?;
        Ok(content)
    }
    /// Retained originals are charged by max(raw content bytes, container bytes).
    /// They remain charged after cancellation; this is not physical disk usage.
    pub fn file_mutation_content_usage(&self) -> Result<(u64, u64)> {
        let tx = sql(self.connection.unchecked_transaction())?;
        if version(&tx)? < 22 {
            return Ok((0, 0));
        }
        let count: i64 = sql(tx.query_row(
            "SELECT count(*) FROM file_mutation_content",
            [],
            |r| r.get(0),
        ))?;
        Ok((
            u64::try_from(count).map_err(|_| Error::Integrity)?,
            accounted(&tx)?,
        ))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::{
        file_mutation::{MutationRequest, Target},
        io_evidence::Material,
    };
    use sha2::{Digest, Sha256};
    #[test]
    fn physical_sqlite_capacity_failure_keeps_plan_without_partial_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("full-content.db");
        let mut store = Store::open(&path, Default::default()).unwrap();
        let mut random = 17u64;
        let bytes: Vec<u8> = (0..512 * 1024)
            .map(|_| {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                (random >> 33) as u8
            })
            .collect();
        let request = RequestRecord::new(MutationRequest {
            operation_id: "disk-full".into(),
            subject: "file-test".into(),
            package_sha256: [1; 32],
            approval_sha256: [2; 32],
            target: Target {
                reference: [3; 32],
                relative_path: None,
            },
            disposition: Disposition::Replace,
            expected_identity: Some([4; 32]),
            content_length: bytes.len() as u64,
            content_sha256: Some(Sha256::digest(&bytes).into()),
        })
        .unwrap();
        let record = Record::prepared(request.command().unwrap()).unwrap();
        let material = Material::encode(
            Kind::Request,
            "disk-full",
            "file-test",
            record.command().request_sha256,
            request.container(),
        )
        .unwrap();
        store
            .prepare_file_mutation_local_authorized(&record, &material, || Ok(()))
            .unwrap();
        let content = FileContent::new(
            "disk-full",
            "file-test",
            record.command().request_sha256,
            &bytes,
        )
        .unwrap();
        let before = store.pending_usage().unwrap();
        let pages: i64 = store
            .connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        store
            .connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        assert_eq!(
            store.stage_file_mutation_content_local_authorized(&content, || Ok(())),
            Err(Error::StorageFull)
        );
        assert_eq!(store.file_mutation_content_usage().unwrap(), (0, 0));
        assert_eq!(store.pending_usage().unwrap(), before);
        drop(store);
        let reopened = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(
            reopened
                .lookup_io_intent("file-test", "disk-full")
                .unwrap()
                .unwrap()
                .container(),
            record.container()
        );
        assert!(
            reopened
                .file_mutation_content_local_authorized("file-test", "disk-full", || Ok(()))
                .unwrap()
                .is_none()
        );
        reopened.integrity_check().unwrap();
    }
}
