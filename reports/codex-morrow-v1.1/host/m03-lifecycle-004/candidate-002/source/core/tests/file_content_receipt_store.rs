//! Immutable audit receipts for retained file bytes; no external file write is performed.
#![cfg(not(target_arch = "wasm32"))]

use morrow_core::{
    Error,
    audit::{self, SigningKey, TrustedLog},
    file_content::FileContent,
    file_content_receipt::{Receipt, Source},
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    io_evidence::{Kind, Material},
    io_intent::Record,
    store::{EventBudget, Store},
};
use sha2::{Digest, Sha256};
use std::path::Path;

const SUBJECT: &str = "plugin.file-content-receipt-test";
const OPERATION: &str = "file-content-receipt";

fn sample() -> (Record, Material, FileContent) {
    let bytes = b"immutable staged bytes for receipt tests";
    let request = RequestRecord::new(MutationRequest {
        operation_id: OPERATION.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: None,
        },
        disposition: Disposition::Replace,
        expected_identity: Some([4; 32]),
        content_length: bytes.len() as u64,
        content_sha256: Some(Sha256::digest(bytes).into()),
    })
    .unwrap();
    let prepared = Record::prepared(request.command().unwrap()).unwrap();
    let command = prepared.command();
    let material = Material::encode(
        Kind::Request,
        &command.operation_id,
        &command.subject,
        command.request_sha256,
        request.container(),
    )
    .unwrap();
    let content = FileContent::new(OPERATION, SUBJECT, command.request_sha256, bytes).unwrap();
    (prepared, material, content)
}

fn prepare_and_stage(
    store: &mut Store,
    record: &Record,
    material: &Material,
    content: &FileContent,
) {
    store
        .prepare_file_mutation_local_authorized(record, material, || Ok(()))
        .unwrap();
    store
        .stage_file_mutation_content_local_authorized(content, || Ok(()))
        .unwrap();
}

fn receipt(store: &Store, content: &FileContent, source: Source) -> Receipt {
    let mut checks = 0;
    let value = store
        .file_mutation_content_receipt_local_authorized(SUBJECT, OPERATION, || {
            checks += 1;
            Ok(())
        })
        .unwrap()
        .unwrap();
    assert_eq!(checks, 2);
    assert_eq!(value.source(), source);
    assert_eq!(value.operation_id(), OPERATION);
    assert_eq!(value.subject(), SUBJECT);
    assert_eq!(value.request_sha256(), content.request_sha256());
    assert_eq!(value.content_sha256(), content.content_sha256());
    assert_eq!(value.content_length(), content.content().len() as u64);
    assert_eq!(
        value.content_container_sha256(),
        <[u8; 32]>::from(Sha256::digest(content.container()))
    );
    assert_eq!(
        Receipt::decode(value.container()).unwrap().raw(),
        value.raw()
    );
    value
}

fn event_count(path: &Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("SELECT count(*) FROM operation_events", [], |r| r.get(0))
        .unwrap()
}

fn reconstruct_v22_with_staged_bytes(path: &Path, live: &Receipt) {
    // v22 retained the content and Prepared event, but had no receipt event.
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.execute(
        "DELETE FROM file_content_receipts WHERE operation_id=?1",
        [OPERATION],
    )
    .unwrap();
    for table in ["operation_events", "outbox", "operations"] {
        sql.execute(
            &format!("DELETE FROM {table} WHERE id=?1"),
            [live.event_id()],
        )
        .unwrap();
    }
    sql.execute_batch(
        "DROP TABLE file_content_receipts; UPDATE sqlite_sequence SET seq=1 WHERE name='outbox'; PRAGMA user_version=22;",
    )
    .unwrap();
}

#[test]
fn live_stage_adds_one_bound_event_and_exact_retry_keeps_original() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("content.db");
    let (record, material, content) = sample();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare_and_stage(&mut store, &record, &material, &content);
    let value = receipt(&store, &content, Source::LiveStaging);
    let pending = store.pending(0, 10).unwrap();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[1].1, value.container());
    assert_eq!(event_count(&path), 2);
    let sql = rusqlite::Connection::open(&path).unwrap();
    let stored: (String, i64, Vec<u8>, i64) = sql
        .query_row(
            "SELECT r.event_id,o.object_kind,o.payload,e.sequence FROM file_content_receipts r JOIN operations o ON o.id=r.event_id JOIN operation_events e ON e.id=r.event_id WHERE r.operation_id=?1",
            [OPERATION],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(stored.0, value.event_id());
    assert_eq!(stored.1, 6);
    assert_eq!(stored.2, value.container());
    assert_eq!(stored.3, pending[1].0);
    drop(sql);
    store
        .stage_file_mutation_content_local_authorized(&content, || Ok(()))
        .unwrap();
    assert_eq!(store.pending(0, 10).unwrap(), pending);
    assert_eq!(event_count(&path), 2);
    assert_eq!(
        receipt(&store, &content, Source::LiveStaging).container(),
        value.container()
    );
    store.integrity_check().unwrap();
}

#[test]
fn missing_receipt_or_original_is_rejected_on_live_read_and_reopen() {
    for missing in ["receipt", "content"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");
        let (record, material, content) = sample();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare_and_stage(&mut store, &record, &material, &content);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute(
            if missing == "receipt" {
                "DELETE FROM file_content_receipts WHERE operation_id=?1"
            } else {
                "DELETE FROM file_mutation_content WHERE operation_id=?1"
            },
            [OPERATION],
        )
        .unwrap();
        drop(sql);
        assert!(store.integrity_check().is_err(), "{missing}");
        assert!(
            store
                .file_mutation_content_receipt_local_authorized(SUBJECT, OPERATION, || Ok(()))
                .is_err(),
            "{missing}"
        );
        drop(store);
        assert!(
            Store::open_existing(&path, EventBudget::default()).is_err(),
            "{missing}"
        );
    }
}

#[test]
fn missing_pending_outbox_entry_blocks_both_live_content_reads_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing-outbox.db");
    let (record, material, content) = sample();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare_and_stage(&mut store, &record, &material, &content);
    let event = receipt(&store, &content, Source::LiveStaging).event_id();
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        sql.execute("DELETE FROM outbox WHERE id=?1", [&event])
            .unwrap(),
        1
    );
    drop(sql);
    assert!(
        store
            .file_mutation_content_receipt_local_authorized(SUBJECT, OPERATION, || Ok(()))
            .is_err()
    );
    assert!(
        store
            .file_mutation_content_local_authorized(SUBJECT, OPERATION, || Ok(()))
            .is_err()
    );
    assert!(store.integrity_check().is_err());
    drop(store);
    assert!(Store::open_existing(&path, EventBudget::default()).is_err());
}

#[test]
fn live_receipt_after_cancel_is_rejected_but_legacy_import_after_cancel_is_valid() {
    let dir = tempfile::tempdir().unwrap();
    let live_path = dir.path().join("live-order.db");
    let (record, material, content) = sample();
    let mut live = Store::open(&live_path, EventBudget::default()).unwrap();
    prepare_and_stage(&mut live, &record, &material, &content);
    let original = receipt(&live, &content, Source::LiveStaging);
    let cancelled = record.propose_cancel_before_dispatch().unwrap();
    live.append_io_intent_local_authorized(&cancelled, || Ok(()))
        .unwrap();
    assert_eq!(live.pending(0, 10).unwrap().len(), 3);
    let sql = rusqlite::Connection::open(&live_path).unwrap();
    // Move the receipt after Cancelled in both indexes, keeping queue membership
    // otherwise consistent so the live-order rule is what rejects the read.
    for table in ["operation_events", "outbox"] {
        sql.execute(
            &format!("UPDATE {table} SET sequence=4 WHERE id=?1"),
            [original.event_id()],
        )
        .unwrap();
        sql.execute(
            &format!("UPDATE {table} SET sequence=2 WHERE id=?1"),
            [cancelled.event_id()],
        )
        .unwrap();
        sql.execute(
            &format!("UPDATE {table} SET sequence=3 WHERE id=?1"),
            [original.event_id()],
        )
        .unwrap();
    }
    drop(sql);
    assert!(
        live.file_mutation_content_receipt_local_authorized(SUBJECT, OPERATION, || Ok(()))
            .is_err()
    );
    drop(live);

    let legacy_path = dir.path().join("legacy-order.db");
    let mut legacy = Store::open(&legacy_path, EventBudget::default()).unwrap();
    prepare_and_stage(&mut legacy, &record, &material, &content);
    let original = receipt(&legacy, &content, Source::LiveStaging);
    legacy
        .append_io_intent_local_authorized(&cancelled, || Ok(()))
        .unwrap();
    drop(legacy);
    reconstruct_v22_with_staged_bytes(&legacy_path, &original);
    let sql = rusqlite::Connection::open(&legacy_path).unwrap();
    for table in ["operation_events", "outbox"] {
        sql.execute(
            &format!("UPDATE {table} SET sequence=2 WHERE id=?1"),
            [cancelled.event_id()],
        )
        .unwrap();
    }
    sql.execute_batch("UPDATE sqlite_sequence SET seq=2 WHERE name='outbox';")
        .unwrap();
    drop(sql);
    let migrated = Store::open_existing(&legacy_path, EventBudget::default()).unwrap();
    assert_eq!(
        receipt(&migrated, &content, Source::LegacyImport).event_id(),
        original.event_id()
    );
    assert_eq!(migrated.pending(0, 10).unwrap().len(), 3);
    migrated.integrity_check().unwrap();
}

#[test]
fn receipt_survives_pending_seal_snapshot_and_pinned_readpoint() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audited.db");
    let before = dir.path().join("pending.db");
    let after = dir.path().join("sealed.db");
    let key = SigningKey::from_bytes(&[84; 32]);
    let trust = TrustedLog {
        id: "file-content-receipt-log".into(),
        key: key.verifying_key(),
    };
    let (record, material, content) = sample();
    let mut store =
        Store::open_audited(&path, EventBudget::default(), true, trust.clone()).unwrap();
    prepare_and_stage(&mut store, &record, &material, &content);
    let pending = store.pending(0, 10).unwrap();
    assert_eq!(pending.len(), 2);
    let receipt_container = receipt(&store, &content, Source::LiveStaging)
        .container()
        .to_vec();
    let snapshot = store.open_card_snapshot().unwrap();
    assert_eq!(snapshot.readpoint().database_version, 23);
    assert_eq!(snapshot.readpoint().operation_sequence, pending[1].0 as u64);
    store.snapshot_to(&before, 16 * 1024 * 1024).unwrap();
    for source in [&path, &before] {
        let reader =
            Store::open_audited(source, EventBudget::default(), false, trust.clone()).unwrap();
        assert_eq!(reader.pending(0, 10).unwrap(), pending);
        assert_eq!(
            receipt(&reader, &content, Source::LiveStaging).container(),
            receipt_container
        );
    }
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &pending).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    assert!(store.seal_pending(&signed).unwrap());
    assert!(store.pending(0, 10).unwrap().is_empty());
    store.snapshot_to(&after, 16 * 1024 * 1024).unwrap();
    drop(snapshot);
    drop(store);
    for source in [&path, &after] {
        let reader = Store::open_read_only_audited(source, trust.clone()).unwrap();
        assert_eq!(reader.sealed_segment(1).unwrap().unwrap(), signed);
        assert_eq!(
            receipt(&reader, &content, Source::LiveStaging).container(),
            receipt_container
        );
        let snapshot = reader.open_card_snapshot().unwrap();
        assert_eq!(snapshot.readpoint().operation_sequence, pending[1].0 as u64);
        reader.integrity_check().unwrap();
    }
}

#[test]
fn existing_v22_content_is_marked_legacy_import_on_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let (record, material, content) = sample();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare_and_stage(&mut store, &record, &material, &content);
    let live = receipt(&store, &content, Source::LiveStaging);
    drop(store);
    reconstruct_v22_with_staged_bytes(&path, &live);
    let reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
    let imported = receipt(&reopened, &content, Source::LegacyImport);
    assert_eq!(imported.event_id(), live.event_id());
    assert_ne!(imported.container(), live.container());
    assert_eq!(reopened.pending(0, 10).unwrap().len(), 2);
    assert_eq!(event_count(&path), 2);
    assert_eq!(
        rusqlite::Connection::open(&path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        23
    );
    reopened.integrity_check().unwrap();
}

#[test]
fn v22_import_without_event_room_rolls_back_and_succeeds_with_room() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy-capacity.db");
    let (record, material, content) = sample();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare_and_stage(&mut store, &record, &material, &content);
    let live = receipt(&store, &content, Source::LiveStaging);
    drop(store);
    reconstruct_v22_with_staged_bytes(&path, &live);
    let sql = rusqlite::Connection::open(&path).unwrap();
    let before_content: Vec<u8> = sql
        .query_row(
            "SELECT container FROM file_mutation_content WHERE operation_id=?1",
            [OPERATION],
            |r| r.get(0),
        )
        .unwrap();
    let before_pending: Vec<(i64, Vec<u8>)> = {
        let mut statement = sql
            .prepare("SELECT sequence,payload FROM outbox ORDER BY sequence")
            .unwrap();
        statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(before_pending.len(), 1);
    drop(sql);
    let constrained = EventBudget {
        max_count: 1,
        max_bytes: EventBudget::default().max_bytes,
    };
    assert!(matches!(
        Store::open_existing(&path, constrained),
        Err(Error::EventCapacity)
    ));
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        22
    );
    let receipt_table: i64 = sql
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='file_content_receipts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(receipt_table, 0);
    let after_content: Vec<u8> = sql
        .query_row(
            "SELECT container FROM file_mutation_content WHERE operation_id=?1",
            [OPERATION],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after_content, before_content);
    let after_pending: Vec<(i64, Vec<u8>)> = {
        let mut statement = sql
            .prepare("SELECT sequence,payload FROM outbox ORDER BY sequence")
            .unwrap();
        statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(after_pending, before_pending);
    drop(sql);
    let reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert_eq!(
        receipt(&reopened, &content, Source::LegacyImport).event_id(),
        live.event_id()
    );
    assert_eq!(reopened.pending(0, 10).unwrap().len(), 2);
    reopened.integrity_check().unwrap();
}
