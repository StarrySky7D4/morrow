//! Original-thread observation of a host-nominated local directory. Every open
//! shares the original approved IO context. Optional native confirmation only
//! narrows the original nomination; no guest can supply paths or a join proof.
#![deny(warnings)]
use super::{
    DirectoryRetirementSnapshot, native_windows,
    retirement::{Ledger, TrackedFile},
};
use crate::{
    fs_watch_binding::{self, WatchApproval},
    io_binding::{IoJobLease, IoResourceLease},
    manager::{ManagedInstance, Manager},
};
use morrow_core::{dispatch::HostRuntime, plugin_package::io::IoCapability};
use morrow_fs_watch_native_v1::windows::{self, NativeWatch, Window};
use morrow_fs_watch_v1::{
    self as wire,
    state::{ClientState, Phase},
    wire_digest,
};
use std::{sync::Arc, time::Duration};
use zeroize::Zeroizing;
#[allow(unsafe_code)]
mod native;
mod picker;

/// Unit fixture only: no flag, parameter or hook exists in a production build.
#[cfg(test)]
pub(crate) fn protect_next_original_leaf_for_close_fixture() {
    native::protect_next_original_leaf_for_close_fixture();
}

const CAPTURE_BYTES: u64 = 65536;
const FRAME_BYTES: u64 = 16 * 1024 + 3 * 65536 + 2 * 128 * 32 + 8 * 128;
const MAX_POLLS: u32 = 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Approval(fs_watch_binding::Error),
    Admission(crate::io_binding::Error),
    Directory(super::Error),
    Native(windows::Fault),
    RelativeOpen(i32),
    ParentOpen(Option<i32>, std::io::ErrorKind),
    Codec(wire::Error),
    /// This version captures a child below a real volume root, not the volume itself.
    VolumeRootUnsupported,
    SelectionMismatch,
    Entropy,
    Limit,
    Pending,
    Correlation,
    Closed,
    Unknown,
}
impl From<super::Error> for Error {
    fn from(e: super::Error) -> Self {
        Self::Directory(e)
    }
}
impl From<fs_watch_binding::Error> for Error {
    fn from(e: fs_watch_binding::Error) -> Self {
        Self::Approval(e)
    }
}
impl From<crate::io_binding::Error> for Error {
    fn from(e: crate::io_binding::Error) -> Self {
        Self::Admission(e)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "original directory watch: {self:?}")
    }
}
impl std::error::Error for Error {}
/// Sampling and admission execute under the same original clock guard. Native
/// work, parsing and destructors stay outside that guard; it supplies no grant.
pub trait WatchClock {
    fn with<T>(&mut self, step: impl FnOnce(u64) -> Result<T, Error>) -> Result<T, Error>;
}
struct Parent {
    file: TrackedFile,
    identity: Option<[u8; 24]>,
}
struct State {
    approval: WatchApproval,
    job: Option<IoJobLease>,
    leases: Vec<IoResourceLease>,
    parents: Vec<Parent>,
    ledger: Arc<Ledger>,
    native: Option<NativeWatch>,
    native_closed: Option<windows::Closed>,
    unknown_open: Option<native::UnknownOpen>,
    epoch: [u8; 32],
    client: Option<ClientState>,
    pending: Option<(u64, [u8; 32])>,
    first_error: Option<Error>,
    relative_opens: u32,
    polls: u32,
    retire_attempted: bool,
    retired: bool,
    unknown: bool,
}
impl State {
    fn authenticate(&self, m: &Manager, h: &HostRuntime, i: &ManagedInstance) -> Result<(), Error> {
        // A foreign caller must not sample this owner's clock, spend its quota,
        // or turn an otherwise usable original watch into a terminal failure.
        self.approval
            .original_binding()
            .validate_identity(m, h, i)
            .map_err(|e| Error::Approval(fs_watch_binding::Error::Binding(e)))
    }
    fn fail(&mut self, error: Error) -> Error {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
        error
    }
    fn usable(&self) -> Result<(), Error> {
        if self.unknown {
            Err(Error::Unknown)
        } else if self.retire_attempted || self.first_error.is_some() {
            Err(Error::Closed)
        } else {
            Ok(())
        }
    }
    fn check(
        &self,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
        bytes: u64,
    ) -> Result<(), Error> {
        c.with(|now| {
            self.approval.check(m, h, i, now)?;
            if bytes != 0 {
                self.job.as_ref().ok_or(Error::Closed)?.charge(
                    &[IoCapability::FileList],
                    bytes,
                    now,
                )?;
            }
            Ok(())
        })
    }
    fn verify(
        &self,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
    ) -> Result<(), Error> {
        for parent in &self.parents {
            self.check(m, h, i, c, 0)?;
            if Some(parent.file.with(native_windows::identity)?) != parent.identity {
                return Err(Error::Directory(super::Error::SourceChanged));
            }
            self.check(m, h, i, c, 0)?;
        }
        Ok(())
    }
}
/// No Send/Sync implementation: the original thread owns the native operation,
/// parent chain, kernel storage, first failure and every held resource/job slot.
pub struct DirectoryWatch {
    state: Option<Box<State>>,
}
impl std::fmt::Debug for DirectoryWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectoryWatch")
            .field("snapshot", &self.snapshot())
            .finish_non_exhaustive()
    }
}
pub struct StartFailure {
    pub error: Error,
    pub owner: Option<DirectoryWatch>,
}
impl std::fmt::Debug for StartFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StartFailure")
            .field("error", &self.error)
            .field("owner", &self.owner)
            .finish()
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub ancestors: DirectoryRetirementSnapshot,
    pub native: Option<windows::Snapshot>,
    pub held_resources: usize,
    pub job_held: bool,
    pub relative_open_calls: u32,
    pub polls: u32,
    pub consumer_phase: Option<Phase>,
    pub first_error: Option<Error>,
    pub unknown: bool,
    pub retired: bool,
}
pub enum Poll {
    Pending,
    DeadlineReached,
    Frame { sequence: u64, wire: Vec<u8> },
}
/// Positive original closure only. This is not a worker join or an assertion
/// that every filesystem change was observed. It cannot be cloned or decoded.
pub struct Closed {
    native: Option<windows::Closed>,
    ancestors: DirectoryRetirementSnapshot,
}
impl Closed {
    pub fn native_snapshot(&self) -> Option<windows::Snapshot> {
        self.native.as_ref().map(windows::Closed::snapshot)
    }
    pub fn ancestor_snapshot(&self) -> DirectoryRetirementSnapshot {
        self.ancestors
    }
}
impl DirectoryWatch {
    pub fn start(
        approval: WatchApproval,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
    ) -> Result<Self, StartFailure> {
        Self::start_with_cancellation(approval, m, h, i, c, || false)
    }
    pub(crate) fn start_with_cancellation(
        approval: WatchApproval,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
        stop: impl FnMut() -> bool,
    ) -> Result<Self, StartFailure> {
        let mut owner = Self::prepare(approval, m, h, i, c)?;
        let result = if owner
            .state
            .as_ref()
            .unwrap()
            .approval
            .requires_native_confirmation()
        {
            picker::run(&mut owner, m, h, i, c, stop)
        } else {
            owner.capture(m, h, i, c)
        };
        match result {
            Ok(()) => Ok(owner),
            Err(error) => {
                owner.state.as_mut().unwrap().fail(error);
                Err(StartFailure {
                    error,
                    owner: Some(owner),
                })
            }
        }
    }
    fn prepare(
        approval: WatchApproval,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
    ) -> Result<Self, StartFailure> {
        // Only the original owned nomination is read. No parameter can replace
        // the approved spelling, package, clock or instance during capture.
        let (_, path) = approval.nominated_path().clone().into_parts();
        let Some(path) = path else {
            return Err(StartFailure {
                error: Error::VolumeRootUnsupported,
                owner: None,
            });
        };
        let count = path.component_count();
        let cost = u64::from(approval.requires_native_confirmation())
            * super::native_picker::INPUT_BYTES
            + CAPTURE_BYTES
            + Ledger::OWNED_BYTES
            + NativeWatch::RESIDENT_BYTES as u64
            + path.retained_input_bytes() as u64
            + (count * count * 128) as u64;
        let job = match c.with(|now| {
            approval.check(m, h, i, now)?;
            let b = approval.original_binding();
            b.admit_job_authenticated(cost, b.watch_job_ceiling(), now)
                .map_err(Error::from)
        }) {
            Ok(job) => job,
            Err(error) => return Err(StartFailure { error, owner: None }),
        };
        let ledger = Ledger::new();
        ledger.bind_worker();
        let owner = Self {
            state: Some(Box::new(State {
                approval,
                job: Some(job),
                leases: Vec::new(),
                parents: Vec::new(),
                ledger,
                native: None,
                native_closed: None,
                unknown_open: None,
                epoch: [0; 32],
                client: None,
                pending: None,
                first_error: None,
                relative_opens: 0,
                polls: 0,
                retire_attempted: false,
                retired: false,
                unknown: false,
            })),
        };
        Ok(owner)
    }
    fn capture(
        &mut self,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
    ) -> Result<(), Error> {
        let s = self.state.as_mut().unwrap();
        let (drive, path) = s.approval.nominated_path().clone().into_parts();
        let path = path.ok_or(Error::VolumeRootUnsupported)?;
        let count = path.component_count();
        let resources = count + 2; // volume/parents, leaf, event
        s.leases
            .try_reserve_exact(resources)
            .map_err(|_| Error::Limit)?;
        s.parents
            .try_reserve_exact(count)
            .map_err(|_| Error::Limit)?;
        for _ in 0..resources {
            let lease = c.with(|now| {
                s.approval.check(m, h, i, now)?;
                s.approval
                    .original_binding()
                    .admit_resource(m, h, i, &[IoCapability::FileList], now)
                    .map_err(Error::from)
            })?;
            s.leases.push(lease);
        }
        let mut random = Zeroizing::new([0u8; 32]);
        getrandom::fill(&mut random[..]).map_err(|_| Error::Entropy)?;
        if *random == [0; 32] {
            return Err(Error::Entropy);
        }
        s.epoch = *random;
        s.client = Some(
            ClientState::new(
                s.epoch,
                wire::state::Limits {
                    frames: s.approval.limits().max_frames,
                    changes: wire::state::MAX_TOTAL_CHANGES,
                    wire_bytes: s.approval.limits().max_wire_bytes,
                },
            )
            .map_err(Error::Codec)?,
        );
        s.check(m, h, i, c, 0)?;
        let root = s.ledger.track(native_windows::open_volume_root(drive)?);
        s.parents.push(Parent {
            file: root,
            identity: None,
        });
        s.parents[0].file.with(native_windows::verify_volume_root)?;
        s.parents[0].identity = Some(s.parents[0].file.with(native_windows::identity)?);
        s.check(m, h, i, c, 0)?;
        for component in &path.as_components()[..count - 1] {
            s.verify(m, h, i, c)?;
            let opened = s.parents.last().unwrap().file.with(|parent| {
                Ok(crate::file_target::open_relative_directory(
                    parent, component, false,
                ))
            })?;
            let file =
                opened.map_err(|error| Error::ParentOpen(error.raw_os_error(), error.kind()))?;
            let file = s.ledger.track(file);
            s.parents.push(Parent {
                file,
                identity: None,
            });
            let last = s.parents.last_mut().unwrap();
            last.identity = Some(last.file.with(native_windows::identity)?);
            s.check(m, h, i, c, 0)?;
        }
        s.verify(m, h, i, c)?;
        s.relative_opens = 1; // latch on the original owner before dispatch
        let opened = s
            .parents
            .last()
            .unwrap()
            .file
            .with(|parent| native::open(parent, &path.as_components()[count - 1]))?;
        match opened {
            native::Opened::File(file) => {
                // No fallible check or implicit File destructor between
                // native capture and the original asynchronous owner.
                let window = Window {
                    epoch: s.epoch,
                    max_frames: s.approval.limits().max_frames,
                    duration: s.approval.remaining_duration(),
                };
                match NativeWatch::open(file, window) {
                    Ok(watch) => s.native = Some(watch),
                    Err(failure) => {
                        s.native = Some(failure.owner);
                        return Err(Error::Native(failure.reason));
                    }
                }
            }
            native::Opened::Refused(status) => return Err(Error::RelativeOpen(status)),
            native::Opened::Unknown(status, pending) => {
                s.unknown_open = Some(pending);
                s.unknown = true;
                return Err(Error::RelativeOpen(status));
            }
        }
        s.verify(m, h, i, c)?;
        s.check(m, h, i, c, 0)
    }
    pub fn snapshot(&self) -> Snapshot {
        let s = self.state.as_ref().expect("original watch owner");
        Snapshot {
            ancestors: s.ledger.snapshot(),
            native: s.native.as_ref().map(NativeWatch::snapshot),
            held_resources: s.leases.len(),
            job_held: s.job.is_some(),
            relative_open_calls: s.relative_opens,
            polls: s.polls,
            consumer_phase: s.client.as_ref().map(ClientState::phase),
            first_error: s.first_error,
            unknown: s.unknown,
            retired: s.retired,
        }
    }
    pub fn epoch(&self) -> [u8; 32] {
        self.state.as_ref().expect("original watch owner").epoch
    }
    pub(crate) fn usage(&self) -> crate::io_binding::Usage {
        self.state.as_ref().unwrap().approval.usage()
    }
    /// Authenticate the exact pending original frame before and after guest
    /// execution. Additional guest wire copies spend this same original job.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn guest_boundary(
        &mut self,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
        sequence: u64,
        digest: [u8; 32],
        bytes: u64,
    ) -> Result<(), Error> {
        let s = self.state.as_mut().unwrap();
        s.authenticate(m, h, i)?;
        s.usable()?;
        if s.pending != Some((sequence, digest)) {
            return Err(Error::Correlation);
        }
        let result = s
            .check(m, h, i, c, bytes)
            .and_then(|()| s.verify(m, h, i, c));
        result.map_err(|error| s.fail(error))
    }
    pub fn arm(
        &mut self,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
    ) -> Result<(), Error> {
        let s = self.state.as_mut().unwrap();
        s.authenticate(m, h, i)?;
        s.usable()?;
        if s.pending.is_some()
            || s.native
                .as_ref()
                .is_some_and(|native| native.snapshot().read_in_flight)
        {
            return Err(Error::Pending);
        }
        let result = (|| {
            s.check(m, h, i, c, FRAME_BYTES)?;
            s.verify(m, h, i, c)?;
            s.native
                .as_mut()
                .ok_or(Error::Closed)?
                .arm()
                .map_err(Error::Native)?;
            s.check(m, h, i, c, 0)
        })();
        result.map_err(|e| s.fail(e))
    }
    pub fn poll(
        &mut self,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
        wait: Duration,
    ) -> Result<Poll, Error> {
        let s = self.state.as_mut().unwrap();
        s.authenticate(m, h, i)?;
        s.usable()?;
        if s.pending.is_some() {
            return Err(Error::Pending);
        }
        if wait > windows::MAX_POLL || s.polls == MAX_POLLS {
            return Err(Error::Limit);
        }
        let result = (|| {
            s.check(m, h, i, c, 128)?;
            s.verify(m, h, i, c)?;
            s.polls += 1;
            let result = s
                .native
                .as_mut()
                .ok_or(Error::Closed)?
                .poll(wait)
                .map_err(Error::Native)?;
            s.verify(m, h, i, c)?;
            s.check(m, h, i, c, 0)?;
            match result {
                windows::Poll::Pending => Ok(Poll::Pending),
                windows::Poll::DeadlineReached => Ok(Poll::DeadlineReached),
                windows::Poll::Frame { sequence, wire } => {
                    let client = s.client.as_mut().ok_or(Error::Closed)?;
                    let decoded = client.receive(&wire).map_err(Error::Codec)?;
                    if decoded.sequence != sequence {
                        s.unknown = true;
                        return Err(Error::Correlation);
                    }
                    s.pending = Some((sequence, wire_digest(&wire)));
                    Ok(Poll::Frame { sequence, wire })
                }
            }
        })();
        result.map_err(|e| s.fail(e))
    }
    /// Acknowledges delivery only. It never closes native resources or proves a join.
    pub fn acknowledge(
        &mut self,
        m: &Manager,
        h: &HostRuntime,
        i: &ManagedInstance,
        c: &mut impl WatchClock,
        sequence: u64,
        digest: [u8; 32],
    ) -> Result<(), Error> {
        let s = self.state.as_mut().unwrap();
        s.authenticate(m, h, i)?;
        s.usable()?;
        s.check(m, h, i, c, 0)?;
        if s.pending != Some((sequence, digest)) {
            return Err(Error::Correlation);
        }
        let result = (|| {
            s.native
                .as_mut()
                .ok_or(Error::Closed)?
                .acknowledge(sequence, digest)
                .map_err(Error::Native)?;
            s.client
                .as_mut()
                .ok_or(Error::Closed)?
                .acknowledge(sequence, digest)
                .map_err(Error::Codec)?;
            s.pending = None;
            Ok(())
        })();
        result.map_err(|e| {
            s.unknown = true;
            s.fail(e)
        })
    }
    /// One explicit original cleanup attempt, even after authority expired.
    /// No fresh grant is needed to close the original owned resources. Lost
    /// delivery and ambiguous native cleanup remain charged after physical close.
    pub fn retire(&mut self) -> Result<Closed, Error> {
        let s = self.state.as_mut().unwrap();
        if s.retire_attempted {
            return Err(if s.unknown {
                Error::Unknown
            } else {
                Error::Closed
            });
        }
        s.retire_attempted = true;
        if s.unknown || s.unknown_open.is_some() {
            s.unknown = true;
            return Err(s.fail(Error::Unknown));
        }
        let lost = s.pending.is_some();
        if lost {
            s.client.as_mut().unwrap().transport_lost();
        }
        if let Some(native) = &mut s.native {
            match native.retire(windows::MAX_CANCEL_WAIT) {
                Ok(proof) => s.native_closed = Some(proof),
                Err(error) => {
                    s.unknown = true;
                    return Err(s.fail(Error::Native(error)));
                }
            }
        }
        while let Some(parent) = s.parents.pop() {
            drop(parent);
            if s.ledger.snapshot().unknown != 0 {
                s.unknown = true;
                return Err(s.fail(Error::Unknown));
            }
        }
        if !s.ledger.snapshot().all_closed() {
            s.unknown = true;
            return Err(s.fail(Error::Unknown));
        }
        if lost
            || s.client
                .as_ref()
                .is_some_and(|client| client.phase() == Phase::Unknown)
        {
            s.unknown = true;
            return Err(s.fail(Error::Unknown));
        }
        let proof = Closed {
            native: s.native_closed.take(),
            ancestors: s.ledger.snapshot(),
        };
        s.leases.clear();
        s.job.take();
        s.retired = true;
        Ok(proof)
    }
}
impl Drop for DirectoryWatch {
    fn drop(&mut self) {
        if let Some(state) = self.state.take()
            && !state.retired
        {
            // Do not run TrackedFile or budget destructors for an unproved original
            // retirement. Stable kernel storage and the exact charge remain held.
            let _ = Box::leak(state);
        }
    }
}
