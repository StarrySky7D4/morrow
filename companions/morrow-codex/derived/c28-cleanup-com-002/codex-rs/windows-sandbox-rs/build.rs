use std::env;
use std::path::PathBuf;
use sha2::{Digest, Sha256};

const SETUP_BIN: &str = "codex-windows-sandbox-setup";
const SETUP_MANIFEST: &str = "codex-windows-sandbox-setup.manifest";

fn main() -> Result<(), String> {
    let schemas = ["schemas/runner_ipc.capnp", "schemas/native_close.capnp"];
    let mut compiler = capnpc::CompilerCommand::new();
    compiler.src_prefix("schemas");
    if let Some(path) = env::var_os("CAPNP") { compiler.capnp_executable(path); }
    for schema in schemas {
        println!("cargo:rerun-if-changed={schema}");
        compiler.file(schema);
    }
    compiler.run().map_err(|_| "Windows IPC schema compilation failed".to_owned())?;
    let runner = std::fs::read(schemas[0]).map_err(|e| e.to_string())?;
    let native = std::fs::read(schemas[1]).map_err(|e| e.to_string())?;
    let runner_pin: [u8; 32] = Sha256::digest(&runner).into();
    let mut combined = Sha256::new();
    combined.update(b"morrow.windows.native-close.capnp.v1\0");
    for bytes in [&runner, &native] {
        combined.update((bytes.len() as u64).to_le_bytes());
        combined.update(bytes);
    }
    let native_pin: [u8; 32] = combined.finalize().into();
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?);
    std::fs::write(out.join("wire_schema_pins.rs"), format!(
        "pub const RUNNER_SCHEMA: [u8;32] = {runner_pin:?};\npub const NATIVE_CLOSE_SCHEMA: [u8;32] = {native_pin:?};\n"
    )).map_err(|e| e.to_string())?;
    println!("cargo:rerun-if-changed={SETUP_MANIFEST}");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }

    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .ok_or_else(|| "CARGO_MANIFEST_DIR should be set for build scripts".to_string())?;
    let manifest_path = PathBuf::from(manifest_dir).join(SETUP_MANIFEST);
    let manifest_path = manifest_path.display();

    // Keep this scoped to the setup helper so Codex binaries that link the
    // library do not inherit any resource metadata from this package.
    match (
        env::var("CARGO_CFG_TARGET_ENV").as_deref(),
        env::var("CARGO_CFG_TARGET_ABI").as_deref(),
    ) {
        (Ok("msvc"), _) => {
            println!("cargo:rustc-link-arg-bin={SETUP_BIN}=/MANIFEST:EMBED");
            println!("cargo:rustc-link-arg-bin={SETUP_BIN}=/MANIFESTINPUT:{manifest_path}");
        }
        (Ok("gnu"), Ok("llvm")) => {
            println!("cargo:rustc-link-arg-bin={SETUP_BIN}=-Wl,-Xlink=/manifest:embed");
            println!(
                "cargo:rustc-link-arg-bin={SETUP_BIN}=-Wl,-Xlink=/manifestinput:{manifest_path}"
            );
        }
        _ => {}
    }

    Ok(())
}
