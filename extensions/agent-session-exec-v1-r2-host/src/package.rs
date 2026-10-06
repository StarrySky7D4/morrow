use morrow_agent_session_exec_v1_r2::{
    Error, Result,
    authority::{Admission, Capabilities, SessionExecHost},
    hash, schema_digest,
};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    plugin_package::Package,
};
use morrow_plugin_runtime::{Cancellation, Limits, Runner, TaskRun};
use prost::Message;
use std::collections::BTreeSet;
const MAGIC: &[u8; 8] = b"MROWARE2";
const MAX_RAW: usize = morrow_core::plugin_package::MAX_PACKAGE_BYTES + 16 * 1024;
const HEADER: usize = 50;
#[derive(Clone, PartialEq, Message)]
struct Envelope {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(uint32, tag = "2")]
    revision: u32,
    #[prost(bytes = "vec", tag = "3")]
    schema: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    package: Vec<u8>,
    #[prost(uint32, tag = "5")]
    capabilities: u32,
    #[prost(string, repeated, tag = "6")]
    sessions: Vec<String>,
    #[prost(string, tag = "7")]
    execution_domain: String,
}
#[derive(Clone)]
pub struct Declaration {
    pub capabilities: Capabilities,
    pub sessions: Vec<String>,
    pub execution_domain: String,
}
fn bits(c: Capabilities) -> u32 {
    u32::from(c.session_read)
        | u32::from(c.session_write) << 1
        | u32::from(c.propose) << 2
        | u32::from(c.execute) << 3
        | u32::from(c.retire) << 4
}
fn capabilities(v: u32) -> Result<Capabilities> {
    if v & !31 != 0 {
        return Err(Error::Contract);
    }
    Ok(Capabilities {
        session_read: v & 1 != 0,
        session_write: v & 2 != 0,
        propose: v & 4 != 0,
        execute: v & 8 != 0,
        retire: v & 16 != 0,
    })
}
fn valid_identity(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && !s
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}
impl Declaration {
    fn validate(&self) -> Result<()> {
        if bits(self.capabilities) == 0
            || !valid_identity(&self.execution_domain)
            || self.sessions.is_empty()
            || self.sessions.len() > 16
            || self.sessions.iter().any(|s| !valid_identity(s))
            || self.sessions.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
/// A separate R2 package format embeds the original immutable Wasm Package
/// and its exact profile declaration; legacy factories do not negotiate it.
pub struct AgentPackage {
    base: Package,
    declaration: Declaration,
    archive: Vec<u8>,
}
impl AgentPackage {
    pub fn build(base: Package, declaration: Declaration) -> Result<Self> {
        declaration.validate()?;
        let raw = Envelope {
            version: 1,
            revision: 2,
            schema: schema_digest().to_vec(),
            package: base.archive().to_vec(),
            capabilities: bits(declaration.capabilities),
            sessions: declaration.sessions.clone(),
            execution_domain: declaration.execution_domain.clone(),
        }
        .encode_to_vec();
        if raw.len() > MAX_RAW {
            return Err(Error::Limit);
        }
        let compressed = lz4_flex::block::compress(&raw);
        let mut archive = Vec::with_capacity(HEADER + compressed.len());
        archive.extend_from_slice(MAGIC);
        archive.extend_from_slice(&1u16.to_le_bytes());
        archive.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        archive.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        archive.extend_from_slice(&hash(&raw));
        archive.extend_from_slice(&compressed);
        Self::decode(&archive)
    }
    pub fn decode(archive: &[u8]) -> Result<Self> {
        if archive.len() > MAX_RAW + MAX_RAW / 255 + 128 {
            return Err(Error::Limit);
        }
        if archive.len() < HEADER || &archive[..8] != MAGIC || archive[8..10] != 1u16.to_le_bytes()
        {
            return Err(Error::Contract);
        }
        let raw_len = u32::from_le_bytes(archive[10..14].try_into().unwrap()) as usize;
        let compressed_len = u32::from_le_bytes(archive[14..18].try_into().unwrap()) as usize;
        if raw_len > MAX_RAW || compressed_len != archive.len() - HEADER {
            return Err(Error::Limit);
        }
        let mut raw = vec![0; raw_len];
        if lz4_flex::block::decompress_into(&archive[HEADER..], &mut raw)
            .map_err(|_| Error::Invalid)?
            != raw_len
            || hash(&raw).as_slice() != &archive[18..50]
        {
            return Err(Error::Invalid);
        }
        let value = Envelope::decode(raw.as_slice()).map_err(|_| Error::Invalid)?;
        if value.encode_to_vec() != raw
            || value.version != 1
            || value.revision != 2
            || value.schema != schema_digest()
        {
            return Err(Error::Contract);
        }
        let base = Package::decode(&value.package).map_err(|_| Error::Contract)?;
        let manifest = base.manifest();
        if manifest.guest_abi_version != 2
            || !manifest.required_features.is_empty()
            || !manifest.requested_capabilities.is_empty()
            || !manifest.transform_handlers.is_empty()
            || base.io_declaration().is_some()
            || base.channel_declaration().is_some()
            || base.mutation_enabled()
        {
            return Err(Error::Denied);
        }
        let declaration = Declaration {
            capabilities: capabilities(value.capabilities)?,
            sessions: value.sessions,
            execution_domain: value.execution_domain,
        };
        declaration.validate()?;
        Ok(Self {
            base,
            declaration,
            archive: archive.to_vec(),
        })
    }
    pub fn archive(&self) -> &[u8] {
        &self.archive
    }
    pub fn review_sha256(&self) -> [u8; 32] {
        hash(&self.archive)
    }
    pub fn declaration(&self) -> &Declaration {
        &self.declaration
    }
}
pub struct PreparedAgentPackage {
    package: AgentPackage,
    runner: Runner,
}
/// Opaque original connection approval, never restored from wire.
pub struct ApprovedSession {
    pub(crate) connection: Connection,
    pub(crate) admission: Admission,
    package: [u8; 32],
}
impl ApprovedSession {
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
    pub fn admission(&self) -> &Admission {
        &self.admission
    }
    pub fn close(self, runtime: &mut HostRuntime, host: &SessionExecHost) -> Result<()> {
        host.revoke(&self.admission)?;
        runtime
            .disconnect(&self.connection)
            .map_err(|_| Error::Storage)
    }
}
impl PreparedAgentPackage {
    pub fn new(package: AgentPackage, limits: Limits) -> Result<Self> {
        if limits.fuel == 0
            || limits.fuel > 100_000_000
            || limits.memory_bytes < 65536
            || limits.memory_bytes > 64 * 1024 * 1024
            || limits.host_calls > 1024
        {
            return Err(Error::Limit);
        }
        let budget = package
            .base
            .manifest()
            .budget
            .as_ref()
            .ok_or(Error::Invalid)?;
        let limits = Limits {
            fuel: limits.fuel.min(budget.fuel),
            memory_bytes: limits.memory_bytes.min(budget.memory_bytes as usize),
            host_calls: limits.host_calls.min(budget.host_calls),
        };
        let runner = Runner::new_agent_session_exec_task(package.base.module(), limits)
            .map_err(|_| Error::Contract)?;
        Ok(Self { package, runner })
    }
    pub fn package(&self) -> &AgentPackage {
        &self.package
    }
    #[allow(clippy::too_many_arguments)]
    pub fn approve(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        review_sha256: [u8; 32],
        approved: Capabilities,
        sessions: Vec<String>,
        expires: u64,
        now: u64,
    ) -> Result<ApprovedSession> {
        let d = &self.package.declaration;
        let requested = bits(approved);
        let scope: BTreeSet<_> = sessions.iter().collect();
        if review_sha256 != self.package.review_sha256()
            || requested & !bits(d.capabilities) != 0
            || requested == 0
            || sessions.is_empty()
            || scope.len() != sessions.len()
            || sessions.iter().any(|s| !d.sessions.contains(s))
        {
            return Err(Error::Denied);
        }
        let connection = runtime
            .connect_package(&self.package.base)
            .map_err(|_| Error::Denied)?;
        let admission = match host.admit(
            runtime,
            &connection,
            d.capabilities,
            approved,
            sessions,
            d.execution_domain.clone(),
            expires,
            now,
        ) {
            Ok(admission) => admission,
            Err(error) => {
                let _ = runtime.disconnect(&connection);
                return Err(error);
            }
        };
        Ok(ApprovedSession {
            connection,
            admission,
            package: review_sha256,
        })
    }
    pub fn run(
        &self,
        runtime: &mut HostRuntime,
        host: &SessionExecHost,
        approved: &ApprovedSession,
        input: &[u8],
        clock: impl FnMut() -> u64,
        cancel: Cancellation,
    ) -> Result<TaskRun> {
        if approved.package != self.package.review_sha256() {
            return Err(Error::Denied);
        }
        let mut clock = clock;
        let mut route = |bytes: &[u8]| {
            crate::native::exchange(runtime, host, approved, bytes, &mut clock).map_err(|_| ())
        };
        Ok(self
            .runner
            .run_agent_session_exec_task(input, &mut route, cancel))
    }
}
