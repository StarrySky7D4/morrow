//! Runtime frames may be embedded at any byte offset in a transport envelope.
use morrow_core::{
    response::{Failure, Outcome, Response},
    runtime::{Command, MAX_MESSAGE_BYTES},
};
#[test]
fn embedded_runtime_command_and_response_decode_at_every_byte_alignment() {
    let command = Command::ReadSummary {
        request_id: "read".into(),
        card_id: "card".into(),
    };
    let response = Response {
        request_id: "read".into(),
        outcome: Outcome::Rejected(Failure::Denied),
    };
    for prefix in 0..8 {
        let raw = command.encode().unwrap();
        let mut embedded = vec![0; prefix];
        embedded.extend_from_slice(&raw);
        assert_eq!(
            Command::decode(&embedded[prefix..]).unwrap(),
            command,
            "command offset {prefix}"
        );
        let raw = response.encode().unwrap();
        let mut embedded = vec![0; prefix];
        embedded.extend_from_slice(&raw);
        assert_eq!(
            Response::decode(&embedded[prefix..]).unwrap(),
            response,
            "response offset {prefix}"
        );
    }
}
#[test]
fn owned_runtime_reader_keeps_framing_and_allocation_bounds() {
    let raw = Command::ReadSummary {
        request_id: "read".into(),
        card_id: "card".into(),
    }
    .encode()
    .unwrap();
    let mut trailing = raw.clone();
    trailing.extend_from_slice(&[0; 8]);
    assert!(Command::decode(&trailing).is_err());
    for end in 0..raw.len() {
        assert!(Command::decode(&raw[..end]).is_err());
    }
    assert!(Command::decode(&vec![0; MAX_MESSAGE_BYTES + 1]).is_err());
    // One segment announces far more words than the bounded reader allows.
    assert!(Command::decode(&[0, 0, 0, 0, 255, 255, 255, 127]).is_err());
}
