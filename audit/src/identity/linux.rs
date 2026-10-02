//! Same-account cooperative identity lease. An open kernel lock is ownership;
//! neither a stored PID nor the disappearance of a process proves release.
use super::LeaseError;
use crate::TrustedLog;
use morrow_core::linux_storage::{PrivateDirectory, account_home};
use sha2::{Digest, Sha256};
use std::{fs::File, os::unix::fs::MetadataExt, path::Path};

pub(crate) struct Lease {
    _file: File,
    _directory: PrivateDirectory,
}
fn leaf(trust: &TrustedLog) -> String {
    let mut digest = Sha256::new();
    digest.update(b"morrow-audit-identity-lease-v1\0");
    digest.update((trust.id.len() as u64).to_le_bytes());
    digest.update(trust.id.as_bytes());
    digest.update(trust.key.as_bytes());
    let name: String = digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{name}.lock")
}
impl Lease {
    pub(crate) fn acquire(trust: &TrustedLog) -> Result<Self, LeaseError> {
        // Real passwd home. No HOME/XDG lookup and no fallback to /tmp or another user.
        let root = account_home()?.join(".local/share/Morrow/audit-identities-v1");
        Self::acquire_in(trust, PrivateDirectory::open(&root, true)?)
    }
    fn acquire_in(trust: &TrustedLog, directory: PrivateDirectory) -> Result<Self, LeaseError> {
        let name = leaf(trust);
        let name = Path::new(&name);
        let file = match directory.create_new(name) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => directory.open_read(name)?,
            value => value?,
        };
        file.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => LeaseError::Busy,
            std::fs::TryLockError::Error(e) => LeaseError::Io(e),
        })?;
        directory.verify()?;
        let a = file.metadata()?;
        let b = directory.inspect(name)?.metadata()?;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return Err(LeaseError::Io(std::io::Error::other(
                "identity lease inode changed",
            )));
        }
        Ok(Self {
            _file: file,
            _directory: directory,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    fn trust() -> TrustedLog {
        TrustedLog {
            id: "synthetic-linux-lease".into(),
            key: crate::SigningKey::from_bytes(&[47; 32]).verifying_key(),
        }
    }
    #[test]
    fn synthetic_private_directory_lease_blocks_until_fd_drop() {
        let temp = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let acquire = || {
            Lease::acquire_in(
                &trust(),
                PrivateDirectory::open(temp.path(), false).unwrap(),
            )
        };
        let contender = || {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "identity::linux::tests::synthetic_lease_child",
                    "--nocapture",
                ])
                .env("MORROW_SYNTHETIC_LEASE_DIRECTORY", temp.path())
                .status()
                .unwrap()
                .code()
                .unwrap()
        };
        let first = acquire().unwrap();
        assert!(matches!(acquire(), Err(LeaseError::Busy)));
        assert_eq!(contender(), 75);
        drop(first);
        assert_eq!(contender(), 0);
        drop(acquire().unwrap());
        // File presence alone is not ownership and does not force identity rotation.
        drop(acquire().unwrap());
    }
    #[test]
    fn symlink_hardlink_and_permissions_fail_before_locking() {
        let temp = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let acquire = || {
            Lease::acquire_in(
                &trust(),
                PrivateDirectory::open(temp.path(), false).unwrap(),
            )
        };
        drop(acquire().unwrap());
        let lock = temp.path().join(leaf(&trust()));
        std::fs::hard_link(&lock, temp.path().join("other")).unwrap();
        assert!(acquire().is_err());
        std::fs::remove_file(temp.path().join("other")).unwrap();
        std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(acquire().is_err());
        std::fs::remove_file(&lock).unwrap();
        symlink("missing", &lock).unwrap();
        assert!(acquire().is_err());
    }
    #[test]
    fn synthetic_lease_child() {
        let Some(directory) = std::env::var_os("MORROW_SYNTHETIC_LEASE_DIRECTORY") else {
            return;
        };
        let lease = Lease::acquire_in(
            &trust(),
            PrivateDirectory::open(Path::new(&directory), false).unwrap(),
        );
        std::process::exit(match lease {
            Ok(_) => 0,
            Err(LeaseError::Busy) => 75,
            Err(_) => 1,
        });
    }
    #[test]
    fn account_home_is_absolute_and_comes_from_passwd() {
        assert!(account_home().unwrap().is_absolute());
    }
}
