use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner};
fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!(r#"(module (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32))) (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32))) (memory (export "memory") 4) (func (export "morrow_run") (result i32) {body}))"#)).unwrap()
}
#[test]
fn task_abi_rejects_wrong_modes_buffers_repetition_and_missing_completion() {
    let cases = [
        "i32.const -1 i32.const 131072 call $read drop i32.const 0",
        "i32.const 0 i32.const 65536 call $read drop i32.const 0",
        "i32.const 196608 i32.const 131072 call $read drop i32.const 0",
        "i32.const 0 i32.const 131072 call $read drop i32.const 0 i32.const 131072 call $read drop i32.const 0",
        "i32.const 0 i32.const 1 call $done drop i32.const 0",
        "i32.const 0 i32.const 131072 call $read drop i32.const 0 i32.const 0 call $done drop i32.const 0",
        "i32.const 0 i32.const 131072 call $read drop i32.const 262144 i32.const 1 call $done drop i32.const 0",
        "i32.const 0 i32.const 131072 call $read drop i32.const 0 i32.const 1 call $done drop i32.const 0 i32.const 1 call $done drop i32.const 0",
        "i32.const 0",
    ];
    for body in cases {
        let wasm = module(body);
        assert!(matches!(
            Runner::new(&wasm, Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
        let runner = Runner::new_task(&wasm, Limits::default()).unwrap();
        let r = runner.run_task(
            &[1],
            &mut |_| panic!("no core callback"),
            Cancellation::default(),
        );
        assert!(r.report.outcome.is_err());
        assert_eq!(r.report.host_calls, 0);
        assert!(r.completion.is_none());
        assert_eq!(
            runner
                .run(&mut |_| panic!(), Cancellation::default())
                .outcome,
            Err(Fault::UnsupportedAbi)
        );
    }
}
#[test]
fn completion_is_copied_and_discarded_after_later_trap_or_cancellation() {
    let body = "i32.const 0 i32.const 131072 call $read drop i32.const 0 i32.const 1 call $done drop i32.const 0 i32.const 99 i32.store8 i32.const 0";
    let runner = Runner::new_task(&module(body), Limits::default()).unwrap();
    let r = runner.run_task(&[7], &mut |_| panic!(), Cancellation::default());
    assert_eq!(r.report.outcome, Ok(0));
    assert_eq!(r.completion, Some(vec![7]));
    let runner = Runner::new_task(
        &module(&format!("{body} drop unreachable")),
        Limits::default(),
    )
    .unwrap();
    let r = runner.run_task(&[7], &mut |_| panic!(), Cancellation::default());
    assert_eq!(r.report.outcome, Err(Fault::Trap));
    assert!(r.completion.is_none());
    let c = Cancellation::default();
    c.cancel();
    let r = runner.run_task(&[7], &mut |_| panic!(), c);
    assert_eq!(r.report.outcome, Err(Fault::Cancelled));
    assert_eq!(r.report.host_calls, 0);
}
