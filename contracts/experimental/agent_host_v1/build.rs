use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=agent_host.capnp");
    let hash: [u8; 32] = Sha256::digest(fs::read("agent_host.capnp")?).into();
    let out = PathBuf::from(env::var("OUT_DIR")?);
    fs::write(
        out.join("identity.rs"),
        format!("pub const SCHEMA_DIGEST: [u8; 32] = {hash:?};\n"),
    )?;
    capnpc::CompilerCommand::new()
        .file("agent_host.capnp")
        .run()?;
    Ok(())
}
