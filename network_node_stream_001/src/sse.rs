//! Bounded SSE framing over the original pull-driven HTTP lease.
//! HTTP EOF is a transport fact, not provider/business completion.
pub mod decoder;
pub use decoder::{DecoderLimits, Event};
use crate::{
    Error as TransportError,
    stream::{Completion as TransportCompletion, MAX_DELIVERY_CHUNK, ResponseHead, StreamLease},
};
use bytes::Bytes;
use decoder::{Decoder, Error as DecoderError, Finish};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Transport(TransportError),
    Decoder(DecoderError),
    HttpStatus(u16),
    InvalidMime,
    InvalidEncoding,
    DecoderProgress,
}
pub type Result<T> = std::result::Result<T, Error>;

/// A failed opening owns a real cleanup receipt, with the header/limit cause
/// separate from the cancellation used to stop the transport.
#[derive(Debug)]
pub struct OpenError {
    pub cause: Error,
    pub transport: TransportCompletion,
}
/// No provider-specific terminal convention is interpreted here.
#[derive(Debug)]
pub struct Completion {
    pub outcome: Result<()>,
    pub transport: TransportCompletion,
    pub decoder: std::result::Result<Finish, DecoderError>,
    /// Actual events delivered after a fresh original-authority check.
    pub delivered_events: usize,
    pub buffered_bytes: usize,
    pub decoder_reached_http_eof: bool,
    /// Includes interrupted input and unconsumed chunk bytes as well as an EOF block.
    pub truncated: bool,
}

/// Holds at most one original <=8KiB delivery chunk and the bounded decoder.
/// No event queue is built. Dropping a next_event future preserves all state.
pub struct SseLease {
    stream: StreamLease,
    decoder: Decoder,
    remainder: Bytes,
    delivered_events: usize,
    decoder_reached_http_eof: bool,
    first_error: Option<Error>,
}
impl SseLease {
    pub async fn new(mut stream: StreamLease, limits: DecoderLimits) -> std::result::Result<Self, OpenError> {
        let result = stream
            .check_delivery()
            .map_err(Error::Transport)
            .and_then(|()| validate_head(stream.head()))
            .and_then(|()| Decoder::new(limits).map_err(Error::Decoder));
        match result {
            Ok(decoder) => Ok(Self {
                stream,
                decoder,
                remainder: Bytes::new(),
                delivered_events: 0,
                decoder_reached_http_eof: false,
                first_error: None,
            }),
            Err(cause) => Err(OpenError {
                cause,
                transport: stream.cancel_and_wait().await,
            }),
        }
    }
    pub fn head(&self) -> &ResponseHead {
        self.stream.head()
    }
    pub fn deadline(&self) -> tokio::time::Instant {
        self.stream.deadline()
    }
    /// An unjoined transport observation; use finish/cancel_and_wait for a receipt.
    pub fn terminal(&self) -> Option<TransportCompletion> {
        self.stream.terminal()
    }
    pub fn buffered_bytes(&self) -> usize {
        self.remainder.len()
    }
    pub fn delivered_events(&self) -> usize {
        self.delivered_events
    }
    pub fn cancel(&self) {
        self.stream.cancel();
    }
    fn fail<T>(&mut self, error: Error) -> Result<T> {
        let first = *self.first_error.get_or_insert(error);
        self.stream.cancel();
        Err(first)
    }
    fn check_delivery(&mut self) -> Result<()> {
        if let Some(error) = self.first_error {
            return Err(error);
        }
        if let Err(error) = self.stream.check_delivery() {
            return self.fail(Error::Transport(error));
        }
        Ok(())
    }
    /// At most one event per call. Parsing and remainder updates contain no await:
    /// future cancellation can occur only at the original cancellation-safe pull.
    pub async fn next_event(&mut self) -> Result<Option<Event>> {
        loop {
            // Required even when bytes came from an earlier authorized chunk.
            self.check_delivery()?;
            if self.decoder_reached_http_eof {
                return Ok(None);
            }
            if !self.remainder.is_empty() {
                let step = match self.decoder.feed(&self.remainder) {
                    Ok(step) => step,
                    Err(error) => return self.fail(Error::Decoder(error)),
                };
                if step.consumed == 0 || step.consumed > self.remainder.len() {
                    return self.fail(Error::DecoderProgress);
                }
                self.remainder = self.remainder.slice(step.consumed..);
                if let Some(event) = step.event {
                    self.check_delivery()?;
                    self.delivered_events += 1;
                    return Ok(Some(event));
                }
                continue;
            }
            match self.stream.next_chunk().await {
                Ok(Some(bytes)) if bytes.len() <= MAX_DELIVERY_CHUNK => {
                    self.remainder = bytes;
                }
                Ok(Some(_)) => return self.fail(Error::Transport(TransportError::Limit)),
                Ok(None) => {
                    self.decoder_reached_http_eof = true;
                    if let Err(error) = self.decoder.finish() {
                        return self.fail(Error::Decoder(error));
                    }
                    self.check_delivery()?;
                    return Ok(None);
                }
                Err(error) => return self.fail(Error::Transport(error)),
            }
        }
    }
    /// Never drains, dispatches buffered events or interprets [DONE]. An unfinished
    /// original HTTP worker is cancelled and actually joined by StreamLease.
    pub async fn finish(mut self) -> Completion {
        let decoder = self.decoder.finish();
        // Finishing may discover a parser fault (for example a partial UTF-8
        // scalar) before cleanup cancels HTTP. Preserve it unless an earlier
        // observed parser/delivery fault already owns the first cause.
        if let Err(error) = &decoder {
            self.first_error.get_or_insert(Error::Decoder(*error));
            self.stream.cancel();
        }
        let buffered_bytes = self.remainder.len();
        let truncated = !self.decoder_reached_http_eof
            || buffered_bytes != 0
            || decoder.is_err()
            || decoder.as_ref().is_ok_and(|finish| finish.truncated);
        let first_error = self.first_error;
        let transport = self.stream.finish().await;
        let outcome = match first_error {
            Some(error) => Err(error),
            None => match transport.outcome {
                Err(error) => Err(Error::Transport(error)),
                Ok(()) => decoder.as_ref().map(|_| ()).map_err(|error| Error::Decoder(*error)),
            },
        };
        Completion {
            outcome,
            transport,
            decoder,
            delivered_events: self.delivered_events,
            buffered_bytes,
            decoder_reached_http_eof: self.decoder_reached_http_eof,
            truncated,
        }
    }
    pub async fn cancel_and_wait(self) -> Completion {
        self.stream.cancel();
        self.finish().await
    }
}
fn validate_head(head: &ResponseHead) -> Result<()> {
    if head.status != 200 {
        return Err(Error::HttpStatus(head.status));
    }
    let mut encodings = head.headers.iter().filter(|(name, _)| name.eq_ignore_ascii_case("content-encoding"));
    if let Some((_, bytes)) = encodings.next() {
        if encodings.next().is_some() || bytes.len() > 64 || !bytes.is_ascii()
            || !std::str::from_utf8(bytes).map_err(|_| Error::InvalidEncoding)?.trim().eq_ignore_ascii_case("identity")
        {
            return Err(Error::InvalidEncoding);
        }
    }
    let mut values = head
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-type"));
    let (_, bytes) = values.next().ok_or(Error::InvalidMime)?;
    if values.next().is_some() || bytes.len() > 128 || !bytes.is_ascii() {
        return Err(Error::InvalidMime);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::InvalidMime)?;
    let mut fields = text.split(';');
    if !fields.next().unwrap_or("").trim().eq_ignore_ascii_case("text/event-stream") {
        return Err(Error::InvalidMime);
    }
    if let Some(parameter) = fields.next() {
        let parameter = parameter.trim();
        if !parameter.eq_ignore_ascii_case("charset=utf-8")
            && !parameter.eq_ignore_ascii_case("charset=\"utf-8\"")
        {
            return Err(Error::InvalidMime);
        }
    }
    if fields.next().is_some() {
        return Err(Error::InvalidMime);
    }
    Ok(())
}
