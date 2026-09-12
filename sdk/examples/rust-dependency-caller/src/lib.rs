//! Caller computes input, calls its approved slot, then wraps the correlated provider output.
use morrow_plugin_sdk::{dependency_call::Request, wasm};
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else {
        return -1;
    };
    let Some(t) = task.transform() else {
        return -1;
    };
    if t.handler != "bytes.dependency-wrap" || t.input_type != "bytes" || t.output_type != "bytes" {
        return -1;
    }
    let mut input = t.input.clone();
    input.make_ascii_uppercase();
    let Ok(request) = Request::new("reverse-1", "reverse", &input) else {
        return -1;
    };
    let Ok(output) = wasm::call_dependency(&request) else {
        return -1;
    };
    if output.output_type != "bytes" {
        return -1;
    }
    let mut result = b"A[".to_vec();
    result.extend_from_slice(&output.bytes);
    result.push(b']');
    if wasm::complete_output(&task, &result).is_ok() {
        0
    } else {
        -1
    }
}
