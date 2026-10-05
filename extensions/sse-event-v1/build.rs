use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    const PIN: &str = "bec1e174b7586e46155b60471867900bf91f1fb11849e6761916bfc3ca7c9863";
    println!("cargo:rerun-if-changed=contracts/sse_event.capnp");
    let bytes = std::fs::read("contracts/sse_event.capnp")?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if digest != PIN {
        return Err("sse-event-v1 exact raw schema digest mismatch".into());
    }
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file("contracts/sse_event.capnp")
        .run()?;
    Ok(())
}
