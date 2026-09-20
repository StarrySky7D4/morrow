//! Build an explicitly synthetic approval fixture, never a compatibility original.
//! The unchanged Rust task module is not evidence of network or file execution.
use morrow_core::plugin_package::{
    Package, catalog,
    io::{self, IoCapability},
};
use std::{fs::OpenOptions, io::Write, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: prepare_io_control_fixture ORIGINAL_TASK_PACKAGE NEW_OUTPUT".into());
    }
    let original = catalog::read_file(Path::new(&args[0]))?;
    let mut manifest = original.manifest().clone();
    manifest.package_id = "org.example.io-control-fixture".into();
    manifest.display_name = "IO permission test fixture".into();
    manifest.required_features.push(io::FEATURE.into());
    manifest.io_declaration = Some(io::declaration(
        vec![IoCapability::HttpRequest, IoCapability::CredentialUse],
        vec!["api.invoke".into()],
    ));
    let package = Package::build(manifest, original.module())?;
    // Existing paths are preserved, including frozen original archives.
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    output.write_all(package.archive())?;
    output.sync_all()?;
    println!("Created synthetic IO category fixture; no guest rebuilt or executed.");
    Ok(())
}
