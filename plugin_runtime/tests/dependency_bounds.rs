use morrow_plugin_runtime::{Cancellation, Fault, Limits, MAX_TASK_BYTES, Runner};

const READ: &str = "i32.const 0 i32.const 131072 call $read drop";
const CALL: &str = "i32.const 0 i32.const 1 i32.const 131072 i32.const 131072 call $dep";
fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
        (import "morrow_dependency_v1" "call" (func $dep (param i32 i32 i32 i32) (result i32)))
        (import "morrow_v1" "exchange" (func $core (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 4)
        (func (export "morrow_run") (result i32) (local $n i32) {body}))"#
    ))
    .unwrap()
}
fn runner(body: &str) -> Runner {
    Runner::new_dependency_task(&module(body), Limits::default()).unwrap()
}
fn completing(body: &str) -> String {
    format!("{READ} {body} local.set $n i32.const 131072 local.get $n call $done drop i32.const 0")
}

#[test]
fn ordinary_modes_reject_import_and_callbacks_cannot_cross_runner_modes() {
    let wasm = module(&completing(CALL));
    assert!(matches!(
        Runner::new(&wasm, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    assert!(matches!(
        Runner::new_task(&wasm, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    let runner = Runner::new_dependency_task(&wasm, Limits::default()).unwrap();
    let mut no_call = |_: &[u8]| panic!("callback must not execute");
    let result = runner.run_task(&[1], &mut no_call, Cancellation::default());
    assert_eq!(result.report.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(result.report.host_calls, 0);
    assert_eq!(
        runner.run(&mut no_call, Cancellation::default()).outcome,
        Err(Fault::UnsupportedAbi)
    );
    let plain = wat::parse_str(r#"(module (memory (export "memory") 4) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
    for ordinary in [
        Runner::new(&plain, Limits::default()).unwrap(),
        Runner::new_task(&plain, Limits::default()).unwrap(),
    ] {
        let result = ordinary.run_task_with_dependencies(
            &[1],
            &mut no_call,
            &mut |_| panic!(),
            Cancellation::default(),
        );
        assert_eq!(result.report.outcome, Err(Fault::UnsupportedAbi));
        assert_eq!(result.report.host_calls, 0);
    }
}

#[test]
fn dependency_mode_admits_only_the_exact_additional_function_signature() {
    for import in [
        r#"(import "morrow_dependency_v1" "other" (func))"#,
        r#"(import "morrow_dependency_v2" "call" (func (param i32 i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_dependency_v1" "call" (func (param i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_dependency_v1" "call" (func (param i32 i32 i32 i64) (result i32)))"#,
        r#"(import "morrow_dependency_v1" "call" (func (param i32 i32 i32 i32)))"#,
        r#"(import "morrow_dependency_v1" "call" (global i32))"#,
        r#"(import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_dependency_v1" "call" (func (param i32 i32 i32 i32) (result i32))) (import "morrow_dependency_v1" "call" (func (param i32 i32 i32 i32) (result i32)))"#,
    ] {
        let bytes = wat::parse_str(format!(r#"(module {import} (memory (export "memory") 4) (func (export "morrow_run") (result i32) i32.const 0))"#)).unwrap();
        assert!(
            matches!(
                Runner::new_dependency_task(&bytes, Limits::default()),
                Err(Fault::UnsupportedAbi)
            ),
            "{import}"
        );
    }
}

#[test]
fn successful_dependency_exchange_delivers_bytes_and_counts_one_import_call() {
    let runner = runner(&completing(CALL));
    let mut invoked = 0;
    let result = runner.run_task_with_dependencies(
        &[7],
        &mut |_| panic!("not a core transaction"),
        &mut |bytes| {
            invoked += 1;
            assert_eq!(bytes, &[7]);
            Ok(vec![8, 9])
        },
        Cancellation::default(),
    );
    assert_eq!(invoked, 1);
    assert_eq!(result.report.host_calls, 1);
    assert_eq!(result.report.outcome, Ok(0));
    assert_eq!(result.completion, Some(vec![8, 9]));
}

#[test]
fn bad_offsets_lengths_capacities_and_any_overlap_never_reach_callback() {
    let cases = [
        (-1, 1, 131072, 131072),
        (0, -1, 131072, 131072),
        (0, 0, 131072, 131072),
        (0, 131073, 131072, 131072),
        (0, 1, -1, 131072),
        (0, 1, 131072, -1),
        (0, 1, 131072, 131071),
        (0, 1, 131072, 131073),
        (262144, 1, 131072, 131072),
        (262143, 2, 0, 131072),
        (0, 1, 131073, 131072), // Whole output region, not merely actual response length.
        (i32::MAX, 1, 131072, 131072),
        (0, 1, i32::MAX, 131072),
        (0, 1, 0, 131072),
        (131071, 2, 0, 131072),
        (0, 2, 1, 131072),
    ];
    for (input, length, output, capacity) in cases {
        let call = format!(
            "i32.const {input} i32.const {length} i32.const {output} i32.const {capacity} call $dep"
        );
        let result = runner(&completing(&call)).run_task_with_dependencies(
            &[7],
            &mut |_| panic!(),
            &mut |_| panic!("invalid range reached callback"),
            Cancellation::default(),
        );
        assert_eq!(result.report.outcome, Err(Fault::Trap), "{call}");
        assert_eq!(result.report.host_calls, 0);
        assert!(result.completion.is_none());
    }
}

#[test]
fn task_input_must_be_read_and_completion_must_not_have_occurred() {
    for body in [
        format!("{CALL} drop i32.const 0"),
        format!("{READ} i32.const 0 i32.const 1 call $done drop {CALL} drop i32.const 0"),
    ] {
        let result = runner(&body).run_task_with_dependencies(
            &[7],
            &mut |_| panic!(),
            &mut |_| panic!("bad task order reached callback"),
            Cancellation::default(),
        );
        assert_eq!(result.report.outcome, Err(Fault::TaskProtocol));
        assert_eq!(result.report.host_calls, 0);
        assert!(result.completion.is_none());
    }
}

#[test]
fn maximum_input_output_and_adjacent_regions_are_supported() {
    let call = "i32.const 0 i32.const 131072 i32.const 131072 i32.const 131072 call $dep";
    let source = vec![19; MAX_TASK_BYTES];
    let result = runner(&completing(call)).run_task_with_dependencies(
        &source,
        &mut |_| panic!(),
        &mut |input| {
            assert_eq!(input, source);
            Ok(vec![20; MAX_TASK_BYTES])
        },
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Ok(0));
    assert_eq!(result.report.host_calls, 1);
    assert_eq!(result.completion, Some(vec![20; MAX_TASK_BYTES]));
}

#[test]
fn callback_failure_empty_and_oversized_responses_trap_without_completion() {
    for response in [Err(()), Ok(vec![]), Ok(vec![0; MAX_TASK_BYTES + 1])] {
        let result = runner(&completing(CALL)).run_task_with_dependencies(
            &[7],
            &mut |_| panic!(),
            &mut |_| response.clone(),
            Cancellation::default(),
        );
        assert_eq!(result.report.outcome, Err(Fault::TaskProtocol));
        assert_eq!(result.report.host_calls, 1);
        assert!(result.completion.is_none());
    }
}

#[test]
fn dependency_and_core_calls_share_one_limit_in_both_directions() {
    let core = "i32.const 0 i32.const 1 i32.const 131072 i32.const 65536 call $core";
    for calls in [
        format!("{CALL} drop {CALL} drop"),
        format!("{core} drop {CALL} drop"),
        format!("{CALL} drop {core} drop"),
    ] {
        let wasm = module(&format!("{READ} {calls} i32.const 0"));
        let runner = Runner::new_dependency_task(
            &wasm,
            Limits {
                host_calls: 1,
                ..Limits::default()
            },
        )
        .unwrap();
        let core_calls = std::cell::Cell::new(0);
        let dep_calls = std::cell::Cell::new(0);
        let result = runner.run_task_with_dependencies(
            &[7],
            &mut |_| {
                core_calls.set(core_calls.get() + 1);
                Ok(vec![1])
            },
            &mut |_| {
                dep_calls.set(dep_calls.get() + 1);
                Ok(vec![2])
            },
            Cancellation::default(),
        );
        assert_eq!(result.report.outcome, Err(Fault::Limits));
        assert_eq!(result.report.host_calls, 1);
        assert_eq!(core_calls.get() + dep_calls.get(), 1);
        assert!(result.completion.is_none());
    }
    let runner = Runner::new_dependency_task(
        &module(&completing(CALL)),
        Limits {
            host_calls: 0,
            ..Limits::default()
        },
    )
    .unwrap();
    let result = runner.run_task_with_dependencies(
        &[7],
        &mut |_| panic!(),
        &mut |_| panic!(),
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Err(Fault::Limits));
    assert_eq!(result.report.host_calls, 0);
}

#[test]
fn cancellation_before_and_during_callback_never_delivers_completion() {
    let runner = runner(&completing(CALL));
    let pre = Cancellation::default();
    pre.cancel();
    let result = runner.run_task_with_dependencies(&[1], &mut |_| panic!(), &mut |_| panic!(), pre);
    assert_eq!(result.report.outcome, Err(Fault::Cancelled));
    assert_eq!(result.report.host_calls, 0);
    let during = Cancellation::default();
    let signal = during.clone();
    let mut performed = 0;
    let result = runner.run_task_with_dependencies(
        &[1],
        &mut |_| panic!(),
        &mut |_| {
            performed += 1;
            signal.cancel();
            Ok(vec![2])
        },
        during,
    );
    assert_eq!(performed, 1); // Cancellation does not roll back the trusted callback.
    assert_eq!(result.report.outcome, Err(Fault::Cancelled));
    assert_eq!(result.report.host_calls, 1);
    assert!(result.completion.is_none());
}

#[test]
fn memory_growth_before_and_after_callback_preserves_returned_bytes() {
    let body = format!(
        "{READ} i32.const 2 memory.grow drop i32.const 0 i32.const 1 i32.const 262144 i32.const 131072 call $dep local.set $n i32.const 2 memory.grow drop i32.const 262144 local.get $n call $done drop i32.const 0"
    );
    let result = runner(&body).run_task_with_dependencies(
        &[7],
        &mut |_| panic!(),
        &mut |bytes| {
            assert_eq!(bytes, &[7]);
            Ok(vec![9])
        },
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Ok(0));
    assert_eq!(result.completion, Some(vec![9]));
}

#[test]
fn later_guest_trap_discards_completion_but_not_callback_count() {
    let body = completing(CALL) + " drop unreachable";
    let result = runner(&body).run_task_with_dependencies(
        &[7],
        &mut |_| panic!(),
        &mut |_| Ok(vec![9]),
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Err(Fault::Trap));
    assert_eq!(result.report.host_calls, 1);
    assert!(result.completion.is_none());
}
