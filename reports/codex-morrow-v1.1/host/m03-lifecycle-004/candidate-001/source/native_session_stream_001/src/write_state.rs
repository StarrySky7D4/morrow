//! Platform-independent accounting used by the real owned-pipe driver.
//! OS operation completion is not protocol-frame completion or peer consumption.
#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Continue,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    EmptyFrame,
    NotIssuable,
    InvalidIssue,
    UntrackedCompletion,
    InvalidLength,
    ZeroProgress,
    Os(u32),
}

pub(crate) struct FrameWrite {
    total: usize,
    offset: usize,
    active: Option<u64>,
    last_id: u64,
    stopped: bool,
}

impl FrameWrite {
    pub(crate) fn new(total: usize) -> Result<Self, Failure> {
        if total == 0 {
            return Err(Failure::EmptyFrame);
        }
        Ok(Self {
            total,
            offset: 0,
            active: None,
            last_id: 0,
            stopped: false,
        })
    }

    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    pub(crate) fn issue(&mut self, id: u64, requested: usize) -> Result<(), Failure> {
        if self.stopped || self.active.is_some() || self.offset == self.total {
            return Err(Failure::NotIssuable);
        }
        if id == 0 || id <= self.last_id || requested != self.total - self.offset {
            self.stopped = true;
            return Err(Failure::InvalidIssue);
        }
        self.active = Some(id);
        self.last_id = id;
        Ok(())
    }

    pub(crate) fn complete(
        &mut self,
        id: u64,
        transferred: usize,
        error: Option<u32>,
        cancelling: bool,
    ) -> Result<Outcome, Failure> {
        let result = self.record(id, transferred, error, cancelling);
        if !matches!(result, Ok(Outcome::Continue)) {
            self.stopped = true;
        }
        result
    }

    fn record(
        &mut self,
        id: u64,
        transferred: usize,
        error: Option<u32>,
        cancelling: bool,
    ) -> Result<Outcome, Failure> {
        if self.stopped || self.active != Some(id) {
            return Err(Failure::UntrackedCompletion);
        }
        self.active = None;
        // Validate before addition, including error completions. Never infer a
        // successful prefix from a byte count returned alongside an OS error.
        if transferred > self.total - self.offset {
            return Err(Failure::InvalidLength);
        }
        if let Some(code) = error {
            return if cancelling && code == 995 {
                // ERROR_OPERATION_ABORTED
                Ok(Outcome::Cancelled)
            } else {
                Err(Failure::Os(code))
            };
        }
        if transferred == 0 {
            return Err(Failure::ZeroProgress);
        }
        self.offset += transferred;
        if self.offset == self.total {
            // Cancellation cannot undo an already observed complete OS write.
            Ok(Outcome::Complete)
        } else if cancelling {
            Ok(Outcome::Cancelled)
        } else {
            Ok(Outcome::Continue)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiple_partial_completions_finish_only_the_whole_original_frame() {
        let mut w = FrameWrite::new(10).unwrap();
        for (id, size, expected) in [
            (1, 3, Outcome::Continue),
            (2, 2, Outcome::Continue),
            (3, 5, Outcome::Complete),
        ] {
            w.issue(id, 10 - w.offset()).unwrap();
            assert_eq!(w.complete(id, size, None, false), Ok(expected));
        }
        assert_eq!(w.offset(), 10);
        assert_eq!(w.issue(4, 0), Err(Failure::NotIssuable));
    }

    #[test]
    fn cancelled_partial_tail_is_never_reissued_or_reported_complete() {
        let mut w = FrameWrite::new(10).unwrap();
        w.issue(1, 10).unwrap();
        assert_eq!(w.complete(1, 4, None, true), Ok(Outcome::Cancelled));
        assert_eq!(w.offset(), 4);
        assert_eq!(w.issue(2, 6), Err(Failure::NotIssuable));
    }

    #[test]
    fn successful_completion_at_cancel_keeps_the_actual_complete_fact_once() {
        let mut w = FrameWrite::new(10).unwrap();
        w.issue(5, 10).unwrap();
        assert_eq!(w.complete(5, 10, None, true), Ok(Outcome::Complete));
        assert_eq!(
            w.complete(5, 10, None, true),
            Err(Failure::UntrackedCompletion)
        );
    }

    #[test]
    fn aborted_operation_with_reported_bytes_is_not_a_complete_frame() {
        for size in [0, 4, 10] {
            let mut w = FrameWrite::new(10).unwrap();
            w.issue(1, 10).unwrap();
            assert_eq!(w.complete(1, size, Some(995), true), Ok(Outcome::Cancelled));
            assert_eq!(w.offset(), 0);
        }
    }

    #[test]
    fn failure_after_confirmed_prefix_preserves_only_that_prefix() {
        let mut w = FrameWrite::new(10).unwrap();
        w.issue(1, 10).unwrap();
        assert_eq!(w.complete(1, 4, None, false), Ok(Outcome::Continue));
        w.issue(2, 6).unwrap();
        assert_eq!(w.complete(2, 2, Some(109), true), Err(Failure::Os(109)));
        assert_eq!(w.offset(), 4);
        assert_eq!(w.issue(3, 6), Err(Failure::NotIssuable));
    }

    #[test]
    fn wrong_or_reused_operation_cannot_advance_offset() {
        let mut w = FrameWrite::new(10).unwrap();
        w.issue(1, 10).unwrap();
        assert_eq!(
            w.complete(2, 4, None, false),
            Err(Failure::UntrackedCompletion)
        );
        assert_eq!(w.offset(), 0);
        let mut w = FrameWrite::new(10).unwrap();
        w.issue(1, 10).unwrap();
        w.complete(1, 4, None, false).unwrap();
        assert_eq!(w.issue(1, 6), Err(Failure::InvalidIssue));
    }

    #[test]
    fn zero_oversized_and_overflow_lengths_fail_closed() {
        for (total, size, expected) in [
            (10, 0, Failure::ZeroProgress),
            (10, 11, Failure::InvalidLength),
        ] {
            let mut w = FrameWrite::new(total).unwrap();
            w.issue(1, total).unwrap();
            assert_eq!(w.complete(1, size, None, false), Err(expected));
            assert_eq!(w.offset(), 0);
        }
        let mut w = FrameWrite::new(usize::MAX).unwrap();
        w.issue(1, usize::MAX).unwrap();
        w.complete(1, usize::MAX - 1, None, false).unwrap();
        w.issue(2, 1).unwrap();
        assert_eq!(w.complete(2, 2, None, false), Err(Failure::InvalidLength));
    }

    #[test]
    fn unexpected_abort_and_wrong_requested_length_are_errors() {
        assert!(matches!(FrameWrite::new(0), Err(Failure::EmptyFrame)));
        let mut w = FrameWrite::new(10).unwrap();
        assert_eq!(w.issue(1, 9), Err(Failure::InvalidIssue));
        let mut w = FrameWrite::new(10).unwrap();
        w.issue(1, 10).unwrap();
        assert_eq!(w.complete(1, 0, Some(995), false), Err(Failure::Os(995)));
    }
}
