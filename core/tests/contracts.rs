use capnp::{
    message::{AllocationStrategy, Builder, HeapAllocator},
    serialize,
};
use morrow_core::{
    Error,
    content::{CardRecord, DESCRIPTOR, MAX_RECORD_BYTES},
    envelope,
    runtime::{MAX_MESSAGE_BYTES, RenameRequest},
    runtime_capnp,
};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};

fn card() -> CardRecord {
    CardRecord::new(
        "legacy-123",
        "plugin.missing.document",
        9,
        "标题",
        vec![0, 255, 17, 128],
    )
    .unwrap()
}
fn request(revision: u64) -> RenameRequest {
    RenameRequest {
        operation_id: "op-1".into(),
        card_id: "legacy-123".into(),
        expected_revision: revision,
        title: "新的标题 🪷".into(),
    }
}
fn message(card: &CardRecord) -> DynamicMessage {
    let pool = DescriptorPool::decode(DESCRIPTOR).unwrap();
    DynamicMessage::decode(
        pool.get_message_by_name("morrow.content.v1.Card").unwrap(),
        card.encode().as_slice(),
    )
    .unwrap()
}

#[test]
fn opaque_content_survives_edit_and_container_without_plugin() {
    let source = card();
    let edited = source.with_title(1, "new").unwrap();
    let restored = envelope::decode(&envelope::encode(&edited).unwrap()).unwrap();
    assert_eq!(restored.body(), source.body());
    assert_eq!(restored.summary().type_id, "plugin.missing.document");
    assert_eq!(restored.summary().format_version, 9);
    assert_eq!(restored.summary().revision, 2);
    assert_eq!(restored.summary().title, "new");
    assert_eq!(source.summary().revision, 1);
}

#[test]
fn future_top_level_nested_enum_and_oneof_survive_old_title_edit() {
    let pool = DescriptorPool::decode(
        include_bytes!(concat!(env!("OUT_DIR"), "/future.descriptor.bin")).as_slice(),
    )
    .unwrap();
    let descriptor = pool.get_message_by_name("test.future.Card").unwrap();
    let mut newer = DynamicMessage::decode(descriptor.clone(), card().encode().as_slice()).unwrap();
    newer.set_field_by_name("status", Value::EnumNumber(9000));
    newer.set_field_by_name("rich_document", Value::Bytes(vec![0, 255, 17].into()));
    let mut preview = DynamicMessage::new(pool.get_message_by_name("test.future.Preview").unwrap());
    preview.set_field_by_name("plain_text", Value::String("cached preview".into()));
    preview.set_field_by_name("future_metadata", Value::Bytes(vec![123, 255].into()));
    newer.set_field_by_name("preview", Value::Message(preview));
    let old = CardRecord::decode(&newer.encode_to_vec()).unwrap();
    assert_eq!(old.summary().preview_text, "cached preview");
    let edited = old.with_title(1, "only known field changed").unwrap();
    let restored = envelope::decode(&envelope::encode(&edited).unwrap()).unwrap();
    let actual = DynamicMessage::decode(descriptor, restored.encode().as_slice()).unwrap();
    newer.set_field_by_name("title", Value::String("only known field changed".into()));
    newer.set_field_by_name("revision", Value::U64(2));
    assert_eq!(actual, newer);
}

#[test]
fn exact_decode_bytes_are_retained_separately_from_reencoding() {
    let mut bytes = card().encode();
    // Duplicate known scalar is legal protobuf and is normalized by reencoding.
    bytes.extend_from_slice(&[0x28, 0x01]);
    let decoded = CardRecord::decode(&bytes).unwrap();
    assert_eq!(decoded.encode(), bytes);
    assert_eq!(
        envelope::decode(&envelope::encode(&decoded).unwrap())
            .unwrap()
            .encode(),
        bytes
    );
    let edited = decoded.with_title(1, "edited").unwrap();
    assert_eq!(edited.original_bytes(), bytes);
    assert_ne!(edited.encode(), bytes);
}

#[test]
fn revision_conflict_does_not_modify_record() {
    let source = card();
    assert!(matches!(
        source.with_title(0, "bad"),
        Err(Error::RevisionConflict)
    ));
    assert_eq!(source.summary().title, "标题");
}

#[test]
fn revision_overflow_is_rejected() {
    let mut msg = message(&card());
    msg.set_field_by_name("revision", Value::U64(u64::MAX));
    let source = CardRecord::decode(&msg.encode_to_vec()).unwrap();
    assert!(source.with_title(u64::MAX, "bad").is_err());
}

#[test]
fn unsupported_schema_does_not_become_an_empty_card() {
    let mut msg = message(&card());
    msg.set_field_by_name("schema_version", Value::U32(2));
    assert!(matches!(
        CardRecord::decode(&msg.encode_to_vec()),
        Err(Error::UnsupportedVersion)
    ));
    assert!(CardRecord::decode(&[]).is_err());
}

#[test]
fn persistent_identity_rejects_platform_paths_and_empty_values() {
    for id in [
        "",
        "C:\\private\\file",
        "/tmp/file",
        "file://data",
        "bad\nvalue",
    ] {
        assert!(CardRecord::new(id, "type", 1, "", vec![]).is_err());
    }
    assert!(CardRecord::new("old-123-0", "extension.custom", 1, "", vec![]).is_ok());
}

#[test]
fn missing_creation_time_is_not_fabricated() {
    assert!(!message(&card()).has_field_by_name("created_at_unix_ms"));
}

#[test]
fn lengths_are_bounded_before_decompression_or_message_allocation() {
    let mut bytes = envelope::encode(&card()).unwrap();
    bytes[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(envelope::decode(&bytes), Err(Error::Limit)));
    assert!(matches!(
        CardRecord::decode(&vec![0; MAX_RECORD_BYTES + 1]),
        Err(Error::Limit)
    ));
    assert!(matches!(
        RenameRequest::decode(&vec![0; MAX_MESSAGE_BYTES + 1]),
        Err(Error::Limit)
    ));
    assert!(request(1).title.len() < 16 * 1024);
    assert!(card().with_title(1, &"a".repeat(16 * 1024 + 1)).is_err());
}

#[test]
fn repeated_messages_and_unknown_field_count_have_predecode_budget() {
    let mut bytes = card().encode();
    for _ in 0..1025 {
        bytes.extend_from_slice(&[0x42, 0x00]);
    }
    assert!(matches!(CardRecord::decode(&bytes), Err(Error::Limit)));
    let mut fields = card().encode();
    for _ in 0..8193 {
        fields.extend_from_slice(&[0xa0, 0x06, 0x00]);
    }
    assert!(matches!(CardRecord::decode(&fields), Err(Error::Limit)));
}

#[test]
fn corrupt_truncated_and_trailing_containers_are_rejected() {
    let bytes = envelope::encode(&card()).unwrap();
    for end in 0..bytes.len() {
        assert!(envelope::decode(&bytes[..end]).is_err(), "prefix {end}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(envelope::decode(&trailing).is_err());
    let mut corrupt = bytes.clone();
    corrupt[18] ^= 1;
    assert!(matches!(envelope::decode(&corrupt), Err(Error::Integrity)));
    let mut future = bytes;
    future[8] = 2;
    assert!(matches!(
        envelope::decode(&future),
        Err(Error::UnsupportedVersion)
    ));
}

#[test]
fn malformed_protobuf_lengths_groups_and_wire_types_are_rejected() {
    for suffix in [&[0x3a, 0xff, 0xff][..], &[0x0f][..], &[0xa3, 0x06][..]] {
        let mut bytes = card().encode();
        bytes.extend_from_slice(suffix);
        assert!(CardRecord::decode(&bytes).is_err());
    }
}

#[test]
fn blob_reference_requires_content_digest_but_no_platform_location() {
    let mut msg = message(&card());
    let pool = DescriptorPool::decode(DESCRIPTOR).unwrap();
    let mut blob = DynamicMessage::new(
        pool.get_message_by_name("morrow.content.v1.BlobRef")
            .unwrap(),
    );
    blob.set_field_by_name("id", Value::String("blob-1".into()));
    msg.set_field_by_name(
        "attachments",
        Value::List(vec![Value::Message(blob.clone())]),
    );
    assert!(CardRecord::decode(&msg.encode_to_vec()).is_err());
    blob.set_field_by_name("sha256", Value::Bytes(vec![17; 32].into()));
    msg.set_field_by_name("attachments", Value::List(vec![Value::Message(blob)]));
    assert!(CardRecord::decode(&msg.encode_to_vec()).is_ok());
}

#[test]
fn capnp_unicode_and_full_uint64_round_trip() {
    for revision in [1, (1u64 << 53) + 1, u64::MAX] {
        let request = request(revision);
        assert_eq!(
            RenameRequest::decode(&request.encode().unwrap()).unwrap(),
            request
        );
    }
}

#[test]
fn capnp_truncation_trailing_and_unsupported_operations_fail() {
    let bytes = request(1).encode().unwrap();
    for end in 0..bytes.len() {
        assert!(RenameRequest::decode(&bytes[..end]).is_err());
    }
    let mut twice = bytes.clone();
    twice.extend_from_slice(&bytes);
    assert!(RenameRequest::decode(&twice).is_err());
    let mut message = Builder::new_default();
    let mut root = message.init_root::<runtime_capnp::request::Builder>();
    root.set_protocol_version(morrow_core::runtime::PROTOCOL_VERSION);
    root.set_runtime_digest(&morrow_core::runtime::runtime_digest());
    root.set_content_digest(&morrow_core::runtime::content_digest());
    root.set_operation_id("op-1");
    root.set_unsupported(());
    assert!(RenameRequest::decode(&serialize::write_message_to_words(&message)).is_err());
    root = message
        .get_root::<runtime_capnp::request::Builder>()
        .unwrap();
    root.set_protocol_version(morrow_core::runtime::PROTOCOL_VERSION + 1);
    assert!(matches!(
        RenameRequest::decode(&serialize::write_message_to_words(&message)),
        Err(Error::UnsupportedVersion)
    ));
}

#[test]
fn capnp_multisegment_messages_are_accepted_within_budget() {
    let allocator = HeapAllocator::new()
        .first_segment_words(1)
        .allocation_strategy(AllocationStrategy::FixedSize);
    let mut message = Builder::new(allocator);
    let mut root = message.init_root::<runtime_capnp::request::Builder>();
    root.set_protocol_version(morrow_core::runtime::PROTOCOL_VERSION);
    root.set_runtime_digest(&morrow_core::runtime::runtime_digest());
    root.set_content_digest(&morrow_core::runtime::content_digest());
    root.set_operation_id("op-1");
    let mut rename = root.init_rename_card();
    rename.set_card_id("legacy-123");
    rename.set_expected_revision(1);
    rename.set_title("新的标题 🪷");
    assert!(message.get_segments_for_output().len() > 1);
    assert_eq!(
        RenameRequest::decode(&serialize::write_message_to_words(&message)).unwrap(),
        request(1)
    );
}

#[test]
fn content_and_view_identity_are_independent_contracts() {
    let pool = DescriptorPool::decode(DESCRIPTOR).unwrap();
    let desc = pool
        .get_message_by_name("morrow.content.v1.ViewPlacement")
        .unwrap();
    let mut a = DynamicMessage::new(desc.clone());
    a.set_field_by_name("id", Value::String("placement-a".into()));
    a.set_field_by_name("workspace_id", Value::String("workspace-a".into()));
    a.set_field_by_name("card_id", Value::String("legacy-123".into()));
    let mut b = a.clone();
    b.set_field_by_name("id", Value::String("placement-b".into()));
    b.set_field_by_name("workspace_id", Value::String("workspace-b".into()));
    b.set_field_by_name("collapsed", Value::Bool(true));
    assert_eq!(
        a.get_field_by_name("card_id"),
        b.get_field_by_name("card_id")
    );
    assert_ne!(a, b);
    assert!(desc.get_field_by_name("body").is_none());
}

#[test]
fn request_for_another_card_cannot_propose_an_edit() {
    let mut wrong = request(1);
    wrong.card_id = "other-card".into();
    assert!(wrong.propose(&card()).is_err());
    assert_eq!(request(1).propose(&card()).unwrap().summary().revision, 2);
}

#[test]
fn capnp_impossible_segment_or_far_pointer_is_rejected() {
    // One segment, one word, with a far pointer addressing an absent segment.
    let bytes = [0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 255, 255, 255, 255];
    assert!(RenameRequest::decode(&bytes).is_err());
    let mut bytes = request(1).encode().unwrap();
    bytes[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(RenameRequest::decode(&bytes).is_err());
}

#[test]
fn altered_contract_digest_is_rejected_before_accepting_command() {
    let mut bytes = request(1).encode().unwrap();
    let digest = morrow_core::runtime::runtime_digest();
    let index = bytes.windows(32).position(|part| part == digest).unwrap();
    bytes[index] ^= 1;
    assert!(matches!(
        RenameRequest::decode(&bytes),
        Err(Error::UnsupportedVersion)
    ));
    assert_eq!(
        morrow_core::runtime::schema_digest(b"line\r\n"),
        morrow_core::runtime::schema_digest(b"line\n")
    );
}
