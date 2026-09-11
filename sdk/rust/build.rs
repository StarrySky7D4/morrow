use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    let version = std::fs::read_to_string("contracts/version.txt")?
        .trim()
        .parse::<u16>()?;
    println!("cargo:rerun-if-changed=contracts/version.txt");
    let mut source = format!("pub const VERSION:u16={version};\n");
    for (name, file) in [
        ("RUNTIME_DIGEST", "runtime.capnp"),
        ("CONTENT_DIGEST", "content.proto"),
        ("TASK_DIGEST", "task.capnp"),
    ] {
        println!("cargo:rerun-if-changed=contracts/{file}");
        let text = std::fs::read_to_string(format!("contracts/{file}"))?.replace("\r\n", "\n");
        let hash = Sha256::digest(text.as_bytes());
        source.push_str(&format!(
            "pub const {name}:[u8;32]={:?};\n",
            hash.as_slice()
        ));
    }
    std::fs::write(out.join("contract.rs"), source)?;
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file("contracts/runtime.capnp")
        .file("contracts/task.capnp")
        .run()?;
    Ok(())
}
