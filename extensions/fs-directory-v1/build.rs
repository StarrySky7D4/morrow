use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    const PIN: &str = "ade60daee77497056a3fe618b38616331b8d5de61bdb494f703592e74ec3d5f7";
    println!("cargo:rerun-if-changed=contracts/fs_directory.capnp");
    if format!(
        "{:x}",
        Sha256::digest(std::fs::read("contracts/fs_directory.capnp")?)
    ) != PIN
    {
        return Err("fs-directory-v1 exact raw schema digest mismatch".into());
    }
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file("contracts/fs_directory.capnp")
        .run()?;
    Ok(())
}
