#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        proto::Capability,
        registry::Registry,
    },
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::PathBuf};
use tempfile::TempDir;
mod wire {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.registry.v1.rs"));
}
const ID: &str = "org.example.io";
fn package(version: &str, caps: Vec<IoCapability>) -> Package {
    let module = b"\0asm\x01\0\0\0";
    let mut m = Package::manifest_for_task(
        ID,
        version,
        module,
        vec![Capability::ReadContent, Capability::RenameCard],
    );
    m.io_declaration = Some(io::declaration(caps, vec!["api.invoke".into()]));
    m.required_features.push(io::FEATURE.into());
    Package::build(m, module).unwrap()
}
fn path(d: &TempDir) -> PathBuf {
    d.path().join("registry/selection.morrow")
}
fn open(d: &TempDir) -> morrow_core::Result<Registry> {
    Registry::open(
        &d.path().join("registry"),
        Catalog::open(&d.path().join("packages"))?,
    )
}
fn setup() -> (TempDir, Registry) {
    let d = tempfile::tempdir().unwrap();
    let r = open(&d).unwrap();
    (d, r)
}
fn install(d: &TempDir, p: &Package) {
    Catalog::open(&d.path().join("packages"))
        .unwrap()
        .install(p)
        .unwrap();
}
fn read(d: &TempDir) -> wire::Registry {
    let bytes = fs::read(path(d)).unwrap();
    let len = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    wire::Registry::decode(
        lz4_flex::block::decompress(&bytes[50..], len)
            .unwrap()
            .as_slice(),
    )
    .unwrap()
}
fn store(d: &TempDir, state: &wire::Registry) -> Vec<u8> {
    let raw = state.encode_to_vec();
    let compressed = lz4_flex::block::compress(&raw);
    let mut bytes = b"MORROWG1".to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(&raw));
    bytes.extend(compressed);
    fs::write(path(d), &bytes).unwrap();
    bytes
}
fn legacy(d: &TempDir, p: &Package, revision: u64) -> Vec<u8> {
    store(
        d,
        &wire::Registry {
            schema_version: 1,
            revision,
            selections: vec![wire::Selection {
                package_id: ID.into(),
                digest: p.digest().to_vec(),
                enabled: true,
                approved_capabilities: vec![1, 7],
                approved_io_capabilities: vec![],
            }],
            dependency_locks: vec![],
        },
    )
}
#[test]
fn v1_migration_is_durable_advances_once_and_never_infers_io_from_content() {
    let (d, r) = setup();
    let p = package(
        "1.0.0",
        vec![IoCapability::FileRead, IoCapability::HttpRequest],
    );
    install(&d, &p);
    drop(r);
    let old = legacy(&d, &p, 17);
    let r = open(&d).unwrap();
    assert_eq!(r.revision(), 18);
    let s = r.selection(ID).unwrap();
    assert!(s.enabled);
    assert_eq!(
        s.approved,
        BTreeSet::from([GrantKind::Rename, GrantKind::ReadContent])
    );
    assert!(s.approved_io.is_empty());
    assert_eq!(s.digest, p.digest());
    assert_ne!(fs::read(path(&d)).unwrap(), old);
    assert_eq!(read(&d).schema_version, 2);
    assert!(read(&d).selections[0].approved_io_capabilities.is_empty());
    drop(r);
    assert_eq!(open(&d).unwrap().revision(), 18);
}
#[test]
fn io_approval_is_independent_subset_cas_and_noop_does_not_republish() {
    let (d, mut r) = setup();
    let p = package(
        "1.0.0",
        vec![IoCapability::FileRead, IoCapability::HttpRequest],
    );
    install(&d, &p);
    r.select(p.digest(), 0).unwrap();
    assert_eq!(read(&d).schema_version, 2);
    let rev = r.revision();
    let raw = fs::read(path(&d)).unwrap();
    assert_eq!(
        r.validate_io_approval(
            ID,
            p.digest(),
            &BTreeSet::from([IoCapability::HttpPublish]),
            rev
        ),
        Err(Error::Invalid("IO approval exceeds declaration"))
    );
    assert_eq!(
        r.approve_io(
            ID,
            p.digest(),
            BTreeSet::from([IoCapability::FileRead]),
            rev + 1
        ),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        r.approve_io(ID, [0; 32], BTreeSet::new(), rev),
        Err(Error::RevisionConflict)
    );
    assert_eq!(fs::read(path(&d)).unwrap(), raw);
    r.validate_io_approval(
        ID,
        p.digest(),
        &BTreeSet::from([IoCapability::FileRead]),
        rev,
    )
    .unwrap();
    assert_eq!(r.revision(), rev);
    r.approve_io(
        ID,
        p.digest(),
        BTreeSet::from([IoCapability::FileRead]),
        rev,
    )
    .unwrap();
    assert!(r.selection(ID).unwrap().approved.is_empty());
    let rev = r.revision();
    let raw = fs::read(path(&d)).unwrap();
    r.approve_io(
        ID,
        p.digest(),
        BTreeSet::from([IoCapability::FileRead]),
        rev,
    )
    .unwrap();
    assert_eq!(r.revision(), rev);
    assert_eq!(fs::read(path(&d)).unwrap(), raw);
    r.approve(
        ID,
        p.digest(),
        BTreeSet::from([GrantKind::ReadContent]),
        rev,
    )
    .unwrap();
    assert_eq!(
        r.selection(ID).unwrap().approved_io,
        BTreeSet::from([IoCapability::FileRead])
    );
    drop(r);
    assert_eq!(
        open(&d).unwrap().selection(ID).unwrap().approved_io,
        BTreeSet::from([IoCapability::FileRead])
    );
}
#[test]
fn upgrade_intersects_io_and_disable_remove_preserve_user_data() {
    let (d, mut r) = setup();
    let first = package(
        "1.0.0",
        vec![IoCapability::FileRead, IoCapability::HttpRequest],
    );
    let next = package(
        "2.0.0",
        vec![IoCapability::FileRead, IoCapability::HttpListen],
    );
    install(&d, &first);
    install(&d, &next);
    let user = d.path().join("user.txt");
    fs::write(&user, b"retained").unwrap();
    r.select(first.digest(), 0).unwrap();
    r.approve_io(
        ID,
        first.digest(),
        first.io_capabilities().clone(),
        r.revision(),
    )
    .unwrap();
    r.set_enabled(ID, first.digest(), true, r.revision())
        .unwrap();
    r.select(next.digest(), r.revision()).unwrap();
    let s = r.selection(ID).unwrap();
    assert!(!s.enabled);
    assert_eq!(s.approved_io, BTreeSet::from([IoCapability::FileRead]));
    assert_eq!(
        r.approve_io(ID, first.digest(), BTreeSet::new(), r.revision()),
        Err(Error::RevisionConflict)
    );
    r.set_enabled(ID, next.digest(), true, r.revision())
        .unwrap();
    r.set_enabled(ID, next.digest(), false, r.revision())
        .unwrap();
    assert_eq!(
        r.selection(ID).unwrap().approved_io,
        BTreeSet::from([IoCapability::FileRead])
    );
    r.remove(ID, r.revision()).unwrap();
    assert_eq!(fs::read(user).unwrap(), b"retained");
    Catalog::open(&d.path().join("packages"))
        .unwrap()
        .load(next.digest())
        .unwrap();
    r.select(next.digest(), r.revision()).unwrap();
    assert!(r.selection(ID).unwrap().approved_io.is_empty());
}
#[test]
fn legacy_with_io_unknown_duplicate_out_of_order_and_excess_approvals_are_rejected_without_rewrite()
{
    let (d, r) = setup();
    let p = package(
        "1.0.0",
        vec![IoCapability::FileRead, IoCapability::HttpRequest],
    );
    install(&d, &p);
    drop(r);
    legacy(&d, &p, 1);
    let original = read(&d);
    for (version, values) in [
        (1, vec![1]),
        (2, vec![0]),
        (2, vec![99]),
        (2, vec![1, 1]),
        (2, vec![6, 1]),
        (2, vec![1; 11]),
        (2, vec![9]),
    ] {
        let mut bad = original.clone();
        bad.schema_version = version;
        bad.selections[0].approved_io_capabilities = values;
        let bytes = store(&d, &bad);
        assert!(open(&d).is_err());
        assert_eq!(fs::read(path(&d)).unwrap(), bytes);
    }
}
#[test]
fn migration_revision_overflow_and_invalid_old_package_preserve_original_bytes() {
    let (d, r) = setup();
    let p = package("1.0.0", vec![IoCapability::FileRead]);
    install(&d, &p);
    drop(r);
    let bytes = legacy(&d, &p, u64::MAX);
    assert!(matches!(open(&d), Err(Error::Limit)));
    assert_eq!(fs::read(path(&d)).unwrap(), bytes);
    legacy(&d, &p, 7);
    let mut bad = read(&d);
    bad.selections[0].digest = vec![8; 32];
    let bytes = store(&d, &bad);
    assert!(open(&d).is_err());
    assert_eq!(fs::read(path(&d)).unwrap(), bytes);
}
#[cfg(windows)]
#[test]
fn migration_publication_failure_keeps_original_and_succeeds_after_delete_lock_released() {
    use std::os::windows::fs::OpenOptionsExt;
    let (d, r) = setup();
    let p = package("1.0.0", vec![IoCapability::FileRead]);
    install(&d, &p);
    drop(r);
    let bytes = legacy(&d, &p, 5);
    // An actual Windows handle permits reads but denies replacement of this test-owned file.
    let guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path(&d))
        .unwrap();
    assert!(matches!(open(&d), Err(Error::Io)));
    assert_eq!(fs::read(path(&d)).unwrap(), bytes);
    drop(guard);
    assert_eq!(open(&d).unwrap().revision(), 6);
    assert_eq!(read(&d).schema_version, 2);
}
#[cfg(windows)]
#[test]
fn io_approval_actual_publication_failure_preserves_memory_and_file() {
    use std::os::windows::fs::OpenOptionsExt;
    let (d, mut r) = setup();
    let p = package("1.0.0", vec![IoCapability::HttpRequest]);
    install(&d, &p);
    r.select(p.digest(), 0).unwrap();
    let rev = r.revision();
    let bytes = fs::read(path(&d)).unwrap();
    let guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path(&d))
        .unwrap();
    assert_eq!(
        r.approve_io(
            ID,
            p.digest(),
            BTreeSet::from([IoCapability::HttpRequest]),
            rev
        ),
        Err(Error::Io)
    );
    assert!(r.selection(ID).unwrap().approved_io.is_empty());
    assert_eq!(r.revision(), rev);
    assert_eq!(fs::read(path(&d)).unwrap(), bytes);
    drop(guard);
    r.approve_io(
        ID,
        p.digest(),
        BTreeSet::from([IoCapability::HttpRequest]),
        rev,
    )
    .unwrap();
}
#[test]
fn all_ten_io_categories_roundtrip_without_touching_content_namespace() {
    let caps = (1..=10)
        .map(|n| IoCapability::from_number(n).unwrap())
        .collect::<Vec<_>>();
    let (d, mut r) = setup();
    let p = package("1.0.0", caps.clone());
    install(&d, &p);
    r.select(p.digest(), 0).unwrap();
    r.approve_io(ID, p.digest(), caps.into_iter().collect(), r.revision())
        .unwrap();
    let state = read(&d);
    assert_eq!(
        state.selections[0].approved_io_capabilities,
        (1..=10).collect::<Vec<_>>()
    );
    assert!(state.selections[0].approved_capabilities.is_empty());
    drop(r);
    assert_eq!(
        open(&d).unwrap().selection(ID).unwrap().approved_io.len(),
        10
    );
}
#[test]
fn dependency_locks_never_inherit_caller_io_approval() {
    use morrow_core::plugin_package::{
        DEPENDENCIES_FEATURE,
        proto::{DependencyRequirement, TransformHandler},
    };
    let module = b"\0asm\x01\0\0\0";
    let build = |id: &str, dependencies: Vec<DependencyRequirement>| {
        let mut m = Package::manifest_for_transform(
            id,
            "1.0.0",
            module,
            vec![TransformHandler {
                handler: "convert".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 64,
                max_output_bytes: 64,
            }],
        );
        if id != "provider" {
            m.io_declaration = Some(io::declaration(
                vec![IoCapability::HttpRequest],
                vec!["api.invoke".into()],
            ));
            m.required_features.push(io::FEATURE.into());
        }
        if !dependencies.is_empty() {
            m.required_features.push(DEPENDENCIES_FEATURE.into());
        }
        m.dependencies = dependencies;
        Package::build(m, module).unwrap()
    };
    let caller = build(
        "caller",
        vec![DependencyRequirement {
            slot: "worker".into(),
            handler: "convert".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            provider_version: "^1.0.0".into(),
            optional: false,
        }],
    );
    let provider = build("provider", vec![]);
    let (d, mut r) = setup();
    for p in [&caller, &provider] {
        install(&d, p);
        r.select(p.digest(), r.revision()).unwrap();
        r.set_enabled(&p.manifest().package_id, p.digest(), true, r.revision())
            .unwrap();
    }
    r.approve_io(
        "caller",
        caller.digest(),
        BTreeSet::from([IoCapability::HttpRequest]),
        r.revision(),
    )
    .unwrap();
    let io_provider = build("io-provider", vec![]);
    install(&d, &io_provider);
    r.select(io_provider.digest(), r.revision()).unwrap();
    let before = fs::read(path(&d)).unwrap();
    assert_eq!(
        r.approve_dependency(
            "caller",
            caller.digest(),
            "worker",
            "io-provider",
            io_provider.digest(),
            r.revision()
        ),
        Err(Error::Invalid("IO package requires IO execution"))
    );
    assert_eq!(fs::read(path(&d)).unwrap(), before);
    r.approve_dependency(
        "caller",
        caller.digest(),
        "worker",
        "provider",
        provider.digest(),
        r.revision(),
    )
    .unwrap();
    let (_, _, selected_provider) = r.resolve_dependency("caller", "worker").unwrap();
    assert!(selected_provider.approved_io.is_empty());
    assert_eq!(
        r.selection("caller").unwrap().approved_io,
        BTreeSet::from([IoCapability::HttpRequest])
    );
    drop(r);
    let r = open(&d).unwrap();
    assert!(
        r.resolve_dependency("caller", "worker")
            .unwrap()
            .2
            .approved_io
            .is_empty()
    );
}
