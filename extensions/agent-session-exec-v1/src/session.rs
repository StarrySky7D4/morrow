//! Durable generic session history in the original Core Store. Model data is opaque.
use crate::authority::{Admission, Right, SessionExecHost, core_error};
use crate::{
    Action, Error, Event, Outcome, Reply, Request, Result, SessionInfo, SessionSnapshot,
    StoredEvent, hash,
};
use morrow_core::{
    agent_ledger::Record,
    dispatch::{Connection, HostRuntime},
};
use prost::Message;

pub const MAX_SESSION_EVENTS: usize = 256;
pub const MAX_SESSION_RECEIPTS: usize = 128;
pub const MAX_SESSION_STATE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, PartialEq, Message)]
struct State {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(string, optional, tag = "2")]
    parent: Option<String>,
    #[prost(uint64, tag = "3")]
    parent_tail: u64,
    #[prost(uint64, tag = "4")]
    epoch: u64,
    #[prost(uint64, tag = "5")]
    tail: u64,
    #[prost(uint64, tag = "6")]
    floor: u64,
    #[prost(bool, tag = "7")]
    archived: bool,
    #[prost(uint64, tag = "8")]
    revision: u64,
    #[prost(uint64, tag = "9")]
    checkpoint_tail: u64,
    #[prost(bytes = "vec", tag = "10")]
    checkpoint: Vec<u8>,
    #[prost(bytes = "vec", tag = "11")]
    writer: Vec<u8>,
    #[prost(bytes = "vec", tag = "12")]
    issuer: Vec<u8>,
    #[prost(message, repeated, tag = "13")]
    events: Vec<PEvent>,
    #[prost(message, repeated, tag = "14")]
    receipts: Vec<PReceipt>,
    #[prost(message, repeated, tag = "15")]
    identities: Vec<PIdentity>,
    #[prost(bool, tag = "16")]
    checkpoint_sealed: bool,
    #[prost(bytes = "vec", tag = "17")]
    parent_checkpoint: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct PEvent {
    #[prost(uint64, tag = "1")]
    sequence: u64,
    #[prost(string, tag = "2")]
    id: String,
    #[prost(bytes = "vec", tag = "3")]
    body: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct PReceipt {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(bytes = "vec", tag = "2")]
    request: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    reply: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    admission: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct PIdentity {
    #[prost(string, tag = "1")]
    id: String,
    #[prost(bytes = "vec", tag = "2")]
    digest: Vec<u8>,
}
fn domain() -> [u8; 32] {
    hash(b"morrow/agent-session-exec-v1/session/state/1")
}
impl State {
    fn info(&self) -> SessionInfo {
        SessionInfo {
            session_id: self.id.clone(),
            parent: self.parent.clone(),
            parent_tail: self.parent_tail,
            epoch: self.epoch,
            tail: self.tail,
            floor: self.floor,
            archived: self.archived,
            revision: self.revision,
            checkpoint_tail: self.checkpoint_tail,
            checkpoint_sha256: hash(&self.checkpoint),
            checkpoint_sealed: self.checkpoint_sealed,
            parent_checkpoint_sha256: self.parent.as_ref().map(|_| {
                self.parent_checkpoint
                    .as_slice()
                    .try_into()
                    .expect("validated parent checkpoint")
            }),
        }
    }
    fn validate(&self) -> Result<()> {
        if (self.parent.is_some() && self.parent_checkpoint.len() != 32)
            || (self.parent.is_none() && !self.parent_checkpoint.is_empty())
        {
            return Err(Error::Invalid);
        }
        self.info().validate()?;
        if self.checkpoint.len() > crate::MAX_BODY_BYTES
            || self.events.len() > MAX_SESSION_EVENTS
            || self.identities.len() > MAX_SESSION_EVENTS
            || self.receipts.len() > MAX_SESSION_RECEIPTS
            || self.encoded_len() > MAX_SESSION_STATE_BYTES
        {
            return Err(Error::Limit);
        }
        if (self.epoch == 0 && (!self.writer.is_empty() || !self.issuer.is_empty()))
            || (self.epoch != 0 && (self.writer.len() != 32 || self.issuer.len() != 32))
        {
            return Err(Error::Invalid);
        }
        if (!self.checkpoint_sealed && (self.checkpoint_tail != 0 || !self.checkpoint.is_empty()))
            || (self.parent.is_some() && self.parent_checkpoint.len() != 32)
            || (self.parent.is_none() && !self.parent_checkpoint.is_empty())
            || self.floor > 1
                && (!self.checkpoint_sealed
                    || self
                        .checkpoint_tail
                        .checked_add(1)
                        .is_none_or(|v| v < self.floor))
        {
            return Err(Error::Invalid);
        }
        let mut ids = std::collections::BTreeMap::new();
        for identity in &self.identities {
            crate::identity(&identity.id)?;
            if identity.digest.len() != 32 || ids.insert(&identity.id, &identity.digest).is_some() {
                return Err(Error::Invalid);
            }
        }
        if self.identities.len() as u64 != self.tail {
            return Err(Error::Invalid);
        }
        if self.events.len() as u64 != self.tail + 1 - self.floor {
            return Err(Error::Invalid);
        }
        for (index, event) in self.events.iter().enumerate() {
            let dto = Event {
                event_id: event.id.clone(),
                body: event.body.clone(),
            };
            dto.validate()?;
            if event.sequence != self.floor + index as u64
                || ids.get(&event.id).map(|v| v.as_slice()) != Some(event_digest(&dto).as_slice())
            {
                return Err(Error::Invalid);
            }
        }
        let mut requests = std::collections::BTreeSet::new();
        for receipt in &self.receipts {
            if receipt.admission.len() != 32 || !requests.insert(&receipt.id) {
                return Err(Error::Invalid);
            }
            let req = Request::decode(&receipt.request)?;
            if req.id() != receipt.id {
                return Err(Error::Invalid);
            }
            Reply::decode_for(&req, &receipt.reply)?;
        }
        Ok(())
    }
}
fn event_digest(event: &Event) -> [u8; 32] {
    let mut bytes = (event.event_id.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(event.event_id.as_bytes());
    bytes.extend_from_slice(&event.body);
    hash(&bytes)
}
fn load(runtime: &HostRuntime, id: &str) -> Result<State> {
    let row = runtime
        .store_local()
        .load_agent_ledger_local(&domain(), id)
        .map_err(core_error)?
        .ok_or(Error::NotFound)?;
    let state = State::decode(row.payload()).map_err(|_| Error::Storage)?;
    if state.encode_to_vec() != row.payload() || state.id != id || state.revision != row.revision()
    {
        return Err(Error::Storage);
    }
    state.validate()?;
    Ok(state)
}
pub(crate) fn require_active(runtime: &HostRuntime, id: &str) -> Result<u64> {
    let state = load(runtime, id)?;
    if state.archived {
        return Err(Error::Denied);
    }
    Ok(state.epoch)
}
impl SessionExecHost {
    /// Every response uses the original raw request identity. An uncertain durable
    /// acknowledgment is never converted into a fresh writer or execution attempt.
    pub fn dispatch(
        &self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        admission: &Admission,
        bytes: &[u8],
        mut clock: impl FnMut() -> u64,
    ) -> Result<Vec<u8>> {
        let _fence = self.execution_fence()?;
        let request = Request::decode(bytes)?;
        let outcome = match request.action() {
            Action::Propose { .. }
            | Action::Claim { .. }
            | Action::Report { .. }
            | Action::Inspect { .. } => {
                self.safe_dispatch(runtime, connection, admission, &request, &mut clock)
            }
            _ => self.session_dispatch(runtime, connection, admission, &request, &mut clock),
        }
        .unwrap_or_else(Outcome::Rejected);
        let success = !matches!(outcome, Outcome::Rejected(_));
        let mut encoded = match Reply::new(&request, outcome)?.encode() {
            Ok(bytes) => bytes,
            Err(Error::Limit) => {
                return Reply::new(&request, Outcome::Rejected(Error::Limit))?.encode();
            }
            Err(error) => return Err(error),
        };
        if success {
            let final_check = match request.action() {
                Action::Propose { .. }
                | Action::Claim { .. }
                | Action::Report { .. }
                | Action::Inspect { .. } => {
                    self.finalize_safe(runtime, connection, admission, &request, &mut clock)
                }
                Action::List => self.check(
                    runtime,
                    connection,
                    admission,
                    Right::SessionRead,
                    None,
                    clock(),
                ),
                Action::Snapshot { session_id, .. } => self.check(
                    runtime,
                    connection,
                    admission,
                    Right::SessionRead,
                    Some(session_id),
                    clock(),
                ),
                Action::Create { session_id, .. }
                | Action::OpenWriter { session_id, .. }
                | Action::Append { session_id, .. }
                | Action::Checkpoint { session_id, .. }
                | Action::Archive { session_id, .. } => self
                    .check(
                        runtime,
                        connection,
                        admission,
                        Right::SessionWrite,
                        Some(session_id),
                        clock(),
                    )
                    .map_err(|_| Error::CommitUnknown),
            };
            if let Err(error) = final_check {
                encoded = Reply::new(&request, Outcome::Rejected(error))?.encode()?;
            }
        }
        Ok(encoded)
    }
    fn session_dispatch(
        &self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        admission: &Admission,
        request: &Request,
        clock: &mut impl FnMut() -> u64,
    ) -> Result<Outcome> {
        let (right, id) = match request.action() {
            Action::List => (Right::SessionRead, None),
            Action::Snapshot { session_id, .. } => (Right::SessionRead, Some(session_id.as_str())),
            Action::Create { session_id, .. }
            | Action::OpenWriter { session_id, .. }
            | Action::Append { session_id, .. }
            | Action::Checkpoint { session_id, .. }
            | Action::Archive { session_id, .. } => {
                (Right::SessionWrite, Some(session_id.as_str()))
            }
            _ => return Err(Error::Invalid),
        };
        self.check(runtime, connection, admission, right, id, clock())?;
        let outcome = match request.action() {
            Action::List => {
                let mut infos = Vec::new();
                for id in admission.sessions() {
                    match load(runtime, id) {
                        Ok(state) => infos.push(state.info()),
                        Err(Error::NotFound) => {}
                        Err(e) => return Err(e),
                    }
                }
                Outcome::Sessions(infos)
            }
            Action::Snapshot {
                session_id,
                after,
                limit,
            } => {
                let state = load(runtime, session_id)?;
                if *after > state.tail {
                    return Err(Error::Conflict);
                }
                let gap = after + 1 < state.floor;
                let events = state
                    .events
                    .iter()
                    .filter(|e| e.sequence > *after)
                    .take(*limit as usize)
                    .map(|e| StoredEvent {
                        sequence: e.sequence,
                        event: Event {
                            event_id: e.id.clone(),
                            body: e.body.clone(),
                        },
                    })
                    .collect();
                Outcome::Snapshot(SessionSnapshot {
                    info: state.info(),
                    events,
                    gap,
                    checkpoint: state.checkpoint.clone(),
                })
            }
            action => {
                let id = id.expect("session action");
                let previous = match load(runtime, id) {
                    Ok(s) => Some(s),
                    Err(Error::NotFound) => None,
                    Err(e) => return Err(e),
                };
                if let Some(state) = &previous
                    && let Some(receipt) = state.receipts.iter().find(|r| r.id == request.id())
                {
                    if receipt.request != request.raw() || receipt.admission != admission.nonce() {
                        return Err(Error::Conflict);
                    }
                    let outcome = Reply::decode_for(request, &receipt.reply)?.outcome;
                    self.check(runtime, connection, admission, right, Some(id), clock())?;
                    return Ok(outcome);
                }
                let expected = previous.as_ref().map_or(0, |s| s.revision);
                let mut state = match action {
                    Action::Create {
                        session_id,
                        parent,
                        parent_tail,
                    } => {
                        if previous.is_some() {
                            return Err(Error::Conflict);
                        }
                        let mut parent_checkpoint = Vec::new();
                        let mut inherited = Vec::new();
                        if let Some(parent) = parent {
                            self.check(
                                runtime,
                                connection,
                                admission,
                                Right::SessionRead,
                                Some(parent),
                                clock(),
                            )?;
                            let source = load(runtime, parent)?;
                            if !source.checkpoint_sealed
                                || source.checkpoint_tail != *parent_tail
                                || source.tail != *parent_tail
                            {
                                return Err(Error::Conflict);
                            }
                            parent_checkpoint = hash(&source.checkpoint).to_vec();
                            inherited = source.checkpoint;
                        }
                        State {
                            id: session_id.clone(),
                            parent: parent.clone(),
                            parent_tail: *parent_tail,
                            epoch: 0,
                            tail: 0,
                            floor: 1,
                            archived: false,
                            revision: 0,
                            checkpoint_tail: 0,
                            checkpoint: inherited,
                            writer: Vec::new(),
                            issuer: Vec::new(),
                            events: Vec::new(),
                            receipts: Vec::new(),
                            identities: Vec::new(),
                            checkpoint_sealed: parent.is_some(),
                            parent_checkpoint,
                        }
                    }
                    _ => previous.ok_or(Error::NotFound)?,
                };
                if state.archived {
                    return Err(Error::Denied);
                }
                match action {
                    Action::Create { .. } => {}
                    Action::OpenWriter { expected_epoch, .. } => {
                        if *expected_epoch != state.epoch {
                            return Err(Error::Conflict);
                        }
                        state.epoch = state.epoch.checked_add(1).ok_or(Error::Limit)?;
                        state.writer = admission.nonce().to_vec();
                        state.issuer = self.issuer_nonce().to_vec();
                    }
                    Action::Append {
                        epoch,
                        expected_tail,
                        events,
                        ..
                    } => {
                        self.writer(&state, admission, *epoch, *expected_tail)?;
                        if state.identities.len() + events.len() > MAX_SESSION_EVENTS {
                            return Err(Error::Limit);
                        }
                        for event in events {
                            if state.identities.iter().any(|e| e.id == event.event_id) {
                                return Err(Error::Conflict);
                            }
                            state.tail = state.tail.checked_add(1).ok_or(Error::Limit)?;
                            state.identities.push(PIdentity {
                                id: event.event_id.clone(),
                                digest: event_digest(event).to_vec(),
                            });
                            state.events.push(PEvent {
                                sequence: state.tail,
                                id: event.event_id.clone(),
                                body: event.body.clone(),
                            });
                        }
                    }
                    Action::Checkpoint {
                        epoch,
                        expected_tail,
                        state: checkpoint,
                        ..
                    } => {
                        self.writer(&state, admission, *epoch, *expected_tail)?;
                        if state.checkpoint_sealed
                            && state.checkpoint_tail == state.tail
                            && state.checkpoint != *checkpoint
                        {
                            return Err(Error::Conflict);
                        }
                        state.checkpoint = checkpoint.clone();
                        state.checkpoint_tail = state.tail;
                        state.checkpoint_sealed = true;
                    }
                    Action::Archive {
                        epoch,
                        expected_tail,
                        ..
                    } => {
                        self.writer(&state, admission, *epoch, *expected_tail)?;
                        state.archived = true;
                    }
                    _ => return Err(Error::Invalid),
                }
                if state.receipts.len() >= MAX_SESSION_RECEIPTS {
                    return Err(Error::Limit);
                }
                state.revision = expected.checked_add(1).ok_or(Error::Limit)?;
                let outcome = Outcome::Session(state.info());
                let reply = Reply::new(request, outcome.clone())?.encode()?;
                state.receipts.push(PReceipt {
                    id: request.id().into(),
                    request: request.raw().to_vec(),
                    reply,
                    admission: admission.nonce().to_vec(),
                });
                state.validate()?;
                self.check(runtime, connection, admission, right, Some(id), clock())?;
                let row = Record::new(domain(), id, state.revision, state.encode_to_vec())
                    .map_err(core_error)?;
                runtime
                    .store_local_mut()
                    .compare_exchange_agent_ledger_local(&row, expected)
                    .map_err(core_error)?;
                outcome
            }
        };
        Ok(outcome)
    }
    fn writer(&self, state: &State, admission: &Admission, epoch: u64, tail: u64) -> Result<()> {
        if state.epoch != epoch || state.tail != tail {
            return Err(Error::Conflict);
        }
        if state.writer != admission.nonce() || state.issuer != self.issuer_nonce() {
            return Err(Error::Denied);
        }
        Ok(())
    }
    /// Trusted retention only. A sealed checkpoint at the exact current tail is
    /// required; event identities and original mutation receipts remain retained.
    pub fn compact(
        &self,
        runtime: &mut HostRuntime,
        connection: &Connection,
        admission: &Admission,
        session: &str,
        checkpoint_tail: u64,
        now: u64,
    ) -> Result<SessionInfo> {
        let _fence = self.execution_fence()?;
        self.check(
            runtime,
            connection,
            admission,
            Right::SessionWrite,
            Some(session),
            now,
        )?;
        let mut state = load(runtime, session)?;
        self.writer(&state, admission, state.epoch, state.tail)?;
        if !state.checkpoint_sealed
            || state.checkpoint_tail != checkpoint_tail
            || state.tail != checkpoint_tail
        {
            return Err(Error::Conflict);
        }
        let expected = state.revision;
        state.floor = checkpoint_tail.checked_add(1).ok_or(Error::Limit)?;
        state.events.clear();
        state.revision += 1;
        state.validate()?;
        let row = Record::new(domain(), session, state.revision, state.encode_to_vec())
            .map_err(core_error)?;
        runtime
            .store_local_mut()
            .compare_exchange_agent_ledger_local(&row, expected)
            .map_err(core_error)?;
        Ok(state.info())
    }
}
