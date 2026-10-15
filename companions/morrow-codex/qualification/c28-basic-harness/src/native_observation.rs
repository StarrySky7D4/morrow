//! Finite historical observations, never a control/authority or native-close witness.
//! The original logical/event layer stays separate from the passive RUNNER layer.
//! CheckedRunnerOwner captures the runner process, not the business child; the
//! additive owner export labels that scope and never conflates their exits.
//! No OS calls, backend calls, controls, retries, cleanup, keys or claim material.
use morrow_agent_process_control_v1::host::Handle;
use morrow_agent_session_exec_v1_r2::{
    ExecutionFacts, ToolPhase, safe_exec::{ToolIdentity, ToolObservation},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_OBSERVATION_BYTES: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Schema {
    #[serde(rename = "C28_NATIVE_OWNER_OBSERVATION_V1")]
    V1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Stage { NativeStart, NativeJoin }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RecordPoint { PreStartClaimOwnerRecord, PostOriginalJoinOwnerRecord }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum NotProven { NotProven }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Ack { NotObserved, OriginalTaskAckAndAd31Observed }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Reclaimed { Reclaimed }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum SourceScope { SameR2OwnerAndExecProcessEventsOnly }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Decimal(String);
impl Decimal {
    fn new(v: u64) -> Self { Self(v.to_string()) }
    fn value(&self) -> Result<u64, &'static str> {
        let value = self.0.parse::<u64>().map_err(|_| "NON_CANONICAL_U64")?;
        if self.0 != value.to_string() { return Err("NON_CANONICAL_U64"); }
        Ok(value)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LogicalHandle {
    // Domain-separated digest of the existing logical Handle, NOT an OS HANDLE.
    // No nonce or constructor/control API is exported.
    historical_reference_sha256: [u8; 32],
    generation: Decimal,
}
impl LogicalHandle {
    fn observed(handle: Handle) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"C28_LOGICAL_PROCESS_REFERENCE_V1");
        hash.update(handle.nonce);
        hash.update(handle.generation.to_le_bytes());
        Self { historical_reference_sha256: hash.finalize().into(),
               generation: Decimal::new(handle.generation) }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicIdentity {
    // Exact full original identity is hashed in a length-delimited domain.
    // Issuer nonce, operation/session strings and execution-domain strings are
    // not emitted. Hash equality is historical correlation, never authority.
    full_identity_sha256: [u8; 32],
    proposal_sha256: [u8; 32],
    intent_sha256: [u8; 32],
    session_epoch: Decimal,
    generation: Decimal,
}
fn identity_digest(value: &ToolIdentity) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"C28_R2_OWNER_IDENTITY_V1");
    for part in [value.operation_id.as_bytes(), value.proposal_sha256.as_slice(),
                 value.intent_sha256.as_slice(), value.execution_domain.as_bytes(),
                 value.session_id.as_bytes(), value.original_issuer_nonce.as_slice()] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    hash.update(value.session_epoch.to_le_bytes());
    hash.update(value.generation.to_le_bytes());
    hash.finalize().into()
}
impl PublicIdentity {
    fn observed(v: &ToolIdentity) -> Self {
        Self { full_identity_sha256: identity_digest(v), proposal_sha256: v.proposal_sha256,
               intent_sha256: v.intent_sha256, session_epoch: Decimal::new(v.session_epoch),
               generation: Decimal::new(v.generation) }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EventFacts {
    // Signed ExecProcessEvent::Exited value; MUST NOT be reinterpreted as a
    // GetExitCodeProcess DWORD or a CheckedRunnerOwner native-exit witness.
    protocol_exit_code_i32: Option<i32>,
    protocol_output_closed: bool,
    stdout_sha256: [u8; 32],
    stderr_sha256: [u8; 32],
    stdout_bytes: Decimal,
    stderr_bytes: Decimal,
}
impl EventFacts {
    fn observed(v: &ExecutionFacts) -> Self {
        Self { protocol_exit_code_i32: v.exit_code, protocol_output_closed: v.output_closed,
               stdout_sha256: v.stdout_sha256, stderr_sha256: v.stderr_sha256,
               stdout_bytes: Decimal::new(v.stdout_bytes), stderr_bytes: Decimal::new(v.stderr_bytes) }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RecordPhase { Proposed, Approved, Revoked, DispatchUnknown, Reported }
impl RecordPhase {
    fn observed(phase: ToolPhase) -> Self {
        match phase {
            ToolPhase::Proposed => Self::Proposed,
            ToolPhase::Approved => Self::Approved,
            ToolPhase::Revoked => Self::Revoked,
            ToolPhase::DispatchUnknown => Self::DispatchUnknown,
            ToolPhase::Reported => Self::Reported,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerRecord {
    identity: PublicIdentity,
    record_revision: Decimal,
    record_sha256: [u8; 32],
    observation_revision: Decimal,
    invocation_started: bool,
    phase: RecordPhase,
    latest_event_facts: Option<EventFacts>,
    accepted_report_event_facts: Option<EventFacts>,
}
impl OwnerRecord {
    fn observed(v: &ToolObservation) -> Self {
        Self { identity: PublicIdentity::observed(&v.identity),
               record_revision: Decimal::new(v.record_revision), record_sha256: v.record_sha256,
               observation_revision: Decimal::new(v.observation_revision),
               invocation_started: v.invocation_started, phase: RecordPhase::observed(v.phase),
               latest_event_facts: v.latest.as_ref().map(EventFacts::observed),
               accepted_report_event_facts: v.accepted_report.as_ref().map(EventFacts::observed) }
    }
}
/// Captured on the existing successful checks in product::stop_and_verify_observed.
/// These are Workbench task and R2 resource-table observations. They do NOT
/// assert original Windows worker joins, native handle closure or lease release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerifiedOwnerJoin {
    storage: Reclaimed,
    execution_result_ok: bool,
    disconnect_result_ok: bool,
    maintenance_result_ok: bool,
    resource_tables_clean_observed: bool,
    pending_fact_table_empty_observed: bool,
}
impl VerifiedOwnerJoin {
    pub(crate) fn captured(execution: bool, disconnect: bool, maintenance: bool,
                          clean: bool, pending_empty: bool) -> Self {
        Self { storage: Reclaimed::Reclaimed, execution_result_ok: execution,
               disconnect_result_ok: disconnect, maintenance_result_ok: maintenance,
               resource_tables_clean_observed: clean, pending_fact_table_empty_observed: pending_empty }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UnavailableNativeFacts {
    original_os_process_identity: NotProven,
    original_creation_filetime_100ns: NotProven,
    get_exit_code_process_raw_dword: NotProven,
    checked_runner_handshake: NotProven,
    original_os_io_eof_and_worker_joins: NotProven,
    input_endpoint_retirement: NotProven,
    complete_descendant_quiescence: NotProven,
    keeper_and_quota_lease_retirement: NotProven,
    authenticated_native_close_ack: NotProven,
    native_handle_close_debt_retirement: NotProven,
}
impl Default for UnavailableNativeFacts {
    fn default() -> Self {
        Self { original_os_process_identity: NotProven::NotProven,
               original_creation_filetime_100ns: NotProven::NotProven,
               get_exit_code_process_raw_dword: NotProven::NotProven,
               checked_runner_handshake: NotProven::NotProven,
               original_os_io_eof_and_worker_joins: NotProven::NotProven,
               input_endpoint_retirement: NotProven::NotProven,
               complete_descendant_quiescence: NotProven::NotProven,
               keeper_and_quota_lease_retirement: NotProven::NotProven,
               authenticated_native_close_ack: NotProven::NotProven,
               native_handle_close_debt_retirement: NotProven::NotProven }
    }
}

// Separate RUNNER-owner history, never a business-child native witness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OwnerAvailability { Unsupported, NotObserved, Busy, Poisoned, HistoryLost, WrongInstance, Observed, Borrowed, Retired }
impl From<codex_exec_server::RunnerObservationAvailability> for OwnerAvailability {
    fn from(v:codex_exec_server::RunnerObservationAvailability)->Self { match v {
        codex_exec_server::RunnerObservationAvailability::Unsupported=>Self::Unsupported,
        codex_exec_server::RunnerObservationAvailability::NotObserved=>Self::NotObserved,
        codex_exec_server::RunnerObservationAvailability::Busy=>Self::Busy,
        codex_exec_server::RunnerObservationAvailability::Poisoned=>Self::Poisoned,
        codex_exec_server::RunnerObservationAvailability::HistoryLost=>Self::HistoryLost,
        codex_exec_server::RunnerObservationAvailability::WrongInstance=>Self::WrongInstance,
        codex_exec_server::RunnerObservationAvailability::Observed=>Self::Observed,
        codex_exec_server::RunnerObservationAvailability::Borrowed=>Self::Borrowed,
        codex_exec_server::RunnerObservationAvailability::Retired=>Self::Retired,
    } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OwnerLifecycle { ReservedBeforeLaunch, ProcessLaunchedInHandshake, HandshakeSucceededInTransport, CheckedIoExtracted, ActorAdopted, FailedOrAbandoned, FinalizedClean, PhysicallyReapedStickyUnknown }
impl From<codex_exec_server::RunnerLifecycle> for OwnerLifecycle {
    fn from(v:codex_exec_server::RunnerLifecycle)->Self { match v {
        codex_exec_server::RunnerLifecycle::ReservedBeforeLaunch=>Self::ReservedBeforeLaunch,
        codex_exec_server::RunnerLifecycle::ProcessLaunchedInHandshake=>Self::ProcessLaunchedInHandshake,
        codex_exec_server::RunnerLifecycle::HandshakeSucceededInTransport=>Self::HandshakeSucceededInTransport,
        codex_exec_server::RunnerLifecycle::CheckedIoExtracted=>Self::CheckedIoExtracted,
        codex_exec_server::RunnerLifecycle::ActorAdopted=>Self::ActorAdopted,
        codex_exec_server::RunnerLifecycle::FailedOrAbandoned=>Self::FailedOrAbandoned,
        codex_exec_server::RunnerLifecycle::FinalizedClean=>Self::FinalizedClean,
        codex_exec_server::RunnerLifecycle::PhysicallyReapedStickyUnknown=>Self::PhysicallyReapedStickyUnknown,
    } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OwnerFailure { CallerAwaitCancelled, IdentityValidationFailed, ThreadCloseFailed, ConnectRetainedUnknown, LateConnectSuccess, ConnectFailed, HandshakeFailed, StartupUnwound, TransportDroppedUnadopted, WrongLegacyConversion, CheckedIoDroppedUnadopted, RuntimeLifecycleFailure, ReaderIncompleteOrFailed, WriterFailed, InputTaskFailedOrAborted, FinalizationDeadlineExpired, ProcessWaitExpiredOrFailed }
impl From<codex_exec_server::RunnerFailure> for OwnerFailure {
    fn from(v:codex_exec_server::RunnerFailure)->Self { match v {
        codex_exec_server::RunnerFailure::CallerAwaitCancelled=>Self::CallerAwaitCancelled,
        codex_exec_server::RunnerFailure::IdentityValidationFailed=>Self::IdentityValidationFailed,
        codex_exec_server::RunnerFailure::ThreadCloseFailed=>Self::ThreadCloseFailed,
        codex_exec_server::RunnerFailure::ConnectRetainedUnknown=>Self::ConnectRetainedUnknown,
        codex_exec_server::RunnerFailure::LateConnectSuccess=>Self::LateConnectSuccess,
        codex_exec_server::RunnerFailure::ConnectFailed=>Self::ConnectFailed,
        codex_exec_server::RunnerFailure::HandshakeFailed=>Self::HandshakeFailed,
        codex_exec_server::RunnerFailure::StartupUnwound=>Self::StartupUnwound,
        codex_exec_server::RunnerFailure::TransportDroppedUnadopted=>Self::TransportDroppedUnadopted,
        codex_exec_server::RunnerFailure::WrongLegacyConversion=>Self::WrongLegacyConversion,
        codex_exec_server::RunnerFailure::CheckedIoDroppedUnadopted=>Self::CheckedIoDroppedUnadopted,
        codex_exec_server::RunnerFailure::RuntimeLifecycleFailure=>Self::RuntimeLifecycleFailure,
        codex_exec_server::RunnerFailure::ReaderIncompleteOrFailed=>Self::ReaderIncompleteOrFailed,
        codex_exec_server::RunnerFailure::WriterFailed=>Self::WriterFailed,
        codex_exec_server::RunnerFailure::InputTaskFailedOrAborted=>Self::InputTaskFailedOrAborted,
        codex_exec_server::RunnerFailure::FinalizationDeadlineExpired=>Self::FinalizationDeadlineExpired,
        codex_exec_server::RunnerFailure::ProcessWaitExpiredOrFailed=>Self::ProcessWaitExpiredOrFailed,
    } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OwnerWorkerPlacement { NotInstalled, Reserved, Pending, Joined, Borrowed }
impl From<codex_exec_server::RunnerWorkerPlacement> for OwnerWorkerPlacement {
    fn from(v:codex_exec_server::RunnerWorkerPlacement)->Self { match v {
        codex_exec_server::RunnerWorkerPlacement::NotInstalled=>Self::NotInstalled,
        codex_exec_server::RunnerWorkerPlacement::Reserved=>Self::Reserved,
        codex_exec_server::RunnerWorkerPlacement::Pending=>Self::Pending,
        codex_exec_server::RunnerWorkerPlacement::Joined=>Self::Joined,
        codex_exec_server::RunnerWorkerPlacement::Borrowed=>Self::Borrowed,
    } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OwnerFinalization { NotFinalized, InProgress, CleanRelease, PhysicallyReapedStickyUnknown }
impl From<codex_exec_server::RunnerFinalization> for OwnerFinalization {
    fn from(v:codex_exec_server::RunnerFinalization)->Self { match v {
        codex_exec_server::RunnerFinalization::NotFinalized=>Self::NotFinalized,
        codex_exec_server::RunnerFinalization::InProgress=>Self::InProgress,
        codex_exec_server::RunnerFinalization::CleanRelease=>Self::CleanRelease,
        codex_exec_server::RunnerFinalization::PhysicallyReapedStickyUnknown=>Self::PhysicallyReapedStickyUnknown,
    } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum OwnerThreadDisposition { ClosedOnCapture, CloseFailedUnknown, ScopedSynthetic }
impl From<codex_exec_server::RunnerThreadDisposition> for OwnerThreadDisposition {
    fn from(v:codex_exec_server::RunnerThreadDisposition)->Self { match v {
        codex_exec_server::RunnerThreadDisposition::ClosedOnCapture=>Self::ClosedOnCapture,
        codex_exec_server::RunnerThreadDisposition::CloseFailedUnknown=>Self::CloseFailedUnknown,
        codex_exec_server::RunnerThreadDisposition::ScopedSynthetic=>Self::ScopedSynthetic,
    } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum RunnerScope { #[serde(rename="ACTUAL_RUNNER_OWNER_HISTORY_NOT_BUSINESS_CHILD")] ActualRunner }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerWorker {
    placement: OwnerWorkerPlacement, within_deadline:Option<bool>, result_clean:Option<bool>,
    physical_eof:Option<bool>, protocol_exit_i32:Option<i32>,
    termination_sent:Option<bool>, joined_error:bool,
}
impl From<codex_exec_server::RunnerWorkerHistory> for OwnerWorker {
    fn from(v:codex_exec_server::RunnerWorkerHistory)->Self {
        Self{placement:v.placement.into(),within_deadline:v.within_deadline,result_clean:v.result_clean,
            physical_eof:v.physical_eof,protocol_exit_i32:v.protocol_exit_i32,
            termination_sent:v.termination_sent,joined_error:v.joined_error}
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerIdentity { runner_pid:u32, runner_creation_filetime_100ns:Decimal }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerHandshake { negotiated_schema:[u8;32], close_stdin:bool, resize:bool }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerHelper { acknowledged:bool, within_deadline:bool, had_unknown:bool }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerSnapshot {
    lifecycle:OwnerLifecycle, runner_identity:Option<OwnerIdentity>,
    runner_get_exit_code_process_u32:Option<u32>,
    negotiated_handshake:Option<OwnerHandshake>,
    protocol_business_exit_i32:Option<i32>,reader_physical_eof_observed:bool,
    no_replay_sealed:bool,first_error:Option<OwnerFailure>,
    reader:OwnerWorker,writer:OwnerWorker,input:OwnerWorker,
    thread_disposition:Option<OwnerThreadDisposition>,thread_close_debt_os_error:Option<u32>,
    process_handle_retained:bool,native_thread_anchors_retained:bool,
    finalization:OwnerFinalization,finalizer_epoch:Decimal,worker_admission_active:bool,
    helpers:[Option<OwnerHelper>;2],
}
impl From<codex_exec_server::RunnerOwnerSnapshot> for OwnerSnapshot {
    fn from(v:codex_exec_server::RunnerOwnerSnapshot)->Self {
        Self{lifecycle:v.lifecycle.into(),
            runner_identity:v.runner_identity.map(|x|OwnerIdentity{runner_pid:x.pid,
                runner_creation_filetime_100ns:Decimal::new(x.creation_filetime_100ns)}),
            runner_get_exit_code_process_u32:v.runner_get_exit_code_process_u32,
            negotiated_handshake:v.negotiated_handshake.map(|x|OwnerHandshake{
                negotiated_schema:x.schema,close_stdin:x.close_stdin,resize:x.resize}),
            protocol_business_exit_i32:v.protocol_business_exit_i32,
            reader_physical_eof_observed:v.reader_physical_eof_observed,
            no_replay_sealed:v.no_replay_sealed,first_error:v.first_error.map(Into::into),
            reader:v.reader.into(),writer:v.writer.into(),input:v.input.into(),
            thread_disposition:v.thread_disposition.map(Into::into),
            thread_close_debt_os_error:v.thread_close_debt_os_error,
            process_handle_retained:v.process_handle_retained,
            native_thread_anchors_retained:v.native_thread_anchors_retained,
            finalization:v.finalization.into(),finalizer_epoch:Decimal::new(v.finalizer_epoch),
            worker_admission_active:v.worker_admission_active,
            helpers:v.helpers.map(|x|x.map(|x|OwnerHelper{acknowledged:x.acknowledged,
                within_deadline:x.within_deadline,had_unknown:x.had_unknown}))}
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunnerOwnerExport {
    scope:RunnerScope, availability:OwnerAvailability,
    instance:Option<Decimal>, startup_attempt:Option<Decimal>, revision:Option<Decimal>,
    snapshot:Option<OwnerSnapshot>,
}
impl From<codex_exec_server::RunnerObservation> for RunnerOwnerExport {
    fn from(v:codex_exec_server::RunnerObservation)->Self {
        Self{scope:RunnerScope::ActualRunner,availability:v.availability.into(),
            instance:v.key.map(|k|Decimal::new(k.instance)),
            startup_attempt:v.key.map(|k|Decimal::new(k.startup_attempt)),
            revision:v.revision.map(Decimal::new),snapshot:v.snapshot.map(Into::into)}
    }
}
impl RunnerOwnerExport {
    fn validate_shape(&self)->Result<(),&'static str> {
        for v in [&self.instance,&self.startup_attempt,&self.revision].into_iter().flatten() {v.value()?;}
        if self.instance.as_ref().is_some_and(|v|v.value()==Ok(0)){return Err("ZERO_RUNNER_INSTANCE");}
        let available=matches!(self.availability,OwnerAvailability::Observed|OwnerAvailability::Borrowed|OwnerAvailability::Retired);
        if available!=(self.snapshot.is_some()) || available&&(self.instance.is_none()||self.revision.is_none()||self.startup_attempt.is_none()){
            return Err("RUNNER_AVAILABILITY_SHAPE");
        }
        if let Some(s)=&self.snapshot {
            s.finalizer_epoch.value()?;
            if let Some(id)=&s.runner_identity {
                if id.runner_pid==0||id.runner_creation_filetime_100ns.value()?==0{return Err("RUNNER_IDENTITY_SHAPE");}
            }
            if s.finalization==OwnerFinalization::CleanRelease &&
                (s.no_replay_sealed||s.first_error.is_some()||s.thread_close_debt_os_error.is_some()
                 ||s.runner_get_exit_code_process_u32.is_none()) {return Err("RUNNER_CLEAN_CONTRADICTION");}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeObservation {
    schema: Schema,
    stage: Stage,
    scope: SourceScope,
    record_point: RecordPoint,
    logical_started_reply_reference: LogicalHandle,
    owner_record: OwnerRecord,
    original_task_join: Option<VerifiedOwnerJoin>,
    acknowledgement: Ack,
    native: UnavailableNativeFacts,
    production_qualification: NotProven,
    #[serde(default)]
    runner_owner: Option<RunnerOwnerExport>,
}
impl NativeObservation {
    pub(crate) fn with_runner_owner(mut self, value: Option<codex_exec_server::RunnerObservation>) -> Self {
        self.runner_owner=value.map(Into::into);
        self
    }
    pub(crate) fn started(record_before_start: &ToolObservation, actual_reply: Handle) -> Self {
        Self { schema: Schema::V1, stage: Stage::NativeStart,
               scope: SourceScope::SameR2OwnerAndExecProcessEventsOnly,
               record_point: RecordPoint::PreStartClaimOwnerRecord,
               logical_started_reply_reference: LogicalHandle::observed(actual_reply),
               owner_record: OwnerRecord::observed(record_before_start), original_task_join: None,
               acknowledgement: Ack::NotObserved, native: Default::default(),
               production_qualification: NotProven::NotProven, runner_owner: None }
    }
    // Called only after existing original stop/verify, AD31 retained, original
    // Workbench ACK and AD31 acknowledged succeed, preserving their order/errors.
    pub(crate) fn joined(record: &ToolObservation, handle: Handle,
                         observed_join: Option<&VerifiedOwnerJoin>) -> Self {
        Self { schema: Schema::V1, stage: Stage::NativeJoin,
               scope: SourceScope::SameR2OwnerAndExecProcessEventsOnly,
               record_point: RecordPoint::PostOriginalJoinOwnerRecord,
               logical_started_reply_reference: LogicalHandle::observed(handle),
               owner_record: OwnerRecord::observed(record),
               original_task_join: observed_join.cloned(),
               acknowledgement: Ack::OriginalTaskAckAndAd31Observed, native: Default::default(),
               production_qualification: NotProven::NotProven, runner_owner: None }
    }
    fn validate_shape(&self) -> Result<(), &'static str> {
        if let Some(owner)=&self.runner_owner { owner.validate_shape()?; }
        self.logical_started_reply_reference.generation.value()?;
        self.owner_record.identity.session_epoch.value()?;
        self.owner_record.identity.generation.value()?;
        self.owner_record.record_revision.value()?;
        self.owner_record.observation_revision.value()?;
        for facts in self.owner_record.latest_event_facts.iter()
            .chain(self.owner_record.accepted_report_event_facts.iter()) {
            for count in [&facts.stdout_bytes, &facts.stderr_bytes] {
                if count.value()? > morrow_agent_session_exec_v1_r2::MAX_OUTPUT_BYTES {
                    return Err("OUTPUT_BOUND");
                }
            }
            let empty = morrow_agent_session_exec_v1_r2::hash(&[]);
            if (facts.stdout_bytes.value()? == 0 && facts.stdout_sha256 != empty)
                || (facts.stderr_bytes.value()? == 0 && facts.stderr_sha256 != empty) {
                return Err("EMPTY_OUTPUT_DIGEST");
            }
        }
        match self.stage {
            Stage::NativeStart if self.record_point == RecordPoint::PreStartClaimOwnerRecord
                && self.original_task_join.is_none() && self.acknowledgement == Ack::NotObserved => Ok(()),
            Stage::NativeJoin if self.record_point == RecordPoint::PostOriginalJoinOwnerRecord
                && self.acknowledgement == Ack::OriginalTaskAckAndAd31Observed
                && self.owner_record.invocation_started => {
                let join = self.original_task_join.as_ref().ok_or("MISSING_ORIGINAL_JOIN")?;
                if !join.execution_result_ok || !join.disconnect_result_ok || !join.maintenance_result_ok
                    || !join.resource_tables_clean_observed || !join.pending_fact_table_empty_observed {
                    return Err("ORIGINAL_JOIN_CHECK_FAILED");
                }
                let facts = self.owner_record.latest_event_facts.as_ref().ok_or("MISSING_EVENT_FACTS")?;
                if !facts.protocol_output_closed || facts.protocol_exit_code_i32.is_none() {
                    return Err("INCOMPLETE_PROTOCOL_FACTS");
                }
                Ok(())
            }
            _ => Err("STAGE_SHAPE"),
        }
    }
}
/// Pure bounded STRUCTURAL decoder. A decoded observation is historical input,
/// not evidence authentication, a native witness, admission, claim or control.
pub fn decode_observation(bytes: &[u8]) -> Result<NativeObservation, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_OBSERVATION_BYTES { return Err("OBSERVATION_BOUND"); }
    let value: NativeObservation = serde_json::from_slice(bytes).map_err(|_| "OBSERVATION_SCHEMA")?;
    value.validate_shape()?;
    Ok(value)
}
/// Pure correlation check for two exported records; never returns full PASS.
/// Native fields stay NOT_PROVEN even when the logical pair matches.
pub fn validate_pair(start: &NativeObservation, joined: &NativeObservation) -> Result<(), &'static str> {
    start.validate_shape()?; joined.validate_shape()?;
    if start.stage != Stage::NativeStart || joined.stage != Stage::NativeJoin
        || start.logical_started_reply_reference != joined.logical_started_reply_reference
        || start.owner_record.identity != joined.owner_record.identity
        || start.owner_record.record_revision.value()? > joined.owner_record.record_revision.value()?
        || start.owner_record.observation_revision.value()? > joined.owner_record.observation_revision.value()? {
        return Err("ORIGINAL_OWNER_PAIR_MISMATCH");
    }
    if let (Some(a),Some(b))=(&start.runner_owner,&joined.runner_owner) {
        if a.instance!=b.instance||a.startup_attempt!=b.startup_attempt {
            return Err("RUNNER_INSTANCE_PAIR_MISMATCH");
        }
        if let (Some(a),Some(b))=(&a.snapshot,&b.snapshot) {
            if let (Some(ai),Some(bi))=(&a.runner_identity,&b.runner_identity) {
                if ai!=bi {return Err("RUNNER_PID_FILETIME_PAIR_MISMATCH");}
            }
        }
        if let (Some(ar),Some(br))=(&a.revision,&b.revision) {
            if ar.value()?>br.value()? {return Err("RUNNER_REVISION_REGRESSION");}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use morrow_agent_session_exec_v1_r2::ToolPhase;
    fn record() -> ToolObservation {
        ToolObservation { identity: ToolIdentity { operation_id: "pure-protocol-input".into(),
            proposal_sha256: [1;32], intent_sha256: [2;32], execution_domain: "not-native".into(),
            session_id: "not-a-live-owner".into(), session_epoch: 1, generation: 2,
            original_issuer_nonce: [3;32] }, record_revision: 4, record_sha256: [5;32],
            phase: ToolPhase::Approved, invocation_started: false, latest: None,
            observation_revision: 0, accepted_report: None }
    }
    #[test]
    fn bounded_schema_rejects_native_claims_authority_and_noncanonical_numbers() {
        // Synthetic protocol data only; never passed into any live/control API.
        let value = NativeObservation::started(&record(), Handle { nonce: [6;32], generation: 7 });
        let raw = serde_json::to_vec(&value).unwrap();
        assert!(raw.len() < MAX_OBSERVATION_BYTES);
        decode_observation(&raw).unwrap();
        let mut fake = serde_json::to_value(&value).unwrap();
        fake["native"]["get_exit_code_process_raw_dword"] = serde_json::json!(7);
        assert!(decode_observation(&serde_json::to_vec(&fake).unwrap()).is_err());
        fake = serde_json::to_value(&value).unwrap();
        fake["production_qualification"] = serde_json::json!("PASS");
        assert!(decode_observation(&serde_json::to_vec(&fake).unwrap()).is_err());
        fake = serde_json::to_value(&value).unwrap();
        fake["live_handle"] = serde_json::json!(123);
        assert!(decode_observation(&serde_json::to_vec(&fake).unwrap()).is_err());
        fake = serde_json::to_value(&value).unwrap();
        fake["owner_record"]["record_revision"] = serde_json::json!("04");
        assert!(decode_observation(&serde_json::to_vec(&fake).unwrap()).is_err());
        assert!(decode_observation(&vec![b' '; MAX_OBSERVATION_BYTES + 1]).is_err());
    }
    #[test]
    fn pair_rejects_cross_owner_and_missing_join_instead_of_promoting_protocol_exit() {
        let mut record = record();
        let handle = Handle { nonce: [6;32], generation: 7 };
        let start = NativeObservation::started(&record, handle);
        record.invocation_started = true;
        record.latest = Some(ExecutionFacts { exit_code: Some(7), output_closed: true,
            stdout_sha256: [8;32], stderr_sha256: [9;32], stdout_bytes: 1, stderr_bytes: 1 });
        let checks = VerifiedOwnerJoin::captured(true, true, true, true, true);
        let mut joined = NativeObservation::joined(&record, handle, Some(&checks));
        validate_pair(&start, &joined).unwrap();
        joined.owner_record.identity.full_identity_sha256[0] ^= 1;
        assert!(validate_pair(&start, &joined).is_err());
        joined = NativeObservation::joined(&record, handle, None);
        assert!(joined.validate_shape().is_err());
        joined = NativeObservation::joined(&record, handle, Some(&checks));
        assert_eq!(joined.native.get_exit_code_process_raw_dword, NotProven::NotProven);
        assert_eq!(joined.production_qualification, NotProven::NotProven);
    }
    #[test]
    fn unsupported_runner_and_busy_payload_cannot_turn_protocol_exit_into_native_proof() {
        let unsupported=codex_exec_server::RunnerObservationReader::unsupported().read();
        let value=NativeObservation::started(&record(),Handle{nonce:[6;32],generation:7})
            .with_runner_owner(Some(unsupported));
        let json=serde_json::to_vec(&value).unwrap();
        assert!(json.len()<MAX_OBSERVATION_BYTES);
        let parsed=decode_observation(&json).unwrap();
        assert_eq!(parsed.runner_owner.as_ref().unwrap().availability,OwnerAvailability::Unsupported);
        assert!(parsed.runner_owner.as_ref().unwrap().snapshot.is_none());
        assert_eq!(parsed.native.get_exit_code_process_raw_dword,NotProven::NotProven);
        let mut forged=serde_json::to_value(&value).unwrap();
        forged["runner_owner"]["scope"]=serde_json::json!("BUSINESS_CHILD_NATIVE_PROOF");
        assert!(decode_observation(&serde_json::to_vec(&forged).unwrap()).is_err());
    }

    #[test]
    fn protocol_exit_never_fills_runner_dword_and_busy_data_is_rejected() {
        use codex_exec_server::{RunnerOwnerSnapshot,RunnerWorkerHistory,RunnerWorkerPlacement,
            RunnerLifecycle,RunnerFailure,RunnerFinalization,RunnerObservation,
            RunnerObservationAvailability,RunnerObservationKey};
        let worker=RunnerWorkerHistory{placement:RunnerWorkerPlacement::Pending,within_deadline:None,
            result_clean:None,physical_eof:None,protocol_exit_i32:None,termination_sent:None,joined_error:false};
        let mut snapshot=RunnerOwnerSnapshot{lifecycle:RunnerLifecycle::FailedOrAbandoned,
            runner_identity:None,runner_get_exit_code_process_u32:None,negotiated_handshake:None,
            protocol_business_exit_i32:Some(7),reader_physical_eof_observed:false,
            no_replay_sealed:true,first_error:Some(RunnerFailure::WriterFailed),
            reader:worker,writer:worker,input:worker,thread_disposition:None,
            thread_close_debt_os_error:Some(5),process_handle_retained:true,native_thread_anchors_retained:true,
            finalization:RunnerFinalization::NotFinalized,finalizer_epoch:1,worker_admission_active:false,
            helpers:[None,None]};
        let actual=|snapshot|RunnerObservation{availability:RunnerObservationAvailability::Observed,
            key:Some(RunnerObservationKey{instance:1,startup_attempt:9}),revision:Some(2),snapshot:Some(snapshot)};
        let value=NativeObservation::started(&record(),Handle{nonce:[6;32],generation:7})
            .with_runner_owner(Some(actual(snapshot)));
        assert!(value.runner_owner.as_ref().unwrap().snapshot.as_ref().unwrap()
            .runner_get_exit_code_process_u32.is_none());
        assert_eq!(value.native.get_exit_code_process_raw_dword,NotProven::NotProven);
        let raw=serde_json::to_vec(&value).unwrap();assert!(raw.len()<MAX_OBSERVATION_BYTES);
        decode_observation(&raw).unwrap();
        let mut forged=serde_json::to_value(&value).unwrap();
        forged["runner_owner"]["availability"]=serde_json::json!("BUSY");
        assert!(decode_observation(&serde_json::to_vec(&forged).unwrap()).is_err());
        forged=serde_json::to_value(&value).unwrap();
        forged["runner_owner"]["snapshot"]["runner_get_exit_code_process_u32"]=serde_json::json!(-1);
        assert!(decode_observation(&serde_json::to_vec(&forged).unwrap()).is_err());
        snapshot.runner_get_exit_code_process_u32=Some(0xffff_ff07);
        let value=NativeObservation::started(&record(),Handle{nonce:[6;32],generation:7})
            .with_runner_owner(Some(actual(snapshot)));
        assert_eq!(decode_observation(&serde_json::to_vec(&value).unwrap()).unwrap()
            .runner_owner.unwrap().snapshot.unwrap().runner_get_exit_code_process_u32,Some(0xffff_ff07));
    }

}
