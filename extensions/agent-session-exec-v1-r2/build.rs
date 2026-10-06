use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=contracts/session_exec.capnp");
    let digest: [u8; 32] = Sha256::digest(std::fs::read("contracts/session_exec.capnp")?).into();
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("schema_digest.rs"),
        format!("pub const SCHEMA_DIGEST: [u8; 32] = {digest:?};\n"),
    )?;
    // Client builds bind the authoritative Core schema files without linking
    // Core's host Store and SQLite dependencies. There is no schema copy.
    let core = std::path::Path::new("../../core");
    let runtime_source = core.join("src/runtime.rs");
    println!("cargo:rerun-if-changed={}", runtime_source.display());
    let version_source = std::fs::read_to_string(runtime_source)?;
    let protocol_constants: Vec<_> = version_source
        .lines()
        .filter(|line| line.starts_with("pub const PROTOCOL_VERSION:"))
        .collect();
    if protocol_constants != ["pub const PROTOCOL_VERSION: u16 = 7;"] {
        return Err("R2 requires the reviewed Core runtime protocol version 7".into());
    }
    let mut identities = String::from("pub const PROTOCOL_VERSION: u16 = 7;\n");
    for (file, name) in [
        ("schemas/runtime.capnp", "RUNTIME_DIGEST"),
        ("schemas/content.proto", "CONTENT_DIGEST"),
    ] {
        let path = core.join(file);
        println!("cargo:rerun-if-changed={}", path.display());
        let source = std::fs::read_to_string(path)?;
        let canonical = source.replace("\r\n", "\n");
        let digest: [u8; 32] = Sha256::digest(canonical.as_bytes()).into();
        identities.push_str(&format!("pub const {name}: [u8; 32] = {digest:?};\n"));
    }
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("core_identity.rs"),
        identities,
    )?;
    capnpc::CompilerCommand::new()
        .src_prefix("contracts")
        .file("contracts/session_exec.capnp")
        .run()?;
    Ok(())
}
