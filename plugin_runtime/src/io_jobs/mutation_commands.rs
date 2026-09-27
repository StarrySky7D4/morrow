//! Original-owner Windows file mutation commands. A queued command is never a
//! guest grant: the retained target, live binding, and Core history remain the
//! only sources of authority and truth.
use super::*;
use crate::{
    file_target::{self, SelectionScope, TargetBroker, TargetControl},
    io_binding::{self, IoBinding},
    manager::{ManagedInstance, Manager},
};
use morrow_core::{
    dispatch::HostRuntime,
    file_effect::{CreateOutcome, DeleteOutcome, ReplaceOutcome},
    file_mutation::{Disposition, RequestRecord},
    file_path::RelativeFilePath,
    io_evidence::Kind,
    io_intent::{Phase as IntentPhase, Record},
    mutation as guest_wire,
    plugin_package::io::IoCapability,
    store::{
        FileMutationPlanCheckpoint, FileMutationPlanCursor,
        MAX_FILE_MUTATION_PLAN_READ_BYTES_PER_CANDIDATE, MAX_FILE_MUTATION_PLAN_SCAN_LIMIT,
    },
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_MUTATION_SESSIONS: usize = 128;
const MAX_SPOOL_BYTES: usize = 64 * 1024 * 1024;
const MAX_MUTATION_CONTENT: u64 = 16 * 1024 * 1024;
// 16 MiB at 60 KiB/chunk needs 274 submissions, plus preparation,
// commit, execute, history and release. This is a worker-lifetime bound.
const MAX_GUEST_SUBMISSIONS: usize = 512;
const REPLY_RESERVATION: usize = 16 * 1024;
// Eight canonical plan containers, decoded request fields, and one bounded
// opaque checkpoint fit within this fixed, charged queue reply reservation.
const DISCOVERY_REPLY_RESERVATION: usize = 192 * 1024;
const MAX_DISCOVERY_CHECKPOINT_CHARGE: usize = 1024;
/// Leaves room for the fixed command and ticket within the 64 KiB input cap.
pub const MAX_MUTATION_CHUNK: usize = 60 * 1024;
static NEXT_MUTATION: AtomicU64 = AtomicU64::new(1);

/// Conservative extra cost of the standard, one-import-per-job mutation
/// recipe after IssueGuest has already been admitted. It includes Prepare,
/// every 60 KiB Chunk, Commit for Create, authorization, Execute, an observed
/// Query with both original-material reads, and Release. It excludes retries,
/// other jobs and already charged selection/plan/issue commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MutationBudgetEstimate {
    pub worker_bytes: u64,
    pub instance_bytes: u64,
    pub max_admission_bytes: u64,
    pub submissions: u16,
}
impl MutationBudgetEstimate {
    pub fn for_plan(plan: &RequestRecord) -> Result<Self, OwnerCommandError> {
        let request = plan.request();
        if !matches!(
            request.disposition,
            Disposition::Create | Disposition::Delete
        ) || request.content_length > MAX_MUTATION_CONTENT
        {
            return Err(OwnerCommandError::Limit);
        }
        let content = request.content_length;
        let container = plan.container().len() as u64;
        let chunks = if request.disposition == Disposition::Create {
            content.div_ceil(MAX_MUTATION_CHUNK as u64)
        } else {
            0
        };
        let submissions = chunks
            .checked_add(if request.disposition == Disposition::Create {
                5
            } else {
                4
            })
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| usize::from(*n) <= MAX_GUEST_SUBMISSIONS)
            .ok_or(OwnerCommandError::Limit)?;
        let frame = morrow_core::mutation::MAX_FRAME_BYTES as u64;
        let guest = u64::from(submissions)
            .checked_mul(frame.checked_mul(3).ok_or(OwnerCommandError::Limit)?)
            .ok_or(OwnerCommandError::Limit)?;
        let auth_queue = size_of::<Request>() as u64 + REPLY_RESERVATION as u64;
        let worker_bytes = guest
            .checked_add(content.checked_mul(6).ok_or(OwnerCommandError::Limit)?)
            .and_then(|n| n.checked_add(container.checked_mul(3)?))
            .and_then(|n| n.checked_add(2 * morrow_core::file_mutation::MAX_RESPONSE_BYTES))
            .and_then(|n| n.checked_add(auth_queue))
            .ok_or(OwnerCommandError::Limit)?;
        let instance_bytes = guest
            .checked_add(content.checked_mul(8).ok_or(OwnerCommandError::Limit)?)
            .and_then(|n| n.checked_add(container.checked_mul(5)?))
            .and_then(|n| n.checked_add(3 * morrow_core::file_mutation::MAX_RESPONSE_BYTES))
            .ok_or(OwnerCommandError::Limit)?;
        let max_admission_bytes = content
            .checked_add(container)
            .and_then(|n| n.checked_add(morrow_core::file_mutation::MAX_RESPONSE_BYTES))
            .ok_or(OwnerCommandError::Limit)?
            .max(frame * 3);
        Ok(Self {
            worker_bytes,
            instance_bytes,
            max_admission_bytes,
            submissions,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MutationSession {
    worker: u64,
    serial: u64,
}

/// Opaque original-worker handle for read-only durable plan discovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MutationDiscoverySession {
    worker: u64,
    serial: u64,
}

/// Receipt for one host-reviewed canonical plan on the original selected owner.
/// This value alone is not dispatch authority; the owner checks delivery and
/// the retained target again when authorizing and executing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MutationGuestLease {
    worker: u64,
    serial: u64,
    reference: [u8; 32],
    plan_sha256: [u8; 32],
    capability: morrow_core::plugin_package::io::IoCapability,
}
impl MutationGuestLease {
    pub(crate) fn worker(self) -> u64 {
        self.worker
    }
    pub(crate) fn capability(self) -> morrow_core::plugin_package::io::IoCapability {
        self.capability
    }
    pub fn reference(&self) -> [u8; 32] {
        self.reference
    }
    pub fn plan_sha256(&self) -> [u8; 32] {
        self.plan_sha256
    }
}
impl std::fmt::Debug for MutationGuestLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MutationGuestLease(..)")
    }
}

/// One-way host permission to attempt the effect of an exact prepared plan.
/// Only the original owner can consume its matching stored delivery receipt.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct MutationGuestExecutionPermit {
    lease: MutationGuestLease,
}

/// The host starts a staging job, reviews the durable plan, then starts a
/// separate execution job carrying the delivered one-shot permit.
#[derive(Clone, Copy, Debug)]
pub enum MutationGuestJobMode {
    Stage(MutationGuestLease),
    Execute(MutationGuestExecutionPermit),
}
impl MutationGuestJobMode {
    pub fn lease(self) -> MutationGuestLease {
        match self {
            Self::Stage(lease) => lease,
            Self::Execute(permit) => permit.lease,
        }
    }
}
impl MutationGuestExecutionPermit {
    pub fn lease(&self) -> MutationGuestLease {
        self.lease
    }
}
impl std::fmt::Debug for MutationGuestExecutionPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MutationGuestExecutionPermit(..)")
    }
}

pub struct MutationHandle {
    inner: OwnerCommandHandle,
}
impl MutationHandle {
    pub fn is_started(&self) -> bool {
        self.inner.is_started()
    }
    pub fn poll(&self) -> OwnerCommandPoll {
        self.inner.poll()
    }
    pub fn cancel(&self) {
        self.inner.cancel();
    }
    /// Outer uncertainty concerns delivery; inspect original history before
    /// inferring anything about a started persistent or native operation.
    pub fn read(
        &mut self,
    ) -> Result<Option<Result<MutationResponse, file_target::Error>>, OwnerCommandError> {
        match self.inner.read_reply()? {
            Some(Reply::Mutation(result)) => Ok(Some(*result)),
            Some(_) => Err(OwnerCommandError::Unknown),
            None => Ok(None),
        }
    }
}

pub enum MutationResponse {
    Selected {
        reference: [u8; 32],
        expected_identity: Option<[u8; 32]>,
    },
    Planned(RequestRecord),
    GuestApproved(MutationGuestLease),
    GuestExecutionAuthorized(MutationGuestExecutionPermit),
    Plans {
        plans: Vec<RequestRecord>,
        scanned: u32,
        done: bool,
        checkpoint: Option<FileMutationPlanCheckpoint>,
    },
    Prepared(Record),
    PlanCancelled(Record),
    Staged {
        bytes: u64,
        durable: bool,
    },
    Created(CreateOutcome),
    Deleted(DeleteOutcome),
    History {
        record: Option<Record>,
        staged_bytes: u64,
        durable_content: bool,
    },
    Reconciled {
        record: Option<Record>,
        outcome: Option<Box<MutationOutcome>>,
    },
    Released,
}
pub enum MutationOutcome {
    Created(CreateOutcome),
    Replaced(ReplaceOutcome),
    Deleted(DeleteOutcome),
}
impl std::fmt::Debug for MutationOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created(_) => f.write_str("Created(..)"),
            Self::Replaced(_) => f.write_str("Replaced(..)"),
            Self::Deleted(_) => f.write_str("Deleted(..)"),
        }
    }
}
impl std::fmt::Debug for MutationResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Selected {
                reference,
                expected_identity,
            } => f
                .debug_struct("Selected")
                .field("reference", reference)
                .field("expected_identity", expected_identity)
                .finish(),
            Self::Planned(request) => f.debug_tuple("Planned").field(request).finish(),
            Self::GuestApproved(_) => f.write_str("GuestApproved(..)"),
            Self::GuestExecutionAuthorized(_) => f.write_str("GuestExecutionAuthorized(..)"),
            Self::Plans {
                plans,
                scanned,
                done,
                checkpoint,
            } => f
                .debug_struct("Plans")
                .field("plans", plans)
                .field("scanned", scanned)
                .field("done", done)
                .field("checkpoint", &checkpoint.is_some())
                .finish(),
            Self::Prepared(record) => f.debug_tuple("Prepared").field(record).finish(),
            Self::PlanCancelled(record) => f.debug_tuple("PlanCancelled").field(record).finish(),
            Self::Staged { bytes, durable } => f
                .debug_struct("Staged")
                .field("bytes", bytes)
                .field("durable", durable)
                .finish(),
            Self::Created(_) => f.write_str("Created(..)"),
            Self::Deleted(_) => f.write_str("Deleted(..)"),
            Self::History {
                record,
                staged_bytes,
                durable_content,
            } => f
                .debug_struct("History")
                .field("record", record)
                .field("staged_bytes", staged_bytes)
                .field("durable_content", durable_content)
                .finish(),
            Self::Reconciled { record, outcome } => f
                .debug_struct("Reconciled")
                .field("record", record)
                .field("outcome", outcome)
                .finish(),
            Self::Released => f.write_str("Released"),
        }
    }
}

pub(super) type Dispatch<O> = fn(
    &mut O,
    Option<&crate::manager::ManagedInstance>,
    &Control,
    &Ticket,
    &mut Resources,
    Request,
    Option<Zeroizing<Vec<u8>>>,
) -> Result<MutationResponse, file_target::Error>;

pub(super) enum Request {
    SelectExisting {
        id: MutationSession,
        path: PathBuf,
        scope: SelectionScope,
        secret: [u8; 32],
    },
    SelectCreate {
        id: MutationSession,
        root: PathBuf,
        relative: RelativeFilePath,
        scope: SelectionScope,
        secret: [u8; 32],
    },
    BuildPlan {
        id: MutationSession,
        operation_id: String,
        content_length: u64,
        content_sha256: Option<[u8; 32]>,
    },
    IssueGuest {
        id: MutationSession,
        plan: Box<RequestRecord>,
        guest_reference: [u8; 32],
    },
    AuthorizeGuestExecution {
        id: MutationSession,
        plan_sha256: [u8; 32],
    },
    Prepare {
        id: MutationSession,
        request: RequestRecord,
    },
    Reconcile {
        request: RequestRecord,
    },
    OpenDiscovery {
        id: MutationDiscoverySession,
        subject: String,
        disposition: Disposition,
        scan_limit: u16,
        checkpoint: Option<FileMutationPlanCheckpoint>,
    },
    NextDiscovery {
        id: MutationDiscoverySession,
        scan_limit: u16,
    },
    CloseDiscovery {
        id: MutationDiscoverySession,
    },
    Chunk {
        id: MutationSession,
        offset: u64,
        len: usize,
        capacity: usize,
    },
    Commit {
        id: MutationSession,
    },
    Execute {
        id: MutationSession,
    },
    Query {
        id: MutationSession,
    },
    CancelPlan {
        id: MutationSession,
    },
    Release {
        id: MutationSession,
    },
}
impl Request {
    pub(super) fn is_history_only(&self) -> bool {
        matches!(
            self,
            Self::Reconcile { .. }
                | Self::OpenDiscovery { .. }
                | Self::NextDiscovery { .. }
                | Self::CloseDiscovery { .. }
        )
    }
    fn reply_reservation(&self) -> usize {
        match self {
            Self::OpenDiscovery { .. } | Self::NextDiscovery { .. } => DISCOVERY_REPLY_RESERVATION,
            _ => REPLY_RESERVATION,
        }
    }
    pub(super) fn charge(&self) -> u64 {
        size_of::<Request>() as u64
            + match self {
                Self::SelectExisting { path, scope, .. } => {
                    (path.capacity() + scope.subject.capacity() + REPLY_RESERVATION) as u64
                }
                Self::SelectCreate {
                    root,
                    relative,
                    scope,
                    ..
                } => {
                    (root.capacity()
                        + relative.as_str().len()
                        + scope.subject.capacity()
                        + REPLY_RESERVATION) as u64
                }
                Self::Prepare { request, .. } | Self::Reconcile { request } => {
                    (request.container().len()
                        + request.request().operation_id.len()
                        + request.request().subject.len()
                        + request
                            .request()
                            .target
                            .relative_path
                            .as_ref()
                            .map_or(0, |path| path.as_str().len())
                        + REPLY_RESERVATION) as u64
                }
                Self::IssueGuest { plan, .. } => {
                    (size_of::<RequestRecord>()
                        + plan.container().len()
                        + plan.request().operation_id.len()
                        + plan.request().subject.len()
                        + plan
                            .request()
                            .target
                            .relative_path
                            .as_ref()
                            .map_or(0, |path| path.as_str().len())
                        + REPLY_RESERVATION) as u64
                }
                Self::BuildPlan { operation_id, .. } => {
                    (operation_id.capacity() + REPLY_RESERVATION) as u64
                }
                Self::OpenDiscovery {
                    subject,
                    checkpoint,
                    ..
                } => {
                    (subject.capacity()
                        + checkpoint
                            .as_ref()
                            .map_or(0, |value| value.retained_bytes())
                        + DISCOVERY_REPLY_RESERVATION) as u64
                }
                Self::NextDiscovery { .. } => DISCOVERY_REPLY_RESERVATION as u64,
                Self::CloseDiscovery { .. } => REPLY_RESERVATION as u64,
                Self::Chunk { capacity, .. } => (*capacity + REPLY_RESERVATION) as u64,
                Self::Commit { .. }
                | Self::Execute { .. }
                | Self::AuthorizeGuestExecution { .. }
                | Self::CancelPlan { .. }
                | Self::Query { .. }
                | Self::Release { .. } => REPLY_RESERVATION as u64,
            }
    }
}

struct Resource {
    broker: TargetBroker,
    reference: [u8; 32],
    scope: SelectionScope,
    selection_status: Arc<Mutex<Status>>,
    request: Option<RequestRecord>,
    buffer: Zeroizing<Vec<u8>>,
    durable: bool,
    commit_attempted: bool,
    plan_cancelled: bool,
    guest: Option<GuestApproval>,
}
struct GuestApproval {
    plan: RequestRecord,
    lease: MutationGuestLease,
    issue_status: Arc<Mutex<Status>>,
    permit_status: Option<Arc<Mutex<Status>>>,
    permit_consumed: bool,
}
struct DiscoveryResource {
    id: MutationDiscoverySession,
    cursor: FileMutationPlanCursor,
    subject: String,
    disposition: Disposition,
    package_sha256: [u8; 32],
    page_status: Arc<Mutex<Status>>,
}
#[derive(Default)]
pub(super) struct Resources(
    BTreeMap<u64, Resource>,
    Option<DiscoveryResource>,
    // Bounded worker-lifetime tombstones. Releasing a selection never makes
    // its guest reference reusable; a fresh worker is needed after 128 issues.
    BTreeSet<[u8; 32]>,
    // Submission fingerprints and bounded replies survive Release for exact
    // receipt retrieval. A different call ID may request the same result.
    BTreeMap<[u8; 32], ([u8; 32], std::time::Instant, Option<guest_wire::Response>)>,
);
impl Resources {
    pub(super) fn reap_cancelled(&mut self) {
        self.0.retain(|_, resource| {
            let status = resource
                .selection_status
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            !status.cancelled || status.delivered
        });
        if self.1.as_ref().is_some_and(|resource| {
            let status = resource
                .page_status
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            status.cancelled && !status.delivered
        }) {
            self.1 = None;
        }
    }
    fn spool_capacity(&self) -> usize {
        self.0
            .values()
            .map(|resource| resource.buffer.capacity())
            .sum()
    }
}

struct CommandControl<'a> {
    control: &'a Control,
    source: DispatchSource<'a>,
    latched_cancel: bool,
}
#[derive(Clone, Copy)]
enum DispatchSource<'a> {
    Queued(&'a Ticket),
    Guest(&'a Cancellation),
}
impl<'a> DispatchSource<'a> {
    fn cancelled(self) -> bool {
        match self {
            Self::Queued(ticket) => ticket.lock().cancelled,
            Self::Guest(cancel) => cancel.fault().is_some(),
        }
    }
    fn queued(self) -> Result<&'a Ticket, file_target::Error> {
        match self {
            Self::Queued(ticket) => Ok(ticket),
            Self::Guest(_) => Err(file_target::Error::InvalidSelection),
        }
    }
}
impl TargetControl for CommandControl<'_> {
    fn with<T>(&mut self, action: impl FnOnce(u64, bool) -> T) -> T {
        // Match the original owner lock order. Never call Control::fault here:
        // it would take the clock a second time while state is held.
        let state = self.control.lock();
        let ticket_cancelled = self.source.cancelled();
        let authority = self.control.authority.as_ref().expect("mutation authority");
        self.latched_cancel |= state.phase != Phase::Running
            || self.control.revocation.is_revoked()
            || authority.cancellation.fault().is_some()
            || ticket_cancelled;
        let mut clock = authority.clock.lock().expect("original mutation clock");
        let result = action(clock(), self.latched_cancel);
        drop(clock);
        drop(state);
        result
    }
}

fn current(
    resources: &mut Resources,
    id: MutationSession,
) -> Result<&mut Resource, file_target::Error> {
    resources
        .0
        .get_mut(&id.serial)
        .ok_or(file_target::Error::Missing)
}
fn same_request(resource: &Resource, request: &RequestRecord) -> Result<(), file_target::Error> {
    if request.request().target.reference != resource.reference
        || request.request().subject != resource.scope.subject
        || request.request().approval_sha256 != resource.scope.approval_sha256
        || request.request().disposition != resource.scope.disposition
    {
        return Err(file_target::Error::Mismatch);
    }
    if let Some(bound) = &resource.request {
        if bound.container() != request.container() || bound.request() != request.request() {
            return Err(file_target::Error::Mismatch);
        }
    }
    if let Some(guest) = &resource.guest
        && (guest.plan.container() != request.container()
            || guest.plan.request() != request.request())
    {
        return Err(file_target::Error::Mismatch);
    }
    Ok(())
}

fn plan_sha256(plan: &RequestRecord) -> [u8; 32] {
    Sha256::digest(plan.container()).into()
}

#[allow(clippy::too_many_arguments)]
fn admit_history_read(
    binding: &IoBinding,
    manager: &Manager,
    host: &HostRuntime,
    instance: &ManagedInstance,
    capability: IoCapability,
    bytes: u64,
    now: u64,
) -> io_binding::Result<io_binding::IoLease> {
    if binding.is_mutation_history() {
        binding.admit_mutation_history_read(manager, host, instance, capability, bytes, now)
    } else {
        binding.admit(manager, host, instance, capability, 0, bytes, now)
    }
}

#[allow(clippy::too_many_arguments)]
fn read_discovery_page(
    manager: &Manager,
    host: &HostRuntime,
    instance: &ManagedInstance,
    binding: &IoBinding,
    control: &Control,
    gate: &mut CommandControl<'_>,
    cursor: &mut FileMutationPlanCursor,
    subject: &str,
    disposition: Disposition,
    scan_limit: u16,
) -> Result<MutationResponse, file_target::Error> {
    if !(1..=MAX_FILE_MUTATION_PLAN_SCAN_LIMIT).contains(&scan_limit) {
        return Err(file_target::Error::Limit);
    }
    let capability = disposition.capability();
    gate.with(|now, cancelled| {
        binding.validate_renewal_manager(manager, manager.revision())?;
        binding.preflight_capability(manager, host, instance, capability, now)?;
        if cancelled {
            return Err(file_target::Error::CancelledBeforeDispatch);
        }
        Ok(())
    })?;
    // Include the bounded keyset lookahead in this conservative reservation.
    let cost = (u64::from(scan_limit) + 1)
        .checked_mul(MAX_FILE_MUTATION_PLAN_READ_BYTES_PER_CANDIDATE as u64)
        .ok_or(file_target::Error::Limit)?;
    if cost > control.limits.max_job_bytes {
        return Err(file_target::Error::Limit);
    }
    control
        .charge(cost, &[], None)
        .map_err(|_| file_target::Error::Limit)?;
    let _read_job = gate.with(|now, cancelled| {
        if cancelled {
            return Err(file_target::Error::CancelledBeforeDispatch);
        }
        Ok(admit_history_read(
            binding, manager, host, instance, capability, cost, now,
        )?)
    })?;
    let mut rejection = None;
    let page = host
        .store_local()
        .read_file_mutation_plan_page(cursor, scan_limit, || {
            gate.with(|now, cancelled| {
                binding.validate_renewal_manager(manager, manager.revision())?;
                binding.check(manager, host, instance, now)?;
                if cancelled {
                    return Err(file_target::Error::CancelledBeforeDispatch);
                }
                Ok(())
            })
            .map_err(|error| {
                rejection = Some(error);
                morrow_core::Error::Invalid("inactive mutation discovery")
            })
        })
        .map_err(|error| rejection.unwrap_or(file_target::Error::Persistence(error)))?;
    if page.scanned > u32::from(scan_limit)
        || page.plans.len() > usize::from(scan_limit)
        || page.plans.iter().any(|plan| {
            let request = plan.request();
            request.subject != subject
                || request.package_sha256 != instance.package().package().digest()
                || request.disposition != disposition
        })
    {
        return Err(file_target::Error::Mismatch);
    }
    gate.with(|now, cancelled| {
        binding.validate_renewal_manager(manager, manager.revision())?;
        binding.check(manager, host, instance, now)?;
        if cancelled {
            return Err(file_target::Error::CancelledBeforeDispatch);
        }
        Ok(())
    })?;
    Ok(MutationResponse::Plans {
        plans: page.plans,
        scanned: page.scanned,
        done: page.done,
        checkpoint: page.checkpoint,
    })
}

impl<O: ManagedHostOwner> IoWorker<O> {
    pub fn select_mutation_existing(
        &self,
        path: PathBuf,
        scope: SelectionScope,
        secret: [u8; 32],
    ) -> Result<(MutationSession, MutationHandle), OwnerCommandError> {
        let path_len = path.as_os_str().len();
        if path_len > 32_760 || scope.subject.len() > 256 {
            return Err(OwnerCommandError::Limit);
        }
        // Copy only the validated spelling. A trusted caller may pass a tiny
        // PathBuf/String whose excess capacity would otherwise occupy the queue.
        let path = PathBuf::from(path.as_os_str());
        let mut scope = scope;
        scope.subject = scope.subject.as_str().to_owned();
        let retained_bytes = path.capacity();
        self.select_mutation(
            Request::SelectExisting {
                id: self.next_mutation_id()?,
                path,
                scope,
                secret,
            },
            retained_bytes,
        )
    }
    pub fn select_mutation_create(
        &self,
        root: PathBuf,
        relative: RelativeFilePath,
        scope: SelectionScope,
        secret: [u8; 32],
    ) -> Result<(MutationSession, MutationHandle), OwnerCommandError> {
        let length = root.as_os_str().len() + relative.as_str().len();
        if length > 32_760 || scope.subject.len() > 256 {
            return Err(OwnerCommandError::Limit);
        }
        let root = PathBuf::from(root.as_os_str());
        let relative =
            RelativeFilePath::parse(relative.as_str()).map_err(|_| OwnerCommandError::Limit)?;
        let mut scope = scope;
        scope.subject = scope.subject.as_str().to_owned();
        let retained_bytes = root.capacity() + relative.as_str().len();
        self.select_mutation(
            Request::SelectCreate {
                id: self.next_mutation_id()?,
                root,
                relative,
                scope,
                secret,
            },
            retained_bytes,
        )
    }
    fn next_mutation_id(&self) -> Result<MutationSession, OwnerCommandError> {
        if self.control.authority.is_none() {
            return Err(OwnerCommandError::Closed);
        }
        let serial = NEXT_MUTATION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| OwnerCommandError::Limit)?;
        Ok(MutationSession {
            worker: self.control.id,
            serial,
        })
    }
    fn select_mutation(
        &self,
        request: Request,
        path_len: usize,
    ) -> Result<(MutationSession, MutationHandle), OwnerCommandError> {
        let id = match &request {
            Request::SelectExisting { id, .. } | Request::SelectCreate { id, .. } => *id,
            _ => unreachable!(),
        };
        let input_bytes = size_of::<Request>()
            + path_len
            + match &request {
                Request::SelectExisting { scope, .. } | Request::SelectCreate { scope, .. } => {
                    scope.subject.capacity()
                }
                _ => 0,
            };
        let handle = self.mutation_command(request, input_bytes, None)?;
        Ok((id, handle))
    }
    pub fn prepare_mutation(
        &self,
        id: MutationSession,
        request: RequestRecord,
    ) -> Result<MutationHandle, OwnerCommandError> {
        if id.worker != self.control.id {
            return Err(OwnerCommandError::Closed);
        }
        let request =
            RequestRecord::decode(request.container()).map_err(|_| OwnerCommandError::Limit)?;
        let bytes = size_of::<Request>()
            + request.container().len()
            + request.request().operation_id.len()
            + request.request().subject.len()
            + request
                .request()
                .target
                .relative_path
                .as_ref()
                .map_or(0, |path| path.as_str().len());
        self.mutation_command(Request::Prepare { id, request }, bytes, None)
    }
    /// Construct the exact draft plan under the original selected owner.
    /// This does not persist preparation or change a filesystem target.
    pub fn build_mutation_plan(
        &self,
        id: MutationSession,
        operation_id: String,
        content_length: u64,
        content_sha256: Option<[u8; 32]>,
    ) -> Result<MutationHandle, OwnerCommandError> {
        if id.worker != self.control.id {
            return Err(OwnerCommandError::Closed);
        }
        if operation_id.is_empty()
            || operation_id.len() > 256
            || operation_id
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
            || content_length > MAX_MUTATION_CONTENT
            || content_length > self.control.limits.max_job_bytes
            || (content_length != 0 && content_sha256.is_none())
            || content_sha256 == Some([0; 32])
            || (content_length == 0
                && content_sha256.is_some()
                && content_sha256 != Some(Sha256::digest([]).into()))
        {
            return Err(OwnerCommandError::Limit);
        }
        // Copy into a bounded allocation before queuing: the caller may pass
        // a small String with arbitrarily large retained capacity.
        let operation_id = operation_id.as_str().to_owned();
        let bytes = size_of::<Request>() + operation_id.capacity();
        self.mutation_command(
            Request::BuildPlan {
                id,
                operation_id,
                content_length,
                content_sha256,
            },
            bytes,
            None,
        )
    }
    /// Bind a trusted host-reviewed canonical Create/Delete plan to the
    /// original selected owner. No persistence or file effect occurs here.
    /// The lease is inactive until the caller reads its successful receipt.
    pub fn issue_mutation_guest(
        &self,
        id: MutationSession,
        plan: RequestRecord,
        guest_reference: [u8; 32],
    ) -> Result<MutationHandle, OwnerCommandError> {
        if id.worker != self.control.id {
            return Err(OwnerCommandError::Closed);
        }
        if guest_reference == [0; 32] {
            return Err(OwnerCommandError::Limit);
        }
        let plan = RequestRecord::decode(plan.container()).map_err(|_| OwnerCommandError::Limit)?;
        if !matches!(
            plan.request().disposition,
            Disposition::Create | Disposition::Delete
        ) || plan.request().content_length > MAX_MUTATION_CONTENT
            || plan.request().content_length > self.control.limits.max_job_bytes
        {
            return Err(OwnerCommandError::Limit);
        }
        let bytes = size_of::<Request>()
            + size_of::<RequestRecord>()
            + plan.container().len()
            + plan.request().operation_id.len()
            + plan.request().subject.len()
            + plan
                .request()
                .target
                .relative_path
                .as_ref()
                .map_or(0, |path| path.as_str().len());
        self.mutation_command(
            Request::IssueGuest {
                id,
                plan: Box::new(plan),
                guest_reference,
            },
            bytes,
            None,
        )
    }
    /// A separate explicit host review after preparation and durable content
    /// staging. Delivery permits only one attempt on this exact original plan.
    pub fn authorize_mutation_guest_execution(
        &self,
        id: MutationSession,
        exact_plan_sha256: [u8; 32],
    ) -> Result<MutationHandle, OwnerCommandError> {
        if id.worker != self.control.id {
            return Err(OwnerCommandError::Closed);
        }
        if exact_plan_sha256 == [0; 32] {
            return Err(OwnerCommandError::Limit);
        }
        self.mutation_command(
            Request::AuthorizeGuestExecution {
                id,
                plan_sha256: exact_plan_sha256,
            },
            size_of::<Request>(),
            None,
        )
    }
    /// Read the exact original plan and any durable outcome under this worker's
    /// current managed authority. This never reopens or selects a file target.
    pub fn reconcile_mutation(
        &self,
        request: RequestRecord,
    ) -> Result<MutationHandle, OwnerCommandError> {
        let request =
            RequestRecord::decode(request.container()).map_err(|_| OwnerCommandError::Limit)?;
        let bytes = size_of::<Request>()
            + request.container().len()
            + request.request().operation_id.len()
            + request.request().subject.len()
            + request
                .request()
                .target
                .relative_path
                .as_ref()
                .map_or(0, |path| path.as_str().len());
        self.mutation_command(Request::Reconcile { request }, bytes, None)
    }
    /// Open the original owner's bounded, read-only plan discovery cursor and
    /// return its first page. The package digest is taken from the live instance.
    pub fn open_mutation_discovery(
        &self,
        subject: String,
        disposition: Disposition,
        scan_limit: u16,
    ) -> Result<(MutationDiscoverySession, MutationHandle), OwnerCommandError> {
        self.open_mutation_discovery_from(subject, disposition, scan_limit, None)
    }
    /// Start a new worker's discovery from a prior successful nonterminal page.
    /// The checkpoint grants no authority and is checked against this Store and
    /// the live package and scope by Core before the first read.
    pub fn open_mutation_discovery_from(
        &self,
        subject: String,
        disposition: Disposition,
        scan_limit: u16,
        checkpoint: Option<FileMutationPlanCheckpoint>,
    ) -> Result<(MutationDiscoverySession, MutationHandle), OwnerCommandError> {
        if subject.is_empty()
            || subject.len() > 256
            || subject
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
            || !(1..=MAX_FILE_MUTATION_PLAN_SCAN_LIMIT).contains(&scan_limit)
            || checkpoint
                .as_ref()
                .is_some_and(|value| value.retained_bytes() > MAX_DISCOVERY_CHECKPOINT_CHARGE)
        {
            return Err(OwnerCommandError::Limit);
        }
        let id = self.next_mutation_id()?;
        let id = MutationDiscoverySession {
            worker: id.worker,
            serial: id.serial,
        };
        let subject = subject.as_str().to_owned();
        let bytes = size_of::<Request>()
            + subject.capacity()
            + checkpoint
                .as_ref()
                .map_or(0, |value| value.retained_bytes());
        let handle = self.mutation_command(
            Request::OpenDiscovery {
                id,
                subject,
                disposition,
                scan_limit,
                checkpoint,
            },
            bytes,
            None,
        )?;
        Ok((id, handle))
    }
    pub fn next_mutation_plans(
        &self,
        id: MutationDiscoverySession,
        scan_limit: u16,
    ) -> Result<MutationHandle, OwnerCommandError> {
        if !(1..=MAX_FILE_MUTATION_PLAN_SCAN_LIMIT).contains(&scan_limit) {
            return Err(OwnerCommandError::Limit);
        }
        self.mutation_command(
            Request::NextDiscovery { id, scan_limit },
            size_of::<Request>(),
            None,
        )
    }
    pub fn close_mutation_discovery(
        &self,
        id: MutationDiscoverySession,
    ) -> Result<MutationHandle, OwnerCommandError> {
        self.mutation_command(Request::CloseDiscovery { id }, size_of::<Request>(), None)
    }
    pub fn stage_mutation_chunk(
        &self,
        id: MutationSession,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<MutationHandle, OwnerCommandError> {
        let bytes = Zeroizing::new(bytes);
        if id.worker != self.control.id {
            return Err(OwnerCommandError::Closed);
        }
        // The trusted caller can supply a tiny len with a huge Vec allocation.
        // Status.input retains the full allocation until cancellation/dequeue.
        if bytes.len() > MAX_MUTATION_CHUNK || bytes.capacity() > MAX_MUTATION_CHUNK {
            return Err(OwnerCommandError::Limit);
        }
        let len = bytes.len();
        let capacity = bytes.capacity();
        self.mutation_command(
            Request::Chunk {
                id,
                offset,
                len,
                capacity,
            },
            size_of::<Request>() + capacity,
            Some(bytes),
        )
    }
    pub fn commit_mutation_content(
        &self,
        id: MutationSession,
    ) -> Result<MutationHandle, OwnerCommandError> {
        self.mutation_command(Request::Commit { id }, size_of::<Request>(), None)
    }
    pub fn execute_mutation(
        &self,
        id: MutationSession,
    ) -> Result<MutationHandle, OwnerCommandError> {
        self.mutation_command(Request::Execute { id }, size_of::<Request>(), None)
    }
    /// Persist cancellation of the exact prepared plan. This is separate from
    /// cancelling a command reply or releasing local target resources. A lost
    /// reply requires history reconciliation; it never permits an effect retry.
    pub fn cancel_mutation_plan(
        &self,
        id: MutationSession,
    ) -> Result<MutationHandle, OwnerCommandError> {
        self.mutation_command(Request::CancelPlan { id }, size_of::<Request>(), None)
    }
    pub fn query_mutation(&self, id: MutationSession) -> Result<MutationHandle, OwnerCommandError> {
        self.mutation_command(Request::Query { id }, size_of::<Request>(), None)
    }
    pub fn release_mutation(
        &self,
        id: MutationSession,
    ) -> Result<MutationHandle, OwnerCommandError> {
        self.mutation_command(Request::Release { id }, size_of::<Request>(), None)
    }
    fn mutation_command(
        &self,
        request: Request,
        input_bytes: usize,
        input: Option<Zeroizing<Vec<u8>>>,
    ) -> Result<MutationHandle, OwnerCommandError> {
        let foreign = match &request {
            Request::SelectExisting { id, .. }
            | Request::SelectCreate { id, .. }
            | Request::BuildPlan { id, .. }
            | Request::IssueGuest { id, .. }
            | Request::AuthorizeGuestExecution { id, .. }
            | Request::Prepare { id, .. }
            | Request::Chunk { id, .. }
            | Request::Commit { id }
            | Request::Execute { id }
            | Request::CancelPlan { id }
            | Request::Query { id }
            | Request::Release { id } => id.worker != self.control.id,
            Request::OpenDiscovery { id, .. }
            | Request::NextDiscovery { id, .. }
            | Request::CloseDiscovery { id } => id.worker != self.control.id,
            Request::Reconcile { .. } => false,
        };
        // Reject a foreign worker before enqueue_owner_command samples time.
        if foreign {
            return Err(OwnerCommandError::Closed);
        }
        if input_bytes > MAX_OWNER_COMMAND_INPUT {
            return Err(OwnerCommandError::Limit);
        }
        let reply_reservation = request.reply_reservation();
        let inner = self.enqueue_owner_command(
            CommandKind::Mutation {
                request,
                dispatch: dispatch::<O>,
            },
            input_bytes + reply_reservation,
            input,
        )?;
        Ok(MutationHandle { inner })
    }
}

fn dispatch<O: ManagedHostOwner>(
    owner: &mut O,
    instance: Option<&crate::manager::ManagedInstance>,
    control: &Control,
    ticket: &Ticket,
    resources: &mut Resources,
    request: Request,
    input: Option<Zeroizing<Vec<u8>>>,
) -> Result<MutationResponse, file_target::Error> {
    dispatch_with_source(
        owner,
        instance,
        control,
        DispatchSource::Queued(ticket),
        resources,
        request,
        input,
    )
}

fn dispatch_with_source<O: ManagedHostOwner>(
    owner: &mut O,
    instance: Option<&ManagedInstance>,
    control: &Control,
    source: DispatchSource<'_>,
    resources: &mut Resources,
    request: Request,
    input: Option<Zeroizing<Vec<u8>>>,
) -> Result<MutationResponse, file_target::Error> {
    if matches!(source, DispatchSource::Guest(_))
        && !matches!(
            request,
            Request::Reconcile { .. }
                | Request::Prepare { .. }
                | Request::Chunk { .. }
                | Request::Commit { .. }
                | Request::Execute { .. }
                | Request::Query { .. }
                | Request::CancelPlan { .. }
                | Request::Release { .. }
        )
    {
        return Err(file_target::Error::InvalidSelection);
    }
    resources.reap_cancelled();
    let instance = instance.ok_or(file_target::Error::InvalidSelection)?;
    let authority = control
        .authority
        .as_ref()
        .ok_or(file_target::Error::Admission(io_binding::Error::Denied))?;
    // A history binding cannot acquire a target resource or mutate persistent
    // state, even if a caller bypassed the queue's initial allow-list.
    if authority.binding.is_mutation_history() && !request.is_history_only() {
        return Err(file_target::Error::Admission(io_binding::Error::Denied));
    }
    let mut gate = CommandControl {
        control,
        source,
        latched_cancel: false,
    };
    owner
        .with_managed_runtime(|manager, host| {
            // An opted-in owner must still be the original runtime. Do this before
            // clock sampling, Store access, or any OS selection/effect.
            if host.binding() != control.host {
                return Err(file_target::Error::Mismatch);
            }
            match request {
                Request::OpenDiscovery {
                    id,
                    subject,
                    disposition,
                    scan_limit,
                    checkpoint,
                } => {
                    if id.worker != control.id || resources.1.is_some() {
                        return Err(file_target::Error::Limit);
                    }
                    let package_sha256 = instance.package().package().digest();
                    gate.with(|now, cancelled| {
                        authority
                            .binding
                            .validate_renewal_manager(manager, manager.revision())?;
                        authority.binding.preflight_capability(
                            manager,
                            host,
                            instance,
                            disposition.capability(),
                            now,
                        )?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    // Core open holds no SQL snapshot. The first actual read is
                    // charged and admitted by read_discovery_page before SQL.
                    let mut cursor = match checkpoint {
                        Some(checkpoint) => host.store_local().resume_file_mutation_plan_cursor(
                            &checkpoint,
                            &subject,
                            package_sha256,
                            disposition,
                        )?,
                        None => host.store_local().open_file_mutation_plan_cursor(
                            &subject,
                            package_sha256,
                            disposition,
                        )?,
                    };
                    let page = read_discovery_page(
                        manager,
                        host,
                        instance,
                        &authority.binding,
                        control,
                        &mut gate,
                        &mut cursor,
                        &subject,
                        disposition,
                        scan_limit,
                    )?;
                    if source.queued()?.lock().cancelled {
                        return Err(file_target::Error::CancelledBeforeDispatch);
                    }
                    resources.1 = Some(DiscoveryResource {
                        id,
                        cursor,
                        subject,
                        disposition,
                        package_sha256,
                        page_status: Arc::clone(&source.queued()?.status),
                    });
                    Ok(page)
                }
                Request::NextDiscovery { id, scan_limit } => {
                    let resource = resources
                        .1
                        .as_mut()
                        .filter(|resource| resource.id == id)
                        .ok_or(file_target::Error::Missing)?;
                    let prior = resource
                        .page_status
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    if !prior.delivered {
                        return Err(file_target::Error::Busy);
                    }
                    drop(prior);
                    if resource.package_sha256 != instance.package().package().digest() {
                        resources.1 = None;
                        return Err(file_target::Error::Mismatch);
                    }
                    let result = read_discovery_page(
                        manager,
                        host,
                        instance,
                        &authority.binding,
                        control,
                        &mut gate,
                        &mut resource.cursor,
                        &resource.subject,
                        resource.disposition,
                        scan_limit,
                    );
                    match result {
                        Ok(page) if !source.queued()?.lock().cancelled => {
                            resource.page_status = Arc::clone(&source.queued()?.status);
                            Ok(page)
                        }
                        Ok(_) => {
                            resources.1 = None;
                            Err(file_target::Error::CancelledBeforeDispatch)
                        }
                        Err(error) => {
                            resources.1 = None;
                            Err(error)
                        }
                    }
                }
                Request::CloseDiscovery { id } => {
                    if resources
                        .1
                        .as_ref()
                        .is_none_or(|resource| resource.id != id)
                    {
                        return Err(file_target::Error::Missing);
                    }
                    resources.1 = None;
                    Ok(MutationResponse::Released)
                }
                Request::Reconcile { request } => {
                    // The request has no package ID or registry revision field.
                    // Its digest must identify this instance's package, and the
                    // existing binding must still be approved at the manager's
                    // current registry revision when this command runs.
                    if request.request().package_sha256 != instance.package().package().digest() {
                        return Err(file_target::Error::Mismatch);
                    }
                    gate.with(|now, cancelled| {
                        authority
                            .binding
                            .validate_renewal_manager(manager, manager.revision())?;
                        authority.binding.preflight_capability(
                            manager,
                            host,
                            instance,
                            request.request().disposition.capability(),
                            now,
                        )?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    // Matching-history verification can decode the original
                    // staged body, request material, and response. Bound and
                    // charge their combined size before entering Store.
                    if request.request().content_length > MAX_MUTATION_CONTENT {
                        return Err(file_target::Error::Limit);
                    }
                    if authority
                        .binding
                        .mutation_history_content_ceiling()
                        .is_some_and(|limit| request.request().content_length > limit)
                    {
                        return Err(file_target::Error::Limit);
                    }
                    let cost = request
                        .request()
                        .content_length
                        .checked_add(request.container().len() as u64)
                        .and_then(|n| n.checked_add(morrow_core::file_mutation::MAX_RESPONSE_BYTES))
                        .ok_or(file_target::Error::Limit)?;
                    if cost > control.limits.max_job_bytes {
                        return Err(file_target::Error::Limit);
                    }
                    control
                        .charge(cost, &[], None)
                        .map_err(|_| file_target::Error::Limit)?;
                    let read_job = gate.with(|now, cancelled| {
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(admit_history_read(
                            &authority.binding,
                            manager,
                            host,
                            instance,
                            request.request().disposition.capability(),
                            cost,
                            now,
                        )?)
                    })?;
                    let command = request.command()?;
                    let record = host.store_local().lookup_matching_io_intent(&command)?;
                    drop(read_job);
                    let outcome = if let Some(observed) = record
                        .as_ref()
                        .filter(|r| r.phase() == IntentPhase::Observed)
                    {
                        // io_material verifies the full history again, including
                        // the protected content. Charge this second read too.
                        control
                            .charge(cost, &[], None)
                            .map_err(|_| file_target::Error::Limit)?;
                        let _material_job = gate.with(|now, cancelled| {
                            if cancelled {
                                return Err(file_target::Error::CancelledBeforeDispatch);
                            }
                            Ok(admit_history_read(
                                &authority.binding,
                                manager,
                                host,
                                instance,
                                request.request().disposition.capability(),
                                cost,
                                now,
                            )?)
                        })?;
                        let material = host
                            .store_local()
                            .io_material(
                                &request.request().subject,
                                &request.request().operation_id,
                                Kind::Response,
                            )?
                            .ok_or(file_target::Error::Missing)?;
                        if material.digest().as_slice() != observed.data().observation_sha256 {
                            return Err(file_target::Error::Mismatch);
                        }
                        Some(match request.request().disposition {
                            Disposition::Create => {
                                MutationOutcome::Created(CreateOutcome::decode(material.payload())?)
                            }
                            Disposition::Replace => MutationOutcome::Replaced(
                                ReplaceOutcome::decode(material.payload())?,
                            ),
                            Disposition::Delete => {
                                MutationOutcome::Deleted(DeleteOutcome::decode(material.payload())?)
                            }
                        })
                    } else {
                        None
                    };
                    gate.with(|now, cancelled| {
                        authority
                            .binding
                            .validate_renewal_manager(manager, manager.revision())?;
                        authority.binding.check(manager, host, instance, now)?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    Ok(MutationResponse::Reconciled {
                        record,
                        outcome: outcome.map(Box::new),
                    })
                }
                Request::SelectExisting {
                    id,
                    path,
                    scope,
                    secret,
                } => {
                    if id.worker != control.id || resources.0.len() >= MAX_MUTATION_SESSIONS {
                        return Err(file_target::Error::Limit);
                    }
                    let mut broker = TargetBroker::new(secret)?;
                    let selected = broker.select_existing_controlled(
                        manager,
                        host,
                        instance,
                        &authority.binding,
                        &path,
                        scope.clone(),
                        &mut gate,
                    )?;
                    if source.queued()?.lock().cancelled {
                        return Err(file_target::Error::CancelledBeforeDispatch);
                    }
                    resources.0.insert(
                        id.serial,
                        Resource {
                            broker,
                            reference: selected.reference,
                            scope,
                            selection_status: Arc::clone(&source.queued()?.status),
                            request: None,
                            buffer: Zeroizing::new(Vec::new()),
                            durable: false,
                            commit_attempted: false,
                            plan_cancelled: false,
                            guest: None,
                        },
                    );
                    Ok(MutationResponse::Selected {
                        reference: selected.reference,
                        expected_identity: Some(selected.expected_identity),
                    })
                }
                Request::SelectCreate {
                    id,
                    root,
                    relative,
                    scope,
                    secret,
                } => {
                    if id.worker != control.id || resources.0.len() >= MAX_MUTATION_SESSIONS {
                        return Err(file_target::Error::Limit);
                    }
                    let mut broker = TargetBroker::new(secret)?;
                    let selected = broker.select_create_controlled(
                        manager,
                        host,
                        instance,
                        &authority.binding,
                        &root,
                        &relative,
                        scope.clone(),
                        &mut gate,
                    )?;
                    if source.queued()?.lock().cancelled {
                        return Err(file_target::Error::CancelledBeforeDispatch);
                    }
                    resources.0.insert(
                        id.serial,
                        Resource {
                            broker,
                            reference: selected.reference,
                            scope,
                            selection_status: Arc::clone(&source.queued()?.status),
                            request: None,
                            buffer: Zeroizing::new(Vec::new()),
                            durable: false,
                            commit_attempted: false,
                            plan_cancelled: false,
                            guest: None,
                        },
                    );
                    Ok(MutationResponse::Selected {
                        reference: selected.reference,
                        expected_identity: None,
                    })
                }
                Request::IssueGuest {
                    id,
                    plan,
                    guest_reference,
                } => {
                    if id.worker != control.id
                        || guest_reference == [0; 32]
                        || resources.2.contains(&guest_reference)
                        || resources.2.len() >= MAX_MUTATION_SESSIONS
                    {
                        return Err(file_target::Error::Limit);
                    }
                    let used_submissions = resources.3.len();
                    let resource = current(resources, id)?;
                    if resource.guest.is_some()
                        || resource.request.is_some()
                        || resource.plan_cancelled
                        || guest_reference == resource.reference
                    {
                        return Err(file_target::Error::AlreadyDispatched);
                    }
                    if !resource
                        .selection_status
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .delivered
                    {
                        return Err(file_target::Error::Busy);
                    }
                    if !matches!(
                        plan.request().disposition,
                        Disposition::Create | Disposition::Delete
                    ) || plan.request().content_length > MAX_MUTATION_CONTENT
                    {
                        return Err(file_target::Error::Limit);
                    }
                    gate.with(|now, cancelled| {
                        authority
                            .binding
                            .validate_renewal_manager(manager, manager.revision())?;
                        authority.binding.preflight_capability(
                            manager,
                            host,
                            instance,
                            resource.scope.disposition.capability(),
                            now,
                        )?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    let canonical = resource.broker.build_request_controlled(
                        manager,
                        host,
                        instance,
                        resource.reference,
                        plan.request().operation_id.clone(),
                        plan.request().content_length,
                        plan.request().content_sha256,
                        &mut gate,
                    )?;
                    same_request(resource, &plan)?;
                    if canonical.container() != plan.container()
                        || canonical.request() != plan.request()
                    {
                        return Err(file_target::Error::Mismatch);
                    }
                    // A mutation-budget package promises a complete standard
                    // recipe only when both current cumulative ledgers have a
                    // conservative snapshot of room. This is no reservation:
                    // every later admission still checks live authority,
                    // cancellation, and exact available bytes.
                    if let Some(budget) = authority.binding.mutation_budget() {
                        let estimate = MutationBudgetEstimate::for_plan(&canonical)
                            .map_err(|_| file_target::Error::Limit)?;
                        if used_submissions
                            .checked_add(usize::from(estimate.submissions))
                            .is_none_or(|n| n > MAX_GUEST_SUBMISSIONS)
                            || estimate.max_admission_bytes > control.limits.max_job_bytes
                            || estimate.max_admission_bytes > budget.max_job_bytes
                        {
                            return Err(file_target::Error::Limit);
                        }
                        let worker_used = control.lock().bytes;
                        let (instance_used, instance_ceiling) = authority
                            .binding
                            .mutation_budget_usage()
                            .ok_or(file_target::Error::Admission(io_binding::Error::Denied))?;
                        if worker_used
                            .checked_add(estimate.worker_bytes)
                            .is_none_or(|n| n > control.limits.max_total_bytes)
                            || instance_used
                                .checked_add(estimate.instance_bytes)
                                .is_none_or(|n| n > instance_ceiling)
                        {
                            return Err(file_target::Error::Limit);
                        }
                    }
                    if source.queued()?.lock().cancelled {
                        return Err(file_target::Error::CancelledBeforeDispatch);
                    }
                    let lease = MutationGuestLease {
                        worker: id.worker,
                        serial: id.serial,
                        reference: guest_reference,
                        plan_sha256: plan_sha256(&canonical),
                        capability: canonical.request().disposition.capability(),
                    };
                    resource.guest = Some(GuestApproval {
                        plan: canonical,
                        lease,
                        issue_status: Arc::clone(&source.queued()?.status),
                        permit_status: None,
                        permit_consumed: false,
                    });
                    resources.2.insert(guest_reference);
                    Ok(MutationResponse::GuestApproved(lease))
                }
                Request::AuthorizeGuestExecution { id, plan_sha256 } => {
                    let resource = current(resources, id)?;
                    let guest = resource.guest.as_ref().ok_or(file_target::Error::Missing)?;
                    if guest.lease.worker != id.worker
                        || guest.lease.serial != id.serial
                        || guest.lease.plan_sha256 != plan_sha256
                    {
                        return Err(file_target::Error::Mismatch);
                    }
                    if !guest
                        .issue_status
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .delivered
                    {
                        return Err(file_target::Error::Busy);
                    }
                    if guest.permit_status.is_some() || guest.permit_consumed {
                        return Err(file_target::Error::AlreadyDispatched);
                    }
                    let plan = resource
                        .request
                        .as_ref()
                        .ok_or(file_target::Error::Missing)?;
                    same_request(resource, plan)?;
                    if resource.plan_cancelled
                        || (plan.request().disposition == Disposition::Create && !resource.durable)
                    {
                        return Err(file_target::Error::Missing);
                    }
                    gate.with(|now, cancelled| {
                        authority
                            .binding
                            .validate_renewal_manager(manager, manager.revision())?;
                        authority.binding.preflight_capability(
                            manager,
                            host,
                            instance,
                            plan.request().disposition.capability(),
                            now,
                        )?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    resource
                        .broker
                        .validate_request_controlled(manager, host, instance, plan, &mut gate)?;
                    let cost = plan
                        .request()
                        .content_length
                        .checked_add(plan.container().len() as u64)
                        .ok_or(file_target::Error::Limit)?;
                    if cost > control.limits.max_job_bytes {
                        return Err(file_target::Error::Limit);
                    }
                    control
                        .charge(cost, &[], None)
                        .map_err(|_| file_target::Error::Limit)?;
                    let _read_job = gate.with(|now, cancelled| {
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(authority.binding.admit(
                            manager,
                            host,
                            instance,
                            plan.request().disposition.capability(),
                            0,
                            cost,
                            now,
                        )?)
                    })?;
                    let command = plan.command()?;
                    let record = host
                        .store_local()
                        .lookup_matching_io_intent(&command)?
                        .ok_or(file_target::Error::Missing)?;
                    if record.phase() != IntentPhase::Prepared {
                        return Err(file_target::Error::AlreadyDispatched);
                    }
                    if plan.request().disposition == Disposition::Create {
                        let mut rejected = None;
                        let content = host
                            .store_local()
                            .file_mutation_content_local_authorized(
                                &plan.request().subject,
                                &plan.request().operation_id,
                                || {
                                    gate.with(|now, cancelled| {
                                        authority.binding.check_liveness(now)?;
                                        if cancelled {
                                            return Err(
                                                file_target::Error::CancelledBeforeDispatch,
                                            );
                                        }
                                        Ok(())
                                    })
                                    .map_err(|error| {
                                        rejected = Some(error);
                                        morrow_core::Error::Invalid("inactive guest content review")
                                    })
                                },
                            )
                            .map_err(|error| {
                                rejected.unwrap_or(file_target::Error::Persistence(error))
                            })?
                            .ok_or(file_target::Error::Missing)?;
                        if content.request_sha256() != command.request_sha256
                            || content.content_sha256()
                                != plan.request().content_sha256.unwrap_or([0; 32])
                            || content.content().len() as u64 != plan.request().content_length
                        {
                            return Err(file_target::Error::Mismatch);
                        }
                    }
                    gate.with(|now, cancelled| {
                        authority.binding.check(manager, host, instance, now)?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    if source.queued()?.lock().cancelled {
                        return Err(file_target::Error::CancelledBeforeDispatch);
                    }
                    let guest = resource.guest.as_mut().ok_or(file_target::Error::Missing)?;
                    guest.permit_status = Some(Arc::clone(&source.queued()?.status));
                    Ok(MutationResponse::GuestExecutionAuthorized(
                        MutationGuestExecutionPermit { lease: guest.lease },
                    ))
                }
                Request::BuildPlan {
                    id,
                    operation_id,
                    content_length,
                    content_sha256,
                } => {
                    let resource = current(resources, id)?;
                    if resource.plan_cancelled {
                        return Err(file_target::Error::Persistence(
                            morrow_core::Error::RevisionConflict,
                        ));
                    }
                    let plan = resource.broker.build_request_controlled(
                        manager,
                        host,
                        instance,
                        resource.reference,
                        operation_id,
                        content_length,
                        content_sha256,
                        &mut gate,
                    )?;
                    same_request(resource, &plan)?;
                    Ok(MutationResponse::Planned(plan))
                }
                Request::Prepare { id, request } => {
                    let resource = current(resources, id)?;
                    same_request(resource, &request)?;
                    if request.request().content_length > MAX_MUTATION_CONTENT {
                        return Err(file_target::Error::Limit);
                    }
                    resource.broker.validate_request_controlled(
                        manager, host, instance, &request, &mut gate,
                    )?;
                    let content_bytes = request.request().content_length;
                    if content_bytes != 0 {
                        if content_bytes > control.limits.max_job_bytes {
                            return Err(file_target::Error::Limit);
                        }
                        control
                            .charge(content_bytes, &[], None)
                            .map_err(|_| file_target::Error::Limit)?;
                        // The matching-history path may decode a previously
                        // staged body. Reserve its size before entering Store,
                        // then release the concurrent slot before preparation.
                        let memory_job = gate.with(|now, cancelled| {
                            if cancelled {
                                return Err(file_target::Error::CancelledBeforeDispatch);
                            }
                            Ok(authority.binding.admit(
                                manager,
                                host,
                                instance,
                                request.request().disposition.capability(),
                                0,
                                content_bytes,
                                now,
                            )?)
                        })?;
                        drop(memory_job);
                    }
                    // Bind before Store commit. A lost preparation reply can query
                    // the exact original plan without guessing whether it committed.
                    if resource.request.is_none() {
                        resource.request = Some(request.clone());
                    }
                    let record = resource
                        .broker
                        .prepare_request_controlled(manager, host, instance, &request, &mut gate)?;
                    Ok(MutationResponse::Prepared(record))
                }
                Request::Chunk {
                    id,
                    offset,
                    len,
                    capacity,
                } => {
                    let bytes = input.ok_or(file_target::Error::InvalidSelection)?;
                    if bytes.len() != len
                        || bytes.capacity() != capacity
                        || capacity > MAX_MUTATION_CHUNK
                    {
                        return Err(file_target::Error::Limit);
                    }
                    let total = resources.spool_capacity();
                    let resource = current(resources, id)?;
                    let plan = resource
                        .request
                        .as_ref()
                        .ok_or(file_target::Error::Missing)?;
                    let expected = plan.request().content_length;
                    if !matches!(
                        plan.request().disposition,
                        Disposition::Create | Disposition::Replace
                    ) || expected > MAX_MUTATION_CONTENT
                        || resource.durable
                        || resource.plan_cancelled
                        || resource.commit_attempted
                        || offset != resource.buffer.len() as u64
                        || offset
                            .checked_add(len as u64)
                            .is_none_or(|end| end > expected)
                    {
                        return Err(file_target::Error::Mismatch);
                    }
                    resource
                        .broker
                        .validate_request_controlled(manager, host, instance, plan, &mut gate)?;
                    if resource.buffer.capacity() == 0 && expected != 0 {
                        let expected =
                            usize::try_from(expected).map_err(|_| file_target::Error::Limit)?;
                        if total
                            .checked_add(expected)
                            .is_none_or(|n| n > MAX_SPOOL_BYTES)
                        {
                            return Err(file_target::Error::Limit);
                        }
                        if expected as u64 > control.limits.max_job_bytes {
                            return Err(file_target::Error::Limit);
                        }
                        control
                            .charge(expected as u64, &[], None)
                            .map_err(|_| file_target::Error::Limit)?;
                        // Reserve original-instance bytes before allocating the full
                        // expected resident spool. Core staging later charges its own
                        // separate durable material cost.
                        let _memory_job = gate.with(|now, cancelled| {
                            if cancelled {
                                return Err(file_target::Error::CancelledBeforeDispatch);
                            }
                            Ok(authority.binding.admit(
                                manager,
                                host,
                                instance,
                                plan.request().disposition.capability(),
                                0,
                                expected as u64,
                                now,
                            )?)
                        })?;
                        resource
                            .buffer
                            .try_reserve_exact(expected)
                            .map_err(|_| file_target::Error::Limit)?;
                        if total
                            .checked_add(resource.buffer.capacity())
                            .is_none_or(|n| n > MAX_SPOOL_BYTES)
                        {
                            resource.buffer = Zeroizing::new(Vec::new());
                            return Err(file_target::Error::Limit);
                        }
                    }
                    resource.buffer.extend_from_slice(&bytes);
                    gate.with(|now, cancelled| {
                        authority.binding.check(manager, host, instance, now)?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    Ok(MutationResponse::Staged {
                        bytes: resource.buffer.len() as u64,
                        durable: false,
                    })
                }
                Request::Commit { id } => {
                    let resource = current(resources, id)?;
                    if resource.plan_cancelled {
                        return Err(file_target::Error::Persistence(
                            morrow_core::Error::RevisionConflict,
                        ));
                    }
                    let plan = resource
                        .request
                        .as_ref()
                        .ok_or(file_target::Error::Missing)?;
                    if !matches!(
                        plan.request().disposition,
                        Disposition::Create | Disposition::Replace
                    ) {
                        return Err(file_target::Error::Mismatch);
                    }
                    if resource.durable {
                        gate.with(|now, cancelled| {
                            authority.binding.preflight_capability(
                                manager,
                                host,
                                instance,
                                plan.request().disposition.capability(),
                                now,
                            )?;
                            if cancelled {
                                return Err(file_target::Error::CancelledBeforeDispatch);
                            }
                            authority.binding.check(manager, host, instance, now)?;
                            Ok(())
                        })?;
                        return Ok(MutationResponse::Staged {
                            bytes: plan.request().content_length,
                            durable: true,
                        });
                    }
                    if resource.commit_attempted {
                        return Err(file_target::Error::OutcomeUnknown);
                    }
                    if resource.buffer.len() as u64 != plan.request().content_length {
                        return Err(file_target::Error::Mismatch);
                    }
                    resource.commit_attempted = true;
                    resource.broker.stage_content_controlled(
                        manager,
                        host,
                        instance,
                        plan,
                        &resource.buffer,
                        &mut gate,
                    )?;
                    resource.durable = true;
                    resource.buffer = Zeroizing::new(Vec::new());
                    Ok(MutationResponse::Staged {
                        bytes: plan.request().content_length,
                        durable: true,
                    })
                }
                Request::Execute { id } => {
                    let resource = current(resources, id)?;
                    if resource.plan_cancelled {
                        return Err(file_target::Error::Persistence(
                            morrow_core::Error::RevisionConflict,
                        ));
                    }
                    let plan = resource
                        .request
                        .as_ref()
                        .ok_or(file_target::Error::Missing)?;
                    if let Some(guest) = resource.guest.as_ref() {
                        if guest.lease.worker != id.worker
                            || guest.lease.serial != id.serial
                            || guest.lease.plan_sha256 != plan_sha256(plan)
                            || guest.plan.container() != plan.container()
                            || guest.plan.request() != plan.request()
                        {
                            return Err(file_target::Error::Mismatch);
                        }
                        if guest.permit_consumed {
                            return Err(file_target::Error::AlreadyDispatched);
                        }
                        if !guest
                            .issue_status
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .delivered
                            || !guest.permit_status.as_ref().is_some_and(|status| {
                                status.lock().unwrap_or_else(|e| e.into_inner()).delivered
                            })
                        {
                            return Err(file_target::Error::Busy);
                        }
                        if plan.request().disposition == Disposition::Create && !resource.durable {
                            return Err(file_target::Error::Missing);
                        }
                        gate.with(|now, cancelled| {
                            authority
                                .binding
                                .validate_renewal_manager(manager, manager.revision())?;
                            authority.binding.preflight_capability(
                                manager,
                                host,
                                instance,
                                plan.request().disposition.capability(),
                                now,
                            )?;
                            if cancelled {
                                return Err(file_target::Error::CancelledBeforeDispatch);
                            }
                            Ok(())
                        })?;
                        resource.broker.validate_request_controlled(
                            manager, host, instance, plan, &mut gate,
                        )?;
                        // One-way before Core's claim/OS boundary. A lost reply
                        // or failed effect cannot manufacture a second permit.
                        resource
                            .guest
                            .as_mut()
                            .ok_or(file_target::Error::Missing)?
                            .permit_consumed = true;
                    }
                    match plan.request().disposition {
                        Disposition::Create => {
                            if !resource.durable {
                                return Err(file_target::Error::Missing);
                            }
                            resource
                                .broker
                                .create_controlled(manager, host, instance, plan, &mut gate)
                                .map(MutationResponse::Created)
                        }
                        Disposition::Delete => resource
                            .broker
                            .delete_controlled(manager, host, instance, plan, &mut gate)
                            .map(MutationResponse::Deleted),
                        Disposition::Replace => match resource
                            .broker
                            .replace_controlled(manager, host, instance, plan, &mut gate)
                        {
                            Err(error) => Err(error),
                            Ok(_) => Err(file_target::Error::OutcomeUnknown),
                        },
                    }
                }
                Request::CancelPlan { id } => {
                    let resource = current(resources, id)?;
                    let plan = resource
                        .request
                        .as_ref()
                        .ok_or(file_target::Error::Missing)?;
                    // History remains queryable after a spent target. Never use
                    // its old handle/reference as renewed effect authority.
                    gate.with(|now, cancelled| {
                        authority.binding.preflight_capability(
                            manager,
                            host,
                            instance,
                            plan.request().disposition.capability(),
                            now,
                        )?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    // Lookup and append each verify history. Admit them as
                    // sequential jobs, charging both reads without doubling the
                    // per-job limit or retaining two concurrent job leases.
                    let cost = plan.request().content_length;
                    if cost > control.limits.max_job_bytes {
                        return Err(file_target::Error::Limit);
                    }
                    let mut admit_read = || {
                        control
                            .charge(cost, &[], None)
                            .map_err(|_| file_target::Error::Limit)?;
                        gate.with(|now, cancelled| {
                            if cancelled {
                                return Err(file_target::Error::CancelledBeforeDispatch);
                            }
                            Ok(authority.binding.admit(
                                manager,
                                host,
                                instance,
                                plan.request().disposition.capability(),
                                0,
                                cost,
                                now,
                            )?)
                        })
                    };
                    let read_job = admit_read()?;
                    let command = plan.command()?;
                    let latest = host
                        .store_local()
                        .lookup_matching_io_intent(&command)?
                        .ok_or(file_target::Error::Missing)?;
                    let candidate = match latest.phase() {
                        morrow_core::io_intent::Phase::Prepared => {
                            latest.propose_cancel_before_dispatch()?
                        }
                        morrow_core::io_intent::Phase::CancelledBeforeDispatch => latest,
                        _ => return Err(file_target::Error::AlreadyDispatched),
                    };
                    drop(read_job);
                    let _append_job = admit_read()?;
                    let mut rejected = None;
                    let stored = host
                        .store_local_mut()
                        .append_io_intent_local_authorized(&candidate, || {
                            gate.with(|now, cancelled| {
                                authority.binding.check_liveness(now)?;
                                if cancelled {
                                    return Err(file_target::Error::CancelledBeforeDispatch);
                                }
                                Ok(())
                            })
                            .map_err(|error| {
                                rejected = Some(error);
                                morrow_core::Error::Invalid("inactive mutation plan cancellation")
                            })
                        })
                        .map_err(|error| {
                            rejected.unwrap_or(file_target::Error::Persistence(error))
                        })?;
                    resource.plan_cancelled = true;
                    resource.buffer = Zeroizing::new(Vec::new());
                    // Durable history and staged evidence are retained. Only the
                    // uncommitted spool is discarded; no filesystem effect runs.
                    gate.with(|now, cancelled| {
                        authority
                            .binding
                            .check(manager, host, instance, now)
                            .map_err(file_target::Error::CommittedButDeliveryDenied)?;
                        if cancelled {
                            return Err(file_target::Error::CommittedButDeliveryCancelled);
                        }
                        Ok(())
                    })?;
                    Ok(MutationResponse::PlanCancelled(stored))
                }
                Request::Query { id } => {
                    let resource = current(resources, id)?;
                    let Some(plan) = resource.request.as_ref() else {
                        gate.with(|now, cancelled| {
                            authority.binding.preflight_capability(
                                manager,
                                host,
                                instance,
                                resource.scope.disposition.capability(),
                                now,
                            )?;
                            if cancelled {
                                return Err(file_target::Error::CancelledBeforeDispatch);
                            }
                            Ok(())
                        })?;
                        gate.with(|now, cancelled| {
                            authority.binding.check(manager, host, instance, now)?;
                            if cancelled {
                                return Err(file_target::Error::CancelledBeforeDispatch);
                            }
                            Ok(())
                        })?;
                        return Ok(MutationResponse::History {
                            record: None,
                            staged_bytes: 0,
                            durable_content: false,
                        });
                    };
                    // The target can already be consumed. Use original live binding,
                    // never the old selection reference as fresh effect authority.
                    gate.with(|now, cancelled| {
                        authority.binding.preflight_capability(
                            manager,
                            host,
                            instance,
                            plan.request().disposition.capability(),
                            now,
                        )?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    let content_bytes = plan.request().content_length;
                    if content_bytes > control.limits.max_job_bytes {
                        return Err(file_target::Error::Limit);
                    }
                    let _read_job = if content_bytes == 0 {
                        None
                    } else {
                        control
                            .charge(content_bytes, &[], None)
                            .map_err(|_| file_target::Error::Limit)?;
                        Some(gate.with(|now, cancelled| {
                            if cancelled {
                                return Err(file_target::Error::CancelledBeforeDispatch);
                            }
                            Ok(authority.binding.admit(
                                manager,
                                host,
                                instance,
                                plan.request().disposition.capability(),
                                0,
                                content_bytes,
                                now,
                            )?)
                        })?)
                    };
                    // lookup_io_intent validates the full history and may read the
                    // staged body, so both host and instance admission precede it.
                    let command = plan.command()?;
                    let record = host.store_local().lookup_matching_io_intent(&command)?;
                    // Unknown/Observed already ran require_for_dispatch within
                    // matching-history verification, binding exact content and
                    // receipt. Only Prepared/Cancelled can have optional staged
                    // bytes, so read those once. Delete has no staged content.
                    let terminal_content = plan.request().disposition != Disposition::Delete
                        && record.as_ref().is_some_and(|record| {
                            matches!(
                                record.phase(),
                                morrow_core::io_intent::Phase::OutcomeUnknown
                                    | morrow_core::io_intent::Phase::Observed
                            )
                        });
                    let content = if plan.request().disposition == Disposition::Delete
                        || terminal_content
                        || record.is_none()
                    {
                        None
                    } else {
                        let mut rejected = None;
                        host.store_local()
                            .file_mutation_content_local_authorized(
                                &plan.request().subject,
                                &plan.request().operation_id,
                                || {
                                    gate.with(|now, cancelled| {
                                        authority.binding.check_liveness(now)?;
                                        if cancelled {
                                            return Err(
                                                file_target::Error::CancelledBeforeDispatch,
                                            );
                                        }
                                        Ok(())
                                    })
                                    .map_err(|error| {
                                        rejected = Some(error);
                                        morrow_core::Error::Invalid(
                                            "inactive file mutation history",
                                        )
                                    })
                                },
                            )
                            .map_err(|error| {
                                rejected.unwrap_or(file_target::Error::Persistence(error))
                            })?
                    };
                    if let Some(value) = &content {
                        if record.is_none()
                            || value.request_sha256() != command.request_sha256
                            || value.content_sha256()
                                != plan.request().content_sha256.unwrap_or([0; 32])
                            || value.content().len() as u64 != plan.request().content_length
                        {
                            return Err(file_target::Error::Mismatch);
                        }
                    }
                    gate.with(|now, cancelled| {
                        authority.binding.check(manager, host, instance, now)?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    if record.as_ref().is_some_and(|record| {
                        record.phase() == morrow_core::io_intent::Phase::CancelledBeforeDispatch
                    }) {
                        resource.plan_cancelled = true;
                        resource.buffer = Zeroizing::new(Vec::new());
                    }
                    let durable_content = terminal_content || content.is_some();
                    if durable_content {
                        resource.durable = true;
                        resource.buffer = Zeroizing::new(Vec::new());
                    } else {
                        resource.durable = false;
                    }
                    if !durable_content
                        && record.as_ref().is_some_and(|record| {
                            record.phase() == morrow_core::io_intent::Phase::Prepared
                        })
                    {
                        // A deliberate query found no committed content while the
                        // exact plan is still Prepared. An explicit later commit may
                        // retry the same retained buffer; never resend automatically.
                        resource.commit_attempted = false;
                    }
                    let staged_bytes = if terminal_content {
                        plan.request().content_length
                    } else {
                        content
                            .as_ref()
                            .map_or(resource.buffer.len() as u64, |value| {
                                value.content().len() as u64
                            })
                    };
                    Ok(MutationResponse::History {
                        record,
                        staged_bytes,
                        durable_content,
                    })
                }
                Request::Release { id } => {
                    let resource = current(resources, id)?;
                    gate.with(|now, cancelled| {
                        authority.binding.preflight_capability(
                            manager,
                            host,
                            instance,
                            resource.scope.disposition.capability(),
                            now,
                        )?;
                        if cancelled {
                            return Err(file_target::Error::CancelledBeforeDispatch);
                        }
                        Ok(())
                    })?;
                    match resource.broker.release_controlled(
                        manager,
                        host,
                        instance,
                        resource.reference,
                        &mut gate,
                    ) {
                        Ok(()) | Err(file_target::Error::Missing) => {} // spent after a one-shot effect
                        Err(error) => return Err(error),
                    }
                    resources.0.remove(&id.serial);
                    Ok(MutationResponse::Released)
                }
            }
        })
        .ok_or(file_target::Error::Admission(io_binding::Error::Denied))?
}

/// Called synchronously at a paused mutation import on the original owner.
/// The typed callback is installed only by managed mutation-job admission.
#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch_guest<O: ManagedHostOwner>(
    owner: &mut dyn std::any::Any,
    instance: Option<&ManagedInstance>,
    control: &Control,
    resources: &mut Resources,
    cancel: &Cancellation,
    deadline: std::time::Instant,
    mode: MutationGuestJobMode,
    request: &guest_wire::Request,
) -> Result<guest_wire::Response, file_target::Error> {
    if control.mutation_history {
        return Err(file_target::Error::Admission(io_binding::Error::Denied));
    }
    let owner = owner
        .downcast_mut::<O>()
        .ok_or(file_target::Error::InvalidSelection)?;
    let instance = instance.ok_or(file_target::Error::InvalidSelection)?;
    let lease = mode.lease();
    if lease.worker != control.id
        || request.reference != lease.reference
        || request.operation_id.is_empty()
    {
        return Err(file_target::Error::Mismatch);
    }
    let semantic = {
        let mut normalized = request.clone();
        normalized.call_id = 1;
        Sha256::digest(normalized.encode()?).into()
    };
    if let Some((old_semantic, original_deadline, saved)) = resources.3.get(&request.submission) {
        if *old_semantic != semantic {
            return Err(file_target::Error::Mismatch);
        }
        if std::time::Instant::now() > *original_deadline {
            return Err(file_target::Error::CancelledBeforeDispatch);
        }
        if matches!(mode, MutationGuestJobMode::Stage(_))
            && matches!(request.action, guest_wire::Action::Execute)
            || matches!(mode, MutationGuestJobMode::Execute(_))
                && !matches!(
                    request.action,
                    guest_wire::Action::Execute
                        | guest_wire::Action::Query
                        | guest_wire::Action::Release
                )
        {
            return Err(file_target::Error::InvalidSelection);
        }
        // Cached receipts are still subject to current worker, package and
        // registry authority. Release is the only retrievable receipt after
        // its selected resource is removed.
        let authority = control
            .authority
            .as_ref()
            .ok_or(file_target::Error::Admission(io_binding::Error::Denied))?;
        if control.fault(cancel).is_some() {
            return Err(file_target::Error::CancelledBeforeDispatch);
        }
        owner
            .with_managed_runtime(|manager, host| {
                if host.binding() != control.host {
                    return Err(file_target::Error::Mismatch);
                }
                authority
                    .with_time(|now| {
                        authority
                            .binding
                            .validate_renewal_manager(manager, manager.revision())?;
                        authority.binding.preflight_capability(
                            manager,
                            host,
                            instance,
                            lease.capability,
                            now,
                        )?;
                        Ok(())
                    })
                    .map_err(file_target::Error::Admission)
            })
            .ok_or(file_target::Error::InvalidSelection)??;
        if let Some(resource) = resources.0.get(&lease.serial) {
            let guest = resource.guest.as_ref().ok_or(file_target::Error::Missing)?;
            if guest.lease != lease
                || !guest
                    .issue_status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .delivered
                || matches!(mode, MutationGuestJobMode::Execute(_))
                    && !guest.permit_status.as_ref().is_some_and(|status| {
                        status.lock().unwrap_or_else(|e| e.into_inner()).delivered
                    })
            {
                return Err(file_target::Error::Missing);
            }
        } else if !matches!(request.action, guest_wire::Action::Release)
            || !resources.2.contains(&lease.reference)
            || saved
                .as_ref()
                .is_none_or(|reply| reply.kind != guest_wire::Kind::Release)
        {
            return Err(file_target::Error::Missing);
        }
        let mut response = saved.clone().ok_or(file_target::Error::AlreadyDispatched)?;
        response.call_id = request.call_id;
        return Ok(response);
    }
    if resources.3.len() >= MAX_GUEST_SUBMISSIONS {
        return Err(file_target::Error::Limit);
    }
    let resource = resources
        .0
        .get(&lease.serial)
        .ok_or(file_target::Error::Missing)?;
    let guest = resource.guest.as_ref().ok_or(file_target::Error::Missing)?;
    if guest.lease != lease
        || !guest
            .issue_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .delivered
        || matches!(mode, MutationGuestJobMode::Execute(_))
            && !guest
                .permit_status
                .as_ref()
                .is_some_and(|status| status.lock().unwrap_or_else(|e| e.into_inner()).delivered)
        || guest.plan.request().operation_id != request.operation_id
        || plan_sha256(&guest.plan) != lease.plan_sha256
        || resource.plan_cancelled
            && !matches!(
                request.action,
                guest_wire::Action::Query | guest_wire::Action::Release
            )
    {
        return Err(file_target::Error::Mismatch);
    }
    if matches!(mode, MutationGuestJobMode::Stage(_))
        && matches!(request.action, guest_wire::Action::Execute)
        || matches!(mode, MutationGuestJobMode::Execute(_))
            && !matches!(
                request.action,
                guest_wire::Action::Execute
                    | guest_wire::Action::Query
                    | guest_wire::Action::Release
            )
    {
        return Err(file_target::Error::InvalidSelection);
    }
    let plan = guest.plan.clone();
    let id = MutationSession {
        worker: lease.worker,
        serial: lease.serial,
    };
    let (command, bytes) = match &request.action {
        guest_wire::Action::Create {
            content_length,
            content_sha256,
        } if plan.request().disposition == Disposition::Create
            && plan.request().content_length == *content_length
            && plan.request().content_sha256 == Some(*content_sha256) =>
        {
            (
                Request::Prepare {
                    id,
                    request: plan.clone(),
                },
                None,
            )
        }
        guest_wire::Action::Delete if plan.request().disposition == Disposition::Delete => (
            Request::Prepare {
                id,
                request: plan.clone(),
            },
            None,
        ),
        guest_wire::Action::Chunk { offset, bytes }
            if plan.request().disposition == Disposition::Create =>
        {
            (
                Request::Chunk {
                    id,
                    offset: *offset,
                    len: bytes.len(),
                    capacity: bytes.len(),
                },
                Some(Zeroizing::new(bytes.clone())),
            )
        }
        guest_wire::Action::Commit if plan.request().disposition == Disposition::Create => {
            (Request::Commit { id }, None)
        }
        guest_wire::Action::Execute => (Request::Execute { id }, None),
        guest_wire::Action::Query => (Request::Query { id }, None),
        guest_wire::Action::CancelPlan => (Request::CancelPlan { id }, None),
        guest_wire::Action::Release => (Request::Release { id }, None),
        _ => return Err(file_target::Error::Mismatch),
    };
    resources
        .3
        .insert(request.submission, (semantic, deadline, None));
    let result = dispatch_with_source(
        owner,
        Some(instance),
        control,
        DispatchSource::Guest(cancel),
        resources,
        command,
        bytes,
    );
    let mut response = guest_wire::Response::for_request(
        request,
        guest_wire::Status::Completed,
        guest_wire::Phase::None,
        guest_wire::Effect::Unspecified,
    );
    match result {
        Ok(MutationResponse::Prepared(_)) => response.phase = guest_wire::Phase::Prepared,
        Ok(MutationResponse::Staged { bytes, durable }) => {
            response.phase = guest_wire::Phase::Prepared;
            response.staged_bytes = bytes;
            response.durable_content = durable;
        }
        Ok(MutationResponse::Created(outcome)) => {
            response.phase = guest_wire::Phase::Observed;
            response.effect = match outcome.result() {
                morrow_core::file_effect::CreateResult::Created => guest_wire::Effect::OsSucceeded,
                morrow_core::file_effect::CreateResult::OsRejected { .. } => {
                    guest_wire::Effect::OsRejected
                }
            };
            response.staged_bytes = plan.request().content_length;
            response.durable_content = true;
        }
        Ok(MutationResponse::Deleted(outcome)) => {
            response.phase = guest_wire::Phase::Observed;
            response.effect = match outcome.result() {
                morrow_core::file_effect::DeleteResult::Deleted => guest_wire::Effect::OsSucceeded,
                morrow_core::file_effect::DeleteResult::OsRejected { .. } => {
                    guest_wire::Effect::OsRejected
                }
            };
        }
        Ok(MutationResponse::PlanCancelled(_)) => {
            response.phase = guest_wire::Phase::CancelledBeforeDispatch;
        }
        Ok(MutationResponse::History {
            record,
            staged_bytes,
            durable_content,
        }) => {
            response.phase = match record.as_ref().map(Record::phase) {
                None => guest_wire::Phase::Absent,
                Some(IntentPhase::Prepared) => guest_wire::Phase::Prepared,
                Some(IntentPhase::OutcomeUnknown) => guest_wire::Phase::OutcomeUnknown,
                Some(IntentPhase::Observed) => guest_wire::Phase::Observed,
                Some(IntentPhase::CancelledBeforeDispatch) => {
                    guest_wire::Phase::CancelledBeforeDispatch
                }
                Some(IntentPhase::InvalidPhase) => return Err(file_target::Error::Mismatch),
            };
            if matches!(
                response.phase,
                guest_wire::Phase::Prepared | guest_wire::Phase::OutcomeUnknown
            ) {
                response.staged_bytes = staged_bytes;
                response.durable_content = durable_content;
            }
            if response.phase == guest_wire::Phase::Observed {
                let reconciled = dispatch_with_source(
                    owner,
                    Some(instance),
                    control,
                    DispatchSource::Guest(cancel),
                    resources,
                    Request::Reconcile {
                        request: plan.clone(),
                    },
                    None,
                )?;
                response.effect = match reconciled {
                    MutationResponse::Reconciled {
                        outcome: Some(outcome),
                        ..
                    } => match *outcome {
                        MutationOutcome::Created(value) => match value.result() {
                            morrow_core::file_effect::CreateResult::Created => {
                                guest_wire::Effect::OsSucceeded
                            }
                            morrow_core::file_effect::CreateResult::OsRejected { .. } => {
                                guest_wire::Effect::OsRejected
                            }
                        },
                        MutationOutcome::Deleted(value) => match value.result() {
                            morrow_core::file_effect::DeleteResult::Deleted => {
                                guest_wire::Effect::OsSucceeded
                            }
                            morrow_core::file_effect::DeleteResult::OsRejected { .. } => {
                                guest_wire::Effect::OsRejected
                            }
                        },
                        MutationOutcome::Replaced(_) => return Err(file_target::Error::Mismatch),
                    },
                    _ => return Err(file_target::Error::Missing),
                };
            }
        }
        Ok(MutationResponse::Released) => {}
        Ok(_) => return Err(file_target::Error::InvalidSelection),
        Err(error) => {
            if matches!(
                error,
                file_target::Error::CommittedButDeliveryDenied(_)
                    | file_target::Error::CommittedButDeliveryCancelled
            ) && !matches!(request.action, guest_wire::Action::Execute)
            {
                // Preparation or content commit may be durable, while this
                // wire kind cannot encode OutcomeUnknown. Preserve transport
                // uncertainty and require an explicit history query.
                return Err(error);
            }
            response.status = match error {
                file_target::Error::OutcomeUnknown | file_target::Error::AlreadyDispatched
                    if matches!(
                        request.action,
                        guest_wire::Action::Execute | guest_wire::Action::Query
                    ) =>
                {
                    guest_wire::Status::OutcomeUnknown
                }
                file_target::Error::CommittedButDeliveryDenied(_)
                | file_target::Error::CommittedButDeliveryCancelled
                    if matches!(request.action, guest_wire::Action::Execute) =>
                {
                    guest_wire::Status::OutcomeUnknown
                }
                file_target::Error::Admission(_) => guest_wire::Status::Revoked,
                file_target::Error::CancelledBeforeDispatch => guest_wire::Status::Cancelled,
                file_target::Error::Limit => guest_wire::Status::Quota,
                file_target::Error::Missing => guest_wire::Status::NotFound,
                file_target::Error::Mismatch
                | file_target::Error::Changed
                | file_target::Error::AlreadyDispatched => guest_wire::Status::Conflict,
                file_target::Error::UnsupportedConditionalReplacement => {
                    guest_wire::Status::Unsupported
                }
                _ => guest_wire::Status::Failed,
            };
            response.phase = if response.status == guest_wire::Status::OutcomeUnknown {
                guest_wire::Phase::OutcomeUnknown
            } else {
                guest_wire::Phase::None
            };
        }
    }
    response.validate(request)?;
    resources.3.insert(
        request.submission,
        (semantic, deadline, Some(response.clone())),
    );
    Ok(response)
}
