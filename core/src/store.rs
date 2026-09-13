//! Shared single-database storage with native and explicitly selected OPFS adapters. Only a trusted host owns Store and HostPolicy.
//! SQLite pages/indices are engine-owned; business payloads are Protobuf + LZ4.
use crate::{
    Error, Result,
    content::CardRecord,
    envelope, identity,
    runtime::RenameRequest,
    transaction::{self, Lookup, Receipt},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{path::Path, time::Duration};
mod binding;
mod card_snapshot;
pub use card_snapshot::{CardPage, CardReadSnapshot, Census, FrozenCard, ReadPoint};
mod blobs;
mod evidence;
mod evidence_chunks;
pub use binding::{AuditBinding, AuditBindingState};
mod read_journal;
mod records;
mod seals;
const APPLICATION_ID: i64 = 0x4d4f5252;
#[derive(Clone, Copy)]
pub struct EventBudget {
    pub max_count: u32,
    pub max_bytes: u64,
}
impl Default for EventBudget {
    fn default() -> Self {
        Self {
            max_count: 1024,
            max_bytes: 64 * 1024 * 1024,
        }
    }
}
pub struct Store {
    connection: Connection,
    budget: EventBudget,
    audit_trust: Option<crate::audit::TrustedLog>,
    snapshot_origin: Option<std::path::PathBuf>,
    snapshot_identity: std::sync::Arc<()>,
}
fn sql<T>(value: rusqlite::Result<T>) -> Result<T> {
    value.map_err(|error| {
        #[cfg(all(target_arch = "wasm32", feature = "web-test-hooks"))]
        web_boundary(&format!("sqlite-error:{error:?}"));
        match error.sqlite_error_code() {
            Some(rusqlite::ErrorCode::DiskFull) => Error::StorageFull,
            Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
                Error::StorageBusy
            }
            _ => Error::Storage,
        }
    })
}
// Failpoints are excluded from default production builds and require an explicit feature.
#[cfg(all(target_arch = "wasm32", feature = "web-test-hooks"))]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_name=__morrowFaultBoundary)]
    fn web_boundary(name: &str);
}
fn boundary(_name: &str) {
    #[cfg(all(target_arch = "wasm32", feature = "web-test-hooks"))]
    web_boundary(_name);
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_TEST_CRASH_AT").as_deref() == Ok(_name) {
        // No stack unwinding or connection drop; tests recover in a fresh process.
        std::process::exit(86);
    }
}
fn blob(connection: &Connection, query: &str, id: &str, limit: usize) -> Result<Option<Vec<u8>>> {
    let mut statement = sql(connection.prepare(query))?;
    let value = sql(statement
        .query_row([id], |row| {
            let bytes = row.get_ref(0)?.as_blob()?;
            Ok(if bytes.len() > limit {
                None
            } else {
                Some(bytes.to_vec())
            })
        })
        .optional())?;
    match value {
        Some(None) => Err(Error::Limit),
        Some(Some(v)) => Ok(Some(v)),
        None => Ok(None),
    }
}
fn read_card(connection: &Connection, id: &str) -> Result<Option<CardRecord>> {
    let bytes = blob(
        connection,
        "SELECT payload FROM cards WHERE id=?1",
        id,
        envelope::MAX_CONTAINER_BYTES,
    )?;
    bytes
        .map(|v| {
            let card = envelope::decode(&v)?;
            if card.summary().id != id {
                return Err(Error::Integrity);
            }
            Ok(card)
        })
        .transpose()
}
fn read_commit(connection: &Connection, id: &str) -> Result<Option<Vec<u8>>> {
    blob(
        connection,
        "SELECT payload FROM operations WHERE id=?1",
        id,
        transaction::MAX_EVENT_BYTES + transaction::MAX_EVENT_BYTES / 255 + 128,
    )
}
impl Store {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(path: &Path, budget: EventBudget) -> Result<Self> {
        Self::open_mode(path, budget, true)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_existing(path: &Path, budget: EventBudget) -> Result<Self> {
        Self::open_mode(path, budget, false)
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn open_mode(path: &Path, budget: EventBudget, create: bool) -> Result<Self> {
        Self::open_adapter(path, budget, create, None, false, None)
    }
    /// Independent native verification: no creation, migration or journal changes.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_read_only_audited(path: &Path, trust: crate::audit::TrustedLog) -> Result<Self> {
        let connection = sql(Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ))?;
        sql(connection.busy_timeout(Duration::ZERO))?;
        let app: i64 = sql(connection.query_row("PRAGMA application_id", [], |r| r.get(0)))?;
        let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
        if app != APPLICATION_ID || !matches!(version, 5..=11) {
            return Err(Error::UnsupportedVersion);
        }
        let snapshot_origin = card_snapshot::origin(&connection, true)?;
        let store = Self {
            snapshot_origin,
            snapshot_identity: std::sync::Arc::new(()),
            connection,
            budget: EventBudget::default(),
            audit_trust: Some(trust),
        };
        store.integrity_check()?;
        Ok(store)
    }
    /// Host-selected OPFS adapter; missing named VFS is an error, never memory fallback.
    #[cfg(all(target_arch = "wasm32", feature = "web-storage"))]
    pub fn open_opfs(path: &Path, budget: EventBudget, create: bool) -> Result<Self> {
        Self::open_adapter(path, budget, create, Some("morrow-opfs"), true, None)
    }
    /// Native qualification of the exclusive rollback-journal transaction profile.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_exclusive(path: &Path, budget: EventBudget, create: bool) -> Result<Self> {
        Self::open_adapter(path, budget, create, None, true, None)
    }
    /// Host-pinned identity for reading or sealing an audited database.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_audited(
        path: &Path,
        budget: EventBudget,
        create: bool,
        trust: crate::audit::TrustedLog,
    ) -> Result<Self> {
        Self::open_adapter(path, budget, create, None, false, Some(trust))
    }
    #[cfg(all(target_arch = "wasm32", feature = "web-storage"))]
    pub fn open_opfs_audited(
        path: &Path,
        budget: EventBudget,
        create: bool,
        trust: crate::audit::TrustedLog,
    ) -> Result<Self> {
        Self::open_adapter(path, budget, create, Some("morrow-opfs"), true, Some(trust))
    }
    fn open_adapter(
        path: &Path,
        budget: EventBudget,
        create: bool,
        vfs: Option<&str>,
        exclusive: bool,
        audit_trust: Option<crate::audit::TrustedLog>,
    ) -> Result<Self> {
        let mut flags =
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
        if create {
            flags |= rusqlite::OpenFlags::SQLITE_OPEN_CREATE;
        }
        let mut connection = sql(match vfs {
            Some(name) => Connection::open_with_flags_and_vfs(path, flags, name),
            None => Connection::open_with_flags(path, flags),
        })?;
        sql(connection.busy_timeout(Duration::ZERO))?;
        // Reject unrelated and future databases before changing their pragmas/schema.
        let app: i64 = sql(connection.query_row("PRAGMA application_id", [], |r| r.get(0)))?;
        let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
        if !(app == 0 && version == 0 && create)
            && (app != APPLICATION_ID || !matches!(version, 4..=11))
        {
            return Err(Error::UnsupportedVersion);
        }
        if app == 0 && version == 0 && create {
            let tables: i64 = sql(connection.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            ))?;
            if tables != 0 {
                return Err(Error::Invalid("unrelated database"));
            }
        }
        if exclusive {
            let locking: String =
                sql(connection.query_row("PRAGMA locking_mode=EXCLUSIVE", [], |r| r.get(0)))?;
            if locking != "exclusive" {
                return Err(Error::Invalid("exclusive locking unavailable"));
            }
            let journal: String =
                sql(connection.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0)))?;
            if journal != "delete" {
                return Err(Error::Invalid("rollback journal unavailable"));
            }
        }
        sql(connection.pragma_update(None, "synchronous", "FULL"))?;
        if app == 0 && version == 0 && create {
            let tx = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
            let tables: i64 = sql(tx.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            ))?;
            if tables != 0 {
                return Err(Error::Invalid("unrelated database"));
            }
            sql(tx.execute_batch("CREATE TABLE cards (id TEXT PRIMARY KEY, payload BLOB NOT NULL) STRICT;
                CREATE TABLE operations (id TEXT PRIMARY KEY, card_id TEXT NOT NULL, object_kind INTEGER NOT NULL DEFAULT 0, payload BLOB NOT NULL) STRICT;
                CREATE INDEX operation_card ON operations(card_id);
                CREATE TABLE outbox (sequence INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT UNIQUE NOT NULL REFERENCES operations(id), payload BLOB NOT NULL) STRICT;
                PRAGMA application_id=1297044050; PRAGMA user_version=4;"))?;
            sql(tx.execute_batch(blobs::SCHEMA))?;
            sql(tx.execute_batch(records::SCHEMA))?;
            sql(tx.commit())?;
        } else if app != APPLICATION_ID || !matches!(version, 4..=11) {
            return Err(Error::UnsupportedVersion);
        }
        if version == 4 || (app == 0 && version == 0 && create) {
            let tx = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
            sql(tx.execute_batch(seals::SCHEMA))?;
            sql(tx.execute(
                "INSERT INTO operation_events(sequence,id) SELECT sequence,id FROM outbox",
                [],
            ))?;
            boundary("seal-migration-after-index");
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            sql(tx.pragma_update(None, "user_version", 5))?;
            boundary("seal-migration-before-commit");
            tx.commit().map_err(|_| Error::CommitUnknown)?;
            boundary("seal-migration-after-commit");
        }
        // Bind identity before any audited caller can publish a content event.
        // A v5 signed database must first verify with caller-supplied trust.
        if version < 6 {
            let tx = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            sql(tx.execute_batch(binding::SCHEMA))?;
            if let Some(trust) = audit_trust.as_ref() {
                binding::bind(&tx, trust)?;
            }
            sql(tx.pragma_update(None, "user_version", 6))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            boundary("binding-before-commit");
            tx.commit().map_err(|_| Error::CommitUnknown)?;
            boundary("binding-after-commit");
        } else if let Some(trust) = audit_trust.as_ref() {
            let tx = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
            Self::integrity_connection(&tx, Some(trust))?;
            binding::bind(&tx, trust)?;
            boundary("binding-before-commit");
            tx.commit().map_err(|_| Error::CommitUnknown)?;
            boundary("binding-after-commit");
        }
        if version < 7 {
            let tx = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            sql(tx.execute_batch(evidence::SCHEMA))?;
            sql(tx.pragma_update(None, "user_version", 7))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            boundary("evidence-migration-before-commit");
            tx.commit().map_err(|_| Error::CommitUnknown)?;
            boundary("evidence-migration-after-commit");
        }
        if version < 8 {
            let tx = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            evidence_chunks::migrate(&tx)?;
            sql(tx.pragma_update(None, "user_version", 8))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            boundary("evidence-chunks-migration-before-commit");
            tx.commit().map_err(|_| Error::CommitUnknown)?;
            boundary("evidence-chunks-migration-after-commit");
        }
        if version < 9 {
            let tx = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            sql(tx.pragma_update(None, "user_version", 9))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            boundary("batch-evidence-migration-before-commit");
            tx.commit().map_err(|_| Error::CommitUnknown)?;
            boundary("batch-evidence-migration-after-commit");
        }
        if version < 10 {
            let tx = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            sql(tx.pragma_update(None, "user_version", 10))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            boundary("projection-evidence-migration-before-commit");
            tx.commit().map_err(|_| Error::CommitUnknown)?;
            boundary("projection-evidence-migration-after-commit");
        }
        if version < 11 {
            let tx = sql(connection.transaction_with_behavior(TransactionBehavior::Immediate))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            sql(tx.pragma_update(None, "user_version", 11))?;
            Self::integrity_connection(&tx, audit_trust.as_ref())?;
            boundary("read-journal-migration-before-commit");
            tx.commit().map_err(|_| Error::CommitUnknown)?;
            boundary("read-journal-migration-after-commit");
        }
        sql(connection.pragma_update(None, "foreign_keys", true))?;
        sql(connection.pragma_update(None, "trusted_schema", false))?;
        if !exclusive {
            let mode: String =
                sql(connection.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0)))?;
            if mode != "wal" {
                return Err(Error::Invalid("WAL unavailable"));
            }
        }
        sql(connection.pragma_update(None, "synchronous", "FULL"))?;
        let snapshot_origin = card_snapshot::origin(&connection, !exclusive && vfs.is_none())?;
        let store = Self {
            snapshot_origin,
            snapshot_identity: std::sync::Arc::new(()),
            connection,
            budget,
            audit_trust,
        };
        store.integrity_check()?;
        Ok(store)
    }
    pub fn card(&self, id: &str) -> Result<Option<CardRecord>> {
        identity(id)?;
        read_card(&self.connection, id)
    }
    /// Host-only query. A transport must apply its read permissions before exposing this.
    pub fn lookup(&self, operation_id: &str) -> Result<Lookup> {
        identity(operation_id)?;
        match read_commit(&self.connection, operation_id)? {
            Some(raw) => {
                let (_, receipt) = transaction::decode_commit(&raw)?;
                if receipt.operation_id != operation_id {
                    return Err(Error::Integrity);
                }
                Ok(Lookup::Committed(receipt))
            }
            None => Ok(Lookup::Absent),
        }
    }
    /// Scoped before payload access: a receipt for another card is indistinguishable from absence.
    pub fn lookup_for_card(&self, card_id: &str, operation_id: &str) -> Result<Lookup> {
        identity(card_id)?;
        identity(operation_id)?;
        let mut statement = sql(self.connection.prepare(
            "SELECT payload FROM operations WHERE id=?1 AND card_id=?2 AND object_kind=0",
        ))?;
        let value = sql(statement
            .query_row(params![operation_id, card_id], |row| {
                let bytes = row.get_ref(0)?.as_blob()?;
                Ok(
                    if bytes.len()
                        <= transaction::MAX_EVENT_BYTES + transaction::MAX_EVENT_BYTES / 255 + 128
                    {
                        Some(bytes.to_vec())
                    } else {
                        None
                    },
                )
            })
            .optional())?;
        match value {
            None => Ok(Lookup::Absent),
            Some(None) => Err(Error::Limit),
            Some(Some(bytes)) => {
                let (_, receipt) = transaction::decode_commit(&bytes)?;
                if receipt.card_id != card_id || receipt.operation_id != operation_id {
                    return Err(Error::Integrity);
                }
                Ok(Lookup::Committed(receipt))
            }
        }
    }
    /// Host-local creation only. Not a public plugin command or migration entry point.
    /// Every attachment must already exist as a verified staged payload.
    pub fn create_local(&mut self, operation_id: &str, card: &CardRecord) -> Result<Receipt> {
        let command = transaction::create_command(operation_id, card)?;
        let id = card.summary().id;
        self.apply(
            operation_id,
            &id,
            command,
            &[],
            |_| Ok(card.clone()),
            || Ok(()),
            true,
        )
    }
    pub fn set_attachments_local(
        &mut self,
        operation_id: &str,
        card_id: &str,
        expected_revision: u64,
        attachments: &[crate::content::Attachment],
    ) -> Result<Receipt> {
        let command = transaction::set_attachments_command(
            operation_id,
            card_id,
            expected_revision,
            attachments,
        )?;
        self.apply(
            operation_id,
            card_id,
            command,
            &[],
            |card| {
                card.ok_or(Error::NotFound)?
                    .with_attachments(expected_revision, attachments)
            },
            || Ok(()),
            false,
        )
    }
    pub(crate) fn create_authorized_with_evidence(
        &mut self,
        operation_id: &str,
        card: &CardRecord,
        evidence: &[crate::task_evidence::Evidence],
        authorize: impl FnMut() -> Result<()>,
    ) -> Result<Receipt> {
        if card.summary().revision != 1 {
            return Err(Error::Invalid("creation revision"));
        }
        let command = transaction::create_command(operation_id, card)?;
        self.apply(
            operation_id,
            &card.summary().id,
            command,
            evidence,
            |_| Ok(card.clone()),
            authorize,
            true,
        )
    }
    pub(crate) fn edit_content_with_evidence(
        &mut self,
        change: &crate::content_change::ContentChange,
        evidence: &[crate::task_evidence::Evidence],
        authorize: impl FnMut() -> Result<()>,
    ) -> Result<Receipt> {
        let command = transaction::content_command(change)?;
        self.apply(
            &change.operation_id,
            &change.card_id,
            command,
            evidence,
            |card| change.propose(card.ok_or(Error::NotFound)?),
            authorize,
            false,
        )
    }
    /// Trusted bounded index for host discovery. Returning IDs does not grant
    /// plugins read access; each resulting object still needs authorization.
    pub fn card_ids_local(&self, after: &str, limit: u32) -> Result<Vec<String>> {
        if limit == 0 || limit > 128 {
            return Err(Error::Limit);
        }
        if !after.is_empty() {
            identity(after)?;
        }
        let mut stmt = sql(self
            .connection
            .prepare("SELECT id FROM cards WHERE id>?1 ORDER BY id LIMIT ?2"))?;
        let rows = sql(stmt.query_map(params![after, limit], |r| r.get(0)))?;
        rows.map(sql).collect()
    }
    pub(crate) fn rename(
        &mut self,
        request: &RenameRequest,
        authorize: impl FnMut() -> Result<()>,
    ) -> Result<Receipt> {
        let command = transaction::rename_command(request)?;
        self.apply(
            &request.operation_id,
            &request.card_id,
            command,
            &[],
            |current| request.propose(current.ok_or(Error::NotFound)?),
            authorize,
            false,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        operation_id: &str,
        card_id: &str,
        command: Vec<u8>,
        task_evidence: &[crate::task_evidence::Evidence],
        propose: impl FnOnce(Option<&CardRecord>) -> Result<CardRecord>,
        mut authorize: impl FnMut() -> Result<()>,
        create: bool,
    ) -> Result<Receipt> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        boundary("after-begin");
        authorize()?;
        let evidence_digests = evidence::digests(task_evidence)?;
        if let Some(raw) = read_commit(&tx, operation_id)? {
            if !raw.starts_with(b"MORROWT1") {
                return Err(Error::OperationConflict);
            }
            let (previous, receipt) = transaction::decode_commit(&raw)?;
            if previous.operation_id != operation_id {
                return Err(Error::Integrity);
            }
            if previous.command != command
                || previous
                    .task_evidence_sha256
                    .iter()
                    .map(Vec::as_slice)
                    .ne(evidence_digests.iter().map(|d| d.as_slice()))
            {
                return Err(Error::OperationConflict);
            }
            evidence::verify_retry(
                &tx,
                operation_id,
                &previous.task_evidence_sha256,
                task_evidence,
            )?;
            authorize()?;
            return Ok(receipt);
        }
        let current = read_card(&tx, card_id)?;
        if create && current.is_some() {
            return Err(Error::RevisionConflict);
        }
        let next = propose(current.as_ref())?;
        let event = transaction::encode_commit_with_evidence(command, &next, &evidence_digests)?;
        let receipt = transaction::decode_commit(&event)?.1;
        let (count, bytes): (i64, i64) = sql(tx.query_row(
            "SELECT count(*), coalesce(sum(length(payload)),0) FROM outbox",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ))?;
        if count >= i64::from(self.budget.max_count)
            || (bytes as u64).saturating_add(event.len() as u64) > self.budget.max_bytes
        {
            return Err(Error::EventCapacity);
        }
        let payload = envelope::encode(&next)?;
        sql(tx.execute("INSERT INTO cards(id,payload) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", params![card_id, payload]))?;
        boundary("after-card");
        sql(tx.execute(
            "INSERT INTO operations(id,card_id,payload) VALUES(?1,?2,?3)",
            params![operation_id, card_id, event],
        ))?;
        boundary("after-operation");
        sql(tx.execute(
            "INSERT INTO outbox(id,payload) VALUES(?1,?2)",
            params![operation_id, event],
        ))?;
        sql(tx.execute(
            "INSERT INTO operation_events(sequence,id) VALUES(last_insert_rowid(),?1)",
            [operation_id],
        ))?;
        boundary("after-event");
        blobs::bind(&tx, &next, operation_id)?;
        evidence::bind(&tx, operation_id, task_evidence)?;
        boundary("after-task-evidence");
        // The host is exclusively borrowed throughout. Revocation and commit are serialized.
        // A fresh host clock tick rejects expiry during synchronous preparation/I/O.
        authorize()?;
        boundary("before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("after-commit");
        Ok(receipt)
    }
    /// Engine-consistent snapshot into a new staging file. A failed partial file
    /// must never be published by the caller; this does not modify the source.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn snapshot_to(&self, path: &Path, max_bytes: u64) -> Result<()> {
        let pinned = sql(self.connection.unchecked_transaction())?;
        let _: i64 = sql(pinned.query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0)))?;
        Self::integrity_connection(&pinned, self.audit_trust.as_ref())?;
        let pages: u64 = u64::try_from(sql(
            pinned.query_row("PRAGMA page_count", [], |r| r.get::<_, i64>(0))
        )?)
        .map_err(|_| Error::Integrity)?;
        let page_size: u64 = u64::try_from(sql(
            pinned.query_row("PRAGMA page_size", [], |r| r.get::<_, i64>(0))
        )?)
        .map_err(|_| Error::Integrity)?;
        if pages.checked_mul(page_size).ok_or(Error::Limit)? > max_bytes {
            return Err(Error::Limit);
        }
        let created = std::fs::File::options()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| Error::Io)?;
        drop(created);
        let mut target = sql(Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        ))?;
        let backup = sql(rusqlite::backup::Backup::new(&pinned, &mut target))?;
        for _ in 0..=pages.div_ceil(128) {
            match sql(backup.step(128))? {
                rusqlite::backup::StepResult::Done => return Ok(()),
                rusqlite::backup::StepResult::More => {}
                rusqlite::backup::StepResult::Busy | rusqlite::backup::StepResult::Locked => {
                    return Err(Error::StorageBusy);
                }
                _ => return Err(Error::Storage),
            }
        }
        Err(Error::Limit)
    }
    /// Host-local queue pressure without reading or allocating event payloads.
    pub fn pending_usage(&self) -> Result<(u64, u64)> {
        let (events, bytes): (i64, i64) = sql(self.connection.query_row(
            "SELECT count(*),coalesce(sum(length(payload)),0) FROM outbox",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ))?;
        Ok((
            u64::try_from(events).map_err(|_| Error::Integrity)?,
            u64::try_from(bytes).map_err(|_| Error::Integrity)?,
        ))
    }
    /// Bounded immutable events that have not yet been atomically sealed.
    pub fn pending(&self, after_sequence: i64, limit: u32) -> Result<Vec<(i64, Vec<u8>)>> {
        if after_sequence < 0 || limit == 0 || limit > 128 {
            return Err(Error::Limit);
        }
        let mut statement = sql(self.connection.prepare(
            "SELECT sequence,CASE WHEN length(payload)<=?3 THEN payload ELSE NULL END FROM outbox WHERE sequence>?1 ORDER BY sequence LIMIT ?2",
        ))?;
        let mut rows = sql(statement.query(params![
            after_sequence,
            limit,
            crate::read_journal::MAX_CONTAINER_BYTES as i64
        ]))?;
        let mut result = Vec::new();
        let mut total = 0usize;
        while let Some(row) = sql(rows.next())? {
            let raw = sql(row.get_ref(1))?;
            if matches!(raw, rusqlite::types::ValueRef::Null) {
                return Err(Error::Limit);
            }
            let value = raw.as_blob().map_err(|_| Error::Integrity)?;
            total = total.checked_add(value.len()).ok_or(Error::Limit)?;
            if total > 32 * 1024 * 1024 {
                return Err(Error::Limit);
            }
            if value.starts_with(crate::read_journal::MAGIC) {
                let version: i64 =
                    sql(self
                        .connection
                        .query_row("PRAGMA user_version", [], |r| r.get(0)))?;
                if version < 11 {
                    return Err(Error::UnsupportedVersion);
                }
                crate::read_journal::decode(value)?;
            } else if value.starts_with(b"MORROWR1") {
                crate::records::decode_commit(value)?;
            } else {
                transaction::decode_commit(value)?;
            }
            result.push((sql(row.get(0))?, value.to_vec()));
        }
        Ok(result)
    }
    pub fn integrity_check(&self) -> Result<()> {
        let snapshot = sql(self.connection.unchecked_transaction())?;
        Self::integrity_connection(&snapshot, self.audit_trust.as_ref())
    }
    fn integrity_connection(
        snapshot: &Connection,
        trust: Option<&crate::audit::TrustedLog>,
    ) -> Result<()> {
        let result: String = sql(snapshot.query_row("PRAGMA integrity_check", [], |r| r.get(0)))?;
        if result != "ok" {
            return Err(Error::Integrity);
        }
        evidence::verify_schema(snapshot)?;
        evidence_chunks::verify_schema(snapshot)?;
        binding::verify(snapshot, trust)?;
        seals::verify(snapshot, trust)?;
        let mut operations = sql(snapshot.prepare(
            "SELECT id,card_id,CASE WHEN length(payload)<=?1 THEN payload ELSE NULL END,object_kind FROM operations",
        ))?;
        let mut rows = sql(operations.query([crate::read_journal::MAX_CONTAINER_BYTES as i64]))?;
        while let Some(row) = sql(rows.next())? {
            let value = sql(row.get_ref(2))?;
            if matches!(value, rusqlite::types::ValueRef::Null) {
                return Err(Error::Limit);
            }
            let raw = value.as_blob().map_err(|_| Error::Integrity)?;
            let kind: i64 = sql(row.get(3))?;
            if kind == 4 {
                read_journal::verify_operation(
                    snapshot,
                    &sql(row.get::<_, String>(0))?,
                    &sql(row.get::<_, String>(1))?,
                    raw,
                )?;
                continue;
            }
            if kind != 0 {
                records::verify_operation(
                    snapshot,
                    kind,
                    &sql(row.get::<_, String>(0))?,
                    &sql(row.get::<_, String>(1))?,
                    raw,
                )?;
                continue;
            }
            let (event, receipt) = transaction::decode_commit(raw)?;
            blobs::verify_event(snapshot, &receipt.operation_id, &event.attachment_sha256)?;
            evidence::verify_event(snapshot, &receipt.operation_id, &event.task_evidence_sha256)?;
            if receipt.operation_id != sql(row.get::<_, String>(0))?
                || receipt.card_id != sql(row.get::<_, String>(1))?
            {
                return Err(Error::Integrity);
            }
        }
        let mut cards = sql(snapshot.prepare("SELECT id,payload FROM cards"))?;
        let mut rows = sql(cards.query([]))?;
        while let Some(row) = sql(rows.next())? {
            use sha2::{Digest, Sha256};
            let id: String = sql(row.get(0))?;
            let raw = sql(row.get_ref(1))?
                .as_blob()
                .map_err(|_| Error::Integrity)?;
            let card = envelope::decode(raw)?;
            blobs::verify_card(snapshot, &card)?;
            let event = blob(snapshot,
                "SELECT o.payload FROM operations o JOIN operation_events e ON o.id=e.id WHERE o.card_id=?1 AND o.object_kind=0 ORDER BY e.sequence DESC LIMIT 1", &id,
                transaction::MAX_EVENT_BYTES + transaction::MAX_EVENT_BYTES / 255 + 128)?.ok_or(Error::Integrity)?;
            let (_, receipt) = transaction::decode_commit(&event)?;
            if card.summary().id != id
                || receipt.card_id != id
                || receipt.revision != card.summary().revision
                || receipt.content_sha256 != <[u8; 32]>::from(Sha256::digest(card.encode()))
            {
                return Err(Error::Integrity);
            }
        }
        let missing: i64 = sql(snapshot.query_row("SELECT count(*) FROM operations o LEFT JOIN cards c ON o.card_id=c.id WHERE o.object_kind=0 AND c.id IS NULL", [], |r| r.get(0)))?;
        if missing != 0 {
            return Err(Error::Integrity);
        }
        records::verify(snapshot)?;
        blobs::verify(snapshot)?;
        evidence::verify(snapshot)?;
        Ok(())
    }
}

#[cfg(test)]
mod disk_tests {
    use super::*;
    #[test]
    fn sqlite_full_during_raw_stage_leaves_no_partial_blob() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("full.db"), EventBudget::default()).unwrap();
        let pages: i64 = store
            .connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        store
            .connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        let result = store.stage_blob(&mut std::io::repeat(7), 1024 * 1024, None, 0);
        assert!(matches!(result, Err(Error::StorageFull)));
        assert!(store.list_blobs_local("", 128).unwrap().is_empty());
        store.integrity_check().unwrap();
    }

    #[test]
    fn sqlite_full_rolls_back_all_three_records() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("full.db"), EventBudget::default()).unwrap();
        let pages: i64 = store
            .connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        store
            .connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        let mut seed = 0x12345678u32;
        let body: Vec<u8> = (0..512 * 1024)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        let card = CardRecord::new("large", "unknown", 1, "test", body).unwrap();
        assert_eq!(store.create_local("full", &card), Err(Error::StorageFull));
        assert!(store.card("large").unwrap().is_none());
        assert_eq!(store.lookup("full").unwrap(), Lookup::Absent);
        assert!(store.pending(0, 10).unwrap().is_empty());
        store.integrity_check().unwrap();
    }
}
