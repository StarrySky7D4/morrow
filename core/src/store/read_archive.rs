//! Single-database preparation and atomic publication of opaque read-capture parts.
use super::{Store, boundary, sql};
use crate::{
    Error, Result, identity,
    read_archive::{self, Finish, Manifest, Part, Plan, Status},
    read_journal::{self, ReadObservation, Receipt},
    task_evidence::Evidence,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
pub(super) const SCHEMA:&str="CREATE TABLE read_archives(operation_id TEXT PRIMARY KEY,subject TEXT NOT NULL,published INTEGER NOT NULL,payload BLOB NOT NULL) STRICT;
CREATE TABLE read_archive_parts(operation_id TEXT NOT NULL REFERENCES read_archives(operation_id),ordinal INTEGER NOT NULL,payload BLOB NOT NULL,PRIMARY KEY(operation_id,ordinal)) STRICT;
CREATE TABLE operation_read_archives(operation_id TEXT PRIMARY KEY REFERENCES operations(id),root BLOB NOT NULL) STRICT;";
fn version(c: &Connection) -> Result<i64> {
    sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))
}
fn require(c: &Connection) -> Result<()> {
    if !matches!(version(c)?, 12..=16) {
        return Err(Error::UnsupportedVersion);
    }
    Ok(())
}
pub(super) fn verify_schema(c: &Connection) -> Result<()> {
    let n:i64=sql(c.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('read_archives','read_archive_parts','operation_read_archives')",[],|r|r.get(0)))?;
    if n != if version(c)? < 12 { 0 } else { 3 } {
        return Err(Error::Integrity);
    }
    Ok(())
}
pub(super) fn load(c: &Connection, subject: &str, operation: &str) -> Result<Option<Manifest>> {
    require(c)?;
    identity(subject)?;
    identity(operation)?;
    let mut st=sql(c.prepare("SELECT subject,published,CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM read_archives WHERE operation_id=?1"))?;
    let mut rows = sql(st.query(params![operation, read_archive::MAX_CONTAINER_BYTES as i64]))?;
    let Some(row) = sql(rows.next())? else {
        return Ok(None);
    };
    let id = sql(row.get_ref(0))?;
    let id = id.as_str().map_err(|_| Error::Integrity)?;
    identity(id)?;
    if id != subject {
        return Ok(None);
    }
    let published: i64 = sql(row.get(1))?;
    let raw = sql(row.get_ref(2))?;
    if matches!(raw, rusqlite::types::ValueRef::Null) {
        return Err(Error::Limit);
    }
    let m = Manifest::decode(raw.as_blob().map_err(|_| Error::Integrity)?)?;
    let status = m.status();
    if status.plan.operation_id != operation
        || status.plan.subject != subject
        || published != i64::from(status.root.is_some())
    {
        return Err(Error::Integrity);
    }
    super::read_capture::verify_binding(c, &m)?;
    Ok(Some(m))
}
fn part(c: &Connection, operation: &str, ordinal: u32) -> Result<Option<Part>> {
    let raw:Option<Option<Vec<u8>>>=sql(c.query_row("SELECT CASE WHEN length(payload)<=?3 THEN payload ELSE NULL END FROM read_archive_parts WHERE operation_id=?1 AND ordinal=?2",params![operation,ordinal,read_archive::MAX_CONTAINER_BYTES as i64],|r|r.get(0)).optional())?;
    let Some(raw) = raw else { return Ok(None) };
    let p = Part::decode(&raw.ok_or(Error::Limit)?)?;
    if p.ordinal() != ordinal {
        return Err(Error::Integrity);
    }
    Ok(Some(p))
}
pub(super) fn verify_parts(c: &Connection, m: &Manifest) -> Result<()> {
    let state = m.status();
    let mut count = 0u32;
    let mut total = 0u64;
    let mut chain = m.initial_chain();
    let mut st=sql(c.prepare("SELECT ordinal,CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM read_archive_parts WHERE operation_id=?1 ORDER BY ordinal"))?;
    let mut rows = sql(st.query(params![
        state.plan.operation_id,
        read_archive::MAX_CONTAINER_BYTES as i64
    ]))?;
    while let Some(row) = sql(rows.next())? {
        if count >= state.count {
            return Err(Error::Integrity);
        }
        let ordinal: i64 = sql(row.get(0))?;
        if ordinal != i64::from(count) {
            return Err(Error::Integrity);
        }
        let raw = sql(row.get_ref(1))?;
        if matches!(raw, rusqlite::types::ValueRef::Null) {
            return Err(Error::Limit);
        }
        let p = Part::decode(raw.as_blob().map_err(|_| Error::Integrity)?)?;
        if p.ordinal() != count || p.previous_sha256() != chain {
            return Err(Error::Integrity);
        }
        total = total
            .checked_add((p.raw().len() + p.container().len()) as u64)
            .ok_or(Error::Limit)?;
        if total > state.plan.budget.max_bytes {
            return Err(Error::Limit);
        }
        chain = p.digest();
        count += 1;
    }
    if count != state.count || total != state.logical_bytes || chain != state.chain_sha256 {
        return Err(Error::Integrity);
    }
    Ok(())
}
pub(super) fn verify_association(c: &Connection, m: &Manifest) -> Result<()> {
    let state = m.status();
    let linked:Option<Vec<u8>>=sql(c.query_row("SELECT CASE WHEN length(root)=32 THEN root ELSE NULL END FROM operation_read_archives WHERE operation_id=?1",[&state.plan.operation_id],|r|r.get(0)).optional())?;
    if let Some(root) = state.root {
        if linked.as_deref() != Some(root.as_slice()) {
            return Err(Error::Integrity);
        }
        let mut st=sql(c.prepare("SELECT card_id,object_kind,CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM operations WHERE id=?1"))?;
        let mut rows = sql(st.query(params![
            state.plan.operation_id,
            read_journal::MAX_CONTAINER_BYTES as i64
        ]))?;
        let row = sql(rows.next())?.ok_or(Error::Integrity)?;
        let subject = sql(row.get_ref(0))?;
        let subject = subject.as_str().map_err(|_| Error::Integrity)?;
        if subject != state.plan.subject || sql(row.get::<_, i64>(1))? != 4 {
            return Err(Error::Integrity);
        }
        let raw = sql(row.get_ref(2))?;
        if matches!(raw, rusqlite::types::ValueRef::Null) {
            return Err(Error::Limit);
        }
        let observed = read_journal::decode(raw.as_blob().map_err(|_| Error::Integrity)?)?;
        match_observation(m, &observed)?;
    } else {
        if linked.is_some() {
            return Err(Error::Integrity);
        }
        // A damaged publication cannot be reclassified as disposable preparation.
        // Ordinary content/record/schema1 operations may still occupy this provisional
        // ID; only an archived read proves that its original archive must be retained.
        let raw: Option<Option<Vec<u8>>> = sql(c.query_row(
            "SELECT CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM operations WHERE id=?1 AND object_kind=4",
            params![state.plan.operation_id, read_journal::MAX_CONTAINER_BYTES as i64],
            |r| r.get(0),
        ).optional())?;
        if let Some(raw) = raw {
            let observed = read_journal::decode(&raw.ok_or(Error::Limit)?)?;
            if observed.data().schema_version == 2 {
                return Err(Error::Integrity);
            }
        }
    }
    Ok(())
}
fn match_observation(m: &Manifest, observation: &ReadObservation) -> Result<()> {
    let s = m.status();
    let v = observation.data();
    if v.schema_version != 2
        || v.archive_sha256.as_slice() != m.digest()
        || s.root.is_none()
        || v.operation_id != s.plan.operation_id
        || v.subject != s.plan.subject
        || v.request_type != s.plan.request_type
        || v.request != s.plan.request
        || v.response_type != s.plan.response_type
        || m.response_sha256() != Some(Sha256::digest(&v.response).into())
    {
        return Err(Error::Integrity);
    }
    Ok(())
}
pub(super) fn verify_observation(c: &Connection, v: &ReadObservation) -> Result<()> {
    let version = version(c)?;
    if v.data().schema_version == 1 {
        if version >= 12 {
            let n: bool = sql(c.query_row(
                "SELECT EXISTS(SELECT 1 FROM operation_read_archives WHERE operation_id=?1)",
                [&v.data().operation_id],
                |r| r.get(0),
            ))?;
            if n {
                return Err(Error::Integrity);
            }
        }
        return Ok(());
    }
    if version < 12 {
        return Err(Error::UnsupportedVersion);
    }
    let m = load(c, &v.data().subject, &v.data().operation_id)?.ok_or(Error::Integrity)?;
    match_observation(&m, v)?;
    verify_parts(c, &m)?;
    verify_association(c, &m)
}
pub(super) fn verify(c: &Connection) -> Result<()> {
    verify_schema(c)?;
    if version(c)? < 12 {
        return Ok(());
    }
    let bad:bool=sql(c.query_row("SELECT EXISTS(SELECT 1 FROM read_archive_parts p LEFT JOIN read_archives a ON a.operation_id=p.operation_id WHERE a.operation_id IS NULL) OR EXISTS(SELECT 1 FROM operation_read_archives r LEFT JOIN read_archives a ON a.operation_id=r.operation_id LEFT JOIN operations o ON o.id=r.operation_id WHERE a.operation_id IS NULL OR o.id IS NULL OR a.published!=1 OR o.object_kind!=4)",[],|r|r.get(0)))?;
    if bad {
        return Err(Error::Integrity);
    }
    let mut st = sql(c.prepare("SELECT operation_id,subject FROM read_archives"))?;
    let mut rows = sql(st.query([]))?;
    while let Some(row) = sql(rows.next())? {
        let op = sql(row.get_ref(0))?;
        let op = op.as_str().map_err(|_| Error::Integrity)?;
        let subject = sql(row.get_ref(1))?;
        let subject = subject.as_str().map_err(|_| Error::Integrity)?;
        let m = load(c, subject, op)?.ok_or(Error::Integrity)?;
        verify_parts(c, &m)?;
        verify_association(c, &m)?;
    }
    Ok(())
}
impl Store {
    /// Preparation is not a global operation reservation and creates no audit event.
    pub fn begin_read_archive(&mut self, plan: &Plan) -> Result<Status> {
        self.begin_read_archive_with_admission_budget(plan, Default::default())
    }
    /// Apply a stricter budget to this admission, not a persistent per-library policy.
    /// Exact no-growth retries remain available after a budget reduction.
    pub fn begin_read_archive_with_admission_budget(
        &mut self,
        plan: &Plan,
        budget: read_archive::PreparationBudget,
    ) -> Result<Status> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        super::read_capture::reject_tracked(&tx, &plan.operation_id)?;
        let value = begin_in(&tx, &Manifest::new(plan)?, budget, self.retention_budget)?;
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("archive-after-begin-commit");
        Ok(value)
    }
    pub fn append_read_archive(
        &mut self,
        subject: &str,
        operation: &str,
        ordinal: u32,
        type_id: &str,
        data: &[u8],
    ) -> Result<Status> {
        self.append_read_archive_with_admission_budget(
            subject,
            operation,
            ordinal,
            type_id,
            data,
            Default::default(),
        )
    }
    /// Every writer uses the database-wide counters in the same IMMEDIATE transaction.
    /// The optional stricter budget applies to this call; hard global ceilings cannot be raised.
    #[allow(clippy::too_many_arguments)]
    pub fn append_read_archive_with_admission_budget(
        &mut self,
        subject: &str,
        operation: &str,
        ordinal: u32,
        type_id: &str,
        data: &[u8],
        budget: read_archive::PreparationBudget,
    ) -> Result<Status> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        super::read_capture::reject_tracked(&tx, operation)?;
        let value = append_in(
            &tx,
            subject,
            operation,
            ordinal,
            type_id,
            data,
            budget,
            self.retention_budget,
        )?;
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("archive-after-append-commit");
        Ok(value)
    }
    /// End metadata is a trusted adapter's claim of completeness; the store validates
    /// exact bounded bytes, ordered closure and current guard, not application semantics.
    pub fn finish_read_archive_local_authorized(
        &mut self,
        subject: &str,
        operation: &str,
        end: &Finish,
        evidence: &[Evidence],
        authorize: impl FnOnce() -> Result<()>,
    ) -> Result<Receipt> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        super::read_capture::reject_tracked(&tx, operation)?;
        let value = finish_in(
            &tx,
            subject,
            operation,
            end,
            evidence,
            self.budget,
            self.retention_budget,
            authorize,
        )?;
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("archive-after-commit");
        Ok(value)
    }
    pub fn lookup_read_archive(&self, subject: &str, operation: &str) -> Result<Option<Status>> {
        Ok(self
            .read_archive_manifest(subject, operation)?
            .map(|m| m.status()))
    }
    pub fn read_archive_manifest(
        &self,
        subject: &str,
        operation: &str,
    ) -> Result<Option<Manifest>> {
        let tx = sql(self.connection.unchecked_transaction())?;
        let m = load(&tx, subject, operation)?;
        if let Some(m) = &m {
            verify_parts(&tx, m)?;
            verify_association(&tx, m)?;
        }
        Ok(m)
    }
    /// Bounded whole-part access is trusted-local; it neither publishes nor authorizes a read.
    pub fn read_archive_part(
        &self,
        subject: &str,
        operation: &str,
        ordinal: u32,
    ) -> Result<Option<Part>> {
        let tx = sql(self.connection.unchecked_transaction())?;
        let Some(m) = load(&tx, subject, operation)? else {
            return Ok(None);
        };
        // Authentication to the final root needs the complete chain in this same
        // transaction. This bounded whole-part API is O(N); it is not a seek proof.
        verify_parts(&tx, &m)?;
        verify_association(&tx, &m)?;
        let s = m.status();
        if ordinal >= s.count {
            return Ok(None);
        }
        let p = part(&tx, operation, ordinal)?.ok_or(Error::Integrity)?;
        let previous = if ordinal == 0 {
            m.initial_chain()
        } else {
            part(&tx, operation, ordinal - 1)?
                .ok_or(Error::Integrity)?
                .digest()
        };
        if p.previous_sha256() != previous
            || (ordinal + 1 == s.count && p.digest() != s.chain_sha256)
        {
            return Err(Error::Integrity);
        }
        Ok(Some(p))
    }
    pub fn abort_read_archive(&mut self, subject: &str, operation: &str) -> Result<bool> {
        let tx = sql(self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate))?;
        super::read_capture::reject_tracked(&tx, operation)?;
        let value = abort_in(&tx, subject, operation)?;
        tx.commit().map_err(|_| Error::CommitUnknown)?;
        boundary("read-archive-abort-after-commit");
        Ok(value)
    }
}

pub(super) fn begin_in(
    tx: &Connection,
    initial: &Manifest,
    budget: read_archive::PreparationBudget,
    retention: read_archive::RetentionBudget,
) -> Result<Status> {
    budget.validate()?;
    let plan = &initial.status().plan;
    require(tx)?;
    if let Some(m) = load(tx, &plan.subject, &plan.operation_id)? {
        if m.status().plan != *plan {
            return Err(Error::OperationConflict);
        }
        verify_parts(tx, &m)?;
        verify_association(tx, &m)?;
        return Ok(m.status());
    }
    let used:bool=sql(tx.query_row("SELECT EXISTS(SELECT 1 FROM operations WHERE id=?1) OR EXISTS(SELECT 1 FROM read_archives WHERE operation_id=?1)",[&plan.operation_id],|r|r.get(0)))?;
    if used {
        return Err(Error::OperationConflict);
    }
    super::read_archive_budget::admit(tx, None, initial, budget)?;
    sql(tx.execute(
        "INSERT INTO read_archives(operation_id,subject,published,payload) VALUES(?1,?2,0,?3)",
        params![plan.operation_id, plan.subject, initial.container()],
    ))?;
    super::read_archive_retention::change(tx, None, Some(initial), retention)?;
    boundary("archive-after-begin");
    Ok(initial.status())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn append_in(
    tx: &Connection,
    subject: &str,
    operation: &str,
    ordinal: u32,
    type_id: &str,
    data: &[u8],
    budget: read_archive::PreparationBudget,
    retention: read_archive::RetentionBudget,
) -> Result<Status> {
    budget.validate()?;
    identity(type_id)?;
    if data.len() > read_archive::MAX_PART_BYTES {
        return Err(Error::Limit);
    }
    let m = load(tx, subject, operation)?.ok_or(Error::NotFound)?;
    let state = m.status();
    if state.root.is_some() {
        return Err(Error::OperationConflict);
    }
    verify_association(tx, &m)?;
    if ordinal < state.count {
        let old = part(tx, operation, ordinal)?.ok_or(Error::Integrity)?;
        if old.type_id() != type_id || old.data() != data {
            return Err(Error::OperationConflict);
        }
        return Ok(state);
    }
    if ordinal != state.count {
        return Err(Error::OperationConflict);
    }
    if ordinal > 0 {
        let prev = part(tx, operation, ordinal - 1)?.ok_or(Error::Integrity)?;
        if prev.digest() != state.chain_sha256 {
            return Err(Error::Integrity);
        }
    }
    let p = Part::new(ordinal, type_id, data, state.chain_sha256)?;
    let next = m.append(&p)?;
    super::read_archive_budget::admit(tx, Some(&m), &next, budget)?;
    super::read_archive_retention::change(tx, Some(&m), Some(&next), retention)?;
    sql(tx.execute(
        "INSERT INTO read_archive_parts(operation_id,ordinal,payload) VALUES(?1,?2,?3)",
        params![operation, ordinal, p.container()],
    ))?;
    sql(tx.execute(
        "UPDATE read_archives SET payload=?2 WHERE operation_id=?1",
        params![operation, next.container()],
    ))?;
    boundary("archive-after-part");
    boundary("archive-before-append-commit");
    Ok(next.status())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn finish_in(
    tx: &Connection,
    subject: &str,
    operation: &str,
    end: &Finish,
    evidence: &[Evidence],
    event_budget: super::EventBudget,
    retention: read_archive::RetentionBudget,
    authorize: impl FnOnce() -> Result<()>,
) -> Result<Receipt> {
    let m = load(tx, subject, operation)?.ok_or(Error::NotFound)?;
    verify_parts(tx, &m)?;
    verify_association(tx, &m)?;
    let finalized = m.finalize(end)?;
    let state = finalized.status();
    let refs = super::evidence::digests(evidence)?;
    let input = read_journal::Input {
        operation_id: operation.into(),
        subject: subject.into(),
        request_type: state.plan.request_type,
        request: state.plan.request,
        response_type: state.plan.response_type,
        response: end.response.clone(),
    };
    let observed = read_journal::encode_archived(&input, &refs, finalized.digest())?;
    let existing: Option<(String, i64)> = sql(tx
        .query_row(
            "SELECT card_id,object_kind FROM operations WHERE id=?1",
            [operation],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional())?;
    if let Some((id, kind)) = existing {
        if m.status().root.is_none() || id != subject || kind != 4 || m.raw() != finalized.raw() {
            return Err(Error::OperationConflict);
        }
        let raw:Option<Vec<u8>>=sql(tx.query_row("SELECT CASE WHEN length(payload)<=?2 THEN payload ELSE NULL END FROM operations WHERE id=?1",params![operation,read_journal::MAX_CONTAINER_BYTES as i64],|r|r.get(0)))?;
        let old = read_journal::decode(&raw.ok_or(Error::Limit)?)?;
        if old.raw() != observed.raw() {
            return Err(Error::OperationConflict);
        }
        super::evidence::verify_retry(tx, operation, &old.data().task_evidence_sha256, evidence)?;
        authorize()?;
        return Ok(old.receipt());
    }
    if m.status().root.is_some() {
        return Err(Error::Integrity);
    }
    super::read_archive_retention::change(tx, Some(&m), Some(&finalized), retention)?;
    super::event_room(tx, event_budget, observed.container().len() as u64)?;
    sql(tx.execute(
        "INSERT INTO operations(id,card_id,object_kind,payload) VALUES(?1,?2,4,?3)",
        params![operation, subject, observed.container()],
    ))?;
    super::evidence::bind(tx, operation, evidence)?;
    sql(tx.execute(
        "UPDATE read_archives SET published=1,payload=?2 WHERE operation_id=?1",
        params![operation, finalized.container()],
    ))?;
    sql(tx.execute(
        "INSERT INTO operation_read_archives(operation_id,root) VALUES(?1,?2)",
        params![operation, finalized.digest().as_slice()],
    ))?;
    sql(tx.execute(
        "INSERT INTO outbox(id,payload) VALUES(?1,?2)",
        params![operation, observed.container()],
    ))?;
    sql(tx.execute(
        "INSERT INTO operation_events(sequence,id) VALUES(last_insert_rowid(),?1)",
        [operation],
    ))?;
    boundary("archive-after-publish");
    authorize()?;
    boundary("archive-before-commit");
    Ok(observed.receipt())
}
pub(super) fn abort_in(tx: &Connection, subject: &str, operation: &str) -> Result<bool> {
    let Some(m) = load(tx, subject, operation)? else {
        return Ok(false);
    };
    if m.status().root.is_some() {
        return Err(Error::OperationConflict);
    }
    verify_association(tx, &m)?;
    super::read_archive_retention::change(tx, Some(&m), None, Default::default())?;
    sql(tx.execute(
        "DELETE FROM read_archive_parts WHERE operation_id=?1",
        [operation],
    ))?;
    sql(tx.execute(
        "DELETE FROM read_archives WHERE operation_id=?1",
        [operation],
    ))?;
    boundary("read-archive-abort-before-commit");
    Ok(true)
}
