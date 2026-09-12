//! Independent provider used by the actual two-Rust-package dependency qualification.
use morrow_plugin_sdk::wasm;
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else { return -1; };
    let Some(input) = task.transform() else { return -1; };
    if input.handler != "bytes.tag-reverse" || input.input_type != "bytes"
        || input.output_type != "bytes" { return -1; }
    if input.input.len() > 65534 { return -1; }
    let mut bytes = b"B:".to_vec();
    bytes.extend(input.input.iter().rev());
    if wasm::complete_output(&task, &bytes).is_ok() { 0 } else { -1 }
}
