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
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Write {
    card_id: String,
    draft_id: String,
    source_kind: u32,
    source_revision: String,
    expected_generation: String,
    operation_id: String,
    values: Option<Values>,
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
        let request = proto::WriteRequest {
            schema_version: 1,
            card_id: self.card_id,
            draft_id: self.draft_id,
            source_kind: self.source_kind,
            source_revision: number(&self.source_revision)?,
            expected_generation: number(&self.expected_generation)?,
            operation_id: self.operation_id,
            values: self.values.map(Into::into),
            ..Default::default()
        };
        morrow_editor_draft_model::validate_request(&request).map_err(err)?;
        Ok(request)
    }
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
    values: Values,
    operation_id: String,
    generation: String,
    current_generation: String,
    active: bool,
    current_active: bool,
    repeated: bool,
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
            values: values.into(),
            operation_id: request.operation_id.clone(),
            generation: record.slot.generation.to_string(),
            current_generation: record.current_generation.to_string(),
            active: record.slot.active,
            current_active: record.current_active,
            repeated: record.repeated,
        })
    }
}
