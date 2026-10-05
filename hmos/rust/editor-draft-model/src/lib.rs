//! Shape and size validation for a host-owned successor editor draft.
//! The journal storage layer checks canonical protobuf bytes, source/attachment
//! authority and pins. This model grants no runtime access or capture tickets.

use crate::Result;
use prost::Message;
use std::collections::HashSet;

pub mod proto {
    include!(concat!(
        env!("OUT_DIR"),
        "/morrow.workbench.editor_draft.v1.rs"
    ));
}

pub const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TEXT_BYTES: usize = 512 * 1024;
const MAX_CATEGORY_BYTES: usize = 16 * 1024;
const MAX_ALIAS_BYTES: usize = 16 * 1024;
const MAX_ASSETS: usize = 20;
const MAX_ALIASES: usize = 8;

fn identity(value: &str) -> Result<()> {
    morrow_core::runtime::Command::ReadSummary {
        request_id: value.into(),
        card_id: value.into(),
    }
    .validate()?;
    Ok(())
}

// Flutter stores offsets as UTF-16 code units, including transient positions
// inside a surrogate pair and mixed -1 sentinel states.
fn utf16_offset(units: usize, offset: i32) -> bool {
    offset == -1 || usize::try_from(offset).is_ok_and(|offset| offset <= units)
}

fn validate_text(value: &proto::TextValue) -> Result<()> {
    if value.text.len() > MAX_TEXT_BYTES || value.affinity > 1 {
        return Err("editor draft text or affinity limit".into());
    }
    let units = value.text.encode_utf16().count();
    if !utf16_offset(units, value.selection_base) || !utf16_offset(units, value.selection_extent) {
        return Err("editor draft selection offset is out of bounds".into());
    }
    if !utf16_offset(units, value.composing_start)
        || !utf16_offset(units, value.composing_end)
        || (value.composing_start >= 0
            && value.composing_end >= 0
            && value.composing_start > value.composing_end)
    {
        return Err("editor draft composing offset is out of bounds".into());
    }
    Ok(())
}

/// Validate only the caller's raw draft shape. Empty and intermediate text is
/// deliberate; saving or rebasing it is a separate, explicitly authorized step.
pub fn validate_request(request: &proto::WriteRequest) -> Result<()> {
    if request.schema_version != 1
        || request.encoded_len() > MAX_BODY_BYTES
        || !matches!(
            (request.source_kind, request.source_revision),
            (0, 1..) | (1, 0)
        )
        || request.expected_generation == u64::MAX
    {
        return Err("invalid editor draft request version, size or revision".into());
    }
    identity(&request.card_id)?;
    identity(&request.draft_id)?;
    identity(&request.operation_id)?;

    let has_predecessor = match (
        request.predecessor_operation.is_empty(),
        request.predecessor_sha256.len(),
    ) {
        (true, 0) => false,
        (false, 32) => {
            identity(&request.predecessor_operation)?;
            true
        }
        _ => return Err("invalid predecessor operation and digest pair".into()),
    };
    if request.source_kind == 1 && has_predecessor {
        return Err("new-card draft cannot claim a predecessor".into());
    }
    let values = request
        .values
        .as_ref()
        .ok_or("missing editor draft values")?;
    for value in [
        values.title.as_ref(),
        values.description.as_ref(),
        values.hypothesis.as_ref(),
        values.conclusion.as_ref(),
        values.todos.as_ref(),
    ] {
        validate_text(value.ok_or("missing editor draft text value")?)?;
    }
    if values.category.len() > MAX_CATEGORY_BYTES || values.stage.len() > MAX_CATEGORY_BYTES {
        return Err("editor draft category or stage limit".into());
    }

    if request.assets.len() > MAX_ASSETS {
        return Err("too many selected editor draft assets".into());
    }
    let mut ids = HashSet::with_capacity(request.assets.len());
    for asset in &request.assets {
        identity(&asset.asset_id)?;
        if !ids.insert(asset.asset_id.as_str()) {
            return Err("duplicate editor draft asset ID".into());
        }
        match asset.origin {
            0 if request.source_kind == 0 => {}
            1 if request.source_kind == 0 && has_predecessor => {}
            2 => {}
            3 if request.expected_generation > 0 => {}
            4 if request.source_kind == 0 => {}
            _ => return Err("invalid editor draft asset origin".into()),
        }
        if asset.aliases.len() > MAX_ALIASES
            || asset
                .aliases
                .iter()
                .any(|alias| alias.is_empty() || alias.len() > MAX_ALIAS_BYTES)
        {
            return Err("editor draft asset alias limit".into());
        }
    }
    Ok(())
}

/// Validate the immutable parent evidence carried by a child draft slot.
/// This checks shape only; the host resolves the parent receipt and pins.
pub fn validate_parent_link(request: &proto::WriteRequest, link: &proto::ParentLink) -> Result<()> {
    validate_request(request)?;
    if request.source_kind != 0
        || !request.predecessor_operation.is_empty()
        || !request.predecessor_sha256.is_empty()
    {
        return Err("parent-linked draft must use an existing source without predecessor".into());
    }
    if link.parent_draft_id.is_empty()
        || link.parent_draft_id == request.draft_id
        || link.parent_save_operation.is_empty()
        || link.committed_operation.is_empty()
        || link.child_operation.is_empty()
    {
        return Err("invalid editor draft parent link identity".into());
    }
    for value in [
        &link.parent_draft_id,
        &link.parent_save_operation,
        &link.committed_operation,
        &link.child_operation,
    ] {
        identity(value)?;
    }
    if link.parent_generation == 0 || link.parent_generation == u64::MAX {
        return Err("invalid editor draft parent generation".into());
    }
    if link.parent_request_sha256.len() != 32 || link.committed_sha256.len() != 32 {
        return Err("invalid editor draft parent digest".into());
    }
    if request.expected_generation == 0 && link.child_operation != request.operation_id {
        return Err("first child draft operation does not match parent link".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> proto::TextValue {
        proto::TextValue {
            text: value.into(),
            selection_base: 0,
            selection_extent: 0,
            affinity: 0,
            directional: false,
            composing_start: -1,
            composing_end: -1,
        }
    }

    fn valid() -> proto::WriteRequest {
        proto::WriteRequest {
            schema_version: 1,
            card_id: "card".into(),
            draft_id: "draft".into(),
            operation_id: "draft-operation".into(),
            expected_generation: 0,
            source_revision: 1,
            source_kind: 0,
            predecessor_operation: String::new(),
            predecessor_sha256: Vec::new(),
            values: Some(proto::Values {
                title: Some(text("")),
                description: Some(text("")),
                hypothesis: Some(text("")),
                conclusion: Some(text("")),
                todos: Some(text("")),
                category: String::new(),
                stage: String::new(),
            }),
            assets: Vec::new(),
        }
    }

    #[test]
    fn empty_and_intermediate_values_are_retained() {
        let mut request = valid();
        request
            .values
            .as_mut()
            .unwrap()
            .description
            .as_mut()
            .unwrap()
            .text = "\0".into();
        assert!(validate_request(&request).is_ok());
        assert_eq!(request.values.unwrap().title.unwrap().text, "");
    }

    #[test]
    fn emoji_code_unit_offsets_roundtrip_even_inside_surrogate_pair() {
        let mut request = valid();
        let value = request.values.as_mut().unwrap().title.as_mut().unwrap();
        value.text = "A😀B".into(); // UTF-16 length is four code units.
        value.selection_base = 2; // Inside the emoji surrogate pair.
        value.selection_extent = 1; // Reversed selections are preserved.
        value.composing_start = 1;
        value.composing_end = 2;
        assert!(validate_request(&request).is_ok());
        let encoded = request.encode_to_vec();
        let decoded = proto::WriteRequest::decode(encoded.as_slice()).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn mixed_sentinels_roundtrip_and_out_of_range_offsets_reject() {
        let mut request = valid();
        let value = request.values.as_mut().unwrap().title.as_mut().unwrap();
        value.text = "🙂".into(); // UTF-16 length is two code units.
        value.selection_base = -1;
        value.selection_extent = 1; // Mixed sentinel and mid-surrogate.
        value.composing_start = -1;
        value.composing_end = 1;
        assert!(validate_request(&request).is_ok());
        let encoded = request.encode_to_vec();
        assert_eq!(
            proto::WriteRequest::decode(encoded.as_slice()).unwrap(),
            request
        );

        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .selection_base = -2;
        assert!(validate_request(&request).is_err());
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .selection_base = 0;
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .selection_extent = 3;
        assert!(validate_request(&request).is_err());
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .selection_extent = 2;
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .composing_start = -2;
        assert!(validate_request(&request).is_err());
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .composing_start = 0;
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .composing_end = 3;
        assert!(validate_request(&request).is_err());
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .composing_end = -1;
        assert!(validate_request(&request).is_ok());
    }

    #[test]
    fn rejects_reversed_composition_and_unknown_affinity() {
        let mut request = valid();
        let value = request.values.as_mut().unwrap().title.as_mut().unwrap();
        value.text = "abc".into();
        value.composing_start = 2;
        value.composing_end = 1;
        assert!(validate_request(&request).is_err());
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .composing_start = -1;
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .composing_end = -1;
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .affinity = 2;
        assert!(validate_request(&request).is_err());
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .affinity = 1;
        assert!(validate_request(&request).is_ok());
    }

    #[test]
    fn enforces_text_alias_and_encoded_body_budgets() {
        let mut request = valid();
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .text = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(validate_request(&request).is_err());
        request
            .values
            .as_mut()
            .unwrap()
            .title
            .as_mut()
            .unwrap()
            .text
            .clear();
        request.assets.push(proto::AssetSelection {
            origin: 0,
            asset_id: "asset".into(),
            aliases: vec!["x".repeat(MAX_ALIAS_BYTES + 1)],
        });
        assert!(validate_request(&request).is_err());

        request.assets.clear();
        for index in 0..MAX_ASSETS {
            request.assets.push(proto::AssetSelection {
                origin: 0,
                asset_id: format!("asset-{index}"),
                aliases: vec!["x".repeat(14 * 1024); MAX_ALIASES],
            });
        }
        let values = request.values.as_mut().unwrap();
        values.title.as_mut().unwrap().text = "x".repeat(MAX_TEXT_BYTES);
        values.description.as_mut().unwrap().text = "x".repeat(MAX_TEXT_BYTES);
        values.hypothesis.as_mut().unwrap().text = "x".repeat(MAX_TEXT_BYTES);
        values.conclusion.as_mut().unwrap().text = "x".repeat(MAX_TEXT_BYTES);
        values.todos.as_mut().unwrap().text = "x".repeat(MAX_TEXT_BYTES);
        assert!(request.encoded_len() > MAX_BODY_BYTES);
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn new_card_has_no_source_or_predecessor_and_only_draft_assets() {
        let mut request = valid();
        request.source_kind = 1;
        assert!(validate_request(&request).is_err());
        request.source_revision = 0;
        assert!(validate_request(&request).is_ok());
        assert_eq!(
            proto::WriteRequest::decode(request.encode_to_vec().as_slice()).unwrap(),
            request
        );

        request.predecessor_operation = "S1".into();
        request.predecessor_sha256 = vec![3; 32];
        assert!(validate_request(&request).is_err());
        request.predecessor_operation.clear();
        request.predecessor_sha256.clear();

        request.assets.push(proto::AssetSelection {
            origin: 0,
            asset_id: "asset".into(),
            aliases: Vec::new(),
        });
        assert!(validate_request(&request).is_err());
        request.assets[0].origin = 1;
        assert!(validate_request(&request).is_err());
        request.assets[0].origin = 2;
        assert!(validate_request(&request).is_ok());
        request.assets[0].origin = 3;
        assert!(validate_request(&request).is_err());
        request.assets[0].origin = 4;
        assert!(validate_request(&request).is_err());
        request.assets[0].origin = 3;
        request.expected_generation = 1;
        assert!(validate_request(&request).is_ok());

        request.assets.clear();
        request.source_kind = 2;
        assert!(validate_request(&request).is_err());
    }

    fn valid_parent_link() -> proto::ParentLink {
        proto::ParentLink {
            parent_draft_id: "parent-draft".into(),
            parent_generation: 1,
            parent_save_operation: "parent-save".into(),
            parent_request_sha256: vec![1; 32],
            committed_operation: "parent-commit".into(),
            committed_sha256: vec![2; 32],
            child_operation: "draft-operation".into(),
        }
    }

    #[test]
    fn parent_link_accepts_first_and_later_child_generations() {
        let mut request = valid();
        request.assets.push(proto::AssetSelection {
            origin: 4,
            asset_id: "parent-asset".into(),
            aliases: Vec::new(),
        });
        let mut link = valid_parent_link();
        assert!(validate_parent_link(&request, &link).is_ok());

        request.expected_generation = 1;
        request.operation_id = "draft-operation-2".into();
        assert!(validate_parent_link(&request, &link).is_ok());
        link.child_operation = request.operation_id.clone();
        assert!(validate_parent_link(&request, &link).is_ok());
    }

    #[test]
    fn parent_link_rejects_bad_identity_generation_and_digests() {
        let request = valid();
        let valid = valid_parent_link();
        let mut link = valid.clone();

        link.parent_draft_id = request.draft_id.clone();
        assert!(validate_parent_link(&request, &link).is_err());
        link = valid.clone();
        link.parent_draft_id = "bad/parent".into();
        assert!(validate_parent_link(&request, &link).is_err());

        for generation in [0, u64::MAX] {
            link = valid.clone();
            link.parent_generation = generation;
            assert!(validate_parent_link(&request, &link).is_err());
        }
        link = valid.clone();
        link.parent_request_sha256.pop();
        assert!(validate_parent_link(&request, &link).is_err());
        link = valid.clone();
        link.committed_sha256.push(0);
        assert!(validate_parent_link(&request, &link).is_err());

        for field in 0..3 {
            link = valid.clone();
            match field {
                0 => link.parent_save_operation.clear(),
                1 => link.committed_operation = "bad/op".into(),
                _ => link.child_operation.clear(),
            }
            assert!(validate_parent_link(&request, &link).is_err());
        }
    }

    #[test]
    fn parent_link_requires_existing_source_without_predecessor_and_first_operation() {
        let mut request = valid();
        let link = valid_parent_link();

        request.source_kind = 1;
        request.source_revision = 0;
        assert!(validate_parent_link(&request, &link).is_err());
        request = valid();
        request.predecessor_operation = "S1".into();
        request.predecessor_sha256 = vec![3; 32];
        assert!(validate_parent_link(&request, &link).is_err());
        request = valid();
        request.operation_id = "other-child-operation".into();
        assert!(validate_parent_link(&request, &link).is_err());
    }

    #[test]
    fn validates_origins_predecessor_and_all_identities() {
        let mut request = valid();
        request.assets.push(proto::AssetSelection {
            origin: 1,
            asset_id: "asset".into(),
            aliases: Vec::new(),
        });
        assert!(validate_request(&request).is_err());
        request.predecessor_operation = "S1".into();
        request.predecessor_sha256 = vec![3; 32];
        assert!(validate_request(&request).is_ok());
        let duplicate = request.assets[0].clone();
        request.assets.push(duplicate);
        assert!(validate_request(&request).is_err());
        request.assets.pop();
        request.assets[0].origin = 3;
        assert!(validate_request(&request).is_err());
        request.expected_generation = 1;
        assert!(validate_request(&request).is_ok());
        request.assets[0].origin = 4;
        assert!(validate_request(&request).is_ok());

        request.assets.clear();
        request.card_id = "bad/id".into();
        assert!(validate_request(&request).is_err());
        request.card_id = "card".into();
        request.draft_id = "bad:draft".into();
        assert!(validate_request(&request).is_err());
        request.draft_id = "draft".into();
        request.operation_id = "bad\\operation".into();
        assert!(validate_request(&request).is_err());
        request.operation_id = "draft-operation".into();
        request.predecessor_operation = "bad/op".into();
        assert!(validate_request(&request).is_err());
    }
}
