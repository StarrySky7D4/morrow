//! Host-private frozen mappings. No raw address, OS handle, or writable API is exported.
//! Wasm consumers still require an explicit copy into guest linear memory.
use std::io;
pub const MAX_REGION_BYTES: usize = 16 * 1024 * 1024;
fn check_length(length: usize) -> io::Result<()> {
    if !(1..=MAX_REGION_BYTES).contains(&length) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "shared region must contain 1..=16 MiB",
        ));
    }
    Ok(())
}
#[cfg(windows)]
pub use windows::FrozenRegion;
#[cfg(not(windows))]
#[derive(Debug)]
pub enum FrozenRegion {}
#[cfg(not(windows))]
impl FrozenRegion {
    pub fn copy_from(bytes: &[u8]) -> io::Result<Self> {
        check_length(bytes.len())?;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "frozen mappings require the Windows backend",
        ))
    }
    pub fn bytes(&self) -> &[u8] {
        match *self {}
    }
    pub fn len(&self) -> usize {
        match *self {}
    }
    pub fn is_empty(&self) -> bool {
        match *self {}
    }
}
#[cfg(windows)]
mod windows {
    use super::{check_length, io};
    use std::{
        ffi::c_void,
        fmt,
        ptr::{self, NonNull},
    };
    type Handle = *mut c_void;
    const PAGE_READWRITE: u32 = 0x04;
    const FILE_MAP_WRITE: u32 = 0x02;
    const FILE_MAP_READ: u32 = 0x04;
    // SDK 10.0.26100.0 memoryapi.h and handleapi.h. Null SECURITY_ATTRIBUTES and
    // null name create a non-inheritable, anonymous paging-file mapping.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileMappingW(
            file: Handle,
            attributes: *const c_void,
            protect: u32,
            size_high: u32,
            size_low: u32,
            name: *const u16,
        ) -> Handle;
        fn MapViewOfFile(
            mapping: Handle,
            access: u32,
            offset_high: u32,
            offset_low: u32,
            bytes: usize,
        ) -> *mut c_void;
        fn UnmapViewOfFile(address: *const c_void) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    struct Mapping(NonNull<c_void>);
    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: owned valid mapping handle, never duplicated or exported; close once.
            unsafe {
                CloseHandle(self.0.as_ptr());
            }
        }
    }
    struct View(Option<NonNull<c_void>>);
    impl View {
        fn map(mapping: &Mapping, access: u32, len: usize) -> io::Result<Self> {
            // SAFETY: valid live handle, zero aligned offset and bounded nonzero extent.
            let address = unsafe { MapViewOfFile(mapping.0.as_ptr(), access, 0, 0, len) };
            let address = NonNull::new(address).ok_or_else(io::Error::last_os_error)?;
            Ok(Self(Some(address)))
        }
        fn pointer(&self) -> *mut u8 {
            self.0.expect("live view").as_ptr().cast()
        }
        fn unmap(&mut self) -> io::Result<()> {
            if let Some(address) = self.0 {
                // SAFETY: this is the original base returned by MapViewOfFile. No exposed
                // borrow exists at explicit unmap; Drop only runs after all region borrows end.
                if unsafe { UnmapViewOfFile(address.as_ptr()) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                self.0 = None;
            }
            Ok(())
        }
    }
    impl Drop for View {
        fn drop(&mut self) {
            let _ = self.unmap();
        }
    }
    /// No Clone: share ownership with Arc<FrozenRegion>. Only the last owner unmaps.
    /// Field order ensures the view drops before the mapping handle.
    pub struct FrozenRegion {
        view: View,
        _mapping: Mapping,
        len: usize,
    }
    impl fmt::Debug for FrozenRegion {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("FrozenRegion")
                .field("len", &self.len)
                .finish_non_exhaustive()
        }
    }
    // SAFETY: construction removes the sole writable view before publication. The only
    // retained view is FILE_MAP_READ, handle is private/non-inheritable, and no API creates
    // another view or mutates bytes. Moving ownership cannot invalidate the virtual address.
    unsafe impl Send for FrozenRegion {}
    // SAFETY: shared access is immutable; all readers borrow Self or own an Arc. Drop cannot
    // run while such a borrow/lease remains. The OS mapping outlives every returned slice.
    unsafe impl Sync for FrozenRegion {}
    impl FrozenRegion {
        pub fn copy_from(bytes: &[u8]) -> io::Result<Self> {
            check_length(bytes.len())?;
            // SAFETY: INVALID_HANDLE_VALUE selects paging-file storage. All optional pointers
            // are null, length fits u32 and is nonzero, protection excludes executable access.
            let handle = unsafe {
                CreateFileMappingW(
                    (-1isize) as Handle,
                    ptr::null(),
                    PAGE_READWRITE,
                    0,
                    bytes.len() as u32,
                    ptr::null(),
                )
            };
            let mapping = Mapping(NonNull::new(handle).ok_or_else(io::Error::last_os_error)?);
            let mut writable = View::map(&mapping, FILE_MAP_WRITE, bytes.len())?;
            // SAFETY: fresh private writable mapping has at least len bytes. Source is a
            // valid slice, cannot overlap this new mapping, and remains borrowed during copy.
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), writable.pointer(), bytes.len());
            }
            // Publication barrier: never create or return a readable region if removing the
            // sole writable view fails. Error cleanup drops view before closing mapping.
            writable.unmap()?;
            let view = View::map(&mapping, FILE_MAP_READ, bytes.len())?;
            Ok(Self {
                view,
                _mapping: mapping,
                len: bytes.len(),
            })
        }
        pub fn bytes(&self) -> &[u8] {
            // SAFETY: live read-only view contains exactly the initialized logical extent;
            // allocation/page padding is excluded, and the borrow cannot outlive this owner.
            unsafe { std::slice::from_raw_parts(self.view.pointer().cast_const(), self.len) }
        }
        pub fn len(&self) -> usize {
            self.len
        }
        pub fn is_empty(&self) -> bool {
            false
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
            sync::Arc,
            time::{Duration, Instant},
        };
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetErrorMode(mode: u32) -> u32;
        }
        #[test]
        fn producer_changes_do_not_change_frozen_mapping() {
            let mut original = vec![0x5a; 8193];
            original[8192] = 7;
            let frozen = FrozenRegion::copy_from(&original).unwrap();
            original.fill(0xff);
            drop(original);
            assert_eq!(frozen.len(), 8193);
            assert!(!frozen.is_empty());
            assert!(frozen.bytes()[..8192].iter().all(|v| *v == 0x5a));
            assert_eq!(frozen.bytes()[8192], 7);
        }
        #[test]
        fn arc_leases_can_outlive_producer_and_move_across_threads() {
            let region = Arc::new(FrozenRegion::copy_from(b"shared immutable bytes").unwrap());
            let weak = Arc::downgrade(&region);
            let readers = (0..8)
                .map(|_| {
                    let reader = Arc::clone(&region);
                    std::thread::spawn(move || {
                        assert_eq!(reader.bytes(), b"shared immutable bytes");
                        reader
                    })
                })
                .collect::<Vec<_>>();
            drop(region);
            let leases = readers
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(weak.strong_count(), 8);
            assert_eq!(leases[0].bytes(), b"shared immutable bytes");
            drop(leases);
            assert!(weak.upgrade().is_none());
        }
        #[test]
        fn size_boundaries_are_checked_before_allocation() {
            assert_eq!(
                FrozenRegion::copy_from(&[]).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(
                FrozenRegion::copy_from(&vec![0; super::super::MAX_REGION_BYTES + 1])
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
            for len in [1, 4095, 4096, 4097, super::super::MAX_REGION_BYTES] {
                let original = vec![0x3c; len];
                let region = FrozenRegion::copy_from(&original).unwrap();
                assert_eq!(region.bytes(), original);
                assert_eq!(region.len(), len);
            }
        }
        #[test]
        fn readonly_child() {
            let Some(marker) = std::env::var_os("MORROW_FROZEN_WRITE_PROBE") else {
                return;
            };
            // SAFETY: only this isolated qualification child changes its own error mode.
            unsafe {
                SetErrorMode(0x8003);
            }
            let region = FrozenRegion::copy_from(b"read only").unwrap();
            std::fs::write(marker, b"mapped-readonly").unwrap();
            // Deliberate fault probe in a throwaway hidden subprocess, NOT a safe API usage:
            // force a machine write to the read-only mapping and require OS access violation.
            // This must never return; it cannot contaminate the main test process.
            unsafe {
                ptr::write_volatile(region.bytes().as_ptr().cast_mut(), 0x42);
            }
            std::process::exit(42);
        }
        #[test]
        fn readonly_mapping_rejects_actual_write_without_a_dialog() {
            let dir = tempfile::tempdir().unwrap();
            let marker = dir.path().join("ready");
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "shared_memory::windows::tests::readonly_child",
                    "--nocapture",
                ])
                .env("MORROW_FROZEN_WRITE_PROBE", &marker)
                .creation_flags(0x08000000)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("read-only fault child timed out")
                }
                std::thread::sleep(Duration::from_millis(5));
            };
            assert_eq!(std::fs::read(marker).unwrap(), b"mapped-readonly");
            assert_eq!(status.code(), Some(0xc0000005u32 as i32));
        }
    }
}
#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    #[test]
    fn non_windows_is_explicitly_unsupported() {
        assert_eq!(
            FrozenRegion::copy_from(b"x").unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
    }
}
