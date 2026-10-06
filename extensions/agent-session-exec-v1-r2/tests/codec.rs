//! Public consumer and hostile-wire checks; no Store or execution authority.
use capnp::{message::Builder, serialize};
use morrow_agent_session_exec_v1_r2::*;
mod runtime {
    pub const PROTOCOL_VERSION: u16 = morrow_agent_session_exec_v1_r2::CORE_RUNTIME_VERSION;
    pub const fn runtime_digest() -> [u8; 32] {
        morrow_agent_session_exec_v1_r2::core_runtime_digest()
    }
    pub const fn content_digest() -> [u8; 32] {
        morrow_agent_session_exec_v1_r2::core_content_digest()
    }
}
#[allow(clippy::all, dead_code)]
mod session_exec_capnp {
    include!(concat!(env!("OUT_DIR"), "/session_exec_capnp.rs"));
}
use session_exec_capnp as wire;

fn intent() -> Intent {
    Intent {
        operation_id: "op-one".into(),
        artifact_sha256: hash(b"synthetic reviewed artifact"),
        program: "/trusted/bin/tool".into(),
        argv: vec!["--fixed".into()],
        cwd: "/trusted/work".into(),
        env: vec![Environment {
            name: "LANG".into(),
            value: "C".into(),
        }],
        input: b"opaque input".to_vec(),
        execution_domain: "domain-one".into(),
        max_runtime_ms: 1000,
    }
}
fn event(n: u8) -> Event {
    Event {
        event_id: format!("event-{n}"),
        body: vec![n, 0, 255],
    }
}
fn facts() -> ExecutionFacts {
    ExecutionFacts {
        exit_code: Some(0),
        output_closed: true,
        stdout_sha256: hash(&[]),
        stderr_sha256: hash(&[]),
        stdout_bytes: 0,
        stderr_bytes: 0,
    }
}
fn set_wire_facts(mut out: wire::execution_facts::Builder<'_>, value: &ExecutionFacts) {
    out.set_has_exit_code(value.exit_code.is_some());
    out.set_exit_code(value.exit_code.unwrap_or_default());
    out.set_output_closed(value.output_closed);
    out.set_stdout_sha256(&value.stdout_sha256);
    out.set_stderr_sha256(&value.stderr_sha256);
    out.set_stdout_bytes(value.stdout_bytes);
    out.set_stderr_bytes(value.stderr_bytes);
}
fn session_info() -> SessionInfo {
    SessionInfo {
        session_id: "session-one".into(),
        parent: None,
        parent_tail: 0,
        epoch: 0,
        tail: 0,
        floor: 1,
        archived: false,
        revision: 1,
        checkpoint_tail: 0,
        checkpoint_sha256: hash(&[]),
        checkpoint_sealed: false,
        parent_checkpoint_sha256: None,
    }
}
fn tool(phase: ToolPhase) -> ToolInfo {
    ToolInfo {
        operation_id: "op-one".into(),
        session_id: "session-one".into(),
        intent_sha256: intent().digest().unwrap(),
        phase,
        facts: if phase == ToolPhase::Reported {
            Some(facts())
        } else {
            None
        },
        observation: (phase == ToolPhase::Reported).then(facts),
        observation_revision: u64::from(phase == ToolPhase::Reported),
    }
}
fn req(action: Action) -> Request {
    Request::new("request-one", action).unwrap()
}
fn list() -> Request {
    req(Action::List)
}
fn actions() -> Vec<Action> {
    vec![
        Action::Create {
            session_id: "session-one".into(),
            parent: None,
            parent_tail: 0,
        },
        Action::List,
        Action::Snapshot {
            session_id: "session-one".into(),
            after: 0,
            limit: 16,
        },
        Action::OpenWriter {
            session_id: "session-one".into(),
            expected_epoch: 0,
        },
        Action::Append {
            session_id: "session-one".into(),
            epoch: 1,
            expected_tail: 0,
            events: vec![event(1)],
        },
        Action::Checkpoint {
            session_id: "session-one".into(),
            epoch: 1,
            expected_tail: 1,
            state: vec![0, 255],
        },
        Action::Archive {
            session_id: "session-one".into(),
            epoch: 1,
            expected_tail: 1,
        },
        Action::Propose {
            session_id: "session-one".into(),
            intent: intent(),
        },
        Action::Claim {
            operation_id: "op-one".into(),
            permit: [7; 32],
        },
        Action::Report {
            operation_id: "op-one".into(),
            claim: [8; 32],
            facts: facts(),
        },
        Action::Inspect {
            operation_id: "op-one".into(),
        },
    ]
}
fn raw_request(edit: impl FnOnce(wire::request::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    {
        let mut root = message.init_root::<wire::request::Builder<'_>>();
        root.set_version(VERSION);
        root.set_revision(REVISION);
        root.set_schema_sha256(&schema_digest());
        root.set_runtime_digest(&runtime::runtime_digest());
        root.set_content_digest(&runtime::content_digest());
        root.set_core_runtime_version(runtime::PROTOCOL_VERSION);
        root.set_request_id("request-one");
        root.set_generation(1);
        root.set_list(());
        edit(root);
    }
    serialize::write_message_to_words(&message)
}
fn raw_reply(request: &Request, edit: impl FnOnce(wire::reply::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    {
        let mut root = message.init_root::<wire::reply::Builder<'_>>();
        root.set_version(VERSION);
        root.set_revision(REVISION);
        root.set_schema_sha256(&schema_digest());
        root.set_runtime_digest(&runtime::runtime_digest());
        root.set_content_digest(&runtime::content_digest());
        root.set_core_runtime_version(runtime::PROTOCOL_VERSION);
        root.set_request_id(request.id());
        root.set_request_sha256(&request.digest());
        root.set_generation(request.generation());
        root.set_rejected(wire::Failure::Denied);
        edit(root);
    }
    serialize::write_message_to_words(&message)
}
fn root_data(bytes: &[u8]) -> usize {
    assert_eq!(&bytes[..4], &[0; 4]);
    let pointer = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    assert_eq!(pointer & 3, 0);
    let offset = ((pointer as u32 as i32) >> 2) as isize;
    (16isize + offset * 8) as usize
}
fn set_root_u16(bytes: &mut [u8], index: usize, value: u16) {
    let at = root_data(bytes) + index * 2;
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn assert_wrong_reply(request: &Request, outcome: Outcome) {
    assert_eq!(
        Reply::new(request, outcome.clone()),
        Err(Error::Correlation)
    );
    let reply = Reply {
        request_id: request.id().into(),
        generation: request.generation(),
        request_sha256: request.digest(),
        outcome,
    };
    if let Ok(bytes) = reply.encode() {
        assert_eq!(Reply::decode_for(request, &bytes), Err(Error::Correlation));
    }
}

#[test]
fn schema_digest_is_exact_raw_file_sha256() {
    assert_eq!(hash(SCHEMA), schema_digest());
}
#[cfg(feature = "host")]
#[test]
fn client_core_identities_equal_the_host_core_contract() {
    assert_eq!(CORE_RUNTIME_VERSION, morrow_core::runtime::PROTOCOL_VERSION);
    assert_eq!(
        core_runtime_digest(),
        morrow_core::runtime::runtime_digest()
    );
    assert_eq!(
        core_content_digest(),
        morrow_core::runtime::content_digest()
    );
}
#[test]
fn eleven_actions_roundtrip_and_retain_full_original_bytes() {
    for action in actions() {
        let value = req(action);
        assert_eq!(Request::decode(value.raw()).unwrap(), value);
        assert_eq!(value.digest(), hash(value.raw()));
    }
}
#[test]
fn generation_is_nonzero_and_bound_to_raw_request_and_reply() {
    let initial = list();
    assert_eq!(initial.generation(), 1);
    let renewed = Request::new_for_generation("request-one", 2, Action::List).unwrap();
    assert_eq!(Request::decode(renewed.raw()).unwrap(), renewed);
    assert_ne!(initial.raw(), renewed.raw());
    assert_ne!(initial.digest(), renewed.digest());
    assert_eq!(
        Request::new_for_generation("request-one", 0, Action::List),
        Err(Error::Invalid)
    );
    assert_eq!(
        Request::decode(&raw_request(|mut root| root.set_generation(0))),
        Err(Error::Invalid)
    );
    let reply = Reply::new(&renewed, Outcome::Rejected(Error::Denied)).unwrap();
    assert_eq!(reply.generation, 2);
    assert_eq!(
        Reply::decode_for(&renewed, &reply.encode().unwrap()).unwrap(),
        reply
    );
    assert_eq!(
        Reply::decode_for(&initial, &reply.encode().unwrap()),
        Err(Error::Correlation)
    );
    for generation in [0, 1, 3] {
        assert_eq!(
            Reply::decode_for(
                &renewed,
                &raw_reply(&renewed, |mut root| root.set_generation(generation))
            ),
            Err(Error::Correlation)
        );
        let mut changed = reply.clone();
        changed.generation = generation;
        assert_eq!(changed.validate_for(&renewed), Err(Error::Correlation));
    }
}
#[test]
fn six_reply_shapes_roundtrip_with_request_correlation() {
    let cases = vec![
        (req(actions()[0].clone()), Outcome::Session(session_info())),
        (list(), Outcome::Sessions(vec![session_info()])),
        (
            req(actions()[2].clone()),
            Outcome::Snapshot(SessionSnapshot {
                info: session_info(),
                events: vec![],
                gap: false,
                checkpoint: vec![],
            }),
        ),
        (
            req(actions()[7].clone()),
            Outcome::Tool(tool(ToolPhase::Proposed)),
        ),
        (
            req(actions()[8].clone()),
            Outcome::Claimed {
                info: tool(ToolPhase::DispatchUnknown),
                intent: intent(),
                claim: [8; 32],
            },
        ),
        (list(), Outcome::Rejected(Error::Denied)),
    ];
    for (request, outcome) in cases {
        let reply = Reply::new(&request, outcome).unwrap();
        let bytes = reply.encode().unwrap();
        assert_eq!(Reply::decode_for(&request, &bytes).unwrap(), reply);
    }
}
#[test]
fn all_rejections_are_correlated_and_canonical() {
    for error in [
        Error::Invalid,
        Error::Contract,
        Error::Limit,
        Error::Correlation,
        Error::Denied,
        Error::Conflict,
        Error::NotFound,
        Error::CommitUnknown,
        Error::Storage,
    ] {
        let request = list();
        let reply = Reply::new(&request, Outcome::Rejected(error)).unwrap();
        assert_eq!(
            Reply::decode_for(&request, &reply.encode().unwrap()),
            Ok(reply)
        );
    }
}
#[test]
fn every_intent_input_field_changes_complete_digest() {
    let original = intent();
    let mut changes = vec![];
    let mut value = original.clone();
    value.artifact_sha256[0] ^= 1;
    changes.push(value);
    let mut value = original.clone();
    value.operation_id.push('x');
    changes.push(value);
    let mut value = original.clone();
    value.program.push('x');
    changes.push(value);
    let mut value = original.clone();
    value.argv[0].push('x');
    changes.push(value);
    let mut value = original.clone();
    value.cwd.push('x');
    changes.push(value);
    let mut value = original.clone();
    value.env[0].name.push('X');
    changes.push(value);
    let mut value = original.clone();
    value.env[0].value.push('x');
    changes.push(value);
    let mut value = original.clone();
    value.input.push(0);
    changes.push(value);
    let mut value = original.clone();
    value.execution_domain.push('x');
    changes.push(value);
    let mut value = original.clone();
    value.max_runtime_ms += 1;
    changes.push(value);
    let original_hash = original.digest().unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for changed in changes {
        let digest = changed.digest().unwrap();
        assert_ne!(digest, original_hash);
        assert!(seen.insert(digest));
    }
}
#[test]
fn environment_order_is_preserved_and_bound_to_digest() {
    let mut value = intent();
    value.env.push(Environment {
        name: "TZ".into(),
        value: "UTC".into(),
    });
    let digest = value.digest().unwrap();
    value.env.reverse();
    assert_ne!(digest, value.digest().unwrap());
    let request = req(Action::Propose {
        session_id: "session-one".into(),
        intent: value,
    });
    assert_eq!(Request::decode(request.raw()).unwrap(), request);
}
#[test]
fn environment_duplicate_keys_are_rejected_case_insensitively() {
    let mut value = intent();
    value.env.push(Environment {
        name: "lang".into(),
        value: "changed".into(),
    });
    assert_eq!(value.validate(), Err(Error::Invalid));
}
#[test]
fn relative_traversal_or_control_paths_are_rejected() {
    for path in [
        "tool",
        "./tool",
        "/trusted/../tool",
        "C:\\trusted\\.\\tool",
        "C:tool",
        "/trusted/\0tool",
        "/trusted/\ntool",
    ] {
        let mut value = intent();
        value.program = path.into();
        assert_eq!(value.validate(), Err(Error::Invalid));
    }
    let mut value = intent();
    value.program = "C:\\trusted\\tool.exe".into();
    value.cwd = "C:/trusted/work".into();
    assert_eq!(value.validate(), Ok(()));
}
#[test]
fn fixed_runtime_and_output_bounds_are_enforced() {
    let mut value = intent();
    value.max_runtime_ms = 0;
    assert_eq!(value.validate(), Err(Error::Limit));
    value.max_runtime_ms = MAX_RUNTIME_MS + 1;
    assert_eq!(value.validate(), Err(Error::Limit));
    value.max_runtime_ms = MAX_RUNTIME_MS;
    assert_eq!(value.validate(), Ok(()));
    let mut value = facts();
    value.stdout_bytes = MAX_OUTPUT_BYTES + 1;
    assert_eq!(value.validate(), Err(Error::Limit));
}
#[test]
fn each_bounded_collection_and_blob_rejects_overflow() {
    let mut value = intent();
    value.argv = vec![String::new(); MAX_ARGUMENTS + 1];
    assert_eq!(value.validate(), Err(Error::Limit));
    let mut value = intent();
    value.env = vec![
        Environment {
            name: "A".into(),
            value: String::new()
        };
        MAX_ENVIRONMENT + 1
    ];
    assert_eq!(value.validate(), Err(Error::Limit));
    let mut value = intent();
    value.input = vec![0; MAX_BODY_BYTES + 1];
    assert_eq!(value.validate(), Err(Error::Limit));
    let mut value = event(1);
    value.body = vec![0; MAX_BODY_BYTES + 1];
    assert_eq!(value.validate(), Err(Error::Limit));
    assert_eq!(
        Request::new(
            "r",
            Action::Append {
                session_id: "s".into(),
                epoch: 1,
                expected_tail: 0,
                events: (0..17).map(event).collect()
            }
        ),
        Err(Error::Limit)
    );
    assert_eq!(
        Request::new(
            "r",
            Action::Checkpoint {
                session_id: "s".into(),
                epoch: 1,
                expected_tail: 0,
                state: vec![0; MAX_BODY_BYTES + 1]
            }
        ),
        Err(Error::Limit)
    );
}
#[test]
fn duplicate_event_ids_and_invalid_environment_names_are_rejected() {
    assert_eq!(
        Request::new(
            "r",
            Action::Append {
                session_id: "s".into(),
                epoch: 1,
                expected_tail: 0,
                events: vec![event(1), event(1)]
            }
        ),
        Err(Error::Invalid)
    );
    for name in ["", "1ENV", "A=B", "A-B", "A\0B"] {
        let mut value = intent();
        value.env[0].name = name.into();
        assert_eq!(value.validate(), Err(Error::Invalid));
    }
}
#[test]
fn legal_individual_events_can_exceed_aggregate_request_frame() {
    let events = (0..4)
        .map(|n| Event {
            event_id: format!("event-{n}"),
            body: vec![n; MAX_BODY_BYTES],
        })
        .collect();
    assert_eq!(
        Request::new(
            "r",
            Action::Append {
                session_id: "s".into(),
                epoch: 1,
                expected_tail: 0,
                events
            }
        ),
        Err(Error::Limit)
    );
}
#[test]
fn legal_individual_events_cannot_escape_aggregate_reply_bound() {
    let mut info = session_info();
    info.tail = 4;
    let snapshot = SessionSnapshot {
        info,
        events: (1..=4)
            .map(|n| StoredEvent {
                sequence: n,
                event: Event {
                    event_id: format!("event-{n}"),
                    body: vec![n as u8; MAX_BODY_BYTES],
                },
            })
            .collect(),
        gap: false,
        checkpoint: vec![],
    };
    let request = req(Action::Snapshot {
        session_id: "session-one".into(),
        after: 0,
        limit: 4,
    });
    let reply = Reply::new(&request, Outcome::Snapshot(snapshot)).unwrap();
    assert_eq!(reply.encode(), Err(Error::Limit));
}
#[test]
fn empty_oversized_trailing_and_truncated_frames_are_rejected() {
    assert_eq!(Request::decode(&[]), Err(Error::Limit));
    assert_eq!(
        Request::decode(&vec![0; MAX_FRAME_BYTES + 1]),
        Err(Error::Limit)
    );
    let request = list();
    for n in (0..request.raw().len()).step_by(8) {
        assert!(Request::decode(&request.raw()[..n]).is_err());
    }
    let mut bytes = request.raw().to_vec();
    bytes.extend_from_slice(&[0; 8]);
    assert_eq!(Request::decode(&bytes), Err(Error::Invalid));
    let reply = Reply::new(&request, Outcome::Rejected(Error::Denied)).unwrap();
    let mut bytes = reply.encode().unwrap();
    bytes.extend_from_slice(&[0; 8]);
    assert_eq!(Reply::decode_for(&request, &bytes), Err(Error::Invalid));
}
#[test]
fn every_contract_identity_component_is_required_on_requests() {
    type RequestChange = Box<dyn for<'a> Fn(wire::request::Builder<'a>)>;
    let changes: Vec<RequestChange> = vec![
        Box::new(|mut root| root.set_version(VERSION + 1)),
        Box::new(|mut root| root.set_revision(REVISION + 1)),
        Box::new(|mut root| root.set_schema_sha256(&[0; 32])),
        Box::new(|mut root| root.set_core_runtime_version(runtime::PROTOCOL_VERSION + 1)),
        Box::new(|mut root| root.set_runtime_digest(&[0; 32])),
        Box::new(|mut root| root.set_content_digest(&[0; 32])),
    ];
    for change in changes {
        assert_eq!(Request::decode(&raw_request(change)), Err(Error::Contract));
    }
}
#[test]
fn every_contract_identity_component_is_required_on_replies() {
    let request = list();
    type ReplyChange = Box<dyn for<'a> Fn(wire::reply::Builder<'a>)>;
    let changes: Vec<ReplyChange> = vec![
        Box::new(|mut root| root.set_version(VERSION + 1)),
        Box::new(|mut root| root.set_revision(REVISION + 1)),
        Box::new(|mut root| root.set_schema_sha256(&[0; 32])),
        Box::new(|mut root| root.set_core_runtime_version(runtime::PROTOCOL_VERSION + 1)),
        Box::new(|mut root| root.set_runtime_digest(&[0; 32])),
        Box::new(|mut root| root.set_content_digest(&[0; 32])),
    ];
    for change in changes {
        assert_eq!(
            Reply::decode_for(&request, &raw_reply(&request, change)),
            Err(Error::Contract)
        );
    }
}
#[test]
fn unknown_request_reply_union_and_failure_tags_are_rejected() {
    let mut bytes = list().raw().to_vec();
    set_root_u16(&mut bytes, 3, 99);
    assert_eq!(Request::decode(&bytes), Err(Error::Invalid));
    let request = list();
    let mut bytes = raw_reply(&request, |_| {});
    set_root_u16(&mut bytes, 3, 99);
    assert_eq!(Reply::decode_for(&request, &bytes), Err(Error::Invalid));
    let mut bytes = raw_reply(&request, |_| {});
    set_root_u16(&mut bytes, 4, 99);
    assert_eq!(Reply::decode_for(&request, &bytes), Err(Error::Invalid));
}
#[test]
fn enlarged_root_layout_is_rejected_before_projection() {
    let mut bytes = list().raw().to_vec();
    let mut pointer = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    pointer += 1 << 32;
    bytes[8..16].copy_from_slice(&pointer.to_le_bytes());
    assert_eq!(Request::decode(&bytes), Err(Error::Contract));
}
#[test]
fn capability_pointers_are_rejected_before_action_decoding() {
    let mut bytes = list().raw().to_vec();
    let pointer = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let data_words = ((pointer >> 32) & 0xffff) as usize;
    let pointer_at = root_data(&bytes) + data_words * 8 + 4 * 8;
    bytes[pointer_at..pointer_at + 8].copy_from_slice(&3u64.to_le_bytes());
    assert!(Request::decode(&bytes).is_err());
}
#[test]
fn noncanonical_unreachable_bytes_are_rejected() {
    let bytes = raw_request(|mut root| root.set_request_id("another-valid-request"));
    assert_eq!(Request::decode(&bytes), Err(Error::Contract));
    let bytes = raw_request(|mut root| {
        root.reborrow().init_create().set_session_id("unused");
        root.set_list(());
    });
    assert_eq!(Request::decode(&bytes), Err(Error::Contract));
}
#[test]
fn list_pointer_hidden_under_void_union_is_rejected() {
    let bytes = raw_request(|mut root| {
        root.reborrow().init_inspect().set_operation_id("hidden");
        root.set_list(());
    });
    assert_eq!(Request::decode(&bytes), Err(Error::Contract));
}
#[test]
fn identity_controls_and_path_separators_are_rejected() {
    for id in ["", "a/b", "a\\b", "a:b", "a\0b", "a\nb"] {
        assert_eq!(Request::new(id, Action::List), Err(Error::Invalid));
    }
    assert_eq!(
        Request::new("x".repeat(257), Action::List),
        Err(Error::Invalid)
    );
}
#[test]
fn epoch_tail_and_read_after_overflow_are_rejected() {
    assert_eq!(
        Request::new(
            "r",
            Action::OpenWriter {
                session_id: "s".into(),
                expected_epoch: u64::MAX
            }
        ),
        Err(Error::Limit)
    );
    assert_eq!(
        Request::new(
            "r",
            Action::Append {
                session_id: "s".into(),
                epoch: 1,
                expected_tail: u64::MAX,
                events: vec![event(1)]
            }
        ),
        Err(Error::Limit)
    );
    assert_eq!(
        Request::new(
            "r",
            Action::Snapshot {
                session_id: "s".into(),
                after: u64::MAX,
                limit: 1
            }
        ),
        Err(Error::Limit)
    );
    assert_eq!(
        Request::new(
            "r",
            Action::Snapshot {
                session_id: "s".into(),
                after: 0,
                limit: 17
            }
        ),
        Err(Error::Limit)
    );
}
#[test]
fn request_id_and_full_raw_request_sha_bind_every_reply() {
    let request = list();
    assert_eq!(
        Reply::decode_for(
            &request,
            &raw_reply(&request, |mut root| root.set_request_id("another"))
        ),
        Err(Error::Correlation)
    );
    assert_eq!(
        Reply::decode_for(
            &request,
            &raw_reply(&request, |mut root| root.set_request_sha256(&[0; 32]))
        ),
        Err(Error::Correlation)
    );
    let other = Request::new(
        "request-one",
        Action::Inspect {
            operation_id: "op-one".into(),
        },
    )
    .unwrap();
    let reply = Reply::new(&request, Outcome::Rejected(Error::Denied)).unwrap();
    assert_eq!(
        Reply::decode_for(&other, &reply.encode().unwrap()),
        Err(Error::Correlation)
    );
}
#[test]
fn wrong_action_outcome_pair_is_rejected() {
    assert_wrong_reply(&list(), Outcome::Session(session_info()));
}
#[test]
fn create_parent_epoch_and_initial_tail_are_bound() {
    let request = req(actions()[0].clone());
    let mut changed = session_info();
    changed.session_id = "wrong".into();
    assert_wrong_reply(&request, Outcome::Session(changed));
    let mut changed = session_info();
    changed.epoch = 1;
    assert_wrong_reply(&request, Outcome::Session(changed));
    let mut changed = session_info();
    changed.parent = Some("parent".into());
    changed.parent_checkpoint_sha256 = Some(hash(&[]));
    assert_wrong_reply(&request, Outcome::Session(changed));
}
#[test]
fn writer_append_checkpoint_and_archive_bind_epoch_and_tail() {
    let requests = actions();
    let mut writer = session_info();
    writer.epoch = 1;
    assert!(Reply::new(&req(requests[3].clone()), Outcome::Session(writer.clone())).is_ok());
    writer.epoch = 2;
    assert_wrong_reply(&req(requests[3].clone()), Outcome::Session(writer));
    let mut append = session_info();
    append.epoch = 1;
    append.tail = 1;
    assert!(Reply::new(&req(requests[4].clone()), Outcome::Session(append.clone())).is_ok());
    append.tail = 2;
    assert_wrong_reply(&req(requests[4].clone()), Outcome::Session(append));
    let mut checkpoint = session_info();
    checkpoint.epoch = 1;
    checkpoint.tail = 1;
    checkpoint.checkpoint_tail = 1;
    checkpoint.checkpoint_sha256 = hash(&[0, 255]);
    checkpoint.checkpoint_sealed = true;
    assert!(
        Reply::new(
            &req(requests[5].clone()),
            Outcome::Session(checkpoint.clone())
        )
        .is_ok()
    );
    checkpoint.checkpoint_sha256 = hash(&[]);
    assert_wrong_reply(&req(requests[5].clone()), Outcome::Session(checkpoint));
    let mut archived = session_info();
    archived.epoch = 1;
    archived.tail = 1;
    archived.archived = true;
    assert!(
        Reply::new(
            &req(requests[6].clone()),
            Outcome::Session(archived.clone())
        )
        .is_ok()
    );
    archived.archived = false;
    assert_wrong_reply(&req(requests[6].clone()), Outcome::Session(archived));
}
#[test]
fn snapshot_gap_checkpoint_digest_and_full_contiguous_window_are_bound() {
    let request = req(Action::Snapshot {
        session_id: "session-one".into(),
        after: 0,
        limit: 16,
    });
    let mut info = session_info();
    info.tail = 4;
    info.floor = 3;
    info.checkpoint_tail = 2;
    info.checkpoint_sealed = true;
    info.checkpoint_sha256 = hash(b"checkpoint");
    let snapshot = SessionSnapshot {
        info,
        events: vec![
            StoredEvent {
                sequence: 3,
                event: event(3),
            },
            StoredEvent {
                sequence: 4,
                event: event(4),
            },
        ],
        gap: true,
        checkpoint: b"checkpoint".to_vec(),
    };
    assert!(Reply::new(&request, Outcome::Snapshot(snapshot.clone())).is_ok());
    let mut bad = snapshot.clone();
    bad.gap = false;
    assert_wrong_reply(&request, Outcome::Snapshot(bad));
    let mut bad = snapshot.clone();
    bad.events.pop();
    assert_wrong_reply(&request, Outcome::Snapshot(bad));
    let mut bad = snapshot;
    bad.checkpoint.push(0);
    assert_eq!(
        Reply::new(&request, Outcome::Snapshot(bad)),
        Err(Error::Correlation)
    );
}
#[test]
fn snapshot_cursor_beyond_reply_tail_is_rejected_on_construction_and_decode() {
    let snapshot = SessionSnapshot {
        info: session_info(),
        events: Vec::new(),
        gap: false,
        checkpoint: Vec::new(),
    };
    let at_tail = req(Action::Snapshot {
        session_id: "session-one".into(),
        after: 0,
        limit: 1,
    });
    let accepted = Reply::new(&at_tail, Outcome::Snapshot(snapshot.clone())).unwrap();
    assert_eq!(
        Reply::decode_for(&at_tail, &accepted.encode().unwrap()).unwrap(),
        accepted
    );
    let beyond_tail = req(Action::Snapshot {
        session_id: "session-one".into(),
        after: 1,
        limit: 1,
    });
    assert_wrong_reply(&beyond_tail, Outcome::Snapshot(snapshot));
}
#[test]
fn snapshot_cursor_at_compacted_tail_is_a_valid_empty_window() {
    let mut info = session_info();
    info.tail = 4;
    info.floor = 5;
    info.checkpoint_tail = 4;
    info.checkpoint_sealed = true;
    let request = req(Action::Snapshot {
        session_id: "session-one".into(),
        after: 4,
        limit: 16,
    });
    let reply = Reply::new(
        &request,
        Outcome::Snapshot(SessionSnapshot {
            info,
            events: Vec::new(),
            gap: false,
            checkpoint: Vec::new(),
        }),
    )
    .unwrap();
    assert_eq!(
        Reply::decode_for(&request, &reply.encode().unwrap()).unwrap(),
        reply
    );
}
#[test]
fn snapshot_duplicate_event_ids_and_sequence_overflow_do_not_panic() {
    let mut info = session_info();
    info.tail = 2;
    let value = SessionSnapshot {
        info,
        events: vec![
            StoredEvent {
                sequence: 1,
                event: event(1),
            },
            StoredEvent {
                sequence: 2,
                event: event(1),
            },
        ],
        gap: false,
        checkpoint: vec![],
    };
    assert_eq!(value.validate(), Err(Error::Correlation));
    let mut info = session_info();
    info.tail = u64::MAX;
    info.floor = u64::MAX;
    info.checkpoint_sealed = true;
    info.checkpoint_tail = u64::MAX - 1;
    let value = SessionSnapshot {
        info,
        events: vec![
            StoredEvent {
                sequence: u64::MAX,
                event: event(1),
            },
            StoredEvent {
                sequence: u64::MAX,
                event: event(2),
            },
        ],
        gap: false,
        checkpoint: vec![],
    };
    assert_eq!(value.validate(), Err(Error::Correlation));
}
#[test]
fn proposal_claim_report_and_inspection_bind_operation_and_inputs() {
    let requests = actions();
    let proposal = req(requests[7].clone());
    let mut wrong = tool(ToolPhase::Proposed);
    wrong.intent_sha256 = [0; 32];
    assert_wrong_reply(&proposal, Outcome::Tool(wrong));
    let claim = req(requests[8].clone());
    let mut wrong_intent = intent();
    wrong_intent.input.push(0);
    assert_wrong_reply(
        &claim,
        Outcome::Claimed {
            info: tool(ToolPhase::DispatchUnknown),
            intent: wrong_intent,
            claim: [8; 32],
        },
    );
    let report = req(requests[9].clone());
    assert!(Reply::new(&report, Outcome::Tool(tool(ToolPhase::Reported))).is_ok());
    let mut wrong = tool(ToolPhase::Reported);
    wrong.facts.as_mut().unwrap().exit_code = Some(9);
    wrong.observation.as_mut().unwrap().exit_code = Some(9);
    assert_wrong_reply(&report, Outcome::Tool(wrong));
    let inspect = req(requests[10].clone());
    let mut wrong = tool(ToolPhase::Approved);
    wrong.operation_id = "other".into();
    assert_wrong_reply(&inspect, Outcome::Tool(wrong));
}
#[test]
fn exit_and_output_closed_remain_independent_observations() {
    let mut value = facts();
    value.output_closed = false;
    assert_eq!(value.validate(), Ok(()));
    value.exit_code = None;
    value.output_closed = true;
    assert_eq!(value.validate(), Ok(()));
    let request = req(Action::Report {
        operation_id: "op-one".into(),
        claim: [8; 32],
        facts: value.clone(),
    });
    assert_eq!(
        Request::decode(request.raw()).unwrap().action(),
        request.action()
    );
}
#[test]
fn latest_observation_is_separate_from_accepted_report_and_roundtrips() {
    let mut accepted = facts();
    accepted.exit_code = None;
    let mut info = tool(ToolPhase::Reported);
    info.facts = Some(accepted.clone());
    info.observation_revision = 7;
    let request = req(Action::Report {
        operation_id: "op-one".into(),
        claim: [8; 32],
        facts: accepted,
    });
    let reply = Reply::new(&request, Outcome::Tool(info.clone())).unwrap();
    assert_eq!(
        Reply::decode_for(&request, &reply.encode().unwrap()).unwrap(),
        reply
    );
    assert_eq!(info.facts.as_ref().unwrap().exit_code, None);
    assert_eq!(info.observation.as_ref().unwrap().exit_code, Some(0));

    info.phase = ToolPhase::DispatchUnknown;
    info.facts = None;
    let inspect = req(Action::Inspect {
        operation_id: "op-one".into(),
    });
    let reply = Reply::new(&inspect, Outcome::Tool(info)).unwrap();
    assert_eq!(
        Reply::decode_for(&inspect, &reply.encode().unwrap()).unwrap(),
        reply
    );
}
#[test]
fn observation_presence_revision_phase_and_bounds_are_validated() {
    let mut info = tool(ToolPhase::Reported);
    info.observation = None;
    assert_eq!(info.validate(), Err(Error::Invalid));
    let mut info = tool(ToolPhase::Reported);
    info.observation_revision = 0;
    assert_eq!(info.validate(), Err(Error::Invalid));
    for phase in [ToolPhase::Proposed, ToolPhase::Approved, ToolPhase::Revoked] {
        let mut info = tool(phase);
        info.observation = Some(facts());
        info.observation_revision = 1;
        assert_eq!(info.validate(), Err(Error::Invalid));
    }
    let mut info = tool(ToolPhase::DispatchUnknown);
    info.observation_revision = 1;
    assert_eq!(info.validate(), Err(Error::Invalid));
    info.observation = Some(facts());
    info.observation.as_mut().unwrap().stdout_bytes = MAX_OUTPUT_BYTES + 1;
    assert_eq!(info.validate(), Err(Error::Limit));
}
#[test]
fn hostile_observation_revision_and_hidden_payload_are_rejected_on_decode() {
    let inspect = req(Action::Inspect {
        operation_id: "op-one".into(),
    });
    let bytes = raw_reply(&inspect, |root| {
        let mut out = root.init_tool();
        out.set_operation_id("op-one");
        out.set_session_id("session-one");
        out.set_intent_sha256(&intent().digest().unwrap());
        out.set_phase(wire::ToolPhase::DispatchUnknown);
        out.set_has_observation(true);
        set_wire_facts(out.init_observation(), &facts());
    });
    assert_eq!(Reply::decode_for(&inspect, &bytes), Err(Error::Invalid));
    let bytes = raw_reply(&inspect, |root| {
        let mut out = root.init_tool();
        out.set_operation_id("op-one");
        out.set_session_id("session-one");
        out.set_intent_sha256(&intent().digest().unwrap());
        out.set_phase(wire::ToolPhase::DispatchUnknown);
        out.set_has_observation(false);
        set_wire_facts(out.init_observation(), &facts());
    });
    assert_eq!(Reply::decode_for(&inspect, &bytes), Err(Error::Contract));
}
#[test]
fn accepted_facts_cannot_be_retracted_by_latest_observation() {
    let mut info = tool(ToolPhase::Reported);
    info.observation.as_mut().unwrap().exit_code = None;
    assert_eq!(info.validate(), Err(Error::Conflict));
    let mut info = tool(ToolPhase::Reported);
    info.observation.as_mut().unwrap().exit_code = Some(1);
    assert_eq!(info.validate(), Err(Error::Conflict));
    let mut info = tool(ToolPhase::Reported);
    info.observation.as_mut().unwrap().output_closed = false;
    assert_eq!(info.validate(), Err(Error::Conflict));
    let mut info = tool(ToolPhase::Reported);
    let observation = info.observation.as_mut().unwrap();
    observation.stdout_bytes = 1;
    observation.stdout_sha256 = hash(b"x");
    assert_eq!(info.validate(), Err(Error::Conflict));
    let mut info = tool(ToolPhase::Reported);
    let accepted = info.facts.as_mut().unwrap();
    accepted.output_closed = false;
    accepted.stdout_bytes = 1;
    accepted.stdout_sha256 = hash(b"x");
    let observation = info.observation.as_mut().unwrap();
    observation.stdout_bytes = 1;
    observation.stdout_sha256 = hash(b"y");
    assert_eq!(info.validate(), Err(Error::Conflict));
    let mut info = tool(ToolPhase::Reported);
    info.facts.as_mut().unwrap().output_closed = false;
    let observation = info.observation.as_mut().unwrap();
    observation.stdout_bytes = 1;
    observation.stdout_sha256 = hash(b"x");
    assert_eq!(info.validate(), Ok(()));
}
#[test]
fn zero_length_output_requires_empty_sha256() {
    let mut value = facts();
    value.stdout_sha256 = [0; 32];
    assert_eq!(value.validate(), Err(Error::Invalid));
}
#[test]
fn list_replies_reject_duplicate_sessions_and_limit_overflow() {
    assert_eq!(
        Reply::new(
            &list(),
            Outcome::Sessions(vec![session_info(), session_info()])
        ),
        Err(Error::Invalid)
    );
    let values = (0..17)
        .map(|n| {
            let mut info = session_info();
            info.session_id = format!("s-{n}");
            info
        })
        .collect();
    assert_eq!(
        Reply::new(&list(), Outcome::Sessions(values)),
        Err(Error::Limit)
    );
}
#[test]
fn unsealed_empty_checkpoint_is_distinct_from_sealed_empty_checkpoint() {
    let mut value = session_info();
    assert_eq!(value.validate(), Ok(()));
    value.checkpoint_sealed = true;
    assert_eq!(value.validate(), Ok(()));
    let request = req(Action::Snapshot {
        session_id: "session-one".into(),
        after: 0,
        limit: 1,
    });
    let reply = Reply::new(
        &request,
        Outcome::Snapshot(SessionSnapshot {
            info: value,
            events: vec![],
            gap: false,
            checkpoint: vec![],
        }),
    )
    .unwrap();
    assert_eq!(
        Reply::decode_for(&request, &reply.encode().unwrap()),
        Ok(reply)
    );
}
#[test]
fn all_debug_views_redact_data_environment_arguments_and_nonces() {
    let secret = "DO_NOT_LOG_SECRET";
    let mut value = intent();
    value.argv = vec![secret.into()];
    value.env[0].value = secret.into();
    value.input = secret.as_bytes().to_vec();
    assert!(!format!("{value:?}").contains(secret));
    assert!(!format!("{:?}", value.env).contains(secret));
    let request = req(Action::Propose {
        session_id: "session-one".into(),
        intent: value.clone(),
    });
    assert!(!format!("{request:?}").contains(secret));
    let event = Event {
        event_id: "e".into(),
        body: secret.as_bytes().to_vec(),
    };
    assert!(!format!("{event:?}").contains(secret));
    let request = req(Action::Claim {
        operation_id: "op-one".into(),
        permit: [99; 32],
    });
    let mut tool = tool(ToolPhase::DispatchUnknown);
    tool.intent_sha256 = value.digest().unwrap();
    let reply = Reply::new(
        &request,
        Outcome::Claimed {
            info: tool,
            intent: value,
            claim: [88; 32],
        },
    )
    .unwrap();
    let debug = format!("{reply:?}");
    assert!(!debug.contains(secret));
    assert!(!debug.contains("88, 88"));
    assert!(!format!("{request:?}").contains("99, 99"));
}

#[test]
fn absent_or_zero_artifact_identity_is_rejected() {
    let mut value = intent();
    value.artifact_sha256 = [0; 32];
    assert_eq!(value.validate(), Err(Error::Invalid));
    assert_eq!(
        Request::new(
            "r",
            Action::Propose {
                session_id: "s".into(),
                intent: value
            }
        ),
        Err(Error::Invalid)
    );
    let bytes = raw_request(|root| {
        let mut out = root.init_propose();
        out.set_session_id("s");
        let mut value = out.init_intent();
        value.set_operation_id("op");
        value.set_program("/trusted/tool");
        value.set_cwd("/trusted/work");
        value.set_execution_domain("domain");
        value.set_max_runtime_ms(1000);
    });
    assert!(Request::decode(&bytes).is_err());
}
#[test]
fn compacted_floor_requires_sufficient_sealed_checkpoint() {
    let mut value = session_info();
    value.tail = 4;
    value.floor = 3;
    assert_eq!(value.validate(), Err(Error::Invalid));
    value.checkpoint_sealed = true;
    value.checkpoint_tail = 1;
    assert_eq!(value.validate(), Err(Error::Invalid));
    value.checkpoint_tail = 2;
    assert_eq!(value.validate(), Ok(()));
}
#[test]
fn fork_metadata_exposes_fixed_source_checkpoint_identity() {
    let request = req(Action::Create {
        session_id: "session-one".into(),
        parent: Some("source".into()),
        parent_tail: 7,
    });
    let mut info = session_info();
    info.parent = Some("source".into());
    info.parent_tail = 7;
    info.parent_checkpoint_sha256 = Some(hash(b"sealed source"));
    info.checkpoint_sealed = true;
    info.checkpoint_sha256 = hash(b"sealed source");
    let reply = Reply::new(&request, Outcome::Session(info.clone())).unwrap();
    assert_eq!(
        Reply::decode_for(&request, &reply.encode().unwrap()),
        Ok(reply)
    );
    info.parent_checkpoint_sha256 = None;
    assert_eq!(info.validate(), Err(Error::Invalid));
}
#[test]
fn unused_reply_padding_must_be_canonical_zero() {
    let request = list();
    let mut bytes = raw_reply(&request, |_| {});
    // Rejected stores its failure enum at bytes 8..10. Generation occupies
    // the following UInt64 word; bytes 10..16 remain padding.
    let at = root_data(&bytes) + 15;
    bytes[at] = 0xff;
    assert_eq!(Reply::decode_for(&request, &bytes), Err(Error::Contract));
}

#[test]
fn fork_create_reply_requires_the_inherited_checkpoint_body_identity() {
    let request = req(Action::Create {
        session_id: "session-one".into(),
        parent: Some("source".into()),
        parent_tail: 7,
    });
    let mut info = session_info();
    info.parent = Some("source".into());
    info.parent_tail = 7;
    info.parent_checkpoint_sha256 = Some(hash(b"sealed source"));
    info.checkpoint_sealed = true;
    info.checkpoint_sha256 = hash(b"wrong inherited state");
    assert_wrong_reply(&request, Outcome::Session(info.clone()));
    info.checkpoint_sha256 = hash(b"sealed source");
    info.checkpoint_sealed = false;
    assert_eq!(
        Reply::new(&request, Outcome::Session(info)),
        Err(Error::Invalid)
    );
}
