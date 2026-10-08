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
    _handles: Vec<File>,
}
impl MatchedRunnerArtifact {
    /// Protocol compatibility metadata; not binary identity or pipe authentication.
    pub fn protocol_version(&self) -> u32 {
        u32::from(crate::ipc_framed::CHECKED_IPC_PROTOCOL_VERSION)
    }
    pub fn control_schema_sha256(&self) -> [u8; 32] {
        crate::ipc_framed::CHECKED_CONTROL_SCHEMA
    }
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
            _handles: handles,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn sha256(&self) -> [u8; 32] {
        self.expected_sha256
    }
    pub(crate) fn verify_resolved(&self, resolved: &Path) -> Result<()> {
        ensure!(
            dunce::canonicalize(resolved)? == self.path,
            "resolved runner differs from retained exact artifact"
        );
        Ok(())
    }
}
