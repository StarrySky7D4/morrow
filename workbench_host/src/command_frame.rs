use crate::Result;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

const MAX_TOTAL: usize = 128 * 1024;
const MAX_PART: usize = 32 * 1024;
const TTL: Duration = Duration::from_secs(120);

#[derive(Default)]
pub struct FrameUpload {
    entry: Option<Entry>,
}

struct Entry {
    token: String,
    total: usize,
    digest: [u8; 32],
    bytes: Zeroizing<Vec<u8>>,
    deadline: Instant,
}

fn is_valid_token(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        && !token.bytes().all(|b| b == b'0')
}

impl Entry {
    fn expired(&self, now: Instant) -> bool {
        now >= self.deadline
    }
}

impl FrameUpload {
    fn evict(&mut self, now: Instant) {
        if self.entry.as_ref().is_some_and(|e| e.expired(now)) {
            self.entry = None;
        }
    }

    pub fn begin(&mut self, token: &str, total: u64, digest: &[u8], now: Instant) -> Result<()> {
        self.evict(now);
        if self.entry.is_some() {
            return Err("slot busy".into());
        }
        if !is_valid_token(token) {
            return Err("bad token".into());
        }
        if total < 1 || total > MAX_TOTAL as u64 {
            return Err("bad total".into());
        }
        if digest.len() != 32 {
            return Err("bad digest".into());
        }
        let total = total as usize;
        let mut d = [0u8; 32];
        d.copy_from_slice(digest);
        let deadline = now.checked_add(TTL).ok_or("deadline overflow")?;
        self.entry = Some(Entry {
            token: token.to_owned(),
            total,
            digest: d,
            bytes: Zeroizing::new(Vec::with_capacity(total)),
            deadline,
        });
        Ok(())
    }

    pub fn append(
        &mut self,
        token: &str,
        offset: u64,
        bytes: &[u8],
        now: Instant,
    ) -> Result<usize> {
        self.evict(now);
        let e = match &mut self.entry {
            Some(e) => e,
            None => return Err("no upload".into()),
        };
        if e.token != token {
            return Err("token mismatch".into());
        }
        if bytes.is_empty() {
            return Err("empty part".into());
        }
        if bytes.len() > MAX_PART {
            return Err("part too large".into());
        }
        let offset = usize::try_from(offset).map_err(|_| "bad offset")?;
        let end = offset.checked_add(bytes.len()).ok_or("offset overflow")?;
        if end > e.total {
            return Err("out of range".into());
        }
        if offset != e.bytes.len() {
            return Err("offset mismatch".into());
        }
        e.bytes.extend_from_slice(bytes);
        Ok(end)
    }
}
impl FrameUpload {
    pub fn take(
        &mut self,
        token: &str,
        total: u64,
        digest: &[u8],
        now: Instant,
    ) -> Result<Zeroizing<Vec<u8>>> {
        self.evict(now);
        let entry = match self.entry.as_mut() {
            Some(e) => e,
            None => return Err("no upload in progress".into()),
        };
        if entry.token != token
            || entry.total as u64 != total
            || entry.digest.as_slice() != digest
            || entry.bytes.len() != entry.total
        {
            return Err("upload metadata mismatch".into());
        }
        let mut hasher = Sha256::new();
        hasher.update(entry.bytes.as_slice());
        let got = hasher.finalize();
        if got.as_slice() != digest {
            self.entry = None;
            return Err("upload digest mismatch".into());
        }
        let entry = self.entry.take().unwrap();
        Ok(entry.bytes)
    }

    pub fn abort(&mut self, token: &str) {
        if self.entry.as_ref().is_some_and(|e| e.token == token) {
            self.entry = None;
        }
    }

    pub fn clear(&mut self) {
        self.entry = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn token() -> String {
        "ab".repeat(32)
    }
    #[test]
    fn boundaries_order_metadata_and_digest_are_enforced_without_early_consumption() {
        let now = Instant::now();
        let mut upload = FrameUpload::default();
        let data = vec![7; MAX_TOTAL];
        let sha = Sha256::digest(&data);
        for bad in ["", "A", &"00".repeat(32), &"AB".repeat(32)] {
            assert!(upload.begin(bad, 1, &sha, now).is_err());
        }
        for size in [0, MAX_TOTAL as u64 + 1, u64::MAX] {
            assert!(upload.begin(&token(), size, &sha, now).is_err());
        }
        assert!(upload.begin(&token(), 1, &[1; 31], now).is_err());
        upload
            .begin(&token(), data.len() as u64, &sha, now)
            .unwrap();
        assert!(upload.begin(&"cd".repeat(32), 1, &sha, now).is_err());
        for (offset, bytes) in [
            (1, &data[..1]),
            (u64::MAX, &data[..1]),
            (0, &data[..0]),
            (0, &data[..MAX_PART + 1]),
        ] {
            assert!(upload.append(&token(), offset, bytes, now).is_err());
        }
        assert!(upload.take(&token(), data.len() as u64, &sha, now).is_err());
        for (index, part) in data.chunks(MAX_PART).enumerate() {
            assert_eq!(
                upload
                    .append(&token(), (index * MAX_PART) as u64, part, now)
                    .unwrap(),
                (index + 1) * MAX_PART
            );
        }
        assert!(upload.append(&token(), 0, &[7], now).is_err());
        assert!(
            upload
                .take(&"cd".repeat(32), data.len() as u64, &sha, now)
                .is_err()
        );
        assert!(
            upload
                .take(&token(), data.len() as u64 - 1, &sha, now)
                .is_err()
        );
        assert!(
            upload
                .take(&token(), data.len() as u64, &[8; 32], now)
                .is_err()
        );
        assert_eq!(
            *upload.take(&token(), data.len() as u64, &sha, now).unwrap(),
            data
        );
        assert!(upload.take(&token(), data.len() as u64, &sha, now).is_err());
    }
    #[test]
    fn expiry_abort_clear_and_corruption_retire_only_the_original_staging_bytes() {
        let mut upload = FrameUpload::default();
        let now = Instant::now();
        let sha = Sha256::digest([7]);
        upload.begin(&token(), 1, &sha, now).unwrap();
        upload.abort(&"cd".repeat(32));
        upload.append(&token(), 0, &[7], now).unwrap();
        assert!(upload.take(&token(), 1, &sha, now + TTL).is_err());
        assert!(upload.entry.is_none());
        upload.begin(&token(), 1, &sha, now).unwrap();
        upload.append(&token(), 0, &[8], now).unwrap();
        assert!(upload.take(&token(), 1, &sha, now).is_err());
        assert!(upload.entry.is_none());
        upload.begin(&token(), 1, &sha, now).unwrap();
        upload.abort(&token());
        assert!(upload.entry.is_none());
        upload.begin(&token(), 1, &sha, now).unwrap();
        upload.clear();
        assert!(upload.entry.is_none());
    }
}
