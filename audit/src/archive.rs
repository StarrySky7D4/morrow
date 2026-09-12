//! Native archive adapter. Sealed bytes and event identities commit together.
//! This store never acknowledges/removes the core outbox or creates signing keys.
use crate::{
    AuditError, ChainVerifier, Checkpoint, MAX_CONTAINER_BYTES, TrustedLog, VerifiedSegment, verify,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{path::Path, time::Duration};
const APPLICATION_ID: i64 = 0x4d415544;
const SCHEMA: &str = "
CREATE TABLE segments (
  segment_index INTEGER PRIMARY KEY CHECK(segment_index > 0),
  digest BLOB NOT NULL UNIQUE CHECK(length(digest)=32),
  payload BLOB NOT NULL
) STRICT;
CREATE TABLE events (
  sequence INTEGER PRIMARY KEY CHECK(sequence > 0),
  operation_id TEXT NOT NULL UNIQUE,
  segment_index INTEGER NOT NULL REFERENCES segments(segment_index)
) STRICT;
CREATE INDEX event_segment ON events(segment_index);
CREATE TRIGGER immutable_segments_update BEFORE UPDATE ON segments BEGIN SELECT RAISE(ABORT,'sealed'); END;
CREATE TRIGGER immutable_segments_delete BEFORE DELETE ON segments BEGIN SELECT RAISE(ABORT,'sealed'); END;
CREATE TRIGGER immutable_events_update BEFORE UPDATE ON events BEGIN SELECT RAISE(ABORT,'sealed'); END;
CREATE TRIGGER immutable_events_delete BEFORE DELETE ON events BEGIN SELECT RAISE(ABORT,'sealed'); END;";
#[derive(Debug, PartialEq, Eq)]
pub enum ArchiveError {
    Audit(AuditError),
    Storage,
    Busy,
    Full,
    Integrity,
    Conflict,
    Unsupported,
    ReadOnly,
    /// Query/retry the same segment after reopening; never assume it rolled back.
    CommitUnknown,
}
impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "archive {self:?}")
    }
}
impl std::error::Error for ArchiveError {}
impl From<AuditError> for ArchiveError {
    fn from(e: AuditError) -> Self {
        Self::Audit(e)
    }
}
pub type Result<T> = std::result::Result<T, ArchiveError>;
#[derive(Debug, PartialEq, Eq)]
pub struct Receipt {
    pub index: u64,
    pub last_sequence: u64,
    pub digest: [u8; 32],
}
impl Receipt {
    fn of(v: &VerifiedSegment) -> Self {
        Self {
            index: v.segment().index,
            last_sequence: v.segment().events.last().unwrap().sequence,
            digest: v.digest(),
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum Append {
    Stored(Receipt),
    AlreadyStored(Receipt),
}
pub struct Archive {
    connection: Connection,
    trusted: TrustedLog,
    read_only: bool,
}
fn sql<T>(r: rusqlite::Result<T>) -> Result<T> {
    r.map_err(|e| match e.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            ArchiveError::Busy
        }
        Some(rusqlite::ErrorCode::DiskFull) => ArchiveError::Full,
        Some(rusqlite::ErrorCode::ConstraintViolation) => ArchiveError::Conflict,
        _ => ArchiveError::Storage,
    })
}
fn value_count(v: &VerifiedSegment) -> i64 {
    v.segment().events.len() as i64
}
fn integer(v: u64) -> Result<i64> {
    i64::try_from(v).map_err(|_| ArchiveError::Integrity)
}
fn boundary(_name: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok(_name) {
        std::process::exit(86);
    }
}
fn read(
    connection: &Connection,
    index: i64,
    trust: &TrustedLog,
) -> Result<Option<VerifiedSegment>> {
    let mut stmt =
        sql(connection.prepare("SELECT digest,payload FROM segments WHERE segment_index=?1"))?;
    let mut rows = sql(stmt.query([index]))?;
    let Some(row) = sql(rows.next())? else {
        return Ok(None);
    };
    let payload = sql(row.get_ref(1))?
        .as_blob()
        .map_err(|_| ArchiveError::Integrity)?;
    if payload.len() > MAX_CONTAINER_BYTES {
        return Err(AuditError::Limit.into());
    }
    let verified = verify(payload, trust)?;
    let digest = sql(row.get_ref(0))?
        .as_blob()
        .map_err(|_| ArchiveError::Integrity)?;
    if verified.segment().index != index as u64 || digest != verified.digest() {
        return Err(ArchiveError::Integrity);
    }
    let mut statement = sql(connection.prepare(
        "SELECT sequence,operation_id FROM events WHERE segment_index=?1 ORDER BY sequence",
    ))?;
    let mut rows = sql(statement.query([index]))?;
    for event in &verified.segment().events {
        let row = sql(rows.next())?.ok_or(ArchiveError::Integrity)?;
        if sql(row.get::<_, i64>(0))? != integer(event.sequence)?
            || sql(row.get::<_, String>(1))? != event.operation_id
        {
            return Err(ArchiveError::Integrity);
        }
    }
    if sql(rows.next())?.is_some() {
        return Err(ArchiveError::Integrity);
    }
    Ok(Some(verified))
}
impl Archive {
    /// Open an existing archive, or explicitly allow initialization of an empty DB.
    /// Trust comes from the caller, never from this archive's embedded key.
    pub fn open(path: &Path, trusted: TrustedLog, create: bool) -> Result<Self> {
        Self::open_mode(path, trusted, create, false)
    }
    pub fn open_read_only(path: &Path, trusted: TrustedLog) -> Result<Self> {
        Self::open_mode(path, trusted, false, true)
    }
    fn open_mode(path: &Path, trusted: TrustedLog, create: bool, read_only: bool) -> Result<Self> {
        if trusted.key.is_weak()
            || trusted.id.is_empty()
            || trusted.id.len() > 256
            || trusted
                .id
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
        {
            return Err(AuditError::Contract.into());
        }
        use rusqlite::OpenFlags as F;
        let mut flags = if read_only {
            F::SQLITE_OPEN_READ_ONLY
        } else {
            F::SQLITE_OPEN_READ_WRITE
        } | F::SQLITE_OPEN_NO_MUTEX;
        if create {
            flags |= F::SQLITE_OPEN_CREATE;
        }
        let mut connection = sql(Connection::open_with_flags(path, flags))?;
        sql(connection.busy_timeout(Duration::ZERO))?;
        let app: i64 = sql(connection.query_row("PRAGMA application_id", [], |r| r.get(0)))?;
        let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
        let fresh = app == 0 && version == 0 && create;
        if !fresh && (app != APPLICATION_ID || version != 1) {
            return Err(ArchiveError::Unsupported);
        }
        if fresh {
            let objects: i64 = sql(connection.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            ))?;
            if objects != 0 {
                return Err(ArchiveError::Unsupported);
            }
        }
        sql(connection.pragma_update(None, "foreign_keys", true))?;
        sql(connection.pragma_update(None, "trusted_schema", false))?;
        if !read_only {
            let mode: String =
                sql(connection.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0)))?;
            if mode != "delete" {
                return Err(ArchiveError::Unsupported);
            }
            sql(connection.pragma_update(None, "synchronous", "EXTRA"))?;
            sql(connection.pragma_update(None, "fullfsync", true))?;
        }
        if fresh {
            let tx = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
            let objects: i64 = sql(tx.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            ))?;
            if objects != 0 {
                return Err(ArchiveError::Conflict);
            }
            sql(tx.execute_batch(SCHEMA))?;
            sql(tx.pragma_update(None, "application_id", APPLICATION_ID))?;
            sql(tx.pragma_update(None, "user_version", 1))?;
            tx.commit().map_err(|_| ArchiveError::CommitUnknown)?;
        }
        let archive = Self {
            connection,
            trusted,
            read_only,
        };
        archive.check(None)?;
        Ok(archive)
    }
    /// Reverify the complete chain and every event index in one read snapshot.
    /// Empty is valid only without a checkpoint. Missing history is otherwise refused.
    pub fn check(&self, checkpoint: Option<Checkpoint>) -> Result<Option<Receipt>> {
        let tx = sql(self.connection.unchecked_transaction())?;
        let ok: String = sql(tx.query_row("PRAGMA integrity_check", [], |r| r.get(0)))?;
        if ok != "ok" {
            return Err(ArchiveError::Integrity);
        }
        let mut chain = ChainVerifier::new(self.trusted.clone(), checkpoint.clone())?;
        let mut last = None;
        let mut count = 0i64;
        let mut statement =
            sql(tx.prepare("SELECT segment_index FROM segments ORDER BY segment_index"))?;
        let mut rows = sql(statement.query([]))?;
        while let Some(row) = sql(rows.next())? {
            let index: i64 = sql(row.get(0))?;
            let verified = read(&tx, index, &self.trusted)?.ok_or(ArchiveError::Integrity)?;
            chain.accept(verified.container())?;
            count = count
                .checked_add(value_count(&verified))
                .ok_or(ArchiveError::Integrity)?;
            last = Some(Receipt::of(&verified));
        }
        let indexed: i64 = sql(tx.query_row("SELECT count(*) FROM events", [], |r| r.get(0)))?;
        let unique: i64 = sql(tx.query_row(
            "SELECT count(DISTINCT operation_id) FROM events",
            [],
            |r| r.get(0),
        ))?;
        if count != indexed || indexed != unique {
            return Err(ArchiveError::Integrity);
        }
        if last.is_some() || checkpoint.is_some() {
            chain.finish()?;
        }
        Ok(last)
    }
    /// Stored bytes are returned exactly, including unknown fields and compression.
    pub fn segment(&self, index: u64) -> Result<Option<VerifiedSegment>> {
        read(&self.connection, integer(index)?, &self.trusted)
    }
    /// Signature, continuity, stable event identities and immutable bytes are checked
    /// before commit. Retrying an already committed signed segment is idempotent.
    pub fn append(&mut self, bytes: &[u8]) -> Result<Append> {
        if self.read_only {
            return Err(ArchiveError::ReadOnly);
        }
        let verified = verify(bytes, &self.trusted)?;
        let value = verified.segment();
        let index = integer(value.index)?;
        for event in &value.events {
            integer(event.sequence)?;
        }
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if let Some(existing) = read(&tx, index, &self.trusted)? {
            return if existing.digest() == verified.digest() {
                Ok(Append::AlreadyStored(Receipt::of(&existing)))
            } else {
                Err(ArchiveError::Conflict)
            };
        }
        let tail: Option<i64> = sql(tx
            .query_row(
                "SELECT segment_index FROM segments ORDER BY segment_index DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional())?;
        let (next, sequence, previous) = match tail {
            Some(i) => {
                let v = read(&tx, i, &self.trusted)?.ok_or(ArchiveError::Integrity)?;
                (
                    v.segment().index.checked_add(1),
                    v.segment().events.last().unwrap().sequence.checked_add(1),
                    v.digest(),
                )
            }
            None => (Some(1), Some(1), [0; 32]),
        };
        if next != Some(value.index)
            || sequence != Some(value.events[0].sequence)
            || value.previous_sha256 != previous
        {
            return Err(ArchiveError::Conflict);
        }
        sql(tx.execute(
            "INSERT INTO segments(segment_index,digest,payload) VALUES(?1,?2,?3)",
            params![index, verified.digest().as_slice(), bytes],
        ))?;
        boundary("after-segment");
        for event in &value.events {
            sql(tx.execute(
                "INSERT INTO events(sequence,operation_id,segment_index) VALUES(?1,?2,?3)",
                params![integer(event.sequence)?, event.operation_id, index],
            ))?;
            boundary("after-event");
        }
        boundary("before-commit");
        tx.commit().map_err(|_| ArchiveError::CommitUnknown)?;
        boundary("after-commit");
        Ok(Append::Stored(Receipt::of(&verified)))
    }
}
