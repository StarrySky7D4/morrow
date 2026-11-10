//! Single-owner, bounded-lifetime optional TxF transaction.
//!
//! This primitive is not a replacement authorization or a pathname-safety proof.
//! Its caller must reserve every native resource, pin and validate the complete
//! ancestor chain, and commit the original Store claim before any file effect.
//! The broker still rejects replacement while those integrations are incomplete.
use super::{CLOSE_UNCERTAIN, CloseHandle, available, last_error};
use std::{
    ffi::c_void,
    fs::{File, Metadata, OpenOptions},
    io::Write,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle, IntoRawHandle},
    },
    path::{Path, PathBuf},
    sync::{OnceLock, atomic::Ordering},
    time::{Duration, Instant},
};

type Handle = *mut c_void;
type CreateTransaction =
    unsafe extern "system" fn(Handle, Handle, u32, u32, u32, u32, *const u16) -> Handle;
type CompleteTransaction = unsafe extern "system" fn(Handle) -> i32;
type CreateFileTransacted = unsafe extern "system" fn(
    *const u16,
    u32,
    u32,
    Handle,
    u32,
    u32,
    Handle,
    Handle,
    Handle,
    Handle,
) -> Handle;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(name: *const u16, file: Handle, flags: u32) -> Handle;
    fn GetProcAddress(module: Handle, name: *const u8) -> Handle;
    fn GetFileInformationByHandleEx(
        file: Handle,
        class: i32,
        buffer: *mut c_void,
        size: u32,
    ) -> i32;
    fn GetFinalPathNameByHandleW(file: Handle, path: *mut u16, capacity: u32, flags: u32) -> u32;
}

#[repr(C)]
#[derive(Default, Eq, PartialEq)]
struct FileIdentity {
    volume: u64,
    file: [u8; 16],
}

fn identity(file: &File) -> Result<FileIdentity, Failure> {
    let mut value = FileIdentity::default();
    // SAFETY: the borrowed File keeps the handle alive. FileIdInfo (18) writes
    // the initialized, aligned FILE_ID_INFO layout only during this call.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            18,
            std::ptr::addr_of_mut!(value).cast(),
            std::mem::size_of::<FileIdentity>() as u32,
        )
    } == 0
    {
        return Err(Failure::Os(last_error()));
    }
    Ok(value)
}

fn directory(file: &File) -> Result<(), Failure> {
    let metadata = file
        .metadata()
        .map_err(|error| Failure::Os(error.raw_os_error().unwrap_or(0) as u32))?;
    if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
        return Err(Failure::InvalidBounds);
    }
    Ok(())
}

fn volume_root(file: &File) -> Result<PathBuf, Failure> {
    // Only a drive-root handle is passed, so a volume GUID root has fixed size.
    // VOLUME_NAME_GUID rejects remote redirectors and removes drive-letter
    // reassignment from all later transacted pathname resolution.
    let mut buffer = [0_u16; 64];
    // SAFETY: the borrowed live root File and fixed initialized output buffer
    // remain valid for this synchronous query; no pointer is retained.
    let size =
        unsafe { GetFinalPathNameByHandleW(file.as_raw_handle(), buffer.as_mut_ptr(), 64, 1) };
    if size == 0 {
        return Err(Failure::Os(last_error()));
    }
    if size as usize >= buffer.len() {
        return Err(Failure::InvalidBounds);
    }
    let path = String::from_utf16(&buffer[..size as usize]).map_err(|_| Failure::InvalidBounds)?;
    let guid = path
        .strip_prefix(r"\\?\Volume{")
        .and_then(|s| s.strip_suffix("}\\"))
        .ok_or(Failure::InvalidBounds)?;
    if guid.len() != 36
        || !guid.bytes().enumerate().all(|(index, value)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                value == b'-'
            } else {
                value.is_ascii_hexdigit()
            }
        })
    {
        return Err(Failure::InvalidBounds);
    }
    Ok(PathBuf::from(path))
}

struct Api {
    create: CreateTransaction,
    commit: CompleteTransaction,
    rollback: CompleteTransaction,
    open: CreateFileTransacted,
}
static API: OnceLock<Result<Api, Failure>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Failure {
    Unsupported(u32),
    InvalidBounds,
    Inactive,
    Os(u32),
    CloseUnknown,
    ResolutionUnknown,
    RestartRequired,
    Limit,
    SelectionMismatch,
}

fn load_api() -> Result<&'static Api, Failure> {
    API.get_or_init(|| {
        let library = |name: &str| -> Result<Handle, Failure> {
            let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            // SAFETY: NUL-terminated constant library names, NULL file, and
            // LOAD_LIBRARY_SEARCH_SYSTEM32. Never search the working directory.
            let handle = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), 0x800) };
            if handle.is_null() {
                Err(Failure::Unsupported(last_error()))
            } else {
                Ok(handle)
            }
        };
        let ktm = library("KtmW32.dll")?;
        let kernel = library("kernel32.dll")?;
        let symbol = |module, name: &'static [u8]| -> Result<Handle, Failure> {
            // SAFETY: a loaded System32 module and a constant terminated name.
            let address = unsafe { GetProcAddress(module, name.as_ptr()) };
            if address.is_null() {
                Err(Failure::Unsupported(last_error()))
            } else {
                Ok(address)
            }
        };
        // The once-only loader references remain pinned for process lifetime.
        // Function pointers must never outlive an unloaded module. Missing APIs
        // are remembered as Unsupported; loading is not retried on each request.
        // SAFETY: each export is converted to its documented Win32 ABI only.
        unsafe {
            Ok(Api {
                create: std::mem::transmute::<Handle, CreateTransaction>(symbol(
                    ktm,
                    b"CreateTransaction\0",
                )?),
                commit: std::mem::transmute::<Handle, CompleteTransaction>(symbol(
                    ktm,
                    b"CommitTransaction\0",
                )?),
                rollback: std::mem::transmute::<Handle, CompleteTransaction>(symbol(
                    ktm,
                    b"RollbackTransaction\0",
                )?),
                open: std::mem::transmute::<Handle, CreateFileTransacted>(symbol(
                    kernel,
                    b"CreateFileTransactedW\0",
                )?),
            })
        }
    })
    .as_ref()
    .map_err(|error| *error)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Active,
    Committed,
    RolledBack,
    Unknown,
}

/// `charge` is the original owner's reservation, not fresh authority. It is
/// released only after known native closure and a known transaction resolution.
/// No File/raw transaction handle escapes or can be duplicated by this type.
pub(super) struct Transaction<C> {
    api: &'static Api,
    handle: Option<Handle>,
    files: Vec<File>,
    // Non-transacted directory handles outlive file closure and the final
    // commit/rollback. No WRITE/DELETE sharing permits ancestry substitution.
    ancestors: Vec<File>,
    file_acquisition_closed: bool,
    file_capacity: usize,
    charge: Option<C>,
    deadline: Instant,
    state: State,
    close_unknown: bool,
    validation_failure: Option<Failure>,
    // Only the path whose original identity was checked under the writer lock
    // can be replaced. Its complete ancestry remains a caller-owned boundary.
    writer_paths: Vec<PathBuf>,
    replacement_attempted: bool,
}

impl<C> Transaction<C> {
    /// `remaining` must be sampled from the original lease. It is never renewed.
    /// Reserve one transaction plus `file_capacity` files before calling this.
    pub(super) fn begin(
        remaining: Duration,
        file_capacity: usize,
        charge: C,
    ) -> Result<Self, Failure> {
        if !available() {
            return Err(Failure::RestartRequired);
        }
        let millis = u32::try_from(remaining.as_millis()).map_err(|_| Failure::InvalidBounds)?;
        if millis == 0 || millis == u32::MAX || file_capacity == 0 || file_capacity > 128 {
            return Err(Failure::InvalidBounds);
        }
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(u64::from(millis)))
            .ok_or(Failure::InvalidBounds)?;
        let api = load_api()?;
        // SAFETY: no caller-owned pointer, no inherited/external transaction;
        // timeout is finite and rounded DOWN from the original remaining lease.
        let raw = unsafe {
            (api.create)(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                0,
                0,
                millis,
                std::ptr::null(),
            )
        };
        if raw.is_null() || raw as isize == -1 {
            return Err(Failure::Os(last_error()));
        }
        Ok(Self {
            api,
            handle: Some(raw),
            files: Vec::new(),
            ancestors: Vec::new(),
            file_acquisition_closed: false,
            file_capacity,
            charge: Some(charge),
            deadline,
            state: State::Active,
            close_unknown: false,
            validation_failure: None,
            writer_paths: Vec::new(),
            replacement_attempted: false,
        })
    }

    pub(super) fn begin_until(
        deadline: Instant,
        file_capacity: usize,
        charge: C,
    ) -> Result<Self, Failure> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let mut owner = Self::begin(remaining, file_capacity, charge)?;
        owner.deadline = owner.deadline.min(deadline);
        owner.live()?;
        Ok(owner)
    }

    pub(super) fn reservation(&self) -> &C {
        self.charge.as_ref().expect("original native reservation")
    }
    pub(super) fn reservation_mut(&mut self) -> &mut C {
        self.charge.as_mut().expect("original native reservation")
    }

    pub(super) fn selected_metadata(&self) -> Result<Metadata, Failure> {
        self.live()?;
        self.files
            .first()
            .ok_or(Failure::Inactive)?
            .metadata()
            .map_err(|error| Failure::Os(error.raw_os_error().unwrap_or(0) as u32))
    }

    /// The original selection itself is an exclusive transacted open. It is
    /// never closed and reopened to obtain authority for a later transaction.
    pub(super) fn select_local(&mut self, path: &Path) -> Result<Metadata, Failure> {
        let result = (|| {
            let anchored = self.pin_local_ancestry(path)?;
            self.open_selected_writer(&anchored, None, 0)
        })();
        if let Err(failure) = &result {
            self.validation_failure.get_or_insert(*failure);
        }
        result
    }

    fn live(&self) -> Result<(), Failure> {
        if !available() {
            return Err(Failure::RestartRequired);
        }
        if self.state != State::Active || Instant::now() >= self.deadline {
            return Err(Failure::Inactive);
        }
        if let Some(failure) = self.validation_failure {
            return Err(failure);
        }
        Ok(())
    }

    /// Pin a trusted absolute local selection from its volume root down to its
    /// immediate parent, then acquire the original target's writer condition.
    /// The supplied charge must cover root + parents + target + temporary file
    /// and the transaction itself BEFORE this call. No native object escapes.
    /// All path components are checked in their original spelling; directories
    /// are opened relative to their retained parent, without following reparse
    /// points. The target identity is compared only after the writer lock holds.
    pub(super) fn retain_local_writer(
        &mut self,
        path: &Path,
        selected: &File,
    ) -> Result<Metadata, Failure> {
        let result = self.retain_local_writer_once(path, selected);
        if let Err(failure) = &result {
            self.validation_failure.get_or_insert(*failure);
        }
        result
    }

    fn retain_local_writer_once(
        &mut self,
        path: &Path,
        selected: &File,
    ) -> Result<Metadata, Failure> {
        let anchored = self.pin_local_ancestry(path)?;
        self.retain_selected_writer(&anchored, selected)
    }

    fn pin_local_ancestry(&mut self, path: &Path) -> Result<PathBuf, Failure> {
        self.live()?;
        if self.file_acquisition_closed || !self.files.is_empty() || !self.ancestors.is_empty() {
            return Err(Failure::Inactive);
        }
        super::super::validate_path(path).map_err(|_| Failure::InvalidBounds)?;
        let spelling = path.to_str().ok_or(Failure::InvalidBounds)?;
        let prefix = if spelling.starts_with(r"\\?\") { 7 } else { 3 };
        let segments: Vec<_> = spelling[prefix..].split(['\\', '/']).collect();
        if segments
            .len()
            .checked_add(2)
            .is_none_or(|count| count > self.file_capacity)
        {
            return Err(Failure::Limit);
        }
        let os_error =
            |error: std::io::Error| Failure::Os(error.raw_os_error().unwrap_or(0) as u32);
        // TRAVERSE | READ_ATTRIBUTES | SYNCHRONIZE; share READ only; open the
        // root itself. Child opens below are strictly relative to retained FDs.
        let root = OpenOptions::new()
            .access_mode(0x0010_00a0)
            .share_mode(1)
            .custom_flags(0x0220_0000)
            .open(Path::new(&spelling[..prefix]))
            .map_err(os_error)?;
        self.ancestors.push(root);
        directory(&self.ancestors[0])?;
        let mut anchored = volume_root(&self.ancestors[0])?;
        self.live()?;
        for segment in &segments[..segments.len() - 1] {
            let child =
                super::open_child_directory(self.ancestors.last().expect("retained root"), segment)
                    .map_err(os_error)?;
            self.ancestors.push(child);
            directory(self.ancestors.last().expect("registered child"))?;
            anchored.push(segment);
            self.live()?;
        }
        anchored.push(segments.last().ok_or(Failure::InvalidBounds)?);
        Ok(anchored)
    }

    /// Opens without create/truncate and holds the original writer transaction.
    /// The complete ancestry MUST already be pinned by the trusted selection.
    /// `selected` is the original retained read-only selection, never a newly
    /// opened substitute. Compare full identities while the writer lock holds.
    /// Only the identity-checked path is retained for the same transaction's
    /// replacement. It is never accepted as identity or reopened by a new owner.
    /// Any failed acquisition prevents reuse or commit by this same owner.
    pub(super) fn retain_writer(
        &mut self,
        path: &Path,
        selected: &File,
    ) -> Result<Metadata, Failure> {
        let result = self.retain_selected_writer(path, selected);
        if let Err(failure) = &result {
            self.validation_failure.get_or_insert(*failure);
        }
        result
    }

    fn retain_selected_writer(
        &mut self,
        path: &Path,
        selected: &File,
    ) -> Result<Metadata, Failure> {
        self.open_selected_writer(path, Some(selected), 1)
    }

    fn open_selected_writer(
        &mut self,
        path: &Path,
        selected: Option<&File>,
        share: u32,
    ) -> Result<Metadata, Failure> {
        self.live()?;
        if self.file_acquisition_closed {
            return Err(Failure::Inactive);
        }
        if self.files.len() + self.ancestors.len() >= self.file_capacity {
            return Err(Failure::Limit);
        }
        if !path.is_absolute() {
            return Err(Failure::InvalidBounds);
        }
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.is_empty() || wide.len() > 32_760 || wide.contains(&0) {
            return Err(Failure::InvalidBounds);
        }
        wide.push(0);
        // SAFETY: initialized terminated UTF-16 and the original owned live
        // transaction. OPEN_EXISTING + OPEN_REPARSE_POINT never creates/truncates
        // or follows the leaf. FILE_SHARE_READ allows the retained read-only
        // selection; writers and deleters remain excluded.
        let raw = unsafe {
            (self.api.open)(
                wide.as_ptr(),
                0xc011_0000,
                share,
                std::ptr::null_mut(),
                3,
                0x0020_0000,
                std::ptr::null_mut(),
                self.handle.ok_or(Failure::Inactive)?,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if raw.is_null() || raw as isize == -1 {
            return Err(Failure::Os(last_error()));
        }
        // SAFETY: success transferred sole ownership; register BEFORE fallible
        // metadata/liveness checks so every failure follows the same close path.
        self.files.push(unsafe { File::from_raw_handle(raw) });
        let locked = self.files.last().expect("just registered owned file");
        let metadata = locked
            .metadata()
            .map_err(|error| Failure::Os(error.raw_os_error().unwrap_or(0) as u32))?;
        if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
            return Err(Failure::InvalidBounds);
        }
        if let Some(selected) = selected {
            if identity(locked)? != identity(selected)? {
                return Err(Failure::SelectionMismatch);
            }
        }
        self.live()?;
        self.writer_paths.push(path.to_path_buf());
        Ok(metadata)
    }

    /// Stage one bounded payload under the already retained writer condition.
    /// The caller MUST hold the full ancestry, commit its original Store claim,
    /// reserve target plus temporary file capacity, and close the original
    /// read-only selection before calling. This method grants none of those.
    ///
    /// The name is one host-derived component in the selected file's directory;
    /// CREATE_NEW never adopts or overwrites an earlier temporary. All writes
    /// and the rename remain invisible until this same owner commits. A failed
    /// stage is sticky: commit and another stage are both forbidden.
    pub(super) fn stage_replacement(
        &mut self,
        temporary_name: &str,
        content: &[u8],
        mut authorize: impl FnMut() -> bool,
    ) -> Result<(), Failure> {
        let result = self.stage_replacement_once(temporary_name, content, &mut authorize);
        if let Err(failure) = result {
            self.validation_failure.get_or_insert(failure);
        }
        result
    }

    fn stage_replacement_once(
        &mut self,
        temporary_name: &str,
        content: &[u8],
        authorize: &mut impl FnMut() -> bool,
    ) -> Result<(), Failure> {
        self.live()?;
        if self.replacement_attempted || self.file_acquisition_closed {
            return Err(Failure::Inactive);
        }
        self.replacement_attempted = true;
        if self.writer_paths.len() != 1 || self.files.len() != 1 {
            return Err(Failure::InvalidBounds);
        }
        if self.file_capacity < self.ancestors.len() + 2
            || content.len() > morrow_core::file_content::MAX_CONTENT_BYTES
        {
            return Err(Failure::Limit);
        }
        if temporary_name.contains('/')
            || morrow_core::file_path::RelativeFilePath::parse(temporary_name).is_err()
        {
            return Err(Failure::InvalidBounds);
        }
        let target = &self.writer_paths[0];
        let temporary = target
            .parent()
            .ok_or(Failure::InvalidBounds)?
            .join(temporary_name);
        if temporary == *target {
            return Err(Failure::InvalidBounds);
        }
        let encode = |path: &Path| -> Result<Vec<u16>, Failure> {
            let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
            if wide.is_empty() || wide.len() > 32_760 || wide.contains(&0) {
                return Err(Failure::InvalidBounds);
            }
            wide.push(0);
            Ok(wide)
        };
        let source = encode(&temporary)?;
        let leaf = target
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(Failure::InvalidBounds)?
            .to_owned();
        if !super::single_component(&leaf) {
            return Err(Failure::InvalidBounds);
        }
        if !authorize() {
            return Err(Failure::Inactive);
        }
        self.live()?;
        // SAFETY: terminated host-derived sibling name, finite original
        // transaction, CREATE_NEW (1), no sharing or inherited handle. No
        // path fallback, cross-volume copy, DELETE_ON_CLOSE or restart work.
        let raw = unsafe {
            (self.api.open)(
                source.as_ptr(),
                0xc011_0000,
                0,
                std::ptr::null_mut(),
                1,
                0x0020_0080,
                std::ptr::null_mut(),
                self.handle.ok_or(Failure::Inactive)?,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if raw.is_null() || raw as isize == -1 {
            return Err(Failure::Os(last_error()));
        }
        // SAFETY: register sole ownership before any fallible work. Partial
        // writes, failed flushes and cancellation all use checked owner closure.
        self.files.push(unsafe { File::from_raw_handle(raw) });
        let os_error =
            |error: std::io::Error| Failure::Os(error.raw_os_error().unwrap_or(0) as u32);
        for chunk in content.chunks(64 * 1024) {
            if !authorize() {
                return Err(Failure::Inactive);
            }
            self.live()?;
            self.files
                .last_mut()
                .expect("registered temporary")
                .write_all(chunk)
                .map_err(os_error)?;
        }
        if !authorize() {
            return Err(Failure::Inactive);
        }
        self.live()?;
        self.files
            .last()
            .expect("registered temporary")
            .sync_all()
            .map_err(os_error)?;
        // The original target handle must close before replacement. Retain the
        // original temporary's transacted handle so rename is handle relative,
        // without reopening its parent using broader sharing/access modes.
        self.file_acquisition_closed = true;
        let original = self.files.remove(0).into_raw_handle();
        // SAFETY: remove consumes the one original writer handle exactly once;
        // its transaction retains the writer lock after this checked close.
        if unsafe { CloseHandle(original) } == 0 {
            self.close_unknown = true;
            CLOSE_UNCERTAIN.store(true, Ordering::Release);
            return Err(Failure::CloseUnknown);
        }
        if !authorize() {
            return Err(Failure::Inactive);
        }
        self.live()?;
        // SAFETY: all-zero scalar/array/pointer fields are valid. The same
        // layout is used by the existing native create backend. Only the fixed
        // ReplaceIfExists byte is set; POSIX/readonly bypass flags are absent.
        let mut info: super::RenameInformation = unsafe { std::mem::zeroed() };
        let name: Vec<u16> = leaf.encode_utf16().collect();
        info.flags = 1;
        info.file_name_length = (name.len() * 2) as u32;
        info.file_name[..name.len()].copy_from_slice(&name);
        let mut status = super::IoStatusBlock {
            status: super::IoStatus {
                pointer: std::ptr::null_mut(),
            },
            information: 0,
        };
        // SAFETY: the synchronous original transacted temporary has DELETE
        // access and remains owned here. NULL root + one validated leaf renames
        // only inside its existing parent, held without WRITE/DELETE sharing.
        // The operation belongs to that file handle's original transaction.
        let code = unsafe {
            super::NtSetInformationFile(
                self.files[0].as_raw_handle(),
                &mut status,
                (&info as *const super::RenameInformation).cast(),
                std::mem::size_of::<super::RenameInformation>() as u32,
                10,
            )
        };
        if code < 0 {
            // SAFETY: stateless conversion of the first native status.
            return Err(Failure::Os(unsafe { super::RtlNtStatusToDosError(code) }));
        }
        self.close_files()?;
        self.live()
    }

    /// Close selected file handles exactly once; the transaction keeps its
    /// writer lock. This permanently ends file acquisition, including when no
    /// files were opened. Close uncertainty is process-wide poison, never retryable.
    pub(super) fn close_files(&mut self) -> Result<(), Failure> {
        self.file_acquisition_closed = true;
        for file in self.files.drain(..) {
            let raw = file.into_raw_handle();
            // SAFETY: sole ownership transferred from File; no reconstruction.
            if unsafe { CloseHandle(raw) } == 0 {
                self.close_unknown = true;
                CLOSE_UNCERTAIN.store(true, Ordering::Release);
            }
        }
        if self.close_unknown {
            Err(Failure::CloseUnknown)
        } else {
            Ok(())
        }
    }

    /// Commit is called only after the original durable claim and native effect.
    /// The final callback resamples that owner's authority after file closure;
    /// it grants neither a replacement claim nor permission by itself.
    pub(super) fn commit(mut self, authorize: impl FnOnce() -> bool) -> Result<(), Failure> {
        self.close_files()?;
        self.live()?;
        if !authorize() {
            return Err(Failure::Inactive);
        }
        self.live()?;
        // Mark before calling: a failed commit must never be replayed or turned
        // into an automatic rollback. Its historical outcome remains Unknown.
        self.state = State::Unknown;
        // SAFETY: original transaction, no retained file handles, synchronous.
        if unsafe { (self.api.commit)(self.handle.ok_or(Failure::Inactive)?) } == 0 {
            return Err(Failure::ResolutionUnknown);
        }
        self.state = State::Committed;
        self.close_transaction()?;
        self.close_ancestors()
    }

    pub(super) fn rollback(mut self) -> Result<(), Failure> {
        self.close_files()?;
        self.rollback_once()?;
        self.close_transaction()?;
        self.close_ancestors()
    }

    fn rollback_once(&mut self) -> Result<(), Failure> {
        if self.state != State::Active {
            return Ok(());
        }
        self.state = State::Unknown;
        // SAFETY: this owner holds the transaction; invoked at most once.
        if unsafe { (self.api.rollback)(self.handle.ok_or(Failure::Inactive)?) } == 0 {
            return Err(Failure::ResolutionUnknown);
        }
        self.state = State::RolledBack;
        Ok(())
    }

    fn close_transaction(&mut self) -> Result<(), Failure> {
        if let Some(raw) = self.handle.take() {
            // SAFETY: take retires sole ownership before the one native close.
            if unsafe { CloseHandle(raw) } == 0 {
                self.close_unknown = true;
                CLOSE_UNCERTAIN.store(true, Ordering::Release);
            }
        }
        if self.close_unknown {
            Err(Failure::CloseUnknown)
        } else {
            Ok(())
        }
    }

    fn close_ancestors(&mut self) -> Result<(), Failure> {
        for file in self.ancestors.drain(..).rev() {
            let raw = file.into_raw_handle();
            // SAFETY: drain consumes this owner's original directory handle
            // once, after resolution. An uncertain close is never reconstructed.
            if unsafe { CloseHandle(raw) } == 0 {
                self.close_unknown = true;
                CLOSE_UNCERTAIN.store(true, Ordering::Release);
            }
        }
        if self.close_unknown {
            Err(Failure::CloseUnknown)
        } else {
            Ok(())
        }
    }
}

impl<C> Drop for Transaction<C> {
    fn drop(&mut self) {
        let _ = self.close_files();
        // A transaction with no attempted commit has no authorized published
        // effect. The original owner rolls it back once, including unwinding.
        // An uncertain completion is never retried with any API.
        let _ = self.rollback_once();
        let _ = self.close_transaction();
        let _ = self.close_ancestors();
        if self.close_unknown || self.state == State::Unknown {
            if let Some(charge) = self.charge.take() {
                std::mem::forget(charge);
            }
        }
    }
}

#[cfg(test)]
#[path = "transaction/tests.rs"]
mod tests;
