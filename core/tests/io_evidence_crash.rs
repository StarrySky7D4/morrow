//! Real SQLite process-exit boundaries for protected IO material only; no
//! backend dispatch, remote effect, response inspection or recovered grant is
//! exercised here. A stored original is historical bytes, never a permit.
#![cfg(all(feature = "fault-injection", not(target_arch = "wasm32")))]
use morrow_core::{
    content::CardRecord,
    io_evidence::{self, Kind, Material},
    io_intent::{Command, Record},
    plugin_package::io,
    store::Store,
};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    process::{Command as Process, Stdio},
    time::{Duration, Instant},
};

const SUBJECT: &str = "plugin.io-test";
const OPERATION: &str = "material-command";

fn request_payload() -> Vec<u8> {
    (0..27u32).map(|i| i as u8).collect()
}
fn request_digest() -> [u8; 32] {
    Sha256::digest(request_payload()).into()
}
fn command() -> Command {
    Command {
        operation_id: OPERATION.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        capability: io::IoCapability::HttpRequest,
        protocol_sha256: io::schema_digest(),
        request_sha256: request_digest(),
        approval_sha256: [3; 32],
        target_sha256: [4; 32],
        request_bytes: request_payload().len() as u64,
        response_limit: 4096,
    }
}
fn prepared() -> Record {
    Record::prepared(command()).unwrap()
}
fn material(kind: Kind, payload: &[u8]) -> Material {
    Material::encode(kind, OPERATION, SUBJECT, request_digest(), payload).unwrap()
}
fn request_material() -> Material {
    material(Kind::Request, &request_payload())
}
fn response_material() -> Material {
    material(Kind::Response, b"protected-response")
}
fn planned() -> (u64, u64) {
    (
        io_evidence::max_container_bytes(command().request_bytes).unwrap(),
        io_evidence::max_container_bytes(command().response_limit).unwrap(),
    )
}

#[test]
#[ignore = "child harness; parent passes an isolated database and crash point"]
fn io_evidence_crash_child() {
    let path = std::env::var_os("MORROW_IO_EVIDENCE_DB").expect("child database");
    let mut store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
    match std::env::var("MORROW_IO_EVIDENCE_MODE")
        .unwrap()
        .as_str()
    {
        "reserve" => {
            store
                .reserve_io_materials(&command(), || Ok(()))
                .unwrap();
        }
        "store-request" => {
            store
                .reserve_io_materials(&command(), || Ok(()))
                .unwrap();
            store
                .store_io_material(SUBJECT, Kind::Request, &request_material(), || Ok(()))
                .unwrap();
        }
        "store-response" => {
            store
                .store_io_material(SUBJECT, Kind::Response, &response_material(), || Ok(()))
                .unwrap();
        }
        "migrate" => {}
        other => panic!("unexpected child mode {other}"),
    }
    panic!("fault injection boundary was not reached");
}

fn crash(path: &Path, mode: &str, point: &str) {
    let mut process = Process::new(std::env::current_exe().unwrap());
    process
        .args([
            "--exact",
            "io_evidence_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env("MORROW_IO_EVIDENCE_DB", path)
        .env("MORROW_IO_EVIDENCE_MODE", mode)
        .env("MORROW_TEST_CRASH_AT", point)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
    let mut child = process.spawn().unwrap();
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

fn count(path: &Path, table: &str) -> i64 {
    assert!(
        [
            "io_evidence",
            "io_material_reservations",
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

fn version(path: &Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn seed_prepared(path: &Path) {
    let mut store = Store::open(path, Default::default()).unwrap();
    store
        .append_io_intent_local_authorized(&prepared(), || Ok(()))
        .unwrap();
}

fn seed_request(path: &Path) {
    let mut store = Store::open_existing(path, Default::default()).unwrap();
    store
        .reserve_io_materials(&command(), || Ok(()))
        .unwrap();
    store
        .store_io_material(SUBJECT, Kind::Request, &request_material(), || Ok(()))
        .unwrap();
    store
        .reserve_io_intent_followup(&command(), || Ok(()))
        .unwrap();
    let unknown = prepared().propose_dispatch_boundary().unwrap();
    store
        .append_io_intent_local_authorized(&unknown, || Ok(()))
        .unwrap();
}

#[test]
fn material_reservation_commit_crashes_hold_or_release_exactly_once() {
    let (request_bytes, response_bytes) = planned();
    for (point, committed) in [
        ("io-material-reservation-after-insert", false),
        ("io-material-reservation-before-commit", false),
        ("io-material-reservation-after-commit", true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reserve.db");
        seed_prepared(&path);
        crash(&path, "reserve", point);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        store.integrity_check().unwrap();
        let expected = if committed {
            (2, request_bytes + response_bytes)
        } else {
            (0, 0)
        };
        assert_eq!(
            store.io_material_reservation_usage().unwrap(),
            expected,
            "{point}"
        );
        assert_eq!(count(&path, "io_material_reservations"), i64::from(committed) * 2);
        assert_eq!(count(&path, "io_evidence"), 0);
        assert_eq!(count(&path, "operations"), 1);
        assert_eq!(count(&path, "io_intents"), 1);
        assert_eq!(count(&path, "outbox"), 1);
    }
}

#[test]
fn request_material_commit_crashes_keep_reservation_or_original_exactly_once() {
    let (request_bytes, response_bytes) = planned();
    for (point, committed) in [
        ("io-material-after-evidence", false),
        ("io-material-before-commit", false),
        ("io-material-after-commit", true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("request.db");
        seed_prepared(&path);
        crash(&path, "store-request", point);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        store.integrity_check().unwrap();
        if committed {
            // The consumed reservation leaves exactly the response slot held and
            // the request original durable; no audit event was fabricated.
            assert_eq!(
                store.io_material_reservation_usage().unwrap(),
                (1, response_bytes),
                "{point}"
            );
            assert_eq!(count(&path, "io_evidence"), 1);
            let stored = store
                .io_material(SUBJECT, OPERATION, Kind::Request)
                .unwrap()
                .unwrap();
            assert_eq!(stored.payload(), request_payload().as_slice());
            assert_eq!(
                store
                    .io_material(SUBJECT, OPERATION, Kind::Response)
                    .unwrap_err(),
                morrow_core::Error::EvidenceUnavailable
            );
        } else {
            // The unconsumed pair still guards the exact boundary; no partial
            // original was admitted.
            assert_eq!(
                store.io_material_reservation_usage().unwrap(),
                (2, request_bytes + response_bytes),
                "{point}"
            );
            assert_eq!(count(&path, "io_evidence"), 0);
            assert_eq!(
                store
                    .io_material(SUBJECT, OPERATION, Kind::Request)
                    .unwrap_err(),
                morrow_core::Error::EvidenceUnavailable
            );
        }
        assert_eq!(count(&path, "operations"), 1);
        assert_eq!(count(&path, "io_intents"), 1);
        assert_eq!(count(&path, "outbox"), 1);
        assert_eq!(count(&path, "cards"), 0);
        assert_eq!(count(&path, "records"), 0);
    }
}

#[test]
fn response_material_commit_crashes_keep_exactly_one_slot_state() {
    let (_request_bytes, response_bytes) = planned();
    for (point, committed) in [
        ("io-material-after-evidence", false),
        ("io-material-before-commit", false),
        ("io-material-after-commit", true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("response.db");
        seed_prepared(&path);
        seed_request(&path);
        crash(&path, "store-response", point);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        store.integrity_check().unwrap();
        if committed {
            assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
            assert_eq!(count(&path, "io_evidence"), 2);
            let stored = store
                .io_material(SUBJECT, OPERATION, Kind::Response)
                .unwrap()
                .unwrap();
            assert_eq!(stored.payload(), b"protected-response");
        } else {
            // The response reservation survives the crash so the observation can
            // still be recorded, while the durable request original is unchanged.
            assert_eq!(
                store.io_material_reservation_usage().unwrap(),
                (1, response_bytes),
                "{point}"
            );
            assert_eq!(count(&path, "io_evidence"), 1);
            assert_eq!(
                store
                    .io_material(SUBJECT, OPERATION, Kind::Response)
                    .unwrap_err(),
                morrow_core::Error::EvidenceUnavailable
            );
        }
        // The dispatch boundary was already committed before this crash point.
        assert_eq!(count(&path, "io_intents"), 2);
        assert_eq!(count(&path, "operations"), 2);
        assert_eq!(count(&path, "io_reservations"), 1);
    }
}

#[test]
fn material_migration_crashes_keep_complete_old_or_new_schema_and_originals() {
    for (point, expected_version) in [
        ("io-evidence-migration-before-commit", 16),
        ("io-evidence-migration-after-commit", 17),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.db");
        let mut store = Store::open(&path, Default::default()).unwrap();
        let card =
            CardRecord::new("old-card", "test.card", 1, "", b"retained body".to_vec()).unwrap();
        store.create_local("old-create", &card).unwrap();
        store
            .append_io_intent_local_authorized(&prepared(), || Ok(()))
            .unwrap();
        let originals = store.pending(0, 128).unwrap();
        drop(store);
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(
                "DROP TABLE IF EXISTS service_authority_identity; DROP TABLE IF EXISTS service_authorities; DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; PRAGMA user_version=16;",
            )
            .unwrap();
        crash(&path, "migrate", point);
        assert_eq!(version(&path), expected_version, "{point}");
        let tables: i64 = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name IN ('io_evidence','io_evidence_kind','io_material_reservations')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 3 * i64::from(expected_version >= 17), "{point}");
        let old: Vec<u8> = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT payload FROM operations WHERE id='old-create'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old, originals[0].1, "{point}");
        let store = Store::open_existing(&path, Default::default()).unwrap();
        store.integrity_check().unwrap();
        assert_eq!(store.pending(0, 128).unwrap(), originals);
        assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    }
}
