//! Immutable portable package container. Integrity is not signature or trust.
use crate::{Error, Result, envelope, identity, lifecycle::GrantKind, runtime};
use prost::Message;
use prost_reflect::DescriptorPool;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::OnceLock};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.v1.rs"));
}
pub const MAX_MODULE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_RAW_BYTES: usize = MAX_MODULE_BYTES + MAX_MANIFEST_BYTES + 256;
pub const MAX_PACKAGE_BYTES: usize = MAX_RAW_BYTES + MAX_RAW_BYTES / 255 + 128;
const MAGIC: &[u8; 8] = b"MORROWP1";
fn preflight(bytes: &[u8], name: &str) -> Result<()> {
    static POOL: OnceLock<DescriptorPool> = OnceLock::new();
    let pool = POOL.get_or_init(|| {
        DescriptorPool::decode(
            include_bytes!(concat!(env!("OUT_DIR"), "/plugin_package.descriptor.bin")).as_slice(),
        )
        .expect("compiled descriptor")
    });
    crate::content::preflight(
        bytes,
        &pool.get_message_by_name(name).expect("compiled message"),
        &mut 256,
        0,
    )
}
fn capability(raw: i32) -> Result<GrantKind> {
    match proto::Capability::try_from(raw).map_err(|_| Error::UnsupportedVersion)? {
        proto::Capability::RenameCard => Ok(GrantKind::Rename),
        proto::Capability::ReadSummary => Ok(GrantKind::ReadSummary),
        proto::Capability::QueryOperation => Ok(GrantKind::QueryOperation),
        proto::Capability::ReadAttachment => Ok(GrantKind::ReadAttachment),
        proto::Capability::Unspecified => Err(Error::Invalid("unspecified capability")),
    }
}
pub struct Package {
    archive: Vec<u8>,
    manifest_bytes: Vec<u8>,
    manifest: proto::Manifest,
    module: Vec<u8>,
    digest: [u8; 32],
    ceiling: BTreeSet<GrantKind>,
}
impl Package {
    pub fn manifest_for(
        id: &str,
        version: &str,
        module: &[u8],
        capabilities: Vec<proto::Capability>,
    ) -> proto::Manifest {
        proto::Manifest {
            schema_version: 1,
            package_id: id.into(),
            package_version: version.into(),
            display_name: id.into(),
            guest_abi_version: 1,
            runtime_protocol_version: runtime::PROTOCOL_VERSION.into(),
            runtime_schema_sha256: runtime::runtime_digest().to_vec(),
            content_schema_sha256: runtime::content_digest().to_vec(),
            entrypoint: "morrow_run".into(),
            module_sha256: Sha256::digest(module).to_vec(),
            requested_capabilities: capabilities.into_iter().map(|v| v as i32).collect(),
            budget: Some(proto::ExecutionBudget {
                fuel: 20_000_000,
                memory_bytes: 16 * 1024 * 1024,
                host_calls: 16,
            }),
            required_features: vec![],
        }
    }
    pub fn build(manifest: proto::Manifest, module: &[u8]) -> Result<Self> {
        Self::from_parts(&manifest.encode_to_vec(), module)
    }
    /// Repacking supplied manifest bytes preserves their unknown optional fields.
    pub fn from_parts(manifest: &[u8], module: &[u8]) -> Result<Self> {
        if manifest.len() > MAX_MANIFEST_BYTES || module.len() > MAX_MODULE_BYTES {
            return Err(Error::Limit);
        }
        let raw = proto::Package {
            schema_version: 1,
            manifest: manifest.into(),
            module: module.into(),
        }
        .encode_to_vec();
        Self::decode(&envelope::pack(MAGIC, &raw, MAX_RAW_BYTES)?)
    }
    pub fn decode(archive: &[u8]) -> Result<Self> {
        let raw = envelope::unpack(MAGIC, archive, MAX_RAW_BYTES)?;
        preflight(&raw, "morrow.plugin.v1.Package")?;
        let package = proto::Package::decode(raw.as_slice())
            .map_err(|_| Error::Invalid("package protobuf"))?;
        if package.schema_version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        if package.manifest.len() > MAX_MANIFEST_BYTES || package.module.len() > MAX_MODULE_BYTES {
            return Err(Error::Limit);
        }
        preflight(&package.manifest, "morrow.plugin.v1.Manifest")?;
        let manifest = proto::Manifest::decode(package.manifest.as_slice())
            .map_err(|_| Error::Invalid("manifest protobuf"))?;
        if manifest.schema_version != 1
            || manifest.guest_abi_version != 1
            || manifest.runtime_protocol_version != u32::from(runtime::PROTOCOL_VERSION)
            || manifest.runtime_schema_sha256 != runtime::runtime_digest()
            || manifest.content_schema_sha256 != runtime::content_digest()
            || !manifest.required_features.is_empty()
        {
            return Err(Error::UnsupportedVersion);
        }
        identity(&manifest.package_id)?;
        if manifest.package_version.len() > 128
            || manifest.display_name.is_empty()
            || manifest.display_name.len() > 128
            || manifest.display_name.chars().any(char::is_control)
        {
            return Err(Error::Invalid("package metadata"));
        }
        semver::Version::parse(&manifest.package_version)
            .map_err(|_| Error::Invalid("package version"))?;
        if manifest.entrypoint != "morrow_run" {
            return Err(Error::UnsupportedVersion);
        }
        if !package.module.starts_with(b"\0asm\x01\0\0\0") {
            return Err(Error::Invalid("wasm module header"));
        }
        if manifest.module_sha256.as_slice() != Sha256::digest(&package.module).as_slice() {
            return Err(Error::Integrity);
        }
        let budget = manifest
            .budget
            .as_ref()
            .ok_or(Error::Invalid("missing budget"))?;
        if budget.fuel == 0
            || budget.fuel > 100_000_000
            || budget.memory_bytes < 65536
            || budget.memory_bytes > 64 * 1024 * 1024
            || budget.memory_bytes % 65536 != 0
            || budget.host_calls > 1024
        {
            return Err(Error::Limit);
        }
        if manifest.requested_capabilities.len() > 4 {
            return Err(Error::Limit);
        }
        let mut ceiling = BTreeSet::new();
        for cap in &manifest.requested_capabilities {
            if !ceiling.insert(capability(*cap)?) {
                return Err(Error::Invalid("duplicate capability"));
            }
        }
        Ok(Self {
            archive: archive.into(),
            manifest_bytes: package.manifest,
            manifest,
            module: package.module,
            digest: Sha256::digest(archive).into(),
            ceiling,
        })
    }
    pub fn archive(&self) -> &[u8] {
        &self.archive
    }
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest_bytes
    }
    pub fn manifest(&self) -> &proto::Manifest {
        &self.manifest
    }
    pub fn module(&self) -> &[u8] {
        &self.module
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn capabilities(&self) -> &BTreeSet<GrantKind> {
        &self.ceiling
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub mod catalog {
    use super::*;
    use std::{
        fs,
        io::{Read, Write},
        path::{Path, PathBuf},
    };
    /// Host-managed immutable files; this is not an activation registry or an OS sandbox.
    pub struct Catalog {
        root: PathBuf,
    }
    pub fn read_file(path: &Path) -> Result<Package> {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(|_| Error::Io)?
            .take(MAX_PACKAGE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Io)?;
        Package::decode(&bytes)
    }
    impl Catalog {
        pub fn open(root: &Path) -> Result<Self> {
            fs::create_dir_all(root).map_err(|_| Error::Io)?;
            Ok(Self {
                root: root.canonicalize().map_err(|_| Error::Io)?,
            })
        }
        fn path(&self, digest: [u8; 32]) -> PathBuf {
            self.root.join(format!(
                "{}.mplugin",
                digest
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ))
        }
        pub fn load(&self, digest: [u8; 32]) -> Result<Package> {
            let path = self.path(digest);
            if !fs::symlink_metadata(&path)
                .map_err(|_| Error::Io)?
                .file_type()
                .is_file()
            {
                return Err(Error::Invalid("package file type"));
            }
            let package = read_file(&path)?;
            if package.digest() != digest {
                return Err(Error::Integrity);
            }
            Ok(package)
        }
        /// Publish a complete synced file without overwriting another digest. Activation is separate.
        pub fn install(&self, package: &Package) -> Result<PathBuf> {
            let path = self.path(package.digest());
            let mut staged = tempfile::NamedTempFile::new_in(&self.root).map_err(|_| Error::Io)?;
            staged.write_all(package.archive()).map_err(|_| Error::Io)?;
            staged.as_file().sync_all().map_err(|_| Error::Io)?;
            match staged.persist_noclobber(&path) {
                Ok(_) => {}
                Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(Error::Io),
            }
            self.load(package.digest())?;
            Ok(path)
        }
    }
}
