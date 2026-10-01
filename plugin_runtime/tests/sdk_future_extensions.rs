//! Compatibility fixtures for future mandatory profiles. These do not implement
//! streaming, Cloud sync or execution; they test the actual current boundaries.
#![cfg(feature = "packages")]
use morrow_core::plugin_package::Package;
use morrow_plugin_runtime::{Cancellation, Fault, Limits, Runner};
use prost::Message;

fn plain() -> Vec<u8> {
    wat::parse_str(r#"(module (memory (export "memory") 4) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap()
}

#[test]
fn future_required_profiles_and_changed_contracts_are_not_silently_ignored() {
    let module = plain();
    let original =
        Package::manifest_for_task("org.example.extension-fixture", "1.0.0", &module, vec![]);
    assert!(Package::build(original.clone(), &module).is_ok());
    for feature in [
        "stream-v1",
        "event-subscription-v1",
        "cloud-changes-v1",
        "native-exec-v1",
    ] {
        let mut future = original.clone();
        future.required_features.push(feature.into());
        assert!(
            matches!(
                Package::build(future, &module),
                Err(morrow_core::Error::UnsupportedVersion)
            ),
            "{feature}"
        );
    }
    let mut changed = original;
    changed.task_schema_sha256[0] ^= 1;
    assert!(matches!(
        Package::build(changed, &module),
        Err(morrow_core::Error::UnsupportedVersion)
    ));
}

#[test]
fn unknown_optional_metadata_preserves_original_bytes_and_grants_no_dependency() {
    let module = plain();
    let manifest =
        Package::manifest_for_task("org.example.optional-fixture", "1.0.0", &module, vec![]);
    let mut bytes = manifest.encode_to_vec();
    bytes.extend_from_slice(&[0xa0, 0x06, 0x07]); // optional protobuf field 100
    let package = Package::from_parts(&bytes, &module).unwrap();
    assert_eq!(package.manifest_bytes(), bytes);
    assert!(package.manifest().required_features.is_empty());
    assert!(package.manifest().dependency_schema_sha256.is_empty());
    assert!(package.capabilities().is_empty());
    let reopened = Package::decode(package.archive()).unwrap();
    assert_eq!(reopened.archive(), package.archive());
    assert_eq!(reopened.manifest_bytes(), bytes);
}

#[test]
fn additional_imports_require_an_explicit_runner_and_bound_callback() {
    for name in ["morrow_stream_v1", "morrow_events_v1", "morrow_exec_v1"] {
        let module = wat::parse_str(format!(r#"(module (import "{name}" "call" (func (param i32 i32 i32 i32) (result i32))) (memory (export "memory") 4) (func (export "morrow_run") (result i32) i32.const 0))"#)).unwrap();
        assert!(matches!(
            Runner::new_task(&module, Limits::default()),
            Err(Fault::UnsupportedAbi)
        ));
    }
    // An already implemented extension supplies the concrete side-by-side seam:
    // the ordinary task route still rejects it; the explicit route admits it.
    let module = wat::parse_str(r#"(module (import "morrow_dependency_v1" "call" (func (param i32 i32 i32 i32) (result i32))) (memory (export "memory") 4) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
    assert!(matches!(
        Runner::new_task(&module, Limits::default()),
        Err(Fault::UnsupportedAbi)
    ));
    let runner = Runner::new_dependency_task(&module, Limits::default()).unwrap();
    let result = runner.run_task(
        &[1],
        &mut |_| panic!("unbound callbacks must not run"),
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Err(Fault::UnsupportedAbi));
    assert_eq!(result.report.host_calls, 0);
    assert!(result.completion.is_none());
}
