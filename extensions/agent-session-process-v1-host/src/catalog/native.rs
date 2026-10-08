//! Original exclusive SQLite snapshot lease plus full immutable SHA-addressed wrapper files.
use super::*;
use morrow_core::plugin_package::registry::{RegistryStorage, sqlite::SqliteRegistryStorage};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
pub const MAX_WRAPPER_ARCHIVE_BYTES: usize = crate::MAX_ARCHIVE_BYTES;
pub const MAX_ARCHIVE_TOTAL_BYTES: u64 = MAX_ENTRIES as u64 * MAX_WRAPPER_ARCHIVE_BYTES as u64;
pub struct NativeCatalogStorage {
    snapshots: SqliteRegistryStorage,
    wrappers: PathBuf,
}
fn error(error: morrow_core::Error) -> Error {
    match error {
        morrow_core::Error::CommitUnknown => Error::CommitUnknown,
        morrow_core::Error::Limit => Error::Limit,
        morrow_core::Error::NotFound => Error::NotFound,
        _ => Error::Storage,
    }
}
fn plain(metadata: &fs::Metadata, directory: bool) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    if directory {
        metadata.file_type().is_dir()
    } else {
        metadata.file_type().is_file()
    }
}
fn checked(path: &Path, directory: bool) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|_| Error::Storage)?;
    if !plain(&meta, directory) {
        return Err(Error::Denied);
    }
    Ok(())
}
impl NativeCatalogStorage {
    /// Root belongs to the trusted host. Never use a guest-selected library/storage path.
    pub fn open(root: &Path, create: bool) -> Result<Self> {
        if create && !root.exists() {
            fs::create_dir_all(root).map_err(|_| Error::Storage)?;
        }
        checked(root, true)?;
        let root = root.canonicalize().map_err(|_| Error::Storage)?;
        let database = root.join("catalog.sqlite");
        if fs::symlink_metadata(&database).is_ok() {
            checked(&database, false)?;
        }
        let snapshots = SqliteRegistryStorage::open(&database, create).map_err(error)?;
        let wrappers = root.join("wrappers");
        if create && !wrappers.exists() {
            fs::create_dir(&wrappers).map_err(|_| Error::Storage)?;
        }
        checked(&wrappers, true)?;
        let storage = Self {
            snapshots,
            wrappers,
        };
        storage.quota()?;
        Ok(storage)
    }
    fn path(&self, sha: [u8; 32]) -> PathBuf {
        self.wrappers.join(format!("{}.magent", hex(&sha)))
    }
    fn quota(&self) -> Result<(usize, u64)> {
        checked(&self.wrappers, true)?;
        let mut count = 0usize;
        let mut bytes = 0u64;
        for entry in fs::read_dir(&self.wrappers)
            .map_err(|_| Error::Storage)?
            .take(MAX_ENTRIES + 1)
        {
            let entry = entry.map_err(|_| Error::Storage)?;
            let meta = fs::symlink_metadata(entry.path()).map_err(|_| Error::Storage)?;
            if !plain(&meta, false) {
                return Err(Error::Denied);
            }
            count += 1;
            bytes = bytes.checked_add(meta.len()).ok_or(Error::Limit)?;
            if count > MAX_ENTRIES || bytes > MAX_ARCHIVE_TOTAL_BYTES {
                return Err(Error::Limit);
            }
        }
        Ok((count, bytes))
    }
    fn archive(&self, sha: [u8; 32]) -> Result<Vec<u8>> {
        checked(&self.wrappers, true)?;
        let path = self.path(sha);
        checked(&path, false)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1).custom_flags(0x00200000); // Open reparse point itself; no write/delete sharing.
        }
        let mut file = options.open(path).map_err(|_| Error::Storage)?;
        if !plain(&file.metadata().map_err(|_| Error::Storage)?, false) {
            return Err(Error::Denied);
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_WRAPPER_ARCHIVE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Storage)?;
        if bytes.len() > MAX_WRAPPER_ARCHIVE_BYTES {
            return Err(Error::Limit);
        }
        if morrow_agent_session_exec_v1_r2::hash(&bytes) != sha {
            return Err(Error::Contract);
        }
        Ok(bytes)
    }
}
impl WrapperCatalogStorage for NativeCatalogStorage {
    fn read(&self) -> Result<Option<Vec<u8>>> {
        self.snapshots.read().map_err(error)
    }
    fn publish(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > morrow_core::plugin_package::registry::MAX_CONTAINER {
            return Err(Error::Limit);
        }
        self.snapshots.publish(bytes).map_err(error)
    }
    fn load(&self, sha: [u8; 32]) -> Result<AgentProcessPackage> {
        let p = AgentProcessPackage::decode(&self.archive(sha)?)?;
        let base = self
            .snapshots
            .load_package(p.base_sha256())
            .map_err(error)?;
        if base.archive() != p.base().archive() {
            return Err(Error::Contract);
        }
        Ok(p)
    }
    fn install(&self, p: &AgentProcessPackage) -> Result<()> {
        checked(&self.wrappers, true)?;
        let (count, total) = self.quota()?;
        let sha = p.review_sha256();
        let target = self.path(sha);
        match fs::symlink_metadata(&target) {
            Ok(_) => {
                if self.archive(sha)? != p.archive() {
                    return Err(Error::Contract);
                }
                self.snapshots.install_package(p.base()).map_err(error)?;
                return self.load(sha).map(|_| ());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(Error::Storage),
        }
        if count >= MAX_ENTRIES
            || total
                .checked_add(p.archive().len() as u64)
                .is_none_or(|v| v > MAX_ARCHIVE_TOTAL_BYTES)
        {
            return Err(Error::Limit);
        }
        let mut staged =
            tempfile::NamedTempFile::new_in(&self.wrappers).map_err(|_| Error::Storage)?;
        staged.write_all(p.archive()).map_err(|_| Error::Storage)?;
        staged.as_file().sync_all().map_err(|_| Error::Storage)?;
        match staged.persist_noclobber(target) {
            Ok(file) => {
                file.sync_all().map_err(|_| Error::CommitUnknown)?;
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(Error::Storage),
        }
        #[cfg(unix)]
        fs::File::open(&self.wrappers)
            .and_then(|file| file.sync_all())
            .map_err(|_| Error::CommitUnknown)?;
        // Publish the full file before adding a base-cache row, so failed imports stay quota-counted.
        self.snapshots.install_package(p.base()).map_err(error)?;
        let saved = self.load(sha)?;
        if saved.archive() != p.archive() {
            return Err(Error::Contract);
        }
        Ok(())
    }
}
