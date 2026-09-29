use crate::{
    admission::{Admission, progress_follows},
    delivery::{Available, Delivery},
};
use bytes::Bytes;
use morrow_codex_m03_stream::driver::*;
use morrow_codex_m03_stream::fixture::Fixture;
use morrow_native_http_stream_wire as wire;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio::sync::Notify;

#[derive(Clone, Debug, serde::Serialize)]
pub struct DataFailureObservation {
    pub stage: &'static str,
    pub error: &'static str,
    pub io_index: Option<usize>,
    pub read_id: Option<u64>,
    pub os_error: Option<u32>,
    pub buffered_bytes: Option<usize>,
    pub expected_frame_bytes: Option<usize>,
    pub prefix_sha256: Option<String>,
}
pub(crate) struct State {
    pub disconnect_deadline: Option<Instant>,
    pub disconnect_explained: bool,
    pub disconnect_frame_index: Option<usize>,
    pub disconnect_io_index: Option<usize>,
    pub disconnect_wait_ns: Option<u64>,
    pub host_cancel_transition: Option<CancelTransition>,
    pub fixture: Option<Arc<Fixture>>,
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
    pub credit_requests: BTreeMap<u64, wire::Credit>,
    pub credit_acked: Option<wire::Credit>,
    pub closed: Option<wire::Progress>,
    pub control_eof: bool,
    pub control_stream_ended: bool,
    pub close_written: bool,
    pub close_acked: bool,
    pub close_ack: Option<CloseAckObservation>,
    pub max_chunk: usize,
    pub first_data_failure: Option<DataFailureObservation>,
    pub last_body_io_index: Option<usize>,
    pub last_body_frame_index: Option<usize>,
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
    fn revoked_frame(session: &Session) -> wire::Frame {
        let mut frame = session
            .shared
            .state
            .lock()
            .unwrap()
            .admission
            .initial()
            .clone();
        frame.kind = wire::Kind::HttpTerminal;
        frame.sequence = 0;
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
        frame
    }
    #[tokio::test]
    async fn boundary_disconnect_waits_for_valid_revoke_without_local_first_reason() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let signal = CancellationSignal::default();
        session
            .bind_cancellation(RequestKey(1), signal.clone())
            .unwrap();
        let started = Instant::now();
        session
            .shared
            .state
            .lock()
            .unwrap()
            .begin_disconnect(started);
        assert!(!signal.is_cancelled());
        assert!(signal.is_paused());
        assert_eq!(signal.reason(), None);
        let mut read = Box::pin(session.read(RequestKey(1), 0, 1024));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut read)
                .await
                .is_err()
        );
        assert_eq!(signal.reason(), None);
        let frame = revoked_frame(&session);
        session.shared.handle_control(frame.clone());
        {
            let s = session.shared.state.lock().unwrap();
            assert!(s.disconnect_status(Instant::now()).unwrap());
            assert_eq!(s.fatal, Some(BridgeError::Unknown));
            let t = s.host_cancel_transition.as_ref().unwrap();
            assert!(
                t.host_control_closed_gate
                    && !t.cancel_gate_before
                    && t.cancel_gate_after
                    && t.delivery_paused_before
            );
            assert_eq!(
                t.first_cancel_reason_after,
                Some(CancelReason::HostCancelled)
            );
        }
        assert_eq!(signal.reason(), Some(CancelReason::HostCancelled));
        assert!(read.await.is_err());
        let mut closed = frame;
        closed.kind = wire::Kind::RequestClosed;
        if let wire::Payload::Progress(p) = &mut closed.payload {
            p.request_closed = true;
        }
        session.shared.handle_control(closed);
        let s = session.shared.state.lock().unwrap();
        assert!(s.closed.is_some());
        assert!(
            !s.host_cancel_transition
                .as_ref()
                .unwrap()
                .host_control_closed_gate
        );
        assert!(s.disconnect_status(Instant::now()).unwrap());
    }
    #[test]
    fn pending_disconnect_is_fixed_bounded_and_local_cancel_or_gen1_close_cannot_explain_it() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let original = session.deadline();
        let now = Instant::now();
        {
            let mut s = session.shared.state.lock().unwrap();
            s.begin_disconnect(now);
            assert_eq!(
                s.disconnect_deadline,
                Some(original.min(now + Duration::from_millis(500)))
            );
            s.begin_disconnect(now + Duration::from_millis(100));
            assert_eq!(
                s.disconnect_deadline,
                Some(original.min(now + Duration::from_millis(500)))
            );
            s.cancel_for(CancelReason::User);
            assert!(!s.disconnect_status(now).unwrap());
        }
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
        frame.payload = wire::Payload::Progress(terminal_progress());
        session.shared.handle_control(frame);
        let mut s = session.shared.state.lock().unwrap();
        assert!(!s.disconnect_status(now).unwrap());
        let reason = s
            .disconnect_status(now + Duration::from_millis(501))
            .unwrap_err();
        s.fail(BridgeError::Protocol(reason));
        assert_eq!(
            s.fatal,
            Some(BridgeError::Protocol(
                "data disconnect control explanation timeout"
            ))
        );
    }
    #[test]
    fn pending_disconnect_is_capped_by_original_authority_and_late_revoke_never_repairs_timeout() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        {
            let mut s = session.shared.state.lock().unwrap();
            let original = s.admission.deadline();
            s.begin_disconnect(original - Duration::from_millis(10));
            assert_eq!(s.disconnect_deadline, Some(original));
        }
        let late = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        late.shared
            .state
            .lock()
            .unwrap()
            .begin_disconnect(Instant::now() - Duration::from_secs(1));
        late.shared.handle_control(revoked_frame(&late));
        let s = late.shared.state.lock().unwrap();
        assert!(!s.disconnect_explained);
        assert_eq!(
            s.fatal,
            Some(BridgeError::Protocol(
                "data disconnect control explanation timeout"
            ))
        );
        assert_eq!(s.cancel_reason, Some(CancelReason::NativeFailure));
    }
    #[test]
    fn bad_tuple_then_valid_revoke_cannot_explain_disconnect_or_clear_control_failure() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        session
            .shared
            .state
            .lock()
            .unwrap()
            .begin_disconnect(Instant::now());
        let mut bad = revoked_frame(&session);
        bad.session += 1;
        session.shared.handle_control(bad);
        session.shared.handle_control(revoked_frame(&session));
        let s = session.shared.state.lock().unwrap();
        assert!(!s.disconnect_explained);
        assert!(s.control_failure.is_some());
        assert!(matches!(s.fatal, Some(BridgeError::Protocol(_))));
        assert!(s.disconnect_status(Instant::now()).is_err());
    }
    #[test]
    fn selected_effect_commands_rejected_by_pause_leave_no_sequence_hole_or_ghost_request() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let mut s = session.shared.state.lock().unwrap();
        s.begin_disconnect(Instant::now());
        assert!(
            s.admit_control_write(wire::Kind::HttpCommit, wire::Payload::None)
                .unwrap()
                .is_none()
        );
        let credit = wire::Credit {
            consumed_offset: 0,
            parser_yielded_bytes: 0,
            drain_discarded_bytes: 0,
            error_consumed_bytes: 0,
            cancel_discarded_bytes: 0,
            window_bytes: 16384,
            max_chunk_bytes: 1024,
        };
        assert!(
            s.admit_control_write(wire::Kind::HttpCredit, wire::Payload::Credit(credit))
                .unwrap()
                .is_none()
        );
        assert_eq!(s.requests.len(), 0);
        let cancel = s
            .admit_control_write(wire::Kind::HttpCancel, wire::Payload::None)
            .unwrap()
            .unwrap();
        let close = s
            .admit_control_write(wire::Kind::Close, wire::Payload::None)
            .unwrap()
            .unwrap();
        assert_eq!(cancel.sequence, 2);
        assert_eq!(close.sequence, 3);
        assert_eq!(s.requests.len(), 2);
        assert_eq!(s.disconnect_frame_index, Some(0));
        assert!(
            s.frame_events
                .iter()
                .filter(|f| f.lane == "guest_control_write_admitted")
                .all(|f| f.delivery_paused
                    && f.kind != "HttpCommit"
                    && f.credit_window_bytes.unwrap_or(0) == 0)
        );
    }
    #[test]
    fn pause_blocks_positive_credit_but_real_host_cancel_settles_original_consumption_with_zero_window()
     {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        {
            let mut s = session.shared.state.lock().unwrap();
            let mut delivery = Delivery::new(65536, 1024).unwrap();
            delivery.received(0, Bytes::from_static(b"abc")).unwrap();
            assert!(matches!(delivery.take(0, 3).unwrap(), Available::Bytes(_)));
            delivery
                .consumed(ConsumptionProgress {
                    consumed_offset: 3,
                    parser_yielded_bytes: 3,
                    drain_discarded_bytes: 0,
                    error_body_consumed_bytes: 0,
                })
                .unwrap();
            s.delivery = Some(delivery);
            s.credit_dirty = true;
            s.begin_disconnect(Instant::now());
            assert!(s.take_credit_command().unwrap().is_none());
            assert!(s.requests.is_empty());
        }
        session.shared.handle_control(revoked_frame(&session));
        let mut s = session.shared.state.lock().unwrap();
        assert!(s.credit_dirty);
        let (kind, payload) = s.take_credit_command().unwrap().unwrap();
        if let wire::Payload::Credit(c) = &payload {
            assert_eq!(c.window_bytes, 0);
            assert_eq!(c.consumed_offset, 3);
            assert_eq!(c.parser_yielded_bytes, 3);
            assert_eq!(c.cancel_discarded_bytes, 0);
            assert_eq!(c.drain_discarded_bytes, 0);
            assert_eq!(c.error_consumed_bytes, 0);
        } else {
            panic!("expected real credit");
        }
        let ack = s.admit_control_write(kind, payload).unwrap().unwrap();
        let close = s
            .admit_control_write(wire::Kind::Close, wire::Payload::None)
            .unwrap()
            .unwrap();
        assert_eq!(ack.sequence, 2);
        assert_eq!(close.sequence, 3);
    }
    fn settlement_session(consumed: u64) -> (Arc<Session>, wire::Frame) {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        {
            let mut s = session.shared.state.lock().unwrap();
            let mut delivery = Delivery::new(65536, 1024).unwrap();
            delivery.received(0, Bytes::from_static(b"abcde")).unwrap();
            let _ = delivery.take(0, 5).unwrap();
            delivery
                .consumed(ConsumptionProgress {
                    consumed_offset: consumed,
                    parser_yielded_bytes: consumed,
                    drain_discarded_bytes: 0,
                    error_body_consumed_bytes: 0,
                })
                .unwrap();
            s.delivery = Some(delivery);
            s.credit_dirty = true;
            s.head = Some(wire::Head {
                status: 200,
                headers: vec![],
                remote_address: "127.0.0.1:12345".into(),
            });
            s.begin_disconnect(Instant::now());
        }
        let mut closed = revoked_frame(&session);
        closed.kind = wire::Kind::RequestClosed;
        if let wire::Payload::Progress(p) = &mut closed.payload {
            p.received_offset = 5;
            p.reserved_offset = 5;
            p.issued_offset = 5;
            p.os_completed_offset = 5;
            p.request_closed = true;
        }
        session.shared.handle_control(closed.clone());
        session.shared.state.lock().unwrap().io_result = Some(Ok(()));
        *session.io_thread.lock().unwrap() = Some(std::thread::spawn(|| Ok(())));
        (session, closed)
    }
    fn settlement_ack(mut closed: wire::Frame, seq: u64, consumed: u64) -> wire::Frame {
        closed.kind = wire::Kind::CreditState;
        closed.sequence = seq;
        closed.code = 0;
        if let wire::Payload::Progress(p) = &mut closed.payload {
            p.peer_consumed_offset = consumed;
            p.parser_yielded_bytes = consumed;
        }
        closed
    }
    #[tokio::test]
    async fn early_request_closed_and_queued_close_wait_for_matching_final_credit_ack() {
        let (session, closed) = settlement_session(5);
        let credit = {
            let mut s = session.shared.state.lock().unwrap();
            s.queue(wire::Kind::Close, wire::Payload::None).unwrap();
            let (kind, payload) = s.take_settlement_or_outbound().unwrap().unwrap();
            assert_eq!(kind, wire::Kind::HttpCredit);
            let credit = s.admit_control_write(kind, payload).unwrap().unwrap();
            assert!(s.take_settlement_or_outbound().unwrap().is_none());
            assert_eq!(s.outbound.len(), 1);
            assert!(s.cleanup_progress().unwrap().is_none());
            credit
        };
        assert!(
            tokio::time::timeout(Duration::from_millis(10), session.cleanup(RequestKey(1)))
                .await
                .is_err()
        );
        assert!(session.shared.state.lock().unwrap().io_joined);
        session
            .shared
            .handle_control(settlement_ack(closed, credit.sequence, 5));
        let receipt = tokio::time::timeout(Duration::from_secs(1), session.cleanup(RequestKey(1)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(receipt.acknowledged_offset, 5);
        let mut s = session.shared.state.lock().unwrap();
        assert_eq!(
            s.closed.as_ref().unwrap().peer_consumed_offset,
            0,
            "early closed fact remains intact"
        );
        let (kind, payload) = s.take_settlement_or_outbound().unwrap().unwrap();
        assert_eq!(kind, wire::Kind::Close);
        assert_eq!(
            s.admit_control_write(kind, payload)
                .unwrap()
                .unwrap()
                .sequence,
            credit.sequence + 1
        );
    }
    #[tokio::test]
    async fn incorrect_credit_ack_never_confirms_cleanup_or_erases_sticky_failure() {
        let (session, closed) = settlement_session(5);
        let credit = {
            let mut s = session.shared.state.lock().unwrap();
            let (kind, payload) = s.take_credit_command().unwrap().unwrap();
            s.admit_control_write(kind, payload).unwrap().unwrap()
        };
        session
            .shared
            .handle_control(settlement_ack(closed, credit.sequence, 4));
        let result = tokio::time::timeout(Duration::from_secs(1), session.cleanup(RequestKey(1)))
            .await
            .unwrap();
        assert_eq!(result.unwrap_err(), BridgeError::CleanupUnconfirmed);
        let s = session.shared.state.lock().unwrap();
        assert!(s.io_joined);
        assert!(s.credit_acked.is_none());
        assert_eq!(
            s.control_failure.as_ref().unwrap().error,
            BridgeError::Protocol("credit ACK consumption mismatch")
        );
    }
    #[tokio::test]
    async fn late_subthreshold_classification_requires_new_zero_credit_and_old_ack_cannot_clear_dirty()
     {
        let (session, closed) = settlement_session(3);
        let first = {
            let mut s = session.shared.state.lock().unwrap();
            let (kind, payload) = s.take_credit_command().unwrap().unwrap();
            let frame = s.admit_control_write(kind, payload).unwrap().unwrap();
            assert!(
                s.take_credit_command().unwrap().is_none(),
                "same in-flight credit is not resent"
            );
            s.delivery
                .as_mut()
                .unwrap()
                .consumed(ConsumptionProgress {
                    consumed_offset: 5,
                    parser_yielded_bytes: 5,
                    drain_discarded_bytes: 0,
                    error_body_consumed_bytes: 0,
                })
                .unwrap();
            s.credit_dirty = true;
            frame
        };
        session
            .shared
            .handle_control(settlement_ack(closed.clone(), first.sequence, 3));
        let second = {
            let mut s = session.shared.state.lock().unwrap();
            assert!(s.credit_dirty);
            assert!(s.cleanup_progress().unwrap().is_none());
            let (kind, payload) = s.take_credit_command().unwrap().unwrap();
            if let wire::Payload::Credit(c) = &payload {
                assert_eq!(c.consumed_offset, 5);
                assert_eq!(c.cancel_discarded_bytes, 0);
            }
            s.admit_control_write(kind, payload).unwrap().unwrap()
        };
        assert_eq!(second.sequence, first.sequence + 1);
        session
            .shared
            .handle_control(settlement_ack(closed, second.sequence, 5));
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), session.cleanup(RequestKey(1)))
                .await
                .unwrap()
                .unwrap()
                .acknowledged_offset,
            5
        );
    }
    #[tokio::test]
    async fn approved_pre_head_zero_consumption_revoke_needs_no_fictitious_credit_ack() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        {
            let mut s = session.shared.state.lock().unwrap();
            s.delivery = Some(Delivery::new(65536, 1024).unwrap());
            s.approved = Some(wire::Decision {
                proposal_ref: vec![1; 32],
                body_sha256: vec![2; 32],
                request_sha256: vec![3; 32],
                http_grant_ref: vec![4; 32],
                endpoint_ref: vec![5; 32],
                body_bytes: 0,
                send_budget: 1,
                response_limit_bytes: 65536,
            });
        }
        let mut closed = revoked_frame(&session);
        closed.kind = wire::Kind::RequestClosed;
        if let wire::Payload::Progress(p) = &mut closed.payload {
            p.request_closed = true;
        }
        session.shared.handle_control(closed);
        session.shared.state.lock().unwrap().io_result = Some(Ok(()));
        *session.io_thread.lock().unwrap() = Some(std::thread::spawn(|| Ok(())));
        let receipt = tokio::time::timeout(Duration::from_secs(1), session.cleanup(RequestKey(1)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(receipt.acknowledged_offset, 0);
        let mut s = session.shared.state.lock().unwrap();
        assert!(s.io_joined);
        assert!(!s.needs_settlement());
        s.queue(wire::Kind::Close, wire::Payload::None).unwrap();
        let (kind, payload) = s.take_settlement_or_outbound().unwrap().unwrap();
        assert_eq!(kind, wire::Kind::Close);
        assert_eq!(
            s.admit_control_write(kind, payload)
                .unwrap()
                .unwrap()
                .sequence,
            2
        );
        assert!(s.credit_requests.is_empty());
        assert!(s.last_credit.is_none());
    }
    #[tokio::test]
    async fn errored_data_worker_is_actually_joined_and_original_failure_is_retained() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        {
            let mut s = session.shared.state.lock().unwrap();
            s.fail(BridgeError::Protocol("original worker error"));
            s.io_result = Some(Err("original worker error"));
        }
        *session.io_thread.lock().unwrap() =
            Some(std::thread::spawn(|| Err("original worker error")));
        let result = tokio::time::timeout(Duration::from_secs(1), session.cleanup(RequestKey(1)))
            .await
            .unwrap();
        assert_eq!(result.unwrap_err(), BridgeError::CleanupUnconfirmed);
        let s = session.shared.state.lock().unwrap();
        assert!(s.io_joined);
        assert_eq!(s.io_result, Some(Err("original worker error")));
        assert_eq!(
            s.fatal,
            Some(BridgeError::Protocol("original worker error"))
        );
    }
    #[tokio::test]
    async fn live_data_thread_is_not_joined_or_reported_joined_on_cleanup_timeout() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let (tx, rx) = std::sync::mpsc::channel();
        *session.io_thread.lock().unwrap() = Some(std::thread::spawn(move || {
            rx.recv().unwrap();
            Err("worker error")
        }));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), session.cleanup(RequestKey(1)))
                .await
                .is_err()
        );
        assert!(!session.shared.state.lock().unwrap().io_joined);
        assert!(session.io_thread.lock().unwrap().is_some());
        tx.send(()).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), session.cleanup(RequestKey(1)))
                .await
                .unwrap()
                .is_err()
        );
        assert!(session.shared.state.lock().unwrap().io_joined);
    }
    #[tokio::test]
    async fn control_handles_are_retained_until_finished_and_joined_inside_budget() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let mut releases = Vec::new();
        for name in ["control_reader", "control_router", "control_writer"] {
            let (tx, rx) = std::sync::mpsc::channel::<()>();
            let handle = std::thread::spawn(move || {
                rx.recv().unwrap();
            });
            session
                .control_threads
                .lock()
                .unwrap()
                .push((name, Some(handle), false));
            releases.push(tx);
        }
        assert!(
            !session
                .join_control_until(tokio::time::Instant::now())
                .await
        );
        assert!(
            session
                .control_threads
                .lock()
                .unwrap()
                .iter()
                .all(|(_, h, j)| h.is_some() && !*j)
        );
        for tx in releases {
            tx.send(()).unwrap();
        }
        assert!(
            session
                .join_control_until(tokio::time::Instant::now() + Duration::from_secs(1))
                .await
        );
        assert!(
            session
                .control_threads
                .lock()
                .unwrap()
                .iter()
                .all(|(_, h, j)| h.is_none() && *j)
        );
    }
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
    pub delivery_paused: bool,
    pub credit_window_bytes: Option<u64>,
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
            delivery_paused: self.disconnect_deadline.is_some(),
            credit_window_bytes: match &frame.payload {
                wire::Payload::Credit(c) => Some(c.window_bytes as u64),
                _ => None,
            },
        });
    }
    pub(crate) fn cancel(&mut self) {
        self.cancel_for(CancelReason::HostCancelled);
    }
    pub(crate) fn begin_disconnect(&mut self, now: Instant) {
        if self.disconnect_deadline.is_none() {
            self.disconnect_deadline = Some(
                self.admission
                    .deadline()
                    .min(now + Duration::from_millis(500)),
            );
            if let Some(signal) = &self.cancellation {
                signal.pause_delivery();
            }
            self.disconnect_frame_index = Some(self.frame_events.len());
            self.disconnect_io_index = Some(self.io_events.len());
            self.disconnect_wait_ns = Some(
                self.disconnect_deadline
                    .unwrap()
                    .saturating_duration_since(now)
                    .as_nanos()
                    .min(u64::MAX as u128) as u64,
            );
            self.outbound
                .retain(|(k, _)| matches!(k, wire::Kind::Query | wire::Kind::Close));
            self.credit_dirty = false;
        }
    }
    pub(crate) fn disconnect_status(&self, now: Instant) -> Result<bool, &'static str> {
        if self.control_failure.is_some() {
            return Err("data disconnect with invalid control");
        }
        if self.disconnect_explained {
            return Ok(true);
        }
        if self.disconnect_deadline.is_some_and(|d| now >= d) {
            return Err("data disconnect control explanation timeout");
        }
        Ok(false)
    }
    pub(crate) fn cancel_for(&mut self, reason: CancelReason) {
        if self.cancel_reason.is_none() {
            self.cancel_reason = Some(reason)
        }
        if let Some(signal) = &self.cancellation {
            let transition = signal.cancel_observed_at(
                self.cancel_reason.unwrap(),
                match reason {
                    CancelReason::HostCancelled => "native_host_cancel",
                    CancelReason::Deadline => "native_deadline",
                    _ => "native_state_cancel",
                },
            );
            if reason == CancelReason::HostCancelled && self.host_cancel_transition.is_none() {
                self.host_cancel_transition = Some(transition);
            }
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
        if self.disconnect_deadline.is_some()
            && !matches!(
                kind,
                wire::Kind::Query | wire::Kind::Close | wire::Kind::HttpCancel
            )
        {
            return Err(BridgeError::Cancelled);
        }
        if self.outbound.len() >= 8 {
            return Err(BridgeError::Protocol("bounded control queue"));
        }
        self.outbound.push_back((kind, payload));
        Ok(())
    }
    pub(crate) fn take_credit_command(
        &mut self,
    ) -> Result<Option<(wire::Kind, wire::Payload)>, BridgeError> {
        let delivery = self
            .delivery
            .as_ref()
            .ok_or(BridgeError::Protocol("credit without delivery"))?;
        let credit = delivery.credit();
        // Pause alone never reclassifies bytes. Real cancellation makes the
        // existing zero-window consumption settlement eligible again.
        if self.disconnect_deadline.is_some() && credit.window_bytes != 0 {
            return Ok(None);
        }
        let due = self.last_credit.as_ref().is_none_or(|last| {
            credit.window_bytes != last.window_bytes
                || (credit.window_bytes == 0 && credit != *last)
                || credit.consumed_offset.saturating_sub(last.consumed_offset) >= 8192
                || ((delivery.final_offset() == Some(credit.consumed_offset)
                    || delivery.at_limit())
                    && credit != *last)
        });
        if !due {
            return Ok(None);
        }
        self.credit_dirty = false;
        self.last_credit = Some(credit.clone());
        Ok(Some((
            wire::Kind::HttpCredit,
            wire::Payload::Credit(credit),
        )))
    }
    fn needs_settlement(&self) -> bool {
        self.cancel_requested
            && self.delivery.as_ref().is_some_and(|d| {
                self.head.is_some()
                    || self.last_credit.is_some()
                    || !self.credit_requests.is_empty()
                    || self.credit_acked.is_some()
                    || d.received_offset() != 0
                    || d.delivered_offset() != 0
                    || d.credit().consumed_offset != 0
            })
    }
    fn settlement_confirmed(&self) -> bool {
        !self.needs_settlement()
            || self
                .delivery
                .as_ref()
                .is_some_and(|d| self.credit_acked.as_ref() == Some(&d.credit()))
    }
    pub(crate) fn take_settlement_or_outbound(
        &mut self,
    ) -> Result<Option<(wire::Kind, wire::Payload)>, BridgeError> {
        if self.cancel_requested && self.credit_dirty && self.head.is_some() {
            if let Some(command) = self.take_credit_command()? {
                return Ok(Some(command));
            }
        }
        if self
            .outbound
            .front()
            .is_some_and(|(k, _)| *k == wire::Kind::Close)
            && !self.settlement_confirmed()
        {
            return Ok(None);
        }
        Ok(self.outbound.pop_front())
    }
    pub(crate) fn cleanup_progress(&self) -> Result<Option<wire::Progress>, BridgeError> {
        // A failed/ended control lane cannot supply a trustworthy late RequestClosed.
        // Keep this failure independent of whether a resource receipt ever arrived.
        if self.control_failure.is_some() {
            return Err(BridgeError::CleanupUnconfirmed);
        }
        let Some(mut closed) = self.closed.clone() else {
            return Ok(None);
        };
        if !self.settlement_confirmed() {
            return Ok(None);
        }
        if self.needs_settlement()
            && let Some(delivery) = &self.delivery
        {
            let latest = self
                .progress
                .as_ref()
                .ok_or(BridgeError::CleanupUnconfirmed)?;
            if !credit_matches_progress(&delivery.credit(), latest) {
                return Err(BridgeError::CleanupUnconfirmed);
            }
            // Actual RequestClosed facts plus later validated cumulative ACK.
            closed.peer_consumed_offset = latest.peer_consumed_offset;
        }
        Ok(Some(closed))
    }
    pub(crate) fn admit_control_write(
        &mut self,
        kind: wire::Kind,
        payload: wire::Payload,
    ) -> Result<Option<wire::Frame>, BridgeError> {
        let cleanup = matches!(
            kind,
            wire::Kind::Query | wire::Kind::Close | wire::Kind::HttpCancel
        ) || matches!(&payload, wire::Payload::Credit(c) if kind == wire::Kind::HttpCredit && c.window_bytes == 0);
        if self.disconnect_deadline.is_some() && !cleanup {
            return Ok(None);
        }
        // Final logical admission and sequence allocation share the state lock.
        // This is not an OS-issue observation. No rollback or sequence holes.
        let frame = self
            .admission
            .control(kind, payload)
            .map_err(BridgeError::Protocol)?;
        if let wire::Payload::Credit(credit) = &frame.payload {
            self.credit_requests.insert(frame.sequence, credit.clone());
        }
        self.requests.insert(frame.sequence, kind);
        self.record("guest_control_queued", &frame);
        self.record("guest_control_write_admitted", &frame);
        Ok(Some(frame))
    }
    pub(crate) fn accept_control(&mut self, frame: wire::Frame) -> Result<(), BridgeError> {
        let request_kind = self.requests.get(&frame.sequence).copied();
        let credit_request = self.credit_requests.get(&frame.sequence).cloned();
        self.admission
            .host_control(&frame)
            .map_err(BridgeError::Protocol)?;
        self.record("host_control", &frame);
        if frame.sequence != 0 {
            self.requests.remove(&frame.sequence);
            self.credit_requests.remove(&frame.sequence);
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
                if kind == wire::Kind::CreditState
                    && frame.code == 0
                    && request_kind == Some(wire::Kind::HttpCredit)
                {
                    let credit = credit_request
                        .ok_or(BridgeError::Protocol("credit ACK without request"))?;
                    if !credit_matches_progress(&credit, &progress) {
                        return Err(BridgeError::Protocol("credit ACK consumption mismatch"));
                    }
                    self.credit_acked = Some(credit);
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

fn credit_matches_progress(credit: &wire::Credit, progress: &wire::Progress) -> bool {
    progress.peer_consumed_offset == credit.consumed_offset
        && progress.parser_yielded_bytes == credit.parser_yielded_bytes
        && progress.drain_discarded_bytes == credit.drain_discarded_bytes
        && progress.error_consumed_bytes == credit.error_consumed_bytes
        && progress.cancel_discarded_bytes == credit.cancel_discarded_bytes
}

pub(crate) struct Shared {
    pub state: Mutex<State>,
    pub changed: Notify,
    pub work: Condvar,
}
impl Shared {
    pub fn handle_control(&self, frame: wire::Frame) {
        let revoked = frame.revocation_generation == 2
            && (matches!(&frame.payload,wire::Payload::Progress(p) if p.revoke_applied&&p.revoke_persisted)
                || matches!(frame.kind, wire::Kind::Stop | wire::Kind::Denied));
        let received = serde_json::json!({"kind":format!("{:?}",frame.kind),"sequence":frame.sequence,
            "generation":frame.revocation_generation,"code":frame.code});
        let head = if let wire::Payload::Head(head) = &frame.payload {
            if frame.kind == wire::Kind::ResponseHead {
                Some(
                    serde_json::json!({"kind":"ResponseHead","sequence":frame.sequence,"generation":frame.revocation_generation,"status":head.status}),
                )
            } else {
                None
            }
        } else {
            None
        };
        let mut state = self.state.lock().unwrap();
        state.host_cancel_transition = None;
        if state
            .disconnect_deadline
            .is_some_and(|d| Instant::now() >= d)
            && !state.disconnect_explained
        {
            state.fail(BridgeError::Protocol(
                "data disconnect control explanation timeout",
            ));
        }
        let frames_before = state.frame_events.len();
        let accepted = match state.accept_control(frame) {
            Ok(()) => true,
            Err(error) => {
                state.fail_control("control_validation", error);
                false
            }
        };
        if accepted
            && revoked
            && state.control_failure.is_none()
            && state
                .disconnect_deadline
                .is_some_and(|d| Instant::now() < d)
        {
            state.disconnect_explained = true;
        }
        let head_frame_index =
            if accepted && head.is_some() && state.frame_events.len() > frames_before {
                Some(frames_before)
            } else {
                None
            };
        let transition = state.host_cancel_transition.clone();
        let fixture = state.fixture.clone();
        let gate_closed = state
            .cancellation
            .as_ref()
            .is_some_and(|s| s.token().is_cancelled());
        drop(state);
        if let (Some(mut head), Some(index), Some(fixture)) = (head, head_frame_index, &fixture) {
            head["frames_index"] = serde_json::json!(index);
            fixture.emit_once("response_head_accepted", head);
        }
        if accepted && revoked {
            if let Some(fixture) = fixture {
                let mut received = received;
                received["cancel_transition"] = serde_json::to_value(transition).unwrap();
                fixture.observe_revoke(received, gate_closed);
            }
        }
        self.signal();
    }
    pub fn flush_cancel_observations(&self) {
        // signal is also used under State; try_lock avoids reacquiring that lock.
        let binding = self
            .state
            .try_lock()
            .ok()
            .and_then(|s| Some((s.fixture.clone()?, s.cancellation.clone()?)));
        if let Some((fixture, signal)) = binding {
            let (records, overflow) = signal.take_observations();
            if overflow {
                fixture.fail("cancellation observation overflow");
            }
            for record in records {
                fixture.emit(
                    "cancellation_transition",
                    serde_json::to_value(record).unwrap(),
                );
            }
        }
    }
    pub fn record_data_failure(&self, failure: DataFailureObservation) {
        let mut s = self.state.lock().unwrap();
        if s.first_data_failure.is_none() {
            s.first_data_failure = Some(failure);
        }
    }
    pub fn signal(&self) {
        self.flush_cancel_observations();
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
    pub(crate) control_threads: Mutex<Vec<(&'static str, Option<JoinHandle<()>>, bool)>>,
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
            control_threads: Mutex::new(Vec::new()),
            shared: Arc::new(Shared {
                changed: Notify::new(),
                work: Condvar::new(),
                state: Mutex::new(State {
                    disconnect_deadline: None,
                    disconnect_explained: false,
                    disconnect_frame_index: None,
                    disconnect_io_index: None,
                    disconnect_wait_ns: None,
                    host_cancel_transition: None,
                    fixture: None,
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
                    credit_requests: BTreeMap::new(),
                    credit_acked: None,
                    closed: None,
                    control_eof: false,
                    control_stream_ended: false,
                    close_written: false,
                    close_acked: false,
                    close_ack: None,
                    max_chunk,
                    first_data_failure: None,
                    last_body_io_index: None,
                    last_body_frame_index: None,
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
    pub fn flush_observations(&self) {
        self.shared.flush_cancel_observations();
    }
    pub fn data_worker_observation(&self) -> serde_json::Value {
        let (result, failure, joined) = {
            let s = self.shared.state.lock().unwrap();
            (s.io_result, s.first_data_failure.clone(), s.io_joined)
        };
        let slot = self.io_thread.lock().unwrap();
        let finished = slot.as_ref().map_or(joined, |h| h.is_finished());
        serde_json::json!({"result":result.map(|r|match r {Ok(())=>serde_json::json!({"ok":true,"error":null}),Err(e)=>serde_json::json!({"ok":false,"error":e})}),
            "first_data_failure":failure,"finished":finished,"joined":joined,"handle_retained":slot.is_some()})
    }
    pub fn deadline(&self) -> Instant {
        self.shared.state.lock().unwrap().admission.deadline()
    }
    pub async fn join_control_until(&self, deadline: tokio::time::Instant) -> bool {
        loop {
            if tokio::time::Instant::now() >= deadline {
                let threads = self.control_threads.lock().unwrap();
                return threads.len() == 3 && threads.iter().all(|(_, _, joined)| *joined);
            }
            let done = {
                let mut threads = self.control_threads.lock().unwrap();
                for (name, handle, joined) in threads.iter_mut() {
                    if handle.as_ref().is_some_and(|h| h.is_finished()) {
                        *joined = handle.take().unwrap().join().is_ok();
                        if !*joined {
                            self.shared.fail_control(name, BridgeError::Unknown);
                        }
                    }
                }
                threads.len() == 3 && threads.iter().all(|(_, _, joined)| *joined)
            };
            if done {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    pub fn control_thread_observations(&self) -> serde_json::Value {
        let threads = self.control_threads.lock().unwrap();
        serde_json::json!(threads.iter().map(|(name,handle,joined)|serde_json::json!({"name":name,
            "joined":joined,"handle_retained":handle.is_some(),"finished":*joined||handle.as_ref().is_some_and(|h|h.is_finished())})).collect::<Vec<_>>())
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
                if (allow_cleanup || state.disconnect_deadline.is_none())
                    && let Some(value) = inspect(&mut state)?
                {
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
    pub fn data_end_observation(&self) -> serde_json::Value {
        let s = self.shared.state.lock().unwrap();
        serde_json::json!({"pause_observed":s.disconnect_deadline.is_some(),
            "control_explained":s.disconnect_explained,
            "frames_index_at_pause":s.disconnect_frame_index,"io_index_at_pause":s.disconnect_io_index,
            "fixed_wait_budget_ns":s.disconnect_wait_ns,"wait_cap_ms":500,
            "authority_deadline_renewed":false,"delivery_resumed":false})
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
        if state.disconnect_deadline.is_some() {
            signal.pause_delivery();
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
            let mut request = Some(request);
            self.wait(false, |state| {
                if !state.data_bound {
                    return Ok(None);
                }
                let request = request.take().ok_or(BridgeError::DuplicateRequest)?;
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
                Ok(Some(()))
            })
            .await?;
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
            self.wait(false, |s| {
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
                Ok(Some(()))
            })
            .await?;
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
        let (fixture, detail) = {
            let mut s = self.shared.state.lock().unwrap();
            s.delivery
                .as_mut()
                .ok_or(BridgeError::Protocol("ACK without delivery"))?
                .consumed(progress)
                .map_err(BridgeError::Protocol)?;
            s.credit_dirty = true;
            let current = s.delivery.as_ref().unwrap().credit();
            let seq = s
                .credit_requests
                .iter()
                .find(|(_, c)| **c == current)
                .map(|(seq, _)| *seq);
            (
                s.fixture.clone(),
                serde_json::json!({"consumed_offset":progress.consumed_offset,"parser_yielded_bytes":progress.parser_yielded_bytes,
                "drain_discarded_bytes":progress.drain_discarded_bytes,"error_body_consumed_bytes":progress.error_body_consumed_bytes,
                "credit_sequence":seq,"io_index":s.last_body_io_index,"body_frame_index":s.last_body_frame_index}),
            )
        };
        if progress.parser_yielded_bytes > 0 {
            if let Some(fixture) = fixture {
                fixture.emit_once("parser_progress_observed", detail);
            }
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
            // Work outcome never skips the actual finished-only thread join.
            loop {
                let outcome = {
                    let mut slot = self.io_thread.lock().unwrap();
                    if slot.as_ref().is_some_and(|h| h.is_finished()) {
                        Some(slot.take().unwrap().join())
                    } else {
                        None
                    }
                };
                if let Some(outcome) = outcome {
                    self.shared.state.lock().unwrap().io_joined = true;
                    if !matches!(outcome, Ok(Ok(()))) {
                        return Err(BridgeError::CleanupUnconfirmed);
                    }
                    break;
                }
                if self.shared.state.lock().unwrap().io_joined {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            let p = self
                .wait(true, |s| {
                    if s.io_result.as_ref().is_some_and(|r| r.is_err()) {
                        return Err(BridgeError::CleanupUnconfirmed);
                    }
                    s.cleanup_progress()
                })
                .await?;
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
