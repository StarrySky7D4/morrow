//! Actual fixed-schema codec tests. These frames confer no live IO authority.
use capnp::{message::Builder, serialize};
use morrow_core::{
    Error,
    io::{
        self, Action, Header, HttpOutcome, HttpSubmission, Request, Response, Status,
        UnsupportedAction,
    },
    io_capnp as wire,
};
use sha2::{Digest, Sha256};
fn request(make: impl FnOnce(wire::request::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::request::Builder>();
    root.set_version(io::VERSION);
    root.set_schema_sha256(&io::schema_digest());
    root.set_call_id(7);
    make(root);
    serialize::write_message_to_words(&message)
}
fn response(req: &Request, make: impl FnOnce(wire::response::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::response::Builder>();
    root.set_version(io::VERSION);
    root.set_schema_sha256(&io::schema_digest());
    root.set_call_id(req.call_id());
    root.set_request_sha256(&req.digest());
    root.set_status(Status::Completed);
    root.set_eof(true);
    make(root);
    serialize::write_message_to_words(&message)
}
fn sample() -> Request {
    Request::encode_read(7, &[4; 32], 0, 3).unwrap()
}
#[test]
fn read_roundtrip_preserves_u64_values_binary_reference_and_exact_frame_digest() {
    for (call, offset, limit) in [
        (0, 0, 0),
        (u64::MAX, u64::MAX, 1),
        (1, u64::MAX - 65536, 65536),
    ] {
        let reference = std::array::from_fn(|i| i as u8);
        let req = Request::encode_read(call, &reference, offset, limit).unwrap();
        assert_eq!(req.call_id(), call);
        assert_eq!(
            req.action(),
            &Action::Read {
                reference,
                offset,
                limit
            }
        );
        assert_eq!(req.digest(), <[u8; 32]>::from(Sha256::digest(req.bytes())));
        assert_eq!(Request::decode(req.bytes()).unwrap(), req);
    }
}
#[test]
fn finish_cancel_and_empty_reference_value_are_decoded_without_grant_claim() {
    for req in [
        Request::encode_finish(7, &[0; 32]).unwrap(),
        Request::encode_cancel(7, &[0; 32]).unwrap(),
    ] {
        assert_eq!(Request::decode(req.bytes()).unwrap(), req);
        let bytes = Response::encode(&req, Status::Completed, b"", 0, true).unwrap();
        assert_eq!(
            Response::decode(&req, &bytes).unwrap().status,
            Status::Completed
        );
        assert!(Response::encode(&req, Status::Completed, b"not a read", 0, true).is_err());
        assert!(
            Response::decode(&req, &response(&req, |mut r| r.set_bytes(b"not a read"))).is_err()
        );
    }
}
#[test]
fn each_known_unimplemented_union_is_explicitly_unsupported_and_correlated() {
    for (which, expected) in [
        (0, UnsupportedAction::Submit),
        (1, UnsupportedAction::Poll),
        (2, UnsupportedAction::Write),
        (3, UnsupportedAction::QueryOperation),
    ] {
        let bytes = request(|mut root| match which {
            0 => {
                root.init_submit().set_file_read(&[5; 32]);
            }
            1 => root.set_poll(&[5; 32]),
            2 => {
                root.init_write().set_reference(&[5; 32]);
            }
            3 => root.set_query_operation(b"op"),
            _ => unreachable!(),
        });
        let req = Request::decode(&bytes).unwrap();
        assert_eq!(req.action(), &Action::Unsupported(expected));
        let reply = Response::encode(&req, Status::Unsupported, b"", 0, true).unwrap();
        assert_eq!(
            Response::decode(&req, &reply).unwrap().status,
            Status::Unsupported
        );
        assert!(Response::encode(&req, Status::Completed, b"", 0, true).is_err());
        assert!(Response::decode(&req, &response(&req, |_| {})).is_err());
    }
}
#[test]
fn read_response_enforces_limit_offset_overflow_empty_progress_and_eof() {
    let req = sample();
    for (payload, offset, eof) in [
        (&b"abc"[..], 0, true),
        (&b"a"[..], 0, false),
        (&b""[..], 0, true),
    ] {
        let raw = Response::encode(&req, Status::Completed, payload, offset, eof).unwrap();
        let result = Response::decode(&req, &raw).unwrap();
        assert_eq!(
            (result.payload, result.offset, result.eof),
            (payload.to_vec(), offset, eof)
        );
    }
    for (payload, offset, eof) in [
        (&b"abcd"[..], 0, true),
        (&b"a"[..], 1, true),
        (&b""[..], 0, false),
    ] {
        assert!(Response::encode(&req, Status::Completed, payload, offset, eof).is_err());
        let raw = response(&req, |mut root| {
            root.set_bytes(payload);
            root.set_offset(offset);
            root.set_eof(eof);
        });
        assert!(Response::decode(&req, &raw).is_err());
    }
    let end = Request::encode_read(7, &[4; 32], u64::MAX, 1).unwrap();
    assert!(Response::encode(&end, Status::Completed, b"a", u64::MAX, true).is_err());
    let raw = Response::encode(&end, Status::Completed, b"", u64::MAX, true).unwrap();
    Response::decode(&end, &raw).unwrap();
}
#[test]
fn zero_limit_is_bounded_default_and_payload_upper_bound_is_enforced() {
    let req = Request::encode_read(7, &[1; 32], 0, 0).unwrap();
    let payload = vec![0xa5; io::MAX_PAYLOAD_BYTES];
    let raw = Response::encode(&req, Status::Completed, &payload, 0, true).unwrap();
    assert_eq!(Response::decode(&req, &raw).unwrap().payload, payload);
    let oversized = vec![0x5a; io::MAX_PAYLOAD_BYTES + 1];
    assert!(matches!(
        Response::encode(&req, Status::Completed, &oversized, 0, true),
        Err(Error::Limit)
    ));
    assert!(Response::decode(&req, &response(&req, |mut r| r.set_bytes(&oversized))).is_err());
}
#[test]
fn response_requires_exact_request_hash_even_with_identical_call_id() {
    let req = sample();
    let reply = Response::encode(&req, Status::Completed, b"abc", 0, true).unwrap();
    for other in [
        Request::encode_read(8, &[4; 32], 0, 3).unwrap(),
        Request::encode_read(7, &[3; 32], 0, 3).unwrap(),
        Request::encode_read(7, &[4; 32], 1, 3).unwrap(),
        Request::encode_read(7, &[4; 32], 0, 2).unwrap(),
    ] {
        assert!(Response::decode(&other, &reply).is_err());
    }
    for n in [0, 31, 32, 33] {
        let raw = response(&req, |mut r| r.set_request_sha256(&vec![0; n]));
        assert!(Response::decode(&req, &raw).is_err());
    }
    assert!(Response::decode(&req, &response(&req, |mut r| r.set_call_id(8))).is_err());
}
#[test]
fn all_known_statuses_have_no_unexpected_result_body_and_invalid_is_rejected() {
    let req = sample();
    for status in [
        Status::Accepted,
        Status::Pending,
        Status::Denied,
        Status::Revoked,
        Status::Expired,
        Status::Unsupported,
        Status::Quota,
        Status::NotFound,
        Status::Conflict,
        Status::Cancelled,
        Status::OutcomeUnknown,
        Status::EvidenceUnavailable,
        Status::Failed,
    ] {
        let bytes = Response::encode(&req, status, b"", 0, true).unwrap();
        assert_eq!(Response::decode(&req, &bytes).unwrap().status, status);
        for (payload, offset, eof) in [
            (&b"x"[..], 0, true),
            (&b""[..], 1, true),
            (&b""[..], 0, false),
        ] {
            assert!(Response::encode(&req, status, payload, offset, eof).is_err());
            assert!(
                Response::decode(
                    &req,
                    &response(&req, |mut r| {
                        r.set_status(status);
                        r.set_bytes(payload);
                        r.set_offset(offset);
                        r.set_eof(eof);
                    })
                )
                .is_err()
            );
        }
    }
    assert!(Response::encode(&req, Status::Invalid, b"", 0, true).is_err());
    assert!(
        Response::decode(&req, &response(&req, |mut r| r.set_status(Status::Invalid))).is_err()
    );
}
#[test]
fn response_rejects_extra_resource_http_status_and_headers() {
    let req = sample();
    for n in 0..3 {
        let raw = response(&req, |mut r| match n {
            0 => r.set_reference(&[1; 32]),
            1 => r.set_http_status(200),
            2 => {
                let mut h = r.init_headers(1);
                h.reborrow().get(0).set_name("Content-Type");
            }
            _ => unreachable!(),
        });
        assert!(Response::decode(&req, &raw).is_err());
    }
}
#[test]
fn wrong_versions_schema_sizes_reference_sizes_and_read_limits_are_rejected() {
    let req = sample();
    for version in [0, 2, u16::MAX] {
        assert!(
            Request::decode(&request(|mut r| {
                r.set_version(version);
                r.set_finish(&[1; 32]);
            }))
            .is_err()
        );
        assert!(Response::decode(&req, &response(&req, |mut r| r.set_version(version))).is_err());
    }
    for length in [0, 31, 32, 33] {
        let wrong = vec![0; length];
        assert!(
            Request::decode(&request(|mut r| {
                r.set_schema_sha256(&wrong);
                r.set_finish(&[1; 32]);
            }))
            .is_err()
        );
        assert!(
            Response::decode(&req, &response(&req, |mut r| r.set_schema_sha256(&wrong))).is_err()
        );
    }
    for length in [0, 31, 33] {
        for action in 0..3 {
            let raw = request(|mut r| match action {
                0 => r.init_read().set_reference(&vec![1; length]),
                1 => r.set_finish(&vec![1; length]),
                2 => r.set_cancel(&vec![1; length]),
                _ => unreachable!(),
            });
            assert!(Request::decode(&raw).is_err());
        }
    }
    assert!(Request::encode_read(7, &[1; 32], 0, 65537).is_err());
    let raw = request(|r| {
        let mut read = r.init_read();
        read.set_reference(&[1; 32]);
        read.set_limit(65537);
    });
    assert!(Request::decode(&raw).is_err());
}
#[test]
fn arbitrary_transport_alignment_is_supported_without_reencoding_originals() {
    let req = sample();
    let reply = Response::encode(&req, Status::Completed, b"abc", 0, true).unwrap();
    for prefix in 0..8 {
        let mut buffer = vec![0; prefix];
        buffer.extend_from_slice(req.bytes());
        let decoded = Request::decode(&buffer[prefix..]).unwrap();
        assert_eq!(decoded, req);
        let mut buffer = vec![0; prefix];
        buffer.extend_from_slice(&reply);
        assert_eq!(
            Response::decode(&decoded, &buffer[prefix..])
                .unwrap()
                .payload,
            b"abc"
        );
    }
}
#[test]
fn truncated_tail_concatenated_and_oversized_frames_are_rejected() {
    let req = sample();
    let reply = Response::encode(&req, Status::Completed, b"abc", 0, true).unwrap();
    for n in 0..req.bytes().len() {
        assert!(
            Request::decode(&req.bytes()[..n]).is_err(),
            "request prefix {n}"
        );
    }
    for n in 0..reply.len() {
        assert!(
            Response::decode(&req, &reply[..n]).is_err(),
            "response prefix {n}"
        );
    }
    for suffix in [&b"\0"[..], req.bytes()] {
        let mut bytes = req.bytes().to_vec();
        bytes.extend_from_slice(suffix);
        assert!(Request::decode(&bytes).is_err());
        let mut bytes = reply.clone();
        bytes.extend_from_slice(suffix);
        assert!(Response::decode(&req, &bytes).is_err());
    }
    let big = vec![0; io::MAX_FRAME_BYTES + 1];
    assert!(matches!(Request::decode(&big), Err(Error::Limit)));
    assert!(matches!(Response::decode(&req, &big), Err(Error::Limit)));
}
#[test]
fn segment_allocation_bombs_and_recursive_structures_are_bounded() {
    let req = sample();
    let mut huge = 0u32.to_le_bytes().to_vec();
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    let mut many = 511u32.to_le_bytes().to_vec();
    for _ in 0..512 {
        many.extend_from_slice(&4096u32.to_le_bytes());
    }
    many.extend_from_slice(&0u32.to_le_bytes());
    // One pointer-only struct points to its own pointer word (signed offset -1).
    let recursive = vec![0, 0, 0, 0, 1, 0, 0, 0, 0xfc, 0xff, 0xff, 0xff, 0, 0, 1, 0];
    for bytes in [huge, many, recursive] {
        assert!(Request::decode(&bytes).is_err());
        assert!(Response::decode(&req, &bytes).is_err());
    }
}
// Locate generated-schema struct data in a one-segment test frame. This mutates
// discriminants that safe generated builders deliberately cannot construct.
fn struct_target(bytes: &[u8], pointer: usize) -> (usize, usize) {
    let word = u64::from_le_bytes(bytes[pointer..pointer + 8].try_into().unwrap());
    assert_eq!(word & 3, 0);
    let relative = ((word as u32 as i32) >> 2) as i64;
    let start = (pointer as i64 + 8 + relative * 8) as usize;
    (start, ((word >> 32) & 0xffff) as usize)
}
#[test]
fn unknown_outer_or_nested_union_and_response_status_tags_are_rejected() {
    let req = sample();
    let mut bytes = req.bytes().to_vec();
    assert_eq!(&bytes[..4], &0u32.to_le_bytes());
    let (root, _) = struct_target(&bytes, 8);
    bytes[root + 2..root + 4].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(Request::decode(&bytes).is_err());
    let mut bytes = request(|root| {
        root.init_submit().set_file_read(&[1; 32]);
    });
    assert!(Request::decode(&bytes).is_ok());
    let (root, data_words) = struct_target(&bytes, 8);
    let (submit, _) = struct_target(&bytes, root + data_words * 8 + 8);
    bytes[submit + 8..submit + 10].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(Request::decode(&bytes).is_err());
    let mut bytes = Response::encode(&req, Status::Completed, b"abc", 0, true).unwrap();
    let (root, _) = struct_target(&bytes, 8);
    bytes[root + 2..root + 4].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(Response::decode(&req, &bytes).is_err());
}
#[test]
fn equivalent_fields_with_different_original_wire_bytes_do_not_share_responses() {
    let req = sample();
    let mut modified = req.bytes().to_vec();
    let (root, _) = struct_target(&modified, 8);
    // Reserved data bytes do not change any currently interpreted request field.
    modified[root + 4] = 1;
    let other = Request::decode(&modified).unwrap();
    assert_eq!(other.action(), req.action());
    assert_eq!(other.call_id(), req.call_id());
    assert_ne!(other.digest(), req.digest());
    let reply = Response::encode(&req, Status::Completed, b"abc", 0, true).unwrap();
    assert!(Response::decode(&other, &reply).is_err());
}
#[test]
fn all_known_submission_variants_are_unsupported_but_wrong_variant_pointer_is_rejected() {
    // Variant 5 is the HTTP submission and is covered separately.
    for variant in (0..12).filter(|variant| *variant != 5) {
        let raw = request(|r| {
            let mut s = r.init_submit();
            match variant {
                0 => s.set_file_read(&[1; 32]),
                1 => s.set_file_list(&[1; 32]),
                2 => {
                    s.init_file_create().set_name("new");
                }
                3 => {
                    s.init_file_replace().set_name("old");
                }
                4 => {
                    s.init_file_delete().set_name("old");
                }
                5 => {
                    s.init_http_request().set_method("GET");
                }
                6 => s.set_http_listen(&[1; 32]),
                7 => {
                    s.init_http_publish().set_handler("serve");
                }
                8 => s.set_http_unpublish(&[1; 32]),
                9 => {
                    s.init_web_socket_connect().set_method("GET");
                }
                10 => s.set_service_accept(&[1; 32]),
                11 => {
                    s.init_service_reply().set_status(200);
                }
                _ => unreachable!(),
            }
        });
        let req = Request::decode(&raw).unwrap();
        assert_eq!(
            req.action(),
            &Action::Unsupported(UnsupportedAction::Submit)
        );
        let bytes = Response::encode(&req, Status::Unsupported, b"", 0, true).unwrap();
        assert_eq!(
            Response::decode(&req, &bytes).unwrap().status,
            Status::Unsupported
        );
    }
    let mut raw = request(|r| {
        r.init_submit().init_file_create().set_name("new");
    });
    let (root, data_words) = struct_target(&raw, 8);
    let (submit, _) = struct_target(&raw, root + data_words * 8 + 8);
    // fileCreate struct left in place, but discriminant claims fileRead Data.
    raw[submit + 8..submit + 10].copy_from_slice(&0u16.to_le_bytes());
    assert!(Request::decode(&raw).is_err());
}

fn http_submission() -> HttpSubmission {
    HttpSubmission {
        operation_id: b"operation-1".to_vec(),
        deadline_ms: 30_000,
        endpoint: b"endpoint-ref".to_vec(),
        method: "POST".into(),
        relative_target: "/v1/items?q=1".into(),
        headers: vec![
            Header {
                name: "accept".into(),
                value: b"application/json".to_vec(),
            },
            Header {
                name: "x-trace".into(),
                value: b"trace-123".to_vec(),
            },
        ],
        body: b"{\"a\":1}".to_vec(),
        credential: b"credential-ref".to_vec(),
    }
}
fn http_rejected(submission: &HttpSubmission) -> bool {
    Request::encode_http_submit(1, submission).is_err()
}

#[test]
fn http_submission_roundtrip_binds_exact_references_and_headers() {
    let submission = http_submission();
    let req = Request::encode_http_submit(9, &submission).unwrap();
    assert_eq!(req.call_id(), 9);
    match req.action() {
        Action::SubmitHttp(decoded) => assert_eq!(decoded, &submission),
        other => panic!("expected HTTP submission, got {other:?}"),
    }
    let outcome = HttpOutcome {
        status: Status::Completed,
        http_status: 201,
        headers: vec![Header {
            name: "content-type".into(),
            value: b"application/json".to_vec(),
        }],
        body: b"{\"ok\":true}".to_vec(),
    };
    let completed_bytes = Response::encode_http(&req, &outcome).unwrap();
    assert_eq!(
        Response::decode_http(&req, &completed_bytes).unwrap(),
        outcome
    );
    // The read profile cannot claim an HTTP submission as a read success.
    assert!(matches!(
        Response::decode(&req, &completed_bytes),
        Err(Error::Invalid(_))
    ));
    // A remote 4xx is still a completed outcome with its real status.
    let not_found = HttpOutcome {
        status: Status::Completed,
        http_status: 404,
        headers: vec![],
        body: b"missing".to_vec(),
    };
    let bytes = Response::encode_http(&req, &not_found).unwrap();
    assert_eq!(Response::decode_http(&req, &bytes).unwrap(), not_found);
    // A transport-unknown state carries no remote fields at all.
    let unknown = HttpOutcome {
        status: Status::OutcomeUnknown,
        http_status: 0,
        headers: vec![],
        body: vec![],
    };
    let bytes = Response::encode_http(&req, &unknown).unwrap();
    assert_eq!(Response::decode_http(&req, &bytes).unwrap(), unknown);
}

#[test]
fn http_submission_rejects_runtime_headers_bad_targets_and_bounds() {
    let base = http_submission();
    let mut lower = base.clone();
    lower.method = "post".into();
    assert!(http_rejected(&lower));
    let mut scheme = base.clone();
    scheme.relative_target = "https://evil.example/x".into();
    assert!(http_rejected(&scheme));
    let mut relative = base.clone();
    relative.relative_target = "x".into();
    assert!(http_rejected(&relative));
    let mut fragment = base.clone();
    fragment.relative_target = "/x#frag".into();
    assert!(http_rejected(&fragment));
    let mut crlf = base.clone();
    crlf.headers[0].value = b"a\r\nx".to_vec();
    assert!(http_rejected(&crlf));
    let mut host = base.clone();
    host.headers.push(Header {
        name: "Host".into(),
        value: b"evil.example".to_vec(),
    });
    assert!(http_rejected(&host));
    let mut framing = base.clone();
    framing.headers.push(Header {
        name: "content-length".into(),
        value: b"1".to_vec(),
    });
    assert!(http_rejected(&framing));
    let mut many = base.clone();
    many.headers = vec![
        Header {
            name: "x".into(),
            value: vec![],
        };
        io::MAX_HEADERS + 1
    ];
    assert!(http_rejected(&many));
    let mut oversized_body = base.clone();
    oversized_body.body = vec![0; io::MAX_PAYLOAD_BYTES + 1];
    assert!(http_rejected(&oversized_body));
    let mut deadline = base.clone();
    deadline.deadline_ms = io::MAX_SUBMIT_DEADLINE_MS + 1;
    assert!(http_rejected(&deadline));
    let mut no_operation = base.clone();
    no_operation.operation_id = vec![];
    assert!(http_rejected(&no_operation));
    let mut no_target = base.clone();
    no_target.relative_target = String::new();
    assert!(http_rejected(&no_target));
}

#[test]
fn http_outcome_rejects_bad_status_fields_and_frame_binding() {
    let req = Request::encode_http_submit(1, &http_submission()).unwrap();
    let completed = |http_status, body: &[u8]| HttpOutcome {
        status: Status::Completed,
        http_status,
        headers: vec![],
        body: body.to_vec(),
    };
    assert!(Response::encode_http(&req, &completed(99, b"")).is_err());
    assert!(Response::encode_http(&req, &completed(600, b"")).is_err());
    assert!(
        Response::encode_http(&req, &completed(200, &vec![0; io::MAX_PAYLOAD_BYTES + 1])).is_err()
    );
    let remote_fields_on_denied = HttpOutcome {
        status: Status::Denied,
        http_status: 200,
        headers: vec![],
        body: vec![],
    };
    assert!(Response::encode_http(&req, &remote_fields_on_denied).is_err());
    let read = Request::encode_read(1, &[4; 32], 0, 3).unwrap();
    assert!(Response::encode_http(&read, &completed(200, b"")).is_err());
    let good = Response::encode_http(&req, &completed(200, b"ok")).unwrap();
    let other = Request::encode_http_submit(2, &http_submission()).unwrap();
    assert!(matches!(
        Response::decode_http(&other, &good),
        Err(Error::Integrity)
    ));
    assert!(matches!(
        Response::decode_http(&read, &good),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn crafted_http_submission_is_validated_on_decode() {
    let raw = request(|mut root| {
        let mut submit = root.reborrow().init_submit();
        submit.set_operation_id(b"operation-1");
        submit.set_deadline_ms(0);
        let mut http = submit.init_http_request();
        http.set_endpoint(b"endpoint-ref");
        http.set_method("POST");
        http.set_relative_target("/x");
        let mut headers = http.reborrow().init_headers(1);
        let mut header = headers.reborrow().get(0);
        header.set_name("Host");
        header.set_value(b"evil.example");
        http.set_body(b"");
        http.set_credential(b"credential-ref");
    });
    assert!(matches!(
        Request::decode(&raw),
        Err(Error::Invalid("HTTP runtime header"))
    ));
    let raw = request(|mut root| {
        let mut submit = root.reborrow().init_submit();
        submit.set_operation_id(b"operation-1");
        let mut http = submit.init_http_request();
        http.set_endpoint(b"endpoint-ref");
        http.set_method("GET");
        http.set_relative_target("/x");
        http.set_credential(b"credential-ref");
    });
    assert!(matches!(
        Request::decode(&raw).unwrap().action(),
        Action::SubmitHttp(_)
    ));
}

fn raw_http_submission(value: &HttpSubmission) -> Vec<u8> {
    request(|root| {
        let mut submit = root.init_submit();
        submit.set_operation_id(&value.operation_id);
        submit.set_deadline_ms(value.deadline_ms);
        let mut http = submit.init_http_request();
        http.set_endpoint(&value.endpoint);
        http.set_method(&value.method);
        http.set_relative_target(&value.relative_target);
        http.set_body(&value.body);
        http.set_credential(&value.credential);
        let mut headers = http.init_headers(value.headers.len() as u32);
        for (i, header) in value.headers.iter().enumerate() {
            let mut item = headers.reborrow().get(i as u32);
            item.set_name(&header.name);
            item.set_value(&header.value);
        }
    })
}

#[test]
fn http_rejects_guest_authority_headers_on_encode_and_wire_decode() {
    for name in [
        "Authorization",
        "cOoKiE",
        "HOST",
        "Connection",
        "Content-Length",
        "Transfer-Encoding",
        "Upgrade",
        "TE",
        "Trailer",
        "Expect",
        "Keep-Alive",
        "Proxy-Authorization",
        "pRoXy-Authenticate",
        "proxy-custom",
    ] {
        let mut value = http_submission();
        value.headers = vec![Header {
            name: name.into(),
            value: b"guest-chosen".to_vec(),
        }];
        assert!(Request::encode_http_submit(1, &value).is_err(), "{name}");
        assert!(
            Request::decode(&raw_http_submission(&value)).is_err(),
            "{name}"
        );
    }
}

#[test]
fn http_rejects_ambiguous_targets_on_encode_and_wire_decode() {
    for target in [
        "//evil.example/x",
        "/\\evil.example/x",
        "/x\\y",
        "/x#fragment",
    ] {
        let mut value = http_submission();
        value.relative_target = target.into();
        assert!(Request::encode_http_submit(1, &value).is_err(), "{target}");
        assert!(
            Request::decode(&raw_http_submission(&value)).is_err(),
            "{target}"
        );
    }
}

#[test]
fn http_header_control_bytes_are_rejected_in_both_directions() {
    let req = Request::encode_http_submit(1, &http_submission()).unwrap();
    for byte in (0u8..=31).filter(|b| *b != b'\t').chain([127]) {
        let header = Header {
            name: "x-data".into(),
            value: vec![b'a', byte, b'b'],
        };
        let mut value = http_submission();
        value.headers = vec![header.clone()];
        assert!(Request::encode_http_submit(1, &value).is_err(), "{byte}");
        assert!(
            Request::decode(&raw_http_submission(&value)).is_err(),
            "{byte}"
        );
        let outcome = HttpOutcome {
            status: Status::Completed,
            http_status: 200,
            headers: vec![header.clone()],
            body: vec![],
        };
        assert!(Response::encode_http(&req, &outcome).is_err(), "{byte}");
        let raw = response(&req, |mut root| {
            root.set_http_status(200);
            let mut headers = root.init_headers(1);
            let mut item = headers.reborrow().get(0);
            item.set_name(&header.name);
            item.set_value(&header.value);
        });
        assert!(Response::decode_http(&req, &raw).is_err(), "{byte}");
    }
    let mut value = http_submission();
    value.headers[0].value = b"a\tb".to_vec();
    let req = Request::encode_http_submit(1, &value).unwrap();
    let outcome = HttpOutcome {
        status: Status::Completed,
        http_status: 200,
        headers: value.headers,
        body: vec![],
    };
    let raw = Response::encode_http(&req, &outcome).unwrap();
    assert_eq!(Response::decode_http(&req, &raw).unwrap(), outcome);
}

#[test]
fn http_invalid_result_status_is_rejected_on_encode_and_wire_decode() {
    let req = Request::encode_http_submit(1, &http_submission()).unwrap();
    let outcome = HttpOutcome {
        status: Status::Invalid,
        http_status: 0,
        headers: vec![],
        body: vec![],
    };
    assert!(Response::encode_http(&req, &outcome).is_err());
    let raw = response(&req, |mut root| root.set_status(Status::Invalid));
    assert!(Response::decode_http(&req, &raw).is_err());
}
