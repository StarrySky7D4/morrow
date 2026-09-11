//! Host-supplied content task; no embedded card, title, revision or operation ID.
use morrow_plugin_sdk::{Client, wasm};
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else {
        return -1;
    };
    let host = wasm::host();
    // SAFETY: the fixed adapter remains valid for this invocation and exposes no identity selector.
    let Ok(mut client) = (unsafe { Client::from_host(&host) }) else {
        return -1;
    };
    let Ok(response) = client.exchange(task.command_bytes()) else {
        return -1;
    };
    if wasm::complete_task(&task, &response).is_ok() {
        0
    } else {
        -1
    }
}
