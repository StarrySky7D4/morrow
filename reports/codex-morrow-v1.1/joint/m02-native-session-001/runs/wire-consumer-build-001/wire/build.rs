fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=native_session.capnp");
    capnpc::CompilerCommand::new()
        .file("native_session.capnp")
        .run()?;
    Ok(())
}
