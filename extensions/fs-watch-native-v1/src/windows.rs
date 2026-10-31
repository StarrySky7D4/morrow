//! One original-thread native owner. No guest paths, raw-handle adoption,
//! automatic replay, shared-budget grants, or implicit close in Drop.
use crate::notification::{self, BUFFER_BYTES, Notification};
use morrow_fs_watch_v1::{Batch, End, wire_digest};
use std::{
    cell::UnsafeCell,
    fs::File,
    marker::PhantomData,
    os::windows::io::IntoRawHandle,
    ptr,
    rc::Rc,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_NOT_FOUND, ERROR_NOTIFY_ENUM_DIR,
        ERROR_OPERATION_ABORTED, GetLastError, HANDLE, WAIT_TIMEOUT,
    },
    Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
        FILE_ID_INFO, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
        FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FileAttributeTagInfo, FileIdInfo,
        GetFileInformationByHandleEx, ReadDirectoryChangesW,
    },
    System::{
        IO::{CancelIoEx, GetOverlappedResultEx, IO_STATUS_BLOCK, OVERLAPPED},
        Threading::{CreateEventW, ResetEvent},
    },
};

pub const MAX_WINDOW: Duration = Duration::from_secs(60);
pub const MAX_FRAMES: u32 = 64;
pub const MAX_POLL: Duration = Duration::from_millis(100);
pub const MAX_CANCEL_WAIT: Duration = Duration::from_secs(5);
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationFile(
        handle: HANDLE,
        status: *mut IO_STATUS_BLOCK,
        information: *mut std::ffi::c_void,
        length: u32,
        class: i32,
    ) -> i32;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    SourceAttributes,
    SourceIdentity,
    SourceMode,
    CreateEvent,
    ResetEvent,
    Read,
    Completion,
    Cancel,
    CloseDirectory,
    CloseEvent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    InvalidInput,
    WrongState,
    Deadline,
    Limit,
    Identity,
    SynchronousSource,
    UnexpectedSourceMode,
    Notification,
    Codec,
    Correlation,
    Native { call: Call, code: u32 },
    NativeStatus { call: Call, status: i32 },
    Unknown,
}
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "directory native observation {self:?}")
    }
}
impl std::error::Error for Fault {}

/// Trusted host limits only, never a plugin grant or a replacement deadline.
pub struct Window {
    pub epoch: [u8; 32],
    pub max_frames: u32,
    pub duration: Duration,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub owned_handles: u32,
    pub close_calls: u32,
    pub confirmed_closed: u32,
    pub read_calls: u32,
    pub completions: u32,
    pub observed_frames: u32,
    pub identity_checked: bool,
    pub original_directory_consumed: bool,
    pub mode_query_calls: u32,
    pub mode_query_pending: bool,
    pub asynchronous_verified: bool,
    pub read_in_flight: bool,
    pub delivery_pending: bool,
    pub cancel_called: bool,
    pub cancel_error: Option<u32>,
    pub discarded_completion_bytes: u32,
    pub first_error: Option<Fault>,
    pub unknown: bool,
    pub retired: bool,
}

/// Only the original owner can create this, after all original native closes.
/// It is deliberately not Clone. It proves no delivery, approval, or worker join.
pub struct Closed {
    snapshot: Snapshot,
}
impl Closed {
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
    }
}

pub enum Poll {
    Pending,
    DeadlineReached,
    /// Exact Cap'n Proto bytes. A local matching ACK permits the next read;
    /// it does not independently prove a guest received this frame.
    Frame {
        sequence: u64,
        wire: Vec<u8>,
    },
}

struct Handle {
    raw: HANDLE,
    attempted: bool,
    closed: bool,
}
impl Handle {
    fn new(raw: HANDLE) -> Self {
        Self {
            raw,
            attempted: false,
            closed: false,
        }
    }
    fn close_once(&mut self, call: Call) -> Result<(), Fault> {
        if self.attempted || self.closed {
            return Err(Fault::WrongState);
        }
        self.attempted = true;
        // SAFETY: exactly this owner-created handle; no reconstruction/duplicate.
        if unsafe { CloseHandle(self.raw) } == 0 {
            return Err(Fault::Native {
                call,
                code: unsafe { GetLastError() },
            });
        }
        self.closed = true;
        Ok(())
    }
}

// The kernel writes these while IO is outstanding. UnsafeCell avoids creating
// Rust references to memory concurrently modified through the original pointers.
// Both addresses remain stable in this one Box until completion is observed.
struct IoStorage {
    overlapped: UnsafeCell<OVERLAPPED>,
    words: UnsafeCell<[u32; BUFFER_BYTES / 4]>,
}
struct ModeQuery {
    status: UnsafeCell<IO_STATUS_BLOCK>,
    flags: UnsafeCell<u32>,
}
struct PendingFrame {
    sequence: u64,
    digest: [u8; 32],
    end: End,
}
struct State {
    directory: Option<Handle>,
    event: Option<Handle>,
    io: Box<IoStorage>,
    mode: Box<ModeQuery>,
    identity: Option<[u8; 24]>,
    epoch: [u8; 32],
    deadline: Instant,
    max_frames: u32,
    next_sequence: u64,
    pending: Option<PendingFrame>,
    ended: bool,
    snapshot: Snapshot,
}
impl State {
    fn error(&mut self, value: Fault) -> Fault {
        if self.snapshot.first_error.is_none() {
            self.snapshot.first_error = Some(value);
        }
        value
    }
    fn unknown(&mut self, value: Fault) -> Fault {
        self.snapshot.unknown = true;
        self.error(value)
    }
    fn usable(&self) -> Result<(), Fault> {
        if self.snapshot.unknown {
            Err(Fault::Unknown)
        } else if self.snapshot.retired {
            Err(Fault::WrongState)
        } else {
            Ok(())
        }
    }
    fn native_completed(&mut self) {
        self.snapshot.read_in_flight = false;
        self.snapshot.completions += 1;
    }
    fn frame(&mut self, changes: Vec<morrow_fs_watch_v1::Change>, end: End) -> Result<Poll, Fault> {
        let sequence = self.next_sequence;
        let wire = Batch {
            watch_epoch: self.epoch,
            sequence,
            changes,
            end,
        }
        .encode()
        .map_err(|_| self.error(Fault::Codec))?;
        self.snapshot.observed_frames += 1;
        self.pending = Some(PendingFrame {
            sequence,
            digest: wire_digest(&wire),
            end,
        });
        self.snapshot.delivery_pending = true;
        Ok(Poll::Frame { sequence, wire })
    }
}

pub struct NativeWatch {
    state: Option<Box<State>>,
    // The original worker constructs and keeps this owner. No cross-thread
    // Send/Sync escape is supplied for its native operation or closure capability.
    original_thread: PhantomData<Rc<()>>,
}
impl std::fmt::Debug for NativeWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeWatch")
            .field("snapshot", &self.snapshot())
            .finish_non_exhaustive()
    }
}
pub struct StartFailure {
    pub reason: Fault,
    /// Partial native resources still belong to this exact original owner.
    pub owner: NativeWatch,
}
impl std::fmt::Debug for StartFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartFailure")
            .field("reason", &self.reason)
            .field("owner", &self.owner)
            .finish()
    }
}

fn identity(handle: HANDLE) -> Result<[u8; 24], Fault> {
    let attribute_call = Call::SourceAttributes;
    let id_call = Call::SourceIdentity;
    let mut attributes = FILE_ATTRIBUTE_TAG_INFO::default();
    // SAFETY: live original borrowed/owned handle and exactly sized local output.
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileAttributeTagInfo,
            ptr::from_mut(&mut attributes).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    } == 0
    {
        return Err(Fault::Native {
            call: attribute_call,
            code: unsafe { GetLastError() },
        });
    }
    if attributes.FileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
        || attributes.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || attributes.ReparseTag != 0
    {
        return Err(Fault::Identity);
    }
    let mut id = FILE_ID_INFO::default();
    // SAFETY: same live handle; FILE_ID_INFO is a fixed-layout initialized output.
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            ptr::from_mut(&mut id).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
    {
        return Err(Fault::Native {
            call: id_call,
            code: unsafe { GetLastError() },
        });
    }
    let mut bytes = [0; 24];
    bytes[..8].copy_from_slice(&id.VolumeSerialNumber.to_le_bytes());
    bytes[8..].copy_from_slice(&id.FileId.Identifier);
    if bytes == [0; 24] {
        return Err(Fault::Identity);
    }
    Ok(bytes)
}

impl NativeWatch {
    /// Minimum resident allocation, excluding allocator bookkeeping, original
    /// selected directory/ancestors, output bytes, and caller worker/ledger.
    /// It is not a sufficient standalone product resource budget.
    pub const RESIDENT_BYTES: usize =
        size_of::<Self>() + size_of::<State>() + size_of::<IoStorage>() + size_of::<ModeQuery>();

    /// Consumes the exact host-captured File. No reopening, duplication,
    /// pathname lookup, ID lookup, or adoption from an integer handle occurs.
    /// The original approval/selection owner must retain every ancestor and
    /// reserve this resource before transfer. This API is not itself authority.
    pub fn open(original: File, window: Window) -> Result<Self, StartFailure> {
        let now = Instant::now();
        let deadline = now.checked_add(window.duration).unwrap_or(now);
        let raw = original.into_raw_handle();
        let mut owner = Self {
            state: Some(Box::new(State {
                directory: Some(Handle::new(raw)),
                event: None,
                io: Box::new(IoStorage {
                    overlapped: UnsafeCell::new(OVERLAPPED::default()),
                    words: UnsafeCell::new([0; BUFFER_BYTES / 4]),
                }),
                mode: Box::new(ModeQuery {
                    status: UnsafeCell::new(IO_STATUS_BLOCK::default()),
                    flags: UnsafeCell::new(0),
                }),
                identity: None,
                epoch: window.epoch,
                deadline,
                max_frames: window.max_frames,
                next_sequence: 1,
                pending: None,
                ended: false,
                snapshot: Snapshot {
                    owned_handles: 1,
                    original_directory_consumed: true,
                    ..Snapshot::default()
                },
            })),
            original_thread: PhantomData,
        };
        let result = (|| {
            let s = owner.state.as_mut().unwrap();
            if window.epoch == [0; 32]
                || window.max_frames == 0
                || window.max_frames > MAX_FRAMES
                || window.duration.is_zero()
                || window.duration > MAX_WINDOW
                || deadline <= now
            {
                return Err(Fault::InvalidInput);
            }
            s.identity = Some(identity(raw)?);
            s.snapshot.identity_checked = true;
            s.snapshot.mode_query_calls = 1;
            s.snapshot.mode_query_pending = true;
            // SAFETY: original owned handle; both outputs are stable owned
            // UnsafeCells. If STATUS_PENDING is returned, they stay retained
            // with this original Unknown owner and are never read or freed.
            let status = unsafe {
                NtQueryInformationFile(raw, s.mode.status.get(), s.mode.flags.get().cast(), 4, 16)
            };
            if status == 0x103 {
                return Err(s.unknown(Fault::NativeStatus {
                    call: Call::SourceMode,
                    status,
                }));
            }
            s.snapshot.mode_query_pending = false;
            if status != 0 {
                return Err(Fault::NativeStatus {
                    call: Call::SourceMode,
                    status,
                });
            }
            // The definitive successful return proves the output is complete.
            // For non-PENDING NT returns, use that return status, not IOSB.Status.
            let information = unsafe { (*s.mode.status.get()).Information };
            if information != 4 {
                return Err(Fault::UnexpectedSourceMode);
            }
            let mode = unsafe { *s.mode.flags.get() };
            if mode & 0x30 != 0 {
                return Err(Fault::SynchronousSource);
            }
            // Refuse delete-on-close and unbuffered objects for this profile.
            if mode & 0x1008 != 0 {
                return Err(Fault::UnexpectedSourceMode);
            }
            s.snapshot.asynchronous_verified = true;
            // SAFETY: unnamed, non-inherited, unique manual-reset event.
            let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
            if event.is_null() {
                return Err(Fault::Native {
                    call: Call::CreateEvent,
                    code: unsafe { GetLastError() },
                });
            }
            s.event = Some(Handle::new(event));
            s.snapshot.owned_handles += 1;
            Ok(())
        })();
        if let Err(reason) = result {
            owner.state.as_mut().unwrap().error(reason);
            return Err(StartFailure { reason, owner });
        }
        Ok(owner)
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.as_ref().expect("original live owner").snapshot
    }

    pub fn arm(&mut self) -> Result<(), Fault> {
        let s = self.state.as_mut().unwrap();
        s.usable()?;
        if s.ended
            || s.pending.is_some()
            || s.snapshot.read_in_flight
            || s.snapshot.first_error.is_some()
            || !s.snapshot.identity_checked
            || s.identity.is_none()
            || !s.snapshot.asynchronous_verified
        {
            return Err(Fault::WrongState);
        }
        if Instant::now() >= s.deadline {
            return Err(Fault::Deadline);
        }
        if s.snapshot.read_calls >= s.max_frames {
            return Err(Fault::Limit);
        }
        let directory = s.directory.as_ref().ok_or(Fault::WrongState)?.raw;
        let event = s.event.as_ref().ok_or(Fault::WrongState)?.raw;
        let current = identity(directory).map_err(|error| s.error(error))?;
        if s.identity != Some(current) {
            return Err(s.error(Fault::Identity));
        }
        if Instant::now() >= s.deadline {
            return Err(Fault::Deadline);
        }
        // SAFETY: no IO is pending and this event belongs only to this owner.
        if unsafe { ResetEvent(event) } == 0 {
            return Err(s.error(Fault::Native {
                call: Call::ResetEvent,
                code: unsafe { GetLastError() },
            }));
        }
        // SAFETY: the previous operation completed; only now may the same
        // original OVERLAPPED be initialized for a new bounded observation.
        unsafe {
            *s.io.overlapped.get() = OVERLAPPED {
                hEvent: event,
                ..OVERLAPPED::default()
            };
        }
        s.snapshot.read_calls += 1;
        s.snapshot.read_in_flight = true; // latch before native dispatch
        // SAFETY: fixed DWORD-aligned buffer and stable OVERLAPPED remain owned
        // and untouched until completion; shallow observation, no callback.
        let queued = unsafe {
            ReadDirectoryChangesW(
                directory,
                s.io.words.get().cast(),
                BUFFER_BYTES as u32,
                0,
                FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_LAST_WRITE,
                ptr::null_mut(),
                s.io.overlapped.get(),
                None,
            )
        };
        if queued == 0 {
            let code = unsafe { GetLastError() };
            if code != ERROR_IO_PENDING {
                s.snapshot.read_in_flight = false;
                return Err(s.error(Fault::Native {
                    call: Call::Read,
                    code,
                }));
            }
        }
        Ok(())
    }

    /// One short bounded wait. Original approval/deadline checks belong around
    /// each call in the host owner; this method creates no replacement authority.
    pub fn poll(&mut self, wait: Duration) -> Result<Poll, Fault> {
        let s = self.state.as_mut().unwrap();
        s.usable()?;
        if !s.snapshot.read_in_flight || s.pending.is_some() || s.ended {
            return Err(Fault::WrongState);
        }
        if wait > MAX_POLL {
            return Err(Fault::InvalidInput);
        }
        let remaining = s.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(Poll::DeadlineReached);
        }
        let millis = wait.min(remaining).as_millis() as u32;
        let mut transferred = 0;
        // SAFETY: original live directory/event and original stable operation;
        // no alertable wait/callback, never INFINITE.
        let done = unsafe {
            GetOverlappedResultEx(
                s.directory.as_ref().unwrap().raw,
                s.io.overlapped.get(),
                &mut transferred,
                millis,
                0,
            )
        };
        if done == 0 {
            let code = unsafe { GetLastError() };
            if matches!(code, ERROR_IO_INCOMPLETE | WAIT_TIMEOUT) {
                return Ok(Poll::Pending);
            }
            if code == ERROR_NOTIFY_ENUM_DIR {
                s.native_completed();
                s.ended = true;
                return s.frame(Vec::new(), End::Gap);
            }
            if code == ERROR_OPERATION_ABORTED {
                s.native_completed();
                s.ended = true;
                s.error(Fault::Native {
                    call: Call::Completion,
                    code,
                });
                return s.frame(Vec::new(), End::Cancelled);
            }
            // An unclassified failure is not used as proof that the operation
            // released its pointers. Keep the exact allocation and handles.
            return Err(s.unknown(Fault::Native {
                call: Call::Completion,
                code,
            }));
        }
        s.native_completed();
        let current =
            identity(s.directory.as_ref().unwrap().raw).map_err(|error| s.error(error))?;
        if s.identity != Some(current) {
            s.ended = true;
            return Err(s.error(Fault::Identity));
        }
        if Instant::now() >= s.deadline {
            // A completed read can race the original deadline. Completion is
            // retained, but late bytes are not promoted to an approved frame.
            s.snapshot.discarded_completion_bytes = transferred;
            s.ended = true;
            return Ok(Poll::DeadlineReached);
        }
        if transferred as usize > BUFFER_BYTES {
            s.ended = true;
            return Err(s.error(Fault::Notification));
        }
        // SAFETY: completion was observed; the kernel no longer owns these
        // bytes. Only the exact returned prefix is exposed to the safe parser.
        let bytes = unsafe {
            std::slice::from_raw_parts(s.io.words.get().cast::<u8>(), transferred as usize)
        };
        match notification::decode(bytes) {
            Ok(Notification::Changes(changes)) => s.frame(changes, End::Continue),
            Ok(Notification::Gap) => {
                s.ended = true;
                s.frame(Vec::new(), End::Gap)
            }
            Err(_) => {
                s.ended = true;
                Err(s.error(Fault::Notification))
            }
        }
    }

    pub fn acknowledge(&mut self, sequence: u64, digest: [u8; 32]) -> Result<(), Fault> {
        let s = self.state.as_mut().unwrap();
        s.usable()?;
        let pending = s.pending.as_ref().ok_or(Fault::WrongState)?;
        if pending.sequence != sequence || pending.digest != digest {
            return Err(Fault::Correlation);
        }
        let terminal = pending.end != End::Continue;
        s.next_sequence = sequence
            .checked_add(1)
            .ok_or_else(|| s.unknown(Fault::Limit))?;
        s.pending = None;
        s.snapshot.delivery_pending = false;
        s.ended |= terminal;
        Ok(())
    }

    /// Exactly one native cancellation attempt if necessary, one bounded
    /// completion observation, then one close attempt for each original handle.
    /// Timeout/ambiguity permanently blocks this owner's further native calls.
    pub fn retire(&mut self, cancel_wait: Duration) -> Result<Closed, Fault> {
        let s = self.state.as_mut().unwrap();
        s.usable()?;
        if cancel_wait.is_zero() || cancel_wait > MAX_CANCEL_WAIT {
            return Err(Fault::InvalidInput);
        }
        if s.snapshot.mode_query_pending {
            return Err(s.unknown(Fault::Unknown));
        }
        s.ended = true;
        if s.snapshot.read_in_flight {
            if s.snapshot.cancel_called {
                return Err(s.unknown(Fault::WrongState));
            }
            s.snapshot.cancel_called = true;
            let directory = s.directory.as_ref().unwrap().raw;
            // SAFETY: exact pending operation on the original live handle;
            // neither its storage nor handles are freed on a cancellation request.
            if unsafe { CancelIoEx(directory, s.io.overlapped.get()) } == 0 {
                let code = unsafe { GetLastError() };
                s.snapshot.cancel_error = Some(code);
                if code != ERROR_NOT_FOUND {
                    return Err(s.unknown(Fault::Native {
                        call: Call::Cancel,
                        code,
                    }));
                }
            }
            let mut transferred = 0;
            // SAFETY: same original operation; known finite wait independent of
            // the expired observation window, never an extension of data approval.
            let done = unsafe {
                GetOverlappedResultEx(
                    directory,
                    s.io.overlapped.get(),
                    &mut transferred,
                    cancel_wait.as_millis() as u32,
                    0,
                )
            };
            if done == 0 {
                let code = unsafe { GetLastError() };
                if !matches!(code, ERROR_OPERATION_ABORTED | ERROR_NOTIFY_ENUM_DIR) {
                    return Err(s.unknown(Fault::Native {
                        call: Call::Completion,
                        code,
                    }));
                }
            } else {
                s.snapshot.discarded_completion_bytes = transferred;
            }
            s.native_completed();
        }
        // No native operation may reference the buffer/event before either close.
        for call in [Call::CloseDirectory, Call::CloseEvent] {
            let handle = if call == Call::CloseDirectory {
                &mut s.directory
            } else {
                &mut s.event
            };
            if let Some(handle) = handle {
                s.snapshot.close_calls += 1;
                match handle.close_once(call) {
                    Ok(()) => s.snapshot.confirmed_closed += 1,
                    Err(error) => {
                        // The first ambiguous original close seals the owner.
                        // Do not close another handle after Unknown was latched.
                        return Err(s.unknown(error));
                    }
                }
            }
        }
        if s.snapshot.unknown
            || s.snapshot.read_in_flight
            || s.snapshot.owned_handles != s.snapshot.confirmed_closed
        {
            return Err(s.unknown(Fault::Unknown));
        }
        s.snapshot.retired = true;
        Ok(Closed {
            snapshot: s.snapshot,
        })
    }
}

impl Drop for NativeWatch {
    fn drop(&mut self) {
        if let Some(state) = self.state.take()
            && !state.snapshot.retired
        {
            // Preserve the exact original handles, pending kernel pointers,
            // first error and counters. No Drop cleanup or second observer.
            let _ = Box::leak(state);
        }
    }
}
