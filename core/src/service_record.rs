//! Historical inbound service identity and bounded request originals.
//! A record is not authorization. Expiry neither deletes it nor permits resend.
use crate::{
    Error, Result, envelope, io_intent::Command, plugin_package::io::IoCapability, service,
};
use prost::Message;
use sha2::{Digest, Sha256};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.service_record.v1.rs"));
}
pub const MAGIC: &[u8; 8] = b"MROWSRQ1";
pub const VERSION: u32 = 1;
pub const MAX_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;
pub const MAX_RAW_BYTES: usize = service::MAX_FRAME_BYTES + 1024;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
#[derive(Clone)]
pub struct Policy {
    pub namespace: [u8; 32],
    pub retention_ms: u64,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        if self.namespace == [0; 32] {
            return Err(Error::Invalid("service namespace"));
        }
        if self.retention_ms == 0 || self.retention_ms > MAX_RETENTION_MS {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/service_record.proto"))
}
fn key_digest(key: &str) -> Result<[u8; 32]> {
    if key.is_empty() || key.len() > 128 || !key.bytes().all(|byte| (33..=126).contains(&byte)) {
        return Err(Error::Invalid("service idempotency key"));
    }
    Ok(Sha256::digest(key.as_bytes()).into())
}
fn field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}
fn operation_digest(
    namespace: &[u8; 32],
    key: &[u8; 32],
    invocation: &service::Invocation,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    field(&mut hash, b"morrow.service-record.operation.v1");
    field(&mut hash, namespace);
    field(&mut hash, invocation.service.as_bytes());
    field(&mut hash, invocation.principal.as_bytes());
    field(&mut hash, key);
    hash.finalize().into()
}
fn hex_id(prefix: &str, digest: [u8; 32]) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(prefix.len() + 64);
    output.push_str(prefix);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("String write");
    }
    output
}
/// Stable external identity excludes internal handler and method/target/body so changed input conflicts
/// with the original operation rather than obtaining a fresh dispatch identity.
pub fn operation_id(policy: &Policy, key: &str, request: &service::Request) -> Result<String> {
    policy.validate()?;
    Ok(hex_id(
        "service-op-",
        operation_digest(&policy.namespace, &key_digest(key)?, request.invocation()),
    ))
}
/// Construct the correlation before encoding the final service Request. This is
/// deterministic identity, not a secret or an independently authenticated token.
pub fn call_id(policy: &Policy, key: &str, invocation: &service::Invocation) -> Result<u64> {
    policy.validate()?;
    let key = key_digest(key)?;
    service::Request::encode(1, invocation)?;
    let digest = operation_digest(&policy.namespace, &key, invocation);
    let value = u64::from_le_bytes(digest[..8].try_into().expect("eight digest bytes"));
    Ok(value.max(1))
}
pub struct RequestRecord {
    value: proto::RequestRecord,
    request: service::Request,
    namespace: [u8; 32],
    key_sha256: [u8; 32],
    container: Vec<u8>,
}
impl RequestRecord {
    pub fn encode(
        policy: &Policy,
        key: &str,
        request: &service::Request,
        created_ms: u64,
    ) -> Result<Self> {
        policy.validate()?;
        if created_ms == 0 {
            return Err(Error::Invalid("service creation time"));
        }
        let expires_ms = created_ms
            .checked_add(policy.retention_ms)
            .ok_or(Error::Invalid("service expiry overflow"))?;
        let value = proto::RequestRecord {
            schema_version: VERSION,
            namespace: policy.namespace.to_vec(),
            key_sha256: key_digest(key)?.to_vec(),
            created_ms,
            expires_ms,
            retention_ms: policy.retention_ms,
            request_frame: request.bytes().to_vec(),
        };
        let container = envelope::pack(MAGIC, &value.encode_to_vec(), MAX_RAW_BYTES)?;
        Self::from_parts(value, container)
    }
    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        preflight(&raw)?;
        let value = proto::RequestRecord::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("service record protobuf"))?;
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical service record"));
        }
        Self::from_parts(value, container.to_vec())
    }
    fn from_parts(value: proto::RequestRecord, container: Vec<u8>) -> Result<Self> {
        if value.schema_version != VERSION {
            return Err(Error::UnsupportedVersion);
        }
        let namespace: [u8; 32] = value
            .namespace
            .as_slice()
            .try_into()
            .map_err(|_| Error::Invalid("service namespace length"))?;
        let key_sha256: [u8; 32] = value
            .key_sha256
            .as_slice()
            .try_into()
            .map_err(|_| Error::Invalid("service key digest length"))?;
        Policy {
            namespace,
            retention_ms: value.retention_ms,
        }
        .validate()?;
        if key_sha256 == [0; 32] {
            return Err(Error::Invalid("service key digest"));
        }
        if value.created_ms == 0
            || value.created_ms.checked_add(value.retention_ms) != Some(value.expires_ms)
        {
            return Err(Error::Invalid("service expiry"));
        }
        let request = service::Request::decode(&value.request_frame)?;
        Ok(Self {
            value,
            request,
            namespace,
            key_sha256,
            container,
        })
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn request(&self) -> &service::Request {
        &self.request
    }
    pub fn created_ms(&self) -> u64 {
        self.value.created_ms
    }
    pub fn expires_ms(&self) -> u64 {
        self.value.expires_ms
    }
    pub fn namespace(&self) -> [u8; 32] {
        self.namespace
    }
    pub fn key_sha256(&self) -> [u8; 32] {
        self.key_sha256
    }
    pub fn matches(&self, policy: &Policy, key: &str, request: &service::Request) -> Result<()> {
        policy.validate()?;
        if self.namespace != policy.namespace
            || self.value.retention_ms != policy.retention_ms
            || self.key_sha256 != key_digest(key)?
            || self.request.bytes() != request.bytes()
        {
            return Err(Error::OperationConflict);
        }
        Ok(())
    }
    pub fn is_expired(&self, now: u64) -> Result<bool> {
        if now < self.created_ms() {
            return Err(Error::Invalid("service clock regression"));
        }
        Ok(now >= self.expires_ms())
    }
    /// Historical command proposal. Store/CAS and current live authorization must
    /// independently admit it; this method never provides permission to dispatch.
    pub fn command(&self, package_sha: [u8; 32]) -> Result<Command> {
        if package_sha == [0; 32] {
            return Err(Error::Invalid("service package digest"));
        }
        let invocation = self.request.invocation();
        let mut subject = Sha256::new();
        field(&mut subject, b"morrow.service-record.subject.v1");
        field(&mut subject, &self.namespace);
        field(&mut subject, invocation.service.as_bytes());
        field(&mut subject, invocation.principal.as_bytes());
        let mut approval = Sha256::new();
        field(&mut approval, b"morrow.service-record.approval.v1");
        field(&mut approval, &self.namespace);
        field(&mut approval, &self.value.retention_ms.to_le_bytes());
        field(&mut approval, &package_sha);
        field(&mut approval, invocation.service.as_bytes());
        field(&mut approval, invocation.handler.as_bytes());
        field(&mut approval, invocation.principal.as_bytes());
        let mut target = Sha256::new();
        field(&mut target, b"morrow.service-record.target.v1");
        field(&mut target, &self.namespace);
        field(&mut target, invocation.service.as_bytes());
        field(&mut target, invocation.handler.as_bytes());
        Ok(Command {
            subject: hex_id("service-scope-", subject.finalize().into()),
            operation_id: hex_id(
                "service-op-",
                operation_digest(&self.namespace, &self.key_sha256, invocation),
            ),
            capability: IoCapability::HttpPublish,
            package_sha256: package_sha,
            protocol_sha256: schema_digest(),
            request_sha256: Sha256::digest(&self.container).into(),
            request_bytes: self.container.len() as u64,
            approval_sha256: approval.finalize().into(),
            target_sha256: target.finalize().into(),
            response_limit: service::MAX_FRAME_BYTES as u64,
        })
    }
}
// Inspect field keys/types/lengths before prost creates any owned field. Exact
// re-encoding additionally rejects reordered, overlong, or explicit-default PB.
fn preflight(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut seen = [false; 8];
    while !raw.is_empty() {
        let (number, wire) =
            decode_key(&mut raw).map_err(|_| Error::Invalid("service record field"))?;
        if !(1..=7).contains(&number) {
            return Err(Error::UnsupportedVersion);
        }
        if std::mem::replace(&mut seen[number as usize], true) {
            return Err(Error::Invalid("duplicate service record field"));
        }
        let scalar = matches!(number, 1 | 4 | 5 | 6);
        if wire
            != if scalar {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err(Error::Invalid("service record wire type"));
        }
        let value = decode_varint(&mut raw).map_err(|_| Error::Invalid("service record value"))?;
        if scalar {
            if number == 1 && value > u32::MAX as u64 {
                return Err(Error::Invalid("service schema version overflow"));
            }
            continue;
        }
        let limit = if number == 7 {
            service::MAX_FRAME_BYTES
        } else {
            32
        };
        if value > limit as u64 {
            return Err(Error::Limit);
        }
        if value > raw.len() as u64 {
            return Err(Error::Invalid("service record span"));
        }
        raw = &raw[value as usize..];
    }
    Ok(())
}
