//! Bounded echo service. Host owns authentication, publication and listener.
use morrow_plugin_sdk::{service::Reply, wasm};
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(request) = wasm::read_service_request() else {
        return -1;
    };
    let reply = Reply {
        status: 200,
        headers: vec![],
        body: request.invocation().body.clone(),
    };
    if wasm::complete_service_response(&request, &reply).is_ok() {
        0
    } else {
        -1
    }
}
