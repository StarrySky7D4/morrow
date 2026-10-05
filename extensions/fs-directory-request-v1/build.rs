use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    const PIN: &str = "04bc8c556059551c320df641f28bf8da5475737534f9d161ad2269bfe5b1d0df";
    println!("cargo:rerun-if-changed=contracts/fs_directory_request.capnp");
    let raw=std::fs::read("contracts/fs_directory_request.capnp")?;
    if format!("{:x}",Sha256::digest(&raw)) != PIN {return Err("directory request exact raw schema mismatch".into());}
    let digest:[u8;32]=Sha256::digest(&raw).into();
    std::fs::write(std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("schema_digest.rs"),format!("pub const SCHEMA_DIGEST: [u8;32] = {digest:?};\n"))?;
    capnpc::CompilerCommand::new().src_prefix("contracts").file("contracts/fs_directory_request.capnp").run()?;
    Ok(())
}
