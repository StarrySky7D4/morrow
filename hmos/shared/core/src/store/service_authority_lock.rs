//! Service-authority coordination for cooperating hosts of the same OS user.
//! The lock is separate from SQLite so ordinary content and snapshots remain
//! available. A persisted database identity, not a pathname, selects the lock:
//! hard links, renames and copies with the same identity conservatively contend.
//! The fixed per-user lock directory and database metadata are trusted profile
//! state, not protection against a user who can replace either arbitrarily.
use crate::{Error, Result};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[cfg(not(target_arch = "wasm32"))]
use std::{fs::File, path::PathBuf};

mod epochs;
use epochs::Epochs;

/// Exact persisted dependency in this Store; namespaces never alias each other.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ServiceAuthorityResource {
    Configuration(String),
    Inbound([u8; 32]),
    Outbound([u8; 32]),
    TlsIdentity([u8; 32]),
}
impl ServiceAuthorityResource {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Configuration(id) => crate::identity(id),
            Self::Inbound(id) | Self::Outbound(id) | Self::TlsIdentity(id) if *id != [0; 32] => {
                Ok(())
            }
            _ => Err(Error::Invalid("authority dependency")),
        }
    }
}

struct State {
    alive: AtomicBool,
    epoch: Mutex<Epochs>,
}
/// A read-only observation of an authority resolution from one original Store.
/// It cannot extend the Store lifetime, restore a revoked epoch, or grant IO.
#[derive(Clone)]
pub struct ServiceAuthorityLease {
    state: Arc<State>,
    global: Arc<AtomicBool>,
    writes: Option<Arc<AtomicBool>>,
    dependencies: Vec<Arc<AtomicBool>>,
}
impl ServiceAuthorityLease {
    pub fn check(&self) -> Result<()> {
        if !self.state.alive.load(Ordering::Acquire)
            || self.global.load(Ordering::Acquire)
            || self
                .writes
                .as_ref()
                .is_some_and(|v| v.load(Ordering::Acquire))
            || self.dependencies.iter().any(|v| v.load(Ordering::Acquire))
        {
            return Err(Error::Invalid("inactive service authority"));
        }
        Ok(())
    }
}
/// Thread-safe invalidation only. Holding this control does not keep the owning
/// Store alive and cannot issue a replacement lease or release its native lock.
#[derive(Clone)]
pub struct ServiceAuthorityControl {
    state: Arc<State>,
}
impl ServiceAuthorityControl {
    pub fn revoke_all(&self) {
        let mut epoch = self.state.epoch.lock().unwrap_or_else(|poisoned| {
            self.state.alive.store(false, Ordering::Release);
            poisoned.into_inner()
        });
        epoch.revoke_all();
    }
    /// Revoke exact dependents and every legacy all-writes lease. Re-resolution
    /// is required even when the following write rolls back or remains unknown.
    pub fn revoke_resource(&self, resource: &ServiceAuthorityResource) {
        let mut epoch = self.state.epoch.lock().unwrap_or_else(|poisoned| {
            self.state.alive.store(false, Ordering::Release);
            poisoned.into_inner()
        });
        epoch.revoke(resource);
    }
}
/// A transient writer reservation, or another reference to this Store's pin.
/// Never relock a cloned native handle. Keep the reservation through commit,
/// rollback, or an uncertain commit result.
pub(crate) struct ServiceAuthorityWriteGuard {
    #[cfg(not(target_arch = "wasm32"))]
    _file: Option<Arc<File>>,
}
pub(crate) struct ServiceAuthorityCoordinator {
    store_id: [u8; 32],
    native_restore_supported: bool,
    state: Arc<State>,
    #[cfg(not(target_arch = "wasm32"))]
    pinned: Option<Arc<File>>,
}
impl ServiceAuthorityCoordinator {
    pub(crate) fn new(store_id: [u8; 32], native_restore_supported: bool) -> Self {
        Self {
            store_id,
            native_restore_supported,
            state: Arc::new(State {
                alive: AtomicBool::new(true),
                epoch: Mutex::new(Epochs::default()),
            }),
            #[cfg(not(target_arch = "wasm32"))]
            pinned: None,
        }
    }
    pub(crate) fn control(&self) -> ServiceAuthorityControl {
        ServiceAuthorityControl {
            state: self.state.clone(),
        }
    }
    pub(crate) fn validate(&self, lease: &ServiceAuthorityLease) -> Result<()> {
        if !Arc::ptr_eq(&self.state, &lease.state) {
            return Err(Error::Invalid("foreign service authority"));
        }
        lease.check()
    }
    pub(crate) fn pin(&mut self) -> Result<ServiceAuthorityLease> {
        if !self.native_restore_supported {
            return Err(Error::UnsupportedVersion);
        }
        self.check_owner()?;
        #[cfg(target_arch = "wasm32")]
        {
            Err(Error::UnsupportedVersion)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.pinned.is_none() {
                self.pinned = Some(Arc::new(acquire(self.store_id)?));
            }
            let epochs = self.state.epoch.lock().map_err(|_| Error::Integrity)?;
            let lease = ServiceAuthorityLease {
                state: self.state.clone(),
                global: epochs.global.clone(),
                writes: Some(epochs.writes.clone()),
                dependencies: vec![],
            };
            lease.check()?;
            Ok(lease)
        }
    }
    /// Convert a still-valid all-writes resolution guard after reading records.
    /// Validation and dependency capture share the invalidation mutex: a revoke
    /// between loading a record and narrowing cannot issue a fresh valid lease.
    pub(crate) fn narrow(
        &self,
        guard: &ServiceAuthorityLease,
        resources: &[ServiceAuthorityResource],
    ) -> Result<ServiceAuthorityLease> {
        if !Arc::ptr_eq(&self.state, &guard.state) || guard.writes.is_none() {
            return Err(Error::Invalid("authority resolution guard"));
        }
        if resources.is_empty() || resources.len() > 66 {
            return Err(Error::Limit);
        }
        for resource in resources {
            resource.validate()?;
        }
        let mut epochs = self.state.epoch.lock().map_err(|_| Error::Integrity)?;
        guard.check()?;
        let dependencies = epochs.bind(resources)?;
        let lease = ServiceAuthorityLease {
            state: self.state.clone(),
            global: guard.global.clone(),
            writes: None,
            dependencies,
        };
        lease.check()?;
        Ok(lease)
    }
    pub(crate) fn writer(&self) -> Result<ServiceAuthorityWriteGuard> {
        self.check_owner()?;
        // Unsupported adapters cannot create a lease. Their storage codecs and
        // local CAS writes remain usable without pretending to publish authority.
        #[cfg(target_arch = "wasm32")]
        {
            Ok(ServiceAuthorityWriteGuard {})
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let file = if !self.native_restore_supported {
                None
            } else if let Some(file) = &self.pinned {
                Some(file.clone())
            } else {
                Some(Arc::new(acquire(self.store_id)?))
            };
            Ok(ServiceAuthorityWriteGuard { _file: file })
        }
    }
    fn check_owner(&self) -> Result<()> {
        if !self.state.alive.load(Ordering::Acquire)
            || (self.native_restore_supported && self.store_id == [0; 32])
        {
            return Err(Error::Integrity);
        }
        Ok(())
    }
}
impl Drop for ServiceAuthorityCoordinator {
    fn drop(&mut self) {
        // Atomic invalidation precedes the automatic release of the pinned file.
        // Leases/control clones retain State, never the owning coordinator.
        self.state.alive.store(false, Ordering::Release);
        self.control().revoke_all();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn lock_root() -> Result<PathBuf> {
    #[cfg(windows)]
    let path = {
        let base = std::env::var_os("LOCALAPPDATA")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or(Error::UnsupportedVersion)?;
        base.join("Morrow").join("service-authority-locks")
    };
    #[cfg(unix)]
    let path = {
        let base = if let Some(value) = std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty())
        {
            PathBuf::from(value)
        } else {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .ok_or(Error::UnsupportedVersion)?
                .join(".local")
                .join("state")
        };
        base.join("morrow").join("service-authority-locks")
    };
    #[cfg(not(any(windows, unix)))]
    {
        return Err(Error::UnsupportedVersion);
    }
    #[cfg(any(windows, unix))]
    {
        if !path.is_absolute() {
            return Err(Error::Invalid("service lock root"));
        }
        std::fs::create_dir_all(&path).map_err(|_| Error::Io)?;
        let canonical = std::fs::canonicalize(&path).map_err(|_| Error::Io)?;
        if !canonical.is_dir() {
            return Err(Error::Invalid("service lock root"));
        }
        Ok(canonical)
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn lock_path(store_id: [u8; 32]) -> Result<PathBuf> {
    if store_id == [0; 32] {
        return Err(Error::Integrity);
    }
    let name: String = store_id.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(lock_root()?.join(format!("{name}.lock")))
}
#[cfg(not(target_arch = "wasm32"))]
fn regular(metadata: &std::fs::Metadata) -> bool {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}
#[cfg(not(target_arch = "wasm32"))]
fn acquire(store_id: [u8; 32]) -> Result<File> {
    let path = lock_path(store_id)?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if !regular(&metadata) => return Err(Error::Invalid("service lock file")),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(Error::Io),
    }
    let mut options = File::options();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | FILE_SHARE_WRITE; exclude delete/rename while held.
        options.share_mode(3);
    }
    let file = options.open(&path).map_err(|_| Error::Io)?;
    let opened = file.metadata().map_err(|_| Error::Io)?;
    let current = std::fs::symlink_metadata(&path).map_err(|_| Error::Io)?;
    if !regular(&opened) || !regular(&current) {
        return Err(Error::Invalid("service lock file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.dev() != current.dev() || opened.ino() != current.ino() {
            return Err(Error::Invalid("replaced service lock file"));
        }
    }
    file.try_lock().map_err(|error| match error {
        std::fs::TryLockError::WouldBlock => Error::StorageBusy,
        std::fs::TryLockError::Error(_) => Error::Io,
    })?;
    Ok(file)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod resource_tests;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    pub(super) fn id() -> [u8; 32] {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mut id = [0; 32];
        id[..8].copy_from_slice(&u64::from(std::process::id()).to_le_bytes());
        id[8..24].copy_from_slice(
            &std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .to_le_bytes(),
        );
        id[24..].copy_from_slice(&NEXT.fetch_add(1, Ordering::Relaxed).to_le_bytes());
        id
    }
    #[test]
    fn original_owner_epoch_rotation_and_drop_invalidate_all_copies() {
        let identity = id();
        let mut owner = ServiceAuthorityCoordinator::new(identity, true);
        let control = owner.control();
        let first = owner.pin().unwrap();
        let copy = first.clone();
        first.check().unwrap();
        let other = ServiceAuthorityCoordinator::new(id(), true);
        assert!(other.validate(&first).is_err());
        let writer = owner.writer().unwrap();
        control.revoke_all();
        assert!(first.check().is_err());
        assert!(copy.check().is_err());
        let fresh = owner.pin().unwrap();
        owner.validate(&fresh).unwrap();
        drop(writer);
        drop(owner);
        assert!(fresh.check().is_err());
        control.revoke_all();
        assert!(fresh.check().is_err());
        let mut next_owner = ServiceAuthorityCoordinator::new(identity, true);
        next_owner.pin().unwrap().check().unwrap();
    }
    #[test]
    fn independent_stores_cannot_write_or_pin_active_identity() {
        let identity = id();
        let mut owner = ServiceAuthorityCoordinator::new(identity, true);
        let mut other = ServiceAuthorityCoordinator::new(identity, true);
        let temporary = other.writer().unwrap();
        assert_eq!(owner.pin().err(), Some(Error::StorageBusy));
        drop(temporary);
        let live = owner.pin().unwrap();
        assert_eq!(other.pin().err(), Some(Error::StorageBusy));
        assert_eq!(other.writer().err(), Some(Error::StorageBusy));
        let writer = owner.writer().unwrap();
        drop(owner);
        assert!(live.check().is_err());
        // An outstanding writer retains the pin through its transaction.
        assert_eq!(other.pin().err(), Some(Error::StorageBusy));
        drop(writer);
        other.pin().unwrap();
    }
    #[test]
    fn unsupported_memory_restore_rejects_live_lease_but_preserves_local_storage() {
        let mut owner = ServiceAuthorityCoordinator::new(id(), false);
        assert_eq!(owner.pin().err(), Some(Error::UnsupportedVersion));
        owner.writer().unwrap();
        owner.control().revoke_all();
        owner.writer().unwrap();
        assert_eq!(owner.pin().err(), Some(Error::UnsupportedVersion));
    }
    #[test]
    fn preexisting_nonregular_lock_is_rejected_without_replacement() {
        let identity = id();
        let path = lock_path(identity).unwrap();
        std::fs::create_dir(&path).unwrap();
        let mut owner = ServiceAuthorityCoordinator::new(identity, true);
        assert!(matches!(
            owner.pin(),
            Err(Error::Invalid("service lock file"))
        ));
        assert!(path.is_dir());
        std::fs::remove_dir(&path).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlink_lock_is_rejected_even_when_target_is_regular() {
        let identity = id();
        let path = lock_path(identity).unwrap();
        let target = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(target.path(), &path).unwrap();
        let mut owner = ServiceAuthorityCoordinator::new(identity, true);
        assert!(matches!(
            owner.pin(),
            Err(Error::Invalid("service lock file"))
        ));
        std::fs::remove_file(&path).unwrap();
    }
    #[test]
    #[ignore = "child harness; parent supplies a dedicated metadata identity"]
    fn child_cannot_acquire_another_process_authority_lock() {
        let encoded = std::env::var("MORROW_SERVICE_LOCK_TEST_ID").expect("child identity");
        assert_eq!(encoded.len(), 64);
        let mut identity = [0; 32];
        for (i, byte) in identity.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&encoded[i * 2..i * 2 + 2], 16).unwrap();
        }
        let mut owner = ServiceAuthorityCoordinator::new(identity, true);
        assert_eq!(owner.pin().err(), Some(Error::StorageBusy));
        assert_eq!(owner.writer().err(), Some(Error::StorageBusy));
        // Distinguish actual child execution from an accidentally empty filter.
        std::process::exit(42);
    }
    #[test]
    fn active_authority_blocks_writers_in_an_independent_process() {
        let identity = id();
        let mut owner = ServiceAuthorityCoordinator::new(identity, true);
        let lease = owner.pin().unwrap();
        let encoded: String = identity.iter().map(|byte| format!("{byte:02x}")).collect();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args([
            "--exact",
            "store::service_authority_lock::tests::child_cannot_acquire_another_process_authority_lock",
            "--ignored",
            "--nocapture",
        ]).env("MORROW_SERVICE_LOCK_TEST_ID", encoded).stdin(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    assert_eq!(status.code(), Some(42));
                    break;
                }
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                other => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("service-lock child did not finish: {other:?}");
                }
            }
        }
        lease.check().unwrap();
    }
}
