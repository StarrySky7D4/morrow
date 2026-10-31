use super::*;
use crate::state::{ClientState, Limits, Phase};

fn name(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}
fn batch(sequence: u64, end: End) -> Batch {
    Batch {
        watch_epoch: [7; 32],
        sequence,
        changes: if end == End::Continue {
            vec![Change {
                kind: Kind::Added,
                name_utf16_le: name("雪 😀.txt"),
            }]
        } else {
            vec![]
        },
        end,
    }
}
fn limits() -> Limits {
    Limits {
        frames: 4,
        changes: 32,
        wire_bytes: 32768,
    }
}
fn new_client() -> ClientState {
    ClientState::new([7; 32], limits()).unwrap()
}
fn wire_with(mutate: impl FnOnce(&mut wire::batch::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    {
        let mut root = message.init_root::<wire::batch::Builder<'_>>();
        root.set_version(VERSION);
        root.set_sequence(1);
        root.set_end(End::WindowEnded as u16);
        root.set_schema_sha256(&SCHEMA_DIGEST);
        root.set_watch_epoch(&[7; 32]);
        mutate(&mut root);
    }
    serialize::write_message_to_words(&message)
}

#[test]
fn all_change_kinds_preserve_order_raw_names_and_separate_rename_halves() {
    let mut value = batch(1, End::Continue);
    value.changes = [
        Kind::Added,
        Kind::Removed,
        Kind::Modified,
        Kind::RenameOldName,
        Kind::RenameNewName,
    ]
    .into_iter()
    .map(|kind| Change {
        kind,
        name_utf16_le: name("雪 😀.txt"),
    })
    .collect();
    assert_eq!(Batch::decode(&value.encode().unwrap()).unwrap(), value);
    value.changes = vec![Change {
        kind: Kind::RenameOldName,
        name_utf16_le: name("old.txt"),
    }];
    assert_eq!(Batch::decode(&value.encode().unwrap()).unwrap(), value);
}

#[test]
fn unpaired_utf16_units_are_observations_not_lossy_strings() {
    let mut value = batch(1, End::Continue);
    value.changes[0].name_utf16_le = vec![0x00, 0xd8, 0x61, 0x00, 0xff, 0xdf];
    assert_eq!(Batch::decode(&value.encode().unwrap()).unwrap(), value);
}

#[test]
fn diagnostics_do_not_disclose_names_or_epoch() {
    let value = batch(1, End::Continue);
    assert!(!format!("{value:?}").contains("雪"));
    assert!(!format!("{:?}", value.changes[0]).contains("雪"));
    assert!(!format!("{:?}", new_client()).contains("[7, 7"));
}

#[test]
fn empty_or_zero_identifiers_and_nonterminal_max_sequence_are_refused() {
    let mut value = batch(1, End::Continue);
    value.watch_epoch = [0; 32];
    assert_eq!(value.encode(), Err(Error::Invalid));
    value.watch_epoch = [7; 32];
    value.sequence = 0;
    assert_eq!(value.encode(), Err(Error::Invalid));
    value.sequence = u64::MAX;
    assert_eq!(value.encode(), Err(Error::Invalid));
    value.end = End::WindowEnded;
    assert!(value.encode().is_ok());
}

#[test]
fn gap_revocation_cancellation_and_failure_never_carry_partial_changes() {
    for end in [
        End::Gap,
        End::Revoked,
        End::Cancelled,
        End::ObservationFailed,
    ] {
        let mut value = batch(1, end);
        assert_eq!(Batch::decode(&value.encode().unwrap()).unwrap(), value);
        value.changes = batch(1, End::Continue).changes;
        assert_eq!(value.encode(), Err(Error::Invalid));
    }
    let mut value = batch(1, End::Continue);
    value.changes.clear();
    assert_eq!(value.encode(), Err(Error::Invalid));
}

#[test]
fn single_component_names_reject_traversal_streams_separators_and_nul() {
    for text in [".", "..", "a/b", "a\\b", "C:a", "a\0b", ""] {
        let mut value = batch(1, End::Continue);
        value.changes[0].name_utf16_le = name(text);
        assert!(value.encode().is_err(), "unexpectedly accepted {text:?}");
    }
    let mut value = batch(1, End::Continue);
    value.changes[0].name_utf16_le = vec![1];
    assert_eq!(value.encode(), Err(Error::Limit));
}

#[test]
fn entry_name_and_aggregate_limits_are_enforced_before_wire_delivery() {
    let mut value = batch(1, End::Continue);
    value.changes = vec![
        Change {
            kind: Kind::Modified,
            name_utf16_le: name("x")
        };
        MAX_CHANGES
    ];
    assert_eq!(Batch::decode(&value.encode().unwrap()).unwrap(), value);
    value.changes.push(value.changes[0].clone());
    assert_eq!(value.encode(), Err(Error::Limit));
    value.changes = vec![
        Change {
            kind: Kind::Modified,
            name_utf16_le: name(&"x".repeat(MAX_NAME_BYTES / 2))
        };
        48
    ];
    assert_eq!(Batch::decode(&value.encode().unwrap()).unwrap(), value);
    value.changes.push(value.changes[0].clone());
    assert_eq!(value.encode(), Err(Error::Limit));
    value.changes.truncate(1);
    value.changes[0].name_utf16_le.extend([b'x', 0]);
    assert_eq!(value.encode(), Err(Error::Limit));
}

#[test]
fn decoder_rejects_every_truncated_prefix_and_trailing_words() {
    let wire = batch(1, End::Continue).encode().unwrap();
    for size in 0..wire.len() {
        assert!(Batch::decode(&wire[..size]).is_err());
    }
    let mut extra = wire;
    extra.extend([0; 8]);
    assert!(Batch::decode(&extra).is_err());
}

#[test]
fn segment_claims_are_bounded_before_allocation() {
    let mut wire = batch(1, End::Continue).encode().unwrap();
    wire[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(Batch::decode(&wire), Err(Error::Limit));
    wire[..4].copy_from_slice(&(MAX_SEGMENTS as u32).to_le_bytes());
    assert_eq!(Batch::decode(&wire), Err(Error::Limit));
    wire[..4].copy_from_slice(&0u32.to_le_bytes());
    wire[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(Batch::decode(&wire), Err(Error::Limit));
}

#[test]
fn wrong_contract_reserved_fields_and_unknown_end_are_refused() {
    for wire in [
        wire_with(|r| r.set_version(2)),
        wire_with(|r| r.set_schema_sha256(&[0; 32])),
    ] {
        assert_eq!(Batch::decode(&wire), Err(Error::Contract));
    }
    for wire in [
        wire_with(|r| r.set_reserved(1)),
        wire_with(|r| r.set_end(6)),
        wire_with(|r| r.set_watch_epoch(&[0; 32])),
    ] {
        assert_eq!(Batch::decode(&wire), Err(Error::Invalid));
    }
}

#[test]
fn malformed_entry_fields_are_not_normalized() {
    for variant in 0..4 {
        let wire = wire_with(|root| {
            root.set_end(End::Continue as u16);
            let mut item = root.reborrow().init_changes(1).get(0);
            item.set_kind(Kind::Added as u16);
            item.set_name_utf16_le(&name("a"));
            match variant {
                0 => item.set_kind(0),
                1 => item.set_reserved(1),
                2 => item.set_reserved2(1),
                _ => item.set_name_utf16_le(&name("../a")),
            }
        });
        assert_eq!(Batch::decode(&wire), Err(Error::Invalid));
    }
}

#[test]
fn one_pending_frame_requires_exact_ack_and_is_charged_once() {
    let mut client = new_client();
    let wire = batch(1, End::Continue).encode().unwrap();
    client.receive(&wire).unwrap();
    let usage = client.usage();
    assert_eq!(usage.frames, 1);
    assert_eq!(usage.changes, 1);
    assert_eq!(usage.wire_bytes, wire.len() as u64);
    assert_eq!(client.receive(&wire), Err(Error::Pending));
    assert_eq!(client.usage(), usage);
    assert_eq!(
        client.acknowledge(2, wire_digest(&wire)),
        Err(Error::Correlation)
    );
    assert_eq!(client.acknowledge(1, [0; 32]), Err(Error::Correlation));
    assert_eq!(client.phase(), Phase::Pending);
    client.acknowledge(1, wire_digest(&wire)).unwrap();
    assert_eq!(client.phase(), Phase::Ready);
    assert_eq!(client.acknowledge(1, wire_digest(&wire)), Err(Error::State));
}

#[test]
fn replay_wrong_epoch_and_skipped_sequence_latch_unknown_without_replacing_identity() {
    let mut client = new_client();
    let first = batch(1, End::Continue).encode().unwrap();
    client.receive(&first).unwrap();
    client.acknowledge(1, wire_digest(&first)).unwrap();
    assert_eq!(client.receive(&first), Err(Error::Correlation));
    assert_eq!(client.phase(), Phase::Unknown);
    let second = batch(2, End::Continue).encode().unwrap();
    assert_eq!(client.receive(&second), Err(Error::OutcomeUnknown));
    for mut value in [batch(2, End::Continue), batch(1, End::Continue)] {
        if value.sequence == 1 {
            value.watch_epoch = [8; 32];
        }
        let mut client = new_client();
        assert_eq!(
            client.receive(&value.encode().unwrap()),
            Err(Error::Correlation)
        );
        assert_eq!(client.phase(), Phase::Unknown);
        assert_eq!(client.usage().frames, 0);
    }
}

#[test]
fn missing_delivery_keeps_charge_and_cannot_be_repaired_by_late_ack() {
    let mut client = new_client();
    let wire = batch(1, End::Continue).encode().unwrap();
    client.receive(&wire).unwrap();
    let usage = client.usage();
    client.transport_lost();
    assert_eq!(
        client.acknowledge(1, wire_digest(&wire)),
        Err(Error::OutcomeUnknown)
    );
    assert_eq!(client.usage(), usage);
    assert_eq!(client.receive(&wire), Err(Error::OutcomeUnknown));
}

#[test]
fn terminal_observation_requires_ack_and_does_not_reopen_on_transport_loss() {
    for end in [
        End::WindowEnded,
        End::Gap,
        End::Revoked,
        End::Cancelled,
        End::ObservationFailed,
    ] {
        let mut client = new_client();
        let wire = batch(1, end).encode().unwrap();
        client.receive(&wire).unwrap();
        assert_eq!(client.phase(), Phase::Pending);
        client.acknowledge(1, wire_digest(&wire)).unwrap();
        assert_eq!(client.phase(), Phase::Ended(end));
        client.transport_lost();
        assert_eq!(client.phase(), Phase::Ended(end));
        assert_eq!(client.receive(&wire), Err(Error::State));
        assert_eq!(client.usage().frames, 1);
    }
}

#[test]
fn each_cumulative_budget_is_enforced_without_refunding_prior_delivery() {
    let first = batch(1, End::Continue).encode().unwrap();
    let second = batch(2, End::Continue).encode().unwrap();
    for budget in [
        Limits {
            frames: 1,
            ..limits()
        },
        Limits {
            changes: 1,
            ..limits()
        },
        Limits {
            wire_bytes: first.len() as u64,
            ..limits()
        },
    ] {
        let mut client = ClientState::new([7; 32], budget).unwrap();
        client.receive(&first).unwrap();
        client.acknowledge(1, wire_digest(&first)).unwrap();
        let usage = client.usage();
        assert_eq!(client.receive(&second), Err(Error::Budget));
        assert_eq!(client.phase(), Phase::Unknown);
        assert_eq!(client.usage(), usage);
        assert_eq!(client.receive(&first), Err(Error::OutcomeUnknown));
    }
}

#[test]
fn malformed_input_latches_unknown_without_claiming_any_resource_return() {
    let mut client = new_client();
    assert_eq!(client.receive(b"{}"), Err(Error::Limit));
    assert_eq!(client.phase(), Phase::Unknown);
    assert_eq!(client.usage().frames, 0);
    assert_eq!(
        client.receive(&batch(1, End::Continue).encode().unwrap()),
        Err(Error::OutcomeUnknown)
    );
}

#[test]
fn invalid_limits_and_zero_epoch_do_not_construct_a_client() {
    for value in [
        Limits {
            frames: 0,
            ..limits()
        },
        Limits {
            frames: state::MAX_FRAMES + 1,
            ..limits()
        },
        Limits {
            changes: state::MAX_TOTAL_CHANGES + 1,
            ..limits()
        },
        Limits {
            wire_bytes: 0,
            ..limits()
        },
        Limits {
            wire_bytes: state::MAX_TOTAL_BYTES + 1,
            ..limits()
        },
    ] {
        assert!(matches!(
            ClientState::new([7; 32], value),
            Err(Error::Limit)
        ));
    }
    assert!(matches!(
        ClientState::new([0; 32], limits()),
        Err(Error::Invalid)
    ));
}
