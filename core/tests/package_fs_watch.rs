#![cfg(not(target_arch = "wasm32"))]
#![deny(warnings)]
use morrow_core::{
    Error, Result,
    plugin_package::{
        Package, fs_watch, io, proto,
        registry::{Registry, RegistryStorage},
    },
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
mod wire {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.registry.v1.rs"));
}
const MODULE: &[u8] = b"\0asm\x01\0\0\0";
const ID: &str = "org.example.watch";
fn manifest() -> proto::Manifest {
    Package::manifest_for_fs_watch(ID, "1.0.0", MODULE, vec!["observe".into()])
}
fn package(version: &str) -> Package {
    let mut m = manifest();
    m.package_version = version.into();
    Package::build(m, MODULE).unwrap()
}
#[test]
fn watch_metadata_round_trip_preserves_exact_bytes_and_does_not_add_io_capability() {
    let m = manifest();
    let bytes = m.encode_to_vec();
    let p = Package::build(m.clone(), MODULE).unwrap();
    assert_eq!(p.manifest_bytes(), bytes);
    assert_eq!(Package::decode(p.archive()).unwrap().manifest(), &m);
    assert_eq!(p.fs_watch_declaration(), m.fs_watch_declaration.as_ref());
    assert_eq!(
        p.io_capabilities(),
        &[io::IoCapability::FileList].into_iter().collect()
    );
    assert!(p.capabilities().is_empty());
    assert!(io::IoCapability::from_number(11).is_err());
    assert_eq!(io::MAX_CAPABILITIES, 10);
    assert_eq!(p.manifest().budget.as_ref().unwrap().fuel, 20_000_000);
}
#[test]
fn watch_profile_refuses_missing_feature_hash_and_legacy_combinations() {
    for case in 0..16 {
        let mut m = manifest();
        match case {
            0 => m.required_features.retain(|f| f != fs_watch::FEATURE),
            1 => m.fs_watch_declaration = None,
            2 => m.required_features.retain(|f| f != io::FEATURE),
            3 => m.io_declaration = None,
            4 => {
                m.guest_abi_version = 1;
                m.task_schema_sha256.clear();
            }
            5 => m.fs_watch_declaration.as_mut().unwrap().schema_version = 2,
            6 => m.fs_watch_declaration.as_mut().unwrap().frame_schema_sha256[0] ^= 1,
            7 => m.required_features.push(fs_watch::FEATURE.into()),
            8 => m
                .requested_capabilities
                .push(proto::Capability::ReadContent as i32),
            9 => {
                m.io_declaration.as_mut().unwrap().requested_capabilities =
                    vec![io::IoCapability::FileRead.number()]
            }
            10 => m
                .io_declaration
                .as_mut()
                .unwrap()
                .requested_capabilities
                .push(io::IoCapability::FileRead.number()),
            11 => m.required_features.push("fs-directory-request-v1".into()),
            12 => m.required_features.push("channel-v1".into()),
            13 => m.required_features.push("blob-upload-request-v1".into()),
            14 => m.required_features.push("mutation-v1".into()),
            _ => m.required_features.push("dependencies-v1".into()),
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
}
#[test]
fn watch_limits_remain_subordinate_to_original_io_bytes_and_lifetime() {
    for case in 0..10 {
        let mut m = manifest();
        let d = m.fs_watch_declaration.as_mut().unwrap();
        match case {
            0 => d.max_frames = 0,
            1 => d.max_frames = fs_watch::MAX_FRAMES + 1,
            2 => d.max_wire_bytes = 0,
            3 => d.max_wire_bytes = fs_watch::MAX_WIRE_BYTES + 1,
            4 => d.max_window_ms = 0,
            5 => d.max_window_ms = fs_watch::MAX_WINDOW_MS + 1,
            6 => {
                m.io_declaration
                    .as_mut()
                    .unwrap()
                    .budget
                    .as_mut()
                    .unwrap()
                    .max_job_bytes = 1
            }
            7 => {
                m.io_declaration
                    .as_mut()
                    .unwrap()
                    .budget
                    .as_mut()
                    .unwrap()
                    .max_bytes = 1
            }
            8 => {
                m.io_declaration
                    .as_mut()
                    .unwrap()
                    .budget
                    .as_mut()
                    .unwrap()
                    .max_duration_ms = 1
            }
            _ => m.budget.as_mut().unwrap().fuel = 100_000_001,
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
    let mut m = manifest();
    let d = m.fs_watch_declaration.as_mut().unwrap();
    d.max_frames = 1;
    d.max_wire_bytes = 1;
    d.max_window_ms = 1;
    assert!(Package::build(m, MODULE).is_ok());
}
#[test]
fn watch_and_io_handler_sets_must_be_complete_unique_and_separate_from_transforms() {
    for case in 0..6 {
        let mut m = manifest();
        match case {
            0 => m.fs_watch_declaration.as_mut().unwrap().handlers.clear(),
            1 => m
                .fs_watch_declaration
                .as_mut()
                .unwrap()
                .handlers
                .push("observe".into()),
            2 => m.fs_watch_declaration.as_mut().unwrap().handlers[0] = "other".into(),
            3 => m
                .io_declaration
                .as_mut()
                .unwrap()
                .handlers
                .push("other".into()),
            4 => m.fs_watch_declaration.as_mut().unwrap().handlers[0] = "bad name".into(),
            _ => {
                m.required_features.push("transform-handlers-v1".into());
                m.transform_handlers.push(proto::TransformHandler {
                    handler: "observe".into(),
                    input_type: "input.v1".into(),
                    output_type: "output.v1".into(),
                    max_input_bytes: 32,
                    max_output_bytes: 32,
                });
            }
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
}
fn field24(out: &mut Vec<u8>, value: &[u8]) {
    prost::encoding::encode_key(24, prost::encoding::WireType::LengthDelimited, out);
    prost::encoding::encode_varint(value.len() as u64, out);
    out.extend_from_slice(value);
}
#[test]
fn watch_declaration_rejects_duplicate_unknown_and_noncanonical_privileged_wire() {
    let m = manifest();
    let d = m.fs_watch_declaration.as_ref().unwrap().encode_to_vec();
    let mut repeated = m.encode_to_vec();
    field24(&mut repeated, &d);
    assert!(Package::from_parts(&repeated, MODULE).is_err());
    for suffix in [[0x08, 1], [0x38, 1]] {
        let mut m = m.clone();
        m.fs_watch_declaration = None;
        let mut raw = m.encode_to_vec();
        let mut changed = d.clone();
        changed.extend_from_slice(&suffix);
        field24(&mut raw, &changed);
        assert!(Package::from_parts(&raw, MODULE).is_err());
    }
    let mut m = m.clone();
    m.fs_watch_declaration = None;
    let mut raw = m.encode_to_vec();
    let mut changed = d;
    assert_eq!(&changed[..2], &[8, 1]);
    changed.splice(1..2, [0x81, 0]);
    field24(&mut raw, &changed);
    assert!(Package::from_parts(&raw, MODULE).is_err());
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
fn setup(p: &Package) -> (Arc<Mutex<Memory>>, Registry) {
    let s = Arc::new(Mutex::new(Memory::default()));
    let mut r = Registry::from_storage(Box::new(Storage(s.clone()))).unwrap();
    r.install_package(p.archive()).unwrap();
    r.select(p.digest(), 0).unwrap();
    (s, r)
}
fn snapshot(r: &Registry) -> wire::Registry {
    let b = r.persisted_snapshot().unwrap().unwrap();
    let n = u32::from_le_bytes(b[10..14].try_into().unwrap()) as usize;
    wire::Registry::decode(lz4_flex::block::decompress(&b[50..], n).unwrap().as_slice()).unwrap()
}
fn pack(s: &wire::Registry) -> Vec<u8> {
    let raw = s.encode_to_vec();
    let packed = lz4_flex::block::compress(&raw);
    let mut bytes = b"MORROWG1".to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(&raw));
    bytes.extend(packed);
    bytes
}
#[test]
fn list_approval_never_implies_watch_and_v4_restore_grants_no_live_object() {
    let p = package("1.0.0");
    let (s, mut r) = setup(&p);
    r.approve_io(ID, p.digest(), p.io_capabilities().clone(), r.revision())
        .unwrap();
    r.set_enabled(ID, p.digest(), true, r.revision()).unwrap();
    assert!(!r.selection(ID).unwrap().approved_fs_watch);
    assert_eq!(snapshot(&r).schema_version, 2);
    r.approve_fs_watch(ID, p.digest(), true, r.revision())
        .unwrap();
    assert_eq!(snapshot(&r).schema_version, 4);
    let rev = r.revision();
    drop(r);
    let mut r = Registry::from_storage(Box::new(Storage(s))).unwrap();
    assert!(r.selection(ID).unwrap().approved_fs_watch);
    assert_eq!(r.revision(), rev);
    assert!(!r.selection(ID).unwrap().approved_blob_upload);
    r.approve_fs_watch(ID, p.digest(), false, r.revision())
        .unwrap();
    assert_eq!(snapshot(&r).schema_version, 4);
    let rev = r.revision();
    r.approve_fs_watch(ID, p.digest(), false, rev).unwrap();
    assert_eq!(r.revision(), rev);
}
#[test]
fn upgrade_removal_and_reselection_clear_watch_approval() {
    let p = package("1.0.0");
    let next = package("2.0.0");
    let (_, mut r) = setup(&p);
    r.install_package(next.archive()).unwrap();
    r.approve_fs_watch(ID, p.digest(), true, r.revision())
        .unwrap();
    r.set_enabled(ID, p.digest(), true, r.revision()).unwrap();
    r.select(next.digest(), r.revision()).unwrap();
    assert!(!r.selection(ID).unwrap().approved_fs_watch);
    assert!(!r.selection(ID).unwrap().enabled);
    assert_eq!(
        r.approve_fs_watch(ID, p.digest(), true, r.revision()),
        Err(Error::RevisionConflict)
    );
    r.approve_fs_watch(ID, next.digest(), true, r.revision())
        .unwrap();
    r.remove(ID, r.revision()).unwrap();
    r.select(next.digest(), r.revision()).unwrap();
    assert!(!r.selection(ID).unwrap().approved_fs_watch);
}
#[test]
fn invalid_approval_is_readonly_and_revocation_survives_missing_package() {
    let p = package("1.0.0");
    let (s, mut r) = setup(&p);
    let before = r.persisted_snapshot().unwrap();
    assert_eq!(
        r.approve_fs_watch(ID, [9; 32], true, r.revision()),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        r.approve_fs_watch(ID, p.digest(), true, 0),
        Err(Error::RevisionConflict)
    );
    assert_eq!(r.persisted_snapshot().unwrap(), before);
    r.approve_fs_watch(ID, p.digest(), true, r.revision())
        .unwrap();
    s.lock().unwrap().packages.clear();
    assert!(
        r.approve_fs_watch(ID, p.digest(), true, r.revision())
            .is_err()
    );
    r.approve_fs_watch(ID, p.digest(), false, r.revision())
        .unwrap();
    assert!(!r.selection(ID).unwrap().approved_fs_watch);
}
#[test]
fn ordinary_list_package_cannot_receive_independent_watch_approval() {
    let mut m = manifest();
    m.fs_watch_declaration = None;
    m.required_features.retain(|f| f != fs_watch::FEATURE);
    let p = Package::build(m, MODULE).unwrap();
    let (_, mut r) = setup(&p);
    r.approve_io(ID, p.digest(), p.io_capabilities().clone(), r.revision())
        .unwrap();
    let before = r.persisted_snapshot().unwrap();
    assert!(
        r.approve_fs_watch(ID, p.digest(), true, r.revision())
            .is_err()
    );
    assert_eq!(r.persisted_snapshot().unwrap(), before);
    assert_eq!(snapshot(&r).schema_version, 2);
}
#[test]
fn uncertain_watch_approval_blocks_second_decision_and_noop() {
    let p = package("1.0.0");
    let (s, mut r) = setup(&p);
    let rev = r.revision();
    s.lock().unwrap().fail_after_publish = true;
    assert_eq!(
        r.approve_fs_watch(ID, p.digest(), true, rev),
        Err(Error::CommitUnknown)
    );
    assert!(!r.selection(ID).unwrap().approved_fs_watch);
    assert_eq!(r.ensure_ready(), Err(Error::CommitUnknown));
    assert_eq!(
        r.approve_fs_watch(ID, p.digest(), false, rev),
        Err(Error::CommitUnknown)
    );
    drop(r);
    let r = Registry::from_storage(Box::new(Storage(s))).unwrap();
    assert!(r.selection(ID).unwrap().approved_fs_watch);
    assert_eq!(r.revision(), rev + 1);
}
#[test]
fn old_registry_versions_cannot_smuggle_watch_or_silently_downgrade_v4() {
    for version in [1, 2, 3, 5] {
        let p = package("1.0.0");
        let (s, mut r) = setup(&p);
        r.approve_fs_watch(ID, p.digest(), true, r.revision())
            .unwrap();
        let mut raw = snapshot(&r);
        raw.schema_version = version;
        let bytes = pack(&raw);
        drop(r);
        s.lock().unwrap().bytes = Some(bytes.clone());
        assert!(matches!(
            Registry::from_storage(Box::new(Storage(s.clone()))),
            Err(Error::UnsupportedVersion)
        ));
        assert_eq!(s.lock().unwrap().bytes, Some(bytes));
    }
}
#[test]
fn actual_native_registry_reopen_preserves_only_the_exact_package_ceiling() {
    use morrow_core::plugin_package::catalog::Catalog;
    let dir = tempfile::tempdir().unwrap();
    let p = package("1.0.0");
    let catalog = Catalog::open(&dir.path().join("packages")).unwrap();
    catalog.install(&p).unwrap();
    let mut r = Registry::open(&dir.path().join("registry"), catalog).unwrap();
    r.select(p.digest(), 0).unwrap();
    r.approve_fs_watch(ID, p.digest(), true, r.revision())
        .unwrap();
    let bytes = r.persisted_snapshot().unwrap();
    let rev = r.revision();
    drop(r);
    let mut r = Registry::open(
        &dir.path().join("registry"),
        Catalog::open(&dir.path().join("packages")).unwrap(),
    )
    .unwrap();
    assert_eq!(r.persisted_snapshot().unwrap(), bytes);
    assert_eq!(r.revision(), rev);
    let selected = r.selection(ID).unwrap();
    assert!(selected.approved_fs_watch);
    assert!(!selected.enabled);
    assert!(selected.approved_io.is_empty());
    r.approve_fs_watch(ID, p.digest(), false, rev).unwrap();
    drop(r);
    let r = Registry::open(
        &dir.path().join("registry"),
        Catalog::open(&dir.path().join("packages")).unwrap(),
    )
    .unwrap();
    assert!(!r.selection(ID).unwrap().approved_fs_watch);
    assert_eq!(snapshot(&r).schema_version, 4);
}
