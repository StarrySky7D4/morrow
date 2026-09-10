fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=schemas/content.proto");
    println!("cargo:rerun-if-changed=schemas/runtime.capnp");
    println!("cargo:rerun-if-changed=tests/schemas/future.proto");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .file_descriptor_set_path(out.join("content.descriptor.bin"))
        .compile_protos(&["schemas/content.proto"], &["schemas"])?;
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .file_descriptor_set_path(out.join("future.descriptor.bin"))
        .compile_protos(&["tests/schemas/future.proto"], &["tests/schemas"])?;
    capnpc::CompilerCommand::new()
        .src_prefix("schemas")
        .file("schemas/runtime.capnp")
        .run()?;
    Ok(())
}
