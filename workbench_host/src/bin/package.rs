use morrow_core::plugin_package::{
    Package, catalog,
    proto::{Capability, TransformHandler},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let module = std::fs::read(args.next().ok_or("module")?)?;
    let destination = args.next().ok_or("archive")?;
    let handlers = [
        (
            "workbench.query.v2",
            "morrow.workbench.query.request.v2",
            "morrow.workbench.query.response.v2",
        ),
        (
            "workbench.cards.v2",
            "morrow.workbench.cards.request.v2",
            "morrow.workbench.cards.response.v2",
        ),
        (
            "workbench.tasks.v2",
            "morrow.workbench.tasks.request.v2",
            "morrow.workbench.tasks.response.v2",
        ),
        ("ui.form", "text.utf8", "morrow.ui.document.v1"),
        ("ui.edit", "morrow.ui.event.v1", "morrow.ui.document.v1"),
        (
            "capture.convert",
            "morrow.capture.request.v1",
            "morrow.capture.response.v1",
        ),
        (
            "workbench.command",
            "morrow.workbench.request.v1",
            "morrow.workbench.response.v1",
        ),
        (
            "studio.command",
            "morrow.studio.request.v1",
            "morrow.studio.response.v1",
        ),
        (
            "studio.preferences",
            "morrow.studio.preferences.v1",
            "morrow.studio.preferences.v1",
        ),
    ]
    .into_iter()
    .map(|(handler, input_type, output_type)| TransformHandler {
        handler: handler.into(),
        input_type: input_type.into(),
        output_type: output_type.into(),
        max_input_bytes: 65536,
        max_output_bytes: 65536,
    })
    .collect();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        morrow_workbench_plugin::PACKAGE_VERSION,
        &module,
        handlers,
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    let package = Package::build(manifest, &module)?;
    if std::path::Path::new(&destination).is_file() {
        let previous = catalog::read_file(std::path::Path::new(&destination))?;
        if previous.manifest().package_version == package.manifest().package_version
            && previous.digest() != package.digest()
        {
            return Err("Bundled guest bytes changed without a package version bump; update plugins/workbench/Cargo.toml before packaging".into());
        }
    }
    std::fs::write(&destination, package.archive())?;
    let loaded = catalog::read_file(std::path::Path::new(&destination))?;
    assert_eq!(loaded.digest(), package.digest());
    println!("Bundled immutable Rust workbench package verified");
    Ok(())
}
