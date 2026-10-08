//! Safe typed process control over an original opaque handle and generation.
//! Discovery reflects provider support; unknown controls never retry or unlock themselves.
#![deny(unsafe_code)]
pub use morrow_agent_process_control_v1 as protocol;
use protocol::{
    Action, Capabilities, MAX_FRAME_BYTES, OutputPage, ReadQuery, Reply, ReplyBody, Request,
};

pub trait Transport {
    /// One bounded canonical exchange on the original host binding. No retries.
    fn exchange_once(&mut self, request: &[u8]) -> std::result::Result<Vec<u8>, ()>;
}
impl<F: FnMut(&[u8]) -> std::result::Result<Vec<u8>, ()>> Transport for F {
    fn exchange_once(&mut self, request: &[u8]) -> std::result::Result<Vec<u8>, ()> {
        self(request)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestIdentity {
    pub request_id: String,
    pub generation: u64,
    pub request_sha256: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Budget,
    Undiscovered,
    Unsupported,
    InputClosed,
    ControlsUnknown,
    Protocol(protocol::Error),
    Rejected(protocol::Error),
    /// Correlation only: no bearer handle or process input is exposed.
    Unknown(RequestIdentity),
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, Copy)]
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
pub struct Client<T> {
    transport: T,
    handle: [u8; 32],
    generation: u64,
    prefix: String,
    sequence: u64,
    declared: Capabilities,
    supported: Option<Capabilities>,
    controls_unknown: bool,
    input_closed: bool,
    remaining: Budget,
}
impl<T: Transport> Client<T> {
    /// The trusted task host supplies a unique invocation prefix; no handle is minted here.
    pub fn new(
        transport: T,
        handle: [u8; 32],
        generation: u64,
        prefix: impl Into<String>,
        declared: Capabilities,
        budget: Budget,
    ) -> Result<Self> {
        let prefix = prefix.into();
        Request::new(
            format!("{prefix}-{}", u64::MAX),
            handle,
            generation,
            Action::Discover,
        )
        .map_err(|_| Error::Invalid)?;
        if prefix.is_empty()
            || budget.calls == 0
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
            handle,
            generation,
            prefix,
            sequence: 0,
            declared,
            supported: None,
            controls_unknown: false,
            input_closed: false,
            remaining: budget,
        })
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn capabilities(&self) -> Option<Capabilities> {
        self.supported
    }
    pub fn controls_unknown(&self) -> bool {
        self.controls_unknown
    }
    pub fn remaining(&self) -> Budget {
        self.remaining
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
    /// Returns the original canonical host reply, including explicit rejection.
    /// Discover must precede controls; neither discovery nor reads unlock an Unknown effect.
    pub fn call(&mut self, action: Action) -> Result<Reply> {
        action.validate().map_err(Error::Protocol)?;
        let mutation = action.is_mutation();
        if mutation && self.controls_unknown {
            return Err(Error::ControlsUnknown);
        }
        if matches!(action, Action::Write(_) | Action::CloseInput) && self.input_closed {
            return Err(Error::InputClosed);
        }
        if !matches!(action, Action::Discover) {
            let supported = self.supported.ok_or(Error::Undiscovered)?;
            if !self.declared.supports(&action) || !supported.supports(&action) {
                return Err(Error::Unsupported);
            }
        }
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Budget)?;
        let request = Request::new(
            format!("{}-{}", self.prefix, self.sequence),
            self.handle,
            self.generation,
            action,
        )
        .map_err(Error::Protocol)?;
        let raw = request.encode().map_err(Error::Protocol)?;
        let identity = RequestIdentity {
            request_id: request.request_id.clone(),
            generation: self.generation,
            request_sha256: protocol::hash(&raw),
        };
        if self.remaining.calls == 0
            || self.remaining.request_bytes < raw.len()
            || self.remaining.reply_bytes < MAX_FRAME_BYTES
        {
            return Err(Error::Budget);
        }
        self.remaining.calls -= 1;
        self.remaining.request_bytes -= raw.len();
        self.remaining.reply_bytes -= MAX_FRAME_BYTES;
        // Latch before crossing the transport boundary. If native transport panics
        // and an outer caller catches its unwind, this same client remains blocked.
        if mutation {
            self.controls_unknown = true;
        }
        let bytes = match self.transport.exchange_once(&raw) {
            Ok(bytes) if !bytes.is_empty() && bytes.len() <= MAX_FRAME_BYTES => bytes,
            _ => {
                if mutation {
                    self.controls_unknown = true;
                }
                return Err(Error::Unknown(identity));
            }
        };
        self.remaining.reply_bytes += MAX_FRAME_BYTES - bytes.len();
        let reply = match Reply::decode_for(&request, &bytes) {
            Ok(reply) => reply,
            Err(_) => {
                if mutation {
                    self.controls_unknown = true;
                }
                return Err(Error::Unknown(identity));
            }
        };
        if mutation {
            match &reply.body {
                ReplyBody::Accepted => self.controls_unknown = false,
                ReplyBody::Rejected(error) if *error != protocol::Error::Unknown => {
                    self.controls_unknown = false;
                }
                _ => {}
            }
        }
        match &reply.body {
            ReplyBody::Capabilities(caps) => self.supported = Some(caps.intersect(self.declared)),
            ReplyBody::Accepted if matches!(request.action, Action::CloseInput) => {
                self.input_closed = true
            }
            _ => {}
        }
        Ok(reply)
    }
    pub fn discover(&mut self) -> Result<Capabilities> {
        let reply = self.call(Action::Discover)?;
        match reply.body {
            ReplyBody::Capabilities(_) => self.supported.ok_or(Error::Undiscovered),
            ReplyBody::Rejected(e) => Err(Error::Rejected(e)),
            _ => Err(Error::Protocol(protocol::Error::Correlation)),
        }
    }
    pub fn read(&mut self, query: ReadQuery) -> Result<OutputPage> {
        self.page(Action::Read(query))
    }
    pub fn events(&mut self, query: ReadQuery) -> Result<OutputPage> {
        self.page(Action::Events(query))
    }
    fn page(&mut self, action: Action) -> Result<OutputPage> {
        match self.call(action)?.body {
            ReplyBody::Page(page) => Ok(page),
            ReplyBody::Rejected(e) => Err(Error::Rejected(e)),
            _ => Err(Error::Protocol(protocol::Error::Correlation)),
        }
    }
    fn control(&mut self, action: Action) -> Result<()> {
        let reply = self.call(action)?;
        match reply.body {
            ReplyBody::Accepted => Ok(()),
            ReplyBody::Rejected(protocol::Error::Unknown) => Err(Error::Unknown(RequestIdentity {
                request_id: reply.request_id,
                generation: reply.generation,
                request_sha256: reply.request_sha256,
            })),
            ReplyBody::Rejected(e) => Err(Error::Rejected(e)),
            _ => Err(Error::Protocol(protocol::Error::Correlation)),
        }
    }
    /// Accepted input/terminate/interrupt never establishes process exit or output EOF.
    pub fn write(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.control(Action::Write(bytes))
    }
    pub fn close_input(&mut self) -> Result<()> {
        self.control(Action::CloseInput)
    }
    pub fn interrupt(&mut self) -> Result<()> {
        self.control(Action::Interrupt)
    }
    pub fn terminate(&mut self) -> Result<()> {
        self.control(Action::Terminate)
    }
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        self.control(Action::Resize { rows, cols })
    }
}
