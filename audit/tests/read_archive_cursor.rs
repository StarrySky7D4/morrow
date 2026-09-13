#![cfg(target_os = "windows")]
use morrow_audit::{
    session::{OpenMode, Session, SessionError},
    snapshot, verify,
};
use morrow_core::{
    content::CardRecord,
    read_archive::{Budget, Finish, Plan},
    read_journal,
    store::ReadArchiveCursor,
};
use std::path::Path;
const SUBJECT: &str = "test.cursor-reader";
const OPERATION: &str = "cursor-archive";
fn open(db: &Path, mode: OpenMode) -> Session {
    Session::open(db, Default::default(), mode).unwrap()
}
struct Originals {
    root: [u8; 32],
    manifest: Vec<u8>,
    parts: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>,
}
fn publish(session: &mut Session) -> Originals {
    let store = session.runtime().store_local_mut();
    store
        .begin_read_archive(&Plan {
            operation_id: OPERATION.into(),
            subject: SUBJECT.into(),
            request_type: "test.cursor.request.v1".into(),
            request: b"fixed request".to_vec(),
            response_type: "test.cursor.result.v1".into(),
            budget: Budget {
                max_parts: 8,
                max_bytes: 1024 * 1024,
            },
        })
        .unwrap();
    let mut parts = Vec::new();
    for ordinal in 0..5 {
        let data: Vec<u8> = (0..(1000 + ordinal * 16000))
            .map(|i| ((i + ordinal) % 251) as u8)
            .collect();
        store
            .append_read_archive(SUBJECT, OPERATION, ordinal, "test.cursor.page.v1", &data)
            .unwrap();
        let part = store
            .read_archive_part(SUBJECT, OPERATION, ordinal)
            .unwrap()
            .unwrap();
        parts.push((part.raw().to_vec(), part.container().to_vec(), data));
    }
    let status = store
        .lookup_read_archive(SUBJECT, OPERATION)
        .unwrap()
        .unwrap();
    store
        .finish_read_archive_local_authorized(
            SUBJECT,
            OPERATION,
            &Finish {
                response: b"complete ordered result".to_vec(),
                part_count: status.count,
                logical_bytes: status.logical_bytes,
                chain_sha256: status.chain_sha256,
                metadata_type: "test.cursor.metadata.v1".into(),
                metadata: b"frozen host facts".to_vec(),
            },
            &[],
            || Ok(()),
        )
        .unwrap();
    session.flush(16).unwrap();
    let signed = session.store().sealed_segment(1).unwrap().unwrap();
    let trust = session.trust();
    let verified = verify(&signed, &trust).unwrap();
    assert_eq!(verified.segment().events.len(), 1);
    // The expected root is extracted only after verification of the actual Ed25519 event.
    let observation = read_journal::decode(&verified.segment().events[0].original_commit).unwrap();
    assert_eq!(observation.data().subject, SUBJECT);
    assert_eq!(observation.data().operation_id, OPERATION);
    let root = observation
        .data()
        .archive_sha256
        .as_slice()
        .try_into()
        .unwrap();
    let manifest = session
        .store()
        .read_archive_manifest(SUBJECT, OPERATION)
        .unwrap()
        .unwrap()
        .container()
        .to_vec();
    Originals {
        root,
        manifest,
        parts,
    }
}
fn consume(cursor: &mut ReadArchiveCursor, expected: &Originals) {
    assert_eq!(cursor.manifest().digest(), expected.root);
    assert_eq!(cursor.manifest().container(), expected.manifest);
    assert!(cursor.finish().is_err());
    // Insufficient page capacity neither skips a part nor confirms delivery.
    assert!(cursor.next_page(1, 1).is_err());
    let mut ordinal = 0;
    loop {
        let page = cursor.next_page(2, 256 * 1024).unwrap();
        assert!(page.parts.len() <= 2);
        for part in page.parts {
            let (raw, container, data) = &expected.parts[ordinal];
            assert_eq!(part.ordinal(), ordinal as u32);
            assert_eq!(part.raw(), raw);
            assert_eq!(part.container(), container);
            assert_eq!(part.data(), data);
            ordinal += 1;
        }
        if page.done {
            break;
        }
        assert!(cursor.finish().is_err());
    }
    assert_eq!(ordinal, expected.parts.len());
    cursor.finish().unwrap();
    let eof = cursor.next_page(2, 256 * 1024).unwrap();
    assert!(eof.done && eof.parts.is_empty());
}
fn checkpoint(db: &Path) -> i64 {
    let sql = rusqlite::Connection::open(db).unwrap();
    sql.busy_timeout(std::time::Duration::ZERO).unwrap();
    sql.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
        .unwrap()
}
#[test]
fn restored_signed_cursor_retains_originals_and_releases_only_its_read_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let db = source.join("workbench.db");
    let mut session = open(&db, OpenMode::Initialize);
    let expected = publish(&mut session);
    let signed = session.store().sealed_segment(1).unwrap().unwrap();
    let backup = dir.path().join("archive.morrowbackup");
    session.backup_snapshot(&backup).unwrap();
    drop(session);
    assert!(source.starts_with(dir.path()) && source.file_name().unwrap() == "source");
    std::fs::remove_dir_all(&source).unwrap();
    let restored = dir.path().join("restored");
    snapshot::restore(&backup, &restored).unwrap();
    let restored_db = restored.join("workbench.db");
    let mut restored = open(&restored_db, OpenMode::Existing);
    assert_eq!(restored.store().sealed_segment(1).unwrap().unwrap(), signed);
    let mut cursor = restored
        .store()
        .open_read_archive_cursor(SUBJECT, OPERATION, expected.root)
        .unwrap();
    restored
        .store()
        .validate_read_archive_cursor(&cursor)
        .unwrap();
    restored
        .runtime()
        .store_local_mut()
        .create_local(
            "after-cursor",
            &CardRecord::new("later", "text", 1, "Later", vec![1]).unwrap(),
        )
        .unwrap();
    assert_eq!(
        checkpoint(&restored_db),
        1,
        "live cursor pins the WAL read point"
    );
    consume(&mut cursor, &expected);
    cursor.close().unwrap();
    assert_eq!(
        checkpoint(&restored_db),
        0,
        "explicit close releases the SQLite reader"
    );
    assert!(
        matches!(
            Session::open(&restored_db, Default::default(), OpenMode::Existing),
            Err(SessionError::Busy)
        ),
        "closing a cursor cannot release its live Session's application lease"
    );
    // The cursor owns its connection, not a borrow of the source Session/Store.
    let mut owned = restored
        .store()
        .open_read_archive_cursor(SUBJECT, OPERATION, expected.root)
        .unwrap();
    restored.flush(16).unwrap();
    drop(restored);
    let reopened = open(&restored_db, OpenMode::Existing);
    consume(&mut owned, &expected);
    owned.close().unwrap();
    drop(reopened);
    drop(open(&restored_db, OpenMode::Existing));
}
#[test]
fn wrong_signed_pin_and_damaged_root_are_rejected_without_rebinding_existing_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut session = open(&db, OpenMode::Initialize);
    let expected = publish(&mut session);
    let mut wrong = expected.root;
    wrong[0] ^= 1;
    assert!(
        session
            .store()
            .open_read_archive_cursor(SUBJECT, OPERATION, wrong)
            .is_err()
    );
    let mut frozen = session
        .store()
        .open_read_archive_cursor(SUBJECT, OPERATION, expected.root)
        .unwrap();
    let sql = rusqlite::Connection::open(&db).unwrap();
    assert_eq!(
        sql.execute(
            "UPDATE operation_read_archives SET root=?1 WHERE operation_id=?2",
            rusqlite::params![wrong.as_slice(), OPERATION]
        )
        .unwrap(),
        1
    );
    assert!(
        session
            .store()
            .open_read_archive_cursor(SUBJECT, OPERATION, expected.root)
            .is_err()
    );
    assert!(
        session
            .store()
            .open_read_archive_cursor(SUBJECT, OPERATION, wrong)
            .is_err(),
        "changed index cannot grant a new valid root"
    );
    consume(&mut frozen, &expected);
    frozen.close().unwrap();
    sql.execute(
        "UPDATE operation_read_archives SET root=?1 WHERE operation_id=?2",
        rusqlite::params![expected.root.as_slice(), OPERATION],
    )
    .unwrap();
    drop(sql);
    let mut healthy = session
        .store()
        .open_read_archive_cursor(SUBJECT, OPERATION, expected.root)
        .unwrap();
    consume(&mut healthy, &expected);
    drop(healthy); // Drop is also a read-transaction cleanup path.
    assert_eq!(checkpoint(&db), 0);
    drop(session);
    drop(open(&db, OpenMode::Existing));
}
