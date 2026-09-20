#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{Package, proto::TransformHandler},
    read_journal::{self, Input},
    records::{Kind, Record},
    store::{EventBudget, Store},
    task::{Invocation, Transform},
    task_evidence::{
        self, Evidence,
        proto::{ExecutionBudget, TaskEvidence},
    },
    transaction::Lookup,
};
use prost::Message;
use sha2::{Digest, Sha256};
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

fn input(op: &str) -> Input {
    Input {
        operation_id: op.into(),
        subject: "workspace.query".into(),
        request_type: "test.query.v1".into(),
        request: b"filter".to_vec(),
        response_type: "test.result.v1".into(),
        response: b"[card]".to_vec(),
    }
}
fn setup() -> (tempfile::TempDir, PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("read.db");
    let store = Store::open(&path, Default::default()).unwrap();
    (dir, path, store)
}
fn count(path: &Path, table: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn empty(path: &Path) {
    for table in [
        "cards",
        "records",
        "operations",
        "outbox",
        "task_evidence",
        "operation_evidence",
        "evidence_chunks",
        "task_evidence_chunks",
    ] {
        assert_eq!(count(path, table), 0, "{table}");
    }
}
fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut bytes = read_journal::MAGIC.to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend(compressed);
    bytes
}
#[test]
fn read_journal_changes_no_card_record_or_revision_and_has_typed_lookup() {
    let (_dir, path, mut store) = setup();
    let card = CardRecord::new("card", "test.card", 1, "Title", b"body".to_vec()).unwrap();
    let workspace = Record::workspace("workspace.query", "Workspace").unwrap();
    store.create_local("create", &card).unwrap();
    store
        .create_record_local("workspace-create", &workspace)
        .unwrap();
    let value = input("query");
    let ev = evidence("one", 8);
    let receipt = store
        .record_read_local_authorized(&value, std::slice::from_ref(&ev), || Ok(()))
        .unwrap();
    let obs = store.lookup_read(&value.subject, "query").unwrap().unwrap();
    assert_eq!(receipt, obs.receipt());
    assert_eq!(obs.data().request, value.request);
    assert_eq!(obs.data().response, value.response);
    assert_eq!(store.card("card").unwrap().unwrap().encode(), card.encode());
    assert_eq!(
        store
            .record_local(Kind::Workspace, "workspace.query")
            .unwrap()
            .unwrap()
            .encode(),
        workspace.encode()
    );
    assert_eq!(count(&path, "cards"), 1);
    assert_eq!(count(&path, "records"), 1);
    assert_eq!(
        store.lookup_for_card("card", "query").unwrap(),
        Lookup::Absent
    );
    assert!(
        store
            .lookup_record_local(Kind::Workspace, "workspace.query", "query")
            .unwrap()
            .is_none()
    );
    for (subject, op) in [
        ("other", "query"),
        ("workspace.query", "missing"),
        ("card", "create"),
        ("workspace.query", "workspace-create"),
    ] {
        assert!(store.lookup_read(subject, op).unwrap().is_none());
        assert!(store.read_evidence(subject, op).unwrap().is_none());
    }
    assert_eq!(
        store.pending(0, 10).unwrap().last().unwrap().1,
        obs.container()
    );
    store.integrity_check().unwrap();
    drop(store);
    let reopened = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        reopened
            .lookup_read(&value.subject, "query")
            .unwrap()
            .unwrap()
            .container(),
        obs.container()
    );
    assert_eq!(
        reopened
            .read_evidence(&value.subject, "query")
            .unwrap()
            .unwrap()[0]
            .container(),
        ev.container()
    );
    // A stored receipt contains host-recorded facts; it has no UI delivery ACK or user-received status.
    assert_eq!(reopened.pending(0, 10).unwrap().len(), 3);
}
#[test]
fn exact_retry_requires_current_guard_and_all_original_fields_and_ordered_evidence() {
    let (_dir, path, mut store) = setup();
    let value = input("query");
    let a = evidence("a", 8);
    let b = evidence("b", 8);
    let originals = [a.clone(), b.clone(), a.clone()];
    let receipt = store
        .record_read_local_authorized(&value, &originals, || Ok(()))
        .unwrap();
    let container = store
        .lookup_read(&value.subject, "query")
        .unwrap()
        .unwrap()
        .container()
        .to_vec();
    assert_eq!(
        store
            .record_read_local_authorized(&value, &originals, || Ok(()))
            .unwrap(),
        receipt
    );
    assert_eq!(
        store.record_read_local_authorized(&value, &originals, || Err(Error::NotFound)),
        Err(Error::NotFound)
    );
    for field in 0..5 {
        let mut changed = value.clone();
        match field {
            0 => changed.subject.push('x'),
            1 => changed.request_type.push('x'),
            2 => changed.request.push(0),
            3 => changed.response_type.push('x'),
            _ => changed.response.push(0),
        }
        assert_eq!(
            store.record_read_local_authorized(&changed, &originals, || Ok(())),
            Err(Error::OperationConflict)
        );
    }
    for changed in [
        vec![],
        vec![a.clone()],
        vec![b.clone(), a.clone(), a.clone()],
        vec![a.clone(), b.clone(), b.clone()],
    ] {
        assert_eq!(
            store.record_read_local_authorized(&value, &changed, || Ok(())),
            Err(Error::OperationConflict)
        );
    }
    assert_eq!(
        store
            .lookup_read(&value.subject, "query")
            .unwrap()
            .unwrap()
            .container(),
        container
    );
    assert_eq!(count(&path, "operations"), 1);
    assert_eq!(count(&path, "outbox"), 1);
    let card = CardRecord::new("card", "test.card", 1, "Title", vec![]).unwrap();
    assert_eq!(
        store.create_local("query", &card),
        Err(Error::OperationConflict)
    );
    assert_eq!(
        store.create_record_local("query", &Record::workspace("w", "W").unwrap()),
        Err(Error::OperationConflict)
    );
    store.create_local("card-operation", &card).unwrap();
    assert_eq!(
        store.record_read_local_authorized(&input("card-operation"), &[], || Ok(())),
        Err(Error::OperationConflict)
    );
    store
        .create_record_local("record-operation", &Record::workspace("w", "W").unwrap())
        .unwrap();
    assert_eq!(
        store.record_read_local_authorized(&input("record-operation"), &[], || Ok(())),
        Err(Error::OperationConflict)
    );
    store.integrity_check().unwrap();
}
#[test]
fn final_guard_failure_rolls_back_journal_evidence_chunks_and_outbox() {
    let (_dir, path, mut store) = setup();
    let e = evidence("large", 128 * 1024);
    assert_eq!(
        store.record_read_local_authorized(&input("query"), std::slice::from_ref(&e), || Err(
            Error::NotFound
        )),
        Err(Error::NotFound)
    );
    empty(&path);
    assert!(
        store
            .lookup_read("workspace.query", "query")
            .unwrap()
            .is_none()
    );
    store
        .record_read_local_authorized(&input("query"), &[e], || Ok(()))
        .unwrap();
    store.integrity_check().unwrap();
}
#[test]
fn actual_commit_unknown_rolls_back_and_retry_uses_same_original_observation() {
    let (_dir, path, mut store) = setup();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TABLE review_parent(id INTEGER PRIMARY KEY); CREATE TABLE review_deferred(id INTEGER PRIMARY KEY,parent INTEGER REFERENCES review_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER review_read_failure AFTER INSERT ON outbox BEGIN INSERT INTO review_deferred(id,parent) VALUES(1,99); END;").unwrap();
    let value = input("query");
    let e = evidence("commit", 128 * 1024);
    assert_eq!(
        store.record_read_local_authorized(&value, std::slice::from_ref(&e), || Ok(())),
        Err(Error::CommitUnknown)
    );
    empty(&path);
    sql.execute_batch(
        "DROP TRIGGER review_read_failure; DROP TABLE review_deferred; DROP TABLE review_parent;",
    )
    .unwrap();
    drop(sql);
    let expected = read_journal::encode(&value, &[e.digest()]).unwrap();
    store
        .record_read_local_authorized(&value, &[e], || Ok(()))
        .unwrap();
    assert_eq!(
        store
            .lookup_read(&value.subject, &value.operation_id)
            .unwrap()
            .unwrap()
            .container(),
        expected.container()
    );
    store.integrity_check().unwrap();
}
#[test]
fn content_and_read_events_share_originals_and_chunks_without_cross_kind_lookup() {
    let (_dir, path, store) = setup();
    let e = evidence("shared", 128 * 1024);
    let mut host = HostRuntime::new(store).unwrap();
    let mut conn = host.connect().unwrap();
    host.grant(&mut conn, GrantKind::CreateContent, "card", 100, 0)
        .unwrap();
    let card = CardRecord::new("card", "test.card", 1, "C", vec![]).unwrap();
    host.create_content_with_evidence(&conn, "create", &card, std::slice::from_ref(&e), || 1)
        .unwrap();
    let chunks = count(&path, "evidence_chunks");
    assert!(chunks > 1);
    host.store_local_mut()
        .record_read_local_authorized(&input("query"), &[e.clone(), e.clone()], || Ok(()))
        .unwrap();
    assert_eq!(count(&path, "task_evidence"), 1);
    assert_eq!(count(&path, "evidence_chunks"), chunks);
    assert_eq!(count(&path, "operation_evidence"), 3);
    assert_eq!(
        host.store_local()
            .operation_evidence("card", "create")
            .unwrap()[0]
            .container(),
        e.container()
    );
    let copies = host
        .store_local()
        .read_evidence("workspace.query", "query")
        .unwrap()
        .unwrap();
    assert_eq!(copies.len(), 2);
    for copy in copies {
        assert_eq!(copy.container(), e.container());
    }
    assert!(
        host.store_local()
            .operation_evidence("workspace.query", "query")
            .is_err()
    );
    host.store_local().integrity_check().unwrap();
}
#[test]
fn corrupted_missing_swapped_and_orphan_evidence_are_rejected_without_repair() {
    for damage in 0..7 {
        let (_dir, path, mut store) = setup();
        let a = evidence("a", 128 * 1024);
        let b = evidence("b", 128 * 1024);
        store
            .record_read_local_authorized(&input("query"), &[a, b], || Ok(()))
            .unwrap();
        let sql = rusqlite::Connection::open(&path).unwrap();
        let statement = match damage {
            0 => {
                "DELETE FROM task_evidence WHERE digest=(SELECT digest FROM task_evidence LIMIT 1)"
            }
            1 => {
                "UPDATE evidence_chunks SET payload=x'00' WHERE digest=(SELECT digest FROM evidence_chunks LIMIT 1)"
            }
            2 => {
                "DELETE FROM evidence_chunks WHERE digest=(SELECT digest FROM evidence_chunks LIMIT 1)"
            }
            3 => {
                "UPDATE task_evidence SET payload=x'00' WHERE digest=(SELECT digest FROM task_evidence LIMIT 1)"
            }
            4 => {
                "UPDATE operation_evidence SET digest=(SELECT digest FROM operation_evidence WHERE ordinal=1) WHERE ordinal=0"
            }
            5 => "INSERT INTO evidence_chunks(digest,payload) VALUES(zeroblob(32),x'00')",
            _ => "DELETE FROM operation_evidence",
        };
        sql.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        sql.execute_batch(statement).unwrap();
        assert!(store.integrity_check().is_err(), "damage {damage}");
        if damage < 5 {
            assert!(
                store.read_evidence("workspace.query", "query").is_err(),
                "damage {damage}"
            );
        }
        drop(store);
        assert!(
            Store::open_existing(&path, Default::default()).is_err(),
            "damage {damage}"
        );
    }
}
#[test]
fn codec_preserves_unknown_raw_and_rejects_duplicate_or_forged_fields() {
    let original = read_journal::encode(&input("query"), &[[7; 32]]).unwrap();
    let mut raw = original.raw().to_vec();
    raw.extend([0xa0, 0x06, 42]);
    let unknown = pack(&raw);
    let loaded = read_journal::decode(&unknown).unwrap();
    assert_eq!(loaded.raw(), raw);
    assert_eq!(loaded.container(), unknown);
    let mut duplicate = original.raw().to_vec();
    duplicate.extend([8, 1]);
    assert!(read_journal::decode(&pack(&duplicate)).is_err());
    for field in 0..5 {
        let mut data = original.data().clone();
        match field {
            0 => data.request.push(1),
            1 => data.response.push(1),
            2 => data.task_evidence_sha256[0].pop().map(|_| ()).unwrap(),
            3 => data.schema_version = 2,
            _ => data.task_evidence_sha256 = vec![vec![7; 32]; 17],
        };
        assert!(
            read_journal::decode(&pack(&data.encode_to_vec())).is_err(),
            "field {field}"
        );
    }
    let mut bad = original.container().to_vec();
    bad[18] ^= 1;
    assert!(read_journal::decode(&bad).is_err());
    let mut bad = original.container().to_vec();
    bad.extend([0]);
    assert!(read_journal::decode(&bad).is_err());
}
#[test]
fn maximum_values_and_reference_count_are_accepted_but_excess_is_rejected() {
    let mut value = input("maximum");
    value.request = vec![7; read_journal::MAX_VALUE_BYTES];
    value.response = vec![9; read_journal::MAX_VALUE_BYTES];
    let obs = read_journal::encode(&value, &[[3; 32]; 16]).unwrap();
    assert_eq!(
        read_journal::decode(obs.container()).unwrap().raw(),
        obs.raw()
    );
    value.request.push(1);
    assert!(matches!(
        read_journal::encode(&value, &[]),
        Err(Error::Limit)
    ));
    value.request.pop();
    value.response.push(1);
    assert!(matches!(
        read_journal::encode(&value, &[]),
        Err(Error::Limit)
    ));
    value.response.pop();
    assert!(matches!(
        read_journal::encode(&value, &[[3; 32]; 17]),
        Err(Error::Limit)
    ));
    let (_dir, path, mut store) = setup();
    let e = evidence("quota", 8);
    store
        .record_read_local_authorized(&input("sixteen"), &vec![e.clone(); 16], || Ok(()))
        .unwrap();
    assert!(matches!(
        store.record_read_local_authorized(&input("seventeen"), &vec![e; 17], || Ok(())),
        Err(Error::Limit)
    ));
    assert_eq!(count(&path, "operations"), 1);
    store.integrity_check().unwrap();
}
#[test]
fn pending_event_budget_blocks_new_reads_but_exact_retry_remains_available() {
    let (_dir, path, store) = setup();
    drop(store);
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 1,
            max_bytes: 1024 * 1024,
        },
    )
    .unwrap();
    let receipt = store
        .record_read_local_authorized(&input("first"), &[], || Ok(()))
        .unwrap();
    assert!(
        store
            .record_read_local_authorized(&input("second"), &[evidence("a", 128 * 1024)], || Ok(()))
            .is_err()
    );
    assert_eq!(count(&path, "task_evidence"), 0);
    assert_eq!(count(&path, "operations"), 1);
    assert_eq!(
        store
            .record_read_local_authorized(&input("first"), &[], || Ok(()))
            .unwrap(),
        receipt
    );
    assert_eq!(
        store
            .read_evidence("workspace.query", "first")
            .unwrap()
            .unwrap()
            .len(),
        0
    );
}
fn version(path: &Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}
#[test]
fn format_ten_migration_preserves_original_content_evidence_and_signed_segment() {
    use ed25519_dalek::SigningKey;
    use morrow_core::audit::{self, TrustedLog};
    let (_dir, path, store) = setup();
    drop(store);
    let key = SigningKey::from_bytes(&[41; 32]);
    let trust = TrustedLog {
        id: "read-migration".into(),
        key: key.verifying_key(),
    };
    let store = Store::open_audited(&path, Default::default(), false, trust.clone()).unwrap();
    let card = CardRecord::new("card", "test.card", 1, "C", vec![]).unwrap();
    let ev = evidence("historical", 128 * 1024);
    let mut host = HostRuntime::new(store).unwrap();
    let mut conn = host.connect().unwrap();
    host.grant(&mut conn, GrantKind::CreateContent, "card", 100, 0)
        .unwrap();
    host.create_content_with_evidence(&conn, "create", &card, std::slice::from_ref(&ev), || 1)
        .unwrap();
    let store = host.store_local_mut();
    let before = store.pending(0, 10).unwrap();
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &before).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    store.seal_pending(&signed).unwrap();
    drop(host);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE IF EXISTS outbound_authorities; DROP TABLE IF EXISTS service_authority_identity; DROP TABLE IF EXISTS service_authorities; DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; DROP TABLE io_reservations; DROP TABLE io_intents; DROP TABLE read_archive_costs; DROP TABLE read_archive_totals; DROP TABLE read_captures; DROP TABLE operation_read_archives; DROP TABLE read_archive_parts; DROP TABLE read_archives; PRAGMA user_version=10;")
        .unwrap();
    let old = Store::open_read_only_audited(&path, trust.clone()).unwrap();
    assert_eq!(old.sealed_segment(1).unwrap().unwrap(), signed);
    assert_eq!(
        old.operation_evidence("card", "create").unwrap()[0].container(),
        ev.container()
    );
    drop(old);
    assert_eq!(version(&path), 10);
    let migrated = Store::open_audited(&path, Default::default(), false, trust).unwrap();
    assert_eq!(
        migrated.card("card").unwrap().unwrap().encode(),
        card.encode()
    );
    assert_eq!(migrated.sealed_segment(1).unwrap().unwrap(), signed);
    let restored = migrated
        .operation_evidence("card", "create")
        .unwrap()
        .remove(0);
    assert_eq!(restored.raw(), ev.raw());
    assert_eq!(restored.container(), ev.container());
    assert_eq!(restored.digest(), ev.digest());
    migrated.integrity_check().unwrap();
    drop(migrated);
    assert_eq!(version(&path), morrow_core::store::SCHEMA_VERSION);
}
#[test]
fn read_event_cannot_be_smuggled_into_old_format_ten() {
    let (_dir, path, mut store) = setup();
    store
        .record_read_local_authorized(&input("query"), &[evidence("a", 8)], || Ok(()))
        .unwrap();
    drop(store);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE IF EXISTS outbound_authorities; DROP TABLE IF EXISTS service_authority_identity; DROP TABLE IF EXISTS service_authorities; DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; DROP TABLE io_reservations; DROP TABLE io_intents; DROP TABLE read_archive_costs; DROP TABLE read_archive_totals; DROP TABLE read_captures; DROP TABLE operation_read_archives; DROP TABLE read_archive_parts; DROP TABLE read_archives; PRAGMA user_version=10;")
        .unwrap();
    assert!(matches!(
        Store::open_existing(&path, Default::default()),
        Err(Error::UnsupportedVersion)
    ));
    assert_eq!(version(&path), 10);
}
#[cfg(feature = "fault-injection")]
#[test]
fn read_journal_crash_child() {
    let Some(path) = std::env::var_os("MORROW_READ_REVIEW_DB") else {
        return;
    };
    let mut store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
    if std::env::var("MORROW_READ_REVIEW_MODE").as_deref() == Ok("read") {
        store
            .record_read_local_authorized(&input("query"), &[evidence("crash", 128 * 1024)], || {
                Ok(())
            })
            .unwrap();
    }
}
#[cfg(feature = "fault-injection")]
fn child(path: &Path, mode: &str, point: &str) -> std::process::ExitStatus {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "read_journal_crash_child", "--nocapture"])
        .env("MORROW_READ_REVIEW_DB", path)
        .env("MORROW_READ_REVIEW_MODE", mode)
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
            Ok(Some(status)) => return status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe child: {error}")
            }
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("read journal child timed out: {point}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn actual_crashes_publish_either_no_read_or_complete_original_with_exact_retry() {
    for point in [
        "read-after-begin",
        "read-after-operation",
        "read-after-task-evidence",
        "read-after-event",
        "read-before-commit",
        "read-after-commit",
    ] {
        let (_dir, path, store) = setup();
        drop(store);
        assert_eq!(child(&path, "read", point).code(), Some(86), "{point}");
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        store.integrity_check().unwrap();
        let e = evidence("crash", 128 * 1024);
        let expected = read_journal::encode(&input("query"), &[e.digest()]).unwrap();
        if point == "read-after-commit" {
            assert_eq!(
                store
                    .lookup_read("workspace.query", "query")
                    .unwrap()
                    .unwrap()
                    .container(),
                expected.container()
            );
            assert_eq!(
                store
                    .read_evidence("workspace.query", "query")
                    .unwrap()
                    .unwrap()[0]
                    .container(),
                e.container()
            );
        } else {
            empty(&path);
        }
        store
            .record_read_local_authorized(&input("query"), &[e], || Ok(()))
            .unwrap();
        assert_eq!(
            store
                .lookup_read("workspace.query", "query")
                .unwrap()
                .unwrap()
                .container(),
            expected.container()
        );
        assert_eq!(count(&path, "outbox"), 1);
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn migration_crashes_keep_whole_ten_or_eleven_and_original_content() {
    for point in [
        "read-journal-migration-before-commit",
        "read-journal-migration-after-commit",
    ] {
        let (_dir, path, mut store) = setup();
        let card = CardRecord::new("card", "test.card", 1, "C", vec![]).unwrap();
        store.create_local("create", &card).unwrap();
        let before = store.pending(0, 10).unwrap();
        drop(store);
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch("DROP TABLE IF EXISTS outbound_authorities; DROP TABLE IF EXISTS service_authority_identity; DROP TABLE IF EXISTS service_authorities; DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; DROP TABLE io_reservations; DROP TABLE io_intents; DROP TABLE read_archive_costs; DROP TABLE read_archive_totals; DROP TABLE read_captures; DROP TABLE operation_read_archives; DROP TABLE read_archive_parts; DROP TABLE read_archives; PRAGMA user_version=10;")
            .unwrap();
        assert_eq!(child(&path, "migrate", point).code(), Some(86));
        assert_eq!(
            version(&path),
            if point.ends_with("after-commit") {
                11
            } else {
                10
            }
        );
        let reopened = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(reopened.pending(0, 10).unwrap(), before);
        assert_eq!(
            reopened.card("card").unwrap().unwrap().encode(),
            card.encode()
        );
        reopened.integrity_check().unwrap();
        assert_eq!(version(&path), morrow_core::store::SCHEMA_VERSION);
    }
}

#[test]
fn repeated_evidence_is_charged_logically_even_when_physical_chunks_are_shared() {
    let (_dir, path, mut store) = setup();
    let ev = evidence("logical-budget", 2 * 1024 * 1024);
    let unit = ev.raw().len() + ev.container().len();
    assert!(unit * 15 <= 64 * 1024 * 1024 && unit * 16 > 64 * 1024 * 1024);
    let accepted = vec![ev.clone(); 15];
    store
        .record_read_local_authorized(&input("accepted"), &accepted, || Ok(()))
        .unwrap();
    let chunks = count(&path, "evidence_chunks");
    assert_eq!(count(&path, "task_evidence"), 1);
    assert_eq!(
        store.record_read_local_authorized(&input("oversized"), &vec![ev; 16], || Ok(())),
        Err(Error::Limit)
    );
    assert_eq!(count(&path, "evidence_chunks"), chunks);
    assert_eq!(count(&path, "operations"), 1);
    assert!(
        store
            .lookup_read("workspace.query", "oversized")
            .unwrap()
            .is_none()
    );
    store.integrity_check().unwrap();
}
#[test]
fn oversized_sql_payload_is_rejected_before_materializing_the_container() {
    let (_dir, path, mut store) = setup();
    store
        .record_read_local_authorized(&input("query"), &[], || Ok(()))
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute(
        "UPDATE operations SET payload=zeroblob(?1) WHERE id='query'",
        [(read_journal::MAX_CONTAINER_BYTES + 1) as i64],
    )
    .unwrap();
    assert!(matches!(
        store.lookup_read("workspace.query", "query"),
        Err(Error::Limit)
    ));
    assert!(matches!(
        store.read_evidence("workspace.query", "query"),
        Err(Error::Limit)
    ));
    assert!(matches!(
        store.integrity_check(),
        Err(Error::Limit) | Err(Error::Integrity)
    ));
}
