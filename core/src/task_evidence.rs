//! Self-contained bounded pure-task observations. Integrity is neither authorship nor permission.
//! Unknown optional protobuf fields remain in `raw`; never re-encode `data` to verify its digest.
use crate::{
    Error, Result, envelope,
    plugin_package::{DEPENDENCY_CALLS_FEATURE, MAX_PACKAGE_BYTES, Package},
    task::{Invocation, MAX_TASK_BYTES, TransformResult},
};
use prost::{
    Message,
    encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
};
use sha2::{Digest, Sha256};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.task_evidence.v1.rs"));
}
pub const VERSION: u32 = 1;
pub const BATCH_VERSION: u32 = 2;
pub const BACKEND: &str = "wasmi-1.1.0/morrow-pure-v1";
pub const MAX_SINGLE_RAW_BYTES: usize = MAX_PACKAGE_BYTES + 2 * MAX_TASK_BYTES + 4096;
pub const MAX_RAW_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
pub const MAX_BATCH_OBSERVATIONS: usize = 1024;
pub const MAX_INTENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TOTAL_FUEL: u64 = 10_000_000_000;
const MAX_FIELDS: usize = 128;
const MAX_BATCH_FIELDS: usize = MAX_FIELDS * (MAX_BATCH_OBSERVATIONS + 2);
const MAGIC: &[u8; 8] = b"MORROWE1";
#[derive(Debug, Clone)]
pub struct Evidence {
    raw: Vec<u8>,
    digest: [u8; 32],
    container: Vec<u8>,
    data: proto::TaskEvidence,
}
impl Evidence {
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn container(&self) -> &[u8] {
        &self.container
    }
    pub fn data(&self) -> &proto::TaskEvidence {
        &self.data
    }
}
fn invalid<T>(_: T) -> Error {
    Error::Invalid("task evidence protobuf")
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Evidence,
    Budget,
    Batch,
    Observation,
}
impl Kind {
    fn last_field(self) -> u32 {
        match self {
            Self::Evidence => 11,
            Self::Budget => 3,
            Self::Batch => 4,
            Self::Observation => 8,
        }
    }
    fn delimited(self, field: u32) -> bool {
        match self {
            Self::Evidence => (2..=6).contains(&field) || field == 11,
            Self::Budget => false,
            Self::Batch => field != 3,
            Self::Observation => field <= 4,
        }
    }
    fn span_limit(self, field: u32) -> usize {
        match (self, field) {
            (Self::Evidence, 2) => MAX_PACKAGE_BYTES,
            (Self::Evidence, 3 | 6) | (Self::Observation, 1 | 4) => MAX_TASK_BYTES,
            (Self::Evidence, 4) | (Self::Observation, 2) => 256,
            (Self::Evidence, 5) | (Self::Observation, 3) | (Self::Batch, 1) => 128,
            (Self::Batch, 2) => MAX_INTENT_BYTES,
            (Self::Batch, 4) => 2 * MAX_TASK_BYTES + 1024,
            (Self::Budget, _) => 256,
            _ => MAX_RAW_BYTES,
        }
    }
    fn nested(self, field: u32) -> Option<Self> {
        match (self, field) {
            (Self::Evidence, 4) | (Self::Observation, 2) => Some(Self::Budget),
            (Self::Evidence, 11) => Some(Self::Batch),
            (Self::Batch, 4) => Some(Self::Observation),
            _ => None,
        }
    }
    fn is_u32(self, field: u32) -> bool {
        matches!(
            (self, field),
            (Self::Evidence, 1 | 7 | 8 | 9) | (Self::Budget, 3) | (Self::Observation, 5..=7)
        )
    }
}
// Validate all nested lengths/counts before prost allocates. Only these four fixed levels exist;
// groups are forbidden and opaque unknown fields are never traversed as messages.
fn preflight(mut bytes: &[u8], kind: Kind, remaining: &mut usize) -> Result<Option<u32>> {
    let mut seen = 0u16;
    let mut count = 0usize;
    let mut observations = 0usize;
    let mut version = None;
    while !bytes.is_empty() {
        *remaining = remaining.checked_sub(1).ok_or(Error::Limit)?;
        count += 1;
        if count
            > if kind == Kind::Batch {
                MAX_BATCH_OBSERVATIONS + MAX_FIELDS
            } else {
                MAX_FIELDS
            }
        {
            return Err(Error::Limit);
        }
        let (number, wire) = decode_key(&mut bytes).map_err(invalid)?;
        let known = number <= kind.last_field();
        if known {
            let repeated = kind == Kind::Batch && number == 4;
            if repeated {
                observations += 1;
                if observations > MAX_BATCH_OBSERVATIONS {
                    return Err(Error::Limit);
                }
            } else {
                let bit = 1u16 << number;
                if seen & bit != 0 {
                    return Err(Error::Invalid("duplicate evidence field"));
                }
                seen |= bit;
            }
            let expected = if kind.delimited(number) {
                WireType::LengthDelimited
            } else {
                WireType::Varint
            };
            if wire != expected {
                return Err(Error::Invalid("evidence wire type"));
            }
        }
        if wire == WireType::LengthDelimited {
            let len = usize::try_from(decode_varint(&mut bytes).map_err(invalid)?)
                .map_err(|_| Error::Limit)?;
            if len > kind.span_limit(number) {
                return Err(Error::Limit);
            }
            if len > bytes.len() {
                return Err(Error::Invalid("evidence field length"));
            }
            let (value, tail) = bytes.split_at(len);
            bytes = tail;
            if let Some(nested) = kind.nested(number) {
                preflight(value, nested, remaining)?;
            }
        } else if known && wire == WireType::Varint {
            let value = decode_varint(&mut bytes).map_err(invalid)?;
            if kind.is_u32(number) && value > u64::from(u32::MAX) {
                return Err(Error::Limit);
            }
            if kind == Kind::Evidence && number == 1 {
                version = Some(value as u32);
            }
        } else {
            if matches!(wire, WireType::StartGroup | WireType::EndGroup) {
                return Err(Error::Invalid("evidence group"));
            }
            skip_field(wire, number, &mut bytes, DecodeContext::default()).map_err(invalid)?;
        }
    }
    Ok(version)
}
fn raw_limit(version: u32) -> Result<usize> {
    match version {
        VERSION => Ok(MAX_SINGLE_RAW_BYTES),
        BATCH_VERSION => Ok(MAX_RAW_BYTES),
        _ => Err(Error::UnsupportedVersion),
    }
}
fn validate_package(package: &Package) -> Result<()> {
    if package.archive().len() > MAX_PACKAGE_BYTES {
        return Err(Error::Limit);
    }
    if package.manifest().guest_abi_version != 2
        || !package.manifest().dependencies.is_empty()
        || package
            .manifest()
            .required_features
            .iter()
            .any(|f| f == DEPENDENCY_CALLS_FEATURE)
    {
        return Err(Error::Invalid("evidence requires leaf task ABI"));
    }
    Ok(())
}
/// Validate a replay plan before any execution; this confers no live host permissions.
pub fn validate_plan(
    package: &Package,
    invocation: &Invocation,
    budget: &proto::ExecutionBudget,
) -> Result<()> {
    validate_package(package)?;
    if invocation.bytes().len() > MAX_TASK_BYTES {
        return Err(Error::Limit);
    }
    let transform = invocation
        .transform()
        .ok_or(Error::Invalid("evidence requires pure transform"))?;
    package.transform_handler(transform)?;
    let declared = package
        .manifest()
        .budget
        .as_ref()
        .ok_or(Error::Invalid("package budget"))?;
    if budget.fuel == 0
        || budget.fuel > 100_000_000
        || budget.fuel > declared.fuel
        || budget.memory_bytes < 65536
        || budget.memory_bytes > 64 * 1024 * 1024
        || budget.memory_bytes > declared.memory_bytes
        || budget.host_calls > 1024
        || budget.host_calls > declared.host_calls
    {
        return Err(Error::Limit);
    }
    Ok(())
}
/// Validate batch metadata before capture. Every individual invocation still needs validate_plan.
/// Intent bytes are historical data, not proof of their relation to any content submission.
pub fn validate_batch_plan(
    package: &Package,
    intent_type: &str,
    intent: &[u8],
    total_fuel: u64,
) -> Result<()> {
    validate_package(package)?;
    if intent_type.is_empty()
        || intent_type.len() > 128
        || !intent_type.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(Error::Invalid("evidence intent type"));
    }
    if intent.len() > MAX_INTENT_BYTES || total_fuel == 0 || total_fuel > MAX_TOTAL_FUEL {
        return Err(Error::Limit);
    }
    Ok(())
}
struct ObservationRef<'a> {
    invocation: &'a [u8],
    budget: Option<&'a proto::ExecutionBudget>,
    backend: &'a str,
    completion: &'a [u8],
    fault: i32,
    exit_code: Option<i32>,
    observed_host_calls: u32,
    fuel_remaining: u64,
}
impl<'a> From<&'a proto::Observation> for ObservationRef<'a> {
    fn from(value: &'a proto::Observation) -> Self {
        Self {
            invocation: &value.invocation,
            budget: value.budget.as_ref(),
            backend: &value.backend,
            completion: &value.completion,
            fault: value.fault,
            exit_code: value.exit_code,
            observed_host_calls: value.observed_host_calls,
            fuel_remaining: value.fuel_remaining,
        }
    }
}
impl<'a> From<&'a proto::TaskEvidence> for ObservationRef<'a> {
    fn from(value: &'a proto::TaskEvidence) -> Self {
        Self {
            invocation: &value.invocation,
            budget: value.budget.as_ref(),
            backend: &value.backend,
            completion: &value.completion,
            fault: value.fault,
            exit_code: value.exit_code,
            observed_host_calls: value.observed_host_calls,
            fuel_remaining: value.fuel_remaining,
        }
    }
}
fn validate_observation_inner(
    package: &Package,
    data: ObservationRef<'_>,
    success_only: bool,
) -> Result<()> {
    if data.invocation.len() > MAX_TASK_BYTES || data.completion.len() > MAX_TASK_BYTES {
        return Err(Error::Limit);
    }
    if data.backend.is_empty()
        || data.backend.len() > 128
        || !data.backend.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(Error::Invalid("evidence backend"));
    }
    let invocation = Invocation::decode(data.invocation)?;
    let budget = data.budget.ok_or(Error::Invalid("evidence budget"))?;
    validate_plan(package, &invocation, budget)?;
    let registration = package.transform_handler(
        invocation
            .transform()
            .ok_or(Error::Invalid("evidence transform"))?,
    )?;
    if data.observed_host_calls > budget.host_calls || data.fuel_remaining > budget.fuel {
        return Err(Error::Limit);
    }
    let fault = proto::StableFault::try_from(data.fault).map_err(|_| Error::UnsupportedVersion)?;
    if success_only && (fault != proto::StableFault::Unspecified || data.exit_code != Some(0)) {
        return Err(Error::Invalid("batch observation must succeed"));
    }
    if fault == proto::StableFault::Unspecified {
        if data.completion.is_empty() || data.exit_code.is_none() {
            return Err(Error::Invalid("missing successful observation"));
        }
        match invocation.verify_transform_result(data.completion)? {
            TransformResult::Output(output)
                if output.bytes.len() > registration.max_output_bytes as usize =>
            {
                return Err(Error::Limit);
            }
            TransformResult::Failure(_) if success_only => {
                return Err(Error::Invalid("batch observation failed"));
            }
            _ => {}
        }
    } else if data.exit_code.is_some() {
        return Err(Error::Invalid("mixed task observation"));
    }
    Ok(())
}
/// Validate one successful version-2 observation against the shared package.
/// This does not replace ordered batch fuel accounting or prove the observation was executed.
pub fn validate_observation(package: &Package, data: &proto::Observation) -> Result<()> {
    validate_observation_inner(package, data.into(), true)
}
fn validate(data: &proto::TaskEvidence) -> Result<()> {
    raw_limit(data.schema_version)?;
    if data.package_archive.len() > MAX_PACKAGE_BYTES {
        return Err(Error::Limit);
    }
    let package = Package::decode(&data.package_archive)?;
    if data.schema_version == VERSION {
        if data.batch.is_some() {
            return Err(Error::Invalid("batch in single evidence"));
        }
        return validate_observation_inner(&package, data.into(), false);
    }
    if !data.invocation.is_empty()
        || data.budget.is_some()
        || !data.backend.is_empty()
        || !data.completion.is_empty()
        || data.fault != 0
        || data.exit_code.is_some()
        || data.observed_host_calls != 0
        || data.fuel_remaining != 0
    {
        return Err(Error::Invalid("mixed batch evidence"));
    }
    let batch = data
        .batch
        .as_ref()
        .ok_or(Error::Invalid("missing evidence batch"))?;
    validate_batch_plan(
        &package,
        &batch.intent_type,
        &batch.intent,
        batch.total_fuel,
    )?;
    if batch.observations.is_empty() || batch.observations.len() > MAX_BATCH_OBSERVATIONS {
        return Err(Error::Limit);
    }
    let mut remaining = batch.total_fuel;
    for observation in &batch.observations {
        validate_observation(&package, observation)?;
        let budget = observation
            .budget
            .as_ref()
            .ok_or(Error::Invalid("evidence budget"))?;
        if budget.fuel > remaining {
            return Err(Error::Limit);
        }
        remaining = remaining
            .checked_sub(budget.fuel - observation.fuel_remaining)
            .ok_or(Error::Limit)?;
    }
    Ok(())
}
pub fn encode(data: proto::TaskEvidence) -> Result<Evidence> {
    let limit = raw_limit(data.schema_version)?;
    if data.encoded_len() > limit {
        return Err(Error::Limit);
    }
    validate(&data)?;
    let raw = data.encode_to_vec();
    let digest = Sha256::digest(&raw).into();
    let container = envelope::pack(MAGIC, &raw, limit)?;
    Ok(Evidence {
        raw,
        digest,
        container,
        data,
    })
}
/// `expected_digest` must come from the trusted evidence reference, not its untrusted container.
/// This checks byte integrity only; callers separately decide whether to execute a replay.
pub fn decode(container: &[u8], expected_digest: [u8; 32]) -> Result<Evidence> {
    let raw = envelope::unpack(MAGIC, container, MAX_RAW_BYTES)?;
    let digest: [u8; 32] = Sha256::digest(&raw).into();
    if digest != expected_digest {
        return Err(Error::Integrity);
    }
    let mut fields = MAX_BATCH_FIELDS;
    let version = preflight(&raw, Kind::Evidence, &mut fields)?.unwrap_or_default();
    if raw.len() > raw_limit(version)?
        || (version == VERSION && MAX_BATCH_FIELDS - fields > MAX_FIELDS)
    {
        return Err(Error::Limit);
    }
    let data = proto::TaskEvidence::decode(raw.as_slice()).map_err(invalid)?;
    validate(&data)?;
    Ok(Evidence {
        raw,
        digest,
        container: container.to_vec(),
        data,
    })
}
