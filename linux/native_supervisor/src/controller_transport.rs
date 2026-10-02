//! Capability-authenticated inherited anonymous-pipe fixture transport.
//! It is not a public socket server, per-write PID/UID authentication, malicious
//! same-UID isolation, product authority, durable release or a message MAC.
use crate::{
    OriginalController, OwnedProcess, SealedExecutable, SpawnFailure, invalid, kernel_error,
};
use std::{
    collections::VecDeque,
    ffi::OsStr,
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::fs::{FileExt, MetadataExt},
    },
    path::Path,
    time::Duration,
};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, Zeroizing};

const RX_FD: RawFd = 4;
const TX_FD: RawFd = 5;
const BOOT_FD: RawFd = 6;
const CONTROLLER_FD: RawFd = 3;
const BOOT_BYTES: usize = 128;
const FRAME_PREFIX: usize = 80;
pub const MAX_CONTROL_PAYLOAD: usize = 128 * 1024;
const MAX_QUEUE_BYTES: usize = 256 * 1024 + 1024;
const MAX_QUEUED_FRAMES: usize = 8;
const POLL_BYTES: usize = 16 * 1024;
const POLL_ATTEMPTS: usize = 32;
const BOOT_SEALS: i32 = libc::F_SEAL_WRITE
    | libc::F_SEAL_GROW
    | libc::F_SEAL_SHRINK
    | libc::F_SEAL_SEAL
    | libc::F_SEAL_EXEC;

#[derive(Clone, Copy, Debug)]
pub struct ControllerLimits {
    pub heartbeat_timeout: Duration,
    pub frame_timeout: Duration,
}
impl ControllerLimits {
    fn validate(self) -> io::Result<()> {
        if !(100..=5000).contains(&self.heartbeat_timeout.as_millis())
            || !(20..=1000).contains(&self.frame_timeout.as_millis())
            || self.heartbeat_timeout.subsec_nanos() % 1_000_000 != 0
            || self.frame_timeout.subsec_nanos() % 1_000_000 != 0
        {
            return Err(invalid("controller/transport limits"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Data = 1,
    Heartbeat = 2,
    GracefulClose = 3,
    Reply = 4,
    Status = 5,
    Diagnostic = 6,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Controller = 1,
    Supervisor = 2,
}
impl Role {
    fn accepts(self, kind: FrameKind) -> bool {
        match self {
            Self::Controller => matches!(
                kind,
                FrameKind::Reply | FrameKind::Status | FrameKind::Diagnostic
            ),
            Self::Supervisor => matches!(
                kind,
                FrameKind::Data | FrameKind::Heartbeat | FrameKind::GracefulClose
            ),
        }
    }
    fn sends(self, kind: FrameKind) -> bool {
        match self {
            Self::Controller => Role::Supervisor.accepts(kind),
            Self::Supervisor => Role::Controller.accepts(kind),
        }
    }
}
/// This can only be constructed after the session/capability/sequence checks.
/// The transient capability is stripped; no raw authenticated header escapes.
pub struct AuthenticatedFrame {
    kind: FrameKind,
    sequence: u64,
    session: [u8; 32],
    payload: Zeroizing<Vec<u8>>,
}
impl AuthenticatedFrame {
    pub fn kind(&self) -> FrameKind {
        self.kind
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
    pub(crate) fn session(&self) -> &[u8; 32] {
        &self.session
    }
}
pub enum FrameRead {
    Pending,
    Frame(AuthenticatedFrame),
    Eof,
}

struct SecretConfig {
    limits: ControllerLimits,
    session: [u8; 32],
    capability: Zeroizing<[u8; 32]>,
}
pub struct ControllerFixtureLaunch {
    config: SecretConfig,
    read: OwnedFd,
    write: OwnedFd,
}
pub struct SupervisorFixtureLaunch {
    config: SecretConfig,
    read: OwnedFd,
    write: OwnedFd,
}
pub struct BoundSupervisorFixtureLaunch {
    launch: SupervisorFixtureLaunch,
    original: OriginalController,
}
/// Descriptor construction is private: callers cannot feed arbitrary raw fds or
/// numeric PIDs into the original-controller handoff.
pub(crate) struct InheritedLaunch {
    pub(crate) descriptors: Vec<(RawFd, OwnedFd)>,
}

pub fn fixture_channel_pair(
    limits: ControllerLimits,
) -> io::Result<(ControllerFixtureLaunch, SupervisorFixtureLaunch)> {
    limits.validate()?;
    let mut session = [0u8; 32];
    let mut capability = Zeroizing::new([0u8; 32]);
    random(&mut session)?;
    random(&mut *capability)?;
    let to_supervisor = pipe()?;
    let to_controller = pipe()?;
    let controller = ControllerFixtureLaunch {
        config: SecretConfig {
            limits,
            session,
            capability: Zeroizing::new(*capability),
        },
        read: to_controller.0,
        write: to_supervisor.1,
    };
    let supervisor = SupervisorFixtureLaunch {
        config: SecretConfig {
            limits,
            session,
            capability,
        },
        read: to_supervisor.0,
        write: to_controller.1,
    };
    Ok((controller, supervisor))
}
impl ControllerFixtureLaunch {
    pub fn spawn(
        self,
        executable: SealedExecutable,
        cwd: &Path,
        args: &[&OsStr],
    ) -> Result<OwnedProcess, SpawnFailure> {
        let inherited = prepare_launch(
            self.config,
            self.read,
            self.write,
            None,
            Role::Controller,
            executable.artifact_digest(),
        )
        .map_err(SpawnFailure::NotCreated)?;
        OwnedProcess::spawn_with_inherited(executable, cwd, args, inherited)
    }
}
impl SupervisorFixtureLaunch {
    pub fn bind_original(
        self,
        original: OriginalController,
    ) -> io::Result<BoundSupervisorFixtureLaunch> {
        if original.poll_exited()? {
            return Err(invalid("original controller exited before binding"));
        }
        Ok(BoundSupervisorFixtureLaunch {
            launch: self,
            original,
        })
    }
}
impl BoundSupervisorFixtureLaunch {
    pub fn spawn(
        self,
        executable: SealedExecutable,
        cwd: &Path,
        args: &[&OsStr],
    ) -> Result<OwnedProcess, SpawnFailure> {
        let inherited = prepare_launch(
            self.launch.config,
            self.launch.read,
            self.launch.write,
            Some(self.original),
            Role::Supervisor,
            executable.artifact_digest(),
        )
        .map_err(SpawnFailure::NotCreated)?;
        OwnedProcess::spawn_with_inherited(executable, cwd, args, inherited)
    }
}
fn prepare_launch(
    config: SecretConfig,
    read: OwnedFd,
    write: OwnedFd,
    original: Option<OriginalController>,
    role: Role,
    artifact: [u8; 32],
) -> io::Result<InheritedLaunch> {
    let pid = original
        .as_ref()
        .map_or(0, OriginalController::diagnostic_pid);
    let bootstrap = bootstrap_file(&config, role, pid, artifact)?;
    let mut descriptors = vec![
        (RX_FD, duplicate_high(read.as_raw_fd())?),
        (TX_FD, duplicate_high(write.as_raw_fd())?),
        (BOOT_FD, duplicate_high(bootstrap.as_raw_fd())?),
    ];
    if let Some(original) = original {
        descriptors.push((CONTROLLER_FD, duplicate_high(original.fd().as_raw_fd())?));
    }
    Ok(InheritedLaunch { descriptors })
}
fn bootstrap_file(
    config: &SecretConfig,
    role: Role,
    pid: u32,
    artifact: [u8; 32],
) -> io::Result<File> {
    let mut bytes = Zeroizing::new([0u8; BOOT_BYTES]);
    bytes[..8].copy_from_slice(b"MCBOOT01");
    bytes[8] = role as u8;
    bytes[9] = 1;
    bytes[12..16].copy_from_slice(&pid.to_le_bytes());
    bytes[16..20]
        .copy_from_slice(&(config.limits.heartbeat_timeout.as_millis() as u32).to_le_bytes());
    bytes[20..24].copy_from_slice(&(config.limits.frame_timeout.as_millis() as u32).to_le_bytes());
    bytes[24..28].copy_from_slice(&(MAX_CONTROL_PAYLOAD as u32).to_le_bytes());
    bytes[32..64].copy_from_slice(&config.session);
    bytes[64..96].copy_from_slice(&*config.capability);
    bytes[96..128].copy_from_slice(&artifact);
    let fd = unsafe {
        libc::memfd_create(
            c"morrow-controller-bootstrap-v1".as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING | libc::MFD_NOEXEC_SEAL,
        )
    };
    if fd < 0 {
        return Err(kernel_error("memfd_create private controller bootstrap"));
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    file.write_all(&*bytes)?;
    if unsafe { libc::fchmod(fd, 0o400) } < 0
        || unsafe { libc::fcntl(fd, libc::F_ADD_SEALS, BOOT_SEALS) } < 0
    {
        return Err(kernel_error("seal private controller bootstrap"));
    }
    Ok(file)
}
/// Takes fixed role descriptors owned exclusively by this fixture process.
/// # Safety
/// Must be called exactly once after ControllerFixtureLaunch::spawn, before
/// opening/repurposing fds4/5/6 or spawning any child. This is not a raw-fd/PID
/// authority constructor and must not be used as a product bootstrap.
pub unsafe fn take_controller_fixture_transport() -> io::Result<CapabilityTransport> {
    let (transport, original) = unsafe { take_inherited(Role::Controller) }?;
    if original.is_some() {
        return Err(invalid("controller fixture received unexpected watch"));
    }
    Ok(transport)
}
/// # Safety
/// Must be called exactly once after BoundSupervisorFixtureLaunch::spawn. The
/// caller owns fixed fds3/4/5/6 exclusively. It is a sibling-fixture handoff, not
/// proof of the actual GTK-parent bootstrap or an independent product owner.
pub unsafe fn take_supervisor_fixture_transport()
-> io::Result<(CapabilityTransport, crate::ControllerWatch)> {
    let (transport, original) = unsafe { take_inherited(Role::Supervisor) }?;
    let watch = crate::ControllerWatch::new(
        original.ok_or(invalid("missing original controller watch"))?,
        transport.limits.heartbeat_timeout,
        transport.session,
    )?;
    Ok((transport, watch))
}
unsafe fn take_inherited(
    role: Role,
) -> io::Result<(CapabilityTransport, Option<OriginalController>)> {
    // Own the sealed bootstrap first. No caller-controlled path or PID is read.
    let bootstrap = unsafe { File::from_raw_fd(BOOT_FD) };
    cloexec(bootstrap.as_raw_fd())?;
    let metadata = bootstrap.metadata()?;
    let seals = unsafe { libc::fcntl(bootstrap.as_raw_fd(), libc::F_GET_SEALS) };
    validate_bootstrap_seals(seals)?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 0
        || metadata.mode() & 0o7777 != 0o400
        || metadata.len() != BOOT_BYTES as u64
    {
        return Err(invalid("controller bootstrap object policy"));
    }
    let mut bytes = Zeroizing::new([0u8; BOOT_BYTES]);
    bootstrap.read_exact_at(&mut *bytes, 0)?;
    if &bytes[..8] != b"MCBOOT01"
        || bytes[8] != role as u8
        || bytes[9] != 1
        || bytes[10..12] != [0, 0]
        || bytes[28..32] != [0; 4]
        || u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize != MAX_CONTROL_PAYLOAD
    {
        return Err(invalid("controller bootstrap role/version"));
    }
    let pid = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if (role == Role::Controller && pid != 0) || (role == Role::Supervisor && pid == 0) {
        return Err(invalid("controller bootstrap watch role"));
    }
    let limits = ControllerLimits {
        heartbeat_timeout: Duration::from_millis(u32::from_le_bytes(
            bytes[16..20].try_into().unwrap(),
        ) as u64),
        frame_timeout: Duration::from_millis(
            u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as u64
        ),
    };
    limits.validate()?;
    let session = bytes[32..64].try_into().unwrap();
    let capability = Zeroizing::new(bytes[64..96].try_into().unwrap());
    let artifact = bytes[96..128].try_into().unwrap();
    let read = unsafe { File::from_raw_fd(RX_FD) };
    let write = unsafe { File::from_raw_fd(TX_FD) };
    validate_pipe(&read, libc::O_RDONLY)?;
    validate_pipe(&write, libc::O_WRONLY)?;
    let original = if role == Role::Supervisor {
        let fd = unsafe { OwnedFd::from_raw_fd(CONTROLLER_FD) };
        cloexec(fd.as_raw_fd())?;
        Some(unsafe { OriginalController::from_inherited(fd, pid) }?)
    } else {
        None
    };
    Ok((
        CapabilityTransport {
            reader: Some(read),
            writer: Some(write),
            session,
            capability,
            artifact,
            role,
            limits,
            rx: Zeroizing::new(Vec::new()),
            target: 4,
            started: None,
            last_now: Duration::ZERO,
            received: 0,
            sent: 0,
            outgoing: VecDeque::new(),
            queued_bytes: 0,
            failed: false,
            eof: false,
        },
        original,
    ))
}
fn validate_bootstrap_seals(seals: i32) -> io::Result<()> {
    if seals < 0 {
        return Err(kernel_error("F_GET_SEALS controller bootstrap"));
    }
    if seals & BOOT_SEALS != BOOT_SEALS {
        return Err(invalid("controller bootstrap seal policy"));
    }
    Ok(())
}
fn validate_pipe(file: &File, access: i32) -> io::Result<()> {
    cloexec(file.as_raw_fd())?;
    let metadata = file.metadata()?;
    let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
    if metadata.mode() & libc::S_IFMT != libc::S_IFIFO
        || metadata.uid() != unsafe { libc::geteuid() }
        || flags < 0
        || flags & libc::O_ACCMODE != access
        || flags & libc::O_NONBLOCK == 0
    {
        return Err(invalid("controller pipe direction/object policy"));
    }
    Ok(())
}
fn cloexec(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        return Err(kernel_error("seal inherited controller role descriptor"));
    }
    Ok(())
}
pub(crate) fn duplicate_high(fd: RawFd) -> io::Result<OwnedFd> {
    let copied = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 16) };
    if copied < 0 {
        return Err(kernel_error("stage typed controller descriptor"));
    }
    Ok(unsafe { OwnedFd::from_raw_fd(copied) })
}
fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } < 0 {
        return Err(kernel_error("private nonblocking controller pipes"));
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}
fn random(output: &mut [u8]) -> io::Result<()> {
    let mut done = 0;
    for _ in 0..8 {
        let count =
            unsafe { libc::getrandom(output[done..].as_mut_ptr().cast(), output.len() - done, 0) };
        if count < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if count == 0 {
            return Err(invalid("short controller entropy"));
        }
        done += count as usize;
        if done == output.len() {
            return Ok(());
        }
    }
    Err(invalid("bounded controller entropy failed"))
}

struct Pending {
    bytes: Zeroizing<Vec<u8>>,
    offset: usize,
    deadline: Duration,
}
/// Every syscall here is nonblocking; retained input/output bytes and work per
/// poll are capped. This type cannot mint a product owner or release proof.
pub struct CapabilityTransport {
    reader: Option<File>,
    writer: Option<File>,
    session: [u8; 32],
    capability: Zeroizing<[u8; 32]>,
    artifact: [u8; 32],
    role: Role,
    limits: ControllerLimits,
    rx: Zeroizing<Vec<u8>>,
    target: usize,
    started: Option<Duration>,
    last_now: Duration,
    received: u64,
    sent: u64,
    outgoing: VecDeque<Pending>,
    queued_bytes: usize,
    failed: bool,
    eof: bool,
}
impl CapabilityTransport {
    pub fn bound_artifact_digest(&self) -> [u8; 32] {
        self.artifact
    }
    pub fn queued_bytes(&self) -> usize {
        self.queued_bytes
    }
    pub fn healthy(&self) -> bool {
        !self.failed && !self.eof
    }
    fn reject(&mut self, message: &'static str) -> io::Error {
        self.failed = true;
        invalid(message)
    }
    fn now(&mut self, now: Duration) -> io::Result<()> {
        if now < self.last_now {
            return Err(self.reject("controller transport clock regression"));
        }
        self.last_now = now;
        if self.failed {
            return Err(invalid("controller transport already failed"));
        }
        Ok(())
    }
    pub fn queue(&mut self, kind: FrameKind, payload: &[u8], now: Duration) -> io::Result<()> {
        self.now(now)?;
        if !self.healthy() || !self.role.sends(kind) || !payload_valid(kind, payload.len()) {
            return Err(self.reject("controller output role/payload policy"));
        }
        let length = FRAME_PREFIX + payload.len();
        if self.outgoing.len() >= MAX_QUEUED_FRAMES
            || self.queued_bytes + length + 4 > MAX_QUEUE_BYTES
        {
            return Err(self.reject("controller output queue budget"));
        }
        let sequence = self
            .sent
            .checked_add(1)
            .ok_or_else(|| self.reject("controller output sequence exhausted"))?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(length + 4));
        bytes.extend_from_slice(&(length as u32).to_le_bytes());
        bytes.extend_from_slice(b"MCTL");
        bytes.extend_from_slice(&[1, self.role as u8, kind as u8, 0]);
        bytes.extend_from_slice(&sequence.to_le_bytes());
        bytes.extend_from_slice(&self.session);
        bytes.extend_from_slice(&*self.capability);
        bytes.extend_from_slice(payload);
        let deadline = now
            .checked_add(self.limits.frame_timeout)
            .ok_or_else(|| self.reject("controller output deadline overflow"))?;
        self.queued_bytes += bytes.len();
        self.sent = sequence;
        self.outgoing.push_back(Pending {
            bytes,
            offset: 0,
            deadline,
        });
        Ok(())
    }
    /// true means every queued frame physically completed. false is still a
    /// pending original offset, never permission to replay it from byte zero.
    pub fn poll_write(&mut self, now: Duration) -> io::Result<bool> {
        self.now(now)?;
        let mut written = 0;
        for _ in 0..POLL_ATTEMPTS {
            let Some(frame) = self.outgoing.front_mut() else {
                return Ok(true);
            };
            if now >= frame.deadline {
                return Err(self.reject("controller output deadline expired"));
            }
            let amount = (frame.bytes.len() - frame.offset).min(POLL_BYTES - written);
            if amount == 0 {
                return Ok(false);
            }
            let writer = self
                .writer
                .as_mut()
                .ok_or(invalid("controller writer retired"))?;
            match writer.write(&frame.bytes[frame.offset..frame.offset + amount]) {
                Ok(0) => return Err(self.reject("zero controller write")),
                Ok(count) => {
                    frame.offset += count;
                    written += count;
                    if frame.offset == frame.bytes.len() {
                        let frame = self.outgoing.pop_front().unwrap();
                        self.queued_bytes -= frame.bytes.len();
                    }
                    if written >= POLL_BYTES {
                        return Ok(self.outgoing.is_empty());
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(self.reject("controller write failed")),
            }
        }
        Ok(self.outgoing.is_empty())
    }
    pub fn poll_read(&mut self, now: Duration) -> io::Result<FrameRead> {
        self.now(now)?;
        if self.eof {
            return Ok(FrameRead::Eof);
        }
        if self
            .started
            .is_some_and(|first| now.saturating_sub(first) >= self.limits.frame_timeout)
        {
            return Err(self.reject("controller input original deadline expired"));
        }
        let mut buffer = Zeroizing::new([0u8; 4096]);
        let mut total = 0;
        for _ in 0..POLL_ATTEMPTS {
            let amount = (self.target - self.rx.len())
                .min(buffer.len())
                .min(POLL_BYTES - total);
            if amount == 0 {
                return Ok(FrameRead::Pending);
            }
            let reader = self
                .reader
                .as_mut()
                .ok_or(invalid("controller reader retired"))?;
            match reader.read(&mut buffer[..amount]) {
                Ok(0) => {
                    if !self.rx.is_empty() {
                        return Err(self.reject("truncated controller frame"));
                    }
                    self.eof = true;
                    return Ok(FrameRead::Eof);
                }
                Ok(count) => {
                    self.started.get_or_insert(now);
                    self.rx.extend_from_slice(&buffer[..count]);
                    total += count;
                    if self.target == 4 && self.rx.len() == 4 {
                        let body = u32::from_le_bytes(self.rx[..4].try_into().unwrap()) as usize;
                        if !(FRAME_PREFIX..=FRAME_PREFIX + MAX_CONTROL_PAYLOAD).contains(&body) {
                            return Err(self.reject("controller input length budget"));
                        }
                        self.target = body + 4;
                    }
                    if self.target > 4 && self.rx.len() == self.target {
                        let result = self.decode();
                        self.rx.zeroize();
                        self.target = 4;
                        self.started = None;
                        return result.map(FrameRead::Frame);
                    }
                    if total >= POLL_BYTES {
                        return Ok(FrameRead::Pending);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Ok(FrameRead::Pending);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(self.reject("controller read failed")),
            }
        }
        Ok(FrameRead::Pending)
    }
    fn decode(&mut self) -> io::Result<AuthenticatedFrame> {
        let frame = &self.rx[4..];
        let expected_role = if self.role == Role::Controller {
            Role::Supervisor
        } else {
            Role::Controller
        };
        if &frame[..4] != b"MCTL"
            || frame[4] != 1
            || frame[5] != expected_role as u8
            || frame[7] != 0
            || frame[16..48] != self.session
            || !bool::from(frame[48..80].ct_eq(&*self.capability))
        {
            return Err(self.reject("controller frame authentication/session/role mismatch"));
        }
        let kind = match frame[6] {
            1 => FrameKind::Data,
            2 => FrameKind::Heartbeat,
            3 => FrameKind::GracefulClose,
            4 => FrameKind::Reply,
            5 => FrameKind::Status,
            6 => FrameKind::Diagnostic,
            _ => return Err(self.reject("unknown controller frame kind")),
        };
        if !self.role.accepts(kind) || !payload_valid(kind, frame.len() - FRAME_PREFIX) {
            return Err(self.reject("controller input kind/payload policy"));
        }
        let sequence = u64::from_le_bytes(frame[8..16].try_into().unwrap());
        if self.received.checked_add(1) != Some(sequence) {
            return Err(self.reject("controller input sequence replay/gap"));
        }
        let result = AuthenticatedFrame {
            kind,
            sequence,
            session: self.session,
            payload: Zeroizing::new(frame[FRAME_PREFIX..].to_vec()),
        };
        self.received = sequence;
        Ok(result)
    }
    /// Explicit IO retirement is distinct from EOF/reap. Nonblocking syscalls
    /// have already returned; no async worker or pending kernel buffer escapes.
    pub fn retire(&mut self) {
        self.failed = true;
        self.reader.take();
        self.writer.take();
        self.outgoing.clear();
        self.queued_bytes = 0;
        self.rx.zeroize();
        self.started = None;
        self.capability.zeroize();
    }
    pub fn io_retired(&self) -> bool {
        self.reader.is_none()
            && self.writer.is_none()
            && self.outgoing.is_empty()
            && self.rx.is_empty()
    }
}
fn payload_valid(kind: FrameKind, length: usize) -> bool {
    match kind {
        FrameKind::Data => (1..=MAX_CONTROL_PAYLOAD).contains(&length),
        FrameKind::Heartbeat | FrameKind::GracefulClose => length == 0,
        FrameKind::Reply => length <= MAX_CONTROL_PAYLOAD,
        FrameKind::Status | FrameKind::Diagnostic => length <= 4096,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn endpoints() -> (CapabilityTransport, CapabilityTransport) {
        let (controller, supervisor) = fixture_channel_pair(ControllerLimits {
            heartbeat_timeout: Duration::from_millis(500),
            frame_timeout: Duration::from_millis(100),
        })
        .unwrap();
        fn endpoint(
            config: SecretConfig,
            read: OwnedFd,
            write: OwnedFd,
            role: Role,
        ) -> CapabilityTransport {
            CapabilityTransport {
                reader: Some(File::from(read)),
                writer: Some(File::from(write)),
                session: config.session,
                capability: config.capability,
                artifact: [0; 32],
                role,
                limits: config.limits,
                rx: Zeroizing::new(Vec::new()),
                target: 4,
                started: None,
                last_now: Duration::ZERO,
                received: 0,
                sent: 0,
                outgoing: VecDeque::new(),
                queued_bytes: 0,
                failed: false,
                eof: false,
            }
        }
        (
            endpoint(
                controller.config,
                controller.read,
                controller.write,
                Role::Controller,
            ),
            endpoint(
                supervisor.config,
                supervisor.read,
                supervisor.write,
                Role::Supervisor,
            ),
        )
    }
    #[test]
    fn authenticated_sequence_and_payload_are_stripped_from_private_header() {
        let (mut controller, mut supervisor) = endpoints();
        controller
            .queue(FrameKind::Heartbeat, &[], Duration::ZERO)
            .unwrap();
        controller
            .queue(FrameKind::Data, b"data", Duration::ZERO)
            .unwrap();
        assert!(controller.poll_write(Duration::ZERO).unwrap());
        let FrameRead::Frame(first) = supervisor.poll_read(Duration::ZERO).unwrap() else {
            panic!("missing heartbeat")
        };
        assert_eq!(first.kind(), FrameKind::Heartbeat);
        assert_eq!(first.sequence(), 1);
        assert!(first.payload().is_empty());
        let FrameRead::Frame(second) = supervisor.poll_read(Duration::ZERO).unwrap() else {
            panic!("missing data")
        };
        assert_eq!(second.sequence(), 2);
        assert_eq!(second.payload(), b"data");
    }
    #[test]
    fn invalid_auth_session_version_direction_reserved_kind_and_sequence_are_refused() {
        for index in [8usize, 9, 10, 11, 20, 52, 12] {
            let (mut controller, mut supervisor) = endpoints();
            controller
                .queue(FrameKind::Heartbeat, &[], Duration::ZERO)
                .unwrap();
            let mut bytes = Zeroizing::new(controller.outgoing.front().unwrap().bytes.to_vec());
            bytes[index] ^= 0x80;
            controller
                .writer
                .as_mut()
                .unwrap()
                .write_all(&bytes)
                .unwrap();
            assert!(supervisor.poll_read(Duration::ZERO).is_err());
            assert!(!supervisor.healthy());
        }
    }
    #[test]
    fn full_authenticated_frame_replay_is_refused() {
        let (mut controller, mut supervisor) = endpoints();
        controller
            .queue(FrameKind::Heartbeat, &[], Duration::ZERO)
            .unwrap();
        let bytes = Zeroizing::new(controller.outgoing.front().unwrap().bytes.to_vec());
        assert!(controller.poll_write(Duration::ZERO).unwrap());
        assert!(matches!(
            supervisor.poll_read(Duration::ZERO).unwrap(),
            FrameRead::Frame(_)
        ));
        controller
            .writer
            .as_mut()
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        assert!(supervisor.poll_read(Duration::ZERO).is_err());
    }
    #[test]
    fn length_budget_rejects_before_body_allocation() {
        for length in [
            0u32,
            79,
            (FRAME_PREFIX + MAX_CONTROL_PAYLOAD + 1) as u32,
            u32::MAX,
        ] {
            let (mut controller, mut supervisor) = endpoints();
            controller
                .writer
                .as_mut()
                .unwrap()
                .write_all(&length.to_le_bytes())
                .unwrap();
            assert!(supervisor.poll_read(Duration::ZERO).is_err());
            assert_eq!(supervisor.rx.len(), 4);
        }
    }
    #[test]
    fn drip_feed_keeps_first_byte_deadline_and_partial_eof_rejects() {
        let (mut controller, mut supervisor) = endpoints();
        controller
            .writer
            .as_mut()
            .unwrap()
            .write_all(&[80])
            .unwrap();
        assert!(matches!(
            supervisor.poll_read(Duration::ZERO).unwrap(),
            FrameRead::Pending
        ));
        controller.writer.as_mut().unwrap().write_all(&[0]).unwrap();
        assert!(matches!(
            supervisor.poll_read(Duration::from_millis(99)).unwrap(),
            FrameRead::Pending
        ));
        assert!(supervisor.poll_read(Duration::from_millis(100)).is_err());
        let (mut controller, mut supervisor) = endpoints();
        controller
            .writer
            .as_mut()
            .unwrap()
            .write_all(&[80])
            .unwrap();
        controller.writer.take();
        assert!(supervisor.poll_read(Duration::ZERO).is_err());
    }
    #[test]
    fn queue_byte_count_role_and_sequence_limits_reject_without_growth() {
        let (mut controller, _supervisor) = endpoints();
        for _ in 0..MAX_QUEUED_FRAMES {
            controller
                .queue(FrameKind::Heartbeat, &[], Duration::ZERO)
                .unwrap();
        }
        let before = controller.queued_bytes();
        assert!(
            controller
                .queue(FrameKind::Heartbeat, &[], Duration::ZERO)
                .is_err()
        );
        assert_eq!(controller.queued_bytes(), before);
        controller.retire();
        assert!(controller.io_retired());
        assert!(bool::from(controller.capability.ct_eq(&[0; 32])));
        let (mut controller, _supervisor) = endpoints();
        assert!(
            controller
                .queue(FrameKind::Status, &[], Duration::ZERO)
                .is_err()
        );
        assert_eq!(controller.queued_bytes(), 0);
        let (mut controller, _supervisor) = endpoints();
        controller.sent = u64::MAX;
        assert!(
            controller
                .queue(FrameKind::Heartbeat, &[], Duration::ZERO)
                .is_err()
        );
        assert_eq!(controller.queued_bytes(), 0);
    }
    #[test]
    fn real_pipe_backpressure_retains_exact_partial_offset_without_replay() {
        let (mut controller, mut supervisor) = endpoints();
        let payload: Vec<u8> = (0..MAX_CONTROL_PAYLOAD).map(|i| (i % 251) as u8).collect();
        controller
            .queue(FrameKind::Data, &payload, Duration::ZERO)
            .unwrap();
        assert!(!controller.poll_write(Duration::ZERO).unwrap());
        let offset = controller.outgoing.front().unwrap().offset;
        assert!(offset > 0 && offset < FRAME_PREFIX + 4 + payload.len());
        let mut got = false;
        for _ in 0..64 {
            match supervisor.poll_read(Duration::ZERO).unwrap() {
                FrameRead::Frame(frame) => {
                    assert!(frame.payload() == payload);
                    got = true;
                    break;
                }
                FrameRead::Pending => (),
                FrameRead::Eof => panic!("unexpected EOF"),
            }
            let _ = controller.poll_write(Duration::ZERO).unwrap();
        }
        assert!(got);
        assert_eq!(controller.queued_bytes(), 0);
    }
    #[test]
    fn incomplete_original_write_times_out_and_explicit_retirement_is_separate() {
        let (mut controller, _supervisor) = endpoints();
        controller
            .queue(
                FrameKind::Data,
                &vec![1; MAX_CONTROL_PAYLOAD],
                Duration::ZERO,
            )
            .unwrap();
        assert!(!controller.poll_write(Duration::ZERO).unwrap());
        assert!(controller.poll_write(Duration::from_millis(100)).is_err());
        assert!(!controller.io_retired());
        controller.retire();
        assert!(controller.io_retired());
    }
    #[test]
    fn negative_seal_query_cannot_masquerade_as_every_required_seal() {
        assert!(validate_bootstrap_seals(-1).is_err());
        assert!(validate_bootstrap_seals(0).is_err());
        assert!(validate_bootstrap_seals(libc::F_SEAL_SEAL).is_err());
        assert!(validate_bootstrap_seals(BOOT_SEALS).is_ok());
    }
    #[test]
    fn bootstrap_is_non_executable_private_and_immutably_sealed() {
        let (controller, _supervisor) = fixture_channel_pair(ControllerLimits {
            heartbeat_timeout: Duration::from_millis(500),
            frame_timeout: Duration::from_millis(100),
        })
        .unwrap();
        let file = bootstrap_file(&controller.config, Role::Controller, 0, [0; 32]).unwrap();
        let fd = file.as_raw_fd();
        assert_eq!(unsafe { libc::pwrite(fd, b"x".as_ptr().cast(), 1, 0) }, -1);
        assert_eq!(unsafe { libc::ftruncate(fd, 0) }, -1);
        assert_eq!(unsafe { libc::fchmod(fd, 0o500) }, -1);
        assert_eq!(file.metadata().unwrap().mode() & 0o7777, 0o400);
    }
}
