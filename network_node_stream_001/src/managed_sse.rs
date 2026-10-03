//! Explicit native host-approved SSE source for the existing Receive/ACK ABI.
//!
//! This is an opt-in, uncredentialed POST slice. A package channel declaration
//! never approves a network endpoint. Only the original host Store may claim the
//! command; normal EOF and committed ACKs leave that command OutcomeUnknown.
//! There is no replay, resume, Last-Event-ID injection or retry interpretation.
mod envelope;
pub use envelope::{EventEnvelope, MAX_ENVELOPE_BYTES, VERSION, schema_digest};
use crate::{Limits, RawHttpRequest, client::{Client, EndpointPolicy}, sse, stream};
use morrow_core::{dispatch::HostRuntime, io, io_evidence::{Kind as MaterialKind, Material},
    io_intent::{Command, Phase, Record}, plugin_package::io::IoCapability};
use morrow_plugin_runtime::{channel::{ChannelBroker, Producer, SourceGrant},
    manager::{ManagedInstance, Manager}};
use sha2::{Digest, Sha256};
use std::{sync::{Arc, Condvar, Mutex, OnceLock, atomic::{AtomicBool, Ordering}},
    time::{Duration, Instant}};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkProfile {
    PublicHttps,
    /// Explicit synthetic loopback exception; never a private-network wildcard.
    LoopbackHttp,
    /// Ordinary hostname/certificate validation remains mandatory.
    LoopbackHttps,
}
/// All fields are consumed into an immutable approval, never mutated at start.
/// Deliberately without Debug: request values may contain private content.
#[derive(Clone)]
pub struct Approval {
    pub request: RawHttpRequest,
    pub operation_id: String,
    /// Nonzero trusted-host approval nonce. Not a guest-provided resource ID.
    pub approval_epoch: [u8; 32],
    pub deadline: Instant,
    pub profile: NetworkProfile,
    pub limits: Limits,
    pub decoder_limits: sse::DecoderLimits,
    /// Independent cumulative encoded frame ceiling, without ACK refunds.
    pub max_encoded_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    UnsupportedCredentials,
    Limit,
    AlreadyStarted,
    Conflict,
    OutcomeUnknown,
    CommitUnknown,
    Storage,
    Thread,
    Truncated,
    Source(morrow_plugin_runtime::channel::Error),
    Transport(crate::Error),
    Sse(sse::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "managed SSE: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
/// First failure, HTTP cleanup and ACK accounting are independent facts.
/// This receipt is published by the native callback. Only the original broker
/// reaper's CleanupProof::Joined proves that native thread actually joined.
#[derive(Debug)]
pub struct Completion {
    pub outcome: Result<()>,
    pub sse: Option<sse::Completion>,
    pub transport: Option<stream::Completion>,
    pub opening_cleanup: Option<stream::OpeningCleanup>,
    /// Frames whose original queue admission actually succeeded.
    pub events_queued: u64,
    /// Exact original frames whose consumption ACK was committed successfully.
    pub events_acked: u64,
    /// Encoded bytes reserved before queue admission, including failed attempts.
    /// No refund occurs on ACK, revocation or a failed subsequent push.
    pub encoded_bytes: u64,
}
impl Completion {
    fn empty(outcome: Result<()>) -> Self {
        Self { outcome, sse: None, transport: None, opening_cleanup: None,
            events_queued: 0, events_acked: 0, encoded_bytes: 0 }
    }
}
struct State {
    grant: SourceGrant,
    approval: Approval,
    client: Client,
    request: io::Request,
    frame_limit: usize,
    cursor_epoch: [u8; 32],
    started: AtomicBool,
    cancel: CancellationToken,
    completion: Mutex<Option<Arc<Completion>>>,
    changed: Condvar,
}
/// Clones share the same one-shot command and original grant. They do not issue
/// another approval, create a Store, reset budgets or permit a second POST.
#[derive(Clone)]
pub struct SseSource { state: Arc<State> }
impl SseSource {
    #[allow(clippy::too_many_arguments)]
    pub fn approve(manager: &Manager, host: &HostRuntime, instance: &ManagedInstance,
        broker: &ChannelBroker, expected_revision: u64, approval: Approval, now: u64,
    ) -> Result<Self> {
        Self::approve_with_live_guard(manager, host, instance, broker,
            expected_revision, approval, now, || true)
    }
    /// The restricting callback must be bounded, pure and non-reentrant. It is
    /// called even inside the original channel ACK transaction's final guard.
    #[allow(clippy::too_many_arguments)]
    pub fn approve_with_live_guard(manager: &Manager, host: &HostRuntime,
        instance: &ManagedInstance, broker: &ChannelBroker, expected_revision: u64,
        approval: Approval, now: u64, live: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Self> {
        let (client, request, target_digest, approval_digest) = prepare(&approval, broker)?;
        let package = instance.package().package();
        let expected = Command {
            operation_id: approval.operation_id.clone(),
            subject: package.manifest().package_id.clone(),
            package_sha256: package.digest(), capability: IoCapability::HttpRequest,
            protocol_sha256: io::schema_digest(), request_sha256: request.digest(),
            approval_sha256: approval_digest, target_sha256: target_digest,
            request_bytes: request.bytes().len() as u64,
            response_limit: approval.limits.max_response_bytes as u64,
        };
        // Validate immutable evidence pins before the grant can charge a channel.
        Record::prepared(expected.clone()).map_err(storage)?;
        let deadline = approval.deadline;
        let grant = SourceGrant::issue_with_deadline_and_live_guard(manager, host, instance, broker,
            expected_revision, expected, now, deadline, live)
            .map_err(Error::Source)?;
        let endpoint = broker.endpoint();
        Ok(Self { state: Arc::new(State {
            grant, approval, client, request,
            frame_limit: (endpoint.budget.max_frame_bytes as usize).min(MAX_ENVELOPE_BYTES),
            cursor_epoch: endpoint.source_epoch,
            started: AtomicBool::new(false), cancel: CancellationToken::new(),
            completion: Mutex::new(None), changed: Condvar::new(),
        }) })
    }
    pub fn command(&self) -> &Command { self.state.grant.expected_command() }
    pub fn grant(&self) -> &SourceGrant { &self.state.grant }
    pub fn revoke(&self) {
        self.state.grant.revoke();
        self.state.cancel.cancel();
    }
    pub fn completion(&self) -> Option<Arc<Completion>> {
        self.state.completion.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    /// Wait only for the callback's receipt. Separately use the original broker
    /// reaper for native join. The caller's timeout never renews source authority.
    pub fn wait_completion(&self, timeout: Duration) -> Option<Arc<Completion>> {
        let end = Instant::now().checked_add(timeout)?;
        let mut receipt = self.state.completion.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if receipt.is_some() { return receipt.clone(); }
            let remaining = end.checked_duration_since(Instant::now())?;
            let (next, wait) = self.state.changed.wait_timeout(receipt, remaining)
                .unwrap_or_else(|e| e.into_inner());
            receipt = next;
            if wait.timed_out() { return receipt.clone(); }
        }
    }
    /// Authenticate the exact original owner, retain and reserve original request
    /// material, win the one strict dispatch claim, then create the native source.
    /// Any error (including uncertain commit) consumes this object's start attempt.
    /// No async execution or new Store is involved in the claim transaction.
    pub fn start(&self, manager: &Manager, host: &mut HostRuntime,
        instance: &ManagedInstance, broker: &ChannelBroker,
    ) -> Result<()> {
        if self.state.started.swap(true, Ordering::AcqRel) { return Err(Error::AlreadyStarted); }
        let result = self.start_inner(manager, host, instance, broker);
        if let Err(error) = result { publish(&self.state, Completion::empty(Err(error))); }
        result
    }
    fn start_inner(&self, manager: &Manager, host: &mut HostRuntime,
        instance: &ManagedInstance, broker: &ChannelBroker,
    ) -> Result<()> {
        self.state.grant.validate_owner(manager, host, instance, broker).map_err(Error::Source)?;
        self.state.grant.check().map_err(Error::Source)?;
        let expected = self.command();
        let store = host.store_local_mut();
        let prepared = match store.lookup_io_intent(&expected.subject, &expected.operation_id)
            .map_err(storage)? {
            Some(record) => {
                record.matches_command(expected).map_err(storage)?;
                if record.phase() == Phase::OutcomeUnknown { return Err(Error::OutcomeUnknown); }
                if record.phase() != Phase::Prepared { return Err(Error::Conflict); }
                record
            }
            None => {
                let candidate = Record::prepared(expected.clone()).map_err(storage)?;
                store.append_io_intent_local_authorized(&candidate, || authorize(&self.state.grant))
                    .map_err(storage)?
            }
        };
        let original = Material::encode(MaterialKind::Request, &expected.operation_id,
            &expected.subject, expected.request_sha256, self.state.request.bytes()).map_err(storage)?;
        store.reserve_io_materials(expected, || authorize(&self.state.grant)).map_err(storage)?;
        store.store_io_material(&expected.subject, MaterialKind::Request, &original,
            || authorize(&self.state.grant)).map_err(storage)?;
        store.reserve_io_intent_followup(expected, || authorize(&self.state.grant)).map_err(storage)?;
        let boundary = prepared.propose_dispatch_boundary().map_err(storage)?;
        store.claim_io_dispatch_local_authorized(&boundary, || authorize(&self.state.grant))
            .map_err(storage)?;
        // After this commit all outcomes remain historically Unknown. Neither
        // thread-start failure nor EOF can undo it or authorize automatic replay.
        self.state.grant.check().map_err(Error::Source)?;
        let state = Arc::clone(&self.state);
        broker.spawn(move |producer| run_source(state, producer)).map_err(Error::Source)
    }
}
fn authorize(grant: &SourceGrant) -> morrow_core::Result<()> {
    grant.check().map_err(|_| morrow_core::Error::Invalid("SSE source no longer approved"))
}
fn storage(error: morrow_core::Error) -> Error {
    match error {
        morrow_core::Error::Limit => Error::Limit,
        morrow_core::Error::OperationConflict | morrow_core::Error::RevisionConflict => Error::Conflict,
        morrow_core::Error::CommitUnknown => Error::CommitUnknown,
        morrow_core::Error::Invalid(_) | morrow_core::Error::UnsupportedVersion => Error::Invalid,
        _ => Error::Storage,
    }
}
fn publish(state: &State, receipt: Completion) {
    let mut slot = state.completion.lock().unwrap_or_else(|e| e.into_inner());
    // Cleanup never replaces an earlier start/source failure receipt.
    if slot.is_none() { *slot = Some(Arc::new(receipt)); }
    state.changed.notify_all();
}
struct Guard { grant: SourceGrant, cancel: CancellationToken }
impl stream::StreamGuard for Guard {
    fn check(&self) -> crate::Result<()> {
        match self.grant.check() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.cancel.cancel();
                Err(match error {
                    morrow_plugin_runtime::channel::Error::Expired => crate::Error::Timeout,
                    morrow_plugin_runtime::channel::Error::Closed => crate::Error::Closed,
                    _ => crate::Error::Denied,
                })
            }
        }
    }
}
fn run_source(state: Arc<State>, producer: Producer) {
    // Blocking channel ACK waits happen only on this original native thread.
    // A dedicated runtime worker continues HTTP cancellation/guard polling while
    // that thread waits; no shared executor or Store lock is blocked by the wait.
    let runtime = match tokio::runtime::Builder::new_multi_thread().worker_threads(1)
        .enable_all().build() {
        Ok(runtime) => runtime,
        Err(_) => { drop(producer); publish(&state, Completion::empty(Err(Error::Thread))); return; }
    };
    let guard = Arc::new(Guard { grant: state.grant.clone(), cancel: state.cancel.clone() });
    let context = stream::SendContext::guarded(tokio::time::Instant::from_std(state.approval.deadline),
        state.cancel.clone(), guard);
    let opened = runtime.block_on(state.client.send_stream_with_receipt(
        state.approval.request.clone(), context));
    let stream = match opened {
        Ok(stream) => stream,
        Err(error) => {
            let mut receipt = Completion::empty(Err(Error::Transport(error.cause)));
            if let stream::OpeningCleanup::Worker(transport) = &error.cleanup {
                receipt.transport = Some(transport.clone());
            }
            receipt.opening_cleanup = Some(error.cleanup);
            drop(producer);
            publish(&state, receipt);
            return;
        }
    };
    let mut lease = match runtime.block_on(sse::SseLease::new(stream, state.approval.decoder_limits)) {
        Ok(lease) => lease,
        Err(error) => {
            let mut receipt = Completion::empty(Err(Error::Sse(error.cause)));
            receipt.transport = Some(error.transport);
            drop(producer);
            publish(&state, receipt);
            return;
        }
    };
    let mut receipt = Completion::empty(Ok(()));
    let mut first = None;
    loop {
        if let Err(error) = state.grant.check() { first = Some(Error::Source(error)); break; }
        match runtime.block_on(lease.next_event()) {
            Ok(Some(event)) => {
                let bytes = match EventEnvelope::from(event).encode() {
                    Ok(bytes) => bytes,
                    Err(error) => { first = Some(error); break; }
                };
                let charge = bytes.len() as u64;
                let Some(total) = receipt.encoded_bytes.checked_add(charge) else {
                    first = Some(Error::Limit); break;
                };
                if bytes.len() > state.frame_limit || total > state.approval.max_encoded_bytes {
                    first = Some(Error::Limit); break;
                }
                // Reserve once before push; successful ACK never refunds it.
                receipt.encoded_bytes = total;
                if let Err(error) = state.grant.check() { first = Some(Error::Source(error)); break; }
                let ordinal = receipt.events_queued + 1;
                let cursor = event_cursor(&state, ordinal, &bytes);
                match producer.push(bytes, cursor.to_vec()) {
                    Ok(()) => receipt.events_queued = ordinal,
                    Err(error) => { first = Some(Error::Source(error)); break; }
                }
                // One fresh broker starts at sequence zero, and this is its only
                // producer. No second SSE event is requested until this exact
                // frame's real original Store consumption ACK has committed.
                if let Err(error) = producer.wait_acked(ordinal) {
                    first = Some(Error::Source(error)); break;
                }
                receipt.events_acked += 1;
            }
            Ok(None) => break,
            Err(error) => { first = Some(Error::Sse(error)); break; }
        }
    }
    let completion = if first.is_some() {
        runtime.block_on(lease.cancel_and_wait())
    } else {
        runtime.block_on(lease.finish())
    };
    if first.is_none() {
        if let Err(error) = completion.outcome { first = Some(Error::Sse(error)); }
        else if completion.truncated || !completion.decoder_reached_http_eof
            || !completion.transport.http_eof || !completion.transport.worker_joined
            || !completion.decoder.as_ref().is_ok_and(|finish| !finish.truncated)
        { first = Some(Error::Truncated); }
    }
    receipt.transport = Some(completion.transport.clone());
    receipt.sse = Some(completion);
    // HTTP/parser EOF and successful ACKs still do not prove business/model
    // completion, or permit an IO Observed record for a streaming operation.
    if let Some(error) = first {
        receipt.outcome = Err(error);
        drop(producer);
    } else {
        receipt.outcome = producer.finish().map_err(Error::Source);
    }
    publish(&state, receipt);
}
fn event_cursor(state: &State, ordinal: u64, bytes: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"Morrow/managed-sse/cursor/v1\0");
    hash.update(state.cursor_epoch);
    hash.update(state.approval.approval_epoch);
    hash.update(state.grant.expected_command().request_sha256);
    hash.update(ordinal.to_be_bytes());
    hash.update(Sha256::digest(bytes));
    hash.finalize().into()
}
fn field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_be_bytes()); hash.update(value);
}
fn prepare(approval: &Approval, broker: &ChannelBroker) -> Result<(Client, io::Request, [u8;32], [u8;32])> {
    approval.limits.validate().map_err(Error::Transport)?;
    sse::decoder::Decoder::new(approval.decoder_limits).map_err(|error| Error::Sse(sse::Error::Decoder(error)))?;
    let endpoint = broker.endpoint();
    let remaining = approval.deadline.checked_duration_since(Instant::now()).ok_or(Error::Invalid)?;
    if approval.approval_epoch == [0;32] || approval.request.method != "POST"
        || remaining > approval.limits.timeout
        || approval.max_encoded_bytes == 0 || approval.max_encoded_bytes > endpoint.budget.max_bytes
        || approval.request.body.len() > approval.limits.max_request_bytes
        || approval.request.body.len() > io::MAX_PAYLOAD_BYTES
        || approval.decoder_limits.max_line_bytes > MAX_ENVELOPE_BYTES
        || approval.decoder_limits.max_event_bytes > MAX_ENVELOPE_BYTES
        || approval.decoder_limits.max_total_bytes > approval.limits.max_response_bytes
        || approval.decoder_limits.max_events as u64 > endpoint.budget.max_messages
        || approval.limits.max_response_bytes as u64 > morrow_core::plugin_package::io::MAX_JOB_BYTES
    { return Err(Error::Limit); }
    // Reject normalization and hidden authority before hashing the exact URL.
    let raw = &approval.request.target;
    if raw.len() > 8192 || raw.chars().any(|c| c.is_control() || c.is_whitespace() || c == '\\') {
        return Err(Error::Invalid);
    }
    let target = url::Url::parse(raw).map_err(|_| Error::Invalid)?;
    if target.as_str() != raw || !target.username().is_empty() || target.password().is_some()
        || target.fragment().is_some() || target.host_str().is_none()
    { return Err(Error::Invalid); }
    let origin = target.origin().ascii_serialization();
    let policy = match approval.profile {
        NetworkProfile::PublicHttps => {
            if target.scheme() != "https" { return Err(Error::Invalid); }
            EndpointPolicy::new(&origin, &["POST"], false)
        }
        NetworkProfile::LoopbackHttp => {
            if target.scheme() != "http" || !is_loopback(&target) { return Err(Error::Invalid); }
            EndpointPolicy::new(&origin, &["POST"], true)
        }
        NetworkProfile::LoopbackHttps => EndpointPolicy::local_https(&origin, &["POST"]),
    }.map_err(Error::Transport)?;
    let client = Client::new(policy, approval.limits).map_err(Error::Transport)?;
    let mut header_bytes = 0usize;
    for (name, value) in &approval.request.headers {
        // This slice has no account/credential injection contract. Even a host
        // cannot smuggle one through the raw Authorization/Cookie escape hatch.
        if matches!(name.to_ascii_lowercase().as_str(), "authorization" | "cookie"
            | "proxy-authorization" | "last-event-id" | "x-api-key" | "api-key") {
            return Err(Error::UnsupportedCredentials);
        }
        reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| Error::Invalid)?;
        reqwest::header::HeaderValue::from_bytes(value).map_err(|_| Error::Invalid)?;
        header_bytes = header_bytes.checked_add(name.len()).and_then(|n| n.checked_add(value.len()))
            .and_then(|n| n.checked_add(4)).ok_or(Error::Limit)?;
        if header_bytes > approval.limits.max_header_bytes { return Err(Error::Limit); }
    }
    let mut target_hash = Sha256::new();
    target_hash.update(b"Morrow/managed-sse/target/v1\0");
    target_hash.update([approval.profile as u8]); field(&mut target_hash, raw.as_bytes());
    let target_digest: [u8;32] = target_hash.finalize().into();
    let mut relative = target.path().to_owned();
    if let Some(query) = target.query() { relative.push('?'); relative.push_str(query); }
    let reference = hex(&target_digest);
    let request = io::Request::encode_http_submit(1, &io::HttpSubmission {
        operation_id: approval.operation_id.as_bytes().to_vec(),
        // Immutable approved duration encoded in the retained logical request;
        // execution is restricted by the original absolute grant deadline.
        deadline_ms: approval.limits.timeout.as_millis().try_into().map_err(|_| Error::Limit)?,
        endpoint: reference.into_bytes(), method: "POST".into(), relative_target: relative,
        headers: approval.request.headers.iter().map(|(name,value)| io::Header {
            name: name.clone(), value: value.clone() }).collect(),
        body: approval.request.body.clone(), credential: Vec::new(),
    }).map_err(storage)?;
    let mut hash = Sha256::new();
    hash.update(b"Morrow/managed-sse/approval/v1\0");
    hash.update(approval.approval_epoch); hash.update(target_digest); hash.update(request.digest());
    // Process-local Instant pin distinguishes a changed deadline on another
    // approval. It is historical metadata, never portable live authority.
    static CLOCK_ORIGIN: OnceLock<Instant> = OnceLock::new();
    let origin = *CLOCK_ORIGIN.get_or_init(Instant::now);
    let tick = approval.deadline.checked_duration_since(origin).ok_or(Error::Invalid)?;
    hash.update(tick.as_nanos().to_be_bytes());
    for value in [approval.limits.max_request_bytes as u64, approval.limits.max_response_bytes as u64,
        approval.limits.max_header_bytes as u64, approval.limits.max_concurrent as u64,
        approval.max_encoded_bytes, approval.decoder_limits.max_line_bytes as u64,
        approval.decoder_limits.max_event_bytes as u64, approval.decoder_limits.max_total_bytes as u64,
        approval.decoder_limits.max_events as u64, approval.decoder_limits.max_id_bytes as u64,
        approval.decoder_limits.max_retry_digits as u64] { hash.update(value.to_be_bytes()); }
    Ok((client, request, target_digest, hash.finalize().into()))
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes { write!(&mut result, "{byte:02x}").expect("String write"); }
    result
}
fn is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}
