//! Existing ABI2 Transform adapter; no core exchange, I/O, or fallback.
use morrow_cc_switch_pure::{HANDLER, INPUT_TYPE, OUTPUT_TYPE, ProfileError, evaluate_json};
use morrow_plugin_sdk::{task::FailureCode, wasm};
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else {
        return -1;
    };
    let Some(transform) = task.transform() else {
        return -1;
    };
    let completion = if transform.handler != HANDLER
        || transform.input_type != INPUT_TYPE
        || transform.output_type != OUTPUT_TYPE
    {
        wasm::complete_failure(
            &task,
            FailureCode::UnsupportedInput,
            "Unsupported capability transform profile",
        )
    } else {
        match evaluate_json(&transform.input) {
            Ok(output) => wasm::complete_output(&task, &output),
            Err(error) => {
                let code = match error {
                    ProfileError::InvalidInput => FailureCode::InvalidInput,
                    ProfileError::UnsupportedInput => FailureCode::UnsupportedInput,
                    ProfileError::ResourceLimit => FailureCode::ResourceLimit,
                };
                wasm::complete_failure(&task, code, "Capability transform input rejected")
            }
        }
    };
    if completion.is_ok() { 0 } else { -1 }
}
