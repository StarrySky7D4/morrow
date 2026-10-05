//! Development metadata projection of the original durable import schema.
use crate::{Result, err, hex, unhex, draft_bridge, editor_draft_staging};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImportWrite {
    pub card_id: String,
    pub draft_id: String,
    pub operation_id: String,
    pub expected_generation: String,
    pub name: String,
    pub kind: String,
    pub byte_length: String,
    pub sha256: String,
}
impl ImportWrite {
    pub fn request(&self) -> Result<editor_draft_staging::proto::ImportRequest> {
        let hash = unhex(&self.sha256)?;
        if hash.len() != 32 { return Err("InvalidImportHash".into()); }
        let request = editor_draft_staging::proto::ImportRequest {
            schema_version: 1,
            card_id: self.card_id.clone(), draft_id: self.draft_id.clone(), operation_id: self.operation_id.clone(),
            expected_generation: draft_bridge::number(&self.expected_generation)?,
            name: self.name.clone(), kind: self.kind.clone(), byte_length: draft_bridge::number(&self.byte_length)?, sha256: hash,
        };
        editor_draft_staging::validate_request(&request).map_err(err)?;
        Ok(request)
    }
}
#[derive(Debug, Serialize)]
pub struct ImportView {
    pub request: ImportWrite,
    pub asset_id: String,
    pub phase: String,
    pub current_active: bool,
    pub bytes_retained: bool,
    pub staging_revision: String,
    pub repeated: bool,
}
impl From<editor_draft_staging::DraftImportRecord> for ImportView {
    fn from(record: editor_draft_staging::DraftImportRecord) -> Self {
        let request = record.request;
        Self {
            request: ImportWrite { card_id: request.card_id, draft_id: request.draft_id, operation_id: request.operation_id,
                expected_generation: request.expected_generation.to_string(), name: request.name, kind: request.kind,
                byte_length: request.byte_length.to_string(), sha256: hex(&request.sha256) },
            asset_id: record.asset_id,
            phase: match record.phase { editor_draft_staging::DraftImportPhase::Pending => "pending", editor_draft_staging::DraftImportPhase::Ready => "ready", editor_draft_staging::DraftImportPhase::Retired => "retired" }.into(),
            current_active: record.current_active, bytes_retained: record.bytes_retained,
            staging_revision: record.staging_revision.to_string(), repeated: record.repeated,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct AssetView {
    pub id: String, pub name: String, pub kind: String,
    pub byte_length: String, pub sha256: String, pub media_type: String,
}
