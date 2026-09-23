use capnp::{message::Builder, serialize};
use morrow_core::{
    Error,
    io::Header,
    plugin_package::{
        Package,
        io::{self, IoCapability},
    },
    service::{self, Invocation, Reply, Request, Response},
    service_capnp as wire,
};
use prost::Message;
fn invocation() -> Invocation {
    Invocation {
        service: "service-one".into(),
        handler: "api.invoke".into(),
        principal: "principal-one".into(),
        method: "POST".into(),
        target: "/api?q=one%20two".into(),
        headers: vec![Header {
            name: "X-Label".into(),
            value: vec![0xe9],
        }],
        body: vec![0, 255, 1],
    }
}
fn reply() -> Reply {
    Reply {
        status: 422,
        headers: vec![Header {
            name: "x-label".into(),
            value: vec![0xe9],
        }],
        body: vec![255, 0],
    }
}
fn raw_request(edit: impl FnOnce(&mut wire::request::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::request::Builder>();
    root.set_version(service::VERSION);
    root.set_schema_sha256(&service::schema_digest());
    root.set_call_id(7);
    root.set_service("service-one");
    root.set_handler("api.invoke");
    root.set_principal("principal-one");
    root.set_method("POST");
    root.set_target("/api");
    edit(&mut root);
    serialize::write_message_to_words(&message)
}
fn raw_response(request: &Request, edit: impl FnOnce(&mut wire::response::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::response::Builder>();
    root.set_version(service::VERSION);
    root.set_schema_sha256(&service::schema_digest());
    root.set_call_id(request.call_id());
    root.set_request_sha256(&request.digest());
    root.set_status(200);
    edit(&mut root);
    serialize::write_message_to_words(&message)
}
#[test]
fn original_frames_and_binary_values_roundtrip_from_unaligned_slices() {
    let value = invocation();
    let request = Request::encode(7, &value).unwrap();
    let mut unaligned = vec![1];
    unaligned.extend_from_slice(request.bytes());
    let parsed = Request::decode(&unaligned[1..]).unwrap();
    assert!(parsed.invocation() == &value);
    assert_eq!(parsed.call_id(), 7);
    assert_eq!(parsed.bytes(), request.bytes());
    assert_eq!(parsed.digest(), request.digest());
    let expected = reply();
    let encoded = Response::encode(&request, &expected).unwrap();
    assert!(Response::decode(&parsed, &encoded).unwrap() == expected);
}
#[test]
fn response_binds_exact_original_request_and_call_id() {
    let original = Request::encode(7, &invocation()).unwrap();
    let response = Response::encode(&original, &reply()).unwrap();
    let other_id = Request::encode(8, &invocation()).unwrap();
    assert!(matches!(
        Response::decode(&other_id, &response),
        Err(Error::Integrity)
    ));
    let mut other = invocation();
    other.principal = "another-principal".into();
    assert!(matches!(
        Response::decode(&Request::encode(7, &other).unwrap(), &response),
        Err(Error::Integrity)
    ));
    assert!(matches!(
        Response::decode(&original, &raw_response(&original, |r| r.set_call_id(8))),
        Err(Error::Integrity)
    ));
    assert!(matches!(
        Response::decode(
            &original,
            &raw_response(&original, |r| r.set_request_sha256(&[0; 32]))
        ),
        Err(Error::Integrity)
    ));
}
#[test]
fn versions_schema_utf8_truncation_and_trailing_frames_fail_closed() {
    for frame in [
        raw_request(|r| r.set_version(2)),
        raw_request(|r| r.set_schema_sha256(&[0; 32])),
        raw_request(|r| r.set_schema_sha256(&[0; 31])),
    ] {
        assert!(matches!(
            Request::decode(&frame),
            Err(Error::UnsupportedVersion)
        ));
    }
    let request = Request::encode(7, &invocation()).unwrap();
    for frame in [
        raw_response(&request, |r| r.set_version(2)),
        raw_response(&request, |r| r.set_schema_sha256(&[0; 32])),
    ] {
        assert!(matches!(
            Response::decode(&request, &frame),
            Err(Error::UnsupportedVersion)
        ));
    }
    let mut invalid = raw_request(|_| {});
    let start = invalid
        .windows(11)
        .position(|w| w == b"service-one")
        .unwrap();
    invalid[start] = 255;
    assert!(Request::decode(&invalid).is_err());
    for n in 0..16 {
        assert!(Request::decode(&request.bytes()[..n]).is_err());
    }
    let mut tail = request.bytes().to_vec();
    tail.extend_from_slice(&[0; 8]);
    assert!(Request::decode(&tail).is_err());
    let mut tail = Response::encode(&request, &reply()).unwrap();
    tail.extend_from_slice(&[0; 8]);
    assert!(Response::decode(&request, &tail).is_err());
    assert!(matches!(
        Request::decode(&vec![0; service::MAX_FRAME_BYTES + 1]),
        Err(Error::Limit)
    ));
}
#[test]
fn identity_method_and_origin_form_validation_is_not_display_text() {
    for id in ["", "has/slash", "has:colon", "line\nfeed"] {
        for field in 0..3 {
            let mut value = invocation();
            match field {
                0 => value.service = id.into(),
                1 => value.handler = id.into(),
                _ => value.principal = id.into(),
            };
            assert!(Request::encode(1, &value).is_err());
        }
    }
    for method in ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"] {
        let mut value = invocation();
        value.method = method.into();
        assert!(Request::encode(1, &value).is_ok());
    }
    for method in ["get", "CONNECT", "TRACE", "POST\r\n"] {
        let mut value = invocation();
        value.method = method.into();
        assert!(Request::encode(1, &value).is_err());
    }
    for target in [
        "",
        "https://example.test/",
        "//evil",
        "/\\evil",
        "/path#fragment",
        "/a b",
        "/a\u{a0}b",
        "/%",
        "/%GG",
    ] {
        let mut value = invocation();
        value.target = target.into();
        assert!(Request::encode(1, &value).is_err(), "{target}");
    }
}
#[test]
fn sensitive_and_transport_headers_are_rejected_on_encode_and_decode() {
    for name in [
        "Authorization",
        "COOKIE",
        "Host",
        "Content-Length",
        "Transfer-Encoding",
        "Connection",
        "Keep-Alive",
        "Upgrade",
        "TE",
        "Trailer",
        "Expect",
        "Proxy-Anything",
    ] {
        let mut value = invocation();
        value.headers = vec![Header {
            name: name.into(),
            value: b"x".to_vec(),
        }];
        assert!(Request::encode(1, &value).is_err(), "{name}");
        let encoded = raw_request(|r| {
            let mut h = r.reborrow().init_headers(1).get(0);
            h.set_name(name);
            h.set_value(b"x");
        });
        assert!(Request::decode(&encoded).is_err(), "{name}");
    }
    let request = Request::encode(1, &invocation()).unwrap();
    for name in [
        "Content-Length",
        "Transfer-Encoding",
        "Connection",
        "Keep-Alive",
        "Upgrade",
        "TE",
        "Trailer",
        "Proxy-Authenticate",
        "Proxy-Custom",
    ] {
        let mut value = reply();
        value.headers = vec![Header {
            name: name.into(),
            value: b"x".to_vec(),
        }];
        assert!(Response::encode(&request, &value).is_err());
        let encoded = raw_response(&request, |r| {
            let mut h = r.reborrow().init_headers(1).get(0);
            h.set_name(name);
            h.set_value(b"x");
        });
        assert!(Response::decode(&request, &encoded).is_err());
    }
}
#[test]
fn body_header_count_and_header_byte_budgets_apply_in_both_directions() {
    let mut value = invocation();
    value.body = vec![0; service::MAX_BODY_BYTES];
    assert!(Request::encode(1, &value).is_ok());
    value.body.push(0);
    assert!(matches!(Request::encode(1, &value), Err(Error::Limit)));
    assert!(
        Request::decode(&raw_request(|r| r.set_body(&vec![
            0;
            service::MAX_BODY_BYTES
                + 1
        ])))
        .is_err()
    );
    for headers in [
        vec![
            Header {
                name: "x".into(),
                value: vec![]
            };
            65
        ],
        vec![
            Header {
                name: "x".into(),
                value: vec![b'a'; 8192]
            };
            2
        ],
    ] {
        let mut value = invocation();
        value.headers = headers.clone();
        assert!(Request::encode(1, &value).is_err());
        let request = Request::encode(1, &invocation()).unwrap();
        let mut value = reply();
        value.headers = headers;
        assert!(Response::encode(&request, &value).is_err());
    }
    let request = Request::encode(1, &invocation()).unwrap();
    assert!(
        Response::decode(
            &request,
            &raw_response(&request, |r| r
                .set_body(&vec![0; service::MAX_BODY_BYTES + 1]))
        )
        .is_err()
    );
}
#[test]
fn response_status_and_bodyless_statuses_are_strict() {
    let request = Request::encode(1, &invocation()).unwrap();
    for status in [0, 101, 199, 600, 65535] {
        let mut value = reply();
        value.status = status;
        assert!(Response::encode(&request, &value).is_err());
        assert!(
            Response::decode(&request, &raw_response(&request, |r| r.set_status(status))).is_err()
        );
    }
    for status in [204, 205, 304] {
        let mut value = reply();
        value.status = status;
        assert!(Response::encode(&request, &value).is_err());
        value.body.clear();
        assert!(Response::encode(&request, &value).is_ok());
        assert!(
            Response::decode(
                &request,
                &raw_response(&request, |r| {
                    r.set_status(status);
                    r.set_body(b"x");
                })
            )
            .is_err()
        );
    }
}
#[test]
fn service_manifest_pin_requires_publish_and_preserves_legacy_empty_encoding() {
    let module = b"\0asm\x01\0\0\0";
    let mut manifest = Package::manifest_for_task("service.example", "1.0.0", module, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    manifest.io_declaration = Some(io::declaration(
        vec![IoCapability::HttpPublish],
        vec!["api.invoke".into()],
    ));
    assert!(
        manifest
            .io_declaration
            .as_ref()
            .unwrap()
            .service_schema_sha256
            .is_empty()
    );
    let legacy = manifest.encode_to_vec();
    let package = Package::build(manifest.clone(), module).unwrap();
    assert_eq!(
        Package::decode(package.archive()).unwrap().manifest_bytes(),
        legacy
    );
    manifest
        .io_declaration
        .as_mut()
        .unwrap()
        .service_schema_sha256 = service::schema_digest().to_vec();
    assert!(Package::build(manifest.clone(), module).is_ok());
    for digest in [vec![0; 31], vec![0; 32], vec![0; 33]] {
        let mut bad = manifest.clone();
        bad.io_declaration.as_mut().unwrap().service_schema_sha256 = digest;
        assert!(Package::build(bad, module).is_err());
    }
    manifest
        .io_declaration
        .as_mut()
        .unwrap()
        .requested_capabilities = vec![IoCapability::HttpRequest.number()];
    assert!(Package::build(manifest, module).is_err());
}
