use morrow_plugin_sdk::channel::{
    Action, Error, Frame, Request, Response, Status, TranscriptDigest,
};

#[test]
fn discovery_is_bounded_owned_metadata() {
    use morrow_plugin_sdk::channel::{Budget,Directory,Endpoint,Kind};
    let endpoint=Endpoint{reference:[2;32],source_epoch:[3;32],kind:Kind::Events,budget:Budget::default()};
    let mut directory=Directory{scope_sha256:[4;32],channels:vec![endpoint.clone()]};
    let bytes=directory.encode().unwrap();assert_eq!(Directory::decode(&bytes).unwrap(),directory);
    directory.channels.push(endpoint.clone());assert_eq!(directory.encode(),Err(Error::Invalid));
    directory.channels=vec![endpoint;9];assert_eq!(directory.encode(),Err(Error::Limit));
    let mut bytes=bytes;bytes.push(0);assert_eq!(Directory::decode(&bytes),Err(Error::Invalid));
}
fn request() -> Request {
    Request {
        call_id: [1; 32],
        reference: [2; 32],
        source_epoch: [3; 32],
        action: Action::Receive {
            last_acked: 0,
            credit_bytes: 65536,
        },
    }
}
fn response(r: &Request) -> Response {
    Response {
        call_id: r.call_id,
        request_sha256: r.digest().unwrap(),
        reference: r.reference,
        source_epoch: r.source_epoch,
        status: Status::Frame,
        frame: Some(Frame {
            sequence: 1,
            source_epoch: r.source_epoch,
            bytes: vec![0xff; 65536],
            cursor: vec![0, 255],
        }),
        last_acked: 0,
        accepted_sequence: 0,
        resource_reclaimed: false,
    }
}
#[test]
fn max_payload_binary_roundtrip_and_exact_ack() {
    let r = request();
    let p = response(&r);
    let bytes = p.encode().unwrap();
    assert!(bytes.len() < 131072);
    let p = Response::decode(&bytes).unwrap();
    p.validate_for(&r).unwrap();
    let f = p.frame.unwrap();
    let ack = Request {
        action: Action::Ack {
            sequence: f.sequence,
            frame_sha256: f.digest().unwrap(),
            cursor: f.cursor,
        },
        ..r
    };
    assert_eq!(Request::decode(&ack.encode().unwrap()).unwrap(), ack);
}
#[test]
fn rejects_contract_tail_epoch_digest_credit_and_sequence() {
    let r = request();
    let mut encoded = r.encode().unwrap();
    encoded.push(0);
    assert_eq!(Request::decode(&encoded), Err(Error::Invalid));
    let mut p = response(&r);
    p.source_epoch = [4; 32];
    p.frame.as_mut().unwrap().source_epoch = [4; 32];
    assert_eq!(p.validate_for(&r), Err(Error::Correlation));
    let mut p = response(&r);
    p.request_sha256 = [5; 32];
    assert_eq!(p.validate_for(&r), Err(Error::Correlation));
    let mut p = response(&r);
    p.frame.as_mut().unwrap().sequence = 2;
    assert_eq!(p.validate_for(&r), Err(Error::Correlation));
    let smaller = Request {
        action: Action::Receive {
            last_acked: 0,
            credit_bytes: 1,
        },
        ..r
    };
    let mut p = response(&smaller);
    p.request_sha256 = smaller.digest().unwrap();
    assert_eq!(p.validate_for(&smaller), Err(Error::Correlation));
    let mut encoded = smaller.encode().unwrap();
    encoded[16] ^= 1;
    assert!(Request::decode(&encoded).is_err());
}
#[test]
fn accepted_is_send_admission_and_unknown_is_explicit() {
    let r = Request {
        action: Action::Send {
            sequence: 7,
            bytes: vec![1],
        },
        ..request()
    };
    let mut p = Response {
        call_id: r.call_id,
        request_sha256: r.digest().unwrap(),
        reference: r.reference,
        source_epoch: r.source_epoch,
        status: Status::Accepted,
        frame: None,
        last_acked: 0,
        accepted_sequence: 7,
        resource_reclaimed: false,
    };
    p.validate_for(&r).unwrap();
    p.accepted_sequence = 8;
    assert_eq!(p.validate_for(&r), Err(Error::Correlation));
    p.status = Status::Unknown;
    p.accepted_sequence = 0;
    p.resource_reclaimed = true;
    p.validate_for(&r).unwrap();
    p.status = Status::ClosingUnconfirmed;
    p.resource_reclaimed = false;
    p.validate_for(&r).unwrap();
}
#[test]
fn stream_digest_exceeds_legacy_value_limit_without_retaining_stream() {
    let mut a = TranscriptDigest::new();
    let mut b = TranscriptDigest::new();
    for _ in 0..5 {
        a.update(&vec![7; 32768]).unwrap();
        b.update(&vec![7; 16384]).unwrap();
        b.update(&vec![7; 16384]).unwrap();
    }
    assert_eq!(a.finish(), b.finish());
    assert_eq!(a.update(&vec![0; 65537]), Err(Error::Limit));
}
