//! Client-side bounds only. Authority, wire and connection identity belong to the host.
pub mod client;
pub mod framing;
pub mod process_io;
use std::time::{Duration, Instant};

/// An immutable local timeout, created once when a client operation begins.
/// It never creates authority and cannot extend the host's admission deadline.
#[derive(Debug)]
pub struct Deadline {
    started: Instant,
    duration: Duration,
}

impl Deadline {
    pub fn start(duration: Duration) -> Self {
        Self {
            started: Instant::now(),
            duration,
        }
    }

    pub fn remaining(&self) -> Option<Duration> {
        self.duration
            .checked_sub(self.started.elapsed())
            .filter(|left| !left.is_zero())
    }
}

/// Accumulates one bounded unit supplied by a future host-owned framing codec.
/// Overflow clears pending bytes and poisons the buffer; callers must close the channel.
#[derive(Debug)]
pub struct BoundedBuffer {
    bytes: Vec<u8>,
    maximum: usize,
    poisoned: bool,
}

impl BoundedBuffer {
    pub fn new(maximum: usize) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
            poisoned: false,
        }
    }

    pub fn append(&mut self, chunk: &[u8]) -> Result<(), &'static str> {
        if self.poisoned {
            return Err("buffer already rejected");
        }
        if chunk.len() > self.maximum.saturating_sub(self.bytes.len()) {
            self.bytes.clear();
            self.poisoned = true;
            return Err("host framing byte bound exceeded");
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }

    pub fn bytes(&self) -> Result<&[u8], &'static str> {
        if self.poisoned {
            Err("buffer already rejected")
        } else {
            Ok(&self.bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragmented_boundary_accepts_then_poisoned_overflow_cannot_resume() {
        let mut buffer = BoundedBuffer::new(4);
        buffer.append(b"ab").unwrap();
        buffer.append(b"cd").unwrap();
        assert_eq!(buffer.bytes().unwrap(), b"abcd");
        assert_eq!(buffer.append(b"e"), Err("host framing byte bound exceeded"));
        assert_eq!(buffer.bytes(), Err("buffer already rejected"));
        assert_eq!(buffer.append(b""), Err("buffer already rejected"));
    }

    #[test]
    fn zero_budget_cannot_accept_nonempty_input_or_renew_deadline() {
        let mut buffer = BoundedBuffer::new(0);
        assert!(buffer.append(b"x").is_err());
        let deadline = Deadline::start(Duration::ZERO);
        assert_eq!(deadline.remaining(), None);
        assert_eq!(deadline.remaining(), None);
    }
}
