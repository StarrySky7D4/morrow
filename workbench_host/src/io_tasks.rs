//! Trusted application ownership and short, nonblocking IO task controls.
//! This is not a guest capability or an approval for a network destination.
use crate::{Result, Workbench, WorkbenchState, now};
use morrow_core::{dispatch::HostRuntime, plugin_package::io::IoCapability};
use morrow_plugin_runtime::{
    io_binding::{IoBinding, MutationBudget},
    io_jobs::{BrokerRouter, IoWorker, JobError, JobHandle, JobLimits, JobReport, Poll},
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind},
    time::Duration,
};

#[cfg(windows)]
#[path = "mutation_tasks.rs"]
pub mod mutation;

#[cfg(windows)]
#[path = "directory_tasks.rs"]
pub mod directory;
#[path = "file_tasks.rs"]
pub mod file;
#[path = "service_tasks.rs"]
pub mod service;
#[path = "service_commands.rs"]
pub mod service_commands;

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
/// Only a trusted guest mutation start may select the extended binding.
/// Historical reads use ordinary ceilings and cannot execute guest or effect jobs.
#[derive(Clone, Copy)]
enum BindingProfile {
    Ordinary,
    Mutation(MutationBudget),
    MutationHistory,
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
// Preserve all existing file/IO/mutation admission paths through pure wrapping.
// Conversion occurs immediately after the original preparation closure returns.
enum TaskAdmission {
    Original(file::Admission),
    #[cfg(windows)]
    Directory(directory::Admission),
}
enum TaskSubmitted {
    Original(file::Submitted),
    #[cfg(windows)]
    Directory(directory::DirectoryTask),
}
impl From<file::Admission> for TaskAdmission {
    fn from(value: file::Admission) -> Self {
        Self::Original(value)
    }
}
#[cfg(windows)]
impl From<directory::Admission> for TaskAdmission {
    fn from(value: directory::Admission) -> Self {
        Self::Directory(value)
    }
}
impl TaskAdmission {
    fn timeout(&self) -> Duration {
        match self {
            Self::Original(value) => value.timeout(),
            #[cfg(windows)]
            Self::Directory(value) => value.timeout(),
        }
    }
    fn submit(self, worker: &IoWorker<WorkbenchState>) -> Result<TaskSubmitted> {
        match self {
            Self::Original(value) => value.submit(worker).map(TaskSubmitted::Original),
            #[cfg(windows)]
            Self::Directory(value) => value.submit(worker).map(TaskSubmitted::Directory),
        }
    }
    fn persistent(&self) -> bool {
        !matches!(self, Self::Original(file::Admission::Io(_)))
    }
}

struct Task {
    key: TaskKey,
    commands: service_commands::Registry,
    worker: Option<Executor>,
    handle: Option<JobHandle>,
    stopping: bool,
    exit: Option<ExitStatus>,
    service: Option<service::Progress>,
    file: Option<file::FileTask>,
    #[cfg(windows)]
    directory: Option<directory::DirectoryTask>,
    #[cfg(windows)]
    mutation: Option<mutation::MutationTask>,
}
enum Executor {
    Io(Box<IoWorker<WorkbenchState>>),
    Service(service::ServiceExecution),
    #[cfg(windows)]
    Agent(Box<crate::agent_tasks::AgentWorker<WorkbenchState>>),
}
enum Reclaimed {
    Original(Box<morrow_plugin_runtime::io_jobs::WorkerExit<WorkbenchState>>),
    #[cfg(windows)]
    Agent(Box<crate::agent_tasks::AgentExit<WorkbenchState>>),
}
enum Cleanup {
    Original(ManagedInstance),
    #[cfg(windows)]
    Agent(Box<crate::agent_tasks::AgentCleanup>),
}
impl Executor {
    fn stop(&self) {
        match self {
            Self::Io(worker) => worker.stop(),
            Self::Service(worker) => worker.stop(),
            #[cfg(windows)]
            Self::Agent(worker) => worker.stop(),
        }
    }
    fn try_reclaim(&mut self) -> std::result::Result<Option<Reclaimed>, JobError> {
        match self {
            Self::Io(worker) => worker
                .try_reclaim()
                .map(|e| e.map(|e| Reclaimed::Original(Box::new(e)))),
            Self::Service(worker) => worker
                .try_reclaim()
                .map(|e| e.map(|e| Reclaimed::Original(Box::new(e)))),
            #[cfg(windows)]
            Self::Agent(worker) => worker
                .try_reclaim()
                .map(|e| e.map(|e| Reclaimed::Agent(Box::new(e))))
                .map_err(|_| JobError::Unavailable),
        }
    }
    fn service_progress(&self) -> Option<service::Progress> {
        match self {
            Self::Io(_) => None,
            Self::Service(worker) => Some(worker.progress()),
            #[cfg(windows)]
            Self::Agent(_) => None,
        }
    }
}
/// The complete state is either local or owned by the original worker. No
/// fallback store, partial state, or panicking Deref can mask its absence.
pub(crate) struct StateSlot {
    #[cfg(windows)]
    pub(crate) agent_preparation_debt:
        Option<crate::agent_tasks::preparation::AgentPreparationDebt>,
    #[cfg(windows)]
    pub(crate) agent_contexts:
        std::sync::Arc<crate::agent_tasks::context_lifecycle::ContextLifecycleLedger>,
    pub(crate) product_gate: crate::product_gate::ProductGate,
    owner: Option<WorkbenchState>,
    task: Option<Task>,
    cleanup: Option<Cleanup>,
    #[cfg(windows)]
    agent_exit: Option<(TaskKey, crate::agent_tasks::AgentExitStatus)>,
    repair_needed: bool,
    lost: bool,
    service_submissions: BTreeSet<[u8; 32]>,
    file_submissions: BTreeSet<[u8; 32]>,
    #[cfg(windows)]
    mutation_submissions: std::collections::BTreeMap<[u8; 32], ([u8; 32], Option<TaskKey>)>,
    #[cfg(windows)]
    mutation_checkpoints:
        std::collections::VecDeque<([u8; 32], morrow_core::store::FileMutationPlanCheckpoint)>,
    pub(crate) submission: Option<[u8; 32]>,
}
impl StateSlot {
    #[cfg(windows)]
    pub(crate) fn ensure_agent_admission(&self) -> Result<()> {
        self.require_writable()?;
        if self.task.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        Ok(())
    }
    #[cfg(windows)]
    pub(crate) fn take_agent_owner(&mut self) -> Result<WorkbenchState> {
        // Admission and preparation happened under the same exclusive borrow.
        // Gate loss now is handled by spawn returning this exact owner/bridge.
        if self.task.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        self.owner
            .take()
            .ok_or_else(|| AccessError::OwnerUnavailable.into())
    }
    #[cfg(windows)]
    pub(crate) fn install_agent_worker(
        &mut self,
        key: TaskKey,
        worker: crate::agent_tasks::AgentWorker<WorkbenchState>,
    ) {
        self.agent_exit = None;
        self.task = Some(Task {
            key,
            commands: Default::default(),
            worker: Some(Executor::Agent(Box::new(worker))),
            handle: None,
            stopping: false,
            exit: None,
            service: None,
            file: None,
            directory: None,
            mutation: None,
        });
    }
    #[cfg(windows)]
    pub(crate) fn restore_agent_failure(
        &mut self,
        key: TaskKey,
        failure: crate::agent_tasks::AgentSpawnFailure<WorkbenchState>,
    ) {
        self.owner = Some(failure.owner);
        self.cleanup = Some(Cleanup::Agent(Box::new(failure.cleanup)));
        self.repair_needed = true;
        let status = crate::agent_tasks::AgentExitStatus {
            execution: Err(failure.error),
            disconnect: Err(crate::agent_tasks::AgentError::Disconnect),
            maintenance: Ok(()),
        };
        self.agent_exit = Some((key, status));
        self.task = Some(Task {
            key,
            commands: Default::default(),
            worker: None,
            handle: None,
            stopping: false,
            exit: Some(ExitStatus {
                execution: Err(JobError::Spawn),
                disconnect: Err(JobError::Disconnect),
                maintenance: Ok(()),
            }),
            service: None,
            file: None,
            directory: None,
            mutation: None,
        });
    }
    #[cfg(windows)]
    pub(crate) fn check_agent_task(&self, key: TaskKey) -> Result<()> {
        let task = self
            .task
            .as_ref()
            .filter(|t| t.key == key)
            .ok_or(AccessError::StaleTask)?;
        if matches!(task.worker.as_ref(), Some(Executor::Agent(_)))
            || self.agent_exit.is_some_and(|(old, _)| old == key)
        {
            Ok(())
        } else {
            Err(AccessError::StaleTask.into())
        }
    }
    #[cfg(windows)]
    pub(crate) fn agent_worker(
        &self,
        key: TaskKey,
    ) -> Result<&crate::agent_tasks::AgentWorker<WorkbenchState>> {
        self.check_agent_task(key)?;
        match self.task.as_ref().and_then(|t| t.worker.as_ref()) {
            Some(Executor::Agent(worker)) => Ok(worker),
            _ => Err(AccessError::StaleTask.into()),
        }
    }
    #[cfg(windows)]
    pub(crate) fn agent_snapshot(
        &mut self,
        key: TaskKey,
    ) -> Result<crate::agent_tasks::AgentSnapshot> {
        self.check_agent_task(key)?;
        let progress = self.agent_worker(key).ok().map(|w| w.progress());
        let exit = self.agent_exit.filter(|(k, _)| *k == key).map(|(_, e)| e);
        Ok(crate::agent_tasks::AgentSnapshot {
            task: self.snapshot(),
            progress,
            exit,
        })
    }
    #[cfg(windows)]
    pub(crate) fn current_agent_snapshot(
        &mut self,
    ) -> Result<Option<crate::agent_tasks::AgentSnapshot>> {
        let Some(key) = self.task.as_ref().map(|task| task.key) else {
            return Ok(None);
        };
        if self.check_agent_task(key).is_err() {
            return Ok(None);
        }
        self.agent_snapshot(key).map(Some)
    }
    pub(crate) fn take_channel_owner(&mut self) -> Result<WorkbenchState> {
        self.require_writable()?;
        if self.task.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        self.owner.take().ok_or_else(|| AccessError::Busy.into())
    }
    pub(crate) fn restore_channel_owner(
        &mut self,
        owner: WorkbenchState,
        repair: bool,
    ) -> Result<()> {
        if self.owner.is_some() || self.task.is_some() || self.lost {
            return Err(AccessError::OwnerUnavailable.into());
        }
        self.owner = Some(owner);
        self.repair_needed |= repair;
        Ok(())
    }
    pub(crate) fn channel_owner_lost(&mut self) {
        self.lost = true;
        self.owner = None;
    }
    pub(crate) fn has_service_task(&self) -> bool {
        self.task
            .as_ref()
            .is_some_and(|task| task.service.is_some())
    }
    pub(crate) fn new(owner: WorkbenchState) -> Self {
        #[cfg(windows)]
        let agent_contexts = crate::agent_tasks::context_lifecycle::ContextLifecycleLedger::new(
            owner.host.binding(),
        );
        Self {
            #[cfg(windows)]
            agent_contexts,
            #[cfg(windows)]
            agent_preparation_debt: None,
            product_gate: Default::default(),
            owner: Some(owner),
            task: None,
            cleanup: None,
            #[cfg(windows)]
            agent_exit: None,
            repair_needed: false,
            lost: false,
            service_submissions: BTreeSet::new(),
            file_submissions: BTreeSet::new(),
            #[cfg(windows)]
            mutation_submissions: Default::default(),
            #[cfg(windows)]
            mutation_checkpoints: Default::default(),
            submission: None,
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
        self.product_gate.check()?;
        self.local()?;
        #[cfg(windows)]
        if self.agent_preparation_debt.is_some() || self.agent_contexts.needs_repair() {
            return Err(AccessError::RecoveryRequired.into());
        }
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
        #[cfg(windows)]
        if self.agent_preparation_debt.is_some() || self.agent_contexts.needs_repair() {
            return Some("代理会话准备的连接清理尚未完成，请显式修复后再关闭内容库。");
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
        let result = worker.try_reclaim();
        if let Some(progress) = worker.service_progress() {
            task.stopping |= progress.phase == service::ServicePhase::Stopping;
            task.service = Some(progress);
        }
        match result {
            Ok(None) => Ok(false),
            Ok(Some(Reclaimed::Original(exit))) => {
                // Restore the original owner before recording any fallible cleanup.
                self.owner = Some(exit.owner);
                self.cleanup = exit.instance.map(Cleanup::Original);
                self.repair_needed = exit.maintenance.is_err() || exit.disconnect.is_err();
                task.exit = Some(ExitStatus {
                    execution: exit.result,
                    disconnect: exit.disconnect,
                    maintenance: exit.maintenance,
                });
                task.worker = None;
                if let Some(progress) = &mut task.service {
                    progress.phase = service::ServicePhase::Exited;
                }
                Ok(true)
            }
            #[cfg(windows)]
            Ok(Some(Reclaimed::Agent(exit))) => {
                // AgentExit exists only after native cleanup and actual thread join.
                let status = crate::agent_tasks::AgentExitStatus {
                    execution: exit.execution,
                    disconnect: exit.disconnect,
                    maintenance: exit.maintenance,
                };
                self.owner = Some(exit.owner);
                self.cleanup = exit.cleanup.map(|c| Cleanup::Agent(Box::new(c)));
                self.repair_needed = status.disconnect.is_err() || status.maintenance.is_err();
                self.agent_exit = Some((task.key, status));
                task.exit = Some(ExitStatus {
                    execution: status.execution.map_err(|_| JobError::Unavailable),
                    disconnect: status.disconnect.map_err(|_| JobError::Disconnect),
                    maintenance: status.maintenance.map_err(|_| JobError::Unavailable),
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
            delivery: self.task.as_mut().and_then(|t| {
                t.handle
                    .as_mut()
                    .map(JobHandle::poll)
                    .or_else(|| t.file.as_ref().and_then(file::FileTask::poll))
                    .or_else(|| {
                        #[cfg(windows)]
                        {
                            t.directory
                                .as_ref()
                                .and_then(directory::DirectoryTask::poll)
                        }
                        #[cfg(not(windows))]
                        {
                            None
                        }
                    })
                    .or_else(|| {
                        #[cfg(windows)]
                        {
                            t.mutation.as_mut().and_then(mutation::MutationTask::poll)
                        }
                        #[cfg(not(windows))]
                        {
                            None
                        }
                    })
            }),
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
            self.cleanup = Some(Cleanup::Original(instance));
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
        if let Some(cleanup) = self.state.cleanup.take() {
            match cleanup {
                Cleanup::Original(instance) => self.state.cleanup_instance(instance)?,
                #[cfg(windows)]
                Cleanup::Agent(mut cleanup) => {
                    let result = cleanup.repair(
                        self.state
                            .owner
                            .as_mut()
                            .ok_or(AccessError::OwnerUnavailable)?,
                    );
                    if result.is_err() {
                        self.state.cleanup = Some(Cleanup::Agent(cleanup));
                        self.state.repair_needed = true;
                        return Err(AccessError::RecoveryRequired.into());
                    }
                }
            }
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
        self.start_task(options, |p| prepare(p).map(file::Admission::Io))
    }
    fn start_task<J: Into<TaskAdmission>>(
        &mut self,
        options: StartOptions,
        prepare: impl FnOnce(Preparation<'_>) -> Result<J>,
    ) -> Result<TaskKey> {
        self.start_task_with_binding(options, BindingProfile::Ordinary, prepare)
    }
    fn start_task_with_binding<J: Into<TaskAdmission>>(
        &mut self,
        options: StartOptions,
        binding_profile: BindingProfile,
        prepare: impl FnOnce(Preparation<'_>) -> Result<J>,
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
            let binding = match binding_profile {
                BindingProfile::Ordinary => manager.bind_io(
                    &state.host,
                    &instance,
                    options.digest,
                    options.revision,
                    &options.capabilities,
                    expires,
                    time,
                )?,
                BindingProfile::Mutation(approved) => manager.bind_budgeted_mutation(
                    &state.host,
                    &instance,
                    options.digest,
                    options.revision,
                    &options.capabilities,
                    expires,
                    time,
                    approved,
                )?,
                BindingProfile::MutationHistory => manager.bind_mutation_history(
                    &state.host,
                    &instance,
                    options.digest,
                    options.revision,
                    &options.capabilities,
                    expires,
                    time,
                )?,
            };
            let job: TaskAdmission = prepare(Preparation {
                manager,
                host: &state.host,
                instance: &instance,
                binding: &binding,
                now: time,
            })?
            .into();
            if job.timeout().is_zero() || job.timeout() > options.lifetime {
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
                        commands: Default::default(),
                        key,
                        worker: None,
                        handle: None,
                        stopping: false,
                        service: None,
                        file: None,
                        #[cfg(windows)]
                        directory: None,
                        #[cfg(windows)]
                        mutation: None,
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
                    commands: Default::default(),
                    key,
                    worker: None,
                    handle: None,
                    stopping: false,
                    service: None,
                    file: None,
                    #[cfg(windows)]
                    directory: None,
                    #[cfg(windows)]
                    mutation: None,
                    exit: Some(ExitStatus {
                        execution: Err(error),
                        disconnect,
                        maintenance: Ok(()),
                    }),
                });
                return Err(format!("IO worker admission failed: {error:?}").into());
            }
        };
        let persistent_job = job.persistent();
        let admission = self.state.product_gate.register(worker.stop_handle());
        let submitted = if admission.is_ok() {
            Some(job.submit(&worker))
        } else {
            None
        };
        self.state.task = Some(Task {
            commands: Default::default(),
            key,
            worker: Some(Executor::Io(Box::new(worker))),
            handle: None,
            stopping: false,
            exit: None,
            service: None,
            file: None,
            #[cfg(windows)]
            directory: None,
            #[cfg(windows)]
            mutation: None,
        });
        if let Err(error) = admission {
            self.state.request_stop();
            return Err(error);
        }
        let task = self.state.checked_task(key)?;
        match submitted.expect("admitted original worker") {
            Ok(TaskSubmitted::Original(file::Submitted::Io(handle))) => task.handle = Some(handle),
            Ok(TaskSubmitted::Original(file::Submitted::File(file))) => task.file = Some(file),
            #[cfg(windows)]
            Ok(TaskSubmitted::Directory(directory)) => task.directory = Some(directory),
            #[cfg(windows)]
            Ok(TaskSubmitted::Original(file::Submitted::Mutation(mutation))) => {
                task.mutation = Some(mutation)
            }
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
        if !persistent_job
            && let Some(Executor::Io(worker)) = &task.worker
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
