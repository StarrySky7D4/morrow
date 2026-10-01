//! Three modes share a bounded channel and only complete a 64-byte digest/status.
use morrow_plugin_sdk::{
    channel::{self, Action, Request, Status},
    wasm,
};
fn run() -> Result<(), morrow_plugin_sdk::Error> {
    let task = wasm::read_task()?;
    let transform = task
        .transform()
        .ok_or(morrow_plugin_sdk::Error::InvalidArgument)?;
    if transform.handler != "channel.exercise"
        || transform.input_type != "bytes"
        || transform.output_type != "bytes"
        || transform.input.len() != 65
        || transform.input[0] > 2
    {
        return Err(morrow_plugin_sdk::Error::InvalidArgument);
    }
    let mode = transform.input[0];
    let reference = transform.input[1..33].try_into().unwrap();
    let source_epoch = transform.input[33..65].try_into().unwrap();
    let mut request = Request {
        call_id: [1; 32],
        reference,
        source_epoch,
        action: Action::Query,
    };
    let mut call_number = 0u64;
    let mut submit = |action: Action| {
        call_number += 1;
        request.call_id = [0x43; 32];
        request.call_id[..8].copy_from_slice(&call_number.to_le_bytes());
        request.action = action;
        channel::transport::call_wasm(&request)
    };
    let mut digest = channel::TranscriptDigest::new();
    let mut count = 0u64;
    let mut total = 0u64;
    let mut status = Status::Ready;
    let mut reclaimed = false;
    if mode == 1 {
        for sequence in 1..=5u64 {
            let bytes: Vec<u8> = (0..32768usize)
                .map(|i| ((i + sequence as usize * 17) % 251) as u8)
                .collect();
            let response = submit(Action::Send {
                sequence,
                bytes: bytes.clone(),
            })?;
            status = response.status;
            reclaimed = response.resource_reclaimed;
            if status != Status::Accepted {
                break;
            }
            digest
                .update(&bytes)
                .map_err(|_| morrow_plugin_sdk::Error::BadReply)?;
            count += 1;
            total += bytes.len() as u64;
        }
    } else {
        let mut last_acked = 0;
        for _ in 0..32 {
            let response = submit(Action::Receive {
                last_acked,
                credit_bytes: 65536,
            })?;
            status = response.status;
            reclaimed = response.resource_reclaimed;
            if status != Status::Frame {
                break;
            }
            let frame = response.frame.ok_or(morrow_plugin_sdk::Error::BadReply)?;
            // Consumed bytes remain owned while the exact frame receipt is ACKed.
            let sha = frame
                .digest()
                .map_err(|_| morrow_plugin_sdk::Error::BadReply)?;
            digest
                .update(&frame.bytes)
                .map_err(|_| morrow_plugin_sdk::Error::BadReply)?;
            count += 1;
            total += frame.bytes.len() as u64;
            let ack = submit(Action::Ack {
                sequence: frame.sequence,
                frame_sha256: sha,
                cursor: frame.cursor,
            })?;
            status = ack.status;
            reclaimed = ack.resource_reclaimed;
            if status != Status::Acked {
                break;
            }
            last_acked = frame.sequence;
        }
    }
    if matches!(
        status,
        Status::Accepted | Status::Acked | Status::Idle | Status::Ready
    ) {
        let response = submit(Action::Close)?;
        status = response.status;
        reclaimed = response.resource_reclaimed;
    }
    let mut summary = [0u8; 64];
    summary[..4].copy_from_slice(b"CHV1");
    summary[4..8].copy_from_slice(&(mode as u32).to_le_bytes());
    summary[8..12].copy_from_slice(&(status as u32).to_le_bytes());
    summary[12..16].copy_from_slice(&u32::from(reclaimed).to_le_bytes());
    summary[16..24].copy_from_slice(&count.to_le_bytes());
    summary[24..32].copy_from_slice(&total.to_le_bytes());
    summary[32..].copy_from_slice(&digest.finish());
    wasm::complete_output(&task, &summary)
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    if run().is_ok() { 0 } else { -1 }
}
