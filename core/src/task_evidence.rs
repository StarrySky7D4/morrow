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
pub const BACKEND: &str = "wasmi-1.1.0/morrow-pure-v1";
pub const MAX_RAW_BYTES: usize = MAX_PACKAGE_BYTES + 2 * MAX_TASK_BYTES + 4096;
pub const MAX_CONTAINER_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
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
// Check field count, individual lengths, wire types and duplicate known fields before allocation.
// Groups are forbidden; only the fixed budget submessage is parsed recursively.
fn preflight(mut bytes: &[u8], budget_message: bool, remaining: &mut usize) -> Result<()> {
    let mut seen = 0u16;
    while !bytes.is_empty() {
        *remaining = remaining.checked_sub(1).ok_or(Error::Limit)?;
        let (number, wire) = decode_key(&mut bytes).map_err(invalid)?;
        let known = number <= if budget_message { 3 } else { 10 };
        if known {
            let bit = 1u16 << number;
            if seen & bit != 0 {
                return Err(Error::Invalid("duplicate evidence field"));
            }
            seen |= bit;
            let expected = if !budget_message && (2..=6).contains(&number) {
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
            let limit = if budget_message {
                256
            } else {
                match number {
                    2 => MAX_PACKAGE_BYTES,
                    3 | 6 => MAX_TASK_BYTES,
                    4 => 256,
                    5 => 128,
                    _ => MAX_RAW_BYTES,
                }
            };
            if len > limit {
                return Err(Error::Limit);
            }
            if len > bytes.len() {
                return Err(Error::Invalid("evidence field length"));
            }
            let (value, tail) = bytes.split_at(len);
            bytes = tail;
            if !budget_message && number == 4 {
                preflight(value, true, remaining)?;
            }
        } else {
            if matches!(wire, WireType::StartGroup | WireType::EndGroup) {
                return Err(Error::Invalid("evidence group"));
            }
            skip_field(wire, number, &mut bytes, DecodeContext::default()).map_err(invalid)?;
        }
    }
    Ok(())
}
/// Validate a replay plan before any execution; this confers no live host permissions.
pub fn validate_plan(
    package: &Package,
    invocation: &Invocation,
    budget: &proto::ExecutionBudget,
) -> Result<()> {
    if package.archive().len() > MAX_PACKAGE_BYTES || invocation.bytes().len() > MAX_TASK_BYTES {
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
fn validate(data: &proto::TaskEvidence) -> Result<()> {
    if data.schema_version != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    if data.package_archive.len() > MAX_PACKAGE_BYTES
        || data.invocation.len() > MAX_TASK_BYTES
        || data.completion.len() > MAX_TASK_BYTES
    {
        return Err(Error::Limit);
    }
    if data.backend.is_empty()
        || data.backend.len() > 128
        || !data.backend.bytes().all(|b| b.is_ascii_graphic())
    {
        return Err(Error::Invalid("evidence backend"));
    }
    let package = Package::decode(&data.package_archive)?;
    let invocation = Invocation::decode(&data.invocation)?;
    let budget = data
        .budget
        .as_ref()
        .ok_or(Error::Invalid("evidence budget"))?;
    validate_plan(&package, &invocation, budget)?;
    let registration = package.transform_handler(
        invocation
            .transform()
            .ok_or(Error::Invalid("evidence transform"))?,
    )?;
    if data.observed_host_calls > budget.host_calls || data.fuel_remaining > budget.fuel {
        return Err(Error::Limit);
    }
    let fault = proto::StableFault::try_from(data.fault).map_err(|_| Error::UnsupportedVersion)?;
    if fault == proto::StableFault::Unspecified {
        if data.completion.is_empty() || data.exit_code.is_none() {
            return Err(Error::Invalid("missing successful observation"));
        }
        if let TransformResult::Output(output) =
            invocation.verify_transform_result(&data.completion)?
            && output.bytes.len() > registration.max_output_bytes as usize
        {
            return Err(Error::Limit);
        }
    } else if data.exit_code.is_some() {
        return Err(Error::Invalid("mixed task observation"));
    }
    Ok(())
}
pub fn encode(data: proto::TaskEvidence) -> Result<Evidence> {
    if data.encoded_len() > MAX_RAW_BYTES {
        return Err(Error::Limit);
    }
    validate(&data)?;
    let raw = data.encode_to_vec();
    let digest = Sha256::digest(&raw).into();
    let container = envelope::pack(MAGIC, &raw, MAX_RAW_BYTES)?;
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
    preflight(&raw, false, &mut 128)?;
    let data = proto::TaskEvidence::decode(raw.as_slice()).map_err(invalid)?;
    validate(&data)?;
    Ok(Evidence {
        raw,
        digest,
        container: container.to_vec(),
        data,
    })
}
