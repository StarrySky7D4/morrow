//! Full typed metadata consumption; no reconnection or provider-success convention.
use morrow_plugin_sdk::{
    Error,
    channel::{self, Action, Request, Status},
    wasm,
};
use morrow_sse_event_v1::Event;
fn run() -> Result<(), Error> {
    let task = wasm::read_task()?;
    let input = task.transform().ok_or(Error::InvalidArgument)?;
    if input.handler != "channel.sse.sdk"
        || input.input_type != "bytes"
        || input.output_type != "bytes"
        || input.input.len() != 65
        || input.input[0] != 5
    {
        return Err(Error::InvalidArgument);
    }
    let mut request = Request {
        call_id: [1; 32],
        reference: input.input[1..33].try_into().unwrap(),
        source_epoch: input.input[33..65].try_into().unwrap(),
        action: Action::Query,
    };
    let mut serial = 0u64;
    let mut submit = |action| {
        serial += 1;
        if serial > 1024 {
            return Err(Error::Limit);
        }
        request.call_id = [0x65; 32];
        request.call_id[..8].copy_from_slice(&serial.to_le_bytes());
        request.action = action;
        channel::transport::call_wasm(&request)
    };
    let data = ["你\nsecond", "", "最终 🙂", "[DONE]", "after-DONE"];
    let event = ["delta", "message", "更新", "message", "message"];
    let retries = [
        None,
        Some(0),
        Some(u64::MAX),
        Some(u64::MAX),
        Some(u64::MAX),
    ];
    let mut transcript = channel::TranscriptDigest::new();
    let mut total = 0u64;
    let mut last_acked = 0;
    for i in 0..5 {
        let received = loop {
            let r = submit(Action::Receive {
                last_acked,
                credit_bytes: 32768,
            })?;
            if !matches!(r.status, Status::Idle | Status::ClosingUnconfirmed) {
                break r;
            }
        };
        if received.status != Status::Frame {
            return Err(Error::BadReply);
        }
        let frame = received.frame.ok_or(Error::BadReply)?;
        let decoded = Event::decode(&frame.bytes).map_err(|_| Error::BadReply)?;
        if frame.sequence != i as u64 + 1
            || decoded.data != data[i]
            || decoded.event != event[i]
            || decoded.retry != retries[i]
            || if i == 0 {
                decoded.id.len() != 300 || !decoded.id.bytes().all(|b| b == b'i')
            } else {
                !decoded.id.is_empty()
            }
        {
            return Err(Error::BadReply);
        }
        // Parser ignores the NUL-bearing id line; codec permits UTF8 NUL metadata.
        // Only the exact original incoming wire contributes transcript and ACK identity.
        transcript
            .update(&frame.bytes)
            .map_err(|_| Error::BadReply)?;
        total += frame.bytes.len() as u64;
        let ack = submit(Action::Ack {
            sequence: frame.sequence,
            frame_sha256: frame.digest().map_err(|_| Error::BadReply)?,
            cursor: frame.cursor,
        })?;
        if ack.status != Status::Acked || ack.last_acked != frame.sequence {
            return Err(Error::BadReply);
        }
        last_acked = frame.sequence;
    }
    let terminal = loop {
        let r = submit(Action::Receive {
            last_acked,
            credit_bytes: 32768,
        })?;
        if !matches!(r.status, Status::Idle | Status::ClosingUnconfirmed) {
            break r;
        }
    };
    if terminal.status != Status::Closed {
        return Err(Error::BadReply);
    }
    let mut summary = [0u8; 64];
    summary[..4].copy_from_slice(b"SES1");
    summary[4..8].copy_from_slice(&5u32.to_le_bytes());
    summary[8..12].copy_from_slice(&(terminal.status as u32).to_le_bytes());
    summary[12..16].copy_from_slice(&u32::from(terminal.resource_reclaimed).to_le_bytes());
    summary[16..24].copy_from_slice(&last_acked.to_le_bytes());
    summary[24..32].copy_from_slice(&total.to_le_bytes());
    summary[32..].copy_from_slice(&transcript.finish());
    wasm::complete_output(&task, &summary)
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    if run().is_ok() { 0 } else { -1 }
}
