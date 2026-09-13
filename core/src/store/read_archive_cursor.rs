//! Published archive readers pin a native SQLite read transaction. The complete chain
//! is verified before the first part is released. The owner token and logical source
//! comparison are not plugin grants or operating-system file identity attestation.
#[cfg(not(target_arch = "wasm32"))]
use super::{APPLICATION_ID, read_archive};
use super::{Store, sql};
use crate::{
    Error, Result,
    read_archive::{MAX_CONTAINER_BYTES, Manifest, Part},
};
#[cfg(not(target_arch = "wasm32"))]
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};
use std::sync::Arc;

/// Per-page original-plus-container limit; the archive's overall capacity is unchanged.
pub const MAX_ARCHIVE_PAGE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub struct ArchivePage {
    pub parts: Vec<Part>,
    pub done: bool,
}
/// Opaque verified, immutable database view. Closing is separate from consuming EOF.
pub struct ReadArchiveCursor {
    connection: Connection,
    owner: Arc<()>,
    manifest: Manifest,
    operation: String,
    count: u32,
    logical_bytes: u64,
    expected_chain: [u8; 32],
    next: u32,
    consumed: u64,
    previous: [u8; 32],
    done: bool,
    poisoned: bool,
    closed: bool,
}
impl std::fmt::Debug for ReadArchiveCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadArchiveCursor")
            .field("operation", &self.operation)
            .field("next", &self.next)
            .field("count", &self.count)
            .field("done", &self.done)
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}
impl Drop for ReadArchiveCursor {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.connection.execute_batch("ROLLBACK");
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn identity(connection: &Connection) -> Result<Option<Vec<u8>>> {
    let app: i64 = sql(connection.query_row("PRAGMA application_id", [], |r| r.get(0)))?;
    if app != APPLICATION_ID {
        return Err(Error::UnsupportedVersion);
    }
    let raw:Option<Option<Vec<u8>>>=sql(connection.query_row(
        "SELECT CASE WHEN length(payload)<=8192 THEN payload ELSE NULL END FROM audit_identity WHERE slot=1",[],|r|r.get(0)).optional())?;
    raw.map(|v| v.ok_or(Error::Limit)).transpose()
}
impl Store {
    /// Ordinary native WAL only. A captured Store path and pinned manifest/audit identity
    /// tie the reader to this logical source; unsupported profiles never read the latest
    /// primary connection as a fallback. Publication is not evidence of user delivery.
    pub fn open_read_archive_cursor(
        &self,
        subject: &str,
        operation: &str,
        expected_root: [u8; 32],
    ) -> Result<ReadArchiveCursor> {
        crate::identity(subject)?;
        crate::identity(operation)?;
        #[cfg(target_arch = "wasm32")]
        {
            let _ = expected_root;
            Err(Error::Invalid("read archive cursor unavailable"))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = self
                .snapshot_origin
                .as_ref()
                .ok_or(Error::Invalid("read archive cursor unavailable"))?;
            let mode: String = sql(self
                .connection
                .query_row("PRAGMA journal_mode", [], |r| r.get(0)))?;
            if !mode.eq_ignore_ascii_case("wal") {
                return Err(Error::Invalid("read archive cursor unavailable"));
            }
            let source = sql(self.connection.unchecked_transaction())?;
            let source_manifest =
                read_archive::load(&source, subject, operation)?.ok_or(Error::NotFound)?;
            if source_manifest.status().root != Some(expected_root) {
                return Err(Error::Integrity);
            }
            super::binding::verify(&source, self.audit_trust.as_ref())?;
            let source_identity = identity(&source)?;
            let connection = sql(Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ))?;
            sql(connection.busy_timeout(std::time::Duration::ZERO))?;
            sql(connection.execute_batch(
                "PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; BEGIN DEFERRED;",
            ))?;
            let manifest =
                read_archive::load(&connection, subject, operation)?.ok_or(Error::NotFound)?;
            if manifest.status().root != Some(expected_root)
                || manifest.raw() != source_manifest.raw()
            {
                return Err(Error::RevisionConflict);
            }
            super::binding::verify(&connection, self.audit_trust.as_ref())?;
            if identity(&connection)? != source_identity {
                return Err(Error::RevisionConflict);
            }
            // Full verification stays in the very transaction that all pages will use.
            read_archive::verify_parts(&connection, &manifest)?;
            read_archive::verify_association(&connection, &manifest)?;
            sql(source.rollback())?;
            let state = manifest.status();
            Ok(ReadArchiveCursor {
                connection,
                owner: self.snapshot_identity.clone(),
                operation: operation.into(),
                count: state.count,
                logical_bytes: state.logical_bytes,
                expected_chain: state.chain_sha256,
                previous: manifest.initial_chain(),
                manifest,
                next: 0,
                consumed: 0,
                done: false,
                poisoned: false,
                closed: false,
            })
        }
    }
    /// Owner admission includes empty archives and establishes no runtime permission.
    pub fn validate_read_archive_cursor(&self, cursor: &ReadArchiveCursor) -> Result<()> {
        if cursor.poisoned || cursor.closed || !Arc::ptr_eq(&self.snapshot_identity, &cursor.owner)
        {
            return Err(Error::Invalid("foreign or invalid read archive cursor"));
        }
        Ok(())
    }
}
impl ReadArchiveCursor {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    /// Costs are original Part protobuf bytes plus the complete compressed container.
    /// Too small a first-part budget returns Limit without advancing, allowing retry.
    pub fn next_page(&mut self, max_parts: u32, max_bytes: usize) -> Result<ArchivePage> {
        if self.poisoned || self.closed {
            return Err(Error::Invalid("invalid read archive cursor"));
        }
        if max_parts == 0 || max_parts > 128 || max_bytes == 0 || max_bytes > MAX_ARCHIVE_PAGE_BYTES
        {
            return Err(Error::Limit);
        }
        if self.done {
            return Ok(ArchivePage {
                parts: vec![],
                done: true,
            });
        }
        match self.page(max_parts, max_bytes) {
            Ok(Some(page)) => Ok(page),
            Ok(None) => Err(Error::Limit),
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }
    fn page(&mut self, max_parts: u32, max_bytes: usize) -> Result<Option<ArchivePage>> {
        let mut st=sql(self.connection.prepare(
            "SELECT ordinal,CASE WHEN length(payload)<=?3 THEN payload ELSE NULL END FROM read_archive_parts WHERE operation_id=?1 AND ordinal>=?2 ORDER BY ordinal LIMIT ?4"))?;
        let mut rows = sql(st.query(params![
            self.operation,
            self.next,
            MAX_CONTAINER_BYTES as i64,
            max_parts
        ]))?;
        let mut parts = Vec::new();
        let mut used = 0usize;
        let mut next = self.next;
        let mut consumed = self.consumed;
        let mut previous = self.previous;
        while let Some(row) = sql(rows.next())? {
            let ordinal: i64 = sql(row.get(0))?;
            if ordinal != i64::from(next) || next >= self.count {
                return Err(Error::Integrity);
            }
            let raw = sql(row.get_ref(1))?;
            if matches!(raw, rusqlite::types::ValueRef::Null) {
                return Err(Error::Limit);
            }
            let part = Part::decode(raw.as_blob().map_err(|_| Error::Integrity)?)?;
            if part.ordinal() != next || part.previous_sha256() != previous {
                return Err(Error::Integrity);
            }
            let cost = part
                .raw()
                .len()
                .checked_add(part.container().len())
                .ok_or(Error::Limit)?;
            let added = used.checked_add(cost).ok_or(Error::Limit)?;
            if added > max_bytes {
                if parts.is_empty() {
                    return Ok(None);
                }
                break;
            }
            consumed = consumed.checked_add(cost as u64).ok_or(Error::Limit)?;
            if consumed > self.logical_bytes {
                return Err(Error::Integrity);
            }
            used = added;
            next = next.checked_add(1).ok_or(Error::Limit)?;
            previous = part.digest();
            parts.push(part);
        }
        // The complete sequence was verified at admission and this transaction cannot
        // see concurrent changes. A single indexed EOF probe also guards page completion.
        let remaining: bool = sql(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM read_archive_parts WHERE operation_id=?1 AND ordinal>=?2)",
            params![self.operation, next],
            |r| r.get(0),
        ))?;
        let done = !remaining;
        if done
            && (next != self.count
                || consumed != self.logical_bytes
                || previous != self.expected_chain)
        {
            return Err(Error::Integrity);
        }
        self.next = next;
        self.consumed = consumed;
        self.previous = previous;
        self.done = done;
        Ok(Some(ArchivePage { parts, done }))
    }
    /// Certifies complete cursor consumption, not delivery; release with close afterwards.
    pub fn finish(&self) -> Result<()> {
        if self.poisoned || self.closed || !self.done {
            return Err(Error::Invalid("incomplete read archive cursor"));
        }
        Ok(())
    }
    /// Explicit release is required before calling a completed reading operation closed.
    /// A rollback failure remains an error; Drop only supplies best-effort cleanup.
    pub fn close(mut self) -> Result<()> {
        let result = sql(self.connection.execute_batch("ROLLBACK"));
        self.closed = result.is_ok();
        result
    }
}
