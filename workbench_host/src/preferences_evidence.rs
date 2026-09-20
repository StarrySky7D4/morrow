//! One canonical settings intent, one ordered pure-task batch and one content transaction.
//! Historical retries reuse the original observations and require fresh content authority.
use super::*;
use morrow_core::{
    task_evidence::{self, Evidence},
    transaction::{self, Lookup, Receipt, proto::command::Action as StoredAction},
};
use morrow_workbench_plugin::preferences;

const ID: &str = "morrow-studio-preferences";
const INTENT: &str = "morrow.studio.preferences-save.v1";
const WIRE_TYPE: &str = "morrow.studio.preferences.v1";
const HANDLER: &str = "studio.preferences";
const TOTAL_FUEL: u64 = 1_000_000_000;

enum Change {
    Create(CardRecord),
    Edit(ContentChange),
}
fn verify_body(body: &[u8], intent: &[u8]) -> Result<()> {
    if preferences::encode_wire(&preferences::decode_persistent(body)?)? != intent {
        return Err("stored preferences differ from the original intent".into());
    }
    Ok(())
}
impl WorkbenchState {
    pub(super) fn save_preferences_observed(
        &mut self,
        operation: &str,
        input: Vec<u8>,
    ) -> Result<Vec<u8>> {
        self.prepare_write()?;
        let preferences = preferences::decode_wire(&input)?;
        let intent = preferences::encode_wire(&preferences)?;
        // Also retain the existing individual 64 KiB page bound for a no-op.
        let pages = preferences::validation_pages(&preferences)?;
        if self.retry_preferences(operation, &intent, &pages)? {
            return Ok(intent);
        }
        let prior = self.host.store_local().card(ID)?;
        if let Some(prior) = &prior
            && (prior.summary().type_id != "org.morrow.studio"
                || prior.summary().format_version != 1)
        {
            return Err("preferences type".into());
        }
        // Freeze unknown-field preservation and the base revision before executing any page.
        let body = preferences::encode_persistent(
            &preferences,
            prior.as_ref().map(|v| v.body()).as_deref(),
        )?;
        if prior.as_ref().is_some_and(|prior| prior.body() == body) {
            // No content change means no operation receipt, evidence, or reserved operation ID.
            return Ok(intent);
        }
        let change = match prior {
            Some(prior) => Change::Edit(ContentChange {
                operation_id: operation.into(),
                card_id: ID.into(),
                expected_revision: prior.summary().revision,
                title: "工作台设置".into(),
                body,
                preview_text: "外观、日常小事与随身听设置".into(),
                attachments: None,
            }),
            None => Change::Create(CardRecord::new(
                ID,
                "org.morrow.studio",
                1,
                "工作台设置",
                body,
            )?),
        };
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or("task counter exhausted")?;
        let tasks = pages
            .iter()
            .enumerate()
            .map(|(index, page)| {
                Invocation::new_transform(
                    &format!("preferences-{}-{index}", self.counter),
                    Transform {
                        handler: HANDLER.into(),
                        input_type: WIRE_TYPE.into(),
                        output_type: WIRE_TYPE.into(),
                        input: page.clone(),
                    },
                )
            })
            .collect::<morrow_core::Result<Vec<_>>>()?;
        let captured = self.pool.record_transform_batch(
            self.manager.as_ref().ok_or("plugin manager unavailable")?,
            &mut self.host,
            self.plugin.as_ref().ok_or("plugin unavailable")?,
            &tasks,
            INTENT,
            &intent,
            TOTAL_FUEL,
        );
        self.finish_stopped_session();
        let (reports, evidence) = captured?.into_parts();
        if reports.len() != pages.len() {
            return Err("incomplete preferences validation batch".into());
        }
        for (report, page) in reports.iter().zip(&pages) {
            if report.execution.outcome != Ok(0)
                || report.execution.host_calls != 0
                || report.response.is_some()
            {
                return Err(
                    format!("preferences plugin failed: {:?}", report.execution.outcome).into(),
                );
            }
            if let Some(failure) = &report.failure {
                return Err(failure.message.clone().into());
            }
            let output = report
                .output
                .as_ref()
                .ok_or("missing preferences validation output")?;
            if output.type_id != WIRE_TYPE
                || preferences::decode_wire(&output.bytes)? != preferences::decode_wire(page)?
            {
                return Err("plugin altered preference validation page".into());
            }
        }
        self.commit_preferences(operation, &change, std::slice::from_ref(&evidence))?;
        Ok(intent)
    }

    fn retry_preferences(
        &mut self,
        operation: &str,
        intent: &[u8],
        pages: &[Vec<u8>],
    ) -> Result<bool> {
        // Global lookup rejects an operation used by another card or command family.
        if matches!(self.host.store_local().lookup(operation)?, Lookup::Absent) {
            return Ok(false);
        }
        let (commit, expected_receipt) = self
            .host
            .store_local()
            .operation_commit(ID, operation)?
            .ok_or("operation belongs to another content object")?;
        let evidence = self.host.store_local().operation_evidence(ID, operation)?;
        if evidence.len() != 1 || evidence[0].data().schema_version != task_evidence::BATCH_VERSION
        {
            return Err("existing operation has no supported original settings batch".into());
        }
        let batch = evidence[0]
            .data()
            .batch
            .as_ref()
            .ok_or("missing original settings batch")?;
        if batch.intent_type != INTENT
            || batch.intent != intent
            || batch.observations.len() != pages.len()
        {
            return Err("operation conflict: different original settings intent".into());
        }
        for (observation, page) in batch.observations.iter().zip(pages) {
            let invocation = Invocation::decode(&observation.invocation)?;
            let transform = invocation
                .transform()
                .ok_or("original settings task was not a transform")?;
            if transform.handler != HANDLER
                || transform.input_type != WIRE_TYPE
                || transform.output_type != WIRE_TYPE
                || transform.input != *page
            {
                return Err("operation conflict: different original settings pages".into());
            }
            if observation.exit_code != Some(0)
                || observation.fault != task_evidence::proto::StableFault::Unspecified as i32
                || observation.observed_host_calls != 0
            {
                return Err("original settings validation was unsuccessful".into());
            }
            let output = invocation.verify_output(&observation.completion)?;
            if preferences::decode_wire(&output.bytes)? != preferences::decode_wire(page)? {
                return Err("original settings validation altered a page".into());
            }
        }
        let command = transaction::decode_command(&commit.command)?;
        let change = match command.action.ok_or("missing original settings command")? {
            StoredAction::CreateCard(raw) => {
                let card = CardRecord::decode(&raw)?;
                let summary = card.summary();
                if summary.id != ID
                    || summary.type_id != "org.morrow.studio"
                    || summary.format_version != 1
                {
                    return Err("original operation is not settings creation".into());
                }
                verify_body(&card.body(), intent)?;
                Change::Create(card)
            }
            StoredAction::SetContent(value)
                if value.card_id == ID && value.attachments.is_none() =>
            {
                verify_body(&value.body, intent)?;
                Change::Edit(ContentChange {
                    operation_id: operation.into(),
                    card_id: value.card_id,
                    expected_revision: value.expected_revision,
                    title: value.title,
                    body: value.body,
                    preview_text: value.preview_text,
                    attachments: None,
                })
            }
            _ => return Err("operation conflict: different original settings route".into()),
        };
        let receipt = self.commit_preferences(operation, &change, &evidence)?;
        if receipt != expected_receipt {
            return Err("historical settings receipt mismatch".into());
        }
        Ok(true)
    }

    fn commit_preferences(
        &mut self,
        operation: &str,
        change: &Change,
        evidence: &[Evidence],
    ) -> Result<Receipt> {
        let kind = match change {
            Change::Create(_) => GrantKind::CreateContent,
            Change::Edit(_) => GrantKind::EditContent,
        };
        self.grant(ID, kind)?;
        let start = self.start;
        let connection = self
            .pool
            .root(self.plugin.as_ref().ok_or("plugin unavailable")?)?
            .connection();
        let result = match change {
            Change::Create(card) => self.host.create_content_with_evidence(
                connection,
                operation,
                card,
                evidence,
                || now(start),
            ),
            Change::Edit(change) => {
                self.host
                    .edit_content_with_evidence(connection, change, evidence, || now(start))
            }
        };
        self.revoke(ID, kind)?;
        Ok(result?)
    }
}
