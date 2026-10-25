//! Trusted independent command-runner artifact pin. No Core/runtime authority.
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf, Prefix};

/// Read-sharing only denies file/ancestor writes and replacement for this Arc's lifetime.
pub struct MatchedRunnerArtifact {
    path: PathBuf,
    expected_sha256: [u8; 32],
    native_close_v8: bool,
    _handles: Vec<File>,
    // Forking an opt-in view preserves THIS acquired lock/instance, rather than
    // replacing it with a later path/hash equality test or duplicating handles.
    _parent: Option<std::sync::Arc<MatchedRunnerArtifact>>,
    instance: std::sync::Arc<()>,
    precreation:Option<std::sync::Arc<crate::asuser_a1_helper::parent_native::OriginGuard>>,
    asuser:Option<std::sync::Arc<crate::AsUserFactoryBinding>>,
}
impl MatchedRunnerArtifact {
    pub fn native_close_view(self:&std::sync::Arc<Self>)->std::sync::Arc<Self> {
        std::sync::Arc::new(Self {path:self.path.clone(),expected_sha256:self.expected_sha256,
            native_close_v8:true,_handles:Vec::new(),_parent:Some(std::sync::Arc::clone(self)),
            instance:std::sync::Arc::clone(&self.instance),precreation:self.precreation.clone(),asuser:self.asuser.clone()})
    }
    /// Instance relation only; never a native completion or cleanup authority.
    pub fn shares_original_pin(&self,other:&Self)->bool {std::sync::Arc::ptr_eq(&self.instance,&other.instance)}
    /// Protocol compatibility metadata; not binary identity or pipe authentication.
    pub fn protocol_version(&self) -> u32 {
        if self.native_close_v8 {8} else {u32::from(crate::ipc_framed::CHECKED_IPC_PROTOCOL_VERSION)}
    }
    pub fn control_schema_sha256(&self) -> [u8; 32] {
        if self.native_close_v8 {crate::elevated::native_close_protocol::NATIVE_CLOSE_PROTOCOL_SCHEMA}
        else {crate::ipc_framed::CHECKED_CONTROL_SCHEMA}
    }
    /// Explicit opt-in on the SAME reviewed independent artifact. Existing acquire
    /// remains v7 and legacy callers are never upgraded by metadata alone.
    pub fn acquire_native_close_v8(expected_path:&Path, expected_sha256:[u8;32])->Result<Self> {
        let mut artifact=Self::acquire(expected_path,expected_sha256)?;
        artifact.native_close_v8=true;Ok(artifact)
    }
    pub(crate) fn native_close_v8(&self)->bool {self.native_close_v8}
    pub fn acquire(expected_path: &Path, expected_sha256: [u8; 32]) -> Result<Self> {
        let mut components = expected_path.components();
        ensure!(
            matches!(components.next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
                && matches!(components.next(), Some(Component::RootDir))
                && components.all(
                    |c| matches!(c, Component::Normal(s) if !s.to_string_lossy().contains(':'))
                ),
            "runner must have a trusted absolute drive path"
        );
        let text = expected_path.to_str().context("runner path is not UTF-8")?;
        ensure!(
            !text.split(['/', '\\']).any(|c| c == "." || c == ".."),
            "runner dot segment"
        );
        ensure!(
            expected_path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe")),
            "runner extension"
        );
        let mut parents = expected_path.ancestors().skip(1).collect::<Vec<_>>();
        parents.reverse();
        let mut handles = Vec::new();
        for parent in parents {
            let handle = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x02000000 | 0x00200000)
                .open(parent)
                .context("lock runner ancestor")?;
            let metadata = handle.metadata()?;
            ensure!(
                metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
                "runner reparse ancestor"
            );
            handles.push(handle);
        }
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(expected_path)
            .context("lock exact runner")?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file()
                && metadata.file_attributes() & 0x400 == 0
                && metadata.len() <= 512 * 1024 * 1024,
            "runner is not a bounded regular file"
        );
        let mut dos = [0u8; 64];
        file.read_exact(&mut dos)?;
        ensure!(&dos[..2] == b"MZ", "runner executable header");
        let offset = u32::from_le_bytes(dos[60..64].try_into()?) as u64;
        ensure!(
            offset <= metadata.len().saturating_sub(4),
            "runner PE offset"
        );
        file.seek(SeekFrom::Start(offset))?;
        let mut pe = [0u8; 4];
        file.read_exact(&mut pe)?;
        ensure!(pe == *b"PE\0\0", "runner PE signature");
        file.seek(SeekFrom::Start(0))?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 32768];
        loop {
            let size = file.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            digest.update(&buffer[..size]);
        }
        ensure!(
            <[u8; 32]>::from(digest.finalize()) == expected_sha256,
            "runner artifact SHA mismatch"
        );
        let path = dunce::canonicalize(expected_path)?;
        handles.push(file);
        Ok(Self {
            path,
            expected_sha256,
            native_close_v8: false,
            _handles: handles,
            _parent: None,
            instance: std::sync::Arc::new(()),
            precreation:None,
            asuser:None,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    // Called ONLY by the native authenticated P->C inlet after all original
    // inherited file capabilities and actual creator/process identity checks.
    // The same guard, not a later path-acquire, retains these C prebirth pins.
    pub(crate) fn from_parent_original(path:PathBuf,expected_sha256:[u8;32],
        original:std::sync::Arc<crate::asuser_a1_helper::parent_native::OriginGuard>)->Self {
        Self{path,expected_sha256,native_close_v8:false,_handles:Vec::new(),_parent:None,
            instance:std::sync::Arc::new(()),precreation:Some(original),asuser:None}
    }
    pub(crate) fn asuser_view(self:&std::sync::Arc<Self>,binding:std::sync::Arc<crate::AsUserFactoryBinding>)->std::sync::Arc<Self> {
        std::sync::Arc::new(Self{path:self.path.clone(),expected_sha256:self.expected_sha256,native_close_v8:true,
            _handles:Vec::new(),_parent:Some(self.clone()),instance:self.instance.clone(),precreation:self.precreation.clone(),asuser:Some(binding)})
    }
    pub(crate) fn asuser_binding(&self)->Option<std::sync::Arc<crate::AsUserFactoryBinding>>{self.asuser.clone()}
    #[doc(hidden)]
    pub fn qualify_asuser_logon_rejection_for<T:std::any::Any+Send+Sync>(&self,backend:&std::sync::Arc<T>)
        ->std::io::Result<()> {
        self.asuser.as_ref().ok_or_else(||std::io::Error::from(std::io::ErrorKind::PermissionDenied))?
            .qualify_logon_rejection_for(backend)
    }
    #[doc(hidden)]
    pub fn qualify_asuser_source_policy_rejection_for<T:std::any::Any+Send+Sync>(&self,backend:&std::sync::Arc<T>)
        ->std::io::Result<()> {
        self.asuser.as_ref().ok_or_else(||std::io::Error::from(std::io::ErrorKind::PermissionDenied))?
            .qualify_source_policy_rejection_for(backend)
    }
    #[doc(hidden)]
    pub fn qualify_asuser_after_helper_stop_for<T:std::any::Any+Send+Sync>(&self,backend:&std::sync::Arc<T>)
        ->std::io::Result<()> {
        self.asuser.as_ref().ok_or_else(||std::io::Error::from(std::io::ErrorKind::PermissionDenied))?
            .qualify_after_helper_stop_for(backend)
    }
    #[doc(hidden)]
    pub fn take_asuser_caller_refusal_for<T:std::any::Any+Send+Sync>(&self,backend:&std::sync::Arc<T>)
        ->std::io::Result<Option<crate::AsUserCallerRefusal>> {
        match self.asuser.as_ref(){Some(binding)=>binding.take_caller_refusal_for(backend),None=>Ok(None)}
    }
    /// Borrow only the home held by THIS original A1 setup/factory binding.
    /// A path is data, not admission, identity, completion or cleanup authority.
    /// No lookup, reacquisition, environment read or new owner is performed.
    #[doc(hidden)]
    pub fn asuser_setup_codex_home(&self) -> Option<&Path> {
        self.asuser.as_ref().map(|binding| binding.original_setup().codex_home())
    }
    pub fn sha256(&self) -> [u8; 32] {
        self.expected_sha256
    }
    /// Inspect only the original held S file, including an opt-in view's
    /// original parent. Does not reacquire a path or create a role capability.
    pub(crate) fn asuser_s_table(&self)->std::io::Result<crate::asuser_a1_protocol::RoleTable> {
        let mut original=self;
        for _ in 0..32 {
            if let Some(file)=original._handles.last() {
                return crate::asuser_a1_table::from_held_file(file);
            }
            original=original._parent.as_deref().ok_or_else(||std::io::Error::other("A1 original S pin absent"))?;
        }
        Err(std::io::Error::other("A1 artifact view ancestry bound exceeded"))
    }
    pub(crate) fn verify_resolved(&self, resolved: &Path) -> Result<()> {
        ensure!(
            dunce::canonicalize(resolved)? == self.path,
            "resolved runner differs from retained exact artifact"
        );
        Ok(())
    }
}
