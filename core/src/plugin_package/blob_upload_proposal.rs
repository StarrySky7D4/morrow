//! Same-package pure planning metadata. No selected object, original owner or approval.
use super::{MAX_TRANSFORM_HANDLERS, proto};
use crate::{Error, Result};
use prost::{
    Message,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const FEATURE: &str = "blob-upload-proposal-v1";
pub const OUTPUT_TYPE: &str = "morrow.blob.upload.proposal.v1";
pub const BOUND_INPUT_TYPE: &str = "morrow.blob.upload.input.v1";
pub const MAX_PROPOSAL_BYTES: u32 = 34 * 1024;
pub const MAX_BOUND_INPUT_BYTES: u32 = 36 * 1024;
pub const MAX_PLAN_INPUT_BYTES: u32 = 32 * 1024;
pub fn schema_digest() -> [u8; 32] {
    Sha256::digest(include_bytes!(
        "../../../extensions/blob-upload-proposal-v1/contracts/blob_upload_proposal.capnp"
    ))
    .into()
}
pub fn declaration(
    handlers: Vec<proto::BlobUploadProposalHandler>,
) -> proto::BlobUploadProposalDeclaration {
    proto::BlobUploadProposalDeclaration {
        schema_version: 1,
        schema_sha256: schema_digest().to_vec(),
        handlers,
    }
}
pub(super) fn validate_manifest(mut wire: &[u8], manifest: &proto::Manifest) -> Result<()> {
    let value = manifest.blob_upload_proposal_declaration.as_ref();
    let mut seen = false;
    while !wire.is_empty() {
        let (number, kind) = decode_key(&mut wire).map_err(|_| Error::Invalid("manifest key"))?;
        if number != 23 {
            skip_field(kind, number, &mut wire, DecodeContext::default())
                .map_err(|_| Error::Invalid("manifest field"))?;
            continue;
        }
        if seen || kind != WireType::LengthDelimited {
            return Err(Error::Invalid("duplicate upload proposal declaration"));
        }
        seen = true;
        let length = decode_varint(&mut wire)
            .map_err(|_| Error::Invalid("upload proposal declaration length"))?;
        if length > wire.len() as u64 {
            return Err(Error::Invalid("upload proposal declaration length"));
        }
        let (bytes, rest) = wire.split_at(length as usize);
        wire = rest;
        if value
            .ok_or(Error::Invalid("missing upload proposal declaration"))?
            .encode_to_vec()
            != bytes
        {
            return Err(Error::Invalid("noncanonical upload proposal declaration"));
        }
    }
    let feature = manifest.required_features.iter().any(|f| f == FEATURE);
    if seen != value.is_some() || feature != value.is_some() {
        return Err(Error::Invalid("upload proposal declaration feature"));
    }
    let Some(value) = value else {
        return Ok(());
    };
    let upload = manifest
        .blob_upload_declaration
        .as_ref()
        .ok_or(Error::Invalid("proposal without upload profile"))?;
    if value.schema_version != 1 || value.schema_sha256 != schema_digest() {
        return Err(Error::UnsupportedVersion);
    }
    if value.handlers.is_empty() || value.handlers.len() > MAX_TRANSFORM_HANDLERS {
        return Err(Error::Limit);
    }
    let mut names = BTreeSet::new();
    for mapping in &value.handlers {
        crate::identity(&mapping.handler)?;
        crate::identity(&mapping.upload_handler)?;
        if !names.insert(&mapping.handler)
            || upload.handlers.contains(&mapping.handler)
            || !upload.handlers.contains(&mapping.upload_handler)
        {
            return Err(Error::Invalid("invalid upload proposal mapping"));
        }
        let plan = manifest
            .transform_handlers
            .iter()
            .find(|h| h.handler == mapping.handler)
            .ok_or(Error::Invalid("missing proposal metadata"))?;
        let target = manifest
            .transform_handlers
            .iter()
            .find(|h| h.handler == mapping.upload_handler)
            .ok_or(Error::Invalid("missing proposal target metadata"))?;
        if plan.output_type != OUTPUT_TYPE || target.input_type != BOUND_INPUT_TYPE {
            return Err(Error::Invalid("upload proposal contract type"));
        }
        if plan.max_input_bytes > MAX_PLAN_INPUT_BYTES
            || plan.max_output_bytes == 0
            || plan.max_output_bytes > MAX_PROPOSAL_BYTES
            || target.max_input_bytes == 0
            || target.max_input_bytes > MAX_BOUND_INPUT_BYTES
        {
            return Err(Error::Limit);
        }
    }
    Ok(())
}
