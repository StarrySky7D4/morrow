//! Owner routes for common format-2 card operations, independent of Widget lifetime.
use crate::{
    Result, Workbench, WorkbenchState, cards_edit, now,
    tasks_content::{TaskCommit, task_record},
};
use morrow_core::{
    content::{Attachment, CardRecord},
    lifecycle::GrantKind,
    transaction::Receipt,
};
use morrow_workbench_plugin::cards_v2::{Command, Fields};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardAction {
    Edit(Fields),
    SetFavorite(bool),
    SetCategory { category: String, stage: String },
    Delete,
    Restore,
}
impl CardAction {
    fn command(&self, observed: u64) -> Command {
        match self {
            Self::Edit(fields) => Command::Edit(fields.clone()),
            Self::SetFavorite(value) => Command::SetFavorite(*value),
            Self::SetCategory { category, stage } => Command::SetCategory {
                category: category.clone(),
                stage: stage.clone(),
            },
            Self::Delete => Command::Delete { now_ms: observed },
            Self::Restore => Command::Restore { now_ms: observed },
        }
    }
}
impl WorkbenchState {
    pub(crate) fn card_attachments(
        &self,
        id: &str,
        source: &CardRecord,
        action: &CardAction,
    ) -> Result<Vec<Attachment>> {
        let current = source.attachments();
        let CardAction::Edit(fields) = action else {
            return Ok(current);
        };
        let mut selected = Vec::with_capacity(fields.assets.len());
        for asset in &fields.assets {
            let attachment = self
                .staged
                .get(&(id.into(), asset.id.clone()))
                .or_else(|| current.iter().find(|value| value.id == asset.id))
                .ok_or("attachment was not selected for this card")?;
            if attachment.display_name != asset.name || attachment.byte_length != asset.bytes {
                return Err("selected attachment metadata mismatch".into());
            }
            selected.push(attachment.clone());
        }
        Ok(selected)
    }
    fn commit_card_edit(&mut self, projected: &cards_edit::ProjectedEdit) -> Result<Receipt> {
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
    pub(crate) fn edit_card(
        &mut self,
        operation: &str,
        id: &str,
        revision: u64,
        action: &CardAction,
    ) -> Result<TaskCommit> {
        self.prepare_write()
            .map_err(|e| self.no_commit_if_absent(id, operation, e))?;
        if let Some((commit, expected, evidence)) = self.task_history(id, operation)? {
            let projected = cards_edit::verify_commit(&commit, &evidence)?;
            let command = action.command(projected.observed_now());
            if projected.card().summary().id != id
                || !projected.matches_intent(operation, revision, &command)?
            {
                return Err("card retry differs from original request".into());
            }
            let committed = task_record(projected.card())?;
            let receipt = self.commit_card_edit(&projected)?;
            if receipt != expected {
                return Err("historical card receipt mismatch".into());
            }
            // Never recreate an undo window or discard newly staged assets on a retry.
            return Ok(TaskCommit {
                receipt,
                committed,
                repeated: true,
            });
        }
        // All failures in this block precede commit_card_edit. Recheck the
        // exact operation after failure before claiming no commit.
        let prepared = (|| -> Result<_> {
            let source = self.authorized_read(id)?;
            if source.summary().revision != revision {
                return Err("card source revision conflict".into());
            }
            task_record(&source)?;
            let observed = now(self.start);
            let undo = if matches!(action, CardAction::Restore) {
                let &(undo_revision, deadline) = self.undo.get(id).ok_or("undo expired")?;
                if undo_revision != revision || observed >= deadline {
                    return Err("undo expired".into());
                }
                Some(cards_edit::Undo {
                    revision: undo_revision,
                    deadline,
                })
            } else {
                None
            };
            let attachments = self.card_attachments(id, &source, action)?;
            let plan = cards_edit::Plan::prepare(
                &source,
                self.bundle.as_ref().ok_or("plugin unavailable")?,
                operation,
                &action.command(observed),
                &attachments,
                undo,
            )?;
            let capture = self.captured_task(plan.invocation())?;
            let projected = plan.capture(&capture)?;
            let committed = task_record(projected.card())?;
            let undo_deadline = if matches!(action, CardAction::Delete) {
                Some(observed.checked_add(8000).ok_or("undo clock overflow")?)
            } else {
                None
            };
            Ok((projected, committed, undo_deadline))
        })();
        let (projected, committed, undo_deadline) =
            prepared.map_err(|e| self.no_commit_if_absent(id, operation, e))?;
        let receipt = self.commit_card_edit(&projected)?;
        if let Some(deadline) = undo_deadline {
            self.undo.insert(id.into(), (receipt.revision, deadline));
        } else {
            self.undo.remove(id);
        }
        self.staged.retain(|(card, _), _| card != id);
        Ok(TaskCommit {
            receipt,
            committed,
            repeated: false,
        })
    }
}
impl Workbench {
    /// Result contains the operation's historical snapshot; read_versioned gets current content.
    pub fn edit_card(
        &mut self,
        operation: &str,
        id: &str,
        revision: u64,
        action: &CardAction,
    ) -> Result<TaskCommit> {
        self.local_state_mut()?
            .edit_card(operation, id, revision, action)
    }
}
