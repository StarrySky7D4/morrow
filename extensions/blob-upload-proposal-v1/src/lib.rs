//! Bounded plugin-produced metadata. Parsing/binding never grants an upload.
//! Package, handler, revision, original owner and approval are host responsibilities.
#![deny(unsafe_op_in_unsafe_fn)]
use capnp::{
    message::{Builder, HeapAllocator, ReaderOptions},
    serialize,
    traits::{HasStructSize, IntoInternalStructReader},
};
use morrow_blob_transfer_v1::{Action, Frame, Identity};
use sha2::{Digest, Sha256};
use std::fmt;

#[allow(unsafe_op_in_unsafe_fn)]
mod blob_upload_proposal_capnp {
    include!(concat!(env!("OUT_DIR"), "/blob_upload_proposal_capnp.rs"));
}
use blob_upload_proposal_capnp as wire;
include!(concat!(env!("OUT_DIR"), "/schema_digest.rs"));
pub mod ffi;

pub const PROFILE: &str = "blob-upload-proposal-v1";
pub const VERSION: u16 = 1;
pub const OUTPUT_TYPE: &str = "morrow.blob.upload.proposal.v1";
pub const BOUND_INPUT_TYPE: &str = "morrow.blob.upload.input.v1";
pub const MAX_LABEL_BYTES: usize = 512;
pub const MAX_LABEL_CHARS: usize = 128;
pub const MAX_PRODUCER_INPUT_BYTES: usize = 32 * 1024;
pub const MAX_PROPOSAL_BYTES: usize = 34 * 1024;
pub const MAX_BOUND_INPUT_BYTES: usize = 36 * 1024;
pub const MAX_OBJECT_BYTES: u64 = morrow_blob_transfer_v1::MAX_OBJECT_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    Invalid = 1,
    Contract = 2,
    Limit = 3,
    Correlation = 4,
    Buffer = 5,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blob-upload-proposal-v1: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
fn invalid<T>(_: T) -> Error {
    Error::Invalid
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn builder() -> Builder<HeapAllocator> {
    // Every admitted message fits one segment. Allocation order is part of this
    // exact profile, so alternative pointer aliases/padding cannot change its hash.
    Builder::new(HeapAllocator::new().first_segment_words((MAX_BOUND_INPUT_BYTES / 8) as u32))
}
fn options() -> ReaderOptions {
    ReaderOptions {
        traversal_limit_in_words: Some(MAX_BOUND_INPUT_BYTES / 4),
        nesting_limit: 8,
    }
}
fn bounded(bytes: &[u8], max: usize) -> Result<()> {
    if bytes.is_empty() || bytes.len() > max {
        Err(Error::Limit)
    } else {
        Ok(())
    }
}
fn layout<'a>(root: impl IntoInternalStructReader<'a>, data: u16, pointers: u16) -> Result<()> {
    let root = root.into_internal_struct_reader();
    if root.get_data_section_size() != u32::from(data) * 64
        || root.get_pointer_section_size() != pointers
    {
        Err(Error::Contract)
    } else {
        Ok(())
    }
}
fn validate(label: &str, length: u64, sha: [u8; 32], input: &[u8]) -> Result<()> {
    if label.len() > MAX_LABEL_BYTES
        || label.chars().count() > MAX_LABEL_CHARS
        || length > MAX_OBJECT_BYTES
        || input.len() > MAX_PRODUCER_INPUT_BYTES
    {
        return Err(Error::Limit);
    }
    if label.is_empty() || label.trim().is_empty() || label.trim() != label
        || label.chars().any(|c| c.is_control() || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}'))
        || (length == 0 && sha != digest(&[])) {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn proposal_wire(label: &str, length: u64, sha: [u8; 32], input: &[u8]) -> Vec<u8> {
    let mut message = builder();
    {
        let mut root = message.init_root::<wire::proposal::Builder>();
        root.set_version(VERSION);
        root.set_schema_sha256(&SCHEMA_DIGEST);
        root.set_label(label);
        root.set_byte_length(length);
        root.set_whole_sha256(&sha);
        root.set_producer_input(input);
    }
    serialize::write_message_to_words(&message)
}

/// Immutable proposal data copied from the producer, with no object/operation ID.
#[derive(Clone, PartialEq, Eq)]
pub struct Proposal {
    wire: Vec<u8>,
    label: String,
    length: u64,
    sha: [u8; 32],
    input: Vec<u8>,
}
impl fmt::Debug for Proposal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadProposal")
            .field("byte_length", &self.length)
            .field("producer_input_bytes", &self.input.len())
            .finish_non_exhaustive()
    }
}
impl Proposal {
    pub fn new(
        label: &str,
        byte_length: u64,
        whole_sha256: [u8; 32],
        producer_input: &[u8],
    ) -> Result<Self> {
        validate(label, byte_length, whole_sha256, producer_input)?;
        let wire = proposal_wire(label, byte_length, whole_sha256, producer_input);
        bounded(&wire, MAX_PROPOSAL_BYTES)?;
        Ok(Self {
            wire,
            label: label.into(),
            length: byte_length,
            sha: whole_sha256,
            input: producer_input.into(),
        })
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        bounded(bytes, MAX_PROPOSAL_BYTES)?;
        let mut rest = bytes;
        let message =
            serialize::read_message_from_flat_slice(&mut rest, options()).map_err(invalid)?;
        if !rest.is_empty() {
            return Err(Error::Invalid);
        }
        let root = message
            .get_root::<wire::proposal::Reader>()
            .map_err(invalid)?;
        let size = <wire::proposal::Builder as HasStructSize>::STRUCT_SIZE;
        layout(root, size.data, size.pointers)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != SCHEMA_DIGEST
        {
            return Err(Error::Contract);
        }
        if root.get_reserved() != 0 || root.get_reserved2() != 0 {
            return Err(Error::Invalid);
        }
        let label = root
            .get_label()
            .map_err(invalid)?
            .to_str()
            .map_err(invalid)?;
        let sha = root
            .get_whole_sha256()
            .map_err(invalid)?
            .try_into()
            .map_err(invalid)?;
        let value = Self::new(
            label,
            root.get_byte_length(),
            sha,
            root.get_producer_input().map_err(invalid)?,
        )?;
        if value.wire != bytes {
            return Err(Error::Invalid);
        }
        Ok(value)
    }
    pub fn wire(&self) -> &[u8] {
        &self.wire
    }
    pub fn digest(&self) -> [u8; 32] {
        digest(&self.wire)
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn byte_length(&self) -> u64 {
        self.length
    }
    pub fn whole_sha256(&self) -> [u8; 32] {
        self.sha
    }
    pub fn producer_input(&self) -> &[u8] {
        &self.input
    }
}

/// Correlates the exact proposal with host-selected frame names. This codec can
/// be called by anyone; only the original live host can authorize those names.
pub struct BoundInput {
    wire: Vec<u8>,
    proposal: Proposal,
    identity: Identity,
}
impl fmt::Debug for BoundInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoundUploadInput")
            .field("proposal", &self.proposal)
            .finish_non_exhaustive()
    }
}
impl BoundInput {
    pub fn new(proposal: &Proposal, identity: Identity) -> Result<Self> {
        let descriptor = Frame {
            identity,
            action: Action::Descriptor {
                total_length: proposal.length,
                whole_sha256: proposal.sha,
            },
        }
        .encode()
        .map_err(invalid)?;
        let mut message = builder();
        {
            let mut root = message.init_root::<wire::bound_input::Builder>();
            root.set_version(VERSION);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_frame_schema_sha256(&morrow_blob_transfer_v1::SCHEMA_DIGEST);
            root.set_proposal_digest(&proposal.digest());
            root.set_proposal(proposal.wire());
            root.set_descriptor(&descriptor);
        }
        let wire = serialize::write_message_to_words(&message);
        bounded(&wire, MAX_BOUND_INPUT_BYTES)?;
        Ok(Self {
            wire,
            proposal: proposal.clone(),
            identity,
        })
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        bounded(bytes, MAX_BOUND_INPUT_BYTES)?;
        let mut rest = bytes;
        let message =
            serialize::read_message_from_flat_slice(&mut rest, options()).map_err(invalid)?;
        if !rest.is_empty() {
            return Err(Error::Invalid);
        }
        let root = message
            .get_root::<wire::bound_input::Reader>()
            .map_err(invalid)?;
        let size = <wire::bound_input::Builder as HasStructSize>::STRUCT_SIZE;
        layout(root, size.data, size.pointers)?;
        if root.total_size().map_err(invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(invalid)? != SCHEMA_DIGEST
            || root.get_frame_schema_sha256().map_err(invalid)?
                != morrow_blob_transfer_v1::SCHEMA_DIGEST
        {
            return Err(Error::Contract);
        }
        if root.get_reserved() != 0 || root.get_reserved2() != 0 {
            return Err(Error::Invalid);
        }
        let proposal = Proposal::decode(root.get_proposal().map_err(invalid)?)?;
        if root.get_proposal_digest().map_err(invalid)? != proposal.digest() {
            return Err(Error::Correlation);
        }
        let descriptor_bytes = root.get_descriptor().map_err(invalid)?;
        if descriptor_bytes.len() > 1024 {
            return Err(Error::Limit);
        }
        let frame = Frame::decode(descriptor_bytes).map_err(invalid)?;
        match frame.action {
            Action::Descriptor {
                total_length,
                whole_sha256,
            } if total_length == proposal.length && whole_sha256 == proposal.sha => {}
            _ => return Err(Error::Correlation),
        }
        let value = Self::new(&proposal, frame.identity)?;
        if value.wire != bytes {
            return Err(Error::Invalid);
        }
        Ok(value)
    }
    pub fn wire(&self) -> &[u8] {
        &self.wire
    }
    pub fn proposal(&self) -> &Proposal {
        &self.proposal
    }
    pub fn identity(&self) -> Identity {
        self.identity
    }
}

#[cfg(test)]
mod tests;
