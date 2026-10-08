use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = "contracts/agent_catalog.capnp";
    println!("cargo:rerun-if-changed={schema}");
    let normalized = std::fs::read_to_string(schema)?.replace("\r\n", "\n");
    let digest: [u8; 32] = Sha256::digest(normalized.as_bytes()).into();
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
