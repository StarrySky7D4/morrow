//! Bounded file-descriptor transport. Paths/URIs never cross the JSON bridge.
use crate::{Result, err, hex};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

pub const MAX_IMPORT_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Clone, Debug, Default, Serialize)]
pub struct FileMetadata {
    pub byte_length: String,
    pub sha256: String,
}
impl FileMetadata {
    pub fn new(length: u64, hash: &[u8]) -> Self {
        Self { byte_length: length.to_string(), sha256: hex(hash) }
    }
}
#[derive(Serialize)]
pub struct FileReply {
    pub ok: bool,
    pub error: String,
    pub byte_length: String,
    pub sha256: String,
}
impl FileReply {
    pub fn from_result(result: Result<FileMetadata>) -> Self {
        match result {
            Ok(file) => Self { ok: true, error: String::new(), byte_length: file.byte_length, sha256: file.sha256 },
            Err(error) => Self { ok: false, error, byte_length: String::new(), sha256: String::new() },
        }
    }
}
/// A failed copy leaves a private incomplete spool, never an admitted import.
/// write_all handles short writes; no byte beyond the limit is written.
pub fn prepare(reader: &mut impl Read, writer: &mut impl Write, limit: u64) -> Result<FileMetadata> {
    if limit > MAX_IMPORT_BYTES { return Err("ImportByteLimit".into()); }
    let mut hash = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = match reader.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result.map_err(err)?,
        };
        if count == 0 { break; }
        total = total.checked_add(count as u64).ok_or("ImportByteLimit")?;
        if total > limit { return Err("ImportByteLimit".into()); }
        writer.write_all(&buffer[..count]).map_err(err)?;
        hash.update(&buffer[..count]);
    }
    writer.flush().map_err(err)?;
    Ok(FileMetadata::new(total, &hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_copy_preserves_binary_and_cross_block_hash() {
        let data: Vec<u8> = (0..131_073).map(|n| (n % 251) as u8).collect();
        let mut copied = Vec::new();
        let file = prepare(&mut data.as_slice(), &mut copied, data.len() as u64).unwrap();
        assert_eq!(copied, data);
        assert_eq!(file.byte_length, data.len().to_string());
        assert_eq!(file.sha256, hex(&Sha256::digest(&data)));
        assert!(prepare(&mut data.as_slice(), &mut Vec::new(), data.len() as u64 - 1).is_err());
        assert!(prepare(&mut [].as_slice(), &mut Vec::new(), MAX_IMPORT_BYTES + 1).is_err());
        assert_eq!(prepare(&mut [].as_slice(), &mut Vec::new(), 0).unwrap().sha256, hex(&Sha256::digest([])));
    }
    #[test]
    fn interrupted_short_reads_and_short_writes_are_complete() {
        struct ShortRead { interrupted: bool, remaining: Vec<u8> }
        impl Read for ShortRead {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                if !self.interrupted { self.interrupted = true; return Err(std::io::ErrorKind::Interrupted.into()); }
                let count = bytes.len().min(3).min(self.remaining.len());
                bytes[..count].copy_from_slice(&self.remaining[..count]); self.remaining.drain(..count); Ok(count)
            }
        }
        struct ShortWrite(Vec<u8>);
        impl Write for ShortWrite {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> { let count = bytes.len().min(2); self.0.extend(&bytes[..count]); Ok(count) }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        let data = b"Unicode is bytes: \xe4\xb8\xad\xe6\x96\x87\0\xff";
        let mut writer = ShortWrite(Vec::new());
        let file = prepare(&mut ShortRead { interrupted: false, remaining: data.to_vec() }, &mut writer, 100).unwrap();
        assert_eq!(writer.0, data);
        assert_eq!(file.sha256, hex(&Sha256::digest(data)));
    }
    #[test]
    fn failed_writer_cannot_produce_a_confirmed_metadata_reply() {
        struct Failed;
        impl Write for Failed {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> { Err(std::io::ErrorKind::PermissionDenied.into()) }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        let reply = FileReply::from_result(prepare(&mut b"data".as_slice(), &mut Failed, 4));
        assert!(!reply.ok && reply.sha256.is_empty() && reply.byte_length.is_empty());
    }
}
