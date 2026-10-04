//! NEW qualification guest. It uses only the existing task/channel SDK imports.
use morrow_plugin_sdk::{channel::{self, Action, Request, Status}, wasm, Error};
include!("payloads.rs");
fn run() -> Result<(), Error> {
    let task = wasm::read_task()?;
    let input = task.transform().ok_or(Error::InvalidArgument)?;
    if input.handler != "channel.ws.duplex" || input.input_type != "bytes"
        || input.output_type != "bytes" || input.input.len() != 65 || input.input[0] != 3 {
        return Err(Error::InvalidArgument);
    }
    let mut request = Request { call_id: [1; 32], reference: input.input[1..33].try_into().unwrap(),
        source_epoch: input.input[33..65].try_into().unwrap(), action: Action::Query };
    let mut serial = 0u64;
    let mut submit = |action| {
        serial += 1;
        request.call_id = [0x57; 32];
        request.call_id[..8].copy_from_slice(&serial.to_le_bytes());
        request.action = action;
        channel::transport::call_wasm(&request)
    };
    let mut transcript = channel::TranscriptDigest::new();
    let mut total = 0u64;
    let mut last_acked = 0;
    for (index, payload) in PAYLOADS.iter().enumerate() {
        let sent = submit(Action::Send { sequence: index as u64 + 1, bytes: payload.to_vec() })?;
        if sent.status != Status::Accepted { return Err(Error::BadReply); }
        let received = submit(Action::Receive { last_acked, credit_bytes: 32768 })?;
        if received.status != Status::Frame { return Err(Error::BadReply); }
        let frame = received.frame.ok_or(Error::BadReply)?;
        if frame.bytes != *payload || frame.sequence != index as u64 + 1 { return Err(Error::BadReply); }
        transcript.update(&frame.bytes).map_err(|_| Error::BadReply)?;
        total += frame.bytes.len() as u64;
        let ack = submit(Action::Ack { sequence: frame.sequence,
            frame_sha256: frame.digest().map_err(|_| Error::BadReply)?, cursor: frame.cursor })?;
        if ack.status != Status::Acked { return Err(Error::BadReply); }
        last_acked = index as u64 + 1;
    }
    // A peer Close payload is not EOF/join. Await the original source's terminal.
    let terminal = submit(Action::Receive { last_acked, credit_bytes: 32768 })?;
    if terminal.status != Status::Closed { return Err(Error::BadReply); }
    let mut summary = [0u8; 64];
    summary[..4].copy_from_slice(b"WSV1");
    summary[4..8].copy_from_slice(&3u32.to_le_bytes());
    summary[8..12].copy_from_slice(&(terminal.status as u32).to_le_bytes());
    summary[12..16].copy_from_slice(&u32::from(terminal.resource_reclaimed).to_le_bytes());
    summary[16..24].copy_from_slice(&3u64.to_le_bytes());
    summary[24..32].copy_from_slice(&total.to_le_bytes());
    summary[32..].copy_from_slice(&transcript.finish());
    wasm::complete_output(&task, &summary)
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 { if run().is_ok() { 0 } else { -1 } }
