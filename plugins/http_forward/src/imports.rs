//! The only unsafe boundary: fixed experimental wasm32 imports and one export.
//! No pointers, caller identities, credentials, or grants come from task data.
use crate::forward::{self, FRAME_BYTES, Transport};

#[link(wasm_import_module = "morrow_task_v1")]
unsafe extern "C" {
    fn read_input(output: *mut u8, capacity: u32) -> i32;
    fn complete(input: *const u8, length: u32) -> i32;
}

#[link(wasm_import_module = "morrow_io_v1")]
unsafe extern "C" {
    #[link_name = "call"]
    fn io_call(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
}

struct Host;
impl Transport for Host {
    fn read(&mut self, output: &mut [u8; FRAME_BYTES]) -> i32 {
        // SAFETY: this exclusive owned buffer spans the exact 128 KiB capacity.
        // The synchronous runtime import validates the full guest memory range.
        unsafe { read_input(output.as_mut_ptr(), FRAME_BYTES as u32) }
    }
    fn call(&mut self, input: &[u8], output: &mut [u8; FRAME_BYTES]) -> i32 {
        if input.is_empty() || input.len() > FRAME_BYTES {
            return -1;
        }
        // SAFETY: live borrowed slices describe separate request/response allocations.
        // Rust borrowing prevents aliasing. Both ranges fit u32 on wasm32 and remain
        // valid throughout the synchronous call; the runtime also validates them.
        unsafe {
            io_call(
                input.as_ptr(),
                input.len() as u32,
                output.as_mut_ptr(),
                FRAME_BYTES as u32,
            )
        }
    }
    fn complete(&mut self, response: &[u8]) -> i32 {
        if response.is_empty() || response.len() > FRAME_BYTES {
            return -1;
        }
        // SAFETY: the verified bounded response slice remains live until the runtime
        // copies it. No TaskCompletion wrapper or response reconstruction is added.
        unsafe { complete(response.as_ptr(), response.len() as u32) }
    }
}

// SAFETY: this crate defines the sole export with this fixed ABI name and signature.
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    forward::run(&mut Host)
}
