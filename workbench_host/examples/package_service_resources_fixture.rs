//! Packages an actual Rust/Wasm service test guest and two caller input frames.
use morrow_core::{
    io::{HttpSubmission, Request},
    plugin_package::{
        MAX_MODULE_BYTES, Package,
        io::{self, IoCapability},
    },
    service, service_resources,
};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
};

fn write_new(path: &str, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: output.mplugin guest.wasm input.bin".into());
    }
    let mut wasm = Vec::new();
    File::open(&args[1])?
        .take(MAX_MODULE_BYTES as u64 + 1)
        .read_to_end(&mut wasm)?;
    if wasm.len() > MAX_MODULE_BYTES {
        return Err("oversize guest".into());
    }
    let mut manifest = Package::manifest_for_task(
        "org.example.workbench.application.service",
        "1.0.0",
        &wasm,
        vec![],
    );
    let mut declaration = io::declaration(
        vec![
            IoCapability::HttpListen,
            IoCapability::HttpPublish,
            IoCapability::HttpRequest,
        ],
        vec!["application.serve".into()],
    );
    declaration.service_schema_sha256 = service::schema_digest().to_vec();
    let budget = declaration.budget.as_mut().ok_or("missing IO budget")?;
    budget.max_resources = 8;
    budget.max_jobs = 4;
    budget.max_job_bytes = 1024 * 1024;
    budget.max_bytes = 4 * 1024 * 1024;
    declaration.service_run = Some(io::proto::ServiceRunProfile {
        schema_version: io::SERVICE_RUN_VERSION,
        max_duration_ms: 120_000,
        budget: Some(io::proto::ServiceRunBudget {
            schema_version: io::SERVICE_RUN_BUDGET_VERSION,
            max_jobs: 64,
            max_bytes: 4 * 1024 * 1024,
        }),
    });
    manifest.required_features.extend([
        io::FEATURE.into(),
        io::SERVICE_RUN_FEATURE.into(),
        io::SERVICE_RUN_BUDGET_FEATURE.into(),
        service_resources::FEATURE.into(),
    ]);
    manifest.io_declaration = Some(declaration);
    let package = Package::build(manifest, &wasm)?;
    write_new(&args[0], package.archive())?;
    for (suffix, operation) in [
        ("", "window-resource-call"),
        (".wait", "window-resource-wait"),
    ] {
        let request = Request::encode_http_submit(
            1,
            &HttpSubmission {
                operation_id: operation.as_bytes().to_vec(),
                deadline_ms: 0,
                endpoint: vec![b'f'; 64],
                method: "GET".into(),
                relative_target: "/value".into(),
                headers: vec![],
                body: vec![],
                credential: b"untrusted-caller".to_vec(),
            },
        )?;
        write_new(&format!("{}{suffix}", args[2]), request.bytes())?;
    }
    Ok(())
}
