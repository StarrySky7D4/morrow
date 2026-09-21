#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    read_archive::{
        Budget, Finish, MAX_PREPARATION_BYTES, MAX_PREPARATIONS, Plan, PreparationBudget,
        PreparationUsage,
    },
    store::Store,
};
fn plan(id: &str) -> Plan {
    Plan {
        operation_id: id.into(),
        subject: "reader".into(),
        request_type: "test.query".into(),
        request: vec![13; 4096],
        response_type: "test.result".into(),
        budget: Budget::default(),
    }
}
fn setup() -> (tempfile::TempDir, std::path::PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let store = Store::open(&path, Default::default()).unwrap();
    (dir, path, store)
}
fn cost(store: &Store, id: &str) -> u64 {
    let m = store.read_archive_manifest("reader", id).unwrap().unwrap();
    m.status().logical_bytes + m.raw().len() as u64 + m.container().len() as u64
}
fn publish(store: &mut Store, id: &str) {
    let s = store.lookup_read_archive("reader", id).unwrap().unwrap();
    store
        .finish_read_archive_local_authorized(
            "reader",
            id,
            &Finish {
                response: b"ready".to_vec(),
                part_count: s.count,
                logical_bytes: s.logical_bytes,
                chain_sha256: s.chain_sha256,
                metadata_type: "test.eof".into(),
                metadata: vec![],
            },
            &[],
            || Ok(()),
        )
        .unwrap();
}
#[test]
fn default_hard_count_is_shared_across_subjects_reopen_and_store_handles() {
    let (_dir, path, mut store) = setup();
    for i in 0..MAX_PREPARATIONS {
        let mut p = plan(&format!("query-{i}"));
        p.subject = format!("reader-{i}");
        store.begin_read_archive(&p).unwrap();
    }
    assert_eq!(
        store.read_archive_preparation_usage().unwrap().archives,
        MAX_PREPARATIONS
    );
    let mut other = Store::open_existing(&path, Default::default()).unwrap();
    assert!(matches!(
        other.begin_read_archive(&plan("over-count")),
        Err(Error::Limit)
    ));
    let mut existing = plan("query-0");
    existing.subject = "reader-0".into();
    // A lower call budget cannot charge a no-growth retry a second time.
    other
        .begin_read_archive_with_admission_budget(
            &existing,
            PreparationBudget {
                max_archives: 1,
                max_bytes: 1,
            },
        )
        .unwrap();
    assert!(other.lookup_read("reader", "over-count").unwrap().is_none());
    store.abort_read_archive("reader-0", "query-0").unwrap();
    other.begin_read_archive(&plan("replacement")).unwrap();
    drop(store);
    drop(other);
    let reopened = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        reopened.read_archive_preparation_usage().unwrap().archives,
        MAX_PREPARATIONS
    );
}
#[test]
fn manifest_bytes_are_charged_and_failed_admission_leaves_no_preparation() {
    let (_dir, _path, mut store) = setup();
    let p = plan("operation");
    store.begin_read_archive(&p).unwrap();
    let expected = cost(&store, &p.operation_id);
    assert_eq!(
        store.read_archive_preparation_usage().unwrap(),
        PreparationUsage {
            archives: 1,
            logical_bytes: expected
        }
    );
    assert!(expected > p.request.len() as u64);
    store.abort_read_archive("reader", "operation").unwrap();
    let budget = PreparationBudget {
        max_archives: 1,
        max_bytes: expected - 1,
    };
    assert!(matches!(
        store.begin_read_archive_with_admission_budget(&p, budget),
        Err(Error::Limit)
    ));
    assert_eq!(
        store.read_archive_preparation_usage().unwrap(),
        PreparationUsage::default()
    );
    store
        .begin_read_archive_with_admission_budget(
            &p,
            PreparationBudget {
                max_bytes: expected,
                ..budget
            },
        )
        .unwrap();
}
#[test]
fn append_charges_original_plus_container_and_manifest_delta_atomically() {
    let (_dir, path, mut store) = setup();
    let p = plan("operation");
    let data = vec![0; 1024 * 1024];
    store.begin_read_archive(&p).unwrap();
    let initial = cost(&store, "operation");
    store
        .append_read_archive("reader", "operation", 0, "test.bytes", &data)
        .unwrap();
    let expected = cost(&store, "operation");
    assert!(expected > initial + data.len() as u64);
    store.abort_read_archive("reader", "operation").unwrap();
    store.begin_read_archive(&p).unwrap();
    let budget = PreparationBudget {
        max_archives: 2,
        max_bytes: expected - 1,
    };
    assert!(matches!(
        store.append_read_archive_with_admission_budget(
            "reader",
            "operation",
            0,
            "test.bytes",
            &data,
            budget
        ),
        Err(Error::Limit)
    ));
    assert_eq!(
        store
            .lookup_read_archive("reader", "operation")
            .unwrap()
            .unwrap()
            .count,
        0
    );
    assert_eq!(
        store
            .read_archive_preparation_usage()
            .unwrap()
            .logical_bytes,
        initial
    );
    let mut other = Store::open_existing(&path, Default::default()).unwrap();
    other
        .append_read_archive_with_admission_budget(
            "reader",
            "operation",
            0,
            "test.bytes",
            &data,
            PreparationBudget {
                max_bytes: expected,
                ..budget
            },
        )
        .unwrap();
    let before = store.read_archive_preparation_usage().unwrap();
    store
        .append_read_archive_with_admission_budget(
            "reader",
            "operation",
            0,
            "test.bytes",
            &data,
            PreparationBudget {
                max_archives: 1,
                max_bytes: 1,
            },
        )
        .unwrap();
    assert_eq!(store.read_archive_preparation_usage().unwrap(), before);
    assert_eq!(before.logical_bytes, expected);
}
#[test]
fn publishing_and_abort_release_only_unpublished_accounting() {
    let (_dir, path, mut store) = setup();
    store.begin_read_archive(&plan("published")).unwrap();
    store
        .append_read_archive("reader", "published", 0, "test.bytes", b"original")
        .unwrap();
    publish(&mut store, "published");
    assert_eq!(
        store.read_archive_preparation_usage().unwrap(),
        PreparationUsage::default()
    );
    let original = store
        .read_archive_part("reader", "published", 0)
        .unwrap()
        .unwrap()
        .container()
        .to_vec();
    store.begin_read_archive(&plan("pending")).unwrap();
    store.abort_read_archive("reader", "pending").unwrap();
    assert_eq!(
        store.read_archive_preparation_usage().unwrap(),
        PreparationUsage::default()
    );
    assert!(store.abort_read_archive("reader", "published").is_err());
    drop(store);
    let reopened = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        reopened
            .read_archive_part("reader", "published", 0)
            .unwrap()
            .unwrap()
            .container(),
        original
    );
}
#[test]
fn stricter_call_budget_cannot_exceed_hard_limits_or_mutate_on_invalid_policy() {
    let (_dir, _path, mut store) = setup();
    for budget in [
        PreparationBudget {
            max_archives: 0,
            max_bytes: 100,
        },
        PreparationBudget {
            max_archives: 1,
            max_bytes: 0,
        },
        PreparationBudget {
            max_archives: MAX_PREPARATIONS + 1,
            max_bytes: 100,
        },
        PreparationBudget {
            max_archives: 1,
            max_bytes: MAX_PREPARATION_BYTES + 1,
        },
    ] {
        assert!(matches!(
            store.begin_read_archive_with_admission_budget(&plan("invalid"), budget),
            Err(Error::Limit)
        ));
    }
    assert_eq!(
        store.read_archive_preparation_usage().unwrap(),
        PreparationUsage::default()
    );
}
#[test]
fn concurrent_store_admissions_cannot_both_consume_the_last_slot() {
    let (_dir, path, store) = setup();
    drop(store);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    // Store opening can legitimately return StorageBusy. Open handles sequentially
    // so this test races only the admission transactions that it intends to verify.
    let stores = [
        Store::open_existing(&path, Default::default()).unwrap(),
        Store::open_existing(&path, Default::default()).unwrap(),
    ];
    let handles: Vec<_> = stores
        .into_iter()
        .enumerate()
        .map(|(i, mut s)| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let result = s.begin_read_archive_with_admission_budget(
                    &plan(&format!("race-{i}")),
                    PreparationBudget {
                        max_archives: 1,
                        max_bytes: 1024 * 1024,
                    },
                );
                match result {
                    Ok(_) => true,
                    Err(Error::Limit | Error::StorageBusy) => false,
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
    assert_eq!(store.read_archive_preparation_usage().unwrap().archives, 1);
    assert!(store.lookup_read("reader", "race-0").unwrap().is_none());
    assert!(store.lookup_read("reader", "race-1").unwrap().is_none());
}
#[test]
fn pending_manifest_corruption_is_not_accepted_as_free_capacity() {
    let (_dir, path, mut store) = setup();
    store.begin_read_archive(&plan("damaged")).unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE read_archives SET payload=x'00' WHERE operation_id='damaged'",
            [],
        )
        .unwrap();
    assert!(store.begin_read_archive(&plan("next")).is_err());
    assert!(store.read_archive_preparation_usage().is_err());
}
#[test]
fn pending_scan_uses_rebuildable_sqlite_index_without_schema_or_signature_change() {
    let (_dir, path, store) = setup();
    drop(store);
    let sql = rusqlite::Connection::open(&path).unwrap();
    let plan:String=sql.query_row("EXPLAIN QUERY PLAN SELECT operation_id,subject,payload FROM read_archives WHERE published=0 ORDER BY operation_id LIMIT 33",[],|r|r.get(3)).unwrap();
    assert!(
        plan.contains("SEARCH") && plan.contains("read_archive_preparations"),
        "{plan}"
    );
    sql.execute_batch("DROP INDEX read_archive_preparations")
        .unwrap();
    drop(sql);
    let _store = Store::open_existing(&path, Default::default()).unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        sql.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        morrow_core::store::SCHEMA_VERSION as u32
    );
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='read_archive_preparations'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn two_appends_cannot_both_consume_the_last_byte_budget() {
    let (_dir, path, mut s) = setup();
    let data = vec![7; 256 * 1024];
    s.begin_read_archive(&plan("left")).unwrap();
    s.begin_read_archive(&plan("right")).unwrap();
    let before = s.read_archive_preparation_usage().unwrap();
    s.append_read_archive("reader", "left", 0, "test.bytes", &data)
        .unwrap();
    let left_delta =
        s.read_archive_preparation_usage().unwrap().logical_bytes - before.logical_bytes;
    s.append_read_archive("reader", "right", 0, "test.bytes", &data)
        .unwrap();
    let both = s.read_archive_preparation_usage().unwrap();
    let right_delta = both.logical_bytes - before.logical_bytes - left_delta;
    s.abort_read_archive("reader", "left").unwrap();
    s.abort_read_archive("reader", "right").unwrap();
    s.begin_read_archive(&plan("left")).unwrap();
    s.begin_read_archive(&plan("right")).unwrap();
    assert_eq!(s.read_archive_preparation_usage().unwrap(), before);
    let budget = PreparationBudget {
        max_archives: 2,
        max_bytes: before.logical_bytes + left_delta + right_delta - 1,
    };
    let other = Store::open_existing(&path, Default::default()).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = [("left", s), ("right", other)]
        .into_iter()
        .map(|(id, mut store)| {
            let barrier = barrier.clone();
            let data = data.clone();
            std::thread::spawn(move || {
                barrier.wait();
                match store.append_read_archive_with_admission_budget(
                    "reader",
                    id,
                    0,
                    "test.bytes",
                    &data,
                    budget,
                ) {
                    Ok(_) => true,
                    Err(Error::Limit | Error::StorageBusy) => false,
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
    let reopened = Store::open_existing(&path, Default::default()).unwrap();
    let usage = reopened.read_archive_preparation_usage().unwrap();
    assert!(usage.logical_bytes <= budget.max_bytes);
    assert_eq!(
        reopened
            .lookup_read_archive("reader", "left")
            .unwrap()
            .unwrap()
            .count
            + reopened
                .lookup_read_archive("reader", "right")
                .unwrap()
                .unwrap()
                .count,
        1
    );
}
#[test]
fn legacy_over_count_preparations_are_preserved_and_can_be_explicitly_drained() {
    let (_dir, path, mut store) = setup();
    for i in 0..MAX_PREPARATIONS {
        store
            .begin_read_archive(&plan(&format!("existing-{i}")))
            .unwrap();
    }
    // A valid original empty preparation produced by the old unbounded API can be
    // reproduced from a second library; it has no database-specific identity or event.
    let (_old_dir, _old_path, mut old) = setup();
    old.begin_read_archive(&plan("legacy-extra")).unwrap();
    let legacy = old
        .read_archive_manifest("reader", "legacy-extra")
        .unwrap()
        .unwrap();
    drop(store);
    let sql = rusqlite::Connection::open(&path).unwrap();
    // This is explicitly a legacy DB13 sample; DB14 requires its derived ledger.
    sql.execute_batch(
        "DROP TABLE IF EXISTS tls_identities; DROP TABLE IF EXISTS outbound_authorities; DROP TABLE IF EXISTS service_authority_identity; DROP TABLE IF EXISTS service_authorities; DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; DROP TABLE io_reservations; DROP TABLE io_intents; DROP TABLE read_archive_costs; DROP TABLE read_archive_totals; PRAGMA user_version=13;",
    )
    .unwrap();
    sql.execute("INSERT INTO read_archives(operation_id,subject,published,payload) VALUES('legacy-extra','reader',0,?1)",[legacy.container()]).unwrap();
    drop(sql);
    let mut store = Store::open_existing(&path, Default::default()).unwrap();
    store.integrity_check().unwrap();
    assert!(matches!(
        store.read_archive_preparation_usage(),
        Err(Error::Limit)
    ));
    assert!(matches!(
        store.begin_read_archive(&plan("new")),
        Err(Error::Limit)
    ));
    assert_eq!(
        store
            .read_archive_manifest("reader", "legacy-extra")
            .unwrap()
            .unwrap()
            .container(),
        legacy.container()
    );
    publish(&mut store, "legacy-extra");
    assert_eq!(
        store.read_archive_preparation_usage().unwrap().archives,
        MAX_PREPARATIONS
    );
    store.abort_read_archive("reader", "existing-0").unwrap();
    store.begin_read_archive(&plan("new")).unwrap();
    assert!(
        store
            .lookup_read("reader", "legacy-extra")
            .unwrap()
            .is_some()
    );
}
