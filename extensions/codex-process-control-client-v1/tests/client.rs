use morrow_codex_process_control_client_v1::{
    protocol::{
        Action, Capabilities, EventKind, MAX_FRAME_BYTES, OutputPage, OutputStream, ProcessEvent,
        ReadQuery, Reply, ReplyBody, Request,
    },
    *,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn caps() -> Capabilities {
    Capabilities {
        read: true,
        events: true,
        write: true,
        close_input: true,
        interrupt: true,
        terminate: true,
        resize_pty: true,
    }
}
fn query() -> ReadQuery {
    ReadQuery {
        after_seq: 0,
        max_bytes: 32768,
        max_events: 16,
        wait_ms: 0,
    }
}
fn page() -> OutputPage {
    OutputPage {
        events: vec![
            ProcessEvent {
                seq: 1,
                kind: EventKind::Output {
                    stream: OutputStream::Stdout,
                    chunk: b"output\0\xff".to_vec(),
                },
            },
            ProcessEvent {
                seq: 2,
                kind: EventKind::Exited {
                    exit_code: 7,
                    sandbox_denied: None,
                },
            },
        ],
        next_seq: 2,
        floor_seq: 1,
        gap: false,
        exited: true,
        exit_code: Some(7),
        closed: false,
        failure: None,
    }
}
fn transport(bytes: &[u8]) -> std::result::Result<Vec<u8>, ()> {
    let request = Request::decode(bytes).unwrap();
    assert_eq!(request.handle, [3; 32]);
    assert_eq!(request.generation, 11);
    let body = match &request.action {
        Action::Discover => ReplyBody::Capabilities(caps()),
        Action::Read(_) | Action::Events(_) => ReplyBody::Page(page()),
        _ => ReplyBody::Accepted,
    };
    Reply::new(&request, body).unwrap().encode().map_err(|_| ())
}
fn client<T: Transport>(transport: T) -> Client<T> {
    Client::new(
        transport,
        [3; 32],
        11,
        "invocation",
        caps(),
        Budget::default(),
    )
    .unwrap()
}

#[test]
fn canonical_typed_calls_keep_exit_and_output_closure_separate() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let seen = calls.clone();
    let mut c = client(move |b: &[u8]| {
        seen.borrow_mut().push(Request::decode(b).unwrap());
        transport(b)
    });
    assert_eq!(c.discover().unwrap(), caps());
    let read = c.read(query()).unwrap();
    assert_eq!(read.exit_code, Some(7));
    assert!(!read.closed);
    assert_eq!(c.events(query()).unwrap().events.len(), 2);
    c.write(b"input\0\xff".to_vec()).unwrap();
    c.interrupt().unwrap();
    c.resize(24, 80).unwrap();
    c.terminate().unwrap();
    c.close_input().unwrap();
    assert_eq!(c.write(vec![1]), Err(Error::InputClosed));
    let seen = calls.borrow();
    assert_eq!(seen.len(), 8);
    for (n, request) in seen.iter().enumerate() {
        assert_eq!(request.request_id, format!("invocation-{}", n + 1));
        assert_eq!(
            request.encode().unwrap(),
            Request::decode(&request.encode().unwrap())
                .unwrap()
                .encode()
                .unwrap()
        );
    }
}
#[test]
fn original_provider_missing_close_and_resize_is_respected_before_dispatch() {
    let count = Cell::new(0);
    let supported = Capabilities {
        close_input: false,
        resize_pty: false,
        ..caps()
    };
    let mut c = client(|b: &[u8]| {
        count.set(count.get() + 1);
        let r = Request::decode(b).unwrap();
        Reply::new(&r, ReplyBody::Capabilities(supported))
            .unwrap()
            .encode()
            .map_err(|_| ())
    });
    let actual = c.discover().unwrap();
    assert!(!actual.close_input);
    assert!(!actual.resize_pty);
    assert_eq!(c.close_input(), Err(Error::Unsupported));
    assert_eq!(c.resize(24, 80), Err(Error::Unsupported));
    assert_eq!(count.get(), 1);
}
#[test]
fn declaration_intersection_and_undiscovered_ceiling_never_grant_controls() {
    let count = Cell::new(0);
    let declared = Capabilities {
        read: true,
        ..Default::default()
    };
    let mut c = Client::new(
        |b: &[u8]| {
            count.set(count.get() + 1);
            transport(b)
        },
        [3; 32],
        11,
        "limited",
        declared,
        Budget::default(),
    )
    .unwrap();
    assert_eq!(c.write(vec![1]), Err(Error::Undiscovered));
    assert_eq!(c.discover().unwrap(), declared);
    assert_eq!(c.write(vec![1]), Err(Error::Unsupported));
    assert_eq!(c.terminate(), Err(Error::Unsupported));
    assert_eq!(count.get(), 1);
    c.read(query()).unwrap();
    assert_eq!(count.get(), 2);
}
#[test]
fn budgets_and_argument_validation_reject_before_any_call() {
    let calls = Cell::new(0);
    let mut c = Client::new(
        |b: &[u8]| {
            calls.set(calls.get() + 1);
            transport(b)
        },
        [3; 32],
        11,
        "budget",
        caps(),
        Budget {
            calls: 1,
            ..Default::default()
        },
    )
    .unwrap();
    c.discover().unwrap();
    assert_eq!(c.write(vec![1]), Err(Error::Budget));
    assert_eq!(
        c.resize(0, 80),
        Err(Error::Protocol(protocol::Error::Limit))
    );
    assert_eq!(calls.get(), 1);
    let mut c = Client::new(
        |b: &[u8]| {
            calls.set(calls.get() + 1);
            transport(b)
        },
        [3; 32],
        11,
        "tiny",
        caps(),
        Budget {
            reply_bytes: MAX_FRAME_BYTES - 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(c.discover(), Err(Error::Budget));
    assert_eq!(calls.get(), 1);
}
#[test]
fn uncertain_control_is_not_replayed_and_reads_or_discovery_never_unlock_it() {
    for bad_reply in [false, true] {
        let calls = Cell::new(0);
        let writes = Cell::new(0);
        let mut c = client(|b: &[u8]| {
            calls.set(calls.get() + 1);
            let r = Request::decode(b).unwrap();
            if matches!(r.action, Action::Write(_)) {
                writes.set(writes.get() + 1);
                if !bad_reply {
                    return Err(());
                }
                let mut reply = Reply::new(&r, ReplyBody::Accepted).unwrap();
                reply.request_sha256 = [8; 32];
                return reply.encode().map_err(|_| ());
            }
            transport(b)
        });
        c.discover().unwrap();
        assert!(matches!(c.write(vec![1]), Err(Error::Unknown(_))));
        assert_eq!(writes.get(), 1);
        assert!(c.controls_unknown());
        assert_eq!(c.write(vec![1]), Err(Error::ControlsUnknown));
        assert_eq!(c.terminate(), Err(Error::ControlsUnknown));
        assert_eq!(calls.get(), 2);
        assert!(c.read(query()).is_ok());
        c.discover().unwrap();
        assert_eq!(c.interrupt(), Err(Error::ControlsUnknown));
        assert_eq!(calls.get(), 4);
        assert_eq!(writes.get(), 1);
    }
}
#[test]
fn explicit_host_unknown_keeps_canonical_error_reply_and_latches_all_controls() {
    let calls = Cell::new(0);
    let mut c = client(|b: &[u8]| {
        calls.set(calls.get() + 1);
        let request = Request::decode(b).unwrap();
        if request.action.is_mutation() {
            return Reply::new(&request, ReplyBody::Rejected(protocol::Error::Unknown))
                .unwrap()
                .encode()
                .map_err(|_| ());
        }
        transport(b)
    });
    c.discover().unwrap();
    let reply = c.call(Action::Terminate).unwrap();
    assert_eq!(reply.body, ReplyBody::Rejected(protocol::Error::Unknown));
    assert!(c.controls_unknown());
    assert_eq!(c.close_input(), Err(Error::ControlsUnknown));
    assert_eq!(calls.get(), 2);
}
#[test]
fn wrong_generation_or_reply_identity_never_counts_as_success() {
    for foreign in [true, false] {
        let count = Cell::new(0);
        let mut c = client(|b: &[u8]| {
            count.set(count.get() + 1);
            let request = Request::decode(b).unwrap();
            let mut reply = Reply::new(&request, ReplyBody::Capabilities(caps())).unwrap();
            if foreign {
                reply.generation = 12;
            } else {
                reply.request_id = "foreign".into();
            }
            reply.encode().map_err(|_| ())
        });
        assert!(matches!(c.discover(), Err(Error::Unknown(_))));
        assert_eq!(c.capabilities(), None);
        assert_eq!(count.get(), 1);
        assert_eq!(c.generation(), 11);
        assert!(!c.controls_unknown());
    }
}
#[test]
fn definite_rejection_is_exposed_without_fabricating_acceptance() {
    let mut c = client(|b: &[u8]| {
        let r = Request::decode(b).unwrap();
        if r.action.is_mutation() {
            return Reply::new(&r, ReplyBody::Rejected(protocol::Error::Unsupported))
                .unwrap()
                .encode()
                .map_err(|_| ());
        }
        transport(b)
    });
    c.discover().unwrap();
    assert_eq!(
        c.close_input(),
        Err(Error::Rejected(protocol::Error::Unsupported))
    );
    assert!(!c.controls_unknown());
    assert!(c.read(query()).is_ok());
}
#[test]
fn invalid_handles_generations_and_overlong_invocation_ids_are_rejected() {
    assert!(Client::new(transport, [0; 32], 11, "x", caps(), Budget::default()).is_err());
    assert!(Client::new(transport, [3; 32], 0, "x", caps(), Budget::default()).is_err());
    assert!(
        Client::new(
            transport,
            [3; 32],
            11,
            "x".repeat(108),
            caps(),
            Budget::default()
        )
        .is_err()
    );
    assert!(
        Client::new(
            transport,
            [3; 32],
            11,
            "x".repeat(107),
            caps(),
            Budget::default()
        )
        .is_ok()
    );
    assert!(
        Client::new(
            transport,
            [3; 32],
            11,
            "contains secret\n",
            caps(),
            Budget::default()
        )
        .is_err()
    );
}

#[test]
fn caught_transport_panic_keeps_the_same_client_locked_against_control_replay() {
    let effects = Cell::new(0);
    let calls = Cell::new(0);
    let mut c = client(|bytes: &[u8]| {
        calls.set(calls.get() + 1);
        let request = Request::decode(bytes).unwrap();
        if request.action.is_mutation() {
            effects.set(effects.get() + 1);
            panic!("synthetic transport panic after admitting a control");
        }
        transport(bytes)
    });
    c.discover().unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| c.write(vec![1])));
    assert!(panic.is_err());
    assert!(c.controls_unknown());
    assert_eq!(effects.get(), 1);
    assert_eq!(c.write(vec![1]), Err(Error::ControlsUnknown));
    assert_eq!(c.terminate(), Err(Error::ControlsUnknown));
    assert_eq!(calls.get(), 2);
    c.read(query()).unwrap();
    c.discover().unwrap();
    assert!(c.controls_unknown());
    assert_eq!(c.interrupt(), Err(Error::ControlsUnknown));
    assert_eq!(effects.get(), 1);
    assert_eq!(calls.get(), 4);
}
