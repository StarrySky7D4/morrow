use crate::{
    admission::{Admission, progress_follows},
    delivery::{Available, Delivery},
};
use bytes::Bytes;
use morrow_codex_m03_stream::driver::*;
use morrow_native_http_stream_wire as wire;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio::sync::Notify;

pub(crate) struct State {
    pub admission: Admission,
    pub outbound: VecDeque<(wire::Kind, wire::Payload)>,
    pub requests: BTreeMap<u64, wire::Kind>,
    pub channel: Option<wire::Channel>,
    pub data_bound: bool,
    pub prepare: Option<wire::Prepare>,
    pub body: Option<Bytes>,
    pub prepare_acked: bool,
    pub body_offset: usize,
    pub proposal: Option<wire::Decision>,
    pub approved: Option<wire::Decision>,
    pub expected_request_hash: Option<[u8; 32]>,
    pub commit_requested: bool,
    pub commit_acked: bool,
    pub head: Option<wire::Head>,
    pub progress: Option<wire::Progress>,
    pub delivery: Option<Delivery>,
    pub fatal: Option<BridgeError>,
    pub cancel_requested: bool,
    pub cancel_written: bool,
    pub credit_dirty: bool,
    pub last_credit: Option<wire::Credit>,
    pub closed: Option<wire::Progress>,
    pub control_eof: bool,
    pub close_written: bool,
    pub close_acked: bool,
    pub max_chunk: usize,
    pub io_result: Option<std::result::Result<(), &'static str>>,
    pub io_joined: bool,
    pub io_events: Vec<IoObservation>,
    pub io_events_overflow: bool,
    pub frame_events: Vec<FrameObservation>,
    pub frame_events_overflow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn dropping_pending_read_wait_does_not_lose_session_owned_chunk() {
        // Local queue/async ownership only; no pipe, approval, HTTP or Core run.
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        session.shared.state.lock().unwrap().delivery = Some(Delivery::new(3, 1024).unwrap());
        {
            let mut wait = session.read(RequestKey(1), 0, 1024);
            assert!(
                std::future::poll_fn(|cx| match wait.as_mut().poll(cx) {
                    std::task::Poll::Pending => std::task::Poll::Ready(true),
                    std::task::Poll::Ready(_) => std::task::Poll::Ready(false),
                })
                .await
            );
        }
        {
            let mut state = session.shared.state.lock().unwrap();
            let delivery = state.delivery.as_mut().unwrap();
            delivery.http_eof(3).unwrap();
            delivery.received(0, Bytes::from_static(b"abc")).unwrap();
        }
        session.shared.signal();
        assert!(
            matches!(session.read(RequestKey(1),0,1024).await.unwrap(),ReadResult::Chunk(chunk) if chunk.offset==0&&chunk.bytes.as_ref()==b"abc")
        );
        assert!(matches!(
            session.read(RequestKey(1), 3, 1024).await.unwrap(),
            ReadResult::Eof { offset: 3 }
        ));
    }
}
#[derive(Clone, serde::Serialize)]
pub struct FrameObservation {
    pub lane: &'static str,
    pub kind: String,
    pub sequence: u64,
    pub generation: u64,
    pub code: u32,
}
#[derive(serde::Serialize)]
pub struct IdentityObservation {
    pub session: u64,
    pub epoch: u64,
    pub child_pid: u32,
    pub attempt: u64,
    pub operation_id_sha256: String,
    pub schema_sha256: String,
    pub artifact_sha256: String,
    pub host_execution_config_sha256: String,
    pub original_remaining_ms: u64,
}
#[derive(Clone, serde::Serialize)]
pub struct IoObservation {
    pub action: &'static str,
    pub id: u64,
    pub kind: String,
    pub bytes: usize,
    pub pending: bool,
    pub error: Option<u32>,
}
impl State {
    pub(crate) fn io_event(&mut self, event: IoObservation) {
        if self.io_events.len() >= 4096 {
            self.io_events_overflow = true
        } else {
            self.io_events.push(event)
        }
    }
    pub(crate) fn record(&mut self, lane: &'static str, frame: &wire::Frame) {
        if self.frame_events.len() >= 512 {
            self.frame_events_overflow = true;
            return;
        }
        self.frame_events.push(FrameObservation {
            lane,
            kind: format!("{:?}", frame.kind),
            sequence: frame.sequence,
            generation: frame.revocation_generation,
            code: frame.code,
        });
    }
    pub(crate) fn cancel(&mut self) {
        self.admission.cancel();
        self.cancel_requested = true;
        if let Some(delivery) = &mut self.delivery {
            delivery.cancel();
            self.credit_dirty = true;
        }
        // A partially issued control frame completes on its sole writer; queued
        // Commit/positive Credit/Prepare must not become fresh work after cancel.
        self.outbound
            .retain(|(kind, _)| matches!(kind, wire::Kind::Query | wire::Kind::Close));
    }
    pub(crate) fn fail(&mut self, error: BridgeError) {
        if self.fatal.is_none() {
            self.fatal = Some(error);
        }
        self.cancel();
    }
    pub(crate) fn queue(
        &mut self,
        kind: wire::Kind,
        payload: wire::Payload,
    ) -> Result<(), BridgeError> {
        if self.outbound.len() >= 8 {
            return Err(BridgeError::Protocol("bounded control queue"));
        }
        self.outbound.push_back((kind, payload));
        Ok(())
    }
    pub(crate) fn accept_control(&mut self, frame: wire::Frame) -> Result<(), BridgeError> {
        let request_kind = self.requests.get(&frame.sequence).copied();
        self.admission
            .host_control(&frame)
            .map_err(BridgeError::Protocol)?;
        self.record("host_control", &frame);
        if frame.sequence != 0 {
            self.requests.remove(&frame.sequence);
        }
        if matches!(frame.kind, wire::Kind::Stop | wire::Kind::Denied) {
            self.fail(if frame.code == 20 {
                BridgeError::Deadline
            } else {
                BridgeError::Denied
            });
            return Ok(());
        }
        if self.admission.cancelled() {
            self.cancel();
        }
        match (frame.kind, frame.payload) {
            (wire::Kind::Welcome, wire::Payload::None) => {}
            (wire::Kind::DataOffer, wire::Payload::Channel(channel)) => {
                if self.channel.is_some() || channel.nonce == self.admission.initial().nonce {
                    return Err(BridgeError::Protocol("duplicate/bad data offer"));
                }
                if self.max_chunk > channel.max_chunk_bytes as usize {
                    return Err(BridgeError::Protocol(
                        "offered max chunk below configured bound",
                    ));
                }
                self.channel = Some(channel);
            }
            (wire::Kind::HttpProposed, wire::Payload::Decision(decision)) => {
                let prepare = self
                    .prepare
                    .as_ref()
                    .ok_or(BridgeError::Protocol("unsolicited proposal"))?;
                if self.proposal.is_some()
                    || self.expected_request_hash.as_ref().map(|h| h.as_slice())
                        != Some(decision.request_sha256.as_slice())
                    || decision.body_sha256 != prepare.body_sha256
                    || decision.body_bytes != prepare.body_bytes
                    || decision.response_limit_bytes != prepare.response_limit_bytes
                {
                    return Err(BridgeError::Protocol("proposal bytes/hash/limits"));
                }
                self.proposal = Some(decision);
            }
            (wire::Kind::HttpApproved, wire::Payload::Decision(decision)) => {
                let proposed = self
                    .proposal
                    .as_ref()
                    .ok_or(BridgeError::Protocol("approval before proposal"))?;
                if self.approved.is_some()
                    || decision.proposal_ref != proposed.proposal_ref
                    || decision.request_sha256 != proposed.request_sha256
                    || decision.body_sha256 != proposed.body_sha256
                    || decision.body_bytes != proposed.body_bytes
                    || decision.response_limit_bytes > proposed.response_limit_bytes
                {
                    return Err(BridgeError::Protocol(
                        "approval changed proposal or expanded cap",
                    ));
                }
                self.delivery = Some(
                    Delivery::new(decision.response_limit_bytes as usize, self.max_chunk)
                        .map_err(BridgeError::Protocol)?,
                );
                self.approved = Some(decision);
            }
            (wire::Kind::ResponseHead, wire::Payload::Head(head)) => {
                if !self.commit_acked || self.head.is_some() {
                    return Err(BridgeError::Protocol("head without accepted unique commit"));
                }
                let actual: std::net::SocketAddr = head
                    .remote_address
                    .parse()
                    .map_err(|_| BridgeError::Protocol("remote address"))?;
                let target: http::Uri = self
                    .prepare
                    .as_ref()
                    .ok_or(BridgeError::Protocol("head without request"))?
                    .absolute_target
                    .parse()
                    .map_err(|_| BridgeError::Protocol("target URI"))?;
                if actual.ip() != std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                    || Some(actual.port()) != target.port_u16()
                {
                    return Err(BridgeError::Protocol("actual remote endpoint"));
                }
                self.head = Some(head);
                if !self.cancel_requested {
                    self.credit_dirty = true;
                }
            }
            (kind, wire::Payload::Progress(progress)) => {
                if let Some(previous) = &self.progress {
                    progress_follows(previous, &progress).map_err(BridgeError::Protocol)?;
                }
                if let Some(approved) = &self.approved {
                    if progress.received_offset > approved.response_limit_bytes as u64 {
                        return Err(BridgeError::Protocol("host response cap"));
                    }
                }
                if kind == wire::Kind::State && frame.code == 0 {
                    if request_kind == Some(wire::Kind::HttpPrepare) {
                        self.prepare_acked = true;
                    }
                    if request_kind == Some(wire::Kind::Close) {
                        self.close_acked = true;
                    }
                    if request_kind == Some(wire::Kind::HttpCommit) {
                        if !matches!(
                            progress.intent,
                            wire::IntentPhase::Unknown | wire::IntentPhase::Observed
                        ) {
                            return Err(BridgeError::Protocol("commit lacks durable claim"));
                        }
                        self.commit_acked = true;
                    }
                }
                if progress.http_eof {
                    if let Some(delivery) = &mut self.delivery {
                        delivery
                            .http_eof(progress.received_offset)
                            .map_err(BridgeError::Protocol)?;
                    }
                }
                if kind == wire::Kind::RequestClosed {
                    if self.closed.is_some() {
                        return Err(BridgeError::Protocol("duplicate request closed"));
                    }
                    self.closed = Some(progress.clone());
                }
                let transport_failed = progress.error_code != 0
                    || matches!(
                        progress.network,
                        wire::NetworkPhase::Failed | wire::NetworkPhase::Cancelled
                    );
                self.progress = Some(progress);
                // Evidence is retained even when delivery subsequently fails.
                if transport_failed {
                    self.fail(BridgeError::Unknown);
                }
            }
            _ => return Err(BridgeError::Protocol("unexpected control payload/state")),
        }
        if self.cancel_requested {
            if let Some(delivery) = &mut self.delivery {
                delivery.cancel();
            }
        }
        Ok(())
    }
    pub(crate) fn accept_data(&mut self, frame: wire::Frame) -> Result<(), BridgeError> {
        let deliver = self
            .admission
            .host_data(&frame)
            .map_err(BridgeError::Protocol)?;
        self.record("host_data", &frame);
        match (frame.kind, frame.payload) {
            (wire::Kind::DataBound, wire::Payload::Channel(channel)) => {
                if self.data_bound || self.channel.as_ref() != Some(&channel) {
                    return Err(BridgeError::Protocol("DataBound channel binding"));
                }
                self.data_bound = deliver;
            }
            (wire::Kind::BodyChunk, wire::Payload::Chunk(chunk)) => {
                if !self.commit_requested || !self.data_bound {
                    return Err(BridgeError::Protocol("body before commit/binding"));
                }
                let delivery = self
                    .delivery
                    .as_mut()
                    .ok_or(BridgeError::Protocol("body without approval"))?;
                if !deliver {
                    delivery.cancel();
                }
                delivery
                    .received(chunk.offset, Bytes::from(chunk.bytes))
                    .map_err(BridgeError::Protocol)?;
            }
            _ => return Err(BridgeError::Protocol("unexpected data payload")),
        }
        Ok(())
    }
}

pub(crate) struct Shared {
    pub state: Mutex<State>,
    pub changed: Notify,
    pub work: Condvar,
}
impl Shared {
    pub fn signal(&self) {
        self.changed.notify_waiters();
        self.work.notify_all();
    }
    pub fn fail(&self, error: BridgeError) {
        self.state.lock().unwrap().fail(error);
        self.signal();
    }
}

pub struct Session {
    pub(crate) shared: Arc<Shared>,
    pub(crate) io_thread: Mutex<Option<JoinHandle<Result<(), &'static str>>>>,
    key: RequestKey,
}
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeV3Session").finish_non_exhaustive()
    }
}
impl Session {
    pub fn identity(&self) -> IdentityObservation {
        let state = self.shared.state.lock().unwrap();
        let f = state.admission.initial();
        IdentityObservation {
            session: f.session,
            epoch: f.instance_epoch,
            child_pid: f.child_pid,
            attempt: f.attempt,
            operation_id_sha256: wire::hex(&wire::digest(&f.operation_id)),
            schema_sha256: wire::hex(&f.schema_sha256),
            artifact_sha256: wire::hex(&f.artifact_sha256),
            host_execution_config_sha256: wire::hex(&f.execution_config_sha256),
            original_remaining_ms: f.remaining_ms,
        }
    }
    pub fn new(admission: Admission, max_chunk: usize, key: RequestKey) -> Arc<Self> {
        Arc::new(Self {
            key,
            io_thread: Mutex::new(None),
            shared: Arc::new(Shared {
                changed: Notify::new(),
                work: Condvar::new(),
                state: Mutex::new(State {
                    admission,
                    outbound: VecDeque::from([(wire::Kind::Hello, wire::Payload::None)]),
                    requests: BTreeMap::new(),
                    channel: None,
                    data_bound: false,
                    prepare: None,
                    body: None,
                    prepare_acked: false,
                    body_offset: 0,
                    proposal: None,
                    approved: None,
                    expected_request_hash: None,
                    commit_requested: false,
                    commit_acked: false,
                    head: None,
                    progress: None,
                    delivery: None,
                    fatal: None,
                    cancel_requested: false,
                    cancel_written: false,
                    credit_dirty: false,
                    last_credit: None,
                    closed: None,
                    control_eof: false,
                    close_written: false,
                    close_acked: false,
                    max_chunk,
                    io_result: None,
                    io_joined: false,
                    io_events: Vec::new(),
                    io_events_overflow: false,
                    frame_events: Vec::new(),
                    frame_events_overflow: false,
                }),
            }),
        })
    }
    pub fn deadline(&self) -> Instant {
        self.shared.state.lock().unwrap().admission.deadline()
    }
    fn key(&self, key: RequestKey) -> Result<(), BridgeError> {
        if key == self.key {
            Ok(())
        } else {
            Err(BridgeError::Protocol("local request key"))
        }
    }
    async fn wait<T>(
        &self,
        allow_cleanup: bool,
        mut inspect: impl FnMut(&mut State) -> Result<Option<T>, BridgeError>,
    ) -> Result<T, BridgeError> {
        loop {
            // Register before inspection; Notify alone is not the state. The
            // periodic wake also bounds missing notifications and expiry checks.
            let changed = self.shared.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut state = self.shared.state.lock().unwrap();
                if !allow_cleanup {
                    if Instant::now() >= state.admission.deadline() {
                        state.fail(BridgeError::Deadline);
                        self.shared.signal();
                    }
                    if let Some(error) = &state.fatal {
                        return Err(error.clone());
                    }
                    if state.cancel_requested {
                        return Err(BridgeError::Cancelled);
                    }
                }
                if let Some(value) = inspect(&mut state)? {
                    return Ok(value);
                }
                if allow_cleanup && state.control_eof && state.closed.is_none() {
                    return Err(BridgeError::CleanupUnconfirmed);
                }
            }
            tokio::select! { _ = &mut changed => {}, _ = tokio::time::sleep(Duration::from_millis(10)) => {} }
        }
    }
    pub fn observations(&self) -> (Vec<FrameObservation>, bool) {
        let s = self.shared.state.lock().unwrap();
        (s.frame_events.clone(), s.frame_events_overflow)
    }
    pub fn io_observations(&self) -> (Vec<IoObservation>, bool, bool) {
        let s = self.shared.state.lock().unwrap();
        (s.io_events.clone(), s.io_events_overflow, s.io_joined)
    }
    pub fn close(&self) -> Result<(), BridgeError> {
        self.shared
            .state
            .lock()
            .unwrap()
            .queue(wire::Kind::Close, wire::Payload::None)?;
        self.shared.signal();
        Ok(())
    }
    pub async fn wait_close(&self) -> Result<(), BridgeError> {
        self.wait(true, |s| {
            if s.control_eof && !s.close_acked {
                return Err(BridgeError::CleanupUnconfirmed);
            }
            Ok(s.close_acked.then_some(()))
        })
        .await
    }
}
impl NativeHttpSession for Session {
    fn prepare(
        &self,
        key: RequestKey,
        request: PreparedHttpRequest,
    ) -> DriverFuture<'_, ProposalRef> {
        Box::pin(async move {
            self.key(key)?;
            self.wait(false, |state| Ok(state.data_bound.then_some(())))
                .await?;
            {
                let mut state = self.shared.state.lock().unwrap();
                if state.prepare.is_some() {
                    return Err(BridgeError::DuplicateRequest);
                }
                let p = wire::Prepare {
                    method: request.method,
                    absolute_target: request.url,
                    headers: request
                        .headers
                        .into_iter()
                        .map(|(name, value)| wire::Header { name, value })
                        .collect(),
                    body_bytes: request
                        .body
                        .len()
                        .try_into()
                        .map_err(|_| BridgeError::InvalidInput("body size"))?,
                    body_sha256: request.body_sha256.to_vec(),
                    response_limit_bytes: request
                        .response_limit
                        .try_into()
                        .map_err(|_| BridgeError::InvalidInput("response cap"))?,
                };
                let initial = state.admission.initial();
                let hash = wire::request_digest(
                    initial.session,
                    initial.instance_epoch,
                    &initial.operation_id,
                    initial.attempt,
                    &p,
                    &request.body,
                )
                .map_err(BridgeError::Protocol)?;
                state.queue(wire::Kind::HttpPrepare, wire::Payload::Prepare(p.clone()))?;
                state.expected_request_hash = Some(hash);
                state.prepare = Some(p);
                state.body = Some(request.body);
            }
            self.shared.signal();
            self.wait(false, |s| {
                Ok(s.proposal
                    .as_ref()
                    .map(|d| ProposalRef(wire::hex(&d.proposal_ref))))
            })
            .await
        })
    }
    fn await_approval(
        &self,
        key: RequestKey,
        proposal: ProposalRef,
    ) -> DriverFuture<'_, ApprovedProposal> {
        Box::pin(async move {
            self.key(key)?;
            self.wait(false, |s| {
                let Some(d) = &s.approved else {
                    return Ok(None);
                };
                if wire::hex(&d.proposal_ref) != proposal.0 {
                    return Err(BridgeError::Protocol("approval proposal ref"));
                }
                Ok(Some(ApprovedProposal {
                    proposal: proposal.clone(),
                    response_limit: d.response_limit_bytes as usize,
                }))
            })
            .await
        })
    }
    fn commit(
        &self,
        key: RequestKey,
        approved: ApprovedProposal,
    ) -> DriverFuture<'_, ResponseHead> {
        Box::pin(async move {
            self.key(key)?;
            {
                let mut s = self.shared.state.lock().unwrap();
                if s.commit_requested {
                    return Err(BridgeError::DuplicateRequest);
                }
                let decision = s.approved.clone().ok_or(BridgeError::Denied)?;
                if wire::hex(&decision.proposal_ref) != approved.proposal.0
                    || decision.response_limit_bytes as usize != approved.response_limit
                {
                    return Err(BridgeError::Protocol("commit exact approval"));
                }
                s.commit_requested = true;
                s.queue(wire::Kind::HttpCommit, wire::Payload::Decision(decision))?;
            }
            self.shared.signal();
            self.wait(false, |s| {
                let Some(head) = &s.head else { return Ok(None) };
                let mut headers = http::HeaderMap::new();
                for h in &head.headers {
                    headers.append(
                        http::HeaderName::from_bytes(h.name.as_bytes())
                            .map_err(|_| BridgeError::Protocol("header name"))?,
                        http::HeaderValue::from_bytes(&h.value)
                            .map_err(|_| BridgeError::Protocol("header value"))?,
                    );
                }
                Ok(Some(ResponseHead {
                    status: http::StatusCode::from_u16(head.status)
                        .map_err(|_| BridgeError::Protocol("status"))?,
                    headers,
                }))
            })
            .await
        })
    }
    fn read(&self, key: RequestKey, offset: u64, max: usize) -> DriverFuture<'_, ReadResult> {
        Box::pin(async move {
            self.key(key)?;
            let value = self
                .wait(false, |s| {
                    match s
                        .delivery
                        .as_mut()
                        .ok_or(BridgeError::Protocol("read without approval"))?
                        .take(offset, max)
                        .map_err(BridgeError::Protocol)?
                    {
                        Available::Bytes(bytes) => {
                            Ok(Some(ReadResult::Chunk(BodyChunk { offset, bytes })))
                        }
                        Available::Eof => Ok(Some(ReadResult::Eof { offset })),
                        Available::Pending => Ok(None),
                    }
                })
                .await;
            self.shared.signal();
            value
        })
    }
    fn acknowledge(
        &self,
        key: RequestKey,
        progress: ConsumptionProgress,
    ) -> Result<(), BridgeError> {
        self.key(key)?;
        {
            let mut s = self.shared.state.lock().unwrap();
            s.delivery
                .as_mut()
                .ok_or(BridgeError::Protocol("ACK without delivery"))?
                .consumed(progress)
                .map_err(BridgeError::Protocol)?;
            s.credit_dirty = true;
        }
        self.shared.signal();
        Ok(())
    }
    fn cancel(&self, key: RequestKey, _reason: CancelReason) -> Result<(), BridgeError> {
        self.key(key)?;
        self.shared.state.lock().unwrap().cancel();
        self.shared.signal();
        Ok(())
    }
    fn cleanup(&self, key: RequestKey) -> DriverFuture<'_, CleanupReceipt> {
        Box::pin(async move {
            self.key(key)?;
            let p = self
                .wait(true, |s| {
                    if s.io_result.as_ref().is_some_and(|r| r.is_err()) {
                        return Err(BridgeError::CleanupUnconfirmed);
                    }
                    Ok(if s.io_result == Some(Ok(())) {
                        s.closed.clone()
                    } else {
                        None
                    })
                })
                .await?;
            // Do not block a Tokio worker on a live owned-I/O thread. is_finished
            // gates join; observation remains bounded by the Core cleanup timer.
            loop {
                let joined = {
                    let mut slot = self.io_thread.lock().unwrap();
                    if slot.as_ref().is_none_or(|h| h.is_finished()) {
                        if let Some(handle) = slot.take() {
                            if !matches!(handle.join(), Ok(Ok(()))) {
                                return Err(BridgeError::CleanupUnconfirmed);
                            }
                        }
                        true
                    } else {
                        false
                    }
                };
                if joined {
                    self.shared.state.lock().unwrap().io_joined = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            Ok(CleanupReceipt {
                network_eof: p.http_eof,
                durable_observed: p.intent == wire::IntentPhase::Observed,
                network_worker_exited: p.worker_joined,
                network_worker_started: p.worker_started,
                data_connect_reaped: p.connect_reaped,
                data_read_reaped: p.read_reaped,
                data_write_reaped: p.write_reaped,
                data_channel_closed: p.data_closed,
                request_closed: p.request_closed,
                session_release: None,
                received_offset: p.received_offset,
                delivered_offset: self
                    .shared
                    .state
                    .lock()
                    .unwrap()
                    .delivery
                    .as_ref()
                    .map_or(0, Delivery::delivered_offset),
                acknowledged_offset: p.peer_consumed_offset,
            })
        })
    }
}
