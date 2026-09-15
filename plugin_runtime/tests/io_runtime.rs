use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner};

fn io_module(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
            (import "morrow_v1" "exchange" (func $ex (param i32 i32 i32 i32) (result i32)))
            (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
            (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
            (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 4)
            (func (export "morrow_run") (result i32) {body}))"#
    ))
    .unwrap()
}

fn read_then_io_then_complete() -> &'static str {
    "i32.const 0 i32.const 131072 call $read drop i32.const 0 i32.const 1 i32.const 256 i32.const 131072 call $io drop i32.const 0 i32.const 1 call $done drop i32.const 0"
}

#[test]
fn ordinary_task_runner_rejects_io_import() {
    let wasm = io_module("i32.const 0");
    assert_eq!(Runner::new_task(&wasm, Limits::default()).err(), Some(Fault::UnsupportedAbi));
}

#[test]
fn io_runner_requires_matching_callback() {
    let wasm = io_module(read_then_io_then_complete());
    let runner = Runner::new_io_task(&wasm, Limits::default()).unwrap();
    let missing = runner.run_task(&[1], &mut |_| panic!("exchange"), Cancellation::default());
    assert_eq!(missing.report.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(missing.report.host_calls, 0);
}

#[test]
fn io_call_counts_against_host_budget_and_returns_frame() {
    let wasm = io_module(read_then_io_then_complete());
    let runner = Runner::new_io_task(&wasm, Limits::default()).unwrap();
    let mut seen = Vec::new();
    let run = runner.run_task_with_io(
        &[7],
        &mut |_| panic!("no content exchange"),
        &mut |request| {
            seen = request.to_vec();
            Ok(vec![9, 9, 9])
        },
        Cancellation::default(),
    );
    assert_eq!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, 1);
    assert_eq!(seen, [7]);
    assert_eq!(run.completion.as_deref(), Some(&[7][..]));
}

#[test]
fn io_transport_error_is_negative_not_success_payload() {
    let wasm = io_module(read_then_io_then_complete());
    let runner = Runner::new_io_task(&wasm, Limits::default()).unwrap();
    let run = runner.run_task_with_io(
        &[1],
        &mut |_| panic!("no content exchange"),
        &mut |_| Err(()),
        Cancellation::default(),
    );
    assert_eq!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, 1);
}

#[test]
fn cancelled_io_does_not_deliver_late_success() {
    let wasm = io_module(read_then_io_then_complete());
    let runner = Runner::new_io_task(&wasm, Limits::default()).unwrap();
    let cancel = Cancellation::default();
    cancel.cancel();
    let run = runner.run_task_with_io(
        &[1],
        &mut |_| panic!("no content exchange"),
        &mut |_| Ok(vec![1]),
        cancel,
    );
    assert_eq!(run.report.outcome, Err(Fault::Cancelled));
    assert!(run.completion.is_none());
}
