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
    println!("cargo:rerun-if-changed=schemas/transaction.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .file_descriptor_set_path(out.join("transaction.descriptor.bin"))
        .compile_protos(&["schemas/transaction.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/attachment.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .file_descriptor_set_path(out.join("attachment.descriptor.bin"))
        .compile_protos(&["schemas/attachment.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/record_transaction.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .file_descriptor_set_path(out.join("record_transaction.descriptor.bin"))
        .compile_protos(&["schemas/record_transaction.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/plugin_package.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .file_descriptor_set_path(out.join("plugin_package.descriptor.bin"))
        .compile_protos(&["schemas/plugin_package.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/ui.capnp");
    println!("cargo:rerun-if-changed=schemas/task.capnp");
    capnpc::CompilerCommand::new()
        .src_prefix("schemas")
        .file("schemas/runtime.capnp")
        .file("schemas/task.capnp")
        .file("schemas/ui.capnp")
        .run()?;
    Ok(())
}
