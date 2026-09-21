//! Current-user protected TLS bytes, with library/reference/revision binding.
//! This codec does not validate PEM, validity, live authority or trust policy.
use crate::keys;
use morrow_core::tls_identity::{self as identity, Record};
use prost::Message;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.tls_secret.v1.rs"));
}
const DOMAIN: &[u8] = b"Morrow/tls/identity/secret/v1";
const MAX_PEM: usize = identity::MAX_PEM_BYTES;
const MAX_PLAIN: usize = 2 * MAX_PEM + 512;
const MAX_PACKED: usize = MAX_PLAIN + MAX_PLAIN / 255 + 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Format,
    Protection,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Format => "invalid protected TLS identity",
            Self::Protection => "TLS identity protection failed",
        })
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
impl Drop for proto::Secret {
    fn drop(&mut self) {
        self.certificate_pem.zeroize();
        self.private_key_pem.zeroize();
        self.domain.zeroize();
        self.store_id.zeroize();
        self.reference.zeroize();
        self.certificate_sha256.zeroize();
    }
}
/// Deliberately not Debug, Clone or serializable. Caller must not log/copy PEM.
pub struct Material {
    secret: proto::Secret,
}
impl Material {
    pub fn certificate_pem(&self) -> &[u8] {
        &self.secret.certificate_pem
    }
    pub fn private_key_pem(&self) -> &[u8] {
        &self.secret.private_key_pem
    }
}
/// Caller validates the certificate/key pair before saving, and zeroizes inputs.
pub fn seal(
    store_id: [u8; 32],
    reference: [u8; 32],
    revision: u64,
    certificate: &[u8],
    key: &[u8],
) -> Result<Record> {
    if store_id == [0; 32]
        || reference == [0; 32]
        || revision == 0
        || revision > i64::MAX as u64
        || certificate.is_empty()
        || certificate.len() > MAX_PEM
        || key.is_empty()
        || key.len() > MAX_PEM
    {
        return Err(Error::Format);
    }
    let digest: [u8; 32] = Sha256::digest(certificate).into();
    let secret = proto::Secret {
        schema_version: 1,
        domain: DOMAIN.to_vec(),
        store_id: store_id.to_vec(),
        reference: reference.to_vec(),
        revision,
        certificate_sha256: digest.to_vec(),
        certificate_pem: certificate.to_vec(),
        private_key_pem: key.to_vec(),
    };
    let raw = Zeroizing::new(secret.encode_to_vec());
    if raw.len() > MAX_PLAIN {
        return Err(Error::Format);
    }
    let packed = Zeroizing::new(lz4_flex::block::compress_prepend_size(&raw));
    if packed.len() > MAX_PACKED {
        return Err(Error::Format);
    }
    let ciphertext = keys::protect_tls_identity(&packed).map_err(|_| Error::Protection)?;
    Record::encode(identity::proto::Record {
        schema_version: 1,
        store_id: store_id.to_vec(),
        reference: reference.to_vec(),
        revision,
        disabled: false,
        certificate_sha256: digest.to_vec(),
        provider: identity::WINDOWS_PROVIDER.into(),
        ciphertext,
    })
    .map_err(|_| Error::Format)
}
/// Expected values must come from the original Store and frozen caller choice.
/// Possession of ciphertext is never service authorization or revision freshness.
pub fn open(
    record: &Record,
    store_id: &[u8; 32],
    reference: &[u8; 32],
    revision: u64,
) -> Result<Material> {
    let value = record.value();
    if value.disabled
        || value.store_id != store_id
        || value.reference != reference
        || value.revision != revision
        || value.provider != identity::WINDOWS_PROVIDER
    {
        return Err(Error::Format);
    }
    let packed = keys::unprotect_tls_identity(&value.ciphertext).map_err(|_| Error::Protection)?;
    let raw = unpack(&packed)?;
    preflight(&raw, record)?;
    let secret = proto::Secret::decode(raw.as_slice()).map_err(|_| Error::Format)?;
    let canonical = Zeroizing::new(secret.encode_to_vec());
    if canonical.as_slice() != raw.as_slice()
        || Sha256::digest(&secret.certificate_pem).as_slice() != value.certificate_sha256
    {
        return Err(Error::Format);
    }
    Ok(Material { secret })
}
fn unpack(packed: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    if packed.len() < 4 || packed.len() > MAX_PACKED {
        return Err(Error::Format);
    }
    let length = u32::from_le_bytes(packed[..4].try_into().map_err(|_| Error::Format)?) as usize;
    if length == 0 || length > MAX_PLAIN {
        return Err(Error::Format);
    }
    let mut raw = Zeroizing::new(vec![0; length]);
    let written =
        lz4_flex::block::decompress_into(&packed[4..], &mut raw).map_err(|_| Error::Format)?;
    if written != length {
        return Err(Error::Format);
    }
    Ok(raw)
}
fn preflight(mut bytes: &[u8], record: &Record) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    for expected in 1..=8 {
        let (tag, wire) = decode_key(&mut bytes).map_err(|_| Error::Format)?;
        if tag != expected {
            return Err(Error::Format);
        }
        if matches!(tag, 1 | 5) {
            if wire != WireType::Varint {
                return Err(Error::Format);
            }
            let n = decode_varint(&mut bytes).map_err(|_| Error::Format)?;
            if n != if tag == 1 { 1 } else { record.value().revision } {
                return Err(Error::Format);
            }
        } else {
            if wire != WireType::LengthDelimited {
                return Err(Error::Format);
            }
            let limit = match tag {
                2 => DOMAIN.len(),
                3 | 4 | 6 => 32,
                7 | 8 => MAX_PEM,
                _ => return Err(Error::Format),
            };
            let count = decode_varint(&mut bytes).map_err(|_| Error::Format)?;
            if count == 0 || count > limit as u64 || count > bytes.len() as u64 {
                return Err(Error::Format);
            }
            let (data, rest) = bytes.split_at(count as usize);
            bytes = rest;
            match tag {
                2 if data == DOMAIN => {}
                3 if data == record.value().store_id.as_slice() => {}
                4 if data == record.value().reference.as_slice() => {}
                6 if data == record.value().certificate_sha256.as_slice() => {}
                7 | 8 => {}
                _ => return Err(Error::Format),
            }
        }
    }
    if !bytes.is_empty() {
        return Err(Error::Format);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_store_and_snapshot_reopen_ciphertext_without_plaintext_persistence() {
        use morrow_core::store::{EventBudget, Store};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("protected.db");
        let snapshot = dir.path().join("snapshot.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let id = store.tls_store_identity().unwrap();
        let record = seal(
            id,
            [42; 32],
            1,
            b"synthetic TLS certificate",
            b"private-test-key-never-store-unencrypted",
        )
        .unwrap();
        store.save_tls_identity_local(&record, 0).unwrap();
        store.snapshot_to(&snapshot, 16 * 1024 * 1024).unwrap();
        drop(store);
        for file in [&path, &snapshot] {
            let store = Store::open_existing(file, EventBudget::default()).unwrap();
            let stored = store.load_tls_identity(&[42; 32]).unwrap().unwrap();
            let secret = open(&stored, &store.tls_store_identity().unwrap(), &[42; 32], 1).unwrap();
            assert!(secret.private_key_pem() == b"private-test-key-never-store-unencrypted");
            let bytes = std::fs::read(file).unwrap();
            assert!(
                !bytes
                    .windows(secret.private_key_pem().len())
                    .any(|w| w == secret.private_key_pem())
            );
            store.integrity_check().unwrap();
        }
    }
    fn synthetic() -> Record {
        seal(
            [1; 32],
            [2; 32],
            7,
            b"synthetic certificate",
            b"synthetic private key",
        )
        .unwrap()
    }
    #[test]
    fn ciphertext_roundtrip_has_no_plaintext_and_requires_exact_binding() {
        let record = synthetic();
        let decoded = Record::decode(record.container()).unwrap();
        let material = open(&decoded, &[1; 32], &[2; 32], 7).unwrap();
        assert!(material.private_key_pem() == b"synthetic private key");
        assert!(material.certificate_pem() == b"synthetic certificate");
        assert!(
            !record
                .container()
                .windows(material.private_key_pem().len())
                .any(|w| w == material.private_key_pem())
        );
        assert!(open(&decoded, &[3; 32], &[2; 32], 7).is_err());
        assert!(open(&decoded, &[1; 32], &[3; 32], 7).is_err());
        assert!(open(&decoded, &[1; 32], &[2; 32], 8).is_err());
        for case in 0..6 {
            let mut value = record.value().clone();
            match case {
                0 => value.store_id[0] ^= 1,
                1 => value.reference[0] ^= 1,
                2 => value.revision += 1,
                3 => value.certificate_sha256[0] ^= 1,
                4 => value.disabled = true,
                5 => value.ciphertext[30] ^= 1,
                _ => unreachable!(),
            }
            let forged = Record::encode(value).unwrap();
            assert!(
                open(
                    &forged,
                    &forged.store_id(),
                    &forged.reference(),
                    forged.value().revision
                )
                .is_err()
            );
        }
    }
    #[test]
    fn domains_are_separate_and_legacy_limits_are_preserved() {
        let record = synthetic();
        let secret = keys::unprotect_tls_identity(&record.value().ciphertext).unwrap();
        assert!(keys::unprotect_http_credential(&record.value().ciphertext).is_err());
        let http = keys::protect_http_credential(&secret).unwrap();
        assert!(keys::unprotect_tls_identity(&http).is_err());
        assert!(keys::protect_http_credential(&vec![1; 65537]).is_err());
        assert!(keys::protect_tls_identity(&vec![1; identity::MAX_CIPHERTEXT_BYTES + 1]).is_err());
    }
    #[test]
    fn maximum_material_and_invalid_inputs_are_bounded() {
        // Deterministic incompressible bytes exercise the larger TLS DPAPI limit.
        let mut seed = 0x12345678u32;
        let mut noise = || {
            (0..MAX_PEM)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed as u8
                })
                .collect::<Vec<_>>()
        };
        let cert = Zeroizing::new(noise());
        let key = Zeroizing::new(noise());
        let record = seal([1; 32], [2; 32], 1, &cert, &key).unwrap();
        assert!(record.value().ciphertext.len() > 65536);
        let opened = open(&record, &[1; 32], &[2; 32], 1).unwrap();
        assert!(opened.certificate_pem() == cert.as_slice());
        assert!(opened.private_key_pem() == key.as_slice());
        for input in [vec![], vec![1; MAX_PEM + 1]] {
            assert!(seal([1; 32], [2; 32], 1, &input, b"key").is_err());
            assert!(seal([1; 32], [2; 32], 1, b"cert", &input).is_err());
        }
        assert!(seal([0; 32], [2; 32], 1, b"cert", b"key").is_err());
        assert!(seal([1; 32], [0; 32], 1, b"cert", b"key").is_err());
        assert!(seal([1; 32], [2; 32], 0, b"cert", b"key").is_err());
        assert!(seal([1; 32], [2; 32], u64::MAX, b"cert", b"key").is_err());
    }
    #[test]
    fn malformed_secret_fields_are_rejected_without_panics_or_unbounded_allocations() {
        let record = synthetic();
        let packed = keys::unprotect_tls_identity(&record.value().ciphertext).unwrap();
        let raw = unpack(&packed).unwrap();
        assert!(preflight(&raw, &record).is_ok());
        for length in 0..raw.len() {
            assert!(preflight(&raw[..length], &record).is_err());
        }
        for extra in [&[8, 1][..], &[42, 1, 1], &[74, 1, 1]] {
            let mut bad = Zeroizing::new(raw.to_vec());
            bad.extend_from_slice(extra);
            assert!(preflight(&bad, &record).is_err());
        }
        let mut bad = Zeroizing::new(raw.to_vec());
        bad[1] = 7; // schema cannot equal revision instead
        assert!(preflight(&bad, &record).is_err());
        let mut bad = Zeroizing::new(raw.to_vec());
        bad[0] = 42; // wrong wire/tag, never unreachable panic
        assert!(preflight(&bad, &record).is_err());
        assert!(unpack(&((MAX_PLAIN + 1) as u32).to_le_bytes()).is_err());
        assert!(unpack(&[1, 0, 0, 0, 255]).is_err());
        // Authenticated but noncanonical protobuf must not bypass canonical check.
        let mut overlong = Zeroizing::new(raw.to_vec());
        overlong.splice(1..2, [0x81, 0]);
        let mut value = record.value().clone();
        value.ciphertext = keys::protect_tls_identity(&Zeroizing::new(
            lz4_flex::block::compress_prepend_size(&overlong),
        ))
        .unwrap();
        assert!(open(&Record::encode(value).unwrap(), &[1; 32], &[2; 32], 7).is_err());
    }
}
