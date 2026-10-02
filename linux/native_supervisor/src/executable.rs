use crate::{above_stdio, invalid, kernel_error};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileExt, MetadataExt, OpenOptionsExt},
        },
    },
    path::Path,
};

pub const MAX_EXECUTABLE_BYTES: u64 = 32 * 1024 * 1024;
const REQUIRED_SEALS: i32 = libc::F_SEAL_WRITE
    | libc::F_SEAL_GROW
    | libc::F_SEAL_SHRINK
    | libc::F_SEAL_SEAL
    | libc::F_SEAL_EXEC;

pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceIdentity {
    pub device: u64,
    pub inode: u64,
    pub length: u64,
}

/// Private, owned, immutable executable object. No writable mapping is created.
/// Requires Linux's explicit MFD_EXEC and F_SEAL_EXEC support; no pathname,
/// /proc descriptor or unsealed execution fallback is provided.
pub struct SealedExecutable {
    file: File,
    digest: [u8; 32],
    source: SourceIdentity,
}

impl SealedExecutable {
    pub fn read(path: &Path, expected_digest: [u8; 32]) -> io::Result<Self> {
        if !path.is_absolute() || path.as_os_str().as_bytes().contains(&0) {
            return Err(invalid("requires absolute artifact path without NUL"));
        }
        // One no-follow source open. All metadata and content are from this
        // held object; replacing the pathname after admission cannot change it.
        let mut source = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        let before = source.metadata()?;
        // SAFETY: geteuid has no pointers or transferred resources.
        let uid = unsafe { libc::geteuid() };
        if !before.is_file()
            || before.uid() != uid
            || before.nlink() != 1
            || before.mode() & 0o7022 != 0
            || before.mode() & 0o100 == 0
            || before.len() > MAX_EXECUTABLE_BYTES
        {
            return Err(invalid("artifact owner/link/mode/type/size policy"));
        }
        let mut bytes = Vec::new();
        (&mut source)
            .take(MAX_EXECUTABLE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_EXECUTABLE_BYTES || bytes.len() as u64 != before.len() {
            return Err(invalid("artifact size changed during admission"));
        }
        let after = source.metadata()?;
        if before.dev() != after.dev()
            || before.ino() != after.ino()
            || before.len() != after.len()
            || before.mode() != after.mode()
            || before.uid() != after.uid()
            || before.nlink() != after.nlink()
            || before.mtime() != after.mtime()
            || before.mtime_nsec() != after.mtime_nsec()
            || before.ctime() != after.ctime()
            || before.ctime_nsec() != after.ctime_nsec()
        {
            return Err(invalid("artifact metadata changed during admission"));
        }
        validate_elf(&bytes)?;
        if digest(&bytes) != expected_digest {
            return Err(invalid("artifact digest mismatch"));
        }
        // SAFETY: name is NUL terminated, flags are the documented Linux ABI.
        let fd = unsafe {
            libc::memfd_create(
                c"morrow-linux-executable-v1".as_ptr(),
                libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING | libc::MFD_EXEC,
            )
        };
        if fd < 0 {
            return Err(kernel_error("memfd_create(MFD_EXEC)"));
        }
        // SAFETY: successful memfd_create returned a fresh owned descriptor.
        let mut file = above_stdio(unsafe { File::from_raw_fd(fd) })?;
        let fd = file.as_raw_fd();
        file.write_all(&bytes)?;
        // SAFETY: fd remains owned; only this fresh anonymous artifact is changed.
        if unsafe { libc::fchmod(fd, 0o500) } < 0 {
            return Err(kernel_error("fchmod executable memfd"));
        }
        // SAFETY: fd is held, and the integer mask is a documented fcntl command.
        if unsafe { libc::fcntl(fd, libc::F_ADD_SEALS, REQUIRED_SEALS) } < 0 {
            return Err(kernel_error("F_ADD_SEALS executable memfd"));
        }
        let result = Self {
            file,
            digest: expected_digest,
            source: SourceIdentity {
                device: before.dev(),
                inode: before.ino(),
                length: before.len(),
            },
        };
        result.verify()?;
        Ok(result)
    }

    pub fn artifact_digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn source_identity(&self) -> SourceIdentity {
        self.source
    }
    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }

    /// Verify the held memfd itself, never its diagnostic /proc pathname.
    pub fn verify(&self) -> io::Result<()> {
        let metadata = self.verify_object()?;
        let mut hasher = Sha256::new();
        let mut offset = 0;
        let mut buffer = [0u8; 16 * 1024];
        while offset < metadata.len() {
            let count = self.file.read_at(&mut buffer, offset)?;
            if count == 0 {
                return Err(invalid("short sealed executable read"));
            }
            hasher.update(&buffer[..count]);
            offset += count as u64;
        }
        let actual: [u8; 32] = hasher.finalize().into();
        if actual != self.digest {
            return Err(invalid("sealed executable digest mismatch"));
        }
        Ok(())
    }
    /// Current identity/policy of the same held immutable memfd. Only a typed
    /// prepared launch may use this without rehashing: WRITE/GROW/SHRINK/SEAL
    /// make the already-verified bytes and size immutable for this object's life.
    pub(crate) fn verify_object(&self) -> io::Result<std::fs::Metadata> {
        let fd = self.file.as_raw_fd();
        let metadata = self.file.metadata()?;
        // SAFETY: these calls have no pointers and the descriptor stays held.
        let (flags, seals, uid) = unsafe {
            (
                libc::fcntl(fd, libc::F_GETFD),
                libc::fcntl(fd, libc::F_GET_SEALS),
                libc::geteuid(),
            )
        };
        if flags < 0 || seals < 0 {
            return Err(kernel_error("verify executable descriptor"));
        }
        if flags & libc::FD_CLOEXEC == 0
            || seals & REQUIRED_SEALS != REQUIRED_SEALS
            || !metadata.is_file()
            || metadata.uid() != uid
            || metadata.nlink() != 0
            || metadata.mode() & 0o7777 != 0o500
            || metadata.len() != self.source.length
        {
            return Err(invalid("sealed executable object policy"));
        }
        Ok(metadata)
    }
}

fn validate_elf(bytes: &[u8]) -> io::Result<()> {
    // ELF64, little endian, current ABI, host machine; only ordinary executable
    // and position-independent executables. Scripts would require leaking the
    // memfd to their interpreter, so they are deliberately refused.
    let machine = if cfg!(target_arch = "x86_64") {
        62u16
    } else if cfg!(target_arch = "aarch64") {
        183u16
    } else {
        return Err(invalid("unsupported Linux supervisor architecture"));
    };
    if bytes.len() < 64
        || &bytes[..4] != b"\x7fELF"
        || bytes[4] != 2
        || bytes[5] != 1
        || bytes[6] != 1
        || !matches!(u16::from_le_bytes([bytes[16], bytes[17]]), 2 | 3)
        || u16::from_le_bytes([bytes[18], bytes[19]]) != machine
    {
        return Err(invalid(
            "only a host-architecture ELF64 executable is admitted",
        ));
    }
    Ok(())
}
