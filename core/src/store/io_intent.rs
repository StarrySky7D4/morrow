//! Immutable IO command histories in the single Store writer and signed outbox.
//! A persisted fact is NOT a dispatch permit. Protected evidence admission and
//! live broker authorization must still be integrated before external IO exists.
use super::{Store, boundary, sql};
use crate::{
    Error, Result, identity,
    io_intent::{MAX_CONTAINER_BYTES, Phase, Record},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(super) const SCHEMA: &str = "CREATE TABLE io_intents(operation_id TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision BETWEEN 1 AND 3),event_id TEXT NOT NULL UNIQUE REFERENCES operations(id),PRIMARY KEY(operation_id,revision)) STRICT;";
/// Pre-dispatch reservations are part of the logical event budget: they are
/// deducted from the capacity every other writer may consume and can therefore
/// never be crowded out. The schema pins the reservation to exactly one event
/// bounded by the largest possible record container.
pub(super) fn reservation_schema() -> String {
    format!(
        "CREATE TABLE io_reservations(operation_id TEXT PRIMARY KEY,subject TEXT NOT NULL,events INTEGER NOT NULL CHECK(events=1),bytes INTEGER NOT NULL CHECK(bytes>0 AND bytes<={})) STRICT;",
        MAX_CONTAINER_BYTES
    )
}
/// One dispatched operation reserves exactly the terminal Observed revision
/// plus its audit event; record containers never exceed MAX_CONTAINER_BYTES.
pub const FOLLOWUP_EVENTS: u64 = 1;
pub const FOLLOWUP_BYTES: u64 = MAX_CONTAINER_BYTES as u64;
/// Logical follow-up capacity currently held for one in-flight IO command.
/// This is an accounting promise inside the Store, not physical disk space and
/// not permission to contact a backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoIntentReservation {
    pub events: u64,
    pub bytes: u64,
}
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let v = version(c)?;
    let n: i64 = sql(c.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE name='io_intents' AND type='table'",
        [],
        |r| r.get(0),
    ))?;
    if n != i64::from(v >= 15) {
        return Err(Error::Integrity);
    }
    let r: i64 = sql(c.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE name='io_reservations' AND type='table'",
        [],
        |r| r.get(0),
    ))?;
    if r != i64::from(v >= 16) {
        return Err(Error::Integrity);
    }
    Ok(())
}
/// Total logical capacity held by all pre-dispatch reservations. Zero before v16.
pub(super) fn reservations(c: &Connection) -> Result<(i64, i64)> {
    if version(c)? < 16 {
        return Ok((0, 0));
    }
    sql(c.query_row(
        "SELECT coalesce(sum(events),0),coalesce(sum(bytes),0) FROM io_reservations",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ))
}
pub(super) fn reject_reserved(c: &Connection, operation: &str) -> Result<()> {
    if version(c)? < 15 {
        return Ok(());
    }
    identity(operation)?;
    let exists: bool = sql(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM io_intents WHERE operation_id=?1)",
        [operation],
        |r| r.get(0),
    ))?;
    if exists {
        return Err(Error::OperationConflict);
    }
    Ok(())
}
fn reject_other_ids(c: &Connection, id: &str) -> Result<()> {
    let exists: bool = sql(c.query_row("SELECT EXISTS(SELECT 1 FROM operations WHERE id=?1) OR EXISTS(SELECT 1 FROM read_captures WHERE operation_id=?1) OR EXISTS(SELECT 1 FROM read_archives WHERE operation_id=?1)", [id], |r| r.get(0)))?;
    if exists {
        return Err(Error::OperationConflict);
    }
    Ok(())
}
pub(super) fn history(c: &Connection, operation: &str) -> Result<Vec<Record>> {
    identity(operation)?;
    if version(c)? < 15 {
        return Ok(Vec::new());
    }
    let mut statement = sql(c.prepare("SELECT i.revision,i.event_id,o.card_id,o.object_kind,CASE WHEN length(o.payload)<=?2 THEN o.payload ELSE NULL END,e.sequence FROM io_intents i LEFT JOIN operations o ON o.id=i.event_id LEFT JOIN operation_events e ON e.id=i.event_id WHERE i.operation_id=?1 ORDER BY i.revision LIMIT 4"))?;
    let mut rows = sql(statement.query(params![operation, MAX_CONTAINER_BYTES as i64]))?;
    let mut records: Vec<Record> = Vec::new();
    let mut sequence = 0i64;
    while let Some(row) = sql(rows.next())? {
        if records.len() == 3 {
            return Err(Error::Integrity);
        }
        let revision: i64 = sql(row.get(0))?;
        let event_ref = sql(row.get_ref(1))?;
        let event = event_ref.as_str().map_err(|_| Error::Integrity)?;
        identity(event)?;
        let subject_ref = sql(row.get_ref(2))?;
        let subject = subject_ref.as_str().map_err(|_| Error::Integrity)?;
        identity(subject)?;
        let kind: i64 = sql(row.get(3))?;
        let raw = sql(row.get_ref(4))?;
        if matches!(raw, rusqlite::types::ValueRef::Null) {
            return Err(Error::Integrity);
        }
        let record = Record::decode(raw.as_blob().map_err(|_| Error::Integrity)?)?;
        let next_sequence: i64 = sql(row.get(5))?;
        if kind != 5
            || revision != records.len() as i64 + 1
            || revision != record.revision() as i64
            || event != record.event_id()
            || subject != record.command().subject
            || operation != record.command().operation_id
            || next_sequence <= sequence
        {
            return Err(Error::Integrity);
        }
        if let Some(previous) = records.last() {
            previous.verify_successor(&record)?;
        } else if record.phase() != Phase::Prepared {
            return Err(Error::Integrity);
        }
        sequence = next_sequence;
        records.push(record);
    }
    Ok(records)
}
pub(super) fn verify_operation(
    c: &Connection,
    event: &str,
    subject: &str,
    raw: &[u8],
) -> Result<()> {
    // Accepts the in-flight v15→v16 and v16→v17 migrations: pre-existing
    // kind-5 events must keep verifying while the reservation and material
    // namespaces are being added.
    if !matches!(version(c)?, 15..=17) {
        return Err(Error::UnsupportedVersion);
    }
    let record = Record::decode(raw)?;
    if record.event_id() != event || record.command().subject != subject {
        return Err(Error::Integrity);
    }
    let all = history(c, &record.command().operation_id)?;
    let stored = all
        .get(record.revision() as usize - 1)
        .ok_or(Error::Integrity)?;
    if stored.container() != raw {
        return Err(Error::Integrity);
    }
    super::blobs::verify_event(c, event, &[])?;
    super::evidence::verify_event(c, event, &[])?;
    Ok(())
}
pub(super) fn verify(c: &Connection) -> Result<()> {
    verify_schema(c)?;
    let v = version(c)?;
    if v < 15 {
        return Ok(());
    }
    let invalid: bool = sql(c.query_row("SELECT EXISTS(SELECT 1 FROM io_intents i LEFT JOIN operations o ON o.id=i.event_id WHERE o.id IS NULL OR o.object_kind!=5) OR EXISTS(SELECT 1 FROM operations o LEFT JOIN io_intents i ON i.event_id=o.id WHERE o.object_kind=5 AND i.event_id IS NULL) OR EXISTS(SELECT 1 FROM io_intents i JOIN operations o ON o.id=i.operation_id) OR EXISTS(SELECT 1 FROM io_intents i JOIN read_captures r ON r.operation_id=i.operation_id) OR EXISTS(SELECT 1 FROM io_intents i JOIN read_archives r ON r.operation_id=i.operation_id)", [], |r| r.get(0)))?;
    if invalid {
        return Err(Error::Integrity);
    }
    if v >= 16 {
        // A reservation must reference a live history owned by the same subject
        // and must never outlive the terminal Observed revision.
        let invalid_reservations: bool = sql(c.query_row(
            "SELECT EXISTS(SELECT 1 FROM io_reservations r LEFT JOIN io_intents i ON i.operation_id=r.operation_id AND i.revision=1 LEFT JOIN operations o ON o.id=i.event_id WHERE i.event_id IS NULL OR o.card_id!=r.subject) OR EXISTS(SELECT 1 FROM io_reservations r JOIN io_intents i ON i.operation_id=r.operation_id AND i.revision=3)",
            [],
            |r| r.get(0),
        ))?;
        if invalid_reservations {
            return Err(Error::Integrity);
        }
    }
    let mut statement =
        sql(c.prepare("SELECT DISTINCT operation_id FROM io_intents ORDER BY operation_id"))?;
    let mut rows = sql(statement.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let operation_ref = sql(row.get_ref(0))?;
        let operation = operation_ref.as_str().map_err(|_| Error::Integrity)?;
        let records = history(c, operation)?;
        if v >= 16 {
            // OutcomeUnknown means the dispatch boundary was crossed, which the
            // append path only permits with a held reservation; terminal phases
            // must have released or consumed it.
            let reserved: bool = sql(c.query_row(
                "SELECT EXISTS(SELECT 1 FROM io_reservations WHERE operation_id=?1)",
                [operation],
                |r| r.get(0),
            ))?;
            let latest_phase = records.last().ok_or(Error::Integrity)?.phase();
            let held_correctly = match latest_phase {
                // Prepared may or may not still hold its reservation.
                Phase::Prepared => true,
                // The dispatch boundary is only reachable with a held reservation.
                Phase::OutcomeUnknown => reserved,
                // Terminal phases must have released or consumed it.
                Phase::Observed | Phase::CancelledBeforeDispatch => !reserved,
                Phase::InvalidPhase => false,
            };
            if !held_correctly {
                return Err(Error::Integrity);
            }
        }
    }
    Ok(())
}
impl Store {
    /// Reserves the complete post-dispatch intent/audit capacity for one stored
    /// Prepared command before the dispatch boundary may be crossed. The
    /// reservation is a logical quota managed inside the same Store write
    /// transaction family as every other writer: it reduces the capacity all
    /// other events may consume, is idempotent per operationId, is bound to the
    /// exact stored command, is consumed by the terminal Observed revision and
    /// released by pre-dispatch cancellation. It is NOT physical disk space and
    /// NOT a dispatch permit.
    pub fn reserve_io_intent_followup(
        &mut self,
        command: &crate::io_intent::Command,
        authorize: impl FnOnce() -> Result<()>,
    ) -> Result<IoIntentReservation> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if !matches!(version(&tx)?, 16..=17) {
            return Err(Error::UnsupportedVersion);
        }
        let all = history(&tx, &command.operation_id)?;
        let stored = all.first().ok_or(Error::NotFound)?;
        // A different command for the same operationId can never reserve or
        // extend the quota bound to the stored original.
        stored.matches_command(command)?;
        if all.last().ok_or(Error::Integrity)?.phase() != Phase::Prepared {
            return Err(Error::Invalid("IO intent reservation phase"));
        }
        let existing: Option<(String, i64)> = sql(tx.query_row(
            "SELECT subject,bytes FROM io_reservations WHERE operation_id=?1",
            [&command.operation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).optional())?;
        if let Some((subject, bytes)) = existing {
            if subject != command.subject {
                return Err(Error::Integrity);
            }
            // Idempotent: the same operationId never deducts twice.
            authorize()?;
            return Ok(IoIntentReservation {
                events: FOLLOWUP_EVENTS,
                bytes: bytes as u64,
            });
        }
        let (count, bytes): (i64, i64) = sql(tx.query_row(
            "SELECT count(*),coalesce(sum(length(payload)),0) FROM outbox",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ))?;
        let (reserved_events, reserved_bytes) = reservations(&tx)?;
        // Protected IO material is held and stored in the same byte budget, so a
        // dispatched command can never be crowded out by material it must keep.
        let material_bytes =
            u64::try_from(super::io_evidence::accounted(&tx)?).map_err(|_| Error::Integrity)?;
        if count.saturating_add(reserved_events).saturating_add(FOLLOWUP_EVENTS as i64)
            >= i64::from(self.budget.max_count)
            || (bytes as u64)
                .saturating_add(reserved_bytes as u64)
                .saturating_add(material_bytes)
                .saturating_add(FOLLOWUP_BYTES)
                > self.budget.max_bytes
        {
            return Err(Error::EventCapacity);
        }
        sql(tx.execute(
            "INSERT INTO io_reservations(operation_id,subject,events,bytes) VALUES(?1,?2,1,?3)",
            params![&command.operation_id, command.subject, FOLLOWUP_BYTES as i64],
        ))?;
        boundary("io-reservation-after-insert");
        // No network or external work may occur in this synchronous callback.
        authorize()?;
        boundary("io-reservation-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("io-reservation-after-commit");
        Ok(IoIntentReservation {
            events: FOLLOWUP_EVENTS,
            bytes: FOLLOWUP_BYTES,
        })
    }
    /// Releases a reservation while the command is still before the dispatch
    /// boundary. After OutcomeUnknown the terminal Observed revision must keep
    /// its guarantee, so release is refused; double release stays idempotent.
    pub fn release_io_intent_followup(&mut self, subject: &str, operation: &str) -> Result<()> {
        identity(subject)?;
        identity(operation)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if !matches!(version(&tx)?, 16..=17) {
            return Err(Error::UnsupportedVersion);
        }
        let all = history(&tx, operation)?;
        let stored = all.first().ok_or(Error::NotFound)?;
        if stored.command().subject != subject {
            return Err(Error::NotFound);
        }
        match all.last().ok_or(Error::Integrity)?.phase() {
            Phase::Prepared => {}
            _ => return Err(Error::Invalid("IO intent reservation terminal")),
        }
        sql(tx.execute(
            "DELETE FROM io_reservations WHERE operation_id=?1",
            [operation],
        ))?;
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        Ok(())
    }
    /// Logical follow-up capacity currently held by pre-dispatch reservations.
    /// New events must fit within the budget minus pending usage minus this.
    pub fn io_intent_reservation_usage(&self) -> Result<(u64, u64)> {
        let (events, bytes) = reservations(&self.connection)?;
        Ok((
            u64::try_from(events).map_err(|_| Error::Integrity)?,
            u64::try_from(bytes).map_err(|_| Error::Integrity)?,
        ))
    }
    /// Trusted-local historical append, NOT authority to contact a backend.
    /// An exact explicit retry returns the original revision without a new event.
    /// The caller must not interpret either result as an instruction to resend.
    /// The dispatch boundary (Prepared → OutcomeUnknown) requires a complete
    /// pre-dispatch reservation for the same command; pre-dispatch cancellation
    /// releases it and the terminal Observed revision consumes it.
    pub fn append_io_intent_local_authorized(
        &mut self,
        candidate: &Record,
        authorize: impl FnOnce() -> Result<()>,
    ) -> Result<Record> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        if !matches!(version(&tx)?, 16..=17) {
            return Err(Error::UnsupportedVersion);
        }
        let command = candidate.command();
        let all = history(&tx, &command.operation_id)?;
        reject_other_ids(&tx, &command.operation_id)?;
        if let Some(first) = all.first() {
            first.matches_command(command)?;
            if let Some(stored) = all.get(candidate.revision() as usize - 1) {
                if stored.raw() != candidate.raw() {
                    return Err(Error::OperationConflict);
                }
                authorize()?;
                return Ok(stored.clone());
            }
            all.last()
                .ok_or(Error::Integrity)?
                .verify_successor(candidate)?;
            match candidate.phase() {
                Phase::OutcomeUnknown => {
                    // Dispatch boundary: the complete post-send revision and
                    // its audit event must already be reserved for this exact
                    // command by the same subject.
                    let reserved: Option<String> = sql(tx.query_row(
                        "SELECT subject FROM io_reservations WHERE operation_id=?1",
                        [&command.operation_id],
                        |r| r.get(0),
                    ).optional())?;
                    if reserved.as_deref() != Some(command.subject.as_str()) {
                        return Err(Error::Invalid("IO intent dispatch reservation"));
                    }
                }
                Phase::CancelledBeforeDispatch => {
                    // No dispatch happened; the held quota returns to everyone.
                    // Any protected material reservation is released with it; an
                    // original already admitted stays retained for the history.
                    sql(tx.execute(
                        "DELETE FROM io_reservations WHERE operation_id=?1",
                        [&command.operation_id],
                    ))?;
                    if version(&tx)? >= 17 {
                        sql(tx.execute(
                            "DELETE FROM io_material_reservations WHERE operation_id=?1",
                            [&command.operation_id],
                        ))?;
                    }
                }
                Phase::Observed => {
                    // Terminal observation consumes exactly this operation's
                    // own reservation. No fresh capacity check: the reservation
                    // already accounted for this event, even if the database is
                    // reopened with a lower budget after dispatch.
                    let deleted = sql(tx.execute(
                        "DELETE FROM io_reservations WHERE operation_id=?1 AND subject=?2",
                        params![&command.operation_id, command.subject.as_str()],
                    ))?;
                    if deleted != 1 {
                        return Err(Error::Integrity);
                    }
                    // Without stored protected material an observation would
                    // leave an unexplained reservation, so a terminal history
                    // must not hold any material quota.
                    if version(&tx)? >= 17 {
                        let held: bool = sql(tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM io_material_reservations WHERE operation_id=?1)",
                            [&command.operation_id],
                            |r| r.get(0),
                        ))?;
                        if held {
                            return Err(Error::Invalid("IO material reservation"));
                        }
                    }
                }
                Phase::Prepared | Phase::InvalidPhase => return Err(Error::Integrity),
            }
        } else {
            if candidate.phase() != Phase::Prepared {
                return Err(Error::RevisionConflict);
            }
            let stray: bool = sql(tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM io_reservations WHERE operation_id=?1)",
                [&command.operation_id],
                |r| r.get(0),
            ))?;
            if stray {
                return Err(Error::Integrity);
            }
        }
        let event = candidate.event_id();
        if event == command.operation_id {
            return Err(Error::OperationConflict);
        }
        reject_other_ids(&tx, &event)?;
        reject_reserved(&tx, &event)?;
        // The terminal observation is paid from this operation's own
        // reservation instead of the budget headroom other writers compete for.
        if candidate.phase() != Phase::Observed {
            super::event_room(&tx, self.budget, candidate.container().len() as u64)?;
        }
        sql(tx.execute(
            "INSERT INTO operations(id,card_id,object_kind,payload) VALUES(?1,?2,5,?3)",
            params![event, command.subject, candidate.container()],
        ))?;
        boundary("io-intent-after-operation");
        sql(tx.execute(
            "INSERT INTO io_intents(operation_id,revision,event_id) VALUES(?1,?2,?3)",
            params![command.operation_id, candidate.revision() as i64, event],
        ))?;
        boundary("io-intent-after-history");
        sql(tx.execute(
            "INSERT INTO outbox(id,payload) VALUES(?1,?2)",
            params![event, candidate.container()],
        ))?;
        sql(tx.execute(
            "INSERT INTO operation_events(sequence,id) VALUES(last_insert_rowid(),?1)",
            [&event],
        ))?;
        boundary("io-intent-after-event");
        // No network or external work may occur in this synchronous callback.
        authorize()?;
        boundary("io-intent-before-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("io-intent-after-commit");
        Ok(candidate.clone())
    }
    /// A pinned local history lookup. Different subjects observe absence.
    /// Historical approval digests never restore live authorization.
    pub fn lookup_io_intent(&self, subject: &str, operation: &str) -> Result<Option<Record>> {
        identity(subject)?;
        identity(operation)?;
        let tx = sql(self.connection.unchecked_transaction())?;
        // Avoid returning another subject's historical command.
        if version(&tx)? < 15 {
            return Ok(None);
        }
        let (present, initial, owned): (bool, bool, bool) = sql(tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM io_intents WHERE operation_id=?1),EXISTS(SELECT 1 FROM io_intents i JOIN operations o ON o.id=i.event_id WHERE i.operation_id=?1 AND i.revision=1),EXISTS(SELECT 1 FROM io_intents i JOIN operations o ON o.id=i.event_id WHERE i.operation_id=?1 AND i.revision=1 AND o.card_id=?2)",
            params![operation, subject], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ))?;
        if present && !initial {
            return Err(Error::Integrity);
        }
        if !owned {
            return Ok(None);
        }
        let all = history(&tx, operation)?;
        Ok(all.last().cloned())
    }
}
