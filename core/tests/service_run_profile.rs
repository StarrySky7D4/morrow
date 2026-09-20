use morrow_core::{
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
    let mut manifest = Package::manifest_for_task("service.run", "1.0.0", MODULE, vec![]);
    manifest.required_features = vec![io::FEATURE.into(), io::SERVICE_RUN_FEATURE.into()];
    let mut declaration = io::declaration(
        vec![IoCapability::HttpListen, IoCapability::HttpPublish],
        vec!["api.invoke".into()],
    );
    declaration.service_schema_sha256 = service::schema_digest().to_vec();
    declaration.service_run = Some(io::proto::ServiceRunProfile {
        schema_version: io::SERVICE_RUN_VERSION,
        max_duration_ms: io::MAX_SERVICE_RUN_DURATION_MS,
        budget: None,
    });
    manifest.io_declaration = Some(declaration);
    manifest
}

fn reject(edit: impl FnOnce(&mut proto::Manifest)) {
    let mut manifest = manifest();
    edit(&mut manifest);
    assert!(Package::build(manifest, MODULE).is_err());
}

#[test]
fn finite_profile_roundtrips_without_content_grants_or_request_budget_extension() {
    for duration in [1, 30_001, io::MAX_SERVICE_RUN_DURATION_MS] {
        let mut manifest = manifest();
        manifest
            .io_declaration
            .as_mut()
            .unwrap()
            .service_run
            .as_mut()
            .unwrap()
            .max_duration_ms = duration;
        let bytes = manifest.encode_to_vec();
        let package = Package::build(manifest.clone(), MODULE).unwrap();
        let decoded = Package::decode(package.archive()).unwrap();
        assert_eq!(decoded.manifest_bytes(), bytes);
        assert_eq!(decoded.io_declaration(), manifest.io_declaration.as_ref());
        assert!(decoded.capabilities().is_empty());
        assert_eq!(
            decoded
                .io_declaration()
                .unwrap()
                .budget
                .as_ref()
                .unwrap()
                .max_duration_ms,
            30_000
        );
    }
}

#[test]
fn profile_and_feature_must_both_be_present() {
    reject(|m| m.required_features.retain(|f| f != io::SERVICE_RUN_FEATURE));
    reject(|m| m.io_declaration.as_mut().unwrap().service_run = None);
    reject(|m| {
        m.io_declaration = None;
        m.required_features.retain(|f| f != io::FEATURE);
    });
    reject(|m| m.required_features.retain(|f| f != io::FEATURE));
    reject(|m| m.required_features.push(io::SERVICE_RUN_FEATURE.into()));
    reject(|m| {
        m.guest_abi_version = 1;
        m.task_schema_sha256.clear();
    });
}

#[test]
fn profile_version_and_finite_duration_are_required() {
    for version in [0, 2, u32::MAX] {
        reject(|m| {
            m.io_declaration
                .as_mut()
                .unwrap()
                .service_run
                .as_mut()
                .unwrap()
                .schema_version = version;
        });
    }
    for duration in [0, io::MAX_SERVICE_RUN_DURATION_MS + 1, u64::MAX] {
        reject(|m| {
            m.io_declaration
                .as_mut()
                .unwrap()
                .service_run
                .as_mut()
                .unwrap()
                .max_duration_ms = duration;
        });
    }
    reject(|m| {
        m.io_declaration
            .as_mut()
            .unwrap()
            .budget
            .as_mut()
            .unwrap()
            .max_duration_ms = 30_001;
    });
}

#[test]
fn profile_requires_listen_publish_and_exact_service_schema() {
    for missing in [IoCapability::HttpListen, IoCapability::HttpPublish] {
        reject(|m| {
            m.io_declaration
                .as_mut()
                .unwrap()
                .requested_capabilities
                .retain(|c| *c != missing.number());
        });
    }
    for digest in [vec![], vec![0; 31], vec![0; 32], vec![0; 33]] {
        reject(|m| m.io_declaration.as_mut().unwrap().service_schema_sha256 = digest);
    }
}

#[test]
fn all_five_recognized_features_can_coexist_with_distinct_handlers() {
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

// Independent pre-extension message definition proves omission emits the same bytes.
#[derive(Clone, PartialEq, Message)]
struct LegacyIoDeclaration {
    #[prost(uint32, tag = "1")]
    schema_version: u32,
    #[prost(uint32, tag = "2")]
    io_version: u32,
    #[prost(bytes = "vec", tag = "3")]
    io_schema_sha256: Vec<u8>,
    #[prost(int32, repeated, tag = "4")]
    requested_capabilities: Vec<i32>,
    #[prost(string, repeated, tag = "5")]
    handlers: Vec<String>,
    #[prost(message, optional, tag = "6")]
    budget: Option<io::proto::IoBudget>,
    #[prost(bytes = "vec", tag = "7")]
    service_schema_sha256: Vec<u8>,
}

#[test]
fn old_declaration_bytes_and_absent_profile_remain_unchanged() {
    let mut m = manifest();
    m.required_features.retain(|f| f != io::SERVICE_RUN_FEATURE);
    let declaration = io::declaration(vec![IoCapability::HttpRequest], vec!["api.invoke".into()]);
    assert!(declaration.service_run.is_none());
    let old = LegacyIoDeclaration {
        schema_version: 1,
        io_version: 1,
        io_schema_sha256: io::schema_digest().to_vec(),
        requested_capabilities: vec![IoCapability::HttpRequest.number()],
        handlers: vec!["api.invoke".into()],
        budget: declaration.budget,
        service_schema_sha256: vec![],
    };
    assert_eq!(old.encode_to_vec(), declaration.encode_to_vec());
    m.io_declaration = Some(declaration);
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

fn with_profile_wire(profile: &[u8], duplicate: bool) -> Vec<u8> {
    let mut m = manifest();
    let mut declaration = m.io_declaration.take().unwrap();
    declaration.service_run = None;
    let mut wire = declaration.encode_to_vec();
    append_message(&mut wire, 8, profile);
    if duplicate {
        append_message(&mut wire, 8, profile);
    }
    let mut bytes = m.encode_to_vec();
    append_message(&mut bytes, 18, &wire);
    bytes
}

#[test]
fn nested_unknown_duplicate_noncanonical_and_wrong_wire_fields_reject() {
    let canonical = vec![0x08, 0x01, 0x10, 0x01];
    assert!(Package::from_parts(&with_profile_wire(&canonical, false), MODULE).is_ok());
    let invalid = [
        vec![0x08, 0x01, 0x10, 0x01, 0x18, 0x01], // unknown nested field
        vec![0x08, 0x01, 0x10, 0x01, 0x08, 0x01], // repeated version
        vec![0x08, 0x01, 0x10, 0x01, 0x10, 0x01], // repeated duration
        vec![0x08, 0x81, 0x00, 0x10, 0x01],       // overlong varint
        vec![0x10, 0x01, 0x08, 0x01],             // reordered fields
        vec![0x0a, 0x01, 0x01, 0x10, 0x01],       // version wrong wire type
    ];
    for profile in invalid {
        assert!(Package::from_parts(&with_profile_wire(&profile, false), MODULE).is_err());
    }
    assert!(Package::from_parts(&with_profile_wire(&canonical, true), MODULE).is_err());
}

#[test]
fn unknown_outer_optional_metadata_is_preserved_with_new_profile() {
    let mut bytes = manifest().encode_to_vec();
    bytes.extend_from_slice(&[0xa0, 0x06, 0x01]);
    assert_eq!(
        Package::from_parts(&bytes, MODULE)
            .unwrap()
            .manifest_bytes(),
        bytes
    );
}
