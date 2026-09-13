//! Durable first binding, before the first event. The identity is not a trust source.
use super::sql;
#[cfg(not(target_arch = "wasm32"))]
use super::{APPLICATION_ID, Store};
use crate::{
    Error, Result,
    audit::{TrustedLog, VerifyingKey, proto::LogIdentity},
    envelope,
};
use prost::Message;
use rusqlite::Connection;
#[cfg(not(target_arch = "wasm32"))]
use rusqlite::OpenFlags;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditBinding {
    pub log_id: String,
    pub public_key: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditBindingState {
    Uninitialized,
    Unbound,
    Bound(AuditBinding),
    LegacySealed,
}
pub(super) const SCHEMA:&str="
CREATE TABLE audit_identity (slot INTEGER PRIMARY KEY CHECK(slot=1), payload BLOB NOT NULL) STRICT;
CREATE TRIGGER identity_no_update BEFORE UPDATE ON audit_identity BEGIN SELECT RAISE(ABORT,'audit identity'); END;
CREATE TRIGGER identity_no_delete BEFORE DELETE ON audit_identity BEGIN SELECT RAISE(ABORT,'audit identity'); END;";
fn read(connection: &Connection) -> Result<Option<AuditBinding>> {
    let mut statement = sql(connection.prepare("SELECT slot,payload FROM audit_identity"))?;
    let mut rows = sql(statement.query([]))?;
    let Some(row) = sql(rows.next())? else {
        return Ok(None);
    };
    if sql(row.get::<_, i64>(0))? != 1 {
        return Err(Error::Integrity);
    }
    let bytes = sql(row.get_ref(1))?
        .as_blob()
        .map_err(|_| Error::Integrity)?;
    let raw = envelope::unpack(b"MORROWI1", bytes, 1024)?;
    let value = LogIdentity::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
    crate::identity(&value.log_id)?;
    let key: [u8; 32] = value.public_key.try_into().map_err(|_| Error::Integrity)?;
    if value.version != 1
        || VerifyingKey::from_bytes(&key)
            .map_err(|_| Error::Integrity)?
            .is_weak()
        || sql(rows.next())?.is_some()
    {
        return Err(Error::Integrity);
    }
    Ok(Some(AuditBinding {
        log_id: value.log_id,
        public_key: key,
    }))
}
pub(super) fn verify(connection: &Connection, trust: Option<&TrustedLog>) -> Result<()> {
    let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    if version < 6 {
        return Ok(());
    } // Legacy verification runs before atomic migration.
    if let Some(binding) = read(connection)? {
        let trust = trust.ok_or(Error::Invalid("audit trust required"))?;
        if binding.log_id != trust.id || binding.public_key != *trust.key.as_bytes() {
            return Err(Error::Integrity);
        }
    } else {
        let sealed: i64 =
            sql(connection.query_row("SELECT count(*) FROM sealed_segments", [], |r| r.get(0)))?;
        if sealed != 0 {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}
pub(super) fn bind(connection: &Connection, trust: &TrustedLog) -> Result<()> {
    crate::identity(&trust.id)?;
    if trust.key.is_weak() {
        return Err(Error::Invalid("weak audit key"));
    }
    if read(connection)?.is_some() {
        return verify(connection, Some(trust));
    }
    let raw = LogIdentity {
        version: 1,
        log_id: trust.id.clone(),
        public_key: trust.key.as_bytes().to_vec(),
    }
    .encode_to_vec();
    let bytes = envelope::pack(b"MORROWI1", &raw, 1024)?;
    sql(connection.execute(
        "INSERT INTO audit_identity(slot,payload) VALUES(1,?1)",
        [bytes],
    ))?;
    Ok(())
}
#[cfg(not(target_arch = "wasm32"))]
impl Store {
    /// Read-only classification for initialization. Returned public data never grants
    /// trust: compare it to a separately protected key before opening the database.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn audit_binding_status(path: &std::path::Path) -> Result<AuditBindingState> {
        let connection = sql(Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ))?;
        let snapshot = sql(connection.unchecked_transaction())?;
        let app: i64 = sql(snapshot.query_row("PRAGMA application_id", [], |r| r.get(0)))?;
        let version: i64 = sql(snapshot.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
        if app == 0 && version == 0 {
            let count: i64 = sql(snapshot.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            ))?;
            if count == 0 {
                return Ok(AuditBindingState::Uninitialized);
            }
        }
        if app != APPLICATION_ID || !matches!(version, 4..=11) {
            return Err(Error::UnsupportedVersion);
        }
        if version >= 6
            && let Some(binding) = read(&snapshot)?
        {
            return Ok(AuditBindingState::Bound(binding));
        }
        if version >= 5 {
            let sealed: i64 =
                sql(snapshot.query_row("SELECT count(*) FROM sealed_segments", [], |r| r.get(0)))?;
            if sealed > 0 {
                return if version == 5 {
                    Ok(AuditBindingState::LegacySealed)
                } else {
                    Err(Error::Integrity)
                };
            }
        }
        Ok(AuditBindingState::Unbound)
    }
}
