//! In-process semantic boundary, NOT a guest wire/schema or approval authority.
//! A future adapter must consume the sole host-owned Capnp contract. All methods
//! below are called through one validated native session driver, never a UI.
use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde::Serialize;
use std::{fmt, future::Future, pin::Pin};

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
