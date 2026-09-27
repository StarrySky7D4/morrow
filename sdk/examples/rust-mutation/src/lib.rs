//! One host-supplied mutation request, one guest call, one correlated completion.
//! The host alone issues selection and execution authority for this invocation.
use morrow_plugin_sdk::mutation;

#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(frame) = mutation::wasm_read_request_frame() else {
        return -1;
    };
    let Ok(response) = mutation::wasm_call_frame(&frame) else {
        return -1;
    };
    if mutation::wasm_complete_response(&response).is_ok() {
        0
    } else {
        -1
    }
}
