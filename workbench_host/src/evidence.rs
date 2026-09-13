//! Trusted historical content retry. A committed observation is never re-executed.
use super::*;
use morrow_core::{
    task_evidence::Evidence,
    transaction::{self, Lookup, proto::command::Action as StoredAction},
};

impl Workbench {
    /// Host-local historical read; this exposes no guest grant or replay authority.
    pub fn operation_evidence(&self, card: &str, operation: &str) -> Result<Vec<Evidence>> {
        Ok(self
            .host
            .store_local()
            .operation_evidence(card, operation)?)
    }
    pub(super) fn retry_observed(
        &mut self,
        operation: &str,
        id: &str,
        intent: &Request,
        revision: Option<u64>,
    ) -> Result<Option<Record>> {
        if matches!(self.host.store_local().lookup(operation)?, Lookup::Absent) {
            return Ok(None);
        }
        let (commit, expected_receipt) = self
            .host
            .store_local()
            .operation_commit(id, operation)?
            .ok_or("operation belongs to another content object")?;
        let evidence = self.host.store_local().operation_evidence(id, operation)?;
        if evidence.len() != 1 {
            return Err("existing operation has no supported original workbench intent".into());
        }
        if evidence[0]
            .data()
            .batch
            .as_ref()
            .is_some_and(|b| b.intent_type == crate::projection_v2::INTENT_TYPE)
        {
            return Err(
                "captured operation retry requires its original scope and editor endpoint".into(),
            );
        }
        if evidence[0].data().schema_version == morrow_core::task_evidence::BATCH_VERSION {
            return self.retry_projected(
                id,
                intent,
                revision,
                &commit,
                &expected_receipt,
                &evidence,
            );
        }
        let invocation = Invocation::decode(&evidence[0].data().invocation)?;
        let transform = invocation
            .transform()
            .ok_or("original task was not a transform")?;
        if transform.handler != "workbench.command"
            || transform.input_type != "morrow.workbench.request.v1"
            || transform.output_type != "morrow.workbench.response.v1"
        {
            return Err("existing operation uses a different task route".into());
        }
        let original = codec::decode_request(&transform.input)?;
        // Current content and the clock are host observations, not new user intent.
        // Compare every other field against the exact original request representation.
        let mut expected = intent.clone();
        if revision.is_some() {
            expected.current = original.current.clone();
            expected.now_ms = original.now_ms;
        }
        if codec::encode_request(&expected)? != transform.input {
            return Err("operation conflict: different original request".into());
        }
        let command = transaction::decode_command(&commit.command)?;
        let (record, receipt) = match command.action.ok_or("missing original command")? {
            StoredAction::CreateCard(raw)
                if revision.is_none() && intent.action == Action::Create =>
            {
                let card = CardRecord::decode(&raw)?;
                let record = Self::decode(&card)?;
                self.grant(id, GrantKind::CreateContent)?;
                let start = self.start;
                let result = self.host.create_content_with_evidence(
                    self.pool
                        .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?
                        .connection(),
                    operation,
                    &card,
                    &evidence,
                    || now(start),
                );
                self.revoke(id, GrantKind::CreateContent)?;
                (record, result?)
            }
            StoredAction::SetContent(value) if revision == Some(value.expected_revision) => {
                let record = Record {
                    idea: persistence::decode(id, &value.title, &value.body)?,
                    revision: expected_receipt.revision,
                };
                let attachments = value
                    .attachments
                    .map(|list| {
                        list.items
                            .into_iter()
                            .map(|a| {
                                Ok(Attachment {
                                    id: a.id,
                                    display_name: a.display_name,
                                    media_type: a.media_type,
                                    byte_length: a.byte_length,
                                    sha256: a
                                        .sha256
                                        .try_into()
                                        .map_err(|_| "invalid historical attachment digest")?,
                                })
                            })
                            .collect::<Result<Vec<_>>>()
                    })
                    .transpose()?;
                let change = ContentChange {
                    operation_id: operation.into(),
                    card_id: value.card_id,
                    expected_revision: value.expected_revision,
                    title: value.title,
                    body: value.body,
                    preview_text: value.preview_text,
                    attachments,
                };
                self.grant(id, GrantKind::EditContent)?;
                let start = self.start;
                let result = self.host.edit_content_with_evidence(
                    self.pool
                        .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?
                        .connection(),
                    &change,
                    &evidence,
                    || now(start),
                );
                self.revoke(id, GrantKind::EditContent)?;
                (record, result?)
            }
            _ => return Err("operation conflict: different route or base revision".into()),
        };
        if receipt != expected_receipt {
            return Err("historical receipt mismatch".into());
        }
        // No new undo deadline, staged-attachment cleanup, or content projection from the
        // latest revision: the caller receives exactly this operation's historical result.
        Ok(Some(record))
    }
}
