//! Windows-only same-handle no-replace publication. Not a reparse-confinement proof.
use super::{observe_result, KeyDiagnosticStage, KeyError, Result, MAX_FILE};
use std::{ffi::c_void, fs::{File, OpenOptions}, io::{self, Read, Seek, SeekFrom, Write}, mem::size_of,
    path::Path, sync::atomic::{AtomicBool, Ordering}};
use std::os::windows::{fs::{MetadataExt, OpenOptionsExt}, io::{AsRawHandle, IntoRawHandle}};
static POISONED: AtomicBool = AtomicBool::new(false);
fn poison() -> KeyError { POISONED.store(true, Ordering::Release); KeyError::PublishUnknown }
pub(super) fn admit() -> Result<()> {
    if POISONED.load(Ordering::Acquire) { Err(KeyError::PublishUnknown) } else { Ok(()) }
}
fn valid_leaf(value: &str) -> bool {
    !value.is_empty() && value != "." && value != ".." && value.encode_utf16().count() <= 255
        && !value.ends_with(['.', ' '])
        && !value.chars().any(|c| c < ' ' || matches!(c, '/' | '\\' | ':' | '\0' | '<' | '>' | '"' | '|' | '?' | '*'))
}
fn temporary_leaf(id: &str) -> Result<String> {
    let Some(hex) = id.strip_prefix("audit-") else { return Err(KeyError::Format) };
    if hex.len() != 32 || !hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err(KeyError::Format);
    }
    Ok(format!(".morrow-key-{id}.tmp"))
}
fn io_error(error: io::Error) -> KeyError {
    if error.kind() == io::ErrorKind::AlreadyExists { KeyError::AlreadyExists } else { KeyError::Io(error) }
}
#[repr(C)]
struct FileDispositionInfo {
    delete_file: u8,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetFileInformationByHandle(
        handle: *mut c_void,
        class: i32,
        information: *mut c_void,
        size: u32,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}
// Windows native structures use pointer-sized alignment even though NTSTATUS
// itself is 32-bit. repr(C) preserves the union/pointer padding on x86 and x64.
#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}
#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: *mut c_void,
    object_name: *mut UnicodeString,
    attributes: u32,
    security_descriptor: *mut c_void,
    security_quality_of_service: *mut c_void,
}
#[repr(C)]
union IoStatus {
    status: i32,
    pointer: *mut c_void,
}
#[repr(C)]
struct IoStatusBlock {
    status: IoStatus,
    information: usize,
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtCreateFile(
        handle: *mut *mut c_void,
        access: u32,
        attributes: *mut ObjectAttributes,
        status: *mut IoStatusBlock,
        allocation_size: *const i64,
        file_attributes: u32,
        share: u32,
        disposition: u32,
        options: u32,
        ea_buffer: *const c_void,
        ea_length: u32,
    ) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}
#[repr(C)]
struct RenameInformation {
    flags: u32,
    root_directory: *mut c_void,
    file_name_length: u32,
    file_name: [u16; 256],
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSetInformationFile(
        file: *mut c_void,
        status: *mut IoStatusBlock,
        information: *const c_void,
        length: u32,
        class: i32,
    ) -> i32;
}
fn open_new(parent: &File, name: &str) -> std::io::Result<File> {
    use std::{os::windows::io::FromRawHandle, ptr::null_mut};
    if !valid_leaf(name) {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    let mut encoded: Vec<u16> = name.encode_utf16().collect();
    let bytes = (encoded.len() * 2) as u16;
    let mut name = UnicodeString {
        length: bytes,
        maximum_length: bytes,
        buffer: encoded.as_mut_ptr(),
    };
    let mut attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: parent.as_raw_handle(),
        object_name: &mut name,
        attributes: 0x40,
        security_descriptor: null_mut(),
        security_quality_of_service: null_mut(),
    };
    let mut status = IoStatusBlock {
        status: IoStatus {
            pointer: null_mut(),
        },
        information: 0,
    };
    let mut raw = null_mut();
    // SAFETY: pointers and UTF-16 storage live through synchronous NtCreateFile.
    // GENERIC_READ|WRITE|DELETE|SYNCHRONIZE. FILE_CREATE=2 never replaces an
    // existing name. NON_DIRECTORY|OPEN_REPARSE_POINT|SYNCHRONOUS_IO_NONALERT.
    // Share none; ordinary attributes; notably NOT FILE_DELETE_ON_CLOSE.
    let result = unsafe {
        NtCreateFile(
            &mut raw,
            0xc011_0000,
            &mut attributes,
            &mut status,
            std::ptr::null(),
            0x80,
            0,
            2,
            0x0020_0060,
            std::ptr::null(),
            0,
        )
    };
    if result < 0 {
        // SAFETY: stateless status conversion.
        return Err(std::io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(result) } as i32,
        ));
    }
    if raw.is_null() || raw as isize == -1 {
        POISONED.store(true, Ordering::Release);
        return Err(std::io::Error::other("invalid created file handle"));
    }
    // SAFETY: one new owned synchronous handle; wrap once and transfer with File.
    Ok(unsafe { File::from_raw_handle(raw) })
}
fn rename_same_directory(file: &File, leaf: &str) -> std::io::Result<()> {
    if !valid_leaf(leaf) {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    // SAFETY: all-zero integer/array/null-pointer fields form a valid value.
    // The native API consumes fields; repr(C) provides their ABI alignment.
    let mut info: RenameInformation = unsafe { std::mem::zeroed() };
    let name: Vec<u16> = leaf.encode_utf16().collect();
    info.file_name[..name.len()].copy_from_slice(&name);
    info.file_name_length = (name.len() * 2) as u32;
    let mut status = IoStatusBlock {
        status: IoStatus {
            pointer: std::ptr::null_mut(),
        },
        information: 0,
    };
    // SAFETY: class10 FILE_RENAME_INFORMATION, aligned repr(C) buffer, flags0
    // means ReplaceIfExists=false. Nt API with NULL root + a SINGLE name renames
    // within the source handle's current parent; no process-CWD path resolution.
    // Source is synchronous, live and DELETE-capable; its immediate parent is
    // retained with share READ only. Caller ancestry validation is separate;
    // this is not a continuous reparse-confinement guarantee. Buffer is padded.
    let result = unsafe {
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut status,
            (&info as *const RenameInformation).cast(),
            size_of::<RenameInformation>() as u32,
            10,
        )
    };
    if result < 0 {
        // SAFETY: stateless status conversion.
        Err(std::io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(result) } as i32,
        ))
    } else {
        Ok(())
    }
}
fn close_created(file: File) -> bool {
    let raw = file.into_raw_handle();
    // SAFETY: transfer sole ownership and close exactly once. Never reconstruct
    // or retry after failure: Windows does not guarantee handle validity then.
    if unsafe { CloseHandle(raw) } != 0 {
        true
    } else {
        POISONED.store(true, Ordering::Release);
        false
    }
}

// No path-based deletion: mark only our original unpublished exclusive handle.
fn cleanup(file: File) -> bool {
    let mut info = FileDispositionInfo { delete_file: 1 };
    // SAFETY: live DELETE-capable original handle and one-byte FILE_DISPOSITION_INFO.
    let marked = unsafe { SetFileInformationByHandle(file.as_raw_handle(), 4,
        (&mut info as *mut FileDispositionInfo).cast(), size_of::<FileDispositionInfo>() as u32) } != 0;
    let closed = close_created(file);
    marked && closed
}
pub(super) struct Publication {
    parent: Option<File>, file: Option<File>, target: String,
    staged: bool, published: bool, finished: bool,
}
impl Publication {
    pub(super) fn create(parent: &Path, target_leaf: &str, public_random_id: &str) -> Result<Self> {
        admit()?;
        if !valid_leaf(target_leaf) { return Err(KeyError::Format); }
        let temporary = temporary_leaf(public_random_id)?;
        let parent = OpenOptions::new().read(true).share_mode(1)
            .custom_flags(0x02000000 | 0x00200000).open(parent).map_err(KeyError::Io)?;
        let metadata = parent.metadata();
        let error = match metadata {
            Ok(m) if m.is_dir() && m.file_attributes() & 0x400 == 0 => None,
            Ok(_) => Some(KeyError::Format),
            Err(e) => Some(KeyError::Io(e)),
        };
        if let Some(error) = error {
            return Err(if close_created(parent) { error } else { poison() });
        }
        let file = match open_new(&parent, &temporary) {
            Ok(file) => file,
            Err(e) => {
                let error = io_error(e);
                let closed = close_created(parent);
                return Err(if closed && admit().is_ok() { error } else { poison() });
            }
        };
        Ok(Self { parent: Some(parent), file: Some(file), target: target_leaf.to_owned(),
            staged: false, published: false, finished: false })
    }
    fn fail(&mut self, primary: KeyError) -> KeyError {
        let mut known = !self.published;
        if let Some(file) = self.file.take() {
            let closed = if self.published { close_created(file) } else { cleanup(file) };
            known &= closed;
        }
        if let Some(parent) = self.parent.take() { known &= close_created(parent); }
        self.finished = true;
        if known { primary } else { poison() }
    }
    pub(super) fn stage(&mut self, bytes: &[u8]) -> Result<()> {
        if let Err(e) = admit() { return Err(self.fail(e)); }
        if self.finished || self.staged || self.published { return Err(self.fail(KeyError::Format)); }
        let result = self.file.as_mut().ok_or(KeyError::PublishUnknown)
            .and_then(|file| file.write_all(bytes).and_then(|_| file.sync_all()).map_err(KeyError::Io));
        match result { Ok(()) => { self.staged = true; Ok(()) }, Err(e) => Err(self.fail(e)) }
    }
    pub(super) fn publish(&mut self) -> Result<()> {
        if let Err(e) = admit() { return Err(self.fail(e)); }
        if self.finished || !self.staged || self.published { return Err(self.fail(KeyError::Format)); }
        let result = self.file.as_ref().ok_or(KeyError::PublishUnknown)
            .and_then(|file| rename_same_directory(file, &self.target).map_err(io_error));
        match result { Ok(()) => { self.published = true; Ok(()) }, Err(e) => Err(self.fail(e)) }
    }
    /// Synchronize, reread and validate the original exclusive file before checked close.
    /// No duplicate handle or path acquisition occurs here. This does not prove ancestry.
    pub(super) fn sync_read_close<T>(mut self, validate: impl FnOnce(&[u8]) -> Result<T>) -> Result<T> {
        if let Err(e) = observe_result(KeyDiagnosticStage::PublishedSync, admit()) {
            return Err(self.fail(e));
        }
        if self.finished || !self.published {
            let error = observe_result::<()>(KeyDiagnosticStage::PublishedSync, Err(KeyError::Format))
                .expect_err("invalid publication state");
            return Err(self.fail(error));
        }
        let verified = (|| -> Result<T> {
            let file = observe_result(KeyDiagnosticStage::ReloadRead,
                self.file.as_mut().ok_or(KeyError::PublishUnknown))?;
            observe_result(KeyDiagnosticStage::PublishedSync, file.sync_all().map_err(KeyError::Io))?;
            let reread = (|| -> Result<Vec<u8>> {
            let before = file.metadata()?;
            if !before.is_file() || before.file_attributes() & 0x400 != 0
                || before.len() > MAX_FILE as u64 {
                return Err(KeyError::Format);
            }
            if file.seek(SeekFrom::Start(0))? != 0 { return Err(KeyError::Format); }
            let mut bytes = Vec::with_capacity(MAX_FILE + 1);
            // One extra byte rejects growth/oversize while bounding all read allocation.
            (&mut *file).take((MAX_FILE + 1) as u64).read_to_end(&mut bytes)?;
            let after = file.metadata()?;
            if bytes.len() > MAX_FILE || bytes.len() as u64 != before.len()
                || !after.is_file() || after.file_attributes() != before.file_attributes()
                || after.len() != before.len()
                || after.creation_time() != before.creation_time()
                || after.last_write_time() != before.last_write_time() {
                return Err(KeyError::Format);
            }
            Ok(bytes)
            })();
            // Observe typed IO/format failure before fail() converts published uncertainty.
            let bytes = observe_result(KeyDiagnosticStage::ReloadRead, reread)?;
            // The callback keeps its original container/provider/material observations.
            // Do not relabel its primary failure as a second PublishedSync failure.
            validate(&bytes)
        })();
        let value = match verified {
            Ok(value) => value,
            // fail keeps a published target and poisons admission, even after known close.
            Err(error) => return Err(self.fail(error)),
        };
        let file_closed = self.file.take().is_some_and(close_created);
        let parent_closed = self.parent.take().is_some_and(close_created);
        self.finished = true;
        // Do not let a validated value escape if either original close is uncertain.
        if file_closed && parent_closed {
            Ok(value)
        } else {
            observe_result(KeyDiagnosticStage::PublishedSync, Err(poison()))
        }
    }
}
impl Drop for Publication {
    fn drop(&mut self) {
        if self.finished { return; }
        // An abandoned published value never deletes its target or claims success.
        let mut known = !self.published;
        if let Some(file) = self.file.take() {
            known &= if self.published { close_created(file) } else { cleanup(file) };
        }
        if let Some(parent) = self.parent.take() { known &= close_created(parent); }
        if !known { let _ = poison(); }
        self.finished = true;
    }
}
