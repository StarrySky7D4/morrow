//! Real Rust Wasm guest: discover actual support, then perform one host-selected action.
#![deny(unsafe_code)]
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
mod ffi;
#[cfg(target_arch = "wasm32")]
fn run() -> Result<(), ()> {
    use morrow_codex_process_control_client_v1::{
        Budget, Client,
        protocol::{Action, Capabilities, MAX_FRAME_BYTES, Request},
    };
    use morrow_plugin_sdk::wasm;
    let task = wasm::read_task().map_err(|_| ())?;
    let transform = task.transform().ok_or(())?;
    if transform.handler != "codex.process.control"
        || transform.input_type != "codex.process.request.v1"
        || transform.output_type != "codex.process.reply.v1"
    {
        return Err(());
    }
    let requested = Request::decode(&transform.input).map_err(|_| ())?;
    let declared = Capabilities {
        read: true,
        events: true,
        write: true,
        close_input: true,
        interrupt: true,
        terminate: true,
        resize_pty: true,
    };
    let mut client = Client::new(
        ffi::Import,
        requested.handle,
        requested.generation,
        requested.request_id,
        declared,
        Budget {
            calls: 2,
            request_bytes: 2 * MAX_FRAME_BYTES,
            reply_bytes: 2 * MAX_FRAME_BYTES,
        },
    )
    .map_err(|_| ())?;
    let discovery = client.call(Action::Discover).map_err(|_| ())?;
    let reply = if matches!(requested.action, Action::Discover) {
        discovery
    } else {
        client.call(requested.action).map_err(|_| ())?
    };
    let raw = reply.encode().map_err(|_| ())?;
    wasm::complete_output(&task, &raw).map_err(|_| ())
}
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    match run() {
        Ok(()) => 0,
        Err(()) => -1,
    }
}
