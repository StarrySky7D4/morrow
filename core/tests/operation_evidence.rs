#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::CardRecord,
    content_change::ContentChange,
    dispatch::{Connection, HostRuntime},
    lifecycle::GrantKind,
    plugin_package::{Package, proto::TransformHandler},
    store::Store,
    task::{Invocation, Transform},
    task_evidence::{
        self, Evidence,
        proto::{ExecutionBudget, TaskEvidence},
    },
    transaction::{self, Lookup},
};
use std::path::{Path, PathBuf};
// Synthetic observations exercise persistence only. These bytes are not evidence of actual guest execution.
fn evidence(label: &str, module_bytes: usize) -> Evidence {
    let mut module = b"\0asm\x01\0\0\0".to_vec();
    let mut random = 0x81234567u32;
    for _ in module.len()..module_bytes {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        module.push(random as u8);
    }
    let p = Package::build(
        Package::manifest_for_transform(
            "test.evidence",
            "1.0.0",
            &module,
            vec![TransformHandler {
                handler: "convert".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            }],
        ),
        &module,
    )
    .unwrap();
    let input = Invocation::new_transform(
        label,
        Transform {
            handler: "convert".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: b"synthetic".to_vec(),
        },
    )
    .unwrap();
    task_evidence::encode(TaskEvidence {
        schema_version: 1,
        package_archive: p.archive().to_vec(),
        invocation: input.bytes().to_vec(),
        budget: Some(ExecutionBudget {
            fuel: 100000,
            memory_bytes: 65536,
            host_calls: 4,
        }),
        backend: task_evidence::BACKEND.into(),
        completion: input.output_completion(b"synthetic output").unwrap(),
        fault: 0,
        exit_code: Some(0),
        observed_host_calls: 0,
        fuel_remaining: 90000,
        batch: None,
    })
    .unwrap()
}
fn card() -> CardRecord {
    CardRecord::new("card", "test.card", 1, "original", b"original".to_vec()).unwrap()
}
fn change() -> ContentChange {
    ContentChange {
        operation_id: "edit".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "changed".into(),
        body: b"changed".to_vec(),
        preview_text: "preview".into(),
        attachments: None,
    }
}
fn host(path: &Path) -> (HostRuntime, Connection) {
    let mut host =
        HostRuntime::new(Store::open_existing(path, Default::default()).unwrap()).unwrap();
    let mut c = host.connect().unwrap();
    for kind in [GrantKind::CreateContent, GrantKind::EditContent] {
        host.grant(&mut c, kind, "card", 100, 0).unwrap();
    }
    (host, c)
}
fn setup(seed: bool) -> (tempfile::TempDir, PathBuf, HostRuntime, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, Default::default()).unwrap();
    if seed {
        store.create_local("seed", &card()).unwrap();
    }
    drop(store);
    let (host, c) = host(&path);
    (dir, path, host, c)
}
fn ids(evidence: &[Evidence]) -> Vec<[u8; 32]> {
    evidence.iter().map(Evidence::digest).collect()
}
fn counts(path: &Path) -> (i64, i64) {
    let sql = rusqlite::Connection::open(path).unwrap();
    (
        sql.query_row("SELECT count(*) FROM task_evidence", [], |r| r.get(0))
            .unwrap(),
        sql.query_row("SELECT count(*) FROM operation_evidence", [], |r| r.get(0))
            .unwrap(),
    )
}
#[test]
fn create_and_edit_keep_exact_ordered_evidence_in_durable_commit() {
    let (_dir, path, mut h, c) = setup(false);
    let a = evidence("a", 8);
    let b = evidence("b", 8);
    let receipt = h
        .create_content_with_evidence(
            &c,
            "create",
            &card(),
            &[a.clone(), b.clone(), a.clone()],
            || 1,
        )
        .unwrap();
    assert_eq!(receipt.revision, 1);
    assert_eq!(
        ids(&h
            .store_local()
            .operation_evidence("card", "create")
            .unwrap()),
        ids(&[a.clone(), b.clone(), a.clone()])
    );
    let event = h.store_local().pending(0, 10).unwrap().remove(0).1;
    let (commit, decoded) = transaction::decode_commit(&event).unwrap();
    assert_eq!(decoded, receipt);
    assert_eq!(commit.schema_version, 2);
    assert_eq!(
        commit.task_evidence_sha256,
        vec![
            a.digest().to_vec(),
            b.digest().to_vec(),
            a.digest().to_vec()
        ]
    );
    h.edit_content_with_evidence(&c, &change(), std::slice::from_ref(&b), || 2)
        .unwrap();
    h.store_local().integrity_check().unwrap();
    drop(h);
    assert_eq!(counts(&path), (2, 4));
    let s = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(s.card("card").unwrap().unwrap().summary().revision, 2);
    let loaded = s.operation_evidence("card", "create").unwrap();
    assert_eq!(loaded[0].container(), a.container());
    assert_eq!(loaded[1].raw(), b.raw());
    assert_eq!(
        ids(&s.operation_evidence("card", "edit").unwrap()),
        ids(&[b])
    );
}
#[test]
fn idempotency_requires_same_command_and_same_ordered_refs() {
    let (_dir, path, mut h, c) = setup(true);
    let a = evidence("a", 8);
    let b = evidence("b", 8);
    let selected = [a.clone(), b.clone()];
    let receipt = h
        .edit_content_with_evidence(&c, &change(), &selected, || 1)
        .unwrap();
    assert_eq!(
        h.edit_content_with_evidence(&c, &change(), &selected, || 2)
            .unwrap(),
        receipt
    );
    for different in [
        vec![],
        vec![a.clone()],
        vec![b.clone(), a.clone()],
        vec![a.clone(), a.clone()],
    ] {
        assert_eq!(
            h.edit_content_with_evidence(&c, &change(), &different, || 2),
            Err(Error::OperationConflict)
        );
    }
    let mut changed = change();
    changed.body.push(1);
    assert_eq!(
        h.edit_content_with_evidence(&c, &changed, &selected, || 2),
        Err(Error::OperationConflict)
    );
    assert_eq!(h.store_local().pending(0, 10).unwrap().len(), 2);
    drop(h);
    assert_eq!(counts(&path), (2, 2));
}
#[test]
fn ordinary_operations_remain_v1_and_cross_card_or_missing_queries_fail() {
    let (_dir, _path, mut h, c) = setup(false);
    h.create_content(&c, "create", &card(), || 1).unwrap();
    let event = h.store_local().pending(0, 10).unwrap().remove(0).1;
    let (commit, _) = transaction::decode_commit(&event).unwrap();
    assert_eq!(commit.schema_version, 1);
    assert!(commit.task_evidence_sha256.is_empty());
    assert!(
        h.store_local()
            .operation_evidence("card", "create")
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        h.store_local().operation_evidence("other", "create"),
        Err(Error::NotFound)
    ));
    assert!(matches!(
        h.store_local().operation_evidence("card", "missing"),
        Err(Error::NotFound)
    ));
}
#[test]
fn late_revocation_or_expiry_rolls_back_card_receipt_event_and_all_evidence() {
    for edit in [false, true] {
        for revoke in [false, true] {
            let (_dir, path, mut h, c) = setup(edit);
            let a = evidence("late", 8);
            let signal = h.revocation(&c).unwrap();
            let mut calls = 0;
            let clock = || {
                calls += 1;
                if calls == 2 && revoke {
                    signal.revoke();
                }
                if calls == 2 && !revoke { 100 } else { 1 }
            };
            let result = if edit {
                h.edit_content_with_evidence(&c, &change(), &[a], clock)
            } else {
                h.create_content_with_evidence(&c, "create", &card(), &[a], clock)
            };
            assert!(result.is_err());
            assert_eq!(calls, 2);
            assert_eq!(
                h.store_local()
                    .card("card")
                    .unwrap()
                    .map(|x| x.summary().revision),
                if edit { Some(1) } else { None }
            );
            assert!(matches!(
                h.store_local()
                    .lookup(if edit { "edit" } else { "create" })
                    .unwrap(),
                Lookup::Absent
            ));
            assert_eq!(
                h.store_local().pending(0, 10).unwrap().len(),
                usize::from(edit)
            );
            h.store_local().integrity_check().unwrap();
            drop(h);
            assert_eq!(counts(&path), (0, 0));
        }
    }
}
#[test]
fn retry_after_revocation_cannot_undo_already_committed_evidence() {
    let (_dir, path, mut h, c) = setup(true);
    let a = evidence("committed", 8);
    let receipt = h
        .edit_content_with_evidence(&c, &change(), std::slice::from_ref(&a), || 1)
        .unwrap();
    let signal = h.revocation(&c).unwrap();
    signal.revoke();
    assert!(
        h.edit_content_with_evidence(&c, &change(), std::slice::from_ref(&a), || 2)
            .is_err()
    );
    assert_eq!(
        h.store_local().lookup("edit").unwrap(),
        Lookup::Committed(receipt)
    );
    assert_eq!(
        ids(&h.store_local().operation_evidence("card", "edit").unwrap()),
        ids(&[a])
    );
    drop(h);
    assert_eq!(counts(&path), (1, 1));
}
#[test]
fn corrupted_payload_missing_payload_or_changed_association_fail_integrity() {
    for mutation in [
        "UPDATE task_evidence SET payload=x'00'",
        "DELETE FROM task_evidence",
        "DELETE FROM operation_evidence WHERE ordinal=0",
        "UPDATE operation_evidence SET digest=(SELECT digest FROM task_evidence WHERE digest!=operation_evidence.digest LIMIT 1) WHERE ordinal=0",
    ] {
        let (_dir, path, mut h, c) = setup(false);
        h.create_content_with_evidence(
            &c,
            "create",
            &card(),
            &[evidence("a", 8), evidence("b", 8)],
            || 1,
        )
        .unwrap();
        drop(h);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        sql.execute_batch(mutation).unwrap();
        drop(sql);
        assert!(
            Store::open_existing(&path, Default::default()).is_err(),
            "accepted {mutation}"
        );
    }
}
#[test]
fn count_quota_rejects_seventeenth_ref_without_partial_rows() {
    let (_dir, path, mut h, c) = setup(false);
    let a = evidence("quota", 8);
    assert!(
        h.create_content_with_evidence(&c, "too-many", &card(), &vec![a.clone(); 17], || 1)
            .is_err()
    );
    assert!(h.store_local().card("card").unwrap().is_none());
    assert!(matches!(
        h.store_local().lookup("too-many").unwrap(),
        Lookup::Absent
    ));
    drop(h);
    assert_eq!(counts(&path), (0, 0));
    let (mut h, c) = host(&path);
    h.create_content_with_evidence(&c, "sixteen", &card(), &vec![a; 16], || 1)
        .unwrap();
    drop(h);
    assert_eq!(counts(&path), (1, 16));
}
#[test]
fn byte_quota_counts_raw_and_container_even_for_deduplicated_payloads() {
    let (_dir, path, mut h, c) = setup(false);
    let large = evidence("large", 2 * 1024 * 1024);
    let charged = large.raw().len() + large.container().len();
    let quota = 64 * 1024 * 1024;
    assert!(
        charged * 15 <= quota && charged * 16 > quota,
        "fixture must cross byte quota within count limit: {charged}"
    );
    assert!(
        h.create_content_with_evidence(&c, "over-bytes", &card(), &vec![large.clone(); 16], || 1)
            .is_err()
    );
    assert!(h.store_local().card("card").unwrap().is_none());
    drop(h);
    assert_eq!(counts(&path), (0, 0));
    let (mut h, c) = host(&path);
    h.create_content_with_evidence(&c, "fifteen", &card(), &vec![large; 15], || 1)
        .unwrap();
    drop(h);
    assert_eq!(counts(&path), (1, 15));
}
#[cfg(feature = "fault-injection")]
#[test]
fn evidence_crash_child() {
    let Ok(path) = std::env::var("MORROW_EVIDENCE_TEST_DB") else {
        return;
    };
    let mode = std::env::var("MORROW_EVIDENCE_TEST_MODE").unwrap();
    if mode == "migrate" {
        Store::open_existing(Path::new(&path), Default::default()).unwrap();
        return;
    }
    let (mut h, c) = host(Path::new(&path));
    let a = evidence("crash", 8);
    if mode == "edit" {
        h.edit_content_with_evidence(&c, &change(), &[a], || 1)
            .unwrap();
    } else {
        h.create_content_with_evidence(&c, "create", &card(), &[a], || 1)
            .unwrap();
    }
}
#[cfg(feature = "fault-injection")]
fn child(path: &Path, mode: &str, point: Option<&str>) -> std::process::Output {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "evidence_crash_child", "--nocapture"])
        .env("MORROW_EVIDENCE_TEST_DB", path)
        .env("MORROW_EVIDENCE_TEST_MODE", mode)
        .env_remove("MORROW_TEST_CRASH_AT");
    if let Some(point) = point {
        command.env("MORROW_TEST_CRASH_AT", point);
    }
    command.output().unwrap()
}
#[cfg(feature = "fault-injection")]
#[test]
fn process_crash_keeps_evidence_card_receipt_and_event_all_or_none() {
    for edit in [false, true] {
        for point in ["after-task-evidence", "before-commit", "after-commit"] {
            let (_dir, path, h, _) = setup(edit);
            drop(h);
            let output = child(&path, if edit { "edit" } else { "create" }, Some(point));
            assert_eq!(output.status.code(), Some(86), "{point}: {output:?}");
            let s = Store::open_existing(&path, Default::default()).unwrap();
            let committed = point == "after-commit";
            assert_eq!(
                matches!(
                    s.lookup(if edit { "edit" } else { "create" }).unwrap(),
                    Lookup::Committed(_)
                ),
                committed
            );
            assert_eq!(
                s.card("card").unwrap().map(|c| c.summary().revision),
                if committed {
                    Some(if edit { 2 } else { 1 })
                } else if edit {
                    Some(1)
                } else {
                    None
                }
            );
            assert_eq!(
                s.pending(0, 10).unwrap().len(),
                usize::from(edit) + usize::from(committed)
            );
            if committed {
                assert_eq!(
                    ids(&s
                        .operation_evidence("card", if edit { "edit" } else { "create" })
                        .unwrap()),
                    ids(&[evidence("crash", 8)])
                );
            }
            s.integrity_check().unwrap();
            drop(s);
            assert_eq!(counts(&path), if committed { (1, 1) } else { (0, 0) });
            for _ in 0..2 {
                let output = child(&path, if edit { "edit" } else { "create" }, None);
                assert!(output.status.success(), "{output:?}");
            }
            assert_eq!(counts(&path), (1, 1));
        }
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn v6_migration_crash_is_atomic_and_preserves_existing_content() {
    for point in [
        "evidence-migration-before-commit",
        "evidence-migration-after-commit",
    ] {
        let (_dir, path, h, _) = setup(true);
        drop(h);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch(
            "DROP TABLE operation_read_archives; DROP TABLE read_archive_parts; DROP TABLE read_archives; DROP TABLE task_evidence_chunks; DROP TABLE evidence_chunks; DROP TABLE operation_evidence; DROP TABLE task_evidence; PRAGMA user_version=6;",
        )
        .unwrap();
        drop(sql);
        let output = child(&path, "migrate", Some(point));
        assert_eq!(output.status.code(), Some(86), "{output:?}");
        let sql = rusqlite::Connection::open(&path).unwrap();
        let version: i64 = sql
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        let tables:i64=sql.query_row("SELECT count(*) FROM sqlite_schema WHERE name IN ('operation_evidence','task_evidence')",[],|r|r.get(0)).unwrap();
        assert_eq!(
            (version, tables),
            if point.ends_with("before-commit") {
                (6, 0)
            } else {
                (7, 2)
            }
        );
        drop(sql);
        let s = Store::open_existing(&path, Default::default()).unwrap();
        s.integrity_check().unwrap();
        assert_eq!(s.card("card").unwrap().unwrap().summary().revision, 1);
        assert!(s.operation_evidence("card", "seed").unwrap().is_empty());
        assert_eq!(s.pending(0, 10).unwrap().len(), 1);
        drop(s);
        assert_eq!(counts(&path), (0, 0));
    }
}

#[test]
fn malformed_commit_versions_and_evidence_refs_are_rejected() {
    use prost::Message;
    use sha2::{Digest, Sha256};
    let container = |raw: &[u8]| {
        let packed = lz4_flex::block::compress(raw);
        let mut bytes = b"MORROWT1".to_vec();
        bytes.extend(1u16.to_le_bytes());
        bytes.extend((raw.len() as u32).to_le_bytes());
        bytes.extend((packed.len() as u32).to_le_bytes());
        bytes.extend(Sha256::digest(raw));
        bytes.extend(packed);
        bytes
    };
    let command = transaction::create_command("create", &card()).unwrap();
    let legacy = transaction::encode_commit(command.clone(), &card()).unwrap();
    assert_eq!(
        legacy,
        transaction::encode_commit_with_evidence(command.clone(), &card(), &[]).unwrap()
    );
    let encoded = transaction::encode_commit_with_evidence(command, &card(), &[[7; 32]]).unwrap();
    let (valid, _) = transaction::decode_commit(&encoded).unwrap();
    assert_eq!(
        transaction::decode_commit(&container(&valid.encode_to_vec()))
            .unwrap()
            .0,
        valid
    );
    for case in 0..5 {
        let mut invalid = valid.clone();
        match case {
            0 => invalid.schema_version = 1,
            1 => invalid.task_evidence_sha256.clear(),
            2 => invalid.schema_version = 3,
            3 => invalid.task_evidence_sha256 = vec![vec![7; 31]],
            _ => invalid.task_evidence_sha256 = vec![vec![7; 32]; 17],
        }
        let bytes = container(&invalid.encode_to_vec());
        assert!(
            transaction::decode_commit(&bytes).is_err(),
            "accepted malformed case {case}"
        );
    }
}
#[test]
fn existing_orphan_evidence_cannot_be_silently_adopted_or_snapshotted() {
    let (dir, path, mut h, c) = setup(false);
    let orphan = evidence("orphan", 8);
    // Deliberate corruption through a separate SQLite connection while the trusted
    // runtime is idle; the next transaction must reject it rather than repair it.
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute(
        "INSERT INTO task_evidence(digest,payload) VALUES(?1,?2)",
        rusqlite::params![orphan.digest().as_slice(), orphan.container()],
    )
    .unwrap();
    drop(sql);
    assert_eq!(
        h.create_content_with_evidence(&c, "create", &card(), std::slice::from_ref(&orphan), || 1),
        Err(Error::Integrity)
    );
    assert!(h.store_local().card("card").unwrap().is_none());
    assert!(matches!(
        h.store_local().lookup("create").unwrap(),
        Lookup::Absent
    ));
    let destination = dir.path().join("invalid-snapshot");
    assert!(
        h.store_local()
            .snapshot_to(&destination, 16 * 1024 * 1024)
            .is_err()
    );
    assert!(!destination.exists());
    drop(h);
    assert_eq!(counts(&path), (1, 0));
    assert!(Store::open_existing(&path, Default::default()).is_err());
}

#[test]
fn oversized_operation_payload_is_rejected_by_existing_reader_without_repair() {
    let (_dir, path, h, _) = setup(true);
    let oversized = transaction::MAX_EVENT_BYTES + transaction::MAX_EVENT_BYTES / 255 + 129;
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute(
        "UPDATE operations SET payload=zeroblob(?1) WHERE id='seed'",
        [oversized as i64],
    )
    .unwrap();
    drop(sql);
    assert!(matches!(
        h.store_local().operation_evidence("card", "seed"),
        Err(Error::Limit)
    ));
    assert_eq!(
        h.store_local()
            .card("card")
            .unwrap()
            .unwrap()
            .summary()
            .revision,
        1
    );
    drop(h);
    let sql = rusqlite::Connection::open(&path).unwrap();
    let actual: i64 = sql
        .query_row(
            "SELECT length(payload) FROM operations WHERE id='seed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        actual, oversized as i64,
        "the corrupt original must not be rewritten by reading"
    );
}
