use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    let version = std::fs::read_to_string("contracts/version.txt")?
        .trim()
        .parse::<u16>()?;
    println!("cargo:rerun-if-changed=contracts/version.txt");
    let task_version = std::fs::read_to_string("contracts/task-version.txt")?
        .trim()
        .parse::<u16>()?;
    println!("cargo:rerun-if-changed=contracts/task-version.txt");
    let ui_version = std::fs::read_to_string("contracts/ui-version.txt")?
        .trim()
        .parse::<u16>()?;
    println!("cargo:rerun-if-changed=contracts/ui-version.txt");
    let mut source =
        format!("pub const VERSION:u16={version};\npub const TASK_VERSION:u16={task_version};\n");
    source.push_str(&format!("pub const UI_VERSION:u16={ui_version};\n"));
    for (name, file) in [
        ("RUNTIME_DIGEST", "runtime.capnp"),
        ("CONTENT_DIGEST", "content.proto"),
        ("TASK_DIGEST", "task.capnp"),
        ("UI_DIGEST", "ui.capnp"),
        ("DEPENDENCY_CALL_DIGEST", "dependency_call.capnp"),
        ("IO_DIGEST", "io.capnp"),
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
        .file("contracts/ui.capnp")
        .file("contracts/dependency_call.capnp")
        .file("contracts/io.capnp")
        .run()?;
    Ok(())
}
