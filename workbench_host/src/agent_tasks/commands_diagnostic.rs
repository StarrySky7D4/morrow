//! Finite, read-only observations of one original command.
//! This module has no owner, clock, callback, transport, payload or authority.
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentCommandStage {
    #[default]
    NotObserved,
    Queued,
    Received,
    CommandStarted,
    Checkpoint,
    OwnerMismatch,
    PrepareIo,
    PortStart,
    PortRejected,
    ProviderObserved,
    RegisterProcess,
    RegisterRejected,
    ControlVeto,
    ReceiverError,
    ReceiverDisconnected,
    ReplySendFailed,
    ReceivedOk,
    Cancelled,
    PortAbsent,
}

impl AgentCommandStage {
    fn from_code(value: u8) -> Self {
        use diagnostic_stage::*;
        match value {
            QUEUED => Self::Queued,
            RECEIVED => Self::Received,
            COMMAND_STARTED => Self::CommandStarted,
            CHECKPOINT => Self::Checkpoint,
            OWNER_MISMATCH => Self::OwnerMismatch,
            PREPARE_IO => Self::PrepareIo,
            PORT_START => Self::PortStart,
            PORT_REJECTED => Self::PortRejected,
            PROVIDER_OBSERVED => Self::ProviderObserved,
            REGISTER_PROCESS => Self::RegisterProcess,
            REGISTER_REJECTED => Self::RegisterRejected,
            CONTROL_VETO => Self::ControlVeto,
            RECEIVER_ERROR => Self::ReceiverError,
            RECEIVER_DISCONNECTED => Self::ReceiverDisconnected,
            REPLY_SEND_FAILED => Self::ReplySendFailed,
            RECEIVED_OK => Self::ReceivedOk,
            CANCELLED => Self::Cancelled,
            PORT_ABSENT => Self::PortAbsent,
            _ => Self::NotObserved,
        }
    }

    /// The existing bounded diagnostic label; no raw operating-system error.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotObserved => "NOT_OBSERVED",
            Self::Queued => "QUEUED",
            Self::Received => "RECEIVED",
            Self::CommandStarted => "COMMAND_STARTED",
            Self::Checkpoint => "CHECKPOINT",
            Self::OwnerMismatch => "OWNER_MISMATCH",
            Self::PrepareIo => "PREPARE_IO",
            Self::PortStart => "PORT_START",
            Self::PortRejected => "PORT_REJECTED",
            Self::ProviderObserved => "PROVIDER_OBSERVED",
            Self::RegisterProcess => "REGISTER_PROCESS",
            Self::RegisterRejected => "REGISTER_REJECTED",
            Self::ControlVeto => "CONTROL_VETO",
            Self::ReceiverError => "RECEIVER_ERROR",
            Self::ReceiverDisconnected => "RECEIVER_DISCONNECTED",
            Self::ReplySendFailed => "REPLY_SEND_FAILED",
            Self::ReceivedOk => "RECEIVED_OK",
            Self::Cancelled => "CANCELLED",
            Self::PortAbsent => "PORT_ABSENT",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentCommandErrorClass {
    #[default]
    NotObserved,
    Invalid,
    Limit,
    Busy,
    Cancelled,
    Unknown,
    Unavailable,
    Maintenance,
    Disconnect,
    Unsupported,
    R2Invalid,
    R2Contract,
    R2Limit,
    R2Correlation,
    R2Denied,
    R2Conflict,
    R2NotFound,
    R2CommitUnknown,
    R2Storage,
    RegistrationError,
}

impl AgentCommandErrorClass {
    fn from_code(value: u8) -> Self {
        match value {
            1 => Self::Invalid,
            2 => Self::Limit,
            3 => Self::Busy,
            4 => Self::Cancelled,
            5 => Self::Unknown,
            6 => Self::Unavailable,
            7 => Self::Maintenance,
            8 => Self::Disconnect,
            9 => Self::Unsupported,
            10 => Self::R2Invalid,
            11 => Self::R2Contract,
            12 => Self::R2Limit,
            13 => Self::R2Correlation,
            14 => Self::R2Denied,
            15 => Self::R2Conflict,
            16 => Self::R2NotFound,
            17 => Self::R2CommitUnknown,
            18 => Self::R2Storage,
            19 => Self::RegistrationError,
            _ => Self::NotObserved,
        }
    }

    /// The existing finite classification, never a message or OS error code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotObserved => "NOT_OBSERVED",
            Self::Invalid => "Invalid",
            Self::Limit => "Limit",
            Self::Busy => "Busy",
            Self::Cancelled => "Cancelled",
            Self::Unknown => "Unknown",
            Self::Unavailable => "Unavailable",
            Self::Maintenance => "Maintenance",
            Self::Disconnect => "Disconnect",
            Self::Unsupported => "Unsupported",
            Self::R2Invalid => "R2Invalid",
            Self::R2Contract => "R2Contract",
            Self::R2Limit => "R2Limit",
            Self::R2Correlation => "R2Correlation",
            Self::R2Denied => "R2Denied",
            Self::R2Conflict => "R2Conflict",
            Self::R2NotFound => "R2NotFound",
            Self::R2CommitUnknown => "R2CommitUnknown",
            Self::R2Storage => "R2Storage",
            Self::RegistrationError => "RegistrationError",
        }
    }
}

/// One coherent local observation, with the same finite fields as the old Debug view.
/// NotObserved/false never proves that no effect occurred. This value cannot
/// approve, cancel, recover, retry, reconstruct a handle or restore authority.
/// This is a Rust host diagnostic value, not a persisted or wire contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentCommandSnapshot {
    pub worker_stage: AgentCommandStage,
    pub worker_error_class: AgentCommandErrorClass,
    pub delivery_stage: AgentCommandStage,
    pub delivery_error_class: AgentCommandErrorClass,
    pub command_started_observed: bool,
    pub provider_observed: bool,
}

impl AgentCommandSnapshot {
    fn from_packed(value: u64) -> Self {
        Self {
            worker_stage: AgentCommandStage::from_code(value as u8),
            worker_error_class: AgentCommandErrorClass::from_code((value >> 8) as u8),
            delivery_stage: AgentCommandStage::from_code((value >> 16) as u8),
            delivery_error_class: AgentCommandErrorClass::from_code((value >> 24) as u8),
            command_started_observed: value & (1 << 32) != 0,
            provider_observed: value & (1 << 33) != 0,
        }
    }
}

// One bounded cell per original command; mutators retain their original behavior.
#[derive(Default)]
pub(crate) struct CommandDiagnostic(AtomicU64);
pub(crate) mod diagnostic_stage {
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

impl CommandDiagnostic {
    #[cfg(test)]
    pub(crate) fn packed_for_test(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
    pub(crate) fn snapshot(&self) -> AgentCommandSnapshot {
        AgentCommandSnapshot::from_packed(self.0.load(Ordering::Acquire))
    }
    pub(crate) fn worker(&self, stage: u8, class: u8) {
        let _ = self.0.fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
            if old & 0xff00 != 0 { return None; }
            Some((old & !0xffff) | u64::from(stage) | (u64::from(class) << 8))
        });
    }
    pub(crate) fn delivery(&self, stage: u8, class: u8) {
        let _ = self.0.fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
            if old & 0xff00_0000 != 0 { return None; }
            Some((old & !0xffff_0000) | (u64::from(stage) << 16) | (u64::from(class) << 24))
        });
    }
    pub(crate) fn flag(&self, flag: u64) { self.0.fetch_or(flag, Ordering::AcqRel); }
}

impl std::fmt::Debug for CommandDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = self.snapshot();
        f.debug_struct("CommandDiagnosticV1")
            .field("worker_stage", &value.worker_stage.as_str())
            .field("worker_error_class", &value.worker_error_class.as_str())
            .field("delivery_stage", &value.delivery_stage.as_str())
            .field("delivery_error_class", &value.delivery_error_class.as_str())
            .field("command_started_observed", &value.command_started_observed)
            .field("provider_observed", &value.provider_observed).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn snapshot_default_is_absent_observation_not_an_execution_receipt() {
        let cell = CommandDiagnostic::default();
        assert_eq!(cell.snapshot(), AgentCommandSnapshot::default());
        assert_eq!(cell.snapshot().worker_stage, AgentCommandStage::NotObserved);
        assert_eq!(cell.snapshot().delivery_error_class, AgentCommandErrorClass::NotObserved);
        assert!(!cell.snapshot().command_started_observed);
        assert!(!cell.snapshot().provider_observed);
    }

    #[test]
    fn returned_snapshot_is_a_detached_value_and_reads_do_not_mutate() {
        let cell = CommandDiagnostic::default();
        cell.worker(diagnostic_stage::PORT_REJECTED, 14);
        cell.delivery(diagnostic_stage::RECEIVER_ERROR, 5);
        let before = cell.0.load(Ordering::Acquire);
        let original = cell.snapshot();
        let mut detached = original;
        detached.worker_stage = AgentCommandStage::ReceivedOk;
        detached.worker_error_class = AgentCommandErrorClass::NotObserved;
        detached.provider_observed = true;
        assert_ne!(detached, original);
        for _ in 0..16 {
            assert_eq!(cell.snapshot(), original);
        }
        assert_eq!(cell.0.load(Ordering::Acquire), before);
    }

    #[test]
    fn delivery_error_cannot_be_cleared_by_late_success_or_flags() {
        let cell = CommandDiagnostic::default();
        cell.delivery(diagnostic_stage::RECEIVER_DISCONNECTED, 5);
        cell.delivery(diagnostic_stage::RECEIVED_OK, 0);
        cell.flag((1 << 32) | (1 << 33));
        let value = cell.snapshot();
        assert_eq!(value.delivery_stage, AgentCommandStage::ReceiverDisconnected);
        assert_eq!(value.delivery_error_class, AgentCommandErrorClass::Unknown);
        assert!(value.command_started_observed && value.provider_observed);
    }

    #[test]
    fn worker_and_delivery_first_errors_are_independent() {
        let cell = CommandDiagnostic::default();
        cell.worker(diagnostic_stage::PORT_REJECTED, 17);
        cell.delivery(diagnostic_stage::CONTROL_VETO, 5);
        cell.worker(diagnostic_stage::REGISTER_REJECTED, 19);
        cell.delivery(diagnostic_stage::RECEIVER_ERROR, 18);
        let value = cell.snapshot();
        assert_eq!(value.worker_stage, AgentCommandStage::PortRejected);
        assert_eq!(value.worker_error_class, AgentCommandErrorClass::R2CommitUnknown);
        assert_eq!(value.delivery_stage, AgentCommandStage::ControlVeto);
        assert_eq!(value.delivery_error_class, AgentCommandErrorClass::Unknown);
    }

    #[test]
    fn concurrent_snapshot_decodes_one_complete_atomic_observation() {
        // Test-only whole-cell writes stress the single-load decoder. Production
        // only uses the original sticky mutators; no reset/replace API is exposed.
        let a = u64::from(diagnostic_stage::PORT_REJECTED) | (14 << 8)
            | (u64::from(diagnostic_stage::CONTROL_VETO) << 16) | (5 << 24) | (1 << 32);
        let b = u64::from(diagnostic_stage::REGISTER_REJECTED) | (19 << 8)
            | (u64::from(diagnostic_stage::RECEIVER_DISCONNECTED) << 16) | (18 << 24) | (1 << 33);
        let cell = Arc::new(CommandDiagnostic(AtomicU64::new(a)));
        let barrier = Arc::new(Barrier::new(2));
        let writer_cell = cell.clone();
        let writer_barrier = barrier.clone();
        let writer = std::thread::spawn(move || {
            writer_barrier.wait();
            for i in 0..20_000 {
                writer_cell.0.store(if i % 2 == 0 { b } else { a }, Ordering::Release);
            }
        });
        let expected_a = AgentCommandSnapshot::from_packed(a);
        let expected_b = AgentCommandSnapshot::from_packed(b);
        barrier.wait();
        for _ in 0..20_000 {
            let value = cell.snapshot();
            assert!(value == expected_a || value == expected_b, "torn diagnostic snapshot");
        }
        writer.join().expect("finite diagnostic writer joined");
    }
}
