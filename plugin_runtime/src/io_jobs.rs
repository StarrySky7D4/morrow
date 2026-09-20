//! Bounded IO job executor: submit, poll, read and cancel on one native thread.
//! Each job carries a trusted-host contract router for its IO calls; this module
//! performs no external network/file operations itself, never retries a call, and never delivers a
//! result that outlived its authorization or deadline as a success.
use crate::{
    Cancellation, Fault, MAX_TASK_BYTES, Report,
    io_binding::{
        Error as BindingError, IoBinding, IoJobLease, ServiceRunBudget, ServiceRunSnapshot,
        ServiceRunUsage,
    },
    io_execution::{self, Broker},
    manager::{ManagedInstance, Manager},
    package::{PreparedPackage, TaskReport},
    service_content::ServiceContentAccess,
    service_history::{self, ServiceJournal},
    service_io::{ListenerGrant, ServiceGrant},
    worker::TaskId,
};
use morrow_core::{
    dispatch::{Connection, HostBinding, HostRuntime},
    io::{Action, HttpOutcome, MAX_FRAME_BYTES, Request, Response},
    io_evidence::{Kind, Material},
    io_intent::{Command, Phase as IntentPhase, Record},
    plugin_package::io::IoCapability,
    service,
};
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub const MAX_PENDING: usize = 64;
pub const MAX_TIMEOUT: Duration = Duration::from_secs(3600);
pub const MAX_CALLS: u32 = 1024;
pub const MAX_JOB_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
static NEXT_EXECUTOR: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobError {
    InvalidOptions,
    Busy,
    Closed,
    Unavailable,
    Consumed,
    ReadBound,
    Limit,
    Spawn,
    Disconnect,
}
/// Trusted owner of the original runtime and its storage/identity guards. The
/// returned runtime must remain the same for this owner's complete lifetime;
/// hooks must not replace it, revive grants, or re-enter worker handles.
pub trait HostOwner: Send + 'static {
    fn runtime(&self) -> &HostRuntime;
    fn runtime_mut(&mut self) -> &mut HostRuntime;
    fn prepare_io(&mut self) -> Result<(), JobError> {
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        Ok(())
    }
}
impl HostOwner for HostRuntime {
    fn runtime(&self) -> &HostRuntime {
        self
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        self
    }
}
/// Execution and maintenance are independent: sealing failure cannot undo an
/// already committed effect. Reclamation never grants permission to replay it.
pub struct WorkerExit<O: HostOwner> {
    pub owner: O,
    pub result: Result<(), JobError>,
    pub maintenance: Result<(), JobError>,
    /// Failed cleanup is separate from execution. The exact managed instance is
    /// retained only when its disconnection could not be confirmed.
    pub disconnect: Result<(), JobError>,
    pub instance: Option<ManagedInstance>,
}
/// Admission did not start a worker. The original owner and managed instance
/// remain available for explicit cleanup; neither is silently reconstructed.
pub struct SpawnFailure<O: HostOwner> {
    pub owner: O,
    pub instance: Option<ManagedInstance>,
    pub error: JobError,
}
impl<O: HostOwner> std::fmt::Debug for SpawnFailure<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Running,
    Draining,
    Stopping,
    Stopped,
    Failed,
}
/// Nonblocking observation. `Pending` is not failure; `Unavailable` means the
/// executor ended without delivering an authoritative result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Poll {
    Pending,
    Ready,
    Consumed,
    Unavailable,
}
/// Why a routed call did not produce a broker response. Denial and quota are
/// local; `Unknown` means an external effect may have happened and must be
/// reconciled, never retried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouterFault {
    Denied,
    Limit,
    Unknown,
}
/// Trusted-host resource-authorizing router supplied per job, not a guest grant.
/// Calls arrive in guest order and are never replayed. The router must bound its
/// own allocations and blocking work; cancellation cannot interrupt a synchronous
/// external effect already in progress. This executor does not implement network IO.
pub trait Router: Send {
    fn route(&mut self, call: u32, request: &[u8]) -> Result<Vec<u8>, RouterFault>;
}
/// Trusted resource-authorizing adapter. Its actual effect must go through the
/// supplied context; returning invented success without dispatch is rejected.
pub trait BrokerRouter: Send {
    fn route(
        &mut self,
        context: &mut RouteContext<'_>,
        call: u32,
        request: &Request,
    ) -> Result<Vec<u8>, RouterFault>;
}
enum JobRouter {
    /// History queries have no executable resource adapter.
    ReadOnly,
    Raw(Box<dyn Router>),
    Brokered(Box<dyn BrokerRouter>),
}
/// Scope of one actual guest IO import. Private fields prevent a router from
/// substituting a host, managed identity, request or job reservation.
pub struct RouteContext<'a> {
    host: &'a mut HostRuntime,
    instance: &'a ManagedInstance,
    broker: &'a Broker,
    control: &'a Control,
    lease: &'a Arc<IoJobLease>,
    cancel: &'a Cancellation,
    request: &'a Request,
    reserved: &'a mut u64,
    used: bool,
    expected: Option<Vec<u8>>,
    uncertain: bool,
    http_guard: Option<crate::http_io::HttpCallGuard>,
    service_validity: Option<&'a service_history::Validity>,
    service_content: Option<&'a ServiceContentAccess>,
}
impl RouteContext<'_> {
    /// Validate a host-issued endpoint grant against this exact managed import.
    /// The returned monitor can be moved into an asynchronous transport without
    /// borrowing the host or keeping worker-state locks alive.
    pub fn authorize_http(
        &mut self,
        grant: &crate::http_io::HttpGrant,
    ) -> io_execution::Result<crate::http_io::HttpCallGuard> {
        if self.http_guard.is_some() {
            return Err(io_execution::Error::Duplicate);
        }
        let authority = self
            .control
            .authority
            .as_ref()
            .ok_or(io_execution::Error::Denied)?;
        let guard = grant
            .authorize(
                self.host,
                self.instance,
                self.lease,
                self.request,
                self.cancel.clone(),
                Arc::clone(&authority.clock),
            )?
            .with_service_validity(self.service_validity)?
            .with_service_content(self.service_content)?;
        self.http_guard = Some(guard.clone());
        Ok(guard)
    }
    /// Persist the exact framed request, commit the send boundary, and execute
    /// one bounded HTTP-frame backend once. Resource selection remains the
    /// trusted router's responsibility; this method does not create its grant.
    pub fn dispatch(
        &mut self,
        command: &Command,
        backend: impl FnOnce(&[u8]) -> Result<Vec<u8>, ()>,
    ) -> io_execution::Result<Vec<u8>> {
        use io_execution::{Error, storage};
        if let Some(validity) = self.service_validity {
            validity.check()?;
        }
        if self.used {
            return Err(Error::Duplicate);
        }
        self.used = true;
        let prepared = Record::prepared(command.clone()).map_err(storage)?;
        let authority = self.control.authority.as_ref().ok_or(Error::Denied)?;
        let child = {
            let mut state = self.control.lock();
            let next = state
                .bytes
                .checked_add(command.response_limit)
                .filter(|n| *n <= self.control.limits.max_total_bytes)
                .ok_or(Error::Limit)?;
            let child = authority.with_execution_time(|now| {
                self.lease
                    .reserve_call(
                        self.host,
                        self.instance,
                        command,
                        self.request,
                        self.cancel.clone(),
                        now,
                    )
                    .map_err(Error::from)
            })?;
            state.bytes = next;
            *self.reserved = command.response_limit;
            child
        };
        let authorize = || {
            if let Some(validity) = self.service_validity {
                validity
                    .check()
                    .map_err(|_| morrow_core::Error::Invalid("expired service request"))?;
            }
            authority
                .with_execution_time(|now| {
                    if let Some(access) = self.service_content {
                        access.check_at(now)?;
                    }
                    child.check(now).map_err(Error::from)
                })
                .map_err(|_| morrow_core::Error::Invalid("inactive IO job"))
        };
        if let Some(stored) = self
            .host
            .store_local()
            .lookup_io_intent(&command.subject, &command.operation_id)
            .map_err(storage)?
        {
            stored.matches_command(command).map_err(storage)?;
            match stored.phase() {
                IntentPhase::Prepared => {}
                IntentPhase::OutcomeUnknown => {
                    self.uncertain = true;
                    return Err(Error::OutcomeUnknown);
                }
                IntentPhase::Observed => return Err(Error::Dispatched),
                IntentPhase::CancelledBeforeDispatch => return Err(Error::Cancelled),
                IntentPhase::InvalidPhase => return Err(Error::Integrity),
            }
        } else {
            self.host
                .store_local_mut()
                .append_io_intent_local_authorized(&prepared, authorize)
                .map_err(storage)?;
        }
        self.host
            .store_local_mut()
            .reserve_io_intent_followup(command, authorize)
            .map_err(storage)?;
        self.host
            .store_local_mut()
            .reserve_io_materials(command, authorize)
            .map_err(storage)?;
        let material = Material::encode(
            Kind::Request,
            &command.operation_id,
            &command.subject,
            command.request_sha256,
            self.request.bytes(),
        )
        .map_err(storage)?;
        self.host
            .store_local_mut()
            .store_io_material(&command.subject, Kind::Request, &material, authorize)
            .map_err(storage)?;
        authority.with_execution_time(|now| {
            self.broker
                .begin_in_job(self.host, self.instance, child, now)
        })?;
        self.uncertain = true;
        let result = self.broker.dispatch_in_job(
            self.host,
            self.instance,
            &command.operation_id,
            |raw| {
                if let Some(validity) = self.service_validity {
                    validity.check().map_err(|_| ())?;
                }
                if let Some(access) = self.service_content {
                    authority
                        .with_execution_time(|now| access.check_at(now).map_err(Error::from))
                        .map_err(|_| ())?;
                }
                let response = backend(raw)?;
                Response::decode_http(self.request, &response).map_err(|_| ())?;
                Ok(response)
            },
            |live| {
                if let Some(validity) = self.service_validity {
                    validity.check()?;
                }
                authority.with_execution_time(|now| {
                    if let Some(access) = self.service_content {
                        access.check_at(now)?;
                    }
                    live.check_liveness(now)
                })
            },
        );
        // Forget only live bookkeeping, never durable Unknown/Observed history.
        self.broker.retire(&command.operation_id);
        if let Ok(response) = &result {
            self.uncertain = false;
            self.expected = Some(response.clone());
        }
        result
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobLimits {
    pub max_calls: u32,
    pub max_job_bytes: u64,
    pub max_total_bytes: u64,
}
impl Default for JobLimits {
    fn default() -> Self {
        Self {
            max_calls: 16,
            max_job_bytes: MAX_JOB_BYTES,
            max_total_bytes: MAX_TOTAL_BYTES,
        }
    }
}
impl JobLimits {
    pub fn new(max_calls: u32, max_job_bytes: u64, max_total_bytes: u64) -> Result<Self, JobError> {
        if !(1..=MAX_CALLS).contains(&max_calls)
            || !(1..=MAX_JOB_BYTES).contains(&max_job_bytes)
            || !(max_job_bytes..=MAX_TOTAL_BYTES).contains(&max_total_bytes)
        {
            return Err(JobError::InvalidOptions);
        }
        Ok(Self {
            max_calls,
            max_job_bytes,
            max_total_bytes,
        })
    }
}
/// Historical request handling does not imply the guest executed on this attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServicePersistenceStatus {
    Completed,
    Replayed,
    /// No matching historical request; querying did not prepare one.
    Missing,
    /// A retained request has not crossed its dispatch boundary.
    Prepared,
    /// A retained cancellation before dispatch.
    Cancelled,
    /// A read-only query returned the validated original completion.
    Observed,
    Unknown,
    Expired,
    Conflict,
    Unavailable,
}
pub struct JobReport {
    /// Execution facts; content imports require an explicit durable service access.
    pub task: TaskReport,
    /// The last brokered IO response, decoded against its own request.
    pub response: Option<Response>,
    /// HTTP result, validated against its exact submission frame.
    pub http_response: Option<HttpOutcome>,
    /// Service completion bound to its original request, independent of IO responses.
    pub service_response: Option<service::Reply>,
    pub service_persistence: Option<ServicePersistenceStatus>,
    service_validity: Option<service_history::Validity>,
    /// Admitted IO calls, in guest order.
    pub calls: u32,
    /// Cumulatively charged request + response bytes for this job.
    pub bytes: u64,
    /// Authorization, cancellation or deadline prevented result delivery.
    pub cancelled: bool,
    /// The last routed call had an unknown external outcome; reconcile.
    pub unknown: bool,
}
impl std::fmt::Debug for JobReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobReport")
            .field("execution", &self.task.execution)
            .field("response_present", &self.response.is_some())
            .field(
                "http_status",
                &self.http_response.as_ref().map(|r| r.http_status),
            )
            .field(
                "service_status",
                &self.service_response.as_ref().map(|r| r.status),
            )
            .field("service_persistence", &self.service_persistence)
            .field("calls", &self.calls)
            .field("bytes", &self.bytes)
            .field("cancelled", &self.cancelled)
            .field("unknown", &self.unknown)
            .finish()
    }
}
impl JobReport {
    /// Fresh historical-result retention check; this never restores live authority.
    pub fn service_retention_valid(&self) -> bool {
        self.service_validity
            .as_ref()
            .is_none_or(|validity| validity.check().is_ok())
    }
    /// Bytes a caller must be willing to read before the result is consumed.
    pub fn payload_bytes(&self) -> usize {
        self.service_response.as_ref().map_or(0, |r| {
            r.body.len()
                + r.headers
                    .iter()
                    .map(|h| h.name.len() + h.value.len())
                    .sum::<usize>()
        }) + self.http_response.as_ref().map_or(0, |r| {
            r.body.len()
                + r.headers
                    .iter()
                    .map(|h| h.name.len() + h.value.len())
                    .sum::<usize>()
        }) + self.response.as_ref().map_or(0, |r| r.payload.len())
            + self.task.output.as_ref().map_or(0, |o| o.bytes.len())
            + self.task.failure.as_ref().map_or(0, |f| f.message.len())
    }
}
/// Slots remain charged through Ready until read/drop. An abandoned running
/// job keeps its slot until the synchronous router actually returns.
struct Slot {
    cancel: Cancellation,
    report: Option<JobReport>,
    abandoned: bool,
    _lease: Option<Arc<IoJobLease>>,
    http_guards: Vec<crate::http_io::HttpCallGuard>,
    service_grant: Option<ServiceGrant>,
    service_content: Option<ServiceContentAccess>,
}
struct State {
    phase: Phase,
    next: u64,
    jobs: BTreeMap<u64, Slot>,
    bytes: u64,
}
struct Authority {
    cancellation: Cancellation,
    binding: IoBinding,
    clock: crate::http_io::SharedClock,
}
impl Authority {
    // Serialize sampling AND validation in the original host clock domain.
    fn with_time<T>(
        &self,
        f: impl FnOnce(u64) -> Result<T, BindingError>,
    ) -> Result<T, BindingError> {
        let mut clock = self.clock.lock().map_err(|_| BindingError::Denied)?;
        f(clock())
    }
    fn with_execution_time<T>(
        &self,
        f: impl FnOnce(u64) -> io_execution::Result<T>,
    ) -> io_execution::Result<T> {
        let mut clock = self.clock.lock().map_err(|_| io_execution::Error::Denied)?;
        f(clock())
    }
}
fn binding_fault(error: BindingError) -> Fault {
    match error {
        BindingError::Expired => Fault::Deadline,
        BindingError::Limit => Fault::Limits,
        BindingError::Denied => Fault::InactiveConnection,
        BindingError::Clock => Fault::TaskProtocol,
    }
}
/// Retain the complete managed instance so its original Control stays live;
/// dropping/closing it revokes the connection, including all late handles.
enum Session {
    Raw(PreparedPackage, Connection),
    Managed(ManagedInstance),
}
impl Session {
    fn package(&self) -> &PreparedPackage {
        match self {
            Self::Raw(p, _) => p,
            Self::Managed(i) => i.package(),
        }
    }
    fn connection(&self) -> &Connection {
        match self {
            Self::Raw(_, c) => c,
            Self::Managed(i) => i.connection(),
        }
    }
}
struct Control {
    host: HostBinding,
    authority: Option<Authority>,
    revocation: morrow_core::lifecycle::Revocation,
    id: u64,
    capacity: usize,
    limits: JobLimits,
    timeout: Duration,
    state: Mutex<State>,
}
impl Control {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn fault(&self, cancel: &Cancellation) -> Option<Fault> {
        self.job_fault(cancel, None, &[], None)
    }
    fn job_fault(
        &self,
        cancel: &Cancellation,
        service: Option<&ServiceGrant>,
        http: &[crate::http_io::HttpCallGuard],
        content: Option<&ServiceContentAccess>,
    ) -> Option<Fault> {
        cancel
            .fault()
            .or_else(|| self.revocation.is_revoked().then_some(Fault::Cancelled))
            .or_else(|| {
                self.authority.as_ref().and_then(|authority| {
                    authority
                        .with_execution_time(|now| {
                            authority.binding.check_liveness(now)?;
                            if let Some(grant) = service {
                                grant.check(now)?;
                            }
                            if let Some(content) = content {
                                content.check_at(now)?;
                            }
                            crate::http_io::HttpCallGuard::check_all_at(http, now)
                        })
                        .err()
                        .map(|error| match error {
                            io_execution::Error::Expired => Fault::Deadline,
                            io_execution::Error::Cancelled => Fault::Cancelled,
                            io_execution::Error::Clock => Fault::TaskProtocol,
                            io_execution::Error::Limit => Fault::Limits,
                            _ => Fault::InactiveConnection,
                        })
                })
            })
    }
    fn refresh(&self, slot: &mut Slot) {
        if let Some(fault) = self.job_fault(
            &slot.cancel,
            slot.service_grant.as_ref(),
            &slot.http_guards,
            slot.service_content.as_ref(),
        ) && let Some(report) = &mut slot.report
        {
            suppress(report, fault);
        } else if let Some(report) = &mut slot.report
            && let Some(validity) = &report.service_validity
            && let Err(error) = validity.check()
        {
            suppress(report, Fault::Deadline);
            report.service_persistence = Some(if error == io_execution::Error::Expired {
                ServicePersistenceStatus::Expired
            } else {
                ServicePersistenceStatus::Unavailable
            });
        }
    }
    fn stop(&self) {
        let mut state = self.lock();
        self.revocation.revoke();
        if !matches!(state.phase, Phase::Stopped | Phase::Failed) {
            state.phase = Phase::Stopping;
        }
        for slot in state.jobs.values_mut() {
            slot.cancel.cancel();
            self.refresh(slot);
        }
    }
    fn cancel(&self, serial: u64) {
        let mut state = self.lock();
        if let Some(slot) = state.jobs.get_mut(&serial) {
            slot.cancel.cancel();
            self.refresh(slot);
        }
    }
    fn abandon(&self, serial: u64) {
        let mut state = self.lock();
        if let Some(slot) = state.jobs.get_mut(&serial) {
            slot.cancel.cancel();
            if slot.report.is_some() || matches!(state.phase, Phase::Stopped | Phase::Failed) {
                state.jobs.remove(&serial);
            } else if let Some(slot) = state.jobs.get_mut(&serial) {
                slot.abandoned = true;
            }
        }
    }
    /// Cumulative byte admission is never refunded, even after cancellation.
    fn charge(
        &self,
        amount: u64,
        capabilities: &[IoCapability],
        lease: Option<&IoJobLease>,
    ) -> Result<(), Fault> {
        let mut state = self.lock();
        let next = state
            .bytes
            .checked_add(amount)
            .filter(|n| *n <= self.limits.max_total_bytes)
            .ok_or(Fault::Limits)?;
        if let (Some(lease), Some(authority)) = (lease, &self.authority) {
            authority
                .with_time(|now| lease.charge(capabilities, amount, now))
                .map_err(binding_fault)?;
        }
        state.bytes = next;
        Ok(())
    }
}
fn suppress(report: &mut JobReport, fault: Fault) {
    // Keep the first terminal delivery cause stable across later cleanup signals.
    if !report.cancelled {
        report.task.execution.outcome = Err(fault);
    }
    report.task.response = None;
    report.task.output = None;
    report.task.failure = None;
    report.response = None;
    report.http_response = None;
    report.service_response = None;
    report.service_persistence = None;
    report.cancelled = true;
    // A routed effect cannot be presumed rolled back when its delivery is suppressed.
    report.unknown |= report.calls > 0;
}
struct ServiceJob {
    request: service::Request,
    grant: ServiceGrant,
    persistence: Option<ServicePersistence>,
    content: Option<ServiceContentAccess>,
}
struct ServicePersistence {
    read_only: bool,
    journal: ServiceJournal,
    key: String,
}
struct Job {
    serial: u64,
    cancel: Cancellation,
    input: Vec<u8>,
    router: JobRouter,
    lease: Option<Arc<IoJobLease>>,
    service: Option<Box<ServiceJob>>,
}
enum Message {
    Job(Job),
    ServiceUpdate(Box<ServiceUpdate>, SyncSender<morrow_core::Result<()>>),
    Wake,
}
/// Trusted host administration, never exposed to guest imports. A missing
/// acknowledgement is not proof of rollback and must not trigger blind replay.
pub enum ServiceUpdate {
    Configuration {
        value: morrow_core::service_config::Config,
        expected_revision: u64,
    },
    Authority {
        value: morrow_core::service_authority::Record,
        expected_revision: u64,
    },
    Outbound {
        value: morrow_core::outbound_authority::Record,
        expected_revision: u64,
    },
}
pub struct ServiceUpdateHandle {
    receiver: Receiver<morrow_core::Result<()>>,
    consumed: bool,
}
impl ServiceUpdateHandle {
    pub fn read(&mut self) -> Result<Option<morrow_core::Result<()>>, JobError> {
        if self.consumed {
            return Err(JobError::Consumed);
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                self.consumed = true;
                Ok(Some(result))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                Err(JobError::Unavailable)
            }
        }
    }
}
/// Opaque handle to a controlled slot; Ready is computation, not delivery.
/// Dropping requests cancellation and never rolls back an admitted effect.
pub struct JobHandle {
    id: TaskId,
    control: Arc<Control>,
    consumed: bool,
}
impl JobHandle {
    pub fn id(&self) -> TaskId {
        self.id
    }
    pub fn cancel(&self) {
        self.control.cancel(self.id.serial);
    }
    pub fn poll(&mut self) -> Poll {
        if self.consumed {
            return Poll::Consumed;
        }
        let mut state = self.control.lock();
        let terminal = matches!(state.phase, Phase::Stopped | Phase::Failed);
        let Some(slot) = state.jobs.get_mut(&self.id.serial) else {
            self.consumed = true;
            return Poll::Unavailable;
        };
        self.control.refresh(slot);
        if slot.report.is_some() {
            return Poll::Ready;
        }
        if terminal {
            state.jobs.remove(&self.id.serial);
            self.consumed = true;
            return Poll::Unavailable;
        }
        Poll::Pending
    }
    /// Final authority check and removal share the stop/cancel lock. Revocation
    /// is observed at this delivery boundary; it cannot retract an earlier read.
    /// ReadBound retains both the report and its capacity reservation.
    pub fn read(&mut self, max_bytes: usize) -> Result<Option<JobReport>, JobError> {
        if self.consumed {
            return Err(JobError::Consumed);
        }
        let mut state = self.control.lock();
        let terminal = matches!(state.phase, Phase::Stopped | Phase::Failed);
        let Some(slot) = state.jobs.get_mut(&self.id.serial) else {
            self.consumed = true;
            return Err(JobError::Unavailable);
        };
        self.control.refresh(slot);
        match slot.report.as_ref() {
            Some(report) if report.payload_bytes() > max_bytes => Err(JobError::ReadBound),
            Some(_) => {
                self.consumed = true;
                Ok(state
                    .jobs
                    .remove(&self.id.serial)
                    .and_then(|slot| slot.report))
            }
            None if terminal => {
                state.jobs.remove(&self.id.serial);
                self.consumed = true;
                Err(JobError::Unavailable)
            }
            None => Ok(None),
        }
    }
}
impl Drop for JobHandle {
    fn drop(&mut self) {
        if !self.consumed {
            self.control.abandon(self.id.serial);
        }
    }
}
/// One bounded executor owning its package, connection and core on a dedicated
/// thread. Ordinary IO jobs deny content exchange; durable content service jobs
/// use original object grants. Routers execute outside caller transactions.
pub struct IoWorker<O: HostOwner = HostRuntime> {
    sender: SyncSender<Message>,
    control: Arc<Control>,
    join: Option<JoinHandle<WorkerExit<O>>>,
    service_authority: morrow_core::store::ServiceAuthorityControl,
}
impl IoWorker<HostRuntime> {
    /// Low-level trusted callback executor; no managed IO authorization is implied.
    pub fn spawn(
        package: PreparedPackage,
        host: HostRuntime,
        connection: Connection,
        _clock: impl FnMut() -> u64 + Send + 'static,
        capacity: usize,
        limits: JobLimits,
    ) -> Result<Self, JobError> {
        Self::spawn_session_owned(
            Session::Raw(package, connection),
            host,
            None,
            capacity,
            limits,
        )
        .map_err(|failure| failure.error)
    }
    /// Authenticate the exact managed owner before handing it to the worker.
    /// Manager remains on the calling side: approval changes and Drop revoke the
    /// original Control immediately, without waiting for a synchronous router.
    /// The clock must be a bounded, non-reentrant monotonic time source: it runs
    /// inside serialized authorization and must not call worker/handle methods.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_managed(
        manager: &Manager,
        host: HostRuntime,
        instance: ManagedInstance,
        binding: IoBinding,
        clock: impl FnMut() -> u64 + Send + 'static,
        capacity: usize,
        limits: JobLimits,
    ) -> Result<Self, JobError> {
        Self::spawn_managed_owned(manager, host, instance, binding, clock, capacity, limits)
            .map_err(|failure| failure.error)
    }

    /// Compatibility path. Use `try_reclaim` with a containing owner when its
    /// storage guards must be recovered even after worker failure.
    pub fn try_finish(&mut self) -> Result<Option<HostRuntime>, JobError> {
        match self.try_reclaim()? {
            None => Ok(None),
            Some(exit) => {
                exit.result?;
                exit.maintenance?;
                Ok(Some(exit.owner))
            }
        }
    }
}
impl<O: HostOwner> IoWorker<O> {
    /// Diagnostic state of the original budgeted run; never a reusable grant.
    pub fn service_run_snapshot(&self) -> Option<ServiceRunSnapshot> {
        self.control
            .authority
            .as_ref()
            .and_then(|authority| authority.binding.service_run_snapshot())
    }

    /// Explicit trusted-host renewal of this same budgeted run. The original
    /// package's first-issuance time horizon and cumulative ceilings still apply.
    /// This does not renew per-request deadlines or independent publication,
    /// authentication, listener and outbound-resource authorizations.
    /// Store-resolved grants recheck their live authority; manually issued grants
    /// retain the trusted host's responsibility for explicit revocation.
    pub fn renew_service_run(
        &self,
        manager: &Manager,
        grant: &ServiceGrant,
        expected_registry_revision: u64,
        expected_run_revision: u64,
        expires: u64,
        budget: ServiceRunBudget,
    ) -> Result<ServiceRunSnapshot, BindingError> {
        let authority = self
            .control
            .authority
            .as_ref()
            .ok_or(BindingError::Denied)?;
        // Reject a foreign identity before it can sample or poison our clock.
        authority
            .binding
            .validate_renewal_manager(manager, expected_registry_revision)?;
        grant.validate_binding(&authority.binding)?;
        let state = self.control.lock();
        if state.phase != Phase::Running
            || self.control.revocation.is_revoked()
            || authority.cancellation.fault().is_some()
        {
            return Err(BindingError::Denied);
        }
        // Keep the admission/stop lock until the original clock and context CAS
        // have completed. No replacement binding, instance or clock is accepted.
        authority.with_time(|now| {
            grant.check(now)?;
            authority.binding.renew_service_run(
                manager,
                expected_registry_revision,
                expected_run_revision,
                expires,
                budget,
                now,
            )
        })
    }

    /// Original run ledger snapshot; reading diagnostics does not confer authority.
    pub fn service_run_usage(&self) -> Option<ServiceRunUsage> {
        self.control
            .authority
            .as_ref()
            .and_then(|authority| authority.binding.service_run_usage())
    }

    /// Move the complete original host owner into the executor. The instance
    /// must be admitted independently; a borrowed Pool root cannot be detached.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_managed_owned(
        manager: &Manager,
        owner: O,
        instance: ManagedInstance,
        binding: IoBinding,
        mut clock: impl FnMut() -> u64 + Send + 'static,
        capacity: usize,
        limits: JobLimits,
    ) -> Result<Self, Box<SpawnFailure<O>>> {
        let admission = catch_unwind(AssertUnwindSafe(|| {
            binding
                .validate_identity(manager, owner.runtime(), &instance)
                .map_err(|_| JobError::InvalidOptions)?;
            binding
                .check(manager, owner.runtime(), &instance, clock())
                .map_err(|_| JobError::InvalidOptions)
        }))
        .unwrap_or(Err(JobError::Unavailable));
        if let Err(error) = admission {
            return Err(Box::new(SpawnFailure {
                owner,
                instance: Some(instance),
                error,
            }));
        }
        let authority = Authority {
            cancellation: instance.cancellation(),
            binding,
            clock: Arc::new(Mutex::new(Box::new(clock))),
        };
        Self::spawn_session_owned(
            Session::Managed(instance),
            owner,
            Some(authority),
            capacity,
            limits,
        )
    }
    fn spawn_session_owned(
        session: Session,
        owner: O,
        authority: Option<Authority>,
        capacity: usize,
        limits: JobLimits,
    ) -> Result<Self, Box<SpawnFailure<O>>> {
        let setup = catch_unwind(AssertUnwindSafe(|| {
            let host = owner.runtime();
            let package = session.package();
            let connection = session.connection();
            JobLimits::new(
                limits.max_calls,
                limits.max_job_bytes,
                limits.max_total_bytes,
            )?;
            let budget = package
                .package()
                .io_declaration()
                .and_then(|declaration| declaration.budget.as_ref())
                .ok_or(JobError::InvalidOptions)?;
            if capacity > budget.max_jobs as usize
                || limits.max_job_bytes > budget.max_job_bytes
                || limits.max_total_bytes > budget.max_bytes
                || limits.max_calls > package.limits().host_calls
            {
                return Err(JobError::InvalidOptions);
            }
            let timeout = Duration::from_millis(budget.max_duration_ms).min(MAX_TIMEOUT);
            if !(1..=MAX_PENDING).contains(&capacity)
                || connection.package_digest() != Some(package.package().digest())
                || host.connection_phase(connection)
                    != Ok(morrow_core::lifecycle::InstancePhase::Ready)
            {
                return Err(JobError::InvalidOptions);
            }
            let id = NEXT_EXECUTOR
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
                .map_err(|_| JobError::InvalidOptions)?;
            let revocation = host
                .revocation(connection)
                .map_err(|_| JobError::InvalidOptions)?;
            let control = Arc::new(Control {
                host: host.binding(),
                authority,
                revocation,
                id,
                capacity,
                limits,
                timeout,
                state: Mutex::new(State {
                    phase: Phase::Running,
                    next: 1,
                    jobs: BTreeMap::new(),
                    bytes: 0,
                }),
            });
            let (sender, receiver) = mpsc::sync_channel(capacity);
            let service_authority = host.store_local().service_authority_control();
            Ok((control, sender, receiver, service_authority))
        }))
        .unwrap_or(Err(JobError::Unavailable));
        let (control, sender, receiver, service_authority) = match setup {
            Ok(setup) => setup,
            Err(error) => return Err(spawn_failure(owner, session, error)),
        };
        let id = control.id;
        let inner = Arc::clone(&control);
        // A failed OS spawn drops its closure. Keep the only owner in a staged
        // slot also reachable here, so failure returns it instead of dropping it.
        let staged = Arc::new(Mutex::new(Some((owner, session))));
        let worker_owner = Arc::clone(&staged);
        let join = spawn_owner_thread(format!("morrow-io-job-{id}"), move || {
            let (mut owner, session) = worker_owner
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
                .expect("worker owner staged once");
            let result = catch_unwind(AssertUnwindSafe(|| {
                execute(&session, &mut owner, &receiver, &inner)
            }));
            let mut result = match result {
                Ok(result) => result,
                Err(_) => Err(JobError::Unavailable),
            };
            if result.is_err() {
                // Failure revocation must not wait behind potentially slow
                // owner maintenance or disconnection hooks.
                inner.stop();
            }
            // Disconnect on success, panic and preparation failure before
            // sealing. Retain the first execution error independently.
            let disconnected = catch_unwind(AssertUnwindSafe(|| {
                checked_runtime(&mut owner, inner.host)?
                    .disconnect(session.connection())
                    .map_err(|_| JobError::Disconnect)
            }))
            .unwrap_or(Err(JobError::Unavailable));
            if result.is_ok() {
                result = disconnected;
            }
            if result.is_err() {
                inner.stop();
            }
            let instance = match session {
                Session::Managed(instance) if disconnected.is_err() => Some(instance),
                _ => None,
            };
            let maintenance = catch_unwind(AssertUnwindSafe(|| {
                // Exception to unconditional maintenance: a substituted runtime
                // must never direct sealing/cleanup at an alternate Store.
                if owner.runtime().binding() != inner.host {
                    return Err(JobError::InvalidOptions);
                }
                let result = owner.finish_io();
                if owner.runtime().binding() != inner.host {
                    return Err(JobError::InvalidOptions);
                }
                result
            }))
            .unwrap_or(Err(JobError::Unavailable));
            let mut state = inner.lock();
            if result.is_err() || maintenance.is_err() {
                inner.revocation.revoke();
            }
            state.phase = if result.is_ok() && maintenance.is_ok() {
                Phase::Stopped
            } else {
                Phase::Failed
            };
            for slot in state.jobs.values_mut() {
                slot.cancel.cancel();
                inner.refresh(slot);
            }
            state.jobs.retain(|_, slot| !slot.abandoned);
            drop(state);
            WorkerExit {
                owner,
                result,
                maintenance,
                disconnect: disconnected,
                instance,
            }
        });
        let join = match join {
            Ok(join) => join,
            Err(_) => {
                let (owner, session) = staged
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("failed spawn retains staged owner");
                return Err(spawn_failure(owner, session, JobError::Spawn));
            }
        };
        Ok(Self {
            sender,
            control,
            join: Some(join),
            service_authority,
        })
    }
    /// Enqueue a bounded CAS mutation on the original Store. Acceptance revokes
    /// all resolved publications immediately, before an in-flight router can
    /// deliver. A later conflict or storage failure never revives old grants.
    /// Reopening publication requires a fresh resolution and fresh grants.
    pub fn update_service(&self, update: ServiceUpdate) -> Result<ServiceUpdateHandle, JobError> {
        let state = self.control.lock();
        if state.phase != Phase::Running {
            return Err(JobError::Closed);
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        self.sender
            .try_send(Message::ServiceUpdate(Box::new(update), sender))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => JobError::Busy,
                mpsc::TrySendError::Disconnected(_) => JobError::Closed,
            })?;
        self.service_authority.revoke_all();
        drop(state);
        Ok(ServiceUpdateHandle {
            receiver,
            consumed: false,
        })
    }
    /// Queued, running and unconsumed Ready slots share one capacity ceiling.
    /// There is no blocking send, auto-retry or replay. The complete input is
    /// charged to per-job and cumulative limits before admission.
    pub fn submit(
        &self,
        input: Vec<u8>,
        router: Box<dyn Router>,
        timeout: Duration,
    ) -> Result<JobHandle, JobError> {
        self.submit_routed(input, JobRouter::Raw(router), timeout, None)
    }
    /// Requires the managed owner; actual side effects must pass the context's
    /// durable dispatch boundary and use the original job's subcall reservation.
    pub fn submit_brokered(
        &self,
        input: Vec<u8>,
        router: Box<dyn BrokerRouter>,
        timeout: Duration,
    ) -> Result<JobHandle, JobError> {
        if self.control.authority.is_none() {
            return Err(JobError::InvalidOptions);
        }
        self.submit_routed(input, JobRouter::Brokered(router), timeout, None)
    }
    /// Validate a service route before native publication, without consuming any
    /// job/byte quota. The original grant already pins declaration and handler.
    pub fn check_service(&self, grant: &ServiceGrant) -> Result<(), JobError> {
        let authority = self
            .control
            .authority
            .as_ref()
            .ok_or(JobError::InvalidOptions)?;
        grant
            .validate_binding(&authority.binding)
            .map_err(|_| JobError::InvalidOptions)?;
        let state = self.control.lock();
        if state.phase != Phase::Running || self.control.revocation.is_revoked() {
            return Err(JobError::Closed);
        }
        authority
            .with_time(|now| {
                authority.binding.check_liveness(now)?;
                grant.check(now)
            })
            .map_err(|_| JobError::Closed)
    }
    /// Check a listener in this worker's original serialized clock domain.
    /// This neither claims the listener nor charges a job or byte reservation.
    pub fn check_listener(&self, grant: &ListenerGrant) -> Result<(), JobError> {
        let authority = self
            .control
            .authority
            .as_ref()
            .ok_or(JobError::InvalidOptions)?;
        grant
            .validate_binding(&authority.binding)
            .map_err(|_| JobError::InvalidOptions)?;
        let state = self.control.lock();
        if state.phase != Phase::Running || self.control.revocation.is_revoked() {
            return Err(JobError::Closed);
        }
        authority
            .with_time(|now| {
                authority.binding.check_liveness(now)?;
                grant.check(now)
            })
            .map_err(|_| JobError::Closed)
    }
    /// Submit a host-authenticated service invocation. This is the same bounded
    /// queue and actual managed instance; all guest IO still uses BrokerRouter.
    pub fn submit_service(
        &self,
        request: service::Request,
        grant: ServiceGrant,
        router: Box<dyn BrokerRouter>,
        timeout: Duration,
    ) -> Result<JobHandle, JobError> {
        let authority = self
            .control
            .authority
            .as_ref()
            .ok_or(JobError::InvalidOptions)?;
        grant
            .validate_job(&authority.binding, &request)
            .map_err(|_| JobError::InvalidOptions)?;
        self.submit_routed(
            request.bytes().to_vec(),
            JobRouter::Brokered(router),
            timeout,
            Some(Box::new(ServiceJob {
                request,
                grant,
                persistence: None,
                content: None,
            })),
        )
    }
    /// Same queue and live authority, with a durable single dispatch boundary.
    /// The host supplies a stable namespace and a bounded UTC clock. The key is
    /// scoped to the authenticated principal/service and is not authorization.
    #[allow(clippy::too_many_arguments)]
    pub fn submit_service_durable(
        &self,
        request: service::Request,
        grant: ServiceGrant,
        journal: ServiceJournal,
        key: &str,
        router: Box<dyn BrokerRouter>,
        timeout: Duration,
    ) -> Result<JobHandle, JobError> {
        let authority = self
            .control
            .authority
            .as_ref()
            .ok_or(JobError::InvalidOptions)?;
        grant
            .validate_job(&authority.binding, &request)
            .map_err(|_| JobError::InvalidOptions)?;
        let expected =
            morrow_core::service_record::call_id(journal.policy(), key, request.invocation())
                .map_err(|_| JobError::InvalidOptions)?;
        if expected != request.call_id() {
            return Err(JobError::InvalidOptions);
        }
        self.submit_routed(
            request.bytes().to_vec(),
            JobRouter::Brokered(router),
            timeout,
            Some(Box::new(ServiceJob {
                request,
                grant,
                content: None,
                persistence: Some(ServicePersistence {
                    read_only: false,
                    journal,
                    key: key.into(),
                }),
            })),
        )
    }
    /// Typed content access is only available on a durable service invocation.
    /// The effective content scope is part of its exact retained input.
    #[allow(clippy::too_many_arguments)]
    pub fn submit_service_content(
        &self,
        request: service::Request,
        grant: ServiceGrant,
        journal: ServiceJournal,
        key: &str,
        access: ServiceContentAccess,
        router: Box<dyn BrokerRouter>,
        timeout: Duration,
    ) -> Result<JobHandle, JobError> {
        let authority = self
            .control
            .authority
            .as_ref()
            .ok_or(JobError::InvalidOptions)?;
        grant
            .validate_job(&authority.binding, &request)
            .map_err(|_| JobError::InvalidOptions)?;
        access
            .validate_grant(&grant)
            .map_err(|_| JobError::InvalidOptions)?;
        access
            .validate_request(&request)
            .map_err(|_| JobError::InvalidOptions)?;
        let expected =
            morrow_core::service_record::call_id(journal.policy(), key, request.invocation())
                .map_err(|_| JobError::InvalidOptions)?;
        if expected != request.call_id() {
            return Err(JobError::InvalidOptions);
        }
        self.submit_routed(
            request.bytes().to_vec(),
            JobRouter::Brokered(router),
            timeout,
            Some(Box::new(ServiceJob {
                request,
                grant,
                content: Some(access),
                persistence: Some(ServicePersistence {
                    read_only: false,
                    journal,
                    key: key.into(),
                }),
            })),
        )
    }
    /// Inspect a durable request without preparing, claiming, executing or
    /// reconciling it. The caller repeats the exact original invocation; the
    /// original principal/content scope and current grants must still match.
    /// Uses the original queue, Store, instance and shared budgets. No resource
    /// router can be provided, and a missing request stays missing.
    #[allow(clippy::too_many_arguments)]
    pub fn query_service_history(
        &self,
        request: service::Request,
        grant: ServiceGrant,
        journal: ServiceJournal,
        key: &str,
        access: Option<ServiceContentAccess>,
        timeout: Duration,
    ) -> Result<JobHandle, JobError> {
        let authority = self
            .control
            .authority
            .as_ref()
            .ok_or(JobError::InvalidOptions)?;
        grant
            .validate_job(&authority.binding, &request)
            .map_err(|_| JobError::InvalidOptions)?;
        if let Some(access) = &access {
            access
                .validate_grant(&grant)
                .map_err(|_| JobError::InvalidOptions)?;
            access
                .validate_request(&request)
                .map_err(|_| JobError::InvalidOptions)?;
        }
        let expected =
            morrow_core::service_record::call_id(journal.policy(), key, request.invocation())
                .map_err(|_| JobError::InvalidOptions)?;
        if expected != request.call_id() {
            return Err(JobError::InvalidOptions);
        }
        self.submit_routed(
            request.bytes().to_vec(),
            JobRouter::ReadOnly,
            timeout,
            Some(Box::new(ServiceJob {
                request,
                grant,
                content: access,
                persistence: Some(ServicePersistence {
                    read_only: true,
                    journal,
                    key: key.into(),
                }),
            })),
        )
    }
    pub fn check_content_access(&self, access: &ServiceContentAccess) -> Result<(), JobError> {
        let authority = self
            .control
            .authority
            .as_ref()
            .ok_or(JobError::InvalidOptions)?;
        access
            .validate_binding(&authority.binding)
            .map_err(|_| JobError::InvalidOptions)?;
        authority
            .with_time(|now| {
                authority.binding.check_liveness(now)?;
                access.check_at(now)
            })
            .map_err(|_| JobError::Closed)
    }
    fn submit_routed(
        &self,
        input: Vec<u8>,
        router: JobRouter,
        timeout: Duration,
        service: Option<Box<ServiceJob>>,
    ) -> Result<JobHandle, JobError> {
        if timeout.is_zero()
            || timeout > self.control.timeout
            || input.is_empty()
            || input.len() > MAX_TASK_BYTES
        {
            return Err(JobError::InvalidOptions);
        }
        if let Some(service) = &service
            && service.content.is_none()
            && service.request.invocation().headers.iter().any(|h| {
                h.name
                    .eq_ignore_ascii_case(crate::service_content::SCOPE_HEADER)
            })
        {
            return Err(JobError::InvalidOptions);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(JobError::InvalidOptions)?;
        let input_bytes = input.len() as u64;
        let mut state = self.control.lock();
        if state.phase != Phase::Running || self.control.revocation.is_revoked() {
            return Err(JobError::Closed);
        }
        if state.jobs.len() >= self.control.capacity {
            return Err(JobError::Busy);
        }
        if input_bytes > self.control.limits.max_job_bytes
            || self
                .control
                .limits
                .max_total_bytes
                .checked_sub(state.bytes)
                .is_none_or(|remaining| input_bytes > remaining)
        {
            return Err(JobError::Limit);
        }
        let serial = state.next;
        let next = state.next.checked_add(1).ok_or(JobError::Closed)?;
        let lease = self
            .control
            .authority
            .as_ref()
            .map(|a| {
                if let Some(service) = &service {
                    service
                        .grant
                        .validate_job(&a.binding, &service.request)
                        .map_err(|_| JobError::InvalidOptions)?;
                }
                a.with_time(|now| {
                    if let Some(service) = &service {
                        service.grant.check(now)?;
                        if let Some(access) = &service.content {
                            access.check_at(now)?;
                        }
                    }
                    a.binding.admit_job_authenticated(
                        input_bytes,
                        self.control.limits.max_job_bytes,
                        now,
                    )
                })
                .map(Arc::new)
                .map_err(|e| match e {
                    BindingError::Limit => JobError::Limit,
                    _ => JobError::Closed,
                })
            })
            .transpose()?;
        state.next = next;
        state.bytes += input_bytes;
        let cancel = Cancellation::until(deadline);
        let cancel = match &self.control.authority {
            Some(authority) => Cancellation::linked(cancel, authority.cancellation.clone()),
            None => cancel,
        };
        let cancel = match &service {
            Some(service) => Cancellation::linked(cancel, service.grant.cancellation()),
            None => cancel,
        };
        state.jobs.insert(
            serial,
            Slot {
                cancel: cancel.clone(),
                report: None,
                abandoned: false,
                _lease: lease.clone(),
                http_guards: Vec::new(),
                service_grant: service.as_ref().map(|service| service.grant.clone()),
                service_content: service.as_ref().and_then(|s| s.content.clone()),
            },
        );
        if self
            .sender
            .try_send(Message::Job(Job {
                serial,
                cancel: cancel.clone(),
                input,
                router,
                lease,
                service,
            }))
            .is_err()
        {
            state.jobs.remove(&serial);
            return Err(JobError::Unavailable);
        }
        Ok(JobHandle {
            id: TaskId {
                worker: self.control.id,
                serial,
            },
            control: Arc::clone(&self.control),
            consumed: false,
        })
    }
    /// Stop admission and shorten every outstanding deadline. A queued job
    /// whose deadline passed is never started.
    pub fn drain(&self, timeout: Duration) -> Result<(), JobError> {
        if timeout.is_zero() || timeout > MAX_TIMEOUT {
            return Err(JobError::InvalidOptions);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(JobError::InvalidOptions)?;
        let mut state = self.control.lock();
        if !matches!(state.phase, Phase::Running | Phase::Draining) {
            return Err(JobError::Closed);
        }
        state.phase = Phase::Draining;
        for slot in state.jobs.values_mut() {
            slot.cancel.limit_deadline(deadline);
            self.control.refresh(slot);
        }
        drop(state);
        let _ = self.sender.try_send(Message::Wake);
        Ok(())
    }
    /// Revoke first, then cancel every job. Late results are never delivered as
    /// successes by `read`.
    pub fn stop(&self) {
        self.control.stop();
        let _ = self.sender.try_send(Message::Wake);
    }
    pub fn phase(&self) -> Phase {
        self.control.lock().phase
    }
    pub fn pending(&self) -> usize {
        self.control.lock().jobs.len()
    }
    /// Cumulative admitted bytes; releasing a job never refunds them.
    pub fn bytes(&self) -> u64 {
        self.control.lock().bytes
    }
    /// Returns exclusive core ownership only after the thread ended.
    pub fn try_reclaim(&mut self) -> Result<Option<WorkerExit<O>>, JobError> {
        let join = self.join.as_ref().ok_or(JobError::Consumed)?;
        if !join.is_finished() {
            return Ok(None);
        }
        self.join
            .take()
            .unwrap()
            .join()
            .map_err(|_| JobError::Unavailable)
            .map(Some)
    }
}
impl<O: HostOwner> Drop for IoWorker<O> {
    fn drop(&mut self) {
        self.stop();
    }
}
fn spawn_failure<O: HostOwner>(
    owner: O,
    session: Session,
    error: JobError,
) -> Box<SpawnFailure<O>> {
    let instance = match session {
        Session::Managed(instance) => Some(instance),
        Session::Raw(..) => None,
    };
    Box::new(SpawnFailure {
        owner,
        instance,
        error,
    })
}
#[cfg(test)]
thread_local! {
    // Thread-local injection cannot interfere with concurrent tests or workers.
    static FAIL_OWNER_SPAWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
fn spawn_owner_thread<T: Send + 'static>(
    name: String,
    run: impl FnOnce() -> T + Send + 'static,
) -> std::io::Result<JoinHandle<T>> {
    #[cfg(test)]
    if FAIL_OWNER_SPAWN.replace(false) {
        return Err(std::io::Error::other("injected worker spawn failure"));
    }
    thread::Builder::new().name(name).spawn(run)
}
fn checked_runtime<O: HostOwner>(
    owner: &mut O,
    original: HostBinding,
) -> Result<&mut HostRuntime, JobError> {
    let runtime = owner.runtime_mut();
    if runtime.binding() != original {
        return Err(JobError::InvalidOptions);
    }
    Ok(runtime)
}

fn execute<O: HostOwner>(
    session: &Session,
    owner: &mut O,
    receiver: &Receiver<Message>,
    control: &Arc<Control>,
) -> Result<(), JobError> {
    let package = session.package();
    let broker = Broker::new();
    loop {
        {
            let mut state = control.lock();
            if control.fault(&Cancellation::default()).is_some()
                && matches!(state.phase, Phase::Running | Phase::Draining)
            {
                state.phase = Phase::Stopping;
            }
            for slot in state.jobs.values_mut() {
                control.refresh(slot);
            }
            let idle = state.jobs.values().all(|slot| slot.report.is_some());
            let finished = match state.phase {
                Phase::Running => false,
                // Gentle draining preserves a valid Ready result until consumed,
                // dropped or expired; then disconnection cannot discard success.
                Phase::Draining => {
                    idle && state.jobs.values().all(|slot| {
                        slot.report.as_ref().is_some_and(|report| report.cancelled)
                            || control.fault(&slot.cancel).is_some()
                    })
                }
                Phase::Stopping | Phase::Stopped | Phase::Failed => idle,
            };
            if finished {
                break;
            }
        }
        let message = match receiver.recv_timeout(Duration::from_millis(10)) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let job = match message {
            Message::Job(job) => job,
            Message::Wake => continue,
            Message::ServiceUpdate(update, reply) => {
                if let Err(error) = owner.prepare_io() {
                    // No Store mutation was attempted; a missing acknowledgement
                    // remains unavailable and must not prompt automatic replay.
                    drop(reply);
                    return Err(error);
                }
                let host = checked_runtime(owner, control.host)?;
                let result = match *update {
                    ServiceUpdate::Configuration {
                        value,
                        expected_revision,
                    } => host
                        .store_local_mut()
                        .save_service_config_local(&value, expected_revision),
                    ServiceUpdate::Authority {
                        value,
                        expected_revision,
                    } => host
                        .store_local_mut()
                        .save_service_authority_local(&value, expected_revision),
                    ServiceUpdate::Outbound {
                        value,
                        expected_revision,
                    } => host
                        .store_local_mut()
                        .save_outbound_authority_local(&value, expected_revision),
                };
                let _ = reply.try_send(result);
                continue;
            }
        };
        let Job {
            serial,
            cancel,
            input,
            mut router,
            lease,
            service,
        } = job;
        let mut http_guards = Vec::new();
        let mut report = match control.job_fault(
            &cancel,
            service.as_ref().map(|s| &s.grant),
            &[],
            service.as_ref().and_then(|s| s.content.as_ref()),
        ) {
            Some(fault) => {
                let mut report = cancelled_report(package, fault);
                report.bytes = input.len() as u64;
                report
            }
            None => {
                if !matches!(router, JobRouter::ReadOnly)
                    && !service.as_ref().is_some_and(|service| {
                        service.persistence.as_ref().is_some_and(|p| p.read_only)
                    })
                {
                    owner.prepare_io()?;
                }
                let host = checked_runtime(owner, control.host)?;
                match prepare_service_history(
                    package,
                    host,
                    control,
                    &cancel,
                    lease.as_ref(),
                    service.as_deref(),
                    input.len(),
                ) {
                    Err(report) => *report,
                    Ok(history) => run_job(
                        package,
                        &input,
                        &cancel,
                        control,
                        &mut router,
                        lease.as_ref(),
                        host,
                        match session {
                            Session::Managed(i) => Some(i),
                            Session::Raw(..) => None,
                        },
                        &broker,
                        &mut http_guards,
                        service.as_deref(),
                        history.as_ref(),
                    ),
                }
            }
        };
        if let Some(fault) = control.job_fault(
            &cancel,
            service.as_ref().map(|s| &s.grant),
            &http_guards,
            service.as_ref().and_then(|s| s.content.as_ref()),
        ) {
            suppress(&mut report, fault);
        }
        // Computation is over. Only the controlled slot owns the job lease when
        // Ready becomes visible, so successful read/drop immediately frees it.
        drop(lease);
        let mut state = control.lock();
        if let Some(slot) = state.jobs.get_mut(&serial) {
            if slot.abandoned {
                state.jobs.remove(&serial);
            } else {
                slot.http_guards = http_guards;
                slot.report = Some(report);
                control.refresh(slot);
            }
        }
    }
    Ok(())
}
fn cancelled_report(package: &PreparedPackage, fault: Fault) -> JobReport {
    JobReport {
        task: TaskReport {
            execution: Report {
                outcome: Err(fault),
                host_calls: 0,
                fuel_remaining: package.limits().fuel,
            },
            response: None,
            output: None,
            failure: None,
        },
        response: None,
        http_response: None,
        service_response: None,
        service_persistence: None,
        service_validity: None,
        calls: 0,
        bytes: 0,
        cancelled: true,
        unknown: false,
    }
}
// Returns a claimed historical boundary, or a complete non-executing report.
#[allow(clippy::too_many_arguments)]
fn prepare_service_history(
    package: &PreparedPackage,
    host: &mut HostRuntime,
    control: &Arc<Control>,
    cancel: &Cancellation,
    lease: Option<&Arc<IoJobLease>>,
    service: Option<&ServiceJob>,
    input_len: usize,
) -> Result<Option<(Record, service_history::Validity)>, Box<JobReport>> {
    let Some(service) = service else {
        return Ok(None);
    };
    let Some(persistence) = &service.persistence else {
        return Ok(None);
    };
    if persistence.read_only {
        return Err(Box::new(query_service_report(
            package,
            host,
            control,
            cancel,
            lease,
            service,
            persistence,
            input_len,
        )));
    }
    let begin = service_history::begin(
        host.store_local_mut(),
        &persistence.journal,
        &persistence.key,
        &service.request,
        package.package().digest(),
        || {
            if control
                .job_fault(cancel, Some(&service.grant), &[], service.content.as_ref())
                .is_some()
            {
                Err(morrow_core::Error::Invalid("service authority"))
            } else {
                Ok(())
            }
        },
    );
    let mut report = cancelled_report(package, Fault::TaskProtocol);
    report.cancelled = false;
    report.bytes = input_len as u64;
    match begin {
        Ok(service_history::Begin::Execute { record, validity }) => {
            if validity.arm(cancel).is_ok() {
                return Ok(Some((record, validity)));
            }
            report.unknown = true;
            report.service_validity = Some(validity);
            report.service_persistence = Some(ServicePersistenceStatus::Unknown);
        }
        Ok(service_history::Begin::Replay {
            reply,
            completion,
            validity,
        }) => {
            report.service_validity = Some(validity);
            let charge = completion.len() as u64;
            if report
                .bytes
                .checked_add(charge)
                .is_none_or(|n| n > control.limits.max_job_bytes)
                || control.charge(charge, &[], lease.map(Arc::as_ref)).is_err()
            {
                report.task.execution.outcome = Err(Fault::Limits);
                report.service_persistence = Some(ServicePersistenceStatus::Unavailable);
            } else {
                report.bytes += charge;
                report.task.execution.outcome = Ok(0);
                report.service_response = Some(reply);
                report.service_persistence = Some(ServicePersistenceStatus::Replayed);
            }
        }
        Ok(service_history::Begin::Unknown) => {
            report.unknown = true;
            report.service_persistence = Some(ServicePersistenceStatus::Unknown);
        }
        Ok(service_history::Begin::Expired) => {
            report.service_persistence = Some(ServicePersistenceStatus::Expired)
        }
        Err(io_execution::Error::Conflict) => {
            report.service_persistence = Some(ServicePersistenceStatus::Conflict)
        }
        Err(error) => {
            report.unknown = matches!(
                error,
                io_execution::Error::CommitUnknown | io_execution::Error::OutcomeUnknown
            );
            report.service_persistence = Some(ServicePersistenceStatus::Unavailable);
        }
    }
    Err(Box::new(report))
}
// This branch always returns a terminal report to the worker loop; it never
// returns a dispatch claim. Read-only status delivery shares the original guards.
#[allow(clippy::too_many_arguments)]
fn query_service_report(
    package: &PreparedPackage,
    host: &HostRuntime,
    control: &Arc<Control>,
    cancel: &Cancellation,
    lease: Option<&Arc<IoJobLease>>,
    service: &ServiceJob,
    persistence: &ServicePersistence,
    input_len: usize,
) -> JobReport {
    let query = service_history::query(
        host.store_local(),
        &persistence.journal,
        &persistence.key,
        &service.request,
        package.package().digest(),
        || {
            if control
                .job_fault(cancel, Some(&service.grant), &[], service.content.as_ref())
                .is_some()
            {
                Err(morrow_core::Error::Invalid("service query authority"))
            } else {
                Ok(())
            }
        },
    );
    let mut report = cancelled_report(package, Fault::TaskProtocol);
    report.cancelled = false;
    report.bytes = input_len as u64;
    report.task.execution.outcome = Ok(0);
    report.service_persistence = Some(match query {
        Ok(service_history::Query::Missing) => ServicePersistenceStatus::Missing,
        Ok(service_history::Query::Prepared { validity }) => {
            report.service_validity = Some(validity);
            ServicePersistenceStatus::Prepared
        }
        Ok(service_history::Query::Unknown { validity }) => {
            report.unknown = true;
            report.service_validity = Some(validity);
            ServicePersistenceStatus::Unknown
        }
        Ok(service_history::Query::Cancelled { validity }) => {
            report.service_validity = Some(validity);
            ServicePersistenceStatus::Cancelled
        }
        Ok(service_history::Query::Expired) => ServicePersistenceStatus::Expired,
        Ok(service_history::Query::Observed {
            reply,
            completion,
            validity,
        }) => {
            report.service_validity = Some(validity);
            let charge = completion.len() as u64;
            if report
                .bytes
                .checked_add(charge)
                .is_none_or(|n| n > control.limits.max_job_bytes)
                || control.charge(charge, &[], lease.map(Arc::as_ref)).is_err()
            {
                report.task.execution.outcome = Err(Fault::Limits);
                ServicePersistenceStatus::Unavailable
            } else {
                report.bytes += charge;
                report.service_response = Some(reply);
                ServicePersistenceStatus::Observed
            }
        }
        Err(io_execution::Error::Conflict) => ServicePersistenceStatus::Conflict,
        Err(_) => {
            report.task.execution.outcome = Err(Fault::TaskProtocol);
            ServicePersistenceStatus::Unavailable
        }
    });
    report
}
fn service_job_fault(
    control: &Control,
    cancel: &Cancellation,
    service: Option<&ServiceJob>,
    http: &[crate::http_io::HttpCallGuard],
    validity: Option<&service_history::Validity>,
) -> Option<Fault> {
    control
        .job_fault(
            cancel,
            service.map(|s| &s.grant),
            http,
            service.and_then(|s| s.content.as_ref()),
        )
        .or_else(|| {
            validity
                .and_then(|v| v.check().err())
                .map(|error| match error {
                    io_execution::Error::Expired => Fault::Deadline,
                    _ => Fault::TaskProtocol,
                })
        })
}
#[allow(clippy::too_many_arguments)]
fn dispatch_service_content(
    host: &mut HostRuntime,
    instance: Option<&ManagedInstance>,
    control: &Control,
    cancel: &Cancellation,
    service: Option<&ServiceJob>,
    validity: Option<&service_history::Validity>,
    request: &[u8],
) -> Result<Vec<u8>, RouterFault> {
    let instance = instance.ok_or(RouterFault::Denied)?;
    let service = service.ok_or(RouterFault::Denied)?;
    let access = service.content.as_ref().ok_or(RouterFault::Denied)?;
    let authority = control.authority.as_ref().ok_or(RouterFault::Denied)?;
    let mut clock = authority.clock.lock().map_err(|_| RouterFault::Denied)?;
    // Hold the original clock domain through dispatch. The guard must not sample
    // that clock again; all original Store boundaries supply their sampled time.
    host.dispatch_guarded(
        instance.connection(),
        request,
        &mut **clock,
        |command, now| {
            if cancel.fault().is_some() || control.revocation.is_revoked() {
                return Err(morrow_core::Error::Invalid("cancelled service content"));
            }
            authority
                .binding
                .check_liveness(now)
                .map_err(|_| morrow_core::Error::Invalid("inactive instance"))?;
            service
                .grant
                .check(now)
                .map_err(|_| morrow_core::Error::Invalid("inactive service"))?;
            if let Some(validity) = validity {
                validity
                    .check()
                    .map_err(|_| morrow_core::Error::Invalid("expired service request"))?;
            }
            access
                .check_command(command, now)
                .map_err(|_| morrow_core::Error::Invalid("denied service content"))
        },
    )
    .map_err(|_| RouterFault::Unknown)
}
#[allow(clippy::too_many_arguments)]
fn run_job(
    package: &PreparedPackage,
    input: &[u8],
    cancel: &Cancellation,
    control: &Arc<Control>,
    router: &mut JobRouter,
    lease: Option<&Arc<IoJobLease>>,
    host: &mut HostRuntime,
    instance: Option<&ManagedInstance>,
    broker: &Broker,
    http_guards: &mut Vec<crate::http_io::HttpCallGuard>,
    service: Option<&ServiceJob>,
    history: Option<&(Record, service_history::Validity)>,
) -> JobReport {
    let mut calls = 0u32;
    let mut bytes = input.len() as u64;
    let mut fault: Option<Fault> = None;
    let mut unknown = false;
    let mut last_request: Option<Vec<u8>> = None;
    let mut last_response: Option<Vec<u8>> = None;
    if let Some(fault) = service_job_fault(
        control,
        cancel,
        service,
        http_guards,
        history.map(|(_, validity)| validity),
    ) {
        let mut report = cancelled_report(package, fault);
        report.bytes = bytes;
        report.unknown = history.is_some();
        report.service_validity = history.map(|(_, validity)| validity.clone());
        return report;
    }
    let run = package.run_service_frame(
        input,
        &mut |content_call, request| {
            if fault.is_some() {
                return Err(());
            }
            if let Some(late) = service_job_fault(
                control,
                cancel,
                service,
                http_guards,
                history.map(|(_, validity)| validity),
            ) {
                fault = Some(late);
                return Err(());
            }
            if calls >= control.limits.max_calls {
                fault = Some(Fault::Limits);
                return Err(());
            }
            if content_call && service.and_then(|s| s.content.as_ref()).is_none() {
                fault = Some(Fault::TaskProtocol);
                return Err(());
            }
            let capabilities: &[IoCapability] = if lease.is_some() && !content_call {
                match Request::decode(request).map(|r| r.action().clone()) {
                    Ok(Action::Read { .. } | Action::Finish { .. } | Action::Cancel { .. }) => {
                        &[IoCapability::FileRead]
                    }
                    Ok(Action::SubmitHttp(http)) if !http.credential.is_empty() => {
                        &[IoCapability::HttpRequest, IoCapability::CredentialUse]
                    }
                    Ok(Action::SubmitHttp(_)) => &[IoCapability::HttpRequest],
                    _ => {
                        fault = Some(Fault::TaskProtocol);
                        return Err(());
                    }
                }
            } else {
                &[]
            };
            // The request is charged before the router may produce any external
            // effect, so a byte cap is never discovered after the call.
            let request_charge = request.len() as u64;
            if bytes
                .checked_add(request_charge)
                .is_none_or(|total| total > control.limits.max_job_bytes)
            {
                fault = Some(Fault::Limits);
                return Err(());
            }
            if let Err(error) = control.charge(request_charge, capabilities, lease.map(Arc::as_ref))
            {
                fault = Some(error);
                return Err(());
            }
            bytes += request_charge;
            // Recheck just before dispatch. Once a synchronous trusted router
            // starts, a later cancellation cannot claim its effects did not occur.
            if let Some(late) = service_job_fault(
                control,
                cancel,
                service,
                http_guards,
                history.map(|(_, validity)| validity),
            ) {
                fault = Some(late);
                return Err(());
            }
            let call = calls;
            calls += 1;
            let mut reserved = 0;
            let routed = if content_call {
                let response_bound = morrow_core::runtime::MAX_MESSAGE_BYTES as u64;
                if bytes
                    .checked_add(response_bound)
                    .is_none_or(|n| n > control.limits.max_job_bytes)
                {
                    fault = Some(Fault::Limits);
                    return Err(());
                }
                if let Err(error) = control.charge(response_bound, &[], lease.map(Arc::as_ref)) {
                    fault = Some(error);
                    return Err(());
                }
                reserved = response_bound;
                // Reserve the full reply bound before a content transaction can commit.
                // A fixed upper bound avoids discovering response quota after a write.
                dispatch_service_content(
                    host,
                    instance,
                    control,
                    cancel,
                    service,
                    history.map(|(_, validity)| validity),
                    request,
                )
            } else {
                match router {
                    JobRouter::ReadOnly => Err(RouterFault::Denied),
                    JobRouter::Raw(router) => router.route(call, request),
                    JobRouter::Brokered(router) => {
                        match (instance, lease, Request::decode(request)) {
                            (Some(instance), Some(lease), Ok(parsed)) => {
                                let mut context = RouteContext {
                                    host,
                                    instance,
                                    broker,
                                    control,
                                    lease,
                                    cancel,
                                    request: &parsed,
                                    reserved: &mut reserved,
                                    used: false,
                                    expected: None,
                                    uncertain: false,
                                    http_guard: None,
                                    service_validity: history.map(|(_, validity)| validity),
                                    service_content: service.and_then(|s| s.content.as_ref()),
                                };
                                let reply = router.route(&mut context, call, &parsed);
                                if let Some(guard) = context.http_guard.take() {
                                    // At most one guard per import; imports are bounded by
                                    // max_calls. Keep successful authorization through Ready.
                                    http_guards.push(guard);
                                }
                                if context.uncertain
                                    || (context.expected.is_some() && reply.is_err())
                                {
                                    Err(RouterFault::Unknown)
                                } else if reply
                                    .as_ref()
                                    .is_ok_and(|v| context.expected.as_ref() != Some(v))
                                {
                                    Err(if context.used {
                                        RouterFault::Unknown
                                    } else {
                                        RouterFault::Denied
                                    })
                                } else {
                                    reply
                                }
                            }
                            _ => Err(RouterFault::Denied),
                        }
                    }
                }
            };
            bytes += reserved;
            if let Some(late) = service_job_fault(
                control,
                cancel,
                service,
                http_guards,
                history.map(|(_, validity)| validity),
            ) {
                fault = Some(late);
                unknown = true;
                return Err(());
            }
            let response = match routed {
                Ok(response) => response,
                Err(RouterFault::Denied) => {
                    fault = Some(Fault::InactiveConnection);
                    return Err(());
                }
                Err(RouterFault::Limit) => {
                    fault = Some(Fault::Limits);
                    return Err(());
                }
                Err(RouterFault::Unknown) => {
                    fault = Some(Fault::TaskProtocol);
                    unknown = true;
                    return Err(());
                }
            };
            let response_charge = if reserved == 0 {
                response.len() as u64
            } else {
                0
            };
            if (reserved != 0 && response.len() as u64 > reserved)
                || response.is_empty()
                || response.len() > MAX_FRAME_BYTES
                || bytes
                    .checked_add(response_charge)
                    .is_none_or(|total| total > control.limits.max_job_bytes)
            {
                // The call already reached the router; the result cannot be
                // delivered within quota, so the outcome needs reconciliation.
                fault = Some(Fault::Limits);
                unknown = true;
                return Err(());
            }
            if let Err(error) = control.charge(response_charge, &[], lease.map(Arc::as_ref)) {
                fault = Some(error);
                unknown = true;
                return Err(());
            }
            bytes += response_charge;
            last_request = Some(request.to_vec());
            last_response = Some(response.clone());
            Ok(response)
        },
        cancel.clone(),
    );
    if let Some(late) = service_job_fault(
        control,
        cancel,
        service,
        http_guards,
        history.map(|(_, validity)| validity),
    ) {
        // A result that outlived its deadline or revocation is not a success.
        let mut report = cancelled_report(package, late);
        report.task.execution.host_calls = run.report.host_calls;
        report.task.execution.fuel_remaining = run.report.fuel_remaining;
        report.calls = calls;
        report.bytes = bytes;
        report.unknown = unknown || calls > 0 || history.is_some();
        report.service_validity = history.map(|(_, validity)| validity.clone());
        return report;
    }
    let mut execution = run.report;
    if let Some(fault) = fault {
        execution.outcome = Err(fault);
    }
    let mut response = None;
    let mut http_response = None;
    let mut service_response = None;
    let mut service_persistence = history.map(|_| ServicePersistenceStatus::Unknown);
    if let Some(service) = service {
        if execution.outcome == Ok(0) {
            match run.completion {
                Some(done) => {
                    let charged = bytes
                        .checked_add(done.len() as u64)
                        .filter(|next| *next <= control.limits.max_job_bytes)
                        .ok_or(Fault::Limits)
                        .and_then(|next| {
                            control.charge(done.len() as u64, &[], lease.map(Arc::as_ref))?;
                            bytes = next;
                            Ok(())
                        });
                    match charged {
                        Err(fault) => execution.outcome = Err(fault),
                        Ok(()) => match service::Response::decode(&service.request, &done) {
                            Ok(reply) => {
                                if let Some(history) = history {
                                    match service_history::finish(
                                        host.store_local_mut(),
                                        &history.0,
                                        &done,
                                    ) {
                                        Ok(()) => {
                                            service_persistence =
                                                Some(ServicePersistenceStatus::Completed);
                                            service_response = Some(reply);
                                        }
                                        Err(_) => {
                                            execution.outcome = Err(Fault::TaskProtocol);
                                            unknown = true;
                                        }
                                    }
                                } else {
                                    service_response = Some(reply);
                                }
                            }
                            Err(_) => execution.outcome = Err(Fault::TaskProtocol),
                        },
                    }
                }
                None => execution.outcome = Err(Fault::TaskProtocol),
            }
        }
        // Past an admitted side effect, an invalid/oversized final reply never
        // proves rollback and must not authorize an automatic replay.
        if execution.outcome != Ok(0) {
            unknown |= calls > 0 || history.is_some();
            service_response = None;
        }
    } else if execution.outcome.is_ok() {
        match (run.completion, last_response, last_request) {
            (Some(done), Some(actual), Some(request)) if calls > 0 && done == actual => {
                let decoded = Request::decode(&request).and_then(|request| {
                    if matches!(request.action(), Action::SubmitHttp(_)) {
                        http_response = Some(Response::decode_http(&request, &actual)?);
                    } else {
                        response = Some(Response::decode(&request, &actual)?);
                    }
                    Ok(())
                });
                if decoded.is_err() {
                    execution.outcome = Err(Fault::TaskProtocol);
                }
            }
            _ => execution.outcome = Err(Fault::TaskProtocol),
        }
    }
    JobReport {
        task: TaskReport {
            execution,
            response: None,
            output: None,
            failure: None,
        },
        response,
        http_response,
        service_response,
        service_persistence,
        service_validity: history.map(|(_, validity)| validity.clone()),
        calls,
        bytes,
        cancelled: false,
        unknown,
    }
}

#[cfg(test)]
mod owner_tests {
    use super::*;
    use morrow_core::{
        plugin_package::{Package, io},
        store::Store,
    };

    struct Owner {
        runtime: HostRuntime,
        token: Arc<()>,
        finished: bool,
    }
    impl HostOwner for Owner {
        fn runtime(&self) -> &HostRuntime {
            &self.runtime
        }
        fn runtime_mut(&mut self) -> &mut HostRuntime {
            &mut self.runtime
        }
        fn prepare_io(&mut self) -> Result<(), JobError> {
            panic!("readonly must not trigger write preparation")
        }
        fn finish_io(&mut self) -> Result<(), JobError> {
            self.finished = true;
            Ok(())
        }
    }
    fn fixture() -> (tempfile::TempDir, Owner, Session) {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(
            r#"(module
            (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
            (memory (export "memory") 1) (data (i32.const 0) "ok")
            (func (export "morrow_run") (result i32)
                i32.const 0 i32.const 2 call $done drop i32.const 0))"#,
        )
        .unwrap();
        let mut manifest =
            Package::manifest_for_task("org.example.owner.unit", "1.0.0", &wasm, vec![]);
        manifest.required_features.push(io::FEATURE.into());
        manifest.io_declaration = Some(io::declaration(
            vec![IoCapability::FileRead],
            vec!["io.invoke".into()],
        ));
        let package = PreparedPackage::new(
            Package::build(manifest, &wasm).unwrap(),
            crate::Limits::default(),
        )
        .unwrap();
        let mut runtime =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        let connection = package.connect(&mut runtime).unwrap();
        (
            dir,
            Owner {
                runtime,
                token: Arc::new(()),
                finished: false,
            },
            Session::Raw(package, connection),
        )
    }
    #[test]
    fn failed_thread_spawn_returns_staged_original_owner() {
        let (_dir, owner, session) = fixture();
        let token = owner.token.clone();
        let binding = owner.runtime.binding();
        FAIL_OWNER_SPAWN.set(true);
        let failure =
            match IoWorker::spawn_session_owned(session, owner, None, 1, JobLimits::default()) {
                Err(failure) => failure,
                Ok(_) => panic!("injected spawn unexpectedly succeeded"),
            };
        assert_eq!(failure.error, JobError::Spawn);
        assert!(Arc::ptr_eq(&failure.owner.token, &token));
        assert_eq!(failure.owner.runtime.binding(), binding);
        assert!(!failure.owner.finished);
    }
    #[test]
    fn readonly_malformed_input_does_not_invoke_owner_write_preparation() {
        let (_dir, owner, session) = fixture();
        let usage = owner.runtime.store_local().pending_usage().unwrap();
        let mut worker =
            IoWorker::spawn_session_owned(session, owner, None, 1, JobLimits::default()).unwrap();
        let mut job = worker
            .submit_routed(vec![1], JobRouter::ReadOnly, Duration::from_secs(10), None)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match job.read(4096) {
                Ok(Some(report)) => {
                    assert_eq!(report.task.execution.outcome, Err(Fault::TaskProtocol));
                    assert_eq!(report.calls, 0);
                    break;
                }
                Ok(None) => {
                    assert!(Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("readonly result {error:?}"),
            }
        }
        worker.stop();
        let exit = loop {
            if let Some(exit) = worker.try_reclaim().unwrap() {
                break exit;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(exit.result, Ok(()));
        assert_eq!(exit.maintenance, Ok(()));
        assert!(exit.owner.finished);
        assert_eq!(
            exit.owner.runtime.store_local().pending_usage().unwrap(),
            usage
        );
    }
}
