#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::CardRecord,
    read_archive::{Budget, Finish, Part, Plan},
    store::{ReadArchiveCursor, Store},
};
use std::path::{Path, PathBuf};
fn data(size: usize) -> Vec<u8> {
    let mut x = 0x134f782au32;
    (0..size)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        })
        .collect()
}
fn plan(op: &str) -> Plan {
    Plan {
        operation_id: op.into(),
        subject: "query".into(),
        request_type: "test.request".into(),
        request: b"opaque request".to_vec(),
        response_type: "test.response".into(),
        budget: Budget {
            max_parts: 1024,
            max_bytes: 128 * 1024 * 1024,
        },
    }
}
fn setup() -> (tempfile::TempDir, PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cursor.db");
    let store = Store::open(&path, Default::default()).unwrap();
    (dir, path, store)
}
fn publish(store: &mut Store, op: &str, count: u32, size: usize) -> [u8; 32] {
    let mut status = store.begin_read_archive(&plan(op)).unwrap();
    for ordinal in 0..count {
        let mut bytes = data(size);
        bytes.extend_from_slice(&ordinal.to_le_bytes());
        status = store
            .append_read_archive("query", op, ordinal, "test.bytes", &bytes)
            .unwrap();
    }
    store
        .finish_read_archive_local_authorized(
            "query",
            op,
            &Finish {
                response: b"result".to_vec(),
                part_count: status.count,
                logical_bytes: status.logical_bytes,
                chain_sha256: status.chain_sha256,
                metadata_type: "test.metadata".into(),
                metadata: vec![],
            },
            &[],
            || Ok(()),
        )
        .unwrap();
    store
        .lookup_read_archive("query", op)
        .unwrap()
        .unwrap()
        .root
        .unwrap()
}
fn all(cursor: &mut ReadArchiveCursor, max: u32) -> Vec<Part> {
    let mut parts = Vec::new();
    loop {
        let page = cursor.next_page(max, 32 * 1024 * 1024).unwrap();
        parts.extend(page.parts);
        if page.done {
            return parts;
        }
    }
}
#[test]
fn cursor_requires_expected_root_subject_and_published_archive() {
    let (_dir, _path, mut store) = setup();
    let root = publish(&mut store, "complete", 3, 100);
    store.begin_read_archive(&plan("pending")).unwrap();
    store
        .append_read_archive("query", "pending", 0, "test.bytes", b"unpublished")
        .unwrap();
    for (subject, op, expected) in [
        ("query", "complete", [0; 32]),
        ("other", "complete", root),
        ("query", "missing", root),
        ("query", "pending", root),
    ] {
        assert!(
            store
                .open_read_archive_cursor(subject, op, expected)
                .is_err()
        );
    }
    let mut cursor = store
        .open_read_archive_cursor("query", "complete", root)
        .unwrap();
    assert_eq!(cursor.manifest().digest(), root);
    assert_eq!(all(&mut cursor, 1).len(), 3);
    cursor.finish().unwrap();
}
#[test]
fn owner_binding_rejects_another_store_even_when_it_opens_the_same_database() {
    let (_dir, path, mut store) = setup();
    let root = publish(&mut store, "complete", 1, 20);
    let cursor = store
        .open_read_archive_cursor("query", "complete", root)
        .unwrap();
    store.validate_read_archive_cursor(&cursor).unwrap();
    let other = Store::open_existing(&path, Default::default()).unwrap();
    assert!(other.validate_read_archive_cursor(&cursor).is_err());
    cursor.close().unwrap();
    store.integrity_check().unwrap();
}
#[test]
fn corrupt_last_part_is_rejected_before_first_page_can_be_obtained() {
    let (_dir, path, mut store) = setup();
    let root = publish(&mut store, "complete", 4, 100);
    rusqlite::Connection::open(&path).unwrap().execute_batch("UPDATE read_archive_parts SET payload=x'00' WHERE operation_id='complete' AND ordinal=3").unwrap();
    assert!(
        store
            .open_read_archive_cursor("query", "complete", root)
            .is_err()
    );
}
#[test]
fn validly_encoded_forged_tail_cannot_pass_the_initial_whole_chain_check() {
    use morrow_core::read_archive::proto;
    use prost::Message;
    use sha2::{Digest, Sha256};
    let (_dir, path, mut store) = setup();
    let root = publish(&mut store, "complete", 3, 100);
    let original = store
        .read_archive_part("query", "complete", 2)
        .unwrap()
        .unwrap();
    let mut value = proto::Part::decode(original.raw()).unwrap();
    value.data.push(9);
    value.data_sha256 = Sha256::digest(&value.data).to_vec();
    let raw = value.encode_to_vec();
    let compressed = lz4_flex::block::compress(&raw);
    let mut encoded = b"MRWAPRT1".to_vec();
    encoded.extend_from_slice(&1u16.to_le_bytes());
    encoded.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    encoded.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    encoded.extend_from_slice(&Sha256::digest(&raw));
    encoded.extend(compressed);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE read_archive_parts SET payload=?1 WHERE operation_id='complete' AND ordinal=2",
            [encoded],
        )
        .unwrap();
    assert!(
        store
            .open_read_archive_cursor("query", "complete", root)
            .is_err()
    );
}
#[test]
fn fixed_cursor_does_not_mix_concurrent_archive_publication_or_content_changes() {
    let (_dir, path, mut store) = setup();
    let root = publish(&mut store, "complete", 5, 200);
    let expected: Vec<_> = (0..5)
        .map(|n| {
            store
                .read_archive_part("query", "complete", n)
                .unwrap()
                .unwrap()
                .container()
                .to_vec()
        })
        .collect();
    let mut cursor = store
        .open_read_archive_cursor("query", "complete", root)
        .unwrap();
    let first = cursor.next_page(1, 4096).unwrap().parts.remove(0);
    let mut writer = Store::open_existing(&path, Default::default()).unwrap();
    publish(&mut writer, "other", 3, 1000);
    writer
        .create_local(
            "create",
            &CardRecord::new("card", "type", 1, "new content", vec![]).unwrap(),
        )
        .unwrap();
    let mut actual = vec![first.container().to_vec()];
    actual.extend(all(&mut cursor, 2).iter().map(|p| p.container().to_vec()));
    assert_eq!(actual, expected);
    assert_eq!(cursor.manifest().digest(), root);
    cursor.finish().unwrap();
    assert!(writer.lookup_read("query", "other").unwrap().is_some());
    assert!(writer.card("card").unwrap().is_some());
}
#[test]
fn verified_transaction_keeps_original_parts_after_later_storage_corruption() {
    let (_dir, path, mut store) = setup();
    let root = publish(&mut store, "complete", 3, 100);
    let expected: Vec<_> = (0..3)
        .map(|n| {
            store
                .read_archive_part("query", "complete", n)
                .unwrap()
                .unwrap()
                .container()
                .to_vec()
        })
        .collect();
    let mut cursor = store
        .open_read_archive_cursor("query", "complete", root)
        .unwrap();
    rusqlite::Connection::open(&path).unwrap().execute_batch("UPDATE read_archive_parts SET payload=x'00' WHERE operation_id='complete' AND ordinal=2").unwrap();
    let actual: Vec<_> = all(&mut cursor, 1)
        .iter()
        .map(|p| p.container().to_vec())
        .collect();
    assert_eq!(actual, expected);
    cursor.finish().unwrap();
    assert!(
        store
            .open_read_archive_cursor("query", "complete", root)
            .is_err()
    );
}
#[test]
fn byte_budget_counts_original_plus_container_and_failed_budget_does_not_advance() {
    let (_dir, _path, mut store) = setup();
    let root = publish(&mut store, "complete", 3, 500);
    let first = store
        .read_archive_part("query", "complete", 0)
        .unwrap()
        .unwrap();
    let cost = first.raw().len() + first.container().len();
    let mut cursor = store
        .open_read_archive_cursor("query", "complete", root)
        .unwrap();
    for (count, bytes) in [
        (0, cost),
        (129, cost),
        (1, 0),
        (1, cost - 1),
        (1, morrow_core::store::MAX_ARCHIVE_PAGE_BYTES + 1),
        (128, usize::MAX),
    ] {
        assert!(cursor.next_page(count, bytes).is_err());
        assert!(cursor.finish().is_err());
        store.validate_read_archive_cursor(&cursor).unwrap();
    }
    let page = cursor.next_page(128, cost).unwrap();
    assert_eq!(page.parts.len(), 1);
    assert_eq!(page.parts[0].ordinal(), 0);
    assert_eq!(page.parts[0].container(), first.container());
    assert!(!page.done);
    let rest = all(&mut cursor, 1);
    assert_eq!(rest.iter().map(Part::ordinal).collect::<Vec<_>>(), [1, 2]);
    cursor.finish().unwrap();
}
#[test]
fn empty_archive_requires_real_eof_and_remains_stable_until_explicit_close() {
    let (_dir, _path, mut store) = setup();
    let root = publish(&mut store, "empty", 0, 0);
    let mut cursor = store
        .open_read_archive_cursor("query", "empty", root)
        .unwrap();
    assert!(cursor.finish().is_err());
    for _ in 0..3 {
        let page = cursor.next_page(128, 1).unwrap();
        assert!(page.parts.is_empty() && page.done);
        cursor.finish().unwrap();
    }
    cursor.close().unwrap();
    assert!(store.lookup_read("query", "empty").unwrap().is_some());
}
#[test]
fn maximum_part_and_readonly_source_drop_preserve_exact_original_bytes() {
    use morrow_core::audit::{SigningKey, TrustedLog};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audited.db");
    let key = SigningKey::from_bytes(&[45; 32]);
    let trust = TrustedLog {
        id: "archive-cursor".into(),
        key: key.verifying_key(),
    };
    let mut store = Store::open_audited(&path, Default::default(), true, trust.clone()).unwrap();
    let root = publish(&mut store, "complete", 1, 8 * 1024 * 1024 - 4);
    let part = store
        .read_archive_part("query", "complete", 0)
        .unwrap()
        .unwrap();
    let cost = part.raw().len() + part.container().len();
    assert_eq!(part.data().len(), 8 * 1024 * 1024);
    drop(store);
    let readonly = Store::open_read_only_audited(&path, trust).unwrap();
    let mut cursor = readonly
        .open_read_archive_cursor("query", "complete", root)
        .unwrap();
    drop(readonly);
    assert!(cursor.next_page(1, cost - 1).is_err());
    let page = cursor.next_page(1, cost).unwrap();
    assert_eq!(page.parts.len(), 1);
    assert_eq!(page.parts[0].raw(), part.raw());
    assert_eq!(page.parts[0].container(), part.container());
    if !page.done {
        assert!(cursor.next_page(1, 1).unwrap().done);
    }
    cursor.finish().unwrap();
    cursor.close().unwrap();
}
#[test]
fn exclusive_profile_is_rejected_without_current_read_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let mut store =
        Store::open_exclusive(&dir.path().join("exclusive.db"), Default::default(), true).unwrap();
    let root = publish(&mut store, "complete", 1, 100);
    assert!(
        store
            .open_read_archive_cursor("query", "complete", root)
            .is_err()
    );
    match Store::open(Path::new(":memory:"), Default::default()) {
        Ok(memory) => assert!(
            memory
                .open_read_archive_cursor("query", "missing", [0; 32])
                .is_err()
        ),
        Err(error) => assert_eq!(error, Error::Invalid("WAL unavailable")),
    };
}
#[test]
fn range_query_uses_composite_index_and_large_archive_pages_preserve_exact_order() {
    let (_dir, path, mut store) = setup();
    let root = publish(&mut store, "complete", 257, 40);
    let sql = rusqlite::Connection::open(&path).unwrap();
    // Planner evidence for the production range shape, independent of timing. This does not
    // instrument the cursor's internal calls; the source SQL is additionally reviewed.
    let query = "EXPLAIN QUERY PLAN SELECT ordinal,CASE WHEN length(payload)<=?3 THEN payload ELSE NULL END FROM read_archive_parts WHERE operation_id=?1 AND ordinal>=?2 ORDER BY ordinal LIMIT ?4";
    let details: Vec<String> = sql
        .prepare(query)
        .unwrap()
        .query_map(
            rusqlite::params![
                "complete",
                128,
                morrow_core::read_archive::MAX_CONTAINER_BYTES as i64,
                64
            ],
            |r| r.get(3),
        )
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        details
            .iter()
            .any(|d| d.contains("SEARCH") && d.contains("operation_id") && d.contains("ordinal")),
        "{details:?}"
    );
    assert!(
        !details.iter().any(|d| d.contains("TEMP B-TREE")),
        "{details:?}"
    );
    let mut cursor = store
        .open_read_archive_cursor("query", "complete", root)
        .unwrap();
    let parts = all(&mut cursor, 64);
    assert_eq!(parts.len(), 257);
    for (ordinal, part) in parts.iter().enumerate() {
        assert_eq!(part.ordinal(), ordinal as u32);
        assert_eq!(
            &part.data()[part.data().len() - 4..],
            &(ordinal as u32).to_le_bytes()
        );
    }
    cursor.finish().unwrap();
}

#[test]
fn eof_retains_transaction_but_close_and_drop_release_the_wal_reader() {
    for explicit in [false, true] {
        let (_dir, path, mut store) = setup();
        let root = publish(&mut store, "complete", 2, 50);
        let mut cursor = store
            .open_read_archive_cursor("query", "complete", root)
            .unwrap();
        store
            .create_local(
                "after-open",
                &CardRecord::new("later", "type", 1, "later", vec![]).unwrap(),
            )
            .unwrap();
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.busy_timeout(std::time::Duration::ZERO).unwrap();
        let busy: i64 = sql
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy, 1, "active read transaction must pin its snapshot");
        assert_eq!(all(&mut cursor, 1).len(), 2);
        cursor.finish().unwrap();
        let busy: i64 = sql
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy, 1, "EOF does not silently discard the reader");
        if explicit {
            cursor.close().unwrap();
        } else {
            drop(cursor);
        }
        let busy: i64 = sql
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy, 0, "close/drop releases the sole read transaction");
        assert!(store.card("later").unwrap().is_some());
    }
}
