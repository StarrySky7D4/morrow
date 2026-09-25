#![cfg(not(target_arch = "wasm32"))]
use capnp::{message::Builder, serialize};
use morrow_core::plugin_package::{MAX_PACKAGE_BYTES, Package, proto::Capability};
use morrow_workbench_host::{Workbench, host_capnp as wire, protocol};
const THEME: &[u8] =
    include_bytes!("../../test/fixtures/plugins/morrow-mid-autumn-1.0.0.morrowplugin");
fn request(action: wire::Action, digest: &[u8], revision: u64, path: &str) -> Vec<u8> {
    let mut m = Builder::new_default();
    let mut r = m.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    r.set_sha256(digest);
    r.set_revision(revision);
    r.set_selected_path(path);
    serialize::write_message_to_words(&m)
}
fn execute(
    w: &mut Workbench,
    action: wire::Action,
    archive: Vec<u8>,
    digest: &[u8],
    revision: u64,
    path: &str,
) -> String {
    let bytes =
        protocol::respond_theme_package(w, &request(action, digest, revision, path), Ok(archive))
            .unwrap();
    let m = serialize::read_message(&mut bytes.as_slice(), Default::default()).unwrap();
    let r = m.get_root::<wire::response::Reader>().unwrap();
    r.get_error().unwrap().to_str().unwrap().into()
}
#[test]
fn original_large_theme_import_is_persistent_disabled_and_digest_bound() {
    assert_eq!(THEME.len(), 661049);
    assert!(THEME.len() > 128 * 1024);
    let p = Package::decode(THEME).unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(root.path(), None).unwrap();
    assert!(
        execute(
            &mut w,
            wire::Action::PluginInspect,
            THEME.into(),
            &[],
            0,
            ""
        )
        .is_empty()
    );
    assert!(w.catalog_page("", None).unwrap().entries.is_empty());
    assert!(
        !execute(
            &mut w,
            wire::Action::PluginImport,
            THEME.into(),
            &[0; 32],
            0,
            ""
        )
        .is_empty()
    );
    assert!(w.catalog_page("", None).unwrap().entries.is_empty());
    assert!(
        execute(
            &mut w,
            wire::Action::PluginImport,
            THEME.into(),
            &p.digest(),
            0,
            ""
        )
        .is_empty()
    );
    let page = w.catalog_page("", None).unwrap();
    assert_eq!(page.entries.len(), 1);
    assert!(!page.entries[0].enabled);
    assert!(page.entries[0].approved.is_empty());
    assert!(
        !execute(
            &mut w,
            wire::Action::PluginImport,
            THEME.into(),
            &p.digest(),
            0,
            ""
        )
        .is_empty()
    );
    assert!(
        execute(
            &mut w,
            wire::Action::PluginImport,
            THEME.into(),
            &p.digest(),
            page.revision,
            ""
        )
        .is_empty()
    );
    drop(w);
    let w = Workbench::open_managed(root.path(), None).unwrap();
    let page = w.catalog_page("", None).unwrap();
    assert_eq!(page.entries[0].digest, p.digest());
    assert!(!page.entries[0].enabled);
}
#[test]
fn invalid_selections_never_install_or_poison_later_requests() {
    let root = tempfile::tempdir().unwrap();
    let mut w = Workbench::open_managed(root.path(), None).unwrap();
    let p = Package::decode(THEME).unwrap();
    let mut privileged = p.manifest().clone();
    privileged.requested_capabilities = vec![Capability::ReadContent as i32];
    let privileged = Package::build(privileged, p.module()).unwrap();
    let mut with_io = p.manifest().clone();
    with_io
        .required_features
        .push(morrow_core::plugin_package::io::FEATURE.into());
    with_io.io_declaration = Some(morrow_core::plugin_package::io::declaration(
        vec![morrow_core::plugin_package::io::IoCapability::FileRead],
        vec!["file.read".into()],
    ));
    let with_io = Package::build(with_io, p.module()).unwrap();
    let dependency = Package::decode(include_bytes!(
        "../../sdk/compat/guest-v1-rc1/rust-dependency.mplugin"
    ))
    .unwrap();
    let mut with_dependency = p.manifest().clone();
    with_dependency.dependencies = dependency.manifest().dependencies.clone();
    with_dependency
        .required_features
        .push(morrow_core::plugin_package::DEPENDENCIES_FEATURE.into());
    for requirement in &mut with_dependency.dependencies {
        requirement.optional = true;
    }
    let with_dependency = Package::build(with_dependency, p.module()).unwrap();
    for rejected in [&privileged, &with_io, &with_dependency] {
        assert!(rejected.is_ui_theme());
        for action in [wire::Action::PluginInspect, wire::Action::PluginImport] {
            assert!(
                execute(
                    &mut w,
                    action,
                    rejected.archive().to_vec(),
                    &rejected.digest(),
                    0,
                    ""
                )
                .contains("pure theme plugins only")
            );
        }
    }
    for bytes in [
        vec![],
        vec![1, 2, 3],
        vec![0; MAX_PACKAGE_BYTES + 1],
        privileged.archive().to_vec(),
        include_bytes!("../../sdk/compat/guest-v1-rc1/rust-transform.mplugin").to_vec(),
        include_bytes!("../../sdk/compat/guest-v1-rc1/rust-dependency.mplugin").to_vec(),
    ] {
        assert!(!execute(&mut w, wire::Action::PluginInspect, bytes, &[], 0, "").is_empty());
        assert!(w.catalog_page("", None).unwrap().entries.is_empty());
    }
    assert!(
        !execute(
            &mut w,
            wire::Action::PluginInspect,
            THEME.into(),
            &[],
            0,
            "C:/theme.morrowplugin"
        )
        .is_empty()
    );
    assert!(
        !execute(
            &mut w,
            wire::Action::PluginApprove,
            THEME.into(),
            &[],
            0,
            ""
        )
        .is_empty()
    );
    let failed_read = protocol::respond_theme_package(
        &mut w,
        &request(wire::Action::PluginInspect, &[], 0, ""),
        Err("selected file read failed".into()),
    )
    .unwrap();
    let m = serialize::read_message(&mut failed_read.as_slice(), Default::default()).unwrap();
    assert!(
        !m.get_root::<wire::response::Reader>()
            .unwrap()
            .get_error()
            .unwrap()
            .is_empty()
    );
    assert!(
        execute(
            &mut w,
            wire::Action::PluginImport,
            THEME.into(),
            &p.digest(),
            0,
            ""
        )
        .is_empty()
    );
}
