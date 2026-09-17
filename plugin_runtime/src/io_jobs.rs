//! Bounded IO job executor: submit, poll, read and cancel on one native thread.
//! Each job carries a trusted-host contract router for its IO calls; this module
//! performs no I/O itself, never retries a routed call, and never delivers a
//! result that outlived its authorization or deadline as a success.
use crate::{
    Cancellation, Fault, MAX_TASK_BYTES, Report,
    package::{PreparedPackage, TaskReport},
    worker::TaskId,
};
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    io::{MAX_FRAME_BYTES, Request, Response},
};
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
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
/// Trusted-host contract router supplied per job. Calls arrive in guest order
/// and are never replayed by the executor.
pub trait Router: Send {
    fn route(&mut self, call: u32, request: &[u8]) -> Result<Vec<u8>, RouterFault>;
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
    pub fn new(
        max_calls: u32,
        max_job_bytes: u64,
        max_total_bytes: u64,
    ) -> Result<Self, JobError> {
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
#[derive(Debug)]
pub struct JobReport {
    /// Execution facts only; the content exchange is always denied for IO jobs.
    pub task: TaskReport,
    /// The last brokered IO response, decoded against its own request.
    pub response: Option<Response>,
    /// Admitted IO calls, in guest order.
    pub calls: u32,
    /// Cumulatively charged request + response bytes for this job.
    pub bytes: u64,
    /// The result arrived after cancellation or deadline and is not a success.
    pub cancelled: bool,
    /// The last routed call had an unknown external outcome; reconcile.
    pub unknown: bool,
}
impl JobReport {
    /// Bytes a caller must be willing to read before the result is consumed.
    pub fn payload_bytes(&self) -> usize {
        self.response.as_ref().map_or(0, |r| r.payload.len())
            + self.task.output.as_ref().map_or(0, |o| o.bytes.len())
            + self.task.failure.as_ref().map_or(0, |f| f.message.len())
    }
}
struct State {
    phase: Phase,
    next: u64,
    jobs: BTreeMap<u64, Cancellation>,
    bytes: u64,
}
struct Control {
    revocation: morrow_core::lifecycle::Revocation,
    id: u64,
    capacity: usize,
    limits: JobLimits,
    state: Mutex<State>,
}
impl Control {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn stop(&self) {
        let mut s = self.lock();
        if matches!(s.phase, Phase::Stopped | Phase::Failed) {
            return;
        }
        self.revocation.revoke();
        s.phase = Phase::Stopping;
        for token in s.jobs.values() {
            token.cancel();
        }
    }
    /// Cumulative byte admission, never refunded; bytes already charged stay
    /// charged even when a later amount is refused.
    fn charge(&self, amount: u64) -> bool {
        let mut state = self.lock();
        if state
            .bytes
            .checked_add(amount)
            .is_none_or(|n| n > self.limits.max_total_bytes)
        {
            return false;
        }
        state.bytes += amount;
        true
    }
}
struct Job {
    serial: u64,
    cancel: Cancellation,
    input: Vec<u8>,
    router: Box<dyn Router>,
    result: SyncSender<JobReport>,
}
enum Message {
    Job(Job),
    Wake,
}
/// Opaque job handle. Dropping it requests cancellation; it does not prove the
/// external outcome or roll back anything already admitted.
pub struct JobHandle {
    id: TaskId,
    cancel: Cancellation,
    result: Receiver<JobReport>,
    ready: Option<JobReport>,
    consumed: bool,
}
impl JobHandle {
    pub fn id(&self) -> TaskId {
        self.id
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    /// Nonblocking and non-consuming.
    pub fn poll(&mut self) -> Poll {
        if self.consumed {
            return Poll::Consumed;
        }
        if self.ready.is_some() {
            return Poll::Ready;
        }
        match self.result.try_recv() {
            Ok(report) => {
                self.ready = Some(report);
                Poll::Ready
            }
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => {
                self.consumed = true;
                Poll::Unavailable
            }
        }
    }
    /// Exactly-once terminal read. `ReadBound` leaves the result unconsumed so
    /// the caller can retry with an explicit larger allowance.
    pub fn read(&mut self, max_bytes: usize) -> Result<Option<JobReport>, JobError> {
        match self.poll() {
            Poll::Consumed => Err(JobError::Consumed),
            Poll::Unavailable => Err(JobError::Unavailable),
            Poll::Pending => Ok(None),
            Poll::Ready => {
                let report = self.ready.as_ref().ok_or(JobError::Unavailable)?;
                if report.payload_bytes() > max_bytes {
                    return Err(JobError::ReadBound);
                }
                self.consumed = true;
                Ok(self.ready.take())
            }
        }
    }
}
impl Drop for JobHandle {
    fn drop(&mut self) {
        if !self.consumed {
            self.cancel();
        }
    }
}
/// One bounded executor owning its package, connection and core on a dedicated
/// thread. Content commits therefore happen outside any caller transaction.
pub struct IoWorker {
    sender: SyncSender<Message>,
    control: Arc<Control>,
    join: Option<JoinHandle<Result<HostRuntime, JobError>>>,
}
impl IoWorker {
    pub fn spawn(
        package: PreparedPackage,
        mut host: HostRuntime,
        connection: Connection,
        clock: impl FnMut() -> u64 + Send + 'static,
        capacity: usize,
        limits: JobLimits,
    ) -> Result<Self, JobError> {
        if !(1..=MAX_PENDING).contains(&capacity)
            || connection.package_digest() != Some(package.package().digest())
            || host.connection_phase(&connection)
                != Ok(morrow_core::lifecycle::InstancePhase::Ready)
        {
            return Err(JobError::InvalidOptions);
        }
        let id = NEXT_EXECUTOR
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| JobError::InvalidOptions)?;
        let revocation = host
            .revocation(&connection)
            .map_err(|_| JobError::InvalidOptions)?;
        let control = Arc::new(Control {
            revocation,
            id,
            capacity,
            limits,
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
                    execute(&package, &mut host, &connection, clock, &receiver, &inner)
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
                for token in state.jobs.values() {
                    token.cancel();
                }
                state.jobs.clear();
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
    /// Queue plus running count is bounded; no blocking send, auto-retry or job
    /// replay. The complete task input frame is charged immediately and never
    /// refunded.
    pub fn submit(
        &self,
        input: Vec<u8>,
        router: Box<dyn Router>,
        timeout: Duration,
    ) -> Result<JobHandle, JobError> {
        if timeout.is_zero()
            || timeout > MAX_TIMEOUT
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
        if state.phase != Phase::Running {
            return Err(JobError::Closed);
        }
        if state.jobs.len() >= self.control.capacity {
            return Err(JobError::Busy);
        }
        if self
            .control
            .limits
            .max_total_bytes
            .checked_sub(state.bytes)
            .is_none_or(|remaining| input_bytes > remaining)
        {
            return Err(JobError::Limit);
        }
        let serial = state.next;
        state.next = state.next.checked_add(1).ok_or(JobError::Closed)?;
        state.bytes += input_bytes;
        let cancel = Cancellation::until(deadline);
        let (result, receiver) = mpsc::sync_channel(1);
        state.jobs.insert(serial, cancel.clone());
        if self
            .sender
            .try_send(Message::Job(Job {
                serial,
                cancel: cancel.clone(),
                input,
                router,
                result,
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
            cancel,
            result: receiver,
            ready: None,
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
        for token in state.jobs.values() {
            token.limit_deadline(deadline);
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
    package: &PreparedPackage,
    host: &mut HostRuntime,
    connection: &Connection,
    _clock: impl FnMut() -> u64,
    receiver: &Receiver<Message>,
    control: &Arc<Control>,
) -> Result<(), JobError> {
    loop {
        {
            let state = control.lock();
            if state.phase != Phase::Running && state.jobs.is_empty() {
                break;
            }
        }
        let message = match receiver.recv() {
            Ok(message) => message,
            Err(_) => break,
        };
        let Message::Job(job) = message else {
            continue;
        };
        let Job {
            serial,
            cancel,
            input,
            mut router,
            result,
        } = job;
        let report = match cancel.fault() {
            Some(fault) => cancelled_report(package, fault),
            None => run_job(
                package,
                &input,
                &cancel,
                control,
                &mut *router,
            ),
        };
        let _ = result.try_send(report);
        control.lock().jobs.remove(&serial);
    }
    host.disconnect(connection)
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
        calls: 0,
        bytes: 0,
        cancelled: true,
        unknown: false,
    }
}
fn run_job(
    package: &PreparedPackage,
    input: &[u8],
    cancel: &Cancellation,
    control: &Arc<Control>,
    router: &mut dyn Router,
) -> JobReport {
    let mut calls = 0u32;
    let mut bytes = 0u64;
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
            if calls >= control.limits.max_calls {
                fault = Some(Fault::Limits);
                return Err(());
            }
            // The request is charged before the router may produce any external
            // effect, so a byte cap is never discovered after the call.
            let request_charge = request.len() as u64;
            if bytes
                .checked_add(request_charge)
                .is_none_or(|total| total > control.limits.max_job_bytes)
                || !control.charge(request_charge)
            {
                fault = Some(Fault::Limits);
                return Err(());
            }
            bytes += request_charge;
            let response = match router.route(calls, request) {
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
            let response_charge = response.len() as u64;
            if response.is_empty()
                || response.len() > MAX_FRAME_BYTES
                || bytes
                    .checked_add(response_charge)
                    .is_none_or(|total| total > control.limits.max_job_bytes)
                || !control.charge(response_charge)
            {
                // The call already reached the router; the result cannot be
                // delivered within quota, so the outcome needs reconciliation.
                fault = Some(Fault::Limits);
                unknown = true;
                return Err(());
            }
            bytes += response_charge;
            calls += 1;
            last_request = Some(request.to_vec());
            last_response = Some(response.clone());
            Ok(response)
        },
        cancel.clone(),
    );
    if let Some(late) = cancel.fault() {
        // A result that outlived its deadline or revocation is not a success.
        let mut report = cancelled_report(package, late);
        report.task.execution.host_calls = run.report.host_calls;
        report.task.execution.fuel_remaining = run.report.fuel_remaining;
        report.calls = calls;
        report.bytes = bytes;
        return report;
    }
    let mut execution = run.report;
    if let Some(fault) = fault {
        execution.outcome = Err(fault);
    }
    let mut response = None;
    if execution.outcome.is_ok() {
        match (run.completion, last_response, last_request) {
            (Some(done), Some(actual), Some(request)) if calls > 0 && done == actual => {
                match Request::decode(&request).map_err(|_| ()).and_then(|request| {
                    Response::decode(&request, &actual).map_err(|_| ())
                }) {
                    Ok(value) => response = Some(value),
                    Err(()) => execution.outcome = Err(Fault::TaskProtocol),
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
        calls,
        bytes,
        cancelled: false,
        unknown,
    }
}
