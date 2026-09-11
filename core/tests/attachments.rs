#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    attachment::{BlobInfo, MAX_BLOB_BYTES, RetentionKind},
    content::{Attachment, CardRecord},
    store::{EventBudget, Store},
    transaction::Lookup,
};
use std::io::{Cursor, Read, Write};
fn item(blob: &BlobInfo, id: &str) -> Attachment {
    Attachment {
        id: id.into(),
        display_name: "原始🎵.bin".into(),
        media_type: "application/octet-stream".into(),
        byte_length: blob.byte_length,
        sha256: blob.sha256,
    }
}
fn card(blob: &BlobInfo) -> CardRecord {
    CardRecord::new_with_attachments(
        "card",
        "unknown.type",
        1,
        "title",
        vec![255],
        &[item(blob, "file")],
    )
    .unwrap()
}
fn stage(store: &mut Store, bytes: &[u8], now: i64) -> BlobInfo {
    store
        .stage_blob(&mut Cursor::new(bytes), bytes.len() as u64, None, now)
        .unwrap()
}
#[test]
fn raw_multichunk_and_empty_file_round_trip_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let bytes: Vec<_> = (0..1024 * 1024 + 17).map(|i| (i * 37) as u8).collect();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let file = stage(&mut store, &bytes, 1);
    let empty = stage(&mut store, &[], 2);
    store.create_local("create", &card(&file)).unwrap();
    drop(store);
    let store = Store::open_existing(&path, EventBudget::default()).unwrap();
    let mut output = vec![];
    store
        .export_attachment_local("card", "file", &mut output)
        .unwrap();
    assert_eq!(output, bytes);
    let mut output = vec![];
    store.export_blob_local(&empty.id, &mut output).unwrap();
    assert!(output.is_empty());
    let conn = rusqlite::Connection::open(&path).unwrap();
    let raw: Vec<u8> = conn
        .query_row("SELECT payload FROM blobs WHERE id=?1", [&file.id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(raw, bytes);
}
#[test]
fn stage_failures_and_limits_leave_no_partial_payload() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    assert!(
        store
            .stage_blob(&mut Cursor::new([1, 2]), 3, None, 0)
            .is_err()
    );
    assert!(
        store
            .stage_blob(&mut Cursor::new([1, 2, 3]), 2, None, 0)
            .is_err()
    );
    assert!(matches!(
        store.stage_blob(&mut Cursor::new([1, 2]), 2, Some([0; 32]), 0),
        Err(Error::Integrity)
    ));
    assert!(matches!(
        store.stage_blob(&mut Cursor::new([]), MAX_BLOB_BYTES + 1, None, 0),
        Err(Error::Limit)
    ));
    struct Fault {
        remaining: usize,
    }
    impl Read for Fault {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            assert!(out.len() <= 64 * 1024);
            if self.remaining == 0 {
                return Err(std::io::Error::other("test source error"));
            }
            let count = out.len().min(self.remaining);
            out[..count].fill(7);
            self.remaining -= count;
            Ok(count)
        }
    }
    assert!(matches!(
        store.stage_blob(&mut Fault { remaining: 70_000 }, 100_000, None, 0),
        Err(Error::Io)
    ));
    assert!(store.list_blobs_local("", 128).unwrap().is_empty());
    store.integrity_check().unwrap();
}
#[test]
fn same_content_is_deduplicated_and_shared_by_logical_attachments() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    let first = stage(&mut store, b"raw", 1);
    let second = stage(&mut store, b"raw", 2);
    assert_eq!(first.id, second.id);
    let card = CardRecord::new_with_attachments(
        "card",
        "type",
        1,
        "same",
        vec![],
        &[item(&first, "a"), item(&first, "b")],
    )
    .unwrap();
    store.create_local("create", &card).unwrap();
    assert_eq!(store.list_blobs_local("", 128).unwrap().len(), 1);
    store.integrity_check().unwrap();
    assert_eq!(store.retire_blob_local(&first.id, 3), Err(Error::Retained));
}
#[test]
fn removing_current_attachment_retains_original_for_history_and_retries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let file = stage(&mut store, b"original", 1);
    let created = store.create_local("create", &card(&file)).unwrap();
    let removed = store
        .set_attachments_local("remove", "card", 1, &[])
        .unwrap();
    assert_eq!(removed.revision, 2);
    assert!(!store.card("card").unwrap().unwrap().has_attachments());
    assert_eq!(
        store
            .set_attachments_local("remove", "card", 1, &[])
            .unwrap(),
        removed
    );
    assert_eq!(store.create_local("create", &card(&file)).unwrap(), created);
    assert_eq!(store.retire_blob_local(&file.id, 2), Err(Error::Retained));
    assert!(
        store
            .collect_retired_local(100_000, 60_000)
            .unwrap()
            .is_empty()
    );
    let mut output = vec![];
    store.export_blob_local(&file.id, &mut output).unwrap();
    assert_eq!(output, b"original");
    store.integrity_check().unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM event_blobs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn reference_insert_failure_rolls_back_card_event_and_retirement_clear() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let file = stage(&mut store, b"raw", 1);
    store.retire_blob_local(&file.id, 2).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_ref BEFORE INSERT ON event_blobs BEGIN SELECT RAISE(ABORT,'test reference failure'); END;").unwrap();
    assert!(store.create_local("create", &card(&file)).is_err());
    assert!(store.card("card").unwrap().is_none());
    assert_eq!(store.lookup("create").unwrap(), Lookup::Absent);
    assert!(store.pending(0, 10).unwrap().is_empty());
    assert_eq!(
        store.blob_info_local(&file.id).unwrap().retired_at_unix_ms,
        Some(2)
    );
    conn.execute_batch("DROP TRIGGER fail_ref;").unwrap();
    store.create_local("create", &card(&file)).unwrap();
    store.integrity_check().unwrap();
}
#[test]
fn stages_need_explicit_retirement_and_grace_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let file = stage(&mut store, b"raw", 0);
    assert!(
        store
            .collect_retired_local(1_000_000, 60_000)
            .unwrap()
            .is_empty()
    );
    store.retire_blob_local(&file.id, 1_000_000).unwrap();
    drop(store);
    let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert!(
        store
            .collect_retired_local(1_059_999, 60_000)
            .unwrap()
            .is_empty()
    );
    assert!(store.collect_retired_local(1_000_001, 60_000).is_err()); // durable clock regression
    assert_eq!(
        store.collect_retired_local(1_060_000, 60_000).unwrap(),
        vec![file.id.clone()]
    );
    assert!(matches!(
        store.blob_info_local(&file.id),
        Err(Error::NotFound)
    ));
    assert!(store.collect_retired_local(1_070_000, 0).is_err());
}
#[test]
fn snapshot_undo_and_evidence_retentions_block_collection() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    let file = stage(&mut store, b"raw", 0);
    store
        .retain_blob_local(&file.id, "undo", RetentionKind::Undo)
        .unwrap();
    store
        .retain_blob_local(&file.id, "snapshot", RetentionKind::Snapshot)
        .unwrap();
    assert_eq!(store.retire_blob_local(&file.id, 1), Err(Error::Retained));
    store
        .release_retention_local(&file.id, "undo", RetentionKind::Undo, 1)
        .unwrap();
    assert_eq!(store.retire_blob_local(&file.id, 1), Err(Error::Retained));
    store
        .release_retention_local(&file.id, "snapshot", RetentionKind::Snapshot, 2)
        .unwrap();
    assert_eq!(
        store.blob_info_local(&file.id).unwrap().retired_at_unix_ms,
        Some(2)
    );
    store
        .retain_blob_local(&file.id, "evidence", RetentionKind::Evidence)
        .unwrap();
    assert_eq!(
        store.release_retention_local(&file.id, "evidence", RetentionKind::Evidence, 3),
        Err(Error::Retained)
    );
    assert!(
        store
            .collect_retired_local(100_000, 60_000)
            .unwrap()
            .is_empty()
    );
    store.integrity_check().unwrap();
}
#[test]
fn export_read_snapshot_survives_concurrent_collection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut writer = Store::open(&path, EventBudget::default()).unwrap();
    let bytes = vec![29; 200_000];
    let file = stage(&mut writer, &bytes, 0);
    writer.retire_blob_local(&file.id, 1).unwrap();
    let reader = Store::open_existing(&path, EventBudget::default()).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (go_tx, go_rx) = std::sync::mpsc::channel();
    struct Sink {
        entered: Option<std::sync::mpsc::Sender<()>>,
        go: std::sync::mpsc::Receiver<()>,
        bytes: Vec<u8>,
    }
    impl Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if let Some(sender) = self.entered.take() {
                sender.send(()).unwrap();
                self.go
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap();
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let id = file.id.clone();
    let thread = std::thread::spawn(move || {
        let mut sink = Sink {
            entered: Some(entered_tx),
            go: go_rx,
            bytes: vec![],
        };
        reader.export_blob_local(&id, &mut sink).unwrap();
        sink.bytes
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    let result = writer.collect_retired_local(60_001, 60_000);
    go_tx.send(()).unwrap();
    assert_eq!(result.unwrap(), vec![file.id]);
    assert_eq!(thread.join().unwrap(), bytes);
}
#[test]
fn corrupted_raw_bytes_or_lost_historical_reference_fail_verification() {
    for raw in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let file = stage(&mut store, b"original", 0);
        store.create_local("create", &card(&file)).unwrap();
        let conn = rusqlite::Connection::open(&path).unwrap();
        if raw {
            conn.execute(
                "UPDATE blobs SET payload=?1 WHERE id=?2",
                rusqlite::params![b"modified".as_slice(), file.id],
            )
            .unwrap();
        } else {
            conn.execute("DELETE FROM event_blobs", []).unwrap();
        }
        assert_eq!(store.integrity_check(), Err(Error::Integrity));
        drop(store);
        assert!(matches!(
            Store::open_existing(&path, EventBudget::default()),
            Err(Error::Integrity)
        ));
    }
}
#[test]
fn editing_attachment_metadata_preserves_nested_unknown_fields() {
    use prost::Message;
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    let file = stage(&mut store, b"raw", 0);
    let pool = prost_reflect::DescriptorPool::decode(morrow_core::content::DESCRIPTOR).unwrap();
    let card_desc = pool.get_message_by_name("morrow.content.v1.Card").unwrap();
    let blob_desc = pool
        .get_message_by_name("morrow.content.v1.BlobRef")
        .unwrap();
    let mut message =
        prost_reflect::DynamicMessage::decode(card_desc.clone(), card(&file).encode().as_slice())
            .unwrap();
    let mut raw = message
        .get_field_by_name("attachments")
        .unwrap()
        .as_list()
        .unwrap()[0]
        .as_message()
        .unwrap()
        .encode_to_vec();
    raw.extend_from_slice(&[0xa0, 0x06, 0x7b]);
    let future = prost_reflect::DynamicMessage::decode(blob_desc, raw.as_slice()).unwrap();
    message.set_field_by_name(
        "attachments",
        prost_reflect::Value::List(vec![prost_reflect::Value::Message(future)]),
    );
    store
        .create_local(
            "create",
            &CardRecord::decode(&message.encode_to_vec()).unwrap(),
        )
        .unwrap();
    let mut changed = item(&file, "file");
    changed.display_name = "renamed".into();
    store
        .set_attachments_local("edit", "card", 1, &[changed])
        .unwrap();
    let saved = store.card("card").unwrap().unwrap();
    let message =
        prost_reflect::DynamicMessage::decode(card_desc, saved.encode().as_slice()).unwrap();
    assert_eq!(
        message
            .get_field_by_name("attachments")
            .unwrap()
            .as_list()
            .unwrap()[0]
            .as_message()
            .unwrap()
            .unknown_fields()
            .count(),
        1
    );
    assert_eq!(saved.attachments()[0].display_name, "renamed");
}
#[test]
fn new_storage_refuses_old_database_version_without_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE keep(value TEXT); PRAGMA application_id=1297044050; PRAGMA user_version=2;",
    )
    .unwrap();
    assert!(matches!(
        Store::open_existing(&path, EventBudget::default()),
        Err(Error::UnsupportedVersion)
    ));
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}
