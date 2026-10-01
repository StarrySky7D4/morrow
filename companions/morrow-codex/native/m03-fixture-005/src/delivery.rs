//! Session-owned bounded queue. Waiting futures borrow it; they never own a
//! pending OS operation or a not-yet-delivered chunk.
use bytes::Bytes;
use morrow_codex_m03_stream::driver::ConsumptionProgress;
use morrow_native_http_stream_wire::{Credit, MAX_CREDIT};
use std::collections::VecDeque;

type Result<T> = std::result::Result<T, &'static str>;
pub enum Available {
    Bytes(Bytes),
    Eof,
    Pending,
}
pub struct Delivery {
    chunks: VecDeque<Bytes>,
    limit: u64,
    max_chunk: usize,
    received: u64,
    delivered: u64,
    final_offset: Option<u64>,
    cancelled: bool,
    progress: ConsumptionProgress,
}
impl Delivery {
    pub fn new(limit: usize, max_chunk: usize) -> Result<Self> {
        if limit == 0 || limit > 65536 || max_chunk == 0 || max_chunk > 8192 {
            return Err("delivery limits");
        }
        Ok(Self {
            chunks: VecDeque::new(),
            limit: limit as u64,
            max_chunk,
            received: 0,
            delivered: 0,
            final_offset: None,
            cancelled: false,
            progress: ConsumptionProgress {
                consumed_offset: 0,
                parser_yielded_bytes: 0,
                drain_discarded_bytes: 0,
                error_body_consumed_bytes: 0,
            },
        })
    }
    /// Reserve room before issuing the next pipe read. A single owned I/O thread
    /// is the only producer; no third chunk may be read into an application queue.
    pub fn can_issue_read(&self) -> bool {
        !self.cancelled && self.chunks.len() < 2
    }
    pub fn received_offset(&self) -> u64 {
        self.received
    }
    pub fn delivered_offset(&self) -> u64 {
        self.delivered
    }
    pub fn final_offset(&self) -> Option<u64> {
        self.final_offset
    }
    pub fn at_limit(&self) -> bool {
        self.received == self.limit
    }
    pub fn received(&mut self, offset: u64, bytes: Bytes) -> Result<()> {
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or("delivery overflow")?;
        if offset != self.received
            || bytes.is_empty()
            || bytes.len() > self.max_chunk
            || end > self.limit
            || self.final_offset.is_some_and(|n| end > n)
        {
            return Err("noncontiguous/out-of-budget data");
        }
        if !self.cancelled && self.chunks.len() == 2 {
            return Err("reader exceeded two owned chunks");
        }
        self.received = end;
        if !self.cancelled {
            self.chunks.push_back(bytes);
        }
        Ok(())
    }
    pub fn http_eof(&mut self, final_offset: u64) -> Result<()> {
        if final_offset > self.limit
            || final_offset < self.received
            || self.final_offset.is_some_and(|old| old != final_offset)
        {
            return Err("conflicting HTTP final offset");
        }
        self.final_offset = Some(final_offset);
        Ok(())
    }
    pub fn take(&mut self, offset: u64, max: usize) -> Result<Available> {
        if self.cancelled {
            return Err("cancelled delivery");
        }
        if offset != self.delivered || max == 0 || max > self.max_chunk {
            return Err("read offset/limit");
        }
        if let Some(front) = self.chunks.front_mut() {
            let count = max.min(front.len());
            let bytes = front.split_to(count);
            if front.is_empty() {
                self.chunks.pop_front();
            }
            self.delivered += count as u64;
            return Ok(Available::Bytes(bytes));
        }
        if self.final_offset == Some(self.delivered) && self.received == self.delivered {
            Ok(Available::Eof)
        } else {
            Ok(Available::Pending)
        }
    }
    pub fn consumed(&mut self, p: ConsumptionProgress) -> Result<()> {
        if p.parser_yielded_bytes
            .checked_add(p.drain_discarded_bytes)
            .and_then(|n| n.checked_add(p.error_body_consumed_bytes))
            != Some(p.consumed_offset)
            || p.consumed_offset > self.delivered
            || p.consumed_offset < self.progress.consumed_offset
            || p.parser_yielded_bytes < self.progress.parser_yielded_bytes
            || p.drain_discarded_bytes < self.progress.drain_discarded_bytes
            || p.error_body_consumed_bytes < self.progress.error_body_consumed_bytes
        {
            return Err("consumer classifications");
        }
        self.progress = p;
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.chunks.clear();
    }
    pub fn credit(&self) -> Credit {
        // Last delivered chunk can be unACKed when owner/cancel wins. If that
        // classification gap exists, keep the tail explicitly unACKed. Never
        // relabel parser-delivered bytes as cancelled merely to close a prefix.
        let discarded = if self.cancelled && self.progress.consumed_offset == self.delivered {
            self.received - self.delivered
        } else {
            0
        };
        Credit {
            consumed_offset: self.progress.consumed_offset + discarded,
            parser_yielded_bytes: self.progress.parser_yielded_bytes,
            drain_discarded_bytes: self.progress.drain_discarded_bytes,
            error_consumed_bytes: self.progress.error_body_consumed_bytes,
            cancel_discarded_bytes: discarded,
            window_bytes: if self.cancelled { 0 } else { MAX_CREDIT },
            max_chunk_bytes: self.max_chunk as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn early_control_eof_waits_for_contiguous_data_and_survives_detached_waiter() {
        let mut queue = Delivery::new(3, 1024).unwrap();
        queue.http_eof(3).unwrap();
        assert!(matches!(queue.take(0, 1024).unwrap(), Available::Pending));
        // A pending read future may disappear here; the session still owns queue.
        queue.received(0, Bytes::from_static(b"abc")).unwrap();
        assert!(
            matches!(queue.take(0,1024).unwrap(), Available::Bytes(bytes) if bytes.as_ref() == b"abc")
        );
        assert!(matches!(queue.take(3, 1024).unwrap(), Available::Eof));
    }
    #[test]
    fn exact_limit_is_not_eof_and_cancel_never_relabels_unacked_parser_bytes() {
        let mut queue = Delivery::new(3, 1024).unwrap();
        queue.received(0, Bytes::from_static(b"abc")).unwrap();
        let _ = queue.take(0, 1024).unwrap();
        assert!(queue.received(3, Bytes::from_static(b"d")).is_err());
        assert!(matches!(queue.take(3, 1024).unwrap(), Available::Pending));
        queue.cancel();
        let credit = queue.credit();
        assert_eq!(credit.consumed_offset, 0);
        assert_eq!(credit.cancel_discarded_bytes, 0);
        assert_eq!(credit.window_bytes, 0);
        assert!(queue.take(3, 1024).is_err());
    }
    #[test]
    fn late_cancel_bytes_only_extend_a_fully_classified_prefix() {
        let mut queue = Delivery::new(5, 1024).unwrap();
        queue.received(0, Bytes::from_static(b"abc")).unwrap();
        let _ = queue.take(0, 1024).unwrap();
        queue
            .consumed(ConsumptionProgress {
                consumed_offset: 3,
                parser_yielded_bytes: 3,
                drain_discarded_bytes: 0,
                error_body_consumed_bytes: 0,
            })
            .unwrap();
        queue.cancel();
        queue.received(3, Bytes::from_static(b"de")).unwrap();
        let credit = queue.credit();
        assert_eq!(credit.consumed_offset, 5);
        assert_eq!(credit.parser_yielded_bytes, 3);
        assert_eq!(credit.cancel_discarded_bytes, 2);
        assert_eq!(credit.window_bytes, 0);
        assert_eq!(queue.credit(), credit);
    }
}
