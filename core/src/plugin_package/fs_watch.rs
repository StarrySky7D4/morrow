//! Versioned observation ceilings, independent of ordinary FileList approval.
#![deny(warnings)]
use super::{io, proto};
use crate::{Error, Result};
use prost::{
    Message,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const FEATURE: &str = "fs-watch-v1";
pub const MAX_FRAMES: u32 = 64;
pub const MAX_WIRE_BYTES: u64 = 64 * 65536;
pub const MAX_WINDOW_MS: u64 = io::MAX_DURATION_MS;

pub fn frame_schema_digest() -> [u8; 32] {
    Sha256::digest(include_bytes!(
        "../../../extensions/fs-watch-v1/contracts/fs_watch.capnp"
    ))
    .into()
}
pub fn declaration(handlers: Vec<String>) -> proto::FsWatchDeclaration {
    proto::FsWatchDeclaration {
        schema_version: 1,
        frame_schema_sha256: frame_schema_digest().to_vec(),
        handlers,
        max_frames: MAX_FRAMES,
        max_wire_bytes: MAX_WIRE_BYTES,
        max_window_ms: MAX_WINDOW_MS,
    }
}
fn exact_wire(mut wire: &[u8], value: Option<&proto::FsWatchDeclaration>) -> Result<()> {
    let mut seen = false;
    while !wire.is_empty() {
        let (number, kind) = decode_key(&mut wire).map_err(|_| Error::Invalid("manifest key"))?;
        if number != 24 {
            skip_field(kind, number, &mut wire, DecodeContext::default())
                .map_err(|_| Error::Invalid("manifest field"))?;
            continue;
        }
        if seen || kind != WireType::LengthDelimited {
            return Err(Error::Invalid("duplicate or malformed watch declaration"));
        }
        seen = true;
        let length =
            decode_varint(&mut wire).map_err(|_| Error::Invalid("watch declaration length"))?;
        if length > wire.len() as u64 {
            return Err(Error::Invalid("watch declaration length"));
        }
        let (bytes, rest) = wire.split_at(length as usize);
        wire = rest;
        if value
            .ok_or(Error::Invalid("missing watch declaration"))?
            .encode_to_vec()
            != bytes
        {
            return Err(Error::Invalid("noncanonical watch declaration"));
        }
    }
    if seen != value.is_some() {
        return Err(Error::Invalid("watch declaration presence"));
    }
    Ok(())
}
pub(super) fn validate_manifest(wire: &[u8], manifest: &proto::Manifest) -> Result<()> {
    exact_wire(wire, manifest.fs_watch_declaration.as_ref())?;
    let feature = manifest.required_features.iter().any(|f| f == FEATURE);
    if feature != manifest.fs_watch_declaration.is_some() {
        return Err(Error::Invalid("watch declaration feature"));
    }
    let Some(value) = &manifest.fs_watch_declaration else {
        return Ok(());
    };
    let io = manifest
        .io_declaration
        .as_ref()
        .ok_or(Error::Invalid("watch IO budget"))?;
    if manifest.guest_abi_version != 2
        || manifest.required_features.len() != 2
        || !manifest.required_features.iter().any(|f| f == io::FEATURE)
        || manifest
            .required_features
            .iter()
            .any(|f| !matches!(f.as_str(), FEATURE | io::FEATURE))
        || !manifest.requested_capabilities.is_empty()
        || !manifest.transform_handlers.is_empty()
        || !manifest.dependencies.is_empty()
        || !manifest.dependency_schema_sha256.is_empty()
        || manifest.channel_declaration.is_some()
        || manifest.blob_upload_declaration.is_some()
        || manifest.blob_upload_proposal_declaration.is_some()
        || !manifest.mutation_schema_sha256.is_empty()
        || manifest.mutation_budget.is_some()
        || io.requested_capabilities != [io::IoCapability::FileList.number()]
        || io.service_run.is_some()
        || !io.service_schema_sha256.is_empty()
    {
        return Err(Error::Invalid("watch profile isolation"));
    }
    if value.schema_version != 1 || value.frame_schema_sha256 != frame_schema_digest() {
        return Err(Error::UnsupportedVersion);
    }
    let budget = io
        .budget
        .as_ref()
        .ok_or(Error::Invalid("watch IO budget"))?;
    if value.max_frames == 0
        || value.max_frames > MAX_FRAMES
        || value.max_wire_bytes == 0
        || value.max_wire_bytes > MAX_WIRE_BYTES
        || value.max_wire_bytes > budget.max_bytes
        || value.max_wire_bytes > budget.max_job_bytes
        || value.max_window_ms == 0
        || value.max_window_ms > MAX_WINDOW_MS
        || value.max_window_ms > budget.max_duration_ms
        || value.handlers.is_empty()
        || value.handlers.len() > io::MAX_HANDLERS
    {
        return Err(Error::Limit);
    }
    let mut names = BTreeSet::new();
    for name in &value.handlers {
        crate::identity(name)?;
        if !names.insert(name) {
            return Err(Error::Invalid("duplicate watch handler"));
        }
    }
    if names != io.handlers.iter().collect() {
        return Err(Error::Invalid("mixed watch and ordinary IO handlers"));
    }
    Ok(())
}
