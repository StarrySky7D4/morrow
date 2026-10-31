use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "contracts/blob_upload_history.capnp";
    println!("cargo:rerun-if-changed={path}");
    println!("cargo:rerun-if-changed=contracts/schema.sha256");
    let bytes = std::fs::read(path)?;
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    let expected = std::fs::read_to_string("contracts/schema.sha256")?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected.trim() {
        return Err("upload history exact schema mismatch".into());
    }
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("schema_digest.rs"),
        format!("pub const SCHEMA_DIGEST: [u8; 32] = {digest:?};\n"),
    )?;
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file(path)
        .run()?;
    Ok(())
}
