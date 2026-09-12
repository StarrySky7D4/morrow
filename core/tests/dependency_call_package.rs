use morrow_core::plugin_package::{
    DEPENDENCIES_FEATURE, DEPENDENCY_CALLS_FEATURE, Package, proto::TransformHandler,
};
fn manifest() -> morrow_core::plugin_package::proto::Manifest {
    let mut m = Package::manifest_for_transform(
        "test.dynamic",
        "1.0.0",
        b"\0asm\x01\0\0\0",
        vec![TransformHandler {
            handler: "transform".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    m.required_features
        .extend([DEPENDENCIES_FEATURE.into(), DEPENDENCY_CALLS_FEATURE.into()]);
    m.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
    m
}
#[test]
fn dynamic_feature_requires_exact_schema_and_both_task_and_transform_contracts() {
    let m = manifest();
    assert!(Package::build(m.clone(), b"\0asm\x01\0\0\0").is_ok());
    for mode in 0..6 {
        let mut bad = m.clone();
        match mode {
            0 => bad.dependency_schema_sha256.clear(),
            1 => bad.dependency_schema_sha256[0] ^= 1,
            2 => bad.required_features.retain(|f| f != DEPENDENCIES_FEATURE),
            3 => bad
                .required_features
                .retain(|f| f != "transform-handlers-v1"),
            4 => bad
                .required_features
                .retain(|f| f != DEPENDENCY_CALLS_FEATURE),
            _ => {
                bad.guest_abi_version = 1;
                bad.task_schema_sha256.clear();
            }
        }
        assert!(
            Package::build(bad, b"\0asm\x01\0\0\0").is_err(),
            "mode {mode}"
        );
    }
}
#[test]
fn old_manifests_keep_no_dynamic_authority_and_unknown_features_reject() {
    let plain = Package::manifest_for_task("old", "1.0.0", b"\0asm\x01\0\0\0", vec![]);
    let package = Package::build(plain, b"\0asm\x01\0\0\0").unwrap();
    assert!(package.manifest().dependency_schema_sha256.is_empty());
    let mut m = manifest();
    m.required_features.push("dependency-calls-v2".into());
    assert!(Package::build(m, b"\0asm\x01\0\0\0").is_err());
}
