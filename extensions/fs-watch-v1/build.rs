use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const PIN: &str = "3312e334f5d298d3c56df9c06ed3ac940bbf8fc9a058d113ac30b0d6854f346f";
    println!("cargo:rerun-if-changed=contracts/fs_watch.capnp");
    let raw = std::fs::read("contracts/fs_watch.capnp")?;
    let digest: [u8; 32] = Sha256::digest(&raw).into();
    if format!("{:x}", Sha256::digest(&raw)) != PIN {
        return Err("fs-watch-v1 exact raw schema mismatch".into());
    }
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("schema_digest.rs"),
        format!("pub const SCHEMA_DIGEST: [u8; 32] = {digest:?};\n"),
    )?;
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file("contracts/fs_watch.capnp")
        .run()?;
    Ok(())
}
