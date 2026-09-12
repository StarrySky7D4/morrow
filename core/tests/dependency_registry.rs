#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    plugin_package::{
        DEPENDENCIES_FEATURE, Package,
        catalog::Catalog,
        proto::{DependencyRequirement, TransformHandler},
        registry::{MAX_DEPENDENCY_LOCKS, Registry},
    },
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
use tempfile::TempDir;
mod wire {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.registry.v1.rs"));
}
const MODULE: &[u8] = b"\0asm\x01\0\0\0";
fn dep(slot: &str, optional: bool) -> DependencyRequirement {
    DependencyRequirement {
        slot: slot.into(),
        handler: "convert".into(),
        input_type: "bytes".into(),
        output_type: "bytes".into(),
        provider_version: "^1.0.0".into(),
        optional,
    }
}
fn package(id: &str, version: &str, dependencies: Vec<DependencyRequirement>) -> Package {
    let mut m = Package::manifest_for_transform(
        id,
        version,
        MODULE,
        vec![TransformHandler {
            handler: "convert".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    if !dependencies.is_empty() {
        m.required_features.push(DEPENDENCIES_FEATURE.into());
    }
    m.dependencies = dependencies;
    Package::build(m, MODULE).unwrap()
}
struct Fixture {
    dir: TempDir,
    registry: Registry,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let registry = Self::open(&dir).unwrap();
        Self { dir, registry }
    }
    fn open(dir: &TempDir) -> morrow_core::Result<Registry> {
        Registry::open(
            &dir.path().join("registry"),
            Catalog::open(&dir.path().join("packages"))?,
        )
    }
    fn path(&self) -> PathBuf {
        self.dir.path().join("registry/selection.morrow")
    }
    fn install(&mut self, package: &Package) {
        Catalog::open(&self.dir.path().join("packages"))
            .unwrap()
            .install(package)
            .unwrap();
        self.registry
            .select(package.digest(), self.registry.revision())
            .unwrap();
    }
    fn enable(&mut self, id: &str, enabled: bool) {
        let digest = self.registry.selection(id).unwrap().digest;
        self.registry
            .set_enabled(id, digest, enabled, self.registry.revision())
            .unwrap();
    }
    fn approve(&mut self, caller: &str, slot: &str, provider: &str) -> morrow_core::Result<()> {
        self.registry.approve_dependency(
            caller,
            self.registry.selection(caller).unwrap().digest,
            slot,
            provider,
            self.registry.selection(provider).unwrap().digest,
            self.registry.revision(),
        )
    }
}
#[test]
fn incomplete_required_configuration_can_reopen_but_cannot_resolve() {
    let mut f = Fixture::new();
    let a = package("a", "1.0.0", vec![dep("worker", false)]);
    let b = package("b", "1.0.0", vec![]);
    f.install(&a);
    f.install(&b);
    f.enable("a", true);
    assert!(f.registry.resolve_enabled("a").is_err());
    let Fixture { dir, registry } = f;
    let revision = registry.revision();
    drop(registry);
    let registry = Fixture::open(&dir).unwrap();
    assert_eq!(registry.revision(), revision);
    let mut f = Fixture { dir, registry };
    f.approve("a", "worker", "b").unwrap(); // Disabled provider may be approved.
    assert!(f.registry.resolve_enabled("a").is_err());
    assert!(f.registry.resolve_dependency("a", "worker").is_err());
    f.enable("b", true);
    let (lock, resolved, selection) = f.registry.resolve_dependency("a", "worker").unwrap();
    assert_eq!(lock.caller_digest, a.digest());
    assert_eq!(lock.provider_digest, b.digest());
    assert_eq!(resolved.digest(), b.digest());
    assert!(selection.approved.is_empty());
    let Fixture { dir, registry } = f;
    let revision = registry.revision();
    drop(registry);
    let registry = Fixture::open(&dir).unwrap();
    assert_eq!(registry.revision(), revision);
    assert_eq!(registry.dependency("a", "worker"), Some(&lock));
    assert!(registry.resolve_enabled("a").is_ok());
    assert!(matches!(Fixture::open(&dir), Err(Error::StorageBusy)));
}
#[test]
fn approval_checks_exact_digests_slot_contract_and_version_without_mutation() {
    let mut f = Fixture::new();
    let a = package("a", "1.0.0", vec![dep("worker", false)]);
    let b = package("b", "1.0.0", vec![]);
    let too_new = package("new", "2.0.0", vec![]);
    let mut m = b.manifest().clone();
    m.package_id = "wrong".into();
    m.transform_handlers[0].output_type = "wrong".into();
    let wrong = Package::build(m, MODULE).unwrap();
    for p in [&a, &b, &too_new, &wrong] {
        f.install(p);
    }
    let old = fs::read(f.path()).unwrap();
    let rev = f.registry.revision();
    assert_eq!(
        f.registry
            .approve_dependency("a", [0; 32], "worker", "b", b.digest(), rev),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        f.registry
            .approve_dependency("a", a.digest(), "worker", "b", [0; 32], rev),
        Err(Error::RevisionConflict)
    );
    for (slot, provider) in [("absent", "b"), ("worker", "new"), ("worker", "wrong")] {
        assert!(f.approve("a", slot, provider).is_err());
    }
    assert_eq!(f.registry.revision(), rev);
    assert_eq!(fs::read(f.path()).unwrap(), old);
    assert!(f.registry.dependency("a", "worker").is_none());
}
#[test]
fn stale_cas_cannot_restore_removed_lock_or_remove_replaced_lock() {
    let mut f = Fixture::new();
    for p in [
        package("a", "1.0.0", vec![dep("worker", false)]),
        package("b", "1.0.0", vec![]),
        package("c", "1.0.0", vec![]),
    ] {
        f.install(&p);
    }
    f.approve("a", "worker", "b").unwrap();
    let stale = f.registry.revision();
    f.registry.remove_dependency("a", "worker", stale).unwrap();
    let a = f.registry.selection("a").unwrap().digest;
    let b = f.registry.selection("b").unwrap().digest;
    assert_eq!(
        f.registry
            .approve_dependency("a", a, "worker", "b", b, stale),
        Err(Error::RevisionConflict)
    );
    f.approve("a", "worker", "c").unwrap();
    assert_eq!(
        f.registry.remove_dependency("a", "worker", stale),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        f.registry.dependency("a", "worker").unwrap().provider_id,
        "c"
    );
    let rev = f.registry.revision();
    f.approve("a", "worker", "c").unwrap();
    assert_eq!(f.registry.revision(), rev); // Exact repeat is a no-op after CAS.
}
#[test]
fn required_cycles_reject_atomically_optional_and_mixed_cycles_do_not_recurse() {
    for (a_optional, b_optional) in [(false, false), (true, true), (true, false)] {
        let mut f = Fixture::new();
        for p in [
            package("a", "1.0.0", vec![dep("worker", a_optional)]),
            package("b", "1.0.0", vec![dep("worker", b_optional)]),
        ] {
            f.install(&p);
        }
        f.enable("a", true);
        f.enable("b", true);
        f.approve("a", "worker", "b").unwrap();
        let before = fs::read(f.path()).unwrap();
        let rev = f.registry.revision();
        let result = f.approve("b", "worker", "a");
        if !a_optional && !b_optional {
            assert_eq!(result, Err(Error::Invalid("required dependency cycle")));
            assert_eq!(f.registry.revision(), rev);
            assert_eq!(fs::read(f.path()).unwrap(), before);
        } else {
            result.unwrap();
            assert!(f.registry.resolve_dependency("a", "worker").is_ok());
            assert!(f.registry.resolve_dependency("b", "worker").is_ok());
        }
    }
    let mut f = Fixture::new();
    f.install(&package("self", "1.0.0", vec![dep("worker", false)]));
    assert!(f.approve("self", "worker", "self").is_err());
}
#[test]
fn transitive_required_consumers_are_sorted_and_optional_failure_does_not_block_start() {
    let mut f = Fixture::new();
    for p in [
        package("a", "1.0.0", vec![dep("worker", false)]),
        package("b", "1.0.0", vec![dep("worker", false)]),
        package("c", "1.0.0", vec![]),
        package("optional", "1.0.0", vec![dep("worker", true)]),
    ] {
        f.install(&p);
    }
    for id in ["a", "b", "c", "optional"] {
        f.enable(id, true);
    }
    assert!(f.registry.resolve_enabled("optional").is_ok());
    assert!(f.registry.resolve_dependency("optional", "worker").is_err());
    f.approve("a", "worker", "b").unwrap();
    assert!(f.registry.resolve_enabled("a").is_err());
    f.approve("b", "worker", "c").unwrap();
    f.approve("optional", "worker", "c").unwrap();
    assert_eq!(f.registry.required_dependents("c"), vec!["a", "b"]);
    assert_eq!(f.registry.required_dependents("b"), vec!["a"]);
    assert!(f.registry.resolve_enabled("a").is_ok());
    f.enable("c", false);
    assert!(f.registry.dependency("b", "worker").is_some());
    assert!(f.registry.resolve_enabled("a").is_err());
    assert!(f.registry.resolve_enabled("optional").is_ok());
    assert!(f.registry.resolve_dependency("optional", "worker").is_err());
    f.enable("c", true);
    assert!(f.registry.resolve_enabled("a").is_ok());
    f.enable("a", false);
    assert!(f.registry.resolve_dependency("a", "worker").is_err());
}
#[test]
fn upgrade_and_remove_clear_all_incident_locks_in_same_snapshot() {
    for upgrade in [false, true] {
        let mut f = Fixture::new();
        for p in [
            package("a", "1.0.0", vec![dep("worker", false)]),
            package("b", "1.0.0", vec![dep("worker", false)]),
            package("c", "1.0.0", vec![]),
        ] {
            f.install(&p);
        }
        f.approve("a", "worker", "b").unwrap();
        f.approve("b", "worker", "c").unwrap();
        if upgrade {
            f.install(&package("b", "1.1.0", vec![dep("worker", false)]));
        } else {
            f.registry.remove("b", f.registry.revision()).unwrap();
        }
        assert!(f.registry.dependency("a", "worker").is_none());
        assert!(f.registry.dependency("b", "worker").is_none());
        assert!(f.registry.required_dependents("c").is_empty());
        let Fixture { dir, registry } = f;
        drop(registry);
        let registry = Fixture::open(&dir).unwrap();
        assert!(registry.dependency("a", "worker").is_none());
        assert!(registry.dependency("b", "worker").is_none());
        assert_eq!(registry.selection("b").is_some(), upgrade);
    }
}
#[test]
fn failed_save_preserves_selections_locks_and_revision_together() {
    let mut f = Fixture::new();
    let next = package("b", "1.1.0", vec![]);
    for p in [
        package("a", "1.0.0", vec![dep("worker", false)]),
        package("b", "1.0.0", vec![]),
        package("c", "1.0.0", vec![]),
    ] {
        f.install(&p);
    }
    Catalog::open(&f.dir.path().join("packages"))
        .unwrap()
        .install(&next)
        .unwrap();
    f.approve("a", "worker", "b").unwrap();
    let previous = f.registry.dependency("a", "worker").unwrap().clone();
    let rev = f.registry.revision();
    let original = fs::read(f.path()).unwrap();
    let saved = f.dir.path().join("registry/saved");
    fs::rename(f.path(), &saved).unwrap();
    fs::create_dir(f.path()).unwrap();
    assert!(f.approve("a", "worker", "c").is_err());
    assert!(f.registry.remove_dependency("a", "worker", rev).is_err());
    assert!(f.registry.select(next.digest(), rev).is_err());
    assert!(f.registry.remove("b", rev).is_err());
    assert_eq!(f.registry.revision(), rev);
    assert_eq!(f.registry.dependency("a", "worker"), Some(&previous));
    assert_eq!(
        f.registry.selection("b").unwrap().digest,
        previous.provider_digest
    );
    fs::remove_dir(f.path()).unwrap();
    fs::rename(saved, f.path()).unwrap();
    assert_eq!(fs::read(f.path()).unwrap(), original);
    let Fixture { dir, registry } = f;
    drop(registry);
    assert_eq!(
        Fixture::open(&dir).unwrap().dependency("a", "worker"),
        Some(&previous)
    );
}
fn container(raw: &[u8]) -> Vec<u8> {
    let packed = lz4_flex::block::compress(raw);
    let mut bytes = b"MORROWG1".to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend(packed);
    bytes
}
fn raw(bytes: &[u8]) -> Vec<u8> {
    lz4_flex::block::decompress(
        &bytes[50..],
        u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize,
    )
    .unwrap()
}
#[test]
fn old_snapshot_without_locks_remains_readable() {
    let f = Fixture::new();
    // Exact former schema bytes: version 1, revision 1; omitted empty repeated fields.
    fs::write(f.path(), container(&[8, 1, 16, 1])).unwrap();
    let Fixture { dir, registry } = f;
    drop(registry);
    let registry = Fixture::open(&dir).unwrap();
    assert_eq!(registry.revision(), 1);
    assert!(registry.selections().next().is_none());
    assert!(registry.dependency("absent", "absent").is_none());
}
#[test]
fn canonical_lock_snapshot_rejects_unknown_tail_hash_duplicates_and_stale_identity() {
    let mut f = Fixture::new();
    for p in [
        package("a", "1.0.0", vec![dep("z", false), dep("a", true)]),
        package("b", "1.0.0", vec![]),
    ] {
        f.install(&p);
    }
    f.approve("a", "z", "b").unwrap();
    f.approve("a", "a", "b").unwrap();
    let good = fs::read(f.path()).unwrap();
    let state = wire::Registry::decode(raw(&good).as_slice()).unwrap();
    assert_eq!(
        state
            .dependency_locks
            .iter()
            .map(|l| l.slot.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "z"]
    );
    let mut variants = vec![];
    let mut unknown = raw(&good);
    unknown.extend_from_slice(&[0x28, 1]);
    variants.push(container(&unknown));
    let mut tail = good.clone();
    tail.push(0);
    variants.push(tail);
    let mut hash = good.clone();
    hash[18] ^= 1;
    variants.push(hash);
    let mut duplicate = state.clone();
    duplicate
        .dependency_locks
        .push(duplicate.dependency_locks[0].clone());
    variants.push(container(&duplicate.encode_to_vec()));
    let mut unsorted = state.clone();
    unsorted.dependency_locks.reverse();
    variants.push(container(&unsorted.encode_to_vec()));
    let mut short = state.clone();
    short.dependency_locks[0].caller_digest.pop();
    variants.push(container(&short.encode_to_vec()));
    let mut stale = state.clone();
    stale.dependency_locks[0].provider_digest[0] ^= 1;
    variants.push(container(&stale.encode_to_vec()));
    let mut absent = state.clone();
    absent.dependency_locks[0].slot = "absent".into();
    variants.push(container(&absent.encode_to_vec()));
    let mut over = state.clone();
    over.dependency_locks
        .resize(MAX_DEPENDENCY_LOCKS + 1, state.dependency_locks[0].clone());
    variants.push(container(&over.encode_to_vec()));
    // Unknown nested lock field must not be discarded by a rewrite either.
    let mut nested = state.clone();
    nested.dependency_locks.clear();
    let mut raw_nested = nested.encode_to_vec();
    let mut lock = state.dependency_locks[0].encode_to_vec();
    lock.extend_from_slice(&[0x30, 1]);
    assert!(lock.len() < 128);
    raw_nested.push(0x22);
    raw_nested.push(lock.len() as u8);
    raw_nested.extend(lock);
    variants.push(container(&raw_nested));
    let Fixture { dir, registry } = f;
    drop(registry);
    let path = dir.path().join("registry/selection.morrow");
    for bytes in variants {
        fs::write(&path, &bytes).unwrap();
        assert!(Fixture::open(&dir).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::write(path, good).unwrap();
    assert!(Fixture::open(&dir).is_ok());
}
#[test]
fn forged_required_cycle_refuses_open_and_corrupt_catalog_does_not_hide_dependents() {
    let mut f = Fixture::new();
    for p in [
        package("a", "1.0.0", vec![dep("worker", false)]),
        package("b", "1.0.0", vec![dep("worker", false)]),
    ] {
        f.install(&p);
    }
    f.approve("a", "worker", "b").unwrap();
    let good = fs::read(f.path()).unwrap();
    let mut state = wire::Registry::decode(raw(&good).as_slice()).unwrap();
    state.dependency_locks.push(wire::LockedDependency {
        caller_id: "b".into(),
        caller_digest: f.registry.selection("b").unwrap().digest.to_vec(),
        slot: "worker".into(),
        provider_id: "a".into(),
        provider_digest: f.registry.selection("a").unwrap().digest.to_vec(),
    });
    let forged = container(&state.encode_to_vec());
    let Fixture { dir, registry } = f;
    drop(registry);
    let path = dir.path().join("registry/selection.morrow");
    fs::write(&path, &forged).unwrap();
    assert!(matches!(
        Fixture::open(&dir),
        Err(Error::Invalid("required dependency cycle"))
    ));
    assert_eq!(fs::read(&path).unwrap(), forged);
    fs::write(path, good).unwrap();
    let registry = Fixture::open(&dir).unwrap();
    let catalog = Catalog::open(&dir.path().join("packages")).unwrap();
    let caller = catalog
        .load(registry.selection("a").unwrap().digest)
        .unwrap();
    let archive = catalog.install(&caller).unwrap();
    fs::write(archive, b"corrupt").unwrap();
    assert_eq!(registry.required_dependents("b"), vec!["a"]);
    assert!(registry.resolve_enabled("a").is_err());
}

#[test]
fn maximum_lock_snapshot_opens_and_one_more_approval_preserves_it() {
    let f = Fixture::new();
    let provider = package("provider", "1.0.0", vec![]);
    let catalog = Catalog::open(&f.dir.path().join("packages")).unwrap();
    catalog.install(&provider).unwrap();
    let mut state = wire::Registry {
        schema_version: 1,
        revision: 1,
        selections: vec![],
        dependency_locks: vec![],
    };
    // 64 callers x 16 declared slots exercise the actual 1024-lock upper boundary.
    for index in 0..65 {
        let id = format!("caller-{index:03}");
        let requirements = (0..16)
            .map(|n| dep(&format!("slot-{n:02}"), true))
            .collect();
        let caller = package(&id, "1.0.0", requirements);
        catalog.install(&caller).unwrap();
        state.selections.push(wire::Selection {
            package_id: id.clone(),
            digest: caller.digest().to_vec(),
            enabled: true,
            approved_capabilities: vec![],
        });
        if index < 64 {
            for n in 0..16 {
                state.dependency_locks.push(wire::LockedDependency {
                    caller_id: id.clone(),
                    caller_digest: caller.digest().to_vec(),
                    slot: format!("slot-{n:02}"),
                    provider_id: "provider".into(),
                    provider_digest: provider.digest().to_vec(),
                });
            }
        }
    }
    state.selections.push(wire::Selection {
        package_id: "provider".into(),
        digest: provider.digest().to_vec(),
        enabled: true,
        approved_capabilities: vec![],
    });
    assert_eq!(state.dependency_locks.len(), MAX_DEPENDENCY_LOCKS);
    let saved = container(&state.encode_to_vec());
    fs::write(f.path(), &saved).unwrap();
    let Fixture { dir, registry } = f;
    drop(registry);
    let mut f = Fixture {
        registry: Fixture::open(&dir).unwrap(),
        dir,
    };
    assert!(
        f.registry
            .resolve_dependency("caller-063", "slot-15")
            .is_ok()
    );
    assert_eq!(
        f.approve("caller-064", "slot-00", "provider"),
        Err(Error::Limit)
    );
    assert_eq!(f.registry.revision(), 1);
    assert!(f.registry.dependency("caller-064", "slot-00").is_none());
    assert_eq!(fs::read(f.path()).unwrap(), saved);
}

#[test]
fn shared_required_subgraph_and_unresolvable_optional_provider_are_distinct() {
    let mut f = Fixture::new();
    for p in [
        package("a", "1.0.0", vec![dep("left", false), dep("right", false)]),
        package("b", "1.0.0", vec![dep("worker", false)]),
        package("c", "1.0.0", vec![dep("worker", false)]),
        package("d", "1.0.0", vec![]),
        package("optional", "1.0.0", vec![dep("worker", true)]),
    ] {
        f.install(&p);
    }
    for id in ["a", "b", "c", "d", "optional"] {
        f.enable(id, true);
    }
    f.approve("optional", "worker", "b").unwrap();
    assert!(f.registry.resolve_enabled("optional").is_ok());
    assert!(f.registry.resolve_dependency("optional", "worker").is_err());
    f.approve("a", "left", "b").unwrap();
    f.approve("a", "right", "c").unwrap();
    f.approve("b", "worker", "d").unwrap();
    f.approve("c", "worker", "d").unwrap();
    assert!(f.registry.resolve_enabled("a").is_ok());
    assert_eq!(f.registry.required_dependents("d"), vec!["a", "b", "c"]);
}
