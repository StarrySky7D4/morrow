fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=schemas/service_record.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/service_record.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/io_intent.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/io_intent.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/io_evidence.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/io_evidence.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/read_capture.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/read_capture.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/read_archive.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/read_archive.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/read_journal.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/read_journal.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/evidence_storage.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/evidence_storage.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/task_evidence.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/task_evidence.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/audit.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/audit.proto"], &["schemas"])?;
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
    println!("cargo:rerun-if-changed=schemas/io_manifest.proto");
    println!("cargo:rerun-if-changed=schemas/io.capnp");
    println!("cargo:rerun-if-changed=schemas/plugin_package.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .extern_path(".morrow.plugin.io.v1", "crate::plugin_package::io::proto")
        .file_descriptor_set_path(out.join("plugin_package.descriptor.bin"))
        .compile_protos(&["schemas/plugin_package.proto"], &["schemas"])?;
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/io_manifest.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/plugin_registry.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(&["schemas/plugin_registry.proto"], &["schemas"])?;
    println!("cargo:rerun-if-changed=schemas/ui.capnp");
    println!("cargo:rerun-if-changed=schemas/task.capnp");
    println!("cargo:rerun-if-changed=schemas/shared_object.capnp");
    println!("cargo:rerun-if-changed=schemas/shared_transfer.capnp");
    println!("cargo:rerun-if-changed=schemas/dependency_call.capnp");
    println!("cargo:rerun-if-changed=schemas/service.capnp");
    capnpc::CompilerCommand::new()
        .src_prefix("schemas")
        .file("schemas/runtime.capnp")
        .file("schemas/task.capnp")
        .file("schemas/io.capnp")
        .file("schemas/service.capnp")
        .file("schemas/ui.capnp")
        .file("schemas/shared_object.capnp")
        .file("schemas/shared_transfer.capnp")
        .file("schemas/dependency_call.capnp")
        .run()?;
    Ok(())
}
