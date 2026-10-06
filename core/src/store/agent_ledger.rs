//! Host-local durable ledger, sharing the original SQLite library and byte quota.
//! Strict CAS never treats an already committed claim as a fresh successful claim.
use super::{Store, boundary, byte_room, sql};
use crate::{
    Error, Result,
    agent_ledger::{MAX_CONTAINER_BYTES, Record},
    identity,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub const MAX_RECORDS: usize = 128;
pub const MAX_MUTATIONS: usize = MAX_RECORDS + 1;
const MAX_TRANSACTION_BYTES: u64 = 64 * 1024 * 1024;
/// Trusted-local atomic changes, bound to the original ledger owner lease.
/// Delete compares the complete original container, not only its revision.
#[derive(Clone)]
pub enum AgentLedgerMutation {
    Put {
        record: Record,
        expected_revision: u64,
    },
    Delete {
        expected: Record,
    },
}
pub(super) const SCHEMA: &str = "CREATE TABLE agent_ledger(domain BLOB NOT NULL CHECK(length(domain)=32),id TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0),payload BLOB NOT NULL,PRIMARY KEY(domain,id)) STRICT";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |row| row.get(0)))
}
pub(super) fn accounted(c: &Connection) -> Result<u64> {
    if version(c)? < 25 {
        return Ok(0);
    }
    // Keep the v25 schema and every existing container unchanged. Quota uses a
    // bounded verified scan rather than a new mutable accounting column that
    // could disagree with opaque payloads or undercharge compressible padding.
    let mut statement = sql(c.prepare(&format!("SELECT {COLUMNS} FROM agent_ledger LIMIT ?2")))?;
    let mut rows =
        sql(statement.query(params![MAX_CONTAINER_BYTES as i64, MAX_RECORDS as i64 + 1]))?;
    let mut total = 0u64;
    let mut count = 0usize;
    while let Some(row) = sql(rows.next())? {
        count += 1;
        if count > MAX_RECORDS {
            return Err(Error::Limit);
        }
        total = total
            .checked_add(decode_row(row)?.retained_bytes())
            .ok_or(Error::Limit)?;
    }
    Ok(total)
}

fn check_schema(c: &Connection) -> Result<()> {
    let stored: Option<(String, String)> = sql(c
        .query_row(
            "SELECT type,sql FROM sqlite_schema WHERE name='agent_ledger'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional())?;
    if version(c)? < 25 {
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
const COLUMNS: &str = "CASE WHEN typeof(domain)='blob' AND length(domain)=32 THEN domain ELSE NULL END,CASE WHEN typeof(id)='text' AND length(CAST(id AS BLOB))<=256 THEN id ELSE NULL END,revision,CASE WHEN typeof(payload)='blob' AND length(payload)<=?1 THEN payload ELSE NULL END";
fn decode_row(row: &rusqlite::Row<'_>) -> Result<Record> {
    let domain_ref = sql(row.get_ref(0))?;
    let domain = domain_ref.as_blob().map_err(|_| Error::Integrity)?;
    let id_ref = sql(row.get_ref(1))?;
    let id = id_ref.as_str().map_err(|_| Error::Integrity)?;
    let revision = sql(row.get_ref(2))?
        .as_i64()
        .map_err(|_| Error::Integrity)?;
    let payload_ref = sql(row.get_ref(3))?;
    let payload = payload_ref.as_blob().map_err(|_| Error::Integrity)?;
    let record = Record::decode(payload)?;
    if domain != record.domain()
        || id != record.id()
        || revision <= 0
        || revision as u64 != record.revision()
    {
        return Err(Error::Integrity);
    }
    Ok(record)
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    check_schema(c)?;
    if version(c)? < 25 {
        return Ok(());
    }
    let mut statement = sql(c.prepare(&format!("SELECT {COLUMNS} FROM agent_ledger LIMIT ?2")))?;
    let mut rows =
        sql(statement.query(params![MAX_CONTAINER_BYTES as i64, MAX_RECORDS as i64 + 1]))?;
    let mut count = 0;
    while let Some(row) = sql(rows.next())? {
        count += 1;
        if count > MAX_RECORDS {
            return Err(Error::Limit);
        }
        decode_row(row)?;
    }
    Ok(())
}
fn load(c: &Connection, domain: &[u8; 32], id: &str) -> Result<Option<Record>> {
    identity(id)?;
    if *domain == [0; 32] {
        return Err(Error::Invalid("agent ledger domain"));
    }
    check_schema(c)?;
    if version(c)? < 25 {
        return Ok(None);
    }
    let mut statement = sql(c.prepare(&format!(
        "SELECT {COLUMNS} FROM agent_ledger WHERE domain=?2 AND id=?3"
    )))?;
    let mut rows =
        sql(statement.query(params![MAX_CONTAINER_BYTES as i64, domain.as_slice(), id]))?;
    sql(rows.next())?.map(decode_row).transpose()
}
impl Store {
    /// Bounded identities only. Callers load and validate each selected payload
    /// individually; this avoids allocating every large body merely to enumerate.
    pub fn agent_ledger_ids_local(&self, domain: &[u8; 32]) -> Result<Vec<String>> {
        if *domain == [0; 32] {
            return Err(Error::Invalid("agent ledger domain"));
        }
        check_schema(&self.connection)?;
        if version(&self.connection)? < 25 {
            return Ok(vec![]);
        }
        let mut statement = sql(self.connection.prepare(
            "SELECT CASE WHEN typeof(id)='text' AND length(CAST(id AS BLOB))<=256 THEN id ELSE NULL END FROM agent_ledger WHERE domain=?1 ORDER BY id LIMIT ?2"
        ))?;
        let mut rows = sql(statement.query(params![domain.as_slice(), MAX_RECORDS as i64 + 1]))?;
        let mut ids = Vec::new();
        while let Some(row) = sql(rows.next())? {
            if ids.len() >= MAX_RECORDS {
                return Err(Error::Limit);
            }
            let id: String = sql(row.get(0))?;
            identity(&id)?;
            ids.push(id);
        }
        Ok(ids)
    }
    /// All expected rows are checked before any changes. A quota/CAS failure
    /// rolls every mutation back; any commit error remains CommitUnknown.
    /// Historical reload never proves a new claim or live deletion authority.
    pub fn batch_agent_ledger_local(
        &mut self,
        owner: &super::AgentLedgerOwnerLease,
        mutations: &[AgentLedgerMutation],
    ) -> Result<()> {
        self.validate_agent_ledger_owner(owner)?;
        if mutations.is_empty() || mutations.len() > MAX_MUTATIONS {
            return Err(Error::Limit);
        }
        let mut keys = std::collections::BTreeSet::new();
        let mut admitted = 0u64;
        for mutation in mutations {
            let record = match mutation {
                AgentLedgerMutation::Put {
                    record,
                    expected_revision,
                } => {
                    if expected_revision.checked_add(1) != Some(record.revision()) {
                        return Err(Error::RevisionConflict);
                    }
                    record
                }
                AgentLedgerMutation::Delete { expected } => expected,
            };
            if !keys.insert((record.domain(), record.id())) {
                return Err(Error::Invalid("duplicate ledger mutation"));
            }
            admitted = admitted
                .checked_add(record.retained_bytes())
                .ok_or(Error::Limit)?;
            if admitted > MAX_TRANSACTION_BYTES {
                return Err(Error::Limit);
            }
        }
        let _writer = self.service_authority_coordinator.writer()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if !(25..=super::SCHEMA_VERSION).contains(&version(&tx)?) {
            return Err(Error::UnsupportedVersion);
        }
        self.service_authority_coordinator
            .validate_ledger_owner(owner)?;
        check_schema(&tx)?;
        let count: i64 =
            sql(tx.query_row("SELECT count(*) FROM agent_ledger", [], |row| row.get(0)))?;
        if count > MAX_RECORDS as i64 {
            return Err(Error::Limit);
        }
        // Verify every existing ledger container, including rows this batch
        // does not touch. Paid reservation updates must not bypass corruption.
        let old_bytes = accounted(&tx)?;
        let mut removed = 0i64;
        let mut removed_bytes = 0u64;
        let mut puts = 0i64;
        let mut new_bytes = 0u64;
        for mutation in mutations {
            let (candidate, expected_revision) = match mutation {
                AgentLedgerMutation::Put {
                    record,
                    expected_revision,
                } => (record, *expected_revision),
                AgentLedgerMutation::Delete { expected } => (expected, expected.revision()),
            };
            let previous = load(&tx, &candidate.domain(), candidate.id())?;
            if previous.as_ref().map_or(0, Record::revision) != expected_revision {
                return Err(Error::RevisionConflict);
            }
            if let AgentLedgerMutation::Delete { expected } = mutation {
                if previous.as_ref().map(Record::container) != Some(expected.container()) {
                    return Err(Error::RevisionConflict);
                }
            } else {
                puts += 1;
                new_bytes = new_bytes
                    .checked_add(candidate.retained_bytes())
                    .ok_or(Error::Limit)?;
            }
            if let Some(previous) = previous {
                removed += 1;
                removed_bytes = removed_bytes
                    .checked_add(previous.retained_bytes())
                    .ok_or(Error::Limit)?;
            }
        }
        let new_count = count - removed + puts;
        if new_count > MAX_RECORDS as i64 {
            return Err(Error::Limit);
        }
        let new_total = old_bytes
            .checked_sub(removed_bytes)
            .and_then(|remaining| remaining.checked_add(new_bytes))
            .ok_or(Error::Integrity)?;
        let paid_reservation = new_count <= count && new_total <= old_bytes;
        if paid_reservation {
            // Reopening with a lower quota cannot prevent a same-sized marker
            // or shrinking cleanup from using its previously paid reservation.
            // Still run the complete shared-family accounting before changes.
            byte_room(
                &tx,
                super::EventBudget {
                    max_bytes: u64::MAX,
                    ..self.budget
                },
                0,
            )?;
        }
        for mutation in mutations {
            let (candidate, expected_revision) = match mutation {
                AgentLedgerMutation::Put {
                    record,
                    expected_revision,
                } => (record, *expected_revision),
                AgentLedgerMutation::Delete { expected } => (expected, expected.revision()),
            };
            if expected_revision != 0 {
                if sql(tx.execute(
                    "DELETE FROM agent_ledger WHERE domain=?1 AND id=?2 AND revision=?3",
                    params![
                        candidate.domain().as_slice(),
                        candidate.id(),
                        expected_revision as i64
                    ],
                ))? != 1
                {
                    return Err(Error::RevisionConflict);
                }
            }
        }
        if !paid_reservation {
            byte_room(&tx, self.budget, new_bytes)?;
        }
        for mutation in mutations {
            if let AgentLedgerMutation::Put { record, .. } = mutation {
                sql(tx.execute(
                    "INSERT INTO agent_ledger(domain,id,revision,payload) VALUES(?1,?2,?3,?4)",
                    params![
                        record.domain().as_slice(),
                        record.id(),
                        record.revision() as i64,
                        record.container()
                    ],
                ))?;
            }
        }
        let _commit = self
            .service_authority_coordinator
            .ledger_commit_guard(owner)?;
        boundary("agent-ledger-batch-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("agent-ledger-batch-after-commit");
        Ok(())
    }
    /// Host-owned durable history only; no plugin transport or live grant is
    /// introduced. Historical read-only Stores before v25 return absence.
    pub fn load_agent_ledger_local(&self, domain: &[u8; 32], id: &str) -> Result<Option<Record>> {
        load(&self.connection, domain, id)
    }
    /// Expected zero inserts revision one. Every later write requires exactly
    /// current+1. Replaying an identical committed candidate conflicts: a claim
    /// must never be acquired twice through an idempotent-success shortcut.
    /// All commit errors retain CommitUnknown; callers must reconcile history
    /// and must not infer a new execution permission from a successful reload.
    pub fn compare_exchange_agent_ledger_local(
        &mut self,
        record: &Record,
        expected_revision: u64,
    ) -> Result<()> {
        if expected_revision.checked_add(1) != Some(record.revision()) {
            return Err(Error::RevisionConflict);
        }
        // Generic local writes cannot bypass an active profile owner's registry
        // or its execution fence. Legacy service-only leases retain their rules.
        let _unowned = self
            .service_authority_coordinator
            .legacy_ledger_write_guard()?;
        let _writer = self.service_authority_coordinator.writer()?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if !(25..=super::SCHEMA_VERSION).contains(&version(&tx)?) {
            return Err(Error::UnsupportedVersion);
        }
        let previous = load(&tx, &record.domain(), record.id())?;
        if previous.as_ref().map_or(0, Record::revision) != expected_revision {
            return Err(Error::RevisionConflict);
        }
        let count: i64 =
            sql(tx.query_row("SELECT count(*) FROM agent_ledger", [], |row| row.get(0)))?;
        if count > MAX_RECORDS as i64 || (previous.is_none() && count >= MAX_RECORDS as i64) {
            return Err(Error::Limit);
        }
        if previous.is_some() {
            let removed = sql(tx.execute(
                "DELETE FROM agent_ledger WHERE domain=?1 AND id=?2 AND revision=?3",
                params![
                    record.domain().as_slice(),
                    record.id(),
                    expected_revision as i64
                ],
            ))?;
            if removed != 1 {
                return Err(Error::RevisionConflict);
            }
        }
        // Charge reserved worst-case bytes after releasing the old row in this
        // transaction. Capacity failure rolls the entire replacement back.
        byte_room(&tx, self.budget, record.retained_bytes())?;
        sql(tx.execute(
            "INSERT INTO agent_ledger(domain,id,revision,payload) VALUES(?1,?2,?3,?4)",
            params![
                record.domain().as_slice(),
                record.id(),
                record.revision() as i64,
                record.container()
            ],
        ))?;
        boundary("agent-ledger-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("agent-ledger-after-commit");
        Ok(())
    }
}
