#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    content::{self, CardRecord},
    content_change::ContentChange,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    read_journal::Input,
    records::Record,
    store::{CardReadSnapshot, FrozenCard, Store},
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
fn card(id: &str) -> CardRecord {
    CardRecord::new(id, "test.card", 1, "original", b"original body".to_vec()).unwrap()
}
fn setup(count: usize) -> (tempfile::TempDir, PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cards.db");
    let mut store = Store::open(&path, Default::default()).unwrap();
    for n in 0..count {
        let id = format!("c{n:04}");
        store
            .create_local(&format!("create-{id}"), &card(&id))
            .unwrap();
    }
    (dir, path, store)
}
fn scan(snapshot: &mut CardReadSnapshot, count: u32) -> Vec<FrozenCard> {
    let mut entries = Vec::new();
    loop {
        let page = snapshot
            .next_page(count, content::MAX_RECORD_BYTES)
            .unwrap();
        entries.extend(page.entries);
        if page.done {
            return entries;
        }
    }
}
fn edit(store: Store, id: &str) -> HostRuntime {
    let mut host = HostRuntime::new(store).unwrap();
    let mut conn = host.connect().unwrap();
    host.grant(&mut conn, GrantKind::EditContent, id, 100, 0)
        .unwrap();
    host.edit_content(
        &conn,
        &ContentChange {
            operation_id: format!("edit-{id}"),
            card_id: id.into(),
            expected_revision: 1,
            title: "changed".into(),
            body: b"new body".to_vec(),
            preview_text: String::new(),
            attachments: None,
        },
        || 1,
    )
    .unwrap();
    host
}
#[test]
fn independent_writer_cannot_change_pinned_bytes_membership_or_readpoint() {
    let (_dir, path, store) = setup(4);
    let originals: Vec<_> = (0..4)
        .map(|n| store.card(&format!("c{n:04}")).unwrap().unwrap().encode())
        .collect();
    let mut snapshot = store.open_card_snapshot().unwrap();
    let point = snapshot.readpoint().clone();
    let first = snapshot.next_page(1, 1024).unwrap();
    assert_eq!(first.entries[0].original_bytes(), originals[0]);
    let writer = Store::open_existing(&path, Default::default()).unwrap();
    let mut writer = edit(writer, "c0001");
    writer
        .store_local_mut()
        .create_local("create-later", &card("c0004"))
        .unwrap();
    assert_eq!(
        writer
            .store_local()
            .card("c0001")
            .unwrap()
            .unwrap()
            .summary()
            .revision,
        2
    );
    let rest = scan(&mut snapshot, 1);
    assert_eq!(rest.len(), 3);
    for (frozen, expected) in rest.iter().zip(&originals[1..]) {
        assert_eq!(frozen.original_bytes(), expected);
        assert_eq!(frozen.revision(), 1);
    }
    assert_eq!(
        snapshot.readpoint().operation_sequence,
        point.operation_sequence
    );
    assert_eq!(
        snapshot.readpoint().operation_sha256,
        point.operation_sha256
    );
    let census = snapshot.finish().unwrap();
    assert_eq!(census.count, 4);
    let mut fresh = writer.store_local().open_card_snapshot().unwrap();
    let latest = scan(&mut fresh, 128);
    assert_eq!(latest.len(), 5);
    assert_eq!(latest[1].revision(), 2);
}
#[test]
fn all_types_are_returned_and_page_boundaries_do_not_imply_workspace_eof() {
    let (_dir, _path, mut store) = setup(128);
    let target = CardRecord::new("z-target", "org.morrow.idea", 1, "last", vec![]).unwrap();
    store.create_local("target", &target).unwrap();
    let mut snapshot = store.open_card_snapshot().unwrap();
    assert!(snapshot.finish().is_err());
    let first = snapshot.next_page(128, 1024 * 1024).unwrap();
    assert_eq!(first.entries.len(), 128);
    assert!(!first.done);
    assert!(
        first
            .entries
            .iter()
            .all(|c| c.card().summary().type_id != "org.morrow.idea")
    );
    let tail = scan(&mut snapshot, 128);
    assert_eq!(tail.len(), 1);
    assert_eq!(tail[0].id(), "z-target");
    assert_eq!(snapshot.finish().unwrap().count, 129);
    let census = snapshot.finish().unwrap();
    for _ in 0..3 {
        let page = snapshot.next_page(1, 1024).unwrap();
        assert!(page.done && page.entries.is_empty());
        let current = snapshot.finish().unwrap();
        assert_eq!(current.count, census.count);
        assert_eq!(current.sha256, census.sha256);
    }
    let mut other = store.open_card_snapshot().unwrap();
    assert_eq!(scan(&mut other, 1).len(), 129);
    assert_eq!(other.finish().unwrap().sha256, census.sha256);
}
#[test]
fn empty_snapshot_requires_actual_eof_before_a_stable_zero_census() {
    let (_dir, _path, store) = setup(0);
    let mut snapshot = store.open_card_snapshot().unwrap();
    assert!(snapshot.finish().is_err());
    assert_eq!(snapshot.readpoint().operation_sequence, 0);
    assert!(snapshot.readpoint().operation_sha256.is_none());
    let page = snapshot.next_page(128, 1024).unwrap();
    assert!(page.done && page.entries.is_empty());
    assert_eq!(snapshot.finish().unwrap().count, 0);
}
fn varint(mut value: usize, bytes: &mut Vec<u8>) {
    while value >= 128 {
        bytes.push((value as u8) | 128);
        value >>= 7;
    }
    bytes.push(value as u8);
}
fn maximum_unknown_card() -> CardRecord {
    let mut raw = card("maximum").encode();
    let remaining = content::MAX_RECORD_BYTES - raw.len() - 6;
    raw.extend([0xa2, 0x06]);
    varint(remaining, &mut raw);
    let mut random = 0x983721u32;
    for _ in 0..remaining {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        raw.push(random as u8);
    }
    assert_eq!(raw.len(), content::MAX_RECORD_BYTES);
    CardRecord::decode(&raw).unwrap()
}
#[test]
fn legal_eight_mib_unknown_fields_survive_and_small_budget_does_not_advance_cursor() {
    let (_dir, _path, mut store) = setup(0);
    let maximum = maximum_unknown_card();
    store.create_local("maximum-create", &maximum).unwrap();
    let mut snapshot = store.open_card_snapshot().unwrap();
    for (count, bytes) in [
        (0, content::MAX_RECORD_BYTES),
        (129, content::MAX_RECORD_BYTES),
        (1, 0),
        (1, 1024),
    ] {
        assert!(snapshot.next_page(count, bytes).is_err());
        assert!(snapshot.finish().is_err());
    }
    let entries = scan(&mut snapshot, 1);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].original_bytes(), maximum.original_bytes());
    assert_eq!(entries[0].card().encode(), maximum.original_bytes());
    assert_eq!(snapshot.finish().unwrap().count, 1);
}
#[test]
fn frozen_latest_content_commit_is_distinct_from_global_record_and_read_event_head() {
    let (_dir, _path, store) = setup(2);
    let mut host = edit(store, "c0000");
    host.store_local_mut()
        .create_record_local("workspace", &Record::workspace("w", "W").unwrap())
        .unwrap();
    host.store_local_mut()
        .record_read_local_authorized(
            &Input {
                operation_id: "read".into(),
                subject: "c0000".into(),
                request_type: "test.request".into(),
                request: vec![],
                response_type: "test.response".into(),
                response: vec![],
            },
            &[],
            || Ok(()),
        )
        .unwrap();
    let events = host.store_local().pending(0, 10).unwrap();
    assert_eq!(events.len(), 5);
    let mut snapshot = host.store_local().open_card_snapshot().unwrap();
    assert_eq!(snapshot.readpoint().operation_sequence, 5);
    let entries = scan(&mut snapshot, 128);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].latest_operation_id(), "edit-c0000");
    assert_eq!(entries[0].latest_sequence(), 3);
    assert_eq!(entries[0].revision(), 2);
    assert_eq!(entries[1].latest_operation_id(), "create-c0001");
    assert_eq!(entries[1].latest_sequence(), 2);
    assert_eq!(
        entries[0].latest_commit_sha256(),
        <[u8; 32]>::from(Sha256::digest(&events[2].1))
    );
    assert_eq!(
        snapshot.readpoint().operation_sha256,
        Some(<[u8; 32]>::from(Sha256::digest(&events[4].1)))
    );
    assert_eq!(snapshot.finish().unwrap().sha256, expected_census(&entries));
}
#[test]
fn frozen_authorized_read_returns_old_bytes_even_after_current_card_changes() {
    let (_dir, path, store) = setup(1);
    let mut snapshot = store.open_card_snapshot().unwrap();
    let frozen = scan(&mut snapshot, 1).remove(0);
    let mut host = HostRuntime::new(store).unwrap();
    let mut conn = host.connect().unwrap();
    host.grant(&mut conn, GrantKind::ReadContent, "c0000", 100, 0)
        .unwrap();
    let _writer = edit(
        Store::open_existing(&path, Default::default()).unwrap(),
        "c0000",
    );
    let copy = host
        .read_snapshot_content(&conn, &snapshot, &frozen, || 1)
        .unwrap();
    assert_eq!(copy.original_bytes(), frozen.original_bytes());
    assert_eq!(copy.summary().revision, 1);
    assert_eq!(
        host.store_local()
            .card("c0000")
            .unwrap()
            .unwrap()
            .summary()
            .revision,
        2
    );
}
#[test]
fn foreign_store_snapshot_instance_and_object_are_rejected_before_clock_and_do_not_poison_it() {
    let (_dir, path, store) = setup(2);
    let mut first = store.open_card_snapshot().unwrap();
    let frozen = scan(&mut first, 128);
    let mut second = store.open_card_snapshot().unwrap();
    let second_frozen = scan(&mut second, 128);
    let other = Store::open_existing(&path, Default::default()).unwrap();
    let mut foreign_snapshot = other.open_card_snapshot().unwrap();
    let foreign_frozen = scan(&mut foreign_snapshot, 128);
    let mut foreign_host = HostRuntime::new(other).unwrap();
    let mut foreign_conn = foreign_host.connect().unwrap();
    foreign_host
        .grant(&mut foreign_conn, GrantKind::ReadContent, "c0000", 100, 0)
        .unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let mut conn = host.connect().unwrap();
    host.grant(&mut conn, GrantKind::ReadContent, "c0000", 100, 0)
        .unwrap();
    let other_instance = host.connect().unwrap();
    for mismatch in 0..5 {
        let mut called = false;
        let result = match mismatch {
            0 => host.read_snapshot_content(&conn, &first, &second_frozen[0], || {
                called = true;
                u64::MAX
            }),
            1 => host.read_snapshot_content(&conn, &foreign_snapshot, &foreign_frozen[0], || {
                called = true;
                u64::MAX
            }),
            2 => host.read_snapshot_content(&foreign_conn, &first, &frozen[0], || {
                called = true;
                u64::MAX
            }),
            3 => host.read_snapshot_content(&other_instance, &first, &frozen[0], || {
                called = true;
                u64::MAX
            }),
            _ => host.read_snapshot_content(&conn, &first, &frozen[1], || {
                called = true;
                u64::MAX
            }),
        };
        assert!(result.is_err(), "mismatch {mismatch}");
        assert!(!called, "mismatch {mismatch}");
        assert!(
            host.read_snapshot_content(&conn, &first, &frozen[0], || 1)
                .is_ok()
        );
    }
}
#[test]
fn expiry_and_revocation_at_second_clock_prevent_frozen_bytes_delivery() {
    for revoke in [false, true] {
        let (_dir, _path, store) = setup(1);
        let mut snapshot = store.open_card_snapshot().unwrap();
        let frozen = scan(&mut snapshot, 1).remove(0);
        let mut host = HostRuntime::new(store).unwrap();
        let mut conn = host.connect().unwrap();
        host.grant(&mut conn, GrantKind::ReadContent, "c0000", 2, 0)
            .unwrap();
        let signal = host.revocation(&conn).unwrap();
        let mut ticks = 0;
        let result = host.read_snapshot_content(&conn, &snapshot, &frozen, || {
            ticks += 1;
            if ticks == 2 {
                if revoke {
                    signal.revoke();
                    1
                } else {
                    2
                }
            } else {
                1
            }
        });
        assert!(result.is_err());
        assert_eq!(ticks, 2);
    }
}
#[test]
fn revoked_object_grant_cannot_be_recovered_from_frozen_card() {
    let (_dir, _path, store) = setup(1);
    let mut snapshot = store.open_card_snapshot().unwrap();
    let frozen = scan(&mut snapshot, 1).remove(0);
    let mut host = HostRuntime::new(store).unwrap();
    let mut conn = host.connect().unwrap();
    host.grant(&mut conn, GrantKind::ReadContent, "c0000", 100, 0)
        .unwrap();
    host.revoke(&mut conn, GrantKind::ReadContent, "c0000")
        .unwrap();
    let mut called = false;
    assert!(
        host.read_snapshot_content(&conn, &snapshot, &frozen, || {
            called = true;
            u64::MAX
        })
        .is_err()
    );
    assert!(!called);
    host.grant(&mut conn, GrantKind::ReadContent, "c0000", 100, 1)
        .unwrap();
    assert!(
        host.read_snapshot_content(&conn, &snapshot, &frozen, || 1)
            .is_ok()
    );
}
#[test]
fn unsupported_memory_and_exclusive_profiles_fail_closed() {
    match Store::open(Path::new(":memory:"), Default::default()) {
        Ok(memory) => assert!(memory.open_card_snapshot().is_err()),
        Err(error) => assert_eq!(error, morrow_core::Error::Invalid("WAL unavailable")),
    }
    let dir = tempfile::tempdir().unwrap();
    let exclusive =
        Store::open_exclusive(&dir.path().join("exclusive.db"), Default::default(), true).unwrap();
    assert!(exclusive.open_card_snapshot().is_err());
}
#[test]
fn corrupted_or_missing_card_commit_and_oversized_sql_data_cannot_complete_census() {
    for damage in 0..5 {
        let (_dir, path, store) = setup(2);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        match damage {
            0 => {
                sql.execute_batch("UPDATE cards SET payload=x'00' WHERE id='c0000'")
                    .unwrap();
            }
            1 => {
                sql.execute_batch("DELETE FROM operations WHERE id='create-c0000'")
                    .unwrap();
            }
            2 => {
                sql.execute_batch("UPDATE operations SET payload=x'00' WHERE id='create-c0000'")
                    .unwrap();
            }
            3 => {
                sql.execute(
                    "UPDATE cards SET payload=zeroblob(?1) WHERE id='c0000'",
                    [(morrow_core::envelope::MAX_CONTAINER_BYTES + 1) as i64],
                )
                .unwrap();
            }
            _ => {
                sql.execute_batch("DELETE FROM operation_events WHERE id='create-c0000'")
                    .unwrap();
            }
        }
        if let Ok(mut snapshot) = store.open_card_snapshot() {
            assert!(
                snapshot.next_page(128, content::MAX_RECORD_BYTES).is_err(),
                "damage {damage}"
            );
            assert!(snapshot.finish().is_err());
        }
    }
}

fn hash_text(hash: &mut Sha256, text: &str) {
    hash.update((text.len() as u32).to_le_bytes());
    hash.update(text.as_bytes());
}
fn expected_census(entries: &[FrozenCard]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"Morrow/card-census/v1\0");
    for (ordinal, entry) in entries.iter().enumerate() {
        let summary = entry.card().summary();
        hash.update((ordinal as u64).to_le_bytes());
        hash_text(&mut hash, entry.id());
        hash_text(&mut hash, &summary.type_id);
        hash.update(summary.format_version.to_le_bytes());
        hash.update(entry.revision().to_le_bytes());
        hash.update((entry.original_bytes().len() as u64).to_le_bytes());
        hash.update(Sha256::digest(entry.original_bytes()));
        hash_text(&mut hash, entry.latest_operation_id());
        hash.update(entry.latest_sequence().to_le_bytes());
        hash.update(entry.latest_commit_sha256());
    }
    hash.update(b"\0end");
    hash.update((entries.len() as u64).to_le_bytes());
    hash.finalize().into()
}
#[test]
fn census_matches_independent_fixed_encoding_and_exact_byte_page_boundaries() {
    let (_dir, _path, store) = setup(3);
    let mut snapshot = store.open_card_snapshot().unwrap();
    let size = store.card("c0000").unwrap().unwrap().original_bytes().len();
    let first = snapshot.next_page(128, size).unwrap();
    assert_eq!(first.entries.len(), 1);
    assert!(!first.done);
    let mut entries = first.entries;
    entries.extend(scan(&mut snapshot, 128));
    assert_eq!(
        entries.iter().map(|e| e.id()).collect::<Vec<_>>(),
        ["c0000", "c0001", "c0002"]
    );
    assert_eq!(snapshot.finish().unwrap().sha256, expected_census(&entries));
    let (_empty_dir, _empty_path, empty) = setup(0);
    let mut empty_snapshot = empty.open_card_snapshot().unwrap();
    assert!(scan(&mut empty_snapshot, 1).is_empty());
    assert_eq!(
        empty_snapshot.finish().unwrap().sha256,
        expected_census(&[])
    );
}
#[test]
fn readonly_audited_wal_snapshot_owns_source_after_store_drop() {
    use morrow_core::audit::{SigningKey, TrustedLog};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audited.db");
    let key = SigningKey::from_bytes(&[43; 32]);
    let trust = TrustedLog {
        id: "snapshot-review".into(),
        key: key.verifying_key(),
    };
    let mut source = Store::open_audited(&path, Default::default(), true, trust.clone()).unwrap();
    source.create_local("create", &card("card")).unwrap();
    drop(source);
    let readonly = Store::open_read_only_audited(&path, trust).unwrap();
    let mut snapshot = readonly.open_card_snapshot().unwrap();
    drop(readonly);
    let entries = scan(&mut snapshot, 1);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].original_bytes(), card("card").original_bytes());
    assert_eq!(snapshot.finish().unwrap().sha256, expected_census(&entries));
}

#[test]
fn corrupted_empty_id_is_not_silently_excluded_from_complete_census() {
    let (_dir, path, store) = setup(1);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute(
        "INSERT INTO cards(id,payload) VALUES('',?1)",
        [morrow_core::envelope::encode(&card("not-empty")).unwrap()],
    )
    .unwrap();
    if let Ok(mut snapshot) = store.open_card_snapshot() {
        assert!(snapshot.next_page(128, content::MAX_RECORD_BYTES).is_err());
        assert!(snapshot.finish().is_err());
    }
}
#[test]
fn corrupt_read_event_head_cannot_be_used_as_a_valid_readpoint() {
    let (_dir, path, mut store) = setup(1);
    store
        .record_read_local_authorized(
            &Input {
                operation_id: "tail".into(),
                subject: "query".into(),
                request_type: "request".into(),
                request: vec![],
                response_type: "response".into(),
                response: vec![],
            },
            &[],
            || Ok(()),
        )
        .unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("UPDATE operations SET payload=x'00' WHERE id='tail'")
        .unwrap();
    assert!(store.open_card_snapshot().is_err());
}
#[test]
fn integrity_failure_poisoning_also_rejects_previously_frozen_authorized_cards() {
    let (_dir, path, store) = setup(3);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("UPDATE cards SET payload=x'00' WHERE id='c0001'")
        .unwrap();
    let mut snapshot = store.open_card_snapshot().unwrap();
    let first = snapshot.next_page(1, 1024).unwrap().entries.remove(0);
    assert!(snapshot.next_page(1, 1024).is_err());
    assert!(snapshot.finish().is_err());
    assert!(snapshot.next_page(1, 1024).is_err());
    let mut host = HostRuntime::new(store).unwrap();
    let mut conn = host.connect().unwrap();
    host.grant(&mut conn, GrantKind::ReadContent, "c0000", 100, 0)
        .unwrap();
    let mut called = false;
    assert!(
        host.read_snapshot_content(&conn, &snapshot, &first, || {
            called = true;
            u64::MAX
        })
        .is_err()
    );
    assert!(!called);
}
#[test]
fn empty_snapshots_have_store_identity_and_explicit_close_does_not_close_source() {
    let (_dir, path, store) = setup(0);
    let snapshot = store.open_card_snapshot().unwrap();
    let mut other = Store::open_existing(&path, Default::default()).unwrap();
    store.validate_card_snapshot(&snapshot).unwrap();
    assert!(other.validate_card_snapshot(&snapshot).is_err());
    snapshot.close().unwrap();
    other.create_local("after-close", &card("later")).unwrap();
    let mut next = store.open_card_snapshot().unwrap();
    assert_eq!(scan(&mut next, 1).len(), 1);
}

#[test]
fn missing_card_referenced_by_permanent_content_history_cannot_complete_census() {
    let (_dir, path, store) = setup(2);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DELETE FROM cards WHERE id='c0000'")
        .unwrap();
    // The highest operation still belongs to intact c0001; validating only the head and
    // the cards which remain would incorrectly claim that this library has one card.
    assert!(store.open_card_snapshot().is_err());
}
