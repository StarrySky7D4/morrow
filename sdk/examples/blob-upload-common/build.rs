use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=contracts/sample_options.capnp");
    println!("cargo:rerun-if-changed=contracts/schema.sha256");
    let bytes = std::fs::read("contracts/sample_options.capnp")?;
    let hash = Sha256::digest(&bytes);
    let expected = std::fs::read_to_string("contracts/schema.sha256")?;
    if format!("{hash:x}") != expected.trim() {
        return Err("sample options exact schema mismatch".into());
    }
    let digest: [u8; 32] = hash.into();
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("schema_digest.rs"),
        format!("pub const SCHEMA_DIGEST:[u8;32]={digest:?};\n"),
    )?;
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file("contracts/sample_options.capnp")
        .run()?;
    Ok(())
}
