use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=contracts/blob_upload_proposal.capnp");
    println!("cargo:rerun-if-changed=contracts/schema.sha256");
    let raw = std::fs::read("contracts/blob_upload_proposal.capnp")?;
    let expected = std::fs::read_to_string("contracts/schema.sha256")?;
    let digest: [u8; 32] = Sha256::digest(&raw).into();
    if format!("{:x}", Sha256::digest(&raw)) != expected.trim() {
        return Err("upload proposal exact schema mismatch".into());
    }
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("schema_digest.rs"),
        format!("pub const SCHEMA_DIGEST: [u8; 32] = {digest:?};\n"),
    )?;
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file("contracts/blob_upload_proposal.capnp")
        .run()?;
    Ok(())
}
