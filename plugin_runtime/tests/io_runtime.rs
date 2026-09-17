//! Raw import tests, not managed IO authorization or external-effect evidence.
use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner};

const FRAME: usize = 128 * 1024;
const READ: &str = "i32.const 0 i32.const 131072 call $read drop";
const IO: &str = "i32.const 0 i32.const 1 i32.const 131072 i32.const 131072 call $io";
const DONE: &str = "i32.const 0 i32.const 1 call $done drop i32.const 0";

fn module(body: &str) -> Vec<u8> {
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
fn runner(body: &str, calls: u32) -> Runner {
    Runner::new_io_task(
        &module(body),
        Limits {
            host_calls: calls,
            ..Limits::default()
        },
    )
    .unwrap()
}

#[test]
fn ordinary_modes_reject_io_and_combined_dependency_imports() {
    let wasm = module("i32.const 0");
    assert_eq!(
        Runner::new(&wasm, Limits::default()).err(),
        Some(Fault::UnsupportedAbi)
    );
    assert_eq!(
        Runner::new_task(&wasm, Limits::default()).err(),
        Some(Fault::UnsupportedAbi)
    );
    assert_eq!(
        Runner::new_dependency_task(&wasm, Limits::default()).err(),
        Some(Fault::UnsupportedAbi)
    );
    let mixed = wat::parse_str(
        r#"(module
      (import "morrow_io_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
      (import "morrow_dependency_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 4) (func (export "morrow_run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    assert_eq!(
        Runner::new_io_task(&mixed, Limits::default()).err(),
        Some(Fault::UnsupportedAbi)
    );
    assert_eq!(
        Runner::new_dependency_task(&mixed, Limits::default()).err(),
        Some(Fault::UnsupportedAbi)
    );
}

#[test]
fn callback_mode_mismatch_never_calls_callbacks() {
    let r = runner("i32.const 0", 1);
    let run = r.run_task(&[1], &mut |_| panic!("exchange"), Cancellation::default());
    assert_eq!(run.report.outcome, Err(Fault::UnsupportedAbi));
    let run = r.run_task_with_dependencies(
        &[1],
        &mut |_| panic!("exchange"),
        &mut |_| panic!("dependency"),
        Cancellation::default(),
    );
    assert_eq!(run.report.outcome, Err(Fault::UnsupportedAbi));
    let plain = wat::parse_str(
        r#"(module (memory (export "memory") 4)
      (func (export "morrow_run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    for r in [
        Runner::new_task(&plain, Limits::default()).unwrap(),
        Runner::new_dependency_task(&plain, Limits::default()).unwrap(),
        Runner::new(&plain, Limits::default()).unwrap(),
    ] {
        let run = r.run_task_with_io(
            &[1],
            &mut |_| panic!("exchange"),
            &mut |_| panic!("io"),
            Cancellation::default(),
        );
        assert_eq!(run.report.outcome, Err(Fault::UnsupportedAbi));
        assert_eq!(run.report.host_calls, 0);
    }
}

#[test]
fn returned_length_and_actual_response_bytes_reach_guest() {
    let r = runner(
        &format!(
            "{READ} {IO} i32.const 3 i32.ne if unreachable end i32.const 131072 i32.const 3 call $done drop i32.const 0"
        ),
        1,
    );
    let run = r.run_task_with_io(
        &[7],
        &mut |_| panic!("exchange"),
        &mut |request| {
            assert_eq!(request, [7]);
            Ok(vec![9, 8, 6])
        },
        Cancellation::default(),
    );
    assert_eq!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, 1);
    assert_eq!(run.completion, Some(vec![9, 8, 6]));
}

#[test]
fn transport_failure_returns_exact_negative_one_without_writing_output() {
    let r = runner(
        &format!(
            "{READ} i32.const 131072 i32.const 42 i32.store8 {IO} i32.const -1 i32.ne if unreachable end i32.const 131072 i32.load8_u i32.const 42 i32.ne if unreachable end {DONE}"
        ),
        1,
    );
    let run = r.run_task_with_io(
        &[7],
        &mut |_| panic!("exchange"),
        &mut |_| Err(()),
        Cancellation::default(),
    );
    assert_eq!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, 1);
    assert_eq!(run.completion, Some(vec![7]));
}

#[test]
fn cancellation_inside_callback_rejects_late_success() {
    let r = runner(&format!("{READ} {IO} drop {DONE}"), 1);
    let cancel = Cancellation::default();
    let signal = cancel.clone();
    let mut calls = 0;
    let run = r.run_task_with_io(
        &[7],
        &mut |_| panic!("exchange"),
        &mut |_| {
            calls += 1;
            signal.cancel();
            Ok(vec![9])
        },
        cancel,
    );
    assert_eq!(calls, 1);
    assert_eq!(run.report.host_calls, 1);
    assert_eq!(run.report.outcome, Err(Fault::Cancelled));
    assert!(run.completion.is_none());
}

#[test]
fn pre_cancelled_does_not_enter_callback() {
    let r = runner(&format!("{READ} {IO} drop {DONE}"), 1);
    let cancel = Cancellation::default();
    cancel.cancel();
    let run = r.run_task_with_io(
        &[7],
        &mut |_| panic!("exchange"),
        &mut |_| panic!("io"),
        cancel,
    );
    assert_eq!(run.report.outcome, Err(Fault::Cancelled));
    assert_eq!(run.report.host_calls, 0);
}

#[test]
fn invalid_order_never_enters_callback() {
    for body in [
        format!("{IO} drop {DONE}"),
        format!("{READ} i32.const 0 i32.const 1 call $done drop {IO} drop i32.const 0"),
    ] {
        let run = runner(&body, 1).run_task_with_io(
            &[7],
            &mut |_| panic!("exchange"),
            &mut |_| panic!("io"),
            Cancellation::default(),
        );
        assert_eq!(run.report.outcome, Err(Fault::TaskProtocol));
        assert_eq!(run.report.host_calls, 0);
    }
}

#[test]
fn invalid_ranges_and_full_capacity_are_checked_before_callback() {
    for (input, length, output, capacity) in [
        (-1, 1, 131072, 131072),
        (0, 1, -1, 131072),
        (0, 0, 131072, 131072),
        (0, -1, 131072, 131072),
        (0, 131073, 131072, 131072),
        (0, 1, 131072, 131071),
        (0, 1, 131072, -1),
        (0, 1, 131072, 131073),
        (0, 1, 0, 131072),
        (65536, 1, 1, 131072),
        (0, 1, 131073, 131072),
        (262144, 1, 131072, 131072),
        (i32::MAX, 2, 131072, 131072),
        (0, 1, i32::MAX, 131072),
    ] {
        let body = format!(
            "{READ} i32.const {input} i32.const {length} i32.const {output} i32.const {capacity} call $io drop {DONE}"
        );
        let run = runner(&body, 1).run_task_with_io(
            &[7],
            &mut |_| panic!("exchange"),
            &mut |_| panic!("io"),
            Cancellation::default(),
        );
        assert_eq!(
            run.report.outcome,
            Err(Fault::Trap),
            "{input}/{length}/{output}/{capacity}"
        );
        assert_eq!(run.report.host_calls, 0);
    }
}

#[test]
fn host_call_budget_rejects_zero_and_repeated_calls_before_callback() {
    for budget in [0, 2] {
        let mut calls = 0;
        let r = runner(
            &format!("{READ} {IO} drop {IO} drop {IO} drop {DONE}"),
            budget,
        );
        let run = r.run_task_with_io(
            &[7],
            &mut |_| panic!("exchange"),
            &mut |_| {
                calls += 1;
                Ok(vec![8])
            },
            Cancellation::default(),
        );
        assert_eq!(run.report.outcome, Err(Fault::Limits));
        assert_eq!(calls, budget);
        assert_eq!(run.report.host_calls, budget);
        assert!(run.completion.is_none());
    }
}

#[test]
fn exchange_and_io_share_one_budget() {
    for io_first in [true, false] {
        let ex = "i32.const 0 i32.const 1 i32.const 131072 i32.const 65536 call $ex drop";
        let body = if io_first {
            format!("{READ} {IO} drop {ex} {DONE}")
        } else {
            format!("{READ} {ex} {IO} drop {DONE}")
        };
        let (mut exchanges, mut ios) = (0, 0);
        let run = runner(&body, 1).run_task_with_io(
            &[7],
            &mut |_| {
                exchanges += 1;
                Ok(vec![8])
            },
            &mut |_| {
                ios += 1;
                Ok(vec![9])
            },
            Cancellation::default(),
        );
        assert_eq!(run.report.outcome, Err(Fault::Limits));
        assert_eq!(run.report.host_calls, 1);
        assert_eq!((exchanges, ios), if io_first { (0, 1) } else { (1, 0) });
    }
}

#[test]
fn malformed_callback_response_is_not_delivered_as_success() {
    for response in [vec![], vec![1; FRAME + 1]] {
        let mut calls = 0;
        let run = runner(&format!("{READ} {IO} drop {DONE}"), 1).run_task_with_io(
            &[7],
            &mut |_| panic!("exchange"),
            &mut |_| {
                calls += 1;
                Ok(response.clone())
            },
            Cancellation::default(),
        );
        assert_eq!(run.report.outcome, Err(Fault::Trap));
        assert_eq!(calls, 1);
        assert_eq!(run.report.host_calls, 1);
        assert!(run.completion.is_none());
    }
}

#[test]
fn exact_maximum_input_and_response_fit_nonoverlapping_regions() {
    let body = format!(
        "{READ} i32.const 0 i32.const 131072 i32.const 131072 i32.const 131072 call $io i32.const 131072 i32.ne if unreachable end i32.const 131072 i32.const 131072 call $done drop i32.const 0"
    );
    let input = vec![7; FRAME];
    let run = runner(&body, 1).run_task_with_io(
        &input,
        &mut |_| panic!("exchange"),
        &mut |request| {
            assert_eq!(request, input);
            Ok(vec![9; FRAME])
        },
        Cancellation::default(),
    );
    assert_eq!(run.report.outcome, Ok(0));
    assert_eq!(run.report.host_calls, 1);
    assert_eq!(run.completion, Some(vec![9; FRAME]));
}
