//! Package an inert ABI-v2 guest for private service administration tests.
//! Its declared service handler is never executed by these tests.
use morrow_core::plugin_package::{
    Package,
    io::{self, IoCapability},
};
use std::{io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(std::env::args().nth(1).ok_or("output path required")?);
    let wasm = wat::parse_str(
        r#"(module
        (memory (export "memory") 1)
        (func (export "morrow_run") (result i32) i32.const 0))"#,
    )?;
    let mut manifest = Package::manifest_for_task(
        "org.example.morrow.service-admin-test",
        "1.0.0",
        &wasm,
        vec![],
    );
    manifest.required_features.push(io::FEATURE.into());
    let mut declaration = io::declaration(
        vec![IoCapability::HttpListen, IoCapability::HttpPublish],
        vec!["morrow.service.admin.test.v1".into()],
    );
    declaration.service_schema_sha256 = morrow_core::service::schema_digest().to_vec();
    manifest.io_declaration = Some(declaration);
    let package = Package::build(manifest, &wasm)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(package.archive())?;
    file.sync_all()?;
    Ok(())
}
