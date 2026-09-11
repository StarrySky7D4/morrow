#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, proto::Capability},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{Cancellation, Fault, Limits, package::PreparedPackage};
fn package(wat: &str, version: &str) -> Package {
    let bytes = wat::parse_str(wat).unwrap();
    Package::build(
        Package::manifest_for("example", version, &bytes, vec![Capability::RenameCard]),
        &bytes,
    )
    .unwrap()
}
const SIMPLE: &str = r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 7))"#;
fn host(dir: &tempfile::TempDir) -> HostRuntime {
    HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap()).unwrap()
}
#[test]
fn changed_archive_or_unbound_connection_cannot_execute() {
    let a = PreparedPackage::new(package(SIMPLE, "1.0.0"), Limits::default()).unwrap();
    let b = PreparedPackage::new(package(SIMPLE, "1.0.1"), Limits::default()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut h = host(&dir);
    let ca = a.connect(&mut h).unwrap();
    let raw = h.connect().unwrap();
    for c in [&ca, &raw] {
        let r = b.run(
            &mut h,
            c,
            || panic!("must not call host"),
            Cancellation::default(),
        );
        assert_eq!(r.outcome, Err(Fault::PackageBinding));
        assert_eq!(r.host_calls, 0);
    }
    assert_eq!(
        a.run(&mut h, &ca, || 0, Cancellation::default()).outcome,
        Ok(7)
    );
}
#[test]
fn limits_are_intersected_and_initial_memory_obeys_host_ceiling() {
    let mut p = package(SIMPLE, "1.0.0");
    let mut m = p.manifest().clone();
    let b = m.budget.as_mut().unwrap();
    b.fuel = 1000;
    b.memory_bytes = 131072;
    b.host_calls = 3;
    p = Package::build(m, p.module()).unwrap();
    let prepared = PreparedPackage::new(
        p,
        Limits {
            fuel: 2000,
            memory_bytes: 65536,
            host_calls: 1,
        },
    )
    .unwrap();
    assert_eq!(prepared.limits().fuel, 1000);
    assert_eq!(prepared.limits().memory_bytes, 65536);
    assert_eq!(prepared.limits().host_calls, 1);
    let p=PreparedPackage::new(package(r#"(module (memory (export "memory") 2) (func (export "morrow_run") (result i32) i32.const 7))"#,"1.0.0"),Limits{memory_bytes:65536,..Limits::default()}).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut h = host(&dir);
    let c = p.connect(&mut h).unwrap();
    let r = p.run(&mut h, &c, || 0, Cancellation::default());
    assert_eq!(r.outcome, Err(Fault::Limits));
    assert_eq!(r.host_calls, 0);
    assert!(matches!(
        PreparedPackage::new(
            package(SIMPLE, "1.0.0"),
            Limits {
                fuel: 0,
                ..Limits::default()
            }
        ),
        Err(Fault::Limits)
    ));
}
#[test]
fn declared_fuel_and_call_budgets_stop_execution() {
    for code in [
        r#"(module (memory (export "memory") 2) (func (export "morrow_run") (result i32) (loop $l br $l) i32.const 0))"#,
        r#"(module (import "morrow_v1" "exchange" (func $e (param i32 i32 i32 i32) (result i32))) (memory (export "memory") 2) (func (export "morrow_run") (result i32) i32.const 1 i32.const 1 i32.const 65536 i32.const 65536 call $e))"#,
    ] {
        let p = package(code, "1.0.0");
        let mut m = p.manifest().clone();
        let b = m.budget.as_mut().unwrap();
        b.fuel = 100;
        b.host_calls = 0;
        let p = PreparedPackage::new(Package::build(m, p.module()).unwrap(), Limits::default())
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut h = host(&dir);
        let c = p.connect(&mut h).unwrap();
        let r = p.run(
            &mut h,
            &c,
            || panic!("must not call host"),
            Cancellation::default(),
        );
        assert_eq!(r.outcome, Err(Fault::Limits));
        assert_eq!(r.host_calls, 0);
    }
}
#[test]
fn preparation_rejects_executable_start_and_noncontract_imports() {
    for code in [
        r#"(module (memory (export "memory") 1) (func $s) (start $s) (func (export "morrow_run") (result i32) i32.const 0))"#,
        r#"(module (import "wasi_snapshot_preview1" "fd_write" (func)) (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#,
    ] {
        assert!(matches!(
            PreparedPackage::new(package(code, "1.0.0"), Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
    // A valid container does not imply a runnable module.
    let bytes = b"\0asm\x01\0\0\0";
    let p = Package::build(
        Package::manifest_for("empty", "1.0.0", bytes, vec![]),
        bytes,
    )
    .unwrap();
    assert!(matches!(
        PreparedPackage::new(p, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
}
