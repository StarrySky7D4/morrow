use morrow_core::{
    Error,
    file_mutation::{self, Disposition, MutationRequest, RequestRecord, Target, proto},
    file_path::RelativeFilePath,
    io_intent::Record,
    plugin_package::io::IoCapability,
};
use prost::Message;
use sha2::{Digest, Sha256};

fn request() -> MutationRequest {
    MutationRequest {
        operation_id: "mutation".into(),
        subject: "file-plugin".into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: Some(RelativeFilePath::parse("notes/data.txt").unwrap()),
        },
        disposition: Disposition::Replace,
        expected_identity: Some([4; 32]),
        content_length: 3,
        content_sha256: Some(Sha256::digest(b"abc").into()),
    }
}
fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut out = file_mutation::MAGIC.to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend_from_slice(&compressed);
    out
}
fn wire() -> proto::Request {
    let record = RequestRecord::new(request()).unwrap();
    let raw = lz4_flex::block::decompress(
        &record.container()[50..],
        u32::from_le_bytes(record.container()[10..14].try_into().unwrap()) as usize,
    )
    .unwrap();
    proto::Request::decode(raw.as_slice()).unwrap()
}
#[test]
fn dispositions_roundtrip_and_bind_exact_inputs() {
    for disposition in [
        Disposition::Create,
        Disposition::Replace,
        Disposition::Delete,
    ] {
        let mut input = request();
        input.disposition = disposition;
        if disposition == Disposition::Create {
            input.expected_identity = None;
        }
        if disposition == Disposition::Delete {
            input.content_length = 0;
            input.content_sha256 = None;
        }
        let encoded = RequestRecord::new(input.clone()).unwrap();
        let decoded = RequestRecord::decode(encoded.container()).unwrap();
        assert_eq!(decoded.request(), &input);
        assert_eq!(decoded.container(), encoded.container());
        assert_eq!(decoded.command().unwrap(), encoded.command().unwrap());
        Record::prepared(decoded.command().unwrap()).unwrap();
    }
    let original = RequestRecord::new(request()).unwrap().command().unwrap();
    for index in 0..9 {
        let mut changed = request();
        match index {
            0 => changed.operation_id.push('2'),
            1 => changed.subject.push('2'),
            2 => changed.package_sha256 = [8; 32],
            3 => changed.approval_sha256 = [8; 32],
            4 => changed.target.reference = [8; 32],
            5 => changed.target.relative_path = None,
            6 => changed.expected_identity = Some([8; 32]),
            7 => changed.content_length += 1,
            _ => changed.content_sha256 = Some([8; 32]),
        }
        let updated = RequestRecord::new(changed).unwrap().command().unwrap();
        assert_ne!(updated.request_sha256, original.request_sha256);
        assert_eq!(
            updated.target_sha256 != original.target_sha256,
            matches!(index, 4 | 5)
        );
    }
}
#[test]
fn identities_accept_shared_256_byte_boundary_and_reject_larger() {
    let mut input = request();
    input.subject = "a".repeat(256);
    input.operation_id = "b".repeat(256);
    let encoded = RequestRecord::new(input.clone()).unwrap();
    assert_eq!(
        RequestRecord::decode(encoded.container())
            .unwrap()
            .request(),
        &input
    );
    input.subject.push('c');
    assert!(RequestRecord::new(input).is_err());
}
#[test]
fn invalid_disposition_preconditions_and_content_are_rejected() {
    for i in 0..10 {
        let mut input = request();
        match i {
            0 => input.expected_identity = None,
            1 => input.expected_identity = Some([0; 32]),
            2 => input.content_sha256 = None,
            3 => input.content_sha256 = Some([0; 32]),
            4 => input.content_length = 0,
            5 => input.content_length = 16 * 1024 * 1024 + 1,
            6 => input.disposition = Disposition::Create,
            7 => input.disposition = Disposition::Delete,
            8 => input.target.reference = [0; 32],
            _ => input.approval_sha256 = [0; 32],
        }
        assert!(RequestRecord::new(input).is_err(), "case {i}");
    }
    let mut empty = request();
    empty.content_length = 0;
    empty.content_sha256 = Some(Sha256::digest([]).into());
    RequestRecord::new(empty).unwrap();
}
#[test]
fn malformed_or_noncanonical_wire_is_rejected_before_use() {
    let valid = wire().encode_to_vec();
    for suffix in [
        &[8u8, 1][..],
        &[96, 1],
        &[56, 0],
        &[18, 255, 255, 255, 255, 15],
    ] {
        let mut raw = valid.clone();
        raw.extend_from_slice(suffix);
        assert!(RequestRecord::decode(&pack(&raw)).is_err());
    }
    // Nonminimal varint for schema_version=1, with a correct outer checksum.
    let mut overlong = valid.clone();
    assert_eq!(&overlong[..2], &[8, 1]);
    overlong.splice(1..2, [0x81, 0]);
    assert!(RequestRecord::decode(&pack(&overlong)).is_err());
    for i in 0..6 {
        let mut malformed = wire();
        match i {
            0 => malformed.schema_version = 2,
            1 => malformed.disposition = 4,
            2 => malformed.target_reference.pop().map(|_| ()).unwrap(),
            3 => malformed.relative_path = "../escape".into(),
            4 => malformed.content_sha256.push(1),
            _ => malformed.operation_id = "a".repeat(257),
        }
        assert!(
            RequestRecord::decode(&pack(&malformed.encode_to_vec())).is_err(),
            "case {i}"
        );
    }
    let mut container = pack(&valid);
    container[20] ^= 1;
    assert!(RequestRecord::decode(&container).is_err());
    assert!(RequestRecord::decode(&pack(&vec![0; file_mutation::MAX_RAW_BYTES + 1])).is_err());
}
#[test]
fn dedicated_protocol_cannot_be_relabelled_as_network_or_read() {
    let command = RequestRecord::new(request()).unwrap().command().unwrap();
    for capability in [
        IoCapability::FileRead,
        IoCapability::FileList,
        IoCapability::HttpRequest,
        IoCapability::HttpPublish,
    ] {
        let mut relabelled = command.clone();
        relabelled.capability = capability;
        assert!(matches!(
            Record::prepared(relabelled),
            Err(Error::UnsupportedVersion)
        ));
    }
}
