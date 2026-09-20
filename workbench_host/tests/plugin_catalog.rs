#![cfg(target_os = "windows")]
use morrow_core::{
    plugin_package::{Package, catalog},
    ui::{Document, Event, EventKind},
};
use morrow_plugin_runtime::inline_ui::Reply;
use morrow_workbench_host::{Workbench, plugin_catalog::PluginEntry};
use std::path::{Path, PathBuf};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}
fn fixture(stem: &str) -> PathBuf {
    root().join(format!("sdk/compat/guest-v1-rc1/{stem}.mplugin"))
}
fn bundle() -> Package {
    catalog::read_file(&root().join("build/workbench-host/bundle/workbench.morrowplugin")).unwrap()
}
fn revision(w: &Workbench) -> u64 {
    w.catalog_page("", None).unwrap().revision
}
fn entry(w: &Workbench, id: &str) -> PluginEntry {
    let mut page = w.catalog_page("", None).unwrap();
    loop {
        if let Some(index) = page.entries.iter().position(|e| e.id == id) {
            return page.entries.remove(index);
        }
        assert!(!page.cursor.is_empty());
        page = w.catalog_page(&page.cursor, Some(page.revision)).unwrap();
    }
}
fn import(w: &mut Workbench, path: &Path) -> PluginEntry {
    let page = w.inspect_plugin(path).unwrap();
    let e = &page.entries[0];
    w.import_plugin(path, &e.digest, page.revision).unwrap();
    entry(w, &e.id)
}
fn enable(w: &mut Workbench, e: &PluginEntry) {
    w.configure_external(&e.id, &e.digest, revision(w), &e.declared, true)
        .unwrap();
}
fn event(reply: &Reply, text: &str) -> Vec<u8> {
    Event {
        view: reply.view.clone(),
        generation: reply.generation,
        revision: reply.revision,
        serial: reply.serial + 1,
        node: "title".into(),
        action: "title.edit".into(),
        kind: EventKind::EditText,
        text: text.into(),
        checked: false,
    }
    .encode()
    .unwrap()
}
#[test]
fn missing_bundle_still_manages_packages_and_three_languages_execute_without_grants() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    assert!(!w.writable());
    for lang in ["rust", "c", "cpp"] {
        let e = import(&mut w, &fixture(&format!("{lang}-transform")));
        assert!(!e.enabled && e.approved.is_empty());
        assert!(
            w.run_external_transform(
                &e.id,
                &e.digest,
                revision(&w),
                "bytes.reverse",
                "bytes",
                "bytes",
                b"abc"
            )
            .is_err()
        );
        enable(&mut w, &e);
        assert_eq!(
            w.run_external_transform(
                &e.id,
                &e.digest,
                revision(&w),
                "bytes.reverse",
                "bytes",
                "bytes",
                b"abc"
            )
            .unwrap(),
            b"cba"
        );
        assert!(
            w.run_external_transform(
                &e.id,
                &e.digest,
                revision(&w),
                "bytes.require-ascii",
                "bytes",
                "bytes",
                b"\xff"
            )
            .unwrap_err()
            .to_string()
            .contains("UnsupportedInput")
        );
    }
    let p = w.catalog_page("", None).unwrap();
    assert_eq!(p.entries.len(), 2);
    assert!(!p.cursor.is_empty());
    assert!(w.catalog_page(&p.cursor, None).is_err());
    let rest = w.catalog_page(&p.cursor, Some(p.revision)).unwrap();
    assert_eq!(rest.entries.len(), 1);
    assert!(rest.cursor.is_empty());
    w.finish().unwrap();
    drop(w);
    let w = Workbench::open_managed(dir.path(), None).unwrap();
    assert!(
        w.catalog_page("", None)
            .unwrap()
            .entries
            .iter()
            .all(|e| e.enabled)
    );
}
#[test]
fn stale_digest_revision_and_invalid_approval_never_change_live_form() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let e = import(&mut w, &fixture("rust-ui"));
    enable(&mut w, &e);
    let reply = w
        .external_ui_open(&e.id, &e.digest, revision(&w), "seed")
        .unwrap();
    assert!(reply.failure.is_none());
    let r = revision(&w);
    assert!(
        w.configure_external(&e.id, &e.digest, r + 1, &[], false)
            .is_err()
    );
    assert!(
        w.configure_external(&e.id, &[0; 32], r, &[], false)
            .is_err()
    );
    assert!(
        w.configure_external(&e.id, &e.digest, r, &["read-content".into()], true)
            .is_err()
    );
    assert_eq!(revision(&w), r);
    assert!(w.external_ui_open(&e.id, &e.digest, r, "other").is_err());
    assert!(w.external_ui_close("wrong", reply.generation).is_err());
    let bytes = event(&reply, "alive");
    let updated = w
        .external_ui_event(&e.id, reply.generation, &bytes)
        .unwrap();
    assert!(updated.failure.is_none());
    Document::decode(updated.document.as_ref().unwrap()).unwrap();
    assert!(
        w.external_ui_event(&e.id, reply.generation, &bytes)
            .is_err()
    );
    w.external_ui_close(&e.id, reply.generation).unwrap();
}
#[test]
fn disable_remove_reopen_and_restart_reject_late_events() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let e = import(&mut w, &fixture("c-ui"));
    enable(&mut w, &e);
    let first = w
        .external_ui_open(&e.id, &e.digest, revision(&w), "seed")
        .unwrap();
    let old = event(&first, "old");
    w.configure_external(&e.id, &e.digest, revision(&w), &e.declared, false)
        .unwrap();
    assert!(w.external_ui_event(&e.id, first.generation, &old).is_err());
    enable(&mut w, &e);
    let next = w
        .external_ui_open(&e.id, &e.digest, revision(&w), "new")
        .unwrap();
    assert_ne!(first.generation, next.generation);
    assert!(w.external_ui_event(&e.id, first.generation, &old).is_err());
    w.external_ui_close(&e.id, first.generation).unwrap();
    w.finish().unwrap();
    drop(w);
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let restarted = w
        .external_ui_open(&e.id, &e.digest, revision(&w), "restart")
        .unwrap();
    assert_ne!(restarted.view, next.view);
    assert_ne!(restarted.generation, next.generation);
    assert!(
        w.external_ui_event(&e.id, next.generation, &event(&next, "stale"))
            .is_err()
    );
    w.remove_external(&e.id, &e.digest, revision(&w)).unwrap();
    assert!(w.catalog_page("", None).unwrap().entries.is_empty());
    assert!(
        w.external_ui_event(&e.id, restarted.generation, &event(&restarted, "gone"))
            .is_err()
    );
    let catalog = catalog::Catalog::open(&dir.path().join("plugin-manager/packages")).unwrap();
    catalog.load(e.digest.try_into().unwrap()).unwrap();
}
#[test]
fn external_controls_preserve_builtin_content_and_builtin_switch_preserves_external_form() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), Some(bundle())).unwrap();
    let idea = morrow_workbench_plugin::Idea {
        id: "saved-card".into(),
        title: "keep".into(),
        description: "body".into(),
        category: "灵感".into(),
        stage: "待整理".into(),
        ..Default::default()
    };
    w.create("save", idea).unwrap();
    let e = import(&mut w, &fixture("cpp-ui"));
    enable(&mut w, &e);
    let reply = w
        .external_ui_open(&e.id, &e.digest, revision(&w), "seed")
        .unwrap();
    assert!(w.writable());
    let status = w.plugin_status().unwrap();
    w.configure_plugin(status.revision, &status.digest, false)
        .unwrap();
    let update = w
        .external_ui_event(&e.id, reply.generation, &event(&reply, "survived"))
        .unwrap();
    assert!(update.failure.is_none());
    let status = w.plugin_status().unwrap();
    w.configure_plugin(status.revision, &status.digest, true)
        .unwrap();
    assert!(w.writable());
    w.remove_external(&e.id, &e.digest, revision(&w)).unwrap();
    assert!(w.writable());
    assert_eq!(w.read("saved-card").unwrap().revision, 1);
    let b = root().join("build/workbench-host/bundle/workbench.morrowplugin");
    let preview = w.inspect_plugin(&b).unwrap();
    assert!(preview.entries[0].builtin);
    assert!(
        w.import_plugin(&b, &preview.entries[0].digest, preview.revision)
            .is_err()
    );
    assert!(w.writable());
}
#[test]
fn changed_file_bad_preparation_and_required_dependencies_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let path = dir.path().join("incoming.mplugin");
    std::fs::copy(fixture("rust-ui"), &path).unwrap();
    let preview = w.inspect_plugin(&path).unwrap();
    std::fs::copy(fixture("c-ui"), &path).unwrap();
    assert!(
        w.import_plugin(&path, &preview.entries[0].digest, preview.revision)
            .is_err()
    );
    assert!(w.catalog_page("", None).unwrap().entries.is_empty());
    // Synthetic negative archive only: metadata passes Package, missing task exports cannot prepare.
    let module = b"\0asm\x01\0\0\0";
    let m = Package::manifest_for_task("org.example.invalid", "1.0.0", module, vec![]);
    let bad = Package::build(m, module).unwrap();
    std::fs::write(&path, bad.archive()).unwrap();
    let preview = w.inspect_plugin(&path).unwrap();
    assert!(!preview.entries[0].available);
    assert!(
        w.import_plugin(&path, &bad.digest(), preview.revision)
            .is_err()
    );
    assert_eq!(revision(&w), 0);
    let original = catalog::read_file(&fixture("rust-ui")).unwrap();
    let mut manifest = original.manifest().clone();
    manifest.package_id = "org.example.requires-provider".into();
    manifest
        .required_features
        .push(morrow_core::plugin_package::DEPENDENCIES_FEATURE.into());
    manifest
        .dependencies
        .push(morrow_core::plugin_package::proto::DependencyRequirement {
            slot: "required".into(),
            handler: "bytes.reverse".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            provider_version: "^1".into(),
            optional: false,
        });
    // Negative metadata fixture; never executed or counted as a frozen compatibility artifact.
    let dependent = Package::build(manifest, original.module()).unwrap();
    std::fs::write(&path, dependent.archive()).unwrap();
    let e = import(&mut w, &path);
    assert!(!e.available);
    assert!(!e.dependencies.is_empty());
    assert!(
        w.configure_external(&e.id, &e.digest, revision(&w), &e.declared, true)
            .unwrap_err()
            .to_string()
            .contains("依赖")
    );
    assert!(!entry(&w, &e.id).enabled);
}
#[test]
fn disable_preserves_approved_ceiling_and_removal_keeps_installed_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let e = import(&mut w, &fixture("rust-task"));
    assert_eq!(e.declared.len(), 7);
    enable(&mut w, &e);
    let r = revision(&w);
    assert!(
        w.configure_external(&e.id, &e.digest, r, &[], false)
            .is_err()
    );
    assert_eq!(revision(&w), r);
    w.configure_external(&e.id, &e.digest, r, &e.declared, false)
        .unwrap();
    let current = entry(&w, &e.id);
    assert_eq!(current.approved, e.declared);
    assert!(!current.enabled);
    w.remove_external(&e.id, &e.digest, revision(&w)).unwrap();
    let again = import(&mut w, &fixture("rust-task"));
    assert!(!again.enabled && again.approved.is_empty());
}
#[test]
fn test49_generated_package_is_loaded_as_published_without_repacking() {
    let path = root().join("build/test49-projects/rust-ui/dist");
    let files: Vec<_> = std::fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "mplugin"))
        .collect();
    assert_eq!(files.len(), 1);
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let e = import(&mut w, &files[0]);
    enable(&mut w, &e);
    let reply = w
        .external_ui_open(&e.id, &e.digest, revision(&w), "generated")
        .unwrap();
    assert!(reply.failure.is_none());
    assert!(
        w.external_ui_event(&e.id, reply.generation, &event(&reply, "actual"))
            .unwrap()
            .failure
            .is_none()
    );
}

#[test]
fn real_upgrade_replaces_module_revokes_form_and_restarts_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let v1 = root().join("build/test50-upgrade-v1.mplugin");
    let v2 = root().join("build/test50-upgrade-v2.mplugin");
    let old = import(&mut w, &v1);
    enable(&mut w, &old);
    let form = w
        .external_ui_open(&old.id, &old.digest, revision(&w), "before")
        .unwrap();
    assert!(form.failure.is_none());
    let new = import(&mut w, &v2);
    assert_eq!(new.id, old.id);
    assert_ne!(new.digest, old.digest);
    assert_eq!(new.version, "1.1.0");
    assert!(!new.enabled);
    assert!(
        w.external_ui_event(&old.id, form.generation, &event(&form, "late"))
            .is_err()
    );
    assert!(
        w.configure_external(&old.id, &old.digest, revision(&w), &old.declared, true)
            .is_err()
    );
    enable(&mut w, &new);
    let live = w
        .external_ui_open(&new.id, &new.digest, revision(&w), "after")
        .unwrap();
    assert!(live.failure.is_none());
    let r = revision(&w);
    let preview = w.inspect_plugin(&v1).unwrap();
    assert!(w.import_plugin(&v1, &preview.entries[0].digest, r).is_err());
    assert_eq!(revision(&w), r);
    assert!(
        w.external_ui_event(&new.id, live.generation, &event(&live, "still-live"))
            .unwrap()
            .failure
            .is_none()
    );
    w.finish().unwrap();
    drop(w);
    let w = Workbench::open_managed(dir.path(), None).unwrap();
    let current = entry(&w, &new.id);
    assert_eq!(current.version, "1.1.0");
    assert!(current.enabled);
}

#[test]
fn corrupt_selected_file_can_be_disabled_and_removed_without_rewriting_approvals() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let e = import(&mut w, &fixture("rust-ui"));
    enable(&mut w, &e);
    let form = w
        .external_ui_open(&e.id, &e.digest, revision(&w), "seed")
        .unwrap();
    let name = e
        .digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let path = dir
        .path()
        .join(format!("plugin-manager/packages/{name}.mplugin"));
    std::fs::write(&path, b"broken").unwrap();
    assert!(!entry(&w, &e.id).available);
    assert!(
        w.configure_external(&e.id, &e.digest, revision(&w), &e.declared, true)
            .is_err()
    );
    w.configure_external(&e.id, &e.digest, revision(&w), &e.declared, false)
        .unwrap();
    assert!(
        w.external_ui_event(&e.id, form.generation, &event(&form, "late"))
            .is_err()
    );
    w.remove_external(&e.id, &e.digest, revision(&w)).unwrap();
    assert!(w.catalog_page("", None).unwrap().entries.is_empty());
    assert_eq!(std::fs::read(path).unwrap(), b"broken");
}

#[test]
fn repeated_close_and_close_after_revocation_do_not_close_a_new_form() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(dir.path(), None).unwrap();
    let e = import(&mut w, &fixture("rust-ui"));
    enable(&mut w, &e);
    let first = w
        .external_ui_open(&e.id, &e.digest, revision(&w), "first")
        .unwrap();
    w.external_ui_close(&e.id, first.generation).unwrap();
    w.external_ui_close(&e.id, first.generation).unwrap();
    let second = w
        .external_ui_open(&e.id, &e.digest, revision(&w), "second")
        .unwrap();
    w.external_ui_close(&e.id, first.generation).unwrap();
    assert!(w.external_ui_close("wrong", second.generation).is_err());
    assert!(
        w.external_ui_event(&e.id, second.generation, &event(&second, "still open"))
            .unwrap()
            .failure
            .is_none()
    );
    w.configure_external(&e.id, &e.digest, revision(&w), &e.declared, false)
        .unwrap();
    w.external_ui_close(&e.id, second.generation).unwrap();
}
