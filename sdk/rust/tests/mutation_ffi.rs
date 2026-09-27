use morrow_plugin_sdk::mutation::{self, Action, Request};
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
struct Descriptor {
    abi_version: u32,
    struct_size: u32,
    kind: u32,
    call_id: u64,
    reference: Span,
    submission: Span,
    operation_id: Span,
    deadline_ms: u32,
    content_length: u64,
    content_sha256: Span,
    offset: u64,
    bytes: Span,
}
impl Default for Descriptor {
    fn default() -> Self {
        Self {
            abi_version: 1,
            struct_size: size_of::<Self>() as u32,
            kind: 2,
            call_id: 7,
            reference: Span::default(),
            submission: Span::default(),
            operation_id: Span::default(),
            deadline_ms: 1_000,
            content_length: 0,
            content_sha256: Span::default(),
            offset: 0,
            bytes: Span::default(),
        }
    }
}

#[repr(C)]
#[derive(Default)]
struct View {
    kind: u32,
    status: u32,
    phase: u32,
    effect: u32,
    call_id: u64,
    staged_bytes: u64,
    durable_content: u32,
    reference: Span,
    submission: Span,
    operation_id: Span,
    encoded_frame: Span,
}

unsafe extern "C" {
    fn mp_mutation_request_encode(
        raw: *const Descriptor,
        out: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> u32;
    fn mp_mutation_request_validate(bytes: *const u8, length: u32) -> u32;
    fn mp_mutation_schema_digest(out: *mut u8, capacity: u32) -> u32;
    fn mp_mutation_response_decode(
        bytes: *const u8,
        length: u32,
        request: *const u8,
        request_length: u32,
        out: *mut *mut c_void,
    ) -> u32;
    fn mp_mutation_response_get(raw: *const c_void, out: *mut View, size: u32) -> u32;
    fn mp_mutation_response_free(raw: *mut c_void);
}

fn owned_request(action: Action) -> Request {
    Request {
        call_id: 7,
        reference: [1; 32],
        submission: [2; 32],
        operation_id: "ffi-operation".into(),
        deadline_ms: 1_000,
        action,
    }
}

fn encode(raw: &Descriptor, output: &mut [u8], length: &mut u32) -> u32 {
    unsafe { mp_mutation_request_encode(raw, output.as_mut_ptr(), output.len() as u32, length) }
}

#[test]
fn descriptor_encodes_all_actions_and_preserves_buffer_on_failure() {
    let reference = [1; 32];
    let submission = [2; 32];
    let operation = b"ffi-operation";
    let hash = [3; 32];
    let chunk = [4, 5, 6];
    let mut raw = Descriptor {
        reference: span(&reference),
        submission: span(&submission),
        operation_id: span(operation),
        ..Default::default()
    };
    let cases = [
        (
            1,
            Action::Create {
                content_length: 3,
                content_sha256: hash,
            },
        ),
        (2, Action::Delete),
        (
            3,
            Action::Chunk {
                offset: 2,
                bytes: chunk.to_vec(),
            },
        ),
        (4, Action::Commit),
        (5, Action::Execute),
        (6, Action::Query),
        (7, Action::CancelPlan),
        (8, Action::Release),
    ];
    let mut output = vec![0xa5; mutation::MAX_FRAME_BYTES];
    let mut length = 99;
    for (kind, action) in cases {
        raw.kind = kind;
        raw.content_length = if kind == 1 { 3 } else { 0 };
        raw.content_sha256 = if kind == 1 {
            span(&hash)
        } else {
            Span::default()
        };
        raw.offset = if kind == 3 { 2 } else { 0 };
        raw.bytes = if kind == 3 {
            span(&chunk)
        } else {
            Span::default()
        };
        assert_eq!(encode(&raw, &mut output, &mut length), 0, "kind {kind}");
        assert_eq!(
            &output[..length as usize],
            owned_request(action).encode().unwrap(),
            "kind {kind}"
        );
        assert_eq!(
            unsafe { mp_mutation_request_validate(output.as_ptr(), length) },
            0
        );
        output.fill(0xa5);
    }
    raw.kind = 2;
    raw.content_length = 0;
    raw.content_sha256 = Span::default();
    raw.offset = 0;
    raw.bytes = Span::default();
    length = 99;
    assert_eq!(
        unsafe { mp_mutation_request_encode(&raw, output.as_mut_ptr(), 1, &mut length) },
        17
    );
    assert_eq!(length, 0);
    assert!(output.iter().all(|&b| b == 0xa5));
}

#[test]
fn descriptor_prefix_and_unused_fields_are_rejected_before_copy() {
    let reference = [1; 32];
    let submission = [2; 32];
    let mut raw = Descriptor {
        reference: span(&reference),
        submission: span(&submission),
        operation_id: span(b"ffi-operation"),
        ..Default::default()
    };
    let mut output = [0xa5; 512];
    let mut length = 9;
    raw.abi_version = 2;
    assert_eq!(encode(&raw, &mut output, &mut length), 18);
    assert_eq!(length, 0);
    raw.abi_version = 1;
    raw.struct_size = (size_of::<Descriptor>() - 1) as u32;
    assert_eq!(encode(&raw, &mut output, &mut length), 18);
    raw.struct_size = size_of::<Descriptor>() as u32;
    raw.kind = 9;
    assert_eq!(encode(&raw, &mut output, &mut length), 16);
    raw.kind = 2;
    raw.content_length = 1;
    assert_eq!(encode(&raw, &mut output, &mut length), 16);
    raw.content_length = 0;
    raw.content_sha256 = Span {
        data: reference.as_ptr(),
        length: 0,
    };
    assert_eq!(encode(&raw, &mut output, &mut length), 16);
    raw.content_sha256 = Span::default();
    raw.bytes = Span {
        data: reference.as_ptr(),
        length: 0,
    };
    assert_eq!(encode(&raw, &mut output, &mut length), 16);
    raw.bytes = Span::default();
    raw.offset = 1;
    assert_eq!(encode(&raw, &mut output, &mut length), 16);
    raw.offset = 0;
    raw.reference.length = 31;
    assert_eq!(encode(&raw, &mut output, &mut length), 16);
    raw.reference.length = 32;
    raw.submission = Span {
        data: std::ptr::null(),
        length: 32,
    };
    assert_eq!(encode(&raw, &mut output, &mut length), 16);
    assert_eq!(length, 0);
    assert!(output.iter().all(|&b| b == 0xa5));

    #[repr(C, align(8))]
    struct ShortPrefix {
        abi_version: u32,
        struct_size: u32,
    }
    let short = ShortPrefix {
        abi_version: 1,
        struct_size: 8,
    };
    length = 99;
    assert_eq!(
        unsafe {
            mp_mutation_request_encode(
                (&short as *const ShortPrefix).cast(),
                output.as_mut_ptr(),
                output.len() as u32,
                &mut length,
            )
        },
        18
    );
    assert_eq!(length, 0);
    assert!(output.iter().all(|&b| b == 0xa5));
}

#[test]
fn null_misaligned_and_oversized_inputs_fail_safely() {
    let mut output = [0xa5; 512];
    let mut length = 99;
    assert_eq!(
        unsafe {
            mp_mutation_request_encode(
                std::ptr::null(),
                output.as_mut_ptr(),
                output.len() as u32,
                &mut length,
            )
        },
        16
    );
    assert_eq!(length, 0);
    let mut storage = [0u8; size_of::<Descriptor>() + 16];
    let base = storage.as_mut_ptr() as usize;
    let offset = (1..=8)
        .find(|n| !(base + n).is_multiple_of(align_of::<Descriptor>()))
        .unwrap();
    length = 99;
    assert_eq!(
        unsafe {
            mp_mutation_request_encode(
                storage.as_ptr().add(offset).cast(),
                output.as_mut_ptr(),
                output.len() as u32,
                &mut length,
            )
        },
        16
    );
    assert_eq!(length, 0);
    assert!(output.iter().all(|&b| b == 0xa5));
    assert_eq!(
        unsafe {
            mp_mutation_request_validate(output.as_ptr(), (mutation::MAX_FRAME_BYTES + 1) as u32)
        },
        17
    );
    assert_eq!(
        unsafe { mp_mutation_request_validate(std::ptr::null(), 1) },
        16
    );
    assert_eq!(
        unsafe { mp_mutation_schema_digest(output.as_mut_ptr(), 31) },
        17
    );
    assert!(output.iter().all(|&b| b == 0xa5));
    assert_eq!(
        unsafe { mp_mutation_schema_digest(output.as_mut_ptr(), 32) },
        0
    );
    assert_eq!(&output[..32], &mutation::schema_digest());
}

#[test]
fn all_shared_vectors_match_c_validator() {
    for bytes in [
        include_bytes!("../../vectors/mutation-v1/create-empty.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/create-content.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/delete.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/chunk.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/commit.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/execute.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/query.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/cancel.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/release.bin").as_slice(),
    ] {
        assert_eq!(
            unsafe { mp_mutation_request_validate(bytes.as_ptr(), bytes.len() as u32) },
            0
        );
    }
    for bytes in [
        include_bytes!("../../vectors/mutation-v1/bad-zero-reference.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/bad-empty-hash.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/bad-chunk-overflow.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/bad-call.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/bad-deadline.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/bad-unicode-control.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/bad-trailing.bin").as_slice(),
    ] {
        assert_ne!(
            unsafe { mp_mutation_request_validate(bytes.as_ptr(), bytes.len() as u32) },
            0
        );
    }
}

#[test]
fn response_handle_owns_reply_and_correlates_to_original_request() {
    let original_request = include_bytes!("../../vectors/mutation-v1/execute.bin");
    for (name, fixture, status, phase, effect) in [
        (
            "created",
            include_bytes!("../../vectors/mutation-v1/created.bin").as_slice(),
            1,
            4,
            1,
        ),
        (
            "unknown",
            include_bytes!("../../vectors/mutation-v1/unknown.bin").as_slice(),
            10,
            3,
            0,
        ),
        (
            "denied",
            include_bytes!("../../vectors/mutation-v1/denied.bin").as_slice(),
            2,
            0,
            0,
        ),
    ] {
        let mut request = original_request.to_vec();
        let expected = Request::decode(&request).unwrap();
        let mut reply = fixture.to_vec();
        let saved = reply.clone();
        let mut handle: *mut c_void = std::ptr::null_mut();
        assert_eq!(
            unsafe {
                mp_mutation_response_decode(
                    reply.as_ptr(),
                    reply.len() as u32,
                    request.as_ptr(),
                    request.len() as u32,
                    &mut handle,
                )
            },
            0,
            "{name}"
        );
        assert!(!handle.is_null());
        reply.fill(0);
        request.fill(0);
        let mut view = View::default();
        assert_eq!(
            unsafe { mp_mutation_response_get(handle, &mut view, size_of::<View>() as u32) },
            0
        );
        assert_eq!(
            (view.kind, view.status, view.phase, view.effect),
            (5, status, phase, effect)
        );
        assert_eq!(view.call_id, expected.call_id);
        let encoded = unsafe {
            std::slice::from_raw_parts(view.encoded_frame.data, view.encoded_frame.length as usize)
        };
        assert_eq!(encoded, saved);
        assert_eq!(
            unsafe { std::slice::from_raw_parts(view.reference.data, 32) },
            expected.reference
        );
        assert_eq!(
            unsafe { std::slice::from_raw_parts(view.submission.data, 32) },
            expected.submission
        );
        assert_eq!(
            unsafe {
                std::slice::from_raw_parts(
                    view.operation_id.data,
                    view.operation_id.length as usize,
                )
            },
            expected.operation_id.as_bytes()
        );
        unsafe { mp_mutation_response_free(handle) };
    }
    for fixture in [
        include_bytes!("../../vectors/mutation-v1/bad-success-effect.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/bad-denied-effect.bin").as_slice(),
    ] {
        let mut handle = std::ptr::dangling_mut::<c_void>();
        assert_ne!(
            unsafe {
                mp_mutation_response_decode(
                    fixture.as_ptr(),
                    fixture.len() as u32,
                    original_request.as_ptr(),
                    original_request.len() as u32,
                    &mut handle,
                )
            },
            0
        );
        assert!(handle.is_null());
    }
    let foreign = Request {
        action: Action::Query,
        ..Request::decode(original_request).unwrap()
    }
    .encode()
    .unwrap();
    let created = include_bytes!("../../vectors/mutation-v1/created.bin");
    let mut handle = std::ptr::dangling_mut::<c_void>();
    assert_eq!(
        unsafe {
            mp_mutation_response_decode(
                created.as_ptr(),
                created.len() as u32,
                foreign.as_ptr(),
                foreign.len() as u32,
                &mut handle,
            )
        },
        19
    );
    assert!(handle.is_null());
    unsafe { mp_mutation_response_free(std::ptr::null_mut()) };
}

#[test]
fn truncated_frames_never_produce_a_response_handle() {
    let requests: &[&[u8]] = &[
        include_bytes!("../../vectors/mutation-v1/create-empty.bin"),
        include_bytes!("../../vectors/mutation-v1/create-content.bin"),
        include_bytes!("../../vectors/mutation-v1/delete.bin"),
        include_bytes!("../../vectors/mutation-v1/chunk.bin"),
        include_bytes!("../../vectors/mutation-v1/commit.bin"),
        include_bytes!("../../vectors/mutation-v1/execute.bin"),
        include_bytes!("../../vectors/mutation-v1/query.bin"),
        include_bytes!("../../vectors/mutation-v1/cancel.bin"),
        include_bytes!("../../vectors/mutation-v1/release.bin"),
    ];
    for frame in requests {
        for end in 0..frame.len() {
            assert_ne!(
                unsafe { mp_mutation_request_validate(frame.as_ptr(), end as u32) },
                0,
                "accepted truncated request at {end}/{}",
                frame.len()
            );
        }
    }
    let request = include_bytes!("../../vectors/mutation-v1/execute.bin");
    for reply in [
        include_bytes!("../../vectors/mutation-v1/created.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/unknown.bin").as_slice(),
        include_bytes!("../../vectors/mutation-v1/denied.bin").as_slice(),
    ] {
        // The original request is untrusted too. Independently truncate each
        // side of the correlation check; failure must clear a stale output slot.
        for (frame, other, truncate_reply) in [
            (reply, request.as_slice(), true),
            (request.as_slice(), reply, false),
        ] {
            for end in 0..frame.len() {
                let (response, response_len, original, original_len) = if truncate_reply {
                    (frame, end, other, other.len())
                } else {
                    (other, other.len(), frame, end)
                };
                let mut handle = std::ptr::dangling_mut::<c_void>();
                let result = unsafe {
                    mp_mutation_response_decode(
                        response.as_ptr(),
                        response_len as u32,
                        original.as_ptr(),
                        original_len as u32,
                        &mut handle,
                    )
                };
                assert_ne!(
                    result,
                    0,
                    "accepted truncated frame at {end}/{}",
                    frame.len()
                );
                assert!(handle.is_null());
            }
        }
    }
}

#[test]
fn maximum_chunk_and_short_output_slots_preserve_canaries() {
    let reference = [1; 32];
    let submission = [2; 32];
    let chunk = vec![0x5a; mutation::MAX_CHUNK_BYTES];
    let raw = Descriptor {
        kind: 3,
        reference: span(&reference),
        submission: span(&submission),
        operation_id: span(b"ffi-operation"),
        offset: mutation::MAX_CONTENT_BYTES - chunk.len() as u64,
        bytes: span(&chunk),
        ..Default::default()
    };
    let expected = owned_request(Action::Chunk {
        offset: raw.offset,
        bytes: chunk.clone(),
    })
    .encode()
    .unwrap();
    let mut output = vec![0xa5; expected.len() + 32];
    for capacity in [0, 1, expected.len() - 1, expected.len()] {
        output.fill(0xa5);
        let mut length = u32::MAX;
        let result = unsafe {
            mp_mutation_request_encode(
                &raw,
                output.as_mut_ptr().add(16),
                capacity as u32,
                &mut length,
            )
        };
        if capacity < expected.len() {
            assert_eq!(result, 17);
            assert_eq!(length, 0);
            assert!(output.iter().all(|&byte| byte == 0xa5));
        } else {
            assert_eq!(result, 0);
            assert_eq!(length as usize, expected.len());
            assert_eq!(&output[16..16 + expected.len()], expected);
            assert!(output[..16].iter().all(|&byte| byte == 0xa5));
            assert!(
                output[16 + expected.len()..]
                    .iter()
                    .all(|&byte| byte == 0xa5)
            );
        }
    }
    // A too-small view must not even be cleared, since the caller has not
    // provided storage for the entire structure. Use real aligned storage.
    let mut storage = vec![0xa5a5_a5a5_a5a5_a5a5u64; size_of::<View>().div_ceil(8) + 2];
    for size in 0..size_of::<View>() {
        assert_eq!(
            unsafe {
                mp_mutation_response_get(std::ptr::null(), storage.as_mut_ptr().cast(), size as u32)
            },
            17
        );
        assert!(storage.iter().all(|&word| word == 0xa5a5_a5a5_a5a5_a5a5));
    }
}
