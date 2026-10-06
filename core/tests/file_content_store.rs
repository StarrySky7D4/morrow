//! Durable write bytes for prepared file plans. These tests never open or
//! modify a target file; staging and historical reads are not dispatch grants.
#![cfg(not(target_arch = "wasm32"))]

use morrow_core::{
    Error,
    file_content::FileContent,
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    io_evidence::{Kind, Material},
    io_intent::{Command, Record},
    plugin_package::io::{self, IoCapability},
    store::{EventBudget, Store},
};
use sha2::{Digest, Sha256};
use std::path::Path;

const SUBJECT: &str = "plugin.file-content-test";
const OPERATION: &str = "file-content-stage";

fn plan(
    operation: &str,
    disposition: Disposition,
    bytes: &[u8],
) -> (Record, Material, FileContent) {
    let request = RequestRecord::new(MutationRequest {
        operation_id: operation.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: None,
        },
        disposition,
        expected_identity: if disposition == Disposition::Create {
            None
        } else {
            Some([4; 32])
        },
        content_length: if disposition == Disposition::Delete {
            0
        } else {
            bytes.len() as u64
        },
        content_sha256: if disposition == Disposition::Delete {
            None
        } else {
            Some(Sha256::digest(bytes).into())
        },
    })
    .unwrap();
    let prepared = Record::prepared(request.command().unwrap()).unwrap();
    let command = prepared.command();
    let material = Material::encode(
        Kind::Request,
        &command.operation_id,
        &command.subject,
        command.request_sha256,
        request.container(),
    )
    .unwrap();
    let content = FileContent::new(operation, SUBJECT, command.request_sha256, bytes).unwrap();
    (prepared, material, content)
}

fn prepare(store: &mut Store, record: &Record, material: &Material) {
    store
        .prepare_file_mutation_local_authorized(record, material, || Ok(()))
        .unwrap();
}

fn row_count(path: &Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("SELECT count(*) FROM file_mutation_content", [], |r| {
            r.get(0)
        })
        .unwrap()
}

fn retained(store: &Store, content: &FileContent) {
    let mut checks = 0;
    let read = store
        .file_mutation_content_local_authorized(SUBJECT, content.operation_id(), || {
            checks += 1;
            Ok(())
        })
        .unwrap()
        .unwrap();
    assert_eq!(checks, 2);
    assert_eq!(read.container(), content.container());
    assert_eq!(read.content(), content.content());
    assert_eq!(
        store.file_mutation_content_usage().unwrap(),
        (
            1,
            content.content().len().max(content.container().len()) as u64
        )
    );
    store.integrity_check().unwrap();
}

#[test]
fn create_and_replace_content_stage_once_then_read_after_reopen_and_snapshot() {
    for disposition in [Disposition::Create, Disposition::Replace] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("content.db");
        let snapshot = dir.path().join("snapshot.db");
        let bytes = vec![0x5a; 64 * 1024];
        let (record, material, content) = plan(OPERATION, disposition, &bytes);
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &record, &material);
        let mut checks = 0;
        store
            .stage_file_mutation_content_local_authorized(&content, || {
                checks += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(checks, 2);
        retained(&store, &content);
        let usage = store.file_mutation_content_usage().unwrap();
        assert_eq!(row_count(&path), 1);
        checks = 0;
        store
            .stage_file_mutation_content_local_authorized(&content, || {
                checks += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(checks, 2, "exact retry still needs fresh authorization");
        assert_eq!(store.file_mutation_content_usage().unwrap(), usage);
        assert_eq!(row_count(&path), 1);
        store.snapshot_to(&snapshot, 16 * 1024 * 1024).unwrap();
        drop(store);
        for source in [&path, &snapshot] {
            let reopened = Store::open_existing(source, EventBudget::default()).unwrap();
            retained(&reopened, &content);
        }
    }
}

#[test]
fn admission_requires_a_matching_prepared_plan_and_delete_cannot_stage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("binding.db");
    let bytes = b"stage only for the exact plan";
    let (record, material, content) = plan(OPERATION, Disposition::Replace, bytes);
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    assert!(
        store
            .stage_file_mutation_content_local_authorized(&content, || Ok(()))
            .is_err()
    );
    assert_eq!(store.file_mutation_content_usage().unwrap(), (0, 0));
    prepare(&mut store, &record, &material);
    for wrong in [
        FileContent::new(OPERATION, SUBJECT, [9; 32], bytes).unwrap(),
        FileContent::new(
            OPERATION,
            SUBJECT,
            record.command().request_sha256,
            b"changed",
        )
        .unwrap(),
        FileContent::new(
            OPERATION,
            "another-subject",
            record.command().request_sha256,
            bytes,
        )
        .unwrap(),
        FileContent::new(
            "another-operation",
            SUBJECT,
            record.command().request_sha256,
            bytes,
        )
        .unwrap(),
    ] {
        assert!(
            store
                .stage_file_mutation_content_local_authorized(&wrong, || Ok(()))
                .is_err()
        );
        assert_eq!(store.file_mutation_content_usage().unwrap(), (0, 0));
    }
    assert!(
        store
            .file_mutation_content_local_authorized("another-subject", OPERATION, || Ok(()))
            .unwrap()
            .is_none()
    );
    assert_eq!(row_count(&path), 0);

    let (delete, original, empty) = plan("delete-plan", Disposition::Delete, b"");
    prepare(&mut store, &delete, &original);
    assert!(
        store
            .stage_file_mutation_content_local_authorized(&empty, || Ok(()))
            .is_err()
    );
    assert_eq!(store.file_mutation_content_usage().unwrap(), (0, 0));
    store.integrity_check().unwrap();
}

#[test]
fn cancellation_retains_auditable_bytes_but_closes_staging() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cancelled.db");
    let (record, material, content) = plan(OPERATION, Disposition::Replace, b"retained bytes");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, &record, &material);
    store
        .stage_file_mutation_content_local_authorized(&content, || Ok(()))
        .unwrap();
    store
        .append_io_intent_local_authorized(
            &record.propose_cancel_before_dispatch().unwrap(),
            || Ok(()),
        )
        .unwrap();
    retained(&store, &content);
    assert!(
        store
            .stage_file_mutation_content_local_authorized(&content, || Ok(()))
            .is_err()
    );
    drop(store);
    let reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
    retained(&reopened, &content);
}

#[test]
fn each_stage_or_read_authorization_failure_withholds_content_and_rolls_back() {
    let (record, material, content) = plan(OPERATION, Disposition::Create, b"authorized bytes");
    for denied in 1..=2 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("denied.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &record, &material);
        let mut checks = 0;
        assert!(matches!(
            store.stage_file_mutation_content_local_authorized(&content, || {
                checks += 1;
                if checks == denied {
                    Err(Error::Invalid("test content denial"))
                } else {
                    Ok(())
                }
            }),
            Err(Error::Invalid("test content denial"))
        ));
        assert_eq!(checks, denied);
        assert_eq!(row_count(&path), 0);
        assert_eq!(store.file_mutation_content_usage().unwrap(), (0, 0));
        store.integrity_check().unwrap();
        store
            .stage_file_mutation_content_local_authorized(&content, || Ok(()))
            .unwrap();
        checks = 0;
        assert!(matches!(
            store.file_mutation_content_local_authorized(SUBJECT, OPERATION, || {
                checks += 1;
                if checks == denied {
                    Err(Error::Invalid("test read denial"))
                } else {
                    Ok(())
                }
            }),
            Err(Error::Invalid("test read denial"))
        ));
        assert_eq!(checks, denied);
        retained(&store, &content);
    }
}

#[test]
fn low_budget_cannot_stage_and_existing_content_remains_readable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("small-budget.db");
    let bytes = vec![0x61; 64 * 1024];
    let (record, material, content) = plan(OPERATION, Disposition::Replace, &bytes);
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, &record, &material);
    drop(store);
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 1024,
            max_bytes: 1024,
        },
    )
    .unwrap();
    assert!(matches!(
        store.stage_file_mutation_content_local_authorized(&content, || Ok(())),
        Err(Error::EventCapacity)
    ));
    assert_eq!(store.file_mutation_content_usage().unwrap(), (0, 0));
    drop(store);
    let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
    store
        .stage_file_mutation_content_local_authorized(&content, || Ok(()))
        .unwrap();
    retained(&store, &content);
}

#[test]
fn staged_raw_bytes_are_counted_against_a_new_http_followup_reservation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared-budget.db");
    let bytes = vec![0x62; 64 * 1024];
    let (record, material, content) = plan(OPERATION, Disposition::Replace, &bytes);
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, &record, &material);
    store
        .stage_file_mutation_content_local_authorized(&content, || Ok(()))
        .unwrap();
    assert!(content.container().len() < content.content().len());
    let http = Record::prepared(Command {
        operation_id: "http-followup-after-file".into(),
        subject: SUBJECT.into(),
        package_sha256: [10; 32],
        capability: IoCapability::HttpRequest,
        protocol_sha256: io::schema_digest(),
        request_sha256: [11; 32],
        approval_sha256: [12; 32],
        target_sha256: [13; 32],
        request_bytes: 27,
        response_limit: 4096,
    })
    .unwrap();
    store
        .append_io_intent_local_authorized(&http, || Ok(()))
        .unwrap();
    let accounted = store.pending_usage().unwrap().1
        + store.io_intent_reservation_usage().unwrap().1
        + store.io_material_reservation_usage().unwrap().1
        + material.container().len() as u64
        + store.file_mutation_content_usage().unwrap().1;
    let usage = store.file_mutation_content_usage().unwrap();
    drop(store);
    let mut store = Store::open_existing(
        &path,
        EventBudget {
            max_count: 1024,
            max_bytes: accounted,
        },
    )
    .unwrap();
    retained(&store, &content);
    let mut authorized = false;
    assert!(matches!(
        store.reserve_io_intent_followup(http.command(), || {
            authorized = true;
            Ok(())
        }),
        Err(Error::EventCapacity)
    ));
    assert!(!authorized);
    assert_eq!(store.io_intent_reservation_usage().unwrap().0, 1);
    assert_eq!(store.file_mutation_content_usage().unwrap(), usage);
    let mut checks = 0;
    store
        .stage_file_mutation_content_local_authorized(&content, || {
            checks += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(checks, 2);
    retained(&store, &content);
}

#[test]
fn bad_mirror_original_or_orphan_blocks_reopen() {
    for damage in ["mirror", "original", "orphan"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("damaged.db");
        let (record, material, content) = plan(OPERATION, Disposition::Replace, b"original bytes");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &record, &material);
        store
            .stage_file_mutation_content_local_authorized(&content, || Ok(()))
            .unwrap();
        drop(store);
        let connection = rusqlite::Connection::open(&path).unwrap();
        match damage {
            "mirror" => {
                connection
                    .execute(
                        "UPDATE file_mutation_content SET content_sha256=zeroblob(32) WHERE operation_id=?1",
                        [OPERATION],
                    )
                    .unwrap();
            }
            "original" => {
                connection
                    .execute(
                        "UPDATE file_mutation_content SET container=X'00' WHERE operation_id=?1",
                        [OPERATION],
                    )
                    .unwrap();
            }
            _ => {
                let orphan = FileContent::new("orphan", SUBJECT, [15; 32], b"orphan").unwrap();
                connection
                    .execute(
                        "INSERT INTO file_mutation_content(operation_id,subject,request_sha256,content_sha256,content_length,container) VALUES(?1,?2,?3,?4,?5,?6)",
                        rusqlite::params![
                            orphan.operation_id(),
                            orphan.subject(),
                            orphan.request_sha256().as_slice(),
                            orphan.content_sha256().as_slice(),
                            orphan.content().len() as i64,
                            orphan.container()
                        ],
                    )
                    .unwrap();
            }
        }
        drop(connection);
        assert!(Store::open_existing(&path, EventBudget::default()).is_err());
    }
}

#[test]
fn honest_v21_without_the_new_table_migrates_but_forged_downgrade_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("version21.db");
    let (record, material, _) = plan(OPERATION, Disposition::Replace, b"future content");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    prepare(&mut store, &record, &material);
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA user_version=21;").unwrap();
    drop(connection);
    assert!(Store::open_existing(&path, EventBudget::default()).is_err());
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE IF EXISTS agent_ledger; DROP TABLE IF EXISTS channel_ack_receipts; DROP TABLE IF EXISTS channel_checkpoints; DROP TABLE IF EXISTS file_content_receipts; DROP TABLE file_mutation_content;")
        .unwrap();
    drop(connection);
    let reopened = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert_eq!(reopened.file_mutation_content_usage().unwrap(), (0, 0));
    assert_eq!(
        reopened
            .lookup_io_intent(SUBJECT, OPERATION)
            .unwrap()
            .unwrap()
            .container(),
        record.container()
    );
    assert_eq!(
        rusqlite::Connection::open(&path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        morrow_core::store::SCHEMA_VERSION
    );
    reopened.integrity_check().unwrap();
}

#[test]
fn oversized_sql_metadata_and_containers_are_rejected_by_live_read_and_open() {
    for column in ["subject", "operation_id", "container"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oversized.db");
        let (record, material, content) = plan(OPERATION, Disposition::Replace, b"original");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        prepare(&mut store, &record, &material);
        store
            .stage_file_mutation_content_local_authorized(&content, || Ok(()))
            .unwrap();
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("PRAGMA ignore_check_constraints=ON;")
            .unwrap();
        match column {
            "container" => {
                sql.execute(
                    "UPDATE file_mutation_content SET container=zeroblob(?1)",
                    [morrow_core::file_content::MAX_CONTAINER_BYTES as i64 + 1],
                )
                .unwrap();
            }
            "subject" => {
                sql.execute(
                    "UPDATE file_mutation_content SET subject=?1",
                    ["a".repeat(1024 * 1024)],
                )
                .unwrap();
            }
            _ => {
                sql.execute(
                    "UPDATE file_mutation_content SET operation_id=?1",
                    ["a".repeat(1024 * 1024)],
                )
                .unwrap();
            }
        }
        drop(sql);
        if column != "operation_id" {
            assert!(
                store
                    .file_mutation_content_local_authorized(SUBJECT, OPERATION, || Ok(()))
                    .is_err()
            );
        }
        assert!(store.integrity_check().is_err());
        drop(store);
        assert!(
            Store::open_existing(&path, EventBudget::default()).is_err(),
            "{column}"
        );
    }
}
