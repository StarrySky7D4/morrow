use morrow_core::{
    Error,
    file_content::FileContent,
    file_content_receipt::{self, FileContentReceipt, Source, proto},
};
use prost::Message;
use sha2::{Digest, Sha256};

fn content() -> FileContent {
    FileContent::new(
        "receipt-operation",
        "plugin.receipt",
        [7; 32],
        b"retained bytes",
    )
    .unwrap()
}

fn raw(receipt: &FileContentReceipt) -> Vec<u8> {
    let container = receipt.container();
    let length = u32::from_le_bytes(container[10..14].try_into().unwrap()) as usize;
    lz4_flex::block::decompress(&container[50..], length).unwrap()
}

fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut result = file_content_receipt::MAGIC.to_vec();
    result.extend_from_slice(&1u16.to_le_bytes());
    result.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    result.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    result.extend_from_slice(&Sha256::digest(raw));
    result.extend_from_slice(&compressed);
    result
}

fn wire(receipt: &FileContentReceipt) -> proto::Receipt {
    proto::Receipt::decode(receipt.raw()).unwrap()
}

#[test]
fn live_and_legacy_roundtrip_binds_exact_content_container_bytes() {
    let content = content();
    let live = FileContentReceipt::new(&content, Source::LiveStaging).unwrap();
    let legacy = FileContentReceipt::new(&content, Source::LegacyImport).unwrap();
    for (receipt, source, number) in [
        (&live, Source::LiveStaging, 1),
        (&legacy, Source::LegacyImport, 2),
    ] {
        assert_eq!(receipt.operation_id(), content.operation_id());
        assert_eq!(receipt.subject(), content.subject());
        assert_eq!(receipt.request_sha256(), content.request_sha256());
        assert_eq!(receipt.content_sha256(), content.content_sha256());
        let expected_container_sha256: [u8; 32] = Sha256::digest(content.container()).into();
        assert_eq!(
            receipt.content_container_sha256(),
            expected_container_sha256
        );
        assert_eq!(receipt.content_length(), content.content().len() as u64);
        assert_eq!(receipt.source(), source);
        assert_eq!(receipt.raw(), raw(receipt));
        assert!(receipt.container().len() <= file_content_receipt::MAX_CONTAINER_BYTES);
        let message = wire(receipt);
        assert_eq!(message.schema_version, 1);
        assert_eq!(message.source, number);
        assert_eq!(message.encode_to_vec(), receipt.raw());
        let restored = FileContentReceipt::decode(receipt.container()).unwrap();
        assert_eq!(restored.container(), receipt.container());
        assert_eq!(restored.raw(), receipt.raw());
        assert_eq!(restored.source(), source);
        assert_eq!(restored.event_id(), receipt.event_id());
    }
    // Migration records an observation of existing bytes, not a past commit.
    assert_ne!(live.container(), legacy.container());
    assert_eq!(live.event_id(), legacy.event_id());
}

#[test]
fn event_identity_is_domain_separated_and_ignores_source() {
    let content = content();
    let live = FileContentReceipt::new(&content, Source::LiveStaging).unwrap();
    let other_request = FileContent::new(
        "receipt-operation",
        "plugin.receipt",
        [8; 32],
        b"retained bytes",
    )
    .unwrap();
    let other_operation = FileContent::new(
        "another-operation",
        "plugin.receipt",
        [7; 32],
        b"retained bytes",
    )
    .unwrap();
    assert_ne!(
        live.event_id(),
        FileContentReceipt::new(&other_request, Source::LiveStaging)
            .unwrap()
            .event_id()
    );
    assert_ne!(
        live.event_id(),
        FileContentReceipt::new(&other_operation, Source::LiveStaging)
            .unwrap()
            .event_id()
    );

    let mut hash = Sha256::new();
    hash.update(b"morrow.file-content-receipt.event.v1\0");
    hash.update((content.operation_id().len() as u64).to_le_bytes());
    hash.update(content.operation_id().as_bytes());
    hash.update(content.request_sha256());
    let expected = format!("file-content-{:x}", hash.finalize());
    assert_eq!(live.event_id(), expected);
}

#[test]
fn empty_content_and_shared_identity_limits_are_validated() {
    let content = FileContent::new(&"a".repeat(256), &"b".repeat(256), [1; 32], b"").unwrap();
    let receipt = FileContentReceipt::new(&content, Source::LiveStaging).unwrap();
    assert_eq!(receipt.content_length(), 0);
    let empty_sha256: [u8; 32] = Sha256::digest([]).into();
    assert_eq!(receipt.content_sha256(), empty_sha256);
    FileContentReceipt::decode(receipt.container()).unwrap();
    let mut impossible_empty = wire(&receipt);
    impossible_empty.content_sha256 = vec![9; 32];
    assert!(matches!(
        FileContentReceipt::decode(&pack(&impossible_empty.encode_to_vec())),
        Err(Error::Integrity)
    ));

    let mut value = wire(&receipt);
    value.operation_id.push('c');
    assert!(FileContentReceipt::decode(&pack(&value.encode_to_vec())).is_err());
    value = wire(&receipt);
    value.subject = "bad/path".into();
    assert!(FileContentReceipt::decode(&pack(&value.encode_to_vec())).is_err());
    value = wire(&receipt);
    value.content_length = 16 * 1024 * 1024 + 1;
    assert!(matches!(
        FileContentReceipt::decode(&pack(&value.encode_to_vec())),
        Err(Error::Limit)
    ));
}

#[test]
fn invalid_digest_lengths_zeros_and_sources_are_rejected() {
    let receipt = FileContentReceipt::new(&content(), Source::LiveStaging).unwrap();
    for field in 0..6 {
        let mut value = wire(&receipt);
        match field {
            0 => value.request_sha256.clear(),
            1 => value.content_sha256.pop().map(|_| ()).unwrap(),
            2 => value.content_container_sha256.push(1),
            3 => value.request_sha256 = vec![0; 32],
            4 => value.content_sha256 = vec![0; 32],
            _ => value.content_container_sha256 = vec![0; 32],
        }
        assert!(
            FileContentReceipt::decode(&pack(&value.encode_to_vec())).is_err(),
            "digest case {field}"
        );
    }
    for source in [0, 3, -1] {
        let mut value = wire(&receipt);
        value.source = source;
        assert!(
            FileContentReceipt::decode(&pack(&value.encode_to_vec())).is_err(),
            "source {source}"
        );
    }
}

#[test]
fn malformed_envelope_unknown_duplicate_and_noncanonical_protobuf_fail() {
    let receipt = FileContentReceipt::new(&content(), Source::LiveStaging).unwrap();
    let valid = receipt.raw().to_vec();
    let mut unknown = valid.clone();
    unknown.extend_from_slice(&[0x48, 1]);
    let mut duplicate = valid.clone();
    duplicate.extend_from_slice(&[0x08, 1]);
    let mut overlong = valid.clone();
    overlong.splice(1..2, [0x81, 0]);
    let mut wrong_wire = valid.clone();
    wrong_wire[2] = 0x10;
    let mut truncated = valid.clone();
    truncated.pop();
    for bytes in [unknown, duplicate, overlong, wrong_wire, truncated] {
        assert!(FileContentReceipt::decode(&pack(&bytes)).is_err());
    }
    let mut oversized_field = wire(&receipt);
    oversized_field.operation_id = "a".repeat(257);
    assert!(matches!(
        FileContentReceipt::decode(&pack(&oversized_field.encode_to_vec())),
        Err(Error::Limit)
    ));

    let mut wrong_magic = receipt.container().to_vec();
    wrong_magic[0] ^= 1;
    assert!(FileContentReceipt::decode(&wrong_magic).is_err());
    let mut wrong_version = receipt.container().to_vec();
    wrong_version[8] = 2;
    assert!(matches!(
        FileContentReceipt::decode(&wrong_version),
        Err(Error::UnsupportedVersion)
    ));
    let mut wrong_outer_sha = receipt.container().to_vec();
    wrong_outer_sha[20] ^= 1;
    assert!(matches!(
        FileContentReceipt::decode(&wrong_outer_sha),
        Err(Error::Integrity)
    ));
    assert!(matches!(
        FileContentReceipt::decode(&vec![0; file_content_receipt::MAX_CONTAINER_BYTES + 1]),
        Err(Error::Limit)
    ));
    let mut oversized_raw = receipt.container().to_vec();
    oversized_raw[10..14]
        .copy_from_slice(&((file_content_receipt::MAX_RAW_BYTES + 1) as u32).to_le_bytes());
    assert!(matches!(
        FileContentReceipt::decode(&oversized_raw),
        Err(Error::Limit)
    ));
}
