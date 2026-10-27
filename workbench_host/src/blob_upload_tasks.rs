//! Trusted native product carrier. The entire original protected WorkbenchState
//! moves into UploadWorker. This API does not establish rendered approval UI.
use super::{AccessError, Cleanup, Executor, ExitStatus, Snapshot, Task, TaskKey};
use crate::{Result, Workbench, product_gate::StopRegistration};
use morrow_blob_upload_request_v1::Phase;
use morrow_core::{dispatch::HostBinding, store::BlobStageStoreBinding, task::Invocation};
use morrow_plugin_runtime::{
    Limits, Report,
    blob_upload::Spec,
    blob_upload_binding::{ApprovalRequest, UploadApproval, Usage},
    blob_upload_worker::{Error, GuestResult, UploadControl, UploadWorker},
    io_jobs::JobError,
};
use std::{sync::Arc, time::Duration};

/// Trusted application input after explicit operation approval. Names, hashes
/// and package declarations are correlation data, never permission by themselves.
/// No caller may reconstruct this from a previous Unknown task to replay it.
pub struct UploadStart {
    pub submission: [u8; 32],
    pub package_id: String,
    pub digest: [u8; 32],
    pub revision: u64,
    pub nomination_ref: [u8; 32],
    pub spec: Spec,
    pub input: Invocation,
    pub lifetime: Duration,
    pub limits: Limits,
}
#[derive(Clone, Debug)]
pub struct UploadResult {
    pub outcome: std::result::Result<Phase, Error>,
    pub unknown: bool,
    pub usage: Usage,
    pub execution: Option<Report>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observation {
    Absent,
    Staged,
    Released,
}
#[derive(Clone, Copy, Debug)]
pub struct UploadSnapshot {
    pub task: Snapshot,
    pub ready: bool,
    pub delivered: bool,
    pub recovery_required: bool,
    /// A separate historical observation; never rewrites the original outcome.
    pub observed: Option<Observation>,
}
pub(super) struct UploadTask {
    key: TaskKey,
    control: Option<UploadControl>,
    _stop: StopRegistration,
    spec: Spec,
    host: HostBinding,
    store: BlobStageStoreBinding,
    result: Option<UploadResult>,
    delivered: bool,
    observed: Option<Observation>,
    recovery_confirmed: bool,
}
impl UploadTask {
    pub(super) fn cancel(&self) {
        if let Some(control) = &self.control {
            control.cancel();
        }
    }
    pub(super) fn record_exit(
        &mut self,
        result: std::result::Result<GuestResult, Error>,
        usage: Usage,
        execution: Option<Report>,
        unknown: bool,
    ) {
        self.result = Some(UploadResult {
            outcome: result.map(|r| r.phase),
            usage,
            execution,
            unknown,
        });
    }
    fn refresh_delivery(&mut self) {
        if self.delivered {
            return;
        }
        if let Some(error) = self
            .control
            .as_ref()
            .and_then(UploadControl::delivery_error)
            && let Some(result) = &mut self.result
            && let Ok(phase) = result.outcome
        {
            result.unknown |= matches!(phase, Phase::Staged | Phase::Released);
            result.outcome = Err(error);
        }
    }
    pub(super) fn needs_review(&self) -> bool {
        if self.recovery_confirmed {
            return false;
        }
        self.result.as_ref().is_some_and(|r| {
            r.unknown
                || (!self.delivered
                    && r.outcome
                        .is_ok_and(|p| matches!(p, Phase::Staged | Phase::Released))
                    && self
                        .control
                        .as_ref()
                        .is_some_and(|c| c.delivery_error().is_some()))
        })
    }
    pub(super) fn check_acknowledgement(&self) -> Result<()> {
        if !self.delivered || self.result.is_none() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if self.needs_review() {
            return Err(AccessError::RecoveryRequired.into());
        }
        Ok(())
    }
}
impl Workbench {
    pub fn start_blob_upload(&mut self, options: UploadStart) -> Result<TaskKey> {
        self.state.try_reclaim()?;
        self.state.require_writable()?;
        if self.state.task.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if options.submission == [0; 32]
            || options.nomination_ref == [0; 32]
            || self.state.upload_submissions.contains(&options.submission)
            || self.state.upload_submissions.len() >= 1024
            || options.lifetime.is_zero()
            || options.lifetime > Duration::from_secs(120)
        {
            return Err("上传任务参数无效或提交已使用；不会自动重试。".into());
        }
        options
            .spec
            .validate()
            .map_err(|_| "上传对象超出批准范围。")?;
        let gate = self.state.product_gate.clone();
        gate.check()?;
        let mut raw_key = [0; 32];
        getrandom::fill(&mut raw_key)?;
        if raw_key == [0; 32] {
            return Err("上传任务身份不可用。".into());
        }
        let key = TaskKey(raw_key);
        // Finish fallible host-clock acquisition before connecting or moving the owner.
        let now = crate::platform::unix_millis()?;
        let state = self.state.local_mut()?;
        let manager = state.manager.as_mut().ok_or("插件管理不可用。")?;
        let selection = manager
            .selection(&options.package_id)
            .ok_or("插件未选择。")?;
        if selection.digest != options.digest
            || manager.revision() != options.revision
            || !selection.enabled
            || !selection.approved_blob_upload
        {
            return Err("上传批准已变化，请重新核对。".into());
        }
        let package = manager.installed_package(options.digest)?;
        package.blob_upload_handler(options.input.transform().ok_or("上传输入类型不正确。")?)?;
        state.host.prepare_write()?;
        let instance =
            manager.connect_blob_upload(&options.package_id, &mut state.host, options.limits)?;
        let approval = UploadApproval::issue(
            manager,
            &state.host,
            &instance,
            ApprovalRequest {
                expected_revision: options.revision,
                nomination_ref: options.nomination_ref,
                spec: options.spec.clone(),
                lifetime: options.lifetime,
            },
        );
        let approval = match approval {
            Ok(approval) => approval,
            Err(error) => {
                self.state.cleanup_instance(instance)?;
                return Err(format!("上传批准未建立：{error:?}").into());
            }
        };
        let revoke = approval.revocation();
        let stop = match gate.register_scoped(Arc::new(move || revoke.revoke())) {
            Ok(stop) => stop,
            Err(error) => {
                self.state.cleanup_instance(instance)?;
                return Err(error);
            }
        };
        if let Err(error) = gate.check() {
            self.state.cleanup_instance(instance)?;
            return Err(error);
        }
        let owner = self
            .state
            .owner
            .take()
            .ok_or(AccessError::OwnerUnavailable)?;
        let original_host = owner.host.binding();
        let original_store = owner.host.store_local().blob_stage_store_binding();
        self.state.upload_submissions.insert(options.submission);
        self.state.submission = Some(options.submission);
        let mut record = UploadTask {
            key,
            control: None,
            _stop: stop,
            spec: options.spec,
            host: original_host,
            store: original_store,
            result: None,
            delivered: false,
            observed: None,
            recovery_confirmed: false,
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
            #[cfg(windows)]
            directory: None,
            #[cfg(windows)]
            mutation: None,
        };
        match UploadWorker::spawn_owned(owner, instance, approval, options.input, now) {
            Ok(worker) => {
                record.control = Some(worker.control());
                task.worker = Some(Executor::Upload(Box::new(worker)));
            }
            Err(failure) => {
                self.state.owner = Some(failure.owner);
                self.state.cleanup = Some(Cleanup::Original(failure.instance));
                self.state.repair_needed = true;
                record.result = Some(UploadResult {
                    outcome: Err(failure.error),
                    unknown: false,
                    usage: failure.approval.usage(),
                    execution: None,
                });
                task.exit = Some(ExitStatus {
                    execution: Err(JobError::Spawn),
                    disconnect: Err(JobError::Disconnect),
                    maintenance: Ok(()),
                });
            }
        }
        self.state.task = Some(task);
        self.state.upload_task = Some(record);
        if gate.closed() {
            self.state.request_stop();
        }
        Ok(key)
    }
    pub fn poll_blob_upload(&mut self, key: TaskKey) -> Result<UploadSnapshot> {
        self.state.checked_task(key)?;
        self.state.try_reclaim()?;
        let task = self.state.snapshot();
        let upload = self
            .state
            .upload_task
            .as_mut()
            .filter(|u| u.key == key)
            .ok_or(AccessError::StaleTask)?;
        upload.refresh_delivery();
        Ok(UploadSnapshot {
            task,
            ready: upload.result.is_some() && !upload.delivered,
            delivered: upload.delivered,
            recovery_required: upload.needs_review(),
            observed: upload.observed,
        })
    }
    /// Exactly one result delivery after the original join; no Store handle or
    /// renewed upload authority escapes. Recovery observations are separate.
    pub fn read_blob_upload(&mut self, key: TaskKey) -> Result<Option<UploadResult>> {
        self.poll_blob_upload(key)?;
        let gate = self.state.product_gate.clone();
        gate.check()?;
        let upload = self
            .state
            .upload_task
            .as_mut()
            .ok_or(AccessError::StaleTask)?;
        if upload.delivered {
            return Err(AccessError::StaleTask.into());
        }
        upload.refresh_delivery();
        let Some(result) = upload.result.clone() else {
            return Ok(None);
        };
        gate.check()?;
        upload.delivered = true;
        Ok(Some(result))
    }
    /// Explicit read-only recovery on the original returned protected Store.
    /// Never constructs a new worker, approval, operation ID or replacement Store.
    pub fn inspect_blob_upload_history(&mut self, key: TaskKey) -> Result<Observation> {
        self.state.checked_task(key)?;
        self.state.try_reclaim()?;
        let gate = self.state.product_gate.clone();
        gate.check()?;
        let upload = self
            .state
            .upload_task
            .as_ref()
            .filter(|u| u.key == key)
            .ok_or(AccessError::StaleTask)?;
        if upload.result.is_none() {
            return Err(AccessError::Busy.into());
        }
        let owner = self.state.local()?;
        if owner.host.binding() != upload.host {
            return Err(AccessError::OwnerUnavailable.into());
        }
        owner
            .host
            .store_local()
            .validate_blob_stage_store_binding(&upload.store)?;
        let record = owner
            .host
            .store_local()
            .inspect_blob_stage_local_authorized(&upload.spec.command(), || {
                gate.check()
                    .map_err(|_| morrow_core::Error::Invalid("upload recovery gate closed"))
            })?;
        gate.check()?;
        let observed = match record {
            None => Observation::Absent,
            Some(record) if record.is_released() => Observation::Released,
            Some(_) => Observation::Staged,
        };
        self.state
            .upload_task
            .as_mut()
            .ok_or(AccessError::StaleTask)?
            .observed = Some(observed);
        Ok(observed)
    }
    /// Explicit confirmation after viewing original history. Preserve the
    /// original Unknown result; only allow acknowledgement of this closed task.
    pub fn confirm_blob_upload_recovery(
        &mut self,
        key: TaskKey,
        expected: Observation,
    ) -> Result<()> {
        let upload = self
            .state
            .upload_task
            .as_ref()
            .filter(|u| u.key == key)
            .ok_or(AccessError::StaleTask)?;
        if !upload.delivered || upload.observed != Some(expected) {
            return Err(AccessError::RecoveryRequired.into());
        }
        if self.inspect_blob_upload_history(key)? != expected {
            return Err("上传记录已变化，请再次核对。".into());
        }
        self.state
            .upload_task
            .as_mut()
            .ok_or(AccessError::StaleTask)?
            .recovery_confirmed = true;
        Ok(())
    }
}
