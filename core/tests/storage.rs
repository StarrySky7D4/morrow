#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::CardRecord,
    lifecycle::HostPolicy,
    runtime::RenameRequest,
    store::{EventBudget, Store},
    transaction::{self, Lookup},
};
use rusqlite::Connection;
fn card() -> CardRecord {
    CardRecord::new("card-1", "unknown.type", 1, "原始", vec![0, 255, 7]).unwrap()
}
fn request(op: &str, revision: u64) -> RenameRequest {
    RenameRequest {
        operation_id: op.into(),
        card_id: "card-1".into(),
        expected_revision: revision,
        title: "修改 🧭".into(),
    }
}
fn rename(store: &mut Store, request: &RenameRequest) -> morrow_core::Result<transaction::Receipt> {
    let mut host = HostPolicy::new()?;
    let instance = host.activate()?;
    host.ready(instance)?;
    let grant = host.grant_rename(instance, "card-1", 100, 0)?;
    let permit = host.begin(instance, grant, request, 1)?;
    host.commit_rename(permit, store, || 2)
}
#[test]
fn atomic_create_rename_reopen_and_result_query() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let first = store.create_local("create-1", &card()).unwrap();
    let receipt = rename(&mut store, &request("edit-1", 1)).unwrap();
    assert_eq!(receipt.revision, 2);
    store.integrity_check().unwrap();
    let events = store.pending(0, 10).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(transaction::decode_commit(&events[1].1).unwrap().1, receipt);
    assert_eq!(store.pending(events[1].0, 10).unwrap().len(), 0);
    drop(store);
    let store = Store::open(&path, EventBudget::default()).unwrap();
    assert_eq!(store.lookup("create-1").unwrap(), Lookup::Committed(first));
    assert_eq!(store.lookup("edit-1").unwrap(), Lookup::Committed(receipt));
    assert_eq!(store.lookup("pending-elsewhere").unwrap(), Lookup::Absent);
    let loaded = store.card("card-1").unwrap().unwrap();
    assert_eq!(loaded.body(), card().body());
    assert_eq!(loaded.summary().title, "修改 🧭");
}
#[test]
fn duplicate_returns_original_result_even_after_later_edits_and_capacity_reached() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(
        &dir.path().join("test.db"),
        EventBudget {
            max_count: 3,
            ..EventBudget::default()
        },
    )
    .unwrap();
    let initial = card();
    let create = store.create_local("create", &initial).unwrap();
    let first = rename(&mut store, &request("first", 1)).unwrap();
    rename(&mut store, &request("second", 2)).unwrap();
    assert_eq!(rename(&mut store, &request("first", 1)).unwrap(), first);
    assert_eq!(store.create_local("create", &initial).unwrap(), create);
    assert_eq!(store.card("card-1").unwrap().unwrap().summary().revision, 3);
    assert_eq!(store.pending(0, 10).unwrap().len(), 3);
}
#[test]
fn reused_operation_id_with_changed_input_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("test.db"), EventBudget::default()).unwrap();
    store.create_local("create", &card()).unwrap();
    rename(&mut store, &request("edit", 1)).unwrap();
    let mut changed = request("edit", 1);
    changed.title = "other".into();
    assert_eq!(rename(&mut store, &changed), Err(Error::OperationConflict));
    assert_eq!(
        rename(&mut store, &request("create", 2)),
        Err(Error::OperationConflict)
    );
    assert_eq!(store.card("card-1").unwrap().unwrap().summary().revision, 2);
}
#[test]
fn revision_conflict_and_missing_card_leave_no_event_or_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("test.db"), EventBudget::default()).unwrap();
    assert_eq!(
        rename(&mut store, &request("missing", 1)),
        Err(Error::NotFound)
    );
    store.create_local("create", &card()).unwrap();
    assert_eq!(
        rename(&mut store, &request("conflict", 7)),
        Err(Error::RevisionConflict)
    );
    assert_eq!(store.lookup("conflict").unwrap(), Lookup::Absent);
    assert_eq!(store.pending(0, 10).unwrap().len(), 1);
}
#[test]
fn event_count_and_byte_budgets_fail_closed() {
    for budget in [
        EventBudget {
            max_count: 1,
            ..EventBudget::default()
        },
        EventBudget {
            max_bytes: 1,
            ..EventBudget::default()
        },
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let mut seed = Store::open(&path, EventBudget::default()).unwrap();
        seed.create_local("create", &card()).unwrap();
        drop(seed);
        let mut store = Store::open(&path, budget).unwrap();
        assert_eq!(
            rename(&mut store, &request("edit", 1)),
            Err(Error::EventCapacity)
        );
        assert_eq!(store.card("card-1").unwrap().unwrap().summary().revision, 1);
        assert_eq!(store.lookup("edit").unwrap(), Lookup::Absent);
        store.integrity_check().unwrap();
    }
}
#[test]
fn revoked_and_expired_authority_never_commit_or_read_dedup_results() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("test.db"), EventBudget::default()).unwrap();
    store.create_local("create", &card()).unwrap();
    for revoke in [false, true] {
        let mut host = HostPolicy::new().unwrap();
        let instance = host.activate().unwrap();
        host.ready(instance).unwrap();
        let grant = host.grant_rename(instance, "card-1", 10, 0).unwrap();
        let permit = host.begin(instance, grant, &request("edit", 1), 1).unwrap();
        if revoke {
            host.revoke(grant).unwrap();
        }
        // Last authorization runs after card/operation/event SQL writes, before COMMIT.
        let mut ticks = [2, 3, 10].into_iter();
        assert!(
            host.commit_rename(permit, &mut store, || ticks.next().unwrap())
                .is_err()
        );
        assert_eq!(store.card("card-1").unwrap().unwrap().summary().revision, 1);
        assert_eq!(store.lookup("edit").unwrap(), Lookup::Absent);
        assert_eq!(store.pending(0, 10).unwrap().len(), 1);
    }
    rename(&mut store, &request("edit", 1)).unwrap();
    let mut host = HostPolicy::new().unwrap();
    let instance = host.activate().unwrap();
    host.ready(instance).unwrap();
    let grant = host.grant_rename(instance, "card-1", 10, 0).unwrap();
    let permit = host.begin(instance, grant, &request("edit", 1), 1).unwrap();
    host.revoke(grant).unwrap();
    assert!(host.commit_rename(permit, &mut store, || 2).is_err());
}
#[test]
fn event_insert_failure_rolls_back_card_and_dedup_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store.create_local("create", &card()).unwrap();
    let injector = Connection::open(&path).unwrap();
    injector.execute_batch("CREATE TRIGGER fail_event BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'test event failure'); END;").unwrap();
    assert_eq!(rename(&mut store, &request("edit", 1)), Err(Error::Storage));
    assert_eq!(store.lookup("edit").unwrap(), Lookup::Absent);
    assert_eq!(store.card("card-1").unwrap().unwrap().summary().revision, 1);
    injector.execute_batch("DROP TRIGGER fail_event;").unwrap();
    rename(&mut store, &request("edit", 1)).unwrap();
    store.integrity_check().unwrap();
}
#[test]
fn simultaneous_same_revision_writers_have_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let mut first = Store::open(&path, EventBudget::default()).unwrap();
    first.create_local("create", &card()).unwrap();
    let mut second = Store::open(&path, EventBudget::default()).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let b = barrier.clone();
    let task = std::thread::spawn(move || {
        b.wait();
        rename(&mut first, &request("first", 1))
    });
    barrier.wait();
    let second_result = rename(&mut second, &request("second", 1));
    let first_result = task.join().unwrap();
    assert_ne!(first_result.is_ok(), second_result.is_ok());
    assert_eq!(
        second.card("card-1").unwrap().unwrap().summary().revision,
        2
    );
    assert_eq!(second.pending(0, 10).unwrap().len(), 2);
    second.integrity_check().unwrap();
}
#[test]
fn unrelated_or_future_database_is_not_reinitialized() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE keep(value TEXT); INSERT INTO keep VALUES('preserve');")
        .unwrap();
    assert!(Store::open(&path, EventBudget::default()).is_err());
    assert_eq!(
        conn.query_row("SELECT value FROM keep", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "preserve"
    );
    conn.execute_batch("PRAGMA application_id=1297044050; PRAGMA user_version=999;")
        .unwrap();
    assert!(matches!(
        Store::open(&path, EventBudget::default()),
        Err(Error::UnsupportedVersion)
    ));
}
#[test]
fn unknown_fields_and_full_u64_revision_survive_storage() {
    use prost::Message;
    let mut raw = card().encode();
    raw.extend_from_slice(&[0xa0, 0x06, 0x7b]); // future field 100
    let pool = prost_reflect::DescriptorPool::decode(morrow_core::content::DESCRIPTOR).unwrap();
    let mut msg = prost_reflect::DynamicMessage::decode(
        pool.get_message_by_name("morrow.content.v1.Card").unwrap(),
        raw.as_slice(),
    )
    .unwrap();
    msg.set_field_by_name("revision", prost_reflect::Value::U64(u64::MAX - 1));
    let source = CardRecord::decode(&msg.encode_to_vec()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("test.db"), EventBudget::default()).unwrap();
    store.create_local("create", &source).unwrap();
    let receipt = rename(&mut store, &request("edit", u64::MAX - 1)).unwrap();
    assert_eq!(receipt.revision, u64::MAX);
    let saved = store.card("card-1").unwrap().unwrap();
    let msg = prost_reflect::DynamicMessage::decode(
        pool.get_message_by_name("morrow.content.v1.Card").unwrap(),
        saved.encode().as_slice(),
    )
    .unwrap();
    assert_eq!(msg.unknown_fields().count(), 1);
    assert!(rename(&mut store, &request("overflow", u64::MAX)).is_err());
    assert_eq!(store.lookup("overflow").unwrap(), Lookup::Absent);
}

#[test]
fn integrity_check_detects_valid_container_with_wrong_current_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store.create_local("create", &card()).unwrap();
    let other = CardRecord::new("card-1", "unknown.type", 1, "tampered", vec![]).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "UPDATE cards SET payload=?1",
        [morrow_core::envelope::encode(&other).unwrap()],
    )
    .unwrap();
    assert_eq!(store.integrity_check(), Err(Error::Integrity));
    drop(store);
    assert!(matches!(
        Store::open_existing(&path, EventBudget::default()),
        Err(Error::Integrity)
    ));
}
#[test]
fn missing_outbox_event_blocks_reopen_and_existing_query_does_not_create_file() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("absent.db");
    assert!(Store::open_existing(&missing, EventBudget::default()).is_err());
    assert!(!missing.exists());
    let path = dir.path().join("bad.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    store.create_local("create", &card()).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute("DELETE FROM outbox", []).unwrap();
    assert_eq!(store.integrity_check(), Err(Error::Integrity));
    drop(store);
    assert!(matches!(
        Store::open_existing(&path, EventBudget::default()),
        Err(Error::Integrity)
    ));
}
#[test]
fn missing_attachment_payload_is_refused() {
    use prost::Message;
    let pool = prost_reflect::DescriptorPool::decode(morrow_core::content::DESCRIPTOR).unwrap();
    let mut record = prost_reflect::DynamicMessage::decode(
        pool.get_message_by_name("morrow.content.v1.Card").unwrap(),
        card().encode().as_slice(),
    )
    .unwrap();
    let mut blob = prost_reflect::DynamicMessage::new(
        pool.get_message_by_name("morrow.content.v1.BlobRef")
            .unwrap(),
    );
    blob.set_field_by_name("id", prost_reflect::Value::String("missing-blob".into()));
    blob.set_field_by_name("sha256", prost_reflect::Value::Bytes(vec![0; 32].into()));
    record.set_field_by_name(
        "attachments",
        prost_reflect::Value::List(vec![prost_reflect::Value::Message(blob)]),
    );
    let card = CardRecord::decode(&record.encode_to_vec()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("test.db"), EventBudget::default()).unwrap();
    assert!(store.create_local("create", &card).is_err());
    assert!(store.card("card-1").unwrap().is_none());
    assert!(store.pending(0, 10).unwrap().is_empty());
}
#[test]
fn drain_deadline_between_sql_writes_and_commit_rolls_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("test.db"), EventBudget::default()).unwrap();
    store.create_local("create", &card()).unwrap();
    let mut host = HostPolicy::new().unwrap();
    let instance = host.activate().unwrap();
    host.ready(instance).unwrap();
    let grant = host.grant_rename(instance, "card-1", 100, 0).unwrap();
    let permit = host.begin(instance, grant, &request("edit", 1), 1).unwrap();
    host.drain(instance, 5, 1).unwrap();
    let mut ticks = [2, 3, 5].into_iter();
    assert!(
        host.commit_rename(permit, &mut store, || ticks.next().unwrap())
            .is_err()
    );
    assert_eq!(store.card("card-1").unwrap().unwrap().summary().revision, 1);
    assert_eq!(store.lookup("edit").unwrap(), Lookup::Absent);
    host.stop(instance).unwrap();
}

#[test]
fn exclusive_profile_reopens_deduplicates_and_rolls_back_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exclusive.db");
    let budget = EventBudget {
        max_count: 2,
        ..EventBudget::default()
    };
    let mut store = Store::open_exclusive(&path, budget, true).unwrap();
    store.create_local("seed", &card()).unwrap();
    assert!(matches!(
        Store::open_exclusive(&path, budget, false),
        Err(Error::StorageBusy)
    ));
    let receipt = rename(&mut store, &request("edit", 1)).unwrap();
    assert_eq!(
        rename(&mut store, &request("overflow", 2)),
        Err(Error::EventCapacity)
    );
    drop(store);
    let mut store = Store::open_exclusive(&path, budget, false).unwrap();
    assert_eq!(rename(&mut store, &request("edit", 1)).unwrap(), receipt);
    assert_eq!(store.lookup("overflow").unwrap(), Lookup::Absent);
    assert_eq!(store.card("card-1").unwrap().unwrap().summary().revision, 2);
    assert_eq!(store.pending(0, 10).unwrap().len(), 2);
    store.integrity_check().unwrap();
    drop(store);
    let db = Connection::open(&path).unwrap();
    let mode: String = db
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "delete");
}
#[test]
fn exclusive_profile_does_not_change_an_unrelated_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unrelated.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE unrelated(value TEXT); INSERT INTO unrelated VALUES ('keep');")
        .unwrap();
    drop(db);
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(
        Store::open_exclusive(&path, EventBudget::default(), true),
        Err(Error::Invalid("unrelated database"))
    ));
    assert_eq!(std::fs::read(path).unwrap(), before);
}
