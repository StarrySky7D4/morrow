//! Trusted native application mutation tasks. No guest ABI or UI path grants.
use super::{AccessError, BindingProfile, Executor, StartOptions, TaskKey, file};
use crate::{Result, Workbench, WorkbenchState};
use morrow_core::{
    file_mutation::{Disposition, RequestRecord},
    file_path::RelativeFilePath,
    io_intent::Phase,
    store::FileMutationPlanCheckpoint,
};
use morrow_plugin_runtime::{
    file_target::{Error as TargetError, SelectionScope},
    io_jobs::{
        IoWorker, MutationDiscoverySession, MutationHandle, MutationResponse, MutationSession,
        OwnerCommandError, OwnerCommandPoll, Poll,
    },
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf, time::Duration};
use zeroize::Zeroizing;

#[path = "mutation_guest_tasks.rs"]
pub mod guest;

pub enum Selection {
    Existing(PathBuf),
    Create {
        root: PathBuf,
        relative: RelativeFilePath,
    },
}
pub enum Action {
    NextPlans {
        scan_limit: u16,
    },
    BuildPlan {
        operation_id: String,
        content_length: u64,
        content_sha256: Option<[u8; 32]>,
    },
    Prepare(Box<RequestRecord>),
    Chunk {
        offset: u64,
        bytes: Zeroizing<Vec<u8>>,
    },
    CommitContent,
    Execute,
    Query,
    CancelPlan,
    Release,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Select,
    Prepare,
    Chunk,
    CommitContent,
    Execute,
    Query,
    CancelPlan,
    Release,
    Reconcile,
    BuildPlan,
    Discover,
}
impl Action {
    fn kind(&self) -> CommandKind {
        match self {
            Self::NextPlans { .. } => CommandKind::Discover,
            Self::BuildPlan { .. } => CommandKind::BuildPlan,
            Self::Prepare(_) => CommandKind::Prepare,
            Self::Chunk { .. } => CommandKind::Chunk,
            Self::CommitContent => CommandKind::CommitContent,
            Self::Execute => CommandKind::Execute,
            Self::Query => CommandKind::Query,
            Self::CancelPlan => CommandKind::CancelPlan,
            Self::Release => CommandKind::Release,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Delivery(OwnerCommandError),
    Target(TargetError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedTarget {
    pub reference: [u8; 32],
    pub expected_identity: Option<[u8; 32]>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MutationSnapshot {
    /// One-based identity within the exact TaskKey; never reused after read.
    pub command: u64,
    pub kind: CommandKind,
    pub delivery: OwnerCommandPoll,
    pub selected: bool,
    /// Local selection metadata, not renewed permission or durable identity.
    pub selection: Option<SelectedTarget>,
    pub reconcile_required: bool,
    /// A known terminal plan, or completed native discovery cursor.
    /// Neither state grants authority to dispatch a filesystem effect.
    pub terminal: bool,
}
pub(super) enum AdmissionKind {
    Selected {
        selection: Selection,
        scope: SelectionScope,
        secret: [u8; 32],
    },
    GuestSelected {
        selection: Selection,
        scope: SelectionScope,
        secret: [u8; 32],
    },
    Reconcile(RequestRecord),
    Discover {
        subject: String,
        disposition: Disposition,
        scan_limit: u16,
        checkpoint: Option<FileMutationPlanCheckpoint>,
    },
}
pub(super) struct Admission {
    kind: AdmissionKind,
    pub(super) timeout: Duration,
}
impl Admission {
    pub(super) fn submit(self, worker: &IoWorker<WorkbenchState>) -> Result<MutationTask> {
        let (session, discovery, handle, kind, guest_mode) = match self.kind {
            AdmissionKind::Selected {
                selection,
                scope,
                secret,
            } => {
                let (session, handle) = match selection {
                    Selection::Existing(path) => {
                        worker.select_mutation_existing(path, scope, secret)?
                    }
                    Selection::Create { root, relative } => {
                        worker.select_mutation_create(root, relative, scope, secret)?
                    }
                };
                (Some(session), None, handle, CommandKind::Select, false)
            }
            AdmissionKind::GuestSelected {
                selection,
                scope,
                secret,
            } => {
                let (session, handle) = match selection {
                    Selection::Existing(path) => {
                        worker.select_mutation_existing(path, scope, secret)?
                    }
                    Selection::Create { root, relative } => {
                        worker.select_mutation_create(root, relative, scope, secret)?
                    }
                };
                (Some(session), None, handle, CommandKind::Select, true)
            }
            AdmissionKind::Reconcile(request) => (
                None,
                None,
                worker.reconcile_mutation(request)?,
                CommandKind::Reconcile,
                false,
            ),
            AdmissionKind::Discover {
                subject,
                disposition,
                scan_limit,
                checkpoint,
            } => {
                let (discovery, handle) = worker.open_mutation_discovery_from(
                    subject,
                    disposition,
                    scan_limit,
                    checkpoint,
                )?;
                (None, Some(discovery), handle, CommandKind::Discover, false)
            }
        };
        Ok(MutationTask {
            session,
            discovery,
            pending: Some(handle),
            command: 1,
            kind,
            selected: false,
            selection: None,
            reconcile_required: false,
            terminal: false,
            submissions: Default::default(),
            guest: guest_mode.then(|| guest::GuestState::new(self.timeout)),
        })
    }
}
pub(super) struct MutationTask {
    session: Option<MutationSession>,
    discovery: Option<MutationDiscoverySession>,
    pending: Option<MutationHandle>,
    command: u64,
    kind: CommandKind,
    selected: bool,
    selection: Option<SelectedTarget>,
    reconcile_required: bool,
    terminal: bool,
    submissions: std::collections::BTreeMap<[u8; 32], ([u8; 32], u64)>,
    guest: Option<guest::GuestState>,
}
impl MutationTask {
    fn snapshot(&self) -> MutationSnapshot {
        MutationSnapshot {
            command: self.command,
            kind: self.kind,
            delivery: self
                .pending
                .as_ref()
                .map_or(OwnerCommandPoll::Consumed, MutationHandle::poll),
            selected: self.selected,
            selection: self.selection,
            reconcile_required: self.reconcile_required,
            terminal: self.terminal,
        }
    }
    pub(super) fn poll(&mut self) -> Option<Poll> {
        if let Some(guest) = &mut self.guest {
            return guest.poll(self.pending.as_ref());
        }
        self.pending.as_ref().map(|h| match h.poll() {
            OwnerCommandPoll::Pending => Poll::Pending,
            OwnerCommandPoll::Ready => Poll::Ready,
            OwnerCommandPoll::Consumed => Poll::Consumed,
        })
    }
}
impl Workbench {
    /// A versioned write-budget declaration does not grant that budget to
    /// recovery. Only discovery/reconciliation admissions call this selector.
    fn mutation_history_profile(&mut self, digest: [u8; 32]) -> Result<BindingProfile> {
        let package = self
            .local_state()?
            .manager
            .as_ref()
            .ok_or("catalog unavailable")?
            .installed_package(digest)?;
        Ok(if package.mutation_budget().is_some() {
            BindingProfile::MutationHistory
        } else {
            BindingProfile::Ordinary
        })
    }
    /// Trusted caller supplies an explicit approved scope. This API never claims
    /// that an arbitrary path came from a native picker. No OS IO runs on admission.
    pub fn start_mutation(
        &mut self,
        options: StartOptions,
        selection: Selection,
        scope: SelectionScope,
    ) -> Result<TaskKey> {
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        self.local_state()?;
        if options.capabilities != BTreeSet::from([scope.disposition.capability()])
            || matches!(
                (&selection, scope.disposition),
                (Selection::Existing(_), Disposition::Create)
            )
            || matches!(
                (&selection, scope.disposition),
                (
                    Selection::Create { .. },
                    Disposition::Delete | Disposition::Replace
                )
            )
        {
            return Err("mutation selection and capability mismatch".into());
        }
        self.state.submission = None;
        let timeout = options.lifetime;
        self.start_task(options, move |_| {
            let mut secret = [0; 32];
            getrandom::fill(&mut secret)?;
            Ok(file::Admission::Mutation(Admission {
                kind: AdmissionKind::Selected {
                    selection,
                    scope,
                    secret,
                },
                timeout,
            }))
        })
    }
    /// Read an exact historical plan using a freshly admitted current authority.
    /// The previous task must be joined, repaired if necessary, and acknowledged.
    /// No path/selection is accepted or restored, and this task accepts no effects.
    pub fn start_mutation_reconciliation(
        &mut self,
        options: StartOptions,
        request: RequestRecord,
    ) -> Result<TaskKey> {
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        self.local_state()?;
        if options.digest != request.request().package_sha256
            || options.capabilities != BTreeSet::from([request.request().disposition.capability()])
        {
            return Err("mutation history authority mismatch".into());
        }
        let binding = self.mutation_history_profile(options.digest)?;
        self.state.submission = None;
        let timeout = options.lifetime;
        self.start_task_with_binding(options, binding, move |_| {
            Ok(file::Admission::Mutation(Admission {
                kind: AdmissionKind::Reconcile(request),
                timeout,
            }))
        })
    }
    /// Start a read-only directory of protected original plans using current
    /// approval. No selected target or effect session is created or restored.
    pub fn start_mutation_discovery(
        &mut self,
        options: StartOptions,
        subject: String,
        disposition: Disposition,
        scan_limit: u16,
    ) -> Result<TaskKey> {
        self.start_mutation_discovery_from(options, subject, disposition, scan_limit, None)
    }
    pub fn start_mutation_discovery_from(
        &mut self,
        options: StartOptions,
        subject: String,
        disposition: Disposition,
        scan_limit: u16,
        checkpoint: Option<FileMutationPlanCheckpoint>,
    ) -> Result<TaskKey> {
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        self.local_state()?;
        if options.capabilities != BTreeSet::from([disposition.capability()])
            || subject.is_empty()
            || subject.len() > 256
            || subject
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
            || !(1..=morrow_core::store::MAX_FILE_MUTATION_PLAN_SCAN_LIMIT).contains(&scan_limit)
        {
            return Err("invalid mutation discovery scope".into());
        }
        let binding = self.mutation_history_profile(options.digest)?;
        self.state.submission = None;
        let timeout = options.lifetime;
        self.start_task_with_binding(options, binding, move |_| {
            Ok(file::Admission::Mutation(Admission {
                kind: AdmissionKind::Discover {
                    subject,
                    disposition,
                    scan_limit,
                    checkpoint,
                },
                timeout,
            }))
        })
    }
    pub fn mutation_status(&mut self, key: TaskKey) -> Result<MutationSnapshot> {
        let task = self.state.checked_task(key)?;
        let mutation = task.mutation.as_ref().ok_or(AccessError::StaleTask)?;
        if mutation.guest.is_some() {
            return Err("guest mutation task requires guest status".into());
        }
        Ok(mutation.snapshot())
    }
    /// Exactly one pending/unread command. Returns an identity required to read
    /// that reply; a delayed read of an earlier command cannot consume a new one.
    pub fn request_mutation(&mut self, key: TaskKey, action: Action) -> Result<u64> {
        let task = self.state.checked_task(key)?;
        let mutation = task.mutation.as_mut().ok_or(AccessError::StaleTask)?;
        if mutation.guest.is_some() {
            return Err("native mutation action cannot dispatch a guest task".into());
        }
        if mutation.pending.is_some() {
            return Err(AccessError::Busy.into());
        }
        let kind = action.kind();
        if mutation.discovery.is_some()
            && !matches!(kind, CommandKind::Discover | CommandKind::Release)
        {
            return Err("discovery task cannot mutate or reconcile a target".into());
        }
        if mutation.discovery.is_none() && kind == CommandKind::Discover {
            return Err("not a mutation discovery task".into());
        }
        if mutation.discovery.is_none()
            && !mutation.selected
            && !matches!(kind, CommandKind::Query | CommandKind::Release)
        {
            return Err(AccessError::StaleTask.into());
        }
        if (mutation.reconcile_required || mutation.terminal)
            && !matches!(
                kind,
                CommandKind::Query | CommandKind::CancelPlan | CommandKind::Release
            )
        {
            return Err("mutation result requires explicit history reconciliation".into());
        }
        let Some(Executor::Io(worker)) = &task.worker else {
            return Err(AccessError::StaleTask.into());
        };
        let command = mutation
            .command
            .checked_add(1)
            .ok_or("mutation command identity exhausted")?;
        if let Some(discovery) = mutation.discovery {
            let handle = match action {
                Action::NextPlans { scan_limit } => {
                    worker.next_mutation_plans(discovery, scan_limit)?
                }
                Action::Release => worker.close_mutation_discovery(discovery)?,
                _ => return Err("discovery task accepts only paging and release".into()),
            };
            mutation.command = command;
            mutation.kind = kind;
            mutation.pending = Some(handle);
            return Ok(command);
        }
        let session = mutation
            .session
            .ok_or("history task cannot select or mutate a target")?;
        let handle = match action {
            Action::NextPlans { .. } => return Err("not a discovery task".into()),
            Action::BuildPlan {
                operation_id,
                content_length,
                content_sha256,
            } => {
                worker.build_mutation_plan(session, operation_id, content_length, content_sha256)?
            }
            Action::Prepare(plan) => worker.prepare_mutation(session, *plan)?,
            Action::Chunk { offset, mut bytes } => {
                worker.stage_mutation_chunk(session, offset, std::mem::take(&mut *bytes))?
            }
            Action::CommitContent => worker.commit_mutation_content(session)?,
            Action::Execute => worker.execute_mutation(session)?,
            Action::Query => worker.query_mutation(session)?,
            Action::CancelPlan => worker.cancel_mutation_plan(session)?,
            Action::Release => worker.release_mutation(session)?,
        };
        mutation.command = command;
        mutation.kind = kind;
        mutation.pending = Some(handle);
        Ok(command)
    }
    /// Cancel only the matching command's delivery. The task stays alive for an
    /// explicit Query; cancel_io instead stops the worker and requires real join.
    pub fn cancel_mutation_command(&mut self, key: TaskKey, command: u64) -> Result<()> {
        let task = self.state.checked_task(key)?;
        let mutation = task.mutation.as_mut().ok_or(AccessError::StaleTask)?;
        if mutation.guest.is_some() {
            return Err("native mutation command cannot cancel a guest task".into());
        }
        if mutation.command != command {
            return Err(AccessError::StaleTask.into());
        }
        mutation
            .pending
            .as_ref()
            .ok_or(AccessError::StaleTask)?
            .cancel();
        Ok(())
    }
    /// Structured domain versus delivery failures preserve uncertainty. Failed
    /// commands never auto-replay; the caller explicitly reconciles or releases.
    pub fn read_mutation_result(
        &mut self,
        key: TaskKey,
        command: u64,
    ) -> Result<Option<std::result::Result<MutationResponse, Failure>>> {
        let task = self.state.checked_task(key)?;
        let mutation = task.mutation.as_mut().ok_or(AccessError::StaleTask)?;
        if mutation.guest.is_some() {
            return Err("native mutation read cannot consume a guest task".into());
        }
        if mutation.command != command {
            return Err(AccessError::StaleTask.into());
        }
        let handle = mutation.pending.as_mut().ok_or(AccessError::StaleTask)?;
        let result = match handle.read() {
            Ok(None) => return Ok(None),
            Ok(Some(result)) => result.map_err(Failure::Target),
            Err(error) => Err(Failure::Delivery(error)),
        };
        mutation.pending = None;
        match &result {
            Ok(MutationResponse::Plans { done, .. }) => {
                mutation.terminal = *done;
                mutation.reconcile_required = false;
            }
            Ok(MutationResponse::Selected {
                reference,
                expected_identity,
            }) => {
                mutation.selected = true;
                mutation.selection = Some(SelectedTarget {
                    reference: *reference,
                    expected_identity: *expected_identity,
                });
            }
            Ok(MutationResponse::Released) => {
                mutation.selected = false;
                mutation.selection = None;
                mutation.terminal = true;
                mutation.reconcile_required = false;
            }
            Ok(
                MutationResponse::History { record, .. }
                | MutationResponse::Reconciled { record, .. },
            ) => {
                mutation.reconcile_required = record
                    .as_ref()
                    .is_some_and(|r| r.phase() == Phase::OutcomeUnknown);
                mutation.terminal = record.as_ref().is_some_and(|r| {
                    matches!(r.phase(), Phase::Observed | Phase::CancelledBeforeDispatch)
                });
            }
            Ok(
                MutationResponse::PlanCancelled(_)
                | MutationResponse::Created(_)
                | MutationResponse::Deleted(_),
            ) => {
                mutation.terminal = true;
                mutation.reconcile_required = false;
            }
            Err(_) if mutation.discovery.is_some() => {
                // A failed directory page is not an uncertain file effect.
                // Its cursor may have advanced; only close/stop remains valid.
                mutation.terminal = true;
                mutation.reconcile_required = false;
            }
            Err(_) => mutation.reconcile_required = true,
            _ => {}
        }
        let stop = mutation.kind == CommandKind::Reconcile
            || matches!(&result, Ok(MutationResponse::Released));
        if stop {
            self.state.request_stop();
        }
        if let Ok(MutationResponse::Plans {
            checkpoint: Some(checkpoint),
            ..
        }) = &result
        {
            // Only a successfully consumed page can publish a continuation.
            // Tokens are opaque lookup identities, never serialized cursors or grants.
            let token = checkpoint_token(key, command);
            if self.state.mutation_checkpoints.len() == 64 {
                self.state.mutation_checkpoints.pop_front();
            }
            self.state
                .mutation_checkpoints
                .push_back((token, checkpoint.clone()));
        }
        Ok(Some(result))
    }
}

#[cfg(test)]
#[path = "mutation_tasks_tests.rs"]
mod tests;

/// Bounded trusted private start. Its submission ID lives for the workbench
/// session, including failed admission; replay never opens another target.
pub struct StartRequest {
    pub submission: [u8; 32],
    pub package_id: String,
    pub digest: [u8; 32],
    pub revision: u64,
    pub disposition: Disposition,
    pub selected_path: PathBuf,
    pub relative_path: Option<RelativeFilePath>,
    pub subject: String,
    pub approval: [u8; 32],
    pub timeout_ms: u32,
}
/// A fresh private history read, with no target path or restored selection.
pub struct ReconciliationStart {
    pub submission: [u8; 32],
    pub package_id: String,
    pub digest: [u8; 32],
    pub revision: u64,
    pub plan: RequestRecord,
    pub timeout_ms: u32,
}
/// Trusted scope for bounded discovery; does not restore target authority.
pub struct DiscoveryStart {
    pub submission: [u8; 32],
    pub package_id: String,
    pub digest: [u8; 32],
    pub revision: u64,
    pub subject: String,
    pub disposition: Disposition,
    pub scan_limit: u16,
    pub timeout_ms: u32,
    pub checkpoint: Option<[u8; 32]>,
}
pub(crate) fn checkpoint_token(key: TaskKey, command: u64) -> [u8; 32] {
    let mut hash = Sha256::new();
    field(&mut hash, b"morrow.mutation.checkpoint.v1");
    field(&mut hash, key.as_bytes());
    field(&mut hash, &command.to_le_bytes());
    hash.finalize().into()
}
fn field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
impl Workbench {
    pub fn start_selected_mutation(&mut self, request: StartRequest) -> Result<TaskKey> {
        let path = request
            .selected_path
            .to_str()
            .ok_or("invalid mutation path")?;
        if request.submission == [0; 32]
            || request.package_id.is_empty()
            || request.package_id.len() > 256
            || request.subject.is_empty()
            || request.subject.len() > 256
            || request.subject.contains('\0')
            || request.approval == [0; 32]
            || !request.selected_path.is_absolute()
            || path.len() > 4096
            || path.contains('\0')
            || request.timeout_ms == 0
            || request.timeout_ms > 30_000
            || (request.disposition == Disposition::Create) != request.relative_path.is_some()
        {
            return Err("invalid mutation submission".into());
        }
        let mut hash = Sha256::new();
        field(&mut hash, b"morrow.mutation.start.v1");
        field(&mut hash, request.package_id.as_bytes());
        field(&mut hash, &request.digest);
        field(&mut hash, &request.revision.to_le_bytes());
        field(
            &mut hash,
            &[match request.disposition {
                Disposition::Create => 1,
                Disposition::Replace => 2,
                Disposition::Delete => 3,
            }],
        );
        field(&mut hash, path.as_bytes());
        field(
            &mut hash,
            request
                .relative_path
                .as_ref()
                .map_or("", |p| p.as_str())
                .as_bytes(),
        );
        field(&mut hash, request.subject.as_bytes());
        field(&mut hash, &request.approval);
        field(&mut hash, &request.timeout_ms.to_le_bytes());
        let digest: [u8; 32] = hash.finalize().into();
        if let Some((original, key)) = self.state.mutation_submissions.get(&request.submission) {
            if original != &digest {
                return Err("mutation submission conflicts with original request".into());
            }
            let key =
                key.ok_or("original mutation admission failed; submission cannot be reused")?;
            self.state.checked_task(key)?;
            return Ok(key);
        }
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if self.state.mutation_submissions.len() >= 512 {
            return Err("mutation submission capacity exhausted".into());
        }
        // Once structurally admitted with no active task, burn the identity
        // before catalog/budget checks too. Later installation or permission
        // changes must not turn a failed retry into a fresh target selection.
        let submission = request.submission;
        self.state
            .mutation_submissions
            .insert(submission, (digest, None));
        self.state.submission = Some(submission);
        let owner = self.local_state()?;
        let package = owner
            .manager
            .as_ref()
            .ok_or("catalog unavailable")?
            .installed_package(request.digest)?;
        let budget = package
            .io_declaration()
            .and_then(|d| d.budget.as_ref())
            .ok_or("missing mutation IO budget")?;
        if u64::from(request.timeout_ms) > budget.max_duration_ms {
            return Err("mutation duration exceeds declaration".into());
        }
        let limits = morrow_plugin_runtime::io_jobs::JobLimits::new(
            1,
            budget
                .max_job_bytes
                .min(morrow_plugin_runtime::io_jobs::MAX_JOB_BYTES),
            budget
                .max_bytes
                .min(morrow_plugin_runtime::io_jobs::MAX_TOTAL_BYTES),
        )
        .map_err(|_| "invalid mutation budget")?;
        let selection = match request.relative_path {
            Some(relative) => Selection::Create {
                root: request.selected_path,
                relative,
            },
            None => Selection::Existing(request.selected_path),
        };
        let result = self.start_mutation(
            StartOptions {
                package_id: request.package_id,
                digest: request.digest,
                revision: request.revision,
                capabilities: BTreeSet::from([request.disposition.capability()]),
                lifetime: Duration::from_millis(u64::from(request.timeout_ms)),
                limits,
            },
            selection,
            SelectionScope {
                subject: request.subject,
                approval_sha256: request.approval,
                disposition: request.disposition,
            },
        );
        // A failed worker admission may leave a cleanup-only Task. Keep its
        // status available through ioStatus, never claim it is a started mutation.
        let key = result.as_ref().ok().copied();
        self.state
            .mutation_submissions
            .insert(submission, (digest, key));
        self.state.submission = Some(submission);
        result
    }
    /// Deduplicated private reconciliation start. A repeat recovers only the
    /// original task identity; it never creates a fresh read or effect job.
    pub fn start_mutation_reconciliation_request(
        &mut self,
        request: ReconciliationStart,
    ) -> Result<TaskKey> {
        if request.submission == [0; 32]
            || request.package_id.is_empty()
            || request.package_id.len() > 256
            || request.package_id.contains('\0')
            || request.digest == [0; 32]
            || request.timeout_ms == 0
            || request.timeout_ms > 30_000
        {
            return Err("invalid mutation reconciliation submission".into());
        }
        let mut hash = Sha256::new();
        field(&mut hash, b"morrow.mutation.reconcile.v1");
        field(&mut hash, request.package_id.as_bytes());
        field(&mut hash, &request.digest);
        field(&mut hash, &request.revision.to_le_bytes());
        field(&mut hash, request.plan.container());
        field(&mut hash, &request.timeout_ms.to_le_bytes());
        let digest: [u8; 32] = hash.finalize().into();
        if let Some((original, key)) = self.state.mutation_submissions.get(&request.submission) {
            if original != &digest {
                return Err("mutation submission conflicts with original request".into());
            }
            let key =
                key.ok_or("original mutation admission failed; submission cannot be reused")?;
            self.state.checked_task(key)?;
            return Ok(key);
        }
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if self.state.mutation_submissions.len() >= 512 {
            return Err("mutation submission capacity exhausted".into());
        }
        // Share the bounded submission namespace with effect starts, but use a
        // distinct digest domain. Burn structurally admitted identities before
        // any mutable catalog/authority check, including a digest mismatch.
        let submission = request.submission;
        self.state
            .mutation_submissions
            .insert(submission, (digest, None));
        self.state.submission = Some(submission);
        let owner = self.local_state()?;
        let package = owner
            .manager
            .as_ref()
            .ok_or("catalog unavailable")?
            .installed_package(request.digest)?;
        let budget = package
            .io_declaration()
            .and_then(|d| d.budget.as_ref())
            .ok_or("missing mutation IO budget")?;
        if u64::from(request.timeout_ms) > budget.max_duration_ms {
            return Err("mutation duration exceeds declaration".into());
        }
        let max_content_bytes = budget
            .max_job_bytes
            .min(morrow_plugin_runtime::io_jobs::MAX_JOB_BYTES);
        let max_total_bytes = budget
            .max_bytes
            .min(morrow_plugin_runtime::io_jobs::MAX_TOTAL_BYTES);
        let history_job_bytes = max_content_bytes
            .checked_add(morrow_plugin_runtime::io_jobs::MAX_MUTATION_HISTORY_METADATA_BYTES);
        let limits = if package.mutation_budget().is_some()
            && history_job_bytes.is_some_and(|bytes| bytes <= max_total_bytes)
        {
            morrow_plugin_runtime::io_jobs::JobLimits::mutation_history(
                1,
                max_content_bytes,
                max_total_bytes,
            )
        } else {
            morrow_plugin_runtime::io_jobs::JobLimits::new(1, max_content_bytes, max_total_bytes)
        }
        .map_err(|_| "invalid mutation history budget")?;
        let capability = request.plan.request().disposition.capability();
        let result = self.start_mutation_reconciliation(
            StartOptions {
                package_id: request.package_id,
                digest: request.digest,
                revision: request.revision,
                capabilities: BTreeSet::from([capability]),
                lifetime: Duration::from_millis(u64::from(request.timeout_ms)),
                limits,
            },
            request.plan,
        );
        // Cleanup-only tasks from failed admission never masquerade as a
        // successful history task. ioStatus still exposes their cleanup key.
        self.state
            .mutation_submissions
            .insert(submission, (digest, result.as_ref().ok().copied()));
        self.state.submission = Some(submission);
        result
    }
    /// Deduplicated private discovery start. A repeat recovers only the
    /// original task identity; it never creates a fresh read or effect job.
    pub fn start_mutation_discovery_request(&mut self, request: DiscoveryStart) -> Result<TaskKey> {
        if request.submission == [0; 32]
            || request.package_id.is_empty()
            || request.package_id.len() > 256
            || request.package_id.contains('\0')
            || request.digest == [0; 32]
            || request.checkpoint == Some([0; 32])
            || request.subject.is_empty()
            || request.subject.len() > 256
            || request
                .subject
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
            || !(1..=morrow_core::store::MAX_FILE_MUTATION_PLAN_SCAN_LIMIT)
                .contains(&request.scan_limit)
            || request.timeout_ms == 0
            || request.timeout_ms > 30_000
        {
            return Err("invalid mutation discovery submission".into());
        }
        let mut hash = Sha256::new();
        field(&mut hash, b"morrow.mutation.discover.v1");
        field(&mut hash, request.package_id.as_bytes());
        field(&mut hash, &request.digest);
        field(&mut hash, &request.revision.to_le_bytes());
        field(&mut hash, request.subject.as_bytes());
        field(
            &mut hash,
            &[match request.disposition {
                Disposition::Create => 1,
                Disposition::Replace => 2,
                Disposition::Delete => 3,
            }],
        );
        field(&mut hash, &request.scan_limit.to_le_bytes());
        field(&mut hash, &request.timeout_ms.to_le_bytes());
        field(
            &mut hash,
            request.checkpoint.as_ref().map_or(&[][..], |v| &v[..]),
        );
        let digest: [u8; 32] = hash.finalize().into();
        if let Some((original, key)) = self.state.mutation_submissions.get(&request.submission) {
            if original != &digest {
                return Err("mutation submission conflicts with original request".into());
            }
            let key =
                key.ok_or("original mutation admission failed; submission cannot be reused")?;
            self.state.checked_task(key)?;
            return Ok(key);
        }
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if self.state.mutation_submissions.len() >= 512 {
            return Err("mutation submission capacity exhausted".into());
        }
        // Share the bounded submission namespace with effect starts, but use a
        // distinct digest domain. Burn structurally admitted identities before
        // any mutable catalog/authority check, including a digest mismatch.
        let submission = request.submission;
        self.state
            .mutation_submissions
            .insert(submission, (digest, None));
        self.state.submission = Some(submission);
        let checkpoint = request
            .checkpoint
            .map(|token| {
                self.state
                    .mutation_checkpoints
                    .iter()
                    .find(|(key, _)| key == &token)
                    .map(|(_, value)| value.clone())
                    .ok_or("mutation checkpoint expired or belongs to another host")
            })
            .transpose()?;
        let owner = self.local_state()?;
        let package = owner
            .manager
            .as_ref()
            .ok_or("catalog unavailable")?
            .installed_package(request.digest)?;
        let budget = package
            .io_declaration()
            .and_then(|d| d.budget.as_ref())
            .ok_or("missing mutation IO budget")?;
        if u64::from(request.timeout_ms) > budget.max_duration_ms {
            return Err("mutation duration exceeds declaration".into());
        }
        let limits = morrow_plugin_runtime::io_jobs::JobLimits::new(
            1,
            budget
                .max_job_bytes
                .min(morrow_plugin_runtime::io_jobs::MAX_JOB_BYTES),
            budget
                .max_bytes
                .min(morrow_plugin_runtime::io_jobs::MAX_TOTAL_BYTES),
        )
        .map_err(|_| "invalid mutation budget")?;
        let capability = request.disposition.capability();
        let result = self.start_mutation_discovery_from(
            StartOptions {
                package_id: request.package_id,
                digest: request.digest,
                revision: request.revision,
                capabilities: BTreeSet::from([capability]),
                lifetime: Duration::from_millis(u64::from(request.timeout_ms)),
                limits,
            },
            request.subject,
            request.disposition,
            request.scan_limit,
            checkpoint,
        );
        // Cleanup-only tasks from failed admission never masquerade as a
        // successful history task. ioStatus still exposes their cleanup key.
        self.state
            .mutation_submissions
            .insert(submission, (digest, result.as_ref().ok().copied()));
        self.state.submission = Some(submission);
        result
    }
    /// Exact repeated submissions return the original command ID, including
    /// after consumption. A reused ID with different bytes is always rejected.
    pub fn submit_mutation(
        &mut self,
        key: TaskKey,
        submission: [u8; 32],
        action: Action,
    ) -> Result<u64> {
        if submission == [0; 32] {
            return Err("mutation command submission is required".into());
        }
        let mut hash = Sha256::new();
        field(&mut hash, b"morrow.mutation.command.v1");
        field(&mut hash, &[action.kind() as u8]);
        match &action {
            Action::NextPlans { scan_limit } => field(&mut hash, &scan_limit.to_le_bytes()),
            Action::BuildPlan {
                operation_id,
                content_length,
                content_sha256,
            } => {
                if operation_id.is_empty()
                    || operation_id.len() > 256
                    || operation_id
                        .chars()
                        .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
                {
                    return Err("invalid mutation operation identity".into());
                }
                field(&mut hash, operation_id.as_bytes());
                field(&mut hash, &content_length.to_le_bytes());
                field(
                    &mut hash,
                    content_sha256.as_ref().map_or(&[][..], |v| &v[..]),
                );
            }
            Action::Prepare(plan) => field(&mut hash, plan.container()),
            Action::Chunk { offset, bytes } => {
                if bytes.len() > morrow_plugin_runtime::io_jobs::MAX_MUTATION_CHUNK {
                    return Err("mutation chunk limit".into());
                }
                field(&mut hash, &offset.to_le_bytes());
                field(&mut hash, bytes);
            }
            _ => {}
        }
        let digest: [u8; 32] = hash.finalize().into();
        let task = self
            .state
            .checked_task(key)?
            .mutation
            .as_ref()
            .ok_or(AccessError::StaleTask)?;
        if task.guest.is_some() {
            return Err("native mutation submission cannot dispatch a guest task".into());
        }
        if let Some((original, command)) = task.submissions.get(&submission) {
            if original != &digest {
                return Err("mutation command submission conflicts with original bytes".into());
            }
            return Ok(*command);
        }
        if task.submissions.len() >= 512 {
            return Err("mutation command history capacity exhausted".into());
        }
        let command = self.request_mutation(key, action)?;
        self.state
            .checked_task(key)?
            .mutation
            .as_mut()
            .ok_or(AccessError::StaleTask)?
            .submissions
            .insert(submission, (digest, command));
        Ok(command)
    }
}
