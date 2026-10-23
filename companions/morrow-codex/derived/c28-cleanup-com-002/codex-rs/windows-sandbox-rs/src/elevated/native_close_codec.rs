//! Strictly bounded, canonical, typed Cap'n Proto wire codec for the v8 native-close
//! protocol (`native_close_protocol`) and runtime envelope (`native_close_runtime`).
//!
//! # Security and Determinism Invariants
//! - **No JSON intermediate or fallback**: Only binary Cap'n Proto messages matching
//!   `NATIVE_CLOSE_WIRE_MAGIC`, `NATIVE_CLOSE_WIRE_REVISION`, `WireFormatKind::CapnpTyped`,
//!   and the expected `EnvelopeKind` are accepted. JSON or text inputs are rejected before
//!   any state machine dispatch.
//! - **Strict 16 KiB frame bound, depth limit, and traversal limit**: Every payload is
//!   bounded by `MAX_NATIVE_CLOSE_FRAME_BYTES` (16 KiB), `MAX_NATIVE_CLOSE_CAPNP_DEPTH` (16),
//!   and a bounded traversal budget for canonical-layout and typed-field reads.
//! - **Segment table and trailing byte validation**: Multi-message concatenation, trailing
//!   bytes inside the payload or after the length-prefixed frame, and truncated segments
//!   are rejected deterministically.
//! - **Canonical re-encoding**: Every decoded `Version8WireFrame` is re-encoded to canonical
//!   single-segment Cap'n Proto bytes and compared byte-for-byte against the input slice,
//!   rejecting unknown schema fields, omitted required pointers, non-zero padding, and
//!   extra unreferenced words.
//! - **Redacted diagnostics**: Error values never retain or format untrusted wire strings,
//!   field names, or secret nonce/tag bytes.

use super::super::ipc_framed::{ControlStatus, FramedMessage, Message};
use super::super::native_close_runtime::{
    RuntimeBootstrapWire, RuntimeDirection, RuntimePacketContent, RuntimePacketWire,
    RuntimeRunnerIdentity,
};
use super::{
    AttemptLeaseRetirementFact, BusinessExitWriterFact, ChildNativeExitFact, CloseDebtFact,
    CloseDebtTarget, ConptyClosureFact, DebtAndStickyUnknownFacts, EncodeErrorKind,
    ExtraJobMembersOriginFact, FirstErrorFact, HandleCloseFact, InputPeerEofFact,
    InputStreamWorkerFact, InputTerminationFact, IoStreamsAndWorkersFacts, JobActiveZeroDrainFact,
    JobAdmissionFact, JobForbiddenOpFact, LeaseRetirementFact, LeaseRetirementFacts,
    MalformedCapnpKind, NativeCloseBusinessExitWire, NativeCloseCapabilityOptIn,
    NativeCloseCodecError, NativeCloseControlResultWire, NativeCloseHandshakeAckWire,
    NativeCloseHandshakeHelloWire, NativeCloseOutputFrameWire, NativeCloseOutputStream,
    NativeCloseReceiptWire, NativeCloseSpawnReadyWire, ObservationProof, OriginalHandlesCloseFacts,
    OriginalJobLifecycleFacts, OriginalProcessWaitProof, OutputStreamWorkerFact, PipeEofFact,
    PoisonStateFact, PoisonTarget, PriorControlWritersFact, ProtocolFailureStage,
    ProtocolStickyFailureKind, ReplayDisposition, ReportedNativeCloseFacts,
    SenderReceiptWriterFact, StageOrderingFact, StderrStreamWorkerFact, StdinHandleCloseFact,
    StdinWriteDisposition, StickyUnknownStateFact, TimeoutDisposition, Version8Message,
    Version8WireFrame, WireAuthTag, WireChildIdentity, WorkerNativeJoinFact, WriterClosureFacts,
    MAX_NATIVE_CLOSE_CAPNP_DEPTH, MAX_NATIVE_CLOSE_FRAME_BYTES, NATIVE_CLOSE_IPC_PROTOCOL_VERSION,
};
use crate::native_close_wire_capnp as wire;

/// 64-bit magic header identifying Morrow v8 native-close Cap'n Proto wire envelopes (`"WMORCLV8"`).
pub const NATIVE_CLOSE_WIRE_MAGIC: u64 = 0x3856_4c43_524f_4d57;

/// Pinned binary wire revision for `schemas/native_close.capnp`.
pub const NATIVE_CLOSE_WIRE_REVISION: u16 = 1;

/// Permit the canonical-layout pass and typed reads, bounded to four wire sizes.
pub const MAX_NATIVE_CLOSE_CAPNP_TRAVERSAL_WORDS: usize = 4 * MAX_NATIVE_CLOSE_FRAME_BYTES / 8;

/// Maximum number of segments allowed in a canonical native-close Cap'n Proto message.
const MAX_CAPNP_SEGMENTS: usize = 4;

fn map_not_in_schema(_: capnp::NotInSchema) -> NativeCloseCodecError {
    NativeCloseCodecError::UnknownSchemaVariant
}

fn map_capnp_err(err: capnp::Error) -> NativeCloseCodecError {
    match err.kind {
        capnp::ErrorKind::NestingLimitExceeded
        | capnp::ErrorKind::MessageIsTooDeeplyNested
        | capnp::ErrorKind::MessageIsTooDeeplyNestedOrContainsCycles => {
            NativeCloseCodecError::MaxCapnpDepthExceeded {
                max_depth: MAX_NATIVE_CLOSE_CAPNP_DEPTH,
            }
        }
        capnp::ErrorKind::ReadLimitExceeded => NativeCloseCodecError::TraversalLimitExceeded,
        _ => NativeCloseCodecError::MalformedCapnp(MalformedCapnpKind::InvalidPointer),
    }
}

fn array32(slice: &[u8]) -> Result<[u8; 32], NativeCloseCodecError> {
    if slice.len() != 32 {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidDataFieldLength,
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(slice);
    Ok(out)
}

fn looks_like_json_or_text_prefix(payload: &[u8]) -> bool {
    let trimmed = match payload.iter().position(|b| !b.is_ascii_whitespace()) {
        Some(idx) => &payload[idx..],
        None => return true,
    };
    matches!(trimmed.first(), Some(b'{' | b'[' | b'"'))
}

/// Validates the raw Cap'n Proto flat-slice segment table before message construction.
///
/// Distinguishes:
/// - legacy JSON / text inputs (`MalformedCapnp(NonCapnpOrJsonInput)`),
/// - truncated headers or segments (`MalformedCapnp(TruncatedSegment)`),
/// - invalid segment counts/sizes (`MalformedCapnp(InvalidSegmentTable)`),
/// - trailing bytes or concatenated frames (`TrailingBytesInPayload`).
fn validate_capnp_segment_framing(payload: &[u8]) -> Result<(), NativeCloseCodecError> {
    if payload.is_empty() {
        return Err(NativeCloseCodecError::EmptyFrame);
    }
    if payload.len() > MAX_NATIVE_CLOSE_FRAME_BYTES {
        return Err(NativeCloseCodecError::FrameTooLarge {
            len: payload.len(),
            max: MAX_NATIVE_CLOSE_FRAME_BYTES,
        });
    }
    if looks_like_json_or_text_prefix(payload) {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::NonCapnpOrJsonInput,
        ));
    }
    if payload.len() < 8 {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::TruncatedSegment,
        ));
    }
    let seg_count_minus_one =
        u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
    let seg_count =
        seg_count_minus_one
            .checked_add(1)
            .ok_or(NativeCloseCodecError::MalformedCapnp(
                MalformedCapnpKind::InvalidSegmentTable,
            ))?;
    if seg_count > MAX_CAPNP_SEGMENTS {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidSegmentTable,
        ));
    }
    let header_words = (seg_count / 2) + 1;
    let header_bytes = header_words * 8;
    if payload.len() < header_bytes {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::TruncatedSegment,
        ));
    }
    // Verify unused padding half-word in header is zero when segment count is even.
    if seg_count % 2 == 0 {
        let pad_offset = 4 + seg_count * 4;
        let pad = u32::from_le_bytes([
            payload[pad_offset],
            payload[pad_offset + 1],
            payload[pad_offset + 2],
            payload[pad_offset + 3],
        ]);
        if pad != 0 {
            return Err(NativeCloseCodecError::MalformedCapnp(
                MalformedCapnpKind::InvalidSegmentTable,
            ));
        }
    }
    let mut total_segment_words = 0usize;
    for i in 0..seg_count {
        let offset = 4 + i * 4;
        let seg_words = u32::from_le_bytes([
            payload[offset],
            payload[offset + 1],
            payload[offset + 2],
            payload[offset + 3],
        ]) as usize;
        if seg_words == 0 || seg_words > MAX_NATIVE_CLOSE_CAPNP_TRAVERSAL_WORDS {
            return Err(NativeCloseCodecError::MalformedCapnp(
                MalformedCapnpKind::InvalidSegmentTable,
            ));
        }
        total_segment_words = total_segment_words.checked_add(seg_words).ok_or(
            NativeCloseCodecError::MalformedCapnp(MalformedCapnpKind::InvalidSegmentTable),
        )?;
    }
    let total_words = header_words.checked_add(total_segment_words).ok_or(
        NativeCloseCodecError::MalformedCapnp(MalformedCapnpKind::InvalidSegmentTable),
    )?;
    let expected_bytes =
        total_words
            .checked_mul(8)
            .ok_or(NativeCloseCodecError::MalformedCapnp(
                MalformedCapnpKind::InvalidSegmentTable,
            ))?;
    if payload.len() < expected_bytes {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::TruncatedSegment,
        ));
    }
    if payload.len() > expected_bytes {
        return Err(NativeCloseCodecError::TrailingBytesInPayload);
    }
    if total_segment_words > MAX_NATIVE_CLOSE_CAPNP_TRAVERSAL_WORDS {
        return Err(NativeCloseCodecError::TraversalLimitExceeded);
    }
    Ok(())
}

/// Validates that an embedded `FramedMessage` inside a `RuntimePacketWire` has its
/// session nonce redacted to all-zeros (`[0u8; 32]`) and never carries a raw `Hello`.
fn validate_redacted_runner_frame_for_encode(
    framed: &FramedMessage,
) -> Result<(), NativeCloseCodecError> {
    if framed.version != NATIVE_CLOSE_IPC_PROTOCOL_VERSION {
        return Err(NativeCloseCodecError::EncodeFailed(
            EncodeErrorKind::InvalidEmbeddedRunnerFrame,
        ));
    }
    match &framed.message {
        Message::Hello { .. } => Err(NativeCloseCodecError::EncodeFailed(
            EncodeErrorKind::UnredactedWireNonce,
        )),
        Message::HelloAck { payload } => {
            if payload.hello.nonce != [0u8; 32] {
                return Err(NativeCloseCodecError::EncodeFailed(
                    EncodeErrorKind::UnredactedWireNonce,
                ));
            }
            Ok(())
        }
        Message::ControlRequest { payload } => {
            if payload.nonce != [0u8; 32] {
                return Err(NativeCloseCodecError::EncodeFailed(
                    EncodeErrorKind::UnredactedWireNonce,
                ));
            }
            Ok(())
        }
        Message::ControlResult { payload } => {
            if payload.request.nonce != [0u8; 32] {
                return Err(NativeCloseCodecError::EncodeFailed(
                    EncodeErrorKind::UnredactedWireNonce,
                ));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_redacted_runner_frame_on_decode(
    framed: &FramedMessage,
) -> Result<(), NativeCloseCodecError> {
    if framed.version != NATIVE_CLOSE_IPC_PROTOCOL_VERSION {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidEmbeddedRunnerFrame,
        ));
    }
    match &framed.message {
        Message::Hello { .. } => Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::UnredactedWireNonce,
        )),
        Message::HelloAck { payload } => {
            if payload.hello.nonce != [0u8; 32] {
                return Err(NativeCloseCodecError::MalformedCapnp(
                    MalformedCapnpKind::UnredactedWireNonce,
                ));
            }
            Ok(())
        }
        Message::ControlRequest { payload } => {
            if payload.nonce != [0u8; 32] {
                return Err(NativeCloseCodecError::MalformedCapnp(
                    MalformedCapnpKind::UnredactedWireNonce,
                ));
            }
            Ok(())
        }
        Message::ControlResult { payload } => {
            if payload.request.nonce != [0u8; 32] {
                return Err(NativeCloseCodecError::MalformedCapnp(
                    MalformedCapnpKind::UnredactedWireNonce,
                ));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

// ============================================================================
// Primitive & Fact Domain Encoders / Decoders
// ============================================================================

fn put_maybe_i32(mut b: wire::maybe_i32::Builder<'_>, val: Option<i32>) {
    match val {
        None => b.set_none(()),
        Some(v) => b.set_some(v),
    }
}

fn get_maybe_i32(r: wire::maybe_i32::Reader<'_>) -> Result<Option<i32>, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::maybe_i32::None(()) => None,
        wire::maybe_i32::Some(v) => Some(v),
    })
}

fn put_maybe_auth_tag(mut b: wire::maybe_auth_tag::Builder<'_>, val: Option<WireAuthTag>) {
    match val {
        None => b.set_none(()),
        Some(tag) => b.set_some(tag.as_bytes()),
    }
}

fn get_maybe_auth_tag(
    r: wire::maybe_auth_tag::Reader<'_>,
) -> Result<Option<WireAuthTag>, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::maybe_auth_tag::None(()) => None,
        wire::maybe_auth_tag::Some(data) => {
            let bytes = data.map_err(map_capnp_err)?;
            Some(WireAuthTag::from_bytes(array32(bytes)?))
        }
    })
}

fn put_wire_child_identity(mut b: wire::wire_child_identity::Builder<'_>, id: &WireChildIdentity) {
    b.set_pid(id.pid);
    b.set_thread_id(id.thread_id);
    b.set_creation_time_ticks(id.creation_filetime_100ns);
}

fn get_wire_child_identity(r: wire::wire_child_identity::Reader<'_>) -> WireChildIdentity {
    WireChildIdentity {
        pid: r.get_pid(),
        thread_id: r.get_thread_id(),
        creation_filetime_100ns: r.get_creation_time_ticks(),
    }
}

fn put_runtime_runner_identity(
    mut b: wire::runtime_runner_identity::Builder<'_>,
    id: &RuntimeRunnerIdentity,
) {
    b.set_process_id(id.process_id);
    b.set_thread_id(id.thread_id);
    b.set_creation_time_ticks(id.creation_filetime_100ns);
}

fn get_runtime_runner_identity(
    r: wire::runtime_runner_identity::Reader<'_>,
) -> RuntimeRunnerIdentity {
    RuntimeRunnerIdentity {
        process_id: r.get_process_id(),
        thread_id: r.get_thread_id(),
        creation_filetime_100ns: r.get_creation_time_ticks(),
    }
}

fn put_observation_proof(v: ObservationProof) -> wire::ObservationProof {
    match v {
        ObservationProof::NotObserved => wire::ObservationProof::NotObserved,
        ObservationProof::NotProven => wire::ObservationProof::NotProven,
        ObservationProof::ProvenOnOriginal => wire::ObservationProof::ProvenOnOriginal,
    }
}

fn get_observation_proof(v: wire::ObservationProof) -> ObservationProof {
    match v {
        wire::ObservationProof::NotObserved => ObservationProof::NotObserved,
        wire::ObservationProof::NotProven => ObservationProof::NotProven,
        wire::ObservationProof::ProvenOnOriginal => ObservationProof::ProvenOnOriginal,
    }
}

fn put_timeout_disposition(v: TimeoutDisposition) -> wire::TimeoutDisposition {
    match v {
        TimeoutDisposition::CompletedWithinBudget => {
            wire::TimeoutDisposition::CompletedWithinBudget
        }
        TimeoutDisposition::TimedOut => wire::TimeoutDisposition::TimedOut,
    }
}

fn get_timeout_disposition(v: wire::TimeoutDisposition) -> TimeoutDisposition {
    match v {
        wire::TimeoutDisposition::CompletedWithinBudget => {
            TimeoutDisposition::CompletedWithinBudget
        }
        wire::TimeoutDisposition::TimedOut => TimeoutDisposition::TimedOut,
    }
}

fn put_stage_ordering_fact(v: StageOrderingFact) -> wire::StageOrderingFact {
    match v {
        StageOrderingFact::NotObserved => wire::StageOrderingFact::NotObserved,
        StageOrderingFact::NotProven => wire::StageOrderingFact::NotProven,
        StageOrderingFact::ProvenInStrictOrder => wire::StageOrderingFact::ProvenInStrictOrder,
        StageOrderingFact::OrderViolated => wire::StageOrderingFact::OrderViolated,
    }
}

fn get_stage_ordering_fact(v: wire::StageOrderingFact) -> StageOrderingFact {
    match v {
        wire::StageOrderingFact::NotObserved => StageOrderingFact::NotObserved,
        wire::StageOrderingFact::NotProven => StageOrderingFact::NotProven,
        wire::StageOrderingFact::ProvenInStrictOrder => StageOrderingFact::ProvenInStrictOrder,
        wire::StageOrderingFact::OrderViolated => StageOrderingFact::OrderViolated,
    }
}

fn put_original_process_wait_proof(v: OriginalProcessWaitProof) -> wire::OriginalProcessWaitProof {
    match v {
        OriginalProcessWaitProof::NotObserved => wire::OriginalProcessWaitProof::NotObserved,
        OriginalProcessWaitProof::NotProven => wire::OriginalProcessWaitProof::NotProven,
        OriginalProcessWaitProof::QueryOnlyWithoutWaitSignal => {
            wire::OriginalProcessWaitProof::QueryOnlyWithoutWaitSignal
        }
        OriginalProcessWaitProof::WaitTimedOutOrStillActive => {
            wire::OriginalProcessWaitProof::WaitTimedOutOrStillActive
        }
        OriginalProcessWaitProof::SignaledExitedOnOriginalHandle => {
            wire::OriginalProcessWaitProof::SignaledExitedOnOriginalHandle
        }
    }
}

fn get_original_process_wait_proof(v: wire::OriginalProcessWaitProof) -> OriginalProcessWaitProof {
    match v {
        wire::OriginalProcessWaitProof::NotObserved => OriginalProcessWaitProof::NotObserved,
        wire::OriginalProcessWaitProof::NotProven => OriginalProcessWaitProof::NotProven,
        wire::OriginalProcessWaitProof::QueryOnlyWithoutWaitSignal => {
            OriginalProcessWaitProof::QueryOnlyWithoutWaitSignal
        }
        wire::OriginalProcessWaitProof::WaitTimedOutOrStillActive => {
            OriginalProcessWaitProof::WaitTimedOutOrStillActive
        }
        wire::OriginalProcessWaitProof::SignaledExitedOnOriginalHandle => {
            OriginalProcessWaitProof::SignaledExitedOnOriginalHandle
        }
    }
}

fn put_child_native_exit_fact(
    mut b: wire::child_native_exit_fact::Builder<'_>,
    fact: &ChildNativeExitFact,
) {
    match fact {
        ChildNativeExitFact::NotObserved => b.set_not_observed(()),
        ChildNativeExitFact::NotProven => b.set_not_proven(()),
        ChildNativeExitFact::NotCreatedPreLaunch => b.set_not_created_pre_launch(()),
        ChildNativeExitFact::WaitTimedOutOrIncomplete { identity } => {
            put_wire_child_identity(b.init_wait_timed_out_or_incomplete(), identity);
        }
        ChildNativeExitFact::Witnessed {
            identity,
            raw_exit_code_dword,
            business_exit_code_i32,
            wait_on_original_handle,
            timeout_disposition,
        } => {
            let mut w = b.init_witnessed();
            put_wire_child_identity(w.reborrow().init_identity(), identity);
            w.set_raw_exit_code_dword(*raw_exit_code_dword);
            w.set_business_exit_code(*business_exit_code_i32);
            w.set_wait_on_original_handle(put_original_process_wait_proof(
                *wait_on_original_handle,
            ));
            w.set_timeout_disposition(put_timeout_disposition(*timeout_disposition));
        }
    }
}

fn get_child_native_exit_fact(
    r: wire::child_native_exit_fact::Reader<'_>,
) -> Result<ChildNativeExitFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::child_native_exit_fact::NotObserved(()) => ChildNativeExitFact::NotObserved,
        wire::child_native_exit_fact::NotProven(()) => ChildNativeExitFact::NotProven,
        wire::child_native_exit_fact::NotCreatedPreLaunch(()) => {
            ChildNativeExitFact::NotCreatedPreLaunch
        }
        wire::child_native_exit_fact::WaitTimedOutOrIncomplete(id) => {
            let id = id.map_err(map_capnp_err)?;
            ChildNativeExitFact::WaitTimedOutOrIncomplete {
                identity: get_wire_child_identity(id),
            }
        }
        wire::child_native_exit_fact::Witnessed(w) => {
            let w = w.map_err(map_capnp_err)?;
            if !w.has_identity() {
                return Err(NativeCloseCodecError::MalformedCapnp(
                    MalformedCapnpKind::InvalidPointer,
                ));
            }
            ChildNativeExitFact::Witnessed {
                identity: get_wire_child_identity(w.get_identity().map_err(map_capnp_err)?),
                raw_exit_code_dword: w.get_raw_exit_code_dword(),
                business_exit_code_i32: w.get_business_exit_code(),
                wait_on_original_handle: get_original_process_wait_proof(
                    w.get_wait_on_original_handle().map_err(map_not_in_schema)?,
                ),
                timeout_disposition: get_timeout_disposition(
                    w.get_timeout_disposition().map_err(map_not_in_schema)?,
                ),
            }
        }
    })
}

fn put_pipe_eof_fact(mut b: wire::pipe_eof_fact::Builder<'_>, fact: &PipeEofFact) {
    match fact {
        PipeEofFact::NotObserved => b.set_not_observed(()),
        PipeEofFact::NotProven => b.set_not_proven(()),
        PipeEofFact::CleanEofObserved => b.set_clean_eof_observed(()),
        PipeEofFact::ReadFailedBeforeEof { os_error } => {
            put_maybe_i32(b.init_read_failed_before_eof(), *os_error);
        }
    }
}

fn get_pipe_eof_fact(
    r: wire::pipe_eof_fact::Reader<'_>,
) -> Result<PipeEofFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::pipe_eof_fact::NotObserved(()) => PipeEofFact::NotObserved,
        wire::pipe_eof_fact::NotProven(()) => PipeEofFact::NotProven,
        wire::pipe_eof_fact::CleanEofObserved(()) => PipeEofFact::CleanEofObserved,
        wire::pipe_eof_fact::ReadFailedBeforeEof(err) => PipeEofFact::ReadFailedBeforeEof {
            os_error: get_maybe_i32(err.map_err(map_capnp_err)?)?,
        },
    })
}

fn put_worker_native_join_fact(
    mut b: wire::worker_native_join_fact::Builder<'_>,
    fact: &WorkerNativeJoinFact,
) {
    match fact {
        WorkerNativeJoinFact::NotObserved => b.set_not_observed(()),
        WorkerNativeJoinFact::NotProven => b.set_not_proven(()),
        WorkerNativeJoinFact::StillRunning => b.set_still_running(()),
        WorkerNativeJoinFact::JoinedTimelyClean => b.set_joined_timely_clean(()),
        WorkerNativeJoinFact::JoinedWithWorkerError => b.set_joined_with_worker_error(()),
        WorkerNativeJoinFact::JoinedLateAfterBudget => b.set_joined_late_after_budget(()),
        WorkerNativeJoinFact::TimedOutStillRunning => b.set_timed_out_still_running(()),
        WorkerNativeJoinFact::WaitFailed { os_error } => {
            put_maybe_i32(b.init_wait_failed(), *os_error);
        }
        WorkerNativeJoinFact::Panicked => b.set_panicked(()),
    }
}

fn get_worker_native_join_fact(
    r: wire::worker_native_join_fact::Reader<'_>,
) -> Result<WorkerNativeJoinFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::worker_native_join_fact::NotObserved(()) => WorkerNativeJoinFact::NotObserved,
        wire::worker_native_join_fact::NotProven(()) => WorkerNativeJoinFact::NotProven,
        wire::worker_native_join_fact::StillRunning(()) => WorkerNativeJoinFact::StillRunning,
        wire::worker_native_join_fact::JoinedTimelyClean(()) => {
            WorkerNativeJoinFact::JoinedTimelyClean
        }
        wire::worker_native_join_fact::JoinedWithWorkerError(()) => {
            WorkerNativeJoinFact::JoinedWithWorkerError
        }
        wire::worker_native_join_fact::JoinedLateAfterBudget(()) => {
            WorkerNativeJoinFact::JoinedLateAfterBudget
        }
        wire::worker_native_join_fact::TimedOutStillRunning(()) => {
            WorkerNativeJoinFact::TimedOutStillRunning
        }
        wire::worker_native_join_fact::WaitFailed(err) => WorkerNativeJoinFact::WaitFailed {
            os_error: get_maybe_i32(err.map_err(map_capnp_err)?)?,
        },
        wire::worker_native_join_fact::Panicked(()) => WorkerNativeJoinFact::Panicked,
    })
}

fn put_handle_close_fact(mut b: wire::handle_close_fact::Builder<'_>, fact: &HandleCloseFact) {
    match fact {
        HandleCloseFact::NotObserved => b.set_not_observed(()),
        HandleCloseFact::NotProven => b.set_not_proven(()),
        HandleCloseFact::ClosedVerifiedOnce => b.set_closed_verified_once(()),
        HandleCloseFact::NotApplicableAbsent => b.set_not_applicable_absent(()),
        HandleCloseFact::StillOpen => b.set_still_open(()),
        HandleCloseFact::CloseDebtRetained { os_error } => {
            put_maybe_i32(b.init_close_debt_retained(), *os_error);
        }
    }
}

fn get_handle_close_fact(
    r: wire::handle_close_fact::Reader<'_>,
) -> Result<HandleCloseFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::handle_close_fact::NotObserved(()) => HandleCloseFact::NotObserved,
        wire::handle_close_fact::NotProven(()) => HandleCloseFact::NotProven,
        wire::handle_close_fact::ClosedVerifiedOnce(()) => HandleCloseFact::ClosedVerifiedOnce,
        wire::handle_close_fact::NotApplicableAbsent(()) => HandleCloseFact::NotApplicableAbsent,
        wire::handle_close_fact::StillOpen(()) => HandleCloseFact::StillOpen,
        wire::handle_close_fact::CloseDebtRetained(err) => HandleCloseFact::CloseDebtRetained {
            os_error: get_maybe_i32(err.map_err(map_capnp_err)?)?,
        },
    })
}

fn put_output_stream_worker_fact(
    mut b: wire::output_stream_worker_fact::Builder<'_>,
    fact: &OutputStreamWorkerFact,
) {
    put_pipe_eof_fact(b.reborrow().init_pipe_eof(), &fact.pipe_eof);
    put_worker_native_join_fact(b.reborrow().init_reader_join(), &fact.reader_join);
    put_handle_close_fact(b.init_handle_close(), &fact.handle_close);
}

fn get_output_stream_worker_fact(
    r: wire::output_stream_worker_fact::Reader<'_>,
) -> Result<OutputStreamWorkerFact, NativeCloseCodecError> {
    if !r.has_pipe_eof() || !r.has_reader_join() || !r.has_handle_close() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(OutputStreamWorkerFact {
        pipe_eof: get_pipe_eof_fact(r.get_pipe_eof().map_err(map_capnp_err)?)?,
        reader_join: get_worker_native_join_fact(r.get_reader_join().map_err(map_capnp_err)?)?,
        handle_close: get_handle_close_fact(r.get_handle_close().map_err(map_capnp_err)?)?,
    })
}

fn put_stderr_stream_worker_fact(
    mut b: wire::stderr_stream_worker_fact::Builder<'_>,
    fact: &StderrStreamWorkerFact,
) {
    match fact {
        StderrStreamWorkerFact::MergedConptyOrAbsent { handle_close } => {
            put_handle_close_fact(b.init_merged_conpty_or_absent(), handle_close);
        }
        StderrStreamWorkerFact::SeparateStream { stream } => {
            put_output_stream_worker_fact(b.init_separate_stream(), stream);
        }
    }
}

fn get_stderr_stream_worker_fact(
    r: wire::stderr_stream_worker_fact::Reader<'_>,
) -> Result<StderrStreamWorkerFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::stderr_stream_worker_fact::MergedConptyOrAbsent(hc) => {
            StderrStreamWorkerFact::MergedConptyOrAbsent {
                handle_close: get_handle_close_fact(hc.map_err(map_capnp_err)?)?,
            }
        }
        wire::stderr_stream_worker_fact::SeparateStream(s) => {
            StderrStreamWorkerFact::SeparateStream {
                stream: get_output_stream_worker_fact(s.map_err(map_capnp_err)?)?,
            }
        }
    })
}

fn put_input_peer_eof_fact(mut b: wire::input_peer_eof_fact::Builder<'_>, fact: &InputPeerEofFact) {
    match fact {
        InputPeerEofFact::NotObserved => b.set_not_observed(()),
        InputPeerEofFact::NotProven => b.set_not_proven(()),
        InputPeerEofFact::CleanPostChildExitFrameBoundaryEof => {
            b.set_clean_post_child_exit_frame_boundary_eof(())
        }
        InputPeerEofFact::PrematureDisconnectBeforeChildExit => {
            b.set_premature_disconnect_before_child_exit(())
        }
        InputPeerEofFact::PartialFrameAtEof => b.set_partial_frame_at_eof(()),
        InputPeerEofFact::FrameTooLarge => b.set_frame_too_large(()),
        InputPeerEofFact::MalformedFrame => b.set_malformed_frame(()),
        InputPeerEofFact::OperationAborted => b.set_operation_aborted(()),
        InputPeerEofFact::IoError { os_error } => {
            put_maybe_i32(b.init_io_error(), *os_error);
        }
    }
}

fn get_input_peer_eof_fact(
    r: wire::input_peer_eof_fact::Reader<'_>,
) -> Result<InputPeerEofFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::input_peer_eof_fact::NotObserved(()) => InputPeerEofFact::NotObserved,
        wire::input_peer_eof_fact::NotProven(()) => InputPeerEofFact::NotProven,
        wire::input_peer_eof_fact::CleanPostChildExitFrameBoundaryEof(()) => {
            InputPeerEofFact::CleanPostChildExitFrameBoundaryEof
        }
        wire::input_peer_eof_fact::PrematureDisconnectBeforeChildExit(()) => {
            InputPeerEofFact::PrematureDisconnectBeforeChildExit
        }
        wire::input_peer_eof_fact::PartialFrameAtEof(()) => InputPeerEofFact::PartialFrameAtEof,
        wire::input_peer_eof_fact::FrameTooLarge(()) => InputPeerEofFact::FrameTooLarge,
        wire::input_peer_eof_fact::MalformedFrame(()) => InputPeerEofFact::MalformedFrame,
        wire::input_peer_eof_fact::OperationAborted(()) => InputPeerEofFact::OperationAborted,
        wire::input_peer_eof_fact::IoError(err) => InputPeerEofFact::IoError {
            os_error: get_maybe_i32(err.map_err(map_capnp_err)?)?,
        },
    })
}

fn put_input_termination_fact(v: InputTerminationFact) -> wire::InputTerminationFact {
    match v {
        InputTerminationFact::NoTerminationObserved => {
            wire::InputTerminationFact::NoTerminationObserved
        }
        InputTerminationFact::ExplicitTerminateReceived => {
            wire::InputTerminationFact::ExplicitTerminateReceived
        }
        InputTerminationFact::TerminatedOnDisconnect => {
            wire::InputTerminationFact::TerminatedOnDisconnect
        }
    }
}

fn get_input_termination_fact(v: wire::InputTerminationFact) -> InputTerminationFact {
    match v {
        wire::InputTerminationFact::NoTerminationObserved => {
            InputTerminationFact::NoTerminationObserved
        }
        wire::InputTerminationFact::ExplicitTerminateReceived => {
            InputTerminationFact::ExplicitTerminateReceived
        }
        wire::InputTerminationFact::TerminatedOnDisconnect => {
            InputTerminationFact::TerminatedOnDisconnect
        }
    }
}

fn put_input_stream_worker_fact(
    mut b: wire::input_stream_worker_fact::Builder<'_>,
    fact: &InputStreamWorkerFact,
) {
    put_input_peer_eof_fact(b.reborrow().init_peer_eof(), &fact.peer_eof);
    put_worker_native_join_fact(b.reborrow().init_reader_join(), &fact.reader_join);
    b.set_termination_disposition(put_input_termination_fact(fact.termination_disposition));
}

fn get_input_stream_worker_fact(
    r: wire::input_stream_worker_fact::Reader<'_>,
) -> Result<InputStreamWorkerFact, NativeCloseCodecError> {
    if !r.has_peer_eof() || !r.has_reader_join() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(InputStreamWorkerFact {
        peer_eof: get_input_peer_eof_fact(r.get_peer_eof().map_err(map_capnp_err)?)?,
        reader_join: get_worker_native_join_fact(r.get_reader_join().map_err(map_capnp_err)?)?,
        termination_disposition: get_input_termination_fact(
            r.get_termination_disposition().map_err(map_not_in_schema)?,
        ),
    })
}

fn put_io_streams_and_workers_facts(
    mut b: wire::io_streams_and_workers_facts::Builder<'_>,
    facts: &IoStreamsAndWorkersFacts,
) {
    put_output_stream_worker_fact(b.reborrow().init_stdout(), &facts.stdout);
    put_stderr_stream_worker_fact(b.reborrow().init_stderr(), &facts.stderr);
    put_input_stream_worker_fact(b.init_input(), &facts.input);
}

fn get_io_streams_and_workers_facts(
    r: wire::io_streams_and_workers_facts::Reader<'_>,
) -> Result<IoStreamsAndWorkersFacts, NativeCloseCodecError> {
    if !r.has_stdout() || !r.has_stderr() || !r.has_input() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(IoStreamsAndWorkersFacts {
        stdout: get_output_stream_worker_fact(r.get_stdout().map_err(map_capnp_err)?)?,
        stderr: get_stderr_stream_worker_fact(r.get_stderr().map_err(map_capnp_err)?)?,
        input: get_input_stream_worker_fact(r.get_input().map_err(map_capnp_err)?)?,
    })
}

fn put_stdin_write_disposition(v: StdinWriteDisposition) -> wire::StdinWriteDisposition {
    match v {
        StdinWriteDisposition::AllWritesSucceeded => {
            wire::StdinWriteDisposition::AllWritesSucceeded
        }
        StdinWriteDisposition::WriteFailedBeforeClose => {
            wire::StdinWriteDisposition::WriteFailedBeforeClose
        }
    }
}

fn get_stdin_write_disposition(v: wire::StdinWriteDisposition) -> StdinWriteDisposition {
    match v {
        wire::StdinWriteDisposition::AllWritesSucceeded => {
            StdinWriteDisposition::AllWritesSucceeded
        }
        wire::StdinWriteDisposition::WriteFailedBeforeClose => {
            StdinWriteDisposition::WriteFailedBeforeClose
        }
    }
}

fn put_stdin_handle_close_fact(
    mut b: wire::stdin_handle_close_fact::Builder<'_>,
    fact: &StdinHandleCloseFact,
) {
    match fact {
        StdinHandleCloseFact::NotObserved => b.set_not_observed(()),
        StdinHandleCloseFact::NotProven => b.set_not_proven(()),
        StdinHandleCloseFact::InitiallyNotOpen => b.set_initially_not_open(()),
        StdinHandleCloseFact::ClosedOnceVerified { write_disposition } => {
            b.set_closed_once_verified(put_stdin_write_disposition(*write_disposition));
        }
        StdinHandleCloseFact::StillOpen => b.set_still_open(()),
        StdinHandleCloseFact::ClosedMultipleTimes { close_count } => {
            b.set_closed_multiple_times(*close_count);
        }
        StdinHandleCloseFact::CloseDebtRetained {
            close_count,
            os_error,
        } => {
            let mut d = b.init_close_debt_retained();
            d.set_close_count(*close_count);
            put_maybe_i32(d.init_os_error(), *os_error);
        }
    }
}

fn get_stdin_handle_close_fact(
    r: wire::stdin_handle_close_fact::Reader<'_>,
) -> Result<StdinHandleCloseFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::stdin_handle_close_fact::NotObserved(()) => StdinHandleCloseFact::NotObserved,
        wire::stdin_handle_close_fact::NotProven(()) => StdinHandleCloseFact::NotProven,
        wire::stdin_handle_close_fact::InitiallyNotOpen(()) => {
            StdinHandleCloseFact::InitiallyNotOpen
        }
        wire::stdin_handle_close_fact::ClosedOnceVerified(wd) => {
            StdinHandleCloseFact::ClosedOnceVerified {
                write_disposition: get_stdin_write_disposition(wd.map_err(map_not_in_schema)?),
            }
        }
        wire::stdin_handle_close_fact::StillOpen(()) => StdinHandleCloseFact::StillOpen,
        wire::stdin_handle_close_fact::ClosedMultipleTimes(close_count) => {
            StdinHandleCloseFact::ClosedMultipleTimes { close_count }
        }
        wire::stdin_handle_close_fact::CloseDebtRetained(d) => {
            let d = d.map_err(map_capnp_err)?;
            if !d.has_os_error() {
                return Err(NativeCloseCodecError::MalformedCapnp(
                    MalformedCapnpKind::InvalidPointer,
                ));
            }
            StdinHandleCloseFact::CloseDebtRetained {
                close_count: d.get_close_count(),
                os_error: get_maybe_i32(d.get_os_error().map_err(map_capnp_err)?)?,
            }
        }
    })
}

fn put_conpty_closure_fact(v: ConptyClosureFact) -> wire::ConptyClosureFact {
    match v {
        ConptyClosureFact::NotObserved => wire::ConptyClosureFact::NotObserved,
        ConptyClosureFact::NotProven => wire::ConptyClosureFact::NotProven,
        ConptyClosureFact::NotUsedPipeMode => wire::ConptyClosureFact::NotUsedPipeMode,
        ConptyClosureFact::ClosedBeforeOutputJoinVerified => {
            wire::ConptyClosureFact::ClosedBeforeOutputJoinVerified
        }
        ConptyClosureFact::StillActiveOrPoisoned => wire::ConptyClosureFact::StillActiveOrPoisoned,
    }
}

fn get_conpty_closure_fact(v: wire::ConptyClosureFact) -> ConptyClosureFact {
    match v {
        wire::ConptyClosureFact::NotObserved => ConptyClosureFact::NotObserved,
        wire::ConptyClosureFact::NotProven => ConptyClosureFact::NotProven,
        wire::ConptyClosureFact::NotUsedPipeMode => ConptyClosureFact::NotUsedPipeMode,
        wire::ConptyClosureFact::ClosedBeforeOutputJoinVerified => {
            ConptyClosureFact::ClosedBeforeOutputJoinVerified
        }
        wire::ConptyClosureFact::StillActiveOrPoisoned => ConptyClosureFact::StillActiveOrPoisoned,
    }
}

fn put_original_handles_close_facts(
    mut b: wire::original_handles_close_facts::Builder<'_>,
    facts: &OriginalHandlesCloseFacts,
) {
    b.set_conpty_closure(put_conpty_closure_fact(facts.conpty_closure));
    put_stdin_handle_close_fact(b.reborrow().init_stdin_closure(), &facts.stdin_closure);
    put_handle_close_fact(
        b.reborrow().init_child_primary_thread_handle(),
        &facts.child_primary_thread_handle,
    );
    put_handle_close_fact(
        b.reborrow().init_child_process_handle(),
        &facts.child_process_handle,
    );
    b.set_close_ordering_after_io_joins(put_stage_ordering_fact(
        facts.close_ordering_after_io_joins,
    ));
}

fn get_original_handles_close_facts(
    r: wire::original_handles_close_facts::Reader<'_>,
) -> Result<OriginalHandlesCloseFacts, NativeCloseCodecError> {
    if !r.has_stdin_closure()
        || !r.has_child_primary_thread_handle()
        || !r.has_child_process_handle()
    {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(OriginalHandlesCloseFacts {
        conpty_closure: get_conpty_closure_fact(r.get_conpty_closure().map_err(map_not_in_schema)?),
        stdin_closure: get_stdin_handle_close_fact(r.get_stdin_closure().map_err(map_capnp_err)?)?,
        child_primary_thread_handle: get_handle_close_fact(
            r.get_child_primary_thread_handle().map_err(map_capnp_err)?,
        )?,
        child_process_handle: get_handle_close_fact(
            r.get_child_process_handle().map_err(map_capnp_err)?,
        )?,
        close_ordering_after_io_joins: get_stage_ordering_fact(
            r.get_close_ordering_after_io_joins()
                .map_err(map_not_in_schema)?,
        ),
    })
}

fn put_job_admission_fact(mut b: wire::job_admission_fact::Builder<'_>, fact: &JobAdmissionFact) {
    match fact {
        JobAdmissionFact::NotObserved => b.set_not_observed(()),
        JobAdmissionFact::NotProven => b.set_not_proven(()),
        JobAdmissionFact::JobMissing => b.set_job_missing(()),
        JobAdmissionFact::AdmittedVerifiedOriginalJob {
            job_instance_id,
            child_membership_verified,
        } => {
            let mut a = b.init_admitted_verified_original_job();
            a.set_job_instance_id(*job_instance_id);
            a.set_child_membership_verified(put_observation_proof(*child_membership_verified));
        }
    }
}

fn get_job_admission_fact(
    r: wire::job_admission_fact::Reader<'_>,
) -> Result<JobAdmissionFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::job_admission_fact::NotObserved(()) => JobAdmissionFact::NotObserved,
        wire::job_admission_fact::NotProven(()) => JobAdmissionFact::NotProven,
        wire::job_admission_fact::JobMissing(()) => JobAdmissionFact::JobMissing,
        wire::job_admission_fact::AdmittedVerifiedOriginalJob(a) => {
            let a = a.map_err(map_capnp_err)?;
            JobAdmissionFact::AdmittedVerifiedOriginalJob {
                job_instance_id: a.get_job_instance_id(),
                child_membership_verified: get_observation_proof(
                    a.get_child_membership_verified()
                        .map_err(map_not_in_schema)?,
                ),
            }
        }
    })
}

fn put_job_active_zero_drain_fact(
    mut b: wire::job_active_zero_drain_fact::Builder<'_>,
    fact: &JobActiveZeroDrainFact,
) {
    match fact {
        JobActiveZeroDrainFact::NotObserved => b.set_not_observed(()),
        JobActiveZeroDrainFact::NotProven => b.set_not_proven(()),
        JobActiveZeroDrainFact::DrainedToActiveZero {
            total_processes,
            active_processes,
            total_terminated_processes,
            ordering_after_child_handle_close,
        } => {
            let mut d = b.init_drained_to_active_zero();
            d.set_total_processes(*total_processes);
            d.set_active_processes(*active_processes);
            d.set_total_terminated_processes(*total_terminated_processes);
            d.set_ordering_after_child_handle_close(put_stage_ordering_fact(
                *ordering_after_child_handle_close,
            ));
        }
        JobActiveZeroDrainFact::DrainTimedOut { active_processes } => {
            b.set_drain_timed_out(*active_processes);
        }
        JobActiveZeroDrainFact::DrainFailed { os_error } => {
            put_maybe_i32(b.init_drain_failed(), *os_error);
        }
    }
}

fn get_job_active_zero_drain_fact(
    r: wire::job_active_zero_drain_fact::Reader<'_>,
) -> Result<JobActiveZeroDrainFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::job_active_zero_drain_fact::NotObserved(()) => JobActiveZeroDrainFact::NotObserved,
        wire::job_active_zero_drain_fact::NotProven(()) => JobActiveZeroDrainFact::NotProven,
        wire::job_active_zero_drain_fact::DrainedToActiveZero(d) => {
            let d = d.map_err(map_capnp_err)?;
            JobActiveZeroDrainFact::DrainedToActiveZero {
                total_processes: d.get_total_processes(),
                active_processes: d.get_active_processes(),
                total_terminated_processes: d.get_total_terminated_processes(),
                ordering_after_child_handle_close: get_stage_ordering_fact(
                    d.get_ordering_after_child_handle_close()
                        .map_err(map_not_in_schema)?,
                ),
            }
        }
        wire::job_active_zero_drain_fact::DrainTimedOut(active_processes) => {
            JobActiveZeroDrainFact::DrainTimedOut { active_processes }
        }
        wire::job_active_zero_drain_fact::DrainFailed(err) => JobActiveZeroDrainFact::DrainFailed {
            os_error: get_maybe_i32(err.map_err(map_capnp_err)?)?,
        },
    })
}

fn put_job_forbidden_op_fact(v: JobForbiddenOpFact) -> wire::JobForbiddenOpFact {
    match v {
        JobForbiddenOpFact::NotInvoked => wire::JobForbiddenOpFact::NotInvoked,
        JobForbiddenOpFact::InvokedViolation => wire::JobForbiddenOpFact::InvokedViolation,
    }
}

fn get_job_forbidden_op_fact(v: wire::JobForbiddenOpFact) -> JobForbiddenOpFact {
    match v {
        wire::JobForbiddenOpFact::NotInvoked => JobForbiddenOpFact::NotInvoked,
        wire::JobForbiddenOpFact::InvokedViolation => JobForbiddenOpFact::InvokedViolation,
    }
}

fn put_extra_job_members_origin_fact(
    v: ExtraJobMembersOriginFact,
) -> wire::ExtraJobMembersOriginFact {
    match v {
        ExtraJobMembersOriginFact::NotObserved => wire::ExtraJobMembersOriginFact::NotObserved,
        ExtraJobMembersOriginFact::NotProven => wire::ExtraJobMembersOriginFact::NotProven,
        ExtraJobMembersOriginFact::SingleProcessObservedOnly => {
            wire::ExtraJobMembersOriginFact::SingleProcessObservedOnly
        }
        ExtraJobMembersOriginFact::ExtraMembersObservedOriginNotProvenByJobHelper => {
            wire::ExtraJobMembersOriginFact::ExtraMembersObservedOriginNotProvenByJobHelper
        }
    }
}

fn get_extra_job_members_origin_fact(
    v: wire::ExtraJobMembersOriginFact,
) -> ExtraJobMembersOriginFact {
    match v {
        wire::ExtraJobMembersOriginFact::NotObserved => ExtraJobMembersOriginFact::NotObserved,
        wire::ExtraJobMembersOriginFact::NotProven => ExtraJobMembersOriginFact::NotProven,
        wire::ExtraJobMembersOriginFact::SingleProcessObservedOnly => {
            ExtraJobMembersOriginFact::SingleProcessObservedOnly
        }
        wire::ExtraJobMembersOriginFact::ExtraMembersObservedOriginNotProvenByJobHelper => {
            ExtraJobMembersOriginFact::ExtraMembersObservedOriginNotProvenByJobHelper
        }
    }
}

fn put_original_job_lifecycle_facts(
    mut b: wire::original_job_lifecycle_facts::Builder<'_>,
    facts: &OriginalJobLifecycleFacts,
) {
    put_job_admission_fact(b.reborrow().init_admission(), &facts.admission);
    put_job_active_zero_drain_fact(
        b.reborrow().init_active_zero_drain(),
        &facts.active_zero_drain,
    );
    b.set_extra_member_origin(put_extra_job_members_origin_fact(facts.extra_member_origin));
    b.set_preserve_descendants(put_job_forbidden_op_fact(facts.preserve_descendants));
    b.set_terminate_job(put_job_forbidden_op_fact(facts.terminate_job));
}

fn get_original_job_lifecycle_facts(
    r: wire::original_job_lifecycle_facts::Reader<'_>,
) -> Result<OriginalJobLifecycleFacts, NativeCloseCodecError> {
    if !r.has_admission() || !r.has_active_zero_drain() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(OriginalJobLifecycleFacts {
        admission: get_job_admission_fact(r.get_admission().map_err(map_capnp_err)?)?,
        active_zero_drain: get_job_active_zero_drain_fact(
            r.get_active_zero_drain().map_err(map_capnp_err)?,
        )?,
        extra_member_origin: get_extra_job_members_origin_fact(
            r.get_extra_member_origin().map_err(map_not_in_schema)?,
        ),
        preserve_descendants: get_job_forbidden_op_fact(
            r.get_preserve_descendants().map_err(map_not_in_schema)?,
        ),
        terminate_job: get_job_forbidden_op_fact(r.get_terminate_job().map_err(map_not_in_schema)?),
    })
}

fn put_business_exit_writer_fact(
    mut b: wire::business_exit_writer_fact::Builder<'_>,
    fact: &BusinessExitWriterFact,
) {
    match fact {
        BusinessExitWriterFact::NotObserved => b.set_not_observed(()),
        BusinessExitWriterFact::NotProven => b.set_not_proven(()),
        BusinessExitWriterFact::JoinedTimelyBeforeReceipt { exit_code_i32 } => {
            b.set_joined_timely_before_receipt(*exit_code_i32);
        }
        BusinessExitWriterFact::StillRunning => b.set_still_running(()),
        BusinessExitWriterFact::WriteOrJoinFailed => b.set_write_or_join_failed(()),
        BusinessExitWriterFact::Panicked => b.set_panicked(()),
    }
}

fn get_business_exit_writer_fact(
    r: wire::business_exit_writer_fact::Reader<'_>,
) -> Result<BusinessExitWriterFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::business_exit_writer_fact::NotObserved(()) => BusinessExitWriterFact::NotObserved,
        wire::business_exit_writer_fact::NotProven(()) => BusinessExitWriterFact::NotProven,
        wire::business_exit_writer_fact::JoinedTimelyBeforeReceipt(exit_code_i32) => {
            BusinessExitWriterFact::JoinedTimelyBeforeReceipt { exit_code_i32 }
        }
        wire::business_exit_writer_fact::StillRunning(()) => BusinessExitWriterFact::StillRunning,
        wire::business_exit_writer_fact::WriteOrJoinFailed(()) => {
            BusinessExitWriterFact::WriteOrJoinFailed
        }
        wire::business_exit_writer_fact::Panicked(()) => BusinessExitWriterFact::Panicked,
    })
}

fn put_prior_control_writers_fact(
    mut b: wire::prior_control_writers_fact::Builder<'_>,
    fact: &PriorControlWritersFact,
) {
    match fact {
        PriorControlWritersFact::NotObserved => b.set_not_observed(()),
        PriorControlWritersFact::NotProven => b.set_not_proven(()),
        PriorControlWritersFact::AllJoinedAndQuiesced {
            control_results_written,
        } => {
            b.set_all_joined_and_quiesced(*control_results_written);
        }
        PriorControlWritersFact::WriterStillRunning => b.set_writer_still_running(()),
        PriorControlWritersFact::ControlWriteFailed => b.set_control_write_failed(()),
        PriorControlWritersFact::PrecontextWriterRetained => b.set_precontext_writer_retained(()),
    }
}

fn get_prior_control_writers_fact(
    r: wire::prior_control_writers_fact::Reader<'_>,
) -> Result<PriorControlWritersFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::prior_control_writers_fact::NotObserved(()) => PriorControlWritersFact::NotObserved,
        wire::prior_control_writers_fact::NotProven(()) => PriorControlWritersFact::NotProven,
        wire::prior_control_writers_fact::AllJoinedAndQuiesced(control_results_written) => {
            PriorControlWritersFact::AllJoinedAndQuiesced {
                control_results_written,
            }
        }
        wire::prior_control_writers_fact::WriterStillRunning(()) => {
            PriorControlWritersFact::WriterStillRunning
        }
        wire::prior_control_writers_fact::ControlWriteFailed(()) => {
            PriorControlWritersFact::ControlWriteFailed
        }
        wire::prior_control_writers_fact::PrecontextWriterRetained(()) => {
            PriorControlWritersFact::PrecontextWriterRetained
        }
    })
}

fn put_sender_receipt_writer_fact(v: SenderReceiptWriterFact) -> wire::SenderReceiptWriterFact {
    match v {
        SenderReceiptWriterFact::NotObserved => wire::SenderReceiptWriterFact::NotObserved,
        SenderReceiptWriterFact::NotProven => wire::SenderReceiptWriterFact::NotProven,
        SenderReceiptWriterFact::PendingCurrentReceiptFlushAndClose => {
            wire::SenderReceiptWriterFact::PendingCurrentReceiptFlushAndClose
        }
        SenderReceiptWriterFact::ClaimedJoinedBeforeSend => {
            wire::SenderReceiptWriterFact::ClaimedJoinedBeforeSend
        }
        SenderReceiptWriterFact::WriteFailed => wire::SenderReceiptWriterFact::WriteFailed,
    }
}

fn get_sender_receipt_writer_fact(v: wire::SenderReceiptWriterFact) -> SenderReceiptWriterFact {
    match v {
        wire::SenderReceiptWriterFact::NotObserved => SenderReceiptWriterFact::NotObserved,
        wire::SenderReceiptWriterFact::NotProven => SenderReceiptWriterFact::NotProven,
        wire::SenderReceiptWriterFact::PendingCurrentReceiptFlushAndClose => {
            SenderReceiptWriterFact::PendingCurrentReceiptFlushAndClose
        }
        wire::SenderReceiptWriterFact::ClaimedJoinedBeforeSend => {
            SenderReceiptWriterFact::ClaimedJoinedBeforeSend
        }
        wire::SenderReceiptWriterFact::WriteFailed => SenderReceiptWriterFact::WriteFailed,
    }
}

fn put_writer_closure_facts(
    mut b: wire::writer_closure_facts::Builder<'_>,
    facts: &WriterClosureFacts,
) {
    put_business_exit_writer_fact(
        b.reborrow().init_business_exit_writer(),
        &facts.business_exit_writer,
    );
    put_prior_control_writers_fact(
        b.reborrow().init_prior_control_and_frame_writers(),
        &facts.prior_control_and_frame_writers,
    );
    b.set_sender_receipt_writer(put_sender_receipt_writer_fact(facts.sender_receipt_writer));
}

fn get_writer_closure_facts(
    r: wire::writer_closure_facts::Reader<'_>,
) -> Result<WriterClosureFacts, NativeCloseCodecError> {
    if !r.has_business_exit_writer() || !r.has_prior_control_and_frame_writers() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(WriterClosureFacts {
        business_exit_writer: get_business_exit_writer_fact(
            r.get_business_exit_writer().map_err(map_capnp_err)?,
        )?,
        prior_control_and_frame_writers: get_prior_control_writers_fact(
            r.get_prior_control_and_frame_writers()
                .map_err(map_capnp_err)?,
        )?,
        sender_receipt_writer: get_sender_receipt_writer_fact(
            r.get_sender_receipt_writer().map_err(map_not_in_schema)?,
        ),
    })
}

fn put_lease_retirement_fact(v: LeaseRetirementFact) -> wire::LeaseRetirementFact {
    match v {
        LeaseRetirementFact::NotObserved => wire::LeaseRetirementFact::NotObserved,
        LeaseRetirementFact::NotProven => wire::LeaseRetirementFact::NotProven,
        LeaseRetirementFact::ActiveOrLoaned => wire::LeaseRetirementFact::ActiveOrLoaned,
        LeaseRetirementFact::ParkedUnresolved => wire::LeaseRetirementFact::ParkedUnresolved,
        LeaseRetirementFact::OutOfRunnerScopePendingHostOwner => {
            wire::LeaseRetirementFact::OutOfRunnerScopePendingHostOwner
        }
        LeaseRetirementFact::RetiredCleanly => wire::LeaseRetirementFact::RetiredCleanly,
    }
}

fn get_lease_retirement_fact(v: wire::LeaseRetirementFact) -> LeaseRetirementFact {
    match v {
        wire::LeaseRetirementFact::NotObserved => LeaseRetirementFact::NotObserved,
        wire::LeaseRetirementFact::NotProven => LeaseRetirementFact::NotProven,
        wire::LeaseRetirementFact::ActiveOrLoaned => LeaseRetirementFact::ActiveOrLoaned,
        wire::LeaseRetirementFact::ParkedUnresolved => LeaseRetirementFact::ParkedUnresolved,
        wire::LeaseRetirementFact::OutOfRunnerScopePendingHostOwner => {
            LeaseRetirementFact::OutOfRunnerScopePendingHostOwner
        }
        wire::LeaseRetirementFact::RetiredCleanly => LeaseRetirementFact::RetiredCleanly,
    }
}

fn put_attempt_lease_retirement_fact(
    mut b: wire::attempt_lease_retirement_fact::Builder<'_>,
    fact: &AttemptLeaseRetirementFact,
) {
    match fact {
        AttemptLeaseRetirementFact::NotObserved => b.set_not_observed(()),
        AttemptLeaseRetirementFact::NotProven => b.set_not_proven(()),
        AttemptLeaseRetirementFact::ActiveOrLoaned => b.set_active_or_loaned(()),
        AttemptLeaseRetirementFact::ParkedUnresolved => b.set_parked_unresolved(()),
        AttemptLeaseRetirementFact::ReadyToRetireOnFinalReceiptFlush { startup_attempt_id } => {
            b.set_ready_to_retire_on_final_receipt_flush(*startup_attempt_id);
        }
    }
}

fn get_attempt_lease_retirement_fact(
    r: wire::attempt_lease_retirement_fact::Reader<'_>,
) -> Result<AttemptLeaseRetirementFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::attempt_lease_retirement_fact::NotObserved(()) => {
            AttemptLeaseRetirementFact::NotObserved
        }
        wire::attempt_lease_retirement_fact::NotProven(()) => AttemptLeaseRetirementFact::NotProven,
        wire::attempt_lease_retirement_fact::ActiveOrLoaned(()) => {
            AttemptLeaseRetirementFact::ActiveOrLoaned
        }
        wire::attempt_lease_retirement_fact::ParkedUnresolved(()) => {
            AttemptLeaseRetirementFact::ParkedUnresolved
        }
        wire::attempt_lease_retirement_fact::ReadyToRetireOnFinalReceiptFlush(
            startup_attempt_id,
        ) => AttemptLeaseRetirementFact::ReadyToRetireOnFinalReceiptFlush { startup_attempt_id },
    })
}

fn put_lease_retirement_facts(
    mut b: wire::lease_retirement_facts::Builder<'_>,
    facts: &LeaseRetirementFacts,
) {
    b.set_runner_input_and_stdin_lease(put_lease_retirement_fact(
        facts.runner_input_and_stdin_lease,
    ));
    b.set_input_lease(put_lease_retirement_fact(facts.input_lease));
    b.set_keeper_lease(put_lease_retirement_fact(facts.keeper_lease));
    b.set_quota_lease(put_lease_retirement_fact(facts.quota_lease));
    put_attempt_lease_retirement_fact(b.init_attempt_lease(), &facts.attempt_lease);
}

fn get_lease_retirement_facts(
    r: wire::lease_retirement_facts::Reader<'_>,
) -> Result<LeaseRetirementFacts, NativeCloseCodecError> {
    if !r.has_attempt_lease() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(LeaseRetirementFacts {
        runner_input_and_stdin_lease: get_lease_retirement_fact(
            r.get_runner_input_and_stdin_lease()
                .map_err(map_not_in_schema)?,
        ),
        input_lease: get_lease_retirement_fact(r.get_input_lease().map_err(map_not_in_schema)?),
        keeper_lease: get_lease_retirement_fact(r.get_keeper_lease().map_err(map_not_in_schema)?),
        quota_lease: get_lease_retirement_fact(r.get_quota_lease().map_err(map_not_in_schema)?),
        attempt_lease: get_attempt_lease_retirement_fact(
            r.get_attempt_lease().map_err(map_capnp_err)?,
        )?,
    })
}

fn put_close_debt_target(v: CloseDebtTarget) -> wire::CloseDebtTarget {
    match v {
        CloseDebtTarget::HostRunnerProcessHandle => wire::CloseDebtTarget::HostRunnerProcessHandle,
        CloseDebtTarget::HostInputPipeWriter => wire::CloseDebtTarget::HostInputPipeWriter,
        CloseDebtTarget::HostOutputPipeReader => wire::CloseDebtTarget::HostOutputPipeReader,
        CloseDebtTarget::HostStartupPipeAlias => wire::CloseDebtTarget::HostStartupPipeAlias,
        CloseDebtTarget::HostWorkerThreadAnchor => wire::CloseDebtTarget::HostWorkerThreadAnchor,
        CloseDebtTarget::HostDesktopHandle => wire::CloseDebtTarget::HostDesktopHandle,
        CloseDebtTarget::HostDesktopToken => wire::CloseDebtTarget::HostDesktopToken,
        CloseDebtTarget::HostDesktopSecurityDescriptor => {
            wire::CloseDebtTarget::HostDesktopSecurityDescriptor
        }
        CloseDebtTarget::ChildProcessHandle => wire::CloseDebtTarget::ChildProcessHandle,
        CloseDebtTarget::ChildPrimaryThreadHandle => {
            wire::CloseDebtTarget::ChildPrimaryThreadHandle
        }
        CloseDebtTarget::ChildStdoutHandle => wire::CloseDebtTarget::ChildStdoutHandle,
        CloseDebtTarget::ChildStderrHandle => wire::CloseDebtTarget::ChildStderrHandle,
        CloseDebtTarget::ChildStdinHandle => wire::CloseDebtTarget::ChildStdinHandle,
        CloseDebtTarget::SpawnPipeAuxiliaryHandle => {
            wire::CloseDebtTarget::SpawnPipeAuxiliaryHandle
        }
    }
}

fn get_close_debt_target(v: wire::CloseDebtTarget) -> CloseDebtTarget {
    match v {
        wire::CloseDebtTarget::HostRunnerProcessHandle => CloseDebtTarget::HostRunnerProcessHandle,
        wire::CloseDebtTarget::HostInputPipeWriter => CloseDebtTarget::HostInputPipeWriter,
        wire::CloseDebtTarget::HostOutputPipeReader => CloseDebtTarget::HostOutputPipeReader,
        wire::CloseDebtTarget::HostStartupPipeAlias => CloseDebtTarget::HostStartupPipeAlias,
        wire::CloseDebtTarget::HostWorkerThreadAnchor => CloseDebtTarget::HostWorkerThreadAnchor,
        wire::CloseDebtTarget::HostDesktopHandle => CloseDebtTarget::HostDesktopHandle,
        wire::CloseDebtTarget::HostDesktopToken => CloseDebtTarget::HostDesktopToken,
        wire::CloseDebtTarget::HostDesktopSecurityDescriptor => {
            CloseDebtTarget::HostDesktopSecurityDescriptor
        }
        wire::CloseDebtTarget::ChildProcessHandle => CloseDebtTarget::ChildProcessHandle,
        wire::CloseDebtTarget::ChildPrimaryThreadHandle => {
            CloseDebtTarget::ChildPrimaryThreadHandle
        }
        wire::CloseDebtTarget::ChildStdoutHandle => CloseDebtTarget::ChildStdoutHandle,
        wire::CloseDebtTarget::ChildStderrHandle => CloseDebtTarget::ChildStderrHandle,
        wire::CloseDebtTarget::ChildStdinHandle => CloseDebtTarget::ChildStdinHandle,
        wire::CloseDebtTarget::SpawnPipeAuxiliaryHandle => {
            CloseDebtTarget::SpawnPipeAuxiliaryHandle
        }
    }
}

fn put_close_debt_fact(mut b: wire::close_debt_fact::Builder<'_>, fact: &CloseDebtFact) {
    match fact {
        CloseDebtFact::NotObserved => b.set_not_observed(()),
        CloseDebtFact::NotProven => b.set_not_proven(()),
        CloseDebtFact::ZeroDebtProven => b.set_zero_debt_proven(()),
        CloseDebtFact::DebtRetained {
            debt_count,
            first_target,
            first_os_error,
        } => {
            let mut d = b.init_debt_retained();
            d.set_debt_count(*debt_count);
            d.set_first_target(put_close_debt_target(*first_target));
            put_maybe_i32(d.init_first_os_error(), *first_os_error);
        }
    }
}

fn get_close_debt_fact(
    r: wire::close_debt_fact::Reader<'_>,
) -> Result<CloseDebtFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::close_debt_fact::NotObserved(()) => CloseDebtFact::NotObserved,
        wire::close_debt_fact::NotProven(()) => CloseDebtFact::NotProven,
        wire::close_debt_fact::ZeroDebtProven(()) => CloseDebtFact::ZeroDebtProven,
        wire::close_debt_fact::DebtRetained(d) => {
            let d = d.map_err(map_capnp_err)?;
            if !d.has_first_os_error() {
                return Err(NativeCloseCodecError::MalformedCapnp(
                    MalformedCapnpKind::InvalidPointer,
                ));
            }
            CloseDebtFact::DebtRetained {
                debt_count: d.get_debt_count(),
                first_target: get_close_debt_target(
                    d.get_first_target().map_err(map_not_in_schema)?,
                ),
                first_os_error: get_maybe_i32(d.get_first_os_error().map_err(map_capnp_err)?)?,
            }
        }
    })
}

fn put_sticky_failure_kind(v: ProtocolStickyFailureKind) -> wire::ProtocolStickyFailureKind {
    match v {
        ProtocolStickyFailureKind::RuntimeTransportFailed => {
            wire::ProtocolStickyFailureKind::RuntimeTransportFailed
        }
        ProtocolStickyFailureKind::StartupValidationFailed => {
            wire::ProtocolStickyFailureKind::StartupValidationFailed
        }
        ProtocolStickyFailureKind::SpawnChildFailed => {
            wire::ProtocolStickyFailureKind::SpawnChildFailed
        }
        ProtocolStickyFailureKind::WriteSpawnReadyFailed => {
            wire::ProtocolStickyFailureKind::WriteSpawnReadyFailed
        }
        ProtocolStickyFailureKind::ChildWaitFailed => {
            wire::ProtocolStickyFailureKind::ChildWaitFailed
        }
        ProtocolStickyFailureKind::ChildExitCodeUnavailable => {
            wire::ProtocolStickyFailureKind::ChildExitCodeUnavailable
        }
        ProtocolStickyFailureKind::ChildTimedOut => wire::ProtocolStickyFailureKind::ChildTimedOut,
        ProtocolStickyFailureKind::ExplicitUserTerminate => {
            wire::ProtocolStickyFailureKind::ExplicitUserTerminate
        }
        ProtocolStickyFailureKind::ConptyLockPoisoned => {
            wire::ProtocolStickyFailureKind::ConptyLockPoisoned
        }
        ProtocolStickyFailureKind::StdoutForwardingFailed => {
            wire::ProtocolStickyFailureKind::StdoutForwardingFailed
        }
        ProtocolStickyFailureKind::StdoutJoinTimedOut => {
            wire::ProtocolStickyFailureKind::StdoutJoinTimedOut
        }
        ProtocolStickyFailureKind::StdoutWorkerPanicked => {
            wire::ProtocolStickyFailureKind::StdoutWorkerPanicked
        }
        ProtocolStickyFailureKind::StderrForwardingFailed => {
            wire::ProtocolStickyFailureKind::StderrForwardingFailed
        }
        ProtocolStickyFailureKind::StderrJoinTimedOut => {
            wire::ProtocolStickyFailureKind::StderrJoinTimedOut
        }
        ProtocolStickyFailureKind::StderrWorkerPanicked => {
            wire::ProtocolStickyFailureKind::StderrWorkerPanicked
        }
        ProtocolStickyFailureKind::BusinessExitGateViolation => {
            wire::ProtocolStickyFailureKind::BusinessExitGateViolation
        }
        ProtocolStickyFailureKind::BusinessExitWriteFailed => {
            wire::ProtocolStickyFailureKind::BusinessExitWriteFailed
        }
        ProtocolStickyFailureKind::InputPartialFrame => {
            wire::ProtocolStickyFailureKind::InputPartialFrame
        }
        ProtocolStickyFailureKind::InputFrameTooLarge => {
            wire::ProtocolStickyFailureKind::InputFrameTooLarge
        }
        ProtocolStickyFailureKind::InputMalformedFrame => {
            wire::ProtocolStickyFailureKind::InputMalformedFrame
        }
        ProtocolStickyFailureKind::InputProtocolViolation => {
            wire::ProtocolStickyFailureKind::InputProtocolViolation
        }
        ProtocolStickyFailureKind::InputIoError => wire::ProtocolStickyFailureKind::InputIoError,
        ProtocolStickyFailureKind::InputPrematureDisconnect => {
            wire::ProtocolStickyFailureKind::InputPrematureDisconnect
        }
        ProtocolStickyFailureKind::InputControlWriteFailed => {
            wire::ProtocolStickyFailureKind::InputControlWriteFailed
        }
        ProtocolStickyFailureKind::InputStdinCloseFailed => {
            wire::ProtocolStickyFailureKind::InputStdinCloseFailed
        }
        ProtocolStickyFailureKind::InputJoinTimedOut => {
            wire::ProtocolStickyFailureKind::InputJoinTimedOut
        }
        ProtocolStickyFailureKind::InputWorkerPanicked => {
            wire::ProtocolStickyFailureKind::InputWorkerPanicked
        }
        ProtocolStickyFailureKind::ChildHandleCloseFailed => {
            wire::ProtocolStickyFailureKind::ChildHandleCloseFailed
        }
        ProtocolStickyFailureKind::JobPreserveDescendantsInvoked => {
            wire::ProtocolStickyFailureKind::JobPreserveDescendantsInvoked
        }
        ProtocolStickyFailureKind::JobTerminateInvoked => {
            wire::ProtocolStickyFailureKind::JobTerminateInvoked
        }
        ProtocolStickyFailureKind::JobDrainFailed => {
            wire::ProtocolStickyFailureKind::JobDrainFailed
        }
        ProtocolStickyFailureKind::JobDrainTimedOut => {
            wire::ProtocolStickyFailureKind::JobDrainTimedOut
        }
        ProtocolStickyFailureKind::OwnerMutexPoisoned => {
            wire::ProtocolStickyFailureKind::OwnerMutexPoisoned
        }
    }
}

fn get_sticky_failure_kind(v: wire::ProtocolStickyFailureKind) -> ProtocolStickyFailureKind {
    match v {
        wire::ProtocolStickyFailureKind::RuntimeTransportFailed => {
            ProtocolStickyFailureKind::RuntimeTransportFailed
        }
        wire::ProtocolStickyFailureKind::StartupValidationFailed => {
            ProtocolStickyFailureKind::StartupValidationFailed
        }
        wire::ProtocolStickyFailureKind::SpawnChildFailed => {
            ProtocolStickyFailureKind::SpawnChildFailed
        }
        wire::ProtocolStickyFailureKind::WriteSpawnReadyFailed => {
            ProtocolStickyFailureKind::WriteSpawnReadyFailed
        }
        wire::ProtocolStickyFailureKind::ChildWaitFailed => {
            ProtocolStickyFailureKind::ChildWaitFailed
        }
        wire::ProtocolStickyFailureKind::ChildExitCodeUnavailable => {
            ProtocolStickyFailureKind::ChildExitCodeUnavailable
        }
        wire::ProtocolStickyFailureKind::ChildTimedOut => ProtocolStickyFailureKind::ChildTimedOut,
        wire::ProtocolStickyFailureKind::ExplicitUserTerminate => {
            ProtocolStickyFailureKind::ExplicitUserTerminate
        }
        wire::ProtocolStickyFailureKind::ConptyLockPoisoned => {
            ProtocolStickyFailureKind::ConptyLockPoisoned
        }
        wire::ProtocolStickyFailureKind::StdoutForwardingFailed => {
            ProtocolStickyFailureKind::StdoutForwardingFailed
        }
        wire::ProtocolStickyFailureKind::StdoutJoinTimedOut => {
            ProtocolStickyFailureKind::StdoutJoinTimedOut
        }
        wire::ProtocolStickyFailureKind::StdoutWorkerPanicked => {
            ProtocolStickyFailureKind::StdoutWorkerPanicked
        }
        wire::ProtocolStickyFailureKind::StderrForwardingFailed => {
            ProtocolStickyFailureKind::StderrForwardingFailed
        }
        wire::ProtocolStickyFailureKind::StderrJoinTimedOut => {
            ProtocolStickyFailureKind::StderrJoinTimedOut
        }
        wire::ProtocolStickyFailureKind::StderrWorkerPanicked => {
            ProtocolStickyFailureKind::StderrWorkerPanicked
        }
        wire::ProtocolStickyFailureKind::BusinessExitGateViolation => {
            ProtocolStickyFailureKind::BusinessExitGateViolation
        }
        wire::ProtocolStickyFailureKind::BusinessExitWriteFailed => {
            ProtocolStickyFailureKind::BusinessExitWriteFailed
        }
        wire::ProtocolStickyFailureKind::InputPartialFrame => {
            ProtocolStickyFailureKind::InputPartialFrame
        }
        wire::ProtocolStickyFailureKind::InputFrameTooLarge => {
            ProtocolStickyFailureKind::InputFrameTooLarge
        }
        wire::ProtocolStickyFailureKind::InputMalformedFrame => {
            ProtocolStickyFailureKind::InputMalformedFrame
        }
        wire::ProtocolStickyFailureKind::InputProtocolViolation => {
            ProtocolStickyFailureKind::InputProtocolViolation
        }
        wire::ProtocolStickyFailureKind::InputIoError => ProtocolStickyFailureKind::InputIoError,
        wire::ProtocolStickyFailureKind::InputPrematureDisconnect => {
            ProtocolStickyFailureKind::InputPrematureDisconnect
        }
        wire::ProtocolStickyFailureKind::InputControlWriteFailed => {
            ProtocolStickyFailureKind::InputControlWriteFailed
        }
        wire::ProtocolStickyFailureKind::InputStdinCloseFailed => {
            ProtocolStickyFailureKind::InputStdinCloseFailed
        }
        wire::ProtocolStickyFailureKind::InputJoinTimedOut => {
            ProtocolStickyFailureKind::InputJoinTimedOut
        }
        wire::ProtocolStickyFailureKind::InputWorkerPanicked => {
            ProtocolStickyFailureKind::InputWorkerPanicked
        }
        wire::ProtocolStickyFailureKind::ChildHandleCloseFailed => {
            ProtocolStickyFailureKind::ChildHandleCloseFailed
        }
        wire::ProtocolStickyFailureKind::JobPreserveDescendantsInvoked => {
            ProtocolStickyFailureKind::JobPreserveDescendantsInvoked
        }
        wire::ProtocolStickyFailureKind::JobTerminateInvoked => {
            ProtocolStickyFailureKind::JobTerminateInvoked
        }
        wire::ProtocolStickyFailureKind::JobDrainFailed => {
            ProtocolStickyFailureKind::JobDrainFailed
        }
        wire::ProtocolStickyFailureKind::JobDrainTimedOut => {
            ProtocolStickyFailureKind::JobDrainTimedOut
        }
        wire::ProtocolStickyFailureKind::OwnerMutexPoisoned => {
            ProtocolStickyFailureKind::OwnerMutexPoisoned
        }
    }
}

fn put_failure_stage(v: ProtocolFailureStage) -> wire::ProtocolFailureStage {
    match v {
        ProtocolFailureStage::StartupAndHandshake => {
            wire::ProtocolFailureStage::StartupAndHandshake
        }
        ProtocolFailureStage::ChildWait => wire::ProtocolFailureStage::ChildWait,
        ProtocolFailureStage::OutputDrainAndJoin => wire::ProtocolFailureStage::OutputDrainAndJoin,
        ProtocolFailureStage::BusinessExitWrite => wire::ProtocolFailureStage::BusinessExitWrite,
        ProtocolFailureStage::InputDrainAndJoin => wire::ProtocolFailureStage::InputDrainAndJoin,
        ProtocolFailureStage::HandleClosure => wire::ProtocolFailureStage::HandleClosure,
        ProtocolFailureStage::JobActiveZeroDrain => wire::ProtocolFailureStage::JobActiveZeroDrain,
        ProtocolFailureStage::LeaseRetirement => wire::ProtocolFailureStage::LeaseRetirement,
    }
}

fn get_failure_stage(v: wire::ProtocolFailureStage) -> ProtocolFailureStage {
    match v {
        wire::ProtocolFailureStage::StartupAndHandshake => {
            ProtocolFailureStage::StartupAndHandshake
        }
        wire::ProtocolFailureStage::ChildWait => ProtocolFailureStage::ChildWait,
        wire::ProtocolFailureStage::OutputDrainAndJoin => ProtocolFailureStage::OutputDrainAndJoin,
        wire::ProtocolFailureStage::BusinessExitWrite => ProtocolFailureStage::BusinessExitWrite,
        wire::ProtocolFailureStage::InputDrainAndJoin => ProtocolFailureStage::InputDrainAndJoin,
        wire::ProtocolFailureStage::HandleClosure => ProtocolFailureStage::HandleClosure,
        wire::ProtocolFailureStage::JobActiveZeroDrain => ProtocolFailureStage::JobActiveZeroDrain,
        wire::ProtocolFailureStage::LeaseRetirement => ProtocolFailureStage::LeaseRetirement,
    }
}

fn put_replay_disposition(v: ReplayDisposition) -> wire::ReplayDisposition {
    match v {
        ReplayDisposition::NoReplaySealed => wire::ReplayDisposition::NoReplaySealed,
        ReplayDisposition::UnsealedInvalid => wire::ReplayDisposition::UnsealedInvalid,
    }
}

fn get_replay_disposition(v: wire::ReplayDisposition) -> ReplayDisposition {
    match v {
        wire::ReplayDisposition::NoReplaySealed => ReplayDisposition::NoReplaySealed,
        wire::ReplayDisposition::UnsealedInvalid => ReplayDisposition::UnsealedInvalid,
    }
}

fn put_first_error_fact(mut b: wire::first_error_fact::Builder<'_>, fact: &FirstErrorFact) {
    match fact {
        FirstErrorFact::NotObserved => b.set_not_observed(()),
        FirstErrorFact::NoneObserved => b.set_none_observed(()),
        FirstErrorFact::Recorded {
            kind,
            stage,
            os_error,
            replay_policy,
        } => {
            let mut rec = b.init_recorded();
            rec.set_kind(put_sticky_failure_kind(*kind));
            rec.set_stage(put_failure_stage(*stage));
            put_maybe_i32(rec.reborrow().init_os_error(), *os_error);
            rec.set_replay_policy(put_replay_disposition(*replay_policy));
        }
    }
}

fn get_first_error_fact(
    r: wire::first_error_fact::Reader<'_>,
) -> Result<FirstErrorFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::first_error_fact::NotObserved(()) => FirstErrorFact::NotObserved,
        wire::first_error_fact::NoneObserved(()) => FirstErrorFact::NoneObserved,
        wire::first_error_fact::Recorded(rec) => {
            let rec = rec.map_err(map_capnp_err)?;
            if !rec.has_os_error() {
                return Err(NativeCloseCodecError::MalformedCapnp(
                    MalformedCapnpKind::InvalidPointer,
                ));
            }
            FirstErrorFact::Recorded {
                kind: get_sticky_failure_kind(rec.get_kind().map_err(map_not_in_schema)?),
                stage: get_failure_stage(rec.get_stage().map_err(map_not_in_schema)?),
                os_error: get_maybe_i32(rec.get_os_error().map_err(map_capnp_err)?)?,
                replay_policy: get_replay_disposition(
                    rec.get_replay_policy().map_err(map_not_in_schema)?,
                ),
            }
        }
    })
}

fn put_poison_target(v: PoisonTarget) -> wire::PoisonTarget {
    match v {
        PoisonTarget::ChildProcessOwnerMutex => wire::PoisonTarget::ChildProcessOwnerMutex,
        PoisonTarget::ConptyOwnerMutex => wire::PoisonTarget::ConptyOwnerMutex,
        PoisonTarget::OutputPipeWriterMutex => wire::PoisonTarget::OutputPipeWriterMutex,
        PoisonTarget::StdinCloseDebtMutex => wire::PoisonTarget::StdinCloseDebtMutex,
        PoisonTarget::StickyLatchMutex => wire::PoisonTarget::StickyLatchMutex,
        PoisonTarget::AttemptSlotMutex => wire::PoisonTarget::AttemptSlotMutex,
        PoisonTarget::HostOwnerMutex => wire::PoisonTarget::HostOwnerMutex,
    }
}

fn get_poison_target(v: wire::PoisonTarget) -> PoisonTarget {
    match v {
        wire::PoisonTarget::ChildProcessOwnerMutex => PoisonTarget::ChildProcessOwnerMutex,
        wire::PoisonTarget::ConptyOwnerMutex => PoisonTarget::ConptyOwnerMutex,
        wire::PoisonTarget::OutputPipeWriterMutex => PoisonTarget::OutputPipeWriterMutex,
        wire::PoisonTarget::StdinCloseDebtMutex => PoisonTarget::StdinCloseDebtMutex,
        wire::PoisonTarget::StickyLatchMutex => PoisonTarget::StickyLatchMutex,
        wire::PoisonTarget::AttemptSlotMutex => PoisonTarget::AttemptSlotMutex,
        wire::PoisonTarget::HostOwnerMutex => PoisonTarget::HostOwnerMutex,
    }
}

fn put_poison_state_fact(mut b: wire::poison_state_fact::Builder<'_>, fact: &PoisonStateFact) {
    match fact {
        PoisonStateFact::NotObserved => b.set_not_observed(()),
        PoisonStateFact::NotProven => b.set_not_proven(()),
        PoisonStateFact::NoPoisonObserved => b.set_no_poison_observed(()),
        PoisonStateFact::PoisonObserved { target } => {
            b.set_poison_observed(put_poison_target(*target));
        }
    }
}

fn get_poison_state_fact(
    r: wire::poison_state_fact::Reader<'_>,
) -> Result<PoisonStateFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::poison_state_fact::NotObserved(()) => PoisonStateFact::NotObserved,
        wire::poison_state_fact::NotProven(()) => PoisonStateFact::NotProven,
        wire::poison_state_fact::NoPoisonObserved(()) => PoisonStateFact::NoPoisonObserved,
        wire::poison_state_fact::PoisonObserved(t) => PoisonStateFact::PoisonObserved {
            target: get_poison_target(t.map_err(map_not_in_schema)?),
        },
    })
}

fn put_sticky_unknown_state_fact(
    mut b: wire::sticky_unknown_state_fact::Builder<'_>,
    fact: &StickyUnknownStateFact,
) {
    match fact {
        StickyUnknownStateFact::NotObserved => b.set_not_observed(()),
        StickyUnknownStateFact::NotProven => b.set_not_proven(()),
        StickyUnknownStateFact::Clear => b.set_clear(()),
        StickyUnknownStateFact::LatchedStickyUnknown { replay_policy } => {
            b.set_latched_sticky_unknown(put_replay_disposition(*replay_policy));
        }
    }
}

fn get_sticky_unknown_state_fact(
    r: wire::sticky_unknown_state_fact::Reader<'_>,
) -> Result<StickyUnknownStateFact, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::sticky_unknown_state_fact::NotObserved(()) => StickyUnknownStateFact::NotObserved,
        wire::sticky_unknown_state_fact::NotProven(()) => StickyUnknownStateFact::NotProven,
        wire::sticky_unknown_state_fact::Clear(()) => StickyUnknownStateFact::Clear,
        wire::sticky_unknown_state_fact::LatchedStickyUnknown(rp) => {
            StickyUnknownStateFact::LatchedStickyUnknown {
                replay_policy: get_replay_disposition(rp.map_err(map_not_in_schema)?),
            }
        }
    })
}

fn put_debt_and_sticky_unknown_facts(
    mut b: wire::debt_and_sticky_unknown_facts::Builder<'_>,
    facts: &DebtAndStickyUnknownFacts,
) {
    put_close_debt_fact(b.reborrow().init_close_debt(), &facts.close_debt);
    put_first_error_fact(b.reborrow().init_first_error(), &facts.first_error);
    put_poison_state_fact(b.reborrow().init_poison_state(), &facts.poison_state);
    put_sticky_unknown_state_fact(b.init_sticky_unknown_state(), &facts.sticky_unknown_state);
}

fn get_debt_and_sticky_unknown_facts(
    r: wire::debt_and_sticky_unknown_facts::Reader<'_>,
) -> Result<DebtAndStickyUnknownFacts, NativeCloseCodecError> {
    if !r.has_close_debt()
        || !r.has_first_error()
        || !r.has_poison_state()
        || !r.has_sticky_unknown_state()
    {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(DebtAndStickyUnknownFacts {
        close_debt: get_close_debt_fact(r.get_close_debt().map_err(map_capnp_err)?)?,
        first_error: get_first_error_fact(r.get_first_error().map_err(map_capnp_err)?)?,
        poison_state: get_poison_state_fact(r.get_poison_state().map_err(map_capnp_err)?)?,
        sticky_unknown_state: get_sticky_unknown_state_fact(
            r.get_sticky_unknown_state().map_err(map_capnp_err)?,
        )?,
    })
}

pub(crate) fn put_reported_native_close_facts(
    mut b: wire::reported_native_close_facts::Builder<'_>,
    facts: &ReportedNativeCloseFacts,
) {
    put_child_native_exit_fact(b.reborrow().init_child_exit(), &facts.child_exit);
    put_io_streams_and_workers_facts(
        b.reborrow().init_io_streams_and_workers(),
        &facts.io_streams_and_workers,
    );
    put_original_handles_close_facts(b.reborrow().init_handles_close(), &facts.handles_close);
    put_original_job_lifecycle_facts(b.reborrow().init_job_lifecycle(), &facts.job_lifecycle);
    put_writer_closure_facts(b.reborrow().init_writers(), &facts.writers);
    put_lease_retirement_facts(b.reborrow().init_leases(), &facts.leases);
    put_debt_and_sticky_unknown_facts(
        b.init_debt_and_sticky_unknown(),
        &facts.debt_and_sticky_unknown,
    );
}

pub(crate) fn get_reported_native_close_facts(
    r: wire::reported_native_close_facts::Reader<'_>,
) -> Result<ReportedNativeCloseFacts, NativeCloseCodecError> {
    if !r.has_child_exit()
        || !r.has_io_streams_and_workers()
        || !r.has_handles_close()
        || !r.has_job_lifecycle()
        || !r.has_writers()
        || !r.has_leases()
        || !r.has_debt_and_sticky_unknown()
    {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(ReportedNativeCloseFacts {
        child_exit: get_child_native_exit_fact(r.get_child_exit().map_err(map_capnp_err)?)?,
        io_streams_and_workers: get_io_streams_and_workers_facts(
            r.get_io_streams_and_workers().map_err(map_capnp_err)?,
        )?,
        handles_close: get_original_handles_close_facts(
            r.get_handles_close().map_err(map_capnp_err)?,
        )?,
        job_lifecycle: get_original_job_lifecycle_facts(
            r.get_job_lifecycle().map_err(map_capnp_err)?,
        )?,
        writers: get_writer_closure_facts(r.get_writers().map_err(map_capnp_err)?)?,
        leases: get_lease_retirement_facts(r.get_leases().map_err(map_capnp_err)?)?,
        debt_and_sticky_unknown: get_debt_and_sticky_unknown_facts(
            r.get_debt_and_sticky_unknown().map_err(map_capnp_err)?,
        )?,
    })
}

// ============================================================================
// Wire Envelopes & Runtime Packet / Bootstrap Encoders / Decoders
// ============================================================================

fn put_capability_opt_in(v: NativeCloseCapabilityOptIn) -> wire::NativeCloseCapabilityOptIn {
    match v {
        NativeCloseCapabilityOptIn::ExplicitlyOptedInV8 => {
            wire::NativeCloseCapabilityOptIn::ExplicitlyOptedInVersionEight
        }
        NativeCloseCapabilityOptIn::DisabledOrNotRequested => {
            wire::NativeCloseCapabilityOptIn::DisabledOrNotRequested
        }
    }
}

fn get_capability_opt_in(v: wire::NativeCloseCapabilityOptIn) -> NativeCloseCapabilityOptIn {
    match v {
        wire::NativeCloseCapabilityOptIn::ExplicitlyOptedInVersionEight => {
            NativeCloseCapabilityOptIn::ExplicitlyOptedInV8
        }
        wire::NativeCloseCapabilityOptIn::DisabledOrNotRequested => {
            NativeCloseCapabilityOptIn::DisabledOrNotRequested
        }
    }
}

fn put_handshake_hello(
    mut b: wire::native_close_handshake_hello_wire::Builder<'_>,
    hello: &NativeCloseHandshakeHelloWire,
) {
    b.set_protocol_version(hello.protocol_version);
    b.set_schema(&hello.schema);
    b.set_startup_attempt_id(hello.startup_attempt_id);
    b.set_capability(put_capability_opt_in(hello.capability));
    b.set_auth_tag(hello.auth_tag.as_bytes());
}

fn get_handshake_hello(
    r: wire::native_close_handshake_hello_wire::Reader<'_>,
) -> Result<NativeCloseHandshakeHelloWire, NativeCloseCodecError> {
    if !r.has_schema() || !r.has_auth_tag() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(NativeCloseHandshakeHelloWire {
        protocol_version: r.get_protocol_version(),
        schema: array32(r.get_schema().map_err(map_capnp_err)?)?,
        startup_attempt_id: r.get_startup_attempt_id(),
        capability: get_capability_opt_in(r.get_capability().map_err(map_not_in_schema)?),
        auth_tag: WireAuthTag::from_bytes(array32(r.get_auth_tag().map_err(map_capnp_err)?)?),
    })
}

fn put_handshake_ack(
    mut b: wire::native_close_handshake_ack_wire::Builder<'_>,
    ack: &NativeCloseHandshakeAckWire,
) {
    b.set_protocol_version(ack.protocol_version);
    b.set_schema(&ack.schema);
    b.set_startup_attempt_id(ack.startup_attempt_id);
    b.set_capability(put_capability_opt_in(ack.capability));
    b.set_auth_tag(ack.auth_tag.as_bytes());
}

fn get_handshake_ack(
    r: wire::native_close_handshake_ack_wire::Reader<'_>,
) -> Result<NativeCloseHandshakeAckWire, NativeCloseCodecError> {
    if !r.has_schema() || !r.has_auth_tag() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(NativeCloseHandshakeAckWire {
        protocol_version: r.get_protocol_version(),
        schema: array32(r.get_schema().map_err(map_capnp_err)?)?,
        startup_attempt_id: r.get_startup_attempt_id(),
        capability: get_capability_opt_in(r.get_capability().map_err(map_not_in_schema)?),
        auth_tag: WireAuthTag::from_bytes(array32(r.get_auth_tag().map_err(map_capnp_err)?)?),
    })
}

fn put_spawn_ready(
    mut b: wire::native_close_spawn_ready_wire::Builder<'_>,
    ready: &NativeCloseSpawnReadyWire,
) {
    b.set_startup_attempt_id(ready.startup_attempt_id);
    put_wire_child_identity(b.reborrow().init_child_identity(), &ready.child_identity);
    b.set_auth_tag(ready.auth_tag.as_bytes());
}

fn get_spawn_ready(
    r: wire::native_close_spawn_ready_wire::Reader<'_>,
) -> Result<NativeCloseSpawnReadyWire, NativeCloseCodecError> {
    if !r.has_child_identity() || !r.has_auth_tag() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(NativeCloseSpawnReadyWire {
        startup_attempt_id: r.get_startup_attempt_id(),
        child_identity: get_wire_child_identity(r.get_child_identity().map_err(map_capnp_err)?),
        auth_tag: WireAuthTag::from_bytes(array32(r.get_auth_tag().map_err(map_capnp_err)?)?),
    })
}

fn put_output_frame(
    mut b: wire::native_close_output_frame_wire::Builder<'_>,
    out: &NativeCloseOutputFrameWire,
) {
    b.set_stream(match out.stream {
        NativeCloseOutputStream::Stdout => wire::NativeCloseOutputStream::Stdout,
        NativeCloseOutputStream::Stderr => wire::NativeCloseOutputStream::Stderr,
    });
    b.set_byte_len(out.byte_len);
}

fn get_output_frame(
    r: wire::native_close_output_frame_wire::Reader<'_>,
) -> Result<NativeCloseOutputFrameWire, NativeCloseCodecError> {
    Ok(NativeCloseOutputFrameWire {
        stream: match r.get_stream().map_err(map_not_in_schema)? {
            wire::NativeCloseOutputStream::Stdout => NativeCloseOutputStream::Stdout,
            wire::NativeCloseOutputStream::Stderr => NativeCloseOutputStream::Stderr,
        },
        byte_len: r.get_byte_len(),
    })
}

fn put_control_result(
    mut b: wire::native_close_control_result_wire::Builder<'_>,
    ctrl: &NativeCloseControlResultWire,
) {
    b.set_request_id(ctrl.request_id);
    b.set_status(match ctrl.status {
        ControlStatus::Applied => wire::NativeCloseControlStatus::Applied,
        ControlStatus::Rejected => wire::NativeCloseControlStatus::Rejected,
        ControlStatus::Unsupported => wire::NativeCloseControlStatus::Unsupported,
        ControlStatus::Unknown => wire::NativeCloseControlStatus::Unknown,
    });
}

fn get_control_result(
    r: wire::native_close_control_result_wire::Reader<'_>,
) -> Result<NativeCloseControlResultWire, NativeCloseCodecError> {
    Ok(NativeCloseControlResultWire {
        request_id: r.get_request_id(),
        status: match r.get_status().map_err(map_not_in_schema)? {
            wire::NativeCloseControlStatus::Applied => ControlStatus::Applied,
            wire::NativeCloseControlStatus::Rejected => ControlStatus::Rejected,
            wire::NativeCloseControlStatus::Unsupported => ControlStatus::Unsupported,
            wire::NativeCloseControlStatus::Unknown => ControlStatus::Unknown,
        },
    })
}

fn put_business_exit(
    mut b: wire::native_close_business_exit_wire::Builder<'_>,
    exit: &NativeCloseBusinessExitWire,
) {
    b.set_exit_code(exit.exit_code);
    b.set_timeout_disposition(put_timeout_disposition(exit.timeout_disposition));
}

fn get_business_exit(
    r: wire::native_close_business_exit_wire::Reader<'_>,
) -> Result<NativeCloseBusinessExitWire, NativeCloseCodecError> {
    Ok(NativeCloseBusinessExitWire {
        exit_code: r.get_exit_code(),
        timeout_disposition: get_timeout_disposition(
            r.get_timeout_disposition().map_err(map_not_in_schema)?,
        ),
    })
}

fn put_receipt(
    mut b: wire::native_close_receipt_wire::Builder<'_>,
    receipt: &NativeCloseReceiptWire,
) {
    b.set_protocol_version(receipt.protocol_version);
    b.set_schema(&receipt.schema);
    b.set_startup_attempt_id(receipt.startup_attempt_id);
    b.set_sequence_number(receipt.sequence_number);
    put_reported_native_close_facts(b.reborrow().init_facts(), &receipt.facts);
    b.set_auth_tag(receipt.auth_tag.as_bytes());
}

fn get_receipt(
    r: wire::native_close_receipt_wire::Reader<'_>,
) -> Result<NativeCloseReceiptWire, NativeCloseCodecError> {
    if !r.has_schema() || !r.has_facts() || !r.has_auth_tag() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(NativeCloseReceiptWire {
        protocol_version: r.get_protocol_version(),
        schema: array32(r.get_schema().map_err(map_capnp_err)?)?,
        startup_attempt_id: r.get_startup_attempt_id(),
        sequence_number: r.get_sequence_number(),
        facts: get_reported_native_close_facts(r.get_facts().map_err(map_capnp_err)?)?,
        auth_tag: WireAuthTag::from_bytes(array32(r.get_auth_tag().map_err(map_capnp_err)?)?),
    })
}

fn put_runtime_bootstrap(
    mut b: wire::runtime_bootstrap_wire::Builder<'_>,
    boot: &RuntimeBootstrapWire,
) {
    b.set_schema(&boot.schema);
    b.set_attempt(boot.attempt);
    put_runtime_runner_identity(b.reborrow().init_runner(), &boot.runner);
    b.set_seed(&boot.seed);
    put_handshake_hello(b.reborrow().init_hello(), &boot.hello);
    b.set_terminal_close_ack_required(boot.terminal_close_ack_required);
}

fn get_runtime_bootstrap(
    r: wire::runtime_bootstrap_wire::Reader<'_>,
) -> Result<RuntimeBootstrapWire, NativeCloseCodecError> {
    if !r.has_schema() || !r.has_runner() || !r.has_seed() || !r.has_hello() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(RuntimeBootstrapWire {
        schema: array32(r.get_schema().map_err(map_capnp_err)?)?,
        attempt: r.get_attempt(),
        runner: get_runtime_runner_identity(r.get_runner().map_err(map_capnp_err)?),
        seed: array32(r.get_seed().map_err(map_capnp_err)?)?,
        hello: get_handshake_hello(r.get_hello().map_err(map_capnp_err)?)?,
        terminal_close_ack_required: r.get_terminal_close_ack_required(),
    })
}

fn put_runtime_direction(v: RuntimeDirection) -> wire::RuntimeDirection {
    match v {
        RuntimeDirection::HostToRunner => wire::RuntimeDirection::HostToRunner,
        RuntimeDirection::RunnerToHost => wire::RuntimeDirection::RunnerToHost,
    }
}

fn get_runtime_direction(v: wire::RuntimeDirection) -> RuntimeDirection {
    match v {
        wire::RuntimeDirection::HostToRunner => RuntimeDirection::HostToRunner,
        wire::RuntimeDirection::RunnerToHost => RuntimeDirection::RunnerToHost,
    }
}

fn put_runtime_packet_content(
    mut b: wire::runtime_packet_content::Builder<'_>,
    content: &RuntimePacketContent,
) -> Result<(), NativeCloseCodecError> {
    match content {
        RuntimePacketContent::Empty => {
            b.set_empty(());
            Ok(())
        }
        RuntimePacketContent::Message(framed) => {
            validate_redacted_runner_frame_for_encode(framed)?;
            let encoded = crate::runner_wire::encode_frame_payload(framed).map_err(|_| {
                NativeCloseCodecError::EncodeFailed(EncodeErrorKind::InvalidEmbeddedRunnerFrame)
            })?;
            if encoded.is_empty() || encoded.len() > MAX_NATIVE_CLOSE_FRAME_BYTES {
                return Err(NativeCloseCodecError::EncodeFailed(
                    EncodeErrorKind::InvalidEmbeddedRunnerFrame,
                ));
            }
            b.set_runner_message(&encoded);
            Ok(())
        }
        RuntimePacketContent::TerminalAck {
            child,
            raw_child_exit_dword,
        } => {
            let mut ack = b.init_terminal_ack();
            put_wire_child_identity(ack.reborrow().init_child(), child);
            ack.set_raw_child_exit_dword(*raw_child_exit_dword);
            Ok(())
        }
    }
}

fn get_runtime_packet_content(
    r: wire::runtime_packet_content::Reader<'_>,
) -> Result<RuntimePacketContent, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::runtime_packet_content::Empty(()) => RuntimePacketContent::Empty,
        wire::runtime_packet_content::RunnerMessage(data) => {
            let bytes = data.map_err(map_capnp_err)?;
            if bytes.is_empty() || bytes.len() > MAX_NATIVE_CLOSE_FRAME_BYTES {
                return Err(NativeCloseCodecError::MalformedCapnp(
                    MalformedCapnpKind::InvalidDataFieldLength,
                ));
            }
            let framed = crate::runner_wire::decode_frame_payload(bytes).map_err(|_| {
                NativeCloseCodecError::MalformedCapnp(
                    MalformedCapnpKind::InvalidEmbeddedRunnerFrame,
                )
            })?;
            validate_redacted_runner_frame_on_decode(&framed)?;
            RuntimePacketContent::Message(Box::new(framed))
        }
        wire::runtime_packet_content::TerminalAck(ack) => {
            let ack = ack.map_err(map_capnp_err)?;
            if !ack.has_child() {
                return Err(NativeCloseCodecError::MalformedCapnp(
                    MalformedCapnpKind::InvalidPointer,
                ));
            }
            RuntimePacketContent::TerminalAck {
                child: get_wire_child_identity(ack.get_child().map_err(map_capnp_err)?),
                raw_child_exit_dword: ack.get_raw_child_exit_dword(),
            }
        }
    })
}

fn put_maybe_runtime_packet_event(
    mut b: wire::maybe_runtime_packet_event::Builder<'_>,
    event: &Option<Box<Version8WireFrame>>,
) -> Result<(), NativeCloseCodecError> {
    match event {
        None => {
            b.set_none(());
            Ok(())
        }
        Some(frame) => {
            let mut ev = b.init_some();
            ev.set_version(frame.version);
            match &frame.message {
                Version8Message::RuntimeBootstrap(_) | Version8Message::RuntimePacket(_) => Err(
                    NativeCloseCodecError::EncodeFailed(EncodeErrorKind::RecursiveRuntimePacket),
                ),
                Version8Message::HandshakeHello(v) => {
                    put_handshake_hello(ev.init_handshake_hello(), v);
                    Ok(())
                }
                Version8Message::HandshakeAck(v) => {
                    put_handshake_ack(ev.init_handshake_ack(), v);
                    Ok(())
                }
                Version8Message::SpawnReady(v) => {
                    put_spawn_ready(ev.init_spawn_ready(), v);
                    Ok(())
                }
                Version8Message::Output(v) => {
                    put_output_frame(ev.init_output(), v);
                    Ok(())
                }
                Version8Message::ControlResult(v) => {
                    put_control_result(ev.init_control_result(), v);
                    Ok(())
                }
                Version8Message::BusinessExit(v) => {
                    put_business_exit(ev.init_business_exit(), v);
                    Ok(())
                }
                Version8Message::NativeCloseReceipt(v) => {
                    put_receipt(ev.init_native_close_receipt(), v);
                    Ok(())
                }
            }
        }
    }
}

fn get_maybe_runtime_packet_event(
    r: wire::maybe_runtime_packet_event::Reader<'_>,
) -> Result<Option<Box<Version8WireFrame>>, NativeCloseCodecError> {
    Ok(match r.which().map_err(map_not_in_schema)? {
        wire::maybe_runtime_packet_event::None(()) => None,
        wire::maybe_runtime_packet_event::Some(ev) => {
            let ev = ev.map_err(map_capnp_err)?;
            let version = ev.get_version();
            let message = match ev.which().map_err(map_not_in_schema)? {
                wire::runtime_packet_event_frame::HandshakeHello(v) => {
                    Version8Message::HandshakeHello(get_handshake_hello(v.map_err(map_capnp_err)?)?)
                }
                wire::runtime_packet_event_frame::HandshakeAck(v) => {
                    Version8Message::HandshakeAck(get_handshake_ack(v.map_err(map_capnp_err)?)?)
                }
                wire::runtime_packet_event_frame::SpawnReady(v) => {
                    Version8Message::SpawnReady(get_spawn_ready(v.map_err(map_capnp_err)?)?)
                }
                wire::runtime_packet_event_frame::Output(v) => {
                    Version8Message::Output(get_output_frame(v.map_err(map_capnp_err)?)?)
                }
                wire::runtime_packet_event_frame::ControlResult(v) => {
                    Version8Message::ControlResult(get_control_result(v.map_err(map_capnp_err)?)?)
                }
                wire::runtime_packet_event_frame::BusinessExit(v) => {
                    Version8Message::BusinessExit(get_business_exit(v.map_err(map_capnp_err)?)?)
                }
                wire::runtime_packet_event_frame::NativeCloseReceipt(v) => {
                    Version8Message::NativeCloseReceipt(Box::new(get_receipt(
                        v.map_err(map_capnp_err)?,
                    )?))
                }
            };
            Some(Box::new(Version8WireFrame { version, message }))
        }
    })
}

fn put_runtime_packet(
    mut b: wire::runtime_packet_wire::Builder<'_>,
    packet: &RuntimePacketWire,
) -> Result<(), NativeCloseCodecError> {
    b.set_schema(&packet.schema);
    b.set_attempt(packet.attempt);
    b.set_sequence(packet.sequence);
    b.set_direction(put_runtime_direction(packet.direction));
    put_maybe_runtime_packet_event(b.reborrow().init_event(), &packet.event)?;
    put_runtime_packet_content(b.reborrow().init_content(), &packet.content)?;
    put_maybe_auth_tag(
        b.reborrow().init_terminal_close_ack(),
        packet.terminal_close_ack,
    );
    b.set_tag(packet.tag.as_bytes());
    Ok(())
}

fn get_runtime_packet(
    r: wire::runtime_packet_wire::Reader<'_>,
) -> Result<RuntimePacketWire, NativeCloseCodecError> {
    if !r.has_schema()
        || !r.has_event()
        || !r.has_content()
        || !r.has_terminal_close_ack()
        || !r.has_tag()
    {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    Ok(RuntimePacketWire {
        schema: array32(r.get_schema().map_err(map_capnp_err)?)?,
        attempt: r.get_attempt(),
        sequence: r.get_sequence(),
        direction: get_runtime_direction(r.get_direction().map_err(map_not_in_schema)?),
        event: get_maybe_runtime_packet_event(r.get_event().map_err(map_capnp_err)?)?,
        content: get_runtime_packet_content(r.get_content().map_err(map_capnp_err)?)?,
        terminal_close_ack: get_maybe_auth_tag(r.get_terminal_close_ack().map_err(map_capnp_err)?)?,
        tag: WireAuthTag::from_bytes(array32(r.get_tag().map_err(map_capnp_err)?)?),
    })
}

fn put_version_eight_wire_frame(
    mut b: wire::version_eight_wire_frame::Builder<'_>,
    frame: &Version8WireFrame,
) -> Result<(), NativeCloseCodecError> {
    b.set_version(frame.version);
    match &frame.message {
        Version8Message::RuntimeBootstrap(v) => {
            put_runtime_bootstrap(b.init_runtime_bootstrap(), v);
            Ok(())
        }
        Version8Message::RuntimePacket(v) => put_runtime_packet(b.init_runtime_packet(), v),
        Version8Message::HandshakeHello(v) => {
            put_handshake_hello(b.init_handshake_hello(), v);
            Ok(())
        }
        Version8Message::HandshakeAck(v) => {
            put_handshake_ack(b.init_handshake_ack(), v);
            Ok(())
        }
        Version8Message::SpawnReady(v) => {
            put_spawn_ready(b.init_spawn_ready(), v);
            Ok(())
        }
        Version8Message::Output(v) => {
            put_output_frame(b.init_output(), v);
            Ok(())
        }
        Version8Message::ControlResult(v) => {
            put_control_result(b.init_control_result(), v);
            Ok(())
        }
        Version8Message::BusinessExit(v) => {
            put_business_exit(b.init_business_exit(), v);
            Ok(())
        }
        Version8Message::NativeCloseReceipt(v) => {
            put_receipt(b.init_native_close_receipt(), v);
            Ok(())
        }
    }
}

fn get_version_eight_wire_frame(
    r: wire::version_eight_wire_frame::Reader<'_>,
) -> Result<Version8WireFrame, NativeCloseCodecError> {
    let version = r.get_version();
    let message = match r.which().map_err(map_not_in_schema)? {
        wire::version_eight_wire_frame::RuntimeBootstrap(v) => Version8Message::RuntimeBootstrap(
            Box::new(get_runtime_bootstrap(v.map_err(map_capnp_err)?)?),
        ),
        wire::version_eight_wire_frame::RuntimePacket(v) => {
            Version8Message::RuntimePacket(Box::new(get_runtime_packet(v.map_err(map_capnp_err)?)?))
        }
        wire::version_eight_wire_frame::HandshakeHello(v) => {
            Version8Message::HandshakeHello(get_handshake_hello(v.map_err(map_capnp_err)?)?)
        }
        wire::version_eight_wire_frame::HandshakeAck(v) => {
            Version8Message::HandshakeAck(get_handshake_ack(v.map_err(map_capnp_err)?)?)
        }
        wire::version_eight_wire_frame::SpawnReady(v) => {
            Version8Message::SpawnReady(get_spawn_ready(v.map_err(map_capnp_err)?)?)
        }
        wire::version_eight_wire_frame::Output(v) => {
            Version8Message::Output(get_output_frame(v.map_err(map_capnp_err)?)?)
        }
        wire::version_eight_wire_frame::ControlResult(v) => {
            Version8Message::ControlResult(get_control_result(v.map_err(map_capnp_err)?)?)
        }
        wire::version_eight_wire_frame::BusinessExit(v) => {
            Version8Message::BusinessExit(get_business_exit(v.map_err(map_capnp_err)?)?)
        }
        wire::version_eight_wire_frame::NativeCloseReceipt(v) => {
            Version8Message::NativeCloseReceipt(Box::new(get_receipt(v.map_err(map_capnp_err)?)?))
        }
    };
    Ok(Version8WireFrame { version, message })
}

// ============================================================================
// Top-Level Canonical Encoders & Decoders
// ============================================================================

/// Encodes a `Version8WireFrame` into an unframed canonical Cap'n Proto payload slice,
/// enforcing `MAX_NATIVE_CLOSE_FRAME_BYTES`.
fn canonical_bytes(
    message: capnp::message::Builder<capnp::message::HeapAllocator>,
) -> Result<Vec<u8>, NativeCloseCodecError> {
    let words = message
        .into_reader()
        .canonicalize()
        .map_err(map_capnp_err)?;
    let segments = [capnp::Word::words_to_bytes(&words)];
    Ok(capnp::serialize::write_message_segments_to_words(
        &segments.as_slice(),
    ))
}

pub(crate) fn encode_v8_capnp_payload(
    frame: &Version8WireFrame,
) -> Result<Vec<u8>, NativeCloseCodecError> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut env = message.init_root::<wire::native_close_envelope::Builder<'_>>();
        env.set_magic(NATIVE_CLOSE_WIRE_MAGIC);
        env.set_wire_revision(NATIVE_CLOSE_WIRE_REVISION);
        env.set_wire_format(wire::WireFormatKind::CapnpTyped);
        env.set_envelope_kind(wire::EnvelopeKind::VersionEightFrame);
        put_version_eight_wire_frame(env.init_frame(), frame)?;
    }
    let bytes = canonical_bytes(message)?;
    if bytes.is_empty() {
        return Err(NativeCloseCodecError::EmptyFrame);
    }
    if bytes.len() > MAX_NATIVE_CLOSE_FRAME_BYTES {
        return Err(NativeCloseCodecError::FrameTooLarge {
            len: bytes.len(),
            max: MAX_NATIVE_CLOSE_FRAME_BYTES,
        });
    }
    Ok(bytes)
}

/// Validates and decodes an unframed Cap'n Proto payload slice into a `Version8WireFrame`.
///
/// Rejects:
/// - empty or oversize payloads (`> 16 KiB`),
/// - legacy JSON or non-Cap'n Proto inputs,
/// - truncated segment tables or segment bodies,
/// - trailing bytes or concatenated messages inside the payload,
/// - nesting depth (`> 16`) or bounded traversal violations,
/// - wrong magic, wire revision, wire format, or envelope kind,
/// - unknown enum or union discriminants (`capnp::NotInSchema`),
/// - recursive `RuntimePacket` events or unredacted embedded runner nonces,
/// - non-canonical encodings (unknown fields, omitted pointers, extra segments, non-zero padding).
pub fn decode_v8_capnp_payload_exact(
    payload: &[u8],
) -> Result<Version8WireFrame, NativeCloseCodecError> {
    validate_capnp_segment_framing(payload)?;

    let options = capnp::message::ReaderOptions {
        traversal_limit_in_words: Some(MAX_NATIVE_CLOSE_CAPNP_TRAVERSAL_WORDS),
        nesting_limit: MAX_NATIVE_CLOSE_CAPNP_DEPTH as i32,
    };
    let mut remaining = payload;
    let message = capnp::serialize::read_message_from_flat_slice(&mut remaining, options)
        .map_err(map_capnp_err)?;
    if !remaining.is_empty() {
        return Err(NativeCloseCodecError::TrailingBytesInPayload);
    }
    if !message.is_canonical().map_err(map_capnp_err)? {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::NonCanonicalEncoding,
        ));
    }

    let env = message
        .get_root::<wire::native_close_envelope::Reader<'_>>()
        .map_err(map_capnp_err)?;
    if env.get_magic() != NATIVE_CLOSE_WIRE_MAGIC
        || env.get_wire_revision() != NATIVE_CLOSE_WIRE_REVISION
    {
        return Err(NativeCloseCodecError::UnknownWireIdentity);
    }
    let wire_format = env.get_wire_format().map_err(map_not_in_schema)?;
    if wire_format != wire::WireFormatKind::CapnpTyped {
        return Err(NativeCloseCodecError::UnknownWireIdentity);
    }
    let envelope_kind = env.get_envelope_kind().map_err(map_not_in_schema)?;
    if envelope_kind != wire::EnvelopeKind::VersionEightFrame {
        return Err(NativeCloseCodecError::UnknownWireIdentity);
    }
    if !env.has_frame() {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::InvalidPointer,
        ));
    }
    let frame = get_version_eight_wire_frame(env.get_frame().map_err(map_capnp_err)?)?;

    // Canonical exact re-encode check: rejects unknown future fields, omitted explicit pointers,
    // multi-segment or far-pointer non-canonical layouts, and non-zero padding bytes.
    let canonical = encode_v8_capnp_payload(&frame)?;
    if canonical.as_slice() != payload {
        return Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::NonCanonicalEncoding,
        ));
    }

    Ok(frame)
}

/// Deterministically encodes `ReportedNativeCloseFacts` as a canonical typed Cap'n Proto
/// `ReceiptAuthFactsEnvelope` for receipt authentication tag computation.
///
/// Never uses `serde_json` or derived debug/text serialization.
pub(crate) fn encode_reported_facts_canonical(facts: &ReportedNativeCloseFacts) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut env = message.init_root::<wire::receipt_auth_facts_envelope::Builder<'_>>();
        env.set_magic(NATIVE_CLOSE_WIRE_MAGIC);
        env.set_wire_revision(NATIVE_CLOSE_WIRE_REVISION);
        env.set_wire_format(wire::WireFormatKind::CapnpTyped);
        env.set_envelope_kind(wire::EnvelopeKind::ReceiptAuthFacts);
        put_reported_native_close_facts(env.init_facts(), facts);
    }
    // All fact domains are finite enums and fixed-size scalar/identity fields.
    canonical_bytes(message).expect("bounded typed receipt fact tree")
}

/// Deterministically encodes the authenticated body of a `RuntimePacketWire` (with `tag`
/// omitted in the dedicated `RuntimePacketAuthBody` schema) into canonical Cap'n Proto bytes.
///
/// Includes `schema`, `attempt`, `sequence`, `direction`, `event`, `content`, and
/// `terminal_close_ack`. Never uses `serde_json`.
pub(crate) fn encode_runtime_packet_auth_body(
    packet: &RuntimePacketWire,
) -> Result<Vec<u8>, NativeCloseCodecError> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut env = message.init_root::<wire::runtime_packet_auth_body_envelope::Builder<'_>>();
        env.set_magic(NATIVE_CLOSE_WIRE_MAGIC);
        env.set_wire_revision(NATIVE_CLOSE_WIRE_REVISION);
        env.set_wire_format(wire::WireFormatKind::CapnpTyped);
        env.set_envelope_kind(wire::EnvelopeKind::RuntimePacketAuthBody);
        let mut body = env.init_body();
        body.set_schema(&packet.schema);
        body.set_attempt(packet.attempt);
        body.set_sequence(packet.sequence);
        body.set_direction(put_runtime_direction(packet.direction));
        put_maybe_runtime_packet_event(body.reborrow().init_event(), &packet.event)?;
        put_runtime_packet_content(body.reborrow().init_content(), &packet.content)?;
        put_maybe_auth_tag(body.init_terminal_close_ack(), packet.terminal_close_ack);
    }
    let bytes = canonical_bytes(message)?;
    if bytes.is_empty() || bytes.len() > MAX_NATIVE_CLOSE_FRAME_BYTES {
        return Err(NativeCloseCodecError::EncodeFailed(
            EncodeErrorKind::SerializationFailed,
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "native_close_codec_tests.rs"]
mod tests;
