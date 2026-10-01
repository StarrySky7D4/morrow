//! Independent runtime compatibility boundaries. No network or service adapters.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    channel,
    plugin_package::{Package, proto::TransformHandler},
};
use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner, package::PreparedPackage};
use prost::Message;

fn module(import: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module {import}
      (memory (export "memory") 4)
      (func (export "morrow_run") (result i32) i32.const 0))"#
    ))
    .unwrap()
}
fn channel_module() -> Vec<u8> {
    module(r#"(import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i32)))"#)
}

#[test]
fn ordinary_task_and_legacy_runners_reject_the_new_channel_import() {
    let wasm = channel_module();
    assert!(matches!(
        Runner::new(&wasm, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    assert!(matches!(
        Runner::new_task(&wasm, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    assert!(matches!(
        Runner::new_dependency_task(&wasm, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    assert!(matches!(
        Runner::new_io_task(&wasm, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    let undeclared = Package::build(
        Package::manifest_for_task("org.example.channel.compat", "1.0.0", &wasm, vec![]),
        &wasm,
    )
    .unwrap();
    assert!(matches!(
        PreparedPackage::new(undeclared, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
}

#[test]
fn channel_runner_requires_the_exact_import_and_rejects_mixed_profiles() {
    assert!(matches!(
        Runner::new_channel_task(&module(""), Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    for import in [
        r#"(import "morrow_channel_v1" "call" (func (param i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i64)))"#,
        r#"(import "morrow_channel_v2" "call" (func (param i32 i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_channel_v1" "receive" (func (param i32 i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i32))) (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i32))) (import "morrow_io_v1" "call" (func (param i32 i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i32))) (import "morrow_dependency_v1" "call" (func (param i32 i32 i32 i32) (result i32)))"#,
    ] {
        assert!(
            matches!(
                Runner::new_channel_task(&module(import), Limits::default()),
                Err(Fault::UnsupportedAbi)
            ),
            "{import}"
        );
    }
}

#[test]
fn channel_runner_cannot_be_driven_through_unbound_generic_callbacks() {
    let runner = Runner::new_channel_task(&channel_module(), Limits::default()).unwrap();
    let report = runner.run_task(
        &[1],
        &mut |_| panic!("unbound callback ran"),
        Cancellation::default(),
    );
    assert_eq!(report.report.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(report.report.host_calls, 0);
    assert!(report.completion.is_none());
}

#[test]
fn old_task_manifest_and_optional_metadata_remain_channel_free() {
    let wasm = module("");
    let manifest = Package::manifest_for_task("org.example.channel.old", "1.0.0", &wasm, vec![]);
    assert_eq!(manifest.guest_abi_version, 2);
    assert_eq!(manifest.runtime_protocol_version, 7);
    assert_eq!(
        manifest.task_schema_sha256,
        morrow_core::task::schema_digest()
    );
    assert!(manifest.channel_declaration.is_none());
    let mut bytes = manifest.encode_to_vec();
    bytes.extend_from_slice(&[0xa0, 0x06, 0x07]); // unrelated optional protobuf field 100
    let old = Package::from_parts(&bytes, &wasm).unwrap();
    assert_eq!(old.manifest_bytes(), bytes);
    assert!(old.channel_declaration().is_none());
    assert!(old.manifest().required_features.is_empty());
    let reopened = Package::decode(old.archive()).unwrap();
    assert_eq!(reopened.manifest_bytes(), bytes);
    PreparedPackage::new(reopened, Limits::default()).unwrap();
}

#[test]
fn channel_feature_and_declaration_cannot_be_admitted_independently() {
    let wasm = channel_module();
    let original = Package::manifest_for_transform(
        "org.example.channel.required",
        "1.0.0",
        &wasm,
        vec![TransformHandler {
            handler: "channel.exercise".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 65,
            max_output_bytes: 64,
        }],
    );
    let mut required_only = original.clone();
    required_only
        .required_features
        .push(channel::FEATURE.into());
    assert!(Package::build(required_only, &wasm).is_err());
    let mut declaration_only = original.clone();
    declaration_only.channel_declaration = Some(channel::declaration(
        vec!["channel.exercise".into()],
        vec![channel::Kind::ByteStream],
    ));
    assert!(Package::build(declaration_only, &wasm).is_err());
    let mut declared = original;
    declared.required_features.push(channel::FEATURE.into());
    declared.channel_declaration = Some(channel::declaration(
        vec!["channel.exercise".into()],
        vec![channel::Kind::ByteStream],
    ));
    let admitted = Package::build(declared, &wasm).unwrap();
    let task = morrow_core::task::Transform {
        handler: "channel.exercise".into(),
        input_type: "bytes".into(),
        output_type: "bytes".into(),
        input: vec![0; 65],
    };
    assert!(admitted.channel_handler(&task).is_ok());
    assert!(
        admitted.transform_handler(&task).is_err(),
        "channel handlers must never enter the pure transform/cache route"
    );
    PreparedPackage::new(admitted, Limits::default()).unwrap();
}
