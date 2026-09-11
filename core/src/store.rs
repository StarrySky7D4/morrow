//! Native single-database storage. Only a trusted host owns Store and HostPolicy.
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
}
fn sql<T>(value: rusqlite::Result<T>) -> Result<T> {
    value.map_err(|error| match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DiskFull) => Error::StorageFull,
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            Error::StorageBusy
        }
        _ => Error::Storage,
    })
}
// Failpoints are excluded from default production builds and require an explicit feature.
fn boundary(_name: &str) {
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
    pub fn open(path: &Path, budget: EventBudget) -> Result<Self> {
        Self::open_mode(path, budget, true)
    }
    pub fn open_existing(path: &Path, budget: EventBudget) -> Result<Self> {
        Self::open_mode(path, budget, false)
    }
    fn open_mode(path: &Path, budget: EventBudget, create: bool) -> Result<Self> {
        let mut flags =
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
        if create {
            flags |= rusqlite::OpenFlags::SQLITE_OPEN_CREATE;
        }
        let mut connection = sql(Connection::open_with_flags(path, flags))?;
        sql(connection.busy_timeout(Duration::ZERO))?;
        // Reject unrelated and future databases before changing their pragmas/schema.
        let app: i64 = sql(connection.query_row("PRAGMA application_id", [], |r| r.get(0)))?;
        let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
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
                CREATE TABLE operations (id TEXT PRIMARY KEY, card_id TEXT NOT NULL, payload BLOB NOT NULL) STRICT;
                CREATE INDEX operation_card ON operations(card_id);
                CREATE TABLE outbox (sequence INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT UNIQUE NOT NULL REFERENCES operations(id), payload BLOB NOT NULL) STRICT;
                PRAGMA application_id=1297044050; PRAGMA user_version=1;"))?;
            sql(tx.commit())?;
        } else if app != APPLICATION_ID || version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        sql(connection.pragma_update(None, "foreign_keys", true))?;
        sql(connection.pragma_update(None, "trusted_schema", false))?;
        let mode: String = sql(connection.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0)))?;
        if mode != "wal" {
            return Err(Error::Invalid("WAL unavailable"));
        }
        sql(connection.pragma_update(None, "synchronous", "FULL"))?;
        let store = Self { connection, budget };
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
    /// Host-local creation only. Not a public plugin command or migration entry point.
    /// Attachment references are rejected until the blob transaction protocol exists.
    pub fn create_local(&mut self, operation_id: &str, card: &CardRecord) -> Result<Receipt> {
        if card.has_attachments() {
            return Err(Error::Invalid("blob storage not established"));
        }
        let command = transaction::create_command(operation_id, card)?;
        let id = card.summary().id;
        self.apply(
            operation_id,
            &id,
            command,
            |_| Ok(card.clone()),
            || Ok(()),
            true,
        )
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
            |current| request.propose(current.ok_or(Error::NotFound)?),
            authorize,
            false,
        )
    }
    fn apply(
        &mut self,
        operation_id: &str,
        card_id: &str,
        command: Vec<u8>,
        propose: impl FnOnce(Option<&CardRecord>) -> Result<CardRecord>,
        mut authorize: impl FnMut() -> Result<()>,
        create: bool,
    ) -> Result<Receipt> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        boundary("after-begin");
        authorize()?;
        if let Some(raw) = read_commit(&tx, operation_id)? {
            let (previous, receipt) = transaction::decode_commit(&raw)?;
            if previous.operation_id != operation_id {
                return Err(Error::Integrity);
            }
            if previous.command != command {
                return Err(Error::OperationConflict);
            }
            authorize()?;
            return Ok(receipt);
        }
        let current = read_card(&tx, card_id)?;
        if create && current.is_some() {
            return Err(Error::RevisionConflict);
        }
        let next = propose(current.as_ref())?;
        let event = transaction::encode_commit(command, &next)?;
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
        boundary("after-event");
        // The host is exclusively borrowed throughout. Revocation and commit are serialized.
        // A fresh host clock tick rejects expiry during synchronous preparation/I/O.
        authorize()?;
        boundary("before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("after-commit");
        Ok(receipt)
    }
    /// Bounded, immutable pending records for the future sealer. No deletion/ack until M4.
    pub fn pending(&self, after_sequence: i64, limit: u32) -> Result<Vec<(i64, Vec<u8>)>> {
        if after_sequence < 0 || limit == 0 || limit > 128 {
            return Err(Error::Limit);
        }
        let mut statement = sql(self.connection.prepare(
            "SELECT sequence,payload FROM outbox WHERE sequence>?1 ORDER BY sequence LIMIT ?2",
        ))?;
        let mut rows = sql(statement.query(params![after_sequence, limit]))?;
        let mut result = Vec::new();
        let mut total = 0usize;
        while let Some(row) = sql(rows.next())? {
            let value = sql(row.get_ref(1))?
                .as_blob()
                .map_err(|_| Error::Integrity)?;
            total = total.checked_add(value.len()).ok_or(Error::Limit)?;
            if total > 32 * 1024 * 1024 {
                return Err(Error::Limit);
            }
            transaction::decode_commit(value)?;
            result.push((sql(row.get(0))?, value.to_vec()));
        }
        Ok(result)
    }
    pub fn integrity_check(&self) -> Result<()> {
        let snapshot = sql(self.connection.unchecked_transaction())?;
        let result: String = sql(snapshot.query_row("PRAGMA integrity_check", [], |r| r.get(0)))?;
        if result != "ok" {
            return Err(Error::Integrity);
        }
        let broken: i64 = sql(snapshot.query_row("SELECT count(*) FROM operations o LEFT JOIN outbox e ON o.id=e.id WHERE e.id IS NULL OR o.payload != e.payload", [], |r| r.get(0)))?;
        let orphans: i64 = sql(snapshot.query_row(
            "SELECT count(*) FROM outbox e LEFT JOIN operations o ON o.id=e.id WHERE o.id IS NULL",
            [],
            |r| r.get(0),
        ))?;
        if broken != 0 || orphans != 0 {
            return Err(Error::Integrity);
        }
        let mut operations = sql(snapshot.prepare("SELECT id,card_id,payload FROM operations"))?;
        let mut rows = sql(operations.query([]))?;
        while let Some(row) = sql(rows.next())? {
            let raw = sql(row.get_ref(2))?
                .as_blob()
                .map_err(|_| Error::Integrity)?;
            let (_, receipt) = transaction::decode_commit(raw)?;
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
            let event = blob(&snapshot,
                "SELECT o.payload FROM operations o JOIN outbox e ON o.id=e.id WHERE o.card_id=?1 ORDER BY e.sequence DESC LIMIT 1", &id,
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
        let missing: i64 = sql(snapshot.query_row("SELECT count(*) FROM operations o LEFT JOIN cards c ON o.card_id=c.id WHERE c.id IS NULL", [], |r| r.get(0)))?;
        if missing != 0 {
            return Err(Error::Integrity);
        }
        Ok(())
    }
}

#[cfg(test)]
mod disk_tests {
    use super::*;
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
