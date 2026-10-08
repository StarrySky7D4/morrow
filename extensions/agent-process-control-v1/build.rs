use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = "contracts/process_control.capnp";
    println!("cargo:rerun-if-changed={schema}");
    let digest: [u8; 32] = Sha256::digest(std::fs::read(schema)?).into();
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("schema_digest.rs"),
        format!("pub const SCHEMA_DIGEST: [u8; 32] = {digest:?};\n"),
    )?;
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file(schema)
        .run()?;
    Ok(())
}
