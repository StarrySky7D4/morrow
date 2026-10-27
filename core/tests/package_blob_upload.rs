#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error, Result,
    plugin_package::{
        Package, blob_upload,
        catalog::Catalog,
        io, proto,
        registry::{Registry, RegistryStorage},
    },
};
use prost::Message;
use std::{
    collections::BTreeMap,
    fs,
    sync::{Arc, Mutex},
};
mod wire {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.registry.v1.rs"));
}
const MODULE: &[u8] = b"\0asm\x01\0\0\0";
const ID: &str = "org.example.upload";
fn handler() -> proto::TransformHandler {
    proto::TransformHandler {
        handler: "blob.upload".into(),
        input_type: "upload.input.v1".into(),
        output_type: "upload.result.v1".into(),
        max_input_bytes: 1024,
        max_output_bytes: 1024,
    }
}
fn manifest() -> proto::Manifest {
    Package::manifest_for_blob_upload(ID, "1.0.0", MODULE, vec![handler()])
}
fn package(version: &str) -> Package {
    let mut m = manifest();
    m.package_version = version.into();
    Package::build(m, MODULE).unwrap()
}
fn field22(out: &mut Vec<u8>, bytes: &[u8]) {
    prost::encoding::encode_key(22, prost::encoding::WireType::LengthDelimited, out);
    prost::encoding::encode_varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}
#[test]
fn declaration_round_trip_is_separate_from_content_io_and_pure_transform() {
    let m = manifest();
    let original = m.encode_to_vec();
    let p = Package::build(m.clone(), MODULE).unwrap();
    assert_eq!(p.manifest_bytes(), original);
    assert_eq!(Package::decode(p.archive()).unwrap().manifest(), &m);
    assert_eq!(
        p.blob_upload_declaration(),
        m.blob_upload_declaration.as_ref()
    );
    assert!(p.capabilities().is_empty() && p.io_capabilities().is_empty());
    assert!(
        p.io_declaration().is_none() && p.channel_declaration().is_none() && !p.mutation_enabled()
    );
    let h = handler();
    assert!(
        p.transform_handler(&morrow_core::task::Transform {
            handler: h.handler,
            input_type: h.input_type,
            output_type: h.output_type,
            input: vec![]
        })
        .is_err()
    );
}
#[test]
fn independent_upload_fuel_ceiling_never_enlarges_legacy_budgets() {
    let mut m = manifest();
    assert_eq!(m.budget.as_ref().unwrap().fuel, 20_000_000);
    m.budget.as_mut().unwrap().fuel = blob_upload::MAX_FUEL;
    assert!(Package::build(m.clone(), MODULE).is_ok());
    m.budget.as_mut().unwrap().fuel += 1;
    assert_eq!(Package::build(m.clone(), MODULE).err(), Some(Error::Limit));
    m.blob_upload_declaration = None;
    m.required_features
        .retain(|feature| feature != blob_upload::FEATURE);
    m.budget.as_mut().unwrap().fuel = 100_000_001;
    assert_eq!(Package::build(m.clone(), MODULE).err(), Some(Error::Limit));
    m.budget.as_mut().unwrap().fuel = 100_000_000;
    assert!(Package::build(m, MODULE).is_ok());
}
#[test]
fn dedicated_upload_handler_checks_types_and_bounds_without_enabling_pure_execution() {
    let package = package("1.0.0");
    let input = morrow_core::task::Transform {
        handler: "blob.upload".into(),
        input_type: "upload.input.v1".into(),
        output_type: "upload.result.v1".into(),
        input: vec![1; 1024],
    };
    assert_eq!(package.blob_upload_handler(&input).unwrap(), &handler());
    assert!(package.transform_handler(&input).is_err());
    for field in 0..4 {
        let mut wrong = input.clone();
        match field {
            0 => wrong.handler = "other".into(),
            1 => wrong.input_type = "other".into(),
            2 => wrong.output_type = "other".into(),
            _ => wrong.input.push(1),
        }
        assert!(package.blob_upload_handler(&wrong).is_err());
    }
    let pure = Package::build(
        Package::manifest_for_transform(ID, "1.0.0", MODULE, vec![handler()]),
        MODULE,
    )
    .unwrap();
    assert!(pure.blob_upload_handler(&input).is_err());
    assert!(pure.transform_handler(&input).is_ok());
}
#[test]
fn declaration_requires_exact_feature_abi_and_both_schema_digests() {
    for case in 0..8 {
        let mut m = manifest();
        match case {
            0 => m.required_features.retain(|f| f != blob_upload::FEATURE),
            1 => m.blob_upload_declaration = None,
            2 => m.required_features.retain(|f| f != "transform-handlers-v1"),
            3 => {
                m.guest_abi_version = 1;
                m.task_schema_sha256.clear();
            }
            4 => m.blob_upload_declaration.as_mut().unwrap().schema_version = 2,
            5 => {
                m.blob_upload_declaration
                    .as_mut()
                    .unwrap()
                    .request_schema_sha256[0] ^= 1
            }
            6 => {
                m.blob_upload_declaration
                    .as_mut()
                    .unwrap()
                    .frame_schema_sha256[0] ^= 1
            }
            _ => m.required_features.push(blob_upload::FEATURE.into()),
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
}
#[test]
fn independent_profile_refuses_every_legacy_permission_combination() {
    for cap in [
        proto::Capability::RenameCard,
        proto::Capability::ReadSummary,
        proto::Capability::QueryOperation,
        proto::Capability::ReadAttachment,
        proto::Capability::CreateContent,
        proto::Capability::EditContent,
        proto::Capability::ReadContent,
    ] {
        let mut m = manifest();
        m.requested_capabilities.push(cap as i32);
        assert!(Package::build(m, MODULE).is_err());
    }
    for feature in [
        io::FEATURE,
        "dependencies-v1",
        "dependency-calls-v1",
        "mutation-v1",
        "mutation-budget-v1",
        "channel-v1",
        "changes-metadata-v1",
        "fs-directory-request-v1",
    ] {
        let mut m = manifest();
        m.required_features.push(feature.into());
        assert!(Package::build(m, MODULE).is_err());
    }
    let mut m = manifest();
    m.io_declaration = Some(io::declaration(
        vec![io::IoCapability::FileCreate],
        vec!["blob.upload".into()],
    ));
    assert!(Package::build(m, MODULE).is_err());
}
#[test]
fn declaration_budgets_and_complete_unique_handler_set_are_bounded() {
    for case in 0..11 {
        let mut m = manifest();
        let d = m.blob_upload_declaration.as_mut().unwrap();
        match case {
            0 => d.max_object_bytes = 0,
            1 => d.max_object_bytes = blob_upload::MAX_OBJECT_BYTES + 1,
            2 => d.max_calls = 0,
            3 => d.max_calls = blob_upload::MAX_CALLS + 1,
            4 => d.max_wire_bytes = 0,
            5 => d.max_wire_bytes = blob_upload::MAX_WIRE_BYTES + 1,
            6 => d.handlers.clear(),
            7 => d.handlers.push(d.handlers[0].clone()),
            8 => d.handlers[0] = "unregistered".into(),
            9 => d.handlers[0] = "invalid handler".into(),
            _ => {
                let mut h = handler();
                h.handler = "other".into();
                m.transform_handlers.push(h);
            }
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
    let mut m = manifest();
    let d = m.blob_upload_declaration.as_mut().unwrap();
    d.max_object_bytes = 1;
    d.max_calls = 1;
    d.max_wire_bytes = 1;
    assert!(Package::build(m, MODULE).is_ok()); // A ceiling does not promise a runnable operation.
}
#[test]
fn duplicate_unknown_and_noncanonical_upload_fields_are_not_silently_discarded() {
    let m = manifest();
    let declaration = m.blob_upload_declaration.as_ref().unwrap().encode_to_vec();
    let mut duplicate = m.encode_to_vec();
    field22(&mut duplicate, &declaration);
    assert!(Package::from_parts(&duplicate, MODULE).is_err());
    for suffix in [[0x40, 1], [0x08, 1]] {
        let mut changed = declaration.clone();
        changed.extend_from_slice(&suffix);
        let mut outer = m.clone();
        outer.blob_upload_declaration = None;
        let mut raw = outer.encode_to_vec();
        field22(&mut raw, &changed);
        assert!(Package::from_parts(&raw, MODULE).is_err());
    }
}
fn open(d: &tempfile::TempDir) -> Registry {
    Registry::open(
        &d.path().join("registry"),
        Catalog::open(&d.path().join("packages")).unwrap(),
    )
    .unwrap()
}
fn snapshot(r: &Registry) -> wire::Registry {
    let bytes = r.persisted_snapshot().unwrap().unwrap();
    let len = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    wire::Registry::decode(
        lz4_flex::block::decompress(&bytes[50..], len)
            .unwrap()
            .as_slice(),
    )
    .unwrap()
}
#[test]
fn exact_upload_approval_persists_v3_without_restoring_live_authority() {
    let d = tempfile::tempdir().unwrap();
    let mut r = open(&d);
    let p = package("1.0.0");
    r.install_package(p.archive()).unwrap();
    r.select(p.digest(), 0).unwrap();
    assert!(!r.selection(ID).unwrap().approved_blob_upload);
    assert_eq!(snapshot(&r).schema_version, 2);
    r.approve_blob_upload(ID, p.digest(), true, r.revision())
        .unwrap();
    let revision = r.revision();
    assert_eq!(snapshot(&r).schema_version, 3);
    assert!(!r.selection(ID).unwrap().enabled);
    assert!(
        r.selection(ID).unwrap().approved.is_empty()
            && r.selection(ID).unwrap().approved_io.is_empty()
    );
    drop(r);
    let mut r = open(&d);
    assert_eq!(r.revision(), revision);
    assert!(r.selection(ID).unwrap().approved_blob_upload);
    r.approve_blob_upload(ID, p.digest(), false, r.revision())
        .unwrap();
    assert!(!r.selection(ID).unwrap().approved_blob_upload);
    assert_eq!(snapshot(&r).schema_version, 3); // Do not downgrade a v3 history after revocation.
    let revision = r.revision();
    r.approve_blob_upload(ID, p.digest(), false, revision)
        .unwrap();
    assert_eq!(r.revision(), revision);
}
#[test]
fn upgrade_and_reselection_never_inherit_the_old_upload_approval() {
    let d = tempfile::tempdir().unwrap();
    let mut r = open(&d);
    let first = package("1.0.0");
    let next = package("2.0.0");
    for p in [&first, &next] {
        r.install_package(p.archive()).unwrap();
    }
    r.select(first.digest(), 0).unwrap();
    r.approve_blob_upload(ID, first.digest(), true, r.revision())
        .unwrap();
    r.set_enabled(ID, first.digest(), true, r.revision())
        .unwrap();
    r.select(next.digest(), r.revision()).unwrap();
    let s = r.selection(ID).unwrap();
    assert!(!s.approved_blob_upload && !s.enabled);
    assert_eq!(
        r.approve_blob_upload(ID, first.digest(), true, r.revision()),
        Err(Error::RevisionConflict)
    );
    r.approve_blob_upload(ID, next.digest(), true, r.revision())
        .unwrap();
    r.remove(ID, r.revision()).unwrap();
    r.select(next.digest(), r.revision()).unwrap();
    assert!(!r.selection(ID).unwrap().approved_blob_upload);
}
#[test]
fn invalid_approval_keeps_snapshot_and_revocation_survives_a_missing_package() {
    let d = tempfile::tempdir().unwrap();
    let mut r = open(&d);
    let p = package("1.0.0");
    let path = Catalog::open(&d.path().join("packages"))
        .unwrap()
        .install(&p)
        .unwrap();
    r.select(p.digest(), 0).unwrap();
    let before = r.persisted_snapshot().unwrap();
    assert!(
        r.approve_blob_upload(ID, [9; 32], true, r.revision())
            .is_err()
    );
    assert!(r.approve_blob_upload(ID, p.digest(), true, 0).is_err());
    assert_eq!(r.persisted_snapshot().unwrap(), before);
    r.approve_blob_upload(ID, p.digest(), true, r.revision())
        .unwrap();
    fs::remove_file(path).unwrap();
    assert!(
        r.approve_blob_upload(ID, p.digest(), true, r.revision())
            .is_err()
    );
    r.approve_blob_upload(ID, p.digest(), false, r.revision())
        .unwrap();
    assert!(!r.selection(ID).unwrap().approved_blob_upload);
}
#[test]
fn ordinary_content_and_io_approval_cannot_approve_upload() {
    let d = tempfile::tempdir().unwrap();
    let mut r = open(&d);
    let mut m =
        Package::manifest_for_task(ID, "1.0.0", MODULE, vec![proto::Capability::CreateContent]);
    m.required_features.push(io::FEATURE.into());
    m.io_declaration = Some(io::declaration(
        vec![io::IoCapability::FileCreate],
        vec!["ordinary".into()],
    ));
    let p = Package::build(m, MODULE).unwrap();
    r.install_package(p.archive()).unwrap();
    r.select(p.digest(), 0).unwrap();
    r.approve(ID, p.digest(), p.capabilities().clone(), r.revision())
        .unwrap();
    r.approve_io(ID, p.digest(), p.io_capabilities().clone(), r.revision())
        .unwrap();
    let before = r.persisted_snapshot().unwrap();
    assert!(
        r.approve_blob_upload(ID, p.digest(), true, r.revision())
            .is_err()
    );
    assert!(!r.selection(ID).unwrap().approved_blob_upload);
    assert_eq!(r.persisted_snapshot().unwrap(), before);
    assert_eq!(snapshot(&r).schema_version, 2);
}
#[derive(Default)]
struct Memory {
    bytes: Option<Vec<u8>>,
    packages: BTreeMap<[u8; 32], Vec<u8>>,
    fail_after_publish: bool,
}
struct Storage(Arc<Mutex<Memory>>);
impl RegistryStorage for Storage {
    fn read(&self) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().bytes.clone())
    }
    fn publish(&mut self, bytes: &[u8]) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        s.bytes = Some(bytes.to_vec());
        if s.fail_after_publish {
            Err(Error::CommitUnknown)
        } else {
            Ok(())
        }
    }
    fn load_package(&self, d: [u8; 32]) -> Result<Package> {
        Package::decode(
            self.0
                .lock()
                .unwrap()
                .packages
                .get(&d)
                .ok_or(Error::NotFound)?,
        )
    }
    fn install_package(&self, p: &Package) -> Result<()> {
        self.0
            .lock()
            .unwrap()
            .packages
            .insert(p.digest(), p.archive().to_vec());
        Ok(())
    }
}
#[test]
fn unknown_approval_publication_does_not_allow_a_noop_or_second_decision() {
    let state = Arc::new(Mutex::new(Memory::default()));
    let mut r = Registry::from_storage(Box::new(Storage(state.clone()))).unwrap();
    let p = package("1.0.0");
    r.install_package(p.archive()).unwrap();
    r.select(p.digest(), 0).unwrap();
    let revision = r.revision();
    state.lock().unwrap().fail_after_publish = true;
    assert_eq!(
        r.approve_blob_upload(ID, p.digest(), true, revision),
        Err(Error::CommitUnknown)
    );
    assert!(!r.selection(ID).unwrap().approved_blob_upload);
    assert_eq!(r.ensure_ready(), Err(Error::CommitUnknown));
    assert_eq!(
        r.approve_blob_upload(ID, p.digest(), false, revision),
        Err(Error::CommitUnknown)
    );
    // Reopening the original fixture observes the committed ceiling only, no live grant.
    drop(r);
    let r = Registry::from_storage(Box::new(Storage(state))).unwrap();
    assert!(r.selection(ID).unwrap().approved_blob_upload);
    assert_eq!(r.revision(), revision + 1);
}
#[test]
fn older_registry_versions_cannot_smuggle_the_new_approval() {
    for version in [1, 2] {
        let state = Arc::new(Mutex::new(Memory::default()));
        let mut r = Registry::from_storage(Box::new(Storage(state.clone()))).unwrap();
        let p = package("1.0.0");
        r.install_package(p.archive()).unwrap();
        r.select(p.digest(), 0).unwrap();
        r.approve_blob_upload(ID, p.digest(), true, r.revision())
            .unwrap();
        let mut s = snapshot(&r);
        s.schema_version = version;
        let raw = s.encode_to_vec();
        let packed = lz4_flex::block::compress(&raw);
        use sha2::{Digest, Sha256};
        let mut bytes = b"MORROWG1".to_vec();
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&Sha256::digest(&raw));
        bytes.extend(packed);
        drop(r);
        state.lock().unwrap().bytes = Some(bytes.clone());
        assert!(matches!(
            Registry::from_storage(Box::new(Storage(state.clone()))),
            Err(Error::UnsupportedVersion)
        ));
        assert_eq!(state.lock().unwrap().bytes, Some(bytes));
    }
}
