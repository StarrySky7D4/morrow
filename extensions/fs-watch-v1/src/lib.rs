//! Bounded change observations for a separately approved finite watch window.
//! This codec grants no path, file, watch, rename, replay or native authority.
//! The original source/transport owner must prove cancellation and actual close.
#![deny(unsafe_op_in_unsafe_fn)]

use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
    traits::{HasStructSize, IntoInternalStructReader},
};
use sha2::{Digest, Sha256};
use std::fmt;

#[allow(unsafe_op_in_unsafe_fn)]
mod fs_watch_capnp {
    include!(concat!(env!("OUT_DIR"), "/fs_watch_capnp.rs"));
}
use fs_watch_capnp as wire;
pub mod state;
#[cfg(test)]
mod tests;

/// Payload identity only. Registering a runtime feature/route is separate work.
pub const PROFILE: &str = "fs-watch-v1";
pub const VERSION: u16 = 1;
pub const MAX_WIRE_BYTES: usize = 64 * 1024;
pub const MAX_SEGMENTS: usize = 32;
pub const MAX_CHANGES: usize = 128;
pub const MAX_NAME_BYTES: usize = 1024;
pub const MAX_TOTAL_NAME_BYTES: usize = 48 * 1024;
pub const SCHEMA: &[u8] = include_bytes!("../contracts/fs_watch.capnp");
include!(concat!(env!("OUT_DIR"), "/schema_digest.rs"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Contract,
    Limit,
    Allocation,
    Correlation,
    Pending,
    State,
    Budget,
    OutcomeUnknown,
}
pub type Result<T> = std::result::Result<T, Error>;
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "directory observation {self:?}")
    }
}
impl std::error::Error for Error {}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Added = 1,
    Removed = 2,
    Modified = 3,
    RenameOldName = 4,
    RenameNewName = 5,
}
impl TryFrom<u16> for Kind {
    type Error = Error;
    fn try_from(value: u16) -> Result<Self> {
        match value {
            1 => Ok(Self::Added),
            2 => Ok(Self::Removed),
            3 => Ok(Self::Modified),
            4 => Ok(Self::RenameOldName),
            5 => Ok(Self::RenameNewName),
            _ => Err(Error::Invalid),
        }
    }
}

/// Ends the observation window only. Neither this value nor ACK proves close.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Continue = 0,
    WindowEnded = 1,
    Gap = 2,
    Revoked = 3,
    Cancelled = 4,
    ObservationFailed = 5,
}
impl TryFrom<u16> for End {
    type Error = Error;
    fn try_from(value: u16) -> Result<Self> {
        match value {
            0 => Ok(Self::Continue),
            1 => Ok(Self::WindowEnded),
            2 => Ok(Self::Gap),
            3 => Ok(Self::Revoked),
            4 => Ok(Self::Cancelled),
            5 => Ok(Self::ObservationFailed),
            _ => Err(Error::Invalid),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Change {
    pub kind: Kind,
    /// Raw little-endian UTF-16 filename units, including unpaired surrogates.
    /// A single observed component, not an openable path or opaque file grant.
    pub name_utf16_le: Vec<u8>,
}
impl fmt::Debug for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Change")
            .field("kind", &self.kind)
            .field("name_bytes", &self.name_utf16_le.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Batch {
    pub watch_epoch: [u8; 32],
    pub sequence: u64,
    pub changes: Vec<Change>,
    pub end: End,
}
impl fmt::Debug for Batch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Batch")
            .field("sequence", &self.sequence)
            .field("changes", &self.changes.len())
            .field("end", &self.end)
            .finish_non_exhaustive()
    }
}

fn validate_name(name: &[u8]) -> Result<()> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES || !name.len().is_multiple_of(2) {
        return Err(Error::Limit);
    }
    if name == [b'.', 0] || name == [b'.', 0, b'.', 0] {
        return Err(Error::Invalid);
    }
    if name
        .chunks_exact(2)
        .any(|u| matches!(u16::from_le_bytes([u[0], u[1]]), 0 | 0x2f | 0x5c | 0x3a))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

/// Reject excessive segment declarations before an owned Cap'n Proto reader
/// can allocate segment storage. Every declared byte must exist exactly once.
fn serialized_size(bytes: &[u8]) -> Result<()> {
    if bytes.len() < 16 || bytes.len() > MAX_WIRE_BYTES || !bytes.len().is_multiple_of(8) {
        return Err(Error::Limit);
    }
    let segments = u32::from_le_bytes(bytes[..4].try_into().unwrap())
        .checked_add(1)
        .ok_or(Error::Limit)? as usize;
    if segments > MAX_SEGMENTS {
        return Err(Error::Limit);
    }
    let table_bytes = ((segments + 2) & !1) * 4;
    if table_bytes > bytes.len() {
        return Err(Error::Invalid);
    }
    let mut payload_bytes = 0usize;
    for index in 0..segments {
        let start = 4 * (index + 1);
        let words = u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()) as usize;
        payload_bytes = payload_bytes
            .checked_add(words.checked_mul(8).ok_or(Error::Limit)?)
            .ok_or(Error::Limit)?;
        if payload_bytes > MAX_WIRE_BYTES {
            return Err(Error::Limit);
        }
    }
    if table_bytes.checked_add(payload_bytes) != Some(bytes.len()) {
        return Err(Error::Invalid);
    }
    Ok(())
}

fn exact_layout<'a>(
    root: impl IntoInternalStructReader<'a>,
    data: u16,
    pointers: u16,
) -> Result<()> {
    let reader = root.into_internal_struct_reader();
    if reader.get_data_section_size() != u32::from(data) * 64
        || reader.get_pointer_section_size() != pointers
    {
        return Err(Error::Contract);
    }
    Ok(())
}

impl Batch {
    pub fn validate(&self) -> Result<()> {
        if self.watch_epoch == [0; 32] || self.sequence == 0 {
            return Err(Error::Invalid);
        }
        if self.changes.len() > MAX_CHANGES {
            return Err(Error::Limit);
        }
        if self.end == End::Continue && (self.changes.is_empty() || self.sequence == u64::MAX) {
            return Err(Error::Invalid);
        }
        if matches!(
            self.end,
            End::Gap | End::Revoked | End::Cancelled | End::ObservationFailed
        ) && !self.changes.is_empty()
        {
            return Err(Error::Invalid);
        }
        let mut total = 0usize;
        for change in &self.changes {
            validate_name(&change.name_utf16_le)?;
            total = total
                .checked_add(change.name_utf16_le.len())
                .ok_or(Error::Limit)?;
            if total > MAX_TOTAL_NAME_BYTES {
                return Err(Error::Limit);
            }
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut message = Builder::new_default();
        {
            let mut root = message.init_root::<wire::batch::Builder<'_>>();
            root.set_version(VERSION);
            root.set_end(self.end as u16);
            root.set_reserved(0);
            root.set_sequence(self.sequence);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_watch_epoch(&self.watch_epoch);
            let mut changes = root.init_changes(self.changes.len() as u32);
            for (i, change) in self.changes.iter().enumerate() {
                let mut item = changes.reborrow().get(i as u32);
                item.set_kind(change.kind as u16);
                item.set_reserved(0);
                item.set_reserved2(0);
                item.set_name_utf16_le(&change.name_utf16_le);
            }
        }
        let encoded = serialize::write_message_to_words(&message);
        serialized_size(&encoded)?;
        Ok(encoded)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        serialized_size(bytes)?;
        let mut remaining = bytes;
        let message = serialize::read_message(
            &mut remaining,
            ReaderOptions {
                traversal_limit_in_words: Some(4 * MAX_WIRE_BYTES / 8),
                nesting_limit: 8,
            },
        )
        .map_err(|_| Error::Invalid)?;
        if !remaining.is_empty() {
            return Err(Error::Invalid);
        }
        let root = message
            .get_root::<wire::batch::Reader<'_>>()
            .map_err(|_| Error::Invalid)?;
        let size = <wire::batch::Builder<'_> as HasStructSize>::STRUCT_SIZE;
        exact_layout(root, size.data, size.pointers)?;
        if root.total_size().map_err(|_| Error::Invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        if root.get_version() != VERSION
            || root.get_schema_sha256().map_err(|_| Error::Invalid)? != SCHEMA_DIGEST
        {
            return Err(Error::Contract);
        }
        if root.get_reserved() != 0 {
            return Err(Error::Invalid);
        }
        let watch_epoch: [u8; 32] = root
            .get_watch_epoch()
            .map_err(|_| Error::Invalid)?
            .try_into()
            .map_err(|_| Error::Invalid)?;
        let end = End::try_from(root.get_end())?;
        let sequence = root.get_sequence();
        let list = root.get_changes().map_err(|_| Error::Invalid)?;
        if list.len() as usize > MAX_CHANGES {
            return Err(Error::Limit);
        }
        // Validate the complete aggregate before copying any names. Shared wire
        // pointers still count once for each semantic field that will be owned.
        let mut total = 0usize;
        let entry_size = <wire::change::Builder<'_> as HasStructSize>::STRUCT_SIZE;
        for item in list.iter() {
            exact_layout(item, entry_size.data, entry_size.pointers)?;
            Kind::try_from(item.get_kind())?;
            if item.get_reserved() != 0 || item.get_reserved2() != 0 {
                return Err(Error::Invalid);
            }
            let name = item.get_name_utf16_le().map_err(|_| Error::Invalid)?;
            validate_name(name)?;
            total = total.checked_add(name.len()).ok_or(Error::Limit)?;
            if total > MAX_TOTAL_NAME_BYTES {
                return Err(Error::Limit);
            }
        }
        let mut changes = Vec::new();
        changes
            .try_reserve_exact(list.len() as usize)
            .map_err(|_| Error::Allocation)?;
        for item in list.iter() {
            let name = item.get_name_utf16_le().map_err(|_| Error::Invalid)?;
            let mut owned = Vec::new();
            owned
                .try_reserve_exact(name.len())
                .map_err(|_| Error::Allocation)?;
            owned.extend_from_slice(name);
            changes.push(Change {
                kind: Kind::try_from(item.get_kind())?,
                name_utf16_le: owned,
            });
        }
        let result = Self {
            watch_epoch,
            sequence,
            changes,
            end,
        };
        result.validate()?;
        Ok(result)
    }
}

/// Correlates an ACK with the exact accepted wire, including encoding shape.
pub fn wire_digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
