//! Typed runtime process control. Host bindings, never wire IDs, convey authority.
#![deny(unsafe_code)]
use sha2::{Digest, Sha256};
#[allow(clippy::all, unsafe_code)]
mod process_control_capnp {
    include!(concat!(env!("OUT_DIR"), "/process_control_capnp.rs"));
}
mod codec;
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
pub mod host;
pub const PROFILE: &str = "agent-process-control-v1";
pub const VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_BODY_BYTES: usize = 32 * 1024;
pub const MAX_EVENTS: usize = 16;
pub const MAX_WAIT_MS: u16 = 1000;
pub const MAX_PROCESSES: usize = 16;
pub const MAX_TOMBSTONES: usize = 128;
pub const MAX_RECEIPTS: usize = 128;
pub const MAX_READ_CALLS: u32 = 128;
pub const MAX_TOTAL_OUTPUT_BYTES: u64 = 1024 * 1024;
pub const MAX_TOTAL_INPUT_BYTES: u64 = 1024 * 1024;
pub const SCHEMA: &[u8] = include_bytes!("../contracts/process_control.capnp");
include!(concat!(env!("OUT_DIR"), "/schema_digest.rs"));
pub const fn schema_digest() -> [u8; 32] {
    SCHEMA_DIGEST
}
pub fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Contract,
    Limit,
    Correlation,
    Denied,
    Conflict,
    NotFound,
    Unknown,
    Unsupported,
    Closed,
}
pub type Result<T> = std::result::Result<T, Error>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "process control {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub read: bool,
    pub events: bool,
    pub write: bool,
    pub close_input: bool,
    pub interrupt: bool,
    pub terminate: bool,
    pub resize_pty: bool,
}
impl Capabilities {
    pub fn intersect(self, approved: Self) -> Self {
        Self {
            read: self.read && approved.read,
            events: self.events && approved.events,
            write: self.write && approved.write,
            close_input: self.close_input && approved.close_input,
            interrupt: self.interrupt && approved.interrupt,
            terminate: self.terminate && approved.terminate,
            resize_pty: self.resize_pty && approved.resize_pty,
        }
    }
    pub fn supports(self, action: &Action) -> bool {
        match action {
            Action::Discover => true,
            Action::Read(_) => self.read,
            Action::Events(_) => self.events,
            Action::Write(_) => self.write,
            Action::CloseInput => self.close_input,
            Action::Interrupt => self.interrupt,
            Action::Terminate => self.terminate,
            Action::Resize { .. } => self.resize_pty,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadQuery {
    pub after_seq: u64,
    pub max_bytes: u32,
    pub max_events: u16,
    pub wait_ms: u16,
}
impl ReadQuery {
    pub fn validate(self) -> Result<()> {
        if self.after_seq == u64::MAX
            || self.max_bytes == 0
            || self.max_bytes as usize > MAX_BODY_BYTES
            || self.max_events == 0
            || self.max_events as usize > MAX_EVENTS
            || self.wait_ms > MAX_WAIT_MS
        {
            Err(Error::Limit)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Discover,
    Read(ReadQuery),
    Events(ReadQuery),
    Write(Vec<u8>),
    CloseInput,
    Interrupt,
    Terminate,
    Resize { rows: u16, cols: u16 },
}
impl Action {
    pub fn is_mutation(&self) -> bool {
        !matches!(self, Self::Discover | Self::Read(_) | Self::Events(_))
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Read(q) | Self::Events(q) => q.validate(),
            Self::Write(v) if v.is_empty() || v.len() > MAX_BODY_BYTES => Err(Error::Limit),
            Self::Resize { rows, cols }
                if *rows == 0 || *cols == 0 || *rows > 4096 || *cols > 4096 =>
            {
                Err(Error::Limit)
            }
            _ => Ok(()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub request_id: String,
    pub handle: [u8; 32],
    pub generation: u64,
    pub action: Action,
}
impl Request {
    pub fn new(
        request_id: impl Into<String>,
        handle: [u8; 32],
        generation: u64,
        action: Action,
    ) -> Result<Self> {
        let value = Self {
            request_id: request_id.into(),
            handle,
            generation,
            action,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        valid_id(&self.request_id)?;
        if self.handle == [0; 32] || self.generation == 0 {
            return Err(Error::Invalid);
        }
        self.action.validate()
    }
    pub fn digest(&self) -> Result<[u8; 32]> {
        Ok(hash(&self.encode()?))
    }
}
pub(crate) fn valid_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || b"._:-".contains(&v))
    {
        Err(Error::Invalid)
    } else {
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
    Pty,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Output {
        stream: OutputStream,
        chunk: Vec<u8>,
    },
    Exited {
        exit_code: i32,
        sandbox_denied: Option<bool>,
    },
    Closed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEvent {
    pub seq: u64,
    pub kind: EventKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputPage {
    pub events: Vec<ProcessEvent>,
    pub next_seq: u64,
    pub floor_seq: u64,
    pub gap: bool,
    pub exited: bool,
    pub exit_code: Option<i32>,
    pub closed: bool,
    pub failure: Option<String>,
}
impl OutputPage {
    pub fn validate(&self) -> Result<()> {
        if self.events.len() > MAX_EVENTS
            || self.floor_seq == 0
            || self.exit_code.is_some() && !self.exited
            || self
                .failure
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 4096)
        {
            return Err(Error::Invalid);
        }
        let mut seq = 0;
        let mut bytes = 0usize;
        let mut saw_exit = false;
        let mut saw_closed = false;
        for event in &self.events {
            if event.seq <= seq || event.seq < self.floor_seq || saw_closed {
                return Err(Error::Invalid);
            }
            seq = event.seq;
            match &event.kind {
                EventKind::Output { chunk, .. } => {
                    if chunk.is_empty() {
                        return Err(Error::Invalid);
                    }
                    bytes = bytes.checked_add(chunk.len()).ok_or(Error::Limit)?;
                }
                EventKind::Exited { exit_code, .. } => {
                    if !self.exited || self.exit_code != Some(*exit_code) || saw_exit {
                        return Err(Error::Invalid);
                    }
                    saw_exit = true;
                }
                EventKind::Closed => {
                    if !self.closed {
                        return Err(Error::Invalid);
                    }
                    saw_closed = true;
                }
            }
        }
        if bytes > MAX_BODY_BYTES || self.next_seq < seq {
            return Err(Error::Limit);
        }
        Ok(())
    }
    pub fn validate_for(&self, query: ReadQuery) -> Result<()> {
        self.validate()?;
        if self.events.len() > query.max_events as usize
            || self
                .events
                .first()
                .is_some_and(|e| e.seq <= query.after_seq)
            || self
                .events
                .iter()
                .map(|e| match &e.kind {
                    EventKind::Output { chunk, .. } => chunk.len(),
                    _ => 0,
                })
                .sum::<usize>()
                > query.max_bytes as usize
            || self.next_seq < query.after_seq
            || self.gap != (query.after_seq.saturating_add(1) < self.floor_seq)
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyBody {
    Capabilities(Capabilities),
    Page(OutputPage),
    Accepted,
    Rejected(Error),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub request_id: String,
    pub request_sha256: [u8; 32],
    pub generation: u64,
    pub body: ReplyBody,
}
impl Reply {
    pub fn new(request: &Request, body: ReplyBody) -> Result<Self> {
        if let ReplyBody::Page(page) = &body {
            page.validate()?
        }
        Ok(Self {
            request_id: request.request_id.clone(),
            request_sha256: request.digest()?,
            generation: request.generation,
            body,
        })
    }
}
