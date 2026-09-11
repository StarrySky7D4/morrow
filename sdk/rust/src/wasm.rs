//! Experimental wasm32 guest import adapter; no WASI or host identity parameters.
use crate::HostV1;
use std::ffi::c_void;
#[link(wasm_import_module = "morrow_v1")]
unsafe extern "C" {
    fn exchange(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
}
unsafe extern "C" fn adapter(
    _: *mut c_void,
    input: *const u8,
    length: u32,
    output: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> u32 {
    // SAFETY: Client supplies live disjoint bounded buffers. Import is synchronous
    // and the selected backend validates guest offsets before reading or submitting.
    let result = unsafe { exchange(input, length, output, capacity) };
    if result <= 0 {
        return 1;
    }
    unsafe {
        *written = result as u32;
    }
    0
}
/// Host descriptor for the fixed import, with no caller-selected context/identity.
pub fn host() -> HostV1 {
    HostV1 {
        abi_version: 1,
        struct_size: size_of::<HostV1>() as u32,
        context: std::ptr::null_mut(),
        exchange: Some(adapter),
    }
}
