//! A format-aware content edit bound to the complete original Card bytes.
//! Hosts validate the format-specific body and any guest evidence before submission.
use crate::{
    Error, Result,
    content::{Attachment, CardRecord},
    content_change::ContentChange,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionedContentChange {
    pub operation_id: String,
    pub source_card: Vec<u8>,
    pub title: String,
    pub body: Vec<u8>,
    pub preview_text: String,
    pub attachments: Option<Vec<Attachment>>,
}

impl VersionedContentChange {
    pub fn source(&self) -> Result<CardRecord> {
        CardRecord::decode(&self.source_card)
    }

    pub fn validate(&self) -> Result<()> {
        let source = self.source()?;
        let summary = source.summary();
        ContentChange {
            operation_id: self.operation_id.clone(),
            card_id: summary.id,
            expected_revision: summary.revision,
            title: self.title.clone(),
            body: self.body.clone(),
            preview_text: self.preview_text.clone(),
            attachments: self.attachments.clone(),
        }
        .validate()?;
        if summary.revision == u64::MAX {
            return Err(Error::Invalid("revision overflow"));
        }
        Ok(())
    }

    pub fn propose(&self, current: &CardRecord) -> Result<CardRecord> {
        self.validate()?;
        if current.encode() != self.source_card {
            return Err(Error::RevisionConflict);
        }
        let summary = current.summary();
        current.with_content_and_attachments(
            summary.revision,
            &self.title,
            &self.body,
            &self.preview_text,
            self.attachments.as_deref(),
        )
    }
}
