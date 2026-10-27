//! Original background proposal, retained until one explicit trusted decision.
//! The worker owns only a package and task bytes: no Store, Manager or live grant.
use crate::{
    Result, Workbench,
    io_tasks::{AccessError, TaskKey, blob_upload::UploadStart},
    product_gate::{ProductGate, StopRegistration},
};
use morrow_blob_transfer_v1::{Identity, Limits as FrameLimits};
use morrow_blob_upload_proposal_v1::{BOUND_INPUT_TYPE, BoundInput};
use morrow_core::task::{Invocation, PluginFailure, Transform};
use morrow_plugin_runtime::{
    Cancellation, Limits, Report,
    blob_upload::Spec,
    package::{PreparedPackage, ProposalReport},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanKey([u8; 32]);
impl PlanKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let value: [u8; 32] = bytes.try_into()?;
        if value == [0; 32] {
            return Err("invalid planning identity".into());
        }
        Ok(Self(value))
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
pub struct PlanStart {
    pub submission: [u8; 32],
    pub package_id: String,
    pub digest: [u8; 32],
    pub revision: u64,
    pub input: Invocation,
    pub lifetime: Duration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    Cancelled,
    Expired,
    StaleSelection,
    GateClosed,
    Protocol,
    Execution,
    PluginFailure,
    Preparation,
    Spawn,
    Worker,
}
#[derive(Clone, Debug)]
pub struct PlanView {
    pub package_id: String,
    pub package_digest: [u8; 32],
    pub registry_revision: u64,
    pub planner_handler: String,
    pub upload_handler: String,
    pub planning_input_sha256: [u8; 32],
    /// Untrusted plain text, displayed separately from original package attribution.
    pub label: String,
    pub byte_length: u64,
    pub whole_sha256: [u8; 32],
    pub proposal_sha256: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct PlanResult {
    pub view: Option<PlanView>,
    pub error: Option<PlanError>,
    /// Original execution evidence; a later delivery veto does not rewrite it.
    pub execution: Option<Report>,
    pub failure: Option<PluginFailure>,
}
#[derive(Clone, Copy, Debug)]
pub struct PlanSnapshot {
    pub key: PlanKey,
    pub submission: [u8; 32],
    pub running: bool,
    pub original_worker_joined: bool,
    pub ready: bool,
    pub delivered: bool,
    pub consumed: bool,
    pub error: Option<PlanError>,
    pub upload_key: Option<TaskKey>,
    pub upload_submission: Option<[u8; 32]>,
    /// An attempted submission is never automatically repeated after a failure.
    pub submission_failed: bool,
}
pub struct PlanApproval {
    pub key: PlanKey,
    pub package_digest: [u8; 32],
    pub registry_revision: u64,
    pub proposal_sha256: [u8; 32],
}
struct Attempt {
    key: PlanKey,
    submission: [u8; 32],
    package_id: String,
    digest: [u8; 32],
    revision: u64,
    planner_handler: String,
    upload_handler: String,
    upload_output_type: String,
    input_sha256: [u8; 32],
    deadline: Instant,
    cancel: Cancellation,
    cancelled: bool,
    gate: ProductGate,
    _stop: StopRegistration,
    join: Option<JoinHandle<std::result::Result<ProposalReport, PlanError>>>,
    joined: bool,
    report: Option<ProposalReport>,
    error: Option<PlanError>,
    delivered: bool,
    consumed: bool,
    upload_key: Option<TaskKey>,
    upload_submission: Option<[u8; 32]>,
    submission_failed: bool,
    submitted_spec: Option<Spec>,
}
impl Attempt {
    fn collect(&mut self) {
        if self.join.as_ref().is_some_and(JoinHandle::is_finished) {
            let joined = self.join.take().expect("finished original worker").join();
            self.joined = true;
            match joined {
                Ok(Ok(report)) => {
                    if report.proposal.is_none() {
                        self.error = Some(if report.failure.is_some() {
                            PlanError::PluginFailure
                        } else if report.execution.outcome.is_err() {
                            PlanError::Execution
                        } else {
                            PlanError::Protocol
                        });
                    }
                    self.report = Some(report);
                }
                Ok(Err(error)) => self.error = Some(error),
                Err(_) => self.error = Some(PlanError::Worker),
            }
        }
    }
    fn veto(&mut self, selection_current: bool, current_gate_open: bool) {
        // After consuming an approval this record only discovers the original
        // upload. Its owner/result has its own cancellation and recovery policy.
        if self.consumed {
            return;
        }
        let error = if self.cancelled {
            Some(PlanError::Cancelled)
        } else if self.gate.closed() || !current_gate_open {
            Some(PlanError::GateClosed)
        } else if Instant::now() >= self.deadline {
            Some(PlanError::Expired)
        } else if !selection_current {
            Some(PlanError::StaleSelection)
        } else {
            None
        };
        if let Some(error) = error {
            self.cancel.cancel();
            self.error = Some(error);
            // Retain the original report; it remains historical data, not an approval.
        }
    }
    fn snapshot(&self) -> PlanSnapshot {
        PlanSnapshot {
            key: self.key,
            submission: self.submission,
            running: self.join.is_some(),
            original_worker_joined: self.joined,
            ready: self.join.is_none() && !self.delivered,
            delivered: self.delivered,
            consumed: self.consumed,
            error: self.error,
            upload_key: self.upload_key,
            upload_submission: self.upload_submission,
            submission_failed: self.submission_failed,
        }
    }
    fn view(&self) -> Option<PlanView> {
        if self.error.is_some() || self.consumed {
            return None;
        }
        let p = self.report.as_ref()?.proposal.as_ref()?;
        Some(PlanView {
            package_id: self.package_id.clone(),
            package_digest: self.digest,
            registry_revision: self.revision,
            planner_handler: self.planner_handler.clone(),
            upload_handler: self.upload_handler.clone(),
            planning_input_sha256: self.input_sha256,
            label: p.label().to_owned(),
            byte_length: p.byte_length(),
            whole_sha256: p.whole_sha256(),
            proposal_sha256: Sha256::digest(p.wire()).into(),
        })
    }
}
#[derive(Default)]
pub(crate) struct Plans {
    attempt: Option<Attempt>,
    submissions: BTreeSet<[u8; 32]>,
}
impl Plans {
    pub(crate) fn cancel(&mut self) {
        if let Some(p) = &mut self.attempt {
            p.cancelled = true;
            p.cancel.cancel();
        }
    }
    pub(crate) fn finish(&mut self) -> Result<()> {
        self.cancel();
        if let Some(p) = &mut self.attempt {
            p.collect();
            if p.join.is_some() {
                return Err(AccessError::Busy.into());
            }
        }
        Ok(())
    }
    fn checked(&mut self, key: PlanKey) -> Result<&mut Attempt> {
        self.attempt
            .as_mut()
            .filter(|p| p.key == key)
            .ok_or_else(|| AccessError::StaleTask.into())
    }
}
impl Drop for Plans {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn fresh_identity() -> Result<[u8; 32]> {
    let mut value = [0; 32];
    getrandom::fill(&mut value)?;
    if value == [0; 32] {
        return Err("planning identity unavailable".into());
    }
    Ok(value)
}
fn limits() -> Limits {
    Limits {
        fuel: morrow_plugin_runtime::MAX_BLOB_UPLOAD_FUEL,
        host_calls: 1024,
        ..Limits::default()
    }
}
impl Workbench {
    pub fn start_blob_upload_plan(&mut self, options: PlanStart) -> Result<PlanKey> {
        self.state.require_writable()?;
        if self.upload_plans.attempt.is_some() || self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if options.submission == [0; 32]
            || self.upload_plans.submissions.contains(&options.submission)
            || self.upload_plans.submissions.len() >= 1024
            || options.lifetime.is_zero()
            || options.lifetime > Duration::from_secs(120)
        {
            return Err("invalid or reused planning submission".into());
        }
        let state = self.state.local()?;
        let manager = state.manager.as_ref().ok_or("plugin manager unavailable")?;
        let selection = manager
            .selection(&options.package_id)
            .ok_or("plugin not selected")?;
        if manager.revision() != options.revision
            || selection.digest != options.digest
            || !selection.enabled
        {
            return Err("plugin selection changed".into());
        }
        let package = manager.installed_package(options.digest)?;
        let transform = options
            .input
            .transform()
            .ok_or("planning transform required")?;
        let (_, mapping) = package.blob_upload_proposal_handler(transform)?;
        let upload_handler = mapping.upload_handler.clone();
        let upload_output_type = package
            .manifest()
            .transform_handlers
            .iter()
            .find(|h| h.handler == upload_handler)
            .ok_or("upload registration missing")?
            .output_type
            .clone();
        let planner_handler = transform.handler.clone();
        // Bind the bytes the caller supplied, without reconstructing the host task envelope.
        let input_sha256 = Sha256::digest(&transform.input).into();
        let deadline = Instant::now()
            .checked_add(options.lifetime)
            .ok_or("planning deadline overflow")?;
        let cancel = Cancellation::until(deadline);
        let gate = self.state.product_gate.clone();
        let stop_cancel = cancel.clone();
        let stop = gate.register_scoped(Arc::new(move || stop_cancel.cancel()))?;
        let key = PlanKey(fresh_identity()?);
        let mut attempt = Attempt {
            key,
            submission: options.submission,
            package_id: options.package_id,
            digest: options.digest,
            revision: options.revision,
            planner_handler,
            upload_handler,
            upload_output_type,
            input_sha256,
            deadline,
            cancel: cancel.clone(),
            cancelled: false,
            gate,
            _stop: stop,
            join: None,
            joined: false,
            report: None,
            error: None,
            delivered: false,
            consumed: false,
            upload_key: None,
            upload_submission: None,
            submission_failed: false,
            submitted_spec: None,
        };
        self.upload_plans.submissions.insert(options.submission);
        #[cfg(test)]
        let pause = HOLD_WORKER.take();
        // Preparation and the actual interpreter both run away from the UI lane.
        // No authoritative host state is captured by this closure.
        match spawn(move || {
            #[cfg(test)]
            if let Some(pause) = pause {
                let _ = pause.recv();
            }
            catch_unwind(AssertUnwindSafe(|| {
                let prepared =
                    PreparedPackage::new(package, limits()).map_err(|_| PlanError::Preparation)?;
                Ok(prepared.run_blob_upload_proposal(&options.input, cancel))
            }))
            .unwrap_or(Err(PlanError::Worker))
        }) {
            Ok(join) => attempt.join = Some(join),
            Err(_) => attempt.error = Some(PlanError::Spawn),
        }
        self.upload_plans.attempt = Some(attempt);
        Ok(key)
    }
    pub fn poll_blob_upload_plan(&mut self, key: PlanKey) -> Result<PlanSnapshot> {
        let p = self
            .upload_plans
            .attempt
            .as_ref()
            .filter(|p| p.key == key)
            .ok_or(AccessError::StaleTask)?;
        // A busy original Store does not prove registry drift. It does prevent
        // approval until the exact owner returns, when this check is repeated.
        let current = self
            .state
            .local()
            .ok()
            .and_then(|s| s.manager.as_ref())
            .map(|m| {
                m.revision() == p.revision
                    && m.selection(&p.package_id)
                        .is_some_and(|s| s.digest == p.digest && s.enabled)
            })
            .unwrap_or(true);
        let gate_open =
            !self.state.product_gate.closed() && self.state.product_gate.same_gate(&p.gate);
        let p = self.upload_plans.checked(key)?;
        p.collect();
        p.veto(current, gate_open);
        Ok(p.snapshot())
    }
    /// Missing start/approval replies can discover only this original record.
    pub fn discover_blob_upload_plan(
        &mut self,
        submission: [u8; 32],
    ) -> Result<Option<PlanSnapshot>> {
        let key = self
            .upload_plans
            .attempt
            .as_ref()
            .filter(|p| p.submission == submission)
            .map(|p| p.key);
        key.map(|key| self.poll_blob_upload_plan(key)).transpose()
    }
    pub fn read_blob_upload_plan(&mut self, key: PlanKey) -> Result<Option<PlanResult>> {
        let snapshot = self.poll_blob_upload_plan(key)?;
        if !snapshot.ready {
            return Ok(None);
        }
        let p = self.upload_plans.checked(key)?;
        let view = p.view();
        p.delivered = true;
        Ok(Some(PlanResult {
            view,
            error: p.error,
            execution: p.report.as_ref().map(|r| r.execution.clone()),
            failure: p.report.as_ref().and_then(|r| r.failure.clone()),
        }))
    }
    pub fn cancel_blob_upload_plan(&mut self, key: PlanKey) -> Result<PlanSnapshot> {
        let p = self.upload_plans.checked(key)?;
        if p.consumed {
            return Err("planning approval already consumed; use original upload controls".into());
        }
        p.cancelled = true;
        p.cancel.cancel();
        self.poll_blob_upload_plan(key)
    }
    pub fn acknowledge_blob_upload_plan(&mut self, key: PlanKey) -> Result<()> {
        let snapshot = self.poll_blob_upload_plan(key)?;
        if snapshot.running || !snapshot.delivered || self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        self.upload_plans.attempt = None;
        Ok(())
    }
    /// Called only by the trusted product after the user reviews this exact
    /// proposal. Metadata and hashes cannot themselves prove that review occurred.
    pub fn approve_blob_upload_plan(&mut self, approval: PlanApproval) -> Result<TaskKey> {
        self.state.require_writable()?;
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        self.poll_blob_upload_plan(approval.key)?;
        let p = self.upload_plans.checked(approval.key)?;
        if p.join.is_some() || !p.delivered || p.consumed || p.error.is_some() {
            return Err("planning result is not eligible for approval".into());
        }
        let view = p.view().ok_or("no original proposal")?;
        if approval.package_digest != p.digest
            || approval.registry_revision != p.revision
            || approval.proposal_sha256 != view.proposal_sha256
        {
            return Err("reviewed proposal does not match the original plan".into());
        }
        let state = self.state.local()?;
        let manager = state.manager.as_ref().ok_or("plugin manager unavailable")?;
        if manager.revision() != p.revision
            || !manager
                .selection(&p.package_id)
                .is_some_and(|s| s.digest == p.digest && s.enabled && s.approved_blob_upload)
        {
            return Err("upload category approval or plugin selection changed".into());
        }
        let proposal = p
            .report
            .as_ref()
            .and_then(|r| r.proposal.as_ref())
            .ok_or("original proposal absent")?;
        let identity = Identity {
            transfer_epoch: fresh_identity()?,
            object_ref: fresh_identity()?,
            operation_id: fresh_identity()?,
        };
        let spec = Spec {
            owner_domain: fresh_identity()?,
            identity,
            byte_length: proposal.byte_length(),
            sha256: proposal.whole_sha256(),
            limits: FrameLimits::default(),
        };
        let bound = BoundInput::new(proposal, identity).map_err(|_| "bound input rejected")?;
        let submission = fresh_identity()?;
        let task_id: String = submission.iter().map(|b| format!("{b:02x}")).collect();
        let input = Invocation::new_transform(
            &task_id,
            Transform {
                handler: p.upload_handler.clone(),
                input_type: BOUND_INPUT_TYPE.into(),
                output_type: p.upload_output_type.clone(),
                input: bound.wire().to_vec(),
            },
        )?;
        let remaining = p.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || p.gate.closed() {
            return Err("planning review expired or controller lost".into());
        }
        let options = UploadStart {
            submission,
            package_id: p.package_id.clone(),
            digest: p.digest,
            revision: p.revision,
            nomination_ref: identity.transfer_epoch,
            spec: spec.clone(),
            input,
            lifetime: remaining,
            limits: limits(),
        };
        // Commit the local one-shot decision before admission can create effects.
        p.consumed = true;
        p.upload_submission = Some(submission);
        p.submitted_spec = Some(spec);
        let result = self.start_blob_upload(options);
        let p = self.upload_plans.checked(approval.key)?;
        match result {
            Ok(key) => {
                p.upload_key = Some(key);
                Ok(key)
            }
            Err(error) => {
                p.submission_failed = true;
                Err(error)
            }
        }
    }
}
fn spawn(
    run: impl FnOnce() -> std::result::Result<ProposalReport, PlanError> + Send + 'static,
) -> std::io::Result<JoinHandle<std::result::Result<ProposalReport, PlanError>>> {
    #[cfg(test)]
    if FAIL_SPAWN.replace(false) {
        return Err(std::io::Error::other("injected planner spawn failure"));
    }
    thread::Builder::new()
        .name("morrow-upload-plan".into())
        .spawn(run)
}
#[cfg(test)]
thread_local! {static FAIL_SPAWN:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
#[cfg(test)]
thread_local! {static HOLD_WORKER:std::cell::RefCell<Option<std::sync::mpsc::Receiver<()>>>=const{std::cell::RefCell::new(None)};}

#[cfg(all(test, windows, feature = "blob-upload-guest-qualification"))]
#[path = "blob_upload_plans_tests.rs"]
mod tests;
