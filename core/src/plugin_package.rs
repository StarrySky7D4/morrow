//! Immutable portable package container. Integrity is not signature or trust.
use crate::{Error, Result, envelope, identity, lifecycle::GrantKind, runtime};
use prost::Message;
use prost_reflect::DescriptorPool;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::OnceLock};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.plugin.v1.rs"));
}
pub mod io;
pub const TRANSFORM_HANDLERS_FEATURE: &str = "transform-handlers-v1";
pub const MAX_TRANSFORM_HANDLERS: usize = 16;
pub const DEPENDENCY_CALLS_FEATURE: &str = "dependency-calls-v1";
pub const DEPENDENCIES_FEATURE: &str = "dependencies-v1";
pub const MUTATION_FEATURE: &str = "mutation-v1";
pub const MUTATION_BUDGET_FEATURE: &str = "mutation-budget-v1";
pub const MAX_MUTATION_JOB_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_MUTATION_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_DEPENDENCIES: usize = 16;
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
fn validate_mutation_budget_wire(
    mut manifest: &[u8],
    value: Option<&proto::MutationBudget>,
) -> Result<()> {
    use prost::encoding::{DecodeContext, WireType, decode_key, decode_varint, skip_field};
    let mut seen = false;
    while !manifest.is_empty() {
        let (number, wire) =
            decode_key(&mut manifest).map_err(|_| Error::Invalid("manifest key"))?;
        if number != 20 {
            skip_field(wire, number, &mut manifest, DecodeContext::default())
                .map_err(|_| Error::Invalid("manifest field"))?;
            continue;
        }
        if seen || wire != WireType::LengthDelimited {
            return Err(Error::Invalid("duplicate or malformed mutation budget"));
        }
        seen = true;
        let length =
            decode_varint(&mut manifest).map_err(|_| Error::Invalid("mutation budget length"))?;
        if length > manifest.len() as u64 {
            return Err(Error::Invalid("mutation budget length"));
        }
        let (bytes, rest) = manifest.split_at(length as usize);
        manifest = rest;
        let value = value.ok_or(Error::Invalid("missing mutation budget"))?;
        if value.encode_to_vec() != bytes {
            return Err(Error::Invalid("noncanonical mutation budget"));
        }
    }
    if seen != value.is_some() {
        return Err(Error::Invalid("mutation budget presence"));
    }
    Ok(())
}
/// Decode the manifest/registry content-capability namespace at a trusted boundary.
pub fn capability(raw: i32) -> Result<GrantKind> {
    match proto::Capability::try_from(raw).map_err(|_| Error::UnsupportedVersion)? {
        proto::Capability::RenameCard => Ok(GrantKind::Rename),
        proto::Capability::ReadSummary => Ok(GrantKind::ReadSummary),
        proto::Capability::QueryOperation => Ok(GrantKind::QueryOperation),
        proto::Capability::ReadAttachment => Ok(GrantKind::ReadAttachment),
        proto::Capability::CreateContent => Ok(GrantKind::CreateContent),
        proto::Capability::EditContent => Ok(GrantKind::EditContent),
        proto::Capability::ReadContent => Ok(GrantKind::ReadContent),
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
    io_ceiling: BTreeSet<io::IoCapability>,
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
            task_schema_sha256: vec![],
            transform_handlers: vec![],
            dependencies: vec![],
            dependency_schema_sha256: vec![],
            io_declaration: None,
            mutation_schema_sha256: vec![],
            mutation_budget: None,
            channel_declaration: None,
        }
    }
    pub fn manifest_for_task(
        id: &str,
        version: &str,
        module: &[u8],
        capabilities: Vec<proto::Capability>,
    ) -> proto::Manifest {
        let mut manifest = Self::manifest_for(id, version, module, capabilities);
        manifest.guest_abi_version = 2;
        manifest.task_schema_sha256 = crate::task::schema_digest().to_vec();
        manifest
    }
    pub fn manifest_for_transform(
        id: &str,
        version: &str,
        module: &[u8],
        handlers: Vec<proto::TransformHandler>,
    ) -> proto::Manifest {
        let mut manifest = Self::manifest_for_task(id, version, module, vec![]);
        manifest
            .required_features
            .push(TRANSFORM_HANDLERS_FEATURE.into());
        manifest.transform_handlers = handlers;
        manifest
    }
    /// Explicit metadata-only package shape; declarations are never approval.
    pub fn manifest_for_changes_metadata(
        id: &str, version: &str, module: &[u8], handlers: Vec<proto::TransformHandler>,
    ) -> proto::Manifest {
        let names=handlers.iter().map(|h| h.handler.clone()).collect();
        let mut manifest=Self::manifest_for_transform(id,version,module,handlers);
        manifest.required_features.push(crate::channel::FEATURE.into());
        manifest.required_features.push(crate::changes_metadata::FEATURE.into());
        manifest.channel_declaration=Some(crate::channel::declaration(names,vec![crate::channel::Kind::Events]));
        manifest
    }
    pub fn is_changes_metadata(&self) -> bool {
        self.manifest.required_features.iter().any(|f| f == crate::changes_metadata::FEATURE)
    }
    /// Presentation providers share one active slot; styles remain client settings.
    pub fn is_ui_theme(&self) -> bool {
        self.manifest.transform_handlers.iter().any(|h| {
            h.handler == "theme.describe"
                && h.input_type == "morrow.ui.theme.request.v1"
                && h.output_type == "morrow.ui.theme.v1"
        })
    }
    /// Package declarations are not grants or proof that the guest implements the handler.
    pub fn transform_handler(
        &self,
        input: &crate::task::Transform,
    ) -> Result<&proto::TransformHandler> {
        // IO packages require their dedicated execution/observation contract, including when
        // the module also advertises a pure entrypoint. Never capture them as pure evidence.
        if self.io_declaration().is_some() || self.channel_declaration().is_some() {
            return Err(Error::Invalid("IO package requires IO execution"));
        }
        let handler = self
            .manifest
            .transform_handlers
            .iter()
            .find(|h| h.handler == input.handler)
            .ok_or(Error::Invalid("unregistered transform handler"))?;
        if handler.input_type != input.input_type || handler.output_type != input.output_type {
            return Err(Error::Invalid("transform type mismatch"));
        }
        if input.input.len() > handler.max_input_bytes as usize {
            return Err(Error::Limit);
        }
        Ok(handler)
    }
    /// Task shape for the dedicated channel executor. Metadata never grants a source.
    /// The ordinary pure transform route rejects every channel package.
    pub fn channel_handler(
        &self,
        input: &crate::task::Transform,
    ) -> Result<&proto::TransformHandler> {
        let declaration = self
            .channel_declaration()
            .ok_or(Error::Invalid("missing channel declaration"))?;
        if !declaration
            .handlers
            .iter()
            .any(|handler| handler == &input.handler)
        {
            return Err(Error::Invalid("unregistered channel handler"));
        }
        let handler = self
            .manifest
            .transform_handlers
            .iter()
            .find(|handler| handler.handler == input.handler)
            .ok_or(Error::Invalid("channel handler missing task metadata"))?;
        if handler.input_type != input.input_type || handler.output_type != input.output_type {
            return Err(Error::Invalid("channel task type mismatch"));
        }
        if input.input.len() > handler.max_input_bytes as usize {
            return Err(Error::Limit);
        }
        Ok(handler)
    }
    /// Resolve a declared slot only. The declaration neither selects nor authorizes a provider.
    pub fn dependency(&self, slot: &str) -> Result<&proto::DependencyRequirement> {
        self.manifest
            .dependencies
            .iter()
            .find(|d| d.slot == slot)
            .ok_or(Error::Invalid("undeclared dependency slot"))
    }
    /// Check one selected provider's package version and exact transform contract.
    /// Optional slots may be unbound by the host; a bound optional slot still must be compatible.
    pub fn check_dependency(&self, slot: &str, provider: &Package) -> Result<()> {
        let requirement = self.dependency(slot)?;
        if provider.manifest.guest_abi_version != 2 {
            return Err(Error::UnsupportedVersion);
        }
        let version = semver::Version::parse(&provider.manifest.package_version)
            .map_err(|_| Error::Invalid("package version"))?;
        let requested = semver::VersionReq::parse(&requirement.provider_version)
            .map_err(|_| Error::Invalid("dependency version requirement"))?;
        if !requested.matches(&version) {
            return Err(Error::Invalid("dependency provider version"));
        }
        provider.transform_handler(&crate::task::Transform {
            handler: requirement.handler.clone(),
            input_type: requirement.input_type.clone(),
            output_type: requirement.output_type.clone(),
            input: vec![],
        })?;
        Ok(())
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
            || !matches!(manifest.guest_abi_version, 1 | 2)
            || manifest.runtime_protocol_version != u32::from(runtime::PROTOCOL_VERSION)
            || manifest.runtime_schema_sha256 != runtime::runtime_digest()
            || manifest.content_schema_sha256 != runtime::content_digest()
            || manifest.required_features.len() > 10
            || manifest.required_features.iter().any(|f| {
                f != TRANSFORM_HANDLERS_FEATURE
                    && f != DEPENDENCIES_FEATURE
                    && f != DEPENDENCY_CALLS_FEATURE
                    && f != io::FEATURE
                    && f != io::SERVICE_RUN_FEATURE
                    && f != io::SERVICE_RUN_BUDGET_FEATURE
                    && f != crate::service_resources::FEATURE
                    && f != MUTATION_FEATURE
                    && f != MUTATION_BUDGET_FEATURE
                    && f != crate::channel::FEATURE
                    && f != crate::changes_metadata::FEATURE
            })
            || manifest
                .required_features
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != manifest.required_features.len()
        {
            return Err(Error::UnsupportedVersion);
        }
        if (manifest.guest_abi_version == 1 && !manifest.task_schema_sha256.is_empty())
            || (manifest.guest_abi_version == 2
                && manifest.task_schema_sha256 != crate::task::schema_digest())
        {
            return Err(Error::UnsupportedVersion);
        }
        let registered = manifest
            .required_features
            .iter()
            .any(|f| f == TRANSFORM_HANDLERS_FEATURE);
        let dependencies_feature = manifest
            .required_features
            .iter()
            .any(|f| f == DEPENDENCIES_FEATURE);
        if (!manifest.dependencies.is_empty() && !dependencies_feature)
            || (dependencies_feature && manifest.guest_abi_version != 2)
        {
            return Err(Error::Invalid("dependency registration feature"));
        }
        let dependency_calls = manifest
            .required_features
            .iter()
            .any(|f| f == DEPENDENCY_CALLS_FEATURE);
        if dependency_calls {
            if manifest.guest_abi_version != 2
                || !dependencies_feature
                || !registered
                || manifest.dependency_schema_sha256 != crate::dependency_call::schema_digest()
            {
                return Err(Error::Invalid("dependency call feature"));
            }
        } else if !manifest.dependency_schema_sha256.is_empty() {
            return Err(Error::Invalid("unexpected dependency schema"));
        }
        if manifest.dependencies.len() > MAX_DEPENDENCIES {
            return Err(Error::Limit);
        }
        let mut slots = BTreeSet::new();
        for dependency in &manifest.dependencies {
            identity(&dependency.slot)?;
            identity(&dependency.handler)?;
            identity(&dependency.input_type)?;
            identity(&dependency.output_type)?;
            if !slots.insert(&dependency.slot) {
                return Err(Error::Invalid("duplicate dependency slot"));
            }
            if dependency.provider_version.trim().is_empty()
                || dependency.provider_version.len() > 128
                || dependency.provider_version.chars().any(char::is_control)
            {
                return Err(Error::Invalid("dependency version requirement"));
            }
            semver::VersionReq::parse(&dependency.provider_version)
                .map_err(|_| Error::Invalid("dependency version requirement"))?;
        }
        if registered == manifest.transform_handlers.is_empty()
            || (registered && manifest.guest_abi_version != 2)
        {
            return Err(Error::Invalid("transform registration feature"));
        }
        if manifest.transform_handlers.len() > MAX_TRANSFORM_HANDLERS {
            return Err(Error::Limit);
        }
        let mut handlers = BTreeSet::new();
        for handler in &manifest.transform_handlers {
            identity(&handler.handler)?;
            identity(&handler.input_type)?;
            identity(&handler.output_type)?;
            if !handlers.insert(&handler.handler) {
                return Err(Error::Invalid("duplicate transform handler"));
            }
            if handler.max_input_bytes as usize > crate::task::MAX_VALUE_BYTES
                || handler.max_output_bytes as usize > crate::task::MAX_VALUE_BYTES
            {
                return Err(Error::Limit);
            }
        }
        crate::channel::validate_manifest_wire(
            &package.manifest,
            manifest.channel_declaration.as_ref(),
        )?;
        let channel_feature = manifest
            .required_features
            .iter()
            .any(|f| f == crate::channel::FEATURE);
        if channel_feature != manifest.channel_declaration.is_some()
            || channel_feature
                && (manifest.guest_abi_version != 2
                    || manifest.io_declaration.is_some()
                    || !manifest.dependencies.is_empty()
                    || dependencies_feature
                    || dependency_calls
                    || !manifest.mutation_schema_sha256.is_empty()
                    || manifest.mutation_budget.is_some()
                    || manifest
                        .required_features
                        .iter()
                        .any(|f| matches!(f.as_str(), MUTATION_FEATURE | MUTATION_BUDGET_FEATURE)))
        {
            return Err(Error::Invalid("channel declaration feature combination"));
        }
        if let Some(declaration) = &manifest.channel_declaration {
            crate::channel::validate_declaration(declaration)?;
            if declaration.handlers.iter().any(|h| !handlers.contains(h)) {
                return Err(Error::Invalid("channel handler missing task metadata"));
            }
        }
        if manifest.required_features.iter().any(|f| f == crate::changes_metadata::FEATURE) {
            // Opt-in metadata profile. Never relax legacy channel semantics or
            // reinterpret content/IO/service/dependency/mutation permissions.
            if manifest.guest_abi_version != 2 || !channel_feature
                || !manifest.requested_capabilities.is_empty()
                || manifest.required_features.iter().any(|f| !matches!(f.as_str(),
                    TRANSFORM_HANDLERS_FEATURE | crate::channel::FEATURE | crate::changes_metadata::FEATURE))
                || manifest.channel_declaration.as_ref().is_none_or(|d| d.kinds != [2])
            { return Err(Error::Invalid("changes metadata feature combination")); }
        }
        io::validate_wire(&package.manifest, manifest.io_declaration.as_ref())?;
        validate_mutation_budget_wire(&package.manifest, manifest.mutation_budget.as_ref())?;
        let io_feature = manifest.required_features.iter().any(|f| f == io::FEATURE);
        if io_feature != manifest.io_declaration.is_some()
            || (io_feature && manifest.guest_abi_version != 2)
        {
            return Err(Error::Invalid("IO declaration feature"));
        }
        let service_run_feature = manifest
            .required_features
            .iter()
            .any(|f| f == io::SERVICE_RUN_FEATURE);
        if service_run_feature
            != manifest
                .io_declaration
                .as_ref()
                .is_some_and(|declaration| declaration.service_run.is_some())
        {
            return Err(Error::Invalid("service run declaration feature"));
        }
        let service_run_budget_feature = manifest
            .required_features
            .iter()
            .any(|f| f == io::SERVICE_RUN_BUDGET_FEATURE);
        if service_run_budget_feature
            != manifest
                .io_declaration
                .as_ref()
                .and_then(|declaration| declaration.service_run.as_ref())
                .is_some_and(|profile| profile.budget.is_some())
        {
            return Err(Error::Invalid("service run budget declaration feature"));
        }
        let io_ceiling = if let Some(declaration) = &manifest.io_declaration {
            let ceiling = io::validate(declaration)?;
            if declaration.handlers.iter().any(|h| handlers.contains(h)) {
                return Err(Error::Invalid("IO handler registered as pure transform"));
            }
            ceiling
        } else {
            BTreeSet::new()
        };
        let mutation_feature = manifest
            .required_features
            .iter()
            .any(|f| f == MUTATION_FEATURE);
        if mutation_feature {
            if manifest.guest_abi_version != 2
                || !io_feature
                || dependency_calls
                || manifest.mutation_schema_sha256 != crate::mutation::schema_digest()
                || !io_ceiling.contains(&io::IoCapability::FileCreate)
                    && !io_ceiling.contains(&io::IoCapability::FileDelete)
            {
                return Err(Error::Invalid("mutation feature declaration"));
            }
        } else if !manifest.mutation_schema_sha256.is_empty() {
            return Err(Error::Invalid("unexpected mutation schema"));
        }
        let mutation_budget_feature = manifest
            .required_features
            .iter()
            .any(|f| f == MUTATION_BUDGET_FEATURE);
        if mutation_budget_feature != manifest.mutation_budget.is_some() {
            return Err(Error::Invalid("mutation budget declaration feature"));
        }
        if let Some(budget) = &manifest.mutation_budget {
            if !mutation_feature
                || dependencies_feature
                || dependency_calls
                || service_run_feature
                || service_run_budget_feature
                || io_ceiling.iter().any(|cap| {
                    !matches!(
                        cap,
                        io::IoCapability::FileCreate | io::IoCapability::FileDelete
                    )
                })
                || budget.max_job_bytes == 0
                || budget.max_job_bytes > MAX_MUTATION_JOB_BYTES
                || budget.max_bytes < budget.max_job_bytes
                || budget.max_bytes > MAX_MUTATION_BYTES
            {
                return Err(Error::Invalid("mutation budget declaration"));
            }
        }
        if manifest
            .required_features
            .iter()
            .any(|f| f == crate::service_resources::FEATURE)
            && (!io_ceiling.contains(&io::IoCapability::HttpRequest)
                || !io_ceiling.contains(&io::IoCapability::HttpPublish)
                || manifest
                    .io_declaration
                    .as_ref()
                    .is_none_or(|d| d.service_schema_sha256 != crate::service::schema_digest()))
        {
            return Err(Error::Invalid("service resources feature"));
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
        if manifest.requested_capabilities.len() > 7 {
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
            io_ceiling,
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
    pub fn io_declaration(&self) -> Option<&io::proto::IoDeclaration> {
        self.manifest.io_declaration.as_ref()
    }
    pub fn channel_declaration(&self) -> Option<&crate::channel::proto::ChannelDeclaration> {
        self.manifest.channel_declaration.as_ref()
    }
    pub fn mutation_enabled(&self) -> bool {
        self.manifest
            .required_features
            .iter()
            .any(|f| f == MUTATION_FEATURE)
    }
    pub fn mutation_budget(&self) -> Option<&proto::MutationBudget> {
        self.manifest.mutation_budget.as_ref()
    }
    pub fn io_capabilities(&self) -> &BTreeSet<io::IoCapability> {
        &self.io_ceiling
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
            // Reopening an installed immutable bundle only needs validation.
            // Avoid writing/syncing a temporary archive on every application
            // launch. File type, contents and digest still pass load(), and an
            // invalid existing entry is never silently repaired or overwritten.
            match fs::symlink_metadata(&path) {
                Ok(_) => {
                    self.load(package.digest())?;
                    return Ok(path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(Error::Io),
            }
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

pub mod registry;
