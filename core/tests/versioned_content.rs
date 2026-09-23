#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::{self, CardRecord},
    content_change::ContentChange,
    content_migration::ContentMigration,
    dispatch::{Connection, HostRuntime},
    lifecycle::GrantKind,
    response::{Outcome, Response},
    runtime::Command,
    store::{EventBudget, Store},
    transaction::{self, Lookup},
    versioned_content_change::VersionedContentChange,
};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};

fn fixture() -> (tempfile::TempDir, HostRuntime, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    store
        .create_local(
            "seed",
            &CardRecord::new("card", "idea", 1, "one", b"v1".to_vec()).unwrap(),
        )
        .unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let mut connection = host.connect().unwrap();
    host.grant(&mut connection, GrantKind::EditContent, "card", 1000, 0)
        .unwrap();
    (dir, host, connection)
}

fn legacy(operation: &str, revision: u64) -> ContentChange {
    ContentChange {
        operation_id: operation.into(),
        card_id: "card".into(),
        expected_revision: revision,
        title: "old editor".into(),
        body: b"text v1".to_vec(),
        preview_text: "text".into(),
        attachments: None,
    }
}

fn migrate(host: &mut HostRuntime, connection: &Connection) {
    let source = host.store_local().card("card").unwrap().unwrap();
    let change = ContentMigration {
        operation_id: "migration".into(),
        source_card: source.encode(),
        target_format_version: 2,
        body: vec![0, 2, 255],
        preview_text: "v2".into(),
    };
    host.migrate_content_guarded_with_evidence(connection, &change, &[], || 1, |_| Ok(()))
        .unwrap();
}

fn changed_source_field(source: &[u8], field: &str, value: Value) -> Vec<u8> {
    let descriptor = DescriptorPool::decode(content::DESCRIPTOR)
        .unwrap()
        .get_message_by_name("morrow.content.v1.Card")
        .unwrap();
    let mut card = DynamicMessage::decode(descriptor, source).unwrap();
    card.set_field_by_name(field, value);
    card.encode_to_vec()
}
fn versioned(host: &HostRuntime, operation: &str) -> VersionedContentChange {
    VersionedContentChange {
        operation_id: operation.into(),
        source_card: host.store_local().card("card").unwrap().unwrap().encode(),
        title: "new editor".into(),
        body: vec![0, 3, 255],
        preview_text: "v2 changed".into(),
        attachments: None,
    }
}

#[test]
fn old_live_edit_cannot_overwrite_migrated_format_even_with_current_revision_and_grant() {
    let (_dir, mut host, connection) = fixture();
    let old = legacy("old", 1);
    let historical = host.edit_content(&connection, &old, || 1).unwrap();
    migrate(&mut host, &connection);
    let migrated = host.store_local().card("card").unwrap().unwrap();
    let pending = host.store_local().pending(0, 20).unwrap().len();
    let change = legacy("new-old", migrated.summary().revision);
    assert_eq!(
        host.edit_content(&connection, &change, || 2),
        Err(Error::UnsupportedVersion)
    );
    let wire = Command::EditContent(legacy("wire-old", migrated.summary().revision))
        .encode()
        .unwrap();
    let reply = Response::decode(&host.dispatch(&connection, &wire, || 2).unwrap()).unwrap();
    assert!(matches!(reply.outcome, Outcome::Rejected(_)));
    assert_eq!(
        host.store_local().card("card").unwrap().unwrap().encode(),
        migrated.encode()
    );
    assert_eq!(host.store_local().pending(0, 20).unwrap().len(), pending);
    assert_eq!(
        host.store_local().lookup("new-old").unwrap(),
        Lookup::Absent
    );
    assert_eq!(
        host.store_local().lookup("wire-old").unwrap(),
        Lookup::Absent
    );
    assert!(
        host.store_local()
            .operation_evidence("card", "new-old")
            .is_err()
    );
    assert_eq!(
        host.edit_content(&connection, &old, || 3).unwrap(),
        historical
    );
    assert_eq!(
        host.store_local().card("card").unwrap().unwrap().encode(),
        migrated.encode()
    );
    host.store_local().integrity_check().unwrap();
}

#[test]
fn versioned_edit_binds_complete_source_and_reconstructs_receipt() {
    let (_dir, mut host, connection) = fixture();
    migrate(&mut host, &connection);
    let change = versioned(&host, "typed");
    let receipt = host
        .edit_versioned_content(&connection, &change, || 2)
        .unwrap();
    let edited = host.store_local().card("card").unwrap().unwrap();
    assert_eq!(edited.summary().format_version, 2);
    assert_eq!(edited.summary().type_id, "idea");
    assert_eq!(edited.body(), change.body);
    assert_eq!(receipt.revision, 3);
    let (commit, stored) = host
        .store_local()
        .operation_commit("card", "typed")
        .unwrap()
        .unwrap();
    assert_eq!(receipt, stored);
    assert_eq!(
        transaction::decode_command(&commit.command)
            .unwrap()
            .schema_version,
        2
    );
    assert_eq!(
        host.edit_versioned_content(&connection, &change, || 3)
            .unwrap(),
        receipt
    );
    assert_eq!(
        host.store_local().card("card").unwrap().unwrap().encode(),
        edited.encode()
    );
    let mut stale = change.clone();
    stale.operation_id = "stale".into();
    assert_eq!(
        host.edit_versioned_content(&connection, &stale, || 3),
        Err(Error::RevisionConflict)
    );
    let mut wrong_format = versioned(&host, "wrong-format");
    wrong_format.source_card =
        changed_source_field(&wrong_format.source_card, "format_version", Value::U32(3));
    assert_eq!(
        host.edit_versioned_content(&connection, &wrong_format, || 3),
        Err(Error::RevisionConflict)
    );
    let mut wrong_type = versioned(&host, "wrong-type");
    wrong_type.source_card = changed_source_field(
        &wrong_type.source_card,
        "type_id",
        Value::String("other".into()),
    );
    assert_eq!(
        host.edit_versioned_content(&connection, &wrong_type, || 3),
        Err(Error::RevisionConflict)
    );
    for operation in ["stale", "wrong-format", "wrong-type"] {
        assert_eq!(
            host.store_local().lookup(operation).unwrap(),
            Lookup::Absent
        );
    }
    host.store_local().integrity_check().unwrap();
}

#[test]
fn versioned_guard_and_revocation_roll_back_without_record_or_event() {
    let (_dir, mut host, mut connection) = fixture();
    migrate(&mut host, &connection);
    let change = versioned(&host, "denied");
    let before = host.store_local().card("card").unwrap().unwrap().encode();
    let pending = host.store_local().pending(0, 20).unwrap().len();
    let mut checks = 0;
    assert_eq!(
        host.edit_versioned_content_guarded_with_evidence(
            &connection,
            &change,
            &[],
            || 3,
            |_| {
                checks += 1;
                if checks == 2 {
                    Err(Error::Invalid("guard denied"))
                } else {
                    Ok(())
                }
            },
        ),
        Err(Error::Invalid("guard denied"))
    );
    assert_eq!(checks, 2);
    assert_eq!(
        host.store_local().card("card").unwrap().unwrap().encode(),
        before
    );
    assert_eq!(host.store_local().pending(0, 20).unwrap().len(), pending);
    assert_eq!(host.store_local().lookup("denied").unwrap(), Lookup::Absent);
    host.revoke(&mut connection, GrantKind::EditContent, "card")
        .unwrap();
    assert!(
        host.edit_versioned_content(&connection, &change, || 4)
            .is_err()
    );
    assert_eq!(host.store_local().lookup("denied").unwrap(), Lookup::Absent);
}

#[test]
fn historical_set_content_proposal_keeps_its_pure_high_format_interpretation() {
    let source = CardRecord::new("card", "old", 3, "source", vec![1]).unwrap();
    let change = legacy("historical", 1);
    let result = change.propose(&source).unwrap();
    assert_eq!(result.summary().format_version, 3);
    assert_eq!(result.body(), change.body);
    let event = transaction::encode_commit(transaction::content_command(&change).unwrap(), &result)
        .unwrap();
    assert_eq!(transaction::decode_commit(&event).unwrap().1.revision, 2);
}
#[test]
fn schema_one_field_eight_is_still_an_unknown_legacy_field() {
    let card = CardRecord::new("card", "old", 1, "source", vec![1]).unwrap();
    let original = transaction::create_command("old-op", &card).unwrap();
    let mut historical = original.clone();
    historical.extend_from_slice(&[0x42, 3, 0xff, 0xff, 0xff]);
    assert_eq!(
        transaction::decode_command(&historical).unwrap(),
        transaction::decode_command(&original).unwrap()
    );
    let commit = transaction::encode_commit(historical, &card).unwrap();
    assert_eq!(
        transaction::decode_commit(&commit).unwrap().1.operation_id,
        "old-op"
    );
}
#[test]
fn schema_two_rejects_mixed_and_repeated_actions() {
    let source = CardRecord::new("card", "idea", 2, "source", vec![1]).unwrap();
    let change = VersionedContentChange {
        operation_id: "typed".into(),
        source_card: source.encode(),
        title: "changed".into(),
        body: vec![2],
        preview_text: String::new(),
        attachments: None,
    };
    let command = transaction::versioned_content_command(&change).unwrap();
    // Field 6 is the old SetContent action; field 7 is MigrateContent.
    for extra in [&[0x32, 0][..], &[0x3a, 0][..], &[0x42, 0][..]] {
        let mut after = command.clone();
        after.extend_from_slice(extra);
        assert!(transaction::decode_command(&after).is_err());
        let mut before = extra.to_vec();
        before.extend_from_slice(&command);
        assert!(transaction::decode_command(&before).is_err());
    }
}
