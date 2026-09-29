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
    pub control_failure: Option<ControlFailureObservation>,
    pub cancel_requested: bool,
    pub cancel_reason: Option<CancelReason>,
    pub cancellation: Option<CancellationSignal>,
    pub cancel_written: bool,
    pub credit_dirty: bool,
    pub last_credit: Option<wire::Credit>,
    pub closed: Option<wire::Progress>,
    pub control_eof: bool,
    pub control_stream_ended: bool,
    pub close_written: bool,
    pub close_acked: bool,
    pub close_ack: Option<CloseAckObservation>,
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
    fn terminal_progress() -> wire::Progress {
        wire::Progress {
            intent: wire::IntentPhase::Observed,
            network: wire::NetworkPhase::Eof,
            owner: wire::OwnerPhase::Active,
            http_status: 200,
            error_code: 0,
            received_offset: 0,
            reserved_offset: 0,
            issued_offset: 0,
            os_completed_offset: 0,
            peer_consumed_offset: 0,
            parser_yielded_bytes: 0,
            drain_discarded_bytes: 0,
            cancel_discarded_bytes: 0,
            error_consumed_bytes: 0,
            last_write_ordinal: 0,
            revoke_persisted: false,
            revoke_applied: false,
            http_eof: true,
            response_material_stored: true,
            worker_joined: false,
            connect_reaped: true,
            read_reaped: true,
            write_reaped: true,
            data_closed: true,
            child_exited: false,
            stdout_eof: false,
            stderr_eof: false,
            owner_released: false,
            worker_started: false,
            request_closed: true,
        }
    }
    #[tokio::test]
    async fn decoded_host_stop_denial_and_revoke_synchronously_close_core_event_gate() {
        for kind in [
            wire::Kind::Stop,
            wire::Kind::Denied,
            wire::Kind::HttpTerminal,
        ] {
            let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
            let op = morrow_codex_m03_stream::network::Operation::new(
                session.clone(),
                RequestKey(1),
                "http://127.0.0.1:12345/v1",
                tokio::time::Instant::now() + Duration::from_secs(30),
                1024,
            )
            .unwrap();
            op.observe_core_completion(Some(false));
            let (tx, mut rx) = tokio::sync::mpsc::channel(2);
            tx.send(codex_core::ResponseEvent::OutputTextDelta("queued".into()))
                .await
                .unwrap();
            let permit = tx.reserve().await.unwrap();
            let mut frame = session
                .shared
                .state
                .lock()
                .unwrap()
                .admission
                .initial()
                .clone();
            frame.kind = kind;
            frame.sequence = 0;
            frame.code = 19;
            if kind == wire::Kind::HttpTerminal {
                frame.revocation_generation = 2;
                let mut p = terminal_progress();
                p.intent = wire::IntentPhase::Unknown;
                p.network = wire::NetworkPhase::Cancelled;
                p.owner = wire::OwnerPhase::Revoked;
                p.http_eof = false;
                p.response_material_stored = false;
                p.request_closed = false;
                p.error_code = 19;
                p.revoke_persisted = true;
                p.revoke_applied = true;
                frame.payload = wire::Payload::Progress(p);
            }
            let decoded = wire::Frame::decode(&frame.encode().unwrap()).unwrap();
            session.shared.handle_control(decoded);
            assert!(
                op.token().is_cancelled(),
                "host application must not wait for a network read"
            );
            assert!(op.deliver_if_live(|| rx.try_recv()).is_none());
            assert!(
                op.deliver_if_live(
                    || permit.send(codex_core::ResponseEvent::OutputTextDelta("late".into()))
                )
                .is_none()
            );
            assert_eq!(rx.len(), 1);
            assert_eq!(op.audit().core_completion.unwrap().end_turn, Some(false));
            assert_eq!(op.first_cancel_reason(), Some(CancelReason::HostCancelled));
        }
    }
    #[tokio::test]
    async fn cancellation_before_core_binding_is_replayed_without_a_lost_wakeup() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        session.shared.state.lock().unwrap().cancel();
        let op = morrow_codex_m03_stream::network::Operation::new(
            session,
            RequestKey(1),
            "http://127.0.0.1:12345/v1",
            tokio::time::Instant::now() + Duration::from_secs(30),
            1024,
        )
        .unwrap();
        assert!(op.token().is_cancelled());
        assert!(op.deliver_if_live(|| ()).is_none());
    }
    #[tokio::test]
    async fn bad_control_after_request_closed_then_valid_close_ack_stays_failed() {
        // Synthetic terminal frames only: this is a protocol negative test,
        // not proof of HTTP completion or native OS cleanup.
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let mut closed = session
            .shared
            .state
            .lock()
            .unwrap()
            .admission
            .initial()
            .clone();
        closed.kind = wire::Kind::RequestClosed;
        closed.sequence = 0;
        closed.payload = wire::Payload::Progress(terminal_progress());
        session
            .shared
            .handle_control(wire::Frame::decode(&closed.encode().unwrap()).unwrap());
        let mut bad = closed.clone();
        bad.kind = wire::Kind::State;
        bad.sequence = 99;
        session
            .shared
            .handle_control(wire::Frame::decode(&bad.encode().unwrap()).unwrap());
        let close = {
            let mut s = session.shared.state.lock().unwrap();
            let close = s
                .admission
                .control(wire::Kind::Close, wire::Payload::None)
                .unwrap();
            s.requests.insert(close.sequence, wire::Kind::Close);
            s.close_written = true;
            close
        };
        let mut ack = closed;
        ack.kind = wire::Kind::State;
        ack.sequence = close.sequence;
        session
            .shared
            .handle_control(wire::Frame::decode(&ack.encode().unwrap()).unwrap());
        assert!(
            session.shared.state.lock().unwrap().close_acked,
            "cleanup acknowledgement is still consumed"
        );
        assert!(session.wait_close().await.is_err());
        assert!(session.final_control_status().is_err());
        let observation = session.close_observation();
        assert!(observation.close_written && observation.close_acked);
        assert!(matches!(
            observation.sticky_control_failure.unwrap().error,
            BridgeError::Protocol(_)
        ));
        assert!(
            !observation.control_protocol_clean,
            "ACK alone is not terminal control evidence"
        );
        let state = session.shared.state.lock().unwrap();
        assert!(state.closed.is_some());
        assert_eq!(
            state.progress.as_ref().unwrap().intent,
            wire::IntentPhase::Observed
        );
    }
    fn closed_with_transport_error() -> (Arc<Session>, wire::Frame) {
        // State/codec only. No OS write, process, pipe, HTTP or Core invocation.
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        session
            .cancel(RequestKey(1), CancelReason::CoreError)
            .unwrap();
        let mut frame = session
            .shared
            .state
            .lock()
            .unwrap()
            .admission
            .initial()
            .clone();
        frame.kind = wire::Kind::RequestClosed;
        frame.sequence = 0;
        frame.revocation_generation = 2;
        let mut progress = terminal_progress();
        progress.http_status = 503;
        progress.error_code = 19;
        progress.revoke_applied = true;
        progress.revoke_persisted = true;
        frame.payload = wire::Payload::Progress(progress);
        session
            .shared
            .handle_control(wire::Frame::decode(&frame.encode().unwrap()).unwrap());
        (session, frame)
    }
    fn receive_close_ack(session: &Session, mut frame: wire::Frame) -> u64 {
        let sequence = {
            let mut s = session.shared.state.lock().unwrap();
            let close = s
                .admission
                .control(wire::Kind::Close, wire::Payload::None)
                .unwrap();
            s.requests.insert(close.sequence, wire::Kind::Close);
            s.close_written = true; // Synthetic completed write, not OS proof.
            close.sequence
        };
        frame.kind = wire::Kind::State;
        frame.sequence = sequence;
        session
            .shared
            .handle_control(wire::Frame::decode(&frame.encode().unwrap()).unwrap());
        sequence
    }
    #[tokio::test]
    async fn cancelled_transport_keeps_unknown_but_close_and_clean_eof_are_observable() {
        let (session, frame) = closed_with_transport_error();
        let sequence = receive_close_ack(&session, frame);
        assert!(matches!(
            session.wait_close().await,
            Err(BridgeError::Unknown)
        ));
        assert!(!session.close_observation().control_protocol_clean);
        session.shared.finish_control(true, None);
        session.wait_control_end().await.unwrap();
        let observation = session.close_observation();
        assert!(
            observation.close_written
                && observation.close_acked
                && observation.control_protocol_clean
        );
        assert_eq!(observation.matching_ack.unwrap().sequence, sequence);
        assert!(observation.sticky_control_failure.is_none());
        assert!(matches!(
            observation.aggregate_error,
            Some(BridgeError::Unknown)
        ));
        assert!(matches!(
            session.final_control_status(),
            Err(BridgeError::Unknown)
        ));
        let progress = observation.final_progress.unwrap();
        assert_eq!(progress["intent"], "Observed");
        assert_eq!(progress["error_code"], 19);
        assert_eq!(progress["http_status"], 503);
    }
    #[tokio::test]
    async fn transport_unknown_cannot_hide_later_bad_sequence_before_valid_close_ack() {
        let (session, frame) = closed_with_transport_error();
        let mut bad = frame.clone();
        bad.kind = wire::Kind::State;
        bad.sequence = 99;
        session
            .shared
            .handle_control(wire::Frame::decode(&bad.encode().unwrap()).unwrap());
        receive_close_ack(&session, frame);
        session.shared.finish_control(true, None);
        let observation = session.close_observation();
        assert!(observation.close_written && observation.close_acked && observation.control_eof);
        assert!(matches!(
            observation.aggregate_error,
            Some(BridgeError::Unknown)
        ));
        let failure = observation.sticky_control_failure.unwrap();
        assert_eq!(failure.source, "control_validation");
        assert!(matches!(failure.error, BridgeError::Protocol(_)));
        assert!(!observation.control_protocol_clean);
        assert!(session.wait_close().await.is_err());
        assert!(session.final_control_status().is_err());
    }
    #[tokio::test]
    async fn ack_does_not_end_observation_before_trailing_bad_frame_and_eof() {
        let (session, frame) = closed_with_transport_error();
        receive_close_ack(&session, frame.clone());
        let mut end = Box::pin(session.wait_control_end());
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(end.as_mut().poll(cx).is_pending()))
                .await
        );
        assert!(!session.close_observation().control_protocol_clean);
        let mut bad = frame;
        bad.kind = wire::Kind::State;
        bad.sequence = 99;
        session
            .shared
            .handle_control(wire::Frame::decode(&bad.encode().unwrap()).unwrap());
        session.shared.finish_control(true, None);
        end.await.unwrap();
        let observation = session.close_observation();
        assert!(observation.control_stream_ended && observation.control_eof);
        assert!(observation.sticky_control_failure.is_some());
        assert!(!observation.control_protocol_clean);
    }
    #[test]
    fn control_io_unknown_is_separate_from_existing_transport_unknown() {
        let (session, frame) = closed_with_transport_error();
        receive_close_ack(&session, frame);
        session
            .shared
            .fail_control("control_writer", BridgeError::Unknown);
        session.shared.finish_control(true, None);
        let observation = session.close_observation();
        assert!(matches!(
            observation.aggregate_error,
            Some(BridgeError::Unknown)
        ));
        assert_eq!(
            observation.sticky_control_failure.unwrap().source,
            "control_writer"
        );
        assert!(!observation.control_protocol_clean);
    }
    #[test]
    fn malformed_control_after_request_closed_and_ack_is_never_normal_eof() {
        for source in ["control_partial", "control_decode"] {
            let (session, frame) = closed_with_transport_error();
            receive_close_ack(&session, frame);
            session.shared.finish_control(
                false,
                Some(ControlFailureObservation {
                    source,
                    error: BridgeError::Protocol("invalid trailing control bytes"),
                }),
            );
            let observation = session.close_observation();
            assert!(observation.close_acked && observation.control_stream_ended);
            assert!(!observation.control_eof && !observation.control_protocol_clean);
            assert_eq!(observation.sticky_control_failure.unwrap().source, source);
            assert!(matches!(
                observation.aggregate_error,
                Some(BridgeError::Unknown)
            ));
        }
    }
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
#[derive(Clone, Debug, serde::Serialize)]
pub struct ControlFailureObservation {
    pub source: &'static str,
    pub error: BridgeError,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct CloseAckObservation {
    pub session: u64,
    pub epoch: u64,
    pub child_pid: u32,
    pub attempt: u64,
    pub sequence: u64,
    pub generation: u64,
    pub code: u32,
}
/// One state-lock snapshot. Handshake facts never erase aggregate failures.
/// A clean control conclusion requires a consumed boundary EOF after the ACK;
/// an empty failure slot before the router terminates is not a clean verdict.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CloseObservation {
    pub close_written: bool,
    pub close_acked: bool,
    pub matching_ack: Option<CloseAckObservation>,
    pub aggregate_error: Option<BridgeError>,
    pub sticky_control_failure: Option<ControlFailureObservation>,
    pub control_eof: bool,
    pub control_stream_ended: bool,
    pub control_protocol_clean: bool,
    pub final_progress: Option<serde_json::Value>,
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
    pub(crate) fn fail_control(&mut self, source: &'static str, error: BridgeError) {
        // Separate first-control-error latch: an earlier transport Unknown must
        // not hide a later bad sequence, malformed frame or control I/O failure.
        if self.control_failure.is_none() {
            self.control_failure = Some(ControlFailureObservation {
                source,
                error: error.clone(),
            });
        }
        self.fail(error);
    }
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
        self.cancel_for(CancelReason::HostCancelled);
    }
    pub(crate) fn cancel_for(&mut self, reason: CancelReason) {
        if self.cancel_reason.is_none() {
            self.cancel_reason = Some(reason)
        }
        if let Some(signal) = &self.cancellation {
            signal.cancel(self.cancel_reason.unwrap())
        }
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
        let reason = match error {
            BridgeError::Deadline => CancelReason::Deadline,
            BridgeError::Denied => CancelReason::HostCancelled,
            _ => CancelReason::NativeFailure,
        };
        if self.fatal.is_none() {
            self.fatal = Some(error);
        }
        self.cancel_for(reason);
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
                        self.close_ack = Some(CloseAckObservation {
                            session: frame.session,
                            epoch: frame.instance_epoch,
                            child_pid: frame.child_pid,
                            attempt: frame.attempt,
                            sequence: frame.sequence,
                            generation: frame.revocation_generation,
                            code: frame.code,
                        });
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
    pub fn handle_control(&self, frame: wire::Frame) {
        let mut state = self.state.lock().unwrap();
        if let Err(error) = state.accept_control(frame) {
            state.fail_control("control_validation", error)
        }
        drop(state);
        self.signal();
    }
    pub fn signal(&self) {
        self.changed.notify_waiters();
        self.work.notify_all();
    }
    pub fn fail(&self, error: BridgeError) {
        self.state.lock().unwrap().fail(error);
        self.signal();
    }
    pub(crate) fn fail_control(&self, source: &'static str, error: BridgeError) {
        self.state.lock().unwrap().fail_control(source, error);
        self.signal();
    }
    pub(crate) fn finish_control(
        &self,
        boundary_eof: bool,
        failure: Option<ControlFailureObservation>,
    ) {
        let mut state = self.state.lock().unwrap();
        state.control_eof = boundary_eof;
        if let Some(failure) = failure {
            state.fail_control(failure.source, failure.error);
        } else if !boundary_eof || !state.close_acked {
            state.fail_control("control_read", BridgeError::Unknown);
        }
        // Set only after all earlier channel entries have been applied. A
        // boundary EOF is final for this single owned control reader.
        state.control_stream_ended = true;
        drop(state);
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
                    control_failure: None,
                    cancel_requested: false,
                    cancel_reason: None,
                    cancellation: None,
                    cancel_written: false,
                    credit_dirty: false,
                    last_credit: None,
                    closed: None,
                    control_eof: false,
                    control_stream_ended: false,
                    close_written: false,
                    close_acked: false,
                    close_ack: None,
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
            if s.close_acked && s.close_written {
                return match &s.fatal {
                    Some(error) => Err(error.clone()),
                    None => Ok(Some(())),
                };
            }
            if s.control_eof && !s.close_acked {
                return Err(BridgeError::CleanupUnconfirmed);
            }
            Ok(None)
        })
        .await
    }
    pub fn final_control_status(&self) -> Result<(), BridgeError> {
        let state = self.shared.state.lock().unwrap();
        if let Some(error) = &state.fatal {
            return Err(error.clone());
        }
        if !state.close_acked || !state.close_written {
            return Err(BridgeError::CleanupUnconfirmed);
        }
        Ok(())
    }
    /// Additional terminal observation; does not redefine wait_close or renew
    /// authority. The caller shares its existing bounded Close wait budget.
    pub async fn wait_control_end(&self) -> Result<(), BridgeError> {
        self.wait(true, |s| Ok(s.control_stream_ended.then_some(())))
            .await
    }
    pub fn close_observation(&self) -> CloseObservation {
        let s = self.shared.state.lock().unwrap();
        CloseObservation {
            close_written: s.close_written,
            close_acked: s.close_acked,
            matching_ack: s.close_ack.clone(),
            aggregate_error: s.fatal.clone(),
            sticky_control_failure: s.control_failure.clone(),
            control_eof: s.control_eof,
            control_stream_ended: s.control_stream_ended,
            control_protocol_clean: s.control_stream_ended
                && s.control_eof
                && s.close_written
                && s.close_acked
                && s.control_failure.is_none(),
            final_progress: s.progress.as_ref().map(progress_observation),
        }
    }
}

fn progress_observation(p: &wire::Progress) -> serde_json::Value {
    serde_json::json!({
        "intent": format!("{:?}", p.intent), "network": format!("{:?}", p.network),
        "owner": format!("{:?}", p.owner), "http_status": p.http_status, "error_code": p.error_code,
        "received_offset": p.received_offset, "reserved_offset": p.reserved_offset,
        "issued_offset": p.issued_offset, "os_completed_offset": p.os_completed_offset,
        "peer_consumed_offset": p.peer_consumed_offset, "parser_yielded_bytes": p.parser_yielded_bytes,
        "drain_discarded_bytes": p.drain_discarded_bytes, "cancel_discarded_bytes": p.cancel_discarded_bytes,
        "error_consumed_bytes": p.error_consumed_bytes, "last_write_ordinal": p.last_write_ordinal,
        "revoke_persisted": p.revoke_persisted, "revoke_applied": p.revoke_applied,
        "http_eof": p.http_eof, "response_material_stored": p.response_material_stored,
        "worker_joined": p.worker_joined, "connect_reaped": p.connect_reaped,
        "read_reaped": p.read_reaped, "write_reaped": p.write_reaped, "data_closed": p.data_closed,
        "child_exited": p.child_exited, "stdout_eof": p.stdout_eof, "stderr_eof": p.stderr_eof,
        "owner_released": p.owner_released, "worker_started": p.worker_started,
        "request_closed": p.request_closed,
    })
}
impl NativeHttpSession for Session {
    fn bind_cancellation(
        &self,
        key: RequestKey,
        signal: CancellationSignal,
    ) -> Result<(), BridgeError> {
        self.key(key)?;
        let mut state = self.shared.state.lock().unwrap();
        if state.cancellation.is_some() {
            return Err(BridgeError::DuplicateRequest);
        }
        if let Some(reason) = state.cancel_reason {
            signal.cancel(reason)
        }
        state.cancellation = Some(signal);
        Ok(())
    }
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
    fn cancel(&self, key: RequestKey, reason: CancelReason) -> Result<(), BridgeError> {
        self.key(key)?;
        self.shared.state.lock().unwrap().cancel_for(reason);
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
