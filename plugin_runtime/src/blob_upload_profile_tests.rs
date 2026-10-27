//! Memory/continuation checks only; these raw bytes are never an upload authorization.
use super::*;
fn runner(
    input: i32,
    length: i32,
    output: i32,
    capacity: i32,
    read: bool,
    limits: Limits,
) -> Runner {
    let read = if read {
        "i32.const 0 i32.const 131072 call $read drop"
    } else {
        ""
    };
    let wat = format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
        (import "morrow_blob_upload_v1" "call" (func $upload (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 4) (data (i32.const 4096) "A")
        (func (export "morrow_run") (result i32)
            {read}
            i32.const {input} i32.const {length} i32.const {output} i32.const {capacity} call $upload drop
            i32.const 131072 i32.const 1 call $complete drop i32.const 0))"#
    );
    Runner::new_blob_upload_task(&wat::parse_str(wat).unwrap(), limits).unwrap()
}
#[test]
fn max_upload_request_suspends_one_original_owned_frame_then_completes() {
    let r = runner(4096, 65536, 131072, 4096, true, Limits::default());
    let mut e = continuation::Execution::start(&r, Some(&[1]), Cancellation::default()).unwrap();
    let p = e.pending().unwrap();
    assert_eq!(p.kind, continuation::Kind::BlobUpload);
    assert_eq!(p.bytes.len(), 65536);
    assert_eq!(p.bytes[0], b'A');
    let token = p.token.clone();
    e.resume(&token, Ok(vec![b'Z'])).unwrap();
    assert!(e.pending().is_none());
    let out = e.finish();
    assert_eq!(out.report.outcome, Ok(0));
    assert_eq!(out.report.host_calls, 1);
    assert_eq!(out.completion, Some(vec![b'Z']));
}
#[test]
fn invalid_upload_memory_ranges_never_become_pending_calls() {
    for (input, len, out, cap) in [
        (-1, 1, 131072, 4096),
        (4096, 0, 131072, 4096),
        (4096, -1, 131072, 4096),
        (4096, 65537, 131072, 4096),
        (4096, 1, 131072, 4095),
        (4096, 1, 131072, -1),
        (4096, 1, 4096, 4096),
        (262144, 1, 131072, 4096),
        (4096, 1, 262144, 4096),
        (4096, 65536, 65536, 4096),
    ] {
        let r = runner(input, len, out, cap, true, Limits::default());
        let mut e =
            continuation::Execution::start(&r, Some(&[1]), Cancellation::default()).unwrap();
        assert!(e.pending().is_none());
        let out = e.finish();
        assert_eq!(out.report.outcome, Err(Fault::Trap));
        assert!(out.completion.is_none());
        assert_eq!(out.report.host_calls, 0);
    }
}
#[test]
fn upload_before_input_read_and_exhausted_host_call_budget_fail_closed() {
    let r = runner(4096, 1, 131072, 4096, false, Limits::default());
    let mut e = continuation::Execution::start(&r, Some(&[1]), Cancellation::default()).unwrap();
    assert!(e.pending().is_none());
    assert_eq!(e.finish().report.outcome, Err(Fault::TaskProtocol));
    let r = runner(
        4096,
        1,
        131072,
        4096,
        true,
        Limits {
            host_calls: 0,
            ..Limits::default()
        },
    );
    let mut e = continuation::Execution::start(&r, Some(&[1]), Cancellation::default()).unwrap();
    assert!(e.pending().is_none());
    let out = e.finish();
    assert_eq!(out.report.outcome, Err(Fault::Limits));
    assert_eq!(out.report.host_calls, 0);
}
#[test]
fn foreign_continuation_token_cannot_consume_the_original_upload_call() {
    let r = runner(4096, 1, 131072, 4096, true, Limits::default());
    let mut a = continuation::Execution::start(&r, Some(&[1]), Cancellation::default()).unwrap();
    let foreign = a.pending().unwrap().token.clone();
    let mut b = continuation::Execution::start(&r, Some(&[1]), Cancellation::default()).unwrap();
    assert_eq!(b.resume(&foreign, Ok(vec![1])), Err(Fault::TaskProtocol));
    let original = b.pending().unwrap().token.clone();
    b.resume(&original, Ok(vec![2])).unwrap();
    assert_eq!(b.finish().completion, Some(vec![2]));
    a.abort(Fault::Cancelled);
    assert_eq!(a.finish().report.outcome, Err(Fault::Cancelled));
}
#[test]
fn revoked_or_oversized_delivery_cannot_resume_the_upload_guest() {
    let r = runner(4096, 1, 131072, 4096, true, Limits::default());
    let cancel = Cancellation::default();
    let mut a = continuation::Execution::start(&r, Some(&[1]), cancel.clone()).unwrap();
    let token = a.pending().unwrap().token.clone();
    cancel.cancel();
    a.resume(&token, Ok(vec![2])).unwrap();
    let out = a.finish();
    assert_eq!(out.report.outcome, Err(Fault::Cancelled));
    assert!(out.completion.is_none());
    let mut b = continuation::Execution::start(&r, Some(&[1]), Cancellation::default()).unwrap();
    let token = b.pending().unwrap().token.clone();
    b.resume(&token, Ok(vec![2; 4097])).unwrap();
    let out = b.finish();
    assert_eq!(out.report.outcome, Err(Fault::Trap));
    assert!(out.completion.is_none());
}
