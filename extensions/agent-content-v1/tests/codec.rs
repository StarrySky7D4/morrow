//! Consumer and hostile-wire checks; no Store, native owner, or approval authority.
use capnp::{message::Builder, serialize};
use morrow_agent_content_v1::{
    Action, ContentRef, Error, MAX_CONTENT_BYTES, MAX_FRAME_BYTES, MAX_QUERY_CARDS, MAX_READ_BYTES,
    Outcome, REVISION, Reply, Request, SCHEMA, VERSION, schema_digest,
};
use morrow_core::{
    content::CardSummary,
    content_change::ContentChange,
    response::{self, Response},
    runtime::{self, Command, ContentChunk},
    transaction::Lookup,
};
use sha2::{Digest, Sha256};

// The consumer owns a separately compiled view of the schema. Public codec
// constructors cannot generate many of the invalid wire values tested below.
#[allow(clippy::all, dead_code)]
mod agent_content_capnp {
    include!(concat!(env!("OUT_DIR"), "/agent_content_capnp.rs"));
}
use agent_content_capnp as wire;

fn reference() -> ContentRef {
    ContentRef {
        card_id: "card-one".into(),
        revision: 3,
        total_length: 6,
        body_sha256: Sha256::digest(b"abcdef").into(),
    }
}
fn query() -> Request {
    Request::new(
        "request-one",
        Action::Query {
            cards: vec!["card-one".into()],
        },
    )
    .unwrap()
}
fn read_request() -> Request {
    Request::new(
        "request-one",
        Action::ReadRef {
            reference: reference(),
            offset: 2,
            length: 3,
        },
    )
    .unwrap()
}
fn change(card: &str, revision: u64) -> Vec<u8> {
    Command::EditContent(ContentChange {
        operation_id: "operation-one".into(),
        card_id: card.into(),
        expected_revision: revision,
        title: "changed title".into(),
        body: b"changed body".to_vec(),
        preview_text: "changed".into(),
        attachments: None,
    })
    .encode()
    .unwrap()
}
fn proposal() -> Request {
    Request::new(
        "request-one",
        Action::ProposeMutation {
            reference: reference(),
            command: change("card-one", 3),
        },
    )
    .unwrap()
}
fn inspect() -> Request {
    Request::new(
        "request-one",
        Action::InspectOperation {
            card_id: "card-one".into(),
            operation_id: "operation-one".into(),
        },
    )
    .unwrap()
}
fn summary(request_id: &str, card_id: &str) -> Vec<u8> {
    Response {
        request_id: request_id.into(),
        outcome: response::Outcome::Summary(CardSummary {
            id: card_id.into(),
            type_id: "idea".into(),
            format_version: 1,
            revision: 3,
            title: "title".into(),
            preview_text: "preview".into(),
        }),
    }
    .encode()
    .unwrap()
}
fn chunk() -> ContentChunk {
    ContentChunk {
        card_id: "card-one".into(),
        revision: 3,
        offset: 2,
        total_length: 6,
        body_sha256: reference().body_sha256,
        bytes: b"cde".to_vec(),
    }
}
fn chunk_response(value: ContentChunk) -> Vec<u8> {
    Response {
        request_id: "request-one".into(),
        outcome: response::Outcome::ContentChunk(value),
    }
    .encode()
    .unwrap()
}
fn operation_response(card: &str, operation: &str) -> Vec<u8> {
    Response {
        request_id: "request-one".into(),
        outcome: response::Outcome::OperationResult {
            card_id: card.into(),
            operation_id: operation.into(),
            result: Lookup::Absent,
        },
    }
    .encode()
    .unwrap()
}
fn rejected() -> Outcome {
    Outcome::Rejected {
        failure: response::Failure::Denied,
    }
}
fn set_ref(mut out: wire::content_ref::Builder<'_>, value: &ContentRef) {
    out.set_card_id(value.card_id.as_str());
    out.set_revision(value.revision);
    out.set_total_length(value.total_length);
    out.set_body_sha256(&value.body_sha256);
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
        root.reborrow()
            .init_query()
            .init_cards(1)
            .set(0, "card-one");
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
        root.set_request_id(request.request_id());
        root.set_request_sha256(&request.digest());
        root.set_rejected(wire::Failure::Denied);
        edit(root);
    }
    serialize::write_message_to_words(&message)
}
fn root_data(bytes: &[u8]) -> usize {
    // Tests serialize a single segment with a near struct root; derive its
    // location from the wire pointer instead of depending on allocation order.
    assert_eq!(&bytes[..4], &[0, 0, 0, 0]);
    let pointer = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    assert_eq!(pointer & 3, 0);
    let offset = ((pointer as u32 as i32) >> 2) as isize;
    (16isize + offset * 8) as usize
}
fn set_root_u16(bytes: &mut [u8], index: usize, value: u16) {
    let start = root_data(bytes) + index * 2;
    bytes[start..start + 2].copy_from_slice(&value.to_le_bytes());
}
fn assert_rejects_reply(request: &Request, outcome: Outcome, error: Error) {
    // The direct constructor and hostile consumer path must agree. Encoding a
    // public Reply without request context deliberately exercises decode_for.
    assert_eq!(Reply::new(request, outcome.clone()), Err(error));
    let untrusted = Reply {
        request_id: request.request_id().into(),
        request_sha256: request.digest(),
        outcome,
    };
    if let Ok(bytes) = untrusted.encode() {
        assert_eq!(Reply::decode_for(&bytes, request), Err(error));
    }
}

#[test]
fn four_actions_roundtrip_with_exact_original_wire_identity() {
    for request in [query(), read_request(), proposal(), inspect()] {
        let encoded = request.encode().unwrap();
        let decoded = Request::decode(&encoded).unwrap();
        assert_eq!(decoded, request);
        assert_eq!(decoded.wire(), encoded);
        assert_eq!(decoded.digest(), <[u8; 32]>::from(Sha256::digest(&encoded)));
        assert_eq!(decoded.encode().unwrap(), encoded);
    }
}

#[test]
fn four_success_replies_roundtrip_and_bind_requests() {
    let requests = [query(), read_request(), proposal(), inspect()];
    let outcomes = [
        Outcome::Query {
            responses: vec![summary("request-one", "card-one")],
        },
        Outcome::ReadRef {
            response: chunk_response(chunk()),
        },
        Outcome::Proposed {
            operation_id: "operation-one".into(),
            proposal_sha256: requests[2].digest(),
        },
        Outcome::Operation {
            response: operation_response("card-one", "operation-one"),
        },
    ];
    for (request, outcome) in requests.iter().zip(outcomes) {
        let reply = Reply::new(request, outcome).unwrap();
        assert_eq!(
            Reply::decode_for(&reply.encode().unwrap(), request).unwrap(),
            reply
        );
    }
}

#[test]
fn all_failure_codes_roundtrip_without_granting_authority() {
    let request = proposal();
    for failure in [
        response::Failure::Denied,
        response::Failure::NotFound,
        response::Failure::RevisionConflict,
        response::Failure::OperationConflict,
        response::Failure::Capacity,
        response::Failure::Busy,
        response::Failure::Storage,
        response::Failure::CommitUnknown,
        response::Failure::Limit,
    ] {
        let reply = Reply::new(&request, Outcome::Rejected { failure }).unwrap();
        assert_eq!(
            Reply::decode_for(&reply.encode().unwrap(), &request).unwrap(),
            reply
        );
    }
}

#[test]
fn schema_identity_hashes_exact_raw_bytes() {
    assert_eq!(schema_digest(), <[u8; 32]>::from(Sha256::digest(SCHEMA)));
    let mut different_bytes = SCHEMA.to_vec();
    different_bytes
        .extend_from_slice(b"\n# harmless schema text still changes this profile's identity\n");
    let changed_digest: [u8; 32] = Sha256::digest(different_bytes).into();
    assert_ne!(changed_digest, schema_digest());
    let bytes = raw_request(|mut root| root.set_schema_sha256(&changed_digest));
    assert_eq!(Request::decode(&bytes), Err(Error::Contract));
}

#[test]
fn semantically_equal_noncanonical_frame_retains_distinct_digest() {
    let request = query();
    let mut bytes = request.encode().unwrap();
    let words = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    bytes[4..8].copy_from_slice(&(words + 1).to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    let decoded = Request::decode(&bytes).unwrap();
    assert_eq!(decoded.request_id(), request.request_id());
    assert_eq!(decoded.action(), request.action());
    assert_ne!(decoded.digest(), request.digest());
    assert_eq!(decoded.encode().unwrap(), bytes);
    let reply = Reply::new(&request, rejected()).unwrap().encode().unwrap();
    assert_eq!(Reply::decode_for(&reply, &decoded), Err(Error::Correlation));
}

#[test]
fn every_request_contract_identity_is_required() {
    let frames = [
        raw_request(|mut root| root.set_version(VERSION + 1)),
        raw_request(|mut root| root.set_revision(REVISION + 1)),
        raw_request(|mut root| root.set_schema_sha256(&[0; 32])),
        raw_request(|mut root| root.set_runtime_digest(&[0; 32])),
        raw_request(|mut root| root.set_content_digest(&[0; 32])),
        raw_request(|mut root| root.set_core_runtime_version(runtime::PROTOCOL_VERSION + 1)),
        raw_request(|mut root| root.set_schema_sha256(&[0; 31])),
    ];
    for bytes in frames {
        assert_eq!(Request::decode(&bytes), Err(Error::Contract));
    }
}

#[test]
fn every_reply_contract_identity_is_required() {
    let request = query();
    let frames = [
        raw_reply(&request, |mut root| root.set_version(VERSION + 1)),
        raw_reply(&request, |mut root| root.set_revision(REVISION + 1)),
        raw_reply(&request, |mut root| root.set_schema_sha256(&[0; 32])),
        raw_reply(&request, |mut root| root.set_runtime_digest(&[0; 32])),
        raw_reply(&request, |mut root| root.set_content_digest(&[0; 32])),
        raw_reply(&request, |mut root| {
            root.set_core_runtime_version(runtime::PROTOCOL_VERSION + 1)
        }),
    ];
    for bytes in frames {
        assert_eq!(Reply::decode_for(&bytes, &request), Err(Error::Contract));
    }
}

#[test]
fn trailing_bytes_truncations_and_unaligned_transport_are_checked() {
    let request = query();
    let valid = request.encode().unwrap();
    let mut trailing = valid.clone();
    trailing.push(0);
    assert_eq!(Request::decode(&trailing), Err(Error::Invalid));
    for prefix in 0..valid.len() {
        assert!(
            Request::decode(&valid[..prefix]).is_err(),
            "prefix {prefix}"
        );
    }
    let mut unaligned = vec![17];
    unaligned.extend_from_slice(&valid);
    assert_eq!(Request::decode(&unaligned[1..]).unwrap(), request);
    let reply = Reply::new(&request, rejected()).unwrap().encode().unwrap();
    let mut trailing_reply = reply.clone();
    trailing_reply.extend_from_slice(&[0; 8]);
    assert_eq!(
        Reply::decode_for(&trailing_reply, &request),
        Err(Error::Invalid)
    );
    for prefix in 0..reply.len() {
        assert!(Reply::decode_for(&reply[..prefix], &request).is_err());
    }
}

#[test]
fn root_capabilities_and_foreign_struct_layouts_are_rejected() {
    let request = query();
    for mut bytes in [
        request.encode().unwrap(),
        Reply::new(&request, rejected()).unwrap().encode().unwrap(),
    ] {
        bytes[8..16].copy_from_slice(&3u64.to_le_bytes());
        assert!(Request::decode(&bytes).is_err());
        assert!(Reply::decode_for(&bytes, &request).is_err());
    }
    let mut request_bytes = request.encode().unwrap();
    let pointer = u64::from_le_bytes(request_bytes[8..16].try_into().unwrap());
    request_bytes[8..16].copy_from_slice(&(pointer & !(0xffffu64 << 48)).to_le_bytes());
    assert_eq!(Request::decode(&request_bytes), Err(Error::Contract));
    // A Rejected reply never reads the payload pointer. It must still reject
    // a capability hidden there instead of silently passing the active union.
    let mut hidden_capability = raw_reply(&request, |_| {});
    let unused_pointer = root_data(&hidden_capability) + 16 + 5 * 8;
    hidden_capability[unused_pointer..unused_pointer + 8].copy_from_slice(&3u64.to_le_bytes());
    assert_eq!(
        Reply::decode_for(&hidden_capability, &request),
        Err(Error::Invalid)
    );
    let mut reply_bytes = Reply::new(&request, rejected()).unwrap().encode().unwrap();
    let pointer = u64::from_le_bytes(reply_bytes[8..16].try_into().unwrap());
    reply_bytes[8..16].copy_from_slice(&(pointer & !(0xffffu64 << 48)).to_le_bytes());
    assert_eq!(
        Reply::decode_for(&reply_bytes, &request),
        Err(Error::Contract)
    );
}

#[test]
fn request_reply_direction_cannot_be_swapped() {
    let request = query();
    let reply = Reply::new(&request, rejected()).unwrap().encode().unwrap();
    assert_eq!(Request::decode(&reply), Err(Error::Contract));
    assert_eq!(
        Reply::decode_for(request.wire(), &request),
        Err(Error::Contract)
    );
}

#[test]
fn unknown_request_actions_do_not_create_approve_or_execute_slots() {
    for ordinal in [4, 5, u16::MAX] {
        let mut bytes = raw_request(|_| {});
        set_root_u16(&mut bytes, 2, ordinal);
        assert_eq!(Request::decode(&bytes), Err(Error::Invalid));
    }
    let source = std::str::from_utf8(SCHEMA).unwrap();
    let request_fields = source
        .split("struct Request {")
        .nth(1)
        .unwrap()
        .split("struct Proposed")
        .next()
        .unwrap();
    assert!(!request_fields.contains("approve @"));
    assert!(!request_fields.contains("execute @"));
}

#[test]
fn unknown_reply_union_and_failure_codes_are_rejected() {
    let request = query();
    let mut bytes = raw_reply(&request, |_| {});
    set_root_u16(&mut bytes, 2, 5);
    assert_eq!(Reply::decode_for(&bytes, &request), Err(Error::Invalid));
    let mut bytes = raw_reply(&request, |_| {});
    set_root_u16(&mut bytes, 3, u16::MAX);
    assert_eq!(Reply::decode_for(&bytes, &request), Err(Error::Invalid));
}

#[test]
fn frame_budget_is_checked_before_parsing() {
    let bytes = vec![0; MAX_FRAME_BYTES + 1];
    assert_eq!(Request::decode(&bytes), Err(Error::Limit));
    assert_eq!(Reply::decode_for(&bytes, &query()), Err(Error::Limit));
    assert_eq!(Request::decode(&[]), Err(Error::Limit));

    // A tiny self-referential graph cannot evade traversal/nesting limits by
    // satisfying only the outer byte budget. Query.cards is redirected to a
    // one-pointer struct whose only pointer targets itself.
    let mut cyclic = raw_request(|_| {});
    let action_pointer = root_data(&cyclic) + 8 + 4 * 8;
    let pointer = u64::from_le_bytes(
        cyclic[action_pointer..action_pointer + 8]
            .try_into()
            .unwrap(),
    );
    let offset = ((pointer as u32 as i32) >> 2) as isize;
    let cards_pointer = (action_pointer as isize + 8 + offset * 8) as usize;
    let self_pointer = 0xffff_fffcu64 | (1u64 << 48);
    cyclic[cards_pointer..cards_pointer + 8].copy_from_slice(&self_pointer.to_le_bytes());
    assert!(Request::decode(&cyclic).is_err());
}

#[test]
fn query_card_count_duplicate_and_identity_limits_apply_on_wire() {
    for count in [0, MAX_QUERY_CARDS + 1] {
        let bytes = raw_request(|root| {
            let mut cards = root.init_query().init_cards(count as u32);
            for i in 0..count {
                cards.set(i as u32, format!("card-{i}").as_str());
            }
        });
        assert_eq!(Request::decode(&bytes), Err(Error::Limit));
    }
    let duplicate = raw_request(|root| {
        let mut cards = root.init_query().init_cards(2);
        cards.set(0, "card-one");
        cards.set(1, "card-one");
    });
    assert_eq!(Request::decode(&duplicate), Err(Error::Invalid));
    for card in [
        "",
        "../card",
        "C:card",
        "card\\secret",
        "card\n",
        &"x".repeat(257),
    ] {
        let bytes = raw_request(|root| root.init_query().init_cards(1).set(0, card));
        assert_eq!(Request::decode(&bytes), Err(Error::Invalid));
    }
}

#[test]
fn request_and_operation_ids_cannot_smuggle_paths_or_controls() {
    for id in ["", "/native/path", "x\\y", "x:y", "line\r", "null\0"] {
        assert_eq!(
            Request::new(
                id,
                Action::Query {
                    cards: vec!["card-one".into()]
                }
            ),
            Err(Error::Invalid)
        );
        let bytes = raw_request(|mut root| root.set_request_id(id));
        assert_eq!(Request::decode(&bytes), Err(Error::Invalid));
        assert_eq!(
            Request::new(
                "request-one",
                Action::InspectOperation {
                    card_id: "card-one".into(),
                    operation_id: id.into(),
                }
            ),
            Err(Error::Invalid)
        );
    }
}

#[test]
fn content_reference_and_read_range_limits_apply_on_wire() {
    for (revision, total, offset, length) in [
        (0, 6, 0, 1),
        (3, MAX_CONTENT_BYTES + 1, 0, 1),
        (3, 6, 7, 1),
        (3, 6, 0, 0),
        (3, 6, 0, MAX_READ_BYTES + 1),
    ] {
        let bytes = raw_request(|root| {
            let mut read = root.init_read_ref();
            read.set_offset(offset);
            read.set_length(length);
            let mut value = reference();
            value.revision = revision;
            value.total_length = total;
            set_ref(read.init_reference(), &value);
        });
        assert_eq!(Request::decode(&bytes), Err(Error::Limit));
    }
    let malformed_hash = raw_request(|root| {
        let mut read = root.init_read_ref();
        read.set_offset(0);
        read.set_length(1);
        let mut value = read.init_reference();
        value.set_card_id("card-one");
        value.set_revision(3);
        value.set_total_length(6);
        value.set_body_sha256(&[0; 31]);
    });
    assert_eq!(Request::decode(&malformed_hash), Err(Error::Invalid));
}

#[test]
fn propose_accepts_only_current_core_edit_content_and_matching_reference() {
    let unsupported = [
        Command::ReadSummary {
            request_id: "operation-one".into(),
            card_id: "card-one".into(),
        }
        .encode()
        .unwrap(),
        Command::Rename(runtime::RenameRequest {
            operation_id: "operation-one".into(),
            card_id: "card-one".into(),
            expected_revision: 3,
            title: "rename".into(),
        })
        .encode()
        .unwrap(),
    ];
    for command in unsupported {
        assert_eq!(
            Request::new(
                "request-one",
                Action::ProposeMutation {
                    reference: reference(),
                    command: command.clone()
                }
            ),
            Err(Error::Invalid)
        );
        let bytes = raw_request(|root| {
            let mut proposal = root.init_propose_mutation();
            proposal.set_command(&command);
            set_ref(proposal.init_reference(), &reference());
        });
        assert_eq!(Request::decode(&bytes), Err(Error::Invalid));
    }
    for command in [change("card-two", 3), change("card-one", 4)] {
        assert_eq!(
            Request::new(
                "request-one",
                Action::ProposeMutation {
                    reference: reference(),
                    command
                }
            ),
            Err(Error::Correlation)
        );
    }
}

#[test]
fn nested_core_command_version_and_identity_are_checked() {
    let mut command = change("card-one", 3);
    set_root_u16(&mut command, 0, runtime::PROTOCOL_VERSION + 1);
    let bytes = raw_request(|root| {
        let mut proposal = root.init_propose_mutation();
        proposal.set_command(&command);
        set_ref(proposal.init_reference(), &reference());
    });
    assert_eq!(Request::decode(&bytes), Err(Error::Contract));
    assert_eq!(
        Request::new(
            "request-one",
            Action::ProposeMutation {
                reference: reference(),
                command
            }
        ),
        Err(Error::Contract)
    );
}

#[test]
fn propose_command_payload_budget_is_checked_without_decoding() {
    for command in [vec![], vec![0; runtime::MAX_MESSAGE_BYTES + 1]] {
        assert_eq!(
            Request::new(
                "request-one",
                Action::ProposeMutation {
                    reference: reference(),
                    command: command.clone()
                }
            ),
            Err(Error::Limit)
        );
        let bytes = raw_request(|root| {
            let mut proposal = root.init_propose_mutation();
            proposal.set_command(&command);
            set_ref(proposal.init_reference(), &reference());
        });
        assert_eq!(Request::decode(&bytes), Err(Error::Limit));
    }
}

#[test]
fn reply_must_match_outer_request_id_digest_and_digest_width() {
    let request = query();
    let wrong_id = raw_reply(&request, |mut root| root.set_request_id("other-request"));
    assert_eq!(
        Reply::decode_for(&wrong_id, &request),
        Err(Error::Correlation)
    );
    let wrong_hash = raw_reply(&request, |mut root| root.set_request_sha256(&[0; 32]));
    assert_eq!(
        Reply::decode_for(&wrong_hash, &request),
        Err(Error::Correlation)
    );
    let short_hash = raw_reply(&request, |mut root| root.set_request_sha256(&[0; 31]));
    assert_eq!(
        Reply::decode_for(&short_hash, &request),
        Err(Error::Invalid)
    );
}

#[test]
fn reply_outcome_must_match_requested_action() {
    let request = proposal();
    assert_rejects_reply(
        &request,
        Outcome::Query {
            responses: vec![summary("request-one", "card-one")],
        },
        Error::Correlation,
    );
    assert_rejects_reply(
        &query(),
        Outcome::Operation {
            response: operation_response("card-one", "operation-one"),
        },
        Error::Correlation,
    );
}

#[test]
fn query_responses_must_match_card_count_order_and_nested_request_id() {
    let request = Request::new(
        "request-one",
        Action::Query {
            cards: vec!["card-one".into(), "card-two".into()],
        },
    )
    .unwrap();
    assert_rejects_reply(
        &request,
        Outcome::Query {
            responses: vec![summary("request-one", "card-one")],
        },
        Error::Correlation,
    );
    assert_rejects_reply(
        &request,
        Outcome::Query {
            responses: vec![
                summary("request-one", "card-two"),
                summary("request-one", "card-one"),
            ],
        },
        Error::Correlation,
    );
    assert_rejects_reply(
        &query(),
        Outcome::Query {
            responses: vec![summary("other-request", "card-one")],
        },
        Error::Correlation,
    );
    assert_rejects_reply(
        &query(),
        Outcome::Query {
            responses: vec![chunk_response(chunk())],
        },
        Error::Correlation,
    );
}

#[test]
fn per_card_core_rejections_preserve_ordered_result_cardinality() {
    let request = Request::new(
        "request-one",
        Action::Query {
            cards: vec!["card-one".into(), "card-two".into()],
        },
    )
    .unwrap();
    let denial = Response {
        request_id: "request-one".into(),
        outcome: response::Outcome::Rejected(response::Failure::Denied),
    }
    .encode()
    .unwrap();
    let reply = Reply::new(
        &request,
        Outcome::Query {
            responses: vec![summary("request-one", "card-one"), denial],
        },
    )
    .unwrap();
    assert_eq!(
        Reply::decode_for(&reply.encode().unwrap(), &request).unwrap(),
        reply
    );
}

#[test]
fn read_reply_must_match_all_reference_and_range_fields() {
    let request = read_request();
    let original = chunk();
    let mut wrong_card = original.clone();
    wrong_card.card_id = "card-two".into();
    let mut wrong_revision = original.clone();
    wrong_revision.revision += 1;
    let mut wrong_offset = original.clone();
    wrong_offset.offset = 1;
    let mut wrong_total = original.clone();
    wrong_total.total_length = 7;
    let mut wrong_digest = original.clone();
    wrong_digest.body_sha256 = [0; 32];
    let mut short_payload = original.clone();
    short_payload.bytes.pop();
    let mut long_payload = original.clone();
    long_payload.bytes.push(b'f');
    for value in [
        wrong_card,
        wrong_revision,
        wrong_offset,
        wrong_total,
        wrong_digest,
        short_payload,
        long_payload,
    ] {
        assert_rejects_reply(
            &request,
            Outcome::ReadRef {
                response: chunk_response(value),
            },
            Error::Correlation,
        );
    }
}

#[test]
fn final_and_empty_eof_reads_use_exact_expected_payload_length() {
    let request = Request::new(
        "request-one",
        Action::ReadRef {
            reference: reference(),
            offset: 4,
            length: 10,
        },
    )
    .unwrap();
    let mut value = chunk();
    value.offset = 4;
    value.bytes = b"ef".to_vec();
    let reply = Reply::new(
        &request,
        Outcome::ReadRef {
            response: chunk_response(value),
        },
    )
    .unwrap();
    assert!(Reply::decode_for(&reply.encode().unwrap(), &request).is_ok());
    let request = Request::new(
        "request-one",
        Action::ReadRef {
            reference: reference(),
            offset: 6,
            length: 1,
        },
    )
    .unwrap();
    let mut value = chunk();
    value.offset = 6;
    value.bytes.clear();
    let reply = Reply::new(
        &request,
        Outcome::ReadRef {
            response: chunk_response(value),
        },
    )
    .unwrap();
    assert!(Reply::decode_for(&reply.encode().unwrap(), &request).is_ok());
}

#[test]
fn proposed_reply_binds_operation_and_complete_original_request_digest() {
    let request = proposal();
    assert_rejects_reply(
        &request,
        Outcome::Proposed {
            operation_id: "other-operation".into(),
            proposal_sha256: request.digest(),
        },
        Error::Correlation,
    );
    let command_hash = match request.action() {
        Action::ProposeMutation { command, .. } => Sha256::digest(command).into(),
        _ => unreachable!(),
    };
    assert_ne!(command_hash, request.digest());
    assert_rejects_reply(
        &request,
        Outcome::Proposed {
            operation_id: "operation-one".into(),
            proposal_sha256: command_hash,
        },
        Error::Correlation,
    );
    let bytes = raw_reply(&request, |root| {
        let mut proposed = root.init_proposed();
        proposed.set_operation_id("operation-one");
        proposed.set_proposal_sha256(&[0; 31]);
    });
    assert_eq!(Reply::decode_for(&bytes, &request), Err(Error::Invalid));
}

#[test]
fn operation_reply_binds_card_and_operation_identity() {
    let request = inspect();
    for (card, operation) in [("card-two", "operation-one"), ("card-one", "operation-two")] {
        assert_rejects_reply(
            &request,
            Outcome::Operation {
                response: operation_response(card, operation),
            },
            Error::Correlation,
        );
    }
}

#[test]
fn nested_core_response_version_is_checked_on_constructor_and_consumer() {
    let request = query();
    let mut nested = summary("request-one", "card-one");
    set_root_u16(&mut nested, 0, runtime::PROTOCOL_VERSION + 1);
    assert_eq!(
        Reply::new(
            &request,
            Outcome::Query {
                responses: vec![nested.clone()]
            }
        ),
        Err(Error::Contract)
    );
    let bytes = raw_reply(&request, |root| root.init_query(1).set(0, &nested));
    assert_eq!(Reply::decode_for(&bytes, &request), Err(Error::Contract));
}

#[test]
fn reply_payload_empty_oversized_and_list_budgets_are_checked() {
    let request = query();
    for count in [0, MAX_QUERY_CARDS + 1] {
        let nested = summary("request-one", "card-one");
        let bytes = raw_reply(&request, |root| {
            let mut responses = root.init_query(count as u32);
            for index in 0..count {
                responses.set(index as u32, &nested);
            }
        });
        assert_eq!(Reply::decode_for(&bytes, &request), Err(Error::Limit));
    }
    for nested in [vec![], vec![0; runtime::MAX_MESSAGE_BYTES + 1]] {
        let bytes = raw_reply(&request, |root| root.init_query(1).set(0, &nested));
        assert_eq!(Reply::decode_for(&bytes, &request), Err(Error::Limit));
        let bytes = raw_reply(&read_request(), |mut root| root.set_read_ref(&nested));
        assert_eq!(
            Reply::decode_for(&bytes, &read_request()),
            Err(Error::Limit)
        );
        let bytes = raw_reply(&inspect(), |mut root| root.set_operation(&nested));
        assert_eq!(Reply::decode_for(&bytes, &inspect()), Err(Error::Limit));
    }
}
