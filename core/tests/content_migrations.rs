#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::{self, Attachment, CardRecord},
    content_migration::ContentMigration,
    dispatch::{Connection, HostRuntime},
    lifecycle::GrantKind,
    plugin_package::{Package, proto::TransformHandler},
    store::{EventBudget, Store},
    task::{Invocation, Transform},
    task_evidence::{
        self, Evidence,
        proto::{ExecutionBudget, TaskEvidence},
    },
    transaction::{self, Lookup},
};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, Value};

// Synthetic task observations exercise atomic storage only, not guest correctness.
fn evidence() -> Evidence {
    let module = b"\0asm\x01\0\0\0";
    let package = Package::build(
        Package::manifest_for_transform(
            "test.migration",
            "1.0.0",
            module,
            vec![TransformHandler {
                handler: "convert".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 1024,
                max_output_bytes: 1024,
            }],
        ),
        module,
    )
    .unwrap();
    let invocation = Invocation::new_transform(
        "migration-task",
        Transform {
            handler: "convert".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: vec![1],
        },
    )
    .unwrap();
    task_evidence::encode(TaskEvidence {
        schema_version: 1,
        package_archive: package.archive().to_vec(),
        invocation: invocation.bytes().to_vec(),
        budget: Some(ExecutionBudget {
            fuel: 100000,
            memory_bytes: 65536,
            host_calls: 4,
        }),
        backend: task_evidence::BACKEND.into(),
        completion: invocation.output_completion(&[2]).unwrap(),
        fault: 0,
        exit_code: Some(0),
        observed_host_calls: 0,
        fuel_remaining: 90000,
        batch: None,
    })
    .unwrap()
}

struct Fixture {
    dir: tempfile::TempDir,
    host: HostRuntime,
    connection: Connection,
    migration: ContentMigration,
}
impl Fixture {
    fn new(budget: EventBudget) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("db"), budget).unwrap();
        let raw = b"original attachment";
        let blob = store
            .stage_blob(&mut &raw[..], raw.len() as u64, None, 0)
            .unwrap();
        let card = CardRecord::new_with_attachments(
            "card",
            "org.morrow.idea",
            1,
            "original title",
            vec![1],
            &[Attachment {
                id: "file".into(),
                display_name: "original.dat".into(),
                media_type: "application/octet-stream".into(),
                byte_length: raw.len() as u64,
                sha256: blob.sha256,
            }],
        )
        .unwrap();
        let pool = DescriptorPool::decode(content::DESCRIPTOR).unwrap();
        let mut known = DynamicMessage::decode(
            pool.get_message_by_name("morrow.content.v1.Card").unwrap(),
            card.encode().as_slice(),
        )
        .unwrap();
        let mut relation = DynamicMessage::new(
            pool.get_message_by_name("morrow.content.v1.Relation")
                .unwrap(),
        );
        relation.set_field_by_name("target_card_id", Value::String("other".into()));
        relation.set_field_by_name("kind", Value::String("reference".into()));
        known.set_field_by_name("relations", Value::List(vec![Value::Message(relation)]));
        known.set_field_by_name("created_at_unix_ms", Value::I64(1234));
        let future = DescriptorPool::decode(
            include_bytes!(concat!(env!("OUT_DIR"), "/future.descriptor.bin")).as_slice(),
        )
        .unwrap();
        let mut card = DynamicMessage::decode(
            future.get_message_by_name("test.future.Card").unwrap(),
            known.encode_to_vec().as_slice(),
        )
        .unwrap();
        card.set_field_by_name("status", Value::EnumNumber(9000));
        card.set_field_by_name("rich_document", Value::Bytes(vec![0, 255, 42].into()));
        let mut preview =
            DynamicMessage::new(future.get_message_by_name("test.future.Preview").unwrap());
        preview.set_field_by_name("future_metadata", Value::Bytes(vec![1, 255].into()));
        card.set_field_by_name("preview", Value::Message(preview));
        let card = CardRecord::decode(&card.encode_to_vec()).unwrap();
        store.create_local("seed", &card).unwrap();
        let mut host = HostRuntime::new(store).unwrap();
        let mut connection = host.connect().unwrap();
        host.grant(&mut connection, GrantKind::EditContent, "card", 100, 0)
            .unwrap();
        let migration = ContentMigration {
            operation_id: "migrate".into(),
            source_card: card.encode(),
            target_format_version: 2,
            body: vec![2, 0, 255],
            preview_text: "new preview".into(),
        };
        Self {
            dir,
            host,
            connection,
            migration,
        }
    }
    fn commit(&mut self, evidence: &[Evidence]) -> morrow_core::Result<transaction::Receipt> {
        self.host.migrate_content_guarded_with_evidence(
            &self.connection,
            &self.migration,
            evidence,
            || 1,
            |_| Ok(()),
        )
    }
    fn unchanged(&self) {
        assert_eq!(
            self.host
                .store_local()
                .card("card")
                .unwrap()
                .unwrap()
                .encode(),
            self.migration.source_card
        );
        assert_eq!(
            self.host.store_local().lookup("migrate").unwrap(),
            Lookup::Absent
        );
        assert_eq!(self.host.store_local().pending(0, 128).unwrap().len(), 1);
        assert!(matches!(
            self.host
                .store_local()
                .operation_evidence("card", "migrate"),
            Err(Error::NotFound)
        ));
        self.host.store_local().integrity_check().unwrap();
    }
}

#[test]
fn migration_preserves_full_record_and_evidence_atomically_across_reopen_and_retry() {
    let mut f = Fixture::new(EventBudget::default());
    let evidence = vec![evidence()];
    let receipt = f.commit(&evidence).unwrap();
    assert_eq!(receipt.revision, 2);
    let migrated = f.host.store_local().card("card").unwrap().unwrap();
    assert_eq!(migrated.summary().format_version, 2);
    assert_eq!(migrated.body(), f.migration.body);
    let pool = DescriptorPool::decode(content::DESCRIPTOR).unwrap();
    let descriptor = pool.get_message_by_name("morrow.content.v1.Card").unwrap();
    let source =
        DynamicMessage::decode(descriptor.clone(), f.migration.source_card.as_slice()).unwrap();
    let mut actual = DynamicMessage::decode(descriptor, migrated.encode().as_slice()).unwrap();
    for name in ["format_version", "revision", "body", "preview"] {
        actual.set_field_by_name(name, source.get_field_by_name(name).unwrap().into_owned());
    }
    assert_eq!(actual, source); // Includes unknown outer bytes, relations and creation time.
    let future = DescriptorPool::decode(
        include_bytes!(concat!(env!("OUT_DIR"), "/future.descriptor.bin")).as_slice(),
    )
    .unwrap();
    let restored = DynamicMessage::decode(
        future.get_message_by_name("test.future.Card").unwrap(),
        migrated.encode().as_slice(),
    )
    .unwrap();
    assert_eq!(
        restored
            .get_field_by_name("preview")
            .unwrap()
            .as_message()
            .unwrap()
            .get_field_by_name("future_metadata")
            .unwrap()
            .as_bytes()
            .unwrap()
            .as_ref(),
        &[1, 255]
    );
    let mut exported = Vec::new();
    f.host
        .store_local()
        .export_attachment_local("card", "file", &mut exported)
        .unwrap();
    assert_eq!(exported, b"original attachment");
    // A second migration advances the live card. Original retries return historical receipts.
    let later = ContentMigration {
        operation_id: "migrate-next".into(),
        source_card: migrated.encode(),
        target_format_version: 3,
        body: vec![3],
        preview_text: "later".into(),
    };
    f.host
        .migrate_content_guarded_with_evidence(&f.connection, &later, &[], || 1, |_| Ok(()))
        .unwrap();
    assert_eq!(f.commit(&evidence).unwrap(), receipt);
    assert_eq!(
        f.host
            .store_local()
            .card("card")
            .unwrap()
            .unwrap()
            .summary()
            .format_version,
        3
    );
    f.migration.body.push(9);
    assert_eq!(f.commit(&evidence), Err(Error::OperationConflict));
    f.migration.body.pop();
    assert_eq!(f.commit(&[]), Err(Error::OperationConflict));
    drop(f.host);
    let store = Store::open_existing(&f.dir.path().join("db"), EventBudget::default()).unwrap();
    let (commit, found) = store.operation_commit("card", "migrate").unwrap().unwrap();
    assert_eq!(receipt, found);
    assert_eq!(
        commit.task_evidence_sha256,
        vec![evidence[0].digest().to_vec()]
    );
    assert_eq!(
        store.operation_evidence("card", "migrate").unwrap()[0].raw(),
        evidence[0].raw()
    );
    assert_eq!(
        transaction::decode_command(&commit.command)
            .unwrap()
            .schema_version,
        2
    );
    store.integrity_check().unwrap();
    let mut reopened = HostRuntime::new(store).unwrap();
    let mut connection = reopened.connect().unwrap();
    reopened
        .grant(&mut connection, GrantKind::EditContent, "card", 100, 0)
        .unwrap();
    assert_eq!(
        reopened
            .migrate_content_guarded_with_evidence(
                &connection,
                &f.migration,
                &evidence,
                || 1,
                |_| Ok(())
            )
            .unwrap(),
        receipt
    );
    for deny_at in [1, 2] {
        let mut checks = 0;
        assert_eq!(
            reopened.migrate_content_guarded_with_evidence(
                &connection,
                &f.migration,
                &evidence,
                || 1,
                |_| {
                    checks += 1;
                    if checks == deny_at {
                        Err(Error::Invalid("migration denied"))
                    } else {
                        Ok(())
                    }
                }
            ),
            Err(Error::Invalid("migration denied"))
        );
        assert_eq!(checks, deny_at);
        assert_eq!(
            reopened.store_local().lookup("migrate").unwrap(),
            Lookup::Committed(receipt.clone())
        );
        assert_eq!(
            reopened
                .store_local()
                .card("card")
                .unwrap()
                .unwrap()
                .summary()
                .format_version,
            3
        );
    }
}

#[test]
fn changed_baselines_unknowns_and_adjacent_high_revisions_are_not_rebased() {
    let f = Fixture::new(EventBudget::default());
    let pool = DescriptorPool::decode(content::DESCRIPTOR).unwrap();
    let descriptor = pool.get_message_by_name("morrow.content.v1.Card").unwrap();
    for revision in [(1u64 << 53) + 1, (1u64 << 63) + 1, u64::MAX - 1] {
        let mut source =
            DynamicMessage::decode(descriptor.clone(), f.migration.source_card.as_slice()).unwrap();
        source.set_field_by_name("revision", Value::U64(revision));
        let mut proposal = f.migration.clone();
        proposal.source_card = source.encode_to_vec();
        let current = proposal.source().unwrap();
        assert_eq!(
            proposal.propose(&current).unwrap().summary().revision,
            revision + 1
        );
        source.set_field_by_name("revision", Value::U64(revision + 1));
        assert!(matches!(
            proposal.propose(&CardRecord::decode(&source.encode_to_vec()).unwrap()),
            Err(Error::RevisionConflict)
        ));
        proposal.source_card = source.encode_to_vec();
        if revision == u64::MAX - 1 {
            assert!(proposal.validate().is_err());
        }
    }
    for (field, value) in [
        ("id", Value::String("other".into())),
        ("type_id", Value::String("other".into())),
        ("format_version", Value::U32(2)),
        ("body", Value::Bytes(vec![9].into())),
    ] {
        let mut altered =
            DynamicMessage::decode(descriptor.clone(), f.migration.source_card.as_slice()).unwrap();
        altered.set_field_by_name(field, value);
        assert!(matches!(
            f.migration
                .propose(&CardRecord::decode(&altered.encode_to_vec()).unwrap()),
            Err(Error::RevisionConflict)
        ));
    }
    let mut unknown = f.migration.source_card.clone();
    unknown.extend_from_slice(&[0xA0, 0x06, 1]); // Unknown field 100 changes while revision stays fixed.
    assert!(matches!(
        f.migration.propose(&CardRecord::decode(&unknown).unwrap()),
        Err(Error::RevisionConflict)
    ));
    let mut f = f;
    let source = f.migration.source().unwrap();
    f.host
        .store_local_mut()
        .set_attachments_local("concurrent", "card", 1, &source.attachments())
        .unwrap();
    let before = f.host.store_local().card("card").unwrap().unwrap().encode();
    assert_eq!(f.commit(&[]), Err(Error::RevisionConflict));
    assert_eq!(
        f.host.store_local().card("card").unwrap().unwrap().encode(),
        before
    );
    assert_eq!(
        f.host.store_local().lookup("migrate").unwrap(),
        Lookup::Absent
    );
}

#[test]
fn scope_final_revocation_expiry_and_guard_rejection_rollback_even_evidence() {
    for mode in 0..4 {
        let mut f = Fixture::new(EventBudget::default());
        let signal = f.host.revocation(&f.connection).unwrap();
        let mut calls = 0;
        let result = f.host.migrate_content_guarded_with_evidence(
            &f.connection,
            &f.migration,
            &[evidence()],
            || {
                calls += 1;
                if mode == 0 || (mode == 1 && calls == 2) {
                    signal.revoke();
                }
                if mode == 2 && calls == 2 { 100 } else { 1 }
            },
            |now| {
                if mode == 3 && now == 1 {
                    Err(Error::Invalid("migration denied"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(calls, if mode == 0 || mode == 3 { 1 } else { 2 });
        f.unchanged();
    }
    let mut f = Fixture::new(EventBudget::default());
    f.host
        .revoke(&mut f.connection, GrantKind::EditContent, "card")
        .unwrap();
    assert!(f.commit(&[]).is_err());
    f.unchanged();
}

#[test]
fn capacity_and_post_content_evidence_failure_leave_no_partial_migration() {
    let mut f = Fixture::new(EventBudget {
        max_count: 1,
        ..EventBudget::default()
    });
    assert_eq!(f.commit(&[evidence()]), Err(Error::EventCapacity));
    f.unchanged();
    let mut f = Fixture::new(EventBudget::default());
    let too_many = vec![evidence(); 17];
    assert_eq!(f.commit(&too_many), Err(Error::Limit));
    f.unchanged();
    f.migration.body = vec![0; content::MAX_RECORD_BYTES]; // Body fits but the complete Card does not.
    assert_eq!(f.commit(&[]), Err(Error::Limit));
    f.unchanged();
    f.migration.body = vec![2];
    let sql = rusqlite::Connection::open(f.dir.path().join("db")).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_test_evidence BEFORE INSERT ON operation_evidence BEGIN SELECT RAISE(ABORT, 'test evidence unavailable'); END;").unwrap();
    assert_eq!(f.commit(&[evidence()]), Err(Error::Storage));
    sql.execute_batch("DROP TRIGGER reject_test_evidence;")
        .unwrap();
    f.unchanged();
    let count: i64 = sql
        .query_row("SELECT count(*) FROM task_evidence", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    f.commit(&[evidence()]).unwrap(); // Same operation remains usable after proven rollback.
}

#[test]
fn migration_version_and_exact_result_cannot_be_confused_with_legacy_commands() {
    let f = Fixture::new(EventBudget::default());
    let raw = transaction::migration_command(&f.migration).unwrap();
    let mut command = transaction::decode_command(&raw).unwrap();
    command.schema_version = 1;
    assert!(transaction::decode_command(&command.encode_to_vec()).is_err());
    let mut legacy = transaction::decode_command(
        &transaction::create_command("old", &f.migration.source().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(legacy.schema_version, 1);
    legacy.schema_version = 2;
    assert_eq!(
        transaction::decode_command(&legacy.encode_to_vec()),
        Err(Error::UnsupportedVersion)
    );
    for target in [0, 1] {
        let mut invalid = f.migration.clone();
        invalid.target_format_version = target;
        assert!(transaction::migration_command(&invalid).is_err());
    }
    let source = f.migration.source().unwrap();
    assert!(transaction::encode_commit(raw.clone(), &source).is_err());
    let migrated = f.migration.propose(&source).unwrap();
    transaction::encode_commit(raw.clone(), &migrated).unwrap();
    let pool = DescriptorPool::decode(content::DESCRIPTOR).unwrap();
    let descriptor = pool.get_message_by_name("morrow.content.v1.Card").unwrap();
    for (name, value) in [
        ("type_id", Value::String("forged".into())),
        ("format_version", Value::U32(3)),
        ("title", Value::String("wrong".into())),
        ("body", Value::Bytes(vec![9].into())),
    ] {
        let mut altered =
            DynamicMessage::decode(descriptor.clone(), migrated.encode().as_slice()).unwrap();
        altered.set_field_by_name(name, value);
        assert!(
            transaction::encode_commit(
                raw.clone(),
                &CardRecord::decode(&altered.encode_to_vec()).unwrap()
            )
            .is_err()
        );
    }
}
