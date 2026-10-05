//! Bounded directory observations. Names and opaque identities are data, never
//! paths, FileRead/recursion/rename permissions or proof of an atomic OS snapshot.
#![deny(unsafe_op_in_unsafe_fn)]
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use std::fmt;
#[allow(unsafe_op_in_unsafe_fn)]
mod fs_directory_capnp {
    include!(concat!(env!("OUT_DIR"), "/fs_directory_capnp.rs"));
}
pub mod ffi;
pub mod state;
pub use state::{DirectoryState, StateLimits, StateSnapshot};
pub const PROFILE: &str = "fs-directory-v1";
pub const VERSION: u16 = 1;
pub const MAX_ENVELOPE_BYTES: usize = 65536;
pub const MAX_ENTRIES_PER_PAGE: usize = 32;
pub const MAX_NAME_BYTES_PER_PAGE: usize = 16384;
pub const TRAVERSAL_LIMIT_WORDS: usize = 16384;
pub const NESTING_LIMIT: i32 = 8;
pub const SCHEMA: &[u8] = include_bytes!("../contracts/fs_directory.capnp");
pub const SCHEMA_DIGEST: [u8; 32] = [
    173, 230, 13, 174, 231, 116, 151, 5, 106, 63, 230, 24, 179, 134, 22, 51, 27, 141, 93, 230, 27,
    219, 73, 79, 112, 53, 146, 231, 78, 195, 213, 247,
];
pub const fn schema_digest() -> [u8; 32] {
    SCHEMA_DIGEST
}
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid = 1,
    Contract = 2,
    Limit = 3,
    Utf8 = 4,
    Buffer = 5,
    Sequence = 6,
    Epoch = 7,
    Duplicate = 8,
    Terminal = 9,
    Budget = 10,
    Released = 11,
}
pub type Result<T> = std::result::Result<T, Error>;
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "directory payload {self:?}")
    }
}
impl std::error::Error for Error {}
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameEncoding {
    Utf8 = 1,
    Utf16Le = 2,
}
impl TryFrom<u32> for NameEncoding {
    type Error = Error;
    fn try_from(v: u32) -> Result<Self> {
        match v {
            1 => Ok(Self::Utf8),
            2 => Ok(Self::Utf16Le),
            _ => Err(Error::Invalid),
        }
    }
}
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File = 1,
    Directory = 2,
    Other = 3,
}
impl TryFrom<u32> for EntryKind {
    type Error = Error;
    fn try_from(v: u32) -> Result<Self> {
        match v {
            1 => Ok(Self::File),
            2 => Ok(Self::Directory),
            3 => Ok(Self::Other),
            _ => Err(Error::Invalid),
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub entry_id: [u8; 32],
    pub name: Vec<u8>,
    pub encoding: NameEncoding,
    pub kind: EntryKind,
    pub logical_length: Option<u64>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DirectoryEntryRef<'a> {
    pub entry_id: [u8; 32],
    pub name: &'a [u8],
    pub encoding: NameEncoding,
    pub kind: EntryKind,
    pub logical_length: Option<u64>,
}
impl DirectoryEntry {
    pub fn as_ref(&self) -> DirectoryEntryRef<'_> {
        DirectoryEntryRef {
            entry_id: self.entry_id,
            name: &self.name,
            encoding: self.encoding,
            kind: self.kind,
            logical_length: self.logical_length,
        }
    }
}
impl fmt::Debug for DirectoryEntryRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DirectoryEntry")
            .field("name_bytes", &self.name.len())
            .field("encoding", &self.encoding)
            .field("kind", &self.kind)
            .field("has_logical_length", &self.logical_length.is_some())
            .finish()
    }
}
impl fmt::Debug for DirectoryEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.as_ref(), f)
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct FsDirectoryPage {
    pub selection_epoch: [u8; 32],
    pub page_sequence: u64,
    pub entries: Vec<DirectoryEntry>,
    pub terminal: bool,
}
#[derive(Clone, Copy)]
pub struct FsDirectoryPageRef<'a> {
    pub selection_epoch: [u8; 32],
    pub page_sequence: u64,
    pub entries: &'a [DirectoryEntryRef<'a>],
    pub terminal: bool,
}
impl fmt::Debug for FsDirectoryPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FsDirectoryPage")
            .field("page_sequence", &self.page_sequence)
            .field("entries", &self.entries.len())
            .field("terminal", &self.terminal)
            .finish()
    }
}
impl fmt::Debug for FsDirectoryPageRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FsDirectoryPageRef")
            .field("page_sequence", &self.page_sequence)
            .field("entries", &self.entries.len())
            .field("terminal", &self.terminal)
            .finish()
    }
}
pub(crate) fn nonzero(id: &[u8; 32]) -> bool {
    id.iter().any(|b| *b != 0)
}
fn validate_name(name: &[u8], encoding: NameEncoding) -> Result<()> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES_PER_PAGE {
        return Err(Error::Limit);
    }
    match encoding {
        NameEncoding::Utf8 => {
            std::str::from_utf8(name).map_err(|_| Error::Utf8)?;
            if name.iter().any(|c| matches!(c, 0 | b'/' | b'\\')) || name == b"." || name == b".." {
                return Err(Error::Invalid);
            }
        }
        NameEncoding::Utf16Le => {
            if !name.len().is_multiple_of(2) {
                return Err(Error::Invalid);
            }
            if name
                .chunks_exact(2)
                .any(|c| matches!(u16::from_le_bytes([c[0], c[1]]), 0 | 47 | 92))
                || name == [46, 0]
                || name == [46, 0, 46, 0]
            {
                return Err(Error::Invalid);
            }
            // No lossy conversion or surrogate validation: raw Windows code units survive.
        }
    }
    Ok(())
}
impl DirectoryEntryRef<'_> {
    pub fn validate(&self) -> Result<()> {
        if !nonzero(&self.entry_id) {
            return Err(Error::Invalid);
        }
        validate_name(self.name, self.encoding)?;
        if self.kind != EntryKind::File && self.logical_length.is_some() {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
fn validate_page<'a>(
    epoch: &[u8; 32],
    sequence: u64,
    count: usize,
    entries: impl Iterator<Item = DirectoryEntryRef<'a>>,
) -> Result<usize> {
    if !nonzero(epoch) || sequence == 0 {
        return Err(Error::Invalid);
    }
    if count > MAX_ENTRIES_PER_PAGE {
        return Err(Error::Limit);
    }
    let mut ids = [[0u8; 32]; MAX_ENTRIES_PER_PAGE];
    let mut used = 0;
    let mut names = 0usize;
    for e in entries {
        if used >= count {
            return Err(Error::Invalid);
        }
        names = names
            .checked_add(e.name.len())
            .filter(|n| *n <= MAX_NAME_BYTES_PER_PAGE)
            .ok_or(Error::Limit)?;
        e.validate()?;
        if ids[..used].contains(&e.entry_id) {
            return Err(Error::Duplicate);
        }
        ids[used] = e.entry_id;
        used += 1;
    }
    if used != count {
        return Err(Error::Invalid);
    }
    Ok(names)
}
fn encode_page<'a>(
    epoch: [u8; 32],
    sequence: u64,
    count: usize,
    entries: impl Iterator<Item = DirectoryEntryRef<'a>>,
    terminal: bool,
) -> Result<Vec<u8>> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<fs_directory_capnp::page::Builder>();
    root.set_version(VERSION);
    root.set_schema_sha256(&SCHEMA_DIGEST);
    root.set_selection_epoch(&epoch);
    root.set_page_sequence(sequence);
    root.set_terminal(terminal);
    let mut list = root.init_entries(count as u32);
    for (i, e) in entries.enumerate() {
        let mut row = list.reborrow().get(i as u32);
        row.set_entry_id(&e.entry_id);
        row.set_name(e.name);
        row.set_encoding(match e.encoding {
            NameEncoding::Utf8 => fs_directory_capnp::NameEncoding::Utf8,
            NameEncoding::Utf16Le => fs_directory_capnp::NameEncoding::Utf16Le,
        });
        row.set_kind(match e.kind {
            EntryKind::File => fs_directory_capnp::EntryKind::File,
            EntryKind::Directory => fs_directory_capnp::EntryKind::Directory,
            EntryKind::Other => fs_directory_capnp::EntryKind::Other,
        });
        row.set_has_logical_length(e.logical_length.is_some());
        row.set_logical_length(e.logical_length.unwrap_or(0));
    }
    let wire = serialize::write_message_to_words(&message);
    if wire.len() > MAX_ENVELOPE_BYTES {
        return Err(Error::Limit);
    }
    Ok(wire)
}
impl FsDirectoryPageRef<'_> {
    pub fn validate(&self) -> Result<()> {
        validate_page(
            &self.selection_epoch,
            self.page_sequence,
            self.entries.len(),
            self.entries.iter().copied(),
        )
        .map(|_| ())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        encode_page(
            self.selection_epoch,
            self.page_sequence,
            self.entries.len(),
            self.entries.iter().copied(),
            self.terminal,
        )
    }
}
fn fixed_id(data: &[u8]) -> Result<[u8; 32]> {
    let id = data.try_into().map_err(|_| Error::Invalid)?;
    if !nonzero(&id) {
        return Err(Error::Invalid);
    }
    Ok(id)
}
fn row_ref(row: fs_directory_capnp::entry::Reader<'_>) -> Result<DirectoryEntryRef<'_>> {
    let entry_id = fixed_id(row.get_entry_id().map_err(|_| Error::Invalid)?)?;
    let name = row.get_name().map_err(|_| Error::Invalid)?;
    let encoding = match row.get_encoding().map_err(|_| Error::Invalid)? {
        fs_directory_capnp::NameEncoding::Utf8 => NameEncoding::Utf8,
        fs_directory_capnp::NameEncoding::Utf16Le => NameEncoding::Utf16Le,
        _ => return Err(Error::Invalid),
    };
    let kind = match row.get_kind().map_err(|_| Error::Invalid)? {
        fs_directory_capnp::EntryKind::File => EntryKind::File,
        fs_directory_capnp::EntryKind::Directory => EntryKind::Directory,
        fs_directory_capnp::EntryKind::Other => EntryKind::Other,
        _ => return Err(Error::Invalid),
    };
    if !row.get_has_logical_length() && row.get_logical_length() != 0 {
        return Err(Error::Invalid);
    }
    Ok(DirectoryEntryRef {
        entry_id,
        name,
        encoding,
        kind,
        logical_length: row
            .get_has_logical_length()
            .then(|| row.get_logical_length()),
    })
}
impl FsDirectoryPage {
    pub fn validate(&self) -> Result<()> {
        validate_page(
            &self.selection_epoch,
            self.page_sequence,
            self.entries.len(),
            self.entries.iter().map(DirectoryEntry::as_ref),
        )
        .map(|_| ())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        encode_page(
            self.selection_epoch,
            self.page_sequence,
            self.entries.len(),
            self.entries.iter().map(DirectoryEntry::as_ref),
            self.terminal,
        )
    }
    /// Fully bounded owned decode; every semantic/range/aggregate check precedes
    /// allocation of owned names or entries. ReaderOptions bounds segment allocation.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(Error::Limit);
        }
        let mut remaining = bytes;
        let message = serialize::read_message(
            &mut remaining,
            ReaderOptions {
                traversal_limit_in_words: Some(TRAVERSAL_LIMIT_WORDS),
                nesting_limit: NESTING_LIMIT,
            },
        )
        .map_err(|_| Error::Invalid)?;
        if !remaining.is_empty() {
            return Err(Error::Invalid);
        }
        let root = message
            .get_root::<fs_directory_capnp::page::Reader>()
            .map_err(|_| Error::Invalid)?;
        if root.total_size().map_err(|_| Error::Invalid)?.cap_count != 0 {
            return Err(Error::Invalid);
        }
        let digest = root.get_schema_sha256().map_err(|_| Error::Invalid)?;
        if root.get_version() != VERSION || digest != SCHEMA_DIGEST {
            return Err(Error::Contract);
        }
        let epoch_data = root.get_selection_epoch().map_err(|_| Error::Invalid)?;
        let epoch = fixed_id(epoch_data)?;
        let seq = root.get_page_sequence();
        if seq == 0 {
            return Err(Error::Invalid);
        }
        let list = root.get_entries().map_err(|_| Error::Invalid)?;
        let count = list.len() as usize;
        if count > MAX_ENTRIES_PER_PAGE {
            return Err(Error::Limit);
        }
        let mut refs = [DirectoryEntryRef {
            entry_id: [0; 32],
            name: &[],
            encoding: NameEncoding::Utf8,
            kind: EntryKind::Other,
            logical_length: None,
        }; MAX_ENTRIES_PER_PAGE];
        for (i, row) in list.iter().enumerate() {
            refs[i] = row_ref(row)?;
        }
        validate_page(&epoch, seq, count, refs[..count].iter().copied())?;
        let entries = refs[..count]
            .iter()
            .map(|e| DirectoryEntry {
                entry_id: e.entry_id,
                name: e.name.to_vec(),
                encoding: e.encoding,
                kind: e.kind,
                logical_length: e.logical_length,
            })
            .collect();
        Ok(Self {
            selection_epoch: epoch,
            page_sequence: seq,
            entries,
            terminal: root.get_terminal(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn page(seq: u64, id: u8, terminal: bool) -> FsDirectoryPage {
        FsDirectoryPage {
            selection_epoch: [1; 32],
            page_sequence: seq,
            entries: vec![DirectoryEntry {
                entry_id: [id; 32],
                name: b"private-name".to_vec(),
                encoding: NameEncoding::Utf8,
                kind: EntryKind::File,
                logical_length: Some(u64::MAX),
            }],
            terminal,
        }
    }
    #[test]
    fn complete_entries_utf16_codeunits_and_empty_terminal_roundtrip() {
        let mut p = page(1, 2, false);
        p.entries.push(DirectoryEntry {
            entry_id: [3; 32],
            name: vec![0, 0xd8, 0x78, 0],
            encoding: NameEncoding::Utf16Le,
            kind: EntryKind::Directory,
            logical_length: None,
        });
        p.entries.push(DirectoryEntry {
            entry_id: [4; 32],
            name: "雪🙂".as_bytes().to_vec(),
            encoding: NameEncoding::Utf8,
            kind: EntryKind::Other,
            logical_length: None,
        });
        assert_eq!(FsDirectoryPage::decode(&p.encode().unwrap()).unwrap(), p);
        let debug = format!("{:?}{:?}", p, p.entries);
        assert!(!debug.contains("private-name"));
        assert!(!debug.contains("雪"));
        assert!(!debug.contains(&u64::MAX.to_string()));
        p.entries.clear();
        p.terminal = true;
        assert_eq!(FsDirectoryPage::decode(&p.encode().unwrap()).unwrap(), p);
    }
    #[test]
    fn strict_names_identity_length_and_duplicate_policy() {
        for name in [
            b"".as_slice(),
            b".",
            b"..",
            b"a/b",
            b"a\\b",
            b"a\0b",
            b"\xff",
        ] {
            let mut p = page(1, 2, false);
            p.entries[0].name = name.to_vec();
            assert!(p.encode().is_err());
        }
        for name in [
            vec![0],
            vec![0, 0],
            vec![46, 0],
            vec![46, 0, 46, 0],
            vec![47, 0],
            vec![92, 0],
        ] {
            let mut p = page(1, 2, false);
            p.entries[0].name = name;
            p.entries[0].encoding = NameEncoding::Utf16Le;
            assert!(p.encode().is_err());
        }
        let mut p = page(1, 0, false);
        assert_eq!(p.validate(), Err(Error::Invalid));
        p.entries[0].entry_id = [2; 32];
        p.selection_epoch = [0; 32];
        assert_eq!(p.validate(), Err(Error::Invalid));
        p.selection_epoch = [1; 32];
        p.page_sequence = 0;
        assert_eq!(p.validate(), Err(Error::Invalid));
        p.page_sequence = 1;
        p.entries.push(p.entries[0].clone());
        assert_eq!(p.validate(), Err(Error::Duplicate));
        p.entries.pop();
        p.entries[0].kind = EntryKind::Directory;
        assert_eq!(p.validate(), Err(Error::Invalid));
    }
    #[test]
    fn entry_and_name_limits_before_encoding_with_real_multisegment_allocator() {
        let mut p = page(1, 2, false);
        p.entries = (1..=32)
            .map(|i| DirectoryEntry {
                entry_id: [i; 32],
                name: vec![b'x'; 512],
                encoding: NameEncoding::Utf8,
                kind: EntryKind::File,
                logical_length: None,
            })
            .collect();
        let raw = p.encode().unwrap();
        assert!(u32::from_le_bytes(raw[..4].try_into().unwrap()) > 0);
        assert_eq!(FsDirectoryPage::decode(&raw).unwrap(), p);
        p.entries[0].name.push(b'x');
        assert_eq!(p.encode(), Err(Error::Limit));
        p.entries[0].name.pop();
        p.entries.push(p.entries[0].clone());
        assert_eq!(p.validate(), Err(Error::Limit));
    }
    #[test]
    fn borrowed_encode_does_not_own_or_modify_names() {
        let name = b"borrowed";
        let rows = [DirectoryEntryRef {
            entry_id: [2; 32],
            name,
            encoding: NameEncoding::Utf8,
            kind: EntryKind::File,
            logical_length: Some(0),
        }];
        let view = FsDirectoryPageRef {
            selection_epoch: [1; 32],
            page_sequence: 1,
            entries: &rows,
            terminal: true,
        };
        let p = FsDirectoryPage::decode(&view.encode().unwrap()).unwrap();
        assert_eq!(p.entries[0].name, name);
        assert_eq!(p.entries[0].logical_length, Some(0));
    }
    #[test]
    fn malformed_contract_tail_utf8_and_lengthflag_reject() {
        let raw = page(1, 2, false).encode().unwrap();
        let mut bad = raw.clone();
        bad.extend([0; 8]);
        assert_eq!(FsDirectoryPage::decode(&bad), Err(Error::Invalid));
        let mut bad = raw.clone();
        bad[16] = 2;
        assert_eq!(FsDirectoryPage::decode(&bad), Err(Error::Contract));
        let mut bad = raw;
        let offset = bad.windows(12).position(|s| s == b"private-name").unwrap();
        bad[offset] = 255;
        assert_eq!(FsDirectoryPage::decode(&bad), Err(Error::Utf8));
        let mut msg = Builder::new_default();
        {
            let mut root = msg.init_root::<fs_directory_capnp::page::Builder>();
            root.set_version(VERSION);
            root.set_schema_sha256(&SCHEMA_DIGEST);
            root.set_selection_epoch(&[1; 32]);
            root.set_page_sequence(1);
            let mut entry = root.init_entries(1).get(0);
            entry.set_entry_id(&[2; 32]);
            entry.set_name(b"entry");
            entry.set_encoding(fs_directory_capnp::NameEncoding::Utf8);
            entry.set_kind(fs_directory_capnp::EntryKind::File);
            entry.set_logical_length(1);
        }
        assert_eq!(
            FsDirectoryPage::decode(&serialize::write_message_to_words(&msg)),
            Err(Error::Invalid)
        );
    }
}
