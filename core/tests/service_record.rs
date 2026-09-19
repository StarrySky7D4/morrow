use morrow_core::{
    Error,
    io_intent::{Command, Record},
    plugin_package::io::{self, IoCapability},
    service::{Invocation, Request},
    service_record::{self, Policy, RequestRecord, proto},
};
use prost::Message;
use sha2::{Digest, Sha256};
fn policy() -> Policy {
    Policy {
        namespace: [7; 32],
        retention_ms: 1000,
    }
}
fn invocation() -> Invocation {
    Invocation {
        service: "service.example".into(),
        handler: "api.invoke".into(),
        principal: "alice".into(),
        method: "POST".into(),
        target: "/items".into(),
        headers: vec![],
        body: b"hello".to_vec(),
    }
}
fn request() -> Request {
    let invocation = invocation();
    let call = service_record::call_id(&policy(), "key", &invocation).unwrap();
    Request::encode(call, &invocation).unwrap()
}
fn pack(raw: &[u8]) -> Vec<u8> {
    let packed = lz4_flex::block::compress(raw);
    let mut out = service_record::MAGIC.to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend_from_slice(&packed);
    out
}
fn value() -> proto::RequestRecord {
    proto::RequestRecord {
        schema_version: 1,
        namespace: policy().namespace.to_vec(),
        key_sha256: Sha256::digest(b"key").to_vec(),
        created_ms: 10,
        expires_ms: 1010,
        retention_ms: 1000,
        request_frame: request().bytes().to_vec(),
    }
}
#[test]
fn exact_original_and_expiry_survive_container_roundtrip_without_raw_key() {
    let original = request();
    let record = RequestRecord::encode(&policy(), "key", &original, 10).unwrap();
    let parsed = RequestRecord::decode(record.container()).unwrap();
    assert_eq!(parsed.container(), record.container());
    assert_eq!(parsed.request().bytes(), original.bytes());
    assert_eq!(parsed.namespace(), [7; 32]);
    assert_eq!(
        parsed.key_sha256(),
        <[u8; 32]>::from(Sha256::digest(b"key"))
    );
    assert_eq!((parsed.created_ms(), parsed.expires_ms()), (10, 1010));
    assert!(!parsed.is_expired(10).unwrap());
    assert!(!parsed.is_expired(1009).unwrap());
    assert!(parsed.is_expired(1010).unwrap());
    assert!(parsed.is_expired(9).is_err());
    let secret = "distinct-raw-key-not-stored-anywhere";
    let record = RequestRecord::encode(&policy(), secret, &original, 10).unwrap();
    let raw = lz4_flex::block::decompress(
        &record.container()[50..],
        u32::from_le_bytes(record.container()[10..14].try_into().unwrap()) as usize,
    )
    .unwrap();
    assert!(!raw.windows(secret.len()).any(|w| w == secret.as_bytes()));
}
#[test]
fn same_scoped_key_binds_changed_input_to_same_operation_but_matching_conflicts() {
    let original = request();
    let record = RequestRecord::encode(&policy(), "key", &original, 10).unwrap();
    let operation = service_record::operation_id(&policy(), "key", &original).unwrap();
    for mutation in 0..4 {
        let mut invocation = invocation();
        match mutation {
            0 => invocation.method = "PUT".into(),
            1 => invocation.target = "/other".into(),
            2 => invocation.body = b"changed".to_vec(),
            _ => invocation.handler = "renamed-handler".into(),
        };
        let call = service_record::call_id(&policy(), "key", &invocation).unwrap();
        assert_eq!(call, original.call_id());
        let changed = Request::encode(call, &invocation).unwrap();
        assert_eq!(
            service_record::operation_id(&policy(), "key", &changed).unwrap(),
            operation
        );
        assert!(matches!(
            record.matches(&policy(), "key", &changed),
            Err(Error::OperationConflict)
        ));
    }
    let same_body_other_call =
        Request::encode(original.call_id().wrapping_add(1), &invocation()).unwrap();
    assert!(matches!(
        record.matches(&policy(), "key", &same_body_other_call),
        Err(Error::OperationConflict)
    ));
}
#[test]
fn namespace_principal_service_and_key_scope_identity() {
    let original = request();
    let operation = service_record::operation_id(&policy(), "key", &original).unwrap();
    for field in 0..2 {
        let mut invocation = invocation();
        match field {
            0 => invocation.service = "other-service".into(),
            _ => invocation.principal = "bob".into(),
        };
        let changed = Request::encode(1, &invocation).unwrap();
        assert_ne!(
            service_record::operation_id(&policy(), "key", &changed).unwrap(),
            operation
        );
    }
    let mut different = policy();
    different.namespace = [8; 32];
    assert_ne!(
        service_record::operation_id(&different, "key", &original).unwrap(),
        operation
    );
    assert_ne!(
        service_record::operation_id(&policy(), "other-key", &original).unwrap(),
        operation
    );
    assert_ne!(
        service_record::call_id(&policy(), "key", &invocation()).unwrap(),
        0
    );
    // Prefix lengths distinguish concatenation-ambiguous field tuples.
    let mut one = invocation();
    one.service = "ab".into();
    one.principal = "c".into();
    let mut two = invocation();
    two.service = "a".into();
    two.principal = "bc".into();
    assert_ne!(
        service_record::call_id(&policy(), "key", &one).unwrap(),
        service_record::call_id(&policy(), "key", &two).unwrap()
    );
}
#[test]
fn matching_preserves_original_timestamps_and_compares_policy_and_exact_frame() {
    let original = request();
    let first = RequestRecord::encode(&policy(), "key", &original, 10).unwrap();
    let later = RequestRecord::encode(&policy(), "key", &original, 99).unwrap();
    first.matches(&policy(), "key", &original).unwrap();
    later.matches(&policy(), "key", &original).unwrap();
    assert_ne!(
        first.command([4; 32]).unwrap().request_sha256,
        later.command([4; 32]).unwrap().request_sha256
    );
    assert_eq!(
        first.command([4; 32]).unwrap().operation_id,
        later.command([4; 32]).unwrap().operation_id
    );
    let mut changed = policy();
    changed.retention_ms += 1;
    assert!(matches!(
        first.matches(&changed, "key", &original),
        Err(Error::OperationConflict)
    ));
    changed = policy();
    changed.namespace = [8; 32];
    assert!(first.matches(&changed, "key", &original).is_err());
    assert!(first.matches(&policy(), "other", &original).is_err());
    // An expired historical record still matches; this is not permission to resend.
    assert!(first.is_expired(10000).unwrap());
    first.matches(&policy(), "key", &original).unwrap();
}
#[test]
fn policies_keys_and_timestamp_overflow_are_rejected_before_encoding() {
    let original = request();
    for key in ["", "with space", "\n", "非ASCII", "\u{7f}"] {
        assert!(RequestRecord::encode(&policy(), key, &original, 10).is_err());
        assert!(service_record::call_id(&policy(), key, &invocation()).is_err());
    }
    assert!(RequestRecord::encode(&policy(), &"x".repeat(129), &original, 10).is_err());
    assert!(RequestRecord::encode(&policy(), &"x".repeat(128), &original, 10).is_ok());
    for retention_ms in [0, service_record::MAX_RETENTION_MS + 1] {
        assert!(
            Policy {
                namespace: [1; 32],
                retention_ms
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        Policy {
            namespace: [0; 32],
            retention_ms: 1
        }
        .validate()
        .is_err()
    );
    for retention_ms in [1, service_record::MAX_RETENTION_MS] {
        Policy {
            namespace: [1; 32],
            retention_ms,
        }
        .validate()
        .unwrap();
    }
    assert!(RequestRecord::encode(&policy(), "key", &original, 0).is_err());
    assert!(RequestRecord::encode(&policy(), "key", &original, u64::MAX).is_err());
}
#[test]
fn protobuf_rejects_unknown_duplicate_wrong_wire_noncanonical_and_oversized_fields() {
    let raw = value().encode_to_vec();
    for suffix in [&[0x40, 1][..], &[8, 1][..]] {
        let mut bad = raw.clone();
        bad.extend_from_slice(suffix);
        assert!(RequestRecord::decode(&pack(&bad)).is_err());
    }
    let mut version_overflow = raw.clone();
    version_overflow.splice(1..2, [0x81, 0x80, 0x80, 0x80, 0x10]);
    assert!(RequestRecord::decode(&pack(&version_overflow)).is_err());
    let mut wrong = raw.clone();
    wrong[0] = 0x0a;
    assert!(RequestRecord::decode(&pack(&wrong)).is_err());
    // Overlong encoding of version 1 is semantically equal but not canonical.
    let mut overlong = raw.clone();
    overlong.splice(1..2, [0x81, 0]);
    assert!(RequestRecord::decode(&pack(&overlong)).is_err());
    for edit in 0..8 {
        let mut bad = value();
        match edit {
            0 => bad.schema_version = 2,
            1 => bad.namespace = vec![1; 31],
            2 => bad.key_sha256 = vec![1; 33],
            3 => bad.created_ms = 0,
            4 => bad.expires_ms += 1,
            5 => bad.retention_ms = 0,
            6 => bad.request_frame = vec![0; service_record::MAX_RAW_BYTES],
            _ => bad.key_sha256 = vec![0; 32],
        };
        assert!(
            RequestRecord::decode(&pack(&bad.encode_to_vec())).is_err(),
            "case {edit}"
        );
    }
    let mut overflow = value();
    overflow.created_ms = u64::MAX;
    overflow.expires_ms = 999;
    assert!(RequestRecord::decode(&pack(&overflow.encode_to_vec())).is_err());
}
#[test]
fn corrupt_container_and_invalid_inner_service_are_never_accepted() {
    let record = RequestRecord::encode(&policy(), "key", &request(), 10).unwrap();
    let mut corrupt = record.container().to_vec();
    corrupt[18] ^= 1;
    assert!(matches!(
        RequestRecord::decode(&corrupt),
        Err(Error::Integrity)
    ));
    let mut trailing = record.container().to_vec();
    trailing.push(0);
    assert!(RequestRecord::decode(&trailing).is_err());
    for n in 0..50 {
        assert!(RequestRecord::decode(&record.container()[..n]).is_err());
    }
    let mut bad = value();
    bad.request_frame = vec![0; 16];
    assert!(RequestRecord::decode(&pack(&bad.encode_to_vec())).is_err());
    let mut bad = value();
    bad.request_frame.extend_from_slice(&[0; 8]);
    assert!(RequestRecord::decode(&pack(&bad.encode_to_vec())).is_err());
}
#[test]
fn publish_command_binds_original_container_policy_package_and_scope() {
    let record = RequestRecord::encode(&policy(), "key", &request(), 10).unwrap();
    let command = record.command([4; 32]).unwrap();
    assert_eq!(
        command.operation_id,
        service_record::operation_id(&policy(), "key", record.request()).unwrap()
    );
    assert_eq!(command.capability, IoCapability::HttpPublish);
    assert_eq!(command.protocol_sha256, service_record::schema_digest());
    assert_eq!(
        command.request_sha256,
        <[u8; 32]>::from(Sha256::digest(record.container()))
    );
    assert_eq!(command.request_bytes, record.container().len() as u64);
    assert_eq!(
        command.response_limit,
        morrow_core::service::MAX_FRAME_BYTES as u64
    );
    assert!(command.subject.len() < 256);
    assert_ne!(
        record.command([5; 32]).unwrap().approval_sha256,
        command.approval_sha256
    );
    assert!(record.command([0; 32]).is_err());
    Record::prepared(command).unwrap();
}
#[test]
fn service_digest_is_only_allowed_for_publish_while_legacy_publish_survives() {
    let record = RequestRecord::encode(&policy(), "key", &request(), 10).unwrap();
    let service_command = record.command([4; 32]).unwrap();
    for capability in [
        IoCapability::FileRead,
        IoCapability::FileList,
        IoCapability::FileCreate,
        IoCapability::FileReplace,
        IoCapability::FileDelete,
        IoCapability::HttpRequest,
        IoCapability::HttpListen,
        IoCapability::CredentialUse,
        IoCapability::WebSocketConnect,
    ] {
        let command = Command {
            capability,
            ..service_command.clone()
        };
        assert!(matches!(
            Record::prepared(command),
            Err(Error::UnsupportedVersion)
        ));
    }
    Record::prepared(Command {
        protocol_sha256: io::schema_digest(),
        ..service_command
    })
    .unwrap();
}
