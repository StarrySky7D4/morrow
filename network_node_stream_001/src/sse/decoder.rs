//! Bounded, strict UTF-8 SSE framing for trusted transport.
//!
//! The field/line rules follow the HTML event-stream interpretation algorithm.
//! Unlike browser EventSource, malformed UTF-8 fails rather than being replaced.
//! EOF never dispatches an unterminated block. `retry` is inert metadata: this
//! decoder creates no connection, replay, checkpoint, credential or authority.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecoderLimits {
    /// Raw line content bytes after the initial BOM, excluding CR/LF.
    pub max_line_bytes: usize,
    /// Raw block bytes including separators and BOM. An LF after CR is counted
    /// only in max_total_bytes, because CR has already terminated that line.
    pub max_event_bytes: usize,
    pub max_total_bytes: usize,
    pub max_events: usize,
    pub max_id_bytes: usize,
    pub max_retry_digits: usize,
}
impl Default for DecoderLimits {
    fn default() -> Self {
        Self {
            max_line_bytes: 8 * 1024,
            max_event_bytes: 64 * 1024,
            max_total_bytes: 4 * 1024 * 1024,
            max_events: 4096,
            max_id_bytes: 1024,
            max_retry_digits: 20,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidLimits,
    LineLimit,
    EventLimit,
    TotalLimit,
    EventCountLimit,
    IdLimit,
    RetryLimit,
    InvalidUtf8,
    Closed,
}
pub type Result<T> = std::result::Result<T, Error>;

// Deliberately no Debug: event payloads and IDs may contain private data.
#[derive(Clone, PartialEq, Eq)]
pub struct Event {
    pub kind: String,
    pub data: String,
    /// Current last-event ID, including an explicit empty-ID reset.
    pub id: String,
    /// Most recent valid retry value. It never causes reconnect or replay.
    pub retry: Option<u64>,
}
pub struct Step {
    pub consumed: usize,
    pub event: Option<Event>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Finish {
    /// EOF left a line or block without its blank-line delimiter. No event was
    /// manufactured from it, including blocks containing only non-data fields.
    pub truncated: bool,
}

pub struct Decoder {
    limits: DecoderLimits,
    total_bytes: usize,
    block_bytes: usize,
    events: usize,
    bom_checked: bool,
    bom_prefix: Vec<u8>,
    pending_lf: bool,
    block_open: bool,
    line: Vec<u8>,
    data: String,
    kind: String,
    id: String,
    retry: Option<u64>,
    fault: Option<Error>,
    finished: Option<Finish>,
}
impl Decoder {
    pub fn new(limits: DecoderLimits) -> Result<Self> {
        if limits.max_line_bytes == 0
            || limits.max_event_bytes == 0
            || limits.max_total_bytes == 0
            || limits.max_events == 0
            || limits.max_id_bytes == 0
            || limits.max_retry_digits == 0
            || limits.max_retry_digits > 20
            || limits.max_id_bytes > limits.max_line_bytes
            || limits.max_line_bytes > limits.max_event_bytes
            || limits.max_event_bytes > limits.max_total_bytes
        {
            return Err(Error::InvalidLimits);
        }
        Ok(Self {
            limits,
            total_bytes: 0,
            block_bytes: 0,
            events: 0,
            bom_checked: false,
            bom_prefix: Vec::new(),
            pending_lf: false,
            block_open: false,
            line: Vec::new(),
            data: String::new(),
            kind: String::new(),
            id: String::new(),
            retry: None,
            fault: None,
            finished: None,
        })
    }

    /// Consumes a prefix of this input and yields at most one event. With no
    /// event, a successful call consumes the entire input. The caller retains
    /// the remainder; there is no event queue or speculative parsing of it.
    /// The first error is fused across both feed and finish.
    pub fn feed(&mut self, input: &[u8]) -> Result<Step> {
        if let Some(error) = self.fault {
            return Err(error);
        }
        if self.finished.is_some() {
            return self.fail(Error::Closed);
        }
        let result = self.feed_inner(input);
        if let Err(error) = result {
            return self.fail(error);
        }
        result
    }
    fn feed_inner(&mut self, input: &[u8]) -> Result<Step> {
        for (index, &byte) in input.iter().enumerate() {
            self.total_bytes = self.total_bytes.checked_add(1).ok_or(Error::TotalLimit)?;
            if self.total_bytes > self.limits.max_total_bytes {
                return Err(Error::TotalLimit);
            }
            if self.pending_lf {
                self.pending_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            self.block_bytes = self.block_bytes.checked_add(1).ok_or(Error::EventLimit)?;
            if self.block_bytes > self.limits.max_event_bytes {
                return Err(Error::EventLimit);
            }
            if let Some(event) = self.accept_byte(byte)? {
                return Ok(Step { consumed: index + 1, event: Some(event) });
            }
        }
        Ok(Step { consumed: input.len(), event: None })
    }
    fn accept_byte(&mut self, byte: u8) -> Result<Option<Event>> {
        const BOM: &[u8] = b"\xEF\xBB\xBF";
        if !self.bom_checked {
            self.bom_prefix.push(byte);
            if BOM.starts_with(&self.bom_prefix) {
                if self.bom_prefix.len() < BOM.len() {
                    return Ok(None);
                }
                self.bom_checked = true;
                self.bom_prefix.clear();
                return Ok(None);
            }
            self.bom_checked = true;
            let prefix = std::mem::take(&mut self.bom_prefix);
            for byte in prefix {
                if let Some(event) = self.line_byte(byte)? {
                    return Ok(Some(event));
                }
            }
            return Ok(None);
        }
        self.line_byte(byte)
    }
    fn line_byte(&mut self, byte: u8) -> Result<Option<Event>> {
        if byte == b'\r' || byte == b'\n' {
            self.pending_lf = byte == b'\r';
            return self.end_line();
        }
        if self.line.len() >= self.limits.max_line_bytes {
            return Err(Error::LineLimit);
        }
        self.line.push(byte);
        self.block_open = true;
        Ok(None)
    }
    fn end_line(&mut self) -> Result<Option<Event>> {
        if self.line.is_empty() {
            self.block_bytes = 0;
            self.block_open = false;
            if self.data.is_empty() {
                // A blank block resets event type even if it dispatches nothing.
                self.kind.clear();
                return Ok(None);
            }
            if self.events >= self.limits.max_events {
                return Err(Error::EventCountLimit);
            }
            self.events += 1;
            self.data.pop(); // Each data field appends exactly one LF.
            let kind = if self.kind.is_empty() {
                "message".to_owned()
            } else {
                std::mem::take(&mut self.kind)
            };
            return Ok(Some(Event {
                kind,
                data: std::mem::take(&mut self.data),
                id: self.id.clone(),
                retry: self.retry,
            }));
        }
        let line = std::str::from_utf8(&self.line).map_err(|_| Error::InvalidUtf8)?;
        if !line.starts_with(':') {
            let (field, mut value) = line.split_once(':').unwrap_or((line, ""));
            if let Some(rest) = value.strip_prefix(' ') {
                value = rest;
            }
            match field {
                "data" => {
                    self.data.push_str(value);
                    self.data.push('\n');
                }
                "event" => {
                    self.kind.clear();
                    self.kind.push_str(value);
                }
                "id" if !value.contains('\0') => {
                    if value.len() > self.limits.max_id_bytes {
                        return Err(Error::IdLimit);
                    }
                    self.id.clear();
                    self.id.push_str(value);
                }
                "retry" if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) => {
                    if value.len() > self.limits.max_retry_digits {
                        return Err(Error::RetryLimit);
                    }
                    self.retry = Some(value.parse().map_err(|_| Error::RetryLimit)?);
                }
                _ => {}
            }
        }
        self.line.clear();
        Ok(None)
    }
    /// Closes this decoder without dispatching a partial block. Strict UTF-8 is
    /// checked even for discarded EOF material. Repeated finish is idempotent.
    pub fn finish(&mut self) -> Result<Finish> {
        if let Some(error) = self.fault {
            return Err(error);
        }
        if let Some(finish) = self.finished {
            return Ok(finish);
        }
        if std::str::from_utf8(&self.bom_prefix).is_err()
            || std::str::from_utf8(&self.line).is_err()
        {
            return self.fail(Error::InvalidUtf8);
        }
        let finish = Finish {
            truncated: self.block_open || !self.line.is_empty() || !self.bom_prefix.is_empty(),
        };
        self.line.clear();
        self.data.clear();
        self.kind.clear();
        self.finished = Some(finish);
        Ok(finish)
    }
    fn fail<T>(&mut self, error: Error) -> Result<T> {
        self.fault = Some(error);
        // Failed input is never retained for a future delivery or diagnostic dump.
        self.line.clear();
        self.bom_prefix.clear();
        self.data.clear();
        self.kind.clear();
        Err(error)
    }
}

#[cfg(test)]
#[path = "decoder_tests.rs"]
mod tests;
