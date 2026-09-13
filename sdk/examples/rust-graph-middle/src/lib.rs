//! Actual middle guest: forward input to the approved leaf slot and wrap its correlated output.
use morrow_plugin_sdk::{dependency_call::Request, wasm};
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else {
        return -1;
    };
    let Some(input) = task.transform() else {
        return -1;
    };
    if input.handler != "bytes.graph-middle"
        || input.input_type != "bytes"
        || input.output_type != "bytes"
    {
        return -1;
    }
    let Ok(request) = Request::new("leaf-1", "leaf", &input.input) else {
        return -1;
    };
    let Ok(output) = wasm::call_dependency(&request) else {
        return -1;
    };
    if output.output_type != "bytes" {
        return -1;
    }
    let mut result = b"M[".to_vec();
    result.extend_from_slice(&output.bytes);
    result.push(b']');
    if wasm::complete_output(&task, &result).is_ok() {
        0
    } else {
        -1
    }
}
