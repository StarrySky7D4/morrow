use morrow_plugin_sdk::{
    Error,
    channel::{
        Action, Frame, MAX_WIRE_BYTES, Request, Response, Status,
        transport::{Client, HostV1},
    },
};
use sha2::{Digest, Sha256};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    ffi::c_void,
};

struct CountingAllocator;
thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static WIRE_ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == MAX_WIRE_BYTES {
            let _ = COUNTING.try_with(|enabled| {
                if enabled.get() {
                    let _ = WIRE_ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
                }
            });
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn request() -> Request {
    Request {
        call_id: [1; 32],
        reference: [2; 32],
        source_epoch: [3; 32],
        action: Action::Receive {
            last_acked: 0,
            credit_bytes: 65536,
        },
    }
}
fn response(request: &Request) -> Response {
    Response {
        call_id: request.call_id,
        reference: request.reference,
        source_epoch: request.source_epoch,
        request_sha256: request.digest().unwrap(),
        status: Status::Frame,
        frame: Some(Frame {
            sequence: 1,
            source_epoch: request.source_epoch,
            bytes: vec![0xff; 65536],
            cursor: vec![0, 255],
        }),
        last_acked: 0,
        accepted_sequence: 0,
        resource_reclaimed: false,
    }
}

struct State {
    calls: usize,
    reply: Vec<u8>,
    request: Vec<u8>,
    status: u32,
    written: Option<u32>,
    output_address: usize,
    changed_address: bool,
    clean: bool,
    disjoint: bool,
}
impl State {
    fn new(request: &Request) -> Self {
        Self {
            calls: 0,
            reply: response(request).encode().unwrap(),
            request: request.encode().unwrap(),
            status: 0,
            written: None,
            output_address: 0,
            changed_address: false,
            clean: true,
            disjoint: true,
        }
    }
    fn host(&mut self) -> HostV1 {
        HostV1 {
            abi_version: 1,
            struct_size: size_of::<HostV1>() as u32,
            context: (self as *mut Self).cast(),
            call: Some(callback),
        }
    }
}
unsafe extern "C" fn callback(
    context: *mut c_void,
    input: *const u8,
    length: u32,
    output: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> u32 {
    let state = unsafe { &mut *context.cast::<State>() };
    state.calls += 1;
    let input = unsafe { std::slice::from_raw_parts(input, length as usize) };
    let output = unsafe { std::slice::from_raw_parts_mut(output, capacity as usize) };
    state.clean &= output.iter().all(|byte| *byte == 0);
    let in_start = input.as_ptr() as usize;
    let out_start = output.as_ptr() as usize;
    state.disjoint &= in_start + input.len() <= out_start || out_start + output.len() <= in_start;
    state.changed_address |= state.output_address != 0 && state.output_address != out_start;
    state.output_address = out_start;
    // A native callback may use the entire capacity as scratch, beyond its reply.
    output.fill(0xa5);
    assert_eq!(capacity as usize, MAX_WIRE_BYTES);
    assert_eq!(input, state.request);
    assert_eq!(
        Response::decode(&state.reply)
            .ok()
            .map(|r| r.request_sha256),
        Some(Sha256::digest(input).into())
    );
    output[..state.reply.len()].copy_from_slice(&state.reply);
    unsafe { *written = state.written.unwrap_or(state.reply.len() as u32) };
    state.status
}

#[test]
fn repeated_native_calls_reuse_bounded_storage_and_own_replies() {
    let request = request();
    let mut state = State::new(&request);
    let host = state.host();
    let mut client = unsafe { Client::from_host(&host) }.unwrap();
    let mut first = None;
    for call in 0..16 {
        WIRE_ALLOCATIONS.with(|count| count.set(0));
        COUNTING.with(|enabled| enabled.set(true));
        let result = client.call(&request);
        COUNTING.with(|enabled| enabled.set(false));
        let allocations = WIRE_ALLOCATIONS.with(Cell::get);
        // One request encode and one canonical response re-encode. Only the first
        // call allocates output; correlation must not encode the request again.
        // The callback's own canonical decode contributes one additional encode.
        assert_eq!(allocations, if call == 0 { 4 } else { 3 }, "call {call}");
        let reply = result.unwrap();
        if call == 0 {
            first = Some(reply);
        }
    }
    assert_eq!(state.calls, 16);
    assert!(state.clean && state.disjoint && !state.changed_address);
    assert_eq!(first.unwrap(), response(&request));
}

#[test]
fn failure_and_bad_lengths_are_one_submission_and_recover_cleanly() {
    for (status, written, expected) in [
        (9, None, Error::TransportFailure),
        (9, Some(MAX_WIRE_BYTES as u32 + 1), Error::TransportFailure),
        (0, Some(0), Error::BadReply),
        (0, Some(1), Error::BadReply),
        (0, Some(MAX_WIRE_BYTES as u32 + 1), Error::BadReply),
    ] {
        let request = request();
        let mut state = State::new(&request);
        state.status = status;
        state.written = written;
        let host = state.host();
        let mut client = unsafe { Client::from_host(&host) }.unwrap();
        assert_eq!(client.call(&request), Err(expected));
        assert_eq!(state.calls, 1);
        state.status = 0;
        state.written = None;
        assert_eq!(client.call(&request).unwrap(), response(&request));
        assert_eq!(state.calls, 2);
        assert!(state.clean && state.disjoint);
    }
}

#[test]
fn invalid_request_and_host_errors_keep_their_priority() {
    let mut request = request();
    let mut state = State::new(&request);
    let mut host = state.host();
    host.call = None;
    host.abi_version = 2;
    assert!(matches!(
        unsafe { Client::from_host(&host) },
        Err(Error::AbiMismatch)
    ));
    host.abi_version = 1;
    assert!(matches!(
        unsafe { Client::from_host(&host) },
        Err(Error::InvalidArgument)
    ));
    host.call = Some(callback);
    let mut client = unsafe { Client::from_host(&host) }.unwrap();
    request.action = Action::Receive {
        last_acked: 0,
        credit_bytes: 0,
    };
    assert_eq!(client.call(&request), Err(Error::InvalidArgument));
    assert_eq!(state.calls, 0);
}

#[test]
fn public_validation_checks_response_then_identity_then_request_digest() {
    use morrow_plugin_sdk::channel::Error as WireError;
    let good = request();
    let mut malformed = good.clone();
    malformed.action = Action::Receive {
        last_acked: 0,
        credit_bytes: 0,
    };
    for identity in 0..3 {
        let mut reply = response(&good);
        match identity {
            0 => reply.call_id = [4; 32],
            1 => reply.reference = [4; 32],
            _ => {
                reply.source_epoch = [4; 32];
                reply.frame.as_mut().unwrap().source_epoch = [4; 32];
            }
        }
        assert_eq!(reply.validate_for(&malformed), Err(WireError::Correlation));
    }
    let mut reply = response(&good);
    assert_eq!(reply.validate_for(&malformed), Err(WireError::Limit));
    reply.call_id = [0; 32];
    assert_eq!(reply.validate_for(&malformed), Err(WireError::Invalid));
}

unsafe extern "C" {
    fn mp_channel_response_decode(
        input: *const u8,
        length: u32,
        original: *const u8,
        original_length: u32,
        out: *mut *mut c_void,
    ) -> u32;
    fn mp_channel_response_free(handle: *mut c_void);
    fn mp_channel_request_encode(
        raw: *const c_void,
        out: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> u32;
}

#[test]
fn c_response_decode_hashes_original_once_without_reencoding_for_correlation() {
    let request = request();
    let input = request.encode().unwrap();
    let output = response(&request).encode().unwrap();
    let mut handle = std::ptr::null_mut();
    WIRE_ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    let code = unsafe {
        mp_channel_response_decode(
            output.as_ptr(),
            output.len() as u32,
            input.as_ptr(),
            input.len() as u32,
            &mut handle,
        )
    };
    COUNTING.with(|enabled| enabled.set(false));
    let allocations = WIRE_ALLOCATIONS.with(Cell::get);
    unsafe { mp_channel_response_free(handle) };
    assert_eq!(code, 0);
    // Canonical request decode, response decode and owned frame digest only.
    assert_eq!(allocations, 3);
}

#[test]
fn c_decode_retains_malformed_argument_priority() {
    let request = request();
    let original = request.encode().unwrap();
    let reply = response(&request).encode().unwrap();
    let mut handle = std::ptr::null_mut();
    assert_eq!(
        unsafe {
            mp_channel_response_decode(
                std::ptr::null(),
                u32::MAX,
                std::ptr::null(),
                u32::MAX,
                std::ptr::null_mut(),
            )
        },
        16
    );
    assert_eq!(
        unsafe {
            mp_channel_response_decode(
                std::ptr::null(),
                1,
                std::ptr::null(),
                MAX_WIRE_BYTES as u32 + 1,
                &mut handle,
            )
        },
        17
    );
    assert!(handle.is_null());
    assert_eq!(
        unsafe {
            mp_channel_response_decode(
                std::ptr::null(),
                MAX_WIRE_BYTES as u32 + 1,
                original.as_ptr(),
                1,
                &mut handle,
            )
        },
        17
    );
    assert_eq!(
        unsafe {
            mp_channel_response_decode(
                reply.as_ptr(),
                reply.len() as u32,
                original.as_ptr(),
                1,
                &mut handle,
            )
        },
        16
    );
    assert!(handle.is_null());
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Span {
    data: *const u8,
    length: u32,
}
#[repr(C)]
struct CRequest {
    abi_version: u32,
    struct_size: u32,
    kind: u32,
    call_id: Span,
    reference: Span,
    source_epoch: Span,
    sequence: u64,
    credit_bytes: u32,
    frame_sha256: Span,
    cursor: Span,
    bytes: Span,
}

#[test]
fn c_encoder_copies_overlapping_descriptor_and_spans_before_output() {
    let mut storage = vec![0u64; MAX_WIRE_BYTES / 8];
    let pointer = storage.as_mut_ptr().cast::<u8>();
    let ids_offset = size_of::<CRequest>();
    let payload_offset = ids_offset + 96;
    let expected = Request {
        action: Action::Send {
            sequence: 7,
            bytes: vec![0, 255, 42],
        },
        ..request()
    };
    let span = |offset, length| Span {
        data: unsafe { pointer.add(offset) },
        length,
    };
    unsafe {
        std::ptr::copy_nonoverlapping(expected.call_id.as_ptr(), pointer.add(ids_offset), 32);
        std::ptr::copy_nonoverlapping(
            expected.reference.as_ptr(),
            pointer.add(ids_offset + 32),
            32,
        );
        std::ptr::copy_nonoverlapping(
            expected.source_epoch.as_ptr(),
            pointer.add(ids_offset + 64),
            32,
        );
        std::ptr::copy_nonoverlapping([0, 255, 42].as_ptr(), pointer.add(payload_offset), 3);
        pointer.cast::<CRequest>().write(CRequest {
            abi_version: 1,
            struct_size: size_of::<CRequest>() as u32,
            kind: 3,
            call_id: span(ids_offset, 32),
            reference: span(ids_offset + 32, 32),
            source_epoch: span(ids_offset + 64, 32),
            sequence: 7,
            credit_bytes: 0,
            frame_sha256: Span::default(),
            cursor: Span::default(),
            bytes: span(payload_offset, 3),
        });
    }
    let mut written = 99;
    assert_eq!(
        unsafe {
            mp_channel_request_encode(pointer.cast(), pointer, MAX_WIRE_BYTES as u32, &mut written)
        },
        0
    );
    let bytes = unsafe { std::slice::from_raw_parts(pointer, written as usize) };
    assert_eq!(Request::decode(bytes).unwrap(), expected);
}
