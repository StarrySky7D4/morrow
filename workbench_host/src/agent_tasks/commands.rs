//! Bounded, single-delivery commands for the exclusive agent owner.
use super::worker::Control;
use morrow_agent_process_control_v1::{
    Capabilities,
    host::{Budget, Handle},
};
use morrow_agent_session_exec_v1_r2::safe_exec::{ToolObservation, ToolReview};
use morrow_codex_session_exec_windows_v1::ReviewedBorrowedInvocation;
use morrow_plugin_runtime::TaskRun;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentError {
    Invalid,
    Limit,
    Busy,
    Cancelled,
    Unknown,
    Unavailable,
    Maintenance,
    Disconnect,
    Unsupported,
    Session(morrow_agent_session_exec_v1_r2::Error),
}


// One bounded cell per original command, never a log sink or authority input.
// The packed load is coherent; absent observations do not prove zero effects.
#[derive(Default)]
pub(super) struct CommandDiagnostic(AtomicU64);
pub(super) mod diagnostic_stage {
    pub const QUEUED: u8 = 1;
    pub const RECEIVED: u8 = 2;
    pub const COMMAND_STARTED: u8 = 3;
    pub const CHECKPOINT: u8 = 4;
    pub const OWNER_MISMATCH: u8 = 5;
    pub const PREPARE_IO: u8 = 6;
    pub const PORT_START: u8 = 7;
    pub const PORT_REJECTED: u8 = 8;
    pub const PROVIDER_OBSERVED: u8 = 9;
    pub const REGISTER_PROCESS: u8 = 10;
    pub const REGISTER_REJECTED: u8 = 11;
    pub const CONTROL_VETO: u8 = 12;
    pub const RECEIVER_ERROR: u8 = 13;
    pub const RECEIVER_DISCONNECTED: u8 = 14;
    pub const REPLY_SEND_FAILED: u8 = 15;
    pub const RECEIVED_OK: u8 = 16;
    pub const CANCELLED: u8 = 17;
    pub const PORT_ABSENT: u8 = 18;
}
fn diagnostic_name(stage: u8) -> &'static str {
    use diagnostic_stage::*;
    match stage {
        QUEUED => "QUEUED", RECEIVED => "RECEIVED", COMMAND_STARTED => "COMMAND_STARTED",
        CHECKPOINT => "CHECKPOINT", OWNER_MISMATCH => "OWNER_MISMATCH", PREPARE_IO => "PREPARE_IO",
        PORT_START => "PORT_START", PORT_REJECTED => "PORT_REJECTED", PROVIDER_OBSERVED => "PROVIDER_OBSERVED",
        REGISTER_PROCESS => "REGISTER_PROCESS", REGISTER_REJECTED => "REGISTER_REJECTED", CONTROL_VETO => "CONTROL_VETO",
        RECEIVER_ERROR => "RECEIVER_ERROR", RECEIVER_DISCONNECTED => "RECEIVER_DISCONNECTED", REPLY_SEND_FAILED => "REPLY_SEND_FAILED",
        RECEIVED_OK => "RECEIVED_OK", CANCELLED => "CANCELLED", PORT_ABSENT => "PORT_ABSENT", _ => "NOT_OBSERVED",
    }
}
pub(super) fn diagnostic_error(error: AgentError) -> u8 {
    match error {
        AgentError::Invalid => 1, AgentError::Limit => 2, AgentError::Busy => 3,
        AgentError::Cancelled => 4, AgentError::Unknown => 5, AgentError::Unavailable => 6,
        AgentError::Maintenance => 7, AgentError::Disconnect => 8, AgentError::Unsupported => 9,
        AgentError::Session(error) => diagnostic_session_error(error),
    }
}
pub(super) fn diagnostic_session_error(error: morrow_agent_session_exec_v1_r2::Error) -> u8 {
    use morrow_agent_session_exec_v1_r2::Error;
    match error {
        Error::Invalid => 10, Error::Contract => 11, Error::Limit => 12, Error::Correlation => 13,
        Error::Denied => 14, Error::Conflict => 15, Error::NotFound => 16,
        Error::CommitUnknown => 17, Error::Storage => 18,
    }
}
fn diagnostic_error_name(class: u8) -> &'static str {
    match class {
        1=>"Invalid", 2=>"Limit", 3=>"Busy", 4=>"Cancelled", 5=>"Unknown", 6=>"Unavailable",
        7=>"Maintenance", 8=>"Disconnect", 9=>"Unsupported", 10=>"R2Invalid", 11=>"R2Contract",
        12=>"R2Limit", 13=>"R2Correlation", 14=>"R2Denied", 15=>"R2Conflict", 16=>"R2NotFound",
        17=>"R2CommitUnknown", 18=>"R2Storage", 19=>"RegistrationError", _=>"NOT_OBSERVED",
    }
}
impl CommandDiagnostic {
    pub(super) fn worker(&self, stage: u8, class: u8) {
        let _ = self.0.fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
            if old & 0xff00 != 0 { return None; }
            Some((old & !0xffff) | u64::from(stage) | (u64::from(class) << 8))
        });
    }
    pub(super) fn delivery(&self, stage: u8, class: u8) {
        let _ = self.0.fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
            if old & 0xff00_0000 != 0 { return None; }
            Some((old & !0xffff_0000) | (u64::from(stage) << 16) | (u64::from(class) << 24))
        });
    }
    pub(super) fn flag(&self, flag: u64) { self.0.fetch_or(flag, Ordering::AcqRel); }
}
impl std::fmt::Debug for CommandDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = self.0.load(Ordering::Acquire);
        f.debug_struct("CommandDiagnosticV1")
            .field("worker_stage", &diagnostic_name(value as u8))
            .field("worker_error_class", &diagnostic_error_name((value >> 8) as u8))
            .field("delivery_stage", &diagnostic_name((value >> 16) as u8))
            .field("delivery_error_class", &diagnostic_error_name((value >> 24) as u8))
            .field("command_started_observed", &(value & (1 << 32) != 0))
            .field("provider_observed", &(value & (1 << 33) != 0)).finish()
    }
}

/// Host-reviewed execution is a separate command; no guest output approves itself.
pub enum AgentCommand {
    OpenNativeSession,
    NativeSessionWriter { session_id: String },
    NativeSessionExchange {
        endpoint: morrow_agent_session_process_v1_host::native_session::NativeEndpointId,
        canonical: Zeroizing<Vec<u8>>,
    },
    RunFrame(Zeroizing<Vec<u8>>),
    /// Run the one fixed separately approved proposal guest in this context.
    RunSealedProposal {
        request_sha256: [u8; 32],
    },
    /// Native command only; guest imports cannot call the trusted review lane.
    ReviewTool {
        operation_id: String,
    },
    /// Review is data. Approval still needs this exact original live executor.
    ApproveClaim {
        request_id: String,
        operation_id: String,
        proposal_sha256: [u8; 32],
        intent_sha256: [u8; 32],
        expected_record_revision: u64,
        expected_record_sha256: [u8; 32],
    },
    StartClaimed {
        reviewed: Box<ReviewedBorrowedInvocation>,
        claim: [u8; 32],
        capabilities: Capabilities,
        budget: Budget,
    },
}
impl AgentCommand {
    pub(super) fn input_bytes(&self) -> usize {
        match self {
            Self::OpenNativeSession => 0,
            Self::NativeSessionWriter { session_id } => session_id.len(),
            Self::NativeSessionExchange { canonical, .. } => canonical.len(),
            Self::RunFrame(bytes) => bytes.len(),
            Self::RunSealedProposal { .. } => 32,
            Self::ReviewTool { operation_id } => operation_id.len(),
            Self::ApproveClaim {
                request_id,
                operation_id,
                ..
            } => request_id
                .len()
                .saturating_add(operation_id.len())
                .saturating_add(104),
            Self::StartClaimed { .. } => 0,
        }
    }
}
pub enum AgentReply {
    NativeEndpoint(morrow_agent_session_process_v1_host::native_session::NativeEndpointAuthority),
    NativeSession(Zeroizing<Vec<u8>>),
    Frame(TaskRun),
    Started(Handle),
    ToolReview {
        review: ToolReview,
        observation: ToolObservation,
    },
    Claimed {
        observation: ToolObservation,
        claim: [u8; 32],
    },
}
impl std::fmt::Debug for AgentReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NativeEndpoint(endpoint) => f.debug_tuple("NativeEndpoint")
                .field(&endpoint.id()).finish(),
            Self::NativeSession(_) => f.write_str("NativeSession([redacted])"),
            Self::Frame(run) => f
                .debug_tuple("Frame")
                .field(&run.report)
                .finish_non_exhaustive(),
            Self::Started(handle) => f.debug_tuple("Started").field(handle).finish(),
            Self::ToolReview { observation, .. } => f
                .debug_struct("ToolReview")
                .field("identity", &observation.identity)
                .field("record_revision", &observation.record_revision)
                .finish_non_exhaustive(),
            Self::Claimed { observation, .. } => f
                .debug_struct("Claimed")
                .field("identity", &observation.identity)
                .field("record_revision", &observation.record_revision)
                .finish_non_exhaustive(),
        }
    }
}
impl Drop for AgentReply {
    fn drop(&mut self) {
        if let Self::Frame(run) = self
            && let Some(bytes) = &mut run.completion
        {
            bytes.fill(0);
        }
        if let Self::Claimed { claim, .. } = self {
            claim.fill(0);
        }
        if let Self::ToolReview { review, .. } = self {
            super::trusted::erase_review(review);
        }
    }
}
pub(super) struct Command {
    pub diagnostic: Arc<CommandDiagnostic>,
    pub value: AgentCommand,
    pub cancel: Arc<AtomicBool>,
    pub started: Arc<AtomicBool>,
    pub reply: mpsc::SyncSender<Result<AgentReply, AgentError>>,
}
/// Cancel before start has no command effect; cancel after start stops the whole
/// original lease. Missing delivery remains Unknown and is never replayed.
pub struct AgentCommandHandle {
    diagnostic: Arc<CommandDiagnostic>,
    receiver: mpsc::Receiver<Result<AgentReply, AgentError>>,
    cancel: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    control: Arc<Control>,
    consumed: bool,
}
impl AgentCommandHandle {
    /// One genuine receive. Disconnected post-start delivery is Unknown; no retry.
    pub(super) fn wait(mut self) -> Result<AgentReply, AgentError> {
        let result = self.receiver.recv().unwrap_or_else(|_| Err(if self.has_started() {
            AgentError::Unknown
        } else { AgentError::Unavailable }));
        self.consumed = true;
        if result.is_ok() && !self.control.live() {
            self.diagnostic.delivery(diagnostic_stage::CONTROL_VETO, diagnostic_error(AgentError::Unknown));
            return Err(AgentError::Unknown);
        }
        result
    }
    pub(super) fn pair(value: AgentCommand, control: Arc<Control>) -> (Command, Self) {
        let (reply, receiver) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let started = Arc::new(AtomicBool::new(false));
        let diagnostic = Arc::new(CommandDiagnostic::default());
        diagnostic.worker(diagnostic_stage::QUEUED, 0);
        (
            Command {
                diagnostic: diagnostic.clone(),
                value,
                cancel: cancel.clone(),
                started: started.clone(),
                reply,
            },
            Self {
                diagnostic,
                receiver,
                cancel,
                started,
                control,
                consumed: false,
            },
        )
    }
    pub fn has_started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
        if self.has_started() {
            self.control.stop();
        }
    }
    pub fn try_read(&mut self) -> Result<Option<AgentReply>, AgentError> {
        if self.consumed {
            return Err(AgentError::Invalid);
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                self.consumed = true;
                if result.is_ok() && !self.control.live() {
                    self.diagnostic.delivery(diagnostic_stage::CONTROL_VETO, diagnostic_error(AgentError::Unknown));
                    return Err(AgentError::Unknown);
                }
                match &result {
                    Ok(_) => self.diagnostic.delivery(diagnostic_stage::RECEIVED_OK, 0),
                    Err(error) => self.diagnostic.delivery(diagnostic_stage::RECEIVER_ERROR, diagnostic_error(*error)),
                }
                result.map(Some)
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.consumed = true;
                self.diagnostic.delivery(diagnostic_stage::RECEIVER_DISCONNECTED, diagnostic_error(if self.has_started() { AgentError::Unknown } else { AgentError::Unavailable }));
                Err(if self.has_started() {
                    AgentError::Unknown
                } else {
                    AgentError::Unavailable
                })
            }
        }
    }
}
impl Drop for AgentCommandHandle {
    fn drop(&mut self) {
        if !self.consumed {
            self.cancel();
        }
    }
}

// Formatting loads only the private cell, never Control/live/clock/Core/IO.
impl std::fmt::Debug for AgentCommandHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { std::fmt::Debug::fmt(&*self.diagnostic, f) }
}
#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn command_first_error_is_sticky_and_snapshot_is_bounded() {
        let trace = CommandDiagnostic::default();
        trace.worker(diagnostic_stage::PORT_REJECTED, 14);
        trace.worker(diagnostic_stage::REGISTER_REJECTED, 19);
        let text = format!("{trace:?}");
        assert!(text.contains("PORT_REJECTED") && text.contains("R2Denied"));
        assert!(!text.contains("REGISTER_REJECTED"));
        assert!(text.len() < 384);
    }
    #[test]
    fn command_late_flags_preserve_worker_and_delivery_causes() {
        let trace = CommandDiagnostic::default();
        trace.worker(diagnostic_stage::PORT_REJECTED, 17);
        trace.delivery(diagnostic_stage::CONTROL_VETO, 5);
        trace.flag((1 << 32) | (1 << 33));
        let value = trace.0.load(Ordering::Acquire);
        assert_eq!(value & 0xffff, 8 | (17 << 8));
        assert_eq!((value >> 16) & 0xffff, 12 | (5 << 8));
        assert!(format!("{trace:?}").contains("provider_observed: true"));
    }
    #[test]
    fn command_format_failure_does_not_mutate_snapshot() {
        struct Refuse;
        impl std::fmt::Write for Refuse { fn write_str(&mut self, _: &str) -> std::fmt::Result { Err(std::fmt::Error) } }
        let trace = CommandDiagnostic::default();
        trace.worker(diagnostic_stage::PORT_START, 0);
        let before = trace.0.load(Ordering::Acquire);
        assert!(std::fmt::write(&mut Refuse, format_args!("{trace:?}")).is_err());
        assert_eq!(trace.0.load(Ordering::Acquire), before);
    }
    #[test]
    fn command_debug_has_fixed_fields_and_no_payload_input() {
        let trace = CommandDiagnostic::default();
        assert_eq!(format!("{trace:?}"), "CommandDiagnosticV1 { worker_stage: \"NOT_OBSERVED\", worker_error_class: \"NOT_OBSERVED\", delivery_stage: \"NOT_OBSERVED\", delivery_error_class: \"NOT_OBSERVED\", command_started_observed: false, provider_observed: false }");
        assert_eq!(diagnostic_error(AgentError::Session(morrow_agent_session_exec_v1_r2::Error::Storage)), 18);
    }
}
