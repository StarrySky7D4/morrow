//! Exclusive original owner; no replacement Store, Core or protected Session.
use super::commands::{AgentCommand, AgentCommandHandle, AgentError, AgentReply, Command};
use crate::product_gate::{ProductGate, StopRegistration};
use morrow_agent_process_control_v1::{ReadQuery, host::Host as ProcessHost};
use morrow_agent_session_exec_v1_r2::authority::{Admission, SessionExecHost};
use morrow_agent_session_process_v1_host::catalog::CatalogManagedPackage;
use morrow_codex_session_exec_windows_v1::{
    BorrowedNativeResources, BorrowedStopHandle, BorrowedWindowsExecutionPort,
};
use morrow_core::{
    dispatch::{Connection, HostBinding},
    lifecycle::{InstancePhase, Revocation},
};
use morrow_plugin_runtime::{
    io_jobs::{HostOwner, JobError, ManagedHostOwner},
    manager::Manager,
};
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};

pub const MAX_AGENT_FRAME_BYTES: usize = 128 * 1024;
#[derive(Clone, Copy)]
pub struct AgentLimits {
    pub max_commands: u32,
    pub max_total_bytes: u64,
    pub max_imports: u32,
}
impl Default for AgentLimits {
    fn default() -> Self {
        Self {
            max_commands: 16,
            max_total_bytes: 4 * 1024 * 1024,
            max_imports: 1024,
        }
    }
}
impl AgentLimits {
    pub(super) fn validate(self) -> Result<Self, AgentError> {
        if !(1..=16).contains(&self.max_commands)
            || !(1..=16 * 1024).contains(&self.max_imports)
            || self.max_total_bytes < MAX_AGENT_FRAME_BYTES as u64
            || self.max_total_bytes > 4 * 1024 * 1024
        {
            return Err(AgentError::Limit);
        }
        Ok(self)
    }
}
/// Live objects supplied by the original trusted owner. No persisted record or
/// guest bytes can reconstruct this context or grant executor authority.
pub struct AgentContext {
    pub host: Arc<SessionExecHost>,
    pub processes: ProcessHost,
    pub resources: Arc<BorrowedNativeResources>,
    port: Option<BorrowedWindowsExecutionPort>,
    pub executor_connection: Arc<Connection>,
    pub executor_admission: Admission,
    scheduler: Option<Arc<SchedulerLease>>,
    pub(super) sealed_proposal: Option<super::sealed_proposal::SealedProposalLease>,
    pub(super) lifecycle: Option<super::context_lifecycle::Ticket>,
    pub(super) native_session: Option<morrow_agent_session_process_v1_host::native_session::NativeSessionLease>,
    // Independently approved session-only bridge, retained with the original context.
    native_session_package: Option<CatalogManagedPackage>,
    // Actual OS-worker entry, including port=None contexts with no scheduler.
    worker_started: bool,
}
pub struct AgentContextFailure {
    pub context: AgentContext,
    pub error: AgentError,
}
impl AgentContext {
    pub fn new(
        host: Arc<SessionExecHost>,
        processes: ProcessHost,
        resources: Arc<BorrowedNativeResources>,
        port: Option<BorrowedWindowsExecutionPort>,
        executor_connection: Arc<Connection>,
        executor_admission: Admission,
    ) -> Result<Self, Box<AgentContextFailure>> {
        let mut context = Self {
            host,
            processes,
            resources,
            port,
            executor_connection,
            executor_admission,
            scheduler: None,
            sealed_proposal: None,
            lifecycle: None,
            native_session: None,
            native_session_package: None,
            worker_started: false,
        };
        if let Some(port) = &context.port {
            match SchedulerLease::new(port.scheduler(), context.resources.clone()) {
                Ok(scheduler) => context.scheduler = Some(scheduler),
                Err(error) => return Err(Box::new(AgentContextFailure { context, error })),
            }
        }
        Ok(context)
    }
    /// Attach one real independently approved session-only package before ownership transfer.
    /// Rejection leaves the caller's Option unchanged, including its cleanup obligation.
    pub fn attach_native_session_package(
        &mut self, manager: &Manager, runtime: &morrow_core::dispatch::HostRuntime,
        package: &mut Option<CatalogManagedPackage>, now: u64,
    ) -> Result<(), AgentError> {
        if self.lifecycle.is_some() || self.worker_started || self.native_session.is_some()
            || self.native_session_package.is_some()
            || self.scheduler.as_ref().is_some_and(|s| s.started.load(Ordering::Acquire))
            || self.resources.owner_binding() != runtime.binding() {
            return Err(AgentError::Invalid);
        }
        let candidate = package.as_ref().ok_or(AgentError::Invalid)?;
        candidate.validate_native_session_package(manager, runtime, &self.host, now)
            .map_err(AgentError::Session)?;
        if candidate.shared_connection().binding() == self.executor_connection.binding() {
            return Err(AgentError::Invalid);
        }
        self.native_session_package = package.take();
        Ok(())
    }
    /// Cancel an untracked, never-started secondary attachment. This returns
    /// the same approved package without revoking or renewing any authority.
    /// Caller must close_session on its original owner before discarding it.
    /// Tracked or dirty contexts must use original lifecycle cleanup instead.
    pub fn detach_native_session_package(
        &mut self,
    ) -> Result<Option<CatalogManagedPackage>, AgentError> {
        if self.lifecycle.is_some() || self.worker_started || self.native_session.is_some()
            || self.scheduler.as_ref().is_some_and(|s| s.started.load(Ordering::Acquire))
            || !self.resources.is_clean().map_err(|_| AgentError::Unknown)? {
            return Err(AgentError::Invalid);
        }
        Ok(self.native_session_package.take())
    }
    pub fn port(&self) -> Option<&BorrowedWindowsExecutionPort> {
        self.port.as_ref()
    }
    pub(super) fn validate_sealed_proposer<O: ManagedHostOwner>(
        &self,
        owner: &O,
        operation_id: &str,
        proposal_sha256: [u8; 32],
        checkpoint: &dyn Fn() -> u64,
    ) -> Result<(), AgentError> {
        if let Some(lease) = &self.sealed_proposal {
            lease
                .validate_live(
                    owner,
                    &self.host,
                    operation_id,
                    proposal_sha256,
                    checkpoint(),
                )
                .map_err(|_| AgentError::Unknown)?;
        }
        Ok(())
    }
    pub(super) fn close_sealed(
        &mut self,
        runtime: &mut morrow_core::dispatch::HostRuntime,
    ) -> Result<(), AgentError> {
        if let Some(lease) = &self.native_session {
            lease.close().map_err(AgentError::Session)?;
            self.native_session = None;
        }
        if let Some(package) = &self.native_session_package {
            package.request_stop();
            package.close_session(runtime, &self.host).map_err(AgentError::Session)?;
            self.native_session_package = None;
        }
        if let Some(lease) = &self.sealed_proposal {
            lease.request_stop();
            lease
                .close(runtime, &self.host)
                .map_err(|_| AgentError::Disconnect)?;
            self.sealed_proposal = None;
        }
        Ok(())
    }
    /// Discard a context with no AgentWorker and clean original native facts on
    /// a synchronous caller. Scheduler aliases remain charged in the bounded
    /// debt pool; otherwise returns the same inputs for recovery.
    pub fn try_dispose(self) -> Result<(), Box<Self>> {
        // A tracked context needs original-runtime cleanup, not just native cleanliness.
        if self.lifecycle.is_some() {
            return Err(Box::new(self));
        }
        if tokio::runtime::Handle::try_current().is_ok()
            || self.worker_started
            || self.sealed_proposal.is_some()
            || self.native_session.is_some()
            || self.native_session_package.is_some()
            || !self.resources.is_clean().unwrap_or(false)
            || self
                .scheduler
                .as_ref()
                .is_some_and(|s| s.started.load(Ordering::Acquire))
        {
            return Err(Box::new(self));
        }
        drop(self);
        Ok(())
    }
    pub(super) fn original_authority(&mut self) {
        if let Some(ticket) = &self.lifecycle {
            let original = ticket.original();
            self.host = original.host;
            self.executor_connection = original.connection;
            self.executor_admission = original.admission;
            self.resources = original.resources;
        }
    }
    pub(super) fn attach_lifecycle(&mut self) -> crate::Result<()> {
        if let Some(ticket) = &self.lifecycle {
            ticket.attach(self)?;
        }
        self.original_authority();
        Ok(())
    }
    pub(super) fn native_session_obligations(&self) -> bool {
        self.native_session.is_some() || self.native_session_package.is_some()
    }
    pub(super) fn settle_lifecycle(
        &mut self,
        runtime: &morrow_core::dispatch::HostRuntime,
    ) -> crate::Result<()> {
        if self.port.is_some()
            || self
                .scheduler
                .as_ref()
                .is_some_and(|s| !s.confirmed.load(Ordering::Acquire))
        {
            return Err("context scheduler has not shut down".into());
        }
        if let Some(ticket) = &self.lifecycle {
            ticket.settle(runtime, self)?;
        }
        self.lifecycle = None;
        Ok(())
    }
    pub(super) fn dispose_on_original(
        mut self,
        runtime: &morrow_core::dispatch::HostRuntime,
    ) -> Result<(), Box<Self>> {
        if tokio::runtime::Handle::try_current().is_ok()
            || self.sealed_proposal.is_some()
            || self.native_session.is_some()
            || self.native_session_package.is_some()
            || !self.resources.is_clean().unwrap_or(false)
            || self
                .scheduler
                .as_ref()
                .is_some_and(|s| s.started.load(Ordering::Acquire))
        {
            return Err(Box::new(self));
        }
        self.release_backend();
        if self
            .scheduler
            .as_ref()
            .is_some_and(|s| s.try_shutdown() != AgentSchedulerPhase::Shutdown)
            || self.settle_lifecycle(runtime).is_err()
        {
            return Err(Box::new(self));
        }
        Ok(())
    }
    fn release_backend(&mut self) {
        self.port = None;
    }
}
impl Drop for AgentContext {
    fn drop(&mut self) {
        if let Some(ticket) = self.lifecycle.take() {
            let original = ticket.original();
            let parked = Self {
                host: original.host,
                executor_connection: original.connection,
                executor_admission: original.admission,
                resources: original.resources,
                processes: std::mem::take(&mut self.processes),
                port: self.port.take(),
                scheduler: self.scheduler.take(),
                sealed_proposal: self.sealed_proposal.take(),
                native_session: self.native_session.take(),
                native_session_package: self.native_session_package.take(),
                worker_started: self.worker_started,
                lifecycle: None,
            };
            ticket.park(parked);
            return;
        }
        if let Some(scheduler) = &self.scheduler
            && Arc::strong_count(scheduler) == 1
            && !scheduler.started.load(Ordering::Acquire)
            && tokio::runtime::Handle::try_current().is_err()
            && self.resources.is_clean().unwrap_or(false)
        {
            // No AgentWorker was launched, but the trusted port may have been
            // used already. Release it before checking exclusive scheduler ownership.
            self.port = None;
            scheduler.release_unused();
        }
    }
}
/// This token owns exactly one independent Arc to the port's actual scheduler.
/// Cloning the token never clones Core, Store, backend or Runtime ownership.
struct SchedulerLease {
    id: u64,
    runtime: Mutex<Option<Arc<tokio::runtime::Runtime>>>,
    confirmed: AtomicBool,
    started: AtomicBool,
    released_unused: AtomicBool,
    resources: Arc<BorrowedNativeResources>,
}
pub const MAX_AGENT_SCHEDULERS: usize = 16;
static NEXT_SCHEDULER: AtomicU64 = AtomicU64::new(1);
static SCHEDULERS: Mutex<BTreeMap<u64, SchedulerRecord>> = Mutex::new(BTreeMap::new());
struct SchedulerRecord {
    runtime_pointer: usize,
    lease: Weak<SchedulerLease>,
    resources: Arc<BorrowedNativeResources>,
    debt: Option<Arc<tokio::runtime::Runtime>>,
    uncertain: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentSchedulerStatus {
    pub active_tokens: usize,
    pub retained_debts: usize,
    pub uncertain_debts: usize,
    pub capacity: usize,
}
pub fn agent_scheduler_status() -> AgentSchedulerStatus {
    let slots = SCHEDULERS.lock().unwrap_or_else(|e| e.into_inner());
    let active_tokens = slots
        .values()
        .filter(|r| r.lease.strong_count() != 0)
        .count();
    AgentSchedulerStatus {
        active_tokens,
        retained_debts: slots.len() - active_tokens,
        uncertain_debts: slots
            .values()
            .filter(|r| r.uncertain && r.lease.strong_count() == 0)
            .count(),
        capacity: MAX_AGENT_SCHEDULERS,
    }
}
/// Explicit scheduler-only recovery. It cannot acknowledge facts, restore a
/// missing owner, restart a command or repair ProtectedSession storage.
pub fn reap_agent_scheduler_debts() -> Result<usize, AgentError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(AgentError::Busy);
    }
    let ids: Vec<_> = SCHEDULERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(_, r)| r.lease.strong_count() == 0 && r.debt.is_some())
        .map(|(id, _)| *id)
        .collect();
    let mut reaped = 0;
    for id in ids {
        let item = {
            let mut slots = SCHEDULERS.lock().unwrap_or_else(|e| e.into_inner());
            slots
                .get_mut(&id)
                .and_then(|r| r.debt.take().map(|runtime| (runtime, r.resources.clone())))
        };
        let Some((runtime, resources)) = item else {
            continue;
        };
        let runtime = if resources.is_clean().unwrap_or(false) {
            match Arc::try_unwrap(runtime) {
                Ok(runtime) => {
                    if catch_unwind(AssertUnwindSafe(|| drop(runtime))).is_ok() {
                        SCHEDULERS
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remove(&id);
                        reaped += 1;
                    } else if let Some(r) = SCHEDULERS
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .get_mut(&id)
                    {
                        r.uncertain = true;
                    }
                    continue;
                }
                Err(runtime) => runtime,
            }
        } else {
            runtime
        };
        if let Some(r) = SCHEDULERS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
        {
            r.debt = Some(runtime);
        }
    }
    Ok(reaped)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentSchedulerPhase {
    NotRequired,
    Retained,
    WaitingForReferences,
    TokioContext,
    Shutdown,
    Unknown,
}
impl SchedulerLease {
    fn new(
        runtime: Arc<tokio::runtime::Runtime>,
        resources: Arc<BorrowedNativeResources>,
    ) -> Result<Arc<Self>, AgentError> {
        let pointer = Arc::as_ptr(&runtime) as usize;
        let mut slots = SCHEDULERS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(record) = slots.values().find(|r| r.runtime_pointer == pointer) {
            if !Arc::ptr_eq(&record.resources, &resources) {
                return Err(AgentError::Invalid);
            }
            return record.lease.upgrade().ok_or(AgentError::Busy);
        }
        if slots.len() >= MAX_AGENT_SCHEDULERS {
            return Err(AgentError::Limit);
        }
        let id = NEXT_SCHEDULER
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| AgentError::Limit)?;
        let lease = Arc::new(Self {
            id,
            runtime: Mutex::new(Some(runtime)),
            confirmed: AtomicBool::new(false),
            started: AtomicBool::new(false),
            released_unused: AtomicBool::new(false),
            resources: resources.clone(),
        });
        slots.insert(
            id,
            SchedulerRecord {
                runtime_pointer: pointer,
                lease: Arc::downgrade(&lease),
                resources,
                debt: None,
                uncertain: false,
            },
        );
        Ok(lease)
    }
    fn release_unused(&self) {
        if self.started.load(Ordering::Acquire)
            || tokio::runtime::Handle::try_current().is_ok()
            || !self.resources.is_clean().unwrap_or(false)
        {
            return;
        }
        // Native clean counts do not prove that async captures have dropped.
        // Keep the original anchor when another Runtime Arc remains.
        if self.try_shutdown() == AgentSchedulerPhase::Shutdown {
            self.released_unused.store(true, Ordering::Release);
        }
    }
    /// The caller must first have actually joined AgentWorker and released its
    /// retired backend. A native resource count is not an async-task join proof.
    fn try_shutdown(&self) -> AgentSchedulerPhase {
        if tokio::runtime::Handle::try_current().is_ok() {
            return AgentSchedulerPhase::TokioContext;
        }
        let mut slot = self.runtime.lock().unwrap_or_else(|e| e.into_inner());
        let Some(runtime) = slot.take() else {
            return if self.confirmed.load(Ordering::Acquire) {
                AgentSchedulerPhase::Shutdown
            } else {
                AgentSchedulerPhase::Unknown
            };
        };
        match Arc::try_unwrap(runtime) {
            Err(runtime) => {
                *slot = Some(runtime);
                AgentSchedulerPhase::WaitingForReferences
            }
            Ok(runtime) => {
                // Runtime::Drop synchronously shuts down its scheduler threads.
                // No deadline or shutdown_background can stand in for this.
                if catch_unwind(AssertUnwindSafe(|| drop(runtime))).is_err() {
                    return AgentSchedulerPhase::Unknown;
                }
                self.confirmed.store(true, Ordering::Release);
                SCHEDULERS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&self.id);
                AgentSchedulerPhase::Shutdown
            }
        }
    }
}
impl Drop for SchedulerLease {
    fn drop(&mut self) {
        self.release_unused();
        if let Some(runtime) = self
            .runtime
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            // This token already owns a reserved bounded slot. Quarantine keeps
            // it alive; explicit synchronous reap can retire it after facts/jobs
            // are actually clean and all external Runtime Arcs have gone.
            if let Some(record) = SCHEDULERS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_mut(&self.id)
            {
                record.debt = Some(runtime);
                record.uncertain = true;
            }
        } else {
            let mut slots = SCHEDULERS.lock().unwrap_or_else(|e| e.into_inner());
            if self.confirmed.load(Ordering::Acquire)
                || self.released_unused.load(Ordering::Acquire)
            {
                slots.remove(&self.id);
            } else if let Some(record) = slots.get_mut(&self.id) {
                record.uncertain = true;
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPhase {
    Running,
    Stopping,
    Cleanup,
    Joined,
}
#[derive(Clone, Copy, Debug)]
pub struct AgentProgress {
    pub phase: AgentPhase,
    pub accepted_commands: u32,
    pub reserved_bytes: u64,
    pub imported_frames: u32,
    pub cleanup_error: Option<AgentError>,
    pub scheduler: AgentSchedulerPhase,
}
pub(super) struct Control {
    stopped: AtomicBool,
    gate: ProductGate,
    package_stop: Arc<dyn Fn() + Send + Sync>,
    executor_revoke: Revocation,
    package_revoke: Revocation,
    sealed_revoke: Option<Revocation>,
    sealed_stop: Option<Arc<dyn Fn() + Send + Sync>>,
    session_revoke: Option<Revocation>,
    session_stop: Option<Arc<dyn Fn() + Send + Sync>>,
    native: Arc<BorrowedNativeResources>,
    native_stop: Option<BorrowedStopHandle>,
    limits: AgentLimits,
    progress: Mutex<AgentProgress>,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    expires: u64,
    reconcile_requested: AtomicBool,
    reconcile_failed: AtomicBool,
}
impl Control {
    pub(super) fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        (self.package_stop)();
        if let Some(stop) = &self.sealed_stop {
            stop();
        }
        if let Some(stop) = &self.session_stop { stop(); }
        self.executor_revoke.revoke();
        if let Some(stop) = &self.native_stop {
            stop.request_stop();
        }
        self.native.request_stop();
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .phase = AgentPhase::Stopping;
    }
    pub(super) fn live(&self) -> bool {
        let now = (self.clock)();
        self.live_at(now)
    }
    fn live_at(&self, now: u64) -> bool {
        !self.stopped.load(Ordering::Acquire)
            && !self.executor_revoke.is_revoked()
            && !self.package_revoke.is_revoked()
            && !self
                .sealed_revoke
                .as_ref()
                .is_some_and(Revocation::is_revoked)
            && !self.session_revoke.as_ref().is_some_and(Revocation::is_revoked)
            && !self.gate.closed()
            && now < self.expires
    }
    fn checkpoint(&self) -> Result<(), AgentError> {
        if self.live() {
            Ok(())
        } else {
            self.stop();
            Err(AgentError::Unknown)
        }
    }
    fn permit_import(&self) -> Result<(), AgentError> {
        self.checkpoint()?;
        let mut p = self.progress.lock().unwrap_or_else(|e| e.into_inner());
        if p.imported_frames >= self.limits.max_imports {
            return Err(AgentError::Limit);
        }
        p.imported_frames += 1;
        Ok(())
    }
}
// This wrapper only moves the complete original value. Its hooks borrow that
// same owner and are a budget/maintenance boundary, never IO carrier approval.
struct BudgetOwner<O> {
    original: O,
    control: Arc<Control>,
}
impl<O: ManagedHostOwner> HostOwner for BudgetOwner<O> {
    fn runtime(&self) -> &morrow_core::dispatch::HostRuntime {
        self.original.runtime()
    }
    fn runtime_mut(&mut self) -> &mut morrow_core::dispatch::HostRuntime {
        self.original.runtime_mut()
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        if self.control.permit_import().is_err() {
            self.control.stop();
            return Err(JobError::Limit);
        }
        if let Err(error) = self.original.prepare_io() {
            self.control.stop();
            return Err(error);
        }
        self.control.checkpoint().map_err(|_| JobError::Unavailable)
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.original.finish_io()
    }
}
impl<O: ManagedHostOwner> ManagedHostOwner for BudgetOwner<O> {
    fn manager(&self) -> Option<&Manager> {
        self.original.manager()
    }
    fn with_managed_runtime<T>(
        &mut self,
        action: impl FnOnce(&Manager, &mut morrow_core::dispatch::HostRuntime) -> T,
    ) -> Option<T> {
        self.original.with_managed_runtime(action)
    }
}
/// Retained exact bridge/context when original disconnection or maintenance
/// could not be confirmed. Repair first confirms actual native completion.
pub struct AgentCleanup {
    pub package: CatalogManagedPackage,
    pub context: AgentContext,
    package_closed: bool,
    executor_closed: bool,
    executor_grant_revoked: bool,
}
impl AgentCleanup {
    fn new(package: CatalogManagedPackage, context: AgentContext) -> Self {
        Self {
            package,
            context,
            package_closed: false,
            executor_closed: false,
            executor_grant_revoked: false,
        }
    }
    fn disconnect<O: ManagedHostOwner>(&mut self, owner: &mut O) -> Result<(), AgentError> {
        self.context.original_authority();
        if self.context.resources.owner_binding() != owner.runtime().binding() {
            return Err(AgentError::Disconnect);
        }
        self.package.request_stop();
        let mut failed = false;
        if self.context.close_sealed(owner.runtime_mut()).is_err() {
            failed = true;
        }
        if !self.package_closed {
            if self
                .package
                .close(
                    owner.runtime_mut(),
                    &self.context.host,
                    &mut self.context.processes,
                )
                .is_ok()
            {
                self.package_closed = true;
            } else {
                failed = true;
            }
        }
        if !self.executor_grant_revoked {
            if self
                .context
                .host
                .revoke(&self.context.executor_admission)
                .is_ok()
            {
                self.executor_grant_revoked = true;
            } else {
                failed = true;
            }
        }
        if !self.executor_closed {
            if owner
                .runtime_mut()
                .disconnect(&self.context.executor_connection)
                .is_ok()
            {
                self.executor_closed = true;
            } else {
                failed = true;
            }
        }
        if failed {
            Err(AgentError::Disconnect)
        } else {
            if let Some(ticket) = &self.context.lifecycle {
                ticket
                    .authority_retired(owner.runtime())
                    .map_err(|_| AgentError::Disconnect)?;
            }
            Ok(())
        }
    }
    pub fn repair<O: ManagedHostOwner>(&mut self, owner: &mut O) -> Result<(), AgentError> {
        self.context.original_authority();
        if self.context.resources.owner_binding() != owner.runtime().binding()
            || !self
                .context
                .resources
                .is_clean()
                .map_err(|_| AgentError::Unknown)?
        {
            return Err(AgentError::Unknown);
        }
        self.disconnect(owner)?;
        owner.finish_io().map_err(|_| AgentError::Maintenance)?;
        self.context.release_backend();
        if let Some(scheduler) = &self.context.scheduler
            && scheduler.try_shutdown() != AgentSchedulerPhase::Shutdown
        {
            return Err(AgentError::Busy);
        }
        self.context
            .settle_lifecycle(owner.runtime())
            .map_err(|_| AgentError::Disconnect)?;
        Ok(())
    }
}
pub struct AgentExit<O: ManagedHostOwner> {
    pub owner: O,
    pub execution: Result<(), AgentError>,
    pub disconnect: Result<(), AgentError>,
    pub maintenance: Result<(), AgentError>,
    pub cleanup: Option<AgentCleanup>,
    retired_context: Option<AgentContext>,
    // Returned with the owner and dropped by StateSlot only after actual join.
    _stop_registration: Option<StopRegistration>,
}
pub struct AgentSpawnFailure<O: ManagedHostOwner> {
    pub owner: O,
    pub cleanup: AgentCleanup,
    pub error: AgentError,
}
pub struct AgentWorker<O: ManagedHostOwner> {
    sender: mpsc::SyncSender<Command>,
    control: Arc<Control>,
    join: Option<JoinHandle<AgentExit<O>>>,
    pending_exit: Option<AgentExit<O>>,
    scheduler: Option<Arc<SchedulerLease>>,
}
/// Cloneable queue lease; the single original worker remains the exclusive owner.
#[derive(Clone)]
pub(super) struct NativeCommandQueue {
    sender: mpsc::SyncSender<Command>,
    control: Arc<Control>,
}
impl NativeCommandQueue {
    pub(super) fn submit(&self, value: AgentCommand) -> Result<AgentCommandHandle, AgentError> {
        submit_command(&self.sender, &self.control, value)
    }
    pub(super) fn live(&self) -> bool { self.control.live() }
    pub(super) fn now_millis(&self) -> u64 { (self.control.clock)() }
    pub(super) fn live_at(&self, now: u64) -> bool { self.control.live_at(now) }
}
fn submit_command(sender: &mpsc::SyncSender<Command>, control: &Arc<Control>,
    value: AgentCommand) -> Result<AgentCommandHandle, AgentError> {
    control.checkpoint()?;
    let input = value.input_bytes();
    if input > MAX_AGENT_FRAME_BYTES { return Err(AgentError::Limit); }
    let reserved = (input + MAX_AGENT_FRAME_BYTES) as u64;
    let mut p = control.progress.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = p.reserved_bytes.checked_add(reserved).ok_or(AgentError::Limit)?;
    if p.accepted_commands >= control.limits.max_commands
        || bytes > control.limits.max_total_bytes { return Err(AgentError::Limit); }
    let (command, handle) = AgentCommandHandle::pair(value, control.clone());
    sender.try_send(command).map_err(|e| match e {
        mpsc::TrySendError::Full(_) => AgentError::Busy,
        mpsc::TrySendError::Disconnected(_) => AgentError::Unavailable,
    })?;
    p.accepted_commands += 1;
    p.reserved_bytes = bytes;
    Ok(handle)
}
impl<O: ManagedHostOwner> AgentWorker<O> {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        owner: O,
        package: CatalogManagedPackage,
        context: AgentContext,
        gate: ProductGate,
        limits: AgentLimits,
        expires: u64,
        clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Result<Self, Box<AgentSpawnFailure<O>>> {
        let scheduler = context.scheduler.clone();
        let failed = |owner, package, context, error| {
            Box::new(AgentSpawnFailure {
                owner,
                cleanup: AgentCleanup::new(package, context),
                error,
            })
        };
        let setup = catch_unwind(AssertUnwindSafe(|| {
            let limits = limits.validate()?;
            let now = clock();
            if context.port.is_none() && !context.resources.is_clean().unwrap_or(false) {
                return Err(AgentError::Unknown);
            }
            if expires <= now
                || expires - now > 60_000
                || gate.closed()
                || context.resources.owner_binding() != owner.runtime().binding()
                || context.port.is_some() && context.scheduler.is_none()
                || context
                    .port
                    .as_ref()
                    .is_some_and(|port| port.owner_binding() != owner.runtime().binding())
                || package.shared_connection().binding() == context.executor_connection.binding()
                || owner
                    .runtime()
                    .connection_phase(&package.shared_connection())
                    != Ok(InstancePhase::Ready)
                || owner
                    .runtime()
                    .connection_phase(&context.executor_connection)
                    != Ok(InstancePhase::Ready)
                || context.host.generation(owner.runtime()).ok() != Some(package.generation())
            {
                return Err(AgentError::Invalid);
            }
            if let Some(session) = &context.native_session_package {
                session.validate_native_session_package(
                    owner.manager().ok_or(AgentError::Unavailable)?, owner.runtime(),
                    &context.host, now).map_err(AgentError::Session)?;
                if session.shared_connection().binding() == package.shared_connection().binding() {
                    return Err(AgentError::Invalid);
                }
            }
            let executor_revoke = owner
                .runtime()
                .revocation(&context.executor_connection)
                .map_err(|_| AgentError::Invalid)?;
            let package_revoke = owner
                .runtime()
                .revocation(&package.shared_connection())
                .map_err(|_| AgentError::Invalid)?;
            let sealed_revoke = context
                .sealed_proposal
                .as_ref()
                .map(|lease| lease.revocation(owner.runtime()))
                .transpose()
                .map_err(|_| AgentError::Invalid)?;
            let control = Arc::new(Control {
                stopped: AtomicBool::new(false),
                gate: gate.clone(),
                package_stop: package.stop_handle(),
                executor_revoke,
                package_revoke,
                sealed_revoke,
                sealed_stop: context
                    .sealed_proposal
                    .as_ref()
                    .map(|lease| lease.stop_handle()),
                session_revoke: context.native_session_package.as_ref()
                    .map(|p| p.revocation(owner.runtime())).transpose().map_err(AgentError::Session)?,
                session_stop: context.native_session_package.as_ref().map(|p| p.stop_handle()),
                native: context.resources.clone(),
                native_stop: context.port.as_ref().map(|port| port.stop_handle()),
                limits,
                clock: clock.clone(),
                expires,
                reconcile_requested: AtomicBool::new(false),
                reconcile_failed: AtomicBool::new(false),
                progress: Mutex::new(AgentProgress {
                    phase: AgentPhase::Running,
                    accepted_commands: 0,
                    reserved_bytes: 0,
                    imported_frames: 0,
                    cleanup_error: None,
                    scheduler: if scheduler.is_some() {
                        AgentSchedulerPhase::Retained
                    } else {
                        AgentSchedulerPhase::NotRequired
                    },
                }),
            });
            let stopping = control.clone();
            let registration = gate
                .register_scoped(Arc::new(move || stopping.stop()))
                .map_err(|_| AgentError::Unavailable)?;
            Ok((control, registration))
        }))
        .unwrap_or(Err(AgentError::Unavailable));
        let (control, registration) = match setup {
            Ok(value) => value,
            Err(error) => {
                package.request_stop();
                if let Some(session) = &context.native_session_package { session.request_stop(); }
                return Err(failed(owner, package, context, error));
            }
        };
        let (sender, receiver) = mpsc::sync_channel(limits.max_commands as usize);
        let staged = Arc::new(Mutex::new(Some((owner, package, context, registration))));
        let staged_worker = staged.clone();
        let worker_control = control.clone();
        let join = std::thread::Builder::new()
            .name("morrow-agent-owner".into())
            .spawn(move || {
                let (owner, mut package, mut context, registration) = staged_worker
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("only original staged agent owner");
                let binding = owner.runtime().binding();
                context.worker_started = true;
                if let Some(ticket) = &context.lifecycle {
                    ticket.worker_started();
                }
                if let Some(scheduler) = &context.scheduler {
                    scheduler.started.store(true, Ordering::Release);
                }
                let mut owner = BudgetOwner {
                    original: owner,
                    control: worker_control.clone(),
                };
                let execution = catch_unwind(AssertUnwindSafe(|| {
                    execute(
                        &mut owner,
                        &mut package,
                        &mut context,
                        &receiver,
                        &worker_control,
                        binding,
                        expires,
                        &clock,
                    )
                }))
                .unwrap_or(Err(AgentError::Unknown));
                // Stop never asserts native exit. The complete owner and scheduler
                // stay inside this thread while actual cleanup/facts remain charged.
                let _ = catch_unwind(AssertUnwindSafe(|| worker_control.stop()));
                while let Ok(command) = receiver.try_recv() {
                    let _ = command.reply.send(Err(AgentError::Cancelled));
                }
                let mut cursors = BTreeMap::new();
                let mut reconcile = !worker_control.reconcile_failed.load(Ordering::Acquire);
                loop {
                    worker_control
                        .progress
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .phase = AgentPhase::Cleanup;
                    let step = catch_unwind(AssertUnwindSafe(|| {
                        if worker_control
                            .reconcile_requested
                            .swap(false, Ordering::AcqRel)
                        {
                            reconcile = true;
                        }
                        cleanup_step(
                            &mut owner.original,
                            &package,
                            &mut context,
                            &mut cursors,
                            binding,
                            reconcile,
                        )
                    }))
                    .unwrap_or(Err(AgentError::Unknown));
                    match step {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(error) => {
                            reconcile = false;
                            worker_control
                                .progress
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .cleanup_error = Some(error);
                        }
                    }
                    // No cleanup deadline fabricates exit/EOF or returns the owner.
                    std::thread::sleep(Duration::from_millis(5));
                }
                let mut cleanup = AgentCleanup::new(package, context);
                let disconnect =
                    catch_unwind(AssertUnwindSafe(|| cleanup.disconnect(&mut owner.original)))
                        .unwrap_or(Err(AgentError::Disconnect));
                let maintenance = catch_unwind(AssertUnwindSafe(|| owner.original.finish_io()))
                    .unwrap_or(Err(JobError::Unavailable))
                    .map_err(|_| AgentError::Maintenance);
                let (cleanup, retired_context) = if disconnect.is_err() || maintenance.is_err() {
                    (Some(cleanup), None)
                } else {
                    (None, Some(cleanup.context))
                };
                AgentExit {
                    owner: owner.original,
                    execution,
                    disconnect,
                    maintenance,
                    cleanup,
                    retired_context,
                    _stop_registration: Some(registration),
                }
            });
        let join = match join {
            Ok(join) => join,
            Err(_) => {
                let (owner, package, context, registration) = staged
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("failed spawn retains original agent owner");
                let _ = catch_unwind(AssertUnwindSafe(|| control.stop()));
                drop(registration);
                return Err(failed(owner, package, context, AgentError::Unavailable));
            }
        };
        Ok(Self {
            sender,
            control,
            join: Some(join),
            pending_exit: None,
            scheduler,
        })
    }
    pub fn submit(&self, value: AgentCommand) -> Result<AgentCommandHandle, AgentError> {
        submit_command(&self.sender, &self.control, value)
    }
    /// Concrete factory for a bounded native Codex-style session port.
    pub fn native_session_port(&self) -> Result<super::native_session::NativeSessionPort, AgentError> {
        super::native_session::NativeSessionPort::open(NativeCommandQueue {
            sender: self.sender.clone(), control: self.control.clone(),
        })
    }
    pub fn stop(&self) {
        self.control.stop();
    }
    /// Explicitly retry only original cleanup/maintenance/facts, never a command.
    pub fn retry_cleanup(&self) {
        self.control
            .reconcile_requested
            .store(true, Ordering::Release);
    }
    pub fn progress(&self) -> AgentProgress {
        *self
            .control
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }
    pub fn try_reclaim(&mut self) -> Result<Option<AgentExit<O>>, AgentError> {
        if self.pending_exit.is_none() {
            let join = self.join.as_ref().ok_or(AgentError::Invalid)?;
            if !join.is_finished() {
                return Ok(None);
            }
            self.pending_exit = Some(
                self.join
                    .take()
                    .ok_or(AgentError::Invalid)?
                    .join()
                    .map_err(|_| AgentError::Unavailable)?,
            );
        }
        let exit = self.pending_exit.as_mut().ok_or(AgentError::Invalid)?;
        if let Some(context) = exit
            .retired_context
            .as_ref()
            .or_else(|| exit.cleanup.as_ref().map(|c| &c.context))
            && let Some(ticket) = &context.lifecycle
        {
            ticket
                .worker_joined(exit.owner.runtime())
                .map_err(|_| AgentError::Disconnect)?;
        }
        // A real Agent OS-thread join has happened. Retain the original owner
        // here until the scheduler itself can be synchronously shut down.
        if tokio::runtime::Handle::try_current().is_ok() && self.scheduler.is_some() {
            self.control
                .progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .scheduler = AgentSchedulerPhase::TokioContext;
            return Ok(None);
        }
        if exit
            .retired_context
            .as_ref()
            .or_else(|| exit.cleanup.as_ref().map(|c| &c.context))
            .is_some_and(|c| !c.resources.is_clean().unwrap_or(false))
        {
            return Ok(None);
        }
        if let Some(context) = &mut exit.retired_context {
            context.release_backend();
        }
        if let Some(cleanup) = &mut exit.cleanup {
            cleanup.context.release_backend();
        }
        if let Some(scheduler) = &self.scheduler {
            let phase = scheduler.try_shutdown();
            self.control
                .progress
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .scheduler = phase;
            if phase != AgentSchedulerPhase::Shutdown {
                return Ok(None);
            }
        }
        if let Some(context) = &mut exit.retired_context {
            context
                .settle_lifecycle(exit.owner.runtime())
                .map_err(|_| AgentError::Disconnect)?;
        }
        exit.retired_context = None;
        self.control
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .phase = AgentPhase::Joined;
        Ok(self.pending_exit.take())
    }
}
impl<O: ManagedHostOwner> Drop for AgentWorker<O> {
    fn drop(&mut self) {
        let _ = catch_unwind(AssertUnwindSafe(|| self.stop()));
    }
}
#[allow(clippy::too_many_arguments)]
fn execute<O: ManagedHostOwner>(
    owner: &mut BudgetOwner<O>,
    package: &mut CatalogManagedPackage,
    context: &mut AgentContext,
    receiver: &mpsc::Receiver<Command>,
    control: &Arc<Control>,
    binding: HostBinding,
    expires: u64,
    clock: &Arc<dyn Fn() -> u64 + Send + Sync>,
) -> Result<(), AgentError> {
    while control.live() {
        if clock() >= expires {
            control.stop();
            break;
        }
        let command = match receiver.recv_timeout(Duration::from_millis(5)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        command.diagnostic.worker(super::commands::diagnostic_stage::RECEIVED, 0);
        if command.cancel.load(Ordering::Acquire) {
            command.diagnostic.worker(super::commands::diagnostic_stage::CANCELLED, super::commands::diagnostic_error(AgentError::Cancelled));
            let _ = command.reply.send(Err(AgentError::Cancelled));
            continue;
        }
        if matches!(&command.value, AgentCommand::StartClaimed { .. }) && context.port.is_none() {
            command.diagnostic.worker(super::commands::diagnostic_stage::PORT_ABSENT, super::commands::diagnostic_error(AgentError::Unavailable));
            let _ = command.reply.send(Err(AgentError::Unavailable));
            continue;
        }
        command.started.store(true, Ordering::Release);
        command.diagnostic.flag(1 << 32);
        command.diagnostic.worker(super::commands::diagnostic_stage::COMMAND_STARTED, 0);
        if command.cancel.load(Ordering::Acquire) {
            control.stop();
        }
        command.diagnostic.worker(super::commands::diagnostic_stage::CHECKPOINT, 0);
        control.checkpoint().map_err(|error| { command.diagnostic.worker(super::commands::diagnostic_stage::CHECKPOINT, super::commands::diagnostic_error(error)); error })?;
        if owner.runtime().binding() != binding {
            command.diagnostic.worker(super::commands::diagnostic_stage::OWNER_MISMATCH, super::commands::diagnostic_error(AgentError::Unknown));
            return Err(AgentError::Unknown);
        }
        let checkpoint = || {
            if command.cancel.load(Ordering::Acquire) || !control.live() || clock() >= expires {
                control.stop();
            }
            clock()
        };
        let result = match command.value {
            AgentCommand::OpenNativeSession => (|| {
                owner.prepare_io().map_err(|_| AgentError::Maintenance)?;
                if context.native_session.is_none() {
                    let lease = owner.with_managed_runtime(|manager, runtime| {
                        context.native_session_package.as_ref().unwrap_or(package)
                            .open_native_session(manager, runtime, context.host.clone(), checkpoint())
                    }).ok_or(AgentError::Unavailable)?.map_err(AgentError::Session)?;
                    context.native_session = Some(lease);
                }
                Ok(AgentReply::NativeEndpoint(context.native_session.as_ref()
                    .ok_or(AgentError::Unavailable)?.control()))
            })(),
            AgentCommand::NativeSessionWriter { session_id } => (|| {
                owner.prepare_io().map_err(|_| AgentError::Maintenance)?;
                let lease = context.native_session.as_mut().ok_or(AgentError::Unavailable)?;
                owner.with_managed_runtime(|manager, runtime| {
                    context.native_session_package.as_ref().unwrap_or(package)
                        .native_session_writer(manager, runtime, lease, &context.host,
                        &session_id, checkpoint())
                }).ok_or(AgentError::Unavailable)?.map(AgentReply::NativeEndpoint)
                    .map_err(AgentError::Session)
            })(),
            AgentCommand::NativeSessionExchange { endpoint, canonical } => (|| {
                owner.prepare_io().map_err(|_| AgentError::Maintenance)?;
                let lease = context.native_session.as_ref().ok_or(AgentError::Unavailable)?;
                owner.with_managed_runtime(|manager, runtime| {
                    context.native_session_package.as_ref().unwrap_or(package)
                        .exchange_native_session(manager, runtime, lease, &context.host,
                        endpoint, &canonical, checkpoint)
                }).ok_or(AgentError::Unavailable)?.map(|bytes|
                    AgentReply::NativeSession(zeroize::Zeroizing::new(bytes)))
                    .map_err(AgentError::Session)
            })(),
            AgentCommand::RunSealedProposal { request_sha256 } => {
                let lease = context
                    .sealed_proposal
                    .as_mut()
                    .ok_or(AgentError::Unavailable)?;
                if lease.request_sha256() != request_sha256 {
                    Err(AgentError::Invalid)
                } else {
                    lease
                        .run_owned(owner, &context.host, checkpoint)
                        .map_err(|_| AgentError::Unknown)
                        .and_then(|mut run| {
                            if run
                                .completion
                                .as_ref()
                                .is_some_and(|b| b.len() > MAX_AGENT_FRAME_BYTES)
                                || !control.live()
                            {
                                erase_run(&mut run);
                                control.stop();
                                return Err(AgentError::Unknown);
                            }
                            Ok(AgentReply::Frame(run))
                        })
                }
            }
            AgentCommand::ReviewTool { operation_id } => {
                super::trusted::review(owner, context, &operation_id, &checkpoint)
            }
            AgentCommand::ApproveClaim {
                request_id,
                operation_id,
                proposal_sha256,
                intent_sha256,
                expected_record_revision,
                expected_record_sha256,
            } => super::trusted::approve_claim(
                owner,
                context,
                &request_id,
                &operation_id,
                proposal_sha256,
                intent_sha256,
                expected_record_revision,
                expected_record_sha256,
                &checkpoint,
            ),
            AgentCommand::RunFrame(input) => package
                .run_owned(
                    owner,
                    &context.host,
                    &mut context.processes,
                    &input,
                    checkpoint,
                )
                .map_err(|_| AgentError::Unknown)
                .and_then(|mut run| {
                    if run
                        .completion
                        .as_ref()
                        .is_some_and(|b| b.len() > MAX_AGENT_FRAME_BYTES)
                    {
                        erase_run(&mut run);
                        control.stop();
                        return Err(AgentError::Unknown);
                    }
                    if !control.live() {
                        erase_run(&mut run);
                        return Err(AgentError::Unknown);
                    }
                    Ok(AgentReply::Frame(run))
                }),
            AgentCommand::StartClaimed {
                reviewed,
                claim,
                capabilities,
                budget,
            } => {
                command.diagnostic.worker(super::commands::diagnostic_stage::PREPARE_IO, 0);
                owner.prepare_io().map_err(|_| { command.diagnostic.worker(super::commands::diagnostic_stage::PREPARE_IO, super::commands::diagnostic_error(AgentError::Maintenance)); AgentError::Maintenance })?;
                control.checkpoint().map_err(|error| { command.diagnostic.worker(super::commands::diagnostic_stage::CHECKPOINT, super::commands::diagnostic_error(error)); error })?;
                let port = context.port.as_ref().ok_or(AgentError::Unavailable)?;
                command.diagnostic.worker(super::commands::diagnostic_stage::PORT_START, 0);
                let provider = port
                    .start_claimed(
                        owner.runtime_mut(),
                        context.executor_connection.clone(),
                        &context.executor_admission,
                        *reviewed,
                        claim,
                        checkpoint,
                        || control.live(),
                    )
                    .map_err(|error| { command.diagnostic.worker(super::commands::diagnostic_stage::PORT_REJECTED, super::commands::diagnostic_session_error(error)); AgentError::Unknown })?;
                command.diagnostic.flag(1 << 33);
                command.diagnostic.worker(super::commands::diagnostic_stage::PROVIDER_OBSERVED, 0);
                let identity = provider.observed_identity().clone();
                command.diagnostic.worker(super::commands::diagnostic_stage::REGISTER_PROCESS, 0);
                owner
                    .with_managed_runtime(|manager, runtime| {
                        package.register_process(
                            manager,
                            runtime,
                            &context.host,
                            &mut context.processes,
                            identity,
                            context.executor_connection.clone(),
                            &context.executor_admission,
                            capabilities,
                            budget,
                            Box::new(provider),
                            checkpoint,
                        )
                    })
                    .ok_or_else(|| { command.diagnostic.worker(super::commands::diagnostic_stage::REGISTER_REJECTED, super::commands::diagnostic_error(AgentError::Unavailable)); AgentError::Unavailable })?
                    .map(AgentReply::Started)
                    .map_err(|_| { command.diagnostic.worker(super::commands::diagnostic_stage::REGISTER_REJECTED, 19); AgentError::Unknown })
            }
        };
        let result = if !control.live() {
            command.diagnostic.worker(super::commands::diagnostic_stage::CONTROL_VETO, super::commands::diagnostic_error(AgentError::Unknown));
            Err(AgentError::Unknown)
        } else {
            result
        };
        let uncertain = result.as_ref().is_err_and(|e| *e == AgentError::Unknown);
        let delivery_failed = command.reply.send(result).is_err();
        if delivery_failed { command.diagnostic.delivery(super::commands::diagnostic_stage::REPLY_SEND_FAILED, super::commands::diagnostic_error(AgentError::Unknown)); }
        if delivery_failed || uncertain {
            control.stop();
        }
        if !control.live() {
            break;
        }
        // Historical completion facts are drained on this same owner only.
        if let Err(error) = drain_ready(&mut owner.original, context) {
            control.reconcile_failed.store(true, Ordering::Release);
            return Err(error);
        }
    }
    Ok(())
}
fn cleanup_step<O: ManagedHostOwner>(
    owner: &mut O,
    package: &CatalogManagedPackage,
    context: &mut AgentContext,
    cursors: &mut BTreeMap<[u8; 32], u64>,
    binding: HostBinding,
    reconcile: bool,
) -> Result<bool, AgentError> {
    if owner.runtime().binding() != binding {
        return Err(AgentError::Unknown);
    }
    context.resources.request_stop();
    for handle in package
        .owned_handles(owner.runtime())
        .map_err(|_| AgentError::Unknown)?
    {
        let seq = *cursors.get(&handle.nonce).unwrap_or(&0);
        match context.processes.trusted_observe(
            handle,
            ReadQuery {
                after_seq: seq,
                max_bytes: 32768,
                max_events: 16,
                wait_ms: 0,
            },
        ) {
            Ok(page) => {
                cursors.insert(handle.nonce, page.next_seq);
                if page.closed && page.exited {
                    context
                        .processes
                        .finish(handle)
                        .map_err(|_| AgentError::Unknown)?;
                }
            }
            Err(morrow_agent_process_control_v1::Error::NotFound) => {}
            Err(_) => return Err(AgentError::Unknown),
        }
    }
    // Sealing remains on the original protected owner, independent of guest authority.
    if reconcile {
        drain_ready(owner, context)?;
    }
    context
        .resources
        .reap_cleanup()
        .map_err(|_| AgentError::Unknown)?;
    context
        .resources
        .is_clean()
        .map_err(|_| AgentError::Unknown)
}
fn erase_run(run: &mut morrow_plugin_runtime::TaskRun) {
    if let Some(bytes) = &mut run.completion {
        bytes.fill(0);
    }
    run.completion = None;
}
fn drain_ready<O: ManagedHostOwner>(
    owner: &mut O,
    context: &AgentContext,
) -> Result<(), AgentError> {
    let pending = context
        .resources
        .pending_facts()
        .map_err(|_| AgentError::Unknown)?;
    if !pending.iter().any(|p| p.facts.is_some()) {
        return Ok(());
    }
    owner.prepare_io().map_err(|_| AgentError::Maintenance)?;
    let report = context
        .resources
        .drain_completed(owner.runtime_mut(), &context.host)
        .map_err(|_| AgentError::Unknown)?;
    if !report.errors.is_empty()
        || report.retained != 0
            && context
                .resources
                .pending_facts()
                .map_err(|_| AgentError::Unknown)?
                .iter()
                .any(|p| p.facts.is_some())
    {
        return Err(AgentError::Unknown);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // Ordinary disposable SQLite + WAT qualify owner movement and maintenance;
    // they do not exercise ProtectedSession, DPAPI or a native child process.
    use super::*;
    use morrow_agent_session_exec_v1_r2::{Action, Request, authority::Capabilities};
    use morrow_agent_session_process_v1_host::{
        AgentProcessPackage, Declaration,
        catalog::{Approval, Catalog, Revisions},
    };
    use morrow_core::{
        dispatch::HostRuntime,
        plugin_package::{Package, catalog::Catalog as BaseCatalog, registry::Registry},
        store::{EventBudget, Store},
    };
    use morrow_plugin_runtime::Limits;
    use std::{sync::atomic::AtomicUsize, time::Instant};
    use zeroize::Zeroizing;

    const ID: &str = "org.example.original-agent-owner";
    static SERIAL: Mutex<()> = Mutex::new(());
    struct OrdinaryOwner {
        runtime: HostRuntime,
        manager: Manager,
        _catalog: Catalog,
        _temp: tempfile::TempDir,
        identity: Box<u64>,
        imports: Arc<AtomicUsize>,
        fail_prepare: Arc<AtomicBool>,
        fail_finish: Arc<AtomicBool>,
    }
    impl HostOwner for OrdinaryOwner {
        fn runtime(&self) -> &HostRuntime {
            &self.runtime
        }
        fn runtime_mut(&mut self) -> &mut HostRuntime {
            &mut self.runtime
        }
        fn prepare_io(&mut self) -> Result<(), JobError> {
            self.imports.fetch_add(1, Ordering::SeqCst);
            if self.fail_prepare.load(Ordering::Acquire) {
                return Err(JobError::Unavailable);
            }
            Ok(())
        }
        fn finish_io(&mut self) -> Result<(), JobError> {
            if self.fail_finish.load(Ordering::Acquire) {
                return Err(JobError::Unavailable);
            }
            Ok(())
        }
    }
    impl ManagedHostOwner for OrdinaryOwner {
        fn manager(&self) -> Option<&Manager> {
            Some(&self.manager)
        }
        fn with_managed_runtime<T>(
            &mut self,
            action: impl FnOnce(&Manager, &mut HostRuntime) -> T,
        ) -> Option<T> {
            Some(action(&self.manager, &mut self.runtime))
        }
    }
    fn fixture() -> (OrdinaryOwner, CatalogManagedPackage, AgentContext) {
        fixture_with_process(true)
    }
    fn fixture_with_process(process_read: bool) -> (OrdinaryOwner, CatalogManagedPackage, AgentContext) {
        fixture_with_expiry(process_read, 60_000)
    }
    fn fixture_with_expiry(process_read: bool, package_expires: u64) -> (OrdinaryOwner, CatalogManagedPackage, AgentContext) {
        let temp = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
          (import "morrow_agent_session_process_v1" "call" (func $call (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 6)
          (func (export "morrow_run") (result i32) (local $input i32) (local $reply i32)
            (local.set $input (call $read (i32.const 0) (i32.const 131072)))
            (drop (call $call (i32.const 0) (local.get $input) (i32.const 131072) (i32.const 131072)))
            (local.set $reply (call $call (i32.const 0) (local.get $input) (i32.const 131072) (i32.const 131072)))
            (drop (call $done (i32.const 131072) (local.get $reply))) i32.const 0))"#).unwrap();
        let base = Package::build(
            Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]),
            &wasm,
        )
        .unwrap();
        assert!(base.io_declaration().is_none());
        let registry = Registry::open(
            &temp.path().join("registry"),
            BaseCatalog::open(&temp.path().join("packages")).unwrap(),
        )
        .unwrap();
        let mut manager = Manager::new(
            registry,
            Limits {
                fuel: 100_000_000,
                memory_bytes: 16 * 1024 * 1024,
                host_calls: 16,
            },
        );
        manager.install_package(base.archive()).unwrap();
        manager.select(&base, manager.revision()).unwrap();
        manager
            .set_enabled(ID, base.digest(), true, manager.revision())
            .unwrap();
        let mut runtime = HostRuntime::new(
            Store::open(&temp.path().join("ordinary.sqlite"), EventBudget::default()).unwrap(),
        )
        .unwrap();
        let host = Arc::new(SessionExecHost::new(&mut runtime).unwrap());
        let rights = Capabilities {
            session_read: true,
            session_write: true,
            ..Default::default()
        };
        let pc = morrow_agent_process_control_v1::Capabilities {
            read: process_read,
            ..Default::default()
        };
        let wrapper = AgentProcessPackage::build(
            Package::decode(base.archive()).unwrap(),
            Declaration {
                session: rights,
                process: pc,
                sessions: if process_read { vec!["session".into()] }
                else { vec!["session".into(), "session-b".into()] },
                execution_domain: "domain".into(),
            },
        )
        .unwrap();
        let mut catalog = Catalog::open(&temp.path().join("wrappers"), true).unwrap();
        let revisions = |c: &Catalog, m: &Manager| Revisions {
            catalog: c.revision(),
            manager: m.revision(),
        };
        let expected = revisions(&catalog, &manager);
        catalog
            .install(
                wrapper.archive(),
                wrapper.review_sha256(),
                &mut manager,
                expected,
            )
            .unwrap();
        let expected = revisions(&catalog, &manager);
        catalog
            .select(wrapper.review_sha256(), &mut manager, expected)
            .unwrap();
        let expected = revisions(&catalog, &manager);
        catalog
            .approve(
                ID,
                wrapper.review_sha256(),
                Approval {
                    session: rights,
                    process: pc,
                    sessions: if process_read { vec!["session".into()] }
                else { vec!["session".into(), "session-b".into()] },
                    execution_domain: "domain".into(),
                },
                &mut manager,
                expected,
            )
            .unwrap();
        let expected = revisions(&catalog, &manager);
        catalog
            .set_enabled(ID, wrapper.review_sha256(), true, &mut manager, expected)
            .unwrap();
        let expected = revisions(&catalog, &manager);
        let package = catalog
            .connect(
                ID,
                wrapper.review_sha256(),
                &mut manager,
                &mut runtime,
                &host,
                expected,
                package_expires,
                1,
            )
            .unwrap();
        let executor_connection = Arc::new(runtime.connect().unwrap());
        let executor_rights = Capabilities {
            session_read: true,
            execute: true,
            ..Default::default()
        };
        let executor_admission = host
            .admit(
                &runtime,
                &executor_connection,
                executor_rights,
                executor_rights,
                if process_read { vec!["session".into()] }
                else { vec!["session".into(), "session-b".into()] },
                "domain".into(),
                60_000,
                1,
            )
            .unwrap();
        let resources = BorrowedNativeResources::for_owner(&runtime).unwrap();
        let context = AgentContext::new(
            host,
            ProcessHost::default(),
            resources,
            None,
            executor_connection,
            executor_admission,
        )
        .unwrap_or_else(|_| panic!("ordinary context rejected"));
        let owner = OrdinaryOwner {
            runtime,
            manager,
            _catalog: catalog,
            _temp: temp,
            identity: Box::new(37),
            imports: Arc::new(AtomicUsize::new(0)),
            fail_prepare: Arc::new(AtomicBool::new(false)),
            fail_finish: Arc::new(AtomicBool::new(false)),
        };
        (owner, package, context)
    }
    fn spawn(
        owner: OrdinaryOwner,
        package: CatalogManagedPackage,
        context: AgentContext,
    ) -> AgentWorker<OrdinaryOwner> {
        AgentWorker::spawn(
            owner,
            package,
            context,
            ProductGate::default(),
            AgentLimits::default(),
            60_000,
            Arc::new(|| 6),
        )
        .unwrap_or_else(|_| panic!("ordinary owner spawn failed"))
    }
    fn join(worker: &mut AgentWorker<OrdinaryOwner>) -> AgentExit<OrdinaryOwner> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(exit) = worker.try_reclaim().unwrap() {
                return exit;
            }
            assert!(
                Instant::now() < deadline,
                "actual agent join remained pending"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn reply(handle: &mut AgentCommandHandle) -> AgentReply {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(reply) = handle.try_read().unwrap() {
                return reply;
            }
            assert!(Instant::now() < deadline, "reply remained pending");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn create(package: &CatalogManagedPackage) -> Vec<u8> {
        Request::new_for_generation(
            "original-create",
            package.generation(),
            Action::Create {
                session_id: "session".into(),
                parent: None,
                parent_tail: 0,
            },
        )
        .unwrap()
        .raw()
        .to_vec()
    }
    #[test]
    fn original_owner_moves_and_maintains_each_import_without_io_carrier() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture();
        let address = (&*owner.identity) as *const u64 as usize;
        let binding = owner.runtime.binding();
        let proposer = package.shared_connection();
        let executor = context.executor_connection.clone();
        let imports = owner.imports.clone();
        let input = create(&package);
        let mut worker = spawn(owner, package, context);
        let mut handle = worker
            .submit(AgentCommand::RunFrame(Zeroizing::new(input)))
            .unwrap();
        let answer = reply(&mut handle);
        let AgentReply::Frame(run) = &answer else {
            panic!("expected real WAT task report");
        };
        assert_eq!(run.report.outcome, Ok(0));
        assert_eq!(imports.load(Ordering::SeqCst), 2);
        assert_eq!(worker.progress().imported_frames, 2);
        worker.stop();
        let exit = join(&mut worker);
        assert_eq!(address, (&*exit.owner.identity) as *const u64 as usize);
        assert_eq!(binding, exit.owner.runtime.binding());
        assert!(exit.disconnect.is_ok());
        assert!(exit.owner.runtime.connection_phase(&proposer).is_err());
        assert!(exit.owner.runtime.connection_phase(&executor).is_err());
        assert!(exit.cleanup.is_none());
    }
    #[test]
    fn failed_original_maintenance_prevents_first_import_effect_and_returns_owner() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture();
        let imports = owner.imports.clone();
        let host = context.host.clone();
        owner.fail_prepare.store(true, Ordering::Release);
        let input = create(&package);
        let mut worker = spawn(owner, package, context);
        let mut handle = worker
            .submit(AgentCommand::RunFrame(Zeroizing::new(input)))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match handle.try_read() {
                Err(AgentError::Unknown) => break,
                Ok(None) => {}
                _ => panic!("maintenance loss must not deliver success"),
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(imports.load(Ordering::SeqCst), 1);
        worker.stop();
        let mut exit = join(&mut worker);
        assert!(exit.disconnect.is_ok());
        let connection = exit.owner.runtime.connect().unwrap();
        let caps = Capabilities {
            session_read: true,
            ..Default::default()
        };
        // A failed maintenance hook cannot have created the proposed session.
        let grant = host
            .admit(
                &exit.owner.runtime,
                &connection,
                caps,
                caps,
                vec!["session".into()],
                "domain".into(),
                60_000,
                7,
            )
            .unwrap();
        let request = Request::new_for_generation(
            "read-uncreated",
            host.generation(&exit.owner.runtime).unwrap(),
            Action::Snapshot {
                session_id: "session".into(),
                after: 0,
                limit: 1,
            },
        )
        .unwrap();
        let output = host
            .dispatch(
                &mut exit.owner.runtime,
                &connection,
                &grant,
                request.raw(),
                || 7,
            )
            .unwrap();
        assert!(matches!(
            morrow_agent_session_exec_v1_r2::Reply::decode_for(&request, &output)
                .unwrap()
                .outcome,
            morrow_agent_session_exec_v1_r2::Outcome::Rejected(
                morrow_agent_session_exec_v1_r2::Error::NotFound
            )
        ));
        exit.owner.runtime.disconnect(&connection).unwrap();
    }
    #[test]
    fn maintenance_repair_does_not_reclose_confirmed_original_connections() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture();
        let failed = owner.fail_finish.clone();
        failed.store(true, Ordering::Release);
        let mut worker = spawn(owner, package, context);
        worker.stop();
        let mut exit = join(&mut worker);
        assert!(exit.disconnect.is_ok());
        assert_eq!(exit.maintenance, Err(AgentError::Maintenance));
        let mut cleanup = exit.cleanup.take().expect("retained exact cleanup");
        assert!(cleanup.package_closed && cleanup.executor_closed);
        failed.store(false, Ordering::Release);
        cleanup.repair(&mut exit.owner).unwrap();
        assert_eq!(*exit.owner.identity, 37);
    }

    fn scheduler() -> (Arc<tokio::runtime::Runtime>, Arc<AtomicUsize>) {
        let stopped = Arc::new(AtomicUsize::new(0));
        let observed = stopped.clone();
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .on_thread_stop(move || {
                    observed.fetch_add(1, Ordering::SeqCst);
                })
                .build()
                .unwrap(),
        );
        // Exercise real scheduler threads, not a fake runtime or release counter.
        runtime.block_on(async {
            tokio::spawn(async {}).await.unwrap();
        });
        (runtime, stopped)
    }
    fn joined_pending(worker: &mut AgentWorker<OrdinaryOwner>) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                worker.try_reclaim().unwrap().is_none(),
                "owner returned before scheduler retirement"
            );
            if worker.pending_exit.is_some() {
                break;
            }
            assert!(Instant::now() < deadline, "actual OS join remained pending");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            worker.join.is_none(),
            "a pending Exit follows actual OS join"
        );
    }
    #[test]
    fn sole_scheduler_is_synchronously_retired_after_actual_worker_join() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, mut context) = fixture();
        let (runtime, stopped) = scheduler();
        // Private test injection exercises the exact scheduler token without a
        // provisioned child backend. It does not qualify native process behavior.
        context.scheduler =
            Some(SchedulerLease::new(runtime.clone(), context.resources.clone()).unwrap());
        drop(runtime);
        let mut worker = spawn(owner, package, context);
        assert_eq!(stopped.load(Ordering::SeqCst), 0);
        worker.stop();
        let exit = join(&mut worker);
        assert_eq!(*exit.owner.identity, 37);
        assert_eq!(worker.progress().scheduler, AgentSchedulerPhase::Shutdown);
        assert!(
            stopped.load(Ordering::SeqCst) >= 2,
            "real scheduler threads stopped"
        );
    }
    #[test]
    fn external_scheduler_arc_keeps_joined_original_owner_pending() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, mut context) = fixture();
        let address = (&*owner.identity) as *const u64 as usize;
        let (runtime, stopped) = scheduler();
        context.scheduler =
            Some(SchedulerLease::new(runtime.clone(), context.resources.clone()).unwrap());
        let mut worker = spawn(owner, package, context);
        worker.stop();
        joined_pending(&mut worker);
        assert_eq!(
            worker.progress().scheduler,
            AgentSchedulerPhase::WaitingForReferences
        );
        assert_eq!(
            address,
            (&*worker.pending_exit.as_ref().unwrap().owner.identity) as *const u64 as usize
        );
        assert_eq!(stopped.load(Ordering::SeqCst), 0);
        drop(runtime);
        let exit = join(&mut worker);
        assert_eq!(address, (&*exit.owner.identity) as *const u64 as usize);
        assert!(stopped.load(Ordering::SeqCst) >= 2);
    }
    #[test]
    fn tokio_context_cannot_retire_scheduler_or_return_joined_owner() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, mut context) = fixture();
        let (runtime, stopped) = scheduler();
        let handle = runtime.handle().clone();
        context.scheduler =
            Some(SchedulerLease::new(runtime.clone(), context.resources.clone()).unwrap());
        drop(runtime);
        let mut worker = spawn(owner, package, context);
        worker.stop();
        {
            let _entered = handle.enter();
            joined_pending(&mut worker);
            assert_eq!(
                worker.progress().scheduler,
                AgentSchedulerPhase::TokioContext
            );
            assert_eq!(stopped.load(Ordering::SeqCst), 0);
        }
        // A Handle is not a strong Arc<Runtime>; outside its entered context the
        // original token can become sole owner and synchronously shut it down.
        let exit = join(&mut worker);
        assert_eq!(*exit.owner.identity, 37);
        assert_eq!(worker.progress().scheduler, AgentSchedulerPhase::Shutdown);
        assert!(stopped.load(Ordering::SeqCst) >= 2);
    }
    #[test]
    fn repeated_unused_contexts_release_their_keepalive_without_native_effects() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, original) = fixture();
        for _ in 0..8 {
            let (runtime, stopped) = scheduler();
            let mut unused = AgentContext::new(
                original.host.clone(),
                ProcessHost::default(),
                original.resources.clone(),
                None,
                original.executor_connection.clone(),
                original.executor_admission.clone(),
            )
            .unwrap_or_else(|_| panic!("unused context rejected"));
            // Same private token injection as the scheduler-only tests. No
            // backend, worker, grant change or child process is substituted.
            unused.scheduler =
                Some(SchedulerLease::new(runtime.clone(), unused.resources.clone()).unwrap());
            let weak_runtime = Arc::downgrade(&runtime);
            drop(runtime);
            drop(unused);
            assert!(
                weak_runtime.upgrade().is_none(),
                "sole unused scheduler must be synchronously retired"
            );
            assert_eq!(agent_scheduler_status().retained_debts, 0);
            assert!(stopped.load(Ordering::SeqCst) >= 2);
        }
        assert_eq!(owner.imports.load(Ordering::SeqCst), 0);
        assert!(original.resources.is_clean().unwrap());
        let mut worker = spawn(owner, package, original);
        worker.stop();
        assert!(join(&mut worker).disconnect.is_ok());
    }
    #[test]
    fn unused_context_and_lease_keep_async_runtime_alias_until_explicit_reap() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, original) = fixture();
        let binding = owner.runtime.binding();
        assert_eq!(agent_scheduler_status().retained_debts, 0);
        for direct_lease_drop in [false, true] {
            let (runtime, stopped) = scheduler();
            let weak_runtime = Arc::downgrade(&runtime);
            let mut unused = AgentContext::new(
                original.host.clone(),
                ProcessHost::default(),
                original.resources.clone(),
                None,
                original.executor_connection.clone(),
                original.executor_admission.clone(),
            )
            .unwrap_or_else(|_| panic!("unused context rejected"));
            unused.scheduler =
                Some(SchedulerLease::new(runtime.clone(), unused.resources.clone()).unwrap());
            let captured_runtime = runtime.clone();
            let (release, released) = tokio::sync::oneshot::channel();
            let (entered, entered_rx) = mpsc::channel();
            let (finished, finished_rx) = mpsc::channel();
            let task = runtime.spawn(async move {
                assert!(tokio::runtime::Handle::try_current().is_ok());
                entered.send(()).unwrap();
                released.await.unwrap();
                // Actual Tokio task capture outlives the clean native count.
                // Its Arc must never become the last scheduler owner here.
                drop(captured_runtime);
                finished.send(()).unwrap();
            });
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(unused.resources.is_clean().unwrap());
            drop(runtime);
            if direct_lease_drop {
                // Also cover token destruction without AgentContext's earlier
                // release_unused call; both paths must retain the same debt.
                drop(unused.scheduler.take());
            }
            drop(unused);
            assert_eq!(agent_scheduler_status().retained_debts, 1);
            assert_eq!(agent_scheduler_status().active_tokens, 0);
            assert_eq!(weak_runtime.strong_count(), 2);
            assert_eq!(stopped.load(Ordering::SeqCst), 0);
            assert_eq!(reap_agent_scheduler_debts().unwrap(), 0);
            assert_eq!(agent_scheduler_status().retained_debts, 1);
            release.send(()).unwrap();
            finished_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(reap_agent_scheduler_debts().unwrap(), 1);
            assert!(weak_runtime.upgrade().is_none());
            assert!(stopped.load(Ordering::SeqCst) >= 2);
            assert_eq!(agent_scheduler_status().retained_debts, 0);
            drop(task);
        }
        assert_eq!(owner.imports.load(Ordering::SeqCst), 0);
        assert_eq!(owner.runtime.binding(), binding);
        let mut worker = spawn(owner, package, original);
        worker.stop();
        assert!(join(&mut worker).disconnect.is_ok());
    }
    #[test]
    fn scheduler_debt_is_bounded_visible_and_explicitly_reaped_without_core_copy() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (owner, package, context) = fixture();
        let binding = owner.runtime.binding();
        assert_eq!(agent_scheduler_status().retained_debts, 0);
        for _ in 0..MAX_AGENT_SCHEDULERS {
            let (runtime, _) = scheduler();
            let lease = SchedulerLease::new(runtime.clone(), context.resources.clone()).unwrap();
            lease.started.store(true, Ordering::Release);
            drop(runtime);
            // Models an uncertain abandoned token. No native child, second Core
            // or protected owner is invented to make its cleanup appear ready.
            drop(lease);
        }
        let status = agent_scheduler_status();
        assert_eq!(status.retained_debts, MAX_AGENT_SCHEDULERS);
        assert_eq!(status.active_tokens, 0);
        let (extra, _) = scheduler();
        assert_eq!(
            SchedulerLease::new(extra.clone(), context.resources.clone()).err(),
            Some(AgentError::Limit)
        );
        assert_eq!(
            Arc::strong_count(&extra),
            1,
            "refused token returns original scheduler ownership"
        );
        drop(extra);
        assert_eq!(owner.runtime.binding(), binding);
        assert_eq!(reap_agent_scheduler_debts().unwrap(), MAX_AGENT_SCHEDULERS);
        assert_eq!(agent_scheduler_status().retained_debts, 0);
        let mut worker = spawn(owner, package, context);
        worker.stop();
        let exit = join(&mut worker);
        assert_eq!(exit.owner.runtime.binding(), binding);
    }
    include!("native_session_worker_tests.rs");


}
