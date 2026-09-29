//! Host-bound business edit proposal. Owning this value grants no permissions.
use crate::{Error, Result, content::CardRecord, identity, title};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentChange {
    pub operation_id: String,
    pub card_id: String,
    pub expected_revision: u64,
    pub title: String,
    pub body: Vec<u8>,
    pub preview_text: String,
    pub attachments: Option<Vec<crate::content::Attachment>>,
}
impl ContentChange {
    pub fn validate(&self) -> Result<()> {
        identity(&self.operation_id)?;
        identity(&self.card_id)?;
        title(&self.title)?;
        if self.expected_revision == 0
            || self.body.len() > crate::content::MAX_RECORD_BYTES
            || self.preview_text.len() > 16 * 1024
        {
            return Err(Error::Limit);
        }
        if let Some(items) = &self.attachments {
            if items.len() > 1024 {
                return Err(Error::Limit);
            }
            let mut ids = std::collections::BTreeSet::new();
            for item in items {
                item.validate()?;
                if !ids.insert(&item.id) {
                    return Err(Error::Invalid("duplicate attachment id"));
                }
            }
        }
        Ok(())
    }
    pub fn propose(&self, current: &CardRecord) -> Result<CardRecord> {
        self.validate()?;
        if self.card_id != current.summary().id {
            return Err(Error::Invalid("content target"));
        }
        current.with_content_and_attachments(
            self.expected_revision,
            &self.title,
            &self.body,
            &self.preview_text,
            self.attachments.as_deref(),
        )
    }
}
