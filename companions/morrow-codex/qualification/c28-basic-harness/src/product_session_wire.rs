//! Qualification-only PS30 decoder. No controller, worker or authority factory.
use anyhow::{Result, anyhow, ensure};
use capnp::{message::ReaderOptions, serialize};
use morrow_agent_session_exec_v1_r2 as r2;
use morrow_workbench_host::{
    agent_product_control::{self as ps, Reply, Request},
    agent_product_control_capnp as wire,
};
use sha2::{Digest, Sha256};

pub(crate) enum PendingKind {
    Open,
    AdmitWriter,
    Exchange(r2::Request),
}
pub(crate) struct Pending {
    pub token: u64,
    pub kind: PendingKind,
    pub generation: Option<u64>,
    pub polls: u32,
}

fn fixed(value: capnp::Result<&[u8]>) -> Result<[u8; 32]> {
    value.map_err(|_| anyhow!("PS30 data field"))?.try_into()
        .map_err(|_| anyhow!("PS30 fixed field size"))
}

/// Match both transport request and the retained original Submit. The current
/// Read ID is never used to reconstruct that original R2 request.
pub(crate) fn decode_for(
    request: &Request,
    input: &[u8],
    pending: Option<&Pending>,
    output: &[u8],
) -> Result<Reply> {
    ensure!(Request::decode(input).map_err(|_| anyhow!("PS30 input"))? == *request,
        "PS30 exact original input");
    ensure!(output.len() >= 16 && output.len() <= ps::MAX_RESPONSE_BYTES
        && output.starts_with(ps::MAGIC), "PS30 response bounds/magic");
    ensure!((output.len() - 8) % 8 == 0, "PS30 aligned frame");
    let segment_count = u32::from_le_bytes(output[8..12].try_into().unwrap())
        .checked_add(1).ok_or_else(|| anyhow!("PS30 segment count overflow"))?;
    ensure!(segment_count <= 512, "PS30 finite segment count");
    let mut remaining = &output[8..];
    let message = serialize::read_message_from_flat_slice(&mut remaining, ReaderOptions {
        traversal_limit_in_words: Some(ps::MAX_RESPONSE_BYTES / 8 * 4), nesting_limit: 12,
    }).map_err(|_| anyhow!("PS30 bounded frame"))?;
    ensure!(remaining.is_empty(), "PS30 trailing bytes");
    let root = message.get_root::<wire::reply::Reader<'_>>()
        .map_err(|_| anyhow!("PS30 reply root"))?;
    ensure!(root.get_version() == ps::VERSION && fixed(root.get_digest())? == ps::digest()
        && fixed(root.get_id())? == request.id && fixed(root.get_task())? == request.task
        && root.get_operation().map_err(|_| anyhow!("PS30 operation enum"))? == request.operation
        && fixed(root.get_request_sha256())? == <[u8; 32]>::from(Sha256::digest(input)),
        "PS30 current request correlation");
    let status = root.get_status().map_err(|_| anyhow!("PS30 status enum"))?;
    let mut reply = Reply::new(status);
    reply.attachment = root.get_attachment();
    reply.submission = root.get_submission();
    reply.writer_slot = root.get_writer_slot();
    reply.generation = root.get_generation();
    reply.lease_live = root.get_lease_live();
    reply.delivery_unknown = root.get_delivery_unknown();
    let raw = root.get_canonical_reply().map_err(|_| anyhow!("PS30 R2 field"))?;
    ensure!(raw.len() <= r2::MAX_FRAME_BYTES, "PS30 R2 bounds");
    reply.canonical_reply = raw.to_vec();
    reply.canonical_request_sha256 = fixed(root.get_canonical_request_sha256())?;
    ensure!(request.operation == wire::Operation::Attach
        || reply.attachment == request.attachment, "PS30 attachment correlation");

    use wire::{Operation as Op, Status as S};
    let error_status = matches!(status, S::Unsupported | S::Unavailable | S::Busy
        | S::Invalid | S::Unknown | S::Limit);
    if !error_status {
        let allowed = match request.operation {
            Op::Attach => status == S::Attached,
            Op::Open | Op::Submit | Op::AdmitWriter => status == S::Submitted,
            Op::Status | Op::CloseWriter => status == S::Observed,
            Op::MarkLost => false, // Original MarkLost is always Unknown.
            Op::Detach => status == S::Detached,
            Op::Read => match pending.map(|p| &p.kind) {
                Some(PendingKind::Open) => matches!(status, S::Pending | S::Opened),
                Some(PendingKind::AdmitWriter) => matches!(status, S::Pending | S::WriterAdmitted),
                Some(PendingKind::Exchange(_)) => matches!(status, S::Pending | S::Reply),
                None => false,
            },
            Op::Unsupported => false,
        };
        ensure!(allowed, "PS30 operation/status mismatch");
        if request.operation == Op::Read {
            let p = pending.ok_or_else(|| anyhow!("PS30 missing original pending"))?;
            ensure!(request.submission == p.token && reply.submission == p.token,
                "PS30 original Read token");
            if let Some(generation) = p.generation {
                ensure!(reply.generation == generation, "PS30 original generation");
            }
        }
        match status {
            S::Attached => ensure!(reply.attachment != 0 && reply.submission == 0
                && reply.generation == 0, "PS30 Attach shape"),
            S::Submitted => ensure!(reply.submission != 0, "PS30 Submitted token"),
            S::Opened => ensure!(reply.generation != 0, "PS30 Opened generation"),
            S::WriterAdmitted => ensure!(reply.writer_slot != 0 && reply.writer_slot == reply.submission,
                "PS30 WriterAdmitted original slot/token"),
            S::Detached => ensure!(reply.submission == 0 && !reply.lease_live,
                "PS30 Detach shape"),
            _ => {},
        }
    }
    if status == S::Reply {
        let Some(Pending { kind: PendingKind::Exchange(original), .. }) = pending else {
            return Err(anyhow!("PS30 Reply without original Submit"));
        };
        let original_reply = r2::Reply::decode_for(original, &reply.canonical_reply)
            .map_err(|_| anyhow!("PS30 original R2 decode/correlation"))?;
        ensure!(reply.canonical_request_sha256 == original.digest()
            && !matches!(&original_reply.outcome, r2::Outcome::Tool(_) | r2::Outcome::Claimed { .. }),
            "PS30 original R2 digest/outcome");
        reply.outcome = Some(original_reply.outcome);
    } else {
        ensure!(reply.canonical_reply.is_empty() && reply.canonical_request_sha256 == [0; 32],
            "PS30 hidden R2 result");
    }
    ensure!(status == S::WriterAdmitted || reply.writer_slot == 0, "PS30 hidden writer slot");
    // Original encoder reconstructs every typed projection from original R2.
    // Equality rejects hidden fields, unknown result enums and alternate frames.
    ensure!(reply.encode_for(request, input).map_err(|_| anyhow!("PS30 original reply encoder"))?
        .as_slice() == output, "PS30 full-byte canonical reply/projection");
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(op: wire::Operation) -> Request {
        Request::empty([1; 32], [2; 32], if op == wire::Operation::Attach { 0 } else { 9 }, op)
    }
    #[test]
    fn rejects_foreign_task_id_and_trailing_bytes() {
        let q = request(wire::Operation::Attach);
        let input = q.encode().unwrap();
        let mut r = Reply::new(wire::Status::Attached);
        r.attachment = 9;
        let output = r.encode_for(&q, &input).unwrap();
        assert!(decode_for(&q, &input, None, &output).is_ok()); // Codec only, no authority.
        let mut foreign = q.clone();
        foreign.task = [3; 32];
        let bad = r.encode_for(&foreign, &foreign.encode().unwrap()).unwrap();
        assert!(decode_for(&q, &input, None, &bad).is_err());
        foreign = q.clone(); foreign.id = [4; 32];
        let bad = r.encode_for(&foreign, &foreign.encode().unwrap()).unwrap();
        assert!(decode_for(&q, &input, None, &bad).is_err());
        let mut trailing = output; trailing.extend([0; 8]);
        assert!(decode_for(&q, &input, None, &trailing).is_err());
    }
    #[test]
    fn rejects_wrong_pending_kind_token_generation_and_projection() {
        let mut submit = request(wire::Operation::Submit);
        submit.action = wire::Action::List; submit.generation = 7;
        let original = submit.canonical_request().unwrap();
        let mut q = request(wire::Operation::Read); q.id = [5; 32]; q.submission = 11;
        let input = q.encode().unwrap();
        let inner = r2::Reply::new(&original, r2::Outcome::Sessions(vec![])).unwrap();
        let mut r = Reply::new(wire::Status::Reply);
        r.attachment = 9; r.submission = 11; r.generation = 7; r.lease_live = true;
        r.canonical_request_sha256 = original.digest();
        r.canonical_reply = inner.encode().unwrap();
        r.outcome = Some(inner.outcome);
        let pending = Pending { token: 11, kind: PendingKind::Exchange(original.clone()),
            generation: Some(7), polls: 0 };
        let good = r.encode_for(&q, &input).unwrap();
        assert!(decode_for(&q, &input, Some(&pending), &good).is_ok());
        assert!(decode_for(&q, &input, None, &good).is_err());
        let wrong = Pending { token: 11, kind: PendingKind::Open, generation: None, polls: 0 };
        assert!(decode_for(&q, &input, Some(&wrong), &good).is_err());
        let wrong = Pending { token: 12, kind: PendingKind::Exchange(original.clone()),
            generation: Some(7), polls: 0 };
        assert!(decode_for(&q, &input, Some(&wrong), &good).is_err());
        let wrong = Pending { token: 11, kind: PendingKind::Exchange(original),
            generation: Some(8), polls: 0 };
        assert!(decode_for(&q, &input, Some(&wrong), &good).is_err());
        r.outcome = Some(r2::Outcome::Rejected(r2::Error::Denied));
        assert!(decode_for(&q, &input, Some(&pending), &r.encode_for(&q, &input).unwrap()).is_err());
    }
    #[test]
    fn close_slot_zero_and_detach_old_attachment_are_valid_codec_shapes() {
        let mut q = request(wire::Operation::CloseWriter); q.writer_slot = 21;
        let mut r = Reply::new(wire::Status::Observed); r.attachment = 9;
        let input = q.encode().unwrap();
        assert!(decode_for(&q, &input, None, &r.encode_for(&q, &input).unwrap()).is_ok());
        r.writer_slot = 21;
        assert!(decode_for(&q, &input, None, &r.encode_for(&q, &input).unwrap()).is_err());
        let q = request(wire::Operation::Detach); let input = q.encode().unwrap();
        let mut r = Reply::new(wire::Status::Detached); r.attachment = 9;
        assert!(decode_for(&q, &input, None, &r.encode_for(&q, &input).unwrap()).is_ok());
    }
}
