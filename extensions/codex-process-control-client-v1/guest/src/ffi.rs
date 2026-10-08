//! The only unsafe bridge: owned synchronous fixed buffers, matching the existing SDK.
use morrow_codex_process_control_client_v1::{Transport, protocol::MAX_FRAME_BYTES};
#[link(wasm_import_module = "morrow_agent_session_process_v1")]
unsafe extern "C" {
    #[link_name = "call"]
    fn process_call(input: *const u8, length: u32, output: *mut u8, capacity: u32) -> i32;
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
        // SAFETY: Live owned disjoint buffers span the advertised ranges throughout
        // this synchronous fixed import. The Wasm host validates full memory ranges,
        // fixed output capacity and original invocation before dispatching anything.
        let size = unsafe {
            process_call(
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
