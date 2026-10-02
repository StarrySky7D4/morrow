use morrow_plugin_sdk::channel::{Action, Frame, MAX_WIRE_BYTES, Request, Response, Status, transport::{Client, HostV1}};
use std::ffi::c_void;
struct State { input: Vec<u8>, reply: Vec<u8>, calls: u32 }
unsafe extern "C" fn callback(context: *mut c_void, input: *const u8, length: u32,
    output: *mut u8, capacity: u32, written: *mut u32) -> u32 {
    let state = unsafe { &mut *context.cast::<State>() };
    let input = unsafe { std::slice::from_raw_parts(input, length as usize) };
    let output = unsafe { std::slice::from_raw_parts_mut(output, capacity as usize) };
    assert_eq!(input, state.input);
    assert_eq!(output.len(), MAX_WIRE_BYTES);
    assert!(output.iter().all(|byte| *byte == 0));
    assert!((input.as_ptr() as usize + input.len() <= output.as_ptr() as usize)
        || (output.as_ptr() as usize + output.len() <= input.as_ptr() as usize));
    output.fill(0xa5); output[..state.reply.len()].copy_from_slice(&state.reply);
    unsafe { *written = state.reply.len() as u32 };
    state.calls += 1; 0
}
fn main() {
    assert_eq!(size_of::<HostV1>(), 24);
    let request = Request { call_id: [1; 32], reference: [2; 32], source_epoch: [3; 32],
        action: Action::Receive { last_acked: 0, credit_bytes: 65536 } };
    let expected = Response { call_id: request.call_id, reference: request.reference,
        source_epoch: request.source_epoch, request_sha256: request.digest().unwrap(),
        status: Status::Frame, frame: Some(Frame { sequence: 1, source_epoch: request.source_epoch,
            bytes: vec![0xff; 65536], cursor: vec![0, 255] }), last_acked: 0,
        accepted_sequence: 0, resource_reclaimed: false };
    let mut state = State { input: request.encode().unwrap(), reply: expected.encode().unwrap(), calls: 0 };
    let host = HostV1 { abi_version: 1, struct_size: size_of::<HostV1>() as u32,
        context: (&mut state as *mut State).cast(), call: Some(callback) };
    let mut client = unsafe { Client::from_host(&host) }.unwrap();
    let first = client.call(&request).unwrap();
    assert_eq!(client.call(&request).unwrap(), expected);
    assert_eq!(first, expected); assert_eq!(state.calls, 2);
    println!("PASS standalone Rust reusable native client and owned frame");
}
