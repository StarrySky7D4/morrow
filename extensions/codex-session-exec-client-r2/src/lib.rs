//! Safe R2 client orchestration. Local ceilings grant no host authority.
//! Every exchange is attempted once; uncertain effects require explicit host reconciliation.
#![deny(unsafe_code)]

pub use morrow_agent_session_exec_v1_r2 as protocol;
use protocol::{
    Action, Event, ExecutionFacts, Intent, MAX_FRAME_BYTES, Outcome, Reply, Request, SessionInfo,
    SessionSnapshot, StoredEvent, ToolInfo,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestIdentity {
    pub request_id: String,
    pub generation: u64,
    pub request_sha256: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Denied,
    Budget,
    StaleWriter,
    Protocol(protocol::Error),
    Rejected(protocol::Error),
    /// May have committed. Contains correlation only, never input or bearer tokens.
    Unknown(RequestIdentity),
    HistoryChanged,
    Unsealed,
}
pub type Result<T> = std::result::Result<T, Error>;

/// Fixed transport bound to an original host admission. It must not retry a call.
pub trait Transport {
    fn exchange_once(&mut self, canonical_request: &[u8]) -> std::result::Result<Vec<u8>, ()>;
}
impl<F> Transport for F
where
    F: FnMut(&[u8]) -> std::result::Result<Vec<u8>, ()>,
{
    fn exchange_once(&mut self, bytes: &[u8]) -> std::result::Result<Vec<u8>, ()> {
        self(bytes)
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Capabilities {
    pub read: bool,
    pub write: bool,
    pub propose: bool,
    pub execute: bool,
}
/// Guest-side declaration ceiling, never an admission or approval.
pub struct Scope {
    pub capabilities: Capabilities,
    pub sessions: Vec<String>,
    pub operations: Vec<String>,
    pub execution_domain: String,
}
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub calls: u32,
    pub request_bytes: usize,
    pub reply_bytes: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            calls: 32,
            request_bytes: 4 * MAX_FRAME_BYTES,
            reply_bytes: 4 * MAX_FRAME_BYTES,
        }
    }
}
/// A writer fence is invalidated on any failed mutation; reopen explicitly after reconciliation.
#[derive(Debug)]
pub struct Writer {
    session_id: String,
    epoch: u64,
    tail: u64,
    valid: bool,
}
impl Writer {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn tail(&self) -> u64 {
        self.tail
    }
    pub fn is_valid(&self) -> bool {
        self.valid
    }
}
#[derive(Clone, Copy)]
pub struct HistoryLimits {
    pub page_size: u32,
    pub pages: u32,
    pub events: usize,
    pub bytes: usize,
}
impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            page_size: 16,
            pages: 17,
            events: 256,
            bytes: 2 * 1024 * 1024,
        }
    }
}
pub struct History {
    pub info: SessionInfo,
    pub gap: bool,
    /// Apply this checkpoint first only when gap is true, then the returned events.
    pub checkpoint: Vec<u8>,
    pub events: Vec<StoredEvent>,
}
impl std::fmt::Debug for History {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("History")
            .field("info", &self.info)
            .field("gap", &self.gap)
            .field("checkpoint_bytes", &self.checkpoint.len())
            .field("events", &self.events)
            .finish()
    }
}
pub struct Client<T> {
    transport: T,
    generation: u64,
    prefix: String,
    sequence: u64,
    scope: Scope,
    remaining: Budget,
}
fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && !s
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}
impl<T: Transport> Client<T> {
    /// The host supplies a unique nonzero invocation nonce; generation is pinned for life.
    pub fn new(
        transport: T,
        generation: u64,
        nonce: [u8; 16],
        scope: Scope,
        budget: Budget,
    ) -> Result<Self> {
        if generation == 0
            || nonce == [0; 16]
            || !id(&scope.execution_domain)
            || scope.sessions.is_empty()
            || scope.sessions.len() > 16
            || scope.operations.len() > 16
            || scope
                .sessions
                .iter()
                .chain(&scope.operations)
                .any(|s| !id(s))
            || scope.sessions.iter().collect::<BTreeSet<_>>().len() != scope.sessions.len()
            || scope.operations.iter().collect::<BTreeSet<_>>().len() != scope.operations.len()
        {
            return Err(Error::Invalid);
        }
        if budget.calls == 0
            || budget.calls > 128
            || budget.request_bytes == 0
            || budget.reply_bytes == 0
            || budget.request_bytes > 16 * 1024 * 1024
            || budget.reply_bytes > 16 * 1024 * 1024
        {
            return Err(Error::Budget);
        }
        Ok(Self {
            transport,
            generation,
            prefix: nonce.iter().map(|b| format!("{b:02x}")).collect(),
            sequence: 0,
            scope,
            remaining: budget,
        })
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn remaining(&self) -> Budget {
        self.remaining
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
    fn check_scope(&self, action: &Action) -> Result<()> {
        let c = self.scope.capabilities;
        let session = |s: &str| self.scope.sessions.iter().any(|v| v == s);
        let operation = |s: &str| self.scope.operations.iter().any(|v| v == s);
        let allowed = match action {
            Action::List => c.read,
            Action::Snapshot { session_id, .. } => c.read && session(session_id),
            Action::Create {
                session_id, parent, ..
            } => {
                c.write
                    && session(session_id)
                    && parent.as_ref().is_none_or(|p| c.read && session(p))
            }
            Action::OpenWriter { session_id, .. }
            | Action::Append { session_id, .. }
            | Action::Checkpoint { session_id, .. }
            | Action::Archive { session_id, .. } => c.write && session(session_id),
            Action::Propose { session_id, intent } => {
                c.propose
                    && session(session_id)
                    && operation(&intent.operation_id)
                    && intent.execution_domain == self.scope.execution_domain
            }
            Action::Claim { operation_id, .. } | Action::Report { operation_id, .. } => {
                c.execute && operation(operation_id)
            }
            Action::Inspect { operation_id } => c.read && operation(operation_id),
        };
        if allowed { Ok(()) } else { Err(Error::Denied) }
    }
    /// Canonical encoding, one call, full reply correlation; no automatic retries.
    pub fn request(&mut self, action: Action) -> Result<Outcome> {
        self.check_scope(&action)?;
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Budget)?;
        let request_id = format!("codex-{}-{}", self.prefix, self.sequence);
        let request = Request::new_for_generation(request_id.clone(), self.generation, action)
            .map_err(Error::Protocol)?;
        if self.remaining.calls == 0
            || request.raw().len() > self.remaining.request_bytes
            || self.remaining.reply_bytes < MAX_FRAME_BYTES
        {
            return Err(Error::Budget);
        }
        self.remaining.calls -= 1;
        self.remaining.request_bytes -= request.raw().len();
        // Reserve the complete maximum before the host can admit an effect.
        self.remaining.reply_bytes -= MAX_FRAME_BYTES;
        let unknown = || {
            Error::Unknown(RequestIdentity {
                request_id: request_id.clone(),
                generation: self.generation,
                request_sha256: request.digest(),
            })
        };
        let bytes = self
            .transport
            .exchange_once(request.raw())
            .map_err(|_| unknown())?;
        if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
            return Err(unknown());
        }
        self.remaining.reply_bytes += MAX_FRAME_BYTES - bytes.len();
        let reply = Reply::decode_for(&request, &bytes).map_err(|_| unknown())?;
        match reply.outcome {
            Outcome::Rejected(e) => Err(Error::Rejected(e)),
            other => Ok(other),
        }
    }
    pub fn create(&mut self, session_id: &str) -> Result<SessionInfo> {
        session(self.request(Action::Create {
            session_id: session_id.into(),
            parent: None,
            parent_tail: 0,
        })?)
    }
    pub fn list(&mut self) -> Result<Vec<SessionInfo>> {
        match self.request(Action::List)? {
            Outcome::Sessions(v) => {
                if v.iter()
                    .any(|i| !self.scope.sessions.contains(&i.session_id))
                {
                    return Err(Error::Denied);
                }
                Ok(v)
            }
            _ => Err(Error::Protocol(protocol::Error::Correlation)),
        }
    }
    pub fn snapshot(
        &mut self,
        session_id: &str,
        after: u64,
        limit: u32,
    ) -> Result<SessionSnapshot> {
        match self.request(Action::Snapshot {
            session_id: session_id.into(),
            after,
            limit,
        })? {
            Outcome::Snapshot(s) => Ok(s),
            _ => Err(Error::Protocol(protocol::Error::Correlation)),
        }
    }
    pub fn open_writer(&mut self, session_id: &str, expected_epoch: u64) -> Result<Writer> {
        let i = session(self.request(Action::OpenWriter {
            session_id: session_id.into(),
            expected_epoch,
        })?)?;
        Ok(Writer {
            session_id: i.session_id,
            epoch: i.epoch,
            tail: i.tail,
            valid: true,
        })
    }
    fn mutate(&mut self, writer: &mut Writer, action: Action) -> Result<SessionInfo> {
        if !writer.valid {
            return Err(Error::StaleWriter);
        }
        writer.valid = false;
        let info = session(self.request(action)?)?;
        writer.tail = info.tail;
        writer.valid = !info.archived;
        Ok(info)
    }
    pub fn append(&mut self, writer: &mut Writer, events: Vec<Event>) -> Result<SessionInfo> {
        self.mutate(
            writer,
            Action::Append {
                session_id: writer.session_id.clone(),
                epoch: writer.epoch,
                expected_tail: writer.tail,
                events,
            },
        )
    }
    pub fn checkpoint(&mut self, writer: &mut Writer, state: Vec<u8>) -> Result<SessionInfo> {
        self.mutate(
            writer,
            Action::Checkpoint {
                session_id: writer.session_id.clone(),
                epoch: writer.epoch,
                expected_tail: writer.tail,
                state,
            },
        )
    }
    pub fn archive(&mut self, writer: &mut Writer) -> Result<SessionInfo> {
        self.mutate(
            writer,
            Action::Archive {
                session_id: writer.session_id.clone(),
                epoch: writer.epoch,
                expected_tail: writer.tail,
            },
        )
    }
    /// Explicitly continue an exact sealed current tail. Host independently validates CAS.
    pub fn continue_sealed(
        &mut self,
        parent: &SessionSnapshot,
        child_id: &str,
    ) -> Result<SessionInfo> {
        parent.validate().map_err(Error::Protocol)?;
        if !parent.info.checkpoint_sealed || parent.info.checkpoint_tail != parent.info.tail {
            return Err(Error::Unsealed);
        }
        let child = session(self.request(Action::Create {
            session_id: child_id.into(),
            parent: Some(parent.info.session_id.clone()),
            parent_tail: parent.info.tail,
        })?)?;
        if child.parent_checkpoint_sha256 != Some(parent.info.checkpoint_sha256) {
            return Err(Error::Protocol(protocol::Error::Correlation));
        }
        Ok(child)
    }
    /// Fixed revision across pages; a compacted prefix uses its sealed checkpoint explicitly.
    pub fn history(
        &mut self,
        session_id: &str,
        after: u64,
        limits: HistoryLimits,
    ) -> Result<History> {
        if limits.page_size == 0
            || limits.page_size > 16
            || limits.pages == 0
            || limits.pages > 32
            || limits.events > 256
            || limits.bytes > 2 * 1024 * 1024
        {
            return Err(Error::Budget);
        }
        let mut page = self.snapshot(session_id, after, limits.page_size)?;
        let info = page.info.clone();
        let gap = page.gap;
        let checkpoint = page.checkpoint.clone();
        let mut used = checkpoint.len();
        if used > limits.bytes {
            return Err(Error::Budget);
        }
        let mut cursor = if gap { info.checkpoint_tail } else { after };
        let mut events = Vec::new();
        for n in 0..limits.pages {
            if page.info != info || page.checkpoint != checkpoint {
                return Err(Error::HistoryChanged);
            }
            for stored in page.events {
                if stored.sequence <= cursor {
                    continue;
                }
                if stored.sequence != cursor.checked_add(1).ok_or(Error::Budget)? {
                    return Err(Error::Protocol(protocol::Error::Correlation));
                }
                used = used
                    .checked_add(stored.event.body.len())
                    .ok_or(Error::Budget)?;
                if events.len() == limits.events || used > limits.bytes {
                    return Err(Error::Budget);
                }
                cursor = stored.sequence;
                events.push(stored);
            }
            if cursor == info.tail {
                return Ok(History {
                    info,
                    gap,
                    checkpoint,
                    events,
                });
            }
            if n + 1 == limits.pages {
                return Err(Error::Budget);
            }
            page = self.snapshot(session_id, cursor, limits.page_size)?;
        }
        Err(Error::Budget)
    }
    pub fn propose(&mut self, session_id: &str, intent: Intent) -> Result<ToolInfo> {
        tool(self.request(Action::Propose {
            session_id: session_id.into(),
            intent,
        })?)
    }
    pub fn claim(&mut self, operation_id: &str, permit: [u8; 32]) -> Result<Outcome> {
        self.request(Action::Claim {
            operation_id: operation_id.into(),
            permit,
        })
    }
    pub fn report(
        &mut self,
        operation_id: &str,
        claim: [u8; 32],
        facts: ExecutionFacts,
    ) -> Result<ToolInfo> {
        tool(self.request(Action::Report {
            operation_id: operation_id.into(),
            claim,
            facts,
        })?)
    }
    pub fn inspect(&mut self, operation_id: &str) -> Result<ToolInfo> {
        tool(self.request(Action::Inspect {
            operation_id: operation_id.into(),
        })?)
    }
}
fn session(outcome: Outcome) -> Result<SessionInfo> {
    match outcome {
        Outcome::Session(i) => Ok(i),
        _ => Err(Error::Protocol(protocol::Error::Correlation)),
    }
}
fn tool(outcome: Outcome) -> Result<ToolInfo> {
    match outcome {
        Outcome::Tool(i) => Ok(i),
        _ => Err(Error::Protocol(protocol::Error::Correlation)),
    }
}
