use morrow_core::{
    Error,
    file_content::{self, FileContent, proto},
};
use prost::Message;
use sha2::{Digest, Sha256};

const OPERATION: &str = "file-content-operation";
const SUBJECT: &str = "plugin.file-content";

fn content(bytes: &[u8]) -> FileContent {
    FileContent::new(OPERATION, SUBJECT, [7; 32], bytes).unwrap()
}

fn raw(record: &FileContent) -> Vec<u8> {
    let bytes = record.container();
    let length = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    lz4_flex::block::decompress(&bytes[50..], length).unwrap()
}

fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut out = file_content::MAGIC.to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend_from_slice(&compressed);
    out
}

#[test]
fn empty_and_nonempty_content_roundtrip_with_exact_request_binding() {
    for bytes in [&[][..], &[0, 1, 255, 17][..]] {
        let original = content(bytes);
        assert_eq!(original.operation_id(), OPERATION);
        assert_eq!(original.subject(), SUBJECT);
        assert_eq!(original.request_sha256(), [7; 32]);
        assert_eq!(original.content(), bytes);
        let expected_sha256: [u8; 32] = Sha256::digest(bytes).into();
        assert_eq!(original.content_sha256(), expected_sha256);
        assert!(original.container().len() <= file_content::MAX_CONTAINER_BYTES);
        let wire = raw(&original);
        let message = proto::FileContent::decode(wire.as_slice()).unwrap();
        assert_eq!(message.schema_version, 1);
        assert_eq!(message.operation_id, OPERATION);
        assert_eq!(message.subject, SUBJECT);
        assert_eq!(message.request_sha256, [7; 32]);
        assert_eq!(message.content_sha256, Sha256::digest(bytes).to_vec());
        assert_eq!(message.content, bytes);
        assert_eq!(message.encode_to_vec(), wire);
        let restored = FileContent::decode(original.container()).unwrap();
        assert_eq!(restored.container(), original.container());
        assert_eq!(restored.content(), bytes);
        assert_eq!(restored.content_sha256(), original.content_sha256());
        assert_eq!(restored.request_sha256(), original.request_sha256());
    }
    let first = content(b"same bytes");
    let other_request = FileContent::new(OPERATION, SUBJECT, [8; 32], b"same bytes").unwrap();
    let other_operation =
        FileContent::new("another-operation", SUBJECT, [7; 32], b"same bytes").unwrap();
    assert_ne!(first.container(), other_request.container());
    assert_ne!(first.container(), other_operation.container());
    assert_eq!(first.content_sha256(), other_request.content_sha256());
}

#[test]
fn shared_identity_and_content_bounds_are_enforced() {
    let exact_identity =
        FileContent::new(&"a".repeat(256), &"b".repeat(256), [1; 32], b"").unwrap();
    FileContent::decode(exact_identity.container()).unwrap();
    for (operation, subject, digest) in [
        ("", SUBJECT, [7; 32]),
        ("bad/path", SUBJECT, [7; 32]),
        ("a", "bad:subject", [7; 32]),
        ("a", SUBJECT, [0; 32]),
    ] {
        assert!(FileContent::new(operation, subject, digest, b"").is_err());
    }
    assert!(FileContent::new(&"a".repeat(257), SUBJECT, [7; 32], b"").is_err());
    let max = vec![0x5a; file_content::MAX_CONTENT_BYTES];
    let full = content(&max);
    assert_eq!(full.content().len(), file_content::MAX_CONTENT_BYTES);
    assert_eq!(
        FileContent::decode(full.container()).unwrap().content(),
        max
    );
    assert!(matches!(
        FileContent::new(
            OPERATION,
            SUBJECT,
            [7; 32],
            &vec![0; file_content::MAX_CONTENT_BYTES + 1]
        ),
        Err(Error::Limit)
    ));
}

#[test]
fn malformed_envelope_and_oversize_headers_fail_before_use() {
    let original = content(b"payload");
    let mut wrong_magic = original.container().to_vec();
    wrong_magic[0] ^= 1;
    assert!(FileContent::decode(&wrong_magic).is_err());
    let mut wrong_version = original.container().to_vec();
    wrong_version[8] = 2;
    assert!(matches!(
        FileContent::decode(&wrong_version),
        Err(Error::UnsupportedVersion)
    ));
    let mut wrong_digest = original.container().to_vec();
    wrong_digest[20] ^= 1;
    assert!(matches!(
        FileContent::decode(&wrong_digest),
        Err(Error::Integrity)
    ));
    let mut excessive_raw = original.container().to_vec();
    excessive_raw[10..14]
        .copy_from_slice(&((file_content::MAX_CONTENT_BYTES + 1025) as u32).to_le_bytes());
    assert!(matches!(
        FileContent::decode(&excessive_raw),
        Err(Error::Limit)
    ));
    let mut wrong_packed_length = original.container().to_vec();
    wrong_packed_length[14..18].copy_from_slice(&1u32.to_le_bytes());
    assert!(FileContent::decode(&wrong_packed_length).is_err());
    assert!(FileContent::decode(&original.container()[..49]).is_err());
    assert!(matches!(
        FileContent::decode(&vec![0; file_content::MAX_CONTAINER_BYTES + 1]),
        Err(Error::Limit)
    ));
}

#[test]
fn malformed_protobuf_unknown_duplicate_noncanonical_and_oversize_are_rejected() {
    let original = content(b"payload");
    let valid = raw(&original);
    let mut unknown = valid.clone();
    unknown.extend_from_slice(&[0x38, 1]);
    let mut duplicate = valid.clone();
    duplicate.extend_from_slice(&[0x08, 1]);
    let mut overlong = valid.clone();
    overlong.splice(1..2, [0x81, 0]);
    let mut wrong_wire = valid.clone();
    wrong_wire[2] = 0x10;
    let mut empty_field = raw(&content(b""));
    empty_field.extend_from_slice(&[0x32, 0]);
    let mut truncated = valid.clone();
    truncated.pop();
    for value in [
        unknown,
        duplicate,
        overlong,
        wrong_wire,
        empty_field,
        truncated,
    ] {
        assert!(FileContent::decode(&pack(&value)).is_err());
    }
    let mut oversized_content = raw(&content(b""));
    oversized_content.push(0x32);
    prost::encoding::encode_varint(
        (file_content::MAX_CONTENT_BYTES + 1) as u64,
        &mut oversized_content,
    );
    assert!(matches!(
        FileContent::decode(&pack(&oversized_content)),
        Err(Error::Limit)
    ));
    let mut message = proto::FileContent::decode(valid.as_slice()).unwrap();
    message.request_sha256.clear();
    assert!(FileContent::decode(&pack(&message.encode_to_vec())).is_err());
    message.request_sha256 = vec![0; 32];
    assert!(FileContent::decode(&pack(&message.encode_to_vec())).is_err());
}

#[test]
fn valid_outer_checksum_cannot_hide_content_digest_mismatch() {
    let original = content(b"payload");
    let mut message = proto::FileContent::decode(raw(&original).as_slice()).unwrap();
    message.content[0] ^= 1;
    assert!(matches!(
        FileContent::decode(&pack(&message.encode_to_vec())),
        Err(Error::Integrity)
    ));
    message.content_sha256 = vec![9; 32];
    assert!(matches!(
        FileContent::decode(&pack(&message.encode_to_vec())),
        Err(Error::Integrity)
    ));
}
