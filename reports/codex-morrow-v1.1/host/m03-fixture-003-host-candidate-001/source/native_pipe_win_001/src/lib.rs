//! Small Windows-only owned overlapped facade. Each operation owns stable storage until
//! actual completion. Cancellation is a request, and Drop is a blocking safety backstop.
#![cfg(windows)]
#![deny(unsafe_op_in_unsafe_fn)]
use std::{
    cell::UnsafeCell,
    io,
    mem::{ManuallyDrop, size_of},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{IO::*, Pipes::*, Threading::*},
};

pub const MAX_IO: usize = 32772;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Connect,
    Read,
    Write,
}
#[derive(Clone, Copy, Debug)]
pub struct Started {
    pub id: u64,
    pub kind: Kind,
    pub pending: bool,
    pub requested: usize,
}
#[derive(Debug)]
pub struct Completed {
    pub id: u64,
    pub kind: Kind,
    pub transferred: usize,
    pub error: Option<u32>,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Copy, Debug)]
pub struct BufferInfo {
    pub inbound: u32,
    pub outbound: u32,
    pub flags: u32,
    pub max_instances: u32,
}
struct LocalAllocation(*mut std::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}
fn wide(s: &str) -> io::Result<Vec<u16>> {
    if s.contains('\0') {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(s.encode_utf16().chain(Some(0)).collect())
}
fn own(raw: HANDLE) -> io::Result<OwnedHandle> {
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
    }
}
fn raw(h: &OwnedHandle) -> HANDLE {
    h.as_raw_handle()
}
fn name(value: &str) -> io::Result<Vec<u16>> {
    let suffix = value
        .strip_prefix(r"\\.\pipe\morrow-m03-")
        .ok_or(io::ErrorKind::InvalidInput)?;
    if suffix.is_empty()
        || suffix.len() > 200
        || !suffix
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    wide(value)
}
fn descriptor() -> io::Result<LocalAllocation> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = own(token)?;
    let mut size = 0;
    unsafe {
        GetTokenInformation(raw(&token), TokenUser, ptr::null_mut(), 0, &mut size);
    }
    if size < size_of::<TOKEN_USER>() as u32 || size > 65536 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    // usize allocation guarantees TOKEN_USER alignment; the token SID stays within this buffer.
    let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            raw(&token),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut sid = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _sid = LocalAllocation(sid.cast());
    let mut len = 0;
    // ConvertSidToStringSidW owns a NUL-terminated SID string. Windows SID string is bounded.
    while len < 256 && unsafe { *sid.add(len) } != 0 {
        len += 1;
    }
    if len == 256 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(sid, len) })
        .map_err(|_| io::ErrorKind::InvalidData)?;
    // Named-pipe handles use the file access mapping. Use explicit file-all rights so
    // kernel DACL readback is comparable without mistaking GA -> FA mapping for drift.
    let text = wide(&format!("D:P(A;;FA;;;SY)(A;;FA;;;{sid})"))?;
    let mut descriptor = ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(LocalAllocation(descriptor))
}
struct Operation {
    id: u64,
    kind: Kind,
    overlapped: Box<UnsafeCell<OVERLAPPED>>,
    _event: OwnedHandle,
    buffer: Box<[UnsafeCell<u8>]>,
    immediate: Option<(u32, Option<u32>)>,
}
impl Operation {
    fn new(id: u64, kind: Kind, buffer: Vec<u8>) -> io::Result<Self> {
        let event = own(unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) })?;
        let overlapped = Box::new(UnsafeCell::new(OVERLAPPED {
            hEvent: raw(&event),
            ..OVERLAPPED::default()
        }));
        Ok(Self {
            id,
            kind,
            overlapped,
            _event: event,
            buffer: buffer.into_iter().map(UnsafeCell::new).collect(),
            immediate: None,
        })
    }
    fn issued(&mut self, ok: bool, transferred: u32) -> io::Result<bool> {
        if ok {
            self.immediate = Some((transferred, None));
            return Ok(false);
        }
        let e = io::Error::last_os_error();
        let code = e.raw_os_error().unwrap_or(0) as u32;
        if code == ERROR_IO_PENDING {
            return Ok(true);
        }
        if self.kind == Kind::Connect && code == ERROR_PIPE_CONNECTED {
            self.immediate = Some((0, None));
            return Ok(false);
        }
        // Failed initiation owns no outstanding I/O; retain the error for the normal reaper.
        self.immediate = Some((0, Some(code)));
        Ok(false)
    }
    fn completion(&self, handle: HANDLE, wait: bool) -> io::Result<Option<(u32, Option<u32>)>> {
        if let Some(value) = self.immediate {
            return Ok(Some(value));
        }
        let mut n = 0;
        if unsafe { GetOverlappedResult(handle, self.overlapped.get(), &mut n, wait.into()) } != 0 {
            return Ok(Some((n, None)));
        }
        let error = unsafe { GetLastError() };
        if error == ERROR_IO_INCOMPLETE {
            Ok(None)
        } else if matches!(
            error,
            ERROR_OPERATION_ABORTED
                | ERROR_BROKEN_PIPE
                | ERROR_NO_DATA
                | ERROR_PIPE_NOT_CONNECTED
                | ERROR_MORE_DATA
                | ERROR_HANDLE_EOF
                | ERROR_CONNECTION_ABORTED
        ) {
            Ok(Some((n, Some(error))))
        } else {
            // An unexpected API failure is not proof that the OS released the storage.
            // Retain operation + original handle for retry or conservative unconfirmed cleanup.
            Err(io::Error::from_raw_os_error(error as i32))
        }
    }
}

/// Single owner; methods require &mut for issue/reap. Moving ownership between threads
/// does not move pinned OVERLAPPED allocations, buffers or OS events.
pub struct Pipe {
    handle: ManuallyDrop<OwnedHandle>,
    connect: Option<Operation>,
    read: Option<Operation>,
    write: Option<Operation>,
    next_id: u64,
    connected: bool,
    cancelling: bool,
    server: bool,
}
// SAFETY: all OS-referenced memory is heap allocated and never borrowed/accessed while
// pending. No concurrent API access is possible through safe shared references. CancelIoEx
// is process-wide and reaping may legally run on a thread other than the issuing thread.
unsafe impl Send for Pipe {}
impl Pipe {
    pub fn create_private(locator: &str) -> io::Result<Self> {
        let name = name(locator)?;
        let sd = descriptor()?;
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: 0,
        };
        let handle = own(unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                1024,
                1024,
                0,
                &security,
            )
        })?;
        Ok(Self {
            handle: ManuallyDrop::new(handle),
            connect: None,
            read: None,
            write: None,
            next_id: 1,
            connected: false,
            cancelling: false,
            server: true,
        })
    }
    /// One attempt; caller receives errors rather than automatic connect/retry or fallback.
    pub fn open_client(locator: &str) -> io::Result<Self> {
        let name = name(locator)?;
        let handle = own(unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                ptr::null_mut(),
            )
        })?;
        Ok(Self {
            handle: ManuallyDrop::new(handle),
            connect: None,
            read: None,
            write: None,
            next_id: 1,
            connected: true,
            cancelling: false,
            server: false,
        })
    }
    fn operation(&mut self, kind: Kind, buffer: Vec<u8>) -> io::Result<Operation> {
        if self.cancelling || buffer.len() > MAX_IO {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(io::ErrorKind::Other)?;
        Operation::new(id, kind, buffer)
    }
    pub fn begin_connect(&mut self) -> io::Result<Started> {
        if !self.server || self.connected || self.connect.is_some() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let mut op = self.operation(Kind::Connect, vec![])?;
        let ok = unsafe { ConnectNamedPipe(raw(&self.handle), op.overlapped.get()) } != 0;
        let pending = op.issued(ok, 0)?;
        let started = Started {
            id: op.id,
            kind: op.kind,
            pending,
            requested: 0,
        };
        self.connect = Some(op);
        Ok(started)
    }
    pub fn begin_read(&mut self, limit: usize) -> io::Result<Started> {
        if !self.connected || self.read.is_some() || limit == 0 || limit > MAX_IO {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut op = self.operation(Kind::Read, vec![0; limit])?;
        let mut n = 0;
        let ok = unsafe {
            ReadFile(
                raw(&self.handle),
                op.buffer.as_ptr().cast::<u8>().cast_mut(),
                limit as u32,
                &mut n,
                op.overlapped.get(),
            )
        } != 0;
        let pending = op.issued(ok, n)?;
        let started = Started {
            id: op.id,
            kind: op.kind,
            pending,
            requested: limit,
        };
        self.read = Some(op);
        Ok(started)
    }
    pub fn begin_write(&mut self, bytes: Vec<u8>) -> io::Result<Started> {
        if !self.connected || self.write.is_some() || bytes.is_empty() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut op = self.operation(Kind::Write, bytes)?;
        let mut n = 0;
        let ok = unsafe {
            WriteFile(
                raw(&self.handle),
                op.buffer.as_ptr().cast::<u8>(),
                op.buffer.len() as u32,
                &mut n,
                op.overlapped.get(),
            )
        } != 0;
        let pending = op.issued(ok, n)?;
        let started = Started {
            id: op.id,
            kind: op.kind,
            pending,
            requested: op.buffer.len(),
        };
        self.write = Some(op);
        Ok(started)
    }
    pub fn poll(&mut self, kind: Kind) -> io::Result<Option<Completed>> {
        self.reap(kind, false)
    }
    fn reap(&mut self, kind: Kind, wait: bool) -> io::Result<Option<Completed>> {
        let slot = match kind {
            Kind::Connect => &mut self.connect,
            Kind::Read => &mut self.read,
            Kind::Write => &mut self.write,
        };
        let Some(op) = slot.as_ref() else {
            return Err(io::ErrorKind::NotFound.into());
        };
        let Some((n, error)) = op.completion(raw(&self.handle), wait)? else {
            return Ok(None);
        };
        let op = slot.take().unwrap();
        if kind == Kind::Connect && error.is_none() {
            self.connected = true;
        }
        let bytes = if kind == Kind::Read && error.is_none() {
            if n as usize > op.buffer.len() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            // SAFETY: completion was observed above, so the kernel no longer writes.
            op.buffer
                .iter()
                .take(n as usize)
                .map(|cell| unsafe { *cell.get() })
                .collect()
        } else {
            vec![]
        };
        Ok(Some(Completed {
            id: op.id,
            kind,
            transferred: n as usize,
            error,
            bytes,
        }))
    }
    pub fn client_pid(&self) -> io::Result<u32> {
        if !self.server || !self.connected {
            return Err(io::ErrorKind::NotConnected.into());
        }
        let mut pid = 0;
        if unsafe { GetNamedPipeClientProcessId(raw(&self.handle), &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(pid)
    }
    pub fn buffer_info(&self) -> io::Result<BufferInfo> {
        let mut info = BufferInfo {
            inbound: 0,
            outbound: 0,
            flags: 0,
            max_instances: 0,
        };
        if unsafe {
            GetNamedPipeInfo(
                raw(&self.handle),
                &mut info.flags,
                &mut info.outbound,
                &mut info.inbound,
                &mut info.max_instances,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(info)
    }
    pub fn inheritable(&self) -> io::Result<bool> {
        let mut flags = 0;
        if unsafe { GetHandleInformation(raw(&self.handle), &mut flags) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(flags & HANDLE_FLAG_INHERIT != 0)
    }
    /// Read back the actual kernel-object DACL, for trusted diagnostics/qualification.
    pub fn security_sddl(&self) -> io::Result<String> {
        let mut sd = ptr::null_mut();
        let error = unsafe {
            GetSecurityInfo(
                raw(&self.handle),
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut sd,
            )
        };
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error as i32));
        }
        let sd = LocalAllocation(sd);
        sddl_text(sd.0)
    }
    pub fn has_operation(&self, kind: Kind) -> bool {
        match kind {
            Kind::Connect => self.connect.is_some(),
            Kind::Read => self.read.is_some(),
            Kind::Write => self.write.is_some(),
        }
    }
    /// Prevents all future issue. Even ERROR_NOT_FOUND is not a completion receipt.
    pub fn cancel_all(&mut self) -> io::Result<()> {
        self.cancelling = true;
        if unsafe { CancelIoEx(raw(&self.handle), ptr::null()) } == 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(ERROR_NOT_FOUND as i32) {
                return Err(e);
            }
        }
        Ok(())
    }
    /// Blocking safety/worker boundary, not a UI call. Caller can poll after cancel_all
    /// instead; this never frees storage before each outstanding operation completes.
    pub fn cancel_and_reap(&mut self) -> io::Result<Vec<Completed>> {
        let cancel_error = self.cancel_all().err();
        let mut completions = vec![];
        for kind in [Kind::Connect, Kind::Read, Kind::Write] {
            if self.has_operation(kind) {
                if let Some(c) = self.reap(kind, true)? {
                    completions.push(c);
                } else {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
            }
        }
        if let Some(e) = cancel_error {
            return Err(e);
        }
        Ok(completions)
    }
}
impl Drop for Pipe {
    fn drop(&mut self) {
        // An I/O error cannot justify freeing a buffer that the OS might still touch.
        // Ordinarily cancellation immediately reaps. If not, retain the storage by leaking
        // it and the handle instead of introducing a use-after-free during panic/unwind.
        let _ = self.cancel_all();
        for kind in [Kind::Connect, Kind::Read, Kind::Write] {
            if self.has_operation(kind) {
                let _ = self.reap(kind, true);
            }
        }
        if self.connect.is_some() || self.read.is_some() || self.write.is_some() {
            if let Some(op) = self.connect.take() {
                std::mem::forget(op);
            }
            if let Some(op) = self.read.take() {
                std::mem::forget(op);
            }
            if let Some(op) = self.write.take() {
                std::mem::forget(op);
            }
            // ManuallyDrop retains the original handle, without a fallible clone.
        } else {
            // SAFETY: all OS operations have completed; this is the unique owned handle.
            unsafe {
                ManuallyDrop::drop(&mut self.handle);
            }
        }
    }
}
fn sddl_text(sd: PSECURITY_DESCRIPTOR) -> io::Result<String> {
    let mut text = ptr::null_mut();
    let mut length = 0;
    if unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            sd,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let _allocation = LocalAllocation(text.cast());
    if length == 0 || length > 4096 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let value = unsafe { std::slice::from_raw_parts(text, length as usize) };
    let end = value.iter().position(|c| *c == 0).unwrap_or(value.len());
    String::from_utf16(&value[..end]).map_err(|_| io::ErrorKind::InvalidData.into())
}
pub fn expected_private_sddl() -> io::Result<String> {
    let sd = descriptor()?;
    sddl_text(sd.0)
}
