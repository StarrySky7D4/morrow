use morrow_plugin_sdk::{
    protocol::{Action, CodecError, Failure, Reply, Request},
    runtime_capnp as wire,
};
fn req(action: Action) -> Request {
    Request {
        request_id: "vector-op".into(),
        card_id: "legacy-123".into(),
        action,
    }
}
fn rename() -> Request {
    req(Action::Rename {
        revision: u64::MAX - 1,
        title: "消息 🪷".into(),
    })
}
fn query() -> Request {
    req(Action::QueryOperation {
        operation_id: "vector-op".into(),
    })
}
fn attachment() -> Request {
    req(Action::ReadAttachment {
        attachment_id: "asset-1".into(),
        revision: u64::MAX,
        offset: 2,
        length: 3,
    })
}
const RENAMED: &[u8] = include_bytes!("../../tests/fixtures/renamed-reply.capnp");
const SUMMARY: &[u8] = include_bytes!("../../tests/fixtures/summary-reply.capnp");
const ATTACHMENT: &[u8] = include_bytes!("../../tests/fixtures/attachment-reply.capnp");
fn mutate(bytes: &[u8], change: impl FnOnce(wire::response::Builder<'_>)) -> Vec<u8> {
    let reader =
        capnp::serialize::read_message_from_flat_slice(&mut &bytes[..], Default::default())
            .unwrap();
    let mut builder = capnp::message::Builder::new_default();
    builder
        .set_root(reader.get_root::<wire::response::Reader>().unwrap())
        .unwrap();
    change(builder.get_root::<wire::response::Builder>().unwrap());
    capnp::serialize::write_message_to_words(&builder)
}
#[test]
fn four_requests_equal_independent_host_encoding() {
    for (request, expected) in [
        (
            rename(),
            include_bytes!("../../tests/fixtures/rename-request.capnp").as_slice(),
        ),
        (
            req(Action::ReadSummary),
            include_bytes!("../../tests/fixtures/summary-request.capnp").as_slice(),
        ),
        (
            query(),
            include_bytes!("../../tests/fixtures/query-request.capnp").as_slice(),
        ),
        (
            attachment(),
            include_bytes!("../../tests/fixtures/attachment-request.capnp").as_slice(),
        ),
    ] {
        assert_eq!(request.encode().unwrap(), expected);
    }
}
#[test]
fn decodes_every_host_response_and_exact_u64() {
    let Reply::Renamed(r) = rename().decode_reply(RENAMED).unwrap() else {
        panic!()
    };
    assert_eq!(r.revision, u64::MAX);
    assert_eq!(r.sha256, [42; 32]);
    assert_eq!(r.event_id, "event-1");
    let Reply::Summary(s) = req(Action::ReadSummary).decode_reply(SUMMARY).unwrap() else {
        panic!()
    };
    assert_eq!(s.revision, u64::MAX);
    assert_eq!(s.title, "消息 🪷");
    assert_eq!(s.preview, "preview");
    assert_eq!(s.type_id, "morrow.note");
    assert_eq!(
        rename()
            .decode_reply(include_bytes!("../../tests/fixtures/denied-reply.capnp"))
            .unwrap(),
        Reply::Rejected(Failure::Denied)
    );
    assert!(matches!(
        query()
            .decode_reply(include_bytes!("../../tests/fixtures/absent-reply.capnp"))
            .unwrap(),
        Reply::OperationResult { result: None, .. }
    ));
    let Reply::OperationResult {
        result: Some(r), ..
    } = query()
        .decode_reply(include_bytes!("../../tests/fixtures/committed-reply.capnp"))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(r.revision, u64::MAX);
    let Reply::Attachment(p) = attachment().decode_reply(ATTACHMENT).unwrap() else {
        panic!()
    };
    assert_eq!(p.bytes, [0, 255, 1]);
    assert_eq!((p.offset, p.total_length), (2, 5));
    assert_eq!(p.revision, u64::MAX);
}
#[test]
fn rejects_stale_contract_or_request_identity() {
    let r = rename();
    for bytes in [
        mutate(RENAMED, |mut v| v.set_protocol_version(0)),
        mutate(RENAMED, |mut v| v.set_runtime_digest(&[0; 32])),
        mutate(RENAMED, |mut v| v.set_content_digest(&[0; 32])),
    ] {
        assert_eq!(r.decode_reply(&bytes), Err(CodecError::Contract));
    }
    assert_eq!(
        r.decode_reply(&mutate(RENAMED, |mut v| v.set_request_id("wrong"))),
        Err(CodecError::Correlation)
    );
}
#[test]
fn rejects_valid_but_unrelated_successes() {
    assert_eq!(rename().decode_reply(SUMMARY), Err(CodecError::Correlation));
    let mut r = rename();
    r.card_id = "another".into();
    assert_eq!(r.decode_reply(RENAMED), Err(CodecError::Correlation));
    r = rename();
    r.action = Action::Rename {
        revision: u64::MAX,
        title: String::new(),
    };
    assert_eq!(r.decode_reply(RENAMED), Err(CodecError::Correlation));
    let r = req(Action::QueryOperation {
        operation_id: "another".into(),
    });
    assert_eq!(
        r.decode_reply(include_bytes!("../../tests/fixtures/committed-reply.capnp")),
        Err(CodecError::Correlation)
    );
    let r = req(Action::ReadAttachment {
        attachment_id: "asset-1".into(),
        revision: u64::MAX,
        offset: 1,
        length: 3,
    });
    assert_eq!(r.decode_reply(ATTACHMENT), Err(CodecError::Correlation));
    let r = req(Action::ReadAttachment {
        attachment_id: "asset-1".into(),
        revision: u64::MAX,
        offset: 2,
        length: 2,
    });
    assert_eq!(r.decode_reply(ATTACHMENT), Err(CodecError::Correlation));
}
#[test]
fn rejects_malformed_trailing_or_oversized_messages() {
    for size in 0..RENAMED.len() {
        assert!(rename().decode_reply(&RENAMED[..size]).is_err());
    }
    let mut bytes = RENAMED.to_vec();
    bytes.extend_from_slice(&[0; 8]);
    assert!(rename().decode_reply(&bytes).is_err());
    assert_eq!(
        rename().decode_reply(&vec![0; 65537]),
        Err(CodecError::Limit)
    );
}
#[test]
fn rejects_invalid_reply_fields_and_nested_receipt() {
    let bytes = mutate(RENAMED, |v| {
        let wire::response::Renamed(r) = v.which().unwrap() else {
            panic!()
        };
        r.unwrap().set_content_sha256(&[0; 31]);
    });
    assert!(rename().decode_reply(&bytes).is_err());
    let bytes = mutate(SUMMARY, |v| {
        let wire::response::Summary(r) = v.which().unwrap() else {
            panic!()
        };
        r.unwrap().set_format_version(0);
    });
    assert!(req(Action::ReadSummary).decode_reply(&bytes).is_err());
    let bytes = mutate(ATTACHMENT, |v| {
        let wire::response::AttachmentChunk(r) = v.which().unwrap() else {
            panic!()
        };
        r.unwrap().set_total_length(4);
    });
    assert!(attachment().decode_reply(&bytes).is_err());
    let bytes = mutate(
        include_bytes!("../../tests/fixtures/committed-reply.capnp"),
        |v| {
            let wire::response::OperationResult(r) = v.which().unwrap() else {
                panic!()
            };
            let wire::operation_result::LocallyCommitted(r) = r.unwrap().which().unwrap() else {
                panic!()
            };
            r.unwrap().set_operation_id("wrong");
        },
    );
    assert_eq!(query().decode_reply(&bytes), Err(CodecError::Correlation));
}
#[test]
fn request_bounds_prevent_invalid_submissions() {
    let mut r = rename();
    r.request_id = "bad/id".into();
    assert!(r.encode().is_err());
    r = rename();
    r.card_id = "a".repeat(257);
    assert!(r.encode().is_err());
    r = rename();
    r.action = Action::Rename {
        revision: 1,
        title: "a".repeat(16385),
    };
    assert_eq!(r.encode(), Err(CodecError::Limit));
    for length in [0, 32769] {
        assert!(
            req(Action::ReadAttachment {
                attachment_id: "asset-1".into(),
                revision: 1,
                offset: 0,
                length
            })
            .encode()
            .is_err()
        );
    }
}

#[test]
fn content_matches_independent_host_fixtures_and_binds_body_parts() {
    let edit = req(Action::EditContent {
        revision: u64::MAX - 1,
        title: "消息 🪷".into(),
        body: vec![0, 255, 42],
        preview: "preview".into(),
    });
    let create = req(Action::CreateContent {
        type_id: "morrow.note".into(),
        format_version: 1,
        title: "消息 🪷".into(),
        body: vec![0, 255, 42],
    });
    let read = req(Action::ReadContent {
        revision: u64::MAX,
        offset: 2,
        length: 3,
    });
    for (r, bytes) in [
        (
            &edit,
            include_bytes!("../../tests/fixtures/content-edit-request.capnp").as_slice(),
        ),
        (
            &create,
            include_bytes!("../../tests/fixtures/content-create-request.capnp").as_slice(),
        ),
        (
            &read,
            include_bytes!("../../tests/fixtures/content-read-request.capnp").as_slice(),
        ),
    ] {
        assert_eq!(r.encode().unwrap(), bytes);
        Request::decode(bytes).unwrap();
    }
    let committed = include_bytes!("../../tests/fixtures/content-committed-reply.capnp");
    assert!(matches!(
        edit.decode_reply(committed).unwrap(),
        Reply::ContentCommitted(_)
    ));
    assert_eq!(create.decode_reply(committed), Err(CodecError::Correlation));
    let bytes = include_bytes!("../../tests/fixtures/content-reply.capnp");
    let Reply::Content(part) = read.decode_reply(bytes).unwrap() else {
        panic!()
    };
    assert_eq!(part.bytes, [0, 255, 42]);
    assert_eq!(part.sha256, [9; 32]);
    for wrong in [
        req(Action::ReadContent {
            revision: u64::MAX - 1,
            offset: 2,
            length: 3,
        }),
        req(Action::ReadContent {
            revision: u64::MAX,
            offset: 1,
            length: 3,
        }),
        req(Action::ReadContent {
            revision: u64::MAX,
            offset: 2,
            length: 2,
        }),
    ] {
        assert_eq!(wrong.decode_reply(bytes), Err(CodecError::Correlation));
    }
}
