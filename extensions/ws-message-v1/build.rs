use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    const PIN: &str = "2d2f3b1913060bd410d3bc608362c01d28cf7d0e3c1cdc82a5293abcaa696e6d";
    println!("cargo:rerun-if-changed=contracts/ws_message.capnp");
    let bytes = std::fs::read("contracts/ws_message.capnp")?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if digest != PIN {
        return Err("ws-message-v1 exact raw schema digest mismatch".into());
    }
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file("contracts/ws_message.capnp")
        .run()?;
    Ok(())
}
