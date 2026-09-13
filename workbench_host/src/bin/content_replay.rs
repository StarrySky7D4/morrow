//! Verify pinned historical inputs without opening a content store or acquiring credentials.
use morrow_core::{task_evidence, transaction};
use morrow_plugin_runtime::{Limits, replay};
use morrow_workbench_host::projection;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, process::ExitCode};

fn pin(value: &std::ffi::OsStr) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let value = value.to_str().ok_or("digest must be ASCII hex")?;
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("digest must be exactly 64 hex digits".into());
    }
    let mut result = [0; 32];
    for (i, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)?;
    }
    Ok(result)
}
fn read(path: &Path, maximum: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err("input file exceeds verifier budget".into());
    }
    Ok(bytes)
}
fn run() -> Result<bool, Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: morrow-content-replay <commit-file> <commit-container-sha256> <evidence-file> <raw-evidence-sha256>".into());
    }
    let commit_pin = pin(&args[1])?;
    let evidence_pin = pin(&args[3])?;
    let commit_raw = read(
        Path::new(&args[0]),
        transaction::MAX_EVENT_BYTES + transaction::MAX_EVENT_BYTES / 255 + 128,
    )?;
    if Sha256::digest(&commit_raw).as_slice() != commit_pin {
        return Err("commit integrity pin mismatch".into());
    }
    let (commit, _) = transaction::decode_commit(&commit_raw)?;
    let evidence = task_evidence::decode(
        &read(Path::new(&args[2]), task_evidence::MAX_CONTAINER_BYTES)?,
        evidence_pin,
    )?;
    // Reject inconsistent host facts or final commands before executing untrusted guest code.
    projection::verify_commit(&commit, &evidence)?;
    let observed = replay::replay_batch(&evidence, Limits::default(), 1_000_000_000)?;
    println!(
        "{}: versioned content projection and {}/{} pure observations; external pins checked; no signature, source-authority or permission claim",
        if observed.matches {
            "MATCH"
        } else {
            "MISMATCH"
        },
        observed.reports.len(),
        evidence
            .data()
            .batch
            .as_ref()
            .ok_or("missing batch")?
            .observations
            .len()
    );
    Ok(observed.matches)
}
fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(error) => {
            eprintln!("Content replay refused: {error}");
            ExitCode::from(1)
        }
    }
}
