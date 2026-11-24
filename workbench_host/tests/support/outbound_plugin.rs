use super::Workbench;
use morrow_core::plugin_package::{
    Package, catalog,
    io::{self, IoCapability},
};
use std::path::Path;
pub(super) fn revision(w: &Workbench) -> u64 {
    w.catalog_page("", None).unwrap().revision
}
pub(super) fn install(
    w: &mut Workbench,
    dir: &Path,
    id: &str,
    version: &str,
    credential: bool,
) -> Package {
    let base = catalog::read_file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("sdk/compat/guest-v1-rc1/rust-task.mplugin"),
    )
    .unwrap();
    let mut manifest = base.manifest().clone();
    manifest.package_id = id.into();
    manifest.package_version = version.into();
    manifest.required_features.push(io::FEATURE.into());
    let mut caps = vec![IoCapability::HttpRequest];
    if credential {
        caps.push(IoCapability::CredentialUse);
    }
    manifest.io_declaration = Some(io::declaration(caps, vec!["api.invoke".into()]));
    let package = Package::build(manifest, base.module()).unwrap();
    let path = dir.join("incoming.mplugin");
    std::fs::write(&path, package.archive()).unwrap();
    w.import_plugin(&path, &package.digest(), revision(w))
        .unwrap();
    package
}
pub(super) fn approve(w: &mut Workbench, p: &Package, credential: bool) {
    let mut caps = vec!["http-request".into()];
    if credential {
        caps.push("credential-use".into());
    }
    w.configure_external_io(&p.manifest().package_id, &p.digest(), revision(w), &caps)
        .unwrap();
}
