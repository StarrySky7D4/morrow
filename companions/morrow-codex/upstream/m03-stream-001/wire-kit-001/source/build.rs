fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=native_http.capnp");
    capnpc::CompilerCommand::new()
        .file("native_http.capnp")
        .run()?;
    Ok(())
}
