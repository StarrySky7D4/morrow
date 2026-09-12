fn main() {
    println!("cargo:rerun-if-changed=schemas/host.capnp");
    capnpc::CompilerCommand::new()
        .src_prefix("schemas")
        .file("schemas/host.capnp")
        .run()
        .expect("host schema");
}
