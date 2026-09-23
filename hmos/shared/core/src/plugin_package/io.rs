//! Validated IO declarations, independent of content permissions and live grants.
use crate::{Error, Result, identity};
use std::collections::BTreeSet;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.io.v1.rs"));
}
pub const FEATURE: &str = "io-v1";
pub const VERSION: u32 = 1;
pub const DECLARATION_VERSION: u32 = 1;
pub const MAX_CAPABILITIES: usize = 10;
pub const MAX_HANDLERS: usize = 16;
pub const MAX_RESOURCES: u32 = 8;
pub const MAX_JOBS: u32 = 4;
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_JOB_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_DURATION_MS: u64 = 30_000;
pub const SERVICE_RUN_FEATURE: &str = "service-run-v1";
pub const SERVICE_RUN_VERSION: u32 = 1;
pub const SERVICE_RUN_BUDGET_FEATURE: &str = "service-run-budget-v1";
pub const SERVICE_RUN_BUDGET_VERSION: u32 = 1;
/// Finite experimental run ceiling, not a measured throughput guarantee.
pub const MAX_SERVICE_RUN_JOBS: u64 = 1_000_000;
/// Experimental finite service lifetime ceiling, not a live authorization or request timeout.
pub const MAX_SERVICE_RUN_DURATION_MS: u64 = 3_600_000;

/// Names and numbers belong only to the experimental IO profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum IoCapability {
    FileRead = 1,
    FileList = 2,
    FileCreate = 3,
    FileReplace = 4,
    FileDelete = 5,
    HttpRequest = 6,
    HttpListen = 7,
    HttpPublish = 8,
    CredentialUse = 9,
    WebSocketConnect = 10,
}
impl IoCapability {
    pub fn from_number(value: i32) -> Result<Self> {
        match value {
            1 => Ok(Self::FileRead),
            2 => Ok(Self::FileList),
            3 => Ok(Self::FileCreate),
            4 => Ok(Self::FileReplace),
            5 => Ok(Self::FileDelete),
            6 => Ok(Self::HttpRequest),
            7 => Ok(Self::HttpListen),
            8 => Ok(Self::HttpPublish),
            9 => Ok(Self::CredentialUse),
            10 => Ok(Self::WebSocketConnect),
            0 => Err(Error::Invalid("unspecified IO capability")),
            _ => Err(Error::UnsupportedVersion),
        }
    }
    pub fn number(self) -> i32 {
        self as i32
    }
}
pub fn schema_digest() -> [u8; 32] {
    crate::runtime::schema_digest(include_bytes!("../../schemas/io.capnp"))
}
/// Convenience metadata only. Package validation is still required; this grants nothing.
pub fn declaration(capabilities: Vec<IoCapability>, handlers: Vec<String>) -> proto::IoDeclaration {
    proto::IoDeclaration {
        service_run: None,
        service_schema_sha256: Vec::new(),
        schema_version: DECLARATION_VERSION,
        io_version: VERSION,
        io_schema_sha256: schema_digest().to_vec(),
        requested_capabilities: capabilities.into_iter().map(IoCapability::number).collect(),
        handlers,
        budget: Some(proto::IoBudget {
            max_resources: MAX_RESOURCES,
            max_jobs: MAX_JOBS,
            max_bytes: MAX_BYTES,
            max_job_bytes: MAX_JOB_BYTES,
            max_duration_ms: MAX_DURATION_MS,
        }),
    }
}
pub(crate) fn validate(value: &proto::IoDeclaration) -> Result<BTreeSet<IoCapability>> {
    if value.schema_version != DECLARATION_VERSION
        || value.io_version != VERSION
        || value.io_schema_sha256 != schema_digest()
    {
        return Err(Error::UnsupportedVersion);
    }
    if value.requested_capabilities.is_empty() || value.handlers.is_empty() {
        return Err(Error::Invalid("empty IO declaration"));
    }
    if value.requested_capabilities.len() > MAX_CAPABILITIES || value.handlers.len() > MAX_HANDLERS
    {
        return Err(Error::Limit);
    }
    let mut capabilities = BTreeSet::new();
    for raw in &value.requested_capabilities {
        if !capabilities.insert(IoCapability::from_number(*raw)?) {
            return Err(Error::Invalid("duplicate IO capability"));
        }
    }
    if !value.service_schema_sha256.is_empty() {
        if value.service_schema_sha256 != crate::service::schema_digest() {
            return Err(Error::UnsupportedVersion);
        }
        if !capabilities.contains(&IoCapability::HttpPublish) {
            return Err(Error::Invalid("service requires HTTP publish"));
        }
    }
    if let Some(profile) = &value.service_run {
        if profile.schema_version != SERVICE_RUN_VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if profile.max_duration_ms == 0 || profile.max_duration_ms > MAX_SERVICE_RUN_DURATION_MS {
            return Err(Error::Limit);
        }
        if !capabilities.contains(&IoCapability::HttpListen)
            || !capabilities.contains(&IoCapability::HttpPublish)
            || value.service_schema_sha256.is_empty()
        {
            return Err(Error::Invalid(
                "service run requires listen, publish and service schema",
            ));
        }
    }
    let mut handlers = BTreeSet::new();
    for handler in &value.handlers {
        identity(handler)?;
        if !handlers.insert(handler) {
            return Err(Error::Invalid("duplicate IO handler"));
        }
    }
    let budget = value
        .budget
        .as_ref()
        .ok_or(Error::Invalid("missing IO budget"))?;
    if budget.max_resources == 0
        || budget.max_resources > MAX_RESOURCES
        || budget.max_jobs == 0
        || budget.max_jobs > MAX_JOBS
        || budget.max_bytes == 0
        || budget.max_bytes > MAX_BYTES
        || budget.max_job_bytes == 0
        || budget.max_job_bytes > MAX_JOB_BYTES
        || budget.max_job_bytes > budget.max_bytes
        || budget.max_duration_ms == 0
        || budget.max_duration_ms > MAX_DURATION_MS
    {
        return Err(Error::Limit);
    }
    if let Some(run_budget) = value
        .service_run
        .as_ref()
        .and_then(|profile| profile.budget.as_ref())
    {
        if run_budget.schema_version != SERVICE_RUN_BUDGET_VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if run_budget.max_jobs == 0
            || run_budget.max_jobs > MAX_SERVICE_RUN_JOBS
            || run_budget.max_bytes == 0
            || run_budget.max_bytes > budget.max_bytes
        {
            return Err(Error::Limit);
        }
    }
    Ok(capabilities)
}

/// The new privileged declaration has one canonical encoding. Older manifest fields and
/// unknown optional metadata remain byte-preserved; only field 18 is constrained here.
pub(crate) fn validate_wire(
    mut manifest: &[u8],
    value: Option<&proto::IoDeclaration>,
) -> Result<()> {
    use prost::{
        Message,
        encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field},
    };
    let mut seen = false;
    while !manifest.is_empty() {
        let (number, wire) =
            decode_key(&mut manifest).map_err(|_| Error::Invalid("manifest key"))?;
        if number != 18 {
            skip_field(wire, number, &mut manifest, DecodeContext::default())
                .map_err(|_| Error::Invalid("manifest field"))?;
            continue;
        }
        if seen || wire != WireType::LengthDelimited {
            return Err(Error::Invalid("duplicate or malformed IO declaration"));
        }
        seen = true;
        let length =
            decode_varint(&mut manifest).map_err(|_| Error::Invalid("IO declaration length"))?;
        if length > manifest.len() as u64 {
            return Err(Error::Invalid("IO declaration length"));
        }
        let (bytes, rest) = manifest.split_at(length as usize);
        manifest = rest;
        let value = value.ok_or(Error::Invalid("missing IO declaration"))?;
        if value.encode_to_vec() != bytes {
            return Err(Error::Invalid("noncanonical IO declaration"));
        }
    }
    if seen != value.is_some() {
        return Err(Error::Invalid("IO declaration presence"));
    }
    Ok(())
}
