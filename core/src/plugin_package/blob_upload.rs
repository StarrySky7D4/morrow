//! Independent package ceiling. Declaration and hashes never grant a live operation.
use super::{TRANSFORM_HANDLERS_FEATURE, proto};
use crate::{Error, Result};
use prost::{
    Message,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const FEATURE: &str = "blob-upload-request-v1";
pub const MAX_OBJECT_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_CALLS: u32 = 4096;
pub const MAX_WIRE_BYTES: u64 = 64 * 1024 * 1024;
/// Cumulative interpreter fuel for one independent upload task, never per chunk.
/// Explicit package and host budgets must both admit it. Legacy profiles retain
/// their 100 million ceiling; default budgets are unchanged.
pub const MAX_FUEL: u64 = 8_000_000_000;

pub fn request_schema_digest() -> [u8; 32] {
    Sha256::digest(include_bytes!(
        "../../../extensions/blob-upload-request-v1/contracts/blob_upload_request.capnp"
    ))
    .into()
}
pub fn frame_schema_digest() -> [u8; 32] {
    Sha256::digest(include_bytes!(
        "../../../extensions/blob-transfer-v1/contracts/blob_transfer.capnp"
    ))
    .into()
}
pub fn declaration(handlers: Vec<String>) -> proto::BlobUploadDeclaration {
    proto::BlobUploadDeclaration {
        schema_version: 1,
        request_schema_sha256: request_schema_digest().to_vec(),
        frame_schema_sha256: frame_schema_digest().to_vec(),
        handlers,
        max_object_bytes: MAX_OBJECT_BYTES,
        max_calls: 1024,
        max_wire_bytes: MAX_WIRE_BYTES,
    }
}
// Preserve unrelated optional manifest fields, but never accept duplicate, unknown
// or noncanonical semantics inside this mandatory declaration.
fn exact_wire(mut wire: &[u8], value: Option<&proto::BlobUploadDeclaration>) -> Result<()> {
    let mut seen = false;
    while !wire.is_empty() {
        let (number, kind) = decode_key(&mut wire).map_err(|_| Error::Invalid("manifest key"))?;
        if number != 22 {
            skip_field(kind, number, &mut wire, DecodeContext::default())
                .map_err(|_| Error::Invalid("manifest field"))?;
            continue;
        }
        if seen || kind != WireType::LengthDelimited {
            return Err(Error::Invalid("duplicate or malformed upload declaration"));
        }
        seen = true;
        let length =
            decode_varint(&mut wire).map_err(|_| Error::Invalid("upload declaration length"))?;
        if length > wire.len() as u64 {
            return Err(Error::Invalid("upload declaration length"));
        }
        let (bytes, rest) = wire.split_at(length as usize);
        wire = rest;
        if value
            .ok_or(Error::Invalid("missing upload declaration"))?
            .encode_to_vec()
            != bytes
        {
            return Err(Error::Invalid("noncanonical upload declaration"));
        }
    }
    if seen != value.is_some() {
        return Err(Error::Invalid("upload declaration presence"));
    }
    Ok(())
}
pub(super) fn validate_manifest(wire: &[u8], manifest: &proto::Manifest) -> Result<()> {
    exact_wire(wire, manifest.blob_upload_declaration.as_ref())?;
    let feature = manifest.required_features.iter().any(|f| f == FEATURE);
    if feature != manifest.blob_upload_declaration.is_some() {
        return Err(Error::Invalid("upload declaration feature"));
    }
    let Some(value) = &manifest.blob_upload_declaration else {
        return Ok(());
    };
    if manifest.guest_abi_version != 2
        || manifest.required_features.len()
            != 2 + usize::from(manifest.blob_upload_proposal_declaration.is_some())
        || !manifest
            .required_features
            .iter()
            .any(|f| f == TRANSFORM_HANDLERS_FEATURE)
        || manifest.required_features.iter().any(|f| {
            !matches!(
                f.as_str(),
                FEATURE | TRANSFORM_HANDLERS_FEATURE | super::blob_upload_proposal::FEATURE
            )
        })
        || !manifest.requested_capabilities.is_empty()
        || manifest.io_declaration.is_some()
        || manifest.channel_declaration.is_some()
        || !manifest.dependencies.is_empty()
        || !manifest.dependency_schema_sha256.is_empty()
        || !manifest.mutation_schema_sha256.is_empty()
        || manifest.mutation_budget.is_some()
    {
        return Err(Error::Invalid("upload profile isolation"));
    }
    if value.schema_version != 1
        || value.request_schema_sha256 != request_schema_digest()
        || value.frame_schema_sha256 != frame_schema_digest()
    {
        return Err(Error::UnsupportedVersion);
    }
    if value.max_object_bytes == 0
        || value.max_object_bytes > MAX_OBJECT_BYTES
        || value.max_calls == 0
        || value.max_calls > MAX_CALLS
        || value.max_wire_bytes == 0
        || value.max_wire_bytes > MAX_WIRE_BYTES
        || value.handlers.is_empty()
        || value.handlers.len() > super::MAX_TRANSFORM_HANDLERS
    {
        return Err(Error::Limit);
    }
    let mut names = BTreeSet::new();
    for name in &value.handlers {
        crate::identity(name)?;
        if !names.insert(name)
            || !manifest
                .transform_handlers
                .iter()
                .any(|h| &h.handler == name)
        {
            return Err(Error::Invalid("upload handler registration"));
        }
    }
    if let Some(proposals) = &manifest.blob_upload_proposal_declaration {
        for proposal in &proposals.handlers {
            if !names.insert(&proposal.handler) {
                return Err(Error::Invalid("overlapping upload and proposal handlers"));
            }
        }
    }
    if names.len() != manifest.transform_handlers.len() {
        return Err(Error::Invalid("mixed upload and transform handlers"));
    }
    Ok(())
}
