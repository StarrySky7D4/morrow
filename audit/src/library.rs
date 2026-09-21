//! Trusted Windows active-library routing; does not attest the latest history.
use crate::{
    TrustedLog,
    keys::Key,
    session::{OpenMode, Session, SessionError, key_path, reject_link},
};
use morrow_core::store::{EventBudget, Store};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.library.rs"));
}
const MAX: usize = 32768;
const MAGIC: &[u8; 8] = b"MORROWL1";
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Session(SessionError),
    Busy,
    Invalid,
    IdentityMismatch,
    PublishUnknown,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "library selection: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<SessionError> for Error {
    fn from(e: SessionError) -> Self {
        Self::Session(e)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
/// Keep this guard alive as long as its selected Session is running.
pub struct Registry {
    root: PathBuf,
    selection: Option<proto::Selection>,
    _lease: File,
    uncertain: bool,
}
impl Registry {
    pub fn open(root: &Path) -> Result<Self> {
        reject_link(root)?;
        std::fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        let lock = root.join("active-library.lock");
        let path = root.join("active-library.pb.lz4");
        reject_link(&lock)?;
        reject_link(&path)?;
        let lease = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(3)
            .open(lock)?;
        lease.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => Error::Busy,
            std::fs::TryLockError::Error(e) => Error::Io(e),
        })?;
        let selection = match File::open(path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take((MAX + 1) as u64).read_to_end(&mut bytes)?;
                Some(decode(&bytes)?)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            root,
            selection,
            _lease: lease,
            uncertain: false,
        })
    }
    pub fn generation(&self) -> u64 {
        self.selection.as_ref().map_or(0, |s| s.generation)
    }
    pub fn selected_database(&self) -> Result<PathBuf> {
        self.certain()?;
        let dir = match &self.selection {
            Some(s) => {
                let path = Path::new(&s.directory);
                reject_link(path)?;
                let actual = path.canonicalize()?;
                if actual != path || !actual.is_dir() {
                    return Err(Error::Invalid);
                }
                actual
            }
            None => self.root.clone(),
        };
        Ok(dir.join("workbench.db"))
    }
    pub fn open_session(&mut self, budget: EventBudget) -> Result<Session> {
        let database = self.selected_database()?;
        let session = if let Some(selected) = &self.selection {
            // Routing only compares the protected identity. Session re-reads
            // and checks it under the database lease before a verified open;
            // scanning the entire database here duplicates that verification.
            let trust = protected_trust(&database)?;
            if trust.id != selected.log_id || trust.key.as_bytes().as_slice() != selected.public_key
            {
                return Err(Error::IdentityMismatch);
            }
            Session::open_expected(&database, budget, &trust)?
        } else {
            Session::open(&database, budget, OpenMode::Initialize)?
        };
        if self.selection.is_none() {
            self.publish(&self.root.clone(), &session.trust())?;
        }
        Ok(session)
    }
    /// Explicit trusted user selection. Caller must stop the previous host first.
    /// A selected older snapshot is allowed; no latest-head or anti-rollback claim.
    pub fn activate(&mut self, directory: &Path) -> Result<()> {
        self.certain()?;
        reject_link(directory)?;
        let directory = directory.canonicalize()?;
        if !directory.is_dir() {
            return Err(Error::Invalid);
        }
        let database = directory.join("workbench.db");
        let trust = readonly_trust(&database)?;
        let session = Session::open_expected(&database, EventBudget::default(), &trust)?;
        self.publish(&directory, &session.trust())
    }
    pub fn restore_key(&self, selected_key: &Path) -> Result<()> {
        self.certain()?;
        if let Some(selected) = &self.selection {
            let key = Key::load(selected_key).map_err(SessionError::from)?;
            let trust = key.trust();
            if trust.id != selected.log_id || trust.key.as_bytes().as_slice() != selected.public_key
            {
                return Err(Error::IdentityMismatch);
            }
        }
        crate::recovery::restore_key(&self.selected_database()?, selected_key)?;
        Ok(())
    }
    fn certain(&self) -> Result<()> {
        if self.uncertain {
            Err(Error::PublishUnknown)
        } else {
            Ok(())
        }
    }
    fn publish(&mut self, directory: &Path, trust: &TrustedLog) -> Result<()> {
        let next = proto::Selection {
            version: 1,
            generation: self.generation().checked_add(1).ok_or(Error::Invalid)?,
            directory: directory.to_str().ok_or(Error::Invalid)?.into(),
            log_id: trust.id.clone(),
            public_key: trust.key.as_bytes().to_vec(),
            previous_directory: self
                .selection
                .as_ref()
                .map_or(String::new(), |s| s.directory.clone()),
        };
        let raw = next.encode_to_vec();
        if raw.len() > MAX / 2 {
            return Err(Error::Invalid);
        }
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&Sha256::digest(&raw));
        bytes.extend(lz4_flex::block::compress_prepend_size(&raw));
        let path = self.root.join("active-library.pb.lz4");
        reject_link(&path)?;
        let mut staged = tempfile::NamedTempFile::new_in(&self.root)?;
        staged.write_all(&bytes)?;
        staged.as_file().sync_all()?;
        boundary("library-before-publish");
        self.uncertain = true;
        let published = staged.persist(&path).map_err(|_| Error::PublishUnknown)?;
        boundary("library-after-publish");
        #[cfg(feature = "fault-injection")]
        if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok("library-publication-uncertain")
        {
            return Err(Error::PublishUnknown);
        }
        published.sync_all().map_err(|_| Error::PublishUnknown)?;
        drop(published);
        let mut saved = Vec::new();
        File::open(&path)
            .map_err(|_| Error::PublishUnknown)?
            .take((MAX + 1) as u64)
            .read_to_end(&mut saved)
            .map_err(|_| Error::PublishUnknown)?;
        if saved != bytes {
            return Err(Error::PublishUnknown);
        }
        self.selection = Some(next);
        self.uncertain = false;
        Ok(())
    }
}
fn protected_trust(database: &Path) -> Result<TrustedLog> {
    reject_link(database)?;
    let key = key_path(database)?;
    reject_link(&key)?;
    Ok(Key::load(&key).map_err(SessionError::from)?.trust())
}
fn readonly_trust(database: &Path) -> Result<TrustedLog> {
    let trust = protected_trust(database)?;
    // This constructor already performs a complete integrity check.
    let _store =
        Store::open_read_only_audited(database, trust.clone()).map_err(SessionError::from)?;
    Ok(trust)
}
fn decode(bytes: &[u8]) -> Result<proto::Selection> {
    if bytes.len() < 44 || bytes.len() > MAX || &bytes[..8] != MAGIC {
        return Err(Error::Invalid);
    }
    let length = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
    if length == 0 || length > MAX / 2 {
        return Err(Error::Invalid);
    }
    let raw =
        lz4_flex::block::decompress_size_prepended(&bytes[40..]).map_err(|_| Error::Invalid)?;
    if Sha256::digest(&raw).as_slice() != &bytes[8..40] {
        return Err(Error::Invalid);
    }
    let state = proto::Selection::decode(raw.as_slice()).map_err(|_| Error::Invalid)?;
    if state.encode_to_vec() != raw {
        return Err(Error::Invalid);
    }
    if state.version != 1
        || state.generation == 0
        || state.directory.is_empty()
        || !Path::new(&state.directory).is_absolute()
        || state.directory.contains('\0')
        || state.log_id.is_empty()
        || state.log_id.len() > 1024
        || state.public_key.len() != 32
    {
        return Err(Error::Invalid);
    }
    Ok(state)
}
fn boundary(_name: &str) {
    #[cfg(feature = "fault-injection")]
    if std::env::var("MORROW_AUDIT_CRASH_AT").as_deref() == Ok(_name) {
        std::process::exit(86);
    }
}
