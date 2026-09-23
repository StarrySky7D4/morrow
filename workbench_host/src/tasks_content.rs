//! Real owner routes for versioned task content. No legacy business contract is reinterpreted.
//! Commit snapshots are historical receipts; UI callers must read the current versioned card.
use crate::{
    Result, Workbench, WorkbenchState, now, tasks_edit, tasks_migration, versioned_record,
};
use morrow_core::{
    content::CardRecord,
    lifecycle::GrantKind,
    task::Invocation,
    task_evidence::Evidence,
    transaction::{self, Lookup, Receipt},
};
use morrow_workbench_plugin::tasks_v2::Command;
use std::{error::Error, fmt};
use versioned_record::{TaskRecord, VersionedRecord};

/// The exact edit was observed absent after a failure before its commit path.
pub(crate) const VERSIONED_NO_COMMIT_UI_CODE: u16 = 1001;
#[derive(Debug)]
pub(crate) struct VersionedNoCommit {
    pub(crate) card_id: String,
    pub(crate) operation: String,
    cause: Box<dyn Error>,
}
impl fmt::Display for VersionedNoCommit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(f)
    }
}
impl Error for VersionedNoCommit {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
impl WorkbenchState {
    /// Fail closed if the scoped lookup itself fails or finds a receipt.
    pub(crate) fn no_commit_if_absent(
        &self,
        card_id: &str,
        operation: &str,
        cause: Box<dyn Error>,
    ) -> Box<dyn Error> {
        if matches!(
            self.host.store_local().lookup_for_card(card_id, operation),
            Ok(Lookup::Absent)
        ) {
            Box::new(VersionedNoCommit {
                card_id: card_id.into(),
                operation: operation.into(),
                cause,
            })
        } else {
            cause
        }
    }
}

#[derive(Debug, Clone)]
pub struct MigrationPlan {
    pub operation: String,
    pub card_id: String,
    pub source_revision: u64,
}

/// This is the result of one exact operation, not necessarily the latest card.
pub struct TaskCommit {
    pub receipt: Receipt,
    pub committed: TaskRecord,
    pub repeated: bool,
}

pub(super) fn task_record(card: &CardRecord) -> Result<TaskRecord> {
    match versioned_record::decode(card)? {
        VersionedRecord::Tasks(record) => Ok(record),
        VersionedRecord::Legacy(_) => Err("expected task content format 2".into()),
    }
}

impl WorkbenchState {
    pub(crate) fn read_versioned(&self, id: &str) -> Result<VersionedRecord> {
        versioned_record::decode(&self.host.store_local().card(id)?.ok_or("card not found")?)
    }
    pub(crate) fn page_versioned(
        &self,
        after: &str,
        limit: u32,
    ) -> Result<(Vec<VersionedRecord>, String)> {
        let ids = self.host.store_local().card_ids_local(after, limit)?;
        let cursor = ids.last().cloned().unwrap_or_default();
        let mut result = Vec::new();
        for id in ids {
            let card = self
                .host
                .store_local()
                .card(&id)?
                .ok_or("card disappeared")?;
            if card.summary().type_id == "org.morrow.idea" {
                result.push(versioned_record::decode(&card)?);
            }
        }
        Ok((result, cursor))
    }
    pub(crate) fn plan_tasks_migration(&mut self, id: &str) -> Result<MigrationPlan> {
        self.prepare_write()?;
        let source = self.authorized_read(id)?;
        let plan = tasks_migration::Plan::prepare(
            &source,
            self.bundle.as_ref().ok_or("plugin unavailable")?,
        )?;
        Ok(MigrationPlan {
            operation: plan.operation_id().into(),
            card_id: id.into(),
            source_revision: source.summary().revision,
        })
    }
    pub(super) fn captured_task(&mut self, invocation: &Invocation) -> Result<Evidence> {
        let result = self.pool.record_transform(
            self.manager.as_ref().ok_or("plugin manager unavailable")?,
            &mut self.host,
            self.plugin.as_ref().ok_or("plugin unavailable")?,
            invocation,
        );
        self.finish_stopped_session();
        let (_, evidence) = result?.into_parts();
        Ok(evidence)
    }
    pub(super) fn task_history(
        &self,
        id: &str,
        operation: &str,
    ) -> Result<Option<(transaction::proto::Commit, Receipt, Evidence)>> {
        if matches!(
            self.host.store_local().lookup_for_card(id, operation)?,
            Lookup::Absent
        ) {
            return Ok(None);
        }
        let (commit, receipt) = self
            .host
            .store_local()
            .operation_commit(id, operation)?
            .ok_or("operation belongs to another content object")?;
        let mut evidence = self.host.store_local().operation_evidence(id, operation)?;
        if evidence.len() != 1 {
            return Err("operation has no supported task evidence".into());
        }
        Ok(Some((commit, receipt, evidence.remove(0))))
    }
    fn commit_task_migration(
        &mut self,
        projected: &tasks_migration::ProjectedMigration,
    ) -> Result<Receipt> {
        let id = projected.card().summary().id;
        self.grant(&id, GrantKind::EditContent)?;
        let start = self.start;
        let result = projected.commit(
            &mut self.host,
            self.pool
                .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?
                .connection(),
            || now(start),
        );
        let revoked = self.revoke(&id, GrantKind::EditContent);
        let receipt = result?;
        revoked?;
        Ok(receipt)
    }
    fn commit_task_edit(&mut self, projected: &tasks_edit::ProjectedEdit) -> Result<Receipt> {
        let id = projected.card().summary().id;
        self.grant(&id, GrantKind::EditContent)?;
        let start = self.start;
        let result = projected.commit(
            &mut self.host,
            self.pool
                .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?
                .connection(),
            || now(start),
        );
        let revoked = self.revoke(&id, GrantKind::EditContent);
        let receipt = result?;
        revoked?;
        Ok(receipt)
    }
    pub(crate) fn migrate_tasks(
        &mut self,
        operation: &str,
        id: &str,
        source_revision: u64,
    ) -> Result<TaskCommit> {
        self.prepare_write()?;
        if let Some((commit, expected, evidence)) = self.task_history(id, operation)? {
            let projected = tasks_migration::verify_commit(&commit, &evidence)?;
            let source = CardRecord::decode(projected.source_card())?;
            if source.summary().id != id
                || source.summary().revision != source_revision
                || commit.operation_id != operation
            {
                return Err("migration retry differs from original baseline".into());
            }
            let committed = task_record(projected.card())?;
            let receipt = self.commit_task_migration(&projected)?;
            if receipt != expected {
                return Err("historical migration receipt mismatch".into());
            }
            return Ok(TaskCommit {
                receipt,
                committed,
                repeated: true,
            });
        }
        let source = self.authorized_read(id)?;
        if source.summary().revision != source_revision {
            return Err("migration source revision conflict".into());
        }
        let plan = tasks_migration::Plan::prepare(
            &source,
            self.bundle.as_ref().ok_or("plugin unavailable")?,
        )?;
        if plan.operation_id() != operation {
            return Err("migration operation differs from fixed baseline".into());
        }
        let evidence = self.captured_task(plan.invocation())?;
        let projected = plan.capture(&evidence)?;
        let committed = task_record(projected.card())?;
        let receipt = self.commit_task_migration(&projected)?;
        self.undo.remove(id);
        Ok(TaskCommit {
            receipt,
            committed,
            repeated: false,
        })
    }
    pub(crate) fn edit_tasks(
        &mut self,
        operation: &str,
        id: &str,
        source_revision: u64,
        command: &Command,
    ) -> Result<TaskCommit> {
        self.prepare_write()
            .map_err(|e| self.no_commit_if_absent(id, operation, e))?;
        if let Some((commit, expected, evidence)) = self.task_history(id, operation)? {
            let projected = tasks_edit::verify_commit(&commit, &evidence)?;
            if projected.card().summary().id != id
                || !projected.matches_intent(operation, source_revision, command)?
            {
                return Err("task retry differs from original request".into());
            }
            let committed = task_record(projected.card())?;
            let receipt = self.commit_task_edit(&projected)?;
            if receipt != expected {
                return Err("historical task receipt mismatch".into());
            }
            return Ok(TaskCommit {
                receipt,
                committed,
                repeated: true,
            });
        }
        // Keep the classifier outside this block: commit_task_edit may begin a
        // durable operation even when it later returns an error.
        let prepared = (|| -> Result<_> {
            let source = self.authorized_read(id)?;
            if source.summary().revision != source_revision {
                return Err("task source revision conflict".into());
            }
            if task_record(&source)?.properties.deleted {
                return Err("deleted card requires restore".into());
            }
            let plan = tasks_edit::Plan::prepare(
                &source,
                self.bundle.as_ref().ok_or("plugin unavailable")?,
                operation,
                command,
            )?;
            let evidence = self.captured_task(plan.invocation())?;
            let projected = plan.capture(&evidence)?;
            let committed = task_record(projected.card())?;
            Ok((projected, committed))
        })();
        let (projected, committed) =
            prepared.map_err(|e| self.no_commit_if_absent(id, operation, e))?;
        let receipt = self.commit_task_edit(&projected)?;
        self.undo.remove(id);
        Ok(TaskCommit {
            receipt,
            committed,
            repeated: false,
        })
    }
}
impl Workbench {
    /// Trusted local UI read, available without a running plugin. V2 is explicit.
    pub fn read_versioned(&self, id: &str) -> Result<VersionedRecord> {
        self.local_state()?.read_versioned(id)
    }
    pub fn page_versioned(
        &self,
        after: &str,
        limit: u32,
    ) -> Result<(Vec<VersionedRecord>, String)> {
        self.local_state()?.page_versioned(after, limit)
    }
    pub fn plan_tasks_migration(&mut self, id: &str) -> Result<MigrationPlan> {
        self.local_state_mut()?.plan_tasks_migration(id)
    }
    pub fn migrate_tasks(
        &mut self,
        operation: &str,
        id: &str,
        source_revision: u64,
    ) -> Result<TaskCommit> {
        self.local_state_mut()?
            .migrate_tasks(operation, id, source_revision)
    }
    pub fn edit_tasks(
        &mut self,
        operation: &str,
        id: &str,
        source_revision: u64,
        command: &Command,
    ) -> Result<TaskCommit> {
        self.local_state_mut()?
            .edit_tasks(operation, id, source_revision, command)
    }
}
