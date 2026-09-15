#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{
        Package,
        io::{self, IoCapability},
    },
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{Cancellation, Fault, Limits, package::PreparedPackage};
#[test]
fn ordinary_run_and_task_never_execute_an_io_declared_module() {
    let wasm = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "morrow_run") (result i32) unreachable))"#,
    )
    .unwrap();
    let mut manifest = Package::manifest_for_task("io.gate", "1.0.0", &wasm, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    manifest.io_declaration = Some(io::declaration(
        vec![IoCapability::HttpRequest],
        vec!["api.invoke".into()],
    ));
    let p =
        PreparedPackage::new(Package::build(manifest, &wasm).unwrap(), Limits::default()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut host =
        HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
            .unwrap();
    let connection = p.connect(&mut host).unwrap();
    let run = p.run(
        &mut host,
        &connection,
        || panic!("must not dispatch"),
        Cancellation::default(),
    );
    assert_eq!(run.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(run.host_calls, 0);
    assert_eq!(run.fuel_remaining, p.limits().fuel);
    let invocation = Invocation::new_transform(
        "no-effect",
        Transform {
            handler: "api.invoke".into(),
            input_type: "in".into(),
            output_type: "out".into(),
            input: vec![],
        },
    )
    .unwrap();
    let run = p.run_task(
        &mut host,
        &connection,
        &invocation,
        || panic!("must not dispatch"),
        Cancellation::default(),
    );
    assert_eq!(run.execution.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(run.execution.host_calls, 0);
    assert_eq!(run.execution.fuel_remaining, p.limits().fuel);
    assert!(run.output.is_none() && run.response.is_none() && run.failure.is_none());
    host.disconnect(&connection).unwrap();
}
