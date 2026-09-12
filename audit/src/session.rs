//! Explicit first-use protocol. Database precedes key publication and durable binding.
use crate::{
    TrustedLog,
    keys::{Key, KeyError},
    sealer::{Progress, Sealer},
};
use morrow_core::{
    dispatch::HostRuntime,
    store::{AuditBindingState, EventBudget, Store},
};
use std::{
    fs::File,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    Initialize,
    Existing,
}
#[derive(Debug)]
pub enum SessionError {
    Io(std::io::Error),
    Core(morrow_core::Error),
    Key(KeyError),
    Busy,
    InvalidPath,
    MissingDatabase,
    MissingKey,
    KeyWithoutDatabase,
    KeyMismatch,
    InitializationRequired,
}
impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "audit session: {self:?}")
    }
}
impl std::error::Error for SessionError {}
impl From<std::io::Error> for SessionError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<morrow_core::Error> for SessionError {
    fn from(e: morrow_core::Error) -> Self {
        Self::Core(e)
    }
}
impl From<KeyError> for SessionError {
    fn from(e: KeyError) -> Self {
        Self::Key(e)
    }
}
pub type Result<T> = std::result::Result<T, SessionError>;
pub struct Session {
    host: HostRuntime,
    sealer: Sealer,
    _lease: File,
}
fn sibling(database: &Path, suffix: &str) -> Result<PathBuf> {
    let mut name = database
        .file_name()
        .ok_or(SessionError::InvalidPath)?
        .to_os_string();
    name.push(suffix);
    Ok(database.with_file_name(name))
}
pub fn key_path(database: &Path) -> Result<PathBuf> {
    sibling(database, ".audit-key")
}
fn reject_link(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Err(SessionError::InvalidPath),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
impl Session {
    pub fn open(database: &Path, budget: EventBudget, mode: OpenMode) -> Result<Self> {
        let parent = database
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()?;
        let database = parent.join(database.file_name().ok_or(SessionError::InvalidPath)?);
        let key_path = key_path(&database)?;
        let lock_path = sibling(&database, ".audit-lock")?;
        for path in [&database, &key_path, &lock_path] {
            reject_link(path)?;
        }
        let lease = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lease.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => SessionError::Busy,
            std::fs::TryLockError::Error(e) => SessionError::Io(e),
        })?;
        if !database.try_exists()? {
            if key_path.try_exists()? {
                return Err(SessionError::KeyWithoutDatabase);
            }
            if mode == OpenMode::Existing {
                return Err(SessionError::MissingDatabase);
            }
            drop(Store::open(&database, budget)?);
            boundary("bootstrap-after-database");
        }
        let status = Store::audit_binding_status(&database)?;
        let key = if key_path.try_exists()? {
            if matches!(status, AuditBindingState::Uninitialized) {
                return Err(SessionError::KeyWithoutDatabase);
            }
            let key = Key::load(&key_path)?;
            if let AuditBindingState::Bound(binding) = &status {
                let trust = key.trust();
                if binding.log_id != trust.id || binding.public_key != *trust.key.as_bytes() {
                    return Err(SessionError::KeyMismatch);
                }
            }
            if mode == OpenMode::Existing && matches!(status, AuditBindingState::Unbound) {
                return Err(SessionError::InitializationRequired);
            }
            key
        } else {
            match status {
                AuditBindingState::Bound(_) | AuditBindingState::LegacySealed => {
                    return Err(SessionError::MissingKey);
                }
                AuditBindingState::Unbound | AuditBindingState::Uninitialized
                    if mode == OpenMode::Existing =>
                {
                    return Err(SessionError::InitializationRequired);
                }
                AuditBindingState::Unbound | AuditBindingState::Uninitialized => {}
            }
            // Validate/migrate old unbound data before creating any new identity.
            drop(Store::open(&database, budget)?);
            let key = Key::create(&key_path)?;
            boundary("bootstrap-after-key");
            key
        };
        let sealer = Sealer::new(key);
        let store = Store::open_audited(&database, budget, false, sealer.trust())?;
        boundary("bootstrap-after-binding");
        Ok(Self {
            host: HostRuntime::new(store)?,
            sealer,
            _lease: lease,
        })
    }
    pub fn trust(&self) -> TrustedLog {
        self.sealer.trust()
    }
    pub fn runtime(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    pub fn store(&self) -> &Store {
        self.host.store_local()
    }
    pub fn flush(&mut self, max_batches: u32) -> crate::sealer::Result<Progress> {
        self.sealer.flush(self.host.store_local_mut(), max_batches)
    }
}
fn boundary(_name: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok(_name) {
        std::process::exit(86);
    }
}
