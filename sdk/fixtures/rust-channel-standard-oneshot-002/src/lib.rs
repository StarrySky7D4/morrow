//! Fresh real-Wasm qualification consumer; no source authority or automatic retry.
use morrow_plugin_sdk::{
    channel::{Action, Directory, Kind, Request, Response, Status, TranscriptDigest},
    wasm,
};
fn bad_reply() -> morrow_plugin_sdk::Error {
    morrow_plugin_sdk::Error::BadReply
}
fn run() -> Result<(), morrow_plugin_sdk::Error> {
    let task = wasm::read_task()?;
    let transform = task.transform().ok_or_else(bad_reply)?;
    if transform.handler != "channel.bounded.consume"
        || transform.input_type != "morrow.channel.directory.v1"
        || transform.output_type != "bytes"
    {
        return Err(bad_reply());
    }
    let directory = Directory::decode(&transform.input).map_err(|_| bad_reply())?;
    if directory.channels.len() != 1 {
        return Err(bad_reply());
    }
    let endpoint = &directory.channels[0];
    // At most 5*(Receive, Query, Ack) + Close = 16 metered channel calls.
    let count = endpoint.budget.max_messages.min(5);
    if endpoint.budget.max_requests < 3 * count + 1 {
        return Err(bad_reply());
    }
    let mut serial = 0u64;
    let mut submit = |action| {
        serial += 1;
        let mut identity = TranscriptDigest::new();
        for bytes in [
            b"morrow.channel.bounded.request.v1".as_slice(),
            directory.scope_sha256.as_slice(),
            endpoint.reference.as_slice(),
            endpoint.source_epoch.as_slice(),
            serial.to_le_bytes().as_slice(),
        ] {
            identity.update(bytes).map_err(|_| bad_reply())?;
        }
        let request = Request {
            call_id: identity.finish(),
            reference: endpoint.reference,
            source_epoch: endpoint.source_epoch,
            action,
        };
        morrow_plugin_sdk::channel::transport::call_wasm(&request)
    };
    let mut owned: Vec<(Response, Response)> = Vec::new();
    let mut transcript = TranscriptDigest::new();
    let mut total = 0u64;
    let mut last_acked = 0u64;
    for _ in 0..count {
        let response = submit(Action::Receive {
            last_acked,
            credit_bytes: endpoint.budget.max_frame_bytes,
        })?;
        if response.status != Status::Frame {
            return Err(bad_reply());
        }
        // Keep the complete response and an independent owned snapshot through
        // Query, ACK, subsequent Receives and Close, not just through decoding.
        let snapshot = response.clone();
        let query = submit(Action::Query)?;
        if query.status != Status::Ready || response != snapshot {
            return Err(bad_reply());
        }
        let frame = response.frame.as_ref().ok_or_else(bad_reply)?;
        let sha = frame.digest().map_err(|_| bad_reply())?;
        transcript.update(&frame.bytes).map_err(|_| bad_reply())?;
        total += frame.bytes.len() as u64;
        let ack = submit(Action::Ack {
            sequence: frame.sequence,
            frame_sha256: sha,
            cursor: frame.cursor.clone(),
        })?;
        if ack.status != Status::Acked || ack.last_acked != frame.sequence {
            return Err(bad_reply());
        }
        last_acked = frame.sequence;
        owned.push((response, snapshot));
        if owned
            .iter()
            .any(|(response, snapshot)| response != snapshot)
        {
            return Err(bad_reply());
        }
    }
    // Close is explicit even if the source happened to finish. It is exactly
    // one submission; actual producer join is independently checked by host.
    let close = submit(Action::Close)?;
    if !matches!(close.status, Status::Closed | Status::ClosingUnconfirmed)
        || owned
            .iter()
            .any(|(response, snapshot)| response != snapshot)
    {
        return Err(bad_reply());
    }
    let mut summary = [0u8; 64];
    summary[..4].copy_from_slice(b"BSDK");
    let mode = if endpoint.kind == Kind::Events {
        2u32
    } else {
        0
    };
    summary[4..8].copy_from_slice(&mode.to_le_bytes());
    summary[8..12].copy_from_slice(&(close.status as u32).to_le_bytes());
    // Bit 1 reports the business fixture's ownership checks, not new SDK wire.
    let flags = 2 | u32::from(close.resource_reclaimed);
    summary[12..16].copy_from_slice(&flags.to_le_bytes());
    summary[16..24].copy_from_slice(&count.to_le_bytes());
    summary[24..32].copy_from_slice(&total.to_le_bytes());
    summary[32..].copy_from_slice(&transcript.finish());
    wasm::complete_output(&task, &summary)
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    if run().is_ok() { 0 } else { -1 }
}
