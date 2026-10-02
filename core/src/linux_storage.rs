//! Linux private-storage metadata validation, not SQLite connection admission.
//! This helper does not attest the inode actually opened by SQLite. It must not
//! wrap an ordinary pathname SQLite open as a protected substitute. Private
//! Store entry points deliberately remain unavailable pending that proof and
//! complete transaction/snapshot integration. Existing inodes are inspected only
//! with O_PATH: closing a normal descriptor would release this process's SQLite
//! POSIX record locks, even if SQLite owns another descriptor for the same inode.
use std::{
    ffi::{CStr, CString},
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::{Component, Path, PathBuf},
};

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "unsafe private storage")
}
pub fn current_uid() -> u32 {
    unsafe { libc::geteuid() }
}
/// Account database only; HOME/XDG environment variables are not identity proof.
pub fn account_home() -> io::Result<PathBuf> {
    let uid = current_uid();
    if unsafe { libc::getuid() } != uid {
        return Err(invalid());
    }
    let mut size = 16 * 1024;
    loop {
        let mut buffer = vec![0u8; size];
        let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut found = std::ptr::null_mut();
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                pwd.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut found,
            )
        };
        if status == libc::ERANGE && size < 1024 * 1024 {
            size *= 2;
            continue;
        }
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status));
        }
        if found.is_null() {
            return Err(invalid());
        }
        let pwd = unsafe { pwd.assume_init() };
        if pwd.pw_uid != uid || pwd.pw_dir.is_null() {
            return Err(invalid());
        }
        let bytes = unsafe { CStr::from_ptr(pwd.pw_dir) }.to_bytes();
        let home = PathBuf::from(std::ffi::OsStr::from_bytes(bytes));
        if !home.is_absolute() {
            return Err(invalid());
        }
        return Ok(home);
    }
}
fn name(path: &Path) -> io::Result<CString> {
    let mut parts = path.components();
    if !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some() {
        return Err(invalid());
    }
    CString::new(path.as_os_str().as_bytes()).map_err(|_| invalid())
}
fn openat(dir: &File, leaf: &CString, flags: i32, mode: u32) -> io::Result<File> {
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            leaf.as_ptr(),
            flags | libc::O_CLOEXEC,
            mode,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn private_dir(file: &File) -> io::Result<()> {
    let m = file.metadata()?;
    if !m.is_dir() || m.uid() != current_uid() || m.mode() & 0o7777 != 0o700 {
        return Err(invalid());
    }
    Ok(())
}
fn private_file(file: &File) -> io::Result<std::fs::Metadata> {
    private_file_for_owner(file, current_uid())
}
fn private_file_for_owner(file: &File, owner: u32) -> io::Result<std::fs::Metadata> {
    let m = file.metadata()?;
    if !m.is_file() || m.uid() != owner || m.nlink() != 1 || m.mode() & 0o7777 != 0o600 {
        return Err(invalid());
    }
    Ok(m)
}
/// An anchored no-follow directory. Only the final private directory must be
/// 0700; every ancestor is opened component-by-component without following links.
pub struct PrivateDirectory {
    file: File,
    path: PathBuf,
}
impl PrivateDirectory {
    pub fn open(path: &Path, create: bool) -> io::Result<Self> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open("/")?;
        for component in absolute.components() {
            match component {
                Component::RootDir | Component::CurDir => continue,
                Component::Normal(part) => {
                    let leaf = CString::new(part.as_bytes()).map_err(|_| invalid())?;
                    let next = openat(
                        &file,
                        &leaf,
                        libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW,
                        0,
                    );
                    file = match next {
                        Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                            // Existing untrusted ancestors must not be silently repaired.
                            let m = file.metadata()?;
                            if !m.is_dir() || m.uid() != current_uid() || m.mode() & 0o022 != 0 {
                                return Err(invalid());
                            }
                            let rc =
                                unsafe { libc::mkdirat(file.as_raw_fd(), leaf.as_ptr(), 0o700) };
                            if rc < 0 {
                                return Err(io::Error::last_os_error());
                            }
                            openat(
                                &file,
                                &leaf,
                                libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW,
                                0,
                            )?
                        }
                        value => value?,
                    };
                }
                _ => return Err(invalid()),
            }
        }
        private_dir(&file)?;
        Ok(Self {
            file,
            path: absolute,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn verify(&self) -> io::Result<()> {
        private_dir(&self.file)?;
        let current = Self::open(&self.path, false)?;
        let a = self.file.metadata()?;
        let b = current.file.metadata()?;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return Err(invalid());
        }
        Ok(())
    }
    /// Sync directory publication durability. A normal directory descriptor is
    /// safe here: it is never a descriptor for an SQLite database/sidecar inode.
    pub fn sync(&self) -> io::Result<()> {
        self.verify()?;
        let dot = CString::new(".").expect("fixed directory component");
        let directory = openat(
            &self.file,
            &dot,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            0,
        )?;
        directory.sync_all()?;
        self.verify()
    }
    pub fn open_read(&self, leaf: &Path) -> io::Result<File> {
        self.verify()?;
        let file = openat(
            &self.file,
            &name(leaf)?,
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            0,
        )?;
        private_file(&file)?;
        self.verify()?;
        Ok(file)
    }
    pub fn create_new(&self, leaf: &Path) -> io::Result<File> {
        self.verify()?;
        let file = openat(
            &self.file,
            &name(leaf)?,
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
            0o600,
        )?;
        private_file(&file)?;
        self.verify()?;
        Ok(file)
    }
    pub fn inspect(&self, leaf: &Path) -> io::Result<File> {
        // O_PATH descriptors never participate in POSIX record locks. Do not
        // replace this with File::open/OpenOptions::read, including in tests.
        let leaf = name(leaf)?;
        let file = openat(&self.file, &leaf, libc::O_PATH | libc::O_NOFOLLOW, 0)?;
        let opened = private_file(&file)?;
        // Recheck the current pathname through the anchored directory without
        // opening a normal descriptor on an SQLite/sidecar inode.
        let mut current = std::mem::MaybeUninit::<libc::stat>::uninit();
        let rc = unsafe {
            libc::fstatat(
                self.file.as_raw_fd(),
                leaf.as_ptr(),
                current.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        let current = unsafe { current.assume_init() };
        if current.st_dev != opened.dev()
            || current.st_ino != opened.ino()
            || current.st_uid != current_uid()
            || current.st_nlink != 1
            || current.st_mode & libc::S_IFMT != libc::S_IFREG
            || current.st_mode & 0o7777 != 0o600
        {
            return Err(invalid());
        }
        Ok(file)
    }
}
/// Pinned pathname metadata plus anchored private sidecar validation.
/// This is not proof of SQLite's actually opened descriptor/inode and does not
/// establish protected SQLite admission, even if pre/post checks pass.
pub struct SqlitePermissions {
    dir: PrivateDirectory,
    leaf: PathBuf,
    pin: File,
}
impl SqlitePermissions {
    pub fn prepare(path: &Path, create: bool) -> io::Result<Self> {
        let leaf = path.file_name().ok_or_else(invalid)?;
        let dir = PrivateDirectory::open(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
            create,
        )?;
        let leaf = PathBuf::from(leaf);
        check_sidecars(&dir, &leaf)?;
        let pin = match dir.inspect(&leaf) {
            Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                drop(dir.create_new(&leaf)?);
                dir.inspect(&leaf)?
            }
            value => value?,
        };
        let guard = Self { dir, leaf, pin };
        guard.verify()?;
        Ok(guard)
    }
    pub fn verify(&self) -> io::Result<()> {
        self.dir.verify()?;
        let a = private_file(&self.pin)?;
        let current = self.dir.inspect(&self.leaf)?;
        let b = private_file(&current)?;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return Err(invalid());
        }
        check_sidecars(&self.dir, &self.leaf)?;
        self.dir.verify()
    }
}

fn check_sidecars(dir: &PrivateDirectory, leaf: &Path) -> io::Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = leaf.as_os_str().to_os_string();
        sidecar.push(suffix);
        match dir.inspect(Path::new(&sidecar)) {
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod owner_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn private_metadata_owner_rule_rejects_foreign_expected_uid() {
        let dir = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let private = PrivateDirectory::open(dir.path(), false).unwrap();
        let file = private.create_new(Path::new("owned")).unwrap();
        let metadata = private_file(&file).unwrap();
        assert_eq!(metadata.uid(), current_uid());
        // Synthetic owner-policy unit: a different expected UID is rejected.
        // This does not claim a cross-UID filesystem/SecretService integration.
        assert!(private_file_for_owner(&file, current_uid().wrapping_add(1)).is_err());
    }
}
