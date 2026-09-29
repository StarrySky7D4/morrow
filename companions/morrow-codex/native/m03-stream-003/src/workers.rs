//! OS I/O ownership. Only `data_owner` creates, polls, cancels and drops Pipe.
//! No async task or control dispatch owns its blocking cleanup operation.
use crate::{
    admission::Admission,
    session::{ControlFailureObservation, IoObservation, Session, Shared},
};
use morrow_codex_m03_stream::driver::{BridgeError, RequestKey};
use morrow_native_http_stream_wire as wire;
use morrow_native_pipe_win::{Kind as IoKind, Pipe};
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
}
impl ControlReader {
    pub fn start() -> Self {
        let (tx, input) = mpsc::sync_channel(1);
        let partial = Arc::new(Mutex::new(None));
        let tracked = partial.clone();
        let started = Instant::now();
        // Blocking stdio is isolated too. Broken/partial control frames produce
        // an unconfirmed exit; they are never a request-cleanup proof.
        thread::Builder::new()
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
        }
    }
    pub fn admit(
        self,
        artifact: [u8; 32],
        max_chunk: usize,
        key: RequestKey,
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
        let routed = session.shared.clone();
        thread::Builder::new()
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
        let output = session.shared.clone();
        thread::Builder::new()
            .name("m03-control-write".into())
            .spawn(move || {
                if let Err(error) = control_writer(&output) {
                    output.fail_control("control_writer", error)
                }
            })
            .map_err(|_| BridgeError::Unknown)?;
        let data = session.shared.clone();
        let handle = thread::Builder::new()
            .name("m03-owned-pipe".into())
            .spawn(move || {
                let result = data_owner(&data);
                {
                    let mut s = data.state.lock().unwrap();
                    s.io_result = Some(result);
                    if result.is_err() {
                        s.fail(BridgeError::CleanupUnconfirmed)
                    }
                }
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
            let command =
                if state.cancel_requested && !state.cancel_written && state.admission.welcomed() {
                    state.cancel_written = true;
                    Some((wire::Kind::HttpCancel, wire::Payload::None))
                } else if let Some(command) = state.outbound.pop_front() {
                    Some(command)
                } else if state.credit_dirty && state.head.is_some() {
                    let delivery = state
                        .delivery
                        .as_ref()
                        .ok_or(BridgeError::Protocol("credit without delivery"))?;
                    let credit = delivery.credit();
                    let due = state.last_credit.as_ref().is_none_or(|last| {
                        credit.window_bytes != last.window_bytes
                            || credit.consumed_offset.saturating_sub(last.consumed_offset) >= 8192
                            || ((delivery.final_offset() == Some(credit.consumed_offset)
                                || delivery.at_limit())
                                && credit != *last)
                    });
                    if due {
                        state.credit_dirty = false;
                        state.last_credit = Some(credit.clone());
                        Some((wire::Kind::HttpCredit, wire::Payload::Credit(credit)))
                    } else {
                        None
                    }
                } else {
                    None
                };
            if let Some((kind, payload)) = command {
                let frame = state
                    .admission
                    .control(kind, payload)
                    .map_err(BridgeError::Protocol)?;
                state.requests.insert(frame.sequence, kind);
                state.record("guest_control_queued", &frame);
                Some(frame)
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

struct Framer {
    bytes: Vec<u8>,
    expected: usize,
    started: Option<Instant>,
}
impl Framer {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            expected: 4,
            started: None,
        }
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

fn data_owner(shared: &Arc<Shared>) -> Result<(), &'static str> {
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
    let result = (|| {
        loop {
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
            if cancel || closed {
                if closed && !cancel && !framer.bytes.is_empty() {
                    return Err("normal close with partial data frame");
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
            if pipe.has_operation(IoKind::Write) {
                if let Some(done) = pipe
                    .poll(IoKind::Write)
                    .map_err(|_| "write completion unconfirmed")?
                {
                    observe_completion(shared, &done);
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
            if pipe.has_operation(IoKind::Read) {
                if let Some(done) = pipe
                    .poll(IoKind::Read)
                    .map_err(|_| "read completion unconfirmed")?
                {
                    observe_completion(shared, &done);
                    if done.error.is_some() || done.transferred == 0 {
                        // Host closes its data lane before its RequestClosed
                        // control notification. Retain that cross-lane race;
                        // final HTTP/prefix facts must already justify closure.
                        let s = shared.state.lock().unwrap();
                        let disconnect = matches!(done.error, Some(109 | 232 | 233))
                            || (done.error.is_none() && done.transferred == 0);
                        let expected = s.cancel_requested
                            || (disconnect
                                && framer.bytes.is_empty()
                                && (s.closed.is_some()
                                    || s.delivery.as_ref().is_some_and(|d| {
                                        d.final_offset() == Some(d.received_offset())
                                    })));
                        if expected {
                            break;
                        } else {
                            return Err("unexpected data EOF/error");
                        }
                    }
                    if let Some(frame) = framer.push(&done.bytes)? {
                        shared
                            .state
                            .lock()
                            .unwrap()
                            .accept_data(frame)
                            .map_err(|_| "host data protocol")?;
                        shared.signal();
                    }
                }
            }
            if writing.is_none() {
                let mut s = shared.state.lock().unwrap();
                if s.data_bound && s.prepare_acked && !s.cancel_requested {
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
fn observe_completion(shared: &Shared, done: &morrow_native_pipe_win::Completed) {
    shared.state.lock().unwrap().io_event(IoObservation {
        action: "os_completion",
        id: done.id,
        kind: format!("{:?}", done.kind),
        bytes: done.transferred,
        pending: false,
        error: done.error,
    });
}
