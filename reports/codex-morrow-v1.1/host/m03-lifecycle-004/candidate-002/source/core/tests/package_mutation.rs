use morrow_core::plugin_package::{
    MAX_MUTATION_BYTES, MAX_MUTATION_JOB_BYTES, MUTATION_BUDGET_FEATURE, MUTATION_FEATURE, Package,
    io::{self, IoCapability},
    proto,
};
use prost::Message;

const MODULE: &[u8] = b"\0asm\x01\0\0\0";

fn manifest(capabilities: Vec<IoCapability>) -> morrow_core::plugin_package::proto::Manifest {
    let mut m = Package::manifest_for_task("mutation.example", "1.0.0", MODULE, vec![]);
    m.required_features = vec![io::FEATURE.into(), MUTATION_FEATURE.into()];
    m.io_declaration = Some(io::declaration(capabilities, vec!["mutation.run".into()]));
    m.mutation_schema_sha256 = morrow_core::mutation::schema_digest().to_vec();
    m
}

#[test]
fn exact_mutation_feature_and_digest_round_trip_without_grant() {
    for capabilities in [
        vec![IoCapability::FileCreate],
        vec![IoCapability::FileDelete],
        vec![IoCapability::FileCreate, IoCapability::FileDelete],
    ] {
        let m = manifest(capabilities);
        let original = m.encode_to_vec();
        let package = Package::build(m.clone(), MODULE).unwrap();
        assert!(package.mutation_enabled());
        assert_eq!(package.manifest_bytes(), original);
        assert!(
            Package::decode(package.archive())
                .unwrap()
                .mutation_enabled()
        );
        assert!(package.capabilities().is_empty());
        assert_eq!(package.io_declaration(), m.io_declaration.as_ref());
    }
    let plain = Package::manifest_for_task("plain", "1.0.0", MODULE, vec![]);
    assert!(!Package::build(plain, MODULE).unwrap().mutation_enabled());
}

#[test]
fn mutation_declaration_fails_closed_on_missing_or_mismatched_requirements() {
    let base = manifest(vec![IoCapability::FileCreate]);
    for case in 0..10 {
        let mut m = base.clone();
        match case {
            0 => m.required_features.retain(|f| f != MUTATION_FEATURE),
            1 => m.mutation_schema_sha256.clear(),
            2 => m.mutation_schema_sha256[0] ^= 1,
            3 => {
                m.guest_abi_version = 1;
                m.task_schema_sha256.clear();
            }
            4 => m.required_features.retain(|f| f != io::FEATURE),
            5 => m.io_declaration = None,
            6 => {
                m.io_declaration.as_mut().unwrap().requested_capabilities =
                    vec![IoCapability::FileRead.number()]
            }
            7 => m.required_features.push(MUTATION_FEATURE.into()),
            8 => m.required_features.push("unknown-mutation-profile".into()),
            _ => {
                m.required_features.push("dependency-calls-v1".into());
                m.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
            }
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
}

fn budgeted_manifest() -> proto::Manifest {
    let mut m = manifest(vec![IoCapability::FileCreate, IoCapability::FileDelete]);
    m.required_features.push(MUTATION_BUDGET_FEATURE.into());
    m.mutation_budget = Some(proto::MutationBudget {
        max_job_bytes: MAX_MUTATION_JOB_BYTES,
        max_bytes: MAX_MUTATION_BYTES,
    });
    m
}

#[test]
fn explicit_mutation_budget_round_trips_without_changing_legacy_io_budget() {
    let m = budgeted_manifest();
    let old = m.io_declaration.as_ref().unwrap().budget.as_ref().unwrap();
    assert_eq!(old.max_job_bytes, io::MAX_JOB_BYTES);
    assert_eq!(old.max_bytes, io::MAX_BYTES);
    let original = m.encode_to_vec();
    let p = Package::build(m.clone(), MODULE).unwrap();
    assert_eq!(p.manifest_bytes(), original);
    assert_eq!(p.mutation_budget(), m.mutation_budget.as_ref());
    assert_eq!(
        Package::decode(p.archive()).unwrap().mutation_budget(),
        m.mutation_budget.as_ref()
    );
}

#[test]
fn mutation_budget_requires_exact_feature_scope_and_bounded_values() {
    let base = budgeted_manifest();
    for case in 0..11 {
        let mut m = base.clone();
        match case {
            0 => m.required_features.retain(|f| f != MUTATION_BUDGET_FEATURE),
            1 => m.mutation_budget = None,
            2 => m.required_features.retain(|f| f != MUTATION_FEATURE),
            3 => m.mutation_budget.as_mut().unwrap().max_job_bytes = 0,
            4 => m.mutation_budget.as_mut().unwrap().max_job_bytes = MAX_MUTATION_JOB_BYTES + 1,
            5 => m.mutation_budget.as_mut().unwrap().max_bytes = MAX_MUTATION_BYTES + 1,
            6 => m.mutation_budget.as_mut().unwrap().max_bytes = 1,
            7 => m
                .io_declaration
                .as_mut()
                .unwrap()
                .requested_capabilities
                .push(IoCapability::HttpRequest.number()),
            8 => {
                m.io_declaration.as_mut().unwrap().requested_capabilities =
                    vec![IoCapability::FileRead.number()]
            }
            9 => {
                m.required_features.push(io::SERVICE_RUN_FEATURE.into());
                let d = m.io_declaration.as_mut().unwrap();
                d.service_run = Some(io::proto::ServiceRunProfile {
                    schema_version: io::SERVICE_RUN_VERSION,
                    max_duration_ms: 100,
                    budget: None,
                });
            }
            _ => {
                m.required_features.push("dependencies-v1".into());
            }
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
}

#[test]
fn duplicate_or_noncanonical_mutation_budget_wire_rejects() {
    fn append_budget(manifest: &mut Vec<u8>, nested: &[u8]) {
        manifest.extend_from_slice(&[0xa2, 0x01]);
        prost::encoding::encode_varint(nested.len() as u64, manifest);
        manifest.extend_from_slice(nested);
    }
    let m = budgeted_manifest();
    let nested = m.mutation_budget.as_ref().unwrap().encode_to_vec();
    let mut duplicate = m.encode_to_vec();
    append_budget(&mut duplicate, &nested);
    assert!(Package::from_parts(&duplicate, MODULE).is_err());
    let mut base = m.clone();
    base.mutation_budget = None;
    let mut unknown = nested.clone();
    unknown.extend_from_slice(&[0x18, 0x01]);
    let mut bytes = base.encode_to_vec();
    append_budget(&mut bytes, &unknown);
    assert!(Package::from_parts(&bytes, MODULE).is_err());
    let mut repeated = nested;
    repeated.extend_from_slice(&[0x08, 0x01]);
    let mut bytes = base.encode_to_vec();
    append_budget(&mut bytes, &repeated);
    assert!(Package::from_parts(&bytes, MODULE).is_err());
}
