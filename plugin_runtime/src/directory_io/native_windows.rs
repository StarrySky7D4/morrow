//! Only synchronous queries of an already owned directory File. No guest pointers,
//! paths, handles, child opens or platform fallback. Layouts: Windows SDK 26100
//! winbase.h FILE_ID_INFO and FILE_ID_EXTD_DIR_INFO, minwinbase.h enum values.
use super::Error;
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    mem::ManuallyDrop,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, IntoRawHandle},
    },
};

pub(super) const BUFFER_BYTES: usize = 16 * 1024;
const HEADER_BYTES: usize = 88;
const DIRECTORY: u32 = 0x10;
const REPARSE: u32 = 0x400;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetLastError() -> u32;
    fn GetFileInformationByHandleEx(
        handle: *mut c_void,
        class: i32,
        buffer: *mut c_void,
        size: u32,
    ) -> i32;
    fn GetFinalPathNameByHandleW(
        handle: *mut c_void,
        name: *mut u16,
        length: u32,
        flags: u32,
    ) -> u32;
}

/// No implicit File close: the original worker records one native attempt.
pub(super) struct NativeFile {
    file: Option<ManuallyDrop<File>>,
    attempted: bool,
    closed: bool,
    unknown: bool,
    raw: usize,
    error: Option<u32>,
}
impl NativeFile {
    pub(super) fn new(file: File) -> Self {
        Self {
            file: Some(ManuallyDrop::new(file)),
            attempted: false,
            closed: false,
            unknown: false,
            raw: 0,
            error: None,
        }
    }
    pub(super) fn file(&self) -> Option<&File> {
        if self.unknown {
            None
        } else {
            self.file.as_deref()
        }
    }
    pub(super) fn attempted(&self) -> bool {
        self.attempted
    }
    pub(super) fn closed(&self) -> bool {
        self.closed
    }
    pub(super) fn unknown(&self) -> bool {
        self.unknown
    }
    pub(super) fn mark_unknown(&mut self) {
        if !self.closed {
            self.unknown = true;
        }
    }
    pub(super) fn close_once(&mut self, lose_success: bool) {
        if self.closed || self.unknown || self.attempted {
            return;
        }
        let Some(file) = self.file.take() else {
            self.unknown = true;
            return;
        };
        let raw = ManuallyDrop::into_inner(file).into_raw_handle();
        self.raw = raw as usize;
        self.attempted = true;
        // Latch before entering the OS. Interrupted/lost observation is final.
        self.unknown = true;
        // SAFETY: exactly this original owned File was consumed, never cloned
        // or reconstructed. No second attempt is permitted for this record.
        let success = unsafe { CloseHandle(raw) } != 0;
        let error = if success {
            None
        } else {
            Some(unsafe { GetLastError() })
        };
        self.error = error;
        // Test-only loss occurs AFTER the one real close. It never closes a
        // forged/stale handle and is not evidence of an actual OS failure.
        if success && !lose_success {
            self.closed = true;
            self.unknown = false;
        }
    }
}

/// An actual local volume root, never a SUBST directory or network alias.
/// The fixed bounded query rejects unsupported providers and non-root paths.
/// GetFinalPathNameByHandleW with VOLUME_NAME_GUID is documented at:
/// https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew
pub(super) fn open_volume_root(drive: u8) -> Result<File, Error> {
    if !drive.is_ascii_alphabetic() {
        return Err(Error::InvalidNativeRecord);
    }
    let root = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(0x0220_0000)
        .open(format!("{}:\\", char::from(drive)))
        .map_err(|error| Error::Io(error.kind()))?;
    Ok(root)
}

pub(super) fn verify_volume_root(root: &File) -> Result<(), Error> {
    let mut name = [0u16; 128];
    // SAFETY: original owned File remains live; the stack buffer is initialized,
    // exclusively borrowed and exactly the stated capacity. No pointer escapes.
    let length = unsafe {
        GetFinalPathNameByHandleW(
            root.as_raw_handle(),
            name.as_mut_ptr(),
            name.len() as u32,
            1,
        )
    } as usize;
    if length == 0 {
        return Err(Error::Io(std::io::Error::last_os_error().kind()));
    }
    if length >= name.len() || !is_volume_root_name(&name[..length]) {
        return Err(Error::UnsupportedNativeQuery);
    }
    Ok(())
}

fn is_volume_root_name(name: &[u16]) -> bool {
    let prefix: Vec<u16> = r"\\?\Volume{".encode_utf16().collect();
    name.len() == 49
        && name.starts_with(&prefix)
        && name[47..] == [125, 92]
        && name[11..47].iter().enumerate().all(|(i, unit)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *unit == 45
            } else {
                matches!(*unit, 48..=57 | 65..=70 | 97..=102)
            }
        })
}

#[cfg(test)]
mod selected_root_tests {
    use super::*;
    #[test]
    fn final_name_must_be_the_volume_itself_not_a_resolved_alias_below_it() {
        let root = r"\\?\Volume{01234567-89ab-cdef-0123-456789abcdef}\";
        assert!(is_volume_root_name(
            &root.encode_utf16().collect::<Vec<_>>()
        ));
        for name in [
            format!("{root}substituted"),
            root.replace("abcdef", "abcdeg"),
            root.replace("Volume", "Device"),
            String::from(r"\\server\share\"),
            String::from(r"C:\"),
        ] {
            assert!(!is_volume_root_name(
                &name.encode_utf16().collect::<Vec<_>>()
            ));
        }
    }
}

fn query(file: &File, class: i32, words: &mut [u64]) -> Result<bool, Error> {
    let size = u32::try_from(words.len() * 8).map_err(|_| Error::Limit)?;
    // SAFETY: File retains its valid owned handle throughout this synchronous
    // call. The initialized u64 slice is writable, 8-byte aligned, exactly size
    // bytes long and exclusively borrowed; no pointers are retained by the API.
    // All information classes used here are synchronous handle query classes.
    if unsafe {
        GetFileInformationByHandleEx(file.as_raw_handle(), class, words.as_mut_ptr().cast(), size)
    } != 0
    {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(18) if class == 19 || class == 20 => Ok(false), // ERROR_NO_MORE_FILES
        Some(1 | 50 | 87) => Err(Error::UnsupportedNativeQuery),
        Some(234) => Err(Error::Limit), // no partial record on ERROR_MORE_DATA
        _ => Err(Error::Io(error.kind())),
    }
}

/// Selected object identity only. Does not authenticate a path or its ancestors.
pub(super) fn identity(file: &File) -> Result<[u8; 24], Error> {
    let mut attributes = [0u64; 1];
    query(file, 9, &mut attributes)?;
    let bytes = attributes[0].to_le_bytes();
    let flags = u32::from_le_bytes(bytes[..4].try_into().unwrap());
    let tag = u32::from_le_bytes(bytes[4..].try_into().unwrap());
    if flags & REPARSE != 0 || tag != 0 {
        return Err(Error::ReparsePoint);
    }
    if flags & DIRECTORY == 0 {
        return Err(Error::NotDirectory);
    }
    let mut info = [0u64; 3];
    query(file, 18, &mut info)?;
    let mut id = [0u8; 24];
    for (chunk, word) in id.chunks_exact_mut(8).zip(info) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    if id[8..].iter().all(|b| *b == 0) {
        return Err(Error::UnsupportedNativeQuery);
    }
    Ok(id)
}

pub(super) struct Buffer {
    words: Vec<u64>,
}
impl Buffer {
    pub(super) fn new() -> Result<Self, Error> {
        let mut words = Vec::new();
        words
            .try_reserve_exact(BUFFER_BYTES / 8)
            .map_err(|_| Error::Allocation)?;
        words.resize(BUFFER_BYTES / 8, 0);
        Ok(Self { words })
    }
    fn byte(&self, i: usize) -> u8 {
        self.words[i / 8].to_le_bytes()[i % 8]
    }
    fn u32(&self, i: usize) -> u32 {
        u32::from_le_bytes(std::array::from_fn(|j| self.byte(i + j)))
    }
    fn i64(&self, i: usize) -> i64 {
        i64::from_le_bytes(std::array::from_fn(|j| self.byte(i + j)))
    }
    pub(super) fn next(&mut self, file: &File, restart: bool) -> Result<Option<Vec<Entry>>, Error> {
        self.words.fill(0);
        if !query(file, if restart { 20 } else { 19 }, &mut self.words)? {
            return Ok(None);
        }
        let mut result = Vec::new();
        result
            .try_reserve_exact(BUFFER_BYTES / HEADER_BYTES)
            .map_err(|_| Error::Allocation)?;
        let mut position = 0usize;
        loop {
            if position
                .checked_add(HEADER_BYTES)
                .is_none_or(|n| n > BUFFER_BYTES)
            {
                return Err(Error::InvalidNativeRecord);
            }
            let next = self.u32(position) as usize;
            let name_len = self.u32(position + 60) as usize;
            let end = position
                .checked_add(HEADER_BYTES)
                .and_then(|n| n.checked_add(name_len))
                .ok_or(Error::InvalidNativeRecord)?;
            if name_len == 0 || !name_len.is_multiple_of(2) || end > BUFFER_BYTES {
                return Err(Error::InvalidNativeRecord);
            }
            if next != 0
                && (!next.is_multiple_of(8)
                    || next < HEADER_BYTES + name_len
                    || position
                        .checked_add(next)
                        .is_none_or(|n| n + HEADER_BYTES > BUFFER_BYTES))
            {
                return Err(Error::InvalidNativeRecord);
            }
            let mut name = Vec::new();
            name.try_reserve_exact(name_len)
                .map_err(|_| Error::Allocation)?;
            name.extend((position + HEADER_BYTES..end).map(|i| self.byte(i)));
            // Native navigation records are not children; their buffers remain charged.
            if name != [b'.', 0] && name != [b'.', 0, b'.', 0] {
                if name
                    .chunks_exact(2)
                    .any(|p| matches!(u16::from_le_bytes([p[0], p[1]]), 0 | 47 | 92))
                {
                    return Err(Error::InvalidNativeRecord);
                }
                let flags = self.u32(position + 56);
                if flags & REPARSE != 0 || self.u32(position + 68) != 0 {
                    return Err(Error::ReparsePoint);
                }
                let length = self.i64(position + 40);
                if length < 0 {
                    return Err(Error::InvalidNativeRecord);
                }
                let id = std::array::from_fn(|i| self.byte(position + 72 + i));
                if id == [0; 16] {
                    return Err(Error::UnsupportedNativeQuery);
                }
                result.push(Entry {
                    name,
                    id,
                    directory: flags & DIRECTORY != 0,
                    other: flags & 0x40 != 0, // FILE_ATTRIBUTE_DEVICE, no regular-file length claim
                    length: length as u64,
                });
            }
            if next == 0 {
                break;
            }
            position += next;
        }
        Ok(Some(result))
    }
}

pub(super) struct Entry {
    pub(super) name: Vec<u8>,
    pub(super) id: [u8; 16],
    pub(super) directory: bool,
    pub(super) other: bool,
    pub(super) length: u64,
}

#[cfg(test)]
#[path = "native_close_refusal_tests.rs"]
mod native_close_refusal_tests;

// Test-only setup for a newly owned fixture. Unsafe calls stay in this module.
#[cfg(test)]
pub(super) fn protect_close_fixture(file: &File) -> std::io::Result<()> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetHandleInformation(handle: *mut c_void, mask: u32, flags: u32) -> i32;
        fn GetHandleInformation(handle: *mut c_void, flags: *mut u32) -> i32;
    }
    // SAFETY: the borrowed File owns a live handle for both synchronous calls.
    // Only its protect-from-close flag changes; no pointer or ownership escapes.
    if unsafe { SetHandleInformation(file.as_raw_handle(), 2, 2) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut flags = 0;
    // SAFETY: flags is a valid exclusive u32 output during this native call.
    if unsafe { GetHandleInformation(file.as_raw_handle(), &mut flags) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    if flags & 2 != 2 { return Err(std::io::Error::other("native fixture protection absent")); }
    Ok(())
}
