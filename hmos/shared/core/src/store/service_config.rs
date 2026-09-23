//! Desired service configuration in the original Store. Loading a row restores
//! no listener, principal, resource, content grant, or executable instance.
use super::{Store, boundary, byte_room, sql};
use crate::{
    Error, Result, identity,
    service_config::{Config, MAX_CONFIGS, MAX_CONTAINER_BYTES},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
/// A host-local snapshot of desired configuration, never restored authority.
/// No Debug: configuration and opaque approval references stay out of logs.
pub struct ServiceConfigPage {
    pub configs: Vec<Config>,
    pub snapshot: [u8; 32],
    pub next: Option<String>,
}
pub(super) const SCHEMA: &str = "CREATE TABLE service_configs(id TEXT PRIMARY KEY,revision INTEGER NOT NULL CHECK(revision>0),namespace BLOB NOT NULL UNIQUE CHECK(length(namespace)=32),payload BLOB NOT NULL) STRICT";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
pub(super) fn accounted(c: &Connection) -> Result<u64> {
    if version(c)? < 18 {
        return Ok(0);
    }
    let bytes: i64 = sql(c.query_row(
        "SELECT coalesce(sum(length(payload)),0) FROM service_configs",
        [],
        |r| r.get(0),
    ))?;
    u64::try_from(bytes).map_err(|_| Error::Integrity)
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let stored: Option<(String, String)> = sql(c
        .query_row(
            "SELECT type,sql FROM sqlite_schema WHERE name='service_configs'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    if version(c)? < 18 {
        return if stored.is_none() {
            Ok(())
        } else {
            Err(Error::Integrity)
        };
    }
    if stored != Some(("table".into(), SCHEMA.into())) {
        return Err(Error::Integrity);
    }
    let mut statement =
        sql(c.prepare("SELECT id,revision,namespace,payload FROM service_configs LIMIT ?1"))?;
    let mut rows = sql(statement.query([MAX_CONFIGS as i64 + 1]))?;
    let mut count = 0;
    while let Some(row) = sql(rows.next())? {
        count += 1;
        if count > MAX_CONFIGS {
            return Err(Error::Limit);
        }
        decode_row(row)?;
    }
    Ok(())
}
fn decode_row(row: &rusqlite::Row<'_>) -> Result<Config> {
    let id_ref = sql(row.get_ref(0))?;
    let id = id_ref.as_str().map_err(|_| Error::Integrity)?;
    let revision: i64 = sql(row.get(1))?;
    let namespace_ref = sql(row.get_ref(2))?;
    let namespace = namespace_ref.as_blob().map_err(|_| Error::Integrity)?;
    let payload_ref = sql(row.get_ref(3))?;
    let payload = payload_ref.as_blob().map_err(|_| Error::Integrity)?;
    if id.is_empty() || id.len() > 256 || revision <= 0 || namespace.len() != 32 {
        return Err(Error::Integrity);
    }
    if payload.len() > MAX_CONTAINER_BYTES {
        return Err(Error::Limit);
    }
    let config = Config::decode(payload)?;
    if config.value().id != id
        || config.value().revision != revision as u64
        || config.value().namespace != namespace
    {
        return Err(Error::Integrity);
    }
    Ok(config)
}
fn load(c: &Connection, id: &str) -> Result<Option<Config>> {
    identity(id)?;
    if version(c)? < 18 {
        return Ok(None);
    }
    let mut statement =
        sql(c.prepare("SELECT id,revision,namespace,payload FROM service_configs WHERE id=?1"))?;
    let mut rows = sql(statement.query([id]))?;
    sql(rows.next())?.map(decode_row).transpose()
}
impl Store {
    /// Stable host-local pagination over at most 128 validated configurations.
    /// All rows are checked inside one read snapshot, including rows outside the
    /// returned page. Continuation requires an exact known cursor and unchanged
    /// full-container snapshot. Reading neither writes nor pins/revokes authority.
    pub fn list_service_configs_local(
        &mut self,
        after: Option<&str>,
        expected_snapshot: Option<[u8; 32]>,
        limit: u32,
    ) -> Result<ServiceConfigPage> {
        if limit != 1 {
            return Err(Error::Limit);
        }
        if let Some(cursor) = after {
            identity(cursor)?;
            if expected_snapshot.is_none() {
                return Err(Error::Invalid("service config snapshot required"));
            }
        }
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred))?;
        if !matches!(version(&tx)?, 18..=super::SCHEMA_VERSION) {
            return Err(Error::UnsupportedVersion);
        }
        // SQLite guards raw lengths/types before exposing any oversized values
        // to the decoder or allocating owned business metadata.
        let mut statement = sql(tx.prepare(
            "SELECT CASE WHEN typeof(id)='text' AND length(CAST(id AS BLOB)) BETWEEN 1 AND 256 THEN id ELSE NULL END,CASE WHEN typeof(revision)='integer' AND revision>0 THEN revision ELSE NULL END,CASE WHEN typeof(namespace)='blob' AND length(namespace)=32 THEN namespace ELSE NULL END,CASE WHEN typeof(payload)='blob' AND length(payload)<=?2 THEN payload ELSE NULL END FROM service_configs ORDER BY id LIMIT ?1",
        ))?;
        let mut rows =
            sql(statement.query(params![MAX_CONFIGS as i64 + 1, MAX_CONTAINER_BYTES as i64]))?;
        let mut hash = Sha256::new();
        hash.update(b"Morrow/service-config/list-snapshot/v1\0");
        let mut configs = Vec::with_capacity(1);
        let mut count = 0u64;
        let mut found_cursor = after.is_none();
        let mut previous: Option<String> = None;
        let mut more = false;
        while let Some(row) = sql(rows.next())? {
            count += 1;
            if count > MAX_CONFIGS as u64 {
                return Err(Error::Limit);
            }
            let config = decode_row(row)?;
            let id = &config.value().id;
            if previous.as_ref().is_some_and(|old| old >= id) {
                return Err(Error::Integrity);
            }
            previous = Some(id.clone());
            hash.update((id.len() as u64).to_le_bytes());
            hash.update(id.as_bytes());
            hash.update(config.value().revision.to_le_bytes());
            hash.update(Sha256::digest(config.container()));
            if after == Some(id.as_str()) {
                found_cursor = true;
            }
            if after.is_none_or(|cursor| id.as_str() > cursor) {
                if configs.is_empty() {
                    configs.push(config);
                } else {
                    more = true;
                }
            }
        }
        hash.update(count.to_le_bytes());
        let snapshot: [u8; 32] = hash.finalize().into();
        if expected_snapshot.is_some_and(|expected| expected != snapshot) {
            return Err(Error::RevisionConflict);
        }
        if !found_cursor {
            return Err(Error::Invalid("service config cursor"));
        }
        let next = if more {
            configs.last().map(|v| v.value().id.clone())
        } else {
            None
        };
        Ok(ServiceConfigPage {
            configs,
            snapshot,
            next,
        })
    }
    /// Host-only desired state. Expected zero creates revision one; all later
    /// writes require exact previous revision. Disable via a new revision;
    /// deletion/recycling of historical namespaces is deliberately unsupported.
    /// The caller must separately resolve approvals and obtain fresh grants.
    pub fn save_service_config_local(
        &mut self,
        config: &Config,
        expected_revision: u64,
    ) -> Result<()> {
        let value = config.value();
        if expected_revision.checked_add(1) != Some(value.revision) {
            return Err(Error::RevisionConflict);
        }
        let _writer = self.service_authority_coordinator.writer()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if version(&tx)? < 18 {
            return Err(Error::UnsupportedVersion);
        }
        let previous = load(&tx, &value.id)?;
        if previous.as_ref().map_or(0, |old| old.value().revision) != expected_revision {
            return Err(Error::RevisionConflict);
        }
        if let Some(old) = &previous {
            if old.value().namespace != value.namespace || old.value().service != value.service {
                return Err(Error::OperationConflict);
            }
        } else {
            let count: i64 =
                sql(tx.query_row("SELECT count(*) FROM service_configs", [], |r| r.get(0)))?;
            if count >= MAX_CONFIGS as i64 {
                return Err(Error::Limit);
            }
            let reused: bool = sql(tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM service_configs WHERE namespace=?1)",
                [value.namespace.as_slice()],
                |r| r.get(0),
            ))?;
            if reused {
                return Err(Error::OperationConflict);
            }
        }
        self.service_authority_coordinator
            .control()
            .revoke_resource(&super::ServiceAuthorityResource::Configuration(
                value.id.clone(),
            ));
        // Remove the old row only inside the atomic transaction so quota counts
        // replacements once, including updates that shrink an already full DB.
        if previous.is_some() {
            sql(tx.execute(
                "DELETE FROM service_configs WHERE id=?1 AND revision=?2",
                params![value.id, expected_revision as i64],
            ))?;
        }
        byte_room(&tx, self.budget, config.container().len() as u64)?;
        sql(tx.execute(
            "INSERT INTO service_configs(id,revision,namespace,payload) VALUES(?1,?2,?3,?4)",
            params![
                value.id,
                value.revision as i64,
                value.namespace,
                config.container()
            ],
        ))?;
        boundary("service-config-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("service-config-after-commit");
        Ok(())
    }
    pub fn load_service_config(&self, id: &str) -> Result<Option<Config>> {
        load(&self.connection, id)
    }
    /// Bounded keyset page. Rows are desired configuration, not runtime status.
    pub fn list_service_configs(&self, after_id: Option<&str>, limit: u32) -> Result<Vec<Config>> {
        if limit == 0 || limit > 32 {
            return Err(Error::Limit);
        }
        if let Some(id) = after_id {
            identity(id)?;
        }
        if version(&self.connection)? < 18 {
            return Ok(Vec::new());
        }
        let mut statement = sql(self.connection.prepare("SELECT id,revision,namespace,payload FROM service_configs WHERE id>?1 ORDER BY id LIMIT ?2"))?;
        let mut rows = sql(statement.query(params![after_id.unwrap_or(""), limit]))?;
        let mut result = Vec::new();
        while let Some(row) = sql(rows.next())? {
            result.push(decode_row(row)?);
        }
        Ok(result)
    }
}
