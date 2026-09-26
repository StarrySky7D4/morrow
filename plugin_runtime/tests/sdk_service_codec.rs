//! Independent host codec interoperability, including alternate Cap'n Proto layout.
use morrow_core::{io::Header, service as host};
use morrow_plugin_sdk::service as guest;

fn invocation() -> host::Invocation {
    host::Invocation {
        service: "service.echo".into(),
        handler: "service.echo".into(),
        principal: "alice".into(),
        method: "POST".into(),
        target: "/echo?q=one&q=two".into(),
        headers: vec![
            Header {
                name: "x-repeat".into(),
                value: b"one".to_vec(),
            },
            Header {
                name: "x-repeat".into(),
                value: vec![0xe9],
            },
        ],
        body: vec![0, 255, 1],
    }
}

#[test]
fn core_request_and_guest_reply_preserve_u64_binary_and_duplicate_headers() {
    assert_eq!(host::schema_digest(), guest::schema_digest());
    for id in [0, (1_u64 << 53) + 1, (1_u64 << 63) + 1, u64::MAX] {
        let request = host::Request::encode(id, &invocation()).unwrap();
        let parsed = guest::Request::decode(request.bytes()).unwrap();
        assert_eq!(parsed.call_id(), id);
        assert_eq!(parsed.invocation().principal, "alice");
        let reply = guest::Reply {
            status: 200,
            headers: parsed.invocation().headers.clone(),
            body: parsed.invocation().body.clone(),
        };
        let bytes = guest::Response::encode(&parsed, &reply).unwrap();
        let result = host::Response::decode(&request, &bytes).unwrap();
        assert_eq!(result.body, invocation().body);
        assert_eq!(result.headers, invocation().headers);
    }
}

#[test]
fn equivalent_layout_is_not_allowed_to_replace_original_request_digest() {
    use capnp::{
        message::{Builder, HeapAllocator},
        serialize,
    };
    let original = host::Request::encode(u64::MAX, &invocation()).unwrap();
    let reader = serialize::read_message(&mut original.bytes(), Default::default()).unwrap();
    let mut alternate = Builder::new(HeapAllocator::new().first_segment_words(1));
    alternate
        .set_root(
            reader
                .get_root::<morrow_core::service_capnp::request::Reader>()
                .unwrap(),
        )
        .unwrap();
    let bytes = serialize::write_message_to_words(&alternate);
    assert_ne!(original.bytes(), bytes);
    let host_request = host::Request::decode(&bytes).unwrap();
    let guest_request = guest::Request::decode(&bytes).unwrap();
    let response = guest::Response::encode(
        &guest_request,
        &guest::Reply {
            status: 200,
            headers: vec![],
            body: vec![],
        },
    )
    .unwrap();
    assert!(host::Response::decode(&host_request, &response).is_ok());
    assert!(host::Response::decode(&original, &response).is_err());
}

#[test]
#[ignore = "requires compiled native C and C++ service codec executables"]
fn native_c_and_cpp_handles_return_core_verified_frames() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    for count in 0..=2 {
        let mut value = invocation();
        for _ in 0..count {
            value.headers.push(resources().to_header().unwrap());
        }
        let request = host::Request::encode(u64::MAX, &value).unwrap();
        for key in [
            "MORROW_SDK_SERVICE_NATIVE_C",
            "MORROW_SDK_SERVICE_NATIVE_CPP",
        ] {
            let executable =
                std::env::var_os(key).expect("native service test executable required");
            let mut child = Command::new(executable)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(request.bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{key}");
            let reply = host::Response::decode(&request, &output.stdout).unwrap();
            assert_eq!(reply.status, 200);
            assert_eq!(reply.body, invocation().body);
            assert_eq!(reply.headers, value.headers);
        }
    }
}

fn resources() -> morrow_core::service_resources::Directory {
    use morrow_core::service_resources::{Directory, Endpoint};
    Directory {
        scope_sha256: [7; 32],
        endpoints: vec![Endpoint {
            reference: "a".repeat(64),
            credential: b"opaque-reference".to_vec(),
            methods: vec!["GET".into(), "POST".into()],
            max_request_bytes: 1024,
            max_response_bytes: 2048,
            timeout_ms: 3000,
            response_frame_limit: 4096,
        }],
    }
}
#[test]
fn resource_directory_is_identical_across_independent_host_and_sdk_codecs() {
    use morrow_plugin_sdk::service_resources as sdk;
    assert_eq!(
        sdk::schema_digest(),
        morrow_core::service_resources::schema_digest()
    );
    let host = resources();
    let bytes = host.encode().unwrap();
    let guest = sdk::Directory::decode(&bytes).unwrap();
    assert_eq!(guest.encode().unwrap(), bytes);
    let header = host.to_header().unwrap();
    let parsed = sdk::Directory::from_headers(&[morrow_plugin_sdk::io::Header {
        name: header.name,
        value: header.value,
    }])
    .unwrap()
    .unwrap();
    assert_eq!(parsed, guest);
    assert_eq!(parsed.scope_sha256, [7; 32]);
    assert_eq!(parsed.endpoints[0].credential, b"opaque-reference");
}
