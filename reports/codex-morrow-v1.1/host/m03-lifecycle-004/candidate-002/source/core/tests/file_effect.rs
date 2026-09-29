use morrow_core::{
    Error,
    file_effect::{self, DeleteOutcome, DeleteResult, proto},
};
use prost::Message;
use sha2::{Digest, Sha256};

fn outcome(result: DeleteResult) -> DeleteOutcome {
    DeleteOutcome::new(
        "delete-operation",
        "plugin.delete",
        [1; 32],
        [2; 32],
        [3; 32],
        result,
    )
    .unwrap()
}

fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut bytes = file_effect::MAGIC.to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend_from_slice(&compressed);
    bytes
}

#[test]
fn roundtrip_binds_delete_identity_and_result() {
    for result in [DeleteResult::Deleted, DeleteResult::OsRejected { code: 5 }] {
        let value = outcome(result);
        let decoded = DeleteOutcome::decode(value.container()).unwrap();
        assert_eq!(decoded.operation_id(), "delete-operation");
        assert_eq!(decoded.subject(), "plugin.delete");
        assert_eq!(decoded.request_sha256(), [1; 32]);
        assert_eq!(decoded.target_reference(), [2; 32]);
        assert_eq!(decoded.expected_identity(), [3; 32]);
        assert_eq!(decoded.result(), result);
        assert_eq!(decoded.raw(), value.raw());
        assert_eq!(decoded.container(), value.container());
        assert!(decoded.container().len() <= file_effect::MAX_CONTAINER_BYTES);
    }
}

#[test]
fn invalid_identity_result_and_unknown_fields_are_rejected() {
    assert!(
        DeleteOutcome::new(
            "bad/path",
            "plugin.delete",
            [1; 32],
            [2; 32],
            [3; 32],
            DeleteResult::Deleted
        )
        .is_err()
    );
    assert!(
        DeleteOutcome::new(
            "delete-operation",
            "plugin.delete",
            [0; 32],
            [2; 32],
            [3; 32],
            DeleteResult::Deleted
        )
        .is_err()
    );
    assert!(
        DeleteOutcome::new(
            "delete-operation",
            "plugin.delete",
            [1; 32],
            [0; 32],
            [3; 32],
            DeleteResult::Deleted
        )
        .is_err()
    );
    assert!(
        DeleteOutcome::new(
            "delete-operation",
            "plugin.delete",
            [1; 32],
            [2; 32],
            [0; 32],
            DeleteResult::Deleted
        )
        .is_err()
    );
    assert!(
        DeleteOutcome::new(
            "delete-operation",
            "plugin.delete",
            [1; 32],
            [2; 32],
            [3; 32],
            DeleteResult::OsRejected { code: 0 }
        )
        .is_err()
    );
    let value = outcome(DeleteResult::Deleted);
    let mut wire = proto::DeleteOutcome::decode(value.raw()).unwrap();
    wire.result = 2;
    assert!(DeleteOutcome::decode(&pack(&wire.encode_to_vec())).is_err());
    wire.result = 1;
    wire.os_error_code = 7;
    assert!(DeleteOutcome::decode(&pack(&wire.encode_to_vec())).is_err());
    wire.os_error_code = 0;
    wire.expected_identity = vec![4; 31];
    assert!(DeleteOutcome::decode(&pack(&wire.encode_to_vec())).is_err());
    let mut unknown = value.raw().to_vec();
    unknown.extend_from_slice(&[0x48, 0x01]);
    assert!(matches!(
        DeleteOutcome::decode(&pack(&unknown)),
        Err(Error::UnsupportedVersion)
    ));
    let mut duplicate = value.raw().to_vec();
    duplicate.extend_from_slice(&[0x08, 0x01]);
    assert!(DeleteOutcome::decode(&pack(&duplicate)).is_err());
}

#[test]
fn container_corruption_and_bounds_fail_closed() {
    let value = outcome(DeleteResult::Deleted);
    let mut bytes = value.container().to_vec();
    bytes[18] ^= 1;
    assert!(matches!(
        DeleteOutcome::decode(&bytes),
        Err(Error::Integrity)
    ));
    bytes = value.container().to_vec();
    bytes[0] ^= 1;
    assert!(DeleteOutcome::decode(&bytes).is_err());
    let too_large = vec![0; file_effect::MAX_CONTAINER_BYTES + 1];
    assert!(matches!(
        DeleteOutcome::decode(&too_large),
        Err(Error::Limit)
    ));
    let mut wire = proto::DeleteOutcome::decode(value.raw()).unwrap();
    wire.operation_id = "x".repeat(257);
    assert!(matches!(
        DeleteOutcome::decode(&pack(&wire.encode_to_vec())),
        Err(Error::Limit)
    ));
}
