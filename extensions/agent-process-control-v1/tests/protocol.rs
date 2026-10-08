use morrow_agent_process_control_v1::*;
fn request(action: Action) -> Request {
    Request::new("request-1", [7; 32], 1, action).unwrap()
}
fn query() -> ReadQuery {
    ReadQuery {
        after_seq: 0,
        max_bytes: 32,
        max_events: 16,
        wait_ms: 0,
    }
}
#[test]
fn roundtrip_all_request_variants_and_bounds() {
    for action in [
        Action::Discover,
        Action::Read(query()),
        Action::Events(query()),
        Action::Write(vec![0, 255]),
        Action::CloseInput,
        Action::Interrupt,
        Action::Terminate,
        Action::Resize { rows: 24, cols: 80 },
    ] {
        let value = request(action);
        assert_eq!(Request::decode(&value.encode().unwrap()).unwrap(), value)
    }
    assert_eq!(
        Request::new("bad/id", [1; 32], 1, Action::Discover),
        Err(Error::Invalid)
    );
    assert_eq!(request(Action::Discover).encode().unwrap().len() % 8, 0);
    assert_eq!(
        ReadQuery {
            wait_ms: 1001,
            ..query()
        }
        .validate(),
        Err(Error::Limit)
    );
    assert_eq!(
        ReadQuery {
            max_events: 17,
            ..query()
        }
        .validate(),
        Err(Error::Limit)
    );
    assert_eq!(
        Request::new("x", [1; 32], 1, Action::Write(vec![0; MAX_BODY_BYTES + 1])),
        Err(Error::Limit)
    );
}
#[test]
fn rejects_trailing_truncated_foreign_schema_and_noncanonical_headers() {
    let value = request(Action::Discover);
    let bytes = value.encode().unwrap();
    let mut trailing = bytes.clone();
    trailing.extend([0; 8]);
    assert_eq!(Request::decode(&trailing), Err(Error::Invalid));
    for len in 0..bytes.len() {
        assert!(Request::decode(&bytes[..len]).is_err())
    }
    let mut wrong = bytes.clone();
    let offset = wrong.windows(32).position(|v| v == SCHEMA_DIGEST).unwrap();
    wrong[offset] ^= 1;
    assert_eq!(Request::decode(&wrong), Err(Error::Contract));
    let mut version = bytes.clone();
    version[16] = 2;
    assert_eq!(Request::decode(&version), Err(Error::Contract));
    let mut union = bytes.clone();
    union[18..20].copy_from_slice(&65535u16.to_le_bytes());
    assert_eq!(Request::decode(&union), Err(Error::Invalid));
    let mut layout = bytes.clone();
    layout[12..14].copy_from_slice(&65535u16.to_le_bytes());
    assert_eq!(Request::decode(&layout), Err(Error::Invalid));
    let mut padded = bytes.clone();
    padded[4..8].copy_from_slice(&((bytes.len() / 8) as u32).to_le_bytes());
    padded.extend([0; 8]);
    assert_eq!(Request::decode(&padded), Err(Error::Invalid));
}
#[test]
fn reply_correlation_and_action_are_checked() {
    let value = request(Action::Write(vec![1]));
    let reply = Reply::new(&value, ReplyBody::Accepted).unwrap();
    assert_eq!(
        Reply::decode_for(&value, &reply.encode().unwrap()).unwrap(),
        reply
    );
    let other = Request::new("request-2", value.handle, 1, value.action.clone()).unwrap();
    assert_eq!(
        Reply::decode_for(&other, &reply.encode().unwrap()),
        Err(Error::Correlation)
    );
    let bad = Reply::new(&value, ReplyBody::Capabilities(Capabilities::default())).unwrap();
    assert_eq!(
        Reply::decode_for(&value, &bad.encode().unwrap()),
        Err(Error::Correlation)
    );
    for error in [
        Error::Invalid,
        Error::Contract,
        Error::Limit,
        Error::Correlation,
        Error::Denied,
        Error::Conflict,
        Error::NotFound,
        Error::Unknown,
        Error::Unsupported,
        Error::Closed,
    ] {
        let rejected = Reply::new(&value, ReplyBody::Rejected(error)).unwrap();
        assert_eq!(
            Reply::decode_for(&value, &rejected.encode().unwrap()).unwrap(),
            rejected
        )
    }
}
#[test]
fn output_page_preserves_exit_eof_and_unsequenced_failure() {
    let value = request(Action::Read(query()));
    let page = OutputPage {
        events: vec![
            ProcessEvent {
                seq: 1,
                kind: EventKind::Output {
                    stream: OutputStream::Stderr,
                    chunk: vec![0, 255],
                },
            },
            ProcessEvent {
                seq: 2,
                kind: EventKind::Exited {
                    exit_code: 3,
                    sandbox_denied: None,
                },
            },
        ],
        next_seq: 2,
        floor_seq: 1,
        gap: false,
        exited: true,
        exit_code: Some(3),
        closed: false,
        failure: Some("transport disconnected".into()),
    };
    let reply = Reply::new(&value, ReplyBody::Page(page.clone())).unwrap();
    assert_eq!(
        Reply::decode_for(&value, &reply.encode().unwrap()).unwrap(),
        reply
    );
    assert!(!page.closed);
    let mut gap = page.clone();
    gap.floor_seq = 2;
    gap.events.remove(0);
    gap.gap = true;
    assert!(gap.validate_for(query()).is_ok());
    gap.gap = false;
    assert_eq!(gap.validate_for(query()), Err(Error::Invalid));
}
#[test]
fn rejects_page_overflow_sequence_reorder_and_output_after_closed() {
    let event = |seq| ProcessEvent {
        seq,
        kind: EventKind::Output {
            stream: OutputStream::Stdout,
            chunk: vec![1],
        },
    };
    let mut page = OutputPage {
        events: (1..=17).map(event).collect(),
        next_seq: 17,
        floor_seq: 1,
        gap: false,
        exited: false,
        exit_code: None,
        closed: false,
        failure: None,
    };
    assert_eq!(page.validate(), Err(Error::Invalid));
    page.events = vec![event(2), event(1)];
    assert_eq!(page.validate(), Err(Error::Invalid));
    page.events = vec![
        ProcessEvent {
            seq: 1,
            kind: EventKind::Closed,
        },
        event(2),
    ];
    page.closed = true;
    assert_eq!(page.validate(), Err(Error::Invalid));
    page.events = vec![ProcessEvent {
        seq: 1,
        kind: EventKind::Output {
            stream: OutputStream::Pty,
            chunk: vec![1; MAX_BODY_BYTES + 1],
        },
    }];
    assert_eq!(page.validate(), Err(Error::Limit));
    let read = Request::new(
        "read",
        [7; 32],
        1,
        Action::Read(ReadQuery {
            max_bytes: 1,
            ..query()
        }),
    )
    .unwrap();
    page.events = vec![ProcessEvent {
        seq: 1,
        kind: EventKind::Output {
            stream: OutputStream::Pty,
            chunk: vec![1, 2],
        },
    }];
    let reply = Reply::new(&read, ReplyBody::Page(page)).unwrap();
    assert_eq!(
        Reply::decode_for(&read, &reply.encode().unwrap()),
        Err(Error::Invalid)
    );
}
