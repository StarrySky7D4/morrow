//! One relative asynchronous open. Any indeterminate return retains every
//! original kernel pointer; the caller must also keep the original parent chain.
use super::super::selection_path::validate_component;
use std::{
    cell::UnsafeCell,
    ffi::c_void,
    fs::File,
    marker::PhantomData,
    os::windows::io::{AsRawHandle, FromRawHandle},
    ptr,
    rc::Rc,
};

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}
#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root: *mut c_void,
    name: *mut UnicodeString,
    attributes: u32,
    security: *mut c_void,
    qos: *mut c_void,
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
        allocation: *const i64,
        file_attributes: u32,
        share: u32,
        disposition: u32,
        options: u32,
        ea: *const c_void,
        ea_length: u32,
    ) -> i32;
}
struct Storage {
    output: UnsafeCell<*mut c_void>,
    status: UnsafeCell<IoStatusBlock>,
    name: UnicodeString,
    attributes: ObjectAttributes,
    _units: Vec<u16>,
}
/// The original allocation cannot cross threads or free pending kernel storage.
pub(super) struct UnknownOpen {
    storage: Option<Box<Storage>>,
    _thread: PhantomData<Rc<()>>,
}
impl Drop for UnknownOpen {
    fn drop(&mut self) {
        if let Some(storage) = self.storage.take() {
            let _ = Box::leak(storage);
        }
    }
}
pub(super) enum Opened {
    File(File),
    Refused(i32),
    Unknown(i32, UnknownOpen),
}
#[cfg(test)]
std::thread_local! {
    static PROTECT_NEXT_ORIGINAL_LEAF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
pub(super) fn protect_next_original_leaf_for_close_fixture() {
    assert!(!PROTECT_NEXT_ORIGINAL_LEAF.replace(true));
}
#[cfg(test)]
fn apply_close_refusal_fixture(file: File) -> File {
    if PROTECT_NEXT_ORIGINAL_LEAF.replace(false) {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetHandleInformation(handle: *mut c_void, mask: u32, flags: u32) -> i32;
            fn GetHandleInformation(handle: *mut c_void, flags: *mut u32) -> i32;
        }
        const HANDLE_FLAG_PROTECT_FROM_CLOSE: u32 = 2;
        let raw = file.as_raw_handle();
        let mut flags = 0;
        // The unique File just returned on this same thread. No raw handle is
        // saved, duplicated, reconstructed or sent to another thread.
        let set = unsafe {
            SetHandleInformation(
                raw,
                HANDLE_FLAG_PROTECT_FROM_CLOSE,
                HANDLE_FLAG_PROTECT_FROM_CLOSE,
            )
        };
        let queried = unsafe { GetHandleInformation(raw, &mut flags) };
        if set == 0 || queried == 0 || flags & HANDLE_FLAG_PROTECT_FROM_CLOSE == 0 {
            std::mem::forget(file);
            panic!("original close-refusal fixture was not established");
        }
    }
    file
}
pub(super) fn open(parent: &File, component: &[u16]) -> Result<Opened, super::super::Error> {
    validate_component(component).map_err(|_| super::super::Error::InvalidNativeRecord)?;
    let mut units = Vec::new();
    units
        .try_reserve_exact(component.len())
        .map_err(|_| super::super::Error::Allocation)?;
    units.extend_from_slice(component);
    let bytes = (units.len() * 2) as u16;
    let mut storage = Box::new(Storage {
        output: UnsafeCell::new(ptr::null_mut()),
        status: UnsafeCell::new(IoStatusBlock {
            status: IoStatus {
                pointer: ptr::null_mut(),
            },
            information: 0,
        }),
        name: UnicodeString {
            length: bytes,
            maximum_length: bytes,
            buffer: units.as_mut_ptr(),
        },
        attributes: ObjectAttributes {
            length: size_of::<ObjectAttributes>() as u32,
            root: parent.as_raw_handle(),
            name: ptr::null_mut(),
            attributes: 0x1040,
            security: ptr::null_mut(),
            qos: ptr::null_mut(),
        },
        _units: units,
    });
    storage.attributes.name = ptr::from_mut(&mut storage.name);
    // SAFETY: every argument/output has stable original boxed storage. The
    // original parent stays owned by the caller. OPEN never creates/truncates;
    // DIRECTORY|OPEN_REPARSE_POINT, OBJ_DONT_REPARSE and one validated component
    // forbid traversal through a reparse target. No SYNCHRONOUS flag is present.
    // Share READ|WRITE permits child notifications but denies delete/rename of
    // this captured directory while its original handle is held.
    let status = unsafe {
        NtCreateFile(
            storage.output.get(),
            0x0010_00a1,
            ptr::from_mut(&mut storage.attributes),
            storage.status.get(),
            ptr::null(),
            0,
            3,
            1,
            0x0020_0001,
            ptr::null(),
            0,
        )
    };
    if status == 0 {
        // A definitive successful return makes the original output readable.
        let raw = unsafe { *storage.output.get() };
        if !raw.is_null() && raw as isize != -1 {
            // SAFETY: this is the unique newly returned handle. No integer
            // reconstruction, duplication or reopen occurs. The caller must
            // immediately transfer this File into its original NativeWatch.
            let file = unsafe { File::from_raw_handle(raw) };
            #[cfg(test)]
            let file = apply_close_refusal_fixture(file);
            return Ok(Opened::File(file));
        }
    } else if status < 0 {
        // No pending IO on a definitive failure. An unexpected nonzero output
        // is nevertheless ambiguous and must not be guessed into a File.
        if unsafe { (*storage.output.get()).is_null() } {
            return Ok(Opened::Refused(status));
        }
    }
    // Never read an output for STATUS_PENDING or another alternate success;
    // preserve the original storage and parent owners without another query.
    Ok(Opened::Unknown(
        status,
        UnknownOpen {
            storage: Some(storage),
            _thread: PhantomData,
        },
    ))
}
