//! Bounded discovery of original file mutation plans. A page is a fresh SQLite
//! view, so it does not promise a stable census across pages or a current phase.
use super::{Store, io_evidence, io_intent, sql};
use crate::{
    Error, Result,
    file_mutation::{self, Disposition, RequestRecord},
    io_evidence::Kind,
};
use rusqlite::{OptionalExtension, params};
use std::sync::Arc;

pub const MAX_FILE_MUTATION_PLAN_SCAN_LIMIT: u16 = 8;
pub const MAX_FILE_MUTATION_PLAN_BYTES: usize =
    file_mutation::MAX_RAW_BYTES + file_mutation::MAX_RAW_BYTES / 255 + 128;
const MAX_MATERIAL_RAW_BYTES: usize = MAX_FILE_MUTATION_PLAN_BYTES + 4096;
/// Upper bound on the stored material container read for one candidate.
pub const MAX_FILE_MUTATION_PLAN_MATERIAL_BYTES: usize =
    MAX_MATERIAL_RAW_BYTES + MAX_MATERIAL_RAW_BYTES / 255 + 128;
/// Conservative logical reservation per scanned candidate for three small IO
/// records, one bounded material container, decoded request, and SQL metadata.
/// This is a read budget, not a bound on the allocator's peak live memory.
pub const MAX_FILE_MUTATION_PLAN_READ_BYTES_PER_CANDIDATE: usize = 64 * 1024;

pub struct FileMutationPlanCursor {
    owner: Arc<()>,
    subject: String,
    package: [u8; 32],
    disposition: Disposition,
    after: String,
    done: bool,
    poisoned: bool,
}

/// Opaque continuation for a successfully read, nonterminal page. It is
/// bound to one live Store instance and discovery scope, not to a worker lease.
#[derive(Clone)]
pub struct FileMutationPlanCheckpoint {
    owner: Arc<()>,
    subject: String,
    package: [u8; 32],
    disposition: Disposition,
    after: String,
}

impl std::fmt::Debug for FileMutationPlanCheckpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileMutationPlanCheckpoint(..)")
    }
}

impl FileMutationPlanCheckpoint {
    /// Conservative queue charge for the two bounded, owned identifiers.
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.subject.capacity() + self.after.capacity()
    }
}

#[derive(Debug)]
pub struct FileMutationPlanPage {
    pub plans: Vec<RequestRecord>,
    pub scanned: u32,
    pub done: bool,
    pub checkpoint: Option<FileMutationPlanCheckpoint>,
}

impl Store {
    /// No SQL or grant is acquired at open. The cursor belongs only to this
    /// Store instance and retains no snapshot between calls.
    pub fn open_file_mutation_plan_cursor(
        &self,
        subject: &str,
        package: [u8; 32],
        disposition: Disposition,
    ) -> Result<FileMutationPlanCursor> {
        crate::identity(subject)?;
        if package == [0; 32] {
            return Err(Error::Invalid("empty file mutation package identity"));
        }
        Ok(FileMutationPlanCursor {
            owner: self.snapshot_identity.clone(),
            subject: subject.to_owned(),
            package,
            disposition,
            after: String::new(),
            done: false,
            poisoned: false,
        })
    }

    /// Resume only a checkpoint issued by this exact Store and scope. No SQL
    /// or authority is acquired here; each later page remains a fresh view.
    pub fn resume_file_mutation_plan_cursor(
        &self,
        checkpoint: &FileMutationPlanCheckpoint,
        subject: &str,
        package: [u8; 32],
        disposition: Disposition,
    ) -> Result<FileMutationPlanCursor> {
        if !Arc::ptr_eq(&self.snapshot_identity, &checkpoint.owner)
            || subject != checkpoint.subject
            || package != checkpoint.package
            || disposition != checkpoint.disposition
        {
            return Err(Error::Invalid("foreign file mutation plan checkpoint"));
        }
        Ok(FileMutationPlanCursor {
            owner: self.snapshot_identity.clone(),
            subject: checkpoint.subject.clone(),
            package: checkpoint.package,
            disposition: checkpoint.disposition,
            after: checkpoint.after.clone(),
            done: false,
            poisoned: false,
        })
    }

    /// Scans at most `scan_limit` global operation candidates. Foreign cursors
    /// are rejected before SQL. Every failed read poisons the cursor; callers
    /// must open a new one after an authorization or integrity failure.
    pub fn read_file_mutation_plan_page(
        &self,
        cursor: &mut FileMutationPlanCursor,
        scan_limit: u16,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<FileMutationPlanPage> {
        if cursor.poisoned || !Arc::ptr_eq(&self.snapshot_identity, &cursor.owner) {
            cursor.poisoned = true;
            return Err(Error::Invalid(
                "foreign or invalid file mutation plan cursor",
            ));
        }
        if !(1..=MAX_FILE_MUTATION_PLAN_SCAN_LIMIT).contains(&scan_limit) {
            cursor.poisoned = true;
            return Err(Error::Limit);
        }
        match self.file_mutation_plan_page(cursor, scan_limit, &mut authorize) {
            Ok(page) => Ok(page),
            Err(error) => {
                cursor.poisoned = true;
                Err(error)
            }
        }
    }

    fn file_mutation_plan_page(
        &self,
        cursor: &mut FileMutationPlanCursor,
        scan_limit: u16,
        authorize: &mut impl FnMut() -> Result<()>,
    ) -> Result<FileMutationPlanPage> {
        authorize()?;
        if cursor.done {
            authorize()?;
            return Ok(FileMutationPlanPage {
                plans: Vec::new(),
                scanned: 0,
                done: true,
                checkpoint: None,
            });
        }
        let snapshot = sql(self.connection.unchecked_transaction())?;
        let version: i64 = sql(snapshot.query_row("PRAGMA user_version", [], |row| row.get(0)))?;
        if !(21..=super::SCHEMA_VERSION).contains(&version) {
            return Err(Error::UnsupportedVersion);
        }
        // The extra row is lookahead only; it is never scanned or exposed.
        // CASE enforces the byte bound before rusqlite allocates a String.
        let (operations, has_more) = {
            let mut statement = sql(snapshot.prepare(
                "SELECT CASE WHEN length(CAST(operation_id AS BLOB))<=256 THEN operation_id ELSE NULL END \
                 FROM io_intents WHERE operation_id>?1 AND revision=1 \
                 ORDER BY operation_id LIMIT ?2",
            ))?;
            let mut rows = sql(statement.query(params![&cursor.after, i64::from(scan_limit) + 1]))?;
            let mut operations = Vec::with_capacity(usize::from(scan_limit));
            let mut has_more = false;
            while let Some(row) = sql(rows.next())? {
                if operations.len() == usize::from(scan_limit) {
                    has_more = true;
                    break;
                }
                let operation = sql(row.get_ref(0))?
                    .as_str()
                    .map_err(|_| Error::Integrity)?;
                crate::identity(operation).map_err(|_| Error::Integrity)?;
                operations.push(operation.to_owned());
            }
            (operations, has_more)
        };
        let mut plans = Vec::new();
        for operation in &operations {
            authorize()?;
            // Subject isolation is metadata-only and precedes history or
            // protected request reads. The CASE caps text before allocation.
            let subject: Option<Option<String>> = sql(snapshot.query_row(
                "SELECT CASE WHEN length(CAST(o.card_id AS BLOB))<=256 THEN o.card_id ELSE NULL END \
                 FROM io_intents i LEFT JOIN operations o ON o.id=i.event_id \
                 WHERE i.operation_id=?1 AND i.revision=1",
                [operation],
                |row| row.get(0),
            ).optional())?;
            let subject = subject.flatten().ok_or(Error::Integrity)?;
            if subject != cursor.subject {
                continue;
            }
            let history = io_intent::history(&snapshot, operation)?;
            let first = history.first().ok_or(Error::Integrity)?;
            let command = first.command();
            if command.subject != cursor.subject {
                return Err(Error::Integrity);
            }
            if command.package_sha256 != cursor.package
                || command.capability != cursor.disposition.capability()
                || command.protocol_sha256 != file_mutation::schema_digest()
            {
                continue;
            }
            if command.request_bytes == 0
                || command.request_bytes > MAX_FILE_MUTATION_PLAN_BYTES as u64
            {
                return Err(Error::Limit);
            }
            authorize()?;
            let material = io_evidence::load_material_bounded(
                &snapshot,
                operation,
                Kind::Request,
                &history,
                MAX_MATERIAL_RAW_BYTES,
            )?
            .ok_or(Error::Integrity)?;
            if material.payload().len() > MAX_FILE_MUTATION_PLAN_BYTES {
                return Err(Error::Limit);
            }
            let plan = RequestRecord::decode(material.payload())?;
            first.matches_command(&plan.command()?)?;
            if plan.request().disposition != cursor.disposition {
                return Err(Error::Integrity);
            }
            plans.push(plan);
        }
        let scanned = operations.len() as u32;
        let after = operations.last().cloned();
        sql(snapshot.rollback())?;
        authorize()?;
        if let Some(after) = after {
            cursor.after = after;
        }
        cursor.done = !has_more;
        let checkpoint = (!cursor.done).then(|| FileMutationPlanCheckpoint {
            owner: self.snapshot_identity.clone(),
            subject: cursor.subject.clone(),
            package: cursor.package,
            disposition: cursor.disposition,
            after: cursor.after.clone(),
        });
        Ok(FileMutationPlanPage {
            plans,
            scanned,
            done: cursor.done,
            checkpoint,
        })
    }
}
