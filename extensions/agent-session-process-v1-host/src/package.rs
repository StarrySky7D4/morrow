use crate::*;
use prost::Message;
const MAGIC: &[u8; 8] = b"MROWASP1";
const HEADER: usize = 50;
const MAX_RAW: usize = morrow_core::plugin_package::MAX_PACKAGE_BYTES + 16 * 1024;
/// Existing complete MROWASP1 archive bound; exposed without changing its wire format.
pub const MAX_ARCHIVE_BYTES: usize = MAX_RAW + MAX_RAW / 255 + 128;

#[derive(Clone, PartialEq, Message)]
struct Envelope {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(bytes = "vec", tag = "2")]
    session_schema: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    process_schema: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    package: Vec<u8>,
    #[prost(uint32, tag = "5")]
    session_capabilities: u32,
    #[prost(uint32, tag = "6")]
    process_capabilities: u32,
    #[prost(string, repeated, tag = "7")]
    sessions: Vec<String>,
    #[prost(string, tag = "8")]
    execution_domain: String,
}

pub(crate) fn session_bits(c: SessionCapabilities) -> u32 {
    u32::from(c.session_read)
        | u32::from(c.session_write) << 1
        | u32::from(c.propose) << 2
        | u32::from(c.execute) << 3
        | u32::from(c.retire) << 4
}
pub(crate) fn process_bits(c: ProcessCapabilities) -> u32 {
    u32::from(c.read)
        | u32::from(c.events) << 1
        | u32::from(c.write) << 2
        | u32::from(c.close_input) << 3
        | u32::from(c.interrupt) << 4
        | u32::from(c.terminate) << 5
        | u32::from(c.resize_pty) << 6
}
fn session_capabilities(v: u32) -> Result<SessionCapabilities> {
    if v & !31 != 0 {
        return Err(Error::Contract);
    }
    Ok(SessionCapabilities {
        session_read: v & 1 != 0,
        session_write: v & 2 != 0,
        propose: v & 4 != 0,
        execute: v & 8 != 0,
        retire: v & 16 != 0,
    })
}
fn process_capabilities(v: u32) -> Result<ProcessCapabilities> {
    if v & !127 != 0 {
        return Err(Error::Contract);
    }
    Ok(ProcessCapabilities {
        read: v & 1 != 0,
        events: v & 2 != 0,
        write: v & 4 != 0,
        close_input: v & 8 != 0,
        interrupt: v & 16 != 0,
        terminate: v & 32 != 0,
        resize_pty: v & 64 != 0,
    })
}
fn identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

#[derive(Clone)]
pub struct Declaration {
    pub session: SessionCapabilities,
    pub process: ProcessCapabilities,
    pub sessions: Vec<String>,
    pub execution_domain: String,
}
impl Declaration {
    fn validate(&self) -> Result<()> {
        if !self.session.session_read
            || !identity(&self.execution_domain)
            || self.sessions.is_empty()
            || self.sessions.len() > 16
            || self.sessions.iter().any(|v| !identity(v))
            || self.sessions.windows(2).any(|v| v[0] >= v[1])
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

/// Separate versioned container. Its complete digest covers both protocol identities,
/// the original Wasm package, the declaration ceiling and the execution domain.
pub struct AgentProcessPackage {
    pub(crate) base: Package,
    pub(crate) declaration: Declaration,
    archive: Vec<u8>,
}
impl AgentProcessPackage {
    pub fn build(base: Package, declaration: Declaration) -> Result<Self> {
        declaration.validate()?;
        let raw = Envelope {
            version: 1,
            session_schema: morrow_agent_session_exec_v1_r2::schema_digest().to_vec(),
            process_schema: morrow_agent_process_control_v1::schema_digest().to_vec(),
            package: base.archive().to_vec(),
            session_capabilities: session_bits(declaration.session),
            process_capabilities: process_bits(declaration.process),
            sessions: declaration.sessions,
            execution_domain: declaration.execution_domain,
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
        archive.extend_from_slice(&morrow_agent_session_exec_v1_r2::hash(&raw));
        archive.extend_from_slice(&compressed);
        Self::decode(&archive)
    }
    pub fn decode(archive: &[u8]) -> Result<Self> {
        if archive.len() < HEADER
            || archive.len() > MAX_ARCHIVE_BYTES
            || &archive[..8] != MAGIC
            || archive[8..10] != 1u16.to_le_bytes()
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
            || morrow_agent_session_exec_v1_r2::hash(&raw).as_slice() != &archive[18..50]
        {
            return Err(Error::Invalid);
        }
        let value = Envelope::decode(raw.as_slice()).map_err(|_| Error::Invalid)?;
        if value.encode_to_vec() != raw
            || value.version != 1
            || value.session_schema != morrow_agent_session_exec_v1_r2::schema_digest()
            || value.process_schema != morrow_agent_process_control_v1::schema_digest()
        {
            return Err(Error::Contract);
        }
        let base = Package::decode(&value.package).map_err(|_| Error::Contract)?;
        let m = base.manifest();
        if m.guest_abi_version != 2
            || !m.required_features.is_empty()
            || !m.requested_capabilities.is_empty()
            || !m.transform_handlers.is_empty()
            || base.io_declaration().is_some()
            || base.channel_declaration().is_some()
            || base.mutation_enabled()
        {
            return Err(Error::Denied);
        }
        let declaration = Declaration {
            session: session_capabilities(value.session_capabilities)?,
            process: process_capabilities(value.process_capabilities)?,
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
        morrow_agent_session_exec_v1_r2::hash(&self.archive)
    }
    pub fn declaration(&self) -> &Declaration {
        &self.declaration
    }
    /// Read-only original package identity; this accessor creates no approval or connection.
    pub fn base(&self) -> &Package {
        &self.base
    }
    pub fn base_sha256(&self) -> [u8; 32] {
        self.base.digest()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_either_schema_pin_is_rejected_even_with_correct_outer_checksum() {
        let wasm=wat::parse_str("(module (memory (export \"memory\") 4) (func (export \"morrow_run\") (result i32) i32.const 0))").unwrap();
        let base = Package::build(
            Package::manifest_for_task("dual-schema-test", "1.0.0", &wasm, vec![]),
            &wasm,
        )
        .unwrap();
        let package = AgentProcessPackage::build(
            base,
            Declaration {
                session: SessionCapabilities {
                    session_read: true,
                    ..SessionCapabilities::default()
                },
                process: ProcessCapabilities::default(),
                sessions: vec!["session".into()],
                execution_domain: "domain".into(),
            },
        )
        .unwrap();
        let original = package.archive();
        let len = u32::from_le_bytes(original[10..14].try_into().unwrap()) as usize;
        let raw = lz4_flex::block::decompress(&original[HEADER..], len).unwrap();
        for session in [true, false] {
            let mut value = Envelope::decode(raw.as_slice()).unwrap();
            if session {
                value.session_schema[0] ^= 1;
            } else {
                value.process_schema[0] ^= 1;
            }
            let raw = value.encode_to_vec();
            let compressed = lz4_flex::block::compress(&raw);
            let mut archive = MAGIC.to_vec();
            archive.extend_from_slice(&1u16.to_le_bytes());
            archive.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            archive.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            archive.extend_from_slice(&morrow_agent_session_exec_v1_r2::hash(&raw));
            archive.extend_from_slice(&compressed);
            assert!(matches!(
                AgentProcessPackage::decode(&archive),
                Err(Error::Contract)
            ));
        }
    }
}
