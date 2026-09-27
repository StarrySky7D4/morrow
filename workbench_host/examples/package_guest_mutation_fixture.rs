//! Package one real SDK mutation Wasm for isolated native UI qualification.
//! This creates no approval, selection, storage, or filesystem effect target.
use morrow_core::{
    mutation,
    plugin_package::{
        MAX_MODULE_BYTES, MAX_MUTATION_BYTES, MAX_MUTATION_JOB_BYTES, MUTATION_BUDGET_FEATURE,
        MUTATION_FEATURE, Package, catalog,
        io::{self, IoCapability},
        proto::MutationBudget,
    },
};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err(
            "usage: package_guest_mutation_fixture LANGUAGE WASM NEW_OUTPUT.morrowplugin".into(),
        );
    }
    let language = args[0].to_str().ok_or("language is not UTF-8")?;
    if !matches!(language, "rust" | "c" | "cpp") {
        return Err("language must be rust, c or cpp".into());
    }
    let mut wasm = Vec::new();
    File::open(&args[1])?
        .take(MAX_MODULE_BYTES as u64 + 1)
        .read_to_end(&mut wasm)?;
    if wasm.is_empty() || wasm.len() > MAX_MODULE_BYTES {
        return Err("invalid mutation fixture Wasm size".into());
    }
    let id = format!("org.example.workbench.guest-mutation-{language}");
    let mut manifest = Package::manifest_for_task(&id, "1.0.0", &wasm, vec![]);
    manifest.display_name = format!("Real {language} mutation SDK fixture");
    manifest.required_features.extend([
        io::FEATURE.into(),
        MUTATION_FEATURE.into(),
        MUTATION_BUDGET_FEATURE.into(),
    ]);
    manifest.mutation_schema_sha256 = mutation::schema_digest().to_vec();
    manifest.mutation_budget = Some(MutationBudget {
        max_job_bytes: MAX_MUTATION_JOB_BYTES,
        max_bytes: MAX_MUTATION_BYTES,
    });
    let mut declaration = io::declaration(
        vec![IoCapability::FileCreate, IoCapability::FileDelete],
        vec!["mutation.guest.fixture".into()],
    );
    let budget = declaration.budget.as_mut().ok_or("missing IO budget")?;
    budget.max_jobs = 4;
    budget.max_resources = 8;
    budget.max_job_bytes = io::MAX_JOB_BYTES;
    budget.max_bytes = io::MAX_BYTES;
    budget.max_duration_ms = 30_000;
    manifest.io_declaration = Some(declaration);
    let package = Package::build(manifest, &wasm)?;
    if !package.mutation_enabled()
        || package.mutation_budget().is_none_or(|budget| {
            budget.max_job_bytes != MAX_MUTATION_JOB_BYTES || budget.max_bytes != MAX_MUTATION_BYTES
        })
        || package.io_capabilities()
            != &std::collections::BTreeSet::from([
                IoCapability::FileCreate,
                IoCapability::FileDelete,
            ])
    {
        return Err("guest mutation fixture declaration mismatch".into());
    }
    let output = Path::new(&args[2]);
    let mut file = File::options().write(true).create_new(true).open(output)?;
    file.write_all(package.archive())?;
    file.sync_all()?;
    let loaded = catalog::read_file(output)?;
    if loaded.digest() != package.digest() || loaded.module() != wasm {
        return Err("round-trip guest mutation package mismatch".into());
    }
    println!("LANGUAGE={language}");
    println!("WASM_SHA256={}", hex(Sha256::digest(&wasm)));
    println!("PACKAGE_SHA256={}", hex(Sha256::digest(package.archive())));
    println!("PACKAGE_DIGEST={}", hex(package.digest()));
    println!("MUTATION_MAX_JOB_BYTES={MAX_MUTATION_JOB_BYTES}");
    println!("MUTATION_MAX_BYTES={MAX_MUTATION_BYTES}");
    Ok(())
}
