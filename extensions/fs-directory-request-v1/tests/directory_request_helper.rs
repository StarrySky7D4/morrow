//! Execution-ready public-codec regressions proposed for the new request crate.
//! NOT_RUN: source draft only, no authority or native owner substitute.
use morrow_fs_directory_request_v1::{
    Action, ClientState, Error, FsDirectoryPage, Opened, Phase, Reply, Request, Response,
    transport::call_once_with,
};
use std::cell::Cell;

fn opened(state: &mut ClientState) -> [u8; 32] {
    let request = state.begin([2; 32], Action::Open).unwrap();
    let epoch = [3; 32];
    let response = Response::new(&request, Reply::Opened(Opened {
        selection_epoch: epoch, page_sequence: 1, after_entry_id: None,
        entries: 0, metadata_bytes: 0,
    })).unwrap();
    state.accept(response.wire()).unwrap();
    epoch
}

#[test]
fn empty_nonterminal_page_produces_a_valid_next_cursor_without_changing_old_page_codec() {
    let mut state = ClientState::new([1; 32]).unwrap();
    let epoch = opened(&mut state);
    let request = state.begin([4; 32], state.next_action().unwrap()).unwrap();
    let page = FsDirectoryPage {
        selection_epoch: epoch, page_sequence: 1, entries: Vec::new(), terminal: false,
    };
    // The existing page codec deliberately allows empty nonterminal pages.
    page.validate().unwrap();
    let response = Response::new(&request, Reply::Page(page)).unwrap();
    state.accept(response.wire()).unwrap();
    assert_eq!(state.snapshot().phase, Phase::Ready);
    let next = state.next_action().unwrap();
    assert_eq!(next, Action::Next { selection_epoch: epoch, page_sequence: 2, after_entry_id: None });
    let encoded = state.begin([5; 32], next).unwrap();
    Request::decode(encoded.wire()).unwrap();
}

#[test]
fn other_original_request_response_closes_without_cursor_advance_or_refund() {
    let mut state = ClientState::new([1; 32]).unwrap();
    let epoch = opened(&mut state);
    let action = state.next_action().unwrap();
    let pending = state.begin([4; 32], action).unwrap();
    let before = state.snapshot();
    let other = Request::new([1; 32], [5; 32], action).unwrap();
    let response = Response::new(&other, Reply::Page(FsDirectoryPage {
        selection_epoch: epoch, page_sequence: 1, entries: Vec::new(), terminal: true,
    })).unwrap();
    assert_ne!(pending.digest(), other.digest());
    assert_eq!(state.accept(response.wire()), Err(Error::Correlation));
    let after = state.snapshot();
    assert_eq!(after.phase, Phase::Unknown);
    assert_eq!(after.next_sequence, before.next_sequence);
    assert_eq!(after.calls, before.calls);
    assert_eq!(after.request_wire_bytes, before.request_wire_bytes);
    assert_eq!(after.response_wire_bytes, before.response_wire_bytes + response.wire().len() as u64);
    assert!(state.begin([6; 32], action).is_err());
}

#[test]
fn uncertain_transport_calls_once_and_requires_explicit_unknown_close() {
    let mut state = ClientState::new([1; 32]).unwrap();
    let request = state.begin([2; 32], Action::Open).unwrap();
    let calls = Cell::new(0);
    assert_eq!(call_once_with(&request, |input, output| {
        calls.set(calls.get() + 1);
        assert_eq!(input, request.wire());
        assert_eq!(output.len(), 65536);
        -1
    }), Err(Error::OutcomeUnknown));
    assert_eq!(calls.get(), 1);
    // The pure one-shot transport does not mutate a separately owned helper.
    assert_eq!(state.snapshot().phase, Phase::Pending);
    state.transport_unknown().unwrap();
    assert_eq!(state.snapshot().phase, Phase::Unknown);
    assert!(state.begin([3; 32], Action::Open).is_err());
}

#[test]
fn terminal_page_closes_helper_but_does_not_assert_native_worker_join() {
    let mut state = ClientState::new([1; 32]).unwrap();
    let epoch = opened(&mut state);
    let request = state.begin([4; 32], state.next_action().unwrap()).unwrap();
    let response = Response::new(&request, Reply::Page(FsDirectoryPage {
        selection_epoch: epoch, page_sequence: 1, entries: Vec::new(), terminal: true,
    })).unwrap();
    state.accept(response.wire()).unwrap();
    assert_eq!(state.snapshot().phase, Phase::Terminal);
    assert!(state.next_action().is_err());
    assert!(state.finish_action().is_err());
    // Actual worker resource release/join needs the separate native owner suite.
}
