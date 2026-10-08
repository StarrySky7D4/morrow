//! Real Rust guest: seven correlated session calls through the existing R2 import.
#![deny(unsafe_code)]
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
mod ffi;

#[cfg(target_arch = "wasm32")]
fn run() -> Result<(), ()> {
    use morrow_codex_session_exec_client_r2::{
        Budget, Capabilities, Client, Scope,
        protocol::{Event, hash},
    };
    use morrow_plugin_sdk::wasm;
    let task = wasm::read_task().map_err(|_| ())?;
    let transform = task.transform().ok_or(())?;
    if transform.handler != "codex.session.continue"
        || transform.input_type != "codex.session.config.v1"
        || transform.output_type != "codex.session.receipt.v1"
        || transform.input.len() < 25
        || transform.input.len() > 280
    {
        return Err(());
    }
    let bytes = &transform.input;
    let generation = u64::from_le_bytes(bytes[..8].try_into().map_err(|_| ())?);
    let nonce: [u8; 16] = bytes[8..24].try_into().map_err(|_| ())?;
    let session_id = std::str::from_utf8(&bytes[24..]).map_err(|_| ())?;
    let child_id = format!("{session_id}-child");
    let scope = Scope {
        capabilities: Capabilities {
            read: true,
            write: true,
            ..Default::default()
        },
        sessions: vec![session_id.into(), child_id.clone()],
        operations: vec![],
        execution_domain: "session-only".into(),
    };
    let mut client = Client::new(
        ffi::Import,
        generation,
        nonce,
        scope,
        Budget {
            calls: 7,
            ..Default::default()
        },
    )
    .map_err(|_| ())?;
    let created = client.create(session_id).map_err(|_| ())?;
    let mut writer = client
        .open_writer(session_id, created.epoch)
        .map_err(|_| ())?;
    client
        .append(
            &mut writer,
            vec![Event {
                event_id: "guest-event".into(),
                body: b"opaque\0\xff".to_vec(),
            }],
        )
        .map_err(|_| ())?;
    client
        .checkpoint(&mut writer, b"sealed-state\0\xff".to_vec())
        .map_err(|_| ())?;
    let parent = client.snapshot(session_id, 0, 16).map_err(|_| ())?;
    if parent.events.len() != 1 || parent.events[0].event.body != b"opaque\0\xff" {
        return Err(());
    }
    client.continue_sealed(&parent, &child_id).map_err(|_| ())?;
    let child = client.snapshot(&child_id, 0, 16).map_err(|_| ())?;
    if child.checkpoint != parent.checkpoint
        || !child.events.is_empty()
        || child.info.parent_tail != 1
    {
        return Err(());
    }
    let mut output = generation.to_le_bytes().to_vec();
    output.extend_from_slice(&hash(&child.checkpoint));
    output.extend_from_slice(&parent.info.tail.to_le_bytes());
    wasm::complete_output(&task, &output).map_err(|_| ())
}

// This attribute exports a function; it permits no unsafe blocks outside ffi.rs.
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    match run() {
        Ok(()) => 0,
        Err(()) => -1,
    }
}
