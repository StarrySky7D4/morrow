use capnp::{message::Builder, serialize};
use morrow_core::{
    Error,
    shared_object::{Descriptor, MAX_OBJECT_BYTES, Segment},
    shared_transfer::{MAX_FRAME_BYTES, MAX_PAYLOAD_BYTES, Offer, Reply, VERSION, schema_digest},
    shared_transfer_capnp as wire,
};
fn offer() -> Offer {
    Offer {
        transfer: 1,
        remote_handle: 2,
        descriptor: Descriptor::for_bytes(3, 4, 5, b"test payload").unwrap(),
    }
}
fn reply() -> Reply {
    let o = offer();
    Reply {
        transfer: o.transfer,
        descriptor: o.descriptor,
        payload: b"test payload".to_vec(),
        write_rejected: true,
    }
}
fn frame(
    o: Option<&Offer>,
    r: Option<&Reply>,
    descriptor: &[u8],
) -> Builder<capnp::message::HeapAllocator> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<wire::message::Builder>();
    root.set_version(VERSION);
    root.set_schema_digest(&schema_digest());
    if let Some(o) = o {
        let mut v = root.init_offer();
        v.set_transfer(o.transfer);
        v.set_remote_handle(o.remote_handle);
        v.set_descriptor(descriptor);
    } else {
        let r = r.unwrap();
        let mut v = root.init_reply();
        v.set_transfer(r.transfer);
        v.set_descriptor(descriptor);
        v.set_payload(&r.payload);
        v.set_write_rejected(r.write_rejected);
    }
    message
}
fn descriptor_raw(d: &Descriptor) -> Vec<u8> {
    use morrow_core::{shared_object, shared_object_capnp as dw};
    let mut message = Builder::new_default();
    let mut r = message.init_root::<dw::descriptor::Builder>();
    r.set_version(shared_object::VERSION);
    r.set_schema_digest(&shared_object::schema_digest());
    r.set_arena(d.arena);
    r.set_object(d.object);
    r.set_generation(d.generation);
    r.set_length(d.length);
    r.set_sha256(&d.sha256);
    let mut segments = r.init_segments(d.segments.len() as u32);
    for (i, s) in d.segments.iter().enumerate() {
        let mut v = segments.reborrow().get(i as u32);
        v.set_offset(s.offset);
        v.set_length(s.length);
    }
    serialize::write_message_to_words(&message)
}
#[test]
fn offer_and_reply_round_trip_full_ids_bytes_and_probe_outcome() {
    let mut o = offer();
    o.transfer = u64::MAX;
    o.remote_handle = u64::MAX - 1;
    o.descriptor.arena = u64::MAX - 2;
    assert_eq!(Offer::decode(&o.encode().unwrap()).unwrap(), o);
    for observed in [false, true] {
        let mut r = reply();
        r.transfer = u64::MAX;
        r.write_rejected = observed;
        assert_eq!(Reply::decode(&r.encode().unwrap()).unwrap(), r);
    }
}
#[test]
fn full_offer_size_and_reply_copy_bound_are_distinct() {
    let mut o = offer();
    o.descriptor.length = MAX_OBJECT_BYTES;
    o.descriptor.segments = vec![Segment {
        offset: 0,
        length: MAX_OBJECT_BYTES,
    }];
    assert_eq!(Offer::decode(&o.encode().unwrap()).unwrap(), o);
    for size in [1, MAX_PAYLOAD_BYTES] {
        let payload = (0..size).map(|i| i as u8).collect::<Vec<_>>();
        let r = Reply {
            transfer: 1,
            descriptor: Descriptor::for_bytes(1, 2, 3, &payload).unwrap(),
            payload,
            write_rejected: true,
        };
        assert_eq!(Reply::decode(&r.encode().unwrap()).unwrap(), r);
        assert!(r.encode().unwrap().len() < MAX_FRAME_BYTES);
    }
}
#[test]
fn zero_transfer_handle_and_invalid_nested_descriptor_are_rejected() {
    let mut o = offer();
    let d = o.descriptor.encode().unwrap();
    o.transfer = 0;
    assert!(o.encode().is_err());
    assert!(
        Offer::decode(&serialize::write_message_to_words(&frame(
            Some(&o),
            None,
            &d
        )))
        .is_err()
    );
    o.transfer = 1;
    o.remote_handle = 0;
    assert!(o.encode().is_err());
    assert!(
        Offer::decode(&serialize::write_message_to_words(&frame(
            Some(&o),
            None,
            &d
        )))
        .is_err()
    );
    let mut r = reply();
    r.transfer = 0;
    assert!(r.encode().is_err());
    assert!(
        Reply::decode(&serialize::write_message_to_words(&frame(
            None,
            Some(&r),
            &d
        )))
        .is_err()
    );
    let o = offer();
    let r = reply();
    let mut bad = o.descriptor.clone();
    bad.generation = 0;
    let invalid_identity = descriptor_raw(&bad);
    bad.generation = 1;
    bad.segments = vec![Segment {
        offset: u64::MAX,
        length: 2,
    }];
    let overflow = descriptor_raw(&bad);
    let mut trailing = d.clone();
    trailing.extend([0; 8]);
    for nested in [
        invalid_identity,
        overflow,
        trailing,
        vec![],
        vec![0; morrow_limit()],
    ] {
        assert!(
            Offer::decode(&serialize::write_message_to_words(&frame(
                Some(&o),
                None,
                &nested
            )))
            .is_err()
        );
        assert!(
            Reply::decode(&serialize::write_message_to_words(&frame(
                None,
                Some(&r),
                &nested
            )))
            .is_err()
        );
    }
}
fn morrow_limit() -> usize {
    morrow_core::shared_object::MAX_MESSAGE_BYTES + 1
}
#[test]
fn empty_length_mismatch_oversize_and_wrong_payload_digest_are_rejected() {
    for mode in 0..4 {
        let mut r = reply();
        match mode {
            0 => r.payload.clear(),
            1 => r.payload.push(1),
            2 => {
                r.payload = vec![1; MAX_PAYLOAD_BYTES + 1];
                r.descriptor = Descriptor::for_bytes(1, 2, 3, &r.payload).unwrap();
            }
            _ => r.payload[0] ^= 1,
        };
        assert!(r.validate().is_err());
        assert!(r.encode().is_err());
        assert!(
            Reply::decode(&serialize::write_message_to_words(&frame(
                None,
                Some(&r),
                &r.descriptor.encode().unwrap()
            )))
            .is_err()
        );
    }
    let mut r = reply();
    r.descriptor.sha256 = [0; 32];
    assert_eq!(r.validate(), Err(Error::Integrity));
}
#[test]
fn offer_and_reply_kinds_cannot_be_substituted() {
    assert_eq!(
        Offer::decode(&reply().encode().unwrap()),
        Err(Error::Invalid("expected shared transfer offer"))
    );
    assert_eq!(
        Reply::decode(&offer().encode().unwrap()),
        Err(Error::Invalid("expected shared transfer reply"))
    );
}
#[test]
fn wrong_outer_version_and_schema_digest_are_rejected() {
    let o = offer();
    let r = reply();
    let d = o.descriptor.encode().unwrap();
    for is_offer in [true, false] {
        for version in [0, VERSION + 1, u16::MAX] {
            let mut message = frame(is_offer.then_some(&o), (!is_offer).then_some(&r), &d);
            message
                .get_root::<wire::message::Builder>()
                .unwrap()
                .set_version(version);
            let bytes = serialize::write_message_to_words(&message);
            if is_offer {
                assert_eq!(Offer::decode(&bytes), Err(Error::UnsupportedVersion));
            } else {
                assert_eq!(Reply::decode(&bytes), Err(Error::UnsupportedVersion));
            }
        }
        for digest in [vec![], vec![0; 31], vec![0; 32], vec![0; 33]] {
            let mut message = frame(is_offer.then_some(&o), (!is_offer).then_some(&r), &d);
            message
                .get_root::<wire::message::Builder>()
                .unwrap()
                .set_schema_digest(&digest);
            let bytes = serialize::write_message_to_words(&message);
            if is_offer {
                assert_eq!(Offer::decode(&bytes), Err(Error::UnsupportedVersion));
            } else {
                assert_eq!(Reply::decode(&bytes), Err(Error::UnsupportedVersion));
            }
        }
    }
}
#[test]
fn truncated_trailing_and_oversized_frames_never_decode() {
    for bytes in [offer().encode().unwrap(), reply().encode().unwrap()] {
        for end in 0..bytes.len() {
            assert!(Offer::decode(&bytes[..end]).is_err());
            assert!(Reply::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.extend([0; 8]);
        assert!(Offer::decode(&trailing).is_err());
        assert!(Reply::decode(&trailing).is_err());
        let mut joined = bytes.clone();
        joined.extend(bytes);
        assert!(Offer::decode(&joined).is_err());
        assert!(Reply::decode(&joined).is_err());
    }
    let bytes = vec![0; MAX_FRAME_BYTES + 1];
    assert_eq!(Offer::decode(&bytes), Err(Error::Limit));
    assert_eq!(Reply::decode(&bytes), Err(Error::Limit));
    for bytes in [
        vec![255; 8],
        vec![0; 8],
        vec![255; 256],
        vec![0; MAX_FRAME_BYTES],
    ] {
        assert!(Offer::decode(&bytes).is_err());
        assert!(Reply::decode(&bytes).is_err());
    }
}
#[test]
fn unaligned_offer_and_reply_transport_frames_are_supported() {
    let o = offer();
    let r = reply();
    for padding in 1..8 {
        let mut bytes = vec![0; padding];
        bytes.extend(o.encode().unwrap());
        assert_eq!(Offer::decode(&bytes[padding..]).unwrap(), o);
        let mut bytes = vec![0; padding];
        bytes.extend(r.encode().unwrap());
        assert_eq!(Reply::decode(&bytes[padding..]).unwrap(), r);
    }
}
#[test]
fn standalone_reply_validation_does_not_claim_offer_correlation() {
    let mut r = reply();
    r.transfer = 99;
    r.descriptor.generation = 77;
    // The actual parent must bind these claims to its intended child, Offer and live object lease.
    assert_eq!(Reply::decode(&r.encode().unwrap()).unwrap(), r);
    assert_ne!(r.transfer, offer().transfer);
    assert_ne!(r.descriptor, offer().descriptor);
}
