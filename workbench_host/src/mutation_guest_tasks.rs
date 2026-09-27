//! Trusted guest mutation jobs on the original selected worker. The plugin
//! receives one host-built frame per explicit command, never a path or grant.
use super::*;
use crate::io_tasks::BindingProfile;
use morrow_core::mutation as guest_wire;
use morrow_plugin_runtime::{
    Fault,
    io_binding::MutationBudget,
    io_jobs::{
        JobError, JobHandle, MutationGuestExecutionPermit, MutationGuestJobMode, MutationGuestLease,
    },
};
use std::collections::BTreeMap;
use zeroize::Zeroize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuestCommandKind {
    Select,
    BuildPlan,
    Prepare,
    Chunk,
    CommitContent,
    Execute,
    Query,
    CancelPlan,
    Release,
    HostQuery,
    HostCancelPlan,
    HostRelease,
}
impl GuestCommandKind {
    fn tag(self) -> u8 {
        match self {
            Self::Select => 0,
            Self::BuildPlan => 1,
            Self::Prepare => 2,
            Self::Chunk => 3,
            Self::CommitContent => 4,
            Self::Execute => 5,
            Self::Query => 6,
            Self::CancelPlan => 7,
            Self::Release => 8,
            Self::HostQuery => 9,
            Self::HostCancelPlan => 10,
            Self::HostRelease => 11,
        }
    }
}

pub enum GuestAction {
    BuildPlan {
        operation_id: String,
        content_length: u64,
        content_sha256: Option<[u8; 32]>,
    },
    /// First explicit UI review of the exact canonical plan returned by BuildPlan.
    Prepare {
        plan_sha256: [u8; 32],
    },
    Chunk {
        offset: u64,
        bytes: Zeroizing<Vec<u8>>,
    },
    CommitContent,
    /// Second explicit UI review, after Create content is durable.
    Execute {
        plan_sha256: [u8; 32],
    },
    Query,
    CancelPlan,
    Release,
    /// Read-only/cleanup owner operations if the guest frame is unavailable.
    HostQuery,
    HostCancelPlan,
    HostRelease,
}
impl GuestAction {
    fn kind(&self) -> GuestCommandKind {
        match self {
            Self::BuildPlan { .. } => GuestCommandKind::BuildPlan,
            Self::Prepare { .. } => GuestCommandKind::Prepare,
            Self::Chunk { .. } => GuestCommandKind::Chunk,
            Self::CommitContent => GuestCommandKind::CommitContent,
            Self::Execute { .. } => GuestCommandKind::Execute,
            Self::Query => GuestCommandKind::Query,
            Self::CancelPlan => GuestCommandKind::CancelPlan,
            Self::Release => GuestCommandKind::Release,
            Self::HostQuery => GuestCommandKind::HostQuery,
            Self::HostCancelPlan => GuestCommandKind::HostCancelPlan,
            Self::HostRelease => GuestCommandKind::HostRelease,
        }
    }
    fn fingerprint(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        field(&mut hash, b"morrow.mutation.guest.command.v1");
        field(&mut hash, &[self.kind().tag()]);
        match self {
            Self::BuildPlan {
                operation_id,
                content_length,
                content_sha256,
            } => {
                field(&mut hash, operation_id.as_bytes());
                field(&mut hash, &content_length.to_le_bytes());
                field(
                    &mut hash,
                    content_sha256.as_ref().map_or(&[][..], |v| &v[..]),
                );
            }
            Self::Prepare { plan_sha256 } | Self::Execute { plan_sha256 } => {
                field(&mut hash, plan_sha256);
            }
            Self::Chunk { offset, bytes } => {
                field(&mut hash, &offset.to_le_bytes());
                field(&mut hash, bytes);
            }
            _ => {}
        }
        hash.finalize().into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuestMutationSnapshot {
    pub command: u64,
    pub kind: GuestCommandKind,
    pub delivery: Poll,
    pub selected: bool,
    pub selection: Option<SelectedTarget>,
    pub reviewed_plan_sha256: Option<[u8; 32]>,
    pub approval_delivered: bool,
    pub permit_delivered: bool,
    pub staged_bytes: u64,
    pub durable_content: bool,
    pub effect_attempted: bool,
    pub reconcile_required: bool,
    pub terminal: bool,
}

pub enum GuestMutationReply {
    /// Only Selected, Planned, History, PlanCancelled or Released are exposed.
    Owner(MutationResponse),
    /// Correlated Core mutation response from exactly one guest import/completion.
    Frame(GuestMutationFrame),
}
/// The exact completion frame validated against the original request inside
/// the original runtime owner. The private wire returns these bytes verbatim.
pub struct GuestMutationFrame {
    response: guest_wire::Response,
    raw: Vec<u8>,
}
impl GuestMutationFrame {
    pub fn as_bytes(&self) -> &[u8] {
        &self.raw
    }
}
impl std::ops::Deref for GuestMutationFrame {
    type Target = guest_wire::Response;
    fn deref(&self) -> &Self::Target {
        &self.response
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuestMutationFailure {
    Owner(Failure),
    Job(JobError),
    Execution(Fault),
    Cancelled,
    Protocol,
}

pub(super) struct GuestState {
    timeout: Duration,
    kind: GuestCommandKind,
    plan: Option<RequestRecord>,
    lease: Option<MutationGuestLease>,
    permit: Option<MutationGuestExecutionPermit>,
    pending: Option<GuestPending>,
    completed: Option<std::result::Result<GuestMutationReply, GuestMutationFailure>>,
    submissions: BTreeMap<[u8; 32], ([u8; 32], u64)>,
    staged_bytes: u64,
    durable_content: bool,
    prepared: bool,
    effect_attempted: bool,
}
enum GuestPending {
    Issue {
        submission: [u8; 32],
        call_id: u64,
    },
    Authorize {
        submission: [u8; 32],
        call_id: u64,
    },
    Frame {
        handle: JobHandle,
        expected: ExpectedFrame,
    },
}
struct ExpectedFrame {
    call_id: u64,
    reference: [u8; 32],
    submission: [u8; 32],
    operation_id: String,
    kind: guest_wire::Kind,
}
impl GuestState {
    pub(super) fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            kind: GuestCommandKind::Select,
            plan: None,
            lease: None,
            permit: None,
            pending: None,
            completed: None,
            submissions: BTreeMap::new(),
            staged_bytes: 0,
            durable_content: false,
            prepared: false,
            effect_attempted: false,
        }
    }
    pub(super) fn poll(&mut self, owner: Option<&MutationHandle>) -> Option<Poll> {
        if self.completed.is_some() {
            return Some(Poll::Ready);
        }
        if let Some(GuestPending::Frame { handle, .. }) = &mut self.pending {
            return Some(handle.poll());
        }
        owner.map(|handle| match handle.poll() {
            OwnerCommandPoll::Pending => Poll::Pending,
            OwnerCommandPoll::Ready => Poll::Ready,
            OwnerCommandPoll::Consumed => Poll::Consumed,
        })
    }
    fn plan_sha256(&self) -> Option<[u8; 32]> {
        self.plan
            .as_ref()
            .map(|plan| Sha256::digest(plan.container()).into())
    }
}

fn guest_mutation(task: &mut crate::io_tasks::Task) -> Result<&mut MutationTask> {
    let mutation = task.mutation.as_mut().ok_or(AccessError::StaleTask)?;
    if mutation.guest.is_none() {
        return Err("not a guest mutation task".into());
    }
    Ok(mutation)
}

/// Only an observed guest Query carries the original OS effect proof. An
/// absent/prepared read after an Execute attempt does not undo that attempt.
fn history_requires_reconciliation(
    prior: bool,
    effect_attempted: bool,
    phase: guest_wire::Phase,
    observed_effect_proven: bool,
) -> bool {
    match phase {
        // A phase-only owner read cannot clear a prior unknown, but neither
        // can it revoke an OS effect already delivered by a guest reply.
        guest_wire::Phase::Observed => prior && !observed_effect_proven,
        guest_wire::Phase::CancelledBeforeDispatch => false,
        guest_wire::Phase::Prepared => effect_attempted,
        guest_wire::Phase::Absent => prior || effect_attempted,
        guest_wire::Phase::OutcomeUnknown | guest_wire::Phase::None => true,
    }
}

fn submit_frame(
    worker: &IoWorker<WorkbenchState>,
    guest: &GuestState,
    mode: MutationGuestJobMode,
    call_id: u64,
    submission: [u8; 32],
    action: guest_wire::Action,
) -> Result<GuestPending> {
    let operation_id = guest
        .plan
        .as_ref()
        .ok_or("missing reviewed guest plan")?
        .request()
        .operation_id
        .clone();
    let mut request = guest_wire::Request {
        call_id,
        reference: mode.lease().reference(),
        submission,
        operation_id: operation_id.clone(),
        deadline_ms: u32::try_from(guest.timeout.as_millis())?.min(guest_wire::MAX_DEADLINE_MS),
        action,
    };
    let kind = request.action.kind();
    let encoded = request.encode();
    if let guest_wire::Action::Chunk { bytes, .. } = &mut request.action {
        bytes.zeroize();
    }
    let frame = encoded?;
    let handle = worker
        .submit_mutation_guest_frame(frame, mode, guest.timeout)
        .map_err(|error| format!("guest frame admission: {error:?}"))?;
    Ok(GuestPending::Frame {
        handle,
        expected: ExpectedFrame {
            call_id,
            reference: mode.lease().reference(),
            submission,
            operation_id,
            kind,
        },
    })
}

fn advance_internal(mutation: &mut MutationTask, worker: &IoWorker<WorkbenchState>) {
    let Some(guest) = mutation.guest.as_mut() else {
        return;
    };
    let step = match guest.pending.as_ref() {
        Some(GuestPending::Issue {
            submission,
            call_id,
        }) => Some((false, *submission, *call_id)),
        Some(GuestPending::Authorize {
            submission,
            call_id,
        }) => Some((true, *submission, *call_id)),
        _ => None,
    };
    let Some((authorize, submission, call_id)) = step else {
        return;
    };
    let Some(handle) = mutation.pending.as_mut() else {
        guest.completed = Some(Err(GuestMutationFailure::Protocol));
        guest.pending = None;
        mutation.reconcile_required = true;
        return;
    };
    let reply = match handle.read() {
        Ok(None) => return,
        Ok(Some(result)) => {
            result.map_err(|error| GuestMutationFailure::Owner(Failure::Target(error)))
        }
        Err(error) => Err(GuestMutationFailure::Owner(Failure::Delivery(error))),
    };
    mutation.pending = None;
    guest.pending = None;
    let result = match (authorize, reply) {
        (false, Ok(MutationResponse::GuestApproved(lease))) => {
            guest.lease = Some(lease);
            let Some(plan) = guest.plan.as_ref() else {
                guest.completed = Some(Err(GuestMutationFailure::Protocol));
                mutation.reconcile_required = true;
                return;
            };
            let action = match plan.request().disposition {
                Disposition::Create => guest_wire::Action::Create {
                    content_length: plan.request().content_length,
                    content_sha256: plan.request().content_sha256.unwrap(),
                },
                Disposition::Delete => guest_wire::Action::Delete,
                Disposition::Replace => {
                    guest.completed = Some(Err(GuestMutationFailure::Protocol));
                    mutation.reconcile_required = true;
                    return;
                }
            };
            submit_frame(
                worker,
                guest,
                MutationGuestJobMode::Stage(lease),
                call_id,
                submission,
                action,
            )
        }
        (true, Ok(MutationResponse::GuestExecutionAuthorized(permit))) => {
            guest.permit = Some(permit);
            submit_frame(
                worker,
                guest,
                MutationGuestJobMode::Execute(permit),
                call_id,
                submission,
                guest_wire::Action::Execute,
            )
        }
        (_, Ok(_)) => {
            guest.completed = Some(Err(GuestMutationFailure::Protocol));
            mutation.reconcile_required = true;
            return;
        }
        (_, Err(error)) => {
            guest.completed = Some(Err(error));
            mutation.reconcile_required = true;
            return;
        }
    };
    match result {
        Ok(pending) => guest.pending = Some(pending),
        Err(_) => {
            guest.completed = Some(Err(GuestMutationFailure::Protocol));
            mutation.reconcile_required = true;
        }
    }
}

impl Workbench {
    pub fn guest_mutation_status(&mut self, key: TaskKey) -> Result<GuestMutationSnapshot> {
        let task = self.state.checked_task(key)?;
        let mutation = task.mutation.as_mut().ok_or(AccessError::StaleTask)?;
        if mutation.guest.is_none() {
            return Err("not a guest mutation task".into());
        }
        if let Some(Executor::Io(worker)) = &task.worker {
            advance_internal(mutation, worker);
        }
        let guest = mutation.guest.as_mut().expect("checked guest mode");
        let delivery = guest
            .poll(mutation.pending.as_ref())
            .unwrap_or(Poll::Consumed);
        Ok(GuestMutationSnapshot {
            command: mutation.command,
            kind: guest.kind,
            delivery,
            selected: mutation.selected,
            selection: mutation.selection,
            reviewed_plan_sha256: guest.plan_sha256(),
            approval_delivered: guest.lease.is_some(),
            permit_delivered: guest.permit.is_some(),
            staged_bytes: guest.staged_bytes,
            durable_content: guest.durable_content,
            effect_attempted: guest.effect_attempted,
            reconcile_required: mutation.reconcile_required,
            terminal: mutation.terminal,
        })
    }

    /// Each call is a new explicit host instruction. A same-token replay only
    /// returns the original command identity, never queues a second effect.
    pub fn submit_guest_mutation(
        &mut self,
        key: TaskKey,
        submission: [u8; 32],
        action: GuestAction,
    ) -> Result<u64> {
        if submission == [0; 32] {
            return Err("guest submission is required".into());
        }
        let digest = action.fingerprint();
        {
            let task = self.state.checked_task(key)?;
            let mutation = guest_mutation(task)?;
            let guest = mutation.guest.as_ref().expect("checked guest mode");
            if let Some((original, command)) = guest.submissions.get(&submission) {
                if *original != digest {
                    return Err("guest submission conflicts with original command".into());
                }
                return Ok(*command);
            }
            if guest.submissions.len() >= 512 {
                return Err("guest command history exhausted".into());
            }
        }
        let command = self.request_guest_mutation(key, submission, action)?;
        let mutation = guest_mutation(self.state.checked_task(key)?)?;
        mutation
            .guest
            .as_mut()
            .expect("checked guest mode")
            .submissions
            .insert(submission, (digest, command));
        Ok(command)
    }

    fn request_guest_mutation(
        &mut self,
        key: TaskKey,
        submission: [u8; 32],
        action: GuestAction,
    ) -> Result<u64> {
        let task = self.state.checked_task(key)?;
        if task.stopping || task.exit.is_some() {
            return Err(AccessError::StaleTask.into());
        }
        let Some(Executor::Io(worker)) = &task.worker else {
            return Err(AccessError::StaleTask.into());
        };
        let mutation = task.mutation.as_mut().ok_or(AccessError::StaleTask)?;
        if mutation.guest.is_none() {
            return Err("not a guest mutation task".into());
        }
        let guest = mutation.guest.as_mut().expect("checked guest mode");
        if mutation.pending.is_some() || guest.pending.is_some() || guest.completed.is_some() {
            return Err(AccessError::Busy.into());
        }
        if !mutation.selected {
            return Err(AccessError::StaleTask.into());
        }
        let kind = action.kind();
        if (mutation.reconcile_required || mutation.terminal)
            && !matches!(
                kind,
                GuestCommandKind::Query
                    | GuestCommandKind::CancelPlan
                    | GuestCommandKind::Release
                    | GuestCommandKind::HostQuery
                    | GuestCommandKind::HostCancelPlan
                    | GuestCommandKind::HostRelease
            )
        {
            return Err("guest mutation requires explicit reconciliation".into());
        }
        let command = mutation
            .command
            .checked_add(1)
            .ok_or("guest command identity exhausted")?;
        let session = mutation.session.ok_or(AccessError::StaleTask)?;
        match action {
            GuestAction::BuildPlan {
                operation_id,
                content_length,
                content_sha256,
            } => {
                if guest.lease.is_some() || guest.plan.is_some() {
                    return Err("guest plan is already reviewed".into());
                }
                mutation.pending = Some(worker.build_mutation_plan(
                    session,
                    operation_id,
                    content_length,
                    content_sha256,
                )?);
            }
            GuestAction::Prepare { plan_sha256 } => {
                let plan = guest
                    .plan
                    .as_ref()
                    .ok_or("build the canonical plan first")?;
                if guest.lease.is_some() || guest.plan_sha256() != Some(plan_sha256) {
                    return Err("guest plan approval mismatch".into());
                }
                let mut reference = [0; 32];
                getrandom::fill(&mut reference)?;
                mutation.pending =
                    Some(worker.issue_mutation_guest(session, plan.clone(), reference)?);
                guest.pending = Some(GuestPending::Issue {
                    submission,
                    call_id: command,
                });
            }
            GuestAction::Chunk { offset, mut bytes } => {
                let lease = guest.lease.ok_or("guest approval has not been delivered")?;
                let plan = guest.plan.as_ref().ok_or("missing guest plan")?;
                if !guest.prepared
                    || plan.request().disposition != Disposition::Create
                    || offset != guest.staged_bytes
                    || bytes.is_empty()
                    || bytes.len() > guest_wire::MAX_CHUNK_BYTES
                    || offset
                        .checked_add(bytes.len() as u64)
                        .is_none_or(|end| end > plan.request().content_length)
                {
                    return Err("invalid guest content chunk".into());
                }
                guest.pending = Some(submit_frame(
                    worker,
                    guest,
                    MutationGuestJobMode::Stage(lease),
                    command,
                    submission,
                    guest_wire::Action::Chunk {
                        offset,
                        bytes: std::mem::take(&mut *bytes),
                    },
                )?);
            }
            GuestAction::CommitContent => {
                let lease = guest.lease.ok_or("guest approval has not been delivered")?;
                let plan = guest.plan.as_ref().ok_or("missing guest plan")?;
                if !guest.prepared
                    || guest.durable_content
                    || plan.request().disposition != Disposition::Create
                    || guest.staged_bytes != plan.request().content_length
                {
                    return Err("guest content is incomplete".into());
                }
                guest.pending = Some(submit_frame(
                    worker,
                    guest,
                    MutationGuestJobMode::Stage(lease),
                    command,
                    submission,
                    guest_wire::Action::Commit,
                )?);
            }
            GuestAction::Execute { plan_sha256 } => {
                let plan = guest.plan.as_ref().ok_or("missing guest plan")?;
                if !guest.prepared
                    || guest.effect_attempted
                    || guest.plan_sha256() != Some(plan_sha256)
                    || (plan.request().disposition == Disposition::Create && !guest.durable_content)
                {
                    return Err("guest execution review or content is incomplete".into());
                }
                mutation.pending =
                    Some(worker.authorize_mutation_guest_execution(session, plan_sha256)?);
                guest.pending = Some(GuestPending::Authorize {
                    submission,
                    call_id: command,
                });
                guest.effect_attempted = true;
            }
            GuestAction::Query | GuestAction::CancelPlan | GuestAction::Release => {
                let lease = guest.lease.ok_or("guest approval has not been delivered")?;
                let frame_action = match kind {
                    GuestCommandKind::Query => guest_wire::Action::Query,
                    GuestCommandKind::CancelPlan => guest_wire::Action::CancelPlan,
                    _ => guest_wire::Action::Release,
                };
                guest.pending = Some(submit_frame(
                    worker,
                    guest,
                    MutationGuestJobMode::Stage(lease),
                    command,
                    submission,
                    frame_action,
                )?);
            }
            GuestAction::HostQuery => mutation.pending = Some(worker.query_mutation(session)?),
            GuestAction::HostCancelPlan => {
                mutation.pending = Some(worker.cancel_mutation_plan(session)?)
            }
            GuestAction::HostRelease => mutation.pending = Some(worker.release_mutation(session)?),
        }
        mutation.command = command;
        guest.kind = kind;
        Ok(command)
    }

    pub fn cancel_guest_mutation_command(&mut self, key: TaskKey, command: u64) -> Result<()> {
        let mutation = guest_mutation(self.state.checked_task(key)?)?;
        if mutation.command != command {
            return Err(AccessError::StaleTask.into());
        }
        if let Some(handle) = mutation.pending.as_ref() {
            handle.cancel();
            return Ok(());
        }
        let guest = mutation.guest.as_mut().expect("checked guest mode");
        if let Some(GuestPending::Frame { handle, .. }) = guest.pending.take() {
            // Abandon the reply even if the job is already Ready. A committed
            // effect remains uncertain until an explicit history read.
            handle.cancel();
            drop(handle);
            guest.completed = Some(Err(GuestMutationFailure::Cancelled));
            mutation.reconcile_required = true;
            return Ok(());
        }
        Err(AccessError::StaleTask.into())
    }

    pub fn read_guest_mutation_result(
        &mut self,
        key: TaskKey,
        command: u64,
    ) -> Result<Option<std::result::Result<GuestMutationReply, GuestMutationFailure>>> {
        let task = self.state.checked_task(key)?;
        let Some(Executor::Io(worker)) = &task.worker else {
            return Err(AccessError::StaleTask.into());
        };
        let mutation = task.mutation.as_mut().ok_or(AccessError::StaleTask)?;
        if mutation.guest.is_none() {
            return Err("not a guest mutation task".into());
        }
        if mutation.command != command {
            return Err(AccessError::StaleTask.into());
        }
        advance_internal(mutation, worker);
        let guest = mutation.guest.as_mut().expect("checked guest mode");
        if let Some(result) = guest.completed.take() {
            return Ok(Some(result));
        }
        // Only advance_internal may consume an issue/permit ticket. A second
        // read here could race its completion and leak or steal that receipt.
        if matches!(
            guest.pending,
            Some(GuestPending::Issue { .. } | GuestPending::Authorize { .. })
        ) {
            return Ok(None);
        }
        if let Some(GuestPending::Frame { handle, expected }) = guest.pending.as_mut() {
            // JobReport retains both the guest completion frame and the
            // decoded response metadata; account for both bounded payloads.
            let report = match handle.read(guest_wire::MAX_FRAME_BYTES * 2 + 4096) {
                Ok(None) => return Ok(None),
                Ok(Some(report)) => report,
                Err(JobError::ReadBound) => return Err("guest reply exceeded bounded read".into()),
                Err(error) => {
                    guest.pending = None;
                    mutation.reconcile_required = true;
                    return Ok(Some(Err(GuestMutationFailure::Job(error))));
                }
            };
            let result = if let Err(fault) = report.task.execution.outcome {
                Err(GuestMutationFailure::Execution(fault))
            } else if report.task.execution.outcome != Ok(0) {
                Err(GuestMutationFailure::Protocol)
            } else if report.calls != 1 || report.cancelled || report.unknown {
                Err(GuestMutationFailure::Protocol)
            } else if let (Some(response), Some(raw)) =
                (report.mutation_response, report.mutation_frame)
            {
                if response.call_id != expected.call_id
                    || response.reference != expected.reference
                    || response.submission != expected.submission
                    || response.operation_id != expected.operation_id
                    || response.kind != expected.kind
                    || raw.is_empty()
                    || raw.len() > guest_wire::MAX_FRAME_BYTES
                {
                    Err(GuestMutationFailure::Protocol)
                } else {
                    Ok(GuestMutationReply::Frame(GuestMutationFrame {
                        response,
                        raw,
                    }))
                }
            } else {
                Err(GuestMutationFailure::Protocol)
            };
            guest.pending = None;
            match &result {
                Ok(GuestMutationReply::Frame(response)) => {
                    if response.status == guest_wire::Status::Completed {
                        match guest.kind {
                            GuestCommandKind::Prepare => guest.prepared = true,
                            GuestCommandKind::Chunk => guest.staged_bytes = response.staged_bytes,
                            GuestCommandKind::CommitContent => {
                                guest.durable_content = response.durable_content
                            }
                            GuestCommandKind::CancelPlan => mutation.terminal = true,
                            GuestCommandKind::Release => {
                                mutation.terminal = true;
                                mutation.selected = false;
                                mutation.selection = None;
                                task.stopping = true;
                                worker.stop();
                            }
                            GuestCommandKind::Execute => mutation.terminal = true,
                            GuestCommandKind::Query => match response.phase {
                                guest_wire::Phase::Prepared => {
                                    guest.prepared = true;
                                    guest.staged_bytes = response.staged_bytes;
                                    guest.durable_content = response.durable_content;
                                }
                                guest_wire::Phase::Observed
                                | guest_wire::Phase::CancelledBeforeDispatch => {
                                    mutation.terminal = true
                                }
                                _ => {}
                            },
                            _ => {}
                        }
                    }
                    if response.status == guest_wire::Status::Completed {
                        // Only a completed, correlated history read can clear an
                        // earlier uncertainty; denial is never new evidence.
                        if guest.kind == GuestCommandKind::Query {
                            mutation.reconcile_required = history_requires_reconciliation(
                                mutation.reconcile_required,
                                guest.effect_attempted,
                                response.phase,
                                response.phase == guest_wire::Phase::Observed
                                    && matches!(
                                        response.effect,
                                        guest_wire::Effect::OsSucceeded
                                            | guest_wire::Effect::OsRejected
                                    ),
                            );
                        }
                    } else {
                        mutation.reconcile_required = true;
                    }
                }
                Err(_) => mutation.reconcile_required = true,
                _ => {}
            }
            return Ok(Some(result));
        }
        let handle = mutation.pending.as_mut().ok_or(AccessError::StaleTask)?;
        let mut result = match handle.read() {
            Ok(None) => return Ok(None),
            Ok(Some(result)) => result
                .map(GuestMutationReply::Owner)
                .map_err(|error| GuestMutationFailure::Owner(Failure::Target(error))),
            Err(error) => Err(GuestMutationFailure::Owner(Failure::Delivery(error))),
        };
        mutation.pending = None;
        match &result {
            Ok(GuestMutationReply::Owner(MutationResponse::Selected {
                reference,
                expected_identity,
            })) => {
                mutation.selected = true;
                mutation.selection = Some(SelectedTarget {
                    reference: *reference,
                    expected_identity: *expected_identity,
                });
            }
            Ok(GuestMutationReply::Owner(MutationResponse::Planned(plan))) => {
                guest.plan = Some(plan.clone());
            }
            Ok(GuestMutationReply::Owner(MutationResponse::Released)) => {
                mutation.selected = false;
                mutation.selection = None;
                mutation.terminal = true;
                task.stopping = true;
                worker.stop();
            }
            Ok(GuestMutationReply::Owner(MutationResponse::PlanCancelled(_))) => {
                mutation.terminal = true
            }
            Ok(GuestMutationReply::Owner(MutationResponse::History {
                record,
                staged_bytes,
                durable_content,
            })) => {
                if let Some(record) = record {
                    let phase = match record.phase() {
                        Phase::Prepared => guest_wire::Phase::Prepared,
                        Phase::OutcomeUnknown => guest_wire::Phase::OutcomeUnknown,
                        Phase::Observed => guest_wire::Phase::Observed,
                        Phase::CancelledBeforeDispatch => {
                            guest_wire::Phase::CancelledBeforeDispatch
                        }
                        Phase::InvalidPhase => guest_wire::Phase::None,
                    };
                    mutation.reconcile_required = history_requires_reconciliation(
                        mutation.reconcile_required,
                        guest.effect_attempted,
                        phase,
                        false,
                    );
                    if matches!(
                        record.phase(),
                        Phase::Observed | Phase::CancelledBeforeDispatch
                    ) {
                        mutation.terminal = true;
                    }
                    if record.phase() == Phase::Prepared {
                        guest.prepared = true;
                        guest.staged_bytes = *staged_bytes;
                        guest.durable_content = *durable_content;
                    }
                } else {
                    mutation.reconcile_required |= guest.effect_attempted;
                }
                // An absent history entry cannot disprove an earlier unknown
                // dispatch, so leave an existing uncertainty latched.
            }
            Ok(GuestMutationReply::Owner(_)) => {
                mutation.reconcile_required = true;
                result = Err(GuestMutationFailure::Protocol);
            }
            Err(_) => mutation.reconcile_required = true,
            _ => {}
        }
        Ok(Some(result))
    }
}

impl Workbench {
    /// A distinct trusted UI entry point. `approved_budget` is a host review
    /// decision, not a guest-declared grant. Package declarations only bound it.
    pub fn start_selected_guest_mutation(
        &mut self,
        request: StartRequest,
        approved_budget: Option<MutationBudget>,
    ) -> Result<TaskKey> {
        let path = request
            .selected_path
            .to_str()
            .ok_or("invalid guest mutation path")?;
        if request.submission == [0; 32]
            || request.package_id.is_empty()
            || request.package_id.len() > 256
            || request.subject.is_empty()
            || request.subject.len() > 256
            || request
                .subject
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
            || request.approval == [0; 32]
            || !request.selected_path.is_absolute()
            || path.len() > 4096
            || path.contains('\0')
            || request.timeout_ms == 0
            || request.timeout_ms > 30_000
            || !matches!(
                request.disposition,
                Disposition::Create | Disposition::Delete
            )
            || (request.disposition == Disposition::Create) != request.relative_path.is_some()
        {
            return Err("invalid guest mutation start".into());
        }
        let mut hash = Sha256::new();
        field(&mut hash, b"morrow.mutation.guest.start.v1");
        field(&mut hash, request.package_id.as_bytes());
        field(&mut hash, &request.digest);
        field(&mut hash, &request.revision.to_le_bytes());
        field(
            &mut hash,
            &[match request.disposition {
                Disposition::Create => 1,
                Disposition::Delete => 3,
                Disposition::Replace => unreachable!(),
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
        match approved_budget {
            Some(budget) => {
                field(&mut hash, &[1]);
                field(&mut hash, &budget.max_job_bytes.to_le_bytes());
                field(&mut hash, &budget.max_bytes.to_le_bytes());
            }
            None => field(&mut hash, &[0]),
        }
        let digest: [u8; 32] = hash.finalize().into();
        if let Some((original, key)) = self.state.mutation_submissions.get(&request.submission) {
            if original != &digest {
                return Err("guest start submission conflicts with original request".into());
            }
            let key = key.ok_or("original guest admission failed; submission cannot be reused")?;
            self.state.checked_task(key)?;
            return Ok(key);
        }
        if self.io_status().key.is_some() {
            return Err(AccessError::UnacknowledgedTask.into());
        }
        if self.state.mutation_submissions.len() >= 512 {
            return Err("guest start submission capacity exhausted".into());
        }
        let submission = request.submission;
        self.state
            .mutation_submissions
            .insert(submission, (digest, None));
        self.state.submission = Some(submission);
        let result = self.admit_selected_guest_mutation(request, approved_budget);
        let key = result.as_ref().ok().copied();
        self.state
            .mutation_submissions
            .insert(submission, (digest, key));
        self.state.submission = Some(submission);
        result
    }

    fn admit_selected_guest_mutation(
        &mut self,
        request: StartRequest,
        approved_budget: Option<MutationBudget>,
    ) -> Result<TaskKey> {
        let owner = self.local_state()?;
        let package = owner
            .manager
            .as_ref()
            .ok_or("catalog unavailable")?
            .installed_package(request.digest)?;
        if !package.mutation_enabled() {
            return Err("package did not negotiate mutation-v1".into());
        }
        let declaration = package
            .io_declaration()
            .and_then(|d| d.budget.as_ref())
            .ok_or("missing guest mutation IO declaration")?;
        if u64::from(request.timeout_ms) > declaration.max_duration_ms {
            return Err("guest mutation exceeds declared duration".into());
        }
        let limits = match (package.mutation_budget(), approved_budget) {
            (None, None) => morrow_plugin_runtime::io_jobs::JobLimits::new(
                8,
                declaration
                    .max_job_bytes
                    .min(morrow_plugin_runtime::io_jobs::MAX_JOB_BYTES),
                declaration
                    .max_bytes
                    .min(morrow_plugin_runtime::io_jobs::MAX_TOTAL_BYTES),
            )
            .map_err(|_| "invalid legacy guest mutation budget")?,
            (Some(declared), Some(approved))
                if approved.max_job_bytes <= declared.max_job_bytes
                    && approved.max_bytes <= declared.max_bytes =>
            {
                morrow_plugin_runtime::io_jobs::JobLimits::mutation(
                    8,
                    approved.max_job_bytes,
                    approved.max_bytes,
                )
                .map_err(|_| "invalid approved guest mutation budget")?
            }
            _ => return Err("guest mutation budget requires exact host approval".into()),
        };
        let selection = match request.relative_path {
            Some(relative) => Selection::Create {
                root: request.selected_path,
                relative,
            },
            None => Selection::Existing(request.selected_path),
        };
        let timeout = Duration::from_millis(u64::from(request.timeout_ms));
        self.start_task_with_binding(
            StartOptions {
                package_id: request.package_id,
                digest: request.digest,
                revision: request.revision,
                capabilities: BTreeSet::from([request.disposition.capability()]),
                lifetime: timeout,
                limits,
            },
            approved_budget.map_or(BindingProfile::Ordinary, BindingProfile::Mutation),
            move |_| {
                let mut secret = [0; 32];
                getrandom::fill(&mut secret)?;
                Ok(file::Admission::Mutation(Admission {
                    kind: AdmissionKind::GuestSelected {
                        selection,
                        scope: SelectionScope {
                            subject: request.subject,
                            approval_sha256: request.approval,
                            disposition: request.disposition,
                        },
                        secret,
                    },
                    timeout,
                }))
            },
        )
    }
}

#[cfg(test)]
#[path = "mutation_guest_tasks_tests.rs"]
mod tests;
