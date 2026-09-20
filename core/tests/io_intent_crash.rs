//! Real SQLite process-exit boundaries only; no HTTP request, backend dispatch,
//! protected-evidence retention or external-effect outcome is exercised here.
#![cfg(all(feature = "fault-injection", not(target_arch = "wasm32")))]
use morrow_core::{
    Error,
    content::CardRecord,
    io_intent::{Command as IntentCommand, MAX_CONTAINER_BYTES, ObservationSource, Record, Recovery},
    plugin_package::io,
    store::Store,
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn prepared() -> Record {
    Record::prepared(IntentCommand {
        operation_id: "crash-command".into(),
        subject: "test.io-local".into(),
        package_sha256: [1; 32],
        capability: io::IoCapability::HttpRequest,
        protocol_sha256: io::schema_digest(),
        request_sha256: [2; 32],
        approval_sha256: [3; 32],
        target_sha256: [4; 32],
        request_bytes: 5,
        response_limit: 1024,
    })
    .unwrap()
}

#[test]
#[ignore = "child harness; parent passes an isolated database and crash point"]
fn io_intent_crash_child() {
    let path = std::env::var_os("MORROW_IO_CRASH_DB").expect("child database");
    let mut store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
    match std::env::var("MORROW_IO_CRASH_MODE").unwrap().as_str() {
        "unknown" => {
            let original = store
                .lookup_io_intent("test.io-local", "crash-command")
                .unwrap()
                .unwrap();
            store
                .reserve_io_intent_followup(original.command(), || Ok(()))
                .unwrap();
            let candidate = original.propose_dispatch_boundary().unwrap();
            store
                .append_io_intent_local_authorized(&candidate, || Ok(()))
                .unwrap();
        }
        "reserve" => {
            let original = store
                .lookup_io_intent("test.io-local", "crash-command")
                .unwrap()
                .unwrap();
            store
                .reserve_io_intent_followup(original.command(), || Ok(()))
                .unwrap();
        }
        "migrate" => {}
        other => panic!("unexpected child mode {other}"),
    }
    panic!("fault injection boundary was not reached");
}

fn crash(path: &Path, mode: &str, point: &str) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "io_intent_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env("MORROW_IO_CRASH_DB", path)
        .env("MORROW_IO_CRASH_MODE", mode)
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
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe {point}: {error}");
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timeout {point}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn rows(path: &Path) -> Vec<(String, i64, String, Vec<u8>, i64)> {
    let sql = rusqlite::Connection::open(path).unwrap();
    let mut statement=sql.prepare("SELECT i.operation_id,i.revision,i.event_id,o.payload,e.sequence FROM io_intents i JOIN operations o ON o.id=i.event_id JOIN operation_events e ON e.id=i.event_id ORDER BY i.operation_id,i.revision").unwrap();
    statement
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
fn count(path: &Path, table: &str) -> i64 {
    assert!(
        [
            "io_intents",
            "io_reservations",
            "operations",
            "operation_events",
            "outbox",
            "cards",
            "records"
        ]
        .contains(&table)
    );
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn unknown_commit_crashes_leave_original_or_complete_revision_and_no_resend() {
    for (point, committed) in [
        ("io-intent-after-operation", false),
        ("io-intent-after-history", false),
        ("io-intent-after-event", false),
        ("io-intent-before-commit", false),
        ("io-intent-after-commit", true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic.db");
        let mut store = Store::open(&path, Default::default()).unwrap();
        let first = prepared();
        store
            .append_io_intent_local_authorized(&first, || Ok(()))
            .unwrap();
        let expected = first.propose_dispatch_boundary().unwrap();
        let original_pending = store.pending(0, 128).unwrap();
        let original_rows = rows(&path);
        drop(store);
        crash(&path, "unknown", point);
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        store.integrity_check().unwrap();
        // The reservation committed in its own transaction before the boundary
        // and is preserved exactly once across the crash in either outcome.
        assert_eq!(
            store.io_intent_reservation_usage().unwrap(),
            (1, MAX_CONTAINER_BYTES as u64),
            "{point}"
        );
        let latest = store
            .lookup_io_intent("test.io-local", "crash-command")
            .unwrap()
            .unwrap();
        let current_rows = rows(&path);
        let current_pending = store.pending(0, 128).unwrap();
        assert_eq!(current_rows[0], original_rows[0], "{point}");
        assert_eq!(current_pending[0], original_pending[0], "{point}");
        let expected_count = if committed { 2 } else { 1 };
        for table in ["io_intents", "operations", "operation_events", "outbox"] {
            assert_eq!(count(&path, table), expected_count, "{point}: {table}");
        }
        assert_eq!(count(&path, "cards"), 0);
        assert_eq!(count(&path, "records"), 0);
        if committed {
            assert_eq!(latest.container(), expected.container());
            assert_eq!(latest.recovery(), Recovery::ReconcileOnly);
            assert_eq!(
                current_rows[1],
                (
                    "crash-command".into(),
                    2,
                    expected.event_id(),
                    expected.container().to_vec(),
                    2
                )
            );
            assert_eq!(current_pending[1], (2, expected.container().to_vec()));
            assert!(latest.propose_dispatch_boundary().is_err());
            assert!(latest.propose_cancel_before_dispatch().is_err());
            // Explicit same-candidate delivery retry returns the original receipt,
            // not a fresh event or a permission to dispatch again.
            let mut guards = 0;
            let retry = store
                .append_io_intent_local_authorized(&expected, || {
                    guards += 1;
                    Ok(())
                })
                .unwrap();
            assert_eq!(guards, 1);
            assert_eq!(retry.container(), expected.container());
        } else {
            assert_eq!(latest.container(), first.container());
            assert_eq!(latest.recovery(), Recovery::AwaitFreshAuthorization);
            assert_eq!(current_rows, original_rows);
            assert_eq!(current_pending, original_pending);
        }
        assert_eq!(rows(&path), current_rows);
        assert_eq!(store.pending(0, 128).unwrap(), current_pending);
        drop(store);
        // Reopening alone must never manufacture another boundary/reconciliation.
        let reopened = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(
            reopened
                .lookup_io_intent("test.io-local", "crash-command")
                .unwrap()
                .unwrap()
                .container(),
            latest.container()
        );
        assert_eq!(rows(&path), current_rows);
        assert_eq!(reopened.pending(0, 128).unwrap(), current_pending);
    }
}

#[test]
fn migration_crashes_keep_complete_old_or_new_schema_and_unchanged_originals() {
    for (point, expected_version) in [
        ("io-intent-migration-before-commit", 14),
        ("io-intent-migration-after-commit", 15),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.db");
        let mut store = Store::open(&path, Default::default()).unwrap();
        let card =
            CardRecord::new("old-card", "test.card", 1, "", b"retained body".to_vec()).unwrap();
        store.create_local("old-create", &card).unwrap();
        let originals = store.pending(0, 128).unwrap();
        drop(store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("DROP TABLE IF EXISTS outbound_authorities; DROP TABLE IF EXISTS service_authority_identity; DROP TABLE IF EXISTS service_authorities; DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; DROP TABLE io_reservations; DROP TABLE io_intents; PRAGMA user_version=14;")
            .unwrap();
        drop(sql);
        crash(&path, "migrate", point);
        let sql = rusqlite::Connection::open(&path).unwrap();
        let actual: i64 = sql
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(actual, expected_version, "{point}");
        let tables: i64 = sql
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='io_intents'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, i64::from(expected_version == 15));
        let reservations: i64 = sql
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='io_reservations'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reservations, i64::from(expected_version >= 16), "{point}");
        let old: Vec<u8> = sql
            .query_row(
                "SELECT payload FROM operations WHERE id='old-create'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old, originals[0].1);
        drop(sql);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        store.integrity_check().unwrap();
        assert_eq!(store.pending(0, 128).unwrap(), originals);
        assert!(
            store
                .lookup_io_intent("test.io-local", "crash-command")
                .unwrap()
                .is_none()
        );
        assert_eq!(count(&path, "io_intents"), 0);
        assert_eq!(count(&path, "cards"), 1);
    }
}

#[test]
fn reservation_commit_crashes_hold_or_release_the_quota_exactly_once() {
    for (point, committed) in [
        ("io-reservation-after-insert", false),
        ("io-reservation-before-commit", false),
        ("io-reservation-after-commit", true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic.db");
        let mut store = Store::open(&path, Default::default()).unwrap();
        let first = prepared();
        store
            .append_io_intent_local_authorized(&first, || Ok(()))
            .unwrap();
        drop(store);
        crash(&path, "reserve", point);
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        store.integrity_check().unwrap();
        let expected_usage = if committed {
            (1, MAX_CONTAINER_BYTES as u64)
        } else {
            (0, 0)
        };
        assert_eq!(
            store.io_intent_reservation_usage().unwrap(),
            expected_usage,
            "{point}"
        );
        assert_eq!(count(&path, "io_reservations"), i64::from(committed));
        let candidate = first.propose_dispatch_boundary().unwrap();
        if committed {
            // The surviving promise still guards exactly the boundary.
            store
                .append_io_intent_local_authorized(&candidate, || Ok(()))
                .unwrap();
        } else {
            // Without a committed reservation the boundary stays blocked until
            // a fresh quota is granted; no partial deduction existed.
            assert!(matches!(
                store.append_io_intent_local_authorized(&candidate, || Ok(())),
                Err(Error::Invalid("IO intent dispatch reservation"))
            ));
            store
                .reserve_io_intent_followup(first.command(), || Ok(()))
                .unwrap();
            store
                .append_io_intent_local_authorized(&candidate, || Ok(()))
                .unwrap();
        }
        // Crossing the boundary does not consume; the observation does.
        assert_eq!(
            store.io_intent_reservation_usage().unwrap(),
            (1, MAX_CONTAINER_BYTES as u64),
            "{point}"
        );
        let observed = candidate
            .propose_observation([16; 32], ObservationSource::Reconciliation)
            .unwrap();
        store
            .append_io_intent_local_authorized(&observed, || Ok(()))
            .unwrap();
        assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
        store.integrity_check().unwrap();
    }
}

#[test]
fn reservation_migration_crashes_keep_complete_old_or_new_schema_and_originals() {
    for (point, expected_version) in [
        ("io-reservation-migration-before-commit", 15),
        ("io-reservation-migration-after-commit", 16),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.db");
        let mut store = Store::open(&path, Default::default()).unwrap();
        let card =
            CardRecord::new("old-card", "test.card", 1, "", b"retained body".to_vec()).unwrap();
        store.create_local("old-create", &card).unwrap();
        let first = prepared();
        store
            .append_io_intent_local_authorized(&first, || Ok(()))
            .unwrap();
        let originals = store.pending(0, 128).unwrap();
        drop(store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("DROP TABLE IF EXISTS outbound_authorities; DROP TABLE IF EXISTS service_authority_identity; DROP TABLE IF EXISTS service_authorities; DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; DROP TABLE io_reservations; PRAGMA user_version=15;")
            .unwrap();
        drop(sql);
        crash(&path, "migrate", point);
        let sql = rusqlite::Connection::open(&path).unwrap();
        let actual: i64 = sql
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(actual, expected_version, "{point}");
        let tables: i64 = sql
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='io_reservations'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, i64::from(expected_version == 16), "{point}");
        let old: Vec<u8> = sql
            .query_row(
                "SELECT payload FROM operations WHERE id='old-create'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old, originals[0].1, "{point}");
        drop(sql);
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        store.integrity_check().unwrap();
        assert_eq!(store.pending(0, 128).unwrap(), originals);
        // A migrated non-terminal intent can reserve and complete its lifecycle.
        store
            .reserve_io_intent_followup(first.command(), || Ok(()))
            .unwrap();
        let candidate = first.propose_dispatch_boundary().unwrap();
        store
            .append_io_intent_local_authorized(&candidate, || Ok(()))
            .unwrap();
        assert_eq!(
            store.io_intent_reservation_usage().unwrap(),
            (1, MAX_CONTAINER_BYTES as u64)
        );
        store.integrity_check().unwrap();
    }
}
