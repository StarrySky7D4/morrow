fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=schemas/capture.capnp");
    println!("cargo:rerun-if-changed=schemas/workbench.capnp");
    println!("cargo:rerun-if-changed=schemas/studio.capnp");
    capnpc::CompilerCommand::new()
        .src_prefix("schemas")
        .file("schemas/workbench.capnp")
        .file("schemas/studio.capnp")
        .file("schemas/capture.capnp")
        .run()?;
    println!("cargo:rerun-if-changed=schemas/properties.proto");
    println!("cargo:rerun-if-changed=schemas/preferences.proto");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .file_descriptor_set_path(out.join("properties.descriptor.bin"))
        .compile_protos(
            &["schemas/properties.proto", "schemas/preferences.proto"],
            &["schemas"],
        )?;
    Ok(())
}
