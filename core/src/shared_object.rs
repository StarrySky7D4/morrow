//! Portable runtime metadata only: knowing or decoding a descriptor never grants access.
//! Logical segments carry no native addresses or operating-system resource handles.
use crate::{Error, Result, shared_object_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub const VERSION: u16 = 1;
pub const MAX_OBJECT_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_SEGMENTS: usize = 64;
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024;
pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/shared_object.capnp"))
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid("shared object descriptor")
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub offset: u64,
    pub length: u64,
}
/// Untrusted metadata. Access requires a separate, live host-issued lease bound to a connection.
/// Do not persist this reference as permission or treat the declared digest as verified content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub arena: u64,
    pub object: u64,
    pub generation: u64,
    pub length: u64,
    pub sha256: [u8; 32],
    pub segments: Vec<Segment>,
}
impl Descriptor {
    /// Build metadata for bytes already owned by a trusted caller; this still grants no access.
    pub fn for_bytes(arena: u64, object: u64, generation: u64, bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() as u64 > MAX_OBJECT_BYTES {
            return Err(Error::Limit);
        }
        let length = bytes.len() as u64;
        let mut result = Self {
            arena,
            object,
            generation,
            length,
            sha256: [0; 32],
            segments: vec![Segment { offset: 0, length }],
        };
        result.validate()?;
        result.sha256 = Sha256::digest(bytes).into();
        Ok(result)
    }
    pub fn validate(&self) -> Result<()> {
        if self.arena == 0 || self.object == 0 || self.generation == 0 {
            return Err(Error::Invalid("shared object identity"));
        }
        if self.length == 0
            || self.length > MAX_OBJECT_BYTES
            || self.segments.is_empty()
            || self.segments.len() > MAX_SEGMENTS
        {
            return Err(Error::Limit);
        }
        let mut expected = 0;
        for segment in &self.segments {
            let end = segment
                .offset
                .checked_add(segment.length)
                .ok_or(Error::Limit)?;
            if segment.length == 0 || segment.offset != expected || end > self.length {
                return Err(Error::Invalid("shared object segments"));
            }
            expected = end;
        }
        if expected != self.length {
            return Err(Error::Invalid("shared object coverage"));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::descriptor::Builder>();
        root.set_version(VERSION);
        root.set_schema_digest(&schema_digest());
        root.set_arena(self.arena);
        root.set_object(self.object);
        root.set_generation(self.generation);
        root.set_length(self.length);
        root.set_sha256(&self.sha256);
        let mut segments = root.init_segments(self.segments.len() as u32);
        for (i, segment) in self.segments.iter().enumerate() {
            let mut output = segments.reborrow().get(i as u32);
            output.set_offset(segment.offset);
            output.set_length(segment.length);
        }
        let bytes = serialize::write_message_to_words(&message);
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Limit);
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(Error::Limit);
        }
        let mut rest = bytes;
        // Incoming transport slices may be unaligned. Bounded owned words remove that
        // platform-specific precondition; object payload bytes are never copied here.
        let message = serialize::read_message(
            &mut rest,
            ReaderOptions {
                traversal_limit_in_words: Some(MAX_MESSAGE_BYTES / 8),
                nesting_limit: 8,
            },
        )
        .map_err(invalid)?;
        if !rest.is_empty() {
            return Err(Error::Invalid("trailing shared object bytes"));
        }
        let root = message
            .get_root::<wire::descriptor::Reader>()
            .map_err(invalid)?;
        if root.get_version() != VERSION
            || root.get_schema_digest().map_err(invalid)? != schema_digest()
        {
            return Err(Error::UnsupportedVersion);
        }
        let sha256 = root
            .get_sha256()
            .map_err(invalid)?
            .try_into()
            .map_err(invalid)?;
        let raw_segments = root.get_segments().map_err(invalid)?;
        if raw_segments.is_empty() || raw_segments.len() as usize > MAX_SEGMENTS {
            return Err(Error::Limit);
        }
        let descriptor = Self {
            arena: root.get_arena(),
            object: root.get_object(),
            generation: root.get_generation(),
            length: root.get_length(),
            sha256,
            segments: raw_segments
                .iter()
                .map(|s| Segment {
                    offset: s.get_offset(),
                    length: s.get_length(),
                })
                .collect(),
        };
        descriptor.validate()?;
        Ok(descriptor)
    }
}
