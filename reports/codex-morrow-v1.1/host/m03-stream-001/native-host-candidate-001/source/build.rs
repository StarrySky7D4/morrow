fn main() {
    println!("cargo:rerun-if-changed=schemas/native_admission.proto");
    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path().unwrap());
    config
        .compile_protos(&["schemas/native_admission.proto"], &["schemas"])
        .unwrap();
}
