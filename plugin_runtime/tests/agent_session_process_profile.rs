//! Single-import admission and effect boundaries; not an OS sandbox qualification.
use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner};

fn module(extra: &str, body: &str) -> Vec<u8> {
    wat::parse_str(format!(r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (import "morrow_agent_session_process_v1" "call" (func $call (param i32 i32 i32 i32) (result i32)))
      {extra}
      (memory (export "memory") 6)
      (func (export "morrow_run") (result i32) (local $n i32)
        (local.set $n (call $read (i32.const 0) (i32.const 131072)))
        {body}))"#)).unwrap()
}
const CALL: &str = "(local.set $n (call $call (i32.const 0) (local.get $n) (i32.const 131072) (i32.const 131072)))";
fn one() -> Vec<u8> {
    module(
        "",
        &format!("{CALL} (drop (call $done (i32.const 131072) (local.get $n))) (i32.const 0)"),
    )
}
#[test]
fn only_explicit_new_factory_admits_the_single_import() {
    type Factory = fn(&[u8], Limits) -> Result<Runner, Fault>;
    let factories: [Factory; 8] = [
        Runner::new,
        Runner::new_task,
        Runner::new_dependency_task,
        Runner::new_io_task,
        Runner::new_mutation_task,
        Runner::new_channel_task,
        Runner::new_directory_task,
        Runner::new_agent_session_exec_task,
    ];
    for factory in factories {
        assert!(matches!(
            factory(&one(), Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
    assert!(Runner::new_agent_session_process_task(&one(), Limits::default()).is_ok());
}
#[test]
fn no_second_extra_profile_or_future_abi_is_admitted() {
    for profile in [
        "morrow_agent_session_exec_v1",
        "morrow_io_v1",
        "morrow_dependency_v1",
        "morrow_mutation_v1",
        "morrow_channel_v1",
        "morrow_fs_directory_v1",
        "wasi_snapshot_preview1",
        "morrow_agent_session_process_v1",
    ] {
        let extra =
            format!("(import \"{profile}\" \"call\" (func (param i32 i32 i32 i32) (result i32)))");
        assert!(matches!(
            Runner::new_agent_session_process_task(
                &module(&extra, "i32.const 0"),
                Limits::default()
            ),
            Err(Fault::UnsupportedAbi)
        ));
    }
    for replacement in ["morrow_agent_session_process_v2", "unreviewed"] {
        let bytes = wat::parse_str(format!(
            r#"(module
          (import "morrow_task_v1" "read_input" (func (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func (param i32 i32) (result i32)))
          (import "{replacement}" "call" (func (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 6)
          (func (export "morrow_run") (result i32) i32.const 0))"#
        ))
        .unwrap();
        assert!(Runner::new_agent_session_process_task(&bytes, Limits::default()).is_err());
    }
}
#[test]
fn unrelated_entrypoints_cannot_drive_the_new_profile() {
    let runner = Runner::new_agent_session_process_task(&one(), Limits::default()).unwrap();
    let mut route = |_: &[u8]| panic!("unnegotiated entrypoint must not reach host");
    let old = runner.run_agent_session_exec_task(&[1], &mut route, Cancellation::default());
    assert_eq!(old.report.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(old.report.host_calls, 0);
    let ordinary = runner.run_task(&[1], &mut route, Cancellation::default());
    assert_eq!(ordinary.report.outcome, Err(Fault::UnsupportedAbi));
    assert!(ordinary.completion.is_none());
    let ordinary_io = runner.run_task_with_io(
        &[1],
        &mut |_| panic!(),
        &mut |_| panic!(),
        Cancellation::default(),
    );
    assert_eq!(ordinary_io.report.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(ordinary_io.report.host_calls, 0);
}
#[test]
fn negotiated_route_copies_output_once_without_granting_raw_core_exchange() {
    let runner = Runner::new_agent_session_process_task(&one(), Limits::default()).unwrap();
    let mut count = 0;
    let result = runner.run_agent_session_process_task(
        &[4, 5],
        &mut |input| {
            count += 1;
            assert_eq!(input, &[4, 5]);
            Ok(vec![7, 8])
        },
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Ok(0));
    assert_eq!(result.completion, Some(vec![7, 8]));
    assert_eq!(result.report.host_calls, 1);
    assert_eq!(count, 1);
}
#[test]
fn cancellation_after_effect_denies_delivery_without_replaying_the_callback() {
    let cancel = Cancellation::default();
    let runner = Runner::new_agent_session_process_task(&one(), Limits::default()).unwrap();
    let mut count = 0;
    let result = runner.run_agent_session_process_task(
        &[1],
        &mut |_| {
            count += 1;
            cancel.cancel();
            Ok(vec![2])
        },
        cancel.clone(),
    );
    assert_eq!(count, 1);
    assert_eq!(result.report.outcome, Err(Fault::Cancelled));
    assert!(result.completion.is_none());
}
#[test]
fn host_call_budget_is_shared_by_every_frame_and_never_refunded() {
    let bytes = module(
        "",
        &format!("{CALL} {CALL} (drop (call $done (i32.const 131072) (local.get $n))) i32.const 0"),
    );
    let runner = Runner::new_agent_session_process_task(
        &bytes,
        Limits {
            host_calls: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    let mut count = 0;
    let result = runner.run_agent_session_process_task(
        &[1],
        &mut |_| {
            count += 1;
            Ok(vec![1])
        },
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Err(Fault::Limits));
    assert_eq!(result.report.host_calls, 1);
    assert_eq!(count, 1);
    assert!(result.completion.is_none());
}
