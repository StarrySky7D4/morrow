//! Narrow native boundary: retained directory opens, create/publish, delete and owned close.
//! No guest pointers, numeric guest handles, or handle duplication.
use std::{
    ffi::c_void,
    fs::File,
    os::windows::io::{AsRawHandle, IntoRawHandle},
    sync::{
        Mutex, MutexGuard, TryLockError,
        atomic::{AtomicBool, Ordering},
    },
};
static CLOSE_UNCERTAIN: AtomicBool = AtomicBool::new(false);
static EFFECT_GATE: Mutex<()> = Mutex::new(());
pub(super) enum GateError {
    Busy,
    RestartRequired,
}
pub(super) struct EffectGate {
    _lock: MutexGuard<'static, ()>,
}
/// Reserve before claiming dispatch; a contended gate never creates Unknown.
/// try_lock avoids waiting/reentrant-clock deadlocks. Keep the guard through the
/// final authorization, marking and close so poison cannot race a later effect.
pub(super) fn enter() -> Result<EffectGate, GateError> {
    if !available() {
        return Err(GateError::RestartRequired);
    }
    let lock = match EFFECT_GATE.try_lock() {
        Ok(lock) => lock,
        Err(TryLockError::WouldBlock) => return Err(GateError::Busy),
        Err(TryLockError::Poisoned(_)) => {
            CLOSE_UNCERTAIN.store(true, Ordering::Release);
            return Err(GateError::RestartRequired);
        }
    };
    if !available() {
        return Err(GateError::RestartRequired);
    }
    Ok(EffectGate { _lock: lock })
}
impl EffectGate {
    pub(super) fn delete(self, file: File) -> DeleteAttempt {
        delete_locked(file)
    }
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
pub(super) enum DeleteAttempt {
    Deleted,
    /// OS rejected marking. This does not authorize automatic replay.
    Rejected(u32),
    /// Close failed after marking. Handle validity is unknown; do not retry it.
    CloseUnknown(u32),
}
pub(super) fn available() -> bool {
    !CLOSE_UNCERTAIN.load(Ordering::Acquire)
}
fn last_error() -> u32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
}
/// Caller commits Unknown and rechecks live authority before this function.
/// After a CloseHandle failure, no code may retry or wrap the numeric handle:
/// Windows does not promise that it is still valid. Admission is poisoned until
/// process restart and the caller must retain the resource charge. A possibly
/// still-open kernel handle is intentionally left for process teardown.
fn delete_locked(file: File) -> DeleteAttempt {
    let mut info = FileDispositionInfo { delete_file: 1 };
    // SAFETY: File owns a live handle throughout this synchronous call; info is
    // initialized repr(C) FILE_DISPOSITION_INFO (one-byte BOOLEAN), correctly
    // sized/aligned and borrowed until return. FileDispositionInfo=4.
    let marked = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            4,
            (&mut info as *mut FileDispositionInfo).cast(),
            size_of::<FileDispositionInfo>() as u32,
        )
    };
    if marked == 0 {
        return DeleteAttempt::Rejected(last_error());
    }
    let raw = file.into_raw_handle();
    // SAFETY: into_raw_handle transferred sole ownership; close exactly once.
    // Neither branch reconstructs File, preventing a second close/reused-handle bug.
    if unsafe { CloseHandle(raw) } != 0 {
        DeleteAttempt::Deleted
    } else {
        let code = last_error();
        CLOSE_UNCERTAIN.store(true, Ordering::Release);
        DeleteAttempt::CloseUnknown(code)
    }
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
/// Opens one existing directory component. Never resolves a multi-part guest
/// string or creates an object. Caller validates the same returned handle's
/// metadata before using it as the next parent, and retains every ancestor.
pub(super) fn open_child_directory(parent: &File, segment: &str) -> std::io::Result<File> {
    use std::os::windows::io::FromRawHandle;
    use std::ptr::null_mut;
    if segment.contains('/') || morrow_core::file_path::RelativeFilePath::parse(segment).is_err() {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    let mut encoded: Vec<u16> = segment.encode_utf16().collect();
    let bytes = (encoded.len() * 2) as u16; // lexical validation caps at 255 units
    let mut name = UnicodeString {
        length: bytes,
        maximum_length: bytes,
        buffer: encoded.as_mut_ptr(),
    };
    let mut attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: parent.as_raw_handle(),
        object_name: &mut name,
        attributes: 0x40, // OBJ_CASE_INSENSITIVE, no inherited handle
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
    // SAFETY: all pointers refer to initialized, correctly aligned repr(C)
    // structures and live UTF-16 storage for this synchronous call. The parent
    // File remains borrowed. FILE_OPEN (1) never creates/truncates; synchronous
    // NONALERT (0x20) plus SYNCHRONIZE ensures no pending use of stack storage.
    // DIRECTORY (1) and OPEN_REPARSE_POINT (0x200000) require a directory handle
    // without following a final reparse target. Share READ only pins the object.
    let result = unsafe {
        NtCreateFile(
            &mut raw,
            0x0010_00a0,
            &mut attributes,
            &mut status,
            std::ptr::null(),
            0,
            1,
            1,
            0x0020_0021,
            std::ptr::null(),
            0,
        )
    };
    if result < 0 {
        // SAFETY: pure status translation, no borrowed pointers or ownership.
        let code = unsafe { RtlNtStatusToDosError(result) };
        return Err(std::io::Error::from_raw_os_error(code as i32));
    }
    if raw.is_null() || raw as isize == -1 {
        return Err(std::io::Error::other("invalid native directory handle"));
    }
    // SAFETY: successful synchronous NtCreateFile returned a new owned handle.
    // It is wrapped once; neither this function nor the caller closes it manually.
    Ok(unsafe { File::from_raw_handle(raw) })
}

/// Trusted-host single raw UTF-16 component, relative to the retained parent.
/// This readonly open is no grant and never creates or follows a reparse point.
/// The caller retains the root/all ancestors and validates this same returned
/// handle's directory attributes and 24-byte volume/file identity before use.
pub(super) fn open_relative_directory(
    parent: &File,
    component: &[u16],
    listing: bool,
) -> std::io::Result<File> {
    use std::os::windows::io::FromRawHandle;
    use std::ptr::null_mut;
    crate::directory_io::selection_path::validate_component(component)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let mut encoded = Vec::new();
    encoded.try_reserve_exact(component.len())
        .map_err(|_| std::io::Error::other("directory component allocation failed"))?;
    encoded.extend_from_slice(component);
    let bytes = u16::try_from(encoded.len().checked_mul(2)
        .ok_or(std::io::ErrorKind::InvalidInput)?)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let mut name = UnicodeString {
        length: bytes,
        maximum_length: bytes,
        buffer: encoded.as_mut_ptr(),
    };
    let mut attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: parent.as_raw_handle(),
        object_name: &mut name,
        attributes: 0x1040, // OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE
        security_descriptor: null_mut(),
        security_quality_of_service: null_mut(),
    };
    let mut status = IoStatusBlock {
        status: IoStatus { pointer: null_mut() },
        information: 0,
    };
    let mut raw = null_mut();
    // SAFETY: one validated <=255-unit UTF-16 component is live and writable
    // through this synchronous call; all initialized repr(C) native structures
    // have their ABI alignment. The borrowed parent File retains its handle.
    // FILE_OPEN=1 never creates/truncates. DIRECTORY|SYNCHRONOUS_NONALERT|
    // OPEN_REPARSE_POINT=0x200021; SYNCHRONIZE prevents pending buffer use.
    // Fixed readonly rights: TRAVERSE|READ_ATTRIBUTES|SYNCHRONIZE, with LIST
    // only for the final listing directory. Share READ alone denies ordinary
    // WRITE/DELETE opens while this handle lives; no guest handle is exposed.
    let result = unsafe {
        NtCreateFile(
            &mut raw,
            if listing { 0x0010_00a1 } else { 0x0010_00a0 },
            &mut attributes,
            &mut status,
            std::ptr::null(),
            0,
            1,
            1,
            0x0020_0021,
            std::ptr::null(),
            0,
        )
    };
    if result < 0 {
        // SAFETY: pure status translation, no borrowed pointers or ownership.
        return Err(std::io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(result) } as i32,
        ));
    }
    if raw.is_null() || raw as isize == -1 {
        return Err(std::io::Error::other("invalid native directory handle"));
    }
    // SAFETY: synchronous successful NtCreateFile transferred one owned handle;
    // File wraps it once and performs the only close. No raw value is exposed.
    Ok(unsafe { File::from_raw_handle(raw) })
}

pub(super) enum CreateAttempt {
    Created,
    /// API failure, with the newly created temporary file confirmed cleaned up
    /// when one existed. This is historical evidence, never a retry permission.
    Rejected(u32),
    /// Dispatch crossed its boundary but its final effect is uncertain.
    Unknown {
        close_unknown: bool,
    },
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
fn single_component(value: &str) -> bool {
    !value.contains('/') && morrow_core::file_path::RelativeFilePath::parse(value).is_ok()
}
fn open_new(parent: &File, name: &str) -> std::io::Result<File> {
    use std::{os::windows::io::FromRawHandle, ptr::null_mut};
    if !single_component(name) {
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
        return Err(std::io::Error::other("invalid created file handle"));
    }
    // SAFETY: one new owned synchronous handle; wrap once and transfer with File.
    Ok(unsafe { File::from_raw_handle(raw) })
}
fn rename_same_directory(file: &File, leaf: &str) -> std::io::Result<()> {
    if !single_component(leaf) {
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
    // Source is synchronous, live, DELETE-capable; its entire parent chain is
    // retained without WRITE/DELETE sharing. Oversized buffer is zero padded.
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
        CLOSE_UNCERTAIN.store(true, Ordering::Release);
        false
    }
}
fn cleanup_created(file: File, error: Option<u32>) -> CreateAttempt {
    let mut info = FileDispositionInfo { delete_file: 1 };
    // SAFETY: same one-byte repr(C) disposition and borrowed File as delete_locked.
    // Only OUR newly created, still-exclusively-held temp file is eligible.
    let marked = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            4,
            (&mut info as *mut FileDispositionInfo).cast(),
            size_of::<FileDispositionInfo>() as u32,
        )
    };
    if !close_created(file) {
        return CreateAttempt::Unknown {
            close_unknown: true,
        };
    }
    if marked != 0 {
        if let Some(code) = error.filter(|code| *code != 0) {
            return CreateAttempt::Rejected(code);
        }
    }
    CreateAttempt::Unknown {
        close_unknown: false,
    }
}
fn error_code(error: &std::io::Error) -> Option<u32> {
    error
        .raw_os_error()
        .filter(|code| *code > 0)
        .map(|code| code as u32)
}
impl EffectGate {
    /// The caller already holds a confirmed durable claim, all ancestor handles
    /// and a resource lease for this temporary file. No temp or leaf is opened
    /// before that boundary. Errors/authority loss never permit automatic replay.
    pub(super) fn create(
        self,
        parent: &File,
        temporary: &str,
        leaf: &str,
        bytes: &[u8],
        mut live: impl FnMut() -> bool,
    ) -> CreateAttempt {
        use std::io::Write;
        if !live() {
            return CreateAttempt::Unknown {
                close_unknown: false,
            };
        }
        let mut file = match open_new(parent, temporary) {
            Ok(file) => file,
            Err(error) => {
                return match error_code(&error) {
                    Some(code) => CreateAttempt::Rejected(code),
                    None => CreateAttempt::Unknown {
                        close_unknown: false,
                    },
                };
            }
        };
        create_fault("after-temp");
        for chunk in bytes.chunks(64 * 1024) {
            if !live() {
                return cleanup_created(file, None);
            }
            if let Err(error) = file.write_all(chunk) {
                return cleanup_created(file, error_code(&error));
            }
            // Dedicated fault-injection builds can terminate with only a
            // prefix written. Normal builds compile this boundary to a no-op.
            create_fault("after-write-chunk");
        }
        create_fault("after-write");
        if !live() {
            return cleanup_created(file, None);
        }
        if let Err(error) = file.sync_all() {
            return cleanup_created(file, error_code(&error));
        }
        create_fault("after-flush");
        if !live() {
            return cleanup_created(file, None);
        }
        if let Err(error) = rename_same_directory(&file, leaf) {
            return cleanup_created(file, error_code(&error));
        }
        create_fault("after-publish");
        // Publication is an observed fact. Revocation after this point suppresses
        // delivery but must not delete the published file or hide its observation.
        if close_created(file) {
            CreateAttempt::Created
        } else {
            CreateAttempt::Unknown {
                close_unknown: true,
            }
        }
    }
}
fn create_fault(_point: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_FILE_CREATE_FAULT").ok().as_deref() == Some(_point) {
        std::process::exit(86);
    }
}

#[cfg(test)]
mod replacement_qualification;
