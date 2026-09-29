//! In-process semantic boundary, NOT a guest wire/schema or approval authority.
//! A future adapter must consume the sole host-owned Capnp contract. All methods
//! below are called through one validated native session driver, never a UI.
use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::{fmt, future::Future, pin::Pin};
use tokio_util::sync::CancellationToken;

pub type DriverFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BridgeError>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum BridgeError {
    WireNotReady,
    Cancelled,
    Deadline,
    Unsupported,
    DuplicateRequest,
    InvalidInput(&'static str),
    Protocol(&'static str),
    Denied,
    Unknown,
    CleanupUnconfirmed,
}
impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BridgeError {}

/// This local key routes cancellation/observations. It is NEVER a host grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestKey(pub u64);

/// Exact Core prepared request. Header values and body stay bytes; no lossy text.
#[derive(Clone)]
pub struct PreparedHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Bytes,
    pub body_sha256: [u8; 32],
    pub response_limit: usize,
}
impl fmt::Debug for PreparedHttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedHttpRequest")
            .field("method", &self.method)
            .field("header_count", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .field("response_limit", &self.response_limit)
            .finish_non_exhaustive()
    }
}

/// Opaque references originate at the host. These Rust values cannot approve
/// anything by themselves; the host must revalidate the actual session/bindings.
#[derive(Clone, Debug)]
pub struct ProposalRef(pub String);
#[derive(Debug)]
pub struct ApprovedProposal {
    pub proposal: ProposalRef,
    /// Explicit host-approved cap, which may tighten the proposed cap.
    pub response_limit: usize,
}
pub struct ResponseHead {
    pub status: StatusCode,
    pub headers: HeaderMap,
}
pub struct BodyChunk {
    pub offset: u64,
    pub bytes: Bytes,
}
pub enum ReadResult {
    Chunk(BodyChunk),
    Eof { offset: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CancelReason {
    User,
    OwnerDropped,
    CoreError,
    Deadline,
    InvalidResponse,
    ErrorBodyLimit,
    HostCancelled,
    NativeFailure,
}

/// A shared cancellation latch and event-delivery fence. Host application and
/// local cancellation synchronously set the same first reason and wake token.
/// with_live linearizes a nonblocking event enqueue/delivery before cancellation
/// or refuses it after cancellation. No callback may await or acquire driver state.
#[derive(Clone, Debug, Default)]
pub struct CancellationSignal {
    token: CancellationToken,
    gate: Arc<Mutex<DeliveryGate>>,
}
#[derive(Debug, Default)]
struct DeliveryGate {
    first_reason: Option<CancelReason>,
    paused: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct CancelTransition {
    pub cancel_gate_before: bool,
    pub cancel_gate_after: bool,
    pub first_cancel_reason_before: Option<CancelReason>,
    pub first_cancel_reason_after: Option<CancelReason>,
    pub delivery_paused_before: bool,
    pub host_control_closed_gate: bool,
}
impl CancellationSignal {
    pub fn cancel(&self, reason: CancelReason) {
        self.cancel_observed(reason);
    }
    pub fn cancel_observed(&self, reason: CancelReason) -> CancelTransition {
        let mut gate = self.gate.lock().unwrap();
        let before = gate.first_reason;
        gate.first_reason.get_or_insert(reason);
        self.token.cancel();
        CancelTransition {
            cancel_gate_before: before.is_some(),
            cancel_gate_after: true,
            first_cancel_reason_before: before,
            first_cancel_reason_after: gate.first_reason,
            delivery_paused_before: gate.paused,
            host_control_closed_gate: before.is_none() && reason == CancelReason::HostCancelled,
        }
    }
    /// Monotonic fence only: never grants work, resumes delivery, or sets a reason.
    pub fn pause_delivery(&self) {
        self.gate.lock().unwrap().paused = true;
    }
    pub fn reason(&self) -> Option<CancelReason> {
        self.gate.lock().unwrap().first_reason
    }
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
    pub async fn cancelled(&self) {
        self.token.cancelled().await
    }
    pub fn with_live<T>(&self, publish: impl FnOnce() -> T) -> Option<T> {
        self.try_delivery(publish).ok().flatten()
    }
    /// Err returns ownership of the original closure/event/permit while paused.
    pub fn try_delivery<T, F: FnOnce() -> T>(&self, publish: F) -> Result<Option<T>, F> {
        let gate = self.gate.lock().unwrap();
        if gate.first_reason.is_some() {
            Ok(None)
        } else if gate.paused {
            Err(publish)
        } else {
            Ok(Some(publish()))
        }
    }
    pub fn is_paused(&self) -> bool {
        self.gate.lock().unwrap().paused
    }
    pub async fn resolve_pause(&self) {
        if self.is_paused() {
            self.cancelled().await;
        }
    }
}

/// Absolute classifications supplied by the consumer, never inferred by a wire
/// driver from offsets. Parser yielded is not proof of semantic consumption.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ConsumptionProgress {
    pub consumed_offset: u64,
    pub parser_yielded_bytes: u64,
    pub drain_discarded_bytes: u64,
    pub error_body_consumed_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CleanupReceipt {
    pub network_eof: bool,
    pub durable_observed: bool,
    pub network_worker_exited: bool,
    pub network_worker_started: bool,
    pub data_connect_reaped: bool,
    pub data_read_reaped: bool,
    pub data_write_reaped: bool,
    pub data_channel_closed: bool,
    pub request_closed: bool,
    /// A live guest cannot observe its own exit/EOF/release. Request cleanup
    /// must return without waiting for these external session-level facts.
    pub session_release: Option<SessionReleaseObservation>,
    pub received_offset: u64,
    pub delivered_offset: u64,
    pub acknowledged_offset: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionReleaseObservation {
    pub native_child_exit_observed: bool,
    pub native_stdout_eof: bool,
    pub native_stderr_eof: bool,
    pub release_confirmed: bool,
}

pub trait NativeHttpSession: fmt::Debug + Send + Sync {
    /// Bind once under the native cancellation lock, before operation use.
    /// Replay any already-applied cancellation into this latch synchronously.
    /// Host Stop/Denied/revoke/fatal must set it before returning from application,
    /// not after another network read or an asynchronous forwarding task.
    fn bind_cancellation(
        &self,
        key: RequestKey,
        signal: CancellationSignal,
    ) -> Result<(), BridgeError>;
    /// Propose actual bytes only. No DNS/connect/HTTP and no implicit approval.
    fn prepare(
        &self,
        key: RequestKey,
        request: PreparedHttpRequest,
    ) -> DriverFuture<'_, ProposalRef>;
    /// Wait for a separate trusted-operator decision under the original deadline.
    fn await_approval(
        &self,
        key: RequestKey,
        proposal: ProposalRef,
    ) -> DriverFuture<'_, ApprovedProposal>;
    /// At most once; the host's durable claim is authoritative. No replay on error.
    fn commit(&self, key: RequestKey, approved: ApprovedProposal)
    -> DriverFuture<'_, ResponseHead>;
    /// A dropped wait must not lose the pending wire read or duplicate dispatch.
    /// The driver retains any pending result for this key/offset, including drain.
    /// That pending read and its result belong to a session-owned reader task;
    /// cancelling this future only detaches a waiter. Reattachment at the same
    /// offset takes exactly that result. OS read/buffer ownership cannot live in
    /// this disposable future. This is a requirement, not an implemented wire.
    /// A control HttpTerminal can arrive before the last data frame. Return Eof
    /// only after the contiguous data prefix reaches its validated final offset.
    /// Exactly reaching an approved byte cap is not EOF: the host still needs a
    /// bounded terminal poll and final absolute ACK, with no extra body credit.
    fn read(&self, key: RequestKey, offset: u64, max_bytes: usize) -> DriverFuture<'_, ReadResult>;
    /// Queue one absolute ACK; repeated offsets must never add credit twice.
    /// Cancel-generation late data belongs to driver cancellation-discard
    /// accounting, never parser/drain counters or fresh ACK credit.
    fn acknowledge(
        &self,
        key: RequestKey,
        progress: ConsumptionProgress,
    ) -> Result<(), BridgeError>;
    /// Nonblocking reserved control lane; must work while data is blocked.
    fn cancel(&self, key: RequestKey, reason: CancelReason) -> Result<(), BridgeError>;
    /// Observe cleanup facts; sending Cancel or dropping a receiver is not proof.
    /// Already durable Observed must not regress because of IPC/parser failure.
    /// Await only this request's HTTP/data operation quiescence. Never await the
    /// calling guest's own OS exit, diagnostic EOF, or whole-session release.
    fn cleanup(&self, key: RequestKey) -> DriverFuture<'_, CleanupReceipt>;
}

/// Intentional fail-closed placeholder, never a fabricated successful response.
#[derive(Debug)]
pub struct WireNotReady;
impl NativeHttpSession for WireNotReady {
    fn bind_cancellation(&self, _: RequestKey, _: CancellationSignal) -> Result<(), BridgeError> {
        Ok(())
    }
    fn prepare(&self, _: RequestKey, _: PreparedHttpRequest) -> DriverFuture<'_, ProposalRef> {
        Box::pin(async { Err(BridgeError::WireNotReady) })
    }
    fn await_approval(&self, _: RequestKey, _: ProposalRef) -> DriverFuture<'_, ApprovedProposal> {
        Box::pin(async { Err(BridgeError::WireNotReady) })
    }
    fn commit(&self, _: RequestKey, _: ApprovedProposal) -> DriverFuture<'_, ResponseHead> {
        Box::pin(async { Err(BridgeError::WireNotReady) })
    }
    fn read(&self, _: RequestKey, _: u64, _: usize) -> DriverFuture<'_, ReadResult> {
        Box::pin(async { Err(BridgeError::WireNotReady) })
    }
    fn acknowledge(&self, _: RequestKey, _: ConsumptionProgress) -> Result<(), BridgeError> {
        Err(BridgeError::WireNotReady)
    }
    fn cancel(&self, _: RequestKey, _: CancelReason) -> Result<(), BridgeError> {
        Err(BridgeError::WireNotReady)
    }
    fn cleanup(&self, _: RequestKey) -> DriverFuture<'_, CleanupReceipt> {
        Box::pin(async { Err(BridgeError::WireNotReady) })
    }
}
