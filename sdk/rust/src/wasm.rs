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

#[link(wasm_import_module = "morrow_task_v1")]
unsafe extern "C" {
    fn read_input(output: *mut u8, capacity: u32) -> i32;
    fn complete(input: *const u8, length: u32) -> i32;
}
/// Read once from the host-bound invocation. Requires guest ABI v2.
pub fn read_task() -> Result<crate::task::Invocation, crate::Error> {
    let mut bytes = vec![0; crate::task::MAX_TASK_BYTES];
    // SAFETY: the owned buffer spans the advertised capacity; the backend validates offsets.
    let size = unsafe { read_input(bytes.as_mut_ptr(), bytes.len() as u32) };
    if size <= 0 || size as usize > bytes.len() {
        return Err(crate::Error::TransportFailure);
    }
    crate::task::Invocation::decode(&bytes[..size as usize]).map_err(|_| crate::Error::BadReply)
}
/// Submit one correlated completion. The host independently checks actual response provenance.
pub fn complete_task(task: &crate::task::Invocation, response: &[u8]) -> Result<(), crate::Error> {
    let bytes = task
        .completion(response)
        .map_err(|_| crate::Error::BadReply)?;
    // SAFETY: completion bytes remain live for the synchronous import and contain no pointers.
    if unsafe { complete(bytes.as_ptr(), bytes.len() as u32) } != 0 {
        return Err(crate::Error::TransportFailure);
    }
    Ok(())
}
