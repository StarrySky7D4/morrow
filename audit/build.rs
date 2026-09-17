fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=schemas/build_receipt.proto");
    println!("cargo:rerun-if-changed=schemas/keys.proto");
    println!("cargo:rerun-if-changed=schemas/snapshot.proto");
    println!("cargo:rerun-if-changed=schemas/library.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path()?)
        .compile_protos(
            &[
                "schemas/keys.proto",
                "schemas/snapshot.proto",
                "schemas/library.proto",
                "schemas/build_receipt.proto",
            ],
            &["schemas"],
        )?;
    Ok(())
}
