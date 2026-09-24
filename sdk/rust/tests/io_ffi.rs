use morrow_plugin_sdk::{
    io::{self, Action, Request, Status, Submission, SubmissionKind},
    io_capnp,
};
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Span {
    data: *const u8,
    length: u32,
}
fn span(bytes: &[u8]) -> Span {
    Span {
        data: bytes.as_ptr(),
        length: bytes.len() as u32,
    }
}
#[repr(C)]
#[derive(Default)]
struct Header {
    name: Span,
    value: Span,
}
#[repr(C)]
struct Descriptor {
    abi_version: u32,
    struct_size: u32,
    kind: u32,
    call_id: u64,
    reference: Span,
    operation_id: Span,
    endpoint: Span,
    method: Span,
    relative_target: Span,
    credential: Span,
    body: Span,
    headers: *const Header,
    header_count: u32,
    deadline_ms: u64,
    offset: u64,
    limit: u32,
}
impl Default for Descriptor {
    fn default() -> Self {
        Self {
            abi_version: 1,
            struct_size: size_of::<Self>() as u32,
            kind: 0,
            call_id: 77,
            reference: Span::default(),
            operation_id: Span::default(),
            endpoint: Span::default(),
            method: Span::default(),
            relative_target: Span::default(),
            credential: Span::default(),
            body: Span::default(),
            headers: std::ptr::null(),
            header_count: 0,
            deadline_ms: 0,
            offset: 0,
            limit: 0,
        }
    }
}
#[repr(C)]
#[derive(Default)]
struct View {
    status: u32,
    reference: Span,
    bytes: Span,
    encoded_frame: Span,
    offset: u64,
    eof: u32,
    http_status: u32,
    headers: *const Header,
    header_count: u32,
}
unsafe extern "C" {
    fn mp_io_request_encode(
        raw: *const Descriptor,
        out: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> u32;
    fn mp_io_request_validate(bytes: *const u8, length: u32) -> u32;
    fn mp_io_schema_digest(out: *mut u8, capacity: u32) -> u32;
    fn mp_io_response_decode(
        response: *const u8,
        response_length: u32,
        request: *const u8,
        request_length: u32,
        out: *mut *mut c_void,
    ) -> u32;
    fn mp_io_response_get(raw: *const c_void, out: *mut View, size: u32) -> u32;
    fn mp_io_response_free(raw: *mut c_void);
}
fn file_request() -> Request {
    Request::new(
        77,
        Action::Submit(Submission {
            operation_id: b"operation-1".to_vec(),
            deadline_ms: 1000,
            kind: SubmissionKind::FileRead { reference: [7; 32] },
        }),
    )
    .unwrap()
}
fn response(request: &Request, status: Status) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<io_capnp::response::Builder>();
    root.set_version(io::VERSION);
    root.set_schema_sha256(&io::schema_digest());
    root.set_call_id(request.call_id());
    root.set_request_sha256(&request.digest());
    root.set_status(status);
    root.set_eof(true);
    capnp::serialize::write_message_to_words(&message)
}
#[test]
fn c_file_read_matches_rust_and_failure_is_bounded() {
    let reference = [7u8; 32];
    let mut raw = Descriptor {
        kind: 1,
        reference: span(&reference),
        operation_id: span(b"operation-1"),
        deadline_ms: 1000,
        ..Default::default()
    };
    let mut output = vec![0xa5; io::MAX_FRAME_BYTES];
    let mut length = 99;
    unsafe {
        assert_eq!(
            mp_io_request_encode(&raw, output.as_mut_ptr(), output.len() as u32, &mut length),
            0
        );
        assert_eq!(&output[..length as usize], file_request().bytes());
        output.fill(0xa5);
        assert_eq!(
            mp_io_request_encode(&raw, output.as_mut_ptr(), 1, &mut length),
            17
        );
        assert_eq!(length, 0);
        assert!(output.iter().all(|b| *b == 0xa5));
        raw.reference.length = 31;
        assert_eq!(
            mp_io_request_encode(&raw, output.as_mut_ptr(), output.len() as u32, &mut length),
            16
        );
        assert_eq!(length, 0);
        assert!(output.iter().all(|b| *b == 0xa5));
        raw.reference.length = 32;
        raw.abi_version = 2;
        assert_eq!(
            mp_io_request_encode(&raw, output.as_mut_ptr(), output.len() as u32, &mut length),
            18
        );
        assert_eq!(length, 0);
    }
}
#[test]
fn c_response_binds_exact_frame_and_preserves_unknown() {
    let request = file_request();
    let encoded = response(&request, Status::OutcomeUnknown);
    let mut handle = std::ptr::null_mut();
    let mut view = View::default();
    unsafe {
        assert_eq!(
            mp_io_response_decode(
                encoded.as_ptr(),
                encoded.len() as u32,
                request.bytes().as_ptr(),
                request.bytes().len() as u32,
                &mut handle
            ),
            0
        );
        assert!(!handle.is_null());
        assert_eq!(
            mp_io_response_get(handle, &mut view, size_of::<View>() as u32),
            0
        );
        assert_eq!(view.status, 12);
        assert_eq!(view.bytes.length, 0);
        assert_eq!(
            std::slice::from_raw_parts(view.encoded_frame.data, view.encoded_frame.length as usize),
            encoded
        );
        mp_io_response_free(handle);
        handle = std::ptr::dangling_mut::<c_void>();
        let wrong = Request::new(78, request.action().clone()).unwrap();
        assert_eq!(
            mp_io_response_decode(
                encoded.as_ptr(),
                encoded.len() as u32,
                wrong.bytes().as_ptr(),
                wrong.bytes().len() as u32,
                &mut handle
            ),
            19
        );
        assert!(handle.is_null());
        assert_eq!(
            mp_io_response_get(handle, &mut view, size_of::<View>() as u32),
            16
        );
        assert!(view.encoded_frame.data.is_null());
        assert_eq!(view.encoded_frame.length, 0);
        assert_eq!(
            mp_io_response_decode(
                encoded.as_ptr(),
                encoded.len() as u32,
                request.bytes().as_ptr(),
                131073,
                &mut handle
            ),
            17
        );
        assert!(handle.is_null());
    }
}
#[test]
fn c_digest_and_optional_native_fixture() {
    let mut digest = [0xa5; 32];
    unsafe {
        assert_eq!(mp_io_schema_digest(digest.as_mut_ptr(), 31), 17);
    }
    assert_eq!(digest, [0xa5; 32]);
    unsafe {
        assert_eq!(mp_io_schema_digest(digest.as_mut_ptr(), 32), 0);
    }
    assert_eq!(digest, io::schema_digest());
    if let Ok(dir) = std::env::var("MORROW_IO_SMOKE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let request = file_request();
        std::fs::write(dir.join("request.capnp"), request.bytes()).unwrap();
        std::fs::write(
            dir.join("response.capnp"),
            response(&request, Status::OutcomeUnknown),
        )
        .unwrap();
    }
}

#[test]
fn c_validator_rejects_bad_frame_and_exact_layout_is_bound() {
    let canonical = file_request();
    let mut original = canonical.bytes().to_vec();
    let words = u32::from_le_bytes(original[4..8].try_into().unwrap());
    original[4..8].copy_from_slice(&(words + 1).to_le_bytes());
    original.extend([0; 8]);
    let alternate = Request::decode(&original).unwrap();
    let reply = response(&alternate, Status::OutcomeUnknown);
    let mut handle = std::ptr::null_mut();
    unsafe {
        assert_eq!(
            mp_io_request_validate(original.as_ptr(), original.len() as u32),
            0
        );
        assert_eq!(
            mp_io_response_decode(
                reply.as_ptr(),
                reply.len() as u32,
                original.as_ptr(),
                original.len() as u32,
                &mut handle
            ),
            0
        );
        mp_io_response_free(handle);
        assert_eq!(
            mp_io_response_decode(
                reply.as_ptr(),
                reply.len() as u32,
                canonical.bytes().as_ptr(),
                canonical.bytes().len() as u32,
                &mut handle
            ),
            19
        );
        assert!(handle.is_null());
        assert_ne!(mp_io_request_validate(original.as_ptr(), 1), 0);
        assert_eq!(mp_io_request_validate(original.as_ptr(), 131073), 17);
    }
}

#[test]
fn c_http_and_operation_descriptors_match_rust_codec() {
    let header = Header {
        name: span(b"accept"),
        value: span(b"text/plain"),
    };
    let mut raw = Descriptor {
        kind: 2,
        operation_id: span(b"http-op"),
        endpoint: span(b"endpoint-ref"),
        method: span(b"GET"),
        relative_target: span(b"/resource"),
        headers: &header,
        header_count: 1,
        deadline_ms: 500,
        ..Default::default()
    };
    let expected = Request::new(
        77,
        Action::Submit(Submission {
            operation_id: b"http-op".to_vec(),
            deadline_ms: 500,
            kind: SubmissionKind::HttpRequest(io::HttpRequest {
                endpoint: b"endpoint-ref".to_vec(),
                method: "GET".into(),
                relative_target: "/resource".into(),
                headers: vec![io::Header {
                    name: "accept".into(),
                    value: b"text/plain".to_vec(),
                }],
                body: Vec::new(),
                credential: Vec::new(),
            }),
        }),
    )
    .unwrap();
    let mut output = vec![0u8; io::MAX_FRAME_BYTES];
    let mut length = 0;
    unsafe {
        assert_eq!(
            mp_io_request_encode(&raw, output.as_mut_ptr(), output.len() as u32, &mut length),
            0
        );
        assert_eq!(&output[..length as usize], expected.bytes());
        raw.headers = std::ptr::null();
        assert_eq!(
            mp_io_request_encode(&raw, output.as_mut_ptr(), output.len() as u32, &mut length),
            16
        );
        assert_eq!(length, 0);
        raw.header_count = 65;
        assert_eq!(
            mp_io_request_encode(&raw, output.as_mut_ptr(), output.len() as u32, &mut length),
            17
        );
        assert_eq!(length, 0);
        raw.kind = 7;
        raw.operation_id = span(b"http-op");
        assert_eq!(
            mp_io_request_encode(&raw, output.as_mut_ptr(), output.len() as u32, &mut length),
            0
        );
        let query = Request::new(
            77,
            Action::QueryOperation {
                operation_id: b"http-op".to_vec(),
            },
        )
        .unwrap();
        assert_eq!(&output[..length as usize], query.bytes());
    }
}

#[test]
fn c_http_response_owns_payload_headers_and_original_frame() {
    let request = Request::new(
        91,
        Action::Submit(Submission {
            operation_id: b"http-op".to_vec(),
            deadline_ms: 500,
            kind: SubmissionKind::HttpRequest(io::HttpRequest {
                endpoint: b"endpoint-ref".to_vec(),
                method: "GET".into(),
                relative_target: "/resource".into(),
                headers: Vec::new(),
                body: Vec::new(),
                credential: Vec::new(),
            }),
        }),
    )
    .unwrap();
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<io_capnp::response::Builder>();
    root.set_version(io::VERSION);
    root.set_schema_sha256(&io::schema_digest());
    root.set_call_id(request.call_id());
    root.set_request_sha256(&request.digest());
    root.set_status(Status::Completed);
    root.set_eof(true);
    root.set_http_status(200);
    root.set_bytes(b"ok");
    let mut header = root.init_headers(1).get(0);
    header.set_name("content-type");
    header.set_value(b"text/plain");
    let mut encoded = capnp::serialize::write_message_to_words(&message);
    let length = encoded.len();
    let mut handle = std::ptr::null_mut();
    let mut view = View::default();
    unsafe {
        assert_eq!(
            mp_io_response_decode(
                encoded.as_ptr(),
                encoded.len() as u32,
                request.bytes().as_ptr(),
                request.bytes().len() as u32,
                &mut handle
            ),
            0
        );
        encoded.fill(0);
        assert_eq!(
            mp_io_response_get(handle, &mut view, size_of::<View>() as u32),
            0
        );
        assert_eq!(view.status, 3);
        assert_eq!(view.http_status, 200);
        assert_eq!(
            std::slice::from_raw_parts(view.bytes.data, view.bytes.length as usize),
            b"ok"
        );
        assert_eq!(view.encoded_frame.length as usize, length);
        assert_eq!(view.header_count, 1);
        let header = &*view.headers;
        assert_eq!(
            std::slice::from_raw_parts(header.name.data, header.name.length as usize),
            b"content-type"
        );
        assert_eq!(
            std::slice::from_raw_parts(header.value.data, header.value.length as usize),
            b"text/plain"
        );
        mp_io_response_free(handle);
    }
}
