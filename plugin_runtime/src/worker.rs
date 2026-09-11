//! One bounded native executor owning its package, connection and core. No guest code on UI thread.
//! Stop revokes before cancellation; operations past their final authorization may still commit.
use crate::{Cancellation, Report, package::PreparedPackage};
use morrow_core::dispatch::{Connection, HostRuntime};
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
const MAX_PENDING: usize = 64;
const MAX_TIMEOUT: Duration = Duration::from_secs(3600);
static NEXT_WORKER: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Running,
    Draining,
    Stopping,
    Stopped,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerError {
    InvalidOptions,
    Busy,
    Closed,
    Unavailable,
    Consumed,
    Spawn,
    Disconnect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskId {
    pub worker: u64,
    pub serial: u64,
}
struct State {
    phase: Phase,
    next: u64,
    tasks: BTreeMap<u64, Cancellation>,
}
struct Control {
    revocation: morrow_core::lifecycle::Revocation,
    id: u64,
    capacity: usize,
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
        for token in s.tasks.values() {
            token.cancel();
        }
    }
}
enum Work {
    Legacy(SyncSender<Report>),
    Task(
        Box<morrow_core::task::Invocation>,
        SyncSender<crate::package::TaskReport>,
    ),
}
struct Job {
    serial: u64,
    cancel: Cancellation,
    work: Work,
}
enum Message {
    Job(Job),
    Wake,
}
/// Opaque local task handle. TaskId is diagnostic only, never a runtime authority.
/// Dropping a handle requests cancellation; it does not prove rollback or completion.
pub struct TaskHandle<T = Report> {
    id: TaskId,
    cancel: Cancellation,
    result: Receiver<T>,
    consumed: bool,
}
impl<T> TaskHandle<T> {
    pub fn id(&self) -> TaskId {
        self.id
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    /// Nonblocking and exactly-once result consumption. Unavailable means outcome needs reconciliation.
    pub fn try_result(&mut self) -> Result<Option<T>, WorkerError> {
        if self.consumed {
            return Err(WorkerError::Consumed);
        }
        match self.result.try_recv() {
            Ok(report) => {
                self.consumed = true;
                Ok(Some(report))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                self.consumed = true;
                Err(WorkerError::Unavailable)
            }
        }
    }
}
impl<T> Drop for TaskHandle<T> {
    fn drop(&mut self) {
        if !self.consumed {
            self.cancel();
        }
    }
}
/// Own until try_finish confirms termination. Drop requests stop without blocking UI;
/// the worker retains its core while terminating, including any blocking host call.
pub struct Worker {
    sender: SyncSender<Message>,
    control: Arc<Control>,
    join: Option<JoinHandle<Result<HostRuntime, WorkerError>>>,
}
impl Worker {
    /// Prepare/approve/grant on the trusted side first. Clock must be monotonic across grants and calls.
    pub fn spawn(
        package: PreparedPackage,
        mut host: HostRuntime,
        connection: Connection,
        clock: impl FnMut() -> u64 + Send + 'static,
        capacity: usize,
    ) -> Result<Self, WorkerError> {
        if !(1..=MAX_PENDING).contains(&capacity)
            || connection.package_digest() != Some(package.package().digest())
            || host.connection_phase(&connection)
                != Ok(morrow_core::lifecycle::InstancePhase::Ready)
        {
            return Err(WorkerError::InvalidOptions);
        }
        let id = NEXT_WORKER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| WorkerError::InvalidOptions)?;
        let revocation = host
            .revocation(&connection)
            .map_err(|_| WorkerError::InvalidOptions)?;
        let control = Arc::new(Control {
            revocation,
            id,
            capacity,
            state: Mutex::new(State {
                phase: Phase::Running,
                next: 1,
                tasks: BTreeMap::new(),
            }),
        });
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let inner = Arc::clone(&control);
        let join = thread::Builder::new()
            .name(format!("morrow-plugin-{id}"))
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    execute(&package, &mut host, &connection, clock, &receiver, &inner)
                }));
                let result = match result {
                    Ok(r) => r,
                    Err(_) => Err(WorkerError::Unavailable),
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
                for token in state.tasks.values() {
                    token.cancel();
                }
                state.tasks.clear();
                drop(state);
                result.map(|()| host)
            })
            .map_err(|_| WorkerError::Spawn)?;
        Ok(Self {
            sender,
            control,
            join: Some(join),
        })
    }
    /// Queue + running count is bounded; no blocking send, auto-retry or task replay.
    pub fn submit(&self, timeout: Duration) -> Result<TaskHandle, WorkerError> {
        self.enqueue(timeout, Work::Legacy)
    }
    pub fn submit_task(
        &self,
        input: morrow_core::task::Invocation,
        timeout: Duration,
    ) -> Result<TaskHandle<crate::package::TaskReport>, WorkerError> {
        self.enqueue(timeout, |result| Work::Task(Box::new(input), result))
    }
    fn enqueue<T>(
        &self,
        timeout: Duration,
        work: impl FnOnce(SyncSender<T>) -> Work,
    ) -> Result<TaskHandle<T>, WorkerError> {
        if timeout.is_zero() || timeout > MAX_TIMEOUT {
            return Err(WorkerError::InvalidOptions);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(WorkerError::InvalidOptions)?;
        let mut state = self.control.lock();
        if state.phase != Phase::Running {
            return Err(WorkerError::Closed);
        }
        if state.tasks.len() >= self.control.capacity {
            return Err(WorkerError::Busy);
        }
        let serial = state.next;
        state.next = state.next.checked_add(1).ok_or(WorkerError::Closed)?;
        let cancel = Cancellation::until(deadline);
        let (result, receiver) = mpsc::sync_channel(1);
        state.tasks.insert(serial, cancel.clone());
        if self
            .sender
            .try_send(Message::Job(Job {
                serial,
                cancel: cancel.clone(),
                work: work(result),
            }))
            .is_err()
        {
            state.tasks.remove(&serial);
            return Err(WorkerError::Unavailable);
        }
        Ok(TaskHandle {
            id: TaskId {
                worker: self.control.id,
                serial,
            },
            cancel,
            result: receiver,
            consumed: false,
        })
    }
    /// Stop admission and shorten all outstanding task deadlines. Pure guest loops remain fuel-bounded.
    pub fn drain(&self, timeout: Duration) -> Result<(), WorkerError> {
        if timeout.is_zero() || timeout > MAX_TIMEOUT {
            return Err(WorkerError::InvalidOptions);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(WorkerError::InvalidOptions)?;
        let mut state = self.control.lock();
        if !matches!(state.phase, Phase::Running | Phase::Draining) {
            return Err(WorkerError::Closed);
        }
        state.phase = Phase::Draining;
        for token in state.tasks.values() {
            token.limit_deadline(deadline);
        }
        drop(state);
        let _ = self.sender.try_send(Message::Wake);
        Ok(())
    }
    /// Immediately closes admission, revokes the instance, then cancels all jobs.
    /// Stopping is not Stopped; operations past final authorization may still commit.
    pub fn stop(&self) {
        self.control.stop();
        let _ = self.sender.try_send(Message::Wake);
    }
    pub fn phase(&self) -> Phase {
        self.control.lock().phase
    }
    pub fn pending(&self) -> usize {
        self.control.lock().tasks.len()
    }
    /// Returns exclusive core ownership only after the thread ended and connection was retired.
    pub fn try_finish(&mut self) -> Result<Option<HostRuntime>, WorkerError> {
        let join = self.join.as_ref().ok_or(WorkerError::Consumed)?;
        if !join.is_finished() {
            return Ok(None);
        }
        self.join
            .take()
            .unwrap()
            .join()
            .map_err(|_| WorkerError::Unavailable)?
            .map(Some)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}
fn execute(
    package: &PreparedPackage,
    host: &mut HostRuntime,
    connection: &Connection,
    mut clock: impl FnMut() -> u64,
    receiver: &Receiver<Message>,
    control: &Control,
) -> Result<(), WorkerError> {
    loop {
        {
            let state = control.lock();
            if state.phase != Phase::Running && state.tasks.is_empty() {
                break;
            }
        }
        let message = match receiver.recv() {
            Ok(v) => v,
            Err(_) => break,
        };
        if let Message::Job(job) = message {
            let cancelled = job.cancel.fault().map(|fault| Report {
                outcome: Err(fault),
                host_calls: 0,
                fuel_remaining: package.limits().fuel,
            });
            match job.work {
                Work::Legacy(result) => {
                    let report = cancelled
                        .unwrap_or_else(|| package.run(host, connection, &mut clock, job.cancel));
                    let _ = result.try_send(report);
                }
                Work::Task(input, result) => {
                    let report = if let Some(execution) = cancelled {
                        crate::package::TaskReport {
                            execution,
                            response: None,
                        }
                    } else {
                        package.run_task(host, connection, &input, &mut clock, job.cancel)
                    };
                    let _ = result.try_send(report);
                }
            }
            control.lock().tasks.remove(&job.serial);
        }
    }
    host.disconnect(connection)
        .map_err(|_| WorkerError::Disconnect)
}
