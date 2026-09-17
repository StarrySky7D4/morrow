//! Capture state and archive mutations share the existing SQLite writer transaction.
//! Current control records are not signed transition histories. Publication retains its
//! normal signed ReadJournal root; the root pins the immutable capture binding.
use super::{Store, boundary, read_archive, sql};
use crate::{
    Error, Result, identity,
    read_archive::{Finish, Manifest, Plan},
    read_capture::{self, Budget, Phase, State, Token, Usage},
    read_journal::Receipt,
    task_evidence::Evidence,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
pub(super) const SCHEMA: &str = "CREATE TABLE read_captures(operation_id TEXT PRIMARY KEY,subject TEXT NOT NULL,phase INTEGER NOT NULL,charged_bytes INTEGER NOT NULL,payload BLOB NOT NULL) STRICT; CREATE INDEX read_capture_preparing ON read_captures(phase,operation_id);";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
fn require(c: &Connection) -> Result<()> {
    if !matches!(version(c)?, 13..=17) {
        return Err(Error::UnsupportedVersion);
    }
    Ok(())
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let n:i64=sql(c.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('read_captures','read_capture_preparing')",[],|r|r.get(0)))?;
    if n != if version(c)? < 13 { 0 } else { 2 } {
        return Err(Error::Integrity);
    }
    Ok(())
}
fn load(c: &Connection, subject: &str, operation: &str) -> Result<Option<State>> {
    require(c)?;
    identity(subject)?;
    identity(operation)?;
    let mut st=sql(c.prepare("SELECT subject,phase,charged_bytes,CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM read_captures WHERE operation_id=?1"))?;
    let mut rows = sql(st.query(params![operation, read_capture::MAX_CONTAINER_BYTES as i64]))?;
    let Some(row) = sql(rows.next())? else {
        return Ok(None);
    };
    let id = sql(row.get_ref(0))?;
    let id = id.as_str().map_err(|_| Error::Integrity)?;
    identity(id)?;
    if id != subject {
        return Ok(None);
    }
    let phase: i64 = sql(row.get(1))?;
    let charge: i64 = sql(row.get(2))?;
    let raw = sql(row.get_ref(3))?;
    if matches!(raw, rusqlite::types::ValueRef::Null) {
        return Err(Error::Limit);
    }
    let state = State::decode(raw.as_blob().map_err(|_| Error::Integrity)?)?;
    if state.plan().operation_id != operation
        || state.plan().subject != subject
        || phase != state.phase() as i64
        || charge < 0
        || charge as u64 != state.charged_bytes()
    {
        return Err(Error::Integrity);
    }
    Ok(Some(state))
}
pub(super) fn reject_tracked(c: &Connection, operation: &str) -> Result<()> {
    super::io_intent::reject_reserved(c, operation)?;
    if version(c)? < 13 {
        return Ok(());
    }
    identity(operation)?;
    let found: bool = sql(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM read_captures WHERE operation_id=?1)",
        [operation],
        |r| r.get(0),
    ))?;
    if found {
        return Err(Error::OperationConflict);
    }
    // A deleted state cannot turn an immutable tracked marker into a provisional ID.
    let raw:Option<Option<Vec<u8>>>=sql(c.query_row("SELECT CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM read_archives WHERE operation_id=?1",params![operation,crate::read_archive::MAX_CONTAINER_BYTES as i64],|r|r.get(0)).optional())?;
    if let Some(raw) = raw
        && Manifest::decode(&raw.ok_or(Error::Limit)?)?
            .capture_binding()
            .is_some()
    {
        return Err(Error::Integrity);
    }
    Ok(())
}
pub(super) fn verify_binding(c: &Connection, m: &Manifest) -> Result<()> {
    let status = m.status();
    let binding = m.capture_binding();
    if version(c)? < 13 {
        return if binding.is_none() {
            Ok(())
        } else {
            Err(Error::UnsupportedVersion)
        };
    }
    let state = load(c, &status.plan.subject, &status.plan.operation_id)?;
    match (binding, state) {
        (None, None) => Ok(()),
        (Some(binding), Some(state)) => {
            if binding != state.binding()
                || status.plan != *state.plan()
                || m.value.plan != state.data().plan
            {
                return Err(Error::Integrity);
            }
            let (phase, revision) = if status.root.is_some() {
                (Phase::Ready, u64::from(status.count) + 2)
            } else {
                (Phase::Preparing, u64::from(status.count) + 1)
            };
            if state.phase() != phase
                || state.revision() != revision
                || state.archive_root() != status.root
            {
                return Err(Error::Integrity);
            }
            Ok(())
        }
        _ => Err(Error::Integrity),
    }
}
fn verify_state(c: &Connection, state: &State) -> Result<()> {
    let p = state.plan();
    match state.phase() {
        Phase::Preparing | Phase::Ready => {
            let m = read_archive::load(c, &p.subject, &p.operation_id)?.ok_or(Error::Integrity)?;
            verify_binding(c, &m)?;
            read_archive::verify_association(c, &m)?;
        }
        Phase::Failed | Phase::Cancelled | Phase::Interrupted => {
            if state.revision() < 2 {
                return Err(Error::Integrity);
            }
            let exists:bool=sql(c.query_row("SELECT EXISTS(SELECT 1 FROM read_archives WHERE operation_id=?1) OR EXISTS(SELECT 1 FROM operations WHERE id=?1)",[&p.operation_id],|r|r.get(0)))?;
            if exists {
                return Err(Error::Integrity);
            }
        }
        _ => return Err(Error::Integrity),
    }
    Ok(())
}
pub(super) fn verify(c: &Connection) -> Result<()> {
    verify_schema(c)?;
    if version(c)? < 13 {
        return Ok(());
    }
    let mut st = sql(c.prepare("SELECT operation_id,subject FROM read_captures"))?;
    let mut rows = sql(st.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let op = sql(row.get_ref(0))?;
        let op = op.as_str().map_err(|_| Error::Integrity)?;
        let sub = sql(row.get_ref(1))?;
        let sub = sub.as_str().map_err(|_| Error::Integrity)?;
        let state = load(c, sub, op)?.ok_or(Error::Integrity)?;
        verify_state(c, &state)?;
    }
    Ok(())
}
fn usage(c: &Connection) -> Result<Usage> {
    require(c)?;
    let mut result = Usage::default();
    let mut st = sql(
        c.prepare("SELECT operation_id,subject FROM read_captures ORDER BY operation_id LIMIT ?1")
    )?;
    let mut rows = sql(st.query([read_capture::MAX_STATES + 1]))?;
    while let Some(row) = sql(rows.next())? {
        if result.count >= read_capture::MAX_STATES {
            return Err(Error::Limit);
        }
        let op = sql(row.get_ref(0))?;
        let op = op.as_str().map_err(|_| Error::Integrity)?;
        let sub = sql(row.get_ref(1))?;
        let sub = sub.as_str().map_err(|_| Error::Integrity)?;
        let state = load(c, sub, op)?.ok_or(Error::Integrity)?;
        result.count += 1;
        result.charged_bytes = result
            .charged_bytes
            .checked_add(state.charged_bytes())
            .ok_or(Error::Limit)?;
    }
    Ok(result)
}
pub(super) fn preparing_charge(c: &Connection) -> Result<u64> {
    if version(c)? < 13 {
        return Ok(0);
    }
    let mut count = 0;
    let mut bytes = 0u64;
    let mut st=sql(c.prepare("SELECT operation_id,subject FROM read_captures WHERE phase=1 ORDER BY operation_id LIMIT ?1"))?;
    let mut rows = sql(st.query([crate::read_archive::MAX_PREPARATIONS + 1]))?;
    while let Some(row) = sql(rows.next())? {
        if count >= crate::read_archive::MAX_PREPARATIONS {
            return Err(Error::Limit);
        }
        let op = sql(row.get_ref(0))?;
        let op = op.as_str().map_err(|_| Error::Integrity)?;
        let sub = sql(row.get_ref(1))?;
        let sub = sub.as_str().map_err(|_| Error::Integrity)?;
        let state = load(c, sub, op)?.ok_or(Error::Integrity)?;
        if state.phase() != Phase::Preparing {
            return Err(Error::Integrity);
        }
        bytes = bytes
            .checked_add(state.charged_bytes())
            .ok_or(Error::Limit)?;
        count += 1;
    }
    Ok(bytes)
}
fn save(c: &Connection, state: &State) -> Result<()> {
    sql(c.execute(
        "UPDATE read_captures SET phase=?2,charged_bytes=?3,payload=?4 WHERE operation_id=?1",
        params![
            state.plan().operation_id,
            state.phase() as i32,
            state.charged_bytes() as i64,
            state.container()
        ],
    ))?;
    Ok(())
}
impl Store {
    pub fn begin_read_capture(
        &mut self,
        plan: &Plan,
        context_type: &str,
        context: &[u8],
        owner: [u8; 32],
    ) -> Result<State> {
        self.begin_read_capture_with_budget(plan, context_type, context, owner, Budget::default())
    }
    /// A lower admission policy is per-call, not a persisted replacement of hard limits.
    pub fn begin_read_capture_with_budget(
        &mut self,
        plan: &Plan,
        context_type: &str,
        context: &[u8],
        owner: [u8; 32],
        budget: Budget,
    ) -> Result<State> {
        budget.validate()?;
        let state = State::new(plan, context_type, context, owner)?;
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        require(&tx)?;
        if let Some(previous) = load(&tx, &plan.subject, &plan.operation_id)? {
            if previous.plan() != plan
                || previous.context_type() != context_type
                || previous.context() != context
                || previous.owner() != owner
            {
                return Err(Error::OperationConflict);
            }
            verify_state(&tx, &previous)?;
            return Ok(previous);
        }
        reject_tracked(&tx, &plan.operation_id)?;
        let existing:bool=sql(tx.query_row("SELECT EXISTS(SELECT 1 FROM operations WHERE id=?1) OR EXISTS(SELECT 1 FROM read_archives WHERE operation_id=?1)",[&plan.operation_id],|r|r.get(0)))?;
        if existing {
            return Err(Error::OperationConflict);
        }
        let used = usage(&tx)?;
        if used.count >= budget.max_states
            || used
                .charged_bytes
                .checked_add(state.charged_bytes())
                .ok_or(Error::Limit)?
                > budget.max_bytes
        {
            return Err(Error::Limit);
        }
        sql(tx.execute("INSERT INTO read_captures(operation_id,subject,phase,charged_bytes,payload) VALUES(?1,?2,1,?3,?4)",params![plan.operation_id,plan.subject,state.charged_bytes() as i64,state.container()]))?;
        boundary("capture-after-begin-state");
        read_archive::begin_in(
            &tx,
            &Manifest::for_capture(plan, state.binding())?,
            budget.preparation,
            self.retention_budget,
        )?;
        verify_state(&tx, &state)?;
        boundary("capture-before-begin-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("capture-after-begin-commit");
        Ok(state)
    }
    pub fn lookup_read_capture(&self, subject: &str, operation: &str) -> Result<Option<State>> {
        let tx = sql(self.connection.unchecked_transaction())?;
        let value = load(&tx, subject, operation)?;
        if let Some(state) = &value {
            verify_state(&tx, state)?;
        }
        Ok(value)
    }
    /// Ascending IDs, up to limit and 32 MiB originals+containers. Continue until an
    /// empty page; a short nonempty page alone is not EOF.
    pub fn list_read_captures(&self, after: &str, limit: u32) -> Result<Vec<State>> {
        if limit == 0 || limit > 128 {
            return Err(Error::Limit);
        }
        if !after.is_empty() {
            identity(after)?;
        }
        let tx = sql(self.connection.unchecked_transaction())?;
        require(&tx)?;
        let query = if after.is_empty() {
            "SELECT operation_id,subject FROM read_captures ORDER BY operation_id LIMIT ?1"
        } else {
            "SELECT operation_id,subject FROM read_captures WHERE operation_id>?2 ORDER BY operation_id LIMIT ?1"
        };
        let mut st = sql(tx.prepare(query))?;
        let mut rows = if after.is_empty() {
            sql(st.query([limit]))?
        } else {
            sql(st.query(params![limit, after]))?
        };
        let mut values = vec![];
        let mut bytes = 0usize;
        while let Some(row) = sql(rows.next())? {
            let op = sql(row.get_ref(0))?;
            let op = op.as_str().map_err(|_| Error::Integrity)?;
            let sub = sql(row.get_ref(1))?;
            let sub = sub.as_str().map_err(|_| Error::Integrity)?;
            let state = load(&tx, sub, op)?.ok_or(Error::Integrity)?;
            let next = bytes
                .checked_add(state.raw().len() + state.container().len())
                .ok_or(Error::Limit)?;
            if next > 32 * 1024 * 1024 {
                break;
            }
            verify_state(&tx, &state)?;
            bytes = next;
            values.push(state);
        }
        Ok(values)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn append_read_capture(
        &mut self,
        subject: &str,
        operation: &str,
        token: &Token,
        ordinal: u32,
        type_id: &str,
        data: &[u8],
    ) -> Result<State> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        let old = load(&tx, subject, operation)?.ok_or(Error::NotFound)?;
        old.check(token)?;
        if old.phase() != Phase::Preparing {
            return Err(Error::OperationConflict);
        }
        verify_state(&tx, &old)?;
        let before = read_archive::load(&tx, subject, operation)?
            .ok_or(Error::Integrity)?
            .status();
        let after = read_archive::append_in(
            &tx,
            subject,
            operation,
            ordinal,
            type_id,
            data,
            Default::default(),
            self.retention_budget,
        )?;
        let state = if after.count == before.count {
            old
        } else {
            old.advance(Phase::Preparing, None, "")?
        };
        save(&tx, &state)?;
        boundary("capture-after-append-state");
        verify_state(&tx, &state)?;
        boundary("capture-before-append-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("capture-after-append-commit");
        Ok(state)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn finish_read_capture_local_authorized(
        &mut self,
        subject: &str,
        operation: &str,
        token: &Token,
        end: &Finish,
        evidence: &[Evidence],
        authorize: impl FnOnce() -> Result<()>,
    ) -> Result<(State, Receipt)> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        let old = load(&tx, subject, operation)?.ok_or(Error::NotFound)?;
        old.check(token)?;
        if !matches!(old.phase(), Phase::Preparing | Phase::Ready) {
            return Err(Error::OperationConflict);
        }
        verify_state(&tx, &old)?;
        let m = read_archive::load(&tx, subject, operation)?.ok_or(Error::Integrity)?;
        let finalized = m.finalize(end)?;
        let advancing = old.phase() == Phase::Preparing;
        let state = if !advancing {
            old
        } else {
            old.advance(Phase::Ready, Some(finalized.digest()), "")?
        };
        let receipt = read_archive::finish_in(
            &tx,
            subject,
            operation,
            end,
            evidence,
            self.budget,
            self.retention_budget,
            || {
                if advancing {
                    save(&tx, &state)?;
                }
                boundary("capture-after-ready-state");
                verify_state(&tx, &state)?;
                authorize()
            },
        )?;
        boundary("capture-before-finish-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("capture-after-finish-commit");
        Ok((state, receipt))
    }
    pub fn end_read_capture(
        &mut self,
        subject: &str,
        operation: &str,
        token: &Token,
        phase: Phase,
        reason: &str,
    ) -> Result<State> {
        if !matches!(phase, Phase::Failed | Phase::Cancelled | Phase::Interrupted) {
            return Err(Error::Invalid("capture terminal phase"));
        }
        if reason.len() > 256 {
            return Err(Error::Limit);
        }
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        let old = load(&tx, subject, operation)?.ok_or(Error::NotFound)?;
        old.check(token)?;
        verify_state(&tx, &old)?;
        if old.phase() != Phase::Preparing {
            if old.phase() == phase && old.reason() == reason {
                return Ok(old);
            }
            return Err(Error::OperationConflict);
        }
        if !read_archive::abort_in(&tx, subject, operation)? {
            return Err(Error::Integrity);
        }
        let state = old.advance(phase, None, reason)?;
        save(&tx, &state)?;
        boundary("capture-after-end-state");
        verify_state(&tx, &state)?;
        boundary("capture-before-end-commit");
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("capture-after-end-commit");
        Ok(state)
    }
    pub fn read_capture_usage(&self) -> Result<Usage> {
        let tx = sql(self.connection.unchecked_transaction())?;
        usage(&tx)
    }
}
