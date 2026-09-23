use morrow_core::{
    Error,
    plugin_package::{
        self, Package,
        io::{self, IoCapability},
        proto,
    },
    service,
};
use prost::Message;

const MODULE: &[u8] = b"\0asm\x01\0\0\0";

fn manifest() -> proto::Manifest {
    let mut manifest = Package::manifest_for_task("service.budget", "1.0.0", MODULE, vec![]);
    manifest.required_features = vec![
        io::FEATURE.into(),
        io::SERVICE_RUN_FEATURE.into(),
        io::SERVICE_RUN_BUDGET_FEATURE.into(),
    ];
    let mut declaration = io::declaration(
        vec![IoCapability::HttpListen, IoCapability::HttpPublish],
        vec!["api.invoke".into()],
    );
    declaration.service_schema_sha256 = service::schema_digest().to_vec();
    declaration.service_run = Some(io::proto::ServiceRunProfile {
        schema_version: io::SERVICE_RUN_VERSION,
        max_duration_ms: io::MAX_SERVICE_RUN_DURATION_MS,
        budget: Some(io::proto::ServiceRunBudget {
            schema_version: io::SERVICE_RUN_BUDGET_VERSION,
            max_jobs: io::MAX_SERVICE_RUN_JOBS,
            max_bytes: io::MAX_BYTES,
        }),
    });
    manifest.io_declaration = Some(declaration);
    manifest
}

fn run_budget(manifest: &mut proto::Manifest) -> &mut io::proto::ServiceRunBudget {
    manifest
        .io_declaration
        .as_mut()
        .unwrap()
        .service_run
        .as_mut()
        .unwrap()
        .budget
        .as_mut()
        .unwrap()
}

#[test]
fn finite_run_budget_roundtrips_without_granting_content_or_extending_request_limits() {
    for (jobs, bytes) in [(1, 1), (io::MAX_SERVICE_RUN_JOBS, io::MAX_BYTES)] {
        let mut m = manifest();
        run_budget(&mut m).max_jobs = jobs;
        run_budget(&mut m).max_bytes = bytes;
        let original = m.encode_to_vec();
        let package = Package::build(m.clone(), MODULE).unwrap();
        let decoded = Package::decode(package.archive()).unwrap();
        assert_eq!(decoded.manifest_bytes(), original);
        assert_eq!(decoded.io_declaration(), m.io_declaration.as_ref());
        assert!(decoded.capabilities().is_empty());
        let request = decoded.io_declaration().unwrap().budget.as_ref().unwrap();
        assert_eq!(request.max_jobs, io::MAX_JOBS);
        assert_eq!(request.max_job_bytes, io::MAX_JOB_BYTES);
        assert_eq!(request.max_duration_ms, io::MAX_DURATION_MS);
    }
}

#[test]
fn run_budget_requires_supported_version_and_finite_nonzero_values() {
    for version in [0, 2, u32::MAX] {
        let mut m = manifest();
        run_budget(&mut m).schema_version = version;
        assert!(matches!(
            Package::build(m, MODULE),
            Err(Error::UnsupportedVersion)
        ));
    }
    for jobs in [0, io::MAX_SERVICE_RUN_JOBS + 1, u64::MAX] {
        let mut m = manifest();
        run_budget(&mut m).max_jobs = jobs;
        assert!(matches!(Package::build(m, MODULE), Err(Error::Limit)));
    }
    for bytes in [0, io::MAX_BYTES + 1, u64::MAX] {
        let mut m = manifest();
        run_budget(&mut m).max_bytes = bytes;
        assert!(matches!(Package::build(m, MODULE), Err(Error::Limit)));
    }
}

#[test]
fn run_bytes_cannot_exceed_parent_but_may_be_stricter_than_a_job() {
    let mut m = manifest();
    let parent = m.io_declaration.as_mut().unwrap().budget.as_mut().unwrap();
    parent.max_bytes = 128;
    parent.max_job_bytes = 64;
    run_budget(&mut m).max_bytes = 129;
    assert!(matches!(
        Package::build(m.clone(), MODULE),
        Err(Error::Limit)
    ));
    run_budget(&mut m).max_bytes = 128;
    assert!(Package::build(m.clone(), MODULE).is_ok());
    run_budget(&mut m).max_bytes = 1;
    assert!(Package::build(m, MODULE).is_ok());
}

#[test]
fn required_feature_and_nested_budget_must_both_be_present() {
    let mut m = manifest();
    m.required_features
        .retain(|f| f != io::SERVICE_RUN_BUDGET_FEATURE);
    assert!(Package::build(m, MODULE).is_err());
    let mut m = manifest();
    m.io_declaration
        .as_mut()
        .unwrap()
        .service_run
        .as_mut()
        .unwrap()
        .budget = None;
    assert!(Package::build(m, MODULE).is_err());
    let mut m = manifest();
    m.required_features.retain(|f| f != io::SERVICE_RUN_FEATURE);
    assert!(Package::build(m, MODULE).is_err());
    let mut m = manifest();
    m.io_declaration.as_mut().unwrap().service_run = None;
    m.required_features.retain(|f| f != io::SERVICE_RUN_FEATURE);
    assert!(Package::build(m, MODULE).is_err());
    let mut m = manifest();
    m.io_declaration = None;
    m.required_features
        .retain(|f| f == io::SERVICE_RUN_BUDGET_FEATURE);
    assert!(Package::build(m, MODULE).is_err());
    let mut m = manifest();
    m.required_features
        .push(io::SERVICE_RUN_BUDGET_FEATURE.into());
    assert!(Package::build(m, MODULE).is_err());
}

#[test]
fn all_six_recognized_features_can_coexist() {
    let mut m = manifest();
    m.required_features.extend([
        plugin_package::TRANSFORM_HANDLERS_FEATURE.into(),
        plugin_package::DEPENDENCIES_FEATURE.into(),
        plugin_package::DEPENDENCY_CALLS_FEATURE.into(),
    ]);
    m.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
    m.transform_handlers.push(proto::TransformHandler {
        handler: "pure".into(),
        input_type: "in".into(),
        output_type: "out".into(),
        max_input_bytes: 1,
        max_output_bytes: 1,
    });
    assert!(Package::build(m.clone(), MODULE).is_ok());
    m.required_features.push("unknown-feature".into());
    assert!(Package::build(m, MODULE).is_err());
}

// This independent pre-budget type ensures absence retains the original wire shape.
#[derive(Clone, PartialEq, Message)]
struct LegacyServiceRunProfile {
    #[prost(uint32, tag = "1")]
    schema_version: u32,
    #[prost(uint64, tag = "2")]
    max_duration_ms: u64,
}

#[test]
fn absent_budget_preserves_old_profile_bytes_and_package_acceptance() {
    let mut m = manifest();
    m.required_features
        .retain(|f| f != io::SERVICE_RUN_BUDGET_FEATURE);
    let profile = m
        .io_declaration
        .as_mut()
        .unwrap()
        .service_run
        .as_mut()
        .unwrap();
    profile.budget = None;
    let old = LegacyServiceRunProfile {
        schema_version: 1,
        max_duration_ms: io::MAX_SERVICE_RUN_DURATION_MS,
    };
    assert_eq!(old.encode_to_vec(), profile.encode_to_vec());
    let bytes = m.encode_to_vec();
    assert_eq!(
        Package::from_parts(&bytes, MODULE)
            .unwrap()
            .manifest_bytes(),
        bytes
    );
}

fn append_message(bytes: &mut Vec<u8>, tag: u32, value: &[u8]) {
    prost::encoding::encode_varint(u64::from((tag << 3) | 2), bytes);
    prost::encoding::encode_varint(value.len() as u64, bytes);
    bytes.extend_from_slice(value);
}

fn with_budget_wire(budget: &[u8], duplicate: bool) -> Vec<u8> {
    let mut m = manifest();
    let mut declaration = m.io_declaration.take().unwrap();
    let mut profile = declaration.service_run.take().unwrap();
    profile.budget = None;
    let mut profile_bytes = profile.encode_to_vec();
    append_message(&mut profile_bytes, 3, budget);
    if duplicate {
        append_message(&mut profile_bytes, 3, budget);
    }
    let mut declaration_bytes = declaration.encode_to_vec();
    append_message(&mut declaration_bytes, 8, &profile_bytes);
    let mut bytes = m.encode_to_vec();
    append_message(&mut bytes, 18, &declaration_bytes);
    bytes
}

#[test]
fn canonical_nested_budget_rejects_unknown_duplicate_and_ambiguous_fields() {
    let canonical = vec![0x08, 0x01, 0x10, 0x01, 0x18, 0x01];
    assert!(Package::from_parts(&with_budget_wire(&canonical, false), MODULE).is_ok());
    let invalid = [
        vec![0x08, 0x01, 0x10, 0x01, 0x18, 0x01, 0x20, 0x01], // unknown field
        vec![0x08, 0x01, 0x10, 0x01, 0x18, 0x01, 0x08, 0x01], // repeated version
        vec![0x08, 0x01, 0x10, 0x01, 0x18, 0x01, 0x10, 0x01], // repeated jobs
        vec![0x08, 0x01, 0x10, 0x01, 0x18, 0x01, 0x18, 0x01], // repeated bytes
        vec![0x08, 0x81, 0x00, 0x10, 0x01, 0x18, 0x01],       // overlong version
        vec![0x10, 0x01, 0x08, 0x01, 0x18, 0x01],             // reordered fields
        vec![0x0a, 0x01, 0x01, 0x10, 0x01, 0x18, 0x01],       // wrong wire type
    ];
    for budget in invalid {
        assert!(Package::from_parts(&with_budget_wire(&budget, false), MODULE).is_err());
    }
    assert!(Package::from_parts(&with_budget_wire(&canonical, true), MODULE).is_err());
}
