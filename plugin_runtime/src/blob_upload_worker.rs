//! One upload task on the complete original owner. No IO/content capability is borrowed.
//! Results describe an ended execution, never a renewed upload or business-commit grant.
use crate::{
    Cancellation, Fault, Report,
    blob_upload_binding::{UploadApproval, UploadRevocation, Usage},
    io_jobs::{JobError, ManagedHostOwner},
    manager::ManagedInstance,
};
use morrow_core::{dispatch::HostBinding, store::BlobStageStoreBinding, task::Invocation};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

mod guest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Denied,
    Limit,
    Protocol,
    Cancelled,
    Expired,
    Storage,
    Unknown,
    Owner,
    Spawn,
    Consumed,
}
impl Error {
    fn fault(self) -> Fault {
        match self {
            Self::Limit => Fault::Limits,
            Self::Cancelled => Fault::Cancelled,
            Self::Expired => Fault::Deadline,
            _ => Fault::TaskProtocol,
        }
    }
}
impl From<crate::blob_upload_binding::Error> for Error {
    fn from(error: crate::blob_upload_binding::Error) -> Self {
        use crate::blob_upload_binding::Error as E;
        match error {
            E::Expired => Self::Expired,
            E::Limit => Self::Limit,
            _ => Self::Denied,
        }
    }
}
impl From<morrow_blob_upload_request_v1::Error> for Error {
    fn from(_: morrow_blob_upload_request_v1::Error) -> Self {
        Self::Protocol
    }
}

pub struct GuestResult {
    pub execution: Report,
    /// Last response delivered to the guest, verified against the exact pending call.
    /// No payload, Store address, owner identity or executable grant escapes here.
    pub phase: morrow_blob_upload_request_v1::Phase,
}
pub struct Exit<O: ManagedHostOwner> {
    pub owner: O,
    pub result: Result<GuestResult, Error>,
    pub usage: Usage,
    /// Actual interpreter report, including failed execution. None means no
    /// completed interpreter report was obtained; it is never inferred from fees.
    pub execution_report: Option<Report>,
    /// A write may have committed despite failure. Inspect with the returned original
    /// owner under separate trusted authority; never replay this task/approval.
    pub unknown: bool,
    pub disconnect: Result<(), JobError>,
    pub maintenance: Result<(), JobError>,
    /// An unconfirmed disconnect keeps the exact instance for explicit cleanup.
    pub instance: Option<ManagedInstance>,
    possible_effect: bool,
}
pub struct SpawnFailure<O: ManagedHostOwner> {
    pub owner: O,
    pub instance: ManagedInstance,
    pub approval: UploadApproval,
    pub error: Error,
}
impl<O: ManagedHostOwner> std::fmt::Debug for SpawnFailure<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadSpawnFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Poll {
    Running,
    Ready,
    Consumed,
}

/// Cancellation and final-delivery veto only. It cannot issue an approval,
/// resume a guest, access storage, or prove that a worker has stopped.
#[derive(Clone)]
pub struct UploadControl {
    cancel: Cancellation,
    veto: UploadRevocation,
}
impl UploadControl {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    /// Also applies after the original join, while a product result is unread.
    /// Intentional receiver disconnection does not renew or veto this historical
    /// delivery; original cancellation, expiry and explicit revocation still do.
    pub fn delivery_error(&self) -> Option<Error> {
        match self.cancel.fault() {
            Some(Fault::Deadline) => Some(Error::Expired),
            Some(_) => Some(Error::Cancelled),
            None if self.veto.completed_delivery_vetoed() => Some(Error::Denied),
            None => None,
        }
    }
}

/// Exactly one task and one original owner. No resubmit, rebind, retry or restore API.
/// Polling is nonblocking. Drop only requests cancellation; it proves no thread/Store closure.
pub struct UploadWorker<O: ManagedHostOwner> {
    join: Option<JoinHandle<Exit<O>>>,
    cancel: Cancellation,
    delivery_veto: UploadRevocation,
}
#[derive(Default)]
struct Effects {
    execution_report: Option<Report>,
    committed: bool,
    in_store_call: bool,
    commit_unknown: bool,
}
impl Effects {
    fn unknown_after_failure(&self) -> bool {
        self.committed || self.in_store_call || self.commit_unknown
    }
}
fn validate<O: ManagedHostOwner>(
    owner: &O,
    instance: &ManagedInstance,
    approval: &UploadApproval,
) -> Result<(), Error> {
    approval
        .validate_owner(
            owner.manager().ok_or(Error::Denied)?,
            owner.runtime(),
            instance,
        )
        .map_err(Error::from)
}
fn original_store<O: ManagedHostOwner>(
    owner: &O,
    host: HostBinding,
    store: &BlobStageStoreBinding,
) -> Result<(), JobError> {
    if owner.runtime().binding() != host {
        return Err(JobError::InvalidOptions);
    }
    owner
        .runtime()
        .store_local()
        .validate_blob_stage_store_binding(store)
        .map_err(|_| JobError::InvalidOptions)
}
impl<O: ManagedHostOwner> UploadWorker<O> {
    pub fn spawn_owned(
        owner: O,
        instance: ManagedInstance,
        approval: UploadApproval,
        input: Invocation,
        now_unix_ms: i64,
    ) -> Result<Self, Box<SpawnFailure<O>>> {
        let setup = catch_unwind(AssertUnwindSafe(|| {
            validate(&owner, &instance, &approval)?;
            let transform = input.transform().ok_or(Error::Protocol)?;
            let package = instance.package().package();
            package
                .blob_upload_handler(transform)
                .map_err(|_| Error::Protocol)?;
            let declaration = package.blob_upload_declaration().ok_or(Error::Denied)?;
            if !declaration.handlers.contains(&transform.handler)
                || input.bytes().len() > crate::MAX_TASK_BYTES
            {
                return Err(Error::Protocol);
            }
            Ok((
                owner.runtime().binding(),
                owner.runtime().store_local().blob_stage_store_binding(),
            ))
        }))
        .unwrap_or(Err(Error::Owner));
        let (host, store) = match setup {
            Ok(value) => value,
            Err(error) => {
                return Err(Box::new(SpawnFailure {
                    owner,
                    instance,
                    approval,
                    error,
                }));
            }
        };
        let cancel = Cancellation::until(approval.deadline());
        let delivery_veto = approval.revocation();
        let worker_cancel = Cancellation::linked(cancel.clone(), instance.cancellation());
        // OS spawn failure must not drop the only owner in an unexecuted closure.
        let staged = Arc::new(Mutex::new(Some((owner, instance, approval))));
        let worker_owner = staged.clone();
        let join = spawn_thread(move || {
            let (mut owner, instance, approval) = worker_owner
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
                .expect("one original owner");
            let mut effects = Effects::default();
            let result = catch_unwind(AssertUnwindSafe(|| {
                validate(&owner, &instance, &approval)?;
                owner.prepare_io().map_err(|_| Error::Owner)?;
                validate(&owner, &instance, &approval)?;
                guest::execute(
                    &mut owner,
                    &instance,
                    &approval,
                    &input,
                    now_unix_ms,
                    &worker_cancel,
                    &mut effects,
                )
            }))
            .unwrap_or(Err(Error::Unknown));
            let unknown = result.is_err() && effects.unknown_after_failure();
            let usage = approval.usage();
            // Always stop the exact instance. A substituted host/Store receives no cleanup.
            instance.request_stop();
            let disconnect = catch_unwind(AssertUnwindSafe(|| {
                original_store(&owner, host, &store)?;
                let runtime = owner.runtime_mut();
                if runtime.binding() != host
                    || runtime
                        .store_local()
                        .validate_blob_stage_store_binding(&store)
                        .is_err()
                {
                    return Err(JobError::InvalidOptions);
                }
                instance.close(runtime).map_err(|_| JobError::Disconnect)
            }))
            .unwrap_or(Err(JobError::Unavailable));
            let maintenance = catch_unwind(AssertUnwindSafe(|| {
                original_store(&owner, host, &store)?;
                let result = owner.finish_io();
                original_store(&owner, host, &store)?;
                result
            }))
            .unwrap_or(Err(JobError::Unavailable));
            Exit {
                owner,
                result,
                usage,
                execution_report: effects.execution_report.take(),
                unknown,
                disconnect,
                maintenance,
                instance: if disconnect.is_err() {
                    Some(instance)
                } else {
                    None
                },
                possible_effect: effects.unknown_after_failure(),
            }
        });
        match join {
            Ok(join) => Ok(Self {
                join: Some(join),
                cancel,
                delivery_veto,
            }),
            Err(_) => {
                let (owner, instance, approval) = staged
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("failed spawn retains owner");
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
    pub fn control(&self) -> UploadControl {
        UploadControl {
            cancel: self.cancel.clone(),
            veto: self.delivery_veto.clone(),
        }
    }
    pub fn poll(&self) -> Poll {
        match &self.join {
            None => Poll::Consumed,
            Some(join) if join.is_finished() => Poll::Ready,
            Some(_) => Poll::Running,
        }
    }
    /// Returns only after this original thread actually ended. The historical
    /// result does not assert that any upload authority is still live.
    pub fn try_join(&mut self) -> Result<Option<Exit<O>>, Error> {
        match self.poll() {
            Poll::Running => Ok(None),
            Poll::Consumed => Err(Error::Consumed),
            Poll::Ready => {
                let mut exit = self
                    .join
                    .take()
                    .expect("ready original join")
                    .join()
                    .map_err(|_| Error::Unknown)?;
                let veto = self.control().delivery_error();
                if let Some(error) = veto
                    && exit.result.is_ok()
                {
                    exit.result = Err(error);
                    exit.unknown = exit.possible_effect;
                }
                Ok(Some(exit))
            }
        }
    }
}
impl<O: ManagedHostOwner> Drop for UploadWorker<O> {
    fn drop(&mut self) {
        if self.join.is_some() {
            self.cancel.cancel();
        }
    }
}
fn spawn_thread<T: Send + 'static>(
    run: impl FnOnce() -> T + Send + 'static,
) -> std::io::Result<JoinHandle<T>> {
    #[cfg(test)]
    if FAIL_SPAWN.replace(false) {
        return Err(std::io::Error::other("injected upload spawn failure"));
    }
    thread::Builder::new()
        .name("morrow-blob-upload".into())
        .spawn(run)
}
#[cfg(test)]
thread_local! { static FAIL_SPAWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
