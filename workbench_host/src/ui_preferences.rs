//! Trusted presentation preference: one core-owned record, no Dart side database.
use crate::{Result, Workbench, WorkbenchState, now};
use morrow_core::{content::CardRecord, content_change::ContentChange, lifecycle::GrantKind};
use prost::Message;
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.host.ui.v1.rs"));
}
const ID: &str = "morrow-ui-preferences";
const TYPE: &str = "org.morrow.host.ui-preferences";
const TITLE: &str = "UI preferences";
fn validate(locale: &str) -> Result<()> {
    if !matches!(locale, "system" | "zh" | "en") {
        return Err("unsupported UI locale".into());
    }
    Ok(())
}
impl WorkbenchState {
    pub fn read_ui_locale(&self) -> Result<(String, u64)> {
        let Some(card) = self.host.store_local().card(ID)? else {
            return Ok(("system".into(), 0));
        };
        let summary = card.summary();
        if summary.type_id != TYPE || summary.format_version != 1 || card.body().len() > 64 {
            return Err("invalid UI preferences record".into());
        }
        let body = card.body();
        let value = proto::Preferences::decode(body.as_slice())?;
        if value.schema_version != 1 || value.encode_to_vec() != body {
            return Err("unsupported UI preferences".into());
        }
        validate(&value.locale)?;
        Ok((value.locale, summary.revision))
    }
    /// Expected revision and operation ID are supplied on the private host channel.
    /// Retrying uses the same create/edit proposal even after the record has changed.
    pub fn save_ui_locale(
        &mut self,
        operation: &str,
        expected_revision: u64,
        locale: &str,
    ) -> Result<u64> {
        validate(locale)?;
        let _ = self.read_ui_locale()?; // Never overwrite an unknown/corrupt record.
        self.host.prepare_write()?;
        let body = proto::Preferences {
            schema_version: 1,
            locale: locale.into(),
        }
        .encode_to_vec();
        let kind = if expected_revision == 0 {
            GrantKind::CreateContent
        } else {
            GrantKind::EditContent
        };
        let mut connection = self.host.connect()?;
        let start = self.start;
        let result = (|| -> Result<u64> {
            let tick = now(start);
            self.host
                .grant(&mut connection, kind, ID, tick.saturating_add(30_000), tick)?;
            let receipt = if expected_revision == 0 {
                let card = CardRecord::new(ID, TYPE, 1, TITLE, body)?;
                self.host
                    .create_content(&connection, operation, &card, || now(start))?
            } else {
                self.host.edit_content(
                    &connection,
                    &ContentChange {
                        operation_id: operation.into(),
                        card_id: ID.into(),
                        expected_revision,
                        title: TITLE.into(),
                        body,
                        preview_text: String::new(),
                        attachments: None,
                    },
                    || now(start),
                )?
            };
            Ok(receipt.revision)
        })();
        let disconnected = self.host.disconnect(&connection);
        let revision = result?;
        disconnected?;
        // A sealing failure cannot turn an already committed locale into an uncommitted result.
        let _ = self.host.flush_pending();
        Ok(revision)
    }
}

impl Workbench {
    pub fn read_ui_locale(&self) -> Result<(String, u64)> {
        self.local_state()?.read_ui_locale()
    }

    pub fn save_ui_locale(
        &mut self,
        operation: &str,
        expected_revision: u64,
        locale: &str,
    ) -> Result<u64> {
        self.local_state_mut()?
            .save_ui_locale(operation, expected_revision, locale)
    }
}
