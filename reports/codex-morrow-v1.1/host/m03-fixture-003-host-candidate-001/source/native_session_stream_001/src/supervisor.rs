use crate::{
    Admission, Control, Result, Shared,
    http_authority::{NativeHttpGrant, Parent, Proposal},
    pipe_driver::{self, Driver},
    revoke, wire as w,
};
use morrow_core::lifecycle::{HostPolicy, Instance};
use morrow_native_pipe_win::Kind as PipeKind;
use morrow_network_node_stream::stream::{Completion, StreamLease};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Child,
    sync::mpsc,
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
type ChunkResult = morrow_network_node_stream::Result<Option<Vec<u8>>>;
#[cfg(test)]
#[path = "revocation_interleaving_tests.rs"]
mod revocation_interleaving_tests;
enum Network {
    Opening(JoinHandle<morrow_network_node_stream::Result<StreamLease>>),
    Reading(JoinHandle<(StreamLease, ChunkResult)>),
    Finishing(JoinHandle<Completion>),
}
impl Network {
    fn finished(&self) -> bool {
        match self {
            Self::Opening(t) => t.is_finished(),
            Self::Reading(t) => t.is_finished(),
            Self::Finishing(t) => t.is_finished(),
        }
    }
}
struct Output {
    bytes: Vec<u8>,
    offset: usize,
    deadline: Instant,
}
struct Http {
    #[cfg(feature = "qualification-pipe-fault")]
    fault_bound: Option<crate::qualification_pipe::BoundPlan>,
    #[cfg(feature = "qualification-pipe-fault")]
    fault_cut_sent: bool,
    initial: w::Frame,
    parent: Parent,
    shared: Arc<Shared>,
    progress: w::Progress,
    control: VecDeque<Output>,
    frame_ms: u64,
    pipe: Driver,
    channel: w::Channel,
    pipe_created: bool,
    connected: bool,
    bound: bool,
    offered: bool,
    pipe_joined: bool,
    pipe_join_tick: bool,
    data_input: Vec<u8>,
    data_target: usize,
    data_deadline: Option<Instant>,
    read_waiting: bool,
    data_sequence: u64,
    out_sequence: u64,
    data_outputs: VecDeque<(Option<u64>, Vec<u8>)>,
    prepare: Option<w::Prepare>,
    request_body: Vec<u8>,
    proposal: Option<Proposal>,
    proposal_json: Value,
    grant: Option<NativeHttpGrant>,
    credit: w::Credit,
    network: Option<Network>,
    lease: Option<StreamLease>,
    cancel: CancellationToken,
    head: Option<w::Head>,
    body_pending: Vec<u8>,
    response_body: Vec<u8>,
    request_close_sent: bool,
}
fn progress() -> w::Progress {
    w::Progress {
        intent: w::IntentPhase::Absent,
        network: w::NetworkPhase::Idle,
        owner: w::OwnerPhase::Preparing,
        http_status: 0,
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
        http_eof: false,
        response_material_stored: false,
        worker_started: false,
        worker_joined: false,
        connect_reaped: false,
        read_reaped: false,
        write_reaped: false,
        data_closed: false,
        child_exited: false,
        stdout_eof: false,
        stderr_eof: false,
        owner_released: false,
        request_closed: false,
    }
}
fn describe(p: &w::Progress) -> Value {
    json!({"intent":format!("{:?}",p.intent),"network":format!("{:?}",p.network),"owner":format!("{:?}",p.owner),"http_status":p.http_status,"error_code":p.error_code,"received_offset":p.received_offset,"reserved_offset":p.reserved_offset,"issued_offset":p.issued_offset,"os_completed_offset":p.os_completed_offset,"peer_consumed_offset":p.peer_consumed_offset,"parser_yielded_bytes":p.parser_yielded_bytes,"drain_discarded_bytes":p.drain_discarded_bytes,"error_consumed_bytes":p.error_consumed_bytes,"cancel_discarded_bytes":p.cancel_discarded_bytes,"last_write_ordinal":p.last_write_ordinal,"revoke_persisted":p.revoke_persisted,"revoke_applied":p.revoke_applied,"http_eof":p.http_eof,"response_material_stored":p.response_material_stored,"worker_started":p.worker_started,"worker_joined":p.worker_joined,"connect_reaped":p.connect_reaped,"read_reaped":p.read_reaped,"write_reaped":p.write_reaped,"data_closed":p.data_closed,"request_closed":p.request_closed,"child_exited":p.child_exited,"stdout_eof":p.stdout_eof,"stderr_eof":p.stderr_eof,"owner_released":p.owner_released})
}
impl Http {
    fn new(
        initial: w::Frame,
        parent: Parent,
        shared: Arc<Shared>,
        frame_ms: u64,
        #[cfg(feature = "qualification-pipe-fault")] fault_plan: Option<crate::PipeFaultPlan>,
    ) -> Result<Self> {
        let mut nonce = vec![0; 32];
        getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
        let channel = w::Channel {
            locator: format!(r"\\.\pipe\morrow-m03-{}", w::hex(&nonce)),
            nonce,
            max_chunk_bytes: 8192,
            credit_limit: 16384,
        };
        #[cfg(feature = "qualification-pipe-fault")]
        let fault_bound = fault_plan
            .map(|p| crate::qualification_pipe::BoundPlan::new(p, &initial, parent.deadline))
            .transpose()?;
        #[cfg(feature = "qualification-pipe-fault")]
        let pipe = if let Some(bound) = &fault_bound {
            shared.event("qualification_pipe_plan_bound", bound.description());
            Driver::spawn_with_fault(
                channel.locator.clone(),
                initial.child_pid,
                parent.gate.clone(),
                bound.clone(),
            )?
        } else {
            Driver::spawn(
                channel.locator.clone(),
                initial.child_pid,
                parent.gate.clone(),
            )?
        };
        #[cfg(not(feature = "qualification-pipe-fault"))]
        let pipe = Driver::spawn(
            channel.locator.clone(),
            initial.child_pid,
            parent.gate.clone(),
        )?;
        Ok(Self {
            #[cfg(feature = "qualification-pipe-fault")]
            fault_bound,
            #[cfg(feature = "qualification-pipe-fault")]
            fault_cut_sent: false,
            initial,
            parent,
            shared,
            progress: progress(),
            control: VecDeque::new(),
            frame_ms,
            pipe,
            channel,
            pipe_created: false,
            connected: false,
            bound: false,
            offered: false,
            pipe_joined: false,
            pipe_join_tick: false,
            data_input: vec![],
            data_target: 4,
            data_deadline: None,
            read_waiting: false,
            data_sequence: 0,
            out_sequence: 1,
            data_outputs: VecDeque::new(),
            prepare: None,
            request_body: vec![],
            proposal: None,
            proposal_json: Value::Null,
            grant: None,
            credit: w::Credit {
                consumed_offset: 0,
                parser_yielded_bytes: 0,
                drain_discarded_bytes: 0,
                cancel_discarded_bytes: 0,
                error_consumed_bytes: 0,
                window_bytes: 0,
                max_chunk_bytes: 8192,
            },
            network: None,
            lease: None,
            cancel: CancellationToken::new(),
            head: None,
            body_pending: vec![],
            response_body: vec![],
            request_close_sent: false,
        })
    }
    fn publish(&self) {
        self.shared.state.lock().unwrap().http =
            json!({"progress":describe(&self.progress),"proposal":self.proposal_json});
    }
    fn frame(&self, kind: w::Kind, seq: u64, code: u32, payload: w::Payload) -> w::Frame {
        let mut f = self.initial.clone();
        f.kind = kind;
        f.sequence = seq;
        f.code = code;
        f.revocation_generation = self.shared.state.lock().unwrap().generation;
        f.remaining_ms = self
            .parent
            .deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as u64;
        f.payload = payload;
        f
    }
    fn queue(&mut self, kind: w::Kind, seq: u64, code: u32, payload: w::Payload) -> Result<()> {
        if self.control.len() >= 8 {
            return Err("control queue limit".into());
        }
        let bytes = self
            .frame(kind, seq, code, payload)
            .encode()
            .map_err(str::to_owned)?;
        self.control.push_back(Output {
            bytes,
            offset: 0,
            deadline: Instant::now() + Duration::from_millis(self.frame_ms),
        });
        Ok(())
    }
    fn state(&mut self, kind: w::Kind, seq: u64) -> Result<()> {
        self.queue(kind, seq, 0, w::Payload::Progress(self.progress.clone()))
    }
    fn send_data(&mut self, frame: w::Frame, end: Option<u64>) -> Result<bool> {
        if self.data_outputs.len() >= 2 {
            return Ok(false);
        }
        let bytes = frame.encode().map_err(str::to_owned)?;
        #[cfg(feature = "qualification-pipe-fault")]
        if frame.kind == w::Kind::BodyChunk && self.fault_bound.is_some() {
            if self.fault_cut_sent {
                return Ok(false);
            }
            return match self.pipe.send(pipe_driver::Command::PrefixFrame {
                bytes: bytes.clone(),
                body_end: end,
            }) {
                Ok(()) => {
                    self.fault_cut_sent = true;
                    self.data_outputs.push_back((end, bytes));
                    self.progress.write_reaped = false;
                    Ok(true)
                }
                Err(std::sync::mpsc::TrySendError::Full(_)) => Ok(false),
                Err(_) => Err("qualification data writer ended".into()),
            };
        }
        match self.pipe.send(pipe_driver::Command::Write {
            bytes: bytes.clone(),
            body_end: end,
        }) {
            Ok(()) => {
                self.data_outputs.push_back((end, bytes));
                self.progress.write_reaped = false;
                Ok(true)
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => Ok(false),
            Err(_) => Err("data writer ended".into()),
        }
    }
    fn propose(&mut self) -> Result<()> {
        let p = self.prepare.take().ok_or("no Prepare")?;
        let body = std::mem::take(&mut self.request_body);
        let proposal = self.parent.proposal(&self.initial, p, body)?;
        self.proposal_json = json!({"proposal_ref":w::hex(&proposal.decision.proposal_ref),"body_sha256":w::hex(&proposal.decision.body_sha256),"request_sha256":w::hex(&proposal.decision.request_sha256),"method":proposal.prepare.method,"absolute_target":proposal.prepare.absolute_target,"headers":proposal.prepare.headers.iter().map(|h|json!({"name":h.name,"value_hex":w::hex(&h.value)})).collect::<Vec<_>>(),"body_hex":w::hex(&proposal.body),"response_limit":proposal.prepare.response_limit_bytes,"approved":false});
        self.queue(
            w::Kind::HttpProposed,
            0,
            0,
            w::Payload::Decision(proposal.decision.clone()),
        )?;
        self.shared.event("proposal_complete",json!({"request_sha256":w::hex(&proposal.decision.request_sha256),"body_bytes":proposal.body.len()}));
        self.proposal = Some(proposal);
        Ok(())
    }
    fn approve(&mut self, reference: Vec<u8>, hash: Vec<u8>, limit: u32) -> Result<Value> {
        if self.grant.is_some() || self.cancel.is_cancelled() {
            return Err("HTTP approval state".into());
        }
        // Preserve proposal after an invalid expected hash, before any durable change.
        let proposal = self.proposal.as_ref().ok_or("proposal incomplete")?;
        if proposal.decision.proposal_ref != reference
            || proposal.decision.request_sha256 != hash
            || limit == 0
            || limit > proposal.prepare.response_limit_bytes
        {
            return Err("proposal expected digest mismatch".into());
        }
        let grant = NativeHttpGrant::approve(
            self.parent.clone(),
            self.proposal.take().unwrap(),
            &reference,
            &hash,
            limit,
        )?;
        let decision = grant.decision().clone();
        self.grant = Some(grant);
        self.proposal_json["approved"] = json!(true);
        self.queue(
            w::Kind::HttpApproved,
            0,
            0,
            w::Payload::Decision(decision.clone()),
        )?;
        self.shared.event("http_approved",json!({"grant_ref":w::hex(&decision.http_grant_ref),"response_limit":limit,"request_sha256":w::hex(&hash)}));
        Ok(json!({"http_grant_ref":w::hex(&decision.http_grant_ref),"response_limit":limit}))
    }
    fn cancel_http(&mut self, code: u32) -> Result<()> {
        let first = !self.progress.revoke_applied;
        let requested_code = code;
        // Persist provenance together with state3. The first durable revocation wins;
        // an external revocation that precedes Close must still apply as reason19.
        let receipt = self.parent.revoke_native(code)?;
        let code = receipt.reason;
        if !first {
            self.shared.event("http_cancel_already_applied",json!({"requested_code":requested_code,"effective_code":code,"source":receipt.source}));
            return Ok(());
        }
        let completed_close = code == 25 && self.progress.request_closed;
        if !self.progress.revoke_persisted {
            if let Some(g) = &mut self.grant {
                g.revoke()?;
            }
            self.progress.revoke_persisted = true;
        }
        let effect_fence = {
            let mut g = self.parent.gate.lock().map_err(|_| "gate poison")?;
            let previously_revoked = g.revoked;
            g.revoked = true;
            json!({"previously_revoked":previously_revoked,"gate_closed":true,
                "gate_closed_after_ordinal":g.ordinal,"last_write_ordinal":g.last_write,
                "original_deadline_offset_ns":g.deadline.saturating_duration_since(self.shared.created).as_nanos(),
                "parent_deadline_matches_gate":g.deadline==self.parent.deadline})
        };
        self.progress.revoke_applied = true;
        if !completed_close {
            self.progress.error_code = code;
        }
        self.cancel.cancel();
        self.pipe.cancel();
        self.body_pending.clear();
        if !self.progress.http_eof && self.progress.worker_started {
            self.progress.network = if matches!(code, 19 | 25) {
                w::NetworkPhase::Cancelled
            } else {
                w::NetworkPhase::Failed
            };
        }
        self.shared.state.lock().unwrap().generation = 2;
        self.shared.event(
            "http_cancel_applied",
            json!({"code":code,"requested_code":requested_code,"source":receipt.source,"progress":describe(&self.progress),"effect_fence":effect_fence}),
        );
        if first && !completed_close {
            self.state(w::Kind::HttpTerminal, 0)?;
        }
        Ok(())
    }
    fn handle_control(&mut self, f: w::Frame) -> Result<()> {
        match (f.kind, f.payload) {
            (w::Kind::HttpPrepare, w::Payload::Prepare(p)) => {
                if !self.bound
                    || self.prepare.is_some()
                    || self.proposal.is_some()
                    || self.grant.is_some()
                {
                    return Err("Prepare state".into());
                }
                let empty = p.body_bytes == 0;
                self.prepare = Some(p);
                self.state(w::Kind::State, f.sequence)?;
                if empty {
                    self.propose()?;
                }
            }
            (w::Kind::HttpCommit, w::Payload::Decision(d)) => {
                let grant = self.grant.as_mut().ok_or("HTTP not approved")?;
                let result = grant.claim(&d, self.cancel.clone());
                self.progress.intent = grant.intent_phase()?;
                let ticket = result?;
                self.progress.worker_started = true;
                self.progress.network = w::NetworkPhase::AwaitingHead;
                self.shared.event("send_fence",json!({"ordinal":ticket.ordinal,"intent":"Unknown","server_received_proven":false}));
                self.network = Some(Network::Opening(tokio::spawn(async move {
                    let owner = ticket.owner;
                    let result = ticket
                        .client
                        .send_stream(ticket.request, ticket.context)
                        .await;
                    drop(owner);
                    result
                })));
                self.state(w::Kind::State, f.sequence)?;
            }
            (w::Kind::HttpCredit, w::Payload::Credit(c)) => {
                if c.consumed_offset > self.progress.issued_offset
                    || c.consumed_offset < self.credit.consumed_offset
                    || c.parser_yielded_bytes < self.credit.parser_yielded_bytes
                    || c.drain_discarded_bytes < self.credit.drain_discarded_bytes
                    || c.error_consumed_bytes < self.credit.error_consumed_bytes
                    || c.cancel_discarded_bytes < self.credit.cancel_discarded_bytes
                {
                    return Err("credit prefix/classification".into());
                }
                self.progress.peer_consumed_offset = c.consumed_offset;
                self.progress.parser_yielded_bytes = c.parser_yielded_bytes;
                self.progress.drain_discarded_bytes = c.drain_discarded_bytes;
                self.progress.error_consumed_bytes = c.error_consumed_bytes;
                self.progress.cancel_discarded_bytes = c.cancel_discarded_bytes;
                self.credit = c;
                self.state(w::Kind::CreditState, f.sequence)?;
            }
            (w::Kind::HttpCancel, w::Payload::None) => {
                self.cancel_http(19)?;
                self.state(w::Kind::CancelAccepted, f.sequence)?;
            }
            (w::Kind::Query, w::Payload::None) => self.state(w::Kind::State, f.sequence)?,
            _ => return Err("unsupported control payload".into()),
        }
        Ok(())
    }
    fn data_frame(&mut self, f: w::Frame) -> Result<()> {
        if !identity(&f, &self.initial)
            || f.code != 0
            || f.sequence != self.data_sequence + 1
            || self.data_sequence >= 128
        {
            return Err("data identity/sequence".into());
        }
        self.data_sequence = f.sequence;
        match (f.kind, f.payload) {
            (w::Kind::DataBind, w::Payload::Channel(c)) if !self.bound && c == self.channel => {
                let frame = self.frame(
                    w::Kind::DataBound,
                    1,
                    0,
                    w::Payload::Channel(self.channel.clone()),
                );
                if !self.send_data(frame, None)? {
                    return Err("DataBound queue unavailable".into());
                }
                self.out_sequence = 2;
                self.bound = true;
            }
            (w::Kind::RequestChunk, w::Payload::Chunk(c)) if self.bound => {
                let p = self
                    .prepare
                    .as_ref()
                    .ok_or("request chunk before acknowledged Prepare")?;
                if c.offset != self.request_body.len() as u64
                    || self.request_body.len() + c.bytes.len() > p.body_bytes as usize
                {
                    return Err("request offset/length".into());
                }
                self.request_body.extend(c.bytes);
                if self.request_body.len() == p.body_bytes as usize {
                    self.propose()?;
                }
            }
            _ => return Err("data kind/state".into()),
        }
        Ok(())
    }
    fn pipe_events(&mut self) -> Result<()> {
        while let Some(event) = self.pipe.event() {
            match event {
                #[cfg(feature = "qualification-pipe-fault")]
                pipe_driver::Event::QualificationObservation { event, detail } => {
                    self.shared.event(event, detail)
                }
                pipe_driver::Event::Created {
                    inbound,
                    outbound,
                    dacl,
                    noninherited,
                } => {
                    self.pipe_created = true;
                    self.shared.event("data_created",json!({"inbound":inbound,"outbound":outbound,"dacl_sha256":w::hex(&w::digest(dacl.as_bytes())),"noninherited":noninherited}));
                }
                pipe_driver::Event::Connected { pid } => {
                    self.connected = true;
                    self.shared.event("data_peer_pid", json!({"pid":pid}));
                }
                pipe_driver::Event::Read(bytes) => {
                    self.read_waiting = false;
                    if self.cancel.is_cancelled() {
                        continue;
                    }
                    if self.data_input.is_empty() {
                        self.data_deadline = Some(Instant::now() + Duration::from_millis(500));
                    }
                    self.data_input.extend(bytes);
                    if self.data_input.len() > self.data_target {
                        return Err("data read overflow".into());
                    }
                    if self.data_input.len() == self.data_target {
                        if self.data_target == 4 {
                            self.data_target =
                                w::payload_length(&self.data_input).map_err(str::to_owned)? + 4;
                        } else {
                            let raw = std::mem::take(&mut self.data_input);
                            self.data_target = 4;
                            self.data_deadline = None;
                            self.shared
                                .event("data_frame_received", json!({"raw_hex":w::hex(&raw)}));
                            self.data_frame(w::Frame::decode(&raw).map_err(str::to_owned)?)?;
                        }
                    }
                }
                pipe_driver::Event::OwnerObservation(observation) => {
                    self.shared.event("pipe_owner_observation", observation)
                }
                pipe_driver::Event::WriteIssued {
                    id,
                    ordinal,
                    body_end,
                    bytes,
                    pending,
                } => {
                    if let Some(end) = body_end {
                        self.progress.issued_offset = self.progress.issued_offset.max(end);
                    }
                    self.progress.last_write_ordinal = ordinal;
                    self.shared.event("data_write_issued",json!({"id":id,"ordinal":ordinal,"body_end":body_end,"bytes":bytes,"pending":pending}));
                }
                pipe_driver::Event::WriteIncomplete { id, body_end } => self.shared.event(
                    "data_write_incomplete",
                    json!({"id":id,"body_end":body_end}),
                ),
                pipe_driver::Event::WriteCompleted { body_end, bytes } => {
                    let (expected, raw) = self
                        .data_outputs
                        .pop_front()
                        .ok_or("untracked data completion")?;
                    if expected != body_end || bytes != raw.len() {
                        return Err("data completion correlation".into());
                    }
                    if let Some(end) = body_end {
                        self.progress.os_completed_offset =
                            self.progress.os_completed_offset.max(end);
                    }
                    self.shared.event(
                        "data_frame_sent",
                        json!({"raw_hex":w::hex(&raw),"body_end":body_end}),
                    );
                }
                pipe_driver::Event::Reaped {
                    kind,
                    id,
                    bytes,
                    error,
                } => {
                    match kind {
                        PipeKind::Connect => self.progress.connect_reaped = true,
                        PipeKind::Read => {
                            self.progress.read_reaped = true;
                            self.read_waiting = false;
                        }
                        PipeKind::Write => self.progress.write_reaped = true,
                    }
                    self.shared.event(
                        "data_operation_reaped",
                        json!({"kind":format!("{kind:?}"),"id":id,"bytes":bytes,"error":error}),
                    );
                }
                pipe_driver::Event::Error(e) => {
                    self.shared.event("data_error", json!(e));
                    if !self.cancel.is_cancelled() {
                        self.cancel_http(24)?;
                    }
                }
            }
        }
        if let Some(result) = self.pipe.join_if_finished() {
            match result {
                Ok(closed) => {
                    self.pipe_joined = true;
                    self.pipe_join_tick = true;
                    self.progress.data_closed = true;
                    self.progress.connect_reaped = true;
                    self.progress.read_reaped = true;
                    self.progress.write_reaped = true;
                    if let Some(error) = closed.error {
                        self.shared.event("data_worker_error", json!(error));
                        if !self.cancel.is_cancelled() {
                            self.cancel_http(24)?;
                        }
                    }
                    self.shared.event("data_worker_joined", json!(true));
                }
                Err(error) => {
                    self.shared.event("data_worker_unconfirmed", json!(error));
                    return Err("data worker join unconfirmed".into());
                }
            }
        }
        Ok(())
    }
    async fn network_tick(&mut self) -> Result<()> {
        if self.network.as_ref().is_some_and(Network::finished) {
            match self.network.take().unwrap() {
                Network::Opening(task) => {
                    match task.await.map_err(|_| "HTTP opening task panicked")? {
                        Ok(lease) => {
                            let head = lease.head();
                            let head = w::Head {
                                status: head.status,
                                headers: head
                                    .headers
                                    .iter()
                                    .map(|(name, value)| w::Header {
                                        name: name.clone(),
                                        value: value.clone(),
                                    })
                                    .collect(),
                                remote_address: head.remote_addr.to_string(),
                            };
                            self.progress.http_status = head.status;
                            self.head = Some(head.clone());
                            self.lease = Some(lease);
                            if !self.cancel.is_cancelled() {
                                self.progress.network = w::NetworkPhase::Streaming;
                                self.queue(w::Kind::ResponseHead, 0, 0, w::Payload::Head(head))?;
                            }
                        }
                        Err(error) => {
                            self.progress.worker_joined = true;
                            self.shared.event("network_error", json!(error.to_string()));
                            self.cancel_http(network_code(error))?;
                        }
                    }
                }
                Network::Reading(task) => {
                    let (lease, result) = task.await.map_err(|_| "HTTP read task panicked")?;
                    self.lease = Some(lease);
                    match result {
                        Ok(Some(bytes)) if !self.cancel.is_cancelled() => {
                            self.response_body.extend_from_slice(&bytes);
                            self.progress.received_offset = self.response_body.len() as u64;
                            self.body_pending = bytes;
                        }
                        Ok(None) => {
                            self.progress.http_eof = true;
                            self.progress.network = w::NetworkPhase::Eof;
                            let grant = self.grant.as_mut().ok_or("missing HTTP grant")?;
                            grant.observe(
                                self.head.as_ref().ok_or("missing HTTP head")?,
                                &self.response_body,
                            )?;
                            self.progress.intent = w::IntentPhase::Observed;
                            self.progress.response_material_stored = true;
                            self.state(w::Kind::HttpTerminal, 0)?;
                        }
                        Ok(Some(_)) => {}
                        Err(error) => {
                            self.shared.event("network_error", json!(error.to_string()));
                            self.cancel_http(network_code(error))?;
                        }
                    }
                }
                Network::Finishing(task) => {
                    let completed = task.await.map_err(|_| "HTTP finishing task panicked")?;
                    self.progress.worker_joined = completed.worker_joined;
                    self.shared.event("network_worker_joined",json!({"joined":completed.worker_joined,"http_eof":completed.http_eof,"received_bytes":completed.received_bytes,"oneshot_delivered_bytes":completed.delivered_bytes,"peak_upstream_chunk":completed.peak_upstream_chunk}));
                    if !completed.worker_joined {
                        return Err("network worker unconfirmed".into());
                    }
                }
            }
        }
        if self.network.is_none() && (self.cancel.is_cancelled() || self.progress.http_eof) {
            if let Some(lease) = self.lease.take() {
                self.network = Some(Network::Finishing(tokio::spawn(async move {
                    lease.finish().await
                })));
            }
        }
        if !self.cancel.is_cancelled() {
            let cap = self
                .grant
                .as_ref()
                .map(|g| g.decision().response_limit_bytes as u64)
                .unwrap_or(0);
            let end = (self.credit.consumed_offset + self.credit.window_bytes as u64).min(cap);
            if !self.body_pending.is_empty()
                && self.progress.reserved_offset < end
                && self.data_outputs.len() < 2
            {
                let n = self
                    .body_pending
                    .len()
                    .min(self.credit.max_chunk_bytes as usize)
                    .min((end - self.progress.reserved_offset) as usize);
                let next = self.progress.reserved_offset + n as u64;
                let f = self.frame(
                    w::Kind::BodyChunk,
                    self.out_sequence,
                    0,
                    w::Payload::Chunk(w::Chunk {
                        offset: self.progress.reserved_offset,
                        bytes: self.body_pending[..n].to_vec(),
                    }),
                );
                if self.send_data(f, Some(next))? {
                    self.body_pending.drain(..n);
                    self.progress.reserved_offset = next;
                    self.out_sequence += 1;
                }
            }
            let terminal_probe = cap > 0
                && self.progress.received_offset == cap
                && self.progress.peer_consumed_offset == cap
                && self.body_pending.is_empty();
            if self.network.is_none()
                && !self.progress.http_eof
                && self.body_pending.is_empty()
                && self.data_outputs.len() < 2
                && (end > self.progress.reserved_offset || terminal_probe)
            {
                if let Some(mut lease) = self.lease.take() {
                    self.network = Some(Network::Reading(tokio::spawn(async move {
                        let result = lease.next_chunk().await.map(|c| c.map(|b| b.to_vec()));
                        (lease, result)
                    })));
                }
            }
            if self.progress.http_eof
                && self.progress.worker_joined
                && self.body_pending.is_empty()
                && self.progress.peer_consumed_offset == self.progress.received_offset
                && self.progress.os_completed_offset == self.progress.received_offset
                && self.data_outputs.is_empty()
            {
                self.pipe.cancel();
            }
        }
        if self.pipe_joined
            && (!self.progress.worker_started || self.progress.worker_joined)
            && !self.parent.gate.lock().unwrap().network_pending
            && !self.request_close_sent
            && !self.pipe_join_tick
        {
            self.progress.request_closed = true;
            self.state(w::Kind::RequestClosed, 0)?;
            self.request_close_sent = true;
        }
        self.pipe_join_tick = false;
        Ok(())
    }
}
fn network_code(error: morrow_network_node_stream::Error) -> u32 {
    match error {
        morrow_network_node_stream::Error::Limit => 21,
        morrow_network_node_stream::Error::Timeout => 20,
        morrow_network_node_stream::Error::Cancelled => 19,
        _ => 26,
    }
}
fn identity(f: &w::Frame, initial: &w::Frame) -> bool {
    f.same_admission(initial)
        && f.revocation_generation == initial.revocation_generation
        && f.remaining_ms == initial.remaining_ms
        && f.request_budget == initial.request_budget
}

pub(crate) async fn run(
    mut child: Child,
    admission: Admission,
    instance: Instance,
    policy: Arc<Mutex<HostPolicy>>,
    shared: Arc<Shared>,
    mut control: mpsc::Receiver<Control>,
    parent: Parent,
) {
    let spec = &admission.spec;
    let expires = parent.deadline;
    let handshake = (admission.created + Duration::from_millis(spec.handshake_ms)).min(expires);
    let observed = shared.state.lock().unwrap().clone();
    let challenge_sample = Instant::now();
    let initial = w::Frame {
        kind: w::Kind::Challenge,
        sequence: 0,
        session: observed.session,
        instance_epoch: observed.epoch,
        revocation_generation: 1,
        child_pid: observed.pid,
        code: 0,
        remaining_ms: expires
            .saturating_duration_since(challenge_sample)
            .as_millis() as u64,
        nonce: admission.nonce.to_vec(),
        schema_sha256: w::schema_digest().to_vec(),
        artifact_sha256: spec.artifact_sha256.to_vec(),
        execution_config_sha256: admission.config.to_vec(),
        request_budget: spec.request_budget,
        capabilities: 3,
        operation_id: parent.approval.operation.as_bytes().to_vec(),
        attempt: 1,
        payload: w::Payload::None,
    };
    shared.event("challenge_deadline_sample",json!({"sample_offset_ns":challenge_sample.saturating_duration_since(admission.created).as_nanos(),"remaining_ms":initial.remaining_ms,"original_deadline_offset_ns":expires.saturating_duration_since(admission.created).as_nanos()}));
    let mut http = match Http::new(
        initial.clone(),
        parent,
        shared.clone(),
        spec.frame_ms,
        #[cfg(feature = "qualification-pipe-fault")]
        admission.pipe_fault.clone(),
    ) {
        Ok(h) => h,
        Err(e) => {
            shared.event("supervisor_start_unconfirmed", json!(e));
            shared.phase("ClosingUnconfirmed");
            return;
        }
    };
    let initial_bytes = match initial.encode() {
        Ok(b) => b,
        Err(e) => {
            shared.event("challenge_invalid", json!(e));
            shared.phase("ClosingUnconfirmed");
            return;
        }
    };
    http.control.push_back(Output {
        bytes: initial_bytes,
        offset: 0,
        deadline: handshake,
    });
    let mut stdin = child.stdin.take();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut input = Vec::new();
    let mut target = 4;
    let mut input_deadline = None;
    let mut scratch = vec![0; w::MAX_FRAME];
    let mut errbuf = [0; 4096];
    let mut errbytes = 0usize;
    let mut ready = false;
    let mut last_sequence = 0;
    let mut remaining = spec.request_budget;
    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut exited = false;
    let mut control_open = true;
    let mut reason = 0u32;
    let mut close_sequence: Option<u64> = None;
    let mut close_ack_pending = false;
    let mut closing: Option<Instant> = None;
    let mut killed = false;
    let mut unconfirmed = false;
    let mut tick = tokio::time::interval(Duration::from_millis(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if reason != 0 && closing.is_none() {
            shared.event("session_close_reason", json!({"reason":reason}));
            let mut cancellation_proven = true;
            if let Err(e) = http.cancel_http(reason) {
                cancellation_proven = false;
                if close_sequence.is_some() {
                    shared.event(
                        "close_ack_failed",
                        json!({"reason":"cancellation persistence unconfirmed"}),
                    );
                }
                shared.event("cancel_persistence_unconfirmed", json!(e));
                http.cancel.cancel();
                http.pipe.cancel();
                http.parent.gate.lock().unwrap().revoked = true;
            }
            revoke(&policy, instance, &shared, reason);
            shared.phase("Closing");
            http.progress.owner = w::OwnerPhase::Closing;
            closing = Some(Instant::now() + Duration::from_millis(spec.close_ms));
            if let Some(sequence) =
                close_sequence.filter(|_| reason == 25 && cancellation_proven && stdin.is_some())
            {
                // Preserve earlier complete/partial frames. The closing ACK is appended
                // after cancellation state and must itself finish before stdin closes.
                match http.state(w::Kind::State, sequence) {
                    Ok(()) => {
                        close_ack_pending = true;
                        shared.event("close_ack_pending", json!({"sequence":sequence}));
                    }
                    Err(error) => {
                        shared.event(
                            "close_ack_failed",
                            json!({"reason":"enqueue","error":error}),
                        );
                        http.control.clear();
                        stdin = None;
                    }
                }
            } else if http.control.front().is_some_and(|o| o.offset > 0) {
                http.control.clear();
                stdin = None;
                shared.event("partial_control_write_cancelled", json!(true));
            } else {
                http.control.clear();
                if stdin.is_some() {
                    let _ = http.queue(w::Kind::Stop, 0, reason, w::Payload::None);
                }
            }
        }
        if exited
            && stdout_done
            && stderr_done
            && http.pipe_joined
            && (!http.progress.worker_started || http.progress.worker_joined)
            && !http.parent.gate.lock().unwrap().network_pending
        {
            let proven = {
                let s = shared.state.lock().unwrap();
                s.exit_observed && s.stdout_eof && s.stderr_eof
            };
            if proven {
                revoke(&policy, instance, &shared, 24);
                let mut p = policy.lock().unwrap();
                let _ = p.stop(instance);
                let _ = p.retire(instance);
                drop(p);
                http.progress.owner = w::OwnerPhase::Released;
                http.progress.owner_released = true;
                http.progress.child_exited = true;
                http.progress.stdout_eof = true;
                http.progress.stderr_eof = true;
                http.publish();
                shared.state.lock().unwrap().owner_retained = false;
                shared.phase("Released");
                break;
            }
        }
        let can_write = stdin.is_some() && !http.control.is_empty();
        let read_limit = if closing.is_some() {
            w::MAX_FRAME
        } else {
            target - input.len()
        };
        tokio::select! {biased;
            command=control.recv(),if control_open=>match command{
                #[cfg(feature = "qualification-pipe-fault")]
                Some(Control::CloseDataAfterWitness{witness,ack})=>{http.pipe.close_after_witness(witness,ack);},
                Some(Control::ApproveHttp{proposal_ref,expected_hash,response_limit,ack})=>{let result=if ready&&closing.is_none(){http.approve(proposal_ref,expected_hash,response_limit)}else{Err("session not active".into())};http.publish();let _=ack.send(result);},
                Some(Control::Revoke(ack))=>{let result=http.cancel_http(19);if let Err(e)=result{shared.event("revoke_persistence_unconfirmed",json!(e));reason=27;}revoke(&policy,instance,&shared,19);if closing.is_none(){http.progress.owner=w::OwnerPhase::Revoked;shared.phase("Revoked");}shared.event("control_ack",json!({"action":"revoke","generation":2}));let _=ack.send(());},
                Some(Control::Stop(ack))=>{reason=25;http.cancel.cancel();http.pipe.cancel();shared.event("control_ack",json!({"action":"stop"}));let _=ack.send(());},
                None=>{control_open=false;reason=25;},
            },
            _=tick.tick()=>{
                let now=Instant::now();
                if close_ack_pending && (now>=expires || http.control.front().is_some_and(|o|now>=o.deadline) || closing.is_some_and(|d|now>=d)) {
                    close_ack_pending=false;http.control.clear();stdin=None;
                    shared.event("close_ack_failed",json!({"reason":"original/frame/close deadline"}));
                }
                if closing.is_none(){
                    if now>=expires{reason=20;}else if !ready&&now>=handshake{reason=23;}
                    else if input_deadline.is_some_and(|d|now>=d)||http.control.front().is_some_and(|o|now>=o.deadline)||http.data_deadline.is_some_and(|d|now>=d){reason=16;}
                }else if !unconfirmed&&now>=closing.unwrap(){
                    if !killed{killed=true;stdin=None;http.control.clear();let result=child.start_kill();shared.event("kill_requested",json!({"ok":result.is_ok(),"error":result.err().map(|e|e.to_string())}));closing=Some(now+Duration::from_millis(spec.close_ms));}
                    else{unconfirmed=true;shared.phase("ClosingUnconfirmed");http.progress.owner=w::OwnerPhase::ClosingUnconfirmed;shared.event("owner_retained",json!(true));}
                }
                if let Err(e)=http.pipe_events(){shared.event("pipe_driver_error",json!(e));reason=16;}
                // Read actual write-fence state before processing cross-channel ACKs.
                {let g=http.parent.gate.lock().unwrap();http.progress.issued_offset=http.progress.issued_offset.max(g.issued_body_end);http.progress.last_write_ordinal=g.last_write;}
                if let Err(e)=http.network_tick().await{shared.event("network_supervision_error",json!(e));reason=27;}
                if ready&&http.pipe_created&&!http.offered&&closing.is_none(){http.offered=true;if http.queue(w::Kind::DataOffer,0,0,w::Payload::Channel(http.channel.clone())).is_err(){reason=21;}}
                if http.connected&&!http.read_waiting&&!http.pipe_joined&&!http.cancel.is_cancelled()&&closing.is_none(){
                    match http.pipe.send(pipe_driver::Command::Read(http.data_target-http.data_input.len())){Ok(())=>{http.read_waiting=true;http.progress.read_reaped=false;},Err(std::sync::mpsc::TrySendError::Full(_))=>{},Err(_)=>reason=24}
                }
                if http.progress.revoke_applied&&closing.is_none(){http.progress.owner=w::OwnerPhase::Revoked;shared.phase_if_changed("Revoked");let _=policy.lock().unwrap().safety_stop(instance);}
                http.publish();
            },
            result=async{let o=http.control.front().unwrap();stdin.as_mut().unwrap().write(&o.bytes[o.offset..]).await},if can_write=>{
                match result{Ok(0)|Err(_)=>{if close_ack_pending{shared.event("close_ack_failed",json!({"reason":"write incomplete/error"}));close_ack_pending=false;}stdin=None;http.control.clear();reason=24;},Ok(n)=>{let o=http.control.front_mut().unwrap();o.offset+=n;if o.offset==o.bytes.len(){let o=http.control.pop_front().unwrap();let frame=w::Frame::decode(&o.bytes).unwrap();shared.event("control_frame_sent",json!({"raw_hex":w::hex(&o.bytes)}));if close_ack_pending && frame.kind==w::Kind::State && Some(frame.sequence)==close_sequence && frame.code==0 {close_ack_pending=false;shared.event("close_ack_written",json!({"sequence":frame.sequence,"bytes":o.bytes.len()}));stdin=None;http.control.clear();}else if frame.kind==w::Kind::Stop{stdin=None;}}}}
            },
            status=child.wait(),if !exited=>{
                exited=true;match status{Ok(status)=>{let mut s=shared.state.lock().unwrap();s.exit_observed=true;s.exit_code=status.code();drop(s);http.progress.child_exited=true;shared.event("exit",json!({"code":status.code(),"business_success":false}));},Err(error)=>shared.event("wait_unconfirmed",json!(error.to_string()))}reason=if reason==0{24}else{reason};
            },
            result=stdout.read(&mut scratch[..read_limit]),if !stdout_done&&(http.control.len()<8||closing.is_some())=>{
                match result{Ok(0)=>{stdout_done=true;shared.state.lock().unwrap().stdout_eof=true;http.progress.stdout_eof=true;if reason==0{reason=24;}shared.event("stdout_eof",json!(true));},Err(error)=>{stdout_done=true;reason=24;shared.event("stdout_error",json!(error.to_string()));},Ok(n)=>{
                    if closing.is_some(){if close_sequence.is_some(){shared.event("close_ack_failed",json!({"reason":"unexpected guest bytes after Close","bytes":n}));close_ack_pending=false;stdin=None;http.control.clear();}continue;}if input.is_empty(){input_deadline=Some(Instant::now()+Duration::from_millis(spec.frame_ms));}input.extend_from_slice(&scratch[..n]);if input.len()!=target{continue;}
                    if target==4{match w::payload_length(&input){Ok(n)=>{target=n+4;continue;},Err(_)=>{reason=16;continue;}}}
                    shared.event("control_frame_received",json!({"raw_hex":w::hex(&input)}));let decoded=w::Frame::decode(&input);input.clear();target=4;input_deadline=None;
                    let frame=match decoded{Ok(f)=>f,Err(_)=>{reason=16;continue;}};
                    let invalid=if frame.code!=0||!identity(&frame,&initial){17}else if frame.sequence!=last_sequence+1{18}else if !ready&&frame.kind!=w::Kind::Hello{23}else{0};
                    if invalid!=0{let _=http.queue(w::Kind::Denied,frame.sequence,invalid,w::Payload::None);reason=invalid;continue;}
                    last_sequence=frame.sequence;
                    if !ready{if policy.lock().unwrap().ready(instance).is_err(){reason=19;continue;}ready=true;shared.phase("Active");http.progress.owner=w::OwnerPhase::Active;if http.queue(w::Kind::Welcome,last_sequence,0,w::Payload::None).is_err(){reason=21;}continue;}
                    if remaining==0{reason=21;continue;}remaining-=1;
                    let revoked={let g=http.parent.gate.lock().unwrap();http.progress.issued_offset=http.progress.issued_offset.max(g.issued_body_end);http.progress.last_write_ordinal=g.last_write;g.revoked};
                    let cleanup=matches!(frame.kind,w::Kind::Query|w::Kind::HttpCancel|w::Kind::Close)||matches!(&frame.payload,w::Payload::Credit(c)if frame.kind==w::Kind::HttpCredit&&c.window_bytes==0);
                    if revoked&&!cleanup{let _=http.queue(w::Kind::Denied,last_sequence,19,w::Payload::None);continue;}
                    if frame.kind==w::Kind::Close{close_sequence=Some(last_sequence);reason=25;continue;}
                    if let Err(error)=http.handle_control(frame){shared.event("request_denied",json!({"sequence":last_sequence,"error":error}));let _=http.queue(w::Kind::Denied,last_sequence,27,w::Payload::None);reason=27;}
                    http.publish();
                }}
            },
            result=stderr.read(&mut errbuf),if !stderr_done=>match result{
                Ok(0)=>{stderr_done=true;shared.state.lock().unwrap().stderr_eof=true;http.progress.stderr_eof=true;shared.event("stderr_eof",json!({"bytes":errbytes}));},
                Err(error)=>{stderr_done=true;reason=24;shared.event("stderr_error",json!(error.to_string()));},
                Ok(n)=>{errbytes=errbytes.saturating_add(n);if errbytes>65536{reason=21;}},
            },
        }
    }
}
