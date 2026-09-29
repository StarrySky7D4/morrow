//! Immutable file mutation inputs for the existing persistent IO intent ledger.
//! This module neither opens paths nor grants authority. Platform adapters must
//! bind opaque targets to retained handles, reject unsupported conditional
//! effects, stage content under quotas, and claim dispatch before an OS change.
use crate::{
    Error, Result, envelope,
    file_path::RelativeFilePath,
    identity,
    io_intent::Command,
    plugin_package::io::{self, IoCapability},
};
use prost::Message;
use sha2::{Digest, Sha256};

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.file_mutation.v1.rs"));
}
pub const VERSION: u32 = 1;
pub const MAGIC: &[u8; 8] = b"MROWFWR1";
pub const MAX_RAW_BYTES: usize = 8192;
/// Upper bound accepted by the versioned envelope for one original plan.
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
pub const MAX_RESPONSE_BYTES: u64 = 4096;
pub const MAX_HISTORY_METADATA_BYTES: u64 = MAX_CONTAINER_BYTES as u64 + MAX_RESPONSE_BYTES;
pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../schemas/file_mutation.proto"))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Create,
    Replace,
    Delete,
}
impl Disposition {
    pub fn capability(self) -> IoCapability {
        match self {
            Self::Create => IoCapability::FileCreate,
            Self::Replace => IoCapability::FileReplace,
            Self::Delete => IoCapability::FileDelete,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// Host-issued target identity; it is not an OS path or proof of approval.
    pub reference: [u8; 32],
    /// None names an explicitly selected single-file target. Some names an
    /// entry under an independently approved directory handle. Never reparse or
    /// decode this value after validation, and never resolve it by prefix alone.
    pub relative_path: Option<RelativeFilePath>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutationRequest {
    pub operation_id: String,
    pub subject: String,
    pub package_sha256: [u8; 32],
    pub approval_sha256: [u8; 32],
    pub target: Target,
    pub disposition: Disposition,
    /// Required for replace/delete; a platform-defined original object/version
    /// digest. The platform must support checking it at the effect boundary.
    pub expected_identity: Option<[u8; 32]>,
    pub content_length: u64,
    /// Includes SHA-256(empty) for an empty create/replace. Absent for delete.
    pub content_sha256: Option<[u8; 32]>,
}
impl MutationRequest {
    fn validate(&self) -> Result<()> {
        identity(&self.operation_id)?;
        identity(&self.subject)?;
        if [
            self.package_sha256,
            self.approval_sha256,
            self.target.reference,
        ]
        .contains(&[0; 32])
        {
            return Err(Error::Invalid("empty file mutation identity"));
        }
        if self.content_length > io::MAX_JOB_BYTES {
            return Err(Error::Limit);
        }
        match self.disposition {
            Disposition::Create if self.expected_identity.is_some() => {
                return Err(Error::Invalid("create requires absence, not replacement"));
            }
            Disposition::Replace | Disposition::Delete
                if self.expected_identity.is_none() || self.expected_identity == Some([0; 32]) =>
            {
                return Err(Error::Invalid("missing expected file identity"));
            }
            _ => {}
        }
        match self.disposition {
            Disposition::Delete if self.content_length != 0 || self.content_sha256.is_some() => {
                return Err(Error::Invalid("delete contains write data"));
            }
            Disposition::Create | Disposition::Replace => {
                if self.content_sha256.is_none() || self.content_sha256 == Some([0; 32]) {
                    return Err(Error::Invalid("missing staged content digest"));
                }
                if self.content_length == 0
                    && self.content_sha256 != Some(Sha256::digest([]).into())
                {
                    return Err(Error::Invalid("invalid empty content digest"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct RequestRecord {
    request: MutationRequest,
    container: Vec<u8>,
}
impl RequestRecord {
    pub fn new(request: MutationRequest) -> Result<Self> {
        request.validate()?;
        let value = encode_value(&request);
        let container = envelope::pack(MAGIC, &value.encode_to_vec(), MAX_RAW_BYTES)?;
        Ok(Self { request, container })
    }
    pub fn request(&self) -> &MutationRequest {
        &self.request
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        preflight(&raw)?;
        let value = proto::Request::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("file mutation protobuf"))?;
        if value.schema_version != VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical file mutation"));
        }
        let disposition = match value.disposition {
            1 => Disposition::Create,
            2 => Disposition::Replace,
            3 => Disposition::Delete,
            0 => return Err(Error::Invalid("missing file disposition")),
            _ => return Err(Error::UnsupportedVersion),
        };
        let digest = |v: &[u8]| -> Result<[u8; 32]> {
            v.try_into()
                .map_err(|_| Error::Invalid("file mutation digest length"))
        };
        let request = MutationRequest {
            operation_id: value.operation_id,
            subject: value.subject,
            package_sha256: digest(&value.package_sha256)?,
            approval_sha256: digest(&value.approval_sha256)?,
            target: Target {
                reference: digest(&value.target_reference)?,
                relative_path: if value.relative_path.is_empty() {
                    None
                } else {
                    Some(RelativeFilePath::parse(&value.relative_path)?)
                },
            },
            disposition,
            expected_identity: if value.expected_identity.is_empty() {
                None
            } else {
                Some(digest(&value.expected_identity)?)
            },
            content_length: value.content_length,
            content_sha256: if value.content_sha256.is_empty() {
                None
            } else {
                Some(digest(&value.content_sha256)?)
            },
        };
        request.validate()?;
        Ok(Self {
            request,
            container: container.to_vec(),
        })
    }
    pub fn command(&self) -> Result<Command> {
        self.request.validate()?;
        let r = &self.request;
        let mut target = Sha256::new();
        target.update(b"morrow.file-target.v1\0");
        target.update(r.target.reference);
        target.update([u8::from(r.target.relative_path.is_some())]);
        let relative = r.target.relative_path.as_ref().map_or("", |v| v.as_str());
        target.update((relative.len() as u64).to_le_bytes());
        target.update(relative.as_bytes());
        Ok(Command {
            operation_id: r.operation_id.clone(),
            subject: r.subject.clone(),
            package_sha256: r.package_sha256,
            capability: r.disposition.capability(),
            protocol_sha256: schema_digest(),
            request_sha256: Sha256::digest(&self.container).into(),
            approval_sha256: r.approval_sha256,
            target_sha256: target.finalize().into(),
            request_bytes: self.container.len() as u64,
            response_limit: MAX_RESPONSE_BYTES,
        })
    }
}
fn encode_value(r: &MutationRequest) -> proto::Request {
    proto::Request {
        schema_version: VERSION,
        operation_id: r.operation_id.clone(),
        subject: r.subject.clone(),
        package_sha256: r.package_sha256.to_vec(),
        approval_sha256: r.approval_sha256.to_vec(),
        target_reference: r.target.reference.to_vec(),
        relative_path: r
            .target
            .relative_path
            .as_ref()
            .map_or_else(String::new, |v| v.as_str().to_owned()),
        disposition: match r.disposition {
            Disposition::Create => 1,
            Disposition::Replace => 2,
            Disposition::Delete => 3,
        },
        expected_identity: r.expected_identity.map_or_else(Vec::new, |v| v.to_vec()),
        content_length: r.content_length,
        content_sha256: r.content_sha256.map_or_else(Vec::new, |v| v.to_vec()),
    }
}
fn preflight(mut raw: &[u8]) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut seen = [false; 12];
    while !raw.is_empty() {
        let (tag, wire) =
            decode_key(&mut raw).map_err(|_| Error::Invalid("file mutation field"))?;
        if !(1..=11).contains(&tag) {
            return Err(Error::UnsupportedVersion);
        }
        if std::mem::replace(&mut seen[tag as usize], true) {
            return Err(Error::Invalid("duplicate file mutation field"));
        }
        let scalar = matches!(tag, 1 | 8 | 10);
        if wire
            != if scalar {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err(Error::Invalid("file mutation wire type"));
        }
        let value = decode_varint(&mut raw).map_err(|_| Error::Invalid("file mutation value"))?;
        if scalar {
            if tag != 10 && value > u32::MAX as u64 {
                return Err(Error::Invalid("file mutation scalar overflow"));
            }
        } else {
            let limit = match tag {
                2 | 3 => 256,
                7 => 4096,
                _ => 32,
            };
            if value > limit {
                return Err(Error::Limit);
            }
            if value > raw.len() as u64 {
                return Err(Error::Invalid("file mutation span"));
            }
            raw = &raw[value as usize..];
        }
    }
    Ok(())
}
