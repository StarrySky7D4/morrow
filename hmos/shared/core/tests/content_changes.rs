#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::CardRecord,
    content_change::ContentChange,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{Package, proto::Capability},
    store::{EventBudget, Store},
    transaction::Lookup,
};
fn change() -> ContentChange {
    ContentChange {
        operation_id: "edit".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "新标题".into(),
        body: vec![9, 0, 255],
        preview_text: "插件缺席仍可读".into(),
        attachments: None,
    }
}
fn card() -> CardRecord {
    CardRecord::new("card", "org.morrow.idea", 1, "old", vec![1]).unwrap()
}
#[test]
fn scope_ceiling_revocation_and_expiry_gate_complete_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut h = HostRuntime::new(Store::open(&path, EventBudget::default()).unwrap()).unwrap();
    let module = b"\0asm\x01\0\0\0";
    let package = Package::build(
        Package::manifest_for(
            "org.test.editor",
            "0.1.0",
            module,
            vec![
                Capability::CreateContent,
                Capability::EditContent,
                Capability::ReadContent,
            ],
        ),
        module,
    )
    .unwrap();
    let mut c = h.connect_package(&package).unwrap();
    assert!(h.create_content(&c, "create", &card(), || 1).is_err());
    h.grant(&mut c, GrantKind::CreateContent, "card", 100, 1)
        .unwrap();
    let receipt = h.create_content(&c, "create", &card(), || 2).unwrap();
    assert_eq!(
        receipt,
        h.create_content(&c, "create", &card(), || 2).unwrap()
    );
    assert!(h.read_content(&c, "card", || 2).is_err());
    assert!(h.edit_content(&c, &change(), || 2).is_err());
    h.grant(&mut c, GrantKind::EditContent, "other", 100, 2)
        .unwrap();
    assert!(h.edit_content(&c, &change(), || 3).is_err());
    h.grant(&mut c, GrantKind::EditContent, "card", 100, 3)
        .unwrap();
    h.grant(&mut c, GrantKind::ReadContent, "card", 100, 3)
        .unwrap();
    let r = h.edit_content(&c, &change(), || 4).unwrap();
    assert_eq!(r.revision, 2);
    assert_eq!(r, h.edit_content(&c, &change(), || 5).unwrap());
    assert_eq!(
        h.read_content(&c, "card", || 5).unwrap().body(),
        vec![9, 0, 255]
    );
    let mut different = change();
    different.body.push(42);
    assert_eq!(
        h.edit_content(&c, &different, || 6),
        Err(Error::OperationConflict)
    );
    different.operation_id = "new-operation".into();
    assert_eq!(
        h.edit_content(&c, &different, || 6),
        Err(Error::RevisionConflict)
    );
    h.revoke(&mut c, GrantKind::EditContent, "card").unwrap();
    assert!(h.edit_content(&c, &change(), || 6).is_err());
    h.disconnect(&c).unwrap();
    assert!(h.read_content(&c, "card", || 7).is_err());
    let no_caps = Package::build(
        Package::manifest_for("org.test.pure", "0.1.0", module, vec![]),
        module,
    )
    .unwrap();
    let mut no = h.connect_package(&no_caps).unwrap();
    assert!(
        h.grant(&mut no, GrantKind::CreateContent, "card", 100, 7)
            .is_err()
    );
    drop(h);
    let s = Store::open_existing(&path, EventBudget::default()).unwrap();
    s.integrity_check().unwrap();
    assert_eq!(
        s.card("card").unwrap().unwrap().summary().preview_text,
        "插件缺席仍可读"
    );
    assert_eq!(s.pending(0, 10).unwrap().len(), 2);
    assert_eq!(s.card_ids_local("", 1).unwrap(), vec!["card"]);
    assert!(s.card_ids_local("card", 1).unwrap().is_empty());
}
#[test]
fn expiry_and_revocation_at_final_commit_roll_back_all_three_records() {
    for revoke in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut h =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        h.store_local_mut().create_local("seed", &card()).unwrap();
        let mut c = h.connect().unwrap();
        h.grant(&mut c, GrantKind::EditContent, "card", 3, 0)
            .unwrap();
        let signal = h.revocation(&c).unwrap();
        let mut calls = 0;
        assert!(
            h.edit_content(&c, &change(), || {
                calls += 1;
                if calls == 2 && revoke {
                    signal.revoke();
                }
                if calls == 2 && !revoke { 3 } else { 1 }
            })
            .is_err()
        );
        assert_eq!(calls, 2);
        assert_eq!(
            h.store_local().card("card").unwrap().unwrap().body(),
            vec![1]
        );
        assert!(matches!(
            h.store_local().lookup("edit").unwrap(),
            Lookup::Absent
        ));
        assert_eq!(h.store_local().pending(0, 10).unwrap().len(), 1);
    }
}
#[test]
fn edits_preserve_future_outer_and_nested_preview_fields() {
    use prost::Message;
    use prost_reflect::{DescriptorPool, DynamicMessage, Value};
    let pool = DescriptorPool::decode(
        include_bytes!(concat!(env!("OUT_DIR"), "/future.descriptor.bin")).as_slice(),
    )
    .unwrap();
    let descriptor = pool.get_message_by_name("test.future.Card").unwrap();
    let mut future =
        DynamicMessage::decode(descriptor.clone(), card().encode().as_slice()).unwrap();
    future.set_field_by_name("status", Value::EnumNumber(9000));
    let mut preview = DynamicMessage::new(pool.get_message_by_name("test.future.Preview").unwrap());
    preview.set_field_by_name("future_metadata", Value::Bytes(vec![1, 2, 255].into()));
    future.set_field_by_name("preview", Value::Message(preview));
    let original = CardRecord::decode(&future.encode_to_vec()).unwrap();
    let edited = change().propose(&original).unwrap();
    let actual = DynamicMessage::decode(descriptor, edited.encode().as_slice()).unwrap();
    assert_eq!(
        actual.get_field_by_name("status"),
        future.get_field_by_name("status")
    );
    assert_eq!(
        actual
            .get_field_by_name("preview")
            .unwrap()
            .as_message()
            .unwrap()
            .get_field_by_name("future_metadata"),
        future
            .get_field_by_name("preview")
            .unwrap()
            .as_message()
            .unwrap()
            .get_field_by_name("future_metadata")
    );
    assert_eq!(edited.summary().type_id, original.summary().type_id);
    assert_eq!(edited.summary().format_version, 1);
}

#[test]
fn full_content_and_attachment_references_commit_or_rollback_together() {
    use morrow_core::content::Attachment;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let raw = b"verified original bytes";
    let blob = store
        .stage_blob(&mut &raw[..], raw.len() as u64, None, 0)
        .unwrap();
    store.create_local("seed", &card()).unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let mut connection = host.connect().unwrap();
    host.grant(&mut connection, GrantKind::EditContent, "card", 100, 0)
        .unwrap();
    let mut edit = change();
    edit.attachments = Some(vec![Attachment {
        id: "file".into(),
        display_name: "raw.docx".into(),
        media_type: "application/octet-stream".into(),
        byte_length: raw.len() as u64,
        sha256: blob.sha256,
    }]);
    host.edit_content(&connection, &edit, || 1).unwrap();
    let mut exported = Vec::new();
    host.store_local()
        .export_attachment_local("card", "file", &mut exported)
        .unwrap();
    assert_eq!(exported, raw);
    edit.operation_id = "bad-attachment".into();
    edit.expected_revision = 2;
    edit.body = b"must not commit".to_vec();
    edit.attachments.as_mut().unwrap()[0].sha256 = [42; 32];
    assert!(host.edit_content(&connection, &edit, || 2).is_err());
    let actual = host.store_local().card("card").unwrap().unwrap();
    assert_eq!(actual.summary().revision, 2);
    assert_eq!(actual.body(), change().body);
    assert_eq!(host.store_local().pending(0, 128).unwrap().len(), 2);
    host.store_local().integrity_check().unwrap();
}
