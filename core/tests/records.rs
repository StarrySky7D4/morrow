#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::CardRecord,
    records::{self, Kind, Record, proto::Patch},
    store::{EventBudget, Store},
    transaction::Lookup,
};
fn setup() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    store
        .create_local(
            "card-seed",
            &CardRecord::new("card", "type", 1, "original", vec![1, 2, 3]).unwrap(),
        )
        .unwrap();
    (dir, store)
}
#[test]
fn multi_workspace_views_and_drafts_do_not_duplicate_or_overwrite_content() {
    let (dir, mut s) = setup();
    let original = s.card("card").unwrap().unwrap().encode();
    for id in ["first", "second"] {
        s.create_record_local(id, &Record::workspace(id, id).unwrap())
            .unwrap();
        s.create_record_local(
            &format!("place-{id}"),
            &Record::placement(id, id, "card", i64::MIN).unwrap(),
        )
        .unwrap();
    }
    s.patch_record_local(
        "layout",
        Kind::Placement,
        "first",
        1,
        Patch {
            collapsed: Some(true),
            order_key: Some(i64::MAX),
            width_units: Some(3),
            ..Default::default()
        },
    )
    .unwrap();
    s.create_record_local(
        "draft-create",
        &Record::draft("draft", "card", 1, "type", 1, vec![9]).unwrap(),
    )
    .unwrap();
    s.patch_record_local(
        "draft-save",
        Kind::Draft,
        "draft",
        1,
        Patch {
            body: Some(vec![255, 0, 6]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(s.card("card").unwrap().unwrap().encode(), original);
    s.integrity_check().unwrap();
    drop(s);
    let s = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
    assert_eq!(
        s.record_local(Kind::Placement, "first")
            .unwrap()
            .unwrap()
            .revision(),
        2
    );
    assert_eq!(
        s.record_local(Kind::Placement, "second")
            .unwrap()
            .unwrap()
            .revision(),
        1
    );
    assert_eq!(
        s.record_local(Kind::Draft, "draft")
            .unwrap()
            .unwrap()
            .revision(),
        2
    );
    assert_eq!(s.pending(0, 20).unwrap().len(), 8);
}
#[test]
fn retries_are_exact_global_and_queries_do_not_cross_kinds() {
    let (_dir, mut s) = setup();
    let w = Record::workspace("card", "workspace").unwrap();
    let r = s.create_record_local("w", &w).unwrap();
    assert_eq!(s.create_record_local("w", &w).unwrap(), r);
    s.patch_record_local(
        "edit",
        Kind::Workspace,
        "card",
        1,
        Patch {
            title: Some("new".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(s.create_record_local("w", &w).unwrap(), r);
    assert_eq!(s.lookup_for_card("card", "w").unwrap(), Lookup::Absent);
    assert!(
        s.lookup_record_local(Kind::Draft, "card", "w")
            .unwrap()
            .is_none()
    );
    assert!(
        s.lookup_record_local(Kind::Workspace, "else", "w")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        s.create_record_local("card-seed", &w),
        Err(Error::OperationConflict)
    );
    assert_eq!(
        s.create_local(
            "w",
            &CardRecord::new("another", "type", 1, "", vec![]).unwrap()
        ),
        Err(Error::OperationConflict)
    );
    assert_eq!(
        s.patch_record_local(
            "stale",
            Kind::Workspace,
            "card",
            1,
            Patch {
                title: Some("lost".into()),
                ..Default::default()
            }
        ),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        s.patch_record_local(
            "edit",
            Kind::Workspace,
            "card",
            1,
            Patch {
                title: Some("different".into()),
                ..Default::default()
            }
        ),
        Err(Error::OperationConflict)
    );
    s.integrity_check().unwrap();
}
#[test]
fn invalid_references_and_patch_fields_never_commit() {
    let (_dir, mut s) = setup();
    assert_eq!(
        s.create_record_local(
            "missing",
            &Record::placement("p", "missing", "card", 0).unwrap()
        ),
        Err(Error::NotFound)
    );
    assert_eq!(
        s.create_record_local(
            "future",
            &Record::draft("d", "card", 2, "type", 1, vec![]).unwrap()
        ),
        Err(Error::RevisionConflict)
    );
    assert_eq!(
        s.create_record_local(
            "type",
            &Record::draft("d", "card", 1, "other", 1, vec![]).unwrap()
        ),
        Err(Error::RevisionConflict)
    );
    s.create_record_local("w", &Record::workspace("w", "").unwrap())
        .unwrap();
    assert!(
        s.patch_record_local(
            "bad",
            Kind::Workspace,
            "w",
            1,
            Patch {
                body: Some(vec![1]),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(s.pending(0, 10).unwrap().len(), 2);
    s.integrity_check().unwrap();
}
#[test]
fn event_budget_is_shared_with_card_writes_and_rollback_is_total() {
    let (dir, mut s) = setup();
    s.create_record_local("w", &Record::workspace("w", "").unwrap())
        .unwrap();
    drop(s);
    let mut s = Store::open_existing(
        &dir.path().join("db"),
        EventBudget {
            max_count: 2,
            max_bytes: u64::MAX,
        },
    )
    .unwrap();
    assert_eq!(
        s.patch_record_local(
            "blocked",
            Kind::Workspace,
            "w",
            1,
            Patch {
                title: Some("never".into()),
                ..Default::default()
            }
        ),
        Err(Error::EventCapacity)
    );
    assert_eq!(
        s.record_local(Kind::Workspace, "w")
            .unwrap()
            .unwrap()
            .revision(),
        1
    );
    assert!(
        s.lookup_record_local(Kind::Workspace, "w", "blocked")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        s.create_local(
            "card-blocked",
            &CardRecord::new("new", "type", 1, "", vec![]).unwrap()
        ),
        Err(Error::EventCapacity)
    );
    s.integrity_check().unwrap();
}
#[test]
fn unknown_fields_survive_known_patches_and_container_roundtrip() {
    use prost_reflect::{DescriptorPool, DynamicMessage};
    for record in [
        Record::workspace("w", "").unwrap(),
        Record::placement("p", "w", "card", 0).unwrap(),
        Record::draft("d", "card", 1, "type", 1, vec![1]).unwrap(),
    ] {
        let kind = record.kind();
        let mut raw = record.encode();
        raw.extend_from_slice(&[0xa0, 0x06, 0x7b]);
        let decoded = Record::decode(kind, &raw).unwrap();
        assert_eq!(decoded.encode(), raw);
        let patch = match kind {
            Kind::Workspace => Patch {
                title: Some("new".into()),
                ..Default::default()
            },
            Kind::Placement => Patch {
                collapsed: Some(true),
                ..Default::default()
            },
            Kind::Draft => Patch {
                body: Some(vec![9]),
                ..Default::default()
            },
        };
        let next = decoded.patched(1, &patch).unwrap();
        assert_eq!(
            Record::from_container(kind, &next.container().unwrap())
                .unwrap()
                .encode(),
            next.encode()
        );
        let pool = DescriptorPool::decode(morrow_core::content::DESCRIPTOR).unwrap();
        let name = match kind {
            Kind::Workspace => "Workspace",
            Kind::Placement => "ViewPlacement",
            Kind::Draft => "Draft",
        };
        let desc = pool
            .get_message_by_name(&format!("morrow.content.v1.{name}"))
            .unwrap();
        let message = DynamicMessage::decode(desc, next.encode().as_slice()).unwrap();
        assert_eq!(message.unknown_fields().count(), 1);
        let command = records::patch_command("patch", kind, &next.id(), 1, patch).unwrap();
        let event = records::encode_commit(command, &next).unwrap();
        assert_eq!(records::decode_commit(&event).unwrap().1.revision, 2);
    }
}
#[test]
fn tampered_record_or_missing_record_is_detected_on_reopen() {
    for delete in [false, true] {
        let (dir, mut s) = setup();
        s.create_record_local("w", &Record::workspace("w", "saved").unwrap())
            .unwrap();
        drop(s);
        let db = rusqlite::Connection::open(dir.path().join("db")).unwrap();
        if delete {
            db.execute("DELETE FROM records", []).unwrap();
        } else {
            db.execute(
                "UPDATE records SET payload=?1",
                [Record::workspace("w", "tampered")
                    .unwrap()
                    .container()
                    .unwrap()],
            )
            .unwrap();
        }
        drop(db);
        assert!(Store::open_existing(&dir.path().join("db"), EventBudget::default()).is_err());
    }
}

#[test]
fn record_revision_overflow_and_framing_limits_are_explicit() {
    use prost::Message;
    use prost_reflect::{DescriptorPool, DynamicMessage, Value};
    let desc = DescriptorPool::decode(morrow_core::content::DESCRIPTOR)
        .unwrap()
        .get_message_by_name("morrow.content.v1.Workspace")
        .unwrap();
    let mut msg = DynamicMessage::decode(
        desc,
        Record::workspace("w", "title").unwrap().encode().as_slice(),
    )
    .unwrap();
    msg.set_field_by_name("revision", Value::U64(u64::MAX - 1));
    let record = Record::decode(Kind::Workspace, &msg.encode_to_vec()).unwrap();
    let patch = Patch {
        title: Some("last".into()),
        ..Default::default()
    };
    let last = record.patched(u64::MAX - 1, &patch).unwrap();
    assert_eq!(last.revision(), u64::MAX);
    assert!(matches!(last.patched(u64::MAX, &patch), Err(Error::Limit)));
    let bytes = last.container().unwrap();
    for length in 0..bytes.len() {
        assert!(Record::from_container(Kind::Workspace, &bytes[..length]).is_err());
    }
    assert!(Record::from_container(Kind::Draft, &bytes).is_err());
    assert!(
        records::patch_command(
            "op",
            Kind::Placement,
            "p",
            1,
            Patch {
                width_units: Some(0),
                ..Default::default()
            }
        )
        .is_err()
    );
}
