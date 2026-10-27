use super::*;
use morrow_blob_transfer_v1::{
    Acceptance, Action, Frame, Limits as BlobLimits, ReceiptStatus, Receiver,
};

fn descriptor(bytes: &[u8]) -> Descriptor {
    Descriptor {
        identity: Identity {
            transfer_epoch: [11; 32],
            object_ref: [12; 32],
            operation_id: [13; 32],
        },
        byte_length: bytes.len() as u64,
        sha256: digest(bytes),
    }
}
fn frame(d: Descriptor, action: Action) -> Vec<u8> {
    Frame {
        identity: d.identity,
        action,
    }
    .encode()
    .unwrap()
}
fn desc(d: Descriptor) -> Vec<u8> {
    frame(
        d,
        Action::Descriptor {
            total_length: d.byte_length,
            whole_sha256: d.sha256,
        },
    )
}
fn end(d: Descriptor) -> Vec<u8> {
    frame(
        d,
        Action::End {
            total_length: d.byte_length,
            whole_sha256: d.sha256,
        },
    )
}
fn chunk(d: Descriptor, sequence: u64, offset: u64, bytes: &[u8]) -> Vec<u8> {
    frame(
        d,
        Action::Chunk {
            sequence,
            offset,
            payload: bytes.to_vec(),
            chunk_sha256: digest(bytes),
        },
    )
}
fn state(d: Descriptor, phase: Phase) -> State {
    State {
        descriptor: d,
        phase,
        received_bytes: if matches!(phase, Phase::Staged | Phase::Released) {
            d.byte_length
        } else {
            0
        },
        next_sequence: if matches!(phase, Phase::Ready | Phase::Receiving) {
            1
        } else {
            0
        },
    }
}
fn receipt(bytes: &[u8], status: ReceiptStatus) -> Vec<u8> {
    let f = FrameRef::decode(bytes).unwrap();
    let BlobAction::Chunk {
        sequence,
        offset,
        payload,
        chunk_sha256,
    } = f.action
    else {
        panic!()
    };
    frame(
        Descriptor {
            identity: f.identity,
            byte_length: 0,
            sha256: [0; 32],
        },
        Action::Receipt {
            request_digest: digest(bytes),
            sequence,
            offset,
            length: payload.len() as u64,
            chunk_sha256,
            status,
        },
    )
}
fn rewrite_request(request: &Request, edit: impl FnOnce(wire::request::Builder<'_>)) -> Vec<u8> {
    let mut remaining = request.wire();
    let original =
        serialize::read_message_from_flat_slice(&mut remaining, ReaderOptions::new()).unwrap();
    let mut message = Builder::new_default();
    message
        .set_root::<wire::request::Owned>(original.get_root::<wire::request::Reader<'_>>().unwrap())
        .unwrap();
    edit(message.get_root::<wire::request::Builder<'_>>().unwrap());
    serialize::write_message_to_words(&message)
}
fn rewrite_response(
    response: &Response,
    edit: impl FnOnce(wire::response::Builder<'_>),
) -> Vec<u8> {
    let mut remaining = response.wire();
    let original =
        serialize::read_message_from_flat_slice(&mut remaining, ReaderOptions::new()).unwrap();
    let mut message = Builder::new_default();
    message
        .set_root::<wire::response::Owned>(
            original.get_root::<wire::response::Reader<'_>>().unwrap(),
        )
        .unwrap();
    edit(message.get_root::<wire::response::Builder<'_>>().unwrap());
    serialize::write_message_to_words(&message)
}
fn exchange(c: &mut Client, nonce: u8, action: ActionRef<'_>, reply: ReplyRef<'_>) -> Response {
    let request = c.begin([nonce; 32], action).unwrap();
    let response = Response::new(request, reply).unwrap();
    c.accept(response.wire()).unwrap()
}
fn receiving(bytes: &[u8]) -> (Descriptor, Client) {
    let d = descriptor(bytes);
    let mut c = Client::new([1; 32], d, Default::default()).unwrap();
    exchange(
        &mut c,
        1,
        ActionRef::Open,
        ReplyRef::State(state(d, Phase::Ready)),
    );
    exchange(
        &mut c,
        2,
        ActionRef::Push(&desc(d)),
        ReplyRef::State(state(d, Phase::Receiving)),
    );
    (d, c)
}

#[test]
fn exact_frames_and_receipts_round_trip_without_claiming_chunk_durability() {
    let (d, mut c) = receiving(b"payload");
    let bytes = chunk(d, 1, 0, b"payload");
    let ack = receipt(&bytes, ReceiptStatus::Accepted);
    exchange(
        &mut c,
        3,
        ActionRef::Push(&bytes),
        ReplyRef::ChunkReceipt(&ack),
    );
    assert_eq!(c.snapshot().phase, ClientPhase::Receiving);
    assert_eq!(c.snapshot().received_bytes, 7);
    exchange(
        &mut c,
        4,
        ActionRef::Push(&end(d)),
        ReplyRef::State(state(d, Phase::Staged)),
    );
    assert_eq!(c.snapshot().phase, ClientPhase::Staged);
    exchange(
        &mut c,
        5,
        ActionRef::Release,
        ReplyRef::State(state(d, Phase::Released)),
    );
    assert_eq!(c.snapshot().phase, ClientPhase::Released);
    assert!(c.begin([6; 32], ActionRef::Push(&bytes)).is_err());
}
#[test]
fn all_request_variants_reject_hidden_frame_data_bad_contract_and_reserved_fields() {
    let q = Request::new([1; 32], [2; 32], ActionRef::Open).unwrap();
    for bytes in [
        rewrite_request(&q, |mut r| r.set_version(2)),
        rewrite_request(&q, |mut r| r.set_schema_sha256(&[8; 32])),
        rewrite_request(&q, |mut r| r.set_frame_schema_sha256(&[8; 32])),
        rewrite_request(&q, |mut r| r.set_reserved(1)),
        rewrite_request(&q, |mut r| r.set_action(5)),
        rewrite_request(&q, |mut r| r.set_nomination_ref(&[0; 32])),
        rewrite_request(&q, |mut r| r.set_nonce(&[0; 31])),
        rewrite_request(&q, |mut r| r.set_frame(b"hidden")),
    ] {
        assert!(Request::decode(&bytes).is_err());
    }
    for a in [ActionRef::Open, ActionRef::Inspect, ActionRef::Release] {
        let request = Request::new([1; 32], [2; 32], a).unwrap();
        assert_eq!(Request::decode(request.wire()).unwrap(), request);
    }
}
#[test]
fn maximum_chunk_stays_inside_old_call_bound_and_malformed_segment_headers_fail() {
    let payload = vec![7; morrow_blob_transfer_v1::MAX_CHUNK_BYTES];
    let d = descriptor(&payload);
    let bytes = chunk(d, 1, 0, &payload);
    assert!(bytes.len() <= MAX_EMBEDDED_FRAME_BYTES);
    let q = Request::new([1; 32], [2; 32], ActionRef::Push(&bytes)).unwrap();
    assert!(q.wire().len() <= MAX_REQUEST_BYTES);
    let ActionRef::Push(got) = q.action() else {
        panic!()
    };
    assert_eq!(got, bytes);
    let mut trailing = q.wire().to_vec();
    trailing.extend_from_slice(q.wire());
    assert!(Request::decode(&trailing).is_err());
    assert!(Request::decode(&[255; 64]).is_err());
    assert!(Request::decode(&vec![0; MAX_REQUEST_BYTES + 1]).is_err());
    assert!(
        Request::new(
            [1; 32],
            [2; 32],
            ActionRef::Push(&receipt(&bytes, ReceiptStatus::Accepted))
        )
        .is_err()
    );
}
#[test]
fn response_must_match_raw_original_request_and_every_receipt_identity() {
    let d = descriptor(b"abc");
    let bytes = chunk(d, 1, 0, b"abc");
    let q = Request::new([1; 32], [2; 32], ActionRef::Push(&bytes)).unwrap();
    let ack = receipt(&bytes, ReceiptStatus::Existing);
    let response = Response::new(&q, ReplyRef::ChunkReceipt(&ack)).unwrap();
    let other = Request::new([1; 32], [3; 32], ActionRef::Push(&bytes)).unwrap();
    assert_eq!(
        Response::decode_for(response.wire(), &other).unwrap_err(),
        Error::Correlation
    );
    let other_chunk = chunk(d, 2, 3, b"abc");
    assert!(
        Response::new(
            &q,
            ReplyRef::ChunkReceipt(&receipt(&other_chunk, ReceiptStatus::Accepted))
        )
        .is_err()
    );
    let mut other_id = d;
    other_id.identity.object_ref = [14; 32];
    assert!(
        Response::new(
            &q,
            ReplyRef::ChunkReceipt(&receipt(
                &chunk(other_id, 1, 0, b"abc"),
                ReceiptStatus::Accepted
            ))
        )
        .is_err()
    );
    assert!(Response::new(&q, ReplyRef::State(state(d, Phase::Staged))).is_err());
}
#[test]
fn response_forbids_extraneous_fields_and_invalid_phase_shape() {
    let d = descriptor(b"abc");
    let q = Request::new([1; 32], [2; 32], ActionRef::Open).unwrap();
    let r = Response::new(&q, ReplyRef::Error(Status::Denied)).unwrap();
    for bytes in [
        rewrite_response(&r, |mut r| r.set_transfer_epoch(&[8; 32])),
        rewrite_response(&r, |mut r| r.set_receipt(b"hidden")),
        rewrite_response(&r, |mut r| r.set_reserved(1)),
        rewrite_response(&r, |mut r| r.set_reserved2(1)),
        rewrite_response(&r, |mut r| r.set_byte_length(1)),
        rewrite_response(&r, |mut r| r.set_status(99)),
        rewrite_response(&r, |mut r| r.set_kind(9)),
    ] {
        assert!(Response::decode_for(&bytes, &q).is_err());
    }
    let r = Response::new(&q, ReplyRef::State(state(d, Phase::Ready))).unwrap();
    assert!(
        Response::decode_for(&rewrite_response(&r, |mut r| r.set_received_bytes(1)), &q).is_err()
    );
    assert!(Response::decode_for(&rewrite_response(&r, |mut r| r.set_status(1)), &q).is_err());
    let wrong = State {
        next_sequence: 0,
        ..state(d, Phase::Receiving)
    };
    assert!(wrong.validate().is_err());
}
#[test]
fn malformed_or_foreign_delivery_seals_unknown_and_retains_exact_pending_request() {
    for malformed in [vec![0; 8], vec![1; MAX_RESPONSE_BYTES + 1]] {
        let d = descriptor(b"abc");
        let mut c = Client::new([1; 32], d, Default::default()).unwrap();
        let original = c.begin([2; 32], ActionRef::Open).unwrap().clone();
        assert!(c.accept(&malformed).is_err());
        assert_eq!(c.snapshot().phase, ClientPhase::Unknown);
        assert_eq!(c.pending(), Some(&original));
        assert!(c.begin([3; 32], ActionRef::Inspect).is_err());
    }
}
#[test]
fn budgets_and_nonce_reuse_cannot_reset_when_source_is_released_or_refused() {
    let d = descriptor(b"abc");
    let mut c = Client::new(
        [1; 32],
        d,
        ClientLimits {
            max_calls: 2,
            max_wire_bytes: 4096,
        },
    )
    .unwrap();
    exchange(
        &mut c,
        2,
        ActionRef::Open,
        ReplyRef::State(state(d, Phase::Ready)),
    );
    assert_eq!(
        c.begin([2; 32], ActionRef::Inspect).unwrap_err(),
        Error::Correlation
    );
    exchange(
        &mut c,
        3,
        ActionRef::Release,
        ReplyRef::State(state(d, Phase::Absent)),
    );
    assert_eq!(
        c.begin([4; 32], ActionRef::Inspect).unwrap_err(),
        Error::Limit
    );
    assert_eq!(c.snapshot().calls, 2);
    assert!(c.snapshot().wire_bytes > 0);
}
#[test]
fn existing_receipt_is_counted_once_and_end_requires_true_whole_digest() {
    let (d, mut c) = receiving(b"abc");
    let bytes = chunk(d, 1, 0, b"xyz");
    exchange(
        &mut c,
        3,
        ActionRef::Push(&bytes),
        ReplyRef::ChunkReceipt(&receipt(&bytes, ReceiptStatus::Existing)),
    );
    assert_eq!(c.snapshot().received_bytes, 3);
    assert!(c.begin([4; 32], ActionRef::Push(&bytes)).is_err());
    assert_eq!(
        c.begin([5; 32], ActionRef::Push(&end(d))).unwrap_err(),
        Error::Correlation
    );
    assert_eq!(c.snapshot().phase, ClientPhase::Receiving);
}
#[test]
fn inspection_does_not_restore_a_sender_or_an_absent_operation_permission() {
    let d = descriptor(b"abc");
    let mut c = Client::new([1; 32], d, Default::default()).unwrap();
    exchange(
        &mut c,
        2,
        ActionRef::Inspect,
        ReplyRef::State(state(d, Phase::Receiving)),
    );
    assert_eq!(c.snapshot().phase, ClientPhase::Observed);
    assert!(
        c.begin([3; 32], ActionRef::Push(&chunk(d, 1, 0, b"abc")))
            .is_err()
    );
    assert!(c.begin([3; 32], ActionRef::Open).is_err());
    exchange(
        &mut c,
        4,
        ActionRef::Release,
        ReplyRef::State(state(d, Phase::Absent)),
    );
    assert!(c.begin([5; 32], ActionRef::Open).is_err());
}
#[test]
fn known_empty_object_still_requires_descriptor_and_end() {
    let (d, mut c) = receiving(b"");
    assert_eq!(c.snapshot().phase, ClientPhase::Receiving);
    exchange(
        &mut c,
        3,
        ActionRef::Push(&end(d)),
        ReplyRef::State(state(d, Phase::Staged)),
    );
    let invalid = Descriptor {
        sha256: [0; 32],
        ..d
    };
    assert!(invalid.validate().is_err());
}
#[test]
fn callback_is_invoked_once_and_never_replayed_after_delivery_loss() {
    let d = descriptor(b"abc");
    let mut c = Client::new([1; 32], d, Default::default()).unwrap();
    let mut calls = 0;
    assert!(
        c.call_once_with([2; 32], ActionRef::Open, |_, _| {
            calls += 1;
            -1
        })
        .is_err()
    );
    assert!(
        c.call_once_with([3; 32], ActionRef::Open, |_, _| {
            calls += 1;
            -1
        })
        .is_err()
    );
    assert_eq!(calls, 1);
    assert_eq!(c.snapshot().phase, ClientPhase::Unknown);
    c.abandon();
    assert_eq!(c.snapshot().phase, ClientPhase::Unknown);
}
#[test]
fn callback_panic_and_explicit_unknown_reply_keep_original_pending_evidence() {
    let d = descriptor(b"abc");
    let mut c = Client::new([1; 32], d, Default::default()).unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = c.call_once_with([2; 32], ActionRef::Open, |_, _| {
            panic!("test callback failure")
        });
    }));
    assert!(panic.is_err());
    assert_eq!(c.snapshot().phase, ClientPhase::Unknown);
    assert!(c.pending().is_some());
    assert!(c.begin([3; 32], ActionRef::Open).is_err());
    let mut c = Client::new([1; 32], d, Default::default()).unwrap();
    let original = c.begin([2; 32], ActionRef::Open).unwrap().clone();
    let response = Response::new(&original, ReplyRef::Error(Status::OutcomeUnknown)).unwrap();
    c.accept(response.wire()).unwrap();
    assert_eq!(c.pending(), Some(&original));
    assert_eq!(c.snapshot().phase, ClientPhase::Unknown);
}
#[test]
fn exact_identity_and_payload_never_appear_in_debug() {
    let d = descriptor(b"do not disclose payload");
    let bytes = chunk(d, 1, 0, b"do not disclose payload");
    let q = Request::new([1; 32], [2; 32], ActionRef::Push(&bytes)).unwrap();
    let value = format!(
        "{q:?} {d:?} {:?}",
        ReplyRef::ChunkReceipt(&receipt(&bytes, ReceiptStatus::Accepted))
    );
    assert!(
        !value.contains("disclose")
            && !value.contains("[11,")
            && !value.contains("nomination")
            && !value.contains("sha256")
    );
}

#[test]
fn release_and_durable_history_cannot_regress_to_a_new_upload() {
    let d = descriptor(b"abc");
    for (initial, regressed) in [
        (Phase::Released, Phase::Staged),
        (Phase::Staged, Phase::Ready),
    ] {
        let mut c = Client::new([1; 32], d, Default::default()).unwrap();
        exchange(
            &mut c,
            2,
            ActionRef::Open,
            ReplyRef::State(state(d, initial)),
        );
        let request = c.begin([3; 32], ActionRef::Inspect).unwrap();
        let response = Response::new(request, ReplyRef::State(state(d, regressed))).unwrap();
        assert_eq!(c.accept(response.wire()).unwrap_err(), Error::Correlation);
        assert_eq!(c.snapshot().phase, ClientPhase::Unknown);
        assert!(c.begin([4; 32], ActionRef::Open).is_err());
    }
}

#[test]
fn client_rejects_host_descriptor_substitution_before_any_upload() {
    let d = descriptor(b"abc");
    let mut changed = d;
    changed.identity.operation_id = [99; 32];
    let mut c = Client::new([1; 32], d, Default::default()).unwrap();
    let request = c.begin([2; 32], ActionRef::Open).unwrap();
    let response = Response::new(request, ReplyRef::State(state(changed, Phase::Ready))).unwrap();
    assert_eq!(c.accept(response.wire()).unwrap_err(), Error::Correlation);
    assert_eq!(c.snapshot().phase, ClientPhase::Unknown);
}

#[test]
fn semantically_equal_outer_encoding_does_not_share_a_response_digest() {
    let q = Request::new([1; 32], [2; 32], ActionRef::Open).unwrap();
    let changed = rewrite_request(&q, |mut root| root.set_nonce(&[2; 32]));
    let other = Request::decode(&changed).unwrap();
    assert_ne!(q.digest(), other.digest());
    let response = Response::new(&q, ReplyRef::Error(Status::Busy)).unwrap();
    assert_eq!(
        Response::decode_for(response.wire(), &other).unwrap_err(),
        Error::Correlation
    );
    let mut extra_layout = q.wire().to_vec();
    extra_layout[12] += 1;
    assert!(Request::decode(&extra_layout).is_err());
}

#[test]
fn response_wire_exhaustion_is_unknown_and_does_not_refund_request_charge() {
    let d = descriptor(b"abc");
    let q = Request::new([1; 32], [2; 32], ActionRef::Open).unwrap();
    let limits = ClientLimits {
        max_calls: 4,
        max_wire_bytes: q.wire().len() as u64,
    };
    let mut c = Client::new([1; 32], d, limits).unwrap();
    let request = c.begin([2; 32], ActionRef::Open).unwrap();
    let response = Response::new(request, ReplyRef::State(state(d, Phase::Ready))).unwrap();
    assert_eq!(c.accept(response.wire()).unwrap_err(), Error::Limit);
    assert_eq!(c.snapshot().phase, ClientPhase::Unknown);
    assert_eq!(c.snapshot().wire_bytes, limits.max_wire_bytes);
    assert_eq!(c.snapshot().calls, 1);
}

#[test]
fn valid_zero_length_chunk_preserves_original_blob_sequence_semantics() {
    let (d, mut c) = receiving(b"");
    let bytes = chunk(d, 1, 0, b"");
    exchange(
        &mut c,
        3,
        ActionRef::Push(&bytes),
        ReplyRef::ChunkReceipt(&receipt(&bytes, ReceiptStatus::Accepted)),
    );
    let observed = State {
        next_sequence: 2,
        ..state(d, Phase::Receiving)
    };
    exchange(&mut c, 4, ActionRef::Inspect, ReplyRef::State(observed));
    exchange(
        &mut c,
        5,
        ActionRef::Push(&end(d)),
        ReplyRef::State(state(d, Phase::Staged)),
    );
    assert_eq!(c.snapshot().phase, ClientPhase::Staged);
}
#[test]
fn maximum_object_stream_uses_existing_receiver_with_separate_durable_response() {
    let bytes = vec![7; MAX_OBJECT_BYTES as usize];
    let (d, mut c) = receiving(&bytes);
    let mut receiver = Receiver::new(BlobLimits::default()).unwrap();
    receiver.accept(&desc(d)).unwrap();
    let mut offset = 0;
    let mut sequence = 1u64;
    for data in bytes.chunks(morrow_blob_transfer_v1::MAX_CHUNK_BYTES) {
        let wire = chunk(d, sequence, offset, data);
        let Acceptance::Receipt(ack) = receiver.accept(&wire).unwrap() else {
            panic!()
        };
        let nonce = digest(&sequence.to_le_bytes());
        let request = c.begin(nonce, ActionRef::Push(&wire)).unwrap();
        let response =
            Response::new(request, ReplyRef::ChunkReceipt(&ack.encode().unwrap())).unwrap();
        c.accept(response.wire()).unwrap();
        offset += data.len() as u64;
        sequence += 1;
    }
    assert_eq!(offset, MAX_OBJECT_BYTES);
    assert_eq!(c.snapshot().phase, ClientPhase::Receiving);
    assert!(receiver.accept(&end(d)).is_ok());
    // Only this explicit fixture reply claims durable staging. Receiver's own
    // VerifiedBytes state above remains insufficient; no Store was used here.
    exchange(
        &mut c,
        7,
        ActionRef::Push(&end(d)),
        ReplyRef::State(state(d, Phase::Staged)),
    );
    assert_eq!(c.snapshot().phase, ClientPhase::Staged);
    assert_eq!(sequence - 1, 274);
}
