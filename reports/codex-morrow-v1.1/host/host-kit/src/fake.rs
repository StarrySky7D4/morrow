//! Bounded, memory-only qualification implementation. Never linked by default.
//! No subprocess, credential, filesystem, clock, login or network access.
use crate::{agent_host_capnp as wire, *};
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct FixtureIdentity {
    pub session_id: String,
    pub instance_epoch: U64,
    pub plugin_id: String,
    pub package_digest: [u8; 32],
    pub artifact_digest: [u8; 32],
    pub platform: String,
    pub account_epoch: AccountEpoch,
}
impl Default for FixtureIdentity {
    fn default() -> Self {
        Self {
            session_id: "fixture-session".into(),
            instance_epoch: 1,
            plugin_id: "fixture.plugin".into(),
            package_digest: digest(b"fixture-package"),
            artifact_digest: digest(b"fixture-artifact"),
            platform: "fixture-no-os".into(),
            account_epoch: 1,
        }
    }
}
pub fn hello(identity: &FixtureIdentity, request_id: U64) -> Vec<u8> {
    let mut msg = frame(request_id, &identity.session_id, identity.instance_epoch);
    let mut v = msg
        .get_root::<wire::frame::Builder>()
        .unwrap()
        .init_native_session()
        .init_hello();
    v.set_plugin_id(&identity.plugin_id);
    v.set_package_digest(&identity.package_digest);
    v.set_artifact_digest(&identity.artifact_digest);
    v.set_platform(&identity.platform);
    v.set_account_epoch(identity.account_epoch);
    encode(&msg).unwrap()
}
#[derive(Debug)]
struct Fault {
    code: ErrorCode,
    stage: &'static str,
}
impl From<Error> for Fault {
    fn from(_: Error) -> Self {
        fault(ErrorCode::Invalid, "decode")
    }
}
impl From<capnp::Error> for Fault {
    fn from(_: capnp::Error) -> Self {
        fault(ErrorCode::Invalid, "decode")
    }
}
impl From<capnp::NotInSchema> for Fault {
    fn from(_: capnp::NotInSchema) -> Self {
        fault(ErrorCode::Invalid, "enum")
    }
}
fn fault(code: ErrorCode, stage: &'static str) -> Fault {
    Fault { code, stage }
}
type ResultF<T> = Result<T, Fault>;
fn hash32(bytes: &[u8]) -> ResultF<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|_| fault(ErrorCode::Invalid, "digest"))
}
fn resource(r: wire::resource_ref::Reader<'_>, expected: &str) -> ResultF<()> {
    if id(r.get_namespace())? != "fixture"
        || id(r.get_id())? != expected
        || r.get_revision() != 9007199254740993
        || r.get_byte_length() != 0
        || r.get_sha256()? != digest(b"fixture-reference")
    {
        return Err(fault(ErrorCode::Denied, "resource"));
    }
    Ok(())
}

struct StreamData {
    expected: usize,
    expected_digest: [u8; 32],
    body: Vec<u8>,
    response: Vec<u8>,
    state: StreamState,
    offset: usize,
    sequence: U64,
    fixed: bool,
    deadline: U64,
    last_read: Option<(usize, usize, U64, StreamState)>,
}
#[derive(Clone)]
struct EventData {
    turn: String,
    item: String,
    part: String,
    attempt: String,
    kind: String,
    source_digest: [u8; 32],
    payload: Vec<u8>,
}
impl EventData {
    fn read(v: wire::agent_event::Reader<'_>) -> ResultF<Self> {
        let payload = v.get_payload()?.to_vec();
        if payload.len() > MAX_CHUNK_BYTES {
            return Err(fault(ErrorCode::Limit, "event-payload"));
        }
        let source_digest = hash32(v.get_source_digest()?)?;
        if digest(&payload) != source_digest {
            return Err(fault(ErrorCode::Integrity, "event-payload"));
        }
        Ok(Self {
            turn: id(v.get_turn_id())?,
            item: id(v.get_item_id())?,
            part: id(v.get_part_id())?,
            attempt: id(v.get_attempt_id())?,
            kind: id(v.get_semantic_kind())?,
            source_digest,
            payload,
        })
    }
    fn write(&self, mut v: wire::agent_event::Builder<'_>) {
        v.set_turn_id(&self.turn);
        v.set_item_id(&self.item);
        v.set_part_id(&self.part);
        v.set_attempt_id(&self.attempt);
        v.set_semantic_kind(&self.kind);
        v.set_source_digest(&self.source_digest);
        v.set_payload(&self.payload);
    }
}
struct Batch {
    request_digest: [u8; 32],
    tail: U64,
}
struct ToolData {
    request_digest: [u8; 32],
    state: ToolState,
    permit: Option<[u8; 32]>,
    exit_code: i32,
    output_closed: bool,
    report_digest: Option<[u8; 32]>,
}

pub struct FakeHost {
    identity: FixtureIdentity,
    attached: bool,
    draining: bool,
    clock_ms: U64,
    streams: BTreeMap<String, StreamData>,
    response: Vec<u8>,
    writer_epoch: U64,
    events: Vec<EventData>,
    batches: BTreeMap<(U64, U64), Batch>,
    tools: BTreeMap<String, ToolData>,
}
impl FakeHost {
    /// Identity and response are supplied by the trusted test harness, not Hello.
    pub fn new(identity: FixtureIdentity, response: Vec<u8>) -> Result<Self, Error> {
        if response.len() > MAX_REQUEST_BYTES
            || !valid_id(&identity.session_id)
            || !valid_id(&identity.plugin_id)
            || !valid_id(&identity.platform)
            || identity.instance_epoch == 0
            || identity.account_epoch == 0
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            identity,
            attached: false,
            draining: false,
            clock_ms: 0,
            streams: BTreeMap::new(),
            response,
            writer_epoch: 1,
            events: Vec::new(),
            batches: BTreeMap::new(),
            tools: BTreeMap::new(),
        })
    }
    pub fn set_clock_ms(&mut self, now: U64) -> Result<(), Error> {
        if now < self.clock_ms {
            return Err(Error::Invalid);
        }
        self.clock_ms = now;
        Ok(())
    }
    /// Test harness only: model a host-owned writer handoff. Never a wire request.
    pub fn handoff_writer(&mut self) -> Result<U64, Error> {
        self.writer_epoch = self.writer_epoch.checked_add(1).ok_or(Error::Limit)?;
        Ok(self.writer_epoch)
    }
    /// Test harness only: approve this exact proposal in this live fixture epoch.
    pub fn approve(&mut self, operation: &str) -> Result<[u8; 32], Error> {
        if !self.attached || self.draining {
            return Err(Error::Invalid);
        }
        let tool = self.tools.get_mut(operation).ok_or(Error::Invalid)?;
        if tool.state != ToolState::Proposed {
            return Err(Error::Invalid);
        }
        let mut bytes = b"QUALIFICATION-ONLY-NOT-A-PRODUCTION-PERMIT".to_vec();
        bytes.extend_from_slice(operation.as_bytes());
        bytes.extend_from_slice(&tool.request_digest);
        bytes.extend_from_slice(self.identity.session_id.as_bytes());
        bytes.extend_from_slice(&self.identity.instance_epoch.to_le_bytes());
        let permit = digest(&bytes);
        tool.permit = Some(permit);
        tool.state = ToolState::Approved;
        Ok(permit)
    }
    fn dispatch(
        &mut self,
        f: wire::frame::Reader<'_>,
        raw: &[u8],
        mut out: wire::reply::Builder<'_>,
    ) -> ResultF<()> {
        use wire::frame::Which;
        if id(f.get_session_id())? != self.identity.session_id
            || f.get_instance_epoch() != self.identity.instance_epoch
        {
            return Err(fault(ErrorCode::StaleEpoch, "connection"));
        }
        out.set_qualification_only(true);
        if let Which::NativeSession(request) = f.which()? {
            use wire::native_session::Which as N;
            match request?.which()? {
                N::Hello(identity) => {
                    let v = identity?;
                    if self.draining
                        || id(v.get_plugin_id())? != self.identity.plugin_id
                        || v.get_package_digest()? != self.identity.package_digest
                        || v.get_artifact_digest()? != self.identity.artifact_digest
                        || id(v.get_platform())? != self.identity.platform
                        || v.get_account_epoch() != self.identity.account_epoch
                    {
                        return Err(fault(ErrorCode::Denied, "hello"));
                    }
                    self.attached = true;
                    let mut reply = out.init_native_session();
                    reply.set_qualification_only(true);
                    reply.set_capability_bits(15);
                    return Ok(());
                }
                N::Drain(()) if self.attached => {
                    self.draining = true;
                    for stream in self.streams.values_mut() {
                        if !matches!(stream.state, StreamState::Eof | StreamState::Closed) {
                            stream.state = StreamState::Cancelled;
                        }
                    }
                    for tool in self.tools.values_mut() {
                        tool.permit = None;
                    }
                    let mut reply = out.init_native_session();
                    reply.set_qualification_only(true);
                    reply.set_closing_unconfirmed(true);
                    return Ok(());
                }
                N::ObserveExit(()) if self.attached => {
                    return Err(fault(ErrorCode::Unavailable, "observe-exit"));
                }
                _ => return Err(fault(ErrorCode::Denied, "hello-required")),
            }
        }
        if !self.attached {
            return Err(fault(ErrorCode::Denied, "hello-required"));
        }
        match f.which()? {
            Which::Stream(v) => self.stream(v?, out.init_stream()),
            Which::Event(v) => self.event(v?, raw, out.init_event()),
            Which::Tool(v) => self.tool(v?, raw, out.init_tool()),
            _ => Err(fault(ErrorCode::Invalid, "request-direction")),
        }
    }
    fn stream(
        &mut self,
        request: wire::stream::Reader<'_>,
        mut out: wire::stream_receipt::Builder<'_>,
    ) -> ResultF<()> {
        use wire::stream::Which as S;
        let attempt = id(request.get_attempt_id())?;
        let kind = request.which()?;
        if self.draining && !matches!(kind, S::Inspect(()) | S::Cancel(()) | S::Close(())) {
            return Err(fault(ErrorCode::Denied, "draining"));
        }
        if let S::Open(v) = request.which()? {
            let v = v?;
            resource(v.get_destination()?, "destination")?;
            let deadline = v.get_deadline()?;
            if deadline.get_domain()? != ClockDomain::MonotonicMillis
                || id(deadline.get_clock_id())? != "fixture-clock"
                || deadline.get_value() <= self.clock_ms
            {
                return Err(fault(ErrorCode::Denied, "deadline"));
            }
            if v.get_request_bytes() > MAX_REQUEST_BYTES as u64 || self.streams.len() >= MAX_STREAMS
            {
                return Err(fault(ErrorCode::Limit, "stream-open"));
            }
            if self.streams.contains_key(&attempt) {
                return Err(fault(ErrorCode::Conflict, "attempt-exists"));
            }
            self.streams.insert(
                attempt.clone(),
                StreamData {
                    expected: v.get_request_bytes() as usize,
                    expected_digest: hash32(v.get_request_digest()?)?,
                    body: Vec::new(),
                    response: self.response.clone(),
                    state: StreamState::Prepared,
                    offset: 0,
                    sequence: 0,
                    fixed: false,
                    deadline: deadline.get_value(),
                    last_read: None,
                },
            );
        }
        let stream = self
            .streams
            .get_mut(&attempt)
            .ok_or(fault(ErrorCode::Denied, "unknown-attempt"))?;
        if self.clock_ms >= stream.deadline
            && !matches!(kind, S::Inspect(()) | S::Cancel(()) | S::Close(()))
        {
            return Err(fault(ErrorCode::Denied, "deadline"));
        }
        out.set_offset(stream.offset as u64);
        match kind {
            S::Open(_) | S::Inspect(()) => {}
            S::WriteChunk(v) => {
                let v = v?;
                let bytes = v.get_bytes()?;
                if stream.state != StreamState::Prepared
                    || v.get_offset() != stream.body.len() as u64
                {
                    return Err(fault(ErrorCode::Conflict, "request-fixed-or-offset"));
                }
                if bytes.is_empty()
                    || bytes.len() > MAX_CHUNK_BYTES
                    || stream.body.len() + bytes.len() > stream.expected
                {
                    return Err(fault(ErrorCode::Limit, "request-chunk"));
                }
                stream.body.extend_from_slice(bytes);
            }
            S::CommitRequest(()) => {
                if stream.state != StreamState::Prepared {
                    return Err(fault(ErrorCode::Conflict, "already-committed"));
                }
                if stream.body.len() != stream.expected
                    || digest(&stream.body) != stream.expected_digest
                {
                    return Err(fault(ErrorCode::Integrity, "commit-request"));
                }
                stream.fixed = true;
                stream.state = StreamState::Committed;
            }
            S::Read(v) => {
                let v = v?;
                let max = v.get_max_bytes();
                let offset = v.get_expected_offset();
                if max == 0 || max as usize > MAX_CHUNK_BYTES {
                    return Err(fault(ErrorCode::Limit, "stream-read"));
                }
                if !matches!(
                    stream.state,
                    StreamState::Committed | StreamState::Streaming | StreamState::Eof
                ) {
                    return Err(fault(ErrorCode::Conflict, "read-state"));
                }
                if let Some((old_offset, old_max, old_sequence, old_state)) = stream.last_read {
                    if offset == old_offset as u64 && max as usize == old_max {
                        let end = (old_offset + old_max).min(stream.response.len());
                        out.set_offset(old_offset as u64);
                        out.set_bytes(&stream.response[old_offset..end]);
                        out.set_state(old_state);
                        out.set_transport_sequence(old_sequence);
                        out.set_request_fixed(true);
                        return Ok(());
                    }
                }
                if offset != stream.offset as u64 {
                    return Err(fault(ErrorCode::Conflict, "read-offset"));
                }
                let end = (stream.offset + max as usize).min(stream.response.len());
                out.set_offset(stream.offset as u64);
                out.set_bytes(&stream.response[stream.offset..end]);
                if end > stream.offset {
                    stream.sequence += 1;
                }
                let start = stream.offset;
                stream.offset = end;
                stream.state = if end == stream.response.len() {
                    StreamState::Eof
                } else {
                    StreamState::Streaming
                };
                stream.last_read = Some((start, max as usize, stream.sequence, stream.state));
            }
            S::Cancel(()) => {
                if !matches!(stream.state, StreamState::Eof | StreamState::Closed) {
                    stream.state = StreamState::Cancelled;
                }
            }
            S::Close(()) => {
                stream.body.clear();
                stream.response.clear();
                stream.state = StreamState::Closed;
            }
        }
        out.set_state(stream.state);
        out.set_transport_sequence(stream.sequence);
        out.set_request_fixed(stream.fixed);
        Ok(())
    }
    fn event(
        &mut self,
        request: wire::event::Reader<'_>,
        raw: &[u8],
        mut out: wire::event_receipt::Builder<'_>,
    ) -> ResultF<()> {
        use wire::event::Which as E;
        if request.get_writer_epoch() != self.writer_epoch {
            return Err(fault(ErrorCode::StaleEpoch, "writer"));
        }
        out.set_writer_epoch(self.writer_epoch);
        match request.which()? {
            E::OpenWriter(()) => {
                if self.draining {
                    return Err(fault(ErrorCode::Denied, "draining"));
                }
                out.set_durable_sequence(self.events.len() as u64);
            }
            E::AppendBatch(v) => {
                if self.draining {
                    return Err(fault(ErrorCode::Denied, "draining"));
                }
                let v = v?;
                let seq = v.get_producer_sequence();
                if seq == 0 {
                    return Err(fault(ErrorCode::Invalid, "producer-sequence"));
                }
                let key = (self.writer_epoch, seq);
                let request_digest = digest(raw);
                if let Some(batch) = self.batches.get(&key) {
                    if batch.request_digest != request_digest {
                        return Err(fault(ErrorCode::Conflict, "append-key-bytes"));
                    }
                    out.set_durable_sequence(batch.tail);
                    out.set_replayed(true);
                    return Ok(());
                }
                if v.get_expected_tail() != self.events.len() as u64 {
                    return Err(fault(ErrorCode::Conflict, "tail-cas"));
                }
                let events = v.get_events()?;
                if events.is_empty()
                    || events.len() as usize > MAX_BATCH_EVENTS
                    || self.events.len() + events.len() as usize > MAX_EVENTS
                {
                    return Err(fault(ErrorCode::Limit, "event-batch"));
                }
                let batch: Vec<EventData> =
                    events.iter().map(EventData::read).collect::<ResultF<_>>()?;
                self.events.extend(batch);
                let tail = self.events.len() as u64;
                self.batches.insert(
                    key,
                    Batch {
                        request_digest,
                        tail,
                    },
                );
                out.set_durable_sequence(tail);
            }
            E::ReadAfter(after) => {
                if after > self.events.len() as u64 {
                    return Err(fault(ErrorCode::Conflict, "future-cursor"));
                }
                let start = after as usize;
                let end = (start + MAX_BATCH_EVENTS).min(self.events.len());
                out.set_durable_sequence(end as u64);
                let mut list = out.init_events((end - start) as u32);
                for (i, event) in self.events[start..end].iter().enumerate() {
                    event.write(list.reborrow().get(i as u32));
                }
            }
        }
        Ok(())
    }
    fn tool(
        &mut self,
        request: wire::tool::Reader<'_>,
        raw: &[u8],
        mut out: wire::tool_receipt::Builder<'_>,
    ) -> ResultF<()> {
        use wire::tool::Which as T;
        let operation = id(request.get_operation_id())?;
        let kind = request.which()?;
        if self.draining && !matches!(kind, T::Report(_) | T::Inspect(())) {
            return Err(fault(ErrorCode::Denied, "draining"));
        }
        if let T::Propose(v) = request.which()? {
            let v = v?;
            id(v.get_root_operation())?;
            id(v.get_attempt_id())?;
            hash32(v.get_input_digest()?)?;
            hash32(v.get_tool_schema_digest()?)?;
            if v.get_executor_artifact()? != self.identity.artifact_digest {
                return Err(fault(ErrorCode::Denied, "executor-artifact"));
            }
            resource(v.get_source()?, "source")?;
            if let Some(old) = self.tools.get(&operation) {
                if old.request_digest != digest(raw) {
                    return Err(fault(ErrorCode::Conflict, "operation-intent"));
                }
            } else {
                if self.tools.len() >= MAX_TOOLS {
                    return Err(fault(ErrorCode::Limit, "tools"));
                }
                self.tools.insert(
                    operation.clone(),
                    ToolData {
                        request_digest: digest(raw),
                        state: ToolState::Proposed,
                        permit: None,
                        exit_code: 0,
                        output_closed: false,
                        report_digest: None,
                    },
                );
            }
        }
        let tool = self
            .tools
            .get_mut(&operation)
            .ok_or(fault(ErrorCode::Denied, "unknown-operation"))?;
        match kind {
            T::Propose(_) | T::Inspect(()) => {}
            T::Claim(permit) => {
                if tool.permit != Some(hash32(permit?)?) {
                    return Err(fault(ErrorCode::Denied, "permit"));
                }
                if tool.state == ToolState::Approved {
                    tool.state = ToolState::Claimed;
                    out.set_execute(true);
                } else if !matches!(
                    tool.state,
                    ToolState::Claimed | ToolState::Reported | ToolState::Unknown
                ) {
                    return Err(fault(ErrorCode::Conflict, "claim-state"));
                }
            }
            T::Report(v) => {
                let v = v?;
                let state = v.get_state()?;
                if !matches!(state, ToolState::Reported | ToolState::Unknown) {
                    return Err(fault(ErrorCode::Invalid, "report-state"));
                }
                hash32(v.get_output_digest()?)?;
                // Bind retry to the complete report bytes; never replace a final/unknown receipt.
                if let Some(old) = tool.report_digest {
                    if old != digest(raw) {
                        return Err(fault(ErrorCode::Conflict, "report-conflict"));
                    }
                } else {
                    if tool.state != ToolState::Claimed {
                        return Err(fault(ErrorCode::Denied, "report-before-claim"));
                    }
                    tool.state = state;
                    tool.exit_code = v.get_exit_code();
                    tool.output_closed = v.get_output_closed();
                    tool.report_digest = Some(digest(raw));
                }
            }
        }
        out.set_state(tool.state);
        out.set_exit_code(tool.exit_code);
        out.set_output_closed(tool.output_closed);
        Ok(())
    }
}
impl Transport for FakeHost {
    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, Error> {
        let input = decode(request)?;
        let f = input.get_root::<wire::frame::Reader>()?;
        if matches!(f.which()?, wire::frame::Which::Reply(_)) {
            return Err(Error::WrongDirection);
        }
        let mut output = frame(
            f.get_request_id(),
            &text(f.get_session_id())?,
            f.get_instance_epoch(),
        );
        let reply = output.get_root::<wire::frame::Builder>()?.init_reply();
        if let Err(failure) = self.dispatch(f, request, reply) {
            let mut reply = output.get_root::<wire::frame::Builder>()?.init_reply();
            reply.set_qualification_only(true);
            let mut error = reply.init_error();
            error.set_code(failure.code);
            error.set_stage(failure.stage);
            error.set_recovery("inspect-original-do-not-retry-effects");
            error.set_diagnostic_id("fixture-no-private-diagnostic");
            match f.which()? {
                wire::frame::Which::Stream(v) => {
                    if let Ok(v) = id(v?.get_attempt_id()) {
                        error.set_attempt_id(v);
                    }
                }
                wire::frame::Which::Tool(v) => {
                    if let Ok(v) = id(v?.get_operation_id()) {
                        error.set_operation_id(v);
                    }
                }
                _ => {}
            }
        }
        encode(&output)
    }
}
