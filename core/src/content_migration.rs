//! Explicit forward-format proposal. Historical SetContent commands keep their meaning.
//! The exact source Card is retained in the command for independent reconstruction;
//! a host must still validate the chosen migrator and its evidence before submission.
use crate::{Error, Result, content::CardRecord, identity};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentMigration {
    pub operation_id: String,
    pub source_card: Vec<u8>,
    pub target_format_version: u32,
    pub body: Vec<u8>,
    pub preview_text: String,
}

impl ContentMigration {
    pub fn source(&self) -> Result<CardRecord> {
        CardRecord::decode(&self.source_card)
    }

    pub fn validate(&self) -> Result<()> {
        identity(&self.operation_id)?;
        if self.body.len() > crate::content::MAX_RECORD_BYTES || self.preview_text.len() > 16 * 1024
        {
            return Err(Error::Limit);
        }
        let source = self.source()?;
        if self.target_format_version <= source.summary().format_version {
            return Err(Error::Invalid("forward migration required"));
        }
        source
            .summary()
            .revision
            .checked_add(1)
            .ok_or(Error::Invalid("revision overflow"))?;
        Ok(())
    }

    pub fn propose(&self, current: &CardRecord) -> Result<CardRecord> {
        self.validate()?;
        // Also binds unknown fields, attachments, relations, type and source encoding.
        // No source rebase is allowed, even when a caller reused the same revision.
        if current.encode() != self.source_card {
            return Err(Error::RevisionConflict);
        }
        current.with_migrated_content(self.target_format_version, &self.body, &self.preview_text)
    }
}
