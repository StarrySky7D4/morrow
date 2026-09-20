//! Package the real Rust HTTP-forward Wasm guest. This does not install or run it.
use morrow_core::plugin_package::{
    MAX_MODULE_BYTES, Package,
    io::{self, IoCapability},
};
use morrow_plugin_runtime::{Limits, Runner};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
};

const PACKAGE_ID: &str = "org.example.morrow.http-forward";
const HANDLER: &str = "morrow.http.forward.v1";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: package_http_forward RUST_GUEST_WASM NEW_OUTPUT.morrow-plugin".into());
    }
    let mut module = Vec::new();
    File::open(&args[0])?
        .take(MAX_MODULE_BYTES as u64 + 1)
        .read_to_end(&mut module)?;
    if module.len() > MAX_MODULE_BYTES {
        return Err("Wasm module exceeds package size limit".into());
    }
    // Static ABI validation neither executes the guest nor grants IO/network access.
    Runner::new_io_task(
        &module,
        Limits {
            host_calls: 1,
            memory_bytes: 4 * 1024 * 1024,
            ..Limits::default()
        },
    )
    .map_err(|e| format!("unsupported HTTP-forward guest ABI: {e:?}"))?;

    let mut manifest = Package::manifest_for_task(PACKAGE_ID, "0.1.0", &module, vec![]);
    manifest.display_name = "Experimental HTTP forward".into();
    let execution = manifest.budget.as_mut().ok_or("missing execution budget")?;
    execution.host_calls = 1;
    execution.memory_bytes = 4 * 1024 * 1024;
    manifest.required_features.push(io::FEATURE.into());
    let mut declaration = io::declaration(
        vec![IoCapability::HttpRequest, IoCapability::CredentialUse],
        vec![HANDLER.into()],
    );
    let budget = declaration.budget.as_mut().ok_or("missing IO budget")?;
    budget.max_resources = 2;
    budget.max_jobs = 1;
    budget.max_job_bytes = 1024 * 1024;
    budget.max_bytes = 4 * 1024 * 1024;
    budget.max_duration_ms = 30_000;
    manifest.io_declaration = Some(declaration);
    let package = Package::build(manifest, &module)?;
    // Never overwrite an existing package or a preserved compatibility artifact.
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    output.write_all(package.archive())?;
    output.sync_all()?;
    let digest = package
        .digest()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    println!("Created {PACKAGE_ID} ({HANDLER}); package SHA-256 {digest}");
    println!("Packaging only: no install, approval, credential read, or network request.");
    Ok(())
}
