//! Pure versioned wire-codec and receiver state machine for explicitly opted-in
//! native-close evidence transport (`version = 8`).
//!
//! # Scope and Authority Invariants
//! - **Runtime transport**: `native_close_runtime` supplies the explicitly selected
//!   live envelope on original runner/host IO. Receipt fact validation remains pure;
//!   only actual original owners supply physical postconditions. SDK/outer keeper,
//!   quota and factory permission remain separate and pending.
//! - **Existing v6/v7 preserved byte-for-byte**: `IPC_PROTOCOL_VERSION` (6),
//!   `CHECKED_IPC_PROTOCOL_VERSION` (7), `Message`, and `ControlHello` remain untouched.
//! - **Caller-supplied facts remain fact data**: Wire payloads contain no public
//!   deserializable `bool` fields and never confer resource/cleanup authority by
//!   deserialization alone.
//! - **Business `Exit` is not native child exit**: Protocol business `Exit` only
//!   signals business stream completion; it does not prove native child exit, handle
//!   closure, Job active-zero drain, or lease retirement.
//! - **Sender receipt writer cannot claim join before sending**: The sender's final
//!   receipt writer is transmitting the receipt itself and must report
//!   `SenderReceiptWriterFact::PendingCurrentReceiptFlushAndClose`. Claiming it has
//!   already joined before sending is rejected as a protocol violation.
//! - **Receiver physical close is tracked separately**: Even after a valid receipt is
//!   accepted, the receiver separately awaits physical pipe EOF, receiver worker native
//!   joins, and native runner process exit/handle closure.
//! - **No global product close assertion**: Even when a single attempt's receipt and
//!   receiver physical close are validated, this protocol reports
//!   `ProductCloseScopeDisposition::SingleAttemptEvidenceOnlyNotProductGlobalClose` and
//!   never independently asserts that the entire product is physically closed.
//! - **Sticky primary error preservation**: Any receipt with close debt, poison,
//!   sticky `Unknown`, missing/unproven facts, or correlation/order violations is
//!   permanently `NOT_ACCEPTED`. Subsequent physical cleanup observations never
//!   upgrade the outcome to clean or overwrite the primary error.

#![allow(dead_code)]

use serde::Deserialize;
use serde::Serialize;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::num::{NonZeroU32, NonZeroU64};

/// Explicit opt-in protocol version for terminal native-close receipt transport.
/// Never enabled by default on v6 (`IPC_PROTOCOL_VERSION`) or v7 (`CHECKED_IPC_PROTOCOL_VERSION`).
pub const NATIVE_CLOSE_IPC_PROTOCOL_VERSION: u8 = 8;

/// Compatibility schema pin for the v8 native-close evidence protocol.
/// Distinct from `CHECKED_CONTROL_SCHEMA` and never all-zeros.
pub const NATIVE_CLOSE_PROTOCOL_SCHEMA: [u8; 32] = *b"morrow.c28.native-close.ack-v1.\x08";

/// Hard upper bound on serialized byte length of any single v8 wire frame payload.
pub const MAX_NATIVE_CLOSE_FRAME_BYTES: usize = 16 * 1024;

/// Maximum JSON nesting depth permitted by the strict v8 wire validator.
pub const MAX_NATIVE_CLOSE_JSON_DEPTH: usize = 16;

/// Maximum number of output stream frames permitted in one v8 attempt before business `Exit`.
pub const MAX_OUTPUT_FRAMES_PER_ATTEMPT: u32 = 65_536;

/// Maximum number of control result frames permitted in one v8 attempt before business `Exit`.
pub const MAX_CONTROL_FRAMES_PER_ATTEMPT: u32 = 256;

/// Maximum total v8 wire frames permitted across a single startup attempt.
pub const MAX_TOTAL_FRAMES_PER_ATTEMPT: u32 =
    1 + 1 + MAX_OUTPUT_FRAMES_PER_ATTEMPT + MAX_CONTROL_FRAMES_PER_ATTEMPT + 1 + 1;

/// Maximum number of terminal native-close receipts permitted per attempt.
pub const MAX_RECEIPTS_PER_ATTEMPT: u8 = 1;

/// Win32 `STILL_ACTIVE` (`STATUS_PENDING` = `0x0000_0103` = 259).
/// Per Win32 `GetExitCodeProcess` semantics, `259` is returned while a process is still active AND
/// when an already-exited process terminated with raw exit code `259`. Raw `259` is rejected when
/// positive original-handle exit wait proof is absent, query-only, or timed out, and preserved as a
/// legitimate raw exit code when disambiguated by
/// `OriginalProcessWaitProof::SignaledExitedOnOriginalHandle` and
/// `TimeoutDisposition::CompletedWithinBudget`.
pub const WIN32_STILL_ACTIVE_EXIT_CODE_DWORD: u32 = 0x0000_0103;

// ============================================================================
// Distinct Typed Identity, Correlation, and Secret-Redacted Nonce Primitives
// ============================================================================

/// Non-zero startup attempt identifier binding one runner launch attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StartupAttemptId(NonZeroU64);

impl StartupAttemptId {
    pub fn try_new(raw: u64) -> Result<Self, NativeCloseProtocolError> {
        NonZeroU64::new(raw)
            .map(Self)
            .ok_or(NativeCloseProtocolError::InvalidZeroStartupAttemptId)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Private 32-byte session nonce reused internally for wire authentication association.
///
/// Never serialized directly in cleartext on the v8 wire and always redacted in `Debug`
/// and public projections.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SessionNonce([u8; 32]);

impl SessionNonce {
    pub fn try_new(bytes: [u8; 32]) -> Result<Self, NativeCloseProtocolError> {
        if bytes == [0u8; 32] {
            return Err(NativeCloseProtocolError::InvalidZeroSessionNonce);
        }
        Ok(Self(bytes))
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SessionNonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionNonce([REDACTED_NONCE])")
    }
}

/// Validated 32-byte protocol schema identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaDigest([u8; 32]);

impl SchemaDigest {
    pub const fn v8_native_close() -> Self {
        Self(NATIVE_CLOSE_PROTOCOL_SCHEMA)
    }

    pub fn try_from_wire(bytes: [u8; 32]) -> Result<Self, NativeCloseProtocolError> {
        if bytes != NATIVE_CLOSE_PROTOCOL_SCHEMA {
            return Err(NativeCloseProtocolError::SchemaMismatch);
        }
        Ok(Self(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Explicit capability opt-in required to negotiate v8 native-close receipt transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeCloseCapabilityOptIn {
    ExplicitlyOptedInV8,
    DisabledOrNotRequested,
}

/// Keyed SHA-256 authentication tag associating a wire frame with the private session nonce.
///
/// Redacted in `Debug` so authentication material is never leaked in logs or handoffs.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WireAuthTag([u8; 32]);

impl WireAuthTag {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for WireAuthTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WireAuthTag([REDACTED_AUTH_TAG])")
    }
}

/// Immutable host/runner correlation binding for a single startup attempt.
#[derive(Clone, PartialEq, Eq)]
pub struct StartupAttemptBinding {
    startup_attempt_id: StartupAttemptId,
    session_nonce: SessionNonce,
    expected_schema: SchemaDigest,
    capability_opt_in: NativeCloseCapabilityOptIn,
}

impl StartupAttemptBinding {
    pub fn try_new(
        startup_attempt_id: StartupAttemptId,
        session_nonce: SessionNonce,
        expected_schema: SchemaDigest,
        capability_opt_in: NativeCloseCapabilityOptIn,
    ) -> Result<Self, NativeCloseProtocolError> {
        if capability_opt_in != NativeCloseCapabilityOptIn::ExplicitlyOptedInV8 {
            return Err(NativeCloseProtocolError::MissingCapabilityOptIn);
        }
        if expected_schema.as_bytes() != &NATIVE_CLOSE_PROTOCOL_SCHEMA {
            return Err(NativeCloseProtocolError::SchemaMismatch);
        }
        Ok(Self {
            startup_attempt_id,
            session_nonce,
            expected_schema,
            capability_opt_in,
        })
    }

    pub const fn startup_attempt_id(&self) -> StartupAttemptId {
        self.startup_attempt_id
    }

    pub(crate) const fn session_nonce(&self) -> &SessionNonce {
        &self.session_nonce
    }

    pub const fn expected_schema(&self) -> SchemaDigest {
        self.expected_schema
    }

    pub const fn capability_opt_in(&self) -> NativeCloseCapabilityOptIn {
        self.capability_opt_in
    }
}

impl fmt::Debug for StartupAttemptBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StartupAttemptBinding")
            .field("startup_attempt_id", &self.startup_attempt_id)
            .field("session_nonce", &"[REDACTED_NONCE]")
            .field("expected_schema", &self.expected_schema)
            .field("capability_opt_in", &self.capability_opt_in)
            .finish()
    }
}

/// Validated non-zero Win32 child process ID (`DWORD`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonZeroChildPid(NonZeroU32);

impl NonZeroChildPid {
    pub fn try_new(pid: u32) -> Result<Self, NativeCloseProtocolError> {
        NonZeroU32::new(pid)
            .map(Self)
            .ok_or(NativeCloseProtocolError::InvalidZeroChildIdentity)
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Validated non-zero Win32 child primary thread ID (`DWORD`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonZeroChildThreadId(NonZeroU32);

impl NonZeroChildThreadId {
    pub fn try_new(tid: u32) -> Result<Self, NativeCloseProtocolError> {
        NonZeroU32::new(tid)
            .map(Self)
            .ok_or(NativeCloseProtocolError::InvalidZeroChildIdentity)
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Validated non-zero Win32 process creation `FILETIME` in 100-nanosecond ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChildCreationFiletime100ns(NonZeroU64);

impl ChildCreationFiletime100ns {
    pub fn try_new(ticks_100ns: u64) -> Result<Self, NativeCloseProtocolError> {
        NonZeroU64::new(ticks_100ns)
            .map(Self)
            .ok_or(NativeCloseProtocolError::InvalidZeroChildIdentity)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Raw Win32 `DWORD` exit code returned by `GetExitCodeProcess` on the original child handle.
/// Distinct from `BusinessExitCodeI32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RawProcessExitCodeDword(pub u32);

/// Protocol business `Exit` code (`i32`), distinct from `RawProcessExitCodeDword`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BusinessExitCodeI32(pub i32);

/// Validated non-zero identity of the original launched child process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifiedChildProcessIdentity {
    pub pid: NonZeroChildPid,
    pub thread_id: NonZeroChildThreadId,
    pub creation_filetime_100ns: ChildCreationFiletime100ns,
}

impl VerifiedChildProcessIdentity {
    pub fn try_new(
        pid: u32,
        thread_id: u32,
        creation_filetime_100ns: u64,
    ) -> Result<Self, NativeCloseProtocolError> {
        Ok(Self {
            pid: NonZeroChildPid::try_new(pid)?,
            thread_id: NonZeroChildThreadId::try_new(thread_id)?,
            creation_filetime_100ns: ChildCreationFiletime100ns::try_new(creation_filetime_100ns)?,
        })
    }

    pub const fn to_wire(self) -> WireChildIdentity {
        WireChildIdentity {
            pid: self.pid.get(),
            thread_id: self.thread_id.get(),
            creation_filetime_100ns: self.creation_filetime_100ns.get(),
        }
    }
}

/// Wire representation of child process identity. Validated to reject zero/default values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireChildIdentity {
    pub pid: u32,
    pub thread_id: u32,
    pub creation_filetime_100ns: u64,
}

impl WireChildIdentity {
    pub fn validate(&self) -> Result<VerifiedChildProcessIdentity, NativeCloseProtocolError> {
        VerifiedChildProcessIdentity::try_new(
            self.pid,
            self.thread_id,
            self.creation_filetime_100ns,
        )
    }
}

// ============================================================================
// Wire Fact Enums (No Deserializable `bool` Authority; Preserves NotObserved/NotProven)
// ============================================================================

/// Proof state for an observation on an original resource owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationProof {
    NotObserved,
    NotProven,
    ProvenOnOriginal,
}

/// Timeout disposition for waits or business exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeoutDisposition {
    CompletedWithinBudget,
    TimedOut,
}

/// Ordering proof between dependent shutdown stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageOrderingFact {
    NotObserved,
    NotProven,
    ProvenInStrictOrder,
    OrderViolated,
}

/// Disambiguated proof state for waiting on the original child process handle prior to reading
/// `GetExitCodeProcess`.
///
/// Per Win32 `GetExitCodeProcess` semantics, `WIN32_STILL_ACTIVE_EXIT_CODE_DWORD` (`259`) is
/// returned both for an active process and for a terminated process that exited with code `259`.
/// Only `SignaledExitedOnOriginalHandle` paired with `TimeoutDisposition::CompletedWithinBudget`
/// proves that the original child process terminated before its raw exit code was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginalProcessWaitProof {
    NotObserved,
    NotProven,
    QueryOnlyWithoutWaitSignal,
    WaitTimedOutOrStillActive,
    SignaledExitedOnOriginalHandle,
}

/// Domain 1: Real child native identity and raw exit code fact.
/// Explicitly distinct from protocol business `Exit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChildNativeExitFact {
    NotObserved,
    NotProven,
    NotCreatedPreLaunch,
    WaitTimedOutOrIncomplete {
        identity: WireChildIdentity,
    },
    Witnessed {
        identity: WireChildIdentity,
        raw_exit_code_dword: u32,
        business_exit_code_i32: i32,
        wait_on_original_handle: OriginalProcessWaitProof,
        timeout_disposition: TimeoutDisposition,
    },
}

/// Pipe EOF observation state for stdout/stderr readers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum PipeEofFact {
    NotObserved,
    NotProven,
    CleanEofObserved,
    ReadFailedBeforeEof { os_error: Option<i32> },
}

/// Native OS thread join observation for a retained worker `JoinHandle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerNativeJoinFact {
    NotObserved,
    NotProven,
    StillRunning,
    JoinedTimelyClean,
    JoinedWithWorkerError,
    JoinedLateAfterBudget,
    TimedOutStillRunning,
    WaitFailed { os_error: Option<i32> },
    Panicked,
}

/// Checked `CloseHandle` disposition for an owned native handle.
/// Never carries raw `HANDLE` integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum HandleCloseFact {
    NotObserved,
    NotProven,
    ClosedVerifiedOnce,
    NotApplicableAbsent,
    StillOpen,
    CloseDebtRetained { os_error: Option<i32> },
}

/// Output stream (stdout/stderr) EOF, native reader join, and handle closure facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputStreamWorkerFact {
    pub pipe_eof: PipeEofFact,
    pub reader_join: WorkerNativeJoinFact,
    pub handle_close: HandleCloseFact,
}

/// Stderr stream fact: either a separate stream or merged into ConPTY / absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum StderrStreamWorkerFact {
    MergedConptyOrAbsent { handle_close: HandleCloseFact },
    SeparateStream { stream: OutputStreamWorkerFact },
}

/// Input pipe peer EOF observation state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputPeerEofFact {
    NotObserved,
    NotProven,
    CleanPostChildExitFrameBoundaryEof,
    PrematureDisconnectBeforeChildExit,
    PartialFrameAtEof,
    FrameTooLarge,
    MalformedFrame,
    OperationAborted,
    IoError { os_error: Option<i32> },
}

/// Whether the input loop received an explicit `Terminate` or disconnect-termination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputTerminationFact {
    NoTerminationObserved,
    ExplicitTerminateReceived,
    TerminatedOnDisconnect,
}

/// Input worker EOF, native join, and termination facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputStreamWorkerFact {
    pub peer_eof: InputPeerEofFact,
    pub reader_join: WorkerNativeJoinFact,
    pub termination_disposition: InputTerminationFact,
}

/// Domain 2: Original stdout, stderr, and input EOF and native reader joins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IoStreamsAndWorkersFacts {
    pub stdout: OutputStreamWorkerFact,
    pub stderr: StderrStreamWorkerFact,
    pub input: InputStreamWorkerFact,
}

/// Write health of child stdin prior to single checked close.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StdinWriteDisposition {
    AllWritesSucceeded,
    WriteFailedBeforeClose,
}

/// Single-owner child stdin handle closure disposition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum StdinHandleCloseFact {
    NotObserved,
    NotProven,
    InitiallyNotOpen,
    ClosedOnceVerified {
        write_disposition: StdinWriteDisposition,
    },
    StillOpen,
    ClosedMultipleTimes {
        close_count: u32,
    },
    CloseDebtRetained {
        close_count: u32,
        os_error: Option<i32>,
    },
}

/// ConPTY pseudo-console owner closure disposition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConptyClosureFact {
    NotObserved,
    NotProven,
    NotUsedPipeMode,
    ClosedBeforeOutputJoinVerified,
    StillActiveOrPoisoned,
}

/// Domain 3: Original stdin, ConPTY, and child thread/process handle closure facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalHandlesCloseFacts {
    pub conpty_closure: ConptyClosureFact,
    pub stdin_closure: StdinHandleCloseFact,
    pub child_primary_thread_handle: HandleCloseFact,
    pub child_process_handle: HandleCloseFact,
    pub close_ordering_after_io_joins: StageOrderingFact,
}

/// JobObject admission and identity correlation fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum JobAdmissionFact {
    NotObserved,
    NotProven,
    JobMissing,
    AdmittedVerifiedOriginalJob {
        job_instance_id: u64,
        child_membership_verified: ObservationProof,
    },
}

/// JobObject active-zero drain fact on the same retained original JobObject.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum JobActiveZeroDrainFact {
    NotObserved,
    NotProven,
    DrainedToActiveZero {
        total_processes: u32,
        active_processes: u32,
        total_terminated_processes: u32,
        ordering_after_child_handle_close: StageOrderingFact,
    },
    DrainTimedOut {
        active_processes: u32,
    },
    DrainFailed {
        os_error: Option<i32>,
    },
}

/// Forbidden Job operations on the checked normal-close path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobForbiddenOpFact {
    NotInvoked,
    InvokedViolation,
}

/// Disposition of extra JobObject members beyond the single verified child process.
///
/// Aggregate `JOBOBJECT_BASIC_ACCOUNTING_INFORMATION` counters (`TotalProcesses > 1`)
/// cannot prove the identity or origin of extra processes; any extra members observed
/// must be reported honestly as `ExtraMembersObservedOriginNotProvenByJobHelper` and
/// fail closed during strict single-attempt native-close validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtraJobMembersOriginFact {
    NotObserved,
    NotProven,
    SingleProcessObservedOnly,
    ExtraMembersObservedOriginNotProvenByJobHelper,
}

/// Domain 4: Original Job admission/identity and active-zero drain facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalJobLifecycleFacts {
    pub admission: JobAdmissionFact,
    pub active_zero_drain: JobActiveZeroDrainFact,
    pub extra_member_origin: ExtraJobMembersOriginFact,
    pub preserve_descendants: JobForbiddenOpFact,
    pub terminate_job: JobForbiddenOpFact,
}

/// Anchored business `Exit` writer join disposition prior to terminal receipt construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum BusinessExitWriterFact {
    NotObserved,
    NotProven,
    JoinedTimelyBeforeReceipt { exit_code_i32: i32 },
    StillRunning,
    WriteOrJoinFailed,
    Panicked,
}

/// Prior control-result and startup/error frame writers disposition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum PriorControlWritersFact {
    NotObserved,
    NotProven,
    AllJoinedAndQuiesced { control_results_written: u16 },
    WriterStillRunning,
    ControlWriteFailed,
    PrecontextWriterRetained,
}

/// Disposition of the sender's own terminal receipt writer at the time the receipt is encoded.
///
/// Because the sender's receipt writer is in the middle of sending the receipt frame itself,
/// it CANNOT honestly claim to have already joined before sending. Only
/// `PendingCurrentReceiptFlushAndClose` is valid on the wire; `ClaimedJoinedBeforeSend`
/// is rejected as a fabricated/impossible state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderReceiptWriterFact {
    NotObserved,
    NotProven,
    PendingCurrentReceiptFlushAndClose,
    ClaimedJoinedBeforeSend,
    WriteFailed,
}

/// Domain 5: Original control writer closure and pending sender receipt writer facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriterClosureFacts {
    pub business_exit_writer: BusinessExitWriterFact,
    pub prior_control_and_frame_writers: PriorControlWritersFact,
    pub sender_receipt_writer: SenderReceiptWriterFact,
}

/// Retirement or scope-boundary state for runner-owned vs host-owned leases.
///
/// - Runner-owned input reader/stdin retirement (`runner_input_and_stdin_lease`) must be
///   `RetiredCleanly` in the runner's terminal receipt.
/// - Host-owned leases (`input_lease` for host input pipe writer/keeper, `keeper_lease` for
///   host output pipe reader / private desktop / ACL keepers, and `quota_lease` for host
///   local `CheckedOwnerRegistry` slot / outer quota adapter) are out of the runner's scope.
///   In the runner receipt they MUST be reported as `OutOfRunnerScopePendingHostOwner`.
///   A runner receipt claiming `RetiredCleanly` for any host-owned lease is rejected as a
///   scope violation, and the host receiver must supply actual host-owner retirement proof
///   at the final physical-close gate before any local registry slot can be released.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseRetirementFact {
    NotObserved,
    NotProven,
    ActiveOrLoaned,
    ParkedUnresolved,
    OutOfRunnerScopePendingHostOwner,
    RetiredCleanly,
}

/// Retirement state for the single-attempt reservation/context lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttemptLeaseRetirementFact {
    NotObserved,
    NotProven,
    ActiveOrLoaned,
    ParkedUnresolved,
    ReadyToRetireOnFinalReceiptFlush { startup_attempt_id: u64 },
}

/// Domain 6: Runner-owned retirement and host-owned lease scope-boundary facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseRetirementFacts {
    pub runner_input_and_stdin_lease: LeaseRetirementFact,
    pub input_lease: LeaseRetirementFact,
    pub keeper_lease: LeaseRetirementFact,
    pub quota_lease: LeaseRetirementFact,
    pub attempt_lease: AttemptLeaseRetirementFact,
}

/// Named target categories for handle close debt (never exposes raw OS `HANDLE` values).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseDebtTarget {
    HostRunnerProcessHandle,
    HostInputPipeWriter,
    HostOutputPipeReader,
    HostStartupPipeAlias,
    HostWorkerThreadAnchor,
    HostDesktopHandle,
    HostDesktopToken,
    HostDesktopSecurityDescriptor,
    ChildProcessHandle,
    ChildPrimaryThreadHandle,
    ChildStdoutHandle,
    ChildStderrHandle,
    ChildStdinHandle,
    SpawnPipeAuxiliaryHandle,
}

/// Summary of handle close debt across all context owners.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum CloseDebtFact {
    NotObserved,
    NotProven,
    ZeroDebtProven,
    DebtRetained {
        debt_count: u8,
        first_target: CloseDebtTarget,
        first_os_error: Option<i32>,
    },
}

/// Typed sticky `Unknown` failure categories reportable in wire facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolStickyFailureKind {
    RuntimeTransportFailed,
    StartupValidationFailed,
    SpawnChildFailed,
    WriteSpawnReadyFailed,
    ChildWaitFailed,
    ChildExitCodeUnavailable,
    ChildTimedOut,
    ExplicitUserTerminate,
    ConptyLockPoisoned,
    StdoutForwardingFailed,
    StdoutJoinTimedOut,
    StdoutWorkerPanicked,
    StderrForwardingFailed,
    StderrJoinTimedOut,
    StderrWorkerPanicked,
    BusinessExitGateViolation,
    BusinessExitWriteFailed,
    InputPartialFrame,
    InputFrameTooLarge,
    InputMalformedFrame,
    InputProtocolViolation,
    InputIoError,
    InputPrematureDisconnect,
    InputControlWriteFailed,
    InputStdinCloseFailed,
    InputJoinTimedOut,
    InputWorkerPanicked,
    ChildHandleCloseFailed,
    JobPreserveDescendantsInvoked,
    JobTerminateInvoked,
    JobDrainFailed,
    JobDrainTimedOut,
    OwnerMutexPoisoned,
}

/// Lifecycle stage where the first sticky failure was latched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolFailureStage {
    StartupAndHandshake,
    ChildWait,
    OutputDrainAndJoin,
    BusinessExitWrite,
    InputDrainAndJoin,
    HandleClosure,
    JobActiveZeroDrain,
    LeaseRetirement,
}

/// Replay sealing policy on sticky `Unknown`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayDisposition {
    NoReplaySealed,
    UnsealedInvalid,
}

/// First error record reported by the sender's sticky `Unknown` latch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum FirstErrorFact {
    NotObserved,
    NoneObserved,
    Recorded {
        kind: ProtocolStickyFailureKind,
        stage: ProtocolFailureStage,
        os_error: Option<i32>,
        replay_policy: ReplayDisposition,
    },
}

/// Target of a mutex or owner poison observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoisonTarget {
    ChildProcessOwnerMutex,
    ConptyOwnerMutex,
    OutputPipeWriterMutex,
    StdinCloseDebtMutex,
    StickyLatchMutex,
    AttemptSlotMutex,
    HostOwnerMutex,
}

/// Mutex/owner poison fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum PoisonStateFact {
    NotObserved,
    NotProven,
    NoPoisonObserved,
    PoisonObserved { target: PoisonTarget },
}

/// Sticky `Unknown` latch state fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum StickyUnknownStateFact {
    NotObserved,
    NotProven,
    Clear,
    LatchedStickyUnknown { replay_policy: ReplayDisposition },
}

/// Domain 7: Close debt, first error, mutex/owner poison, and sticky `Unknown` facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebtAndStickyUnknownFacts {
    pub close_debt: CloseDebtFact,
    pub first_error: FirstErrorFact,
    pub poison_state: PoisonStateFact,
    pub sticky_unknown_state: StickyUnknownStateFact,
}

/// Complete set of 7 separated fact domains carried by a v8 `NativeCloseReceiptWire`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportedNativeCloseFacts {
    pub child_exit: ChildNativeExitFact,
    pub io_streams_and_workers: IoStreamsAndWorkersFacts,
    pub handles_close: OriginalHandlesCloseFacts,
    pub job_lifecycle: OriginalJobLifecycleFacts,
    pub writers: WriterClosureFacts,
    pub leases: LeaseRetirementFacts,
    pub debt_and_sticky_unknown: DebtAndStickyUnknownFacts,
}

// ============================================================================
// Version 8 Bounded Wire Envelopes & Keyed Authentication Helpers
// ============================================================================

/// Handshake Hello payload for v8 native-close transport opt-in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCloseHandshakeHelloWire {
    pub protocol_version: u8,
    pub schema: [u8; 32],
    pub startup_attempt_id: u64,
    pub capability: NativeCloseCapabilityOptIn,
    pub auth_tag: WireAuthTag,
}

impl NativeCloseHandshakeHelloWire {
    pub fn new_signed(binding: &StartupAttemptBinding) -> Self {
        let protocol_version = NATIVE_CLOSE_IPC_PROTOCOL_VERSION;
        let schema = *binding.expected_schema().as_bytes();
        let startup_attempt_id = binding.startup_attempt_id().get();
        let capability = binding.capability_opt_in();
        let auth_tag = compute_handshake_auth_tag(
            binding.session_nonce(),
            b"morrow.c28.v8.handshake.hello\0",
            protocol_version,
            &schema,
            startup_attempt_id,
            capability,
        );
        Self {
            protocol_version,
            schema,
            startup_attempt_id,
            capability,
            auth_tag,
        }
    }

    pub fn verify_auth_tag(&self, nonce: &SessionNonce) -> bool {
        let expected = compute_handshake_auth_tag(
            nonce,
            b"morrow.c28.v8.handshake.hello\0",
            self.protocol_version,
            &self.schema,
            self.startup_attempt_id,
            self.capability,
        );
        self.auth_tag == expected
    }
}

/// Handshake Ack payload from runner confirming v8 native-close receipt obligation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCloseHandshakeAckWire {
    pub protocol_version: u8,
    pub schema: [u8; 32],
    pub startup_attempt_id: u64,
    pub capability: NativeCloseCapabilityOptIn,
    pub auth_tag: WireAuthTag,
}

impl NativeCloseHandshakeAckWire {
    pub fn new_signed(binding: &StartupAttemptBinding) -> Self {
        let protocol_version = NATIVE_CLOSE_IPC_PROTOCOL_VERSION;
        let schema = *binding.expected_schema().as_bytes();
        let startup_attempt_id = binding.startup_attempt_id().get();
        let capability = binding.capability_opt_in();
        let auth_tag = compute_handshake_auth_tag(
            binding.session_nonce(),
            b"morrow.c28.v8.handshake.ack\0",
            protocol_version,
            &schema,
            startup_attempt_id,
            capability,
        );
        Self {
            protocol_version,
            schema,
            startup_attempt_id,
            capability,
            auth_tag,
        }
    }

    pub fn verify_auth_tag(&self, nonce: &SessionNonce) -> bool {
        let expected = compute_handshake_auth_tag(
            nonce,
            b"morrow.c28.v8.handshake.ack\0",
            self.protocol_version,
            &self.schema,
            self.startup_attempt_id,
            self.capability,
        );
        self.auth_tag == expected
    }
}

/// SpawnReady wire frame binding the launched child's `(pid, thread_id, creation_filetime_100ns)`
/// to the negotiated v8 attempt before any output, business `Exit`, or receipt is accepted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCloseSpawnReadyWire {
    pub startup_attempt_id: u64,
    pub child_identity: WireChildIdentity,
    pub auth_tag: WireAuthTag,
}

impl NativeCloseSpawnReadyWire {
    pub fn new_signed(
        binding: &StartupAttemptBinding,
        child_identity: VerifiedChildProcessIdentity,
    ) -> Self {
        let wire_identity = child_identity.to_wire();
        let startup_attempt_id = binding.startup_attempt_id().get();
        let auth_tag = compute_spawn_ready_auth_tag(
            binding.session_nonce(),
            startup_attempt_id,
            &wire_identity,
        );
        Self {
            startup_attempt_id,
            child_identity: wire_identity,
            auth_tag,
        }
    }

    pub fn verify_auth_tag(&self, nonce: &SessionNonce) -> bool {
        let expected = compute_spawn_ready_auth_tag(
            nonce,
            self.startup_attempt_id,
            &self.child_identity,
        );
        self.auth_tag == expected
    }
}

/// Stream identifier for output frames observed before business `Exit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeCloseOutputStream {
    Stdout,
    Stderr,
}

/// Output frame summary tracked by the v8 receiver state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCloseOutputFrameWire {
    pub stream: NativeCloseOutputStream,
    pub byte_len: u32,
}

/// Control result frame summary tracked by the v8 receiver state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCloseControlResultWire {
    pub request_id: u64,
    pub status: super::ipc_framed::ControlStatus,
}

/// Business `Exit` wire frame in the v8 envelope.
///
/// Explicitly distinct from `ChildNativeExitFact` and `NativeCloseReceiptWire`:
/// business `Exit` only marks the end of output streaming and carries the business exit code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCloseBusinessExitWire {
    pub exit_code: i32,
    pub timeout_disposition: TimeoutDisposition,
}

impl NativeCloseBusinessExitWire {
    pub const fn from_exit_payload(payload: &super::ipc_framed::ExitPayload) -> Self {
        Self {
            exit_code: payload.exit_code,
            timeout_disposition: if payload.timed_out {
                TimeoutDisposition::TimedOut
            } else {
                TimeoutDisposition::CompletedWithinBudget
            },
        }
    }
}

/// Single terminal native-close receipt frame sent after business `Exit` and before physical pipe EOF.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCloseReceiptWire {
    pub protocol_version: u8,
    pub schema: [u8; 32],
    pub startup_attempt_id: u64,
    pub sequence_number: u8,
    pub facts: ReportedNativeCloseFacts,
    pub auth_tag: WireAuthTag,
}

impl NativeCloseReceiptWire {
    pub fn new_signed(
        binding: &StartupAttemptBinding,
        facts: ReportedNativeCloseFacts,
    ) -> Self {
        let protocol_version = NATIVE_CLOSE_IPC_PROTOCOL_VERSION;
        let schema = *binding.expected_schema().as_bytes();
        let startup_attempt_id = binding.startup_attempt_id().get();
        let sequence_number = 1;
        let auth_tag = compute_receipt_auth_tag(
            binding.session_nonce(),
            protocol_version,
            &schema,
            startup_attempt_id,
            sequence_number,
            &facts,
        );
        Self {
            protocol_version,
            schema,
            startup_attempt_id,
            sequence_number,
            facts,
            auth_tag,
        }
    }

    pub fn recompute_auth_tag(&mut self, nonce: &SessionNonce) {
        self.auth_tag = compute_receipt_auth_tag(
            nonce,
            self.protocol_version,
            &self.schema,
            self.startup_attempt_id,
            self.sequence_number,
            &self.facts,
        );
    }

    pub fn verify_auth_tag(&self, nonce: &SessionNonce) -> bool {
        let expected = compute_receipt_auth_tag(
            nonce,
            self.protocol_version,
            &self.schema,
            self.startup_attempt_id,
            self.sequence_number,
            &self.facts,
        );
        self.auth_tag == expected
    }
}

/// Version 8 tagged message variants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case", deny_unknown_fields)]
pub enum Version8Message {
    RuntimeBootstrap(Box<super::native_close_runtime::RuntimeBootstrapWire>),
    RuntimePacket(Box<super::native_close_runtime::RuntimePacketWire>),
    HandshakeHello(NativeCloseHandshakeHelloWire),
    HandshakeAck(NativeCloseHandshakeAckWire),
    SpawnReady(NativeCloseSpawnReadyWire),
    Output(NativeCloseOutputFrameWire),
    ControlResult(NativeCloseControlResultWire),
    BusinessExit(NativeCloseBusinessExitWire),
    NativeCloseReceipt(Box<NativeCloseReceiptWire>),
}

/// Bounded version 8 wire envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Version8WireFrame {
    pub version: u8,
    pub message: Version8Message,
}

impl Version8WireFrame {
    pub const fn new(message: Version8Message) -> Self {
        Self {
            version: NATIVE_CLOSE_IPC_PROTOCOL_VERSION,
            message,
        }
    }
}

fn compute_handshake_auth_tag(
    nonce: &SessionNonce,
    domain: &[u8],
    protocol_version: u8,
    schema: &[u8; 32],
    startup_attempt_id: u64,
    capability: NativeCloseCapabilityOptIn,
) -> WireAuthTag {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(nonce.as_bytes());
    hasher.update([protocol_version]);
    hasher.update(schema);
    hasher.update(startup_attempt_id.to_le_bytes());
    hasher.update([match capability {
        NativeCloseCapabilityOptIn::ExplicitlyOptedInV8 => 1u8,
        NativeCloseCapabilityOptIn::DisabledOrNotRequested => 0u8,
    }]);
    WireAuthTag(hasher.finalize().into())
}

fn compute_spawn_ready_auth_tag(
    nonce: &SessionNonce,
    startup_attempt_id: u64,
    child_identity: &WireChildIdentity,
) -> WireAuthTag {
    let mut hasher = Sha256::new();
    hasher.update(b"morrow.c28.v8.spawn_ready\0");
    hasher.update(nonce.as_bytes());
    hasher.update(startup_attempt_id.to_le_bytes());
    hasher.update(child_identity.pid.to_le_bytes());
    hasher.update(child_identity.thread_id.to_le_bytes());
    hasher.update(child_identity.creation_filetime_100ns.to_le_bytes());
    WireAuthTag(hasher.finalize().into())
}

fn compute_receipt_auth_tag(
    nonce: &SessionNonce,
    protocol_version: u8,
    schema: &[u8; 32],
    startup_attempt_id: u64,
    sequence_number: u8,
    facts: &ReportedNativeCloseFacts,
) -> WireAuthTag {
    let canonical_facts_bytes =
        serde_json::to_vec(facts).expect("ReportedNativeCloseFacts serialization is infallible");
    let mut hasher = Sha256::new();
    hasher.update(b"morrow.c28.v8.native_close_receipt\0");
    hasher.update(nonce.as_bytes());
    hasher.update([protocol_version, sequence_number]);
    hasher.update(schema);
    hasher.update(startup_attempt_id.to_le_bytes());
    hasher.update((canonical_facts_bytes.len() as u32).to_le_bytes());
    hasher.update(&canonical_facts_bytes);
    WireAuthTag(hasher.finalize().into())
}

// ============================================================================
// Pure Bounded Wire Codec & Strict JSON Structure Validator
// ============================================================================

/// Redacted structural location for a duplicate JSON object key.
/// Never retains or echoes untrusted key names or field values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DuplicateFieldError {
    pub line: usize,
    pub column: usize,
}

/// Bounded structural category for an unrecognized schema token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownSchemaKind {
    Field,
    Variant,
}

/// Redacted structural details for an unknown JSON field or enum variant.
/// Never retains or echoes untrusted field or variant names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownSchemaError {
    pub kind: UnknownSchemaKind,
    pub line: usize,
    pub column: usize,
}

/// Bounded structural category for malformed or type-invalid JSON payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MalformedJsonKind {
    NonObjectRootEnvelope,
    SyntaxError,
    UnexpectedEof,
    InvalidType,
    InvalidValue,
    InvalidLength,
    MissingField,
    DataContractError,
    IoError,
}

/// Redacted structural details for malformed or type-invalid JSON payloads.
/// Never retains or echoes untrusted field names, variant names, or literal values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MalformedJsonError {
    pub kind: MalformedJsonKind,
    pub line: usize,
    pub column: usize,
}

/// Bounded structural category for JSON frame encoding failures.
/// Never retains or echoes raw payload strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeErrorKind {
    SerializationFailed,
}

/// Wire framing and JSON decoding errors for the v8 codec.
///
/// Stores only bounded structural categories and numeric byte/line/column positions;
/// never retains raw strings from `serde_json::Error` or untrusted wire payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeCloseCodecError {
    EmptyFrame,
    PartialHeader { bytes_read: usize },
    PartialPayload { expected: usize, available: usize },
    FrameTooLarge { len: usize, max: usize },
    TrailingBytesAfterFrame { trailing_bytes: usize },
    TrailingBytesInPayload,
    MaxJsonDepthExceeded { max_depth: usize },
    DuplicateField(DuplicateFieldError),
    UnknownField(UnknownSchemaError),
    MalformedJson(MalformedJsonError),
    EncodeFailed(EncodeErrorKind),
    StreamIoError { os_error: Option<i32> },
}

impl fmt::Display for NativeCloseCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyFrame => f.write_str("empty v8 wire frame"),
            Self::PartialHeader { bytes_read } => {
                write!(f, "partial v8 frame header ({bytes_read} bytes read)")
            }
            Self::PartialPayload {
                expected,
                available,
            } => write!(
                f,
                "partial v8 frame payload (expected {expected} bytes, available {available} bytes)"
            ),
            Self::FrameTooLarge { len, max } => {
                write!(f, "v8 frame length {len} exceeds maximum {max}")
            }
            Self::TrailingBytesAfterFrame { trailing_bytes } => {
                write!(f, "v8 frame followed by {trailing_bytes} trailing bytes")
            }
            Self::TrailingBytesInPayload => f.write_str("v8 JSON payload contains trailing bytes"),
            Self::MaxJsonDepthExceeded { max_depth } => {
                write!(f, "v8 JSON payload exceeded maximum nesting depth {max_depth}")
            }
            Self::DuplicateField(info) => write!(
                f,
                "duplicate JSON object field at line {}, column {}",
                info.line, info.column
            ),
            Self::UnknownField(info) => write!(
                f,
                "unknown JSON schema {:?} at line {}, column {}",
                info.kind, info.line, info.column
            ),
            Self::MalformedJson(info) => write!(
                f,
                "malformed JSON ({:?}) at line {}, column {}",
                info.kind, info.line, info.column
            ),
            Self::EncodeFailed(kind) => write!(f, "v8 JSON frame encode failed ({kind:?})"),
            Self::StreamIoError { os_error } => {
                write!(f, "v8 stream I/O error (os_error={os_error:?})")
            }
        }
    }
}

impl std::error::Error for NativeCloseCodecError {}

struct DepthAndDuplicateChecker {
    depth: usize,
    max_depth: usize,
}

impl<'de> DeserializeSeed<'de> for DepthAndDuplicateChecker {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if self.depth > self.max_depth {
            return Err(de::Error::custom(format!(
                "max_json_depth_exceeded:{}",
                self.max_depth
            )));
        }
        deserializer.deserialize_any(StrictValueVisitor {
            depth: self.depth,
            max_depth: self.max_depth,
        })
    }
}

struct StrictValueVisitor {
    depth: usize,
    max_depth: usize,
}

impl<'de> Visitor<'de> for StrictValueVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any valid JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, _v: bool) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_i64<E>(self, _v: i64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_u64<E>(self, _v: u64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_f64<E>(self, _v: f64) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_str<E>(self, _v: &str) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_string<E>(self, _v: String) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        DepthAndDuplicateChecker {
            depth: self.depth + 1,
            max_depth: self.max_depth,
        }
        .deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        if self.depth + 1 > self.max_depth {
            return Err(de::Error::custom(format!(
                "max_json_depth_exceeded:{}",
                self.max_depth
            )));
        }
        while seq
            .next_element_seed(DepthAndDuplicateChecker {
                depth: self.depth + 1,
                max_depth: self.max_depth,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        if self.depth + 1 > self.max_depth {
            return Err(de::Error::custom(format!(
                "max_json_depth_exceeded:{}",
                self.max_depth
            )));
        }
        let mut seen_keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen_keys.insert(key) {
                return Err(de::Error::custom("duplicate_object_key"));
            }
            map.next_value_seed(DepthAndDuplicateChecker {
                depth: self.depth + 1,
                max_depth: self.max_depth,
            })?;
        }
        Ok(())
    }
}

fn classify_serde_json_error(err: serde_json::Error) -> NativeCloseCodecError {
    let line = err.line();
    let column = err.column();
    let category = err.classify();
    let msg = err.to_string();
    if msg.contains("max_json_depth_exceeded:") {
        NativeCloseCodecError::MaxJsonDepthExceeded {
            max_depth: MAX_NATIVE_CLOSE_JSON_DEPTH,
        }
    } else if msg.contains("duplicate_object_key") || msg.starts_with("duplicate field") {
        NativeCloseCodecError::DuplicateField(DuplicateFieldError { line, column })
    } else if msg.starts_with("unknown field") {
        NativeCloseCodecError::UnknownField(UnknownSchemaError {
            kind: UnknownSchemaKind::Field,
            line,
            column,
        })
    } else if msg.starts_with("unknown variant") {
        NativeCloseCodecError::UnknownField(UnknownSchemaError {
            kind: UnknownSchemaKind::Variant,
            line,
            column,
        })
    } else if msg.contains("trailing characters") {
        NativeCloseCodecError::TrailingBytesInPayload
    } else {
        let kind = match category {
            serde_json::error::Category::Io => MalformedJsonKind::IoError,
            serde_json::error::Category::Eof => MalformedJsonKind::UnexpectedEof,
            serde_json::error::Category::Syntax => MalformedJsonKind::SyntaxError,
            serde_json::error::Category::Data => {
                if msg.starts_with("missing field") {
                    MalformedJsonKind::MissingField
                } else if msg.starts_with("invalid type") {
                    MalformedJsonKind::InvalidType
                } else if msg.starts_with("invalid value") {
                    MalformedJsonKind::InvalidValue
                } else if msg.starts_with("invalid length") {
                    MalformedJsonKind::InvalidLength
                } else {
                    MalformedJsonKind::DataContractError
                }
            }
        };
        NativeCloseCodecError::MalformedJson(MalformedJsonError { kind, line, column })
    }
}

/// Validates and decodes a single unframed JSON payload slice into a `Version8WireFrame`.
///
/// Rejects empty payloads, oversize payloads, leading/trailing whitespace or trailing bytes,
/// duplicate keys at any nesting level, unknown fields/variants, and malformed JSON.
pub fn decode_v8_json_payload_exact(
    payload: &[u8],
) -> Result<Version8WireFrame, NativeCloseCodecError> {
    if payload.is_empty() {
        return Err(NativeCloseCodecError::EmptyFrame);
    }
    if payload.len() > MAX_NATIVE_CLOSE_FRAME_BYTES {
        return Err(NativeCloseCodecError::FrameTooLarge {
            len: payload.len(),
            max: MAX_NATIVE_CLOSE_FRAME_BYTES,
        });
    }
    // Reject leading or trailing whitespace/garbage around the root JSON object.
    if payload.first() != Some(&b'{') || payload.last() != Some(&b'}') {
        if payload.first() == Some(&b'{') {
            return Err(NativeCloseCodecError::TrailingBytesInPayload);
        }
        return Err(NativeCloseCodecError::MalformedJson(MalformedJsonError {
            kind: MalformedJsonKind::NonObjectRootEnvelope,
            line: 1,
            column: 1,
        }));
    }

    // Pass 1: Verify nesting depth, duplicate keys at every object level, and absence of trailing tokens.
    let mut raw_de = serde_json::Deserializer::from_slice(payload);
    DepthAndDuplicateChecker {
        depth: 0,
        max_depth: MAX_NATIVE_CLOSE_JSON_DEPTH,
    }
    .deserialize(&mut raw_de)
    .map_err(classify_serde_json_error)?;
    raw_de.end().map_err(classify_serde_json_error)?;

    // Pass 2: Deserialize into strongly typed structs/enums with `#[serde(deny_unknown_fields)]`.
    serde_json::from_slice::<Version8WireFrame>(payload).map_err(classify_serde_json_error)
}

/// Encodes a `Version8WireFrame` into a 4-byte little-endian length-prefixed byte vector,
/// enforcing `MAX_NATIVE_CLOSE_FRAME_BYTES`.
pub fn encode_v8_length_prefixed_frame(
    frame: &Version8WireFrame,
) -> Result<Vec<u8>, NativeCloseCodecError> {
    let payload = serde_json::to_vec(frame)
        .map_err(|_err| NativeCloseCodecError::EncodeFailed(EncodeErrorKind::SerializationFailed))?;
    if payload.is_empty() {
        return Err(NativeCloseCodecError::EmptyFrame);
    }
    if payload.len() > MAX_NATIVE_CLOSE_FRAME_BYTES {
        return Err(NativeCloseCodecError::FrameTooLarge {
            len: payload.len(),
            max: MAX_NATIVE_CLOSE_FRAME_BYTES,
        });
    }
    let len_u32 = payload.len() as u32;
    let mut framed = Vec::with_capacity(4 + payload.len());
    framed.extend_from_slice(&len_u32.to_le_bytes());
    framed.extend_from_slice(&payload);
    Ok(framed)
}

/// Decodes an exact 4-byte length-prefixed `Version8WireFrame`, rejecting any trailing
/// bytes after the single frame as well as partial headers, truncated payloads, oversize
/// frames, unknown fields, duplicate fields, and malformed JSON.
pub fn decode_v8_length_prefixed_frame_exact(
    framed_bytes: &[u8],
) -> Result<Version8WireFrame, NativeCloseCodecError> {
    if framed_bytes.is_empty() {
        return Err(NativeCloseCodecError::EmptyFrame);
    }
    if framed_bytes.len() < 4 {
        return Err(NativeCloseCodecError::PartialHeader {
            bytes_read: framed_bytes.len(),
        });
    }
    let mut header = [0u8; 4];
    header.copy_from_slice(&framed_bytes[..4]);
    let len = u32::from_le_bytes(header) as usize;
    if len == 0 {
        return Err(NativeCloseCodecError::EmptyFrame);
    }
    if len > MAX_NATIVE_CLOSE_FRAME_BYTES {
        return Err(NativeCloseCodecError::FrameTooLarge {
            len,
            max: MAX_NATIVE_CLOSE_FRAME_BYTES,
        });
    }
    let available = framed_bytes.len() - 4;
    if available < len {
        return Err(NativeCloseCodecError::PartialPayload {
            expected: len,
            available,
        });
    }
    if available > len {
        return Err(NativeCloseCodecError::TrailingBytesAfterFrame {
            trailing_bytes: available - len,
        });
    }
    decode_v8_json_payload_exact(&framed_bytes[4..4 + len])
}

/// Reads the next length-prefixed `Version8WireFrame` from an in-memory byte slice cursor,
/// returning `Ok(None)` on a clean 0-byte frame boundary EOF.
pub fn decode_next_v8_frame_from_cursor(
    cursor: &mut &[u8],
) -> Result<Option<Version8WireFrame>, NativeCloseCodecError> {
    if cursor.is_empty() {
        return Ok(None);
    }
    if cursor.len() < 4 {
        return Err(NativeCloseCodecError::PartialHeader {
            bytes_read: cursor.len(),
        });
    }
    let mut header = [0u8; 4];
    header.copy_from_slice(&cursor[..4]);
    let len = u32::from_le_bytes(header) as usize;
    if len == 0 {
        return Err(NativeCloseCodecError::EmptyFrame);
    }
    if len > MAX_NATIVE_CLOSE_FRAME_BYTES {
        return Err(NativeCloseCodecError::FrameTooLarge {
            len,
            max: MAX_NATIVE_CLOSE_FRAME_BYTES,
        });
    }
    let available = cursor.len() - 4;
    if available < len {
        return Err(NativeCloseCodecError::PartialPayload {
            expected: len,
            available,
        });
    }
    let payload = &cursor[4..4 + len];
    *cursor = &cursor[4 + len..];
    let frame = decode_v8_json_payload_exact(payload)?;
    Ok(Some(frame))
}

/// Reads a single 4-byte little-endian length-prefixed `Version8WireFrame` from `reader`
/// with strict `MAX_NATIVE_CLOSE_FRAME_BYTES` pre-allocation enforcement.
///
/// - Returns `Ok(None)` only on a clean 0-byte EOF before the first header byte.
/// - Rejects partial headers (`1..=3` bytes before EOF), zero-length frames, frames exceeding
///   `MAX_NATIVE_CLOSE_FRAME_BYTES`, truncated payloads, unknown/duplicate fields, and trailing bytes.
pub fn read_v8_wire_frame_bounded<R: std::io::Read>(
    reader: &mut R,
) -> Result<Option<Version8WireFrame>, NativeCloseCodecError> {
    let mut header = [0u8; 4];
    let mut header_read = 0usize;
    while header_read < 4 {
        match reader.read(&mut header[header_read..]) {
            Ok(0) => {
                if header_read == 0 {
                    return Ok(None);
                }
                return Err(NativeCloseCodecError::PartialHeader {
                    bytes_read: header_read,
                });
            }
            Ok(n) => {
                header_read += n;
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) if err.raw_os_error()==Some(109) => {
                if header_read==0 {return Ok(None);}
                return Err(NativeCloseCodecError::PartialHeader{bytes_read:header_read});
            }
            Err(err) => {
                return Err(NativeCloseCodecError::StreamIoError {
                    os_error: err.raw_os_error(),
                });
            }
        }
    }
    let len = u32::from_le_bytes(header) as usize;
    if len == 0 {
        return Err(NativeCloseCodecError::EmptyFrame);
    }
    if len > MAX_NATIVE_CLOSE_FRAME_BYTES {
        return Err(NativeCloseCodecError::FrameTooLarge {
            len,
            max: MAX_NATIVE_CLOSE_FRAME_BYTES,
        });
    }
    let mut payload = vec![0u8; len];
    let mut payload_read = 0usize;
    while payload_read < len {
        match reader.read(&mut payload[payload_read..]) {
            Ok(0) => {
                return Err(NativeCloseCodecError::PartialPayload {
                    expected: len,
                    available: payload_read,
                });
            }
            Ok(n) => {
                payload_read += n;
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) if err.raw_os_error()==Some(109) => return Err(NativeCloseCodecError::PartialPayload {
                expected:len,available:payload_read,
            }),
            Err(err) => {
                return Err(NativeCloseCodecError::StreamIoError {
                    os_error: err.raw_os_error(),
                });
            }
        }
    }
    let frame = decode_v8_json_payload_exact(&payload)?;
    Ok(Some(frame))
}

/// Encodes and writes a single 4-byte little-endian length-prefixed `Version8WireFrame`
/// to `writer` and flushes the stream, enforcing `MAX_NATIVE_CLOSE_FRAME_BYTES`.
pub fn write_v8_wire_frame_bounded<W: std::io::Write>(
    writer: &mut W,
    frame: &Version8WireFrame,
) -> Result<usize, NativeCloseCodecError> {
    let framed = encode_v8_length_prefixed_frame(frame)?;
    writer
        .write_all(&framed)
        .map_err(|err| NativeCloseCodecError::StreamIoError {
            os_error: err.raw_os_error(),
        })?;
    writer
        .flush()
        .map_err(|err| NativeCloseCodecError::StreamIoError {
            os_error: err.raw_os_error(),
        })?;
    Ok(framed.len())
}

// ============================================================================
// Receiver Validation Errors, Stage Machine, and Actionable Typed Output
// ============================================================================

/// Specific failure causes for Domain 2 (IO streams and workers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoFactFailureReason {
    StdoutEofNotProven,
    StdoutReaderJoinNotTimelyClean,
    StdoutHandleNotClosedVerified,
    StderrEofNotProven,
    StderrReaderJoinNotTimelyClean,
    StderrHandleDispositionInvalid,
    InputPeerEofNotPostExitClean,
    InputReaderJoinNotTimelyClean,
    InputTerminationInvoked,
}

/// Specific failure causes for Domain 3 (original stdin, ConPTY, and child handle closure).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleCloseFailureReason {
    ConptyNotClosedBeforeOutputJoin,
    StdinCloseNotProvenOnceClean,
    ChildPrimaryThreadHandleNotClosedVerified,
    ChildProcessHandleNotClosedVerified,
    HandleCloseOrderNotAfterIoJoins,
}

/// Specific failure causes for Domain 4 (original Job admission and active-zero drain).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobFactFailureReason {
    JobAdmissionNotProvenOnOriginal,
    InvalidZeroJobInstanceId,
    PreserveDescendantsInvoked,
    TerminateJobInvoked,
    ActiveZeroDrainNotProven,
    ActiveProcessesNonZero,
    TotalProcessesZero,
    TerminatedProcessesNonZero,
    ExtraJobMemberOriginNotProven,
    DrainOrderNotAfterChildHandleClose,
}

/// Specific failure causes for Domain 5 (original control writers & sender receipt writer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriterFactFailureReason {
    BusinessExitWriterNotJoinedTimely,
    BusinessExitWriterCodeMismatch,
    PriorControlWritersNotQuiesced,
    PriorControlResultsCountMismatch { expected: u16, reported: u16 },
    SenderWriterClaimedJoinedBeforeSend,
    SenderWriterNotPendingCurrentFlush,
}

/// Specific failure causes for Domain 6 (runner-owned vs host-owned leases and attempt slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseFactFailureReason {
    RunnerInputAndStdinLeaseNotRetiredCleanly,
    RunnerClaimedHostInputLeaseRetired,
    RunnerClaimedHostKeeperLeaseRetired,
    RunnerClaimedHostQuotaLeaseRetired,
    InputLeaseNotPendingHostOwner,
    KeeperLeaseNotPendingHostOwner,
    QuotaLeaseNotPendingHostOwner,
    AttemptLeaseNotReadyToRetire,
    AttemptLeaseIdMismatch { expected: u64, reported: u64 },
    HostInputLeaseNotRetiredCleanly,
    HostKeeperLeaseNotRetiredCleanly,
    HostLocalRegistrySlotNotRetiredCleanly,
    ExternalQuotaClaimedBySingleAttemptReceipt,
}

/// Primary sticky protocol error categories latched by `NativeCloseReceiverState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeCloseProtocolError {
    InvalidZeroStartupAttemptId,
    InvalidZeroSessionNonce,
    MissingCapabilityOptIn,
    VersionMismatch { expected: u8, observed: u8 },
    SchemaMismatch,
    NonceMismatch,
    AttemptIdMismatch { expected: u64, observed: u64 },
    InvalidZeroChildIdentity,
    ChildIdentityMismatch {
        expected: VerifiedChildProcessIdentity,
        observed: VerifiedChildProcessIdentity,
    },
    CodecError(NativeCloseCodecError),
    FrameCardinalityExceeded { max_frames: u32 },
    OutputCardinalityExceeded { max_output_frames: u32 },
    ControlCardinalityExceeded { max_control_frames: u32 },
    MissingHandshake,
    DuplicateHandshake,
    HandshakeOrderViolation,
    MissingSpawnReady,
    DuplicateSpawnReady,
    FrameBeforeHandshakeOrSpawnReady,
    DuplicateBusinessExit,
    BusinessExitTimedOut,
    OutputAfterBusinessExit,
    ControlAfterBusinessExit,
    ReceiptBeforeBusinessExit,
    DuplicateReceipt,
    InvalidReceiptSequenceNumber { observed: u8 },
    ReceiptAfterPhysicalEof,
    MissingReceiptAtPhysicalEof,
    DuplicatePhysicalEof,
    ReceiptReportsCloseDebt {
        debt_count: u8,
        first_target: CloseDebtTarget,
        first_os_error: Option<i32>,
    },
    ReceiptCloseDebtNotProvenZero,
    ReceiptReportsPoison { target: PoisonTarget },
    ReceiptPoisonStateNotProvenClean,
    ReceiptReportsFirstError {
        kind: ProtocolStickyFailureKind,
        stage: ProtocolFailureStage,
        os_error: Option<i32>,
    },
    ReceiptFirstErrorNotProvenNone,
    ReceiptReportsStickyUnknown,
    ReceiptStickyUnknownNotProvenClear,
    ChildNativeExitNotProven,
    ChildExitStillActiveRawCode,
    ChildExitCodeMismatchWithBusinessExit {
        business_exit_i32: i32,
        receipt_business_i32: i32,
        raw_exit_dword: u32,
    },
    IoFactRejected(IoFactFailureReason),
    HandleCloseFactRejected(HandleCloseFailureReason),
    JobFactRejected(JobFactFailureReason),
    WriterFactRejected(WriterFactFailureReason),
    LeaseFactRejected(LeaseFactFailureReason),
    ReceiverPhysicalCloseBeforeEof,
    ReceiverWorkerJoinFailed,
    RunnerNativeProcessCloseFailed,
    ExternalStickyUnknownLatched(ProtocolStickyFailureKind),
}

impl fmt::Display for NativeCloseProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CodecError(err) => write!(f, "v8 codec error: {err}"),
            other => write!(f, "{other:?}"),
        }
    }
}

impl std::error::Error for NativeCloseProtocolError {}

/// Ordered stages of the v8 native-close receiver state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiverProtocolStage {
    AwaitingHandshakeHello,
    AwaitingHandshakeAck,
    HandshakeNegotiatedAwaitingSpawnReady,
    ActiveStreamingAwaitingBusinessExit,
    BusinessExitObservedAwaitingReceipt,
    ReceiptAcceptedAwaitingPhysicalEof,
    PhysicalEofObservedAwaitingReceiverNativeClose,
    HostNativeClosedPendingOuterScope,
    RunnerAttemptTransportClosed,
    LatchedNotAcceptedStickyUnknown,
}

/// Disposition of the sender's receipt writer from the receiver's perspective.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SenderWriterReceiverTracking {
    ReceiptNotYetAccepted,
    SenderWriterPendingPipeFlushAndEof,
    PipePhysicalEofObserved,
    RejectedOrUnknown,
}

/// Explicit phase gate tracking host-owned resource completion after runner receipt decoding.
///
/// A host receiver must NEVER release a `CheckedOwnerRegistry` slot merely because a
/// decoded runner receipt was accepted (`ReceiptAcceptedPendingPhysicalClose`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOwnerCompletionGate {
    ReceiptNotYetAccepted,
    PendingPhysicalEofAndHostOwnerRetirement,
    PhysicalEofObservedPendingHostWorkersRunnerAndKeepers,
    HostPhysicalCloseAndLocalSlotVerifiedExternalQuotaPendingAdapter,
    RejectedOrStickyUnknown,
}

/// Explicit distinction between local `CheckedOwnerRegistry` (16-slot) capacity and
/// external product/factory quota authority (`ExternalNativeQuotaAdapter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalQuotaScopeDisposition {
    LocalRegistryOnlyExternalQuotaRequiresOuterAdapter,
    InvalidClaimedGlobalQuotaOrFactoryAuthority,
}

/// Receiver-side local worker join, native runner process close, and host-owned keeper/quota
/// retirement observations, tracked separately from the sender's wire receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceiverPhysicalCloseFacts {
    pub reader_worker_joined_timely_clean: ObservationProof,
    pub writer_worker_joined_timely_clean: ObservationProof,
    pub input_worker_joined_timely_clean: ObservationProof,
    pub runner_process_identity: Option<VerifiedChildProcessIdentity>,
    pub runner_exit_code_dword: Option<RawProcessExitCodeDword>,
    pub runner_handle_closed_verified: ObservationProof,
    pub host_input_writer_lease: LeaseRetirementFact,
    pub host_keeper_and_desktop_lease: LeaseRetirementFact,
    pub host_local_registry_quota_lease: LeaseRetirementFact,
    pub external_quota_scope: ExternalQuotaScopeDisposition,
}

impl Default for ReceiverPhysicalCloseFacts {
    fn default() -> Self {
        Self {
            reader_worker_joined_timely_clean: ObservationProof::NotObserved,
            writer_worker_joined_timely_clean: ObservationProof::NotObserved,
            input_worker_joined_timely_clean: ObservationProof::NotObserved,
            runner_process_identity: None,
            runner_exit_code_dword: None,
            runner_handle_closed_verified: ObservationProof::NotObserved,
            host_input_writer_lease: LeaseRetirementFact::NotObserved,
            host_keeper_and_desktop_lease: LeaseRetirementFact::NotObserved,
            host_local_registry_quota_lease: LeaseRetirementFact::NotObserved,
            external_quota_scope:
                ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter,
        }
    }
}

/// Explicit scope boundary marker: the pure protocol validates one attempt's native-close
/// transport evidence and never independently asserts that the entire product is physically closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductCloseScopeDisposition {
    SingleAttemptEvidenceOnlyNotProductGlobalClose,
}

/// Whether late physical cleanup/reaping was observed after a primary sticky error.
/// Never upgrades the attempt to clean business or protocol acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatePhysicalReapDisposition {
    NotObserved,
    ObservedUnderStickyUnknown,
}

/// High-level acceptance status of the native-close receipt and receiver lifecycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiptAcceptanceStatus {
    InProgress {
        stage: ReceiverProtocolStage,
    },
    ReceiptAcceptedPendingPhysicalClose {
        startup_attempt_id: StartupAttemptId,
        child_identity: VerifiedChildProcessIdentity,
        raw_child_exit_dword: RawProcessExitCodeDword,
        business_exit_i32: BusinessExitCodeI32,
        sender_writer_tracking: SenderWriterReceiverTracking,
        host_completion_gate: HostOwnerCompletionGate,
    },
    AttemptTransportClosedEvidenceValidated {
        startup_attempt_id: StartupAttemptId,
        child_identity: VerifiedChildProcessIdentity,
        raw_child_exit_dword: RawProcessExitCodeDword,
        business_exit_i32: BusinessExitCodeI32,
        job_total_processes: u32,
        runner_exit_dword: RawProcessExitCodeDword,
        host_completion_gate: HostOwnerCompletionGate,
        external_quota_scope: ExternalQuotaScopeDisposition,
        product_scope: ProductCloseScopeDisposition,
    },
    NotAcceptedStickyUnknown {
        primary_error: NativeCloseProtocolError,
        no_replay_sealed: ReplayDisposition,
        late_physical_eof: LatePhysicalReapDisposition,
        late_physical_reap: LatePhysicalReapDisposition,
    },
}

/// Validated summary stored inside `NativeCloseReceiverState` only after all 7 receipt
/// fact domains pass validation. Private fields prevent external fabrication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedReceiptEvidence {
    startup_attempt_id: StartupAttemptId,
    child_identity: VerifiedChildProcessIdentity,
    raw_child_exit_dword: RawProcessExitCodeDword,
    business_exit_i32: BusinessExitCodeI32,
    job_instance_id: u64,
    job_total_processes: u32,
    control_results_written: u16,
}

impl ValidatedReceiptEvidence {
    pub const fn startup_attempt_id(&self) -> StartupAttemptId {
        self.startup_attempt_id
    }

    pub const fn child_identity(&self) -> VerifiedChildProcessIdentity {
        self.child_identity
    }

    pub const fn raw_child_exit_dword(&self) -> RawProcessExitCodeDword {
        self.raw_child_exit_dword
    }

    pub const fn business_exit_i32(&self) -> BusinessExitCodeI32 {
        self.business_exit_i32
    }

    pub const fn job_instance_id(&self) -> u64 {
        self.job_instance_id
    }

    pub const fn job_total_processes(&self) -> u32 {
        self.job_total_processes
    }

    pub const fn control_results_written(&self) -> u16 {
        self.control_results_written
    }
}

/// Public secret-free projection of the receiver's current native-close state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeClosePublicProjection {
    pub protocol_version: u8,
    pub startup_attempt_id: StartupAttemptId,
    pub stage: ReceiverProtocolStage,
    pub bound_child_identity: Option<VerifiedChildProcessIdentity>,
    pub business_exit_code: Option<BusinessExitCodeI32>,
    pub validated_receipt: Option<ValidatedReceiptEvidence>,
    pub sender_writer_tracking: SenderWriterReceiverTracking,
    pub receiver_physical_eof: ObservationProof,
    pub host_completion_gate: HostOwnerCompletionGate,
    pub local_registry_slot_releasable: bool,
    pub external_quota_scope: ExternalQuotaScopeDisposition,
    pub acceptance: ReceiptAcceptanceStatus,
    pub primary_error: Option<NativeCloseProtocolError>,
    pub replay_disposition: Option<ReplayDisposition>,
    pub product_scope: ProductCloseScopeDisposition,
}

/// Strictly ordered single-attempt receiver state machine for v8 native-close evidence.
pub struct NativeCloseReceiverState {
    binding: StartupAttemptBinding,
    stage: ReceiverProtocolStage,
    frames_observed: u32,
    output_frames_observed: u32,
    control_frames_observed: u32,
    receipt_frames_observed: u8,
    bound_child_identity: Option<VerifiedChildProcessIdentity>,
    bound_runner_identity: Option<VerifiedChildProcessIdentity>,
    business_exit: Option<NativeCloseBusinessExitWire>,
    validated_receipt: Option<ValidatedReceiptEvidence>,
    receiver_physical_eof_observed: bool,
    receiver_physical_close: ReceiverPhysicalCloseFacts,
    late_physical_reap_observed: bool,
    primary_error: Option<NativeCloseProtocolError>,
}

impl fmt::Debug for NativeCloseReceiverState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeCloseReceiverState")
            .field("startup_attempt_id", &self.binding.startup_attempt_id())
            .field("stage", &self.stage)
            .field("frames_observed", &self.frames_observed)
            .field("output_frames_observed", &self.output_frames_observed)
            .field("control_frames_observed", &self.control_frames_observed)
            .field("receipt_frames_observed", &self.receipt_frames_observed)
            .field("bound_child_identity", &self.bound_child_identity)
            .field("business_exit", &self.business_exit)
            .field("validated_receipt", &self.validated_receipt)
            .field(
                "receiver_physical_eof_observed",
                &self.receiver_physical_eof_observed,
            )
            .field("late_physical_reap_observed", &self.late_physical_reap_observed)
            .field("primary_error", &self.primary_error)
            .finish()
    }
}

impl NativeCloseReceiverState {
    /// Creates a new v8 receiver state machine bound to a single validated `StartupAttemptBinding`.
    pub fn new(binding: StartupAttemptBinding) -> Self {
        Self {
            binding,
            stage: ReceiverProtocolStage::AwaitingHandshakeHello,
            frames_observed: 0,
            output_frames_observed: 0,
            control_frames_observed: 0,
            receipt_frames_observed: 0,
            bound_child_identity: None,
            bound_runner_identity: None,
            business_exit: None,
            validated_receipt: None,
            receiver_physical_eof_observed: false,
            receiver_physical_close: ReceiverPhysicalCloseFacts::default(),
            late_physical_reap_observed: false,
            primary_error: None,
        }
    }

    /// Live transport binds the separately captured original RUNNER, before Hello.
    pub fn bind_original_runner_identity(&mut self, identity: VerifiedChildProcessIdentity) -> Result<(),NativeCloseProtocolError> {
        if self.stage!=ReceiverProtocolStage::AwaitingHandshakeHello || self.bound_runner_identity.is_some() {
            return Err(self.latch_error(NativeCloseProtocolError::HandshakeOrderViolation));
        }
        self.bound_runner_identity=Some(identity);Ok(())
    }

    pub const fn stage(&self) -> ReceiverProtocolStage {
        self.stage
    }

    pub const fn primary_error(&self) -> Option<&NativeCloseProtocolError> {
        self.primary_error.as_ref()
    }

    pub const fn no_replay_sealed(&self) -> bool {
        self.primary_error.is_some()
    }

    pub const fn validated_receipt(&self) -> Option<&ValidatedReceiptEvidence> {
        self.validated_receipt.as_ref()
    }

    pub const fn is_attempt_transport_closed_clean(&self) -> bool {
        matches!(
            self.stage,
            ReceiverProtocolStage::RunnerAttemptTransportClosed
        ) && self.primary_error.is_none()
    }

    /// Latches the primary sticky error if none has been recorded yet, permanently
    /// transitioning the receiver to `LatchedNotAcceptedStickyUnknown` and clearing any
    /// clean receipt acceptance so no late upgrade is possible.
    fn latch_error(&mut self, err: NativeCloseProtocolError) -> NativeCloseProtocolError {
        self.stage = ReceiverProtocolStage::LatchedNotAcceptedStickyUnknown;
        self.validated_receipt = None;
        if let Some(ref existing) = self.primary_error {
            return existing.clone();
        }
        self.primary_error = Some(err.clone());
        err
    }

    fn count_frame(&mut self) -> Result<(), NativeCloseProtocolError> {
        if self.frames_observed >= MAX_TOTAL_FRAMES_PER_ATTEMPT {
            return Err(self.latch_error(NativeCloseProtocolError::FrameCardinalityExceeded {
                max_frames: MAX_TOTAL_FRAMES_PER_ATTEMPT,
            }));
        }
        self.frames_observed += 1;
        Ok(())
    }

    /// Feeds raw length-prefixed v8 frame bytes into the receiver state machine.
    pub fn apply_length_prefixed_frame_bytes(
        &mut self,
        framed_bytes: &[u8],
    ) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        let frame = decode_v8_length_prefixed_frame_exact(framed_bytes)
            .map_err(|err| self.latch_error(NativeCloseProtocolError::CodecError(err)))?;
        self.apply_wire_frame(&frame)
    }

    /// Applies a decoded `Version8WireFrame` to the receiver state machine.
    pub fn apply_wire_frame(
        &mut self,
        frame: &Version8WireFrame,
    ) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        if frame.version != NATIVE_CLOSE_IPC_PROTOCOL_VERSION {
            return Err(self.latch_error(NativeCloseProtocolError::VersionMismatch {
                expected: NATIVE_CLOSE_IPC_PROTOCOL_VERSION,
                observed: frame.version,
            }));
        }
        match &frame.message {
            Version8Message::RuntimeBootstrap(_) | Version8Message::RuntimePacket(_) =>
                Err(self.latch_error(NativeCloseProtocolError::HandshakeOrderViolation)),
            Version8Message::HandshakeHello(hello) => self.record_handshake_hello(hello),
            Version8Message::HandshakeAck(ack) => self.record_handshake_ack(ack),
            Version8Message::SpawnReady(ready) => self.record_spawn_ready(ready),
            Version8Message::Output(output) => self.record_output_frame(output),
            Version8Message::ControlResult(ctrl) => self.record_control_result_frame(ctrl),
            Version8Message::BusinessExit(exit) => self.record_business_exit(exit),
            Version8Message::NativeCloseReceipt(receipt) => {
                self.record_native_close_receipt(receipt)
            }
        }
    }

    /// Step 1a: Validates the outgoing/initial `NativeCloseHandshakeHelloWire`.
    pub fn record_handshake_hello(
        &mut self,
        hello: &NativeCloseHandshakeHelloWire,
    ) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        self.count_frame()?;
        if let Some(ref existing) = self.primary_error {
            return Err(existing.clone());
        }
        if self.stage != ReceiverProtocolStage::AwaitingHandshakeHello {
            return Err(self.latch_error(NativeCloseProtocolError::DuplicateHandshake));
        }
        if hello.protocol_version != NATIVE_CLOSE_IPC_PROTOCOL_VERSION {
            return Err(self.latch_error(NativeCloseProtocolError::VersionMismatch {
                expected: NATIVE_CLOSE_IPC_PROTOCOL_VERSION,
                observed: hello.protocol_version,
            }));
        }
        if hello.schema != *self.binding.expected_schema().as_bytes() {
            return Err(self.latch_error(NativeCloseProtocolError::SchemaMismatch));
        }
        if hello.capability != NativeCloseCapabilityOptIn::ExplicitlyOptedInV8 {
            return Err(self.latch_error(NativeCloseProtocolError::MissingCapabilityOptIn));
        }
        if hello.startup_attempt_id == 0 {
            return Err(self.latch_error(NativeCloseProtocolError::InvalidZeroStartupAttemptId));
        }
        if hello.startup_attempt_id != self.binding.startup_attempt_id().get() {
            return Err(self.latch_error(NativeCloseProtocolError::AttemptIdMismatch {
                expected: self.binding.startup_attempt_id().get(),
                observed: hello.startup_attempt_id,
            }));
        }
        if !hello.verify_auth_tag(self.binding.session_nonce()) {
            return Err(self.latch_error(NativeCloseProtocolError::NonceMismatch));
        }
        self.stage = ReceiverProtocolStage::AwaitingHandshakeAck;
        Ok(self.stage)
    }

    /// Step 1b: Validates the runner's `NativeCloseHandshakeAckWire`.
    pub fn record_handshake_ack(
        &mut self,
        ack: &NativeCloseHandshakeAckWire,
    ) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        self.count_frame()?;
        if let Some(ref existing) = self.primary_error {
            return Err(existing.clone());
        }
        match self.stage {
            ReceiverProtocolStage::AwaitingHandshakeAck => {}
            ReceiverProtocolStage::AwaitingHandshakeHello => {
                return Err(self.latch_error(NativeCloseProtocolError::HandshakeOrderViolation));
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::DuplicateHandshake));
            }
        }
        if ack.protocol_version != NATIVE_CLOSE_IPC_PROTOCOL_VERSION {
            return Err(self.latch_error(NativeCloseProtocolError::VersionMismatch {
                expected: NATIVE_CLOSE_IPC_PROTOCOL_VERSION,
                observed: ack.protocol_version,
            }));
        }
        if ack.schema != *self.binding.expected_schema().as_bytes() {
            return Err(self.latch_error(NativeCloseProtocolError::SchemaMismatch));
        }
        if ack.capability != NativeCloseCapabilityOptIn::ExplicitlyOptedInV8 {
            return Err(self.latch_error(NativeCloseProtocolError::MissingCapabilityOptIn));
        }
        if ack.startup_attempt_id == 0 {
            return Err(self.latch_error(NativeCloseProtocolError::InvalidZeroStartupAttemptId));
        }
        if ack.startup_attempt_id != self.binding.startup_attempt_id().get() {
            return Err(self.latch_error(NativeCloseProtocolError::AttemptIdMismatch {
                expected: self.binding.startup_attempt_id().get(),
                observed: ack.startup_attempt_id,
            }));
        }
        if !ack.verify_auth_tag(self.binding.session_nonce()) {
            return Err(self.latch_error(NativeCloseProtocolError::NonceMismatch));
        }
        self.stage = ReceiverProtocolStage::HandshakeNegotiatedAwaitingSpawnReady;
        Ok(self.stage)
    }

    /// Step 2: Validates `NativeCloseSpawnReadyWire` and binds the non-zero child identity.
    pub fn record_spawn_ready(
        &mut self,
        ready: &NativeCloseSpawnReadyWire,
    ) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        self.count_frame()?;
        if let Some(ref existing) = self.primary_error {
            return Err(existing.clone());
        }
        match self.stage {
            ReceiverProtocolStage::HandshakeNegotiatedAwaitingSpawnReady => {}
            ReceiverProtocolStage::AwaitingHandshakeHello
            | ReceiverProtocolStage::AwaitingHandshakeAck => {
                return Err(self.latch_error(NativeCloseProtocolError::MissingHandshake));
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::DuplicateSpawnReady));
            }
        }
        if ready.startup_attempt_id == 0 {
            return Err(self.latch_error(NativeCloseProtocolError::InvalidZeroStartupAttemptId));
        }
        if ready.startup_attempt_id != self.binding.startup_attempt_id().get() {
            return Err(self.latch_error(NativeCloseProtocolError::AttemptIdMismatch {
                expected: self.binding.startup_attempt_id().get(),
                observed: ready.startup_attempt_id,
            }));
        }
        let verified_identity = ready
            .child_identity
            .validate()
            .map_err(|err| self.latch_error(err))?;
        if !ready.verify_auth_tag(self.binding.session_nonce()) {
            return Err(self.latch_error(NativeCloseProtocolError::NonceMismatch));
        }
        self.bound_child_identity = Some(verified_identity);
        self.stage = ReceiverProtocolStage::ActiveStreamingAwaitingBusinessExit;
        Ok(self.stage)
    }

    /// Step 3a: Records an output frame during active streaming, rejecting any output after `BusinessExit`.
    pub fn record_output_frame(
        &mut self,
        _output: &NativeCloseOutputFrameWire,
    ) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        self.count_frame()?;
        if let Some(ref existing) = self.primary_error {
            return Err(existing.clone());
        }
        match self.stage {
            ReceiverProtocolStage::ActiveStreamingAwaitingBusinessExit => {}
            ReceiverProtocolStage::AwaitingHandshakeHello
            | ReceiverProtocolStage::AwaitingHandshakeAck => {
                return Err(self.latch_error(NativeCloseProtocolError::MissingHandshake));
            }
            ReceiverProtocolStage::HandshakeNegotiatedAwaitingSpawnReady => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::FrameBeforeHandshakeOrSpawnReady)
                );
            }
            ReceiverProtocolStage::BusinessExitObservedAwaitingReceipt
            | ReceiverProtocolStage::ReceiptAcceptedAwaitingPhysicalEof
            | ReceiverProtocolStage::PhysicalEofObservedAwaitingReceiverNativeClose
            | ReceiverProtocolStage::HostNativeClosedPendingOuterScope
            | ReceiverProtocolStage::RunnerAttemptTransportClosed => {
                return Err(self.latch_error(NativeCloseProtocolError::OutputAfterBusinessExit));
            }
            ReceiverProtocolStage::LatchedNotAcceptedStickyUnknown => {
                return Err(self
                    .primary_error
                    .clone()
                    .expect("primary_error present in LatchedNotAcceptedStickyUnknown"));
            }
        }
        if self.output_frames_observed >= MAX_OUTPUT_FRAMES_PER_ATTEMPT {
            return Err(self.latch_error(NativeCloseProtocolError::OutputCardinalityExceeded {
                max_output_frames: MAX_OUTPUT_FRAMES_PER_ATTEMPT,
            }));
        }
        self.output_frames_observed += 1;
        Ok(self.stage)
    }

    /// Step 3b: Records a control result frame during active streaming, rejecting any control frame after `BusinessExit`.
    pub fn record_control_result_frame(
        &mut self,
        _control: &NativeCloseControlResultWire,
    ) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        self.count_frame()?;
        if let Some(ref existing) = self.primary_error {
            return Err(existing.clone());
        }
        match self.stage {
            ReceiverProtocolStage::ActiveStreamingAwaitingBusinessExit => {}
            ReceiverProtocolStage::AwaitingHandshakeHello
            | ReceiverProtocolStage::AwaitingHandshakeAck => {
                return Err(self.latch_error(NativeCloseProtocolError::MissingHandshake));
            }
            ReceiverProtocolStage::HandshakeNegotiatedAwaitingSpawnReady => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::FrameBeforeHandshakeOrSpawnReady)
                );
            }
            ReceiverProtocolStage::BusinessExitObservedAwaitingReceipt
            | ReceiverProtocolStage::ReceiptAcceptedAwaitingPhysicalEof
            | ReceiverProtocolStage::PhysicalEofObservedAwaitingReceiverNativeClose
            | ReceiverProtocolStage::HostNativeClosedPendingOuterScope
            | ReceiverProtocolStage::RunnerAttemptTransportClosed => {
                return Err(self.latch_error(NativeCloseProtocolError::ControlAfterBusinessExit));
            }
            ReceiverProtocolStage::LatchedNotAcceptedStickyUnknown => {
                return Err(self
                    .primary_error
                    .clone()
                    .expect("primary_error present in LatchedNotAcceptedStickyUnknown"));
            }
        }
        if self.control_frames_observed >= MAX_CONTROL_FRAMES_PER_ATTEMPT {
            return Err(
                self.latch_error(NativeCloseProtocolError::ControlCardinalityExceeded {
                    max_control_frames: MAX_CONTROL_FRAMES_PER_ATTEMPT,
                }),
            );
        }
        self.control_frames_observed += 1;
        Ok(self.stage)
    }

    /// Step 4: Records the business `Exit` frame.
    ///
    /// Business `Exit` does NOT prove native child exit or resource cleanup; it transitions the
    /// receiver into `BusinessExitObservedAwaitingReceipt`.
    pub fn record_business_exit(
        &mut self,
        exit: &NativeCloseBusinessExitWire,
    ) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        self.count_frame()?;
        if let Some(ref existing) = self.primary_error {
            return Err(existing.clone());
        }
        match self.stage {
            ReceiverProtocolStage::ActiveStreamingAwaitingBusinessExit => {}
            ReceiverProtocolStage::AwaitingHandshakeHello
            | ReceiverProtocolStage::AwaitingHandshakeAck => {
                return Err(self.latch_error(NativeCloseProtocolError::MissingHandshake));
            }
            ReceiverProtocolStage::HandshakeNegotiatedAwaitingSpawnReady => {
                return Err(self.latch_error(NativeCloseProtocolError::MissingSpawnReady));
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::DuplicateBusinessExit));
            }
        }
        self.business_exit = Some(*exit);
        if exit.timeout_disposition != TimeoutDisposition::CompletedWithinBudget {
            return Err(self.latch_error(NativeCloseProtocolError::BusinessExitTimedOut));
        }
        self.stage = ReceiverProtocolStage::BusinessExitObservedAwaitingReceipt;
        Ok(self.stage)
    }

    /// Step 5: Validates the single terminal `NativeCloseReceiptWire` after business `Exit`.
    pub fn record_native_close_receipt(
        &mut self,
        receipt: &NativeCloseReceiptWire,
    ) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        self.count_frame()?;
        if self.receipt_frames_observed >= MAX_RECEIPTS_PER_ATTEMPT {
            return Err(self.latch_error(NativeCloseProtocolError::DuplicateReceipt));
        }
        self.receipt_frames_observed = self.receipt_frames_observed.saturating_add(1);

        if let Some(ref existing) = self.primary_error {
            return Err(existing.clone());
        }
        if self.receiver_physical_eof_observed {
            return Err(self.latch_error(NativeCloseProtocolError::ReceiptAfterPhysicalEof));
        }

        match self.stage {
            ReceiverProtocolStage::BusinessExitObservedAwaitingReceipt => {}
            ReceiverProtocolStage::AwaitingHandshakeHello
            | ReceiverProtocolStage::AwaitingHandshakeAck => {
                return Err(self.latch_error(NativeCloseProtocolError::MissingHandshake));
            }
            ReceiverProtocolStage::HandshakeNegotiatedAwaitingSpawnReady
            | ReceiverProtocolStage::ActiveStreamingAwaitingBusinessExit => {
                return Err(self.latch_error(NativeCloseProtocolError::ReceiptBeforeBusinessExit));
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::DuplicateReceipt));
            }
        }

        // 1. Envelope version, schema, attempt ID, sequence number, and private nonce auth_tag correlation.
        if receipt.protocol_version != NATIVE_CLOSE_IPC_PROTOCOL_VERSION {
            return Err(self.latch_error(NativeCloseProtocolError::VersionMismatch {
                expected: NATIVE_CLOSE_IPC_PROTOCOL_VERSION,
                observed: receipt.protocol_version,
            }));
        }
        if receipt.schema != *self.binding.expected_schema().as_bytes() {
            return Err(self.latch_error(NativeCloseProtocolError::SchemaMismatch));
        }
        if receipt.startup_attempt_id == 0 {
            return Err(self.latch_error(NativeCloseProtocolError::InvalidZeroStartupAttemptId));
        }
        if receipt.startup_attempt_id != self.binding.startup_attempt_id().get() {
            return Err(self.latch_error(NativeCloseProtocolError::AttemptIdMismatch {
                expected: self.binding.startup_attempt_id().get(),
                observed: receipt.startup_attempt_id,
            }));
        }
        if receipt.sequence_number != 1 {
            return Err(
                self.latch_error(NativeCloseProtocolError::InvalidReceiptSequenceNumber {
                    observed: receipt.sequence_number,
                }),
            );
        }
        if !receipt.verify_auth_tag(self.binding.session_nonce()) {
            return Err(self.latch_error(NativeCloseProtocolError::NonceMismatch));
        }

        let bound_child = self
            .bound_child_identity
            .ok_or_else(|| self.latch_error(NativeCloseProtocolError::MissingSpawnReady))?;
        let business_exit = self
            .business_exit
            .ok_or_else(|| self.latch_error(NativeCloseProtocolError::ReceiptBeforeBusinessExit))?;

        let facts = &receipt.facts;

        // 2. Domain 7 check first: close debt, poison, first error, and sticky Unknown must be clean.
        match facts.debt_and_sticky_unknown.close_debt {
            CloseDebtFact::ZeroDebtProven => {}
            CloseDebtFact::DebtRetained {
                debt_count,
                first_target,
                first_os_error,
            } => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::ReceiptReportsCloseDebt {
                        debt_count,
                        first_target,
                        first_os_error,
                    }),
                );
            }
            CloseDebtFact::NotObserved | CloseDebtFact::NotProven => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::ReceiptCloseDebtNotProvenZero)
                );
            }
        }

        match facts.debt_and_sticky_unknown.poison_state {
            PoisonStateFact::NoPoisonObserved => {}
            PoisonStateFact::PoisonObserved { target } => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::ReceiptReportsPoison { target })
                );
            }
            PoisonStateFact::NotObserved | PoisonStateFact::NotProven => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::ReceiptPoisonStateNotProvenClean)
                );
            }
        }

        match facts.debt_and_sticky_unknown.first_error {
            FirstErrorFact::NoneObserved => {}
            FirstErrorFact::Recorded {
                kind,
                stage,
                os_error,
                ..
            } => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::ReceiptReportsFirstError {
                        kind,
                        stage,
                        os_error,
                    }),
                );
            }
            FirstErrorFact::NotObserved => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::ReceiptFirstErrorNotProvenNone)
                );
            }
        }

        match facts.debt_and_sticky_unknown.sticky_unknown_state {
            StickyUnknownStateFact::Clear => {}
            StickyUnknownStateFact::LatchedStickyUnknown { .. } => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::ReceiptReportsStickyUnknown)
                );
            }
            StickyUnknownStateFact::NotObserved | StickyUnknownStateFact::NotProven => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::ReceiptStickyUnknownNotProvenClear)
                );
            }
        }

        // 3. Domain 1: Child native identity and raw exit code.
        let (receipt_child_identity, raw_exit_code_dword) = match facts.child_exit {
            ChildNativeExitFact::Witnessed {
                identity,
                raw_exit_code_dword,
                business_exit_code_i32,
                wait_on_original_handle,
                timeout_disposition,
            } => {
                let verified = identity.validate().map_err(|err| self.latch_error(err))?;
                if verified != bound_child {
                    return Err(
                        self.latch_error(NativeCloseProtocolError::ChildIdentityMismatch {
                            expected: bound_child,
                            observed: verified,
                        }),
                    );
                }
                let positive_original_exit_proven = wait_on_original_handle
                    == OriginalProcessWaitProof::SignaledExitedOnOriginalHandle
                    && timeout_disposition == TimeoutDisposition::CompletedWithinBudget;
                if !positive_original_exit_proven {
                    if raw_exit_code_dword == WIN32_STILL_ACTIVE_EXIT_CODE_DWORD {
                        return Err(self
                            .latch_error(NativeCloseProtocolError::ChildExitStillActiveRawCode));
                    }
                    return Err(
                        self.latch_error(NativeCloseProtocolError::ChildNativeExitNotProven)
                    );
                }
                if business_exit_code_i32 != business_exit.exit_code
                    || (raw_exit_code_dword as i32) != business_exit.exit_code
                {
                    return Err(self.latch_error(
                        NativeCloseProtocolError::ChildExitCodeMismatchWithBusinessExit {
                            business_exit_i32: business_exit.exit_code,
                            receipt_business_i32: business_exit_code_i32,
                            raw_exit_dword: raw_exit_code_dword,
                        },
                    ));
                }
                (verified, raw_exit_code_dword)
            }
            ChildNativeExitFact::WaitTimedOutOrIncomplete { identity } => {
                let verified = identity.validate().map_err(|err| self.latch_error(err))?;
                if verified != bound_child {
                    return Err(
                        self.latch_error(NativeCloseProtocolError::ChildIdentityMismatch {
                            expected: bound_child,
                            observed: verified,
                        }),
                    );
                }
                return Err(self.latch_error(NativeCloseProtocolError::ChildNativeExitNotProven));
            }
            ChildNativeExitFact::NotObserved
            | ChildNativeExitFact::NotProven
            | ChildNativeExitFact::NotCreatedPreLaunch => {
                return Err(self.latch_error(NativeCloseProtocolError::ChildNativeExitNotProven));
            }
        };

        // 4. Domain 2: Original stdout, stderr, and input EOF and reader joins.
        let io = &facts.io_streams_and_workers;
        if io.stdout.pipe_eof != PipeEofFact::CleanEofObserved {
            return Err(self.latch_error(NativeCloseProtocolError::IoFactRejected(
                IoFactFailureReason::StdoutEofNotProven,
            )));
        }
        if io.stdout.reader_join != WorkerNativeJoinFact::JoinedTimelyClean {
            return Err(self.latch_error(NativeCloseProtocolError::IoFactRejected(
                IoFactFailureReason::StdoutReaderJoinNotTimelyClean,
            )));
        }
        if io.stdout.handle_close != HandleCloseFact::ClosedVerifiedOnce {
            return Err(self.latch_error(NativeCloseProtocolError::IoFactRejected(
                IoFactFailureReason::StdoutHandleNotClosedVerified,
            )));
        }

        match io.stderr {
            StderrStreamWorkerFact::MergedConptyOrAbsent { handle_close } => {
                if handle_close != HandleCloseFact::NotApplicableAbsent {
                    return Err(self.latch_error(NativeCloseProtocolError::IoFactRejected(
                        IoFactFailureReason::StderrHandleDispositionInvalid,
                    )));
                }
            }
            StderrStreamWorkerFact::SeparateStream { stream } => {
                if stream.pipe_eof != PipeEofFact::CleanEofObserved {
                    return Err(self.latch_error(NativeCloseProtocolError::IoFactRejected(
                        IoFactFailureReason::StderrEofNotProven,
                    )));
                }
                if stream.reader_join != WorkerNativeJoinFact::JoinedTimelyClean {
                    return Err(self.latch_error(NativeCloseProtocolError::IoFactRejected(
                        IoFactFailureReason::StderrReaderJoinNotTimelyClean,
                    )));
                }
                if stream.handle_close != HandleCloseFact::ClosedVerifiedOnce {
                    return Err(self.latch_error(NativeCloseProtocolError::IoFactRejected(
                        IoFactFailureReason::StderrHandleDispositionInvalid,
                    )));
                }
            }
        }

        if io.input.termination_disposition != InputTerminationFact::NoTerminationObserved {
            return Err(self.latch_error(NativeCloseProtocolError::IoFactRejected(
                IoFactFailureReason::InputTerminationInvoked,
            )));
        }
        if io.input.peer_eof != InputPeerEofFact::CleanPostChildExitFrameBoundaryEof {
            return Err(self.latch_error(NativeCloseProtocolError::IoFactRejected(
                IoFactFailureReason::InputPeerEofNotPostExitClean,
            )));
        }
        if io.input.reader_join != WorkerNativeJoinFact::JoinedTimelyClean {
            return Err(self.latch_error(NativeCloseProtocolError::IoFactRejected(
                IoFactFailureReason::InputReaderJoinNotTimelyClean,
            )));
        }

        // 5. Domain 3: Original ConPTY, stdin, and child thread/process handle closure.
        let hc = &facts.handles_close;
        match hc.conpty_closure {
            ConptyClosureFact::NotUsedPipeMode
            | ConptyClosureFact::ClosedBeforeOutputJoinVerified => {}
            ConptyClosureFact::NotObserved
            | ConptyClosureFact::NotProven
            | ConptyClosureFact::StillActiveOrPoisoned => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::HandleCloseFactRejected(
                        HandleCloseFailureReason::ConptyNotClosedBeforeOutputJoin,
                    )),
                );
            }
        }
        match hc.stdin_closure {
            StdinHandleCloseFact::InitiallyNotOpen
            | StdinHandleCloseFact::ClosedOnceVerified {
                write_disposition: StdinWriteDisposition::AllWritesSucceeded,
            } => {}
            _ => {
                return Err(
                    self.latch_error(NativeCloseProtocolError::HandleCloseFactRejected(
                        HandleCloseFailureReason::StdinCloseNotProvenOnceClean,
                    )),
                );
            }
        }
        if hc.child_primary_thread_handle != HandleCloseFact::ClosedVerifiedOnce {
            return Err(
                self.latch_error(NativeCloseProtocolError::HandleCloseFactRejected(
                    HandleCloseFailureReason::ChildPrimaryThreadHandleNotClosedVerified,
                )),
            );
        }
        if hc.child_process_handle != HandleCloseFact::ClosedVerifiedOnce {
            return Err(
                self.latch_error(NativeCloseProtocolError::HandleCloseFactRejected(
                    HandleCloseFailureReason::ChildProcessHandleNotClosedVerified,
                )),
            );
        }
        if hc.close_ordering_after_io_joins != StageOrderingFact::ProvenInStrictOrder {
            return Err(
                self.latch_error(NativeCloseProtocolError::HandleCloseFactRejected(
                    HandleCloseFailureReason::HandleCloseOrderNotAfterIoJoins,
                )),
            );
        }

        // 6. Domain 4: Original Job admission/identity and active-zero drain.
        let job = &facts.job_lifecycle;
        if job.preserve_descendants != JobForbiddenOpFact::NotInvoked {
            return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                JobFactFailureReason::PreserveDescendantsInvoked,
            )));
        }
        if job.terminate_job != JobForbiddenOpFact::NotInvoked {
            return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                JobFactFailureReason::TerminateJobInvoked,
            )));
        }
        let job_instance_id = match job.admission {
            JobAdmissionFact::AdmittedVerifiedOriginalJob {
                job_instance_id,
                child_membership_verified,
            } => {
                if job_instance_id == 0 {
                    return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                        JobFactFailureReason::InvalidZeroJobInstanceId,
                    )));
                }
                if child_membership_verified != ObservationProof::ProvenOnOriginal {
                    return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                        JobFactFailureReason::JobAdmissionNotProvenOnOriginal,
                    )));
                }
                job_instance_id
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                    JobFactFailureReason::JobAdmissionNotProvenOnOriginal,
                )));
            }
        };
        let job_total_processes = match job.active_zero_drain {
            JobActiveZeroDrainFact::DrainedToActiveZero {
                total_processes,
                active_processes,
                total_terminated_processes,
                ordering_after_child_handle_close,
            } => {
                if total_processes == 0 {
                    return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                        JobFactFailureReason::TotalProcessesZero,
                    )));
                }
                if active_processes != 0 {
                    return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                        JobFactFailureReason::ActiveProcessesNonZero,
                    )));
                }
                if total_terminated_processes != 0 {
                    return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                        JobFactFailureReason::TerminatedProcessesNonZero,
                    )));
                }
                if ordering_after_child_handle_close != StageOrderingFact::ProvenInStrictOrder {
                    return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                        JobFactFailureReason::DrainOrderNotAfterChildHandleClose,
                    )));
                }
                if total_processes != 1
                    || job.extra_member_origin
                        != ExtraJobMembersOriginFact::SingleProcessObservedOnly
                {
                    return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                        JobFactFailureReason::ExtraJobMemberOriginNotProven,
                    )));
                }
                total_processes
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::JobFactRejected(
                    JobFactFailureReason::ActiveZeroDrainNotProven,
                )));
            }
        };

        // 7. Domain 5: Original control writer closure and pending sender receipt writer.
        let writers = &facts.writers;
        match writers.business_exit_writer {
            BusinessExitWriterFact::JoinedTimelyBeforeReceipt { exit_code_i32 } => {
                if exit_code_i32 != business_exit.exit_code {
                    return Err(self.latch_error(NativeCloseProtocolError::WriterFactRejected(
                        WriterFactFailureReason::BusinessExitWriterCodeMismatch,
                    )));
                }
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::WriterFactRejected(
                    WriterFactFailureReason::BusinessExitWriterNotJoinedTimely,
                )));
            }
        }
        let control_results_written = match writers.prior_control_and_frame_writers {
            PriorControlWritersFact::AllJoinedAndQuiesced {
                control_results_written,
            } => {
                let expected = self.control_frames_observed as u16;
                if control_results_written != expected {
                    return Err(self.latch_error(NativeCloseProtocolError::WriterFactRejected(
                        WriterFactFailureReason::PriorControlResultsCountMismatch {
                            expected,
                            reported: control_results_written,
                        },
                    )));
                }
                control_results_written
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::WriterFactRejected(
                    WriterFactFailureReason::PriorControlWritersNotQuiesced,
                )));
            }
        };
        match writers.sender_receipt_writer {
            SenderReceiptWriterFact::PendingCurrentReceiptFlushAndClose => {}
            SenderReceiptWriterFact::ClaimedJoinedBeforeSend => {
                return Err(self.latch_error(NativeCloseProtocolError::WriterFactRejected(
                    WriterFactFailureReason::SenderWriterClaimedJoinedBeforeSend,
                )));
            }
            SenderReceiptWriterFact::NotObserved
            | SenderReceiptWriterFact::NotProven
            | SenderReceiptWriterFact::WriteFailed => {
                return Err(self.latch_error(NativeCloseProtocolError::WriterFactRejected(
                    WriterFactFailureReason::SenderWriterNotPendingCurrentFlush,
                )));
            }
        }

        // 8. Domain 6: Runner-owned retirement vs host-owned lease scope boundary.
        //    - Runner's own input reader & child stdin lease must be RetiredCleanly.
        //    - Host-owned input writer, keeper/desktop/ACL, and quota leases are out of runner
        //      scope and MUST be reported as OutOfRunnerScopePendingHostOwner. A runner receipt
        //      claiming RetiredCleanly for any host-owned lease is rejected as a scope violation.
        let leases = &facts.leases;
        if leases.runner_input_and_stdin_lease != LeaseRetirementFact::RetiredCleanly {
            return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                LeaseFactFailureReason::RunnerInputAndStdinLeaseNotRetiredCleanly,
            )));
        }
        match leases.input_lease {
            LeaseRetirementFact::OutOfRunnerScopePendingHostOwner => {}
            LeaseRetirementFact::RetiredCleanly => {
                return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::RunnerClaimedHostInputLeaseRetired,
                )));
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::InputLeaseNotPendingHostOwner,
                )));
            }
        }
        match leases.keeper_lease {
            LeaseRetirementFact::OutOfRunnerScopePendingHostOwner => {}
            LeaseRetirementFact::RetiredCleanly => {
                return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::RunnerClaimedHostKeeperLeaseRetired,
                )));
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::KeeperLeaseNotPendingHostOwner,
                )));
            }
        }
        match leases.quota_lease {
            LeaseRetirementFact::OutOfRunnerScopePendingHostOwner => {}
            LeaseRetirementFact::RetiredCleanly => {
                return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::RunnerClaimedHostQuotaLeaseRetired,
                )));
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::QuotaLeaseNotPendingHostOwner,
                )));
            }
        }
        match leases.attempt_lease {
            AttemptLeaseRetirementFact::ReadyToRetireOnFinalReceiptFlush {
                startup_attempt_id,
            } => {
                let expected = self.binding.startup_attempt_id().get();
                if startup_attempt_id != expected {
                    return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                        LeaseFactFailureReason::AttemptLeaseIdMismatch {
                            expected,
                            reported: startup_attempt_id,
                        },
                    )));
                }
            }
            _ => {
                return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::AttemptLeaseNotReadyToRetire,
                )));
            }
        }

        self.validated_receipt = Some(ValidatedReceiptEvidence {
            startup_attempt_id: self.binding.startup_attempt_id(),
            child_identity: receipt_child_identity,
            raw_child_exit_dword: RawProcessExitCodeDword(raw_exit_code_dword),
            business_exit_i32: BusinessExitCodeI32(business_exit.exit_code),
            job_instance_id,
            job_total_processes,
            control_results_written,
        });
        self.stage = ReceiverProtocolStage::ReceiptAcceptedAwaitingPhysicalEof;
        Ok(self.stage)
    }

    /// Step 6: Records physical pipe EOF on the receiver's output/control reader pipe.
    ///
    /// - If the receiver has not yet received and accepted the single terminal receipt,
    ///   physical pipe EOF is rejected as `MissingReceiptAtPhysicalEof`.
    /// - If the receiver was already in `LatchedNotAcceptedStickyUnknown`, records
    ///   `receiver_physical_eof_observed = true` for physical reaping tracking while
    ///   preserving `primary_error` and never upgrading to clean.
    pub fn record_physical_eof(&mut self) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        if self.receiver_physical_eof_observed {
            return Err(self.latch_error(NativeCloseProtocolError::DuplicatePhysicalEof));
        }
        if let Some(ref existing) = self.primary_error {
            self.receiver_physical_eof_observed = true;
            return Err(existing.clone());
        }
        match self.stage {
            ReceiverProtocolStage::ReceiptAcceptedAwaitingPhysicalEof => {
                self.receiver_physical_eof_observed = true;
                self.stage = ReceiverProtocolStage::PhysicalEofObservedAwaitingReceiverNativeClose;
                Ok(self.stage)
            }
            _ => {
                self.receiver_physical_eof_observed = true;
                Err(self.latch_error(NativeCloseProtocolError::MissingReceiptAtPhysicalEof))
            }
        }
    }

    /// Step 7: Records the receiver's own native worker joins, native runner process
    /// exit/handle closure, and host-owned input/keeper/local-registry lease retirement
    /// after physical pipe EOF.
    ///
    /// If a primary error was previously latched (e.g., receipt had debt, poison, missing
    /// facts, or late logical `Unknown`), this records late physical reap completion if
    /// all physical resources closed, but NEVER clears the primary error or upgrades the
    /// status to clean.
    /// Live owner has proved ONLY its native/IO closure. Unobserved outer
    /// keeper/desktop/quota is pending, never clean, never a local-slot release.
    pub fn record_host_native_close_pending_outer(&mut self,facts:ReceiverPhysicalCloseFacts)
        ->Result<ReceiverProtocolStage,NativeCloseProtocolError> {
        if let Some(error)=self.primary_error.clone() {return Err(error);}
        if !self.receiver_physical_eof_observed || !matches!(self.stage,
            ReceiverProtocolStage::PhysicalEofObservedAwaitingReceiverNativeClose|ReceiverProtocolStage::HostNativeClosedPendingOuterScope) {
            return Err(self.latch_error(NativeCloseProtocolError::ReceiverPhysicalCloseBeforeEof));
        }
        if facts.reader_worker_joined_timely_clean!=ObservationProof::ProvenOnOriginal
            || facts.writer_worker_joined_timely_clean!=ObservationProof::ProvenOnOriginal
            || facts.input_worker_joined_timely_clean!=ObservationProof::ProvenOnOriginal {
            return Err(self.latch_error(NativeCloseProtocolError::ReceiverWorkerJoinFailed));
        }
        if self.bound_runner_identity.is_none() || facts.runner_process_identity!=self.bound_runner_identity
            || !facts.runner_exit_code_dword.is_some_and(|raw|self.business_exit.is_some_and(|business|raw.0==business.exit_code as u32))
            || facts.runner_handle_closed_verified!=ObservationProof::ProvenOnOriginal {
            return Err(self.latch_error(NativeCloseProtocolError::RunnerNativeProcessCloseFailed));
        }
        if facts.host_input_writer_lease!=LeaseRetirementFact::RetiredCleanly {
            return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(LeaseFactFailureReason::HostInputLeaseNotRetiredCleanly)));
        }
        if facts.external_quota_scope!=ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter {
            return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(LeaseFactFailureReason::ExternalQuotaClaimedBySingleAttemptReceipt)));
        }
        self.receiver_physical_close=facts;
        self.stage=ReceiverProtocolStage::HostNativeClosedPendingOuterScope;Ok(self.stage)
    }

    pub fn record_receiver_and_runner_native_close(
        &mut self,
        facts: ReceiverPhysicalCloseFacts,
    ) -> Result<ReceiverProtocolStage, NativeCloseProtocolError> {
        self.receiver_physical_close = facts;
        let workers_clean = facts.reader_worker_joined_timely_clean
            == ObservationProof::ProvenOnOriginal
            && facts.writer_worker_joined_timely_clean == ObservationProof::ProvenOnOriginal
            && facts.input_worker_joined_timely_clean == ObservationProof::ProvenOnOriginal;
        // Observe the original RUNNER independently. Its real full DWORD must
        // match this checked runner's actual propagation contract, never forced0
        // or narrowed to u8. Child identity is separately bound by SpawnReady.
        let runner_clean = facts.runner_process_identity.is_some()
            && self.bound_runner_identity.map_or(true, |expected| facts.runner_process_identity==Some(expected))
            && facts.runner_exit_code_dword.is_some_and(|code|
                self.business_exit.is_some_and(|business| code.0==business.exit_code as u32))
            && facts.runner_handle_closed_verified == ObservationProof::ProvenOnOriginal;
        let host_leases_clean =
            facts.host_input_writer_lease == LeaseRetirementFact::RetiredCleanly
                && facts.host_keeper_and_desktop_lease == LeaseRetirementFact::RetiredCleanly
                && facts.host_local_registry_quota_lease == LeaseRetirementFact::RetiredCleanly
                && facts.external_quota_scope
                    == ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter;

        if let Some(ref existing) = self.primary_error {
            if self.receiver_physical_eof_observed
                && workers_clean
                && runner_clean
                && host_leases_clean
            {
                self.late_physical_reap_observed = true;
            }
            return Err(existing.clone());
        }

        if !matches!(self.stage, ReceiverProtocolStage::PhysicalEofObservedAwaitingReceiverNativeClose
            |ReceiverProtocolStage::HostNativeClosedPendingOuterScope) || !self.receiver_physical_eof_observed
        {
            return Err(self.latch_error(NativeCloseProtocolError::ReceiverPhysicalCloseBeforeEof));
        }
        if !workers_clean {
            return Err(self.latch_error(NativeCloseProtocolError::ReceiverWorkerJoinFailed));
        }
        if !runner_clean {
            return Err(
                self.latch_error(NativeCloseProtocolError::RunnerNativeProcessCloseFailed)
            );
        }
        if facts.host_input_writer_lease != LeaseRetirementFact::RetiredCleanly {
            return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                LeaseFactFailureReason::HostInputLeaseNotRetiredCleanly,
            )));
        }
        if facts.host_keeper_and_desktop_lease != LeaseRetirementFact::RetiredCleanly {
            return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                LeaseFactFailureReason::HostKeeperLeaseNotRetiredCleanly,
            )));
        }
        if facts.host_local_registry_quota_lease != LeaseRetirementFact::RetiredCleanly {
            return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                LeaseFactFailureReason::HostLocalRegistrySlotNotRetiredCleanly,
            )));
        }
        if facts.external_quota_scope
            != ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter
        {
            return Err(self.latch_error(NativeCloseProtocolError::LeaseFactRejected(
                LeaseFactFailureReason::ExternalQuotaClaimedBySingleAttemptReceipt,
            )));
        }

        self.stage = ReceiverProtocolStage::RunnerAttemptTransportClosed;
        Ok(self.stage)
    }

    /// Records an external sticky `Unknown` failure from the host owner (such as caller
    /// await cancellation, deadline expiry, or owner mutex poison), preserving any
    /// earlier primary error and preventing any subsequent upgrade to clean.
    pub fn record_external_sticky_unknown(
        &mut self,
        kind: ProtocolStickyFailureKind,
    ) -> NativeCloseProtocolError {
        self.latch_error(NativeCloseProtocolError::ExternalStickyUnknownLatched(kind))
    }

    /// Computes the current sender writer tracking status from the receiver's vantage point.
    pub const fn sender_writer_tracking(&self) -> SenderWriterReceiverTracking {
        if self.primary_error.is_some() {
            SenderWriterReceiverTracking::RejectedOrUnknown
        } else if self.receiver_physical_eof_observed
            && self.validated_receipt.is_some()
        {
            SenderWriterReceiverTracking::PipePhysicalEofObserved
        } else if self.validated_receipt.is_some() {
            SenderWriterReceiverTracking::SenderWriterPendingPipeFlushAndEof
        } else {
            SenderWriterReceiverTracking::ReceiptNotYetAccepted
        }
    }

    /// Computes the host-owner completion phase gate from the receiver's current state.
    pub const fn host_completion_gate(&self) -> HostOwnerCompletionGate {
        if self.primary_error.is_some() {
            HostOwnerCompletionGate::RejectedOrStickyUnknown
        } else {
            match self.stage {
                ReceiverProtocolStage::RunnerAttemptTransportClosed => {
                    HostOwnerCompletionGate::HostPhysicalCloseAndLocalSlotVerifiedExternalQuotaPendingAdapter
                }
                ReceiverProtocolStage::PhysicalEofObservedAwaitingReceiverNativeClose
                | ReceiverProtocolStage::HostNativeClosedPendingOuterScope => {
                    HostOwnerCompletionGate::PhysicalEofObservedPendingHostWorkersRunnerAndKeepers
                }
                ReceiverProtocolStage::ReceiptAcceptedAwaitingPhysicalEof => {
                    HostOwnerCompletionGate::PendingPhysicalEofAndHostOwnerRetirement
                }
                _ => HostOwnerCompletionGate::ReceiptNotYetAccepted,
            }
        }
    }

    /// Returns whether the host's local `CheckedOwnerRegistry` (16-slot) reservation for this
    /// attempt may be released.
    ///
    /// A decoded runner receipt alone (`ReceiptAcceptedAwaitingPhysicalEof`) or physical pipe
    /// EOF alone (`PhysicalEofObservedAwaitingReceiverNativeClose`) NEVER authorizes slot release.
    pub const fn can_release_host_local_registry_slot(&self) -> bool {
        self.is_attempt_transport_closed_clean()
    }

    /// Returns the actionable typed acceptance status of this receiver state machine.
    pub fn acceptance_status(&self) -> ReceiptAcceptanceStatus {
        if let Some(ref primary_error) = self.primary_error {
            return ReceiptAcceptanceStatus::NotAcceptedStickyUnknown {
                primary_error: primary_error.clone(),
                no_replay_sealed: ReplayDisposition::NoReplaySealed,
                late_physical_eof: if self.receiver_physical_eof_observed {
                    LatePhysicalReapDisposition::ObservedUnderStickyUnknown
                } else {
                    LatePhysicalReapDisposition::NotObserved
                },
                late_physical_reap: if self.late_physical_reap_observed {
                    LatePhysicalReapDisposition::ObservedUnderStickyUnknown
                } else {
                    LatePhysicalReapDisposition::NotObserved
                },
            };
        }
        match self.stage {
            ReceiverProtocolStage::ReceiptAcceptedAwaitingPhysicalEof
            | ReceiverProtocolStage::PhysicalEofObservedAwaitingReceiverNativeClose
            | ReceiverProtocolStage::HostNativeClosedPendingOuterScope => {
                let evidence = self
                    .validated_receipt
                    .expect("validated_receipt present after receipt acceptance");
                ReceiptAcceptanceStatus::ReceiptAcceptedPendingPhysicalClose {
                    startup_attempt_id: evidence.startup_attempt_id,
                    child_identity: evidence.child_identity,
                    raw_child_exit_dword: evidence.raw_child_exit_dword,
                    business_exit_i32: evidence.business_exit_i32,
                    sender_writer_tracking: self.sender_writer_tracking(),
                    host_completion_gate: self.host_completion_gate(),
                }
            }
            ReceiverProtocolStage::RunnerAttemptTransportClosed => {
                let evidence = self
                    .validated_receipt
                    .expect("validated_receipt present in RunnerAttemptTransportClosed");
                let runner_exit_dword = self
                    .receiver_physical_close
                    .runner_exit_code_dword
                    .expect("runner_exit_code_dword present in RunnerAttemptTransportClosed");
                ReceiptAcceptanceStatus::AttemptTransportClosedEvidenceValidated {
                    startup_attempt_id: evidence.startup_attempt_id,
                    child_identity: evidence.child_identity,
                    raw_child_exit_dword: evidence.raw_child_exit_dword,
                    business_exit_i32: evidence.business_exit_i32,
                    job_total_processes: evidence.job_total_processes,
                    runner_exit_dword,
                    host_completion_gate: self.host_completion_gate(),
                    external_quota_scope: self.receiver_physical_close.external_quota_scope,
                    product_scope:
                        ProductCloseScopeDisposition::SingleAttemptEvidenceOnlyNotProductGlobalClose,
                }
            }
            other_stage => ReceiptAcceptanceStatus::InProgress { stage: other_stage },
        }
    }

    /// Produces a secret-free public projection suitable for observation mirrors and handoffs.
    pub fn public_projection(&self) -> NativeClosePublicProjection {
        NativeClosePublicProjection {
            protocol_version: NATIVE_CLOSE_IPC_PROTOCOL_VERSION,
            startup_attempt_id: self.binding.startup_attempt_id(),
            stage: self.stage,
            bound_child_identity: self.bound_child_identity,
            business_exit_code: self
                .business_exit
                .map(|exit| BusinessExitCodeI32(exit.exit_code)),
            validated_receipt: self.validated_receipt,
            sender_writer_tracking: self.sender_writer_tracking(),
            receiver_physical_eof: if self.receiver_physical_eof_observed {
                ObservationProof::ProvenOnOriginal
            } else {
                ObservationProof::NotObserved
            },
            host_completion_gate: self.host_completion_gate(),
            local_registry_slot_releasable: self.can_release_host_local_registry_slot(),
            external_quota_scope: self.receiver_physical_close.external_quota_scope,
            acceptance: self.acceptance_status(),
            primary_error: self.primary_error.clone(),
            replay_disposition: self
                .primary_error
                .as_ref()
                .map(|_| ReplayDisposition::NoReplaySealed),
            product_scope:
                ProductCloseScopeDisposition::SingleAttemptEvidenceOnlyNotProductGlobalClose,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const TEST_NONCE_BYTES: [u8; 32] = [
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x10, 0x21, 0x32, 0x43, 0x54, 0x65, 0x76, 0x87, 0x98, 0xa9, 0xba, 0xcb, 0xdc, 0xed, 0xfe,
        0x0f, 0x1e,
    ];

    fn sample_binding() -> StartupAttemptBinding {
        let attempt_id = StartupAttemptId::try_new(901).expect("non-zero attempt id");
        let nonce = SessionNonce::try_new(TEST_NONCE_BYTES).expect("non-zero nonce");
        StartupAttemptBinding::try_new(
            attempt_id,
            nonce,
            SchemaDigest::v8_native_close(),
            NativeCloseCapabilityOptIn::ExplicitlyOptedInV8,
        )
        .expect("valid v8 binding")
    }

    fn sample_child_identity() -> VerifiedChildProcessIdentity {
        VerifiedChildProcessIdentity::try_new(4242, 4243, 133_700_000_000_000_000)
            .expect("valid non-zero child identity")
    }

    fn sample_runner_identity() -> VerifiedChildProcessIdentity {
        VerifiedChildProcessIdentity::try_new(3100, 3101, 133_699_999_000_000_000)
            .expect("valid non-zero runner identity")
    }

    fn sample_clean_receiver_physical_close() -> ReceiverPhysicalCloseFacts {
        ReceiverPhysicalCloseFacts {
            reader_worker_joined_timely_clean: ObservationProof::ProvenOnOriginal,
            writer_worker_joined_timely_clean: ObservationProof::ProvenOnOriginal,
            input_worker_joined_timely_clean: ObservationProof::ProvenOnOriginal,
            runner_process_identity: Some(sample_runner_identity()),
            runner_exit_code_dword: Some(RawProcessExitCodeDword(0)),
            runner_handle_closed_verified: ObservationProof::ProvenOnOriginal,
            host_input_writer_lease: LeaseRetirementFact::RetiredCleanly,
            host_keeper_and_desktop_lease: LeaseRetirementFact::RetiredCleanly,
            host_local_registry_quota_lease: LeaseRetirementFact::RetiredCleanly,
            external_quota_scope:
                ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter,
        }
    }

    pub(crate) fn sample_clean_facts(
        attempt_id: u64,
        child: VerifiedChildProcessIdentity,
        exit_code: i32,
        control_results_written: u16,
    ) -> ReportedNativeCloseFacts {
        ReportedNativeCloseFacts {
            child_exit: ChildNativeExitFact::Witnessed {
                identity: child.to_wire(),
                raw_exit_code_dword: exit_code as u32,
                business_exit_code_i32: exit_code,
                wait_on_original_handle: OriginalProcessWaitProof::SignaledExitedOnOriginalHandle,
                timeout_disposition: TimeoutDisposition::CompletedWithinBudget,
            },
            io_streams_and_workers: IoStreamsAndWorkersFacts {
                stdout: OutputStreamWorkerFact {
                    pipe_eof: PipeEofFact::CleanEofObserved,
                    reader_join: WorkerNativeJoinFact::JoinedTimelyClean,
                    handle_close: HandleCloseFact::ClosedVerifiedOnce,
                },
                stderr: StderrStreamWorkerFact::SeparateStream {
                    stream: OutputStreamWorkerFact {
                        pipe_eof: PipeEofFact::CleanEofObserved,
                        reader_join: WorkerNativeJoinFact::JoinedTimelyClean,
                        handle_close: HandleCloseFact::ClosedVerifiedOnce,
                    },
                },
                input: InputStreamWorkerFact {
                    peer_eof: InputPeerEofFact::CleanPostChildExitFrameBoundaryEof,
                    reader_join: WorkerNativeJoinFact::JoinedTimelyClean,
                    termination_disposition: InputTerminationFact::NoTerminationObserved,
                },
            },
            handles_close: OriginalHandlesCloseFacts {
                conpty_closure: ConptyClosureFact::ClosedBeforeOutputJoinVerified,
                stdin_closure: StdinHandleCloseFact::ClosedOnceVerified {
                    write_disposition: StdinWriteDisposition::AllWritesSucceeded,
                },
                child_primary_thread_handle: HandleCloseFact::ClosedVerifiedOnce,
                child_process_handle: HandleCloseFact::ClosedVerifiedOnce,
                close_ordering_after_io_joins: StageOrderingFact::ProvenInStrictOrder,
            },
            job_lifecycle: OriginalJobLifecycleFacts {
                admission: JobAdmissionFact::AdmittedVerifiedOriginalJob {
                    job_instance_id: 77_001,
                    child_membership_verified: ObservationProof::ProvenOnOriginal,
                },
                active_zero_drain: JobActiveZeroDrainFact::DrainedToActiveZero {
                    total_processes: 1,
                    active_processes: 0,
                    total_terminated_processes: 0,
                    ordering_after_child_handle_close: StageOrderingFact::ProvenInStrictOrder,
                },
                extra_member_origin: ExtraJobMembersOriginFact::SingleProcessObservedOnly,
                preserve_descendants: JobForbiddenOpFact::NotInvoked,
                terminate_job: JobForbiddenOpFact::NotInvoked,
            },
            writers: WriterClosureFacts {
                business_exit_writer: BusinessExitWriterFact::JoinedTimelyBeforeReceipt {
                    exit_code_i32: exit_code,
                },
                prior_control_and_frame_writers: PriorControlWritersFact::AllJoinedAndQuiesced {
                    control_results_written,
                },
                sender_receipt_writer: SenderReceiptWriterFact::PendingCurrentReceiptFlushAndClose,
            },
            leases: LeaseRetirementFacts {
                runner_input_and_stdin_lease: LeaseRetirementFact::RetiredCleanly,
                input_lease: LeaseRetirementFact::OutOfRunnerScopePendingHostOwner,
                keeper_lease: LeaseRetirementFact::OutOfRunnerScopePendingHostOwner,
                quota_lease: LeaseRetirementFact::OutOfRunnerScopePendingHostOwner,
                attempt_lease: AttemptLeaseRetirementFact::ReadyToRetireOnFinalReceiptFlush {
                    startup_attempt_id: attempt_id,
                },
            },
            debt_and_sticky_unknown: DebtAndStickyUnknownFacts {
                close_debt: CloseDebtFact::ZeroDebtProven,
                first_error: FirstErrorFact::NoneObserved,
                poison_state: PoisonStateFact::NoPoisonObserved,
                sticky_unknown_state: StickyUnknownStateFact::Clear,
            },
        }
    }

    fn advance_receiver_to_business_exit(
        receiver: &mut NativeCloseReceiverState,
        binding: &StartupAttemptBinding,
        child: VerifiedChildProcessIdentity,
        exit_code: i32,
    ) {
        let hello = Version8WireFrame::new(Version8Message::HandshakeHello(
            NativeCloseHandshakeHelloWire::new_signed(binding),
        ));
        let ack = Version8WireFrame::new(Version8Message::HandshakeAck(
            NativeCloseHandshakeAckWire::new_signed(binding),
        ));
        let ready = Version8WireFrame::new(Version8Message::SpawnReady(
            NativeCloseSpawnReadyWire::new_signed(binding, child),
        ));
        let out = Version8WireFrame::new(Version8Message::Output(NativeCloseOutputFrameWire {
            stream: NativeCloseOutputStream::Stdout,
            byte_len: 12,
        }));
        let exit = Version8WireFrame::new(Version8Message::BusinessExit(
            NativeCloseBusinessExitWire {
                exit_code,
                timeout_disposition: TimeoutDisposition::CompletedWithinBudget,
            },
        ));

        for frame in [hello, ack, ready, out, exit] {
            let bytes = encode_v8_length_prefixed_frame(&frame).expect("encode frame");
            receiver
                .apply_length_prefixed_frame_bytes(&bytes)
                .expect("valid pre-receipt step");
        }
        assert_eq!(
            receiver.stage(),
            ReceiverProtocolStage::BusinessExitObservedAwaitingReceipt
        );
    }

    #[test]
    fn bounded_roundtrip_and_ordered_lifecycle_progression() {
        let binding = sample_binding();
        let child = sample_child_identity();
        let mut receiver = NativeCloseReceiverState::new(binding.clone());

        advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
        // Business Exit alone is NOT native child exit or receipt acceptance.
        assert_eq!(receiver.validated_receipt(), None);
        assert_eq!(
            receiver.sender_writer_tracking(),
            SenderWriterReceiverTracking::ReceiptNotYetAccepted
        );
        assert_eq!(
            receiver.host_completion_gate(),
            HostOwnerCompletionGate::ReceiptNotYetAccepted
        );
        assert!(!receiver.can_release_host_local_registry_slot());

        let facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
        let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
        let wire_frame =
            Version8WireFrame::new(Version8Message::NativeCloseReceipt(Box::new(receipt)));
        let encoded = encode_v8_length_prefixed_frame(&wire_frame).expect("encode v8 receipt");
        assert!(encoded.len() <= 4 + MAX_NATIVE_CLOSE_FRAME_BYTES);

        let decoded =
            decode_v8_length_prefixed_frame_exact(&encoded).expect("decode exact v8 receipt");
        assert_eq!(decoded, wire_frame);

        let stage = receiver
            .apply_length_prefixed_frame_bytes(&encoded)
            .expect("accept valid terminal receipt");
        assert_eq!(
            stage,
            ReceiverProtocolStage::ReceiptAcceptedAwaitingPhysicalEof
        );
        assert_eq!(
            receiver.sender_writer_tracking(),
            SenderWriterReceiverTracking::SenderWriterPendingPipeFlushAndEof
        );
        assert_eq!(
            receiver.host_completion_gate(),
            HostOwnerCompletionGate::PendingPhysicalEofAndHostOwnerRetirement
        );
        assert!(!receiver.can_release_host_local_registry_slot());
        assert!(!receiver.is_attempt_transport_closed_clean());

        // Next: physical EOF on receiver pipe.
        let stage_eof = receiver
            .record_physical_eof()
            .expect("physical EOF after receipt");
        assert_eq!(
            stage_eof,
            ReceiverProtocolStage::PhysicalEofObservedAwaitingReceiverNativeClose
        );
        assert_eq!(
            receiver.sender_writer_tracking(),
            SenderWriterReceiverTracking::PipePhysicalEofObserved
        );
        assert_eq!(
            receiver.host_completion_gate(),
            HostOwnerCompletionGate::PhysicalEofObservedPendingHostWorkersRunnerAndKeepers
        );
        assert!(!receiver.can_release_host_local_registry_slot());

        // Finally: receiver local workers joined, native runner process closed, and host leases retired.
        let stage_closed = receiver
            .record_receiver_and_runner_native_close(sample_clean_receiver_physical_close())
            .expect("receiver and runner native close");
        assert_eq!(
            stage_closed,
            ReceiverProtocolStage::RunnerAttemptTransportClosed
        );
        assert!(receiver.is_attempt_transport_closed_clean());
        assert!(receiver.can_release_host_local_registry_slot());

        let projection = receiver.public_projection();
        assert_eq!(
            projection.product_scope,
            ProductCloseScopeDisposition::SingleAttemptEvidenceOnlyNotProductGlobalClose
        );
        assert!(projection.local_registry_slot_releasable);
        assert_eq!(
            projection.external_quota_scope,
            ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter
        );
        assert_eq!(
            projection.acceptance,
            ReceiptAcceptanceStatus::AttemptTransportClosedEvidenceValidated {
                startup_attempt_id: binding.startup_attempt_id(),
                child_identity: child,
                raw_child_exit_dword: RawProcessExitCodeDword(0),
                business_exit_i32: BusinessExitCodeI32(0),
                job_total_processes: 1,
                runner_exit_dword: RawProcessExitCodeDword(0),
                host_completion_gate:
                    HostOwnerCompletionGate::HostPhysicalCloseAndLocalSlotVerifiedExternalQuotaPendingAdapter,
                external_quota_scope:
                    ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter,
                product_scope:
                    ProductCloseScopeDisposition::SingleAttemptEvidenceOnlyNotProductGlobalClose,
            }
        );

        // Verify Debug formatting redacts nonce and auth_tag secrets.
        let debug_text = format!("{binding:?} {receiver:?} {decoded:?}");
        assert!(debug_text.contains("REDACTED_NONCE"));
        assert!(debug_text.contains("REDACTED_AUTH_TAG"));
        assert!(!debug_text.contains("17, 34, 51"));
    }

    #[test]
    fn rejects_version_schema_nonce_attempt_and_child_identity_mismatches() {
        let binding = sample_binding();
        let child = sample_child_identity();

        // 1. Version mismatch on receipt.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut receipt = NativeCloseReceiptWire::new_signed(
                &binding,
                sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0),
            );
            receipt.protocol_version = 7;
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(
                err,
                NativeCloseProtocolError::VersionMismatch {
                    expected: 8,
                    observed: 7
                }
            );
        }

        // 2. Schema mismatch on receipt.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut receipt = NativeCloseReceiptWire::new_signed(
                &binding,
                sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0),
            );
            receipt.schema = super::super::ipc_framed::CHECKED_CONTROL_SCHEMA;
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::SchemaMismatch);
        }

        // 3. Attempt ID mismatch on receipt.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut receipt = NativeCloseReceiptWire::new_signed(
                &binding,
                sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0),
            );
            receipt.startup_attempt_id = 902;
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(
                err,
                NativeCloseProtocolError::AttemptIdMismatch {
                    expected: 901,
                    observed: 902
                }
            );
        }

        // 4. Nonce / auth_tag mismatch on receipt.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut receipt = NativeCloseReceiptWire::new_signed(
                &binding,
                sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0),
            );
            let wrong_nonce = SessionNonce::try_new([0x5a; 32]).unwrap();
            receipt.recompute_auth_tag(&wrong_nonce);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::NonceMismatch);
        }

        // 5. Child identity mismatch and zero-identity rejection.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let wrong_child =
                VerifiedChildProcessIdentity::try_new(4242, 4243, 133_700_000_000_000_999)
                    .unwrap();
            let receipt = NativeCloseReceiptWire::new_signed(
                &binding,
                sample_clean_facts(binding.startup_attempt_id().get(), wrong_child, 0, 0),
            );
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(
                err,
                NativeCloseProtocolError::ChildIdentityMismatch {
                    expected: child,
                    observed: wrong_child,
                }
            );
        }

        // 6. Zero PID / TID / creation FILETIME rejected.
        assert_eq!(
            VerifiedChildProcessIdentity::try_new(0, 10, 100).unwrap_err(),
            NativeCloseProtocolError::InvalidZeroChildIdentity
        );
        assert_eq!(
            VerifiedChildProcessIdentity::try_new(10, 0, 100).unwrap_err(),
            NativeCloseProtocolError::InvalidZeroChildIdentity
        );
        assert_eq!(
            VerifiedChildProcessIdentity::try_new(10, 10, 0).unwrap_err(),
            NativeCloseProtocolError::InvalidZeroChildIdentity
        );
    }

    #[test]
    fn rejects_missing_capability_out_of_order_duplicate_and_missing_receipt_at_eof() {
        let binding = sample_binding();
        let child = sample_child_identity();

        // Missing capability opt-in rejected at binding construction and handshake.
        let disabled_err = StartupAttemptBinding::try_new(
            StartupAttemptId::try_new(901).unwrap(),
            SessionNonce::try_new(TEST_NONCE_BYTES).unwrap(),
            SchemaDigest::v8_native_close(),
            NativeCloseCapabilityOptIn::DisabledOrNotRequested,
        )
        .unwrap_err();
        assert_eq!(disabled_err, NativeCloseProtocolError::MissingCapabilityOptIn);

        // Receipt before BusinessExit is rejected.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            receiver
                .record_handshake_hello(&NativeCloseHandshakeHelloWire::new_signed(&binding))
                .unwrap();
            receiver
                .record_handshake_ack(&NativeCloseHandshakeAckWire::new_signed(&binding))
                .unwrap();
            receiver
                .record_spawn_ready(&NativeCloseSpawnReadyWire::new_signed(&binding, child))
                .unwrap();
            let receipt = NativeCloseReceiptWire::new_signed(
                &binding,
                sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0),
            );
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::ReceiptBeforeBusinessExit);
        }

        // Output or ControlResult after BusinessExit is rejected.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let err = receiver
                .record_output_frame(&NativeCloseOutputFrameWire {
                    stream: NativeCloseOutputStream::Stdout,
                    byte_len: 4,
                })
                .unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::OutputAfterBusinessExit);
        }
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let err = receiver
                .record_control_result_frame(&NativeCloseControlResultWire {
                    request_id: 1,
                    status: super::super::ipc_framed::ControlStatus::Applied,
                })
                .unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::ControlAfterBusinessExit);
        }

        // Duplicate receipt is rejected and voids earlier acceptance into sticky Unknown.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let receipt = NativeCloseReceiptWire::new_signed(
                &binding,
                sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0),
            );
            receiver.record_native_close_receipt(&receipt).unwrap();
            let dup_err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(dup_err, NativeCloseProtocolError::DuplicateReceipt);
            assert_eq!(receiver.validated_receipt(), None);
            assert!(receiver.no_replay_sealed());
        }

        // Missing receipt at physical EOF is rejected.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let eof_err = receiver.record_physical_eof().unwrap_err();
            assert_eq!(eof_err, NativeCloseProtocolError::MissingReceiptAtPhysicalEof);
            assert!(receiver.no_replay_sealed());
        }
    }

    #[test]
    fn rejects_unknown_duplicate_trailing_malformed_and_oversize_wire_frames() {
        // 1. Unknown field rejected.
        let unknown_json = br#"{"version":8,"message":{"type":"business_exit","payload":{"exit_code":0,"timeout_disposition":"completed_within_budget","extra_unknown":1}}}"#;
        assert!(matches!(
            decode_v8_json_payload_exact(unknown_json),
            Err(NativeCloseCodecError::UnknownField(_))
        ));

        // 2. Duplicate field rejected.
        let duplicate_json = br#"{"version":8,"version":8,"message":{"type":"business_exit","payload":{"exit_code":0,"timeout_disposition":"completed_within_budget"}}}"#;
        assert!(matches!(
            decode_v8_json_payload_exact(duplicate_json),
            Err(NativeCloseCodecError::DuplicateField(_))
        ));

        // 3. Trailing bytes in payload or after length-prefixed frame rejected.
        let valid_frame = Version8WireFrame::new(Version8Message::BusinessExit(
            NativeCloseBusinessExitWire {
                exit_code: 0,
                timeout_disposition: TimeoutDisposition::CompletedWithinBudget,
            },
        ));
        let mut framed = encode_v8_length_prefixed_frame(&valid_frame).unwrap();
        framed.push(b'\n');
        assert_eq!(
            decode_v8_length_prefixed_frame_exact(&framed),
            Err(NativeCloseCodecError::TrailingBytesAfterFrame { trailing_bytes: 1 })
        );

        let mut payload_with_trailing = serde_json::to_vec(&valid_frame).unwrap();
        payload_with_trailing.push(b' ');
        assert_eq!(
            decode_v8_json_payload_exact(&payload_with_trailing),
            Err(NativeCloseCodecError::TrailingBytesInPayload)
        );

        // 4. Oversize frame rejected.
        let oversize_len = (MAX_NATIVE_CLOSE_FRAME_BYTES as u32) + 1;
        let mut oversize_framed = Vec::from(oversize_len.to_le_bytes());
        oversize_framed.extend_from_slice(b"{}");
        assert_eq!(
            decode_v8_length_prefixed_frame_exact(&oversize_framed),
            Err(NativeCloseCodecError::FrameTooLarge {
                len: MAX_NATIVE_CLOSE_FRAME_BYTES + 1,
                max: MAX_NATIVE_CLOSE_FRAME_BYTES,
            })
        );

        // 5. Partial header and partial payload rejected.
        assert_eq!(
            decode_v8_length_prefixed_frame_exact(&[1, 2]),
            Err(NativeCloseCodecError::PartialHeader { bytes_read: 2 })
        );
        assert_eq!(
            decode_v8_length_prefixed_frame_exact(&[10, 0, 0, 0, b'{', b'}']),
            Err(NativeCloseCodecError::PartialPayload {
                expected: 10,
                available: 2
            })
        );
    }

    #[test]
    fn rejects_incomplete_io_job_leases_sender_writer_joined_claim_and_preserves_sticky_error() {
        let binding = sample_binding();
        let child = sample_child_identity();

        // 1. Sender receipt writer claiming joined BEFORE sending is rejected.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.writers.sender_receipt_writer = SenderReceiptWriterFact::ClaimedJoinedBeforeSend;
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(
                err,
                NativeCloseProtocolError::WriterFactRejected(
                    WriterFactFailureReason::SenderWriterClaimedJoinedBeforeSend
                )
            );
        }

        // 2. Incomplete IO (e.g. input reader join NotProven) is rejected.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.io_streams_and_workers.input.reader_join = WorkerNativeJoinFact::NotProven;
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(
                err,
                NativeCloseProtocolError::IoFactRejected(
                    IoFactFailureReason::InputReaderJoinNotTimelyClean
                )
            );
        }

        // 3. Incomplete Job drain (active_processes > 0 or preserve_descendants invoked) is rejected.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.job_lifecycle.active_zero_drain = JobActiveZeroDrainFact::DrainedToActiveZero {
                total_processes: 2,
                active_processes: 1,
                total_terminated_processes: 0,
                ordering_after_child_handle_close: StageOrderingFact::ProvenInStrictOrder,
            };
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(
                err,
                NativeCloseProtocolError::JobFactRejected(
                    JobFactFailureReason::ActiveProcessesNonZero
                )
            );
        }

        // 4. Unretired runner lease or invalid host lease state in receipt is rejected.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.leases.keeper_lease = LeaseRetirementFact::ActiveOrLoaned;
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(
                err,
                NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::KeeperLeaseNotPendingHostOwner
                )
            );
        }

        // 5. Poison and sticky Unknown / NotObserved facts are rejected.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.debt_and_sticky_unknown.poison_state = PoisonStateFact::PoisonObserved {
                target: PoisonTarget::ChildProcessOwnerMutex,
            };
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(
                err,
                NativeCloseProtocolError::ReceiptReportsPoison {
                    target: PoisonTarget::ChildProcessOwnerMutex
                }
            );
        }
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.debt_and_sticky_unknown.sticky_unknown_state =
                StickyUnknownStateFact::LatchedStickyUnknown {
                    replay_policy: ReplayDisposition::NoReplaySealed,
                };
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::ReceiptReportsStickyUnknown);
        }

        // 6. Receipt with close debt / poison / sticky Unknown remains NOT_ACCEPTED even after
        //    later physical EOF and receiver/runner physical cleanup complete; primary error preserved.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.debt_and_sticky_unknown.close_debt = CloseDebtFact::DebtRetained {
                debt_count: 1,
                first_target: CloseDebtTarget::ChildStdinHandle,
                first_os_error: Some(6),
            };
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let expected_primary = NativeCloseProtocolError::ReceiptReportsCloseDebt {
                debt_count: 1,
                first_target: CloseDebtTarget::ChildStdinHandle,
                first_os_error: Some(6),
            };
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, expected_primary);

            // Subsequent external sticky Unknown, physical EOF, and full physical cleanup
            // cannot upgrade to clean or overwrite primary_error.
            let ext_err = receiver
                .record_external_sticky_unknown(ProtocolStickyFailureKind::OwnerMutexPoisoned);
            assert_eq!(ext_err, expected_primary);
            let eof_err = receiver.record_physical_eof().unwrap_err();
            assert_eq!(eof_err, expected_primary);
            let close_err = receiver
                .record_receiver_and_runner_native_close(sample_clean_receiver_physical_close())
                .unwrap_err();
            assert_eq!(close_err, expected_primary);
            assert!(!receiver.is_attempt_transport_closed_clean());
            assert!(!receiver.can_release_host_local_registry_slot());
            assert_eq!(
                receiver.acceptance_status(),
                ReceiptAcceptanceStatus::NotAcceptedStickyUnknown {
                    primary_error: expected_primary,
                    no_replay_sealed: ReplayDisposition::NoReplaySealed,
                    late_physical_eof: LatePhysicalReapDisposition::ObservedUnderStickyUnknown,
                    late_physical_reap: LatePhysicalReapDisposition::ObservedUnderStickyUnknown,
                }
            );
        }

        // 7. Missing/NotObserved child exit fact or un-disambiguated STILL_ACTIVE (query-only) raw exit code is rejected.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.child_exit = ChildNativeExitFact::NotObserved;
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::ChildNativeExitNotProven);
        }
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(
                &mut receiver,
                &binding,
                child,
                WIN32_STILL_ACTIVE_EXIT_CODE_DWORD as i32,
            );
            let mut facts = sample_clean_facts(
                binding.startup_attempt_id().get(),
                child,
                WIN32_STILL_ACTIVE_EXIT_CODE_DWORD as i32,
                0,
            );
            facts.child_exit = ChildNativeExitFact::Witnessed {
                identity: child.to_wire(),
                raw_exit_code_dword: WIN32_STILL_ACTIVE_EXIT_CODE_DWORD,
                business_exit_code_i32: WIN32_STILL_ACTIVE_EXIT_CODE_DWORD as i32,
                wait_on_original_handle: OriginalProcessWaitProof::QueryOnlyWithoutWaitSignal,
                timeout_disposition: TimeoutDisposition::CompletedWithinBudget,
            };
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::ChildExitStillActiveRawCode);
        }
    }

    #[test]
    fn enforces_runner_vs_host_lease_scope_split_and_extra_job_member_origin_gate() {
        let binding = sample_binding();
        let child = sample_child_identity();

        // 1. Runner receipt claiming host-owned input_lease, keeper_lease, or quota_lease
        //    is already RetiredCleanly must be rejected as a scope violation.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.leases.input_lease = LeaseRetirementFact::RetiredCleanly;
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            assert_eq!(
                receiver.record_native_close_receipt(&receipt).unwrap_err(),
                NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::RunnerClaimedHostInputLeaseRetired
                )
            );
        }
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.leases.keeper_lease = LeaseRetirementFact::RetiredCleanly;
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            assert_eq!(
                receiver.record_native_close_receipt(&receipt).unwrap_err(),
                NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::RunnerClaimedHostKeeperLeaseRetired
                )
            );
        }
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.leases.quota_lease = LeaseRetirementFact::RetiredCleanly;
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            assert_eq!(
                receiver.record_native_close_receipt(&receipt).unwrap_err(),
                NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::RunnerClaimedHostQuotaLeaseRetired
                )
            );
        }

        // 2. Unretired runner_input_and_stdin_lease is rejected.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.leases.runner_input_and_stdin_lease = LeaseRetirementFact::ActiveOrLoaned;
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            assert_eq!(
                receiver.record_native_close_receipt(&receipt).unwrap_err(),
                NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::RunnerInputAndStdinLeaseNotRetiredCleanly
                )
            );
        }

        // 3. Extra Job members (total_processes > 1 or ExtraMembersObservedOriginNotProvenByJobHelper)
        //    cannot have their origin proven by aggregate Job accounting counters and must fail closed.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let mut facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0);
            facts.job_lifecycle.active_zero_drain = JobActiveZeroDrainFact::DrainedToActiveZero {
                total_processes: 2,
                active_processes: 0,
                total_terminated_processes: 0,
                ordering_after_child_handle_close: StageOrderingFact::ProvenInStrictOrder,
            };
            facts.job_lifecycle.extra_member_origin =
                ExtraJobMembersOriginFact::ExtraMembersObservedOriginNotProvenByJobHelper;
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            assert_eq!(
                receiver.record_native_close_receipt(&receipt).unwrap_err(),
                NativeCloseProtocolError::JobFactRejected(
                    JobFactFailureReason::ExtraJobMemberOriginNotProven
                )
            );
        }

        // 4. Host completion gate requires host input/keeper/local-registry retirement and
        //    forbids claiming global external quota authority from a single attempt.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let receipt = NativeCloseReceiptWire::new_signed(
                &binding,
                sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0),
            );
            receiver.record_native_close_receipt(&receipt).unwrap();
            assert!(!receiver.can_release_host_local_registry_slot());
            receiver.record_physical_eof().unwrap();
            assert!(!receiver.can_release_host_local_registry_slot());

            let mut bad_host_close = sample_clean_receiver_physical_close();
            bad_host_close.host_keeper_and_desktop_lease = LeaseRetirementFact::ActiveOrLoaned;
            assert_eq!(
                receiver
                    .record_receiver_and_runner_native_close(bad_host_close)
                    .unwrap_err(),
                NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::HostKeeperLeaseNotRetiredCleanly
                )
            );
            assert!(!receiver.can_release_host_local_registry_slot());
        }
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, 0);
            let receipt = NativeCloseReceiptWire::new_signed(
                &binding,
                sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0),
            );
            receiver.record_native_close_receipt(&receipt).unwrap();
            receiver.record_physical_eof().unwrap();

            let mut bad_quota_close = sample_clean_receiver_physical_close();
            bad_quota_close.external_quota_scope =
                ExternalQuotaScopeDisposition::InvalidClaimedGlobalQuotaOrFactoryAuthority;
            assert_eq!(
                receiver
                    .record_receiver_and_runner_native_close(bad_quota_close)
                    .unwrap_err(),
                NativeCloseProtocolError::LeaseFactRejected(
                    LeaseFactFailureReason::ExternalQuotaClaimedBySingleAttemptReceipt
                )
            );
            assert!(!receiver.can_release_host_local_registry_slot());
        }

        // 5. Bounded stream read/write helpers roundtrip cleanly and reject truncated/oversize streams.
        {
            let receipt = NativeCloseReceiptWire::new_signed(
                &binding,
                sample_clean_facts(binding.startup_attempt_id().get(), child, 0, 0),
            );
            let frame =
                Version8WireFrame::new(Version8Message::NativeCloseReceipt(Box::new(receipt)));
            let mut stream_buf = Vec::new();
            let written = write_v8_wire_frame_bounded(&mut stream_buf, &frame)
                .expect("write bounded v8 frame");
            assert_eq!(written, stream_buf.len());

            let mut reader = std::io::Cursor::new(&stream_buf[..]);
            let decoded = read_v8_wire_frame_bounded(&mut reader)
                .expect("read bounded v8 frame")
                .expect("frame present");
            assert_eq!(decoded, frame);
            assert_eq!(
                read_v8_wire_frame_bounded(&mut reader).expect("clean EOF after frame"),
                None
            );
        }
    }

    #[test]
    fn existing_v6_and_v7_protocol_constants_and_frames_remain_unchanged() {
        use super::super::ipc_framed::{
            CHECKED_CONTROL_SCHEMA, CHECKED_IPC_PROTOCOL_VERSION, ControlHello, ExitPayload,
            FramedMessage, IPC_PROTOCOL_VERSION, Message,
        };

        assert_eq!(IPC_PROTOCOL_VERSION, 6);
        assert_eq!(CHECKED_IPC_PROTOCOL_VERSION, 7);
        assert_eq!(NATIVE_CLOSE_IPC_PROTOCOL_VERSION, 8);
        assert_ne!(CHECKED_CONTROL_SCHEMA, NATIVE_CLOSE_PROTOCOL_SCHEMA);

        let v6_exit = FramedMessage {
            version: IPC_PROTOCOL_VERSION,
            message: Message::Exit {
                payload: ExitPayload {
                    exit_code: 0,
                    timed_out: false,
                },
            },
        };
        let v6_json = serde_json::to_string(&v6_exit).expect("serialize v6 exit");
        assert_eq!(
            v6_json,
            r#"{"version":6,"type":"exit","payload":{"exit_code":0,"timed_out":false}}"#
        );

        let v7_hello = FramedMessage {
            version: CHECKED_IPC_PROTOCOL_VERSION,
            message: Message::Hello {
                payload: ControlHello {
                    nonce: [1u8; 32],
                    schema: CHECKED_CONTROL_SCHEMA,
                },
            },
        };
        let v7_val = serde_json::to_value(&v7_hello).expect("serialize v7 hello");
        assert_eq!(v7_val["version"], 7);
        assert_eq!(v7_val["type"], "hello");
    }

    #[test]
    fn preserves_legitimate_259_after_positive_original_exit_proof_and_rejects_unproven_or_timed_out_259() {
        let binding = sample_binding();
        let child = sample_child_identity();
        let raw_259 = WIN32_STILL_ACTIVE_EXIT_CODE_DWORD;
        let biz_259 = WIN32_STILL_ACTIVE_EXIT_CODE_DWORD as i32;

        // 1. Positive case: already-exited process legitimately returning raw exit code 259
        //    after positive original-handle wait proof (`SignaledExitedOnOriginalHandle`)
        //    and `CompletedWithinBudget` on verified original identity is accepted, keeping
        //    original child identity, raw DWORD, and business result distinct.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, biz_259);

            let facts = sample_clean_facts(binding.startup_attempt_id().get(), child, biz_259, 0);
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let wire_frame =
                Version8WireFrame::new(Version8Message::NativeCloseReceipt(Box::new(receipt)));
            let encoded =
                encode_v8_length_prefixed_frame(&wire_frame).expect("encode receipt with 259");

            let stage = receiver
                .apply_length_prefixed_frame_bytes(&encoded)
                .expect("accept legitimate 259 after positive original-handle exit proof");
            assert_eq!(
                stage,
                ReceiverProtocolStage::ReceiptAcceptedAwaitingPhysicalEof
            );

            let validated = receiver
                .validated_receipt()
                .expect("validated receipt present for legitimate 259");
            assert_eq!(validated.child_identity(), child);
            assert_eq!(
                validated.raw_child_exit_dword(),
                RawProcessExitCodeDword(raw_259)
            );
            assert_eq!(
                validated.business_exit_i32(),
                BusinessExitCodeI32(biz_259)
            );

            receiver
                .record_physical_eof()
                .expect("physical EOF after legitimate 259 receipt");
            // Actual checked runner propagates the complete child DWORD; its
            // separately observed native status must not be replaced with0.
            let mut receiver_native=sample_clean_receiver_physical_close();
            receiver_native.runner_exit_code_dword=Some(RawProcessExitCodeDword(raw_259));
            let final_stage = receiver
                .record_receiver_and_runner_native_close(receiver_native)
                .expect("clean physical close after legitimate 259 child exit");
            assert_eq!(
                final_stage,
                ReceiverProtocolStage::RunnerAttemptTransportClosed
            );
            assert!(receiver.is_attempt_transport_closed_clean());
            assert_eq!(receiver.primary_error(), None);
            assert_eq!(
                receiver.acceptance_status(),
                ReceiptAcceptanceStatus::AttemptTransportClosedEvidenceValidated {
                    startup_attempt_id: binding.startup_attempt_id(),
                    child_identity: child,
                    raw_child_exit_dword: RawProcessExitCodeDword(raw_259),
                    business_exit_i32: BusinessExitCodeI32(biz_259),
                    job_total_processes: 1,
                    runner_exit_dword: RawProcessExitCodeDword(raw_259),
                    host_completion_gate:
                        HostOwnerCompletionGate::HostPhysicalCloseAndLocalSlotVerifiedExternalQuotaPendingAdapter,
                    external_quota_scope:
                        ExternalQuotaScopeDisposition::LocalRegistryOnlyExternalQuotaRequiresOuterAdapter,
                    product_scope:
                        ProductCloseScopeDisposition::SingleAttemptEvidenceOnlyNotProductGlobalClose,
                }
            );
        }

        // 2. Valid nonzero known business failure (e.g. exit code 42) with proven physical close
        //    remains distinct from unknown physical close (`NotAcceptedStickyUnknown`).
        {
            let mut clean_nonzero_receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut clean_nonzero_receiver, &binding, child, 42);
            let clean_facts = sample_clean_facts(binding.startup_attempt_id().get(), child, 42, 0);
            let clean_receipt = NativeCloseReceiptWire::new_signed(&binding, clean_facts);
            clean_nonzero_receiver
                .record_native_close_receipt(&clean_receipt)
                .expect("accept nonzero business exit with clean native close proof");
            clean_nonzero_receiver.record_physical_eof().unwrap();
            let mut receiver_native=sample_clean_receiver_physical_close();
            receiver_native.runner_exit_code_dword=Some(RawProcessExitCodeDword(42));
            clean_nonzero_receiver
                .record_receiver_and_runner_native_close(receiver_native)
                .unwrap();
            assert!(clean_nonzero_receiver.is_attempt_transport_closed_clean());
            assert_eq!(
                clean_nonzero_receiver
                    .validated_receipt()
                    .unwrap()
                    .business_exit_i32(),
                BusinessExitCodeI32(42)
            );
            assert_eq!(
                clean_nonzero_receiver
                    .validated_receipt()
                    .unwrap()
                    .raw_child_exit_dword(),
                RawProcessExitCodeDword(42)
            );

            let mut unknown_close_receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut unknown_close_receiver, &binding, child, 42);
            let mut broken_facts =
                sample_clean_facts(binding.startup_attempt_id().get(), child, 42, 0);
            broken_facts.handles_close.child_process_handle = HandleCloseFact::StillOpen;
            let broken_receipt = NativeCloseReceiptWire::new_signed(&binding, broken_facts);
            let err = unknown_close_receiver
                .record_native_close_receipt(&broken_receipt)
                .unwrap_err();
            assert_eq!(
                err,
                NativeCloseProtocolError::HandleCloseFactRejected(
                    HandleCloseFailureReason::ChildProcessHandleNotClosedVerified
                )
            );
            assert!(!unknown_close_receiver.is_attempt_transport_closed_clean());
            assert_eq!(unknown_close_receiver.validated_receipt(), None);
            assert!(matches!(
                unknown_close_receiver.acceptance_status(),
                ReceiptAcceptanceStatus::NotAcceptedStickyUnknown { .. }
            ));
        }

        // 3. Negative 259 cases: query-only 259, absent/unproven original wait, wait timeout,
        //    and unproven/mismatched child identity must all fail.
        for bad_wait_proof in [
            OriginalProcessWaitProof::QueryOnlyWithoutWaitSignal,
            OriginalProcessWaitProof::NotObserved,
            OriginalProcessWaitProof::NotProven,
            OriginalProcessWaitProof::WaitTimedOutOrStillActive,
        ] {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, biz_259);
            let mut facts =
                sample_clean_facts(binding.startup_attempt_id().get(), child, biz_259, 0);
            facts.child_exit = ChildNativeExitFact::Witnessed {
                identity: child.to_wire(),
                raw_exit_code_dword: raw_259,
                business_exit_code_i32: biz_259,
                wait_on_original_handle: bad_wait_proof,
                timeout_disposition: TimeoutDisposition::CompletedWithinBudget,
            };
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::ChildExitStillActiveRawCode);
            assert_eq!(receiver.validated_receipt(), None);
            assert!(receiver.no_replay_sealed());
        }

        // Timed-out wait disposition with raw 259 must fail even if wait proof claims signaled.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, biz_259);
            let mut facts =
                sample_clean_facts(binding.startup_attempt_id().get(), child, biz_259, 0);
            facts.child_exit = ChildNativeExitFact::Witnessed {
                identity: child.to_wire(),
                raw_exit_code_dword: raw_259,
                business_exit_code_i32: biz_259,
                wait_on_original_handle: OriginalProcessWaitProof::SignaledExitedOnOriginalHandle,
                timeout_disposition: TimeoutDisposition::TimedOut,
            };
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::ChildExitStillActiveRawCode);
            assert_eq!(receiver.validated_receipt(), None);
        }

        // WaitTimedOutOrIncomplete variant after business exit 259 must fail.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, biz_259);
            let mut facts =
                sample_clean_facts(binding.startup_attempt_id().get(), child, biz_259, 0);
            facts.child_exit = ChildNativeExitFact::WaitTimedOutOrIncomplete {
                identity: child.to_wire(),
            };
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::ChildNativeExitNotProven);
        }

        // Unproven (zero PID) or mismatched child identity with raw 259 must fail even with signaled wait proof.
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, biz_259);
            let mut facts =
                sample_clean_facts(binding.startup_attempt_id().get(), child, biz_259, 0);
            facts.child_exit = ChildNativeExitFact::Witnessed {
                identity: WireChildIdentity {
                    pid: 0,
                    thread_id: child.thread_id.get(),
                    creation_filetime_100ns: child.creation_filetime_100ns.get(),
                },
                raw_exit_code_dword: raw_259,
                business_exit_code_i32: biz_259,
                wait_on_original_handle: OriginalProcessWaitProof::SignaledExitedOnOriginalHandle,
                timeout_disposition: TimeoutDisposition::CompletedWithinBudget,
            };
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(err, NativeCloseProtocolError::InvalidZeroChildIdentity);
        }
        {
            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            advance_receiver_to_business_exit(&mut receiver, &binding, child, biz_259);
            let wrong_child =
                VerifiedChildProcessIdentity::try_new(9999, 4243, 133_700_000_000_000_000)
                    .unwrap();
            let facts = sample_clean_facts(
                binding.startup_attempt_id().get(),
                wrong_child,
                biz_259,
                0,
            );
            let receipt = NativeCloseReceiptWire::new_signed(&binding, facts);
            let err = receiver.record_native_close_receipt(&receipt).unwrap_err();
            assert_eq!(
                err,
                NativeCloseProtocolError::ChildIdentityMismatch {
                    expected: child,
                    observed: wrong_child,
                }
            );
        }
    }

    #[test]
    fn codec_errors_redact_untrusted_field_variant_duplicate_and_value_markers_in_debug_and_display() {
        const MARKER_UNKNOWN_KEY: &str = "SYNTHETIC_PRIVATE_UNKNOWN_KEY_9F01";
        const MARKER_UNKNOWN_VAL: &str = "SYNTHETIC_PRIVATE_UNKNOWN_VAL_9F02";
        const MARKER_UNKNOWN_VARIANT: &str = "SYNTHETIC_PRIVATE_UNKNOWN_VARIANT_9F03";
        const MARKER_DUP_KEY: &str = "SYNTHETIC_PRIVATE_DUP_KEY_9F04";
        const MARKER_DUP_VAL: &str = "SYNTHETIC_PRIVATE_DUP_VAL_9F05";
        const MARKER_INVALID_TYPE_VAL: &str = "SYNTHETIC_PRIVATE_INVALID_TYPE_VAL_9F06";
        const MARKER_MALFORMED_SYNTAX: &str = "SYNTHETIC_PRIVATE_MALFORMED_SYNTAX_9F07";

        let all_markers = [
            MARKER_UNKNOWN_KEY,
            MARKER_UNKNOWN_VAL,
            MARKER_UNKNOWN_VARIANT,
            MARKER_DUP_KEY,
            MARKER_DUP_VAL,
            MARKER_INVALID_TYPE_VAL,
            MARKER_MALFORMED_SYNTAX,
        ];

        let binding = sample_binding();

        let assert_redacted = |codec_err: NativeCloseCodecError, raw_payload: &[u8]| {
            let mut framed = Vec::with_capacity(4 + raw_payload.len());
            framed.extend_from_slice(&(raw_payload.len() as u32).to_le_bytes());
            framed.extend_from_slice(raw_payload);

            let mut receiver = NativeCloseReceiverState::new(binding.clone());
            let proto_err = receiver
                .apply_length_prefixed_frame_bytes(&framed)
                .expect_err("frame with synthetic error must fail");
            assert_eq!(proto_err, NativeCloseProtocolError::CodecError(codec_err));

            let formatted_outputs = [
                format!("{codec_err:?}"),
                format!("{codec_err:#?}"),
                format!("{codec_err}"),
                format!("{proto_err:?}"),
                format!("{proto_err:#?}"),
                format!("{proto_err}"),
                format!("{receiver:?}"),
                format!("{:?}", receiver.public_projection()),
            ];
            for rendered in &formatted_outputs {
                for marker in &all_markers {
                    assert!(
                        !rendered.contains(marker),
                        "formatted error/state leaked synthetic marker {marker}: {rendered}"
                    );
                }
            }
        };

        // 1. Unknown field key and value containing synthetic private markers.
        let unknown_field_payload = format!(
            r#"{{"version":8,"message":{{"type":"business_exit","payload":{{"exit_code":0,"timeout_disposition":"completed_within_budget","{MARKER_UNKNOWN_KEY}":"{MARKER_UNKNOWN_VAL}"}}}}}}"#
        );
        let err_unknown_field =
            decode_v8_json_payload_exact(unknown_field_payload.as_bytes()).unwrap_err();
        assert!(matches!(
            err_unknown_field,
            NativeCloseCodecError::UnknownField(UnknownSchemaError {
                kind: UnknownSchemaKind::Field,
                line: 1,
                column,
            }) if column > 0
        ));
        assert_redacted(err_unknown_field, unknown_field_payload.as_bytes());

        // 2. Unknown enum variant containing synthetic private marker.
        let unknown_variant_payload = format!(
            r#"{{"version":8,"message":{{"type":"{MARKER_UNKNOWN_VARIANT}","payload":{{"exit_code":0,"timeout_disposition":"completed_within_budget"}}}}}}"#
        );
        let err_unknown_variant =
            decode_v8_json_payload_exact(unknown_variant_payload.as_bytes()).unwrap_err();
        assert!(matches!(
            err_unknown_variant,
            NativeCloseCodecError::UnknownField(UnknownSchemaError {
                kind: UnknownSchemaKind::Variant,
                line: 1,
                column,
            }) if column > 0
        ));
        assert_redacted(err_unknown_variant, unknown_variant_payload.as_bytes());

        // 3. Duplicate field key and value containing synthetic private markers.
        let duplicate_field_payload = format!(
            r#"{{"version":8,"{MARKER_DUP_KEY}":"{MARKER_DUP_VAL}","{MARKER_DUP_KEY}":"{MARKER_DUP_VAL}","message":{{"type":"business_exit","payload":{{"exit_code":0,"timeout_disposition":"completed_within_budget"}}}}}}"#
        );
        let err_duplicate_field =
            decode_v8_json_payload_exact(duplicate_field_payload.as_bytes()).unwrap_err();
        assert!(matches!(
            err_duplicate_field,
            NativeCloseCodecError::DuplicateField(DuplicateFieldError {
                line: 1,
                column,
            }) if column > 0
        ));
        assert_redacted(err_duplicate_field, duplicate_field_payload.as_bytes());

        // 4. Invalid-type payload (string marker where integer exit_code is expected).
        let invalid_type_payload = format!(
            r#"{{"version":8,"message":{{"type":"business_exit","payload":{{"exit_code":"{MARKER_INVALID_TYPE_VAL}","timeout_disposition":"completed_within_budget"}}}}}}"#
        );
        let err_invalid_type =
            decode_v8_json_payload_exact(invalid_type_payload.as_bytes()).unwrap_err();
        assert!(matches!(
            err_invalid_type,
            NativeCloseCodecError::MalformedJson(MalformedJsonError {
                kind: MalformedJsonKind::InvalidType,
                line: 1,
                column,
            }) if column > 0
        ));
        assert_redacted(err_invalid_type, invalid_type_payload.as_bytes());

        // 5. Malformed JSON syntax payload containing synthetic private marker.
        let malformed_syntax_payload =
            format!(r#"{{"version":8,"message":"{MARKER_MALFORMED_SYNTAX}",}}"#);
        let err_malformed_syntax =
            decode_v8_json_payload_exact(malformed_syntax_payload.as_bytes()).unwrap_err();
        assert!(matches!(
            err_malformed_syntax,
            NativeCloseCodecError::MalformedJson(MalformedJsonError {
                kind: MalformedJsonKind::SyntaxError,
                line: 1,
                column,
            }) if column > 0
        ));
        assert_redacted(err_malformed_syntax, malformed_syntax_payload.as_bytes());
    }
}
