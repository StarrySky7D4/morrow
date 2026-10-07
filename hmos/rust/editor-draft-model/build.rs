fn main() {
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("bundled protoc");
    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc);
    config
        .compile_protos(&["schemas/editor_draft.proto", "schemas/editor_intent.proto"], &["schemas"])
        .expect("original editor draft schema");
    println!("cargo:rerun-if-changed=schemas/editor_draft.proto");
    println!("cargo:rerun-if-changed=schemas/editor_intent.proto");
}
