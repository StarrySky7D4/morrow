//! Current-user Windows protection. No load-or-create path and no plaintext key export.
use crate::{SigningKey, TrustedLog};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};
use zeroize::{Zeroize, Zeroizing};
#[allow(unsafe_code)]
mod windows;
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.keys.v1.rs"));
}
const MAGIC: &[u8; 8] = b"MORROWK1";
const PROVIDER: &str = "windows-current-user-dpapi-v1";
const MAX_FILE: usize = 64 * 1024;
impl Drop for proto::SecretKey {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}
#[derive(Debug)]
pub enum KeyError {
    Io(std::io::Error),
    Format,
    Protection,
    Random,
    AlreadyExists,
    PublishUnknown,
}
impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "protected audit key: {self:?}")
    }
}
impl std::error::Error for KeyError {}
impl From<std::io::Error> for KeyError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
pub type Result<T> = std::result::Result<T, KeyError>;
/// Private seed cannot be formatted, exported or passed to a guest through this API.
pub struct Key {
    signing: SigningKey,
    trust: TrustedLog,
}
impl Key {
    pub fn trust(&self) -> TrustedLog {
        self.trust.clone()
    }
    pub(crate) fn sign(&self, value: &crate::proto::Segment) -> crate::Result<Vec<u8>> {
        crate::sign(value, &self.trust, &self.signing)
    }
    /// Explicit provisioning only. Refuse all existing destinations, even damaged ones.
    pub fn create(path: &Path) -> Result<Self> {
        if path.try_exists()? {
            return Err(KeyError::AlreadyExists);
        }
        let mut random = Zeroizing::new([0u8; 48]);
        windows::random(&mut *random)?;
        let id = format!(
            "audit-{}",
            random[32..]
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>()
        );
        let mut seed = Zeroizing::new([0u8; 32]);
        seed.copy_from_slice(&random[..32]);
        let public = SigningKey::from_bytes(&seed).verifying_key();
        let material = proto::SecretKey {
            version: 1,
            log_id: id,
            seed: random[..32].to_vec(),
            public_key: public.as_bytes().to_vec(),
        };
        let plain = Zeroizing::new(material.encode_to_vec());
        let packed = Zeroizing::new(lz4_flex::block::compress_prepend_size(&plain));
        let ciphertext = windows::protect(&packed)?;
        let container = proto::ProtectedKey {
            version: 1,
            provider: PROVIDER.into(),
            ciphertext,
        }
        .encode_to_vec();
        let bytes = frame(&container);
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        boundary("key-before-publish");
        let published = file.persist_noclobber(path).map_err(|e| {
            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                KeyError::AlreadyExists
            } else {
                KeyError::Io(e.error)
            }
        })?;
        boundary("key-after-publish");
        published.sync_all().map_err(|_| KeyError::PublishUnknown)?;
        drop(published);
        // Verify the actual published file. If this fails, never replace it on retry.
        Self::load(path)
    }
    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let meta = file.metadata()?;
        if !meta.is_file() || meta.len() > MAX_FILE as u64 {
            return Err(KeyError::Format);
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > MAX_FILE || bytes.len() < 44 || !bytes.starts_with(MAGIC) {
            return Err(KeyError::Format);
        }
        if Sha256::digest(&bytes[40..]).as_slice() != &bytes[8..40] {
            return Err(KeyError::Format);
        }
        let raw = unpack(&bytes[40..], MAX_FILE)?;
        let protected =
            proto::ProtectedKey::decode(raw.as_slice()).map_err(|_| KeyError::Format)?;
        if protected.version != 1
            || protected.provider != PROVIDER
            || protected.ciphertext.len() > 8192
        {
            return Err(KeyError::Format);
        }
        let packed = windows::unprotect(&protected.ciphertext)?;
        let plain = Zeroizing::new(unpack(&packed, 4096)?);
        let material = proto::SecretKey::decode(plain.as_slice()).map_err(|_| KeyError::Format)?;
        if material.version != 1
            || material.seed.len() != 32
            || material.log_id.len() != 38
            || !material.log_id.starts_with("audit-")
            || !material.log_id[6..].bytes().all(|v| v.is_ascii_hexdigit())
        {
            return Err(KeyError::Format);
        }
        let mut seed = Zeroizing::new([0u8; 32]);
        seed.copy_from_slice(&material.seed);
        let signing = SigningKey::from_bytes(&seed);
        if signing.verifying_key().as_bytes().as_slice() != material.public_key {
            return Err(KeyError::Format);
        }
        let trust = TrustedLog {
            id: material.log_id.clone(),
            key: signing.verifying_key(),
        };
        Ok(Self { signing, trust })
    }
}
fn unpack(bytes: &[u8], limit: usize) -> Result<Vec<u8>> {
    if bytes.len() < 4 || u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize > limit {
        return Err(KeyError::Format);
    }
    lz4_flex::block::decompress_size_prepended(bytes).map_err(|_| KeyError::Format)
}
fn boundary(_name: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok(_name) {
        std::process::exit(86);
    }
}

// Whole compressed-file checksum detects damaged encoding as well as payload;
// authenticity of the secret still comes from DPAPI and the derived public key.
fn frame(container: &[u8]) -> Vec<u8> {
    let packed = lz4_flex::block::compress_prepend_size(container);
    [
        MAGIC.as_slice(),
        Sha256::digest(&packed).as_slice(),
        &packed,
    ]
    .concat()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authenticated_payload_and_derived_key_are_checked_beyond_file_checksum() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("key");
        Key::create(&p).unwrap();
        let original = std::fs::read(&p).unwrap();
        let raw = unpack(&original[40..], MAX_FILE).unwrap();
        let mut protected = proto::ProtectedKey::decode(raw.as_slice()).unwrap();
        protected.ciphertext[0] ^= 1;
        std::fs::write(&p, frame(&protected.encode_to_vec())).unwrap();
        assert!(Key::load(&p).is_err());
        let mut protected = proto::ProtectedKey::decode(raw.as_slice()).unwrap();
        let decrypted = windows::unprotect(&protected.ciphertext).unwrap();
        let plain = Zeroizing::new(unpack(&decrypted, 4096).unwrap());
        let mut secret = proto::SecretKey::decode(plain.as_slice()).unwrap();
        secret.seed[0] ^= 1; // stale embedded public key, despite valid OS protection
        let raw = Zeroizing::new(secret.encode_to_vec());
        let packed = Zeroizing::new(lz4_flex::block::compress_prepend_size(&raw));
        protected.ciphertext = windows::protect(&packed).unwrap();
        std::fs::write(&p, frame(&protected.encode_to_vec())).unwrap();
        assert!(matches!(Key::load(&p), Err(KeyError::Format)));
    }
}
