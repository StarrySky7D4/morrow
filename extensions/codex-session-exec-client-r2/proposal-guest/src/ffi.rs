//! Isolated fixed Wasm import using the established owned bounded buffer bridge.
use morrow_codex_session_exec_client_r2::{Transport, protocol::MAX_FRAME_BYTES};
#[link(wasm_import_module = "morrow_agent_session_process_v1")]
unsafe extern "C" {
    #[link_name = "call"]
    fn agent_call(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
}
pub struct Import;
impl Transport for Import {
    fn exchange_once(&mut self, input: &[u8]) -> Result<Vec<u8>, ()> {
        if input.is_empty() || input.len() > MAX_FRAME_BYTES {
            return Err(());
        }
        let mut output = Vec::new();
        output.try_reserve_exact(MAX_FRAME_BYTES).map_err(|_| ())?;
        output.resize(MAX_FRAME_BYTES, 0);
        // SAFETY: Owned disjoint buffers remain live over this synchronous fixed
        // import. The original Wasm host checks full ranges, fixed capacity and
        // task state before dispatch. No native identity or retained pointer crosses.
        let size = unsafe {
            agent_call(
                input.as_ptr(),
                input.len() as u32,
                output.as_mut_ptr(),
                output.len() as u32,
            )
        };
        if size <= 0 || size as usize > output.len() {
            return Err(());
        }
        output.truncate(size as usize);
        Ok(output)
    }
}
