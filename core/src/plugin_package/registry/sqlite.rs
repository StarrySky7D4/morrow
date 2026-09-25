//! Exclusive SQLite package/registry storage, including the named browser OPFS VFS.
//! Snapshot bytes are the same envelope as the native file registry.
use super::{MAX_CONTAINER, RegistryStorage};
use crate::{
    Error, Result,
    plugin_package::{MAX_PACKAGE_BYTES, Package},
};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use std::{cell::Cell, path::Path, time::Duration};

const APPLICATION_ID: i64 = 0x4d505247;
const VERSION: i64 = 1;
pub struct SqliteRegistryStorage {
    connection: Connection,
    poisoned: Cell<bool>,
    #[cfg(target_arch = "wasm32")]
    _lease: OpfsRegistryLease,
}
// The SAH pool owns the cross-Worker lease. Its SQLite xLock does not
// arbitrate connections within one Wasm instance, so enforce that here.
#[cfg(target_arch = "wasm32")]
static OPEN_OPFS_REGISTRIES: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());
#[cfg(target_arch = "wasm32")]
struct OpfsRegistryLease(String);
#[cfg(target_arch = "wasm32")]
impl OpfsRegistryLease {
    fn acquire(path: &Path) -> Result<Self> {
        let name = path.to_str().ok_or(Error::Invalid("registry path"))?;
        // Restrict the named adapter to canonical, single-component names.
        if !name.starts_with("plugin-registry-")
            || !name.ends_with(".sqlite3")
            || name.len() > 96
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
        {
            return Err(Error::Invalid("registry path"));
        }
        let mut opened = OPEN_OPFS_REGISTRIES.lock().map_err(|_| Error::Storage)?;
        if !opened.insert(name.to_owned()) {
            return Err(Error::StorageBusy);
        }
        Ok(Self(name.to_owned()))
    }
}
#[cfg(target_arch = "wasm32")]
impl Drop for OpfsRegistryLease {
    fn drop(&mut self) {
        if let Ok(mut opened) = OPEN_OPFS_REGISTRIES.lock() {
            opened.remove(&self.0);
        }
    }
}
fn sql<T>(result: rusqlite::Result<T>) -> Result<T> {
    result.map_err(|error| match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            Error::StorageBusy
        }
        Some(rusqlite::ErrorCode::DiskFull) => Error::StorageFull,
        _ => Error::Storage,
    })
}
impl SqliteRegistryStorage {
    /// Native qualification of the same exclusive rollback-journal storage profile.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(path: &Path, create: bool) -> Result<Self> {
        Self::open_adapter(path, create, None)
    }
    #[cfg(all(target_arch = "wasm32", feature = "web-storage"))]
    pub fn open_opfs(path: &Path, create: bool) -> Result<Self> {
        Self::open_adapter(path, create, Some("morrow-opfs"))
    }
    fn open_adapter(path: &Path, create: bool, vfs: Option<&str>) -> Result<Self> {
        #[cfg(target_arch = "wasm32")]
        let lease = OpfsRegistryLease::acquire(path)?;
        let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        if create {
            flags |= OpenFlags::SQLITE_OPEN_CREATE;
        }
        let mut connection = sql(match vfs {
            Some(vfs) => Connection::open_with_flags_and_vfs(path, flags, vfs),
            None => Connection::open_with_flags(path, flags),
        })?;
        sql(connection.busy_timeout(Duration::ZERO))?;
        let app: i64 = sql(connection.query_row("PRAGMA application_id", [], |row| row.get(0)))?;
        let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |row| row.get(0)))?;
        let fresh = app == 0 && version == 0 && create;
        if !fresh && (app != APPLICATION_ID || version != VERSION) {
            return Err(Error::UnsupportedVersion);
        }
        if fresh {
            let count: i64 = sql(connection.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            ))?;
            if count != 0 {
                return Err(Error::Invalid("unrelated registry database"));
            }
        }
        sql(connection.pragma_update(None, "trusted_schema", false))?;
        let mode: String =
            sql(connection.query_row("PRAGMA locking_mode=EXCLUSIVE", [], |row| row.get(0)))?;
        let journal: String =
            sql(connection.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0)))?;
        if mode != "exclusive" || journal != "delete" {
            return Err(Error::Invalid("registry locking unavailable"));
        }
        sql(connection.pragma_update(None, "synchronous", "FULL"))?;
        let transaction =
            sql(connection.transaction_with_behavior(TransactionBehavior::Exclusive))?;
        let locked_app: i64 =
            sql(transaction.query_row("PRAGMA application_id", [], |row| row.get(0)))?;
        let locked_version: i64 =
            sql(transaction.query_row("PRAGMA user_version", [], |row| row.get(0)))?;
        if locked_app != app || locked_version != version {
            return Err(Error::Integrity);
        }
        if fresh {
            let count: i64 = sql(transaction.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            ))?;
            if count != 0 {
                return Err(Error::Invalid("unrelated registry database"));
            }
            sql(transaction.execute_batch(
                "CREATE TABLE packages(digest BLOB PRIMARY KEY CHECK(length(digest)=32), archive BLOB NOT NULL) STRICT;
                 CREATE TABLE selection(id INTEGER PRIMARY KEY CHECK(id=1), snapshot BLOB NOT NULL) STRICT;"
            ))?;
            sql(transaction.pragma_update(None, "application_id", APPLICATION_ID))?;
            sql(transaction.pragma_update(None, "user_version", VERSION))?;
        }
        let integrity: String =
            sql(transaction.query_row("PRAGMA quick_check", [], |row| row.get(0)))?;
        if integrity != "ok" {
            return Err(Error::Integrity);
        }
        transaction.commit().map_err(|_| Error::CommitUnknown)?;
        // EXCLUSIVE mode retains the database lock after commit until drop.
        Ok(Self {
            connection,
            poisoned: Cell::new(false),
            #[cfg(target_arch = "wasm32")]
            _lease: lease,
        })
    }
    fn ready(&self) -> Result<()> {
        if self.poisoned.get() {
            Err(Error::CommitUnknown)
        } else {
            Ok(())
        }
    }
    fn commit(&self, transaction: Transaction<'_>) -> Result<()> {
        transaction.commit().map_err(|_| {
            self.poisoned.set(true);
            Error::CommitUnknown
        })
    }
}
impl RegistryStorage for SqliteRegistryStorage {
    fn read(&self) -> Result<Option<Vec<u8>>> {
        self.ready()?;
        let length: Option<i64> = sql(self
            .connection
            .query_row(
                "SELECT length(snapshot) FROM selection WHERE id=1",
                [],
                |row| row.get(0),
            )
            .optional())?;
        let Some(length) = length else {
            return Ok(None);
        };
        if length < 0 || length as usize > MAX_CONTAINER {
            return Err(Error::Limit);
        }
        sql(self
            .connection
            .query_row("SELECT snapshot FROM selection WHERE id=1", [], |row| {
                row.get(0)
            }))
        .map(Some)
    }
    fn publish(&mut self, bytes: &[u8]) -> Result<()> {
        self.ready()?;
        if bytes.len() > MAX_CONTAINER {
            return Err(Error::Limit);
        }
        let transaction = sql(Transaction::new_unchecked(
            &self.connection,
            TransactionBehavior::Immediate,
        ))?;
        sql(transaction.execute("INSERT INTO selection(id,snapshot) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET snapshot=excluded.snapshot", [bytes]))?;
        self.commit(transaction)
    }
    fn load_package(&self, digest: [u8; 32]) -> Result<Package> {
        self.ready()?;
        let length: Option<i64> = sql(self
            .connection
            .query_row(
                "SELECT length(archive) FROM packages WHERE digest=?1",
                [digest.as_slice()],
                |row| row.get(0),
            )
            .optional())?;
        let length = length.ok_or(Error::NotFound)?;
        if length < 0 || length as usize > MAX_PACKAGE_BYTES {
            return Err(Error::Limit);
        }
        let archive: Vec<u8> = sql(self.connection.query_row(
            "SELECT archive FROM packages WHERE digest=?1",
            [digest.as_slice()],
            |row| row.get(0),
        ))?;
        let package = Package::decode(&archive)?;
        if package.digest() != digest {
            return Err(Error::Integrity);
        }
        Ok(package)
    }
    fn install_package(&self, package: &Package) -> Result<()> {
        self.ready()?;
        match self.load_package(package.digest()) {
            Ok(_) => return Ok(()),
            Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
        let transaction = sql(Transaction::new_unchecked(
            &self.connection,
            TransactionBehavior::Immediate,
        ))?;
        sql(transaction.execute(
            "INSERT INTO packages(digest,archive) VALUES(?1,?2)",
            params![package.digest().as_slice(), package.archive()],
        ))?;
        self.commit(transaction)?;
        self.load_package(package.digest()).map(|_| ())
    }
}
