//! Read-only standalone verifier; an external digest pin is required.
use morrow_core::task_evidence;
use morrow_plugin_runtime::{Limits, replay};
use std::{io::Read, path::Path, process::ExitCode};
fn run() -> Result<bool, Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: morrow-transform-replay <evidence-file> <raw-evidence-sha256>".into());
    }
    let digest_text = args[1].to_str().ok_or("digest must be ASCII hex")?;
    if digest_text.len() != 64 || !digest_text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("digest must be exactly 64 hex digits".into());
    }
    let mut digest = [0u8; 32];
    for (i, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digest_text[i * 2..i * 2 + 2], 16)?;
    }
    let mut container = Vec::new();
    std::fs::File::open(Path::new(&args[0]))?
        .take(task_evidence::MAX_CONTAINER_BYTES as u64 + 1)
        .read_to_end(&mut container)?;
    if container.len() > task_evidence::MAX_CONTAINER_BYTES {
        return Err("evidence file too large".into());
    }
    let evidence = task_evidence::decode(&container, digest)?;
    let result = replay::replay(&evidence, Limits::default())?;
    println!(
        "{}: pure-transform observation; integrity pin checked; unsigned evidence, no commit or authorization claim",
        if result.matches { "MATCH" } else { "MISMATCH" }
    );
    Ok(result.matches)
}
fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(error) => {
            eprintln!("Replay refused: {error}");
            ExitCode::from(1)
        }
    }
}
