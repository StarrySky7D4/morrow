//! Cooperative per-account, per-machine ownership of a signing identity.
use crate::TrustedLog;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
#[allow(unsafe_code)]
mod windows;
#[derive(Debug)]
pub enum LeaseError {
    Busy,
    Io(io::Error),
}
impl std::fmt::Display for LeaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "audit identity lease: {self:?}")
    }
}
impl std::error::Error for LeaseError {}
impl From<io::Error> for LeaseError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
pub(crate) struct Lease {
    _file: File,
}
fn reject_link(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            Err(io::Error::other("identity lease path is a link"))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
fn lease_path(trust: &TrustedLog) -> io::Result<PathBuf> {
    // Token-selected profile; no USERPROFILE or LOCALAPPDATA environment lookup.
    let root = windows::profile_dir()?
        .canonicalize()?
        .join("AppData")
        .join("Local")
        .join("Morrow");
    reject_link(&root)?;
    std::fs::create_dir_all(&root)?;
    let root = root.join("audit-identities-v1");
    reject_link(&root)?;
    std::fs::create_dir_all(&root)?;
    let mut digest = Sha256::new();
    digest.update(b"morrow-audit-identity-lease-v1\0");
    digest.update((trust.id.len() as u64).to_le_bytes());
    digest.update(trust.id.as_bytes());
    digest.update(trust.key.as_bytes());
    let name = digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let path = root.join(format!("{name}.lock"));
    reject_link(&path)?;
    Ok(path)
}
impl Lease {
    pub(crate) fn acquire(trust: &TrustedLog) -> Result<Self, LeaseError> {
        let path = lease_path(trust)?;
        // Allow peer opens for locking, forbid deleting/renaming a live guard file.
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(3)
            .open(path)?;
        file.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => LeaseError::Busy,
            std::fs::TryLockError::Error(e) => LeaseError::Io(e),
        })?;
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_guard_cannot_be_deleted_and_file_presence_alone_does_not_hold_ownership() {
        let d = tempfile::tempdir().unwrap();
        let key = crate::keys::Key::create(&d.path().join("key")).unwrap();
        let path = lease_path(&key.trust()).unwrap();
        let lease = Lease::acquire(&key.trust()).unwrap();
        assert!(std::fs::remove_file(&path).is_err());
        drop(lease);
        let lease = Lease::acquire(&key.trust()).unwrap();
        drop(lease);
        std::fs::remove_file(path).unwrap();
    }
}
