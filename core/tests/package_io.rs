use morrow_core::{
    Error,
    plugin_package::{
        Package,
        io::{self, IoCapability},
        proto,
    },
};
use prost::Message;
const MODULE: &[u8] = b"\0asm\x01\0\0\0";
fn manifest() -> proto::Manifest {
    let mut m = Package::manifest_for_task("io.example", "1.0.0", MODULE, vec![]);
    m.required_features.push(io::FEATURE.into());
    m.io_declaration = Some(io::declaration(
        vec![IoCapability::HttpRequest, IoCapability::HttpPublish],
        vec!["api.invoke".into()],
    ));
    m
}
fn reject(edit: impl FnOnce(&mut proto::Manifest)) {
    let mut m = manifest();
    edit(&mut m);
    assert!(Package::build(m, MODULE).is_err());
}
#[test]
fn declaration_roundtrips_without_content_grants_or_byte_rewriting() {
    let m = manifest();
    let bytes = m.encode_to_vec();
    let p = Package::build(m.clone(), MODULE).unwrap();
    let q = Package::decode(p.archive()).unwrap();
    assert_eq!(q.io_declaration(), m.io_declaration.as_ref());
    assert_eq!(
        q.io_capabilities(),
        &std::collections::BTreeSet::from([IoCapability::HttpRequest, IoCapability::HttpPublish])
    );
    assert!(q.capabilities().is_empty());
    assert_eq!(q.manifest_bytes(), bytes);
    assert_eq!(q.archive(), p.archive());
    let mut changed = m;
    changed.io_declaration.as_mut().unwrap().handlers[0] = "other".into();
    assert_ne!(
        p.digest(),
        Package::build(changed, MODULE).unwrap().digest()
    );
}
#[test]
fn legacy_abi_one_and_two_have_no_io_declaration_or_approval() {
    for m in [
        Package::manifest_for("old", "1.0.0", MODULE, vec![]),
        Package::manifest_for_task("old", "1.0.0", MODULE, vec![]),
    ] {
        let original = m.encode_to_vec();
        let p = Package::from_parts(&original, MODULE).unwrap();
        assert!(p.io_declaration().is_none());
        assert!(p.io_capabilities().is_empty());
        assert_eq!(p.manifest_bytes(), original);
    }
}
#[test]
fn declaration_feature_abi_and_both_versions_must_match() {
    reject(|m| m.required_features.clear());
    reject(|m| m.io_declaration = None);
    reject(|m| m.required_features.push(io::FEATURE.into()));
    reject(|m| {
        m.guest_abi_version = 1;
        m.task_schema_sha256.clear();
    });
    reject(|m| m.io_declaration.as_mut().unwrap().schema_version = 2);
    reject(|m| m.io_declaration.as_mut().unwrap().io_version = 2);
    reject(|m| m.io_declaration.as_mut().unwrap().io_schema_sha256[0] ^= 1);
    reject(|m| m.io_declaration.as_mut().unwrap().io_schema_sha256.clear());
}
#[test]
fn unknown_duplicate_empty_or_excessive_capabilities_reject() {
    for values in [vec![], vec![0], vec![11], vec![-1], vec![1, 1], vec![1; 11]] {
        reject(|m| m.io_declaration.as_mut().unwrap().requested_capabilities = values);
    }
    for n in 1..=10 {
        assert_eq!(IoCapability::from_number(n).unwrap().number(), n);
    }
    assert_eq!(
        IoCapability::from_number(99),
        Err(Error::UnsupportedVersion)
    );
}
#[test]
fn handler_identity_and_pure_namespace_are_checked() {
    for handlers in [
        vec![],
        vec!["".into()],
        vec!["bad/path".into()],
        vec!["bad\n".into()],
        vec!["a".into(), "a".into()],
        (0..17).map(|i| format!("handler{i}")).collect(),
    ] {
        reject(|m| m.io_declaration.as_mut().unwrap().handlers = handlers);
    }
    reject(|m| {
        m.required_features.push("transform-handlers-v1".into());
        m.transform_handlers.push(proto::TransformHandler {
            handler: "api.invoke".into(),
            input_type: "input".into(),
            output_type: "output".into(),
            max_input_bytes: 1,
            max_output_bytes: 1,
        });
    });
}
#[test]
fn budgets_are_required_and_bounded_before_any_execution() {
    reject(|m| m.io_declaration.as_mut().unwrap().budget = None);
    for case in 0..11 {
        reject(|m| {
            let b = m.io_declaration.as_mut().unwrap().budget.as_mut().unwrap();
            match case {
                0 => b.max_resources = 0,
                1 => b.max_resources = io::MAX_RESOURCES + 1,
                2 => b.max_jobs = 0,
                3 => b.max_jobs = io::MAX_JOBS + 1,
                4 => b.max_bytes = 0,
                5 => b.max_bytes = io::MAX_BYTES + 1,
                6 => b.max_job_bytes = 0,
                7 => b.max_job_bytes = io::MAX_JOB_BYTES + 1,
                8 => b.max_bytes = b.max_job_bytes - 1,
                9 => b.max_duration_ms = 0,
                _ => b.max_duration_ms = io::MAX_DURATION_MS + 1,
            }
        });
    }
}
#[test]
fn io_package_cannot_enter_pure_handler_or_evidence_validation() {
    let mut m = manifest();
    m.required_features.push("transform-handlers-v1".into());
    m.transform_handlers.push(proto::TransformHandler {
        handler: "pure".into(),
        input_type: "in".into(),
        output_type: "out".into(),
        max_input_bytes: 1,
        max_output_bytes: 1,
    });
    let p = Package::build(m, MODULE).unwrap();
    assert!(
        p.transform_handler(&morrow_core::task::Transform {
            handler: "pure".into(),
            input_type: "in".into(),
            output_type: "out".into(),
            input: vec![],
        })
        .is_err()
    );
}
#[test]
fn truncated_nested_declaration_rejects_before_protobuf_allocation() {
    let m = manifest().encode_to_vec();
    for size in [m.len() - 1, m.len() - 2, m.len() - 5] {
        assert!(Package::from_parts(&m[..size], MODULE).is_err());
    }
    // field 18 has the wrong wire type (varint instead of a nested message).
    let mut legacy = Package::manifest_for_task("old", "1.0.0", MODULE, vec![]).encode_to_vec();
    legacy.extend_from_slice(&[0x90, 0x01, 0x01]);
    assert!(Package::from_parts(&legacy, MODULE).is_err());
}

#[test]
fn duplicate_declaration_and_unknown_nested_privilege_fields_reject() {
    fn append_declaration(bytes: &mut Vec<u8>, declaration: &[u8]) {
        bytes.extend_from_slice(&[0x92, 0x01]);
        prost::encoding::encode_varint(declaration.len() as u64, bytes);
        bytes.extend_from_slice(declaration);
    }
    let m = manifest();
    let original = m.io_declaration.as_ref().unwrap().encode_to_vec();
    let mut duplicate = m.encode_to_vec();
    append_declaration(&mut duplicate, &original);
    assert!(Package::from_parts(&duplicate, MODULE).is_err());
    let mut base = m.clone();
    base.io_declaration = None;
    let mut nested_unknown = original.clone();
    nested_unknown.extend_from_slice(&[0xa0, 0x06, 0x01]);
    let mut bytes = base.encode_to_vec();
    append_declaration(&mut bytes, &nested_unknown);
    assert!(Package::from_parts(&bytes, MODULE).is_err());
    let mut repeated_version = original;
    repeated_version.extend_from_slice(&[0x08, 0x01]);
    let mut bytes = base.encode_to_vec();
    append_declaration(&mut bytes, &repeated_version);
    assert!(Package::from_parts(&bytes, MODULE).is_err());
    // Existing unknown optional manifest data still survives on an IO package.
    let mut outer_unknown = m.encode_to_vec();
    outer_unknown.extend_from_slice(&[0xa0, 0x06, 0x01]);
    assert_eq!(
        Package::from_parts(&outer_unknown, MODULE)
            .unwrap()
            .manifest_bytes(),
        outer_unknown
    );
}

#[test]
fn actual_pure_evidence_plan_rejects_io_even_for_a_disjoint_handler() {
    use morrow_core::{
        task::{Invocation, Transform},
        task_evidence,
    };
    let mut m = manifest();
    m.required_features.push("transform-handlers-v1".into());
    m.transform_handlers.push(proto::TransformHandler {
        handler: "pure".into(),
        input_type: "in".into(),
        output_type: "out".into(),
        max_input_bytes: 1,
        max_output_bytes: 1,
    });
    let p = Package::build(m, MODULE).unwrap();
    let input = Invocation::new_transform(
        "evidence-probe",
        Transform {
            handler: "pure".into(),
            input_type: "in".into(),
            output_type: "out".into(),
            input: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        task_evidence::validate_plan(
            &p,
            &input,
            &task_evidence::proto::ExecutionBudget {
                fuel: 20_000_000,
                memory_bytes: 16 * 1024 * 1024,
                host_calls: 16,
            }
        ),
        Err(Error::Invalid("IO package requires IO execution"))
    );
}
