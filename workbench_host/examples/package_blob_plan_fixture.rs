//! Package the actual SDK planners and typed sample options for qualification.
//! Creates only a new fixture directory, never an approval or protected store.
use morrow_blob_upload_proposal_v1::{
    BOUND_INPUT_TYPE, MAX_BOUND_INPUT_BYTES, MAX_PROPOSAL_BYTES, OUTPUT_TYPE,
};
use morrow_blob_upload_sample::{INPUT_TYPE, Options};
use morrow_core::plugin_package::{MAX_MODULE_BYTES, Package, proto};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: package_blob_plan_fixture LANGUAGE WASM NEW_OUTPUT_DIRECTORY".into());
    }
    let language = args[0].to_str().ok_or("language")?;
    if !matches!(language, "rust" | "c" | "cpp") {
        return Err("unsupported example language".into());
    }
    let mut wasm = Vec::new();
    File::open(&args[1])?
        .take(MAX_MODULE_BYTES as u64 + 1)
        .read_to_end(&mut wasm)?;
    if wasm.is_empty() || wasm.len() > MAX_MODULE_BYTES {
        return Err("module size".into());
    }
    let mut handlers = vec![proto::TransformHandler {
        handler: "blob.plan".into(),
        input_type: INPUT_TYPE.into(),
        output_type: OUTPUT_TYPE.into(),
        max_input_bytes: 256,
        max_output_bytes: MAX_PROPOSAL_BYTES as u32,
    }];
    for handler in ["blob.upload", "blob.upload.release"] {
        handlers.push(proto::TransformHandler {
            handler: handler.into(),
            input_type: BOUND_INPUT_TYPE.into(),
            output_type: "upload.result.v1".into(),
            max_input_bytes: MAX_BOUND_INPUT_BYTES as u32,
            max_output_bytes: 4096,
        });
    }
    let mut manifest = Package::manifest_for_blob_upload_proposals(
        &format!("org.example.blob-plan.{language}"),
        "1.0.0",
        &wasm,
        handlers,
        vec!["blob.upload".into(), "blob.upload.release".into()],
        vec![proto::BlobUploadProposalHandler {
            handler: "blob.plan".into(),
            upload_handler: "blob.upload".into(),
        }],
    );
    manifest.budget.as_mut().unwrap().fuel = morrow_plugin_runtime::MAX_BLOB_UPLOAD_FUEL;
    manifest.budget.as_mut().unwrap().host_calls = 1024;
    let package = Package::build(manifest, &wasm)?;
    let directory = PathBuf::from(&args[2]);
    fs::create_dir(&directory)?;
    File::create_new(directory.join("planner.mplugin"))?.write_all(package.archive())?;
    for length in [150017, 16777216] {
        let options = Options::new(length, 73).map_err(|_| "sample options")?;
        File::create_new(directory.join(format!("options-{length}.bin")))?
            .write_all(&options.encode())?;
    }
    println!("Packaged {language} pure planner and typed options; no store or approval created");
    Ok(())
}
