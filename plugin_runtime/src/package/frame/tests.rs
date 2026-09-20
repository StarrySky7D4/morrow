use super::*;
use crate::Limits;
use morrow_core::plugin_package::{Package, io};

fn prepared(core_calls: usize) -> PreparedPackage {
    let core = "i32.const 0 i32.const 1 i32.const 131072 i32.const 65536 call $core i32.const -1 i32.ne if unreachable end ".repeat(core_calls);
    let module = wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
        (import "morrow_v1" "exchange" (func $core (param i32 i32 i32 i32) (result i32)))
        (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 4)
        (func (export "morrow_run") (result i32) (local $n i32)
          i32.const 0 i32.const 131072 call $read local.set $n
          {core}
          i32.const 131072
          i32.const 0 local.get $n i32.const 131072 i32.const 131072 call $io
          call $done drop i32.const 0))"#
    ))
    .unwrap();
    let mut manifest = Package::manifest_for_task("owned.frame", "1.0.0", &module, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    manifest.io_declaration = Some(io::declaration(
        vec![io::IoCapability::HttpRequest],
        vec!["api.invoke".into()],
    ));
    PreparedPackage::new(
        Package::build(manifest, &module).unwrap(),
        Limits::default(),
    )
    .unwrap()
}

#[test]
fn owned_service_frame_survives_package_and_input_and_exposes_exact_imports() {
    let mut frame = {
        let package = prepared(2);
        let input = vec![7];
        package
            .start_service_frame(&input, Cancellation::default())
            .unwrap()
    };
    let mut kinds = Vec::new();
    while let Some(call) = frame.pending() {
        let token = call.token.clone();
        kinds.push(call.kind);
        assert_eq!(call.bytes, [7]);
        let reply = match call.kind {
            Kind::Core => Err(()),
            Kind::Io => Ok(vec![9]),
            Kind::Dependency => panic!("not an admitted IO import"),
        };
        frame.resume(&token, reply).unwrap();
    }
    let result = frame.finish();
    assert_eq!(kinds, [Kind::Core, Kind::Core, Kind::Io]);
    assert_eq!(result.report.outcome, Ok(0));
    assert_eq!(result.report.host_calls, 3);
    assert_eq!(result.completion, Some(vec![9]));
}

#[test]
fn earlier_denied_core_remains_protocol_fault_after_pending_io_cancellation() {
    let package = prepared(2);
    let cancel = Cancellation::default();
    let mut frame = package.start_io_frame(&[7], cancel.clone()).unwrap();
    let call = frame.pending().unwrap();
    assert_eq!(call.kind, Kind::Io);
    let token = call.token.clone();
    cancel.cancel();
    frame.resume(&token, Ok(vec![9])).unwrap();
    let result = frame.finish();
    assert_eq!(result.report.outcome, Err(Fault::TaskProtocol));
    assert_eq!(result.report.host_calls, 3);
    assert!(result.completion.is_none());
}

#[test]
fn ordinary_package_cannot_enter_a_service_frame_or_run_its_guest() {
    let module = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "morrow_run") (result i32) unreachable))"#,
    )
    .unwrap();
    let manifest = Package::manifest_for_task("plain.frame", "1.0.0", &module, vec![]);
    let package = PreparedPackage::new(
        Package::build(manifest, &module).unwrap(),
        Limits::default(),
    )
    .unwrap();
    let result = match package.start_service_frame(&[7], Cancellation::default()) {
        Ok(_) => panic!("ordinary package admitted to IO frame"),
        Err(run) => run,
    };
    assert_eq!(result.report.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(result.report.host_calls, 0);
    assert_eq!(result.report.fuel_remaining, package.limits().fuel);
    assert!(result.completion.is_none());
}

#[test]
fn denied_second_core_not_misrouted_to_io() {
    let p = prepared(2);
    let mut callback_calls = 0;
    let result = p.run_io_frame(
        &[7],
        &mut |bytes| {
            assert_eq!(bytes, &[7]);
            callback_calls += 1;
            Ok(vec![9])
        },
        Cancellation::default(),
    );
    assert_eq!(callback_calls, 1);
    assert_eq!(result.report.host_calls, 3);
    assert_eq!(result.report.outcome, Err(Fault::TaskProtocol));
    assert!(result.completion.is_none());
}
