use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=contracts/session_exec.capnp");
    let digest: [u8; 32] = Sha256::digest(std::fs::read("contracts/session_exec.capnp")?).into();
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("schema_digest.rs"),
        format!("pub const SCHEMA_DIGEST: [u8; 32] = {digest:?};\n"),
    )?;
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file("contracts/session_exec.capnp")
        .run()?;
    Ok(())
}
