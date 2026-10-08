use morrow_agent_session_exec_v1_r2::{Error, Result};
use sha2::{Digest, Sha256};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, Prefix},
};

/// Read handles deny write/delete sharing until the genuine process completes.
/// Parent directory handles prevent replacement of an ancestor by a junction.
pub(crate) struct LockedArtifact {
    _handles: Vec<File>,
}
impl LockedArtifact {
    pub(crate) fn acquire(path: &str, expected: [u8; 32], cwd: &str) -> Result<Self> {
        let path = Path::new(path);
        let mut components = path.components();
        if !matches!(components.next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
            || !matches!(components.next(), Some(Component::RootDir))
            || components.any(|c| !matches!(c, Component::Normal(_)))
            || !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            || path
                .to_str()
                .is_none_or(|p| p.split(['/', '\\']).any(|c| c == "." || c == ".."))
        {
            return Err(Error::Denied);
        }
        let mut parents = path.ancestors().skip(1).collect::<Vec<_>>();
        parents.reverse();
        let mut handles = Vec::new();
        let cwd = Path::new(cwd);
        let mut cwd_components = cwd.components();
        if !matches!(cwd_components.next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
            || !matches!(cwd_components.next(), Some(Component::RootDir))
            || cwd_components.any(|c| !matches!(c, Component::Normal(_)))
            || cwd
                .to_str()
                .is_none_or(|p| p.split(['/', '\\']).any(|c| c == "." || c == ".."))
        {
            return Err(Error::Denied);
        }
        let mut cwd_parents = cwd.ancestors().collect::<Vec<_>>();
        cwd_parents.reverse();
        parents.extend(cwd_parents);
        for parent in parents {
            let handle = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x02000000 | 0x00200000)
                .open(parent)
                .map_err(|_| Error::Denied)?;
            let meta = handle.metadata().map_err(|_| Error::Storage)?;
            if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
                return Err(Error::Denied);
            }
            handles.push(handle);
        }
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(path)
            .map_err(|_| Error::Denied)?;
        let meta = file.metadata().map_err(|_| Error::Storage)?;
        if !meta.is_file() || meta.file_attributes() & 0x400 != 0 || meta.len() > 512 * 1024 * 1024
        {
            return Err(Error::Denied);
        }
        // The Windows loader still validates the full executable. This check rejects
        // fixtures that merely rename arbitrary bytes to .exe before any invocation.
        let mut dos = [0u8; 64];
        file.read_exact(&mut dos).map_err(|_| Error::Denied)?;
        if &dos[..2] != b"MZ" {
            return Err(Error::Denied);
        }
        let offset = u32::from_le_bytes(dos[60..64].try_into().map_err(|_| Error::Denied)?) as u64;
        if offset > meta.len().saturating_sub(4) {
            return Err(Error::Denied);
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| Error::Storage)?;
        let mut signature = [0; 4];
        file.read_exact(&mut signature).map_err(|_| Error::Denied)?;
        if signature != *b"PE\0\0" {
            return Err(Error::Denied);
        }
        file.seek(SeekFrom::Start(0)).map_err(|_| Error::Storage)?;
        let mut digest = Sha256::new();
        let mut buffer = [0; 32768];
        loop {
            let count = file.read(&mut buffer).map_err(|_| Error::Storage)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        if <[u8; 32]>::from(digest.finalize()) != expected {
            return Err(Error::Denied);
        }
        handles.push(file);
        Ok(Self { _handles: handles })
    }
}
