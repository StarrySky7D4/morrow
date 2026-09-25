//! Pure theme provider. No content, network, filesystem or IO permissions.
use morrow_plugin_sdk::wasm;
const THEME: &[u8] = include_bytes!("../theme.json");
const ART: &[u8] = include_bytes!("../artwork/moonlit-garden.webp");
const CHUNK: usize = 32 * 1024;
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else { return -1; };
    let Some(t) = task.transform() else { return -1; };
    let output = match (t.handler.as_str(), t.input_type.as_str(), t.output_type.as_str()) {
        ("theme.describe", "morrow.ui.theme.request.v1", "morrow.ui.theme.v1") if t.input.is_empty() => THEME,
        ("theme.artwork", "morrow.ui.theme.chunk.v1", "bytes") if t.input.len() == 4 => {
            let offset = u32::from_le_bytes(t.input.as_slice().try_into().unwrap()) as usize;
            if offset >= ART.len() || offset % CHUNK != 0 { return -1; }
            &ART[offset..(offset + CHUNK).min(ART.len())]
        },
        _ => return -1,
    };
    if wasm::complete_output(&task, output).is_ok() { 0 } else { -1 }
}
