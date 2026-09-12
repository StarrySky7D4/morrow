//! Frozen mappings with trusted read-only delivery to an owned child process.
//! No raw address, source handle, writable view, or arbitrary PID interface is exported.
//! Wasm consumers still require an explicit copy into guest linear memory.
use std::io;
pub const MAX_REGION_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_TRANSFER_BYTES: usize = 64 * 1024;
fn check_transfer_length(length: usize) -> io::Result<()> {
    if !(1..=MAX_TRANSFER_BYTES).contains(&length) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "mapping transfer must contain 1..=64 KiB",
        ));
    }
    Ok(())
}
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
pub use windows::{FrozenRegion, copy_received_readonly};
#[cfg(not(windows))]
pub fn copy_received_readonly(_raw: u64, length: usize) -> io::Result<(Vec<u8>, bool)> {
    check_transfer_length(length)?;
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "mapping transfer requires Windows",
    ))
}
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
    pub fn duplicate_readonly(&self, _child: &std::process::Child) -> io::Result<u64> {
        match *self {}
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
    use super::{check_length, check_transfer_length, io};
    use std::{
        ffi::c_void,
        fmt,
        os::windows::io::AsRawHandle,
        process::Child,
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
        fn GetCurrentProcess() -> Handle;
        fn DuplicateHandle(
            source_process: Handle,
            source_handle: Handle,
            target_process: Handle,
            target_handle: *mut Handle,
            desired_access: u32,
            inherit: i32,
            options: u32,
        ) -> i32;
        fn ReadProcessMemory(
            process: Handle,
            source: *const c_void,
            destination: *mut c_void,
            length: usize,
            copied: *mut usize,
        ) -> i32;
    }
    struct Mapping(NonNull<c_void>);
    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: this locally owned handle is closed exactly once. Duplication creates
            // separate handle entries; a foreign handle value is never stored here.
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
    // a writable view or mutates bytes. Delivery exports only SECTION_MAP_READ to an owned
    // child. Moving ownership cannot invalidate the virtual address.
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
        /// Delivers SECTION_MAP_READ only, non-inheritable, to this actual child handle.
        /// The returned number is valid ONLY in the child. Do not close it locally or try
        /// remotely closing it by number: after delivery it may have been closed/reused.
        /// Caller must retain the region until real child termination even if Offer fails.
        pub fn duplicate_readonly(&self, child: &Child) -> io::Result<u64> {
            check_transfer_length(self.len)?;
            // SAFETY: Child owns a valid process handle for the duration of this borrow.
            // This does not reopen by PID or manufacture arbitrary process authority.
            let target = duplicate_read(self._mapping.0.as_ptr(), child.as_raw_handle())?;
            // Do not wrap a target-process handle in local RAII or call CloseHandle on it.
            Ok(target as usize as u64)
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
    // A successful remote duplicate belongs to target_process, NOT this function.
    fn duplicate_read(source: Handle, target_process: Handle) -> io::Result<Handle> {
        let mut duplicate = ptr::null_mut();
        // SAFETY: out pointer is writable local storage, source is only an opaque handle
        // looked up by the kernel in this process. options=0 neither closes the source nor
        // requests SAME_ACCESS. target_process is a borrowed Child or current pseudo handle.
        let succeeded = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                source,
                target_process,
                &mut duplicate,
                FILE_MAP_READ,
                0,
                0,
            )
        };
        if succeeded == 0 {
            return Err(io::Error::last_os_error());
        }
        if duplicate.is_null() {
            return Err(io::Error::other("null duplicated handle"));
        }
        Ok(duplicate)
    }
    /// Borrow an incoming current-process handle; never consume or close `raw`.
    /// Success bool=true means the original handle's WRITE mapping was ACCESS_DENIED.
    /// Other denial reasons or a writable original are errors, not successful validation.
    /// The returned Vec is a detached copy, not a borrowed view of untrusted shared bytes.
    pub fn copy_received_readonly(raw: u64, length: usize) -> io::Result<(Vec<u8>, bool)> {
        check_transfer_length(length)?;
        let raw = usize::try_from(raw).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "handle does not fit this process",
            )
        })?;
        if raw == 0 || raw == usize::MAX || raw == usize::MAX - 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid mapping handle",
            ));
        }
        let original = raw as Handle;
        // SAFETY: GetCurrentProcess has no parameters and returns a non-owned pseudo handle.
        let current = unsafe { GetCurrentProcess() };
        let duplicate = duplicate_read(original, current)?;
        let mapping = Mapping(NonNull::new(duplicate).expect("checked duplicate"));
        // SAFETY: the kernel validates the opaque original handle and requested extent.
        // No Rust slice is created for this view, and no write is performed by the probe.
        let writable = unsafe { MapViewOfFile(original, FILE_MAP_WRITE, 0, 0, length) };
        if let Some(pointer) = NonNull::new(writable) {
            let _view = View(Some(pointer));
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "received mapping permits write access",
            ));
        }
        let denied = io::Error::last_os_error();
        if denied.raw_os_error() != Some(5) {
            return Err(denied);
        }
        let view = View::map(&mapping, FILE_MAP_READ, length)?;
        let mut output = vec![0u8; length];
        let mut copied = 0usize;
        // SAFETY: only the destination is Rust-owned mutable memory. ReadProcessMemory
        // validates/reads the possibly mutable foreign view through the OS; no &[u8] is
        // constructed over it, avoiding a shared-reference immutability promise. Current
        // process is fixed; the API never grants arbitrary process-memory reading.
        let succeeded = unsafe {
            ReadProcessMemory(
                current,
                view.pointer().cast_const().cast(),
                output.as_mut_ptr().cast(),
                length,
                &mut copied,
            )
        };
        if succeeded == 0 {
            return Err(io::Error::last_os_error());
        }
        if copied != length {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete mapping copy",
            ));
        }
        Ok((output, true))
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
            fn GetHandleInformation(handle: Handle, flags: *mut u32) -> i32;
        }
        struct TestChild(Child);
        impl Drop for TestChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        fn receive_child_process(marker: &std::path::Path) -> TestChild {
            TestChild(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "shared_memory::windows::tests::received_readonly_child",
                        "--nocapture",
                    ])
                    .env("MORROW_MAPPING_RECEIVER", marker)
                    .creation_flags(0x08000000)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            )
        }
        fn wait_child(child: &mut Child) -> std::process::ExitStatus {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    return status;
                }
                assert!(Instant::now() < deadline, "mapping child timeout");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        #[test]
        fn received_readonly_child() {
            use std::io::Read;
            let Some(marker) = std::env::var_os("MORROW_MAPPING_RECEIVER") else {
                return;
            };
            let mut input = String::new();
            std::io::stdin()
                .take(128)
                .read_to_string(&mut input)
                .unwrap();
            let mut fields = input.split_whitespace();
            let raw = fields.next().unwrap().parse::<u64>().unwrap();
            let len = fields.next().unwrap().parse::<usize>().unwrap();
            assert!(fields.next().is_none());
            let mut flags = 0u32;
            // SAFETY: OS checks the received opaque handle; flags is writable local memory.
            assert_ne!(
                unsafe { GetHandleInformation(raw as usize as Handle, &mut flags) },
                0
            );
            assert_eq!(flags & 1, 0, "target handle must not be inheritable");
            let (first, readonly) = copy_received_readonly(raw, len).unwrap();
            assert!(readonly);
            let (second, readonly) = copy_received_readonly(raw, len).unwrap();
            assert!(readonly);
            assert_eq!(first, second, "helper must not consume incoming handle");
            assert_eq!(first, (0..len).map(|i| (i % 251) as u8).collect::<Vec<_>>());
            std::fs::write(marker, first).unwrap();
            // The borrowed raw is intentionally left for actual process termination.
        }
        #[test]
        fn actual_child_receives_readonly_noninheritable_mapping_and_reuses_input_handle() {
            use std::io::Write;
            for length in [1, 4097, super::super::MAX_TRANSFER_BYTES] {
                let dir = tempfile::tempdir().unwrap();
                let marker = dir.path().join("received");
                let mut source = (0..length).map(|i| (i % 251) as u8).collect::<Vec<_>>();
                let region = FrozenRegion::copy_from(&source).unwrap();
                let mut child = receive_child_process(&marker);
                let raw = region.duplicate_readonly(&child.0).unwrap();
                source.fill(0xff);
                let mut input = child.0.stdin.take().unwrap();
                writeln!(input, "{raw} {length}").unwrap();
                drop(input);
                assert!(wait_child(&mut child.0).success());
                assert_eq!(std::fs::read(marker).unwrap(), region.bytes());
                // Region ownership deliberately remains alive until after real child exit.
            }
        }
        #[test]
        fn writable_original_and_invalid_handles_are_rejected_without_consuming_source() {
            let region = FrozenRegion::copy_from(b"read-only source view").unwrap();
            let raw = region._mapping.0.as_ptr() as usize as u64;
            assert_eq!(
                copy_received_readonly(raw, region.len())
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::PermissionDenied
            );
            assert_eq!(region.bytes(), b"read-only source view");
            // Original mapping remains usable; the helper did not close the caller's handle.
            let view = View::map(&region._mapping, FILE_MAP_READ, region.len()).unwrap();
            drop(view);
            for raw in [0, u64::MAX, u64::MAX - 1] {
                assert_eq!(
                    copy_received_readonly(raw, 1).unwrap_err().kind(),
                    io::ErrorKind::InvalidInput
                );
            }
            for length in [0, super::super::MAX_TRANSFER_BYTES + 1] {
                assert_eq!(
                    copy_received_readonly(raw, length).unwrap_err().kind(),
                    io::ErrorKind::InvalidInput
                );
            }
        }
        #[test]
        fn transfer_rejects_oversize_and_exited_actual_child() {
            let dir = tempfile::tempdir().unwrap();
            let mut child = receive_child_process(&dir.path().join("unused"));
            let large =
                FrozenRegion::copy_from(&vec![0; super::super::MAX_TRANSFER_BYTES + 1]).unwrap();
            assert_eq!(
                large.duplicate_readonly(&child.0).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
            child.0.kill().unwrap();
            child.0.wait().unwrap();
            let small = FrozenRegion::copy_from(b"x").unwrap();
            assert!(small.duplicate_readonly(&child.0).is_err());
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
