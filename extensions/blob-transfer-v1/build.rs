use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=contracts/blob_transfer.capnp");
    let bytes = std::fs::read("contracts/blob_transfer.capnp")?;
    if format!("{:x}", Sha256::digest(&bytes))
        != "941db7c662815f8963b46bbb65a5c143d90f9e7217937e84c05d1b8f11024543"
    {
        return Err("blob-transfer-v1 exact raw contract mismatch".into());
    }
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file("contracts/blob_transfer.capnp")
        .run()?;
    Ok(())
}
