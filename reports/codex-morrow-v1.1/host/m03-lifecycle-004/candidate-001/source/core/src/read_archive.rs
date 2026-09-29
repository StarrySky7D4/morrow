//! Bounded opaque read-capture fragments. These hashes prove byte association, not
//! execution, source completeness, authorization or delivery. An adapter must verify semantics.
use crate::{Error, Result, identity};
use prost::Message;
use sha2::{Digest, Sha256};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.read_archive.v1.rs"));
}
pub const MAX_PART_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PARTS: u32 = 65_536;
pub const MAX_LOGICAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_RAW_BYTES: usize = 8 * 1024 * 1024 + 4096;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
/// Hard admission ceilings for all unpublished archives in one database.
/// These do not cap retained published history or reserve filesystem capacity.
pub const MAX_PREPARATIONS: u32 = 32;
pub const MAX_PREPARATION_BYTES: u64 = 8 * 1024 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparationBudget {
    pub max_archives: u32,
    pub max_bytes: u64,
}
impl Default for PreparationBudget {
    fn default() -> Self {
        Self {
            max_archives: MAX_PREPARATIONS,
            max_bytes: MAX_PREPARATION_BYTES,
        }
    }
}
impl PreparationBudget {
    pub(crate) fn validate(self) -> Result<()> {
        if self.max_archives == 0
            || self.max_archives > MAX_PREPARATIONS
            || self.max_bytes == 0
            || self.max_bytes > MAX_PREPARATION_BYTES
        {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreparationUsage {
    pub archives: u32,
    /// Manifest original + container, plus every Part original + container.
    pub logical_bytes: u64,
}
/// All pending and published archive originals, independent of capture-State quotas.
pub const MAX_RETAINED_ARCHIVES: u32 = 8192;
pub const MAX_RETAINED_BYTES: u64 = 64 * 1024 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetentionBudget {
    pub max_archives: u32,
    pub max_bytes: u64,
}
impl Default for RetentionBudget {
    fn default() -> Self {
        Self {
            max_archives: MAX_RETAINED_ARCHIVES,
            max_bytes: MAX_RETAINED_BYTES,
        }
    }
}
impl RetentionBudget {
    pub(crate) fn validate(self) -> Result<()> {
        if self.max_archives == 0
            || self.max_archives > MAX_RETAINED_ARCHIVES
            || self.max_bytes == 0
            || self.max_bytes > MAX_RETAINED_BYTES
        {
            return Err(Error::Limit);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetentionUsage {
    pub archives: u64,
    /// Every Manifest and Part original protobuf plus its complete container.
    pub logical_bytes: u64,
}
const PART_MAGIC: &[u8; 8] = b"MRWAPRT1";
const MANIFEST_MAGIC: &[u8; 8] = b"MRWAMNF1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    pub max_parts: u32,
    /// Sum of original Part protobuf bytes plus each complete compressed container.
    pub max_bytes: u64,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            max_parts: MAX_PARTS,
            max_bytes: MAX_LOGICAL_BYTES,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub operation_id: String,
    pub subject: String,
    pub request_type: String,
    pub request: Vec<u8>,
    pub response_type: String,
    pub budget: Budget,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub plan: Plan,
    pub count: u32,
    /// Logical original-plus-container cost; deduplication never reduces this value.
    pub logical_bytes: u64,
    pub chain_sha256: [u8; 32],
    pub root: Option<[u8; 32]>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finish {
    pub response: Vec<u8>,
    pub part_count: u32,
    pub logical_bytes: u64,
    pub chain_sha256: [u8; 32],
    pub metadata_type: String,
    pub metadata: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Part {
    value: proto::Part,
    raw: Vec<u8>,
    container: Vec<u8>,
    digest: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct Manifest {
    pub(crate) value: proto::Manifest,
    plan: Plan,
    raw: Vec<u8>,
    container: Vec<u8>,
    digest: [u8; 32],
}
fn sha(raw: &[u8]) -> [u8; 32] {
    Sha256::digest(raw).into()
}
fn digest(raw: &[u8]) -> Result<[u8; 32]> {
    raw.try_into().map_err(|_| Error::Integrity)
}
impl Plan {
    pub(crate) fn raw(&self) -> Result<Vec<u8>> {
        for v in [
            &self.operation_id,
            &self.subject,
            &self.request_type,
            &self.response_type,
        ] {
            identity(v)?;
        }
        if self.request.len() > MAX_METADATA_BYTES
            || self.budget.max_parts == 0
            || self.budget.max_parts > MAX_PARTS
            || self.budget.max_bytes == 0
            || self.budget.max_bytes > MAX_LOGICAL_BYTES
        {
            return Err(Error::Limit);
        }
        Ok(proto::Plan {
            schema_version: 1,
            operation_id: self.operation_id.clone(),
            subject: self.subject.clone(),
            request_type: self.request_type.clone(),
            request: self.request.clone(),
            response_type: self.response_type.clone(),
            max_parts: self.budget.max_parts,
            max_bytes: self.budget.max_bytes,
        }
        .encode_to_vec())
    }
    pub(crate) fn decode(raw: &[u8]) -> Result<Self> {
        preflight(raw, Kind::Plan)?;
        let v = proto::Plan::decode(raw).map_err(|_| Error::Integrity)?;
        if v.schema_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        let result = Self {
            operation_id: v.operation_id,
            subject: v.subject,
            request_type: v.request_type,
            request: v.request,
            response_type: v.response_type,
            budget: Budget {
                max_parts: v.max_parts,
                max_bytes: v.max_bytes,
            },
        };
        result.raw()?;
        Ok(result)
    }
}
impl Part {
    pub fn data(&self) -> &[u8] {
        &self.value.data
    }
    pub fn type_id(&self) -> &str {
        &self.value.type_id
    }
    pub fn ordinal(&self) -> u32 {
        self.value.ordinal
    }
    pub fn previous_sha256(&self) -> [u8; 32] {
        self.value
            .previous_sha256
            .as_slice()
            .try_into()
            .expect("validated digest")
    }
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub(crate) fn new(
        ordinal: u32,
        type_id: &str,
        data: &[u8],
        previous: [u8; 32],
    ) -> Result<Self> {
        identity(type_id)?;
        if data.len() > MAX_PART_BYTES || ordinal >= MAX_PARTS {
            return Err(Error::Limit);
        }
        let value = proto::Part {
            schema_version: 1,
            ordinal,
            type_id: type_id.into(),
            data: data.to_vec(),
            data_sha256: sha(data).to_vec(),
            previous_sha256: previous.to_vec(),
        };
        let raw = value.encode_to_vec();
        let container = pack(PART_MAGIC, &raw)?;
        Ok(Self {
            value,
            digest: sha(&raw),
            raw,
            container,
        })
    }
    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = unpack(PART_MAGIC, container)?;
        preflight(&raw, Kind::Part)?;
        let value = proto::Part::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
        if value.schema_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        identity(&value.type_id)?;
        if value.ordinal >= MAX_PARTS || value.data.len() > MAX_PART_BYTES {
            return Err(Error::Limit);
        }
        if value.data_sha256.as_slice() != sha(&value.data) || value.previous_sha256.len() != 32 {
            return Err(Error::Integrity);
        }
        Ok(Self {
            value,
            digest: sha(&raw),
            raw,
            container: container.to_vec(),
        })
    }
}
impl Manifest {
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn status(&self) -> Status {
        Status {
            plan: self.plan.clone(),
            count: self.value.part_count,
            logical_bytes: self.value.logical_bytes,
            chain_sha256: digest(&self.value.chain_sha256).expect("validated digest"),
            root: self.value.finalized.as_ref().map(|_| self.digest),
        }
    }
    pub fn metadata_type(&self) -> Option<&str> {
        self.value
            .finalized
            .as_ref()
            .map(|v| v.metadata_type.as_str())
    }
    pub fn metadata(&self) -> Option<&[u8]> {
        self.value.finalized.as_ref().map(|v| v.metadata.as_slice())
    }
    pub fn response_sha256(&self) -> Option<[u8; 32]> {
        self.value
            .finalized
            .as_ref()
            .map(|v| digest(&v.response_sha256).expect("validated digest"))
    }
    pub(crate) fn new(plan: &Plan) -> Result<Self> {
        let raw = plan.raw()?;
        Self::from_value(proto::Manifest {
            schema_version: 1,
            chain_sha256: sha(&raw).to_vec(),
            plan: raw,
            part_count: 0,
            logical_bytes: 0,
            finalized: None,
            capture_binding: Vec::new(),
        })
    }
    pub(crate) fn for_capture(plan: &Plan, binding: [u8; 32]) -> Result<Self> {
        let mut value = Self::new(plan)?.value;
        value.capture_binding = binding.to_vec();
        Self::from_value(value)
    }
    pub fn capture_binding(&self) -> Option<[u8; 32]> {
        self.value.capture_binding.as_slice().try_into().ok()
    }
    pub(crate) fn append(&self, part: &Part) -> Result<Self> {
        if self.value.finalized.is_some() {
            return Err(Error::OperationConflict);
        }
        if part.ordinal() != self.value.part_count
            || part.previous_sha256() != digest(&self.value.chain_sha256)?
        {
            return Err(Error::OperationConflict);
        }
        let mut v = self.value.clone();
        v.part_count = v.part_count.checked_add(1).ok_or(Error::Limit)?;
        v.logical_bytes = v
            .logical_bytes
            .checked_add((part.raw().len() + part.container().len()) as u64)
            .ok_or(Error::Limit)?;
        v.chain_sha256 = part.digest().to_vec();
        Self::from_value(v)
    }
    pub(crate) fn finalize(&self, end: &Finish) -> Result<Self> {
        identity(&end.metadata_type)?;
        if end.metadata.len() > MAX_METADATA_BYTES || end.response.len() > MAX_METADATA_BYTES {
            return Err(Error::Limit);
        }
        if end.part_count != self.value.part_count
            || end.logical_bytes != self.value.logical_bytes
            || end.chain_sha256 != digest(&self.value.chain_sha256)?
        {
            return Err(Error::OperationConflict);
        }
        let mut v = self.value.clone();
        v.finalized = Some(proto::Final {
            metadata_type: end.metadata_type.clone(),
            metadata: end.metadata.clone(),
            response_sha256: sha(&end.response).to_vec(),
        });
        Self::from_value(v)
    }
    fn validate(value: &proto::Manifest) -> Result<Plan> {
        if value.schema_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        if !value.capture_binding.is_empty() && value.capture_binding.len() != 32 {
            return Err(Error::Integrity);
        }
        let plan = Plan::decode(&value.plan)?;
        if value.part_count > plan.budget.max_parts || value.logical_bytes > plan.budget.max_bytes {
            return Err(Error::Limit);
        }
        digest(&value.chain_sha256)?;
        if value.part_count == 0
            && (value.logical_bytes != 0 || value.chain_sha256.as_slice() != sha(&value.plan))
        {
            return Err(Error::Integrity);
        }
        if let Some(end) = &value.finalized {
            identity(&end.metadata_type)?;
            digest(&end.response_sha256)?;
            if end.metadata.len() > MAX_METADATA_BYTES {
                return Err(Error::Limit);
            }
        }
        Ok(plan)
    }
    fn from_value(value: proto::Manifest) -> Result<Self> {
        let plan = Self::validate(&value)?;
        let raw = value.encode_to_vec();
        let container = pack(MANIFEST_MAGIC, &raw)?;
        Ok(Self {
            value,
            plan,
            digest: sha(&raw),
            raw,
            container,
        })
    }
    pub fn decode(container: &[u8]) -> Result<Self> {
        let raw = unpack(MANIFEST_MAGIC, container)?;
        preflight(&raw, Kind::Manifest)?;
        let value = proto::Manifest::decode(raw.as_slice()).map_err(|_| Error::Integrity)?;
        let plan = Self::validate(&value)?;
        Ok(Self {
            value,
            plan,
            digest: sha(&raw),
            raw,
            container: container.to_vec(),
        })
    }
    pub(crate) fn initial_chain(&self) -> [u8; 32] {
        sha(&self.value.plan)
    }
}
#[derive(Clone, Copy)]
enum Kind {
    Plan,
    Part,
    Manifest,
    Final,
}
fn preflight(mut raw: &[u8], kind: Kind) -> Result<()> {
    use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field};
    if raw.len() > MAX_RAW_BYTES {
        return Err(Error::Limit);
    }
    let mut seen = [false; 9];
    let mut count = 0;
    while !raw.is_empty() {
        count += 1;
        if count > 128 {
            return Err(Error::Limit);
        }
        let (n, w) = decode_key(&mut raw).map_err(|_| Error::Integrity)?;
        let fields = match kind {
            Kind::Plan => 8,
            Kind::Part => 6,
            Kind::Manifest => 7,
            Kind::Final => 3,
        };
        if n <= fields {
            if seen[n as usize] {
                return Err(Error::Invalid("duplicate archive field"));
            }
            seen[n as usize] = true;
            let integer = match kind {
                Kind::Plan => matches!(n, 1 | 7 | 8),
                Kind::Part => matches!(n, 1 | 2),
                Kind::Manifest => matches!(n, 1 | 3 | 4),
                Kind::Final => false,
            };
            if w != if integer {
                WireType::Varint
            } else {
                WireType::LengthDelimited
            } {
                return Err(Error::Integrity);
            }
        }
        if w == WireType::LengthDelimited {
            let size = decode_varint(&mut raw).map_err(|_| Error::Integrity)?;
            let max = match (kind, n) {
                (Kind::Plan, 2 | 3 | 4 | 6) | (Kind::Part, 3) | (Kind::Final, 1) => 256,
                (Kind::Plan, 5) | (Kind::Final, 2) => MAX_METADATA_BYTES,
                (Kind::Part, 4) => MAX_PART_BYTES,
                (Kind::Part, 5 | 6) | (Kind::Manifest, 5 | 7) | (Kind::Final, 3) => 32,
                (Kind::Manifest, 2) => MAX_METADATA_BYTES + 2048,
                (Kind::Manifest, 6) => MAX_METADATA_BYTES + 1024,
                _ => MAX_RAW_BYTES,
            };
            if size > max as u64 {
                return Err(Error::Limit);
            }
            if size > raw.len() as u64 {
                return Err(Error::Integrity);
            }
            let value = &raw[..size as usize];
            match (kind, n) {
                (Kind::Manifest, 2) => preflight(value, Kind::Plan)?,
                (Kind::Manifest, 6) => preflight(value, Kind::Final)?,
                _ => {}
            }
            raw = &raw[size as usize..];
        } else if n <= fields && w == WireType::Varint {
            let value = decode_varint(&mut raw).map_err(|_| Error::Integrity)?;
            let is_u64 = matches!((kind, n), (Kind::Plan, 8) | (Kind::Manifest, 4));
            if !is_u64 && value > u64::from(u32::MAX) {
                return Err(Error::Limit);
            }
            if n == 1 && value != 1 {
                return Err(Error::UnsupportedVersion);
            }
        } else {
            if matches!(w, WireType::StartGroup | WireType::EndGroup) {
                return Err(Error::Integrity);
            }
            skip_field(w, n, &mut raw, DecodeContext::default()).map_err(|_| Error::Integrity)?;
        }
    }
    Ok(())
}
fn pack(magic: &[u8; 8], raw: &[u8]) -> Result<Vec<u8>> {
    if raw.len() > MAX_RAW_BYTES {
        return Err(Error::Limit);
    }
    let packed = lz4_flex::block::compress(raw);
    let mut out = Vec::with_capacity(50 + packed.len());
    out.extend_from_slice(magic);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    out.extend_from_slice(&sha(raw));
    out.extend_from_slice(&packed);
    if out.len() > MAX_CONTAINER_BYTES {
        return Err(Error::Limit);
    }
    Ok(out)
}
fn unpack(magic: &[u8; 8], container: &[u8]) -> Result<Vec<u8>> {
    if container.len() > MAX_CONTAINER_BYTES {
        return Err(Error::Limit);
    }
    if container.len() < 50 || &container[..8] != magic {
        return Err(Error::Integrity);
    }
    if container[8..10] != 1u16.to_le_bytes() {
        return Err(Error::UnsupportedVersion);
    }
    let len =
        u32::from_le_bytes(container[10..14].try_into().map_err(|_| Error::Integrity)?) as usize;
    let packed =
        u32::from_le_bytes(container[14..18].try_into().map_err(|_| Error::Integrity)?) as usize;
    if len > MAX_RAW_BYTES {
        return Err(Error::Limit);
    }
    if container.len() - 50 != packed {
        return Err(Error::Integrity);
    }
    let mut raw = vec![0; len];
    let actual = lz4_flex::block::decompress_into(&container[50..], &mut raw)
        .map_err(|_| Error::Integrity)?;
    if actual != len || container[18..50] != sha(&raw) {
        return Err(Error::Integrity);
    }
    Ok(raw)
}
