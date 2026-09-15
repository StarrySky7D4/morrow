use morrow_core::{
    Error,
    io::{self, manifest_proto},
    plugin_package::{Package, proto::Capability},
};
use prost::Message;

const MODULE: &[u8] = b"\0asm\x01\0\0\0";

fn declaration(kinds: Vec<manifest_proto::IoKind>) -> Vec<u8> {
    manifest_proto::IoDeclaration {
        schema_version: 1,
        schema_sha256: io::schema_digest().to_vec(),
        requested: kinds.into_iter().map(|k| k as i32).collect(),
        effectful_handlers: vec![],
        budget: Some(manifest_proto::IoBudget {
            max_chunk_bytes: 64 * 1024,
            max_resources: 1,
            max_jobs: 1,
            max_job_bytes: 1024 * 1024,
            max_instance_bytes: 4 * 1024 * 1024,
            max_seconds: 5,
        }),
    }
    .encode_to_vec()
}

fn io_manifest() -> morrow_core::plugin_package::proto::Manifest {
    let mut manifest = Package::manifest_for_task("org.morrow.io", "0.1.9-test.51", MODULE, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    manifest.io_schema_sha256 = io::schema_digest().to_vec();
    manifest.io_declaration = declaration(vec![manifest_proto::IoKind::FileRead]);
    manifest
}

#[test]
fn io_feature_requires_declaration_and_guest_abi_v2() {
    assert!(Package::build(io_manifest(), MODULE).is_ok());
    let mut missing = io_manifest();
    missing.io_declaration.clear();
    assert!(Package::build(missing, MODULE).is_err());
    let mut digest = io_manifest();
    digest.io_schema_sha256[0] ^= 1;
    assert!(Package::build(digest, MODULE).is_err());
    let mut legacy = io_manifest();
    legacy.guest_abi_version = 1;
    legacy.task_schema_sha256.clear();
    assert!(Package::build(legacy, MODULE).is_err());
}

#[test]
fn unexpected_io_bytes_without_feature_are_rejected() {
    let mut manifest = Package::manifest_for("org.morrow.example", "0.1.9-test.10", MODULE, vec![]);
    manifest.io_declaration = declaration(vec![manifest_proto::IoKind::FileRead]);
    assert!(Package::build(manifest, MODULE).is_err());
}

#[test]
fn unknown_feature_still_fails_closed() {
    let mut manifest = Package::manifest_for("org.morrow.example", "0.1.9-test.10", MODULE, vec![Capability::RenameCard]);
    manifest.required_features.push("io-v2".into());
    assert!(matches!(
        Package::build(manifest, MODULE),
        Err(Error::UnsupportedVersion)
    ));
}

#[test]
fn empty_or_unspecified_declaration_rejected() {
    assert!(io::decode_declaration(b"", &io::schema_digest()).is_err());
    let empty = manifest_proto::IoDeclaration {
        schema_version: 1,
        schema_sha256: io::schema_digest().to_vec(),
        requested: vec![],
        effectful_handlers: vec![],
        budget: None,
    }
    .encode_to_vec();
    assert!(io::decode_declaration(&empty, &io::schema_digest()).is_err());
    let unspecified = declaration(vec![manifest_proto::IoKind::Unspecified]);
    assert!(io::decode_declaration(&unspecified, &io::schema_digest()).is_err());
}
