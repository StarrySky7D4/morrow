//! Bounded human confirmations; no configuration flag grants guest or execution authority.
use anyhow::{Result, ensure};
use std::io::{BufRead, Write};

pub fn line(reader: &mut impl BufRead) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        let chunk = reader.fill_buf()?;
        ensure!(
            !chunk.is_empty(),
            "interactive input ended; retain evidence and original cleanup state"
        );
        let take = chunk
            .iter()
            .position(|b| *b == b'\n')
            .map_or(chunk.len(), |n| n + 1);
        ensure!(
            bytes.len() + take <= 512,
            "interactive input exceeds 512 bytes"
        );
        let done = chunk[take - 1] == b'\n';
        bytes.extend_from_slice(&chunk[..take]);
        reader.consume(take);
        if done {
            break;
        }
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    Ok(String::from_utf8(bytes)?)
}

pub fn confirm(reader: &mut impl BufRead, expected: &str, description: &str) -> Result<()> {
    println!("{description}\nType exactly: {expected}");
    std::io::stdout().flush()?;
    ensure!(
        line(reader)? == expected,
        "confirmation not supplied; no action authorized"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn oversized_input_is_rejected_before_unbounded_allocation() {
        assert!(line(&mut Cursor::new(vec![b'x'; 513])).is_err());
    }
    #[test]
    fn eof_does_not_become_an_implicit_confirmation() {
        assert!(confirm(&mut Cursor::new(b""), "APPLY", "test").is_err());
        assert!(confirm(&mut Cursor::new(b"APPLY \n"), "APPLY", "test").is_err());
    }
    #[test]
    fn exact_confirmations_do_not_consume_the_next_step() {
        let mut input = Cursor::new(b"READ\r\nCREATE\n");
        confirm(&mut input, "READ", "test").unwrap();
        confirm(&mut input, "CREATE", "test").unwrap();
    }
}
