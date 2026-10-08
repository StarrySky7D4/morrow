//! Explicit same-executable child witnesses. No owner/backend/storage construction.
#![forbid(unsafe_code)]
use std::io::{self, BufRead, Read, Write};
use sha2::{Digest, Sha256};

pub fn valid_nonce(nonce: &str) -> bool {
    nonce.len() == 32 && nonce.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Args exclude argv[0]. Default/preflight arguments do not query the OS console.
pub fn run_child_if_requested(args: &[String]) -> io::Result<Option<i32>> {
    let Some(mode) = args.first() else { return Ok(None) };
    if mode != "--witness-eof" && mode != "--witness-pty" {
        return Ok(None);
    }
    if args.len() != 2 || !valid_nonce(&args[1]) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid witness arguments"));
    }
    let nonce = &args[1];
    if mode == "--witness-eof" {
        let mut bytes = Vec::new();
        io::stdin().lock().take(32769).read_to_end(&mut bytes)?;
        if bytes.len() > 32768 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "witness input limit"));
        }
        println!("WITNESS-EOF {nonce} {} {}", bytes.len(), hex(&Sha256::digest(&bytes)));
        eprintln!("WITNESS-STDERR {nonce}");
        io::stdout().flush()?;
        io::stderr().flush()?;
    } else {
        pty(nonce)?;
    }
    Ok(Some(0))
}

#[cfg(windows)]
fn pty(nonce: &str) -> io::Result<()> {
    let mut input = io::stdin().lock();
    for expected in ["size-1", "size-2", "exit"] {
        let mut line = String::new();
        let count = (&mut input).take(65).read_line(&mut line)?;
        if count == 0 || count > 64 || line.trim_end_matches(['\r', '\n']) != expected {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "witness command mismatch"));
        }
        if expected == "exit" { return Ok(()) }
        // Existing safe dependency calls the child's real GetConsoleScreenBufferInfo.
        // A pipe/not-console error propagates; cached parent sizes never substitute.
        let info = winapi_util::console::screen_buffer_info(io::stdout())?;
        let (cols, rows) = info.size();
        let rect = info.window_rect();
        let view_cols = i32::from(rect.right) - i32::from(rect.left) + 1;
        let view_rows = i32::from(rect.bottom) - i32::from(rect.top) + 1;
        if rows <= 0 || cols <= 0 || view_rows <= 0 || view_cols <= 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid OS console dimensions"));
        }
        println!("WITNESS-SIZE {nonce} {expected} buffer={rows}x{cols} viewport={view_rows}x{view_cols}");
        io::stdout().flush()?;
    }
    Err(io::Error::new(io::ErrorKind::UnexpectedEof, "missing witness exit"))
}

#[cfg(not(windows))]
fn pty(_: &str) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Windows console witness only"))
}
