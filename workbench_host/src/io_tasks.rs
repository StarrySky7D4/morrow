//! Trusted application ownership and short, nonblocking IO task controls.
//! This is not a guest capability or an approval for a network destination.
use crate::{Result, Workbench, WorkbenchState, now};
use morrow_core::{dispatch::HostRuntime, plugin_package::io::IoCapability};
use morrow_plugin_runtime::{
    io_binding::IoBinding,
    io_jobs::{BrokerRouter, IoWorker, JobError, JobHandle, JobLimits, JobReport, Poll},
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessError {
    Busy,
    RecoveryRequired,
    OwnerUnavailable,
    StaleTask,
    UnacknowledgedTask,
}
impl std::fmt::Display for AccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Busy => "内容库正在执行后台任务，请稍后重试。",
            Self::RecoveryRequired => "后台任务已退出，但存储维护或连接清理需要恢复。",
            Self::OwnerUnavailable => "后台线程未能归还原内容库；不能重新打开内容库继续执行。",
            Self::StaleTask => "任务身份已失效，请刷新任务状态。",
            Self::UnacknowledgedTask => "请先确认上一任务的结束状态。",
        })
    }
}
impl std::error::Error for AccessError {}

/// Random identity bound to one admission in this workbench session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskKey([u8; 32]);
impl TaskKey {
    pub fn from_bytes(value: &[u8]) -> Result<Self> {
        Ok(Self(value.try_into().map_err(|_| AccessError::StaleTask)?))
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoragePhase {
    Local,
    Running,
    Stopping,
    Reclaimed,
    RecoveryRequired,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitStatus {
    /// Executor exit status; the actual operation outcome belongs to JobReport
    /// and durable IO history. An error here is never evidence of rollback.
    pub execution: std::result::Result<(), JobError>,
    pub disconnect: std::result::Result<(), JobError>,
    pub maintenance: std::result::Result<(), JobError>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub key: Option<TaskKey>,
    pub storage: StoragePhase,
    /// Ready still requires read's final authorization check.
    pub delivery: Option<Poll>,
    pub exit: Option<ExitStatus>,
}
pub struct StartOptions {
    pub package_id: String,
    pub digest: [u8; 32],
    pub revision: u64,
    pub capabilities: BTreeSet<IoCapability>,
    pub lifetime: Duration,
    pub limits: JobLimits,
}
/// Resources must be approved on these exact references, before ownership moves.
pub struct Preparation<'a> {
    pub manager: &'a Manager,
    pub host: &'a HostRuntime,
    pub instance: &'a ManagedInstance,
    pub binding: &'a IoBinding,
    pub now: u64,
}
pub struct PreparedJob {
    pub input: Vec<u8>,
    pub router: Box<dyn BrokerRouter>,
    pub timeout: Duration,
}
struct Task {
    key: TaskKey,
    worker: Option<IoWorker<WorkbenchState>>,
    handle: Option<JobHandle>,
    stopping: bool,
    exit: Option<ExitStatus>,
}
/// The complete state is either local or owned by the original worker. No
/// fallback store, partial state, or panicking Deref can mask its absence.
pub(crate) struct StateSlot {
    owner: Option<WorkbenchState>,
    task: Option<Task>,
    cleanup: Option<ManagedInstance>,
    repair_needed: bool,
    lost: bool,
}
impl StateSlot {
    pub(crate) fn new(owner: WorkbenchState) -> Self {
        Self {
            owner: Some(owner),
            task: None,
            cleanup: None,
            repair_needed: false,
            lost: false,
        }
    }
    pub(crate) fn local(&self) -> Result<&WorkbenchState> {
        if self.lost {
            return Err(AccessError::OwnerUnavailable.into());
        }
        let owner = self.owner.as_ref().ok_or(AccessError::Busy)?;
        if self.cleanup.is_some() {
            return Err(AccessError::RecoveryRequired.into());
        }
        Ok(owner)
    }
    pub(crate) fn local_mut(&mut self) -> Result<&mut WorkbenchState> {
        self.local()?;
        self.owner.as_mut().ok_or_else(|| AccessError::Busy.into())
    }
    pub(crate) fn require_writable(&self) -> Result<()> {
        self.local()?;
        if self.repair_needed {
            return Err(AccessError::RecoveryRequired.into());
        }
        Ok(())
    }
    pub(crate) fn finish_maintenance(&mut self) -> Result<()> {
        self.local_mut()?.host.flush_pending()?;
        self.repair_needed = false;
        Ok(())
    }
    pub(crate) fn warning(&self) -> Option<&str> {
        if self.lost {
            return Some("后台线程未能归还原内容库。");
        }
        if self.owner.is_none() {
            return Some("内容库正在执行后台任务。");
        }
        if self.repair_needed || self.cleanup.is_some() {
            return Some("后台任务已退出，存储维护或连接清理需要恢复。");
        }
        self.owner.as_ref().and_then(|state| state.host.warning())
    }
    pub(crate) fn request_stop(&mut self) {
        if let Some(task) = &mut self.task
            && let Some(worker) = &task.worker
        {
            task.stopping = true;
            worker.stop();
        }
    }
    /// A stop request or a terminal job report is not a completed thread join.
    pub(crate) fn try_reclaim(&mut self) -> Result<bool> {
        let Some(task) = &mut self.task else {
            return Ok(false);
        };
        let Some(worker) = &mut task.worker else {
            return Ok(false);
        };
        match worker.try_reclaim() {
            Ok(None) => Ok(false),
            Ok(Some(exit)) => {
                // Restore the original owner before recording any fallible cleanup.
                self.owner = Some(exit.owner);
                self.cleanup = exit.instance;
                self.repair_needed = exit.maintenance.is_err() || exit.disconnect.is_err();
                task.exit = Some(ExitStatus {
                    execution: exit.result,
                    disconnect: exit.disconnect,
                    maintenance: exit.maintenance,
                });
                task.worker = None;
                Ok(true)
            }
            Err(error) => {
                self.lost = true;
                task.worker = None;
                task.exit = Some(ExitStatus {
                    execution: Err(error),
                    disconnect: Err(JobError::Unavailable),
                    maintenance: Err(JobError::Unavailable),
                });
                Err(AccessError::OwnerUnavailable.into())
            }
        }
    }
    fn snapshot(&mut self) -> Snapshot {
        let storage = if self.lost {
            StoragePhase::Unavailable
        } else if self.owner.is_none() {
            if self.task.as_ref().is_some_and(|t| t.stopping) {
                StoragePhase::Stopping
            } else {
                StoragePhase::Running
            }
        } else if self.repair_needed || self.cleanup.is_some() {
            StoragePhase::RecoveryRequired
        } else if self.task.is_some() {
            StoragePhase::Reclaimed
        } else {
            StoragePhase::Local
        };
        Snapshot {
            key: self.task.as_ref().map(|t| t.key),
            storage,
            delivery: self
                .task
                .as_mut()
                .and_then(|t| t.handle.as_mut().map(JobHandle::poll)),
            exit: self.task.as_ref().and_then(|t| t.exit),
        }
    }
    fn checked_task(&mut self, key: TaskKey) -> Result<&mut Task> {
        self.task
            .as_mut()
            .filter(|t| t.key == key)
            .ok_or_else(|| AccessError::StaleTask.into())
    }
    fn cleanup_instance(&mut self, instance: ManagedInstance) -> Result<()> {
        let result = self
            .owner
            .as_mut()
            .ok_or(AccessError::OwnerUnavailable)?
            .host
            .disconnect(instance.connection());
        if let Err(error) = result {
            self.cleanup = Some(instance);
            self.repair_needed = true;
            return Err(error.into());
        }
        Ok(())
    }
}
impl Workbench {
    /// Nonblocking observation. Only an actual completed join restores access.
    pub fn io_status(&mut self) -> Snapshot {
        let _ = self.state.try_reclaim();
        self.state.snapshot()
    }
    pub fn poll_io(&mut self, key: TaskKey) -> Result<Snapshot> {
        self.state.checked_task(key)?;
        let _ = self.state.try_reclaim();
        Ok(self.state.snapshot())
    }
    pub fn read_io(&mut self, key: TaskKey, max_bytes: usize) -> Result<Option<JobReport>> {
        let task = self.state.checked_task(key)?;
        let handle = task.handle.as_mut().ok_or(AccessError::StaleTask)?;
        handle
            .read(max_bytes)
            .map_err(|e| format!("IO result: {e:?}").into())
    }
    pub fn cancel_io(&mut self, key: TaskKey) -> Result<Snapshot> {
        self.state.checked_task(key)?;
        self.state.request_stop();
        Ok(self.io_status())
    }
    /// Explicitly forget one finished task. Never starts or retries its request.
    pub fn acknowledge_io(&mut self, key: TaskKey) -> Result<()> {
        self.state.checked_task(key)?;
        self.state.try_reclaim()?;
        self.state.local()?;
        if self.state.repair_needed {
            return Err(AccessError::RecoveryRequired.into());
        }
        self.state.task = None;
        Ok(())
    }
    /// Retry only cleanup/sealing of this same returned owner, never its job.
    pub fn repair_io(&mut self, key: TaskKey) -> Result<Snapshot> {
        self.state.checked_task(key)?;
        self.state.try_reclaim()?;
        if self.state.lost {
            return Err(AccessError::OwnerUnavailable.into());
        }
        if self.state.owner.is_none() {
            return Err(AccessError::Busy.into());
        }
        if let Some(instance) = self.state.cleanup.take() {
            self.state.cleanup_instance(instance)?;
        }
        self.state.finish_maintenance()?;
        Ok(self.state.snapshot())
    }
    /// Start one explicitly approved job on an independently admitted instance.
    /// The preparer is trusted local code, must be bounded, and must not send IO.
    /// Destination/credential approval remains the preparer's responsibility.
    pub fn start_io(
        &mut self,
        options: StartOptions,
        prepare: impl FnOnce(Preparation<'_>) -> Result<PreparedJob>,
    ) -> Result<TaskKey> {
        self.state.try_reclaim()?;
        self.state.local()?;
        if self.state.repair_needed {
            return Err(AccessError::RecoveryRequired.into());
        }
        if self.state.task.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if options.lifetime.is_zero()
            || options.lifetime > morrow_plugin_runtime::io_jobs::MAX_TIMEOUT
        {
            return Err("invalid IO lifetime".into());
        }
        let start = self.state.local()?.start;
        let time = now(start);
        let expires = time
            .checked_add(u64::try_from(options.lifetime.as_millis())?)
            .ok_or("IO deadline overflow")?;
        if expires <= time {
            return Err("IO lifetime must be at least one millisecond".into());
        }
        let state = self.state.local_mut()?;
        let manager = state.manager.as_mut().ok_or("plugin manager unavailable")?;
        let selection = manager
            .selection(&options.package_id)
            .ok_or("plugin not selected")?;
        if manager.revision() != options.revision
            || selection.digest != options.digest
            || !selection.enabled
        {
            return Err("plugin approval changed".into());
        }
        let mut key = [0; 32];
        getrandom::fill(&mut key)?;
        let key = TaskKey(key);
        state.host.prepare_write()?;
        let instance = manager.connect(&options.package_id, &mut state.host)?;
        let prepared = catch_unwind(AssertUnwindSafe(|| -> Result<_> {
            let binding = manager.bind_io(
                &state.host,
                &instance,
                options.digest,
                options.revision,
                &options.capabilities,
                expires,
                time,
            )?;
            let job = prepare(Preparation {
                manager,
                host: &state.host,
                instance: &instance,
                binding: &binding,
                now: time,
            })?;
            if job.timeout.is_zero() || job.timeout > options.lifetime {
                return Err("invalid IO job timeout".into());
            }
            Ok((binding, job))
        }))
        .unwrap_or_else(|_| Err("IO preparation panicked; no job started".into()));
        let (binding, job) = match prepared {
            Ok(value) => value,
            Err(error) => {
                // A failed cleanup remains explicit and retains its exact instance.
                if self.state.cleanup_instance(instance).is_err() {
                    self.state.task = Some(Task {
                        key,
                        worker: None,
                        handle: None,
                        stopping: false,
                        exit: Some(ExitStatus {
                            execution: Err(JobError::InvalidOptions),
                            disconnect: Err(JobError::Disconnect),
                            maintenance: Ok(()),
                        }),
                    });
                }
                return Err(error);
            }
        };
        let owner = self.state.owner.take().ok_or(AccessError::Busy)?;
        let worker = match IoWorker::spawn_managed_owner(
            owner,
            instance,
            binding,
            move || now(start),
            1,
            options.limits,
        ) {
            Ok(worker) => worker,
            Err(failure) => {
                let error = failure.error;
                self.state.owner = Some(failure.owner);
                let disconnect = if let Some(instance) = failure.instance {
                    self.state
                        .cleanup_instance(instance)
                        .map_err(|_| JobError::Disconnect)
                } else {
                    Ok(())
                };
                self.state.task = Some(Task {
                    key,
                    worker: None,
                    handle: None,
                    stopping: false,
                    exit: Some(ExitStatus {
                        execution: Err(error),
                        disconnect,
                        maintenance: Ok(()),
                    }),
                });
                return Err(format!("IO worker admission failed: {error:?}").into());
            }
        };
        let submitted = worker.submit_brokered(job.input, job.router, job.timeout);
        self.state.task = Some(Task {
            key,
            worker: Some(worker),
            handle: None,
            stopping: false,
            exit: None,
        });
        let task = self.state.checked_task(key)?;
        match submitted {
            Ok(handle) => task.handle = Some(handle),
            Err(error) => {
                self.state.request_stop();
                return Err(format!(
                    "IO submission failed: {error:?}; inspect task status before retrying"
                )
                .into());
            }
        }
        // Draining preserves Ready until read, abandonment or deadline, so final
        // delivery can still check the original active instance and authority.
        if let Some(worker) = &task.worker
            && let Err(error) = worker.drain(options.lifetime)
        {
            self.state.request_stop();
            return Err(
                format!("IO drain failed: {error:?}; inspect task status before retrying").into(),
            );
        }
        Ok(key)
    }
}

#[cfg(all(test, target_os = "windows"))]
#[path = "io_tasks_tests.rs"]
mod tests;
