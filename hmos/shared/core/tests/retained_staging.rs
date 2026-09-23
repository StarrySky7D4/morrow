#![cfg(not(target_arch = "wasm32"))]
//! Disk-backed retained staging, including owner retries and process exits.
use morrow_core::{
    attachment::{BlobInfo, RetentionKind, MAX_BLOB_BYTES},
    content::{Attachment, CardRecord},
    store::{EventBudget, Store},
    Error,
};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn stage(store: &mut Store, owner: &str, bytes: &[u8], now: i64) -> BlobInfo {
    store
        .stage_blob_retained(
            &mut Cursor::new(bytes),
            bytes.len() as u64,
            digest(bytes),
            owner,
            now,
        )
        .unwrap()
}
fn output(store: &Store, id: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    store.export_blob_local(id, &mut bytes).unwrap();
    bytes
}
struct ThrowReader;
impl Read for ThrowReader {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("confirmed retained stage reread the original source")
    }
}

#[test]
fn owner_and_payload_are_one_durable_fact_and_confirmed_retry_does_not_read_source() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("store.db");
    let bytes = (0..100_001).map(|n| (n % 251) as u8).collect::<Vec<_>>();
    let mut store = Store::open(&db, EventBudget::default()).unwrap();
    let first = stage(&mut store, "snapshot-owner", &bytes, 10);
    assert_eq!(
        store.retained_blob_local("snapshot-owner").unwrap(),
        Some(first.clone())
    );
    assert_eq!(store.list_blobs_local("", 128).unwrap().len(), 1);
    assert_eq!(output(&store, &first.id), bytes);
    drop(store);

    let mut store =
        Store::open_existing(std::path::Path::new(&db), EventBudget::default()).unwrap();
    let again = store
        .stage_blob_retained(
            &mut ThrowReader,
            bytes.len() as u64,
            digest(&bytes),
            "snapshot-owner",
            0, // Old replay is read-only and must not advance the durable clock.
        )
        .unwrap();
    assert_eq!(again, first);
    assert_eq!(store.list_blobs_local("", 128).unwrap().len(), 1);
    assert_eq!(output(&store, &first.id), bytes);
    assert_eq!(store.retained_blob_local("not-an-owner").unwrap(), None);
    assert_eq!(
        store.stage_blob_retained(
            &mut ThrowReader,
            bytes.len() as u64,
            digest(b"different"),
            "snapshot-owner",
            11,
        ),
        Err(Error::OperationConflict)
    );
    assert_eq!(
        store.stage_blob_retained(
            &mut ThrowReader,
            bytes.len() as u64 + 1,
            digest(&bytes),
            "snapshot-owner",
            11,
        ),
        Err(Error::OperationConflict)
    );
    assert_eq!(
        store.retained_blob_local("snapshot-owner").unwrap(),
        Some(first)
    );
    store.integrity_check().unwrap();
}

#[test]
fn source_and_retention_insert_failures_roll_back_every_byte_and_owner() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("store.db");
    let mut store = Store::open(&db, EventBudget::default()).unwrap();
    let expected = digest(b"expected");
    assert!(store
        .stage_blob_retained(&mut Cursor::new(b"short"), 8, expected, "short-owner", 1)
        .is_err());
    assert!(store
        .stage_blob_retained(
            &mut Cursor::new(b"extra"),
            4,
            digest(b"extr"),
            "extra-owner",
            1
        )
        .is_err());
    assert_eq!(
        store.stage_blob_retained(&mut Cursor::new(b"wrong"), 5, expected, "wrong-owner", 1),
        Err(Error::Integrity)
    );
    assert_eq!(
        store.stage_blob_retained(
            &mut ThrowReader,
            MAX_BLOB_BYTES + 1,
            expected,
            "large-owner",
            1
        ),
        Err(Error::Limit)
    );
    for owner in ["short-owner", "extra-owner", "wrong-owner", "large-owner"] {
        assert_eq!(store.retained_blob_local(owner).unwrap(), None);
    }
    assert!(store.list_blobs_local("", 128).unwrap().is_empty());

    struct Fault {
        remaining: usize,
    }
    impl Read for Fault {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("source gone"));
            }
            let len = out.len().min(self.remaining);
            out[..len].fill(7);
            self.remaining -= len;
            Ok(len)
        }
    }
    let expected_large = digest(&vec![7; 100_000]);
    assert_eq!(
        store.stage_blob_retained(
            &mut Fault { remaining: 70_000 },
            100_000,
            expected_large,
            "io-owner",
            2
        ),
        Err(Error::Io)
    );
    assert_eq!(store.retained_blob_local("io-owner").unwrap(), None);
    assert!(store.list_blobs_local("", 128).unwrap().is_empty());

    let sql = rusqlite::Connection::open(&db).unwrap();
    sql.execute_batch("CREATE TRIGGER deny_snapshot BEFORE INSERT ON retentions WHEN NEW.owner='trigger-owner' BEGIN SELECT RAISE(ABORT,'retention insert denied'); END;").unwrap();
    assert!(store
        .stage_blob_retained(
            &mut Cursor::new(b"durable"),
            7,
            digest(b"durable"),
            "trigger-owner",
            3
        )
        .is_err());
    assert_eq!(store.retained_blob_local("trigger-owner").unwrap(), None);
    assert!(store.list_blobs_local("", 128).unwrap().is_empty());
    store.integrity_check().unwrap();
    sql.execute_batch("DROP TRIGGER deny_snapshot;").unwrap();
    assert_eq!(
        stage(&mut store, "trigger-owner", b"durable", 3).byte_length,
        7
    );
}

#[test]
fn distinct_owners_share_one_blob_and_release_needs_both_then_full_grace() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("store.db");
    let mut store = Store::open(&db, EventBudget::default()).unwrap();
    let a = stage(&mut store, "owner-a", b"same physical payload", 0);
    let b = stage(&mut store, "owner-b", b"same physical payload", 1);
    assert_eq!(a.id, b.id);
    assert_eq!(store.list_blobs_local("", 128).unwrap().len(), 1);
    store
        .release_retention_local(&a.id, "owner-a", RetentionKind::Snapshot, 2)
        .unwrap();
    assert_eq!(store.retained_blob_local("owner-a").unwrap(), None);
    assert_eq!(
        store.retained_blob_local("owner-b").unwrap().unwrap().id,
        b.id
    );
    assert_eq!(store.retire_blob_local(&a.id, 2), Err(Error::Retained));
    assert!(store
        .collect_retired_local(60_002, 60_000)
        .unwrap()
        .is_empty());
    store
        .release_retention_local(&b.id, "owner-b", RetentionKind::Snapshot, 60_003)
        .unwrap();
    assert_eq!(
        store.blob_info_local(&a.id).unwrap().retired_at_unix_ms,
        Some(60_003)
    );
    drop(store);
    let mut store =
        Store::open_existing(std::path::Path::new(&db), EventBudget::default()).unwrap();
    assert!(store
        .collect_retired_local(120_002, 60_000)
        .unwrap()
        .is_empty());
    assert_eq!(
        store.collect_retired_local(120_003, 60_000).unwrap(),
        vec![a.id.clone()]
    );
    assert!(matches!(store.blob_info_local(&a.id), Err(Error::NotFound)));
    assert_eq!(store.retained_blob_local("owner-b").unwrap(), None);
    store.integrity_check().unwrap();
}

#[test]
fn ordinary_card_and_event_history_keep_bytes_after_snapshot_owner_releases() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("store.db");
    let mut store = Store::open(&db, EventBudget::default()).unwrap();
    let file = stage(&mut store, "draft-owner", b"business and history pin", 0);
    let card = CardRecord::new_with_attachments(
        "business-card",
        "unknown.type",
        1,
        "title",
        vec![],
        &[Attachment {
            id: "selected".into(),
            display_name: "selected.bin".into(),
            media_type: "application/octet-stream".into(),
            byte_length: file.byte_length,
            sha256: file.sha256,
        }],
    )
    .unwrap();
    store.create_local("business-create", &card).unwrap();
    store
        .release_retention_local(&file.id, "draft-owner", RetentionKind::Snapshot, 1)
        .unwrap();
    assert_eq!(store.retire_blob_local(&file.id, 1), Err(Error::Retained));
    store
        .set_attachments_local("business-remove", "business-card", 1, &[])
        .unwrap();
    assert_eq!(store.retire_blob_local(&file.id, 2), Err(Error::Retained));
    assert!(store
        .collect_retired_local(60_003, 60_000)
        .unwrap()
        .is_empty());
    assert_eq!(output(&store, &file.id), b"business and history pin");
    let sql = rusqlite::Connection::open(&db).unwrap();
    let history: i64 = sql
        .query_row(
            "SELECT count(*) FROM event_blobs WHERE blob_id=?1",
            [&file.id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(history >= 1);
    store.integrity_check().unwrap();
}

#[test]
fn ambiguous_or_corrupted_owner_never_replays_or_creates_new_bytes() {
    for corruption in ["ambiguous", "retention", "metadata", "payload", "chunk"] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("store.db");
        let mut store = Store::open(&db, EventBudget::default()).unwrap();
        let file = stage(&mut store, "owner", b"original bytes", 0);
        let sql = rusqlite::Connection::open(&db).unwrap();
        match corruption {
            "ambiguous" => {
                let other = store
                    .stage_blob(&mut Cursor::new(b"different"), 9, None, 1)
                    .unwrap();
                store
                    .retain_blob_local(&other.id, "owner", RetentionKind::Snapshot)
                    .unwrap();
            }
            "retention" => {
                sql.execute(
                    "UPDATE retentions SET metadata=x'00' WHERE owner='owner'",
                    [],
                )
                .unwrap();
            }
            "metadata" => {
                sql.execute("UPDATE blobs SET metadata=x'00' WHERE id=?1", [&file.id])
                    .unwrap();
            }
            "payload" => {
                sql.execute(
                    "UPDATE blobs SET payload=?1 WHERE id=?2",
                    rusqlite::params![b"modified bytes".as_slice(), file.id],
                )
                .unwrap();
            }
            "chunk" => {
                sql.execute(
                    "UPDATE blob_chunks SET metadata=x'00' WHERE blob_id=?1",
                    [&file.id],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            store.retained_blob_local("owner").is_err(),
            "{corruption} lookup accepted"
        );
        assert!(
            store
                .stage_blob_retained(&mut ThrowReader, file.byte_length, file.sha256, "owner", 2)
                .is_err(),
            "{corruption} replay accepted"
        );
    }
}

#[test]
fn confirmed_replay_succeeds_at_physical_capacity_without_advancing_clock() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("store.db");
    let mut store = Store::open(&db, EventBudget::default()).unwrap();
    let original = stage(&mut store, "capacity-owner", b"original", 0);
    for index in 1_u16..2048 {
        let bytes = index.to_le_bytes();
        store
            .stage_blob(&mut Cursor::new(bytes), 2, None, i64::from(index))
            .unwrap();
    }
    assert_eq!(store.list_blobs_local("", 128).unwrap().len(), 128);
    let replay = store
        .stage_blob_retained(
            &mut ThrowReader,
            8,
            digest(b"original"),
            "capacity-owner",
            0,
        )
        .unwrap();
    assert_eq!(replay.id, original.id);
    assert!(store
        .stage_blob_retained(
            &mut Cursor::new(b"new"),
            3,
            digest(b"new"),
            "new-owner",
            2048
        )
        .is_err());
    assert_eq!(store.retained_blob_local("new-owner").unwrap(), None);
    assert!(store
        .stage_blob_retained(
            &mut Cursor::new(b"new"),
            3,
            digest(b"new"),
            "regressed-owner",
            0
        )
        .is_err());
    assert_eq!(store.retained_blob_local("regressed-owner").unwrap(), None);
    store.integrity_check().unwrap();
}

#[cfg(feature = "fault-injection")]
#[test]
fn retained_stage_child() {
    let Ok(db) = std::env::var("MORROW_RETAINED_CHILD_DB") else {
        return;
    };
    let source = std::env::var("MORROW_RETAINED_CHILD_SOURCE").unwrap();
    let mut store =
        Store::open_existing(std::path::Path::new(&db), EventBudget::default()).unwrap();
    let bytes = std::fs::read(&source).unwrap();
    let mut reader = std::fs::File::open(source).unwrap();
    store
        .stage_blob_retained(
            &mut reader,
            bytes.len() as u64,
            digest(&bytes),
            "crash-owner",
            1,
        )
        .unwrap();
}

#[cfg(feature = "fault-injection")]
#[test]
fn process_exit_before_and_after_commit_preserves_atomic_blob_and_owner() {
    use std::process::Command;
    let bytes = (0..100_001).map(|n| (n % 251) as u8).collect::<Vec<_>>();
    for point in [
        "stage-before-commit",
        "stage-retained-before-commit",
        "stage-after-commit",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("store.db");
        let source = dir.path().join("source.bin");
        std::fs::write(&source, &bytes).unwrap();
        Store::open(&db, EventBudget::default()).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .arg("--exact")
            .arg("retained_stage_child")
            .arg("--nocapture")
            .env("MORROW_RETAINED_CHILD_DB", &db)
            .env("MORROW_RETAINED_CHILD_SOURCE", &source)
            .env("MORROW_TEST_CRASH_AT", point);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            child.creation_flags(0x08000000);
        }
        let exited = child.output().unwrap();
        assert_eq!(exited.status.code(), Some(86), "{point}: {exited:?}");
        let mut store =
            Store::open_existing(std::path::Path::new(&db), EventBudget::default()).unwrap();
        store.integrity_check().unwrap();
        let durable = point == "stage-after-commit";
        let restored = store.retained_blob_local("crash-owner").unwrap();
        assert_eq!(
            restored.is_some(),
            durable,
            "{point} owner did not match transaction"
        );
        assert_eq!(
            store.list_blobs_local("", 128).unwrap().len(),
            usize::from(durable),
            "{point} blob did not match transaction"
        );
        if durable {
            std::fs::remove_file(&source).unwrap();
            let replay = store
                .stage_blob_retained(
                    &mut ThrowReader,
                    bytes.len() as u64,
                    digest(&bytes),
                    "crash-owner",
                    0,
                )
                .unwrap();
            assert_eq!(replay, restored.unwrap());
            assert_eq!(output(&store, &replay.id), bytes);
        } else {
            let replay = stage(&mut store, "crash-owner", &bytes, 1);
            assert_eq!(output(&store, &replay.id), bytes);
        }
        store.integrity_check().unwrap();
    }
}
