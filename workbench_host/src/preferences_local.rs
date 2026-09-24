//! Private UI settings transactions owned by the host, not by an enabled guest.
//! Content commands and the public guest-evidence save route retain their gates.
use crate::{Result, WorkbenchState, now};
use morrow_core::{
    content::CardRecord,
    content_change::ContentChange,
    lifecycle::GrantKind,
    transaction::{self, Lookup, proto::command::Action},
};
use morrow_workbench_plugin::preferences;
const ID: &str = "morrow-studio-preferences";
const TYPE: &str = "org.morrow.studio";

impl WorkbenchState {
    pub(crate) fn preferences_commit_matches(
        &self,
        operation: &str,
        intent: &[u8],
    ) -> Result<bool> {
        let Some((commit, _)) = self.host.store_local().operation_commit(ID, operation)? else {
            return Ok(false);
        };
        let command = transaction::decode_command(&commit.command)?;
        let body = match command.action {
            Some(Action::CreateCard(raw)) => {
                let card = CardRecord::decode(&raw)?;
                if card.summary().id != ID
                    || card.summary().type_id != TYPE
                    || card.summary().format_version != 1
                {
                    return Ok(false);
                }
                card.body()
            }
            Some(Action::SetContent(change))
                if change.card_id == ID && change.attachments.is_none() =>
            {
                change.body
            }
            _ => return Ok(false),
        };
        Ok(preferences::encode_wire(&preferences::decode_persistent(&body)?)? == intent)
    }

    pub(crate) fn save_local_preferences(
        &mut self,
        operation: &str,
        input: Vec<u8>,
    ) -> Result<Vec<u8>> {
        self.host.prepare_write()?;
        let value = preferences::decode_wire(&input)
            .map_err(|_| crate::preferences_evidence::PreferencesRejected)?;
        preferences::validation_pages(&value)
            .map_err(|_| crate::preferences_evidence::PreferencesRejected)?;
        let intent = preferences::encode_wire(&value)?;
        // An original committed proposal is observed, never reapplied over later settings.
        if !matches!(self.host.store_local().lookup(operation)?, Lookup::Absent) {
            if self.preferences_commit_matches(operation, &intent)? {
                return Ok(intent);
            }
            return Err("preferences operation conflict".into());
        }
        let prior = self.host.store_local().card(ID)?;
        if let Some(card) = &prior {
            if card.summary().type_id != TYPE || card.summary().format_version != 1 {
                return Err("preferences type".into());
            }
        }
        let body =
            preferences::encode_persistent(&value, prior.as_ref().map(|c| c.body()).as_deref())?;
        if prior.as_ref().is_some_and(|c| c.body() == body) {
            return Ok(intent);
        }
        let kind = if prior.is_some() {
            GrantKind::EditContent
        } else {
            GrantKind::CreateContent
        };
        let mut connection = self.host.connect()?;
        let start = self.start;
        let result = (|| -> Result<()> {
            let tick = now(start);
            self.host
                .grant(&mut connection, kind, ID, tick.saturating_add(30_000), tick)?;
            if let Some(prior) = prior {
                self.host.edit_content(
                    &connection,
                    &ContentChange {
                        operation_id: operation.into(),
                        card_id: ID.into(),
                        expected_revision: prior.summary().revision,
                        title: "工作台设置".into(),
                        body,
                        preview_text: "外观、日常小事与随身听设置".into(),
                        attachments: None,
                    },
                    || now(start),
                )?;
            } else {
                let card = CardRecord::new(ID, TYPE, 1, "工作台设置", body)?;
                self.host
                    .create_content(&connection, operation, &card, || now(start))?;
            }
            Ok(())
        })();
        let disconnected = self.host.disconnect(&connection);
        result?;
        disconnected?;
        Ok(intent)
    }
}
impl WorkbenchState {
    pub(crate) fn prepare_preferences_write(&mut self) -> Result<()> {
        self.host.prepare_write()?;
        Ok(())
    }
}
