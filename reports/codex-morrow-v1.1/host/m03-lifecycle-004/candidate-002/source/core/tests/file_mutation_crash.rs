//! Real process-exit checks for atomic file mutation preparation. No file effect
//! or dispatch occurs; only the persistent Prepared history is exercised.
#![cfg(all(feature = "fault-injection", not(target_arch = "wasm32")))]

use morrow_core::{
    Error,
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    io_evidence::{self, Kind, Material},
    io_intent::{Phase, Record, Recovery},
    store::{EventBudget, Store},
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const SUBJECT: &str = "plugin.file-crash";
const OPERATION: &str = "file-mutation-crash";
const DATABASE_ENV: &str = "MORROW_FILE_MUTATION_CRASH_DB";

fn sample() -> (Record, Material) {
    let request = RequestRecord::new(MutationRequest {
        operation_id: OPERATION.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: None,
        },
        disposition: Disposition::Replace,
        expected_identity: Some([4; 32]),
        content_length: 128,
        content_sha256: Some([5; 32]),
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
    (prepared, material)
}

fn counts(path: &Path) -> Vec<i64> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    [
        "io_intents",
        "io_reservations",
        "io_material_reservations",
        "io_evidence",
        "operations",
        "operation_events",
        "outbox",
    ]
    .into_iter()
    .map(|table| {
        connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    })
    .collect()
}

fn assert_empty(store: &Store, path: &Path) {
    assert!(
        store
            .lookup_io_intent(SUBJECT, OPERATION)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .io_material(SUBJECT, OPERATION, Kind::Request)
            .unwrap()
            .is_none()
    );
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    assert_eq!(counts(path), vec![0; 7]);
    store.integrity_check().unwrap();
}

fn assert_complete(store: &Store, path: &Path, prepared: &Record, material: &Material) {
    let command = prepared.command();
    let stored = store.lookup_io_intent(SUBJECT, OPERATION).unwrap().unwrap();
    assert_eq!(stored.phase(), Phase::Prepared);
    assert_eq!(stored.recovery(), Recovery::AwaitFreshAuthorization);
    assert_eq!(stored.container(), prepared.container());
    let original = store
        .io_material(SUBJECT, OPERATION, Kind::Request)
        .unwrap()
        .unwrap();
    assert_eq!(original.container(), material.container());
    assert_eq!(original.payload(), material.payload());
    assert_eq!(store.io_intent_reservation_usage().unwrap().0, 1);
    assert_eq!(
        store.io_material_reservation_usage().unwrap(),
        (
            1,
            io_evidence::max_container_bytes(command.response_limit).unwrap()
        )
    );
    assert_eq!(counts(path), vec![1; 7]);
    store.integrity_check().unwrap();
}

#[test]
#[ignore = "child harness; parent supplies an isolated database and crash point"]
fn file_mutation_crash_child() {
    let path = std::env::var_os(DATABASE_ENV).expect("child database path");
    let mut store = Store::open_existing(Path::new(&path), EventBudget::default()).unwrap();
    let (prepared, material) = sample();
    store
        .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    panic!("file mutation crash boundary was not reached");
}

fn crash(path: &Path, point: &str) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "file_mutation_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env(DATABASE_ENV, path)
        .env("MORROW_TEST_CRASH_AT", point)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
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
            panic!("child timeout: {point}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn process_exit_before_commit_rolls_back_after_commit_retains_exact_prepared() {
    for point in ["file-mutation-before-commit", "file-mutation-after-commit"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file-mutation-crash.db");
        drop(Store::open(&path, EventBudget::default()).unwrap());
        crash(&path, point);
        let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
        let (prepared, material) = sample();
        if point == "file-mutation-before-commit" {
            assert_empty(&store, &path);
            continue;
        }

        assert_complete(&store, &path, &prepared, &material);
        let usage = store.pending_usage().unwrap();
        let intent_reserve = store.io_intent_reservation_usage().unwrap();
        let material_reserve = store.io_material_reservation_usage().unwrap();
        let rows = counts(&path);
        let duplicate = store
            .prepare_file_mutation_local_authorized(&prepared, &material, || Ok(()))
            .unwrap();
        assert_eq!(duplicate.container(), prepared.container());
        assert_eq!(store.pending_usage().unwrap(), usage);
        assert_eq!(store.io_intent_reservation_usage().unwrap(), intent_reserve);
        assert_eq!(
            store.io_material_reservation_usage().unwrap(),
            material_reserve
        );
        assert_eq!(counts(&path), rows);

        let attempted = prepared.propose_dispatch_boundary().unwrap();
        let mut authorized = false;
        assert!(matches!(
            store.claim_io_dispatch_local_authorized(&attempted, || {
                authorized = true;
                Ok(())
            }),
            Err(Error::UnsupportedVersion)
        ));
        assert!(!authorized);
        assert_complete(&store, &path, &prepared, &material);
    }
}
