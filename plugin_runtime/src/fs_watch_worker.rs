//! One approved observation window on the complete original host owner.
//! A guest echoes the exact Cap'n Proto batch to acknowledge delivery only.
//! Native closure, thread join, disconnection and maintenance are independent.
#![deny(warnings)]
use crate::{
    Cancellation, Fault, Report,
    directory_io::watch::{self, DirectoryWatch, WatchClock},
    fs_watch_binding::{self, WatchApproval},
    io_binding::Usage,
    io_jobs::{JobError, ManagedHostOwner},
    manager::ManagedInstance,
};
use morrow_fs_watch_v1::wire_digest;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Approval(fs_watch_binding::Error),
    Native(watch::Error),
    Guest(Fault),
    Owner,
    Protocol,
    /// The guest received a terminal observation. A gap is not a full snapshot,
    /// automatic rescan, renewed window or permission to replay missed events.
    ObservationEnded(morrow_fs_watch_v1::End),
    Cancelled,
    Expired,
    Spawn,
    Unknown,
    Consumed,
}
impl From<watch::Error> for Error {
    fn from(error: watch::Error) -> Self {
        Self::Native(error)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    Observing,
    Finishing,
    Finished,
}
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub phase: Phase,
    pub native: Option<watch::Snapshot>,
    pub delivered: u32,
    pub fuel_remaining: u64,
    pub task_calls_remaining: u32,
}
/// Historical original-guest delivery. It supplies no authority to act on a name.
pub struct Delivery {
    pub sequence: u64,
    pub digest: [u8; 32],
    pub execution: Report,
    wire: Zeroizing<Vec<u8>>,
}
impl Delivery {
    pub fn bytes(&self) -> &[u8] {
        &self.wire
    }
}
/// Constructed only after the original worker's explicit retirement attempt.
/// A successful join cannot manufacture this proof.
pub struct NativeRetirement {
    complete: bool,
    proof: Option<watch::Closed>,
}
impl NativeRetirement {
    pub fn is_complete(&self) -> bool {
        self.complete
    }
    pub fn closed(&self) -> Option<&watch::Closed> {
        self.proof.as_ref()
    }
}
pub struct Exit<O: ManagedHostOwner> {
    /// None on Unknown: the complete original owner remains retained on its
    /// original thread's allocation, with no maintenance, replay or adoption.
    pub owner: Option<O>,
    pub instance: Option<ManagedInstance>,
    pub result: Result<(), Error>,
    pub unknown: bool,
    pub native_retirement: NativeRetirement,
    pub disconnect: Option<Result<(), JobError>>,
    pub maintenance: Option<Result<(), JobError>>,
    pub snapshot: Snapshot,
    pub usage: Usage,
    pub deliveries: Vec<Delivery>,
    pub last_execution: Option<Report>,
}
pub struct SpawnFailure<O: ManagedHostOwner> {
    pub owner: O,
    pub instance: ManagedInstance,
    pub approval: WatchApproval,
    pub error: Error,
}
impl<O: ManagedHostOwner> std::fmt::Debug for SpawnFailure<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchSpawnFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
/// No commands, retries, replacement bindings or native objects cross threads.
/// Drop requests cancellation; it does not assert original thread closure.
pub struct WatchWorker<O: ManagedHostOwner> {
    join: Option<JoinHandle<Exit<O>>>,
    cancel: Cancellation,
    progress: Arc<Mutex<Snapshot>>,
}
struct Clock<C>(C);
impl<C: FnMut() -> u64> WatchClock for Clock<C> {
    fn with<T>(
        &mut self,
        step: impl FnOnce(u64) -> Result<T, watch::Error>,
    ) -> Result<T, watch::Error> {
        step((self.0)())
    }
}
fn validate<O: ManagedHostOwner>(
    owner: &O,
    instance: &ManagedInstance,
    approval: &WatchApproval,
) -> Result<(), Error> {
    approval
        .original_binding()
        .validate_identity(
            owner.manager().ok_or(Error::Owner)?,
            owner.runtime(),
            instance,
        )
        .map_err(|error| Error::Approval(fs_watch_binding::Error::Binding(error)))
}
fn cancelled(cancel: &Cancellation) -> Result<(), Error> {
    match cancel.fault() {
        None => Ok(()),
        Some(Fault::Deadline) => Err(Error::Expired),
        Some(_) => Err(Error::Cancelled),
    }
}
fn picker_stopped(first: &mut Option<Fault>, cancel: &Cancellation) -> bool {
    // The native callback needs a stop bit, but its first original cause must
    // survive the modal close. Never resample it after that close returns.
    if first.is_none() {
        *first = cancel.fault();
    }
    first.is_some()
}
fn capture_failure(error: watch::Error, stop: Option<Fault>) -> Error {
    match (error, stop) {
        (watch::Error::Directory(crate::directory_io::Error::Cancelled), Some(Fault::Deadline)) => {
            Error::Expired
        }
        (
            watch::Error::Directory(crate::directory_io::Error::Cancelled),
            Some(Fault::Cancelled),
        ) => Error::Cancelled,
        // A native refusal, mismatch or Unknown is never replaced by a later
        // cancellation/deadline, and a user-closed dialog has no sampled cause.
        (error, _) => Error::Native(error),
    }
}
struct Work {
    watch: Option<DirectoryWatch>,
    capture_entered: bool,
    capture_returned: bool,
    usage: Usage,
    deliveries: Vec<Delivery>,
    execution: Option<Report>,
    fuel: u64,
    calls: u32,
}
impl Work {
    fn publish(&self, progress: &Mutex<Snapshot>, phase: Phase) {
        *progress.lock().unwrap_or_else(|error| error.into_inner()) = Snapshot {
            phase,
            native: self.watch.as_ref().map(DirectoryWatch::snapshot),
            delivered: self.deliveries.len() as u32,
            fuel_remaining: self.fuel,
            task_calls_remaining: self.calls,
        };
    }
    #[allow(clippy::too_many_arguments)]
    fn execute<O: ManagedHostOwner, C: FnMut() -> u64>(
        &mut self,
        owner: &mut O,
        instance: &ManagedInstance,
        approval: &mut Option<WatchApproval>,
        clock: &mut Clock<C>,
        cancel: &Cancellation,
        progress: &Mutex<Snapshot>,
    ) -> Result<(), Error> {
        cancelled(cancel)?;
        validate(owner, instance, approval.as_ref().unwrap())?;
        owner.prepare_io().map_err(|_| Error::Owner)?;
        validate(owner, instance, approval.as_ref().unwrap())?;
        cancelled(cancel)?;
        let frames = approval.as_ref().unwrap().limits().max_frames;
        self.deliveries.reserve_exact(frames as usize);
        self.capture_entered = true;
        let mut picker_stop = None;
        let started = DirectoryWatch::start_with_cancellation(
            approval.take().unwrap(),
            owner.manager().ok_or(Error::Owner)?,
            owner.runtime(),
            instance,
            clock,
            || picker_stopped(&mut picker_stop, cancel),
        );
        self.capture_returned = true;
        match started {
            Ok(watch) => self.watch = Some(watch),
            Err(failure) => {
                self.watch = failure.owner;
                return Err(capture_failure(failure.error, picker_stop));
            }
        }
        for _ in 0..frames {
            cancelled(cancel)?;
            self.watch.as_mut().unwrap().arm(
                owner.manager().ok_or(Error::Owner)?,
                owner.runtime(),
                instance,
                clock,
            )?;
            self.publish(progress, Phase::Observing);
            let (sequence, input) = loop {
                cancelled(cancel)?;
                match self.watch.as_mut().unwrap().poll(
                    owner.manager().ok_or(Error::Owner)?,
                    owner.runtime(),
                    instance,
                    clock,
                    Duration::from_millis(25),
                )? {
                    watch::Poll::Pending => (),
                    watch::Poll::DeadlineReached => return Err(Error::Expired),
                    watch::Poll::Frame { sequence, wire } => {
                        break (sequence, Zeroizing::new(wire));
                    }
                }
            };
            let digest = wire_digest(&input);
            let watch = self.watch.as_mut().unwrap();
            watch.guest_boundary(
                owner.manager().ok_or(Error::Owner)?,
                owner.runtime(),
                instance,
                clock,
                sequence,
                digest,
                (input.len() as u64) * 4,
            )?;
            cancelled(cancel)?;
            let run =
                instance
                    .package()
                    .run_watch_frame(&input, cancel.clone(), self.fuel, self.calls);
            self.execution = Some(run.report.clone());
            if run.report.fuel_remaining > self.fuel || run.report.host_calls > self.calls {
                return Err(Error::Protocol);
            }
            self.fuel = run.report.fuel_remaining;
            self.calls -= run.report.host_calls;
            let output = run.completion.map(Zeroizing::new);
            watch.guest_boundary(
                owner.manager().ok_or(Error::Owner)?,
                owner.runtime(),
                instance,
                clock,
                sequence,
                digest,
                0,
            )?;
            cancelled(cancel)?;
            run.report.outcome.clone().map_err(Error::Guest)?;
            let output = output.ok_or(Error::Protocol)?;
            if output.as_slice() != input.as_slice() {
                return Err(Error::Protocol);
            }
            watch.acknowledge(
                owner.manager().ok_or(Error::Owner)?,
                owner.runtime(),
                instance,
                clock,
                sequence,
                digest,
            )?;
            self.deliveries.push(Delivery {
                sequence,
                digest,
                execution: run.report,
                wire: output,
            });
            self.publish(progress, Phase::Observing);
            if let Some(morrow_fs_watch_v1::state::Phase::Ended(end)) =
                self.watch.as_ref().unwrap().snapshot().consumer_phase
            {
                return Err(Error::ObservationEnded(end));
            }
        }
        Ok(())
    }
}
impl<O: ManagedHostOwner> WatchWorker<O> {
    /// Moves the original clock and complete owner together. A foreign identity
    /// is refused before that clock is sampled or any worker/native object exists.
    pub fn spawn_owned(
        owner: O,
        instance: ManagedInstance,
        approval: WatchApproval,
        mut clock: impl FnMut() -> u64 + Send + 'static,
    ) -> Result<Self, Box<SpawnFailure<O>>> {
        let setup = catch_unwind(AssertUnwindSafe(|| {
            validate(&owner, &instance, &approval)?;
            approval
                .check(
                    owner.manager().ok_or(Error::Owner)?,
                    owner.runtime(),
                    &instance,
                    clock(),
                )
                .map_err(Error::Approval)
        }))
        .unwrap_or(Err(Error::Owner));
        if let Err(error) = setup {
            return Err(Box::new(SpawnFailure {
                owner,
                instance,
                approval,
                error,
            }));
        }
        let host = owner.runtime().binding();
        let cancel = Cancellation::until(approval.deadline());
        let worker_cancel = Cancellation::linked(cancel.clone(), instance.cancellation());
        let limits = instance.package().limits();
        let progress = Arc::new(Mutex::new(Snapshot {
            phase: Phase::Starting,
            native: None,
            delivered: 0,
            fuel_remaining: limits.fuel,
            task_calls_remaining: limits.host_calls,
        }));
        let inner = progress.clone();
        let staged = Arc::new(Mutex::new(Some((owner, instance, approval))));
        let original = staged.clone();
        let join = thread::Builder::new()
            .name("morrow-fs-watch".into())
            .spawn(move || {
                let (mut owner, instance, approval) = original
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                    .expect("original owner staged once");
                let mut work = Work {
                    watch: None,
                    capture_entered: false,
                    capture_returned: false,
                    usage: approval.usage(),
                    deliveries: Vec::new(),
                    execution: None,
                    fuel: limits.fuel,
                    calls: limits.host_calls,
                };
                let mut approval = Some(approval);
                let executed = catch_unwind(AssertUnwindSafe(|| {
                    work.execute(
                        &mut owner,
                        &instance,
                        &mut approval,
                        &mut Clock(clock),
                        &worker_cancel,
                        &inner,
                    )
                }));
                let panicked = executed.is_err();
                let mut result = executed.unwrap_or(Err(Error::Unknown));
                work.publish(&inner, Phase::Finishing);
                // Never query/retry an open whose original completion is unknown.
                let native_retirement = if work.capture_entered && !work.capture_returned {
                    NativeRetirement {
                        complete: false,
                        proof: None,
                    }
                } else if let Some(watch) = &mut work.watch {
                    let proof = catch_unwind(AssertUnwindSafe(|| watch.retire()))
                        .ok()
                        .and_then(Result::ok);
                    work.usage = watch.usage();
                    NativeRetirement {
                        complete: proof.is_some(),
                        proof,
                    }
                } else {
                    NativeRetirement {
                        complete: true,
                        proof: None,
                    }
                };
                let unknown = panicked || !native_retirement.is_complete();
                if unknown && result.is_ok() {
                    result = Err(Error::Unknown);
                }
                let last_native = work.watch.as_ref().map(DirectoryWatch::snapshot);
                let delivered = work.deliveries.len() as u32;
                let (returned, retained_instance, disconnect, maintenance) = if unknown {
                    instance.request_stop();
                    // Native objects are !Send and must not be adopted by the joining
                    // thread. Retain the whole original owner; no cleanup retry API.
                    let _ = Box::leak(Box::new((owner, instance, approval, work.watch.take())));
                    work.deliveries.clear();
                    (None, None, None, None)
                } else {
                    instance.request_stop();
                    let disconnect = catch_unwind(AssertUnwindSafe(|| {
                        if owner.runtime().binding() != host {
                            return Err(JobError::InvalidOptions);
                        }
                        let runtime = owner.runtime_mut();
                        if runtime.binding() != host {
                            return Err(JobError::InvalidOptions);
                        }
                        instance.close(runtime).map_err(|_| JobError::Disconnect)
                    }))
                    .unwrap_or(Err(JobError::Unavailable));
                    let maintenance = catch_unwind(AssertUnwindSafe(|| {
                        if owner.runtime().binding() != host {
                            return Err(JobError::InvalidOptions);
                        }
                        let ended = owner.finish_io();
                        if owner.runtime().binding() != host {
                            return Err(JobError::InvalidOptions);
                        }
                        ended
                    }))
                    .unwrap_or(Err(JobError::Unavailable));
                    (
                        Some(owner),
                        if disconnect.is_err() {
                            Some(instance)
                        } else {
                            None
                        },
                        Some(disconnect),
                        Some(maintenance),
                    )
                };
                // Keep the last native snapshot even when its owner is retained.
                work.publish(&inner, Phase::Finished);
                let mut snapshot = *inner.lock().unwrap_or_else(|error| error.into_inner());
                snapshot.native = last_native;
                snapshot.delivered = delivered;
                *inner.lock().unwrap_or_else(|error| error.into_inner()) = snapshot;
                Exit {
                    owner: returned,
                    instance: retained_instance,
                    result,
                    unknown,
                    native_retirement,
                    disconnect,
                    maintenance,
                    snapshot,
                    usage: work.usage,
                    deliveries: work.deliveries,
                    last_execution: work.execution,
                }
            });
        match join {
            Ok(join) => Ok(Self {
                join: Some(join),
                cancel,
                progress,
            }),
            Err(_) => {
                let (owner, instance, approval) = staged
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                    .expect("failed OS spawn retains original owner");
                Err(Box::new(SpawnFailure {
                    owner,
                    instance,
                    approval,
                    error: Error::Spawn,
                }))
            }
        }
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    pub fn snapshot(&self) -> Snapshot {
        *self
            .progress
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
    /// Some is returned only after this original JoinHandle actually joined.
    /// Result fields remain historical; neither join nor delivery renews authority.
    pub fn try_join(&mut self) -> Result<Option<Exit<O>>, Error> {
        let join = self.join.as_ref().ok_or(Error::Consumed)?;
        if !join.is_finished() {
            return Ok(None);
        }
        self.join
            .take()
            .unwrap()
            .join()
            .map(Some)
            .map_err(|_| Error::Unknown)
    }
}
impl<O: ManagedHostOwner> Drop for WatchWorker<O> {
    fn drop(&mut self) {
        if self.join.is_some() {
            self.cancel.cancel();
        }
    }
}

#[cfg(test)]
mod close_refusal_tests;

#[cfg(test)]
mod picker_stop_tests {
    use super::*;
    use std::time::Instant;

    fn native_cancelled() -> watch::Error {
        watch::Error::Directory(crate::directory_io::Error::Cancelled)
    }

    #[test]
    fn original_deadline_survives_a_later_cancel_signal() {
        let token = Cancellation::until(Instant::now());
        let mut first = None;
        assert!(picker_stopped(&mut first, &token));
        assert_eq!(first, Some(Fault::Deadline));
        token.cancel();
        assert!(picker_stopped(&mut first, &token));
        assert_eq!(capture_failure(native_cancelled(), first), Error::Expired);
    }

    #[test]
    fn original_cancel_survives_a_later_deadline() {
        let token = Cancellation::until(Instant::now() + Duration::from_secs(60));
        let mut first = None;
        assert!(!picker_stopped(&mut first, &token));
        token.cancel();
        assert!(picker_stopped(&mut first, &token));
        token.limit_deadline(Instant::now());
        assert!(picker_stopped(&mut first, &token));
        assert_eq!(capture_failure(native_cancelled(), first), Error::Cancelled);
    }

    #[test]
    fn unrelated_native_failure_and_unknown_are_never_reclassified() {
        for error in [watch::Error::SelectionMismatch, watch::Error::Unknown] {
            assert_eq!(
                capture_failure(error, Some(Fault::Deadline)),
                Error::Native(error)
            );
        }
        assert_eq!(
            capture_failure(native_cancelled(), None),
            Error::Native(native_cancelled())
        );
    }
}
