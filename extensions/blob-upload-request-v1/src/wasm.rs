//! Fixed import for the independent, explicitly approved upload profile.
//! There is no fallback, retry, native owner handle or caller-selected transport.
use crate::{ActionRef, Client, Error, Response, Result};

#[link(wasm_import_module = "morrow_blob_upload_v1")]
unsafe extern "C" {
    fn call(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
}
#[link(wasm_import_module = "morrow_task_v1")]
unsafe extern "C" {
    fn complete(input: *const u8, length: u32) -> i32;
}

/// One admitted request; the client preserves uncertainty and cumulative charges.
pub fn call_once(client: &mut Client, nonce: [u8; 32], action: ActionRef<'_>) -> Result<Response> {
    client.call_once_with(nonce, action, |request, response| {
        // SAFETY: client-owned disjoint bounded buffers live throughout this call.
        unsafe {
            call(
                request.as_ptr(),
                request.len() as u32,
                response.as_mut_ptr(),
                response.len() as u32,
            )
        }
    })
}

/// Submit the last validated response, exactly as required by this profile.
/// The original worker independently checks equality and actual execution return.
pub fn complete_response(response: &Response) -> Result<()> {
    // SAFETY: the owned bounded wire buffer remains live through the synchronous import.
    if unsafe { complete(response.wire().as_ptr(), response.wire().len() as u32) } == 0 {
        Ok(())
    } else {
        Err(Error::State)
    }
}
