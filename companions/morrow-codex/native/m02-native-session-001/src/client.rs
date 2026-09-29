//! Read-only consumer of the host-owned native-session Cap'n Proto kit.
use crate::{
    Deadline,
    framing::LengthDecoder,
    process_io::{ProcessIo, ReadEvent},
};
use morrow_native_session_wire::{self as wire, Frame, Kind};
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct Options {
    pub queries: u64,
    pub interval_ms: u64,
    pub io_timeout_ms: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            queries: 1,
            interval_ms: 0,
            io_timeout_ms: 1000,
        }
    }
}
impl Options {
    pub fn parse(args: &[std::ffi::OsString]) -> Result<Self, &'static str> {
        let mut result = Self::default();
        let mut seen = std::collections::HashSet::new();
        for pair in args.chunks(2) {
            if pair.len() != 2 {
                return Err("missing option value");
            }
            let key = pair[0].to_str().ok_or("non-text option")?;
            if !seen.insert(key) {
                return Err("duplicate option");
            }
            let value = pair[1]
                .to_str()
                .ok_or("non-text value")?
                .parse::<u64>()
                .map_err(|_| "invalid numeric value")?;
            match key {
                "--queries" if (1..=64).contains(&value) => result.queries = value,
                "--interval-ms" if value <= 10_000 => result.interval_ms = value,
                "--io-timeout-ms" if (100..=5000).contains(&value) => result.io_timeout_ms = value,
                _ => return Err("unsupported option or bound"),
            }
        }
        Ok(result)
    }
}

#[derive(Debug)]
pub struct End {
    pub exit: u8,
    pub reason: &'static str,
}
fn protocol(reason: &'static str) -> End {
    End { exit: 16, reason }
}
fn disconnected() -> End {
    End {
        exit: 24,
        reason: "host pipe disconnected",
    }
}
fn timeout() -> End {
    End {
        exit: 26,
        reason: "local fixed deadline reached; no replay",
    }
}

struct Driver {
    pipes: ProcessIo,
    decoder: LengthDecoder,
    initial: Option<Frame>,
    generation: u64,
    remaining: u64,
    budget: u64,
    lifetime: Deadline,
    io_timeout: Duration,
    partial_started: Option<Instant>,
}
impl Driver {
    fn available(&mut self) -> Result<Option<Frame>, End> {
        let Some(payload) = self.decoder.next_payload().map_err(protocol)? else {
            return Ok(None);
        };
        if !self.decoder.has_partial() {
            self.partial_started = None;
        }
        let mut bytes = (payload.len() as u32).to_le_bytes().to_vec();
        bytes.extend(payload);
        let frame = Frame::decode(&bytes).map_err(protocol)?;
        if let Some(initial) = &self.initial {
            if frame.session != initial.session
                || frame.epoch != initial.epoch
                || frame.pid != initial.pid
                || frame.nonce != initial.nonce
                || frame.schema != initial.schema
                || frame.artifact != initial.artifact
                || frame.config != initial.config
                || frame.capabilities != initial.capabilities
            {
                return Err(End {
                    exit: 17,
                    reason: "host response binding mismatch",
                });
            }
            if frame.generation < self.generation
                || frame.generation > 2
                || frame.remaining_ms > self.remaining
                || frame.budget > self.budget
            {
                return Err(protocol("host response state moved backwards"));
            }
            self.generation = frame.generation;
            self.remaining = frame.remaining_ms;
            self.budget = frame.budget;
            if frame.kind == Kind::Stop {
                if frame.sequence != 0 || !(16..=25).contains(&frame.code) {
                    return Err(protocol("invalid Stop"));
                }
                return Err(End {
                    exit: if frame.code == 25 {
                        0
                    } else {
                        frame.code as u8
                    },
                    reason: "host Stop observed",
                });
            }
        }
        Ok(Some(frame))
    }
    fn tick(&mut self, wait: Duration) -> Result<Option<Frame>, End> {
        if self
            .partial_started
            .is_some_and(|started| started.elapsed() >= self.io_timeout)
        {
            return Err(timeout());
        }
        if self.lifetime.remaining().is_none() {
            return Err(timeout());
        }
        if let Some(frame) = self.available()? {
            return Ok(Some(frame));
        }
        match self.pipes.read_event(wait.min(Duration::from_millis(10))) {
            Ok(ReadEvent::Chunk(bytes)) => {
                if self.partial_started.is_none() {
                    self.partial_started = Some(Instant::now());
                }
                self.decoder.feed(&bytes).map_err(protocol)?;
                self.available()
            }
            Ok(ReadEvent::Eof) => Err(End {
                exit: 24,
                reason: if self.decoder.has_partial() {
                    "truncated host frame at EOF"
                } else {
                    "host pipe EOF"
                },
            }),
            Ok(ReadEvent::Failed(_)) | Err(RecvTimeoutError::Disconnected) => Err(disconnected()),
            Err(RecvTimeoutError::Timeout) => Ok(None),
        }
    }
    fn first(&mut self) -> Result<Frame, End> {
        let deadline = Deadline::start(self.io_timeout);
        loop {
            if let Some(frame) = self.tick(deadline.remaining().ok_or_else(timeout)?)? {
                return Ok(frame);
            }
        }
    }
    fn exchange(&mut self, request: Frame, expected: Kind) -> Result<Frame, End> {
        if self.tick(Duration::ZERO)?.is_some() {
            return Err(protocol("unsolicited data before request"));
        }
        self.pipes.try_write(request.encode()).map_err(protocol)?;
        let deadline = Deadline::start(self.io_timeout);
        let mut written = false;
        let mut reply = None;
        loop {
            // Input/Stop is observed before write completion, including when the
            // writer is blocked. The fixed operation deadline never renews.
            if let Some(frame) = self.tick(deadline.remaining().ok_or_else(timeout)?)? {
                if reply.is_some() {
                    return Err(protocol("multiple replies without request"));
                }
                if frame.sequence != request.sequence {
                    return Err(End {
                        exit: 18,
                        reason: "reply sequence mismatch",
                    });
                }
                if frame.kind == Kind::Denied {
                    if !(16..=25).contains(&frame.code) {
                        return Err(protocol("unknown denial reason"));
                    }
                    return Err(End {
                        exit: frame.code as u8,
                        reason: "host denied request; no fallback",
                    });
                }
                if frame.kind != expected || frame.code != 2 || frame.generation != 1 {
                    return Err(protocol("invalid success phase/kind/generation"));
                }
                reply = Some(frame);
            }
            if !written {
                match self.pipes.poll_write().map_err(protocol)? {
                    Some(Ok(())) => written = true,
                    Some(Err(_)) => return Err(disconnected()),
                    None => {}
                }
            }
            if written && reply.is_some() {
                return Ok(reply.take().unwrap());
            }
        }
    }
    fn interval(&mut self, duration: Duration) -> Result<(), End> {
        let start = Instant::now();
        while start.elapsed() < duration {
            if self
                .tick(duration.saturating_sub(start.elapsed()))?
                .is_some()
            {
                return Err(protocol("unsolicited data while idle"));
            }
        }
        Ok(())
    }
}

pub fn run(options: Options) -> Result<(), End> {
    // Host TTL is authoritative; this independent 60-second safety cap never renews.
    let lifetime = Deadline::start(Duration::from_secs(60));
    let pipes = ProcessIo::claim_stdio(wire::MAX_FRAME, wire::MAX_FRAME)
        .map_err(|_| protocol("cannot claim protocol pipes"))?;
    let mut driver = Driver {
        pipes,
        decoder: LengthDecoder::new(wire::MAX_PAYLOAD, wire::MAX_FRAME)
            .map_err(protocol)?
            .with_prefix_validator(wire::payload_length),
        initial: None,
        generation: 1,
        remaining: u64::MAX,
        budget: u64::MAX,
        lifetime,
        io_timeout: Duration::from_millis(options.io_timeout_ms),
        partial_started: None,
    };
    let initial = driver.first()?;
    if initial.kind != Kind::Challenge
        || initial.sequence != 0
        || initial.code != 0
        || initial.session == 0
        || initial.epoch == 0
        || initial.generation != 1
        || initial.pid != std::process::id()
        || initial.schema != wire::schema_digest()
        || initial.capabilities != 1
        || initial.remaining_ms == 0
        || initial.budget == 0
        || initial.nonce == [0; 32]
        || initial.config == [0; 32]
    {
        return Err(End {
            exit: 17,
            reason: "invalid Challenge identity/capability",
        });
    }
    let own = std::env::current_exe().map_err(|_| protocol("own artifact unavailable"))?;
    let size = std::fs::metadata(&own)
        .map_err(|_| protocol("own artifact metadata unavailable"))?
        .len();
    if size > 64 * 1024 * 1024 {
        return Err(protocol("own artifact exceeds verification bound"));
    }
    let bytes = std::fs::read(own).map_err(|_| protocol("own artifact digest unavailable"))?;
    if wire::digest(&bytes) != initial.artifact {
        return Err(End {
            exit: 17,
            reason: "own artifact differs from Challenge",
        });
    }
    driver.remaining = initial.remaining_ms;
    driver.budget = initial.budget;
    driver.initial = Some(initial.clone());
    driver.exchange(initial.request(Kind::Hello, 1), Kind::Welcome)?;
    for index in 0..options.queries {
        if index > 0 {
            driver.interval(Duration::from_millis(options.interval_ms))?;
        }
        driver.exchange(initial.request(Kind::Query, index + 2), Kind::State)?;
    }
    driver.exchange(
        initial.request(Kind::Close, options.queries + 2),
        Kind::Stop,
    )?;
    Err(protocol("Close returned without Stop"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_reject_authority_claims_duplicates_and_unbounded_load() {
        for args in [
            vec!["--approved", "true"],
            vec!["--queries", "65"],
            vec!["--queries", "1", "--queries", "2"],
            vec!["--interval-ms", "10001"],
            vec!["--io-timeout-ms", "0"],
        ] {
            assert!(Options::parse(&args.into_iter().map(Into::into).collect::<Vec<_>>()).is_err());
        }
    }
    #[test]
    fn consumes_fixed_host_vectors_and_rejects_invalid_vectors() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/capnp-kit-001/vectors");
        for name in [
            "challenge",
            "hello",
            "welcome",
            "query",
            "state",
            "denied-revoked",
            "stop",
            "close",
        ] {
            let bytes = std::fs::read(root.join(format!("{name}.frame"))).unwrap();
            let frame = Frame::decode(&bytes).unwrap();
            assert_eq!(frame.encode(), bytes);
        }
        for name in ["bad-root", "oversize-prefix", "truncated"] {
            assert!(
                Frame::decode(&std::fs::read(root.join(format!("{name}.invalid"))).unwrap())
                    .is_err()
            );
        }
    }
}
