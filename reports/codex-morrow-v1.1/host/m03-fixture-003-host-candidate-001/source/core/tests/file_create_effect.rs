use morrow_core::{
    Error,
    file_content::MAX_CONTENT_BYTES,
    file_effect::{self, CreateOutcome, CreateResult, DeleteOutcome, DeleteResult, proto},
};
use prost::Message;
use sha2::{Digest, Sha256};

fn outcome(result: CreateResult, length: u64) -> CreateOutcome {
    CreateOutcome::new(
        "create-operation",
        "plugin.create",
        [1; 32],
        [2; 32],
        if length == 0 {
            <[u8; 32]>::from(Sha256::digest([]))
        } else {
            [3; 32]
        },
        length,
        result,
    )
    .unwrap()
}

fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut bytes = b"MROWFEC1".to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend_from_slice(&compressed);
    bytes
}

#[test]
fn roundtrip_binds_create_identity_content_and_result_including_empty_content() {
    for (result, length) in [
        (CreateResult::Created, 17),
        (CreateResult::OsRejected { code: 5 }, 17),
        (CreateResult::Created, 0),
    ] {
        let value = outcome(result, length);
        let decoded = CreateOutcome::decode(value.container()).unwrap();
        assert_eq!(decoded.operation_id(), "create-operation");
        assert_eq!(decoded.subject(), "plugin.create");
        assert_eq!(decoded.request_sha256(), [1; 32]);
        assert_eq!(decoded.target_reference(), [2; 32]);
        assert_eq!(
            decoded.content_sha256(),
            if length == 0 {
                <[u8; 32]>::from(Sha256::digest([]))
            } else {
                [3; 32]
            }
        );
        assert_eq!(decoded.content_length(), length);
        assert_eq!(decoded.result(), result);
        assert_eq!(decoded.raw(), value.raw());
        assert_eq!(decoded.container(), value.container());
        assert!(decoded.container().len() <= file_effect::MAX_CREATE_CONTAINER_BYTES);
    }
    let deleted = DeleteOutcome::new(
        "create-operation",
        "plugin.create",
        [1; 32],
        [2; 32],
        [3; 32],
        DeleteResult::Deleted,
    )
    .unwrap();
    assert!(CreateOutcome::decode(deleted.container()).is_err());
    assert!(DeleteOutcome::decode(outcome(CreateResult::Created, 1).container()).is_err());
}

#[test]
fn caller_validation_rejects_bad_identity_digest_length_and_result_code() {
    for (operation, subject, request, target, content, length, result) in [
        (
            "bad/path",
            "plugin.create",
            [1; 32],
            [2; 32],
            [3; 32],
            1,
            CreateResult::Created,
        ),
        (
            "create-operation",
            "bad/path",
            [1; 32],
            [2; 32],
            [3; 32],
            1,
            CreateResult::Created,
        ),
        (
            "create-operation",
            "plugin.create",
            [0; 32],
            [2; 32],
            [3; 32],
            1,
            CreateResult::Created,
        ),
        (
            "create-operation",
            "plugin.create",
            [1; 32],
            [0; 32],
            [3; 32],
            1,
            CreateResult::Created,
        ),
        (
            "create-operation",
            "plugin.create",
            [1; 32],
            [2; 32],
            [0; 32],
            1,
            CreateResult::Created,
        ),
        (
            "create-operation",
            "plugin.create",
            [1; 32],
            [2; 32],
            [3; 32],
            0,
            CreateResult::Created,
        ),
        (
            "create-operation",
            "plugin.create",
            [1; 32],
            [2; 32],
            [3; 32],
            MAX_CONTENT_BYTES as u64 + 1,
            CreateResult::Created,
        ),
        (
            "create-operation",
            "plugin.create",
            [1; 32],
            [2; 32],
            [3; 32],
            1,
            CreateResult::OsRejected { code: 0 },
        ),
    ] {
        assert!(
            CreateOutcome::new(operation, subject, request, target, content, length, result)
                .is_err()
        );
    }
}

#[test]
fn decode_rejects_noncanonical_unknown_duplicate_wrong_wire_and_invalid_fields() {
    let valid = outcome(CreateResult::Created, 9);
    let mut wire = proto::CreateOutcome::decode(valid.raw()).unwrap();
    wire.result = 2;
    assert!(CreateOutcome::decode(&pack(&wire.encode_to_vec())).is_err());
    wire.result = 1;
    wire.os_error_code = 7;
    assert!(CreateOutcome::decode(&pack(&wire.encode_to_vec())).is_err());
    wire.os_error_code = 0;
    wire.content_sha256 = vec![4; 31];
    assert!(CreateOutcome::decode(&pack(&wire.encode_to_vec())).is_err());
    wire.content_sha256 = vec![3; 32];
    wire.content_length = MAX_CONTENT_BYTES as u64 + 1;
    assert!(matches!(
        CreateOutcome::decode(&pack(&wire.encode_to_vec())),
        Err(Error::Limit)
    ));
    wire.content_length = 9;
    wire.result = 3;
    assert!(matches!(
        CreateOutcome::decode(&pack(&wire.encode_to_vec())),
        Err(Error::UnsupportedVersion)
    ));
    let mut unknown = valid.raw().to_vec();
    unknown.extend_from_slice(&[0x50, 0x01]);
    assert!(matches!(
        CreateOutcome::decode(&pack(&unknown)),
        Err(Error::UnsupportedVersion)
    ));
    let mut duplicate = valid.raw().to_vec();
    duplicate.extend_from_slice(&[0x08, 0x01]);
    assert!(CreateOutcome::decode(&pack(&duplicate)).is_err());
    let mut wrong_wire = valid.raw().to_vec();
    wrong_wire.extend_from_slice(&[0x4a, 0x00]);
    assert!(CreateOutcome::decode(&pack(&wrong_wire)).is_err());
    let mut oversized = proto::CreateOutcome::decode(valid.raw()).unwrap();
    oversized.subject = "x".repeat(257);
    assert!(matches!(
        CreateOutcome::decode(&pack(&oversized.encode_to_vec())),
        Err(Error::Limit)
    ));
}

#[test]
fn envelope_corruption_and_bounds_fail_closed() {
    let value = outcome(CreateResult::Created, 1);
    let mut bytes = value.container().to_vec();
    bytes[18] ^= 1;
    assert!(matches!(
        CreateOutcome::decode(&bytes),
        Err(Error::Integrity)
    ));
    bytes = value.container().to_vec();
    bytes[0] ^= 1;
    assert!(CreateOutcome::decode(&bytes).is_err());
    let too_large = vec![0; file_effect::MAX_CREATE_CONTAINER_BYTES + 1];
    assert!(matches!(
        CreateOutcome::decode(&too_large),
        Err(Error::Limit)
    ));
}
