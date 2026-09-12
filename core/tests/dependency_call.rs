use capnp::{message::Builder, serialize};
use morrow_core::{
    dependency_call::{MAX_FRAME_BYTES, MAX_PAYLOAD_BYTES, Request, VERSION, schema_digest},
    dependency_call_capnp as wire,
};
use sha2::{Digest, Sha256};
fn frame(call: &str, slot: &str, data: &[u8]) -> Builder<capnp::message::HeapAllocator> {
    let mut m = Builder::new_default();
    let mut r = m.init_root::<wire::message::Builder>();
    r.set_version(VERSION);
    r.set_schema_digest(&schema_digest());
    let mut r = r.init_request();
    r.set_call_id(call);
    r.set_slot(slot);
    r.set_input(data);
    m
}
fn response(call: &str, hash: &[u8], kind: &str, data: &[u8]) -> Vec<u8> {
    let mut m = Builder::new_default();
    let mut r = m.init_root::<wire::message::Builder>();
    r.set_version(VERSION);
    r.set_schema_digest(&schema_digest());
    let mut r = r.init_response();
    r.set_call_id(call);
    r.set_request_sha256(hash);
    r.set_output_type(kind);
    r.set_output(data);
    serialize::write_message_to_words(&m)
}
#[test]
fn schema_snapshots_share_lf_normalized_digest() {
    let source = String::from_utf8(include_bytes!("../schemas/dependency_call.capnp").to_vec())
        .unwrap()
        .replace("\r\n", "\n");
    let sdk = String::from_utf8(
        include_bytes!("../../sdk/rust/contracts/dependency_call.capnp").to_vec(),
    )
    .unwrap()
    .replace("\r\n", "\n");
    assert_eq!(source, sdk);
    assert_eq!(
        schema_digest(),
        <[u8; 32]>::from(Sha256::digest(source.as_bytes()))
    );
    assert_eq!(VERSION, 1);
}
#[test]
fn owned_roundtrip_preserves_binary_and_distinguishes_empty_success_from_failure() {
    let mut input = b"abc\0\xff".to_vec();
    let req = Request::new("调用🌈", "formatter", &input).unwrap();
    input.fill(0);
    assert_eq!(req.input(), b"abc\0\xff");
    assert_eq!(req.call_id(), "调用🌈");
    assert_eq!(req.slot(), "formatter");
    assert_eq!(Request::decode(req.bytes()).unwrap(), req);
    for data in [b"".as_slice(), b"out\0\xff".as_slice()] {
        let bytes = req.encode_response("bytes.v1", data).unwrap();
        let output = req.verify_response(&bytes).unwrap();
        assert_eq!(output.output_type, "bytes.v1");
        assert_eq!(output.bytes, data);
        assert!(Request::decode(&bytes).is_err());
    }
    assert!(req.verify_response(req.bytes()).is_err());
    assert!(req.verify_response(&[]).is_err());
}
#[test]
fn payload_and_identity_limits_apply_at_both_encoding_and_decoding() {
    let max = vec![0xa5; MAX_PAYLOAD_BYTES];
    let req = Request::new(&"x".repeat(256), &"界".repeat(85), &max).unwrap();
    assert_eq!(Request::decode(req.bytes()).unwrap(), req);
    let bytes = req.encode_response(&"y".repeat(256), &max).unwrap();
    assert_eq!(req.verify_response(&bytes).unwrap().bytes, max);
    assert!(req.bytes().len() <= MAX_FRAME_BYTES);
    assert!(bytes.len() <= MAX_FRAME_BYTES);
    for input in [vec![], vec![0; MAX_PAYLOAD_BYTES + 1]] {
        assert!(Request::new("call", "slot", &input).is_err());
        assert!(
            Request::decode(&serialize::write_message_to_words(&frame(
                "call", "slot", &input
            )))
            .is_err()
        );
    }
    let hash = Sha256::digest(req.bytes());
    let too_big = vec![0; MAX_PAYLOAD_BYTES + 1];
    assert!(req.encode_response("bytes", &too_big).is_err());
    assert!(
        req.verify_response(&response(req.call_id(), &hash, "bytes", &too_big))
            .is_err()
    );
    for id in [
        "".into(),
        "x".repeat(257),
        "界".repeat(86),
        "bad/name".into(),
        "bad\\name".into(),
        "bad:name".into(),
        "bad\0name".into(),
        "bad\nname".into(),
    ] {
        assert!(Request::new(&id, "slot", b"x").is_err());
        assert!(Request::new("call", &id, b"x").is_err());
        for (call, slot) in [(id.as_str(), "slot"), ("call", id.as_str())] {
            assert!(
                Request::decode(&serialize::write_message_to_words(&frame(call, slot, b"x")))
                    .is_err()
            );
        }
        assert!(req.encode_response(&id, b"").is_err());
        assert!(
            req.verify_response(&response(req.call_id(), &hash, &id, b""))
                .is_err()
        );
    }
}
#[test]
fn correlation_binds_call_slot_input_and_entire_original_frame() {
    let req = Request::new("call", "slot", b"one").unwrap();
    let reply = req.encode_response("bytes", b"done").unwrap();
    for other in [
        Request::new("other", "slot", b"one").unwrap(),
        Request::new("call", "other", b"one").unwrap(),
        Request::new("call", "slot", b"two").unwrap(),
    ] {
        assert!(other.verify_response(&reply).is_err());
    }
    // Unused word within a declared segment is legal framing but must change request identity.
    let mut alternate = req.bytes().to_vec();
    assert_eq!(&alternate[..4], &[0; 4]);
    let count = u32::from_le_bytes(alternate[4..8].try_into().unwrap());
    alternate[4..8].copy_from_slice(&(count + 1).to_le_bytes());
    alternate.extend([0; 8]);
    let decoded = Request::decode(&alternate).unwrap();
    assert_eq!(decoded.bytes(), alternate);
    assert_eq!(decoded.call_id(), req.call_id());
    assert_eq!(decoded.slot(), req.slot());
    assert_eq!(decoded.input(), req.input());
    assert!(decoded.verify_response(&reply).is_err());
    let bound = decoded.encode_response("bytes", b"done").unwrap();
    assert!(req.verify_response(&bound).is_err());
    assert!(decoded.verify_response(&bound).is_ok());
    let hash = Sha256::digest(req.bytes());
    assert!(
        req.verify_response(&response("wrong", &hash, "bytes", b"done"))
            .is_err()
    );
    for hash in [vec![], vec![0; 31], vec![0; 32], vec![0; 33]] {
        assert!(
            req.verify_response(&response("call", &hash, "bytes", b"done"))
                .is_err()
        );
    }
}
#[test]
fn incorrect_version_digest_and_union_tag_are_rejected() {
    let req = Request::new("call", "slot", b"x").unwrap();
    for reply in [false, true] {
        for mode in 0..7 {
            let mut m = frame("call", "slot", b"x");
            if reply {
                let mut r = m
                    .get_root::<wire::message::Builder>()
                    .unwrap()
                    .init_response();
                r.set_call_id("call");
                r.set_request_sha256(&Sha256::digest(req.bytes()));
                r.set_output_type("bytes");
                r.set_output(b"x");
            }
            let mut r = m.get_root::<wire::message::Builder>().unwrap();
            match mode {
                0 => r.set_version(0),
                1 => r.set_version(VERSION + 1),
                2 => r.set_version(u16::MAX),
                3 => r.set_schema_digest(&[]),
                4 => r.set_schema_digest(&[0; 31]),
                5 => r.set_schema_digest(&[0; 32]),
                _ => r.set_schema_digest(&[0; 33]),
            }
            let bytes = serialize::write_message_to_words(&m);
            assert!(Request::decode(&bytes).is_err());
            assert!(req.verify_response(&bytes).is_err());
        }
    }
    let mut unknown = req.bytes().to_vec();
    unknown[18..20].copy_from_slice(&99u16.to_le_bytes());
    assert!(Request::decode(&unknown).is_err());
    assert!(req.verify_response(&unknown).is_err());
}
#[test]
fn malformed_utf8_in_all_text_fields_is_rejected() {
    let req = Request::new("call-unique", "slot-unique", b"x").unwrap();
    for needle in ["call-unique", "slot-unique"] {
        let mut bytes = req.bytes().to_vec();
        let offset = bytes
            .windows(needle.len())
            .position(|v| v == needle.as_bytes())
            .unwrap();
        bytes[offset] = 255;
        assert!(Request::decode(&bytes).is_err());
    }
    for needle in ["call-unique", "type-unique"] {
        let mut bytes = req.encode_response("type-unique", b"x").unwrap();
        let offset = bytes
            .windows(needle.len())
            .position(|v| v == needle.as_bytes())
            .unwrap();
        bytes[offset] = 255;
        assert!(req.verify_response(&bytes).is_err());
    }
}
#[test]
fn truncated_tailed_joined_and_oversized_frames_fail_without_panics() {
    let req = Request::new("call", "slot", b"x").unwrap();
    for bytes in [
        req.bytes().to_vec(),
        req.encode_response("bytes", b"x").unwrap(),
    ] {
        for end in 0..bytes.len() {
            assert!(Request::decode(&bytes[..end]).is_err());
            assert!(req.verify_response(&bytes[..end]).is_err());
        }
        for tail in [vec![0], vec![0; 8], bytes.clone()] {
            let mut joined = bytes.clone();
            joined.extend(tail);
            assert!(Request::decode(&joined).is_err());
            assert!(req.verify_response(&joined).is_err());
        }
    }
    for bytes in [
        vec![0; MAX_FRAME_BYTES + 1],
        vec![255; 8],
        vec![0; 8],
        vec![255; 256],
        vec![0; MAX_FRAME_BYTES],
    ] {
        assert!(Request::decode(&bytes).is_err());
        assert!(req.verify_response(&bytes).is_err());
    }
}
#[test]
fn unaligned_transport_buffers_preserve_complete_frame_correlation() {
    let req = Request::new("call", "slot", b"x").unwrap();
    let reply = req.encode_response("bytes", b"out").unwrap();
    for pad in 1..8 {
        let mut bytes = vec![0; pad];
        bytes.extend(req.bytes());
        let decoded = Request::decode(&bytes[pad..]).unwrap();
        assert_eq!(decoded, req);
        let mut bytes = vec![0; pad];
        bytes.extend(&reply);
        assert_eq!(
            decoded.verify_response(&bytes[pad..]).unwrap().bytes,
            b"out"
        );
    }
}
// Manual Capnp words provide a fixture independent of the generated encoder.
fn raw_request(extra: usize, cycle: bool) -> Vec<u8> {
    let schema = 4 + extra;
    let request = schema + 4;
    let call = request + 3;
    let slot = call + 1;
    let payload = slot + 1;
    let mut words = vec![0u64; payload + MAX_PAYLOAD_BYTES / 8];
    words[0] = (1 << 32) | (((2 + extra) as u64) << 48);
    words[1] = VERSION as u64;
    let list = |from: usize, to: usize, count: usize| {
        1 | (((to - from - 1) as u64) << 2) | (2 << 32) | ((count as u64) << 35)
    };
    words[2] = list(2, schema, 32);
    words[3] = (((request - 4) as u64) << 2) | (3 << 48);
    words[request] = list(request, call, 2);
    words[request + 1] = list(request + 1, slot, 2);
    words[request + 2] = list(request + 2, payload, MAX_PAYLOAD_BYTES);
    for i in 0..extra {
        words[4 + i] = if cycle {
            0xffff_fffc | (1 << 48)
        } else {
            list(4 + i, payload, MAX_PAYLOAD_BYTES)
        };
    }
    words[call] = b'c' as u64;
    words[slot] = b's' as u64;
    let mut bytes = vec![0; 4];
    bytes.extend((words.len() as u32).to_le_bytes());
    for word in words {
        bytes.extend(word.to_le_bytes());
    }
    bytes[8 + schema * 8..8 + schema * 8 + 32].copy_from_slice(&schema_digest());
    bytes
}
#[test]
fn bounded_traversal_rejects_unknown_alias_amplification_and_nested_cycles() {
    let valid = raw_request(0, false);
    let req = Request::decode(&valid).unwrap();
    assert_eq!(req.call_id(), "c");
    assert_eq!(req.slot(), "s");
    assert_eq!(req.input(), vec![0; MAX_PAYLOAD_BYTES]);
    assert!(Request::decode(&raw_request(3, false)).is_err());
    assert!(Request::decode(&raw_request(1, true)).is_err());
}
