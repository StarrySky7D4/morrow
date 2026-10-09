//! JSON presentation of the original editor-draft protobuf model. This is a
//! trusted development bridge, not the production private capture transport.
use crate::{Result, err, hex};
use morrow_editor_draft_model::proto;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextValue {
    text: String,
    selection_base: i32,
    selection_extent: i32,
    affinity: u32,
    directional: bool,
    composing_start: i32,
    composing_end: i32,
}
impl From<TextValue> for proto::TextValue {
    fn from(v: TextValue) -> Self {
        Self {
            text: v.text,
            selection_base: v.selection_base,
            selection_extent: v.selection_extent,
            affinity: v.affinity,
            directional: v.directional,
            composing_start: v.composing_start,
            composing_end: v.composing_end,
        }
    }
}
impl From<&proto::TextValue> for TextValue {
    fn from(v: &proto::TextValue) -> Self {
        Self {
            text: v.text.clone(),
            selection_base: v.selection_base,
            selection_extent: v.selection_extent,
            affinity: v.affinity,
            directional: v.directional,
            composing_start: v.composing_start,
            composing_end: v.composing_end,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Values {
    title: Option<TextValue>,
    description: Option<TextValue>,
    hypothesis: Option<TextValue>,
    conclusion: Option<TextValue>,
    todos: Option<TextValue>,
    category: String,
    stage: String,
}
impl From<Values> for proto::Values {
    fn from(v: Values) -> Self {
        Self {
            title: v.title.map(Into::into),
            description: v.description.map(Into::into),
            hypothesis: v.hypothesis.map(Into::into),
            conclusion: v.conclusion.map(Into::into),
            todos: v.todos.map(Into::into),
            category: v.category,
            stage: v.stage,
        }
    }
}
impl From<&proto::Values> for Values {
    fn from(v: &proto::Values) -> Self {
        Self {
            title: v.title.as_ref().map(Into::into),
            description: v.description.as_ref().map(Into::into),
            hypothesis: v.hypothesis.as_ref().map(Into::into),
            conclusion: v.conclusion.as_ref().map(Into::into),
            todos: v.todos.as_ref().map(Into::into),
            category: v.category.clone(),
            stage: v.stage.clone(),
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AssetSelection {
    origin: u32,
    asset_id: String,
    aliases: Vec<String>,
}
impl From<AssetSelection> for proto::AssetSelection {
    fn from(v: AssetSelection) -> Self {
        Self {
            origin: v.origin,
            asset_id: v.asset_id,
            aliases: v.aliases,
        }
    }
}
impl From<&proto::AssetSelection> for AssetSelection {
    fn from(v: &proto::AssetSelection) -> Self {
        Self {
            origin: v.origin,
            asset_id: v.asset_id.clone(),
            aliases: v.aliases.clone(),
        }
    }
}
#[derive(Debug, Serialize)]
pub struct StoredAsset {
    selection: AssetSelection,
    pin_id: String,
    display_name: String,
    media_type: String,
    byte_length: String,
    sha256: String,
}
impl StoredAsset {
    fn from_proto(v: &proto::StoredAsset) -> Result<Self> {
        Ok(Self {
            selection: v
                .selection
                .as_ref()
                .ok_or("DraftAssetSelectionMissing")?
                .into(),
            pin_id: v.pin_id.clone(),
            display_name: v.display_name.clone(),
            media_type: v.media_type.clone(),
            byte_length: v.byte_length.to_string(),
            sha256: hex(&v.sha256),
        })
    }
}
/// The UI observes text and selected identities together. On the write wire,
/// the same selections live at Write.assets, exactly as in the original schema.
#[derive(Debug, Serialize)]
pub struct ViewValues {
    #[serde(flatten)]
    text: Values,
    assets: Vec<AssetSelection>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Write {
    card_id: String,
    draft_id: String,
    source_kind: u32,
    source_revision: String,
    expected_generation: String,
    operation_id: String,
    values: Option<Values>,
    assets: Vec<AssetSelection>,
}
pub fn number(v: &str) -> Result<u64> {
    let n: u64 = v.parse().map_err(|_| "InvalidDraftInteger")?;
    if n.to_string() != v {
        return Err("InvalidDraftInteger".into());
    }
    Ok(n)
}
impl Write {
    pub fn request(self) -> Result<proto::WriteRequest> {
        let request = self.raw_request()?;
        morrow_editor_draft_model::validate_request(&request).map_err(err)?;
        Ok(request)
    }
    fn raw_request(self) -> Result<proto::WriteRequest> {
        let request = proto::WriteRequest {
            schema_version: 1,
            card_id: self.card_id,
            draft_id: self.draft_id,
            source_kind: self.source_kind,
            source_revision: number(&self.source_revision)?,
            expected_generation: number(&self.expected_generation)?,
            operation_id: self.operation_id,
            values: self.values.map(Into::into),
            assets: self.assets.into_iter().map(Into::into).collect(),
            ..Default::default()
        };
        Ok(request)
    }
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fork {
    child: Write,
    parent_draft_id: String,
    parent_generation: String,
    parent_save_operation: String,
    parent_request_sha256: String,
}
fn digest(value: &str) -> Result<Vec<u8>> {
    if value.len() != 64 {
        return Err("InvalidDraftDigest".into());
    }
    crate::unhex(value)
}
impl Fork {
    pub fn request(self) -> Result<(proto::WriteRequest, proto::DevelopmentForkLink)> {
        let child = self.child.raw_request()?;
        let link = proto::DevelopmentForkLink {
            schema_version: 1,
            parent_draft_id: self.parent_draft_id,
            parent_generation: number(&self.parent_generation)?,
            parent_save_operation: self.parent_save_operation,
            parent_request_sha256: digest(&self.parent_request_sha256)?,
            child_operation: child.operation_id.clone(),
        };
        crate::editor_draft::validate_fork_request(&child, &link)?;
        Ok((child, link))
    }
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ForkRetirement {
    card_id: String,
    child_draft_id: String,
    child_operation: String,
    parent_draft_id: String,
    parent_generation: String,
    parent_save_operation: String,
    parent_request_sha256: String,
    operation_id: String,
}
impl ForkRetirement {
    pub fn request(self) -> Result<(String, proto::DevelopmentForkRetirement)> {
        Ok((
            self.card_id,
            proto::DevelopmentForkRetirement {
                schema_version: 1,
                child_draft_id: self.child_draft_id,
                operation_id: self.operation_id,
                fork_link: Some(proto::DevelopmentForkLink {
                    schema_version: 1,
                    parent_draft_id: self.parent_draft_id,
                    parent_generation: number(&self.parent_generation)?,
                    parent_save_operation: self.parent_save_operation,
                    parent_request_sha256: digest(&self.parent_request_sha256)?,
                    child_operation: self.child_operation,
                }),
            },
        ))
    }
}
#[derive(Debug, Serialize)]
pub struct ForkLinkView {
    schema_version: u32,
    parent_draft_id: String,
    parent_generation: String,
    parent_save_operation: String,
    parent_request_sha256: String,
    child_operation: String,
}
impl From<&proto::DevelopmentForkLink> for ForkLinkView {
    fn from(v: &proto::DevelopmentForkLink) -> Self {
        Self {
            schema_version: v.schema_version,
            parent_draft_id: v.parent_draft_id.clone(),
            parent_generation: v.parent_generation.to_string(),
            parent_save_operation: v.parent_save_operation.clone(),
            parent_request_sha256: hex(&v.parent_request_sha256),
            child_operation: v.child_operation.clone(),
        }
    }
}
#[derive(Debug, Serialize)]
pub struct ForkRetirementView {
    schema_version: u32,
    child_draft_id: String,
    operation_id: String,
    fork_link: ForkLinkView,
}
#[derive(Debug, Serialize)]
pub struct Scope {
    card_id: String,
    draft_id: String,
    source_kind: u32,
    source_revision: String,
    source: String,
}
#[derive(Debug, Serialize)]
pub struct View {
    scope: Scope,
    values: ViewValues,
    assets: Vec<StoredAsset>,
    consumed_imports: Vec<String>,
    operation_id: String,
    generation: String,
    current_generation: String,
    active: bool,
    current_active: bool,
    repeated: bool,
    request_sha256: String,
    fork_link: Option<ForkLinkView>,
    fork_retirement: Option<ForkRetirementView>,
    business_link: Option<crate::editor_handoff::LinkView>,
    business_retirement: Option<crate::editor_handoff::RetirementView>,
}
impl View {
    pub fn from_record(record: crate::editor_draft::DraftRecord) -> Result<Self> {
        let request = record.slot.request.as_ref().ok_or("DraftRequestMissing")?;
        let values = request.values.as_ref().ok_or("DraftValuesMissing")?;
        Ok(Self {
            scope: Scope {
                card_id: request.card_id.clone(),
                draft_id: request.draft_id.clone(),
                source_kind: request.source_kind,
                source_revision: request.source_revision.to_string(),
                source: hex(&record.slot.source_card),
            },
            values: ViewValues {
                text: values.into(),
                assets: request.assets.iter().map(Into::into).collect(),
            },
            assets: record
                .slot
                .assets
                .iter()
                .map(StoredAsset::from_proto)
                .collect::<Result<Vec<_>>>()?,
            consumed_imports: record.slot.consumed_imports.clone(),
            operation_id: request.operation_id.clone(),
            generation: record.slot.generation.to_string(),
            current_generation: record.current_generation.to_string(),
            active: record.slot.active,
            current_active: record.current_active,
            repeated: record.repeated,
            request_sha256: crate::editor_draft::request_sha256(request),
            fork_link: record.slot.development_fork_link.as_ref().map(Into::into),
            business_link: record.slot.development_business_link.as_ref().map(crate::editor_handoff::LinkView::from_proto).transpose()?,
            business_retirement: record.slot.development_business_retirement.as_ref().map(crate::editor_handoff::RetirementView::from_proto).transpose()?,
            fork_retirement: record
                .slot
                .development_fork_retirement
                .as_ref()
                .map(|v| -> Result<ForkRetirementView> {
                    Ok(ForkRetirementView {
                        schema_version: v.schema_version,
                        child_draft_id: v.child_draft_id.clone(),
                        operation_id: v.operation_id.clone(),
                        fork_link: v
                            .fork_link
                            .as_ref()
                            .ok_or("DraftForkRetirementLinkMissing")?
                            .into(),
                    })
                })
                .transpose()?,
        })
    }
}
