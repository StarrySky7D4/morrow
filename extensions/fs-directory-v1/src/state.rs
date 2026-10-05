//! Local bounded semantic validation, never scope approval or backend support.
//! Admission charges and accepted semantic state are intentionally separate.
use crate::{Error, FsDirectoryPage, MAX_ENVELOPE_BYTES, Result, nonzero};
use std::collections::BTreeSet;
pub const MAX_STATE_ENTRIES: usize = 1024;
pub const MAX_STATE_PAGES: u64 = 1024;
pub const MAX_STATE_NAME_BYTES: u64 = 1024 * 1024;
pub const MAX_STATE_WIRE_BYTES: u64 = 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateLimits {
    pub max_entries: usize,
    pub max_pages: u64,
    pub max_name_bytes: u64,
    pub max_wire_bytes: u64,
}
impl Default for StateLimits {
    fn default() -> Self {
        Self {
            max_entries: MAX_STATE_ENTRIES,
            max_pages: MAX_STATE_PAGES,
            max_name_bytes: MAX_STATE_NAME_BYTES,
            max_wire_bytes: MAX_STATE_WIRE_BYTES,
        }
    }
}
impl StateLimits {
    pub fn validate(&self) -> Result<()> {
        if self.max_entries > MAX_STATE_ENTRIES
            || self.max_pages > MAX_STATE_PAGES
            || self.max_name_bytes > MAX_STATE_NAME_BYTES
            || self.max_wire_bytes > MAX_STATE_WIRE_BYTES
        {
            Err(Error::Limit)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateSnapshot {
    pub accepted_pages: u64,
    pub accepted_entries: usize,
    pub accepted_name_bytes: u64,
    pub next_sequence: u64,
    pub admitted_wire_bytes: u64,
    pub resident_ids: usize,
    pub terminal: bool,
    pub released: bool,
}
pub struct DirectoryState {
    epoch: [u8; 32],
    limits: StateLimits,
    ids: BTreeSet<[u8; 32]>,
    snapshot: StateSnapshot,
}
impl std::fmt::Debug for DirectoryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectoryState")
            .field("limits", &self.limits)
            .field("snapshot", &self.snapshot)
            .finish()
    }
}
impl DirectoryState {
    pub fn new(expected_epoch: [u8; 32], limits: StateLimits) -> Result<Self> {
        if !nonzero(&expected_epoch) {
            return Err(Error::Epoch);
        }
        limits.validate()?;
        Ok(Self {
            epoch: expected_epoch,
            limits,
            ids: BTreeSet::new(),
            snapshot: StateSnapshot {
                accepted_pages: 0,
                accepted_entries: 0,
                accepted_name_bytes: 0,
                next_sequence: 1,
                admitted_wire_bytes: 0,
                resident_ids: 0,
                terminal: false,
                released: false,
            },
        })
    }
    pub fn snapshot(&self) -> StateSnapshot {
        self.snapshot
    }
    /// Release retained identity resources and close this validator. Cumulative
    /// admission and semantic counters remain observable and are never refunded.
    pub fn release(&mut self) {
        self.ids.clear();
        self.snapshot.resident_ids = 0;
        self.snapshot.released = true;
    }
    /// Reserve actual wire once before decoding. Size/budget/refusal before
    /// admission reserves nothing. Every admitted malformed/foreign/duplicate or
    /// post-terminal page keeps its charge but advances no accepted semantic state.
    /// Liveness/deadline and real original broker charges are outside this helper.
    pub fn admit(&mut self, wire: &[u8]) -> Result<FsDirectoryPage> {
        if self.snapshot.released {
            return Err(Error::Released);
        }
        if wire.is_empty() || wire.len() > MAX_ENVELOPE_BYTES {
            return Err(Error::Limit);
        }
        let charged = self
            .snapshot
            .admitted_wire_bytes
            .checked_add(wire.len() as u64)
            .filter(|n| *n <= self.limits.max_wire_bytes)
            .ok_or(Error::Budget)?;
        self.snapshot.admitted_wire_bytes = charged;
        let page = FsDirectoryPage::decode(wire)?;
        self.accept_decoded(&page)?;
        Ok(page)
    }
    /// Convenience for trusted producers: charge actual default-allocator wire,
    /// rather than accepting a caller-provided cost or approval boolean.
    pub fn accept_page(&mut self, page: &FsDirectoryPage) -> Result<()> {
        if self.snapshot.released {
            return Err(Error::Released);
        }
        self.admit(&page.encode()?).map(|_| ())
    }
    fn accept_decoded(&mut self, page: &FsDirectoryPage) -> Result<()> {
        if self.snapshot.terminal {
            return Err(Error::Terminal);
        }
        if page.selection_epoch != self.epoch {
            return Err(Error::Epoch);
        }
        if page.page_sequence != self.snapshot.next_sequence {
            return Err(Error::Sequence);
        }
        let pages = self
            .snapshot
            .accepted_pages
            .checked_add(1)
            .filter(|n| *n <= self.limits.max_pages)
            .ok_or(Error::Budget)?;
        let entries = self
            .snapshot
            .accepted_entries
            .checked_add(page.entries.len())
            .filter(|n| *n <= self.limits.max_entries)
            .ok_or(Error::Budget)?;
        let names = page
            .entries
            .iter()
            .try_fold(self.snapshot.accepted_name_bytes, |sum, e| {
                sum.checked_add(e.name.len() as u64)
            })
            .filter(|n| *n <= self.limits.max_name_bytes)
            .ok_or(Error::Budget)?;
        let next = page.page_sequence.checked_add(1).ok_or(Error::Sequence)?;
        if page.entries.iter().any(|e| self.ids.contains(&e.entry_id)) {
            return Err(Error::Duplicate);
        }
        // All fallible semantic checks finished. OOM follows ordinary Rust allocation
        // semantics; this codec does not promise recovery from allocator failure.
        self.ids.extend(page.entries.iter().map(|e| e.entry_id));
        self.snapshot.accepted_pages = pages;
        self.snapshot.accepted_entries = entries;
        self.snapshot.accepted_name_bytes = names;
        self.snapshot.next_sequence = next;
        self.snapshot.resident_ids = self.ids.len();
        self.snapshot.terminal = page.terminal;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::page;
    fn semantic(mut s: StateSnapshot) -> StateSnapshot {
        s.admitted_wire_bytes = 0;
        s
    }
    #[test]
    fn foreign_gap_duplicate_and_terminal_fail_atomically_but_keep_admission_charge() {
        let mut state = DirectoryState::new([1; 32], StateLimits::default()).unwrap();
        state.accept_page(&page(1, 2, false)).unwrap();
        let saved = state.snapshot();
        for (p, error) in [
            (page(3, 3, false), Error::Sequence),
            (page(2, 2, false), Error::Duplicate),
        ] {
            let before = state.snapshot().admitted_wire_bytes;
            assert_eq!(state.accept_page(&p), Err(error));
            assert_eq!(semantic(state.snapshot()), semantic(saved));
            assert!(state.snapshot().admitted_wire_bytes > before);
        }
        let mut foreign = page(2, 3, false);
        foreign.selection_epoch = [9; 32];
        assert_eq!(state.accept_page(&foreign), Err(Error::Epoch));
        assert_eq!(semantic(state.snapshot()), semantic(saved));
        state.accept_page(&page(2, 3, true)).unwrap();
        let end = state.snapshot();
        assert_eq!(state.accept_page(&page(3, 4, false)), Err(Error::Terminal));
        assert_eq!(semantic(state.snapshot()), semantic(end));
    }
    #[test]
    fn malformed_admission_budget_refusal_and_release_do_not_refund() {
        let good = page(1, 2, false).encode().unwrap();
        let mut state = DirectoryState::new(
            [1; 32],
            StateLimits {
                max_wire_bytes: good.len() as u64,
                ..StateLimits::default()
            },
        )
        .unwrap();
        assert!(state.admit(&[0; 8]).is_err());
        assert_eq!(state.snapshot().admitted_wire_bytes, 8);
        let saved = state.snapshot();
        assert_eq!(state.admit(&good), Err(Error::Budget));
        assert_eq!(state.snapshot(), saved);
        state.release();
        assert_eq!(state.snapshot().admitted_wire_bytes, 8);
        assert_eq!(state.admit(&good), Err(Error::Released));
        assert_eq!(state.snapshot().admitted_wire_bytes, 8);
    }
    #[test]
    fn entry_name_page_limits_plus_one_preserve_semantic_state() {
        for limits in [
            StateLimits {
                max_entries: 0,
                ..StateLimits::default()
            },
            StateLimits {
                max_name_bytes: 0,
                ..StateLimits::default()
            },
            StateLimits {
                max_pages: 0,
                ..StateLimits::default()
            },
        ] {
            let mut s = DirectoryState::new([1; 32], limits).unwrap();
            let saved = s.snapshot();
            assert_eq!(s.accept_page(&page(1, 2, false)), Err(Error::Budget));
            assert_eq!(semantic(s.snapshot()), semantic(saved));
            assert!(s.snapshot().admitted_wire_bytes > 0);
        }
        assert!(DirectoryState::new([0; 32], StateLimits::default()).is_err());
        assert!(
            DirectoryState::new(
                [1; 32],
                StateLimits {
                    max_entries: 1025,
                    ..StateLimits::default()
                }
            )
            .is_err()
        );
        assert!(
            DirectoryState::new(
                [1; 32],
                StateLimits {
                    max_pages: 1025,
                    ..StateLimits::default()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn full1024_identity_budget_and_release_have_no_hidden_replay_credit() {
        let mut s = DirectoryState::new([1; 32], StateLimits::default()).unwrap();
        for seq in 1..=32 {
            let mut p = page(seq, 1, false);
            p.entries = (0..32)
                .map(|n| {
                    let mut e = p.entries[0].clone();
                    e.entry_id = [0; 32];
                    e.entry_id[..8].copy_from_slice(&(seq * 32 + n).to_le_bytes());
                    e.name = b"x".to_vec();
                    e
                })
                .collect();
            s.accept_page(&p).unwrap();
        }
        assert_eq!(s.snapshot().resident_ids, 1024);
        let saved = s.snapshot();
        assert_eq!(s.accept_page(&page(33, 255, false)), Err(Error::Budget));
        assert_eq!(semantic(s.snapshot()), semantic(saved));
        let paid = s.snapshot().admitted_wire_bytes;
        s.release();
        assert_eq!(s.snapshot().resident_ids, 0);
        assert_eq!(s.snapshot().accepted_entries, 1024);
        assert_eq!(s.snapshot().admitted_wire_bytes, paid);
    }
    #[test]
    fn empty_pages_are_bounded_and_terminal_empty_page_is_a_valid_observation() {
        let mut s = DirectoryState::new(
            [1; 32],
            StateLimits {
                max_pages: 2,
                ..StateLimits::default()
            },
        )
        .unwrap();
        let mut p = page(1, 2, false);
        p.entries.clear();
        s.accept_page(&p).unwrap();
        p.page_sequence = 2;
        p.terminal = true;
        s.accept_page(&p).unwrap();
        assert_eq!(s.snapshot().accepted_pages, 2);
        assert_eq!(s.snapshot().accepted_entries, 0);
        assert!(s.snapshot().terminal);
    }
}
