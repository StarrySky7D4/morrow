//! Core-local immutable signed evidence and pending-queue confirmation share one transaction.
use super::{Store, boundary, sql};
use crate::{
    Error, Result,
    audit::{self, ChainVerifier, TrustedLog, VerifiedSegment},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
pub(super) const SCHEMA: &str = "
CREATE TABLE sealed_segments (segment_index INTEGER PRIMARY KEY CHECK(segment_index>0), payload BLOB NOT NULL) STRICT;
CREATE TABLE operation_events (sequence INTEGER PRIMARY KEY CHECK(sequence>0), id TEXT NOT NULL UNIQUE REFERENCES operations(id), seal_index INTEGER REFERENCES sealed_segments(segment_index)) STRICT;
CREATE INDEX operation_seal ON operation_events(seal_index);
CREATE TRIGGER sealed_no_update BEFORE UPDATE ON sealed_segments BEGIN SELECT RAISE(ABORT,'sealed'); END;
CREATE TRIGGER sealed_no_delete BEFORE DELETE ON sealed_segments BEGIN SELECT RAISE(ABORT,'sealed'); END;";
fn number(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::Limit)
}
fn checked<T>(v: audit::Result<T>) -> Result<T> {
    v.map_err(|_| Error::Integrity)
}
fn stored(
    connection: &Connection,
    index: i64,
    trust: &TrustedLog,
) -> Result<Option<VerifiedSegment>> {
    let mut statement =
        sql(connection.prepare("SELECT payload FROM sealed_segments WHERE segment_index=?1"))?;
    let mut rows = sql(statement.query([index]))?;
    let Some(row) = sql(rows.next())? else {
        return Ok(None);
    };
    let bytes = sql(row.get_ref(0))?
        .as_blob()
        .map_err(|_| Error::Integrity)?;
    if bytes.len() > audit::MAX_CONTAINER_BYTES {
        return Err(Error::Limit);
    }
    let verified = checked(audit::verify(bytes, trust))?;
    if verified.segment().index != index as u64 {
        return Err(Error::Integrity);
    }
    Ok(Some(verified))
}
fn match_events(connection: &Connection, segment: &VerifiedSegment, sealed: bool) -> Result<()> {
    let mut statement = sql(connection.prepare("SELECT e.id,e.seal_index,o.payload,p.payload,p.sequence FROM operation_events e JOIN operations o ON o.id=e.id LEFT JOIN outbox p ON p.id=e.id WHERE e.sequence=?1"))?;
    for event in &segment.segment().events {
        let mut rows = sql(statement.query([number(event.sequence)?]))?;
        let row = sql(rows.next())?.ok_or(Error::Integrity)?;
        let id: String = sql(row.get(0))?;
        let seal: Option<i64> = sql(row.get(1))?;
        let raw = sql(row.get_ref(2))?
            .as_blob()
            .map_err(|_| Error::Integrity)?;
        if id != event.operation_id || raw != event.original_commit {
            return Err(Error::Integrity);
        }
        let pending = sql(row.get_ref(3))?;
        if sealed {
            if seal != Some(number(segment.segment().index)?)
                || !matches!(pending, rusqlite::types::ValueRef::Null)
            {
                return Err(Error::Integrity);
            }
        } else if seal.is_some()
            || pending.as_blob().map_err(|_| Error::Integrity)? != raw
            || sql(row.get::<_, i64>(4))? != number(event.sequence)?
        {
            return Err(Error::Integrity);
        }
    }
    Ok(())
}
pub(super) fn verify(connection: &Connection, trust: Option<&TrustedLog>) -> Result<()> {
    // Every operation retains exactly one durable sequence, irrespective of queue state.
    let missing: i64 = sql(connection.query_row("SELECT count(*) FROM operations o LEFT JOIN operation_events e ON e.id=o.id WHERE e.id IS NULL", [], |r| r.get(0)))?;
    let orphans: i64 = sql(connection.query_row("SELECT count(*) FROM operation_events e LEFT JOIN operations o ON o.id=e.id LEFT JOIN sealed_segments s ON s.segment_index=e.seal_index WHERE o.id IS NULL OR (e.seal_index IS NOT NULL AND s.segment_index IS NULL)", [], |r| r.get(0)))?;
    let (count, maximum): (i64, i64) = sql(connection.query_row(
        "SELECT count(*),coalesce(max(sequence),0) FROM operation_events",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ))?;
    let invalid_pending: i64 = sql(connection.query_row("SELECT count(*) FROM outbox p LEFT JOIN operation_events e ON e.id=p.id LEFT JOIN operations o ON o.id=p.id WHERE e.id IS NULL OR e.seal_index IS NOT NULL OR e.sequence!=p.sequence OR o.payload!=p.payload", [], |r| r.get(0)))?;
    let absent_pending: i64 = sql(connection.query_row("SELECT count(*) FROM operation_events e LEFT JOIN outbox p ON p.id=e.id WHERE e.seal_index IS NULL AND p.id IS NULL", [], |r| r.get(0)))?;
    if missing != 0
        || orphans != 0
        || count != maximum
        || invalid_pending != 0
        || absent_pending != 0
    {
        return Err(Error::Integrity);
    }
    let sealed_count: i64 = sql(connection.query_row(
        "SELECT count(*) FROM operation_events WHERE seal_index IS NOT NULL",
        [],
        |r| r.get(0),
    ))?;
    let segment_count: i64 =
        sql(connection.query_row("SELECT count(*) FROM sealed_segments", [], |r| r.get(0)))?;
    if segment_count == 0 {
        return if sealed_count == 0 {
            Ok(())
        } else {
            Err(Error::Integrity)
        };
    }
    let trust = trust.ok_or(Error::Invalid("audit trust required"))?;
    let mut chain = checked(ChainVerifier::new(trust.clone(), None))?;
    let mut statement = sql(
        connection.prepare("SELECT segment_index FROM sealed_segments ORDER BY segment_index")
    )?;
    let mut rows = sql(statement.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let index: i64 = sql(row.get(0))?;
        let segment = stored(connection, index, trust)?.ok_or(Error::Integrity)?;
        checked(chain.accept(segment.container()))?;
        match_events(connection, &segment, true)?;
    }
    let (_, events) = checked(chain.finish())?;
    if number(events)? != sealed_count {
        return Err(Error::Integrity);
    }
    Ok(())
}
impl Store {
    /// Host-only API. Verify with the independently pinned identity, persist the
    /// signed original locally, bind its events, then confirm the queue atomically.
    /// No signing key or mutation entrypoint is exposed to plugins.
    pub fn seal_pending(&mut self, bytes: &[u8]) -> Result<bool> {
        let trust = self
            .audit_trust
            .as_ref()
            .ok_or(Error::Invalid("audit trust required"))?;
        let segment = checked(audit::verify(bytes, trust))?;
        let index = number(segment.segment().index)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if let Some(existing) = stored(&tx, index, trust)? {
            if existing.digest() != segment.digest() {
                return Err(Error::OperationConflict);
            }
            match_events(&tx, &existing, true)?;
            return Ok(false); // already sealed; never regenerate original bytes
        }
        let tail: Option<i64> = sql(tx
            .query_row(
                "SELECT segment_index FROM sealed_segments ORDER BY segment_index DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional())?;
        let (next_index, next_sequence, previous) = match tail {
            Some(i) => {
                let previous = stored(&tx, i, trust)?.ok_or(Error::Integrity)?;
                (
                    previous.segment().index.checked_add(1),
                    previous
                        .segment()
                        .events
                        .last()
                        .unwrap()
                        .sequence
                        .checked_add(1),
                    previous.digest(),
                )
            }
            None => (Some(1), Some(1), [0; 32]),
        };
        if next_index != Some(segment.segment().index)
            || next_sequence != Some(segment.segment().events[0].sequence)
            || previous.as_slice() != segment.segment().previous_sha256
        {
            return Err(Error::Integrity);
        }
        match_events(&tx, &segment, false)?;
        sql(tx.execute(
            "INSERT INTO sealed_segments(segment_index,payload) VALUES(?1,?2)",
            params![index, bytes],
        ))?;
        boundary("seal-after-segment");
        for event in &segment.segment().events {
            let sequence = number(event.sequence)?;
            if sql(tx.execute("UPDATE operation_events SET seal_index=?1 WHERE sequence=?2 AND id=?3 AND seal_index IS NULL", params![index,sequence,event.operation_id]))? != 1 { return Err(Error::Integrity); }
            boundary("seal-after-link");
            if sql(tx.execute(
                "DELETE FROM outbox WHERE sequence=?1 AND id=?2",
                params![sequence, event.operation_id],
            ))? != 1
            {
                return Err(Error::Integrity);
            }
            boundary("seal-after-confirm");
        }
        boundary("seal-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("seal-after-commit");
        Ok(true)
    }
    /// Latest immutable signed original for bounded host sealer resume.
    pub fn last_sealed_segment(&self) -> Result<Option<Vec<u8>>> {
        let index: Option<i64> = sql(self
            .connection
            .query_row(
                "SELECT segment_index FROM sealed_segments ORDER BY segment_index DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional())?;
        index
            .map(|i| self.sealed_segment(i as u64)?.ok_or(Error::Integrity))
            .transpose()
    }
    /// Original signed container for recovery/export. Requires pinned trust on open.
    pub fn sealed_segment(&self, index: u64) -> Result<Option<Vec<u8>>> {
        let trust = self
            .audit_trust
            .as_ref()
            .ok_or(Error::Invalid("audit trust required"))?;
        let value = stored(&self.connection, number(index)?, trust)?;
        if let Some(v) = &value {
            match_events(&self.connection, v, true)?;
        }
        Ok(value.map(|v| v.container().to_vec()))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    #[test]
    fn full_archive_write_preserves_pending_original_and_capacity() {
        let directory = tempfile::tempdir().unwrap();
        let key = audit::SigningKey::from_bytes(&[7; 32]);
        let trust = TrustedLog {
            id: "disk-full-test".into(),
            key: key.verifying_key(),
        };
        let mut store = Store::open_audited(
            &directory.path().join("db"),
            super::super::EventBudget::default(),
            true,
            trust.clone(),
        )
        .unwrap();
        let mut seed = 0x12345678u32;
        let body = (0..128 * 1024)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        store
            .create_local(
                "create",
                &crate::content::CardRecord::new("card", "type", 1, "", body).unwrap(),
            )
            .unwrap();
        let before = store.pending(0, 10).unwrap();
        let signed = audit::sign(
            &audit::from_pending(&trust, 1, [0; 32], &before).unwrap(),
            &trust,
            &key,
        )
        .unwrap();
        let pages: i64 = store
            .connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        store
            .connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        assert_eq!(store.seal_pending(&signed), Err(Error::StorageFull));
        assert_eq!(store.pending(0, 10).unwrap(), before);
        assert!(store.sealed_segment(1).unwrap().is_none());
        store.integrity_check().unwrap();
    }
}
