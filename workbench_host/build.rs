fn main() {
    println!("cargo:rerun-if-changed=schemas/projection.proto");
    prost_build::Config::new()
        .protoc_executable(protoc_bin_vendored::protoc_bin_path().expect("protoc"))
        .compile_protos(&["schemas/projection.proto"], &["schemas"])
        .expect("projection schema");
    println!("cargo:rerun-if-changed=schemas/host.capnp");
    capnpc::CompilerCommand::new()
        .src_prefix("schemas")
        .file("schemas/host.capnp")
        .run()
        .expect("host schema");
}
