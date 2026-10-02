//! Owned SQLite VFS foundation using only the public sqlite3_vfs/io_methods ABI.
//!
//! This is deliberately a single-main-database, rollback-journal, lifetime
//! exclusive Linux OFD-lock profile. It is not the enabled private Store, audit
//! owner, snapshot/backup/recovery API, or malicious-same-UID isolation. No
//! default-VFS file callback, private unixFile layout, or /proc fd inference is
//! used. An owned fd is attested before pMethods is installed or any SQLite IO.
//!
//! Public methods bracket even cache-only SQL with metadata checks. The typed
//! transaction checks again immediately before commit and withholds results on
//! post-commit failure. No public raw Connection, statement, or handle escapes.
//! WAL/SHM, mmap, attachments, temporary files, extensions and other journal
//! profiles are unsupported. Production Store entry points remain unavailable.
use super::{
    PrivateDirectory, SqlitePermissions, check_sidecars, invalid, name, openat, private_file,
};
use crate::{Error, Result};
use rusqlite::{Connection, OpenFlags, Params, Row, TransactionBehavior, ffi};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    fs::File,
    io,
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, MetadataExt},
    },
    path::{Path, PathBuf},
    ptr,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_DESCRIPTORS: usize = 64;
const MAX_PATH: usize = 4096;
static DESCRIPTORS: AtomicUsize = AtomicUsize::new(0);
static RETIRED: OnceLock<Mutex<Vec<(File, DescriptorSlot)>>> = OnceLock::new();
static NEXT_VFS: AtomicU64 = AtomicU64::new(1);
#[cfg(test)]
static ACTUAL_OPENS: AtomicUsize = AtomicUsize::new(0);

// SQLite's ordinary Unix VFS uses process-owned POSIX record locks. Closing
// ANY normal fd on the same inode drops those locks, including rejected opens.
// Never close one of our normal inode descriptors until a whole-file OFD write
// lock proves absence of conflicting POSIX locks. A failed proof retains the
// fd in a bounded pool; opens reserve capacity BEFORE the open syscall. Reap
// only when the kernel can establish the same proof. Unsupported OFD kernels
// fail closed and may retain bounded descriptors; there is no POSIX fallback.
fn exclusive_ofd(file: &File) -> io::Result<()> {
    let lock = libc::flock {
        l_type: libc::F_WRLCK as _,
        l_whence: libc::SEEK_SET as _,
        l_start: 0,
        l_len: 0,
        l_pid: 0,
    };
    loop {
        let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_OFD_SETLK, &lock) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
fn reap_retired() {
    let mut retired = RETIRED
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut index = 0;
    while index < retired.len() {
        if exclusive_ofd(&retired[index].0).is_ok() {
            drop(retired.swap_remove(index));
        } else {
            index += 1;
        }
    }
}
struct DescriptorSlot;
impl DescriptorSlot {
    fn reserve() -> io::Result<Self> {
        reap_retired();
        DESCRIPTORS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_DESCRIPTORS).then_some(count + 1)
            })
            .map_err(|_| io::Error::from_raw_os_error(libc::EBUSY))?;
        Ok(Self)
    }
}
impl Drop for DescriptorSlot {
    fn drop(&mut self) {
        DESCRIPTORS.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Descriptor {
    file: Option<File>,
    slot: Option<DescriptorSlot>,
}
impl Descriptor {
    fn open(dir: &PrivateDirectory, leaf: &Path, flags: c_int) -> io::Result<Self> {
        let slot = DescriptorSlot::reserve()?;
        #[cfg(test)]
        ACTUAL_OPENS.fetch_add(1, Ordering::SeqCst);
        let file = openat(
            &dir.file,
            &name(leaf)?,
            flags | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            0o600,
        )?;
        Ok(Self {
            file: Some(file),
            slot: Some(slot),
        })
    }
    fn file(&self) -> &File {
        self.file.as_ref().expect("live VFS descriptor")
    }
}
impl Drop for Descriptor {
    fn drop(&mut self) {
        if let (Some(file), Some(slot)) = (self.file.take(), self.slot.take()) {
            if exclusive_ofd(&file).is_ok() {
                drop(file);
                drop(slot);
            } else {
                RETIRED
                    .get_or_init(Default::default)
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((file, slot));
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
}
impl Identity {
    fn of(file: &File) -> io::Result<Self> {
        let metadata = private_file(file)?;
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Main,
    Journal,
}
struct Journal {
    // Only O_PATH pins. Retain until authorized deletion so inode reuse cannot
    // pass an identity comparison after close and before xDelete.
    pin: Option<File>,
    open: bool,
}
struct State {
    permissions: SqlitePermissions,
    main_path: CString,
    journal_path: CString,
    journal_leaf: PathBuf,
    main: Identity,
    main_open: AtomicBool,
    journal: Mutex<Journal>,
    poisoned: AtomicBool,
    internal_control: AtomicBool,
    #[cfg(test)]
    admission_swap: Mutex<Option<(PathBuf, PathBuf)>>,
    #[cfg(test)]
    header_reads: AtomicUsize,
    #[cfg(test)]
    fail_sync: AtomicBool,
    #[cfg(test)]
    fail_rollback: AtomicBool,
}
impl State {
    fn prepare(path: &Path, create: bool) -> io::Result<Arc<Self>> {
        // Reject an unsupported existing sidecar before creating a main file.
        let directory = PrivateDirectory::open(
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
            create,
        )?;
        let leaf = PathBuf::from(path.file_name().ok_or_else(invalid)?);
        check_sidecars(&directory, &leaf)?;
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = leaf.as_os_str().to_os_string();
            sidecar.push(suffix);
            match directory.inspect(Path::new(&sidecar)) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                _ => return Err(invalid()),
            }
        }
        let pin = match directory.inspect(&leaf) {
            Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                // New main creation participates in the same bounded, safe
                // descriptor-retirement rule, rather than open-and-close.
                let actual = Descriptor::open(
                    &directory,
                    &leaf,
                    libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                )?;
                let pin = directory.inspect(&leaf)?;
                if Identity::of(actual.file())? != Identity::of(&pin)? {
                    return Err(invalid());
                }
                drop(actual);
                pin
            }
            value => value?,
        };
        let permissions = SqlitePermissions {
            dir: directory,
            leaf,
            pin,
        };
        permissions.verify()?;
        let full = permissions.dir.path().join(&permissions.leaf);
        use std::os::unix::ffi::OsStrExt;
        let main_path = CString::new(full.as_os_str().as_bytes()).map_err(|_| invalid())?;
        if main_path.as_bytes().len() + 12 >= MAX_PATH {
            return Err(invalid());
        }
        let mut journal_leaf = permissions.leaf.as_os_str().to_os_string();
        journal_leaf.push("-journal");
        let journal_leaf = PathBuf::from(journal_leaf);
        let journal_path = CString::new(
            permissions
                .dir
                .path()
                .join(&journal_leaf)
                .as_os_str()
                .as_bytes(),
        )
        .map_err(|_| invalid())?;
        let pin = match permissions.dir.inspect(&journal_leaf) {
            Ok(pin) => Some(pin),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let state = Arc::new(Self {
            main: Identity::of(&permissions.pin)?,
            permissions,
            main_path,
            journal_path,
            journal_leaf,
            main_open: AtomicBool::new(false),
            journal: Mutex::new(Journal { pin, open: false }),
            poisoned: AtomicBool::new(false),
            internal_control: AtomicBool::new(false),
            #[cfg(test)]
            admission_swap: Mutex::new(None),
            #[cfg(test)]
            header_reads: AtomicUsize::new(0),
            #[cfg(test)]
            fail_sync: AtomicBool::new(false),
            #[cfg(test)]
            fail_rollback: AtomicBool::new(false),
        });
        state.verify()?;
        Ok(state)
    }
    fn reject(&self) -> io::Error {
        self.poisoned.store(true, Ordering::Release);
        invalid()
    }
    fn verify(&self) -> io::Result<()> {
        if self.poisoned.load(Ordering::Acquire) {
            return Err(invalid());
        }
        self.verify_inner().map_err(|_| self.reject())
    }
    fn verify_inner(&self) -> io::Result<()> {
        self.permissions.verify()?;
        // WAL/SHM are rejected even when their metadata is otherwise private.
        // We cannot downgrade an existing WAL database to rollback mode.
        for suffix in ["-wal", "-shm"] {
            let mut leaf = self.permissions.leaf.as_os_str().to_os_string();
            leaf.push(suffix);
            match self.permissions.dir.inspect(Path::new(&leaf)) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                _ => return Err(invalid()),
            }
        }
        let journal = self.journal.lock().map_err(|_| invalid())?;
        match (
            journal.pin.as_ref(),
            self.permissions.dir.inspect(&self.journal_leaf),
        ) {
            (None, Err(error)) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            (Some(pin), Ok(current)) if Identity::of(pin)? == Identity::of(&current)? => Ok(()),
            _ => Err(invalid()),
        }
    }
    fn role(&self, path: &CStr) -> io::Result<Role> {
        if path == self.main_path.as_c_str() {
            Ok(Role::Main)
        } else if path == self.journal_path.as_c_str() {
            Ok(Role::Journal)
        } else {
            Err(invalid())
        }
    }
    fn leaf(&self, role: Role) -> &Path {
        match role {
            Role::Main => &self.permissions.leaf,
            Role::Journal => &self.journal_leaf,
        }
    }
}

struct OpenFile {
    descriptor: Descriptor,
    state: Arc<State>,
    role: Role,
    identity: Identity,
    writable: bool,
    lock: c_int,
}
impl OpenFile {
    fn verify(&self) -> io::Result<()> {
        self.state.verify()?;
        let actual = Identity::of(self.descriptor.file()).map_err(|_| self.state.reject())?;
        let current = self
            .state
            .permissions
            .dir
            .inspect(self.state.leaf(self.role))
            .map_err(|_| self.state.reject())?;
        if actual != self.identity || Identity::of(&current)? != self.identity {
            return Err(self.state.reject());
        }
        Ok(())
    }
}
#[repr(C)]
struct VfsFile {
    base: ffi::sqlite3_file,
    owned: *mut OpenFile,
}
// SQLite allocates szOsFile bytes with its standard pointer alignment. The
// repr(C) prefix is the public sqlite3_file, not an inferred Unix-VFS layout.
unsafe fn opened<'a>(file: *mut ffi::sqlite3_file) -> &'a mut OpenFile {
    unsafe { &mut *(*file.cast::<VfsFile>()).owned }
}
unsafe fn state<'a>(vfs: *mut ffi::sqlite3_vfs) -> &'a State {
    unsafe { &*(*vfs).pAppData.cast::<State>() }
}
unsafe fn clone_state(vfs: *mut ffi::sqlite3_vfs) -> Arc<State> {
    let pointer = unsafe { (*vfs).pAppData.cast::<State>() };
    unsafe {
        Arc::increment_strong_count(pointer);
        Arc::from_raw(pointer)
    }
}
fn callback(operation: impl FnOnce() -> c_int) -> c_int {
    // Never unwind across the C ABI. Ordinary guard failures separately poison
    // their connection; a panic is an IO failure, never successful admission.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).unwrap_or(ffi::SQLITE_IOERR)
}
fn io_code(error: io::Error, fallback: c_int) -> c_int {
    match error.raw_os_error() {
        Some(libc::EACCES | libc::EAGAIN | libc::EBUSY) => ffi::SQLITE_BUSY,
        Some(libc::ENOSPC | libc::EDQUOT) => ffi::SQLITE_FULL,
        _ => fallback,
    }
}

unsafe extern "C" fn x_open(
    vfs: *mut ffi::sqlite3_vfs,
    path: *const c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out: *mut c_int,
) -> c_int {
    // SQLite requires pMethods to be NULL on a failed unopened object.
    unsafe {
        ptr::write(
            file.cast::<VfsFile>(),
            VfsFile {
                base: ffi::sqlite3_file {
                    pMethods: ptr::null(),
                },
                owned: ptr::null_mut(),
            },
        );
    }
    callback(|| {
        if path.is_null()
            || flags
                & (ffi::SQLITE_OPEN_DELETEONCLOSE | ffi::SQLITE_OPEN_URI | ffi::SQLITE_OPEN_MEMORY)
                != 0
        {
            return ffi::SQLITE_CANTOPEN;
        }
        let state = unsafe { clone_state(vfs) };
        let role = match state.role(unsafe { CStr::from_ptr(path) }) {
            Ok(role) => role,
            Err(_) => return ffi::SQLITE_CANTOPEN,
        };
        let object_flags = flags
            & (ffi::SQLITE_OPEN_MAIN_DB
                | ffi::SQLITE_OPEN_MAIN_JOURNAL
                | ffi::SQLITE_OPEN_TEMP_DB
                | ffi::SQLITE_OPEN_TEMP_JOURNAL
                | ffi::SQLITE_OPEN_TRANSIENT_DB
                | ffi::SQLITE_OPEN_SUBJOURNAL
                | ffi::SQLITE_OPEN_SUPER_JOURNAL
                | ffi::SQLITE_OPEN_WAL);
        if object_flags
            != match role {
                Role::Main => ffi::SQLITE_OPEN_MAIN_DB,
                Role::Journal => ffi::SQLITE_OPEN_MAIN_JOURNAL,
            }
        {
            return ffi::SQLITE_CANTOPEN;
        }
        if state.verify().is_err() {
            return ffi::SQLITE_IOERR;
        }
        if role == Role::Main
            && (flags & ffi::SQLITE_OPEN_READWRITE == 0 || state.main_open.load(Ordering::Acquire))
        {
            return ffi::SQLITE_CANTOPEN;
        }
        let result = (|| -> io::Result<OpenFile> {
            // Every actual descriptor is O_RDWR, including SQLite read-only
            // journal playback. xWrite still honors SQLite's readonly flag.
            // This avoids EBADF retirement of an underlying O_RDONLY fd.
            let mut open_flags = libc::O_RDWR;
            if role == Role::Journal {
                let journal = state.journal.lock().map_err(|_| invalid())?;
                if journal.open {
                    return Err(invalid());
                }
                if journal.pin.is_none() {
                    if flags & ffi::SQLITE_OPEN_CREATE == 0 {
                        return Err(io::Error::from_raw_os_error(libc::ENOENT));
                    }
                    open_flags |= libc::O_CREAT | libc::O_EXCL;
                } else if flags & ffi::SQLITE_OPEN_EXCLUSIVE != 0 {
                    return Err(io::Error::from_raw_os_error(libc::EEXIST));
                }
            }
            // Unit-only deterministic A->B->A admission race: the pathname is
            // restored before fstat/anchored attestation, but the real fd is B.
            // No hook exists in production or the public API.
            #[cfg(test)]
            let swap = state.admission_swap.lock().map_err(|_| invalid())?.take();
            #[cfg(test)]
            if let Some((candidate, displaced)) = &swap {
                let main = state.permissions.dir.path().join(&state.permissions.leaf);
                std::fs::rename(&main, displaced)?;
                std::fs::rename(candidate, &main)?;
            }
            let descriptor_result =
                Descriptor::open(&state.permissions.dir, state.leaf(role), open_flags);
            #[cfg(test)]
            if let Some((candidate, displaced)) = &swap {
                let main = state.permissions.dir.path().join(&state.permissions.leaf);
                std::fs::rename(&main, candidate)?;
                std::fs::rename(displaced, &main)?;
            }
            let descriptor = descriptor_result?;
            let identity = Identity::of(descriptor.file()).map_err(|_| state.reject())?;
            let pin = state
                .permissions
                .dir
                .inspect(state.leaf(role))
                .map_err(|_| state.reject())?;
            if identity != Identity::of(&pin)?
                || (role == Role::Main && identity != state.main)
                || (role == Role::Journal && identity == state.main)
            {
                return Err(state.reject());
            }
            if role == Role::Main {
                // A whole-file OFD lock conflicts with SQLite's POSIX lock bytes
                // and survives unrelated descriptor closes. No timeout/fallback.
                exclusive_ofd(descriptor.file())?;
                state.verify()?;
                // Refuse a WAL header before SQLite reads it or can downgrade it.
                let mut header = [0; 20];
                #[cfg(test)]
                state.header_reads.fetch_add(1, Ordering::SeqCst);
                let count = descriptor.file().read_at(&mut header, 0)?;
                if count >= 20
                    && &header[..16] == b"SQLite format 3\0"
                    && (header[18] != 1 || header[19] != 1)
                {
                    return Err(state.reject());
                }
            } else {
                let mut journal = state.journal.lock().map_err(|_| invalid())?;
                if let Some(expected) = &journal.pin {
                    if Identity::of(expected)? != identity {
                        return Err(state.reject());
                    }
                } else {
                    journal.pin = Some(pin);
                }
                journal.open = true;
            }
            let owned = OpenFile {
                descriptor,
                state: state.clone(),
                role,
                identity,
                writable: flags & ffi::SQLITE_OPEN_READWRITE != 0,
                lock: ffi::SQLITE_LOCK_NONE,
            };
            owned.verify()?;
            if role == Role::Main {
                state.main_open.store(true, Ordering::Release);
            }
            Ok(owned)
        })();
        match result {
            Ok(owned) => {
                unsafe {
                    (*file.cast::<VfsFile>()).owned = Box::into_raw(Box::new(owned));
                    (*file).pMethods = &IO_METHODS;
                    if !out.is_null() {
                        *out = flags;
                    }
                }
                ffi::SQLITE_OK
            }
            Err(error) => io_code(error, ffi::SQLITE_CANTOPEN),
        }
    })
}
unsafe extern "C" fn x_close(file: *mut ffi::sqlite3_file) -> c_int {
    callback(|| {
        let pointer = unsafe { (*file.cast::<VfsFile>()).owned };
        unsafe {
            (*file).pMethods = ptr::null();
            (*file.cast::<VfsFile>()).owned = ptr::null_mut();
        }
        if pointer.is_null() {
            return ffi::SQLITE_OK;
        }
        let owned = unsafe { Box::from_raw(pointer) };
        let safe = owned.verify().is_ok();
        match owned.role {
            Role::Main => owned.state.main_open.store(false, Ordering::Release),
            Role::Journal => {
                if let Ok(mut journal) = owned.state.journal.lock() {
                    journal.open = false;
                }
            }
        }
        drop(owned);
        if safe {
            ffi::SQLITE_OK
        } else {
            ffi::SQLITE_IOERR_CLOSE
        }
    })
}
unsafe extern "C" fn x_read(
    file: *mut ffi::sqlite3_file,
    buffer: *mut c_void,
    amount: c_int,
    offset: i64,
) -> c_int {
    callback(|| {
        if amount < 0 || offset < 0 || buffer.is_null() {
            return ffi::SQLITE_IOERR_READ;
        }
        let output =
            unsafe { std::slice::from_raw_parts_mut(buffer.cast::<u8>(), amount as usize) };
        output.fill(0); // Also zero on rejected reads; short-read zero fill is ABI-required.
        let owned = unsafe { opened(file) };
        if owned.verify().is_err() {
            return ffi::SQLITE_IOERR_READ;
        }
        let mut count = 0;
        while count < output.len() {
            match owned
                .descriptor
                .file()
                .read_at(&mut output[count..], offset as u64 + count as u64)
            {
                Ok(0) => break,
                Ok(bytes) => count += bytes,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    output.fill(0);
                    return io_code(error, ffi::SQLITE_IOERR_READ);
                }
            }
        }
        if owned.verify().is_err() {
            output.fill(0);
            return ffi::SQLITE_IOERR_READ;
        }
        if count == output.len() {
            ffi::SQLITE_OK
        } else {
            ffi::SQLITE_IOERR_SHORT_READ
        }
    })
}
unsafe extern "C" fn x_write(
    file: *mut ffi::sqlite3_file,
    buffer: *const c_void,
    amount: c_int,
    offset: i64,
) -> c_int {
    callback(|| {
        if amount < 0 || offset < 0 || buffer.is_null() {
            return ffi::SQLITE_IOERR_WRITE;
        }
        let owned = unsafe { opened(file) };
        if !owned.writable {
            return ffi::SQLITE_READONLY;
        }
        if owned.verify().is_err() {
            return ffi::SQLITE_IOERR_WRITE;
        }
        let input = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), amount as usize) };
        let mut count = 0;
        while count < input.len() {
            match owned
                .descriptor
                .file()
                .write_at(&input[count..], offset as u64 + count as u64)
            {
                Ok(0) => return ffi::SQLITE_IOERR_WRITE,
                Ok(bytes) => count += bytes,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return io_code(error, ffi::SQLITE_IOERR_WRITE),
            }
        }
        if owned.verify().is_ok() {
            ffi::SQLITE_OK
        } else {
            ffi::SQLITE_IOERR_WRITE
        }
    })
}
unsafe extern "C" fn x_truncate(file: *mut ffi::sqlite3_file, size: i64) -> c_int {
    callback(|| {
        let owned = unsafe { opened(file) };
        if size < 0 || !owned.writable || owned.verify().is_err() {
            return ffi::SQLITE_IOERR_TRUNCATE;
        }
        match owned.descriptor.file().set_len(size as u64) {
            Ok(()) if owned.verify().is_ok() => ffi::SQLITE_OK,
            Err(error) => io_code(error, ffi::SQLITE_IOERR_TRUNCATE),
            _ => ffi::SQLITE_IOERR_TRUNCATE,
        }
    })
}
unsafe extern "C" fn x_sync(file: *mut ffi::sqlite3_file, _flags: c_int) -> c_int {
    callback(|| {
        let owned = unsafe { opened(file) };
        if owned.verify().is_err() {
            return ffi::SQLITE_IOERR_FSYNC;
        }
        #[cfg(test)]
        if owned.state.fail_sync.swap(false, Ordering::AcqRel) {
            return ffi::SQLITE_IOERR_FSYNC;
        }
        // Always the stronger fsync, including directory publication. No
        // assertion about storage hardware honoring fsync or true power loss.
        if let Err(error) = owned
            .descriptor
            .file()
            .sync_all()
            .and_then(|_| owned.state.permissions.dir.sync())
        {
            return io_code(error, ffi::SQLITE_IOERR_FSYNC);
        }
        if owned.verify().is_ok() {
            ffi::SQLITE_OK
        } else {
            ffi::SQLITE_IOERR_FSYNC
        }
    })
}
unsafe extern "C" fn x_size(file: *mut ffi::sqlite3_file, size: *mut i64) -> c_int {
    callback(|| {
        unsafe {
            *size = 0;
        }
        let owned = unsafe { opened(file) };
        if owned.verify().is_err() {
            return ffi::SQLITE_IOERR_FSTAT;
        }
        match owned
            .descriptor
            .file()
            .metadata()
            .and_then(|m| i64::try_from(m.len()).map_err(|_| invalid()))
        {
            Ok(value) if owned.verify().is_ok() => {
                unsafe {
                    *size = value;
                }
                ffi::SQLITE_OK
            }
            _ => ffi::SQLITE_IOERR_FSTAT,
        }
    })
}
unsafe extern "C" fn x_lock(file: *mut ffi::sqlite3_file, lock: c_int) -> c_int {
    callback(|| {
        let owned = unsafe { opened(file) };
        if owned.role != Role::Main
            || owned.verify().is_err()
            || !(ffi::SQLITE_LOCK_SHARED..=ffi::SQLITE_LOCK_EXCLUSIVE).contains(&lock)
        {
            return ffi::SQLITE_IOERR_LOCK;
        }
        owned.lock = owned.lock.max(lock);
        ffi::SQLITE_OK // Actual whole-file OFD exclusive lock never downgrades.
    })
}
unsafe extern "C" fn x_unlock(file: *mut ffi::sqlite3_file, lock: c_int) -> c_int {
    callback(|| {
        let owned = unsafe { opened(file) };
        if owned.verify().is_err()
            || !matches!(lock, ffi::SQLITE_LOCK_NONE | ffi::SQLITE_LOCK_SHARED)
        {
            return ffi::SQLITE_IOERR_UNLOCK;
        }
        owned.lock = owned.lock.min(lock);
        ffi::SQLITE_OK
    })
}
unsafe extern "C" fn x_reserved(file: *mut ffi::sqlite3_file, output: *mut c_int) -> c_int {
    callback(|| {
        unsafe {
            *output = 0;
        }
        let owned = unsafe { opened(file) };
        if owned.verify().is_err() {
            return ffi::SQLITE_IOERR_CHECKRESERVEDLOCK;
        }
        // Logical lock, not the lifetime exclusion lease: on SHARED hot-journal
        // detection must see false, allowing SQLite to perform real playback.
        // The actual OFD exclusive lease excludes all other POSIX lock holders.
        unsafe {
            *output = c_int::from(owned.lock >= ffi::SQLITE_LOCK_RESERVED);
        }
        ffi::SQLITE_OK
    })
}
unsafe extern "C" fn x_control(
    file: *mut ffi::sqlite3_file,
    operation: c_int,
    argument: *mut c_void,
) -> c_int {
    callback(|| {
        let owned = unsafe { opened(file) };
        if owned.verify().is_err() {
            return ffi::SQLITE_IOERR;
        }
        if operation == ffi::SQLITE_FCNTL_LOCKSTATE {
            unsafe {
                *argument.cast::<c_int>() = owned.lock;
            }
            ffi::SQLITE_OK
        } else {
            // Unsupported public controls, including FILE_POINTER/HAS_MOVED,
            // are never used as descriptor identity proof.
            ffi::SQLITE_NOTFOUND
        }
    })
}
unsafe extern "C" fn x_sector(_file: *mut ffi::sqlite3_file) -> c_int {
    4096
}
unsafe extern "C" fn x_characteristics(_file: *mut ffi::sqlite3_file) -> c_int {
    0
}
static IO_METHODS: ffi::sqlite3_io_methods = ffi::sqlite3_io_methods {
    iVersion: 1,
    xClose: Some(x_close),
    xRead: Some(x_read),
    xWrite: Some(x_write),
    xTruncate: Some(x_truncate),
    xSync: Some(x_sync),
    xFileSize: Some(x_size),
    xLock: Some(x_lock),
    xUnlock: Some(x_unlock),
    xCheckReservedLock: Some(x_reserved),
    xFileControl: Some(x_control),
    xSectorSize: Some(x_sector),
    xDeviceCharacteristics: Some(x_characteristics),
    xShmMap: None,
    xShmLock: None,
    xShmBarrier: None,
    xShmUnmap: None,
    xFetch: None,
    xUnfetch: None,
};

unsafe extern "C" fn x_delete(
    vfs: *mut ffi::sqlite3_vfs,
    path: *const c_char,
    sync: c_int,
) -> c_int {
    callback(|| {
        if path.is_null() {
            return ffi::SQLITE_IOERR_DELETE;
        }
        let state = unsafe { state(vfs) };
        if !matches!(
            state.role(unsafe { CStr::from_ptr(path) }),
            Ok(Role::Journal)
        ) {
            return ffi::SQLITE_IOERR_DELETE;
        }
        if state.verify().is_err() {
            return ffi::SQLITE_IOERR_DELETE;
        }
        let mut journal = match state.journal.lock() {
            Ok(value) => value,
            Err(_) => return ffi::SQLITE_IOERR_DELETE,
        };
        if journal.open {
            return ffi::SQLITE_IOERR_DELETE;
        }
        if journal.pin.is_none() {
            return ffi::SQLITE_OK;
        }
        let leaf = match name(&state.journal_leaf) {
            Ok(value) => value,
            Err(_) => return ffi::SQLITE_IOERR_DELETE,
        };
        // Every component and the pinned current journal were verified above.
        // POSIX unlinkat has no conditional-inode primitive. This does not
        // promise isolation against an actively malicious same-UID path race.
        let status =
            unsafe { libc::unlinkat(state.permissions.dir.file.as_raw_fd(), leaf.as_ptr(), 0) };
        if status != 0 {
            state.reject();
            return ffi::SQLITE_IOERR_DELETE;
        }
        journal.pin = None;
        drop(journal);
        if state.verify().is_err() || (sync != 0 && state.permissions.dir.sync().is_err()) {
            state.reject();
            return ffi::SQLITE_IOERR_DELETE;
        }
        ffi::SQLITE_OK
    })
}
unsafe extern "C" fn x_access(
    vfs: *mut ffi::sqlite3_vfs,
    path: *const c_char,
    flags: c_int,
    output: *mut c_int,
) -> c_int {
    callback(|| {
        unsafe {
            *output = 0;
        }
        if path.is_null()
            || !matches!(
                flags,
                ffi::SQLITE_ACCESS_EXISTS | ffi::SQLITE_ACCESS_READ | ffi::SQLITE_ACCESS_READWRITE
            )
        {
            return ffi::SQLITE_IOERR_ACCESS;
        }
        let state = unsafe { state(vfs) };
        if state.verify().is_err() {
            return ffi::SQLITE_IOERR_ACCESS;
        }
        let path = unsafe { CStr::from_ptr(path) };
        // SQLite probes the exact absent WAL pathname during rollback bootstrap.
        // verify() has proved WAL and SHM absent. Reporting that absence is not
        // admitting their xOpen, which remains unsupported.
        if let Some(suffix) = path.to_bytes().strip_prefix(state.main_path.as_bytes()) {
            if matches!(suffix, b"-wal" | b"-shm") {
                return ffi::SQLITE_OK;
            }
        }
        let role = match state.role(path) {
            Ok(role) => role,
            Err(_) => return ffi::SQLITE_IOERR_ACCESS,
        };
        match state.permissions.dir.inspect(state.leaf(role)) {
            Ok(file) => {
                // Match the default Unix VFS: a zero-byte journal is not hot.
                unsafe {
                    *output = c_int::from(
                        flags != ffi::SQLITE_ACCESS_EXISTS
                            || file.metadata().map(|m| m.len() > 0).unwrap_or(false),
                    );
                }
                ffi::SQLITE_OK
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => ffi::SQLITE_OK,
            Err(_) => {
                state.reject();
                ffi::SQLITE_IOERR_ACCESS
            }
        }
    })
}
unsafe extern "C" fn x_full_path(
    vfs: *mut ffi::sqlite3_vfs,
    path: *const c_char,
    amount: c_int,
    output: *mut c_char,
) -> c_int {
    callback(|| {
        if path.is_null() || amount <= 0 {
            return ffi::SQLITE_CANTOPEN;
        }
        let state = unsafe { state(vfs) };
        let path = unsafe { CStr::from_ptr(path) };
        // The wrapper provides this exact absolute no-follow anchored path.
        if !matches!(state.role(path), Ok(Role::Main))
            || path.to_bytes_with_nul().len() > amount as usize
            || state.verify().is_err()
        {
            return ffi::SQLITE_CANTOPEN;
        }
        unsafe {
            ptr::copy_nonoverlapping(path.as_ptr(), output, path.to_bytes_with_nul().len());
        }
        ffi::SQLITE_OK
    })
}
unsafe extern "C" fn x_dl_open(_vfs: *mut ffi::sqlite3_vfs, _path: *const c_char) -> *mut c_void {
    ptr::null_mut()
}
unsafe extern "C" fn x_dl_error(_vfs: *mut ffi::sqlite3_vfs, amount: c_int, output: *mut c_char) {
    if amount > 0 {
        unsafe {
            *output = 0;
        }
    }
}
unsafe extern "C" fn x_dl_close(_vfs: *mut ffi::sqlite3_vfs, _handle: *mut c_void) {}
unsafe extern "C" fn x_random(
    _vfs: *mut ffi::sqlite3_vfs,
    amount: c_int,
    output: *mut c_char,
) -> c_int {
    callback(|| {
        if amount <= 0 {
            return 0;
        }
        let bytes = unsafe { std::slice::from_raw_parts_mut(output.cast::<u8>(), amount as usize) };
        bytes.fill(0);
        let result = unsafe { libc::getrandom(bytes.as_mut_ptr().cast(), bytes.len(), 0) };
        if result < 0 { 0 } else { result as c_int }
    })
}
unsafe extern "C" fn x_sleep(_vfs: *mut ffi::sqlite3_vfs, microseconds: c_int) -> c_int {
    callback(|| {
        if microseconds > 0 {
            std::thread::sleep(Duration::from_micros(microseconds as u64));
        }
        microseconds.max(0)
    })
}
unsafe extern "C" fn x_time(_vfs: *mut ffi::sqlite3_vfs, output: *mut f64) -> c_int {
    callback(|| match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(time) => {
            unsafe {
                *output = 2440587.5 + time.as_secs_f64() / 86400.0;
            }
            ffi::SQLITE_OK
        }
        Err(_) => ffi::SQLITE_ERROR,
    })
}
unsafe extern "C" fn x_last_error(
    _vfs: *mut ffi::sqlite3_vfs,
    amount: c_int,
    output: *mut c_char,
) -> c_int {
    if amount > 0 {
        unsafe {
            *output = 0;
        }
    }
    0
}
unsafe extern "C" fn authorizer(
    context: *mut c_void,
    action: c_int,
    first: *const c_char,
    second: *const c_char,
    _db: *const c_char,
    _trigger: *const c_char,
) -> c_int {
    callback(|| {
        if matches!(action, ffi::SQLITE_ATTACH | ffi::SQLITE_DETACH) {
            return ffi::SQLITE_DENY;
        }
        if matches!(action, ffi::SQLITE_TRANSACTION | ffi::SQLITE_SAVEPOINT) {
            let state = unsafe { &*context.cast::<State>() };
            if !state.internal_control.load(Ordering::Acquire) {
                return ffi::SQLITE_DENY;
            }
            #[cfg(test)]
            if action == ffi::SQLITE_TRANSACTION
                && unsafe { CStr::from_ptr(first) }
                    .to_bytes()
                    .eq_ignore_ascii_case(b"ROLLBACK")
                && state.fail_rollback.swap(false, Ordering::AcqRel)
            {
                return ffi::SQLITE_DENY;
            }
        }
        if action == ffi::SQLITE_PRAGMA && !second.is_null() {
            let pragma = unsafe { CStr::from_ptr(first) }.to_bytes();
            // Schema identity metadata is allowed. All profile-changing PRAGMA
            // writes are rejected once the wrapper has fixed its profile.
            if !pragma.eq_ignore_ascii_case(b"application_id")
                && !pragma.eq_ignore_ascii_case(b"user_version")
            {
                return ffi::SQLITE_DENY;
            }
        }
        ffi::SQLITE_OK
    })
}

struct Registration {
    vfs: Box<ffi::sqlite3_vfs>,
    name: CString,
    state: Arc<State>,
}
impl Registration {
    fn new(state: Arc<State>) -> Result<Self> {
        let id = NEXT_VFS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| Error::Limit)?;
        let name = CString::new(format!("morrow-owned-linux-{}-{id}", std::process::id()))
            .map_err(|_| Error::Io)?;
        let mut vfs = Box::new(ffi::sqlite3_vfs {
            iVersion: 1,
            szOsFile: std::mem::size_of::<VfsFile>() as c_int,
            mxPathname: MAX_PATH as c_int,
            pNext: ptr::null_mut(),
            zName: name.as_ptr(),
            pAppData: Arc::as_ptr(&state).cast_mut().cast(),
            xOpen: Some(x_open),
            xDelete: Some(x_delete),
            xAccess: Some(x_access),
            xFullPathname: Some(x_full_path),
            xDlOpen: Some(x_dl_open),
            xDlError: Some(x_dl_error),
            xDlSym: None,
            xDlClose: Some(x_dl_close),
            xRandomness: Some(x_random),
            xSleep: Some(x_sleep),
            xCurrentTime: Some(x_time),
            xGetLastError: Some(x_last_error),
            xCurrentTimeInt64: None,
            xSetSystemCall: None,
            xGetSystemCall: None,
            xNextSystemCall: None,
        });
        if unsafe { ffi::sqlite3_initialize() } != ffi::SQLITE_OK
            || !unsafe { ffi::sqlite3_vfs_find(name.as_ptr()) }.is_null()
            || unsafe { ffi::sqlite3_vfs_register(&mut *vfs, 0) } != ffi::SQLITE_OK
        {
            return Err(Error::Storage);
        }
        Ok(Self { vfs, name, state })
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        // The owning wrapper closes SQLite before unregistering. No public
        // statements/backups/handles can outlive the wrapper's borrowed API.
        unsafe {
            ffi::sqlite3_vfs_unregister(&mut *self.vfs);
        }
        reap_retired();
    }
}
fn sqlite<T>(result: rusqlite::Result<T>) -> Result<T> {
    result.map_err(|error| match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            Error::StorageBusy
        }
        Some(rusqlite::ErrorCode::DiskFull) => Error::StorageFull,
        _ => Error::Storage,
    })
}
fn verify(state: &State) -> Result<()> {
    state
        .verify()
        .map_err(|_| Error::Invalid("unsafe owned Linux SQLite object"))
}
struct InternalControl<'state>(&'state State);
impl<'state> InternalControl<'state> {
    fn begin(state: &'state State) -> Self {
        state.internal_control.store(true, Ordering::Release);
        Self(state)
    }
}
impl Drop for InternalControl<'_> {
    fn drop(&mut self) {
        self.0.internal_control.store(false, Ordering::Release);
    }
}
fn readonly_row<T, P: Params>(
    connection: &Connection,
    text: &str,
    params: P,
    row: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>,
) -> Result<T> {
    let mut statement = sqlite(connection.prepare(text))?;
    if !statement.readonly() {
        return Err(Error::Invalid("owned SQLite query must be readonly"));
    }
    sqlite(statement.query_row(params, row))
}

/// Experimental owned-VFS connection foundation, not protected Store admission.
/// One lifetime-exclusive read/write main database with DELETE rollback journal.
/// All operations and transaction boundaries verify current private metadata.
/// Unsupported kernels or profiles reject without a default-VFS fallback.
pub struct GuardedSqliteConnection {
    connection: Option<Connection>,
    registration: Registration,
}
impl GuardedSqliteConnection {
    /// Opens an anchored private 0600 single-link file beneath a private 0700
    /// directory. Existing unsafe metadata is rejected rather than repaired.
    /// WAL headers/sidecars reject before any SQLite header read or SQL.
    pub fn open(path: &Path, create: bool) -> Result<Self> {
        let state = State::prepare(path, create)
            .map_err(|_| Error::Invalid("unsafe owned Linux SQLite object"))?;
        let registration = Registration::new(state)?;
        let path = Path::new(std::ffi::OsStr::from_bytes(
            registration.state.main_path.as_bytes(),
        ));
        let connection = sqlite(Connection::open_with_flags_and_vfs(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            registration.name.to_str().map_err(|_| Error::Io)?,
        ))?;
        verify(&registration.state)?;
        sqlite(connection.busy_timeout(Duration::ZERO))?;
        sqlite(connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA temp_store=MEMORY; PRAGMA mmap_size=0; PRAGMA trusted_schema=OFF;"))?;
        verify(&registration.state)?;
        let journal: String =
            sqlite(connection.query_row("PRAGMA journal_mode", [], |row| row.get(0)))?;
        if journal != "delete" {
            return Err(Error::Invalid("owned SQLite rollback profile unavailable"));
        }
        let result = unsafe {
            ffi::sqlite3_set_authorizer(
                connection.handle(),
                Some(authorizer),
                Arc::as_ptr(&registration.state).cast_mut().cast(),
            )
        };
        if result != ffi::SQLITE_OK {
            return Err(Error::Storage);
        }
        Ok(Self {
            connection: Some(connection),
            registration,
        })
    }
    /// Executes a single statement inside the typed transaction. Mutations
    /// cannot autocommit before its immediate pre-commit object check.
    pub fn execute<P: Params>(&mut self, statement: &str, params: P) -> Result<usize> {
        self.transaction(|transaction| transaction.execute(statement, params))
    }
    /// Returns a row only after post-read object validation succeeds.
    pub fn query_row<T, P: Params>(
        &mut self,
        statement: &str,
        params: P,
        row: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<T> {
        verify(&self.registration.state)?;
        let result = readonly_row(
            self.connection.as_ref().ok_or(Error::Storage)?,
            statement,
            params,
            row,
        );
        verify(&self.registration.state)?;
        result
    }
    /// Runs a guarded IMMEDIATE transaction with an explicit pre-commit check.
    /// An error/panic requests rollback through SQLite. Unsafe IO may prevent
    /// that rollback; the connection is poisoned and recovery is not claimed.
    /// An uncertain commit or unsafe
    /// post-commit object is CommitUnknown; it is never presented as rollback.
    pub fn transaction<T>(
        &mut self,
        operation: impl FnOnce(&mut GuardedSqliteTransaction<'_>) -> Result<T>,
    ) -> Result<T> {
        let state = self.registration.state.clone();
        verify(&state)?;
        let control = InternalControl::begin(&state);
        let transaction = sqlite(
            self.connection
                .as_mut()
                .ok_or(Error::Storage)?
                .transaction_with_behavior(TransactionBehavior::Immediate),
        )?;
        drop(control);
        let mut guarded = GuardedSqliteTransaction {
            transaction: Some(transaction),
            state,
        };
        let value = operation(&mut guarded)?;
        verify(&guarded.state)?;
        let control_state = guarded.state.clone();
        let control = InternalControl::begin(&control_state);
        let committed = guarded.transaction.take().ok_or(Error::Storage)?.commit();
        drop(control);
        if committed.is_err() {
            guarded.state.reject();
            return Err(Error::CommitUnknown);
        }
        if verify(&guarded.state).is_err() {
            return Err(Error::CommitUnknown);
        }
        Ok(value)
    }
    /// Verifies the still-owned object without reading SQLite headers/pages.
    pub fn verify(&self) -> Result<()> {
        verify(&self.registration.state)
    }
}
use std::os::unix::ffi::OsStrExt;
impl Drop for GuardedSqliteConnection {
    fn drop(&mut self) {
        // Drop SQLite first, including rollback/journal callbacks, while both
        // the registration and Arc-backed callback state remain alive.
        drop(self.connection.take());
    }
}
/// Borrowed guarded transaction. Neither its SQLite connection nor raw handle
/// is public, and no statement/backup can escape the owning operation.
pub struct GuardedSqliteTransaction<'connection> {
    transaction: Option<rusqlite::Transaction<'connection>>,
    state: Arc<State>,
}
impl GuardedSqliteTransaction<'_> {
    /// Brackets a transaction statement with current-object verification.
    pub fn execute<P: Params>(&mut self, statement: &str, params: P) -> Result<usize> {
        verify(&self.state)?;
        let result = sqlite(
            self.transaction
                .as_ref()
                .ok_or(Error::Storage)?
                .execute(statement, params),
        );
        verify(&self.state)?;
        result
    }
    /// Withholds a cached row when the transaction's object became unsafe.
    pub fn query_row<T, P: Params>(
        &mut self,
        statement: &str,
        params: P,
        row: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<T> {
        verify(&self.state)?;
        let result = readonly_row(
            self.transaction.as_ref().ok_or(Error::Storage)?,
            statement,
            params,
            row,
        );
        verify(&self.state)?;
        result
    }
}
impl Drop for GuardedSqliteTransaction<'_> {
    fn drop(&mut self) {
        if let Some(transaction) = self.transaction.take() {
            let _control = InternalControl::begin(&self.state);
            if transaction.rollback().is_err() {
                self.state.reject();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    // Pool counters are process-wide. These two pool/ABI tests run under this
    // lock, independently of metadata-only library tests.
    static UNIT_LOCK: Mutex<()> = Mutex::new(());
    fn directory() -> tempfile::TempDir {
        tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
    }
    #[test]
    fn actual_xopen_fd_rejects_path_aba_before_header_read_or_methods_admission() {
        let _lock = UNIT_LOCK.lock().unwrap();
        let dir = directory();
        let path = dir.path().join("main.db");
        let state = State::prepare(&path, true).unwrap();
        let candidate = dir.path().join("candidate.db");
        drop(
            PrivateDirectory::open(dir.path(), false)
                .unwrap()
                .create_new(Path::new("candidate.db"))
                .unwrap(),
        );
        *state.admission_swap.lock().unwrap() =
            Some((candidate.clone(), dir.path().join("displaced.db")));
        let mut registration = Registration::new(state.clone()).unwrap();
        let mut file = VfsFile {
            base: ffi::sqlite3_file {
                pMethods: ptr::null(),
            },
            owned: ptr::null_mut(),
        };
        let mut flags = 0;
        let result = unsafe {
            x_open(
                &mut *registration.vfs,
                state.main_path.as_ptr(),
                &mut file.base,
                ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_MAIN_DB,
                &mut flags,
            )
        };
        assert_ne!(result, ffi::SQLITE_OK);
        assert!(
            file.base.pMethods.is_null(),
            "no IO methods may be admitted for the wrong opened inode"
        );
        assert!(file.owned.is_null());
        assert_eq!(
            state.header_reads.load(Ordering::SeqCst),
            0,
            "wrong fd must reject before even our header preflight"
        );
        assert!(state.poisoned.load(Ordering::Acquire));
        // The pathname is again A; only an actual opened-fd identity proof
        // distinguishes B from the apparently intact pre/post pathname pin.
        assert_eq!(
            Identity::of(
                &state
                    .permissions
                    .dir
                    .inspect(&state.permissions.leaf)
                    .unwrap()
            )
            .unwrap(),
            state.main
        );
        assert!(candidate.exists());
    }
    #[test]
    fn rejected_descriptor_retirement_is_bounded_before_open_and_reaps_after_unlock() {
        let _lock = UNIT_LOCK.lock().unwrap();
        reap_retired();
        assert_eq!(DESCRIPTORS.load(Ordering::SeqCst), 0);
        let dir = directory();
        let path = dir.path().join("legacy.db");
        drop(SqlitePermissions::prepare(&path, true).unwrap());
        let legacy = Connection::open(&path).unwrap();
        legacy
            .execute_batch("CREATE TABLE t(x); BEGIN IMMEDIATE; INSERT INTO t VALUES(7)")
            .unwrap();
        let before = ACTUAL_OPENS.load(Ordering::SeqCst);
        for _ in 0..MAX_DESCRIPTORS {
            assert!(matches!(
                GuardedSqliteConnection::open(&path, false),
                Err(Error::StorageBusy)
            ));
        }
        assert_eq!(DESCRIPTORS.load(Ordering::SeqCst), MAX_DESCRIPTORS);
        assert_eq!(
            ACTUAL_OPENS.load(Ordering::SeqCst) - before,
            MAX_DESCRIPTORS
        );
        assert!(matches!(
            GuardedSqliteConnection::open(&path, false),
            Err(Error::StorageBusy)
        ));
        assert_eq!(
            ACTUAL_OPENS.load(Ordering::SeqCst) - before,
            MAX_DESCRIPTORS,
            "capacity must reject BEFORE any extra actual fd open"
        );
        assert_eq!(
            RETIRED.get().unwrap().lock().unwrap().len(),
            MAX_DESCRIPTORS
        );
        legacy.execute_batch("ROLLBACK").unwrap();
        drop(legacy);
        reap_retired();
        assert_eq!(DESCRIPTORS.load(Ordering::SeqCst), 0);
        assert!(RETIRED.get().unwrap().lock().unwrap().is_empty());
        let mut owned = GuardedSqliteConnection::open(&path, false).unwrap();
        assert_eq!(
            owned
                .query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(owned);
        assert_eq!(DESCRIPTORS.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn commit_sync_error_is_unknown_and_stickily_poisoned() {
        let _lock = UNIT_LOCK.lock().unwrap();
        let dir = directory();
        let mut db = GuardedSqliteConnection::open(&dir.path().join("sync.db"), true).unwrap();
        db.execute("CREATE TABLE t(x)", []).unwrap();
        db.registration
            .state
            .fail_sync
            .store(true, Ordering::Release);
        assert_eq!(
            db.execute("INSERT INTO t VALUES(7)", []),
            Err(Error::CommitUnknown)
        );
        assert!(db.verify().is_err());
        assert!(
            db.query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
                .is_err()
        );
        assert!(db.execute("INSERT INTO t VALUES(8)", []).is_err());
    }
    #[test]
    fn failed_explicit_rollback_stickily_poisons_connection() {
        let _lock = UNIT_LOCK.lock().unwrap();
        let dir = directory();
        let mut db = GuardedSqliteConnection::open(&dir.path().join("rollback.db"), true).unwrap();
        db.execute("CREATE TABLE t(x)", []).unwrap();
        let result = db.transaction::<()>(|transaction| {
            transaction.execute("INSERT INTO t VALUES(9)", [])?;
            transaction
                .state
                .fail_rollback
                .store(true, Ordering::Release);
            Err(Error::Integrity)
        });
        assert_eq!(result, Err(Error::Integrity));
        assert!(db.verify().is_err());
        assert!(
            db.query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
                .is_err()
        );
    }
    #[test]
    fn readonly_journal_fd_is_owned_writable_for_safe_retirement_and_short_reads_zero_fill() {
        let _lock = UNIT_LOCK.lock().unwrap();
        let dir = directory();
        let path = dir.path().join("readonly.db");
        drop(SqlitePermissions::prepare(&path, true).unwrap());
        drop(
            PrivateDirectory::open(dir.path(), false)
                .unwrap()
                .create_new(Path::new("readonly.db-journal"))
                .unwrap(),
        );
        let state = State::prepare(&path, false).unwrap();
        let mut registration = Registration::new(state.clone()).unwrap();
        let mut file = VfsFile {
            base: ffi::sqlite3_file {
                pMethods: ptr::null(),
            },
            owned: ptr::null_mut(),
        };
        let mut flags = 0;
        assert_eq!(
            unsafe {
                x_open(
                    &mut *registration.vfs,
                    state.journal_path.as_ptr(),
                    &mut file.base,
                    ffi::SQLITE_OPEN_READONLY | ffi::SQLITE_OPEN_MAIN_JOURNAL,
                    &mut flags,
                )
            },
            ffi::SQLITE_OK
        );
        let actual_flags =
            unsafe { libc::fcntl((*file.owned).descriptor.file().as_raw_fd(), libc::F_GETFL) };
        assert_eq!(actual_flags & libc::O_ACCMODE, libc::O_RDWR);
        assert_ne!(flags & ffi::SQLITE_OPEN_READONLY, 0);
        let mut bytes = [0xffu8; 16];
        assert_eq!(
            unsafe {
                x_read(
                    &mut file.base,
                    bytes.as_mut_ptr().cast(),
                    bytes.len() as c_int,
                    0,
                )
            },
            ffi::SQLITE_IOERR_SHORT_READ
        );
        assert_eq!(bytes, [0; 16]);
        assert_eq!(
            unsafe {
                x_write(
                    &mut file.base,
                    bytes.as_ptr().cast(),
                    bytes.len() as c_int,
                    0,
                )
            },
            ffi::SQLITE_READONLY
        );
        assert_eq!(unsafe { x_close(&mut file.base) }, ffi::SQLITE_OK);
        assert_eq!(
            DESCRIPTORS.load(Ordering::SeqCst),
            0,
            "underlying readonly descriptor must not be stranded with EBADF"
        );
    }
    #[test]
    fn journal_delete_refuses_replaced_private_inode_and_preserves_both_objects() {
        let _lock = UNIT_LOCK.lock().unwrap();
        let dir = directory();
        let path = dir.path().join("delete.db");
        let state = State::prepare(&path, true).unwrap();
        let mut registration = Registration::new(state.clone()).unwrap();
        let mut file = VfsFile {
            base: ffi::sqlite3_file {
                pMethods: ptr::null(),
            },
            owned: ptr::null_mut(),
        };
        assert_eq!(
            unsafe {
                x_open(
                    &mut *registration.vfs,
                    state.journal_path.as_ptr(),
                    &mut file.base,
                    ffi::SQLITE_OPEN_READWRITE
                        | ffi::SQLITE_OPEN_CREATE
                        | ffi::SQLITE_OPEN_MAIN_JOURNAL,
                    ptr::null_mut(),
                )
            },
            ffi::SQLITE_OK
        );
        assert_eq!(unsafe { x_close(&mut file.base) }, ffi::SQLITE_OK);
        let journal = dir.path().join("delete.db-journal");
        let retained = dir.path().join("original-journal");
        std::fs::rename(&journal, &retained).unwrap();
        drop(
            PrivateDirectory::open(dir.path(), false)
                .unwrap()
                .create_new(Path::new("delete.db-journal"))
                .unwrap(),
        );
        assert_eq!(
            unsafe { x_delete(&mut *registration.vfs, state.journal_path.as_ptr(), 1) },
            ffi::SQLITE_IOERR_DELETE
        );
        assert!(journal.exists());
        assert!(retained.exists());
        assert!(state.poisoned.load(Ordering::Acquire));
    }
}
