use morrow_core::plugin_package::{DEPENDENCIES_FEATURE, MAX_DEPENDENCIES, Package, proto};
use prost::Message;

const MODULE: &[u8] = b"\0asm\x01\0\0\0";
fn requirement() -> proto::DependencyRequirement {
    proto::DependencyRequirement {
        slot: "formatter".into(),
        handler: "text.format".into(),
        input_type: "text.raw.v1".into(),
        output_type: "text.formatted.v1".into(),
        provider_version: "^1.2.0".into(),
        optional: false,
    }
}
fn manifest() -> proto::Manifest {
    let mut m = Package::manifest_for_task("consumer", "1.0.0", MODULE, vec![]);
    m.required_features.push(DEPENDENCIES_FEATURE.into());
    m.dependencies.push(requirement());
    m
}
fn handler() -> proto::TransformHandler {
    let d = requirement();
    proto::TransformHandler {
        handler: d.handler,
        input_type: d.input_type,
        output_type: d.output_type,
        max_input_bytes: 0,
        max_output_bytes: 0,
    }
}
fn provider(version: &str, h: proto::TransformHandler) -> Package {
    Package::build(
        Package::manifest_for_transform("provider", version, MODULE, vec![h]),
        MODULE,
    )
    .unwrap()
}

#[test]
fn declarations_roundtrip_and_change_package_identity_without_granting_capabilities() {
    let m = manifest();
    let p = Package::build(m.clone(), MODULE).unwrap();
    let decoded = Package::decode(p.archive()).unwrap();
    assert_eq!(decoded.manifest().dependencies, m.dependencies);
    assert_eq!(decoded.manifest_bytes(), m.encode_to_vec());
    assert!(p.capabilities().is_empty());
    assert_eq!(p.dependency("formatter").unwrap(), &requirement());
    assert!(p.dependency("absent").is_err());
    let mut changed = m;
    changed.dependencies[0].optional = true;
    assert_ne!(
        p.digest(),
        Package::build(changed, MODULE).unwrap().digest()
    );
}

#[test]
fn legacy_manifest_defaults_remain_dependency_free() {
    for m in [
        Package::manifest_for("legacy", "1.0.0", MODULE, vec![]),
        Package::manifest_for_task("task", "1.0.0", MODULE, vec![]),
    ] {
        assert!(m.dependencies.is_empty());
        let p = Package::build(m, MODULE).unwrap();
        assert_eq!(Package::decode(p.archive()).unwrap().digest(), p.digest());
        assert!(
            p.check_dependency("formatter", &provider("1.2.0", handler()))
                .is_err()
        );
    }
}

#[test]
fn dependencies_require_known_feature_and_task_abi_even_when_optional() {
    for case in 0..7 {
        let mut m = manifest();
        match case {
            0 => m.required_features.clear(),
            1 => m.required_features.push("unknown-mandatory-v1".into()),
            2 => m.required_features.push(DEPENDENCIES_FEATURE.into()),
            3 => {
                m.guest_abi_version = 1;
                m.task_schema_sha256.clear();
            }
            4 => {
                m.dependencies[0].optional = true;
                m.required_features.clear();
            }
            5 => {
                m.dependencies.clear();
                m.guest_abi_version = 1;
                m.task_schema_sha256.clear();
            }
            _ => m.guest_abi_version = 3,
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
    let mut empty = manifest();
    empty.dependencies.clear();
    assert!(Package::build(empty, MODULE).is_ok());
}

#[test]
fn duplicate_slots_and_invalid_dependency_identities_are_rejected() {
    let mut duplicate = manifest();
    duplicate.dependencies.push(requirement());
    assert!(Package::build(duplicate, MODULE).is_err());
    for field in 0..4 {
        for invalid in [
            "".to_string(),
            "../escape".into(),
            "bad:identity".into(),
            "bad\0id".into(),
            "x".repeat(257),
        ] {
            let mut m = manifest();
            let d = &mut m.dependencies[0];
            *match field {
                0 => &mut d.slot,
                1 => &mut d.handler,
                2 => &mut d.input_type,
                _ => &mut d.output_type,
            } = invalid;
            assert!(Package::build(m, MODULE).is_err(), "field {field}");
        }
    }
}

#[test]
fn malformed_empty_and_oversized_version_requirements_are_rejected() {
    for value in [
        "".to_string(),
        " ".into(),
        "not a version".into(),
        ">=1.0.0 || <2.0.0".into(),
        ">=1.0.0\n".into(),
        "*".repeat(129),
    ] {
        let mut m = manifest();
        m.dependencies[0].provider_version = value;
        assert!(Package::build(m, MODULE).is_err());
    }
}

#[test]
fn bounded_dependency_and_handler_lists_can_coexist_at_their_maxima() {
    let mut m = manifest();
    m.dependencies = (0..MAX_DEPENDENCIES)
        .map(|i| {
            let mut d = requirement();
            d.slot = format!("slot{i}");
            d.optional = true;
            d
        })
        .collect();
    m.required_features.push("transform-handlers-v1".into());
    m.transform_handlers = (0..16)
        .map(|i| {
            let mut h = handler();
            h.handler = format!("handler{i}");
            h.max_input_bytes = 65536;
            h.max_output_bytes = 65536;
            h
        })
        .collect();
    assert!(Package::build(m.clone(), MODULE).is_ok());
    let mut extra = requirement();
    extra.slot = "overflow".into();
    m.dependencies.push(extra);
    assert!(Package::build(m, MODULE).is_err());
}

#[test]
fn provider_package_semver_range_and_prerelease_rules_are_enforced() {
    let p = Package::build(manifest(), MODULE).unwrap();
    for version in ["1.2.0", "1.9.9", "1.2.0+build.9"] {
        assert!(
            p.check_dependency("formatter", &provider(version, handler()))
                .is_ok()
        );
    }
    for version in ["1.1.9", "2.0.0", "1.3.0-test.1"] {
        assert!(
            p.check_dependency("formatter", &provider(version, handler()))
                .is_err()
        );
    }
    let mut m = manifest();
    m.dependencies[0].provider_version = ">=1.3.0-test.1, <1.3.0".into();
    let prerelease = Package::build(m, MODULE).unwrap();
    assert!(
        prerelease
            .check_dependency("formatter", &provider("1.3.0-test.2", handler()))
            .is_ok()
    );
    assert!(
        prerelease
            .check_dependency("formatter", &provider("1.3.0", handler()))
            .is_err()
    );
}

#[test]
fn provider_must_register_the_exact_handler_and_input_output_types() {
    let p = Package::build(manifest(), MODULE).unwrap();
    for field in 0..3 {
        let mut h = handler();
        *match field {
            0 => &mut h.handler,
            1 => &mut h.input_type,
            _ => &mut h.output_type,
        } = "other".into();
        assert!(
            p.check_dependency("formatter", &provider("1.2.0", h))
                .is_err()
        );
    }
    for m in [
        Package::manifest_for("old", "1.2.0", MODULE, vec![]),
        Package::manifest_for_task("no-handler", "1.2.0", MODULE, vec![]),
    ] {
        assert!(
            p.check_dependency("formatter", &Package::build(m, MODULE).unwrap())
                .is_err()
        );
    }
    // The check uses empty input: a declaration with a zero-byte budget is compatible.
    assert!(
        p.check_dependency("formatter", &provider("1.2.0", handler()))
            .is_ok()
    );
}

#[test]
fn optional_does_not_bypass_provider_compatibility_or_imply_binding() {
    let mut m = manifest();
    m.dependencies[0].optional = true;
    let p = Package::build(m, MODULE).unwrap();
    assert!(p.dependency("formatter").unwrap().optional);
    assert!(
        p.check_dependency("formatter", &provider("1.2.0", handler()))
            .is_ok()
    );
    assert!(
        p.check_dependency("formatter", &provider("2.0.0", handler()))
            .is_err()
    );
    let mut h = handler();
    h.output_type = "other".into();
    assert!(
        p.check_dependency("formatter", &provider("1.2.0", h))
            .is_err()
    );
}

#[test]
fn appended_dependency_wire_fields_cannot_bypass_feature_gating() {
    let m = Package::manifest_for_task("raw", "1.0.0", MODULE, vec![]);
    let mut bytes = m.encode_to_vec();
    // Field 16, length-delimited, known to the fixed generated descriptor.
    bytes.extend_from_slice(&[0x82, 0x01]);
    let d = requirement().encode_to_vec();
    assert!(d.len() < 128);
    bytes.push(d.len() as u8);
    bytes.extend_from_slice(&d);
    assert!(Package::from_parts(&bytes, MODULE).is_err());
    bytes.pop();
    assert!(Package::from_parts(&bytes, MODULE).is_err());
}
