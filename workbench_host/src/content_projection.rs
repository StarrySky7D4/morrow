//! Freeze host facts before execution, then derive the only command submitted to storage.
use super::*;
use morrow_core::{
    task_evidence::{
        self, Evidence,
        proto::{Batch, Observation, TaskEvidence},
    },
    transaction::{self, Receipt, proto::command::Action as StoredAction},
};

impl WorkbenchState {
    pub(super) fn project_content(
        &mut self,
        operation: &str,
        id: &str,
        request: Request,
        prior: Option<&CardRecord>,
        undo: Option<projection::Undo>,
    ) -> Result<(projection::Projection, Evidence)> {
        // The default guest can retain current assets or select proposed assets. Resolve
        // their host facts now; a later result cannot introduce another staged identity.
        let existing = prior.map(CardRecord::attachments).unwrap_or_default();
        let mut candidates = BTreeMap::new();
        for asset in request
            .current
            .assets
            .iter()
            .chain(&request.proposed.assets)
        {
            if let Some(item) = self
                .staged
                .get(&(id.into(), asset.id.clone()))
                .or_else(|| existing.iter().find(|item| item.id == asset.id))
            {
                candidates.insert(asset.id.clone(), item.clone());
            }
        }
        let observed_now = request.now_ms;
        // Bounds and prior/clock/undo facts are checked before invoking guest code.
        projection::prepare_intent(operation, id, prior, &[], observed_now, undo)?;
        let (response, captured) = self.run_observed(request, true)?;
        let captured = captured.ok_or("missing captured evidence")?;
        let next = response.idea;
        next.validate()?;
        if next.id != id {
            return Err("plugin changed identity".into());
        }
        let attachments = next
            .assets
            .iter()
            .map(|asset| {
                let item = candidates
                    .get(&asset.id)
                    .ok_or("attachment outside selected card")?;
                if asset.bytes != item.byte_length || asset.name != item.display_name {
                    return Err("attachment metadata mismatch".into());
                }
                Ok(item.clone())
            })
            .collect::<Result<Vec<_>>>()?;
        let intent =
            projection::prepare_intent(operation, id, prior, &attachments, observed_now, undo)?;
        let actual = captured.data();
        if actual.schema_version != task_evidence::VERSION || actual.batch.is_some() {
            return Err("expected actual single-task capture".into());
        }
        // Copy only the observation produced by the real capture. The new digest is
        // computed by the evidence encoder; this does not grant projection authority.
        let evidence = task_evidence::encode(TaskEvidence {
            schema_version: task_evidence::BATCH_VERSION,
            package_archive: actual.package_archive.clone(),
            batch: Some(Batch {
                intent_type: projection::INTENT_TYPE.into(),
                intent,
                total_fuel: 20_000_000,
                observations: vec![Observation {
                    invocation: actual.invocation.clone(),
                    budget: actual.budget,
                    backend: actual.backend.clone(),
                    completion: actual.completion.clone(),
                    fault: actual.fault,
                    exit_code: actual.exit_code,
                    observed_host_calls: actual.observed_host_calls,
                    fuel_remaining: actual.fuel_remaining,
                }],
            }),
            ..Default::default()
        })?;
        let projection = projection::derive(&evidence)?;
        Ok((projection, evidence))
    }

    pub(super) fn commit_projection(
        &mut self,
        projection: &projection::Projection,
        evidence: &[Evidence],
    ) -> Result<Receipt> {
        let command = transaction::decode_command(&projection.command)?;
        let id = projection.card.summary().id;
        let operation = command.operation_id;
        match command.action.ok_or("missing projected command")? {
            StoredAction::CreateCard(raw) => {
                let card = CardRecord::decode(&raw)?;
                if card.encode() != projection.card.encode() {
                    return Err("projected create differs from projected card".into());
                }
                self.grant(&id, GrantKind::CreateContent)?;
                let start = self.start;
                let result = self.host.create_content_with_evidence(
                    self.pool
                        .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?
                        .connection(),
                    &operation,
                    &card,
                    evidence,
                    || now(start),
                );
                self.revoke(&id, GrantKind::CreateContent)?;
                Ok(result?)
            }
            StoredAction::SetContent(value) => {
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
                                        .map_err(|_| "invalid projected attachment digest")?,
                                })
                            })
                            .collect::<Result<Vec<_>>>()
                    })
                    .transpose()?;
                let change = ContentChange {
                    operation_id: operation,
                    card_id: value.card_id,
                    expected_revision: value.expected_revision,
                    title: value.title,
                    body: value.body,
                    preview_text: value.preview_text,
                    attachments,
                };
                if change.card_id != id {
                    return Err("projected content target differs".into());
                }
                self.grant(&id, GrantKind::EditContent)?;
                let start = self.start;
                let result = self.host.edit_content_with_evidence(
                    self.pool
                        .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?
                        .connection(),
                    &change,
                    evidence,
                    || now(start),
                );
                self.revoke(&id, GrantKind::EditContent)?;
                Ok(result?)
            }
            _ => Err("unsupported projected command".into()),
        }
    }

    pub(super) fn retry_projected(
        &mut self,
        id: &str,
        intent: &Request,
        revision: Option<u64>,
        commit: &transaction::proto::Commit,
        expected_receipt: &Receipt,
        evidence: &[Evidence],
    ) -> Result<Option<Record>> {
        let evidence_one = evidence.first().ok_or("missing historical projection")?;
        let projection = projection::verify_commit(commit, evidence_one)?;
        if projection.card.summary().id != id {
            return Err("operation belongs to another content object".into());
        }
        let original = &projection.original_request;
        let mut expected = intent.clone();
        if revision.is_some() {
            expected.current = original.current.clone();
            expected.now_ms = original.now_ms;
        }
        let observation = &evidence_one
            .data()
            .batch
            .as_ref()
            .ok_or("missing batch")?
            .observations
            .last()
            .ok_or("missing final workbench observation")?;
        let invocation = Invocation::decode(&observation.invocation)?;
        let transform = invocation
            .transform()
            .ok_or("original task was not a transform")?;
        if codec::encode_request(&expected)? != transform.input {
            return Err("operation conflict: different original request".into());
        }
        let command = transaction::decode_command(&projection.command)?;
        match command.action.ok_or("missing projected command")? {
            StoredAction::CreateCard(_)
                if revision.is_none() && intent.action == Action::Create => {}
            StoredAction::SetContent(value) if revision == Some(value.expected_revision) => {}
            _ => return Err("operation conflict: different route or base revision".into()),
        }
        let receipt = self.commit_projection(&projection, evidence)?;
        if &receipt != expected_receipt {
            return Err("historical receipt mismatch".into());
        }
        // Original projection only: never overwrite a later revision or renew undo state.
        Ok(Some(Self::decode(&projection.card)?))
    }
}
