#![cfg(feature = "qualification")]
mod common;
use common::*;
use morrow_agent_host_contract::{agent_host_capnp as wire, fake::*, *};

#[test]
fn u64_json_boundaries_are_exact_and_noncanonical_forms_rejected() {
    for v in [0, 1, 9007199254740993, 9223372036854775808, u64::MAX] {
        assert_eq!(parse_json_u64(&json_u64(v)), Ok(v));
    }
    for v in [
        "",
        "-1",
        "+1",
        "01",
        "1.0",
        "1e3",
        " 1",
        "18446744073709551616",
        "１",
    ] {
        assert!(parse_json_u64(v).is_err(), "{v}");
    }
}
#[test]
fn version_digest_defaults_malformed_and_trailing_bytes_fail_closed() {
    let mut msg = frame(1, "fixture-session", 1);
    msg.get_root::<wire::frame::Builder>().unwrap().set_major(2);
    assert_eq!(
        decode(&capnp::serialize::write_message_to_words(&msg))
            .err()
            .unwrap(),
        Error::UnsupportedVersion
    );
    msg.get_root::<wire::frame::Builder>().unwrap().set_major(1);
    msg.get_root::<wire::frame::Builder>()
        .unwrap()
        .set_revision(2);
    assert_eq!(
        decode(&capnp::serialize::write_message_to_words(&msg))
            .err()
            .unwrap(),
        Error::UnsupportedVersion
    );
    msg.get_root::<wire::frame::Builder>()
        .unwrap()
        .set_revision(1);
    msg.get_root::<wire::frame::Builder>()
        .unwrap()
        .set_schema_digest(&[0; 32]);
    assert_eq!(
        decode(&capnp::serialize::write_message_to_words(&msg))
            .err()
            .unwrap(),
        Error::SchemaMismatch
    );
    for bytes in [
        vec![],
        vec![0; 8],
        vec![255; 32],
        vec![0; MAX_FRAME_BYTES + 1],
    ] {
        assert!(decode(&bytes).is_err());
    }
    let mut bytes = hello(&FixtureIdentity::default(), 1);
    bytes.push(0);
    assert!(decode(&bytes).is_err());
    assert!(encode(&frame(0, "fixture-session", 1)).is_err());
    assert!(encode(&frame(1, "", 1)).is_err());
    assert!(encode(&frame(1, "fixture-session", 0)).is_err());
}
#[test]
fn trusted_fixture_binding_cannot_be_created_by_self_report() {
    let mut host = fixture();
    assert_eq!(
        error(&exchange(&mut host, &open(2, b""))),
        ErrorCode::Denied
    );
    for identity in [
        FixtureIdentity {
            plugin_id: "forged".into(),
            ..FixtureIdentity::default()
        },
        FixtureIdentity {
            artifact_digest: [0; 32],
            ..FixtureIdentity::default()
        },
        FixtureIdentity {
            account_epoch: 2,
            ..FixtureIdentity::default()
        },
    ] {
        assert_eq!(
            error(&exchange(&mut host, &hello(&identity, 1))),
            ErrorCode::Denied
        );
    }
    attach(&mut host);
    let old = hello(
        &FixtureIdentity {
            instance_epoch: 2,
            ..FixtureIdentity::default()
        },
        2,
    );
    assert_eq!(error(&exchange(&mut host, &old)), ErrorCode::StaleEpoch);
}
#[test]
fn stream_requires_fixed_request_delivers_bounded_prefix_before_eof_and_never_resends() {
    let mut host = fixture();
    attach(&mut host);
    exchange(&mut host, &open(2, b"abc"));
    assert_eq!(
        error(&exchange(&mut host, &read(3, 0, 5))),
        ErrorCode::Conflict
    );
    assert_eq!(
        error(&exchange(
            &mut host,
            &stream(4, |mut s| s.set_commit_request(()))
        )),
        ErrorCode::Integrity
    );
    exchange(&mut host, &chunk(5, 0, b"abc"));
    exchange(&mut host, &stream(6, |mut s| s.set_commit_request(())));
    assert_eq!(
        error(&exchange(&mut host, &chunk(7, 3, b"d"))),
        ErrorCode::Conflict
    );
    assert_eq!(
        error(&exchange(
            &mut host,
            &stream(8, |mut s| s.set_commit_request(()))
        )),
        ErrorCode::Conflict
    );
    let first = exchange(&mut host, &read(9, 0, 5));
    assert_eq!(stream_reply(&first).get_bytes().unwrap(), b"first");
    assert_eq!(
        stream_reply(&first).get_state().unwrap(),
        StreamState::Streaming
    );
    let tail = exchange(&mut host, &read(10, 5, 4096));
    assert_eq!(stream_reply(&tail).get_bytes().unwrap(), b"-second");
    assert_eq!(stream_reply(&tail).get_state().unwrap(), StreamState::Eof);
    assert_eq!(stream_reply(&tail).get_transport_sequence(), 2);
    assert_eq!(
        error(&exchange(&mut host, &read(11, 12, 4097))),
        ErrorCode::Limit
    );
    assert_eq!(
        error(&exchange(&mut host, &open(12, b"abc"))),
        ErrorCode::Conflict
    );
}
#[test]
fn stream_deadline_cancel_close_and_empty_body_are_explicit() {
    let mut host = fixture();
    attach(&mut host);
    exchange(&mut host, &open(2, b""));
    exchange(&mut host, &stream(3, |mut s| s.set_commit_request(())));
    host.set_clock_ms(1000).unwrap();
    assert!(host.set_clock_ms(999).is_err());
    assert_eq!(
        error(&exchange(&mut host, &read(4, 0, 1))),
        ErrorCode::Denied
    );
    let cancelled = exchange(&mut host, &stream(5, |mut s| s.set_cancel(())));
    assert_eq!(
        stream_reply(&cancelled).get_state().unwrap(),
        StreamState::Cancelled
    );
    let closed = exchange(&mut host, &stream(6, |mut s| s.set_close(())));
    assert_eq!(
        stream_reply(&closed).get_state().unwrap(),
        StreamState::Closed
    );
}
#[test]
fn append_exact_retry_returns_original_tail_changed_bytes_and_stale_writer_fail() {
    let mut host = fixture();
    attach(&mut host);
    let a = append(2, 1, 9007199254740993, 0, b"one");
    assert_eq!(
        event_reply(&exchange(&mut host, &a)).get_durable_sequence(),
        1
    );
    exchange(&mut host, &append(3, 1, u64::MAX, 1, b"two"));
    let retry = exchange(&mut host, &a);
    assert_eq!(event_reply(&retry).get_durable_sequence(), 1);
    assert!(event_reply(&retry).get_replayed());
    assert_eq!(
        error(&exchange(
            &mut host,
            &append(2, 1, 9007199254740993, 0, b"changed")
        )),
        ErrorCode::Conflict
    );
    assert_eq!(
        error(&exchange(&mut host, &append(4, 1, 10, 0, b"old-tail"))),
        ErrorCode::Conflict
    );
    let read = request(5, |f| {
        let mut e = f.init_event();
        e.set_writer_epoch(1);
        e.set_read_after(0);
    });
    assert_eq!(
        event_reply(&exchange(&mut host, &read))
            .get_events()
            .unwrap()
            .len(),
        2
    );
    host.handoff_writer().unwrap();
    assert_eq!(error(&exchange(&mut host, &a)), ErrorCode::StaleEpoch);
    assert_eq!(
        event_reply(&exchange(&mut host, &append(6, 2, 1, 2, b"new"))).get_durable_sequence(),
        3
    );
}
#[test]
fn event_limit_is_atomic_and_reply_is_bounded() {
    let mut host = fixture();
    attach(&mut host);
    assert_eq!(
        error(&exchange(&mut host, &append(2, 1, 1, 0, &vec![7; 4097]))),
        ErrorCode::Limit
    );
    for n in 0..MAX_EVENTS {
        exchange(
            &mut host,
            &append(n as u64 + 3, 1, n as u64 + 1, n as u64, b""),
        );
    }
    assert_eq!(
        error(&exchange(&mut host, &append(500, 1, 500, 128, b"extra"))),
        ErrorCode::Limit
    );
    let read = request(501, |f| {
        let mut e = f.init_event();
        e.set_writer_epoch(1);
        e.set_read_after(0);
    });
    let msg = exchange(&mut host, &read);
    assert_eq!(event_reply(&msg).get_events().unwrap().len(), 8);
    assert_eq!(event_reply(&msg).get_durable_sequence(), 8);
}

#[test]
fn maximum_event_batch_remains_readable_by_consumer_after_validation() {
    let mut host = fixture();
    attach(&mut host);
    let payload = vec![42; MAX_CHUNK_BYTES];
    for n in 0..MAX_BATCH_EVENTS {
        exchange(
            &mut host,
            &append(n as u64 + 2, 1, n as u64 + 1, n as u64, &payload),
        );
    }
    let bytes = request(99, |f| {
        let mut e = f.init_event();
        e.set_writer_epoch(1);
        e.set_read_after(0);
    });
    let response = exchange(&mut host, &bytes);
    for event in event_reply(&response).get_events().unwrap().iter() {
        assert_eq!(event.get_payload().unwrap(), payload.as_slice());
        assert_eq!(event.get_source_digest().unwrap(), digest(&payload));
        assert_eq!(event.get_turn_id().unwrap().to_str().unwrap(), "turn-1");
        assert_eq!(event.get_item_id().unwrap().to_str().unwrap(), "item-1");
        assert_eq!(event.get_part_id().unwrap().to_str().unwrap(), "part-1");
        assert_eq!(
            event.get_attempt_id().unwrap().to_str().unwrap(),
            "attempt-1"
        );
        assert_eq!(
            event.get_semantic_kind().unwrap().to_str().unwrap(),
            "opaque.delta"
        );
    }
}
#[test]
fn proposal_is_not_authority_claim_once_unknown_report_stays_unknown() {
    let mut host = fixture();
    attach(&mut host);
    let proposal = propose(2);
    assert_eq!(
        tool_reply(&exchange(&mut host, &proposal))
            .get_state()
            .unwrap(),
        ToolState::Proposed
    );
    assert_eq!(
        error(&exchange(&mut host, &claim(3, &[0; 32]))),
        ErrorCode::Denied
    );
    assert_eq!(
        error(&exchange(&mut host, &report(4, ToolState::Reported))),
        ErrorCode::Denied
    );
    let permit = host.approve("operation-1").unwrap();
    assert!(tool_reply(&exchange(&mut host, &claim(5, &permit))).get_execute());
    assert!(!tool_reply(&exchange(&mut host, &claim(6, &permit))).get_execute());
    let unknown = report(7, ToolState::Unknown);
    exchange(&mut host, &unknown);
    assert_eq!(
        tool_reply(&exchange(&mut host, &unknown))
            .get_state()
            .unwrap(),
        ToolState::Unknown
    );
    assert_eq!(
        error(&exchange(&mut host, &report(8, ToolState::Reported))),
        ErrorCode::Conflict
    );
    assert!(!tool_reply(&exchange(&mut host, &claim(9, &permit))).get_execute());
    assert_eq!(
        error(&exchange(&mut host, &propose(99))),
        ErrorCode::Conflict
    );
}
#[test]
fn drain_revokes_new_work_keeps_inspection_and_never_claims_observed_exit() {
    let mut host = fixture();
    attach(&mut host);
    exchange(&mut host, &propose(2));
    let permit = host.approve("operation-1").unwrap();
    let drain = request(3, |f| f.init_native_session().set_drain(()));
    exchange(&mut host, &drain);
    assert_eq!(
        error(&exchange(&mut host, &claim(4, &permit))),
        ErrorCode::Denied
    );
    assert_eq!(
        error(&exchange(&mut host, &hello(&FixtureIdentity::default(), 5))),
        ErrorCode::Denied
    );
    assert_eq!(
        error(&exchange(
            &mut host,
            &request(6, |f| f.init_native_session().set_observe_exit(()))
        )),
        ErrorCode::Unavailable
    );
    let inspect = request(7, |f| {
        let mut t = f.init_tool();
        t.set_operation_id("operation-1");
        t.set_inspect(());
    });
    assert_eq!(
        tool_reply(&exchange(&mut host, &inspect))
            .get_state()
            .unwrap(),
        ToolState::Approved
    );
}
#[test]
fn consumer_rejects_wrong_correlation_wrong_direction_and_no_backend() {
    struct Bad {
        response: Vec<u8>,
    }
    impl Transport for Bad {
        fn exchange(&mut self, _: &[u8]) -> Result<Vec<u8>, Error> {
            Ok(self.response.clone())
        }
    }
    let request = hello(&FixtureIdentity::default(), 1);
    assert_eq!(
        exchange_checked(
            &mut Bad {
                response: request.clone()
            },
            &request
        )
        .err()
        .unwrap(),
        Error::WrongDirection
    );
    let mut host = fixture();
    let mut msg = frame(99, "fixture-session", 1);
    let mut r = msg.get_root::<wire::frame::Builder>().unwrap().init_reply();
    r.set_qualification_only(true);
    let mut n = r.init_native_session();
    n.set_qualification_only(true);
    n.set_capability_bits(15);
    assert_eq!(
        exchange_checked(
            &mut Bad {
                response: encode(&msg).unwrap()
            },
            &request
        )
        .err()
        .unwrap(),
        Error::Invalid
    );
    let reply = host.exchange(&request).unwrap();
    assert_eq!(host.exchange(&reply).err().unwrap(), Error::WrongDirection);
    struct Offline;
    impl Transport for Offline {
        fn exchange(&mut self, _: &[u8]) -> Result<Vec<u8>, Error> {
            Err(Error::Invalid)
        }
    }
    assert!(exchange_checked(&mut Offline, &request).is_err());
}

#[test]
fn stream_last_read_is_replayable_inspection_shows_cursor_and_cancel_blocks_replay() {
    let mut host = fixture();
    attach(&mut host);
    exchange(&mut host, &open(2, b""));
    exchange(&mut host, &stream(3, |mut s| s.set_commit_request(())));
    let first = exchange(&mut host, &read(4, 0, 5));
    let retry = exchange(&mut host, &read(5, 0, 5));
    assert_eq!(
        stream_reply(&first).get_bytes().unwrap(),
        stream_reply(&retry).get_bytes().unwrap()
    );
    assert_eq!(stream_reply(&retry).get_transport_sequence(), 1);
    let inspect = exchange(&mut host, &stream(6, |mut s| s.set_inspect(())));
    assert_eq!(stream_reply(&inspect).get_offset(), 5);
    assert_eq!(
        error(&exchange(&mut host, &read(7, 0, 4))),
        ErrorCode::Conflict
    );
    exchange(&mut host, &read(8, 5, 7));
    assert_eq!(
        error(&exchange(&mut host, &read(9, 0, 5))),
        ErrorCode::Conflict
    );
    let mut active = fixture();
    attach(&mut active);
    exchange(&mut active, &open(2, b""));
    exchange(&mut active, &stream(3, |mut s| s.set_commit_request(())));
    exchange(&mut active, &read(4, 0, 5));
    exchange(&mut active, &stream(5, |mut s| s.set_cancel(())));
    assert_eq!(
        error(&exchange(&mut active, &read(6, 0, 5))),
        ErrorCode::Conflict
    );
}
#[test]
fn exact_resource_reference_rejects_changed_revision_length_and_digest() {
    for field in 0..3 {
        let mut host = fixture();
        attach(&mut host);
        let forged = request(2, |f| {
            let mut s = f.init_stream();
            s.set_attempt_id("attempt-1");
            let mut o = s.init_open();
            o.set_request_bytes(0);
            o.set_request_digest(&digest(b""));
            let mut r = o.reborrow().init_destination();
            resource(r.reborrow(), "destination");
            match field {
                0 => r.set_revision(1),
                1 => r.set_byte_length(1),
                _ => r.set_sha256(&[0; 32]),
            }
        });
        assert_eq!(error(&exchange(&mut host, &forged)), ErrorCode::Denied);
    }
}

// Single-segment Cap'n Proto pointer navigation for adversarial wire corruption.
// Derived from the encoded pointer, not a byte search that could mutate a payload.
fn struct_location(bytes: &[u8], pointer: usize) -> (usize, usize) {
    assert_eq!(&bytes[0..4], &[0, 0, 0, 0]);
    let word = u64::from_le_bytes(bytes[pointer..pointer + 8].try_into().unwrap());
    assert_eq!(word & 3, 0);
    let offset = (word as u32 as i32) >> 2;
    let data = (pointer as isize + 8 + offset as isize * 8) as usize;
    (data, data + ((word >> 32) & 0xffff) as usize * 8)
}
#[test]
fn unknown_reply_enums_union_tags_defaults_and_wrong_family_are_rejected() {
    struct Return(Vec<u8>);
    impl Transport for Return {
        fn exchange(&mut self, _: &[u8]) -> Result<Vec<u8>, Error> {
            Ok(self.0.clone())
        }
    }
    let input = hello(&FixtureIdentity::default(), 1);
    let mut empty = frame(1, "fixture-session", 1);
    empty
        .get_root::<wire::frame::Builder>()
        .unwrap()
        .init_reply();
    assert!(
        exchange_checked(
            &mut Return(capnp::serialize::write_message_to_words(&empty)),
            &input
        )
        .is_err()
    );
    let mut msg = frame(1, "fixture-session", 1);
    let mut r = msg.get_root::<wire::frame::Builder>().unwrap().init_reply();
    r.set_qualification_only(true);
    let mut s = r.init_stream();
    s.set_state(StreamState::Committed);
    s.set_request_fixed(true);
    let valid = encode(&msg).unwrap();
    assert_eq!(
        exchange_checked(&mut Return(valid.clone()), &input)
            .err()
            .unwrap(),
        Error::WrongDirection
    );
    let (frame_data, frame_pointers) = struct_location(&valid, 8);
    let (reply_data, reply_pointers) = struct_location(&valid, frame_pointers + 16);
    let (state_data, _) = struct_location(&valid, reply_pointers);
    for location in [frame_data + 4, reply_data + 2, state_data] {
        let mut bad = valid.clone();
        bad[location..location + 2].copy_from_slice(&65535u16.to_le_bytes());
        assert_eq!(decode(&bad).err().unwrap(), Error::Invalid);
    }
    let mut oversized = frame(1, "fixture-session", 1);
    let mut r = oversized
        .get_root::<wire::frame::Builder>()
        .unwrap()
        .init_reply();
    r.set_qualification_only(true);
    let mut s = r.init_stream();
    s.set_state(StreamState::Streaming);
    s.set_request_fixed(true);
    s.set_bytes(&vec![0; 4097]);
    assert!(decode(&capnp::serialize::write_message_to_words(&oversized)).is_err());
}
