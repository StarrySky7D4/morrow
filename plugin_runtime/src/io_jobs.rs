//! Bounded IO job executor: submit, poll, read and cancel on one native thread.
//! Each job carries a trusted-host contract router for its IO calls; this module
//! performs no external network/file operations itself, never retries a call, and never delivers a
//! result that outlived its authorization or deadline as a success.
use crate::{
    Cancellation, Fault, MAX_TASK_BYTES, Report,
    io_binding::{Error as BindingError, IoBinding, IoJobLease},
    io_execution::{self, Broker},
    manager::{ManagedInstance, Manager},
    package::{PreparedPackage, TaskReport},
    service_io::{ListenerGrant, ServiceGrant},
    worker::TaskId,
};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
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
        let guard = grant.authorize(
            self.host,
            self.instance,
            self.lease,
            self.request,
            self.cancel.clone(),
            Arc::clone(&authority.clock),
        )?;
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
            authority
                .with_execution_time(|now| child.check(now).map_err(Error::from))
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
                let response = backend(raw)?;
                Response::decode_http(self.request, &response).map_err(|_| ())?;
                Ok(response)
            },
            |live| authority.with_execution_time(|now| live.check_liveness(now)),
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
pub struct JobReport {
    /// Execution facts only; the content exchange is always denied for IO jobs.
    pub task: TaskReport,
    /// The last brokered IO response, decoded against its own request.
    pub response: Option<Response>,
    /// HTTP result, validated against its exact submission frame.
    pub http_response: Option<HttpOutcome>,
    /// Service completion bound to its original request, independent of IO responses.
    pub service_response: Option<service::Reply>,
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
            .field("calls", &self.calls)
            .field("bytes", &self.bytes)
            .field("cancelled", &self.cancelled)
            .field("unknown", &self.unknown)
            .finish()
    }
}
impl JobReport {
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
        self.job_fault(cancel, None, &[])
    }
    fn job_fault(
        &self,
        cancel: &Cancellation,
        service: Option<&ServiceGrant>,
        http: &[crate::http_io::HttpCallGuard],
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
        if let Some(fault) =
            self.job_fault(&slot.cancel, slot.service_grant.as_ref(), &slot.http_guards)
            && let Some(report) = &mut slot.report
        {
            suppress(report, fault);
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
    report.cancelled = true;
    // A routed effect cannot be presumed rolled back when its delivery is suppressed.
    report.unknown |= report.calls > 0;
}
struct ServiceJob {
    request: service::Request,
    grant: ServiceGrant,
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
    Wake,
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
/// thread. IO jobs deny content exchange; routers execute outside caller transactions.
pub struct IoWorker {
    sender: SyncSender<Message>,
    control: Arc<Control>,
    join: Option<JoinHandle<Result<HostRuntime, JobError>>>,
}
impl IoWorker {
    /// Low-level trusted callback executor; no managed IO authorization is implied.
    pub fn spawn(
        package: PreparedPackage,
        host: HostRuntime,
        connection: Connection,
        _clock: impl FnMut() -> u64 + Send + 'static,
        capacity: usize,
        limits: JobLimits,
    ) -> Result<Self, JobError> {
        Self::spawn_session(
            Session::Raw(package, connection),
            host,
            None,
            capacity,
            limits,
        )
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
        mut clock: impl FnMut() -> u64 + Send + 'static,
        capacity: usize,
        limits: JobLimits,
    ) -> Result<Self, JobError> {
        binding
            .validate_identity(manager, &host, &instance)
            .map_err(|_| JobError::InvalidOptions)?;
        binding
            .check(manager, &host, &instance, clock())
            .map_err(|_| JobError::InvalidOptions)?;
        let authority = Authority {
            cancellation: instance.cancellation(),
            binding,
            clock: Arc::new(Mutex::new(Box::new(clock))),
        };
        Self::spawn_session(
            Session::Managed(instance),
            host,
            Some(authority),
            capacity,
            limits,
        )
    }
    fn spawn_session(
        session: Session,
        mut host: HostRuntime,
        authority: Option<Authority>,
        capacity: usize,
        limits: JobLimits,
    ) -> Result<Self, JobError> {
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
            || host.connection_phase(connection) != Ok(morrow_core::lifecycle::InstancePhase::Ready)
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
        let inner = Arc::clone(&control);
        let join = thread::Builder::new()
            .name(format!("morrow-io-job-{id}"))
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    execute(&session, &mut host, &receiver, &inner)
                }));
                let result = match result {
                    Ok(result) => result,
                    Err(_) => Err(JobError::Unavailable),
                };
                let mut state = inner.lock();
                if result.is_err() {
                    inner.revocation.revoke();
                }
                state.phase = if result.is_ok() {
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
                result.map(|()| host)
            })
            .map_err(|_| JobError::Spawn)?;
        Ok(Self {
            sender,
            control,
            join: Some(join),
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
            Some(Box::new(ServiceJob { request, grant })),
        )
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
    pub fn try_finish(&mut self) -> Result<Option<HostRuntime>, JobError> {
        let join = self.join.as_ref().ok_or(JobError::Consumed)?;
        if !join.is_finished() {
            return Ok(None);
        }
        self.join
            .take()
            .unwrap()
            .join()
            .map_err(|_| JobError::Unavailable)?
            .map(Some)
    }
}
impl Drop for IoWorker {
    fn drop(&mut self) {
        self.stop();
    }
}
fn execute(
    session: &Session,
    host: &mut HostRuntime,
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
        let Message::Job(job) = message else {
            continue;
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
        let mut report = match control.job_fault(&cancel, service.as_ref().map(|s| &s.grant), &[]) {
            Some(fault) => {
                let mut report = cancelled_report(package, fault);
                report.bytes = input.len() as u64;
                report
            }
            None => run_job(
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
            ),
        };
        if let Some(fault) =
            control.job_fault(&cancel, service.as_ref().map(|s| &s.grant), &http_guards)
        {
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
    host.disconnect(session.connection())
        .map_err(|_| JobError::Disconnect)
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
        calls: 0,
        bytes: 0,
        cancelled: true,
        unknown: false,
    }
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
) -> JobReport {
    let mut calls = 0u32;
    let mut bytes = input.len() as u64;
    let mut fault: Option<Fault> = None;
    let mut unknown = false;
    let mut last_request: Option<Vec<u8>> = None;
    let mut last_response: Option<Vec<u8>> = None;
    let run = package.run_io_frame(
        input,
        &mut |request| {
            if fault.is_some() {
                return Err(());
            }
            if let Some(late) = control.job_fault(cancel, service.map(|s| &s.grant), http_guards) {
                fault = Some(late);
                return Err(());
            }
            if calls >= control.limits.max_calls {
                fault = Some(Fault::Limits);
                return Err(());
            }
            let capabilities: &[IoCapability] = if lease.is_some() {
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
            if let Some(late) = control.job_fault(cancel, service.map(|s| &s.grant), http_guards) {
                fault = Some(late);
                return Err(());
            }
            let call = calls;
            calls += 1;
            let mut reserved = 0;
            let routed = match router {
                JobRouter::Raw(router) => router.route(call, request),
                JobRouter::Brokered(router) => match (instance, lease, Request::decode(request)) {
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
                        };
                        let reply = router.route(&mut context, call, &parsed);
                        if let Some(guard) = context.http_guard.take() {
                            // At most one guard per import; imports are bounded by
                            // max_calls. Keep successful authorization through Ready.
                            http_guards.push(guard);
                        }
                        if context.uncertain || (context.expected.is_some() && reply.is_err()) {
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
                },
            };
            bytes += reserved;
            if let Some(late) = control.job_fault(cancel, service.map(|s| &s.grant), http_guards) {
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
    if let Some(late) = control.job_fault(cancel, service.map(|s| &s.grant), http_guards) {
        // A result that outlived its deadline or revocation is not a success.
        let mut report = cancelled_report(package, late);
        report.task.execution.host_calls = run.report.host_calls;
        report.task.execution.fuel_remaining = run.report.fuel_remaining;
        report.calls = calls;
        report.bytes = bytes;
        report.unknown = unknown || calls > 0;
        return report;
    }
    let mut execution = run.report;
    if let Some(fault) = fault {
        execution.outcome = Err(fault);
    }
    let mut response = None;
    let mut http_response = None;
    let mut service_response = None;
    if let Some(service) = service {
        if execution.outcome.is_ok() {
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
                            Ok(reply) => service_response = Some(reply),
                            Err(_) => execution.outcome = Err(Fault::TaskProtocol),
                        },
                    }
                }
                None => execution.outcome = Err(Fault::TaskProtocol),
            }
        }
        // Past an admitted side effect, an invalid/oversized final reply never
        // proves rollback and must not authorize an automatic replay.
        if execution.outcome.is_err() {
            unknown |= calls > 0;
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
        calls,
        bytes,
        cancelled: false,
        unknown,
    }
}
