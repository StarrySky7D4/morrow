//! Opt-in, same-HARNESS caller facts only. No launch/token mutation or authority.
//! Raw originals stay in preinstalled slots, including across failure/unwind.
//! There is deliberately no native Drop, retry, PID selector or deserialization.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{ffi::c_void, io::Cursor, mem, panic::{catch_unwind, AssertUnwindSafe},
    ptr, thread::{self, ThreadId}, time::{Duration, Instant}};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, FILETIME, HANDLE, LUID},
    Security::{GetTokenInformation, LookupPrivilegeValueW, TokenUser, TokenStatistics,
        TokenIntegrityLevel, TokenSessionId, TokenElevation, TokenElevationType,
        TokenHasRestrictions, TokenPrivileges, TOKEN_INFORMATION_CLASS, TOKEN_USER,
        TOKEN_MANDATORY_LABEL},
    System::{LibraryLoader::{GetModuleHandleW, GetProcAddress},
        Threading::{GetCurrentProcess, GetCurrentThread, GetCurrentProcessId,
            GetProcessTimes, OpenProcessToken, OpenThreadToken}},
};

const BUFFER_BYTES: usize = 65536;
pub const CALLER_PREREQUISITE_OUTPUT_BYTES: usize = 16384;
const LIMITS: [u8; 10] = [1, 1, 1, 1, 1, 2, 2, 3, 12, 2];
const _: [(); 56] = [(); mem::size_of::<windows_sys::Win32::Security::TOKEN_STATISTICS>()];
const QUERY: usize = 1;
const PURPOSE: usize = 2;
const THREAD: usize = 0;
const SECONDS: u64 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Stage {
    Admission, Module, Export, ProcessId, ProcessTimes, ThreadOpen, QueryOpen,
    QueryAccess, StatisticsInitial, User, Integrity, Session, Elevation,
    ElevationType, Restrictions, PrivilegeName, Privileges, PurposeOpen,
    PurposeAccess, StatisticsFinal, ThreadClose, PurposeClose, QueryClose,
    Delivery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum FailureKind {
    Admission, Win32, NtStatus, Unsupported, InvalidOutput, InvalidBuffer,
    Inconsistent, Deadline, Budget, Unwind, Delivery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct Failure {
    stage: Stage,
    kind: FailureKind,
    win32: Option<u32>,
    ntstatus: Option<i32>,
    returned: Option<u32>,
}
impl Failure {
    fn new(stage: Stage, kind: FailureKind) -> Self {
        Self { stage, kind, win32: None, ntstatus: None, returned: None }
    }
    fn win32(stage: Stage, code: u32) -> Self {
        Self { win32: Some(code), ..Self::new(stage, FailureKind::Win32) }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Phase { Unattempted, Reserved, Running, KnownCallerPrerequisitesOnly, KnownIneligible, Unknown }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Closure { NotAllocated, Allocating, Rejected, Held, Uncertain, CloseAttempted, ClosedVerified, CloseFailed }
#[derive(Clone, Copy, Debug, Serialize)]
struct Opening {
    purpose_mask: u32,
    requested_mask: Option<u32>,
    native_success: Option<bool>,
    canonical_output: Option<bool>,
    open_error: Option<u32>,
    actual_granted_access: Option<u32>,
    object_ntstatus: Option<i32>,
    object_returned_bytes: Option<u32>,
    closure: Closure,
    close_error: Option<u32>,
}
impl Opening {
    const fn new(mask: u32) -> Self {
        Self { purpose_mask: mask, requested_mask: None, native_success: None, canonical_output: None,
            open_error: None, actual_granted_access: None, object_ntstatus: None,
            object_returned_bytes: None, closure: Closure::NotAllocated, close_error: None }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct Statistics { token_id: u64, authentication_id: u64, modified_id: u64, token_type: u32 }
#[derive(Clone, Copy, Debug, Serialize)]
struct Privilege {
    name: &'static str,
    present: Option<bool>,
    attributes: Option<u32>,
    enabled: Option<bool>,
}
impl Privilege {
    const fn new(name: &'static str) -> Self {
        Self { name, present: None, attributes: None, enabled: None }
    }
}

/// A finite historical value; not a token, operation permit or cleanup receipt.
/// No Deserialize implementation exists for this value or its original owner.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct CallerPrerequisiteSnapshot {
    scope: &'static str,
    #[serde(rename = "A1_target_token")]
    a1_target_token: &'static str,
    production_qualified: bool,
    atomic_primary_snapshot_proven: bool,
    phase: Phase,
    native_pid: Option<u32>,
    creation_filetime_100ns: Option<u64>,
    no_thread_token: Option<bool>,
    thread: Opening,
    query8: Opening,
    purpose6: Opening,
    initial_statistics: Option<Statistics>,
    final_statistics: Option<Statistics>,
    user_sid_sha256: Option<[u8; 32]>,
    user_sid_bytes: Option<u32>,
    integrity_sid_sha256: Option<[u8; 32]>,
    integrity_sid_bytes: Option<u32>,
    integrity_rid: Option<u32>,
    session_id: Option<u32>,
    elevation: Option<bool>,
    elevation_type: Option<u32>,
    has_restrictions: Option<bool>,
    privileges: [Privilege; 3],
    first_error: Option<Failure>,
    last_stage: Stage,
    call_names: [&'static str; 10],
    call_counts: [u8; 10],
    call_limits: [u8; 10],
}
const _: () = assert!(mem::size_of::<CallerPrerequisiteSnapshot>() <= 4096);
impl CallerPrerequisiteSnapshot {
    fn initial() -> Self {
        Self {
            scope: "ORIGINAL_HARNESS_CALLER_PREREQUISITES_ONLY",
            a1_target_token: "NOT_PROVEN", production_qualified: false,
            atomic_primary_snapshot_proven: false, phase: Phase::Unattempted,
            native_pid: None, creation_filetime_100ns: None, no_thread_token: None,
            thread: Opening::new(8), query8: Opening::new(8), purpose6: Opening::new(6),
            initial_statistics: None, final_statistics: None, user_sid_sha256: None,
            user_sid_bytes: None, integrity_sid_sha256: None, integrity_sid_bytes: None,
            integrity_rid: None, session_id: None, elevation: None, elevation_type: None,
            has_restrictions: None,
            privileges: [Privilege::new("SeIncreaseQuotaPrivilege"),
                Privilege::new("SeAssignPrimaryTokenPrivilege"),
                Privilege::new("SeImpersonatePrivilege")],
            first_error: None, last_stage: Stage::Admission,
            call_names: ["GetModuleHandleW", "GetProcAddress", "GetCurrentProcessId",
                "GetProcessTimes", "OpenThreadToken", "OpenProcessToken", "NtQueryObject",
                "LookupPrivilegeValueW", "GetTokenInformation", "CloseHandle"],
            call_counts: [0; 10], call_limits: LIMITS,
        }
    }
    /// Writes only into caller-preallocated storage. Cursor never grows it.
    pub fn write_json(&self, storage: &mut [u8]) -> Result<usize, &'static str> {
        let cap = storage.len().min(CALLER_PREREQUISITE_OUTPUT_BYTES);
        let mut cursor = Cursor::new(&mut storage[..cap]);
        serde_json::to_writer(&mut cursor, self).map_err(|_| "CALLER046_OUTPUT_UNAVAILABLE")?;
        Ok(cursor.position() as usize)
    }
}

struct Slot { raw: HANDLE }
impl Slot { const fn new() -> Self { Self { raw: 0 } } }

/// One boxed, acquiring-thread attempt, installed before any token allocation.
/// All methods are selector-free. Observations cannot reconstruct this owner.
pub struct CallerPrerequisiteAttempt {
    acquiring_thread: ThreadId,
    anchor: usize,
    deadline: Option<Instant>,
    slots: [Slot; 3],
    buffer: Box<[u64; BUFFER_BYTES / 8]>,
    snapshot: CallerPrerequisiteSnapshot,
}
impl CallerPrerequisiteAttempt {
    pub fn new() -> Box<Self> {
        let mut result = Box::new(Self {
            acquiring_thread: thread::current().id(), anchor: 0, deadline: None,
            slots: [Slot::new(), Slot::new(), Slot::new()],
            buffer: Box::new([0; BUFFER_BYTES / 8]),
            snapshot: CallerPrerequisiteSnapshot::initial(),
        });
        result.anchor = (&*result as *const Self) as usize;
        result
    }
    fn same_original(&self) -> bool {
        self.anchor == (self as *const Self) as usize
            && self.acquiring_thread == thread::current().id()
    }
    pub fn reserve(&mut self) -> bool {
        if !self.same_original() || self.snapshot.phase != Phase::Unattempted {
            self.fail(Failure::new(Stage::Admission, FailureKind::Admission));
            return false;
        }
        self.snapshot.phase = Phase::Reserved;
        true
    }
    pub fn snapshot(&self) -> CallerPrerequisiteSnapshot { self.snapshot }
    pub fn unresolved(&self) -> bool {
        self.snapshot.phase != Phase::Unattempted && !self.known_closed()
    }
    pub fn known_closed(&self) -> bool {
        matches!(self.snapshot.phase, Phase::KnownCallerPrerequisitesOnly | Phase::KnownIneligible)
            && self.snapshot.first_error.is_none()
            && self.slots.iter().all(|slot| slot.raw == 0)
            && [self.snapshot.thread, self.snapshot.query8, self.snapshot.purpose6].iter()
                .all(|s| matches!(s.closure, Closure::NotAllocated | Closure::Rejected | Closure::ClosedVerified))
    }
    pub fn retain_admission_failure(&mut self) {
        self.fail(Failure::new(Stage::Admission, FailureKind::Admission));
    }
    /// Journal/input failure never replaces an earlier native first error.
    pub fn retain_delivery_failure(&mut self, raw_os_error: Option<i32>) {
        self.fail(Failure { win32: raw_os_error.map(|value| value as u32),
            ..Failure::new(Stage::Delivery, FailureKind::Delivery) });
    }
    pub fn observe_once(&mut self) -> CallerPrerequisiteSnapshot {
        self.observe_with(&mut Native::new())
    }
    fn observe_with<B: Backend>(&mut self, backend: &mut B) -> CallerPrerequisiteSnapshot {
        if !self.same_original() || self.snapshot.phase != Phase::Reserved {
            self.fail(Failure::new(Stage::Admission, FailureKind::Admission));
            return self.snapshot;
        }
        self.snapshot.phase = Phase::Running;
        self.deadline = backend.now().checked_add(Duration::from_secs(SECONDS));
        if self.deadline.is_none() {
            self.fail(Failure::new(Stage::Admission, FailureKind::Deadline));
            return self.snapshot;
        }
        // No original is moved out, even if a native adapter/decoder unwinds.
        match catch_unwind(AssertUnwindSafe(|| self.run(backend))) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => self.fail(error),
            Err(_) => self.fail(Failure::new(self.snapshot.last_stage, FailureKind::Unwind)),
        }
        self.snapshot
    }
    fn fail(&mut self, error: Failure) {
        if self.snapshot.first_error.is_none() { self.snapshot.first_error = Some(error); }
        self.snapshot.phase = Phase::Unknown;
    }
    fn before<B: Backend>(&mut self, b: &B, call: usize, stage: Stage) -> Result<(), Failure> {
        self.snapshot.last_stage = stage;
        if self.snapshot.first_error.is_some() {
            return Err(self.snapshot.first_error.unwrap());
        }
        if self.deadline.is_none_or(|deadline| b.now() >= deadline) {
            return Err(Failure::new(stage, FailureKind::Deadline));
        }
        if self.snapshot.call_counts[call] >= LIMITS[call] {
            return Err(Failure::new(stage, FailureKind::Budget));
        }
        self.snapshot.call_counts[call] += 1;
        Ok(())
    }
    fn after<B: Backend, T>(&mut self, b: &B, stage: Stage, result: Result<T, Failure>) -> Result<T, Failure> {
        if let Err(error) = &result { self.fail(*error); }
        // Native rejection is latched first even when the same call ran late.
        if self.deadline.is_none_or(|deadline| b.now() >= deadline) {
            self.fail(Failure::new(stage, FailureKind::Deadline));
        }
        if let Some(error) = self.snapshot.first_error { return Err(error); }
        result
    }
    fn opening(&mut self, index: usize) -> &mut Opening {
        match index { THREAD => &mut self.snapshot.thread,
            QUERY => &mut self.snapshot.query8, _ => &mut self.snapshot.purpose6 }
    }
    fn open<B: Backend>(&mut self, b: &mut B, index: usize, stage: Stage) -> Result<bool, Failure> {
        let mask = self.opening(index).purpose_mask;
        self.before(b, if index == THREAD { 4 } else { 5 }, stage)?;
        self.opening(index).closure = Closure::Allocating;
        self.opening(index).requested_mask = Some(mask);
        // Output storage belongs to the preinstalled slot DURING the call.
        let reply = b.open(index == THREAD, mask, &mut self.slots[index].raw);
        let raw = self.slots[index].raw;
        let canonical = if reply.ok { raw != 0 && raw != -1 } else { raw == 0 };
        {
            let observed = self.opening(index);
            observed.native_success = Some(reply.ok);
            observed.canonical_output = Some(canonical);
            observed.open_error = (!reply.ok).then_some(reply.error);
            observed.closure = match (reply.ok, canonical) {
                (true, true) => Closure::Held,
                (false, true) => Closure::Rejected,
                _ => Closure::Uncertain,
            };
        }
        let result = if !canonical {
            Err(Failure { win32: (!reply.ok).then_some(reply.error),
                ..Failure::new(stage, FailureKind::InvalidOutput) })
        } else if reply.ok { Ok(true) }
        else if index == THREAD && reply.error == 1008 { Ok(false) }
        else { Err(Failure::win32(stage, reply.error)) };
        self.after(b, stage, result)
    }
    fn access<B: Backend>(&mut self, b: &mut B, index: usize, stage: Stage) -> Result<u32, Failure> {
        self.before(b, 6, stage)?;
        let mut basic = [0u32; 14];
        let reply = b.object(self.slots[index].raw, &mut basic);
        let observed = self.opening(index);
        observed.object_ntstatus = Some(reply.status);
        observed.object_returned_bytes = Some(reply.returned);
        let result = if reply.status != 0 {
            Err(Failure { ntstatus: Some(reply.status), returned: Some(reply.returned),
                ..Failure::new(stage, FailureKind::NtStatus) })
        } else if reply.returned != 56 {
            Err(Failure { returned: Some(reply.returned),
                ..Failure::new(stage, FailureKind::InvalidBuffer) })
        } else {
            observed.actual_granted_access = Some(basic[1]);
            Ok(basic[1])
        };
        self.after(b, stage, result)
    }
    fn information<B: Backend>(&mut self, b: &mut B, class: TOKEN_INFORMATION_CLASS,
        stage: Stage, fixed: Option<u32>) -> Result<usize, Failure> {
        let capacity = if let Some(capacity) = fixed { capacity } else {
            self.before(b, 8, stage)?;
            let probe = b.token(self.slots[QUERY].raw, class, ptr::null_mut(), 0);
            let result = if !probe.ok && probe.error == 122 && probe.returned > 0
                && probe.returned <= BUFFER_BYTES as u32 { Ok(probe.returned) }
            else { Err(Failure { win32: (!probe.ok).then_some(probe.error),
                returned: Some(probe.returned), ..Failure::new(stage,
                    if !probe.ok && probe.error != 122 { FailureKind::Win32 } else { FailureKind::InvalidBuffer }) }) };
            self.after(b, stage, result)?
        };
        self.buffer.fill(0);
        self.before(b, 8, stage)?;
        let reply = b.token(self.slots[QUERY].raw, class, self.buffer.as_mut_ptr().cast(), capacity);
        let result = if !reply.ok {
            Err(Failure { returned: Some(reply.returned), ..Failure::win32(stage, reply.error) })
        } else if reply.returned == 0 || reply.returned > capacity {
            Err(Failure { returned: Some(reply.returned), ..Failure::new(stage, FailureKind::InvalidBuffer) })
        } else { Ok(reply.returned as usize) };
        self.after(b, stage, result)
    }
    fn bytes(&self, length: usize) -> &[u8] {
        // length is validated by information; underlying storage is always live/aligned.
        unsafe { std::slice::from_raw_parts(self.buffer.as_ptr().cast(), length) }
    }
    fn scalar<B: Backend>(&mut self, b: &mut B, class: TOKEN_INFORMATION_CLASS, stage: Stage) -> Result<u32, Failure> {
        let len = self.information(b, class, stage, Some(4))?;
        if len != 4 { return Err(Failure { returned: Some(len as u32),
            ..Failure::new(stage, FailureKind::InvalidBuffer) }); }
        Ok(u32::from_le_bytes(self.bytes(len).try_into().unwrap()))
    }
    fn statistics<B: Backend>(&mut self, b: &mut B, stage: Stage) -> Result<Statistics, Failure> {
        let len = self.information(b, TokenStatistics, stage, Some(56))?;
        parse_statistics(self.bytes(len)).ok_or(Failure::new(stage, FailureKind::InvalidBuffer))
    }
    fn close<B: Backend>(&mut self, b: &mut B, index: usize, stage: Stage) -> Result<(), Failure> {
        if self.opening(index).closure != Closure::Held {
            return Err(Failure::new(stage, FailureKind::InvalidOutput));
        }
        self.before(b, 9, stage)?;
        self.opening(index).closure = Closure::CloseAttempted;
        let reply = b.close(self.slots[index].raw);
        // Capture actual closure BEFORE checking the after-call deadline.
        if reply.ok {
            self.slots[index].raw = 0;
            self.opening(index).closure = Closure::ClosedVerified;
        } else {
            self.opening(index).closure = Closure::CloseFailed;
            self.opening(index).close_error = Some(reply.error);
        }
        self.after(b, stage, if reply.ok { Ok(()) } else { Err(Failure::win32(stage, reply.error)) })
    }
    fn complete<B: Backend>(&mut self, b: &mut B, eligible: bool) -> Result<(), Failure> {
        if self.snapshot.thread.closure == Closure::Held {
            self.close(b, THREAD, Stage::ThreadClose)?;
        } else {
            if self.snapshot.purpose6.closure == Closure::Held { self.close(b, PURPOSE, Stage::PurposeClose)?; }
            if self.snapshot.query8.closure == Closure::Held { self.close(b, QUERY, Stage::QueryClose)?; }
        }
        self.snapshot.phase = if eligible { Phase::KnownCallerPrerequisitesOnly } else { Phase::KnownIneligible };
        Ok(())
    }
    fn run<B: Backend>(&mut self, b: &mut B) -> Result<(), Failure> {
        self.before(b, 0, Stage::Module)?;
        let result = b.module().map_err(|code| Failure::win32(Stage::Module, code));
        self.after(b, Stage::Module, result)?;
        self.before(b, 1, Stage::Export)?;
        let result = b.export().map_err(|code| Failure { win32: Some(code),
            ..Failure::new(Stage::Export, FailureKind::Unsupported) });
        self.after(b, Stage::Export, result)?;
        self.before(b, 2, Stage::ProcessId)?;
        let pid = b.pid();
        self.snapshot.native_pid = Some(pid);
        self.after(b, Stage::ProcessId, if pid != 0 { Ok(()) } else {
            Err(Failure::new(Stage::ProcessId, FailureKind::InvalidOutput)) })?;
        self.before(b, 3, Stage::ProcessTimes)?;
        let result = b.creation().map_err(|code| Failure::win32(Stage::ProcessTimes, code));
        if let Ok(value) = result { self.snapshot.creation_filetime_100ns = Some(value); }
        let creation = self.after(b, Stage::ProcessTimes, result)?;
        if creation == 0 { return Err(Failure::new(Stage::ProcessTimes, FailureKind::InvalidOutput)); }
        let thread_present = self.open(b, THREAD, Stage::ThreadOpen)?;
        self.snapshot.no_thread_token = Some(!thread_present);
        if thread_present { return self.complete(b, false); }
        self.open(b, QUERY, Stage::QueryOpen)?;
        if self.access(b, QUERY, Stage::QueryAccess)? & 8 != 8 {
            return self.complete(b, false);
        }
        let initial = self.statistics(b, Stage::StatisticsInitial)?;
        self.snapshot.initial_statistics = Some(initial);
        let len = self.information(b, TokenUser, Stage::User, None)?;
        let sid = token_sid(self.bytes(len), mem::size_of::<TOKEN_USER>())
            .ok_or(Failure::new(Stage::User, FailureKind::InvalidBuffer))?;
        let fingerprint = Sha256::digest(sid).into();
        let sid_bytes = sid.len() as u32;
        self.snapshot.user_sid_sha256 = Some(fingerprint);
        self.snapshot.user_sid_bytes = Some(sid_bytes);
        let len = self.information(b, TokenIntegrityLevel, Stage::Integrity, None)?;
        let sid = token_sid(self.bytes(len), mem::size_of::<TOKEN_MANDATORY_LABEL>())
            .ok_or(Failure::new(Stage::Integrity, FailureKind::InvalidBuffer))?;
        if sid.len() != 12 || sid[2..8] != [0, 0, 0, 0, 0, 16] {
            return Err(Failure::new(Stage::Integrity, FailureKind::InvalidBuffer));
        }
        let fingerprint = Sha256::digest(sid).into();
        let rid = u32::from_le_bytes(sid[8..12].try_into().unwrap());
        self.snapshot.integrity_sid_sha256 = Some(fingerprint);
        self.snapshot.integrity_sid_bytes = Some(12);
        self.snapshot.integrity_rid = Some(rid);
        self.snapshot.session_id = Some(self.scalar(b, TokenSessionId, Stage::Session)?);
        let elevation = self.scalar(b, TokenElevation, Stage::Elevation)?;
        self.snapshot.elevation = Some(canonical_bool(elevation).ok_or(
            Failure::new(Stage::Elevation, FailureKind::InvalidBuffer))?);
        let elevation_type = self.scalar(b, TokenElevationType, Stage::ElevationType)?;
        if !(1..=3).contains(&elevation_type) {
            return Err(Failure::new(Stage::ElevationType, FailureKind::InvalidBuffer));
        }
        self.snapshot.elevation_type = Some(elevation_type);
        let len = self.information(b, TokenHasRestrictions, Stage::Restrictions, Some(4))?;
        self.snapshot.has_restrictions = Some(restrictions(self.bytes(len))
            .ok_or(Failure::new(Stage::Restrictions, FailureKind::InvalidBuffer))?);
        let mut wanted = [0u64; 3];
        for (index, value) in wanted.iter_mut().enumerate() {
            self.before(b, 7, Stage::PrivilegeName)?;
            let result = b.privilege(index).map_err(|code| Failure::win32(Stage::PrivilegeName, code));
            *value = self.after(b, Stage::PrivilegeName, result)?;
        }
        if wanted[0] == wanted[1] || wanted[0] == wanted[2] || wanted[1] == wanted[2] {
            return Err(Failure::new(Stage::PrivilegeName, FailureKind::InvalidOutput));
        }
        let len = self.information(b, TokenPrivileges, Stage::Privileges, None)?;
        let facts = parse_privileges(self.bytes(len), wanted)
            .ok_or(Failure::new(Stage::Privileges, FailureKind::InvalidBuffer))?;
        for (index, attributes) in facts.into_iter().enumerate() {
            self.snapshot.privileges[index].present = Some(attributes.is_some());
            self.snapshot.privileges[index].attributes = attributes;
            self.snapshot.privileges[index].enabled = Some(attributes.is_some_and(privilege_enabled));
        }
        self.open(b, PURPOSE, Stage::PurposeOpen)?;
        let purpose_grant = self.access(b, PURPOSE, Stage::PurposeAccess)?;
        let final_statistics = self.statistics(b, Stage::StatisticsFinal)?;
        self.snapshot.final_statistics = Some(final_statistics);
        if initial != final_statistics {
            return Err(Failure::new(Stage::StatisticsFinal, FailureKind::Inconsistent));
        }
        let eligible = initial.token_type == 1 && purpose_grant & 6 == 6
            && self.snapshot.privileges.iter().all(|p| p.enabled == Some(true));
        self.complete(b, eligible)
    }
}
// Intentionally NO Drop implementation: not even a best-effort native close.

fn canonical_bool(value: u32) -> Option<bool> {
    match value { 0 => Some(false), 1 => Some(true), _ => None }
}
fn restrictions(bytes: &[u8]) -> Option<bool> {
    match bytes.len() {
        1 => canonical_bool(u32::from(bytes[0])),
        4 => canonical_bool(u32::from_le_bytes(bytes.try_into().ok()?)),
        _ => None,
    }
}
fn parse_statistics(bytes: &[u8]) -> Option<Statistics> {
    if bytes.len() != 56 { return None; }
    let token_type = u32::from_le_bytes(bytes[24..28].try_into().ok()?);
    if !(1..=2).contains(&token_type) { return None; }
    let u64_at = |offset: usize| -> Option<u64> { Some(u64::from_le_bytes(bytes.get(offset..offset + 8)?.try_into().ok()?)) };
    Some(Statistics { token_id: u64_at(0)?, authentication_id: u64_at(8)?,
        modified_id: u64_at(48)?, token_type })
}
fn token_sid(bytes: &[u8], minimum: usize) -> Option<&[u8]> {
    if bytes.len() < minimum || bytes.len() < mem::size_of::<usize>() { return None; }
    let pointer = usize::from_le_bytes(bytes[..mem::size_of::<usize>()].try_into().ok()?);
    let start = pointer.checked_sub(bytes.as_ptr() as usize)?;
    if start < minimum || start % 4 != 0 { return None; }
    let head = bytes.get(start..start.checked_add(8)?)?;
    if head[0] != 1 || head[1] > 15 { return None; }
    bytes.get(start..start.checked_add(8 + usize::from(head[1]) * 4)?)
}
fn privilege_enabled(attributes: u32) -> bool { attributes & 2 != 0 && attributes & 4 == 0 }
fn parse_privileges(bytes: &[u8], wanted: [u64; 3]) -> Option<[Option<u32>; 3]> {
    let count = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    if count > 256 || 4usize.checked_add(count.checked_mul(12)?)? > bytes.len() { return None; }
    let entry = |index: usize| -> Option<(u64, u32)> {
        let offset = 4 + index * 12;
        Some((u64::from_le_bytes(bytes.get(offset..offset + 8)?.try_into().ok()?),
            u32::from_le_bytes(bytes.get(offset + 8..offset + 12)?.try_into().ok()?)))
    };
    let mut out = [None; 3];
    for i in 0..count {
        let (luid, attributes) = entry(i)?;
        for j in 0..i { if entry(j)?.0 == luid { return None; } }
        for (index, wanted) in wanted.iter().enumerate() {
            if *wanted == luid { out[index] = Some(attributes); }
        }
    }
    Some(out)
}

#[derive(Clone, Copy)]
struct WinReply { ok: bool, error: u32 }
struct TokenReply { ok: bool, error: u32, returned: u32 }
struct ObjectReply { status: i32, returned: u32 }
// Private adapter only. Tests script this same algorithm; no public provider.
trait Backend {
    fn now(&self) -> Instant;
    fn module(&mut self) -> Result<(), u32>;
    fn export(&mut self) -> Result<(), u32>;
    fn pid(&mut self) -> u32;
    fn creation(&mut self) -> Result<u64, u32>;
    fn open(&mut self, thread: bool, mask: u32, output: &mut HANDLE) -> WinReply;
    fn object(&mut self, token: HANDLE, output: &mut [u32; 14]) -> ObjectReply;
    fn token(&mut self, token: HANDLE, class: TOKEN_INFORMATION_CLASS, output: *mut c_void, capacity: u32) -> TokenReply;
    fn privilege(&mut self, index: usize) -> Result<u64, u32>;
    fn close(&mut self, token: HANDLE) -> WinReply;
}
type QueryObject = unsafe extern "system" fn(HANDLE, u32, *mut c_void, u32, *mut u32) -> i32;
struct Native { borrowed_module: isize, query: Option<QueryObject> }
impl Native { fn new() -> Self { Self { borrowed_module: 0, query: None } } }
fn win_reply(ok: i32) -> WinReply {
    WinReply { ok: ok != 0, error: if ok == 0 { unsafe { GetLastError() } } else { 0 } }
}
impl Backend for Native {
    fn now(&self) -> Instant { Instant::now() }
    fn module(&mut self) -> Result<(), u32> {
        const NTDLL: &[u16] = &[110, 116, 100, 108, 108, 46, 100, 108, 108, 0];
        self.borrowed_module = unsafe { GetModuleHandleW(NTDLL.as_ptr()) };
        if self.borrowed_module == 0 { Err(unsafe { GetLastError() }) } else { Ok(()) }
    }
    fn export(&mut self) -> Result<(), u32> {
        let address = unsafe { GetProcAddress(self.borrowed_module, b"NtQueryObject\0".as_ptr()) };
        match address {
            Some(address) => { self.query = Some(unsafe { mem::transmute::<unsafe extern "system" fn() -> isize, QueryObject>(address) }); Ok(()) }
            None => Err(unsafe { GetLastError() }),
        }
    }
    fn pid(&mut self) -> u32 { unsafe { GetCurrentProcessId() } }
    fn creation(&mut self) -> Result<u64, u32> {
        let mut created: FILETIME = unsafe { mem::zeroed() };
        let mut ended = created; let mut kernel = created; let mut user = created;
        let reply = win_reply(unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut ended, &mut kernel, &mut user) });
        if reply.ok { Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime)) }
        else { Err(reply.error) }
    }
    fn open(&mut self, thread: bool, mask: u32, output: &mut HANDLE) -> WinReply {
        win_reply(unsafe {
            if thread { OpenThreadToken(GetCurrentThread(), mask, 1, output) }
            else { OpenProcessToken(GetCurrentProcess(), mask, output) }
        })
    }
    fn object(&mut self, token: HANDLE, output: &mut [u32; 14]) -> ObjectReply {
        let mut returned = 0;
        // Export was resolved before any token allocation, never a fallback.
        let status = unsafe { self.query.unwrap()(token, 0, output.as_mut_ptr().cast(), 56, &mut returned) };
        ObjectReply { status, returned }
    }
    fn token(&mut self, token: HANDLE, class: TOKEN_INFORMATION_CLASS, output: *mut c_void, capacity: u32) -> TokenReply {
        let mut returned = 0;
        let reply = win_reply(unsafe { GetTokenInformation(token, class, output, capacity, &mut returned) });
        TokenReply { ok: reply.ok, error: reply.error, returned }
    }
    fn privilege(&mut self, index: usize) -> Result<u64, u32> {
        const NAMES: [&[u16]; 3] = [
            &[83,101,73,110,99,114,101,97,115,101,81,117,111,116,97,80,114,105,118,105,108,101,103,101,0],
            &[83,101,65,115,115,105,103,110,80,114,105,109,97,114,121,84,111,107,101,110,80,114,105,118,105,108,101,103,101,0],
            &[83,101,73,109,112,101,114,115,111,110,97,116,101,80,114,105,118,105,108,101,103,101,0],
        ];
        let mut luid: LUID = unsafe { mem::zeroed() };
        let reply = win_reply(unsafe { LookupPrivilegeValueW(ptr::null(), NAMES[index].as_ptr(), &mut luid) });
        if reply.ok { Ok((u64::from(luid.HighPart as u32) << 32) | u64::from(luid.LowPart)) }
        else { Err(reply.error) }
    }
    fn close(&mut self, token: HANDLE) -> WinReply { win_reply(unsafe { CloseHandle(token) }) }
}

#[cfg(test)]
#[path = "caller_prerequisites_tests.rs"]
mod tests;

/// AsUser-only consumed caller observation. Never serialized or constructed
/// from CallerPrerequisiteSnapshot; Source046's all-three-enabled predicate is
/// unchanged. API temporary enable of already-present privileges is confined to
/// the distinct Source047 contract, not reported as Source046 eligibility.
/// Consume-only proof that THIS original caller observation completed with a
/// missing required privilege and checked-closed its own token handles. It says
/// nothing about runner, registry, factory, account or setup retirement.
/// No public/scalar constructor, Clone, Deserialize or native Drop is provided.
pub(crate) struct ClosedIneligibleCaller { original: Box<CallerPrerequisiteAttempt> }
impl ClosedIneligibleCaller {
    #[cfg(test)]
    pub(crate) fn snapshot(&self)->CallerPrerequisiteSnapshot {self.original.snapshot()}
    pub(crate) fn original_handles_closed(&self) -> bool {
        self.original.anchor == (&*self.original as *const CallerPrerequisiteAttempt) as usize
            && self.original.known_closed()
    }
}
impl CallerPrerequisiteAttempt {
    pub(crate) fn consume_closed_ineligible(self:Box<Self>)
        -> Result<ClosedIneligibleCaller,Box<Self>> {
        let s=&self.snapshot;
        // Expiry, uncertain observations, removed privileges and foreign-thread
        // use cannot be relabeled as a clean missing-privilege refusal.
        let complete=self.same_original() && self.known_closed()
            && s.no_thread_token==Some(true)
            && s.native_pid.is_some_and(|v|v!=0)
            && s.creation_filetime_100ns.is_some_and(|v|v!=0)
            && s.initial_statistics==s.final_statistics
            && s.initial_statistics.is_some_and(|v|v.token_type==1)
            && s.query8.actual_granted_access.is_some_and(|v|v&8==8)
            && s.purpose6.actual_granted_access.is_some_and(|v|v&6==6)
            && s.privileges[..2].iter().all(|p|match p.present {
                Some(false)=>p.attributes.is_none() && p.enabled==Some(false),
                Some(true)=>p.attributes.is_some_and(|v|v&4==0) && p.enabled.is_some(),
                None=>false,
            })
            && s.privileges[..2].iter().any(|p|p.present==Some(false))
            && self.deadline.is_some_and(|deadline|Instant::now()<deadline);
        if complete {Ok(ClosedIneligibleCaller{original:self})}else{Err(self)}
    }
}
pub(crate) struct AsUserCaller {original:Box<CallerPrerequisiteAttempt>}
impl CallerPrerequisiteAttempt {
    pub(crate) fn consume_for_asuser(self:Box<Self>)->Result<AsUserCaller,Box<Self>> {
        if self.asuser_complete() {Ok(AsUserCaller{original:self})}else{Err(self)}
    }
    fn asuser_complete(&self)->bool {
        let s=&self.snapshot;
        let complete=self.same_original()&&self.known_closed()&&s.no_thread_token==Some(true)
            &&s.native_pid.is_some_and(|v|v!=0)&&s.creation_filetime_100ns.is_some_and(|v|v!=0)
            &&s.initial_statistics==s.final_statistics
            &&s.initial_statistics.is_some_and(|v|v.token_type==1)
            &&s.purpose6.actual_granted_access.is_some_and(|v|v&6==6)
            &&s.query8.actual_granted_access.is_some_and(|v|v&8==8)
            &&s.privileges[..2].iter().all(|p|p.present==Some(true)
                &&p.attributes.is_some_and(|v|v&4==0))
            &&self.deadline.is_some_and(|deadline|Instant::now()<deadline);
        complete
    }
    pub(crate) fn require_asuser_preflight(&self,pid:u32,creation:u64,sid:&[u8],auth:u64,session:u32,token:u64)->std::io::Result<()> {
        let s=&self.snapshot;
        if !self.asuser_complete()||s.native_pid!=Some(pid)||s.creation_filetime_100ns!=Some(creation)
            ||s.user_sid_sha256!=Some(Sha256::digest(sid).into())||s.session_id!=Some(session)
            ||s.integrity_rid!=Some(0x3000)||s.initial_statistics.is_none_or(|v|v.authentication_id!=auth||v.token_id!=token) {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied,"A1 original C prerequisites absent before setup"));
        }Ok(())
    }
}
impl AsUserCaller {
    pub(crate) fn original_handles_closed(&self)->bool {
        self.original.anchor == (&*self.original as *const CallerPrerequisiteAttempt) as usize
            && self.original.known_closed()
    }
    pub(crate) fn pid(&self)->u32 {self.original.snapshot.native_pid.unwrap_or(0)}
    pub(crate) fn creation(&self)->u64 {self.original.snapshot.creation_filetime_100ns.unwrap_or(0)}
    pub(crate) fn require_original(&self,pid:u32,creation:u64)->std::io::Result<()> {
        if !self.original.same_original()||pid!=self.pid()||creation!=self.creation()||!self.original.known_closed()
            ||self.original.deadline.is_none_or(|deadline|Instant::now()>=deadline) {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied,"A1 caller observation expired or wrong original"));
        }
        Ok(())
    }
}
#[cfg(test)]
mod asuser_policy_tests {
    use super::*;
    fn closed()->Box<CallerPrerequisiteAttempt> {
        let mut a=CallerPrerequisiteAttempt::new();
        a.snapshot.phase=Phase::KnownIneligible;
        a.snapshot.no_thread_token=Some(true);a.snapshot.native_pid=Some(11);
        a.snapshot.creation_filetime_100ns=Some(22);
        a.snapshot.initial_statistics=Some(Statistics{token_id:1,authentication_id:2,modified_id:3,token_type:1});
        a.snapshot.final_statistics=a.snapshot.initial_statistics;
        a.snapshot.query8.actual_granted_access=Some(8);
        a.snapshot.purpose6.actual_granted_access=Some(6);
        for p in &mut a.snapshot.privileges[..2] {p.present=Some(true);p.attributes=Some(0);p.enabled=Some(false);}
        a.deadline=Some(Instant::now()+Duration::from_secs(1));a
    }
    fn missing_assign()->Box<CallerPrerequisiteAttempt> {
        let mut a=closed();
        a.snapshot.privileges[1].present=Some(false);
        a.snapshot.privileges[1].attributes=None;
        a.snapshot.privileges[1].enabled=Some(false);
        a
    }
    #[test] fn clean_caller_refusal_consumes_exact_closed_observation_without_launch_authority() {
        let original=missing_assign();
        let address=(&*original as *const CallerPrerequisiteAttempt) as usize;
        let calls=original.snapshot.call_counts;
        let proof=match original.consume_closed_ineligible(){Ok(v)=>v,Err(_)=>panic!("closed missing privilege refused")};
        assert_eq!((&*proof.original as *const CallerPrerequisiteAttempt) as usize,address);
        assert_eq!(proof.original.snapshot.call_counts,calls);
        assert!(proof.original_handles_closed());
        assert!(!proof.original.asuser_complete());
        assert!(proof.original.snapshot.privileges[1].present==Some(false));
    }
    #[test] fn clean_caller_refusal_never_accepts_unknown_live_handle_or_failed_close() {
        for fault in 0..5 {
            let mut a=missing_assign();
            match fault {
                0=>a.snapshot.phase=Phase::Unknown,
                1=>a.slots[QUERY].raw=7,
                2=>a.snapshot.query8.closure=Closure::CloseFailed,
                3=>a.snapshot.first_error=Some(Failure::new(Stage::QueryClose,FailureKind::Win32)),
                _=>a.snapshot.purpose6.closure=Closure::Uncertain,
            }
            let original=(&*a as *const CallerPrerequisiteAttempt) as usize;
            let kept=match a.consume_closed_ineligible(){Ok(_)=>panic!("uncertainty minted refusal"),Err(v)=>v};
            assert_eq!((&*kept as *const CallerPrerequisiteAttempt) as usize,original);
        }
    }
    #[test] fn clean_caller_refusal_requires_complete_timely_missing_privilege_evidence() {
        for fault in 0..9 {
            let mut a=missing_assign();
            match fault {
                0=>a.deadline=Some(Instant::now()),
                1=>a.snapshot.privileges[1].present=None,
                2=>a.snapshot.privileges[1].attributes=Some(0),
                3=>a.snapshot.privileges[0].attributes=Some(4),
                4=>a.snapshot.no_thread_token=None,
                5=>a.snapshot.final_statistics.as_mut().unwrap().modified_id+=1,
                6=>a.snapshot.query8.actual_granted_access=None,
                7=>a.snapshot.native_pid=Some(0),
                _=>a.snapshot.creation_filetime_100ns=Some(0),
            }
            assert!(a.consume_closed_ineligible().is_err());
        }
        assert!(closed().consume_closed_ineligible().is_err());
    }
    #[test] fn clean_caller_refusal_cannot_be_minted_by_a_foreign_thread() {
        let original=missing_assign();
        let original=std::thread::spawn(move||match original.consume_closed_ineligible(){
            Ok(_)=>panic!("foreign thread minted refusal"),Err(original)=>original,
        }).join().unwrap();
        assert!(original.consume_closed_ineligible().is_ok());
    }
    #[test]
    #[ignore = "explicit read-only Windows caller fixture requires a primary token missing an AsUser privilege"]
    fn readonly_native_missing_privilege_retains_seventeen_closed_originals() {
        assert_eq!(std::env::var("MORROW_CALLER_REFUSAL_NATIVE_TEST").as_deref(),Ok("1"));
        let destination=std::env::var_os("MORROW_CALLER_REFUSAL_NATIVE_REPORT")
            .expect("fresh local report path required");
        let mut report=std::fs::OpenOptions::new().write(true).create_new(true)
            .open(destination).expect("fresh report already exists or is unavailable");
        let mut proofs=Vec::new();
        let mut addresses=std::collections::BTreeSet::new();
        let mut observations=Vec::new();
        for _ in 0..17 {
            let mut original=CallerPrerequisiteAttempt::new();
            assert!(original.reserve());
            original.observe_once();
            assert!(original.known_closed(),"native caller observation did not close");
            assert_eq!(original.snapshot.query8.closure,Closure::ClosedVerified);
            assert_eq!(original.snapshot.purpose6.closure,Closure::ClosedVerified);
            assert_eq!(original.snapshot.call_counts[9],2);
            assert!(addresses.insert((&*original as *const CallerPrerequisiteAttempt) as usize));
            let proof=match original.consume_closed_ineligible() {
                Ok(proof)=>proof,
                Err(_)=>panic!("native fixture does not have a complete known missing-privilege refusal"),
            };
            assert!(proof.original_handles_closed());
            observations.push(proof.original.snapshot());
            proofs.push(proof);
        }
        assert_eq!(proofs.len(),17);
        serde_json::to_writer(&mut report,&serde_json::json!({
            "scope":"READONLY_ORIGINAL_CALLER_TOKEN_OBSERVATIONS_ONLY",
            "native_observations":17,"actual_checked_token_closes":34,
            "original_boxes_retained":17,"process_id":std::process::id(),
            "runner_starts":0,"account_or_policy_mutations":0,
            "registry_capacity_qualification":false,"full_P2_accepted":false,
            "observations":observations,
        })).unwrap();
        report.sync_all().unwrap();
    }
    #[test] fn asuser_contract_consumes_actual_closed_attempt_without_changing_046_eligibility() {
        let a=closed();assert_eq!(a.snapshot.phase,Phase::KnownIneligible);
        let proof=match a.consume_for_asuser(){Ok(v)=>v,Err(_)=>panic!("scripted complete observation rejected")};
        assert_eq!(proof.original.snapshot.phase,Phase::KnownIneligible);
        assert!(proof.require_original(11,22).is_ok());assert!(proof.require_original(11,23).is_err());
    }
    #[test] fn assign_absence_unknown_close_and_expiry_cannot_be_consumed_for_asuser() {
        let mut a=closed();a.snapshot.privileges[1].present=Some(false);
        assert!(a.consume_for_asuser().is_err());
        let mut a=closed();a.snapshot.query8.closure=Closure::CloseFailed;
        assert!(a.consume_for_asuser().is_err());
        let mut a=closed();a.deadline=Some(Instant::now());
        assert!(a.consume_for_asuser().is_err());
    }
    #[test] fn consumed_asuser_caller_cannot_authorize_a_different_launch_thread() {
        let a=closed();
        let original=match a.consume_for_asuser(){Ok(value)=>value,Err(_)=>panic!("scripted original rejected")};
        assert!(original.require_original(11,22).is_ok());
        let original=std::thread::spawn(move||{
            assert!(original.require_original(11,22).is_err());
            // Read-only rejection neither changes the original snapshot nor
            // performs token/native calls on the foreign thread.
            assert_eq!(original.original.snapshot.phase,Phase::KnownIneligible);original
        }).join().unwrap();
        assert!(original.require_original(11,22).is_ok());
    }
}

impl AsUserCaller {
    pub(crate) fn integrity_rid(&self)->std::io::Result<u32> {
        self.original.snapshot.integrity_rid.ok_or_else(||std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,"A1 original caller integrity was not observed"))
    }
}

impl AsUserCaller {
    pub(crate) fn require_target_policy_caller(&self,sid:&[u8],authentication:u64,session:u32)->std::io::Result<()> {
        let original=&self.original.snapshot;
        if original.user_sid_sha256!=Some(Sha256::digest(sid).into())
            ||original.initial_statistics.is_none_or(|s|s.authentication_id!=authentication)
            ||original.session_id!=Some(session) {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied,
                "A1 caller SID/LUID/session differ from original consumed observation"));
        }
        Ok(())
    }
}

impl AsUserCaller {
    pub(crate) fn require_primary_identity(&self,token:u64,authentication:u64,sid:&[u8])->std::io::Result<()> {
        let original=&self.original.snapshot;
        if original.initial_statistics.is_none_or(|s|s.token_id!=token||s.authentication_id!=authentication||s.token_type!=1)
            ||original.user_sid_sha256!=Some(Sha256::digest(sid).into()) {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied,
                "A1 actual current primary token differs from original closed caller observation"));
        }
        Ok(())
    }
}
