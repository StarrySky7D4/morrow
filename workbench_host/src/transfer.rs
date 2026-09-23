//! Bounded, ordered transfer buffers owned by one private UI connection. Tokens
//! are correlation only; completing an upload never grants permission to commit.
use crate::Result;
use morrow_workbench_plugin::preferences::MAX_BYTES;
use sha2::{Digest, Sha256};
pub const PART_BYTES: usize = 32768;
pub const MAX_DRAFT_BYTES: usize = 8 * 1024 * 1024;
pub const TTL_MS: u64 = 120000;
pub struct Transfers {
    max_bytes: usize,
    sequence: u64,
    upload: Option<Upload>,
    download: Option<Download>,
}
struct Upload {
    token: String,
    operation: String,
    total: usize,
    sha: [u8; 32],
    bytes: Vec<u8>,
    deadline: u64,
}
struct Download {
    token: String,
    sha: [u8; 32],
    bytes: Vec<u8>,
    offset: usize,
    deadline: u64,
}
pub struct Chunk {
    pub token: String,
    pub total: usize,
    pub offset: usize,
    pub sha: [u8; 32],
    pub bytes: Vec<u8>,
}
impl Default for Transfers {
    fn default() -> Self {
        Self {
            max_bytes: MAX_BYTES,
            sequence: 0,
            upload: None,
            download: None,
        }
    }
}
impl Transfers {
    pub fn draft() -> Self {
        Self {
            max_bytes: MAX_DRAFT_BYTES,
            ..Self::default()
        }
    }
    fn expire(&mut self, now: u64) {
        if self.upload.as_ref().is_some_and(|v| now >= v.deadline) {
            self.upload = None;
        }
        if self.download.as_ref().is_some_and(|v| now >= v.deadline) {
            self.download = None;
        }
    }
    fn token(&mut self, prefix: &str) -> Result<String> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("transfer identity exhausted")?;
        Ok(format!("{prefix}-{}", self.sequence))
    }
    pub fn begin(&mut self, operation: String, total: u64, sha: &[u8], now: u64) -> Result<String> {
        self.expire(now);
        if self.upload.is_some() {
            return Err("preference upload busy".into());
        }
        if total == 0 || total > self.max_bytes as u64 || sha.len() != 32 {
            return Err("preference upload bounds".into());
        }
        morrow_core::runtime::Command::ReadSummary {
            request_id: operation.clone(),
            card_id: "preferences".into(),
        }
        .validate()?;
        let token = self.token("upload")?;
        self.upload = Some(Upload {
            token: token.clone(),
            operation,
            total: total as usize,
            sha: sha.try_into()?,
            bytes: Vec::new(),
            deadline: now.saturating_add(TTL_MS),
        });
        Ok(token)
    }
    pub fn append(&mut self, token: &str, offset: u64, bytes: &[u8], now: u64) -> Result<usize> {
        self.expire(now);
        let v = self
            .upload
            .as_mut()
            .ok_or("preference upload absent or expired")?;
        if token != v.token
            || offset != v.bytes.len() as u64
            || bytes.is_empty()
            || bytes.len() > PART_BYTES
            || bytes.len() > v.total - v.bytes.len()
        {
            return Err("preference upload sequence or bounds".into());
        }
        v.bytes.extend_from_slice(bytes);
        Ok(v.bytes.len())
    }
    pub fn finish(&mut self, token: &str, now: u64) -> Result<(String, Vec<u8>)> {
        self.expire(now);
        let v = self
            .upload
            .as_ref()
            .ok_or("preference upload absent or expired")?;
        if token != v.token || v.bytes.len() != v.total {
            return Err("preference upload incomplete".into());
        }
        let v = self.upload.take().unwrap();
        if Sha256::digest(&v.bytes).as_slice() != v.sha {
            return Err("preference upload digest".into());
        }
        Ok((v.operation, v.bytes))
    }
    pub fn open(&mut self, bytes: Vec<u8>, now: u64) -> Result<Chunk> {
        self.expire(now);
        if self.download.is_some() {
            return Err("preference download busy".into());
        }
        if bytes.is_empty() || bytes.len() > self.max_bytes {
            return Err("preference download bounds".into());
        }
        let token = self.token("download")?;
        self.download = Some(Download {
            token: token.clone(),
            sha: Sha256::digest(&bytes).into(),
            bytes,
            offset: 0,
            deadline: now.saturating_add(TTL_MS),
        });
        self.read(&token, 0, now)
    }
    pub fn read(&mut self, token: &str, offset: u64, now: u64) -> Result<Chunk> {
        self.expire(now);
        let v = self
            .download
            .as_mut()
            .ok_or("preference download absent or expired")?;
        if token != v.token || offset != v.offset as u64 {
            return Err("preference download sequence".into());
        }
        let end = (v.offset + PART_BYTES).min(v.bytes.len());
        let part = Chunk {
            token: token.into(),
            total: v.bytes.len(),
            offset: v.offset,
            sha: v.sha,
            bytes: v.bytes[v.offset..end].to_vec(),
        };
        v.offset = end;
        if end == v.bytes.len() {
            self.download = None;
        }
        Ok(part)
    }
    pub fn abort(&mut self, token: &str) {
        if self.upload.as_ref().is_some_and(|v| v.token == token) {
            self.upload = None;
        }
        if self.download.as_ref().is_some_and(|v| v.token == token) {
            self.download = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upload_rejects_bad_sequences_without_advancing_and_checks_digest() {
        let mut t = Transfers::default();
        let bytes = vec![7; PART_BYTES + 19];
        let digest = Sha256::digest(&bytes);
        let token = t
            .begin("save-1".into(), bytes.len() as u64, &digest, 1)
            .unwrap();
        assert!(t.begin("save-2".into(), 1, &digest, 2).is_err());
        assert!(t.append("wrong", 0, &[7], 2).is_err());
        assert!(t.append(&token, 1, &[7], 2).is_err());
        assert!(t.append(&token, 0, &[], 2).is_err());
        assert!(t.append(&token, 0, &bytes, 2).is_err());
        assert!(t.finish(&token, 2).is_err());
        t.append(&token, 0, &bytes[..PART_BYTES], 3).unwrap();
        assert!(t.append(&token, 0, &[7], 4).is_err());
        assert!(t.finish(&token, 4).is_err());
        t.append(&token, PART_BYTES as u64, &bytes[PART_BYTES..], 5)
            .unwrap();
        assert_eq!(t.finish(&token, 6).unwrap(), ("save-1".into(), bytes));
        assert!(t.finish(&token, 7).is_err());
        let bad = t.begin("bad".into(), 1, &[0; 32], 8).unwrap();
        t.append(&bad, 0, &[7], 9).unwrap();
        assert!(t.finish(&bad, 10).is_err());
        assert!(t.upload.is_none());
    }
    #[test]
    fn draft_limit_is_isolated_from_preference_limit() {
        let mut preferences = Transfers::default();
        let mut draft = Transfers::draft();
        assert!(
            preferences
                .begin("save".into(), MAX_BYTES as u64 + 1, &[0; 32], 0)
                .is_err()
        );
        assert!(
            draft
                .begin("save".into(), MAX_DRAFT_BYTES as u64, &[0; 32], 0)
                .is_ok()
        );
        draft.abort("upload-1");
        assert!(
            draft
                .begin("save".into(), MAX_DRAFT_BYTES as u64 + 1, &[0; 32], 0)
                .is_err()
        );
    }

    #[test]
    fn bounds_expiry_abort_and_immutable_download() {
        let mut t = Transfers::default();
        for size in [0, MAX_BYTES as u64 + 1, u64::MAX] {
            assert!(t.begin("save".into(), size, &[0; 32], 0).is_err());
        }
        assert!(t.begin("save".into(), 1, &[0; 31], 0).is_err());
        let token = t.begin("save".into(), 2, &[0; 32], 0).unwrap();
        t.append(&token, 0, &[1], TTL_MS - 1).unwrap();
        assert!(t.append(&token, 1, &[1], TTL_MS).is_err());
        let token = t.begin("save".into(), 2, &[0; 32], TTL_MS).unwrap();
        t.abort("unrelated");
        assert!(t.upload.is_some());
        t.abort(&token);
        assert!(t.upload.is_none());
        let bytes = vec![23; PART_BYTES + 5];
        let first = t.open(bytes.clone(), 0).unwrap();
        assert!(t.open(vec![4], 1).is_err());
        assert!(t.read(&first.token, 0, 1).is_err());
        assert!(t.read("wrong", PART_BYTES as u64, 1).is_err());
        let last = t.read(&first.token, PART_BYTES as u64, 2).unwrap();
        assert_eq!([first.bytes, last.bytes].concat(), bytes);
        assert_eq!(first.sha, last.sha);
        assert!(t.download.is_none());
        let expiring = t.open(vec![3; PART_BYTES + 1], 3).unwrap();
        assert!(
            t.read(&expiring.token, PART_BYTES as u64, 3 + TTL_MS)
                .is_err()
        );
    }
}
