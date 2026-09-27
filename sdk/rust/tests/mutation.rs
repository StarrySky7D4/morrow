use capnp::{message::Builder, serialize};
use morrow_plugin_sdk::{
    mutation::{self, Action, Effect, Error, Kind, Phase, Request, Response, Status},
    mutation_capnp as wire,
};
use sha2::{Digest, Sha256};

fn request(action: Action) -> Request {
    Request {
        call_id: 7,
        reference: [0x11; 32],
        submission: [0x22; 32],
        operation_id: "mutation-vector-1".into(),
        deadline_ms: wire::MAX_DEADLINE_MS,
        action,
    }
}
fn response(request: &Request) -> Response {
    Response {
        call_id: request.call_id,
        reference: request.reference,
        submission: request.submission,
        operation_id: request.operation_id.clone(),
        kind: request.action.kind(),
        status: Status::Completed,
        phase: Phase::Observed,
        effect: Effect::OsSucceeded,
        staged_bytes: 0,
        durable_content: false,
    }
}

#[test]
fn independent_capnp_cli_vectors() {
    let valid_requests: &[(&str, &[u8])] = &[
        (
            "create-empty",
            include_bytes!("../../vectors/mutation-v1/create-empty.bin"),
        ),
        (
            "create-content",
            include_bytes!("../../vectors/mutation-v1/create-content.bin"),
        ),
        (
            "delete",
            include_bytes!("../../vectors/mutation-v1/delete.bin"),
        ),
        (
            "chunk",
            include_bytes!("../../vectors/mutation-v1/chunk.bin"),
        ),
        (
            "commit",
            include_bytes!("../../vectors/mutation-v1/commit.bin"),
        ),
        (
            "execute",
            include_bytes!("../../vectors/mutation-v1/execute.bin"),
        ),
        (
            "query",
            include_bytes!("../../vectors/mutation-v1/query.bin"),
        ),
        (
            "cancel",
            include_bytes!("../../vectors/mutation-v1/cancel.bin"),
        ),
        (
            "release",
            include_bytes!("../../vectors/mutation-v1/release.bin"),
        ),
    ];
    for (name, bytes) in valid_requests {
        let value = Request::decode(bytes).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(value.call_id, 7, "{name}");
        assert_eq!(value.reference, [0x11; 32], "{name}");
        assert_eq!(value.submission, [0x22; 32], "{name}");
        assert_eq!(value.operation_id, "mutation-vector-1", "{name}");
        assert!(Request::decode(&value.encode().unwrap()).is_ok(), "{name}");
    }
    let invalid_requests: &[(&str, &[u8])] = &[
        (
            "bad-zero-reference",
            include_bytes!("../../vectors/mutation-v1/bad-zero-reference.bin"),
        ),
        (
            "bad-empty-hash",
            include_bytes!("../../vectors/mutation-v1/bad-empty-hash.bin"),
        ),
        (
            "bad-chunk-overflow",
            include_bytes!("../../vectors/mutation-v1/bad-chunk-overflow.bin"),
        ),
        (
            "bad-call",
            include_bytes!("../../vectors/mutation-v1/bad-call.bin"),
        ),
        (
            "bad-deadline",
            include_bytes!("../../vectors/mutation-v1/bad-deadline.bin"),
        ),
        (
            "bad-unicode-control",
            include_bytes!("../../vectors/mutation-v1/bad-unicode-control.bin"),
        ),
        (
            "bad-trailing",
            include_bytes!("../../vectors/mutation-v1/bad-trailing.bin"),
        ),
    ];
    for (name, bytes) in invalid_requests {
        assert!(Request::decode(bytes).is_err(), "{name}");
    }
    let execute = Request::decode(include_bytes!("../../vectors/mutation-v1/execute.bin")).unwrap();
    let valid_responses: &[(&str, &[u8])] = &[
        (
            "created",
            include_bytes!("../../vectors/mutation-v1/created.bin"),
        ),
        (
            "unknown",
            include_bytes!("../../vectors/mutation-v1/unknown.bin"),
        ),
        (
            "denied",
            include_bytes!("../../vectors/mutation-v1/denied.bin"),
        ),
    ];
    for (name, bytes) in valid_responses {
        let value = execute
            .decode_reply(bytes)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert!(
            execute
                .decode_reply(&value.encode(&execute).unwrap())
                .is_ok(),
            "{name}"
        );
    }
    for (name, bytes) in [
        (
            "bad-success-effect",
            include_bytes!("../../vectors/mutation-v1/bad-success-effect.bin").as_slice(),
        ),
        (
            "bad-denied-effect",
            include_bytes!("../../vectors/mutation-v1/bad-denied-effect.bin").as_slice(),
        ),
    ] {
        assert!(execute.decode_reply(bytes).is_err(), "{name}");
    }
}

#[test]
fn all_actions_round_trip_and_request_boundaries() {
    let empty_hash: [u8; 32] = Sha256::digest([]).into();
    let actions = [
        Action::Create {
            content_length: 0,
            content_sha256: empty_hash,
        },
        Action::Create {
            content_length: 3,
            content_sha256: Sha256::digest([0, 1, 255]).into(),
        },
        Action::Delete,
        Action::Chunk {
            offset: 0,
            bytes: vec![0, 1, 255],
        },
        Action::Commit,
        Action::Execute,
        Action::Query,
        Action::CancelPlan,
        Action::Release,
    ];
    for action in actions {
        let input = request(action);
        assert_eq!(Request::decode(&input.encode().unwrap()).unwrap(), input);
    }
    let mut bad = request(Action::Delete);
    bad.call_id = 0;
    assert_eq!(bad.encode(), Err(Error::Invalid));
    bad.call_id = 7;
    bad.reference = [0; 32];
    assert_eq!(bad.encode(), Err(Error::Invalid));
    bad.reference = [1; 32];
    bad.submission = [0; 32];
    assert_eq!(bad.encode(), Err(Error::Invalid));
    bad.submission = [2; 32];
    bad.operation_id = "bad/name".into();
    assert_eq!(bad.encode(), Err(Error::Invalid));
    bad.operation_id = "a".repeat(wire::MAX_OPERATION_BYTES as usize + 1);
    assert_eq!(bad.encode(), Err(Error::Limit));
    bad.operation_id = "中文操作".into();
    assert!(bad.encode().is_ok());
    bad.operation_id = "bad\u{0085}name".into();
    assert_eq!(bad.encode(), Err(Error::Invalid));
    bad.operation_id = "ok".into();
    bad.deadline_ms = 0;
    assert_eq!(bad.encode(), Err(Error::Invalid));
    bad.deadline_ms = wire::MAX_DEADLINE_MS + 1;
    assert_eq!(bad.encode(), Err(Error::Limit));
    for action in [
        Action::Create {
            content_length: 0,
            content_sha256: [1; 32],
        },
        Action::Create {
            content_length: wire::MAX_CONTENT_BYTES + 1,
            content_sha256: [1; 32],
        },
        Action::Chunk {
            offset: u64::MAX,
            bytes: vec![1],
        },
        Action::Chunk {
            offset: 0,
            bytes: vec![],
        },
        Action::Chunk {
            offset: 0,
            bytes: vec![1; wire::MAX_CHUNK_BYTES as usize + 1],
        },
    ] {
        assert!(request(action).encode().is_err());
    }
    assert_eq!(
        Request::decode(&vec![0; mutation::MAX_FRAME_BYTES + 1]),
        Err(Error::Limit)
    );
    assert_eq!(Request::decode(&[0]), Err(Error::Invalid));
    let mut trailing = request(Action::Delete).encode().unwrap();
    trailing.extend_from_slice(&[0; 8]);
    assert_eq!(Request::decode(&trailing), Err(Error::Invalid));
}

#[allow(clippy::too_many_arguments)] // Intentional one-to-one wire-field mutation fixture.
fn raw_response(
    version: u16,
    digest: &[u8],
    call_id: u64,
    reference: &[u8],
    submission: &[u8],
    operation: &str,
    kind: Kind,
    status: Status,
    phase: Phase,
    effect: Effect,
    staged: u64,
    durable: bool,
) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::response::Builder>();
    root.set_version(version);
    root.set_schema_sha256(digest);
    root.set_call_id(call_id);
    root.set_reference(reference);
    root.set_submission(submission);
    root.set_operation_id(operation);
    root.set_kind(kind);
    root.set_status(status);
    root.set_phase(phase);
    root.set_effect(effect);
    root.set_staged_bytes(staged);
    root.set_durable_content(durable);
    serialize::write_message_to_words(&message)
}
#[allow(clippy::too_many_arguments)] // Correlation fields vary independently.
fn raw_like(
    _request: &Request,
    version: u16,
    digest: &[u8],
    call_id: u64,
    reference: &[u8],
    submission: &[u8],
    operation: &str,
    kind: Kind,
) -> Vec<u8> {
    raw_response(
        version,
        digest,
        call_id,
        reference,
        submission,
        operation,
        kind,
        Status::Completed,
        Phase::Observed,
        Effect::OsSucceeded,
        0,
        false,
    )
}

#[test]
fn independent_reply_correlation_and_contract_rejection() {
    let req = request(Action::Execute);
    let good = response(&req);
    assert_eq!(req.decode_reply(&good.encode(&req).unwrap()).unwrap(), good);
    let digest = mutation::schema_digest();
    let bad_digest = [0x99; 32];
    for bytes in [
        raw_like(
            &req,
            wire::VERSION + 1,
            &digest,
            7,
            &req.reference,
            &req.submission,
            &req.operation_id,
            Kind::Execute,
        ),
        raw_like(
            &req,
            wire::VERSION,
            &bad_digest,
            7,
            &req.reference,
            &req.submission,
            &req.operation_id,
            Kind::Execute,
        ),
    ] {
        assert_eq!(req.decode_reply(&bytes), Err(Error::Contract));
    }
    for bytes in [
        raw_like(
            &req,
            wire::VERSION,
            &digest,
            8,
            &req.reference,
            &req.submission,
            &req.operation_id,
            Kind::Execute,
        ),
        raw_like(
            &req,
            wire::VERSION,
            &digest,
            7,
            &[0x33; 32],
            &req.submission,
            &req.operation_id,
            Kind::Execute,
        ),
        raw_like(
            &req,
            wire::VERSION,
            &digest,
            7,
            &req.reference,
            &[0x33; 32],
            &req.operation_id,
            Kind::Execute,
        ),
        raw_like(
            &req,
            wire::VERSION,
            &digest,
            7,
            &req.reference,
            &req.submission,
            "foreign",
            Kind::Execute,
        ),
        raw_like(
            &req,
            wire::VERSION,
            &digest,
            7,
            &req.reference,
            &req.submission,
            &req.operation_id,
            Kind::Query,
        ),
    ] {
        assert_eq!(req.decode_reply(&bytes), Err(Error::Correlation));
    }
    let wrong_lease = raw_like(
        &req,
        wire::VERSION,
        &digest,
        7,
        &[0; 32],
        &req.submission,
        &req.operation_id,
        Kind::Execute,
    );
    assert!(req.decode_reply(&wrong_lease).is_err());
    let mut trailing = good.encode(&req).unwrap();
    trailing.extend_from_slice(&[0; 8]);
    assert_eq!(req.decode_reply(&trailing), Err(Error::Invalid));
    assert_eq!(
        req.decode_reply(&vec![0; mutation::MAX_FRAME_BYTES + 1]),
        Err(Error::Limit)
    );
}

#[test]
fn response_status_phase_effect_matrix() {
    let req = request(Action::Execute);
    let mut r = response(&req);
    r.effect = Effect::Unspecified;
    assert_eq!(r.encode(&req), Err(Error::Invalid));
    r.status = Status::Denied;
    r.phase = Phase::None;
    r.effect = Effect::Unspecified;
    assert!(r.encode(&req).is_ok());
    r.durable_content = true;
    assert_eq!(r.encode(&req), Err(Error::Invalid));
    r.durable_content = false;
    r.status = Status::OutcomeUnknown;
    r.phase = Phase::OutcomeUnknown;
    assert!(r.encode(&req).is_ok());
    r.phase = Phase::Observed;
    assert_eq!(r.encode(&req), Err(Error::Invalid));
    let query = request(Action::Query);
    let mut q = response(&query);
    q.phase = Phase::OutcomeUnknown;
    q.effect = Effect::Unspecified;
    q.staged_bytes = wire::MAX_CONTENT_BYTES;
    q.durable_content = true;
    assert!(q.encode(&query).is_ok());
    q.phase = Phase::Absent;
    assert_eq!(q.encode(&query), Err(Error::Invalid));
    q.status = Status::OutcomeUnknown;
    q.phase = Phase::OutcomeUnknown;
    q.staged_bytes = 0;
    q.durable_content = false;
    assert!(q.encode(&query).is_ok());
    let chunk = request(Action::Chunk {
        offset: 4,
        bytes: vec![1, 2, 3],
    });
    let mut c = response(&chunk);
    c.phase = Phase::Prepared;
    c.effect = Effect::Unspecified;
    c.staged_bytes = 7;
    assert!(c.encode(&chunk).is_ok());
    c.staged_bytes = 6;
    assert_eq!(c.encode(&chunk), Err(Error::Invalid));
    c.status = Status::OutcomeUnknown;
    c.phase = Phase::None;
    c.staged_bytes = 0;
    assert_eq!(c.encode(&chunk), Err(Error::Invalid));
}

#[test]
fn request_version_digest_and_borrowed_bounds() {
    fn raw(version: u16, digest: &[u8], operation: &str) -> Vec<u8> {
        let mut message = Builder::new_default();
        let mut root = message.init_root::<wire::request::Builder>();
        root.set_version(version);
        root.set_schema_sha256(digest);
        root.set_call_id(7);
        root.set_reference(&[0x11; 32]);
        root.set_submission(&[0x22; 32]);
        root.set_operation_id(operation);
        root.set_deadline_ms(30000);
        root.set_execute(());
        serialize::write_message_to_words(&message)
    }
    let digest = mutation::schema_digest();
    assert!(Request::decode(&raw(wire::VERSION, &digest, "中文操作")).is_ok());
    assert_eq!(
        Request::decode(&raw(wire::VERSION + 1, &digest, "op")),
        Err(Error::Contract)
    );
    assert_eq!(
        Request::decode(&raw(wire::VERSION, &[0x99; 32], "op")),
        Err(Error::Contract)
    );
    assert_eq!(
        Request::decode(&raw(
            wire::VERSION,
            &digest,
            &"x".repeat(mutation::MAX_OPERATION_BYTES + 1)
        )),
        Err(Error::Limit)
    );
    assert_eq!(
        Request::decode(&raw(wire::VERSION, &digest, "bad\u{009f}name")),
        Err(Error::Invalid)
    );
}
