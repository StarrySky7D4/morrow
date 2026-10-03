//! Capability-authenticated inherited anonymous-pipe fixture transport.
//! It is not a public socket server, per-write PID/UID authentication, malicious
//! same-UID isolation, product authority, durable release or a message MAC.
use crate::{
    CurrentController, OriginalController, OwnedProcess, SealedExecutable, SpawnFailure, invalid,
    kernel_error,
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
    GuardianController = 3,
    GuardianSupervisor = 4,
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
            Self::GuardianController | Self::GuardianSupervisor => false,
        }
    }
    fn sends(self, kind: FrameKind) -> bool {
        match self {
            Self::Controller => Role::Supervisor.accepts(kind),
            Self::Supervisor => Role::Controller.accepts(kind),
            Self::GuardianController | Self::GuardianSupervisor => false,
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
/// Paired current-process endpoint and sealed supervisor launch. The endpoint
/// cannot escape before its self-only capture is revalidated and S is launched
/// by that same C. Ordinary dependent-parent-death and SIGCHLD rules apply.
pub struct CurrentControllerFixtureLaunch {
    current: CurrentController,
    controller: ControllerFixtureLaunch,
    supervisor: SupervisorFixtureLaunch,
    executable: SealedExecutable,
}
pub fn current_controller_fixture_pair(
    current: CurrentController,
    executable: SealedExecutable,
    limits: ControllerLimits,
) -> io::Result<CurrentControllerFixtureLaunch> {
    let (controller, supervisor) = fixture_channel_pair(limits)?;
    Ok(CurrentControllerFixtureLaunch {
        current,
        controller,
        supervisor,
        executable,
    })
}
impl CurrentControllerFixtureLaunch {
    pub fn spawn(
        self,
        cwd: &Path,
        args: &[&OsStr],
    ) -> Result<(CapabilityTransport, OwnedProcess), SpawnFailure> {
        let (transport, launch, original, executable) = self.prepare_current()?;
        let owner = launch
            .bind_original(original)
            .map_err(SpawnFailure::NotCreated)?
            .spawn(executable, cwd, args)?;
        Ok((transport, owner))
    }
    fn prepare_current(
        self,
    ) -> Result<
        (
            CapabilityTransport,
            SupervisorFixtureLaunch,
            OriginalController,
            SealedExecutable,
        ),
        SpawnFailure,
    > {
        // This check occurs before preparation or process creation. A inherited
        // capture in a forked process cannot become authority for its parent.
        let original = self
            .current
            .into_original()
            .map_err(SpawnFailure::NotCreated)?;
        let captured_in = original.diagnostic_pid();
        let artifact = self.executable.artifact_digest();
        let controller = self.controller;
        let reader = File::from(controller.read);
        let writer = File::from(controller.write);
        validate_pipe(&reader, libc::O_RDONLY).map_err(SpawnFailure::NotCreated)?;
        validate_pipe(&writer, libc::O_WRONLY).map_err(SpawnFailure::NotCreated)?;
        let config = controller.config;
        let transport = CapabilityTransport {
            reader: Some(reader),
            writer: Some(writer),
            session: config.session,
            capability: config.capability,
            artifact,
            role: Role::Controller,
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
            current_context: Some(captured_in),
        };
        Ok((transport, self.supervisor, original, self.executable))
    }
}
/// Descriptor construction is private: callers cannot feed arbitrary raw fds or
/// numeric PIDs into the original-controller handoff.
pub(crate) struct InheritedLaunch {
    pub(crate) descriptors: Vec<(RawFd, OwnedFd)>,
    orphan_supervisor: Option<SupervisorOrphanFixtureLifetime>,
}
// The only constructor is inside the guardian's typed supervisor launch below.
// It is unavailable to public/default/D launch APIs and carries no raw PID input.
struct SupervisorOrphanFixtureLifetime;
impl InheritedLaunch {
    pub(crate) fn parent_death_signal(&self) -> i32 {
        match self.orphan_supervisor {
            None => libc::SIGKILL,
            Some(SupervisorOrphanFixtureLifetime) => 0,
        }
    }
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
    Ok(InheritedLaunch {
        descriptors,
        orphan_supervisor: None,
    })
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
/// The typed current-controller fixture may also hand off these fixed slots;
/// its supervisor remains an ordinary child subject to SIGKILL parent death.
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
            current_context: None,
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
    current_context: Option<u32>,
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
        if self
            .current_context
            .is_some_and(|pid| pid != std::process::id())
        {
            return Err(self.reject("current controller endpoint process changed"));
        }
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
                current_context: None,
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

// Dedicated, process-local orphan-cleanup witness. This is not a product
// guardian, an arbitrary descriptor owner factory, or a crash-atomic transfer.
mod guardian_fixture {
    use super::*;
    use crate::{ExitObservation, OutputObservation};
    const GUARD_FD: RawFd = 7;
    const GUARD_BOOT: RawFd = 8;
    const GUARD_PIDFD: RawFd = 9;
    const SUPERVISOR_PEER: RawFd = 10;
    const PACKET: usize = 112;
    const G: u8 = 1;
    const C: u8 = 2;
    const S: u8 = 3;
    const REGISTER: u8 = 1;
    const ACK: u8 = 2;
    const DEPENDENT_READY: u8 = 3;
    const ARMED: u8 = 5;
    const LATE_QUEUED: u8 = 6;
    const CLEANED: u8 = 7;
    const FINISH: u8 = 8;
    const RETAIN_WRITER: u8 = 9;
    struct Wire {
        fd: OwnedFd,
        config: SecretConfig,
        local: u8,
        remote: u8,
        context: u32,
        sent: u64,
        received: u64,
    }
    struct Message {
        kind: u8,
        values: [u32; 8],
        rights: Vec<OwnedFd>,
    }
    impl Wire {
        fn context(&self) -> io::Result<()> {
            if self.context != std::process::id() {
                return Err(invalid("guardian fixture process changed"));
            }
            Ok(())
        }
        fn send(
            &mut self,
            kind: u8,
            values: [u32; 8],
            rights: &[RawFd],
            fault: Option<GuardianRegistrationFault>,
        ) -> io::Result<bool> {
            self.context()?;
            if rights.len() > 5 {
                return Err(invalid("guardian ancillary budget"));
            }
            let mut bytes = Zeroizing::new([0u8; PACKET]);
            bytes[..4].copy_from_slice(b"MGW1");
            bytes[4] = 1;
            bytes[5] = self.local;
            bytes[6] = kind;
            let seq = self
                .sent
                .checked_add(1)
                .ok_or(invalid("guardian sequence exhausted"))?;
            bytes[8..16].copy_from_slice(&seq.to_le_bytes());
            bytes[16..48].copy_from_slice(&self.config.session);
            bytes[48..80].copy_from_slice(&*self.config.capability);
            for (i, v) in values.iter().enumerate() {
                bytes[80 + i * 4..84 + i * 4].copy_from_slice(&v.to_le_bytes());
            }
            if fault == Some(GuardianRegistrationFault::WrongRole) {
                bytes[5] = S;
            }
            if fault == Some(GuardianRegistrationFault::Replay) {
                bytes[8..16].copy_from_slice(&0u64.to_le_bytes());
            }
            let length = if fault == Some(GuardianRegistrationFault::Truncated) {
                PACKET - 1
            } else {
                PACKET
            };
            let mut iov = libc::iovec {
                iov_base: bytes.as_mut_ptr().cast(),
                iov_len: length,
            };
            let mut control = [0usize; 8];
            let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
            msg.msg_iov = &mut iov;
            msg.msg_iovlen = 1;
            if !rights.is_empty() {
                msg.msg_control = control.as_mut_ptr().cast();
                msg.msg_controllen =
                    unsafe { libc::CMSG_SPACE((rights.len() * 4) as u32) } as usize;
                let header = unsafe { libc::CMSG_FIRSTHDR(&msg) };
                unsafe {
                    (*header).cmsg_level = libc::SOL_SOCKET;
                    (*header).cmsg_type = libc::SCM_RIGHTS;
                    (*header).cmsg_len = libc::CMSG_LEN((rights.len() * 4) as u32) as usize;
                    std::ptr::copy_nonoverlapping(
                        rights.as_ptr(),
                        libc::CMSG_DATA(header).cast(),
                        rights.len(),
                    );
                }
            }
            let count = unsafe {
                libc::sendmsg(
                    self.fd.as_raw_fd(),
                    &msg,
                    libc::MSG_NOSIGNAL | libc::MSG_DONTWAIT,
                )
            };
            if count < 0 {
                let e = io::Error::last_os_error();
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) {
                    return Ok(false);
                }
                return Err(e);
            }
            if count as usize != length {
                return Err(invalid("guardian partial packet"));
            }
            self.sent = seq;
            Ok(true)
        }
        fn receive(&mut self) -> io::Result<Option<Message>> {
            self.context()?;
            let mut bytes = Zeroizing::new([0u8; PACKET]);
            let mut iov = libc::iovec {
                iov_base: bytes.as_mut_ptr().cast(),
                iov_len: PACKET,
            };
            let mut control = [0usize; 8];
            let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
            msg.msg_iov = &mut iov;
            msg.msg_iovlen = 1;
            msg.msg_control = control.as_mut_ptr().cast();
            msg.msg_controllen = std::mem::size_of_val(&control);
            let count = unsafe {
                libc::recvmsg(
                    self.fd.as_raw_fd(),
                    &mut msg,
                    libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC,
                )
            };
            if count < 0 {
                let e = io::Error::last_os_error();
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) {
                    return Ok(None);
                }
                return Err(e);
            }
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "guardian peer closed",
                ));
            }
            let mut rights = Vec::new();
            let mut bad_control = false;
            let mut h = unsafe { libc::CMSG_FIRSTHDR(&msg) };
            while !h.is_null() {
                let header = unsafe { &*h };
                let base = unsafe { libc::CMSG_LEN(0) } as usize;
                if header.cmsg_level != libc::SOL_SOCKET
                    || header.cmsg_type != libc::SCM_RIGHTS
                    || header.cmsg_len < base
                    || (header.cmsg_len - base) % 4 != 0
                {
                    bad_control = true;
                    break;
                }
                let n = (header.cmsg_len - base) / 4;
                for i in 0..n {
                    let fd = unsafe { *libc::CMSG_DATA(h).cast::<RawFd>().add(i) };
                    rights.push(unsafe { OwnedFd::from_raw_fd(fd) });
                }
                h = unsafe { libc::CMSG_NXTHDR(&msg, h) };
            }
            // OwnedFd closes all actually delivered rights on every refusal.
            if bad_control
                || rights.len() > 5
                || msg.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0
                || count as usize != PACKET
            {
                return Err(invalid("guardian truncated/ancillary packet"));
            }
            let sequence = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
            if &bytes[..4] != b"MGW1"
                || bytes[4] != 1
                || bytes[5] != self.remote
                || bytes[7] != 0
                || sequence
                    != self
                        .received
                        .checked_add(1)
                        .ok_or(invalid("guardian receive exhausted"))?
                || !bool::from(bytes[16..48].ct_eq(&self.config.session))
                || !bool::from(bytes[48..80].ct_eq(&*self.config.capability))
            {
                return Err(invalid("guardian role/session/capability/sequence"));
            }
            self.received = sequence;
            let mut values = [0; 8];
            for (i, v) in values.iter_mut().enumerate() {
                *v = u32::from_le_bytes(bytes[80 + i * 4..84 + i * 4].try_into().unwrap());
            }
            Ok(Some(Message {
                kind: bytes[6],
                values,
                rights,
            }))
        }
    }
    fn sockets() -> io::Result<(OwnedFd, OwnedFd)> {
        let mut fds = [-1; 2];
        if unsafe {
            libc::socketpair(
                libc::AF_UNIX,
                libc::SOCK_SEQPACKET | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                0,
                fds.as_mut_ptr(),
            )
        } < 0
        {
            return Err(kernel_error("guardian private socketpair"));
        }
        Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
    }
    fn copied_config(config: &SecretConfig) -> SecretConfig {
        SecretConfig {
            limits: config.limits,
            session: config.session,
            capability: Zeroizing::new(*config.capability),
        }
    }
    fn wire(fd: OwnedFd, config: SecretConfig, local: u8, remote: u8) -> Wire {
        Wire {
            fd,
            config,
            local,
            remote,
            context: std::process::id(),
            sent: 0,
            received: 0,
        }
    }
    fn subreaper(expected: i32) -> io::Result<()> {
        let mut value = 0;
        if unsafe { libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut value) } < 0 {
            return Err(kernel_error("inspect fixture subreaper"));
        }
        if value != expected {
            return Err(invalid("guardian fixture subreaper policy"));
        }
        Ok(())
    }
    // G holds this real stdio writer without ever writing it. Blocking stdio
    // policy is preserved; business transport still requires O_NONBLOCK.
    fn validate_output_witness(fd: RawFd) -> io::Result<()> {
        cloexec(fd)?;
        let copy = File::from(duplicate_high(fd)?);
        let metadata = copy.metadata()?;
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if metadata.mode() & libc::S_IFMT != libc::S_IFIFO
            || metadata.uid() != unsafe { libc::geteuid() }
            || flags < 0
            || flags & libc::O_ACCMODE != libc::O_WRONLY
        {
            return Err(invalid("guardian output writer witness policy"));
        }
        Ok(())
    }
    pub struct GuardianControllerFixtureLaunch {
        current: CurrentController,
        executable: SealedExecutable,
        config: SecretConfig,
        guardian_c: OwnedFd,
        controller: OwnedFd,
        guardian_s: OwnedFd,
        supervisor: OwnedFd,
    }
    /// Caller must already be the dedicated isolated G subreaper fixture.
    /// Setup itself is not performed by this API and no host settings are reset.
    pub fn guardian_controller_fixture_launch(
        current: CurrentController,
        executable: SealedExecutable,
        limits: ControllerLimits,
    ) -> io::Result<GuardianControllerFixtureLaunch> {
        subreaper(1)?;
        limits.validate()?;
        let mut session = [0; 32];
        let mut capability = Zeroizing::new([0; 32]);
        random(&mut session)?;
        random(&mut *capability)?;
        let (guardian_c, controller) = sockets()?;
        let (guardian_s, supervisor) = sockets()?;
        Ok(GuardianControllerFixtureLaunch {
            current,
            executable,
            config: SecretConfig {
                limits,
                session,
                capability,
            },
            guardian_c,
            controller,
            guardian_s,
            supervisor,
        })
    }
    impl GuardianControllerFixtureLaunch {
        pub fn spawn(
            self,
            cwd: &Path,
            args: &[&OsStr],
        ) -> Result<(GuardianWitness, OwnedProcess), SpawnFailure> {
            subreaper(1).map_err(SpawnFailure::NotCreated)?;
            let original = self
                .current
                .into_original()
                .map_err(SpawnFailure::NotCreated)?;
            let artifact = self.executable.artifact_digest();
            let boot = bootstrap_file(
                &self.config,
                Role::GuardianController,
                original.diagnostic_pid(),
                artifact,
            )
            .map_err(SpawnFailure::NotCreated)?;
            let inherited = InheritedLaunch {
                descriptors: vec![
                    (
                        GUARD_FD,
                        duplicate_high(self.controller.as_raw_fd())
                            .map_err(SpawnFailure::NotCreated)?,
                    ),
                    (
                        GUARD_BOOT,
                        duplicate_high(boot.as_raw_fd()).map_err(SpawnFailure::NotCreated)?,
                    ),
                    (
                        GUARD_PIDFD,
                        duplicate_high(original.fd().as_raw_fd())
                            .map_err(SpawnFailure::NotCreated)?,
                    ),
                    (
                        SUPERVISOR_PEER,
                        duplicate_high(self.supervisor.as_raw_fd())
                            .map_err(SpawnFailure::NotCreated)?,
                    ),
                ],
                orphan_supervisor: None,
            };
            let owner = OwnedProcess::spawn_with_inherited(self.executable, cwd, args, inherited)?;
            let c_original = match owner.original_controller() {
                Ok(original) => original,
                Err(error) => {
                    return Err(SpawnFailure::Created {
                        error,
                        owner: Box::new(owner),
                    });
                }
            };
            // owner is retained if post-creation reference setup fails.
            let guardian = GuardianWitness {
                controller: wire(self.guardian_c, copied_config(&self.config), G, C),
                supervisor: wire(self.guardian_s, self.config, G, S),
                original_controller: c_original,
                registered: None,
                writer_holder: None,
                output_holder: None,
            };
            Ok((guardian, owner))
        }
    }
    struct InheritedGuardian {
        config: SecretConfig,
        artifact: [u8; 32],
        guardian: OriginalController,
        peer: OwnedFd,
    }
    unsafe fn take_guardian(role: Role) -> io::Result<InheritedGuardian> {
        subreaper(0)?;
        let boot = unsafe { File::from_raw_fd(GUARD_BOOT) };
        cloexec(boot.as_raw_fd())?;
        let md = boot.metadata()?;
        validate_bootstrap_seals(unsafe { libc::fcntl(boot.as_raw_fd(), libc::F_GET_SEALS) })?;
        if !md.is_file()
            || md.uid() != unsafe { libc::geteuid() }
            || md.nlink() != 0
            || md.mode() & 0o7777 != 0o400
            || md.len() != BOOT_BYTES as u64
        {
            return Err(invalid("guardian bootstrap object"));
        }
        let mut bytes = Zeroizing::new([0u8; BOOT_BYTES]);
        boot.read_exact_at(&mut *bytes, 0)?;
        if &bytes[..8] != b"MCBOOT01"
            || bytes[8] != role as u8
            || bytes[9] != 1
            || bytes[10..12] != [0, 0]
            || bytes[28..32] != [0; 4]
            || u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize != MAX_CONTROL_PAYLOAD
        {
            return Err(invalid("guardian bootstrap role/version"));
        }
        let limits = ControllerLimits {
            heartbeat_timeout: Duration::from_millis(u32::from_le_bytes(
                bytes[16..20].try_into().unwrap(),
            ) as u64),
            frame_timeout: Duration::from_millis(u32::from_le_bytes(
                bytes[20..24].try_into().unwrap(),
            ) as u64),
        };
        limits.validate()?;
        let config = SecretConfig {
            limits,
            session: bytes[32..64].try_into().unwrap(),
            capability: Zeroizing::new(bytes[64..96].try_into().unwrap()),
        };
        let fd = unsafe { OwnedFd::from_raw_fd(GUARD_PIDFD) };
        cloexec(fd.as_raw_fd())?;
        let guardian = unsafe {
            OriginalController::from_inherited(
                fd,
                u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            )
        }?;
        let peer = unsafe { OwnedFd::from_raw_fd(GUARD_FD) };
        cloexec(peer.as_raw_fd())?;
        let mut kind = 0i32;
        let mut len = 4;
        if unsafe {
            libc::getsockopt(
                peer.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_TYPE,
                (&mut kind as *mut i32).cast(),
                &mut len,
            )
        } < 0
            || kind != libc::SOCK_SEQPACKET
        {
            return Err(invalid("guardian private packet socket"));
        }
        let flags = unsafe { libc::fcntl(peer.as_raw_fd(), libc::F_GETFL) };
        if flags < 0 || flags & libc::O_NONBLOCK == 0 {
            return Err(invalid("guardian nonblocking socket"));
        }
        Ok(InheritedGuardian {
            config,
            artifact: bytes[96..128].try_into().unwrap(),
            guardian,
            peer,
        })
    }
    pub struct GuardianControllerFixture {
        wire: Wire,
        supervisor_peer: OwnedFd,
        guardian: OriginalController,
        artifact: [u8; 32],
    }
    pub struct GuardianSupervisorProcess {
        owner: OwnedProcess,
        session: [u8; 32],
        controller_context: u32,
    }
    /// Fixed slots are owned exclusively after the typed guardian C launch.
    pub unsafe fn take_guardian_controller_fixture() -> io::Result<GuardianControllerFixture> {
        let inherited = unsafe { take_guardian(Role::GuardianController) }?;
        let supervisor_peer = unsafe { OwnedFd::from_raw_fd(SUPERVISOR_PEER) };
        cloexec(supervisor_peer.as_raw_fd())?;
        Ok(GuardianControllerFixture {
            wire: wire(inherited.peer, inherited.config, C, G),
            supervisor_peer,
            guardian: inherited.guardian,
            artifact: inherited.artifact,
        })
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum GuardianRegistrationFault {
        Missing,
        Extra,
        Truncated,
        WrongRole,
        Replay,
    }
    impl GuardianControllerFixture {
        pub fn spawn_supervisor(
            &self,
            current: CurrentController,
            executable: SealedExecutable,
            cwd: &Path,
            args: &[&OsStr],
        ) -> Result<(CapabilityTransport, GuardianSupervisorProcess), SpawnFailure> {
            self.wire.context().map_err(SpawnFailure::NotCreated)?;
            if self
                .guardian
                .poll_exited()
                .map_err(SpawnFailure::NotCreated)?
                || executable.artifact_digest() != self.artifact
            {
                return Err(SpawnFailure::NotCreated(invalid(
                    "guardian launch identity/liveness",
                )));
            }
            let pair =
                current_controller_fixture_pair(current, executable, self.wire.config.limits)
                    .map_err(SpawnFailure::NotCreated)?;
            let (transport, launch, original, executable) = pair.prepare_current()?;
            let mut inherited = prepare_launch(
                launch.config,
                launch.read,
                launch.write,
                Some(original),
                Role::Supervisor,
                self.artifact,
            )
            .map_err(SpawnFailure::NotCreated)?;
            let boot = bootstrap_file(
                &self.wire.config,
                Role::GuardianSupervisor,
                self.guardian.diagnostic_pid(),
                self.artifact,
            )
            .map_err(SpawnFailure::NotCreated)?;
            for (slot, fd) in [
                (GUARD_FD, self.supervisor_peer.as_raw_fd()),
                (GUARD_BOOT, boot.as_raw_fd()),
                (GUARD_PIDFD, self.guardian.fd().as_raw_fd()),
            ] {
                inherited
                    .descriptors
                    .push((slot, duplicate_high(fd).map_err(SpawnFailure::NotCreated)?));
            }
            inherited.orphan_supervisor = Some(SupervisorOrphanFixtureLifetime);
            let owner = OwnedProcess::spawn_with_inherited(executable, cwd, args, inherited)?;
            let session = transport.session;
            Ok((
                transport,
                GuardianSupervisorProcess {
                    owner,
                    session,
                    controller_context: std::process::id(),
                },
            ))
        }
        pub fn register_supervisor(
            &mut self,
            supervisor: &GuardianSupervisorProcess,
            transport: &CapabilityTransport,
            fault: Option<GuardianRegistrationFault>,
        ) -> io::Result<bool> {
            self.wire.context()?;
            if supervisor.controller_context != std::process::id()
                || transport.current_context != Some(supervisor.controller_context)
                || transport.session != supervisor.session
                || transport.role != Role::Controller
                || transport.artifact != self.artifact
            {
                return Err(invalid("guardian supervisor/control pair changed"));
            }
            let (pidfd, stdout, stderr) = supervisor.owner.guardian_fixture_handles()?;
            let writer = transport
                .writer
                .as_ref()
                .ok_or(invalid("retired C writer"))?;
            let mut rights = vec![
                pidfd.as_raw_fd(),
                stdout.as_raw_fd(),
                stderr.as_raw_fd(),
                writer.as_raw_fd(),
            ];
            if fault == Some(GuardianRegistrationFault::Missing) {
                rights.pop();
            }
            if fault == Some(GuardianRegistrationFault::Extra) {
                rights.push(stdout.as_raw_fd());
            }
            self.wire.send(
                REGISTER,
                [supervisor.owner.pid(), std::process::id(), 0, 0, 0, 0, 0, 0],
                &rights,
                fault,
            )
        }
        pub fn poll_ack(&mut self) -> io::Result<Option<bool>> {
            let Some(m) = self.wire.receive()? else {
                return Ok(None);
            };
            if m.kind != ACK || !m.rights.is_empty() || m.values[1..] != [0; 7] || m.values[0] > 1 {
                return Err(invalid("guardian ack policy"));
            }
            Ok(Some(m.values[0] == 1))
        }
        pub fn late_queued(&mut self) -> io::Result<bool> {
            self.wire.send(LATE_QUEUED, [0; 8], &[], None)
        }
    }
    impl GuardianSupervisorProcess {
        pub fn pid(&self) -> u32 {
            self.owner.pid()
        }
        // Used only before the deliberate-death handoff or on rejection.
        pub fn reclaim_while_controller_live(&mut self) -> io::Result<bool> {
            if self.controller_context != std::process::id() {
                return Err(invalid(
                    "guardian controller process changed before reclaim",
                ));
            }
            self.owner.terminate()?;
            self.owner.wait_bounded(Duration::from_secs(2))
        }
        pub fn output(&self) -> &OutputObservation {
            self.owner.output()
        }
    }
    pub struct GuardianSupervisorFixture {
        wire: Wire,
        guardian: OriginalController,
    }
    /// Fixed slots7/8/9 exclusively inherited through typed S-only launch.
    pub unsafe fn take_guardian_supervisor_fixture() -> io::Result<GuardianSupervisorFixture> {
        let i = unsafe { take_guardian(Role::GuardianSupervisor) }?;
        Ok(GuardianSupervisorFixture {
            wire: wire(i.peer, i.config, S, G),
            guardian: i.guardian,
        })
    }
    impl GuardianSupervisorFixture {
        pub fn guardian_pid(&self) -> u32 {
            self.guardian.diagnostic_pid()
        }
        pub fn dependent_ready(&mut self, d: u32, c: u32) -> io::Result<bool> {
            self.wire.send(
                DEPENDENT_READY,
                [std::process::id(), c, d, self.guardian_pid(), 0, 0, 0, 0],
                &[],
                None,
            )
        }
        pub fn armed(&mut self) -> io::Result<bool> {
            self.wire.send(ARMED, [0; 8], &[], None)
        }
        pub fn cleaned(
            &mut self,
            d: u32,
            c: u32,
            late_heartbeat: u32,
            late_data: u32,
        ) -> io::Result<bool> {
            self.wire.send(
                CLEANED,
                [
                    std::process::id(),
                    unsafe { libc::getppid() } as u32,
                    c,
                    d,
                    late_heartbeat,
                    late_data,
                    1,
                    1,
                ],
                &[],
                None,
            )
        }
        pub fn retain_output_writer(&mut self, stderr: bool) -> io::Result<bool> {
            let fd = duplicate_high(if stderr { 2 } else { 1 })?;
            self.wire.send(
                RETAIN_WRITER,
                [stderr as u32, 0, 0, 0, 0, 0, 0, 0],
                &[fd.as_raw_fd()],
                None,
            )
        }
        pub fn poll_finish(&mut self) -> io::Result<Option<bool>> {
            let Some(m) = self.wire.receive()? else {
                return Ok(None);
            };
            if !m.rights.is_empty() || m.values != [0; 8] || m.kind != FINISH {
                return Err(invalid("guardian witness completion only"));
            }
            Ok(Some(true))
        }
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum GuardianWaitEligibility {
        NotChild,
        LiveChild,
        ExitedUnreaped,
    }
    pub enum GuardianEvent {
        Registered,
        DependentReady {
            supervisor: u32,
            controller: u32,
            dependent: u32,
            guardian: u32,
        },
        Armed,
        LateQueued,
        Cleaned {
            supervisor: u32,
            parent: u32,
            controller: u32,
            dependent: u32,
            late_heartbeat: u32,
            late_data: u32,
        },
        RetainedWriter {
            stderr: bool,
        },
    }
    struct ObservedSupervisor {
        object: OriginalController,
        stdout: File,
        stderr: File,
        output: OutputObservation,
        exit: Option<ExitObservation>,
        adopted: bool,
    }
    pub struct GuardianWitness {
        controller: Wire,
        supervisor: Wire,
        original_controller: OriginalController,
        registered: Option<ObservedSupervisor>,
        writer_holder: Option<OwnedFd>,
        output_holder: Option<OwnedFd>,
    }
    impl GuardianWitness {
        pub fn poll_controller(&mut self) -> io::Result<Option<GuardianEvent>> {
            let Some(mut m) = self.controller.receive()? else {
                return Ok(None);
            };
            match m.kind {
                REGISTER => {
                    if self.registered.is_some()
                        || m.rights.len() != 4
                        || m.values[0] == 0
                        || m.values[1] != self.original_controller.diagnostic_pid()
                        || m.values[2..] != [0; 6]
                    {
                        return Err(invalid("guardian exact single registration"));
                    }
                    let held_writer = m.rights.pop().unwrap();
                    let stderr = File::from(m.rights.pop().unwrap());
                    let stdout = File::from(m.rights.pop().unwrap());
                    let pidfd = m.rights.pop().unwrap();
                    validate_pipe(&stdout, libc::O_RDONLY)?;
                    validate_pipe(&stderr, libc::O_RDONLY)?;
                    validate_pipe(
                        &File::from(duplicate_high(held_writer.as_raw_fd())?),
                        libc::O_WRONLY,
                    )?;
                    let object = unsafe { OriginalController::from_inherited(pidfd, m.values[0]) }?;
                    self.registered = Some(ObservedSupervisor {
                        object,
                        stdout,
                        stderr,
                        output: OutputObservation::default(),
                        exit: None,
                        adopted: false,
                    });
                    self.writer_holder = Some(held_writer);
                    Ok(Some(GuardianEvent::Registered))
                }
                LATE_QUEUED if m.rights.is_empty() && m.values == [0; 8] => {
                    Ok(Some(GuardianEvent::LateQueued))
                }
                _ => Err(invalid("guardian controller report role")),
            }
        }
        pub fn acknowledge_registration(&mut self, accept: bool) -> io::Result<bool> {
            self.controller
                .send(ACK, [accept as u32, 0, 0, 0, 0, 0, 0, 0], &[], None)
        }
        pub fn poll_supervisor(&mut self) -> io::Result<Option<GuardianEvent>> {
            let Some(mut m) = self.supervisor.receive()? else {
                return Ok(None);
            };
            let s = self
                .registered
                .as_ref()
                .ok_or(invalid("S report before registration"))?
                .object
                .diagnostic_pid();
            if m.kind == RETAIN_WRITER
                && m.rights.len() == 1
                && m.values[0] <= 1
                && m.values[1..] == [0; 7]
            {
                // Separate G-owned output witness, never a descendant-tree claim.
                if self.writer_holder.is_none() {
                    return Err(invalid("missing passive C writer"));
                }
                if self.output_holder.is_some() {
                    return Err(invalid("duplicate output retention witness"));
                }
                let fd = m.rights.pop().unwrap();
                validate_output_witness(fd.as_raw_fd())?;
                // Returned via explicit separate storage, not replacing passive C writer.
                self.output_holder = Some(fd);
                return Ok(Some(GuardianEvent::RetainedWriter {
                    stderr: m.values[0] == 1,
                }));
            }
            if !m.rights.is_empty() {
                return Err(invalid("unexpected guardian report rights"));
            }
            match m.kind {
                DEPENDENT_READY
                    if m.values[0] == s
                        && m.values[1] == self.original_controller.diagnostic_pid()
                        && m.values[2] != 0
                        && m.values[3] == std::process::id()
                        && m.values[4..] == [0; 4] =>
                {
                    Ok(Some(GuardianEvent::DependentReady {
                        supervisor: s,
                        controller: m.values[1],
                        dependent: m.values[2],
                        guardian: m.values[3],
                    }))
                }
                ARMED if m.values == [0; 8] => Ok(Some(GuardianEvent::Armed)),
                CLEANED
                    if m.values[0] == s
                        && m.values[1] == std::process::id()
                        && m.values[2] == self.original_controller.diagnostic_pid()
                        && m.values[6..] == [1, 1] =>
                {
                    Ok(Some(GuardianEvent::Cleaned {
                        supervisor: s,
                        parent: m.values[1],
                        controller: m.values[2],
                        dependent: m.values[3],
                        late_heartbeat: m.values[4],
                        late_data: m.values[5],
                    }))
                }
                _ => Err(invalid("guardian supervisor report policy")),
            }
        }
        pub fn wait_eligibility(&mut self) -> io::Result<GuardianWaitEligibility> {
            self.supervisor.context()?;
            subreaper(1)?;
            let observed = self
                .registered
                .as_mut()
                .ok_or(invalid("unregistered supervisor"))?;
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PIDFD,
                    observed.object.fd().as_raw_fd() as u32,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result < 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::ECHILD) {
                    return Ok(GuardianWaitEligibility::NotChild);
                }
                return Err(e);
            }
            if !self.original_controller.poll_exited()? {
                return Err(invalid("wait rights changed while original C lives"));
            }
            observed.adopted = true;
            Ok(if unsafe { info.si_pid() } == 0 {
                GuardianWaitEligibility::LiveChild
            } else {
                GuardianWaitEligibility::ExitedUnreaped
            })
        }
        pub fn acknowledge_finish(&mut self) -> io::Result<bool> {
            self.supervisor.send(FINISH, [0; 8], &[], None)
        }
        pub fn observe_supervisor(&mut self) -> io::Result<bool> {
            self.supervisor.context()?;
            let s = self
                .registered
                .as_mut()
                .ok_or(invalid("unregistered supervisor"))?;
            drain(
                &mut s.stdout,
                &mut s.output.stdout,
                &mut s.output.stdout_eof,
                &mut s.output.stdout_discarded,
            )?;
            drain(
                &mut s.stderr,
                &mut s.output.stderr,
                &mut s.output.stderr_eof,
                &mut s.output.stderr_discarded,
            )?;
            if s.exit.is_none() {
                if !s.adopted {
                    return Err(invalid("no proven supervisor adoption"));
                }
                let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
                if unsafe {
                    libc::waitid(
                        libc::P_PIDFD,
                        s.object.fd().as_raw_fd() as u32,
                        &mut info,
                        libc::WEXITED | libc::WNOHANG,
                    )
                } < 0
                {
                    return Err(kernel_error("guardian exact S reap"));
                }
                let pid = unsafe { info.si_pid() };
                if pid != 0 {
                    if pid != s.object.diagnostic_pid() as i32 {
                        return Err(invalid("guardian reaped another object"));
                    }
                    s.exit = Some(match info.si_code {
                        libc::CLD_EXITED => ExitObservation::Exited(unsafe { info.si_status() }),
                        libc::CLD_KILLED | libc::CLD_DUMPED => {
                            ExitObservation::Signaled(unsafe { info.si_status() })
                        }
                        _ => return Err(invalid("guardian unexpected exit")),
                    });
                }
            }
            Ok(s.exit.is_some() && s.output.stdout_eof && s.output.stderr_eof)
        }
        pub fn supervisor_pid(&self) -> Option<u32> {
            self.registered.as_ref().map(|s| s.object.diagnostic_pid())
        }
        pub fn supervisor_output(&self) -> Option<&OutputObservation> {
            self.registered.as_ref().map(|s| &s.output)
        }
        pub fn supervisor_exit(&self) -> Option<ExitObservation> {
            self.registered.as_ref().and_then(|s| s.exit)
        }
        pub fn release_output_witness(&mut self) {
            self.output_holder.take();
        }
        pub fn release_passive_control_writer(&mut self) {
            self.writer_holder.take();
        }
        // Bounded failure backstop only; signaling is never reaping evidence.
        pub fn signal_registered_supervisor(&mut self) -> io::Result<()> {
            self.supervisor.context()?;
            let s = self
                .registered
                .as_ref()
                .ok_or(invalid("unregistered supervisor"))?;
            let result = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    s.object.fd().as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0u32,
                )
            };
            if result < 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
                return Err(kernel_error("guardian fixture backstop signal"));
            }
            Ok(())
        }
    }
    fn drain(
        file: &mut impl Read,
        retained: &mut Vec<u8>,
        eof: &mut bool,
        discarded: &mut u64,
    ) -> io::Result<()> {
        let mut buf = [0u8; 4096];
        let mut total = 0;
        for _ in 0..32 {
            if total >= 32 * 1024 || *eof {
                break;
            }
            let remaining = (32 * 1024 - total).min(buf.len());
            match file.read(&mut buf[..remaining]) {
                Ok(0) => *eof = true,
                Ok(n) => {
                    total += n;
                    let keep = n.min((64 * 1024usize).saturating_sub(retained.len()));
                    retained.extend_from_slice(&buf[..keep]);
                    *discarded = discarded.saturating_add((n - keep) as u64);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn interrupted_reader_consumes_finite_attempts_without_fabricating_eof() {
            struct Interrupted {
                attempts: usize,
            }
            impl Read for Interrupted {
                fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                    self.attempts += 1;
                    Err(io::Error::from(io::ErrorKind::Interrupted))
                }
            }
            let mut reader = Interrupted { attempts: 0 };
            let mut bytes = vec![];
            let mut eof = false;
            let mut discarded = 0;
            drain(&mut reader, &mut bytes, &mut eof, &mut discarded).unwrap();
            assert_eq!(reader.attempts, 32);
            assert!(!eof);
            assert!(bytes.is_empty());
            assert_eq!(discarded, 0);
            // Uneven real-read sizes must not overrun the per-poll byte cap.
            struct Uneven {
                total: usize,
            }
            impl Read for Uneven {
                fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                    let n = if self.total == 0 { 1 } else { buf.len() };
                    buf[..n].fill(b'x');
                    self.total += n;
                    Ok(n)
                }
            }
            let mut reader = Uneven { total: 0 };
            drain(&mut reader, &mut bytes, &mut eof, &mut discarded).unwrap();
            assert_eq!(reader.total, 32 * 1024);
            assert_eq!(bytes.len(), 32 * 1024);
            assert!(!eof);
            assert_eq!(discarded, 0);
        }
    }
}
pub use guardian_fixture::{
    GuardianControllerFixture, GuardianControllerFixtureLaunch, GuardianEvent,
    GuardianRegistrationFault, GuardianSupervisorFixture, GuardianSupervisorProcess,
    GuardianWaitEligibility, GuardianWitness, guardian_controller_fixture_launch,
    take_guardian_controller_fixture, take_guardian_supervisor_fixture,
};
