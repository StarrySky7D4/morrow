//! Single-endpoint Directory consumer. Summary covers locally consumed payloads.
use morrow_plugin_sdk::{
    channel::{self, Action, Directory, Kind, Request, Status, TranscriptDigest},
    wasm,
};
fn bad_reply() -> morrow_plugin_sdk::Error {
    morrow_plugin_sdk::Error::BadReply
}
fn run() -> Result<(), morrow_plugin_sdk::Error> {
    let task = wasm::read_task()?;
    let transform = task
        .transform()
        .ok_or(morrow_plugin_sdk::Error::InvalidArgument)?;
    if transform.handler != "channel.directory.consume"
        || transform.input_type != "morrow.channel.directory.v1"
        || transform.output_type != "bytes"
    {
        return Err(morrow_plugin_sdk::Error::InvalidArgument);
    }
    let directory = Directory::decode(&transform.input).map_err(|_| bad_reply())?;
    if directory.channels.len() != 1 {
        return Err(morrow_plugin_sdk::Error::InvalidArgument);
    }
    let endpoint = &directory.channels[0];
    let mode = match endpoint.kind {
        Kind::ByteStream => 0u32,
        Kind::Events => 2,
    };
    let mut client = channel::transport::WasmClient::new()?;
    let mut calls = 0u64;
    let mut submit = |action: Action| {
        calls += 1;
        let counter = calls.to_le_bytes();
        let mut id = TranscriptDigest::new();
        for value in [
            b"morrow.channel.directory.request.v1".as_slice(),
            directory.scope_sha256.as_slice(),
            endpoint.reference.as_slice(),
            endpoint.source_epoch.as_slice(),
            counter.as_slice(),
        ] {
            id.update(value).map_err(|_| bad_reply())?;
        }
        client.call(&Request {
            call_id: id.finish(),
            reference: endpoint.reference,
            source_epoch: endpoint.source_epoch,
            action,
        })
    };
    let maximum = 32u64
        .min(endpoint.budget.max_messages)
        .min((endpoint.budget.max_requests - 1) / 2);
    let mut count = 0u64;
    let mut total = 0u64;
    let mut last_acked = 0;
    let mut status = Status::Ready;
    let mut reclaimed = false;
    let mut digest = TranscriptDigest::new();
    for _ in 0..maximum {
        let response = submit(Action::Receive {
            last_acked,
            credit_bytes: endpoint.budget.max_frame_bytes,
        })?;
        status = response.status;
        reclaimed = response.resource_reclaimed;
        if status != Status::Frame {
            break;
        }
        let frame = response.frame.ok_or_else(bad_reply)?;
        let sha = frame.digest().map_err(|_| bad_reply())?;
        digest.update(&frame.bytes).map_err(|_| bad_reply())?;
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
    if matches!(status, Status::Ready | Status::Idle | Status::Acked) {
        let response = submit(Action::Close)?;
        status = response.status;
        reclaimed = response.resource_reclaimed;
    }
    let mut summary = [0u8; 64];
    summary[..4].copy_from_slice(b"CHV1");
    summary[4..8].copy_from_slice(&mode.to_le_bytes());
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
