//! Owned native read transactions. Logical source checks are not OS file-handle attestation.
//! Frozen records are trusted-local data and never grants. No implicit current-store fallback.
#[cfg(not(target_arch = "wasm32"))]
use super::APPLICATION_ID;
use super::{Store, sql};
use crate::{Error, Result, content::CardRecord, envelope, identity, transaction};
#[cfg(not(target_arch = "wasm32"))]
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc};
#[cfg(not(target_arch = "wasm32"))]
const CENSUS_DOMAIN: &[u8] = b"Morrow/card-census/v1\0";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadPoint {
    pub database_version: u32,
    pub operation_sequence: u64,
    /// SHA256 of the complete operations.payload container, not re-encoded protobuf.
    pub operation_sha256: Option<[u8; 32]>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Census {
    pub count: u64,
    pub sha256: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct FrozenCard {
    store: Arc<()>,
    snapshot: Arc<()>,
    card: CardRecord,
    id: String,
    operation: String,
    sequence: u64,
    commit_sha256: [u8; 32],
}
impl FrozenCard {
    pub fn card(&self) -> &CardRecord {
        &self.card
    }
    pub fn original_bytes(&self) -> &[u8] {
        self.card.original_bytes()
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn revision(&self) -> u64 {
        self.card.summary().revision
    }
    pub fn latest_operation_id(&self) -> &str {
        &self.operation
    }
    pub fn latest_sequence(&self) -> u64 {
        self.sequence
    }
    pub fn latest_commit_sha256(&self) -> [u8; 32] {
        self.commit_sha256
    }
}
#[derive(Debug)]
pub struct CardPage {
    pub entries: Vec<FrozenCard>,
    pub done: bool,
}
pub struct CardReadSnapshot {
    connection: Connection,
    store: Arc<()>,
    snapshot: Arc<()>,
    point: ReadPoint,
    cursor: String,
    done: bool,
    poisoned: bool,
    closed: bool,
    count: u64,
    census: Sha256,
}
impl std::fmt::Debug for CardReadSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardReadSnapshot")
            .field("readpoint", &self.point)
            .field("done", &self.done)
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}
impl Drop for CardReadSnapshot {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.connection.execute_batch("ROLLBACK");
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn origin(connection: &Connection, supported: bool) -> Result<Option<PathBuf>> {
    if !supported {
        return Ok(None);
    }
    let name: String = sql(connection.query_row(
        "SELECT file FROM pragma_database_list WHERE name='main'",
        [],
        |r| r.get(0),
    ))?;
    if name.is_empty() {
        return Ok(None);
    }
    // Capture SQLite's actual main file, not a caller-supplied path on each query.
    Ok(Some(std::fs::canonicalize(name).map_err(|_| Error::Io)?))
}
#[cfg(target_arch = "wasm32")]
pub(super) fn origin(_: &Connection, _: bool) -> Result<Option<PathBuf>> {
    Ok(None)
}
#[cfg(not(target_arch = "wasm32"))]
fn point(connection: &Connection) -> Result<ReadPoint> {
    // Reading the permanent index fixes the SQLite snapshot, even for an empty index.
    let sequence: i64 = sql(connection.query_row(
        "SELECT coalesce(max(sequence),0) FROM operation_events",
        [],
        |r| r.get(0),
    ))?;
    let app: i64 = sql(connection.query_row("PRAGMA application_id", [], |r| r.get(0)))?;
    let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    if app != APPLICATION_ID || !(5..=14).contains(&version) {
        return Err(Error::UnsupportedVersion);
    }
    // Physical card deletion has no valid public operation: retain the reverse closure
    // even when the missing card is not the latest event and cannot be enumerated.
    let missing_card: bool = sql(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM operations o LEFT JOIN cards c ON c.id=o.card_id WHERE o.object_kind=0 AND c.id IS NULL)",
        [],
        |r| r.get(0),
    ))?;
    if missing_card {
        return Err(Error::Integrity);
    }
    let operation_sequence = u64::try_from(sequence).map_err(|_| Error::Integrity)?;
    let operation_sha256 = if sequence == 0 {
        None
    } else {
        let mut statement = sql(connection.prepare(
            "SELECT e.id,o.id,o.card_id,o.object_kind,CASE WHEN length(o.payload)<=?2 THEN o.payload ELSE NULL END FROM operation_events e JOIN operations o ON o.id=e.id WHERE e.sequence=?1",
        ))?;
        let mut rows = sql(statement.query(params![
            sequence,
            crate::read_journal::MAX_CONTAINER_BYTES as i64
        ]))?;
        let row = sql(rows.next())?.ok_or(Error::Integrity)?;
        let event_id = sql(row.get_ref(0))?;
        let event_id = event_id.as_str().map_err(|_| Error::Integrity)?;
        let operation = sql(row.get_ref(1))?;
        let operation = operation.as_str().map_err(|_| Error::Integrity)?;
        let subject = sql(row.get_ref(2))?;
        let subject = subject.as_str().map_err(|_| Error::Integrity)?;
        identity(event_id)?;
        identity(operation)?;
        identity(subject)?;
        if event_id != operation {
            return Err(Error::Integrity);
        }
        let kind: i64 = sql(row.get(3))?;
        let raw = sql(row.get_ref(4))?;
        if matches!(raw, rusqlite::types::ValueRef::Null) {
            return Err(Error::Limit);
        }
        let raw = raw.as_blob().map_err(|_| Error::Integrity)?;
        match kind {
            0 => {
                let (event, receipt) = transaction::decode_commit(raw)?;
                if receipt.operation_id != operation || receipt.card_id != subject {
                    return Err(Error::Integrity);
                }
                super::blobs::verify_event(connection, operation, &event.attachment_sha256)?;
                super::evidence::verify_event(connection, operation, &event.task_evidence_sha256)?;
            }
            1..=3 => super::records::verify_operation(connection, kind, operation, subject, raw)?,
            4 => super::read_journal::verify_operation(connection, operation, subject, raw)?,
            _ => return Err(Error::Integrity),
        }
        Some(Sha256::digest(raw).into())
    };
    Ok(ReadPoint {
        database_version: version as u32,
        operation_sequence,
        operation_sha256,
    })
}
#[cfg(not(target_arch = "wasm32"))]
fn audit_identity(connection: &Connection, version: u32) -> Result<Option<Vec<u8>>> {
    if version < 6 {
        return Ok(None);
    }
    let raw: Option<Option<Vec<u8>>> = sql(connection.query_row("SELECT CASE WHEN length(payload)<=8192 THEN payload ELSE NULL END FROM audit_identity WHERE slot=1", [], |r| r.get(0)).optional())?;
    raw.map(|v| v.ok_or(Error::Limit)).transpose()
}
impl Store {
    /// Native ordinary-WAL discovery; unsupported storage profiles explicitly reject.
    /// Source/reader logical heads are compared under pinned read transactions. A concurrent
    /// commit during acquisition may require retry. This is not adversarial file identity proof.
    pub fn open_card_snapshot(&self) -> Result<CardReadSnapshot> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = &self.snapshot_origin;
            Err(Error::Invalid("card snapshot unavailable"))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = self
                .snapshot_origin
                .as_ref()
                .ok_or(Error::Invalid("card snapshot unavailable"))?;
            let mode: String = sql(self
                .connection
                .query_row("PRAGMA journal_mode", [], |r| r.get(0)))?;
            if !mode.eq_ignore_ascii_case("wal") {
                return Err(Error::Invalid("card snapshot unavailable"));
            }
            let source = sql(self.connection.unchecked_transaction())?;
            let source_point = point(&source)?;
            super::binding::verify(&source, self.audit_trust.as_ref())?;
            let source_identity = audit_identity(&source, source_point.database_version)?;
            let connection = sql(Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ))?;
            sql(connection.busy_timeout(std::time::Duration::ZERO))?;
            sql(connection.execute_batch(
                "PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; BEGIN DEFERRED;",
            ))?;
            let reader_point = point(&connection)?;
            super::binding::verify(&connection, self.audit_trust.as_ref())?;
            if source_point != reader_point
                || source_identity != audit_identity(&connection, reader_point.database_version)?
            {
                return Err(Error::RevisionConflict);
            }
            let mut census = Sha256::new();
            census.update(CENSUS_DOMAIN);
            Ok(CardReadSnapshot {
                connection,
                store: self.snapshot_identity.clone(),
                snapshot: Arc::new(()),
                point: reader_point,
                cursor: String::new(),
                done: false,
                poisoned: false,
                closed: false,
                count: 0,
                census,
            })
        }
    }
    /// Source admission includes empty snapshots; it establishes no plugin permission.
    pub fn validate_card_snapshot(&self, snapshot: &CardReadSnapshot) -> Result<()> {
        if snapshot.poisoned
            || snapshot.closed
            || !Arc::ptr_eq(&self.snapshot_identity, &snapshot.store)
        {
            return Err(Error::Invalid("foreign or invalid card snapshot"));
        }
        Ok(())
    }
    pub(crate) fn validate_snapshot_card(
        &self,
        snapshot: &CardReadSnapshot,
        card: &FrozenCard,
    ) -> Result<()> {
        self.validate_card_snapshot(snapshot)?;
        if !Arc::ptr_eq(&snapshot.store, &card.store)
            || !Arc::ptr_eq(&snapshot.snapshot, &card.snapshot)
        {
            return Err(Error::Invalid("foreign or invalid card snapshot"));
        }
        Ok(())
    }
}
fn text(hash: &mut Sha256, value: &str) {
    hash.update((value.len() as u32).to_le_bytes());
    hash.update(value.as_bytes());
}
impl CardReadSnapshot {
    pub fn readpoint(&self) -> &ReadPoint {
        &self.point
    }
    /// Explicitly release the transaction before successful query delivery or host writes.
    /// Drop remains a best-effort error-path fallback; a close error must not be called success.
    pub fn close(mut self) -> Result<()> {
        let result = sql(self.connection.execute_batch("ROLLBACK"));
        self.closed = result.is_ok();
        result
    }

    /// The byte budget counts original Card PB bytes, not an unbounded whole-library buffer.
    /// A too-small first-card budget does not advance and may be retried with a larger budget.
    pub fn next_page(&mut self, max_cards: u32, max_bytes: usize) -> Result<CardPage> {
        if self.poisoned {
            return Err(Error::Invalid("poisoned card snapshot"));
        }
        if max_cards == 0 || max_cards > 128 || max_bytes == 0 {
            return Err(Error::Limit);
        }
        if self.done {
            return Ok(CardPage {
                entries: vec![],
                done: true,
            });
        }
        match self.page(max_cards, max_bytes) {
            Ok(Some(page)) => Ok(page),
            Ok(None) => Err(Error::Limit), // budget too small, no state changed
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }
    fn page(&mut self, max_cards: u32, max_bytes: usize) -> Result<Option<CardPage>> {
        // Keep the first page unbounded so corrupt empty IDs are examined. Later
        // pages use a separate fixed range query, preserving the primary-key seek.
        let first_page = self.count == 0;
        let query = if first_page {
            "SELECT CASE WHEN length(CAST(id AS BLOB))<=256 THEN id ELSE NULL END,CASE WHEN length(payload)<=?1 THEN payload ELSE NULL END FROM cards ORDER BY id LIMIT ?2"
        } else {
            "SELECT CASE WHEN length(CAST(id AS BLOB))<=256 THEN id ELSE NULL END,CASE WHEN length(payload)<=?1 THEN payload ELSE NULL END FROM cards WHERE id>?3 ORDER BY id LIMIT ?2"
        };
        let mut statement = sql(self.connection.prepare(query))?;
        let mut rows = if first_page {
            sql(statement.query(params![envelope::MAX_CONTAINER_BYTES as i64, max_cards]))?
        } else {
            sql(statement.query(params![
                envelope::MAX_CONTAINER_BYTES as i64,
                max_cards,
                self.cursor
            ]))?
        };
        let mut entries = Vec::new();
        let mut used = 0usize;
        while let Some(row) = sql(rows.next())? {
            let id: Option<String> = sql(row.get(0))?;
            let id = id.ok_or(Error::Limit)?;
            identity(&id)?;
            let raw = sql(row.get_ref(1))?;
            if matches!(raw, rusqlite::types::ValueRef::Null) {
                return Err(Error::Limit);
            }
            let bytes = raw.as_blob().map_err(|_| Error::Integrity)?;
            let card = envelope::decode(bytes)?;
            let summary = card.summary();
            if summary.id != id || id <= self.cursor {
                return Err(Error::Integrity);
            }
            let next_used = used
                .checked_add(card.original_bytes().len())
                .ok_or(Error::Limit)?;
            if next_used > max_bytes {
                if entries.is_empty() {
                    return Ok(None);
                }
                break;
            }
            let mut latest = sql(self.connection.prepare("SELECT o.id,e.sequence,CASE WHEN length(o.payload)<=?2 THEN o.payload ELSE NULL END FROM operations o JOIN operation_events e ON e.id=o.id WHERE o.object_kind=0 AND o.card_id=?1 ORDER BY e.sequence DESC LIMIT 1"))?;
            let mut latest_rows =
                sql(latest.query(params![id, crate::read_journal::MAX_CONTAINER_BYTES as i64]))?;
            let event = sql(latest_rows.next())?.ok_or(Error::Integrity)?;
            let operation_raw = sql(event.get_ref(0))?;
            let operation = operation_raw.as_str().map_err(|_| Error::Integrity)?;
            identity(operation)?;
            let sequence =
                u64::try_from(sql(event.get::<_, i64>(1))?).map_err(|_| Error::Integrity)?;
            if sequence == 0 || sequence > self.point.operation_sequence {
                return Err(Error::Integrity);
            }
            let raw = sql(event.get_ref(2))?;
            if matches!(raw, rusqlite::types::ValueRef::Null) {
                return Err(Error::Limit);
            }
            let raw = raw.as_blob().map_err(|_| Error::Integrity)?;
            let (_, receipt) = transaction::decode_commit(raw)?;
            let original_sha: [u8; 32] = Sha256::digest(card.original_bytes()).into();
            if receipt.operation_id != operation
                || receipt.card_id != id
                || receipt.revision != summary.revision
                || receipt.content_sha256 != original_sha
            {
                return Err(Error::Integrity);
            }
            let commit_sha256: [u8; 32] = Sha256::digest(raw).into();
            self.census.update(self.count.to_le_bytes());
            text(&mut self.census, &id);
            text(&mut self.census, &summary.type_id);
            self.census.update(summary.format_version.to_le_bytes());
            self.census.update(summary.revision.to_le_bytes());
            self.census
                .update((card.original_bytes().len() as u64).to_le_bytes());
            self.census.update(original_sha);
            text(&mut self.census, operation);
            self.census.update(sequence.to_le_bytes());
            self.census.update(commit_sha256);
            self.count = self.count.checked_add(1).ok_or(Error::Limit)?;
            self.cursor = id.clone();
            used = next_used;
            entries.push(FrozenCard {
                store: self.store.clone(),
                snapshot: self.snapshot.clone(),
                card,
                id,
                operation: operation.into(),
                sequence,
                commit_sha256,
            });
        }
        let remaining: bool = sql(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM cards WHERE id>?1)",
            [&self.cursor],
            |r| r.get(0),
        ))?;
        self.done = !remaining;
        Ok(Some(CardPage {
            entries,
            done: self.done,
        }))
    }
    /// Completeness digest becomes available only after an actual EOF has been observed.
    /// The read transaction remains pinned until this snapshot is dropped.
    pub fn finish(&self) -> Result<Census> {
        if self.poisoned || !self.done {
            return Err(Error::Invalid("incomplete card snapshot"));
        }
        let mut hash = self.census.clone();
        hash.update(b"\0end");
        hash.update(self.count.to_le_bytes());
        Ok(Census {
            count: self.count,
            sha256: hash.finalize().into(),
        })
    }
}
