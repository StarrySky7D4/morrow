//! Service admission is one transaction; no request is dispatched by these tests.
#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    io_evidence::{self, Kind, Material},
    io_intent::{Phase, Record},
    plugin_package::io::{self, IoCapability},
    service::{Invocation, Request},
    service_record::{Policy, RequestRecord},
    store::{EventBudget, Store},
};
use std::path::Path;
fn sample() -> (Record, Material) {
    let policy = Policy {
        namespace: [7; 32],
        retention_ms: 1000,
    };
    let request = Request::encode(
        9,
        &Invocation {
            service: "service.example".into(),
            handler: "api.invoke".into(),
            principal: "alice".into(),
            method: "POST".into(),
            target: "/items".into(),
            headers: vec![],
            body: vec![3; 128],
        },
    )
    .unwrap();
    let request = RequestRecord::encode(&policy, "atomic-key", &request, 10).unwrap();
    let prepared = Record::prepared(request.command([4; 32]).unwrap()).unwrap();
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
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
fn empty(store: &Store, path: &Path, prepared: &Record) {
    assert!(
        store
            .lookup_io_intent(
                &prepared.command().subject,
                &prepared.command().operation_id
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    assert_eq!(store.io_intent_reservation_usage().unwrap(), (0, 0));
    assert_eq!(store.io_material_reservation_usage().unwrap(), (0, 0));
    assert_eq!(counts(path), vec![0; 7]);
    store.integrity_check().unwrap();
}
fn complete(store: &Store, path: &Path, prepared: &Record, material: &Material) {
    let command = prepared.command();
    let actual = store
        .lookup_io_intent(&command.subject, &command.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(actual.container(), prepared.container());
    assert_eq!(actual.phase(), Phase::Prepared);
    let stored = store
        .io_material(&command.subject, &command.operation_id, Kind::Request)
        .unwrap()
        .unwrap();
    assert_eq!(stored.container(), material.container());
    assert_eq!(stored.payload(), material.payload());
    assert_eq!(store.io_intent_reservation_usage().unwrap().0, 1);
    assert_eq!(
        store.io_material_reservation_usage().unwrap(),
        (
            1,
            io_evidence::max_container_bytes(command.response_limit).unwrap()
        )
    );
    assert_eq!(counts(path), vec![1, 1, 1, 1, 1, 1, 1]);
    store.integrity_check().unwrap();
}
#[test]
fn complete_preparation_reopens_and_exact_retry_does_not_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atomic.db");
    let (prepared, material) = sample();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let mut checks = 0;
    let result = store
        .prepare_service_request_local_authorized(&prepared, &material, || {
            checks += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(checks, 5);
    assert_eq!(result.container(), prepared.container());
    complete(&store, &path, &prepared, &material);
    let usage = store.pending_usage().unwrap();
    drop(store);
    let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
    checks = 0;
    store
        .prepare_service_request_local_authorized(&prepared, &material, || {
            checks += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(checks, 5);
    assert_eq!(store.pending_usage().unwrap(), usage);
    complete(&store, &path, &prepared, &material);
}
#[test]
fn each_authorization_failure_and_final_guard_roll_back_all_four_writes() {
    for denied in 1..=5 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("denied.db");
        let (prepared, material) = sample();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let mut checks = 0;
        let result = store.prepare_service_request_local_authorized(&prepared, &material, || {
            checks += 1;
            if checks == denied {
                Err(Error::Invalid("test denial"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert_eq!(checks, denied);
        empty(&store, &path, &prepared);
        // Same connection remains usable; no stranded transaction/reservation.
        store
            .prepare_service_request_local_authorized(&prepared, &material, || Ok(()))
            .unwrap();
        complete(&store, &path, &prepared, &material);
    }
}
#[test]
fn unwinding_authorizer_rolls_back_and_releases_writer() {
    for panic_at in [2, 5] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("panic.db");
        let (prepared, material) = sample();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let mut checks = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.prepare_service_request_local_authorized(&prepared, &material, || {
                checks += 1;
                assert_ne!(checks, panic_at, "synthetic authorizer panic");
                Ok(())
            })
        }));
        assert!(result.is_err());
        empty(&store, &path, &prepared);
        store
            .prepare_service_request_local_authorized(&prepared, &material, || Ok(()))
            .unwrap();
    }
}
#[test]
fn event_and_material_capacity_failure_leave_no_prepared_remnant() {
    let (prepared, material) = sample();
    let insufficient_material = prepared.container().len() as u64
        + morrow_core::io_intent::MAX_CONTAINER_BYTES as u64
        + io_evidence::max_container_bytes(prepared.command().request_bytes).unwrap()
        + io_evidence::max_container_bytes(prepared.command().response_limit).unwrap()
        - 1;
    for budget in [
        EventBudget {
            max_count: 1,
            max_bytes: 64 * 1024 * 1024,
        },
        EventBudget {
            max_count: 2,
            max_bytes: 64 * 1024 * 1024,
        },
        EventBudget {
            max_count: 10,
            max_bytes: prepared.container().len() as u64,
        },
        EventBudget {
            max_count: 10,
            max_bytes: insufficient_material,
        },
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capacity.db");
        let mut store = Store::open(&path, budget).unwrap();
        assert!(matches!(
            store.prepare_service_request_local_authorized(&prepared, &material, || Ok(())),
            Err(Error::EventCapacity)
        ));
        empty(&store, &path, &prepared);
    }
}
#[test]
fn invalid_metadata_and_non_service_originals_never_create_history() {
    let (original, material) = sample();
    for change in 0..8 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.db");
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        let mut command = original.command().clone();
        match change {
            0 => {
                command.capability = IoCapability::HttpRequest;
                command.protocol_sha256 = io::schema_digest();
            }
            1 => command.protocol_sha256 = io::schema_digest(),
            2 => command.subject = "other-subject".into(),
            3 => command.operation_id = "other-operation".into(),
            4 => command.response_limit += 1,
            5 => command.approval_sha256 = [9; 32],
            6 => command.target_sha256 = [9; 32],
            _ => {}
        };
        let prepared = Record::prepared(command).unwrap();
        let wrong = Material::encode(
            Kind::Response,
            material.operation_id(),
            material.subject(),
            material.request_sha256(),
            material.payload(),
        )
        .unwrap();
        let provided = if change == 7 { &wrong } else { &material };
        assert!(
            store
                .prepare_service_request_local_authorized(&prepared, provided, || Ok(()))
                .is_err()
        );
        empty(&store, &path, &prepared);
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("garbage.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    use sha2::{Digest, Sha256};
    let mut command = original.command().clone();
    command.request_sha256 = Sha256::digest(b"invalid-record").into();
    command.request_bytes = 14;
    let wrong = Material::encode(
        Kind::Request,
        &command.operation_id,
        &command.subject,
        command.request_sha256,
        b"invalid-record",
    )
    .unwrap();
    let prepared = Record::prepared(command).unwrap();
    assert!(
        store
            .prepare_service_request_local_authorized(&prepared, &wrong, || Ok(()))
            .is_err()
    );
    empty(&store, &path, &prepared);
}
#[test]
fn independent_writes_keep_behavior_and_claim_remains_single_winner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("independent.db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let (prepared, material) = sample();
    let command = prepared.command();
    store
        .append_io_intent_local_authorized(&prepared, || Ok(()))
        .unwrap();
    store
        .reserve_io_intent_followup(command, || Ok(()))
        .unwrap();
    store.reserve_io_materials(command, || Ok(())).unwrap();
    store
        .store_io_material(&command.subject, Kind::Request, &material, || Ok(()))
        .unwrap();
    complete(&store, &path, &prepared, &material);
    let unknown = prepared.propose_dispatch_boundary().unwrap();
    store
        .claim_io_dispatch_local_authorized(&unknown, || Ok(()))
        .unwrap();
    assert!(matches!(
        store.claim_io_dispatch_local_authorized(&unknown, || Ok(())),
        Err(Error::RevisionConflict)
    ));
    assert_eq!(
        store
            .append_io_intent_local_authorized(&unknown, || Ok(()))
            .unwrap()
            .container(),
        unknown.container()
    );
    let before = counts(&path);
    assert!(
        store
            .prepare_service_request_local_authorized(&prepared, &material, || Ok(()))
            .is_err()
    );
    assert_eq!(counts(&path), before);
}
#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "child harness; parent supplies isolated database and fault boundary"]
fn service_request_atomic_crash_child() {
    let path = std::env::var_os("MORROW_SERVICE_ATOMIC_DB").unwrap();
    let mut store = Store::open_existing(Path::new(&path), EventBudget::default()).unwrap();
    let (prepared, material) = sample();
    store
        .prepare_service_request_local_authorized(&prepared, &material, || Ok(()))
        .unwrap();
    panic!("crash boundary not reached");
}
#[cfg(feature = "fault-injection")]
fn crash(path: &Path, point: &str) {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "service_request_atomic_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env("MORROW_SERVICE_ATOMIC_DB", path)
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
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(86), "{point}");
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child timeout: {point}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[cfg(feature = "fault-injection")]
#[test]
fn real_process_crash_before_commit_is_empty_after_commit_is_complete() {
    for point in [
        "io-intent-after-event",
        "io-material-after-evidence",
        "service-request-before-commit",
        "service-request-after-commit",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.db");
        let store = Store::open(&path, EventBudget::default()).unwrap();
        drop(store);
        crash(&path, point);
        let store = Store::open_existing(&path, EventBudget::default()).unwrap();
        let (prepared, material) = sample();
        if point == "service-request-after-commit" {
            complete(&store, &path, &prepared, &material);
        } else {
            empty(&store, &path, &prepared);
        }
    }
}
