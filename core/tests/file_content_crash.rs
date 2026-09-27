//! Real process crashes around content admission and v21 -> v22 migration.
#![cfg(all(feature = "fault-injection", not(target_arch = "wasm32")))]
use morrow_core::{
    Error,
    file_content::FileContent,
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    io_evidence::{Kind, Material},
    io_intent::Record,
    store::{EventBudget, Store},
};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const SUBJECT: &str = "content-test";
const OPERATION: &str = "content-crash";
const DATABASE_ENV: &str = "MORROW_FILE_CONTENT_CRASH_DB";
fn sample() -> (Record, Material, FileContent) {
    let bytes = vec![0x74; 128 * 1024];
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
        content_length: bytes.len() as u64,
        content_sha256: Some(Sha256::digest(&bytes).into()),
    })
    .unwrap();
    let record = Record::prepared(request.command().unwrap()).unwrap();
    let material = Material::encode(
        Kind::Request,
        OPERATION,
        SUBJECT,
        record.command().request_sha256,
        request.container(),
    )
    .unwrap();
    let content =
        FileContent::new(OPERATION, SUBJECT, record.command().request_sha256, &bytes).unwrap();
    (record, material, content)
}
#[test]
#[ignore = "child harness; parent supplies isolated DB and crash boundary"]
fn file_content_crash_child() {
    let path = std::env::var_os(DATABASE_ENV).unwrap();
    let mut store = Store::open_existing(Path::new(&path), EventBudget::default()).unwrap();
    store
        .stage_file_mutation_content_local_authorized(&sample().2, || Ok(()))
        .unwrap();
    panic!("fault boundary not reached");
}
fn crash(path: &Path, point: &str) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "file_content_crash_child",
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
fn content_admission_and_schema_migration_survive_real_process_exit() {
    for point in [
        "file-content-after-bytes",
        "file-content-after-receipt",
        "file-content-before-commit",
        "file-content-after-commit",
        "file-content-migration-before-commit",
        "file-content-migration-after-commit",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("content.db");
        let (record, material, content) = sample();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        store
            .prepare_file_mutation_local_authorized(&record, &material, || Ok(()))
            .unwrap();
        let pending = store.pending(0, 10).unwrap();
        drop(store);
        if point.contains("migration") {
            rusqlite::Connection::open(&path)
                .unwrap()
                .execute_batch("DROP TABLE IF EXISTS file_content_receipts; DROP TABLE file_mutation_content; PRAGMA user_version=21;")
                .unwrap();
        }
        crash(&path, point);
        let sql = rusqlite::Connection::open(&path).unwrap();
        let version: i64 = sql
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            version,
            if point == "file-content-migration-before-commit" {
                21
            } else if point == "file-content-migration-after-commit" {
                22
            } else {
                morrow_core::store::SCHEMA_VERSION
            }
        );
        drop(sql);
        let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
        let after = store.pending(0, 10).unwrap();
        if point == "file-content-after-commit" {
            assert_eq!(&after[..pending.len()], pending.as_slice());
            assert_eq!(after.len(), pending.len() + 1);
            let receipt =
                morrow_core::file_content_receipt::Receipt::decode(&after.last().unwrap().1)
                    .unwrap();
            assert_eq!(
                receipt.source(),
                morrow_core::file_content_receipt::Source::LiveStaging
            );
        } else {
            assert_eq!(after, pending);
        }

        assert_eq!(
            store
                .lookup_io_intent(SUBJECT, OPERATION)
                .unwrap()
                .unwrap()
                .container(),
            record.container()
        );
        let received = store
            .file_mutation_content_local_authorized(SUBJECT, OPERATION, || Ok(()))
            .unwrap();
        let admitted = point == "file-content-after-commit";
        assert_eq!(received.is_some(), admitted);
        assert_eq!(
            store.file_mutation_content_usage().unwrap().0,
            u64::from(admitted)
        );
        if let Some(received) = received {
            assert_eq!(received.container(), content.container());
        }
        store
            .stage_file_mutation_content_local_authorized(&content, || Ok(()))
            .unwrap();
        let usage = store.file_mutation_content_usage().unwrap();
        store
            .stage_file_mutation_content_local_authorized(&content, || Ok(()))
            .unwrap();
        assert_eq!(store.file_mutation_content_usage().unwrap(), usage);
        assert!(matches!(
            store.claim_io_dispatch_local_authorized(
                &record.propose_dispatch_boundary().unwrap(),
                || Ok(())
            ),
            Err(Error::UnsupportedVersion)
        ));
        store.integrity_check().unwrap();
    }
}
