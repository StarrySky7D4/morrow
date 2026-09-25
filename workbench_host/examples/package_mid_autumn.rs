use morrow_core::plugin_package::{Package, catalog, proto::TransformHandler};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let module = std::fs::read(args.next().ok_or("Wasm path required")?)?;
    let output = args.next().ok_or("Output .morrowplugin path required")?;
    let handlers = [
        ("theme.describe", "morrow.ui.theme.request.v1", "morrow.ui.theme.v1", 1, 16384),
        ("theme.artwork", "morrow.ui.theme.chunk.v1", "bytes", 4, 32768),
    ].into_iter().map(|(name, input, output, max_input, max_output)| TransformHandler {
        handler: name.into(), input_type: input.into(), output_type: output.into(),
        max_input_bytes: max_input, max_output_bytes: max_output,
    }).collect();
    let mut manifest = Package::manifest_for_transform("org.morrow.theme.mid-autumn", "1.0.0", &module, handlers);
    manifest.display_name = "中秋 · 月满庭 / Moonlit Court".into();
    let package = Package::build(manifest, &module)?;
    if std::path::Path::new(&output).exists() {
        let old = catalog::read_file(std::path::Path::new(&output))?;
        if old.manifest().package_version == package.manifest().package_version && old.digest() != package.digest() {
            return Err("Theme bytes changed: bump the theme package version before replacing this file".into());
        }
    }
    std::fs::write(output, package.archive())?;
    println!("Independent Mid-Autumn theme package built ({} bytes)", package.archive().len());
    Ok(())
}
