use super::*;

fn proposal() -> Proposal {
    Proposal::new("上传样本 😀", 150017, [9; 32], b"producer configuration").unwrap()
}
fn identity() -> Identity {
    Identity {
        transfer_epoch: [1; 32],
        object_ref: [2; 32],
        operation_id: [3; 32],
    }
}
fn altered_proposal(change: impl FnOnce(wire::proposal::Builder<'_>)) -> Vec<u8> {
    let p = proposal();
    let mut message = builder();
    let mut root = message.init_root::<wire::proposal::Builder>();
    root.set_version(VERSION);
    root.set_schema_sha256(&SCHEMA_DIGEST);
    root.set_label(p.label());
    root.set_byte_length(p.byte_length());
    root.set_whole_sha256(&p.whole_sha256());
    root.set_producer_input(p.producer_input());
    change(root);
    serialize::write_message_to_words(&message)
}
fn altered_bound(change: impl FnOnce(wire::bound_input::Builder<'_>)) -> Vec<u8> {
    let p = proposal();
    let descriptor = Frame {
        identity: identity(),
        action: Action::Descriptor {
            total_length: p.byte_length(),
            whole_sha256: p.whole_sha256(),
        },
    }
    .encode()
    .unwrap();
    let mut message = builder();
    let mut root = message.init_root::<wire::bound_input::Builder>();
    root.set_version(VERSION);
    root.set_schema_sha256(&SCHEMA_DIGEST);
    root.set_frame_schema_sha256(&morrow_blob_transfer_v1::SCHEMA_DIGEST);
    root.set_proposal_digest(&p.digest());
    root.set_proposal(p.wire());
    root.set_descriptor(&descriptor);
    change(root);
    serialize::write_message_to_words(&message)
}

#[test]
fn immutable_proposal_round_trip_owns_bounded_input() {
    let mut input = vec![1, 2, 3];
    let value = Proposal::new("Name", 7, [4; 32], &input).unwrap();
    input.fill(8);
    assert_eq!(value.producer_input(), [1, 2, 3]);
    assert_eq!(Proposal::decode(value.wire()).unwrap(), value);
    assert_eq!(value.digest(), digest(value.wire()));
    assert_eq!(value.label(), "Name");
    assert_eq!(value.byte_length(), 7);
    assert_eq!(value.whole_sha256(), [4; 32]);
}
#[test]
fn unicode_labels_and_empty_input_are_preserved() {
    for label in ["中文标题 😀", "تقرير", "Résumé", "家族 👨‍👩‍👧", "a  b"]
    {
        let p = Proposal::new(label, 1, [3; 32], &[]).unwrap();
        let decoded = Proposal::decode(p.wire()).unwrap();
        assert_eq!(decoded.label(), label);
        assert!(decoded.producer_input().is_empty());
    }
}
#[test]
fn label_control_bidi_and_ambiguous_outer_whitespace_refuse() {
    for label in [
        "",
        " ",
        " leading",
        "trailing ",
        "x\nnext",
        "x\0tail",
        "x\u{7f}tail",
        "x\u{202e}tail",
        "x\u{2066}tail",
        "x\u{061c}tail",
        "x\u{feff}tail",
        "x\u{2028}tail",
        "x\u{200e}tail",
    ] {
        assert_eq!(
            Proposal::new(label, 1, [3; 32], &[]).unwrap_err(),
            Error::Invalid,
            "{label:?}"
        );
    }
}
#[test]
fn maxima_fit_and_each_oversized_field_refuses_before_encoding() {
    let label = "😀".repeat(MAX_LABEL_CHARS);
    assert_eq!(label.len(), MAX_LABEL_BYTES);
    let p = Proposal::new(
        &label,
        MAX_OBJECT_BYTES,
        [3; 32],
        &vec![42; MAX_PRODUCER_INPUT_BYTES],
    )
    .unwrap();
    assert!(p.wire().len() <= MAX_PROPOSAL_BYTES);
    assert_eq!(Proposal::decode(p.wire()).unwrap(), p);
    let bound = BoundInput::new(&p, identity()).unwrap();
    assert!(bound.wire().len() <= MAX_BOUND_INPUT_BYTES);
    assert_eq!(BoundInput::decode(bound.wire()).unwrap().proposal(), &p);
    for (label, length, input) in [
        ("a".repeat(MAX_LABEL_CHARS + 1), 1, vec![]),
        ("😀".repeat(MAX_LABEL_CHARS + 1), 1, vec![]),
        ("Name".into(), MAX_OBJECT_BYTES + 1, vec![]),
        ("Name".into(), 1, vec![0; MAX_PRODUCER_INPUT_BYTES + 1]),
    ] {
        assert_eq!(
            Proposal::new(&label, length, [3; 32], &input).unwrap_err(),
            Error::Limit
        );
    }
}
#[test]
fn empty_object_requires_exact_empty_digest() {
    let p = Proposal::new("Empty", 0, digest(&[]), &[]).unwrap();
    assert_eq!(Proposal::decode(p.wire()).unwrap(), p);
    assert!(BoundInput::decode(BoundInput::new(&p, identity()).unwrap().wire()).is_ok());
    assert_eq!(
        Proposal::new("Empty", 0, [0; 32], &[]).unwrap_err(),
        Error::Invalid
    );
    assert_eq!(
        Proposal::decode(&altered_proposal(|mut r| r.set_byte_length(0))).unwrap_err(),
        Error::Invalid
    );
}
#[test]
fn proposal_contract_and_reserved_fields_are_exact() {
    for bytes in [
        altered_proposal(|mut r| r.set_version(2)),
        altered_proposal(|mut r| r.set_schema_sha256(&[0; 32])),
    ] {
        assert_eq!(Proposal::decode(&bytes).unwrap_err(), Error::Contract);
    }
    for bytes in [
        altered_proposal(|mut r| r.set_reserved(1)),
        altered_proposal(|mut r| r.set_reserved2(1)),
        altered_proposal(|mut r| r.set_whole_sha256(&[9; 31])),
        altered_proposal(|mut r| r.set_label("bad\nlabel")),
    ] {
        assert_eq!(Proposal::decode(&bytes).unwrap_err(), Error::Invalid);
    }
    assert_eq!(
        Proposal::decode(&altered_proposal(
            |mut r| r.set_byte_length(MAX_OBJECT_BYTES + 1)
        ))
        .unwrap_err(),
        Error::Limit
    );
}
#[test]
fn extra_fields_aliases_padding_and_capability_pointers_refuse() {
    let p = proposal();
    let mut extra = p.wire().to_vec();
    // Increase declared root pointer count without granting an optional field.
    let size = <wire::proposal::Builder as HasStructSize>::STRUCT_SIZE;
    assert_eq!((size.data, size.pointers), (2, 4));
    extra[14..16].copy_from_slice(&(size.pointers + 1).to_le_bytes());
    assert!(Proposal::decode(&extra).is_err());
    let mut unused = p.wire().to_vec();
    let words = u32::from_le_bytes(unused[4..8].try_into().unwrap());
    unused[4..8].copy_from_slice(&(words + 1).to_le_bytes());
    unused.extend_from_slice(&[0; 8]);
    assert_eq!(Proposal::decode(&unused).unwrap_err(), Error::Invalid);
    let mut capability = p.wire().to_vec();
    capability[32..40].copy_from_slice(&3_u64.to_le_bytes());
    assert!(Proposal::decode(&capability).is_err());
    let mut alias = p.wire().to_vec();
    // producerInput aliases the schema byte-list with a relative pointer.
    let pointer = u64::from_le_bytes(alias[32..40].try_into().unwrap());
    let offset = ((pointer as u32) >> 2).wrapping_sub(3);
    let replacement = (pointer & 0xffff_ffff_0000_0000) | u64::from((offset << 2) | 1);
    alias[56..64].copy_from_slice(&replacement.to_le_bytes());
    assert_eq!(Proposal::decode(&alias).unwrap_err(), Error::Invalid);
}
#[test]
fn full_input_envelope_binds_exact_proposal_and_descriptor() {
    let p = proposal();
    let bound = BoundInput::new(&p, identity()).unwrap();
    let decoded = BoundInput::decode(bound.wire()).unwrap();
    assert_eq!(decoded.proposal(), &p);
    assert_eq!(decoded.identity().transfer_epoch, identity().transfer_epoch);
    assert_eq!(decoded.identity().object_ref, identity().object_ref);
    assert_eq!(decoded.identity().operation_id, identity().operation_id);
    assert_eq!(bound.wire(), decoded.wire());
}
#[test]
fn frame_identities_must_be_valid_but_are_not_approval() {
    let p = proposal();
    for i in 0..3 {
        let mut id = identity();
        match i {
            0 => id.transfer_epoch = [0; 32],
            1 => id.object_ref = [0; 32],
            _ => id.operation_id = [0; 32],
        }
        assert!(BoundInput::new(&p, id).is_err());
    }
    let a = BoundInput::new(&p, identity()).unwrap();
    let mut id = identity();
    id.operation_id = [8; 32];
    let b = BoundInput::new(&p, id).unwrap();
    assert_ne!(a.wire(), b.wire()); // Host approval must still bind these exact names.
}
#[test]
fn changed_proposal_digest_size_or_sha_cannot_be_bound() {
    assert_eq!(
        BoundInput::decode(&altered_bound(|mut r| r.set_proposal_digest(&[0; 32]))).unwrap_err(),
        Error::Correlation
    );
    for action in [
        Action::Descriptor {
            total_length: 150018,
            whole_sha256: [9; 32],
        },
        Action::Descriptor {
            total_length: 150017,
            whole_sha256: [8; 32],
        },
        Action::End {
            total_length: 150017,
            whole_sha256: [9; 32],
        },
    ] {
        let descriptor = Frame {
            identity: identity(),
            action,
        }
        .encode()
        .unwrap();
        let raw = altered_bound(|mut r| r.set_descriptor(&descriptor));
        assert_eq!(BoundInput::decode(&raw).unwrap_err(), Error::Correlation);
    }
}
#[test]
fn changed_producer_input_changes_exact_digest_without_renewing_authority() {
    let p = proposal();
    let changed =
        Proposal::new(p.label(), p.byte_length(), p.whole_sha256(), b"different").unwrap();
    assert_ne!(p.digest(), changed.digest());
    let raw = altered_bound(|mut r| r.set_proposal(changed.wire()));
    assert_eq!(BoundInput::decode(&raw).unwrap_err(), Error::Correlation);
}
#[test]
fn bound_contract_reserved_fields_and_embedded_limit_are_checked() {
    for bytes in [
        altered_bound(|mut r| r.set_version(2)),
        altered_bound(|mut r| r.set_schema_sha256(&[0; 32])),
        altered_bound(|mut r| r.set_frame_schema_sha256(&[0; 32])),
    ] {
        assert_eq!(BoundInput::decode(&bytes).unwrap_err(), Error::Contract);
    }
    for bytes in [
        altered_bound(|mut r| r.set_reserved(1)),
        altered_bound(|mut r| r.set_reserved2(1)),
    ] {
        assert_eq!(BoundInput::decode(&bytes).unwrap_err(), Error::Invalid);
    }
    assert_eq!(
        BoundInput::decode(&altered_bound(|mut r| r.set_descriptor(&[0; 1025]))).unwrap_err(),
        Error::Limit
    );
}
#[test]
fn truncation_trailing_messages_and_oversized_wire_are_rejected() {
    let p = proposal();
    let b = BoundInput::new(&p, identity()).unwrap();
    for end in 0..p.wire().len() {
        assert!(
            Proposal::decode(&p.wire()[..end]).is_err(),
            "proposal {end}"
        );
    }
    for end in 0..b.wire().len() {
        assert!(BoundInput::decode(&b.wire()[..end]).is_err(), "bound {end}");
    }
    let mut p2 = p.wire().to_vec();
    p2.extend_from_slice(p.wire());
    let mut b2 = b.wire().to_vec();
    b2.extend_from_slice(b.wire());
    assert!(Proposal::decode(&p2).is_err());
    assert!(BoundInput::decode(&b2).is_err());
    assert_eq!(
        Proposal::decode(&vec![0; MAX_PROPOSAL_BYTES + 1]).unwrap_err(),
        Error::Limit
    );
    assert_eq!(
        BoundInput::decode(&vec![0; MAX_BOUND_INPUT_BYTES + 1]).unwrap_err(),
        Error::Limit
    );
}
#[test]
fn malformed_pointer_bytes_do_not_panic_or_allocate_unbounded() {
    let p = proposal();
    let b = BoundInput::new(&p, identity()).unwrap();
    for position in 0..64 {
        for value in [0, 3, 127, 255] {
            let mut bytes = p.wire().to_vec();
            bytes[position] = value;
            let _ = Proposal::decode(&bytes);
            let mut bytes = b.wire().to_vec();
            bytes[position] = value;
            let _ = BoundInput::decode(&bytes);
        }
    }
}
#[test]
fn debug_output_does_not_expose_producer_bytes_or_labels() {
    let p = proposal();
    let b = BoundInput::new(&p, identity()).unwrap();
    for debug in [format!("{p:?}"), format!("{b:?}")] {
        assert!(!debug.contains("configuration"));
        assert!(!debug.contains(p.label()));
    }
}
