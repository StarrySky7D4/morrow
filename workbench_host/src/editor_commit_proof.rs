//! Read-only proof of one historical captured editor business commit.
//! This deliberately does not inspect the current card, recovery journal, or guest.
use crate::{Result, Workbench, WorkbenchState, captured_cards, projection, projection_v2};
use morrow_core::{
    content::CardRecord,
    transaction::{self, proto::command::Action as StoredAction},
};
use morrow_workbench_plugin::Action as PluginAction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorCommitProof {
    pub id: String,
    pub operation: String,
    pub digest: [u8; 32],
    pub source_revision: u64,
    pub committed_revision: u64,
}

fn identity(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err("invalid editor commit identity".into());
    }
    Ok(())
}

impl WorkbenchState {
    pub(crate) fn inspect_editor_commit(
        &self,
        id: &str,
        operation: &str,
    ) -> Result<EditorCommitProof> {
        identity(id)?;
        identity(operation)?;
        let (commit, receipt) = self
            .host
            .store_local()
            .operation_commit(id, operation)?
            .ok_or("editor commit absent")?;
        let historical = self.host.store_local().operation_evidence(id, operation)?;
        if historical.len() != 1 {
            return Err("editor commit evidence count changed".into());
        }
        let evidence = &historical[0];
        let batch = evidence
            .data()
            .batch
            .as_ref()
            .ok_or("editor commit has no captured batch")?;
        let (source_revision, target) = match batch.intent_type.as_str() {
            captured_cards::INTENT_TYPE => {
                let projected = captured_cards::verify_commit(&commit, evidence)?;
                if projected.operation_id() != operation {
                    return Err("captured editor operation changed".into());
                }
                let source = CardRecord::decode(projected.source_card())?;
                if source.encode().as_slice() != projected.source_card()
                    || source.summary().id != id
                {
                    return Err("captured editor source changed".into());
                }
                let command = transaction::decode_command(projected.command())?;
                if !matches!(
                    command.action,
                    Some(StoredAction::SetVersionedContent(ref change))
                        if change.source_card.as_slice() == projected.source_card()
                ) {
                    return Err("captured editor command changed".into());
                }
                (source.summary().revision, projected.card().clone())
            }
            projection_v2::INTENT_TYPE => {
                let facts = projection_v2::decode(&batch.intent)?;
                let content = projection::decode_intent_v1(&facts.content)?;
                let projected = projection::verify_commit(&commit, evidence)?;
                let command = transaction::decode_command(&projected.command)?;
                if command.operation_id != operation {
                    return Err("legacy editor operation changed".into());
                }
                if content.prior.is_empty() {
                    if projected.original_request.action != PluginAction::Create
                        || !matches!(command.action, Some(StoredAction::CreateCard(_)))
                    {
                        return Err("editor Create capture changed".into());
                    }
                    (0, projected.card)
                } else {
                    let source = CardRecord::decode(&content.prior)?;
                    if source.encode() != content.prior || source.summary().id != id {
                        return Err("legacy editor source changed".into());
                    }
                    if projected.original_request.action != PluginAction::Edit
                        || !matches!(
                            command.action,
                            Some(StoredAction::SetContent(ref change))
                                if change.card_id == id
                                    && change.expected_revision == source.summary().revision
                        )
                    {
                        return Err("legacy editor command changed".into());
                    }
                    (source.summary().revision, projected.card)
                }
            }
            _ => return Err("operation is not a captured editor commit".into()),
        };
        let summary = target.summary();
        if summary.id != id
            || source_revision.checked_add(1) != Some(summary.revision)
            || (source_revision == 0 && summary.format_version != 1)
            || receipt.revision != summary.revision
            || commit.card_id != id
            || commit.operation_id != operation
        {
            return Err("editor commit revision or identity changed".into());
        }
        Ok(EditorCommitProof {
            id: id.into(),
            operation: operation.into(),
            digest: evidence.digest(),
            source_revision,
            committed_revision: summary.revision,
        })
    }
}

impl Workbench {
    /// Verify the exact original captured commit without replaying or changing it.
    pub fn inspect_editor_commit(&self, id: &str, operation: &str) -> Result<EditorCommitProof> {
        self.local_state()?.inspect_editor_commit(id, operation)
    }
}
