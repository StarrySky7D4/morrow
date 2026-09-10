use prost::Message;

#[derive(Clone, PartialEq, Message)]
struct OldRecord {
    #[prost(uint32, tag = "1")]
    id: u32,
}
#[derive(Clone, PartialEq, Message)]
struct EvidenceEnvelope {
    #[prost(bytes = "vec", tag = "1")]
    original: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct EnumRecord {
    #[prost(enumeration = "KnownMode", tag = "1")]
    mode: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
enum KnownMode {
    Default = 0,
}

fn main() {
    // Synthetic v2 field 2 is unknown to OldRecord.
    let newer = vec![0x08, 1, 0x12, 3, b'n', b'e', b'w'];
    let old = OldRecord::decode(newer.as_slice()).unwrap();
    assert_eq!(old.id, 1);
    assert_eq!(old.encode_to_vec(), vec![0x08, 1]);
    println!("CONFIRMED: prost 0.14.4 drops this unknown field on typed round-trip");

    let unknown_enum = EnumRecord::decode(&[0x08, 42][..]).unwrap();
    assert_eq!(unknown_enum.mode, 42);
    assert_eq!(unknown_enum.encode_to_vec(), vec![0x08, 42]);
    println!("CONFIRMED: unknown enum integer survives; this is a different guarantee");

    let repeated = vec![0x08, 1, 0x08, 2];
    let parsed = OldRecord::decode(repeated.as_slice()).unwrap();
    assert_eq!(parsed.id, 2);
    assert_ne!(parsed.encode_to_vec(), repeated);
    println!("CONFIRMED: valid original bytes can differ from re-encoded bytes");

    let stored = EvidenceEnvelope {
        original: newer.clone(),
    }
    .encode_to_vec();
    assert_eq!(
        EvidenceEnvelope::decode(stored.as_slice())
            .unwrap()
            .original,
        newer
    );
    println!(
        "CONFIRMED: opaque payload preserves exact bytes; outer unknown fields remain unprotected"
    );
    println!("4 characterization checks passed; NOT a persistence compatibility approval");
}
