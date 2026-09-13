//! Explicit one-time capture only. Never called by the compatibility verifier.
use morrow_core::plugin_package::{
    DEPENDENCIES_FEATURE, DEPENDENCY_CALLS_FEATURE, Package,
    proto::{Capability, DependencyRequirement, TransformHandler},
};
use std::{fs, io::Write, path::Path};
fn handler(name: &str, input: &str, output: &str, max: u32) -> TransformHandler {
    TransformHandler {
        handler: name.into(),
        input_type: input.into(),
        output_type: output.into(),
        max_input_bytes: max,
        max_output_bytes: 65536,
    }
}
fn save(
    root: &Path,
    stem: &str,
    source: &Path,
    profile: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let wasm = fs::read(source)?;
    let id = format!("org.morrow.compat.{}", stem.replace('-', "."));
    let mut manifest = if profile == "task" {
        Package::manifest_for_task(
            &id,
            "1.0.0",
            &wasm,
            vec![
                Capability::RenameCard,
                Capability::ReadSummary,
                Capability::QueryOperation,
                Capability::ReadAttachment,
                Capability::CreateContent,
                Capability::EditContent,
                Capability::ReadContent,
            ],
        )
    } else {
        let handlers = match profile {
            "transform" => [
                "bytes.reverse",
                "bytes.ascii-uppercase",
                "bytes.require-ascii",
            ]
            .into_iter()
            .map(|n| handler(n, "bytes", "bytes", 65536))
            .collect(),
            "ui" => vec![
                handler("ui.form", "text.utf8", "morrow.ui.document.v1", 32),
                handler(
                    "ui.edit",
                    "morrow.ui.event.v1",
                    "morrow.ui.document.v1",
                    65536,
                ),
            ],
            "dependency" => vec![handler("bytes.dependency-wrap", "bytes", "bytes", 65531)],
            "provider" => vec![handler("bytes.tag-reverse", "bytes", "bytes", 65531)],
            _ => return Err("unknown capture profile".into()),
        };
        Package::manifest_for_transform(&id, "1.0.0", &wasm, handlers)
    };
    if profile == "dependency" {
        manifest.requested_capabilities = vec![
            Capability::ReadContent as i32,
            Capability::EditContent as i32,
        ];
        manifest
            .required_features
            .extend([DEPENDENCIES_FEATURE.into(), DEPENDENCY_CALLS_FEATURE.into()]);
        manifest.dependency_schema_sha256 = morrow_core::dependency_call::schema_digest().to_vec();
        manifest.dependencies.push(DependencyRequirement {
            slot: "reverse".into(),
            handler: "bytes.tag-reverse".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            provider_version: "^1.0.0".into(),
            optional: false,
        });
    }
    let package = Package::build(manifest, &wasm)?;
    for (suffix, bytes) in [("wasm", wasm.as_slice()), ("mplugin", package.archive())] {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(format!("{stem}.{suffix}")))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    println!("captured {stem} from {}", source.display());
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        return Err(
            "usage: capture_sdk_baseline NEW_DIRECTORY; refuses an existing directory".into(),
        );
    }
    let root = Path::new(&args[0]);
    fs::create_dir(root)?;
    let rust = Path::new("build/plugin-guest/wasm32-unknown-unknown/release");
    for language in ["rust", "c", "cpp"] {
        for profile in ["task", "transform", "ui", "dependency"] {
            let file = if language == "rust" {
                rust.join(format!(
                    "morrow_example_{}.wasm",
                    if profile == "dependency" {
                        "dependency_caller"
                    } else {
                        profile
                    }
                ))
            } else if profile == "dependency" {
                Path::new("build/dynamic-sdk/wasm")
                    .join(format!("{language}_dependency_caller.wasm"))
            } else {
                Path::new("build/plugin-c-guest").join(format!("{language}_{profile}.wasm"))
            };
            save(root, &format!("{language}-{profile}"), &file, profile)?;
        }
    }
    save(
        root,
        "rust-provider",
        &rust.join("morrow_example_chain_provider.wasm"),
        "provider",
    )?;
    Ok(())
}
