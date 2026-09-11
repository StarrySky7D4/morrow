//! Synthetic qualification example; no credentials or user data.
use morrow_plugin_sdk::{
    Client,
    protocol::{Action, Failure, Reply, Request},
    wasm,
};
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let host = wasm::host();
    // SAFETY: fixed import adapter owns no context and remains available for this call.
    let mut client = unsafe { Client::from_host(&host) }.unwrap();
    let request = Request {
        request_id: "wasm-op".into(),
        card_id: "legacy-123".into(),
        action: Action::Rename {
            revision: 1,
            title: "Wasm SDK rename".into(),
        },
    };
    let input = request.encode().unwrap();
    let Ok(bytes) = client.exchange(&input) else {
        return -1;
    };
    match request.decode_reply(&bytes) {
        Ok(Reply::Rejected(Failure::Denied)) => 10,
        Ok(Reply::Renamed(r)) if r.revision == 2 => 20,
        _ => 99,
    }
}
