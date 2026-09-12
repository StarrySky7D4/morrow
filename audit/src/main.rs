use morrow_audit::{
    ChainVerifier, Checkpoint, MAX_CONTAINER_BYTES, TrustedLog, VerifyingKey, verify,
};
use std::{io::Read, path::Path};
fn read(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_CONTAINER_BYTES as u64 {
        return Err("audit file bounds".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_CONTAINER_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_CONTAINER_BYTES {
        return Err("audit file bounds".into());
    }
    Ok(bytes)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let hex = args
        .next()
        .ok_or("usage: morrow-audit-check PUBLIC_KEY_HEX LOG_ID [--checkpoint FILE] (SEGMENT... | --archive DB | --core-store DB)")?;
    if hex.len() != 64 || !hex.is_ascii() {
        return Err("expected a pinned 32-byte public key".into());
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    let trusted = TrustedLog {
        id: args.next().ok_or("log ID")?,
        key: VerifyingKey::from_bytes(&key)?,
    };
    let mut paths = args.peekable();
    let checkpoint = if paths.peek().is_some_and(|v| v == "--checkpoint") {
        paths.next();
        let path = paths.next().ok_or("checkpoint file")?;
        Some(Checkpoint::from_verified(&verify(
            &read(Path::new(&path))?,
            &trusted,
        )?))
    } else {
        None
    };
    let pinned = checkpoint.is_some();
    let (segments, events) = if paths.peek().is_some_and(|v| v == "--core-store") {
        paths.next();
        let path = paths.next().ok_or("core database")?;
        if paths.next().is_some() {
            return Err("unexpected core database argument".into());
        }
        let store =
            morrow_core::store::Store::open_read_only_audited(Path::new(&path), trusted.clone())?;
        let mut chain = ChainVerifier::new(trusted, checkpoint)?;
        let mut index = 1u64;
        while let Some(bytes) = store.sealed_segment(index)? {
            chain.accept(&bytes)?;
            index = index.checked_add(1).ok_or("segment index overflow")?;
        }
        chain.finish()?
    } else if paths.peek().is_some_and(|v| v == "--archive") {
        paths.next();
        let path = paths.next().ok_or("archive database")?;
        if paths.next().is_some() {
            return Err("unexpected archive argument".into());
        }
        let archive = morrow_audit::archive::Archive::open_read_only(Path::new(&path), trusted)?;
        let receipt = archive
            .check(checkpoint)?
            .ok_or("archive has no sealed segments")?;
        (receipt.index, receipt.last_sequence)
    } else {
        let mut chain = ChainVerifier::new(trusted, checkpoint)?;
        for path in paths {
            chain.accept(&read(Path::new(&path))?)?;
        }
        chain.finish()?
    };
    println!("Verified {segments} signed segments and {events} commit events.");
    println!(
        "Checkpoint supplied: {pinned}. Its independence must be established outside this tool; signatures do not prove a witness or replay correctness."
    );
    Ok(())
}
