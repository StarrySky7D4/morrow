//! Trusted presentation preference: one core-owned record, no Dart side database.
use crate::{Result, Workbench, WorkbenchState, now};
use morrow_core::{content::CardRecord, content_change::ContentChange, lifecycle::GrantKind};
use prost::Message;
pub(crate) mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.host.ui.v1.rs"));
}
const ID: &str = "morrow-ui-preferences";
const TYPE: &str = "org.morrow.host.ui-preferences";
const TITLE: &str = "UI preferences";
include!("generated_ui_locales.rs");
fn validate(locale: &str) -> Result<()> {
    if locale != "system" && !UI_LOCALES.contains(&locale) {
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

/// Host-owned appearance metadata; no arbitrary paths or font bytes are accepted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontPreference {
    pub family: String,
    pub asset: String,
    pub name: String,
}
impl FontPreference {
    fn validate(&self) -> Result<()> {
        let valid_text = |s: &str, max| s.len() <= max && !s.chars().any(|c| c.is_control());
        if !valid_text(&self.family, 128)
            || self.family.trim() != self.family
            || !valid_text(&self.name, 255)
            || (self.asset.is_empty() && !self.name.is_empty())
            || (!self.asset.is_empty()
                && (self.asset.len() != 64
                    || !self
                        .asset
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    || !self.family.is_empty()
                    || self.name.is_empty()))
        {
            return Err("invalid font preference".into());
        }
        Ok(())
    }
}
const FONT_ID: &str = "morrow-ui-font";
const FONT_TYPE: &str = "org.morrow.host.ui-font";
impl WorkbenchState {
    pub(crate) fn read_ui_font(&self) -> Result<(FontPreference, u64)> {
        let Some(card) = self.host.store_local().card(FONT_ID)? else {
            return Ok((FontPreference::default(), 0));
        };
        let summary = card.summary();
        if summary.type_id != FONT_TYPE || summary.format_version != 1 || card.body().len() > 1024 {
            return Err("invalid font preference record".into());
        }
        let body = card.body();
        let value = proto::FontPreference::decode(body.as_slice())?;
        if value.schema_version != 1 || value.encode_to_vec() != body {
            return Err("unsupported font preference".into());
        }
        let font = FontPreference {
            family: value.family,
            asset: value.asset,
            name: value.name,
        };
        font.validate()?;
        Ok((font, summary.revision))
    }
    pub(crate) fn save_ui_font(
        &mut self,
        operation: &str,
        revision: u64,
        font: &FontPreference,
    ) -> Result<u64> {
        font.validate()?;
        let _ = self.read_ui_font()?;
        self.host.prepare_write()?;
        let body = proto::FontPreference {
            schema_version: 1,
            family: font.family.clone(),
            asset: font.asset.clone(),
            name: font.name.clone(),
        }
        .encode_to_vec();
        let kind = if revision == 0 {
            GrantKind::CreateContent
        } else {
            GrantKind::EditContent
        };
        let mut connection = self.host.connect()?;
        let start = self.start;
        let result = (|| -> Result<u64> {
            let tick = now(start);
            self.host.grant(
                &mut connection,
                kind,
                FONT_ID,
                tick.saturating_add(30_000),
                tick,
            )?;
            let receipt = if revision == 0 {
                let card = CardRecord::new(FONT_ID, FONT_TYPE, 1, "UI font", body)?;
                self.host
                    .create_content(&connection, operation, &card, || now(start))?
            } else {
                self.host.edit_content(
                    &connection,
                    &ContentChange {
                        operation_id: operation.into(),
                        card_id: FONT_ID.into(),
                        expected_revision: revision,
                        title: "UI font".into(),
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
        let _ = self.host.flush_pending();
        Ok(revision)
    }
}
impl Workbench {
    pub fn read_ui_font(&self) -> Result<(FontPreference, u64)> {
        self.local_state()?.read_ui_font()
    }
    pub fn save_ui_font(
        &mut self,
        operation: &str,
        revision: u64,
        font: &FontPreference,
    ) -> Result<u64> {
        self.local_state_mut()?
            .save_ui_font(operation, revision, font)
    }
}
