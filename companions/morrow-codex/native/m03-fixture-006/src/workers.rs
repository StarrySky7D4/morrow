//! OS I/O ownership. Only `data_owner` creates, polls, cancels and drops Pipe.
//! No async task or control dispatch owns its blocking cleanup operation.
use crate::{
    admission::Admission,
    session::{ControlFailureObservation, DataFailureObservation, IoObservation, Session, Shared},
};
use morrow_codex_m03_stream::driver::{BridgeError, RequestKey};
use morrow_codex_m03_stream::fixture::{Fixture, Identity};
use morrow_native_http_stream_wire as wire;
use morrow_native_pipe_win::{Kind as IoKind, Pipe};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
enum ControlReadEnd {
    BoundaryEof,
    Failure(ControlFailureObservation),
}
type ControlInput = std::result::Result<wire::Frame, ControlReadEnd>;

fn control_read_failure(source: &'static str, reason: &'static str) -> ControlReadEnd {
    ControlReadEnd::Failure(ControlFailureObservation {
        source,
        error: BridgeError::Protocol(reason),
    })
}

fn read_control_frame(reader: &mut impl Read, partial: &Mutex<Option<Instant>>) -> ControlInput {
    let mut prefix = [0u8; 4];
    if let Err(error) = reader.read_exact(&mut prefix[..1]) {
        return Err(if error.kind() == std::io::ErrorKind::UnexpectedEof {
            ControlReadEnd::BoundaryEof
        } else {
            ControlReadEnd::Failure(ControlFailureObservation {
                source: "control_read",
                error: BridgeError::Unknown,
            })
        });
    }
    *partial.lock().unwrap() = Some(Instant::now());
    reader
        .read_exact(&mut prefix[1..])
        .map_err(|_| control_read_failure("control_partial", "control partial prefix"))?;
    let n =
        wire::payload_length(&prefix).map_err(|e| control_read_failure("control_framing", e))?;
    let mut bytes = Vec::with_capacity(n + 4);
    bytes.extend(prefix);
    bytes.resize(n + 4, 0);
    reader
        .read_exact(&mut bytes[4..])
        .map_err(|_| control_read_failure("control_partial", "control partial payload"))?;
    if partial
        .lock()
        .unwrap()
        .is_some_and(|t| t.elapsed() > Duration::from_millis(500))
    {
        return Err(control_read_failure(
            "control_partial",
            "control partial timeout",
        ));
    }
    *partial.lock().unwrap() = None;
    wire::Frame::decode(&bytes).map_err(|e| control_read_failure("control_decode", e))
}

#[cfg(test)]
mod control_read_tests {
    use super::*;
    #[test]
    fn only_frame_boundary_eof_is_normal_and_trailing_failures_keep_their_source() {
        let initial = crate::admission::tests::admitted().initial().clone();
        let encoded = initial.encode().unwrap();
        for (tail, expected) in [
            (vec![], None),
            (vec![8], Some("control_partial")),
            (vec![8, 0, 0, 0, 1, 2], Some("control_partial")),
            (vec![0, 0, 0, 0], Some("control_framing")),
            (
                [vec![8, 0, 0, 0], vec![255; 8]].concat(),
                Some("control_decode"),
            ),
        ] {
            let partial = Mutex::new(None);
            let mut input = std::io::Cursor::new([encoded.clone(), tail].concat());
            read_control_frame(&mut input, &partial).unwrap();
            match (read_control_frame(&mut input, &partial), expected) {
                (Err(ControlReadEnd::BoundaryEof), None) => {}
                (Err(ControlReadEnd::Failure(failure)), Some(source)) => {
                    assert_eq!(failure.source, source)
                }
                other => panic!("incorrect control end classification: {other:?}"),
            }
        }
    }
    #[test]
    fn boundary_read_io_error_is_not_eof_even_if_error_enum_is_unknown() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        match read_control_frame(&mut Broken, &Mutex::new(None)) {
            Err(ControlReadEnd::Failure(failure)) => {
                assert_eq!(failure.source, "control_read");
                assert!(matches!(failure.error, BridgeError::Unknown));
            }
            other => panic!("I/O error incorrectly treated as EOF: {other:?}"),
        }
    }
}
pub struct ControlReader {
    input: mpsc::Receiver<ControlInput>,
    partial: Arc<Mutex<Option<Instant>>>,
    started: Instant,
    thread: Option<thread::JoinHandle<()>>,
}
impl ControlReader {
    pub fn start() -> Self {
        let (tx, input) = mpsc::sync_channel(1);
        let partial = Arc::new(Mutex::new(None));
        let tracked = partial.clone();
        let started = Instant::now();
        // Blocking stdio is isolated too. Broken/partial control frames produce
        // an unconfirmed exit; they are never a request-cleanup proof.
        let owned = thread::Builder::new()
            .name("m03-control-read".into())
            .spawn(move || {
                let stdin = std::io::stdin();
                let mut reader = stdin.lock();
                loop {
                    let result = read_control_frame(&mut reader, &tracked);
                    let failed = result.is_err();
                    if tx.send(result).is_err() || failed {
                        break;
                    }
                }
            })
            .expect("create owned control reader");
        Self {
            input,
            partial,
            started,
            thread: Some(owned),
        }
    }
    pub fn admit(
        mut self,
        artifact: [u8; 32],
        max_chunk: usize,
        key: RequestKey,
        fixture: Arc<Fixture>,
    ) -> Result<Arc<Session>, BridgeError> {
        let initial = self
            .input
            .recv_timeout(Duration::from_millis(500))
            .map_err(|_| BridgeError::Protocol("initial Challenge timeout"))?
            .map_err(|end| match end {
                ControlReadEnd::BoundaryEof => BridgeError::Protocol("initial control EOF"),
                ControlReadEnd::Failure(failure) => failure.error,
            })?;
        let admission = Admission::new(initial, self.started, std::process::id(), &artifact)
            .map_err(BridgeError::Protocol)?;
        let session = Session::new(admission, max_chunk, key);
        let identity = session.identity();
        fixture
            .bind(Identity {
                session: identity.session,
                epoch: identity.epoch,
                child_pid: identity.child_pid,
                attempt: identity.attempt,
                operation_id_sha256: identity.operation_id_sha256,
                host_execution_config_sha256: identity.host_execution_config_sha256,
            })
            .map_err(BridgeError::Protocol)?;
        fixture
            .bind_authority(
                self.started,
                session.deadline(),
                identity.original_remaining_ms,
            )
            .map_err(BridgeError::Protocol)?;
        session.shared.state.lock().unwrap().fixture = Some(fixture);
        session
            .control_threads
            .lock()
            .unwrap()
            .push(("control_reader", self.thread.take(), false));
        let routed = session.shared.clone();
        let router = thread::Builder::new()
            .name("m03-control-router".into())
            .spawn(move || {
                loop {
                    match self.input.recv_timeout(Duration::from_millis(10)) {
                        Ok(Ok(frame)) => {
                            routed.handle_control(frame);
                        }
                        Ok(Err(ControlReadEnd::BoundaryEof)) => {
                            routed.finish_control(true, None);
                            break;
                        }
                        Ok(Err(ControlReadEnd::Failure(failure))) => {
                            routed.finish_control(false, Some(failure));
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            routed.finish_control(
                                false,
                                Some(ControlFailureObservation {
                                    source: "control_reader_disconnected",
                                    error: BridgeError::Unknown,
                                }),
                            );
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if self
                        .partial
                        .lock()
                        .unwrap()
                        .is_some_and(|t| t.elapsed() > Duration::from_millis(500))
                    {
                        routed.finish_control(
                            false,
                            Some(ControlFailureObservation {
                                source: "control_partial",
                                error: BridgeError::Protocol("control partial-frame timeout"),
                            }),
                        );
                        break;
                    }
                    let mut s = routed.state.lock().unwrap();
                    if Instant::now() >= s.admission.deadline()
                        && !s.cancel_requested
                        && s.closed.is_none()
                    {
                        s.fail(BridgeError::Deadline);
                        drop(s);
                        routed.signal()
                    }
                }
            })
            .map_err(|_| BridgeError::Unknown)?;
        session
            .control_threads
            .lock()
            .unwrap()
            .push(("control_router", Some(router), false));
        let output = session.shared.clone();
        let writer = thread::Builder::new()
            .name("m03-control-write".into())
            .spawn(move || {
                if let Err(error) = control_writer(&output) {
                    output.fail_control("control_writer", error)
                }
            })
            .map_err(|_| BridgeError::Unknown)?;
        session
            .control_threads
            .lock()
            .unwrap()
            .push(("control_writer", Some(writer), false));
        let data = session.shared.clone();
        let handle = thread::Builder::new()
            .name("m03-owned-pipe".into())
            .spawn(move || {
                let result = data_owner(&data);
                if let Err(error)=result {record_owner_failure(&data,"worker_other",error,None,None,None,None);}
                {
                    let mut s = data.state.lock().unwrap();
                    s.io_result = Some(result);
                    if result.is_err() {
                        s.fail(BridgeError::CleanupUnconfirmed)
                    }
                }
                let (fixture,failure)={let s=data.state.lock().unwrap();(s.fixture.clone(),s.first_data_failure.clone())};
                if let Some(fixture)=fixture {fixture.emit_once("data_worker_finished",serde_json::json!({"result":match result {Ok(())=>serde_json::json!({"ok":true,"error":null}),Err(e)=>serde_json::json!({"ok":false,"error":e})},"first_data_failure":failure}));}
                data.signal();
                result
            })
            .map_err(|_| BridgeError::Unknown)?;
        *session.io_thread.lock().unwrap() = Some(handle);
        Ok(session)
    }
}

fn control_writer(shared: &Arc<Shared>) -> Result<(), BridgeError> {
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    loop {
        let frame = {
            let mut state = shared.state.lock().unwrap();
            // The sole control input has ended. Never spin waiting for a
            // command or emit a new frame after that terminal observation.
            // Preserve the recorded failure and require real Close/ACK.
            if state.control_stream_ended {
                if let Some(failure) = &state.control_failure {
                    return Err(failure.error.clone());
                }
                return if state.close_written && state.close_acked {
                    Ok(())
                } else {
                    Err(BridgeError::CleanupUnconfirmed)
                };
            }
            let command =
                if state.cancel_requested && !state.cancel_written && state.admission.welcomed() {
                    state.cancel_written = true;
                    Some((wire::Kind::HttpCancel, wire::Payload::None))
                } else if let Some(command) = state.take_settlement_or_outbound()? {
                    Some(command)
                } else if state.credit_dirty && state.head.is_some() {
                    state.take_credit_command()?
                } else {
                    None
                };
            if let Some((kind, payload)) = command {
                state.admit_control_write(kind, payload)?
            } else {
                let _ = shared
                    .work
                    .wait_timeout(state, Duration::from_millis(5))
                    .unwrap();
                None
            }
        };
        let Some(frame) = frame else { continue };
        let bytes = frame.encode().map_err(BridgeError::Protocol)?;
        writer.write_all(&bytes).map_err(|_| BridgeError::Unknown)?;
        writer.flush().map_err(|_| BridgeError::Unknown)?;
        let close = frame.kind == wire::Kind::Close;
        {
            let mut s = shared.state.lock().unwrap();
            s.record("guest_control_written", &frame);
            if close {
                s.close_written = true
            }
        }
        shared.signal();
        if close {
            return Ok(());
        }
    }
}

fn clean_disconnect(done: &morrow_native_pipe_win::Completed, framer: &Framer) -> bool {
    done.transferred == 0
        && done.bytes.is_empty()
        && matches!(done.error, None | Some(109 | 232 | 233))
        && framer.bytes.is_empty()
        && framer.expected == 4
}

#[derive(Clone, Debug, serde::Serialize)]
struct ReadFragment {
    io_index: usize,
    read_id: u64,
    transferred_bytes: usize,
    bytes_hex: String,
}
struct Framer {
    bytes: Vec<u8>,
    expected: usize,
    started: Option<Instant>,
    read_fragments: Option<Vec<ReadFragment>>,
}
impl Framer {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            expected: 4,
            started: None,
            read_fragments: Some(Vec::new()),
        }
    }
    fn push_read(
        &mut self,
        done: &morrow_native_pipe_win::Completed,
        io_index: Option<usize>,
    ) -> Result<Option<wire::Frame>, &'static str> {
        // Diagnostic prefix only: never retain arbitrary body bytes or invent completion facts.
        if done.kind != IoKind::Read
            || done.error.is_some()
            || done.transferred == 0
            || done.transferred != done.bytes.len()
            || self.bytes.len().saturating_add(done.bytes.len()) > 12
            || io_index.is_none()
        {
            self.read_fragments = None;
        }
        if let (Some(fragments), Some(index)) = (&mut self.read_fragments, io_index) {
            if fragments.len() < 12 {
                fragments.push(ReadFragment {
                    io_index: index,
                    read_id: done.id,
                    transferred_bytes: done.transferred,
                    bytes_hex: wire::hex(&done.bytes),
                });
            } else {
                self.read_fragments = None;
            }
        }
        self.push(&done.bytes)
    }
    fn needed(&self) -> usize {
        self.expected - self.bytes.len()
    }
    fn push(&mut self, bytes: &[u8]) -> Result<Option<wire::Frame>, &'static str> {
        if self.started.is_none() {
            self.started = Some(Instant::now())
        }
        self.bytes.extend_from_slice(bytes);
        if self.bytes.len() > self.expected {
            return Err("data frame overread");
        }
        if self.bytes.len() == 4 && self.expected == 4 {
            self.expected = 4 + wire::payload_length(&self.bytes)?
        }
        if self.bytes.len() == self.expected {
            let frame = wire::Frame::decode(&self.bytes)?;
            *self = Self::new();
            Ok(Some(frame))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod pause_tests {
    use super::*;
    #[test]
    fn provisional_disconnect_requires_zero_bytes_known_status_and_complete_frame() {
        for error in [None, Some(109), Some(232), Some(233)] {
            let mut done = morrow_native_pipe_win::Completed {
                id: 9,
                kind: IoKind::Read,
                transferred: 0,
                error,
                bytes: vec![],
            };
            assert!(clean_disconnect(&done, &Framer::new()));
            let mut partial = Framer::new();
            partial.bytes.push(8);
            assert!(!clean_disconnect(&done, &partial));
            done.bytes.push(1);
            assert!(!clean_disconnect(&done, &Framer::new()));
            done.bytes.clear();
            done.transferred = 1;
            assert!(!clean_disconnect(&done, &Framer::new()));
        }
        let done = morrow_native_pipe_win::Completed {
            id: 9,
            kind: IoKind::Read,
            transferred: 0,
            error: Some(5),
            bytes: vec![],
        };
        assert!(!clean_disconnect(&done, &Framer::new()));
    }
}

fn data_owner(shared: &Arc<Shared>) -> Result<(), &'static str> {
    let fixture = shared.state.lock().unwrap().fixture.clone();
    let mut read_issue_count = 0u64;
    let channel = loop {
        let state = shared.state.lock().unwrap();
        if state.cancel_requested || Instant::now() >= state.admission.deadline() {
            return Ok(());
        }
        if let Some(channel) = &state.channel {
            break channel.clone();
        }
        let _ = shared
            .work
            .wait_timeout(state, Duration::from_millis(5))
            .unwrap();
    };
    // Exactly one CreateFileW attempt; there is no reconnect loop.
    let mut pipe = Pipe::open_client(&channel.locator).map_err(|_| "open client")?;
    if pipe.inheritable().map_err(|_| "pipe handle flag")? {
        return Err("inherited client handle");
    }
    let bind = {
        let mut s = shared.state.lock().unwrap();
        let f = s
            .admission
            .data(wire::Kind::DataBind, wire::Payload::Channel(channel))
            .map_err(|_| "data bind admission")?;
        s.record("guest_data_queued", &f);
        f.encode()?
    };
    let mut writing = Some((bind, 0usize));
    let mut framer = Framer::new();
    let mut stage = "worker_other";
    let mut last_io_index = None;
    let mut last_read_id = None;
    let mut last_os_error = None;
    let result = (|| {
        loop {
            if fixture.as_ref().is_some_and(|f| f.failed()) {
                return Err("fixture evidence failure");
            }
            // No lane poll/issue or driver delivery while awaiting control.
            // Existing operations are cancelled/reaped below only after the
            // explanation or the fixed deadline; local cancel cannot explain it.
            {
                let s = shared.state.lock().unwrap();
                if s.disconnect_deadline.is_some() {
                    if s.disconnect_status(Instant::now())? {
                        break;
                    }
                    drop(s);
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
            }
            let (cancel, closed, deadline) = {
                let mut s = shared.state.lock().unwrap();
                if Instant::now() >= s.admission.deadline()
                    && !s.cancel_requested
                    && s.closed.is_none()
                {
                    s.fail(BridgeError::Deadline);
                    shared.signal()
                }
                (
                    s.cancel_requested,
                    s.closed.is_some(),
                    s.admission.deadline(),
                )
            };
            stage = "framer";
            if cancel || closed {
                if !framer.bytes.is_empty() {
                    return Err("close with partial data frame");
                }
                break;
            }
            if framer
                .started
                .is_some_and(|t| t.elapsed() > Duration::from_millis(500))
            {
                return Err("data partial-frame timeout");
            }
            if Instant::now() >= deadline {
                return Err("original data deadline");
            }
            stage = "write_completion";
            last_io_index = None;
            last_os_error = None;
            if pipe.has_operation(IoKind::Write) {
                if let Some(done) = pipe
                    .poll(IoKind::Write)
                    .map_err(|_| "write completion unconfirmed")?
                {
                    last_io_index = observe_completion(shared, &done);
                    last_os_error = done.error;
                    if done.error.is_some() || done.transferred == 0 {
                        return Err("data write completion error");
                    }
                    let current = writing.as_mut().ok_or("write without owned frame")?;
                    current.1 += done.transferred;
                    if current.1 > current.0.len() {
                        return Err("write byte overflow");
                    }
                    if current.1 == current.0.len() {
                        writing = None
                    }
                }
            }
            stage = "read_completion";
            last_io_index = None;
            last_os_error = None;
            last_read_id = None;
            if pipe.has_operation(IoKind::Read) {
                if let Some(done) = pipe
                    .poll(IoKind::Read)
                    .map_err(|_| "read completion unconfirmed")?
                {
                    last_io_index = observe_completion(shared, &done);
                    last_read_id = Some(done.id);
                    last_os_error = done.error;
                    if done.error.is_some() || done.transferred == 0 {
                        if let Some(fixture) = &fixture {
                            fixture.emit_once("data_end_observed",serde_json::json!({"read_id":done.id,"io_index":last_io_index,"os_error":done.error,"transferred_bytes":done.transferred,"buffered_bytes":framer.bytes.len(),"expected_frame_bytes":framer.expected,"complete_boundary":framer.bytes.is_empty()&&framer.expected==4}));
                        }
                        // Host closes its data lane before its RequestClosed
                        // control notification. Retain that cross-lane race;
                        // final HTTP/prefix facts must already justify closure.
                        if !clean_disconnect(&done, &framer) {
                            return Err("unexpected data EOF/error");
                        }
                        let mut s = shared.state.lock().unwrap();
                        let expected = s.cancel_requested
                            || s.closed.is_some()
                            || s.delivery
                                .as_ref()
                                .is_some_and(|d| d.final_offset() == Some(d.received_offset()));
                        if expected {
                            break;
                        } else {
                            if writing.is_some() || pipe.has_operation(IoKind::Write) {
                                return Err("disconnect with unaccounted request write");
                            }
                            s.begin_disconnect(Instant::now());
                            drop(s);
                            shared.signal();
                            continue;
                        }
                    }
                    stage = "framer";
                    if let Some(frame) = framer.push_read(&done, last_io_index)? {
                        let body = frame.kind == wire::Kind::BodyChunk;
                        stage = "protocol_accept";
                        {
                            let mut state = shared.state.lock().unwrap();
                            let frames_before = state.frame_events.len();
                            state.accept_data(frame).map_err(|_| "host data protocol")?;
                            if body {
                                state.last_body_io_index = last_io_index;
                                state.last_body_frame_index =
                                    if state.frame_events.len() > frames_before {
                                        Some(frames_before)
                                    } else {
                                        None
                                    };
                            }
                        }
                        shared.signal();
                    } else if framer.bytes.len() == 12
                        && framer.expected > 12
                        && shared.state.lock().unwrap().data_bound
                    {
                        if let Some(fixture) = &fixture {
                            if let (Some(index), Some(fragments)) =
                                (last_io_index, &framer.read_fragments)
                            {
                                fixture.emit_once("data_partial_frame_observed",serde_json::json!({"read_id":done.id,"read_issue_count":read_issue_count,"transferred_bytes":done.transferred,"buffered_bytes":12,"declared_payload_bytes":framer.expected-4,"expected_frame_bytes":framer.expected,"prefix_sha256":format!("{:x}",Sha256::digest(&framer.bytes)),"prefix_hex":wire::hex(&framer.bytes),"read_fragments":fragments,"io_index":index}));
                            } else {
                                fixture.fail("partial-frame IO observation overflow");
                            }
                        }
                    }
                }
            }
            stage = "worker_other";
            last_io_index = None;
            last_os_error = None;
            last_read_id = None;
            if writing.is_none() {
                let mut s = shared.state.lock().unwrap();
                if s.data_bound
                    && s.prepare_acked
                    && !s.cancel_requested
                    && s.disconnect_deadline.is_none()
                {
                    if let Some(body) = &s.body {
                        if s.body_offset < body.len() {
                            let start = s.body_offset;
                            let end = (start + 8192).min(body.len());
                            let chunk = wire::Chunk {
                                offset: start as u64,
                                bytes: body[start..end].to_vec(),
                            };
                            let frame = s
                                .admission
                                .data(wire::Kind::RequestChunk, wire::Payload::Chunk(chunk))
                                .map_err(|_| "request data admission")?;
                            s.body_offset = end;
                            s.record("guest_data_queued", &frame);
                            writing = Some((frame.encode()?, 0));
                        }
                    }
                }
            }
            // Re-check generation under the same state lock that local cancel
            // applies. Hold it only across the nonblocking OS issue, never poll.
            {
                let mut s = shared.state.lock().unwrap();
                if !s.cancel_requested
                    && s.disconnect_deadline.is_none()
                    && s.closed.is_none()
                    && Instant::now() < s.admission.deadline()
                {
                    if !pipe.has_operation(IoKind::Write) {
                        if let Some((bytes, offset)) = &writing {
                            let started = pipe
                                .begin_write(bytes[*offset..].to_vec())
                                .map_err(|_| "begin data write")?;
                            s.io_event(start_event(&started));
                        }
                    }
                    // Poll above may just have enqueued the second chunk.
                    // Recompute room now, never reuse a pre-poll capacity value.
                    let can_read = s.delivery.as_ref().is_none_or(|d| d.can_issue_read());
                    if !pipe.has_operation(IoKind::Read) && (can_read || !framer.bytes.is_empty()) {
                        let started = pipe
                            .begin_read(framer.needed())
                            .map_err(|_| "begin data read")?;
                        s.io_event(start_event(&started));
                        read_issue_count += 1;
                        // Atomic count only; marker queue/file I/O is outside this lock.
                        if let Some(fixture) = &fixture {
                            fixture.read_issued(read_issue_count);
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    })();
    // Preserve each completion before attempting the next one. An unexpected
    // poll error never turns partial recovery into successful RequestClosed.
    if let Err(reason) = result {
        record_owner_failure(
            shared,
            stage,
            reason,
            Some(&framer),
            last_io_index,
            if matches!(stage, "framer" | "read_completion" | "protocol_accept") {
                last_read_id
            } else {
                None
            },
            last_os_error,
        );
        shared.fail(BridgeError::Protocol(reason));
    }
    let cancel = pipe.cancel_all().map_err(|_| "CancelIoEx request failed");
    let reaped = (|| {
        let mut unexpected_tail = false;
        while [IoKind::Connect, IoKind::Read, IoKind::Write]
            .iter()
            .any(|k| pipe.has_operation(*k))
        {
            for kind in [IoKind::Connect, IoKind::Read, IoKind::Write] {
                if pipe.has_operation(kind) {
                    if let Some(done) = pipe.poll(kind).map_err(|_| "pipe reap unconfirmed")? {
                        observe_completion(shared, &done);
                        if done.kind == IoKind::Read
                            && !done.bytes.is_empty()
                            && !shared.state.lock().unwrap().cancel_requested
                        {
                            unexpected_tail = true;
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
        if unexpected_tail {
            Err("unaccounted data during normal close")
        } else {
            Ok(())
        }
    })();
    // This drop is on the owning thread, after known operation recovery. If
    // recovery errored, the shared platform's safe retention rules still apply.
    drop(pipe);
    if let Err(reason) = cancel.and(reaped) {
        record_owner_failure(
            shared,
            "cancel_reap",
            reason,
            Some(&framer),
            None,
            None,
            None,
        );
    }
    result.and(cancel).and(reaped)
}
fn start_event(start: &morrow_native_pipe_win::Started) -> IoObservation {
    IoObservation {
        action: "os_issue",
        id: start.id,
        kind: format!("{:?}", start.kind),
        bytes: start.requested,
        pending: start.pending,
        error: None,
    }
}
fn observe_completion(shared: &Shared, done: &morrow_native_pipe_win::Completed) -> Option<usize> {
    let mut s = shared.state.lock().unwrap();
    let index = s.io_events.len();
    s.io_event(IoObservation {
        action: "os_completion",
        id: done.id,
        kind: format!("{:?}", done.kind),
        bytes: done.transferred,
        pending: false,
        error: done.error,
    });
    if s.io_events.len() > index {
        Some(index)
    } else {
        None
    }
}
fn record_owner_failure(
    shared: &Shared,
    stage: &'static str,
    error: &'static str,
    framer: Option<&Framer>,
    io_index: Option<usize>,
    read_id: Option<u64>,
    os_error: Option<u32>,
) {
    shared.record_data_failure(DataFailureObservation {
        stage,
        error,
        io_index,
        read_id,
        os_error,
        buffered_bytes: framer.map(|f| f.bytes.len()),
        expected_frame_bytes: framer.map(|f| f.expected),
        prefix_sha256: framer
            .filter(|f| !f.bytes.is_empty())
            .map(|f| format!("{:x}", Sha256::digest(&f.bytes))),
    });
}

#[cfg(test)]
mod passive_tests {
    use super::*;
    use bytes::Bytes;
    use morrow_codex_m03_stream::driver::{CancelReason, CancellationSignal, NativeHttpSession};
    use morrow_codex_m03_stream::fixture::Spec;
    fn fixture(session: &Session) -> Arc<Fixture> {
        let root = std::env::temp_dir().join(format!(
            "m03-fixture003-native-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let examples: serde_json::Value = serde_json::from_str(include_str!(
            "../../../receipts/m03-fixture-003-design-001/scenario-specs.example.json"
        ))
        .unwrap();
        let mut spec: Spec =
            serde_json::from_value(examples["pipe-partial-close"].clone()).unwrap();
        spec.nonce = "a".repeat(64);
        let bytes = serde_json::to_vec(&spec).unwrap();
        std::fs::write(root.join("fixture-spec.json"), &bytes).unwrap();
        let f = Fixture::open(&root, &format!("{:x}", Sha256::digest(&bytes))).unwrap();
        let id = session.identity();
        f.bind(Identity {
            session: id.session,
            epoch: id.epoch,
            child_pid: id.child_pid,
            attempt: id.attempt,
            operation_id_sha256: id.operation_id_sha256,
            host_execution_config_sha256: id.host_execution_config_sha256,
        })
        .unwrap();
        f
    }
    #[tokio::test]
    async fn partial_frame_protocol_error_is_independent_of_first_unknown_and_actual_failed_worker_join()
     {
        // Synthetic wire/completion facts, plus a real local owned thread. No pipe/HTTP pair.
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let signal = CancellationSignal::default();
        session
            .bind_cancellation(RequestKey(1), signal.clone())
            .unwrap();
        session
            .shared
            .state
            .lock()
            .unwrap()
            .cancel_for(CancelReason::HostCancelled);
        session.shared.fail(BridgeError::Unknown);
        let mut frame = session
            .shared
            .state
            .lock()
            .unwrap()
            .admission
            .initial()
            .clone();
        frame.kind = wire::Kind::BodyChunk;
        frame.sequence = 2;
        frame.payload = wire::Payload::Chunk(wire::Chunk {
            offset: 0,
            bytes: vec![9; 16],
        });
        let bytes = frame.encode().unwrap();
        let mut framer = Framer::new();
        let first = morrow_native_pipe_win::Completed {
            id: 5,
            kind: IoKind::Read,
            transferred: 4,
            error: None,
            bytes: bytes[..4].to_vec(),
        };
        let second = morrow_native_pipe_win::Completed {
            id: 6,
            kind: IoKind::Read,
            transferred: 8,
            error: None,
            bytes: bytes[4..12].to_vec(),
        };
        let first_index = observe_completion(&session.shared, &first);
        assert!(framer.push_read(&first, first_index).unwrap().is_none());
        let second_index = observe_completion(&session.shared, &second);
        assert!(framer.push_read(&second, second_index).unwrap().is_none());
        let fragments = framer.read_fragments.as_ref().unwrap();
        assert_eq!(fragments.len(), 2);
        assert_eq!(fragments[0].io_index, 0);
        assert_eq!(fragments[0].read_id, 5);
        assert_eq!(fragments[0].transferred_bytes, 4);
        assert_eq!(fragments[1].io_index, 1);
        assert_eq!(fragments[1].read_id, 6);
        assert_eq!(fragments[1].transferred_bytes, 8);
        assert_eq!(
            format!("{}{}", fragments[0].bytes_hex, fragments[1].bytes_hex),
            wire::hex(&bytes[..12])
        );
        let eof = morrow_native_pipe_win::Completed {
            id: 7,
            kind: IoKind::Read,
            transferred: 0,
            error: Some(109),
            bytes: vec![],
        };
        assert!(!clean_disconnect(&eof, &framer));
        let index = observe_completion(&session.shared, &eof);
        record_owner_failure(
            &session.shared,
            "read_completion",
            "unexpected data EOF/error",
            Some(&framer),
            index,
            Some(7),
            Some(109),
        );
        session
            .shared
            .fail(BridgeError::Protocol("unexpected data EOF/error"));
        let (tx, rx) = mpsc::channel();
        *session.io_thread.lock().unwrap() = Some(thread::spawn(move || {
            rx.recv().unwrap();
            Err("unexpected data EOF/error")
        }));
        session.shared.state.lock().unwrap().io_result = Some(Err("unexpected data EOF/error"));
        let before = session.data_worker_observation();
        assert_eq!(before["result"]["ok"], false);
        assert_eq!(before["finished"], false);
        assert_eq!(before["joined"], false);
        assert_eq!(before["handle_retained"], true);
        tx.send(()).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), session.cleanup(RequestKey(1)))
                .await
                .unwrap()
                .unwrap_err(),
            BridgeError::CleanupUnconfirmed
        );
        let after = session.data_worker_observation();
        assert_eq!(after["finished"], true);
        assert_eq!(after["joined"], true);
        assert_eq!(after["handle_retained"], false);
        assert_eq!(after["first_data_failure"]["buffered_bytes"], 12);
        assert_eq!(after["first_data_failure"]["os_error"], 109);
        assert_eq!(after["first_data_failure"]["io_index"], 2);
        assert_eq!(after["first_data_failure"]["read_id"], 7);
        assert_eq!(signal.reason(), Some(CancelReason::HostCancelled));
        assert_eq!(
            session.shared.state.lock().unwrap().fatal,
            Some(BridgeError::Unknown)
        );
    }
    #[tokio::test]
    async fn parser_observation_keeps_actual_body_io_reference_without_forcing_small_credit() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let f = fixture(&session);
        {
            let mut s = session.shared.state.lock().unwrap();
            let mut d = crate::delivery::Delivery::new(65536, 1024).unwrap();
            d.received(0, Bytes::from_static(b"abc")).unwrap();
            d.take(0, 3).unwrap();
            s.delivery = Some(d);
            s.last_body_io_index = Some(9);
            s.last_body_frame_index = Some(4);
            s.fixture = Some(f.clone());
        }
        session
            .acknowledge(
                RequestKey(1),
                morrow_codex_m03_stream::driver::ConsumptionProgress {
                    consumed_offset: 3,
                    parser_yielded_bytes: 3,
                    drain_discarded_bytes: 0,
                    error_body_consumed_bytes: 0,
                },
            )
            .unwrap();
        assert!(
            session
                .shared
                .state
                .lock()
                .unwrap()
                .credit_requests
                .is_empty()
        );
        f.request_stop();
        assert!(
            f.join_until(tokio::time::Instant::now() + Duration::from_secs(1))
                .await
        );
        assert_eq!(f.snapshot()["evidence_complete"], true);
        assert_eq!(f.snapshot()["enqueued_records"], 1);
        // The observer queues a marker only. It does not dispatch a credit or create an ACK.
    }
    #[tokio::test]
    async fn control_and_evidence_join_share_one_500ms_deadline_without_renewal() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let f = fixture(&session);
        for name in ["control_reader", "control_router", "control_writer"] {
            session.control_threads.lock().unwrap().push((
                name,
                Some(thread::spawn(|| thread::sleep(Duration::from_millis(600)))),
                false,
            ));
        }
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        assert!(!session.join_control_until(deadline).await);
        f.request_stop();
        assert!(!f.join_until(deadline).await);
        assert_eq!(f.snapshot()["writer_joined"], false);
        assert!(
            session
                .control_thread_observations()
                .as_array()
                .unwrap()
                .iter()
                .any(|o| o["handle_retained"] == true)
        );
        // Explicit local teardown after the observation failed, not runtime permission renewal.
        let teardown = tokio::time::Instant::now() + Duration::from_secs(1);
        assert!(session.join_control_until(teardown).await);
        assert!(f.join_until(teardown).await);
    }
    #[tokio::test]
    async fn actual_control_end_after_stop20_fails_cleanup_without_waiting_for_missing_request_closed()
     {
        // Synthetic validated control/EOF observations; actual local data handle join.
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let signal = CancellationSignal::default();
        session
            .bind_cancellation(RequestKey(1), signal.clone())
            .unwrap();
        let mut stop = session
            .shared
            .state
            .lock()
            .unwrap()
            .admission
            .initial()
            .clone();
        stop.kind = wire::Kind::Stop;
        stop.sequence = 0;
        stop.code = 20;
        stop.revocation_generation = 2;
        stop.payload = wire::Payload::None;
        session.shared.handle_control(stop);
        assert_eq!(signal.reason(), Some(CancelReason::Deadline));
        session.shared.finish_control(true, None);
        assert!(session.shared.state.lock().unwrap().closed.is_none());
        *session.io_thread.lock().unwrap() = Some(thread::spawn(|| Ok(())));
        session.shared.state.lock().unwrap().io_result = Some(Ok(()));
        let result =
            tokio::time::timeout(Duration::from_millis(100), session.cleanup(RequestKey(1)))
                .await
                .unwrap();
        assert_eq!(result.unwrap_err(), BridgeError::CleanupUnconfirmed);
        assert_eq!(session.data_worker_observation()["joined"], true);
        assert_eq!(signal.reason(), Some(CancelReason::Deadline));
        assert!(
            session
                .shared
                .state
                .lock()
                .unwrap()
                .control_failure
                .is_some()
        );
    }
    #[test]
    fn bounded_read_fragment_chain_discards_after_twelve_bytes_and_resets_at_full_frame() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        let mut frame = session
            .shared
            .state
            .lock()
            .unwrap()
            .admission
            .initial()
            .clone();
        frame.kind = wire::Kind::BodyChunk;
        frame.sequence = 2;
        frame.payload = wire::Payload::Chunk(wire::Chunk {
            offset: 0,
            bytes: vec![9; 16],
        });
        let bytes = frame.encode().unwrap();
        let mut framer = Framer::new();
        for (i, byte) in bytes.iter().enumerate() {
            let done = morrow_native_pipe_win::Completed {
                id: (i + 1) as u64,
                kind: IoKind::Read,
                transferred: 1,
                error: None,
                bytes: vec![*byte],
            };
            let frame = framer.push_read(&done, Some(i)).unwrap();
            if i < 12 {
                assert_eq!(framer.read_fragments.as_ref().unwrap().len(), i + 1);
            } else if i + 1 < bytes.len() {
                assert!(framer.read_fragments.is_none());
            } else {
                assert!(frame.is_some());
                assert_eq!(framer.bytes.len(), 0);
                assert!(framer.read_fragments.as_ref().unwrap().is_empty());
            }
        }
        let done = morrow_native_pipe_win::Completed {
            id: 99,
            kind: IoKind::Read,
            transferred: 4,
            error: None,
            bytes: bytes[..4].to_vec(),
        };
        assert!(framer.push_read(&done, Some(100)).unwrap().is_none());
        let fragment = &framer.read_fragments.as_ref().unwrap()[0];
        assert_eq!(fragment.read_id, 99);
        assert_eq!(fragment.io_index, 100);
        // Missing actual IO reference discards the chain instead of inventing an index.
        let done = morrow_native_pipe_win::Completed {
            id: 100,
            kind: IoKind::Read,
            transferred: 8,
            error: None,
            bytes: bytes[4..12].to_vec(),
        };
        assert!(framer.push_read(&done, None).unwrap().is_none());
        assert!(framer.read_fragments.is_none());
    }
}

#[cfg(test)]
mod terminal_writer_tests {
    use super::*;
    #[test]
    fn ended_control_input_exits_writer_without_fabricating_close_or_ack() {
        let session = Session::new(crate::admission::tests::admitted(), 1024, RequestKey(1));
        session.shared.finish_control(true, None);
        assert_eq!(control_writer(&session.shared), Err(BridgeError::Unknown));
        let state = session.shared.state.lock().unwrap();
        assert!(!state.close_written && !state.close_acked);
        assert_eq!(state.control_failure.as_ref().unwrap().source, "control_read");
    }
}
