mod common;
use morrow_native_http_stream_wire::*;
#[test]
fn all_kinds_roundtrip_and_truncation_trailing_lengths_fail() {
    let cases = common::cases();
    assert_eq!(cases.len(), 24);
    for (name, f) in cases {
        let bytes = f.encode().unwrap();
        assert_eq!(Frame::decode(&bytes).unwrap(), f, "{name}");
        for end in 0..bytes.len() {
            assert!(
                Frame::decode(&bytes[..end]).is_err(),
                "{name} truncated {end}"
            );
        }
        let mut more = bytes.clone();
        more.extend([0; 8]);
        assert!(Frame::decode(&more).is_err());
        let n = more.len() - 4;
        more[..4].copy_from_slice(&(n as u32).to_le_bytes());
        assert!(Frame::decode(&more).is_err());
    }
    for n in [0u32, 1, 7, 9, 32769, u32::MAX] {
        assert!(payload_length(&n.to_le_bytes()).is_err());
    }
}
#[test]
fn bounds_union_and_false_completion_are_rejected() {
    let mut f = common::initial();
    f.kind = Kind::HttpPrepare;
    assert!(f.encode().is_err());
    f.payload = Payload::Prepare(common::prepare());
    assert!(f.encode().is_ok());
    if let Payload::Prepare(ref mut p) = f.payload {
        p.body_bytes = 32769;
    }
    assert!(f.encode().is_err());
    f.kind = Kind::BodyChunk;
    f.payload = Payload::Chunk(Chunk {
        offset: 0,
        bytes: vec![1; 8193],
    });
    assert!(f.encode().is_err());
    f.payload = Payload::Chunk(Chunk {
        offset: 65535,
        bytes: vec![1; 2],
    });
    assert!(f.encode().is_err());
    f.kind = Kind::State;
    let mut p = common::progress();
    p.intent = IntentPhase::Observed;
    f.payload = Payload::Progress(p.clone());
    assert!(f.encode().is_err());
    p.http_eof = true;
    p.response_material_stored = true;
    f.payload = Payload::Progress(p.clone());
    assert!(f.encode().is_ok());
    p.owner_released = true;
    f.payload = Payload::Progress(p);
    assert!(f.encode().is_err());
    let mut f = common::initial();
    f.schema_sha256[0] ^= 1;
    assert!(f.encode().is_err());
    let mut f = common::initial();
    f.capabilities = 1;
    assert!(f.encode().is_err());
}
#[test]
fn raw_request_hash_preserves_key_order_headers_and_identity_without_json_reencoding() {
    let mut p = common::prepare();
    p.headers.push(Header {
        name: "x-raw".into(),
        value: vec![255],
    });
    let a = request_digest(1, 2, b"op", 1, &p, b"{}").unwrap();
    p.response_limit_bytes = 1024;
    assert_ne!(a, request_digest(1, 2, b"op", 1, &p, b"{}").unwrap());
    p.response_limit_bytes = 65536;
    assert_ne!(a, request_digest(1, 3, b"op", 1, &p, b"{}").unwrap());
    p.headers.reverse();
    assert_ne!(a, request_digest(1, 2, b"op", 1, &p, b"{}").unwrap());
    assert!(request_digest(1, 2, b"op", 1, &p, b"[]").is_err());
    let body = b"{\"a\":1,\"b\":2}";
    p.body_bytes = body.len() as u32;
    p.body_sha256 = digest(body).to_vec();
    let a = request_digest(1, 2, b"op", 1, &p, body).unwrap();
    let body = b"{\"b\":2,\"a\":1}";
    p.body_sha256 = digest(body).to_vec();
    assert_ne!(a, request_digest(1, 2, b"op", 1, &p, body).unwrap());
}
#[test]
fn orthogonal_eof_observed_cancel_and_worker_owner_status_remain_expressible() {
    let mut f = common::initial();
    f.kind = Kind::HttpTerminal;
    let mut p = common::progress();
    p.intent = IntentPhase::Observed;
    p.http_eof = true;
    p.response_material_stored = true;
    // Transport finished and persisted, IPC delivery cancelled before owner release.
    p.network = NetworkPhase::Eof;
    p.revoke_persisted = true;
    p.revoke_applied = true;
    p.error_code = 24;
    f.payload = Payload::Progress(p);
    assert_eq!(Frame::decode(&f.encode().unwrap()).unwrap(), f);
    let mut f = common::initial();
    f.kind = Kind::HttpCredit;
    f.payload = Payload::Credit(Credit {
        consumed_offset: 10,
        parser_yielded_bytes: 4,
        drain_discarded_bytes: 6,
        cancel_discarded_bytes: 0,
        error_consumed_bytes: 0,
        window_bytes: 16384,
        max_chunk_bytes: 1024,
    });
    assert!(f.encode().is_ok());
    if let Payload::Credit(ref mut c) = f.payload {
        c.parser_yielded_bytes = 5;
    }
    assert!(f.encode().is_err());
}
