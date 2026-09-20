//! Current-user DPAPI protection for outbound HTTP credentials. Only ciphertext
//! records leave this codec for the original Store's CAS API; no secret files.
//! Opening is decryption and metadata validation, not live authority or a clock
//! check. The original runtime must check current authorization and expiry.
use crate::keys;
use morrow_core::{
    io,
    outbound_authority::{
        self, Record,
        proto::{self as authority, record::Kind},
    },
};
use prost::Message;
use zeroize::{Zeroize, Zeroizing};
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.credentials.v1.rs"));
}
const DOMAIN: &[u8] = b"Morrow/http/credential/secret/v1";
const MAX_PLAIN: usize = 9 * 1024;
const MAX_PACKED: usize = MAX_PLAIN + MAX_PLAIN / 255 + 64;
const MAX_CIPHERTEXT: usize = outbound_authority::MAX_CIPHERTEXT_BYTES;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Format,
    Protection,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Format => "invalid protected HTTP credential",
            Self::Protection => "HTTP credential protection failed",
        })
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
impl Drop for proto::SecretHeader {
    fn drop(&mut self) {
        self.header_name.zeroize();
        self.header_value.zeroize();
        self.domain.zeroize();
        self.reference.zeroize();
    }
}
/// Secret text is deliberately not Debug, Display, Clone or serializable.
pub struct SecretHeader {
    secret: proto::SecretHeader,
}
impl SecretHeader {
    pub fn header_name(&self) -> &str {
        &self.secret.header_name
    }
    pub fn value(&self) -> &str {
        &self.secret.header_value
    }
}
fn validate_header(name: &str, value: &str) -> Result<()> {
    // RFC token characters, canonical lowercase for the private encoding.
    if name.is_empty()
        || name.len() > io::MAX_HEADER_NAME_BYTES
        || !name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b"!#$%&'*+-.^_`|~".contains(&b)
        })
        || value.is_empty()
        || value.len() > io::MAX_HEADER_VALUE_BYTES
        || !value.bytes().all(|b| (0x20..=0x7e).contains(&b))
    {
        return Err(Error::Format);
    }
    if matches!(
        name,
        "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "keep-alive"
            | "upgrade"
            | "te"
            | "trailer"
            | "expect"
    ) || name.starts_with("proxy-")
    {
        return Err(Error::Format);
    }
    Ok(())
}
/// Provision a header; the caller remains responsible for zeroizing its input.
/// Values support printable ASCII plus spaces; tabs, controls and non-ASCII
/// values are rejected. Names are canonicalized to lowercase.
#[allow(clippy::too_many_arguments)]
pub fn seal(
    reference: [u8; 32],
    revision: u64,
    created_ms: u64,
    expires_ms: u64,
    header_name: &str,
    header_value: &str,
) -> Result<Record> {
    // Bound before normalization or any secret-owning allocation.
    if header_name.len() > io::MAX_HEADER_NAME_BYTES
        || header_value.len() > io::MAX_HEADER_VALUE_BYTES
        || reference == [0; 32]
        || revision == 0
        || revision > i64::MAX as u64
        || created_ms == 0
        || expires_ms
            .checked_sub(created_ms)
            .is_none_or(|duration| duration == 0 || duration > outbound_authority::MAX_LIFETIME_MS)
    {
        return Err(Error::Format);
    }
    let name = Zeroizing::new(header_name.to_ascii_lowercase());
    validate_header(&name, header_value)?;
    let secret = proto::SecretHeader {
        schema_version: 1,
        domain: DOMAIN.to_vec(),
        reference: reference.to_vec(),
        revision,
        created_ms,
        expires_ms,
        header_name: name.to_string(),
        header_value: header_value.into(),
    };
    let raw = Zeroizing::new(secret.encode_to_vec());
    if raw.len() > MAX_PLAIN {
        return Err(Error::Format);
    }
    let packed = Zeroizing::new(lz4_flex::block::compress_prepend_size(&raw));
    if packed.len() > MAX_PACKED {
        return Err(Error::Format);
    }
    let ciphertext = keys::protect_http_credential(&packed).map_err(|_| Error::Protection)?;
    if ciphertext.len() > MAX_CIPHERTEXT {
        return Err(Error::Format);
    }
    Record::encode(authority::Record {
        schema_version: 1,
        reference: reference.to_vec(),
        revision,
        created_ms,
        expires_ms,
        disabled: false,
        kind: Some(Kind::Credential(authority::Credential {
            provider: outbound_authority::WINDOWS_DPAPI_PROVIDER.into(),
            ciphertext,
        })),
    })
    .map_err(|_| Error::Format)
}
/// Decrypt only the expected provider/kind and exactly matching metadata. A
/// successful return does not validate current time or grant credential use.
pub fn open(record: &Record) -> Result<SecretHeader> {
    let value = record.value();
    if value.disabled {
        return Err(Error::Format);
    }
    let Some(Kind::Credential(credential)) = value.kind.as_ref() else {
        return Err(Error::Format);
    };
    if credential.provider != outbound_authority::WINDOWS_DPAPI_PROVIDER
        || credential.ciphertext.is_empty()
        || credential.ciphertext.len() > MAX_CIPHERTEXT
    {
        return Err(Error::Format);
    }
    let packed =
        keys::unprotect_http_credential(&credential.ciphertext).map_err(|_| Error::Protection)?;
    let raw = unpack(&packed)?;
    preflight(&raw, record)?;
    let secret = proto::SecretHeader::decode(raw.as_slice()).map_err(|_| Error::Format)?;
    let canonical = Zeroizing::new(secret.encode_to_vec());
    if canonical.as_slice() != raw.as_slice() {
        return Err(Error::Format);
    }
    validate_header(&secret.header_name, &secret.header_value)?;
    Ok(SecretHeader { secret })
}
fn unpack(packed: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    if packed.len() < 4 || packed.len() > MAX_PACKED {
        return Err(Error::Format);
    }
    let length = u32::from_le_bytes(packed[..4].try_into().map_err(|_| Error::Format)?) as usize;
    if length == 0 || length > MAX_PLAIN {
        return Err(Error::Format);
    }
    // Decode directly into a zeroizing allocation, including malformed LZ4 paths.
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
    let metadata = record.value();
    let mut name = None;
    let mut value = None;
    for expected in 1..=8 {
        let (tag, wire) = decode_key(&mut bytes).map_err(|_| Error::Format)?;
        if tag != expected {
            return Err(Error::Format);
        }
        if matches!(tag, 1 | 4 | 5 | 6) {
            if wire != WireType::Varint {
                return Err(Error::Format);
            }
            let number = decode_varint(&mut bytes).map_err(|_| Error::Format)?;
            let expected_number = match tag {
                1 => 1,
                4 => metadata.revision,
                5 => metadata.created_ms,
                6 => metadata.expires_ms,
                _ => return Err(Error::Format),
            };
            if number != expected_number {
                return Err(Error::Format);
            }
        } else {
            if wire != WireType::LengthDelimited {
                return Err(Error::Format);
            }
            let count = decode_varint(&mut bytes).map_err(|_| Error::Format)?;
            let limit = match tag {
                2 => DOMAIN.len(),
                3 => 32,
                7 => io::MAX_HEADER_NAME_BYTES,
                8 => io::MAX_HEADER_VALUE_BYTES,
                _ => return Err(Error::Format),
            };
            if count == 0 || count > limit as u64 || count > bytes.len() as u64 {
                return Err(Error::Format);
            }
            let (field, remaining) = bytes.split_at(count as usize);
            bytes = remaining;
            match tag {
                2 if field != DOMAIN => return Err(Error::Format),
                3 if field != metadata.reference => return Err(Error::Format),
                7 => name = Some(std::str::from_utf8(field).map_err(|_| Error::Format)?),
                8 => value = Some(std::str::from_utf8(field).map_err(|_| Error::Format)?),
                _ => {}
            }
        }
    }
    if !bytes.is_empty() {
        return Err(Error::Format);
    }
    validate_header(name.ok_or(Error::Format)?, value.ok_or(Error::Format)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use morrow_core::store::{EventBudget, Store};
    fn synthetic() -> Record {
        seal(
            [91; 32],
            1,
            100,
            10000,
            "Authorization",
            "Bearer synthetic-unit-test-token",
        )
        .unwrap()
    }
    #[test]
    fn dpapi_header_reopens_from_original_store_without_plaintext_storage() {
        let record = synthetic();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credential.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        store.save_outbound_authority_local(&record, 0).unwrap();
        drop(store);
        let store = Store::open_existing(&path, EventBudget::default()).unwrap();
        let reopened = store
            .load_outbound_authority(&record.reference())
            .unwrap()
            .unwrap();
        let header = open(&reopened).unwrap();
        assert!(header.header_name() == "authorization");
        assert!(header.value() == "Bearer synthetic-unit-test-token");
        let Some(Kind::Credential(credential)) = reopened.value().kind.as_ref() else {
            panic!("wrong kind");
        };
        assert!(
            !credential
                .ciphertext
                .windows(header.value().len())
                .any(|value| value == header.value().as_bytes())
        );
        let database = std::fs::read(&path).unwrap();
        assert!(
            !database
                .windows(header.value().len())
                .any(|value| value == header.value().as_bytes())
        );
        store.integrity_check().unwrap();
    }
    #[test]
    fn ciphertext_cannot_be_rebound_to_other_reference_revision_or_timestamps() {
        let original = synthetic();
        for field in 0..5 {
            let mut value = original.value().clone();
            match field {
                0 => value.reference = vec![92; 32],
                1 => value.revision += 1,
                2 => value.created_ms += 1,
                3 => value.expires_ms += 1,
                4 => value.disabled = true,
                _ => unreachable!(),
            }
            let record = Record::encode(value).unwrap();
            assert!(open(&record).is_err());
        }
    }
    #[test]
    fn ciphertext_tamper_and_foreign_secret_domain_fail_closed() {
        let original = synthetic();
        let mut value = original.value().clone();
        let Some(Kind::Credential(credential)) = value.kind.as_mut() else {
            panic!("wrong kind");
        };
        let last = credential.ciphertext.len() - 1;
        credential.ciphertext[last] ^= 1;
        assert!(open(&Record::encode(value).unwrap()).is_err());
        let mut value = original.value().clone();
        let Some(Kind::Credential(credential)) = value.kind.as_mut() else {
            panic!("wrong kind");
        };
        let decrypted = keys::unprotect_http_credential(&credential.ciphertext).unwrap();
        let raw = unpack(&decrypted).unwrap();
        let mut secret = proto::SecretHeader::decode(raw.as_slice()).unwrap();
        secret.domain[0] ^= 1;
        let raw = Zeroizing::new(secret.encode_to_vec());
        let packed = Zeroizing::new(lz4_flex::block::compress_prepend_size(&raw));
        credential.ciphertext = keys::protect_http_credential(&packed).unwrap();
        assert!(matches!(
            open(&Record::encode(value).unwrap()),
            Err(Error::Format)
        ));
    }
    #[test]
    fn wrong_provider_and_endpoint_kind_never_open_as_secret() {
        let original = synthetic();
        let mut value = original.value().clone();
        let Some(Kind::Credential(credential)) = value.kind.as_mut() else {
            panic!("wrong kind");
        };
        credential.provider = "windows-current-user-dpapi-v1".into();
        assert!(Record::encode(value).is_err());
        let mut value = original.value().clone();
        value.kind = Some(Kind::Endpoint(authority::Endpoint {
            package_id: "org.example.credential.test".into(),
            package_sha256: vec![7; 32],
            origin: "http://127.0.0.1:12345".into(),
            profile: 2,
            methods: vec!["GET".into()],
            credential_reference: vec![],
            root_certificate: vec![],
            max_request_bytes: 1,
            max_response_bytes: 1,
            max_header_bytes: 1,
            max_concurrent: 1,
            timeout_ms: 1,
            max_frame_bytes: 1,
        }));
        assert!(matches!(
            open(&Record::encode(value).unwrap()),
            Err(Error::Format)
        ));
    }
    #[test]
    fn header_framing_controls_empty_and_size_limits_are_rejected() {
        for name in [
            "",
            "Host",
            "Content-Length",
            "Transfer-Encoding",
            "Connection",
            "Keep-Alive",
            "Upgrade",
            "TE",
            "Trailer",
            "Expect",
            "Proxy-Authorization",
            "bad name",
            "bad:name",
            "bad\rname",
        ] {
            assert!(seal([1; 32], 1, 100, 10000, name, "synthetic").is_err());
        }
        for value in ["", "x\r\ny", "x\ty", "x\0y", "x\u{7f}y", "nonascii-雪"] {
            assert!(seal([1; 32], 1, 100, 10000, "authorization", value).is_err());
        }
        assert!(
            seal(
                [1; 32],
                1,
                100,
                10000,
                &"a".repeat(io::MAX_HEADER_NAME_BYTES + 1),
                "x"
            )
            .is_err()
        );
        let long = Zeroizing::new("x".repeat(io::MAX_HEADER_VALUE_BYTES + 1));
        assert!(seal([1; 32], 1, 100, 10000, "authorization", &long).is_err());
        let max = Zeroizing::new("x".repeat(io::MAX_HEADER_VALUE_BYTES));
        let record = seal([1; 32], 1, 100, 10000, "authorization", &max).unwrap();
        assert!(open(&record).unwrap().value() == max.as_str());
    }
    #[test]
    fn malformed_secret_encoding_and_lz4_size_are_bounded_before_decode() {
        let original = synthetic();
        let Some(Kind::Credential(credential)) = original.value().kind.as_ref() else {
            panic!("wrong kind");
        };
        let packed = keys::unprotect_http_credential(&credential.ciphertext).unwrap();
        let raw = unpack(&packed).unwrap();
        let mut duplicate = Zeroizing::new(raw.to_vec());
        duplicate.extend_from_slice(&[8, 1]);
        assert!(preflight(&duplicate, &original).is_err());
        assert!(preflight(&raw[..raw.len() - 1], &original).is_err());
        assert!(unpack(&((MAX_PLAIN as u32) + 1).to_le_bytes()).is_err());
        assert!(unpack(&[1, 0, 0, 0, 0xff]).is_err());
    }
}
