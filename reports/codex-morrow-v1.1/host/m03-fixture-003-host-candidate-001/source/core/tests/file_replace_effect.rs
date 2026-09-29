use morrow_core::{
    Error,
    file_content::MAX_CONTENT_BYTES,
    file_effect::{self, CreateOutcome, CreateResult, ReplaceOutcome, ReplaceResult, proto},
};
use prost::Message;
use sha2::{Digest, Sha256};

fn outcome(result: ReplaceResult, length: u64) -> ReplaceOutcome {
    ReplaceOutcome::new(
        "replace-operation",
        "plugin.replace",
        [1; 32],
        [2; 32],
        [3; 32],
        if length == 0 {
            <[u8; 32]>::from(Sha256::digest([]))
        } else {
            [4; 32]
        },
        length,
        result,
    )
    .unwrap()
}

fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut bytes = b"MROWFER1".to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend_from_slice(&compressed);
    bytes
}

#[test]
fn roundtrip_binds_every_replace_field_and_separate_magic() {
    for (result, length) in [
        (ReplaceResult::Replaced, 17),
        (ReplaceResult::OsRejected { code: 5 }, 17),
        (ReplaceResult::Replaced, 0),
    ] {
        let value = outcome(result, length);
        let decoded = ReplaceOutcome::decode(value.container()).unwrap();
        assert_eq!(decoded.operation_id(), "replace-operation");
        assert_eq!(decoded.subject(), "plugin.replace");
        assert_eq!(decoded.request_sha256(), [1; 32]);
        assert_eq!(decoded.target_reference(), [2; 32]);
        assert_eq!(decoded.expected_identity(), [3; 32]);
        assert_eq!(
            decoded.content_sha256(),
            if length == 0 {
                <[u8; 32]>::from(Sha256::digest([]))
            } else {
                [4; 32]
            }
        );
        assert_eq!(decoded.content_length(), length);
        assert_eq!(decoded.result(), result);
        assert_eq!(decoded.raw(), value.raw());
        assert_eq!(decoded.container(), value.container());
        assert!(decoded.container().len() <= file_effect::MAX_REPLACE_CONTAINER_BYTES);
    }
    let created = CreateOutcome::new(
        "replace-operation",
        "plugin.replace",
        [1; 32],
        [2; 32],
        [4; 32],
        17,
        CreateResult::Created,
    )
    .unwrap();
    assert!(ReplaceOutcome::decode(created.container()).is_err());
    assert!(CreateOutcome::decode(outcome(ReplaceResult::Replaced, 17).container()).is_err());
}

#[test]
fn constructor_rejects_bad_identity_digests_length_and_error_code() {
    let cases = [
        (
            "bad/path",
            "plugin.replace",
            [1; 32],
            [2; 32],
            [3; 32],
            [4; 32],
            1,
            ReplaceResult::Replaced,
        ),
        (
            "replace-operation",
            "bad/path",
            [1; 32],
            [2; 32],
            [3; 32],
            [4; 32],
            1,
            ReplaceResult::Replaced,
        ),
        (
            "replace-operation",
            "plugin.replace",
            [0; 32],
            [2; 32],
            [3; 32],
            [4; 32],
            1,
            ReplaceResult::Replaced,
        ),
        (
            "replace-operation",
            "plugin.replace",
            [1; 32],
            [0; 32],
            [3; 32],
            [4; 32],
            1,
            ReplaceResult::Replaced,
        ),
        (
            "replace-operation",
            "plugin.replace",
            [1; 32],
            [2; 32],
            [0; 32],
            [4; 32],
            1,
            ReplaceResult::Replaced,
        ),
        (
            "replace-operation",
            "plugin.replace",
            [1; 32],
            [2; 32],
            [3; 32],
            [0; 32],
            1,
            ReplaceResult::Replaced,
        ),
        (
            "replace-operation",
            "plugin.replace",
            [1; 32],
            [2; 32],
            [3; 32],
            [4; 32],
            MAX_CONTENT_BYTES as u64 + 1,
            ReplaceResult::Replaced,
        ),
        (
            "replace-operation",
            "plugin.replace",
            [1; 32],
            [2; 32],
            [3; 32],
            [4; 32],
            0,
            ReplaceResult::Replaced,
        ),
        (
            "replace-operation",
            "plugin.replace",
            [1; 32],
            [2; 32],
            [3; 32],
            [4; 32],
            1,
            ReplaceResult::OsRejected { code: 0 },
        ),
    ];
    for (op, subject, request, target, expected, content, length, result) in cases {
        assert!(
            ReplaceOutcome::new(
                op, subject, request, target, expected, content, length, result
            )
            .is_err()
        );
    }
}

#[test]
fn decode_rejects_unknown_duplicate_wrong_wire_and_inconsistent_values() {
    let valid = outcome(ReplaceResult::Replaced, 9);
    let mut wire = proto::ReplaceOutcome::decode(valid.raw()).unwrap();
    wire.result = 2;
    assert!(ReplaceOutcome::decode(&pack(&wire.encode_to_vec())).is_err());
    wire.result = 1;
    wire.os_error_code = 5;
    assert!(ReplaceOutcome::decode(&pack(&wire.encode_to_vec())).is_err());
    wire.os_error_code = 0;
    wire.expected_identity = vec![8; 31];
    assert!(ReplaceOutcome::decode(&pack(&wire.encode_to_vec())).is_err());
    wire.expected_identity = vec![3; 32];
    wire.content_length = MAX_CONTENT_BYTES as u64 + 1;
    assert!(matches!(
        ReplaceOutcome::decode(&pack(&wire.encode_to_vec())),
        Err(Error::Limit)
    ));
    wire.content_length = 9;
    wire.result = 3;
    assert!(matches!(
        ReplaceOutcome::decode(&pack(&wire.encode_to_vec())),
        Err(Error::UnsupportedVersion)
    ));
    let mut unknown = valid.raw().to_vec();
    unknown.extend_from_slice(&[0x58, 0x01]);
    assert!(matches!(
        ReplaceOutcome::decode(&pack(&unknown)),
        Err(Error::UnsupportedVersion)
    ));
    let mut duplicate = valid.raw().to_vec();
    duplicate.extend_from_slice(&[0x08, 0x01]);
    assert!(ReplaceOutcome::decode(&pack(&duplicate)).is_err());
    let mut wrong_wire = valid.raw().to_vec();
    wrong_wire.extend_from_slice(&[0x52, 0x00]);
    assert!(ReplaceOutcome::decode(&pack(&wrong_wire)).is_err());
    let mut oversized = proto::ReplaceOutcome::decode(valid.raw()).unwrap();
    oversized.operation_id = "x".repeat(257);
    assert!(matches!(
        ReplaceOutcome::decode(&pack(&oversized.encode_to_vec())),
        Err(Error::Limit)
    ));
}

#[test]
fn envelope_corruption_and_bounds_fail_closed() {
    let value = outcome(ReplaceResult::Replaced, 1);
    let mut bytes = value.container().to_vec();
    bytes[18] ^= 1;
    assert!(matches!(
        ReplaceOutcome::decode(&bytes),
        Err(Error::Integrity)
    ));
    bytes = value.container().to_vec();
    bytes[0] ^= 1;
    assert!(ReplaceOutcome::decode(&bytes).is_err());
    assert!(matches!(
        ReplaceOutcome::decode(&vec![0; file_effect::MAX_REPLACE_CONTAINER_BYTES + 1]),
        Err(Error::Limit)
    ));
}
