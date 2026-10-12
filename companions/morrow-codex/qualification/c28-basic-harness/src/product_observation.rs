//! Qualification receipts from the original public AD31 responder only.
//! No StateSlot mutation, worker reclaim, authority creation or retry occurs here.
use anyhow::{Result, anyhow, ensure};
use morrow_workbench_host::{
    Workbench,
    agent_task_discovery::{Kind, Observation, Request},
    io_tasks::TaskKey,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy)]
pub(crate) enum Phase {
    SessionBeforeStart,
    SessionRunning,
    SessionRetainedExit,
    SessionAcknowledged,
    NativeBeforeStart,
    NativeRunning,
    NativeRetainedExit,
    NativeAcknowledged,
}
impl Phase {
    fn label(self) -> &'static str {
        match self {
            Self::SessionBeforeStart => "session-before-start",
            Self::SessionRunning => "session-running",
            Self::SessionRetainedExit => "session-retained-exit",
            Self::SessionAcknowledged => "session-acknowledged",
            Self::NativeBeforeStart => "native-before-start",
            Self::NativeRunning => "native-running",
            Self::NativeRetainedExit => "native-retained-exit",
            Self::NativeAcknowledged => "native-acknowledged",
        }
    }
    fn expected(self) -> Kind {
        match self {
            Self::SessionBeforeStart | Self::NativeBeforeStart
            | Self::SessionAcknowledged | Self::NativeAcknowledged => Kind::None,
            Self::SessionRunning | Self::NativeRunning => Kind::RunningAgent,
            Self::SessionRetainedExit | Self::NativeRetainedExit => Kind::RetainedAgentExit,
        }
    }
    fn before_start(self) -> bool {
        matches!(self, Self::SessionBeforeStart | Self::NativeBeforeStart)
    }
}

/// `lifecycle_task` is the actual returned key, retained as correlation data even
/// after ACK. It is never an admission or authority witness. Workflow's existing
/// attempted-step latch bounds these calls; this function never resends a request.
pub(crate) fn expect(
    workbench: &mut Workbench,
    namespace: &str,
    phase: Phase,
    lifecycle_task: Option<TaskKey>,
) -> Result<Value> {
    ensure!(!namespace.is_empty() && namespace.len() <= 256,
        "AD31 qualification namespace bounds");
    ensure!(phase.before_start() == lifecycle_task.is_none(),
        "AD31 qualification lifecycle key shape");
    let expected_kind = phase.expected();
    let expected_task = if matches!(expected_kind, Kind::RunningAgent | Kind::RetainedAgentExit) {
        lifecycle_task
    } else { None };
    let owner = workbench.agent_owner_binding();
    let gate = workbench.product_gate();
    let gate_closed_before = gate.closed();
    ensure!(!gate_closed_before, "original AD31 gate closed before observation; no retry");

    // Length-delimited domain/namespace/phase plus a keyed/no-key discriminator.
    // The eight fixed phase labels give distinct IDs within one workflow.
    let mut identity = Sha256::new();
    for part in [b"stage34-protected-owner-ad31-v1".as_slice(), namespace.as_bytes(), phase.label().as_bytes()] {
        identity.update((part.len() as u64).to_le_bytes());
        identity.update(part);
    }
    identity.update([u8::from(lifecycle_task.is_some())]);
    if let Some(key) = lifecycle_task { identity.update(key.as_bytes()); }
    let id: [u8; 32] = identity.finalize().into();
    ensure!(id != [0; 32], "AD31 qualification request identity unavailable");
    let request = Request { id };
    let input = request.encode().map_err(|e| anyhow!("original AD31 request encoding: {e}"))?;
    let request_sha256 = crate::preflight::hex(&Sha256::digest(&input));
    let response = morrow_workbench_host::protocol::respond(workbench, &input)
        .map_err(|e| anyhow!("original public AD31 response unavailable; phase={} id={} request_sha256={request_sha256}: {e}; preserve original state and remaining cleanup; no retry",
            phase.label(), crate::preflight::hex(&id)))?;
    let reply_sha256 = crate::preflight::hex(&Sha256::digest(&response));
    let observed = Observation::decode_for(request, &input, &response)
        .map_err(|e| anyhow!("original AD31 correlation/decode failed; phase={} id={} request_sha256={request_sha256} reply_sha256={reply_sha256}: {e}; preserve original state and remaining cleanup; no retry",
            phase.label(), crate::preflight::hex(&id)))?;
    let gate_closed_after = gate.closed();
    let owner_unchanged = workbench.agent_owner_binding() == owner;
    let receipt = json!({
        "schema":"STAGE34_ORIGINAL_PROTECTED_OWNER_AD31_OBSERVATION_V1",
        "scope":"PUBLIC_RESPONDER_IN_PROCESS_NOT_MAIN_FRAMES_OR_SUPERVISOR_DART",
        "namespace":namespace, "phase":phase.label(), "request_id":crate::preflight::hex(&id),
        "request_bytes":input.len(), "request_sha256":request_sha256,
        "reply_bytes":response.len(), "reply_sha256":reply_sha256,
        "expected_kind":format!("{expected_kind:?}"), "observed_kind":format!("{:?}", observed.kind),
        "lifecycle_task":lifecycle_task.map(|key| crate::preflight::hex(key.as_bytes())),
        "expected_task":expected_task.map(|key| crate::preflight::hex(key.as_bytes())),
        "observed_task":observed.task.map(|key| crate::preflight::hex(key.as_bytes())),
        "same_key":observed.task == expected_task,
        "original_owner_binding":format!("{owner:?}"), "owner_binding_unchanged":owner_unchanged,
        "gate_closed_before":gate_closed_before, "gate_closed_after":gate_closed_after,
        "canonical_correlation_checked":true, "replayed":false,
    });
    ensure!(!gate_closed_after && owner_unchanged
        && observed.kind == expected_kind && observed.task == expected_task,
        "original AD31 lifecycle assertion failed; receipt={receipt}; preserve original state and remaining cleanup; no retry");
    Ok(receipt)
}
