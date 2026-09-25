//! Native lease and atomic-file implementation; the shared rules own serialization.
use super::{MAX_CONTAINER, Registry, RegistryStorage};
use crate::{
    Error, Result,
    plugin_package::{Package, catalog::Catalog},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
struct FileStorage {
    root: PathBuf,
    catalog: Catalog,
    _lease: File,
}
fn regular_or_absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        Ok(_) => Err(Error::Invalid("registry file type")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(Error::Io),
    }
}
impl Registry {
    /// Registry and catalog roots belong to the trusted host, not to package metadata.
    pub fn open(root: &Path, catalog: Catalog) -> Result<Self> {
        if let Ok(meta) = fs::symlink_metadata(root)
            && !meta.file_type().is_dir()
        {
            return Err(Error::Invalid("registry directory type"));
        }
        fs::create_dir_all(root).map_err(|_| Error::Io)?;
        let root = root.canonicalize().map_err(|_| Error::Io)?;
        let lock = root.join("registry.lock");
        regular_or_absent(&lock)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3); // Keep the locked file from being deleted/replaced while live.
        }
        let lease = options.open(lock).map_err(|_| Error::Io)?;
        lease.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => Error::StorageBusy,
            std::fs::TryLockError::Error(_) => Error::Io,
        })?;
        Self::from_storage(Box::new(FileStorage {
            root,
            catalog,
            _lease: lease,
        }))
    }
}
impl FileStorage {
    fn path(&self) -> PathBuf {
        self.root.join("selection.morrow")
    }
}
impl RegistryStorage for FileStorage {
    fn read(&self) -> Result<Option<Vec<u8>>> {
        let path = self.path();
        if !regular_or_absent(&path)? {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|_| Error::Io)?
            .take(MAX_CONTAINER as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Io)?;
        Ok(Some(bytes))
    }
    fn publish(&mut self, bytes: &[u8]) -> Result<()> {
        regular_or_absent(&self.path())?;
        let mut staged = tempfile::NamedTempFile::new_in(&self.root).map_err(|_| Error::Io)?;
        staged.write_all(bytes).map_err(|_| Error::Io)?;
        staged.as_file().sync_all().map_err(|_| Error::Io)?;
        staged.persist(self.path()).map_err(|_| Error::Io)?;
        Ok(())
    }
    fn load_package(&self, digest: [u8; 32]) -> Result<Package> {
        self.catalog.load(digest)
    }
    fn install_package(&self, package: &Package) -> Result<()> {
        self.catalog.install(package).map(|_| ())
    }
}
