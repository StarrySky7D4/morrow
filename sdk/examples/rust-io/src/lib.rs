//! A single host-selected IO frame. The host owns grants, binding, and delivery.
use morrow_plugin_sdk::wasm;

#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(request) = wasm::read_io_request() else {
        return -1;
    };
    let Ok(response) = wasm::call_io(&request) else {
        return -1;
    };
    if wasm::complete_io_response(&response).is_ok() {
        0
    } else {
        -1
    }
}
