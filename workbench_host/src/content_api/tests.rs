use super::*;
use crate::{Record, versioned_record::VersionedRecord};
use morrow_workbench_plugin::Idea;

fn conditions() -> Conditions {
    Conditions {
        section: "概览".into(),
        filter: "全部".into(),
        text: String::new(),
        sort: "最近添加".into(),
    }
}
#[test]
fn private_edit_and_query_payloads_require_canonical_action_specific_fields() {
    let task = TaskCommand::Remove("task-1".into());
    let raw = encode_task_edit(&task);
    assert!(matches!(task_edit(&raw).unwrap(), TaskCommand::Remove(id) if id == "task-1"));
    let mut trailing = raw.clone();
    trailing.extend([0; 8]);
    assert!(task_edit(&trailing).is_err());
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::task_edit::Builder>();
        r.set_version(1);
        r.set_digest(&digest());
        r.set_action(wire::TaskAction::Remove);
        r.set_task_id("task-1");
        r.set_text("unrelated");
    }
    assert!(task_edit(&serialize::write_message_to_words(&message)).is_err());
    let mut wrong_digest = Builder::new_default();
    {
        let mut r = wrong_digest.init_root::<wire::task_edit::Builder>();
        r.set_version(1);
        r.set_digest(&[0; 32]);
        r.set_action(wire::TaskAction::Remove);
        r.set_task_id("task-1");
    }
    assert!(task_edit(&serialize::write_message_to_words(&wrong_digest)).is_err());
    assert!(task_edit(&vec![0; MAX_FRAME + 1]).is_err());
    let card = CardAction::SetFavorite(true);
    assert!(matches!(
        card_edit(&encode_card_edit(&card)).unwrap(),
        CardAction::SetFavorite(true)
    ));
    let mut extra_card = Builder::new_default();
    {
        let mut r = extra_card.init_root::<wire::card_edit::Builder>();
        r.set_version(1);
        r.set_digest(&digest());
        r.set_action(wire::CardAction::Delete);
        r.set_category("实验");
    }
    assert!(card_edit(&serialize::write_message_to_words(&extra_card)).is_err());
    assert_eq!(query(&encode_query(&conditions())).unwrap(), conditions());
    let mut extra_query = encode_query(&conditions());
    extra_query.extend([0; 8]);
    assert!(query(&extra_query).is_err());
}
#[test]
fn equivalent_multisegment_payloads_are_accepted() {
    use capnp::message::HeapAllocator;

    let mut task_message = Builder::new(HeapAllocator::new().first_segment_words(1));
    {
        let mut r = task_message.init_root::<wire::task_edit::Builder>();
        r.set_version(1);
        r.set_digest(&digest());
        r.set_action(wire::TaskAction::Rename);
        r.set_task_id("task-1");
        r.set_text("renamed");
    }
    let task_bytes = serialize::write_message_to_words(&task_message);
    assert_ne!(
        task_bytes,
        encode_task_edit(&TaskCommand::Rename {
            id: "task-1".into(),
            text: "renamed".into(),
        })
    );
    assert!(
        matches!(task_edit(&task_bytes).unwrap(), TaskCommand::Rename { id, text } if id == "task-1" && text == "renamed")
    );

    let edit = CardAction::Edit(Fields {
        title: "Large title".into(),
        description: "description".repeat(1024),
        hypothesis: String::new(),
        conclusion: String::new(),
        icon: 1,
        color: 42,
        assets: vec![],
    });
    let mut card_message = Builder::new(HeapAllocator::new().first_segment_words(1));
    {
        let mut r = card_message.init_root::<wire::card_edit::Builder>();
        r.set_version(1);
        r.set_digest(&digest());
        r.set_action(wire::CardAction::Edit);
        if let CardAction::Edit(fields) = &edit {
            fields_to_wire(r.init_fields(), fields);
        }
    }
    let card_bytes = serialize::write_message_to_words(&card_message);
    assert_ne!(card_bytes, encode_card_edit(&edit));
    assert_eq!(card_edit(&card_bytes).unwrap(), edit);

    let mut query_message = Builder::new(HeapAllocator::new().first_segment_words(1));
    {
        let mut r = query_message.init_root::<wire::query::Builder>();
        r.set_version(1);
        r.set_digest(&digest());
        r.set_section("概览");
        r.set_filter("全部");
        r.set_text("");
        r.set_sort("最近添加");
    }
    let query_bytes = serialize::write_message_to_words(&query_message);
    assert_ne!(query_bytes, encode_query(&conditions()));
    assert_eq!(query(&query_bytes).unwrap(), conditions());
    let normalized = encode_query(&query(&query_bytes).unwrap());
    assert_eq!(
        cursor_token("operation", &normalized, 10),
        cursor_token("operation", &encode_query(&conditions()), 10)
    );
}
#[test]
fn envelope_retains_full_uint64_revisions() {
    for revision in [(1u64 << 53) - 1, (1u64 << 53) + 1, 1u64 << 63, u64::MAX] {
        let record = VersionedRecord::Legacy(Record {
            idea: Idea {
                id: "card".into(),
                title: "Exact".into(),
                category: "灵感".into(),
                stage: "待整理".into(),
                ..Default::default()
            },
            revision,
        });
        let bytes = encode_envelope(
            wire::EnvelopeKind::Record,
            "card",
            "",
            revision,
            revision,
            false,
            Some(&record),
        )
        .unwrap();
        let message = frame(&bytes).unwrap();
        let root = message.get_root::<wire::envelope::Reader>().unwrap();
        version(root.get_version(), root.get_digest()).unwrap();
        assert_eq!(root.get_source_revision(), revision);
        assert_eq!(root.get_revision(), revision);
        let card = root.get_record().unwrap();
        assert_eq!(card.get_revision(), revision);
        assert_eq!(card.get_format_version(), 1);
        assert_eq!(
            card.get_projected_stage().unwrap().to_str().unwrap(),
            "推进中"
        );
    }
}
#[test]
fn query_pages_fit_actual_outer_frame_and_cursor_binds_identity() {
    let ids: Vec<String> = (0..4096)
        .map(|n| format!("id-{n:04}-{}", "x".repeat(240)))
        .collect();
    let payload = encode_query(&conditions());
    let mut offset = 0;
    let mut seen = Vec::new();
    let mut cursor = String::new();
    while offset < ids.len() {
        assert_eq!(
            cursor_offset(&cursor, "operation", &payload).unwrap(),
            offset
        );
        let (page, next) =
            query_page(&ids, offset, 4096, "operation", &payload, false, None).unwrap();
        assert!(!page.is_empty());
        assert!(outer_page_size(page, &next, false, None) <= MAX_FRAME);
        seen.extend(page.iter().cloned());
        offset += page.len();
        cursor = next;
    }
    assert!(cursor.is_empty());
    assert_eq!(seen, ids);
    let token = cursor_token("operation", &payload, 10);
    assert_eq!(cursor_offset(&token, "operation", &payload).unwrap(), 10);
    assert!(cursor_offset(&token, "other-operation", &payload).is_err());
    assert!(
        cursor_offset(
            &token,
            "operation",
            &encode_query(&Conditions {
                text: "other".into(),
                ..conditions()
            })
        )
        .is_err()
    );
    assert!(cursor_offset("v1:010:bad", "operation", &payload).is_err());
    assert!(query_page(&ids, ids.len() + 1, 128, "operation", &payload, false, None).is_err());
    assert!(query_page(&ids, 0, 4097, "operation", &payload, false, None).is_err());
}

#[cfg(target_os = "windows")]
#[test]
fn invalid_page_request_cannot_create_a_query_capture() {
    let dir = tempfile::tempdir().unwrap();
    let host = crate::Workbench::open(&dir.path().join("db"), None).unwrap();
    let state = host.local_state().unwrap();
    assert!(validate_query_page_request(state, "no-query", 0, 4097).is_err());
    assert!(validate_query_page_request(state, "no-query", 4096, 128).is_err());
    assert!(validate_query_page_request(state, "no-query", 1, 128).is_err());
    assert!(
        state
            .host
            .store_local()
            .lookup_read_capture(crate::query_capture_v2::SUBJECT, "no-query")
            .unwrap()
            .is_none()
    );
    assert!(validate_query_page_request(state, "no-query", 0, 128).is_ok());
}
