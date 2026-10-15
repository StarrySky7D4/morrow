//! Bounded owned named-pipe connect helper for the elevated Windows sandbox runner.
//!
//! Each connect worker clones its own `File` and pins a cloned process `OwnedHandle`
//! before spawning. Parent observation waits on the retained `JoinHandle` native
//! thread `HANDLE` (`WaitForSingleObject`) before calling `JoinHandle::join`, ensuring
//! native thread and TLS teardown have completed before the worker is considered joined.

use super::checked_owner::{OriginalProcessIdentity, StartupId};
use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{GetLastError, HANDLE};
use windows_sys::Win32::System::IO::CancelSynchronousIo;
use windows_sys::Win32::System::Threading::{WaitForMultipleObjects, WaitForSingleObject};

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const WAIT_OBJECT_0: u32 = 0;
const WAIT_TIMEOUT: u32 = 258;
const WAIT_FAILED: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct AttemptId(pub(crate) u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CancelObservation {
    Requested,
    Failed(u32), // Includes ERROR_NOT_FOUND unchanged; neither case proves completion.
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnknownReason {
    Deadline,
    WaitFailed(u32),
    UnexpectedWait(u32),
    GoNotDelivered,
    OriginalRunnerExited,
    OriginalRunnerWaitUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Observation {
    RetainedUnknown {
        reason: UnknownReason,
        cancel: CancelObservation,
    },
    Joined {
        result: Result<(), String>,
        within_deadline: bool,
        prior_unknown: Option<(UnknownReason, CancelObservation)>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JoinedErrorMeta {
    pub(crate) kind: io::ErrorKind,
    pub(crate) raw_os_error: Option<i32>,
}

struct WorkerExit {
    finished_at: Instant,
    result: io::Result<()>,
}

struct Pending {
    worker: JoinHandle<WorkerExit>, // Owns native HANDLE used for both wait and cancel.
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    unknown: Option<(UnknownReason, CancelObservation)>,
}

enum Phase {
    Pending(Pending),
    Joined {
        observation: Observation,
        error_meta: Option<JoinedErrorMeta>,
    },
}

struct Active {
    id: AttemptId,
    startup_id: Option<StartupId>,
    // Pin the original runner object; never reopen it using a PID.
    _runner_pin: OwnedHandle,
    phase: Option<Phase>, // Taken ONLY after the native thread has signaled.
}

struct Slot {
    next_id: u64,
    reserved_startup: Option<StartupId>,
    active: Option<Active>,
}

pub(crate) struct Owner {
    slot: Mutex<Slot>,
}

struct RunnerWatch<'a> {
    process: &'a OwnedHandle,
    identity: OriginalProcessIdentity,
    startup_id: StartupId,
    diagnostic: Option<&'a crate::WindowsStartDiagnostic>,
}
impl RunnerWatch<'_> {
    fn observe(&self) -> Option<UnknownReason> {
        let observed = self.identity.observe_startup_process(self.process, self.startup_id);
        if let Some(diagnostic) = self.diagnostic { diagnostic.observe_original_runner(observed); }
        match observed.state {
            crate::WindowsStartRunnerState::RunningAtObservation => None,
            crate::WindowsStartRunnerState::Exited { .. }
                | crate::WindowsStartRunnerState::SignaledExitQueryFailed { .. } => Some(UnknownReason::OriginalRunnerExited),
            _ => Some(UnknownReason::OriginalRunnerWaitUnavailable),
        }
    }
}

impl Owner {
    pub(crate) const fn new() -> Self {
        Self {
            slot: Mutex::new(Slot {
                next_id: 1,
                reserved_startup: None,
                active: None,
            }),
        }
    }

    pub(crate) fn reserve_for_startup(&self, startup_id: StartupId) -> io::Result<()> {
        let mut slot = self
            .slot
            .try_lock()
            .map_err(|_| io::Error::other("connect owner unavailable"))?;
        if slot.active.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "connect lease occupied",
            ));
        }
        if let Some(existing) = slot.reserved_startup
            && existing != startup_id
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "connect lane reserved by another startup",
            ));
        }
        slot.reserved_startup = Some(startup_id);
        Ok(())
    }

    pub(crate) fn release_startup_reservation(&self, startup_id: StartupId) -> io::Result<()> {
        let mut slot = self
            .slot
            .try_lock()
            .map_err(|_| io::Error::other("connect owner unavailable"))?;
        if let Some(active) = &slot.active
            && active.startup_id == Some(startup_id)
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "connect helper still active; reservation remains charged",
            ));
        }
        if slot.reserved_startup == Some(startup_id) {
            slot.reserved_startup = None;
        }
        Ok(())
    }

    pub(crate) fn begin_with<F>(
        &self,
        pipe: &File,
        runner: &OwnedHandle,
        deadline: Instant,
        connect: F,
    ) -> io::Result<AttemptId>
    where
        F: FnOnce(&File) -> io::Result<()> + Send + 'static,
    {
        self.begin_with_for_startup(None, pipe, runner, deadline, connect)
    }

    pub(crate) fn begin_with_for_startup<F>(
        &self,
        startup_id: Option<StartupId>,
        pipe: &File,
        runner: &OwnedHandle,
        deadline: Instant,
        connect: F,
    ) -> io::Result<AttemptId>
    where
        F: FnOnce(&File) -> io::Result<()> + Send + 'static,
    {
        // No wait for another caller's lock; poisoned/busy admission fails closed.
        let mut slot = self
            .slot
            .try_lock()
            .map_err(|_| io::Error::other("connect owner unavailable"))?;
        if slot.active.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "connect lease occupied",
            ));
        }
        if let Some(reserved) = slot.reserved_startup
            && startup_id != Some(reserved)
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "connect lane reserved by another startup",
            ));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "connect deadline expired before spawn",
            ));
        }
        let id = AttemptId(slot.next_id);
        slot.next_id = slot
            .next_id
            .checked_add(1)
            .ok_or_else(|| io::Error::other("attempt id exhausted"))?;
        // Admission is reserved by this lock before any handle/thread allocation.
        let owned_pipe = pipe.try_clone()?;
        let runner_pin = runner.try_clone()?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (go_tx, go_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("codex-owned-pipe-connect".into())
            .spawn(move || {
                let result = match go_rx
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                {
                    Ok(())
                        if Instant::now() < deadline
                            && !worker_cancelled.load(Ordering::Acquire) =>
                    {
                        // HANDLE is borrowed ONLY inside the owned File's lifetime.
                        // Cancellation can race after this check; retention covers that race.
                        let res = connect(&owned_pipe);
                        if res.is_ok()
                            && (Instant::now() >= deadline
                                || worker_cancelled.load(Ordering::Acquire))
                        {
                            Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "connect completed after deadline or cancellation",
                            ))
                        } else {
                            res
                        }
                    }
                    _ => Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "connect not admitted before deadline",
                    )),
                };
                drop(owned_pipe);
                WorkerExit {
                    finished_at: Instant::now(),
                    result,
                }
            })?;
        // No fallible return between spawn and installation. No raw-handle publication.
        slot.active = Some(Active {
            id,
            startup_id,
            _runner_pin: runner_pin,
            phase: Some(Phase::Pending(Pending {
                worker,
                cancelled,
                deadline,
                unknown: None,
            })),
        });
        let go_failed = go_tx.try_send(()).is_err();
        let post_go_expired = Instant::now() >= deadline;
        if (go_failed || post_go_expired)
            && let Some(active) = slot.active.as_mut()
            && let Some(Phase::Pending(pending)) = active.phase.as_mut()
        {
            let reason = if go_failed {
                UnknownReason::GoNotDelivered
            } else {
                UnknownReason::Deadline
            };
            mark_unknown(pending, reason);
        }
        drop(go_tx);
        Ok(id)
    }

    pub(crate) fn observe(&self, id: AttemptId, wait_until: Instant) -> io::Result<Observation> {
        self.observe_detailed(id, wait_until).map(|(obs, _)| obs)
    }

    pub(crate) fn observe_detailed(
        &self,
        id: AttemptId,
        wait_until: Instant,
    ) -> io::Result<(Observation, Option<JoinedErrorMeta>)> {
        self.observe_detailed_inner(id, wait_until, None)
    }

    /// v8 startup only: observe the original owner's borrowed HPROCESS and
    /// helper thread together. Early runner exit is Unknown, never a child exit
    /// receipt or permission to release either original owner/lease.
    pub(crate) fn observe_detailed_with_runner(
        &self, id: AttemptId, wait_until: Instant, process: &OwnedHandle,
        identity: OriginalProcessIdentity, startup_id: StartupId,
        diagnostic: Option<&crate::WindowsStartDiagnostic>,
    ) -> io::Result<(Observation, Option<JoinedErrorMeta>)> {
        identity.verify_same_process(process)?;
        self.observe_detailed_inner(id, wait_until, Some(RunnerWatch { process, identity, startup_id, diagnostic }))
    }

    fn observe_detailed_inner(
        &self, id: AttemptId, wait_until: Instant, watch: Option<RunnerWatch<'_>>,
    ) -> io::Result<(Observation, Option<JoinedErrorMeta>)> {
        let mut slot = self
            .slot
            .try_lock()
            .map_err(|_| io::Error::other("connect owner unavailable; resources retained"))?;
        let active = slot
            .active
            .as_mut()
            .filter(|active| active.id == id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "attempt not owned"))?;
        if watch.as_ref().is_some_and(|watch| active.startup_id != Some(watch.startup_id)) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "original runner watch belongs to another startup"));
        }
        if let Some(watch) = &watch {
            // Scalar startup equality alone cannot substitute a different
            // process. Both retained original handles must name the same PID/FT.
            watch.identity.verify_same_process(&active._runner_pin)?;
        }
        let phase = active
            .phase
            .as_mut()
            .ok_or_else(|| io::Error::other("connect phase missing"))?;
        let pending = match phase {
            Phase::Joined {
                observation,
                error_meta,
            } => return Ok((observation.clone(), error_meta.clone())),
            Phase::Pending(pending) => pending,
        };
        if let Some(reason) = watch.as_ref().and_then(RunnerWatch::observe) {
            return Ok(retain_runner_unknown(pending, reason));
        }
        // Follow-up reap calls use their own bounded observation deadline. They do
        // not extend the original connect deadline or make a late success timely.
        let observation_cap = Instant::now()
            .checked_add(CONNECT_TIMEOUT)
            .ok_or_else(|| io::Error::other("observation deadline overflow"))?;
        let wait_until = wait_until.min(observation_cap);
        let effective_deadline = if pending.unknown.is_some() {
            wait_until
        } else {
            wait_until.min(pending.deadline)
        };
        if Instant::now() >= effective_deadline {
            mark_unknown(pending, UnknownReason::Deadline);
        }
        let wait_ms = finite_wait_ms(effective_deadline.saturating_duration_since(Instant::now()));
        let wait = if let Some(watch) = &watch {
            let handles = [pending.worker.as_raw_handle() as HANDLE, watch.process.as_raw_handle() as HANDLE];
            unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, wait_ms) }
        } else {
            unsafe { WaitForSingleObject(pending.worker.as_raw_handle() as HANDLE, wait_ms) }
        };
        let wait_error = if wait == WAIT_FAILED {
            Some(unsafe { GetLastError() })
        } else {
            None
        };
        let observed_at = Instant::now();
        if let Some(watch) = &watch {
            // Preserve the first original wait/deadline failure. A later zero
            // probe may add runner history but cannot reclassify that failure.
            if wait == WAIT_FAILED {
                mark_unknown(pending, UnknownReason::WaitFailed(wait_error.unwrap_or(0)));
            } else if wait == WAIT_TIMEOUT {
                mark_unknown(pending, UnknownReason::Deadline);
            } else if wait != WAIT_OBJECT_0 && wait != WAIT_OBJECT_0 + 1 {
                mark_unknown(pending, UnknownReason::UnexpectedWait(wait));
            }
            if observed_at >= effective_deadline || observed_at >= pending.deadline {
                mark_unknown(pending, UnknownReason::Deadline);
            }
            // Capture after every native wait, even if the original outer caller
            // has already timed out. No global process lookup or handle reopen.
            if let Some(reason) = watch.observe() {
                return Ok(retain_runner_unknown(pending, reason));
            }
            if wait == WAIT_OBJECT_0 + 1 {
                return Ok(retain_runner_unknown(pending, UnknownReason::OriginalRunnerWaitUnavailable));
            }
        }
        if wait == WAIT_OBJECT_0 {
            if observed_at >= effective_deadline || observed_at >= pending.deadline {
                mark_unknown(pending, UnknownReason::Deadline);
            }
            // Native thread termination, including TLS destruction, precedes join.
            // Result delivery / CancelSynchronousIo / is_finished alone is not used.
            let Some(Phase::Pending(pending)) = active.phase.take() else {
                return Err(io::Error::other("connect pending phase missing"));
            };
            let original_deadline = pending.deadline;
            let mut prior_unknown = pending.unknown;
            let (result, error_meta, completed_at) = match pending.worker.join() {
                Ok(exit) => match exit.result {
                    Ok(()) => (Ok(()), None, Some(exit.finished_at)),
                    Err(err) => {
                        let meta = JoinedErrorMeta {
                            kind: err.kind(),
                            raw_os_error: err.raw_os_error(),
                        };
                        (Err(err.to_string()), Some(meta), Some(exit.finished_at))
                    }
                },
                Err(_) => (
                    Err("connect helper panicked".to_owned()),
                    Some(JoinedErrorMeta {
                        kind: io::ErrorKind::Other,
                        raw_os_error: None,
                    }),
                    None,
                ),
            };
            let joined_at = Instant::now();
            if joined_at >= effective_deadline
                || joined_at >= original_deadline
                || completed_at.is_none_or(|completed| completed >= original_deadline)
            {
                if prior_unknown.is_none() {
                    prior_unknown = Some((
                        UnknownReason::Deadline,
                        CancelObservation::Failed(
                            windows_sys::Win32::Foundation::ERROR_NOT_FOUND,
                        ),
                    ));
                }
            }
            let within_deadline = prior_unknown.is_none()
                && observed_at < effective_deadline
                && observed_at < original_deadline
                && joined_at < effective_deadline
                && joined_at < original_deadline
                && completed_at.is_some_and(|completed| completed < original_deadline);
            let observation = Observation::Joined {
                result,
                within_deadline,
                prior_unknown,
            };
            active.phase = Some(Phase::Joined {
                observation: observation.clone(),
                error_meta: error_meta.clone(),
            });
            return Ok((observation, error_meta));
        }
        let reason = match wait {
            WAIT_TIMEOUT => UnknownReason::Deadline,
            WAIT_FAILED => UnknownReason::WaitFailed(wait_error.unwrap_or(0)),
            other => UnknownReason::UnexpectedWait(other),
        };
        mark_unknown(pending, reason);
        let Some((reason, cancel)) = pending.unknown.clone() else {
            return Err(io::Error::other("unknown state missing"));
        };
        Ok((Observation::RetainedUnknown { reason, cancel }, None))
    }

    pub(crate) fn acknowledge_joined(&self, id: AttemptId) -> io::Result<()> {
        let mut slot = self
            .slot
            .try_lock()
            .map_err(|_| io::Error::other("connect owner unavailable"))?;
        let active = slot
            .active
            .as_ref()
            .filter(|active| active.id == id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "attempt not owned"))?;
        if !matches!(active.phase, Some(Phase::Joined { .. })) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "helper not joined; lease remains charged",
            ));
        }
        // This releases only the helper lease. It does not clear a startup noReplay seal.
        slot.active = None;
        Ok(())
    }
}

fn retain_runner_unknown(pending: &mut Pending, reason: UnknownReason)
    -> (Observation, Option<JoinedErrorMeta>)
{
    // Existing bounded cancellation applies only to this original helper's
    // synchronous IO. It proves neither join nor process/resource cleanup.
    // Keep the same JoinHandle, process pin, occupied lane and sticky reason.
    mark_unknown(pending, reason);
    let (reason, cancel) = pending.unknown.clone().expect("unknown was retained");
    (Observation::RetainedUnknown { reason, cancel }, None)
}

fn mark_unknown(pending: &mut Pending, reason: UnknownReason) {
    if pending.unknown.is_some() {
        return;
    }
    pending.cancelled.store(true, Ordering::Release);
    let ok = unsafe { CancelSynchronousIo(pending.worker.as_raw_handle() as HANDLE) };
    let cancel = if ok != 0 {
        CancelObservation::Requested
    } else {
        CancelObservation::Failed(unsafe { GetLastError() })
    };
    pending.unknown = Some((reason, cancel)); // No retry and no completion inference.
}

pub(crate) fn finite_wait_ms(duration: Duration) -> u32 {
    // Round up sub-millisecond waits; never pass INFINITE (u32::MAX).
    duration
        .as_millis()
        .saturating_add(u128::from(duration.subsec_nanos() % 1_000_000 != 0))
        .min(u128::from(u32::MAX - 1)) as u32
}

// Statics have process lifetime and are not dropped on a startup Err/unwind.
// Exactly one worker can be charged. Unknown blocks admission across callers.
pub(crate) static CONNECT_OWNER: Owner = Owner::new();

pub(crate) fn reserve_for_startup(startup_id: StartupId) -> io::Result<()> {
    CONNECT_OWNER.reserve_for_startup(startup_id)
}

pub(crate) fn release_startup_reservation(startup_id: StartupId) -> io::Result<()> {
    CONNECT_OWNER.release_startup_reservation(startup_id)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn begin(pipe: &File, runner: &OwnedHandle, expected_pid: u32) -> io::Result<AttemptId> {
    let deadline = Instant::now()
        .checked_add(CONNECT_TIMEOUT)
        .ok_or_else(|| io::Error::other("deadline overflow"))?;
    begin_until(None, pipe, runner, expected_pid, deadline)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn begin_until(
    startup_id: Option<StartupId>,
    pipe: &File,
    runner: &OwnedHandle,
    expected_pid: u32,
    deadline: Instant,
) -> io::Result<AttemptId> {
    CONNECT_OWNER.begin_with_for_startup(startup_id, pipe, runner, deadline, move |owned_pipe| {
        crate::runner_pipe::connect_pipe(owned_pipe.as_raw_handle() as HANDLE, expected_pid)
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn observe(id: AttemptId, wait_until: Instant) -> io::Result<Observation> {
    CONNECT_OWNER.observe(id, wait_until)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn observe_detailed(
    id: AttemptId,
    wait_until: Instant,
) -> io::Result<(Observation, Option<JoinedErrorMeta>)> {
    CONNECT_OWNER.observe_detailed(id, wait_until)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn acknowledge_joined(id: AttemptId) -> io::Result<()> {
    CONNECT_OWNER.acknowledge_joined(id)
}

#[cfg(test)]
#[path = "owned_connect_worker_tests.rs"]
mod tests;
