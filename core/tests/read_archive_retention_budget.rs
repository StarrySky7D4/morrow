#![cfg(not(target_arch = "wasm32"))]
// Opaque trusted-local storage fixtures. They do not assert guest execution or source provenance.
use morrow_core::{
    Error,
    read_archive::{
        Budget, Finish, MAX_RETAINED_ARCHIVES, MAX_RETAINED_BYTES, Plan, RetentionBudget,
        RetentionUsage,
    },
    store::Store,
};
use std::path::{Path, PathBuf};
fn plan(id: &str, subject: &str) -> Plan {
    Plan {
        operation_id: id.into(),
        subject: subject.into(),
        request_type: "review.request".into(),
        request: b"original request".to_vec(),
        response_type: "review.response".into(),
        budget: Budget::default(),
    }
}
fn setup() -> (tempfile::TempDir, PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("retention.db");
    let store = Store::open(&path, Default::default()).unwrap();
    (dir, path, store)
}
fn finish(store: &Store, id: &str, subject: &str) -> Finish {
    let s = store.lookup_read_archive(subject, id).unwrap().unwrap();
    Finish {
        response: b"ready".to_vec(),
        part_count: s.count,
        logical_bytes: s.logical_bytes,
        chain_sha256: s.chain_sha256,
        metadata_type: "review.metadata".into(),
        metadata: b"fixed metadata".to_vec(),
    }
}
fn publish(store: &mut Store, id: &str, subject: &str) {
    store.begin_read_archive(&plan(id, subject)).unwrap();
    store
        .append_read_archive(subject, id, 0, "review.part", b"original bytes")
        .unwrap();
    store
        .finish_read_archive_local_authorized(subject, id, &finish(store, id, subject), &[], || {
            Ok(())
        })
        .unwrap();
}
fn count(path: &Path, table: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn random_bytes(size: usize) -> Vec<u8> {
    let mut n = 0x127a835du32;
    (0..size)
        .map(|_| {
            n ^= n << 13;
            n ^= n >> 17;
            n ^= n << 5;
            n as u8
        })
        .collect()
}

fn archive_cost(store: &Store, id: &str, subject: &str) -> u64 {
    let m = store.read_archive_manifest(subject, id).unwrap().unwrap();
    (m.raw().len() + m.container().len()) as u64 + m.status().logical_bytes
}
#[test]
fn published_archives_and_final_metadata_remain_charged_exactly() {
    let (_dir, path, mut store) = setup();
    assert_eq!(
        store.read_archive_retention_usage().unwrap(),
        RetentionUsage {
            archives: 0,
            logical_bytes: 0
        }
    );
    store.begin_read_archive(&plan("one", "a")).unwrap();
    assert_eq!(
        store.read_archive_retention_usage().unwrap().logical_bytes,
        archive_cost(&store, "one", "a")
    );
    store
        .append_read_archive("a", "one", 0, "review.part", &random_bytes(65537))
        .unwrap();
    let before = archive_cost(&store, "one", "a");
    let mut end = finish(&store, "one", "a");
    end.metadata = random_bytes(131073);
    store
        .finish_read_archive_local_authorized("a", "one", &end, &[], || Ok(()))
        .unwrap();
    let published = archive_cost(&store, "one", "a");
    assert!(published > before + end.metadata.len() as u64);
    assert_eq!(store.read_archive_preparation_usage().unwrap().archives, 0);
    assert_eq!(
        store.read_archive_retention_usage().unwrap(),
        RetentionUsage {
            archives: 1,
            logical_bytes: published
        }
    );
    publish(&mut store, "two", "b");
    let total = published + archive_cost(&store, "two", "b");
    assert_eq!(
        store.read_archive_retention_usage().unwrap(),
        RetentionUsage {
            archives: 2,
            logical_bytes: total
        }
    );
    drop(store);
    let store = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        store.read_archive_retention_usage().unwrap().logical_bytes,
        total
    );
    store.integrity_check().unwrap();
}
#[test]
fn final_metadata_growth_is_rejected_atomically_but_original_retry_is_free_under_lower_policy() {
    let (_dir, path, mut store) = setup();
    store
        .begin_read_archive(&plan("operation", "query"))
        .unwrap();
    store
        .append_read_archive("query", "operation", 0, "review.part", b"original")
        .unwrap();
    let before = store.read_archive_retention_usage().unwrap();
    let original = store
        .read_archive_manifest("query", "operation")
        .unwrap()
        .unwrap();
    store
        .set_read_archive_retention_budget(RetentionBudget {
            max_archives: 1,
            max_bytes: before.logical_bytes,
        })
        .unwrap();
    store
        .begin_read_archive(&plan("operation", "query"))
        .unwrap();
    store
        .append_read_archive("query", "operation", 0, "review.part", b"original")
        .unwrap();
    assert_eq!(store.read_archive_retention_usage().unwrap(), before);
    let end = finish(&store, "operation", "query");
    assert!(matches!(
        store.finish_read_archive_local_authorized("query", "operation", &end, &[], || Ok(())),
        Err(Error::ArchiveCapacity)
    ));
    assert_eq!(count(&path, "operations"), 0);
    assert_eq!(
        store
            .read_archive_manifest("query", "operation")
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
    assert_eq!(store.read_archive_retention_usage().unwrap(), before);
    store
        .set_read_archive_retention_budget(RetentionBudget::default())
        .unwrap();
    let receipt = store
        .finish_read_archive_local_authorized("query", "operation", &end, &[], || Ok(()))
        .unwrap();
    let published = store.read_archive_retention_usage().unwrap();
    store
        .set_read_archive_retention_budget(RetentionBudget {
            max_archives: 1,
            max_bytes: 1,
        })
        .unwrap();
    assert!(
        store
            .finish_read_archive_local_authorized("query", "operation", &end, &[], || Err(
                Error::Integrity
            ))
            .is_err()
    );
    assert_eq!(
        store
            .finish_read_archive_local_authorized("query", "operation", &end, &[], || Ok(()))
            .unwrap()
            .observation_sha256,
        receipt.observation_sha256
    );
    assert_eq!(store.read_archive_retention_usage().unwrap(), published);
    assert!(matches!(
        store.begin_read_archive(&plan("new", "other")),
        Err(Error::ArchiveCapacity)
    ));
    assert_eq!(count(&path, "read_archives"), 1);
}
#[test]
fn failed_begin_append_and_abort_only_charge_committed_originals() {
    let (_dir, path, mut store) = setup();
    store
        .set_read_archive_retention_budget(RetentionBudget {
            max_archives: 1,
            max_bytes: 1,
        })
        .unwrap();
    assert!(matches!(
        store.begin_read_archive(&plan("operation", "query")),
        Err(Error::ArchiveCapacity)
    ));
    assert_eq!(store.read_archive_retention_usage().unwrap().archives, 0);
    assert_eq!(count(&path, "read_archive_costs"), 0);
    store
        .set_read_archive_retention_budget(RetentionBudget::default())
        .unwrap();
    store
        .begin_read_archive(&plan("operation", "query"))
        .unwrap();
    let initial = store.read_archive_retention_usage().unwrap();
    store
        .set_read_archive_retention_budget(RetentionBudget {
            max_archives: 1,
            max_bytes: initial.logical_bytes,
        })
        .unwrap();
    assert!(matches!(
        store.append_read_archive("query", "operation", 0, "review.part", b"growth"),
        Err(Error::ArchiveCapacity)
    ));
    assert_eq!(store.read_archive_retention_usage().unwrap(), initial);
    assert_eq!(count(&path, "read_archive_parts"), 0);
    store
        .set_read_archive_retention_budget(RetentionBudget {
            max_archives: 1,
            max_bytes: 1,
        })
        .unwrap();
    assert!(store.abort_read_archive("query", "operation").unwrap());
    assert_eq!(
        store.read_archive_retention_usage().unwrap(),
        RetentionUsage {
            archives: 0,
            logical_bytes: 0
        }
    );
    assert!(!store.abort_read_archive("query", "operation").unwrap());
    assert_eq!(count(&path, "read_archive_costs"), 0);
    store.integrity_check().unwrap();
}
#[test]
fn concurrent_subjects_and_stores_cannot_both_claim_last_published_plus_preparing_slot() {
    let (_dir, path, mut store) = setup();
    publish(&mut store, "published", "history");
    drop(store);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = [
        Store::open_existing(&path, Default::default()).unwrap(),
        Store::open_existing(&path, Default::default()).unwrap(),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, mut s)| {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            s.set_read_archive_retention_budget(RetentionBudget {
                max_archives: 2,
                max_bytes: 1024 * 1024,
            })
            .unwrap();
            barrier.wait();
            let result = s.begin_read_archive(&plan(&format!("race-{i}"), &format!("subject-{i}")));
            match result {
                Ok(_) => true,
                Err(Error::ArchiveCapacity | Error::StorageBusy) => false,
                Err(e) => panic!("{e:?}"),
            }
        })
    })
    .collect();
    assert_eq!(
        handles
            .into_iter()
            .map(|h| u32::from(h.join().unwrap()))
            .sum::<u32>(),
        1
    );
    let mut store = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(store.read_archive_retention_usage().unwrap().archives, 2);
    store
        .set_read_archive_retention_budget(RetentionBudget {
            max_archives: 2,
            max_bytes: 1024 * 1024,
        })
        .unwrap();
    assert!(matches!(
        store.begin_read_archive(&plan("another", "another")),
        Err(Error::ArchiveCapacity)
    ));
    assert_eq!(count(&path, "read_archive_costs"), 2);
    store.integrity_check().unwrap();
}
#[test]
fn per_store_policy_is_not_persistent_and_invalid_limits_do_not_replace_it() {
    let (_dir, path, mut store) = setup();
    store
        .set_read_archive_retention_budget(RetentionBudget {
            max_archives: 1,
            max_bytes: 1024 * 1024,
        })
        .unwrap();
    publish(&mut store, "one", "one");
    for budget in [
        RetentionBudget {
            max_archives: 0,
            max_bytes: 1,
        },
        RetentionBudget {
            max_archives: 1,
            max_bytes: 0,
        },
        RetentionBudget {
            max_archives: MAX_RETAINED_ARCHIVES + 1,
            max_bytes: 1,
        },
        RetentionBudget {
            max_archives: 1,
            max_bytes: MAX_RETAINED_BYTES + 1,
        },
    ] {
        assert!(matches!(
            store.set_read_archive_retention_budget(budget),
            Err(Error::Limit)
        ));
    }
    assert!(matches!(
        store.begin_read_archive(&plan("two", "two")),
        Err(Error::ArchiveCapacity)
    ));
    let mut other = Store::open_existing(&path, Default::default()).unwrap();
    publish(&mut other, "two", "two");
    assert_eq!(store.read_archive_retention_usage().unwrap().archives, 2);
    assert!(matches!(
        store.begin_read_archive(&plan("three", "three")),
        Err(Error::ArchiveCapacity)
    ));
}
#[test]
fn tracked_capture_uses_same_retention_gate_and_cancellation_releases_only_archive_cost() {
    use morrow_core::read_capture::Phase;
    let (_dir, path, mut store) = setup();
    store
        .set_read_archive_retention_budget(RetentionBudget {
            max_archives: 1,
            max_bytes: 1,
        })
        .unwrap();
    assert!(matches!(
        store.begin_read_capture(
            &plan("capture", "query"),
            "review.context",
            b"facts",
            [47; 32]
        ),
        Err(Error::ArchiveCapacity)
    ));
    assert_eq!(count(&path, "read_captures"), 0);
    assert_eq!(count(&path, "read_archives"), 0);
    store
        .set_read_archive_retention_budget(RetentionBudget::default())
        .unwrap();
    let initial = store
        .begin_read_capture(
            &plan("capture", "query"),
            "review.context",
            b"facts",
            [47; 32],
        )
        .unwrap();
    let charge = archive_cost(&store, "capture", "query");
    assert_eq!(
        store.read_archive_retention_usage().unwrap().logical_bytes,
        charge
    );
    store
        .set_read_archive_retention_budget(RetentionBudget {
            max_archives: 1,
            max_bytes: charge,
        })
        .unwrap();
    assert!(matches!(
        store.append_read_capture(
            "query",
            "capture",
            &initial.token(),
            0,
            "review.part",
            b"growth"
        ),
        Err(Error::ArchiveCapacity)
    ));
    assert!(matches!(
        store.finish_read_capture_local_authorized(
            "query",
            "capture",
            &initial.token(),
            &finish(&store, "capture", "query"),
            &[],
            || Ok(())
        ),
        Err(Error::ArchiveCapacity)
    ));
    assert_eq!(
        store
            .lookup_read_capture("query", "capture")
            .unwrap()
            .unwrap()
            .container(),
        initial.container()
    );
    store
        .end_read_capture(
            "query",
            "capture",
            &initial.token(),
            Phase::Cancelled,
            "cancel",
        )
        .unwrap();
    assert_eq!(store.read_archive_retention_usage().unwrap().archives, 0);
    assert_eq!(
        store.read_capture_usage().unwrap().charged_bytes,
        initial.charged_bytes()
    );
    assert_eq!(count(&path, "read_captures"), 1);
    assert_eq!(count(&path, "read_archive_costs"), 0);
}

fn install_failure(path: &Path) {
    rusqlite::Connection::open(path).unwrap().execute_batch("CREATE TABLE review_parent(id INTEGER PRIMARY KEY);CREATE TABLE review_deferred(id INTEGER PRIMARY KEY,parent INTEGER REFERENCES review_parent(id) DEFERRABLE INITIALLY DEFERRED);CREATE TRIGGER review_failure AFTER UPDATE ON read_archive_totals BEGIN INSERT INTO review_deferred(id,parent) VALUES(1,99);END;").unwrap();
}
fn remove_failure(path: &Path) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch(
            "DROP TRIGGER review_failure;DROP TABLE review_deferred;DROP TABLE review_parent;",
        )
        .unwrap();
}
fn mutate(store: &mut Store, mode: &str) -> morrow_core::Result<()> {
    match mode {
        "begin" => store
            .begin_read_archive(&plan("operation", "query"))
            .map(|_| ()),
        "append" => store
            .append_read_archive("query", "operation", 0, "review.part", b"new original")
            .map(|_| ()),
        "final" => store
            .finish_read_archive_local_authorized(
                "query",
                "operation",
                &finish(store, "operation", "query"),
                &[],
                || Ok(()),
            )
            .map(|_| ()),
        "abort" => store.abort_read_archive("query", "operation").map(|_| ()),
        _ => panic!("unexpected review mode"),
    }
}
#[test]
fn actual_commit_failure_rolls_back_ledger_together_with_each_archive_mutation() {
    for mode in ["begin", "append", "final", "abort"] {
        let (_dir, path, mut store) = setup();
        if mode != "begin" {
            store
                .begin_read_archive(&plan("operation", "query"))
                .unwrap();
        }
        let before = store.read_archive_retention_usage().unwrap();
        let manifest = store.read_archive_manifest("query", "operation").unwrap();
        install_failure(&path);
        let result = mutate(&mut store, mode);
        assert!(
            matches!(result, Err(Error::CommitUnknown)),
            "{mode}: {result:?}"
        );
        assert_eq!(store.read_archive_retention_usage().unwrap(), before);
        assert_eq!(
            store
                .read_archive_manifest("query", "operation")
                .unwrap()
                .as_ref()
                .map(|m| m.container()),
            manifest.as_ref().map(|m| m.container())
        );
        assert_eq!(count(&path, "read_archive_parts"), 0);
        assert_eq!(count(&path, "operations"), 0);
        remove_failure(&path);
        mutate(&mut store, mode).unwrap();
        store.integrity_check().unwrap();
    }
}
#[test]
fn final_authorization_failure_does_not_retain_metadata_charge_or_read_event() {
    let (_dir, path, mut store) = setup();
    store
        .begin_read_archive(&plan("operation", "query"))
        .unwrap();
    let before = store.read_archive_retention_usage().unwrap();
    assert!(
        store
            .finish_read_archive_local_authorized(
                "query",
                "operation",
                &finish(&store, "operation", "query"),
                &[],
                || Err(Error::Integrity)
            )
            .is_err()
    );
    assert_eq!(store.read_archive_retention_usage().unwrap(), before);
    assert!(store.lookup_read("query", "operation").unwrap().is_none());
    assert_eq!(count(&path, "operation_read_archives"), 0);
    store.integrity_check().unwrap();
}
#[test]
fn corrupted_accounting_is_rejected_by_integrity_and_reopening_without_rebuild() {
    for damage in 0..6 {
        let (_dir, path, mut store) = setup();
        publish(&mut store, "published", "query");
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        sql.execute_batch(match damage {
            0 => "DELETE FROM read_archive_costs",
            1 => "UPDATE read_archive_costs SET logical_bytes=logical_bytes+1",
            2 => "UPDATE read_archive_totals SET archives=0",
            3 => "UPDATE read_archive_totals SET logical_bytes=logical_bytes-1",
            4 => "DELETE FROM read_archive_totals",
            _ => "INSERT INTO read_archive_costs(operation_id,logical_bytes) VALUES('orphan',1)",
        })
        .unwrap();
        assert!(store.integrity_check().is_err(), "damage {damage}");
        drop(store);
        assert!(
            Store::open_existing(&path, Default::default()).is_err(),
            "damage {damage}"
        );
        assert_eq!(count(&path, "read_archives"), 1);
    }
}
fn downgrade_to_thirteen(path: &Path) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch(
            "DROP TABLE IF EXISTS tls_identities; DROP TABLE IF EXISTS outbound_authorities; DROP TABLE IF EXISTS service_authority_identity; DROP TABLE IF EXISTS service_authorities; DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; DROP TABLE io_reservations; DROP TABLE io_intents; DROP TABLE read_archive_costs;DROP TABLE read_archive_totals;PRAGMA user_version=13;",
        )
        .unwrap();
}
fn version(path: &Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}
#[test]
fn migration_and_snapshot_preserve_originals_and_signatures_with_self_contained_ledger() {
    use morrow_core::audit::{self, SigningKey, TrustedLog};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("signed.db");
    let key = SigningKey::from_bytes(&[47; 32]);
    let trust = TrustedLog {
        id: "retention-migration".into(),
        key: key.verifying_key(),
    };
    let mut store = Store::open_audited(&path, Default::default(), true, trust.clone()).unwrap();
    publish(&mut store, "published", "query");
    let observed = store.lookup_read("query", "published").unwrap().unwrap();
    let part = store
        .read_archive_part("query", "published", 0)
        .unwrap()
        .unwrap();
    let manifest = store
        .read_archive_manifest("query", "published")
        .unwrap()
        .unwrap();
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &store.pending(0, 10).unwrap()).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    store.seal_pending(&signed).unwrap();
    drop(store);
    downgrade_to_thirteen(&path);
    let old = Store::open_read_only_audited(&path, trust.clone()).unwrap();
    assert_eq!(
        old.lookup_read("query", "published")
            .unwrap()
            .unwrap()
            .container(),
        observed.container()
    );
    drop(old);
    assert_eq!(version(&path), 13);
    let store = Store::open_audited(&path, Default::default(), false, trust.clone()).unwrap();
    assert_eq!(version(&path), morrow_core::store::SCHEMA_VERSION);
    assert_eq!(
        store
            .lookup_read("query", "published")
            .unwrap()
            .unwrap()
            .raw(),
        observed.raw()
    );
    assert_eq!(store.sealed_segment(1).unwrap().unwrap(), signed);
    let usage = store.read_archive_retention_usage().unwrap();
    assert_eq!(usage.archives, 1);
    let target = dir.path().join("snapshot.db");
    store.snapshot_to(&target, 16 * 1024 * 1024).unwrap();
    drop(store);
    std::fs::remove_file(&path).unwrap();
    let backup = Store::open_read_only_audited(&target, trust).unwrap();
    assert_eq!(backup.read_archive_retention_usage().unwrap(), usage);
    assert_eq!(
        backup
            .read_archive_manifest("query", "published")
            .unwrap()
            .unwrap()
            .container(),
        manifest.container()
    );
    assert_eq!(
        backup
            .read_archive_part("query", "published", 0)
            .unwrap()
            .unwrap()
            .container(),
        part.container()
    );
    assert_eq!(
        backup
            .lookup_read("query", "published")
            .unwrap()
            .unwrap()
            .container(),
        observed.container()
    );
    assert_eq!(backup.sealed_segment(1).unwrap().unwrap(), signed);
    backup.integrity_check().unwrap();
}
fn packed_manifest(raw: &[u8]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let data = lz4_flex::block::compress(raw);
    let mut out = b"MRWAMNF1".to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend_from_slice(&data);
    out
}
#[test]
fn legacy_over_hard_archive_count_is_preserved_readable_and_blocks_only_growth() {
    use prost::Message;
    use sha2::{Digest, Sha256};
    let (_dir, path, mut store) = setup();
    publish(&mut store, "published", "query");
    let original = store.lookup_read("query", "published").unwrap().unwrap();
    drop(store);
    downgrade_to_thirteen(&path);
    // Valid legacy unsigned preparations, not fabricated counter values or published reads.
    // Historical over-cap preparations are retained; no claim that DB13 admission created them.
    let mut sql = rusqlite::Connection::open(&path).unwrap();
    let tx = sql.transaction().unwrap();
    for i in 0..MAX_RETAINED_ARCHIVES {
        let id = format!("legacy-{i:05}");
        let p = morrow_core::read_archive::proto::Plan {
            schema_version: 1,
            operation_id: id.clone(),
            subject: "legacy".into(),
            request_type: "review.request".into(),
            request: vec![],
            response_type: "review.response".into(),
            max_parts: 1,
            max_bytes: 1024,
        };
        let raw = p.encode_to_vec();
        let manifest = morrow_core::read_archive::proto::Manifest {
            schema_version: 1,
            plan: raw.clone(),
            part_count: 0,
            logical_bytes: 0,
            chain_sha256: Sha256::digest(&raw).to_vec(),
            finalized: None,
            capture_binding: vec![],
        };
        tx.execute("INSERT INTO read_archives(operation_id,subject,published,payload) VALUES(?1,'legacy',0,?2)",rusqlite::params![id,packed_manifest(&manifest.encode_to_vec())]).unwrap();
    }
    tx.commit().unwrap();
    drop(sql);
    let mut migrated = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(version(&path), morrow_core::store::SCHEMA_VERSION);
    assert_eq!(
        migrated.read_archive_retention_usage().unwrap().archives,
        u64::from(MAX_RETAINED_ARCHIVES) + 1
    );
    assert_eq!(
        migrated
            .lookup_read("query", "published")
            .unwrap()
            .unwrap()
            .container(),
        original.container()
    );
    assert!(
        migrated
            .read_archive_part("query", "published", 0)
            .unwrap()
            .is_some()
    );
    migrated
        .begin_read_archive(&plan("published", "query"))
        .unwrap();
    assert_eq!(
        migrated
            .finish_read_archive_local_authorized(
                "query",
                "published",
                &finish(&migrated, "published", "query"),
                &[],
                || Ok(())
            )
            .unwrap()
            .observation_sha256,
        original.digest()
    );
    assert!(matches!(
        migrated.begin_read_archive(&plan("growth", "other")),
        Err(Error::ArchiveCapacity) | Err(Error::Limit)
    ));
    let before = migrated.read_archive_retention_usage().unwrap();
    migrated
        .abort_read_archive("legacy", "legacy-00000")
        .unwrap();
    assert_eq!(
        migrated.read_archive_retention_usage().unwrap().archives,
        before.archives - 1
    );
    assert_eq!(
        count(&path, "read_archives"),
        i64::from(MAX_RETAINED_ARCHIVES)
    );
    migrated.integrity_check().unwrap();
}

#[test]
fn concurrent_byte_admission_reserves_actual_original_cost_across_store_handles() {
    let (_dir, path, mut store) = setup();
    publish(&mut store, "published", "history");
    let initial = store.read_archive_retention_usage().unwrap();
    let mut largest = 0;
    for i in 0..2 {
        let id = format!("race-{i}");
        store.begin_read_archive(&plan(&id, "query")).unwrap();
        largest = largest.max(archive_cost(&store, &id, "query"));
        store.abort_read_archive("query", &id).unwrap();
    }
    let budget = RetentionBudget {
        max_archives: 3,
        max_bytes: initial.logical_bytes + largest,
    };
    drop(store);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = [
        Store::open_existing(&path, Default::default()).unwrap(),
        Store::open_existing(&path, Default::default()).unwrap(),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, mut store)| {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            store.set_read_archive_retention_budget(budget).unwrap();
            barrier.wait();
            match store.begin_read_archive(&plan(&format!("race-{i}"), "query")) {
                Ok(_) => true,
                Err(Error::ArchiveCapacity | Error::StorageBusy) => false,
                Err(e) => panic!("{e:?}"),
            }
        })
    })
    .collect();
    assert_eq!(
        handles
            .into_iter()
            .map(|h| u32::from(h.join().unwrap()))
            .sum::<u32>(),
        1
    );
    let store = Store::open_existing(&path, Default::default()).unwrap();
    let usage = store.read_archive_retention_usage().unwrap();
    assert_eq!(usage.archives, 2);
    assert!(usage.logical_bytes <= budget.max_bytes);
    store.integrity_check().unwrap();
}
#[cfg(feature = "fault-injection")]
#[test]
fn retention_crash_child() {
    let Some(path) = std::env::var_os("MORROW_RETENTION_REVIEW_DB") else {
        return;
    };
    let mut store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
    let mode = std::env::var("MORROW_RETENTION_REVIEW_MODE").unwrap();
    if mode != "migrate" {
        mutate(&mut store, &mode).unwrap();
    }
}
#[cfg(feature = "fault-injection")]
fn crash(path: &Path, mode: &str, point: &str) {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "retention_crash_child", "--nocapture"])
        .env("MORROW_RETENTION_REVIEW_DB", path)
        .env("MORROW_RETENTION_REVIEW_MODE", mode)
        .env("MORROW_TEST_CRASH_AT", point)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                assert_eq!(status.code(), Some(86), "{point}");
                return;
            }
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe {point}: {e}");
            }
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timeout {point}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn crash_after_accounting_or_commit_leaves_archive_and_ledger_in_same_version() {
    for mode in ["begin", "append", "final", "abort"] {
        for committed in [false, true] {
            let (_dir, path, mut store) = setup();
            if mode != "begin" {
                store
                    .begin_read_archive(&plan("operation", "query"))
                    .unwrap();
            }
            let before = store.read_archive_retention_usage().unwrap();
            drop(store);
            let point = if !committed {
                "retention-after-accounting"
            } else {
                match mode {
                    "begin" => "archive-after-begin-commit",
                    "append" => "archive-after-append-commit",
                    "final" => "archive-after-commit",
                    _ => "read-archive-abort-after-commit",
                }
            };
            crash(&path, mode, point);
            let mut store = Store::open_existing(&path, Default::default()).unwrap();
            let after = store.read_archive_retention_usage().unwrap();
            if !committed {
                assert_eq!(after, before, "{mode}");
            } else {
                match mode {
                    "begin" => assert_eq!(after.archives, 1),
                    "append" | "final" => assert!(after.logical_bytes > before.logical_bytes),
                    _ => assert_eq!(after.archives, 0),
                }
            }
            assert_eq!(
                count(&path, "operations"),
                i64::from(mode == "final" && committed)
            );
            assert_eq!(count(&path, "read_archive_costs"), after.archives as i64);
            if !committed {
                mutate(&mut store, mode).unwrap();
            }
            store.integrity_check().unwrap();
        }
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn migration_crashes_preserve_old_originals_and_publish_whole_derived_ledger() {
    for (point, expected) in [
        ("retention-migration-before-commit", 13),
        ("retention-migration-after-commit", 14),
    ] {
        let (_dir, path, mut store) = setup();
        publish(&mut store, "published", "query");
        let original = store.lookup_read("query", "published").unwrap().unwrap();
        let before = store.read_archive_retention_usage().unwrap();
        drop(store);
        downgrade_to_thirteen(&path);
        crash(&path, "migrate", point);
        assert_eq!(version(&path), expected);
        let tablecount:i64=rusqlite::Connection::open(&path).unwrap().query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('read_archive_costs','read_archive_totals')",[],|r|r.get(0)).unwrap();
        assert_eq!(tablecount, if expected == 14 { 2 } else { 0 });
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(store.read_archive_retention_usage().unwrap(), before);
        assert_eq!(
            store
                .lookup_read("query", "published")
                .unwrap()
                .unwrap()
                .container(),
            original.container()
        );
        store.integrity_check().unwrap();
    }
}
