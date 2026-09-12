use morrow_core::{
    Error,
    shared_object::{
        Descriptor, MAX_MESSAGE_BYTES, MAX_OBJECT_BYTES, MAX_SEGMENTS, Segment, VERSION,
        schema_digest,
    },
    shared_object_capnp as wire,
};
fn descriptor() -> Descriptor {
    Descriptor {
        arena: 1,
        object: 2,
        generation: 3,
        length: 9,
        sha256: [7; 32],
        segments: vec![
            Segment {
                offset: 0,
                length: 4,
            },
            Segment {
                offset: 4,
                length: 5,
            },
        ],
    }
}
fn raw(d: &Descriptor, version: u16, digest: &[u8], hash: &[u8]) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut r = message.init_root::<wire::descriptor::Builder>();
    r.set_version(version);
    r.set_schema_digest(digest);
    r.set_arena(d.arena);
    r.set_object(d.object);
    r.set_generation(d.generation);
    r.set_length(d.length);
    r.set_sha256(hash);
    let mut s = r.init_segments(d.segments.len() as u32);
    for (i, segment) in d.segments.iter().enumerate() {
        let mut v = s.reborrow().get(i as u32);
        v.set_offset(segment.offset);
        v.set_length(segment.length);
    }
    capnp::serialize::write_message_to_words(&message)
}
fn rejected_both(d: &Descriptor) {
    assert!(d.validate().is_err());
    assert!(d.encode().is_err());
    assert!(Descriptor::decode(&raw(d, VERSION, &schema_digest(), &d.sha256)).is_err());
}
#[test]
fn round_trip_preserves_u64_identity_hash_and_segment_layout() {
    let mut d = descriptor();
    d.arena = u64::MAX;
    d.object = u64::MAX - 1;
    d.generation = u64::MAX - 2;
    let encoded = d.encode().unwrap();
    assert_eq!(encoded, raw(&d, VERSION, &schema_digest(), &d.sha256));
    assert_eq!(Descriptor::decode(&encoded).unwrap(), d);
    assert!(encoded.len() <= MAX_MESSAGE_BYTES);
    // Shape validation intentionally cannot prove the claim's hash matches actual stored bytes.
    d.sha256 = [0; 32];
    assert_eq!(Descriptor::decode(&d.encode().unwrap()).unwrap(), d);
}
#[test]
fn minimum_and_maximum_objects_and_all_64_segments_are_supported() {
    for length in [1, MAX_OBJECT_BYTES] {
        let mut d = descriptor();
        d.length = length;
        d.segments = vec![Segment { offset: 0, length }];
        assert_eq!(Descriptor::decode(&d.encode().unwrap()).unwrap(), d);
    }
    let mut d = descriptor();
    d.length = MAX_OBJECT_BYTES;
    let size = MAX_OBJECT_BYTES / MAX_SEGMENTS as u64;
    d.segments = (0..MAX_SEGMENTS)
        .map(|i| Segment {
            offset: i as u64 * size,
            length: size,
        })
        .collect();
    assert_eq!(Descriptor::decode(&d.encode().unwrap()).unwrap(), d);
}
#[test]
fn zero_identities_empty_oversize_and_excess_segments_fail_on_both_paths() {
    for field in 0..3 {
        let mut d = descriptor();
        match field {
            0 => d.arena = 0,
            1 => d.object = 0,
            _ => d.generation = 0,
        };
        rejected_both(&d);
    }
    for length in [0, MAX_OBJECT_BYTES + 1, u64::MAX] {
        let mut d = descriptor();
        d.length = length;
        d.segments = vec![Segment { offset: 0, length }];
        rejected_both(&d);
    }
    let mut d = descriptor();
    d.segments.clear();
    rejected_both(&d);
    d.length = 65;
    d.segments = (0..65)
        .map(|i| Segment {
            offset: i,
            length: 1,
        })
        .collect();
    rejected_both(&d);
}
#[test]
fn gaps_overlap_reordering_zero_length_and_partial_coverage_fail() {
    for segments in [
        vec![Segment {
            offset: 1,
            length: 8,
        }],
        vec![
            Segment {
                offset: 0,
                length: 4,
            },
            Segment {
                offset: 5,
                length: 4,
            },
        ],
        vec![
            Segment {
                offset: 0,
                length: 5,
            },
            Segment {
                offset: 4,
                length: 5,
            },
        ],
        vec![
            Segment {
                offset: 4,
                length: 5,
            },
            Segment {
                offset: 0,
                length: 4,
            },
        ],
        vec![
            Segment {
                offset: 0,
                length: 0,
            },
            Segment {
                offset: 0,
                length: 9,
            },
        ],
        vec![Segment {
            offset: 0,
            length: 8,
        }],
        vec![Segment {
            offset: 0,
            length: 10,
        }],
    ] {
        let mut d = descriptor();
        d.segments = segments;
        rejected_both(&d);
    }
}
#[test]
fn checked_segment_addition_rejects_wraparound() {
    let mut d = descriptor();
    d.segments = vec![Segment {
        offset: u64::MAX,
        length: 1,
    }];
    assert_eq!(d.validate(), Err(Error::Limit));
    rejected_both(&d);
    d.segments = vec![
        Segment {
            offset: 0,
            length: 1,
        },
        Segment {
            offset: 1,
            length: u64::MAX,
        },
    ];
    assert_eq!(d.validate(), Err(Error::Limit));
    rejected_both(&d);
}
#[test]
fn mismatched_version_schema_and_content_hash_length_are_rejected() {
    let d = descriptor();
    for version in [0, VERSION + 1, u16::MAX] {
        assert_eq!(
            Descriptor::decode(&raw(&d, version, &schema_digest(), &d.sha256)),
            Err(Error::UnsupportedVersion)
        );
    }
    for schema in [vec![], vec![0; 31], vec![0; 32], vec![0; 33]] {
        assert_eq!(
            Descriptor::decode(&raw(&d, VERSION, &schema, &d.sha256)),
            Err(Error::UnsupportedVersion)
        );
    }
    for hash in [vec![], vec![0; 31], vec![0; 33]] {
        assert!(Descriptor::decode(&raw(&d, VERSION, &schema_digest(), &hash)).is_err());
    }
}
#[test]
fn truncation_trailing_messages_and_oversized_frames_are_rejected() {
    let valid = descriptor().encode().unwrap();
    for end in 0..valid.len() {
        assert!(
            Descriptor::decode(&valid[..end]).is_err(),
            "accepted truncated length {end}"
        );
    }
    let mut trailing = valid.clone();
    trailing.extend([0; 8]);
    assert_eq!(
        Descriptor::decode(&trailing),
        Err(Error::Invalid("trailing shared object bytes"))
    );
    let mut joined = valid.clone();
    joined.extend(valid);
    assert!(Descriptor::decode(&joined).is_err());
    assert_eq!(
        Descriptor::decode(&vec![0; MAX_MESSAGE_BYTES + 1]),
        Err(Error::Limit)
    );
    for bytes in [
        vec![255; 8],
        vec![0; 8],
        vec![255; 256],
        vec![0; MAX_MESSAGE_BYTES],
    ] {
        assert!(Descriptor::decode(&bytes).is_err());
    }
}
#[test]
fn all_identity_and_layout_variants_remain_descriptions_not_access_handles() {
    let base = descriptor();
    for value in 1..128 {
        let mut d = base.clone();
        d.object = value;
        d.generation = value;
        d.sha256 = [value as u8; 32];
        assert_eq!(Descriptor::decode(&d.encode().unwrap()).unwrap(), d);
    }
    // No catalog lookup, native mapping, file access, or grant creation is performed by the codec.
}

#[test]
fn byte_constructor_computes_sha256_and_checks_limits_before_hashing() {
    let d = Descriptor::for_bytes(1, 2, 3, b"abc").unwrap();
    assert_eq!(d.length, 3);
    assert_eq!(
        d.segments,
        vec![Segment {
            offset: 0,
            length: 3
        }]
    );
    assert_eq!(
        d.sha256,
        [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad
        ]
    );
    assert_eq!(Descriptor::decode(&d.encode().unwrap()).unwrap(), d);
    assert_eq!(Descriptor::for_bytes(1, 2, 3, b""), Err(Error::Limit));
    assert_eq!(
        Descriptor::for_bytes(1, 2, 3, &vec![0; MAX_OBJECT_BYTES as usize + 1]),
        Err(Error::Limit)
    );
    assert_eq!(
        Descriptor::for_bytes(0, 2, 3, b"abc"),
        Err(Error::Invalid("shared object identity"))
    );
}

#[test]
fn descriptor_decode_accepts_unaligned_transport_slices() {
    let d = descriptor();
    let encoded = d.encode().unwrap();
    for padding in 1..8 {
        let mut framed = vec![0; padding];
        framed.extend_from_slice(&encoded);
        assert_eq!(Descriptor::decode(&framed[padding..]).unwrap(), d);
    }
}
