use morrow_core::{
    Error,
    mutation::{self, Action, Effect, Kind, Phase, Request, Response, Status},
};
use sha2::{Digest, Sha256};

fn request(action: Action) -> Request {
    Request {
        call_id: 7,
        reference: [1; 32],
        submission: [2; 32],
        operation_id: "mutation-test".into(),
        deadline_ms: 1_000,
        action,
    }
}

#[test]
fn schema_limits_match_existing_content_contract() {
    assert_eq!(
        mutation::MAX_CONTENT_BYTES as usize,
        morrow_core::file_content::MAX_CONTENT_BYTES
    );
    assert_eq!(mutation::MAX_FRAME_BYTES, 128 * 1024);
    assert_eq!(mutation::MAX_CHUNK_BYTES, 60 * 1024);
    assert_eq!(mutation::MAX_OPERATION_BYTES, 256);
    assert_eq!(mutation::MAX_DEADLINE_MS, 30_000);
}

#[test]
fn independent_capnp_vectors_decode_and_reencode() {
    const GOOD: [(&str, &[u8], Kind); 9] = [
        (
            "create-empty",
            include_bytes!("../../sdk/vectors/mutation-v1/create-empty.bin"),
            Kind::PrepareCreate,
        ),
        (
            "create-content",
            include_bytes!("../../sdk/vectors/mutation-v1/create-content.bin"),
            Kind::PrepareCreate,
        ),
        (
            "delete",
            include_bytes!("../../sdk/vectors/mutation-v1/delete.bin"),
            Kind::PrepareDelete,
        ),
        (
            "chunk",
            include_bytes!("../../sdk/vectors/mutation-v1/chunk.bin"),
            Kind::Chunk,
        ),
        (
            "commit",
            include_bytes!("../../sdk/vectors/mutation-v1/commit.bin"),
            Kind::Commit,
        ),
        (
            "execute",
            include_bytes!("../../sdk/vectors/mutation-v1/execute.bin"),
            Kind::Execute,
        ),
        (
            "query",
            include_bytes!("../../sdk/vectors/mutation-v1/query.bin"),
            Kind::Query,
        ),
        (
            "cancel",
            include_bytes!("../../sdk/vectors/mutation-v1/cancel.bin"),
            Kind::CancelPlan,
        ),
        (
            "release",
            include_bytes!("../../sdk/vectors/mutation-v1/release.bin"),
            Kind::Release,
        ),
    ];
    for (name, bytes, kind) in GOOD {
        let decoded = Request::decode(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(decoded.action.kind(), kind, "{name}");
        assert_eq!(
            Request::decode(&decoded.encode().unwrap()).unwrap(),
            decoded,
            "{name}"
        );
    }
    const BAD: [(&str, &[u8]); 7] = [
        (
            "bad-zero-reference",
            include_bytes!("../../sdk/vectors/mutation-v1/bad-zero-reference.bin"),
        ),
        (
            "bad-empty-hash",
            include_bytes!("../../sdk/vectors/mutation-v1/bad-empty-hash.bin"),
        ),
        (
            "bad-chunk-overflow",
            include_bytes!("../../sdk/vectors/mutation-v1/bad-chunk-overflow.bin"),
        ),
        (
            "bad-call",
            include_bytes!("../../sdk/vectors/mutation-v1/bad-call.bin"),
        ),
        (
            "bad-deadline",
            include_bytes!("../../sdk/vectors/mutation-v1/bad-deadline.bin"),
        ),
        (
            "bad-unicode-control",
            include_bytes!("../../sdk/vectors/mutation-v1/bad-unicode-control.bin"),
        ),
        (
            "bad-trailing",
            include_bytes!("../../sdk/vectors/mutation-v1/bad-trailing.bin"),
        ),
    ];
    for (name, bytes) in BAD {
        assert!(Request::decode(bytes).is_err(), "{name}");
    }
    let execute =
        Request::decode(include_bytes!("../../sdk/vectors/mutation-v1/execute.bin")).unwrap();
    for (name, bytes) in [
        (
            "created",
            include_bytes!("../../sdk/vectors/mutation-v1/created.bin").as_slice(),
        ),
        (
            "unknown",
            include_bytes!("../../sdk/vectors/mutation-v1/unknown.bin").as_slice(),
        ),
        (
            "denied",
            include_bytes!("../../sdk/vectors/mutation-v1/denied.bin").as_slice(),
        ),
    ] {
        let decoded = Response::decode(&execute, bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            Response::decode(&execute, &decoded.encode(&execute).unwrap()).unwrap(),
            decoded
        );
    }
    for (name, bytes) in [
        (
            "bad-success-effect",
            include_bytes!("../../sdk/vectors/mutation-v1/bad-success-effect.bin").as_slice(),
        ),
        (
            "bad-denied-effect",
            include_bytes!("../../sdk/vectors/mutation-v1/bad-denied-effect.bin").as_slice(),
        ),
    ] {
        assert!(Response::decode(&execute, bytes).is_err(), "{name}");
    }
}

#[test]
fn request_rejects_invalid_identities_and_bounds() {
    let mut value = request(Action::Delete);
    value.call_id = 0;
    assert!(value.encode().is_err());
    value.call_id = 7;
    value.reference = [0; 32];
    assert!(value.encode().is_err());
    value.reference = [1; 32];
    value.submission = [0; 32];
    assert!(value.encode().is_err());
    value.submission = [2; 32];
    for bad in ["", "a/b", "a\\b", "a:b", "a\n"] {
        value.operation_id = bad.into();
        assert!(value.encode().is_err(), "{bad:?}");
    }
    value.operation_id = "x".repeat(mutation::MAX_OPERATION_BYTES + 1);
    assert_eq!(value.encode(), Err(Error::Limit));
    value.operation_id = "good".into();
    for bad in [0, mutation::MAX_DEADLINE_MS + 1] {
        value.deadline_ms = bad;
        assert!(value.encode().is_err());
    }
    let empty_hash: [u8; 32] = Sha256::digest([]).into();
    assert!(
        request(Action::Create {
            content_length: 0,
            content_sha256: empty_hash
        })
        .encode()
        .is_ok()
    );
    assert!(
        request(Action::Create {
            content_length: 0,
            content_sha256: [3; 32]
        })
        .encode()
        .is_err()
    );
    assert!(
        request(Action::Create {
            content_length: 1,
            content_sha256: [0; 32]
        })
        .encode()
        .is_err()
    );
    assert!(
        request(Action::Create {
            content_length: mutation::MAX_CONTENT_BYTES + 1,
            content_sha256: [3; 32]
        })
        .encode()
        .is_err()
    );
    for (offset, bytes) in [
        (0, vec![]),
        (0, vec![1; mutation::MAX_CHUNK_BYTES + 1]),
        (mutation::MAX_CONTENT_BYTES, vec![1]),
        (u64::MAX, vec![1]),
    ] {
        assert!(request(Action::Chunk { offset, bytes }).encode().is_err());
    }
}

#[test]
fn response_must_match_exact_request_and_action() {
    let execute = request(Action::Execute);
    let good = Response::for_request(
        &execute,
        Status::Completed,
        Phase::Observed,
        Effect::OsSucceeded,
    );
    let encoded = good.encode(&execute).unwrap();
    assert_eq!(Response::decode(&execute, &encoded).unwrap(), good);
    let mut other = execute.clone();
    other.call_id += 1;
    assert_eq!(Response::decode(&other, &encoded), Err(Error::Integrity));
    other = execute.clone();
    other.reference[0] ^= 1;
    assert_eq!(Response::decode(&other, &encoded), Err(Error::Integrity));
    other = execute.clone();
    other.submission[0] ^= 1;
    assert_eq!(Response::decode(&other, &encoded), Err(Error::Integrity));
    other = execute.clone();
    other.operation_id.push('x');
    assert_eq!(Response::decode(&other, &encoded), Err(Error::Integrity));
    other = execute.clone();
    other.action = Action::Query;
    assert_eq!(Response::decode(&other, &encoded), Err(Error::Integrity));
    other = execute.clone();
    other.call_id = 0;
    assert!(Response::decode(&other, &encoded).is_err());
}

#[test]
fn response_matrix_rejects_cross_action_and_false_success() {
    let create = request(Action::Create {
        content_length: 3,
        content_sha256: [3; 32],
    });
    let prepared = Response::for_request(
        &create,
        Status::Completed,
        Phase::Prepared,
        Effect::Unspecified,
    );
    assert!(prepared.encode(&create).is_ok());
    let mut bad = prepared.clone();
    bad.effect = Effect::OsSucceeded;
    assert!(bad.encode(&create).is_err());
    let chunk = request(Action::Chunk {
        offset: 5,
        bytes: vec![1, 2, 3],
    });
    let mut chunk_reply = Response::for_request(
        &chunk,
        Status::Completed,
        Phase::Prepared,
        Effect::Unspecified,
    );
    chunk_reply.staged_bytes = 8;
    assert!(chunk_reply.encode(&chunk).is_ok());
    chunk_reply.staged_bytes = 7;
    assert!(chunk_reply.encode(&chunk).is_err());
    let execute = request(Action::Execute);
    let unknown = Response::for_request(
        &execute,
        Status::OutcomeUnknown,
        Phase::OutcomeUnknown,
        Effect::Unspecified,
    );
    assert!(unknown.encode(&execute).is_ok());
    let mut bad = unknown.clone();
    bad.effect = Effect::OsSucceeded;
    assert!(bad.encode(&execute).is_err());
    bad = Response::for_request(
        &execute,
        Status::Denied,
        Phase::Observed,
        Effect::OsRejected,
    );
    assert!(bad.encode(&execute).is_err());
    bad = Response::for_request(
        &execute,
        Status::Completed,
        Phase::Observed,
        Effect::Unspecified,
    );
    assert!(bad.encode(&execute).is_err());
    let query = request(Action::Query);
    assert!(
        Response::for_request(
            &query,
            Status::Completed,
            Phase::Absent,
            Effect::Unspecified
        )
        .encode(&query)
        .is_ok()
    );
    assert!(
        Response::for_request(
            &query,
            Status::Completed,
            Phase::Observed,
            Effect::OsRejected
        )
        .encode(&query)
        .is_ok()
    );
    assert!(
        Response::for_request(
            &query,
            Status::Completed,
            Phase::CancelledBeforeDispatch,
            Effect::OsRejected
        )
        .encode(&query)
        .is_err()
    );
    let release = request(Action::Release);
    assert!(
        Response::for_request(
            &release,
            Status::Completed,
            Phase::None,
            Effect::Unspecified
        )
        .encode(&release)
        .is_ok()
    );
    assert!(
        Response::for_request(
            &release,
            Status::Completed,
            Phase::Prepared,
            Effect::Unspecified
        )
        .encode(&release)
        .is_err()
    );
}

#[test]
fn truncated_trailing_and_oversized_frames_fail_without_payload_allocation() {
    let frame = request(Action::Chunk {
        offset: 0,
        bytes: vec![7; 16],
    })
    .encode()
    .unwrap();
    for end in 0..frame.len() {
        assert!(Request::decode(&frame[..end]).is_err());
    }
    let mut trailing = frame.clone();
    trailing.extend_from_slice(&[0; 8]);
    assert!(Request::decode(&trailing).is_err());
    let mut oversized = frame;
    oversized.resize(mutation::MAX_FRAME_BYTES + 1, 0);
    assert_eq!(Request::decode(&oversized), Err(Error::Limit));
}

#[test]
fn wire_header_and_result_enum_cannot_bypass_validation() {
    use capnp::{message::Builder, serialize};
    use morrow_core::mutation_capnp as wire;

    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::request::Builder>();
    root.set_version(mutation::VERSION + 1);
    root.set_schema_sha256(&mutation::schema_digest());
    root.set_call_id(7);
    root.set_reference(&[1; 32]);
    root.set_submission(&[2; 32]);
    root.set_operation_id("mutation-test");
    root.set_deadline_ms(1_000);
    root.set_prepare_delete(());
    drop(root);
    let bytes = serialize::write_message_to_words(&message);
    assert_eq!(Request::decode(&bytes), Err(Error::UnsupportedVersion));

    let mut root = message.get_root::<wire::request::Builder>().unwrap();
    root.set_version(mutation::VERSION);
    root.set_schema_sha256(&[4]);
    let bytes = serialize::write_message_to_words(&message);
    assert_eq!(Request::decode(&bytes), Err(Error::UnsupportedVersion));

    let execute = request(Action::Execute);
    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::response::Builder>();
    root.set_version(mutation::VERSION);
    root.set_schema_sha256(&mutation::schema_digest());
    root.set_call_id(execute.call_id);
    root.set_reference(&execute.reference);
    root.set_submission(&execute.submission);
    root.set_operation_id(&execute.operation_id);
    root.set_kind(Kind::Execute);
    root.set_status(Status::Invalid);
    root.set_phase(Phase::Observed);
    root.set_effect(Effect::OsSucceeded);
    let bytes = serialize::write_message_to_words(&message);
    assert!(Response::decode(&execute, &bytes).is_err());
}
