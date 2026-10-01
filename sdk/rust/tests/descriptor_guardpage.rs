//! A short native descriptor ends exactly at an unreadable Windows page.
#![cfg(windows)]
use std::ffi::c_void;
#[repr(C)]
struct SystemInfo {
    architecture: u16,
    reserved: u16,
    page_size: u32,
    min_address: *mut c_void,
    max_address: *mut c_void,
    processor_mask: usize,
    processors: u32,
    processor_type: u32,
    allocation_granularity: u32,
    level: u16,
    revision: u16,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemInfo(info: *mut SystemInfo);
    fn VirtualAlloc(address: *mut c_void, size: usize, kind: u32, protection: u32) -> *mut c_void;
    fn VirtualProtect(address: *mut c_void, size: usize, protection: u32, old: *mut u32) -> i32;
    fn VirtualFree(address: *mut c_void, size: usize, kind: u32) -> i32;
}
unsafe extern "C" {
    fn mp_channel_request_encode(
        raw: *const c_void,
        out: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> u32;
    fn mp_request_encode(raw: *const c_void, out: *mut u8, capacity: u32, length: *mut u32) -> u32;
    fn mp_content_request_encode(
        raw: *const c_void,
        out: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> u32;
    fn mp_dependency_request_encode(
        raw: *const c_void,
        out: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> u32;
    fn mp_io_request_encode(
        raw: *const c_void,
        out: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> u32;
    fn mp_mutation_request_encode(
        raw: *const c_void,
        out: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> u32;
    fn mp_service_request_decode(input: *const u8, length: u32, out: *mut *mut c_void) -> u32;
    fn mp_service_response_encode(
        handle: *const c_void,
        reply: *const c_void,
        out: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> u32;
    fn mp_service_request_free(handle: *mut c_void);
}
struct Pages {
    base: *mut c_void,
    prefix: *mut c_void,
}
impl Pages {
    fn new() -> Self {
        let mut info: SystemInfo = unsafe { std::mem::zeroed() };
        unsafe { GetSystemInfo(&mut info) };
        assert!(info.page_size >= 4096);
        let base = unsafe {
            VirtualAlloc(
                std::ptr::null_mut(),
                info.page_size as usize * 2,
                0x3000,
                0x04,
            )
        };
        assert!(!base.is_null());
        let guard = unsafe { base.cast::<u8>().add(info.page_size as usize) };
        let mut old = 0;
        assert_ne!(
            unsafe { VirtualProtect(guard.cast(), info.page_size as usize, 0x01, &mut old) },
            0
        );
        Self {
            base,
            prefix: unsafe { guard.sub(8) }.cast(),
        }
    }
    fn set(&self, version: u32, size: u32) {
        unsafe {
            self.prefix.cast::<u32>().write(version);
            self.prefix.cast::<u32>().add(1).write(size)
        }
    }
}
impl Drop for Pages {
    fn drop(&mut self) {
        assert_ne!(unsafe { VirtualFree(self.base, 0, 0x8000) }, 0)
    }
}
#[test]
fn all_sized_codec_prefixes_reject_before_windows_guard_page() {
    // Anchor the SDK Rust crate and create a real live service request handle.
    let invocation = morrow_plugin_sdk::service::Invocation {
        service: "guard-service".into(),
        handler: "guard-handler".into(),
        principal: "local".into(),
        method: "GET".into(),
        target: "/".into(),
        headers: vec![],
        body: vec![],
    };
    let request = morrow_plugin_sdk::service::Request::encode(1, &invocation).unwrap();
    let mut handle = std::ptr::null_mut();
    assert_eq!(
        unsafe {
            mp_service_request_decode(
                request.bytes().as_ptr(),
                request.bytes().len() as u32,
                &mut handle,
            )
        },
        0
    );
    let pages = Pages::new();
    let mut output = vec![0xa5; 131072];
    type Encode = unsafe extern "C" fn(*const c_void, *mut u8, u32, *mut u32) -> u32;
    let encoders: [(&str, Encode); 6] = [
        ("channel", mp_channel_request_encode),
        ("command", mp_request_encode),
        ("content", mp_content_request_encode),
        ("dependency", mp_dependency_request_encode),
        ("io", mp_io_request_encode),
        ("mutation", mp_mutation_request_encode),
    ];
    for (version, size) in [(1, 8), (0, u32::MAX), (1, 0)] {
        pages.set(version, size);
        for (name, encode) in encoders {
            let mut length = 99;
            assert_eq!(
                unsafe { encode(pages.prefix, output.as_mut_ptr(), 131072, &mut length) },
                18,
                "{name}"
            );
            assert_eq!(length, 0);
            assert!(output.iter().all(|b| *b == 0xa5));
        }
        let mut length = 99;
        assert_eq!(
            unsafe {
                mp_service_response_encode(
                    handle,
                    pages.prefix,
                    output.as_mut_ptr(),
                    131072,
                    &mut length,
                )
            },
            18
        );
        assert_eq!(length, 0);
    }
    unsafe { mp_service_request_free(handle) };
}
