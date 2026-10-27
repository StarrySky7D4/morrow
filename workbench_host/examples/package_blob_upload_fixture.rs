//! Package the real three-language SDK examples for product qualification.
//! All output is a new directory; this tool creates no approval or content store.
use morrow_blob_transfer_v1::{Action, Frame, Identity};
use morrow_core::plugin_package::{MAX_MODULE_BYTES, Package, proto::TransformHandler};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: package_blob_upload_fixture LANGUAGE WASM NEW_OUTPUT_DIRECTORY".into());
    }
    let language = args[0].to_str().ok_or("language")?;
    if !matches!(language, "rust" | "c" | "cpp") {
        return Err("language must be rust, c or cpp".into());
    }
    let mut wasm = Vec::new();
    File::open(&args[1])?
        .take(MAX_MODULE_BYTES as u64 + 1)
        .read_to_end(&mut wasm)?;
    if wasm.is_empty() || wasm.len() > MAX_MODULE_BYTES {
        return Err("module size".into());
    }
    let mut manifest = Package::manifest_for_blob_upload(
        &format!("org.example.blob-upload.{language}"),
        "1.0.0",
        &wasm,
        ["blob.upload", "blob.upload.release"]
            .into_iter()
            .map(|handler| TransformHandler {
                handler: handler.into(),
                input_type: "upload.descriptor.v1".into(),
                output_type: "upload.result.v1".into(),
                max_input_bytes: 1024,
                max_output_bytes: 4096,
            })
            .collect(),
    );
    manifest.budget.as_mut().unwrap().fuel = morrow_plugin_runtime::MAX_BLOB_UPLOAD_FUEL;
    manifest.budget.as_mut().unwrap().host_calls = 1024;
    let package = Package::build(manifest, &wasm)?;
    let output = PathBuf::from(&args[2]);
    fs::create_dir(&output)?;
    File::create_new(output.join("upload.mplugin"))?.write_all(package.archive())?;
    for length in [150017_usize, 16 * 1024 * 1024] {
        let frame = Frame {
            identity: Identity {
                transfer_epoch: [71; 32],
                object_ref: [2; 32],
                operation_id: [3; 32],
            },
            action: Action::Descriptor {
                total_length: length as u64,
                whole_sha256: Sha256::digest(vec![42; length]).into(),
            },
        };
        File::create_new(output.join(format!("descriptor-{length}.bin")))?
            .write_all(&frame.encode()?)?;
    }
    println!("Packaged {language} bounded example; no live approval or storage was created");
    Ok(())
}
