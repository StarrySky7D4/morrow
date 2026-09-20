//! Bounded host-local desired service configuration, never restored authority.
//! The host must resolve opaque references and obtain fresh grants after loading.
use crate::{Error, Result, envelope, identity, service_record::Policy};
use prost::Message;
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.service_config.v1.rs"));
}
pub const MAGIC: &[u8; 8] = b"MROWSCF1";
pub const VERSION: u32 = 1;
pub const MAX_RAW_BYTES: usize = 128 * 1024;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
pub const MAX_CONFIGS: usize = 128;
pub const MAX_PRINCIPALS: usize = 64;
pub const MAX_SCOPES: usize = 128;
pub const MAX_APPROVAL_REFERENCES: usize = 64;
#[derive(Clone, Debug)]
pub struct Config {
    value: proto::Configuration,
    container: Vec<u8>,
}
impl Config {
    pub fn encode(value: proto::Configuration) -> Result<Self> {
        validate(&value)?;
        let raw = value.encode_to_vec();
        let container = envelope::pack(MAGIC, &raw, MAX_RAW_BYTES)?;
        Ok(Self { value, container })
    }
    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
        let mut scope_count = 0;
        preflight(&raw, 0, &mut scope_count)?;
        let value = proto::Configuration::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("service config protobuf"))?;
        if value.encode_to_vec() != raw {
            return Err(Error::Invalid("noncanonical service config"));
        }
        validate(&value)?;
        Ok(Self {
            value,
            container: container.to_vec(),
        })
    }
    pub fn value(&self) -> &proto::Configuration {
        &self.value
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn policy(&self) -> Result<Policy> {
        Ok(Policy {
            namespace: digest(&self.value.namespace)?,
            retention_ms: self.value.retention_ms,
        })
    }
}
fn digest(bytes: &[u8]) -> Result<[u8; 32]> {
    let value: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::Invalid("service config reference"))?;
    if value == [0; 32] {
        return Err(Error::Invalid("service config reference"));
    }
    Ok(value)
}
fn validate(value: &proto::Configuration) -> Result<()> {
    if value.schema_version != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    identity(&value.id)?;
    identity(&value.service)?;
    identity(&value.handler)?;
    if value.revision == 0 || value.revision > i64::MAX as u64 {
        return Err(Error::Invalid("service config revision"));
    }
    Policy {
        namespace: digest(&value.namespace)?,
        retention_ms: value.retention_ms,
    }
    .validate()?;
    digest(&value.package_sha256)?;
    if value.principals.len() > MAX_PRINCIPALS
        || value.approval_references.len() > MAX_APPROVAL_REFERENCES
    {
        return Err(Error::Limit);
    }
    let mut last_principal: Option<&str> = None;
    let mut total_scopes = 0usize;
    for principal in &value.principals {
        identity(&principal.id)?;
        digest(&principal.authentication_reference)?;
        if last_principal.is_some_and(|last| last >= principal.id.as_str()) {
            return Err(Error::Invalid("service config principal order"));
        }
        last_principal = Some(&principal.id);
        total_scopes = total_scopes
            .checked_add(principal.content_scopes.len())
            .ok_or(Error::Limit)?;
        if total_scopes > MAX_SCOPES {
            return Err(Error::Limit);
        }
        let mut last_scope = None;
        for scope in &principal.content_scopes {
            if !(1..=7).contains(&scope.kind) {
                return Err(Error::Invalid("service config content kind"));
            }
            identity(&scope.card_id)?;
            if scope.kind == 4 {
                identity(&scope.attachment_id)?;
            } else if !scope.attachment_id.is_empty() {
                return Err(Error::Invalid("service config attachment scope"));
            }
            let key = (
                scope.kind,
                scope.card_id.as_str(),
                scope.attachment_id.as_str(),
            );
            if last_scope.is_some_and(|last| last >= key) {
                return Err(Error::Invalid("service config scope order"));
            }
            last_scope = Some(key);
        }
    }
    let mut last_approval: Option<&[u8]> = None;
    for approval in &value.approval_references {
        digest(approval)?;
        if last_approval.is_some_and(|last| last >= approval.as_slice()) {
            return Err(Error::Invalid("service config approval order"));
        }
        last_approval = Some(approval);
    }
    if value.encoded_len() > MAX_RAW_BYTES {
        return Err(Error::Limit);
    }
    Ok(())
}
// Inspect nested counts and spans before Prost allocates. Only three fixed
// message levels exist. Canonical re-encoding rejects duplicates/defaults/order.
fn preflight(mut raw: &[u8], level: u8, scope_count: &mut usize) -> Result<()> {
    use prost::encoding::{WireType, decode_key, decode_varint};
    let mut seen = [0usize; 12];
    while !raw.is_empty() {
        let (tag, wire) =
            decode_key(&mut raw).map_err(|_| Error::Invalid("service config field"))?;
        let max_tag = if level == 0 { 11 } else { 3 };
        if tag == 0 || tag > max_tag {
            return Err(Error::UnsupportedVersion);
        }
        let repeated_limit = match (level, tag) {
            (0, 10) => MAX_PRINCIPALS,
            (0, 11) => MAX_APPROVAL_REFERENCES,
            (1, 3) => MAX_SCOPES,
            _ => 1,
        };
        seen[tag as usize] += 1;
        if seen[tag as usize] > repeated_limit {
            return Err(Error::Limit);
        }
        let scalar = matches!((level, tag), (0, 1 | 3 | 5 | 9) | (2, 1));
        if wire
            != if scalar {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            }
        {
            return Err(Error::Invalid("service config wire type"));
        }
        let number = decode_varint(&mut raw).map_err(|_| Error::Invalid("service config value"))?;
        if scalar {
            if (level == 0 && tag == 1 && number > u32::MAX as u64)
                || (level == 2 && number > 7)
                || (level == 0 && tag == 9 && number > 1)
            {
                return Err(Error::Invalid("service config scalar"));
            }
            continue;
        }
        let limit = match (level, tag) {
            (0, 10) | (1, 3) => MAX_RAW_BYTES,
            (0, 4 | 8 | 11) | (1, 2) => 32,
            _ => 256,
        };
        if number > limit as u64 {
            return Err(Error::Limit);
        }
        if number > raw.len() as u64 {
            return Err(Error::Invalid("service config span"));
        }
        let (value, tail) = raw.split_at(number as usize);
        raw = tail;
        if matches!((level, tag), (0, 10) | (1, 3)) {
            if level == 1 {
                *scope_count += 1;
                if *scope_count > MAX_SCOPES {
                    return Err(Error::Limit);
                }
            }
            preflight(value, level + 1, scope_count)?;
        }
    }
    Ok(())
}
