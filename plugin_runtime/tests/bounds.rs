use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner};
fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!(r#"(module (import "morrow_v1" "exchange" (func $exchange (param i32 i32 i32 i32) (result i32))) (memory (export "memory") 2) (func (export "morrow_run") (result i32) {body}))"#)).unwrap()
}
fn runner(body: &str, limits: Limits) -> Runner {
    Runner::new(&module(body), limits).unwrap()
}
#[test]
fn cannot_import_wasi_identity_or_memory() {
    for source in [
        r#"(module (import "wasi_snapshot_preview1" "fd_write" (func)) (memory (export "memory") 2) (func (export "morrow_run") (result i32) i32.const 0))"#,
        r#"(module (import "morrow_v1" "grant" (func)) (memory (export "memory") 2) (func (export "morrow_run") (result i32) i32.const 0))"#,
        r#"(module (import "morrow_v1" "exchange" (func (param i64))) (memory (export "memory") 2) (func (export "morrow_run") (result i32) i32.const 0))"#,
        r#"(module (import "morrow_v1" "memory" (memory 2)) (export "memory" (memory 0)) (func (export "morrow_run") (result i32) i32.const 0))"#,
    ] {
        assert!(matches!(
            Runner::new(&wat::parse_str(source).unwrap(), Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
}
#[test]
fn rejects_start_before_any_execution_and_requires_entrypoint() {
    for source in [
        r#"(module (memory (export "memory") 2) (func $start unreachable) (start $start) (func (export "morrow_run") (result i32) i32.const 0))"#,
        r#"(module (memory (export "memory") 2) (func (export "morrow_run") (result i64) i64.const 0))"#,
    ] {
        assert!(matches!(
            Runner::new(&wat::parse_str(source).unwrap(), Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
}
#[test]
fn loop_exhausts_fuel_and_next_run_is_fresh() {
    let r = runner(
        "(loop $again br $again) i32.const 0",
        Limits {
            fuel: 1000,
            ..Limits::default()
        },
    );
    for _ in 0..2 {
        let report = r.run(
            &mut |_| panic!("unexpected import"),
            Cancellation::default(),
        );
        assert_eq!(report.outcome, Err(Fault::Limits));
        assert_eq!(report.host_calls, 0);
    }
}
#[test]
fn traps_memory_growth_and_memory_load_outside_guest() {
    let r = runner(
        "i32.const 1 memory.grow",
        Limits {
            memory_bytes: 131072,
            ..Limits::default()
        },
    );
    assert_eq!(
        r.run(&mut |_| panic!(), Cancellation::default()).outcome,
        Err(Fault::Trap)
    );
    let r = runner("i32.const 131072 i32.load", Limits::default());
    assert_eq!(
        r.run(&mut |_| panic!(), Cancellation::default()).outcome,
        Err(Fault::Trap)
    );
}
#[test]
fn rejects_oversized_initial_memory() {
    let bytes=wat::parse_str(r#"(module (memory (export "memory") 257) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
    let r = Runner::new(&bytes, Limits::default()).unwrap();
    assert_eq!(
        r.run(&mut |_| panic!(), Cancellation::default()).outcome,
        Err(Fault::Limits)
    );
}
#[test]
fn all_foreign_buffer_checks_precede_submission() {
    for arguments in [
        "-1 1 65536 65536",
        "0 0 65536 65536",
        "0 65537 65536 65536",
        "0 1 65536 65535",
        "0 1 65537 65536",
        "0 1 0 65536",
        "131072 1 65536 65536",
    ] {
        let body = format!(
            "{} call $exchange",
            arguments
                .split_whitespace()
                .map(|v| format!("i32.const {v}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let report = runner(&body, Limits::default())
            .run(&mut |_| panic!("must not submit"), Cancellation::default());
        assert_eq!(report.outcome, Err(Fault::Trap));
        assert_eq!(report.host_calls, 0);
    }
}
#[test]
fn host_call_budget_terminates_spam_and_counts_attempts() {
    let r = runner(
        "(loop $again i32.const 0 i32.const 1 i32.const 65536 i32.const 65536 call $exchange drop br $again) i32.const 0",
        Limits {
            host_calls: 2,
            ..Limits::default()
        },
    );
    let mut called = 0;
    let report = r.run(
        &mut |_| {
            called += 1;
            Err(())
        },
        Cancellation::default(),
    );
    assert_eq!(report.outcome, Err(Fault::Limits));
    assert_eq!(called, 2);
    assert_eq!(report.host_calls, 2);
}
#[test]
fn cancellation_prevents_submission_and_discards_late_reply() {
    let r = runner(
        "i32.const 0 i32.const 1 i32.const 65536 i32.const 65536 call $exchange",
        Limits::default(),
    );
    let cancel = Cancellation::default();
    cancel.cancel();
    assert_eq!(
        r.run(&mut |_| panic!(), cancel).outcome,
        Err(Fault::Cancelled)
    );
    let cancel = Cancellation::default();
    let signal = cancel.clone();
    let mut called = 0;
    let report = r.run(
        &mut |_| {
            called += 1;
            signal.cancel();
            Ok(vec![1])
        },
        cancel,
    );
    assert_eq!(report.outcome, Err(Fault::Cancelled));
    assert_eq!(called, 1);
    assert_eq!(report.host_calls, 1);
}
#[test]
fn owned_input_survives_and_output_views_are_reacquired_after_grow() {
    let r = runner(
        "i32.const 0 i32.const 42 i32.store8 i32.const 1 memory.grow drop i32.const 0 i32.const 1 i32.const 65536 i32.const 65536 call $exchange drop i32.const 65536 i32.load8_u",
        Limits::default(),
    );
    let report = r.run(
        &mut |bytes| {
            assert_eq!(bytes, [42]);
            Ok(vec![99])
        },
        Cancellation::default(),
    );
    assert_eq!(report.outcome, Ok(99));
}
#[test]
fn invalid_host_response_is_not_exposed_as_success() {
    for reply in [vec![], vec![0; 65537]] {
        let r = runner(
            "i32.const 0 i32.const 1 i32.const 65536 i32.const 65536 call $exchange",
            Limits::default(),
        );
        assert_eq!(
            r.run(&mut |_| Ok(reply.clone()), Cancellation::default())
                .outcome,
            Err(Fault::Trap)
        );
    }
}
#[test]
fn module_and_administrative_limits_are_bounded() {
    assert!(matches!(
        Runner::new(&vec![0; 4 * 1024 * 1024 + 1], Limits::default()),
        Err(Fault::Limits)
    ));
    assert!(matches!(
        Runner::new(b"broken", Limits::default()),
        Err(Fault::InvalidModule)
    ));
    assert!(matches!(
        Runner::new(
            &module("i32.const 0"),
            Limits {
                fuel: 0,
                ..Limits::default()
            }
        ),
        Err(Fault::Limits)
    ));
}
