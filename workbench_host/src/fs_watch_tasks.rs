//! One explicitly approved, bounded observation on the complete protected owner.
//! The native chooser confirms the immutable nomination on the original worker.
//! A result receipt never grants a path, renews a window or repairs an Unknown.
use super::{AccessError, Cleanup, Executor, ExitStatus, Snapshot, StateSlot, Task, TaskKey};
use crate::{Result, Workbench, WorkbenchState, now, product_gate::StopRegistration};
use morrow_core::plugin_package::io::IoCapability;
use morrow_plugin_runtime::{
    Report,
    directory_io::DirectoryAbsolutePath,
    fs_watch_binding::{ApprovalRequest, WatchApproval},
    fs_watch_worker::{self, Delivery, Error, NativeRetirement, WatchWorker},
    io_binding::Usage,
    io_jobs::JobError,
};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

/// Trusted local approval input. A guest cannot submit or substitute this path.
pub struct WatchStart {
    pub submission: [u8; 32],
    pub package_id: String,
    pub digest: [u8; 32],
    pub revision: u64,
    pub handler: String,
    pub nominated_path: PathBuf,
    pub max_frames: u32,
    pub max_wire_bytes: u64,
    pub window_ms: u64,
}
#[derive(Clone, Debug)]
pub struct DeliveryReceipt {
    pub sequence: u64,
    pub digest: [u8; 32],
    pub execution: Report,
}
#[derive(Clone, Debug)]
pub struct WatchResult {
    pub outcome: std::result::Result<(), Error>,
    pub unknown: bool,
    pub original_worker_joined: bool,
    pub native_retirement_complete: bool,
    pub disconnect: Option<std::result::Result<(), JobError>>,
    pub maintenance: Option<std::result::Result<(), JobError>>,
    pub snapshot: Option<fs_watch_worker::Snapshot>,
    pub usage: Usage,
    pub deliveries: Vec<DeliveryReceipt>,
}
#[derive(Clone, Copy, Debug)]
pub struct WatchSnapshot {
    pub task: Snapshot,
    pub progress: Option<fs_watch_worker::Snapshot>,
    pub ready: bool,
    pub delivered: bool,
}
pub(super) struct WatchTask {
    key: TaskKey,
    _stop: StopRegistration,
    result: Option<WatchResult>,
    // Retain the actual proof and original zeroizing guest frames through review.
    retirement: Option<NativeRetirement>,
    _frames: Vec<Delivery>,
    delivered: bool,
}
impl WatchTask {
    pub(super) fn record_exit(&mut self, exit: fs_watch_worker::Exit<WorkbenchState>) {
        self.result = Some(WatchResult {
            outcome: exit.result,
            unknown: exit.unknown,
            original_worker_joined: true,
            native_retirement_complete: exit.native_retirement.is_complete(),
            disconnect: exit.disconnect,
            maintenance: exit.maintenance,
            snapshot: Some(exit.snapshot),
            usage: exit.usage,
            deliveries: exit
                .deliveries
                .iter()
                .map(|d| DeliveryReceipt {
                    sequence: d.sequence,
                    digest: d.digest,
                    execution: d.execution.clone(),
                })
                .collect(),
        });
        self.retirement = Some(exit.native_retirement);
        self._frames = exit.deliveries;
    }
    pub(super) fn check_acknowledgement(&self) -> Result<()> {
        let result = self
            .result
            .as_ref()
            .ok_or(AccessError::UnacknowledgedTask)?;
        if !self.delivered {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if result.unknown
            || !result.native_retirement_complete
            || (result.original_worker_joined
                && !self
                    .retirement
                    .as_ref()
                    .is_some_and(NativeRetirement::is_complete))
        {
            return Err(AccessError::RecoveryRequired.into());
        }
        Ok(())
    }
}
impl StateSlot {
    pub(crate) fn require_watch_resolution(&self) -> Result<()> {
        if self.watch_task.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        Ok(())
    }
    fn check_watch(&mut self, key: TaskKey) -> Result<()> {
        self.checked_task(key)?;
        if !self.watch_task.as_ref().is_some_and(|w| w.key == key) {
            return Err(AccessError::StaleTask.into());
        }
        Ok(())
    }
}
impl Workbench {
    pub fn start_fs_watch(&mut self, options: WatchStart) -> Result<TaskKey> {
        self.state.try_reclaim()?;
        self.state.require_writable()?;
        if self.state.task.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if options.submission == [0; 32]
            || self.state.watch_submissions.contains(&options.submission)
            || self.state.watch_submissions.len() >= 1024
            || options.window_ms == 0
            || options.window_ms > 30_000
            || options.max_frames == 0
            || options.max_wire_bytes == 0
        {
            return Err("目录观察参数无效或提交已使用；不会自动重试。".into());
        }
        let nominated_path = DirectoryAbsolutePath::new(&options.nominated_path)?;
        let gate = self.state.product_gate.clone();
        gate.check()?;
        let mut key = [0; 32];
        getrandom::fill(&mut key)?;
        if key == [0; 32] {
            return Err("目录观察任务身份不可用。".into());
        }
        let key = TaskKey(key);
        let start = self.state.local()?.start;
        let time = now(start);
        let expires = time
            .checked_add(options.window_ms)
            .ok_or("观察期限溢出。")?;
        let state = self.state.local_mut()?;
        let manager = state.manager.as_mut().ok_or("插件管理不可用。")?;
        let selection = manager
            .selection(&options.package_id)
            .ok_or("插件未选择。")?;
        if selection.digest != options.digest
            || manager.revision() != options.revision
            || !selection.enabled
            || !selection.approved_fs_watch
            || !selection.approved_io.contains(&IoCapability::FileList)
        {
            return Err("目录观察批准已变化，请重新核对。".into());
        }
        let package = manager.installed_package(options.digest)?;
        let declaration = package
            .fs_watch_declaration()
            .ok_or("插件未声明目录观察。")?;
        if !declaration.handlers.contains(&options.handler)
            || options.max_frames > declaration.max_frames
            || options.max_wire_bytes > declaration.max_wire_bytes
            || options.window_ms > declaration.max_window_ms
        {
            return Err("目录观察超出插件声明的范围。".into());
        }
        state.host.prepare_write()?;
        let instance = manager.connect(&options.package_id, &mut state.host)?;
        let approval = (|| -> Result<WatchApproval> {
            let binding = manager.bind_io(
                &state.host,
                &instance,
                options.digest,
                options.revision,
                &BTreeSet::from([IoCapability::FileList]),
                expires,
                time,
            )?;
            Ok(WatchApproval::issue(
                manager,
                &state.host,
                &instance,
                &binding,
                ApprovalRequest {
                    expected_revision: options.revision,
                    handler: options.handler,
                    nominated_path,
                    max_frames: options.max_frames,
                    max_wire_bytes: options.max_wire_bytes,
                    window_ms: options.window_ms,
                },
                time,
            )?
            .require_native_confirmation())
        })();
        let approval = match approval {
            Ok(value) => value,
            Err(error) => {
                self.state.cleanup_instance(instance)?;
                return Err(error);
            }
        };
        let revoke = approval.revocation();
        let stop = match gate.register_scoped(Arc::new(move || revoke.revoke())) {
            Ok(value) => value,
            Err(error) => {
                self.state.cleanup_instance(instance)?;
                return Err(error);
            }
        };
        if let Err(error) = gate.check() {
            self.state.cleanup_instance(instance)?;
            return Err(error);
        }
        // Consume once before dispatch, including a returned spawn refusal.
        self.state.watch_submissions.insert(options.submission);
        let owner = self
            .state
            .owner
            .take()
            .ok_or(AccessError::OwnerUnavailable)?;
        self.state.submission = Some(options.submission);
        let mut record = WatchTask {
            key,
            _stop: stop,
            result: None,
            retirement: None,
            _frames: vec![],
            delivered: false,
        };
        let mut task = Task {
            key,
            commands: Default::default(),
            worker: None,
            handle: None,
            stopping: false,
            exit: None,
            service: None,
            file: None,
            directory: None,
            mutation: None,
        };
        match WatchWorker::spawn_owned(owner, instance, approval, move || now(start)) {
            Ok(worker) => task.worker = Some(Executor::Watch(Box::new(worker))),
            Err(failure) => {
                self.state.owner = Some(failure.owner);
                self.state.cleanup = Some(Cleanup::Original(Box::new(failure.instance)));
                self.state.repair_needed = true;
                record.result = Some(WatchResult {
                    outcome: Err(failure.error),
                    unknown: false,
                    original_worker_joined: false,
                    native_retirement_complete: true,
                    disconnect: None,
                    maintenance: None,
                    snapshot: None,
                    usage: failure.approval.usage(),
                    deliveries: vec![],
                });
                task.exit = Some(ExitStatus {
                    execution: Err(JobError::Spawn),
                    disconnect: Err(JobError::Disconnect),
                    maintenance: Err(JobError::Unavailable),
                });
            }
        }
        self.state.watch_task = Some(record);
        self.state.task = Some(task);
        if gate.closed() {
            self.state.request_stop();
        }
        Ok(key)
    }
    pub fn poll_fs_watch(&mut self, key: TaskKey) -> Result<WatchSnapshot> {
        self.state.check_watch(key)?;
        self.state.try_reclaim()?;
        let task = self.state.snapshot();
        let record = self
            .state
            .watch_task
            .as_ref()
            .ok_or(AccessError::StaleTask)?;
        let progress = match self.state.task.as_ref().and_then(|t| t.worker.as_ref()) {
            Some(Executor::Watch(worker)) => Some(worker.snapshot()),
            _ => record.result.as_ref().and_then(|r| r.snapshot),
        };
        Ok(WatchSnapshot {
            task,
            progress,
            ready: record.result.is_some() && !record.delivered,
            delivered: record.delivered,
        })
    }
    /// One review delivery after the actual join. Original event bytes and proof
    /// remain in this task until explicit acknowledgement; no authority escapes.
    pub fn read_fs_watch(&mut self, key: TaskKey) -> Result<Option<WatchResult>> {
        self.poll_fs_watch(key)?;
        let gate = self.state.product_gate.clone();
        gate.check()?;
        let record = self
            .state
            .watch_task
            .as_mut()
            .ok_or(AccessError::StaleTask)?;
        if record.delivered {
            return Err(AccessError::StaleTask.into());
        }
        let Some(result) = record.result.clone() else {
            return Ok(None);
        };
        gate.check()?;
        record.delivered = true;
        Ok(Some(result))
    }
}
