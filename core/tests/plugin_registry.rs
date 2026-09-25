#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    lifecycle::GrantKind,
    plugin_package::{Package, catalog::Catalog, proto::Capability, registry::Registry},
};
use std::{collections::BTreeSet, fs};
use tempfile::TempDir;
fn package(id: &str, version: &str, caps: Vec<Capability>) -> Package {
    let wasm = b"\0asm\x01\0\0\0";
    Package::build(Package::manifest_for(id, version, wasm, caps), wasm).unwrap()
}
fn setup() -> (TempDir, Registry) {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(&dir.path().join("packages")).unwrap();
    let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
    (dir, registry)
}
fn install(dir: &TempDir, package: &Package) -> std::path::PathBuf {
    Catalog::open(&dir.path().join("packages"))
        .unwrap()
        .install(package)
        .unwrap()
}
fn reopen(dir: &TempDir) -> morrow_core::Result<Registry> {
    Registry::open(
        &dir.path().join("registry"),
        Catalog::open(&dir.path().join("packages"))?,
    )
}
#[test]
fn theme_slot_switch_is_one_snapshot_and_keeps_business_approvals() {
    use morrow_core::plugin_package::proto::TransformHandler;
    let (dir, mut r) = setup();
    let theme = |id: &str| {
        let wasm = b"\0asm\x01\0\0\0";
        let m = Package::manifest_for_transform(id, "1.0.0", wasm, vec![TransformHandler {
            handler: "theme.describe".into(), input_type: "morrow.ui.theme.request.v1".into(),
            output_type: "morrow.ui.theme.v1".into(), max_input_bytes: 1, max_output_bytes: 16384,
        }]);
        Package::build(m, wasm).unwrap()
    };
    let first = theme("org.example.theme.first");
    let second = theme("org.example.theme.second");
    let business = package("org.example.business", "1.0.0", vec![Capability::ReadContent]);
    for p in [&first, &second, &business] {
        install(&dir, p);
        r.select(p.digest(), r.revision()).unwrap();
    }
    let biz = business.manifest().package_id.as_str();
    r.approve(biz, business.digest(), BTreeSet::from([GrantKind::ReadContent]), r.revision()).unwrap();
    r.set_enabled(biz, business.digest(), true, r.revision()).unwrap();
    let business_before = r.selection(biz).unwrap().clone();
    let a = first.manifest().package_id.as_str();
    let b = second.manifest().package_id.as_str();
    r.set_enabled(a, first.digest(), true, r.revision()).unwrap();
    let revision = r.revision();
    assert!(r.set_enabled(b, second.digest(), true, revision - 1).is_err());
    assert!(r.selection(a).unwrap().enabled);
    r.set_enabled(b, second.digest(), true, revision).unwrap();
    assert_eq!(r.revision(), revision + 1);
    assert!(!r.selection(a).unwrap().enabled);
    assert!(r.selection(b).unwrap().enabled);
    assert_eq!(r.selection(biz).unwrap(), &business_before);
    drop(r);
    let r = reopen(&dir).unwrap();
    assert!(!r.selection(a).unwrap().enabled);
    assert!(r.selection(b).unwrap().enabled);
    assert_eq!(r.selection(biz).unwrap(), &business_before);
}
#[test]
fn persistence_upgrade_requires_enable_and_never_expands_approval() {
    let (dir, mut registry) = setup();
    let first = package(
        "org.example.test",
        "1.0.0",
        vec![Capability::RenameCard, Capability::ReadSummary],
    );
    let second = package(
        "org.example.test",
        "2.0.0",
        vec![Capability::RenameCard, Capability::ReadAttachment],
    );
    install(&dir, &first);
    install(&dir, &second);
    registry
        .select(first.digest(), registry.revision())
        .unwrap();
    assert_eq!(registry.revision(), 1);
    assert!(
        registry
            .selection("org.example.test")
            .unwrap()
            .approved
            .is_empty()
    );
    assert!(registry.resolve_enabled("org.example.test").is_err());
    registry
        .approve(
            "org.example.test",
            first.digest(),
            BTreeSet::from([GrantKind::Rename, GrantKind::ReadSummary]),
            registry.revision(),
        )
        .unwrap();
    registry
        .set_enabled(
            "org.example.test",
            first.digest(),
            true,
            registry.revision(),
        )
        .unwrap();
    let rev = registry.revision();
    registry
        .select(first.digest(), registry.revision())
        .unwrap();
    assert_eq!(registry.revision(), rev);
    registry
        .select(second.digest(), registry.revision())
        .unwrap();
    let choice = registry.selection("org.example.test").unwrap();
    assert!(!choice.enabled);
    assert_eq!(choice.approved, BTreeSet::from([GrantKind::Rename]));
    registry
        .set_enabled(
            "org.example.test",
            second.digest(),
            true,
            registry.revision(),
        )
        .unwrap();
    let rev = registry.revision();
    drop(registry);
    let registry = reopen(&dir).unwrap();
    let (loaded, choice) = registry.resolve_enabled("org.example.test").unwrap();
    assert_eq!(loaded.digest(), second.digest());
    assert_eq!(choice.approved, BTreeSet::from([GrantKind::Rename]));
    assert_eq!(registry.revision(), rev);
}
#[test]
fn invalid_upgrade_and_approval_preserve_persisted_selection() {
    let (dir, mut registry) = setup();
    let first = package("org.example.test", "2.0.0", vec![Capability::RenameCard]);
    let older = package("org.example.test", "1.0.0", vec![]);
    let same_version = package("org.example.test", "2.0.0", vec![]);
    install(&dir, &first);
    install(&dir, &older);
    install(&dir, &same_version);
    registry
        .select(first.digest(), registry.revision())
        .unwrap();
    let path = dir.path().join("registry/selection.morrow");
    let bytes = fs::read(&path).unwrap();
    assert_eq!(
        registry.select(older.digest(), registry.revision()),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        registry.select(same_version.digest(), registry.revision()),
        Err(Error::RevisionConflict)
    );
    assert!(registry.select([99; 32], registry.revision()).is_err());
    assert_eq!(
        registry.approve(
            "org.example.test",
            first.digest(),
            BTreeSet::from([GrantKind::ReadAttachment]),
            registry.revision()
        ),
        Err(Error::Invalid("approval exceeds declaration"))
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(
        registry.selection("org.example.test").unwrap().digest,
        first.digest()
    );
}
#[test]
fn disable_remove_keep_package_and_content_and_reset_approvals() {
    let (dir, mut registry) = setup();
    let first = package("org.example.test", "1.0.0", vec![Capability::RenameCard]);
    let package_path = install(&dir, &first);
    let user_content = dir.path().join("user-content");
    fs::write(&user_content, b"preserved user data").unwrap();
    registry
        .select(first.digest(), registry.revision())
        .unwrap();
    registry
        .approve(
            "org.example.test",
            first.digest(),
            BTreeSet::from([GrantKind::Rename]),
            registry.revision(),
        )
        .unwrap();
    registry
        .set_enabled(
            "org.example.test",
            first.digest(),
            true,
            registry.revision(),
        )
        .unwrap();
    registry
        .set_enabled(
            "org.example.test",
            first.digest(),
            false,
            registry.revision(),
        )
        .unwrap();
    assert!(registry.resolve_enabled("org.example.test").is_err());
    registry
        .remove("org.example.test", registry.revision())
        .unwrap();
    assert!(registry.selection("org.example.test").is_none());
    assert_eq!(fs::read(package_path).unwrap(), first.archive());
    assert_eq!(fs::read(user_content).unwrap(), b"preserved user data");
    registry
        .select(first.digest(), registry.revision())
        .unwrap();
    assert!(!registry.selection("org.example.test").unwrap().enabled);
    assert!(
        registry
            .selection("org.example.test")
            .unwrap()
            .approved
            .is_empty()
    );
}
#[test]
fn one_cooperative_manager_per_directory_and_reopen_after_drop() {
    let (dir, registry) = setup();
    assert!(matches!(reopen(&dir), Err(Error::StorageBusy)));
    drop(registry);
    assert!(reopen(&dir).is_ok());
}
#[test]
fn corrupt_selected_package_refuses_resolution_and_reopen() {
    let (dir, mut registry) = setup();
    let first = package("org.example.test", "1.0.0", vec![]);
    let path = install(&dir, &first);
    registry
        .select(first.digest(), registry.revision())
        .unwrap();
    registry
        .set_enabled(
            "org.example.test",
            first.digest(),
            true,
            registry.revision(),
        )
        .unwrap();
    fs::write(path, b"corrupt package").unwrap();
    assert!(registry.resolve_enabled("org.example.test").is_err());
    drop(registry);
    assert!(reopen(&dir).is_err());
}
#[test]
fn failed_publication_preserves_revision_and_selection() {
    let (dir, mut registry) = setup();
    let first = package("org.example.test", "1.0.0", vec![]);
    install(&dir, &first);
    registry
        .select(first.digest(), registry.revision())
        .unwrap();
    let revision = registry.revision();
    let path = dir.path().join("registry/selection.morrow");
    let saved = dir.path().join("registry/saved");
    fs::rename(&path, &saved).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(
        registry
            .set_enabled(
                "org.example.test",
                first.digest(),
                true,
                registry.revision()
            )
            .is_err()
    );
    assert_eq!(registry.revision(), revision);
    assert!(!registry.selection("org.example.test").unwrap().enabled);
    fs::remove_dir(&path).unwrap();
    fs::rename(saved, &path).unwrap();
    drop(registry);
    assert!(
        !reopen(&dir)
            .unwrap()
            .selection("org.example.test")
            .unwrap()
            .enabled
    );
}
#[test]
fn corrupt_and_oversized_state_fail_closed_without_overwrite() {
    let (dir, registry) = setup();
    drop(registry);
    let path = dir.path().join("registry/selection.morrow");
    fs::write(&path, b"broken").unwrap();
    assert!(reopen(&dir).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"broken");
    let file = fs::File::create(&path).unwrap();
    file.set_len(2 * 1024 * 1024).unwrap();
    drop(file);
    assert!(matches!(reopen(&dir), Err(Error::Limit)));
    assert_eq!(fs::metadata(path).unwrap().len(), 2 * 1024 * 1024);
}

#[test]
fn build_metadata_does_not_make_an_upgrade() {
    let (dir, mut registry) = setup();
    let first = package("org.example.test", "1.0.0+aaa", vec![]);
    let metadata_only = package("org.example.test", "1.0.0+bbb", vec![]);
    install(&dir, &first);
    install(&dir, &metadata_only);
    registry
        .select(first.digest(), registry.revision())
        .unwrap();
    assert_eq!(
        registry.select(metadata_only.digest(), registry.revision()),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        registry.selection("org.example.test").unwrap().digest,
        first.digest()
    );
}

#[test]
fn stale_approval_and_enable_cannot_confirm_another_digest() {
    let (dir, mut registry) = setup();
    let first = package("org.example.test", "1.0.0", vec![Capability::RenameCard]);
    let second = package("org.example.test", "2.0.0", vec![Capability::RenameCard]);
    install(&dir, &first);
    install(&dir, &second);
    registry
        .select(first.digest(), registry.revision())
        .unwrap();
    registry
        .select(second.digest(), registry.revision())
        .unwrap();
    let revision = registry.revision();
    assert_eq!(
        registry.approve(
            "org.example.test",
            first.digest(),
            BTreeSet::from([GrantKind::Rename]),
            registry.revision()
        ),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        registry.set_enabled(
            "org.example.test",
            first.digest(),
            true,
            registry.revision()
        ),
        Err(Error::RevisionConflict)
    );
    assert_eq!(registry.revision(), revision);
    let selection = registry.selection("org.example.test").unwrap();
    assert!(!selection.enabled);
    assert!(selection.approved.is_empty());
}

#[test]
fn stale_revision_cannot_revive_revoked_approval_or_remove_new_selection() {
    let (dir, mut registry) = setup();
    let first = package(
        "org.example.test",
        "1.0.0",
        vec![Capability::RenameCard, Capability::ReadSummary],
    );
    let second = package("org.example.test", "2.0.0", vec![Capability::RenameCard]);
    install(&dir, &first);
    install(&dir, &second);
    registry
        .select(first.digest(), registry.revision())
        .unwrap();
    registry
        .approve(
            "org.example.test",
            first.digest(),
            BTreeSet::from([GrantKind::Rename, GrantKind::ReadSummary]),
            registry.revision(),
        )
        .unwrap();
    let stale = registry.revision();
    registry
        .approve(
            "org.example.test",
            first.digest(),
            BTreeSet::from([GrantKind::ReadSummary]),
            registry.revision(),
        )
        .unwrap();
    let narrowed = registry.revision();
    assert_eq!(
        registry.approve(
            "org.example.test",
            first.digest(),
            BTreeSet::from([GrantKind::Rename, GrantKind::ReadSummary]),
            stale
        ),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        registry.set_enabled("org.example.test", first.digest(), true, stale),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        registry.select(second.digest(), stale),
        Err(Error::RevisionConflict)
    );
    assert_eq!(registry.revision(), narrowed);
    assert_eq!(
        registry.selection("org.example.test").unwrap().approved,
        BTreeSet::from([GrantKind::ReadSummary])
    );
    registry.select(second.digest(), narrowed).unwrap();
    assert_eq!(
        registry.remove("org.example.test", narrowed),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        registry.selection("org.example.test").unwrap().digest,
        second.digest()
    );
    assert!(!registry.selection("org.example.test").unwrap().enabled);
}
